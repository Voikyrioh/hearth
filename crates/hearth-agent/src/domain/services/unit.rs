//! Unités systemd qui possèdent un conteneur (HRT-63, ADR-0037) : règles pures de la détection du
//! superviseur, sans lecture de `/proc` ni lancement de commande (les adaptateurs s'en chargent).
//!
//! Un nom d'unité vient du système (le groupe de contrôle d'un processus) : il est VALIDÉ avant d'aller
//! où que ce soit (clé de service, argument de `systemctl`, base). Le jeu de caractères est celui de
//! `systemd.unit(5)` ; un nom qui n'y tient pas n'est pas une unité pour Hearth.

use std::collections::{HashMap, HashSet};
use std::fmt;

/// Longueur maximale d'un nom d'unité, suffixe compris (limite de systemd : 255).
pub const MAX_UNIT_NAME_CHARS: usize = 255;
/// Suffixe des seules unités que Hearth reconnaît : les services.
const SERVICE_SUFFIX: &str = ".service";
/// Nombre maximal de conteneurs désignés par une même ligne de commande.
const MAX_REFS: usize = 8;
/// Longueur maximale du nom ou de l'identifiant d'un conteneur désigné par une ligne de commande.
const MAX_REF_CHARS: usize = 255;

/// Le nom d'une unité de service systemd, validé : caractères de `systemd.unit(5)` (lettres, chiffres,
/// `:`, `-`, `_`, `.`, `\`, `@`), 255 caractères au plus, suffixe `.service`, un début qui n'est ni un
/// tiret (une option pour `systemctl`) ni un point.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnitName(String);

impl UnitName {
    pub fn parse(text: &str) -> Option<Self> {
        let stem = text.strip_suffix(SERVICE_SUFFIX)?;
        let first = stem.chars().next()?;
        if text.len() > MAX_UNIT_NAME_CHARS || first == '-' || first == '.' {
            return None;
        }
        let allowed =
            |c: char| c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '_' | '.' | '\\' | '@');
        text.chars().all(allowed).then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for UnitName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UnitName({})", self.0)
    }
}

impl fmt::Display for UnitName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// `ActiveState` d'une unité.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitActivity {
    Active,
    /// Démarrage en cours, ou attente d'une relance (`SubState=auto-restart`).
    Activating,
    Deactivating,
    Inactive,
    Failed,
    /// Reload, maintenance ou une valeur que Hearth ne connaît pas.
    Other,
}

impl UnitActivity {
    fn parse(text: &str) -> Self {
        match text {
            "active" => Self::Active,
            "activating" => Self::Activating,
            "deactivating" => Self::Deactivating,
            "inactive" => Self::Inactive,
            "failed" => Self::Failed,
            _ => Self::Other,
        }
    }
}

/// Ce que `systemctl show` dit d'une unité chargée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitStatus {
    pub activity: UnitActivity,
    /// Le système attend de relancer l'unité après un arrêt (`SubState=auto-restart`).
    pub auto_restart: bool,
    /// L'unité se relance toute seule (`Restart=` autre que `no`).
    pub restarts: bool,
    /// Délai d'arrêt déclaré (`TimeoutStopUSec`), en secondes ; `None` : illimité ou illisible.
    pub timeout_stop_s: Option<u32>,
}

/// Ce que le superviseur sait d'une unité.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitLookup {
    Loaded(UnitStatus),
    /// Le système ne connaît plus cette unité (non chargée, introuvable, masquée) : le service n'existe
    /// plus pour lui.
    NotLoaded,
    /// Le superviseur ne répond pas (pas de `systemctl`, système sans systemd, délai) : on ne sait rien.
    Unavailable,
}

/// Les propriétés demandées à `systemctl show` (`--property=`), séparées par des virgules.
pub const SHOW_PROPERTIES: &str = "LoadState,ActiveState,SubState,Restart,TimeoutStopUSec,MainPID";

/// Le processus principal de l'unité (`MainPID`) d'une sortie de `systemctl show`. `0` (aucun processus)
/// et tout texte illisible donnent `None`.
pub fn parse_main_pid(text: &str) -> Option<u32> {
    text.lines()
        .find_map(|line| line.strip_prefix("MainPID="))
        .and_then(|value| value.trim().parse::<u32>().ok())
        .filter(|pid| *pid != 0)
}

