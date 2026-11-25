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
    BIG_FILE_BYTES, BackupAccepted, BackupDeleted, BackupIntegrity, BackupMode, BackupPreview,
    BackupRequest, BackupSettings, BackupView, BackupsView, BigFile, ESTIMATE_BUDGET_S,
    IgnoredItem, IgnoredView, IgnoredWhy, MAX_LISTED, VerifyResult,
};
use hearth_proto::api::mods::{
    BackupModItem, BackupModPlace, BackupModsCompare, BackupModsCounts, BackupModsGame,
    BackupModsSummary, BackupModsView, ChangeKindView, EnvironmentView, ModChange, ModIdentityView,
    ModOrigin, ModsListState,
};
use thiserror::Error;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::sync::Mutex as AsyncMutex;

use super::ports::{
    ArchiveBuilder, ArchiveEnd, AuditSink, BackupArchive, BackupFiles, BackupFsError,
    BackupRemoved, BackupRepo, BackupRow, BackupSink, Clock, ContainerEngine, DiskSpace,
    EngineError, EntryKind, EntryMeta, Expected, FileStamp, FsError, HostPaths, IdGen, ModsReading,
    ModsRecord, ModsSnapshot, ModsSummaryRow, NewBackup, Progress, Published, Publishing, ReadFile,
    Resolved, SafeFs, ServiceContainers, ServiceIntents, ServiceRoot, VerifyError,
};
use super::service_lifecycle::reason_of;
use super::service_ops::{AcceptRequest, AuditDraft, OpError, ServiceOps};
use super::service_specs::ServiceSpecs;
use super::service_stop::{
    StepShown, StopContext, StopRequest, account_of, forget_stop, stop_running,
};
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
use crate::domain::backups::rules::{
    ItemsError, NoteError, backable, clean_note, flags_of, is_running, validate_items,
};
use crate::domain::backups::space::{
    CHECK_EVERY_BYTES, MAX_COPY, SpaceVerdict, judge, too_low_during_copy,
};
use crate::domain::backups::{BackupId, archive_dir_name, sha256_hex};
use crate::domain::files::{MAX_DEPTH, RelPath};
use crate::domain::mods::backup_list::{self, ManifestFile, RecordedSet};
use crate::domain::ops::CreatedObject;
use crate::domain::ops::backup::{ModeVerdict, effective_mode, op_plan, step};
use crate::domain::ops::{
    Cause, Holder, ObjectKind, ObjectRoot, OpKind, OpStatus, Refusal, UndoInfo, WAIT_FLOOR,
    WAIT_MARGIN,
};
use crate::domain::sealed::{AnswerId, SealContext, SecretField};
use crate::domain::services::delete::{ManagedFolder, split_mounts};
use crate::domain::services::plan::image_reference;
use crate::domain::services::spec::{SpecBody, SpecMountKind};
use crate::domain::services::{EngineState, KeyKind, ServiceKey, ServiceStateKind, TransitionKind};

/// Au plus ce temps pour lire un chemin de la racine des sauvegardes (un partage mort ne bloque pas la requête).
const PATH_FACTS_TIMEOUT: Duration = Duration::from_secs(5);

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
    /// Le service n'a pas de liste de mods (fiche sans `[mods.jars]` ou moteur sans mods).
    #[error("ce service n'a pas de mods")]
    ModsNotApplicable,
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
    /// Les éléments que la copie n'archive pas (tubes, sockets, autre montage, nom illisible, trop profond...).
    skipped: AtomicU32,
    deadline: Instant,
}

/// Demande l'arrêt de la copie quand la tâche qui la suit disparaît, quelle que soit la façon dont elle sort.
struct HaltOnDrop(Arc<Control>);

impl Drop for HaltOnDrop {
    fn drop(&mut self) {
        self.0.ask(STOP_SHUTDOWN);
    }
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
        /// Les éléments non archivés.
        skipped: u32,
    },
    /// Arrêtée avant la fin : `STOP_*`.
    Stopped(u8),
    Failed(Cause),
}

struct CopyJob {
    fs: Arc<dyn SafeFs>,
    files: Arc<dyn BackupFiles>,
    archive: Arc<dyn BackupArchive>,
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
    writer: Box<dyn ArchiveBuilder>,
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
    /// Des éléments que la copie n'archive pas : comptés, jamais omis en silence.
    fn skip(&self, count: u32) {
        self.control.skipped.fetch_add(count, Ordering::Relaxed);
    }

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
            Err(FsError::NotFound | FsError::Blocked | FsError::NotRegular) => {
                // Un dossier devenu un lien ou disparu entre le listage de son parent et son ouverture.
                self.skip(1);
                return Ok(());
            }
            Err(_) => return Err(Halt::Fail(Cause::StepFailed)),
        };
        self.skip(listing.skipped_names);
        for entry in listing.entries {
            self.check()?;
            let path = within_path(within, &entry.name);
            if rules.judge(&folder.target, &path, &entry.name).is_some() {
                self.ignored.push(ManifestIgnored { root: id, path });
                continue;
            }
            let Some(child) = join(rel, &entry.name) else {
                self.skip(1);
                continue;
            };
            let tar = tar_path(dir_name, &path);
            match entry.meta.kind {
                EntryKind::Dir => {
                    if depth >= MAX_DEPTH {
                        self.skip(1);
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
                        _ => {
                            self.skip(1);
                            continue;
                        }
                    };
                    self.writer
                        .symlink(&tar, &entry.meta, &target)
                        .map_err(|e| Halt::Fail(cause_of_io(&e)))?;
                    let mut item = entry_from(id, &path, EntryType::Symlink, &entry.meta);
                    item.link_target = Some(target);
                    self.entries.push(item);
                }
                // Un tube, un socket, un périphérique, un autre montage : ni lu ni archivé, mais compté.
                EntryKind::Other => self.skip(1),
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
                self.skip(1);
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
        archive,
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
    let mut walk = Walk {
        root: root.as_ref(),
        writer: archive.builder(sink),
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
    let skipped = control.skipped.load(Ordering::Relaxed);
    base.skipped = skipped;
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
            skipped,
        },
        Err(error) => CopyOutcome::Failed(cause_of_io(&error)),
    }
}

