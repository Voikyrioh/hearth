//! Le quotidien d'un serveur de jeu (HRT-104) : deux lectures, la vue (`GET /services/{key}/game`) et le mot de passe
//! (`GET /services/{key}/game-secret`, réservé au droit « gérer les services »). Les requêtes sont construites par la
//! liaison (`LinkManager::{read_game_view, reveal_game_password}`, chemins fixes, clé vérifiée puis encodée) ; les refus
//! se lisent par leur CODE, jamais par leur texte. Rien n'est gardé ni tracé ici. Sans Tauri.

use hearth_link::domain::server::ServerId;
use hearth_link::{LinkError, LinkManager};
use hearth_proto::error::ErrorCode;

use super::game_view_dto::{GameReadDto, GameSecretDto, GameViewDto};
use crate::link_dto::LinkFailure;

/// La vue d'un service de jeu. Un service qui n'est pas né d'une fiche (adopté, ordinaire) n'est pas une panne :
/// `Absent`.
pub async fn read(
    manager: &LinkManager,
    id: &ServerId,
    key: &str,
) -> Result<GameReadDto, LinkFailure> {
    match manager.read_game_view(id, key).await {
        Ok(view) => Ok(GameReadDto::Found {
            view: Box::new(GameViewDto::from(&view)),
        }),
        Err(LinkError::Rejected(Some(ErrorCode::NotAGameService))) => Ok(GameReadDto::Absent),
        Err(other) => Err(other.into()),
    }
}

/// Le mot de passe du serveur de jeu : rendu à l'appelant et à lui seul.
pub async fn reveal(
    manager: &LinkManager,
    id: &ServerId,
    key: &str,
) -> Result<GameSecretDto, LinkFailure> {
    let secret = manager.reveal_game_password(id, key).await?;
    Ok(GameSecretDto::from(&secret))
}
