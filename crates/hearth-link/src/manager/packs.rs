//! Lire un pack de mods déposé avant de l'installer (HRT-124, `POST /deployments/pack-check`, BR-MOD-007) : une
//! CONSULTATION, ni acte, ni clé d'opération, ni journal. Le corps ne porte que l'identifiant du dépôt (jamais un
//! chemin, jamais un nom de fichier) ; la réponse dit ce que l'agent a lu dans le CONTENU (genre, fichiers, mods,
//! résumé Modrinth). Un refus se lit par son code (`PACK_UNKNOWN`, `PACK_REFUSED` et sa raison fixe).

use std::time::Duration;

use hearth_proto::api::packs::{
    INSPECT_BUDGET_S, INSPECT_PATH, PackInspectRequest, PackInspectView,
};
use hearth_proto::error::{ErrorBody, ErrorCode};
use serde_json::Value;

use super::LinkManager;
use super::audit::rejection_at;
use super::parse::parse;
use crate::domain::server::ServerId;
use crate::error::LinkError;

/// Un identifiant de dépôt : lettres, chiffres et tirets, 64 caractères au plus (comme l'agent ; un ULID en majuscules
/// pour un transfert de la zone d'attente).
pub fn is_deposit_id(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 64
        && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// Pourquoi l'agent n'a pas pu dire ce qu'est ce pack : des CODES et des nombres, jamais un nom d'entrée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackRefusal {
    /// Aucun dépôt complet de ce compte sous cet identifiant (`PACK_UNKNOWN`).
    Unknown,
    /// Le pack est refusé (`PACK_REFUSED`) : la raison fixe de l'agent (`PACK_IS_A_DOWNLOAD_LIST`, `ARCHIVE_REFUSED`...)
    /// et, selon elle, le nombre de mods listés ou l'hôte refusé.
    Refused {
        reason: String,
        mods: Option<u32>,
        host: Option<String>,
    },
}

/// Un code de raison de l'agent : majuscules, chiffres et souligné, 40 caractères au plus ; tout autre texte n'est
/// jamais répété.
fn stable_reason(text: &str) -> Option<String> {
    let ok = !text.is_empty()
        && text.len() <= 40
        && text
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_');
    ok.then(|| text.to_owned())
}

/// Un nom d'hôte tel que l'agent le rend : minuscules, chiffres, point et tiret, 100 caractères au plus.
fn stable_host(text: &str) -> Option<String> {
    let ok = !text.is_empty()
        && text.len() <= 100
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-');
    ok.then(|| text.to_owned())
}

/// Lit le refus d'une lecture de pack : `404 PACK_UNKNOWN`, `422 PACK_REFUSED` et ses détails ; tout autre refus est
/// un refus ordinaire (droit, moteur, lien).
pub fn read_refusal(status: u16, body: &Value) -> Result<PackRefusal, LinkError> {
    let Ok(parsed) = serde_json::from_value::<ErrorBody>(body.clone()) else {
        return Err(rejection_at(body, status));
    };
    if parsed.error.missing_right().is_some() {
        return Err(rejection_at(body, status));
    }
    match parsed.error.code.or_from_status(status) {
        ErrorCode::PackUnknown => Ok(PackRefusal::Unknown),
        ErrorCode::PackRefused => {
            let details = &parsed.error.details;
            Ok(PackRefusal::Refused {
                reason: details
                    .get("reason")
                    .and_then(Value::as_str)
                    .and_then(stable_reason)
                    .unwrap_or_else(|| "PACK_REFUSED".to_owned()),
                mods: details
                    .get("mods")
                    .and_then(Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok()),
                host: details
                    .get("host")
                    .and_then(Value::as_str)
                    .and_then(stable_host),
            })
        }
        _ => Err(rejection_at(body, status)),
    }
}

