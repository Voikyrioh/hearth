//! Modifier un service : ce que le serveur juge d'une demande, sans E/S (HRT-87, ADR-0044, ADR-0041,
//! BR-EDIT-004 à BR-EDIT-007, BR-EDIT-010, BR-EDIT-013, BR-EDIT-028, BR-EDIT-037).
//!
//! **Un seul constructeur de description.** Une demande ([`SpecChange`]) s'applique à la description courante
//! (corps et secrets descellés) et devient une [`Candidate`] : la description voulue après la modification. Elle
//! passe par le validateur du déploiement ([`validate`]) : une modification n'obtient pas ce que le déploiement
//! refuse (BR-EDIT-028). Le conteneur se crée ensuite DEPUIS la description, par `plan_from_spec`.
//!
//! **Les valeurs conservées viennent de la description, jamais du client** (BR-EDIT-006) : une variable absente
//! du corps, ou marquée `keep`, garde la valeur de la révision précédente. Le client n'envoie que les valeurs
//! qu'il change.
//!
//! Ce que la modification **conserve** (BR-EDIT-014) : le nom, le dépôt de l'image, la règle de redémarrage, le
//! délai d'arrêt, l'entrée standard, les limites, les réglages (seuil du disque, éléments ignorés des
//! sauvegardes). Un refus ne cite JAMAIS une valeur ni un nom de variable : un champ et un code stable.

use hearth_proto::api::deploy::{
    DeployAccept, DeployEnv, DeployMount, DeployPlace, DeployPort, DeployProtocol, DeployResources,
    DeploySpec,
};
use hearth_proto::api::service_update::{
    CommandChange, CommandOp, EnvChange, EnvOp, SpecChange, command_changes,
};

use super::deploy::{Invalid, Refusal, ValidDeploy, validate};
use super::drift::ObservedContainer;
use super::spec::{
    EnvEntry, SpecBody, SpecMount, SpecMountKind, SpecPort, SpecProtocol, SpecRestart, SpecSecrets,
};
use super::state::{MountType, Observation, PortProtocol, RestartPolicy};
use crate::domain::secret::Secret;

/// Longueur maximale de l'adresse à donner aux joueurs.
pub const MAX_PUBLIC_ADDRESS_CHARS: usize = 255;

/// Pourquoi une demande de modification est refusée avant tout geste.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateRefusal {
    /// Un champ ne respecte pas sa règle (mêmes codes que le déploiement).
    Invalid(Invalid),
    /// Un réglage hors du plan fermé, ou hors de ce qu'un service de fiche accepte (BR-EDIT-028, BR-EDIT-037).
    /// `reason` : code stable (`unknown_member`, `frozen`, `fiche`).
    SettingNotAllowed { reason: &'static str },
    /// Les limites du service ne passent plus le validateur.
    LimitsRequired,
    /// Un dossier du serveur : cette version ne monte que les dossiers gérés par Hearth (HRT-70).
    HostFolder { index: usize },
}

fn invalid(field: &'static str, reason: &'static str) -> UpdateRefusal {
    UpdateRefusal::Invalid(Invalid { field, reason })
}

/// Ce que la modification change, en nombres et en catégories : de quoi le journal d'activité et le
/// récapitulatif parlent, **jamais un nom ni une valeur** (BR-EDIT-033).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Summary {
    pub tag: bool,
    pub ports: bool,
    pub mounts: bool,
    pub command: bool,
    /// Variables posées (nouvelles ou remplacées, valeur différente) et retirées.
    pub env_set: u32,
    pub env_removed: u32,
    /// L'adresse à donner aux joueurs change (sans recréer le service).
    pub public_address: bool,
    /// « Récupérer à nouveau cette version » (BR-EDIT-011).
    pub repull: bool,
}

impl Summary {
    /// Un changement qui oblige à recréer le conteneur (tout sauf l'adresse des joueurs).
    pub fn needs_recreation(&self) -> bool {
        self.tag
            || self.ports
            || self.mounts
            || self.command
            || self.env_set > 0
            || self.env_removed > 0
            || self.repull
    }
}

