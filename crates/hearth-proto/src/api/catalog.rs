//! Le catalogue des fiches de serveurs de jeu que l'agent propose (HRT-101, ADR-0050) : `GET /catalog` et
//! `GET /catalog/{id}`. Annoncé par la capacité [`crate::api::service_ops::CAPABILITY_CATALOG`].
//!
//! Aucun secret n'y figure : une fiche ne contient aucune valeur secrète, et les réponses d'un utilisateur ne
//! passent jamais par ces types. Une fiche `preview` n'est listée que par une construction d'essai de l'agent.

use serde::{Deserialize, Serialize};

use crate::lenient::Items;

/// L'état d'une fiche dans le catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FicheStatus {
    Published,
    /// En cours de preuve : visible dans une construction d'essai seulement.
    Preview,
    #[serde(other)]
    Unknown,
}

/// D'où vient un chiffre de ressource (BR-FICHE-008).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceView {
    /// Mesuré par les essais de Hearth.
    Measured,
    /// Mesuré au repos : avec des joueurs, il en faut plus.
    MeasuredIdle,
    /// Annoncé par l'éditeur du jeu, à cette adresse.
    Publisher { url: String },
    /// Aucune preuve : Hearth ne sait pas.
    Unmeasured,
    #[serde(other)]
    Unknown,
}

/// Une fiche dans la liste.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FicheCard {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub version: u32,
    pub status: FicheStatus,
    /// `false` : la fiche est listée mais ne s'installe pas sur ce serveur (plateforme non disponible).
    #[serde(default = "yes")]
    pub installable: bool,
}

fn yes() -> bool {
    true
}

/// Une fiche écartée au chargement : son dossier et l'invariant violé (jamais un contenu).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefusedFiche {
    pub dir: String,
    pub invariant: String,
}

/// Réponse de `GET /catalog`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CatalogView {
    #[serde(default)]
    pub fiches: Items<FicheCard>,
    #[serde(default)]
    pub refused: Items<RefusedFiche>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageView {
    pub repository: String,
    pub tag: String,
    /// L'empreinte épinglée, si la fiche en a une.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantView {
    pub id: String,
    pub label: String,
    pub help: String,
    pub default: bool,
}

/// Un port que la fiche demande.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortView {
    pub id: String,
    pub container: u16,
    /// `tcp` ou `udp`.
    pub protocol: String,
    /// `game`, `query`, `voice`, `web_admin`, `remote_console` ou `other`.
    pub role: String,
    /// `always`, `optional` ou `never`.
    pub publish: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_host: Option<u16>,
}

/// Un dossier géré que la fiche demande (jamais un chemin du serveur).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderView {
    pub id: String,
    pub role: String,
    /// Le chemin DANS le service.
    pub target: String,
    pub backup: bool,
}

/// Un contrat à accepter avant l'installation (BR-FICHE-058).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgreementView {
    pub id: String,
    pub title: String,
    pub url: String,
    pub sentence: String,
}

/// Le genre de valeur d'une question : ce que le client doit afficher comme champ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum QuestionKind {
    Text,
    Secret,
    Int,
    Bool,
    Choice,
    IdList,
    Opaque,
    /// Un genre d'une version plus récente : le client montre un champ texte et l'agent juge.
    #[default]
    #[serde(other)]
    Unknown,
}

/// Un choix d'une question de genre `choice`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChoiceView {
    pub value: String,
    pub label: String,
}

/// Une question d'installation (réglage essentiel non imposé).
///
/// Le genre, les bornes et les choix sont ceux de la fiche : le client n'en invente aucun (HRT-102). Tous les
/// champs ajoutés par HRT-102 ont une valeur par défaut, pour qu'un agent plus ancien reste lisible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionView {
    pub id: String,
    pub label: String,
    pub help: String,
    pub secret: bool,
    /// La valeur proposée ; absente pour un secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed: Option<String>,
    #[serde(default)]
    pub kind: QuestionKind,
    /// Les choix d'une question de genre `choice` ; pour `bool`, les deux valeurs que le jeu comprend (activé, puis
    /// désactivé).
    #[serde(default)]
    pub choices: Vec<ChoiceView>,
    /// Bornes d'un entier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    /// Longueurs permises d'un texte ou d'un secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_len: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_len: Option<u32>,
    /// Jeu de caractères d'un texte : `line`, `file_stem` ou `arg_safe`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charset: Option<String>,
    /// « Ne pourra plus être changé après la création du monde ».
    #[serde(default)]
    pub frozen_after_creation: bool,
    /// Les variantes pour lesquelles la question existe ; vide : toutes.
    #[serde(default)]
    pub variants: Vec<String>,
    /// Le genre d'un secret : `game_password`, `admin_password` ou `server_token` ; absent pour un autre genre.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_kind: Option<String>,
    /// L'adresse où l'utilisateur crée un jeton de serveur (genre `secret` d'un jeton).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obtain_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuNeed {
    pub min_millis: Option<u32>,
    pub recommended_millis: Option<u32>,
    pub evidence: EvidenceView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryNeed {
    pub min_bytes: Option<u64>,
    pub recommended_bytes: Option<u64>,
    pub evidence: EvidenceView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskNeed {
    pub install_bytes: Option<u64>,
    pub evidence: EvidenceView,
}

/// Les ressources d'une fiche, chacune avec sa preuve (BR-FICHE-008) : aucun chiffre sans preuve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourcesView {
    pub cpu: CpuNeed,
    pub memory: MemoryNeed,
    pub disk: DiskNeed,
}

