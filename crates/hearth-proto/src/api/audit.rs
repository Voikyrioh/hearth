//! Journal d'activité : `GET /audit` et `GET /audit/export` (administrateurs seulement).
//!
//! Le même `AuditEventItem` voyage dans le message `audit { event }` du flux temps réel.

use serde::{Deserialize, Serialize};

/// En-tête de la réponse de l'export, `true` quand le fichier ne contient que les entrées les
/// plus récentes du résultat (plafond atteint).
pub const EXPORT_TRUNCATED_HEADER: &str = "x-hearth-export-truncated";

/// Catalogue des codes d'action du journal : UNE source pour l'agent (`AuditAction::code`), le filtre
/// `action=` et la liaison cliente (`hearth-link`, types d'action de l'interface). Un code ajouté ici
/// sans être rangé dans un type d'action du client fait échouer un test de `hearth-link`.
pub mod action {
    pub const LOGIN: &str = "login";
    pub const LOGIN_LOCKED: &str = "login.locked";
    pub const LOGOUT: &str = "logout";
    pub const ACCOUNT_CREATE: &str = "account.create";
    pub const ACCOUNT_DELETE: &str = "account.delete";
    pub const ACCOUNT_ROLE: &str = "account.role";
    pub const ACCOUNT_PASSWORD: &str = "account.password";
    pub const ACCOUNT_PASSWORD_OWN: &str = "account.password.own";
    pub const SESSIONS_REVOKE: &str = "sessions.revoke";
    pub const ACCOUNTS_READ: &str = "accounts.read";
    pub const AUDIT_READ: &str = "audit.read";
    pub const AGENT_UPDATE: &str = "agent.update";

    /// Tous les codes, dans l'ordre du catalogue.
    pub const ALL: [&str; 12] = [
        LOGIN,
        LOGIN_LOCKED,
        LOGOUT,
        ACCOUNT_CREATE,
        ACCOUNT_DELETE,
        ACCOUNT_ROLE,
        ACCOUNT_PASSWORD,
        ACCOUNT_PASSWORD_OWN,
        SESSIONS_REVOKE,
        ACCOUNTS_READ,
        AUDIT_READ,
        AGENT_UPDATE,
    ];
}

/// Paramètres de requête des deux routes, tels qu'ils arrivent : des textes, contrôlés par
/// l'agent. Un filtre à plusieurs valeurs sépare celles-ci par des virgules.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditQuery {
    /// Identifiants de comptes (`marie,paul`).
    #[serde(default)]
    pub account: Option<String>,
    /// Codes d'action (`login,account.create`).
    #[serde(default)]
    pub action: Option<String>,
    /// Résultats : `ok`, `denied`, `failed`.
    #[serde(default)]
    pub outcome: Option<String>,
    /// Début de la période (RFC 3339), inclus.
    #[serde(default)]
    pub from: Option<String>,
    /// Fin de la période (RFC 3339), incluse.
    #[serde(default)]
    pub to: Option<String>,
    /// Recherche plein texte : du texte, jamais une requête.
    #[serde(default)]
    pub q: Option<String>,
    /// Curseur : les entrées d'identifiant plus petit (`next_before` de la page précédente).
    #[serde(default)]
    pub before: Option<String>,
    /// Entrées par page, de 1 à 100 (défaut 100). Sans effet sur l'export.
    #[serde(default)]
    pub limit: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OriginKindName {
    /// Un client sur le réseau.
    Client,
    /// Une sous-commande lancée sur le serveur.
    Cli,
    Assistant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutcomeName {
    Ok,
    Denied,
    Failed,
}

/// D'où vient l'action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditOrigin {
    pub kind: OriginKindName,
    /// Nom du poste (client seulement ; absent s'il n'est pas identifié).
    pub name: Option<String>,
    /// Adresse IP vue par l'agent (client seulement).
    pub addr: Option<String>,
    /// Le texte à afficher : « 10.0.0.7 (poste) », « 10.0.0.7 (inconnu) », « ligne de commande du
    /// serveur », « assistant ».
    pub text: String,
}

/// Une entrée du journal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEventItem {
    /// Croissant : sert de curseur et d'ordre.
    pub id: i64,
    /// RFC 3339 en UTC (l'interface l'affiche dans le fuseau du poste, BR-AUDIT-012).
    pub at: String,
    /// Identifiant du compte, figé à l'écriture ; absent pour la ligne de commande et pour une
    /// connexion refusée.
    pub account: Option<String>,
    pub origin: AuditOrigin,
    /// Code stable de l'action (`login`, `account.create`…).
    pub action: String,
    /// Libellé de l'action tel qu'écrit.
    pub action_label: String,
    pub target: Option<String>,
    pub outcome: OutcomeName,
    /// Pourquoi, pour un refus ou un échec (« … (999 autres fois en 1 min) » pour une synthèse).
    pub reason: Option<String>,
    /// Entrée de synthèse : combien d'autres fois le même événement (même compte ou « anonyme »,
    /// même action, même résultat, même cible et même raison, **jamais l'origine**) s'est produit
    /// dans la minute ; l'origine affichée est celle de la dernière occurrence ; 0 pour une entrée ordinaire.
    pub repeat_count: u32,
}

/// Réponse de `GET /audit` : les entrées de la plus récente à la plus ancienne.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditResponse {
    pub events: Vec<AuditEventItem>,
    /// À passer en `before` pour la page suivante ; absent à la dernière page.
    pub next_before: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item() -> AuditEventItem {
        AuditEventItem {
            id: 7,
            at: "2026-10-04T10:30:15.250Z".into(),
            account: None,
            origin: AuditOrigin {
                kind: OriginKindName::Cli,
                name: None,
                addr: None,
                text: "ligne de commande du serveur".into(),
            },
            action: "account.create".into(),
            action_label: "Création de compte".into(),
            target: Some("paul".into()),
            outcome: OutcomeName::Ok,
            reason: None,
            repeat_count: 0,
        }
    }

    #[test]
    fn kinds_and_outcomes_travel_in_lowercase() {
        let value = serde_json::to_value(item()).unwrap();
        assert_eq!(value["origin"]["kind"], json!("cli"));
        assert_eq!(value["outcome"], json!("ok"));
        for (name, text) in [
            (OutcomeName::Denied, "denied"),
            (OutcomeName::Failed, "failed"),
        ] {
            assert_eq!(serde_json::to_value(name).unwrap(), json!(text));
        }
        assert_eq!(
            serde_json::to_value(OriginKindName::Assistant).unwrap(),
            json!("assistant")
        );
    }

    #[test]
    fn an_absent_account_and_reason_are_null_not_missing() {
        let value = serde_json::to_value(item()).unwrap();
        assert_eq!(value["account"], json!(null));
        assert_eq!(value["reason"], json!(null));
    }

    #[test]
    fn every_query_parameter_is_optional() {
        let query: AuditQuery = serde_json::from_value(json!({})).unwrap();
        assert_eq!(query, AuditQuery::default());
        let query: AuditQuery =
            serde_json::from_value(json!({ "q": "marie", "limit": "20" })).unwrap();
        assert_eq!(query.q.as_deref(), Some("marie"));
        assert_eq!(query.limit.as_deref(), Some("20"));
    }

    #[test]
    fn the_response_round_trips() {
        let response = AuditResponse {
            events: vec![item()],
            next_before: Some(7),
        };
        let back: AuditResponse =
            serde_json::from_value(serde_json::to_value(&response).unwrap()).unwrap();
        assert_eq!(back, response);
    }
}
