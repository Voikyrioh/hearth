//! Modifier un service créé par Hearth : le recréer depuis sa description, revenir seul à l'état d'avant en cas
//! d'échec, recréer un service dont le conteneur a disparu (HRT-87, ADR-0044, ADR-0045, BR-EDIT-*).
//!
//! **Ce que fait [`ServiceUpdates::update`], de façon SYNCHRONE** (rien n'est attendu du moteur au-delà d'une
//! lecture) : le moteur est prêt (`BR-EDIT-029`), le service existe et n'est pas adopté (`BR-EDIT-002`), aucune
//! autre action ne le tient (`BR-EDIT-025`), son état permet la modification (`BR-EDIT-003`), la révision lue par
//! le formulaire est la courante et rien n'a changé hors de Hearth (`BR-EDIT-024`), la demande passe le validateur
//! du déploiement (`BR-EDIT-028`), les ports nouveaux sont libres (`BR-EDIT-008`). Puis l'opération `update` est
//! acceptée (verrou exclusif du service) et l'exécution part en tâche de fond. Réponse : `202 { operation_id }`,
//! ou `200 { applied: false }` quand rien ne change (`BR-EDIT-013`).
//!
//! **L'exécution** suit le plan figé (`domain::ops::update`) :
//! 1. `check`, `pull` : l'image est récupérée SERVICE EN MARCHE ; un échec ne touche à rien (`BR-EDIT-011`,
//!    `BR-EDIT-021`) ; annulable ;
//! 2. `stop_old` (si le service tournait : arrêt voulu écrit avant l'ordre, délai de grâce du service), puis
//!    `rename_old` : l'ancien conteneur est mis de côté sous un nom provisoire, avec ses dossiers et son état ;
//! 3. `create_new` : la révision candidate est écrite (secrets scellés), puis le conteneur est créé DEPUIS elle,
//!    relue en base, par `plan_from_spec` ; il est consigné avant tout ;
//! 4. `start_new` puis `watch` (si le service tournait) : le nouveau est jugé démarré quand il reste en marche
//!    10 secondes ; un arrêt dans la fenêtre, code zéro compris, est un échec (`BR-EDIT-019`) ;
//! 5. `remove_old` : l'ancien conteneur n'est supprimé qu'APRÈS le succès, sans volume ni force ; les révisions
//!    antérieures de la description partent avec lui.
//!
//! **Si le nouveau échoue** (création, démarrage, arrêt dans la fenêtre) : ses 20 dernières lignes sont gardées
//! 24 heures en mémoire, jamais sur un disque ni au journal ; le nouveau est retiré, la révision candidate
//! abandonnée, l'ancien remis sous son nom et relancé s'il tournait, sans rien demander (`BR-EDIT-020`). Si le
//! retour échoue aussi, l'ancien n'est pas supprimé, aucun service ne repart seul, l'opération dit
//! `interrupted` avec la cause `step_failed` (`BR-EDIT-022`).
//!
//! **Rien n'est repris après un redémarrage de l'agent** (`BR-EDIT-031`, `recover`) : l'état cohérent est dit, un
//! état ambigu attend une décision. « Revenir à l'état d'avant » et « Garder la nouvelle configuration » sont
//! exécutés ici ([`ServiceUpdates::decided`]), par le même retour que l'échec, qui se lit dans l'état du moteur et
//! de la base, jamais dans la mémoire d'une exécution disparue.
//!
//! **Jamais** : une valeur ou un nom de variable, la commande, un chemin ou une ligne de journal dans une trace,
//! une erreur ou le journal d'activité (`BR-EDIT-033`, `BR-EDIT-034`) ; un volume supprimé ; un service démarré
//! que personne n'a demandé de démarrer.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use futures_util::StreamExt;
use hearth_proto::api::deploy::{DEPLOY_TIMEOUT_S, DeployAccept, DeployPort};
use hearth_proto::api::service_spec::DriftView;
use hearth_proto::api::service_update::{
    AnswerView, EnvNameView, EnvOrigin, OperationTail, ServiceSpecView, SpecChange, SpecFicheView,
    SpecImageView, TAIL_LINES, UpdateAccepted,
};
use thiserror::Error;
use tokio::time::Instant;

use super::deployments::{DeployError, Deployments};
use super::ports::{
    Clock, ContainerEngine, EngineError, LogQuery, Ownership, Perms, Progress, SafeFs,
    ServiceContainers, ServiceIntents, StopIntent,
};
use super::service_lifecycle::reason_of;
use super::service_op_access::DecisionEffects;
use super::service_ops::{AcceptRequest, AuditDraft, OpError, OpSnapshot, ServiceOps};
use super::service_specs::ServiceSpecs;
use super::services::{Driver, LifecycleTarget, ServicesRegistry};
use crate::domain::audit::{Actor, ClientName, Outcome, Reason, Target, UpdateNote};
use crate::domain::files::RelPath;
use crate::domain::ops::update::{Shape, Watched, op_plan, parked_name, settings_plan, step};
use crate::domain::ops::{
    Cause, Choice, Holder, OpKind, OpStatus, Refusal as LockRefusal, UndoInfo, stop_was_forced,
    wait_budget_with,
};
use crate::domain::services::drift::{Drift, compare};
use crate::domain::services::plan::{PlanContext, image_reference, plan_from_spec};
use crate::domain::services::spec::{SecretsAccess, SpecBody, SpecMountKind, SpecSecrets};
use crate::domain::services::update::{
    Applied, Candidate, GameChange, Summary, UpdateRefusal, apply, apply_game_env, clone_secrets,
    deploy_mounts, deploy_ports, image_variables, observed_container,
};
use crate::domain::services::{
    EngineState, KeyKind, RunState, ServiceKey, ServiceStateKind, TransitionKind,
};

/// Combien de temps les dernières lignes d'un nouveau service qui n'a pas démarré restent lisibles (`BR-EDIT-020`).
const TAIL_KEEP: Duration = Duration::from_secs(24 * 3600);
/// Combien d'opérations ont leurs lignes gardées au plus.
const MAX_TAILS: usize = 64;
/// Combien de temps la lecture des dernières lignes d'un conteneur peut durer.
const TAIL_READ: Duration = Duration::from_secs(5);

/// Les durées d'une modification. Les tests les raccourcissent.
#[derive(Debug, Clone, Copy)]
pub struct UpdateTimings {
    /// Au-delà, la récupération de l'image est arrêtée (`BR-EDIT-032`, `BR-DEPLOY-034`).
    pub pull_deadline: Duration,
    /// Combien de temps le nouveau service est surveillé (`BR-EDIT-019`).
    pub watch: Duration,
    /// L'intervalle entre deux lectures pendant la surveillance.
    pub poll: Duration,
    /// Marge et plancher de l'attente d'un arrêt (`BR-EDIT-032`, `BR-LIFE-021`).
    pub stop_margin: Duration,
    pub stop_floor: Duration,
    /// Combien d'attente pour un geste du moteur qui n'est pas un arrêt (renommer, créer, démarrer, supprimer).
    pub step_wait: Duration,
}

impl UpdateTimings {
    pub const fn production() -> Self {
        Self {
            pull_deadline: Duration::from_secs(DEPLOY_TIMEOUT_S),
            watch: Duration::from_secs(10),
            poll: Duration::from_millis(500),
            stop_margin: crate::domain::ops::WAIT_MARGIN,
            stop_floor: crate::domain::ops::WAIT_FLOOR,
            step_wait: Duration::from_secs(60),
        }
    }

    /// Les durées d'une modification dérivées de celles du déploiement (la récupération, la surveillance) et de la
    /// suppression (l'attente d'un arrêt, d'un geste du moteur) : les tests qui raccourcissent celles-là
    /// raccourcissent aussi celles-ci, sans réglage de plus.
    pub const fn derived(
        deploy: super::deployments::DeployTimings,
        delete: super::delete::DeleteTimings,
    ) -> Self {
        Self {
            pull_deadline: deploy.deadline,
            watch: deploy.watch,
            poll: deploy.poll,
            stop_margin: delete.stop_margin,
            stop_floor: delete.stop_floor,
            step_wait: delete.remove_wait,
        }
    }
}

/// Ce dont le cas d'usage a besoin de la machine.
pub struct UpdateContext {
    pub install_id: String,
    /// `<données de l'agent>/services` : sous lui, un sous-dossier par service.
    pub services_root: String,
    pub timings: UpdateTimings,
}

