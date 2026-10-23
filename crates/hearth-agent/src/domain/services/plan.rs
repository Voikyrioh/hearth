//! Le plan du conteneur que Hearth crée : FERMÉ (HRT-68, ADR-0041, BR-DEPLOY-035).
//!
//! [`ContainerPlan`] est tout ce que l'agent peut demander au moteur pour créer un service. Il n'a AUCUN
//! champ pour un mode privilégié, une capacité, le réseau ou les processus du serveur, un périphérique, une
//! option de sécurité, le socket du moteur, ni un chemin du serveur autre que le dossier géré par Hearth :
//! un réglage hors de cette liste ne peut pas être exprimé, donc ne peut pas être demandé. C'est le
//! constructeur unique : un déploiement par formulaire et (plus tard) par fiche passent par ici
//! ([`plan_from_spec`]) ; le conteneur se crée DEPUIS la description durable, jamais de la demande brute.
//!
//! Le nom de variable, la valeur et la commande restent des [`Secret`] : `Debug` ne montre rien.

use std::collections::BTreeMap;
use std::fmt;

use thiserror::Error;

use super::key::ServiceKey;
use super::spec::{SpecBody, SpecMountKind, SpecPort, SpecRestart, SpecSecrets};
use crate::domain::secret::Secret;

/// Étiquettes posées sur un conteneur créé par Hearth, et sur lui seul (conception technique 6).
pub mod label {
    pub const MANAGED: &str = "io.hearth.managed";
    pub const SERVICE_ID: &str = "io.hearth.service-id";
    pub const INSTALL_ID: &str = "io.hearth.install-id";
    pub const CREATED_BY: &str = "io.hearth.created-by";
    pub const SPEC_REV: &str = "io.hearth.spec-rev";
}

/// Ce que Hearth pose d'office sur tout conteneur qu'il crée, sans champ dans le plan ni dans le formulaire
/// (ADR-0041 §5, BR-DEPLOY-035). Ni l'un ni l'autre ne se règle : c'est la borne, pas une option.
pub mod hardening {
    /// Un programme du service ne gagne jamais plus de droits que ceux avec lesquels il démarre (un binaire à
    /// bit setuid n'élève plus rien).
    pub const NO_NEW_PRIVILEGES: &str = "no-new-privileges";
    /// Le nombre de processus et de fils d'exécution du service : une boucle de `fork` s'arrête là au lieu
    /// d'épuiser la table des processus du serveur. Large pour un serveur de jeu (quelques centaines de fils).
    pub const PIDS_LIMIT: i64 = 4096;
}

/// Un dossier du serveur monté dans le service. Aujourd'hui toujours un dossier GÉRÉ par Hearth :
/// `<racine des services>/<clé>/<sous-dossier>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bind {
    pub host_path: String,
    pub target: String,
    pub read_only: bool,
}

/// Tout ce que l'agent demande au moteur pour créer un service.
pub struct ContainerPlan {
    /// Le nom du conteneur (celui du service).
    pub name: String,
    /// `dépôt:version`.
    pub image: String,
    pub labels: BTreeMap<String, String>,
    pub env: Vec<(Secret, Secret)>,
    /// La commande, en arguments ; `None` : celle de l'image.
    pub command: Option<Vec<Secret>>,
    pub ports: Vec<SpecPort>,
    pub binds: Vec<Bind>,
    /// Redémarre tout seul sauf arrêt voulu depuis Hearth (BR-DEPLOY-021). Seule valeur permise par le
    /// formulaire ; la description la porte.
    pub restart: SpecRestart,
    /// Limites OBLIGATOIRES (BR-DEPLOY-038).
    pub cpu_millis: u32,
    pub memory_bytes: u64,
    pub stdin_open: bool,
    pub stop_grace_s: Option<u32>,
}

impl fmt::Debug for ContainerPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Ni variable, ni commande : seulement leur nombre.
        f.debug_struct("ContainerPlan")
            .field("name", &self.name)
            .field("image", &self.image)
            .field("env", &self.env.len())
            .field("command", &self.command.is_some())
            .field("ports", &self.ports.len())
            .field("binds", &self.binds.len())
            .field("cpu_millis", &self.cpu_millis)
            .field("memory_bytes", &self.memory_bytes)
            .finish_non_exhaustive()
    }
}

