//! La tâche d'un serveur : elle seule possède la machine à états de ce serveur, son flux, ses
//! opérations en suspens. Une boucle unique attend les commandes, le flux, les tentatives, les
//! résultats internes, l'échéance de la machine et le battement.
//!
//! Supervision : la boucle tourne sous `catch_unwind`. Une panique est journalisée, comptée, et
//! la boucle repart à l'état `Offline` avec une nouvelle tentative ; les appels en cours
//! reçoivent `TaskRestarted`. Rien ne remonte en panique à l'appelant.

use std::collections::{HashMap, VecDeque};
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use futures_util::FutureExt as _;
use hearth_proto::api::machine::MachineResponse;
use hearth_proto::api::metrics::Sample;
use hearth_proto::error::ErrorCode;
use hearth_proto::stream::{ClientMessage, ServerMessage, SessionNotice};
use tokio::sync::{mpsc, oneshot};
use tokio::task::{AbortHandle, JoinError, JoinHandle};
use tokio::time::timeout;

use super::attempt::{self, AttemptResult};
use super::{ActionOutcome, ActionRequest, Deps, Shared};
use crate::domain::event::{Event, SessionEnd, StateInfo};
use crate::domain::pending_ops::{Lookup, OperationId, PendingOps, Resolution};
use crate::domain::server::{HISTORY_CAP, LastKnown, ServerId};
use crate::domain::state::{Effect, Input, LinkMachine, LinkState, Start, Status};
use crate::domain::time::WallTime;
use crate::error::LinkError;
use crate::ports::transport::{ApiResponse, Frame, StreamConn, TransportError};
use crate::ports::vault::SecretKind;

/// Ce que la façade demande à la tâche d'un serveur.
pub(crate) enum Command {
    RetryNow,
    Woke,
    NetworkChanged,
    /// L'utilisateur vient de se connecter (jeton et dossier mis à jour par la façade).
    LoggedIn,
    LoginRefused,
    LoggedOut,
    /// L'utilisateur a accepté la nouvelle empreinte (carnet mis à jour par la façade).
    FingerprintAccepted,
    Execute {
        request: ActionRequest,
        reply: oneshot::Sender<Result<ActionOutcome, LinkError>>,
    },
    Shutdown {
        done: oneshot::Sender<()>,
    },
}

/// Messages que les tâches filles (requêtes, relectures) envoient à la boucle.
enum Internal {
    OpResponse {
        id: OperationId,
        result: Result<ApiResponse, TransportError>,
    },
    Lookup {
        id: OperationId,
        lookup: Lookup,
    },
}

struct Waiter {
    reply: oneshot::Sender<Result<ActionOutcome, LinkError>>,
    request: AbortHandle,
}

#[derive(Clone, Copy)]
enum AttemptKind {
    Connect,
    Reauth,
}

enum Exit {
    /// La façade a disparu ou le serveur a été arrêté : fin normale.
    Finished,
}

/// Lance la tâche supervisée d'un serveur.
pub(crate) fn spawn(
    deps: Arc<Deps>,
    shared: Arc<Shared>,
    start: Start,
) -> (mpsc::Sender<Command>, JoinHandle<()>) {
    let (commands, receiver) = mpsc::channel(64);
    let handle = tokio::spawn(supervise(deps, shared, receiver, start));
    (commands, handle)
}

async fn supervise(
    deps: Arc<Deps>,
    shared: Arc<Shared>,
    mut commands: mpsc::Receiver<Command>,
    mut start: Start,
) {
    loop {
        let mut runner = Runner::new(deps.clone(), shared.clone(), start);
        let outcome = AssertUnwindSafe(runner.run(&mut commands))
            .catch_unwind()
            .await;
        match outcome {
            Ok(Exit::Finished) => return,
            Err(payload) => {
                shared.restarts.fetch_add(1, Ordering::SeqCst);
                tracing::error!(
                    server = %shared.id(),
                    panic = %describe(payload.as_ref()),
                    "tâche du lien en panne : le lien repart hors ligne"
                );
                // Les appels en cours de cette génération perdent leur canal de réponse :
                // `execute` rend `TaskRestarted`.
                drop(runner);
                start = Start::Recovered;
                tokio::time::sleep(deps.config.restart_delay).await;
            }
        }
    }
}

