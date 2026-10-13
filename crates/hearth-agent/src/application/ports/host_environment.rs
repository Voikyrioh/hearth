/// Ce que l'agent sait du serveur pour adapter l'aide sur le moteur de conteneurs (`BR-ENGINE-005`,
/// `BR-ENGINE-007`, `BR-ENGINE-008`). Aucune commande ni texte d'aide ici : c'est l'interface qui les
/// porte, d'après ces faits.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HostDescription {
    /// `ID` de `/etc/os-release` (`debian`, `nixos`…), vide si inconnu.
    pub os_id: String,
    /// `ID_LIKE` de `/etc/os-release`.
    pub os_like: Vec<String>,
    /// Le système d'init est systemd.
    pub systemd: bool,
    /// Le serveur est configuré de façon déclarative (NixOS).
    pub declarative: bool,
    /// Compte sous lequel l'agent tourne ; vide si l'agent ne le sait pas.
    pub agent_account: String,
    /// Ce compte est l'administrateur du serveur (root).
    pub agent_is_admin: bool,
}

/// Lit ce que l'agent sait de son serveur.
pub trait HostEnvironment: Send + Sync {
    /// Lu une fois par l'appelant : ces faits ne changent pas pendant la vie de l'agent.
    fn describe(&self) -> HostDescription;
}
