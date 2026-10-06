//! Un banc de mise à jour de l'agent pour les tests de liaison et de la coquille : le banc des tests
//! de l'agent (`crates/hearth-agent/tests/support/update.rs` : téléchargeur simulé avec une porte,
//! machine en mémoire, vraie vérification minisign avec une clé jetable) est repris tel quel, sans
//! copie. L'agent des tests de liaison (`TestAgent::install_with`) reçoit ses adaptateurs par
//! `Rig::updating()`.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use async_trait::async_trait;
use hearth_agent::application::ports::{AuditSink, Clock};
use hearth_agent::domain::audit::{Actor, AuditAction, Outcome, Target};
use hearth_agent::infrastructure::clock::SystemClock;

#[path = "../../../hearth-agent/tests/support/update.rs"]
mod inner;

#[allow(unused_imports)]
pub use inner::{CURRENT, Download, FakeDownloader, Keys, MemHost, Rig, sha256_hex};

/// Ce que le banc de l'agent lit de son environnement : un journal et une horloge (le service
/// propre du banc ne sert pas ici : le vrai agent construit le sien avec son vrai journal).
pub struct Env {
    pub audit_sink: Arc<dyn AuditSink>,
    pub clock: Arc<dyn Clock>,
}

struct NoSink;

#[async_trait]
impl AuditSink for NoSink {
    async fn record(&self, _: Actor, _: AuditAction, _: Target, _: Outcome) {}
}

/// Un banc : `allowed` faux simule une installation gérée ; `gated` garde le téléchargement
/// « en cours » tant que le test n'a pas appelé `release_gate`.
pub fn rig(allowed: bool, gated: bool) -> Rig {
    let env = Env {
        audit_sink: Arc::new(NoSink),
        clock: Arc::new(SystemClock),
    };
    Rig::new(&env, allowed, gated)
}
