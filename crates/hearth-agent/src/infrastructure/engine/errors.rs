//! Traduction des erreurs du moteur en [`EngineError`] typé.
//!
//! Le texte du moteur ne sort JAMAIS d'ici : un message inconnu devient `Refused`, jamais affiché
//! brut (ADR-0036). La table des textes d'erreur d'écriture grandit avec les tranches qui écrivent : le
//! port déjà pris (HRT-64, démarrage) ; les échecs de récupération d'une image et le nom déjà pris à la
//! création (HRT-68, BR-DEPLOY-017). Les textes reconnus sont ceux de Docker et de Podman, à confirmer par le
//! job `engine` de la CI (`tests/engine_real.rs` : conflit de port réel, image et version inexistantes) ;
//! un texte que la table ne connaît pas est `Refused`, jamais deviné.

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

/// Une erreur de RÉCUPÉRATION d'image (HRT-68, BR-DEPLOY-017). Le moteur ne donne pas de code stable : le
/// texte est lu pour être reconnu, puis jeté. L'erreur d'un flux de téléchargement arrive en pleine réponse
/// (`DockerStreamError`), celle d'un refus avant le flux avec un code HTTP.
pub fn translate_pull(error: &Error) -> EngineError {
    match error {
        Error::DockerStreamError { error } => pull_text(error).unwrap_or(EngineError::Refused),
        Error::DockerResponseServerError {
            status_code,
            message,
        } => match pull_text(message) {
            Some(known) => known,
            None => match *status_code {
                401 | 403 | 404 => EngineError::ImagePrivateOrMissing,
                429 => EngineError::RegistryRateLimited,
                _ => translate(error),
            },
        },
        other => translate(other),
    }
}

/// Le texte d'une erreur de téléchargement, reconnu par morceaux connus de Docker et de Podman. L'ordre compte :
/// la limite de débit et le disque plein d'abord (ils masquent le reste), puis la plateforme, la version, le
/// registre, enfin l'image.
///
/// Source des textes (HRT-168) : la suite de contrat récupère son image chez un registre LOCAL (`registry:2`),
/// dont les textes sont ceux de la distribution (`manifest unknown`, `name unknown`). Les textes du registre
/// public (`pull access denied`, `repository does not exist`) ne sont plus confirmés par la suite bloquante :
/// l'étape non bloquante « Textes d'erreur du registre public » du job `engine` de la CI les relit chez ECR
/// (`continue-on-error`) ; si elle devient rouge, relire cette table.
fn pull_text(message: &str) -> Option<EngineError> {
    let text = message.to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|needle| text.contains(needle));
    if has(&[
        "toomanyrequests",
        "too many requests",
        "rate limit",
        "rate exceeded",
    ]) {
        Some(EngineError::RegistryRateLimited)
    } else if has(&[
        "no space left on device",
        "not enough free space",
        "disk quota exceeded",
    ]) {
        Some(EngineError::DiskFull)
    } else if has(&[
        "no matching manifest",
        "no match for platform",
        "does not provide the specified platform",
        "no child with platform",
        "does not match the specified platform",
    ]) {
        Some(EngineError::PlatformMismatch)
    } else if has(&[
        "manifest unknown",
        "manifest for ",
        "tag does not exist",
        "not found: manifest",
    ]) {
        Some(EngineError::TagNotFound)
    } else if has(&[
        "pull access denied",
        "requested access to the resource is denied",
        "unauthorized",
        "authentication required",
        "may require 'docker login'",
        "repository does not exist",
        "denied:",
        ": denied",
    ]) {
        Some(EngineError::ImagePrivateOrMissing)
    } else if has(&[
        "name unknown",
        "repository name not known",
        "image not known",
    ]) {
        Some(EngineError::ImageNotFound)
    } else if has(&[
        "no such host",
        "connection refused",
        "i/o timeout",
        "tls handshake timeout",
        "client.timeout exceeded",
        "network is unreachable",
        "temporary failure in name resolution",
        "request canceled while waiting for connection",
        "context deadline exceeded",
        "dial tcp",
        "connection reset",
        "unexpected eof",
        "server misbehaving",
        "bad gateway",
        "service unavailable",
    ]) {
        Some(EngineError::RegistryUnreachable)
    } else {
        None
    }
}