fn describe(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panique sans message".to_owned())
}

struct Runner {
    deps: Arc<Deps>,
    shared: Arc<Shared>,
    id: ServerId,
    machine: LinkMachine,
    pending: PendingOps,
    waiters: HashMap<OperationId, Waiter>,
    stream: Option<Box<dyn StreamConn>>,
    attempt: Option<JoinHandle<AttemptResult>>,
    internal_tx: mpsc::Sender<Internal>,
    internal_rx: mpsc::Receiver<Internal>,
    history: VecDeque<Sample>,
    machine_info: Option<Arc<MachineResponse>>,
    last_contact: Option<WallTime>,
    published: Option<Status>,
    ping_n: u64,
    last_saved: crate::domain::time::Mono,
    done: Option<oneshot::Sender<()>>,
    stopped: bool,
}

impl Runner {
    fn new(deps: Arc<Deps>, shared: Arc<Shared>, start: Start) -> Self {
        let rng = deps.rng.clone();
        let machine = LinkMachine::new(
            deps.config.thresholds,
            Box::new(move || rng.next_u32()),
            deps.clock.mono(),
            start,
        );
        let (internal_tx, internal_rx) = mpsc::channel(64);
        let last_contact = shared.record().last_contact_at;
        let previous = shared.last_known();
        let (history, machine_info) = match previous {
            Some(view) => (
                view.history.into_iter().collect(),
                view.machine.map(Arc::new),
            ),
            None => (VecDeque::new(), None),
        };
        Self {
            id: shared.id(),
            last_saved: deps.clock.mono(),
            deps,
            shared,
            machine,
            pending: PendingOps::new(),
            waiters: HashMap::new(),
            stream: None,
            attempt: None,
            internal_tx,
            internal_rx,
            history,
            machine_info,
            last_contact,
            published: None,
            ping_n: 0,
            done: None,
            stopped: false,
        }
    }

