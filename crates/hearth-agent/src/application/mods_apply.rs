//! Appliquer un lot de mods à un serveur de jeu installé (HRT-258, HRT-259, ADR-0062, BR-MOD-024 à 027, 031, 032).
//!
//! `ModsService::apply` juge SANS rien arrêter ni écrire (bornes du lot, dépôts rejugés, fichiers présents, noms libres,
//! espace pour la sauvegarde), accepte l'opération `mods` (verrou exclusif ET place des opérations lourdes) puis
//! l'exécute en tâche de fond : `stop` (si le service tournait), `copy` et `verify` (la sauvegarde « Avant mods », cœur
//! commun de `Backups`), `place` (la mise en place : poses et renommages SANS remplacement, jamais une suppression),
//! `start` et `watch` (si le service tournait).
//!
//! Chaque mouvement de fichier est écrit au journal AVANT d'être fait (`UndoInfo::moves`, un par ligne) puis APRÈS (avec
//! l'inode). Une erreur au milieu de `place` rejoue les mouvements faits à l'envers, l'inode vérifié (`revert`).
//!
//! Ce fichier est un enfant de `mods.rs` : il lit ses éléments privés (plan de lecture, lecture du volume, jugement
//! des dépôts).

use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;
use std::time::{Duration, Instant};

use hearth_proto::api::mods::{ModsApplyAccepted, ModsApplyRequest};
use sha2::{Digest, Sha256};

use super::{Gathered, Judged, ModPlace, ModsError, ModsService, game_of, judge_deposit_files};
use crate::application::backups::{BackupFailed, Backups, CopyVerified, Job};
use crate::application::game_phase::GamePhases;
use crate::application::ports::{
    ContainerEngine, DepositFile, DepositId, FileId, FsError, HearthFile, LogQuery, Ownership,
    PackDeposits, Perms, SafeFs, ServiceRoot,
};
use crate::application::service_op_access::DecisionEffects;
use crate::application::service_ops::{OpError, OpSnapshot, PostedMods, ServiceOps};
use crate::application::service_stop::account_of;
use crate::domain::audit::{Actor, AuditAction, ClientName, ModsSummary, Outcome, Reason, Target};
use crate::domain::catalog::readiness::{GamePhase, ReadyRule};
use crate::domain::files::RelPath;
use crate::domain::mods::apply::{self as plan_mods, Action, ApplyRefusal, Input, JudgedAdd};
use crate::domain::mods::deposit::{Installed, Refusal};
use crate::domain::mods::listing;
use crate::domain::ops::mods::{op_plan, step};
use crate::domain::ops::{
    Cause, Choice, FileMove, MoveKind, OpKind, OpStatus, StepRecord, StepState, UndoInfo,
};
use crate::domain::services::RunState;
use crate::domain::services::ServiceKey;

/// Pourquoi un lot n'est pas accepté : rien n'a été arrêté ni écrit.
#[derive(Debug)]
pub enum ApplyError {
    Mods(ModsError),
    Backup(BackupFailed),
    /// Bornes, noms, doublons (`MODS_BATCH_INVALID`).
    Batch,
    /// Un dépôt rejugé est refusé (`MOD_REFUSED`).
    Refused {
        deposit: String,
        refusal: String,
    },
    FileMissing,
    InModrinthPack,
    AlreadyActive,
    NameTaken,
}

impl From<ModsError> for ApplyError {
    fn from(error: ModsError) -> Self {
        Self::Mods(error)
    }
}

impl From<ApplyRefusal> for ApplyError {
    fn from(refusal: ApplyRefusal) -> Self {
        match refusal {
            ApplyRefusal::Batch => Self::Batch,
            ApplyRefusal::Refused { deposit, refusal } => Self::Refused { deposit, refusal },
            ApplyRefusal::FileMissing => Self::FileMissing,
            ApplyRefusal::InModrinthPack => Self::InModrinthPack,
            ApplyRefusal::AlreadyActive => Self::AlreadyActive,
            ApplyRefusal::NameTaken => Self::NameTaken,
        }
    }
}

/// Les durées de la surveillance (réglables par les essais).
#[derive(Debug, Clone, Copy)]
pub struct ApplyTimings {
    /// Le pas de la surveillance du signe « prêt ».
    pub poll: Duration,
    /// Sans signe « prêt » essayé sur la vraie image : le conteneur doit rester en marche ce temps-là.
    pub steady: Duration,
}

impl ApplyTimings {
    pub const fn production() -> Self {
        Self {
            poll: Duration::from_secs(2),
            steady: Duration::from_secs(10),
        }
    }
}

/// Ce que `ModsService` doit à l'extérieur pour appliquer un lot (posé par la racine de composition).
pub struct ApplyDeps {
    pub backups: Arc<Backups>,
    pub ops: Arc<ServiceOps>,
    pub phases: Arc<GamePhases>,
    pub engine: Arc<dyn ContainerEngine>,
    pub timings: ApplyTimings,
}

/// Un dépôt à poser, avec l'empreinte rejugée.
struct AddExec {
    deposit: DepositId,
    sha256: [u8; 32],
}

/// Tout ce que l'exécution emporte avec elle. Aucun secret, aucune ligne du jeu.
struct Exec {
    job: Arc<Job>,
    key: ServiceKey,
    running: bool,
    actions: Vec<Action>,
    adds: Vec<AddExec>,
    account: String,
    mods_dir: RelPath,
    retired_dir: RelPath,
    file_perms: Perms,
    dir_perms: Perms,
    rule: ReadyRule,
    summary: ModsSummary,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// L'empreinte et la taille d'un dépôt, d'un seul passage de lecture.
fn hash_deposit(file: &mut Box<dyn DepositFile>) -> Option<([u8; 32], u64)> {
    file.seek(SeekFrom::Start(0)).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    let mut total = 0u64;
    loop {
        let read = match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return None,
        };
        total += read as u64;
        hasher.update(&buffer[..read]);
    }
    file.seek(SeekFrom::Start(0)).ok()?;
    Some((hasher.finalize().into(), total))
}

