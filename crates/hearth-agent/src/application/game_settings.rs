//! Les réglages d'un serveur de jeu : lire, analyser un texte collé, enregistrer (HRT-108, ADR-0051,
//! BR-FICHE-037, 042, 044, 046 à 049, 052, 053, BR-JEU-035 et BR-JEU-036).
//!
//! **Deux vérités, sans recouvrement.** Le fichier du jeu fait foi pour ses octets ; la table `game_settings` (ou, à
//! défaut, les réponses de l'installation) pour les valeurs que le compte VEUT. Un écart est montré, jamais corrigé
//! seul : l'enregistrement exige un choix par réglage en écart (`SETTINGS_FILE_CHANGED` sinon, rien d'écrit).
//!
//! **Rien d'inconnu ne se perd.** Le fichier est lu par le format du canal, retouché sur les seules valeurs
//! demandées, jamais réécrit : commentaires, lignes inconnues, lignes commentées gardent leurs octets. Un fichier
//! que le format ne sait pas découper est refusé entier (`SETTINGS_FILE_UNREADABLE`).
//!
//! **Le fichier n'est écrit que sur un changement demandé** (BR-FICHE-048) : un enregistrement qui ne change aucun
//! octet n'écrit rien. Aucun réglage n'est appliqué à chaud : l'effet est DÉDUIT du canal (`file` : redémarrage ;
//! `env` et `args` : recréation, refusée `RECREATE_NOT_AVAILABLE` tant que HRT-87 n'est pas livré). Cet agent
//! n'arrête ni ne redémarre rien tout seul : le client lance, s'il le veut, la séquence d'un redémarrage ordinaire
//! (`POST /services/{key}/restart`, HRT-64).
//!
//! **Secrets.** Une valeur de secret reçue est scellée (service, révision 0, champ `answer:<réglage>`) et ne figure
//! dans aucune vue, aucune trace, aucune erreur, aucune entrée du journal. Les valeurs d'un réglage inconnu dont le
//! nom ressemble à un secret ne sont jamais rendues. Le journal nomme les réglages changés qui ne sont pas des
//! secrets, jamais une valeur (BR-FICHE-053).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex as StdMutex, PoisonError};

use hearth_proto::api::game_settings::{
    AnalyzeRequest, ChangeAction, DriftItem, DriftResolution, FicheRef, FileState, ImportLine,
    ImportState, ImportView, MAX_CHANGES, MAX_IMPORT_BYTES, OtherSetting, SaveOrigin,
    SaveSettingsRequest, SaveSettingsResponse, SettingEffect, SettingValue, SettingsView,
    ValueState,
};
use ring::digest;
use thiserror::Error;
use time::OffsetDateTime;
use tokio::sync::Mutex;
use zeroize::Zeroizing;

use super::catalog::CatalogService;
use super::fiche_install::reason_code;
use super::ports::{
    Clock, FsError, GameSettingsRepo, NewSettings, Ownership, Perms, SafeFs, SettingsStoreError,
};
use super::secret_vault::{SecretVault, VaultError};
use super::service_ops::OpLocks;
use super::service_specs::{ServiceSpecs, SpecServiceError};
use crate::domain::audit::{Actor, AuditAction, ClientName, Outcome, Target};
use crate::domain::catalog::formats::{ConfigFormat, MAX_FILE_BYTES, format_for};
use crate::domain::catalog::loading::model::{Fiche, Setting, hex};
use crate::domain::catalog::loading::schema::fiche::{Channel, ChannelKind};
use crate::domain::catalog::loading::schema::settings::ChangeWhen;
use crate::domain::catalog::settings_io::{
    DriftChoice, ImportStatus, SaveFailure, SavePlan, analyze_import, entries_of, imported_value,
    plan_save,
};
use crate::domain::catalog::settings_state::{
    Change, Context, Effect, InstallContext, Phase, Reason, Refusal, ResolvedValue, SettingsState,
    Slot, VersionStatus, drift, effect_of, imposed_broken, looks_secret, resolve_install,
    validate_text, version_status,
};
use crate::domain::files::RelPath;
use crate::domain::sealed::{AnswerId, SealContext, SecretField};
use crate::domain::secret::Secret;
use crate::domain::services::deploy::{flatten_target, normalize_target};
use crate::domain::services::from_fiche::{
    generated_secret_id, produced_file_perms, state_of_answers,
};
use crate::domain::services::key::ServiceKey;
use crate::domain::services::spec::{SecretsAccess, SpecBody};

/// Le scellé d'un secret de réglage n'est pas numéroté comme la description (dont les révisions commencent à 1) :
/// il est lié au service et au réglage.
const SEAL_REV: u32 = 0;

#[derive(Debug, Error)]
pub enum SettingsError {
    /// Aucun serveur de jeu de cette clé (service inconnu, pas créé par Hearth, ou pas depuis une fiche proposée) :
    /// l'API répond `NOT_A_GAME_SERVICE`.
    #[error("aucun serveur de jeu de cette clé")]
    NotFound,
    /// Les réglages ont changé depuis leur lecture.
    #[error("les réglages ont changé depuis leur lecture")]
    Conflict {
        by: Option<String>,
        at: Option<OffsetDateTime>,
    },
    /// Le fichier n'est plus celui que le client a lu, ou des réglages en écart attendent un choix.
    #[error("le fichier de réglages a changé")]
    FileChanged {
        ids: Vec<String>,
        /// Qui a gagné la course, quand c'est elle (comme pour `Conflict`).
        by: Option<String>,
        at: Option<OffsetDateTime>,
    },
    #[error("le fichier de réglages n'est pas lisible")]
    FileUnreadable {
        reason: &'static str,
        line: Option<usize>,
    },
    /// Un changement est refusé : le réglage connu, la raison (code court), au plus UN caractère.
    #[error("réglage refusé")]
    Refused {
        id: Option<String>,
        reason: &'static str,
        character: Option<char>,
    },
    /// Le réglage se reçoit à la création du service : cet agent ne sait pas encore le recréer.
    #[error("recréer le service n'est pas encore possible")]
    RecreateNotAvailable { ids: Vec<String> },
    /// L'effet annoncé (et signé) par le client n'est pas celui que l'agent déduit.
    #[error("l'effet annoncé n'est pas celui des changements")]
    EffectMismatch,
    /// Une opération tient le service.
    #[error("une opération est en cours sur ce service")]
    Busy,
    #[error("les secrets des services sont illisibles")]
    SecretsUnreadable,
    /// Un champ du corps est hors des bornes ou incohérent.
    #[error("corps refusé : {0}")]
    Invalid(&'static str),
    #[error("stockage indisponible : {0}")]
    Storage(String),
}

impl SettingsError {
    fn file_changed(ids: Vec<String>) -> Self {
        Self::FileChanged {
            ids,
            by: None,
            at: None,
        }
    }
}

/// Les octets que le plan d'un canal veut écrire.
fn plan_bytes(plans: &[(usize, SavePlan)], index: usize) -> Option<&[u8]> {
    plans
        .iter()
        .find(|(i, _)| *i == index)
        .and_then(|(_, plan)| plan.file.as_ref())
        .map(|bytes| bytes.as_slice())
}

/// Ce qu'on sait d'un fichier pour en calculer l'empreinte.
enum FileView<'f> {
    Bytes(&'f [u8]),
    Missing,
    Unsafe,
}

fn view_of(read: &FileRead) -> FileView<'_> {
    match read {
        FileRead::Bytes(bytes) => FileView::Bytes(bytes),
        FileRead::Missing => FileView::Missing,
        FileRead::Unsafe => FileView::Unsafe,
    }
}

