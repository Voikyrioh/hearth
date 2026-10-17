//! Traduction des erreurs du moteur en [`EngineError`] typé.
//!
//! Le texte du moteur ne sort JAMAIS d'ici : un message inconnu devient `Refused`, jamais affiché
//! brut (ADR-0036). La table des textes d'erreur d'écriture grandit avec les tranches qui écrivent : le
//! port déjà pris (HRT-64, démarrage) ; l'image inconnue et la limite d'un registre viendront avec le
//! déploiement. Les textes reconnus sont ceux de Docker et de Podman, à confirmer par le job `engine` de la
//! CI (`tests/engine_real.rs`, conflit de port réel : `docs/adr/ADR-0036`, « Non vérifié »).

use bollard::errors::Error;

use crate::application::ports::EngineError;

pub fn translate(error: &Error) -> EngineError {
    match error {
        Error::DockerResponseServerError {
            status_code,
            message,
        } => match *status_code {
            404 => EngineError::NotFound,
            401 | 403 => EngineError::Denied,
            // Ni un refus du moteur, ni un conteneur absent : le moteur ne tient pas la route.
            502..=504 => EngineError::Unreachable,
            // Un démarrage qui bute sur un port pris : Docker répond 500 (« port is already allocated »,
            // « address already in use »), Podman 500 aussi. Le texte n'est lu que pour le reconnaître.
            400 | 409 | 500 if port_taken(message) => EngineError::PortInUse,
            _ => EngineError::Refused,
        },
        Error::RequestTimeoutError => EngineError::Timeout,
        Error::SocketNotFoundError(_)
        | Error::IOError { .. }
        | Error::HyperResponseError { .. }
        | Error::HyperLegacyError { .. } => EngineError::Unreachable,
        // Réponse illisible, version d'API refusée, erreur de flux… : le moteur n'a pas dit ce
        // qu'on attendait.
        _ => EngineError::Refused,
    }
}

/// Le message du moteur dit-il qu'un port est déjà pris ? Cherché sans tenir compte de la casse, parmi les
/// formulations connues de Docker (Linux et Windows) et de Podman.
fn port_taken(message: &str) -> bool {
    let message = message.to_lowercase();
    [
        "port is already allocated",
        "address already in use",
        "ports are not available",
        "only one usage of each socket address",
    ]
    .iter()
    .any(|known| message.contains(known))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(status_code: u16, message: &str) -> Error {
        Error::DockerResponseServerError {
            status_code,
            message: message.into(),
        }
    }

    #[test]
    fn a_missing_container_is_not_found_and_the_text_is_dropped() {
        let translated = translate(&server(404, "No such container: SECRET-SENTINEL"));
        assert_eq!(translated, EngineError::NotFound);
        assert!(!format!("{translated} {translated:?}").contains("SECRET-SENTINEL"));
    }

    #[test]
    fn an_unknown_server_text_is_a_refusal() {
        for status in [400, 409, 500] {
            assert_eq!(
                translate(&server(status, "un texte jamais vu")),
                EngineError::Refused,
                "{status}"
            );
        }
        assert_eq!(translate(&server(503, "x")), EngineError::Unreachable);
        // Un refus d'autorisation (proxy de socket, plugin) est un accès refusé, pas un texte inconnu.
        for status in [401, 403] {
            assert_eq!(translate(&server(status, "x")), EngineError::Denied);
        }
    }

    #[test]
    fn a_taken_port_is_recognised_in_the_text_and_the_text_is_dropped() {
        for text in [
            "driver failed programming external connectivity on endpoint web: Bind for 0.0.0.0:8080 failed: port is already allocated",
            "listen tcp4 0.0.0.0:25565: bind: Address already in use",
            "Ports are not available: exposing port TCP 0.0.0.0:80 -> 0.0.0.0:0: listen tcp 0.0.0.0:80: bind: Only one usage of each socket address",
            "rootlessport listen tcp 0.0.0.0:80: bind: address already in use",
        ] {
            let translated = translate(&server(500, text));
            assert_eq!(translated, EngineError::PortInUse, "{text}");
            let shown = format!("{translated} {translated:?}");
            assert!(
                !shown.contains("8080") && !shown.contains("endpoint"),
                "{shown}"
            );
        }
        // Un autre refus du démarrage reste un refus ; un 404 reste un conteneur absent.
        assert_eq!(
            translate(&server(500, "OCI runtime create failed")),
            EngineError::Refused
        );
        assert_eq!(
            translate(&server(404, "port is already allocated")),
            EngineError::NotFound
        );
    }

    #[test]
    fn a_timeout_and_a_broken_connection_are_told_apart() {
        assert_eq!(translate(&Error::RequestTimeoutError), EngineError::Timeout);
        let io = std::io::Error::new(std::io::ErrorKind::ConnectionReset, "reset");
        assert_eq!(
            translate(&Error::IOError { err: io }),
            EngineError::Unreachable
        );
        assert_eq!(
            translate(&Error::SocketNotFoundError("/var/run/docker.sock".into())),
            EngineError::Unreachable
        );
    }
}
