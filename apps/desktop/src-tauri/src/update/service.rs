//! Cas d'usage de la mise à jour du client : vérifier (au plus une fois par jour, ou à la demande),
//! reporter, télécharger et installer SUR CLIC. Tout ce qui sort du processus passe par les ports
//! (`ports.rs`) ; l'état affiché est calculé ici et publié en entier à chaque changement.
//!
//! Garanties tenues ici (ADR-0017) :
//! - aucune installation sans appel de `begin_install` (donc sans clic : seule la commande
//!   `install_update` l'appelle) ;
//! - toute vérification, réussie ou non, est inscrite sur disque AVANT l'appel réseau : une
//!   application tuée en cours de route ne la refait pas dans la journée ;
//! - une vérification qui échoue ne laisse aucune trace à l'écran (BR-UPDATE-007, 008).

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use super::domain::{
    self, Candidate, DownloadPolicy, Rejection, Release, UpdateRecord, validate_candidate,
};
use super::dto::{AvailableDto, UpdateFailure, UpdatePhase, UpdateStateDto};
use super::ports::{Clock, DownloadError, Feed, StateSink, UpdateStore};

/// Pourquoi une installation ne démarre pas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InstallRefusal {
    #[error("une opération de mise à jour est déjà en cours")]
    Busy,
    #[error("aucune mise à jour connue")]
    NothingAvailable,
}

struct Inner {
    record: UpdateRecord,
    phase: UpdatePhase,
    progress: Option<u8>,
    failure: Option<UpdateFailure>,
    seq: u32,
    /// Visibilité du bandeau dans le dernier état publié (le temps la change sans autre événement).
    last_banner: bool,
}

pub struct UpdateService {
    clock: Arc<dyn Clock>,
    store: Arc<dyn UpdateStore>,
    feed: Arc<dyn Feed>,
    sink: Arc<dyn StateSink>,
    policy: DownloadPolicy,
    current_version: String,
    inner: Mutex<Inner>,
}

