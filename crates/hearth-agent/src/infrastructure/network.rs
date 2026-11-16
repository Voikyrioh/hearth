//! Adaptateur du port `NetworkAddresses` (HRT-223, BR-JEU-043, ADR-0060) : l'adresse du réseau maison lue sur la machine,
//! l'adresse d'internet demandée à la box par UPnP-IGD. Aucun service tiers n'est contacté, jamais.
//!
//! - **Réseau maison** : une socket UDP « connectée » vers une adresse de documentation ne ENVOIE rien ; le système dit
//!   seulement quelle interface il utiliserait. Seule une adresse privée est rendue.
//! - **Internet** : recherche SSDP (multidiffusion du réseau local), description de la box, requête SOAP
//!   `GetExternalIPAddress`. La réponse SSDP est HOSTILE (n'importe qui du réseau peut répondre) : l'adresse de la
//!   description doit être celle de l'émetteur de la réponse et une adresse privée, en `http`, sur un chemin sans
//!   caractère douteux ; chaque lecture est bornée en taille et en temps. Le résultat se garde 10 minutes (2 minutes
//!   quand la box n'a pas répondu).

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket as AsyncUdp};
use tokio::time::timeout;

use crate::application::ports::NetworkAddresses;

const SSDP_ADDR: &str = "239.255.255.250:1900";
const SEARCH_WAIT: Duration = Duration::from_millis(1500);
const STEP_WAIT: Duration = Duration::from_secs(2);
const MAX_BODY: usize = 64 * 1024;
const FOUND_FOR: Duration = Duration::from_secs(10 * 60);
const MISSED_FOR: Duration = Duration::from_secs(2 * 60);

/// Les services de la box qui répondent à `GetExternalIPAddress`.
const WAN_SERVICES: [&str; 4] = [
    "urn:schemas-upnp-org:service:WANIPConnection:2",
    "urn:schemas-upnp-org:service:WANIPConnection:1",
    "urn:schemas-upnp-org:service:WANPPPConnection:1",
    "urn:schemas-upnp-org:service:WANIPConnection:3",
];

/// La découverte de l'adresse d'internet : rend l'adresse dite par la box, ou rien. Remplaçable dans les essais.
type Probe = std::sync::Arc<
    dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<String>> + Send>>
        + Send
        + Sync,
>;

#[derive(Default)]
struct Known {
    value: Option<String>,
    at: Option<Instant>,
    /// Une découverte est en cours : il n'y en a jamais deux.
    running: bool,
}

pub struct SystemNetwork {
    known: std::sync::Arc<Mutex<Known>>,
    probe: Probe,
}

impl Default for SystemNetwork {
    fn default() -> Self {
        Self::with_probe(std::sync::Arc::new(|| {
            Box::pin(async {
                timeout(Duration::from_secs(6), ask_the_box())
                    .await
                    .ok()
                    .flatten()
                    .map(|ip| ip.to_string())
            })
        }))
    }
}

impl SystemNetwork {
    pub fn new() -> Self {
        Self::default()
    }

    /// Une source de découverte donnée (les essais).
    pub fn with_probe(probe: Probe) -> Self {
        Self {
            known: std::sync::Arc::default(),
            probe,
        }
    }
}

#[async_trait]
impl NetworkAddresses for SystemNetwork {
    fn lan_ip(&self) -> Option<String> {
        let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
        socket.connect("192.0.2.1:9").ok()?;
        match socket.local_addr().ok()?.ip() {
            IpAddr::V4(ip) if is_home_network(ip) => Some(ip.to_string()),
            _ => None,
        }
    }

    /// La dernière adresse connue, TOUT DE SUITE (jamais d'attente de la box). Périmée ou jamais cherchée : une seule
    /// découverte part en tâche de fond, quel que soit le nombre d'appels pendant qu'elle dure.
    async fn external_ip(&self) -> Option<String> {
        let (value, start) = {
            let mut known = self
                .known
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let keep = if known.value.is_some() {
                FOUND_FOR
            } else {
                MISSED_FOR
            };
            let stale = known.at.is_none_or(|at| at.elapsed() >= keep);
            let start = stale && !known.running;
            if start {
                known.running = true;
            }
            (known.value.clone(), start)
        };
        if start {
            let known = std::sync::Arc::clone(&self.known);
            let discovery = (self.probe)();
            tokio::spawn(async move {
                let found = discovery.await;
                let mut known = known
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                known.value = found;
                known.at = Some(Instant::now());
                known.running = false;
            });
        }
        value
    }
}

/// Une adresse du réseau d'une maison : privée (RFC 1918) seulement, jamais de lien local (169.254.0.0/16 : services
/// de métadonnées des nuages).
fn is_home_network(ip: Ipv4Addr) -> bool {
    ip.is_private()
}

