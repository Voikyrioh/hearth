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

/// Une question d'installation (réglage essentiel non imposé).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionView {
    pub id: String,
    pub label: String,
    pub help: String,
    pub secret: bool,
    /// La valeur proposée ; absente pour un secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed: Option<String>,
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