/// La description voulue après la modification.
#[derive(Debug)]
pub struct Candidate {
    pub body: SpecBody,
    pub secrets: SpecSecrets,
    pub summary: Summary,
    /// Les ports demandés, pour les vérifier contre le serveur.
    pub ports: Vec<DeployPort>,
    pub accept: DeployAccept,
}

/// Le résultat de l'application d'une demande à la description courante.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)] // une valeur par demande, jamais en collection
pub enum Applied {
    /// Rien ne change (BR-EDIT-013) : aucune recréation, aucune entrée au journal.
    Nothing,
    Changed(Candidate),
}

fn protocol_of(protocol: SpecProtocol) -> DeployProtocol {
    match protocol {
        SpecProtocol::Udp => DeployProtocol::Udp,
        // `sctp` n'existe pas dans le formulaire : il se lit comme TCP, jamais une erreur de lecture.
        SpecProtocol::Tcp | SpecProtocol::Sctp => DeployProtocol::Tcp,
    }
}

/// Les ports de la description, dans la forme du formulaire.
pub fn deploy_ports(body: &SpecBody) -> Vec<DeployPort> {
    body.ports
        .iter()
        .map(|port| DeployPort {
            host_port: port.host_port,
            container_port: port.container_port,
            protocol: protocol_of(port.protocol),
        })
        .collect()
}

/// Les montages gérés de la description, dans la forme du formulaire (l'emplacement géré se redéduit de la
/// cible : `flatten_target`).
pub fn deploy_mounts(body: &SpecBody) -> Vec<DeployMount> {
    body.mounts
        .iter()
        .map(|mount| DeployMount {
            target: mount.target.clone(),
            place: match mount.kind {
                SpecMountKind::Managed => DeployPlace::Managed,
                SpecMountKind::HostFolder => DeployPlace::Host,
            },
            host_path: (mount.kind == SpecMountKind::HostFolder).then(|| mount.source.clone()),
            read_only: mount.read_only,
            create: false,
            confirmed: false,
        })
        .collect()
}

fn same_env(left: &[EnvEntry], right: &[EnvEntry]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(a, b)| {
            a.name.expose() == b.name.expose() && a.value.expose() == b.value.expose()
        })
}

fn same_command(left: &Option<Vec<Secret>>, right: &Option<Vec<Secret>>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.expose() == y.expose())
        }
        _ => false,
    }
}

/// Une copie des secrets de la description : une modification part d'eux et n'en change que ce que le formulaire
/// change.
pub fn clone_secrets(secrets: &SpecSecrets) -> SpecSecrets {
    SpecSecrets {
        env: clone_env(&secrets.env),
        command: secrets.command.as_ref().map(|command| {
            command
                .iter()
                .map(|arg| Secret::new(arg.expose().to_owned()))
                .collect()
        }),
        answers: secrets
            .answers
            .iter()
            .map(|answer| super::spec::AnswerEntry {
                id: answer.id.clone(),
                value: Secret::new(answer.value.expose().to_owned()),
            })
            .collect(),
    }
}

fn clone_env(env: &[EnvEntry]) -> Vec<EnvEntry> {
    env.iter()
        .map(|entry| EnvEntry {
            name: Secret::new(entry.name.expose().to_owned()),
            value: Secret::new(entry.value.expose().to_owned()),
        })
        .collect()
}

