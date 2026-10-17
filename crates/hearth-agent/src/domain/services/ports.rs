//! Qui tient un port du serveur, et quel port proposer à la place (HRT-68, BR-DEPLOY-007 à BR-DEPLOY-009,
//! BR-ADOPT-022).
//!
//! Les ports des conteneurs viennent du moteur (arrêtés compris) ; les programmes du serveur viennent des
//! sockets en écoute (`/proc/net`), leur nom n'étant cherché que pour un port en conflit. Un écouteur
//! `docker-proxy` est celui d'un conteneur : il est compté avec le conteneur, jamais comme un programme.
//! Un conteneur que Hearth ne gère pas n'est jamais nommé (BR-ADOPT-022).

use std::collections::HashSet;

use hearth_proto::api::deploy::{DeployPort, DeployProtocol, PortStatus};

use super::limits::bounded;

/// Le nom du processus que Docker pose pour publier un port : il appartient au conteneur.
pub const DOCKER_PROXY: &str = "docker-proxy";
/// Combien de numéros l'on essaie au plus pour proposer un port libre, dans chaque sens.
const SUGGESTION_SPAN: u32 = 4_096;

/// Un socket en écoute sur le serveur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listener {
    pub port: u16,
    pub protocol: DeployProtocol,
    /// Le programme, quand l'agent a pu le lire (jamais pour un port qui ne gêne personne).
    pub program: Option<String>,
}

/// Qui tient un port dans le moteur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Claimant {
    /// Un service publié (créé par Hearth ou adopté) : son nom est déjà dans la liste.
    Service(String),
    /// Un conteneur hors Hearth non adopté : jamais nommé.
    Unmanaged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    pub claimant: Claimant,
    /// Le conteneur tourne : le port est vraiment tenu. Arrêté, il est seulement réservé.
    pub running: bool,
}

/// Ce que le serveur dit des ports, à cet instant.
pub trait PortWorld {
    /// Le conteneur qui publie ce port (celui qui tourne de préférence, sinon un arrêté).
    fn claim(&self, port: u16, protocol: DeployProtocol) -> Option<Claim>;
    /// Un programme du serveur qui écoute ce port (sans son nom : le chercher coûte).
    fn listener(&self, port: u16, protocol: DeployProtocol) -> Option<Listener>;
    /// Le nom du programme qui écoute ce port, cherché pour un port en conflit seulement.
    fn program(&self, port: u16, protocol: DeployProtocol) -> Option<String> {
        self.listener(port, protocol)
            .and_then(|listener| listener.program)
    }
    /// Le port de l'agent lui-même (TCP).
    fn agent_port(&self) -> u16;
}

/// Le constat sur un port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub status: PortStatus,
    pub holder: Option<String>,
}

impl Verdict {
    fn bare(status: PortStatus) -> Self {
        Self {
            status,
            holder: None,
        }
    }

    /// Le port bloque le lancement quoi que l'utilisateur coche. `ReservedByStopped` ne bloque qu'avec la
    /// case non cochée (BR-DEPLOY-008) : c'est l'appelant qui le sait.
    pub fn blocks(&self) -> bool {
        !matches!(
            self.status,
            PortStatus::Free | PortStatus::ReservedByStopped
        )
    }
}

/// Le constat sur un port du serveur, programme nommé. `dup` : un autre port du formulaire a ce numéro et ce
/// protocole.
pub fn verdict(world: &dyn PortWorld, port: u16, protocol: DeployProtocol, dup: bool) -> Verdict {
    verdict_with(world, port, protocol, dup, true)
}

/// Comme [`verdict`] ; `named: false` ne cherche pas le nom du programme (la proposition d'un port libre teste
/// des milliers de numéros : seul le statut compte).
fn verdict_with(
    world: &dyn PortWorld,
    port: u16,
    protocol: DeployProtocol,
    dup: bool,
    named: bool,
) -> Verdict {
    if port == 0 || dup {
        return Verdict::bare(PortStatus::Invalid);
    }
    if protocol == DeployProtocol::Tcp && port == world.agent_port() {
        return Verdict::bare(PortStatus::AgentPort);
    }
    match world.claim(port, protocol) {
        Some(Claim {
            claimant,
            running: true,
        }) => {
            return match claimant {
                Claimant::Service(name) => Verdict {
                    status: PortStatus::HeldByService,
                    holder: Some(name),
                },
                Claimant::Unmanaged => Verdict::bare(PortStatus::HeldByUnmanaged),
            };
        }
        Some(Claim {
            claimant,
            running: false,
        }) => {
            // Un conteneur arrêté réserve le port ; un programme qui l'écoute en plus passe avant.
            if world.listener(port, protocol).is_some() {
                return held_by_program(world, port, protocol, named);
            }
            return Verdict {
                status: PortStatus::ReservedByStopped,
                holder: match claimant {
                    Claimant::Service(name) => Some(name),
                    Claimant::Unmanaged => None,
                },
            };
        }
        None => {}
    }
    match world.listener(port, protocol) {
        Some(_) => held_by_program(world, port, protocol, named),
        None => Verdict::bare(PortStatus::Free),
    }
}

