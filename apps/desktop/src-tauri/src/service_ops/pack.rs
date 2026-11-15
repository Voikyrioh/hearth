//! Lire un pack de mods déposé avant de l'installer (HRT-124, `POST /deployments/pack-check`) : une CONSULTATION,
//! ni acte ni journal. La liaison construit la requête (identifiant de dépôt seulement, jamais un chemin) ; ici le
//! résultat devient ce que l'écran lit : le pack lu, ou son refus par un code. Pur côté lecture ; sans Tauri.

use hearth_link::LinkManager;
use hearth_link::domain::server::ServerId;

use super::pack_dto::{PackCheckOutcome, PackInspectDto, PackRefusalDto};
use crate::link_dto::LinkFailure;

/// Ce que l'agent lit dans le pack déposé sous `deposit`. Hors « Connecté » : `NotConnected`. Un identifiant qui n'en
/// est pas un : `InvalidInput`, rien ne part. Sans le droit « gérer les services » : `MissingServicesRight`.
pub async fn inspect(
    manager: &LinkManager,
    id: &ServerId,
    deposit: &str,
) -> Result<PackCheckOutcome, LinkFailure> {
    Ok(match manager.inspect_pack(id, deposit).await? {
        Ok(view) => PackCheckOutcome::Read {
            pack: PackInspectDto::from(&view),
        },
        Err(refusal) => PackCheckOutcome::Refused {
            refusal: PackRefusalDto::from(refusal),
        },
    })
}
