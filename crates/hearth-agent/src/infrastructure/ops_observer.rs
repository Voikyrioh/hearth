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
//!   (un service créé par Hearth qu'une recréation a pu dédoubler) ;
//! - `u_<unité>` : « illisible » ici, c'est le superviseur de l'unité qui sait quel conteneur la porte (les
//!   opérations sur un service supervisé arrivent avec leur lot).
//!
//! Aucun nom de conteneur, aucune image, aucun port ne sort d'ici : seulement un nombre.

use std::sync::Arc;

use async_trait::async_trait;

use crate::application::ports::{ContainerEngine, OpRecord, ServiceObserver};
use crate::domain::ops::{Observed, Presence, StepRecord, observed_from_presence};
use crate::domain::services::{KeyKind, ServiceKey};

pub struct UnobservedState;

#[async_trait]
impl ServiceObserver for UnobservedState {
    async fn observe(&self, _op: &OpRecord, _steps: &[StepRecord]) -> Observed {
        Observed::Unreadable
    }
}

/// L'observateur du moteur de conteneurs.
pub struct EngineObserver {
    engine: Arc<dyn ContainerEngine>,
}

impl EngineObserver {
    pub fn new(engine: Arc<dyn ContainerEngine>) -> Self {
        Self { engine }
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
            KeyKind::Hearth => match self.engine.list().await {
                Ok(listing) => {
                    let count = listing
                        .observations
                        .iter()
                        .filter(|observation| observation.hearth_key.as_ref() == Some(key))
                        .count();
                    match count {
                        // Deux conteneurs de la même clé, quoi que dise le reste de la liste.
                        2.. => Presence::Several,
                        // Un conteneur que le moteur n'a pas pu inspecter pourrait être celui-ci, ou un
                        // deuxième : on ne dit « aucun » ni « un seul » sur une liste incomplète.
                        _ if !listing.unreadable.is_empty() => Presence::Unreadable,
                        1 => Presence::One,
                        _ => Presence::Absent,
                    }
                }
                Err(_) => Presence::Unreadable,
            },
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
