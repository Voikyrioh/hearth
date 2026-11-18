//! Jugement d'un pack de mods déposé (HRT-124, BR-MOD-007, BR-MOD-008, BR-MOD-016, ADR-0055).
//!
//! Le fichier est HOSTILE : `domain::archive::inspect` le juge ENTIER (bornes, noms, types, sommes) avant que
//! `domain::mods::pack` ne reconnaisse son genre par son contenu. Hearth ne déballe rien : il lit au plus UN
//! fichier de description (1 Mio) et range l'archive telle quelle, en lecture seule, pour le programme du jeu.
//! Un pack refusé est retiré de la zone d'attente aussitôt. Aucune connexion sortante, aucun nom d'entrée dans
//! une erreur.

use std::sync::Arc;

use thiserror::Error;

use super::ports::{DepositError, DepositFile, DepositId, PackDeposits};
use crate::domain::archive::{self, ArchiveLimits, ArchiveRefusal, EntryKind, GIB, Inspection};
use crate::domain::mods::pack::{
    DescriptionError, DescriptionReader, PackEntry, PackRefusal, PackReport, judge_pack,
};

/// Taille maximale d'un pack (BR-MOD-008) : 4 Gio.
pub const MAX_PACK_BYTES: u64 = 4 * GIB;
/// Taille déballée maximale annoncée : un pack serveur de plusieurs Gio est normal, pas davantage.
pub const MAX_PACK_UNPACKED: u64 = 16 * GIB;

/// Le jugement d'un pack accepté : tout ce que la suite (écran, installation) a besoin de croire.
#[derive(Debug, Clone)]
pub struct JudgedPack {
    pub id: DepositId,
    pub report: PackReport,
    pub sha256: [u8; 32],
    /// Taille du fichier d'archive.
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PackError {
    /// Aucun dépôt de cet identifiant : jamais reçu, incomplet, consommé ou périmé.
    #[error("pack inconnu")]
    Unknown,
    /// L'archive n'est pas lisible en sécurité (motif de l'ADR-0055, jamais un nom d'entrée).
    #[error("archive refusée")]
    Archive(ArchiveRefusal),
    /// L'archive est lisible mais n'est pas un pack que Hearth installe (BR-MOD-008, BR-MOD-016).
    #[error("pack refusé")]
    Refused(PackRefusal),
    #[error("dépôt illisible")]
    Io,
}

impl PackError {
    /// Le code fixe du motif : sûr à journaliser et à montrer.
    pub fn reason(&self) -> &'static str {
        match self {
            Self::Unknown => "PACK_UNKNOWN",
            Self::Archive(_) => "ARCHIVE_REFUSED",
            Self::Refused(refusal) => refusal.code(),
            Self::Io => "PACK_UNREADABLE",
        }
    }
}

#[allow(clippy::field_reassign_with_default)] // `verify_data` n'est pas constructible hors du module `archive`
pub fn pack_limits() -> ArchiveLimits {
    let mut limits = ArchiveLimits::default();
    limits.max_file_size = MAX_PACK_BYTES;
    limits.max_unpacked = MAX_PACK_UNPACKED;
    limits
}

/// Les bornes de lecture d'un `.jar` de mod (HRT-254, BR-MOD-022) : 256 Mio le fichier, 1 Gio déballé, 65 536
/// entrées ; le reste (profondeur, taux de compression, vérification des sommes) est celui de l'ADR-0055.
#[allow(clippy::field_reassign_with_default)] // `verify_data` n'est pas constructible hors du module `archive`
pub fn jar_limits() -> ArchiveLimits {
    let mut limits = ArchiveLimits::default();
    limits.max_file_size = 256 << 20;
    limits.max_unpacked = 1 << 30;
    limits.max_entries = 65_536;
    limits
}

pub struct PackIntake {
    deposits: Arc<dyn PackDeposits>,
}

/// Les fichiers de description qu'un jugement peut demander (un par nom, à la racine du pack).
const DESCRIPTIONS: [&str; 5] = [
    "modrinth.index.json",
    "manifest.json",
    "variables.txt",
    "install.properties",
    ".fabric-installer/install.properties",
];

