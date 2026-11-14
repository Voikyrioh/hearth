//! Sauvegarder les données d'un service créé par Hearth : créer, estimer, lister, vérifier, supprimer, régler
//! les éléments ignorés (HRT-97, ADR-0047, ADR-0045, BR-BACKUP-*).
//!
//! **Ce que fait [`Backups::create`], de façon SYNCHRONE** (rien n'est attendu du moteur au-delà d'une
//! lecture) : la racine des sauvegardes est utilisable et ne chevauche aucun dossier de données
//! (`BR-BACKUP-004` (b)), le service existe, est créé par Hearth et n'est tenu par aucune autre action, son
//! état et le mode choisi s'accordent (`BR-BACKUP-006`), il a des dossiers à sauvegarder (`BR-BACKUP-002`),
//! l'espace suffit (`BR-BACKUP-009`, estimation bornée à 60 s). Puis l'opération `backup` est acceptée (verrou
//! exclusif du service ET place des opérations lourdes du serveur, `BR-BACKUP-010`, `BR-BACKUP-029`) et
//! l'exécution part en tâche de fond : `202 { operation_id, backup_id }`.
//!
//! **L'exécution** suit le plan figé (`domain::ops::backup`) :
//! 1. `stop` (seulement « Arrêter, sauvegarder, relancer ») : l'arrêt voulu est écrit AVANT l'ordre, le délai de
//!    grâce du service est tenu, une coupure forcée est dite (`BR-BACKUP-008`) ;
//! 2. `copy` : l'archive `.part` est créée (exclusive), inscrite au registre `writing` et au journal comme objet
//!    CRÉÉ par l'opération (retiré au redémarrage après vérification de l'inode, `BR-BACKUP-025`), puis les
//!    fichiers sont copiés en flux sur un fil à part (`fstat` avant et après chaque fichier, jamais un fichier en
//!    mémoire, jamais un lien suivi). Elle s'arrête, et sa partie écrite est supprimée, sur annulation, sous la
//!    moitié de la réserve de disque, ou au bout de 6 heures. Puis le manifeste voisin est écrit, l'archive est
//!    publiée sous son nom définitif et la ligne devient `ready` ;
//! 3. `verify` : l'archive est relue et jugée contre le manifeste ; échec : Hearth supprime SES fichiers et dit
//!    l'échec (`BR-BACKUP-013`) ;
//! 4. `start` (seulement ce mode, et seulement si le service tournait) : relancé même si la sauvegarde a échoué
//!    ou a été annulée ; s'il ne repart pas, il reste « Arrêté » et la cause est dite.
//!
//! **Jamais** : un chemin, un nom de fichier ou la note du compte dans une trace, une erreur ou le journal
//! d'activité (`BR-BACKUP-032`) ; un fichier de la racine des sauvegardes touché sans être inscrit au registre
//! avec son inode et l'empreinte de son manifeste (`BR-BACKUP-004`) ; une sauvegarde supprimée automatiquement
//! (`BR-BACKUP-015`).

use std::collections::HashSet;
use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use hearth_proto::api::backups::{
    BIG_FILE_BYTES, BackupAccepted, BackupDeleted, BackupFlag, BackupIntegrity, BackupMode,
    BackupPreview, BackupRequest, BackupSettings, BackupView, BackupsView, BigFile,
    ESTIMATE_BUDGET_S, IgnoredItem, IgnoredView, IgnoredWhy, MAX_ITEMS, MAX_LISTED, NOTE_MAX_CHARS,
    VerifyResult,
};
use thiserror::Error;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::delete::{StepShown, account_of, cause_of};
use super::ports::{
    AuditSink, BackupFiles, BackupFsError, BackupRemoved, BackupRepo, BackupRow, BackupSink, Clock,
    ContainerEngine, DiskSpace, EngineError, EntryKind, EntryMeta, FsError, HostPaths, IdGen,
    NewBackup, Progress, Published, ReadFile, Resolved, SafeFs, ServiceContainers, ServiceIntents,
    ServiceRoot, StopIntent,
};
use super::service_lifecycle::reason_of;
use super::service_ops::{AcceptRequest, AuditDraft, OpError, ServiceOps};
use super::service_specs::ServiceSpecs;
use super::services::{Driver, ServicesRegistry};
use crate::domain::audit::{
    Actor, AuditAction, BackupModeNote, BackupNote, ClientName, Outcome, Reason, Target,
};
use crate::domain::backups::ignore::IgnoreRules;
use crate::domain::backups::manifest::{
    ArchiveFacts, Entry, EntryType, FORMAT, MAX_MANIFEST_BYTES, Manifest, ManifestIgnored,
    ManifestRoot, tar_path,
};
use crate::domain::backups::overlap::{PathFacts, overlaps};
use crate::domain::backups::ownership::{Ownership, Registered, judge_archive, manifest_is_ours};
use crate::domain::backups::space::{
    CHECK_EVERY_BYTES, MAX_COPY, SpaceVerdict, judge, too_low_during_copy,
};
use crate::domain::backups::{BackupId, archive_dir_name};
use crate::domain::files::{MAX_DEPTH, RelPath};
use crate::domain::ops::CreatedObject;
use crate::domain::ops::backup::{ModeVerdict, effective_mode, op_plan, step};
use crate::domain::ops::{
    Cause, Holder, ObjectKind, ObjectRoot, OpKind, OpStatus, Refusal, StopEvidence, UndoInfo,
    WAIT_FLOOR, WAIT_MARGIN, stop_was_forced, wait_budget_with,
};
use crate::domain::sealed::{AnswerId, SealContext, SecretField};
use crate::domain::services::delete::{ManagedFolder, split_mounts};
use crate::domain::services::plan::image_reference;
use crate::domain::services::spec::{SpecBody, SpecMountKind};
use crate::domain::services::{
    EngineState, KeyKind, RunState, ServiceKey, ServiceStateKind, TransitionKind,
};
use crate::infrastructure::backup_archive::{
    ArchiveEnd, ArchiveWriter, VerifyError, sha256_hex, verify_archive,
};

/// Les durées d'une sauvegarde. Les tests les raccourcissent.
#[derive(Debug, Clone, Copy)]
pub struct BackupTimings {
    /// L'estimation de la taille (`BR-BACKUP-009`) : 60 secondes au plus.
    pub estimate_budget: Duration,
    /// L'intervalle entre deux signes de vie de la copie.
    pub progress_every: Duration,
    /// La marge et le plancher de l'attente d'un arrêt (`BR-LIFE-021`).
    pub stop_margin: Duration,
    pub stop_floor: Duration,
    /// Combien attendre au plus le démarrage du conteneur.
    pub start_wait: Duration,
    /// Une copie dure 6 heures au plus (`BR-BACKUP-010`).
    pub max_copy: Duration,
    /// Un fichier de cette taille ou plus est nommé à la fenêtre de création (`BR-BACKUP-033` : 1 Gio).
    pub big_file_bytes: u64,
}

impl BackupTimings {
    pub const fn production() -> Self {
        Self {
            estimate_budget: Duration::from_secs(ESTIMATE_BUDGET_S),
            progress_every: Duration::from_millis(500),
            stop_margin: WAIT_MARGIN,
            stop_floor: WAIT_FLOOR,
            start_wait: Duration::from_secs(60),
            max_copy: MAX_COPY,
            big_file_bytes: BIG_FILE_BYTES,
        }
    }
}

/// Ce dont le cas d'usage a besoin de la machine.
pub struct BackupContext {
    /// `<données de l'agent>/services`, sous lui un sous-dossier par service.
    pub services_root: String,
    /// La racine des sauvegardes (`backups.root`).
    pub backups_root: String,
    pub agent_version: String,
    pub timings: BackupTimings,
}

/// Pourquoi une demande est refusée tout de suite.
#[derive(Debug, Error)]
pub enum BackupError {
    #[error("le moteur n'est pas prêt")]
    EngineUnavailable(EngineState),
    #[error("service inconnu")]
    NotFound,
    /// Un service créé hors de Hearth n'a de sauvegarde que sous un dossier ouvert aux services (`BR-BACKUP-028`) :
    /// cet agent n'en ouvre aucun.
    #[error("service adopté")]
    Adopted,
    #[error("une action est déjà en cours")]
    Busy { by: String, kind: OpKind },
    /// Une sauvegarde, une restauration ou une reprise de monde tient la place (`BR-BACKUP-010`).
    #[error("une opération lourde est en cours")]
    HeavyBusy { service: String },
    #[error("état incompatible")]
    NotBackable(ServiceStateKind),
    #[error("mode refusé")]
    ModeRefused,
    #[error("champ invalide")]
    Invalid {
        field: &'static str,
        reason: &'static str,
    },
    #[error("rien à sauvegarder")]
    NothingToSave,
    #[error("espace insuffisant")]
    NoSpace {
        needed: u64,
        free: u64,
        reserve: u64,
    },
    /// La racine des sauvegardes chevauche un dossier de données (`BR-BACKUP-004` (b)).
    #[error("la racine des sauvegardes chevauche un dossier de données")]
    RootOverlap,
    /// La racine des sauvegardes n'est pas utilisable (plate-forme, droits, lien).
    #[error("sauvegardes indisponibles")]
    Unavailable,
    #[error("sauvegarde inconnue")]
    BackupNotFound,
    #[error("sauvegarde en cours d'utilisation")]
    InUse,
    #[error("la clé des secrets est illisible")]
    KeyUnavailable,
    #[error("erreur interne : {0}")]
    Internal(String),
}

/// Un refus, avec le nom du service quand il est publié (il va au journal d'activité).
#[derive(Debug)]
pub struct BackupFailed {
    pub error: BackupError,
    pub name: Option<String>,
}

impl BackupFailed {
    fn plain(error: BackupError) -> Self {
        Self { error, name: None }
    }

    fn named(error: BackupError, name: &str) -> Self {
        Self {
            error,
            name: Some(name.to_owned()),
        }
    }

    pub fn not_found() -> Self {
        Self::plain(BackupError::NotFound)
    }
}

fn internal(error: impl std::fmt::Display) -> BackupFailed {
    BackupFailed::plain(BackupError::Internal(error.to_string()))
}

// ---------------------------------------------------------------------------------------------
// L'estimation et la copie, sur un fil à part
// ---------------------------------------------------------------------------------------------

/// Ce que l'estimation a vu.
#[derive(Debug, Default)]
struct Estimate {
    bytes: u64,
    files: u64,
    partial: bool,
    big: Vec<BigFile>,
    ignored: Vec<IgnoredView>,
    ignored_more: u32,
}

fn join(base: &RelPath, name: &str) -> Option<RelPath> {
    if base.is_root() {
        RelPath::parse(name).ok()
    } else {
        RelPath::parse(&format!("{base}/{name}")).ok()
    }
}

fn within_path(within: &str, name: &str) -> String {
    if within.is_empty() {
        name.to_owned()
    } else {
        format!("{within}/{name}")
    }
}

