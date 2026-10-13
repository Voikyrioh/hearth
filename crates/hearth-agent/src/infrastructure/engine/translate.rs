//! Traduction d'une réponse d'inspection du moteur en [`Observation`] du domaine.
//!
//! Fonctions pures, testées sans moteur. Ce qu'on NE copie pas : `Config.Env` (variables
//! d'environnement), `Config.Cmd`, `Config.Entrypoint`, `Path`, `Args` : ils restent dans la réponse
//! du moteur et meurent avec elle. Cette tranche ne transporte ni valeur ni nom de variable, ni
//! commande (ADR-0038).

use std::collections::BTreeMap;

use bollard::models::{
    ContainerInspectResponse, ContainerStateStatusEnum, PortMap, RestartPolicyNameEnum,
};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::domain::services::image::image_name;
use crate::domain::services::{
    MAX_IMAGE_REF_CHARS, MAX_IMAGE_TAG_CHARS, MAX_LABEL_CHARS, MAX_MOUNTS, MAX_NAME_CHARS,
    MAX_PATH_CHARS, MAX_PORTS, MountInfo, MountType, Observation, PortBinding, PortProtocol,
    PortScope, RestartPolicy, RunState, bounded, bounded_opt,
};

/// Étiquette posée par Compose sur les conteneurs d'une pile (information seulement).
const COMPOSE_PROJECT_LABEL: &str = "com.docker.compose.project";

/// Date du moteur (`RFC 3339`, nanosecondes) ; `0001-01-01T00:00:00Z` (jamais) et tout texte
/// illisible donnent `None`.
pub fn parse_date(text: &str) -> Option<OffsetDateTime> {
    let parsed = OffsetDateTime::parse(text.trim(), &Rfc3339).ok()?;
    (parsed.year() > 1970).then_some(parsed)
}

/// `Some` seulement pour une réponse qui désigne un conteneur (identifiant non vide).
pub fn observation_from(response: &ContainerInspectResponse) -> Option<Observation> {
    let id = response.id.clone().filter(|id| !id.is_empty())?;
    // Tout texte venu du conteneur est nettoyé et plafonné ici (`domain::services::limits`).
    let name = bounded(
        response
            .name
            .as_deref()
            .unwrap_or_default()
            .trim_start_matches('/'),
        MAX_NAME_CHARS,
    );
    let state = response.state.as_ref();
    let run_state = run_state_of(response);
    let host_config = response.host_config.as_ref();
    let config = response.config.as_ref();

    let mut image = image_name(
        config
            .and_then(|config| config.image.as_deref())
            .unwrap_or_default(),
        response.image.as_deref().unwrap_or_default(),
    );
    image.reference = bounded_opt(image.reference.as_deref(), MAX_IMAGE_REF_CHARS);
    image.tag = bounded_opt(image.tag.as_deref(), MAX_IMAGE_TAG_CHARS);

    // Les ports réels d'un conteneur qui tourne (le moteur attribue les ports demandés « au
    // hasard » au démarrage) ; sinon ceux qu'il a réservés.
    let live = response
        .network_settings
        .as_ref()
        .and_then(|settings| settings.ports.as_ref())
        .filter(|ports| ports.values().any(|bindings| bindings.is_some()));
    let reserved = host_config.and_then(|host| host.port_bindings.as_ref());
    let (ports, ports_omitted) = match (run_state, live, reserved) {
        (RunState::Running | RunState::Paused | RunState::Restarting, Some(live), _) => {
            ports_of(live)
        }
        (_, _, Some(reserved)) => ports_of(reserved),
        _ => (Vec::new(), 0),
    };

    let mut mounts: Vec<MountInfo> = response
        .mounts
        .iter()
        .flatten()
        .filter_map(|mount| {
            let kind = match mount.typ.as_deref() {
                Some("bind") => MountType::Folder,
                Some("volume") => MountType::Volume,
                // tmpfs, pipes nommés… : pas un dossier de données.
                _ => return None,
            };
            let source = match kind {
                MountType::Folder => mount.source.clone(),
                MountType::Volume => mount.name.clone().or_else(|| mount.source.clone()),
            }?;
            Some(MountInfo {
                kind,
                source: bounded(&source, MAX_PATH_CHARS),
                target: bounded(mount.destination.as_deref()?, MAX_PATH_CHARS),
                read_only: mount.rw == Some(false),
            })
        })
        .collect();
    // FIX:01M4SFCRJWX8KMBC1ZK9ZWETTW : les montages au-delà de la borne sont comptés, pas oubliés en silence.
    let mounts_omitted = u32::try_from(mounts.len().saturating_sub(MAX_MOUNTS)).unwrap_or(u32::MAX);
    mounts.truncate(MAX_MOUNTS);

    Some(Observation {
        id,
        name,
        created_at: response.created.as_deref().and_then(parse_date),
        run_state,
        exit_code: state.and_then(|state| state.exit_code),
        started_at: state
            .and_then(|state| state.started_at.as_deref())
            .and_then(parse_date),
        finished_at: state
            .and_then(|state| state.finished_at.as_deref())
            .and_then(parse_date),
        image,
        ports,
        ports_omitted,
        host_network: host_config
            .and_then(|host| host.network_mode.as_deref())
            .is_some_and(|mode| mode == "host"),
        mounts,
        mounts_omitted,
        restart: restart_of(host_config.and_then(|host| host.restart_policy.as_ref())),
        auto_remove: host_config
            .and_then(|host| host.auto_remove)
            .unwrap_or(false),
        compose_project: bounded_opt(
            config
                .and_then(|config| config.labels.as_ref())
                .and_then(|labels| labels.get(COMPOSE_PROJECT_LABEL))
                .map(String::as_str),
            MAX_LABEL_CHARS,
        ),
    })
}

