//! Les mods d'un serveur de jeu installé (HRT-256, ADR-0062, BR-MOD-020 à 033) : la liste, lue dans le volume du service.
//!
//! Le dossier des mods est écrit par le jeu ET par le code de chaque mod : hostile par hypothèse. Tout passe par
//! `SafeFs` (aucun lien suivi, aucun montage franchi), chaque jar est une archive hostile lue par
//! `domain::archive` (jugée entière avant la moindre lecture), le tout dans un budget de temps et de mémoire.
//! La route ne modifie rien : ni le volume, ni la base.
//!
//! Aucun nom de fichier, aucune ligne du jeu n'entre dans une trace ni dans une erreur.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex as StdMutex, PoisonError};
use std::time::{Duration, Instant};

use hearth_proto::api::mods::{
    Applicable, CheckOutcome, EnvironmentView, MinecraftFrom, ModCheckFile, ModIdentityView,
    ModItem, ModOrigin, ModPlace, ModReplaces, ModVerdictView, ModsCheckView, ModsGame, ModsView,
    PackKind, RefusalValues, VerdictCode,
};
use thiserror::Error;
use tokio::sync::Mutex;

use super::catalog::CatalogService;
use super::pack_intake::{jar_limits, pack_limits};
use super::ports::{
    CurrentMods, DepositError, DepositFile, DepositId, FsError, ModOrigins, ModsRead, ModsReading,
    ModsSnapshot, PackDeposits, ReadFile, ServiceRoot, StoreError,
};
use super::service_specs::{ServiceSpecs, SpecServiceError};
use crate::domain::archive::{self, ArchiveRefusal, Inspection};
use crate::domain::catalog::loading::model::{Fiche, hex};
use crate::domain::catalog::translate::pack_file_name;
use crate::domain::files::RelPath;
use crate::domain::mods::backup_list::{
    CurrentMod, IdentitySummary, Place, Snapshot, SnapshotFile,
};
use crate::domain::mods::compat::{self, Candidate, ServerProfile, Verdict};
use crate::domain::mods::deposit::{self, Deposit, Installed, Refusal};
use crate::domain::mods::game::is_game_version;
use crate::domain::mods::game_info::{
    FileFacts, LOG_READ_BYTES, MANIFEST_READ_BYTES, ModrinthManifest, Origin, OriginRow,
    parse_modrinth_manifest, resolve_origin, version_from_log,
};
use crate::domain::mods::jar::{self, Environment, IDENTITY_FILES, JarIdentity, JarLoader};
use crate::domain::mods::listing;
use crate::domain::mods::pack::{DescriptionError, DescriptionReader};
use crate::domain::mods::version_req::GameVersion;
use crate::domain::services::ServiceKey;
use crate::domain::services::deploy::{flatten_target, normalize_target};

/// Le temps qu'une lecture de liste peut prendre ; au-delà, la réponse est partielle (`unread`) et le client relit.
const READ_BUDGET: Duration = Duration::from_secs(20);
/// Au plus ce nombre de fichiers listés par dossier (le reste est `truncated`).
const MAX_FILES_PER_DIR: usize = 1_024;
/// Le cache d'identités en mémoire : au plus ce nombre de jars.
const CACHE_CAP: usize = 4_096;
#[path = "mods_apply.rs"]
mod apply;
pub use apply::{ApplyDeps, ApplyError, ApplyTimings};

const MANIFEST_FILE: &str = ".modrinth-modpack-manifest.json";

#[derive(Debug, Error)]
pub enum ModsError {
    /// Pas un serveur de jeu créé depuis une fiche (inconnu, adopté, déployé à la main).
    #[error("ce n'est pas un serveur de jeu créé depuis une fiche")]
    NotAGameService,
    /// `data/mods` ou `data/mods-retires` est un lien ou n'est pas un dossier.
    #[error("le dossier des mods est bloqué")]
    FolderBlocked,
    /// Le lot à contrôler est vide, de plus de 8 dépôts ou répète un dépôt.
    #[error("lot de dépôts invalide")]
    BatchInvalid,
    /// Un dépôt n'existe pas pour ce compte.
    #[error("dépôt inconnu")]
    DepositUnknown,
    /// Le moteur du serveur ne charge pas de mods.
    #[error("ce serveur ne prend pas de mods")]
    NotApplicable,
    #[error("stockage : {0}")]
    Storage(String),
}

impl From<SpecServiceError> for ModsError {
    fn from(error: SpecServiceError) -> Self {
        Self::Storage(error.to_string())
    }
}

impl From<StoreError> for ModsError {
    fn from(error: StoreError) -> Self {
        Self::Storage(error.to_string())
    }
}

/// Ce que la lecture d'un jar a donné, gardé en mémoire avec l'identité du fichier.
#[derive(Debug, Clone)]
struct Cached {
    identities: Vec<JarIdentity>,
    crc32: Option<u32>,
    sha256: String,
}

/// Un fichier est le même s'il a même place, même nom, même taille et même date (BR-MOD-020).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    service: String,
    place: &'static str,
    name: String,
    size: u64,
    mtime_ns: i64,
}

