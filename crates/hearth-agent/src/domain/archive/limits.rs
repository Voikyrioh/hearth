//! Bornes de la lecture d'une archive (ADR-0055 point 3 et 4). Toute valeur lue dans le fichier est
//! fausse par hypothèse : on la compare à ces bornes AVANT de la laisser dimensionner quoi que ce soit.

/// Un kibioctet, un mébioctet, un gibioctet.
pub const KIB: u64 = 1024;
pub const MIB: u64 = 1024 * KIB;
pub const GIB: u64 = 1024 * MIB;

/// Taille d'un tampon de flux (ADR-0055 point 13) : la mémoire de travail ne dépend d'aucun nombre lu.
pub const BUFFER: usize = 64 * 1024;

/// Plus petite taille d'un enregistrement d'index de fin d'un zip (46 octets, sans son nom).
pub const ZIP_INDEX_RECORD_MIN: u64 = 46;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveLimits {
    /// Taille du fichier d'archive (lue par la position de fin, jamais par un en-tête).
    pub max_file_size: u64,
    pub max_entries: usize,
    /// Index de fin du zip ; pour un tar, total des octets de noms.
    pub max_index_bytes: u64,
    pub max_name_bytes: usize,
    pub max_component_bytes: usize,
    pub max_depth: usize,
    /// Taille déballée totale annoncée. L'appelant la ramène à l'espace libre moins la réserve.
    pub max_unpacked: u64,
    /// Au-delà de `ratio_floor` octets déballés, une entrée ne peut pas dépasser ce taux.
    pub max_entry_ratio: u64,
    pub max_archive_ratio: u64,
    pub ratio_floor: u64,
    /// Noeuds distincts (fichiers, dossiers, dossiers implicites) retenus pour juger les doublons.
    pub max_tree_nodes: usize,
    /// « Lire un seul fichier » (ADR-0055 point 12).
    pub max_single_read: u64,
    /// Relire chaque entrée à l'inspection (tailles et sommes de contrôle réelles) : l'archive est alors
    /// jugée sur ce qu'elle contient et non sur ce qu'elle annonce. Visible du seul module `archive` et de
    /// ses tests (`pub(super)`) : aucun appelant ne peut le couper, ni le lire, ni construire des bornes
    /// par un littéral de structure hors du module ; il ajuste les champs publics d'un
    /// `ArchiveLimits::default()`.
    ///
    /// ```compile_fail,E0451
    /// use hearth_agent::domain::archive::ArchiveLimits;
    /// let _ = ArchiveLimits { verify_data: false, ..ArchiveLimits::default() };
    /// ```
    ///
    /// ```compile_fail,E0616
    /// use hearth_agent::domain::archive::ArchiveLimits;
    /// let mut limits = ArchiveLimits::default();
    /// limits.verify_data = false;
    /// ```
    pub(super) verify_data: bool,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            max_file_size: 8 * GIB,
            max_entries: 200_000,
            max_index_bytes: 64 * MIB,
            max_name_bytes: 1024,
            max_component_bytes: 255,
            max_depth: 32,
            max_unpacked: 64 * GIB,
            max_entry_ratio: 200,
            max_archive_ratio: 100,
            ratio_floor: MIB,
            max_tree_nodes: 1_000_000,
            max_single_read: MIB,
            verify_data: true,
        }
    }
}

impl ArchiveLimits {
    /// Les bornes par défaut, avec la taille déballée ramenée à ce que le disque peut recevoir.
    pub fn for_free_space(free: u64, reserve: u64) -> Self {
        let defaults = Self::default();
        Self {
            max_unpacked: defaults.max_unpacked.min(free.saturating_sub(reserve)),
            ..defaults
        }
    }
}