fn run_state_of(response: &ContainerInspectResponse) -> RunState {
    let Some(state) = response.state.as_ref() else {
        return RunState::Unknown;
    };
    match state.status {
        Some(ContainerStateStatusEnum::CREATED) => RunState::Created,
        Some(ContainerStateStatusEnum::RUNNING) => RunState::Running,
        Some(ContainerStateStatusEnum::PAUSED) => RunState::Paused,
        Some(ContainerStateStatusEnum::RESTARTING) => RunState::Restarting,
        Some(ContainerStateStatusEnum::REMOVING | ContainerStateStatusEnum::STOPPING) => {
            RunState::Removing
        }
        Some(ContainerStateStatusEnum::EXITED) => RunState::Exited,
        Some(ContainerStateStatusEnum::DEAD) => RunState::Dead,
        // Statut absent ou vide : on se rabat sur les drapeaux (anciennes versions).
        Some(ContainerStateStatusEnum::EMPTY) | None => {
            if state.restarting == Some(true) {
                RunState::Restarting
            } else if state.paused == Some(true) {
                RunState::Paused
            } else if state.running == Some(true) {
                RunState::Running
            } else if state.dead == Some(true) {
                RunState::Dead
            } else {
                RunState::Unknown
            }
        }
    }
}

fn restart_of(policy: Option<&bollard::models::RestartPolicy>) -> RestartPolicy {
    match policy.and_then(|policy| policy.name) {
        Some(RestartPolicyNameEnum::ALWAYS) => RestartPolicy::Always,
        Some(RestartPolicyNameEnum::UNLESS_STOPPED) => RestartPolicy::UnlessStopped,
        Some(RestartPolicyNameEnum::ON_FAILURE) => RestartPolicy::OnFailure,
        _ => RestartPolicy::None,
    }
}

