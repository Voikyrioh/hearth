//! Les sauvegardes d'un service depuis le client (HRT-98, HRT-97, ADR-0047, `BR-BACKUP-*`) : la liste, la
//! consultation de la fenêtre de création, la création, la vérification, la suppression et les éléments ignorés.
//! Chemins construits ici : la clé du service est vérifiée puis encodée par le code, l'identifiant d'une sauvegarde
//! ne porte que des lettres et des chiffres ; jamais un chemin fourni par l'appelant.
//!
//! - **Lister**, **consulter** (`POST …/backups/preview`) et **vérifier** (`POST /backups/{id}/verify`) sont des
//!   LECTURES : rien n'est journalisé côté agent. L'estimation de la taille dure 60 secondes au plus
//!   (`BR-BACKUP-009`) : le délai de la consultation est allongé de cette durée.
//! - **Créer** (`BackupCreate`, `0x19`, couvert par l'élévation), **supprimer** (`BackupDelete`, `0x1B`, JAMAIS
//!   couvert) et **régler les éléments ignorés** (`BackupSettings`, `0x1C`) sont des ACTES d'administration
//!   confirmés. L'acte signé est reconstruit depuis la requête par `domain/act.rs` : la clé, le mode, le nombre
//!   et l'empreinte des éléments ; jamais la note ni un chemin. Jamais rejoués : un lien tombé avant la réponse
//!   donne `ResultUnknown`.

use std::time::Duration;

use serde::de::DeserializeOwned;

use hearth_proto::admin_act::is_service_key;
use hearth_proto::api::backups::{
    BackupPreview, BackupRequest, BackupSettings, BackupsView, ESTIMATE_BUDGET_S, VerifyResult,
};

use super::services::key_segment;
use super::{ActionOutcome, ActionRequest, LinkManager};
use crate::domain::secret::Secret;
use crate::domain::server::ServerId;
use crate::error::{InputField, LinkError};
use crate::ports::transport::Method;

fn parse<T: DeserializeOwned>(body: serde_json::Value, what: &str) -> Result<T, LinkError> {
    serde_json::from_value(body).map_err(|_| LinkError::Protocol(format!("{what} illisible")))
}

/// Le chemin des sauvegardes d'un service, ou `None` si `key` n'a pas la forme d'une clé de service.
pub fn backups_path(key: &str) -> Option<String> {
    is_service_key(key).then(|| format!("/services/{}/backups", key_segment(key)))
}

/// Le chemin de la consultation de la fenêtre de création.
pub fn preview_path(key: &str) -> Option<String> {
    backups_path(key).map(|path| format!("{path}/preview"))
}

/// Le chemin des éléments ignorés d'un service.
pub fn settings_path(key: &str) -> Option<String> {
    is_service_key(key).then(|| format!("/services/{}/backup-settings", key_segment(key)))
}

/// L'identifiant d'une sauvegarde est du texte alphanumérique et court : sinon rien ne part.
fn is_backup_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

/// Le chemin d'une sauvegarde (suppression), ou `None`.
pub fn backup_path(id: &str) -> Option<String> {
    is_backup_id(id).then(|| format!("/backups/{id}"))
}

/// Le chemin de la vérification.
pub fn verify_path(id: &str) -> Option<String> {
    backup_path(id).map(|path| format!("{path}/verify"))
}

impl LinkManager {
    /// La liste des sauvegardes d'un service (`GET /services/{key}/backups`), ouverte à tout compte pour un service
    /// créé par Hearth. Un refus de l'agent (service inconnu ou adopté, moteur, racine) est `Rejected(Some(code))`.
    pub async fn list_backups(&self, id: &ServerId, key: &str) -> Result<BackupsView, LinkError> {
        let path = backups_path(key).ok_or(LinkError::InvalidInput(InputField::Filter))?;
        let (target, token) = self.credentials(id)?;
        let body = self.typed_get(&target, &token, path).await?;
        parse(body, "sauvegardes du service")
    }

    /// De quoi bâtir la fenêtre de création (`POST …/backups/preview`) : une consultation, rien n'est écrit.
    pub async fn backup_preview(
        &self,
        id: &ServerId,
        key: &str,
    ) -> Result<BackupPreview, LinkError> {
        let path = preview_path(key).ok_or(LinkError::InvalidInput(InputField::Filter))?;
        let (target, token) = self.credentials(id)?;
        let body = self
            .typed_post_slow(
                &target,
                &token,
                path,
                serde_json::json!({}),
                Duration::from_secs(ESTIMATE_BUDGET_S),
            )
            .await?;
        parse(body, "estimation de la sauvegarde")
    }

    /// Crée une sauvegarde (`POST …/backups`, acte `BackupCreate`). `password` : le mot de passe de confirmation si
    /// l'agent l'exige (jamais gardé). La réponse de l'agent (`202` ou refus typé) est rendue telle quelle.
    pub async fn create_backup(
        &self,
        id: &ServerId,
        key: &str,
        request: &BackupRequest,
        password: Option<&Secret>,
    ) -> Result<ActionOutcome, LinkError> {
        let path = backups_path(key).ok_or(LinkError::UnreadableAct)?;
        let body = serde_json::to_value(request)
            .map_err(|_| LinkError::Protocol("demande de sauvegarde illisible".into()))?;
        let action = ActionRequest {
            method: Method::Post,
            path,
            body: Some(body),
        };
        self.execute_act(id, action, password).await
    }