pub struct ModsService {
    specs: Arc<ServiceSpecs>,
    catalog: Arc<CatalogService>,
    fs: Arc<dyn super::ports::SafeFs>,
    origins: Arc<dyn ModOrigins>,
    cache: Arc<StdMutex<HashMap<CacheKey, Cached>>>,
    /// Une lecture de liste à la fois par service.
    reading: StdMutex<HashMap<String, Arc<Mutex<()>>>>,
    /// Les services dont le pack serveur range a deja ete relu pour inscrire l'origine de ses mods (HRT-256 : un
    /// serveur installe avant la 0.2.2 n'a pas de lignes) : une seule relecture par service et par execution.
    pack_reread: StdMutex<HashSet<String>>,
    /// La zone d'attente des dépôts, pour contrôler des jars (HRT-257) ; posée par la racine de composition.
    deposits: std::sync::OnceLock<Arc<dyn PackDeposits>>,
    /// Ce que l'application d'un lot demande (HRT-258) ; posé par la racine de composition.
    apply_deps: std::sync::OnceLock<ApplyDeps>,
    /// Les dernières lignes du jeu d'un lot qui n'a pas démarré, avec la cause reconnue : 24 heures en mémoire seulement
    /// (HRT-259, BR-MOD-027).
    tails: StdMutex<HashMap<String, KeptTail>>,
    budget: Duration,
}

/// Une queue de journal gardée pour une opération `mods` qui a dû revenir en arrière.
struct KeptTail {
    at: std::time::Instant,
    lines: Vec<String>,
    reason: Option<crate::domain::mods::failure::FailureReason>,
}

/// Ce que la fiche et la description disent du serveur, avant de toucher au volume.
struct Plan {
    variant: String,
    version_answer: Option<String>,
    updates_every_start: bool,
    mods_dir: RelPath,
    retired_dir: RelPath,
    data_dir: RelPath,
    /// Ou est range le pack serveur d'un serveur installe avec un pack (dossier des packs de la fiche, nom fixe).
    pack_rel: Option<RelPath>,
    /// Les noms des deux dossiers de mods et le point de montage du dossier gere qui les porte (HRT-293).
    dir: String,
    retired_dir_name: String,
    data_target: String,
}

enum Judged {
    Done(Applicable),
    Read(Box<Plan>),
}

fn loader_of_variant(variant: &str) -> Option<JarLoader> {
    match variant {
        "fabric" => Some(JarLoader::Fabric),
        "quilt" => Some(JarLoader::Quilt),
        "forge" => Some(JarLoader::Forge),
        "neoforge" => Some(JarLoader::NeoForge),
        _ => None,
    }
}