/// Parcourt les dossiers du service comme la copie le fera, sans rien lire des fichiers : de quoi dire combien
/// il faut d'espace, quels fichiers sont énormes, ce qui est ignoré.
fn estimate_blocking(
    fs: &dyn SafeFs,
    key: &ServiceKey,
    folders: &[ManagedFolder],
    rules: &IgnoreRules,
    budget: Duration,
    big_file_bytes: u64,
) -> Result<Estimate, FsError> {
    let root = fs.open_service_root(key)?;
    let deadline = Instant::now().checked_add(budget);
    let mut estimate = Estimate::default();
    let mut visited = 0_u64;
    for folder in folders {
        if let Some(why) = rules.root_ignored(&folder.target) {
            note_ignored(&mut estimate, &folder.target, "", why);
            continue;
        }
        let mut stack = vec![(folder.rel.clone(), String::new(), 1_usize)];
        while let Some((rel, within, depth)) = stack.pop() {
            let listing = match root.list_dir(&rel) {
                Ok(listing) => listing,
                // Disparu entre-temps ou devenu autre chose : rien à compter.
                Err(FsError::NotFound | FsError::Blocked | FsError::NotRegular) => continue,
                Err(error) => return Err(error),
            };
            for entry in listing.entries {
                visited += 1;
                if visited.is_multiple_of(1024)
                    && deadline.is_some_and(|limit| Instant::now() >= limit)
                {
                    estimate.partial = true;
                    return Ok(estimate);
                }
                let path = within_path(&within, &entry.name);
                if let Some(why) = rules.judge(&folder.target, &path, &entry.name) {
                    note_ignored(&mut estimate, &folder.target, &path, why);
                    continue;
                }
                match entry.meta.kind {
                    EntryKind::File => {
                        estimate.files += 1;
                        estimate.bytes = estimate.bytes.saturating_add(entry.meta.size);
                        if entry.meta.size >= big_file_bytes {
                            estimate.big.push(BigFile {
                                item: IgnoredItem {
                                    root: folder.target.clone(),
                                    path,
                                },
                                bytes: entry.meta.size,
                            });
                        }
                    }
                    EntryKind::Dir => {
                        if depth >= MAX_DEPTH {
                            estimate.partial = true;
                        } else if let Some(child) = join(&rel, &entry.name) {
                            stack.push((child, path, depth + 1));
                        }
                    }
                    EntryKind::Symlink | EntryKind::Other => {}
                }
            }
        }
    }
    estimate
        .big
        .sort_by_key(|file| std::cmp::Reverse(file.bytes));
    estimate.big.truncate(MAX_LISTED);
    Ok(estimate)
}

fn note_ignored(estimate: &mut Estimate, root: &str, path: &str, why: IgnoredWhy) {
    if estimate.ignored.len() >= MAX_LISTED {
        estimate.ignored_more = estimate.ignored_more.saturating_add(1);
        return;
    }
    estimate.ignored.push(IgnoredView {
        item: IgnoredItem {
            root: root.to_owned(),
            path: path.to_owned(),
        },
        why,
    });
}

/// Pourquoi la copie s'est arrêtée avant la fin.
const STOP_NONE: u8 = 0;
const STOP_CANCEL: u8 = 1;
const STOP_SHUTDOWN: u8 = 2;
const STOP_TIMEOUT: u8 = 3;
const STOP_DISK: u8 = 4;

/// Les compteurs et les demandes d'arrêt partagés entre la copie (un fil) et son suivi (une tâche).
struct Control {
    stop: AtomicU8,
    files: AtomicU64,
    bytes: AtomicU64,
    changed: AtomicU32,
    deadline: Instant,
}

impl Control {
    fn ask(&self, reason: u8) {
        let _ = self
            .stop
            .compare_exchange(STOP_NONE, reason, Ordering::SeqCst, Ordering::SeqCst);
    }

    fn reason(&self) -> u8 {
        self.stop.load(Ordering::SeqCst)
    }
}

/// Un fichier lu pour l'archive : compte les octets, relit les demandes d'arrêt à chaque lecture et l'espace du
/// disque tous les 256 Mio lus (un seul fichier peut peser des dizaines de Gio).
struct Guarded<'a> {
    inner: &'a mut dyn ReadFile,
    control: &'a Control,
    files: &'a dyn BackupFiles,
    since_check: u64,
}

impl Read for Guarded<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.control.reason() != STOP_NONE {
            return Err(io::Error::other("copy stopped"));
        }
        if Instant::now() >= self.control.deadline {
            self.control.ask(STOP_TIMEOUT);
            return Err(io::Error::other("copy stopped"));
        }
        let n = self.inner.read(buf)?;
        let n64 = u64::try_from(n).unwrap_or(0);
        self.control.bytes.fetch_add(n64, Ordering::Relaxed);
        self.since_check += n64;
        if self.since_check >= CHECK_EVERY_BYTES {
            self.since_check = 0;
            if disk_low(self.files) {
                self.control.ask(STOP_DISK);
                return Err(io::Error::other("copy stopped"));
            }
        }
        Ok(n)
    }
}

/// L'espace libre est-il tombé sous la moitié de la réserve ? Une mesure qui échoue ne dit rien : la copie continue.
fn disk_low(files: &dyn BackupFiles) -> bool {
    files
        .space()
        .is_ok_and(|space| too_low_during_copy(space.free, space.total))
}

/// L'issue d'une copie.
enum CopyOutcome {
    Done {
        manifest: Box<Manifest>,
        end: ArchiveEnd,
        /// Les fichiers qui ont changé pendant la copie (ceux qui ont disparu en route compris).
        changed: u32,
    },
    /// Arrêtée avant la fin : `STOP_*`.
    Stopped(u8),
    Failed(Cause),
}

struct CopyJob {
    fs: Arc<dyn SafeFs>,
    files: Arc<dyn BackupFiles>,
    key: ServiceKey,
    folders: Vec<ManagedFolder>,
    rules: IgnoreRules,
    sink: Box<dyn BackupSink>,
    control: Arc<Control>,
    /// Le manifeste sans ses entrées : tout ce qui se sait avant la copie.
    base: Manifest,
    forced_stop: bool,
    hot: bool,
}

struct Walk<'a> {
    root: &'a dyn ServiceRoot,
    writer: ArchiveWriter,
    control: &'a Control,
    files: &'a dyn BackupFiles,
    entries: Vec<Entry>,
    ignored: Vec<ManifestIgnored>,
}

/// Pourquoi la marche s'arrête : une demande d'arrêt, ou une erreur.
enum Halt {
    Stop,
    Fail(Cause),
}

fn cause_of_io(error: &io::Error) -> Cause {
    match error.kind() {
        io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => Cause::DiskAlmostFull,
        _ => Cause::StepFailed,
    }
}

fn entry_from(root: u32, path: &str, kind: EntryType, meta: &EntryMeta) -> Entry {
    Entry {
        root,
        path: path.to_owned(),
        kind,
        size: if kind == EntryType::File {
            meta.size
        } else {
            0
        },
        sha256: None,
        mode: meta.mode,
        uid: meta.uid,
        gid: meta.gid,
        mtime_ns: meta.mtime_ns,
        nlink: meta.nlink,
        link_target: None,
        changed: false,
    }
}

impl Walk<'_> {
    fn check(&self) -> Result<(), Halt> {
        if self.control.reason() != STOP_NONE {
            return Err(Halt::Stop);
        }
        if Instant::now() >= self.control.deadline {
            self.control.ask(STOP_TIMEOUT);
            return Err(Halt::Stop);
        }
        Ok(())
    }

    /// Un dossier du service : son entrée, puis tout ce qu'il contient.
    fn folder(
        &mut self,
        id: u32,
        dir: &str,
        folder: &ManagedFolder,
        rules: &IgnoreRules,
    ) -> Result<(), Halt> {
        let meta = match self.root.stat(&folder.rel) {
            Ok(meta) if meta.kind == EntryKind::Dir => meta,
            // Le dossier n'existe pas (encore) ou n'est plus un dossier : rien à sauvegarder dedans.
            Ok(_) | Err(FsError::NotFound | FsError::Blocked | FsError::NotRegular) => {
                return Ok(());
            }
            Err(_) => return Err(Halt::Fail(Cause::StepFailed)),
        };
        self.writer
            .dir(dir, &meta)
            .map_err(|e| Halt::Fail(cause_of_io(&e)))?;
        self.entries.push(entry_from(id, "", EntryType::Dir, &meta));
        self.dir(id, dir, folder, &folder.rel, "", 1, rules)
    }

    #[allow(clippy::too_many_arguments)]
    fn dir(
        &mut self,
        id: u32,
        dir_name: &str,
        folder: &ManagedFolder,
        rel: &RelPath,
        within: &str,
        depth: usize,
        rules: &IgnoreRules,
    ) -> Result<(), Halt> {
        let listing = match self.root.list_dir(rel) {
            Ok(listing) => listing,
            Err(FsError::NotFound | FsError::Blocked | FsError::NotRegular) => return Ok(()),
            Err(_) => return Err(Halt::Fail(Cause::StepFailed)),
        };
        for entry in listing.entries {
            self.check()?;
            let path = within_path(within, &entry.name);
            if rules.judge(&folder.target, &path, &entry.name).is_some() {
                self.ignored.push(ManifestIgnored { root: id, path });
                continue;
            }
            let Some(child) = join(rel, &entry.name) else {
                continue;
            };
            let tar = tar_path(dir_name, &path);
            match entry.meta.kind {
                EntryKind::Dir => {
                    if depth >= MAX_DEPTH {
                        continue;
                    }
                    self.writer
                        .dir(&tar, &entry.meta)
                        .map_err(|e| Halt::Fail(cause_of_io(&e)))?;
                    self.entries
                        .push(entry_from(id, &path, EntryType::Dir, &entry.meta));
                    self.dir(id, dir_name, folder, &child, &path, depth + 1, rules)?;
                }
                EntryKind::File => self.file(id, &tar, &path, &child)?,
                EntryKind::Symlink => {
                    let target = match self.root.read_link(&child) {
                        Ok(target) if !target.is_empty() => target,
                        // Disparu, remplacé ou illisible : pas archivé, jamais suivi.
                        _ => continue,
                    };
                    self.writer
                        .symlink(&tar, &entry.meta, &target)
                        .map_err(|e| Halt::Fail(cause_of_io(&e)))?;
                    let mut item = entry_from(id, &path, EntryType::Symlink, &entry.meta);
                    item.link_target = Some(target);
                    self.entries.push(item);
                }
                // Un tube, un socket, un périphérique, un autre système de fichiers : ni lu ni archivé.
                EntryKind::Other => {}
            }
        }
        Ok(())
    }

    /// Un fichier : `fstat` avant, lecture en flux, `fstat` après sur le MÊME descripteur (BR-BACKUP-007).
    fn file(&mut self, id: u32, tar: &str, path: &str, rel: &RelPath) -> Result<(), Halt> {
        let mut file = match self.root.open_read(rel) {
            Ok(file) => file,
            // Disparu en route : noté et compté comme changé (copie à chaud).
            Err(FsError::NotFound) => {
                self.control.changed.fetch_add(1, Ordering::Relaxed);
                return Ok(());
            }
            // Devenu un lien, un tube, un dossier entre le listage et l'ouverture : jamais lu.
            Err(FsError::Blocked | FsError::NotRegular | FsError::CrossesDevice) => {
                return Ok(());
            }
            Err(_) => return Err(Halt::Fail(Cause::StepFailed)),
        };
        let before = file.meta().map_err(|_| Halt::Fail(Cause::StepFailed))?;
        let digest = {
            let mut guarded = Guarded {
                inner: file.as_mut(),
                control: self.control,
                files: self.files,
                since_check: 0,
            };
            self.writer.file(tar, &before, &mut guarded)
        };
        let digest = match digest {
            Ok(digest) => digest,
            Err(error) => {
                return Err(if self.control.reason() != STOP_NONE {
                    Halt::Stop
                } else {
                    Halt::Fail(cause_of_io(&error))
                });
            }
        };
        let after = file.meta().map_err(|_| Halt::Fail(Cause::StepFailed))?;
        let changed =
            digest.short || after.size != before.size || after.mtime_ns != before.mtime_ns;
        if changed {
            self.control.changed.fetch_add(1, Ordering::Relaxed);
        }
        self.control.files.fetch_add(1, Ordering::Relaxed);
        let mut item = entry_from(id, path, EntryType::File, &before);
        item.sha256 = Some(digest.sha256);
        item.changed = changed;
        self.entries.push(item);
        Ok(())
    }
}

