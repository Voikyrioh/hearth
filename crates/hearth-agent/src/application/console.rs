//! La console d'un serveur de jeu (HRT-268, ADR-0067, BR-CONSOLE-001 à 011) : décrire ce qu'elle permet, envoyer UNE
//! commande, lire les joueurs connectés.
//!
//! **Seul ce cas d'usage atteint `GameConsole`** (garde `architecture_guard.rs`). L'adresse est celle du conteneur DU
//! service lu dans le moteur, le port celui de la fiche : jamais une adresse ni un port venus d'un client. Le mot de
//! passe est descellé pour UN échange (`GameSettings::console_secret`), jamais tracé. Le texte d'une commande et la
//! réponse du jeu ne sont ni gardés, ni tracés, ni journalisés : le journal d'activité reçoit le verbe, la classe, le
//! joueur visé d'un verbe sensible, la longueur et l'issue (`ConsoleNote`).
//!
//! Refus, dans cet ordre (rien n'est envoyé au jeu) : service inconnu, adopté, console non ouverte à Hearth, forme,
//! verbe refusé (journalisé), classe signée fausse, arrêté ou pas prêt, verrou exclusif tenu, débit, console
//! injoignable (journalisé).

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex as StdMutex, PoisonError};
use std::time::Duration;

use hearth_proto::api::console::{
    ConsoleLimits, ConsoleOutcome, ConsolePolicyView, ConsoleReply, ConsoleRequest, ConsoleState,
    ConsoleView, QuickView, RefusedVerbView, SensitiveVerbView, UnavailableReason,
};
use hearth_proto::api::players::{PlayersState, PlayersView};
use hearth_proto::console::{
    ConsoleClass, ConsolePolicy, LineError, MAX_LINE_CHARS, RefusalReason, classify,
};
use time::OffsetDateTime;
use tokio::sync::Mutex;
use tokio::time::Instant;

use super::catalog::CatalogService;
use super::game_phase::GamePhases;
use super::game_settings::GameSettings;
use super::ports::{AuditSink, Clock, ConsoleBudget, ConsoleError, ContainerEngine, GameConsole};
use super::service_ops::{OpLocks, ServiceOps};
use super::service_specs::ServiceSpecs;
use super::services::{Driver, ServicesRegistry};
use crate::domain::audit::{
    Actor, AuditAction, ClientName, ConsoleIssue, ConsoleNote, Outcome, Target,
};
use crate::domain::catalog::loading::schema::fiche::{
    RemoteConsoleProtocol, RemoteConsoleUse, SensitiveTarget,
};
use crate::domain::catalog::readiness::GamePhase;
use crate::domain::console::bucket::{PER_MINUTE, TokenBucket};
use crate::domain::console::players::{self, Reader};
use crate::domain::services::{KeyKind, ServiceKey, ServiceStateKind};

/// Les lectures des joueurs d'un même service se partagent ce temps (BR-CONSOLE-011).
const PLAYERS_TTL: Duration = Duration::from_secs(10);

/// Pourquoi une demande n'aboutit pas. Aucun texte du jeu, aucun secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsoleRefusal {
    NotFound,
    Adopted,
    NotAvailable,
    Invalid(LineError),
    Refused(RefusalReason),
    ClassMismatch,
    NotRunning,
    Starting,
    /// Un geste exclusif tient le service (mods, sauvegarde avec arrêt, modification...).
    Busy,
    RateLimited {
        retry_after_s: u64,
    },
    Unreachable,
    Internal(String),
}

/// Ce que l'état d'un service permet à la console.
enum Readiness {
    Ready { container: String },
    Starting,
    Stopped,
}

/// Ce que la fiche et la description disent d'un service qui a une console ouverte à Hearth.
struct Resolved<'a> {
    usage: &'a RemoteConsoleUse,
    name: String,
    port: u16,
}

/// Ce que l'on sait d'un service avant de lui parler.
enum Standing<'a> {
    Available(Resolved<'a>),
    Unavailable(UnavailableReason),
}

#[derive(Default)]
struct PerService {
    /// Une commande à la fois par service.
    exchange: Mutex<()>,
    bucket: StdMutex<Option<TokenBucket>>,
    /// Les joueurs lus (une seule requête en vol : la serrure est tenue pendant la lecture).
    players: Mutex<Option<(Instant, PlayersView)>>,
}