/// Un téléchargement que le programme du jeu fait lui-même au premier démarrage (BR-FICHE-010).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadView {
    /// Ce qui est téléchargé (« fichiers de jeu »).
    pub what: String,
    /// D'où (nom lisible de la source).
    pub source: String,
    /// La taille quand la fiche la connaît ; sa preuve est `evidence`.
    pub bytes: Option<u64>,
    pub evidence: EvidenceView,
}

/// Le premier démarrage d'une fiche (BR-FICHE-056) : sa durée annoncée, ce qu'il télécharge, ses repères.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirstStartView {
    /// Au-delà, Hearth ne peut plus dire si le jeu est prêt.
    pub ready_timeout_s: u32,
    /// Le jeu génère un monde au premier démarrage.
    pub world_generation: bool,
    #[serde(default)]
    pub downloads: Vec<DownloadView>,
    /// Les libellés des étapes que Hearth sait reconnaître dans les journaux du jeu.
    #[serde(default)]
    pub stages: Vec<String>,
}

/// Comment le monde est enregistré à l'arrêt (BR-JEU-002).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorldSavedView {
    /// L'image enregistre le monde quand elle reçoit le signal d'arrêt.
    ByImageOnSignal,
    /// Hearth envoie la suite d'étapes de la fiche.
    BySequence,
    /// Le jeu ne l'enregistre pas en s'arrêtant.
    NotSaved,
    #[default]
    #[serde(other)]
    Unknown,
}

/// La façon d'arrêter le jeu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopView {
    pub grace_s: u32,
    #[serde(default)]
    pub world_saved: WorldSavedView,
    /// La suite d'étapes a été essayée sur la vraie image ; sinon Hearth ne l'envoie pas.
    #[serde(default)]
    pub sequence_tested: bool,
}

/// Un réglage que la fiche pose sans que l'utilisateur l'ait choisi (BR-JEU-039).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImposedView {
    pub label: String,
    /// La valeur posée ; absente quand elle est tirée, dérivée ou secrète.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub why: String,
}

/// Réponse de `GET /catalog/{id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FicheDetail {
    pub card: FicheCard,
    /// L'empreinte de la fiche (hexadécimale) : celle que `POST /deployments` doit reprendre.
    pub digest: String,
    pub image: ImageView,
    /// Le propriétaire numérique des dossiers du service.
    pub owner_uid: u32,
    pub owner_gid: u32,
    #[serde(default)]
    pub variants: Items<VariantView>,
    #[serde(default)]
    pub ports: Items<PortView>,
    #[serde(default)]
    pub folders: Items<FolderView>,
    #[serde(default)]
    pub agreements: Items<AgreementView>,
    #[serde(default)]
    pub questions: Items<QuestionView>,
    pub resources: ResourcesView,
    /// Le premier démarrage (HRT-102) ; absent d'un agent plus ancien.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_start: Option<FirstStartView>,
    /// La façon d'arrêter le jeu (HRT-102) ; absente d'un agent plus ancien.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<StopView>,
    /// Les réglages posés par la fiche, avec leur raison (HRT-102).
    #[serde(default)]
    pub imposed: Items<ImposedView>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_evidence_this_version_does_not_know_is_unknown_and_a_publisher_keeps_its_address() {
        let read: EvidenceView =
            serde_json::from_value(serde_json::json!("brand_new")).expect("lu");
        assert_eq!(read, EvidenceView::Unknown);
        let publisher = EvidenceView::Publisher {
            url: "https://example.org/size".into(),
        };
        let json = serde_json::to_value(&publisher).expect("json");
        assert_eq!(json["publisher"]["url"], "https://example.org/size");
        assert_eq!(
            serde_json::from_value::<EvidenceView>(json).expect("lu"),
            publisher
        );
    }

    #[test]
    fn the_catalog_tolerates_an_unreadable_card_and_counts_it() {
        let view: CatalogView = serde_json::from_value(serde_json::json!({
            "fiches": [
                {"id": "terraria", "name": "Terraria", "summary": "s", "version": 1, "status": "preview"},
                {"id": 7}
            ],
            "refused": [{"dir": "bad", "invariant": "I2"}]
        }))
        .expect("lu");
        assert_eq!(view.fiches.len(), 1);
        assert_eq!(view.fiches.unreadable(), 1);
        assert!(view.fiches[0].installable);
        assert_eq!(view.refused[0].invariant, "I2");
    }
}
