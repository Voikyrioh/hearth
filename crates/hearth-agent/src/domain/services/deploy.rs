//! Déployer une image : tout ce que le serveur juge d'une demande, sans E/S (HRT-68, ADR-0039, ADR-0041,
//! BR-DEPLOY-002 à BR-DEPLOY-007, BR-DEPLOY-014, BR-DEPLOY-035, BR-DEPLOY-038).
//!
//! **Un seul constructeur de plan.** Une demande (formulaire, plus tard une fiche) devient une
//! [`ValidDeploy`] : une description durable ([`SpecBody`] et [`SpecSecrets`], ADR-0044) et rien d'autre. Le
//! conteneur se crée DEPUIS cette description (`plan::plan_from_spec`), qui n'a aucun champ pour un mode
//! privilégié, un périphérique, le réseau du serveur ou une option de sécurité : le plan est fermé.
//!
//! Un refus ne cite JAMAIS la valeur reçue : seulement le champ (`name`, `image`, `env`, `ports`, `mounts`,
//! `command`, `resources`) et un code stable. La valeur d'une variable, son nom et la commande sont des
//! secrets.

use std::collections::HashSet;

use hearth_proto::api::deploy::{
    DeployAccept, DeployPlace, DeployPort, DeployProtocol, DeployResources, DeploySpec,
    MAX_DEPLOY_MOUNTS, MAX_ENV_VALUE_CHARS, MAX_ENV_VARS, MAX_PORT_MAPPINGS,
};
use hearth_proto::api::service_spec::split_image;

use super::command::split_command;
use super::limits::{
    MAX_COMMAND_ARG_CHARS, MAX_COMMAND_ARGS, MAX_ENV_NAME_CHARS, MAX_IMAGE_REF_CHARS,
    MAX_IMAGE_TAG_CHARS, MAX_PATH_CHARS,
};
use super::spec::{
    EnvEntry, SpecBody, SpecError, SpecImage, SpecMount, SpecMountKind, SpecPort, SpecProtocol,
    SpecRestart, SpecSecrets, SpecSettings,
};
use crate::domain::catalog::loading::schema::fiche::Protocol;
use crate::domain::catalog::translate::{DeploymentValidator, ServiceSpecDraft};
use crate::domain::files::RelPath;
use crate::domain::secret::Secret;

/// Longueur maximale du nom d'un service (BR-DEPLOY-003).
pub const NAME_MIN_CHARS: usize = 2;
pub const NAME_MAX_CHARS: usize = 63;
/// Limite de processeur la plus basse : un quart de cœur (BR-DEPLOY-038).
pub const MIN_CPU_MILLIS: u32 = 250;
/// Le pas du processeur (BR-DEPLOY-038).
pub const CPU_STEP_MILLIS: u32 = 250;
/// Limite de mémoire la plus basse : 128 Mio (BR-DEPLOY-038).
pub const MIN_MEMORY_BYTES: u64 = 128 * 1024 * 1024;
/// Le pas de la saisie exacte : un Mo entier (BR-DEPLOY-038).
pub const MEMORY_STEP_BYTES: u64 = 1024 * 1024;

/// La machine, lue sur la machine et non dans un conteneur (BR-DEPLOY-038).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Machine {
    pub cpus: u32,
    pub memory_bytes: u64,
}

/// Les limites posées au service (BR-DEPLOY-038).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub cpu_millis: u32,
    pub memory_bytes: u64,
}

/// Un champ refusé et pourquoi : deux codes stables, jamais la valeur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Invalid {
    pub field: &'static str,
    pub reason: &'static str,
}

fn invalid(field: &'static str, reason: &'static str) -> Refusal {
    Refusal::Invalid(Invalid { field, reason })
}

/// Pourquoi une demande de déploiement est refusée avant tout geste.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Un champ ne respecte pas sa règle.
    Invalid(Invalid),
    /// Le corps contient un réglage hors du plan fermé (BR-DEPLOY-035).
    SettingNotAllowed,
    /// La limite de mémoire ou de processeur manque (BR-DEPLOY-038).
    LimitsRequired,
    /// Un dossier du serveur est demandé : le jugement (protégé ou pas encore disponible) est celui de
    /// l'appelant. `index` : le rang du montage.
    HostFolder { index: usize },
}

/// Une image : son dépôt et sa version, déjà normalisés (BR-DEPLOY-005).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRef {
    pub repository: String,
    pub tag: String,
}

