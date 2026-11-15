//! Reprendre un monde existant, côté coquille (HRT-106) : deux lectures, la liste des mondes de la fiche et
//! l'inspection d'un monde. Ni acte, ni journal, rien n'est écrit. Sans Tauri.

use hearth_link::LinkManager;
use hearth_link::domain::server::ServerId;

use super::worlds_dto::{WorldInspectReadDto, WorldRefDto, WorldsReadDto};
use crate::link_dto::LinkFailure;

/// Les mondes que la fiche reconnaît dans les dossiers ouverts aux services (`GET /catalog/{id}/worlds`). Une fiche
/// qui ne dit pas ce qu'est un monde n'est pas une panne : `NotSupported`.
pub async fn list(
    manager: &LinkManager,
    id: &ServerId,
    fiche: &str,
) -> Result<WorldsReadDto, LinkFailure> {
    let outcome = manager.read_worlds(id, fiche).await?;
    Ok(WorldsReadDto::from(&outcome))
}

/// Inspecte un monde (`POST /catalog/{id}/worlds/inspect`) : ce que la copie écrirait et ce qu'elle laisserait.
pub async fn inspect(
    manager: &LinkManager,
    id: &ServerId,
    fiche: &str,
    world: WorldRefDto,
) -> Result<WorldInspectReadDto, LinkFailure> {
    let outcome = manager.inspect_world(id, fiche, &world.into()).await?;
    Ok(WorldInspectReadDto::from(&outcome))
}