/// Un client du moteur (processus `docker` ou `podman`) qui désigne un conteneur et tourne dans le groupe
/// de contrôle d'une unité de service : une REVENDICATION, pas encore une attribution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientClaim {
    /// Identifiant complet du conteneur désigné.
    pub container: String,
    pub unit: UnitName,
    /// Le processus du client.
    pub pid: u32,
}

/// Les propriétaires que les revendications établissent (HRT-63, ADR-0037). Un client du moteur qui
/// tourne dans le groupe de contrôle d'un service ne dit pas que ce service EST ce conteneur (une tâche
/// planifiée lance des conteneurs au premier plan, un exécuteur de CI aussi). Trois règles :
///
/// 1. seul le processus PRINCIPAL de l'unité (`main_pids`, lu par `systemctl show`) compte : c'est
///    l'`ExecStart` de l'unité, donc le client dont le conteneur est le service ;
/// 2. un conteneur revendiqué par deux unités n'a pas de propriétaire (ambigu) ;
/// 3. une unité qui revendique plusieurs conteneurs n'en supervise aucun : un service n'a qu'un
///    conteneur, rien n'est caché derrière une clé commune.
pub fn attribute(
    claims: &[ClientClaim],
    main_pids: &HashMap<UnitName, u32>,
) -> HashMap<String, UnitName> {
    let mut by_container: HashMap<&str, HashSet<&UnitName>> = HashMap::new();
    for claim in claims {
        if main_pids.get(&claim.unit) == Some(&claim.pid) {
            by_container
                .entry(claim.container.as_str())
                .or_default()
                .insert(&claim.unit);
        }
    }
    let mut by_unit: HashMap<&UnitName, HashSet<&str>> = HashMap::new();
    for (container, units) in &by_container {
        let mut distinct = units.iter().copied();
        if let (Some(unit), None) = (distinct.next(), distinct.next()) {
            by_unit.entry(unit).or_default().insert(*container);
        }
    }
    by_unit
        .into_iter()
        .filter(|(_, containers)| containers.len() == 1)
        .flat_map(|(unit, containers)| {
            containers
                .into_iter()
                .map(move |container| (container.to_owned(), unit.clone()))
        })
        .collect()
}

/// Lit la sortie de `systemctl show --property=<SHOW_PROPERTIES> -- <unité>` (lignes `Nom=valeur`).
/// Une sortie sans `LoadState` ou sans `ActiveState` n'est pas une réponse d'un systemd.
pub fn parse_show(text: &str) -> UnitLookup {
    let mut load = None;
    let mut active = None;
    let mut sub = "";
    let mut restart = "no";
    let mut timeout = None;
    for line in text.lines() {
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match name {
            "LoadState" => load = Some(value),
            "ActiveState" => active = Some(value),
            "SubState" => sub = value,
            "Restart" => restart = value,
            "TimeoutStopUSec" => timeout = parse_timespan(value),
            _ => {}
        }
    }
    let (Some(load), Some(active)) = (load, active) else {
        return UnitLookup::Unavailable;
    };
    if load != "loaded" {
        return UnitLookup::NotLoaded;
    }
    UnitLookup::Loaded(UnitStatus {
        activity: UnitActivity::parse(active),
        auto_restart: sub == "auto-restart",
        restarts: restart != "no" && !restart.is_empty(),
        timeout_stop_s: timeout,
    })
}