/// Une erreur de CRÉATION de conteneur : le nom déjà pris (409), le disque plein ; le reste comme ailleurs.
pub fn translate_create(error: &Error) -> EngineError {
    if let Error::DockerResponseServerError {
        status_code,
        message,
    } = error
    {
        let text = message.to_lowercase();
        if *status_code == 409 && text.contains("is already in use") {
            return EngineError::NameTaken;
        }
        if text.contains("no space left on device") {
            return EngineError::DiskFull;
        }
        // Un port pris se découvre au démarrage, mais certains moteurs le disent déjà à la création.
        if port_taken(message) {
            return EngineError::PortInUse;
        }
    }
    translate(error)
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

    fn stream_error(text: &str) -> Error {
        Error::DockerStreamError { error: text.into() }
    }

    /// La table des échecs de récupération, textes enregistrés de Docker et de Podman (BR-DEPLOY-017). Le job
    /// `engine` de la CI confirme les textes d'un vrai moteur (`tests/engine_real.rs`).
    #[test]
    fn every_recorded_pull_failure_text_has_its_cause() {
        let table = [
            (
                "pull access denied for nopenope, repository does not exist or may require 'docker login': denied: requested access to the resource is denied",
                EngineError::ImagePrivateOrMissing,
            ),
            (
                "manifest for nginx:nope not found: manifest unknown: manifest unknown",
                EngineError::TagNotFound,
            ),
            (
                "Head \"https://ghcr.io/v2/x/y/manifests/z\": denied",
                EngineError::ImagePrivateOrMissing,
            ),
            (
                "name unknown: repository name not known to registry",
                EngineError::ImageNotFound,
            ),
            (
                "initializing source docker://nope:latest: reading manifest latest in docker.io/library/nope: requested access to the resource is denied",
                EngineError::ImagePrivateOrMissing,
            ),
            ("Error: image not known", EngineError::ImageNotFound),
            (
                "toomanyrequests: You have reached your pull rate limit. You may increase the limit by authenticating and upgrading",
                EngineError::RegistryRateLimited,
            ),
            (
                "no matching manifest for linux/arm64/v8 in the manifest list entries",
                EngineError::PlatformMismatch,
            ),
            (
                "image with reference x was found but its platform (linux/amd64) does not match the specified platform (linux/arm64)",
                EngineError::PlatformMismatch,
            ),
            (
                "write /var/lib/docker/tmp/GetImageBlob123: no space left on device",
                EngineError::DiskFull,
            ),
            (
                "Get \"https://registry-1.docker.io/v2/\": dial tcp: lookup registry-1.docker.io on 127.0.0.53:53: server misbehaving",
                EngineError::RegistryUnreachable,
            ),
            (
                "Get \"https://registry-1.docker.io/v2/\": net/http: request canceled while waiting for connection (Client.Timeout exceeded while awaiting headers)",
                EngineError::RegistryUnreachable,
            ),
            (
                "Get \"https://registry.example/v2/\": dial tcp 10.0.0.1:443: connect: connection refused",
                EngineError::RegistryUnreachable,
            ),
        ];
        for (text, expected) in table {
            assert_eq!(translate_pull(&stream_error(text)), expected, "{text}");
            // Le même texte venu d'un refus HTTP avant le flux.
            assert_eq!(translate_pull(&server(500, text)), expected, "{text}");
        }
    }

    #[test]
    fn an_unknown_pull_text_is_a_refusal_never_shown_and_a_bare_404_is_private_or_missing() {
        assert_eq!(
            translate_pull(&stream_error("un texte jamais vu SENTINEL")),
            EngineError::Refused
        );
        assert_eq!(
            translate_pull(&server(404, "x")),
            EngineError::ImagePrivateOrMissing
        );
        assert_eq!(
            translate_pull(&server(429, "x")),
            EngineError::RegistryRateLimited
        );
        let shown = format!("{:?}", translate_pull(&stream_error("SENTINEL-TEXT")));
        assert!(!shown.contains("SENTINEL"));
        // La limite de débit prime sur « unauthorized » dans le même texte.
        assert_eq!(
            translate_pull(&stream_error("toomanyrequests: unauthorized")),
            EngineError::RegistryRateLimited
        );
    }

    #[test]
    fn a_creation_conflict_on_the_name_is_told_apart() {
        assert_eq!(
            translate_create(&server(
                409,
                "Conflict. The container name \"/web\" is already in use by container \"abc\". You have to remove (or rename) that container to be able to reuse that name."
            )),
            EngineError::NameTaken
        );
        assert_eq!(
            translate_create(&server(500, "write: no space left on device")),
            EngineError::DiskFull
        );
        assert_eq!(
            translate_create(&server(404, "No such image: x")),
            EngineError::NotFound
        );
        assert_eq!(
            translate_create(&server(409, "autre chose")),
            EngineError::Refused
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
