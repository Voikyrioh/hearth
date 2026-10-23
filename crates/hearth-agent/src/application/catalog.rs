//! Le catalogue des fiches de serveurs de jeu que cet agent propose (HRT-101, ADR-0050, BR-FICHE-001 à 004).
//!
//! Les fiches sont embarquées dans le binaire (`domain::catalog::embedded`) et rechargées par le chargeur réel au
//! démarrage : une fiche que le chargeur refuse est ÉCARTÉE, comptée, nommée avec son invariant et consignée au
//! journal d'activité UNE fois (auteur : le système). Une fiche `preview` n'est proposée que par une construction
//! d'essai (`include_preview`). Le catalogue ne rend jamais un secret : une fiche n'en contient aucun.

use hearth_proto::api::catalog::{
    AgreementView, CatalogView, CpuNeed, DiskNeed, EvidenceView, FicheCard, FicheDetail,
    FicheStatus, FolderView, ImageView, MemoryNeed, PortView, QuestionView, RefusedFiche,
    ResourcesView, VariantView,
};

use crate::application::ports::AuditSink;
use crate::domain::audit::{Actor, AuditAction, ClientName, Outcome, Reason, Target};
use crate::domain::catalog::embedded;
use crate::domain::catalog::loading::model::{Fiche, ValueSpec};
use crate::domain::catalog::loading::schema::fiche::{
    Evidence, FolderRole, PlainEvidence, Platform, PortRole, Protocol, Publish, Status,
};
use crate::domain::catalog::loading::schema::settings::Level;

/// Une fiche écartée au chargement : son dossier et l'invariant violé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub dir: String,
    pub invariant: &'static str,
}

pub struct CatalogService {
    fiches: Vec<Fiche>,
    refused: Vec<Refused>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogError {
    /// La fiche n'existe pas dans ce catalogue (ou est d'essai et cet agent est une publication).
    Unknown,
}

impl CatalogService {
    /// Charge les fiches embarquées : toutes sont chargées, les `preview` ne sont gardées que pour une
    /// construction d'essai.
    pub fn embedded(include_preview: bool) -> Self {
        let (accepted, refused) = embedded::load_all();
        Self::from_parts(
            accepted,
            refused
                .into_iter()
                .map(|(dir, refusal)| Refused {
                    dir: dir.to_owned(),
                    invariant: refusal.motif.invariant(),
                })
                .collect(),
            include_preview,
        )
    }

    /// Un catalogue de ces fiches (tests).
    pub fn from_parts(fiches: Vec<Fiche>, refused: Vec<Refused>, include_preview: bool) -> Self {
        let mut fiches: Vec<Fiche> = fiches
            .into_iter()
            .filter(|fiche| include_preview || fiche.file.status == Status::Published)
            .collect();
        fiches.sort_by(|a, b| a.id().cmp(b.id()));
        Self { fiches, refused }
    }

    /// La fiche proposée de cet identifiant, ou rien : une fiche `preview` d'un agent publié n'existe pas.
    pub fn fiche(&self, id: &str) -> Option<&Fiche> {
        self.fiches.iter().find(|fiche| fiche.id() == id)
    }

    pub fn list(&self) -> CatalogView {
        CatalogView {
            fiches: self.fiches.iter().map(card).collect::<Vec<_>>().into(),
            refused: self
                .refused
                .iter()
                .map(|r| RefusedFiche {
                    dir: r.dir.clone(),
                    invariant: r.invariant.to_owned(),
                })
                .collect::<Vec<_>>()
                .into(),
        }
    }

    pub fn detail(&self, id: &str) -> Result<FicheDetail, CatalogError> {
        self.fiche(id).map(detail_of).ok_or(CatalogError::Unknown)
    }

