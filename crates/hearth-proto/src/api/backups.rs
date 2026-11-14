//! Sauvegarder les données d'un service (HRT-97, ADR-0047, BR-BACKUP-*) : lister, estimer, créer, vérifier,
//! supprimer une sauvegarde, régler les éléments ignorés. Le téléchargement d'une archive attend les
//! transferts (HRT-95) et la restauration vient après (HRT-98) : aucun type d'ici n'en parle.
//!
//! - `GET /services/{key}/backups` : la liste, l'espace pris, l'espace libre et les éléments ignorés ;
//! - `POST /services/{key}/backups/preview` : l'estimation de la fenêtre de création (aucune écriture) ;
//! - `POST /services/{key}/backups` (acte `BackupCreate` `0x19`) : `202 { operation_id, backup_id }` ;
//! - `POST /backups/{id}/verify` : la vérification complète à la demande ;
//! - `DELETE /backups/{id}` (acte `BackupDelete` `0x1B`, jamais couvert par l'élévation) ;
//! - `PUT /services/{key}/backup-settings` (acte `BackupSettings` `0x1C`) : les éléments ignorés.
//!
//! Rien d'ici ne porte la note d'un compte hors de la liste (jamais au journal d'activité, BR-BACKUP-012) ni
//! un chemin du serveur : les chemins sont ceux du SERVICE (cible de montage, chemin relatif au dossier), pas
//! ceux de la machine. Évolution : toute énumération lue tolère une valeur inconnue (`Unknown`), tout champ
//! ajouté plus tard est facultatif (`serde(default)`).

use serde::{Deserialize, Serialize};

use crate::admin_act::FINGERPRINT_LEN;

/// Capacité annoncée par `GET /hello` quand l'agent sait sauvegarder (HRT-97).
pub use crate::api::service_ops::CAPABILITY_SERVICE_BACKUPS;

/// La note d'une sauvegarde : 80 caractères au plus (BR-BACKUP-012).
pub const NOTE_MAX_CHARS: usize = 80;
/// Au plus ce nombre d'éléments dans une liste d'éléments ignorés (ou de retraits) d'un service.
pub const MAX_ITEMS: usize = 500;
/// Un fichier au-delà de cette taille est signalé à la fenêtre de création (BR-BACKUP-033).
pub const BIG_FILE_BYTES: u64 = 1 << 30;
/// Combien de gros fichiers et d'éléments ignorés une estimation nomme au plus.
pub const MAX_LISTED: usize = 20;
/// L'estimation de la taille dure 60 secondes au plus (BR-BACKUP-009).
pub const ESTIMATE_BUDGET_S: u64 = 60;
/// Au-delà de ce nombre de sauvegardes, l'onglet avertit (BR-BACKUP-015).
pub const WARN_ABOVE: u32 = 10;

/// Comment la copie a été faite (BR-BACKUP-006).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupMode {
    /// Le service n'était pas en marche : copie directe.
    Stopped,
    /// Copie à chaud : le service n'est pas arrêté.
    Hot,
    /// Arrêter, sauvegarder, relancer (seulement s'il tournait).
    StopCopyStart,
    #[serde(other)]
    Unknown,
}

impl BackupMode {
    /// Le code du mode tel qu'il s'écrit sur le fil ; c'est aussi ce que la preuve de clé de `BackupCreate`
    /// signe.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Hot => "hot",
            Self::StopCopyStart => "stop_copy_start",
            Self::Unknown => "unknown",
        }
    }
}

/// Un repère d'une sauvegarde (BR-BACKUP-012).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupFlag {
    /// « À chaud ».
    Hot,
    /// « Peut être incohérente » : des fichiers ont changé pendant la copie, ou l'arrêt a été forcé.
    MaybeInconsistent,
    /// « Arrêt forcé avant la copie ».
    ForcedStop,
    /// « Avant restauration » (copie de sécurité, HRT-98).
    BeforeRestore,
    #[serde(other)]
    Unknown,
}

/// L'état d'intégrité d'une sauvegarde (BR-BACKUP-013).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupIntegrity {
    /// « Vérifiée le {date} » (`verified_at`).
    Verified,
    /// « Corrompue » : l'archive n'est plus celle que Hearth a écrite, ou un fichier ne correspond plus.
    Corrupt,
    /// Jamais vérifiée (la vérification de création n'a pas abouti).
    Unchecked,
    /// Le fichier de l'archive n'existe plus : la ligne du registre peut être retirée.
    Missing,
    #[serde(other)]
    Unknown,
}

/// Un élément d'un dossier de données du service : le dossier par sa cible dans le service, puis un chemin
/// relatif à ce dossier (`/` pour séparateur ; vide : le dossier entier).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IgnoredItem {
    pub root: String,
    #[serde(default)]
    pub path: String,
}