impl UpdateService {
    pub fn new(
        clock: Arc<dyn Clock>,
        store: Arc<dyn UpdateStore>,
        feed: Arc<dyn Feed>,
        sink: Arc<dyn StateSink>,
        policy: DownloadPolicy,
        current_version: &str,
    ) -> Self {
        let mut record = store.load();
        domain::forget_installed(&mut record, current_version);
        let last_banner = domain::banner_visible(&record, clock.now_ms());
        Self {
            clock,
            store,
            feed,
            sink,
            policy,
            current_version: current_version.to_owned(),
            inner: Mutex::new(Inner {
                record,
                phase: UpdatePhase::Idle,
                progress: None,
                failure: None,
                seq: 0,
                last_banner,
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// L'état courant (sans le republier).
    pub fn state(&self) -> UpdateStateDto {
        let inner = self.lock();
        self.dto(&inner)
    }

    fn dto(&self, inner: &Inner) -> UpdateStateDto {
        let now = self.clock.now_ms();
        let record = &inner.record;
        UpdateStateDto {
            seq: inner.seq,
            current_version: self.current_version.clone(),
            phase: inner.phase,
            progress: inner.progress,
            available: record.available.as_ref().map(|release| AvailableDto {
                version: release.version.clone(),
                notes: release.notes.clone(),
            }),
            banner_visible: domain::banner_visible(record, now),
            postponed_until: record
                .postponed_until
                .filter(|_| domain::is_postponed(now, record.postponed_until))
                .map(|ms| ms as f64),
            last_checked_at: record.last_success_at.map(|ms| ms as f64),
            up_to_date: record.available.is_none()
                && record.last_success_at.is_some()
                && record.last_success_at == record.last_attempt_at,
            failure: inner.failure,
        }
    }

    fn publish(&self, inner: &mut Inner) {
        inner.seq = inner.seq.wrapping_add(1);
        let state = self.dto(inner);
        inner.last_banner = state.banner_visible;
        self.sink.publish(&state);
    }

    fn persist(&self, inner: &Inner) {
        if let Err(error) = self.store.save(&inner.record) {
            tracing::warn!(%error, "état de la mise à jour non enregistré");
        }
    }

    /// Vérification automatique : seulement si la règle des 24 h la permet (BR-UPDATE-001).
    pub async fn check_if_due(&self) -> UpdateStateDto {
        self.check(true).await
    }

    /// « Vérifier maintenant » (BR-UPDATE-026) : à la demande, quelle que soit l'heure.
    pub async fn check_now(&self) -> UpdateStateDto {
        self.check(false).await
    }

    async fn check(&self, automatic: bool) -> UpdateStateDto {
        let started = self.clock.now_ms();
        {
            let mut inner = self.lock();
            if inner.phase != UpdatePhase::Idle {
                return self.dto(&inner);
            }
            if automatic && !domain::check_is_due(started, inner.record.last_attempt_at) {
                return self.dto(&inner);
            }
            inner.phase = UpdatePhase::Checking;
            inner.record.last_attempt_at = Some(started);
            self.persist(&inner);
            self.publish(&mut inner);
        }
        let outcome = self.feed.check().await;
        let mut inner = self.lock();
        let before = inner.record.available.clone();
        if self.absorb(&mut inner.record, outcome, started) && inner.record.available != before {
            inner.failure = None;
        }
        inner.phase = UpdatePhase::Idle;
        self.persist(&inner);
        self.publish(&mut inner);
        self.dto(&inner)
    }

    /// Range la réponse du flux dans l'enregistrement. Rend vrai si elle était valable (la
    /// vérification compte alors comme réussie) ; sinon rien ne change, sans bruit (BR-UPDATE-007).
    fn absorb(
        &self,
        record: &mut UpdateRecord,
        outcome: Result<Option<Candidate>, super::ports::FeedError>,
        started: i64,
    ) -> bool {
        let release = match outcome {
            Err(error) => {
                tracing::debug!(%error, "vérification des mises à jour sans réponse");
                return false;
            }
            Ok(None) => None,
            Ok(Some(candidate)) => {
                match validate_candidate(&self.current_version, &candidate, &self.policy) {
                    Ok(release) => Some(release),
                    Err(Rejection::NotNewer) => None,
                    Err(rejection) => {
                        tracing::warn!(%rejection, "annonce de version refusée");
                        return false;
                    }
                }
            }
        };
        record.available = release;
        record.last_success_at = Some(started);
        true
    }

    /// « Plus tard » : le bandeau disparaît jusqu'au lendemain (BR-UPDATE-006).
    pub fn postpone(&self) -> UpdateStateDto {
        let mut inner = self.lock();
        if matches!(
            inner.phase,
            UpdatePhase::Downloading | UpdatePhase::Installing
        ) {
            return self.dto(&inner);
        }
        if inner.record.available.is_some() {
            inner.record.postponed_until = Some(domain::postponed_until(self.clock.now_ms()));
            inner.failure = None;
            self.persist(&inner);
            self.publish(&mut inner);
        }
        self.dto(&inner)
    }

    /// Passe au téléchargement. À n'appeler que sur un clic de l'utilisateur (BR-UPDATE-002).
    pub fn begin_install(&self) -> Result<(), InstallRefusal> {
        let mut inner = self.lock();
        if inner.phase != UpdatePhase::Idle {
            return Err(InstallRefusal::Busy);
        }
        if inner.record.available.is_none() {
            return Err(InstallRefusal::NothingAvailable);
        }
        inner.phase = UpdatePhase::Downloading;
        inner.progress = Some(0);
        inner.failure = None;
        self.publish(&mut inner);
        Ok(())
    }

    /// Télécharge, vérifie la signature, installe. Suite de `begin_install`.
    pub async fn run_install(&self) {
        let Some(mut version) = self
            .lock()
            .record
            .available
            .as_ref()
            .map(|r| r.version.clone())
        else {
            self.settle(None);
            return;
        };
        let mut result = self.download(&version).await;
        if matches!(result, Err(DownloadError::NotStaged)) {
            // Annonce retenue d'une session précédente : on la relit (sur clic, donc permis).
            match self.refresh_for_install().await {
                Refresh::Version(fresh) => {
                    version = fresh;
                    result = self.download(&version).await;
                }
                Refresh::Gone => {
                    self.settle(None);
                    return;
                }
                Refresh::Unreachable => {
                    self.settle(Some(UpdateFailure::Interrupted));
                    return;
                }
            }
        }
        match result {
            Ok(bytes) => {
                {
                    let mut inner = self.lock();
                    inner.phase = UpdatePhase::Installing;
                    inner.progress = None;
                    self.publish(&mut inner);
                }
                if let Err(error) = self.feed.install(&version, bytes) {
                    tracing::error!(%error, "installation de la mise à jour impossible");
                    self.settle(Some(failure_of(&error)));
                }
            }
            Err(error) => {
                tracing::warn!(%error, "mise à jour non installée");
                self.settle(Some(failure_of(&error)));
            }
        }
    }

    /// `begin_install` puis `run_install` (pour les tests et les appelants qui attendent).
    pub async fn install(&self) -> Result<(), InstallRefusal> {
        self.begin_install()?;
        self.run_install().await;
        Ok(())
    }

    async fn download(&self, version: &str) -> Result<Vec<u8>, DownloadError> {
        let mut last = Some(0u8);
        let mut report = |received: u64, total: Option<u64>| {
            let percent = total
                .filter(|total| *total > 0)
                .map(|total| (received.saturating_mul(100) / total).min(100) as u8);
            if percent.is_some() && percent != last {
                last = percent;
                let mut inner = self.lock();
                inner.progress = percent;
                self.publish(&mut inner);
            }
        };
        self.feed.download(version, &mut report).await
    }

    async fn refresh_for_install(&self) -> Refresh {
        let started = self.clock.now_ms();
        let outcome = self.feed.check().await;
        let mut inner = self.lock();
        inner.record.last_attempt_at = Some(started);
        let valid = self.absorb(&mut inner.record, outcome, started);
        self.persist(&inner);
        match (&inner.record.available, valid) {
            (Some(release), true) => Refresh::Version(release.version.clone()),
            (None, true) => Refresh::Gone,
            (_, false) => Refresh::Unreachable,
        }
    }

    /// Fin d'une tentative sans installation : retour au repos, avec ou sans échec à montrer.
    fn settle(&self, failure: Option<UpdateFailure>) {
        let mut inner = self.lock();
        inner.phase = UpdatePhase::Idle;
        inner.progress = None;
        inner.failure = failure;
        self.publish(&mut inner);
    }

    /// Un battement de l'horloge (toutes les heures, sans réseau) : le bandeau reparaît quand le
    /// report est échu, puis une vérification automatique si elle est permise.
    pub async fn tick(&self) {
        {
            let mut inner = self.lock();
            let visible = domain::banner_visible(&inner.record, self.clock.now_ms());
            if inner.phase == UpdatePhase::Idle && visible != inner.last_banner {
                self.publish(&mut inner);
            }
        }
        self.check_if_due().await;
    }

    /// Boucle de la coquille : la première vérification peu après le lancement (BR-UPDATE-001), puis
    /// un battement par `period`. Ne rend jamais la main.
    pub async fn run_scheduler(&self, first_delay: Duration, period: Duration) {
        tokio::time::sleep(first_delay).await;
        loop {
            self.tick().await;
            tokio::time::sleep(period).await;
        }
    }

    pub fn current_release(&self) -> Option<Release> {
        self.lock().record.available.clone()
    }
}

enum Refresh {
    Version(String),
    Gone,
    Unreachable,
}

fn failure_of(error: &DownloadError) -> UpdateFailure {
    match error {
        DownloadError::Interrupted(_) => UpdateFailure::Interrupted,
        DownloadError::Corrupted(_) => UpdateFailure::Corrupted,
        DownloadError::Failed(_) | DownloadError::NotStaged => UpdateFailure::Failed,
    }
}