/// Les variables après les lignes du formulaire : l'ordre de la description est gardé, une variable posée
/// remplace en place ou s'ajoute à la fin, une variable retirée part. Une ligne `keep` ne change rien. Compte les
/// variables posées avec une valeur différente (ou nouvelles) et les retirées qui existaient.
fn merge_env(
    current: &[EnvEntry],
    lines: &[EnvChange],
) -> Result<(Vec<EnvEntry>, u32, u32), UpdateRefusal> {
    let mut merged = clone_env(current);
    let (mut set, mut removed) = (0_u32, 0_u32);
    for line in lines {
        match line.op {
            EnvOp::Keep => {}
            EnvOp::Set => {
                let Some(value) = &line.value else {
                    return Err(invalid("env", "bad_value"));
                };
                match merged
                    .iter_mut()
                    .find(|entry| entry.name.expose() == line.name)
                {
                    Some(entry) => {
                        if entry.value.expose() != value {
                            entry.value = Secret::new(value.clone());
                            set += 1;
                        }
                    }
                    None => {
                        merged.push(EnvEntry {
                            name: Secret::new(line.name.clone()),
                            value: Secret::new(value.clone()),
                        });
                        set += 1;
                    }
                }
            }
            EnvOp::Remove => {
                let before = merged.len();
                merged.retain(|entry| entry.name.expose() != line.name);
                if merged.len() < before {
                    removed += 1;
                }
            }
            EnvOp::Unknown => return Err(invalid("env", "bad_op")),
        }
    }
    Ok((merged, set, removed))
}

fn refusal_of(refusal: Refusal) -> UpdateRefusal {
    match refusal {
        Refusal::Invalid(invalid) => UpdateRefusal::Invalid(invalid),
        Refusal::SettingNotAllowed => UpdateRefusal::SettingNotAllowed {
            reason: "unknown_member",
        },
        Refusal::LimitsRequired => UpdateRefusal::LimitsRequired,
        Refusal::HostFolder { index } => UpdateRefusal::HostFolder { index },
    }
}

fn valid_public_address(address: &str) -> bool {
    address.chars().count() <= MAX_PUBLIC_ADDRESS_CHARS
        && !address.chars().any(char::is_control)
        && !address.contains(char::is_whitespace)
}

/// Applique la demande à la description courante.
///
/// - Le plan est fermé : un membre inconnu du corps est refusé (`unknown_member`).
/// - Un service de fiche n'accepte que l'adresse des joueurs : une réponse figée est refusée `frozen`, tout le
///   reste `fiche` (BR-EDIT-037). Les questions non figées d'une fiche passent par la traduction de la fiche, qui
///   n'est pas branchée ici (voir `docs/open-api/service-update.md`).
/// - Un service ordinaire passe par le validateur du déploiement : nom, image, variables, ports, montages,
///   commande, limites.
pub fn apply(
    body: &SpecBody,
    secrets: &SpecSecrets,
    change: &SpecChange,
) -> Result<Applied, UpdateRefusal> {
    if change.extra.keys().any(|key| key != "reauth") {
        return Err(UpdateRefusal::SettingNotAllowed {
            reason: "unknown_member",
        });
    }
    if body.fiche.is_some() {
        return apply_to_fiche(body, secrets, change);
    }
    if change.answers.is_some() {
        // Un service qui ne vient pas d'une fiche n'a pas de questions.
        return Err(UpdateRefusal::SettingNotAllowed { reason: "fiche" });
    }

    let (env, env_set, env_removed) = merge_env(&secrets.env, &change.env)?;
    let tag = change
        .tag
        .as_deref()
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .unwrap_or(&body.image.tag)
        .to_owned();
    let ports = change.ports.clone().unwrap_or_else(|| deploy_ports(body));
    let mounts = change.mounts.clone().unwrap_or_else(|| deploy_mounts(body));
    let command_line = match &change.command {
        Some(CommandChange {
            op: CommandOp::Set,
            value,
        }) => Some(
            value
                .clone()
                .ok_or_else(|| invalid("command", "bad_format"))?,
        ),
        Some(CommandChange {
            op: CommandOp::Unknown,
            ..
        }) => return Err(invalid("command", "bad_op")),
        _ => None,
    };
    let spec = DeploySpec {
        name: body.name.clone(),
        image: body.image.repository.clone(),
        tag: Some(tag),
        env: env
            .iter()
            .map(|entry| DeployEnv {
                name: entry.name.expose().to_owned(),
                value: entry.value.expose().to_owned(),
            })
            .collect(),
        ports,
        mounts,
        command: command_line,
        resources: DeployResources {
            cpu_millis: body.cpu_millis,
            memory_bytes: body.memory_bytes,
        },
        accept: DeployAccept {
            reserved_ports: change.accept.reserved_ports,
            low_disk: change.accept.low_disk,
        },
        source: None,
        extra: Default::default(),
    };
    let ValidDeploy {
        body: mut next,
        secrets: mut next_secrets,
        accept,
        ports,
        ..
    } = validate(&spec, None).map_err(refusal_of)?;

    // Ce que la modification conserve (BR-EDIT-014).
    next.restart = body.restart;
    next.stop_grace_s = body.stop_grace_s;
    next.stdin_open = body.stdin_open;
    next.settings = body.settings.clone();
    next.image.digest = body
        .image
        .digest
        .clone()
        .filter(|_| next.image.tag == body.image.tag);
    // La commande : gardée telle quelle (le texte ne se recompose pas depuis ses éléments), posée ou vidée.
    match change.command.as_ref().map(|command| command.op) {
        None | Some(CommandOp::Keep) => {
            next_secrets.command = secrets.command.as_ref().map(|command| {
                command
                    .iter()
                    .map(|arg| Secret::new(arg.expose().to_owned()))
                    .collect()
            });
        }
        Some(CommandOp::Clear) => next_secrets.command = None,
        Some(CommandOp::Set | CommandOp::Unknown) => {}
    }
    if let Some(address) = &change.public_address {
        if !valid_public_address(address) {
            return Err(invalid("public_address", "bad_format"));
        }
        next.settings.public_address = (!address.is_empty()).then(|| address.clone());
    }

    let summary = Summary {
        tag: next.image.tag != body.image.tag,
        ports: sorted_ports(&next) != sorted_ports(body),
        mounts: sorted_mounts(&next) != sorted_mounts(body),
        command: !same_command(&next_secrets.command, &secrets.command),
        env_set,
        env_removed,
        public_address: next.settings.public_address != body.settings.public_address,
        repull: change.repull,
    };
    let unchanged = next == *body
        && same_env(&next_secrets.env, &secrets.env)
        && same_command(&next_secrets.command, &secrets.command)
        && !change.repull;
    if unchanged {
        return Ok(Applied::Nothing);
    }
    Ok(Applied::Changed(Candidate {
        body: next,
        secrets: next_secrets,
        summary,
        ports,
        accept,
    }))
}

