//! Traduction d'une fiche en description de service (conception technique 5, 6 ; invariant I1).
//!
//! Fiche + variante + réponses + secrets (reçus déjà tirés : le domaine ne tire aucun nombre aléatoire) +
//! contrats acceptés donnent un `ServiceSpecDraft`, et rien d'autre. Ce module ne construit JAMAIS un
//! `ContainerPlan` : c'est le constructeur unique du déploiement ordinaire (ADR-0041). Le brouillon n'a aucun
//! champ pour un mode privilégié, une capacité, le réseau ou les processus de l'hôte, un périphérique, une
//! option de sécurité, le socket du moteur, ni un chemin du serveur : un dossier est désigné par son
//! identifiant, sa source est toujours `<racine de données>/<service>/<id>` (décidée par l'appelant).
//!
//! `ServiceSpecDraft` est un type minimal local ; `domain::services::from_fiche` (HRT-101) le transforme en
//! description durable (`SpecBody`, `SpecSecrets`) après l'avoir fait juger par le validateur du déploiement
//! ordinaire (`DeploymentValidator`, ci-dessous ; son implémentation est
//! `domain::services::deploy::OrdinaryDeployment`, HRT-68).

use std::collections::{BTreeMap, BTreeSet};

use crate::domain::secret::Secret;

use super::formats::{Refused, format_for};
use super::loading::model::Fiche;
use super::loading::schema::fiche::{
    ChannelKind, FileMode, FolderRole, GameUpdates, OwnerKind, Platform, PortRole, Protocol,
    Publish, When,
};
use super::loading::schema::settings::ArgForm;
use super::loading::value::{Charset, first_refused_char};
use super::settings_state::{
    InstallContext, Refusal, Resolved, ResolvedValue, SettingsState, check_rules, chosen_host_port,
    initial_state, resolve_install,
};
use super::stop_plan::{StopPlan, save_plan, stop_plan};

/// Valeur d'une variable d'environnement du service.
#[derive(Debug)]
pub enum EnvValue {
    Plain(String),
    Secret(Secret),
}

impl EnvValue {
    pub fn expose(&self) -> &str {
        match self {
            Self::Plain(v) => v,
            Self::Secret(s) => s.expose(),
        }
    }
}

#[derive(Debug)]
pub struct EnvDraft {
    pub name: String,
    pub value: EnvValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortDraft {
    pub id: String,
    pub protocol: Protocol,
    pub role: PortRole,
    pub container: u16,
    pub host: u16,
}

/// Un dossier géré du service. Aucun champ de chemin du serveur (I2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderDraft {
    pub id: String,
    pub role: FolderRole,
    pub target: String,
    pub read_only: bool,
    pub backup: bool,
}

/// Un fichier de réglages à CRÉER (il n'existe pas) dans un dossier géré. Le contenu peut porter un secret.
#[derive(Debug)]
pub struct FileDraft {
    pub channel: String,
    pub folder: String,
    pub path: String,
    pub mode: FileMode,
    pub content: Secret,
    pub holds_secret: bool,
}

#[derive(Debug)]
pub struct ServiceSpecDraft {
    pub fiche_id: String,
    pub fiche_version: u32,
    pub fiche_digest: String,
    pub variant: Option<String>,
    pub image_repository: String,
    pub image_tag: String,
    pub image_digest: Option<String>,
    pub game_updates: GameUpdates,
    pub owner_uid: u32,
    pub owner_gid: u32,
    /// Ports publiés seulement : un port `never` (console distante) n'y figure jamais.
    pub ports: Vec<PortDraft>,
    pub folders: Vec<FolderDraft>,
    pub env: Vec<EnvDraft>,
    /// Arguments de la commande : des jetons séparés, jamais une ligne, jamais d'interpréteur.
    pub command: Vec<String>,
    pub files: Vec<FileDraft>,
    /// Entrée standard ouverte : seulement quand l'arrêt ou l'enregistrement a un ordre à envoyer.
    pub stdin_open: bool,
    pub stop: StopPlan,
    /// Contrats acceptés (identifiants), consignés avec la description.
    pub agreements: Vec<String>,
    /// Secrets à sceller : `gen:<id>` pour les secrets générés, `setting:<id>` pour les secrets saisis.
    pub sealed: Vec<String>,
    pub settings: SettingsState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslateError {
    PlatformUnavailable,
    UnknownVariant(String),
    /// Contrats de la fiche non acceptés (`AGREEMENT_REQUIRED`).
    AgreementRequired(Vec<String>),
    Setting(Refusal),
    GeneratedSecretMissing(String),
    GeneratedSecretInvalid(String),
    /// Port du serveur sous 1024, ou deux ports publiés sur le même numéro et le même protocole.
    PortRefused {
        id: String,
    },
    EnvConflict(String),
    /// Une variable dérivée n'a pas de valeur source.
    DeriveSourceMissing(String),
    /// Défense en profondeur : une valeur d'argument hors de l'alphabet fermé (jamais la valeur).
    ArgumentRefused {
        id: String,
        character: char,
    },
    FileValueRefused {
        channel: String,
    },
}

impl From<Refusal> for TranslateError {
    fn from(refusal: Refusal) -> Self {
        Self::Setting(refusal)
    }
}

pub struct Request<'a> {
    pub variant: Option<&'a str>,
    /// Réponses de l'utilisateur (identifiant de réglage vers texte, secrets saisis compris).
    pub answers: &'a BTreeMap<String, String>,
    /// Contrats acceptés par un geste de l'utilisateur.
    pub agreements: &'a BTreeSet<String>,
    /// Port du serveur choisi par port de la fiche.
    pub host_ports: &'a BTreeMap<String, u16>,
    /// Ports `optional` que l'utilisateur publie aussi.
    pub optional_ports: &'a BTreeSet<String>,
    /// Secrets générés par l'appelant, déjà tirés.
    pub generated: &'a BTreeMap<String, Secret>,
    pub world_imported: bool,
}

