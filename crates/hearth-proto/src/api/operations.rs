//! Suivi d'une opération par sa clé : `GET /operations/{id}`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    Running,
    Succeeded,
    Failed,
    /// L'agent s'est arrêté pendant l'exécution : on ne sait pas si elle a eu lieu.
    Interrupted,
}

/// Réponse de `GET /operations/{id}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationResponse {
    /// La clé d'opération envoyée par le client.
    pub id: String,
    /// Requête d'origine, au format `MÉTHODE /chemin`.
    pub kind: String,
    pub status: OperationStatus,
    /// Corps de la réponse de l'opération terminée, tel que le client l'a (ou l'aurait) reçu.
    pub result: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn statuses_travel_in_snake_case() {
        assert_eq!(
            serde_json::to_value(OperationStatus::Succeeded).unwrap(),
            json!("succeeded")
        );
    }

    #[test]
    fn a_running_operation_has_a_null_result() {
        let response = OperationResponse {
            id: "01J".into(),
            kind: "PUT /me/password".into(),
            status: OperationStatus::Running,
            result: None,
        };
        assert_eq!(
            serde_json::to_value(response).unwrap()["result"],
            json!(null)
        );
    }
}
