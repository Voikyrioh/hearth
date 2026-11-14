//! Reprendre un monde existant : lister les mondes des dossiers ouverts aux services, inspecter un monde, et
//! préparer la copie que le déploiement exécute (HRT-105, `BR-MONDE-002` à `026`, ADR-0059).
//!
//! Ce cas d'usage ne parle qu'à la fiche (ce qu'est un monde), à la configuration de l'agent (les dossiers
//! ouverts) et au port [`WorldFs`] (lecture seule de la source). Il n'écrit rien. La copie elle-même est
//! [`copy::copy_world`], lancée par `application::deployments` sur un fil dédié, sous la place `heavy_io`.
//!
//! Les consultations (liste, inspection) ne sont ni diffusées ni journalisées (`BR-MONDE-003`) ; leurs erreurs ne
//! portent ni chemin ni nom de monde (`BR-MONDE-023`).

pub mod copy;
pub mod search;

use std::path::PathBuf;
use std::sync::Arc;

use hearth_proto::api::worlds::{
    MAX_LISTED_NAMES, WorldEntry, WorldInspection, WorldRef, WorldRoot, WorldsView,
};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use self::search::{Plan, PlanError, find_worlds, plan_world};
use super::catalog::CatalogService;
use super::ports::{FreeSpace, Ownership, WorldFs, WorldFsError};
use crate::domain::catalog::loading::model::Fiche;
use crate::domain::files::RelPath;
use crate::domain::worlds::{
    MAX_DEPTH, MAX_ENTRIES, MEASURE_BUDGET, RulesError, Shape, WorldRules,
};
use crate::infrastructure::config::AllowedRoot;

/// Pourquoi une consultation ou une reprise est refusée. Aucune variante ne porte de valeur reçue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldsError {
    FicheUnknown,
    /// La fiche ne déclare pas ce qu'est un monde, ou le déclare d'une façon que cette version ne reprend pas
    /// (`BR-MONDE-039`).
    NotSupported,
    /// Le dossier n'est pas l'un de ceux que la configuration ouvre aux services (`BR-MONDE-002`).
    UnknownFolder,
    BadPath,
    /// Ce chemin n'est pas un monde de cette fiche, ou est dans un dossier de sauvegardes (`BR-MONDE-005`).
    NotAWorld,
    NotFound,
    /// Plus d'éléments que la borne de la copie.
    TooBig,
    /// Le nom du monde est refusé par le réglage de la fiche (`BR-MONDE-014`).
    NameUnusable,
    /// Lecture impossible (droits, lien, plate-forme).
    Unreadable,
}

impl WorldsError {
    /// Le code stable rendu au client (`field` est `source.world`).
    pub const fn reason(self) -> &'static str {
        match self {
            Self::FicheUnknown => "fiche_unknown",
            Self::NotSupported => "not_supported",
            Self::UnknownFolder => "unknown_folder",
            Self::BadPath => "bad_path",
            Self::NotAWorld => "not_a_world",
            Self::NotFound => "not_found",
            Self::TooBig => "too_big",
            Self::NameUnusable => "name_unusable",
            Self::Unreadable => "unreadable",
        }
    }
}

impl From<PlanError> for WorldsError {
    fn from(error: PlanError) -> Self {
        match error {
            PlanError::NotAWorld => Self::NotAWorld,
            PlanError::TooBig => Self::TooBig,
            PlanError::Fs(error) => Self::from(error),
        }
    }
}

impl From<WorldFsError> for WorldsError {
    fn from(error: WorldFsError) -> Self {
        match error {
            WorldFsError::NotFound => Self::NotFound,
            _ => Self::Unreadable,
        }
    }
}

/// Ce que le déploiement emporte d'une reprise jugée : où est la source, ce qu'elle est, à qui appartient la
/// copie. Le plan, lui, est refait au moment de copier (l'image se récupère avant, des minutes peuvent passer).
#[derive(Debug, Clone)]
pub struct Prepared {
    /// Le dossier ouvert, tel qu'il est écrit dans la configuration (et au journal, `BR-MONDE-023`).
    pub folder: String,
    pub root: PathBuf,
    pub rel: RelPath,
    pub rules: WorldRules,
    /// Le nom du monde, qui devient la réponse figée du réglage de la fiche (`BR-MONDE-014`).
    pub name: String,
    pub owner: Ownership,
    /// Taille à copier et nombre d'éléments laissés (l'espace, `BR-MONDE-012`).
    pub bytes: u64,
    pub not_copied: u64,
}