fn sorted_ports(body: &SpecBody) -> Vec<(u16, u16, SpecProtocol)> {
    let mut ports: Vec<_> = body
        .ports
        .iter()
        .map(|port| (port.host_port, port.container_port, port.protocol))
        .collect();
    ports.sort_unstable();
    ports
}

fn sorted_mounts(body: &SpecBody) -> Vec<(String, String, bool)> {
    let mut mounts: Vec<_> = body
        .mounts
        .iter()
        .map(|mount| (mount.source.clone(), mount.target.clone(), mount.read_only))
        .collect();
    mounts.sort_unstable();
    mounts
}

/// Un service de fiche : seule l'adresse à donner aux joueurs change (sans recréer le service). Une réponse
/// figée est refusée `frozen` ; variables, ports, montages, commande, version : `fiche` (BR-EDIT-037).
fn apply_to_fiche(
    body: &SpecBody,
    secrets: &SpecSecrets,
    change: &SpecChange,
) -> Result<Applied, UpdateRefusal> {
    if let Some(answers) = &change.answers {
        let frozen = body.fiche.as_ref().is_some_and(|fiche| {
            answers.iter().any(|given| {
                fiche
                    .answers
                    .iter()
                    .any(|stored| stored.id == given.id && stored.frozen)
            })
        });
        return Err(UpdateRefusal::SettingNotAllowed {
            reason: if frozen { "frozen" } else { "fiche" },
        });
    }
    let touches_the_plan = change.env.iter().any(|line| line.op != EnvOp::Keep)
        || change.ports.is_some()
        || change.mounts.is_some()
        || command_changes(change)
        || change.tag.is_some();
    if touches_the_plan {
        return Err(UpdateRefusal::SettingNotAllowed { reason: "fiche" });
    }
    let mut next = body.clone();
    let mut summary = Summary::default();
    if let Some(address) = &change.public_address {
        if !valid_public_address(address) {
            return Err(invalid("public_address", "bad_format"));
        }
        next.settings.public_address = (!address.is_empty()).then(|| address.clone());
        summary.public_address = next.settings.public_address != body.settings.public_address;
    }
    summary.repull = change.repull;
    if next == *body && !change.repull {
        return Ok(Applied::Nothing);
    }
    Ok(Applied::Changed(Candidate {
        ports: deploy_ports(&next),
        body: next,
        secrets: clone_secrets(secrets),
        summary,
        accept: DeployAccept::default(),
    }))
}

