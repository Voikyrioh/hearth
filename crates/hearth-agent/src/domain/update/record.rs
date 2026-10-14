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

use super::kept::{Extras, Kept};
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
    /// Reprise d'un travail orphelin (BR-UPDATE-028) : pas d'arrêt ni d'échange, le superviseur part
    /// du contrôle. Un seul sens : ce drapeau ne dit PAS « reprise déjà tentée » (ce sont les reprises
    /// comptées dans le marqueur d'étape, `Leftovers::recovery_attempts`).
    #[serde(default)]
    pub recover: bool,
}

impl Job {
    pub fn requester(&self) -> Requester {
        Requester {
            by: self.requested_by.clone(),
            name: self.client_name.clone(),
            addr: self.client_addr.clone(),
            extra: Extras::default(),
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
    /// Champs d'une version plus récente, gardés et réécrits (`Extras`).
    #[serde(flatten)]
    pub extra: Extras,
}

/// Où en est la mise à jour (`update/state.json`) : l'étape visible, pour l'agent qui revient et
/// pour les clients qui se reconnectent pendant le redémarrage. **Écrit dès la demande** par
/// l'agent, puis par le superviseur : c'est la trace de l'intention. Un fichier qui reste alors
/// qu'aucun superviseur ne travaille est un travail orphelin (`orphan::classify_orphan`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupervisorState {
    pub version: String,
    /// Gardée telle qu'écrite quand cette version ne la comprend pas (`Kept`) : jamais réécrite en `unknown`.
    pub step: Kept<UpdateStep>,
    /// La version qui tournait avant.
    #[serde(default)]
    pub previous: String,
    #[serde(default)]
    pub requester: Requester,
    /// Champs d'une version plus récente, gardés et réécrits (`Extras`).
    #[serde(flatten)]
    pub extra: Extras,
}

impl SupervisorState {
    /// Le fichier d'étape après que le superviseur est passé à `step`. Le fichier trouvé sur le disque est
    /// MIS À JOUR quand il est celui de ce travail (même version visée) : les champs qu'une version plus
    /// récente y a ajoutés, au niveau du fichier comme dans le demandeur, sont gardés (`Extras`). Le fichier
    /// d'un autre travail, ou absent, est remplacé par un fichier neuf.
    pub fn stepped(found: Option<Self>, job: &Job, step: UpdateStep) -> Self {
        let mut state = match found {
            Some(found) if found.version == job.version => found,
            _ => Self {
                version: job.version.clone(),
                step: step.into(),
                previous: String::new(),
                requester: Requester::default(),
                extra: Extras::default(),
            },
        };
        state.step = step.into();
        state.previous = job.previous.clone();
        state.requester.by = job.requested_by.clone();
        state.requester.name = job.client_name.clone();
        state.requester.addr = job.client_addr.clone();
        state
    }
}

/// Le dernier résultat, écrit par le superviseur (ou par l'agent pour un échec avant l'échange).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateRecord {
    /// La version visée ; `None` quand elle ne se sait pas (trace de travail illisible).
    #[serde(default, with = "version_text")]
    pub version: Option<String>,
    pub previous: String,
    /// Gardée telle qu'écrite quand cette version ne la comprend pas (`Kept`) : jamais réécrite en `unknown`.
    pub outcome: Kept<UpdateOutcome>,
    /// Gardée telle qu'écrite quand cette version ne la comprend pas (`Kept`) : un ancien agent qui relit puis
    /// réécrit ce fichier après un retour arrière ne transforme pas une raison nouvelle en `unknown`.
    #[serde(default)]
    pub reason: Option<Kept<UpdateReason>>,
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
    /// Champs d'une version plus récente, gardés et réécrits (`Extras`).
    #[serde(flatten)]
    pub extra: Extras,
}

/// La version visée sur le disque : une **chaîne** (vide quand elle est inconnue), jamais `null` ni
/// une clé absente, pour qu'un agent d'AVANT cette version relise un `last.json` écrit par un plus
/// récent (retour arrière : l'ancien binaire relit le fichier du nouveau). Vide, `null` ou absente
/// se relisent comme « inconnue ».
mod version_text {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &Option<String>, out: S) -> Result<S::Ok, S::Error> {
        out.serialize_str(value.as_deref().unwrap_or(""))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(input: D) -> Result<Option<String>, D::Error> {
        let text = Option::<String>::deserialize(input)?;
        Ok(text.filter(|text| !text.is_empty()))
    }
}