impl ModsService {
    fn mark_reread(&self, key: &ServiceKey) {
        self.pack_reread
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key.as_str().to_owned());
    }

    pub fn new(
        specs: Arc<ServiceSpecs>,
        catalog: Arc<CatalogService>,
        fs: Arc<dyn super::ports::SafeFs>,
        origins: Arc<dyn ModOrigins>,
    ) -> Self {
        Self {
            specs,
            catalog,
            fs,
            origins,
            cache: Arc::new(StdMutex::new(HashMap::new())),
            reading: StdMutex::new(HashMap::new()),
            pack_reread: StdMutex::new(HashSet::new()),
            deposits: std::sync::OnceLock::new(),
            apply_deps: std::sync::OnceLock::new(),
            tails: StdMutex::new(HashMap::new()),
            budget: READ_BUDGET,
        }
    }

    /// Garde la queue du jeu d'une opération (20 lignes au plus, 24 heures, 64 opérations au plus) avec sa cause.
    fn keep_tail(
        &self,
        operation: &str,
        lines: Vec<String>,
        reason: Option<crate::domain::mods::failure::FailureReason>,
    ) {
        const KEEP: Duration = Duration::from_secs(24 * 3600);
        const MAX: usize = 64;
        let now = std::time::Instant::now();
        let mut kept = self.tails.lock().unwrap_or_else(PoisonError::into_inner);
        kept.retain(|_, tail| now.saturating_duration_since(tail.at) < KEEP);
        if kept.len() >= MAX
            && let Some(oldest) = kept
                .iter()
                .min_by_key(|(_, tail)| tail.at)
                .map(|(id, _)| id.clone())
        {
            kept.remove(&oldest);
        }
        kept.insert(
            operation.to_owned(),
            KeptTail {
                at: now,
                lines,
                reason,
            },
        );
    }

    /// `GET /operations/services/{id}/tail` pour une opération `mods` : les dernières lignes du jeu qui n'a pas démarré
    /// avec la cause reconnue (HRT-259). `None` : aucune, ou plus de 24 heures.
    pub fn tail(
        &self,
        operation: &str,
    ) -> Option<hearth_proto::api::service_update::OperationTail> {
        let now = std::time::Instant::now();
        let mut kept = self.tails.lock().unwrap_or_else(PoisonError::into_inner);
        kept.retain(|_, tail| {
            now.saturating_duration_since(tail.at) < Duration::from_secs(24 * 3600)
        });
        kept.get(operation)
            .map(|tail| hearth_proto::api::service_update::OperationTail {
                lines: tail.lines.clone(),
                reason: tail.reason.map(|r| r.code().to_owned()),
                sentence: tail.reason.map(|r| r.sentence().to_owned()),
            })
    }

    /// Un budget de lecture plus court (les essais).
    #[must_use]
    pub fn with_budget(mut self, budget: Duration) -> Self {
        self.budget = budget;
        self
    }

    /// Ce que la description et la fiche disent du serveur : le plan de lecture, ou pourquoi il n'y en a pas.
    async fn plan_of(&self, key: &ServiceKey) -> Result<Judged, ModsError> {
        let Some(body) = self.specs.body(key).await? else {
            return Err(ModsError::NotAGameService);
        };
        let Some(spec_fiche) = &body.fiche else {
            return Err(ModsError::NotAGameService);
        };
        let fiche = self
            .catalog
            .fiche(&spec_fiche.id)
            .ok_or(ModsError::NotAGameService)?;
        Ok(judge_applicable(fiche, spec_fiche))
    }

    /// Lit le volume (une lecture de liste à la fois par service) : les lignes d'origine et les fichiers.
    async fn read_volume(
        &self,
        key: &ServiceKey,
        plan: &Plan,
    ) -> Result<(Vec<OriginRow>, Gathered), ModsError> {
        self.read_volume_within(key, plan, self.budget).await
    }

    async fn read_volume_within(
        &self,
        key: &ServiceKey,
        plan: &Plan,
        budget: Duration,
    ) -> Result<(Vec<OriginRow>, Gathered), ModsError> {
        let gate = {
            let mut map = self.reading.lock().unwrap_or_else(|e| e.into_inner());
            Arc::clone(map.entry(key.as_str().to_owned()).or_default())
        };
        let _one_at_a_time = gate.lock().await;
        let mut rows = self.origins.rows(key).await?;
        let wants_reread = plan.variant != "modrinth"
            && plan.pack_rel.is_some()
            && !rows.iter().any(|row| !row.from_hearth)
            && !self
                .pack_reread
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .contains(key.as_str());
        let gathered = {
            let fs = Arc::clone(&self.fs);
            let cache = Arc::clone(&self.cache);
            let key = key.clone();
            let input = Gather {
                data_dir: plan.data_dir.clone(),
                mods_dir: plan.mods_dir.clone(),
                retired_dir: plan.retired_dir.clone(),
                modrinth: plan.variant == "modrinth",
                reread_pack: wants_reread.then(|| plan.pack_rel.clone()).flatten(),
            };
            tokio::task::spawn_blocking(move || gather(fs.as_ref(), &cache, &key, &input, budget))
                .await
                .map_err(|error| ModsError::Storage(error.to_string()))??
        };
        let mut gathered = gathered;
        match std::mem::replace(&mut gathered.pack, PackRead::NotAsked) {
            PackRead::Mods(found) => {
                // Inscrit une fois ; la reponse en tient deja compte. Une base qui refuse n'empeche pas la liste.
                if !found.is_empty()
                    && let Err(error) = self
                        .origins
                        .record_server_pack(key, &found, time::OffsetDateTime::now_utc())
                        .await
                {
                    tracing::warn!(%error, "origine des mods du pack non inscrite");
                }
                rows.extend(found);
                self.mark_reread(key);
            }
            PackRead::Done => self.mark_reread(key),
            PackRead::OutOfTime | PackRead::NotAsked => {}
        }
        Ok((rows, gathered))
    }

    /// `GET /services/{key}/mods` (BR-MOD-020, 021, 028, 030, 032, 033).
    pub async fn list(&self, key: &ServiceKey) -> Result<ModsView, ModsError> {
        let plan = match self.plan_of(key).await? {
            Judged::Done(applicable) => {
                return Ok(ModsView {
                    applicable,
                    game: ModsGame::default(),
                    mods: Vec::new(),
                    unread: 0,
                    truncated: false,
                });
            }
            Judged::Read(plan) => plan,
        };
        let (rows, gathered) = self.read_volume(key, &plan).await?;
        Ok(assemble(&plan, &rows, gathered))
    }

    /// Branche la zone d'attente des dépôts (racine de composition, une seule fois) : sans elle, `check` refuse.
    pub fn set_deposits(&self, deposits: Arc<dyn PackDeposits>) {
        let _ = self.deposits.set(deposits);
    }

    /// `POST /services/{key}/mods/check` (HRT-257, BR-MOD-021, 022, 023) : juge des `.jar` déposés sans rien écrire dans
    /// le service. Un dépôt refusé est retiré de la zone d'attente aussitôt ; un dépôt accepté y reste.
    pub async fn check(
        &self,
        key: &ServiceKey,
        account: &str,
        deposits: &[String],
    ) -> Result<ModsCheckView, ModsError> {
        if deposits.is_empty() || deposits.len() > deposit::MAX_BATCH {
            return Err(ModsError::BatchInvalid);
        }
        let ids: Vec<DepositId> = deposits
            .iter()
            .map(|text| DepositId::parse(text).ok_or(ModsError::DepositUnknown))
            .collect::<Result<_, _>>()?;
        for (i, id) in ids.iter().enumerate() {
            if ids[..i].contains(id) {
                return Err(ModsError::BatchInvalid);
            }
        }
        let plan = match self.plan_of(key).await? {
            Judged::Done(_) => return Err(ModsError::NotApplicable),
            Judged::Read(plan) => plan,
        };
        let pool = Arc::clone(self.deposits.get().ok_or_else(|| {
            ModsError::Storage("zone d'attente des dépôts non branchée".to_owned())
        })?);
        let (rows, gathered) = self.read_volume(key, &plan).await?;
        let game = game_of(&plan, &rows, &gathered);
        let modrinth = gathered.manifest.clone();
        let installed: Vec<Installed> = gathered
            .files
            .iter()
            .filter(|f| f.place == ModPlace::Active && !f.unread)
            .filter_map(|f| {
                let name = f.name.as_ref()?;
                Some(Installed {
                    file: name.clone(),
                    enabled: f.enabled,
                    identities: f
                        .read
                        .as_ref()
                        .map(|c| c.identities.clone())
                        .unwrap_or_default(),
                    from_modrinth_pack: modrinth
                        .as_ref()
                        .is_some_and(|m| m.mod_files.contains(listing::active_name(name))),
                })
            })
            .collect();
        let profile = game.profile.clone();
        let account = account.to_owned();
        let pool2 = Arc::clone(&pool);
        let ids2 = ids.clone();
        let judged = tokio::task::spawn_blocking(move || {
            judge_deposit_files(pool2.as_ref(), &account, &ids2, &profile, &installed)
        })
        .await
        .map_err(|error| ModsError::Storage(error.to_string()))??;
        // Un dépôt refusé quitte la zone d'attente aussitôt (BR-MOD-016) ; un accepté y reste.
        let account = judged.account;
        let mut files = Vec::with_capacity(ids.len());
        for (id, judgement) in ids.iter().zip(judged.judgements) {
            files.push(match judgement {
                Ok(accepted) => ModCheckFile {
                    deposit: id.as_str().to_owned(),
                    outcome: if accepted.replaces.is_some() {
                        CheckOutcome::Replace
                    } else {
                        CheckOutcome::Add
                    },
                    identity: Some(identity_view(&accepted.identity)),
                    target_file: Some(accepted.target_file),
                    replaces: accepted
                        .replaces
                        .map(|(file, version)| ModReplaces { file, version }),
                    verdict: Some(verdict_view(&accepted.verdict, Vec::new())),
                    refusal: None,
                    refusal_values: None,
                },
                Err((refusal, identity)) => {
                    pool.discard(id, &account);
                    let values = match &refusal {
                        Refusal::IncompatibleLoader { jar, server } => Some(RefusalValues {
                            loader: Some(jar.variant_id().to_owned()),
                            server_loader: server.map(|s| s.variant_id().to_owned()),
                            server_minecraft: None,
                        }),
                        Refusal::IncompatibleMinecraft => Some(RefusalValues {
                            loader: None,
                            server_loader: None,
                            server_minecraft: game.minecraft.clone(),
                        }),
                        _ => None,
                    };
                    ModCheckFile {
                        deposit: id.as_str().to_owned(),
                        outcome: CheckOutcome::Refused,
                        identity: identity.as_ref().map(identity_view),
                        target_file: None,
                        replaces: None,
                        verdict: None,
                        refusal: Some(refusal.code().to_owned()),
                        refusal_values: values,
                    }
                }
            });
        }
        Ok(ModsCheckView { files })
    }
}

