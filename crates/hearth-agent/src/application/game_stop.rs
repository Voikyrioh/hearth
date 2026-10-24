//! Arrêter un serveur de jeu en enregistrant son monde (HRT-103, ADR-0052, BR-JEU-001 à BR-JEU-007, BR-JEU-011,
//! BR-JEU-032, BR-JEU-034, BR-JEU-038).
//!
//! L'agent déroule la SUITE D'ÉTAPES que la fiche déclare, bornée et jamais saisie : un ordre littéral écrit sur
//! l'entrée standard du service (`ContainerEngine::send_stdin_line`), l'attente d'un signe LITTÉRAL dans les
//! journaux du jeu, une pause. Puis l'ordre de sortie, et **aussitôt** l'arrêt demandé au moteur avec le délai du
//! service : sans lui, un programme qui sort de lui-même est relancé par la règle de redémarrage du service.
//!
//! **Ce qui ne se fait jamais** :
//! - envoyer un texte qui ne vient pas de la fiche (aucune route ne porte `send_stdin_line`) ;
//! - envoyer quoi que ce soit à un service adopté, ou lancé par une unité du système (BR-JEU-001) ;
//! - dérouler une suite `untested` (BR-JEU-038) : `stop_plan` la rend « décision », pas « séquence » ;
//! - arrêter quoi que ce soit quand la confirmation manque : une DÉCISION est demandée (« Arrêter quand même » ou
//!   « Annuler », BR-JEU-006), sans réponse en 15 minutes l'opération passe à « annulé », le service reste en
//!   marche et le verrou est rendu ;
//! - renvoyer un ordre tout seul, ni reprendre une suite interrompue par un redémarrage de l'agent (BR-JEU-011) ;
//! - écrire dans une trace ou le journal le texte d'un ordre ou une ligne du jeu.
//!
//! Ce cas d'usage ne connaît ni le droit ni le verrou du service : l'opération est acceptée par
//! `ServiceLifecycle` (qui tient le verrou du début à la fin, BR-JEU-003) et lui rend la main à chaque moment où
//! quelque chose d'irréversible va avoir lieu ([`StopMoments`]).

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use time::OffsetDateTime;
use tokio::time::Instant;

use super::catalog::CatalogService;
use super::ports::{Clock, ContainerEngine, EngineError, LogQuery};
use super::service_ops::ServiceOps;
use super::service_specs::ServiceSpecs;
use crate::domain::audit::SaveMention;
use crate::domain::catalog::stop_plan::{
    DecisionReason, PlannedStep as FicheStep, Signal, StopPlan, stop_plan,
};
use crate::domain::ops::game_stop::{before_quit, quit_line, step};
use crate::domain::ops::{AskReason, Cause, Choice, GameStopKind, OpStatus, confirms};
use crate::domain::services::key::ServiceKey;

/// Les durées du déroulé. Les tests les raccourcissent : la fiche parle en secondes, `unit` dit combien dure une
/// « seconde de fiche ».
#[derive(Debug, Clone, Copy)]
pub struct GameStopTimings {
    /// La durée d'une seconde de fiche (attente, délai d'un signe).
    pub unit: Duration,
    /// L'intervalle entre deux lectures de l'opération pendant une attente (annulation, décision).
    pub poll: Duration,
}

impl GameStopTimings {
    pub const fn production() -> Self {
        Self {
            unit: Duration::from_secs(1),
            poll: Duration::from_secs(1),
        }
    }

    fn seconds(&self, seconds: u32) -> Duration {
        self.unit.saturating_mul(seconds)
    }
}

/// Ce que l'agent sait d'un service à son arrêt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameLookup {
    /// Pas un serveur de jeu créé depuis une fiche : l'arrêt est celui du moteur, tel qu'avant.
    NotAGame,
    /// Un serveur de jeu : comment s'arrête-t-il ?
    Game(GameStopKind),
}

/// Ce que l'arrêt demande de faire au cas d'usage de pilotage aux moments où quelque chose d'irréversible va avoir
/// lieu.
#[async_trait]
pub trait StopMoments: Send + Sync {
    /// Pourquoi l'ordre d'arrêt a été refusé, tel que l'appelant le type.
    type Refusal: Send;

