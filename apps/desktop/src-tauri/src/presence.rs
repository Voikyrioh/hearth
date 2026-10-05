//! Présence du client hors de sa fenêtre, règles pures : aucune E/S, aucun type Tauri, le temps
//! est un paramètre.
//!
//! - [`NotificationGate`] : les notifications système du lien (passage « Hors ligne », retour
//!   « Connecté », paliers d'échecs de reconnexion), au plus UNE par minute et par serveur, agrégées
//!   (BR-RESIL-015, 018) ;
//! - [`LinkPresence`] : l'état affiché par l'icône de la zone de notification (BR-RESIL-016).
//!
//! Le dessin de la pastille de l'icône n'est pas une règle : il est dans `badge.rs`.

use std::collections::HashMap;

use hearth_link::domain::state::LinkState;

/// Une notification système au plus par serveur pendant cette durée (millisecondes), toutes natures
/// confondues (BR-RESIL-015).
pub const NOTIFY_WINDOW_MS: u64 = 60_000;

/// Palier d'échecs de reconnexion : une notification (dans l'application) et au plus une
/// notification système par palier franchi, jamais une par tentative (BR-RESIL-018).
pub const FAILURE_STEP: u32 = 5;

/// Ce que la notification annonce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    /// Le serveur est « Hors ligne » (30 s sans lien).
    Offline,
    /// Le serveur est de nouveau « Connecté » après avoir été « Hors ligne ».
    Back,
    /// Le serveur est toujours « Hors ligne » et un palier d'échecs de reconnexion est franchi
    /// (nombre d'échecs du palier : 5, 10, 15…).
    Failures(u32),
}

/// Une notification à montrer. `suppressed` : combien de changements d'état ont été absorbés par la
/// limite en plus de celui-ci (0 = aucune agrégation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    pub server: String,
    pub kind: AlertKind,
    pub suppressed: u32,
}

#[derive(Debug, Default)]
struct ServerGate {
    /// Le serveur a été vu « Hors ligne » et n'a pas encore été annoncé « de retour ».
    offline_seen: bool,
    /// Date de la dernière notification partie (toutes natures).
    last_sent_at: Option<u64>,
    /// Nature de la dernière notification partie : ce que l'utilisateur sait déjà.
    last_kind: Option<AlertKind>,
    /// Ce qu'il faudrait annoncer maintenant mais que la limite retient.
    wanted: Option<AlertKind>,
    /// Changements d'état absorbés depuis la dernière notification partie.
    suppressed: u32,
    /// Palier d'échecs déjà couvert par une notification partie.
    last_step: u32,
    /// Palier d'échecs du dernier état « Hors ligne » vu.
    step_now: u32,
}

impl ServerGate {
    fn window_open(&self, now_ms: u64) -> bool {
        self.last_sent_at
            .is_none_or(|at| now_ms.saturating_sub(at) >= NOTIFY_WINDOW_MS)
    }
}

/// Limiteur et agrégateur des notifications système (BR-RESIL-015, 018).
///
/// Règles :
/// - une notification par minute et par serveur au plus, toutes natures confondues ;
/// - une situation que l'utilisateur connaît déjà n'est jamais répétée (hors ligne, un essai de
///   reconnexion qui échoue, de nouveau hors ligne : une seule alerte « hors ligne ») ;
/// - une notification que la limite retient n'est pas perdue : seule la DERNIÈRE situation compte,
///   [`NotificationGate::poll`] l'annonce à l'échéance, une fois, avec le nombre de changements
///   absorbés en plus ;
/// - « Hors ligne » et le retour « Connecté » qui le suit notifient ; tant que le serveur reste
///   « Hors ligne », chaque palier de [`FAILURE_STEP`] échecs franchi en fait une de plus ; la
///   reconnexion, la session expirée et l'accès révoqué n'en font pas.
///
/// La mémoire est bornée par le nombre de serveurs (`forget` à la suppression d'un serveur).
#[derive(Debug, Default)]
pub struct NotificationGate {
    servers: HashMap<String, ServerGate>,
}

impl NotificationGate {
    /// Un serveur est passé dans `state` à l'instant `now_ms`, avec `failed_attempts` échecs de
    /// reconnexion consécutifs.
    pub fn observe(
        &mut self,
        server: &str,
        state: LinkState,
        failed_attempts: u32,
        now_ms: u64,
    ) -> Option<Alert> {
        let gate = self.servers.entry(server.to_owned()).or_default();
        let kind = match state {
            LinkState::Offline => {
                gate.offline_seen = true;
                let step = failed_attempts / FAILURE_STEP;
                gate.step_now = step;
                let announced = matches!(
                    gate.last_kind,
                    Some(AlertKind::Offline | AlertKind::Failures(_))
                );
                if !announced {
                    AlertKind::Offline
                } else if step > gate.last_step {
                    AlertKind::Failures(step * FAILURE_STEP)
                } else {
                    // Même situation que celle déjà annoncée : ce qui était retenu est périmé.
                    if gate.wanted.take().is_some() {
                        gate.suppressed = gate.suppressed.saturating_add(1);
                    }
                    return None;
                }
            }
            LinkState::Connected if gate.offline_seen => {
                gate.offline_seen = false;
                gate.last_step = 0;
                gate.step_now = 0;
                AlertKind::Back
            }
            LinkState::Connected => {
                gate.last_step = 0;
                gate.step_now = 0;
                return None;
            }
            // Reconnexion, session expirée, accès révoqué : on ne parle pas, et ce qui était
            // retenu reste ce qu'il est (la prochaine situation tranchera).
            _ => return None,
        };
        if gate.last_kind == Some(kind) {
            // L'utilisateur sait déjà : rien à dire. Ce qui était retenu est périmé : c'était un
            // changement absorbé ; la même situation réannoncée n'en est pas un.
            if gate.wanted.take().is_some() {
                gate.suppressed = gate.suppressed.saturating_add(1);
            }
            return None;
        }
        if gate.wanted == Some(kind) {
            // Déjà retenu tel quel : la même situation réannoncée n'est pas un changement.
            return None;
        }
        if gate.window_open(now_ms) {
            return Some(send(server, gate, kind, now_ms, false));
        }
        gate.suppressed = gate.suppressed.saturating_add(1);
        gate.wanted = Some(kind);
        None
    }

