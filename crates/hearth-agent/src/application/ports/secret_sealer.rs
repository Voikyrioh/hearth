//! Les ports du scellement des secrets d'une description de service (HRT-83, ADR-0044) : le fichier de
//! clé, la vérification de clé gardée dans la base, et l'algorithme.
//!
//! Aucune erreur d'ici ne porte un octet de la clé ni d'un secret : des chemins, des tailles, des droits.

use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;

use super::StoreError;
use crate::domain::sealed::{
    DatabaseFacts, PlainBytes, SealContext, SealedBlob, ServicesKey, UnusableKey,
};

/// Ce que le magasin de clé a trouvé.
#[derive(Debug)]
pub enum KeyRead {
    Absent,
    /// Le fichier existe mais ne sert pas. Jamais réécrit par l'agent.
    Unusable(UnusableKey),
    Present(ServicesKey),
}

#[derive(Debug, Error)]
pub enum KeyStoreError {
    #[error("génération de la clé des services impossible : {0}")]
    Generation(String),
    #[error("accès à la clé des services ({path}) impossible : {message}")]
    Storage { path: String, message: String },
}

/// Le fichier `services.key`.
pub trait ServicesKeyStore: Send + Sync {
    /// Lit sans rien créer, rien corriger, rien supprimer.
    fn read(&self) -> KeyRead;

    /// Crée la clé de façon atomique : écrite dans un fichier voisin ouvert en `0600` avant le premier
    /// octet, synchronisée, puis posée par un lien dur qui échoue si le nom existe. Si un autre démarrage
    /// l'a créée entre-temps, c'est LA SIENNE qui est rendue. Ne remplace JAMAIS un fichier existant.
    fn create(&self) -> Result<ServicesKey, KeyStoreError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SealError {
    #[error("tirage du nonce impossible")]
    Random,
    #[error("secret trop long")]
    TooLong,
    /// L'authentification échoue : mauvaise clé, scellé altéré, ou scellé lié à un autre service, une
    /// autre révision ou un autre champ. Les cas ne se distinguent pas, volontairement.
    #[error("le scellé ne s'ouvre pas")]
    Open,
}

/// L'algorithme, avec une clé : chiffrement authentifié, nonce neuf par scellement, données associées
/// liées au service, à la révision et au champ.
pub trait SecretSealer: Send + Sync {
    fn seal(&self, context: &SealContext<'_>, plain: &[u8]) -> Result<SealedBlob, SealError>;

    fn open(&self, context: &SealContext<'_>, blob: &SealedBlob) -> Result<PlainBytes, SealError>;

    /// La valeur de vérification de cette clé (texte hexadécimal d'un HMAC d'une constante) : gardée dans
    /// la base à la première utilisation, elle dit plus tard si c'est la même clé, sans rien révéler d'elle.
    fn key_check(&self) -> String;
}

/// Construit un scellé à partir d'une clé lue.
pub trait SealerFactory: Send + Sync {
    /// `Err` seulement si la bibliothèque refuse la clé (impossible pour une clé de la bonne taille).
    fn build(&self, key: ServicesKey) -> Result<Arc<dyn SecretSealer>, SealError>;
}

/// Ce que la base sait de la clé : si des secrets sont scellés, et la vérification gardée.
#[async_trait]
pub trait KeyCheckRepo: Send + Sync {
    async fn database_facts(&self) -> Result<DatabaseFacts, StoreError>;

    async fn stored_key_check(&self) -> Result<Option<String>, StoreError>;

    /// Garde la vérification si la base n'en a pas ; ne remplace JAMAIS une vérification existante.
    /// Rend celle qui est gardée après l'appel.
    async fn store_key_check(&self, check: &str) -> Result<String, StoreError>;
}
