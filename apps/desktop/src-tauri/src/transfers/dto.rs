//! Ce que la coquille échange avec l'interface pour envoyer un fichier dans la zone d'attente de l'agent (HRT-96,
//! HRT-95, `BR-FILES-019`, `BR-MOD-015`).
//!
//! - **Entrée** : un chemin de fichier local (rendu par la boîte « Ouvrir » du système, ou par un dépôt dans la
//!   fenêtre) : la coquille lit le fichier elle-même, la page n'en reçoit JAMAIS le contenu.
//! - **Sorties** : l'état de chaque transfert (octets durables sur le serveur, état, raison), copie fidèle de ce que
//!   la liaison rend. Aucun texte de l'agent n'est repris : l'interface choisit ses phrases d'après les codes. Le nom
//!   du fichier ne sort que vers la page qui l'a choisi : il n'est ni tracé ni écrit sur le disque. Les nombres sont
//!   bornés pour `specta` (`f64`).

use hearth_link::UploadRefusal;
use serde::{Deserialize, Serialize};
use specta::Type;

/// Où en est un transfert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TransferStatusDto {
    /// Des morceaux partent.
    Sending,
    /// Le lien est coupé (ou le serveur manque de place) : « Reprendre l'envoi » est un geste, jamais automatique.
    Interrupted,
    /// Le fichier est dans la zone d'attente de l'agent, vérifié.
    Done,
    /// Annulé : la partie reçue est retirée du serveur.
    Cancelled,
    /// Fini sans reprise possible : la raison est dans `refusal`.
    Failed,
}

/// Pourquoi un transfert est coupé ou refusé.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TransferRefusalDto {
    /// Le disque du serveur n'a plus la place : il manque `missing` octets, il resterait `remaining` de libre
    /// (`BR-FILES-013`).
    NoSpace { missing: f64, remaining: f64 },
    /// Trop de transferts en cours sur le serveur ou pour ce compte.
    TooMany,
    /// Le compte n'a plus le droit de gérer les services.
    MissingRight,
    /// L'agent a refusé le fichier : recommencer.
    Rejected,
    /// Le fichier local n'est plus lisible.
    SourceUnreadable,
    /// Le fichier local a changé pendant l'envoi.
    SourceChanged,
    /// Plus rien à reprendre sur le serveur (abandonné, expiré : 24 heures).
    Gone,
}

impl From<&UploadRefusal> for TransferRefusalDto {
    fn from(refusal: &UploadRefusal) -> Self {
        match refusal {
            UploadRefusal::NoSpace {
                missing_bytes,
                free_after_bytes,
            } => Self::NoSpace {
                missing: *missing_bytes as f64,
                remaining: *free_after_bytes as f64,
            },
            UploadRefusal::TooMany => Self::TooMany,
            UploadRefusal::MissingRight => Self::MissingRight,
            UploadRefusal::Rejected => Self::Rejected,
            UploadRefusal::SourceUnreadable => Self::SourceUnreadable,
            UploadRefusal::SourceChanged => Self::SourceChanged,
        }
    }
}

/// Un transfert, tel que le panneau « Transferts » le montre.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TransferDto {
    /// L'identifiant du dépôt sur le serveur (un ULID) : celui que le consommateur du dépôt reçoit.
    pub id: String,
    pub server_id: String,
    /// Le dernier nom du fichier, tel que la page l'a choisi.
    pub name: String,
    /// La taille du fichier, en octets.
    pub size: f64,
    /// Les octets reçus ET écrits sur le serveur (jamais en arrière).
    pub sent: f64,
    pub status: TransferStatusDto,
    pub refusal: Option<TransferRefusalDto>,
}

/// Le fichier choisi, avant tout envoi.
#[derive(Debug, Clone, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DepositSourceDto {
    pub path: String,
}

/// Pourquoi l'ouverture du dépôt est refusée.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DepositRefusal {
    WrongPassword,
    PasswordRequired,
    TooManyAttempts {
        wait: f64,
    },
    Busy,
    /// Le disque du serveur n'a pas la place de tout le fichier (`BR-FILES-013`).
    NoSpace {
        missing: f64,
        remaining: f64,
    },
    /// Trop de dépôts en attente (huit) ou de transferts en cours.
    TooMany,
    /// Le nom du fichier n'est pas valable pour le serveur.
    NameInvalid,
    /// Le fichier est vide, illisible, ou n'est pas un fichier ordinaire (un dossier, par exemple).
    SourceUnreadable,
    /// Un refus que ce client ne distingue pas.
    Other,
}

/// L'issue de l'ouverture d'un dépôt.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DepositOutcome {
    /// Le dépôt est ouvert et l'envoi commence.
    Started {
        transfer: TransferDto,
    },
    Refused {
        refusal: DepositRefusal,
    },
    /// Le lien est tombé avant la réponse : on ne sait pas si le dépôt est ouvert. Jamais rejoué.
    Unknown,
}
