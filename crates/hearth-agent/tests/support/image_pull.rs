//! Téléchargement de l'image des tests contre un vrai moteur (étapes `engine` et `supervised` de la CI).
//!
//! Le registre public peut répondre `toomanyrequests: Rate exceeded` à un job qui n'a rien fait de mal
//! (trois échecs de suite le 2026-10-13). Règle : le téléchargement de l'image, et lui seul, réessaie un
//! petit nombre de fois, avec une attente qui double et qui est bornée, quand l'erreur est une limite de
//! débit ou une panne passagère du registre ; toute autre erreur (image inconnue, accès refusé, moteur
//! absent) échoue tout de suite, sans attente. Quand les essais sont épuisés, le message dit pourquoi et
//! garde la dernière erreur du moteur. Une image déjà présente n'est JAMAIS retéléchargée (`docker pull`
//! interroge le registre même quand l'image est là) : une fois par exécution au plus.
//!
//! Rien ici ne masque un vrai défaut des tests : seul `docker pull` est rejoué, jamais un test, jamais la
//! création ni le lancement d'un conteneur.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

/// Nombre d'essais au total (le premier compris).
pub const MAX_ATTEMPTS: u32 = 5;
/// Première attente ; elle double ensuite, jusqu'à `MAX_WAIT`.
const FIRST_WAIT: Duration = Duration::from_secs(5);
const MAX_WAIT: Duration = Duration::from_secs(60);

/// L'attente avant l'essai suivant, après l'échec numéro `failed` (1 pour le premier) : 5, 10, 20, 40 s,
/// jamais plus de 60 s.
pub fn wait_after(failed: u32) -> Duration {
    let doubled = FIRST_WAIT.saturating_mul(1_u32 << failed.saturating_sub(1).min(16));
    doubled.min(MAX_WAIT)
}

