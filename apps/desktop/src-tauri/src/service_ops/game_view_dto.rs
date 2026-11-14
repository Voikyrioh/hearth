//! Les types que l'interface lit pour le quotidien d'un serveur de jeu (HRT-104, `BR-JEU-017` à `BR-JEU-021`,
//! `BR-JEU-025` à `BR-JEU-027`). Aucune règle ici : la vue vient de l'agent telle quelle, l'interface la met en phrases.
//! Le mot de passe du serveur ne traverse que la réponse de `reveal_game_password` : texte inoffensif pour les traces
//! (`HiddenText`), jamais gardé. Types simples pour `specta` (pas de `u64` ni de `i64`).

use hearth_proto::api::game_secret::GameSecret;
use hearth_proto::api::game_view::{GamePort, GameView};
use serde::Serialize;
use specta::Type;

use super::catalog_dto::{FicheStopDto, stop_dto};
use crate::services::dto::HiddenText;

/// Un port publié par le serveur de jeu (`BR-JEU-017`, `BR-JEU-019`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GamePortDto {
    pub host_port: u16,
    pub protocol: String,
    pub role: String,
    pub changed: bool,
}

/// La vue d'un serveur de jeu : fiche, façon d'arrêter, ports publiés, mot de passe oui ou non.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GameViewDto {
    pub fiche: String,
    pub name: String,
    pub fiche_version: u32,
    pub stop: FicheStopDto,
    pub ports: Vec<GamePortDto>,
    pub has_password: bool,
}

impl From<&GameView> for GameViewDto {
    fn from(view: &GameView) -> Self {
        let GameView {
            fiche,
            name,
            fiche_version,
            stop,
            ports,
            has_password,
        } = view;
        Self {
            fiche: fiche.clone(),
            name: name.clone(),
            fiche_version: *fiche_version,
            stop: stop_dto(stop),
            ports: ports
                .iter()
                .map(|port| {
                    let GamePort {
                        host_port,
                        protocol,
                        role,
                        changed,
                    } = port;
                    GamePortDto {
                        host_port: *host_port,
                        protocol: protocol.clone(),
                        role: role.clone(),
                        changed: *changed,
                    }
                })
                .collect(),
            has_password: *has_password,
        }
    }
}

/// La lecture de la vue : un service qui n'est pas un serveur de jeu créé depuis une fiche n'est pas une panne, c'est
/// `Absent` (`BR-JEU-001`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GameReadDto {
    Found { view: Box<GameViewDto> },
    Absent,
}

/// Le mot de passe du serveur de jeu (`BR-JEU-025`) ; absent quand le serveur n'en a pas (`BR-JEU-027`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GameSecretDto {
    pub password: Option<HiddenText>,
}

impl From<&GameSecret> for GameSecretDto {
    fn from(secret: &GameSecret) -> Self {
        Self {
            password: secret.password.as_ref().map(HiddenText::from),
        }
    }
}

#[cfg(test)]
mod tests {
    use hearth_proto::api::catalog::{StopView, WorldSavedView};
    use hearth_proto::api::services::Concealed;

    use super::*;

    #[test]
    fn the_password_never_shows_in_debug_and_travels_as_plain_text() {
        let secret = GameSecret {
            password: Some(Concealed::new("SENTINEL-PW-4d2")),
        };
        let dto = GameSecretDto::from(&secret);
        assert!(!format!("{dto:?}").contains("SENTINEL"));
        let json = serde_json::to_value(&dto).expect("json");
        assert_eq!(json["password"], "SENTINEL-PW-4d2");
        let none = serde_json::to_value(GameSecretDto::from(&GameSecret { password: None }))
            .expect("json");
        assert!(none["password"].is_null());
    }

    #[test]
    fn a_view_keeps_the_ports_and_the_stop_way() {
        let view = GameView {
            fiche: "terraria".into(),
            name: "Terraria".into(),
            fiche_version: 1,
            stop: StopView {
                grace_s: 30,
                world_saved: WorldSavedView::BySequence,
                sequence_tested: false,
            },
            ports: vec![GamePort {
                host_port: 7778,
                protocol: "tcp".into(),
                role: "game".into(),
                changed: true,
            }],
            has_password: true,
        };
        let json = serde_json::to_value(GameViewDto::from(&view)).expect("json");
        assert_eq!(json["ficheVersion"], 1);
        assert_eq!(json["stop"]["worldSaved"], "by_sequence");
        assert_eq!(json["stop"]["graceSeconds"], 30);
        assert_eq!(json["ports"][0]["hostPort"], 7778);
        assert_eq!(json["ports"][0]["changed"], true);
        assert_eq!(json["hasPassword"], true);
    }
}
