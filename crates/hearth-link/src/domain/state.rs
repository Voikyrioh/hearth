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
//! # Redémarrage annoncé (mise à jour de l'agent, BR-UPDATE-014)
//! Quand l'agent annonce `restart` sur le flux de mise à jour ([`Input::RestartAnnounced`]), la
//! coupure qui suit est ATTENDUE : pendant [`Thresholds::restart_window`] (2 minutes : les 60 s de
//! contrôle du nouvel agent, un éventuel retour arrière, une marge), le lien s'affiche
//! « Reconnexion en cours » dès la coupure, ne passe pas à « Hors ligne », et ne compte pas
//! d'échec de reconnexion (donc ni notification système de panne, ni avis « reconnexion échouée »).
//! Passé ce délai sans retour de l'agent, la coupure est une panne comme une autre. Le retour du
//! lien ([`Input::Connected`]) ou la fin de la mise à jour ([`Input::RestartEnded`]) lève l'attente.
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
/// Durée d'une coupure annoncée par l'agent (redémarrage pour mise à jour) : 60 s de contrôle du
/// nouvel agent, le retour arrière éventuel (arrêt, remise de l'ancien binaire, démarrage), une marge.
pub const RESTART_WINDOW: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thresholds {
    pub silence: Duration,
    pub reconnecting_after: Duration,
    pub offline_after: Duration,
    /// Durée pendant laquelle une coupure annoncée par l'agent (redémarrage de mise à jour) reste
    /// « Reconnexion en cours » (BR-UPDATE-014).
    pub restart_window: Duration,
    /// Diviseur des délais de reconnexion : 1 en production, plus pour les tests rapides.
    pub backoff_divisor: u32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            silence: SILENCE,
            reconnecting_after: RECONNECTING_AFTER,
            offline_after: OFFLINE_AFTER,
            restart_window: RESTART_WINDOW,
            backoff_divisor: 1,
        }
    }
}

