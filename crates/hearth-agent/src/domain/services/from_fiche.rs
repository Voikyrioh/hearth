//! Du brouillon d'une fiche à la description durable d'un service, et les gardes pures d'une installation
//! (HRT-101, BR-FICHE-013, 021, 023, 024, 029 ; ADR-0050).
//!
//! Une installation depuis une fiche est un déploiement : la fiche est traduite en [`ServiceSpecDraft`]
//! (`domain::catalog::translate`), validée par [`OrdinaryDeployment`] (les mêmes règles qu'une image déployée à
//! la main), puis devient la même [`ValidDeploy`] que le formulaire. Rien d'autre ne mène à un conteneur : le
//! plan se construit depuis la description (`plan::plan_from_spec`), plan fermé.
//!
//! Ce module est PUR : aucune E/S, aucun tirage, aucune lecture d'horloge (l'appelant donne l'instant). Le
//! fichier produit à partir des réponses est décrit ici ([`ProducedFile`]) et écrit par l'application, par le
//! port `SafeFs`. Aucune valeur de réponse secrète ne sort de ces types autrement que dans un `Secret`.

use std::collections::BTreeMap;

use hearth_proto::api::deploy::{DeployAccept, DeployPort, DeployProtocol};
use thiserror::Error;

use super::deploy::{
    Invalid, Limits, Machine, ValidDeploy, flatten_target, normalize_target, validate_name,
};
use super::spec::{
    EnvEntry, SpecAgreement, SpecAnswer, SpecBody, SpecError, SpecFiche, SpecImage, SpecMount,
    SpecMountKind, SpecPort, SpecProtocol, SpecRestart, SpecSecrets, SpecSettings,
};
use crate::domain::catalog::loading::model::Fiche;
use crate::domain::catalog::loading::schema::fiche::{Evidence, PlainEvidence, Protocol};
use crate::domain::catalog::loading::schema::settings::ChangeWhen;
use crate::domain::catalog::settings_state::Slot;
use crate::domain::catalog::translate::{DeploymentValidator, ServiceSpecDraft};
use crate::domain::sealed::{AnswerId, MAX_ANSWER_ID_CHARS};
use crate::domain::secret::Secret;

use super::deploy::OrdinaryDeployment;

/// Préfixe, dans les réponses scellées, d'un secret tiré par l'agent. Le tiret ne peut pas entrer en collision
/// avec l'identifiant d'un réglage (`[a-z0-9_]`).
pub const GENERATED_PREFIX: &str = "gen-";

/// Un fichier à créer dans un dossier géré avant la création du conteneur. Son contenu peut porter un secret :
/// c'est le seul endroit où il est volontairement en clair (dans le dossier géré, droits `0600` ou `0640`).
#[derive(Debug)]
pub struct ProducedFile {
    /// Nom du sous-dossier géré qui le contient (le chemin de montage aplati, `flatten_target`).
    pub folder_source: String,
    /// Chemin relatif au dossier géré.
    pub path: String,
    pub owner_uid: u32,
    pub owner_gid: u32,
    pub mode: u32,
    pub content: Secret,
}

/// Ce que l'installation écrit : la même demande jugée que le formulaire, plus les fichiers produits.
#[derive(Debug)]
pub struct FicheDeploy {
    pub valid: ValidDeploy,
    pub files: Vec<ProducedFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FromFicheError {
    #[error("le nom du service est refusé")]
    Name(Invalid),
    /// Le validateur du déploiement ordinaire refuse le brouillon : code fixe `champ:raison`, jamais une valeur.
    #[error("le déploiement ordinaire refuse la description : {0}")]
    Ordinary(String),
    #[error("un dossier de la fiche n'a pas de nom de dossier géré")]
    Folder,
    #[error("un fichier de la fiche pointe un dossier inconnu")]
    FileFolder,
    #[error("un identifiant de réponse est illisible")]
    AnswerId,
    #[error("la description n'est pas lisible : {0}")]
    Spec(SpecError),
}

impl FromFicheError {
    /// Un code fixe par variante, sans jamais de contenu : ce que les messages internes et les traces peuvent dire.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Name(_) => "name",
            Self::Ordinary(_) => "ordinary",
            Self::Folder => "folder",
            Self::FileFolder => "file_folder",
            Self::AnswerId => "answer_id",
            Self::Spec(_) => "spec",
        }
    }
}

