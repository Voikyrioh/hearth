#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
//! Faux moteur de conteneurs : un script de conteneurs, des événements injectables, une sonde réglable.
//!
//! Sert à tous les tests de l'agent : le poste de développement n'a pas de moteur (le vrai n'existe
//! qu'en CI Linux). La même suite de contrat (`tests/support/engine_contract.rs`) passe contre ce
//! faux, contre un faux démon HTTP local et contre un vrai moteur, ce qui l'empêche de mentir.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use futures_util::stream;
use sha2::{Digest, Sha256};
use tokio::sync::{broadcast, watch};

use hearth_agent::application::ports::{
    ContainerEngine, EngineError, EngineEvent, EngineEvents, EngineInfo, EnvEntry, Listing,
    LogQuery, LogStream, PullStream, Unreadable,
};
use hearth_agent::domain::catalog::stop_plan::FicheLiteral;
use hearth_agent::domain::secret::Secret;
use hearth_agent::domain::services::image::image_name;
use hearth_agent::domain::services::logs::{LineAssembler, LogLine, LogSource};
use hearth_agent::domain::services::plan::ContainerPlan;
use hearth_agent::domain::services::pull::{LayerEvent, PullTracker};
use hearth_agent::domain::services::spec::SpecProtocol;
use hearth_agent::domain::services::{
    Announced, ApiVersion, EngineProbe, Hidden, Observation, ProbeOutcome, RestartPolicy, RunState,
};
use hearth_agent::domain::services::{MountInfo, MountType, PortBinding, PortProtocol, PortScope};

#[derive(Debug, Clone)]
enum Feed {
    Event(EngineEvent),
    /// Coupe le flux d'événements : le moteur est à revérifier.
    End,
}

struct State {
    probe: EngineProbe,
    containers: BTreeMap<String, Observation>,
    list_error: Option<EngineError>,
    /// Conteneurs dont l'inspection échoue (la liste les nomme, l'inspection répond par l'erreur).
    inspect_errors: BTreeMap<String, EngineError>,
    /// Variables d'environnement (nom, valeur) par conteneur, pour `env`.
    envs: BTreeMap<String, Vec<(String, String)>>,
    /// Combien de fois `env` a été appelée.
    env_reads: u64,
    /// Le flux d'événements se ferme aussitôt ouvert (un moteur dont le flux ne tient pas).
    events_broken: bool,
    /// Temps que `list` met à répondre (un moteur lent).
    list_delay: std::time::Duration,
    /// Les morceaux écrits par chaque conteneur dans ses journaux (HRT-66).
    logs: BTreeMap<String, Vec<RawLog>>,
    /// Lectures de journaux qui échouent (None : rétablit).
    log_errors: BTreeMap<String, EngineError>,
    /// Génération des flux de journaux de chaque conteneur : l'augmenter ferme ceux déjà ouverts (le
    /// moteur ferme le suivi quand le conteneur s'arrête, même s'il est reparti aussitôt).
    log_epochs: BTreeMap<String, u64>,
    /// La mise en scène des ordres d'écriture (HRT-64).
    script: WriteScript,
    /// Récupérer, créer, supprimer (HRT-68).
    deploy: DeployScript,
}

/// Ce que le registre du faux sait d'une image : des événements de couche rejoués, puis l'issue.
#[derive(Clone)]
pub struct PullScript {
    pub events: Vec<LayerEvent>,
    /// Temps entre deux événements (virtuel sous `tokio::time::pause`).
    pub delay: Duration,
    /// L'erreur de la fin ; `None` : l'image arrive.
    pub error: Option<EngineError>,
}

impl PullScript {
    /// Une image à deux couches qui arrive.
    pub fn two_layers() -> Self {
        let down = |layer: &str, current, total| LayerEvent::Downloading {
            layer: layer.into(),
            current,
            total,
        };
        Self {
            events: vec![
                down("a", 0, 100),
                down("a", 50, 100),
                down("b", 10, 300),
                LayerEvent::Done { layer: "a".into() },
                down("b", 300, 300),
                LayerEvent::Done { layer: "b".into() },
            ],
            delay: Duration::ZERO,
            error: None,
        }
    }

    /// Une récupération qui échoue d'emblée avec cette erreur.
    pub fn failing(error: EngineError) -> Self {
        Self {
            events: Vec::new(),
            delay: Duration::ZERO,
            error: Some(error),
        }
    }
}

/// Ce que le faux a reçu pour créer un conteneur, avec les secrets en clair (le faux est un décor de test).
#[derive(Debug, Clone)]
pub struct CreatedPlan {
    pub name: String,
    pub image: String,
    pub labels: BTreeMap<String, String>,
    pub env: Vec<(String, String)>,
    pub command: Option<Vec<String>>,
    pub ports: Vec<(u16, u16, SpecProtocol)>,
    pub binds: Vec<(String, String, bool)>,
    pub cpu_millis: u32,
    pub memory_bytes: u64,
    /// L'entrée standard du conteneur est ouverte (HRT-103).
    pub stdin_open: bool,
}

#[derive(Default)]
struct DeployScript {
    /// Images présentes sur le serveur.
    images: BTreeSet<String>,
    /// Ce que le registre rend pour une image absente ; pas d'entrée : « privée ou introuvable ».
    registry: BTreeMap<String, PullScript>,
    pulls: Vec<String>,
    /// Récupérations abandonnées avant leur fin (annulation).
    pulls_abandoned: Arc<AtomicUsize>,
    created: Vec<CreatedPlan>,
    removed: Vec<String>,
    create_error: Option<EngineError>,
    /// Le moteur crée le conteneur, puis met ce temps à répondre (la réponse peut ne jamais être lue).
    create_reply_delay: Duration,
    info: Option<EngineInfo>,
    info_error: Option<EngineError>,
    next_id: u64,
    /// Le programme du conteneur de ce NOM s'arrête aussitôt démarré, avec ce code (BR-DEPLOY-020).
    exit_on_start: BTreeMap<String, i64>,
}

