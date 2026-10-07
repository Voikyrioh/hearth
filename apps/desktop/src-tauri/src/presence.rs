//! Présence du client hors de sa fenêtre, règles pures : aucune E/S, aucun type Tauri, le temps
//! est un paramètre.
//!
//! - [`NotificationGate`] : les notifications système du lien (passage « Hors ligne », retour
//!   « Connecté »), au plus UNE par minute et par serveur, agrégées
//!   (BR-RESIL-015, 018) ;
//! - [`LinkPresence`] : l'état affiché par l'icône de la zone de notification (BR-RESIL-016).
//!
//! Les images de l'icône ne sont pas une règle : elles sont dans `tray_icons.rs`.

use std::collections::HashMap;

use hearth_link::domain::state::LinkState;

/// Une notification système au plus par serveur pendant cette durée (millisecondes), toutes natures
/// confondues (BR-RESIL-015).
pub const NOTIFY_WINDOW_MS: u64 = 60_000;

/// Ce que la notification annonce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    /// Le serveur est « Hors ligne » (30 s sans lien).
    Offline,
    /// Le serveur est de nouveau « Connecté » après avoir été « Hors ligne ».
    Back,
    /// Une attaque probable vise l'identifiant (début d'un épisode d'alerte, BR-TRUST-009, 033).
    AttackProbable,
    /// Le mode attaque s'est arrêté tout seul (BR-TRUST-019, 033).
    AttackModeStopped,
}

impl AlertKind {
    /// Une notification de sécurité : elle passe avant celles du lien et son réglage est à part.
    pub fn is_security(self) -> bool {
        matches!(self, Self::AttackProbable | Self::AttackModeStopped)
    }
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
    /// Notifications de SÉCURITÉ retenues par la limite, dans l'ordre où elles partiront (au plus une
    /// par nature) : jamais remplacées par un changement d'état du lien (BR-TRUST-033).
    security_wanted: Vec<AlertKind>,
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
/// - les notifications de SÉCURITÉ (attaque probable, arrêt automatique du mode attaque) suivent la même
///   limite mais ont la PRIORITÉ : retenues, elles partent à l'échéance avant toute notification du
///   lien, que celle-ci reste retenue derrière ; un changement d'état du lien ne les remplace jamais ;
/// - seuls deux CHANGEMENTS d'état du lien notifient : « Hors ligne » et le retour « Connecté » qui le
///   suit. Une panne qui dure ne produit rien de plus, quelle que soit sa durée : le nombre de
///   notifications système est borné (une pour la panne, une au retour), indépendant du temps. Les
///   échecs de reconnexion répétés ne notifient JAMAIS le système : leur compteur vit dans les
///   notifications discrètes de l'application (BR-RESIL-018) ; la reconnexion, la session expirée et
///   l'accès révoqué ne notifient pas non plus.
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
        // Une notification de sécurité retenue passe d'abord : celle du lien attend derrière elle.
        if gate.window_open(now_ms) && gate.security_wanted.is_empty() {
            return Some(send(server, gate, kind, now_ms, false));
        }
        gate.suppressed = gate.suppressed.saturating_add(1);
        gate.wanted = Some(kind);
        None
    }

    /// Une nature de sécurité à annoncer pour ce serveur (début d'un épisode d'alerte, arrêt
    /// automatique du mode attaque). Partie tout de suite si la fenêtre est ouverte, sinon retenue
    /// (une fois par nature) et annoncée par [`NotificationGate::poll`] à l'échéance.
    pub fn push_security(&mut self, server: &str, kind: AlertKind, now_ms: u64) -> Option<Alert> {
        debug_assert!(kind.is_security());
        let gate = self.servers.entry(server.to_owned()).or_default();
        if gate.security_wanted.is_empty() && gate.window_open(now_ms) {
            gate.last_sent_at = Some(now_ms);
            return Some(Alert {
                server: server.to_owned(),
                kind,
                suppressed: 0,
            });
        }
        if !gate.security_wanted.contains(&kind) {
            gate.security_wanted.push(kind);
            // L'attaque probable avant l'arrêt du mode : c'est l'urgence.
            gate.security_wanted
                .sort_by_key(|kind| u8::from(*kind != AlertKind::AttackProbable));
        }
        None
    }

    /// Une notification de sécurité retenue n'a plus lieu d'être (l'épisode d'alerte est fini avant
    /// qu'elle parte) : elle est oubliée.
    pub fn drop_security(&mut self, server: &str, kind: AlertKind) {
        if let Some(gate) = self.servers.get_mut(server) {
            gate.security_wanted.retain(|wanted| *wanted != kind);
        }
    }

    /// Le réglage des notifications du lien est coupé : ce qui était retenu pour le lien est oublié,
    /// la sécurité reste (son réglage est à part).
    pub fn clear_link(&mut self) {
        for gate in self.servers.values_mut() {
            gate.offline_seen = false;
            gate.last_kind = None;
            gate.wanted = None;
            gate.suppressed = 0;
        }
    }

    /// Le réglage « Alertes de sécurité » est coupé : ce qui était retenu est oublié.
    pub fn clear_security(&mut self) {
        for gate in self.servers.values_mut() {
            gate.security_wanted.clear();
        }
    }

    /// Les notifications retenues dont l'échéance est venue (à appeler régulièrement).
    pub fn poll(&mut self, now_ms: u64) -> Vec<Alert> {
        let mut due = Vec::new();
        for (server, gate) in &mut self.servers {
            // La sécurité d'abord : tant qu'elle attend, rien du lien ne passe devant elle.
            if !gate.security_wanted.is_empty() {
                if gate.window_open(now_ms) {
                    let kind = gate.security_wanted.remove(0);
                    gate.last_sent_at = Some(now_ms);
                    due.push(Alert {
                        server: server.clone(),
                        kind,
                        suppressed: 0,
                    });
                }
                continue;
            }
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
    alert
}

/// Le mode attaque tel que la détection des épisodes le lit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeKind {
    Off,
    Active,
    Suspended,
}

