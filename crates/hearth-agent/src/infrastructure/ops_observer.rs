//! Ce que l'agent sait du service d'une opération non terminée quand il redémarre (HRT-83, ADR-0045).
//!
//! Cette tranche ne lit pas le moteur : aucune opération réelle n'existe encore, donc aucune ne peut être
//! interrompue en plein geste. L'observateur d'ici répond « illisible » : une opération destructive dont
//! l'état n'est pas établi est classée `needs_attention`, jamais « tout va bien ». Les tickets qui
//! écrivent les opérations (HRT-64, HRT-68, puis modifier et supprimer) le remplacent par un observateur
//! qui lit le moteur, derrière le même port.

use async_trait::async_trait;

use crate::application::ports::{OpRecord, ServiceObserver};
use crate::domain::ops::Observed;

pub struct UnobservedState;

#[async_trait]
impl ServiceObserver for UnobservedState {
    async fn observe(&self, _op: &OpRecord) -> Observed {
        Observed::Unreadable
    }
}
