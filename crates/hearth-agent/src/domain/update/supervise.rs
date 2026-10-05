//! BR-UPDATE-015 : le contrôle du nouvel agent par le superviseur. Le nouvel agent a 60 secondes
//! pour répondre à `GET /hello` avec la nouvelle version et le même certificat ; sinon l'ancien
//! binaire revient. Fonction pure : le temps écoulé et la dernière réponse en entrée, le verdict
//! en sortie.

use std::time::Duration;

use hearth_proto::api::update::UpdateReason;

use crate::domain::install::Version;

/// Combien de temps le nouvel agent a pour répondre après son démarrage (BR-UPDATE-015).
pub const CHECK_WINDOW: Duration = Duration::from_secs(60);
/// Intervalle entre deux contrôles.
pub const CHECK_POLL: Duration = Duration::from_secs(1);
/// Délai laissé à l'ancien agent, après le lancement du superviseur, pour envoyer l'étape
/// « redémarrage » aux clients avant d'être arrêté.
pub const STOP_GRACE: Duration = Duration::from_secs(2);

/// Ce que le contrôle a obtenu du nouvel agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// Aucune réponse (connexion refusée, délai, réponse illisible).
    None,
    /// Une réponse de `GET /hello`.
    Hello {
        version: Version,
        /// Le certificat servi est celui de l'installation (BR-UPDATE-018 : l'empreinte survit).
        same_identity: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Pas encore : continuer à attendre.
    Wait,
    /// Le nouvel agent répond avec la nouvelle version.
    Succeeded,
    /// Revenir à l'ancien binaire, pour cette raison.
    Rollback(UpdateReason),
}

/// Le verdict après `elapsed` de contrôle, dans une fenêtre de `window` (`CHECK_WINDOW`, raccourcie
/// seulement par les tests).
///
/// - la bonne version avec le bon certificat : réussi, tout de suite ;
/// - la bonne version avec un autre certificat : retour immédiat (une identité qui change est
///   refusée, pas attendue) ;
/// - une autre version ou pas de réponse : on attend, jusqu'à 60 s, puis retour.
pub fn check_verdict(
    elapsed: Duration,
    answer: &Answer,
    expected: Version,
    window: Duration,
) -> Verdict {
    match answer {
        Answer::Hello {
            version,
            same_identity,
        } if *version == expected => {
            if *same_identity {
                Verdict::Succeeded
            } else {
                Verdict::Rollback(UpdateReason::IdentityChanged)
            }
        }
        _ if elapsed >= window => Verdict::Rollback(UpdateReason::NoAnswer),
        _ => Verdict::Wait,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NEW: Version = Version::new(0, 2, 0);
    const OLD: Version = Version::new(0, 1, 0);

    fn hello(version: Version, same_identity: bool) -> Answer {
        Answer::Hello {
            version,
            same_identity,
        }
    }

    #[test]
    fn the_window_is_sixty_seconds() {
        assert_eq!(CHECK_WINDOW, Duration::from_secs(60));
    }

    #[test]
    fn the_new_version_with_the_same_certificate_succeeds_at_once() {
        assert_eq!(
            check_verdict(Duration::ZERO, &hello(NEW, true), NEW, CHECK_WINDOW),
            Verdict::Succeeded
        );
        assert_eq!(
            check_verdict(
                Duration::from_secs(59),
                &hello(NEW, true),
                NEW,
                CHECK_WINDOW
            ),
            Verdict::Succeeded
        );
    }

    #[test]
    fn no_answer_waits_until_the_window_is_over_then_rolls_back() {
        assert_eq!(
            check_verdict(Duration::from_secs(59), &Answer::None, NEW, CHECK_WINDOW),
            Verdict::Wait
        );
        assert_eq!(
            check_verdict(Duration::from_secs(60), &Answer::None, NEW, CHECK_WINDOW),
            Verdict::Rollback(UpdateReason::NoAnswer)
        );
        assert_eq!(
            check_verdict(Duration::from_secs(600), &Answer::None, NEW, CHECK_WINDOW),
            Verdict::Rollback(UpdateReason::NoAnswer)
        );
    }

    #[test]
    fn the_old_version_still_answering_is_not_a_success() {
        assert_eq!(
            check_verdict(Duration::from_secs(3), &hello(OLD, true), NEW, CHECK_WINDOW),
            Verdict::Wait
        );
        assert_eq!(
            check_verdict(
                Duration::from_secs(60),
                &hello(OLD, true),
                NEW,
                CHECK_WINDOW
            ),
            Verdict::Rollback(UpdateReason::NoAnswer)
        );
    }

    #[test]
    fn a_changed_certificate_is_refused_at_once() {
        assert_eq!(
            check_verdict(Duration::ZERO, &hello(NEW, false), NEW, CHECK_WINDOW),
            Verdict::Rollback(UpdateReason::IdentityChanged)
        );
    }
}