fn copy_blocking(job: CopyJob) -> CopyOutcome {
    let CopyJob {
        fs,
        files,
        key,
        folders,
        rules,
        sink,
        control,
        mut base,
        forced_stop,
        hot,
    } = job;
    let root = match fs.open_service_root(&key) {
        Ok(root) => root,
        Err(_) => return CopyOutcome::Failed(Cause::StepFailed),
    };
    let (writer, _written) = ArchiveWriter::new(sink);
    let mut walk = Walk {
        root: root.as_ref(),
        writer,
        control: &control,
        files: files.as_ref(),
        entries: Vec::new(),
        ignored: Vec::new(),
    };
    let mut taken: Vec<String> = Vec::new();
    for (index, folder) in folders.iter().enumerate() {
        let id = u32::try_from(index).unwrap_or(u32::MAX);
        let dir = archive_dir_name(&folder.target, &taken);
        taken.push(dir.clone());
        base.roots.push(ManifestRoot {
            id,
            target: folder.target.clone(),
            dir: dir.clone(),
            managed: true,
        });
        if rules.root_ignored(&folder.target).is_some() {
            walk.ignored.push(ManifestIgnored {
                root: id,
                path: String::new(),
            });
            continue;
        }
        if let Err(halt) = walk.folder(id, &dir, folder, &rules) {
            return match halt {
                Halt::Stop => CopyOutcome::Stopped(control.reason()),
                Halt::Fail(cause) => CopyOutcome::Failed(cause),
            };
        }
        // Le disque est relu aussi entre deux dossiers (une copie de petits fichiers ne le relit qu'ici).
        if disk_low(files.as_ref()) {
            control.ask(STOP_DISK);
            return CopyOutcome::Stopped(STOP_DISK);
        }
    }
    let Walk {
        writer,
        entries,
        ignored,
        ..
    } = walk;
    let changed = control.changed.load(Ordering::Relaxed);
    base.flags = flags_of(hot, forced_stop, changed);
    base.totals = Manifest::recount(&entries);
    base.entries = entries;
    base.ignored = ignored;
    let Ok(json) = serde_json::to_vec(&base) else {
        return CopyOutcome::Failed(Cause::StepFailed);
    };
    match writer.finish(&json) {
        Ok(end) => CopyOutcome::Done {
            manifest: Box::new(base),
            end,
            changed,
        },
        Err(error) => CopyOutcome::Failed(cause_of_io(&error)),
    }
}

/// Les repères d'une sauvegarde (`BR-BACKUP-007`, `BR-BACKUP-008`, `BR-BACKUP-012`).
fn flags_of(hot: bool, forced_stop: bool, changed: u32) -> Vec<BackupFlag> {
    let mut flags = Vec::new();
    if hot {
        flags.push(BackupFlag::Hot);
    }
    if forced_stop {
        flags.push(BackupFlag::ForcedStop);
    }
    if (hot && changed > 0) || forced_stop {
        flags.push(BackupFlag::MaybeInconsistent);
    }
    flags
}

// ---------------------------------------------------------------------------------------------
// Le cas d'usage
// ---------------------------------------------------------------------------------------------

/// Ce que l'exécution décide de son issue.
#[derive(Debug)]
struct Finish {
    status: OpStatus,
    cause: Option<Cause>,
    note: BackupNote,
}

impl Finish {
    fn failed(cause: Cause) -> Self {
        Self {
            status: OpStatus::Failed,
            cause: Some(cause),
            note: BackupNote::Nothing,
        }
    }

    fn cancelled() -> Self {
        Self {
            status: OpStatus::Cancelled,
            cause: Some(Cause::CancelledByAccount),
            note: BackupNote::Nothing,
        }
    }
}

/// Tout ce qu'une exécution emporte avec elle. Aucun secret, aucune note en clair.
struct Job {
    id: String,
    key: ServiceKey,
    name: String,
    actor: Actor,
    backup: BackupId,
    mode: BackupMode,
    containers: Vec<String>,
    grace_s: u32,
    folders: Vec<ManagedFolder>,
    rules: IgnoreRules,
    image: String,
    note: Option<crate::domain::sealed::SealedBlob>,
    estimate_bytes: u64,
    /// Les conteneurs que CETTE opération a arrêtés (à relancer).
    stopped: Mutex<Vec<String>>,
    forced_stop: AtomicBool,
}

struct Subject {
    name: String,
    body: Option<SpecBody>,
    state: Option<ServiceStateKind>,
    grace_s: u32,
    containers: Vec<String>,
}

pub struct Backups {
    registry: Arc<ServicesRegistry>,
    ops: Arc<ServiceOps>,
    specs: Arc<ServiceSpecs>,
    engine: Arc<dyn ContainerEngine>,
    containers: Arc<dyn ServiceContainers>,
    intents: Arc<dyn ServiceIntents>,
    safe_fs: Arc<dyn SafeFs>,
    files: Arc<dyn BackupFiles>,
    repo: Arc<dyn BackupRepo>,
    host_paths: Arc<dyn HostPaths>,
    sink: Arc<dyn AuditSink>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGen>,
    context: BackupContext,
    stopping: Arc<AtomicBool>,
    /// Les sauvegardes qu'une opération utilise (création, vérification) : une suppression est refusée tant
    /// qu'elles y sont (`BR-BACKUP-027`).
    in_use: Mutex<HashSet<String>>,
}

/// Une sauvegarde en usage, rendue à la chute du jeton.
struct UseGuard<'a> {
    backups: &'a Backups,
    id: String,
}

impl Drop for UseGuard<'_> {
    fn drop(&mut self) {
        self.backups.unuse(&self.id);
    }
}