impl From<SpecServiceError> for SettingsError {
    fn from(error: SpecServiceError) -> Self {
        match error {
            SpecServiceError::SecretsUnreadable(_) | SpecServiceError::Tampered => {
                Self::SecretsUnreadable
            }
            other => Self::Storage(other.to_string()),
        }
    }
}

impl From<VaultError> for SettingsError {
    fn from(error: VaultError) -> Self {
        match error {
            VaultError::Key(_) => Self::SecretsUnreadable,
            other => Self::Storage(other.to_string()),
        }
    }
}

impl From<crate::application::ports::StoreError> for SettingsError {
    fn from(error: crate::application::ports::StoreError) -> Self {
        Self::Storage(error.to_string())
    }
}

/// Le verrou d'écriture de fichier d'un service : permis avec d'autres écritures, refusé sous une opération
/// exclusive (déploiement, restauration...), rendu à la chute.
struct SettingsLock {
    locks: Arc<OpLocks>,
    key: String,
}

impl SettingsLock {
    fn take(locks: &Arc<OpLocks>, key: &ServiceKey) -> Result<Self, ()> {
        locks
            .with(|table| table.acquire_shared_write(key.as_str()))
            .map_err(|_| ())?;
        Ok(Self {
            locks: Arc::clone(locks),
            key: key.as_str().to_owned(),
        })
    }
}

impl Drop for SettingsLock {
    fn drop(&mut self) {
        self.locks
            .with(|table| table.release_shared_write(&self.key));
    }
}

/// Un canal fichier de la fiche, avec le chemin relatif dans la racine du service et les droits du fichier.
struct FileChannel<'a> {
    channel: &'a Channel,
    rel: RelPath,
    perms: Perms,
    format: Box<dyn ConfigFormat>,
}

/// Ce que dit la lecture d'un fichier.
enum FileRead {
    /// Les octets d'un fichier de réglages peuvent porter un mot de passe : effacés à la libération.
    Bytes(Zeroizing<Vec<u8>>),
    Missing,
    /// Le fichier existe mais ne se lit pas ici (lien, trop gros, fichier spécial...).
    Unsafe,
}

/// Tout ce qu'il faut savoir d'un service pour lire ou enregistrer ses réglages.
struct Loaded<'a> {
    key: ServiceKey,
    name: String,
    fiche: &'a Fiche,
    body: SpecBody,
    variant: Option<String>,
    /// Numéro de la dernière écriture (0 : aucune ligne encore).
    rev: u32,
    state: SettingsState,
    fingerprint: Option<String>,
    stored: Option<crate::application::ports::StoredSettings>,
    /// Les secrets de la description, descellés UNE fois par chargement.
    access: SecretsAccess,
}

/// Le fichier d'un canal tel qu'il est lu.
struct ChannelFile {
    id: String,
    read: FileRead,
}

pub struct GameSettings {
    catalog: Arc<CatalogService>,
    specs: Arc<ServiceSpecs>,
    repo: Arc<dyn GameSettingsRepo>,
    fs: Arc<dyn SafeFs>,
    locks: Arc<OpLocks>,
    vault: Arc<SecretVault>,
    sink: Arc<dyn super::ports::AuditSink>,
    clock: Arc<dyn Clock>,
    /// Un verrou d'exclusion PAR SERVICE : le verrou d'écriture de fichier de l'agent est partagé (il laisse passer
    /// plusieurs écritures de fichiers à la fois), il ne suffit donc pas à garder deux enregistrements de réglages
    /// l'un derrière l'autre.
    saving: StdMutex<HashMap<String, Arc<Mutex<()>>>>,
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex(digest::digest(&digest::SHA256, bytes).as_ref())
}

fn effect_wire(effect: Effect) -> SettingEffect {
    match effect {
        Effect::None => SettingEffect::None,
        Effect::Restart => SettingEffect::Restart,
        Effect::Recreate => SettingEffect::Recreate,
    }
}

fn refused(refusal: &Refusal, fiche: &Fiche) -> SettingsError {
    SettingsError::Refused {
        // Un identifiant venu du client n'est rendu que s'il est celui d'un réglage de la fiche.
        id: fiche.setting(&refusal.id).map(|s| s.id.clone()),
        reason: reason_code(&refusal.reason),
        character: refusal.character,
    }
}

fn unreadable(failure: &crate::domain::catalog::formats::Unreadable) -> SettingsError {
    SettingsError::FileUnreadable {
        reason: failure.reason.as_str(),
        line: failure.line,
    }
}

/// Les canaux fichier de la fiche que Hearth sait retoucher, avec leur chemin et leurs droits.
fn file_channels(fiche: &Fiche) -> Vec<FileChannel<'_>> {
    let (uid, gid, mode) = produced_file_perms(fiche.file.owner.uid, fiche.file.owner.gid);
    let mut found = Vec::new();
    for channel in fiche
        .file
        .channel
        .iter()
        .filter(|c| c.kind == ChannelKind::File)
    {
        let (Some(folder_id), Some(path), Some(format)) =
            (&channel.folder, &channel.path, format_for(channel))
        else {
            continue;
        };
        let Some(folder) = fiche.file.folder.iter().find(|f| &f.id == folder_id) else {
            continue;
        };
        let Some(source) = normalize_target(&folder.target)
            .ok()
            .and_then(|target| flatten_target(&target))
        else {
            continue;
        };
        let Ok(rel) = RelPath::parse(&format!("{source}/{path}")) else {
            continue;
        };
        let Some(perms) = Ownership::new(uid, gid).and_then(|owner| Perms::new(owner, mode)) else {
            continue;
        };
        found.push(FileChannel {
            channel,
            rel,
            perms,
            format,
        });
    }
    found
}

