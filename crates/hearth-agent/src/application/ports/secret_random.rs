use thiserror::Error;

#[derive(Debug, Error)]
#[error("le générateur aléatoire du système est indisponible : {0}")]
pub struct RandomError(pub String);

/// Octets du hasard cryptographique du système, pour tirer un secret de jeu (HRT-101, BR-FICHE-013). Un échec
/// est une erreur, jamais un repli sur un hasard faible. Le port ne donne que des octets : l'alphabet, la
/// longueur et le rejet des octets biaisés sont des règles du domaine (`domain::catalog::generate`).
pub trait SecretRandom: Send + Sync {
    fn fill(&self, buf: &mut [u8]) -> Result<(), RandomError>;
}