    async fn run(&mut self, commands: &mut mpsc::Receiver<Command>) -> Exit {
        self.publish();
        let mut heartbeat = tokio::time::interval(self.deps.config.heartbeat_period);
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            let wake = self
                .machine
                .deadline()
                .map(|at| at.since(self.deps.clock.mono()));
            tokio::select! {
                command = commands.recv() => match command {
                    Some(command) => self.on_command(command).await,
                    None => {
                        // Plus de façade : on s'arrête proprement.
                        self.input(Input::Shutdown).await;
                    }
                },
                frame = next_frame(&mut self.stream) => self.on_frame(frame).await,
                joined = next_attempt(&mut self.attempt) => self.on_attempt(joined).await,
                Some(message) = self.internal_rx.recv() => self.on_internal(message).await,
                () = sleep_or_pending(wake) => self.input(Input::Tick).await,
                _ = heartbeat.tick() => self.on_heartbeat().await,
            }
            if self.stopped {
                self.finish().await;
                return Exit::Finished;
            }
        }
    }

    /// Fin de vie : dernière vue sauvegardée, appels en attente libérés, accusé de l'arrêt.
    async fn finish(&mut self) {
        self.persist(true).await;
        for (_, waiter) in self.waiters.drain() {
            waiter.request.abort();
            let _ = waiter.reply.send(Err(LinkError::Stopped));
        }
        if let Some(done) = self.done.take() {
            let _ = done.send(());
        }
    }

    // ── Entrées ────────────────────────────────────────────────────────────────────────────

    /// Applique un événement à la machine, exécute les effets, annonce le changement d'état.
    async fn input(&mut self, input: Input) {
        let now = self.deps.clock.mono();
        let effects = self.machine.handle(now, input);
        self.apply(effects).await;
        self.publish();
    }

    async fn on_command(&mut self, command: Command) {
        match command {
            Command::RetryNow | Command::FingerprintAccepted => self.input(Input::RetryNow).await,
            Command::Woke => self.input(Input::Woke).await,
            Command::NetworkChanged => self.input(Input::NetworkChanged).await,
            Command::LoggedIn => self.input(Input::LoginSucceeded).await,
            Command::LoginRefused => self.input(Input::LoginRefused).await,
            Command::LoggedOut => self.input(Input::LoggedOut).await,
            Command::Execute { request, reply } => self.on_execute(request, reply),
            Command::Shutdown { done } => {
                self.done = Some(done);
                self.input(Input::Shutdown).await;
            }
        }
    }

    async fn on_frame(&mut self, frame: Result<Frame, TransportError>) {
        match frame {
            Ok(Frame::Message(message)) => {
                self.traffic();
                self.on_message(*message).await;
            }
            Ok(Frame::Other) => self.traffic(),
            // Le flux reste en place jusqu'à l'effet `CloseStream` de la machine.
            Err(_) => self.input(Input::TransportFailed).await,
        }
    }

    /// Un signe de vie du serveur : repousse l'échéance du silence.
    fn traffic(&mut self) {
        let now = self.deps.clock.mono();
        let wall = self.deps.clock.wall();
        self.machine.handle(now, Input::Traffic);
        self.last_contact = Some(wall);
        self.shared
            .state
            .send_modify(|info| info.last_contact_at = Some(wall));
    }

    async fn on_message(&mut self, message: ServerMessage) {
        match message {
            ServerMessage::Metrics(sample) => {
                let sample = Arc::new(sample);
                self.history.push_back((*sample).clone());
                while self.history.len() > HISTORY_CAP {
                    self.history.pop_front();
                }
                self.refresh_last_known();
                self.deps.sink.emit(Event::Metrics {
                    server: self.id.clone(),
                    sample,
                });
                self.persist(false).await;
            }
            ServerMessage::Snapshot { machine, history } => {
                self.install_snapshot(machine, history);
            }
            ServerMessage::Audit { event } => self.deps.sink.emit(Event::Audit {
                server: self.id.clone(),
                event: Arc::new(event),
            }),
            ServerMessage::Session { kind } => match kind {
                SessionNotice::Expired => self.session_expired().await,
                SessionNotice::Revoked => self.access_revoked().await,
            },
            ServerMessage::Error(detail) => match detail.code {
                ErrorCode::SessionExpired | ErrorCode::Unauthenticated => {
                    self.session_expired().await;
                }
                ErrorCode::SessionRevoked => self.access_revoked().await,
                // Les autres erreurs (rôle, trop de messages…) laissent le flux vivre.
                _ => {}
            },
            ServerMessage::Pong { .. } => {}
        }
    }

    fn install_snapshot(&mut self, machine: MachineResponse, history: Vec<Sample>) {
        let machine = Arc::new(machine);
        self.history = history.into_iter().collect();
        while self.history.len() > HISTORY_CAP {
            self.history.pop_front();
        }
        self.machine_info = Some(machine.clone());
        self.refresh_last_known();
        self.deps.sink.emit(Event::Snapshot {
            server: self.id.clone(),
            machine,
            history: Arc::new(self.history.iter().cloned().collect()),
        });
    }

    async fn on_attempt(&mut self, joined: Result<AttemptResult, JoinError>) {
        self.attempt = None;
        let result = match joined {
            Ok(result) => result,
            Err(error) => {
                tracing::error!(server = %self.id, %error, "tentative de connexion en panne");
                AttemptResult::Failed
            }
        };
        match result {
            AttemptResult::Ready {
                stream,
                machine,
                history,
            } => {
                self.stream = Some(stream);
                let now = self.deps.clock.mono();
                let effects = self.machine.handle(now, Input::Connected);
                if effects.contains(&Effect::ResolvePending) {
                    self.last_contact = Some(self.deps.clock.wall());
                    self.install_snapshot(*machine, history);
                } else {
                    // Résultat périmé (déconnexion entre-temps) : on ne le garde pas.
                    self.stream = None;
                }
                self.apply(effects).await;
                self.publish();
            }
            AttemptResult::Failed => self.input(Input::TransportFailed).await,
            AttemptResult::SessionExpired => self.session_expired().await,
            AttemptResult::Revoked => self.access_revoked().await,
            AttemptResult::Fingerprint { presented } => {
                let expected = self.shared.record().fingerprint;
                self.deps.sink.emit(Event::FingerprintChanged {
                    server: self.id.clone(),
                    expected,
                    presented,
                });
                self.input(Input::FingerprintChanged).await;
            }
            AttemptResult::Incompatible(target) => self.input(Input::Incompatible(target)).await,
            AttemptResult::Reauthenticated => self.input(Input::Reauthenticated).await,
        }
    }

    async fn on_internal(&mut self, message: Internal) {
        match message {
            Internal::OpResponse { id, result } => self.on_op_response(id, result).await,
            Internal::Lookup { id, lookup } => self.on_lookup(id, lookup),
        }
    }

    async fn on_heartbeat(&mut self) {
        let Some(stream) = self.stream.as_mut() else {
            return;
        };
        self.ping_n = self.ping_n.wrapping_add(1);
        let sent = timeout(
            self.deps.config.heartbeat_period,
            stream.send(&ClientMessage::Ping { n: self.ping_n }),
        )
        .await;
        if !matches!(sent, Ok(Ok(()))) {
            self.input(Input::TransportFailed).await;
        }
    }

    // ── Session ────────────────────────────────────────────────────────────────────────────

    fn has_saved_password(&self) -> bool {
        matches!(
            self.deps.vault.get(&self.id, SecretKind::Password),
            Ok(Some(_))
        )
    }

    /// 401 `SESSION_EXPIRED` : reconnexion silencieuse si le mot de passe est au coffre, sinon
    /// « Session expirée » (BR-RESIL-012, 013).
    async fn session_expired(&mut self) {
        let can_reauth = self.has_saved_password();
        self.input(Input::SessionExpired { can_reauth }).await;
        if self.machine.state() == LinkState::SessionExpired {
            let _ = self.deps.vault.delete(&self.id, SecretKind::Token);
            self.deps.sink.emit(Event::SessionEnded {
                server: self.id.clone(),
                kind: SessionEnd::Expired,
            });
        }
    }

    /// 401 `SESSION_REVOKED` ou identifiants refusés : plus aucune tentative (BR-RESIL-014).
    async fn access_revoked(&mut self) {
        self.input(Input::AccessRevoked).await;
        if self.machine.state() == LinkState::AccessRevoked {
            let _ = self.deps.vault.delete(&self.id, SecretKind::Token);
            let _ = self.deps.vault.delete(&self.id, SecretKind::Password);
            let mut record = self.shared.record();
            if record.remember {
                record.remember = false;
                self.shared.set_record(record.clone());
                if let Err(error) = self.deps.servers.save(&record).await {
                    tracing::warn!(server = %self.id, %error, "carnet non mis à jour");
                }
            }
            self.deps.sink.emit(Event::SessionEnded {
                server: self.id.clone(),
                kind: SessionEnd::Revoked,
            });
        }
    }

    // ── Effets ─────────────────────────────────────────────────────────────────────────────

    async fn apply(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::StartAttempt => self.start_attempt(AttemptKind::Connect),
                Effect::Reauthenticate => self.start_attempt(AttemptKind::Reauth),
                Effect::AbortAttempt => self.abort_attempt(),
                Effect::CloseStream => {
                    if self.stream.take().is_some() {
                        self.persist(true).await;
                    }
                }
                Effect::MarkPendingUnknown => self.mark_pending_unknown(),
                Effect::ResolvePending => self.resolve_pending(),
                Effect::Stop => self.stopped = true,
            }
        }
    }

    fn start_attempt(&mut self, kind: AttemptKind) {
        self.abort_attempt();
        let deps = self.deps.clone();
        let shared = self.shared.clone();
        self.attempt = Some(tokio::spawn(async move {
            match kind {
                AttemptKind::Connect => attempt::connect(&deps, &shared).await,
                AttemptKind::Reauth => attempt::reauthenticate(&deps, &shared).await,
            }
        }));
    }

    fn abort_attempt(&mut self) {
        if let Some(handle) = self.attempt.take() {
            handle.abort();
        }
    }

    /// Le lien est tombé : chaque action en vol devient « résultat inconnu » ; ses appelants le
    /// savent tout de suite. Rien n'est jamais renvoyé (BR-RESIL-009).
    fn mark_pending_unknown(&mut self) {
        for id in self.pending.link_lost() {
            if let Some(waiter) = self.waiters.remove(&id) {
                waiter.request.abort();
                let _ = waiter
                    .reply
                    .send(Ok(ActionOutcome::ResultUnknown { id: id.clone() }));
            }
        }
    }

    fn resolve_pending(&mut self) {
        let ids = self.pending.to_resolve();
        if !ids.is_empty() {
            self.spawn_resolver(ids, Duration::ZERO);
        }
    }

    fn spawn_resolver(&self, ids: Vec<OperationId>, delay: Duration) {
        let deps = self.deps.clone();
        let shared = self.shared.clone();
        let sender = self.internal_tx.clone();
        tokio::spawn(async move {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            for id in ids {
                let lookup = attempt::lookup(&deps, &shared, &id).await;
                if sender.send(Internal::Lookup { id, lookup }).await.is_err() {
                    return;
                }
            }
        });
    }

    // ── Actions ────────────────────────────────────────────────────────────────────────────

    fn on_execute(
        &mut self,
        request: ActionRequest,
        reply: oneshot::Sender<Result<ActionOutcome, LinkError>>,
    ) {
        // Hors « Connecté » (flux ouvert), rien ne part (BR-RESIL-008).
        let Some(token) = self.stream.as_ref().and_then(|_| {
            self.deps
                .vault
                .get(&self.id, SecretKind::Token)
                .ok()
                .flatten()
        }) else {
            let _ = reply.send(Err(LinkError::NotConnected));
            return;
        };
        let key = ulid::Ulid::generate().to_string();
        let Ok(id) = OperationId::parse(&key) else {
            let _ = reply.send(Err(LinkError::Protocol("clé d'opération".into())));
            return;
        };
        let kind = format!("{} {}", request.method.as_str(), request.path);
        if self
            .pending
            .register(id.clone(), kind, self.deps.clock.wall())
            .is_err()
        {
            let _ = reply.send(Err(LinkError::TooManyPending));
            return;
        }
        let api_request = crate::ports::transport::ApiRequest {
            method: request.method,
            path: request.path,
            body: request.body,
            idempotency_key: Some(key),
        };
        let deps = self.deps.clone();
        let target = self.shared.target();
        let sender = self.internal_tx.clone();
        let operation = id.clone();
        let task = tokio::spawn(async move {
            let result =
                attempt::guarded(deps.transport.request(&target, &token, &api_request)).await;
            let _ = sender
                .send(Internal::OpResponse {
                    id: operation,
                    result,
                })
                .await;
        });
        self.waiters.insert(
            id,
            Waiter {
                reply,
                request: task.abort_handle(),
            },
        );
    }

    async fn on_op_response(
        &mut self,
        id: OperationId,
        result: Result<ApiResponse, TransportError>,
    ) {
        // Plus de preneur : la réponse est arrivée après la coupure ; la relecture tranchera.
        let Some(waiter) = self.waiters.remove(&id) else {
            return;
        };
        match result {
            Ok(response) => {
                self.pending.complete(&id);
                self.traffic();
                let _ = waiter.reply.send(Ok(ActionOutcome::Completed {
                    status: response.status,
                    body: response.body,
                    replayed: response.replayed,
                }));
            }
            Err(TransportError::FingerprintMismatch { presented }) => {
                self.pending.mark_unknown(&id);
                let _ = waiter
                    .reply
                    .send(Ok(ActionOutcome::ResultUnknown { id: id.clone() }));
                let expected = self.shared.record().fingerprint;
                self.deps.sink.emit(Event::FingerprintChanged {
                    server: self.id.clone(),
                    expected,
                    presented,
                });
                self.input(Input::FingerprintChanged).await;
            }
            Err(_) => {
                // La requête a échoué sans réponse : on ne sait pas ce qu'elle est devenue. Le
                // flux, lui, vit : on relit l'opération tout de suite.
                self.pending.mark_unknown(&id);
                let _ = waiter
                    .reply
                    .send(Ok(ActionOutcome::ResultUnknown { id: id.clone() }));
                if self.stream.is_some() {
                    self.spawn_resolver(vec![id], Duration::ZERO);
                }
            }
        }
    }

    fn on_lookup(&mut self, id: OperationId, lookup: Lookup) {
        let running = matches!(
            &lookup,
            Lookup::Status {
                status: hearth_proto::api::operations::OperationStatus::Running,
                ..
            }
        );
        let wall = self.deps.clock.wall();
        match self.pending.resolve(&id, lookup, wall) {
            Some(Resolution::Final(outcome)) => self.deps.sink.emit(Event::Operation {
                server: self.id.clone(),
                id,
                outcome,
            }),
            Some(Resolution::CheckAgain) if running => {
                let delay = attempt::recheck_delay(&self.deps);
                self.spawn_resolver(vec![id], delay);
            }
            // Agent injoignable : on relira au prochain retour du lien.
            Some(Resolution::CheckAgain) | None => {}
        }
    }

    // ── Dernière vue, carnet ───────────────────────────────────────────────────────────────

    fn refresh_last_known(&mut self) {
        let view = LastKnown {
            at: self.last_contact.unwrap_or_else(|| self.deps.clock.wall()),
            machine: self.machine_info.as_deref().cloned(),
            history: self.history.iter().cloned().collect(),
        };
        self.shared.set_last_known(view);
    }

    /// Sauvegarde la dernière vue et la date du dernier contact. Hors `force`, au plus une fois
    /// par période.
    async fn persist(&mut self, force: bool) {
        let now = self.deps.clock.mono();
        if !force && now.since(self.last_saved) < self.deps.config.snapshot_save_period {
            return;
        }
        self.last_saved = now;
        if let Some(view) = self.shared.last_known()
            && let Err(error) = self.deps.snapshots.save(&self.id, &view).await
        {
            tracing::warn!(server = %self.id, %error, "dernière vue non sauvegardée");
        }
        let mut record = self.shared.record();
        if record.last_contact_at != self.last_contact {
            record.last_contact_at = self.last_contact;
            self.shared.set_record(record.clone());
            if let Err(error) = self.deps.servers.save(&record).await {
                tracing::warn!(server = %self.id, %error, "dernier contact non sauvegardé");
            }
        }
    }

    // ── Annonce de l'état ──────────────────────────────────────────────────────────────────

    fn publish(&mut self) {
        let status = self.machine.status();
        let now = self.deps.clock.mono();
        let wall = self.deps.clock.wall();
        let info = StateInfo::from_status(&status, now, wall, self.last_contact);
        self.shared.state.send_replace(info);
        let changed = self
            .published
            .as_ref()
            .is_none_or(|previous| display_changed(previous, &status));
        self.published = Some(status);
        if changed {
            self.deps.sink.emit(Event::State {
                server: self.id.clone(),
                info,
            });
        }
    }
}

