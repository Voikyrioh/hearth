//! Présence du client hors de sa fenêtre : notifications système du lien (BR-RESIL-015) et icône de
//! la zone de notification (BR-RESIL-016). Les règles sont dans [`crate::presence`] ; ici, la colle :
//! l'état tenu, les ports vers le système (notification, icône), l'horloge.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use hearth_link::domain::state::LinkState;

use crate::link::StateObserver;
use crate::presence::{
    Alert, AlertKind, LinkPresence, NotificationGate, SecurityNotice, SecurityWatch,
    SecurityWording, TrayIcon, TrayStatus,
};
use crate::texts;

/// Affiche une notification système.
pub trait Notifier: Send + Sync {
    fn notify(&self, title: &str, body: &str);
}

/// Montre l'état dans la zone de notification (image de l'icône et infobulle).
pub trait TrayPort: Send + Sync {
    fn show(&self, status: TrayStatus, tooltip: &str);

    /// L'image de l'icône, appelée avant `show` à chaque changement (HRT-19). Sans implémentation
    /// par défaut : un adaptateur ou un double qui l'oublie ne compile pas.
    fn show_icon(&self, icon: TrayIcon);
}

struct Inner {
    gate: NotificationGate,
    presence: LinkPresence,
    /// Nom de chaque serveur (pour le texte des notifications), borné par le carnet.
    names: HashMap<String, String>,
    /// Ce que l'icône montre déjà : on ne la redessine que si ça change.
    shown: Option<(TrayStatus, TrayIcon, String)>,
    /// Les épisodes de sécurité vus (une notification par épisode d'alerte).
    watch: SecurityWatch,
    /// Le texte de l'alerte du moment, par serveur (qui est visé, ce que le compte peut en faire).
    wording: HashMap<String, SecurityWording>,
}

pub struct Alerts {
    inner: Mutex<Inner>,
    enabled: AtomicBool,
    /// Réglage « Alertes de sécurité » : séparé de `enabled`, activé par défaut (BR-TRUST-033).
    security_enabled: AtomicBool,
    notifier: Arc<dyn Notifier>,
    tray: Arc<dyn TrayPort>,
    /// Millisecondes écoulées depuis un instant de référence (l'horloge est injectée).
    now_ms: Box<dyn Fn() -> u64 + Send + Sync>,
}