/// Un état dont chaque réglage de la fiche a un emplacement (un réglage ajouté par une fiche plus récente est
/// « non écrit »).
fn complete(fiche: &Fiche, mut state: SettingsState) -> SettingsState {
    for setting in &fiche.settings {
        if state.slot(&setting.id).is_none() {
            state.set_slot(&setting.id, Slot::Unwritten);
        }
    }
    state
}

/// L'état de l'installation, relu de la description durable (le domaine remplit et relit les réponses au même endroit).
fn state_from_answers(fiche: &Fiche, body: &SpecBody) -> SettingsState {
    let answers = body
        .fiche
        .as_ref()
        .map_or(&[][..], |f| f.answers.as_slice());
    complete(fiche, state_of_answers(fiche, answers))
}

/// Les ports du serveur de ce service, par identifiant de port de la fiche.
fn host_ports(fiche: &Fiche, body: &SpecBody) -> BTreeMap<String, u16> {
    fiche
        .file
        .port
        .iter()
        .filter_map(|port| {
            body.ports
                .iter()
                .find(|p| p.container_port == port.container)
                .map(|p| (port.id.clone(), p.host_port))
        })
        .collect()
}

/// Les valeurs imposées de la fiche pour ce service (texte seulement : un secret imposé se compare à une valeur
/// scellée que l'état ne porte pas).
fn imposed_values(
    fiche: &Fiche,
    state: &SettingsState,
    body: &SpecBody,
    variant: Option<&str>,
) -> (
    Vec<crate::domain::catalog::settings_state::Resolved>,
    BTreeMap<String, String>,
) {
    // Seules les réponses dont une valeur imposée dérive sont passées : une autre valeur périmée de l'état ne doit
    // pas faire taire tous les réglages imposés.
    let wanted: BTreeSet<&str> = fiche
        .settings
        .iter()
        .filter_map(|s| s.imposed.as_ref())
        .filter_map(|i| i.derive.as_ref())
        .map(|d| d.from.as_str())
        .collect();
    let answers: BTreeMap<String, String> = wanted
        .into_iter()
        .filter_map(|id| state.value(id).map(|v| (id.to_owned(), v.to_owned())))
        .collect();
    let ports = host_ports(fiche, body);
    let generated: BTreeMap<String, Secret> = BTreeMap::new();
    let ctx = InstallContext {
        variant,
        world_imported: false,
        host_ports: &ports,
        generated: &generated,
    };
    let Ok(resolved) = resolve_install(fiche, &ctx, &answers) else {
        return (Vec::new(), BTreeMap::new());
    };
    let imposed = resolved
        .iter()
        .filter(|r| r.imposed)
        .filter_map(|r| match &r.value {
            Some(ResolvedValue::Plain(v)) => Some((r.id.clone(), v.clone())),
            _ => None,
        })
        .collect();
    (resolved, imposed)
}