impl Backups {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        registry: Arc<ServicesRegistry>,
        ops: Arc<ServiceOps>,
        specs: Arc<ServiceSpecs>,
        engine: Arc<dyn ContainerEngine>,
        containers: Arc<dyn ServiceContainers>,
        intents: Arc<dyn ServiceIntents>,
        safe_fs: Arc<dyn SafeFs>,
        files: Arc<dyn BackupFiles>,
        repo: Arc<dyn BackupRepo>,
        host_paths: Arc<dyn HostPaths>,
        sink: Arc<dyn AuditSink>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGen>,
        context: BackupContext,
    ) -> Arc<Self> {
        Arc::new(Self {
            registry,
            ops,
            specs,
            engine,
            containers,
            intents,
            safe_fs,
            files,
            repo,
            host_paths,
            sink,
            clock,
            ids,
            context,
            stopping: Arc::new(AtomicBool::new(false)),
            in_use: Mutex::new(HashSet::new()),
        })
    }

    /// L'agent s'arrête : une copie en cours s'arrête, son opération reste en cours dans le journal et sera
    /// « interrompue » au prochain démarrage (sa partie écrite est retirée alors, jamais reprise).
    pub fn begin_shutdown(&self) {
        self.stopping.store(true, Ordering::SeqCst);
    }

    /// Au démarrage, après la reprise des opérations : les lignes `writing` du registre sont retirées (leurs
    /// fichiers partiels l'ont été par le journal, après vérification de l'inode). La racine est jugée et le
    /// résultat DIT : une racine qui chevauche un dossier de données refuse la fonction (`BR-BACKUP-004` (b)).
    pub async fn recover_at_startup(&self) {
        match self.repo.drop_writing().await {
            Ok(0) => {}
            Ok(count) => tracing::warn!(
                count,
                "sauvegardes interrompues par l'arrêt de l'agent : lignes du registre retirées"
            ),
            Err(error) => {
                tracing::error!(%error, "registre des sauvegardes illisible au démarrage")
            }
        }
        match self.check_overlap().await {
            Ok(()) => {}
            Err(BackupError::RootOverlap) => tracing::error!(
                "la racine des sauvegardes chevauche un dossier de données : les sauvegardes sont refusées"
            ),
            Err(error) => tracing::warn!(%error, "la racine des sauvegardes n'est pas jugée sûre"),
        }
    }

    fn lock_in_use(&self) -> std::sync::MutexGuard<'_, HashSet<String>> {
        self.in_use.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn try_use(&self, id: &str) -> Option<UseGuard<'_>> {
        self.lock_in_use().insert(id.to_owned()).then(|| UseGuard {
            backups: self,
            id: id.to_owned(),
        })
    }

    fn unuse(&self, id: &str) {
        self.lock_in_use().remove(id);
    }

    // -----------------------------------------------------------------------------------------
    // La racine des sauvegardes
    // -----------------------------------------------------------------------------------------

    /// Ce que l'on sait d'un chemin : sa forme canonique et sa lignée. Un chemin qui n'existe pas prend celle de
    /// son plus proche parent qui existe. `None` : illisible (on ne sait pas : jamais un feu vert).
    async fn path_facts(&self, path: &str) -> Option<PathFacts> {
        let paths = Arc::clone(&self.host_paths);
        let path = path.to_owned();
        tokio::task::spawn_blocking(move || {
            let mut remaining: Vec<String> = Vec::new();
            let mut current = path.trim_end_matches('/').to_owned();
            loop {
                match paths.resolve(if current.is_empty() { "/" } else { &current }) {
                    Resolved::Found { canonical, lineage } => {
                        let exists = remaining.is_empty();
                        let mut canonical = canonical.trim_end_matches('/').to_owned();
                        for part in remaining.iter().rev() {
                            canonical.push('/');
                            canonical.push_str(part);
                        }
                        return Some(PathFacts {
                            canonical,
                            lineage,
                            exists,
                        });
                    }
                    Resolved::Unreadable => return None,
                    Resolved::Absent => {
                        // Les deux séparateurs : le poste de développement Windows écrit des barres obliques inverses.
                        let cut = current.rfind(['/', '\\'])?;
                        remaining.push(current[cut + 1..].to_owned());
                        current.truncate(cut);
                    }
                }
            }
        })
        .await
        .ok()
        .flatten()
    }

    /// La racine des sauvegardes chevauche-t-elle un dossier de données ? Jugé sur les chemins canoniques ET sur
    /// les identités (`BR-BACKUP-004` (b)), à chaque démarrage et avant chaque opération.
    async fn check_overlap(&self) -> Result<(), BackupError> {
        let (Some(backups), Some(data)) = (
            self.path_facts(&self.context.backups_root).await,
            self.path_facts(&self.context.services_root).await,
        ) else {
            return Err(BackupError::Unavailable);
        };
        if overlaps(&backups, &data) {
            return Err(BackupError::RootOverlap);
        }
        Ok(())
    }

    /// Comme `check_overlap`, après avoir créé la racine et jugé qu'elle est sûre : pour ce qui écrit.
    async fn check_root(&self) -> Result<(), BackupError> {
        // La non-chevauchement d'abord : on ne crée jamais la racine DANS un dossier de données.
        self.check_overlap().await?;
        let files = Arc::clone(&self.files);
        match tokio::task::spawn_blocking(move || files.ensure_root()).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::warn!(?error, "racine des sauvegardes inutilisable");
                return Err(BackupError::Unavailable);
            }
            Err(error) => return Err(BackupError::Internal(error.to_string())),
        }
        self.check_overlap().await
    }

    /// L'espace du disque des sauvegardes, `None` si la mesure échoue.
    async fn space(&self) -> Option<DiskSpace> {
        let files = Arc::clone(&self.files);
        tokio::task::spawn_blocking(move || files.space())
            .await
            .ok()
            .and_then(Result::ok)
    }

    /// Les sauvegardes sont-elles sur le même disque que les données des services (`BR-BACKUP-005`) ?
    async fn same_disk(&self) -> bool {
        let (Some(backups), Some(data)) = (
            self.path_facts(&self.context.backups_root).await,
            self.path_facts(&self.context.services_root).await,
        ) else {
            return false;
        };
        matches!((backups.lineage.first(), data.lineage.first()), (Some(a), Some(b)) if a.0 == b.0)
    }

    // -----------------------------------------------------------------------------------------
    // Ce qui se juge sans rien attendre du moteur
    // -----------------------------------------------------------------------------------------

    async fn subject(&self, key: &ServiceKey) -> Result<Subject, BackupFailed> {
        let engine_state = self.registry.engine_state();
        if engine_state != EngineState::Ready {
            return Err(BackupFailed::plain(BackupError::EngineUnavailable(
                engine_state,
            )));
        }
        let target = self.registry.lifecycle_target(key);
        if key.kind() != KeyKind::Hearth {
            return Err(match target {
                Some(target) => BackupFailed::named(BackupError::Adopted, &target.name),
                None => BackupFailed::not_found(),
            });
        }
        let body = self.specs.body(key).await.map_err(internal)?;
        let name = match (&target, &body) {
            (Some(target), _) => target.name.clone(),
            (None, Some(body)) => body.name.clone(),
            (None, None) => return Err(BackupFailed::not_found()),
        };
        let mut containers = self.containers.of(key).await.map_err(internal)?;
        if let Some(target) = &target
            && let Driver::Engine { container_id } = &target.driver
            && !containers.contains(container_id)
        {
            containers.push(container_id.clone());
        }
        Ok(Subject {
            name,
            body,
            state: target.as_ref().map(|target| target.state),
            grace_s: target.as_ref().map_or(10, |target| target.grace_s),
            containers,
        })
    }

    fn busy_of(&self, key: &ServiceKey) -> Option<BackupError> {
        self.ops
            .busy(key)
            .map(|(by, kind)| BackupError::Busy { by, kind })
    }

    fn folders_of(
        &self,
        key: &ServiceKey,
        body: Option<&SpecBody>,
    ) -> (Vec<ManagedFolder>, Vec<String>) {
        let Some(body) = body else {
            return (Vec::new(), Vec::new());
        };
        let split = split_mounts(body, &self.context.services_root, key.as_str());
        let not_saved = body
            .mounts
            .iter()
            .filter(|mount| mount.kind == SpecMountKind::HostFolder)
            .map(|mount| mount.target.clone())
            .collect();
        (split.managed, not_saved)
    }

    async fn rules_of(
        &self,
        key: &ServiceKey,
        once: Vec<IgnoredItem>,
    ) -> Result<IgnoreRules, BackupFailed> {
        let settings = self.repo.settings(key).await.map_err(internal)?;
        Ok(IgnoreRules {
            chosen: settings.ignored,
            included: settings.included,
            once,
        })
    }

    async fn estimate(
        &self,
        key: &ServiceKey,
        folders: &[ManagedFolder],
        rules: &IgnoreRules,
    ) -> Result<Estimate, BackupFailed> {
        let (fs, key, folders, rules, budget, big) = (
            Arc::clone(&self.safe_fs),
            key.clone(),
            folders.to_vec(),
            rules.clone(),
            self.context.timings.estimate_budget,
            self.context.timings.big_file_bytes,
        );
        let result = tokio::task::spawn_blocking(move || {
            estimate_blocking(fs.as_ref(), &key, &folders, &rules, budget, big)
        })
        .await
        .map_err(internal)?;
        match result {
            Ok(estimate) => Ok(estimate),
            // La racine du service n'existe pas : rien à sauvegarder.
            Err(FsError::NotFound) => Ok(Estimate::default()),
            Err(error) => Err(internal(format!("{error:?}"))),
        }
    }

    fn view_of(row: &BackupRow, integrity: BackupIntegrity, note: Option<String>) -> BackupView {
        BackupView {
            id: row.id.to_string(),
            created_at: rfc3339(row.created_at),
            archive_bytes: row.archive_bytes,
            data_bytes: row.data_bytes,
            files: row.files,
            mode: row.mode,
            flags: row.flags.clone(),
            changed_files: row.changed_files,
            note,
            account: row.created_by.clone(),
            image: row.image.clone(),
            integrity,
            verified_at: row.verified_at.map(rfc3339),
        }
    }

    /// Ce que le disque dit de l'archive d'une ligne : la sienne, remplacée, ou disparue (`BR-BACKUP-004`).
    async fn ownership(&self, row: &BackupRow) -> Result<Ownership, BackupFailed> {
        let files = Arc::clone(&self.files);
        let (dir, name) = (row.service_key.to_string(), row.id.archive_name());
        let registered = registered_of(row);
        let seen = tokio::task::spawn_blocking(move || files.seen(&dir, &name))
            .await
            .map_err(internal)?;
        match seen {
            Ok(seen) => Ok(judge_archive(&registered, &seen)),
            Err(BackupFsError::Unsafe | BackupFsError::Unsupported) => {
                Err(BackupFailed::plain(BackupError::Unavailable))
            }
            Err(error) => Err(internal(format!("{error:?}"))),
        }
    }

    // -----------------------------------------------------------------------------------------
    // Lister
    // -----------------------------------------------------------------------------------------

    /// `GET /services/{key}/backups`.
    pub async fn list(&self, key: &ServiceKey) -> Result<BackupsView, BackupFailed> {
        let subject = self.subject(key).await?;
        self.check_overlap()
            .await
            .map_err(|e| BackupFailed::named(e, &subject.name))?;
        let rows = self.repo.list(key).await.map_err(internal)?;
        let mut views = Vec::with_capacity(rows.len());
        let mut total = 0_u64;
        let mut opener: Option<Arc<dyn crate::application::ports::SecretSealer>> = None;
        for row in &rows {
            let integrity = match self.ownership(row).await? {
                Ownership::Missing => BackupIntegrity::Missing,
                Ownership::Replaced => BackupIntegrity::Corrupt,
                Ownership::Ours => row.integrity,
            };
            total = total.saturating_add(row.archive_bytes);
            let note = match &row.note {
                Some(blob) => {
                    // La clé n'est demandée que s'il y a une note à ouvrir, et jamais créée pour ça : une
                    // note existe, donc la clé a déjà servi.
                    if opener.is_none() {
                        opener = self.specs.vault().sealer().await.ok();
                    }
                    opener
                        .as_ref()
                        .and_then(|sealer| open_note(sealer.as_ref(), row, blob))
                }
                None => None,
            };
            views.push(Self::view_of(row, integrity, note));
        }
        let (folders, _) = self.folders_of(key, subject.body.as_ref());
        Ok(BackupsView {
            backups: views,
            total_bytes: total,
            free_bytes: self.space().await.map(|space| space.free),
            same_disk: self.same_disk().await,
            settings: self.repo.settings(key).await.map_err(internal)?,
            nothing_to_save: folders.is_empty(),
        })
    }

    // -----------------------------------------------------------------------------------------
    // La fenêtre de création
    // -----------------------------------------------------------------------------------------

    /// `POST /services/{key}/backups/preview` : de quoi bâtir la fenêtre. Une consultation : rien n'est écrit.
    pub async fn preview(&self, key: &ServiceKey) -> Result<BackupPreview, BackupFailed> {
        let subject = self.subject(key).await?;
        let named = |error| BackupFailed::named(error, &subject.name);
        self.check_overlap().await.map_err(named)?;
        let (folders, not_saved) = self.folders_of(key, subject.body.as_ref());
        let rules = self.rules_of(key, Vec::new()).await?;
        let estimate = if folders.is_empty() {
            Estimate::default()
        } else {
            self.estimate(key, &folders, &rules).await?
        };
        let space = self.space().await;
        let (free, total) = space.map_or((None, 0), |space| (Some(space.free), space.total));
        let reserve = crate::domain::backups::space::reserve(total);
        let enough = match space {
            Some(space) => matches!(
                judge(estimate.bytes, space.free, space.total),
                SpaceVerdict::Enough
            ),
            // Une mesure qui échoue n'est pas un feu vert : la création la refera.
            None => true,
        };
        Ok(BackupPreview {
            name: subject.name.clone(),
            running: is_running(subject.state),
            stop_grace_s: subject.grace_s,
            roots: folders.iter().map(|folder| folder.target.clone()).collect(),
            not_saved,
            bytes: estimate.bytes,
            files: estimate.files,
            partial: estimate.partial,
            big_files: estimate.big,
            ignored: estimate.ignored,
            ignored_more: estimate.ignored_more,
            free_bytes: free,
            reserve_bytes: reserve,
            enough_space: enough,
            same_disk: self.same_disk().await,
            nothing_to_save: folders.is_empty(),
        })
    }

    // -----------------------------------------------------------------------------------------
    // Créer
    // -----------------------------------------------------------------------------------------

    /// Accepte (ou refuse tout de suite) une sauvegarde. Le droit de gérer les services et la confirmation de
    /// l'acte ont déjà été jugés.
    pub async fn create(
        self: &Arc<Self>,
        key: &ServiceKey,
        request: BackupRequest,
        actor: Actor,
        op_key: &str,
    ) -> Result<BackupAccepted, BackupFailed> {
        self.ops.wait_closed(key).await;
        let subject = self.subject(key).await?;
        let named = |error| BackupFailed::named(error, &subject.name);
        if let Some(busy) = self.busy_of(key) {
            return Err(named(busy));
        }
        let note = clean_note(request.note.as_deref()).map_err(named)?;
        validate_items(&request.ignore_once).map_err(named)?;
        let running = is_running(subject.state);
        if let Some(state) = subject.state
            && !backable(state)
        {
            return Err(named(BackupError::NotBackable(state)));
        }
        let ModeVerdict::Mode(mode) = effective_mode(request.mode, running) else {
            return Err(named(BackupError::ModeRefused));
        };
        self.check_root().await.map_err(named)?;
        let (folders, _) = self.folders_of(key, subject.body.as_ref());
        if folders.is_empty() {
            return Err(named(BackupError::NothingToSave));
        }
        let rules = self.rules_of(key, request.ignore_once).await?;
        let estimate = self.estimate(key, &folders, &rules).await?;
        // `BR-BACKUP-009` : refusé sous la réserve, sans case pour passer outre. Une mesure d'espace qui échoue
        // n'est pas un feu vert pour une sauvegarde qui écrit : refusée.
        let Some(space) = self.space().await else {
            return Err(named(BackupError::Unavailable));
        };
        if let SpaceVerdict::Refused {
            needed,
            free,
            reserve,
        } = judge(estimate.bytes, space.free, space.total)
        {
            return Err(named(BackupError::NoSpace {
                needed,
                free,
                reserve,
            }));
        }
        let backup = BackupId::parse(&self.ids.new_id()).ok_or_else(|| {
            BackupFailed::named(
                BackupError::Internal("identifiant illisible".into()),
                &subject.name,
            )
        })?;
        // La note est scellée AVANT d'accepter : une clé illisible se dit tout de suite.
        let sealed = match note {
            Some(text) => Some(self.seal_note(key, &backup, &text).await.map_err(named)?),
            None => None,
        };
        let plan =
            op_plan(mode).map_err(|error| named(BackupError::Internal(error.to_string())))?;
        let account = account_of(&actor);
        let op = self
            .ops
            .accept_after_wait(AcceptRequest {
                op_key: op_key.to_owned(),
                service: key.clone(),
                service_name: subject.name.clone(),
                kind: OpKind::Backup,
                account,
                plan,
                forced: false,
                reserve_name: None,
                heavy_io: true,
            })
            .await
            .map_err(|error| match error {
                OpError::Locked(Refusal::HeavyIoBusy { service }) => {
                    named(BackupError::HeavyBusy { service })
                }
                OpError::Locked(Refusal::ServiceBusy { holder }) => named(BackupError::Busy {
                    by: self.holder_account(key),
                    kind: match holder {
                        Holder::Operation(kind) => kind,
                        Holder::FileWrites => OpKind::Backup,
                    },
                }),
                OpError::ServiceBusy => named(BackupError::Busy {
                    by: self.holder_account(key),
                    kind: OpKind::Backup,
                }),
                other => named(BackupError::Internal(other.to_string())),
            })?;
        if let Err(error) = self.ops.start(&op).await {
            journal(
                self.ops
                    .finish(&op, OpStatus::Failed, Some(Cause::StepFailed))
                    .await,
                "finish",
            );
            return Err(named(BackupError::Internal(error.to_string())));
        }
        let job = Arc::new(Job {
            id: op.clone(),
            key: key.clone(),
            name: subject.name.clone(),
            actor: actor.clone(),
            backup: backup.clone(),
            mode,
            containers: subject.containers,
            grace_s: subject.grace_s,
            folders,
            rules,
            image: subject
                .body
                .as_ref()
                .map(|body| image_reference(&body.image))
                .unwrap_or_default(),
            note: sealed,
            estimate_bytes: estimate.bytes,
            stopped: Mutex::new(Vec::new()),
            forced_stop: AtomicBool::new(false),
        });
        let this = Arc::clone(self);
        let work_job = Arc::clone(&job);
        let work = tokio::spawn(async move { this.execute(&work_job).await });
        let watcher = Arc::clone(self);
        tokio::spawn(async move {
            if work.await.is_err() {
                tracing::error!(operation = %job.id, "exécution d'une sauvegarde interrompue par une panique");
                watcher.close_after_panic(&job).await;
            }
        });
        Ok(BackupAccepted {
            operation_id: op,
            backup_id: backup.to_string(),
        })
    }

    fn holder_account(&self, key: &ServiceKey) -> String {
        self.ops.busy(key).map(|(by, _)| by).unwrap_or_default()
    }

    async fn seal_note(
        &self,
        key: &ServiceKey,
        backup: &BackupId,
        text: &str,
    ) -> Result<crate::domain::sealed::SealedBlob, BackupError> {
        let sealer = self
            .specs
            .vault()
            .sealer()
            .await
            .map_err(|_| BackupError::KeyUnavailable)?;
        let field = note_field(backup).ok_or(BackupError::KeyUnavailable)?;
        sealer
            .seal(
                &SealContext {
                    service: key.as_str(),
                    rev: 0,
                    field: &field,
                },
                text.as_bytes(),
            )
            .map_err(|_| BackupError::KeyUnavailable)
    }

    // -----------------------------------------------------------------------------------------
    // L'exécution
    // -----------------------------------------------------------------------------------------

    async fn execute(&self, job: &Job) {
        let _in_use = self.try_use(job.backup.as_str());
        let started = Instant::now();
        let Some(finish) = self.run(job).await else {
            tracing::info!(operation = %job.id, "sauvegarde arrêtée par l'arrêt de l'agent : interrompue au prochain démarrage");
            return;
        };
        self.settle(job, finish, started.elapsed()).await;
    }

    async fn is_cancelling(&self, id: &str) -> bool {
        matches!(
            self.ops.snapshot(id).await,
            Ok(Some(snapshot)) if snapshot.record.status == OpStatus::Cancelling
        )
    }

    async fn begin(&self, job: &Job, name: &str, undo: &UndoInfo) -> Result<(), Finish> {
        match self.ops.intent(&job.id, name, undo).await {
            Ok(_) => Ok(()),
            Err(OpError::Stale | OpError::OutOfOrder) if self.is_cancelling(&job.id).await => {
                Err(Finish::cancelled())
            }
            Err(error) => {
                tracing::error!(%error, step = name, "journal de la sauvegarde illisible : l'étape n'est pas lancée");
                Err(Finish::failed(Cause::StepFailed))
            }
        }
    }

    async fn done(&self, job: &Job, name: &str) {
        journal(
            self.ops.done(&job.id, name, &UndoInfo::default()).await,
            "done",
        );
    }

    /// `None` : l'agent s'arrête, rien n'est clos.
    async fn run(&self, job: &Job) -> Option<Finish> {
        // 1. Arrêt, seulement dans ce mode.
        if job.mode == BackupMode::StopCopyStart {
            if let Err(finish) = self.begin(job, step::STOP, &UndoInfo::default()).await {
                return Some(finish);
            }
            if let Err(finish) = self.stop_service(job).await {
                return Some(finish);
            }
            self.done(job, step::STOP).await;
            if self.is_cancelling(&job.id).await {
                return Some(self.finish_after(job, Finish::cancelled()).await);
            }
        }
        // 2. La copie.
        let copied = match self.copy(job).await {
            CopyEnd::Halted => return None,
            CopyEnd::Over(Err(finish)) => return Some(self.finish_after(job, finish).await),
            CopyEnd::Over(Ok(copied)) => copied,
        };
        // 3. La vérification.
        if let Err(finish) = self.begin(job, step::VERIFY, &UndoInfo::default()).await {
            return Some(self.finish_after(job, finish).await);
        }
        let verified = self.verify_new(job).await;
        let finish = match verified {
            Ok(()) => {
                self.done(job, step::VERIFY).await;
                Finish {
                    status: OpStatus::Succeeded,
                    cause: None,
                    note: copied.note(job),
                }
            }
            Err(finish) => finish,
        };
        Some(self.finish_after(job, finish).await)
    }

    /// 4. Relance du service arrêté par cette opération, quelle que soit l'issue (`BR-BACKUP-008`).
    async fn finish_after(&self, job: &Job, finish: Finish) -> Finish {
        if job.mode != BackupMode::StopCopyStart {
            return finish;
        }
        let stopped: Vec<String> = job
            .stopped
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if stopped.is_empty() {
            return finish;
        }
        if self
            .begin(job, step::START, &UndoInfo::default())
            .await
            .is_err()
        {
            // Une annulation passée entre-temps ne dispense pas de relancer ce que l'on a arrêté.
            tracing::warn!(operation = %job.id, "journal illisible : le service arrêté est relancé quand même");
        }
        match self.start_service(job, &stopped).await {
            Ok(()) => {
                self.done(job, step::START).await;
                finish
            }
            Err(()) => {
                journal(self.ops.failed(&job.id, step::START).await, "failed");
                // La sauvegarde a pu réussir : seule la cause dit que le service ne repart pas.
                if finish.status == OpStatus::Succeeded {
                    Finish {
                        cause: Some(Cause::NotRestarted),
                        ..finish
                    }
                } else {
                    finish
                }
            }
        }
    }

    /// Arrête les conteneurs du service qui tournent : l'arrêt voulu est écrit AVANT l'ordre, le délai de grâce
    /// est celui du service, une coupure forcée est dite (`BR-BACKUP-008`).
    async fn stop_service(&self, job: &Job) -> Result<(), Finish> {
        let mut running = Vec::new();
        for id in &job.containers {
            match self.engine.inspect(id).await {
                Ok(Some(observation))
                    if matches!(
                        observation.run_state,
                        RunState::Running | RunState::Restarting | RunState::Paused
                    ) =>
                {
                    running.push(id.clone());
                }
                Ok(_) => {}
                Err(error) => {
                    journal(self.ops.failed(&job.id, step::STOP).await, "failed");
                    return Err(Finish::failed(cause_of(error)));
                }
            }
        }
        if running.is_empty() {
            return Ok(());
        }
        let intent = StopIntent {
            key: job.key.clone(),
            container_id: running.first().cloned().unwrap_or_default(),
            account: account_of(&job.actor),
            at: self.clock.now(),
        };
        if let Err(error) = self.intents.save(&intent).await {
            tracing::error!(%error, "arrêt voulu non écrit : l'ordre d'arrêt n'est pas donné");
            journal(self.ops.failed(&job.id, step::STOP).await, "failed");
            return Err(Finish::failed(Cause::StepFailed));
        }
        self.registry.note_stop_requested(&job.key, intent.at);
        self.registry
            .begin_transition(&job.key, TransitionKind::Stopping);
        let _shown = StepShown {
            registry: &self.registry,
            key: job.key.clone(),
        };
        let grace = Duration::from_secs(u64::from(job.grace_s));
        let wait = wait_budget_with(
            job.grace_s,
            self.context.timings.stop_margin,
            self.context.timings.stop_floor,
        );
        for id in &running {
            let started = Instant::now();
            let ordered = match tokio::time::timeout(wait, self.engine.stop(id, grace)).await {
                Ok(result) => result,
                Err(_) => Err(EngineError::Timeout),
            };
            match ordered {
                Ok(()) | Err(EngineError::NotFound) => {
                    job.stopped
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(id.clone());
                    let evidence = match self.engine.inspect(id).await {
                        Ok(Some(observation)) => StopEvidence::ExitCode {
                            code: observation.exit_code,
                            waited: started.elapsed(),
                        },
                        _ => StopEvidence::Took(started.elapsed()),
                    };
                    if stop_was_forced(evidence, job.grace_s) {
                        job.forced_stop.store(true, Ordering::SeqCst);
                        journal(self.ops.mark_forced(&job.id).await, "forced");
                    }
                }
                Err(error) => {
                    journal(self.ops.failed(&job.id, step::STOP).await, "failed");
                    // Un refus net : rien n'a été arrêté. Un résultat inconnu garde l'arrêt voulu : il a peut-être eu lieu.
                    if matches!(error, EngineError::Refused | EngineError::Denied) {
                        self.forget_stop(&job.key).await;
                    }
                    // Ce qui a déjà été arrêté est relancé par `finish_after`.
                    return Err(Finish::failed(cause_of(error)));
                }
            }
        }
        for id in &running {
            self.registry.reread(id).await;
        }
        Ok(())
    }

    async fn forget_stop(&self, key: &ServiceKey) {
        if let Err(error) = self.intents.clear(key).await {
            tracing::warn!(%error, "arrêt voulu non effacé");
        }
        self.registry.forget_stop_requested(key);
    }

    /// Relance ce que cette opération a arrêté. `Err` : un conteneur n'est pas reparti.
    async fn start_service(&self, job: &Job, stopped: &[String]) -> Result<(), ()> {
        self.registry
            .begin_transition(&job.key, TransitionKind::Starting);
        let _shown = StepShown {
            registry: &self.registry,
            key: job.key.clone(),
        };
        let from = self.clock.now() - time::Duration::minutes(1);
        let mut ok = true;
        for id in stopped {
            let started =
                tokio::time::timeout(self.context.timings.start_wait, self.engine.start(id))
                    .await
                    .unwrap_or(Err(EngineError::Timeout));
            if let Err(error) = started {
                tracing::warn!(
                    ?error,
                    "le service arrêté pour la sauvegarde ne redémarre pas"
                );
                ok = false;
            }
        }
        // Le service est revenu à l'état d'avant : l'arrêt voulu est oublié, les démarrages demandés par Hearth ne
        // comptent pas pour « redémarre en boucle ».
        self.forget_stop(&job.key).await;
        self.registry.exempt_starts(
            &job.key,
            from,
            self.clock.now() + time::Duration::seconds(5),
        );
        for id in stopped {
            self.registry.reread(id).await;
        }
        if ok { Ok(()) } else { Err(()) }
    }

    /// L'étape `copy` : crée l'archive, la copie, la publie. `Over(Err)` : l'issue à rendre (la partie écrite est
    /// déjà retirée) ; `Halted` : l'agent s'arrête.
    async fn copy(&self, job: &Job) -> CopyEnd {
        let service_dir = job.key.to_string();
        let part_name = job.backup.part_name();
        let files = Arc::clone(&self.files);
        let (dir, name) = (service_dir.clone(), part_name.clone());
        let created = tokio::task::spawn_blocking(move || files.create(&dir, &name)).await;
        let (sink, ident) = match created {
            Ok(Ok(created)) => created,
            Ok(Err(error)) => {
                tracing::warn!(?error, "archive de sauvegarde non créée");
                return CopyEnd::Over(Err(Finish::failed(cause_of_fs(error))));
            }
            Err(error) => {
                tracing::error!(%error, "création de l'archive interrompue");
                return CopyEnd::Over(Err(Finish::failed(Cause::StepFailed)));
            }
        };
        // L'inscription au registre et au journal : un agent qui s'arrête ici laisse un objet CRÉÉ par l'opération,
        // retiré au démarrage après vérification de son inode (`BR-BACKUP-025`).
        let undo = UndoInfo {
            created: vec![CreatedObject {
                root: ObjectRoot::Backups,
                rel: format!("{service_dir}/{part_name}"),
                dev: ident.0,
                ino: ident.1,
                kind: ObjectKind::PartialArchive,
            }],
        };
        if let Err(finish) = self.begin(job, step::COPY, &undo).await {
            self.discard_part(job, ident).await;
            return CopyEnd::Over(Err(finish));
        }
        let inserted = self
            .repo
            .insert_writing(&NewBackup {
                id: job.backup.clone(),
                service_key: job.key.clone(),
                service_name: job.name.clone(),
                created_at: self.clock.now(),
                created_by: account_of(&job.actor),
                dev: ident.0,
                ino: ident.1,
                mode: job.mode,
                image: job.image.clone(),
                note: job.note.clone(),
            })
            .await;
        if let Err(error) = inserted {
            tracing::error!(%error, "sauvegarde non inscrite au registre");
            journal(self.ops.failed(&job.id, step::COPY).await, "failed");
            self.discard_part(job, ident).await;
            return CopyEnd::Over(Err(Finish::failed(Cause::StepFailed)));
        }
        let control = Arc::new(Control {
            stop: AtomicU8::new(STOP_NONE),
            files: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            changed: AtomicU32::new(0),
            deadline: Instant::now() + self.context.timings.max_copy,
        });
        let base = Manifest {
            format: FORMAT,
            backup_id: job.backup.to_string(),
            service_id: job.key.to_string(),
            service_name: job.name.clone(),
            created_at: rfc3339(self.clock.now()),
            agent_version: self.context.agent_version.clone(),
            image: job.image.clone(),
            mode: job.mode,
            flags: Vec::new(),
            roots: Vec::new(),
            ignored: Vec::new(),
            entries: Vec::new(),
            totals: crate::domain::backups::manifest::Totals::default(),
            archive: None,
        };
        let copy = CopyJob {
            fs: Arc::clone(&self.safe_fs),
            files: Arc::clone(&self.files),
            key: job.key.clone(),
            folders: job.folders.clone(),
            rules: job.rules.clone(),
            sink,
            control: Arc::clone(&control),
            base,
            forced_stop: job.forced_stop.load(Ordering::SeqCst),
            hot: job.mode == BackupMode::Hot,
        };
        let mut work = tokio::task::spawn_blocking(move || copy_blocking(copy));
        let outcome = loop {
            tokio::select! {
                finished = &mut work => {
                    break finished.unwrap_or(CopyOutcome::Failed(Cause::StepFailed));
                }
                () = tokio::time::sleep(self.context.timings.progress_every) => {
                    if self.stopping.load(Ordering::SeqCst) {
                        control.ask(STOP_SHUTDOWN);
                    } else if self.is_cancelling(&job.id).await {
                        control.ask(STOP_CANCEL);
                    }
                    self.report(job, &control).await;
                }
            }
        };
        match outcome {
            CopyOutcome::Done {
                manifest,
                end,
                changed,
            } => self.publish(job, ident, *manifest, end, changed).await,
            CopyOutcome::Stopped(STOP_SHUTDOWN) => CopyEnd::Halted,
            CopyOutcome::Stopped(reason) => {
                journal(self.ops.failed(&job.id, step::COPY).await, "failed");
                self.discard(job, ident).await;
                CopyEnd::Over(Err(match reason {
                    STOP_CANCEL => Finish::cancelled(),
                    STOP_DISK => Finish::failed(Cause::DiskAlmostFull),
                    _ => Finish::failed(Cause::Timeout),
                }))
            }
            CopyOutcome::Failed(cause) => {
                journal(self.ops.failed(&job.id, step::COPY).await, "failed");
                self.discard(job, ident).await;
                CopyEnd::Over(Err(Finish::failed(cause)))
            }
        }
    }

    /// Le manifeste voisin, l'archive sous son nom définitif, la ligne `ready` (`BR-BACKUP-012`, `BR-BACKUP-014`).
    async fn publish(
        &self,
        job: &Job,
        ident: (u64, u64),
        mut manifest: Manifest,
        end: ArchiveEnd,
        changed: u32,
    ) -> CopyEnd {
        manifest.archive = Some(ArchiveFacts {
            name: job.backup.archive_name(),
            bytes: end.bytes,
            sha256: end.sha256.clone(),
        });
        let Ok(json) = serde_json::to_vec(&manifest) else {
            journal(self.ops.failed(&job.id, step::COPY).await, "failed");
            self.discard(job, ident).await;
            return CopyEnd::Over(Err(Finish::failed(Cause::StepFailed)));
        };
        let manifest_sha = sha256_hex(&json);
        let service_dir = job.key.to_string();
        let files = Arc::clone(&self.files);
        let (dir, side, part, archive) = (
            service_dir.clone(),
            job.backup.manifest_name(),
            job.backup.part_name(),
            job.backup.archive_name(),
        );
        let published = tokio::task::spawn_blocking(move || {
            files.write_new(&dir, &side, &json)?;
            files.publish(&dir, &part, &archive, ident)
        })
        .await;
        match published {
            Ok(Ok(())) => {}
            other => {
                tracing::warn!(?other, "archive de sauvegarde non publiée");
                journal(self.ops.failed(&job.id, step::COPY).await, "failed");
                self.discard(job, ident).await;
                // Le manifeste voisin écrit seul : c'est le nôtre (créé par cette opération), retiré avec le reste.
                self.remove_own_manifest(job, &manifest_sha).await;
                return CopyEnd::Over(Err(Finish::failed(Cause::StepFailed)));
            }
        }
        let row = Published {
            archive_bytes: end.bytes,
            data_bytes: manifest.totals.bytes,
            files: manifest.totals.files,
            flags: manifest.flags.clone(),
            changed_files: changed,
            manifest_sha256: manifest_sha.clone(),
            archive_sha256: end.sha256,
        };
        match self.repo.mark_ready(&job.backup, &row).await {
            Ok(true) => {}
            other => {
                tracing::error!(?other, "sauvegarde publiée mais pas prête au registre");
                journal(self.ops.failed(&job.id, step::COPY).await, "failed");
                // L'archive publiée n'est plus « en cours d'écriture » : elle est retirée avec son inode.
                self.remove_published(job, ident, &manifest_sha).await;
                return CopyEnd::Over(Err(Finish::failed(Cause::StepFailed)));
            }
        }
        self.done(job, step::COPY).await;
        CopyEnd::Over(Ok(Copied {
            mode: job.mode,
            files: manifest.totals.files,
            bytes: manifest.totals.bytes,
            changed: row.changed_files,
            forced_stop: job.forced_stop.load(Ordering::SeqCst),
        }))
    }

    /// Retire SA partie écrite et sa ligne : l'archive en cours, jamais autre chose (`BR-BACKUP-009`).
    async fn discard(&self, job: &Job, ident: (u64, u64)) {
        self.discard_part(job, ident).await;
        if let Err(error) = self.repo.delete(&job.backup).await {
            tracing::warn!(%error, "ligne de sauvegarde interrompue non retirée");
        }
    }

    async fn discard_part(&self, job: &Job, ident: (u64, u64)) {
        let files = Arc::clone(&self.files);
        let (dir, name) = (job.key.to_string(), job.backup.part_name());
        let _ = tokio::task::spawn_blocking(move || files.remove(&dir, &name, ident)).await;
    }

    async fn remove_own_manifest(&self, job: &Job, sha: &str) {
        let files = Arc::clone(&self.files);
        let (dir, name, sha) = (
            job.key.to_string(),
            job.backup.manifest_name(),
            sha.to_owned(),
        );
        let _ = tokio::task::spawn_blocking(move || {
            remove_manifest_if_ours(files.as_ref(), &dir, &name, &sha)
        })
        .await;
    }

    /// Retire une archive publiée et son manifeste (les nôtres), puis sa ligne.
    async fn remove_published(&self, job: &Job, ident: (u64, u64), manifest_sha: &str) {
        let files = Arc::clone(&self.files);
        let (dir, name) = (job.key.to_string(), job.backup.archive_name());
        let _ = tokio::task::spawn_blocking(move || files.remove(&dir, &name, ident)).await;
        self.remove_own_manifest(job, manifest_sha).await;
        if let Err(error) = self.repo.delete(&job.backup).await {
            tracing::warn!(%error, "ligne de sauvegarde non retirée");
        }
    }

    /// La vérification qui suit la création (`BR-BACKUP-013`) : un échec est supprimé par Hearth, c'est son fichier.
    async fn verify_new(&self, job: &Job) -> Result<(), Finish> {
        let Ok(Some(row)) = self.repo.get(&job.backup).await else {
            journal(self.ops.failed(&job.id, step::VERIFY).await, "failed");
            return Err(Finish::failed(Cause::StepFailed));
        };
        let stop = {
            let stopping = Arc::clone(&self.stopping);
            move || stopping.load(Ordering::SeqCst)
        };
        match self.check_row(&row, stop).await {
            Ok(Checked::Verified { .. }) => {
                let at = self.clock.now();
                if let Err(error) = self
                    .repo
                    .set_integrity(&job.backup, BackupIntegrity::Verified, Some(at))
                    .await
                {
                    tracing::warn!(%error, "intégrité non écrite");
                }
                Ok(())
            }
            Ok(Checked::Failed | Checked::Missing) => {
                journal(self.ops.failed(&job.id, step::VERIFY).await, "failed");
                self.remove_published(job, (row.dev, row.ino), &row.manifest_sha256)
                    .await;
                Err(Finish::failed(Cause::BackupCorrupt))
            }
            Err(()) => {
                // La lecture a échoué sans rien dire de l'archive : elle reste, non vérifiée.
                journal(self.ops.failed(&job.id, step::VERIFY).await, "failed");
                Err(Finish::failed(Cause::StepFailed))
            }
        }
    }

    async fn report(&self, job: &Job, control: &Control) {
        let files = control.files.load(Ordering::Relaxed);
        let bytes = control.bytes.load(Ordering::Relaxed);
        let percent = (job.estimate_bytes > 0)
            .then(|| u8::try_from((bytes.saturating_mul(100) / job.estimate_bytes).min(99)).ok())
            .flatten();
        journal(
            self.ops
                .set_progress(
                    &job.id,
                    &Progress {
                        percent,
                        items_done: Some(files),
                        bytes_done: Some(bytes),
                        bytes_total: Some(job.estimate_bytes.max(bytes)),
                    },
                )
                .await,
            "progress",
        );
    }

    // -----------------------------------------------------------------------------------------
    // La fin
    // -----------------------------------------------------------------------------------------

    /// Une annulation arrivée une fois la copie finie : la sauvegarde publiée est « la partie écrite » de ce que le
    /// compte a annulé (le statut « en annulation » n'a qu'une issue, « annulé »). Elle est supprimée : ses fichiers
    /// (les nôtres, revérifiés par leur inode) et sa ligne. Rien d'autre n'est touché.
    async fn cancel_published(&self, job: &Job) {
        match self.repo.get(&job.backup).await {
            Ok(Some(row)) => {
                self.remove_published(job, (row.dev, row.ino), &row.manifest_sha256)
                    .await;
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "sauvegarde annulée tard : ligne illisible"),
        }
    }

    async fn settle(&self, job: &Job, finish: Finish, took: Duration) {
        let cancelling = self.is_cancelling(&job.id).await;
        // Un statut « en annulation » ne devient jamais « échoué » ni « réussi » (diagramme d'états, ADR-0045).
        let finish = match finish.status {
            OpStatus::Failed if cancelling => Finish::cancelled(),
            OpStatus::Succeeded if cancelling => {
                self.cancel_published(job).await;
                Finish::cancelled()
            }
            _ => finish,
        };
        let mut finish = finish;
        let mut result = self
            .ops
            .finish_with(
                &job.id,
                finish.status,
                finish.cause,
                self.draft(job, &finish),
            )
            .await;
        // L'annulation est arrivée entre la dernière relecture du statut et la clôture : même issue.
        if matches!(result, Err(OpError::IllegalTransition | OpError::Stale))
            && finish.status == OpStatus::Succeeded
            && self.is_cancelling(&job.id).await
        {
            self.cancel_published(job).await;
            finish = Finish::cancelled();
            result = self
                .ops
                .finish_with(
                    &job.id,
                    finish.status,
                    finish.cause,
                    self.draft(job, &finish),
                )
                .await;
        }
        match result {
            Ok(()) => {}
            Err(OpError::IllegalTransition | OpError::Stale) => {
                tracing::info!(operation = %job.id, "sauvegarde non close : l'opération a changé de statut entre-temps");
                self.ops.settle(&job.id, None).await;
            }
            Err(error) => {
                tracing::error!(%error, "sauvegarde non close dans le journal");
                self.ops.settle(&job.id, None).await;
            }
        }
        tracing::info!(
            operation = %job.id,
            status = ?finish.status,
            cause = finish.cause.map(Cause::as_str),
            duration_ms = u64::try_from(took.as_millis()).unwrap_or(u64::MAX),
            "sauvegarde terminée"
        );
    }

    fn draft(&self, job: &Job, finish: &Finish) -> Option<AuditDraft> {
        let service = ClientName::parse(&job.name)?;
        let outcome = match (finish.status, finish.cause) {
            (OpStatus::Succeeded, _) => Outcome::Succeeded,
            (_, cause) => Outcome::Failed(cause.map_or(Reason::Internal, reason_of)),
        };
        Some(AuditDraft {
            actor: job.actor.clone(),
            action: AuditAction::ServiceBackupCreate,
            target: Target::ServiceBackup {
                service,
                note: finish.note,
            },
            outcome,
        })
    }

    /// Une panique de l'exécution : l'opération est close par le MÊME chemin que toute clôture. La partie écrite
    /// est retirée au prochain démarrage (journal), la ligne `writing` aussi.
    async fn close_after_panic(&self, job: &Job) {
        let cancelling = self.is_cancelling(&job.id).await;
        let (status, cause) = if cancelling {
            (OpStatus::Cancelled, Cause::CancelledByAccount)
        } else {
            (OpStatus::Failed, Cause::StepFailed)
        };
        // Le service que cette opération avait arrêté est relancé : rien ne le relancerait d'autre.
        let stopped: Vec<String> = job
            .stopped
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if !stopped.is_empty() {
            let _ = self.start_service(job, &stopped).await;
        }
        if let Err(error) = self.repo.delete(&job.backup).await.map(|_| ()) {
            tracing::warn!(%error, "ligne de sauvegarde non retirée après une panique");
        }
        let draft = ClientName::parse(&job.name).map(|service| AuditDraft {
            actor: job.actor.clone(),
            action: AuditAction::ServiceBackupCreate,
            target: Target::ServiceBackup {
                service,
                note: BackupNote::Nothing,
            },
            outcome: if cancelling {
                Outcome::Failed(reason_of(cause))
            } else {
                Outcome::Failed(Reason::Internal)
            },
        });
        if let Err(error) = self
            .ops
            .finish_with(&job.id, status, Some(cause), draft)
            .await
        {
            tracing::error!(%error, "sauvegarde interrompue non close dans le journal");
            self.ops.settle(&job.id, None).await;
        }
    }

    // -----------------------------------------------------------------------------------------
    // Vérifier à la demande
    // -----------------------------------------------------------------------------------------

    /// `POST /backups/{id}/verify` : relit l'archive et la juge (`BR-BACKUP-013`). Le résultat s'écrit dans le
    /// registre ; ni le journal d'activité ni le verrou du service n'y sont (`BR-BACKUP-029`).
    pub async fn verify(&self, id: &BackupId) -> Result<VerifyResult, BackupFailed> {
        let row = self.row(id).await?;
        self.check_overlap()
            .await
            .map_err(|e| BackupFailed::named(e, &row.service_name))?;
        let _in_use = self
            .try_use(id.as_str())
            .ok_or_else(|| BackupFailed::named(BackupError::InUse, &row.service_name))?;
        let stop = {
            let stopping = Arc::clone(&self.stopping);
            move || stopping.load(Ordering::SeqCst)
        };
        let checked = self
            .check_row(&row, stop)
            .await
            .map_err(|()| internal("lecture de l'archive impossible"))?;
        let now = self.clock.now();
        match checked {
            Checked::Verified { files } => {
                self.repo
                    .set_integrity(id, BackupIntegrity::Verified, Some(now))
                    .await
                    .map_err(internal)?;
                Ok(VerifyResult {
                    integrity: BackupIntegrity::Verified,
                    verified_at: Some(rfc3339(now)),
                    files_checked: files,
                })
            }
            Checked::Failed => {
                self.repo
                    .set_integrity(id, BackupIntegrity::Corrupt, None)
                    .await
                    .map_err(internal)?;
                Ok(VerifyResult {
                    integrity: BackupIntegrity::Corrupt,
                    verified_at: None,
                    files_checked: 0,
                })
            }
            Checked::Missing => Ok(VerifyResult {
                integrity: BackupIntegrity::Missing,
                verified_at: None,
                files_checked: 0,
            }),
        }
    }

    async fn row(&self, id: &BackupId) -> Result<BackupRow, BackupFailed> {
        match self.repo.get(id).await.map_err(internal)? {
            Some(row) if row.ready => Ok(row),
            _ => Err(BackupFailed::plain(BackupError::BackupNotFound)),
        }
    }

    /// Juge une archive du registre : propriété (inode), manifeste voisin (empreinte du registre), cohérence du
    /// manifeste, puis relecture complète de l'archive. `Err(())` : la lecture a échoué sans rien dire d'elle.
    async fn check_row(
        &self,
        row: &BackupRow,
        stop: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Result<Checked, ()> {
        let files = Arc::clone(&self.files);
        let row = row.clone();
        let result =
            tokio::task::spawn_blocking(move || check_blocking(files.as_ref(), &row, &stop)).await;
        match result {
            Ok(checked) => checked,
            Err(_) => Err(()),
        }
    }

    // -----------------------------------------------------------------------------------------
    // Supprimer une sauvegarde
    // -----------------------------------------------------------------------------------------

    /// `DELETE /backups/{id}` : retire une sauvegarde de Hearth, JAMAIS autre chose (`BR-BACKUP-004`,
    /// `BR-BACKUP-013`, `BR-BACKUP-027`). Une archive qui n'est plus celle que Hearth a écrite n'est pas touchée :
    /// seule la ligne du registre part.
    pub async fn delete(&self, id: &BackupId, actor: Actor) -> Result<BackupDeleted, BackupFailed> {
        let row = self.row(id).await?;
        let named = |error| BackupFailed::named(error, &row.service_name);
        self.check_overlap().await.map_err(named)?;
        let _in_use = self
            .try_use(id.as_str())
            .ok_or_else(|| named(BackupError::InUse))?;
        let ownership = self.ownership(&row).await?;
        let mut removed = false;
        if ownership == Ownership::Ours {
            let files = Arc::clone(&self.files);
            let (dir, archive, manifest) = (
                row.service_key.to_string(),
                row.id.archive_name(),
                row.id.manifest_name(),
            );
            let (ident, sha) = ((row.dev, row.ino), row.manifest_sha256.clone());
            let outcome = tokio::task::spawn_blocking(move || {
                let outcome = files.remove(&dir, &archive, ident)?;
                // Le manifeste voisin ne part que s'il a l'empreinte du registre.
                if matches!(outcome, BackupRemoved::Done | BackupRemoved::Gone) {
                    remove_manifest_if_ours(files.as_ref(), &dir, &manifest, &sha);
                }
                Ok::<_, BackupFsError>(outcome)
            })
            .await
            .map_err(internal)?;
            match outcome {
                Ok(BackupRemoved::Done) => removed = true,
                Ok(BackupRemoved::Gone | BackupRemoved::NotOurs) => {}
                Err(error) => {
                    tracing::warn!(?error, "archive de sauvegarde non retirée");
                    return Err(internal("archive non retirée"));
                }
            }
        }
        self.repo.delete(id).await.map_err(internal)?;
        let files = Arc::clone(&self.files);
        let dir = row.service_key.to_string();
        let _ = tokio::task::spawn_blocking(move || files.remove_service_dir_if_empty(&dir)).await;
        if let Some(service) = ClientName::parse(&row.service_name) {
            self.sink
                .record(
                    actor,
                    AuditAction::ServiceBackupDelete,
                    Target::ServiceBackup {
                        service,
                        note: BackupNote::Deleted {
                            bytes: row.data_bytes,
                            registry_only: !removed,
                        },
                    },
                    Outcome::Succeeded,
                )
                .await;
        }
        Ok(BackupDeleted {
            removed_files: removed,
        })
    }

    // -----------------------------------------------------------------------------------------
    // Les éléments ignorés
    // -----------------------------------------------------------------------------------------

    /// `PUT /services/{key}/backup-settings` : les éléments ignorés (`BR-BACKUP-003`). Le changement est inscrit
    /// au journal avec le NOMBRE d'éléments, jamais les chemins.
    pub async fn set_settings(
        &self,
        key: &ServiceKey,
        settings: BackupSettings,
        actor: Actor,
    ) -> Result<BackupSettings, BackupFailed> {
        let subject = self.subject(key).await?;
        let named = |error| BackupFailed::named(error, &subject.name);
        validate_items(&settings.ignored).map_err(named)?;
        validate_items(&settings.included).map_err(named)?;
        self.repo
            .save_settings(key, &settings, &account_of(&actor), self.clock.now())
            .await
            .map_err(internal)?;
        if let Some(service) = ClientName::parse(&subject.name) {
            self.sink
                .record(
                    actor,
                    AuditAction::ServiceBackupSettings,
                    Target::ServiceBackup {
                        service,
                        note: BackupNote::Settings {
                            ignored: u32::try_from(settings.ignored.len()).unwrap_or(u32::MAX),
                            included: u32::try_from(settings.included.len()).unwrap_or(u32::MAX),
                        },
                    },
                    Outcome::Succeeded,
                )
                .await;
        }
        Ok(settings)
    }
}

