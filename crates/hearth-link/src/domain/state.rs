//! Machine à états du lien avec un serveur (ADR-0007, BR-RESIL-002 à 006, 012 à 014, 020).
//!
//! Pure : aucune E/S, aucune horloge (chaque appel reçoit l'instant monotone), aucun hasard
//! propre (la source d'aléa est injectée). Elle reçoit des [`Input`] (événements) et rend des
//! [`Effect`] (ce que l'appelant doit faire : lancer une tentative, fermer le flux…). L'état
//! affiché se lit par [`LinkMachine::status`].
//!
//! # Seuils (mesurés depuis le début de la coupure)
//! - moins de 3 s : l'état affiché ne change pas (« Connecté ») ;
//! - de 3 s à 30 s : `Reconnecting` ;
//! - 30 s et plus : `Offline`, les tentatives continuent sans fin.
//!
//! Le début de la coupure est l'instant de la dernière réception (silence de 3 s) ou de l'erreur
//! de transport, selon ce qui la révèle. Les seuils sont des paramètres ([`Thresholds`]) : les
//! tests de résilience les réduisent pour rester rapides ; les valeurs par défaut sont celles de
//! la spec.
//!
//! # Tentatives
//! La première tentative suit immédiatement la perte ; les suivantes sont espacées par
//! [`Backoff`] (0,5 s, 1 s, 2 s, 4 s, 8 s, 15 s, 30 s, 30 s…). Un déclencheur (« Réessayer
//! maintenant », réveil, changement de réseau) lance une tentative tout de suite.

use std::time::Duration;

use hearth_proto::error::UpgradeTarget;

use super::backoff::Backoff;
use super::time::Mono;

/// Silence au-delà duquel le lien est réputé coupé.
pub const SILENCE: Duration = Duration::from_secs(3);
/// Coupure à partir de laquelle l'état affiché passe à « Reconnexion en cours ».
pub const RECONNECTING_AFTER: Duration = Duration::from_secs(3);
/// Coupure à partir de laquelle l'état affiché passe à « Hors ligne ».
pub const OFFLINE_AFTER: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thresholds {
    pub silence: Duration,
    pub reconnecting_after: Duration,
    pub offline_after: Duration,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            silence: SILENCE,
            reconnecting_after: RECONNECTING_AFTER,
            offline_after: OFFLINE_AFTER,
        }
    }
}

/// L'état du lien tel que l'interface l'affiche.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkState {
    Connected,
    Reconnecting,
    Offline,
    SessionExpired,
    AccessRevoked,
}

/// Pourquoi les tentatives sont arrêtées alors que l'état affiché est `Offline`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocked {
    /// L'empreinte du serveur a changé (BR-CONN-003) : à l'utilisateur de trancher.
    FingerprintChanged,
    /// Versions d'interface incompatibles (BR-CONN-014).
    IncompatibleVersion(UpgradeTarget),
}

/// Événements qui pilotent la machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// Une tentative a réussi : flux ouvert, authentifié, instantané reçu.
    Connected,
    /// Un message est arrivé du serveur (battement compris).
    Traffic,
    /// Le temps a passé (réveil à l'échéance donnée par [`LinkMachine::deadline`]).
    Tick,
    /// Erreur de socket, fermeture, ou tentative échouée.
    TransportFailed,
    /// 401 `SESSION_EXPIRED` (ou fin de session annoncée par le flux). `can_reauth` : le mot de
    /// passe est mémorisé au coffre, on peut se reconnecter en silence.
    SessionExpired { can_reauth: bool },
    /// La reconnexion silencieuse a réussi : on rouvre le flux.
    Reauthenticated,
    /// 401 `SESSION_REVOKED` ou `INVALID_CREDENTIALS`.
    AccessRevoked,
    /// Le certificat présenté n'a pas l'empreinte mémorisée.
    FingerprintChanged,
    /// 426 : versions d'interface incompatibles.
    Incompatible(UpgradeTarget),
    /// « Réessayer maintenant ».
    RetryNow,
    /// Réveil du PC.
    Woke,
    /// La liste des adresses réseau locales a changé.
    NetworkChanged,
    /// L'utilisateur s'est (re)connecté avec succès (mot de passe saisi, autre compte…).
    LoginSucceeded,
    /// Le mot de passe saisi est refusé.
    LoginRefused,
    /// L'utilisateur s'est déconnecté.
    LoggedOut,
    /// Arrêt de l'application.
    Shutdown,
}