/// Les ports publiés d'une table `« 80/tcp » -> liaisons`. Les ports seulement exposés (sans
/// liaison) ne sont pas des ports du serveur. Un même port publié sur IPv4 et IPv6 n'est qu'un port.
/// Rend les `MAX_PORTS` premiers et le nombre de ceux qui sont omis au-delà.
fn ports_of(map: &PortMap) -> (Vec<PortBinding>, u32) {
    let mut merged: BTreeMap<(u16, u16, u8), PortBinding> = BTreeMap::new();
    for (container_side, bindings) in map {
        let Some((container_port, protocol)) = container_side.split_once('/') else {
            continue;
        };
        let Ok(container_port) = container_port.parse::<u16>() else {
            continue;
        };
        let (protocol, order) = match protocol {
            "tcp" => (PortProtocol::Tcp, 0u8),
            "udp" => (PortProtocol::Udp, 1),
            "sctp" => (PortProtocol::Sctp, 2),
            _ => continue,
        };
        for binding in bindings.iter().flatten() {
            let Some(host_port) = binding
                .host_port
                .as_deref()
                .and_then(|port| port.parse::<u16>().ok())
                .filter(|port| *port != 0)
            else {
                continue;
            };
            let scope = scope_of(binding.host_ip.as_deref().unwrap_or_default());
            merged
                .entry((host_port, container_port, order))
                .and_modify(|existing| {
                    if scope == PortScope::All {
                        existing.scope = PortScope::All;
                    }
                })
                .or_insert(PortBinding {
                    host_port,
                    container_port,
                    protocol,
                    scope,
                });
        }
    }
    // Les ports sont triés (port du serveur, du conteneur, protocole) : le plafond garde les premiers.
    // FIX:01M4SFCRJWX8KMBC1ZK9ZWETTW : les ports au-delà de la borne sont comptés, pas oubliés en silence.
    let omitted = u32::try_from(merged.len().saturating_sub(MAX_PORTS)).unwrap_or(u32::MAX);
    (merged.into_values().take(MAX_PORTS).collect(), omitted)
}