/// Ce que la demande apporte en plus du brouillon.
pub struct InstallInputs<'a> {
    pub fiche: &'a Fiche,
    /// Réponses saisies (secrètes comprises) : seules les secrètes sont scellées.
    pub answers: &'a BTreeMap<String, String>,
    /// Secrets tirés par l'agent pour cette installation.
    pub generated: &'a BTreeMap<String, Secret>,
    /// Qui accepte les contrats, et quand (secondes Unix).
    pub accepted_by: &'a str,
    pub accepted_at: i64,
    pub accept: DeployAccept,
}

/// Le droit et le propriétaire d'un fichier produit (BR-FICHE-024) : une image qui tourne en root
/// (propriétaire uid 0) donne `0600` root ; toute autre, `0640` et le propriétaire déclaré par la fiche. Le
/// droit déclaré par la fiche (jusqu'à `0644`) est donc abaissé : un fichier qui porte des réponses n'est
/// jamais lisible par tous.
pub fn produced_file_perms(owner_uid: u32, owner_gid: u32) -> (u32, u32, u32) {
    if owner_uid == 0 {
        (0, 0, 0o600)
    } else {
        (owner_uid, owner_gid, 0o640)
    }
}

/// Du brouillon traduit à la description durable (BR-FICHE-013, 029). Le validateur du déploiement ordinaire
/// juge d'abord le brouillon : une fiche n'a pas plus de droits qu'une image déployée à la main (I1).
pub fn body_and_secrets_from(
    inputs: &InstallInputs<'_>,
    draft: &ServiceSpecDraft,
    name: &str,
    limits: Limits,
) -> Result<FicheDeploy, FromFicheError> {
    validate_name(name).map_err(FromFicheError::Name)?;
    OrdinaryDeployment
        .validate(draft)
        .map_err(FromFicheError::Ordinary)?;

    let mut ports = Vec::with_capacity(draft.ports.len());
    let mut requested = Vec::with_capacity(draft.ports.len());
    for port in &draft.ports {
        let (spec_protocol, deploy_protocol) = match port.protocol {
            Protocol::Tcp => (SpecProtocol::Tcp, DeployProtocol::Tcp),
            Protocol::Udp => (SpecProtocol::Udp, DeployProtocol::Udp),
        };
        ports.push(SpecPort {
            host_port: port.host,
            container_port: port.container,
            protocol: spec_protocol,
        });
        requested.push(DeployPort {
            host_port: port.host,
            container_port: port.container,
            protocol: deploy_protocol,
        });
    }

    // Chaque dossier est un dossier géré, nommé par le chemin aplati de son point de montage.
    let mut mounts = Vec::with_capacity(draft.folders.len());
    for folder in &draft.folders {
        let target = normalize_target(&folder.target).map_err(|_| FromFicheError::Folder)?;
        let source = flatten_target(&target).ok_or(FromFicheError::Folder)?;
        mounts.push(SpecMount {
            kind: SpecMountKind::Managed,
            source,
            target,
            read_only: folder.read_only,
        });
    }

    // Le fichier produit : dans le dossier géré de son dossier, propriétaire et droits décidés ici.
    let mut files = Vec::with_capacity(draft.files.len());
    for file in &draft.files {
        let folder = draft
            .folders
            .iter()
            .find(|f| f.id == file.folder)
            .ok_or(FromFicheError::FileFolder)?;
        let target = normalize_target(&folder.target).map_err(|_| FromFicheError::Folder)?;
        let folder_source = flatten_target(&target).ok_or(FromFicheError::Folder)?;
        let (owner_uid, owner_gid, mode) = produced_file_perms(draft.owner_uid, draft.owner_gid);
        files.push(ProducedFile {
            folder_source,
            path: file.path.clone(),
            owner_uid,
            owner_gid,
            mode,
            content: Secret::new(file.content.expose().to_owned()),
        });
    }

    // Les réponses : la valeur en clair d'une réponse non secrète ; une secrète est scellée à part.
    let mut answers = Vec::new();
    let mut sealed_answers = Vec::new();
    let sealed_settings: Vec<&str> = draft
        .sealed
        .iter()
        .filter_map(|s| s.strip_prefix("setting:"))
        .collect();
    for (id, slot) in draft.settings.iter() {
        let frozen = inputs
            .fiche
            .setting(id)
            .is_some_and(|s| s.change == ChangeWhen::WorldCreation);
        match slot {
            Slot::Value(value) => answers.push(SpecAnswer {
                id: id.to_owned(),
                value: Some(value.clone()),
                secret: false,
                frozen,
            }),
            Slot::Secret if sealed_settings.contains(&id) => {
                let value = inputs.answers.get(id).ok_or(FromFicheError::AnswerId)?;
                answers.push(SpecAnswer {
                    id: id.to_owned(),
                    value: None,
                    secret: true,
                    frozen,
                });
                sealed_answers.push(crate::domain::services::spec::AnswerEntry {
                    id: AnswerId::parse(id).ok_or(FromFicheError::AnswerId)?,
                    value: Secret::new(value.clone()),
                });
            }
            // Un secret imposé par la fiche n'est pas une réponse : il n'est pas gardé.
            Slot::Secret | Slot::Unwritten => {}
        }
    }
    for entry in &draft.sealed {
        let Some(id) = entry.strip_prefix("gen:") else {
            continue;
        };
        let value = inputs.generated.get(id).ok_or(FromFicheError::AnswerId)?;
        let key = format!("{GENERATED_PREFIX}{id}");
        if key.chars().count() > MAX_ANSWER_ID_CHARS {
            return Err(FromFicheError::AnswerId);
        }
        sealed_answers.push(crate::domain::services::spec::AnswerEntry {
            id: AnswerId::parse(&key).ok_or(FromFicheError::AnswerId)?,
            value: Secret::new(value.expose().to_owned()),
        });
    }

    let mut fiche = SpecFiche::new(
        draft.fiche_id.clone(),
        draft.fiche_version,
        draft.fiche_digest.clone(),
    );
    fiche.variant = draft.variant.clone();
    fiche.answers = answers;
    fiche.agreements = draft
        .agreements
        .iter()
        .map(|id| SpecAgreement {
            id: id.clone(),
            accepted_by: inputs.accepted_by.to_owned(),
            accepted_at: inputs.accepted_at,
        })
        .collect();

    let body = SpecBody {
        name: name.to_owned(),
        image: SpecImage {
            repository: draft.image_repository.clone(),
            tag: draft.image_tag.clone(),
            digest: draft.image_digest.clone(),
        },
        ports,
        mounts,
        restart: SpecRestart::UnlessStopped,
        stop_grace_s: Some(draft.stop.grace_s()),
        stdin_open: draft.stdin_open,
        cpu_millis: Some(limits.cpu_millis),
        memory_bytes: Some(limits.memory_bytes),
        settings: SpecSettings::default(),
        fiche: Some(fiche),
    };
    let env = draft
        .env
        .iter()
        .map(|variable| EnvEntry {
            name: Secret::new(variable.name.clone()),
            value: Secret::new(variable.value.expose().to_owned()),
        })
        .collect();
    let command = (!draft.command.is_empty())
        .then(|| draft.command.iter().cloned().map(Secret::new).collect());
    let secrets = SpecSecrets {
        env,
        command,
        answers: sealed_answers,
    };
    // La porte commune avec le formulaire : les bornes de la description et des secrets sont jugées là.
    let valid = ValidDeploy::from_parts(body, secrets, inputs.accept, requested)
        .map_err(FromFicheError::Spec)?;

    Ok(FicheDeploy { valid, files })
}