impl ImageRef {
    /// Normalise (la version écrite dans le champ de l'image passe dans le champ Version, vide vaut
    /// `latest`) puis juge. Une version écrite deux fois, différemment, est refusée.
    pub fn parse(image: &str, tag: Option<&str>) -> Result<Self, Invalid> {
        let image = image.trim();
        if image.is_empty() {
            return Err(Invalid {
                field: "image",
                reason: "empty",
            });
        }
        if image.contains('@') {
            // Les empreintes (`@sha256:…`) ne sont pas prises en charge dans ce lot (BR-DEPLOY-005).
            return Err(Invalid {
                field: "image",
                reason: "digest_unsupported",
            });
        }
        let (repository, merged) = split_image(image, tag);
        if let Some(explicit) = tag.map(str::trim).filter(|tag| !tag.is_empty())
            && image_has_version(image)
            && explicit != merged
        {
            return Err(Invalid {
                field: "tag",
                reason: "conflict",
            });
        }
        if repository.chars().count() > MAX_IMAGE_REF_CHARS {
            return Err(Invalid {
                field: "image",
                reason: "too_long",
            });
        }
        if !valid_repository(&repository) {
            return Err(Invalid {
                field: "image",
                reason: "bad_format",
            });
        }
        if merged.chars().count() > MAX_IMAGE_TAG_CHARS || !valid_tag(&merged) {
            return Err(Invalid {
                field: "tag",
                reason: "bad_format",
            });
        }
        Ok(Self {
            repository,
            tag: merged,
        })
    }

    /// `dépôt:version`, tel que le moteur le reçoit.
    pub fn reference(&self) -> String {
        format!("{}:{}", self.repository, self.tag)
    }
}

fn image_has_version(image: &str) -> bool {
    let after_slash = image.rfind('/').map_or(0, |i| i + 1);
    image[after_slash..]
        .rfind(':')
        .is_some_and(|i| !image[after_slash + i + 1..].is_empty())
}