pub struct WorldImport {
    catalog: Arc<CatalogService>,
    fs: Arc<dyn WorldFs>,
    roots: Vec<AllowedRoot>,
    free: Arc<dyn FreeSpace>,
    /// Un dossier du disque où la copie sera écrite (le dossier de données de l'agent) : l'espace libre.
    data_dir: PathBuf,
}

fn root_text(root: &AllowedRoot) -> String {
    root.path.to_string_lossy().into_owned()
}

fn rfc3339(mtime_ns: i128) -> Option<String> {
    OffsetDateTime::from_unix_timestamp_nanos(mtime_ns)
        .ok()
        .and_then(|at| at.format(&Rfc3339).ok())
}

impl WorldImport {
    pub fn new(
        catalog: Arc<CatalogService>,
        fs: Arc<dyn WorldFs>,
        roots: Vec<AllowedRoot>,
        free: Arc<dyn FreeSpace>,
        data_dir: PathBuf,
    ) -> Arc<Self> {
        Arc::new(Self {
            catalog,
            fs,
            roots,
            free,
            data_dir,
        })
    }

    pub fn fs(&self) -> &Arc<dyn WorldFs> {
        &self.fs
    }

    fn fiche_rules(&self, fiche_id: &str) -> Result<(&Fiche, WorldRules), WorldsError> {
        let fiche = self
            .catalog
            .fiche(fiche_id)
            .ok_or(WorldsError::FicheUnknown)?;
        let rules = WorldRules::from_fiche(fiche).map_err(|error| match error {
            RulesError::NoWorlds | RulesError::Unsupported => WorldsError::NotSupported,
        })?;
        Ok((fiche, rules))
    }

    /// `GET /catalog/{id}/worlds` : les mondes que la fiche reconnaît dans chaque dossier ouvert. Lecture de
    /// noms seulement, trois niveaux, 5 000 éléments au plus par dossier (`BR-MONDE-003`).
    pub async fn list(self: &Arc<Self>, fiche_id: &str) -> Result<WorldsView, WorldsError> {
        let (fiche, rules) = self.fiche_rules(fiche_id)?;
        let this = Arc::clone(self);
        let rules_for_task = rules.clone();
        let roots = self.roots.clone();
        let searched = tokio::task::spawn_blocking(move || {
            roots
                .iter()
                .map(|root| {
                    let folder = root_text(root);
                    let found = this
                        .fs
                        .open_source(&root.path)
                        .map_err(WorldsError::from)
                        .and_then(|source| {
                            find_worlds(source.as_ref(), &rules_for_task, MAX_ENTRIES)
                                .map_err(WorldsError::from)
                        });
                    (folder, found)
                })
                .collect::<Vec<_>>()
        })
        .await
        .map_err(|_| WorldsError::Unreadable)?;
        let roots = searched
            .into_iter()
            .map(|(folder, found)| match found {
                Ok(found) => WorldRoot {
                    folder,
                    readable: true,
                    truncated: found.truncated,
                    worlds: found
                        .worlds
                        .into_iter()
                        .map(|world| WorldEntry {
                            usable: rules.name_usable(fiche, &world.name),
                            path: world.rel.to_string(),
                            name: world.name,
                            files: world.files,
                            bytes: world.bytes,
                            modified_at: world.mtime_ns.and_then(rfc3339),
                        })
                        .collect(),
                },
                Err(_) => WorldRoot {
                    folder,
                    readable: false,
                    truncated: false,
                    worlds: Vec::new(),
                },
            })
            .collect();
        Ok(WorldsView {
            roots,
            max_depth: u32::try_from(MAX_DEPTH).unwrap_or(u32::MAX),
            max_entries: u32::try_from(MAX_ENTRIES).unwrap_or(u32::MAX),
        })
    }

    fn root_of(&self, folder: &str) -> Result<&AllowedRoot, WorldsError> {
        self.roots
            .iter()
            .find(|root| root_text(root) == folder)
            .ok_or(WorldsError::UnknownFolder)
    }