/// Une durée au format de `systemd.time(7)` (`1min 30s`, `2min`, `500ms`, `1h 30min`) en secondes
/// entières (arrondies au-dessus). `infinity` et tout texte illisible donnent `None`.
pub fn parse_timespan(text: &str) -> Option<u32> {
    let text = text.trim();
    if text.is_empty() || text == "infinity" {
        return None;
    }
    let mut seconds = 0.0_f64;
    let mut any = false;
    for token in text.split_whitespace() {
        let split = token.find(|c: char| !(c.is_ascii_digit() || c == '.'))?;
        let (number, unit) = token.split_at(split);
        let number: f64 = number.parse().ok()?;
        let factor = match unit {
            "us" | "usec" => 0.000_001,
            "ms" | "msec" => 0.001,
            "s" | "sec" | "second" | "seconds" => 1.0,
            "min" | "minute" | "minutes" => 60.0,
            "h" | "hr" | "hour" | "hours" => 3_600.0,
            "d" | "day" | "days" => 86_400.0,
            "w" | "week" | "weeks" => 604_800.0,
            "month" | "months" => 2_629_800.0,
            "y" | "year" | "years" => 31_557_600.0,
            _ => return None,
        };
        seconds += number * factor;
        any = true;
    }
    if !any || !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    // Arrondi au-dessus, borné : un délai de plus de `u32::MAX` secondes vaut « très long ».
    Some(seconds.ceil().min(f64::from(u32::MAX)) as u32)
}