    /// Juste avant l'ordre de sortie, ou l'arrêt du moteur : l'arrêt voulu est écrit AVANT l'ordre (BR-LIFE-010) et
    /// l'étape « Arrêt » est annoncée. `false` : impossible à écrire, aucun ordre n'est donné.
    async fn stop_wanted(&self) -> bool;

    /// L'arrêt voulu n'aura pas lieu (décision « Annuler », ordre refusé suivi d'un « Annuler », expiration) :
    /// l'arrêt voulu est oublié et l'étape retirée.
    async fn stop_dropped(&self);

    /// L'arrêt demandé au pilote du service, par le SEUL chemin d'ordre du cas d'usage de pilotage (délai du service,
    /// attente bornée, issue typée). `Err` : refusé ou sans réponse, avec l'issue que l'appelant juge.
    async fn order_stop(&self) -> Result<(), Self::Refusal>;
}

/// Ce qu'il faut pour arrêter UN service de jeu.
pub struct StopRequest<'a> {
    pub op_id: &'a str,
    pub container_id: &'a str,
    /// La suite déclarée par la fiche (jamais `ByImageOnSignal` : l'arrêt ordinaire n'en a pas besoin).
    pub plan: &'a StopPlan,
}

/// Comment l'arrêt s'est terminé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameStopEnd<R> {
    /// Le moteur a arrêté le conteneur.
    Stopped {
        mention: SaveMention,
        /// L'heure de l'arrêt demandé au moteur, juste après l'ordre de sortie et APRÈS toute décision attendue : de
        /// là se juge si l'arrêt a été forcé.
        ordered_at: OffsetDateTime,
        /// De cet instant à la fin de l'arrêt du moteur (une décision attendue avant n'y compte pas).
        took: Duration,
    },
    /// L'opération est déjà close (« Annuler », expiration, annulation du compte) : rien n'a été arrêté.
    Closed,
    /// Le journal des opérations ou l'arrêt voulu n'a pas pu être écrit : aucun ordre n'a été donné.
    NotWritten,
    /// L'ordre d'arrêt a été refusé ou est resté sans réponse ([`StopMoments::order_stop`] a rendu `Err`).
    StopRefused(R),
    /// Un ordre écrit sur l'entrée du service a échoué sans appel (le conteneur n'existe plus).
    Failed(EngineError),
}

/// Avant l'ordre de sortie.
enum Before {
    /// Les étapes ont été déroulées ; le signe a-t-il été vu partout où il était attendu ?
    Done {
        mention: SaveMention,
    },
    /// Une confirmation manque : décision.
    Ask(AskReason),
    Closed,
    NotWritten,
    Failed(EngineError),
}

/// Ce que le compte a répondu.
enum Asked {
    Continue,
    Closed,
    NotWritten,
}

/// Le signe a-t-il été vu ?
enum Sign {
    Seen,
    /// Le compte a annulé l'opération pendant l'attente (elle est close).
    Cancelled,
    Missing,
}

pub struct GameStop {
    specs: Arc<ServiceSpecs>,
    catalog: Arc<CatalogService>,
    engine: Arc<dyn ContainerEngine>,
    ops: Arc<ServiceOps>,
    clock: Arc<dyn Clock>,
    timings: GameStopTimings,
}

impl GameStop {
    pub fn new(
        specs: Arc<ServiceSpecs>,
        catalog: Arc<CatalogService>,
        engine: Arc<dyn ContainerEngine>,
        ops: Arc<ServiceOps>,
        clock: Arc<dyn Clock>,
        timings: GameStopTimings,
    ) -> Arc<Self> {
        Arc::new(Self {
            specs,
            catalog,
            engine,
            ops,
            clock,
            timings,
        })
    }

    // -----------------------------------------------------------------------------------------
    // Ce que la fiche dit
    // -----------------------------------------------------------------------------------------