/// Ce qu'on sait du jeu pour juger : moteur, version, et d'où elle vient.
struct GameInfo {
    loader: Option<JarLoader>,
    minecraft: Option<String>,
    minecraft_from: MinecraftFrom,
    profile: ServerProfile,
}

fn game_of(plan: &Plan, _rows: &[OriginRow], gathered: &Gathered) -> GameInfo {
    let manifest = gathered.manifest.as_ref();
    let loader = loader_of_variant(&plan.variant).or_else(|| manifest.and_then(|m| m.loader));
    let from_setting = plan
        .version_answer
        .as_deref()
        .filter(|value| !value.eq_ignore_ascii_case("latest") && is_game_version(value))
        .map(str::to_owned);
    let (minecraft, minecraft_from) = if plan.variant == "modrinth" {
        match manifest.and_then(|m| m.minecraft.clone()) {
            Some(version) => (Some(version), MinecraftFrom::ModrinthManifest),
            None => (None, MinecraftFrom::Unknown),
        }
    } else if let Some(version) = from_setting {
        (Some(version), MinecraftFrom::Setting)
    } else if let Some(version) = gathered.log_version.clone() {
        (Some(version), MinecraftFrom::Log)
    } else {
        (None, MinecraftFrom::Unknown)
    };
    let profile = ServerProfile {
        loader,
        minecraft: minecraft.as_deref().and_then(GameVersion::parse),
    };
    GameInfo {
        loader,
        minecraft,
        minecraft_from,
        profile,
    }
}

/// Ce que `judge_deposit_files` rend.
struct DepositsJudged {
    account: String,
    judgements: Vec<deposit::Judgement>,
}

/// Ouvre TOUS les dépôts d'abord (un inconnu arrête tout, rien n'est jugé ni retiré), puis lit et juge chacun.
fn judge_deposit_files(
    pool: &dyn PackDeposits,
    account: &str,
    ids: &[DepositId],
    profile: &ServerProfile,
    installed: &[Installed],
) -> Result<DepositsJudged, ModsError> {
    let mut opened = Vec::with_capacity(ids.len());
    for id in ids {
        match pool.open(id, account) {
            Ok(file) => opened.push(file),
            Err(DepositError::NotFound) => return Err(ModsError::DepositUnknown),
            Err(_) => return Err(ModsError::Storage("dépôt illisible".to_owned())),
        }
    }
    let batch: Vec<Deposit> = opened.iter_mut().map(read_deposit).collect();
    Ok(DepositsJudged {
        account: account.to_owned(),
        judgements: deposit::judge_deposits(profile, installed, &batch),
    })
}

/// Lit un jar déposé : archive jugée entière, puis les fichiers d'identité. Aucune erreur n'est recopiée.
fn read_deposit(file: &mut Box<dyn DepositFile>) -> Deposit {
    let limits = jar_limits();
    let refused = |too_large: bool| Deposit {
        archive_refused: !too_large,
        too_large,
        identities: Vec::new(),
    };
    let inspection = match archive::inspect(file, &limits, &|| false) {
        Ok(inspection) => inspection,
        Err(ArchiveRefusal::FileTooLarge { .. }) => return refused(true),
        Err(_) => return refused(false),
    };
    let Ok(found) = archive::read_many(file, &inspection, &IDENTITY_FILES, &limits, &|| false)
    else {
        return refused(false);
    };
    let mut files = JarFiles(
        IDENTITY_FILES
            .into_iter()
            .zip(found)
            .map(|(name, bytes)| (name, bytes.map_err(|_| ())))
            .collect(),
    );
    Deposit {
        archive_refused: false,
        too_large: false,
        identities: jar::identify(&mut files),
    }
}