/// Ce que le rejugement des dépôts a rendu.
struct Rejudged {
    judgements: Vec<crate::domain::mods::deposit::Judgement>,
    hashes: Vec<Option<[u8; 32]>>,
    bytes: u64,
}

fn rejudge_blocking(
    pool: &dyn PackDeposits,
    account: &str,
    ids: &[DepositId],
    profile: &crate::domain::mods::compat::ServerProfile,
    installed: &[Installed],
) -> Result<Rejudged, ModsError> {
    // Tous les dépôts s'ouvrent d'abord : un inconnu arrête tout, rien n'est jugé.
    let mut hashes = Vec::with_capacity(ids.len());
    let mut bytes = 0u64;
    for id in ids {
        let mut file = pool.open(id, account).map_err(|error| match error {
            crate::application::ports::DepositError::NotFound => ModsError::DepositUnknown,
            _ => ModsError::Storage("dépôt illisible".to_owned()),
        })?;
        let hashed = hash_deposit(&mut file);
        bytes = bytes.saturating_add(hashed.map_or(0, |(_, size)| size));
        hashes.push(hashed.map(|(hash, _)| hash));
    }
    let judged = judge_deposit_files(pool, account, ids, profile, installed)?;
    Ok(Rejudged {
        judgements: judged.judgements,
        hashes,
        bytes,
    })
}

fn rel_text(dir: &RelPath, name: &str) -> String {
    format!("{dir}/{name}")
}

fn parse(text: &str) -> Result<RelPath, FsError> {
    RelPath::parse(text).map_err(|_| FsError::Refused)
}

impl ModsService {
    /// Branche ce que l'application d'un lot demande (racine de composition, une seule fois).
    pub fn set_apply(&self, deps: ApplyDeps) {
        let _ = self.apply_deps.set(deps);
    }

    /// Le dossier géré qui porte les mods : le propriétaire et le point de montage, d'après la fiche.
    async fn fiche_perms(&self, key: &ServiceKey) -> Result<(Perms, Perms, ReadyRule), ModsError> {
        if key.kind() != crate::domain::services::KeyKind::Hearth {
            return Err(ModsError::Adopted);
        }
        let body = self
            .specs
            .body(key)
            .await?
            .ok_or(ModsError::NotAGameService)?;
        let spec_fiche = body.fiche.ok_or(ModsError::NotAGameService)?;
        let fiche = self
            .catalog
            .fiche(&spec_fiche.id)
            .ok_or(ModsError::NotAGameService)?;
        let owner = Ownership::new(fiche.file.owner.uid, fiche.file.owner.gid)
            .ok_or_else(|| ModsError::Storage("propriétaire de la fiche illisible".to_owned()))?;
        let file = Perms::new(owner, 0o644)
            .ok_or_else(|| ModsError::Storage("droits illisibles".to_owned()))?;
        let dir = Perms::new(owner, 0o755)
            .ok_or_else(|| ModsError::Storage("droits illisibles".to_owned()))?;
        Ok((file, dir, ReadyRule::of(fiche)))
    }

    /// `POST /services/{key}/mods/apply` (HRT-258, BR-MOD-025 à 027, 031) : juge le lot, accepte l'opération et la lance.
    /// Rien n'est arrêté ni écrit avant l'acceptation.
    pub async fn apply(
        self: &Arc<Self>,
        key: &ServiceKey,
        actor: Actor,
        request: ModsApplyRequest,
        op_key: &str,
    ) -> Result<ModsApplyAccepted, ApplyError> {
        let deps = self.apply_deps.get().ok_or_else(|| {
            ApplyError::Mods(ModsError::Storage(
                "application des mods non branchée".to_owned(),
            ))
        })?;
        let plan = match self.plan_of(key).await? {
            Judged::Done(_) => return Err(ModsError::NotApplicable.into()),
            Judged::Read(plan) => plan,
        };
        plan_mods::validate(&request)?;
        let account = account_of(&actor);
        let (rows, gathered) = self.read_volume(key, &plan).await?;
        let game = game_of(&plan, &rows, &gathered);
        let installed = installed_of(&gathered);
        let retired = retired_of(&gathered);
        let ids: Vec<DepositId> = request
            .add
            .iter()
            .map(|add| DepositId::parse(&add.deposit).ok_or(ModsError::DepositUnknown))
            .collect::<Result<_, _>>()?;
        // Les dépôts : rejugés entièrement, l'empreinte relue. Le client n'est jamais cru.
        let pool = self.deposits.get().cloned();
        let (judgements, hashes, deposit_bytes) = if ids.is_empty() {
            (Vec::new(), Vec::new(), 0)
        } else {
            let pool = pool.clone().ok_or_else(|| {
                ModsError::Storage("zone d'attente des dépôts non branchée".to_owned())
            })?;
            let (pool2, account2, profile, installed2) = (
                Arc::clone(&pool),
                account.clone(),
                game.profile.clone(),
                installed.clone(),
            );
            let ids2 = ids.clone();
            let done = tokio::task::spawn_blocking(move || {
                rejudge_blocking(pool2.as_ref(), &account2, &ids2, &profile, &installed2)
            })
            .await
            .map_err(|error| ModsError::Storage(error.to_string()))??;
            (done.judgements, done.hashes, done.bytes)
        };
        let mut adds_judged: Vec<JudgedAdd> = Vec::with_capacity(judgements.len());
        for ((add, judgement), hash) in request.add.iter().zip(judgements).zip(&hashes) {
            let claimed = add.sha256.to_ascii_lowercase();
            let judgement = match judgement {
                Ok(accepted) if hash.map(|h| hex(&h)) != Some(claimed) => {
                    Err((Refusal::Unrecognized, Some(accepted.identity)))
                }
                other => other,
            };
            adds_judged.push(JudgedAdd {
                deposit: add.deposit.clone(),
                judgement,
            });
        }
        let actions = match plan_mods::plan(
            &request,
            &Input {
                profile: &game.profile,
                installed: &installed,
                retired: &retired,
                adds: &adds_judged,
            },
        ) {
            Ok(actions) => actions,
            Err(refusal) => {
                // Un dépôt refusé quitte la zone d'attente aussitôt (BR-MOD-016).
                if let ApplyRefusal::Refused { deposit, .. } = &refusal
                    && let Some(id) = DepositId::parse(deposit)
                    && let Some(pool) = &pool
                {
                    pool.discard(&id, &account);
                }
                return Err(refusal.into());
            }
        };
        let (file_perms, dir_perms, rule) = self.fiche_perms(key).await?;
        let prepared = deps
            .backups
            .prepare_for_mods(key, deposit_bytes)
            .await
            .map_err(ApplyError::Backup)?;
        let running = prepared.running;
        let op_plan = op_plan(running).map_err(|error| {
            ApplyError::Mods(ModsError::Storage(format!("plan de l'opération : {error}")))
        })?;
        // HRT-292 : un jeu qui tourne enregistre son monde par la console avant l'arrêt de l'opération.
        let op_plan = if running {
            deps.ops.plan_before_stop(key, op_plan).await
        } else {
            op_plan
        };
        let job = deps
            .backups
            .accept_for_mods(key, prepared, actor, op_key, op_plan)
            .await
            .map_err(ApplyError::Backup)?;
        let accepted = ModsApplyAccepted {
            operation_id: job.op_id().to_owned(),
            backup_id: job.backup_id().to_string(),
        };
        let summary = summary_of(&request, &actions);
        let adds = request
            .add
            .iter()
            .zip(&ids)
            .zip(&hashes)
            .map(|((_, id), hash)| AddExec {
                deposit: id.clone(),
                sha256: hash.unwrap_or([0; 32]),
            })
            .collect();
        let exec = Exec {
            job,
            key: key.clone(),
            running,
            actions,
            adds,
            account,
            mods_dir: plan.mods_dir.clone(),
            retired_dir: plan.retired_dir.clone(),
            file_perms,
            dir_perms,
            rule,
            summary,
        };
        let this = Arc::clone(self);
        let work = tokio::spawn({
            let this = Arc::clone(&this);
            async move { this.run_apply(exec).await }
        });
        let watcher = Arc::clone(&this);
        let (op, name) = (accepted.operation_id.clone(), key.clone());
        tokio::spawn(async move {
            if work.await.is_err() {
                tracing::error!(operation = %op, "application d'un lot de mods interrompue par une panique");
                watcher.close_after_panic(&op, &name).await;
            }
        });
        Ok(accepted)
    }