/// L'unité de service d'un fichier `/proc/<pid>/cgroup`. Cgroup v2 (`0::/system.slice/x.service`) ou
/// v1 (`1:name=systemd:/system.slice/x.service`). Le premier composant du chemin qui est un service
/// fait foi ; les services du gestionnaire d'un utilisateur (`user@1000.service/...`) et tout chemin
/// qui passe par une session (`.scope`) avant son service ne sont pas des unités du système.
pub fn unit_of_cgroup(text: &str) -> Option<UnitName> {
    for line in text.lines() {
        let mut fields = line.splitn(3, ':');
        let (_, Some(controllers), Some(path)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if !(controllers.is_empty() || controllers == "name=systemd") {
            continue;
        }
        let path = path.trim_end_matches(" (deleted)");
        for component in path.split('/').filter(|part| !part.is_empty()) {
            if component.ends_with(".scope") {
                return None;
            }
            if component.ends_with(SERVICE_SUFFIX) {
                if component.starts_with("user@") || component.starts_with("user-runtime-dir@") {
                    return None;
                }
                return UnitName::parse(component);
            }
        }
    }
    None
}

/// Ce programme est-il un client du moteur (`docker`, `podman`) ? Le nom vient de `/proc/<pid>/exe` ;
/// une enveloppe (`.docker-wrapped`, packages Nix) compte comme le programme qu'elle enveloppe.
pub fn is_engine_client(program: &str) -> bool {
    let name = program.rsplit('/').next().unwrap_or(program);
    let name = name.trim_start_matches('.');
    let name = name.strip_suffix("-wrapped").unwrap_or(name);
    matches!(name, "docker" | "podman")
}

/// Options globales du client qui prennent une valeur dans l'argument suivant (`-H unix://...`).
const GLOBAL_OPTIONS_WITH_VALUE: [&str; 14] = [
    "-H",
    "--host",
    "-c",
    "--context",
    "-l",
    "--log-level",
    "--config",
    "--tlscacert",
    "--tlscert",
    "--tlskey",
    "--root",
    "--runroot",
    "--connection",
    "--url",
];

fn is_container_ref(text: &str) -> bool {
    !text.is_empty()
        && text.chars().count() <= MAX_REF_CHARS
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// Les conteneurs que la ligne de commande d'un client désigne, quand c'est un `run` (son
/// `--name`) ou un `start` (ses arguments : noms ou identifiants). Le premier argument est le
/// programme. Tout autre sous-commande, tout nom qui n'a pas la forme d'un nom de conteneur : rien.
///
/// Limite assumée : après l'image, les arguments sont ceux du conteneur ; le PREMIER `--name` fait foi,
/// et il précède toujours l'image.
pub fn client_refs(args: &[String]) -> Vec<String> {
    let mut rest = args.iter().skip(1).map(String::as_str);
    let mut subcommand = None;
    while let Some(arg) = rest.next() {
        if arg.starts_with('-') {
            if GLOBAL_OPTIONS_WITH_VALUE.contains(&arg) {
                rest.next();
            }
            continue;
        }
        if arg == "container" && subcommand.is_none() {
            subcommand = Some("container");
            continue;
        }
        subcommand = Some(arg);
        break;
    }
    let mut refs: Vec<String> = Vec::new();
    match subcommand {
        Some("run") => {
            while let Some(arg) = rest.next() {
                let name = if let Some(value) = arg.strip_prefix("--name=") {
                    Some(value)
                } else if arg == "--name" {
                    rest.next()
                } else {
                    None
                };
                if let Some(name) = name {
                    if is_container_ref(name) {
                        refs.push(name.to_owned());
                    }
                    break;
                }
            }
        }
        Some("start") => {
            while let Some(arg) = rest.next() {
                if arg == "--detach-keys" {
                    rest.next();
                } else if !arg.starts_with('-') && is_container_ref(arg) && refs.len() < MAX_REFS {
                    refs.push(arg.to_owned());
                }
            }
        }
        _ => {}
    }
    refs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn a_unit_name_is_validated_before_it_goes_anywhere() {
        for good in [
            "docker-terraria.service",
            "hearth-e2e.service",
            "app@instance.service",
            "a\\x2db.service",
            "x:y_z.service",
        ] {
            assert_eq!(
                UnitName::parse(good).map(|u| u.to_string()).as_deref(),
                Some(good)
            );
        }
        let too_long = format!("{}.service", "a".repeat(MAX_UNIT_NAME_CHARS));
        for bad in [
            "",
            ".service",
            "docker",
            "docker.socket",
            "-x.service",
            ".hidden.service",
            "a b.service",
            "a;rm -rf.service",
            "a$(x).service",
            "a\nb.service",
            "a/b.service",
            "é.service",
            "a\0b.service",
            too_long.as_str(),
        ] {
            assert!(UnitName::parse(bad).is_none(), "{bad:?}");
        }
        let longest = format!(
            "{}.service",
            "a".repeat(MAX_UNIT_NAME_CHARS - SERVICE_SUFFIX.len())
        );
        assert!(UnitName::parse(&longest).is_some());
    }

    #[test]
    fn the_unit_of_a_cgroup_is_the_first_system_service_component() {
        let cases = [
            (
                "0::/system.slice/docker-x.service\n",
                Some("docker-x.service"),
            ),
            (
                "0::/system.slice/docker-x.service/sub/leaf\n",
                Some("docker-x.service"),
            ),
            (
                "0::/hearth.slice/hearth-e2e.service (deleted)\n",
                Some("hearth-e2e.service"),
            ),
            (
                "12:cpu,cpuacct:/user.slice\n1:name=systemd:/system.slice/old.service\n0::/\n",
                Some("old.service"),
            ),
            // Un terminal ou une session : pas une unité de service.
            ("0::/user.slice/user-1000.slice/session-3.scope\n", None),
            // Le gestionnaire d'un utilisateur : pas une unité du système.
            (
                "0::/user.slice/user-1000.slice/user@1000.service/app.slice/x.service\n",
                None,
            ),
            // Un service lancé depuis une session.
            ("0::/user.slice/session-3.scope/x.service\n", None),
            ("0::/\n", None),
            ("", None),
            ("garbage", None),
            // Un nom qui ne tient pas dans le jeu de caractères d'une unité.
            ("0::/system.slice/a;b.service\n", None),
        ];
        for (text, expected) in cases {
            assert_eq!(
                unit_of_cgroup(text).map(|u| u.to_string()).as_deref(),
                expected,
                "{text:?}"
            );
        }
    }

    #[test]
    fn engine_clients_are_recognised_by_program_name_wrappers_included() {
        for yes in [
            "docker",
            "/usr/bin/docker",
            "/nix/store/abc-docker-24.0.9/bin/docker",
            "/nix/store/abc-docker/bin/.docker-wrapped",
            "podman",
        ] {
            assert!(is_engine_client(yes), "{yes}");
        }
        for no in [
            "dockerd",
            "containerd",
            "docker-proxy",
            "sh",
            "",
            "/usr/bin/rundocker",
        ] {
            assert!(!is_engine_client(no), "{no}");
        }
    }

    #[test]
    fn a_run_names_its_container_with_name_in_either_form() {
        assert_eq!(client_refs(&args("docker run --name=x busybox")), vec!["x"]);
        assert_eq!(
            client_refs(&args("docker run --rm --name x -p 80:80 busybox")),
            vec!["x"]
        );
        assert_eq!(
            client_refs(&args("docker -H unix:///a run --name=y img")),
            vec!["y"]
        );
        assert_eq!(
            client_refs(&args("docker container run --name=z img")),
            vec!["z"]
        );
        assert_eq!(
            client_refs(&args(
                "podman run --log-driver=journald --name=terraria img --name other"
            )),
            vec!["terraria"]
        );
        // Sans nom, ou un nom qui n'a pas la forme d'un nom de conteneur : rien.
        assert!(client_refs(&args("docker run busybox")).is_empty());
        assert!(client_refs(&args("docker run --name=a;b busybox")).is_empty());
        assert!(client_refs(&args("docker run --name=")).is_empty());
    }

    #[test]
    fn a_start_names_its_containers_and_other_subcommands_name_none() {
        assert_eq!(client_refs(&args("docker start -a web")), vec!["web"]);
        assert_eq!(
            client_refs(&args("docker start --attach --detach-keys ctrl-x web db")),
            vec!["web", "db"]
        );
        for line in [
            "docker ps",
            "docker exec web ls",
            "docker stop web",
            "docker logs -f web",
            "docker",
        ] {
            assert!(client_refs(&args(line)).is_empty(), "{line}");
        }
        assert!(client_refs(&[]).is_empty());
    }

    #[test]
    fn systemctl_show_is_read_for_every_state_a_unit_can_be_in() {
        let text = |load: &str, active: &str, sub: &str, restart: &str, timeout: &str| {
            format!(
                "LoadState={load}\nActiveState={active}\nSubState={sub}\nRestart={restart}\nTimeoutStopUSec={timeout}\n"
            )
        };
        assert_eq!(
            parse_show(&text("loaded", "active", "running", "always", "2min")),
            UnitLookup::Loaded(UnitStatus {
                activity: UnitActivity::Active,
                auto_restart: false,
                restarts: true,
                timeout_stop_s: Some(120),
            })
        );
        assert_eq!(
            parse_show(&text(
                "loaded",
                "activating",
                "auto-restart",
                "on-failure",
                "1min 30s"
            )),
            UnitLookup::Loaded(UnitStatus {
                activity: UnitActivity::Activating,
                auto_restart: true,
                restarts: true,
                timeout_stop_s: Some(90),
            })
        );
        assert_eq!(
            parse_show(&text("loaded", "failed", "failed", "no", "infinity")),
            UnitLookup::Loaded(UnitStatus {
                activity: UnitActivity::Failed,
                auto_restart: false,
                restarts: false,
                timeout_stop_s: None,
            })
        );
        assert_eq!(
            parse_show(&text("not-found", "inactive", "dead", "no", "1min 30s")),
            UnitLookup::NotLoaded
        );
        assert_eq!(
            parse_show(&text("masked", "inactive", "dead", "no", "1min 30s")),
            UnitLookup::NotLoaded
        );
        // Une sortie qui n'est pas celle d'un systemd.
        assert_eq!(parse_show(""), UnitLookup::Unavailable);
        assert_eq!(
            parse_show("Failed to connect to bus"),
            UnitLookup::Unavailable
        );
    }

    #[test]
    fn timespans_follow_systemd_time_and_round_up() {
        assert_eq!(parse_timespan("2min"), Some(120));
        assert_eq!(parse_timespan("1min 30s"), Some(90));
        assert_eq!(parse_timespan("1h 30min"), Some(5400));
        assert_eq!(parse_timespan("120s"), Some(120));
        assert_eq!(parse_timespan("500ms"), Some(1));
        assert_eq!(parse_timespan("1.5s"), Some(2));
        assert_eq!(parse_timespan("0"), None);
        assert_eq!(parse_timespan("infinity"), None);
        assert_eq!(parse_timespan(""), None);
        assert_eq!(parse_timespan("soon"), None);
        assert_eq!(parse_timespan("10 parsecs"), None);
        assert_eq!(parse_timespan("1000y"), Some(u32::MAX));
    }
}
