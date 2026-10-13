//! Adopter un conteneur existant et le relâcher (HRT-80, ADR-0043, `BR-ADOPT-005` à `BR-ADOPT-010`,
//! `BR-ADOPT-013`, `BR-ADOPT-019` à `BR-ADOPT-021`, `BR-ADOPT-024`, `BR-ADOPT-026`).
//!
//! C'est l'acte qui OUVRE la frontière de confidentialité d'un serveur : tant qu'un conteneur n'est pas
//! adopté, Hearth ne le publie pas (HRT-79). Ce module ne décide pas qui peut adopter : le droit « gérer
//! les services » est jugé par la couche d'accès (`Access::Services`) et la confirmation complète (preuve
//! de clé du poste puis mot de passe, jamais l'élévation) par la couche `reauth`, AVANT d'arriver ici. Il
//! garantit le reste :
//!
//! - **rien n'est écrit sur le service ni sur le serveur** (`BR-ADOPT-009`) : l'adoption est une ligne de
//!   la base de l'agent (`adopted_services`), jamais une étiquette, jamais un appel au moteur ;
//! - **l'identité est celle du registre** (`u_<unité>` ou `c_<identifiant>`), jamais un nom, et seulement
//!   pour un conteneur que le registre voit à cet instant : un conteneur disparu entre le choix et l'acte
//!   n'est pas adopté ;
//! - **une seule adoption gagne** quand deux comptes adoptent en même temps : les actes sont sérialisés
//!   ici, et l'écriture est refusée en base si la clé l'est déjà ;
//! - **la ligne et l'entrée du journal d'activité réussissent ensemble** (même transaction) : pas
//!   d'adoption sans trace, pas de trace d'une adoption qui n'a pas eu lieu ;
//! - **un refus ne révèle rien** : une clé cachée, inexistante ou mal formée a la même issue
//!   (`NotFound`, même entrée de journal sans nom), le nom d'un conteneur n'entre au journal qu'au moment
//!   où on l'adopte, et seulement s'il l'est à l'issue de l'acte (`BR-ADOPT-019`) ;
//! - **jamais de relance automatique** : ici rien ne rejoue un acte (`BR-ADOPT-025`).
//!
//! La liste des candidats n'est lue que sur demande d'un compte qui a le droit, et n'est ni diffusée ni
//! gardée (`BR-ADOPT-005`). Sa lecture réussie est consignée par le gestionnaire HTTP, sans aucun nom de
//! conteneur.

use std::collections::HashSet;
use std::sync::Arc;

use hearth_proto::api::services::MAX_ADOPTIONS;
use thiserror::Error;
use tokio::sync::Mutex;

use super::audit::{AuditTrail, Pending};
use super::ports::{AdoptionWrite, Clock, NewAdoption, Store, StoreError};
use super::services::{AdoptionLookup, Candidate, ServicesRegistry};
use crate::domain::audit::{Actor, AuditAction, AuditEvent, ClientName, Outcome, Reason, Target};
use crate::domain::services::{EngineState, KeyKind, ServiceKey};

/// La route où se consignent les issues qui ne nomment aucun service.
const ADOPT_ROUTE: &str = "/services/adopt";

#[derive(Debug, Error)]
pub enum AdoptionError {
    /// `services.adoption = false` (`BR-ADOPT-020`).
    #[error("adoption désactivée")]
    Disabled,
    /// Le moteur n'est pas prêt : rien ne peut être adopté (`BR-ADOPT-026`).
    #[error("moteur non prêt")]
    EngineNotReady(EngineState),
    /// Aucune clé, plus de 50 clés ou une clé en double (`BR-ADOPT-007`).
    #[error("demande invalide")]
    Invalid(Invalid),
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Ce qui rend une demande d'adoption invalide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invalid {
    Empty,
    TooMany,
    Duplicate,
}

/// Les candidats, avec l'état du moteur qui explique une liste vide.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidates {
    pub engine: EngineState,
    pub candidates: Vec<Candidate>,
}

/// Le résultat d'une clé (`BR-ADOPT-010`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyOutcome {
    Adopted,
    AlreadyAdopted { by: String },
    NotFound,
    Failed,
}

/// Le résultat d'un acte d'adoption : une issue par clé, dans l'ordre de la demande.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdoptionReport {
    pub adopted: u32,
    pub results: Vec<(String, KeyOutcome)>,
}

pub struct ServiceAdoption {
    registry: Arc<ServicesRegistry>,
    store: Arc<dyn Store>,
    clock: Arc<dyn Clock>,
    trail: Arc<AuditTrail>,
    /// `services.adoption` : à `false`, aucun candidat et l'acte est refusé ; relâcher reste possible.
    enabled: bool,
    /// Un acte à la fois : « le premier acte arrivé au serveur » gagne (`BR-ADOPT-024`).
    gate: Mutex<()>,
}