/// Pourquoi un élément est ignoré (BR-BACKUP-003).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IgnoredWhy {
    /// « Ressemble à des sauvegardes faites hors de Hearth » : le nom, jamais une preuve.
    LooksLikeBackups,
    /// Ajouté par un compte.
    Chosen,
    /// Ignoré pour cette seule sauvegarde.
    ThisTime,
    #[serde(other)]
    Unknown,
}

/// Un élément ignoré, avec sa raison.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IgnoredView {
    #[serde(flatten)]
    pub item: IgnoredItem,
    pub why: IgnoredWhy,
}

/// Une sauvegarde de la liste.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupView {
    pub id: String,
    /// Date de création, RFC 3339 en UTC.
    pub created_at: String,
    /// Taille de l'archive.
    pub archive_bytes: u64,
    /// Taille des données (somme des tailles des fichiers).
    pub data_bytes: u64,
    pub files: u64,
    pub mode: BackupMode,
    #[serde(default)]
    pub flags: Vec<BackupFlag>,
    /// Fichiers qui ont changé pendant la copie à chaud.
    #[serde(default)]
    pub changed_files: u32,
    /// La note du compte (80 caractères au plus) ; absente quand il n'y en a pas ou qu'elle ne s'ouvre plus.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Le compte qui l'a demandée (identifiant technique).
    pub account: String,
    /// `dépôt:version` de l'image du service au moment de la sauvegarde.
    #[serde(default)]
    pub image: String,
    pub integrity: BackupIntegrity,
    /// Date de la dernière vérification réussie, RFC 3339 en UTC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<String>,
}

/// Réponse de `GET /services/{key}/backups`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupsView {
    /// Les plus récentes d'abord.
    pub backups: Vec<BackupView>,
    /// Espace pris par les archives de ce service (BR-BACKUP-015).
    pub total_bytes: u64,
    /// Espace libre du disque des sauvegardes, absent si la mesure a échoué.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_bytes: Option<u64>,
    /// La racine des sauvegardes est sur le même disque que des données sauvegardées (BR-BACKUP-005).
    #[serde(default)]
    pub same_disk: bool,
    /// Les éléments ignorés ajoutés par un compte, puis les retraits des ignorés par défaut.
    pub settings: BackupSettings,
    /// Le service n'a aucun dossier de données sauvegardable (BR-BACKUP-002).
    #[serde(default)]
    pub nothing_to_save: bool,
}

/// Les réglages des éléments ignorés d'un service (BR-BACKUP-003) : ce que le compte ajoute, et ce qu'il
/// retire des ignorés par défaut (il les fait entrer dans les sauvegardes).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupSettings {
    #[serde(default)]
    pub ignored: Vec<IgnoredItem>,
    #[serde(default)]
    pub included: Vec<IgnoredItem>,
}

impl BackupSettings {
    /// Le nombre d'éléments et l'empreinte de la liste : ce que la preuve de clé de `BackupSettings` signe
    /// (jamais un chemin). Une seule fonction, lue par l'agent et par la liaison cliente.
    pub fn act_digest(&self) -> (u32, [u8; FINGERPRINT_LEN]) {
        let ignored = self
            .ignored
            .iter()
            .map(|item| format!("i\u{0}{}\u{0}{}", item.root, item.path));
        let included = self
            .included
            .iter()
            .map(|item| format!("n\u{0}{}\u{0}{}", item.root, item.path));
        let texts: Vec<String> = ignored.chain(included).collect();
        crate::admin_act::sorted_digest(texts.iter().map(String::as_str))
    }
}

/// Corps de `POST /services/{key}/backups`. Le membre `reauth` voyage à côté.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupRequest {
    /// Le mode choisi par le compte. Pour un service qui n'est pas en marche, le mode est toujours
    /// `stopped` (BR-BACKUP-006) ; `stop_copy_start` n'est accepté que pour un service en marche.
    pub mode: BackupMode,
    /// La note facultative, 80 caractères au plus.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Éléments ignorés POUR CETTE SAUVEGARDE seulement (un gros fichier, BR-BACKUP-033) : la liste du service
    /// ne change pas.
    #[serde(default)]
    pub ignore_once: Vec<IgnoredItem>,
}

/// Réponse de `POST /services/{key}/backups` (`202`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupAccepted {
    pub operation_id: String,
    pub backup_id: String,
}

/// Un gros fichier signalé (BR-BACKUP-033).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BigFile {
    #[serde(flatten)]
    pub item: IgnoredItem,
    pub bytes: u64,
}