fn identity_view(identity: &JarIdentity) -> ModIdentityView {
    ModIdentityView {
        id: identity.id.as_str().to_owned(),
        name: identity.name.clone(),
        version: identity.version.as_ref().map(|v| v.as_str().to_owned()),
        loader: identity.loader.variant_id().to_owned(),
        minecraft_declared: identity.minecraft_declared && identity.minecraft.is_some(),
        environment: environment_view(identity.environment),
    }
}

fn judge_applicable(
    fiche: &Fiche,
    spec_fiche: &crate::domain::services::spec::SpecFiche,
) -> Judged {
    let Some(jars) = fiche.file.mods.as_ref().and_then(|mods| mods.jars.as_ref()) else {
        return Judged::Done(Applicable::NoWay);
    };
    let Some(variant) = spec_fiche.variant.clone() else {
        return Judged::Done(Applicable::NoWay);
    };
    if !jars.variants.contains(&variant) {
        return Judged::Done(match variant.as_str() {
            "paper" | "purpur" => Applicable::Plugins,
            _ => Applicable::NoMods,
        });
    }
    let dirs = (|| {
        let folder = fiche.file.folder.iter().find(|f| f.id == jars.folder)?;
        let target = normalize_target(&folder.target).ok()?;
        let flat = flatten_target(&target)?;
        Some((
            RelPath::parse(&flat).ok()?,
            RelPath::parse(&format!("{flat}/{}", jars.dir)).ok()?,
            RelPath::parse(&format!("{flat}/{}", jars.retired_dir)).ok()?,
            target,
        ))
    })();
    let Some((data_dir, mods_dir, retired_dir, data_target)) = dirs else {
        return Judged::Done(Applicable::NoWay);
    };
    let pack_rel = (|| {
        let way = fiche.file.mods.as_ref()?.pack.as_ref()?;
        let folder = fiche.file.folder.iter().find(|f| f.id == way.folder)?;
        let target = normalize_target(&folder.target).ok()?;
        let flat = flatten_target(&target)?;
        RelPath::parse(&format!(
            "{flat}/{}",
            pack_file_name(crate::domain::catalog::loading::schema::fiche::PackGenre::ServerPack)
        ))
        .ok()
    })();
    let version_answer = spec_fiche
        .answers
        .iter()
        .find(|answer| answer.id == "version" && !answer.secret)
        .and_then(|answer| answer.value.clone());
    let updates_every_start = version_answer
        .as_deref()
        .is_none_or(|value| value.eq_ignore_ascii_case("latest"));
    Judged::Read(Box::new(Plan {
        variant,
        version_answer,
        updates_every_start,
        mods_dir,
        retired_dir,
        data_dir,
        pack_rel,
        dir: jars.dir.clone(),
        retired_dir_name: jars.retired_dir.clone(),
        data_target,
    }))
}

struct Gather {
    data_dir: RelPath,
    mods_dir: RelPath,
    retired_dir: RelPath,
    modrinth: bool,
    /// Le pack serveur a relire une fois pour l'origine de ses mods (service installe avant la 0.2.2).
    reread_pack: Option<RelPath>,
}

/// Ce que la relecture du pack serveur a donne.
enum PackRead {
    NotAsked,
    /// Pack lu : ses mods (peut etre vide), a inscrire.
    Mods(Vec<OriginRow>),
    /// Pack absent, illisible ou refuse : rien a inscrire, pas de nouvel essai.
    Done,
    /// Plus de temps : un prochain essai.
    OutOfTime,
}

/// Un fichier du dossier, tel que la lecture l'a vu.
struct Found {
    rank: u32,
    place: ModPlace,
    /// `None` : nom hors de l'alphabet sûr.
    name: Option<String>,
    enabled: bool,
    size: u64,
    /// `None` : pas lu (lien, fichier spécial, nom illisible, budget épuisé, archive refusée).
    read: Option<Cached>,
    /// Pas lu faute de budget.
    unread: bool,
}

struct Gathered {
    files: Vec<Found>,
    manifest: Option<ModrinthManifest>,
    log_version: Option<String>,
    truncated: bool,
    unread: u32,
    pack: PackRead,
}

fn rel(text: &RelPath, child: &str) -> Option<RelPath> {
    text.child(child).ok()
}