    /// Relit l'archive et la juge contre son manifeste (`POST /backups/{id}/verify`) : une consultation.
    pub async fn verify_backup(
        &self,
        id: &ServerId,
        backup: &str,
    ) -> Result<VerifyResult, LinkError> {
        let path = verify_path(backup).ok_or(LinkError::InvalidInput(InputField::Filter))?;
        let (target, token) = self.credentials(id)?;
        let body = self
            .typed_post(&target, &token, path, serde_json::json!({}))
            .await?;
        parse(body, "vérification de la sauvegarde")
    }

    /// Supprime une sauvegarde (`DELETE /backups/{id}`, acte `BackupDelete`) : le mot de passe est exigé à chaque
    /// fois (jamais couvert par l'élévation).
    pub async fn delete_backup(
        &self,
        id: &ServerId,
        backup: &str,
        password: Option<&Secret>,
    ) -> Result<ActionOutcome, LinkError> {
        let path = backup_path(backup).ok_or(LinkError::UnreadableAct)?;
        let action = ActionRequest {
            method: Method::Delete,
            path,
            body: None,
        };
        self.execute_act(id, action, password).await
    }

    /// Enregistre les éléments ignorés d'un service (`PUT …/backup-settings`, acte `BackupSettings`).
    pub async fn save_backup_settings(
        &self,
        id: &ServerId,
        key: &str,
        settings: &BackupSettings,
        password: Option<&Secret>,
    ) -> Result<ActionOutcome, LinkError> {
        let path = settings_path(key).ok_or(LinkError::UnreadableAct)?;
        let body = serde_json::to_value(settings)
            .map_err(|_| LinkError::Protocol("éléments ignorés illisibles".into()))?;
        let action = ActionRequest {
            method: Method::Put,
            path,
            body: Some(body),
        };
        self.execute_act(id, action, password).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::act::{Route, classify};
    use hearth_proto::api::backups::IgnoredItem;

    #[test]
    fn a_service_key_and_a_backup_id_become_fixed_paths_and_anything_else_never_leaves() {
        assert_eq!(
            backups_path("c_0123abcd").as_deref(),
            Some("/services/c_0123abcd/backups")
        );
        assert_eq!(
            preview_path("c_0123abcd").as_deref(),
            Some("/services/c_0123abcd/backups/preview")
        );
        assert_eq!(
            settings_path("c_0123abcd").as_deref(),
            Some("/services/c_0123abcd/backup-settings")
        );
        assert_eq!(
            verify_path("01J9ZY0G3Q8M2K6W4T7V5N1B9D").as_deref(),
            Some("/backups/01J9ZY0G3Q8M2K6W4T7V5N1B9D/verify")
        );
        for bad in ["", "../x", "a/b", "c_ab/../x", "c_ab?x=1"] {
            assert!(backups_path(bad).is_none(), "{bad}");
            assert!(preview_path(bad).is_none(), "{bad}");
            assert!(settings_path(bad).is_none(), "{bad}");
        }
        for bad in ["", "../x", "a/b", "ab?x=1", "a b"] {
            assert!(backup_path(bad).is_none(), "{bad}");
            assert!(verify_path(bad).is_none(), "{bad}");
        }
        assert!(backup_path(&"a".repeat(65)).is_none());
    }

    #[test]
    fn the_signed_acts_are_rebuilt_from_what_the_link_sends() {
        let key = "h_01J9ZY0G3Q8M2K6W4T7V5N1B9D";
        let request = BackupRequest {
            mode: hearth_proto::api::backups::BackupMode::Hot,
            note: Some("secrète".into()),
            ignore_once: vec![IgnoredItem {
                root: "/data".into(),
                path: "gros.log".into(),
            }],
        };
        let body = serde_json::to_value(&request).expect("json");
        let path = backups_path(key).expect("chemin");
        let Ok(Route::Act(act)) = classify("POST", &path, Some(&body)) else {
            panic!("acte attendu");
        };
        assert_eq!(act.target(), key);
        let (count, digest) = request.act_digest();
        assert_eq!(
            act.params(),
            vec![
                b"hot".to_vec(),
                count.to_string().into_bytes(),
                digest.to_vec()
            ]
        );
        let delete_path = backup_path("01J9ZY0G3Q8M2K6W4T7V5N1B9D").expect("chemin");
        let Ok(Route::Act(delete)) = classify("DELETE", &delete_path, None) else {
            panic!("acte attendu");
        };
        assert!(delete.params().is_empty());
        let settings = BackupSettings::default();
        let body = serde_json::to_value(&settings).expect("json");
        let put_path = settings_path(key).expect("chemin");
        let Ok(Route::Act(saved)) = classify("PUT", &put_path, Some(&body)) else {
            panic!("acte attendu");
        };
        assert_eq!(saved.target(), key);
    }
}