    fn deps(&self) -> &ApplyDeps {
        // `apply` a déjà vérifié que le branchement existe.
        match self.apply_deps.get() {
            Some(deps) => deps,
            None => unreachable!("application des mods non branchée"),
        }
    }

    /// Une panique de l'exécution : l'opération est close par le MÊME chemin que toute clôture, sans rien rejouer.
    async fn close_after_panic(&self, op: &str, _key: &ServiceKey) {
        let deps = self.deps();
        if let Err(error) = deps
            .ops
            .finish(op, OpStatus::Failed, Some(Cause::StepFailed))
            .await
        {
            tracing::error!(%error, "opération de mods non close après une panique");
            deps.ops.settle(op, None).await;
        }
    }

    async fn is_cancelling(&self, id: &str) -> bool {
        matches!(
            self.deps().ops.snapshot(id).await,
            Ok(Some(snapshot)) if snapshot.record.status == OpStatus::Cancelling
        )
    }

    /// Le journal d'une étape : une ligne illisible ne fait jamais échouer un geste déjà fait.
    async fn done_step(&self, id: &str, name: &str) {
        if let Err(error) = self.deps().ops.done(id, name, &UndoInfo::default()).await {
            tracing::warn!(%error, step = name, "journal des mods : fin d'étape non écrite");
        }
    }

    /// L'exécution d'un lot accepté.
    async fn run_apply(self: Arc<Self>, exec: Exec) {
        let deps = self.deps();
        let id = exec.job.op_id().to_owned();
        // 0. La liste des mods d'avant, lue AVANT l'arrêt (le service tourne encore : lecture seule).
        deps.backups.read_mods_for(&exec.job).await;
        // 1. Arrêt, seulement si le service tournait.
        if exec.running {
            deps.ops.console_save(&id, &exec.key).await;
            if deps
                .ops
                .intent(&id, step::STOP, &UndoInfo::default())
                .await
                .is_err()
            {
                return self
                    .end(&exec, OpStatus::Failed, Some(Cause::StepFailed))
                    .await;
            }
            if let Err(finish) = deps.backups.stop_for_mods(&exec.job).await {
                let _ = deps.ops.failed(&id, step::STOP).await;
                return self.end(&exec, finish.status(), finish.cause()).await;
            }
            self.done_step(&id, step::STOP).await;
            if self.is_cancelling(&id).await {
                return self
                    .end(&exec, OpStatus::Cancelled, Some(Cause::CancelledByAccount))
                    .await;
            }
        }
        // 2 et 3. La sauvegarde « Avant mods » : la copie puis la vérification (cœur commun de `Backups`).
        match deps.backups.copy_and_verify(&exec.job).await {
            CopyVerified::Halted => return,
            CopyVerified::Over(finish) if finish.status() != OpStatus::Succeeded => {
                return self.end(&exec, finish.status(), finish.cause()).await;
            }
            CopyVerified::Over(_) => {}
        }
        if self.is_cancelling(&id).await {
            return self
                .end(&exec, OpStatus::Cancelled, Some(Cause::CancelledByAccount))
                .await;
        }
        // 4. La mise en place.
        let placed = match self.place(&exec).await {
            PlaceEnd::Done(placed) => placed,
            PlaceEnd::Cancelled => {
                return self
                    .end(&exec, OpStatus::Cancelled, Some(Cause::CancelledByAccount))
                    .await;
            }
            PlaceEnd::Failed => {
                return self
                    .end(&exec, OpStatus::Failed, Some(Cause::StepFailed))
                    .await;
            }
        };
        // 5 et 6. Relance et surveillance.
        let ending = if exec.running {
            self.start_and_watch(&exec).await
        } else {
            WatchEnd::Ready
        };
        match ending {
            WatchEnd::Ready => {
                self.end_with(&exec, OpStatus::Succeeded, None, Some(&placed))
                    .await;
            }
            WatchEnd::NotReady(cause) => {
                // Le retour automatique (HRT-259) : mods d'avant en place, service relancé.
                self.roll_back(&exec, &placed, cause).await;
            }
        }
    }

