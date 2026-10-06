//! Présence du client hors de sa fenêtre : notifications système du lien (BR-RESIL-015) et icône de
//! la zone de notification (BR-RESIL-016). Les règles sont dans [`crate::presence`] ; ici, la colle :
//! l'état tenu, les ports vers le système (notification, icône), l'horloge.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use hearth_link::domain::state::LinkState;

use crate::link::StateObserver;
use crate::presence::{Alert, LinkPresence, NotificationGate, TrayStatus};
use crate::texts;

/// Affiche une notification système.
pub trait Notifier: Send + Sync {
    fn notify(&self, title: &str, body: &str);
}

/// Montre l'état dans la zone de notification (couleur de la pastille et infobulle).
pub trait TrayPort: Send + Sync {
    fn show(&self, status: TrayStatus, tooltip: &str);
}

struct Inner {
    gate: NotificationGate,
    presence: LinkPresence,
    /// Nom de chaque serveur (pour le texte des notifications), borné par le carnet.
    names: HashMap<String, String>,
    /// Ce que l'icône montre déjà : on ne la redessine que si ça change.
    shown: Option<(TrayStatus, String)>,
}

pub struct Alerts {
    inner: Mutex<Inner>,
    enabled: AtomicBool,
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
            }),
            enabled: AtomicBool::new(enabled),
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
            self.lock().gate = NotificationGate::default();
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
        if !self.is_enabled() {
            return;
        }
        let now = (self.now_ms)();
        let due = {
            let mut inner = self.lock();
            let due = inner.gate.poll(now);
            due.into_iter()
                .map(|alert| Self::text_of(&inner, alert))
                .collect::<Vec<_>>()
        };
        for body in due {
            self.notifier.notify(texts::APP_NAME, &body);
        }
    }

    fn text_of(inner: &Inner, alert: Alert) -> String {
        let name = inner
            .names
            .get(&alert.server)
            .map_or(alert.server.as_str(), String::as_str);
        texts::link_alert_body(name, alert.kind, alert.suppressed)
    }

    fn refresh_tray(inner: &mut Inner) -> Option<(TrayStatus, String)> {
        let status = inner.presence.status();
        let reflected = inner
            .presence
            .reflected()
            .map(|(id, state)| (inner.names.get(id).map_or(id, String::as_str), state));
        let tooltip = texts::tray_tooltip(reflected);
        let next = (status, tooltip);
        if inner.shown.as_ref() == Some(&next) {
            return None;
        }
        inner.shown = Some(next.clone());
        Some(next)
    }

    fn apply_tray(&self, tray: Option<(TrayStatus, String)>) {
        if let Some((status, tooltip)) = tray {
            self.tray.show(status, &tooltip);
        }
    }
}

impl StateObserver for Alerts {
    fn on_state(&self, server: &str, name: &str, state: LinkState, _failed_attempts: u32) {
        let now = (self.now_ms)();
        let (body, tray) = {
            let mut inner = self.lock();
            inner.names.insert(server.to_owned(), name.to_owned());
            inner.presence.set_state(server, state);
            let body = if self.is_enabled() {
                inner
                    .gate
                    .observe(server, state, now)
                    .map(|alert| Self::text_of(&inner, alert))
            } else {
                None
            };
            (body, Self::refresh_tray(&mut inner))
        };
        self.apply_tray(tray);
        if let Some(body) = body {
            self.notifier.notify(texts::APP_NAME, &body);
        }
    }

    fn on_removed(&self, server: &str) {
        let tray = {
            let mut inner = self.lock();
            inner.gate.forget(server);
            inner.presence.remove(server);
            inner.names.remove(server);
            Self::refresh_tray(&mut inner)
        };
        self.apply_tray(tray);
    }
}