/// Un ordre d'écriture reçu par le faux, dans l'ordre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    Start(String),
    Stop(String, Duration),
    Restart(String, Duration),
    /// Une ligne écrite sur l'entrée du conteneur (HRT-103).
    Stdin(String, String),
}

/// Ce que le faux fait des ordres `start`, `stop` et `restart` (HRT-64) : par défaut, tout réussit aussitôt.
#[derive(Default)]
struct WriteScript {
    calls: Vec<Call>,
    /// `start` échoue avec cette erreur (le conteneur reste dans son état).
    start_errors: BTreeMap<String, EngineError>,
    /// `stop` échoue avec cette erreur avant toute action.
    stop_errors: BTreeMap<String, EngineError>,
    /// Temps que met un arrêt propre (virtuel sous `tokio::time::pause`).
    stop_delays: BTreeMap<String, Duration>,
    /// Le programme ignore l'ordre d'arrêt : le moteur attend le délai de grâce puis le tue (code 137).
    stubborn: BTreeSet<String>,
    /// Le moteur ne répond jamais à l'ordre d'arrêt.
    hanging: BTreeSet<String>,
    /// Une règle du serveur relance le conteneur ce temps après chaque arrêt.
    relaunch: BTreeMap<String, Duration>,
    /// Les dates écrites par le faux (`None` : l'heure réelle).
    now: Option<time::OffsetDateTime>,
    /// `send_stdin_line` échoue avec cette erreur (HRT-103).
    stdin_errors: BTreeMap<String, EngineError>,
    /// L'entrée de ce conteneur n'a pas été ouverte à sa création : le moteur refuse l'attache.
    stdin_closed: BTreeSet<String>,
    /// Ce que le jeu fait d'une ligne reçue, par conteneur.
    on_stdin: BTreeMap<String, Vec<StdinReaction>>,
    /// Un ordre de sortie a été reçu et le programme est en train de sortir : l'arrêt demandé au moteur attend sa
    /// fin au lieu d'ajouter son propre délai.
    exiting: BTreeSet<String>,
    /// L'arrêt de ce conteneur a été demandé au moteur : sa règle de redémarrage ne le relance pas (« arrêté sur
    /// demande »), comme Docker.
    stop_requested: BTreeSet<String>,
}

/// Ce que le jeu fait quand il lit une ligne sur son entrée (HRT-103).
#[derive(Debug, Clone)]
pub struct StdinReaction {
    /// La ligne qui déclenche la réaction.
    pub line: String,
    /// Le jeu écrit cette ligne dans ses journaux (à l'heure `at_ms`).
    pub log: Option<(i64, String)>,
    /// Le programme sort ce temps après la ligne, avec ce code (virtuel sous `tokio::time::pause`).
    pub exit_after: Option<(Duration, i64)>,
}

/// Un morceau d'octets écrit par un conteneur sur l'une de ses sorties, à l'heure du moteur.
struct RawLog {
    source: LogSource,
    at_ms: i64,
    bytes: Vec<u8>,
}

pub struct FakeEngine {
    state: Arc<Mutex<State>>,
    feed: broadcast::Sender<Feed>,
    probes: AtomicU64,
    lists: AtomicU64,
    inspections: AtomicU64,
    /// Réveille les lectures de journaux en suivi : une ligne écrite, un conteneur qui change d'état.
    log_tick: watch::Sender<u64>,
    /// Combien de lectures de journaux ont été demandées, et combien sont encore ouvertes.
    log_reads: AtomicU64,
    log_open: Arc<AtomicUsize>,
    /// Combien de lignes le moteur a rendues aux lectures (le travail du moteur, pas du client).
    log_rendered: Arc<AtomicU64>,
    /// Les paramètres de chaque requête de journaux reçue, dans l'ordre.
    log_queries: Mutex<Vec<LogQuery>>,
}

/// La sonde d'un Docker récent qui répond.
pub fn ready_probe() -> EngineProbe {
    EngineProbe {
        outcome: ProbeOutcome::Answered(Announced {
            name: "Docker Engine - Community".into(),
            version: "24.0.7".into(),
            api_version: ApiVersion {
                major: 1,
                minor: 43,
            },
        }),
        installed: true,
    }
}

/// 2026-10-13 00:00:00 UTC plus `hours` heures : des dates fixes pour les conteneurs de test.
pub fn at(hours: i64) -> time::OffsetDateTime {
    time::OffsetDateTime::from_unix_timestamp(1_791_849_600 + hours * 3600)
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH)
}