    /// La mise en place : chaque mouvement est écrit au journal AVANT d'être fait, puis APRÈS avec son inode.
    async fn place(&self, exec: &Exec) -> PlaceEnd {
        let deps = self.deps();
        let id = exec.job.op_id().to_owned();
        let prepared = {
            let (fs, key, dir, perms) = (
                Arc::clone(&self.fs),
                exec.key.clone(),
                exec.retired_dir.clone(),
                exec.dir_perms,
            );
            tokio::task::spawn_blocking(move || -> Result<(), FsError> {
                let root = fs.open_service_root(&key)?;
                root.mkdir(&dir, perms).map(|_| ())
            })
            .await
        };
        if !matches!(prepared, Ok(Ok(()))) {
            // Rien n'est encore touché : ce n'est pas un geste destructif.
            return PlaceEnd::Failed;
        }
        let mut done: Vec<FileMove> = Vec::new();
        for action in &exec.actions {
            let (kind, from, to) = match action {
                Action::Move { kind, from, to } => {
                    let (from_dir, to_dir) = match kind {
                        MoveKind::Retired => (&exec.mods_dir, &exec.retired_dir),
                        MoveKind::Restored => (&exec.retired_dir, &exec.mods_dir),
                        _ => (&exec.mods_dir, &exec.mods_dir),
                    };
                    (*kind, rel_text(from_dir, from), rel_text(to_dir, to))
                }
                Action::Place { file, .. } => (
                    MoveKind::Placed,
                    String::new(),
                    rel_text(&exec.mods_dir, file),
                ),
            };
            let mut mv = FileMove {
                kind,
                from,
                to,
                dev: 0,
                ino: 0,
            };
            let intent = UndoInfo {
                moves: vec![mv.clone()],
                ..UndoInfo::default()
            };
            if let Err(error) = deps.ops.intent(&id, step::PLACE, &intent).await {
                let cancelling = matches!(error, OpError::Stale | OpError::OutOfOrder)
                    && self.is_cancelling(&id).await;
                if cancelling && done.is_empty() {
                    return PlaceEnd::Cancelled;
                }
                tracing::error!(%error, "journal des mods illisible : le mouvement n'est pas fait");
                self.undo_all(exec, &done).await;
                return PlaceEnd::Failed;
            }
            match self.do_move(exec, action, &mv).await {
                Ok(file) => {
                    mv.dev = file.dev;
                    mv.ino = file.ino;
                    done.push(mv.clone());
                    let after = UndoInfo {
                        moves: vec![mv],
                        ..UndoInfo::default()
                    };
                    if let Err(error) = deps.ops.done(&id, step::PLACE, &after).await {
                        // Le journal ne garde que l'INTENTION de ce mouvement (inode 0) : l'identité n'est connue
                        // qu'en mémoire. Un retour après un arrêt de l'agent ne l'adopte pas sans vérification.
                        tracing::warn!(%error, "journal des mods : fin de mouvement non écrite, intention seule");
                    }
                }
                Err(()) => {
                    let _ = deps.ops.failed(&id, step::PLACE).await;
                    self.undo_all(exec, &done).await;
                    return PlaceEnd::Failed;
                }
            }
        }
        PlaceEnd::Done(done)
    }

    /// Un seul geste : une pose ou un renommage sans remplacement. Rend l'identité du fichier déplacé ou posé.
    async fn do_move(&self, exec: &Exec, action: &Action, mv: &FileMove) -> Result<FileId, ()> {
        let (fs, key) = (Arc::clone(&self.fs), exec.key.clone());
        match action {
            Action::Move { .. } => {
                let (from, to) = (mv.from.clone(), mv.to.clone());
                tokio::task::spawn_blocking(move || -> Result<FileId, FsError> {
                    let root = fs.open_service_root(&key)?;
                    root.rename_noreplace(&parse(&from)?, &parse(&to)?)
                })
                .await
                .map_err(|_| ())?
                .map_err(|_| ())
            }
            Action::Place { add, .. } => {
                let pool = Arc::clone(self.deposits.get().ok_or(())?);
                let planned = exec.adds.get(*add).ok_or(())?;
                let (deposit, sha) = (planned.deposit.clone(), planned.sha256);
                let (account, perms, to) = (exec.account.clone(), exec.file_perms, mv.to.clone());
                tokio::task::spawn_blocking(move || -> Result<FileId, ()> {
                    let rel = parse(&to).map_err(|_| ())?;
                    pool.place(&deposit, &account, &key, &rel, &sha, perms)
                        .map_err(|_| ())?;
                    let root = fs.open_service_root(&key).map_err(|_| ())?;
                    root.identify(&rel).map_err(|_| ())
                })
                .await
                .map_err(|_| ())?
            }
        }
    }

    /// Rejoue les mouvements faits à l'envers (inode vérifié). `true` : tout est revenu.
    async fn undo_all(&self, exec: &Exec, done: &[FileMove]) -> bool {
        if done.is_empty() {
            return true;
        }
        let (fs, key, retired) = (
            Arc::clone(&self.fs),
            exec.key.clone(),
            exec.retired_dir.clone(),
        );
        let moves = done.to_vec();
        let outcome = tokio::task::spawn_blocking(move || {
            revert_blocking(fs.as_ref(), &key, &retired, &moves)
        })
        .await
        .unwrap_or_default();
        self.write_undone(exec.job.op_id(), &outcome).await;
        outcome.all
    }

