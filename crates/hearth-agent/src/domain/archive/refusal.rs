//! Refus typés (ADR-0055, `ARCHIVE_REFUSED { reason }`) : un code par cause.
//!
//! Aucune variante ne porte un nom d'entrée, un chemin du serveur ni un octet de contenu : le message
//! et le code sont sûrs à journaliser et à montrer tels quels. Seuls des nombres (bornes) y figurent.

use thiserror::Error;

/// Pourquoi une archive est refusée. Toute l'archive est refusée, jamais une entrée seule.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ArchiveRefusal {
    #[error("le fichier n'est pas une archive .zip, .tar ou .tar.gz que Hearth sait lire")]
    UnrecognizedFormat,
    #[error("l'archive dépasse {max} octets")]
    FileTooLarge { max: u64 },
    #[error("l'archive annonce trop d'entrées (au plus {max})")]
    TooManyEntries { max: usize },
    #[error("l'index de l'archive est trop gros (au plus {max} octets)")]
    IndexTooLarge { max: u64 },
    #[error("la table des matières de l'archive contredit son contenu")]
    IndexLies,
    #[error("l'archive porte plus d'un enregistrement de fin")]
    TwoEndRecords,
    #[error(
        "l'archive contient des octets qu'aucune entrée ne couvre (données préfixées ou cachées)"
    )]
    UncoveredBytes,
    #[error("deux entrées de l'archive se chevauchent")]
    OverlappingEntries,
    #[error("une entrée de l'archive pointe hors du fichier")]
    EntryOutsideFile,
    #[error("l'en-tête local d'une entrée contredit l'index de fin")]
    LocalHeaderMismatch,
    #[error("l'archive est en plusieurs volumes")]
    MultiVolume,
    #[error("une entrée de l'archive est chiffrée")]
    Encrypted,
    #[error("une entrée utilise une méthode de compression que Hearth ne prend pas en charge")]
    UnsupportedMethod,
    #[error("l'archive utilise une extension du format que Hearth ne prend pas en charge")]
    UnsupportedFeature,
    #[error("un nom est trop long (au plus {max} octets)")]
    NameTooLong { max: usize },
    #[error("un chemin est trop profond (au plus {max} niveaux)")]
    TooDeep { max: usize },
    #[error("un chemin est absolu")]
    AbsolutePath,
    #[error("un chemin commence par une lettre de lecteur ou contient « : »")]
    DriveLetter,
    #[error("un chemin sort de son dossier ou contient « . » ou « .. »")]
    ParentComponent,
    #[error("un chemin contient une barre oblique inverse")]
    Backslash,
    #[error("un chemin contient un composant vide")]
    EmptyComponent,
    #[error("un chemin contient un nom réservé de Windows, ou finit par un point ou une espace")]
    ReservedName,
    #[error(
        "un chemin contient un octet nul, un caractère de contrôle ou un caractère de format (sens d'écriture, largeur nulle, séparateur de ligne)"
    )]
    ControlCharacter,
    #[error(
        "Ce fichier contient des noms que Hearth ne sait pas lire. Recrée l'archive avec un outil récent."
    )]
    NonUtf8Name,
    #[error("deux entrées portent le même nom")]
    DuplicateName,
    #[error("deux entrées ne diffèrent que par la casse ou la forme Unicode de leur nom")]
    NameCollision,
    #[error("un fichier et un dossier portent le même nom")]
    FileDirConflict,
    #[error("l'archive contient un lien symbolique")]
    SymbolicLink,
    #[error("l'archive contient un lien physique")]
    HardLink,
    #[error("l'archive contient un fichier spécial (périphérique, tube ou prise)")]
    SpecialFile,
    #[error("une entrée se déballerait en plus de {ratio} fois sa taille compressée")]
    EntryRatioTooHigh { ratio: u64 },
    #[error("l'archive se déballerait en plus de {ratio} fois sa taille")]
    ArchiveRatioTooHigh { ratio: u64 },
    #[error("l'archive se déballerait en plus de {max} octets")]
    TotalSizeTooLarge { max: u64 },
    #[error("la taille d'une entrée n'est pas celle qu'annonce l'archive")]
    SizeMismatch,
    #[error("la somme de contrôle d'une entrée est fausse")]
    ChecksumMismatch,
    #[error("les données compressées de l'archive sont corrompues")]
    CorruptData,
    #[error("l'archive est tronquée")]
    Truncated,
    #[error("un en-tête de l'archive est invalide")]
    BadHeader,
    #[error("l'archive gzip contient plusieurs membres ou des octets après sa fin")]
    MultipleGzipMembers,
    #[error("le fichier d'archive a changé depuis son inspection")]
    ArchiveChanged,
    #[error("le fichier d'archive n'a pas pu être lu")]
    SourceUnreadable,
    #[error("l'écriture de l'extraction a échoué")]
    WriteFailed,
    #[error(
        "le dossier qui doit recevoir l'extraction n'est pas sûr (il doit appartenir à l'utilisateur de l'agent et n'être inscriptible que par lui)"
    )]
    UnsafeDestination,
    #[error(
        "ce système ne sait pas poser le dossier extrait sans remplacer un objet existant (renommage sans écrasement indisponible)"
    )]
    AtomicSwitchUnavailable,
    #[error("l'opération a été annulée")]
    Cancelled,
    #[error("le fichier demandé n'est pas dans l'archive")]
    EntryNotFound,
    #[error("le fichier demandé dépasse {max} octets")]
    EntryTooLargeToRead { max: u64 },
}