/// Qui est visé et ce que le compte peut en faire : le texte de la notification d'attaque probable
/// (conception design, écran D).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecurityWording {
    /// Le titulaire, qui peut activer le mode attaque depuis ce poste.
    Owner { username: String },
    /// Le titulaire qui ne peut pas l'activer d'ici (Lecture seule, poste sans clé inscrite).
    Details { username: String },
    /// Un administrateur dont l'identifiant n'est pas visé mais d'autres comptes le sont (le nombre,
    /// jamais un nom).
    Others { count: u32 },
}

/// Ce que l'état de sécurité d'un serveur dit, réduit à ce qui décide d'une notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityNotice {
    pub wording: SecurityWording,
    /// Une alerte est visible pour ce compte : son identifiant est visé, ou (administrateur) d'autres.
    pub alert_visible: bool,
    pub mode: ModeKind,
    /// Le dernier mode attaque s'est terminé tout seul (`last_end: auto`).
    pub ended_automatically: bool,
}

/// Ce qui vient de changer pour un serveur.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SecurityChanges {
    /// Un épisode d'alerte commence : UNE notification (BR-TRUST-009).
    pub alert_started: bool,
    /// L'épisode est fini.
    pub alert_ended: bool,
    /// Le mode attaque était actif ou suspendu et s'est arrêté tout seul (BR-TRUST-019).
    pub mode_stopped_by_agent: bool,
}

#[derive(Debug, Default)]
struct Watched {
    alert_on: bool,
    mode_on: bool,
}

/// Détecte les ÉPISODES : un épisode d'alerte ne notifie qu'une fois, quel que soit le nombre de
/// messages `security` qui le répètent ; l'arrêt automatique du mode n'est annoncé que si on l'a vu
/// actif juste avant (jamais pour un mode arrêté avant que l'application ne l'ait connu). Pur.
#[derive(Debug, Default)]
pub struct SecurityWatch {
    servers: HashMap<String, Watched>,
}

impl SecurityWatch {
    pub fn observe(&mut self, server: &str, notice: &SecurityNotice) -> SecurityChanges {
        let watched = self.servers.entry(server.to_owned()).or_default();
        let mode_on = notice.mode != ModeKind::Off;
        let changes = SecurityChanges {
            alert_started: notice.alert_visible && !watched.alert_on,
            alert_ended: !notice.alert_visible && watched.alert_on,
            mode_stopped_by_agent: watched.mode_on && !mode_on && notice.ended_automatically,
        };
        watched.alert_on = notice.alert_visible;
        watched.mode_on = mode_on;
        changes
    }

    pub fn forget(&mut self, server: &str) {
        self.servers.remove(server);
    }

    /// Nombre de serveurs suivis (tests de borne).
    #[doc(hidden)]
    pub fn tracked(&self) -> usize {
        self.servers.len()
    }
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

/// L'image de l'icône de la zone de notification : une par état du lien, plus « aucun serveur »
/// (BR-RESIL-016). `TrayStatus` garde la gravité, celle-ci dit LAQUELLE des cinq images montrer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayIcon {
    /// Aucun serveur : l'âtre seul, en trait sourd.
    NoServer,
    Connected,
    Reconnecting,
    Offline,
    SessionExpired,
    AccessRevoked,
}

impl TrayIcon {
    pub fn of(state: LinkState) -> Self {
        match state {
            LinkState::Connected => Self::Connected,
            LinkState::Reconnecting => Self::Reconnecting,
            LinkState::Offline => Self::Offline,
            LinkState::SessionExpired => Self::SessionExpired,
            LinkState::AccessRevoked => Self::AccessRevoked,
        }
    }
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

    /// L'image de l'icône (BR-RESIL-016) : celle de l'état du serveur reflété.
    pub fn icon(&self) -> TrayIcon {
        self.reflected()
            .map_or(TrayIcon::NoServer, |(_, state)| TrayIcon::of(state))
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
