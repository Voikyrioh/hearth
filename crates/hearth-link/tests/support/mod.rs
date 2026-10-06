//! Outils communs des tests d'intégration : un vrai agent, un mandataire à pannes, un
//! `LinkManager` aux seuils réduits (les mêmes scénarios, 6 fois plus vite : 3 s devient 0,5 s,
//! 30 s devient 5 s, les délais de reconnexion suivent).
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

pub mod agent;
pub mod proxy;
pub mod update_rig;

use std::collections::BTreeSet;
use std::net::IpAddr;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use hearth_agent::domain::accounts::Role;
use hearth_link::adapters::{
    FileOperationStore, FileServerStore, FileSnapshotStore, HttpTransport, HttpTransportConfig,
    MemoryVault, OsRng, SystemClock,
};
use hearth_link::domain::event::{Event, StateInfo};
use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::domain::state::{LinkState, Thresholds};
use hearth_link::domain::time::{Mono, WallTime};
use hearth_link::ports::Clock;
use hearth_link::ports::net_watcher::{NetError, NetWatcher};
use hearth_link::{LinkConfig, LinkManager, NewServer, Ports};
use hearth_proto::fingerprint::Fingerprint;
use tempfile::TempDir;

pub use agent::{PASSWORD, TestAgent};
pub use proxy::FaultProxy;

/// Facteur de réduction des durées du produit.
pub const SCALE: u32 = 6;

/// Configuration par défaut des scénarios : AUCUN délai de la bibliothèque ne peut être atteint par la
/// seule lenteur de la machine (silence du lien, battement, seuils « Reconnexion » et « Hors ligne »
/// hors d'atteinte ; la coupure d'un mandataire se voit par l'erreur de transport, pas par le
/// silence). Les scénarios qui ÉPROUVENT un délai le disent : `silence_config()` (gel), ou
/// `Options::with_thresholds` avec les seuils voulus.
pub fn fast_config() -> LinkConfig {
    LinkConfig {
        thresholds: thresholds(Some(never()), true, true),
        heartbeat_period: Duration::from_secs(5),
        ..scaled_config()
    }
}

/// Seuils à l'échelle des tests, silence et battement compris : seulement pour les scénarios qui
/// éprouvent la détection d'un flux muet (agent figé, trou noir).
pub fn silence_config() -> LinkConfig {
    LinkConfig {
        thresholds: Thresholds::scaled(SCALE),
        heartbeat_period: Duration::from_millis(333),
        ..scaled_config()
    }
}

fn scaled_config() -> LinkConfig {
    LinkConfig {
        // Délais de garde des échanges : très larges, pour qu'une machine saturée ne transforme
        // jamais une réponse lente en échec.
        attempt_timeout: Duration::from_secs(30),
        request_timeout: Duration::from_secs(30),
        net_poll_period: Duration::from_millis(60),
        wake_check_period: Duration::from_millis(50),
        snapshot_save_period: Duration::from_millis(200),
        restart_delay: Duration::from_millis(50),
        recheck_delay: Duration::from_millis(100),
        ..LinkConfig::default()
    }
}

/// Réseau scripté : la liste des adresses locales est celle qu'on y met.
pub struct ScriptedNet {
    addresses: Mutex<BTreeSet<IpAddr>>,
    /// Lectures de la liste faites par le veilleur depuis le dernier `set`.
    reads_since_set: std::sync::atomic::AtomicU64,
}

