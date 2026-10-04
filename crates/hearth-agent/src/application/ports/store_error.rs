use thiserror::Error;

/// Erreur d'un dépôt. Nomme toujours la ressource concernée ; ne contient jamais de secret.
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("stockage « {resource} » indisponible : {message}")]
    Unavailable {
        resource: &'static str,
        message: String,
    },
    /// Une contrainte d'unicité est violée (insertion concurrente du même identifiant).
    #[error("« {resource} » existe déjà")]
    Duplicate { resource: &'static str },
}
