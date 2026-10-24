//! Les portes de clôture, UNE PAR SERVICE (FIX:01M5P0Q3T8ZK6V2R9XH4DWB7NC, HRT-101).
//!
//! Une clôture (transition vers un statut terminal) tient la porte de son service, en lecture, du commit
//! jusqu'à la fin de la publication (verrous rendus, projection, annonce). Une demande qui veut juger
//! « occupé » attend UNE fois, au plus l'échéance ([`CLOSING_GATE_WAIT`]), que les clôtures de CE service se terminent :
//! une clôture lente sur A ne fait jamais attendre B. La table n'a une entrée que tant que quelqu'un tient
//! ou attend la porte de la clé (coût : un verrou court sur la table, une allocation par clôture).
//!
//! Pas d'interblocage : une clôture ne prend la porte d'un service qu'une fois à la fois et n'attend, sous
//! la porte, que le journal et la publication, jamais une autre porte. Une attente a une échéance.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::sync::{OwnedRwLockReadGuard, RwLock};

use crate::domain::services::key::ServiceKey;

/// L'attente maximale d'une demande devant une clôture en cours. Elle est au moins égale au délai d'attente de
/// la base (`infrastructure::sqlite::BUSY_TIMEOUT`, 5 s) : une clôture qui attend la base ne doit pas
/// dépasser la porte (un test de l'infrastructure les tient liés). Sans clôture en cours, l'attente est nulle.
pub const CLOSING_GATE_WAIT: Duration = Duration::from_secs(5);

pub struct ClosingGates {
    table: Mutex<HashMap<ServiceKey, Arc<RwLock<()>>>>,
    wait: Duration,
}

impl Default for ClosingGates {
    fn default() -> Self {
        Self::with_wait(CLOSING_GATE_WAIT)
    }
}

/// Retire l'entrée de la clé quand plus personne ne la tient ni ne l'attend (au `Drop`, donc aussi si
/// l'attente est abandonnée).
struct Idle {
    gates: Arc<ClosingGates>,
    key: ServiceKey,
}

impl Drop for Idle {
    fn drop(&mut self) {
        let mut table = self
            .gates
            .table
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if table
            .get(&self.key)
            .is_some_and(|lock| Arc::strong_count(lock) == 1)
        {
            table.remove(&self.key);
        }
    }
}

/// La porte d'un service tenue par une clôture. Les champs tombent dans l'ordre : la lecture est rendue,
/// puis l'entrée est nettoyée.
pub struct ClosingGuard {
    _read: OwnedRwLockReadGuard<()>,
    _idle: Idle,
}

impl ClosingGates {
    /// Des portes à l'échéance choisie (les tests de l'échéance ne paient pas l'horloge réelle).
    pub fn with_wait(wait: Duration) -> Self {
        Self {
            table: Mutex::default(),
            wait,
        }
    }

    pub fn deadline(&self) -> Duration {
        self.wait
    }

    /// Prend la porte du service pour une clôture (plusieurs clôtures du même service peuvent la tenir).
    pub async fn close(self: &Arc<Self>, key: &ServiceKey) -> ClosingGuard {
        let idle = Idle {
            gates: Arc::clone(self),
            key: key.clone(),
        };
        let lock = {
            let mut table = self.table.lock().unwrap_or_else(PoisonError::into_inner);
            Arc::clone(table.entry(key.clone()).or_default())
        };
        let read = lock.read_owned().await;
        ClosingGuard {
            _read: read,
            _idle: idle,
        }
    }

    /// Attend que les clôtures en cours de ce service se terminent, au plus l'échéance ([`CLOSING_GATE_WAIT`]). Rend
    /// `true` si l'échéance est atteinte (on juge alors quand même).
    pub async fn wait(self: &Arc<Self>, key: &ServiceKey) -> bool {
        let _idle = Idle {
            gates: Arc::clone(self),
            key: key.clone(),
        };
        let lock = self
            .table
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key)
            .cloned();
        let Some(lock) = lock else {
            return false;
        };
        tokio::time::timeout(self.wait, lock.write()).await.is_err()
    }

    /// Le nombre de services dont la porte est tenue ou attendue (tests).
    pub fn open(&self) -> usize {
        self.table
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }
}
