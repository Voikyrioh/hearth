//! Port `NetworkAddresses` : les adresses sous lesquelles les joueurs joignent la machine (HRT-223, BR-JEU-043).
//!
//! Deux adresses, jamais devinées par un service tiers : celle du réseau maison (lue sur la machine, aucun paquet
//! envoyé) et celle d'internet (demandée à la box du foyer par UPnP, `GetExternalIPAddress`, aucun service d'écho d'IP).

use async_trait::async_trait;

#[async_trait]
pub trait NetworkAddresses: Send + Sync {
    /// L'adresse IPv4 privée de la machine sur le réseau maison ; `None` si elle n'en a pas.
    fn lan_ip(&self) -> Option<String>;

    /// L'adresse IP publique que la box dit avoir ; `None` si la box ne répond pas (pas d'UPnP, double NAT, adresse
    /// non publique). Le résultat est gardé quelques minutes : un échec ne bloque jamais l'appelant plus que le délai.
    async fn external_ip(&self) -> Option<String>;
}