/// La passerelle par défaut, lue dans la table de routage du système (`/proc/net/route`) : la route de destination
/// `00000000` marquée « passerelle ». Absente ou illisible : `None`, et rien n'est interrogé.
pub fn default_gateway(route_table: &str) -> Option<Ipv4Addr> {
    route_table.lines().skip(1).find_map(|line| {
        let columns: Vec<&str> = line.split_whitespace().collect();
        let (destination, gateway, flags) = (columns.get(1)?, columns.get(2)?, columns.get(3)?);
        let flags = u32::from_str_radix(flags, 16).ok()?;
        if *destination != "00000000" || flags & 0x2 == 0 {
            return None;
        }
        let raw = u32::from_str_radix(gateway, 16).ok()?;
        let ip = Ipv4Addr::from(raw.to_le_bytes());
        (!ip.is_unspecified()).then_some(ip)
    })
}

/// Une adresse d'internet : ni privée, ni de lien local, ni de boucle, ni du partage d'adresses des opérateurs
/// (`100.64.0.0/10`, double NAT), ni de documentation, ni non spécifiée.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_multicast()
                || (o[0] == 100 && (64..128).contains(&o[1])))
        }
        IpAddr::V6(v6) => {
            let first = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80)
        }
    }
}

async fn ask_the_box() -> Option<IpAddr> {
    let table = std::fs::read_to_string("/proc/net/route").ok()?;
    let gateway = default_gateway(&table)?;
    let (gateway, path) = discover(gateway).await?;
    let control = control_url(&fetch(gateway, &path).await?)?;
    let answer = soap(gateway, &control).await?;
    let ip: IpAddr = external_ip_of(&answer)?.parse().ok()?;
    is_public(ip).then_some(ip)
}

/// Cherche la box : la première réponse dont la description est valable. Rend son adresse et le chemin de sa description.
async fn discover(gateway: Ipv4Addr) -> Option<(SocketAddr, String)> {
    let socket = AsyncUdp::bind("0.0.0.0:0").await.ok()?;
    let request = format!(
        "M-SEARCH * HTTP/1.1\r\nHOST: {SSDP_ADDR}\r\nMAN: \"ssdp:discover\"\r\nMX: 1\r\nST: urn:schemas-upnp-org:device:InternetGatewayDevice:1\r\n\r\n"
    );
    socket.send_to(request.as_bytes(), SSDP_ADDR).await.ok()?;
    let deadline = tokio::time::Instant::now() + SEARCH_WAIT;
    let mut buffer = [0u8; 2048];
    loop {
        let (len, from) = tokio::time::timeout_at(deadline, socket.recv_from(&mut buffer))
            .await
            .ok()?
            .ok()?;
        let text = String::from_utf8_lossy(&buffer[..len]);
        if let Some(found) = location_of(&text, from, gateway) {
            return Some(found);
        }
    }
}

/// L'en-tête `LOCATION` d'une réponse SSDP, jugé : `http`, un chemin sûr, et la passerelle par défaut est À LA FOIS
/// l'émetteur de la réponse et l'hôte de la description (une autre machine du réseau ne répond pas à sa place, et une
/// réponse ne fait jamais viser une autre adresse).
pub fn location_of(
    response: &str,
    from: SocketAddr,
    gateway: Ipv4Addr,
) -> Option<(SocketAddr, String)> {
    let line = response
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with("location:"))?;
    let url = line.split_once(':')?.1.trim();
    let rest = url.strip_prefix("http://")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let addr: SocketAddr = authority.parse().ok()?;
    let IpAddr::V4(ip) = addr.ip() else {
        return None;
    };
    (from.ip() == IpAddr::V4(gateway) && ip == gateway && is_home_network(ip) && safe_path(path))
        .then(|| (addr, path.to_owned()))
}

fn safe_path(path: &str) -> bool {
    path.starts_with('/')
        && path.len() <= 256
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-~%?=&:".contains(c))
        && !path.contains("..")
}

/// Une requête HTTP/1.0 vers la box ; rend le corps (64 Kio au plus).
async fn http(addr: SocketAddr, request: String) -> Option<String> {
    let work = async {
        let mut stream = TcpStream::connect(addr).await.ok()?;
        stream.write_all(request.as_bytes()).await.ok()?;
        let mut raw = Vec::new();
        stream
            .take((MAX_BODY + 4096) as u64)
            .read_to_end(&mut raw)
            .await
            .ok()?;
        let text = String::from_utf8_lossy(&raw).into_owned();
        let (head, body) = text.split_once("\r\n\r\n")?;
        head.starts_with("HTTP/1.")
            .then(|| head.split_whitespace().nth(1) == Some("200"))
            .filter(|ok| *ok)?;
        (body.len() <= MAX_BODY).then(|| body.to_owned())
    };
    timeout(STEP_WAIT, work).await.ok().flatten()
}