/// Ce que la description ne dit pas et que l'agent sait.
#[derive(Debug, Clone)]
pub struct PlanContext<'a> {
    pub key: &'a ServiceKey,
    pub install_id: &'a str,
    /// L'identifiant du compte qui crée.
    pub created_by: &'a str,
    /// La révision de la description dont le conteneur est issu.
    pub rev: u32,
    /// `<données de l'agent>/services` : le dossier géré par Hearth, sous lequel vit un sous-dossier par
    /// service.
    pub managed_root: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum PlanError {
    /// Un dossier du serveur autre que le dossier géré : cette version ne le monte pas (HRT-70).
    #[error("seuls les dossiers gérés par Hearth se montent dans cette version")]
    HostFolder,
    /// La clé n'est pas celle d'un service créé par Hearth.
    #[error("seul un service créé par Hearth se crée")]
    NotAHearthKey,
    /// Une limite manque : un service ne se crée pas sans (BR-DEPLOY-038).
    #[error("limites de mémoire et de processeur obligatoires")]
    LimitsRequired,
}

/// La référence de l'image telle que le moteur la reçoit : `dépôt:version`, et `@sha256:...` ajouté quand la
/// description épingle une empreinte (une fiche, BR-FICHE-009) : le moteur crée alors le conteneur depuis cette
/// image-là et aucune autre, même si la version a été déplacée depuis. Sans empreinte, rien ne change.
pub fn image_reference(image: &super::spec::SpecImage) -> String {
    match &image.digest {
        Some(digest) => format!("{}:{}@{digest}", image.repository, image.tag),
        None => format!("{}:{}", image.repository, image.tag),
    }
}

/// Le constructeur unique du plan (conception technique 9, ADR-0041).
pub fn plan_from_spec(
    body: &SpecBody,
    secrets: &SpecSecrets,
    context: &PlanContext<'_>,
) -> Result<ContainerPlan, PlanError> {
    let id = context
        .key
        .as_str()
        .strip_prefix("h_")
        .ok_or(PlanError::NotAHearthKey)?;
    let (Some(cpu_millis), Some(memory_bytes)) = (body.cpu_millis, body.memory_bytes) else {
        return Err(PlanError::LimitsRequired);
    };
    let mut binds = Vec::with_capacity(body.mounts.len());
    for mount in &body.mounts {
        if mount.kind != SpecMountKind::Managed {
            return Err(PlanError::HostFolder);
        }
        binds.push(Bind {
            host_path: format!(
                "{}/{}/{}",
                context.managed_root.trim_end_matches('/'),
                context.key.as_str(),
                mount.source
            ),
            target: mount.target.clone(),
            read_only: mount.read_only,
        });
    }
    let labels = BTreeMap::from([
        (label::MANAGED.to_owned(), "true".to_owned()),
        (label::SERVICE_ID.to_owned(), id.to_owned()),
        (label::INSTALL_ID.to_owned(), context.install_id.to_owned()),
        (label::CREATED_BY.to_owned(), context.created_by.to_owned()),
        (label::SPEC_REV.to_owned(), context.rev.to_string()),
    ]);
    Ok(ContainerPlan {
        name: body.name.clone(),
        image: image_reference(&body.image),
        labels,
        env: secrets
            .env
            .iter()
            .map(|entry| {
                (
                    Secret::new(entry.name.expose().to_owned()),
                    Secret::new(entry.value.expose().to_owned()),
                )
            })
            .collect(),
        command: secrets.command.as_ref().map(|command| {
            command
                .iter()
                .map(|arg| Secret::new(arg.expose().to_owned()))
                .collect()
        }),
        ports: body.ports.clone(),
        binds,
        restart: body.restart,
        cpu_millis,
        memory_bytes,
        stdin_open: body.stdin_open,
        stop_grace_s: body.stop_grace_s,
    })
}

#[cfg(test)]
mod tests {
    use super::super::deploy::{Machine, validate};
    use super::*;

    const KEY: &str = "h_01J0ABCDEFGHJKMNPQRSTVWXYZ";

    fn plan(spec: serde_json::Value) -> ContainerPlan {
        let spec = serde_json::from_value(spec).expect("spec");
        let valid = validate(
            &spec,
            Some(Machine {
                cpus: 8,
                memory_bytes: 16 << 30,
            }),
        )
        .expect("valide");
        let key = ServiceKey::parse(KEY).expect("clé");
        plan_from_spec(
            &valid.body,
            &valid.secrets,
            &PlanContext {
                key: &key,
                install_id: "inst-1",
                created_by: "marie",
                rev: 1,
                managed_root: "/var/lib/hearth/services",
            },
        )
        .expect("plan")
    }

    fn json() -> serde_json::Value {
        serde_json::json!({
            "name": "web", "image": "nginx:1.27",
            "env": [{"name": "TOKEN", "value": "SENTINEL-VALUE"}],
            "command": "serve --flag",
            "ports": [{"host_port": 8080, "container_port": 80}],
            "mounts": [{"target": "/usr/share/data", "read_only": true}],
            "resources": {"cpu_millis": 500, "memory_bytes": 268_435_456}
        })
    }