/// Un programme écoute ce port. S'il s'appelle `docker-proxy`, c'est un conteneur que le moteur ne liste pas (ou
/// plus) : jamais un programme du serveur, et jamais nommé (BR-DEPLOY-008, BR-ADOPT-022).
fn held_by_program(
    world: &dyn PortWorld,
    port: u16,
    protocol: DeployProtocol,
    named: bool,
) -> Verdict {
    let program = if named {
        world.program(port, protocol)
    } else {
        None
    };
    match program.as_deref() {
        Some(DOCKER_PROXY) => Verdict::bare(PortStatus::HeldByUnmanaged),
        Some(name) => Verdict {
            status: PortStatus::HeldByProgram,
            holder: Some(bounded(name, 64)).filter(|name| !name.is_empty()),
        },
        None => Verdict::bare(PortStatus::HeldByProgram),
    }
}

/// Le plus petit port libre au-dessus de `port`, à défaut le plus grand libre en dessous (BR-DEPLOY-009).
/// Libre : ni dans le formulaire (`in_form`), ni tenu ni réservé, ni écouté. Essaie au plus
/// [`SUGGESTION_SPAN`] numéros de chaque côté.
pub fn suggest(
    world: &dyn PortWorld,
    port: u16,
    protocol: DeployProtocol,
    in_form: &HashSet<(u16, DeployProtocol)>,
) -> Option<u16> {
    let free = |candidate: u32| -> Option<u16> {
        let candidate = u16::try_from(candidate).ok().filter(|c| *c >= 1)?;
        (!in_form.contains(&(candidate, protocol))
            && verdict_with(world, candidate, protocol, false, false).status == PortStatus::Free)
            .then_some(candidate)
    };
    let from = u32::from(port);
    (from + 1..=(from + SUGGESTION_SPAN).min(65_535))
        .find_map(free)
        .or_else(|| {
            (from.saturating_sub(SUGGESTION_SPAN).max(1)..from)
                .rev()
                .find_map(free)
        })
}