impl ScriptedNet {
    pub fn new() -> Self {
        Self {
            addresses: Mutex::new(["192.168.1.20".parse().unwrap()].into()),
            reads_since_set: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn set(&self, addresses: &[&str]) {
        *self.addresses.lock().unwrap() = addresses.iter().map(|a| a.parse().unwrap()).collect();
        self.reads_since_set.store(0, Ordering::SeqCst);
    }

    /// Attend que le veilleur ait lu la liste au moins DEUX fois depuis le dernier `set` : la
    /// première lecture a vu le changement, la seconde ne commence qu'une fois la première traitée.
    pub async fn wait_seen(&self, limit: Duration) {
        let deadline = Instant::now() + limit;
        while self.reads_since_set.load(Ordering::SeqCst) < 2 {
            assert!(
                Instant::now() < deadline,
                "le veilleur n'a pas relu le réseau"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
}

#[async_trait]
impl NetWatcher for ScriptedNet {
    async fn addresses(&self) -> Result<BTreeSet<IpAddr>, NetError> {
        self.reads_since_set.fetch_add(1, Ordering::SeqCst);
        Ok(self.addresses.lock().unwrap().clone())
    }
}

/// Horloge dont la date murale peut sauter (veille du PC) sans que l'horloge monotone bouge.
pub struct JumpClock {
    inner: SystemClock,
    jump_ms: AtomicI64,
}

impl JumpClock {
    pub fn new() -> Self {
        Self {
            inner: SystemClock::new(),
            jump_ms: AtomicI64::new(0),
        }
    }

    pub fn jump(&self, by: Duration) {
        self.jump_ms
            .fetch_add(i64::try_from(by.as_millis()).unwrap(), Ordering::SeqCst);
    }
}

impl Clock for JumpClock {
    fn mono(&self) -> Mono {
        self.inner.mono()
    }

    fn wall(&self) -> WallTime {
        WallTime::from_millis(self.inner.wall().as_millis() + self.jump_ms.load(Ordering::SeqCst))
    }
}

/// Tout ce qui s'est passé, daté, relu à volonté.
#[derive(Clone, Default)]
pub struct Recorder {
    log: Arc<Mutex<Vec<(Instant, Event)>>>,
}

impl Recorder {
    pub fn spawn(mut stream: hearth_link::EventStream) -> Self {
        let recorder = Self::default();
        let log = recorder.log.clone();
        tokio::spawn(async move {
            while let Some(event) = stream.recv().await {
                log.lock().unwrap().push((Instant::now(), event));
            }
        });
        recorder
    }

    /// Position actuelle du journal : les attentes ne regardent que ce qui suit.
    pub fn mark(&self) -> usize {
        self.log.lock().unwrap().len()
    }

    pub fn since(&self, mark: usize) -> Vec<(Instant, Event)> {
        self.log
            .lock()
            .unwrap()
            .iter()
            .skip(mark)
            .cloned()
            .collect()
    }

    /// Les états affichés depuis `mark`, sans répétition consécutive (une même valeur annoncée
    /// deux fois, avec une nouvelle date de tentative, n'est pas un changement d'état).
    pub fn states_since(&self, mark: usize) -> Vec<LinkState> {
        let mut states: Vec<LinkState> = self
            .since(mark)
            .into_iter()
            .filter_map(|(_, event)| match event {
                Event::State { info, .. } => Some(info.state),
                _ => None,
            })
            .collect();
        states.dedup();
        states
    }

    /// Attend qu'un événement vérifie `predicate` ; le rend avec l'instant où il est arrivé.
    pub async fn wait_for(
        &self,
        mark: usize,
        what: &str,
        limit: Duration,
        predicate: impl Fn(&Event) -> bool,
    ) -> (Instant, Event) {
        let deadline = Instant::now() + limit;
        loop {
            if let Some(found) = self.since(mark).into_iter().find(|(_, e)| predicate(e)) {
                return found;
            }
            assert!(
                Instant::now() < deadline,
                "délai dépassé en attendant : {what}. États vus : {:?}",
                self.states_since(mark)
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    pub async fn wait_state(&self, mark: usize, state: LinkState, limit: Duration) -> Instant {
        self.wait_for(
            mark,
            &format!("état {state:?}"),
            limit,
            |event| matches!(event, Event::State { info, .. } if info.state == state),
        )
        .await
        .0
    }

    /// Attend que les mesures arrivent de nouveau (le flux vit).
    pub async fn wait_metrics(&self, mark: usize, limit: Duration) {
        self.wait_for(mark, "des mesures", limit, |e| {
            matches!(e, Event::Metrics { .. })
        })
        .await;
    }
}

pub struct World {
    pub agent: TestAgent,
    pub proxy: FaultProxy,
    pub manager: LinkManager,
    pub vault: Arc<MemoryVault>,
    pub id: ServerId,
    pub recorder: Recorder,
    pub net: Arc<ScriptedNet>,
    pub clock: Arc<JumpClock>,
    pub fingerprint: Fingerprint,
    pub dir: TempDir,
}

pub struct Options {
    pub remember: bool,
    pub role: Role,
    pub config: LinkConfig,
    /// Adaptateurs de mise à jour de l'agent (`update_rig::Rig::updating`) ; sans eux, ceux de la
    /// production.
    pub updating: Option<agent::UpdatingFactory>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            remember: false,
            role: Role::Admin,
            config: fast_config(),
            updating: None,
        }
    }
}

/// Un `LinkManager` complet sur ce dossier et ce coffre : carnet et dernières vues en fichiers,
/// transport réel, horloge et réseau fournis.
pub async fn start_manager(
    dir: &std::path::Path,
    vault: Arc<MemoryVault>,
    net: Arc<ScriptedNet>,
    clock: Arc<JumpClock>,
    config: LinkConfig,
) -> LinkManager {
    start_manager_with(dir, vault, net, clock, config, transport()).await
}

pub async fn start_manager_with(
    dir: &std::path::Path,
    vault: Arc<MemoryVault>,
    net: Arc<ScriptedNet>,
    clock: Arc<JumpClock>,
    config: LinkConfig,
    transport: HttpTransport,
) -> LinkManager {
    LinkManager::start(
        Ports {
            transport: Arc::new(transport),
            vault,
            servers: Arc::new(FileServerStore::new(dir.join("servers.json"))),
            snapshots: Arc::new(FileSnapshotStore::new(dir.join("snapshots"))),
            operations: Arc::new(FileOperationStore::new(dir.join("operations"))),
            clock,
            rng: Arc::new(OsRng::default()),
            net,
            extra_sink: None,
        },
        config,
    )
    .await
    .unwrap()
}

pub fn transport() -> HttpTransport {
    HttpTransport::new(HttpTransportConfig {
        connect_timeout: Duration::from_secs(10),
        request_timeout: Duration::from_secs(30),
        send_timeout: Duration::from_secs(10),
        client_name: "poste-test/0.1".into(),
    })
}

/// Transport au délai de connexion court : pour le test dont c'est l'objet (une sonde qui n'obtient
/// jamais de réponse doit échouer sans attendre le délai généreux des autres tests).
pub fn short_transport() -> HttpTransport {
    HttpTransport::new(HttpTransportConfig {
        connect_timeout: Duration::from_millis(500),
        request_timeout: Duration::from_secs(30),
        send_timeout: Duration::from_secs(10),
        client_name: "poste-test/0.1".into(),
    })
}

impl World {
    /// Agent installé, compte « marie », mandataire, `LinkManager` connecté.
    pub async fn connected(options: Options) -> Self {
        let agent = TestAgent::install_with(options.updating).await;
        agent.create_account("marie", options.role).await;
        let proxy = FaultProxy::start(agent.addr).await;
        let dir = tempfile::tempdir().unwrap();
        let vault = Arc::new(MemoryVault::new());
        let net = Arc::new(ScriptedNet::new());
        let clock = Arc::new(JumpClock::new());
        let manager = start_manager(
            dir.path(),
            vault.clone(),
            net.clone(),
            clock.clone(),
            options.config,
        )
        .await;
        let recorder = Recorder::spawn(manager.subscribe());

        let probe = manager.probe("127.0.0.1", proxy.port()).await.unwrap();
        let id = manager
            .add_server(NewServer {
                name: "Forge".into(),
                color: "#7aa2f7".into(),
                host: "127.0.0.1".into(),
                port: proxy.port(),
                fingerprint: probe.fingerprint,
                mac_addresses: probe.hello.mac_addresses.clone(),
            })
            .await
            .unwrap();
        let mark = recorder.mark();
        manager
            .login(&id, "marie", Secret::from(PASSWORD), options.remember)
            .await
            .unwrap();
        recorder.wait_state(mark, LinkState::Connected, WAIT).await;
        recorder.wait_metrics(mark, WAIT).await;
        Self {
            agent,
            proxy,
            manager,
            vault,
            id,
            recorder,
            net,
            clock,
            fingerprint: probe.fingerprint,
            dir,
        }
    }

    pub fn state(&self) -> StateInfo {
        self.manager.state(&self.id).unwrap()
    }
}

/// Délai de garde des attentes : un test bloqué échoue au bout de ce temps. Jamais une assertion
/// de vitesse : chaque attente porte sur un fait observable (état, événement, compteur).
pub const WAIT: Duration = Duration::from_secs(60);

/// Seuils si larges qu'aucune machine ne peut les franchir par hasard : le lien n'affiche jamais
/// rien tant que le test ne le provoque pas (les seuils exacts sont prouvés par `domain/state`).
pub fn never() -> Duration {
    Duration::from_secs(3_600)
}

/// Les seuils de l'échelle des tests, dont certains sont relevés à « jamais » : `silence`
/// (sinon un calcul retardé de 0,5 s passe pour une coupure), `reconnecting_after` et
/// `offline_after` selon ce que le scénario attend.
pub fn thresholds(silence: Option<Duration>, reconnecting: bool, offline: bool) -> Thresholds {
    let base = Thresholds::scaled(SCALE);
    Thresholds {
        silence: silence.unwrap_or(base.silence),
        reconnecting_after: if reconnecting {
            base.reconnecting_after
        } else {
            never()
        },
        offline_after: if offline {
            base.offline_after
        } else {
            never() * 2
        },
        ..base
    }
}

impl Options {
    /// Scénario qui éprouve la détection d'un flux muet : silence et battement à l'échelle.
    pub fn silent_link() -> Self {
        Self {
            config: silence_config(),
            ..Self::default()
        }
    }

    pub fn with_thresholds(thresholds: Thresholds) -> Self {
        Self {
            config: LinkConfig {
                thresholds,
                ..fast_config()
            },
            ..Self::default()
        }
    }
}

/// Attend `n` salves de mesures de plus (le flux vit, quelle que soit la vitesse de la machine).
pub async fn wait_metrics_times(recorder: &Recorder, n: usize) {
    for _ in 0..n {
        let mark = recorder.mark();
        recorder.wait_metrics(mark, WAIT).await;
    }
}

/// Attend que le mandataire ait reçu `n` connexions de plus (le lien tente de se rétablir).
pub async fn wait_attempts(proxy: &FaultProxy, n: u64) {
    let target = proxy.accepted() + n;
    let deadline = Instant::now() + WAIT;
    while proxy.accepted() < target {
        assert!(Instant::now() < deadline, "aucune nouvelle tentative");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// Durée réelle d'une durée du produit à l'échelle des tests.
pub fn scaled(real: Duration) -> Duration {
    real / SCALE
}

/// Attend que l'action arrive chez l'agent ; si `execute` rend avant, dit ce qu'il a rendu (au lieu
/// d'attendre pour rien jusqu'au délai de garde).
pub async fn wait_started_or_returned<T: std::fmt::Debug>(
    agent: &TestAgent,
    baseline: u32,
    call: &mut tokio::task::JoinHandle<T>,
) {
    tokio::select! {
        () = agent.wait_action_started(baseline) => {}
        returned = &mut *call => panic!("execute a rendu avant l'arrivée chez l'agent : {returned:?}"),
    }
}