    /// Consigne UNE entrée par fiche écartée, sous le compte « système » (BR-FICHE-007). À appeler une fois au
    /// démarrage, avant d'accepter une requête. Un échec d'écriture n'empêche rien (l'agent démarre).
    pub async fn record_refused(&self, sink: &dyn AuditSink) {
        for refused in &self.refused {
            let (Some(dir), Some(invariant)) = (
                ClientName::parse(&refused.dir),
                ClientName::parse(refused.invariant),
            ) else {
                continue;
            };
            sink.record(
                Actor::system(),
                AuditAction::CatalogFicheRefused,
                Target::FicheRefused { dir, invariant },
                Outcome::Failed(Reason::FicheRefused),
            )
            .await;
        }
    }
}

fn card(fiche: &Fiche) -> FicheCard {
    FicheCard {
        id: fiche.id().to_owned(),
        name: fiche.file.name.clone(),
        summary: fiche.file.summary.clone(),
        version: fiche.file.version,
        status: match fiche.file.status {
            Status::Published => FicheStatus::Published,
            Status::Preview => FicheStatus::Preview,
        },
        installable: !matches!(fiche.file.platform, Platform::Unavailable(_)),
    }
}

fn evidence(evidence: &Evidence) -> EvidenceView {
    match evidence {
        Evidence::Plain(PlainEvidence::Measured) => EvidenceView::Measured,
        Evidence::Plain(PlainEvidence::MeasuredIdle) => EvidenceView::MeasuredIdle,
        Evidence::Plain(PlainEvidence::Unmeasured) => EvidenceView::Unmeasured,
        Evidence::Publisher(publisher) => EvidenceView::Publisher {
            url: publisher.publisher.clone(),
        },
    }
}

fn detail_of(fiche: &Fiche) -> FicheDetail {
    let file = &fiche.file;
    let protocol = |p: Protocol| match p {
        Protocol::Tcp => "tcp",
        Protocol::Udp => "udp",
    };
    let port_role = |r: PortRole| match r {
        PortRole::Game => "game",
        PortRole::Query => "query",
        PortRole::Voice => "voice",
        PortRole::WebAdmin => "web_admin",
        PortRole::RemoteConsole => "remote_console",
        PortRole::Other => "other",
    };
    let publish = |p: Publish| match p {
        Publish::Always => "always",
        Publish::Optional => "optional",
        Publish::Never => "never",
    };
    let folder_role = |r: FolderRole| match r {
        FolderRole::World => "world",
        FolderRole::GameFiles => "game_files",
        FolderRole::Config => "config",
        FolderRole::Logs => "logs",
        FolderRole::Mods => "mods",
        FolderRole::Data => "data",
    };
    FicheDetail {
        card: card(fiche),
        digest: fiche.digest_hex(),
        image: ImageView {
            repository: file.image.repository.clone(),
            tag: file.image.tag.clone(),
            digest: file.image.digest.clone(),
        },
        owner_uid: file.owner.uid,
        owner_gid: file.owner.gid,
        variants: file
            .variant
            .iter()
            .map(|v| VariantView {
                id: v.id.clone(),
                label: v.label.clone(),
                help: v.help.clone(),
                default: v.default,
            })
            .collect::<Vec<_>>()
            .into(),
        ports: file
            .port
            .iter()
            .map(|p| PortView {
                id: p.id.clone(),
                container: p.container,
                protocol: protocol(p.protocol).to_owned(),
                role: port_role(p.role).to_owned(),
                publish: publish(p.publish).to_owned(),
                default_host: p.default_host,
            })
            .collect::<Vec<_>>()
            .into(),
        folders: file
            .folder
            .iter()
            .map(|f| FolderView {
                id: f.id.clone(),
                role: folder_role(f.role).to_owned(),
                target: f.target.clone(),
                backup: f.backup,
            })
            .collect::<Vec<_>>()
            .into(),
        agreements: file
            .agreement
            .iter()
            .map(|a| AgreementView {
                id: a.id.clone(),
                title: a.title.clone(),
                url: a.url.clone(),
                sentence: a.sentence.clone(),
            })
            .collect::<Vec<_>>()
            .into(),
        // Les questions d'installation : les réglages essentiels que la fiche n'impose pas. Une valeur
        // proposée n'est jamais rendue pour un secret.
        questions: fiche
            .settings
            .iter()
            .filter(|s| s.level == Level::Essential && s.imposed.is_none())
            .map(|s| {
                let secret = matches!(s.spec, ValueSpec::Secret { .. });
                QuestionView {
                    id: s.id.clone(),
                    label: s.label.clone(),
                    help: s.help.clone(),
                    secret,
                    proposed: if secret { None } else { s.proposed.clone() },
                }
            })
            .collect::<Vec<_>>()
            .into(),
        resources: ResourcesView {
            cpu: CpuNeed {
                min_millis: file.resources.cpu.min_millis,
                recommended_millis: file.resources.cpu.recommended_millis,
                evidence: evidence(&file.resources.cpu.evidence),
            },
            memory: MemoryNeed {
                min_bytes: file.resources.memory.min_bytes,
                recommended_bytes: file.resources.memory.recommended_bytes,
                evidence: evidence(&file.resources.memory.evidence),
            },
            disk: DiskNeed {
                install_bytes: file.resources.disk.install_bytes,
                evidence: evidence(&file.resources.disk.evidence),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::catalog::test_support::{base, rich};

    #[test]
    fn a_published_catalog_hides_a_preview_fiche_and_a_trial_one_lists_it_with_its_digest() {
        let published = CatalogService::embedded(false);
        assert!(published.list().fiches.iter().all(|c| c.id != "terraria"));
        assert!(published.detail("terraria").is_err());
        let trial = CatalogService::embedded(true);
        let list = trial.list();
        let terraria = list.fiches.iter().find(|c| c.id == "terraria").unwrap();
        assert_eq!(terraria.status, FicheStatus::Preview);
        let detail = trial.detail("terraria").unwrap();
        assert_eq!(detail.digest.len(), 64);
        assert_eq!(detail.image.repository, "ryshe/terraria");
    }

    #[test]
    fn the_detail_shows_what_the_fiche_imposes_with_the_proof_of_each_figure_and_no_secret() {
        let catalog = CatalogService::from_parts(vec![rich()], Vec::new(), true);
        let id = rich().id().to_owned();
        let detail = catalog.detail(&id).unwrap();
        assert!(detail.ports.iter().any(|p| p.role == "game"));
        assert_eq!(detail.resources.cpu.evidence, EvidenceView::Measured);
        assert_eq!(detail.resources.memory.evidence, EvidenceView::MeasuredIdle);
        assert!(matches!(
            detail.resources.disk.evidence,
            EvidenceView::Publisher { .. }
        ));
        assert!(!detail.agreements.is_empty());
        for question in detail.questions.iter().filter(|q| q.secret) {
            assert!(question.proposed.is_none());
        }
        // Les secrets que l'agent tire ne figurent nulle part dans le détail.
        let json = serde_json::to_string(&detail).unwrap();
        assert!(!json.contains("rcon_password\":\""));
    }

    #[test]
    fn a_refused_fiche_is_counted_with_its_invariant_and_unknown_ids_are_refused() {
        let catalog = CatalogService::from_parts(
            vec![base()],
            vec![Refused {
                dir: "bad".into(),
                invariant: "I2",
            }],
            false,
        );
        let view = catalog.list();
        assert_eq!(view.refused.len(), 1);
        assert_eq!(view.refused[0].invariant, "I2");
        assert_eq!(catalog.detail("nope"), Err(CatalogError::Unknown));
    }
}