/// Faut-il annoncer ce changement ? Oui si l'état affiché ou le blocage change. Tant que l'état
/// affiché est « Connecté » (coupure encore invisible), rien ne s'annonce : ni nouvelle date de
/// tentative ni compteur. Hors « Connecté », on annonce chaque nouvelle date de tentative (compte
/// à rebours) et chaque échec de plus (BR-RESIL-018) ; le passage « tentative en cours » (plus de
/// date) n'est pas un événement de plus : il y en a un par tentative, pas deux. Le dernier
/// contact, qui bouge à chaque message, ne compte jamais.
fn display_changed(previous: &Status, now: &Status) -> bool {
    if previous.state != now.state || previous.blocked != now.blocked || previous.since != now.since
    {
        return true;
    }
    now.state != LinkState::Connected
        && ((now.next_retry_at.is_some() && previous.next_retry_at != now.next_retry_at)
            || previous.failed_attempts != now.failed_attempts)
}

async fn next_frame(stream: &mut Option<Box<dyn StreamConn>>) -> Result<Frame, TransportError> {
    match stream {
        Some(stream) => stream.recv().await,
        None => std::future::pending().await,
    }
}

async fn next_attempt(
    attempt: &mut Option<JoinHandle<AttemptResult>>,
) -> Result<AttemptResult, JoinError> {
    match attempt {
        Some(handle) => handle.await,
        None => std::future::pending().await,
    }
}

async fn sleep_or_pending(wake: Option<Duration>) {
    match wake {
        Some(delay) => tokio::time::sleep(delay).await,
        None => std::future::pending().await,
    }
}