/// Ce que la copie a produit, pour la note du journal.
struct Copied {
    mode: BackupMode,
    files: u64,
    bytes: u64,
    changed: u32,
    forced_stop: bool,
}

impl Copied {
    fn note(&self, _job: &Job) -> BackupNote {
        BackupNote::Created {
            mode: match self.mode {
                BackupMode::Hot => BackupModeNote::Hot,
                BackupMode::StopCopyStart => BackupModeNote::StopCopyStart,
                // Un mode inconnu n'existe pas ici : il est refusé à la demande, jamais enregistré.
                _ => BackupModeNote::Stopped,
            },
            files: self.files,
            bytes: self.bytes,
            forced_stop: self.forced_stop,
            changed: self.changed,
        }
    }
}

enum CopyEnd {
    Halted,
    Over(Result<Copied, Finish>),
}

/// Ce qu'une relecture a conclu.
#[derive(Debug)]
enum Checked {
    Verified {
        files: u64,
    },
    /// L'archive n'est pas ce que Hearth a écrit : remplacée, manifeste étranger ou altéré, contenu qui diffère.
    Failed,
    /// Le fichier n'existe plus.
    Missing,
}

/// La relecture complète d'une archive du registre, sur un fil bloquant.
fn check_blocking(
    files: &dyn BackupFiles,
    row: &BackupRow,
    stop: &dyn Fn() -> bool,
) -> Result<Checked, ()> {
    let dir = row.service_key.to_string();
    let registered = registered_of(row);
    let seen = files.seen(&dir, &row.id.archive_name()).map_err(|_| ())?;
    match judge_archive(&registered, &seen) {
        Ownership::Missing => return Ok(Checked::Missing),
        Ownership::Replaced => return Ok(Checked::Failed),
        Ownership::Ours => {}
    }
    // Le manifeste voisin n'est cru que s'il a l'empreinte du registre.
    let Ok((mut reader, _)) = files.open(&dir, &row.id.manifest_name()) else {
        return Ok(Checked::Failed);
    };
    let mut bytes = Vec::new();
    if (&mut reader)
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
        || u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_MANIFEST_BYTES
    {
        return Ok(Checked::Failed);
    }
    if !manifest_is_ours(&registered, Some(&sha256_hex(&bytes))) {
        return Ok(Checked::Failed);
    }
    let Ok(manifest) = serde_json::from_slice::<Manifest>(&bytes) else {
        return Ok(Checked::Failed);
    };
    let Some(facts) = manifest.archive.clone() else {
        return Ok(Checked::Failed);
    };
    if manifest.check().is_err()
        || manifest.backup_id != row.id.as_str()
        || facts.sha256 != row.archive_sha256
        || facts.bytes != row.archive_bytes
    {
        return Ok(Checked::Failed);
    }
    let (archive, ident) = files.open(&dir, &row.id.archive_name()).map_err(|_| ())?;
    // Le fichier ouvert est celui du registre (revérifié sur le descripteur).
    if ident != (row.dev, row.ino) {
        return Ok(Checked::Failed);
    }
    match verify_archive(archive, &manifest, &facts.sha256, stop) {
        Ok(report) => Ok(Checked::Verified {
            files: report.files_checked,
        }),
        Err(VerifyError::Corrupt) => Ok(Checked::Failed),
        Err(VerifyError::Io(_) | VerifyError::Stopped) => Err(()),
    }
}