/// Lit le volume : le manifeste Modrinth, le journal de démarrage, les deux dossiers de mods.
fn gather(
    fs: &dyn super::ports::SafeFs,
    cache: &StdMutex<HashMap<CacheKey, Cached>>,
    key: &ServiceKey,
    input: &Gather,
    budget: Duration,
) -> Result<Gathered, ModsError> {
    let deadline = Instant::now() + budget;
    let root = match fs.open_service_root(key) {
        Ok(root) => root,
        Err(FsError::NotFound) => {
            return Ok(Gathered {
                files: Vec::new(),
                manifest: None,
                log_version: None,
                truncated: false,
                unread: 0,
                pack: PackRead::Done,
            });
        }
        Err(_) => return Err(ModsError::FolderBlocked),
    };
    let manifest = if input.modrinth {
        rel(&input.data_dir, MANIFEST_FILE)
            .and_then(|path| root.read_bounded(&path, MANIFEST_READ_BYTES).ok())
            .and_then(|bytes| parse_modrinth_manifest(&bytes))
    } else {
        None
    };
    let log_version = read_log_version(root.as_ref(), &input.data_dir);
    let mut files = Vec::new();
    let mut truncated = false;
    let mut unread = 0u32;
    for (dir, place) in [
        (&input.mods_dir, ModPlace::Active),
        (&input.retired_dir, ModPlace::Retired),
    ] {
        let listing = match root.list_dir(dir) {
            Ok(listing) => listing,
            Err(FsError::NotFound) => continue,
            Err(_) => return Err(ModsError::FolderBlocked),
        };
        let mut rank = 0u32;
        for entry in &listing.entries {
            let is_jar =
                listing::is_active_jar(&entry.name) || listing::is_disabled_jar(&entry.name);
            if !is_jar {
                continue;
            }
            if usize::try_from(rank).unwrap_or(usize::MAX) >= MAX_FILES_PER_DIR {
                truncated = true;
                break;
            }
            let safe = listing::is_safe_file_name(&entry.name);
            let enabled = listing::is_active_jar(&entry.name);
            let regular = matches!(entry.meta.kind, crate::application::ports::EntryKind::File);
            let mut found = Found {
                rank,
                place,
                name: safe.then(|| entry.name.clone()),
                enabled,
                size: entry.meta.size,
                read: None,
                unread: false,
            };
            rank += 1;
            if safe && regular {
                let cache_key = CacheKey {
                    service: key.as_str().to_owned(),
                    place: if place == ModPlace::Active { "a" } else { "r" },
                    name: entry.name.clone(),
                    size: entry.meta.size,
                    mtime_ns: entry.meta.mtime_ns,
                };
                let hit = cache
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&cache_key)
                    .cloned();
                if let Some(hit) = hit {
                    found.read = Some(hit);
                } else if Instant::now() >= deadline {
                    found.unread = true;
                    unread += 1;
                } else if let Some(path) = rel(dir, &entry.name) {
                    match read_jar(root.as_ref(), &path, deadline) {
                        JarRead::Done(cached) => {
                            let mut map = cache.lock().unwrap_or_else(|e| e.into_inner());
                            if map.len() >= CACHE_CAP {
                                map.clear();
                            }
                            map.insert(cache_key, cached.clone());
                            found.read = Some(cached);
                        }
                        JarRead::Refused => {}
                        JarRead::OutOfTime => {
                            found.unread = true;
                            unread += 1;
                        }
                    }
                }
            }
            files.push(found);
        }
    }
    Ok(Gathered {
        files,
        manifest,
        log_version,
        truncated,
        unread,
        pack: match &input.reread_pack {
            Some(path) => read_pack_mods(root.as_ref(), path, budget),
            None => PackRead::NotAsked,
        },
    })
}

/// Relit UNE fois le pack serveur range (lecture seule, bornes de pack, son propre budget) pour inscrire l'origine de
/// ses mods : meme jugement que le depot (`inspect` entier), seuls les `.jar` de `mods/` au nom sur sont gardes.
fn read_pack_mods(root: &dyn ServiceRoot, path: &RelPath, budget: Duration) -> PackRead {
    let deadline = Instant::now() + budget;
    let Ok(mut file) = root.open_read(path) else {
        return PackRead::Done;
    };
    let out_of_time = move || Instant::now() >= deadline;
    match archive::inspect(&mut file, &pack_limits(), &out_of_time) {
        Ok(inspection) => PackRead::Mods(
            inspection
                .entries()
                .iter()
                .filter(|entry| entry.kind == archive::EntryKind::File)
                .filter_map(|entry| {
                    let name = entry.path.as_str().strip_prefix("mods/")?;
                    (!name.contains('/')
                        && listing::is_active_jar(name)
                        && listing::is_safe_file_name(name))
                    .then_some(())?;
                    Some(OriginRow {
                        file_name: name.to_owned(),
                        size: entry.size,
                        crc32: Some(entry.crc32?),
                        sha256: None,
                        from_hearth: false,
                    })
                })
                .take(crate::application::ports::MAX_ORIGIN_ROWS)
                .collect(),
        ),
        Err(_) if out_of_time() => PackRead::OutOfTime,
        Err(_) => PackRead::Done,
    }
}

fn read_log_version(root: &dyn ServiceRoot, data_dir: &RelPath) -> Option<String> {
    let path = data_dir.child("logs").ok()?.child("latest.log").ok()?;
    let file = root.open_read(&path).ok()?;
    let mut bytes = Vec::new();
    file.take(LOG_READ_BYTES).read_to_end(&mut bytes).ok()?;
    version_from_log(&bytes)
}

enum JarRead {
    Done(Cached),
    /// Archive refusée par le lecteur d'archives hostiles, ou illisible : « Non reconnu ».
    Refused,
    OutOfTime,
}