/// Pourquoi une demande est refusée tout de suite, avant que rien ne soit ordonné.
#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("le moteur n'est pas prêt")]
    EngineUnavailable(EngineState),
    /// Aucun service de cette clé que Hearth connaisse (`BR-EDIT-026`).
    #[error("service inconnu")]
    NotFound,
    /// Hearth ne modifie jamais un service créé hors de Hearth (`BR-EDIT-002`).
    #[error("service adopté")]
    Adopted,
    /// Une autre action tient le service (`BR-EDIT-025`) : qui, et laquelle.
    #[error("une action est déjà en cours")]
    Busy { by: String, kind: OpKind },
    /// L'état ne permet pas la modification (`BR-EDIT-003`).
    #[error("état incompatible")]
    NotUpdatable(ServiceStateKind),
    /// La description a changé depuis l'ouverture du formulaire, ou le service a été changé hors de Hearth
    /// (`BR-EDIT-024`) : la révision courante. Jamais le nom d'un compte dans ce cas.
    #[error("la configuration a changé")]
    Conflict { current_rev: u32, outside: bool },
    /// La demande est refusée par le validateur du déploiement ou par ce qu'un service de fiche accepte
    /// (`BR-EDIT-028`, `BR-EDIT-037`).
    #[error("demande refusée")]
    Refused(UpdateRefusal),
    /// Un port nouveau est pris (`BR-EDIT-008`) : le même refus que le déploiement.
    #[error("port refusé")]
    Ports(Box<DeployError>),
    /// Les secrets du service ne s'ouvrent pas : la description n'est pas lisible, rien n'est touché.
    #[error("secrets illisibles")]
    SecretsUnreadable,
    #[error("erreur interne : {0}")]
    Internal(String),
}

/// Un refus, avec le nom du service quand il est publié (il va au journal d'activité).
#[derive(Debug)]
pub struct UpdateFailed {
    pub error: UpdateError,
    pub name: Option<String>,
}

impl UpdateFailed {
    fn plain(error: UpdateError) -> Self {
        Self { error, name: None }
    }

    fn named(error: UpdateError, name: &str) -> Self {
        Self {
            error,
            name: Some(name.to_owned()),
        }
    }

    /// Une clé que Hearth ne sait pas lire est un service inconnu.
    pub fn not_found() -> Self {
        Self::plain(UpdateError::NotFound)
    }
}

/// Ce que l'exécution a décidé de son issue.
#[derive(Debug, Clone, Copy)]
struct Finish {
    status: OpStatus,
    cause: Option<Cause>,
    /// Le nouveau a échoué et l'ancien a été remis.
    rolled_back: bool,
}

impl Finish {
    fn done() -> Self {
        Self {
            status: OpStatus::Succeeded,
            cause: None,
            rolled_back: false,
        }
    }

    /// Échec avant tout geste sur le service : « Le service n'a pas été modifié » (`BR-EDIT-021`).
    fn failed(cause: Cause) -> Self {
        Self {
            status: OpStatus::Failed,
            cause: Some(cause),
            rolled_back: false,
        }
    }

    fn cancelled() -> Self {
        Self {
            status: OpStatus::Cancelled,
            cause: Some(Cause::CancelledByAccount),
            rolled_back: false,
        }
    }

    /// Le moteur n'a pas répondu à un ordre : le résultat est inconnu, le compte relit l'état réel.
    fn unknown(cause: Cause) -> Self {
        Self {
            status: OpStatus::Interrupted,
            cause: Some(cause),
            rolled_back: false,
        }
    }
}

/// La cause d'une erreur du moteur, et si elle laisse le résultat inconnu. Jamais un texte du moteur.
fn cause_of(error: EngineError) -> (Cause, bool) {
    match error {
        EngineError::ImageNotFound => (Cause::ImageNotFound, false),
        EngineError::NotFound => (Cause::ServiceNotFound, false),
        EngineError::TagNotFound => (Cause::TagNotFound, false),
        EngineError::ImagePrivateOrMissing => (Cause::ImagePrivateOrMissing, false),
        EngineError::RegistryUnreachable => (Cause::RegistryUnreachable, false),
        EngineError::RegistryRateLimited => (Cause::RegistryRateLimited, false),
        EngineError::PlatformMismatch => (Cause::PlatformMismatch, false),
        EngineError::DiskFull => (Cause::DiskFull, false),
        EngineError::NameTaken => (Cause::NameTaken, false),
        EngineError::PortInUse => (Cause::PortInUse, false),
        EngineError::Unreachable => (Cause::EngineUnavailable, true),
        EngineError::Timeout => (Cause::Timeout, true),
        EngineError::Denied => (Cause::EngineUnavailable, false),
        EngineError::Refused | EngineError::Unsupported => (Cause::EngineRefused, false),
    }
}

/// Tout ce qu'une exécution emporte avec elle. Aucun secret : la description candidate est passée à part.
struct Job {
    id: String,
    key: ServiceKey,
    name: String,
    actor: Actor,
    account: String,
    shape: Shape,
    /// La révision courante au moment de l'acceptation.
    old_rev: u32,
    /// Le conteneur d'avant (il existe si `shape.had_container`).
    old_container: Option<String>,
    grace_s: u32,
    /// Une nouvelle révision de la description est écrite (sinon : « Recréer le service », rien ne change).
    save_revision: bool,
    /// Ce que le journal d'activité dit (catégories et nombres).
    summary: Summary,
    /// La nouvelle version de l'image, quand elle change (le journal d'activité la dit, `BR-EDIT-033`).
    new_tag: Option<String>,
    host_ports: Vec<u16>,
    /// Les sous-dossiers gérés à créer : ceux que la modification ajoute (tous, pour un conteneur disparu).
    new_folders: Vec<String>,
    /// Seule l'adresse des joueurs change : la description change, le conteneur n'est pas touché.
    settings_only: bool,
    /// Un geste destructif a commencé : un échec ne dit plus « le service n'a pas été modifié ».
    touched: AtomicBool,
}

/// Les dernières lignes des nouveaux services qui n'ont pas démarré : en mémoire seulement (`BR-EDIT-020`).
#[derive(Default)]
struct Tails {
    kept: Mutex<HashMap<String, (Instant, Vec<String>)>>,
}

impl Tails {
    fn keep(&self, op: &str, lines: Vec<String>) {
        let now = Instant::now();
        let mut kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        kept.retain(|_, (at, _)| now.saturating_duration_since(*at) < TAIL_KEEP);
        if kept.len() >= MAX_TAILS
            && let Some(oldest) = kept
                .iter()
                .min_by_key(|(_, (at, _))| *at)
                .map(|(id, _)| id.clone())
        {
            kept.remove(&oldest);
        }
        kept.insert(op.to_owned(), (now, lines));
    }

    fn read(&self, op: &str) -> Option<Vec<String>> {
        let now = Instant::now();
        let mut kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        kept.retain(|_, (at, _)| now.saturating_duration_since(*at) < TAIL_KEEP);
        kept.get(op).map(|(_, lines)| lines.clone())
    }
}

pub struct ServiceUpdates {
    registry: Arc<ServicesRegistry>,
    ops: Arc<ServiceOps>,
    specs: Arc<ServiceSpecs>,
    engine: Arc<dyn ContainerEngine>,
    containers: Arc<dyn ServiceContainers>,
    intents: Arc<dyn ServiceIntents>,
    safe_fs: Arc<dyn SafeFs>,
    deployments: Arc<Deployments>,
    clock: Arc<dyn Clock>,
    context: UpdateContext,
    tails: Tails,
}

/// Ce qu'une demande a établi du service visé.
struct Subject {
    name: String,
    rev: u32,
    body: SpecBody,
    secrets: SpecSecrets,
    target: Option<LifecycleTarget>,
    observation: Option<crate::domain::services::Observation>,
    consigned: Vec<String>,
}

/// Ce qu'une modification demande : le formulaire d'un service, ou les réglages d'un serveur de jeu.
enum Wanted {
    Spec(SpecChange),
    Game(GameChange),
}