impl UpdateRecord {
    /// Ce que l'API rend.
    pub fn to_result(&self) -> UpdateResult {
        UpdateResult {
            version: self.version.clone().unwrap_or_default(),
            version_unknown: self.version.is_none(),
            previous: self.previous.clone(),
            // Une issue que cet agent ne comprend pas se dit `unknown` au client (et reste intacte sur le disque).
            outcome: self.outcome.wire(),
            reason: self.reason.as_ref().map(Kept::wire),
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

    fn job() -> Job {
        Job {
            version: "0.3.0".into(),
            previous: "0.2.0".into(),
            binary: "/b".into(),
            staged: "/s".into(),
            backup: "/k".into(),
            probe_addr: "127.0.0.1:7341".into(),
            fingerprint: "ab".into(),
            grace_ms: 1,
            check_window_ms: 1,
            poll_ms: 1,
            requested_by: Some("marie".into()),
            client_name: None,
            client_addr: None,
            recover: false,
        }
    }

    /// Le superviseur met à jour le fichier d'étape de son travail sans effacer les champs d'un agent plus récent,
    /// au niveau du fichier comme dans le demandeur ; un fichier d'un autre travail est remplacé.
    #[test]
    fn the_supervisor_step_keeps_the_fields_of_a_newer_agent_and_replaces_another_jobs_file() {
        let text = r#"{"version":"0.3.0","step":"etape_du_futur","previous":"0.2.0",
            "requester":{"by":"marie","du_futur":{"a":[1,{"b":null}]}},"racine_du_futur":[{"c":2}]}"#;
        let found: SupervisorState = serde_json::from_str(text).unwrap();
        let stepped = SupervisorState::stepped(Some(found), &job(), UpdateStep::Check);
        let written = serde_json::to_value(&stepped).unwrap();
        assert_eq!(written["step"], "check");
        assert_eq!(written["racine_du_futur"], serde_json::json!([{"c":2}]));
        assert_eq!(
            written["requester"]["du_futur"],
            serde_json::json!({"a":[1,{"b":null}]})
        );
        let other: SupervisorState = serde_json::from_str(
            r#"{"version":"0.9.0","step":"check","previous":"0.8.0","racine_du_futur":1}"#,
        )
        .unwrap();
        let fresh = SupervisorState::stepped(Some(other), &job(), UpdateStep::Check);
        assert_eq!(fresh.version, "0.3.0");
        assert!(
            fresh.extra.is_empty(),
            "le fichier d'un autre travail n'est pas repris"
        );
        assert!(
            SupervisorState::stepped(None, &job(), UpdateStep::Check)
                .extra
                .is_empty()
        );
    }

    /// `UpdateReason` : une raison écrite par un agent plus récent est réécrite à l'identique, pas en `unknown`.
    #[test]
    fn a_reason_from_the_future_is_rewritten_verbatim_and_said_unknown_on_the_wire() {
        let text = r#"{"version":"0.3.0","previous":"0.2.0","outcome":"failed","reason":"raison_du_futur","at":"x"}"#;
        let record: UpdateRecord = serde_json::from_str(text).unwrap();
        assert_eq!(record.to_result().reason, Some(UpdateReason::Unknown));
        let written = serde_json::to_value(&record).unwrap();
        assert_eq!(written["reason"], "raison_du_futur");
        let known: UpdateRecord = serde_json::from_str(
            r#"{"version":"0.3.0","previous":"0.2.0","outcome":"failed","reason":"no_answer","at":"x"}"#,
        )
        .unwrap();
        assert_eq!(known.to_result().reason, Some(UpdateReason::NoAnswer));
    }

    #[test]
    fn the_result_carries_what_the_api_documents() {
        let record = UpdateRecord {
            version: Some("0.2.0".into()),
            previous: "0.1.0".into(),
            outcome: UpdateOutcome::RolledBack.into(),
            reason: Some(UpdateReason::NoAnswer.into()),
            at: "2026-10-05T10:00:00Z".into(),
            requested_by: Some("marie".into()),
            client_name: None,
            client_addr: None,
            reported: true,
            extra: Default::default(),
        };
        let result = record.to_result();
        assert_eq!(result.outcome, UpdateOutcome::RolledBack);
        assert_eq!(result.reason, Some(UpdateReason::NoAnswer));
        assert_eq!(result.version, "0.2.0");
        assert_eq!(result.previous, "0.1.0");
    }

    #[test]
    fn an_unknown_version_is_absent_never_a_text_that_looks_like_a_version() {
        let record = UpdateRecord {
            version: None,
            previous: "0.1.0".into(),
            outcome: UpdateOutcome::Failed.into(),
            reason: Some(UpdateReason::Interrupted.into()),
            at: "2026-10-05T10:00:00Z".into(),
            requested_by: None,
            client_name: None,
            client_addr: None,
            reported: false,
            extra: Default::default(),
        };
        let result = record.to_result();
        assert_eq!(result.version, "");
        assert!(result.version_unknown);
        // Sur le disque : une chaîne vide (un agent d'avant lit toujours `version` comme un texte).
        let written = serde_json::to_value(&record).unwrap();
        assert_eq!(written["version"], serde_json::json!(""));
        #[derive(serde::Deserialize)]
        struct OlderRecord {
            version: String,
        }
        let older: OlderRecord = serde_json::from_value(written).unwrap();
        assert_eq!(older.version, "");
        // Écrit puis relu sur le disque : l'absence reste une absence.
        let back: UpdateRecord =
            serde_json::from_str(&serde_json::to_string(&record).unwrap()).unwrap();
        assert_eq!(back.version, None);
        // Un résultat ordinaire n'est pas marqué.
        let known = UpdateRecord {
            version: Some("0.2.0".into()),
            ..record
        };
        assert!(!known.to_result().version_unknown);
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