/// Ce que l'appelant doit faire. Les effets d'un même appel s'exécutent dans l'ordre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Lancer une tentative de connexion (en abandonnant celle qui serait en cours).
    StartAttempt,
    /// Se reconnecter en silence avec le mot de passe du coffre.
    Reauthenticate,
    /// Abandonner la tentative en cours.
    AbortAttempt,
    /// Fermer le flux ouvert.
    CloseStream,
    /// Les actions en vol deviennent « résultat inconnu » (jamais rejouées).
    MarkPendingUnknown,
    /// Le lien est revenu : demander à l'agent le sort des opérations en suspens.
    ResolvePending,
    /// Fin : plus aucun événement ne sera pris en compte.
    Stop,
}

/// État affiché et informations qui l'accompagnent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    pub state: LinkState,
    /// Tentatives arrêtées (empreinte changée, versions incompatibles) alors que `state` est
    /// `Offline`.
    pub blocked: Option<Blocked>,
    /// Depuis quand cet état est affiché.
    pub since: Mono,
    /// Dernier message reçu du serveur.
    pub last_contact: Option<Mono>,
    /// Prochaine tentative planifiée (aucune si une est en cours ou si les tentatives sont
    /// arrêtées).
    pub next_retry_at: Option<Mono>,
    /// Tentatives échouées depuis le dernier succès.
    pub failed_attempts: u32,
}

/// Comment la machine démarre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// Une session est mémorisée : on se connecte (affiché « Reconnexion en cours »).
    Connecting,
    /// Aucune session : en attente d'une connexion de l'utilisateur.
    SignedOut,
    /// Reprise après un incident interne (tâche relancée) : affiché « Hors ligne », on retente.
    Recovered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Reconnect,
    Reauth,
}

#[derive(Debug, Clone, Copy)]
struct Outage {
    /// Début de la coupure.
    since: Mono,
    /// Aucun contact encore sur cette série de tentatives (démarrage, connexion) : on affiche
    /// « Reconnexion en cours » tout de suite plutôt que de faire semblant d'être connecté.
    unproven: bool,
    /// Tentative lancée à la main ou par un déclencheur depuis « Hors ligne » : affichée
    /// « Reconnexion en cours » jusqu'à son échec.
    manual: bool,
    step: Step,
    in_flight: bool,
    next_attempt_at: Option<Mono>,
}

#[derive(Debug, Clone, Copy)]
enum Phase {
    Up { last_traffic: Mono },
    Down(Outage),
    Expired,
    Revoked,
    Blocked(Blocked),
    Stopped,
}

pub struct LinkMachine {
    thresholds: Thresholds,
    backoff: Backoff,
    jitter: Box<dyn FnMut() -> u32 + Send>,
    phase: Phase,
    shown: LinkState,
    shown_since: Mono,
    last_contact: Option<Mono>,
}

impl std::fmt::Debug for LinkMachine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinkMachine")
            .field("phase", &self.phase)
            .field("shown", &self.shown)
            .finish_non_exhaustive()
    }
}