/// L'erreur du moteur est-elle une limite de débit ou une panne passagère du registre ou du réseau ?
/// Reconnue par le texte (le moteur n'a pas de code d'erreur stable pour un téléchargement).
pub fn is_transient(stderr: &str) -> bool {
    let text = stderr.to_lowercase();
    [
        "toomanyrequests",
        "too many requests",
        "rate exceeded",
        "rate limit",
        "500 internal server error",
        "bad gateway",
        "service unavailable",
        "gateway timeout",
        "tls handshake timeout",
        "i/o timeout",
        "timeout exceeded",
        "context deadline exceeded",
        "connection reset",
        "connection refused",
        "unexpected eof",
        "temporary failure in name resolution",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}

/// Rejoue `attempt` tant que son erreur est passagère, au plus `MAX_ATTEMPTS` fois, en appelant `wait`
/// entre deux essais. Séparé de `docker` pour se tester sans moteur ni attente réelle.
pub fn pull_with(
    image: &str,
    mut attempt: impl FnMut() -> Result<(), String>,
    mut wait: impl FnMut(Duration),
) -> Result<(), String> {
    let mut last = String::new();
    for number in 1..=MAX_ATTEMPTS {
        match attempt() {
            Ok(()) => return Ok(()),
            Err(error) if !is_transient(&error) => {
                return Err(format!(
                    "téléchargement de l'image {image} refusé (ce n'est pas une panne passagère du registre) : {error}"
                ));
            }
            Err(error) => {
                eprintln!(
                    "téléchargement de l'image {image} : essai {number}/{MAX_ATTEMPTS} sans succès (limite de débit ou panne passagère) : {}",
                    error.trim()
                );
                last = error;
                if number < MAX_ATTEMPTS {
                    wait(wait_after(number));
                }
            }
        }
    }
    Err(format!(
        "téléchargement de l'image {image} abandonné après {MAX_ATTEMPTS} essais (limite de débit ou panne du registre, pas un défaut du code testé) : dernière erreur : {}",
        last.trim()
    ))
}

fn docker_ok(args: &[&str]) -> Result<(), String> {
    let output = Command::new("docker")
        .args(args)
        .output()
        .map_err(|error| format!("docker est introuvable : {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

/// L'image est-elle déjà dans le moteur ?
fn present(image: &str) -> bool {
    docker_ok(&["image", "inspect", "--format", "{{.Id}}", image]).is_ok()
}

/// Met l'image dans le moteur, UNE fois par exécution : présente, rien n'est téléchargé ; absente, un
/// téléchargement avec reprise. Panique avec un message clair si l'image n'a pas pu être obtenue.
pub fn ensure(image: &'static str) {
    static DONE: OnceLock<Result<(), String>> = OnceLock::new();
    let result = DONE.get_or_init(|| {
        if present(image) {
            return Ok(());
        }
        pull_with(
            image,
            || docker_ok(&["pull", "--quiet", image]),
            std::thread::sleep,
        )
    });
    if let Err(message) = result {
        panic!("{message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wait_doubles_and_is_bounded() {
        let waits: Vec<u64> = (1..=8).map(|n| wait_after(n).as_secs()).collect();
        assert_eq!(waits, [5, 10, 20, 40, 60, 60, 60, 60]);
        assert_eq!(wait_after(u32::MAX), MAX_WAIT);
        assert_eq!(wait_after(0), FIRST_WAIT);
    }

    #[test]
    fn rate_limits_and_registry_outages_are_transient_and_real_errors_are_not() {
        for transient in [
            "Error response from daemon: toomanyrequests: Rate exceeded",
            "Error response from daemon: Get \"https://public.ecr.aws/v2/\": net/http: TLS handshake timeout",
            "received unexpected HTTP status: 503 Service Unavailable",
            "read tcp 10.0.0.1:5555->1.2.3.4:443: connection reset by peer",
            "Too Many Requests",
        ] {
            assert!(is_transient(transient), "{transient}");
        }
        for permanent in [
            "Error response from daemon: manifest for busybox:9.99 not found: manifest unknown",
            "Error response from daemon: pull access denied for x, repository does not exist or may require 'docker login'",
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?",
            "unauthorized: authentication required",
        ] {
            assert!(!is_transient(permanent), "{permanent}");
        }
    }

    #[test]
    fn a_rate_limit_is_retried_with_growing_waits_then_succeeds() {
        let mut calls = 0;
        let mut waits = Vec::new();
        let result = pull_with(
            "img",
            || {
                calls += 1;
                if calls < 4 {
                    Err("toomanyrequests: Rate exceeded".to_owned())
                } else {
                    Ok(())
                }
            },
            |wait| waits.push(wait.as_secs()),
        );
        assert_eq!(result, Ok(()));
        assert_eq!(calls, 4);
        assert_eq!(waits, [5, 10, 20]);
    }

    #[test]
    fn a_permanent_error_fails_at_once_without_waiting() {
        let mut calls = 0;
        let mut waited = false;
        let error = pull_with(
            "img",
            || {
                calls += 1;
                Err("manifest unknown".to_owned())
            },
            |_| waited = true,
        )
        .unwrap_err();
        assert_eq!(calls, 1);
        assert!(!waited);
        assert!(error.contains("refusé"), "{error}");
        assert!(error.contains("manifest unknown"), "{error}");
    }

    #[test]
    fn exhausted_attempts_fail_with_a_clear_message_that_keeps_the_last_error() {
        let mut calls = 0;
        let mut waits = Vec::new();
        let error = pull_with(
            "img",
            || {
                calls += 1;
                Err(format!("toomanyrequests: Rate exceeded ({calls})"))
            },
            |wait| waits.push(wait.as_secs()),
        )
        .unwrap_err();
        assert_eq!(calls, MAX_ATTEMPTS);
        // Pas d'attente après le dernier essai : au plus 5 + 10 + 20 + 40 secondes.
        assert_eq!(waits, [5, 10, 20, 40]);
        assert!(error.contains("abandonné après 5 essais"), "{error}");
        assert!(error.contains("(5)"), "{error}");
    }
}
