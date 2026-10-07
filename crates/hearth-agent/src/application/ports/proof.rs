use thiserror::Error;

/// Vérifie une signature de clé d'appareil. Ne juge rien d'autre : le message à signer, le défi et
/// la clé inscrite sont l'affaire de l'application.
pub trait ProofVerifier: Send + Sync {
    /// `true` seulement si `signature` est une signature valide de `message` sous `public_key` pour
    /// cet algorithme. Aucune entrée ne fait paniquer : un algorithme inconnu, une clé ou une
    /// signature de mauvaise longueur valent `false`.
    fn verify(&self, algorithm: &str, public_key: &[u8], message: &[u8], signature: &[u8]) -> bool;
}

/// Hasard et authentification des défis (clé tirée au démarrage du service, jamais exposée).
pub trait ChallengeCrypto: Send + Sync {
    /// Seize octets tirés du hasard du système.
    fn nonce(&self) -> Result<[u8; 16], CryptoError>;

    /// Code d'authentification (HMAC-SHA256) de ces octets.
    fn mac(&self, input: &[u8]) -> [u8; 32];
}

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("hasard du système indisponible : {0}")]
    Random(String),
}