/// Retire le manifeste voisin SI son empreinte est celle du registre (`BR-BACKUP-004`).
fn remove_manifest_if_ours(files: &dyn BackupFiles, dir: &str, name: &str, sha: &str) {
    let Ok((mut reader, ident)) = files.open(dir, name) else {
        return;
    };
    let mut bytes = Vec::new();
    if (&mut reader)
        .take(MAX_MANIFEST_BYTES)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return;
    }
    if sha256_hex(&bytes) == sha {
        let _ = files.remove(dir, name, ident);
    }
}

fn registered_of(row: &BackupRow) -> Registered {
    Registered {
        dev: row.dev,
        ino: row.ino,
        manifest_sha256: row.manifest_sha256.clone(),
    }
}

fn note_field(backup: &BackupId) -> Option<SecretField> {
    AnswerId::parse(&backup.as_str().to_lowercase()).map(SecretField::BackupNote)
}

fn open_note(
    sealer: &dyn crate::application::ports::SecretSealer,
    row: &BackupRow,
    blob: &crate::domain::sealed::SealedBlob,
) -> Option<String> {
    let field = note_field(&row.id)?;
    let plain = sealer
        .open(
            &SealContext {
                service: row.service_key.as_str(),
                rev: 0,
                field: &field,
            },
            blob,
        )
        .ok()?;
    String::from_utf8(plain.expose().to_vec()).ok()
}