/// Les fichiers d'identité d'un jar, lus en une passe après le jugement de l'archive entière.
struct JarFiles(Vec<(&'static str, Result<Vec<u8>, ()>)>);

impl DescriptionReader for JarFiles {
    fn read(&mut self, name: &str) -> Result<Vec<u8>, DescriptionError> {
        match self.0.iter().find(|(known, _)| *known == name) {
            Some((_, Ok(bytes))) => Ok(bytes.clone()),
            _ => Err(DescriptionError::Unreadable),
        }
    }
}

fn read_jar(root: &dyn ServiceRoot, path: &RelPath, deadline: Instant) -> JarRead {
    let Ok(mut file) = root.open_read(path) else {
        return JarRead::Refused;
    };
    let limits = jar_limits();
    let out_of_time = move || Instant::now() >= deadline;
    let inspection: Inspection = match archive::inspect(&mut file, &limits, &out_of_time) {
        Ok(inspection) => inspection,
        Err(_) if out_of_time() => return JarRead::OutOfTime,
        Err(_) => return JarRead::Refused,
    };
    let found = match archive::read_many(
        &mut file,
        &inspection,
        &IDENTITY_FILES,
        &limits,
        &out_of_time,
    ) {
        Ok(found) => found,
        Err(_) if out_of_time() => return JarRead::OutOfTime,
        Err(_) => return JarRead::Refused,
    };
    let mut files = JarFiles(
        IDENTITY_FILES
            .into_iter()
            .zip(found)
            .map(|(name, bytes)| (name, bytes.map_err(|_| ())))
            .collect(),
    );
    let identities = jar::identify(&mut files);
    let crc32 = crc32_of(&mut file, &out_of_time);
    JarRead::Done(Cached {
        identities,
        crc32,
        sha256: hex(inspection.sha256()),
    })
}

/// Le CRC-32 du fichier entier (celui d'une entrée de pack serveur qui contient le même jar).
fn crc32_of(file: &mut Box<dyn ReadFile>, cancel: &dyn Fn() -> bool) -> Option<u32> {
    file.seek(SeekFrom::Start(0)).ok()?;
    let mut crc = flate2::Crc::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        if cancel() {
            return None;
        }
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            return Some(crc.sum());
        }
        crc.update(&buffer[..read]);
    }
}

fn environment_view(environment: Environment) -> EnvironmentView {
    match environment {
        Environment::Both => EnvironmentView::Both,
        Environment::ServerOnly => EnvironmentView::Server,
        Environment::ClientOnly => EnvironmentView::Client,
        Environment::Unknown => EnvironmentView::Unknown,
    }
}

fn origin_view(origin: Origin) -> ModOrigin {
    match origin {
        Origin::ModrinthPack => ModOrigin::ModrinthPack,
        Origin::Hearth => ModOrigin::Hearth,
        Origin::ServerPack => ModOrigin::ServerPack,
        Origin::Manual => ModOrigin::Manual,
    }
}

fn verdict_view(verdict: &Verdict, needed_by: Vec<String>) -> ModVerdictView {
    let (code, missing) = match verdict {
        Verdict::Compatible => (VerdictCode::Compatible, Vec::new()),
        Verdict::Unverified => (VerdictCode::Unverified, Vec::new()),
        Verdict::ToComplete(ids) => (
            VerdictCode::ToComplete,
            ids.iter().map(|id| id.as_str().to_owned()).collect(),
        ),
        Verdict::IncompatibleLoader { .. } => (VerdictCode::IncompatibleLoader, Vec::new()),
        Verdict::IncompatibleMinecraft => (VerdictCode::IncompatibleMinecraft, Vec::new()),
        Verdict::Duplicate => (VerdictCode::Duplicate, Vec::new()),
        Verdict::Unrecognized => (VerdictCode::Unrecognized, Vec::new()),
    };
    ModVerdictView {
        code,
        missing,
        needed_by,
    }
}

/// Assemble la réponse : le jeu (moteur, version et sa source), puis chaque fichier avec son verdict et son origine.
fn assemble(plan: &Plan, rows: &[OriginRow], gathered: Gathered) -> ModsView {
    let manifest = gathered.manifest.as_ref();
    let GameInfo {
        loader,
        minecraft,
        minecraft_from,
        profile,
    } = game_of(plan, rows, &gathered);
    let pack = if plan.variant == "modrinth" {
        PackKind::ModrinthPack
    } else if rows.iter().any(|row| !row.from_hearth) {
        PackKind::ServerPack
    } else {
        PackKind::None
    };
    // Un fichier désactivé ou retiré garde son verdict mais ne compte ni pour les dépendances ni pour les doublons.
    let read: Vec<&Found> = gathered.files.iter().filter(|f| !f.unread).collect();
    let candidates: Vec<Candidate<'_>> = read
        .iter()
        .map(|found| Candidate {
            identities: found
                .read
                .as_ref()
                .map_or(&[][..], |cached| cached.identities.as_slice()),
            enabled: found.enabled && found.place == ModPlace::Active,
        })
        .collect();
    let verdicts = compat::judge_installed(&profile, &candidates);
    let chosen: Vec<Option<&JarIdentity>> = candidates
        .iter()
        .map(|candidate| match profile.loader {
            Some(server) => compat::choose(candidate.identities, server),
            None => candidate.identities.first(),
        })
        .collect();
    let mut mods = Vec::with_capacity(gathered.files.len());
    let mut judged = 0usize;
    for found in &gathered.files {
        let (identity_ref, verdict, needed_by) = if found.unread {
            (None, Verdict::Unverified, Vec::new())
        } else {
            let at = judged;
            judged += 1;
            (
                chosen[at],
                verdicts[at].clone(),
                needed_by(at, &candidates, &chosen),
            )
        };
        let origin = origin_view(found_origin(found, manifest, rows));
        mods.push(ModItem {
            file: found.name.clone(),
            rank: found.rank,
            place: found.place,
            enabled: found.enabled,
            origin,
            readable_name: found.name.is_some(),
            identity: identity_ref.map(|identity| ModIdentityView {
                id: identity.id.as_str().to_owned(),
                name: identity.name.clone(),
                version: identity.version.as_ref().map(|v| v.as_str().to_owned()),
                loader: identity.loader.variant_id().to_owned(),
                minecraft_declared: identity.minecraft_declared && identity.minecraft.is_some(),
                environment: environment_view(identity.environment),
            }),
            verdict: verdict_view(&verdict, needed_by),
        });
    }
    ModsView {
        applicable: Applicable::Mods,
        game: ModsGame {
            loader: loader.map(|l| l.variant_id().to_owned()),
            minecraft,
            minecraft_from,
            updates_every_start: plan.updates_every_start,
            pack,
        },
        mods,
        unread: gathered.unread,
        truncated: gathered.truncated,
    }
}