/// Ce que le moteur dit du conteneur d'un service, ramené aux réglages que la description pose (BR-EDIT-024,
/// BR-EDIT-036). `consigned` : l'identifiant de ce conteneur est un de ceux que l'agent a consignés pour ce
/// service : c'est la preuve que Hearth l'a créé (l'étiquette n'en est jamais une, ADR-0037) ; faux, quelqu'un a
/// recréé le conteneur à la main sous le même nom. `managed_prefix` : `<racine des services>/<clé>/`, pour
/// ramener un dossier géré à son nom.
pub fn observed_container(
    observation: &Observation,
    rev: u32,
    consigned: bool,
    managed_prefix: &str,
) -> ObservedContainer {
    let image_repository = observation
        .image
        .reference
        .as_deref()
        .map(|reference| reference.split('@').next().unwrap_or(reference))
        .unwrap_or_default()
        .to_owned();
    ObservedContainer {
        label_rev: Some(rev),
        label_mac_valid: consigned,
        image_repository,
        image_tag: observation.image.tag.clone().unwrap_or_default(),
        ports: observation
            .ports
            .iter()
            .map(|port| SpecPort {
                host_port: port.host_port,
                container_port: port.container_port,
                protocol: match port.protocol {
                    PortProtocol::Tcp => SpecProtocol::Tcp,
                    PortProtocol::Udp => SpecProtocol::Udp,
                    PortProtocol::Sctp => SpecProtocol::Sctp,
                },
            })
            .collect(),
        mounts: observation
            .mounts
            .iter()
            .map(|mount| match mount.source.strip_prefix(managed_prefix) {
                Some(name) if mount.kind == MountType::Folder => SpecMount {
                    kind: SpecMountKind::Managed,
                    source: name.to_owned(),
                    target: mount.target.clone(),
                    read_only: mount.read_only,
                },
                _ => SpecMount {
                    kind: SpecMountKind::HostFolder,
                    source: mount.source.clone(),
                    target: mount.target.clone(),
                    read_only: mount.read_only,
                },
            })
            .collect(),
        restart: match observation.restart {
            RestartPolicy::None => SpecRestart::No,
            RestartPolicy::Always => SpecRestart::Always,
            RestartPolicy::UnlessStopped => SpecRestart::UnlessStopped,
            RestartPolicy::OnFailure => SpecRestart::OnFailure,
        },
        cpu_millis: observation.cpu_millis,
        memory_bytes: observation.memory_limit_bytes,
    }
}

/// Les variables que l'image fournit : celles du conteneur qui ne sont pas dans la description (BR-EDIT-007).
/// Le moteur liste les variables de l'image ET celles posées à la création : ce qui n'est pas dans la description
/// vient donc de l'image. Seulement des noms.
pub fn image_variables<'a>(observation: &'a Observation, secrets: &SpecSecrets) -> Vec<&'a str> {
    observation
        .env_names
        .iter()
        .map(|name| name.expose())
        .filter(|name| !secrets.env.iter().any(|entry| entry.name.expose() == *name))
        .collect()
}

#[cfg(test)]
mod tests {
    use hearth_proto::api::service_update::SpecChange;
    use serde_json::json;

    use super::super::spec::{
        SpecAnswer, SpecFiche, SpecImage, SpecMount, SpecPort, SpecRestart, SpecSettings,
    };
    use super::*;