impl ServiceUpdates {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        registry: Arc<ServicesRegistry>,
        ops: Arc<ServiceOps>,
        specs: Arc<ServiceSpecs>,
        engine: Arc<dyn ContainerEngine>,
        containers: Arc<dyn ServiceContainers>,
        intents: Arc<dyn ServiceIntents>,
        safe_fs: Arc<dyn SafeFs>,
        deployments: Arc<Deployments>,
        clock: Arc<dyn Clock>,
        context: UpdateContext,
    ) -> Arc<Self> {
        Arc::new(Self {
            registry,
            ops,
            specs,
            engine,
            containers,
            intents,
            safe_fs,
            deployments,
            clock,
            context,
            tails: Tails::default(),
        })
    }

    // -----------------------------------------------------------------------------------------
    // Ce que Hearth sait d'un service
    // -----------------------------------------------------------------------------------------

    fn managed_prefix(&self, key: &ServiceKey) -> String {
        format!(
            "{}/{}/",
            self.context.services_root.trim_end_matches('/'),
            key.as_str()
        )
    }

    /// Le service visé : sa description courante (secrets descellés), son conteneur et ce que le moteur en dit.
    /// Le moteur n'a pas besoin d'être prêt : sans lui, la description reste lisible (`BR-EDIT-029`).
    async fn subject(&self, key: &ServiceKey) -> Result<Subject, UpdateFailed> {
        let target = self.registry.lifecycle_target(key);
        if key.kind() != KeyKind::Hearth {
            // Un service publié qui n'est pas créé par Hearth est un service adopté : jamais modifié.
            return Err(match target {
                Some(target) => UpdateFailed::named(UpdateError::Adopted, &target.name),
                None => UpdateFailed::plain(UpdateError::NotFound),
            });
        }
        let read = self
            .specs
            .read(key, None)
            .await
            .map_err(|error| UpdateFailed::plain(UpdateError::Internal(error.to_string())))?;
        let Some((revision, access)) = read else {
            return Err(UpdateFailed::plain(UpdateError::NotFound));
        };
        let name = revision.body.name.clone();
        let secrets = match access {
            SecretsAccess::Opened(secrets) => secrets,
            SecretsAccess::Unreadable(_) | SecretsAccess::Tampered => {
                return Err(UpdateFailed::named(UpdateError::SecretsUnreadable, &name));
            }
        };
        let consigned = self.containers.of(key).await.map_err(|error| {
            UpdateFailed::named(UpdateError::Internal(error.to_string()), &name)
        })?;
        let observation = match &target {
            Some(LifecycleTarget {
                driver: Driver::Engine { container_id },
                ..
            }) if self.registry.engine_state() == EngineState::Ready => {
                self.engine.inspect(container_id).await.ok().flatten()
            }
            _ => None,
        };
        Ok(Subject {
            name,
            rev: revision.rev,
            body: revision.body,
            secrets,
            target,
            observation,
            consigned,
        })
    }

    fn drift_of(&self, key: &ServiceKey, subject: &Subject) -> Drift {
        if subject.target.is_none() {
            return Drift::ContainerMissing;
        }
        match &subject.observation {
            None => Drift::None,
            Some(observation) => {
                let consigned = subject.consigned.contains(&observation.id);
                compare(
                    subject.rev,
                    &subject.body,
                    Some(&observed_container(
                        observation,
                        subject.rev,
                        consigned,
                        &self.managed_prefix(key),
                    )),
                )
            }
        }
    }

    /// `GET /services/{key}/spec` : ce que Hearth sait du service, jamais une valeur (`BR-EDIT-005`,
    /// `BR-EDIT-006`).
    pub async fn view(&self, key: &ServiceKey) -> Result<ServiceSpecView, UpdateFailed> {
        let subject = self.subject(key).await?;
        let drift = self.drift_of(key, &subject).view();
        let mut env: Vec<EnvNameView> = subject
            .secrets
            .env
            .iter()
            .map(|entry| EnvNameView {
                name: hearth_proto::api::services::Concealed::new(entry.name.expose()),
                set_by: EnvOrigin::Hearth,
            })
            .collect();
        if let Some(observation) = &subject.observation {
            env.extend(
                image_variables(observation, &subject.secrets)
                    .into_iter()
                    .map(|name| EnvNameView {
                        name: hearth_proto::api::services::Concealed::new(name),
                        set_by: EnvOrigin::Image,
                    }),
            );
        }
        let body = &subject.body;
        Ok(ServiceSpecView {
            rev: subject.rev,
            name: body.name.clone(),
            image: SpecImageView {
                repository: body.image.repository.clone(),
                tag: body.image.tag.clone(),
                digest: body.image.digest.clone(),
            },
            env,
            ports: deploy_ports(body),
            mounts: deploy_mounts(body),
            command_set: subject.secrets.command.is_some(),
            stop_grace_s: body.stop_grace_s,
            stdin_open: body.stdin_open,
            resources: hearth_proto::api::deploy::DeployResources {
                cpu_millis: body.cpu_millis,
                memory_bytes: body.memory_bytes,
            },
            public_address: body.settings.public_address.clone(),
            fiche: body.fiche.as_ref().map(|fiche| SpecFicheView {
                id: fiche.id.clone(),
                version: fiche.version,
                digest: fiche.digest.clone(),
                answers: fiche
                    .answers
                    .iter()
                    .map(|answer| AnswerView {
                        id: answer.id.clone(),
                        value: answer.value.clone(),
                        secret: answer.secret,
                        frozen: answer.frozen,
                    })
                    .collect(),
            }),
            drift,
        })
    }

    /// `GET /operations/services/{id}/tail` : les dernières lignes du nouveau service qui n'a pas démarré
    /// (`BR-EDIT-020`), gardées 24 heures en mémoire. `None` : aucune.
    pub fn tail(&self, operation_id: &str) -> Option<OperationTail> {
        self.tails.read(operation_id).map(|lines| OperationTail {
            lines,
            reason: None,
            sentence: None,
        })
    }

    // -----------------------------------------------------------------------------------------
    // La demande
    // -----------------------------------------------------------------------------------------

    fn busy_error(&self, key: &ServiceKey) -> Option<UpdateError> {
        self.ops
            .busy(key)
            .map(|(by, kind)| UpdateError::Busy { by, kind })
    }

    /// Accepte (ou refuse tout de suite) une modification. Le droit de gérer les services et la confirmation de
    /// l'acte ont déjà été jugés.
    pub async fn update(
        self: &Arc<Self>,
        key: &ServiceKey,
        change: SpecChange,
        actor: Actor,
        op_key: &str,
    ) -> Result<UpdateAccepted, UpdateFailed> {
        let (base_rev, seen_drift) = (change.base_rev, change.seen_drift);
        self.update_wanted(
            key,
            base_rev,
            seen_drift,
            Wanted::Spec(change),
            actor,
            op_key,
        )
        .await
    }

    /// Ce que `update` jugerait AVANT toute écriture, sans rien accepter : le service est modifiable (moteur prêt,
    /// aucune action en cours, état permis, pas changé hors de Hearth). Sert à refuser tout un enregistrement de
    /// réglages avant d'écrire le moindre fichier (HRT-266). Le serveur refait chaque contrôle à l'acceptation.
    pub async fn precheck(&self, key: &ServiceKey) -> Result<(), UpdateFailed> {
        self.ops.wait_closed(key).await;
        let engine_state = self.registry.engine_state();
        let subject = self.subject(key).await?;
        let named = |error| UpdateFailed::named(error, &subject.name);
        if engine_state != EngineState::Ready {
            return Err(named(UpdateError::EngineUnavailable(engine_state)));
        }
        if let Some(busy) = self.busy_error(key) {
            return Err(named(busy));
        }
        if let Some(target) = &subject.target
            && !updatable(target.state)
        {
            return Err(named(UpdateError::NotUpdatable(target.state)));
        }
        if matches!(self.drift_of(key, &subject), Drift::ModifiedOutside(_)) {
            return Err(named(UpdateError::Conflict {
                current_rev: subject.rev,
                outside: true,
            }));
        }
        Ok(())
    }

    /// Change les réglages d'un serveur de jeu qui passent par une variable ou la commande (HRT-266, BR-JEU-035) :
    /// la description est révisée et le conteneur recréé par le plan de HRT-87. `base_rev` est la révision que
    /// l'appelant a lue AVANT d'écrire quoi que ce soit : une description changée entre-temps est un `SPEC_CONFLICT`.
    pub async fn update_game(
        self: &Arc<Self>,
        key: &ServiceKey,
        base_rev: u32,
        change: GameChange,
        actor: Actor,
        op_key: &str,
    ) -> Result<UpdateAccepted, UpdateFailed> {
        self.update_wanted(key, base_rev, None, Wanted::Game(change), actor, op_key)
            .await
    }

    async fn update_wanted(
        self: &Arc<Self>,
        key: &ServiceKey,
        base_rev: u32,
        seen_drift: Option<DriftView>,
        wanted: Wanted,
        actor: Actor,
        op_key: &str,
    ) -> Result<UpdateAccepted, UpdateFailed> {
        // Une clôture en cours sur ce service se termine avant que l'on juge « occupé ».
        self.ops.wait_closed(key).await;
        let engine_state = self.registry.engine_state();
        let subject = match self.subject(key).await {
            Ok(subject) => subject,
            // Une suppression en cours a déjà oublié la description mais tient encore son verrou : « une action est
            // en cours », pas « ce service n'existe plus ».
            Err(failed) if matches!(failed.error, UpdateError::NotFound) => {
                return Err(match self.busy_error(key) {
                    Some(busy) => UpdateFailed::plain(busy),
                    None => failed,
                });
            }
            Err(failed) => return Err(failed),
        };
        let named = |error| UpdateFailed::named(error, &subject.name);
        if engine_state != EngineState::Ready {
            return Err(named(UpdateError::EngineUnavailable(engine_state)));
        }
        // Dans l'ordre de la spec : l'action en cours avant l'état (BR-EDIT-025, BR-EDIT-003).
        if let Some(busy) = self.busy_error(key) {
            return Err(named(busy));
        }
        if let Some(target) = &subject.target
            && !updatable(target.state)
        {
            return Err(named(UpdateError::NotUpdatable(target.state)));
        }
        // La révision lue par le formulaire est la courante (BR-EDIT-024), avant tout arrêt.
        if base_rev != subject.rev {
            return Err(named(UpdateError::Conflict {
                current_rev: subject.rev,
                outside: false,
            }));
        }
        let drift = self.drift_of(key, &subject);
        // Le service a été changé hors de Hearth depuis l'ouverture du formulaire : refusé, sans nommer d'auteur.
        if matches!(drift, Drift::ModifiedOutside(_))
            && seen_drift != Some(DriftView::ModifiedOutside)
        {
            return Err(named(UpdateError::Conflict {
                current_rev: subject.rev,
                outside: true,
            }));
        }

        let applied = match &wanted {
            Wanted::Spec(change) => apply(&subject.body, &subject.secrets, change),
            Wanted::Game(change) => apply_game_env(&subject.body, &subject.secrets, change),
        }
        .map_err(|refusal| named(UpdateError::Refused(refusal)))?;
        let recreate_anyway = !matches!(drift, Drift::None);
        let (candidate, save_revision) = match applied {
            Applied::Changed(candidate) => (candidate, true),
            // « Recréer le service » (BR-EDIT-036) : une modification sans changement, permise seulement quand le
            // conteneur a disparu ou diffère de la description (BR-EDIT-013 sinon).
            Applied::Nothing if recreate_anyway => (
                Candidate {
                    ports: deploy_ports(&subject.body),
                    body: subject.body.clone(),
                    secrets: clone_secrets(&subject.secrets),
                    summary: Summary::default(),
                    accept: DeployAccept::default(),
                },
                false,
            ),
            Applied::Nothing => {
                return Ok(UpdateAccepted {
                    applied: false,
                    operation_id: None,
                });
            }
        };

        // Les ports que le service ne tient pas déjà : jugés comme au déploiement (BR-EDIT-008).
        let held: Vec<(u16, hearth_proto::api::deploy::DeployProtocol)> =
            deploy_ports(&subject.body)
                .into_iter()
                .map(|port| (port.host_port, port.protocol))
                .collect();
        let fresh: Vec<DeployPort> = candidate
            .ports
            .iter()
            .filter(|port| !held.contains(&(port.host_port, port.protocol)))
            .copied()
            .collect();
        self.deployments
            .check_ports(fresh, candidate.accept.reserved_ports)
            .await
            .map_err(|error| named(UpdateError::Ports(Box::new(error))))?;

        let had_container = subject.target.is_some();
        let was_running = subject.target.as_ref().is_some_and(|target| {
            matches!(
                target.state,
                ServiceStateKind::Running | ServiceStateKind::RestartLoop
            )
        });
        let settings_only = had_container
            && drift == Drift::None
            && !candidate.summary.needs_recreation()
            && save_revision;
        let shape = Shape {
            had_container,
            was_running,
        };
        let plan = if settings_only {
            settings_plan()
        } else {
            op_plan(shape)
        }
        .map_err(|error| named(UpdateError::Internal(error.to_string())))?;
        let old_managed: Vec<&str> = subject
            .body
            .mounts
            .iter()
            .filter(|mount| mount.kind == SpecMountKind::Managed)
            .map(|mount| mount.source.as_str())
            .collect();
        let new_folders: Vec<String> = candidate
            .body
            .mounts
            .iter()
            .filter(|mount| mount.kind == SpecMountKind::Managed)
            // Un conteneur disparu : tous les dossiers sont revérifiés (un dossier présent reste tel quel).
            .filter(|mount| !had_container || !old_managed.contains(&mount.source.as_str()))
            .map(|mount| mount.source.clone())
            .collect();

        let account = account_of(&actor);
        let id = self
            .ops
            .accept_after_wait(AcceptRequest {
                op_key: op_key.to_owned(),
                service: key.clone(),
                service_name: subject.name.clone(),
                kind: OpKind::Update,
                account: account.clone(),
                plan,
                forced: false,
                reserve_name: None,
                heavy_io: false,
            })
            .await
            .map_err(|error| match error {
                // Deux demandes simultanées : la seconde n'a pas vu la première dans l'index.
                OpError::Locked(LockRefusal::ServiceBusy { holder }) => named(UpdateError::Busy {
                    by: self.holder_account(key),
                    kind: match holder {
                        Holder::Operation(kind) => kind,
                        Holder::FileWrites => OpKind::Update,
                    },
                }),
                OpError::ServiceBusy => named(UpdateError::Busy {
                    by: self.holder_account(key),
                    kind: OpKind::Update,
                }),
                other => named(UpdateError::Internal(other.to_string())),
            })?;
        if let Err(error) = self.ops.start(&id).await {
            journal(
                self.ops
                    .finish(&id, OpStatus::Failed, Some(Cause::StepFailed))
                    .await,
                "finish",
            );
            return Err(named(UpdateError::Internal(error.to_string())));
        }
        let job = Job {
            id: id.clone(),
            key: key.clone(),
            name: subject.name.clone(),
            actor: actor.clone(),
            account,
            shape,
            old_rev: subject.rev,
            old_container: subject
                .target
                .as_ref()
                .and_then(|target| match &target.driver {
                    Driver::Engine { container_id } => Some(container_id.clone()),
                    Driver::Supervisor { .. } => None,
                }),
            grace_s: subject.target.as_ref().map_or(10, |target| target.grace_s),
            save_revision,
            summary: candidate.summary,
            new_tag: candidate
                .summary
                .tag
                .then(|| candidate.body.image.tag.clone()),
            host_ports: candidate
                .body
                .ports
                .iter()
                .map(|port| port.host_port)
                .collect(),
            new_folders,
            settings_only,
            touched: AtomicBool::new(false),
        };
        let this = Arc::clone(self);
        let work = tokio::spawn(async move { this.execute(job, candidate).await });
        // Une panique de l'exécution ne laisse jamais une opération « en cours » : elle est close, le verrou rendu.
        let watcher = Arc::clone(self);
        let (watched, name) = (id.clone(), subject.name.clone());
        tokio::spawn(async move {
            if work.await.is_err() {
                tracing::error!(operation = %watched, "exécution d'une modification interrompue par une panique");
                watcher.close_after_panic(&watched, &name, actor).await;
            }
        });
        Ok(UpdateAccepted {
            applied: true,
            operation_id: Some(id),
        })
    }

    fn holder_account(&self, key: &ServiceKey) -> String {
        self.ops.busy(key).map(|(by, _)| by).unwrap_or_default()
    }

    // -----------------------------------------------------------------------------------------
    // L'exécution
    // -----------------------------------------------------------------------------------------

    async fn execute(&self, job: Job, candidate: Candidate) {
        let started = Instant::now();
        let timings = self.context.timings;
        let deadline = started
            + timings.pull_deadline
            + wait_budget_with(job.grace_s, timings.stop_margin, timings.stop_floor)
            + timings.watch
            + timings.step_wait * 4
            // Le retour en arrière a son propre budget (retirer le nouveau, rendre son nom à l'ancien, le relancer) :
            // le délai ne le coupe jamais à mi-chemin, ce qui laisserait une opération close sans décision possible.
            + timings.step_wait * 4;
        let finish =
            match tokio::time::timeout_at(deadline, self.run(&job, &candidate, deadline)).await {
                Ok(finish) => finish,
                // Au-delà : le moteur n'a pas répondu à temps, le résultat n'est pas connu (BR-EDIT-032). Si rien n'a
                // été touché, rien n'a changé.
                Err(_) => {
                    if job.touched.load(Ordering::SeqCst) {
                        Finish::unknown(Cause::Timeout)
                    } else {
                        Finish::failed(Cause::Timeout)
                    }
                }
            };
        drop(candidate);
        self.settle(&job, finish, started.elapsed()).await;
    }

    async fn is_cancelling(&self, id: &str) -> bool {
        matches!(
            self.ops.snapshot(id).await,
            Ok(Some(snapshot)) if snapshot.record.status == OpStatus::Cancelling
        )
    }

    /// Écrit l'`intent` d'une étape. `Err` : l'issue à rendre (annulée si une annulation est passée avant, sinon un
    /// échec).
    async fn begin(&self, job: &Job, name: &str) -> Result<(), Finish> {
        match self.ops.intent(&job.id, name, &UndoInfo::default()).await {
            Ok(_) => Ok(()),
            Err(OpError::Stale | OpError::OutOfOrder) if self.is_cancelling(&job.id).await => {
                Err(Finish::cancelled())
            }
            Err(error) => {
                tracing::error!(%error, step = name, "journal de la modification illisible : l'étape n'est pas lancée");
                Err(Finish::failed(Cause::StepFailed))
            }
        }
    }

    async fn done(&self, job: &Job, name: &str) {
        journal(
            self.ops.done(&job.id, name, &UndoInfo::default()).await,
            "done",
        );
    }

    async fn step_failed(&self, job: &Job, name: &str) {
        journal(self.ops.failed(&job.id, name).await, "failed");
    }

    async fn run(&self, job: &Job, candidate: &Candidate, deadline: Instant) -> Finish {
        // 1. Vérification : l'image est-elle déjà là ?
        if let Err(finish) = self.begin(job, step::CHECK).await {
            return finish;
        }
        if job.settings_only {
            return self.settings_only(job, candidate).await;
        }
        let reference = image_reference(&candidate.body.image);
        let present = match self.engine.image_present(&reference).await {
            Ok(present) => present,
            Err(error) => {
                self.step_failed(job, step::CHECK).await;
                return self.before_any_gesture(error);
            }
        };
        self.done(job, step::CHECK).await;
        if self.is_cancelling(&job.id).await {
            return Finish::cancelled();
        }

        // 2. Récupération, SERVICE EN MARCHE, annulable (BR-EDIT-011). Une image déjà là est utilisée telle quelle,
        // sauf si le compte a demandé de la récupérer à nouveau.
        if (!present || candidate.summary.repull)
            && let Err(finish) = self.pull(job, &reference, deadline).await
        {
            return finish;
        }

        // Les sous-dossiers gérés ajoutés (BR-EDIT-023) : créés avant d'arrêter quoi que ce soit.
        if let Err(finish) = self.prepare_folders(job).await {
            return finish;
        }
        if self.is_cancelling(&job.id).await {
            return Finish::cancelled();
        }

        // 3. Arrêt de l'ancien, puis mis de côté : le point de non-retour de l'annulation.
        let was_running = job.shape.was_running;
        if job.shape.had_container {
            if was_running {
                if let Err(finish) = self.begin(job, step::STOP_OLD).await {
                    return finish;
                }
                job.touched.store(true, Ordering::SeqCst);
                if let Err(finish) = self.stop_old(job).await {
                    return finish;
                }
                self.done(job, step::STOP_OLD).await;
            }
            if let Err(finish) = self.begin(job, step::RENAME_OLD).await {
                return finish;
            }
            job.touched.store(true, Ordering::SeqCst);
            let parked = parked_name(&job.name, job.old_rev);
            let old = job.old_container.clone().unwrap_or_default();
            let renamed = tokio::time::timeout(
                self.context.timings.step_wait,
                self.engine.rename(&old, &parked),
            )
            .await
            .unwrap_or(Err(EngineError::Timeout));
            if let Err(error) = renamed {
                self.step_failed(job, step::RENAME_OLD).await;
                return self.after_gesture(job, candidate, error).await;
            }
            self.done(job, step::RENAME_OLD).await;
            self.registry.reread(&old).await;
        }

        // 4. Création du nouveau, DEPUIS la description relue en base (BR-EDIT-018).
        if let Err(finish) = self.begin(job, step::CREATE_NEW).await {
            return finish;
        }
        job.touched.store(true, Ordering::SeqCst);
        let rev = if job.save_revision {
            match self
                .specs
                .save(
                    &job.key,
                    Some(job.old_rev),
                    candidate.body.clone(),
                    &candidate.secrets,
                    &job.account,
                )
                .await
            {
                Ok(rev) => rev,
                Err(error) => {
                    tracing::error!(%error, "révision candidate non écrite : retour à l'état d'avant");
                    self.step_failed(job, step::CREATE_NEW).await;
                    return self.roll_back(job, Cause::StepFailed, None).await;
                }
            }
        } else {
            job.old_rev
        };
        let plan = match self.plan_of(job, rev).await {
            Ok(plan) => plan,
            Err(()) => {
                self.step_failed(job, step::CREATE_NEW).await;
                return self.roll_back(job, Cause::StepFailed, None).await;
            }
        };
        let created =
            tokio::time::timeout(self.context.timings.step_wait, self.engine.create(&plan))
                .await
                .unwrap_or(Err(EngineError::Timeout));
        drop(plan);
        let new_id = match created {
            Ok(id) => id,
            Err(error) => {
                self.step_failed(job, step::CREATE_NEW).await;
                let (cause, unknown) = cause_of(error);
                if unknown {
                    // Le moteur ne répond plus : l'ordre est peut-être parti. Rien n'est retiré ni remis.
                    return Finish::unknown(cause);
                }
                return self.roll_back(job, cause, None).await;
            }
        };
        // Le conteneur est consigné AVANT tout : c'est la preuve qu'il est à Hearth, et ce qui permet de le retirer.
        if let Err(error) = self
            .containers
            .record(&job.key, &new_id, self.clock.now())
            .await
        {
            tracing::error!(%error, "conteneur créé non consigné : retiré");
            // Pas consigné : `restore` ne le verrait pas. Il garderait le nom du service et l'ancien ne pourrait plus
            // le reprendre.
            let lines = self.tail_of(&new_id).await;
            self.tails.keep(&job.id, lines);
            let removed =
                tokio::time::timeout(self.context.timings.step_wait, self.engine.remove(&new_id))
                    .await
                    .unwrap_or(Err(EngineError::Timeout));
            if removed.is_err() {
                tracing::error!(operation = %job.id, "conteneur créé non consigné : non retiré");
            }
            return self.roll_back(job, Cause::StepFailed, None).await;
        }
        self.registry.register_hearth_container(&job.key, &new_id);
        self.registry.reread(&new_id).await;
        self.done(job, step::CREATE_NEW).await;

        // 5. Démarrage et surveillance, seulement si le service tournait (BR-EDIT-015).
        if job.shape.had_container && was_running {
            if let Err(finish) = self.begin(job, step::START_NEW).await {
                return self.after_begin_refused(job, finish, Some(new_id)).await;
            }
            let from = self.clock.now();
            let started =
                tokio::time::timeout(self.context.timings.step_wait, self.engine.start(&new_id))
                    .await
                    .unwrap_or(Err(EngineError::Timeout));
            self.registry.exempt_starts(
                &job.key,
                from,
                self.clock.now()
                    + time::Duration::try_from(self.context.timings.watch).unwrap_or_default()
                    + time::Duration::seconds(5),
            );
            match started {
                Ok(()) => {
                    self.done(job, step::START_NEW).await;
                    self.registry.reread(&new_id).await;
                    self.forget_stop(&job.key).await;
                }
                Err(error) => {
                    self.step_failed(job, step::START_NEW).await;
                    let (cause, unknown) = cause_of(error);
                    if unknown {
                        return Finish::unknown(cause);
                    }
                    return self.roll_back(job, cause, Some(new_id)).await;
                }
            }
            if let Err(finish) = self.begin(job, step::WATCH).await {
                return self.after_begin_refused(job, finish, Some(new_id)).await;
            }
            let watched = self.watch(&new_id).await;
            self.done(job, step::WATCH).await;
            if watched == Watched::Fell {
                return self
                    .roll_back(job, Cause::StoppedRightAfterStart, Some(new_id))
                    .await;
            }
        }

        // 6. Le nouveau tient : l'ancien n'est supprimé qu'APRÈS (BR-EDIT-018), puis les révisions d'avant partent.
        if job.shape.had_container {
            if let Err(finish) = self.begin(job, step::REMOVE_OLD).await {
                // Une annulation ne peut plus passer ici ; un journal illisible laisse l'ancien de côté.
                tracing::warn!(operation = %job.id, "ancien conteneur laissé de côté : journal illisible");
                return if finish.status == OpStatus::Cancelled {
                    Finish::done()
                } else {
                    finish
                };
            }
            self.remove_old(job).await;
            self.done(job, step::REMOVE_OLD).await;
        }
        if job.save_revision
            && let Err(error) = self.specs.discard_before(&job.key, rev).await
        {
            tracing::warn!(%error, "révisions antérieures de la description non purgées");
        }
        Finish::done()
    }

    /// Un changement qui ne recrée rien : la description change, le conteneur n'est pas touché.
    async fn settings_only(&self, job: &Job, candidate: &Candidate) -> Finish {
        match self
            .specs
            .save(
                &job.key,
                Some(job.old_rev),
                candidate.body.clone(),
                &candidate.secrets,
                &job.account,
            )
            .await
        {
            Ok(rev) => {
                self.done(job, step::CHECK).await;
                if let Err(error) = self.specs.discard_before(&job.key, rev).await {
                    tracing::warn!(%error, "révisions antérieures de la description non purgées");
                }
                Finish::done()
            }
            Err(error) => {
                tracing::error!(%error, "révision de la description non écrite");
                self.step_failed(job, step::CHECK).await;
                Finish::failed(Cause::StepFailed)
            }
        }
    }

    /// Une erreur du moteur avant tout geste sur le service : « Le service n'a pas été modifié ».
    fn before_any_gesture(&self, error: EngineError) -> Finish {
        let (cause, unknown) = cause_of(error);
        if unknown {
            Finish::unknown(cause)
        } else {
            Finish::failed(cause)
        }
    }

    /// Une erreur du moteur après un geste destructif : retour à l'état d'avant, sauf si le moteur ne répond plus.
    async fn after_gesture(&self, job: &Job, _candidate: &Candidate, error: EngineError) -> Finish {
        let (cause, unknown) = cause_of(error);
        if unknown {
            return Finish::unknown(cause);
        }
        self.roll_back(job, cause, None).await
    }

    /// Une étape destructive n'a pas pu être écrite au journal : le retour ne se fait pas sur un journal illisible
    /// pour une annulation ; sinon retour à l'état d'avant.
    async fn after_begin_refused(
        &self,
        job: &Job,
        finish: Finish,
        new_container: Option<String>,
    ) -> Finish {
        self.roll_back(
            job,
            finish.cause.unwrap_or(Cause::StepFailed),
            new_container,
        )
        .await
    }

    /// Récupère l'image, en suivant la progression, jusqu'à la fin, une annulation ou le délai.
    async fn pull(&self, job: &Job, reference: &str, deadline: Instant) -> Result<(), Finish> {
        self.begin(job, step::PULL).await?;
        let mut feed = self.ops.subscribe();
        let mut stream = self.engine.pull(reference);
        loop {
            tokio::select! {
                item = stream.next() => match item {
                    None => break,
                    Some(Ok(progress)) => {
                        journal(
                            self.ops
                                .set_progress(
                                    &job.id,
                                    &Progress {
                                        percent: progress.percent,
                                        items_done: None,
                                        bytes_done: Some(progress.bytes_done),
                                        bytes_total: progress.bytes_total,
                                    },
                                )
                                .await,
                            "progress",
                        );
                    }
                    Some(Err(error)) => {
                        self.step_failed(job, step::PULL).await;
                        // Un échec de récupération ne touche pas au service (BR-EDIT-021).
                        return Err(self.before_any_gesture(error));
                    }
                },
                change = feed.recv() => {
                    // Une annulation : le flux est abandonné (la connexion au moteur se ferme), rien n'est touché.
                    let cancelling = match change {
                        Ok(snapshot) => snapshot.record.id == job.id
                            && snapshot.record.status == OpStatus::Cancelling,
                        Err(_) => self.is_cancelling(&job.id).await,
                    };
                    if cancelling {
                        drop(stream);
                        return Err(Finish::cancelled());
                    }
                }
                () = tokio::time::sleep_until(deadline.min(Instant::now() + self.context.timings.pull_deadline)) => {
                    drop(stream);
                    return Err(Finish::failed(Cause::Timeout));
                }
            }
        }
        self.done(job, step::PULL).await;
        if self.is_cancelling(&job.id).await {
            return Err(Finish::cancelled());
        }
        Ok(())
    }

    /// Crée les sous-dossiers gérés que la modification ajoute (`BR-EDIT-023`), sans suivre un lien (`SafeFs`).
    async fn prepare_folders(&self, job: &Job) -> Result<(), Finish> {
        if job.new_folders.is_empty() {
            return Ok(());
        }
        let fs = self.safe_fs.clone();
        let key = job.key.clone();
        let sources = job.new_folders.clone();
        let result = tokio::task::spawn_blocking(move || -> Result<(), ()> {
            let root = fs.ensure_service_root(&key).map_err(|_| ())?;
            let owner = Ownership::new(0, 0).ok_or(())?;
            let perms = Perms::new(owner, 0o777).ok_or(())?;
            for source in sources {
                let rel = RelPath::parse(&source).map_err(|_| ())?;
                root.mkdir(&rel, perms).map_err(|_| ())?;
            }
            Ok(())
        })
        .await;
        match result {
            Ok(Ok(())) => Ok(()),
            _ => {
                tracing::error!("dossiers de données non créés : le service n'est pas touché");
                Err(Finish::failed(Cause::StepFailed))
            }
        }
    }

    /// Arrête l'ancien conteneur : l'arrêt voulu est écrit AVANT l'ordre (`BR-LIFE-010`), le délai de grâce est
    /// celui du service. Un refus net : rien n'a été arrêté, le service n'est pas modifié.
    async fn stop_old(&self, job: &Job) -> Result<(), Finish> {
        let old = job.old_container.clone().unwrap_or_default();
        let intent = StopIntent {
            key: job.key.clone(),
            container_id: old.clone(),
            account: job.account.clone(),
            at: self.clock.now(),
        };
        if let Err(error) = self.intents.save(&intent).await {
            tracing::error!(%error, "arrêt voulu non écrit : l'ordre d'arrêt n'est pas donné");
            self.step_failed(job, step::STOP_OLD).await;
            return Err(Finish::failed(Cause::StepFailed));
        }
        self.registry.note_stop_requested(&job.key, intent.at);
        self.registry
            .begin_transition(&job.key, TransitionKind::Stopping);
        let _shown = StepShown {
            registry: &self.registry,
            key: job.key.clone(),
        };
        let grace = Duration::from_secs(u64::from(job.grace_s));
        let wait = wait_budget_with(
            job.grace_s,
            self.context.timings.stop_margin,
            self.context.timings.stop_floor,
        );
        let started = Instant::now();
        let ordered = match tokio::time::timeout(wait, self.engine.stop(&old, grace)).await {
            Ok(result) => result,
            Err(_) => Err(EngineError::Timeout),
        };
        match ordered {
            Ok(()) | Err(EngineError::NotFound) => {
                let evidence = match self.engine.inspect(&old).await {
                    Ok(Some(observation)) => crate::domain::ops::StopEvidence::ExitCode {
                        code: observation.exit_code,
                        waited: started.elapsed(),
                    },
                    _ => crate::domain::ops::StopEvidence::Took(started.elapsed()),
                };
                if stop_was_forced(evidence, job.grace_s) {
                    journal(self.ops.mark_forced(&job.id).await, "forced");
                }
                self.registry.reread(&old).await;
                Ok(())
            }
            Err(error) => {
                self.step_failed(job, step::STOP_OLD).await;
                let (cause, unknown) = cause_of(error);
                // Un refus net : rien n'a été arrêté, l'arrêt voulu n'a pas lieu d'être. Un résultat inconnu le garde.
                if matches!(error, EngineError::Refused | EngineError::Denied) {
                    self.forget_stop(&job.key).await;
                }
                Err(if unknown {
                    Finish::unknown(cause)
                } else {
                    Finish::failed(cause)
                })
            }
        }
    }

    async fn forget_stop(&self, key: &ServiceKey) {
        if let Err(error) = self.intents.clear(key).await {
            tracing::warn!(%error, "arrêt voulu non effacé");
        }
        self.registry.forget_stop_requested(key);
    }

    /// Le plan fermé du conteneur, construit depuis la révision relue en base (`BR-EDIT-018`).
    async fn plan_of(
        &self,
        job: &Job,
        rev: u32,
    ) -> Result<crate::domain::services::plan::ContainerPlan, ()> {
        let (revision, access) = match self.specs.read(&job.key, Some(rev)).await {
            Ok(Some(read)) => read,
            Ok(None) | Err(_) => {
                tracing::error!("description de la modification illisible : rien n'est créé");
                return Err(());
            }
        };
        let SecretsAccess::Opened(secrets) = access else {
            tracing::error!("secrets de la modification illisibles : rien n'est créé");
            return Err(());
        };
        plan_from_spec(
            &revision.body,
            &secrets,
            &PlanContext {
                key: &job.key,
                install_id: &self.context.install_id,
                created_by: &job.account,
                rev: revision.rev,
                managed_root: &self.context.services_root,
                jvm_options_env: self.deployments.jvm_options_env(&revision.body).as_deref(),
            },
        )
        .map_err(|error| {
            tracing::error!(%error, "plan de conteneur refusé : rien n'est créé");
        })
    }

    /// Surveille le nouveau service pendant la fenêtre (`BR-EDIT-019`) : un arrêt, code zéro compris, une relance
    /// par le moteur ou une disparition est un échec.
    async fn watch(&self, container: &str) -> Watched {
        let timings = self.context.timings;
        let end = Instant::now() + timings.watch;
        while Instant::now() < end {
            tokio::time::sleep(timings.poll).await;
            match self.engine.inspect(container).await {
                Ok(Some(observation)) => {
                    if matches!(
                        observation.run_state,
                        RunState::Exited | RunState::Dead | RunState::Restarting
                    ) {
                        self.registry.reread(container).await;
                        return Watched::Fell;
                    }
                }
                Ok(None) => return Watched::Fell,
                Err(_) => {}
            }
        }
        Watched::Held
    }

    /// Supprime l'ancien conteneur mis de côté, sans volume ni force. Un échec le laisse de côté, tracé : le nouveau
    /// tourne, rien n'est perdu.
    async fn remove_old(&self, job: &Job) {
        let Some(old) = &job.old_container else {
            return;
        };
        let removed = tokio::time::timeout(
            self.context.timings.step_wait,
            self.engine.remove_keeping_volumes(old),
        )
        .await
        .unwrap_or(Err(EngineError::Timeout));
        match removed {
            Ok(()) | Err(EngineError::NotFound) => {
                if let Err(error) = self.containers.forget_container(old).await {
                    tracing::warn!(%error, "ancien conteneur supprimé non oublié");
                }
                self.registry.forget_hearth_container(old);
            }
            Err(error) => {
                tracing::warn!(operation = %job.id, %error, "ancien conteneur laissé de côté après une modification réussie");
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Le retour à l'état d'avant
    // -----------------------------------------------------------------------------------------

    /// Le nouveau a échoué : ses dernières lignes sont gardées, il est retiré, la révision candidate abandonnée,
    /// l'ancien remis (`BR-EDIT-020`). Rien n'est demandé. Si le retour échoue aussi : l'ancien n'est pas
    /// supprimé, aucun service ne repart seul (`BR-EDIT-022`).
    async fn roll_back(&self, job: &Job, cause: Cause, new_container: Option<String>) -> Finish {
        if let Some(new) = &new_container {
            let lines = self.tail_of(new).await;
            self.tails.keep(&job.id, lines);
        } else if let Some(new) = self.new_container_of(job).await {
            let lines = self.tail_of(&new).await;
            self.tails.keep(&job.id, lines);
        }
        if let Err(error) = self.ops.begin_rollback(&job.id).await {
            tracing::warn!(%error, "retour à l'état d'avant : opération non passée en retour arrière");
        }
        match self.restore_previous(job).await {
            Ok(()) => Finish {
                status: OpStatus::RolledBack,
                cause: Some(cause),
                rolled_back: true,
            },
            Err(()) => {
                tracing::error!(operation = %job.id, "retour à l'état d'avant impossible : l'ancien service est gardé, arrêté");
                Finish {
                    status: OpStatus::Interrupted,
                    cause: Some(Cause::StepFailed),
                    rolled_back: false,
                }
            }
        }
    }

    /// Le conteneur « nouveau » du service s'il existe : celui qui porte le nom du service pendant que l'ancien est
    /// de côté.
    async fn new_container_of(&self, job: &Job) -> Option<String> {
        let old = job.old_container.as_deref();
        let consigned = self.containers.of(&job.key).await.ok()?;
        for id in consigned {
            if Some(id.as_str()) == old {
                continue;
            }
            if let Ok(Some(observation)) = self.engine.inspect(&id).await
                && observation.name == job.name
            {
                return Some(id);
            }
        }
        None
    }

    async fn tail_of(&self, container: &str) -> Vec<String> {
        let tty = self
            .engine
            .inspect(container)
            .await
            .ok()
            .flatten()
            .is_some_and(|observation| observation.tty);
        let mut stream = self.engine.logs(
            container,
            LogQuery {
                follow: false,
                tail: Some(u32::try_from(TAIL_LINES).unwrap_or(20)),
                since_ms: None,
                tty,
            },
        );
        let mut lines: Vec<String> = Vec::new();
        let read = async {
            while let Some(item) = stream.next().await {
                match item {
                    Ok(line) => {
                        lines.push(line.text.clone());
                        if lines.len() > TAIL_LINES {
                            lines.remove(0);
                        }
                    }
                    Err(_) => break,
                }
            }
        };
        let _ = tokio::time::timeout(TAIL_READ, read).await;
        lines
    }

    /// Remet l'ancien service, d'après ce que le moteur et la base disent (jamais la mémoire d'une exécution) :
    /// retire le nouveau conteneur, abandonne la révision candidate, rend son nom à l'ancien, le relance s'il
    /// tournait. Sert à l'échec d'une modification ET à la décision « Revenir à l'état d'avant ».
    async fn restore_previous(&self, job: &Job) -> Result<(), ()> {
        self.restore(&job.key, &job.name, job.shape, Some(job.old_rev))
            .await
    }

    async fn restore(
        &self,
        key: &ServiceKey,
        name: &str,
        shape: Shape,
        known_base: Option<u32>,
    ) -> Result<(), ()> {
        let was_running = shape.was_running;
        let consigned = self.containers.of(key).await.map_err(|_| ())?;
        let prefix = format!("{name}.hearth-prev-");
        let mut parked: Option<(String, RunState)> = None;
        // La révision de départ : celle du nom mis de côté quand il existe (il la porte), sinon celle que l'appelant
        // connaît.
        let mut base = known_base;
        let mut named: Vec<(String, RunState)> = Vec::new();
        for id in consigned {
            match self.engine.inspect(&id).await {
                Ok(Some(observation)) => {
                    if let Some(rev) = observation.name.strip_prefix(&prefix) {
                        if let Ok(rev) = rev.parse::<u32>() {
                            base = Some(rev);
                        }
                        parked = Some((id, observation.run_state));
                    } else if observation.name == name {
                        named.push((id, observation.run_state));
                    }
                }
                Ok(None) => {
                    if let Err(error) = self.containers.forget_container(&id).await {
                        tracing::warn!(%error, "conteneur disparu non oublié");
                    }
                    self.registry.forget_hearth_container(&id);
                }
                Err(_) => return Err(()),
            }
        }
        let wait = self.context.timings.step_wait;
        let old = match parked {
            Some((old, state)) => {
                // L'ancien est de côté : le nouveau (s'il existe) part, puis l'ancien reprend son nom.
                for (new, _) in &named {
                    let removed = tokio::time::timeout(wait, self.engine.remove(new))
                        .await
                        .unwrap_or(Err(EngineError::Timeout));
                    if removed.is_err() {
                        return Err(());
                    }
                    if let Err(error) = self.containers.forget_container(new).await {
                        tracing::warn!(%error, "conteneur retiré non oublié");
                    }
                    self.registry.forget_hearth_container(new);
                }
                let renamed = tokio::time::timeout(wait, self.engine.rename(&old, name))
                    .await
                    .unwrap_or(Err(EngineError::Timeout));
                if renamed.is_err() {
                    return Err(());
                }
                (old, state)
            }
            // Pas de conteneur d'avant (« Recréer le service » d'un conteneur disparu) : ce qui porte le nom est le
            // nouveau, il part ; il n'y a rien à remettre.
            None if !shape.had_container => {
                for (new, _) in &named {
                    let removed = tokio::time::timeout(wait, self.engine.remove(new))
                        .await
                        .unwrap_or(Err(EngineError::Timeout));
                    if removed.is_err() {
                        return Err(());
                    }
                    if let Err(error) = self.containers.forget_container(new).await {
                        tracing::warn!(%error, "conteneur retiré non oublié");
                    }
                    self.registry.forget_hearth_container(new);
                }
                return self.abandon_candidate(key, base).await;
            }
            // L'ancien n'a jamais été mis de côté : c'est celui qui porte le nom.
            None => match named.into_iter().next() {
                Some(old) => old,
                None => return Err(()),
            },
        };
        self.abandon_candidate(key, base).await?;
        let (old_id, state) = old;
        if was_running && state != RunState::Running {
            let started = tokio::time::timeout(wait, self.engine.start(&old_id))
                .await
                .unwrap_or(Err(EngineError::Timeout));
            if started.is_err() {
                self.registry.reread(&old_id).await;
                return Err(());
            }
            self.forget_stop(key).await;
        }
        self.registry.reread(&old_id).await;
        Ok(())
    }

    /// La révision candidate est abandonnée : la révision de départ redevient la courante. Seule une révision
    /// plus récente que celle de départ est candidate (jamais la vraie révision courante d'un service dont la
    /// candidate n'a pas été écrite). Sans révision de départ connue : la plus haute, s'il y en a deux.
    async fn abandon_candidate(&self, key: &ServiceKey, base: Option<u32>) -> Result<(), ()> {
        let Ok(revisions) = self.specs.revisions(key).await else {
            return Err(());
        };
        let candidate = match base {
            Some(base) => revisions.iter().copied().filter(|rev| *rev > base).max(),
            None if revisions.len() >= 2 => revisions.iter().copied().max(),
            None => None,
        };
        if let Some(candidate) = candidate
            && let Err(error) = self.specs.abandon(key, candidate).await
        {
            tracing::error!(%error, "révision candidate non abandonnée");
            return Err(());
        }
        Ok(())
    }

    // -----------------------------------------------------------------------------------------
    // Les décisions sur une opération interrompue (BR-EDIT-031)
    // -----------------------------------------------------------------------------------------

    /// « Revenir à l'état d'avant » : le retour se lit dans l'état du moteur et de la base.
    async fn revert_interrupted(&self, snapshot: &OpSnapshot) {
        let record = &snapshot.record;
        // Le service tournait si le plan arrête l'ancien.
        let was_running = record
            .plan
            .steps()
            .iter()
            .any(|planned| planned.name.as_str() == step::STOP_OLD);
        let name = record.service_name.clone();
        let had_container = record
            .plan
            .steps()
            .iter()
            .any(|planned| planned.name.as_str() == step::RENAME_OLD);
        let shape = Shape {
            had_container,
            was_running,
        };
        let outcome = self.restore(&record.service, &name, shape, None).await;
        let (status, cause) = match outcome {
            Ok(()) => (OpStatus::RolledBack, Some(Cause::DecidedByAccount)),
            Err(()) => (OpStatus::Interrupted, Some(Cause::StepFailed)),
        };
        journal(self.ops.finish(&record.id, status, cause).await, "finish");
    }

    /// « Garder la nouvelle configuration » : l'ancien conteneur mis de côté est supprimé, les révisions d'avant
    /// partent. Rien n'est démarré.
    async fn keep_interrupted(&self, snapshot: &OpSnapshot) {
        let record = &snapshot.record;
        // L'opération est déjà `succeeded` : son verrou est rendu. Le geste reprend le verrou exclusif du service,
        // sinon il supprimerait l'ancien conteneur (et la révision de retour arrière) d'une modification acceptée
        // entre-temps. Service tenu : rien n'est retiré, l'ancien reste de côté.
        let holder = format!("keep-{}", record.id);
        let held = self
            .ops
            .locks()
            .with(|table| table.acquire_exclusive(record.service.as_str(), &holder, OpKind::Update))
            .is_ok();
        if !held {
            tracing::warn!(operation = %record.id, "service tenu par une autre action : garder n'a rien retiré");
            return;
        }
        self.keep_locked(record).await;
        self.ops.locks().with(|table| {
            table.release_exclusive(record.service.as_str(), &holder);
        });
    }

    async fn keep_locked(&self, record: &crate::application::ports::OpRecord) {
        let prefix = format!("{}.hearth-prev-", record.service_name);
        let Ok(consigned) = self.containers.of(&record.service).await else {
            return;
        };
        for id in consigned {
            if let Ok(Some(observation)) = self.engine.inspect(&id).await
                && observation.name.starts_with(&prefix)
            {
                let removed = tokio::time::timeout(
                    self.context.timings.step_wait,
                    self.engine.remove_keeping_volumes(&id),
                )
                .await
                .unwrap_or(Err(EngineError::Timeout));
                if removed.is_ok() {
                    if let Err(error) = self.containers.forget_container(&id).await {
                        tracing::warn!(%error, "ancien conteneur supprimé non oublié");
                    }
                    self.registry.forget_hearth_container(&id);
                } else {
                    tracing::warn!(operation = %record.id, "ancien conteneur laissé de côté après la décision de garder la nouvelle configuration");
                }
            }
        }
        if let Ok(revisions) = self.specs.revisions(&record.service).await
            && let Some(newest) = revisions.iter().max().copied()
            && let Err(error) = self.specs.discard_before(&record.service, newest).await
        {
            tracing::warn!(%error, "révisions antérieures de la description non purgées");
        }
    }

    // -----------------------------------------------------------------------------------------
    // La fin
    // -----------------------------------------------------------------------------------------

    async fn settle(&self, job: &Job, finish: Finish, took: Duration) {
        // Une annulation arrivée pendant que l'étape échouait gagne : un statut « en annulation » ne devient jamais
        // « échoué » (diagramme d'états, ADR-0045).
        let finish = if finish.status == OpStatus::Failed && self.is_cancelling(&job.id).await {
            Finish::cancelled()
        } else {
            finish
        };
        match self
            .ops
            .finish_with(
                &job.id,
                finish.status,
                finish.cause,
                self.draft(job, &finish),
            )
            .await
        {
            Ok(()) => {}
            Err(OpError::IllegalTransition | OpError::Stale) => tracing::info!(
                operation = %job.id,
                "modification non close : l'opération a changé de statut entre-temps (annulation)"
            ),
            Err(error) => tracing::error!(%error, "modification non close dans le journal"),
        }
        // La trace dit l'opération, jamais le service, l'image ni une variable.
        tracing::info!(
            operation = %job.id,
            status = ?finish.status,
            cause = finish.cause.map(Cause::as_str),
            duration_ms = u64::try_from(took.as_millis()).unwrap_or(u64::MAX),
            "modification terminée"
        );
    }

    /// Une entrée au journal d'activité (`BR-EDIT-033`) : le compte, le service, les catégories changées, les
    /// nombres de variables, l'issue et sa cause typée. Jamais un nom, une valeur, la commande ni un chemin.
    fn draft(&self, job: &Job, finish: &Finish) -> Option<AuditDraft> {
        let service = ClientName::parse(&job.name)?;
        let outcome = match (finish.status, finish.cause) {
            (OpStatus::Succeeded, _) => Outcome::Succeeded,
            (_, cause) => Outcome::Failed(cause.map_or(Reason::Internal, reason_of)),
        };
        Some(AuditDraft {
            actor: job.actor.clone(),
            action: OpKind::Update.audit_action(),
            target: Target::ServiceUpdate {
                service,
                note: UpdateNote {
                    tag: job.new_tag.as_deref().and_then(ClientName::parse),
                    ports: job.summary.ports,
                    mounts: job.summary.mounts,
                    command: job.summary.command,
                    env_set: job.summary.env_set,
                    env_removed: job.summary.env_removed,
                    public_address: job.summary.public_address,
                    host_ports: job.host_ports.clone(),
                    stopped_during: job.shape.had_container && !job.shape.was_running,
                    rolled_back: finish.rolled_back,
                },
            },
            outcome,
        })
    }

    /// Une panique de l'exécution : l'opération est close `interrupted` (le résultat est inconnu), le verrou rendu.
    async fn close_after_panic(&self, id: &str, name: &str, actor: Actor) {
        let draft = ClientName::parse(name).map(|service| AuditDraft {
            actor,
            action: OpKind::Update.audit_action(),
            target: Target::ServiceUpdate {
                service,
                note: UpdateNote::default(),
            },
            outcome: Outcome::Failed(Reason::Internal),
        });
        journal(
            self.ops
                .finish_with(id, OpStatus::Interrupted, Some(Cause::Interrupted), draft)
                .await,
            "finish",
        );
    }
}

#[async_trait::async_trait]
impl DecisionEffects for ServiceUpdates {
    async fn decided(self: Arc<Self>, snapshot: OpSnapshot, choice: Choice) {
        if snapshot.record.kind != OpKind::Update {
            return;
        }
        match choice {
            Choice::Revert => self.revert_interrupted(&snapshot).await,
            Choice::Keep => self.keep_interrupted(&snapshot).await,
            Choice::Continue | Choice::Cancel | Choice::Verified => {}
        }
    }
}

/// L'état permet la modification : en marche, arrêté, en erreur ou qui redémarre en boucle (`BR-EDIT-003`).
fn updatable(state: ServiceStateKind) -> bool {
    matches!(
        state,
        ServiceStateKind::Running
            | ServiceStateKind::Stopped
            | ServiceStateKind::Error
            | ServiceStateKind::RestartLoop
    )
}

/// L'étape annoncée tant que l'arrêt dure, retirée quoi qu'il arrive.
struct StepShown<'a> {
    registry: &'a ServicesRegistry,
    key: ServiceKey,
}

impl Drop for StepShown<'_> {
    fn drop(&mut self) {
        self.registry.end_transition(&self.key);
    }
}

fn account_of(actor: &Actor) -> String {
    actor
        .account
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_default()
}

/// Une écriture du journal des opérations qui échoue ne fait pas échouer la modification : l'ordre est donné, ou
/// ne le sera pas. Elle se voit : la reprise lira une étape incomplète.
fn journal<T>(result: Result<T, OpError>, what: &'static str) {
    if let Err(error) = result {
        tracing::warn!(%error, write = what, "journal de la modification non écrit");
    }
}