fn rfc3339(at: OffsetDateTime) -> String {
    at.format(&Rfc3339).unwrap_or_default()
}

fn is_running(state: Option<ServiceStateKind>) -> bool {
    matches!(
        state,
        Some(ServiceStateKind::Running | ServiceStateKind::RestartLoop)
    )
}

/// Les états d'un service que l'on sauvegarde : en marche, arrêté, en erreur, ou qui redémarre en boucle. Un
/// déploiement en cours, un arrêt ou un démarrage en cours, une pause, un état inconnu : non.
fn backable(state: ServiceStateKind) -> bool {
    matches!(
        state,
        ServiceStateKind::Running
            | ServiceStateKind::Stopped
            | ServiceStateKind::Error
            | ServiceStateKind::RestartLoop
    )
}

/// La note : 80 caractères au plus, sans caractère de contrôle, rognée ; vide = pas de note.
fn clean_note(note: Option<&str>) -> Result<Option<String>, BackupError> {
    let Some(note) = note.map(str::trim).filter(|note| !note.is_empty()) else {
        return Ok(None);
    };
    if note.chars().count() > NOTE_MAX_CHARS {
        return Err(BackupError::Invalid {
            field: "note",
            reason: "too_long",
        });
    }
    if note.chars().any(char::is_control) {
        return Err(BackupError::Invalid {
            field: "note",
            reason: "invalid",
        });
    }
    Ok(Some(note.to_owned()))
}