impl ServiceAdoption {
    pub fn new(
        registry: Arc<ServicesRegistry>,
        store: Arc<dyn Store>,
        clock: Arc<dyn Clock>,
        trail: Arc<AuditTrail>,
        enabled: bool,
    ) -> Self {
        Self {
            registry,
            store,
            clock,
            trail,
            enabled,
            gate: Mutex::new(()),
        }
    }

    /// Les conteneurs que ce compte peut choisir (`BR-ADOPT-005`, `BR-ADOPT-006`). Le droit est déjà jugé.
    /// Moteur pas prêt : la liste est vide et l'état du moteur dit pourquoi (`BR-ADOPT-026`).
    pub fn candidates(&self) -> Result<Candidates, AdoptionError> {
        if !self.enabled {
            return Err(AdoptionError::Disabled);
        }
        let engine = self.registry.engine_status().state;
        let candidates = if engine == EngineState::Ready {
            self.registry.candidates()
        } else {
            Vec::new()
        };
        Ok(Candidates { engine, candidates })
    }

    /// Adopte ces clés, une issue par clé. Les clés sont celles que la confirmation a montrées (le nombre
    /// et l'empreinte sont signés par la clé du poste, vérifiés par la couche `reauth`).
    pub async fn adopt(
        &self,
        keys: &[String],
        by: &Actor,
    ) -> Result<AdoptionReport, AdoptionError> {
        if !self.enabled {
            return Err(AdoptionError::Disabled);
        }
        if keys.is_empty() {
            return Err(AdoptionError::Invalid(Invalid::Empty));
        }
        if keys.len() > MAX_ADOPTIONS {
            return Err(AdoptionError::Invalid(Invalid::TooMany));
        }
        let distinct: HashSet<&String> = keys.iter().collect();
        if distinct.len() != keys.len() {
            return Err(AdoptionError::Invalid(Invalid::Duplicate));
        }
        let state = self.registry.engine_status().state;
        if state != EngineState::Ready {
            return Err(AdoptionError::EngineNotReady(state));
        }
        let _one_act_at_a_time = self.gate.lock().await;
        let mut report = AdoptionReport {
            adopted: 0,
            results: Vec::with_capacity(keys.len()),
        };
        // Les issues qui ne nomment aucun service (clé inconnue, divergence mémoire et base) sont
        // comptées par motif et consignées UNE fois chacune pour l'acte, avec leur nombre : un acte de
        // cinquante clés inventées ne peut pas écrire cinquante entrées (`BR-ADOPT-019`).
        let mut unnamed: Vec<(Reason, u32)> = Vec::new();
        for raw in keys {
            let (outcome, unnamed_refusal) = self.adopt_one(raw, by).await;
            if let Some(reason) = unnamed_refusal {
                match unnamed.iter_mut().find(|(known, _)| *known == reason) {
                    Some((_, count)) => *count += 1,
                    None => unnamed.push((reason, 1)),
                }
            }
            if outcome == KeyOutcome::Adopted {
                report.adopted += 1;
            }
            report.results.push((raw.clone(), outcome));
        }
        for (reason, count) in unnamed {
            self.refuse(by, Target::Route(ADOPT_ROUTE), reason, count)
                .await;
        }
        let diverged = report
            .results
            .iter()
            .any(|(_, outcome)| matches!(outcome, KeyOutcome::AlreadyAdopted { .. }));
        if report.adopted > 0 || diverged {
            // Publie ce qui vient d'être adopté : le service est dans la liste à la publication suivante.
            // Une clé « déjà adoptée » peut venir d'une base en avance sur la mémoire : on relit aussi.
            self.registry.reload_adopted().await;
        }
        Ok(report)
    }

    /// L'issue d'une clé. Le second élément est le motif d'une issue qui ne nomme aucun service : sa
    /// consignation est groupée par l'appelant, jamais écrite ici.
    async fn adopt_one(&self, raw: &str, by: &Actor) -> (KeyOutcome, Option<Reason>) {
        // Une clé mal formée ou `h_` (déjà publiée, créée par Hearth) a l'issue d'une clé inconnue.
        let key = ServiceKey::parse(raw).filter(|key| key.kind() != KeyKind::Hearth);
        let lookup = key.as_ref().map_or(AdoptionLookup::Unknown, |key| {
            self.registry.adoption_lookup(key)
        });
        let (key, candidate) = match (key, lookup) {
            (Some(key), AdoptionLookup::Candidate(candidate)) => (key, candidate),
            (Some(key), AdoptionLookup::Adopted { by: adopter }) => {
                // Déjà publié à tous : son nom peut s'écrire.
                let target = self
                    .registry
                    .published_label(&key)
                    .map_or(Target::Route(ADOPT_ROUTE), |(name, unit)| {
                        service_target(&name, unit.as_ref().map(|unit| unit.as_str()))
                    });
                // Un service publié, donc nommé dans la liste de tous : une entrée par service, bornée
                // par le nombre de services réellement adoptés.
                self.refuse(by, target, Reason::AlreadyAdopted, 1).await;
                return (KeyOutcome::AlreadyAdopted { by: adopter }, None);
            }
            _ => {
                // Aucun nom, aucune clé : la même issue pour une clé cachée, disparue ou inventée.
                return (KeyOutcome::NotFound, Some(Reason::ServiceNotFound));
            }
        };
        let account = by
            .account
            .as_ref()
            .map_or_else(|| "inconnu".to_owned(), ToString::to_string);
        let adoption = NewAdoption {
            key,
            name: candidate.name.clone(),
            unit: candidate.unit.as_ref().map(|unit| unit.as_str().to_owned()),
            by: account,
            at: self.clock.now(),
        };
        match self.write(&adoption, by).await {
            Ok(AdoptionWrite::Adopted) => (KeyOutcome::Adopted, None),
            Ok(AdoptionWrite::AlreadyAdopted { by: adopter }) => {
                // La mémoire disait « candidat », la base dit « déjà adopté » : le conteneur n'est pas
                // publié (la mémoire diverge), son nom n'entre donc pas au journal. `adopt` relit la base
                // après l'acte pour que la mémoire rejoigne la base.
                (
                    KeyOutcome::AlreadyAdopted { by: adopter },
                    Some(Reason::AlreadyAdopted),
                )
            }
            Err(error) => {
                tracing::warn!(%error, "adoption non écrite");
                (KeyOutcome::Failed, None)
            }
        }
    }

