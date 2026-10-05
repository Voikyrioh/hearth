//! Ce que la mise à jour écrit sur le disque, dans `update/` du dossier de données : le travail du
//! superviseur (`Job`) et le dernier résultat (`UpdateRecord`), qui survit au redémarrage de
//! l'agent (BR-UPDATE-017). Du JSON, lu par l'ancien agent, le superviseur et le nouvel agent :
//! trois processus, parfois de deux versions : les champs ne se retirent pas, ils s'ajoutent
//! (avec `#[serde(default)]`).
//!
//! **Aucun secret** : ni jeton, ni mot de passe. L'identité de qui a demandé est un nom de compte
//! et l'origine, pour le journal d'activité.

use std::path::PathBuf;

use hearth_proto::api::update::{UpdateOutcome, UpdateReason, UpdateResult, UpdateStep};
use serde::{Deserialize, Serialize};

/// Le travail confié au superviseur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    /// La version attendue du nouvel agent.
    pub version: String,
    /// La version qui tourne et à laquelle revenir.
    pub previous: String,
    /// Le binaire installé, à remplacer.
    pub binary: PathBuf,
    /// Le nouveau binaire (vérifié), déposé dans `update/`.
    pub staged: PathBuf,
    /// Où l'ancien binaire est gardé pendant l'échange.
    pub backup: PathBuf,
    /// Adresse du contrôle `GET /hello` (`127.0.0.1:7341`).
    pub probe_addr: String,
    /// Empreinte complète du certificat de l'agent (hexadécimal) : elle ne doit pas changer.
    pub fingerprint: String,
    /// Délais, en millisecondes (les tests les raccourcissent).
    pub grace_ms: u64,
    pub check_window_ms: u64,
    pub poll_ms: u64,
    /// Qui a demandé, pour le journal : nom du compte, poste, adresse.
    #[serde(default)]
    pub requested_by: Option<String>,
    #[serde(default)]
    pub client_name: Option<String>,
    #[serde(default)]
    pub client_addr: Option<String>,
    /// Reprise d'un travail orphelin (BR-UPDATE-028) : pas d'arrêt ni d'échange, seulement le
    /// contrôle du binaire en place, ou le retour à l'ancien.
    #[serde(default)]
    pub recover: bool,
}

impl Job {
    pub fn requester(&self) -> Requester {
        Requester {
            by: self.requested_by.clone(),
            name: self.client_name.clone(),
            addr: self.client_addr.clone(),
        }
    }
}

/// Qui a demandé la mise à jour, pour le journal d'activité.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Requester {
    #[serde(default)]
    pub by: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub addr: Option<String>,
}

/// Où en est la mise à jour (`update/state.json`) : l'étape visible, pour l'agent qui revient et
/// pour les clients qui se reconnectent pendant le redémarrage. **Écrit dès la demande** par
/// l'agent, puis par le superviseur : c'est la trace de l'intention. Un fichier qui reste alors
/// qu'aucun superviseur ne travaille est un travail orphelin (`orphan::classify_orphan`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupervisorState {
    pub version: String,
    pub step: UpdateStep,
    /// La version qui tournait avant.
    #[serde(default)]
    pub previous: String,
    #[serde(default)]
    pub requester: Requester,
}

/// Le dernier résultat, écrit par le superviseur (ou par l'agent pour un échec avant l'échange).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateRecord {
    pub version: String,
    pub previous: String,
    pub outcome: UpdateOutcome,
    #[serde(default)]
    pub reason: Option<UpdateReason>,
    /// RFC 3339, UTC.
    pub at: String,
    #[serde(default)]
    pub requested_by: Option<String>,
    #[serde(default)]
    pub client_name: Option<String>,
    #[serde(default)]
    pub client_addr: Option<String>,
    /// Le journal d'activité en a déjà une entrée : le prochain démarrage n'en écrit pas une
    /// seconde.
    #[serde(default)]
    pub reported: bool,
}

impl UpdateRecord {
    /// Ce que l'API rend.
    pub fn to_result(&self) -> UpdateResult {
        UpdateResult {
            version: self.version.clone(),
            previous: self.previous.clone(),
            outcome: self.outcome,
            reason: self.reason,
            at: self.at.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_written_by_an_older_version_without_the_new_fields_is_still_read() {
        let text = r#"{"version":"0.2.0","previous":"0.1.0","outcome":"succeeded","at":"2026-10-05T10:00:00Z"}"#;
        let record: UpdateRecord = serde_json::from_str(text).expect("lisible");
        assert_eq!(record.reason, None);
        assert!(!record.reported);
        assert_eq!(record.requested_by, None);
    }

    #[test]
    fn the_result_carries_what_the_api_documents() {
        let record = UpdateRecord {
            version: "0.2.0".into(),
            previous: "0.1.0".into(),
            outcome: UpdateOutcome::RolledBack,
            reason: Some(UpdateReason::NoAnswer),
            at: "2026-10-05T10:00:00Z".into(),
            requested_by: Some("marie".into()),
            client_name: None,
            client_addr: None,
            reported: true,
        };
        let result = record.to_result();
        assert_eq!(result.outcome, UpdateOutcome::RolledBack);
        assert_eq!(result.reason, Some(UpdateReason::NoAnswer));
        assert_eq!(result.version, "0.2.0");
        assert_eq!(result.previous, "0.1.0");
    }

    #[test]
    fn a_job_round_trips_and_holds_no_secret_field() {
        let job = Job {
            version: "0.2.0".into(),
            previous: "0.1.0".into(),
            binary: "/usr/local/bin/hearth-agent".into(),
            staged: "/var/lib/hearth/update/hearth-agent.new".into(),
            backup: "/usr/local/bin/.hearth-agent.previous".into(),
            probe_addr: "127.0.0.1:7341".into(),
            fingerprint: "ab".repeat(32),
            grace_ms: 2000,
            check_window_ms: 60_000,
            poll_ms: 1000,
            requested_by: Some("marie".into()),
            client_name: Some("PC".into()),
            client_addr: Some("192.168.1.2".into()),
            recover: false,
        };
        let text = serde_json::to_string(&job).unwrap();
        for forbidden in ["password", "token", "secret", "signature"] {
            assert!(!text.to_lowercase().contains(forbidden), "{forbidden}");
        }
        assert_eq!(serde_json::from_str::<Job>(&text).unwrap(), job);
    }
}
