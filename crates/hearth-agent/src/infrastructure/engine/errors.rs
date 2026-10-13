//! Traduction des erreurs du moteur en [`EngineError`] typé.
//!
//! Le texte du moteur ne sort JAMAIS d'ici : un message inconnu devient `Refused`, jamais affiché
//! brut (ADR-0036). Cette tranche est en lecture seule : la table des textes d'erreur d'écriture
//! (port pris, image inconnue, limite d'un registre) viendra avec les tranches qui écrivent, à
//! enregistrer depuis un vrai moteur (`docs/adr/ADR-0036`, « Non vérifié »).

use bollard::errors::Error;

use crate::application::ports::EngineError;

pub fn translate(error: &Error) -> EngineError {
    match error {
        Error::DockerResponseServerError { status_code, .. } => match *status_code {
            404 => EngineError::NotFound,
            401 | 403 => EngineError::Denied,
            // Ni un refus du moteur, ni un conteneur absent : le moteur ne tient pas la route.
            502..=504 => EngineError::Unreachable,
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