impl LinkMachine {
    /// `jitter` fournit les entiers aléatoires de l'aléa des délais.
    pub fn new(
        thresholds: Thresholds,
        jitter: Box<dyn FnMut() -> u32 + Send>,
        now: Mono,
        start: Start,
    ) -> Self {
        let (phase, shown) = match start {
            Start::Connecting => (
                Phase::Down(Outage {
                    since: now,
                    unproven: true,
                    manual: false,
                    step: Step::Reconnect,
                    in_flight: false,
                    next_attempt_at: Some(now),
                }),
                LinkState::Reconnecting,
            ),
            Start::SignedOut => (Phase::Expired, LinkState::SessionExpired),
            Start::Recovered => (
                Phase::Down(Outage {
                    since: now.before(thresholds.offline_after),
                    unproven: true,
                    manual: false,
                    step: Step::Reconnect,
                    in_flight: false,
                    next_attempt_at: Some(now),
                }),
                LinkState::Offline,
            ),
        };
        Self {
            thresholds,
            backoff: Backoff::new(),
            jitter,
            phase,
            shown,
            shown_since: now,
            last_contact: None,
        }
    }

    pub fn status(&self) -> Status {
        let next_retry_at = match self.phase {
            Phase::Down(outage) if !outage.in_flight => outage.next_attempt_at,
            _ => None,
        };
        Status {
            state: self.shown,
            blocked: match self.phase {
                Phase::Blocked(reason) => Some(reason),
                _ => None,
            },
            since: self.shown_since,
            last_contact: self.last_contact,
            next_retry_at,
            failed_attempts: self.backoff.failures(),
        }
    }

    pub fn state(&self) -> LinkState {
        self.shown
    }

    /// Prochain instant où appeler [`Input::Tick`], s'il y en a un. Chaque échéance rendue est
    /// consommée par le `Tick` correspondant : pas de boucle active.
    pub fn deadline(&self) -> Option<Mono> {
        match self.phase {
            Phase::Up { last_traffic } => Some(last_traffic.after(self.thresholds.silence)),
            Phase::Down(outage) => {
                let mut next: Option<Mono> = None;
                let mut offer = |at: Mono| next = Some(next.map_or(at, |n| n.min(at)));
                if !outage.in_flight
                    && let Some(at) = outage.next_attempt_at
                {
                    offer(at);
                }
                if self.shown == LinkState::Connected {
                    offer(outage.since.after(self.thresholds.reconnecting_after));
                }
                if self.shown != LinkState::Offline && !outage.manual {
                    offer(outage.since.after(self.thresholds.offline_after));
                }
                next
            }
            _ => None,
        }
    }

    /// Applique un événement ; rend les effets à exécuter, dans l'ordre.
    pub fn handle(&mut self, now: Mono, input: Input) -> Vec<Effect> {
        let mut effects = Vec::new();
        if matches!(self.phase, Phase::Stopped) {
            return effects;
        }
        match input {
            Input::Tick => self.on_tick(now, &mut effects),
            Input::Traffic => self.on_traffic(now),
            Input::Connected => self.on_connected(now, &mut effects),
            Input::TransportFailed => self.on_transport_failed(now, &mut effects),
            Input::SessionExpired { can_reauth } => {
                self.on_session_expired(now, can_reauth, &mut effects);
            }
            Input::Reauthenticated => self.on_reauthenticated(&mut effects),
            Input::AccessRevoked => {
                self.stop_trying(now, Phase::Revoked, &mut effects);
            }
            Input::FingerprintChanged => {
                self.stop_trying(
                    now,
                    Phase::Blocked(Blocked::FingerprintChanged),
                    &mut effects,
                );
            }
            Input::Incompatible(target) => {
                self.stop_trying(
                    now,
                    Phase::Blocked(Blocked::IncompatibleVersion(target)),
                    &mut effects,
                );
            }
            Input::RetryNow => self.on_trigger(now, Trigger::RetryNow, &mut effects),
            Input::Woke => self.on_trigger(now, Trigger::Woke, &mut effects),
            Input::NetworkChanged => self.on_trigger(now, Trigger::NetworkChanged, &mut effects),
            Input::LoginSucceeded => self.on_login_succeeded(now, &mut effects),
            Input::LoginRefused => {}
            Input::LoggedOut => {
                self.stop_trying(now, Phase::Expired, &mut effects);
            }
            Input::Shutdown => {
                self.phase = Phase::Stopped;
                effects.extend([Effect::AbortAttempt, Effect::CloseStream, Effect::Stop]);
            }
        }
        self.refresh(now);
        effects
    }

