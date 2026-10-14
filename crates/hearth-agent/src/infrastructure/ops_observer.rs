//! Ce que l'agent sait du service d'une opération non terminée quand il redémarre (HRT-83, HRT-144,
//! ADR-0045).
//!
//! - [`EngineObserver`] LIT le moteur (jamais n'y écrit) : il compte les conteneurs de la clé du service
//!   (aucun, un, plusieurs, illisible) et laisse `domain::ops::observed_from_presence` en tirer l'état que
//!   `recover` classe. C'est ce que l'agent branche au démarrage.
//! - [`UnobservedState`] répond « illisible » : une opération destructive dont l'état n'est pas établi est
//!   « à vérifier », jamais « tout va bien ». **Il ne sert plus qu'aux tests** (et à l'assemblage sans
//!   moteur).
//!
//! Qui trouve ses conteneurs comment :
//! - `c_<identifiant>` : l'inspection du conteneur de cet identifiant (présent ou absent) ;
//! - `h_<ULID>` : les conteneurs que le moteur liste avec l'étiquette `io.hearth.service-id` de ce service
//!   (un service créé par Hearth qu'une recréation a pu dédoubler), **mais l'étiquette ne vaut jamais
//!   identité** : elle peut être posée par n'importe qui, y compris par l'auteur d'une image (`LABEL`).
//!   Un conteneur n'est compté que si la clé de l'étiquette est celle d'un service CONNU de la base de
//!   l'agent ET si son identifiant est un de ceux que l'agent a consignés pour ce service ; tout autre est
//!   ignoré du décompte et signalé en trace (un nombre, jamais un nom ni une étiquette). Tant que le
//!   journal ne consigne aucun identifiant de conteneur (`RecordedService::KnownWithoutContainers`), un
//!   conteneur étiqueté ne peut être ni reconnu ni écarté : l'état est « illisible », donc « à vérifier »,
//!   jamais « un seul conteneur, tout va bien » (BR-OPS-008) ;
//! - `u_<unité>` : « illisible » ici, c'est le superviseur de l'unité qui sait quel conteneur la porte (les
//!   opérations sur un service supervisé arrivent avec leur lot).
//!
//! Aucun nom de conteneur, aucune image, aucun port ne sort d'ici : seulement un nombre.

use std::sync::Arc;

use async_trait::async_trait;

use crate::application::ports::{
    ContainerEngine, OpRecord, RecordedContainers, RecordedService, ServiceObserver,
    ServiceSpecRepo, StoreError,
};
use crate::domain::ops::{Observed, Presence, StepRecord, observed_from_presence};
use crate::domain::services::{KeyKind, ServiceKey};

pub struct UnobservedState;

#[async_trait]
impl ServiceObserver for UnobservedState {
    async fn observe(&self, _op: &OpRecord, _steps: &[StepRecord]) -> Observed {
        Observed::Unreadable
    }
}

/// Ce que la base de l'agent consigne des conteneurs d'un service : le service est connu s'il a une
/// description courante. L'agent ne consigne pas encore d'identifiant de conteneur (le geste qui crée le
/// conteneur arrive avec les lots suivants) : un service connu rend donc `KnownWithoutContainers`.
pub struct SpecRecordedContainers {
    specs: Arc<dyn ServiceSpecRepo>,
}

impl SpecRecordedContainers {
    pub fn new(specs: Arc<dyn ServiceSpecRepo>) -> Self {
        Self { specs }
    }
}

#[async_trait]
impl RecordedContainers for SpecRecordedContainers {
    async fn recorded(&self, key: &ServiceKey) -> Result<RecordedService, StoreError> {
        Ok(match self.specs.current(key).await? {
            Some(_) => RecordedService::KnownWithoutContainers,
            None => RecordedService::Unknown,
        })
    }
}

/// L'observateur du moteur de conteneurs.
pub struct EngineObserver {
    engine: Arc<dyn ContainerEngine>,
    recorded: Arc<dyn RecordedContainers>,
}

impl EngineObserver {
    pub fn new(engine: Arc<dyn ContainerEngine>, recorded: Arc<dyn RecordedContainers>) -> Self {
        Self { engine, recorded }
    }

    async fn presence(&self, key: &ServiceKey) -> Presence {
        match key.kind() {
            KeyKind::Container => {
                let Some(id) = key.as_str().strip_prefix("c_") else {
                    return Presence::Unreadable;
                };
                match self.engine.inspect(id).await {
                    Ok(Some(_)) => Presence::One,
                    Ok(None) => Presence::Absent,
                    Err(_) => Presence::Unreadable,
                }
            }
            KeyKind::Hearth => {
                let Ok(listing) = self.engine.list().await else {
                    return Presence::Unreadable;
                };
                let Ok(recorded) = self.recorded.recorded(key).await else {
                    return Presence::Unreadable;
                };
                // Les conteneurs qui AFFIRMENT être ce service (l'étiquette) : une affirmation, pas une preuve.
                let claiming: Vec<&str> = listing
                    .observations
                    .iter()
                    .filter(|observation| observation.hearth_key.as_ref() == Some(key))
                    .map(|observation| observation.id.as_str())
                    .collect();
                let counted = match &recorded {
                    RecordedService::Containers(ids) => claiming
                        .iter()
                        .filter(|id| ids.iter().any(|recorded| recorded == **id))
                        .count(),
                    RecordedService::Unknown | RecordedService::KnownWithoutContainers => 0,
                };
                let ignored = claiming.len() - counted;
                if ignored > 0 {
                    // Un nombre seulement : ni nom de conteneur, ni image, ni étiquette.
                    tracing::warn!(
                        ignored,
                        known = !matches!(recorded, RecordedService::Unknown),
                        "conteneur(s) portant l'étiquette d'un service de Hearth ignoré(s) pour la reprise : identifiant non consigné par l'agent"
                    );
                }
                match counted {
                    // Deux conteneurs consignés pour la même clé, quoi que dise le reste de la liste.
                    2.. => Presence::Several,
                    // Un conteneur que le moteur n'a pas pu inspecter pourrait être celui-ci, ou un
                    // deuxième : on ne dit « aucun » ni « un seul » sur une liste incomplète.
                    _ if !listing.unreadable.is_empty() => Presence::Unreadable,
                    1 => Presence::One,
                    // Aucun identifiant consigné et des conteneurs qui se disent de ce service : ni reconnus ni
                    // écartés, jamais « aucun » sur la foi de l'agent.
                    _ if ignored > 0 && recorded == RecordedService::KnownWithoutContainers => {
                        Presence::Unreadable
                    }
                    _ => Presence::Absent,
                }
            }
            KeyKind::Unit => Presence::Unreadable,
        }
    }
}

#[async_trait]
impl ServiceObserver for EngineObserver {
    async fn observe(&self, op: &OpRecord, steps: &[StepRecord]) -> Observed {
        let presence = self.presence(&op.service).await;
        observed_from_presence(op.kind, &op.plan, steps, presence)
    }
}