    fn change(value: serde_json::Value) -> SpecChange {
        serde_json::from_value(value).expect("changement")
    }

    fn body() -> SpecBody {
        SpecBody {
            name: "web".into(),
            image: SpecImage {
                repository: "nginx".into(),
                tag: "1.27".into(),
                digest: None,
            },
            ports: vec![SpecPort {
                host_port: 8080,
                container_port: 80,
                protocol: SpecProtocol::Tcp,
            }],
            mounts: vec![SpecMount {
                kind: SpecMountKind::Managed,
                source: "data".into(),
                target: "/data".into(),
                read_only: false,
            }],
            restart: SpecRestart::Always,
            stop_grace_s: Some(30),
            stdin_open: true,
            cpu_millis: Some(1000),
            memory_bytes: Some(256 * 1024 * 1024),
            settings: SpecSettings {
                disk_alert_bytes: Some(5),
                public_address: None,
                backup_ignored: Vec::new(),
            },
            fiche: None,
        }
    }

    fn secrets() -> SpecSecrets {
        SpecSecrets {
            env: vec![
                EnvEntry {
                    name: Secret::new("A".into()),
                    value: Secret::new("SENTINEL-A".into()),
                },
                EnvEntry {
                    name: Secret::new("B".into()),
                    value: Secret::new("SENTINEL-B".into()),
                },
            ],
            command: Some(vec![Secret::new("run".into()), Secret::new("--x".into())]),
            answers: Vec::new(),
        }
    }

    fn changed(applied: Result<Applied, UpdateRefusal>) -> Candidate {
        match applied.expect("accepté") {
            Applied::Changed(candidate) => candidate,
            Applied::Nothing => panic!("un changement était attendu"),
        }
    }

    #[test]
    fn nothing_changes_when_the_form_is_the_description() {
        let applied = apply(&body(), &secrets(), &change(json!({ "base_rev": 1 })));
        assert!(matches!(applied, Ok(Applied::Nothing)), "{applied:?}");
        // Poser la valeur qui est déjà là, ou « garder », ne change rien non plus (BR-EDIT-013).
        let applied = apply(
            &body(),
            &secrets(),
            &change(json!({
                "base_rev": 1,
                "env": [
                    {"name": "A", "op": "set", "value": "SENTINEL-A"},
                    {"name": "B", "op": "keep"}
                ],
                "command": {"op": "keep"},
                "tag": "1.27"
            })),
        );
        assert!(matches!(applied, Ok(Applied::Nothing)), "{applied:?}");
    }

    #[test]
    fn a_kept_value_comes_from_the_description_never_from_the_client() {
        let candidate = changed(apply(
            &body(),
            &secrets(),
            &change(json!({
                "base_rev": 1,
                "env": [{"name": "B", "op": "set", "value": "NEW-B"}]
            })),
        ));
        let values: Vec<(&str, &str)> = candidate
            .secrets
            .env
            .iter()
            .map(|entry| (entry.name.expose(), entry.value.expose()))
            .collect();
        assert_eq!(values, [("A", "SENTINEL-A"), ("B", "NEW-B")]);
        assert_eq!(candidate.summary.env_set, 1);
        assert_eq!(candidate.summary.env_removed, 0);
        assert!(candidate.summary.needs_recreation());
    }

    #[test]
    fn a_variable_is_added_replaced_or_removed_and_counted_without_a_name() {
        let candidate = changed(apply(
            &body(),
            &secrets(),
            &change(json!({
                "base_rev": 1,
                "env": [
                    {"name": "A", "op": "remove"},
                    {"name": "C", "op": "set", "value": "NEW-C"},
                    {"name": "GHOST", "op": "remove"}
                ]
            })),
        ));
        let names: Vec<&str> = candidate
            .secrets
            .env
            .iter()
            .map(|entry| entry.name.expose())
            .collect();
        assert_eq!(names, ["B", "C"]);
        assert_eq!(
            (candidate.summary.env_set, candidate.summary.env_removed),
            (1, 1)
        );
    }