impl Alerts {
    pub fn new(
        notifier: Arc<dyn Notifier>,
        tray: Arc<dyn TrayPort>,
        enabled: bool,
        now_ms: impl Fn() -> u64 + Send + Sync + 'static,
    ) -> Self {
        Self {
            inner: Mutex::new(Inner {
                gate: NotificationGate::default(),
                presence: LinkPresence::default(),
                names: HashMap::new(),
                shown: None,
                watch: SecurityWatch::default(),
                wording: HashMap::new(),
            }),
            enabled: AtomicBool::new(enabled),
            security_enabled: AtomicBool::new(true),
            notifier,
            tray,
            now_ms: Box::new(now_ms),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// Réglage « Notifier quand un serveur devient hors ligne ou revient ». Désactivé : ce qui
    /// était retenu est oublié (rien ne part plus tard).
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
        if !enabled {
            // Ce qui était retenu pour le lien seulement : la sécurité a son propre réglage.
            self.lock().gate.clear_link();
        }
    }

    pub fn is_security_enabled(&self) -> bool {
        self.security_enabled.load(Ordering::SeqCst)
    }

    /// Réglage « Alertes de sécurité » (BR-TRUST-033), séparé de celui du lien. Désactivé : ce qui
    /// était retenu est oublié. Les bandeaux de l'interface ne dépendent JAMAIS de ce réglage : la
    /// sécurité ne se masque pas.
    pub fn set_security_enabled(&self, enabled: bool) {
        self.security_enabled.store(enabled, Ordering::SeqCst);
        if !enabled {
            self.lock().gate.clear_security();
        }
    }

    /// Le serveur affiché dans la fenêtre change (`None` : aucun) : l'icône le suit.
    pub fn set_displayed(&self, server: Option<&str>) {
        let tray = {
            let mut inner = self.lock();
            inner.presence.set_displayed(server);
            Self::refresh_tray(&mut inner)
        };
        self.apply_tray(tray);
    }

    /// Notifications retenues dont l'échéance est venue (appelé régulièrement).
    pub fn tick(&self) {
        // Ce qui est retenu a été oublié quand son réglage a été coupé : rien ne part en trop.
        let now = (self.now_ms)();
        let due = {
            let mut inner = self.lock();
            let due = inner.gate.poll(now);
            due.into_iter()
                .map(|alert| Self::message_of(&inner, alert))
                .collect::<Vec<_>>()
        };
        for (title, body) in due {
            self.notifier.notify(&title, &body);
        }
    }

    /// Titre et corps d'une notification : ceux du lien, ou ceux de la sécurité.
    fn message_of(inner: &Inner, alert: Alert) -> (String, String) {
        let name = inner
            .names
            .get(&alert.server)
            .map_or(alert.server.as_str(), String::as_str);
        if alert.kind.is_security() {
            return (
                texts::security_title(name),
                texts::security_alert_body(alert.kind, inner.wording.get(&alert.server)),
            );
        }
        (
            texts::APP_NAME.to_owned(),
            texts::link_alert_body(name, alert.kind, alert.suppressed),
        )
    }

    fn refresh_tray(inner: &mut Inner) -> Option<(TrayStatus, TrayIcon, String)> {
        let status = inner.presence.status();
        let icon = inner.presence.icon();
        let reflected = inner
            .presence
            .reflected()
            .map(|(id, state)| (inner.names.get(id).map_or(id, String::as_str), state));
        let tooltip = texts::tray_tooltip(reflected);
        let next = (status, icon, tooltip);
        if inner.shown.as_ref() == Some(&next) {
            return None;
        }
        inner.shown = Some(next.clone());
        Some(next)
    }

    fn apply_tray(&self, tray: Option<(TrayStatus, TrayIcon, String)>) {
        if let Some((status, icon, tooltip)) = tray {
            self.tray.show_icon(icon);
            self.tray.show(status, &tooltip);
        }
    }
}

impl StateObserver for Alerts {
    fn on_state(&self, server: &str, name: &str, state: LinkState, _failed_attempts: u32) {
        let now = (self.now_ms)();
        let (message, tray) = {
            let mut inner = self.lock();
            inner.names.insert(server.to_owned(), name.to_owned());
            inner.presence.set_state(server, state);
            let message = if self.is_enabled() {
                inner
                    .gate
                    .observe(server, state, now)
                    .map(|alert| Self::message_of(&inner, alert))
            } else {
                None
            };
            (message, Self::refresh_tray(&mut inner))
        };
        self.apply_tray(tray);
        if let Some((title, body)) = message {
            self.notifier.notify(&title, &body);
        }
    }

    fn on_security(&self, server: &str, name: &str, notice: &SecurityNotice) {
        let now = (self.now_ms)();
        let message = {
            let mut inner = self.lock();
            inner.names.insert(server.to_owned(), name.to_owned());
            inner
                .wording
                .insert(server.to_owned(), notice.wording.clone());
            // Les épisodes sont suivis même réglage coupé : le réactiver ne rejoue pas un épisode en
            // cours.
            let changes = inner.watch.observe(server, notice);
            if changes.alert_ended {
                inner.gate.drop_security(server, AlertKind::AttackProbable);
            }
            if !self.is_security_enabled() {
                return;
            }
            let mut sent = None;
            for kind in [
                (changes.alert_started, AlertKind::AttackProbable),
                (changes.mode_stopped_by_agent, AlertKind::AttackModeStopped),
            ]
            .into_iter()
            .filter_map(|(happened, kind)| happened.then_some(kind))
            {
                if let Some(alert) = inner.gate.push_security(server, kind, now) {
                    sent = Some(Self::message_of(&inner, alert));
                }
            }
            sent
        };
        if let Some((title, body)) = message {
            self.notifier.notify(&title, &body);
        }
    }

    fn on_removed(&self, server: &str) {
        let tray = {
            let mut inner = self.lock();
            inner.gate.forget(server);
            inner.watch.forget(server);
            inner.wording.remove(server);
            inner.presence.remove(server);
            inner.names.remove(server);
            Self::refresh_tray(&mut inner)
        };
        self.apply_tray(tray);
    }
}
