//! Les réglages d'un serveur de jeu depuis le client (HRT-109, ADR-0051, `BR-FICHE-037`, `040` à `049`, `051` à `054`) : `GET
//! /services/{key}/settings` (la vue), `POST /services/{key}/settings/analyze` (la lecture d'un texte collé) et
//! `PUT /services/{key}/settings` (l'acte `GameSettingsSave` `0x1E`). Chemins construits ici : la clé du service est
//! vérifiée puis encodée par le code, jamais fournie comme chemin.
//!
//! - **Lire** et **analyser** sont des LECTURES (l'analyse est un `POST` de `READ_ONLY_POSTS` : ni acte, ni
//!   journal). Le texte collé peut porter un mot de passe : il part dans le corps, n'est ni gardé ni tracé, et un
//!   lien tombé fait échouer l'appel (l'interface le refait).
//! - **Enregistrer** est un ACTE d'administration confirmé, couvert par l'élévation. L'acte signé est reconstruit
//!   depuis le corps par `domain/act.rs` (`AdminAct::game_settings_save`, source unique partagée avec l'agent) : ni
//!   valeur ni texte collé dans la signature. Jamais rejoué : un lien tombé avant la réponse donne
//!   `ResultUnknown`, et l'interface relit la vue (le fichier dit ce qui s'est passé).

use hearth_proto::admin_act::is_service_key;
use hearth_proto::api::game_settings::{
    AnalyzeRequest, ImportView, SaveSettingsRequest, SettingsView,
};
use hearth_proto::error::{ErrorBody, ErrorCode};
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::audit::rejection_at;
use super::services::key_segment;
use super::{ActionOutcome, ActionRequest, LinkManager};
use crate::domain::secret::Secret;
use crate::domain::server::ServerId;
use crate::error::{InputField, LinkError};
use crate::ports::transport::{ApiRequest, Method};

/// La lecture d'un texte collé : lue par le format du jeu, ou illisible (rien n'est écrit dans les deux cas).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Analysis {
    Read(ImportView),
    /// `422 SETTINGS_FILE_UNREADABLE` : le format du jeu ne sait pas découper ce texte. `reason` est un code court de
    /// l'agent et `line` le numéro de la ligne fautive, jamais son contenu.
    Unreadable {
        reason: Option<String>,
        line: Option<u32>,
    },
}

/// Le chemin des réglages d'un service, ou `None` si `key` n'a pas la forme d'une clé de service.
pub fn settings_path(key: &str) -> Option<String> {
    is_service_key(key).then(|| format!("/services/{}/settings", key_segment(key)))
}

/// Lit une réponse typée. L'erreur de lecture ne recopie JAMAIS celle de `serde` : elle citerait la valeur fautive.
fn parse<T: DeserializeOwned>(body: serde_json::Value, what: &str) -> Result<T, LinkError> {
    serde_json::from_value(body).map_err(|_| LinkError::Protocol(format!("{what} illisible")))
}

impl LinkManager {
    /// Les réglages voulus et l'état du fichier d'un service de jeu (`GET /services/{key}/settings`). Un service
    /// qui n'est pas un service de fiche (ou un agent sans la capacité) : `Rejected` (404). Hors « Connecté » :
    /// `NotConnected`. Une clé qui n'en est pas une : `InvalidInput`, rien ne part.
    pub async fn read_game_settings(
        &self,
        id: &ServerId,
        key: &str,
    ) -> Result<SettingsView, LinkError> {
        let path = settings_path(key).ok_or(LinkError::InvalidInput(InputField::Filter))?;
        let (target, token) = self.credentials(id)?;
        let body = self.typed_get(&target, &token, path).await?;
        parse(body, "réglages du jeu")
    }

    /// Lit un texte collé (`POST /services/{key}/settings/analyze`) : une consultation, rien n'est écrit. Le texte
    /// peut porter un mot de passe ; il n'entre dans aucune trace. Un texte que le format du jeu ne sait pas lire
    /// n'est pas une panne : c'est `Analysis::Unreadable`.
    pub async fn analyze_game_settings(
        &self,
        id: &ServerId,
        key: &str,
        text: &Secret,
    ) -> Result<Analysis, LinkError> {
        let path = settings_path(key)
            .map(|path| format!("{path}/analyze"))
            .ok_or(LinkError::InvalidInput(InputField::Filter))?;
        let (target, token) = self.credentials(id)?;
        let body = serde_json::to_value(AnalyzeRequest {
            text: text.expose().to_owned(),
        })
        .map_err(|_| LinkError::Protocol("texte collé illisible".into()))?;
        let request = ApiRequest {
            method: Method::Post,
            path,
            body: Some(body),
            idempotency_key: None,
        };
        let deps = &self.inner.deps;
        let response = tokio::time::timeout(
            deps.config.request_timeout,
            deps.transport.request(&target, &token, &request),
        )
        .await
        .map_err(|_| LinkError::Timeout)??;
        if (200..300).contains(&response.status) {
            return parse(response.body, "lecture du texte collé").map(Analysis::Read);
        }
        match serde_json::from_value::<ErrorBody>(response.body.clone()) {
            Ok(parsed)
                if parsed.error.code.or_from_status(response.status)
                    == ErrorCode::SettingsFileUnreadable =>
            {
                Ok(Analysis::Unreadable {
                    reason: parsed
                        .error
                        .details
                        .get("reason")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    line: parsed
                        .error
                        .details
                        .get("line")
                        .and_then(Value::as_u64)
                        .and_then(|line| u32::try_from(line).ok()),
                })
            }
            _ => Err(rejection_at(&response.body, response.status)),
        }
    }

    /// Enregistre des réglages (`PUT /services/{key}/settings`, acte `GameSettingsSave`). `password` : le mot de
    /// passe de confirmation (`None` sous l'élévation ; l'agent est l'arbitre), jamais gardé. La réponse de l'agent
    /// (`200` ou refus typé) est rendue telle quelle. Les erreurs de `execute_act` s'appliquent (`NotConnected`,
    /// `NoDeviceKey`, `Incompatible(UpdateAgent)`, `InvalidInput(Credentials)`).
    pub async fn save_game_settings(
        &self,
        id: &ServerId,
        key: &str,
        request: &SaveSettingsRequest,
        password: Option<&Secret>,
    ) -> Result<ActionOutcome, LinkError> {
        let path = settings_path(key).ok_or(LinkError::UnreadableAct)?;
        let body = serde_json::to_value(request)
            .map_err(|_| LinkError::Protocol("enregistrement illisible".into()))?;
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

    #[test]
    fn a_service_key_becomes_one_encoded_segment_and_anything_else_never_leaves() {
        assert_eq!(
            settings_path("c_0123abcd").as_deref(),
            Some("/services/c_0123abcd/settings")
        );
        // Une unité du système : son nom s écrit tel quel dans le segment.
        let unit = settings_path("u_docker-terraria.service").unwrap();
        assert_eq!(unit, "/services/u_docker-terraria.service/settings");
        for bad in ["", "../x", "a/b", "c_ab/../settings", "c_ab?x=1"] {
            assert!(settings_path(bad).is_none(), "{bad}");
        }
    }
}