    fn on_tick(&mut self, now: Mono, effects: &mut Vec<Effect>) {
        match self.phase {
            // Silence : la coupure a commencé au dernier message reçu.
            Phase::Up { last_traffic } if now.since(last_traffic) >= self.thresholds.silence => {
                self.lose(last_traffic, effects);
            }
            Phase::Down(mut outage)
                if !outage.in_flight && outage.next_attempt_at.is_some_and(|at| at <= now) =>
            {
                outage.in_flight = true;
                outage.next_attempt_at = None;
                effects.push(step_effect(outage.step));
                self.phase = Phase::Down(outage);
            }
            _ => {}
        }
    }

    fn on_traffic(&mut self, now: Mono) {
        if let Phase::Up { last_traffic } = &mut self.phase {
            *last_traffic = (*last_traffic).max(now);
            self.last_contact = Some(now);
        }
    }

    fn on_connected(&mut self, now: Mono, effects: &mut Vec<Effect>) {
        if matches!(self.phase, Phase::Down(_)) {
            self.phase = Phase::Up { last_traffic: now };
            self.last_contact = Some(now);
            self.backoff.reset();
            effects.push(Effect::ResolvePending);
        }
    }

    fn on_transport_failed(&mut self, now: Mono, effects: &mut Vec<Effect>) {
        match self.phase {
            Phase::Up { .. } => self.lose(now, effects),
            Phase::Down(mut outage) if outage.in_flight => {
                let delay = self.backoff.next_delay((self.jitter)());
                outage.in_flight = false;
                outage.manual = false;
                outage.next_attempt_at = Some(now.after(delay));
                self.phase = Phase::Down(outage);
            }
            _ => {}
        }
    }

    fn on_session_expired(&mut self, now: Mono, can_reauth: bool, effects: &mut Vec<Effect>) {
        if !matches!(self.phase, Phase::Up { .. } | Phase::Down(_)) {
            return;
        }
        if !can_reauth {
            self.stop_trying(now, Phase::Expired, effects);
            return;
        }
        // Reconnexion silencieuse : une coupure comme une autre, qui commence maintenant.
        let since = match self.phase {
            Phase::Down(outage) => outage.since,
            _ => {
                effects.extend([Effect::CloseStream, Effect::MarkPendingUnknown]);
                now
            }
        };
        let unproven = matches!(self.phase, Phase::Down(outage) if outage.unproven);
        self.phase = Phase::Down(Outage {
            since,
            unproven,
            manual: false,
            step: Step::Reauth,
            in_flight: true,
            next_attempt_at: None,
        });
        effects.push(Effect::Reauthenticate);
    }

    fn on_reauthenticated(&mut self, effects: &mut Vec<Effect>) {
        if let Phase::Down(mut outage) = self.phase
            && outage.step == Step::Reauth
        {
            outage.step = Step::Reconnect;
            outage.in_flight = true;
            outage.next_attempt_at = None;
            self.phase = Phase::Down(outage);
            effects.push(Effect::StartAttempt);
        }
    }

    fn on_trigger(&mut self, now: Mono, trigger: Trigger, effects: &mut Vec<Effect>) {
        match self.phase {
            Phase::Up { last_traffic } => match trigger {
                // Le PC s'est réveillé : la coupure a commencé à la dernière réception.
                Trigger::Woke => self.lose(last_traffic, effects),
                // Le réseau a changé : la connexion est peut-être morte, on repart proprement.
                Trigger::NetworkChanged => self.lose(now, effects),
                Trigger::RetryNow => {}
            },
            Phase::Down(mut outage) => {
                outage.manual = outage.manual || self.shown == LinkState::Offline;
                outage.in_flight = true;
                outage.next_attempt_at = None;
                effects.push(step_effect(outage.step));
                self.phase = Phase::Down(outage);
            }
            // Un blocage se lève par une nouvelle tentative explicite seulement.
            Phase::Blocked(_) if trigger == Trigger::RetryNow => {
                self.start_fresh(now, effects);
            }
            _ => {}
        }
    }