fn variant_of<'a>(
    fiche: &'a Fiche,
    requested: Option<&str>,
) -> Result<Option<&'a str>, TranslateError> {
    let variants = &fiche.file.variant;
    match requested {
        None => Ok(variants.iter().find(|v| v.default).map(|v| v.id.as_str())),
        Some(id) => variants
            .iter()
            .find(|v| v.id == id)
            .map(|v| Some(v.id.as_str()))
            .ok_or_else(|| TranslateError::UnknownVariant(id.chars().take(32).collect())),
    }
}

fn applies(only: &Option<Vec<String>>, variant: Option<&str>) -> bool {
    match (only, variant) {
        (None, _) => true,
        (Some(list), Some(v)) => list.iter().any(|o| o == v),
        (Some(_), None) => false,
    }
}

/// Fiche, variante, réponses, secrets et contrats donnent un `ServiceSpecDraft`.
pub fn translate(fiche: &Fiche, request: &Request<'_>) -> Result<ServiceSpecDraft, TranslateError> {
    let file = &fiche.file;
    if matches!(file.platform, Platform::Unavailable(_)) {
        return Err(TranslateError::PlatformUnavailable);
    }
    let variant = variant_of(fiche, request.variant)?;

    // I8 : aucun contrat n'est accepté d'office.
    let missing: Vec<String> = file
        .agreement
        .iter()
        .filter(|a| !request.agreements.contains(&a.id))
        .map(|a| a.id.clone())
        .collect();
    if !missing.is_empty() {
        return Err(TranslateError::AgreementRequired(missing));
    }

    // Secrets générés : tirés par l'appelant, vérifiés ici (longueur, alphabet).
    for secret in &file.secret {
        let Some(value) = request.generated.get(&secret.id) else {
            return Err(TranslateError::GeneratedSecretMissing(secret.id.clone()));
        };
        let text = value.expose();
        if text.chars().count() != secret.length as usize
            || !text.chars().all(|c| c.is_ascii_alphanumeric())
        {
            return Err(TranslateError::GeneratedSecretInvalid(secret.id.clone()));
        }
    }

    // Ports publiés.
    let ctx = InstallContext {
        variant,
        world_imported: request.world_imported,
        host_ports: request.host_ports,
        generated: request.generated,
    };
    let mut ports: Vec<PortDraft> = Vec::new();
    for port in file
        .port
        .iter()
        .filter(|p| applies(&p.only_variants, variant))
    {
        let published = match port.publish {
            Publish::Always => true,
            Publish::Optional => request.optional_ports.contains(&port.id),
            Publish::Never => false,
        };
        if !published || port.role == PortRole::RemoteConsole {
            continue;
        }
        let host = chosen_host_port(fiche, &ctx, &port.id).unwrap_or(port.container);
        let container = match port.mapping {
            super::loading::schema::fiche::PortMapping::Nat => port.container,
            super::loading::schema::fiche::PortMapping::SameNumber => host,
        };
        if host < super::loading::limits::MIN_PORT || container < super::loading::limits::MIN_PORT {
            return Err(TranslateError::PortRefused {
                id: port.id.clone(),
            });
        }
        if ports
            .iter()
            .any(|p| p.host == host && p.protocol == port.protocol)
        {
            return Err(TranslateError::PortRefused {
                id: port.id.clone(),
            });
        }
        ports.push(PortDraft {
            id: port.id.clone(),
            protocol: port.protocol,
            role: port.role,
            container,
            host,
        });
    }

    let folders: Vec<FolderDraft> = file
        .folder
        .iter()
        .filter(|d| applies(&d.only_variants, variant))
        .map(|d| FolderDraft {
            id: d.id.clone(),
            role: d.role,
            target: d.target.clone(),
            read_only: d.read_only,
            backup: d.backup,
        })
        .collect();

    // Réglages.
    let resolved = resolve_install(fiche, &ctx, request.answers)?;
    let settings = initial_state(fiche, &resolved);
    check_rules(fiche, &settings)?;

    // Variables d'environnement.
    let mut env: Vec<EnvDraft> = Vec::new();
    let mut push_env = |name: &str, value: EnvValue| -> Result<(), TranslateError> {
        if env.iter().any(|e| e.name == name) {
            return Err(TranslateError::EnvConflict(name.to_owned()));
        }
        env.push(EnvDraft {
            name: name.to_owned(),
            value,
        });
        Ok(())
    };
    for var in &file.env {
        let wanted = match var.when.unwrap_or(When::Always) {
            When::Always => true,
            When::WorldImported => request.world_imported,
            When::WorldNew => !request.world_imported,
        };
        if !wanted {
            continue;
        }
        let value = if let Some(v) = &var.value {
            EnvValue::Plain(v.clone())
        } else if let Some(derive) = &var.derive {
            let from = resolved
                .iter()
                .find(|r| r.id == derive.from)
                .and_then(|r| r.value.as_ref())
                .ok_or_else(|| TranslateError::DeriveSourceMissing(derive.from.clone()))?;
            EnvValue::Plain(format!(
                "{}{}{}",
                derive.prefix.as_deref().unwrap_or(""),
                from.expose(),
                derive.suffix.as_deref().unwrap_or("")
            ))
        } else if let Some(secret) = &var.secret {
            let value = request
                .generated
                .get(secret)
                .ok_or_else(|| TranslateError::GeneratedSecretMissing(secret.clone()))?;
            EnvValue::Secret(Secret::new(value.expose().to_owned()))
        } else if let Some(owner) = var.owner {
            EnvValue::Plain(match owner {
                OwnerKind::Uid => file.owner.uid.to_string(),
                OwnerKind::Gid => file.owner.gid.to_string(),
            })
        } else {
            continue;
        };
        push_env(&var.name, value)?;
    }
    if let Some(v) = variant.and_then(|id| file.variant.iter().find(|x| x.id == id)) {
        for e in &v.env {
            push_env(&e.name, EnvValue::Plain(e.value.clone()))?;
        }
    }
    // Le contrat accepté par le geste de l'utilisateur est la SEULE source de sa valeur d'acceptation.
    for agreement in &file.agreement {
        if fiche
            .channel(&agreement.binds.channel)
            .is_some_and(|c| c.kind == ChannelKind::Env)
        {
            push_env(
                &agreement.binds.key,
                EnvValue::Plain(agreement.binds.accepted_value.clone()),
            )?;
        }
    }
    let mut command: Vec<String> = Vec::new();
    for r in &resolved {
        let Some(setting) = fiche.setting(&r.id) else {
            continue;
        };
        let Some(channel) = fiche.channel(&r.channel) else {
            continue;
        };
        let Some(value) = &r.value else { continue };
        match channel.kind {
            ChannelKind::Env => {
                let v = match value {
                    ResolvedValue::Plain(p) => EnvValue::Plain(p.clone()),
                    ResolvedValue::Secret(s) => {
                        EnvValue::Secret(Secret::new(s.expose().to_owned()))
                    }
                };
                push_env(&r.key, v)?;
            }
            ChannelKind::Args => {
                let (Some(arg), ResolvedValue::Plain(text)) = (&setting.arg, value) else {
                    // Un secret en argument est impossible (refusé au chargement) : défense en profondeur.
                    return Err(TranslateError::ArgumentRefused {
                        id: r.id.clone(),
                        character: '*',
                    });
                };
                match arg.form {
                    ArgForm::FlagIfTrue => {
                        let on = matches!(&setting.spec, super::loading::model::ValueSpec::Bool { on, .. } if on == text);
                        if on {
                            command.push(arg.flag.clone());
                        }
                    }
                    ArgForm::FlagValue => {
                        let numeric =
                            matches!(&setting.spec, super::loading::model::ValueSpec::Int { .. });
                        // Défense en profondeur, sans exemption : un entier est revalidé comme entier
                        // (le signe `-` d'un entier ne nomme pas une option), tout le reste par l'alphabet.
                        let refused = if numeric {
                            super::loading::model::validate_value(&setting.spec, text)
                                .err()
                                .map(|_| {
                                    text.chars()
                                        .find(|c| !c.is_ascii_digit() && *c != '-')
                                        .unwrap_or('#')
                                })
                        } else {
                            first_refused_char(Charset::ArgSafe, text)
                        };
                        if let Some(character) = refused {
                            return Err(TranslateError::ArgumentRefused {
                                id: r.id.clone(),
                                character,
                            });
                        }
                        command.push(arg.flag.clone());
                        command.push(text.clone());
                    }
                }
            }
            ChannelKind::File | ChannelKind::Unknown => {}
        }
    }

    // Fichiers de réglages à créer.
    let mut files: Vec<FileDraft> = Vec::new();
    for channel in file
        .channel
        .iter()
        .filter(|c| c.kind == ChannelKind::File && applies(&c.only_variants, variant))
    {
        let (Some(folder), Some(path), Some(mode)) = (&channel.folder, &channel.path, channel.mode)
        else {
            continue;
        };
        let Some(format) = format_for(channel) else {
            continue;
        };
        let mut holds_secret = false;
        let pairs: Vec<(String, String)> = resolved
            .iter()
            .filter(|r| r.channel == channel.id)
            .filter_map(|r: &Resolved| {
                r.value.as_ref().map(|v| {
                    holds_secret |= matches!(v, ResolvedValue::Secret(_));
                    (r.key.clone(), v.expose().to_owned())
                })
            })
            .collect();
        let content = format.create(&pairs).map_err(|e: Refused| {
            let _ = e;
            TranslateError::FileValueRefused {
                channel: channel.id.clone(),
            }
        })?;
        let text = String::from_utf8(content).map_err(|_| TranslateError::FileValueRefused {
            channel: channel.id.clone(),
        })?;
        files.push(FileDraft {
            channel: channel.id.clone(),
            folder: folder.clone(),
            path: path.clone(),
            mode,
            content: Secret::new(text),
            holds_secret,
        });
    }

    let stop = stop_plan(fiche);
    let stdin_open = stop.sends_orders() || save_plan(fiche).is_some();
    let mut sealed: Vec<String> = file
        .secret
        .iter()
        .map(|s| format!("gen:{}", s.id))
        .collect();
    for r in &resolved {
        if !r.imposed && matches!(r.value, Some(ResolvedValue::Secret(_))) {
            sealed.push(format!("setting:{}", r.id));
        }
    }

    Ok(ServiceSpecDraft {
        fiche_id: file.id.clone(),
        fiche_version: file.version,
        fiche_digest: fiche.digest_hex(),
        variant: variant.map(str::to_owned),
        image_repository: file.image.repository.clone(),
        image_tag: variant
            .and_then(|id| file.variant.iter().find(|v| v.id == id))
            .and_then(|v| v.image_tag.clone())
            .unwrap_or_else(|| file.image.tag.clone()),
        image_digest: variant
            .and_then(|id| file.variant.iter().find(|v| v.id == id))
            .and_then(|v| v.image_digest.clone())
            .or_else(|| file.image.digest.clone()),
        game_updates: file.image.game_updates,
        owner_uid: file.owner.uid,
        owner_gid: file.owner.gid,
        ports,
        folders,
        env,
        command,
        files,
        stdin_open,
        stop,
        agreements: file.agreement.iter().map(|a| a.id.clone()).collect(),
        sealed,
        settings,
    })
}

// ---- Validateur du déploiement ordinaire : l'interface attendue -----------------------------------------------

/// Ce que le déploiement ordinaire exige d'une description. Le vrai validateur existe depuis HRT-68 :
/// `domain::services::deploy::OrdinaryDeployment`, qui applique les MÊMES fonctions que le déploiement fait à la
/// main (nom et version de l'image, variables, ports, dossiers, commande), pour que la description d'une fiche
/// soit refusée par les mêmes règles. HRT-101 l'a branché (`domain::services::from_fiche`) ; ce fichier n'en tient aucune copie.
pub trait DeploymentValidator {
    fn validate(&self, spec: &ServiceSpecDraft) -> Result<(), String>;
}