    /// La ligne et l'entrée du journal, dans la même transaction.
    async fn write(&self, adoption: &NewAdoption, by: &Actor) -> Result<AdoptionWrite, StoreError> {
        let mut tx = self.store.begin().await?;
        // `BR-ADOPT-027` : une adoption « à confirmer » garde sa ligne mais n'ouvre aucun droit. La
        // confirmer, c'est la réécrire au nom du compte qui agit, avec toutes les gardes de l'acte
        // (droit, preuve de clé, mot de passe) : l'ancienne ligne est remplacée dans la même transaction.
        let stale = self.registry.is_unconfirmed(&adoption.key);
        if stale {
            tx.adoptions().release(&adoption.key).await?;
        }
        let written = tx.adoptions().adopt(adoption).await?;
        if written != AdoptionWrite::Adopted {
            return Ok(written);
        }
        let mut journal = Pending::default();
        let event = AuditEvent::new(
            self.clock.now(),
            by.clone(),
            AuditAction::ServiceAdopt,
            service_target(&adoption.name, adoption.unit.as_deref()),
            Outcome::Succeeded,
        );
        journal.record(&mut *tx, event).await?;
        tx.commit().await?;
        journal.publish(&self.trail);
        if stale {
            self.registry.confirm_adoption(&adoption.key);
        }
        Ok(written)
    }

    /// Une issue qui n'a rien adopté, consignée « Échoué » avec sa cause. `times` : combien de clés de
    /// l'acte ont cette issue (une entrée, `times - 1` autres fois). Un échec d'écriture est tracé.
    async fn refuse(&self, by: &Actor, target: Target, reason: Reason, times: u32) {
        let result = async {
            let mut tx = self.store.begin().await?;
            let mut journal = Pending::default();
            let event = AuditEvent::new(
                self.clock.now(),
                by.clone(),
                AuditAction::ServiceAdopt,
                target,
                Outcome::Failed(reason),
            )
            .with_repeats(times.saturating_sub(1));
            journal.record(&mut *tx, event).await?;
            tx.commit().await?;
            journal.publish(&self.trail);
            Ok::<(), StoreError>(())
        }
        .await;
        if let Err(error) = result {
            tracing::warn!(%error, "issue d'adoption non consignée");
        }
    }

    /// Rend un service adopté à l'état non géré (`BR-ADOPT-013`). `Ok(false)` : il n'était déjà plus adopté
    /// (rien n'est écrit, rien n'est consigné, et l'appelant ne sait pas pourquoi : inconnu, jamais adopté
    /// et déjà relâché se répondent pareil). Le service n'est pas touché ; il sort de la liste de tous à la
    /// publication suivante et ses lectures de journaux ouvertes se ferment.
    pub async fn release(&self, key: &ServiceKey, by: &Actor) -> Result<bool, AdoptionError> {
        let _one_act_at_a_time = self.gate.lock().await;
        let mut tx = self.store.begin().await?;
        let Some(released) = tx.adoptions().release(key).await? else {
            return Ok(false);
        };
        let mut journal = Pending::default();
        let event = AuditEvent::new(
            self.clock.now(),
            by.clone(),
            AuditAction::ServiceRelease,
            service_target(&released.name, released.unit.as_deref()),
            Outcome::Succeeded,
        );
        journal.record(&mut *tx, event).await?;
        tx.commit().await?;
        journal.publish(&self.trail);
        self.registry.reload_adopted().await;
        Ok(true)
    }
}

/// Le nom du service et son unité : les seuls détails d'un conteneur que le journal écrit.
fn service_target(name: &str, unit: Option<&str>) -> Target {
    match ClientName::parse(name) {
        Some(name) => Target::ServiceUnit(name, unit.and_then(ClientName::parse)),
        None => Target::None,
    }
}
