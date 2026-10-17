use crate::domain::services::ports::Listener;
use hearth_proto::api::deploy::DeployProtocol;

/// Les ports que les programmes du serveur écoutent (HRT-68, BR-DEPLOY-008). Lecture seule.
pub trait HostPorts: Send + Sync {
    /// Les sockets en écoute (TCP et UDP, IPv4 et IPv6), sans le nom du programme. `None` : la liste n'a pas
    /// pu être lue ; la collision se découvre alors au lancement (BR-DEPLOY-008) et l'agent ne prétend rien.
    fn listening(&self) -> Option<Vec<Listener>>;

    /// Le programme qui écoute ce port. Cherché pour un port en conflit seulement : parcourir les processus
    /// coûte. `None` : inconnu.
    fn program_of(&self, port: u16, protocol: DeployProtocol) -> Option<String>;
}