/// Une liste d'éléments ignorés : bornée, chaque chemin relatif et sûr.
fn validate_items(items: &[IgnoredItem]) -> Result<(), BackupError> {
    if items.len() > MAX_ITEMS {
        return Err(BackupError::Invalid {
            field: "items",
            reason: "too_many",
        });
    }
    for item in items {
        let root_ok = !item.root.is_empty()
            && item.root.len() <= 4096
            && !item.root.chars().any(char::is_control);
        let path_ok = item.path.is_empty() || RelPath::parse(&item.path).is_ok();
        if !root_ok || !path_ok {
            return Err(BackupError::Invalid {
                field: "items",
                reason: "invalid",
            });
        }
    }
    Ok(())
}

fn cause_of_fs(error: BackupFsError) -> Cause {
    match error {
        BackupFsError::Io(io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded) => {
            Cause::DiskAlmostFull
        }
        _ => Cause::StepFailed,
    }
}

fn journal<T>(result: Result<T, OpError>, what: &'static str) {
    if let Err(error) = result {
        tracing::warn!(%error, write = what, "journal de la sauvegarde non écrit");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_is_trimmed_bounded_and_free_of_control_characters() {
        assert_eq!(clean_note(None).unwrap(), None);
        assert_eq!(clean_note(Some("   ")).unwrap(), None);
        assert_eq!(
            clean_note(Some("  Avant mise à jour 1.4 "))
                .unwrap()
                .as_deref(),
            Some("Avant mise à jour 1.4")
        );
        assert!(clean_note(Some(&"x".repeat(81))).is_err());
        assert!(clean_note(Some(&"x".repeat(80))).is_ok());
        assert!(clean_note(Some("a\nb")).is_err());
    }

    #[test]
    fn the_ignored_lists_are_bounded_and_their_paths_stay_relative() {
        let item = |root: &str, path: &str| IgnoredItem {
            root: root.into(),
            path: path.into(),
        };
        assert!(validate_items(&[item("/data", ""), item("/data", "logs/old")]).is_ok());
        for bad in [
            item("", "a"),
            item("/data", "../etc"),
            item("/data", "/abs"),
            item("/data", "a//b"),
            item("/da\nta", "a"),
        ] {
            assert!(validate_items(&[bad]).is_err());
        }
        let many = vec![item("/data", "a"); MAX_ITEMS + 1];
        assert!(validate_items(&many).is_err());
    }

    #[test]
    fn only_running_stopped_failed_and_looping_services_are_backed_up() {
        for state in [
            ServiceStateKind::Running,
            ServiceStateKind::Stopped,
            ServiceStateKind::Error,
            ServiceStateKind::RestartLoop,
        ] {
            assert!(backable(state), "{state:?}");
        }
        for state in [
            ServiceStateKind::Deploying,
            ServiceStateKind::Stopping,
            ServiceStateKind::Starting,
            ServiceStateKind::Paused,
            ServiceStateKind::Unknown,
        ] {
            assert!(!backable(state), "{state:?}");
        }
    }

    #[test]
    fn a_hot_copy_with_a_changed_file_may_be_inconsistent_and_a_forced_stop_always_is() {
        assert_eq!(flags_of(true, false, 0), [BackupFlag::Hot]);
        assert_eq!(
            flags_of(true, false, 2),
            [BackupFlag::Hot, BackupFlag::MaybeInconsistent]
        );
        assert_eq!(
            flags_of(false, true, 0),
            [BackupFlag::ForcedStop, BackupFlag::MaybeInconsistent]
        );
        assert!(flags_of(false, false, 5).is_empty());
    }

    #[test]
    fn the_note_field_is_lower_case_and_bound_to_the_backup() {
        let id = BackupId::parse("01J9ZY0G3Q8M2K6W4T7V5N1B9D").expect("id");
        let field = note_field(&id).expect("champ");
        assert_eq!(field.as_text(), "backup-note:01j9zy0g3q8m2k6w4t7v5n1b9d");
        assert_eq!(SecretField::parse(&field.as_text()), Some(field));
    }
}