    /// Comment ce service s'arrête, d'après la fiche d'où il vient. Un service sans description, ou créé sans fiche,
    /// n'est pas un serveur de jeu. Une description illisible, ou une fiche que cette construction ne connaît pas,
    /// ne permettent pas de savoir si le monde sera enregistré : c'est une DÉCISION, jamais un arrêt sans question
    /// (le choix le plus prudent pour le monde).
    pub async fn lookup(&self, key: &ServiceKey, grace_s: u32) -> GameLookup {
        let unknown = || {
            GameLookup::Game(GameStopKind::Steps(StopPlan::NeedsDecision {
                reason: DecisionReason::UntestedSequence,
                grace_s,
                signal: Signal::ImageDefault,
            }))
        };
        match self.specs.read(key, None).await {
            Ok(Some((revision, _))) => match revision.body.fiche {
                None => GameLookup::NotAGame,
                Some(from) => match self.catalog.fiche(&from.id) {
                    Some(fiche) => GameLookup::Game(GameStopKind::of(stop_plan(fiche))),
                    None => unknown(),
                },
            },
            Ok(None) => GameLookup::NotAGame,
            Err(error) => {
                tracing::warn!(%error, "description du service illisible : l'arrêt demandera une décision");
                unknown()
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Le déroulé
    // -----------------------------------------------------------------------------------------

    /// Déroule l'arrêt : les étapes d'avant l'ordre de sortie, la décision s'il manque une confirmation, l'ordre de
    /// sortie puis AUSSITÔT l'arrêt demandé au moteur.
    pub async fn run<M: StopMoments>(
        &self,
        request: &StopRequest<'_>,
        moments: &M,
    ) -> GameStopEnd<M::Refusal> {
        let (before, quit) = match request.plan {
            StopPlan::Sequence { steps, .. } => (
                before_quit(steps).cloned().collect::<Vec<_>>(),
                quit_line(steps),
            ),
            StopPlan::NeedsDecision { .. } | StopPlan::ByImageOnSignal { .. } => (Vec::new(), None),
        };
        // La suite n'existe pas, ou n'a pas été essayée : rien n'est envoyé, le compte tranche d'abord.
        let mut mention;
        let mut quit = quit;
        let first = if matches!(request.plan, StopPlan::Sequence { .. }) {
            self.before_quit(request, &before).await
        } else {
            Before::Ask(AskReason::NoUsableSequence)
        };
        match first {
            Before::Done { mention: reached } => mention = reached,
            Before::Ask(reason) => match self.ask(request).await {
                Asked::Continue => {
                    mention = reason.mention_if_stopped_anyway();
                    if !reason.may_still_quit() {
                        quit = None;
                    }
                }
                Asked::Closed => return GameStopEnd::Closed,
                Asked::NotWritten => return GameStopEnd::NotWritten,
            },
            Before::Closed => return GameStopEnd::Closed,
            Before::NotWritten => return GameStopEnd::NotWritten,
            Before::Failed(error) => return GameStopEnd::Failed(error),
        }

        // Le point de non-retour : l'arrêt voulu est écrit AVANT l'ordre de sortie.
        if !moments.stop_wanted().await {
            return GameStopEnd::NotWritten;
        }
        if let Some(line) = quit
            && let Err(error) = self
                .engine
                .send_stdin_line(request.container_id, line)
                .await
        {
            tracing::warn!(?error, "ordre de sortie non envoyé : décision demandée");
            // Rien n'a été arrêté : l'arrêt voulu est retiré le temps de la décision.
            moments.stop_dropped().await;
            if let EngineError::NotFound = error {
                return GameStopEnd::Failed(error);
            }
            match self.ask(request).await {
                Asked::Continue => {
                    mention = AskReason::OrderRefused.mention_if_stopped_anyway();
                    if !moments.stop_wanted().await {
                        return GameStopEnd::NotWritten;
                    }
                }
                Asked::Closed => return GameStopEnd::Closed,
                Asked::NotWritten => return GameStopEnd::NotWritten,
            }
        }
        // AUSSITÔT après l'ordre de sortie : le moteur sait que l'arrêt est voulu, le service ne se relance pas.
        // Les durées se mesurent d'ici : une décision attendue avant ne compte pas dans « l'arrêt a-t-il été forcé ».
        let started = Instant::now();
        let ordered_at = self.clock.now();
        match moments.order_stop().await {
            Ok(()) => GameStopEnd::Stopped {
                mention,
                ordered_at,
                took: started.elapsed(),
            },
            Err(refusal) => GameStopEnd::StopRefused(refusal),
        }
    }

    /// Les étapes d'avant l'ordre de sortie, dans l'ordre. Chaque étape est précédée d'un regard sur l'opération :
    /// une annulation du compte arrête la suite AVANT l'étape suivante.
    async fn before_quit(&self, request: &StopRequest<'_>, steps: &[FicheStep]) -> Before {
        if steps.is_empty() {
            return Before::Done {
                mention: SaveMention::Unconfirmed,
            };
        }
        let tty = match self.engine.inspect(request.container_id).await {
            Ok(Some(observation)) => observation.tty,
            Ok(None) => return Before::Failed(EngineError::NotFound),
            Err(error) => return Before::Failed(error),
        };
        if self
            .ops
            .intent(request.op_id, step::SAVE_WORLD, &Default::default())
            .await
            .is_err()
        {
            tracing::error!("journal de l'action illisible : aucun ordre n'est envoyé au jeu");
            return Before::NotWritten;
        }
        let mut since_ms = ms(self.clock.now());
        let mut orders = 0u32;
        let mut signs = 0u32;
        let mut missed = 0u32;
        for fiche_step in steps {
            if self.cancelled(request.op_id).await {
                return Before::Closed;
            }
            match fiche_step {
                FicheStep::StdinLine { line, .. } => {
                    since_ms = ms(self.clock.now());
                    if let Err(error) = self
                        .engine
                        .send_stdin_line(request.container_id, line)
                        .await
                    {
                        tracing::warn!(
                            ?error,
                            "ordre d'enregistrement non envoyé : décision demandée"
                        );
                        journal(self.ops.failed(request.op_id, step::SAVE_WORLD).await);
                        return Before::Ask(AskReason::OrderRefused);
                    }
                    orders += 1;
                }
                FicheStep::Wait { seconds } => {
                    if self
                        .sleep_watching(request.op_id, self.timings.seconds(*seconds))
                        .await
                    {
                        return Before::Closed;
                    }
                }
                FicheStep::WaitLog {
                    contains,
                    timeout_s,
                    required,
                } => {
                    let seen = self
                        .wait_for_sign(
                            request.op_id,
                            request.container_id,
                            tty,
                            contains,
                            since_ms,
                            self.timings.seconds(*timeout_s),
                        )
                        .await;
                    match seen {
                        Sign::Cancelled => return Before::Closed,
                        Sign::Seen => signs += 1,
                        Sign::Missing if *required => {
                            journal(self.ops.failed(request.op_id, step::SAVE_WORLD).await);
                            return Before::Ask(AskReason::SignNotSeen);
                        }
                        Sign::Missing => missed += 1,
                    }
                }
            }
        }
        journal(
            self.ops
                .done(request.op_id, step::SAVE_WORLD, &Default::default())
                .await,
        );
        // « Monde enregistré » seulement sur la preuve d'un signe vu, et d'aucun signe attendu et manqué.
        let mention = if orders > 0 && signs > 0 && missed == 0 {
            SaveMention::Saved
        } else {
            SaveMention::Unconfirmed
        };
        Before::Done { mention }
    }

    /// Cherche le signe, comme sous-chaîne littérale, dans les seules lignes écrites depuis `since_ms`, au plus
    /// `timeout`. Les journaux illisibles ou un conteneur qui s'arrête valent « signe non vu ».
    async fn wait_for_sign(
        &self,
        op_id: &str,
        container_id: &str,
        tty: bool,
        contains: &str,
        since_ms: i64,
        timeout: Duration,
    ) -> Sign {
        let mut lines = self.engine.logs(
            container_id,
            LogQuery {
                follow: true,
                tail: None,
                since_ms: Some(since_ms),
                tty,
            },
        );
        let deadline = Instant::now() + timeout;
        // Le compte peut annuler pendant l'attente : l'opération est relue à chaque pas, même si le jeu écrit sans cesse.
        let mut watch = tokio::time::interval(self.timings.poll);
        loop {
            tokio::select! {
                _ = watch.tick() => {
                    if self.cancelled(op_id).await {
                        return Sign::Cancelled;
                    }
                }
                item = lines.next() => match item {
                    Some(Ok(line)) => {
                        if confirms(&line, contains, since_ms) {
                            return Sign::Seen;
                        }
                    }
                    Some(Err(_)) | None => return Sign::Missing,
                },
                () = tokio::time::sleep_until(deadline) => return Sign::Missing,
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // La décision
    // -----------------------------------------------------------------------------------------

    /// Demande « Arrêter quand même » ou « Annuler » et attend la réponse. L'expiration (15 minutes, BR-JEU-006) est
    /// celle du balayage des opérations, la seule : sans réponse l'opération passe à « annulé » (`decision_expired`), le
    /// service reste en marche, le verrou est rendu, et cette attente le voit comme toute clôture.
    async fn ask(&self, request: &StopRequest<'_>) -> Asked {
        if let Err(error) = self
            .ops
            .ask_decision(request.op_id, &[Choice::Continue, Choice::Cancel])
            .await
        {
            tracing::error!(%error, "décision d'arrêt non demandée : rien n'est arrêté");
            return Asked::NotWritten;
        }
        let mut feed = self.ops.subscribe();
        loop {
            match self.ops.snapshot(request.op_id).await {
                Ok(Some(snapshot)) => match snapshot.record.status {
                    OpStatus::Running => return Asked::Continue,
                    OpStatus::AwaitingDecision => {}
                    OpStatus::Cancelling => {
                        journal(
                            self.ops
                                .finish(
                                    request.op_id,
                                    OpStatus::Cancelled,
                                    Some(Cause::CancelledByAccount),
                                )
                                .await,
                        );
                        return Asked::Closed;
                    }
                    // Annulée (par le compte ou par l'expiration du balayage), interrompue : close.
                    _ => return Asked::Closed,
                },
                Ok(None) => return Asked::Closed,
                Err(error) => {
                    tracing::error!(%error, "décision d'arrêt illisible : rien n'est arrêté");
                    return Asked::NotWritten;
                }
            }
            tokio::select! {
                _ = feed.recv() => {}
                () = tokio::time::sleep(self.timings.poll) => {}
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // L'annulation du compte
    // -----------------------------------------------------------------------------------------

    /// Le compte a-t-il annulé l'opération ? Alors elle est close ici (`cancelling` → `cancelled`) et la suite
    /// s'arrête avant l'étape suivante. Une opération illisible ne devient pas une annulation : la suite continue.
    async fn cancelled(&self, op_id: &str) -> bool {
        match self.ops.snapshot(op_id).await {
            Ok(Some(snapshot)) if snapshot.record.status == OpStatus::Cancelling => {
                journal(
                    self.ops
                        .finish(op_id, OpStatus::Cancelled, Some(Cause::CancelledByAccount))
                        .await,
                );
                true
            }
            _ => false,
        }
    }

    /// Attend `total` en regardant l'opération à chaque pas ; `true` si elle a été annulée.
    async fn sleep_watching(&self, op_id: &str, total: Duration) -> bool {
        let deadline = Instant::now() + total;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            tokio::time::sleep(left.min(self.timings.poll)).await;
            if self.cancelled(op_id).await {
                return true;
            }
        }
    }
}

/// L'heure en millisecondes depuis l'époque, comme celle des lignes de journaux du moteur.
fn ms(at: OffsetDateTime) -> i64 {
    i64::try_from(at.unix_timestamp_nanos() / 1_000_000).unwrap_or(i64::MAX)
}

/// Une écriture du journal qui échoue ne change pas ce que l'arrêt fait, mais elle se voit.
fn journal<T, E: std::fmt::Display>(result: Result<T, E>) {
    if let Err(error) = result {
        tracing::warn!(%error, "journal de l'arrêt du jeu non écrit");
    }
}