    /// Une ligne `undone` par mouvement revenu, puis celle qui clôt l'étape quand tout est revenu : un retour
    /// interrompu laisse au journal ce qui reste à défaire.
    async fn write_undone(&self, id: &str, outcome: &Reverted) {
        let ops = &self.deps().ops;
        for mv in &outcome.moves {
            let info = UndoInfo {
                moves: vec![mv.clone()],
                ..UndoInfo::default()
            };
            if let Err(error) = ops.undone_move(id, step::PLACE, &info).await {
                tracing::warn!(%error, "journal des mods : mouvement revenu non écrit");
            }
        }
        if outcome.all {
            let _ = ops.undone(id, step::PLACE).await;
        }
    }

    /// 5 et 6 : relance et surveillance du signe « prêt ».
    async fn start_and_watch(&self, exec: &Exec) -> WatchEnd {
        let deps = self.deps();
        let id = exec.job.op_id().to_owned();
        if deps
            .ops
            .intent(&id, step::START, &UndoInfo::default())
            .await
            .is_err()
        {
            tracing::warn!(operation = %id, "journal illisible : le service arrêté est relancé quand même");
        }
        if deps.backups.start_for_mods(&exec.job).await.is_err() {
            let _ = deps.ops.failed(&id, step::START).await;
            return WatchEnd::NotReady(Cause::NotRestarted);
        }
        self.done_step(&id, step::START).await;
        if deps
            .ops
            .intent(&id, step::WATCH, &UndoInfo::default())
            .await
            .is_err()
        {
            tracing::warn!(operation = %id, "journal illisible : surveillance sans ligne");
        }
        let ending = self.watch(exec).await;
        match ending {
            WatchEnd::Ready => self.done_step(&id, step::WATCH).await,
            WatchEnd::NotReady(_) => {
                let _ = deps.ops.failed(&id, step::WATCH).await;
            }
        }
        ending
    }

    /// Le signe « prêt » dans les lignes écrites depuis la relance, avant le délai de la fiche ; sinon, le conteneur
    /// sorti ou le délai dépassé. Sans signe essayé sur la vraie image, le conteneur doit rester en marche un moment.
    async fn watch(&self, exec: &Exec) -> WatchEnd {
        self.watch_service(&exec.key, &exec.job, &exec.rule).await
    }

    async fn watch_service(&self, key: &ServiceKey, job: &Job, rule: &ReadyRule) -> WatchEnd {
        let deps = self.deps();
        let stopped = job.stopped_containers();
        let Some(container) = stopped.first().cloned() else {
            return WatchEnd::Ready;
        };
        let started = Instant::now();
        let budget = Duration::from_secs(u64::from(rule.timeout_s));
        let has_sign = rule.sign.is_some();
        if has_sign {
            deps.phases.begin(key, &container, rule.clone(), false);
        }
        loop {
            if has_sign
                && matches!(
                    deps.phases.refresh(key).await,
                    Some(GamePhase::Ready { .. })
                )
            {
                return WatchEnd::Ready;
            }
            match deps.engine.inspect(&container).await {
                Ok(Some(observed)) if matches!(observed.run_state, RunState::Running) => {}
                Ok(Some(_) | None) => {
                    return WatchEnd::NotReady(Cause::StoppedRightAfterStart);
                }
                // Un moteur muet n'est pas un jeu qui ne démarre pas : la boucle continue jusqu'au délai.
                Err(_) => {}
            }
            let elapsed = started.elapsed();
            if has_sign {
                if elapsed >= budget {
                    return WatchEnd::NotReady(Cause::Timeout);
                }
            } else if elapsed >= deps.timings.steady.min(budget) {
                return WatchEnd::Ready;
            }
            tokio::time::sleep(deps.timings.poll).await;
        }
    }

    /// Le retour automatique (HRT-259) : le serveur ne démarre pas avec ses nouveaux mods. Arrêt, mouvements rejoués à
    /// l'envers, relance, surveillance. Le retour qui échoue ne relance rien seul : `interrupted`.
    async fn roll_back(&self, exec: &Exec, placed: &[FileMove], cause: Cause) {
        let deps = self.deps();
        let id = exec.job.op_id().to_owned();
        if let Err(error) = deps.ops.begin_rollback(&id).await {
            tracing::error!(%error, "retour des mods : statut non changé");
            return self.end(exec, OpStatus::Failed, Some(cause)).await;
        }
        // Les dernières lignes du jeu et la cause reconnue, lues AVANT tout (le journal du conteneur est encore celui du
        // démarrage qui a échoué) et gardées 24 heures en mémoire seulement.
        self.keep_tail_of(exec).await;
        let stopped_ok = self.stop_for_rollback(exec).await;
        let reverted = stopped_ok && self.undo_all(exec, placed).await;
        if !reverted {
            // Rien n'est relancé seul.
            return self
                .end_with(exec, OpStatus::Interrupted, Some(Cause::StepFailed), None)
                .await;
        }
        let restarted = if exec.running {
            deps.backups.start_for_mods(&exec.job).await.is_ok()
        } else {
            true
        };
        if restarted {
            // La surveillance de l'état d'avant : si le jeu ne revient pas non plus, il est arrêté (l'arrêt est voulu,
            // inscrit) et rien n'est relancé seul : « interrompue », la cause est celle de l'échec du retour.
            let back = if exec.running {
                self.watch(exec).await
            } else {
                WatchEnd::Ready
            };
            match back {
                WatchEnd::Ready => {
                    self.end_with(exec, OpStatus::RolledBack, Some(cause), None)
                        .await;
                }
                WatchEnd::NotReady(_) => {
                    let _ = self.stop_for_rollback(exec).await;
                    self.end_with(exec, OpStatus::Interrupted, Some(Cause::StepFailed), None)
                        .await;
                }
            }
        } else {
            self.end_with(exec, OpStatus::Interrupted, Some(Cause::NotRestarted), None)
                .await;
        }
    }