// ---------------------------------------------------------------------------------------------------
// La machine est-elle assez grande (BR-FICHE-021, BR-FICHE-023)
// ---------------------------------------------------------------------------------------------------

/// Ce qui manque à la machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lack {
    Cpu,
    Memory,
}

/// D'où vient un chiffre : mesuré par Hearth (opposable), annoncé par l'éditeur (jamais bloquant), ou inconnu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Measured,
    Publisher,
    Unmeasured,
}

fn source_of(evidence: &Evidence) -> Source {
    match evidence {
        Evidence::Plain(PlainEvidence::Measured | PlainEvidence::MeasuredIdle) => Source::Measured,
        Evidence::Plain(PlainEvidence::Unmeasured) => Source::Unmeasured,
        Evidence::Publisher(_) => Source::Publisher,
    }
}

/// Le verdict qui BLOQUE. Un minimum non mesuré (éditeur ou inconnu) ne bloque jamais (BR-FICHE-021) ; un besoin
/// de disque annoncé par l'éditeur est un avertissement avec case (BR-FICHE-023), jamais un refus ici.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    Ok,
    /// Le minimum MESURÉ est au-dessus de la machine : aucune confirmation ne permet de passer outre.
    HostTooSmall {
        lack: Lack,
        needed: u64,
        has: u64,
    },
    /// L'espace libre est sous le besoin MESURÉ : refus sans case.
    NoSpace {
        needed: u64,
        free: u64,
        missing: u64,
    },
}

