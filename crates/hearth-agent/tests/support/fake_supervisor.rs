#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
//! Faux superviseur de services : des propriétaires de conteneurs et des états d'unités réglables, des
//! compteurs d'appels. Sert à tous les tests de l'agent (aucun systemd sur le poste de développement).
//! Comme le vrai, il n'agit jamais sur une unité.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use async_trait::async_trait;
use tokio::sync::Semaphore;

use hearth_agent::application::ports::{ContainerRef, ServiceSupervisor};
use hearth_agent::domain::services::{UnitActivity, UnitLookup, UnitName, UnitStatus};

#[derive(Default)]
struct State {
    /// Identifiant de conteneur vers l'unité qui le possède.
    owners: HashMap<String, UnitName>,
    units: HashMap<UnitName, UnitLookup>,
}

pub struct FakeSupervisor {
    state: Mutex<State>,
    owner_queries: AtomicU64,
    owner_containers_asked: AtomicU64,
    unit_queries: AtomicU64,
    /// Crochet de test : tant qu'il est posé, la lecture d'une unité signale son arrivée puis attend la
    /// reprise, sans aucun délai réel (relecture suspendue en plein passage).
    held: AtomicBool,
    arrived: Semaphore,
    resume: Semaphore,
}

impl Default for FakeSupervisor {
    fn default() -> Self {
        Self {
            state: Mutex::default(),
            owner_queries: AtomicU64::new(0),
            owner_containers_asked: AtomicU64::new(0),
            unit_queries: AtomicU64::new(0),
            held: AtomicBool::new(false),
            arrived: Semaphore::new(0),
            resume: Semaphore::new(0),
        }
    }
}

/// Une unité de service qui tourne, avec 120 secondes de délai d'arrêt.
pub fn running_unit() -> UnitStatus {
    UnitStatus {
        activity: UnitActivity::Active,
        auto_restart: false,
        restarts: true,
        timeout_stop_s: Some(120),
    }
}

pub fn unit_named(name: &str) -> UnitName {
    UnitName::parse(name).expect("nom d'unité valide")
}

impl FakeSupervisor {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Ce conteneur est lancé sous cette unité.
    pub fn set_owner(&self, container_id: &str, unit: &UnitName) {
        self.lock()
            .owners
            .insert(container_id.to_owned(), unit.clone());
    }

    /// Ce que le système dit de l'unité (`UnitLookup::Unavailable` : pas de réponse).
    pub fn set_unit(&self, unit: &UnitName, lookup: UnitLookup) {
        self.lock().units.insert(unit.clone(), lookup);
    }

    pub fn set_status(&self, unit: &UnitName, status: UnitStatus) {
        self.set_unit(unit, UnitLookup::Loaded(status));
    }

    /// Combien de fois les propriétaires ont été cherchés, et pour combien de conteneurs en tout.
    pub fn owner_queries(&self) -> u64 {
        self.owner_queries.load(Ordering::SeqCst)
    }

    pub fn owner_containers_asked(&self) -> u64 {
        self.owner_containers_asked.load(Ordering::SeqCst)
    }

    /// Suspend les lectures d'unités jusqu'à `release_unit_reads` (une lecture signale son arrivée).
    pub fn hold_unit_reads(&self) {
        self.held.store(true, Ordering::SeqCst);
    }

    /// Attend qu'une lecture d'unité soit arrivée et suspendue.
    pub async fn wait_until_a_unit_read_is_held(&self) {
        self.arrived
            .acquire()
            .await
            .expect("semaphore ouvert")
            .forget();
    }

    /// Reprend toutes les lectures suspendues et ne suspend plus rien.
    pub fn release_unit_reads(&self) {
        self.held.store(false, Ordering::SeqCst);
        self.resume.add_permits(1_000_000);
    }

    pub fn unit_queries(&self) -> u64 {
        self.unit_queries.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ServiceSupervisor for FakeSupervisor {
    async fn owners(&self, containers: &[ContainerRef]) -> HashMap<String, UnitName> {
        self.owner_queries.fetch_add(1, Ordering::SeqCst);
        self.owner_containers_asked
            .fetch_add(containers.len() as u64, Ordering::SeqCst);
        let state = self.lock();
        containers
            .iter()
            .filter_map(|container| {
                state
                    .owners
                    .get(&container.id)
                    .map(|unit| (container.id.clone(), unit.clone()))
            })
            .collect()
    }

    async fn unit(&self, unit: &UnitName) -> UnitLookup {
        self.unit_queries.fetch_add(1, Ordering::SeqCst);
        if self.held.load(Ordering::SeqCst) {
            self.arrived.add_permits(1);
            self.resume
                .acquire()
                .await
                .expect("semaphore ouvert")
                .forget();
        }
        self.lock()
            .units
            .get(unit)
            .copied()
            .unwrap_or(UnitLookup::Unavailable)
    }
}