impl LinkManager {
    /// Ce que l'agent lit dans le pack déposé sous `deposit`, ou pourquoi il ne le dit pas. Hors « Connecté » :
    /// `NotConnected`. Un identifiant qui n'en est pas un : `InvalidInput`, rien ne part. Délai allongé : la lecture
    /// relit toute l'archive.
    pub async fn inspect_pack(
        &self,
        id: &ServerId,
        deposit: &str,
    ) -> Result<Result<PackInspectView, PackRefusal>, LinkError> {
        if !is_deposit_id(deposit) {
            return Err(LinkError::InvalidInput(crate::error::InputField::Deposit));
        }
        let (target, token) = self.credentials(id)?;
        let body = serde_json::to_value(PackInspectRequest {
            deposit: deposit.to_owned(),
        })
        .map_err(|_| LinkError::Protocol("consultation de pack illisible".into()))?;
        let (status, answer) = self
            .post_slow_raw(
                &target,
                &token,
                INSPECT_PATH.to_owned(),
                body,
                Duration::from_secs(INSPECT_BUDGET_S),
            )
            .await?;
        if (200..300).contains(&status) {
            return parse(answer, "pack déposé").map(Ok);
        }
        read_refusal(status, &answer).map(Err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refusal(status: u16, body: &Value) -> Result<PackRefusal, LinkError> {
        read_refusal(status, body)
    }

    fn refused_body(details: Value) -> Value {
        serde_json::json!({ "error": { "code": "PACK_REFUSED", "message": "x", "details": details } })
    }

    #[test]
    fn a_refused_pack_says_its_fixed_reason_and_only_numbers_or_a_host_name() {
        let list =
            refused_body(serde_json::json!({ "reason": "PACK_IS_A_DOWNLOAD_LIST", "mods": 652 }));
        assert_eq!(
            refusal(422, &list).unwrap(),
            PackRefusal::Refused {
                reason: "PACK_IS_A_DOWNLOAD_LIST".into(),
                mods: Some(652),
                host: None
            }
        );
        let host = refused_body(
            serde_json::json!({ "reason": "PACK_HOST_FORBIDDEN", "host": "evil.example" }),
        );
        assert_eq!(
            refusal(422, &host).unwrap(),
            PackRefusal::Refused {
                reason: "PACK_HOST_FORBIDDEN".into(),
                mods: None,
                host: Some("evil.example".into())
            }
        );
        // Un texte qui n'est pas un code, un hôte douteux : jamais répétés.
        let hostile =
            refused_body(serde_json::json!({ "reason": "<script>", "host": "Evil Host/../x" }));
        assert_eq!(
            refusal(422, &hostile).unwrap(),
            PackRefusal::Refused {
                reason: "PACK_REFUSED".into(),
                mods: None,
                host: None
            }
        );
    }

    #[test]
    fn an_unknown_deposit_is_told_and_any_other_refusal_stays_an_ordinary_one() {
        let unknown = serde_json::json!({ "error": { "code": "PACK_UNKNOWN", "message": "x", "details": {} } });
        assert_eq!(refusal(404, &unknown).unwrap(), PackRefusal::Unknown);
        let forbidden = serde_json::json!({ "error": { "code": "FORBIDDEN_ROLE", "message": "x", "details": {} } });
        assert!(matches!(
            refusal(403, &forbidden),
            Err(LinkError::Rejected(Some(ErrorCode::ForbiddenRole)))
        ));
        assert!(matches!(
            refusal(500, &Value::Null),
            Err(LinkError::Rejected(None))
        ));
    }

    #[test]
    fn a_deposit_identifier_never_becomes_a_path_or_a_name() {
        assert!(is_deposit_id("0a1b-2c3d") && is_deposit_id("01HZY3F8M2K9Q4V6T0R7N5X2PA"));
        for bad in ["", "../x", "a/b", "a b", "a.zip", "é", &"a".repeat(65)] {
            assert!(!is_deposit_id(bad), "{bad}");
        }
    }
}