impl GameSettings {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        catalog: Arc<CatalogService>,
        specs: Arc<ServiceSpecs>,
        repo: Arc<dyn GameSettingsRepo>,
        fs: Arc<dyn SafeFs>,
        locks: Arc<OpLocks>,
        sink: Arc<dyn super::ports::AuditSink>,
        clock: Arc<dyn Clock>,
    ) -> Arc<Self> {
        let vault = Arc::clone(specs.vault());
        Arc::new(Self {
            catalog,
            specs,
            repo,
            fs,
            locks,
            vault,
            sink,
            clock,
            saving: StdMutex::new(HashMap::new()),
        })
    }

    /// Le verrou d'exclusion de ce service.
    fn serial(&self, key: &ServiceKey) -> Arc<Mutex<()>> {
        let mut map = self.saving.lock().unwrap_or_else(PoisonError::into_inner);
        // Une entrée que plus personne ne tient (seule la table la garde) est retirée : la table ne grossit pas.
        map.retain(|_, lock| Arc::strong_count(lock) > 1);
        Arc::clone(map.entry(key.as_str().to_owned()).or_default())
    }

    // -------------------------------------------------------------------------------------------------
    // Lecture
    // -------------------------------------------------------------------------------------------------

    async fn load(&self, key: &ServiceKey) -> Result<Loaded<'_>, SettingsError> {
        let Some((revision, access)) = self.specs.read(key, None).await? else {
            return Err(SettingsError::NotFound);
        };
        let body = revision.body;
        let Some(spec_fiche) = &body.fiche else {
            return Err(SettingsError::NotFound);
        };
        let fiche = self
            .catalog
            .fiche(&spec_fiche.id)
            .ok_or(SettingsError::NotFound)?;
        let variant = spec_fiche.variant.clone();
        let stored = self.repo.load(key).await?;
        let (rev, state, fingerprint) = match &stored {
            Some(stored) => (
                stored.rev,
                complete(fiche, stored.state.clone()),
                stored.file_fingerprint.clone(),
            ),
            None => (0, state_from_answers(fiche, &body), None),
        };
        Ok(Loaded {
            key: key.clone(),
            name: body.name.clone(),
            fiche,
            body,
            variant,
            rev,
            state,
            fingerprint,
            stored,
            access,
        })
    }

    async fn read_file(&self, key: &ServiceKey, rel: &RelPath) -> Result<FileRead, SettingsError> {
        let fs = Arc::clone(&self.fs);
        let key = key.clone();
        let rel = rel.clone();
        let result = tokio::task::spawn_blocking(move || {
            let root = fs.open_service_root(&key)?;
            root.read_bounded(&rel, MAX_FILE_BYTES as u64)
        })
        .await
        .map_err(|error| SettingsError::Storage(error.to_string()))?;
        Ok(match result {
            Ok(bytes) => FileRead::Bytes(Zeroizing::new(bytes)),
            Err(FsError::NotFound) => FileRead::Missing,
            Err(_) => FileRead::Unsafe,
        })
    }

    async fn write_file(
        &self,
        key: &ServiceKey,
        rel: &RelPath,
        bytes: &[u8],
        perms: Perms,
    ) -> Result<(), SettingsError> {
        let fs = Arc::clone(&self.fs);
        let key = key.clone();
        let rel = rel.clone();
        let bytes = Zeroizing::new(bytes.to_vec());
        tokio::task::spawn_blocking(move || {
            let root = fs.open_service_root(&key)?;
            root.write_atomic(&rel, &bytes, perms)
        })
        .await
        .map_err(|error| SettingsError::Storage(error.to_string()))?
        .map_err(|error| SettingsError::Storage(format!("fichier non écrit : {error}")))
    }

    async fn read_files(
        &self,
        key: &ServiceKey,
        channels: &[FileChannel<'_>],
    ) -> Result<Vec<ChannelFile>, SettingsError> {
        let mut files = Vec::with_capacity(channels.len());
        for channel in channels {
            files.push(ChannelFile {
                id: channel.channel.id.clone(),
                read: self.read_file(key, &channel.rel).await?,
            });
        }
        Ok(files)
    }

    /// L'empreinte de l'ensemble des fichiers de réglages : un fichier absent et un fichier vide diffèrent.
    fn fingerprint<'f>(files: impl IntoIterator<Item = (&'f str, FileView<'f>)>) -> String {
        let mut all = Vec::new();
        for (id, view) in files {
            all.extend_from_slice(&u16::try_from(id.len()).unwrap_or(u16::MAX).to_be_bytes());
            all.extend_from_slice(id.as_bytes());
            match view {
                FileView::Bytes(bytes) => {
                    all.push(b'f');
                    all.extend_from_slice(sha256_hex(bytes).as_bytes());
                }
                FileView::Missing => all.push(b'm'),
                FileView::Unsafe => all.push(b'u'),
            }
        }
        sha256_hex(&all)
    }

    /// L'empreinte des fichiers tels qu'ils ont été lus.
    fn stamp_of(files: &[ChannelFile]) -> String {
        Self::fingerprint(files.iter().map(|f| (f.id.as_str(), view_of(&f.read))))
    }

    /// Les réglages d'un serveur de jeu : valeurs voulues, état du fichier, écarts, réglages inconnus (BR-FICHE-047,
    /// 048). Jamais la valeur d'un secret.
    pub async fn view(&self, key: &ServiceKey) -> Result<SettingsView, SettingsError> {
        let loaded = self.load(key).await?;
        let fiche = loaded.fiche;
        let channels = file_channels(fiche);
        let files = self.read_files(key, &channels).await?;
        let mut file_state = FileState::Ok;
        let mut drifts: Vec<DriftItem> = Vec::new();
        let mut others: Vec<OtherSetting> = Vec::new();
        let mut entries_by_channel: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        for (channel, file) in channels.iter().zip(&files) {
            let bytes = match &file.read {
                FileRead::Bytes(bytes) => bytes,
                FileRead::Missing => {
                    if file_state == FileState::Ok {
                        file_state = FileState::Missing;
                    }
                    continue;
                }
                FileRead::Unsafe => {
                    file_state = FileState::Unreadable;
                    continue;
                }
            };
            let Ok(entries) = entries_of(channel.format.as_ref(), bytes) else {
                file_state = FileState::Unreadable;
                continue;
            };
            let (found, extras) = drift(fiche, &loaded.state, &channel.channel.id, &entries);
            drifts.extend(
                found
                    .into_iter()
                    .filter(|d| Self::is_decidable(fiche, d))
                    .map(|d| DriftItem {
                        file_valid: match (&d.file, fiche.setting(&d.id)) {
                            (Some(value), Some(setting)) => {
                                validate_text(fiche, setting, value).is_ok()
                            }
                            _ => true,
                        },
                        id: d.id,
                        hearth: d.hearth,
                        file: d.file,
                    }),
            );
            for key in extras {
                let masked = looks_secret(&key);
                let value = (!masked)
                    .then(|| {
                        entries
                            .iter()
                            .rev()
                            .find(|(k, _)| *k == key)
                            .map(|(_, v)| v.clone())
                    })
                    .flatten();
                others.push(OtherSetting { key, value, masked });
            }
            entries_by_channel.insert(channel.channel.id.clone(), entries);
        }
        let (resolved, _imposed) = imposed_values(
            fiche,
            &loaded.state,
            &loaded.body,
            loaded.variant.as_deref(),
        );
        let broken = imposed_broken(fiche, &resolved, &entries_by_channel);
        let stamp = Self::stamp_of(&files);
        let file_changed = loaded
            .fingerprint
            .as_ref()
            .is_some_and(|written| *written != stamp);
        let settings = fiche
            .settings
            .iter()
            .map(|setting| self.value_of(fiche, &loaded.state, setting))
            .collect();
        Ok(SettingsView {
            rev: loaded.rev,
            fiche: FicheRef {
                id: fiche.id().to_owned(),
                version: fiche.file.version,
                digest: fiche.digest_hex(),
            },
            file_state,
            file_stamp: Some(stamp),
            file_changed,
            raw_edit: !channels.is_empty()
                && channels.iter().all(|c| c.channel.raw_edit.unwrap_or(false)),
            settings,
            drifts,
            others,
            imposed_broken: broken,
        })
    }

    fn is_decidable(fiche: &Fiche, d: &crate::domain::catalog::settings_state::Drift) -> bool {
        fiche.setting(&d.id).is_some_and(|s| s.imposed.is_none())
    }

    fn value_of(&self, fiche: &Fiche, state: &SettingsState, setting: &Setting) -> SettingValue {
        let slot = state.slot(&setting.id);
        let (value_state, value) = match slot {
            Some(Slot::Value(v)) => (ValueState::Written, Some(v.clone())),
            Some(Slot::Secret) => (ValueState::SecretSet, None),
            _ => (ValueState::Unwritten, None),
        };
        SettingValue {
            id: setting.id.clone(),
            state: value_state,
            value,
            frozen: setting.change == ChangeWhen::WorldCreation,
            imposed: setting.imposed.is_some(),
            // La version du jeu n'est pas lue (HRT-115) : `version_status` ne dit « disparu » que pour une version connue,
            // donc ce champ vaut faux tant que la lecture de la version n'est pas branchée.
            gone: version_status(setting, None) == VersionStatus::Gone,
            effect: effect_wire(effect_of(fiche, setting)),
        }
    }

    // -------------------------------------------------------------------------------------------------
    // Analyse d'un texte collé
    // -------------------------------------------------------------------------------------------------

    /// Lit un texte collé avec le format du fichier de réglages et le juge réglage par réglage. Rien n'est écrit,
    /// rien n'est gardé : le texte (un mot de passe possible) n'entre dans aucune trace (BR-FICHE-052).
    pub async fn analyze(
        &self,
        key: &ServiceKey,
        request: &AnalyzeRequest,
    ) -> Result<ImportView, SettingsError> {
        if request.text.len() > MAX_IMPORT_BYTES {
            return Err(SettingsError::Invalid("text"));
        }
        let loaded = self.load(key).await?;
        let channels = file_channels(loaded.fiche);
        let Some(channel) = channels.first() else {
            return Err(SettingsError::NotFound);
        };
        let (_resolved, imposed) = imposed_values(
            loaded.fiche,
            &loaded.state,
            &loaded.body,
            loaded.variant.as_deref(),
        );
        let ctx = Self::context(&loaded);
        let analysis = analyze_import(
            loaded.fiche,
            &ctx,
            &loaded.state,
            &channel.channel.id,
            channel.format.as_ref(),
            request.text.as_bytes(),
            &imposed,
        )
        .map_err(|failure| unreadable(&failure))?;
        Ok(ImportView {
            lines: analysis
                .lines
                .into_iter()
                .map(|line| {
                    let (state, reason) = match line.status {
                        ImportStatus::Take => (ImportState::Take, None),
                        ImportStatus::Same => (ImportState::Same, None),
                        ImportStatus::Rejected(reason) => {
                            (ImportState::Rejected, Some(reason_code(&reason).to_owned()))
                        }
                        ImportStatus::Imposed => (ImportState::Imposed, None),
                        ImportStatus::Frozen => (ImportState::Frozen, None),
                    };
                    ImportLine {
                        id: line.id,
                        state,
                        reason,
                        pasted: line.pasted,
                        secret: line.secret,
                    }
                })
                .collect(),
            unknown: analysis.unknown,
            duplicates: analysis.duplicates,
            imposed_undone: analysis.imposed_undone,
        })
    }

    fn context<'a>(loaded: &'a Loaded<'_>) -> Context<'a> {
        Context {
            phase: Phase::Installed,
            variant: loaded.variant.as_deref(),
            game_version: None,
            world_imported: false,
        }
    }

    // -------------------------------------------------------------------------------------------------
    // Enregistrement
    // -------------------------------------------------------------------------------------------------

    /// Enregistre des réglages (acte `GameSettingsSave` `0x1E`, déjà confirmé par la couche d'accès).
    ///
    /// Ordre : bornes du corps ; verrou ; révision de base (`SETTINGS_CONFLICT`) ; fichiers lus (illisible :
    /// refus entier) ; empreinte du fichier (`SETTINGS_FILE_CHANGED`) ; changements jugés, tous ou aucun
    /// (`SETTING_REFUSED`, `RECREATE_NOT_AVAILABLE`) ; écarts tranchés ; effet signé ; scellés ; fichiers écrits
    /// (repris en arrière si l'un échoue) ; ligne de la base ; journal.
    pub async fn save(
        &self,
        key: &ServiceKey,
        actor: &Actor,
        request: &SaveSettingsRequest,
    ) -> Result<SaveSettingsResponse, SettingsError> {
        if request.changes.len() + request.others.len() > MAX_CHANGES
            || request.drift_choices.len() > MAX_CHANGES
            || request
                .import_text
                .as_ref()
                .is_some_and(|t| t.len() > MAX_IMPORT_BYTES)
        {
            return Err(SettingsError::Invalid("size"));
        }
        let serial = self.serial(key);
        let _serial = serial.lock().await;
        let _lock = SettingsLock::take(&self.locks, key).map_err(|()| SettingsError::Busy)?;
        let loaded = self.load(key).await?;
        let fiche = loaded.fiche;
        if request.base_rev != loaded.rev {
            return Err(SettingsError::Conflict {
                by: loaded.stored.as_ref().map(|s| s.updated_by.clone()),
                at: loaded.stored.as_ref().map(|s| s.updated_at),
            });
        }
        let channels = file_channels(fiche);
        let files = self.read_files(key, &channels).await?;
        let stamp = Self::stamp_of(&files);

        // Les changements : chacun vers son canal. Un réglage reçu par variable ou argument se recrée.
        let mut per_channel: BTreeMap<String, Vec<(String, Change)>> = BTreeMap::new();
        let mut recreate: Vec<String> = Vec::new();
        for change in &request.changes {
            let Some(setting) = fiche.setting(&change.id) else {
                let reason = if fiche.retired_setting(&change.id).is_some() {
                    Reason::Retired
                } else {
                    Reason::UnknownId
                };
                return Err(SettingsError::Refused {
                    id: None,
                    reason: reason_code(&reason),
                    character: None,
                });
            };
            // Une remise aux valeurs proposées ne touche jamais un secret : un mot de passe ne revient pas à « non
            // écrit » par un geste en bloc (BR-FICHE-044). Les réglages figés sont refusés plus loin (`frozen`).
            if request.origin == SaveOrigin::Reset && setting.is_secret() {
                return Err(SettingsError::Refused {
                    id: Some(setting.id.clone()),
                    reason: "secret",
                    character: None,
                });
            }
            if effect_of(fiche, setting) == Effect::Recreate {
                recreate.push(setting.id.clone());
                continue;
            }
            let wanted = match change.action {
                ChangeAction::Set => Change::Set(
                    change
                        .value
                        .clone()
                        .ok_or(SettingsError::Invalid("value"))?,
                ),
                ChangeAction::Unset => Change::Unset,
                ChangeAction::Reset => Change::Reset,
                ChangeAction::FromImport => {
                    let text = request
                        .import_text
                        .as_ref()
                        .ok_or(SettingsError::Invalid("import_text"))?;
                    let channel = channels
                        .iter()
                        .find(|c| c.channel.id == setting.channel)
                        .ok_or(SettingsError::Invalid("channel"))?;
                    match imported_value(
                        fiche,
                        &setting.channel,
                        channel.format.as_ref(),
                        text.as_bytes(),
                        &setting.id,
                    )
                    .map_err(|failure| unreadable(&failure))?
                    {
                        Some(value) => Change::Set(value),
                        None => return Err(SettingsError::Invalid("import_text")),
                    }
                }
                ChangeAction::Unknown => return Err(SettingsError::Invalid("action")),
            };
            per_channel
                .entry(setting.channel.clone())
                .or_default()
                .push((setting.id.clone(), wanted));
        }
        if !recreate.is_empty() {
            return Err(SettingsError::RecreateNotAvailable { ids: recreate });
        }
        let choices: BTreeMap<String, DriftChoice> = request
            .drift_choices
            .iter()
            .filter_map(|c| match c.choice {
                DriftResolution::KeepFile => Some((c.id.clone(), DriftChoice::KeepFile)),
                DriftResolution::KeepMine => Some((c.id.clone(), DriftChoice::KeepMine)),
                DriftResolution::Unknown => None,
            })
            .collect();
        if choices.len() != request.drift_choices.len() {
            return Err(SettingsError::Invalid("drift_choices"));
        }
        let others: Vec<(String, Option<String>)> = request
            .others
            .iter()
            .map(|other| {
                if other.remove {
                    Ok((other.key.clone(), None))
                } else {
                    other
                        .value
                        .clone()
                        .map(|v| (other.key.clone(), Some(v)))
                        .ok_or(SettingsError::Invalid("others"))
                }
            })
            .collect::<Result<_, _>>()?;
        let ctx = Self::context(&loaded);

        // Le fichier a changé depuis la lecture du client : rien n'est écrit, il relit et tranche.
        let stamp_moved = request.file_stamp != stamp;

        // Un plan par canal fichier, l'état passant de l'un à l'autre. Rien n'est écrit avant que tous soient bons.
        let mut state = loaded.state.clone();
        let mut plans: Vec<(usize, SavePlan)> = Vec::new();
        let mut effect = Effect::None;
        let mut changed: Vec<String> = Vec::new();
        let mut adopted: Vec<String> = Vec::new();
        let mut secrets: Vec<(String, Secret)> = Vec::new();
        let mut unresolved: Vec<String> = Vec::new();
        for (index, (channel, file)) in channels.iter().zip(&files).enumerate() {
            let bytes = match &file.read {
                FileRead::Bytes(bytes) => Some(bytes.as_slice()),
                FileRead::Missing => None,
                FileRead::Unsafe => {
                    return Err(SettingsError::FileUnreadable {
                        reason: "unsafe",
                        line: None,
                    });
                }
            };
            let mine = per_channel.remove(&channel.channel.id).unwrap_or_default();
            // Les « autres réglages » vont au premier canal fichier (toutes les fiches de ce lot n'en ont qu'un).
            let channel_others: &[(String, Option<String>)] =
                if index == 0 { &others } else { &[] };
            match plan_save(
                fiche,
                &ctx,
                &state,
                &channel.channel.id,
                channel.format.as_ref(),
                bytes,
                &mine,
                &choices,
                channel_others,
            ) {
                Ok(plan) => {
                    state = plan.state.clone();
                    effect = effect.max(plan.effect);
                    changed.extend(plan.changed.iter().cloned());
                    adopted.extend(plan.adopted.iter().cloned());
                    for (id, secret) in &plan.secrets {
                        secrets.push((id.clone(), Secret::new(secret.expose().to_owned())));
                    }
                    plans.push((index, plan));
                }
                Err(SaveFailure::DriftUnresolved(ids)) => unresolved.extend(ids),
                Err(SaveFailure::Refused(refusal)) => return Err(refused(&refusal, fiche)),
                Err(SaveFailure::Unreadable(failure)) => return Err(unreadable(&failure)),
                Err(SaveFailure::Write(_)) => return Err(SettingsError::Invalid("value")),
                Err(SaveFailure::KnownKeyAsOther) => return Err(SettingsError::Invalid("others")),
                Err(SaveFailure::NoFileFormat) => return Err(SettingsError::NotFound),
            }
        }
        if !per_channel.is_empty() {
            return Err(SettingsError::Invalid("channel"));
        }
        if !unresolved.is_empty() || stamp_moved {
            return Err(SettingsError::file_changed(unresolved));
        }
        if request.effect != effect_wire(effect) {
            return Err(SettingsError::EffectMismatch);
        }
        let any_file = plans.iter().any(|(_, plan)| plan.file.is_some());
        if changed.is_empty() && adopted.is_empty() && !any_file {
            return Ok(SaveSettingsResponse {
                rev: loaded.rev,
                effect: SettingEffect::None,
                file_written: false,
                changed: 0,
                adopted: Vec::new(),
            });
        }

        // Les secrets reçus, scellés AVANT toute écriture : une clé illisible n'a rien laissé derrière elle.
        let mut sealed = Vec::with_capacity(secrets.len());
        if !secrets.is_empty() {
            let sealer = self.vault.sealer().await?;
            for (id, plain) in &secrets {
                let answer = AnswerId::parse(id).ok_or(SettingsError::Invalid("id"))?;
                let field = SecretField::Answer(answer);
                let blob = sealer
                    .seal(
                        &SealContext {
                            service: key.as_str(),
                            rev: SEAL_REV,
                            field: &field,
                        },
                        plain.expose().as_bytes(),
                    )
                    .map_err(|_| SettingsError::SecretsUnreadable)?;
                sealed.push((id.clone(), blob));
            }
        }

        // Les fichiers : chacun est écrit seulement s'il est ENCORE tel que le plan l'a lu, en un seul geste (lecture,
        // comparaison, écriture : ADR-0051 point 5). Un fichier réécrit entre-temps n'est jamais recouvert. Le geste
        // rend ce qu'il a remplacé, pour pouvoir le remettre si l'enregistrement perd ensuite en base.
        let mut written: Vec<(usize, Option<Zeroizing<Vec<u8>>>)> = Vec::new();
        for (index, plan) in &plans {
            let Some(bytes) = &plan.file else { continue };
            let channel = &channels[*index];
            match self
                .swap_file(key, channel, bytes, &files[*index].read)
                .await
            {
                Ok(previous) => written.push((*index, previous)),
                Err(error) => {
                    self.undo_writes(key, &channels, &plans, &written).await;
                    return Err(error);
                }
            }
        }
        let new_stamp = Self::fingerprint(files.iter().enumerate().map(|(i, f)| {
            (
                f.id.as_str(),
                plan_bytes(&plans, i).map_or_else(|| view_of(&f.read), FileView::Bytes),
            )
        }));

        let rev = match self
            .repo
            .save(NewSettings {
                key: key.clone(),
                expected_rev: (loaded.rev != 0).then_some(loaded.rev),
                state,
                file_fingerprint: Some(new_stamp),
                sealed,
                by: Self::account_of(actor),
                at: self.clock.now(),
            })
            .await
        {
            Ok(rev) => rev,
            // Une autre écriture a gagné la course en base. Si le fichier porte encore ce que CET enregistrement y a
            // écrit, le contenu qu'il a remplacé (celui du gagnant, ou à défaut l'ancien) y revient : fichier et base
            // restent d'accord. Le client relit, voit l'éventuel écart et tranche.
            Err(SettingsStoreError::Conflict { by, at, .. }) => {
                self.undo_writes(key, &channels, &plans, &written).await;
                return Err(SettingsError::FileChanged {
                    ids: Vec::new(),
                    by,
                    at,
                });
            }
            Err(SettingsStoreError::Store(error)) => return Err(error.into()),
        };
        self.record(&loaded, actor, &changed, &secrets, &request.others)
            .await;
        Ok(SaveSettingsResponse {
            rev,
            effect: effect_wire(effect),
            file_written: !written.is_empty(),
            changed: u32::try_from(changed.len() + request.others.len()).unwrap_or(u32::MAX),
            adopted,
        })
    }

    /// Écrit `bytes` dans le fichier d'un canal SI ce fichier est encore tel que le plan l'a lu, en un seul geste
    /// bloquant (lecture, comparaison, écriture atomique). Rend le contenu remplacé (`None` : le fichier n'existait
    /// pas). Un fichier qui a bougé : `SETTINGS_FILE_CHANGED`, rien n'est écrit.
    async fn swap_file(
        &self,
        key: &ServiceKey,
        channel: &FileChannel<'_>,
        bytes: &[u8],
        expected: &FileRead,
    ) -> Result<Option<Zeroizing<Vec<u8>>>, SettingsError> {
        let expected: Option<Zeroizing<Vec<u8>>> = match expected {
            FileRead::Bytes(old) => Some(Zeroizing::new(old.to_vec())),
            FileRead::Missing => None,
            FileRead::Unsafe => {
                return Err(SettingsError::FileUnreadable {
                    reason: "unsafe",
                    line: None,
                });
            }
        };
        let fs = Arc::clone(&self.fs);
        let key = key.clone();
        let rel = channel.rel.clone();
        let perms = channel.perms;
        let bytes = Zeroizing::new(bytes.to_vec());
        let swapped = tokio::task::spawn_blocking(move || {
            let root = fs.open_service_root(&key)?;
            let now = match root.read_bounded(&rel, MAX_FILE_BYTES as u64) {
                Ok(found) => Some(Zeroizing::new(found)),
                Err(FsError::NotFound) => None,
                Err(error) => return Err(error),
            };
            if now.as_deref() != expected.as_deref() {
                return Ok(None);
            }
            root.write_atomic(&rel, &bytes, perms)?;
            Ok(Some(now))
        })
        .await
        .map_err(|error| SettingsError::Storage(error.to_string()))?
        .map_err(|error| SettingsError::Storage(format!("fichier non écrit : {error}")))?;
        swapped.ok_or_else(|| SettingsError::file_changed(Vec::new()))
    }

    /// Remet, pour chaque canal écrit par cet enregistrement, le contenu qu'il a remplacé, mais seulement si le fichier
    /// porte encore exactement ce que cet enregistrement y a écrit : jamais par-dessus le travail d'un autre.
    async fn undo_writes(
        &self,
        key: &ServiceKey,
        channels: &[FileChannel<'_>],
        plans: &[(usize, SavePlan)],
        written: &[(usize, Option<Zeroizing<Vec<u8>>>)],
    ) {
        for (index, previous) in written {
            let Some(ours) = plan_bytes(plans, *index) else {
                continue;
            };
            let channel = &channels[*index];
            let fs = Arc::clone(&self.fs);
            let key = key.clone();
            let rel = channel.rel.clone();
            let perms = channel.perms;
            let ours = Zeroizing::new(ours.to_vec());
            let previous = previous.as_ref().map(|old| Zeroizing::new(old.to_vec()));
            let _ = tokio::task::spawn_blocking(move || {
                let root = fs.open_service_root(&key)?;
                if root
                    .read_bounded(&rel, MAX_FILE_BYTES as u64)
                    .ok()
                    .as_deref()
                    == Some(&*ours)
                {
                    match &previous {
                        Some(old) => root.write_atomic(&rel, old, perms)?,
                        None => root.remove_tree(&rel)?,
                    }
                }
                Ok::<(), FsError>(())
            })
            .await;
        }
    }

    fn account_of(actor: &Actor) -> String {
        actor
            .account
            .as_ref()
            .map_or_else(|| "agent".to_owned(), ToString::to_string)
    }

    /// L'entrée du journal d'activité : les noms techniques des réglages changés qui ne sont pas des secrets, et
    /// « mot de passe modifié » le cas échéant. Aucune valeur (BR-FICHE-053).
    async fn record(
        &self,
        loaded: &Loaded<'_>,
        actor: &Actor,
        changed: &[String],
        secrets: &[(String, Secret)],
        others: &[hearth_proto::api::game_settings::OtherChange],
    ) {
        // Un enregistrement va TOUJOURS au journal (BR-FICHE-053) : si le nom du service ne se nettoie pas en un nom
        // affichable, la clé du service (`h_...`, toujours affichable) le remplace.
        let service = ClientName::parse(&loaded.name)
            .or_else(|| ClientName::parse(loaded.key.as_str()))
            .or_else(|| ClientName::parse("service"));
        if let Some(service) = service {
            let names: Vec<ClientName> = changed
                .iter()
                .filter(|id| {
                    loaded
                        .fiche
                        .setting(id)
                        .is_some_and(|setting| !setting.is_secret())
                })
                .filter_map(|id| ClientName::parse(id))
                .collect();
            let target = Target::GameSettings {
                service,
                changed: names,
                count: u32::try_from(changed.len() + others.len()).unwrap_or(u32::MAX),
                password_changed: !secrets.is_empty(),
                raw_file: false,
            };
            self.sink
                .record(
                    actor.clone(),
                    AuditAction::ServiceGameSettings,
                    target,
                    Outcome::Succeeded,
                )
                .await;
        }
    }

    // -------------------------------------------------------------------------------------------------
    // Recréer un fichier disparu (quatrième moment d'écriture de BR-FICHE-048)
    // -------------------------------------------------------------------------------------------------

    /// Au démarrage lancé par Hearth, quand un fichier de réglages a disparu : le recrée depuis les valeurs voulues
    /// (secrets compris, descellés pour l'occasion) et rend les canaux recréés, pour que l'écran le dise (« Le fichier
    /// de réglages avait disparu : Hearth l'a recréé. »). Un fichier présent n'est jamais touché ; un fichier
    /// illisible non plus.
    pub async fn recreate_missing_files(
        &self,
        key: &ServiceKey,
        actor: &Actor,
    ) -> Result<Vec<String>, SettingsError> {
        let serial = self.serial(key);
        let _serial = serial.lock().await;
        let _lock = SettingsLock::take(&self.locks, key).map_err(|()| SettingsError::Busy)?;
        let loaded = self.load(key).await?;
        let fiche = loaded.fiche;
        let channels = file_channels(fiche);
        let files = self.read_files(key, &channels).await?;
        let missing: Vec<usize> = files
            .iter()
            .enumerate()
            .filter(|(_, f)| matches!(f.read, FileRead::Missing))
            .map(|(i, _)| i)
            .collect();
        if missing.is_empty() {
            return Ok(Vec::new());
        }
        let (secrets, generated) = self.spec_secrets(&loaded).await?;
        let mut answers: BTreeMap<String, String> = BTreeMap::new();
        for (id, slot) in loaded.state.iter() {
            match slot {
                Slot::Value(v) => {
                    answers.insert(id.to_owned(), v.clone());
                }
                Slot::Secret => {
                    if let Some(secret) = secrets.get(id) {
                        answers.insert(id.to_owned(), secret.expose().to_owned());
                    }
                }
                Slot::Unwritten => {}
            }
        }
        // Un réglage imposé se recalcule ; il n'est jamais une réponse.
        answers.retain(|id, _| fiche.setting(id).is_some_and(|s| s.imposed.is_none()));
        let ports = host_ports(fiche, &loaded.body);
        let resolved = resolve_install(
            fiche,
            &InstallContext {
                variant: loaded.variant.as_deref(),
                world_imported: false,
                host_ports: &ports,
                generated: &generated,
            },
            &answers,
        )
        .map_err(|refusal| refused(&refusal, fiche))?;
        let mut recreated = Vec::new();
        let mut after: Vec<ChannelFile> = files
            .into_iter()
            .map(|f| ChannelFile {
                id: f.id,
                read: f.read,
            })
            .collect();
        for index in missing {
            let channel = &channels[index];
            let pairs: Vec<(String, String)> = resolved
                .iter()
                .filter(|r| r.channel == channel.channel.id)
                // Ce que le compte veut, et ce que la fiche impose : un réglage « non écrit » reste non écrit (la
                // valeur proposée n'est pas une valeur voulue).
                .filter(|r| {
                    r.imposed
                        || matches!(
                            loaded.state.slot(&r.id),
                            Some(Slot::Value(_) | Slot::Secret)
                        )
                })
                .filter_map(|r| {
                    r.value
                        .as_ref()
                        .map(|v| (r.key.clone(), v.expose().to_owned()))
                })
                .collect();
            let bytes = channel
                .format
                .create(&pairs)
                .map_err(|_| SettingsError::Invalid("value"))?;
            let bytes = Zeroizing::new(bytes);
            self.write_file(key, &channel.rel, &bytes, channel.perms)
                .await?;
            after[index].read = FileRead::Bytes(bytes);
            recreated.push(channel.channel.id.clone());
        }
        let stamp = Self::stamp_of(&after);
        let sealed = loaded
            .stored
            .as_ref()
            .map(|s| s.sealed.clone())
            .unwrap_or_default();
        self.repo
            .save(NewSettings {
                key: key.clone(),
                expected_rev: (loaded.rev != 0).then_some(loaded.rev),
                state: loaded.state.clone(),
                file_fingerprint: Some(stamp),
                sealed,
                by: Self::account_of(actor),
                at: self.clock.now(),
            })
            .await
            .map_err(|error| match error {
                SettingsStoreError::Conflict { by, at, .. } => SettingsError::Conflict { by, at },
                SettingsStoreError::Store(error) => error.into(),
            })?;
        Ok(recreated)
    }

    /// Les secrets de la description et ceux changés depuis, descellés : les valeurs des réglages secrets (celles
    /// changées depuis l'installation priment sur les réponses de l'installation) et les secrets tirés par l'agent à
    /// l'installation (`gen-<id>`, pour les réglages imposés qui en dérivent). La description n'est descellée qu'UNE
    /// fois, au chargement (`Loaded::access`).
    async fn spec_secrets(
        &self,
        loaded: &Loaded<'_>,
    ) -> Result<(BTreeMap<String, Secret>, BTreeMap<String, Secret>), SettingsError> {
        let mut settings: BTreeMap<String, Secret> = BTreeMap::new();
        let mut generated: BTreeMap<String, Secret> = BTreeMap::new();
        match &loaded.access {
            SecretsAccess::Opened(secrets) => {
                for entry in &secrets.answers {
                    let value = Secret::new(entry.value.expose().to_owned());
                    match generated_secret_id(entry.id.as_str()) {
                        Some(id) => {
                            generated.insert(id.to_owned(), value);
                        }
                        None => {
                            settings.insert(entry.id.as_str().to_owned(), value);
                        }
                    }
                }
            }
            SecretsAccess::Unreadable(_) | SecretsAccess::Tampered => {
                return Err(SettingsError::SecretsUnreadable);
            }
        }
        if let Some(stored) = &loaded.stored
            && !stored.sealed.is_empty()
        {
            let sealer = self.vault.sealer().await?;
            for (id, blob) in &stored.sealed {
                let answer = AnswerId::parse(id).ok_or(SettingsError::SecretsUnreadable)?;
                let field = SecretField::Answer(answer);
                let plain = sealer
                    .open(
                        &SealContext {
                            service: loaded.key.as_str(),
                            rev: SEAL_REV,
                            field: &field,
                        },
                        blob,
                    )
                    .map_err(|_| SettingsError::SecretsUnreadable)?;
                let text = String::from_utf8(plain.expose().to_vec())
                    .map_err(|_| SettingsError::SecretsUnreadable)?;
                settings.insert(id.clone(), Secret::new(text));
            }
        }
        Ok((settings, generated))
    }
}
