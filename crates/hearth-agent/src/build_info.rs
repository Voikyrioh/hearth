//! La version de cette construction, **en un seul endroit** : `GET /hello`, `--version`,
//! l'installation et la mise à jour la lisent ici. C'est la version du crate, sauf si la variable
//! `HEARTH_AGENT_VERSION` est donnée à la construction (tests de bout en bout seulement : fabriquer
//! une « version suivante » sans toucher au dépôt ; une construction de publication ne la définit
//! pas).

/// Les adresses locales et privées sont permises pour le téléchargement d'une mise à jour. Faux
/// dans toute construction de publication ; `HEARTH_UPDATE_ALLOW_LOCAL_ADDRESSES` n'est donné qu'à
/// la construction des tests de bout en bout (le serveur de versions y est sur le bouclage).
pub const ALLOW_LOCAL_DOWNLOADS: bool =
    option_env!("HEARTH_UPDATE_ALLOW_LOCAL_ADDRESSES").is_some();

/// Ce qui distingue cette construction d'une construction de publication (vide pour une
/// publication) : clé de test, adresses locales permises, version imposée.
pub const BUILD_NOTES: &str = env!("HEARTH_BUILD_NOTES");

/// La version de l'agent (`0.1.0`).
pub const VERSION: &str = match option_env!("HEARTH_AGENT_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};