impl ArchiveRefusal {
    /// Code stable, en majuscules, un par cause (variante).
    pub fn code(&self) -> &'static str {
        use ArchiveRefusal::*;
        match self {
            UnrecognizedFormat => "UNRECOGNIZED_FORMAT",
            FileTooLarge { .. } => "FILE_TOO_LARGE",
            TooManyEntries { .. } => "TOO_MANY_ENTRIES",
            IndexTooLarge { .. } => "INDEX_TOO_LARGE",
            IndexLies => "INDEX_LIES",
            TwoEndRecords => "TWO_END_RECORDS",
            UncoveredBytes => "UNCOVERED_BYTES",
            OverlappingEntries => "OVERLAPPING_ENTRIES",
            EntryOutsideFile => "ENTRY_OUTSIDE_FILE",
            LocalHeaderMismatch => "LOCAL_HEADER_MISMATCH",
            MultiVolume => "MULTI_VOLUME",
            Encrypted => "ENCRYPTED",
            UnsupportedMethod => "UNSUPPORTED_METHOD",
            UnsupportedFeature => "UNSUPPORTED_FEATURE",
            NameTooLong { .. } => "NAME_TOO_LONG",
            TooDeep { .. } => "TOO_DEEP",
            AbsolutePath => "ABSOLUTE_PATH",
            DriveLetter => "DRIVE_LETTER",
            ParentComponent => "PARENT_COMPONENT",
            Backslash => "BACKSLASH",
            EmptyComponent => "EMPTY_COMPONENT",
            ReservedName => "RESERVED_NAME",
            ControlCharacter => "CONTROL_CHARACTER",
            NonUtf8Name => "NON_UTF8_NAME",
            DuplicateName => "DUPLICATE_NAME",
            NameCollision => "NAME_COLLISION",
            FileDirConflict => "FILE_DIR_CONFLICT",
            SymbolicLink => "SYMBOLIC_LINK",
            HardLink => "HARD_LINK",
            SpecialFile => "SPECIAL_FILE",
            EntryRatioTooHigh { .. } => "ENTRY_RATIO_TOO_HIGH",
            ArchiveRatioTooHigh { .. } => "ARCHIVE_RATIO_TOO_HIGH",
            TotalSizeTooLarge { .. } => "TOTAL_SIZE_TOO_LARGE",
            SizeMismatch => "SIZE_MISMATCH",
            ChecksumMismatch => "CHECKSUM_MISMATCH",
            CorruptData => "CORRUPT_DATA",
            Truncated => "TRUNCATED",
            BadHeader => "BAD_HEADER",
            MultipleGzipMembers => "MULTIPLE_GZIP_MEMBERS",
            ArchiveChanged => "ARCHIVE_CHANGED",
            SourceUnreadable => "SOURCE_UNREADABLE",
            WriteFailed => "WRITE_FAILED",
            UnsafeDestination => "UNSAFE_DESTINATION",
            AtomicSwitchUnavailable => "ATOMIC_SWITCH_UNAVAILABLE",
            Cancelled => "CANCELLED",
            EntryNotFound => "ENTRY_NOT_FOUND",
            EntryTooLargeToRead { .. } => "ENTRY_TOO_LARGE_TO_READ",
        }
    }

    /// Ce refus vient-il de l'archive (hostile ou abîmée) plutôt que de la machine ou de l'utilisateur ?
    pub fn is_about_the_archive(&self) -> bool {
        !matches!(
            self,
            ArchiveRefusal::SourceUnreadable
                | ArchiveRefusal::WriteFailed
                | ArchiveRefusal::UnsafeDestination
                | ArchiveRefusal::AtomicSwitchUnavailable
                | ArchiveRefusal::Cancelled
                | ArchiveRefusal::EntryNotFound
                | ArchiveRefusal::ArchiveChanged
        )
    }

    /// Traduit une erreur d'entrée-sortie levée en lisant l'archive : un refus que nos lecteurs ont
    /// glissé dans l'erreur est restitué tel quel, une fin de fichier est « tronquée », le reste est
    /// une source illisible. Le texte du système n'est jamais repris (il peut citer un chemin).
    pub(crate) fn from_io(error: std::io::Error) -> ArchiveRefusal {
        if error
            .get_ref()
            .is_some_and(|inner| inner.is::<ArchiveRefusal>())
        {
            if let Some(inner) = error.into_inner()
                && let Ok(refusal) = inner.downcast::<ArchiveRefusal>()
            {
                return *refusal;
            }
            return ArchiveRefusal::SourceUnreadable;
        }
        match error.kind() {
            std::io::ErrorKind::UnexpectedEof => ArchiveRefusal::Truncated,
            std::io::ErrorKind::InvalidData | std::io::ErrorKind::InvalidInput => {
                ArchiveRefusal::CorruptData
            }
            _ => ArchiveRefusal::SourceUnreadable,
        }
    }

    pub(crate) fn into_io(self) -> std::io::Error {
        std::io::Error::other(self)
    }
}
