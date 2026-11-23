//! Les mods d'un serveur Minecraft installé depuis le client (HRT-260, ADR-0062, `BR-MOD-020` à `033`) : lire la
//! liste, contrôler des dépôts, appliquer un lot, lire la liste des mods d'une sauvegarde. Chemins construits ici : la
//! clé du service est vérifiée puis encodée, un identifiant de dépôt ou de sauvegarde ne porte que des lettres, des
//! chiffres et des tirets ; jamais un chemin ni un nom de fichier fourni par l'appelant.
//!
//! - **Lire** (`GET …/mods`) et **contrôler** (`POST …/mods/check`) sont des LECTURES : rien n'est écrit ni journalisé
//!   côté agent (le contrôle est une consultation de `READ_ONLY_POSTS`). Le délai est allongé : l'agent lit des jars
//!   pendant 20 secondes au plus.
//! - **Appliquer** (`POST …/mods/apply`) est l'ACTE `ModsApply` (`0x20`), jamais couvert par l'élévation : le mot de
//!   passe est exigé à chaque fois. L'acte signé est reconstruit depuis la requête par `domain/act.rs` (la clé, le
//!   nombre et l'empreinte de la liste des changements ; jamais un nom de fichier). Jamais rejoué.

use std::time::Duration;

use hearth_proto::admin_act::is_service_key;
use hearth_proto::api::mods::{
    BackupModsView, ModsApplyRequest, ModsCheckRequest, ModsCheckView, ModsView,
};

use hearth_proto::api::service_update::OperationTail;

use super::packs::is_deposit_id;
use super::parse::parse;
use super::services::key_segment;
use super::{ActionOutcome, ActionRequest, LinkManager};
use crate::domain::secret::Secret;
use crate::domain::server::ServerId;
use crate::error::{InputField, LinkError};
use crate::ports::transport::Method;

/// Le temps que l'agent se donne pour lire les jars d'une liste ou d'un lot (`BR-MOD-020`, 20 secondes) en plus du
/// délai ordinaire.
const READ_EXTRA: Duration = Duration::from_secs(25);
/// Un lot de contrôle compte 8 dépôts au plus (`BR-MOD-022`).
pub const MAX_CHECK_DEPOSITS: usize = 8;

/// Le chemin de la liste des mods d'un service, ou `None` si `key` n'a pas la forme d'une clé de service.
pub fn mods_path(key: &str) -> Option<String> {
    is_service_key(key).then(|| format!("/services/{}/mods", key_segment(key)))
}

/// Le chemin du contrôle des dépôts.
pub fn check_path(key: &str) -> Option<String> {
    mods_path(key).map(|path| format!("{path}/check"))
}

/// Le chemin de l'application d'un lot.
pub fn apply_path(key: &str) -> Option<String> {
    mods_path(key).map(|path| format!("{path}/apply"))
}

/// L'identifiant d'une sauvegarde : du texte alphanumérique et court.
fn is_backup_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

/// Le chemin de la liste des mods d'une sauvegarde (`compare` : avec la comparaison aux mods actuels).
pub fn backup_mods_path(key: &str, backup: &str, compare: bool) -> Option<String> {
    if !is_backup_id(backup) {
        return None;
    }
    is_service_key(key).then(|| {
        let base = format!("/services/{}/backups/{backup}/mods", key_segment(key));
        if compare {
            format!("{base}?compare=current")
        } else {
            base
        }
    })
}

/// Le chemin des dernières lignes du journal d'une opération (`GET /operations/services/{id}/tail`).
pub fn tail_path(operation: &str) -> Option<String> {
    is_backup_id(operation).then(|| format!("/operations/services/{operation}/tail"))
}

impl LinkManager {
    /// Les dernières lignes du journal d'une opération de service et, quand l'agent la reconnaît, la cause et sa phrase
    /// en clair (HRT-259) : le texte vient du jeu, il ne se journalise jamais.
    pub async fn operation_tail(
        &self,
        id: &ServerId,
        operation: &str,
    ) -> Result<OperationTail, LinkError> {
        let path = tail_path(operation).ok_or(LinkError::InvalidInput(InputField::Filter))?;
        let (target, token) = self.credentials(id)?;
        let body = self.typed_get(&target, &token, path).await?;
        parse(body, "journal de l'opération")
    }

    /// La liste des mods d'un service (`GET /services/{key}/mods`). Un refus de l'agent (service inconnu ou adopté, pas
    /// un jeu) est `Rejected(Some(code))`.
    pub async fn list_mods(&self, id: &ServerId, key: &str) -> Result<ModsView, LinkError> {
        let path = mods_path(key).ok_or(LinkError::InvalidInput(InputField::Filter))?;
        let (target, token) = self.credentials(id)?;
        let body = self
            .typed_get_slow(&target, &token, path, READ_EXTRA)
            .await?;
        parse(body, "mods du service")
    }