/// `[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}`.
fn valid_tag(tag: &str) -> bool {
    let mut chars = tag.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// `registre[:port]/chemin/en/minuscules` (grammaire des références de Docker, sans version).
fn valid_repository(repository: &str) -> bool {
    let parts: Vec<&str> = repository.split('/').collect();
    let (host, path) = match parts.as_slice() {
        [first, rest @ ..]
            if !rest.is_empty()
                && (first.contains('.') || first.contains(':') || *first == "localhost") =>
        {
            (Some(*first), rest)
        }
        all => (None, all),
    };
    if let Some(host) = host {
        let (name, port) = match host.split_once(':') {
            Some((name, port)) => (name, Some(port)),
            None => (host, None),
        };
        let name_ok = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
            && !name.starts_with(['.', '-'])
            && !name.ends_with(['.', '-']);
        let port_ok = port.is_none_or(|port| {
            !port.is_empty() && port.len() <= 5 && port.chars().all(|c| c.is_ascii_digit())
        });
        if !name_ok || !port_ok {
            return false;
        }
    }
    !path.is_empty() && path.iter().all(|component| valid_path_component(component))
}

/// `[a-z0-9]+((\.|_|__|-+)[a-z0-9]+)*`.
fn valid_path_component(component: &str) -> bool {
    if component.is_empty() {
        return false;
    }
    let bytes = component.as_bytes();
    let alnum = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    if !alnum(bytes[0]) || !alnum(bytes[bytes.len() - 1]) {
        return false;
    }
    let mut i = 0;
    while i < bytes.len() {
        if alnum(bytes[i]) {
            i += 1;
            continue;
        }
        // Un séparateur : `.`, `_`, `__` ou des tirets, jamais deux séparateurs de natures différentes de suite.
        let start = i;
        match bytes[i] {
            b'.' | b'_' | b'-' => {
                while i < bytes.len() && bytes[i] == bytes[start] {
                    i += 1;
                }
                let run = i - start;
                let ok = match bytes[start] {
                    b'.' => run == 1,
                    b'_' => run <= 2,
                    _ => true,
                };
                if !ok || i >= bytes.len() || !alnum(bytes[i]) {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

/// Le nom d'un service (BR-DEPLOY-003) : lettres sans accent, chiffres, tirets et points, commence par une
/// lettre ou un chiffre, 2 à 63 caractères. Le moteur refuse un nom de conteneur d'un seul caractère
/// (`[a-zA-Z0-9][a-zA-Z0-9_.-]+`) : il est refusé ici, avant de télécharger quoi que ce soit.
pub fn validate_name(name: &str) -> Result<(), Invalid> {
    let mut chars = name.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphanumeric());
    let rest_ok = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.'));
    let length = name.chars().count();
    if !(NAME_MIN_CHARS..=NAME_MAX_CHARS).contains(&length) || !first_ok || !rest_ok {
        return Err(Invalid {
            field: "name",
            reason: "bad_format",
        });
    }
    Ok(())
}

/// Le nom d'une variable (BR-DEPLOY-006) : lettres, chiffres, soulignés, sans commencer par un chiffre.
fn valid_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && name.chars().count() <= MAX_ENV_NAME_CHARS
}

/// Les limites (BR-DEPLOY-038) : obligatoires, au moins 0,25 cœur et 128 Mo, au plus la machine.
/// Sans machine connue (le moteur ne l'a pas dite), seul le plancher est jugé.
pub fn check_limits(
    resources: DeployResources,
    machine: Option<Machine>,
) -> Result<Limits, Refusal> {
    let (Some(cpu_millis), Some(memory_bytes)) = (resources.cpu_millis, resources.memory_bytes)
    else {
        return Err(Refusal::LimitsRequired);
    };
    if cpu_millis % CPU_STEP_MILLIS != 0 {
        return Err(invalid("resources", "cpu_step"));
    }
    if memory_bytes % MEMORY_STEP_BYTES != 0 {
        return Err(invalid("resources", "memory_step"));
    }
    let cpu_max = machine.map_or(u32::MAX, |m| m.cpus.saturating_mul(1000));
    let memory_max = machine.map_or(u64::MAX, |m| m.memory_bytes);
    if cpu_millis < MIN_CPU_MILLIS || cpu_millis > cpu_max {
        return Err(invalid("resources", "cpu_bounds"));
    }
    if memory_bytes < MIN_MEMORY_BYTES || memory_bytes > memory_max {
        return Err(invalid("resources", "memory_bounds"));
    }
    Ok(Limits {
        cpu_millis,
        memory_bytes,
    })
}

/// Le chemin d'un dossier dans le service, normalisé : absolu, sans `.` ni `..`, ni racine, ni vide ni octet de
/// contrôle (BR-DEPLOY-010).
pub fn normalize_target(target: &str) -> Result<String, &'static str> {
    if target.is_empty() || !target.starts_with('/') {
        return Err("not_absolute");
    }
    if target.chars().count() > MAX_PATH_CHARS || target.chars().any(char::is_control) {
        return Err("bad_format");
    }
    let mut parts = Vec::new();
    for part in target.split('/') {
        match part {
            "" => {}
            "." | ".." => return Err("not_normal"),
            part => parts.push(part),
        }
    }
    if parts.is_empty() {
        return Err("root");
    }
    Ok(format!("/{}", parts.join("/")))
}

/// Le nom du sous-dossier géré d'un point de montage : le chemin dans le service « aplati », injectif (deux
/// chemins différents ne donnent jamais le même nom) et sans séparateur ni caractère spécial. Chaque
/// composant garde `[A-Za-z0-9._-]` et code le reste en `%XX` ; les composants se joignent par `+`.
pub fn flatten_target(target: &str) -> Option<String> {
    let mut flat = String::new();
    for (i, part) in target
        .split('/')
        .filter(|part| !part.is_empty())
        .enumerate()
    {
        if i > 0 {
            flat.push('+');
        }
        for byte in part.bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_') {
                flat.push(char::from(byte));
            } else {
                flat.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    // Un composant `.` ou `..` n'arrive jamais ici ; le tout doit être un nom de dossier valable.
    (RelPath::check_component(&flat).is_ok()).then_some(flat)
}

/// Une demande jugée valable : la description durable à écrire, et ce que l'appelant doit encore savoir.
#[derive(Debug)]
pub struct ValidDeploy {
    pub body: SpecBody,
    pub secrets: SpecSecrets,
    pub accept: DeployAccept,
    /// Les ports demandés, pour les vérifier contre le serveur.
    pub ports: Vec<DeployPort>,
}

impl ValidDeploy {
    pub fn env_count(&self) -> usize {
        self.secrets.env.len()
    }

    pub fn has_command(&self) -> bool {
        self.secrets.command.is_some()
    }

    pub fn image(&self) -> ImageRef {
        ImageRef {
            repository: self.body.image.repository.clone(),
            tag: self.body.image.tag.clone(),
        }
    }
}

/// Juge une demande de déploiement. L'ordre des refus : un réglage hors du plan fermé d'abord, puis les champs
/// dans l'ordre du formulaire, puis les limites (obligatoires).
pub fn validate(spec: &DeploySpec, machine: Option<Machine>) -> Result<ValidDeploy, Refusal> {
    // `reauth` est la confirmation de l'acte, lue à part : ce n'est pas un réglage du service.
    if spec.extra.keys().any(|key| key != "reauth") {
        return Err(Refusal::SettingNotAllowed);
    }
    validate_name(&spec.name).map_err(Refusal::Invalid)?;
    let image = ImageRef::parse(&spec.image, spec.tag.as_deref()).map_err(Refusal::Invalid)?;

    if spec.env.len() > MAX_ENV_VARS {
        return Err(invalid("env", "too_many"));
    }
    let mut seen_names = HashSet::new();
    let mut env = Vec::with_capacity(spec.env.len());
    for variable in &spec.env {
        if !valid_env_name(&variable.name) {
            return Err(invalid("env", "bad_name"));
        }
        if !seen_names.insert(variable.name.as_str()) {
            return Err(invalid("env", "duplicate_name"));
        }
        // Une valeur tient sur une seule ligne (BR-DEPLOY-006).
        if variable.value.chars().count() > MAX_ENV_VALUE_CHARS
            || variable
                .value
                .chars()
                .any(|c| c == '\n' || c == '\r' || c == '\0')
        {
            return Err(invalid("env", "bad_value"));
        }
        env.push(EnvEntry {
            name: Secret::new(variable.name.clone()),
            value: Secret::new(variable.value.clone()),
        });
    }

    if spec.ports.len() > MAX_PORT_MAPPINGS {
        return Err(invalid("ports", "too_many"));
    }
    let mut ports = Vec::with_capacity(spec.ports.len());
    let mut seen_ports = HashSet::new();
    for port in &spec.ports {
        if port.host_port == 0 || port.container_port == 0 {
            return Err(invalid("ports", "range"));
        }
        if !seen_ports.insert((port.host_port, port.protocol)) {
            return Err(invalid("ports", "duplicate"));
        }
        ports.push(SpecPort {
            host_port: port.host_port,
            container_port: port.container_port,
            protocol: match port.protocol {
                DeployProtocol::Tcp => SpecProtocol::Tcp,
                DeployProtocol::Udp => SpecProtocol::Udp,
            },
        });
    }

    if spec.mounts.len() > MAX_DEPLOY_MOUNTS {
        return Err(invalid("mounts", "too_many"));
    }
    let mut mounts = Vec::with_capacity(spec.mounts.len());
    let mut seen_targets = HashSet::new();
    for (index, mount) in spec.mounts.iter().enumerate() {
        let target = normalize_target(&mount.target).map_err(|reason| invalid("mounts", reason))?;
        if !seen_targets.insert(target.clone()) {
            return Err(invalid("mounts", "duplicate"));
        }
        if mount.place == DeployPlace::Host {
            return Err(Refusal::HostFolder { index });
        }
        let source = flatten_target(&target).ok_or_else(|| invalid("mounts", "bad_format"))?;
        mounts.push(SpecMount {
            kind: SpecMountKind::Managed,
            source,
            target,
            read_only: mount.read_only,
        });
    }

    let command = match spec.command.as_deref() {
        None => None,
        Some(line) if line.trim().is_empty() => None,
        Some(line) => Some(
            split_command(line)
                .map_err(|_| invalid("command", "bad_format"))?
                .into_iter()
                .map(Secret::new)
                .collect::<Vec<_>>(),
        ),
    };

    let limits = check_limits(spec.resources, machine)?;

    let body = SpecBody {
        name: spec.name.clone(),
        image: SpecImage {
            repository: image.repository,
            tag: image.tag,
            digest: None,
        },
        ports,
        mounts,
        // Un service déployé redémarre tout seul sauf arrêt voulu depuis Hearth (BR-DEPLOY-021).
        restart: SpecRestart::UnlessStopped,
        stop_grace_s: None,
        stdin_open: false,
        cpu_millis: Some(limits.cpu_millis),
        memory_bytes: Some(limits.memory_bytes),
        settings: SpecSettings::default(),
        fiche: None,
    };
    body.validate().map_err(spec_refusal)?;
    let secrets = SpecSecrets {
        env,
        command,
        answers: Vec::new(),
    };
    secrets.validate().map_err(spec_refusal)?;
    Ok(ValidDeploy {
        body,
        secrets,
        accept: spec.accept,
        ports: spec.ports.clone(),
    })
}

/// Une borne de la description que les contrôles ci-dessus n'ont pas attrapée : refusée sur son champ.
fn spec_refusal(error: SpecError) -> Refusal {
    let field = match error {
        SpecError::Name => "name",
        SpecError::Image => "image",
        SpecError::Ports => "ports",
        SpecError::Mounts => "mounts",
        SpecError::Limits => "resources",
        SpecError::Env | SpecError::SecretTooLong => "env",
        SpecError::Command => "command",
        SpecError::Ignored | SpecError::Answers | SpecError::Fiche => "spec",
    };
    invalid(field, "bounds")
}

// ---------------------------------------------------------------------------------------------------
// Le validateur du déploiement ordinaire, pour une description issue d'une fiche
// ---------------------------------------------------------------------------------------------------

/// Ce que le déploiement ordinaire exige d'une description, appliqué à celle qu'une fiche produit (invariant I1 :
/// une fiche et une image à la main passent par les MÊMES règles). Les mêmes fonctions que [`validate`] : le nom
/// et la version de l'image, les noms de variables, les ports, les dossiers, la commande et leurs bornes. Un
/// refus est un code fixe (`champ:raison`), jamais une valeur.
pub struct OrdinaryDeployment;

impl DeploymentValidator for OrdinaryDeployment {
    fn validate(&self, spec: &ServiceSpecDraft) -> Result<(), String> {
        let refuse = |field: &str, reason: &str| Err(format!("{field}:{reason}"));
        if let Err(invalid) = ImageRef::parse(&spec.image_repository, Some(&spec.image_tag)) {
            return refuse(invalid.field, invalid.reason);
        }
        if spec.ports.len() > MAX_PORT_MAPPINGS {
            return refuse("ports", "too_many");
        }
        let mut seen_ports = HashSet::new();
        for port in &spec.ports {
            if port.host == 0 || port.container == 0 {
                return refuse("ports", "range");
            }
            if !seen_ports.insert((port.host, matches!(port.protocol, Protocol::Udp))) {
                return refuse("ports", "duplicate");
            }
        }
        if spec.env.len() > MAX_ENV_VARS {
            return refuse("env", "too_many");
        }
        let mut seen_names = HashSet::new();
        for variable in &spec.env {
            if !valid_env_name(&variable.name) {
                return refuse("env", "bad_name");
            }
            if !seen_names.insert(variable.name.as_str()) {
                return refuse("env", "duplicate_name");
            }
            if variable.value.expose().chars().count() > MAX_ENV_VALUE_CHARS
                || variable.value.expose().contains('\0')
            {
                return refuse("env", "bad_value");
            }
        }
        if spec.folders.len() > MAX_DEPLOY_MOUNTS {
            return refuse("mounts", "too_many");
        }
        let mut seen_targets = HashSet::new();
        for folder in &spec.folders {
            let Ok(target) = normalize_target(&folder.target) else {
                return refuse("mounts", "bad_target");
            };
            if flatten_target(&target).is_none() {
                return refuse("mounts", "bad_format");
            }
            if !seen_targets.insert(target) {
                return refuse("mounts", "duplicate");
            }
        }
        if spec.command.len() > MAX_COMMAND_ARGS
            || spec
                .command
                .iter()
                .any(|arg| arg.chars().count() > MAX_COMMAND_ARG_CHARS || arg.contains('\0'))
        {
            return refuse("command", "bad_format");
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------------
// Le plan de l'opération : les étapes dans l'ordre
// ---------------------------------------------------------------------------------------------------

/// Les étapes d'un déploiement, dans l'ordre (BR-DEPLOY-016). Les codes sont ceux du fil.
pub mod step {
    pub const CHECK: &str = "check";
    pub const PULL: &str = "pull";
    pub const CREATE: &str = "create";
    pub const START: &str = "start";
    pub const WATCH: &str = "watch";
}

/// Le plan figé d'un déploiement : vérification, récupération, création (point de non-retour), démarrage,
/// surveillance. Création et démarrage écrivent leur `intent` avant le geste : un agent qui s'arrête entre les
/// deux laisse une opération « à vérifier », jamais « tout va bien » (ADR-0045).
pub fn op_plan() -> Result<crate::domain::ops::OpPlan, crate::domain::ops::PlanError> {
    use crate::domain::ops::{OpPlan, PlannedStep, StepName};
    let name = |text: &str| StepName::parse(text).ok_or(crate::domain::ops::PlanError::StepName);
    let step = |text: &str, destructive| {
        Ok::<_, crate::domain::ops::PlanError>(PlannedStep {
            name: name(text)?,
            destructive,
        })
    };
    OpPlan::new(
        vec![
            step(step::CHECK, false)?,
            step(step::PULL, false)?,
            step(step::CREATE, true)?,
            step(step::START, true)?,
            step(step::WATCH, false)?,
        ],
        Some(name(step::CREATE)?),
    )
}

#[cfg(test)]
mod tests {
    use hearth_proto::api::deploy::{DeployEnv, DeployMount};

    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    fn machine() -> Option<Machine> {
        Some(Machine {
            cpus: 8,
            memory_bytes: 16 * GIB,
        })
    }

    fn base() -> DeploySpec {
        serde_json::from_value(serde_json::json!({
            "name": "web", "image": "nginx",
            "resources": {"cpu_millis": 1000, "memory_bytes": 268_435_456}
        }))
        .expect("spec")
    }

    fn refused(spec: &DeploySpec) -> Refusal {
        validate(spec, machine()).expect_err("refusée")
    }

    fn field(spec: &DeploySpec) -> (&'static str, &'static str) {
        match refused(spec) {
            Refusal::Invalid(Invalid { field, reason }) => (field, reason),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_minimal_deployment_has_a_name_an_image_and_limits() {
        let valid = validate(&base(), machine()).expect("valide");
        assert_eq!(valid.body.name, "web");
        assert_eq!(valid.body.image.repository, "nginx");
        assert_eq!(valid.body.image.tag, "latest");
        assert_eq!(valid.body.restart, SpecRestart::UnlessStopped);
        assert!(valid.body.ports.is_empty() && valid.body.mounts.is_empty());
        assert_eq!(valid.body.cpu_millis, Some(1000));
        assert_eq!(valid.body.memory_bytes, Some(268_435_456));
        assert!(valid.secrets.is_empty());
    }

    #[test]
    fn the_name_is_letters_digits_dashes_and_dots_from_two_to_sixty_three() {
        for good in ["ab", "web", "Web-1.2", "9lives", &"x".repeat(63)] {
            assert!(validate_name(good).is_ok(), "{good}");
        }
        for bad in [
            "",
            "a",
            "-web",
            ".web",
            "wéb",
            "web app",
            "web_1",
            "web/1",
            &"x".repeat(64),
        ] {
            assert!(validate_name(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_version_in_the_image_field_moves_and_empty_means_latest() {
        let image = |text: &str, tag: Option<&str>| ImageRef::parse(text, tag).expect("image");
        assert_eq!(image("nginx:1.27", None).reference(), "nginx:1.27");
        assert_eq!(image("nginx", Some("1.0")).reference(), "nginx:1.0");
        assert_eq!(image("nginx", None).reference(), "nginx:latest");
        assert_eq!(
            image("registry:5000/team/app", None).reference(),
            "registry:5000/team/app:latest"
        );
        assert_eq!(
            image("ghcr.io/org/app:v2", None).reference(),
            "ghcr.io/org/app:v2"
        );
        assert_eq!(image("nginx:1.27", Some("1.27")).reference(), "nginx:1.27");
    }

    #[test]
    fn a_bad_image_is_refused_on_the_right_field() {
        let reason =
            |text: &str, tag: Option<&str>| ImageRef::parse(text, tag).expect_err("refusée");
        assert_eq!(reason("", None).field, "image");
        assert_eq!(
            reason("nginx@sha256:abc", None).reason,
            "digest_unsupported"
        );
        assert_eq!(reason("Nginx", None).reason, "bad_format");
        assert_eq!(reason("ng inx", None).reason, "bad_format");
        assert_eq!(reason("a//b", None).reason, "bad_format");
        assert_eq!(reason("-a", None).reason, "bad_format");
        assert_eq!(reason("a..b", None).reason, "bad_format");
        assert_eq!(reason("nginx", Some("bad tag")).field, "tag");
        assert_eq!(reason("nginx", Some("-1")).field, "tag");
        // Deux versions différentes.
        let conflict = reason("nginx:1.27", Some("1.28"));
        assert_eq!((conflict.field, conflict.reason), ("tag", "conflict"));
        assert_eq!(reason(&"a".repeat(300), None).reason, "too_long");
    }

    #[test]
    fn the_variables_are_bounded_unique_and_on_one_line() {
        let mut spec = base();
        spec.env = vec![
            DeployEnv {
                name: "A_B1".into(),
                value: String::new(),
            },
            DeployEnv {
                name: "_X".into(),
                value: "ok".into(),
            },
        ];
        assert_eq!(validate(&spec, machine()).expect("valide").env_count(), 2);
        for (name, value, reason) in [
            ("1A", "v", "bad_name"),
            ("A-B", "v", "bad_name"),
            ("", "v", "bad_name"),
            ("A", "ligne\nsuivante", "bad_value"),
            ("A", "nul\0", "bad_value"),
        ] {
            spec.env = vec![DeployEnv {
                name: name.into(),
                value: value.into(),
            }];
            assert_eq!(field(&spec), ("env", reason), "{name:?}");
        }
        spec.env = vec![
            DeployEnv {
                name: "A".into(),
                value: "1".into(),
            },
            DeployEnv {
                name: "A".into(),
                value: "2".into(),
            },
        ];
        assert_eq!(field(&spec), ("env", "duplicate_name"));
        spec.env = vec![
            DeployEnv {
                name: "A".into(),
                value: "x".repeat(MAX_ENV_VALUE_CHARS + 1)
            };
            1
        ];
        assert_eq!(field(&spec), ("env", "bad_value"));
        spec.env = (0..=MAX_ENV_VARS)
            .map(|i| DeployEnv {
                name: format!("V{i}"),
                value: String::new(),
            })
            .collect();
        assert_eq!(field(&spec), ("env", "too_many"));
    }

    #[test]
    fn a_refusal_never_carries_the_value_it_refuses() {
        let mut spec = base();
        spec.env = vec![DeployEnv {
            name: "SENTINEL-NAME".into(),
            value: "SENTINEL-VALUE".into(),
        }];
        let shown = format!("{:?}", refused(&spec));
        assert!(!shown.contains("SENTINEL"), "{shown}");
        spec.env.clear();
        spec.command = Some("run \"SENTINEL-CMD".into());
        let shown = format!("{:?}", refused(&spec));
        assert!(!shown.contains("SENTINEL"), "{shown}");
    }

    #[test]
    fn the_ports_are_bounded_and_a_mapping_is_unique_per_protocol() {
        let mut spec = base();
        let port = |host: u16, container: u16, protocol| DeployPort {
            host_port: host,
            container_port: container,
            protocol,
        };
        spec.ports = vec![
            port(8080, 80, DeployProtocol::Tcp),
            port(8080, 80, DeployProtocol::Udp),
            port(80, 80, DeployProtocol::Tcp),
        ];
        let valid = validate(&spec, machine()).expect("valide");
        assert_eq!(valid.body.ports.len(), 3);
        spec.ports = vec![port(0, 80, DeployProtocol::Tcp)];
        assert_eq!(field(&spec), ("ports", "range"));
        spec.ports = vec![port(80, 0, DeployProtocol::Tcp)];
        assert_eq!(field(&spec), ("ports", "range"));
        spec.ports = vec![
            port(80, 80, DeployProtocol::Tcp),
            port(80, 81, DeployProtocol::Tcp),
        ];
        assert_eq!(field(&spec), ("ports", "duplicate"));
        spec.ports = (1..=u16::try_from(MAX_PORT_MAPPINGS).unwrap() + 1)
            .map(|p| port(p + 1000, p, DeployProtocol::Tcp))
            .collect();
        assert_eq!(field(&spec), ("ports", "too_many"));
    }

    fn mount(target: &str) -> DeployMount {
        DeployMount {
            target: target.into(),
            place: DeployPlace::Managed,
            host_path: None,
            read_only: false,
            create: false,
            confirmed: false,
        }
    }

    #[test]
    fn a_data_folder_is_an_absolute_distinct_path_below_the_root() {
        let mut spec = base();
        spec.mounts = vec![mount("/data/"), mount("/var/lib/app")];
        let valid = validate(&spec, machine()).expect("valide");
        assert_eq!(valid.body.mounts[0].target, "/data");
        assert_eq!(valid.body.mounts[0].source, "data");
        assert_eq!(valid.body.mounts[1].source, "var+lib+app");
        for (target, reason) in [
            ("", "not_absolute"),
            ("data", "not_absolute"),
            ("/", "root"),
            ("//", "root"),
            ("/a/../b", "not_normal"),
            ("/a/./b", "not_normal"),
            ("/a\nb", "bad_format"),
        ] {
            spec.mounts = vec![mount(target)];
            assert_eq!(field(&spec), ("mounts", reason), "{target:?}");
        }
        spec.mounts = vec![mount("/data"), mount("/data/")];
        assert_eq!(field(&spec), ("mounts", "duplicate"));
    }

    #[test]
    fn flattening_is_injective_and_never_a_path() {
        assert_eq!(flatten_target("/a/b").as_deref(), Some("a+b"));
        assert_eq!(flatten_target("/a+b").as_deref(), Some("a%2Bb"));
        assert_ne!(flatten_target("/a/b"), flatten_target("/a+b"));
        assert_ne!(flatten_target("/a%2Bb"), flatten_target("/a+b"));
        assert!(!flatten_target("/x y/é").unwrap().contains(['/', ' ']));
        // Trop long pour un nom de dossier.
        assert_eq!(flatten_target(&format!("/{}", "é".repeat(200))), None);
    }

    #[test]
    fn a_folder_of_the_server_is_not_judged_here() {
        let mut spec = base();
        let mut other = mount("/data");
        other.place = DeployPlace::Host;
        other.host_path = Some("/srv/x".into());
        spec.mounts = vec![mount("/a"), other];
        assert_eq!(refused(&spec), Refusal::HostFolder { index: 1 });
    }

    #[test]
    fn the_command_is_split_without_an_interpreter_and_empty_means_the_image_one() {
        let mut spec = base();
        spec.command = Some("run --name 'a b' | cat".into());
        let valid = validate(&spec, machine()).expect("valide");
        let args: Vec<&str> = valid
            .secrets
            .command
            .as_ref()
            .unwrap()
            .iter()
            .map(Secret::expose)
            .collect();
        assert_eq!(args, ["run", "--name", "a b", "|", "cat"]);
        spec.command = Some("   ".into());
        assert!(
            validate(&spec, machine())
                .expect("valide")
                .secrets
                .command
                .is_none()
        );
        spec.command = Some("a \"b".into());
        assert_eq!(field(&spec), ("command", "bad_format"));
    }

    #[test]
    fn limits_are_required_and_bounded_by_the_machine() {
        let mut spec = base();
        spec.resources = DeployResources::default();
        assert_eq!(refused(&spec), Refusal::LimitsRequired);
        spec.resources = DeployResources {
            cpu_millis: Some(1000),
            memory_bytes: None,
        };
        assert_eq!(refused(&spec), Refusal::LimitsRequired);
        spec.resources = DeployResources {
            cpu_millis: None,
            memory_bytes: Some(GIB),
        };
        assert_eq!(refused(&spec), Refusal::LimitsRequired);
        for (cpu, memory, reason) in [
            (125, GIB, "cpu_step"),
            (0, GIB, "cpu_bounds"),
            (8_125, GIB, "cpu_step"),
            (9_000, GIB, "cpu_bounds"),
            (1000, GIB + 1, "memory_step"),
            (1000, 64 * 1024 * 1024, "memory_bounds"),
            (1000, 17 * GIB, "memory_bounds"),
        ] {
            spec.resources = DeployResources {
                cpu_millis: Some(cpu),
                memory_bytes: Some(memory),
            };
            assert_eq!(field(&spec), ("resources", reason), "{cpu} {memory}");
        }
        // Les bornes incluses.
        spec.resources = DeployResources {
            cpu_millis: Some(250),
            memory_bytes: Some(128 * 1024 * 1024),
        };
        assert!(validate(&spec, machine()).is_ok());
        spec.resources = DeployResources {
            cpu_millis: Some(8_000),
            memory_bytes: Some(16 * GIB),
        };
        assert!(validate(&spec, machine()).is_ok());
        // Sans machine connue : seul le plancher.
        spec.resources = DeployResources {
            cpu_millis: Some(64_000),
            memory_bytes: Some(1024 * GIB),
        };
        assert!(validate(&spec, None).is_ok());
    }

    #[test]
    fn a_setting_outside_the_closed_plan_is_refused_whatever_the_rest_says() {
        let mut spec = base();
        spec.extra
            .insert("privileged".into(), serde_json::json!(true));
        assert_eq!(refused(&spec), Refusal::SettingNotAllowed);
        // Même devant une autre faute : le plan fermé passe d'abord.
        spec.name = "bad name".into();
        assert_eq!(refused(&spec), Refusal::SettingNotAllowed);
        // `reauth` est la confirmation de l'acte, pas un réglage.
        let mut spec = base();
        spec.extra
            .insert("reauth".into(), serde_json::json!({"password": "x"}));
        assert!(validate(&spec, machine()).is_ok());
    }

    #[test]
    fn the_operation_plan_has_five_steps_and_cannot_be_cancelled_from_creation_on() {
        let plan = op_plan().expect("plan");
        let names: Vec<&str> = plan.steps().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["check", "pull", "create", "start", "watch"]);
        assert_eq!(plan.no_return().map(|n| n.as_str()), Some("create"));
        let destructive: Vec<&str> = plan.destructive_steps().map(|s| s.name.as_str()).collect();
        assert_eq!(destructive, ["create", "start"]);
    }
}