// ---------------------------------------------------------------------------------------------
// Le cas d'usage
// ---------------------------------------------------------------------------------------------

/// L'issue du cœur d'une sauvegarde (`Backups::copy_and_verify`).
pub(crate) enum CopyVerified {
    Over(Finish),
    Halted,
}

/// Ce que l'exécution décide de son issue.
#[derive(Debug)]
pub(crate) struct Finish {
    status: OpStatus,
    cause: Option<Cause>,
    note: BackupNote,
}

impl Finish {
    /// L'issue que le cœur de la sauvegarde a décidée (HRT-258 : l'opération `mods` la reprend à son compte).
    pub(crate) fn status(&self) -> OpStatus {
        self.status
    }

    pub(crate) fn cause(&self) -> Option<Cause> {
        self.cause
    }

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
pub(crate) struct Job {
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
    /// La lecture des mods prise AVANT l'arrêt et la copie (HRT-293) ; consommée par la publication.
    mods_read: Mutex<Option<ModsReading>>,
}

impl Job {
    /// L'identifiant de l'opération qui porte cette sauvegarde.
    pub(crate) fn op_id(&self) -> &str {
        &self.id
    }

    pub(crate) fn backup_id(&self) -> &BackupId {
        &self.backup
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn actor(&self) -> &Actor {
        &self.actor
    }

    /// Les conteneurs que l'opération a arrêtés (à relancer).
    pub(crate) fn stopped_containers(&self) -> Vec<String> {
        self.stopped
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// Ce que la partie synchrone d'un lot de mods a décidé pour sa sauvegarde « Avant mods » (HRT-258).
pub(crate) struct ModsPrepared {
    pub(crate) name: String,
    /// Le service tournait au moment du jugement.
    pub(crate) running: bool,
    containers: Vec<String>,
    grace_s: u32,
    folders: Vec<ManagedFolder>,
    rules: IgnoreRules,
    image: String,
    note: Option<crate::domain::sealed::SealedBlob>,
    estimate_bytes: u64,
    backup: BackupId,
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
    archive: Arc<dyn BackupArchive>,
    repo: Arc<dyn BackupRepo>,
    host_paths: Arc<dyn HostPaths>,
    sink: Arc<dyn AuditSink>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGen>,
    context: BackupContext,
    stopping: Arc<AtomicBool>,
    /// Une estimation à la fois, une relecture d'archive à la fois pour tout l'agent : le jeton part AVEC le fil
    /// bloquant (il n'est rendu que quand le travail est fini, même si la demande a été abandonnée).
    estimate_gate: Arc<AsyncMutex<()>>,
    verify_gate: Arc<AsyncMutex<()>>,
    /// Les sauvegardes qu'une opération utilise (création, vérification) : une suppression est refusée tant
    /// qu'elles y sont (`BR-BACKUP-027`).
    in_use: Mutex<HashSet<String>>,
    /// La lecture des mods d'un service (HRT-293) : posée par la racine de composition ; sans elle, aucune liste.
    mods: std::sync::OnceLock<std::sync::Weak<dyn ModsSnapshot>>,
    /// Le même port quand rien d'autre ne le tient en vie (un faux des essais) ; jamais le service des mods réel.
    mods_kept: std::sync::OnceLock<Arc<dyn ModsSnapshot>>,
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
        archive: Arc<dyn BackupArchive>,
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
            archive,
            repo,
            host_paths,
            sink,
            clock,
            ids,
            context,
            stopping: Arc::new(AtomicBool::new(false)),
            estimate_gate: Arc::new(AsyncMutex::new(())),
            verify_gate: Arc::new(AsyncMutex::new(())),
            in_use: Mutex::new(HashSet::new()),
            mods: std::sync::OnceLock::new(),
            mods_kept: std::sync::OnceLock::new(),
        })
    }

    /// L'agent s'arrête : une copie en cours s'arrête, son opération reste en cours dans le journal et sera
    /// « interrompue » au prochain démarrage (sa partie écrite est retirée alors, jamais reprise).
    pub fn begin_shutdown(&self) {
        self.stopping.store(true, Ordering::SeqCst);
    }