    #[test]
    fn the_plan_comes_from_the_description_with_the_hearth_labels_and_limits() {
        let plan = plan(json());
        assert_eq!(plan.name, "web");
        assert_eq!(plan.image, "nginx:1.27");
        assert_eq!(plan.restart, SpecRestart::UnlessStopped);
        assert_eq!((plan.cpu_millis, plan.memory_bytes), (500, 268_435_456));
        assert_eq!(plan.labels["io.hearth.managed"], "true");
        assert_eq!(plan.labels["io.hearth.service-id"], &KEY[2..]);
        assert_eq!(plan.labels["io.hearth.install-id"], "inst-1");
        assert_eq!(plan.labels["io.hearth.created-by"], "marie");
        assert_eq!(plan.labels["io.hearth.spec-rev"], "1");
        assert_eq!(plan.ports.len(), 1);
        assert_eq!(plan.env.len(), 1);
        assert_eq!(plan.command.as_ref().map(Vec::len), Some(2));
    }

    #[test]
    fn a_pinned_image_is_created_by_its_digest() {
        let spec = serde_json::from_value(json()).expect("spec");
        let mut valid = validate(
            &spec,
            Some(Machine {
                cpus: 8,
                memory_bytes: 16 << 30,
            }),
        )
        .expect("valide");
        let key = ServiceKey::parse(KEY).expect("clé");
        let context = PlanContext {
            key: &key,
            install_id: "inst-1",
            created_by: "marie",
            rev: 1,
            managed_root: "/var/lib/hearth/services",
        };
        let digest = format!("sha256:{}", "ab".repeat(32));
        valid.body.image.digest = Some(digest.clone());
        let pinned = plan_from_spec(&valid.body, &valid.secrets, &context).expect("plan");
        assert_eq!(pinned.image, format!("nginx:1.27@{digest}"));
        valid.body.image.digest = None;
        let loose = plan_from_spec(&valid.body, &valid.secrets, &context).expect("plan");
        assert_eq!(loose.image, "nginx:1.27", "sans empreinte, rien ne change");
    }

    #[test]
    fn a_data_folder_is_always_below_the_managed_root_of_this_service() {
        let plan = plan(json());
        assert_eq!(
            plan.binds,
            [Bind {
                host_path: format!("/var/lib/hearth/services/{KEY}/usr+share+data"),
                target: "/usr/share/data".into(),
                read_only: true,
            }]
        );
    }

    #[test]
    fn the_plan_has_no_field_for_the_options_a_privileged_service_would_need() {
        // Garde de forme : ce test casse à la compilation si un champ de ce type apparaît sans que
        // quelqu'un lise cette liste. Toute extension du plan est une décision (ADR-0041).
        let ContainerPlan {
            name: _,
            image: _,
            labels: _,
            env: _,
            command: _,
            ports: _,
            binds: _,
            restart: _,
            cpu_millis: _,
            memory_bytes: _,
            stdin_open: _,
            stop_grace_s: _,
        } = plan(json());
    }

    #[test]
    fn debug_shows_neither_a_variable_nor_the_command() {
        let shown = format!("{:?}", plan(json()));
        assert!(
            !shown.contains("SENTINEL") && !shown.contains("TOKEN") && !shown.contains("serve"),
            "{shown}"
        );
    }

    #[test]
    fn a_description_without_limits_or_with_a_server_folder_makes_no_plan() {
        let spec = serde_json::from_value(json()).expect("spec");
        let valid = validate(&spec, None).expect("valide");
        let key = ServiceKey::parse(KEY).expect("clé");
        let context = PlanContext {
            key: &key,
            install_id: "i",
            created_by: "m",
            rev: 1,
            managed_root: "/r",
        };
        let mut body = valid.body.clone();
        body.memory_bytes = None;
        assert_eq!(
            plan_from_spec(&body, &valid.secrets, &context).err(),
            Some(PlanError::LimitsRequired)
        );
        let mut body = valid.body.clone();
        body.mounts[0].kind = SpecMountKind::HostFolder;
        assert_eq!(
            plan_from_spec(&body, &valid.secrets, &context).err(),
            Some(PlanError::HostFolder)
        );
        let other = ServiceKey::parse("c_abc").expect("clé");
        let context = PlanContext {
            key: &other,
            ..context
        };
        assert_eq!(
            plan_from_spec(&valid.body, &valid.secrets, &context).err(),
            Some(PlanError::NotAHearthKey)
        );
    }
}