/// Les doublons d'un formulaire : (numéro, protocole) qui reviennent.
pub fn duplicates(ports: &[DeployPort]) -> HashSet<(u16, DeployProtocol)> {
    let mut seen = HashSet::new();
    let mut dup = HashSet::new();
    for port in ports {
        if !seen.insert((port.host_port, port.protocol)) {
            dup.insert((port.host_port, port.protocol));
        }
    }
    dup
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[derive(Default)]
    struct World {
        claims: HashMap<(u16, DeployProtocol), Claim>,
        listeners: HashMap<(u16, DeployProtocol), Listener>,
        agent: u16,
    }

    impl PortWorld for World {
        fn claim(&self, port: u16, protocol: DeployProtocol) -> Option<Claim> {
            self.claims.get(&(port, protocol)).cloned()
        }
        fn listener(&self, port: u16, protocol: DeployProtocol) -> Option<Listener> {
            self.listeners.get(&(port, protocol)).cloned()
        }
        fn agent_port(&self) -> u16 {
            self.agent
        }
    }

    fn world() -> World {
        World {
            agent: 7341,
            ..World::default()
        }
    }

    use DeployProtocol::{Tcp, Udp};

    #[test]
    fn a_free_port_is_free() {
        assert_eq!(verdict(&world(), 8080, Tcp, false).status, PortStatus::Free);
    }

    #[test]
    fn the_agent_port_is_refused_for_tcp_only() {
        let world = world();
        assert_eq!(
            verdict(&world, 7341, Tcp, false).status,
            PortStatus::AgentPort
        );
        assert_eq!(verdict(&world, 7341, Udp, false).status, PortStatus::Free);
    }

    #[test]
    fn a_running_service_is_named_and_an_unmanaged_container_never_is() {
        let mut world = world();
        world.claims.insert(
            (8080, Tcp),
            Claim {
                claimant: Claimant::Service("web".into()),
                running: true,
            },
        );
        world.claims.insert(
            (9090, Tcp),
            Claim {
                claimant: Claimant::Unmanaged,
                running: true,
            },
        );
        let named = verdict(&world, 8080, Tcp, false);
        assert_eq!(
            (named.status, named.holder.as_deref()),
            (PortStatus::HeldByService, Some("web"))
        );
        let hidden = verdict(&world, 9090, Tcp, false);
        assert_eq!(
            (hidden.status, hidden.holder.clone()),
            (PortStatus::HeldByUnmanaged, None)
        );
        assert!(named.blocks() && hidden.blocks());
    }

    #[test]
    fn a_stopped_container_only_reserves_the_port() {
        let mut world = world();
        world.claims.insert(
            (8080, Tcp),
            Claim {
                claimant: Claimant::Service("old".into()),
                running: false,
            },
        );
        world.claims.insert(
            (8081, Tcp),
            Claim {
                claimant: Claimant::Unmanaged,
                running: false,
            },
        );
        let reserved = verdict(&world, 8080, Tcp, false);
        assert_eq!(reserved.status, PortStatus::ReservedByStopped);
        assert_eq!(reserved.holder.as_deref(), Some("old"));
        assert!(!reserved.blocks());
        assert_eq!(verdict(&world, 8081, Tcp, false).holder, None);
    }

    #[test]
    fn a_program_holds_the_port_but_docker_proxy_belongs_to_its_container() {
        let mut world = world();
        world.listeners.insert(
            (22, Tcp),
            Listener {
                port: 22,
                protocol: Tcp,
                program: Some("sshd".into()),
            },
        );
        world.listeners.insert(
            (8080, Tcp),
            Listener {
                port: 8080,
                protocol: Tcp,
                program: Some(DOCKER_PROXY.into()),
            },
        );
        let ssh = verdict(&world, 22, Tcp, false);
        assert_eq!(
            (ssh.status, ssh.holder.as_deref()),
            (PortStatus::HeldByProgram, Some("sshd"))
        );
        // Un écouteur `docker-proxy` sans conteneur connu est un conteneur que le moteur ne liste pas : jamais
        // un programme du serveur, jamais nommé.
        let proxied = verdict(&world, 8080, Tcp, false);
        assert_eq!(
            (proxied.status, proxied.holder),
            (PortStatus::HeldByUnmanaged, None)
        );
    }

    #[test]
    fn a_duplicate_in_the_form_or_port_zero_is_invalid() {
        let world = world();
        assert_eq!(verdict(&world, 8080, Tcp, true).status, PortStatus::Invalid);
        assert_eq!(verdict(&world, 0, Tcp, false).status, PortStatus::Invalid);
        let ports = [
            DeployPort {
                host_port: 80,
                container_port: 80,
                protocol: Tcp,
            },
            DeployPort {
                host_port: 80,
                container_port: 81,
                protocol: Tcp,
            },
            DeployPort {
                host_port: 80,
                container_port: 80,
                protocol: Udp,
            },
        ];
        assert_eq!(duplicates(&ports), HashSet::from([(80, Tcp)]));
    }

    #[test]
    fn the_suggestion_is_the_smallest_free_port_above_then_the_largest_below() {
        let mut world = world();
        for port in [8080, 8081, 8082] {
            world.claims.insert(
                (port, Tcp),
                Claim {
                    claimant: Claimant::Unmanaged,
                    running: true,
                },
            );
        }
        let none = HashSet::new();
        assert_eq!(suggest(&world, 8080, Tcp, &none), Some(8083));
        // Ce que le formulaire prend déjà n'est pas libre.
        let form = HashSet::from([(8083, Tcp)]);
        assert_eq!(suggest(&world, 8080, Tcp, &form), Some(8084));
        // Au sommet de l'échelle : le plus grand libre en dessous.
        world.claims.insert(
            (65_535, Tcp),
            Claim {
                claimant: Claimant::Unmanaged,
                running: true,
            },
        );
        assert_eq!(suggest(&world, 65_535, Tcp, &none), Some(65_534));
    }

    #[test]
    fn a_port_reserved_by_a_stopped_service_is_not_suggested() {
        let mut world = world();
        world.claims.insert(
            (8081, Tcp),
            Claim {
                claimant: Claimant::Service("old".into()),
                running: false,
            },
        );
        world.claims.insert(
            (8080, Tcp),
            Claim {
                claimant: Claimant::Unmanaged,
                running: true,
            },
        );
        assert_eq!(suggest(&world, 8080, Tcp, &HashSet::new()), Some(8082));
    }
}