    /// Les notifications retenues dont l'échéance est venue (à appeler régulièrement).
    pub fn poll(&mut self, now_ms: u64) -> Vec<Alert> {
        let mut due = Vec::new();
        for (server, gate) in &mut self.servers {
            let Some(kind) = gate.wanted else { continue };
            if gate.last_kind == Some(kind) {
                gate.wanted = None;
            } else if gate.window_open(now_ms) {
                due.push(send(server, gate, kind, now_ms, true));
            }
        }
        due.sort_by(|a, b| a.server.cmp(&b.server));
        due
    }

    /// Le serveur est retiré du carnet : on l'oublie (mémoire bornée).
    pub fn forget(&mut self, server: &str) {
        self.servers.remove(server);
    }

    /// Nombre de serveurs suivis (tests de borne).
    #[doc(hidden)]
    pub fn tracked(&self) -> usize {
        self.servers.len()
    }
}

fn send(
    server: &str,
    gate: &mut ServerGate,
    kind: AlertKind,
    now_ms: u64,
    retained: bool,
) -> Alert {
    let alert = Alert {
        server: server.to_owned(),
        kind,
        // Une notification retenue EST le dernier changement : seuls les autres sont « absorbés ».
        suppressed: if retained {
            gate.suppressed.saturating_sub(1)
        } else {
            gate.suppressed
        },
    };
    gate.last_sent_at = Some(now_ms);
    gate.last_kind = Some(kind);
    gate.wanted = None;
    gate.suppressed = 0;
    gate.last_step = match kind {
        AlertKind::Failures(n) => n / FAILURE_STEP,
        AlertKind::Offline => gate.step_now,
        AlertKind::Back => 0,
    };
    alert
}

/// Ce que montre l'icône de la zone de notification (BR-RESIL-016).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayStatus {
    /// Aucun serveur : l'icône reste neutre, sans pastille.
    Idle,
    /// Vert : « Connecté ».
    Connected,
    /// Orange : « Reconnexion… » ou « Session expirée ».
    Warning,
    /// Rouge : « Hors ligne » ou « Accès révoqué ».
    Critical,
}

impl TrayStatus {
    pub fn of(state: LinkState) -> Self {
        match state {
            LinkState::Connected => Self::Connected,
            LinkState::Reconnecting | LinkState::SessionExpired => Self::Warning,
            LinkState::Offline | LinkState::AccessRevoked => Self::Critical,
        }
    }

    /// Rang de gravité (pire état).
    fn severity(self) -> u8 {
        match self {
            Self::Idle => 0,
            Self::Connected => 1,
            Self::Warning => 2,
            Self::Critical => 3,
        }
    }
}

/// Les états des liens connus de la coquille et le serveur affiché dans la fenêtre.
#[derive(Debug, Default)]
pub struct LinkPresence {
    displayed: Option<String>,
    states: HashMap<String, LinkState>,
}

impl LinkPresence {
    pub fn set_state(&mut self, server: &str, state: LinkState) {
        self.states.insert(server.to_owned(), state);
    }

    pub fn remove(&mut self, server: &str) {
        self.states.remove(server);
        if self.displayed.as_deref() == Some(server) {
            self.displayed = None;
        }
    }

    /// Le serveur affiché dans la fenêtre a changé (`None` : aucun, par exemple la page des réglages).
    pub fn set_displayed(&mut self, server: Option<&str>) {
        self.displayed = server.map(str::to_owned);
    }

    pub fn displayed(&self) -> Option<&str> {
        self.displayed.as_deref()
    }

    /// Le serveur que l'icône reflète et son état : le serveur affiché dans la fenêtre ; sans
    /// serveur affiché, le plus mauvais état de tous (pour ne jamais cacher une panne derrière la
    /// page des réglages, à gravité égale le premier par ordre alphabétique) ; sans serveur, rien.
    /// Un serveur affiché dont aucun état n'est encore connu compte comme « Reconnexion… ».
    pub fn reflected(&self) -> Option<(&str, LinkState)> {
        if let Some(server) = &self.displayed {
            let state = self
                .states
                .get(server)
                .copied()
                .unwrap_or(LinkState::Reconnecting);
            return Some((server.as_str(), state));
        }
        self.states
            .iter()
            .map(|(server, state)| (server.as_str(), *state))
            .max_by(|(a_id, a), (b_id, b)| {
                TrayStatus::of(*a)
                    .severity()
                    .cmp(&TrayStatus::of(*b).severity())
                    .then_with(|| b_id.cmp(a_id))
            })
    }

    /// L'état de l'icône (BR-RESIL-016).
    pub fn status(&self) -> TrayStatus {
        self.reflected()
            .map_or(TrayStatus::Idle, |(_, state)| TrayStatus::of(state))
    }

    /// Nombre d'états suivis (tests de borne).
    #[doc(hidden)]
    pub fn tracked(&self) -> usize {
        self.states.len()
    }
}