/// Réponse de `POST /services/{key}/backups/preview` : de quoi bâtir la fenêtre de création. Aucune écriture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupPreview {
    pub name: String,
    /// Le service tourne : le compte choisit entre la copie à chaud et l'arrêt (BR-BACKUP-006).
    pub running: bool,
    /// Le délai d'arrêt propre du service, en secondes (BR-BACKUP-008).
    pub stop_grace_s: u32,
    /// Les dossiers du service qui seront sauvegardés (cible dans le service).
    pub roots: Vec<String>,
    /// Les dossiers du service qui ne le sont pas : « Pas sauvegardé : {chemin} n'est pas ouvert aux services ».
    pub not_saved: Vec<String>,
    /// Taille des éléments inclus, sans compter la compression.
    pub bytes: u64,
    pub files: u64,
    /// L'estimation a été coupée (60 secondes, 64 niveaux) : « au moins ».
    pub partial: bool,
    /// Les fichiers de plus de 1 Gio, les plus gros d'abord (20 au plus).
    pub big_files: Vec<BigFile>,
    /// Les éléments qui ne seront ni lus ni archivés (20 au plus).
    pub ignored: Vec<IgnoredView>,
    /// Combien d'éléments ignorés n'ont pas été nommés faute de place.
    #[serde(default)]
    pub ignored_more: u32,
    /// Espace libre du disque des sauvegardes, absent si la mesure a échoué.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_bytes: Option<u64>,
    /// La réserve que la sauvegarde ne mange jamais : le plus grand de 5 Gio et de 5 % du disque.
    #[serde(default)]
    pub reserve_bytes: u64,
    /// La sauvegarde tient au-dessus de la réserve (BR-BACKUP-009). Faux : aucune case ne permet de passer outre.
    pub enough_space: bool,
    pub same_disk: bool,
    /// Rien à sauvegarder : aucun dossier de données (BR-BACKUP-002).
    #[serde(default)]
    pub nothing_to_save: bool,
}

/// Réponse de `POST /backups/{id}/verify`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyResult {
    pub integrity: BackupIntegrity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<String>,
    /// Combien de fichiers ont été relus.
    #[serde(default)]
    pub files_checked: u64,
}

/// Réponse de `DELETE /backups/{id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupDeleted {
    /// L'archive et son manifeste ont été retirés du disque. Faux : ce n'était plus le fichier que Hearth
    /// avait écrit (ou il avait disparu) : seule la ligne du registre est partie, le fichier est intact
    /// (BR-BACKUP-013).
    pub removed_files: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_unknown_mode_flag_or_state_reads_as_unknown() {
        assert_eq!(
            serde_json::from_value::<BackupMode>(json!("teleported")).expect("lu"),
            BackupMode::Unknown
        );
        assert_eq!(
            serde_json::from_value::<BackupFlag>(json!("x")).expect("lu"),
            BackupFlag::Unknown
        );
        assert_eq!(
            serde_json::from_value::<BackupIntegrity>(json!("x")).expect("lu"),
            BackupIntegrity::Unknown
        );
    }

    #[test]
    fn a_request_reads_with_a_future_member_and_the_reauth_envelope() {
        let request: BackupRequest = serde_json::from_value(json!({
            "mode": "hot", "reauth": { "x": 1 }, "future": true
        }))
        .expect("lu");
        assert_eq!(request.mode, BackupMode::Hot);
        assert!(request.note.is_none() && request.ignore_once.is_empty());
    }

    #[test]
    fn the_wire_names_are_the_documented_ones() {
        assert_eq!(CAPABILITY_SERVICE_BACKUPS, "service-backups");
        assert_eq!(BackupMode::StopCopyStart.as_str(), "stop_copy_start");
        assert_eq!(
            serde_json::to_value(BackupFlag::MaybeInconsistent).expect("json"),
            json!("maybe_inconsistent")
        );
    }

    #[test]
    fn the_settings_digest_ignores_order_and_tells_ignored_from_included() {
        let a = IgnoredItem {
            root: "/data".into(),
            path: "logs".into(),
        };
        let b = IgnoredItem {
            root: "/data".into(),
            path: "cache".into(),
        };
        let one = BackupSettings {
            ignored: vec![a.clone(), b.clone()],
            included: vec![],
        };
        let two = BackupSettings {
            ignored: vec![b.clone(), a.clone()],
            included: vec![],
        };
        assert_eq!(one.act_digest(), two.act_digest());
        let moved = BackupSettings {
            ignored: vec![a],
            included: vec![b],
        };
        assert_ne!(one.act_digest(), moved.act_digest());
        assert_eq!(one.act_digest().0, 2);
    }
}
