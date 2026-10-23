//! Préparer l'installation d'un serveur de jeu depuis une fiche (HRT-101, ADR-0050, BR-FICHE-012 à 029).
//!
//! Une installation depuis une fiche est un déploiement ordinaire : ce module ne fait que juger la demande et la
//! traduire en la même [`ValidDeploy`] que le formulaire, puis [`Deployments`](super::deployments::Deployments)
//! la fait suivre le chemin de toute image (nom libre, ports libres, deux déploiements au plus, description
//! durable, plan fermé). Il ne crée rien, n'écrit rien : il rend les fichiers à produire et ce que le journal
//! d'activité doit dire.
//!
//! Ordre des refus, tous SYNCHRONES, typés, sans jamais citer une valeur reçue :
//! 1. la demande ne dit que la fiche (`image`, `env`, `ports`, `mounts`, `command` vides) ;
//! 2. la fiche est proposée par cet agent (`FICHE_UNKNOWN`) et c'est celle que le client a vue (`FICHE_CHANGED`) ;
//! 3. le nom, puis les réponses (`SETTING_REFUSED` : le réglage, la raison et au plus UN caractère) ;
//! 4. les contrats (`AGREEMENT_REQUIRED` : l'agent refuse même si le client est modifié, BR-FICHE-058) ;
//! 5. les secrets que l'agent tire (jamais d'un hasard faible) ;
//! 6. la traduction, le validateur du déploiement ordinaire (une fiche n'a pas plus de droits qu'une image) ;
//! 7. les limites OBLIGATOIRES, puis la machine (`HOST_TOO_SMALL`) et l'espace (`NO_SPACE`, ou la case).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use hearth_proto::api::deploy::{DeploySource, DeploySpec};

use super::catalog::CatalogService;
use super::deployments::DeployError;
use super::ports::{Clock, SecretRandom};
use crate::domain::audit::Actor;
use crate::domain::catalog::generate::draw_secret;
use crate::domain::catalog::settings_state::Reason as SettingReason;
use crate::domain::catalog::translate::{Request, TranslateError, translate};
use crate::domain::secret::Secret;
use crate::domain::services::deploy::{
    Invalid, Limits, Machine, Refusal, check_limits, validate_name,
};
use crate::domain::services::from_fiche::{
    Fit, FitWarning, InstallInputs, Lack, ProducedFile, body_and_secrets_from, fit, fit_warnings,
};
use crate::domain::services::plan::image_reference;

/// Ce que le journal d'activité dit d'une installation : aucun champ pour une valeur.
#[derive(Debug, Clone)]
pub struct InstallAudit {
    pub fiche: String,
    pub version: u32,
    pub image: String,
    pub answers: u32,
    pub cpu_millis: u32,
    pub memory_bytes: u64,
    /// Le titre de chaque contrat accepté.
    pub agreements: Vec<String>,
}

/// Une demande jugée : la même description qu'un déploiement ordinaire, les fichiers à produire avant la création
/// du conteneur, et le texte du journal.
pub struct Prepared {
    pub valid: crate::domain::services::deploy::ValidDeploy,
    pub files: Vec<ProducedFile>,
    pub audit: InstallAudit,
}

pub struct FicheInstaller {
    catalog: Arc<CatalogService>,
    random: Arc<dyn SecretRandom>,
    clock: Arc<dyn Clock>,
}

fn invalid(field: &'static str, reason: &'static str) -> DeployError {
    DeployError::Invalid(Invalid { field, reason })
}

/// Le code stable d'une raison de refus d'une réponse (jamais le texte reçu).
fn reason_code(reason: &SettingReason) -> &'static str {
    match reason {
        SettingReason::Bounds => "bounds",
        SettingReason::Choice => "choice",
        SettingReason::Charset => "charset",
        SettingReason::Length => "length",
        SettingReason::Frozen => "frozen",
        SettingReason::Imposed => "imposed",
        SettingReason::Retired => "retired",
        SettingReason::UnknownId => "unknown_id",
        SettingReason::Variant => "variant",
        SettingReason::Version => "version",
        SettingReason::Check(_) => "check",
        SettingReason::CarriedByWorld => "carried_by_world",
    }
}