    #[test]
    fn what_the_modification_keeps_is_kept() {
        let candidate = changed(apply(
            &body(),
            &secrets(),
            &change(json!({ "base_rev": 1, "tag": "1.28" })),
        ));
        let next = &candidate.body;
        assert_eq!(next.name, "web");
        assert_eq!(next.image.repository, "nginx");
        assert_eq!(next.image.tag, "1.28");
        assert_eq!(next.restart, SpecRestart::Always);
        assert_eq!(next.stop_grace_s, Some(30));
        assert!(next.stdin_open);
        assert_eq!(next.settings.disk_alert_bytes, Some(5));
        assert_eq!(
            (next.cpu_millis, next.memory_bytes),
            (Some(1000), Some(256 * 1024 * 1024))
        );
        assert!(candidate.summary.tag && !candidate.summary.ports && !candidate.summary.mounts);
        // La commande et les variables sont gardées.
        assert_eq!(candidate.secrets.env.len(), 2);
        assert_eq!(candidate.secrets.command.as_ref().map(Vec::len), Some(2));
    }

    #[test]
    fn the_command_is_set_cleared_or_kept() {
        let set = changed(apply(
            &body(),
            &secrets(),
            &change(json!({ "base_rev": 1, "command": {"op": "set", "value": "serve --port 80"} })),
        ));
        assert_eq!(set.secrets.command.as_ref().map(Vec::len), Some(3));
        assert!(set.summary.command);
        let cleared = changed(apply(
            &body(),
            &secrets(),
            &change(json!({ "base_rev": 1, "command": {"op": "clear"} })),
        ));
        assert!(cleared.secrets.command.is_none() && cleared.summary.command);
    }

    #[test]
    fn a_port_or_a_mount_change_is_judged_like_a_deployment() {
        let ports = changed(apply(
            &body(),
            &secrets(),
            &change(json!({
                "base_rev": 1,
                "ports": [{"host_port": 9090, "container_port": 80}]
            })),
        ));
        assert!(ports.summary.ports);
        assert_eq!(ports.ports.len(), 1);
        let refused = apply(
            &body(),
            &secrets(),
            &change(json!({
                "base_rev": 1,
                "ports": [{"host_port": 0, "container_port": 80}]
            })),
        );
        assert!(matches!(
            refused,
            Err(UpdateRefusal::Invalid(Invalid { field: "ports", .. }))
        ));
        // Un dossier du serveur : refusé comme au déploiement.
        let host = apply(
            &body(),
            &secrets(),
            &change(json!({
                "base_rev": 1,
                "mounts": [{"target": "/data", "place": "host", "host_path": "/srv/x"}]
            })),
        );
        assert!(matches!(host, Err(UpdateRefusal::HostFolder { index: 0 })));
        // Retirer un dossier : permis, et sa catégorie est dite.
        let none = changed(apply(
            &body(),
            &secrets(),
            &change(json!({ "base_rev": 1, "mounts": [] })),
        ));
        assert!(none.summary.mounts && none.body.mounts.is_empty());
    }

    #[test]
    fn the_closed_plan_refuses_an_unknown_member() {
        let refused = apply(
            &body(),
            &secrets(),
            &change(json!({ "base_rev": 1, "privileged": true })),
        );
        assert!(matches!(
            refused,
            Err(UpdateRefusal::SettingNotAllowed {
                reason: "unknown_member"
            })
        ));
        // `reauth` est la confirmation de l'acte, pas un réglage.
        let applied = apply(
            &body(),
            &secrets(),
            &change(json!({ "base_rev": 1, "reauth": {"proof": 1} })),
        );
        assert!(matches!(applied, Ok(Applied::Nothing)));
    }