/// Un constat qui n'empêche rien (la case « Je comprends » ou « Continuer malgré le manque de place » le couvre,
/// ou rien du tout).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitWarning {
    /// Le minimum annoncé par l'éditeur dépasse la machine (non mesuré par Hearth).
    PublisherMinimum { lack: Lack, needed: u64, has: u64 },
    /// Le besoin de disque annoncé par l'éditeur dépasse l'espace libre : case obligatoire.
    PublisherSpace { needed: u64, free: u64 },
    /// Hearth ne peut pas dire si la machine est assez grande.
    UnmeasuredMinimum { lack: Lack },
}

/// Le verdict bloquant : processeur, mémoire, puis disque. Le minimum n'est opposable que s'il a été mesuré.
pub fn fit(fiche: &Fiche, machine: Machine, free_disk_bytes: u64) -> Fit {
    let resources = &fiche.file.resources;
    if let Some(needed) = resources.cpu.min_millis
        && source_of(&resources.cpu.evidence) == Source::Measured
    {
        let has = u64::from(machine.cpus) * 1000;
        if u64::from(needed) > has {
            return Fit::HostTooSmall {
                lack: Lack::Cpu,
                needed: u64::from(needed),
                has,
            };
        }
    }
    if let Some(needed) = resources.memory.min_bytes
        && source_of(&resources.memory.evidence) == Source::Measured
        && needed > machine.memory_bytes
    {
        return Fit::HostTooSmall {
            lack: Lack::Memory,
            needed,
            has: machine.memory_bytes,
        };
    }
    if let Some(needed) = resources.disk.install_bytes
        && source_of(&resources.disk.evidence) == Source::Measured
        && needed > free_disk_bytes
    {
        return Fit::NoSpace {
            needed,
            free: free_disk_bytes,
            missing: needed - free_disk_bytes,
        };
    }
    Fit::Ok
}

