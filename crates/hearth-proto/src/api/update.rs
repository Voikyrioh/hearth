//! Mise à jour de l'agent à distance : `GET /agent/update`, `POST /agent/update`,
//! `GET /agent/update/last`, et les messages de progression du flux (`stream::UpdateMessage`).
//!
//! Les dates sont des textes RFC 3339 en UTC. Les textes affichés viennent de l'interface,
//! indexés par ces codes (`step`, `outcome`, `reason`) ; aucun message libre ne voyage ici.

use serde::{Deserialize, Serialize};

/// Les étapes visibles d'une mise à jour (BR-UPDATE-013), dans l'ordre. `Done` clôt la mise à
/// jour : `outcome` dit comment elle s'est terminée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateStep {
    /// Téléchargement du binaire (avec un pourcentage).
    Download,
    /// Vérification de la signature minisign et de la somme SHA-256.
    Verify,
    /// Dépôt du binaire sur le disque.
    Install,
    /// L'ancien agent s'arrête, le nouveau démarre : le lien passe à « Reconnexion… ».
    Restart,
    /// Le superviseur attend que le nouvel agent réponde (60 s au plus).
    Check,
    /// Terminée : voir `outcome`.
    Done,
}

/// Comment une mise à jour s'est terminée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateOutcome {
    /// Le nouvel agent répond avec la nouvelle version (BR-UPDATE-013).
    Succeeded,
    /// Le nouvel agent n'a pas répondu à temps : l'ancien binaire est revenu (BR-UPDATE-015).
    RolledBack,
    /// Échec avant tout échange de binaire : l'agent n'a pas changé (téléchargement, signature…).
    Failed,
}

/// Pourquoi une mise à jour n'a pas abouti.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateReason {
    /// Le serveur n'a pas pu joindre l'adresse (pas d'accès à Internet, BR-UPDATE-019).
    Unreachable,
    /// Le téléchargement a échoué autrement (statut d'erreur, fichier trop gros, coupure).
    DownloadFailed,
    /// La somme SHA-256 du fichier ne correspond pas.
    BadChecksum,
    /// La signature minisign du fichier est refusée.
    BadSignature,
    /// Le fichier n'est pas un agent utilisable ici (mauvaise version annoncée, ne s'exécute pas).
    BadBinary,
    /// Le dépôt du binaire sur le disque a échoué.
    Staging,
    /// L'échange des binaires a échoué (l'ancien est resté en place).
    Swap,
    /// Le superviseur n'a pas pu être lancé.
    SupervisorLaunch,
    /// Le nouvel agent n'a pas répondu avec la nouvelle version dans les 60 s.
    NoAnswer,
    /// Le nouvel agent présente une autre identité (empreinte) : refusé.
    IdentityChanged,
    /// L'agent s'est arrêté pendant la mise à jour avant d'avoir lancé le superviseur.
    Interrupted,
    /// Le retour en arrière lui-même a échoué : à reprendre à la main (voir le runbook).
    RollbackFailed,
}

/// Corps de `POST /agent/update` : la cible, telle que le flux de versions la publie. L'agent
/// n'interroge jamais Internet de lui-même : il télécharge `url` et vérifie.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentUpdateRequest {
    /// La version visée (`0.2.0`) : plus récente que l'agent installé.
    pub version: String,
    /// Adresse HTTPS du binaire (x86_64 statique).
    pub url: String,
    /// Signature minisign du binaire : le contenu du fichier `.minisig`, ou son encodage base64.
    pub signature: String,
    /// Somme SHA-256 du binaire, en hexadécimal (64 caractères).
    pub sha256: String,
}

/// Réponse `202` de `POST /agent/update` : la mise à jour est acceptée, elle s'exécute côté
/// serveur et se suit par le flux (sujet `update`) ou `GET /agent/update`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentUpdateAccepted {
    pub version: String,
    pub step: UpdateStep,
}

/// Le résultat de la dernière mise à jour. Il survit au redémarrage de l'agent (fichier).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateResult {
    /// La version visée.
    pub version: String,
    /// La version qui tournait avant.
    pub previous: String,
    pub outcome: UpdateOutcome,
    pub reason: Option<UpdateReason>,
    pub at: String,
}

/// Où en est la mise à jour en cours.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateProgress {
    /// La version visée.
    pub version: String,
    pub step: UpdateStep,
    /// 0 à 100, pour l'étape `download` seulement.
    pub percent: Option<u8>,
    /// Pour l'étape `done` seulement.
    pub outcome: Option<UpdateOutcome>,
    pub reason: Option<UpdateReason>,
}

/// Réponse de `GET /agent/update`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentUpdateStatus {
    /// La version de l'agent qui répond.
    pub current: String,
    /// Installation gérée par le système (ou sans systemd) : pas de mise à jour à distance.
    pub managed: bool,
    pub in_progress: bool,
    /// La mise à jour en cours, s'il y en a une.
    pub progress: Option<UpdateProgress>,
    /// Le dernier résultat connu.
    pub last: Option<UpdateResult>,
}

/// Réponse de `GET /agent/update/last` : le dernier résultat (`null` si aucune mise à jour
/// n'a jamais eu lieu). Pour le client qui revient après une coupure ou un redémarrage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastUpdateResponse {
    pub last: Option<UpdateResult>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn steps_outcomes_and_reasons_use_snake_case_codes() {
        assert_eq!(
            serde_json::to_value(UpdateStep::Download).unwrap(),
            json!("download")
        );
        assert_eq!(
            serde_json::to_value(UpdateStep::Done).unwrap(),
            json!("done")
        );
        assert_eq!(
            serde_json::to_value(UpdateOutcome::RolledBack).unwrap(),
            json!("rolled_back")
        );
        assert_eq!(
            serde_json::to_value(UpdateReason::NoAnswer).unwrap(),
            json!("no_answer")
        );
        assert_eq!(
            serde_json::to_value(UpdateReason::BadChecksum).unwrap(),
            json!("bad_checksum")
        );
    }

    #[test]
    fn the_status_has_the_documented_shape() {
        let status = AgentUpdateStatus {
            current: "0.1.0".into(),
            managed: false,
            in_progress: true,
            progress: Some(UpdateProgress {
                version: "0.2.0".into(),
                step: UpdateStep::Download,
                percent: Some(35),
                outcome: None,
                reason: None,
            }),
            last: None,
        };
        let value = serde_json::to_value(&status).unwrap();
        assert_eq!(
            value,
            json!({
                "current": "0.1.0",
                "managed": false,
                "in_progress": true,
                "progress": { "version": "0.2.0", "step": "download", "percent": 35, "outcome": null, "reason": null },
                "last": null
            })
        );
        let back: AgentUpdateStatus = serde_json::from_value(value).unwrap();
        assert_eq!(back, status);
    }

    #[test]
    fn the_request_round_trips() {
        let request = AgentUpdateRequest {
            version: "0.2.0".into(),
            url: "https://exemple.org/hearth-agent".into(),
            signature: "untrusted comment: x".into(),
            sha256: "ab".repeat(32),
        };
        let text = serde_json::to_string(&request).unwrap();
        assert_eq!(
            serde_json::from_str::<AgentUpdateRequest>(&text).unwrap(),
            request
        );
    }
}