pub struct ConsoleService {
    specs: Arc<ServiceSpecs>,
    catalog: Arc<CatalogService>,
    registry: Arc<ServicesRegistry>,
    ops: Arc<ServiceOps>,
    locks: Arc<OpLocks>,
    phases: Arc<GamePhases>,
    engine: Arc<dyn ContainerEngine>,
    console: Arc<dyn GameConsole>,
    settings: Arc<GameSettings>,
    sink: Arc<dyn AuditSink>,
    clock: Arc<dyn Clock>,
    started: Instant,
    services: StdMutex<HashMap<String, Arc<PerService>>>,
}

/// L'obstacle d'un verrou partagé rendu à la chute.
struct SharedGuard {
    locks: Arc<OpLocks>,
    key: String,
}

impl SharedGuard {
    fn take(locks: &Arc<OpLocks>, key: &ServiceKey) -> Option<Self> {
        locks
            .with(|table| table.acquire_shared_write(key.as_str()))
            .ok()?;
        Some(Self {
            locks: Arc::clone(locks),
            key: key.as_str().to_owned(),
        })
    }
}

impl Drop for SharedGuard {
    fn drop(&mut self) {
        self.locks
            .with(|table| table.release_shared_write(&self.key));
    }
}

/// La politique des verbes de la fiche, au vocabulaire partagé (`hearth_proto::console`).
fn policy_of(usage: &RemoteConsoleUse) -> ConsolePolicy {
    ConsolePolicy::new(
        usage.refused.iter().filter_map(|refused| {
            RefusalReason::parse(&refused.reason).map(|reason| (refused.verb.clone(), reason))
        }),
        usage
            .sensitive
            .iter()
            .map(|sensitive| sensitive.verb.clone()),
    )
}

