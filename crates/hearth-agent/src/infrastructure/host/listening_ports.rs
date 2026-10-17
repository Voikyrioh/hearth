//! Les ports écoutés par les programmes du serveur (HRT-68, BR-DEPLOY-008, BR-DEPLOY-009) : `/proc/net/tcp`,
//! `tcp6`, `udp`, `udp6`, lus en lecture seule. Le nom du programme (`/proc/<pid>/comm`) n'est cherché que
//! pour un port en conflit, par le numéro d'inode du socket.
//!
//! Le comportement sur un vrai noyau est prouvé par la CI Linux (`tests/host_ports_linux.rs`) ; le poste de
//! développement Windows n'exécute que l'analyse des fichiers.

use std::collections::HashSet;
use std::fs;

use hearth_proto::api::deploy::DeployProtocol;

use crate::application::ports::HostPorts;
use crate::domain::services::ports::Listener;

/// Un socket en écoute : le port et le numéro d'inode (pour retrouver le programme).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Socket {
    pub port: u16,
    pub inode: u64,
}

/// Les sockets en écoute d'un fichier `/proc/net/{tcp,tcp6,udp,udp6}`. TCP : état `0A` (LISTEN). UDP : état
/// `07` (non connecté). Une ligne mal formée est ignorée.
pub fn parse_proc_net(text: &str, protocol: DeployProtocol) -> Vec<Socket> {
    let wanted = match protocol {
        DeployProtocol::Tcp => "0A",
        DeployProtocol::Udp => "07",
    };
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            // sl local_address rem_address st tx:rx tr:when retrnsmt uid timeout inode
            let (local, state, inode) = (fields.get(1)?, fields.get(3)?, fields.get(9)?);
            if *state != wanted {
                return None;
            }
            let port = u16::from_str_radix(local.rsplit_once(':')?.1, 16).ok()?;
            Some(Socket {
                port,
                inode: inode.parse().ok()?,
            })
        })
        .collect()
}

const FILES: [(&str, DeployProtocol); 4] = [
    ("/proc/net/tcp", DeployProtocol::Tcp),
    ("/proc/net/tcp6", DeployProtocol::Tcp),
    ("/proc/net/udp", DeployProtocol::Udp),
    ("/proc/net/udp6", DeployProtocol::Udp),
];

/// Le serveur réel.
#[derive(Debug, Clone, Default)]
pub struct SystemHostPorts;

impl SystemHostPorts {
    pub fn new() -> Self {
        Self
    }
}

impl HostPorts for SystemHostPorts {
    fn listening(&self) -> Option<Vec<Listener>> {
        let mut seen = HashSet::new();
        let mut listeners = Vec::new();
        let mut read_any = false;
        for (path, protocol) in FILES {
            // IPv6 peut être absent d'un noyau : un fichier manquant n'est pas une panne, aucun fichier lisible l'est.
            let Ok(text) = fs::read_to_string(path) else {
                continue;
            };
            read_any = true;
            for socket in parse_proc_net(&text, protocol) {
                if seen.insert((socket.port, protocol)) {
                    listeners.push(Listener {
                        port: socket.port,
                        protocol,
                        program: None,
                    });
                }
            }
        }
        read_any.then_some(listeners)
    }

    fn program_of(&self, port: u16, protocol: DeployProtocol) -> Option<String> {
        let mut inodes = HashSet::new();
        for (path, file_protocol) in FILES {
            if file_protocol != protocol {
                continue;
            }
            if let Ok(text) = fs::read_to_string(path) {
                inodes.extend(
                    parse_proc_net(&text, protocol)
                        .into_iter()
                        .filter(|socket| socket.port == port)
                        .map(|socket| socket.inode),
                );
            }
        }
        if inodes.is_empty() {
            return None;
        }
        for entry in fs::read_dir("/proc").ok()?.flatten() {
            let name = entry.file_name();
            let Some(pid) = name
                .to_str()
                .filter(|n| n.bytes().all(|b| b.is_ascii_digit()))
            else {
                continue;
            };
            let Ok(fds) = fs::read_dir(format!("/proc/{pid}/fd")) else {
                continue;
            };
            for fd in fds.flatten() {
                let Ok(target) = fs::read_link(fd.path()) else {
                    continue;
                };
                let Some(inode) = target
                    .to_str()
                    .and_then(|t| t.strip_prefix("socket:["))
                    .and_then(|t| t.strip_suffix(']'))
                    .and_then(|t| t.parse::<u64>().ok())
                else {
                    continue;
                };
                if inodes.contains(&inode) {
                    let comm = fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
                    let comm = comm.trim();
                    return (!comm.is_empty()).then(|| comm.to_owned());
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TCP: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 12345 1 0000000000000000 100 0 0 10 0
   1: 0100007F:0035 00000000:0000 0A 00000000:00000000 00:00000000 00000000   101        0 22222 1 0000000000000000 100 0 0 10 0
   2: 0100007F:C350 0100007F:1F90 01 00000000:00000000 00:00000000 00000000  1000        0 33333 1 0000000000000000 20 4 30 10 -1
   garbage line
";

    const UDP: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode ref pointer drops
  100: 00000000:0044 00000000:0000 07 00000000:00000000 00:00000000 00000000     0        0 44444 2 0000000000000000 0
  101: 0100007F:E000 0100007F:0035 01 00000000:00000000 00:00000000 00000000     0        0 55555 2 0000000000000000 0
";

    #[test]
    fn only_listening_tcp_sockets_count_and_a_bad_line_is_ignored() {
        let sockets = parse_proc_net(TCP, DeployProtocol::Tcp);
        assert_eq!(
            sockets,
            [
                Socket {
                    port: 0x1F90,
                    inode: 12345
                },
                Socket {
                    port: 0x0035,
                    inode: 22222
                },
            ]
        );
    }

    #[test]
    fn only_unconnected_udp_sockets_count() {
        assert_eq!(
            parse_proc_net(UDP, DeployProtocol::Udp),
            [Socket {
                port: 0x0044,
                inode: 44444
            }]
        );
    }

    #[test]
    fn an_empty_file_has_no_sockets() {
        assert!(parse_proc_net("", DeployProtocol::Tcp).is_empty());
        assert!(parse_proc_net("header only\n", DeployProtocol::Udp).is_empty());
    }
}