    /// Au démarrage, après la reprise des opérations : chaque ligne `writing` du registre est une archive que l'agent
    /// n'a pas fini d'écrire. Ses fichiers (la partie par inode, l'archive publiée par inode et taille, le manifeste
    /// par empreinte : la ligne les connaît avant leur écriture) sont retirés, puis la ligne ; jamais rien d'autre.
    /// La racine est jugée et le résultat DIT : une racine qui chevauche un dossier de données refuse la fonction
    /// (`BR-BACKUP-004` (b)).
    pub async fn recover_at_startup(&self) {
        match self.repo.writing().await {
            Ok(rows) if rows.is_empty() => {}
            Ok(rows) => {
                let mut kept = 0_u64;
                for row in &rows {
                    if !self.purge(row).await {
                        kept += 1;
                    }
                }
                tracing::warn!(
                    count = rows.len(),
                    kept,
                    "sauvegardes interrompues par l'arrêt de l'agent : fichiers de Hearth et lignes retirés"
                );
            }
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
        let work = tokio::task::spawn_blocking(move || {
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
        });
        // Un partage réseau mort ne fait pas pendre la requête : au-delà du délai, on ne sait pas (`None`, jamais un feu
        // vert), comme `fate_now` de la suppression. La lecture bloquée finit seule dans son fil.
        tokio::time::timeout(PATH_FACTS_TIMEOUT, work)
            .await
            .ok()?
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
        let gate = Arc::clone(&self.estimate_gate).lock_owned().await;
        let result = tokio::task::spawn_blocking(move || {
            let _gate = gate;
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
            skipped: row.skipped,
            note,
            account: row.created_by.clone(),
            image: row.image.clone(),
            integrity,
            verified_at: row.verified_at.map(rfc3339),
            mods: None,
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
        // Un service qui a des mods : chaque sauvegarde dit l'état de sa liste (BR-MOD-035), les anciennes
        // « non enregistrée ». Un service sans mods : aucun champ.
        let summaries = match self.mods_port() {
            Some(port) if port.applicable(key).await => {
                Some(self.repo.mods_summaries(key).await.map_err(internal)?)
            }
            _ => None,
        };
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
            let mut view = Self::view_of(row, integrity, note);
            if let Some(summaries) = &summaries {
                view.mods = Some(summary_view(summaries.get(row.id.as_str())));
            }
            views.push(view);
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
        let note = clean_note(request.note.as_deref())
            .map_err(note_error)
            .map_err(named)?;
        validate_items(&request.ignore_once)
            .map_err(items_error)
            .map_err(named)?;
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
        // HRT-292 : avant l'arrêt de la sauvegarde avec arrêt, le monde est enregistré par la console si la fiche le déclare.
        let plan = if mode == BackupMode::StopCopyStart {
            self.ops.plan_before_stop(key, plan).await
        } else {
            plan
        };
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
            mods_read: Mutex::new(None),
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

    /// La partie synchrone de la sauvegarde « Avant mods » d'un lot (HRT-258, BR-MOD-026) : les mêmes refus que
    /// `create` (moteur, service adopté ou inconnu, opération en cours, état, racine, dossiers, espace pour la
    /// sauvegarde PLUS `extra_bytes`, la taille des dépôts), sans rien accepter ni écrire.
    pub(crate) async fn prepare_for_mods(
        &self,
        key: &ServiceKey,
        extra_bytes: u64,
    ) -> Result<ModsPrepared, BackupFailed> {
        self.ops.wait_closed(key).await;
        let subject = self.subject(key).await?;
        let named = |error| BackupFailed::named(error, &subject.name);
        if let Some(busy) = self.busy_of(key) {
            return Err(named(busy));
        }
        let running = is_running(subject.state);
        if let Some(state) = subject.state
            && !backable(state)
        {
            return Err(named(BackupError::NotBackable(state)));
        }
        self.check_root().await.map_err(named)?;
        let (folders, _) = self.folders_of(key, subject.body.as_ref());
        if folders.is_empty() {
            return Err(named(BackupError::NothingToSave));
        }
        let rules = self.rules_of(key, Vec::new()).await?;
        let estimate = self.estimate(key, &folders, &rules).await?;
        let Some(space) = self.space().await else {
            return Err(named(BackupError::Unavailable));
        };
        if let SpaceVerdict::Refused {
            needed,
            free,
            reserve,
        } = judge(
            estimate.bytes.saturating_add(extra_bytes),
            space.free,
            space.total,
        ) {
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
        let day: String = rfc3339(self.clock.now()).chars().take(10).collect();
        let sealed = self
            .seal_note(key, &backup, &format!("Avant mods, {day}"))
            .await
            .map_err(named)?;
        Ok(ModsPrepared {
            name: subject.name.clone(),
            running,
            containers: subject.containers,
            grace_s: subject.grace_s,
            folders,
            rules,
            image: subject
                .body
                .as_ref()
                .map(|body| image_reference(&body.image))
                .unwrap_or_default(),
            note: Some(sealed),
            estimate_bytes: estimate.bytes,
            backup,
        })
    }

    /// Accepte l'opération `mods` (verrou exclusif du service ET place `heavy_io`) puis la démarre ; rend la tâche de
    /// sauvegarde que le plan de l'opération porte. Les refus de verrou sont ceux d'une sauvegarde.
    pub(crate) async fn accept_for_mods(
        &self,
        key: &ServiceKey,
        prepared: ModsPrepared,
        actor: Actor,
        op_key: &str,
        plan: crate::domain::ops::OpPlan,
    ) -> Result<Arc<Job>, BackupFailed> {
        let named = |error| BackupFailed::named(error, &prepared.name);
        let op = self
            .ops
            .accept_after_wait(AcceptRequest {
                op_key: op_key.to_owned(),
                service: key.clone(),
                service_name: prepared.name.clone(),
                kind: OpKind::Mods,
                account: account_of(&actor),
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
                        Holder::FileWrites => OpKind::Mods,
                    },
                }),
                OpError::ServiceBusy => named(BackupError::Busy {
                    by: self.holder_account(key),
                    kind: OpKind::Mods,
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
        let mode = if prepared.running {
            BackupMode::StopCopyStart
        } else {
            BackupMode::Stopped
        };
        Ok(Arc::new(Job {
            id: op,
            key: key.clone(),
            name: prepared.name,
            actor,
            backup: prepared.backup,
            mode,
            containers: prepared.containers,
            grace_s: prepared.grace_s,
            folders: prepared.folders,
            rules: prepared.rules,
            image: prepared.image,
            note: prepared.note,
            estimate_bytes: prepared.estimate_bytes,
            stopped: Mutex::new(Vec::new()),
            forced_stop: AtomicBool::new(false),
            mods_read: Mutex::new(None),
        }))
    }

    /// Les conteneurs consignés d'un service (HRT-259 : le retour d'un lot « à vérifier » les arrête puis les relance).
    pub(crate) async fn containers_for(&self, key: &ServiceKey) -> Vec<String> {
        self.containers.of(key).await.unwrap_or_default()
    }

    /// Un « travail » sans sauvegarde pour le retour d'un lot « à vérifier » (HRT-259) : il porte seulement ce que
    /// l'arrêt voulu et la relance utilisent (conteneurs, compte). `stopped` : les conteneurs à relancer ensuite.
    pub(crate) fn revert_job(
        &self,
        key: &ServiceKey,
        op_id: &str,
        name: &str,
        account: &str,
        containers: Vec<String>,
    ) -> Option<Job> {
        let backup = BackupId::parse(&self.ids.new_id())?;
        let actor = Actor::new(
            crate::domain::accounts::Username::parse(account).ok(),
            crate::domain::audit::Origin::System,
        );
        Some(Job {
            id: op_id.to_owned(),
            key: key.clone(),
            name: name.to_owned(),
            actor,
            backup,
            mode: BackupMode::StopCopyStart,
            containers: containers.clone(),
            grace_s: 30,
            folders: Vec::new(),
            rules: IgnoreRules::default(),
            image: String::new(),
            note: None,
            estimate_bytes: 0,
            stopped: Mutex::new(containers),
            forced_stop: AtomicBool::new(false),
            mods_read: Mutex::new(None),
        })
    }

    /// Lit la liste des mods AVANT l'arrêt et la copie (HRT-293), comme une sauvegarde.
    pub(crate) async fn read_mods_for(&self, job: &Job) {
        self.read_mods(job).await;
    }

    /// Arrête les conteneurs du service (l'étape `stop` de l'opération `mods`) : même code qu'une sauvegarde.
    pub(crate) async fn stop_for_mods(&self, job: &Job) -> Result<(), Finish> {
        self.stop_service(job).await
    }

    /// Arrête de nouveau le service relancé avec de mauvais mods, avant le retour des fichiers (HRT-259) : l'arrêt voulu
    /// est INSCRIT (base et registre) comme pour le premier arrêt, pour que le service ne passe pas par « En erreur » et
    /// ne soit pas relancé par sa règle de redémarrage. `false` : un conteneur ne s'est pas arrêté.
    pub(crate) async fn stop_again_for_mods(&self, job: &Job) -> bool {
        let context = StopContext {
            registry: &self.registry,
            ops: &self.ops,
            engine: self.engine.as_ref(),
            intents: self.intents.as_ref(),
            clock: self.clock.as_ref(),
        };
        let request = StopRequest {
            key: &job.key,
            operation: &job.id,
            step: step::STOP,
            containers: &job.containers,
            grace_s: job.grace_s,
            account: account_of(&job.actor),
            margin: self.context.timings.stop_margin,
            floor: self.context.timings.stop_floor,
        };
        stop_running(&context, &request).await.is_ok()
    }

    /// Relance ce que l'opération a arrêté (HRT-258) ; `Err` : un conteneur n'est pas reparti.
    pub(crate) async fn start_for_mods(&self, job: &Job) -> Result<(), ()> {
        let stopped = job.stopped_containers();
        if stopped.is_empty() {
            return Ok(());
        }
        self.start_service(job, &stopped).await
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
        // 0. La liste des mods, lue AVANT l'arrêt (le service tourne encore : lecture seule, aucune minute d'arrêt de
        // plus) et avant la copie (HRT-293, BR-MOD-034).
        self.read_mods(job).await;
        // 1. Arrêt, seulement dans ce mode.
        if job.mode == BackupMode::StopCopyStart {
            self.ops.console_save(&job.id, &job.key).await;
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
        // 2 et 3. La copie puis la vérification (cœur partagé : `copy_and_verify`).
        let finish = match self.copy_and_verify(job).await {
            CopyVerified::Halted => return None,
            CopyVerified::Over(finish) => finish,
        };
        Some(self.finish_after(job, finish).await)
    }

    /// Le cœur d'une sauvegarde, SANS verrou ni arrêt ni relance : crée l'archive, la copie, la publie (ligne
    /// `ready`), puis la relit (`BR-BACKUP-009`, `010`, `013`, `015`, `025`, `032`). Une autre opération du service
    /// (HRT-255, `mods` : « Avant mods ») l'appelle une fois son propre verrou pris et son service arrêté ; le
    /// service est relancé par l'appelant. `Over(finish)` : l'issue à rendre (la partie écrite d'un échec est déjà
    /// retirée, la relance et le règlement restent à l'appelant) ; `Halted` : l'agent s'arrête.
    pub(crate) async fn copy_and_verify(&self, job: &Job) -> CopyVerified {
        // 2. La copie.
        let copied = match self.copy(job).await {
            CopyEnd::Halted => return CopyVerified::Halted,
            CopyEnd::Over(Err(finish)) => return CopyVerified::Over(finish),
            CopyEnd::Over(Ok(copied)) => copied,
        };
        // 3. La vérification.
        if let Err(finish) = self.begin(job, step::VERIFY, &UndoInfo::default()).await {
            return CopyVerified::Over(finish);
        }
        let verified = self.verify_new(job).await;
        let finish = match verified {
            Ok(()) => {
                self.done(job, step::VERIFY).await;
                Finish {
                    status: OpStatus::Succeeded,
                    cause: None,
                    note: copied.note(),
                }
            }
            Err(Some(finish)) => finish,
            // L'agent s'arrête pendant la relecture : l'opération reste en cours (interrompue au prochain
            // démarrage), l'archive reste publiée, non vérifiée.
            Err(None) => return CopyVerified::Halted,
        };
        CopyVerified::Over(finish)
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

    /// Arrête les conteneurs du service qui tournent (`application::service_stop`, le même code que la
    /// suppression) : l'arrêt voulu est écrit AVANT l'ordre, le délai de grâce est celui du service, une coupure
    /// forcée est dite (`BR-BACKUP-008`). Ce qui est arrêté est noté pour être relancé, même après un échec.
    async fn stop_service(&self, job: &Job) -> Result<(), Finish> {
        let context = StopContext {
            registry: &self.registry,
            ops: &self.ops,
            engine: self.engine.as_ref(),
            intents: self.intents.as_ref(),
            clock: self.clock.as_ref(),
        };
        let request = StopRequest {
            key: &job.key,
            operation: &job.id,
            step: step::STOP,
            containers: &job.containers,
            grace_s: job.grace_s,
            account: account_of(&job.actor),
            margin: self.context.timings.stop_margin,
            floor: self.context.timings.stop_floor,
        };
        let (stopped, forced, failure) = match stop_running(&context, &request).await {
            Ok(done) => (done.containers, done.forced, None),
            Err(failure) => (failure.stopped, failure.forced, Some(failure.cause)),
        };
        job.stopped
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(stopped);
        if forced {
            job.forced_stop.store(true, Ordering::SeqCst);
        }
        match failure {
            None => Ok(()),
            Some(cause) => Err(Finish::failed(cause)),
        }
    }

    async fn forget_stop(&self, key: &ServiceKey) {
        forget_stop(self.intents.as_ref(), &self.registry, key).await;
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
            ..UndoInfo::default()
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
            skipped: AtomicU32::new(0),
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
            skipped: 0,
            archive: None,
        };
        let copy = CopyJob {
            fs: Arc::clone(&self.safe_fs),
            files: Arc::clone(&self.files),
            archive: Arc::clone(&self.archive),
            key: job.key.clone(),
            folders: job.folders.clone(),
            rules: job.rules.clone(),
            sink,
            control: Arc::clone(&control),
            base,
            forced_stop: job.forced_stop.load(Ordering::SeqCst),
            hot: job.mode == BackupMode::Hot,
        };
        // Si cette tâche s'arrête (panique, abandon), le fil de copie reçoit l'ordre de s'arrêter : il n'écrit pas
        // jusqu'à six heures dans une partie que plus personne ne suit.
        let _halt = HaltOnDrop(Arc::clone(&control));
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
                skipped,
            } => {
                self.publish(job, ident, *manifest, end, (changed, skipped))
                    .await
            }
            CopyOutcome::Stopped(STOP_SHUTDOWN) => CopyEnd::Halted,
            CopyOutcome::Stopped(reason) => {
                journal(self.ops.failed(&job.id, step::COPY).await, "failed");
                self.purge_job(job, ident).await;
                CopyEnd::Over(Err(match reason {
                    STOP_CANCEL => Finish::cancelled(),
                    STOP_DISK => Finish::failed(Cause::DiskAlmostFull),
                    _ => Finish::failed(Cause::Timeout),
                }))
            }
            CopyOutcome::Failed(cause) => {
                journal(self.ops.failed(&job.id, step::COPY).await, "failed");
                self.purge_job(job, ident).await;
                CopyEnd::Over(Err(Finish::failed(cause)))
            }
        }
    }

    /// Le manifeste voisin, l'archive sous son nom définitif, la ligne `ready` (`BR-BACKUP-012`, `BR-BACKUP-014`).
    ///
    /// **Avant d'écrire un octet**, la ligne `writing` apprend la taille de l'archive et les empreintes : un agent
    /// qui s'arrête entre l'écriture et `ready` laisse des fichiers que le démarrage sait retrouver (et eux seuls).
    /// `counts` : fichiers changés pendant la copie, éléments non archivés.
    async fn publish(
        &self,
        job: &Job,
        ident: (u64, u64),
        mut manifest: Manifest,
        end: ArchiveEnd,
        counts: (u32, u32),
    ) -> CopyEnd {
        let (changed, skipped) = counts;
        manifest.archive = Some(ArchiveFacts {
            name: job.backup.archive_name(),
            bytes: end.bytes,
            sha256: end.sha256.clone(),
        });
        let Ok(json) = serde_json::to_vec(&manifest) else {
            journal(self.ops.failed(&job.id, step::COPY).await, "failed");
            self.purge_job(job, ident).await;
            return CopyEnd::Over(Err(Finish::failed(Cause::StepFailed)));
        };
        let manifest_sha = sha256_hex(&json);
        let noted = self
            .repo
            .note_publishing(
                &job.backup,
                &Publishing {
                    archive_bytes: end.bytes,
                    manifest_sha256: manifest_sha.clone(),
                    archive_sha256: end.sha256.clone(),
                },
            )
            .await;
        if !matches!(noted, Ok(true)) {
            tracing::error!(
                ?noted,
                "sauvegarde non annoncée au registre avant sa publication"
            );
            journal(self.ops.failed(&job.id, step::COPY).await, "failed");
            self.purge_job(job, ident).await;
            return CopyEnd::Over(Err(Finish::failed(Cause::StepFailed)));
        }
        let files = Arc::clone(&self.files);
        let (dir, side, part, archive) = (
            job.key.to_string(),
            job.backup.manifest_name(),
            job.backup.part_name(),
            job.backup.archive_name(),
        );
        let published = tokio::task::spawn_blocking(move || {
            files.write_new(&dir, &side, &json)?;
            files.publish(&dir, &part, &archive, ident)
        })
        .await;
        let stamp = match published {
            Ok(Ok(stamp)) if stamp.size == end.bytes => stamp,
            other => {
                tracing::warn!(?other, "archive de sauvegarde non publiée");
                journal(self.ops.failed(&job.id, step::COPY).await, "failed");
                if let Ok(Ok(stamp)) = other {
                    // Publiée mais d'une autre taille que ce qui a été écrit : retirée par son identité complète.
                    self.remove_stamped(job, stamp).await;
                }
                self.purge_job(job, ident).await;
                return CopyEnd::Over(Err(Finish::failed(Cause::StepFailed)));
            }
        };
        let mods = self.mods_record(job, &manifest);
        let row = Published {
            archive_bytes: end.bytes,
            archive_mtime_ns: stamp.mtime_ns,
            data_bytes: manifest.totals.bytes,
            files: manifest.totals.files,
            flags: manifest.flags.clone(),
            changed_files: changed,
            skipped,
            manifest_sha256: manifest_sha,
            archive_sha256: end.sha256,
            mods,
        };
        let mut marked = self.repo.mark_ready(&job.backup, &row).await;
        if marked.is_err() && row.mods.is_some() {
            // Lire les mods ne fait JAMAIS échouer une sauvegarde (BR-MOD-034) : une liste que la base refuse
            // s'abandonne, la sauvegarde se publie sans.
            tracing::warn!("liste des mods non enregistrée : la sauvegarde se publie sans elle");
            let without = Published {
                mods: None,
                ..row.clone()
            };
            marked = self.repo.mark_ready(&job.backup, &without).await;
        }
        match marked {
            Ok(true) => {}
            other => {
                tracing::error!(?other, "sauvegarde publiée mais pas prête au registre");
                journal(self.ops.failed(&job.id, step::COPY).await, "failed");
                // La ligne n'est pas `ready` : elle porte encore ce qu'il faut pour retirer SES fichiers.
                self.purge_job(job, ident).await;
                return CopyEnd::Over(Err(Finish::failed(Cause::StepFailed)));
            }
        }
        self.done(job, step::COPY).await;
        CopyEnd::Over(Ok(Copied {
            mode: job.mode,
            files: manifest.totals.files,
            bytes: manifest.totals.bytes,
            changed,
            skipped,
            forced_stop: job.forced_stop.load(Ordering::SeqCst),
        }))
    }

    /// Retire une archive publiée dont on connaît l'identité complète.
    async fn remove_stamped(&self, job: &Job, stamp: FileStamp) {
        let files = Arc::clone(&self.files);
        let (dir, name) = (job.key.to_string(), job.backup.archive_name());
        let _ =
            tokio::task::spawn_blocking(move || files.remove(&dir, &name, Expected::stamp(stamp)))
                .await;
    }

    /// Retire une partie écrite que la ligne du registre ne connaît pas (encore).
    async fn discard_part(&self, job: &Job, ident: (u64, u64)) {
        let files = Arc::clone(&self.files);
        let (dir, name) = (job.key.to_string(), job.backup.part_name());
        let _ =
            tokio::task::spawn_blocking(move || files.remove(&dir, &name, Expected::inode(ident)))
                .await;
    }

    /// Retire SES fichiers et SA ligne : par la ligne du registre quand elle existe, sinon la seule partie écrite.
    async fn purge_job(&self, job: &Job, ident: (u64, u64)) {
        match self.repo.get(&job.backup).await {
            Ok(Some(row)) => {
                self.purge(&row).await;
            }
            _ => self.discard_part(job, ident).await,
        }
    }

    /// Retire les fichiers de Hearth d'une ligne du registre, puis la ligne : la partie écrite (par inode), l'archive
    /// (par inode et taille, et la date quand la ligne est `ready`), le manifeste voisin (par empreinte). Jamais
    /// autre chose (`BR-BACKUP-004`). `false` : un fichier n'a pas pu être retiré, la ligne reste pour un
    /// prochain passage.
    async fn purge(&self, row: &BackupRow) -> bool {
        let files = Arc::clone(&self.files);
        let owned = row.clone();
        let clean = tokio::task::spawn_blocking(move || purge_files(files.as_ref(), &owned))
            .await
            .unwrap_or(false);
        if !clean {
            tracing::warn!(
                "fichiers d'une sauvegarde non retirés : la ligne du registre est gardée"
            );
            return false;
        }
        if let Err(error) = self.repo.delete(&row.id).await {
            tracing::warn!(%error, "ligne de sauvegarde non retirée");
            return false;
        }
        true
    }

    /// La vérification qui suit la création (`BR-BACKUP-013`) : un échec constaté est supprimé par Hearth, c'est son
    /// fichier. `Err(None)` : l'agent s'arrête, rien n'est conclu.
    async fn verify_new(&self, job: &Job) -> Result<(), Option<Finish>> {
        let Ok(Some(row)) = self.repo.get(&job.backup).await else {
            journal(self.ops.failed(&job.id, step::VERIFY).await, "failed");
            return Err(Some(Finish::failed(Cause::StepFailed)));
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
                self.purge(&row).await;
                Err(Some(Finish::failed(Cause::BackupCorrupt)))
            }
            Ok(Checked::Stopped) => Err(None),
            Err(()) => {
                // La lecture a échoué sans rien dire de l'archive (le disque, pas le contenu) : elle reste,
                // non vérifiée.
                journal(self.ops.failed(&job.id, step::VERIFY).await, "failed");
                Err(Some(Finish::failed(Cause::StepFailed)))
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
                self.purge(&row).await;
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
        // Une archive pas encore prête est retirée avec sa ligne ; une sauvegarde déjà prête et publiée reste (elle
        // est complète, seule sa vérification n'a pas eu lieu).
        match self.repo.get(&job.backup).await {
            Ok(Some(row)) if !row.ready => {
                self.purge(&row).await;
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, "ligne de sauvegarde illisible après une panique");
            }
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
            Checked::Stopped => Err(internal("agent en cours d'arrêt")),
        }
    }

    /// Branche la lecture des mods d'un service (racine de composition, une seule fois).
    ///
    // FIX:01M8CJ9T4Q6W2XKZ7N3VHBY5PD le service des mods tient les sauvegardes (`ApplyDeps`) : le port est
    // pris en `Weak`, sinon les deux se tiennent en vie l'un l'autre et la base SQLite ne se ferme jamais.
    pub fn set_mods_snapshot(&self, snapshot: &Arc<dyn ModsSnapshot>) {
        let _ = self.mods.set(Arc::downgrade(snapshot));
    }

    /// Comme `set_mods_snapshot`, mais le port est tenu ici : pour un faux qui ne tient pas les sauvegardes.
    pub fn keep_mods_snapshot(&self, snapshot: Arc<dyn ModsSnapshot>) {
        let _ = self.mods.set(Arc::downgrade(&snapshot));
        let _ = self.mods_kept.set(snapshot);
    }

    fn mods_port(&self) -> Option<Arc<dyn ModsSnapshot>> {
        self.mods.get().and_then(std::sync::Weak::upgrade)
    }

    /// Lit les mods du service avant la copie (HRT-293). Une lecture qui échoue ne fait jamais échouer la sauvegarde.
    async fn read_mods(&self, job: &Job) {
        let Some(port) = self.mods_port() else {
            return;
        };
        let reading = port.read(&job.key).await;
        *job.mods_read.lock().unwrap_or_else(PoisonError::into_inner) = Some(reading);
    }

    /// La liste des mods de cette sauvegarde, recoupée avec le manifeste (la vérité de ce que l'archive contient).
    /// `None` : service sans mods (aucune ligne).
    fn mods_record(&self, job: &Job, manifest: &Manifest) -> Option<ModsRecord> {
        let reading = job
            .mods_read
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()?;
        let set = match reading {
            ModsReading::NotApplicable => return None,
            ModsReading::Unreadable => RecordedSet::unreadable(),
            ModsReading::Read(read) => {
                let root = manifest.roots.iter().find(|r| r.target == read.data_target);
                let ignored = |root: u32| {
                    manifest.ignored.iter().any(|item| {
                        item.root == root
                            && (item.path == read.dir
                                || item.path.starts_with(&format!("{}/", read.dir)))
                    })
                };
                match root {
                    Some(root) if !ignored(root.id) => {
                        let files: Vec<ManifestFile> = manifest
                            .entries
                            .iter()
                            .filter(|entry| entry.root == root.id && entry.kind == EntryType::File)
                            .filter_map(|entry| {
                                Some(ManifestFile {
                                    path: entry.path.clone(),
                                    size: entry.size,
                                    sha256: entry.sha256.clone()?,
                                })
                            })
                            .collect();
                        backup_list::reconcile(&read.snapshot, &files, &read.dir, &read.retired_dir)
                    }
                    _ => RecordedSet::not_in_backup(),
                }
            }
        };
        Some(ModsRecord {
            set,
            at: self.clock.now(),
        })
    }

    /// `GET /services/{key}/backups/{id}/mods` (HRT-293, BR-MOD-034 à 036) : la liste des mods d'une sauvegarde et,
    /// avec `compare`, son écart avec les mods actuels. Lecture seule, jamais journalisée.
    pub async fn mods_of(
        &self,
        key: &ServiceKey,
        id: &BackupId,
        compare: bool,
    ) -> Result<BackupModsView, BackupFailed> {
        let subject = self.subject(key).await?;
        let named = |error| BackupFailed::named(error, &subject.name);
        let Some(port) = self.mods_port() else {
            return Err(named(BackupError::ModsNotApplicable));
        };
        if !port.applicable(key).await {
            return Err(named(BackupError::ModsNotApplicable));
        }
        // La sauvegarde d'un autre service n'existe pas pour celui-ci : jamais de lecture croisée.
        let row = self.row(id).await?;
        if row.service_key != *key {
            return Err(named(BackupError::BackupNotFound));
        }
        let Some(record) = self.repo.mods_of(id).await.map_err(internal)? else {
            return Ok(BackupModsView {
                state: ModsListState::NotRecorded,
                recorded_at: None,
                game: BackupModsGame::default(),
                counts: BackupModsCounts::default(),
                mods: Vec::new(),
                compare: None,
            });
        };
        let compare = if compare {
            match port.current(key).await {
                Some(current) => {
                    let changes = backup_list::compare(&record.set, &current.mods);
                    Some(BackupModsCompare {
                        complete: current.unread == 0,
                        unread: current.unread,
                        same: changes.is_empty(),
                        changes: changes.iter().map(change_view).collect(),
                    })
                }
                // La lecture actuelle est bloquée : la liste de la sauvegarde reste rendue, sans comparaison.
                None => None,
            }
        } else {
            None
        };
        Ok(mods_view(&record, compare))
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
        let archive = Arc::clone(&self.archive);
        let row = row.clone();
        // Une relecture à la fois : le jeton part avec le fil bloquant, rendu quand le travail est fini.
        let gate = Arc::clone(&self.verify_gate).lock_owned().await;
        let result = tokio::task::spawn_blocking(move || {
            let _gate = gate;
            check_blocking(files.as_ref(), archive.as_ref(), &row, &stop)
        })
        .await;
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
            let (expected, sha) = (expected_of(&row), row.manifest_sha256.clone());
            let outcome = tokio::task::spawn_blocking(move || {
                let outcome = files.remove(&dir, &archive, expected)?;
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
        validate_items(&settings.ignored)
            .map_err(items_error)
            .map_err(named)?;
        validate_items(&settings.included)
            .map_err(items_error)
            .map_err(named)?;
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
    skipped: u32,
    forced_stop: bool,
}

impl Copied {
    fn note(&self) -> BackupNote {
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
            skipped: self.skipped,
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
    /// L'agent s'arrête : rien n'est conclu.
    Stopped,
}

/// La relecture complète d'une archive du registre, sur un fil bloquant.
fn check_blocking(
    files: &dyn BackupFiles,
    archive: &dyn BackupArchive,
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
    // Le manifeste voisin n'est cru que s'il a l'empreinte du registre. Un manifeste absent ou qui n'est plus un
    // fichier ordinaire n'est pas celui de Hearth ; une erreur du disque ne dit rien de l'archive.
    let mut reader = match files.open(&dir, &row.id.manifest_name()) {
        Ok((reader, _)) => reader,
        Err(BackupFsError::NotFound | BackupFsError::Unsafe) => return Ok(Checked::Failed),
        Err(_) => return Err(()),
    };
    let mut bytes = Vec::new();
    if (&mut reader)
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return Err(());
    }
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_MANIFEST_BYTES {
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
    let (reader, ident) = files.open(&dir, &row.id.archive_name()).map_err(|_| ())?;
    // Le fichier ouvert est celui du registre (revérifié sur le descripteur).
    if ident != (row.dev, row.ino) {
        return Ok(Checked::Failed);
    }
    match archive.verify(reader, &manifest, &facts.sha256, stop) {
        Ok(report) => Ok(Checked::Verified {
            files: report.files_checked,
        }),
        Err(VerifyError::Corrupt) => Ok(Checked::Failed),
        Err(VerifyError::Stopped) => Ok(Checked::Stopped),
        Err(VerifyError::Io(_)) => Err(()),
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
        let _ = files.remove(dir, name, Expected::inode(ident));
    }
}

/// Retire les fichiers de Hearth d'une ligne : voir [`Backups::purge`]. `true` : plus rien de Hearth ne reste.
fn purge_files(files: &dyn BackupFiles, row: &BackupRow) -> bool {
    let dir = row.service_key.to_string();
    let mut clean = true;
    let mut judge = |result: Result<BackupRemoved, BackupFsError>| {
        // `NotOurs` : ce n'est plus notre fichier, il n'est pas touché et ne nous regarde plus.
        if result.is_err() {
            clean = false;
        }
    };
    judge(files.remove(
        &dir,
        &row.id.part_name(),
        Expected::inode((row.dev, row.ino)),
    ));
    judge(files.remove(&dir, &row.id.archive_name(), expected_of(row)));
    if !row.manifest_sha256.is_empty() {
        // Le manifeste voisin ne part que s'il a l'empreinte de la ligne.
        remove_manifest_if_ours(files, &dir, &row.id.manifest_name(), &row.manifest_sha256);
    }
    clean
}

/// Ce que la ligne sait de son archive : l'inode toujours, la taille dès qu'elle est annoncée, la date une fois
/// publiée.
fn expected_of(row: &BackupRow) -> Expected {
    Expected {
        dev: row.dev,
        ino: row.ino,
        size: (row.archive_bytes > 0).then_some(row.archive_bytes),
        mtime_ns: row.ready.then_some(row.archive_mtime_ns),
    }
}

fn registered_of(row: &BackupRow) -> Registered {
    Registered {
        dev: row.dev,
        ino: row.ino,
        size: row.archive_bytes,
        mtime_ns: row.archive_mtime_ns,
        manifest_sha256: row.manifest_sha256.clone(),
    }
}

fn note_error(error: NoteError) -> BackupError {
    BackupError::Invalid {
        field: "note",
        reason: match error {
            NoteError::TooLong => "too_long",
            NoteError::Invalid => "invalid",
        },
    }
}

fn items_error(error: ItemsError) -> BackupError {
    BackupError::Invalid {
        field: "items",
        reason: match error {
            ItemsError::TooMany => "too_many",
            ItemsError::Invalid => "invalid",
        },
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

// ---------------------------------------------------------------------------------------------------------------
// La liste des mods d'une sauvegarde sur le fil (HRT-293)
// ---------------------------------------------------------------------------------------------------------------

fn state_view(state: backup_list::SetState) -> ModsListState {
    match state {
        backup_list::SetState::Recorded => ModsListState::Recorded,
        backup_list::SetState::Partial => ModsListState::Partial,
        backup_list::SetState::Unreadable => ModsListState::Unreadable,
        backup_list::SetState::NotInBackup => ModsListState::NotInBackup,
    }
}

fn summary_view(summary: Option<&ModsSummaryRow>) -> BackupModsSummary {
    match summary {
        Some(row) => BackupModsSummary {
            state: state_view(row.state),
            active: row.active,
            disabled: row.disabled,
            retired: row.retired,
        },
        None => BackupModsSummary {
            state: ModsListState::NotRecorded,
            active: 0,
            disabled: 0,
            retired: 0,
        },
    }
}

fn identity_wire(identity: &backup_list::IdentitySummary) -> ModIdentityView {
    ModIdentityView {
        id: identity.mod_id.clone(),
        name: identity.name.clone(),
        version: identity.version.clone(),
        loader: identity.loader.variant_id().to_owned(),
        minecraft_declared: false,
        environment: match identity.environment {
            crate::domain::mods::jar::Environment::Both => EnvironmentView::Both,
            crate::domain::mods::jar::Environment::ServerOnly => EnvironmentView::Server,
            crate::domain::mods::jar::Environment::ClientOnly => EnvironmentView::Client,
            crate::domain::mods::jar::Environment::Unknown => EnvironmentView::Unknown,
        },
    }
}

fn change_view(change: &backup_list::Change) -> ModChange {
    ModChange {
        kind: match change.kind {
            backup_list::ChangeKind::Added => ChangeKindView::Added,
            backup_list::ChangeKind::Removed => ChangeKindView::Removed,
            backup_list::ChangeKind::VersionChanged => ChangeKindView::VersionChanged,
            backup_list::ChangeKind::Enabled => ChangeKindView::Enabled,
            backup_list::ChangeKind::Disabled => ChangeKindView::Disabled,
        },
        id: change.mod_id.clone(),
        name: change.name.clone(),
        file: change.file_name.clone(),
        backup_version: change.backup_version.clone(),
        current_version: change.current_version.clone(),
    }
}

fn mods_view(record: &ModsRecord, compare: Option<BackupModsCompare>) -> BackupModsView {
    use crate::domain::mods::game_info::Origin;
    let set = &record.set;
    BackupModsView {
        state: state_view(set.state),
        recorded_at: Some(rfc3339(record.at)),
        game: BackupModsGame {
            loader: set.loader.map(|loader| loader.variant_id().to_owned()),
            minecraft: set.minecraft.clone(),
        },
        counts: BackupModsCounts {
            active: set.active,
            disabled: set.disabled,
            retired: set.retired,
            not_listed: set.not_listed,
        },
        mods: set
            .entries
            .iter()
            .map(|entry| BackupModItem {
                file: entry.file_name.clone(),
                rank: entry.rank,
                place: match entry.place {
                    backup_list::Place::Active => BackupModPlace::Active,
                    backup_list::Place::Disabled => BackupModPlace::Disabled,
                    backup_list::Place::Retired => BackupModPlace::Retired,
                },
                origin: match entry.origin {
                    Origin::ModrinthPack => ModOrigin::ModrinthPack,
                    Origin::Hearth => ModOrigin::Hearth,
                    Origin::ServerPack => ModOrigin::ServerPack,
                    Origin::Manual => ModOrigin::Manual,
                },
                identity: entry.identity.as_ref().map(identity_wire),
            })
            .collect(),
        compare,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_note_field_is_lower_case_and_bound_to_the_backup() {
        let id = BackupId::parse("01J9ZY0G3Q8M2K6W4T7V5N1B9D").expect("id");
        let field = note_field(&id).expect("champ");
        assert_eq!(field.as_text(), "backup-note:01j9zy0g3q8m2k6w4t7v5n1b9d");
        assert_eq!(SecretField::parse(&field.as_text()), Some(field));
    }
}
