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
use crate::domain::catalog::loading::schema::fiche::{
    Evidence, PackGenre, PlainEvidence, PortRole, Protocol,
};
use crate::domain::catalog::loading::schema::settings::ChangeWhen;
use crate::domain::catalog::settings_state::{SettingsState, Slot};
use crate::domain::catalog::translate::{DeploymentValidator, ServiceSpecDraft, pack_file_name};
use crate::domain::sealed::{AnswerId, MAX_ANSWER_ID_CHARS};
use crate::domain::secret::Secret;

use super::deploy::OrdinaryDeployment;

/// Préfixe, dans les réponses scellées, d'un secret tiré par l'agent. Le tiret ne peut pas entrer en collision
/// avec l'identifiant d'un réglage (`[a-z0-9_]`).
pub const GENERATED_PREFIX: &str = "gen-";

/// La clé, dans les réponses scellées, du secret que l'agent a tiré pour le secret `id` de la fiche.
pub fn generated_answer_key(id: &str) -> String {
    format!("{GENERATED_PREFIX}{id}")
}

/// L'inverse de [`generated_answer_key`] : l'identifiant du secret de la fiche, ou rien si la clé n'est pas celle
/// d'un secret tiré.
pub fn generated_secret_id(key: &str) -> Option<&str> {
    key.strip_prefix(GENERATED_PREFIX)
}

/// La réponse que la description garde pour un emplacement de réglage (la valeur seulement si elle n'est pas
/// secrète ; un réglage « non écrit » n'a pas de réponse). L'inverse est [`slot_of_answer`].
pub fn answer_of_slot(id: &str, slot: &Slot, frozen: bool) -> Option<SpecAnswer> {
    match slot {
        Slot::Value(value) => Some(SpecAnswer {
            id: id.to_owned(),
            value: Some(value.clone()),
            secret: false,
            frozen,
        }),
        Slot::Secret => Some(SpecAnswer {
            id: id.to_owned(),
            value: None,
            secret: true,
            frozen,
        }),
        Slot::Unwritten => None,
    }
}

/// L'emplacement d'un réglage d'après la réponse gardée : l'inverse de [`answer_of_slot`].
pub fn slot_of_answer(answer: &SpecAnswer) -> Slot {
    match (answer.secret, &answer.value) {
        (true, _) => Slot::Secret,
        (false, Some(value)) => Slot::Value(value.clone()),
        (false, None) => Slot::Unwritten,
    }
}

/// L'état des réglages d'un service d'après les réponses de sa description : un emplacement par réglage de la
/// fiche, « non écrit » quand il n'a pas de réponse.
pub fn state_of_answers(fiche: &Fiche, answers: &[SpecAnswer]) -> SettingsState {
    let slots: BTreeMap<String, Slot> = fiche
        .settings
        .iter()
        .map(|setting| {
            let slot = answers
                .iter()
                .find(|a| a.id == setting.id)
                .map_or(Slot::Unwritten, slot_of_answer);
            (setting.id.clone(), slot)
        })
        .collect();
    SettingsState::from_slots(slots)
}

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

/// Où ranger le pack déposé (HRT-124) : le sous-dossier géré des packs et le nom FIXE du fichier. Le contenu vient
/// de la zone d'attente, jamais de la description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackPlacement {
    pub folder_source: String,
    pub file_name: &'static str,
    pub genre: PackGenre,
}