/// Une liaison sur l'adresse locale n'est joignable que depuis ce serveur ; toute autre (aucune
/// adresse, « 0.0.0.0 », « :: », ou l'adresse d'une carte réseau) l'est depuis le réseau.
fn scope_of(host_ip: &str) -> PortScope {
    match host_ip.trim() {
        "127.0.0.1" | "::1" | "localhost" => PortScope::Local,
        _ => PortScope::All,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Une réponse d'inspection plausible, d'après la forme documentée de l'API du moteur
    /// (`GET /containers/{id}/json`), avec une variable d'environnement SENTINELLE.
    fn response(extra: serde_json::Value) -> ContainerInspectResponse {
        let mut base = json!({
            "Id": "9f".repeat(32),
            "Created": "2026-10-13T09:00:00.123456789Z",
            "Path": "/entrypoint.sh",
            "Args": ["--secret-flag=SENTINEL-ARG"],
            "Name": "/web",
            "Image": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "State": {
                "Status": "running", "Running": true, "Paused": false, "Restarting": false,
                "Dead": false, "Pid": 4242, "ExitCode": 0,
                "StartedAt": "2026-10-13T10:00:00.5Z",
                "FinishedAt": "0001-01-01T00:00:00Z"
            },
            "HostConfig": {
                "NetworkMode": "bridge",
                "RestartPolicy": { "Name": "unless-stopped", "MaximumRetryCount": 0 },
                "AutoRemove": false,
                "PortBindings": { "80/tcp": [{ "HostIp": "", "HostPort": "8080" }] }
            },
            "Mounts": [
                { "Type": "bind", "Source": "/srv/data", "Destination": "/data", "Mode": "ro", "RW": false },
                { "Type": "volume", "Name": "dbdata", "Source": "/var/lib/docker/volumes/dbdata/_data",
                  "Destination": "/var/lib/db", "RW": true },
                { "Type": "tmpfs", "Destination": "/run", "RW": true }
            ],
            "Config": {
                "Image": "nginx:1.27",
                "Env": ["PASSWORD=SENTINEL-ENV-VALUE", "API_TOKEN=SENTINEL-ENV-VALUE"],
                "Cmd": ["nginx", "-g", "SENTINEL-CMD"],
                "Labels": { "com.docker.compose.project": "shop" }
            },
            "NetworkSettings": {
                "Ports": {
                    "80/tcp": [
                        { "HostIp": "0.0.0.0", "HostPort": "8080" },
                        { "HostIp": "::", "HostPort": "8080" }
                    ],
                    "443/tcp": null
                }
            }
        });
        if let (Some(base), Some(extra)) = (base.as_object_mut(), extra.as_object()) {
            for (key, value) in extra {
                base.insert(key.clone(), value.clone());
            }
        }
        serde_json::from_value(base).expect("réponse d'inspection")
    }

    #[test]
    fn a_running_container_is_translated() {
        let observation = observation_from(&response(json!({}))).expect("observation");
        assert_eq!(observation.name, "web");
        assert_eq!(observation.run_state, RunState::Running);
        assert_eq!(observation.image.reference.as_deref(), Some("nginx"));
        assert_eq!(observation.image.tag.as_deref(), Some("1.27"));
        assert_eq!(observation.image.short_id, "0123456789ab");
        assert_eq!(observation.restart, RestartPolicy::UnlessStopped);
        assert_eq!(observation.compose_project.as_deref(), Some("shop"));
        assert!(observation.started_at.is_some());
        assert_eq!(observation.finished_at, None, "0001-01-01 veut dire jamais");
        // IPv4 et IPv6 du même port : un seul port ; le port seulement exposé (443) est ignoré.
        assert_eq!(
            observation.ports,
            vec![PortBinding {
                host_port: 8080,
                container_port: 80,
                protocol: PortProtocol::Tcp,
                scope: PortScope::All,
            }]
        );
        // Un dossier en lecture seule, un volume nommé ; le tmpfs n'est pas un dossier de données.
        assert_eq!(observation.mounts.len(), 2);
        assert_eq!(observation.mounts[0].kind, MountType::Folder);
        assert!(observation.mounts[0].read_only);
        assert_eq!(observation.mounts[1].kind, MountType::Volume);
        assert_eq!(observation.mounts[1].source, "dbdata");
    }

    #[test]
    fn nothing_from_the_environment_or_the_command_survives_the_translation() {
        let observation = observation_from(&response(json!({}))).expect("observation");
        let rendered = format!("{observation:?}");
        for sentinel in [
            "SENTINEL-ENV-VALUE",
            "SENTINEL-CMD",
            "SENTINEL-ARG",
            "PASSWORD",
            "API_TOKEN",
        ] {
            assert!(!rendered.contains(sentinel), "{sentinel} dans {rendered}");
        }
    }

    #[test]
    fn a_stopped_container_reports_its_reserved_ports_and_exit_code() {
        let observation = observation_from(&response(json!({
            "State": { "Status": "exited", "Running": false, "ExitCode": 137,
                       "StartedAt": "2026-10-13T10:00:00Z", "FinishedAt": "2026-10-13T11:00:00Z" },
            "NetworkSettings": { "Ports": {} },
            "HostConfig": {
                "NetworkMode": "host", "AutoRemove": true,
                "PortBindings": { "53/udp": [{ "HostIp": "127.0.0.1", "HostPort": "5353" }] }
            }
        })))
        .expect("observation");
        assert_eq!(observation.run_state, RunState::Exited);
        assert_eq!(observation.exit_code, Some(137));
        assert!(observation.host_network);
        assert!(observation.auto_remove);
        assert_eq!(observation.restart, RestartPolicy::None);
        assert_eq!(observation.ports.len(), 1);
        assert_eq!(observation.ports[0].protocol, PortProtocol::Udp);
        assert_eq!(observation.ports[0].scope, PortScope::Local);
        assert!(observation.finished_at.is_some());
    }

    #[test]
    fn a_created_container_never_started_has_no_start_date() {
        let observation = observation_from(&response(json!({
            "State": { "Status": "created", "Running": false, "ExitCode": 0,
                       "StartedAt": "0001-01-01T00:00:00Z", "FinishedAt": "0001-01-01T00:00:00Z" }
        })))
        .expect("observation");
        assert_eq!(observation.run_state, RunState::Created);
        assert_eq!(observation.started_at, None);
    }

    #[test]
    fn an_old_engine_without_a_status_falls_back_on_the_flags() {
        let observation = observation_from(&response(json!({
            "State": { "Running": true, "Paused": true, "ExitCode": 0 }
        })))
        .expect("observation");
        assert_eq!(observation.run_state, RunState::Paused);
        let observation = observation_from(&response(json!({
            "State": { "Restarting": true, "Running": true, "ExitCode": 1 }
        })))
        .expect("observation");
        assert_eq!(observation.run_state, RunState::Restarting);
    }

    #[test]
    fn an_image_known_only_by_its_id_has_no_name() {
        let observation = observation_from(&response(json!({
            "Config": { "Image": "sha256:0123456789abcdef0123456789abcdef" }
        })))
        .expect("observation");
        assert_eq!(observation.image.reference, None);
    }

    #[test]
    fn everything_a_container_says_is_cleaned_and_bounded() {
        use crate::domain::services::{MAX_MOUNTS, MAX_NAME_CHARS, MAX_PATH_CHARS, MAX_PORTS};
        let ports: serde_json::Map<String, serde_json::Value> = (1..=3000u32)
            .map(|port| {
                (
                    format!("{port}/tcp"),
                    json!([{ "HostIp": "0.0.0.0", "HostPort": (10_000 + port).to_string() }]),
                )
            })
            .collect();
        let mounts: Vec<_> = (0..500)
            .map(|index| {
                json!({ "Type": "bind", "Source": format!("/srv/{index}"),
                        "Destination": format!("/data{index}"), "RW": true })
            })
            .collect();
        let observation = observation_from(&response(json!({
            "Name": format!("/web\n\u{1b}[31m{}", "x".repeat(1000)),
            "Mounts": mounts,
            "NetworkSettings": { "Ports": ports },
            "Config": {
                "Image": format!("reg/{}:tag\nfake", "i".repeat(1000)),
                "Labels": { "com.docker.compose.project": "shop\r\nforged: line" }
            }
        })))
        .expect("observation");
        assert_eq!(observation.name.chars().count(), MAX_NAME_CHARS);
        assert!(!observation.name.contains(['\n', '\u{1b}']));
        assert_eq!(observation.ports.len(), MAX_PORTS);
        assert_eq!(observation.mounts.len(), MAX_MOUNTS);
        // 3000 ports et 500 montages lus : le surplus est compté (BR-SVC-021).
        assert_eq!(observation.ports_omitted as usize, 3000 - MAX_PORTS);
        assert_eq!(observation.mounts_omitted as usize, 500 - MAX_MOUNTS);
        assert!(
            observation
                .mounts
                .iter()
                .all(|m| m.target.chars().count() <= MAX_PATH_CHARS)
        );
        assert_eq!(
            observation.compose_project.as_deref(),
            Some("shopforged: line")
        );
        let tag = observation.image.tag.expect("version");
        assert!(!tag.contains('\n'));
        assert!(
            observation.image.reference.expect("nom").chars().count()
                <= crate::domain::services::MAX_IMAGE_REF_CHARS
        );
    }

    #[test]
    fn a_response_without_an_id_is_no_container() {
        assert!(observation_from(&ContainerInspectResponse::default()).is_none());
    }

    #[test]
    fn engine_dates_parse_and_never_means_none() {
        assert!(parse_date("2026-10-13T10:00:00.123456789Z").is_some());
        assert_eq!(parse_date("0001-01-01T00:00:00Z"), None);
        assert_eq!(parse_date("hier"), None);
    }
}