/// Un conteneur plausible : identifiant de 64 caractères tiré du nom, image `busybox:1.36`.
pub fn container(name: &str, run_state: RunState) -> Observation {
    let id: String = Sha256::digest(name.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let started = matches!(
        run_state,
        RunState::Running | RunState::Paused | RunState::Restarting | RunState::Exited
    );
    Observation {
        id,
        name: name.to_owned(),
        created_at: Some(at(9)),
        run_state,
        exit_code: matches!(run_state, RunState::Exited).then_some(0),
        started_at: started.then_some(at(10)),
        finished_at: matches!(run_state, RunState::Exited).then_some(at(11)),
        image: image_name("busybox:1.36", "sha256:0123456789abcdef0123456789abcdef"),
        ports: vec![],
        ports_omitted: 0,
        host_network: false,
        mounts: vec![],
        mounts_omitted: 0,
        restart: RestartPolicy::None,
        auto_remove: false,
        compose_project: None,
        env_names: vec![],
        env_omitted: 0,
        command: None,
        stop_timeout_s: None,
        memory_limit_bytes: None,
        cpu_millis: None,
        tty: false,
        log_driver: Some("json-file".into()),
        hearth_key: None,
    }
}

impl FakeEngine {
    /// Un moteur prêt, sans conteneur.
    pub fn new() -> Self {
        let (feed, _) = broadcast::channel(256);
        Self {
            state: Arc::new(Mutex::new(State {
                probe: ready_probe(),
                containers: BTreeMap::new(),
                list_error: None,
                inspect_errors: BTreeMap::new(),
                envs: BTreeMap::new(),
                env_reads: 0,
                events_broken: false,
                list_delay: std::time::Duration::ZERO,
                logs: BTreeMap::new(),
                log_errors: BTreeMap::new(),
                log_epochs: BTreeMap::new(),
                script: WriteScript::default(),
                deploy: DeployScript::default(),
            })),
            feed,
            probes: AtomicU64::new(0),
            lists: AtomicU64::new(0),
            inspections: AtomicU64::new(0),
            log_tick: watch::channel(0).0,
            log_reads: AtomicU64::new(0),
            log_open: Arc::new(AtomicUsize::new(0)),
            log_rendered: Arc::new(AtomicU64::new(0)),
            log_queries: Mutex::new(Vec::new()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Règle ce que la prochaine sonde verra.
    pub fn set_probe(&self, probe: EngineProbe) {
        self.lock().probe = probe;
    }

    pub fn set_outcome(&self, outcome: ProbeOutcome, installed: bool) {
        self.set_probe(EngineProbe { outcome, installed });
    }

    /// Le moteur répond de nouveau comme un Docker récent.
    pub fn set_ready(&self) {
        self.set_probe(ready_probe());
    }

    /// Ajoute ou remplace un conteneur, sans événement.
    pub fn put_silently(&self, observation: Observation) {
        self.lock()
            .containers
            .insert(observation.id.clone(), observation);
        self.wake_logs();
    }

    /// Les identifiants des conteneurs que le moteur connaît maintenant (pour adopter en base, HRT-79).
    pub fn container_ids(&self) -> Vec<String> {
        self.lock().containers.keys().cloned().collect()
    }

    /// Ajoute ou remplace un conteneur et émet l'événement correspondant.
    pub fn put(&self, observation: Observation, action: &str) {
        let id = observation.id.clone();
        self.put_silently(observation);
        self.emit(&id, action);
    }

    /// Modifie un conteneur, sans événement.
    pub fn update(&self, id: &str, change: impl FnOnce(&mut Observation)) {
        if let Some(observation) = self.lock().containers.get_mut(id) {
            change(observation);
        }
        self.wake_logs();
    }

    /// Supprime un conteneur et émet `destroy`.
    pub fn remove(&self, id: &str) {
        self.lock().containers.remove(id);
        self.wake_logs();
        self.emit(id, "destroy");
    }

    pub fn emit(&self, container_id: &str, action: &str) {
        // Sans abonné, `send` rend une erreur : personne n'écoute, ce n'est pas un échec.
        let _ = self.feed.send(Feed::Event(EngineEvent {
            container_id: container_id.to_owned(),
            action: action.to_owned(),
        }));
    }

    /// Coupe les flux d'événements ouverts.
    pub fn break_events(&self) {
        let _ = self.feed.send(Feed::End);
    }

    /// Fait échouer `list` (None : rétablit).
    pub fn fail_list(&self, error: Option<EngineError>) {
        self.lock().list_error = error;
    }

    /// Fait échouer l'inspection d'un conteneur (None : rétablit). La liste le rend « illisible ».
    pub fn fail_inspect(&self, id: &str, error: Option<EngineError>) {
        let mut state = self.lock();
        match error {
            Some(error) => state.inspect_errors.insert(id.to_owned(), error),
            None => state.inspect_errors.remove(id),
        };
    }

    /// Règle les variables d'un conteneur : la LISTE n'en montre que les noms (dans l'observation, à
    /// poser avec `Observation::env_names`), `env` rend noms et valeurs.
    pub fn set_env(&self, id: &str, variables: &[(&str, &str)]) {
        let pairs = variables
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        self.lock().envs.insert(id.to_owned(), pairs);
    }

    /// Combien de fois les valeurs ont été lues (jamais par la liste ni par les événements).
    pub fn env_read_count(&self) -> u64 {
        self.lock().env_reads
    }

    /// Les flux d'événements ouverts à partir de maintenant se ferment aussitôt (true) ou tiennent.
    pub fn set_events_broken(&self, broken: bool) {
        self.lock().events_broken = broken;
    }

    /// `list` met ce temps à répondre (`Duration::ZERO` : rétablit).
    pub fn set_list_delay(&self, delay: std::time::Duration) {
        self.lock().list_delay = delay;
    }

    // -----------------------------------------------------------------------------------------
    // Mise en scène des ordres d'écriture (HRT-64)
    // -----------------------------------------------------------------------------------------

    /// Les ordres d'écriture reçus jusqu'ici, dans l'ordre.
    pub fn calls(&self) -> Vec<Call> {
        self.lock().script.calls.clone()
    }

    /// `start` de ce conteneur échoue (None : rétablit).
    pub fn fail_start(&self, id: &str, error: Option<EngineError>) {
        let mut state = self.lock();
        match error {
            Some(error) => state.script.start_errors.insert(id.to_owned(), error),
            None => state.script.start_errors.remove(id),
        };
    }

    /// `stop` de ce conteneur échoue (None : rétablit).
    pub fn fail_stop(&self, id: &str, error: Option<EngineError>) {
        let mut state = self.lock();
        match error {
            Some(error) => state.script.stop_errors.insert(id.to_owned(), error),
            None => state.script.stop_errors.remove(id),
        };
    }

    /// Un arrêt propre de ce conteneur met ce temps (`Duration::ZERO` : aussitôt).
    pub fn set_stop_delay(&self, id: &str, delay: Duration) {
        self.lock().script.stop_delays.insert(id.to_owned(), delay);
    }

    /// Ce programme ignore l'ordre d'arrêt : tué à l'échéance du délai de grâce, code 137.
    pub fn make_stubborn(&self, id: &str) {
        self.lock().script.stubborn.insert(id.to_owned());
    }

    /// Le moteur ne répond jamais à l'ordre d'arrêt de ce conteneur.
    pub fn make_hang_on_stop(&self, id: &str) {
        self.lock().script.hanging.insert(id.to_owned());
    }

    /// Une règle du serveur relance ce conteneur ce temps après chaque arrêt.
    pub fn relaunch_after_stop(&self, id: &str, after: Duration) {
        self.lock().script.relaunch.insert(id.to_owned(), after);
    }

    /// Les dates que le faux écrit (début d'exécution, fin) : cette heure, au lieu de l'heure réelle.
    pub fn set_now(&self, now: Option<time::OffsetDateTime>) {
        self.lock().script.now = now;
    }

    // -----------------------------------------------------------------------------------------
    // L'entrée standard des conteneurs (HRT-103)
    // -----------------------------------------------------------------------------------------

    /// Les lignes écrites sur l'entrée de ce conteneur, dans l'ordre.
    pub fn stdin_lines(&self, id: &str) -> Vec<String> {
        self.lock()
            .script
            .calls
            .iter()
            .filter_map(|call| match call {
                Call::Stdin(target, line) if target == id => Some(line.clone()),
                _ => None,
            })
            .collect()
    }

    /// `send_stdin_line` de ce conteneur échoue (None : rétablit).
    pub fn fail_stdin(&self, id: &str, error: Option<EngineError>) {
        let mut state = self.lock();
        match error {
            Some(error) => state.script.stdin_errors.insert(id.to_owned(), error),
            None => state.script.stdin_errors.remove(id),
        };
    }

    /// L'entrée de ce conteneur n'est pas ouverte (créé sans `stdin_open`) : le moteur refuse l'attache.
    pub fn close_stdin(&self, id: &str) {
        self.lock().script.stdin_closed.insert(id.to_owned());
    }

    /// Ce que le jeu de ce conteneur fait d'une ligne reçue.
    pub fn on_stdin(&self, id: &str, reaction: StdinReaction) {
        self.lock()
            .script
            .on_stdin
            .entry(id.to_owned())
            .or_default()
            .push(reaction);
    }

    // -----------------------------------------------------------------------------------------
    // Mise en scène de la récupération, de la création et de la suppression (HRT-68)
    // -----------------------------------------------------------------------------------------

    /// L'image est déjà sur le serveur.
    pub fn have_image(&self, image: &str) {
        self.lock().deploy.images.insert(image.to_owned());
    }

    /// L'image n'est pas sur le serveur.
    pub fn forget_image(&self, image: &str) {
        self.lock().deploy.images.remove(image);
    }

    /// Ce que le registre rend pour cette image quand elle est récupérée.
    pub fn publish_image(&self, image: &str, script: PullScript) {
        self.lock().deploy.registry.insert(image.to_owned(), script);
    }

    /// Les images dont la récupération a été demandée, dans l'ordre.
    pub fn pulls(&self) -> Vec<String> {
        self.lock().deploy.pulls.clone()
    }

    /// Combien de récupérations ont été abandonnées avant leur fin.
    pub fn pulls_abandoned(&self) -> usize {
        self.lock().deploy.pulls_abandoned.load(Ordering::SeqCst)
    }

    /// Ce que le faux a reçu pour créer des conteneurs, dans l'ordre.
    pub fn created(&self) -> Vec<CreatedPlan> {
        self.lock().deploy.created.clone()
    }

    /// Les identifiants supprimés, dans l'ordre.
    pub fn removed(&self) -> Vec<String> {
        self.lock().deploy.removed.clone()
    }

    /// `create` échoue avec cette erreur (None : rétablit).
    pub fn fail_create(&self, error: Option<EngineError>) {
        self.lock().deploy.create_error = error;
    }

    /// `create` crée le conteneur aussitôt mais ne répond qu'après ce délai.
    pub fn delay_create_reply(&self, delay: Duration) {
        self.lock().deploy.create_reply_delay = delay;
    }

    /// Les bornes de la machine que `info` rend.
    pub fn set_info(&self, info: Option<EngineInfo>) {
        self.lock().deploy.info = info;
    }

    /// Le programme du conteneur de ce nom s'arrête aussitôt démarré, avec ce code.
    pub fn exit_on_start(&self, name: &str, code: i64) {
        self.lock()
            .deploy
            .exit_on_start
            .insert(name.to_owned(), code);
    }

    pub fn fail_info(&self, error: Option<EngineError>) {
        self.lock().deploy.info_error = error;
    }

    fn script_now(state: &State) -> time::OffsetDateTime {
        state
            .script
            .now
            .unwrap_or_else(time::OffsetDateTime::now_utc)
    }

    /// Le conteneur tourne : exécution commencée maintenant.
    fn mark_running(state: &mut State, id: &str) {
        let now = Self::script_now(state);
        if let Some(container) = state.containers.get_mut(id) {
            container.run_state = RunState::Running;
            container.started_at = Some(now);
            container.finished_at = None;
            container.exit_code = None;
        }
    }

    /// Le conteneur est arrêté (ou supprimé s'il se supprime à l'arrêt) : fin d'exécution maintenant.
    fn mark_exited(state: &mut State, id: &str, code: i64) -> bool {
        let now = Self::script_now(state);
        let remove = state
            .containers
            .get(id)
            .is_some_and(|container| container.auto_remove);
        if remove {
            state.containers.remove(id);
        } else if let Some(container) = state.containers.get_mut(id) {
            container.run_state = RunState::Exited;
            container.exit_code = Some(code);
            container.finished_at = Some(now);
        }
        remove
    }

    /// Un arrêt, du début à la fin (délai propre ou de grâce, code, suppression, relance par le serveur).
    async fn do_stop(&self, id: &str, grace: Duration) -> Result<(), EngineError> {
        let (wait, code, hang) = {
            let state = self.lock();
            if let Some(error) = state
                .list_error
                .or_else(|| state.script.stop_errors.get(id).copied())
            {
                return Err(error);
            }
            let Some(container) = state.containers.get(id) else {
                return Err(EngineError::NotFound);
            };
            if !matches!(
                container.run_state,
                RunState::Running | RunState::Restarting | RunState::Paused
            ) {
                // Déjà arrêté : le moteur répond « non modifié ».
                return Ok(());
            }
            let stubborn = state.script.stubborn.contains(id);
            let delay = state
                .script
                .stop_delays
                .get(id)
                .copied()
                .unwrap_or_default();
            (
                if stubborn { grace } else { delay },
                if stubborn { 137 } else { 0 },
                state.script.hanging.contains(id),
            )
        };
        if hang {
            std::future::pending::<()>().await;
        }
        // Le programme sort déjà de lui-même (ordre de sortie reçu) : l'arrêt demandé attend cette fin, au plus le
        // délai de grâce, puis tue. La règle de redémarrage ne relance pas un conteneur dont l'arrêt est demandé.
        let exiting = {
            let mut state = self.lock();
            state.script.stop_requested.insert(id.to_owned());
            state.script.exiting.contains(id)
        };
        if exiting {
            let finished = tokio::time::timeout(grace, self.wait_not_running(id))
                .await
                .is_ok();
            if !finished {
                let removed = {
                    let mut state = self.lock();
                    Self::mark_exited(&mut state, id, 137)
                };
                self.wake_logs();
                self.emit(id, "die");
                self.emit(id, "stop");
                if removed {
                    self.emit(id, "destroy");
                }
            }
            self.lock().script.exiting.remove(id);
            return Ok(());
        }
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
        let (removed, relaunch) = {
            let mut state = self.lock();
            let removed = Self::mark_exited(&mut state, id, code);
            (removed, state.script.relaunch.get(id).copied())
        };
        self.wake_logs();
        self.emit(id, "die");
        self.emit(id, "stop");
        if removed {
            self.emit(id, "destroy");
        }
        if let Some(after) = relaunch.filter(|_| !removed) {
            let state = self.state.clone();
            let feed = self.feed.clone();
            let id = id.to_owned();
            tokio::spawn(async move {
                tokio::time::sleep(after).await;
                {
                    let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
                    FakeEngine::mark_running(&mut state, &id);
                }
                let _ = feed.send(Feed::Event(EngineEvent {
                    container_id: id,
                    action: "start".to_owned(),
                }));
            });
        }
        Ok(())
    }

    /// Attend que le conteneur ne tourne plus (ou n'existe plus).
    async fn wait_not_running(&self, id: &str) {
        loop {
            let running = self.lock().containers.get(id).is_some_and(|container| {
                matches!(
                    container.run_state,
                    RunState::Running | RunState::Restarting | RunState::Paused
                )
            });
            if !running {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    fn do_start(&self, id: &str) -> Result<(), EngineError> {
        {
            let mut state = self.lock();
            state.script.stop_requested.remove(id);
            if let Some(error) = state
                .list_error
                .or_else(|| state.script.start_errors.get(id).copied())
            {
                return Err(error);
            }
            let Some(container) = state.containers.get(id) else {
                return Err(EngineError::NotFound);
            };
            if container.run_state == RunState::Running {
                return Ok(());
            }
            // Un port publié que tient déjà un autre conteneur en marche : le moteur refuse le démarrage.
            let taken = container.ports.iter().any(|port| {
                state.containers.values().any(|other| {
                    other.id != id
                        && other.run_state == RunState::Running
                        && other.ports.iter().any(|held| {
                            held.host_port == port.host_port && held.protocol == port.protocol
                        })
                })
            });
            if taken {
                return Err(EngineError::PortInUse);
            }
            Self::mark_running(&mut state, id);
            let exits = state
                .containers
                .get(id)
                .and_then(|container| state.deploy.exit_on_start.get(&container.name).copied());
            if let Some(code) = exits {
                Self::mark_exited(&mut state, id, code);
            }
        }
        self.wake_logs();
        self.emit(id, "start");
        Ok(())
    }

    fn wake_logs(&self) {
        self.log_tick.send_modify(|tick| *tick += 1);
    }

    /// Le conteneur écrit une ligne (le texte et une fin de ligne) sur cette sortie, à cette heure.
    pub fn push_log(&self, id: &str, source: LogSource, at_ms: i64, text: &str) {
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(b'\n');
        self.push_log_raw(id, source, at_ms, &bytes);
    }

    /// Le conteneur écrit ces octets tels quels (sans fin de ligne ajoutée).
    pub fn push_log_raw(&self, id: &str, source: LogSource, at_ms: i64, bytes: &[u8]) {
        self.lock()
            .logs
            .entry(id.to_owned())
            .or_default()
            .push(RawLog {
                source,
                at_ms,
                bytes: bytes.to_vec(),
            });
        self.wake_logs();
    }

    /// Le moteur fait tourner ses fichiers : seuls les `keep` derniers morceaux restent.
    pub fn rotate_logs(&self, id: &str, keep: usize) {
        if let Some(entries) = self.lock().logs.get_mut(id) {
            let drop = entries.len().saturating_sub(keep);
            entries.drain(..drop);
        }
        self.wake_logs();
    }

    /// Fait échouer la lecture des journaux de ce conteneur (None : rétablit).
    pub fn fail_logs(&self, id: &str, error: Option<EngineError>) {
        let mut state = self.lock();
        match error {
            Some(error) => state.log_errors.insert(id.to_owned(), error),
            None => state.log_errors.remove(id),
        };
    }

    /// Le moteur ferme les suivis de journaux déjà ouverts sur ce conteneur (il s'est arrêté), sans que
    /// l'état publié change : un conteneur qui repart aussitôt garde ses lecteurs fermés derrière lui.
    pub fn end_log_streams(&self, id: &str) {
        *self.lock().log_epochs.entry(id.to_owned()).or_default() += 1;
        self.wake_logs();
    }

    /// Combien de lignes le moteur a rendues, en tout, aux lectures de journaux.
    /// Les requêtes de journaux reçues jusqu'ici, avec leurs paramètres.
    pub fn log_queries(&self) -> Vec<LogQuery> {
        self.log_queries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn rendered_log_lines(&self) -> u64 {
        self.log_rendered.load(Ordering::SeqCst)
    }

    /// Combien de lectures de journaux ont été demandées au moteur.
    pub fn log_read_count(&self) -> u64 {
        self.log_reads.load(Ordering::SeqCst)
    }

    /// Combien de lectures de journaux sont ouvertes en ce moment (un flux abandonné se ferme).
    pub fn open_log_reads(&self) -> usize {
        self.log_open.load(Ordering::SeqCst)
    }

    pub fn probe_count(&self) -> u64 {
        self.probes.load(Ordering::SeqCst)
    }

    pub fn list_count(&self) -> u64 {
        self.lists.load(Ordering::SeqCst)
    }

    pub fn inspect_count(&self) -> u64 {
        self.inspections.load(Ordering::SeqCst)
    }

    /// Le nombre d'abonnés actuels au flux d'événements.
    pub fn event_subscribers(&self) -> usize {
        self.feed.receiver_count()
    }
}

impl Default for FakeEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ContainerEngine for FakeEngine {
    async fn probe(&self) -> EngineProbe {
        self.probes.fetch_add(1, Ordering::SeqCst);
        self.lock().probe.clone()
    }

    async fn list(&self) -> Result<Listing, EngineError> {
        self.lists.fetch_add(1, Ordering::SeqCst);
        let delay = self.lock().list_delay;
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        let state = self.lock();
        if let Some(error) = state.list_error {
            return Err(error);
        }
        let mut listing = Listing::default();
        for (id, observation) in &state.containers {
            match state.inspect_errors.get(id) {
                Some(error) if error.engine_gone() => return Err(*error),
                Some(_) => listing.unreadable.push(Unreadable {
                    id: id.clone(),
                    name: observation.name.clone(),
                }),
                None => listing.observations.push(observation.clone()),
            }
        }
        Ok(listing)
    }

    async fn inspect(&self, id: &str) -> Result<Option<Observation>, EngineError> {
        self.inspections.fetch_add(1, Ordering::SeqCst);
        let state = self.lock();
        if let Some(error) = state
            .list_error
            .or_else(|| state.inspect_errors.get(id).copied())
        {
            return Err(error);
        }
        Ok(state.containers.get(id).cloned())
    }

    async fn env(&self, id: &str) -> Result<Option<Vec<EnvEntry>>, EngineError> {
        let mut state = self.lock();
        state.env_reads += 1;
        if let Some(error) = state
            .list_error
            .or_else(|| state.inspect_errors.get(id).copied())
        {
            return Err(error);
        }
        if !state.containers.contains_key(id) {
            return Ok(None);
        }
        Ok(Some(
            state
                .envs
                .get(id)
                .into_iter()
                .flatten()
                .map(|(name, value)| EnvEntry {
                    name: Hidden::new(name.clone()),
                    value: Secret::from(value.as_str()),
                })
                .collect(),
        ))
    }

    fn logs(&self, id: &str, query: LogQuery) -> LogStream {
        self.log_reads.fetch_add(1, Ordering::SeqCst);
        self.log_queries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(query);
        self.log_open.fetch_add(1, Ordering::SeqCst);
        let follower = Follower {
            state: self.state.clone(),
            tick: self.log_tick.subscribe(),
            id: id.to_owned(),
            query,
            epoch: self.lock().log_epochs.get(id).copied().unwrap_or_default(),
            rendered: self.log_rendered.clone(),
            started: false,
            finished: false,
            index: 0,
            assembler: LineAssembler::new(),
            ready: VecDeque::new(),
            _open: OpenGuard(self.log_open.clone()),
        };
        stream::unfold(follower, Follower::step).boxed()
    }

    async fn start(&self, id: &str) -> Result<(), EngineError> {
        self.lock().script.calls.push(Call::Start(id.to_owned()));
        self.do_start(id)
    }

    async fn stop(&self, id: &str, grace: Duration) -> Result<(), EngineError> {
        self.lock()
            .script
            .calls
            .push(Call::Stop(id.to_owned(), grace));
        self.do_stop(id, grace).await
    }

    async fn restart(&self, id: &str, grace: Duration) -> Result<(), EngineError> {
        self.lock()
            .script
            .calls
            .push(Call::Restart(id.to_owned(), grace));
        self.do_stop(id, grace).await?;
        let finished = self
            .lock()
            .containers
            .get(id)
            .and_then(|container| container.finished_at);
        self.do_start(id)?;
        // Comme le vrai moteur : le démarrage remet le code à zéro mais garde la fin de l'exécution arrêtée.
        if let Some(container) = self.lock().containers.get_mut(id) {
            container.finished_at = finished;
        }
        Ok(())
    }

    async fn send_stdin_line(&self, id: &str, line: &FicheLiteral) -> Result<(), EngineError> {
        let reaction = {
            let mut state = self.lock();
            if let Some(error) = state
                .list_error
                .or_else(|| state.script.stdin_errors.get(id).copied())
            {
                return Err(error);
            }
            let Some(container) = state.containers.get(id) else {
                return Err(EngineError::NotFound);
            };
            if container.run_state != RunState::Running || state.script.stdin_closed.contains(id) {
                return Err(EngineError::Refused);
            }
            state
                .script
                .calls
                .push(Call::Stdin(id.to_owned(), line.as_str().to_owned()));
            let reaction = state
                .script
                .on_stdin
                .get(id)
                .and_then(|all| all.iter().find(|r| r.line == line.as_str()))
                .cloned();
            if reaction
                .as_ref()
                .is_some_and(|reaction| reaction.exit_after.is_some())
            {
                state.script.exiting.insert(id.to_owned());
            }
            reaction
        };
        if let Some(reaction) = reaction {
            if let Some((at_ms, text)) = reaction.log {
                self.push_log(id, LogSource::Out, at_ms, &text);
            }
            if let Some((after, code)) = reaction.exit_after {
                let state = self.state.clone();
                let feed = self.feed.clone();
                let tick = self.log_tick.clone();
                let id = id.to_owned();
                tokio::spawn(async move {
                    tokio::time::sleep(after).await;
                    let (removed, relaunch, requested) = {
                        let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
                        let removed = FakeEngine::mark_exited(&mut state, &id, code);
                        (
                            removed,
                            state.script.relaunch.get(&id).copied(),
                            state.script.stop_requested.contains(&id),
                        )
                    };
                    tick.send_modify(|tick| *tick += 1);
                    let event = |action: &str| {
                        let _ = feed.send(Feed::Event(EngineEvent {
                            container_id: id.clone(),
                            action: action.to_owned(),
                        }));
                    };
                    event("die");
                    if removed {
                        event("destroy");
                    }
                    // Comme Docker : la règle de redémarrage relance un programme qui sort de lui-même, sauf si
                    // l'arrêt a été demandé au moteur.
                    if let Some(again) = relaunch.filter(|_| !removed && !requested) {
                        tokio::time::sleep(again).await;
                        {
                            let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
                            FakeEngine::mark_running(&mut state, &id);
                            state.script.exiting.remove(&id);
                        }
                        event("start");
                    }
                });
            }
        }
        Ok(())
    }

    async fn image_present(&self, image: &str) -> Result<bool, EngineError> {
        let state = self.lock();
        if let Some(error) = state.list_error {
            return Err(error);
        }
        Ok(state.deploy.images.contains(image))
    }

    fn pull(&self, image: &str) -> PullStream {
        let (script, abandoned, list_error) = {
            let mut state = self.lock();
            state.deploy.pulls.push(image.to_owned());
            (
                state.deploy.registry.get(image).cloned(),
                state.deploy.pulls_abandoned.clone(),
                state.list_error,
            )
        };
        let state = self.state.clone();
        let image = image.to_owned();
        if let Some(error) = list_error {
            return stream::once(async move { Err(error) }).boxed();
        }
        let Some(script) = script else {
            // Le registre ne connaît pas cette image : privée ou introuvable, il ne distingue pas.
            return stream::once(async { Err(EngineError::ImagePrivateOrMissing) }).boxed();
        };
        struct Run {
            script: PullScript,
            next: usize,
            tracker: PullTracker,
            state: Arc<Mutex<State>>,
            image: String,
            abandoned: Arc<AtomicUsize>,
            finished: bool,
        }
        impl Drop for Run {
            fn drop(&mut self) {
                if !self.finished {
                    self.abandoned.fetch_add(1, Ordering::SeqCst);
                }
            }
        }
        let run = Run {
            script,
            next: 0,
            tracker: PullTracker::new(),
            state,
            image,
            abandoned,
            finished: false,
        };
        stream::unfold(run, |mut run| async move {
            if run.finished {
                return None;
            }
            if run.next < run.script.events.len() {
                if !run.script.delay.is_zero() {
                    tokio::time::sleep(run.script.delay).await;
                }
                let event = run.script.events[run.next].clone();
                run.next += 1;
                let progress = run.tracker.apply(event);
                return Some((Ok(progress), run));
            }
            run.finished = true;
            if let Some(error) = run.script.error {
                return Some((Err(error), run));
            }
            run.state
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .deploy
                .images
                .insert(run.image.clone());
            None
        })
        .boxed()
    }

    async fn create(&self, plan: &ContainerPlan) -> Result<String, EngineError> {
        let (id, reply_delay) = {
            let mut state = self.lock();
            if let Some(error) = state.list_error.or(state.deploy.create_error) {
                return Err(error);
            }
            if state
                .containers
                .values()
                .any(|container| container.name == plan.name)
            {
                return Err(EngineError::NameTaken);
            }
            if !state.deploy.images.contains(&plan.image) {
                return Err(EngineError::NotFound);
            }
            state.deploy.next_id += 1;
            let id: String = Sha256::digest(
                format!("created-{}-{}", plan.name, state.deploy.next_id).as_bytes(),
            )
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
            let mut observation = container(&plan.name, RunState::Created);
            observation.id = id.clone();
            observation.image = image_name(&plan.image, "sha256:0123456789abcdef0123456789abcdef");
            observation.ports = plan
                .ports
                .iter()
                .map(|port| PortBinding {
                    host_port: port.host_port,
                    container_port: port.container_port,
                    protocol: match port.protocol {
                        SpecProtocol::Tcp => PortProtocol::Tcp,
                        SpecProtocol::Udp => PortProtocol::Udp,
                        SpecProtocol::Sctp => PortProtocol::Sctp,
                    },
                    scope: PortScope::All,
                })
                .collect();
            observation.mounts = plan
                .binds
                .iter()
                .map(|bind| MountInfo {
                    kind: MountType::Folder,
                    source: bind.host_path.clone(),
                    target: bind.target.clone(),
                    read_only: bind.read_only,
                })
                .collect();
            observation.restart = RestartPolicy::UnlessStopped;
            observation.env_names = plan
                .env
                .iter()
                .map(|(name, _)| Hidden::new(name.expose()))
                .collect();
            observation.command = plan.command.as_ref().map(|command| {
                command
                    .iter()
                    .map(|arg| Hidden::new(arg.expose()))
                    .collect()
            });
            observation.memory_limit_bytes = Some(plan.memory_bytes);
            observation.cpu_millis = Some(plan.cpu_millis);
            observation.hearth_key = plan
                .labels
                .get("io.hearth.service-id")
                .and_then(|ulid| hearth_agent::domain::services::ServiceKey::for_hearth(ulid));
            let pairs: Vec<(String, String)> = plan
                .env
                .iter()
                .map(|(name, value)| (name.expose().to_owned(), value.expose().to_owned()))
                .collect();
            state.envs.insert(id.clone(), pairs.clone());
            if !plan.stdin_open {
                state.script.stdin_closed.insert(id.clone());
            }
            state.deploy.created.push(CreatedPlan {
                stdin_open: plan.stdin_open,
                name: plan.name.clone(),
                image: plan.image.clone(),
                labels: plan.labels.clone(),
                env: pairs,
                command: plan
                    .command
                    .as_ref()
                    .map(|command| command.iter().map(|a| a.expose().to_owned()).collect()),
                ports: plan
                    .ports
                    .iter()
                    .map(|p| (p.host_port, p.container_port, p.protocol))
                    .collect(),
                binds: plan
                    .binds
                    .iter()
                    .map(|b| (b.host_path.clone(), b.target.clone(), b.read_only))
                    .collect(),
                cpu_millis: plan.cpu_millis,
                memory_bytes: plan.memory_bytes,
            });
            state.containers.insert(id.clone(), observation);
            (id, state.deploy.create_reply_delay)
        };
        self.wake_logs();
        self.emit(&id, "create");
        if !reply_delay.is_zero() {
            tokio::time::sleep(reply_delay).await;
        }
        Ok(id)
    }

    async fn remove(&self, id: &str) -> Result<(), EngineError> {
        let existed = {
            let mut state = self.lock();
            if let Some(error) = state.list_error {
                return Err(error);
            }
            state.deploy.removed.push(id.to_owned());
            state.containers.remove(id).is_some()
        };
        if existed {
            self.wake_logs();
            self.emit(id, "destroy");
        }
        Ok(())
    }

    async fn info(&self) -> Result<EngineInfo, EngineError> {
        let state = self.lock();
        if let Some(error) = state.list_error.or(state.deploy.info_error) {
            return Err(error);
        }
        Ok(state.deploy.info.clone().unwrap_or(EngineInfo {
            cpus: 8,
            memory_bytes: 16 * 1024 * 1024 * 1024,
            storage_dir: Some("/var/lib/docker".into()),
        }))
    }

    fn events(&self) -> EngineEvents {
        if self.lock().events_broken {
            return stream::empty().boxed();
        }
        let receiver = self.feed.subscribe();
        stream::unfold(receiver, |mut receiver| async move {
            loop {
                match receiver.recv().await {
                    Ok(Feed::Event(event)) => return Some((Ok(event), receiver)),
                    Ok(Feed::End) | Err(broadcast::error::RecvError::Closed) => return None,
                    // Un lecteur lent a perdu des événements : le vrai moteur ne le dit pas, le
                    // registre se resynchronise au besoin.
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
        })
        .boxed()
    }
}

/// Une lecture de journaux ouverte : se ferme (et se décompte) quand le flux est abandonné.
struct OpenGuard(Arc<AtomicUsize>);

impl Drop for OpenGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// L'état d'une lecture de journaux du faux : mêmes règles que le vrai moteur (les morceaux sont
/// assemblés par le même `LineAssembler`, `since` filtre à la seconde, `tail` garde les dernières
/// lignes, le suivi finit quand le conteneur s'arrête).
struct Follower {
    state: Arc<Mutex<State>>,
    tick: watch::Receiver<u64>,
    id: String,
    query: LogQuery,
    /// La génération des flux de ce conteneur quand cette lecture s'est ouverte.
    epoch: u64,
    rendered: Arc<AtomicU64>,
    started: bool,
    finished: bool,
    /// Combien de morceaux du conteneur ont déjà été lus.
    index: usize,
    assembler: LineAssembler,
    ready: VecDeque<LogLine>,
    _open: OpenGuard,
}

impl Follower {
    async fn step(mut self) -> Option<(Result<LogLine, EngineError>, Self)> {
        loop {
            if let Some(line) = self.ready.pop_front() {
                return Some((Ok(line), self));
            }
            if self.finished {
                return None;
            }
            if let Err(error) = self.advance() {
                self.finished = true;
                return Some((Err(error), self));
            }
            if self.ready.is_empty() && !self.finished && self.tick.changed().await.is_err() {
                self.finished = true;
            }
        }
    }

    /// Un pas, sous le verrou, sans attendre.
    fn advance(&mut self) -> Result<(), EngineError> {
        let state = Arc::clone(&self.state);
        let state = state.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(container) = state.containers.get(&self.id) else {
            return Err(EngineError::NotFound);
        };
        if let Some(error) = state.log_errors.get(&self.id) {
            return Err(*error);
        }
        if container.log_driver.as_deref() == Some("none") {
            return Err(EngineError::Unsupported);
        }
        let alive = matches!(
            container.run_state,
            hearth_agent::domain::services::RunState::Running
                | hearth_agent::domain::services::RunState::Paused
                | hearth_agent::domain::services::RunState::Restarting
        );
        let entries = state
            .logs
            .get(&self.id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let mut lines = Vec::new();
        if !self.started {
            self.started = true;
            let since = self.query.since_ms.map(|ms| ms.div_euclid(1000) * 1000);
            let mut chosen: Vec<&RawLog> = entries
                .iter()
                .filter(|entry| since.is_none_or(|since| entry.at_ms >= since))
                .collect();
            // Comme le vrai moteur : `tail` compte des MESSAGES du moteur (une écriture), pas des lignes
            // (un message plein de retours chariot donne plusieurs lignes).
            if let Some(tail) = self.query.tail {
                let keep = usize::try_from(tail).unwrap_or(usize::MAX);
                let skip = chosen.len().saturating_sub(keep);
                chosen.drain(..skip);
            }
            for entry in chosen {
                self.assembler
                    .push(entry.source, entry.at_ms, &entry.bytes, &mut lines);
            }
            if !self.query.follow {
                self.assembler.finish(&mut lines);
                self.finished = true;
            }
        } else {
            for entry in entries.iter().skip(self.index) {
                self.assembler
                    .push(entry.source, entry.at_ms, &entry.bytes, &mut lines);
            }
            let cut = state.log_epochs.get(&self.id).copied().unwrap_or_default() > self.epoch;
            if !alive || cut {
                self.assembler.finish(&mut lines);
                self.finished = true;
            }
        }
        self.index = entries.len();
        self.rendered.fetch_add(
            u64::try_from(lines.len()).unwrap_or(u64::MAX),
            Ordering::SeqCst,
        );
        self.ready.extend(lines);
        Ok(())
    }
}