/// Les descriptions du pack, lues TOUTES en une passe avant le jugement : l'empreinte de l'archive n'est revérifiée
/// qu'une fois par jugement, pas une fois par fichier lu (HRT-225).
struct Descriptions(Vec<(&'static str, Result<Vec<u8>, ArchiveRefusal>)>);

impl Descriptions {
    fn read_all(
        file: &mut Box<dyn DepositFile>,
        inspection: &Inspection,
        limits: &ArchiveLimits,
    ) -> Result<Self, ArchiveRefusal> {
        let found = archive::read_many(file, inspection, &DESCRIPTIONS, limits, &|| false)?;
        Ok(Self(DESCRIPTIONS.into_iter().zip(found).collect()))
    }
}

impl DescriptionReader for Descriptions {
    fn read(&mut self, name: &str) -> Result<Vec<u8>, DescriptionError> {
        match self.0.iter().find(|(known, _)| *known == name) {
            Some((_, Ok(bytes))) => Ok(bytes.clone()),
            _ => Err(DescriptionError::Unreadable),
        }
    }
}

impl PackIntake {
    pub fn new(deposits: Arc<dyn PackDeposits>) -> Self {
        Self { deposits }
    }

    pub fn deposits(&self) -> &Arc<dyn PackDeposits> {
        &self.deposits
    }

    /// Juge le dépôt. BLOQUANT (lit tout le fichier) : l'appelant l'exécute hors du fil asynchrone. Un pack
    /// refusé, quelle qu'en soit la cause, est retiré de la zone d'attente avant de rendre l'erreur.
    pub fn judge(&self, deposit: &str, account: &str) -> Result<JudgedPack, PackError> {
        let id = DepositId::parse(deposit).ok_or(PackError::Unknown)?;
        let mut file = self
            .deposits
            .open(&id, account)
            .map_err(|error| match error {
                DepositError::NotFound => PackError::Unknown,
                DepositError::Changed | DepositError::Io => PackError::Io,
            })?;
        let judged = Self::judge_file(&id, &mut file);
        if matches!(judged, Err(PackError::Archive(_) | PackError::Refused(_))) {
            self.deposits.discard(&id, account);
        }
        judged
    }

    fn judge_file(
        id: &DepositId,
        file: &mut Box<dyn DepositFile>,
    ) -> Result<JudgedPack, PackError> {
        let limits = pack_limits();
        let inspection = archive::inspect(file, &limits, &|| false).map_err(PackError::Archive)?;
        let entries: Vec<PackEntry<'_>> = inspection
            .entries()
            .iter()
            .map(|entry| PackEntry {
                path: entry.path.as_str(),
                is_file: entry.kind == EntryKind::File,
                size: entry.size,
                crc32: entry.crc32,
            })
            .collect();
        let mut descriptions =
            Descriptions::read_all(file, &inspection, &limits).map_err(PackError::Archive)?;
        let report = judge_pack(&entries, &mut descriptions).map_err(PackError::Refused)?;
        let size = file_len(file)?;
        Ok(JudgedPack {
            id: id.clone(),
            report,
            sha256: *inspection.sha256(),
            size,
        })
    }
}

fn file_len(file: &mut Box<dyn DepositFile>) -> Result<u64, PackError> {
    use std::io::{Seek, SeekFrom};
    file.seek(SeekFrom::End(0)).map_err(|_| PackError::Io)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::io::Cursor;
    use std::sync::Mutex;

    use super::*;
    use crate::domain::archive::testkit::{ZEntry, Zip};
    use crate::domain::catalog::loading::schema::fiche::PackGenre;
    use crate::domain::files::RelPath;
    use crate::domain::services::ServiceKey;

    /// Une zone d'attente en mémoire : (compte, identifiant) vers les octets du dépôt.
    #[derive(Default)]
    struct FakeDeposits {
        files: Mutex<BTreeMap<(String, String), Vec<u8>>>,
        discarded: Mutex<Vec<String>>,
    }

    impl FakeDeposits {
        fn put(&self, account: &str, id: &str, bytes: Vec<u8>) {
            self.files
                .lock()
                .unwrap()
                .insert((account.into(), id.into()), bytes);
        }
    }

    impl PackDeposits for FakeDeposits {
        fn open(
            &self,
            id: &DepositId,
            account: &str,
        ) -> Result<Box<dyn DepositFile>, DepositError> {
            let files = self.files.lock().unwrap();
            let bytes = files
                .get(&(account.to_owned(), id.as_str().to_owned()))
                .ok_or(DepositError::NotFound)?;
            Ok(Box::new(Cursor::new(bytes.clone())))
        }

        fn place(
            &self,
            _: &DepositId,
            _: &str,
            _: &ServiceKey,
            _: &RelPath,
            _: &[u8; 32],
            _: crate::application::ports::Perms,
        ) -> Result<(), DepositError> {
            Err(DepositError::Io)
        }

        fn discard(&self, id: &DepositId, account: &str) {
            self.files
                .lock()
                .unwrap()
                .remove(&(account.to_owned(), id.as_str().to_owned()));
            self.discarded.lock().unwrap().push(id.as_str().to_owned());
        }
    }

    fn intake() -> (PackIntake, Arc<FakeDeposits>) {
        let deposits = Arc::new(FakeDeposits::default());
        (PackIntake::new(deposits.clone()), deposits)
    }

    fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        Zip::of(entries.iter().map(|(n, d)| ZEntry::file(n, d)).collect()).build()
    }

    #[test]
    fn a_server_pack_is_judged_by_its_content_whatever_it_is_called() {
        let (intake, deposits) = intake();
        deposits.put(
            "ada",
            "d1",
            zip(&[
                ("mods/a.jar", b"jar"),
                ("config/a.toml", b"x"),
                ("start.sh", b"#!/bin/sh"),
            ]),
        );
        let judged = intake.judge("d1", "ada").expect("pack serveur");
        assert_eq!(judged.report.genre, PackGenre::ServerPack);
        assert_eq!((judged.report.files, judged.report.mods), (3, 1));
        assert!(judged.size > 0);
        assert!(deposits.discarded.lock().unwrap().is_empty());
    }

    #[test]
    fn a_modrinth_pack_is_read_through_its_one_description_file() {
        let (intake, deposits) = intake();
        let index = br#"{"formatVersion":1,"game":"minecraft","versionId":"2","name":"Pack",
            "files":[{"path":"mods/a.jar","downloads":["https://cdn.modrinth.com/data/x/a.jar"]}],
            "dependencies":{"minecraft":"1.20.1","fabric-loader":"0.15.0"}}"#;
        deposits.put("ada", "d2", zip(&[("modrinth.index.json", index)]));
        let judged = intake.judge("d2", "ada").expect("pack modrinth");
        assert_eq!(judged.report.genre, PackGenre::ModrinthPack);
        assert_eq!(judged.report.modrinth.expect("résumé").downloads, 1);
    }

    #[test]
    fn a_curseforge_list_is_refused_and_removed_at_once() {
        let (intake, deposits) = intake();
        let manifest = br#"{"files":[{"projectID":1,"fileID":2},{"projectID":3,"fileID":4},{"projectID":5,"fileID":6}]}"#;
        deposits.put(
            "ada",
            "d3",
            zip(&[("manifest.json", manifest), ("overrides/a.txt", b"x")]),
        );
        let refused = intake.judge("d3", "ada").unwrap_err();
        assert_eq!(
            refused,
            PackError::Refused(PackRefusal::DownloadList { mods: 3 })
        );
        assert_eq!(refused.reason(), "PACK_IS_A_DOWNLOAD_LIST");
        assert_eq!(*deposits.discarded.lock().unwrap(), vec!["d3".to_owned()]);
        assert_eq!(intake.judge("d3", "ada").unwrap_err(), PackError::Unknown);
    }

    #[test]
    fn a_hostile_archive_is_refused_whole_by_the_archive_reader_and_removed() {
        let (intake, deposits) = intake();
        deposits.put(
            "ada",
            "d4",
            zip(&[("mods/a.jar", b"jar"), ("../evil.txt", b"x")]),
        );
        let refused = intake.judge("d4", "ada").unwrap_err();
        assert!(matches!(refused, PackError::Archive(_)), "{refused:?}");
        assert_eq!(refused.reason(), "ARCHIVE_REFUSED");
        assert_eq!(*deposits.discarded.lock().unwrap(), vec!["d4".to_owned()]);
    }

    #[test]
    fn something_that_is_not_an_archive_is_refused_and_removed() {
        let (intake, deposits) = intake();
        deposits.put("ada", "d5", b"ceci n'est pas une archive".to_vec());
        assert!(matches!(
            intake.judge("d5", "ada"),
            Err(PackError::Archive(_))
        ));
        assert_eq!(deposits.discarded.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_deposit_of_another_account_or_a_bad_id_is_unknown_and_never_removed() {
        let (intake, deposits) = intake();
        deposits.put("ada", "d6", zip(&[("mods/a.jar", b"jar")]));
        assert_eq!(intake.judge("d6", "bob").unwrap_err(), PackError::Unknown);
        for bad in ["", "../x", "d6.zip", "a b", &"x".repeat(65)] {
            assert_eq!(
                intake.judge(bad, "ada").unwrap_err(),
                PackError::Unknown,
                "{bad}"
            );
        }
        assert!(deposits.discarded.lock().unwrap().is_empty());
        assert!(intake.judge("d6", "ada").is_ok());
    }

    #[test]
    fn the_pack_size_limit_is_four_gib() {
        assert_eq!(pack_limits().max_file_size, 4 * GIB);
    }

    #[test]
    fn the_jar_limits_are_256_mib_one_gib_and_65536_entries() {
        let limits = jar_limits();
        assert_eq!(limits.max_file_size, 256 << 20);
        assert_eq!(limits.max_unpacked, GIB);
        assert_eq!(limits.max_entries, 65_536);
    }
}