    /// Arrête le conteneur relancé avec les nouveaux mods avant de remettre les anciens (rien n'est déplacé sous un jeu
    /// qui tourne).
    async fn stop_for_rollback(&self, exec: &Exec) -> bool {
        if !exec.running {
            return true;
        }
        // L'arrêt passe par le chemin commun : arrêt voulu inscrit, « En erreur » jamais montré (HRT-259).
        self.deps().backups.stop_again_for_mods(&exec.job).await
    }

    /// Lit les 20 dernières lignes du jeu relancé et les garde avec la cause reconnue (liste fermée).
    async fn keep_tail_of(&self, exec: &Exec) {
        use futures_util::StreamExt;
        let Some(container) = exec.job.stopped_containers().first().cloned() else {
            return;
        };
        let deps = self.deps();
        let tty = deps
            .engine
            .inspect(&container)
            .await
            .ok()
            .flatten()
            .is_some_and(|observation| observation.tty);
        let mut stream = deps.engine.logs(
            &container,
            LogQuery {
                follow: false,
                tail: Some(20),
                since_ms: None,
                tty,
            },
        );
        let mut lines: Vec<String> = Vec::new();
        let read = async {
            while let Some(item) = stream.next().await {
                match item {
                    Ok(line) => {
                        lines.push(line.text.clone());
                        if lines.len() > 20 {
                            lines.remove(0);
                        }
                    }
                    Err(_) => break,
                }
            }
        };
        let _ = tokio::time::timeout(Duration::from_secs(5), read).await;
        let reason = crate::domain::mods::failure::reason_of(lines.iter().map(String::as_str));
        self.keep_tail(exec.job.op_id(), lines, reason);
    }

    /// Clôt l'opération sans fichier posé à inscrire (échec ou annulation avant ou pendant la mise en place).
    async fn end(&self, exec: &Exec, status: OpStatus, cause: Option<Cause>) {
        // Le service arrêté par l'opération est relancé, quelle que soit l'issue.
        if exec.running && !exec.job.stopped_containers().is_empty() {
            let deps = self.deps();
            let id = exec.job.op_id();
            let _ = deps.ops.intent(id, step::START, &UndoInfo::default()).await;
            if deps.backups.start_for_mods(&exec.job).await.is_ok() {
                self.done_step(id, step::START).await;
            } else {
                let _ = deps.ops.failed(id, step::START).await;
                let cause = if status == OpStatus::Succeeded {
                    Some(Cause::NotRestarted)
                } else {
                    cause
                };
                return self.end_with(exec, status, cause, None).await;
            }
        }
        self.end_with(exec, status, cause, None).await;
    }

    /// La clôture : statut, entrée du journal d'activité et origines `hearth` dans UNE transaction.
    async fn end_with(
        &self,
        exec: &Exec,
        status: OpStatus,
        cause: Option<Cause>,
        placed: Option<&[FileMove]>,
    ) {
        let deps = self.deps();
        let id = exec.job.op_id();
        let cancelling = self.is_cancelling(id).await;
        let (status, cause) = match status {
            OpStatus::Failed if cancelling => {
                (OpStatus::Cancelled, Some(Cause::CancelledByAccount))
            }
            other => (other, cause),
        };
        let posted = match placed {
            Some(moves) if status == OpStatus::Succeeded => Some(self.posted_of(exec, moves).await),
            _ => None,
        };
        let draft = ClientName::parse(exec.job.name()).map(|service| {
            crate::application::service_ops::AuditDraft {
                actor: exec.job.actor().clone(),
                action: AuditAction::ServiceMods,
                target: Target::ServiceMods {
                    service,
                    summary: if status == OpStatus::Succeeded {
                        exec.summary.clone()
                    } else {
                        ModsSummary::default()
                    },
                },
                outcome: if status == OpStatus::Succeeded {
                    Outcome::Succeeded
                } else {
                    Outcome::Failed(cause.map_or(
                        Reason::Internal,
                        crate::application::service_lifecycle::reason_of,
                    ))
                },
            }
        });
        match deps
            .ops
            .finish_with_origins(id, status, cause, draft, posted.as_ref())
            .await
        {
            Ok(()) => {}
            Err(error) => {
                tracing::error!(%error, operation = id, "opération de mods non close dans le journal");
                deps.ops.settle(id, None).await;
            }
        }
        tracing::info!(operation = id, status = ?status, "application d'un lot de mods terminée");
    }

