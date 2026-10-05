//! Veilleurs globaux : réveil du PC (saut d'horloge) et changement de réseau. Chacun déclenche une
//! tentative immédiate sur tous les serveurs (BR-RESIL-006).

use std::collections::BTreeSet;
use std::net::IpAddr;
use std::sync::{Arc, Weak};

use tokio::task::JoinHandle;

use super::task::Command;
use super::{Deps, Registry};
use crate::domain::triggers::{WAKE_JUMP, detect_wake, network_changed};

pub(crate) fn spawn(deps: Arc<Deps>, registry: Weak<Registry>) -> Vec<JoinHandle<()>> {
    vec![
        tokio::spawn(watch_wake(deps.clone(), registry.clone())),
        tokio::spawn(watch_network(deps, registry)),
    ]
}

async fn watch_wake(deps: Arc<Deps>, registry: Weak<Registry>) {
    let period = deps.config.wake_check_period;
    let mut previous = (deps.clock.mono(), deps.clock.wall());
    let mut ticker = tokio::time::interval(period);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticker.tick().await;
    loop {
        ticker.tick().await;
        let now = (deps.clock.mono(), deps.clock.wall());
        let woke = detect_wake(previous, now, period, WAKE_JUMP);
        previous = now;
        let Some(registry) = registry.upgrade() else {
            return;
        };
        if woke {
            tracing::info!("réveil du poste détecté");
            registry.broadcast(|| Command::Woke);
        }
    }
}

async fn watch_network(deps: Arc<Deps>, registry: Weak<Registry>) {
    let mut known: Option<BTreeSet<IpAddr>> = None;
    let mut ticker = tokio::time::interval(deps.config.net_poll_period);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        let Some(registry) = registry.upgrade() else {
            return;
        };
        // Une lecture qui échoue ne dit rien du réseau : on garde la dernière liste connue.
        let Ok(now) = deps.net.addresses().await else {
            continue;
        };
        if let Some(previous) = &known
            && network_changed(previous, &now)
        {
            tracing::info!("changement de réseau détecté");
            registry.broadcast(|| Command::NetworkChanged);
        }
        known = Some(now);
    }
}