/// Les identifiants des mods ACTIFS qui demandent le mod `at` (par son identifiant ou ce qu'il fournit).
fn needed_by(
    at: usize,
    candidates: &[Candidate<'_>],
    chosen: &[Option<&JarIdentity>],
) -> Vec<String> {
    let Some(me) = chosen[at] else {
        return Vec::new();
    };
    if !candidates[at].enabled {
        return Vec::new();
    }
    let mut out: Vec<String> = Vec::new();
    for (j, other) in chosen.iter().enumerate() {
        let Some(other) = other else { continue };
        if j == at || !candidates[j].enabled {
            continue;
        }
        let wanted = other
            .depends
            .iter()
            .any(|dep| dep.id == me.id || me.provides.contains(&dep.id));
        if wanted && !out.iter().any(|id| id == other.id.as_str()) {
            out.push(other.id.as_str().to_owned());
        }
    }
    out.sort();
    out
}

/// L'origine d'un fichier (BR-MOD-033) : un nom illisible n'est jamais attribue.
fn found_origin(found: &Found, manifest: Option<&ModrinthManifest>, rows: &[OriginRow]) -> Origin {
    let Some(name) = found.name.as_deref() else {
        return Origin::Manual;
    };
    resolve_origin(
        &FileFacts {
            active_name: listing::active_name(name),
            size: found.size,
            crc32: found.read.as_ref().and_then(|c| c.crc32),
            sha256: found.read.as_ref().map(|c| c.sha256.as_str()),
        },
        manifest,
        rows,
    )
}

fn summary_of(identity: &JarIdentity) -> IdentitySummary {
    IdentitySummary {
        mod_id: identity.id.as_str().to_owned(),
        name: identity.name.clone(),
        version: identity.version.as_ref().map(|v| v.as_str().to_owned()),
        loader: identity.loader,
        environment: identity.environment,
    }
}

/// Le temps que la lecture des mods d'une sauvegarde peut prendre (HRT-293).
const SNAPSHOT_BUDGET: Duration = Duration::from_secs(60);

#[async_trait::async_trait]
impl ModsSnapshot for ModsService {
    async fn applicable(&self, key: &ServiceKey) -> bool {
        matches!(self.plan_of(key).await, Ok(Judged::Read(_)))
    }

    async fn read(&self, key: &ServiceKey) -> ModsReading {
        let plan = match self.plan_of(key).await {
            Ok(Judged::Read(plan)) => plan,
            Ok(Judged::Done(_)) | Err(ModsError::NotAGameService) => {
                return ModsReading::NotApplicable;
            }
            Err(_) => return ModsReading::Unreadable,
        };
        // Une lecture qui echoue ou depasse son temps ne fait JAMAIS echouer la sauvegarde.
        let within = if self.budget < READ_BUDGET {
            self.budget
        } else {
            SNAPSHOT_BUDGET
        };
        let read = tokio::time::timeout(
            SNAPSHOT_BUDGET + Duration::from_secs(5),
            self.read_volume_within(key, &plan, within),
        )
        .await;
        let Ok(Ok((rows, gathered))) = read else {
            return ModsReading::Unreadable;
        };
        let game = game_of(&plan, &rows, &gathered);
        let manifest = gathered.manifest.as_ref();
        let files = gathered
            .files
            .iter()
            .filter_map(|found| {
                let name = found.name.clone()?;
                let identity = found.read.as_ref().and_then(|cached| {
                    match game.loader {
                        Some(server) => compat::choose(&cached.identities, server),
                        None => cached.identities.first(),
                    }
                    .map(summary_of)
                });
                Some(SnapshotFile {
                    name,
                    retired: found.place == ModPlace::Retired,
                    size: found.size,
                    sha256: found.read.as_ref().map(|cached| cached.sha256.clone()),
                    identity,
                    origin: found_origin(found, manifest, &rows),
                    unread: found.unread,
                })
            })
            .collect();
        ModsReading::Read(ModsRead {
            dir: plan.dir.clone(),
            retired_dir: plan.retired_dir_name.clone(),
            data_target: plan.data_target.clone(),
            snapshot: Snapshot {
                loader: game.loader,
                minecraft: game.minecraft.clone(),
                files,
            },
        })
    }

    async fn current(&self, key: &ServiceKey) -> Option<CurrentMods> {
        let plan = match self.plan_of(key).await {
            Ok(Judged::Read(plan)) => plan,
            _ => return None,
        };
        let (rows, gathered) = self.read_volume(key, &plan).await.ok()?;
        let game = game_of(&plan, &rows, &gathered);
        let mods = gathered
            .files
            .iter()
            .filter(|found| !found.unread)
            .map(|found| {
                let place = if found.place == ModPlace::Retired {
                    Place::Retired
                } else if found.enabled {
                    Place::Active
                } else {
                    Place::Disabled
                };
                CurrentMod {
                    file_name: found.name.clone(),
                    place,
                    identity: found.read.as_ref().and_then(|cached| {
                        match game.loader {
                            Some(server) => compat::choose(&cached.identities, server),
                            None => cached.identities.first(),
                        }
                        .map(summary_of)
                    }),
                }
            })
            .collect();
        Some(CurrentMods {
            mods,
            unread: gathered.unread,
        })
    }
}