impl FicheInstaller {
    pub fn new(
        catalog: Arc<CatalogService>,
        random: Arc<dyn SecretRandom>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            catalog,
            random,
            clock,
        }
    }

    /// Juge la demande. `machine` et `free_disk` absents (le moteur ne les a pas dits) : rien n'est opposé, comme
    /// pour une image à la main.
    pub fn prepare(
        &self,
        spec: &DeploySpec,
        actor: &Actor,
        machine: Option<Machine>,
        free_disk: Option<u64>,
    ) -> Result<Prepared, DeployError> {
        let Some(DeploySource::Fiche {
            id,
            version,
            digest,
            variant,
            answers,
            agreements,
            ports,
            optional_ports,
            world_imported,
        }) = &spec.source
        else {
            return Err(DeployError::SettingNotAllowed);
        };
        // 1. La fiche décide l'image, les variables, les ports, les dossiers et la commande : pas la demande.
        if !spec.image.trim().is_empty()
            || spec.tag.is_some()
            || !spec.env.is_empty()
            || !spec.ports.is_empty()
            || !spec.mounts.is_empty()
            || spec.command.is_some()
            || spec.extra.keys().any(|key| key != "reauth")
        {
            return Err(DeployError::SettingNotAllowed);
        }
        // La reprise d'un monde existant n'est pas dans cette version.
        if *world_imported {
            return Err(invalid("source", "world_import_unavailable"));
        }
        // 2. La fiche est proposée par cet agent, et c'est celle que le client a vue.
        let fiche = self.catalog.fiche(id).ok_or(DeployError::FicheUnknown)?;
        if fiche.file.version != *version || !fiche.digest_hex().eq_ignore_ascii_case(digest) {
            return Err(DeployError::FicheChanged);
        }
        if matches!(
            fiche.file.platform,
            crate::domain::catalog::loading::schema::fiche::Platform::Unavailable(_)
        ) {
            return Err(invalid("source", "platform_unavailable"));
        }
        // 3. Le nom, puis les réponses.
        validate_name(&spec.name).map_err(DeployError::Invalid)?;
        if answers.len() > hearth_proto::api::deploy::MAX_FICHE_ANSWERS
            || agreements.len() > hearth_proto::api::deploy::MAX_FICHE_AGREEMENTS
        {
            return Err(invalid("source", "too_many"));
        }
        let mut answer_map: BTreeMap<String, String> = BTreeMap::new();
        for answer in answers {
            if answer_map
                .insert(answer.id.clone(), answer.value.clone())
                .is_some()
            {
                return Err(invalid("source.answers", "duplicate"));
            }
        }
        let agreed: BTreeSet<String> = agreements.iter().cloned().collect();
        let mut host_ports: BTreeMap<String, u16> = BTreeMap::new();
        for port in ports {
            if host_ports.insert(port.id.clone(), port.host_port).is_some() {
                return Err(invalid("source.ports", "duplicate"));
            }
        }
        let optional: BTreeSet<String> = optional_ports.iter().cloned().collect();

        // 5. Les secrets que l'agent tire : alphabet et longueur de la fiche, octets du système.
        let mut generated: BTreeMap<String, Secret> = BTreeMap::new();
        for secret in &fiche.file.secret {
            let drawn = draw_secret(secret, |buf| self.random.fill(buf));
            match drawn {
                Ok(Some(value)) => {
                    generated.insert(secret.id.clone(), value);
                }
                _ => {
                    return Err(DeployError::Internal(
                        "le hasard du système est indisponible".into(),
                    ));
                }
            }
        }

        // 3 et 4. Traduction : réponses validées, contrats exigés.
        let draft = translate(
            fiche,
            &Request {
                variant: variant.as_deref(),
                answers: &answer_map,
                agreements: &agreed,
                host_ports: &host_ports,
                optional_ports: &optional,
                generated: &generated,
                world_imported: false,
            },
        )
        .map_err(|error| match error {
            TranslateError::AgreementRequired(ids) => DeployError::AgreementRequired(ids),
            TranslateError::Setting(refusal) => DeployError::SettingRefused {
                // L'identifiant n'est rendu que s'il est celui d'une question de la fiche : un identifiant inconnu
                // vient du client et ne revient pas.
                id: fiche.setting(&refusal.id).map(|s| s.id.clone()),
                reason: reason_code(&refusal.reason),
                character: refusal.character,
            },
            TranslateError::UnknownVariant(_) => invalid("source.variant", "unknown"),
            TranslateError::PlatformUnavailable => invalid("source", "platform_unavailable"),
            TranslateError::PortRefused { .. } => invalid("source.ports", "refused"),
            other => DeployError::Internal(format!("fiche non traduite : {other:?}")),
        })?;

        // 7. Les limites sont obligatoires comme pour une image à la main.
        let limits = check_limits(spec.resources, machine).map_err(|refusal| match refusal {
            Refusal::Invalid(found) => DeployError::Invalid(found),
            Refusal::LimitsRequired => DeployError::LimitsRequired,
            Refusal::SettingNotAllowed | Refusal::HostFolder { .. } => {
                DeployError::SettingNotAllowed
            }
        })?;

        // La machine et l'espace : un minimum MESURÉ est opposable, pas un chiffre de l'éditeur (BR-FICHE-021, 023).
        let fit_machine = machine.unwrap_or(Machine {
            cpus: u32::MAX,
            memory_bytes: u64::MAX,
        });
        let free = free_disk.unwrap_or(u64::MAX);
        match fit(fiche, fit_machine, free) {
            Fit::Ok => {}
            Fit::HostTooSmall { lack, needed, has } => {
                return Err(DeployError::HostTooSmall {
                    lack: match lack {
                        Lack::Cpu => "cpu",
                        Lack::Memory => "memory",
                    },
                    needed,
                    has,
                });
            }
            Fit::NoSpace {
                needed,
                free,
                missing,
            } => {
                return Err(DeployError::NotEnoughSpace {
                    needed,
                    free,
                    missing,
                });
            }
        }
        if !spec.accept.low_disk
            && fit_warnings(fiche, fit_machine, free)
                .iter()
                .any(|warning| matches!(warning, FitWarning::PublisherSpace { .. }))
        {
            return Err(DeployError::LowDisk { free_bytes: free });
        }

        let account = actor
            .account
            .as_ref()
            .map_or_else(|| "agent".to_owned(), ToString::to_string);
        let built = body_and_secrets_from(
            &InstallInputs {
                fiche,
                answers: &answer_map,
                generated: &generated,
                accepted_by: &account,
                accepted_at: self.clock.now().unix_timestamp(),
                accept: spec.accept,
            },
            &draft,
            &spec.name,
            Limits {
                cpu_millis: limits.cpu_millis,
                memory_bytes: limits.memory_bytes,
            },
        )
        .map_err(|error| {
            use crate::domain::services::from_fiche::FromFicheError;
            match error {
                FromFicheError::Name(found) => DeployError::Invalid(found),
                other => DeployError::Internal(format!("fiche non jugée : {other}")),
            }
        })?;

        let audit = InstallAudit {
            fiche: fiche.id().to_owned(),
            version: fiche.file.version,
            image: image_reference(&built.valid.body.image),
            answers: u32::try_from(answers.len()).unwrap_or(u32::MAX),
            cpu_millis: limits.cpu_millis,
            memory_bytes: limits.memory_bytes,
            agreements: fiche
                .file
                .agreement
                .iter()
                .filter(|a| agreed.contains(&a.id))
                .map(|a| a.title.clone())
                .collect(),
        };
        Ok(Prepared {
            valid: built.valid,
            files: built.files,
            audit,
        })
    }
}