/// Ce que l'installation écrit : la même demande jugée que le formulaire, plus les fichiers produits.
#[derive(Debug)]
pub struct FicheDeploy {
    pub valid: ValidDeploy,
    pub files: Vec<ProducedFile>,
    pub pack: Option<PackPlacement>,
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
            Slot::Value(_) => answers.extend(answer_of_slot(id, slot, frozen)),
            Slot::Secret if sealed_settings.contains(&id) => {
                let value = inputs.answers.get(id).ok_or(FromFicheError::AnswerId)?;
                answers.extend(answer_of_slot(id, slot, frozen));
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
        let key = generated_answer_key(id);
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

    // Le pack déposé : le dossier géré des packs (déclaré par la fiche) et le nom fixe du fichier.
    let pack = match draft.pack {
        None => None,
        Some(genre) => {
            let way = inputs
                .fiche
                .file
                .mods
                .as_ref()
                .and_then(|m| m.pack.as_ref())
                .ok_or(FromFicheError::Folder)?;
            let folder = draft
                .folders
                .iter()
                .find(|f| f.id == way.folder)
                .ok_or(FromFicheError::Folder)?;
            let target = normalize_target(&folder.target).map_err(|_| FromFicheError::Folder)?;
            Some(PackPlacement {
                folder_source: flatten_target(&target).ok_or(FromFicheError::Folder)?,
                file_name: pack_file_name(genre),
                genre,
            })
        }
    };

    Ok(FicheDeploy { valid, files, pack })
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

/// Un port publié d'un service de jeu, rapproché de ce que la fiche en dit (BR-JEU-017, BR-JEU-019).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GamePortFact {
    pub host_port: u16,
    pub protocol: SpecProtocol,
    /// Le rôle que la fiche donne à ce port ; `None` : la fiche ne reconnaît plus ce port.
    pub role: Option<PortRole>,
    /// Le numéro du serveur diffère de celui que propose la fiche : le rappel « mettez à jour votre box » (BR-JEU-019).
    pub changed: bool,
}

/// Les ports publiés d'un service, rapprochés de ceux de la fiche par numéro de service et protocole. Un protocole que
/// la fiche ne sait pas dire (`sctp`) n'est rapproché de rien.
pub fn game_ports(fiche: &Fiche, ports: &[SpecPort]) -> Vec<GamePortFact> {
    ports
        .iter()
        .map(|port| {
            let wanted = match port.protocol {
                SpecProtocol::Tcp => Some(Protocol::Tcp),
                SpecProtocol::Udp => Some(Protocol::Udp),
                SpecProtocol::Sctp => None,
            };
            let declared = wanted.and_then(|protocol| {
                fiche.file.port.iter().find(|candidate| {
                    candidate.container == port.container_port && candidate.protocol == protocol
                })
            });
            GamePortFact {
                host_port: port.host_port,
                protocol: port.protocol,
                role: declared.map(|declared| declared.role),
                changed: declared
                    .and_then(|declared| declared.default_host)
                    .is_some_and(|default| default != port.host_port),
            }
        })
        .collect()
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
        // Une fiche dont tout est `unmeasured` (Terraria a sa mémoire et son disque mesurés au repos, HRT-114).
        let fiche = rich_with(
            "[resources]
cpu = { evidence = \"unmeasured\" }
memory = { evidence = \"unmeasured\" }
disk = { evidence = \"unmeasured\" }",
        );
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
                pack: None,
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
                pack: None,
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
                pack: None,
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

    #[test]
    fn a_published_port_is_matched_to_the_fiche_and_flags_a_changed_number() {
        let fiche = terraria();
        let declared = fiche.file.port.first().expect("un port");
        let default_host = declared.default_host.expect("un port proposé");
        let (container, protocol) = (declared.container, declared.protocol);
        let spec_protocol = match protocol {
            Protocol::Tcp => SpecProtocol::Tcp,
            Protocol::Udp => SpecProtocol::Udp,
        };
        let same = SpecPort {
            host_port: default_host,
            container_port: container,
            protocol: spec_protocol,
        };
        let moved = SpecPort {
            host_port: default_host.wrapping_add(1),
            ..same
        };
        let facts = game_ports(&fiche, &[same, moved]);
        assert_eq!(facts[0].role, Some(declared.role));
        assert!(
            !facts[0].changed,
            "le numéro proposé n'est pas un changement"
        );
        assert!(
            facts[1].changed,
            "un autre numéro appelle le rappel de la box"
        );
    }

    #[test]
    fn an_unknown_port_or_sctp_is_matched_to_nothing() {
        let fiche = terraria();
        let declared = fiche.file.port.first().expect("un port");
        let unknown = SpecPort {
            host_port: 40000,
            container_port: declared.container.wrapping_add(1000),
            protocol: SpecProtocol::Tcp,
        };
        let sctp = SpecPort {
            host_port: 40001,
            container_port: declared.container,
            protocol: SpecProtocol::Sctp,
        };
        for fact in game_ports(&fiche, &[unknown, sctp]) {
            assert_eq!(fact.role, None);
            assert!(!fact.changed);
        }
    }
}