    /// Les fichiers posés par ce lot qui sont encore dans le dossier des mods, pour leurs origines `hearth`.
    async fn posted_of(&self, exec: &Exec, moves: &[FileMove]) -> PostedMods {
        let (fs, key) = (Arc::clone(&self.fs), exec.key.clone());
        let candidates: Vec<(String, [u8; 32], RelPath)> = exec
            .actions
            .iter()
            .filter_map(|action| match action {
                Action::Place { add, file } => {
                    let sha = exec.adds.get(*add)?.sha256;
                    let rel = parse(&rel_text(&exec.mods_dir, file)).ok()?;
                    Some((file.clone(), sha, rel))
                }
                Action::Move { .. } => None,
            })
            .collect();
        let _ = moves;
        let files = tokio::task::spawn_blocking(move || {
            let Ok(root) = fs.open_service_root(&key) else {
                return Vec::new();
            };
            candidates
                .into_iter()
                .filter_map(|(file, sha, rel)| {
                    let meta = root.stat(&rel).ok()?;
                    Some(HearthFile {
                        file_name: listing::active_name(&file).to_owned(),
                        size: meta.size,
                        sha256: hex(&sha),
                    })
                })
                .collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();
        PostedMods {
            service: exec.key.clone(),
            files,
        }
    }
}

enum PlaceEnd {
    Done(Vec<FileMove>),
    Cancelled,
    Failed,
}

enum WatchEnd {
    Ready,
    NotReady(Cause),
}

/// Les fichiers de `mods`, tels que la lecture les a vus (un nom hors de l'alphabet sûr n'y figure pas).
fn installed_of(gathered: &Gathered) -> Vec<Installed> {
    gathered
        .files
        .iter()
        .filter(|f| f.place == ModPlace::Active)
        .filter_map(|f| {
            let name = f.name.as_ref()?;
            let from_modrinth_pack = gathered
                .manifest
                .as_ref()
                .is_some_and(|m| m.mod_files.contains(listing::active_name(name)));
            Some(Installed {
                file: name.clone(),
                enabled: f.enabled,
                identities: f
                    .read
                    .as_ref()
                    .map(|c| c.identities.clone())
                    .unwrap_or_default(),
                from_modrinth_pack,
            })
        })
        .collect()
}

fn retired_of(gathered: &Gathered) -> Vec<String> {
    gathered
        .files
        .iter()
        .filter(|f| f.place == ModPlace::Retired)
        .filter_map(|f| f.name.clone())
        .collect()
}

fn summary_of(request: &ModsApplyRequest, actions: &[Action]) -> ModsSummary {
    let mut summary = ModsSummary {
        added: u32::try_from(request.add.iter().filter(|a| a.replace.is_none()).count())
            .unwrap_or(u32::MAX),
        replaced: u32::try_from(request.add.iter().filter(|a| a.replace.is_some()).count())
            .unwrap_or(u32::MAX),
        retired: u32::try_from(request.retire.len()).unwrap_or(u32::MAX),
        restored: u32::try_from(request.restore.len()).unwrap_or(u32::MAX),
        disabled: u32::try_from(request.disable.len()).unwrap_or(u32::MAX),
        enabled: u32::try_from(request.enable.len()).unwrap_or(u32::MAX),
        names: Vec::new(),
    };
    for action in actions {
        match action {
            Action::Move { from, .. } => summary.name(from),
            Action::Place { file, .. } => summary.name(file),
        }
    }
    summary
}

/// Ce qu'un retour a rendu : `all` quand tout est revenu, `moves` les mouvements revenus (à écrire au journal).
#[derive(Debug, Default)]
struct Reverted {
    all: bool,
    moves: Vec<FileMove>,
}

/// Le journal ne garde que l'intention de ce mouvement : l'inode n'y est pas.
fn intent_only(mv: &FileMove) -> bool {
    mv.ino == 0
}

/// Rejoue les mouvements à l'envers : l'inode est vérifié, rien n'est supprimé ni remplacé. Un fichier posé par
/// l'opération part dans « Mods retirés ». Rejouable : un mouvement déjà revenu compte comme revenu.
fn revert_blocking(
    fs: &dyn SafeFs,
    key: &ServiceKey,
    retired_dir: &RelPath,
    moves: &[FileMove],
) -> Reverted {
    let Ok(root) = fs.open_service_root(key) else {
        return Reverted::default();
    };
    let mut outcome = Reverted {
        all: true,
        moves: Vec::new(),
    };
    for mv in moves.iter().rev() {
        let mut mv = mv.clone();
        let (Ok(to), from) = (parse(&mv.to), parse(&mv.from)) else {
            outcome.all = false;
            continue;
        };
        let from = match (mv.kind, from) {
            (MoveKind::Placed, _) => None,
            (_, Ok(from)) => Some(from),
            (_, Err(_)) => {
                outcome.all = false;
                continue;
            }
        };
        // Un mouvement dont seule l'INTENTION est au journal (l'agent s'est arrêté pendant ce geste, ou la ligne
        // `done` n'a pas pu s'écrire). Un déplacement : la cible est celle de ce geste si, et seulement si, la
        // source a disparu. Une pose : rien ne dit quel fichier c'est, rien n'est adopté, la main de l'admin reste.
        if intent_only(&mv) {
            let source = from.as_ref().map(|from| root.stat(from));
            match (root.identify(&to), source) {
                (Err(FsError::NotFound), _) => {
                    // Le geste n'a pas eu lieu : rien à défaire.
                    outcome.moves.push(mv);
                    continue;
                }
                (Ok(found), Some(Err(FsError::NotFound))) => {
                    mv.dev = found.dev;
                    mv.ino = found.ino;
                }
                _ => {
                    outcome.all = false;
                    continue;
                }
            }
        }
        if revert_one(root.as_ref(), retired_dir, &mv, &to, from.as_ref()) {
            outcome.moves.push(mv);
        } else {
            outcome.all = false;
        }
    }
    outcome
}

fn revert_one(
    root: &dyn ServiceRoot,
    retired_dir: &RelPath,
    mv: &FileMove,
    to: &RelPath,
    from: Option<&RelPath>,
) -> bool {
    let wanted = FileId {
        dev: mv.dev,
        ino: mv.ino,
    };
    // Le fichier est-il TOUJOURS celui que l'opération a déplacé ou posé ?
    match root.identify(to) {
        Ok(found) if found == wanted => {}
        // Plus rien à la cible : un retour interrompu a peut-être déjà rendu ce mouvement (le journal ne le dit pas
        // toujours). C'est fait si le fichier de même identité est revenu à sa place, ou rangé dans « Mods retirés ».
        Err(FsError::NotFound) => return already_reverted(root, retired_dir, mv, to, from, wanted),
        _ => return false,
    }
    let target = match (mv.kind, from) {
        (MoveKind::Placed, _) | (_, None) => {
            // Mis de côté dans « Mods retirés », sous le premier nom libre.
            let Some(name) = to.split_last().map(|(leaf, _)| leaf.to_owned()) else {
                return false;
            };
            let free = (1..=listing::MAX_RETIRED_SUFFIX)
                .filter_map(|n| listing::retired_name(&name, n))
                .find(|candidate| {
                    retired_dir
                        .child(candidate)
                        .is_ok_and(|rel| matches!(root.stat(&rel), Err(FsError::NotFound)))
                });
            let Some(rel) = free.and_then(|candidate| retired_dir.child(&candidate).ok()) else {
                return false;
            };
            rel
        }
        (_, Some(from)) => from.clone(),
    };
    root.rename_noreplace(to, &target).is_ok()
}

/// Le mouvement est-il déjà revenu ? `from` porte la même identité (déplacement), ou un nom de « Mods retirés »
/// porte celle du fichier posé.
fn already_reverted(
    root: &dyn ServiceRoot,
    retired_dir: &RelPath,
    mv: &FileMove,
    to: &RelPath,
    from: Option<&RelPath>,
    wanted: FileId,
) -> bool {
    if let (false, Some(from)) = (mv.kind == MoveKind::Placed, from) {
        return root.identify(from).is_ok_and(|found| found == wanted);
    }
    let Some(name) = to.split_last().map(|(leaf, _)| leaf.to_owned()) else {
        return false;
    };
    (1..=listing::MAX_RETIRED_SUFFIX)
        .filter_map(|n| listing::retired_name(&name, n))
        .filter_map(|candidate| retired_dir.child(&candidate).ok())
        .any(|rel| root.identify(&rel).is_ok_and(|found| found == wanted))
}

/// Les mouvements du journal d'une opération, dans l'ordre : une ligne `done` (avec l'inode) remplace l'intention du
/// même mouvement ; une ligne `undone` qui porte un mouvement le retire (il est revenu), une ligne `undone` sans
/// mouvement clôt l'étape : plus rien à défaire.
fn moves_of(steps: &[StepRecord]) -> Vec<FileMove> {
    let mut ordered: Vec<&StepRecord> = steps
        .iter()
        .filter(|row| row.step.as_str() == step::PLACE)
        .collect();
    ordered.sort_by_key(|row| row.seq);
    let mut moves: Vec<FileMove> = Vec::new();
    for row in ordered {
        for mv in &row.undo.moves {
            let same = |other: &FileMove| {
                other.kind == mv.kind && other.from == mv.from && other.to == mv.to
            };
            match (row.state, moves.iter().position(same)) {
                (StepState::Undone, Some(at)) => {
                    moves.remove(at);
                }
                (StepState::Undone, None) => {}
                (StepState::Done, Some(at)) => moves[at] = mv.clone(),
                (StepState::Intent, None) | (StepState::Done, None) => moves.push(mv.clone()),
                _ => {}
            }
        }
        if row.state == StepState::Undone && row.undo.moves.is_empty() {
            moves.clear();
        }
    }
    moves
}

#[async_trait::async_trait]
impl DecisionEffects for ModsService {
    async fn decided(self: Arc<Self>, snapshot: OpSnapshot, choice: Choice) {
        if snapshot.record.kind == OpKind::Mods && choice == Choice::Revert {
            self.revert_interrupted(&snapshot).await;
        }
    }
}

impl ModsService {
    /// « Revenir à l'état d'avant » d'un lot « à vérifier » (HRT-259) : les mouvements du journal rejoués à l'envers,
    /// l'inode vérifié, le service qui tournait relancé. Rien n'est supprimé. Le retour qui échoue : `interrupted`.
    async fn revert_interrupted(&self, snapshot: &OpSnapshot) {
        let deps = self.deps();
        let record = &snapshot.record;
        let reverted = async {
            let Ok(Judged::Read(plan)) = self.plan_of(&record.service).await else {
                return false;
            };
            let Ok((_, _, rule)) = self.fiche_perms(&record.service).await else {
                return false;
            };
            let moves = moves_of(&snapshot.steps);
            let was_running = record
                .plan
                .steps()
                .iter()
                .any(|planned| planned.name.as_str() == step::STOP);
            // Les conteneurs consignés sont toujours arrêtés avant de déplacer : `stop_running` ne fait rien si rien ne
            // tourne, et un jeu relancé à la main entre-temps ne doit pas avoir ses fichiers déplacés sous lui.
            let containers = deps.backups.containers_for(&record.service).await;
            let Some(job) = deps.backups.revert_job(
                &record.service,
                &record.id,
                &record.service_name,
                &record.account,
                containers.clone(),
            ) else {
                return false;
            };
            // Rien n'est déplacé sous un jeu qui tourne : un arrêt qui échoue ne déplace rien (l'arrêt voulu est
            // inscrit, comme pour le retour automatique).
            if !containers.is_empty() && !deps.backups.stop_again_for_mods(&job).await {
                return false;
            }
            let (fs, key, retired) = (
                Arc::clone(&self.fs),
                record.service.clone(),
                plan.retired_dir.clone(),
            );
            let outcome = tokio::task::spawn_blocking(move || {
                revert_blocking(fs.as_ref(), &key, &retired, &moves)
            })
            .await
            .unwrap_or_default();
            self.write_undone(&record.id, &outcome).await;
            if !outcome.all {
                return false;
            }
            if !was_running || containers.is_empty() {
                return true;
            }
            // La relance et la surveillance de l'état d'avant, comme le retour automatique : un jeu qui ne revient
            // pas est arrêté et rien n'est relancé seul.
            if deps.backups.start_for_mods(&job).await.is_err() {
                return false;
            }
            match self.watch_service(&record.service, &job, &rule).await {
                WatchEnd::Ready => true,
                WatchEnd::NotReady(_) => {
                    let _ = deps.backups.stop_again_for_mods(&job).await;
                    false
                }
            }
        }
        .await;
        let (status, cause) = if reverted {
            (OpStatus::RolledBack, Some(Cause::DecidedByAccount))
        } else {
            (OpStatus::Interrupted, Some(Cause::StepFailed))
        };
        if let Err(error) = deps.ops.finish(&record.id, status, cause).await {
            tracing::error!(%error, "retour d'un lot de mods non clos dans le journal");
        }
    }
}

#[cfg(test)]
#[path = "mods_apply_tests.rs"]
mod tests;