    /// Juge des dépôts de la zone d'attente contre les mods du service (`POST …/mods/check`) : une consultation. Aucun
    /// dépôt, plus de huit, ou un identifiant qui n'en est pas un : `InvalidInput`, rien ne part.
    pub async fn check_mods(
        &self,
        id: &ServerId,
        key: &str,
        deposits: &[String],
    ) -> Result<ModsCheckView, LinkError> {
        let path = check_path(key).ok_or(LinkError::InvalidInput(InputField::Filter))?;
        if deposits.is_empty()
            || deposits.len() > MAX_CHECK_DEPOSITS
            || !deposits.iter().all(|deposit| is_deposit_id(deposit))
        {
            return Err(LinkError::InvalidInput(InputField::Deposit));
        }
        let (target, token) = self.credentials(id)?;
        let body = serde_json::to_value(ModsCheckRequest {
            deposits: deposits.to_vec(),
        })
        .map_err(|_| LinkError::Protocol("contrôle de mods illisible".into()))?;
        let answer = self
            .typed_post_slow(&target, &token, path, body, READ_EXTRA)
            .await?;
        parse(answer, "contrôle des mods")
    }

    /// Applique un lot (`POST …/mods/apply`, acte `ModsApply`) : le mot de passe est exigé à chaque fois. La réponse de
    /// l'agent (`202` ou refus typé) est rendue telle quelle.
    pub async fn apply_mods(
        &self,
        id: &ServerId,
        key: &str,
        request: &ModsApplyRequest,
        password: Option<&Secret>,
    ) -> Result<ActionOutcome, LinkError> {
        let path = apply_path(key).ok_or(LinkError::UnreadableAct)?;
        let body = serde_json::to_value(request)
            .map_err(|_| LinkError::Protocol("lot de mods illisible".into()))?;
        let action = ActionRequest {
            method: Method::Post,
            path,
            body: Some(body),
        };
        self.execute_act(id, action, password).await
    }

    /// La liste des mods d'une sauvegarde (`GET /services/{key}/backups/{id}/mods`), avec ou sans la comparaison aux
    /// mods actuels. Une lecture : ouverte à qui voit les sauvegardes du service.
    pub async fn backup_mods(
        &self,
        id: &ServerId,
        key: &str,
        backup: &str,
        compare: bool,
    ) -> Result<BackupModsView, LinkError> {
        let path = backup_mods_path(key, backup, compare)
            .ok_or(LinkError::InvalidInput(InputField::Filter))?;
        let (target, token) = self.credentials(id)?;
        let body = self
            .typed_get_slow(&target, &token, path, READ_EXTRA)
            .await?;
        parse(body, "mods de la sauvegarde")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::act::{Route, classify};
    use hearth_proto::api::mods::ModAdd;

    #[test]
    fn a_service_key_and_a_backup_id_become_fixed_paths_and_anything_else_never_leaves() {
        assert_eq!(
            mods_path("c_0123abcd").as_deref(),
            Some("/services/c_0123abcd/mods")
        );
        assert_eq!(
            check_path("c_0123abcd").as_deref(),
            Some("/services/c_0123abcd/mods/check")
        );
        assert_eq!(
            apply_path("c_0123abcd").as_deref(),
            Some("/services/c_0123abcd/mods/apply")
        );
        assert_eq!(
            backup_mods_path("c_0123abcd", "01J9ZY0G3Q8M2K6W4T7V5N1B9D", true).as_deref(),
            Some("/services/c_0123abcd/backups/01J9ZY0G3Q8M2K6W4T7V5N1B9D/mods?compare=current")
        );
        assert_eq!(
            backup_mods_path("c_0123abcd", "01J9", false).as_deref(),
            Some("/services/c_0123abcd/backups/01J9/mods")
        );
        for bad in ["", "../x", "a/b", "c_ab/../x", "c_ab?x=1"] {
            assert!(mods_path(bad).is_none(), "{bad}");
            assert!(apply_path(bad).is_none(), "{bad}");
        }
        assert_eq!(
            tail_path("01J9ZY").as_deref(),
            Some("/operations/services/01J9ZY/tail")
        );
        assert!(tail_path("../x").is_none());
        for bad in ["", "../x", "a/b", "ab?x=1", "a b"] {
            assert!(
                backup_mods_path("c_0123abcd", bad, false).is_none(),
                "{bad}"
            );
        }
    }

    #[test]
    fn the_signed_act_is_rebuilt_from_what_the_link_sends() {
        let key = "h_01J9ZY0G3Q8M2K6W4T7V5N1B9D";
        let request = ModsApplyRequest {
            add: vec![ModAdd {
                deposit: "01HZY3F8M2K9Q4V6T0R7N5X2PA".into(),
                sha256: "a".repeat(64),
                replace: None,
            }],
            disable: vec!["old.jar".into()],
            ..ModsApplyRequest::default()
        };
        let body = serde_json::to_value(&request).expect("json");
        let path = apply_path(key).expect("chemin");
        let Ok(Route::Act(act)) = classify("POST", &path, Some(&body)) else {
            panic!("acte attendu");
        };
        assert_eq!(act.target(), key);
        let expected = hearth_proto::admin_act::AdminAct::mods_apply(key.into(), &request);
        assert_eq!(act.params(), expected.params());
    }
}