    async fn plan(
        self: &Arc<Self>,
        fiche_id: &str,
        pick: &WorldRef,
    ) -> Result<(Prepared, Plan), WorldsError> {
        let (fiche, rules) = self.fiche_rules(fiche_id)?;
        let root = self.root_of(&pick.folder)?.clone();
        let rel = RelPath::parse(&pick.path).map_err(|_| WorldsError::BadPath)?;
        let (leaf, _) = rel.split_last().ok_or(WorldsError::BadPath)?;
        let name = rules
            .world_name(leaf)
            .ok_or(WorldsError::NotAWorld)?
            .to_owned();
        if !rules.name_usable(fiche, &name) {
            return Err(WorldsError::NameUnusable);
        }
        let owner = Ownership::new(fiche.file.owner.uid, fiche.file.owner.gid)
            .ok_or(WorldsError::NotSupported)?;
        let this = Arc::clone(self);
        let (root_path, task_rules, task_rel) = (root.path.clone(), rules.clone(), rel.clone());
        let plan = tokio::task::spawn_blocking(move || {
            let source = this.fs.open_source(&root_path).map_err(WorldsError::from)?;
            plan_world(source.as_ref(), &task_rules, &task_rel, MEASURE_BUDGET)
                .map_err(WorldsError::from)
        })
        .await
        .map_err(|_| WorldsError::Unreadable)??;
        let prepared = Prepared {
            folder: root_text(&root),
            root: root.path,
            rel,
            rules,
            name,
            owner,
            bytes: plan.bytes,
            not_copied: plan.not_copied,
        };
        Ok((prepared, plan))
    }

    /// `POST /catalog/{id}/worlds/inspect` : ce que la reprise copierait et ce qu'elle laisserait.
    pub async fn inspect(
        self: &Arc<Self>,
        fiche_id: &str,
        pick: &WorldRef,
    ) -> Result<WorldInspection, WorldsError> {
        let (_, plan) = self.plan(fiche_id, pick).await?;
        Ok(WorldInspection {
            files: plan.files.len() as u64,
            bytes: plan.bytes,
            not_copied: plan.not_copied,
            not_copied_names: plan
                .not_copied_names
                .into_iter()
                .take(MAX_LISTED_NAMES)
                .collect(),
            skipped_links: plan.skipped_links,
            skipped_link_names: plan
                .skipped_link_names
                .into_iter()
                .take(MAX_LISTED_NAMES)
                .collect(),
            partial: plan.partial,
            free_bytes: self.free.free_bytes(&self.data_dir).ok(),
        })
    }

    /// Juge une reprise demandée par un déploiement : le monde existe et est un monde de la fiche, son nom est
    /// accepté, la copie est possible. Rien n'est écrit.
    pub async fn prepare(
        self: &Arc<Self>,
        fiche_id: &str,
        pick: &WorldRef,
    ) -> Result<Prepared, WorldsError> {
        let (prepared, plan) = self.plan(fiche_id, pick).await?;
        // Un monde « dossier » se liste et s'inspecte ; sa mise en place dépend d'une fiche qui n'existe pas encore.
        if !matches!(prepared.rules.shape, Shape::File { .. }) {
            return Err(WorldsError::NotSupported);
        }
        // Une mesure coupée à son budget ne part jamais à la copie : elle copierait un monde tronqué.
        if plan.partial {
            return Err(WorldsError::TooBig);
        }
        Ok(prepared)
    }

    /// Le plan de copie, relu au moment de copier.
    pub async fn replan(self: &Arc<Self>, prepared: &Prepared) -> Result<Plan, WorldsError> {
        let this = Arc::clone(self);
        let (root, rules, rel) = (
            prepared.root.clone(),
            prepared.rules.clone(),
            prepared.rel.clone(),
        );
        let plan = tokio::task::spawn_blocking(move || {
            let source = this.fs.open_source(&root).map_err(WorldsError::from)?;
            plan_world(source.as_ref(), &rules, &rel, MEASURE_BUDGET).map_err(WorldsError::from)
        })
        .await
        .map_err(|_| WorldsError::Unreadable)??;
        if plan.partial {
            return Err(WorldsError::TooBig);
        }
        Ok(plan)
    }

    /// Les dossiers ouverts sont-ils au moins un ? Sinon la reprise n'est pas proposée (`BR-MONDE-002`).
    pub fn has_roots(&self) -> bool {
        !self.roots.is_empty()
    }
}