async fn fetch(addr: SocketAddr, path: &str) -> Option<String> {
    http(
        addr,
        format!("GET {path} HTTP/1.0\r\nHost: {addr}\r\nConnection: close\r\n\r\n"),
    )
    .await
}

/// Le chemin de commande du service de connexion à internet, dans la description de la box.
pub fn control_url(description: &str) -> Option<(String, String)> {
    for service in WAN_SERVICES {
        let Some(at) = description.find(&format!("<serviceType>{service}</serviceType>")) else {
            continue;
        };
        let after = &description[at..];
        let start = after.find("<controlURL>")? + "<controlURL>".len();
        let end = after[start..].find("</controlURL>")? + start;
        let url = after[start..end].trim();
        // Seul le chemin compte : la box est toujours jointe à l'adresse qui a répondu.
        let path = url
            .strip_prefix("http://")
            .map_or(url, |rest| rest.find('/').map_or("/", |i| &rest[i..]));
        if safe_path(path) {
            return Some((path.to_owned(), service.to_owned()));
        }
    }
    None
}

async fn soap(addr: SocketAddr, (path, service): &(String, String)) -> Option<String> {
    let body = format!(
        "<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body><u:GetExternalIPAddress xmlns:u=\"{service}\"/></s:Body></s:Envelope>"
    );
    http(
        addr,
        format!(
            "POST {path} HTTP/1.0\r\nHost: {addr}\r\nContent-Type: text/xml; charset=\"utf-8\"\r\nSOAPAction: \"{service}#GetExternalIPAddress\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ),
    )
    .await
}

/// L'adresse dans la réponse SOAP.
pub fn external_ip_of(answer: &str) -> Option<&str> {
    let start = answer.find("<NewExternalIPAddress>")? + "<NewExternalIPAddress>".len();
    let end = answer[start..].find("</NewExternalIPAddress>")? + start;
    Some(answer[start..end].trim())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener;

    fn from(ip: &str) -> SocketAddr {
        format!("{ip}:1900").parse().unwrap()
    }

    const GATEWAY: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 1);

    #[test]
    fn only_the_default_gateway_is_believed_and_followed() {
        let ok = "HTTP/1.1 200 OK
LOCATION: http://192.168.1.1:5000/rootDesc.xml
ST: x

";
        assert_eq!(
            location_of(ok, from("192.168.1.1"), GATEWAY),
            Some((
                "192.168.1.1:5000".parse().unwrap(),
                "/rootDesc.xml".to_owned()
            ))
        );
        // Un autre émetteur du réseau (même désignant la box) : refusé.
        assert_eq!(location_of(ok, from("192.168.1.50"), GATEWAY), None);
        // Un émetteur qui est la passerelle mais désigne une autre machine, ou le service de métadonnées : refusé.
        let elsewhere = "LOCATION: http://192.168.1.50:5000/d.xml
";
        assert_eq!(location_of(elsewhere, from("192.168.1.1"), GATEWAY), None);
        let metadata = "LOCATION: http://169.254.169.254:80/latest/meta-data
";
        assert_eq!(
            location_of(metadata, from("169.254.169.254"), GATEWAY),
            None
        );
        let outside = "LOCATION: http://93.184.216.34:80/x.xml
";
        assert_eq!(location_of(outside, from("93.184.216.34"), GATEWAY), None);
        let https = "LOCATION: https://192.168.1.1/x.xml
";
        assert_eq!(location_of(https, from("192.168.1.1"), GATEWAY), None);
        let odd = "LOCATION: http://192.168.1.1:80/../etc;x
";
        assert_eq!(location_of(odd, from("192.168.1.1"), GATEWAY), None);
        // Le lien local n'est jamais un réseau maison.
        assert!(!is_home_network(Ipv4Addr::new(169, 254, 169, 254)));
    }

    #[test]
    fn the_default_gateway_is_read_from_the_routing_table_or_nothing() {
        let table = "Iface	Destination	Gateway	Flags	RefCnt	Use	Metric	Mask	MTU	Window	IRTT
            eth0	00000000	0101A8C0	0003	0	0	100	00000000	0	0	0
            eth0	0001A8C0	00000000	0001	0	0	100	00FFFFFF	0	0	0
";
        assert_eq!(default_gateway(table), Some(GATEWAY));
        // Aucune route par défaut, ou illisible : rien n'est interrogé.
        let none = "Iface	Destination	Gateway	Flags
eth0	0001A8C0	00000000	0001
";
        assert_eq!(default_gateway(none), None);
        assert_eq!(default_gateway(""), None);
        assert_eq!(
            default_gateway(
                "Iface
n'importe quoi"
            ),
            None
        );
    }

    #[test]
    fn the_wan_service_and_the_external_address_are_read_from_the_box_answers() {
        let description = "<service><serviceType>urn:schemas-upnp-org:service:WANCommonInterfaceConfig:1</serviceType><controlURL>/ctl/CmnIfCfg</controlURL></service><service><serviceType>urn:schemas-upnp-org:service:WANIPConnection:1</serviceType><controlURL>/ctl/IPConn</controlURL></service>";
        assert_eq!(
            control_url(description),
            Some((
                "/ctl/IPConn".to_owned(),
                "urn:schemas-upnp-org:service:WANIPConnection:1".to_owned()
            ))
        );
        assert_eq!(control_url("<root/>"), None);
        let answer = "<s:Envelope><s:Body><u:GetExternalIPAddressResponse><NewExternalIPAddress>203.0.113.9</NewExternalIPAddress></u:GetExternalIPAddressResponse></s:Body></s:Envelope>";
        assert_eq!(external_ip_of(answer), Some("203.0.113.9"));
    }

    #[test]
    fn a_private_shared_or_local_address_is_never_taken_for_the_internet_one() {
        for text in [
            "192.168.1.20",
            "10.0.0.2",
            "172.16.5.4",
            "100.64.1.1",
            "127.0.0.1",
            "169.254.1.1",
            "0.0.0.0",
            "fe80::1",
            "fd00::1",
        ] {
            assert!(!is_public(text.parse().unwrap()), "{text}");
        }
        for text in ["93.184.216.34", "81.2.69.160", "2a00:1450::1"] {
            assert!(is_public(text.parse().unwrap()), "{text}");
        }
    }

    /// Une fausse box sur la boucle locale : la requête SOAP est bien faite et la réponse est lue.
    #[tokio::test]
    async fn the_soap_exchange_asks_the_box_and_reads_its_answer() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = vec![0u8; 4096];
            let len = socket.read(&mut buffer).await.unwrap();
            let request = String::from_utf8_lossy(&buffer[..len]).into_owned();
            let reply = "<NewExternalIPAddress>203.0.113.9</NewExternalIPAddress>";
            let response = format!(
                "HTTP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n{reply}",
                reply.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            request
        });
        let control = (
            "/ctl/IPConn".to_owned(),
            "urn:schemas-upnp-org:service:WANIPConnection:1".to_owned(),
        );
        let answer = soap(addr, &control).await.expect("réponse");
        assert_eq!(external_ip_of(&answer), Some("203.0.113.9"));
        let request = server.await.unwrap();
        assert!(
            request.starts_with("POST /ctl/IPConn HTTP/1.0"),
            "{request}"
        );
        assert!(request.contains("#GetExternalIPAddress"), "{request}");
    }

    #[tokio::test]
    async fn a_box_that_answers_an_error_or_nothing_gives_nothing() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = vec![0u8; 1024];
            let _ = socket.read(&mut buffer).await;
            socket
                .write_all(b"HTTP/1.0 500 Internal Server Error\r\n\r\nno")
                .await
                .unwrap();
        });
        assert_eq!(fetch(addr, "/x").await, None);
    }

    /// HRT-223 : N requêtes simultanées ne lancent qu'UNE découverte ; chacune reçoit tout de suite la dernière valeur
    /// connue (rien au début), jamais l'attente de la box.
    #[tokio::test]
    async fn many_simultaneous_requests_start_one_discovery_and_never_wait_for_it() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let started = std::sync::Arc::new(AtomicUsize::new(0));
        let gate = std::sync::Arc::new(tokio::sync::Notify::new());
        let (counter, release) = (started.clone(), gate.clone());
        let network =
            std::sync::Arc::new(SystemNetwork::with_probe(std::sync::Arc::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
                let release = release.clone();
                Box::pin(async move {
                    release.notified().await;
                    Some("203.0.113.9".to_owned())
                })
            })));
        let calls: Vec<_> = (0..20)
            .map(|_| {
                let network = network.clone();
                tokio::spawn(async move { network.external_ip().await })
            })
            .collect();
        for call in calls {
            // La découverte est bloquée : aucun appel n'attend, tous rendent « rien ».
            assert_eq!(call.await.unwrap(), None);
        }
        assert_eq!(started.load(Ordering::SeqCst), 1);
        gate.notify_one();
        let mut seen = None;
        for _ in 0..100 {
            seen = network.external_ip().await;
            if seen.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(seen.as_deref(), Some("203.0.113.9"));
        // La valeur est fraîche : plus aucune découverte.
        assert_eq!(started.load(Ordering::SeqCst), 1);
    }
}