impl Thresholds {
    /// Tous les seuils et délais divisés par `divisor` : les mêmes scénarios, `divisor` fois plus
    /// vite (tests de résilience).
    pub fn scaled(divisor: u32) -> Self {
        let divisor = divisor.max(1);
        Self {
            silence: SILENCE / divisor,
            reconnecting_after: RECONNECTING_AFTER / divisor,
            offline_after: OFFLINE_AFTER / divisor,
            restart_window: RESTART_WINDOW / divisor,
            backoff_divisor: divisor,
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

/// Pourquoi un état d'arrêt (`SessionExpired`, `AccessRevoked`) est affiché : l'interface choisit
/// son message et son action selon la raison, les cinq états restent les mêmes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// Aucune session : serveur tout juste ajouté, en attente d'une première connexion.
    NoSession,
    /// La session a expiré et aucun mot de passe n'est mémorisé.
    Expired,
    /// Le mot de passe mémorisé est refusé (changé côté serveur, compte supprimé) : l'interface
    /// rouvre le formulaire de connexion, identifiant prérempli (BR-CONN-017).
    StoredPasswordRefused,
    /// L'utilisateur s'est déconnecté : aucune reconnexion automatique (BR-CONN-016).
    UserDisconnected,
    /// Session fermée par l'administration (changement de mot de passe, révocation,
    /// suppression du compte : l'agent répond `SESSION_REVOKED` dans tous les cas).
    Revoked,
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
    /// 401 `SESSION_REVOKED`.
    AccessRevoked,
    /// Le mot de passe mémorisé est refusé à la reconnexion silencieuse (`INVALID_CREDENTIALS`).
    StoredPasswordRefused,
    /// Le serveur demande d'attendre (`429`, `503` avec `retry_after_s`) : la prochaine tentative
    /// n'a pas lieu avant ce délai, ni avant le délai habituel s'il est plus long.
    RetryAfter(Duration),
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
    /// L'agent a annoncé `restart` sur le flux de mise à jour : sa coupure est attendue
    /// (BR-UPDATE-014).
    RestartAnnounced,
    /// La mise à jour est terminée (étape `done`) : plus de coupure attendue.
    RestartEnded,
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
    /// Vérifier le lien tout de suite (ping immédiat) : le réseau a changé, le flux est peut-être
    /// sain, peut-être mort.
    PingNow,
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
    /// Raison de l'état d'arrêt affiché (`SessionExpired`, `AccessRevoked`).
    pub reason: Option<Reason>,
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
    /// L'utilisateur s'était déconnecté : pas de reconnexion automatique au démarrage.
    Disconnected,
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
    /// La reconnexion silencieuse vient de réussir et le flux n'est pas encore rouvert : une
    /// nouvelle « session expirée » à ce stade n'est pas relancée sur-le-champ (boucle serrée
    /// contre un serveur qui refuserait toujours), elle suit les délais.
    reauthed: bool,
    /// Un réveil a déjà repoussé « Hors ligne » dans cette coupure : un seul report tant qu'aucun
    /// contact n'a réussi (un poste qui se réveille toutes les 20 s finit « Hors ligne »).
    woken: bool,
}

#[derive(Debug, Clone, Copy)]
enum Phase {
    Up {
        last_traffic: Mono,
        /// Vérification demandée (changement de réseau) : sans message d'ici là, le lien est
        /// réputé coupé.
        check_by: Option<Mono>,
    },
    Down(Outage),
    Expired(Reason),
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
    /// Jusqu'à quand une coupure est attendue (redémarrage annoncé par l'agent).
    restart_until: Option<Mono>,
    /// La coupure attendue est en cours à l'instant du dernier calcul de l'état affiché.
    restart_active: bool,
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
                    reauthed: false,
                    woken: false,
                }),
                LinkState::Reconnecting,
            ),
            Start::SignedOut => (Phase::Expired(Reason::NoSession), LinkState::SessionExpired),
            Start::Disconnected => (
                Phase::Expired(Reason::UserDisconnected),
                LinkState::SessionExpired,
            ),
            Start::Recovered => (
                Phase::Down(Outage {
                    since: now.before(thresholds.offline_after),
                    unproven: true,
                    manual: false,
                    step: Step::Reconnect,
                    in_flight: false,
                    next_attempt_at: Some(now),
                    reauthed: false,
                    woken: false,
                }),
                LinkState::Offline,
            ),
        };
        Self {
            thresholds,
            backoff: Backoff::scaled(thresholds.backoff_divisor),
            jitter,
            phase,
            shown,
            shown_since: now,
            last_contact: None,
            restart_until: None,
            restart_active: false,
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
            reason: match self.phase {
                Phase::Expired(reason) => Some(reason),
                Phase::Revoked => Some(Reason::Revoked),
                _ => None,
            },
            since: self.shown_since,
            last_contact: self.last_contact,
            next_retry_at,
            // Une coupure annoncée ne compte aucun échec : ni avis « reconnexion échouée », ni
            // notification de panne (BR-UPDATE-014).
            failed_attempts: if self.restart_active {
                0
            } else {
                self.backoff.failures()
            },
        }
    }

    pub fn state(&self) -> LinkState {
        self.shown
    }

    /// Prochain instant où appeler [`Input::Tick`], s'il y en a un. Chaque échéance rendue est
    /// consommée par le `Tick` correspondant : pas de boucle active.
    pub fn deadline(&self) -> Option<Mono> {
        match self.phase {
            Phase::Up {
                last_traffic,
                check_by,
            } => {
                let silence = last_traffic.after(self.thresholds.silence);
                Some(check_by.map_or(silence, |at| at.min(silence)))
            }
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
                    offer(self.offline_at(&outage));
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
            Input::StoredPasswordRefused => {
                if matches!(self.phase, Phase::Up { .. } | Phase::Down(_)) {
                    self.stop_trying(
                        now,
                        Phase::Expired(Reason::StoredPasswordRefused),
                        &mut effects,
                    );
                }
            }
            Input::RetryAfter(delay) => self.on_retry_after(now, delay),
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
                self.stop_trying(now, Phase::Expired(Reason::UserDisconnected), &mut effects);
            }
            Input::RestartAnnounced => {
                if matches!(self.phase, Phase::Up { .. } | Phase::Down(_)) {
                    self.restart_until = Some(now.after(self.thresholds.restart_window));
                }
            }
            Input::RestartEnded => self.restart_until = None,
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
            Phase::Up {
                last_traffic,
                check_by,
            } if now.since(last_traffic) >= self.thresholds.silence
                || check_by.is_some_and(|at| at <= now) =>
            {
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
        if let Phase::Up {
            last_traffic,
            check_by,
        } = &mut self.phase
        {
            *last_traffic = (*last_traffic).max(now);
            // Le lien a répondu : la vérification est faite.
            *check_by = None;
            self.last_contact = Some(now);
        }
    }

    fn on_connected(&mut self, now: Mono, effects: &mut Vec<Effect>) {
        if matches!(self.phase, Phase::Down(_)) {
            // Le nouvel agent répond : la coupure attendue est finie.
            self.restart_until = None;
            self.phase = Phase::Up {
                last_traffic: now,
                check_by: None,
            };
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
            self.stop_trying(now, Phase::Expired(Reason::Expired), effects);
            return;
        }
        if let Phase::Down(mut outage) = self.phase
            && outage.reauthed
        {
            // La session toute neuve est déjà refusée : on ne boucle pas, on suit les délais.
            let delay = self.backoff.next_delay((self.jitter)());
            outage.step = Step::Reauth;
            outage.in_flight = false;
            outage.manual = false;
            outage.reauthed = false;
            outage.next_attempt_at = Some(now.after(delay));
            self.phase = Phase::Down(outage);
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
        // Un réveil déjà compté dans cette coupure le reste : la session expirée ne le remet pas
        // à zéro (sinon un second réveil repousserait encore « Hors ligne »).
        let woken = matches!(self.phase, Phase::Down(outage) if outage.woken);
        self.phase = Phase::Down(Outage {
            since,
            unproven,
            manual: false,
            step: Step::Reauth,
            in_flight: true,
            next_attempt_at: None,
            reauthed: false,
            woken,
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
            outage.reauthed = true;
            self.phase = Phase::Down(outage);
            effects.push(Effect::StartAttempt);
        }
    }

    fn on_trigger(&mut self, now: Mono, trigger: Trigger, effects: &mut Vec<Effect>) {
        match (self.phase, trigger) {
            // Réveil du PC : la connexion est morte, la coupure compte à partir du réveil (le
            // réseau n'est peut-être pas encore prêt : « Reconnexion en cours », pas « Hors
            // ligne » parce que le PC a dormi).
            (Phase::Up { .. }, Trigger::Woke) => {
                self.lose(now, effects);
                self.mark_unproven();
            }
            // Réseau changé : le flux est peut-être sain. On ne le coupe pas ; on le vérifie vite.
            (Phase::Up { last_traffic, .. }, Trigger::NetworkChanged) => {
                let check_by = now.after(self.thresholds.silence / 3);
                self.phase = Phase::Up {
                    last_traffic,
                    check_by: Some(check_by),
                };
                effects.push(Effect::PingNow);
            }
            (Phase::Up { .. }, Trigger::RetryNow) => {}
            (Phase::Down(mut outage), trigger) => {
                outage.manual = outage.manual || self.shown == LinkState::Offline;
                if trigger == Trigger::Woke && !outage.woken {
                    // L'horloge de coupure repart du réveil, une seule fois par coupure.
                    outage.since = now;
                    outage.unproven = true;
                    outage.manual = false;
                    outage.woken = true;
                    self.backoff.reset();
                }
                outage.in_flight = true;
                outage.next_attempt_at = None;
                effects.push(step_effect(outage.step));
                self.phase = Phase::Down(outage);
            }
            // Un blocage se lève par une nouvelle tentative explicite seulement.
            (Phase::Blocked(_), Trigger::RetryNow) => self.start_fresh(now, effects),
            _ => {}
        }
    }

    /// Affiche « Reconnexion en cours » dès le début de la coupure en cours.
    fn mark_unproven(&mut self) {
        if let Phase::Down(mut outage) = self.phase {
            outage.unproven = true;
            outage.woken = true;
            self.phase = Phase::Down(outage);
        }
    }

    /// Le serveur demande d'attendre : pas de tentative avant ce délai.
    fn on_retry_after(&mut self, now: Mono, delay: Duration) {
        if let Phase::Down(mut outage) = self.phase
            && outage.in_flight
        {
            let usual = self.backoff.next_delay((self.jitter)());
            outage.in_flight = false;
            outage.manual = false;
            outage.next_attempt_at = Some(now.after(usual.max(delay)));
            self.phase = Phase::Down(outage);
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
            reauthed: false,
            woken: false,
        });
        effects.push(Effect::StartAttempt);
    }

    /// Début d'une coupure à `since` : première tentative tout de suite.
    fn lose(&mut self, since: Mono, effects: &mut Vec<Effect>) {
        self.backoff.reset();
        // Une coupure commencée pendant la fenêtre d'un redémarrage annoncé reste « Reconnexion en
        // cours » jusqu'à son terme, même si la fenêtre expire dans les 3 secondes qui suivent
        // (sinon l'état repasserait à « Connecté » avant de redevenir « Reconnexion »).
        let announced = self.restart_until.is_some_and(|until| since < until);
        self.phase = Phase::Down(Outage {
            since,
            unproven: announced,
            manual: false,
            step: Step::Reconnect,
            in_flight: true,
            next_attempt_at: None,
            reauthed: false,
            woken: false,
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
            Phase::Expired(_) | Phase::Revoked | Phase::Blocked(_)
        );
        // Seule la déconnexion volontaire change la raison d'un état d'arrêt.
        if from_stopped && !matches!(phase, Phase::Expired(Reason::UserDisconnected)) {
            return;
        }
        self.phase = phase;
        self.restart_until = None;
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
            Phase::Expired(_) => (LinkState::SessionExpired, now),
            Phase::Revoked => (LinkState::AccessRevoked, now),
            Phase::Blocked(_) => (LinkState::Offline, now),
            Phase::Stopped => return,
        };
        self.restart_active = self.restart_until.is_some_and(|until| now < until)
            && matches!(self.phase, Phase::Down(_));
        if derived != self.shown {
            self.shown = derived;
            self.shown_since = at.min(now);
        }
    }

    /// Instant où une coupure devient « Hors ligne » : le seuil, repoussé jusqu'à la fin d'une
    /// coupure attendue (redémarrage annoncé par l'agent).
    fn offline_at(&self, outage: &Outage) -> Mono {
        let at = outage.since.after(self.thresholds.offline_after);
        self.restart_until.map_or(at, |until| at.max(until))
    }

    /// État affiché pendant une coupure, et l'instant exact où il l'est devenu.
    fn derive_down(&self, outage: Outage, now: Mono) -> (LinkState, Mono) {
        let elapsed = now.since(outage.since);
        let offline_at = self.offline_at(&outage);
        let reconnecting_at = outage.since.after(self.thresholds.reconnecting_after);
        let expected = self.restart_until.is_some_and(|until| now < until);
        if now >= offline_at && !outage.manual {
            (LinkState::Offline, offline_at)
        } else if outage.manual || outage.unproven || expected {
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