impl ConsoleService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        specs: Arc<ServiceSpecs>,
        catalog: Arc<CatalogService>,
        registry: Arc<ServicesRegistry>,
        ops: Arc<ServiceOps>,
        phases: Arc<GamePhases>,
        engine: Arc<dyn ContainerEngine>,
        console: Arc<dyn GameConsole>,
        settings: Arc<GameSettings>,
        sink: Arc<dyn AuditSink>,
        clock: Arc<dyn Clock>,
    ) -> Arc<Self> {
        Arc::new(Self {
            locks: Arc::clone(ops.locks()),
            specs,
            catalog,
            registry,
            ops,
            phases,
            engine,
            console,
            settings,
            sink,
            clock,
            started: Instant::now(),
            services: StdMutex::new(HashMap::new()),
        })
    }

    fn state_of(&self, key: &ServiceKey) -> Arc<PerService> {
        let mut map = self.services.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(map.entry(key.as_str().to_owned()).or_default())
    }

    fn now_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// Ce que la description et la fiche disent de la console de ce service.
    async fn standing(&self, key: &ServiceKey) -> Result<Standing<'_>, ConsoleRefusal> {
        let target = self.registry.lifecycle_target(key);
        if key.kind() != KeyKind::Hearth {
            return Err(match target {
                Some(_) => ConsoleRefusal::Adopted,
                None => ConsoleRefusal::NotFound,
            });
        }
        let body = self
            .specs
            .body(key)
            .await
            .map_err(|error| ConsoleRefusal::Internal(error.to_string()))?
            .ok_or(ConsoleRefusal::NotFound)?;
        let Some(spec_fiche) = &body.fiche else {
            return Ok(Standing::Unavailable(UnavailableReason::NoConsole));
        };
        let Some(fiche) = self.catalog.fiche(&spec_fiche.id) else {
            return Ok(Standing::Unavailable(UnavailableReason::NoConsole));
        };
        let Some(console) = &fiche.file.remote_console else {
            return Ok(Standing::Unavailable(UnavailableReason::NoConsole));
        };
        let Some(usage) = &console.console_use else {
            return Ok(Standing::Unavailable(UnavailableReason::NoConsole));
        };
        if console.protocol != RemoteConsoleProtocol::SourceRcon {
            return Ok(Standing::Unavailable(
                UnavailableReason::ProtocolUnsupported,
            ));
        }
        let Some(port) = fiche
            .file
            .port
            .iter()
            .find(|port| port.id == console.port)
            .map(|port| port.container)
        else {
            return Ok(Standing::Unavailable(UnavailableReason::NoConsole));
        };
        Ok(Standing::Available(Resolved {
            usage,
            name: body.name.clone(),
            port,
        }))
    }

    /// Le service tourne-t-il et le jeu est-il prêt ? Un jeu dont l'agent n'a pas vu le signe « prêt » depuis son
    /// propre redémarrage (phase inconnue) n'est pas « en démarrage » : Hearth ne peut pas savoir, la commande part.
    async fn readiness(&self, key: &ServiceKey) -> Readiness {
        let Some(target) = self.registry.lifecycle_target(key) else {
            return Readiness::Stopped;
        };
        let container = match &target.driver {
            Driver::Engine { container_id } => container_id.clone(),
            Driver::Supervisor { .. } => return Readiness::Stopped,
        };
        match target.state {
            ServiceStateKind::Running => {}
            ServiceStateKind::Starting => return Readiness::Starting,
            _ => return Readiness::Stopped,
        }
        let phase = match self.phases.phase_of(key) {
            Some(GamePhase::Ready { .. }) => return Readiness::Ready { container },
            Some(_) => self.phases.refresh(key).await,
            None => None,
        };
        match phase {
            Some(GamePhase::FirstStart | GamePhase::Starting) => Readiness::Starting,
            _ => Readiness::Ready { container },
        }
    }

    // -----------------------------------------------------------------------------------------------------------
    // Décrire
    // -----------------------------------------------------------------------------------------------------------

    /// `GET /services/{key}/console` (BR-CONSOLE-001, 002, 004, 006, 010).
    pub async fn describe(
        &self,
        key: &ServiceKey,
        can_write: bool,
    ) -> Result<ConsoleView, ConsoleRefusal> {
        let resolved = match self.standing(key).await {
            Ok(Standing::Available(resolved)) => resolved,
            Ok(Standing::Unavailable(reason)) => return Ok(unavailable(reason, can_write)),
            Err(ConsoleRefusal::Adopted) => {
                return Ok(unavailable(UnavailableReason::Adopted, can_write));
            }
            Err(other) => return Err(other),
        };
        let state = if self.ops.busy(key).is_some() {
            ConsoleState::Busy
        } else {
            match self.readiness(key).await {
                Readiness::Ready { .. } => ConsoleState::Ready,
                Readiness::Starting => ConsoleState::Starting,
                Readiness::Stopped => ConsoleState::Stopped,
            }
        };
        let usage = resolved.usage;
        Ok(ConsoleView {
            available: true,
            unavailable_reason: None,
            state,
            can_write,
            policy: ConsolePolicyView {
                refused: usage
                    .refused
                    .iter()
                    .map(|refused| RefusedVerbView {
                        verb: refused.verb.clone(),
                        reason: refused.reason.clone(),
                    })
                    .collect(),
                sensitive: usage
                    .sensitive
                    .iter()
                    .map(|sensitive| SensitiveVerbView {
                        verb: sensitive.verb.clone(),
                        title: sensitive.title.clone(),
                        sentence: sensitive.sentence.clone(),
                        target: match sensitive.target {
                            SensitiveTarget::FirstArg => "first_arg",
                            SensitiveTarget::None => "none",
                        }
                        .to_owned(),
                    })
                    .collect(),
            },
            quick: usage
                .quick
                .iter()
                .map(|quick| QuickView {
                    id: quick.id.clone(),
                    label: quick.label.clone(),
                    text: quick.text.clone(),
                    mode: match quick.mode {
                        crate::domain::catalog::loading::schema::fiche::QuickMode::Send => "send",
                        crate::domain::catalog::loading::schema::fiche::QuickMode::Fill => "fill",
                    }
                    .to_owned(),
                })
                .collect(),
            limits: Some(ConsoleLimits {
                max_chars: u32::try_from(MAX_LINE_CHARS).unwrap_or(256),
                per_minute: PER_MINUTE,
                reply_timeout_s: usage.reply_timeout_s,
            }),
        })
    }

    // -----------------------------------------------------------------------------------------------------------
    // Envoyer
    // -----------------------------------------------------------------------------------------------------------

    /// `POST /services/{key}/console` (BR-CONSOLE-003 à 009). Le droit, l'acte signé et le mot de passe sont jugés par
    /// la couche d'accès AVANT d'arriver ici.
    pub async fn send(
        &self,
        key: &ServiceKey,
        actor: Actor,
        request: &ConsoleRequest,
    ) -> Result<ConsoleReply, ConsoleRefusal> {
        let resolved = match self.standing(key).await? {
            Standing::Available(resolved) => resolved,
            Standing::Unavailable(_) => return Err(ConsoleRefusal::NotAvailable),
        };
        let line = request.line().map_err(ConsoleRefusal::Invalid)?;
        let policy = policy_of(resolved.usage);
        let class = classify(&policy, &line);
        let service = ClientName::parse(&resolved.name)
            .or_else(|| ClientName::parse(key.as_str()))
            .map_or(Target::None, Target::Service);
        let sensitive_target = |sensitive: bool| -> Option<String> {
            if !sensitive {
                return None;
            }
            let verb = line.verb();
            resolved
                .usage
                .sensitive
                .iter()
                .find(|entry| entry.verb == verb)
                .filter(|entry| entry.target == SensitiveTarget::FirstArg)
                .and_then(|_| line.first_arg().map(str::to_owned))
        };
        let note = |class: &ConsoleClass, issue: ConsoleIssue| {
            ConsoleNote::new(
                &line.verb(),
                matches!(class, ConsoleClass::Sensitive),
                sensitive_target(matches!(class, ConsoleClass::Sensitive)).as_deref(),
                line.chars(),
                issue,
            )
        };
        let record = |class: &ConsoleClass, issue: ConsoleIssue, outcome: Outcome| {
            let note = note(class, issue);
            let target = match &service {
                Target::Service(name) => Target::ServiceConsole {
                    service: name.clone(),
                    note,
                },
                _ => Target::None,
            };
            (target, outcome)
        };
        if let ConsoleClass::Refused(reason) = class {
            let (target, outcome) = record(
                &class,
                ConsoleIssue::Refused,
                Outcome::Denied(crate::domain::audit::Reason::Validation),
            );
            self.sink
                .record(actor, AuditAction::ServiceConsole, target, outcome)
                .await;
            return Err(ConsoleRefusal::Refused(reason));
        }
        if class.signed_code() != Some(request.class) {
            return Err(ConsoleRefusal::ClassMismatch);
        }
        let container = match self.readiness(key).await {
            Readiness::Ready { container } => container,
            Readiness::Starting => return Err(ConsoleRefusal::Starting),
            Readiness::Stopped => return Err(ConsoleRefusal::NotRunning),
        };
        let _shared = SharedGuard::take(&self.locks, key).ok_or(ConsoleRefusal::Busy)?;
        let per_service = self.state_of(key);
        let _one_at_a_time = per_service.exchange.lock().await;
        {
            let now = self.now_ms();
            let mut bucket = per_service
                .bucket
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let bucket = bucket.get_or_insert_with(|| TokenBucket::full(now));
            bucket
                .take(now)
                .map_err(|retry_after_s| ConsoleRefusal::RateLimited { retry_after_s })?;
        }
        let exchange = self
            .talk(
                key,
                &container,
                &resolved,
                line.as_str(),
                resolved.usage.reply_timeout_s,
            )
            .await;
        match exchange {
            Ok(reply) => {
                let (outcome, text, issue) = match reply {
                    Exchange::Replied { text, truncated } if text.trim().is_empty() => (
                        ConsoleOutcome::Empty,
                        None,
                        (ConsoleIssue::Empty, truncated),
                    ),
                    Exchange::Replied { text, truncated } => (
                        ConsoleOutcome::Answered,
                        Some(text),
                        (ConsoleIssue::Answered, truncated),
                    ),
                    Exchange::NoAnswer => (
                        ConsoleOutcome::NoAnswer,
                        None,
                        (ConsoleIssue::NoAnswer, false),
                    ),
                };
                let (target, journal) = record(&class, issue.0, Outcome::Succeeded);
                self.sink
                    .record(actor, AuditAction::ServiceConsole, target, journal)
                    .await;
                tracing::info!(
                    class = if matches!(class, ConsoleClass::Sensitive) { "sensitive" } else { "free" },
                    outcome = ?outcome,
                    "console.send"
                );
                Ok(ConsoleReply {
                    outcome,
                    reply: text,
                    truncated: issue.1,
                })
            }
            Err(()) => {
                let (target, journal) = record(
                    &class,
                    ConsoleIssue::Unreachable,
                    Outcome::Failed(crate::domain::audit::Reason::EngineUnavailable),
                );
                self.sink
                    .record(actor, AuditAction::ServiceConsole, target, journal)
                    .await;
                Err(ConsoleRefusal::Unreachable)
            }
        }
    }

    /// Un échange : adresse privée lue du conteneur du service, port de la fiche, secret descellé pour l'occasion.
    /// `Err(())` : injoignable (aucun détail : ni adresse, ni secret, ni texte du jeu).
    async fn talk(
        &self,
        key: &ServiceKey,
        container: &str,
        resolved: &Resolved<'_>,
        line: &str,
        reply_timeout_s: u32,
    ) -> Result<Exchange, ()> {
        let address = match self.engine.private_address(container).await {
            Ok(Some(address)) => address,
            _ => {
                tracing::warn!("console.unreachable");
                return Err(());
            }
        };
        let secret = self.settings.console_secret(key).await.map_err(|_| {
            tracing::warn!("console.unreachable");
        })?;
        let target = SocketAddr::new(IpAddr::V4(address), resolved.port);
        match self
            .console
            .exchange(
                target,
                &secret,
                line,
                ConsoleBudget::production(reply_timeout_s),
            )
            .await
        {
            Ok(reply) => Ok(Exchange::Replied {
                text: reply.text,
                truncated: reply.truncated,
            }),
            Err(ConsoleError::NoAnswer) => Ok(Exchange::NoAnswer),
            Err(ConsoleError::AuthRefused) => {
                tracing::warn!("console.auth_failed");
                Err(())
            }
            Err(_) => {
                tracing::warn!("console.unreachable");
                Err(())
            }
        }
    }

    // -----------------------------------------------------------------------------------------------------------
    // Joueurs
    // -----------------------------------------------------------------------------------------------------------

    /// `GET /services/{key}/players` (BR-CONSOLE-011) : au plus une commande par 10 secondes et par service, quel que
    /// soit le nombre d'écrans ; hors débit ; aucun journal.
    pub async fn players(&self, key: &ServiceKey) -> Result<PlayersView, ConsoleRefusal> {
        let resolved = match self.standing(key).await {
            Ok(Standing::Available(resolved)) => resolved,
            Ok(Standing::Unavailable(_)) | Err(ConsoleRefusal::Adopted) => {
                return Ok(PlayersView::without_count(PlayersState::NoConsole));
            }
            Err(other) => return Err(other),
        };
        let container = match self.readiness(key).await {
            Readiness::Ready { container } => container,
            Readiness::Starting => {
                return Ok(PlayersView::without_count(PlayersState::Starting));
            }
            Readiness::Stopped => return Ok(PlayersView::without_count(PlayersState::Stopped)),
        };
        let per_service = self.state_of(key);
        let mut cache = per_service.players.lock().await;
        if let Some((at, view)) = cache.as_ref()
            && at.elapsed() < PLAYERS_TTL
        {
            return Ok(view.clone());
        }
        let stale = cache.as_ref().map(|(_, view)| view.clone());
        let (Some(command), Some(reader)) = (
            resolved.usage.players.as_ref(),
            resolved
                .usage
                .players
                .as_ref()
                .and_then(|players| Reader::parse(&players.reader)),
        ) else {
            return Ok(PlayersView::without_count(PlayersState::Unavailable));
        };
        // Un geste exclusif tient le service : la dernière lecture, ou « indisponible ».
        let Some(_shared) = SharedGuard::take(&self.locks, key) else {
            return Ok(
                stale.unwrap_or_else(|| PlayersView::without_count(PlayersState::Unavailable))
            );
        };
        let view = match self
            .talk(
                key,
                &container,
                &resolved,
                &command.command,
                resolved.usage.reply_timeout_s,
            )
            .await
        {
            Ok(Exchange::Replied { text, .. }) => match players::read(reader, &text) {
                Some(found) => PlayersView {
                    state: PlayersState::Ok,
                    online: Some(found.online),
                    max: Some(found.max),
                    names: Some(found.names),
                    hidden_names: found.hidden_names,
                    more_names: 0,
                    read_at: rfc3339(self.clock.now()),
                },
                None => PlayersView::without_count(PlayersState::Unavailable),
            },
            Ok(Exchange::NoAnswer) | Err(()) => {
                PlayersView::without_count(PlayersState::Unavailable)
            }
        };
        tracing::debug!(state = ?view.state, "console.players");
        *cache = Some((Instant::now(), view.clone()));
        Ok(view)
    }
}

enum Exchange {
    Replied { text: String, truncated: bool },
    NoAnswer,
}

fn unavailable(reason: UnavailableReason, can_write: bool) -> ConsoleView {
    ConsoleView {
        available: false,
        unavailable_reason: Some(reason),
        state: ConsoleState::Stopped,
        can_write,
        policy: ConsolePolicyView::default(),
        quick: Vec::new(),
        limits: None,
    }
}

fn rfc3339(at: OffsetDateTime) -> Option<String> {
    at.format(&time::format_description::well_known::Rfc3339)
        .ok()
}
