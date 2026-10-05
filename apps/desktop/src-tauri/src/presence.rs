//! Présence du client hors de sa fenêtre, règles pures : aucune E/S, aucun type Tauri, le temps
//! est un paramètre.
//!
//! - [`NotificationGate`] : les notifications système du lien (passage « Hors ligne », retour
//!   « Connecté »), au plus une par minute et par serveur, agrégées (BR-RESIL-015) ;
//! - [`LinkPresence`] : l'état affiché par l'icône de la zone de notification (BR-RESIL-016) ;
//! - [`paint_badge`] : la pastille de couleur posée sur l'icône.

use std::collections::HashMap;

use hearth_link::domain::state::LinkState;

/// Une notification système au plus par serveur pendant cette durée (millisecondes).
pub const NOTIFY_WINDOW_MS: u64 = 60_000;

/// Ce que la notification annonce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    /// Le serveur est « Hors ligne » (30 s sans lien).
    Offline,
    /// Le serveur est de nouveau « Connecté » après avoir été « Hors ligne ».
    Back,
}

/// Une notification à montrer. `suppressed` : combien de changements d'état ont été absorbés
/// depuis la précédente (0 = aucune agrégation).
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
    /// Date de la dernière notification partie de chaque nature (`[hors ligne, de retour]`).
    last_sent_at: [Option<u64>; 2],
    /// Nature de la dernière notification partie : ce que l'utilisateur sait déjà.
    last_kind: Option<AlertKind>,
    /// Ce qu'il faudrait annoncer maintenant mais que la limite retient.
    wanted: Option<AlertKind>,
    /// Changements d'état absorbés depuis la dernière notification partie.
    suppressed: u32,
}

impl ServerGate {
    fn window_open(&self, kind: AlertKind, now_ms: u64) -> bool {
        self.last_sent_at[kind.index()]
            .is_none_or(|at| now_ms.saturating_sub(at) >= NOTIFY_WINDOW_MS)
    }
}

impl AlertKind {
    fn index(self) -> usize {
        match self {
            Self::Offline => 0,
            Self::Back => 1,
        }
    }
}

/// Limiteur et agrégateur des notifications système (BR-RESIL-015, 018).
///
/// Règles :
/// - « Hors ligne » et « de retour » ont chacun leur limite : au plus une notification de chaque
///   nature par minute et par serveur ;
/// - une situation que l'utilisateur connaît déjà n'est jamais répétée (hors ligne, un essai de
///   reconnexion qui échoue, de nouveau hors ligne : une seule alerte « hors ligne ») ;
/// - une notification que la limite retient n'est pas perdue : seule la DERNIÈRE situation compte,
///   [`NotificationGate::poll`] l'annonce à l'échéance, une fois, avec le nombre de changements
///   absorbés ;
/// - seuls « Hors ligne » et le retour « Connecté » qui le suit font une notification : la
///   reconnexion, la session expirée et l'accès révoqué n'en font pas.
///
/// La mémoire est bornée par le nombre de serveurs (`forget` à la suppression d'un serveur).
#[derive(Debug, Default)]
pub struct NotificationGate {
    servers: HashMap<String, ServerGate>,
}

impl NotificationGate {
    /// Un serveur est passé dans `state` à l'instant `now_ms`.
    pub fn observe(&mut self, server: &str, state: LinkState, now_ms: u64) -> Option<Alert> {
        let gate = self.servers.entry(server.to_owned()).or_default();
        let kind = match state {
            LinkState::Offline => {
                gate.offline_seen = true;
                AlertKind::Offline
            }
            LinkState::Connected if gate.offline_seen => {
                gate.offline_seen = false;
                AlertKind::Back
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
        if gate.window_open(kind, now_ms) {
            return Some(send(server, gate, kind, now_ms));
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
            } else if gate.window_open(kind, now_ms) {
                due.push(send(server, gate, kind, now_ms));
            }
        }
        due.sort_by(|a, b| a.server.cmp(&b.server));
        due
    }

    /// Plus aucune échéance en attente (le minuteur de la coquille peut s'arrêter).
    pub fn is_idle(&self) -> bool {
        self.servers.values().all(|gate| gate.wanted.is_none())
    }

    /// Le serveur est retiré du carnet : on l'oublie (mémoire bornée).
    pub fn forget(&mut self, server: &str) {
        self.servers.remove(server);
    }

    /// Nombre de serveurs suivis (tests de borne).
    pub fn tracked(&self) -> usize {
        self.servers.len()
    }
}

fn send(server: &str, gate: &mut ServerGate, kind: AlertKind, now_ms: u64) -> Alert {
    let alert = Alert {
        server: server.to_owned(),
        kind,
        suppressed: gate.suppressed,
    };
    gate.last_sent_at[kind.index()] = Some(now_ms);
    gate.last_kind = Some(kind);
    gate.wanted = None;
    gate.suppressed = 0;
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

    /// Couleur de la pastille (jetons `--ok`, `--warn`, `--crit` de `tokens.css`) ; `None` : pas de pastille.
    pub fn color(self) -> Option<[u8; 3]> {
        match self {
            Self::Idle => None,
            Self::Connected => Some([0x7e, 0xd3, 0x9a]),
            Self::Warning => Some([0xff, 0xc0, 0x4d]),
            Self::Critical => Some([0xff, 0x5a, 0x5a]),
        }
    }

    /// Le texte de l'infobulle de l'icône est dans `texts` ; ici le rang de gravité (pire état).
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

    pub fn tracked(&self) -> usize {
        self.states.len()
    }
}

/// Pose une pastille pleine de la couleur de `status` dans le coin bas droit d'une image RGBA
/// (`width * height * 4` octets). Rend l'image inchangée si `status` n'a pas de pastille ou si la
/// taille ne correspond pas. Un fin liseré sombre la détache de la flamme.
pub fn paint_badge(rgba: &[u8], width: u32, height: u32, status: TrayStatus) -> Vec<u8> {
    let mut out = rgba.to_vec();
    let Some(color) = status.color() else {
        return out;
    };
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || out.len() != w * h * 4 {
        return out;
    }
    // Rayon : un peu plus d'un quart de la largeur, centre au coin bas droit moins le rayon.
    let radius = (w.min(h) as f32) * 0.26;
    let cx = w as f32 - radius - 0.5;
    let cy = h as f32 - radius - 0.5;
    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let pixel = (y * w + x) * 4;
            if dist <= radius - 1.5 {
                out[pixel..pixel + 4].copy_from_slice(&[color[0], color[1], color[2], 0xff]);
            } else if dist <= radius {
                // Liseré sombre (jeton `--bg`).
                out[pixel..pixel + 4].copy_from_slice(&[0x1c, 0x15, 0x18, 0xff]);
            }
        }
    }
    out
}