    fn on_login_succeeded(&mut self, now: Mono, effects: &mut Vec<Effect>) {
        if matches!(self.phase, Phase::Stopped) {
            return;
        }
        effects.extend([Effect::AbortAttempt, Effect::CloseStream]);
        self.start_fresh(now, effects);
    }

    /// Nouvelle série de tentatives, affichée « Reconnexion en cours » dès le début.
    fn start_fresh(&mut self, now: Mono, effects: &mut Vec<Effect>) {
        self.backoff.reset();
        self.phase = Phase::Down(Outage {
            since: now,
            unproven: true,
            manual: false,
            step: Step::Reconnect,
            in_flight: true,
            next_attempt_at: None,
        });
        effects.push(Effect::StartAttempt);
    }

    /// Début d'une coupure à `since` : première tentative tout de suite.
    fn lose(&mut self, since: Mono, effects: &mut Vec<Effect>) {
        self.backoff.reset();
        self.phase = Phase::Down(Outage {
            since,
            unproven: false,
            manual: false,
            step: Step::Reconnect,
            in_flight: true,
            next_attempt_at: None,
        });
        effects.extend([
            Effect::CloseStream,
            Effect::MarkPendingUnknown,
            Effect::StartAttempt,
        ]);
    }

    /// Plus de tentatives : session expirée, accès révoqué, blocage ou déconnexion.
    fn stop_trying(&mut self, _now: Mono, phase: Phase, effects: &mut Vec<Effect>) {
        if matches!(self.phase, Phase::Stopped) {
            return;
        }
        // Une session déjà expirée ou révoquée ne bascule pas d'un état d'arrêt à l'autre sans
        // passer par une connexion de l'utilisateur, sauf déconnexion explicite.
        let from_stopped = matches!(
            self.phase,
            Phase::Expired | Phase::Revoked | Phase::Blocked(_)
        );
        if from_stopped && !matches!(phase, Phase::Expired) {
            return;
        }
        self.phase = phase;
        effects.extend([
            Effect::AbortAttempt,
            Effect::CloseStream,
            Effect::MarkPendingUnknown,
        ]);
    }

    /// Recalcule l'état affiché depuis la phase et l'instant.
    fn refresh(&mut self, now: Mono) {
        let (derived, at) = match self.phase {
            Phase::Up { .. } => (LinkState::Connected, now),
            Phase::Down(outage) => self.derive_down(outage, now),
            Phase::Expired => (LinkState::SessionExpired, now),
            Phase::Revoked => (LinkState::AccessRevoked, now),
            Phase::Blocked(_) => (LinkState::Offline, now),
            Phase::Stopped => return,
        };
        if derived != self.shown {
            self.shown = derived;
            self.shown_since = at.min(now);
        }
    }

    /// État affiché pendant une coupure, et l'instant exact où il l'est devenu.
    fn derive_down(&self, outage: Outage, now: Mono) -> (LinkState, Mono) {
        let elapsed = now.since(outage.since);
        let offline_at = outage.since.after(self.thresholds.offline_after);
        let reconnecting_at = outage.since.after(self.thresholds.reconnecting_after);
        if elapsed >= self.thresholds.offline_after && !outage.manual {
            (LinkState::Offline, offline_at)
        } else if outage.manual || outage.unproven {
            (LinkState::Reconnecting, now)
        } else if elapsed >= self.thresholds.reconnecting_after {
            (LinkState::Reconnecting, reconnecting_at)
        } else {
            (LinkState::Connected, now)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Trigger {
    RetryNow,
    Woke,
    NetworkChanged,
}

fn step_effect(step: Step) -> Effect {
    match step {
        Step::Reconnect => Effect::StartAttempt,
        Step::Reauth => Effect::Reauthenticate,
    }
}

#[cfg(test)]
mod tests;