/// Les constats qui ne bloquent pas : à montrer, jamais à ignorer en silence.
pub fn fit_warnings(fiche: &Fiche, machine: Machine, free_disk_bytes: u64) -> Vec<FitWarning> {
    let resources = &fiche.file.resources;
    let mut out = Vec::new();
    let mut minimum = |lack, needed: Option<u64>, has: u64, evidence: &Evidence| match (
        source_of(evidence),
        needed,
    ) {
        (Source::Publisher, Some(needed)) if needed > has => {
            out.push(FitWarning::PublisherMinimum { lack, needed, has });
        }
        (Source::Unmeasured, Some(_)) | (Source::Unmeasured, None) => {
            out.push(FitWarning::UnmeasuredMinimum { lack });
        }
        _ => {}
    };
    minimum(
        Lack::Cpu,
        resources.cpu.min_millis.map(u64::from),
        u64::from(machine.cpus) * 1000,
        &resources.cpu.evidence,
    );
    minimum(
        Lack::Memory,
        resources.memory.min_bytes,
        machine.memory_bytes,
        &resources.memory.evidence,
    );
    if let Some(needed) = resources.disk.install_bytes
        && source_of(&resources.disk.evidence) == Source::Publisher
        && needed > free_disk_bytes
    {
        out.push(FitWarning::PublisherSpace {
            needed,
            free: free_disk_bytes,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use hearth_proto::api::deploy::DeployResources;

    use super::*;
    use crate::domain::catalog::embedded::EMBEDDED;
    use crate::domain::catalog::test_support::{RICH_FICHE, RICH_SETTINGS, load_texts, rich};
    use crate::domain::catalog::translate::{Request, translate};
    use crate::domain::services::deploy::{Refusal, check_limits};

    const GIB: u64 = 1024 * 1024 * 1024;
    const SENTINEL_PASSWORD: &str = "Sentinel-pw-91Zx";
    const RCON: &str = "Abcdefghijklmnopqrstuvwxyz012345";

    fn terraria() -> Fiche {
        EMBEDDED
            .iter()
            .find(|e| e.dir == "terraria")
            .expect("terraria")
            .load()
            .expect("chargée")
    }

    fn limits() -> Limits {
        Limits {
            cpu_millis: 1000,
            memory_bytes: 2 * GIB,
        }
    }

    fn machine(cpus: u32, memory_gib: u64) -> Machine {
        Machine {
            cpus,
            memory_bytes: memory_gib * GIB,
        }
    }

    #[test]
    fn a_root_image_writes_0600_root_and_others_0640_with_the_declared_owner() {
        assert_eq!(produced_file_perms(0, 0), (0, 0, 0o600));
        assert_eq!(produced_file_perms(0, 77), (0, 0, 0o600));
        assert_eq!(produced_file_perms(1000, 1001), (1000, 1001, 0o640));
    }

    fn rich_with(resources: &str) -> Fiche {
        let marker = RICH_FICHE.find("[resources]").expect("section ressources");
        let end = RICH_FICHE[marker..]
            .find("\n\n")
            .map_or(RICH_FICHE.len(), |n| marker + n);
        let text = format!(
            "{}{}{}",
            &RICH_FICHE[..marker],
            resources,
            &RICH_FICHE[end..]
        );
        load_texts(&text, RICH_SETTINGS).expect("fiche valide")
    }

    #[test]
    fn a_machine_below_a_measured_minimum_is_too_small_without_a_way_through() {
        // processeur mesuré à 1000, mémoire mesurée au repos à 1 Gio, disque annoncé par l'éditeur.
        let fiche = rich();
        assert_eq!(fit(&fiche, machine(2, 8), 100 * GIB), Fit::Ok);
        assert_eq!(
            fit(&fiche, machine(0, 8), 100 * GIB),
            Fit::HostTooSmall {
                lack: Lack::Cpu,
                needed: 1000,
                has: 0
            }
        );
        assert_eq!(
            fit(&fiche, machine(4, 0), 100 * GIB),
            Fit::HostTooSmall {
                lack: Lack::Memory,
                needed: GIB,
                has: 0
            }
        );
        // Le disque annoncé par l'éditeur ne bloque jamais, il avertit.
        assert_eq!(fit(&fiche, machine(4, 8), 0), Fit::Ok);
        assert_eq!(
            fit_warnings(&fiche, machine(4, 8), 0),
            vec![FitWarning::PublisherSpace {
                needed: GIB,
                free: 0
            }]
        );
        // Le disque MESURÉ refuse sans case.
        let measured = rich_with(
            "[resources]\ncpu = { min_millis = 1000, evidence = \"measured\" }\nmemory = { min_bytes = 1073741824, recommended_bytes = 2147483648, evidence = \"measured\" }\ndisk = { install_bytes = 5368709120, evidence = \"measured\" }",
        );
        assert_eq!(
            fit(&measured, machine(4, 8), 2 * GIB),
            Fit::NoSpace {
                needed: 5 * GIB,
                free: 2 * GIB,
                missing: 3 * GIB
            }
        );
        assert_eq!(fit(&measured, machine(4, 8), 5 * GIB), Fit::Ok);
    }

    #[test]
    fn an_unmeasured_minimum_never_blocks() {
        // Terraria : tout `unmeasured`.
        let fiche = terraria();
        assert_eq!(fit(&fiche, machine(0, 0), 0), Fit::Ok);
        let warnings = fit_warnings(&fiche, machine(0, 0), 0);
        assert!(warnings.contains(&FitWarning::UnmeasuredMinimum { lack: Lack::Cpu }));
        assert!(warnings.contains(&FitWarning::UnmeasuredMinimum { lack: Lack::Memory }));
        // Un minimum de l'éditeur plus grand que la machine : avertissement, rien ne bloque.
        let publisher = rich_with(
            "[resources]\ncpu = { min_millis = 8000, evidence = { publisher = \"https://example.org/req\" } }\nmemory = { min_bytes = 17179869184, evidence = { publisher = \"https://example.org/req\" } }\ndisk = { evidence = \"unmeasured\" }",
        );
        assert_eq!(fit(&publisher, machine(2, 4), 0), Fit::Ok);
        let warnings = fit_warnings(&publisher, machine(2, 4), 0);
        assert!(warnings.contains(&FitWarning::PublisherMinimum {
            lack: Lack::Cpu,
            needed: 8000,
            has: 2000
        }));
        assert!(warnings.contains(&FitWarning::PublisherMinimum {
            lack: Lack::Memory,
            needed: 16 * GIB,
            has: 4 * GIB
        }));
    }

    #[test]
    fn a_request_without_memory_limit_is_refused() {
        let missing = |cpu, memory| {
            check_limits(
                DeployResources {
                    cpu_millis: cpu,
                    memory_bytes: memory,
                },
                None,
            )
        };
        assert_eq!(missing(Some(1000), None), Err(Refusal::LimitsRequired));
        assert_eq!(missing(None, Some(GIB)), Err(Refusal::LimitsRequired));
        assert!(missing(Some(1000), Some(GIB)).is_ok());
    }

    fn translate_rich(accept: bool) -> (Fiche, ServiceSpecDraft, BTreeMap<String, String>) {
        let fiche = rich();
        let answers: BTreeMap<String, String> = BTreeMap::new();
        let agreements: BTreeSet<String> = if accept {
            fiche.file.agreement.iter().map(|a| a.id.clone()).collect()
        } else {
            BTreeSet::new()
        };
        let mut generated = BTreeMap::new();
        for secret in &fiche.file.secret {
            generated.insert(secret.id.clone(), Secret::from(RCON));
        }
        let none = BTreeMap::new();
        let optional = BTreeSet::new();
        let draft = translate(
            &fiche,
            &Request {
                variant: None,
                answers: &answers,
                agreements: &agreements,
                host_ports: &none,
                optional_ports: &optional,
                generated: &generated,
                world_imported: false,
            },
        )
        .expect("traduite");
        (fiche, draft, answers)
    }

    #[test]
    fn a_translated_fiche_becomes_the_description_of_an_ordinary_deployment() {
        let (fiche, draft, answers) = translate_rich(true);
        let mut generated = BTreeMap::new();
        for secret in &fiche.file.secret {
            generated.insert(secret.id.clone(), Secret::from(RCON));
        }
        let inputs = InstallInputs {
            fiche: &fiche,
            answers: &answers,
            generated: &generated,
            accepted_by: "acct1",
            accepted_at: 1_790_000_000,
            accept: DeployAccept::default(),
        };
        let out = body_and_secrets_from(&inputs, &draft, "mon-jeu", limits()).expect("jugée");
        let body = &out.valid.body;
        assert_eq!(body.name, "mon-jeu");
        assert_eq!(body.cpu_millis, Some(1000));
        assert_eq!(body.memory_bytes, Some(2 * GIB));
        assert_eq!(body.restart, SpecRestart::UnlessStopped);
        assert!(body.mounts.iter().all(|m| m.kind == SpecMountKind::Managed));
        let described = body.fiche.as_ref().expect("fiche");
        assert_eq!(described.id, fiche.id());
        assert_eq!(described.digest, fiche.digest_hex());
        assert_eq!(described.agreements.len(), fiche.file.agreement.len());
        assert!(
            described
                .agreements
                .iter()
                .all(|a| a.accepted_by == "acct1")
        );
        // Les secrets tirés sont scellés sous `gen-<id>`, jamais en clair dans la description.
        let json = serde_json::to_string(body).expect("json");
        assert!(!json.contains(RCON));
        for secret in &fiche.file.secret {
            let key = format!("{GENERATED_PREFIX}{}", secret.id);
            assert!(
                out.valid
                    .secrets
                    .answers
                    .iter()
                    .any(|a| a.id.as_str() == key && a.value.expose() == RCON),
                "{key}"
            );
        }
        out.valid.secrets.validate().expect("secrets valides");
        // Un nom refusé par les règles du formulaire l'est ici aussi.
        assert!(matches!(
            body_and_secrets_from(&inputs, &draft, "a", limits()),
            Err(FromFicheError::Name(_))
        ));
    }

    #[test]
    fn a_secret_answer_is_sealed_and_never_kept_in_clear_in_the_description() {
        let fiche = rich();
        let secret_id = fiche
            .settings
            .iter()
            .find(|s| s.is_secret() && s.imposed.is_none())
            .map(|s| s.id.clone())
            .expect("la fiche riche a une réponse secrète");
        let mut answers = BTreeMap::new();
        answers.insert(secret_id.clone(), SENTINEL_PASSWORD.to_owned());
        let agreements: BTreeSet<String> =
            fiche.file.agreement.iter().map(|a| a.id.clone()).collect();
        let mut generated = BTreeMap::new();
        for secret in &fiche.file.secret {
            generated.insert(secret.id.clone(), Secret::from(RCON));
        }
        let none = BTreeMap::new();
        let optional = BTreeSet::new();
        let draft = translate(
            &fiche,
            &Request {
                variant: None,
                answers: &answers,
                agreements: &agreements,
                host_ports: &none,
                optional_ports: &optional,
                generated: &generated,
                world_imported: false,
            },
        )
        .expect("traduite");
        let inputs = InstallInputs {
            fiche: &fiche,
            answers: &answers,
            generated: &generated,
            accepted_by: "a1",
            accepted_at: 0,
            accept: DeployAccept::default(),
        };
        let out = body_and_secrets_from(&inputs, &draft, "mon-jeu", limits()).expect("jugée");
        let json = serde_json::to_string(&out.valid.body).expect("json");
        assert!(!json.contains(SENTINEL_PASSWORD));
        let kept = out
            .valid
            .body
            .fiche
            .as_ref()
            .and_then(|f| f.answers.iter().find(|a| a.id == secret_id))
            .expect("réponse gardée");
        assert!(kept.secret && kept.value.is_none());
        assert!(
            out.valid
                .secrets
                .answers
                .iter()
                .any(|a| a.id.as_str() == secret_id && a.value.expose() == SENTINEL_PASSWORD)
        );
    }

    #[test]
    fn terraria_files_are_produced_root_0600_in_the_managed_config_folder() {
        let fiche = terraria();
        let answers = BTreeMap::new();
        let none = BTreeMap::new();
        let agreements = BTreeSet::new();
        let optional = BTreeSet::new();
        let generated = BTreeMap::new();
        let draft = translate(
            &fiche,
            &Request {
                variant: None,
                answers: &answers,
                agreements: &agreements,
                host_ports: &none,
                optional_ports: &optional,
                generated: &generated,
                world_imported: false,
            },
        )
        .expect("traduite");
        let inputs = InstallInputs {
            fiche: &fiche,
            answers: &answers,
            generated: &generated,
            accepted_by: "a1",
            accepted_at: 0,
            accept: DeployAccept::default(),
        };
        let out = body_and_secrets_from(&inputs, &draft, "terraria", limits()).expect("jugée");
        assert_eq!(out.files.len(), 1);
        let file = &out.files[0];
        assert_eq!(file.path, "serverconfig.txt");
        assert_eq!((file.owner_uid, file.owner_gid, file.mode), (0, 0, 0o600));
        assert!(
            out.valid
                .body
                .mounts
                .iter()
                .any(|m| m.source == file.folder_source),
            "le dossier du fichier est monté en entier"
        );
        assert!(file.content.expose().contains("maxplayers=8"));
    }
}