    #[test]
    fn a_refusal_never_quotes_a_value_or_a_name() {
        for refused in [
            apply(
                &body(),
                &secrets(),
                &change(json!({
                    "base_rev": 1,
                    "env": [{"name": "bad name SENTINEL", "op": "set", "value": "x"}]
                })),
            ),
            apply(
                &body(),
                &secrets(),
                &change(json!({
                    "base_rev": 1,
                    "env": [{"name": "A", "op": "set", "value": "line\nSENTINEL"}]
                })),
            ),
        ] {
            let shown = format!("{refused:?}");
            assert!(!shown.contains("SENTINEL"), "{shown}");
            assert!(matches!(refused, Err(UpdateRefusal::Invalid(_))));
        }
    }

    #[test]
    fn the_public_address_changes_without_recreating() {
        let candidate = changed(apply(
            &body(),
            &secrets(),
            &change(json!({ "base_rev": 1, "public_address": "jeu.example.org:7777" })),
        ));
        assert!(candidate.summary.public_address && !candidate.summary.needs_recreation());
        assert_eq!(
            candidate.body.settings.public_address.as_deref(),
            Some("jeu.example.org:7777")
        );
        // La vider la retire.
        let mut with = body();
        with.settings.public_address = Some("x".into());
        let cleared = changed(apply(
            &with,
            &secrets(),
            &change(json!({ "base_rev": 1, "public_address": "" })),
        ));
        assert_eq!(cleared.body.settings.public_address, None);
        let bad = apply(
            &body(),
            &secrets(),
            &change(json!({ "base_rev": 1, "public_address": "a b" })),
        );
        assert!(matches!(bad, Err(UpdateRefusal::Invalid(_))));
    }

    fn fiche_body() -> SpecBody {
        let mut body = body();
        body.fiche = Some(SpecFiche {
            id: "terraria".into(),
            version: 4,
            digest: "d".into(),
            variant: None,
            answers: vec![
                SpecAnswer {
                    id: "world_name".into(),
                    value: Some("Monde".into()),
                    secret: false,
                    frozen: true,
                },
                SpecAnswer {
                    id: "motd".into(),
                    value: Some("Salut".into()),
                    secret: false,
                    frozen: false,
                },
            ],
            agreements: Vec::new(),
        });
        body
    }

    #[test]
    fn a_game_service_only_takes_the_public_address() {
        let ok = changed(apply(
            &fiche_body(),
            &SpecSecrets::default(),
            &change(json!({ "base_rev": 1, "public_address": "jeu.example.org" })),
        ));
        assert!(!ok.summary.needs_recreation() && ok.summary.public_address);
        for refused in [
            json!({ "base_rev": 1, "tag": "2" }),
            json!({ "base_rev": 1, "ports": [] }),
            json!({ "base_rev": 1, "env": [{"name": "A", "op": "set", "value": "x"}] }),
            json!({ "base_rev": 1, "command": {"op": "clear"} }),
        ] {
            let result = apply(&fiche_body(), &SpecSecrets::default(), &change(refused));
            assert!(
                matches!(
                    result,
                    Err(UpdateRefusal::SettingNotAllowed { reason: "fiche" })
                ),
                "{result:?}"
            );
        }
    }

    #[test]
    fn a_frozen_answer_is_refused_with_its_own_reason() {
        let frozen = apply(
            &fiche_body(),
            &SpecSecrets::default(),
            &change(json!({
                "base_rev": 1,
                "answers": [{"id": "world_name", "value": "Autre"}]
            })),
        );
        assert!(matches!(
            frozen,
            Err(UpdateRefusal::SettingNotAllowed { reason: "frozen" })
        ));
    }

    #[test]
    fn an_ordinary_service_has_no_questions() {
        let refused = apply(
            &body(),
            &secrets(),
            &change(json!({ "base_rev": 1, "answers": [{"id": "x", "value": "y"}] })),
        );
        assert!(matches!(
            refused,
            Err(UpdateRefusal::SettingNotAllowed { reason: "fiche" })
        ));
    }

    #[test]
    fn repulling_alone_is_a_change_that_recreates() {
        let candidate = changed(apply(
            &body(),
            &secrets(),
            &change(json!({ "base_rev": 1, "repull": true })),
        ));
        assert!(candidate.summary.repull && candidate.summary.needs_recreation());
    }
}
