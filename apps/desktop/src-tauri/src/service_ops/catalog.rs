//! Le Catalogue et l'installation depuis une fiche (HRT-102, ADR-0050) : deux lectures (la liste, le détail) et
//! l'installation, qui est le déploiement ordinaire avec un membre `source` (`ServiceDeploy`, jamais couvert par
//! l'élévation, jamais rejoué). Les refus se lisent par leur CODE (`deploy::interpret_deploy`) : aucune valeur
//! reçue n'est répétée. Sans Tauri.

use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::{ActionOutcome, LinkError, LinkManager};
use hearth_proto::api::catalog::{EvidenceView, FicheDetail};
use hearth_proto::error::ErrorCode;

use super::catalog_dto::{CatalogDto, FicheDetailDto, FicheInstallRequestDto, FicheReadDto};
use super::deploy::interpret_deploy;
use super::deploy_dto::DeployOutcome;
use crate::link_dto::LinkFailure;

/// Les fiches que cet agent propose (`GET /catalog`) : une lecture, ouverte à tout compte.
pub async fn list(manager: &LinkManager, id: &ServerId) -> Result<CatalogDto, LinkFailure> {
    let view = manager.read_catalog(id).await?;
    Ok(CatalogDto::from(&view))
}

/// Le détail d'une fiche (`GET /catalog/{id}`). Une fiche que l'agent ne propose pas n'est pas une panne : c'est
/// `Unknown`.
pub async fn fiche(
    manager: &LinkManager,
    id: &ServerId,
    fiche: &str,
) -> Result<FicheReadDto, LinkFailure> {
    match manager.read_fiche(id, fiche).await {
        Ok(detail) => Ok(FicheReadDto::Found {
            fiche: Box::new(FicheDetailDto::from(&detail)),
        }),
        Err(LinkError::Rejected(Some(ErrorCode::FicheUnknown))) => Ok(FicheReadDto::Unknown),
        Err(other) => Err(other.into()),
    }
}

/// Installe un serveur de jeu depuis une fiche. `admin_password` : le mot de passe de confirmation, exigé à chaque
/// fois (jamais couvert par l'élévation). L'agent répond `202` : l'issue ne se lit jamais ici, elle arrive par le
/// suivi des opérations. Un lien tombé avant la réponse donne `Unknown` (BR-DEPLOY-028).
pub async fn install(
    manager: &LinkManager,
    id: &ServerId,
    request: FicheInstallRequestDto,
    admin_password: Option<&Secret>,
) -> Result<DeployOutcome, LinkFailure> {
    let spec = request.into_spec()?;
    match manager.deploy_service(id, &spec, admin_password).await? {
        ActionOutcome::Completed { status, body, .. } => interpret_deploy(status, &body),
        ActionOutcome::ResultUnknown { id } => Ok(DeployOutcome::Unknown {
            op_id: id.as_str().to_owned(),
        }),
    }
}

/// Une adresse que l'interface peut faire ouvrir dans le navigateur de l'utilisateur : `https`, un nom de domaine, aucun
/// identifiant dans l'adresse, uniquement des caractères sans effet sur la ligne de commande de l'explorateur.
pub fn is_safe_https_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    url.len() <= 2048
        && !authority.is_empty()
        && !authority.contains('@')
        && authority.contains('.')
        && url
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~:/?#=&%+!,;".contains(&b))
}

/// L'adresse qu'une fiche porte pour ce lien : le contrat d'un contrat (`agreement`, par son identifiant), le site où
/// créer un jeton (`token`, par l'identifiant de la question) ou la source d'un chiffre de l'éditeur (`evidence` :
/// `cpu`, `memory` ou `disk`). L'adresse vient de la fiche lue chez l'agent, JAMAIS de la WebView.
pub fn link_of(detail: &FicheDetail, kind: &str, key: &str) -> Option<String> {
    let url = match kind {
        "agreement" => detail
            .agreements
            .iter()
            .find(|agreement| agreement.id == key)
            .map(|agreement| agreement.url.clone()),
        "token" => detail
            .questions
            .iter()
            .find(|question| question.id == key)
            .and_then(|question| question.obtain_url.clone()),
        "evidence" => {
            let evidence = match key {
                "cpu" => &detail.resources.cpu.evidence,
                "memory" => &detail.resources.memory.evidence,
                "disk" => &detail.resources.disk.evidence,
                _ => return None,
            };
            match evidence {
                EvidenceView::Publisher { url } => Some(url.clone()),
                _ => None,
            }
        }
        _ => None,
    }?;
    is_safe_https_url(&url).then_some(url)
}

/// Lit la fiche chez l'agent et rend l'adresse demandée ; `None` : la fiche n'a pas ce lien (ou une adresse
/// refusée), rien ne s'ouvre.
pub async fn link(
    manager: &LinkManager,
    id: &ServerId,
    fiche: &str,
    kind: &str,
    key: &str,
) -> Result<Option<String>, LinkFailure> {
    let detail = manager.read_fiche(id, fiche).await?;
    Ok(link_of(&detail, kind, key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service_ops::catalog_dto::{FicheAnswerDto, FicheEvidenceDto, FichePortChoiceDto};
    use crate::service_ops::deploy_dto::{DeployAcceptDto, DeployRefusal, DeployResourcesDto};
    use hearth_proto::api::catalog::FicheDetail;
    use hearth_proto::api::deploy::DeploySource;
    use serde_json::{Value, json};

    fn request() -> FicheInstallRequestDto {
        FicheInstallRequestDto {
            name: "mon-terraria".into(),
            fiche: "terraria".into(),
            version: 2,
            digest: "ab".repeat(32),
            variant: None,
            answers: vec![FicheAnswerDto {
                id: "password".into(),
                value: "SENTINEL-ANSWER".into(),
            }],
            agreements: vec!["eula".into()],
            ports: vec![FichePortChoiceDto {
                id: "game".into(),
                host_port: 7778,
            }],
            optional_ports: vec![],
            resources: DeployResourcesDto {
                memory_bytes: 2_147_483_648.0,
                cpu_millis: 1000,
            },
            accept: DeployAcceptDto {
                reserved_ports: false,
                low_disk: true,
            },
        }
    }

    fn error(code: &str, details: Value) -> Value {
        json!({ "error": { "code": code, "message": "SENTINEL-MESSAGE", "details": details } })
    }

    fn read(status: u16, body: Value) -> DeployRefusal {
        match interpret_deploy(status, &body).unwrap() {
            DeployOutcome::Refused { refusal } => refusal,
            other => panic!("refus attendu, reçu {other:?}"),
        }
    }

    #[test]
    fn the_request_names_only_the_fiche_and_never_an_image_a_variable_or_a_command() {
        let spec = request().into_spec().unwrap();
        assert!(spec.image.is_empty() && spec.tag.is_none() && spec.command.is_none());
        assert!(spec.env.is_empty() && spec.ports.is_empty() && spec.mounts.is_empty());
        assert_eq!(spec.resources.memory_bytes, Some(2_147_483_648));
        assert!(spec.accept.low_disk && !spec.accept.reserved_ports);
        let Some(DeploySource::Fiche {
            id,
            version,
            agreements,
            ports,
            world_imported,
            answers,
            ..
        }) = &spec.source
        else {
            panic!("source attendue");
        };
        assert_eq!((id.as_str(), *version), ("terraria", 2));
        assert_eq!(agreements, &vec!["eula".to_owned()]);
        assert_eq!(ports[0].host_port, 7778);
        assert_eq!(answers.len(), 1);
        // La reprise d'un monde n'existe pas dans cette version.
        assert!(!world_imported);
        // Le corps est exactement celui que l'agent relit : un membre `source`, sans membre inconnu.
        let body = serde_json::to_value(&spec).unwrap();
        assert_eq!(body["source"]["type"], "fiche");
        assert!(spec.extra.is_empty());
    }

    #[test]
    fn a_memory_that_is_not_an_exact_count_of_bytes_never_leaves() {
        for bad in [f64::NAN, f64::INFINITY, -1.0, 1.5, 1e300] {
            let mut asked = request();
            asked.resources.memory_bytes = bad;
            assert!(asked.into_spec().is_err(), "{bad}");
        }
    }

    #[test]
    fn only_a_plain_https_address_with_a_domain_ever_opens() {
        assert!(is_safe_https_url("https://example.org/licence"));
        assert!(is_safe_https_url(
            "https://store.example.org/jetons?lang=fr#haut"
        ));
        for bad in [
            "http://example.org/licence",
            "https://",
            "https:///x",
            "https://user:pass@example.org/",
            "https://localhost/x",
            "https://example.org/a b",
            "https://example.org/\"x",
            "https://example.org/^calc",
            "file:///c:/windows/system32/calc.exe",
            "ftp://example.org/x",
            "javascript:alert(1)",
        ] {
            assert!(!is_safe_https_url(bad), "{bad}");
        }
        assert!(!is_safe_https_url(&format!(
            "https://example.org/{}",
            "a".repeat(2100)
        )));
    }

    #[test]
    fn a_link_comes_from_the_fiche_the_agent_sent_never_from_the_page() {
        let mut body = terraria();
        body["agreements"] = json!([{ "id": "eula", "title": "Contrat", "url": "https://example.org/eula", "sentence": "s" }]);
        body["questions"][1]["obtain_url"] = json!("https://example.org/jetons");
        let detail: FicheDetail = serde_json::from_value(body).unwrap();
        assert_eq!(
            link_of(&detail, "agreement", "eula").as_deref(),
            Some("https://example.org/eula")
        );
        assert_eq!(
            link_of(&detail, "token", "password").as_deref(),
            Some("https://example.org/jetons")
        );
        assert_eq!(
            link_of(&detail, "evidence", "disk").as_deref(),
            Some("https://example.org/size")
        );
        // Ni un contrat inconnu, ni une preuve sans adresse, ni un genre inconnu n'ouvrent quoi que ce soit.
        assert_eq!(link_of(&detail, "agreement", "other"), None);
        assert_eq!(link_of(&detail, "evidence", "memory"), None);
        assert_eq!(link_of(&detail, "anything", "eula"), None);
    }

    #[test]
    fn debug_never_shows_an_answer() {
        let shown = format!("{:?}", request());
        assert!(!shown.contains("SENTINEL"), "{shown}");
        assert!(shown.contains("answers: 1"));
    }

    #[test]
    fn each_refusal_of_a_fiche_installation_is_read_by_its_code() {
        assert_eq!(
            read(404, error("FICHE_UNKNOWN", json!(null))),
            DeployRefusal::FicheUnknown
        );
        assert_eq!(
            read(409, error("FICHE_CHANGED", json!({ "version": 3 }))),
            DeployRefusal::FicheChanged
        );
        assert_eq!(
            read(
                422,
                error(
                    "AGREEMENT_REQUIRED",
                    json!({ "ids": ["eula", "Bad Id", "a/b"] })
                )
            ),
            DeployRefusal::AgreementRequired {
                ids: vec!["eula".into()]
            }
        );
        assert_eq!(
            read(
                422,
                error(
                    "SETTING_REFUSED",
                    json!({ "id": "world_name", "reason": "charset", "char": "é au-delà" })
                )
            ),
            DeployRefusal::AnswerRefused {
                id: Some("world_name".into()),
                reason: "charset".into(),
                // Au plus UN caractère, jamais la valeur.
                character: Some("é".into()),
            }
        );
        assert_eq!(
            read(
                422,
                error(
                    "SETTING_REFUSED",
                    json!({ "id": null, "reason": "SENTINEL token=abc", "char": null })
                )
            ),
            DeployRefusal::AnswerRefused {
                id: None,
                reason: "invalid".into(),
                character: None,
            }
        );
        assert_eq!(
            read(
                409,
                error(
                    "HOST_TOO_SMALL",
                    json!({ "lack": "memory", "needed": 4_294_967_296u64, "has": 2_147_483_648u64 })
                )
            ),
            DeployRefusal::HostTooSmall {
                lack: "memory".into(),
                needed: 4_294_967_296.0,
                has: 2_147_483_648.0,
            }
        );
        // Un manque inconnu n'est pas un manque.
        assert_eq!(
            read(
                409,
                error(
                    "HOST_TOO_SMALL",
                    json!({ "lack": "teleporter", "needed": 1, "has": 0 })
                )
            ),
            DeployRefusal::Other
        );
        assert_eq!(
            read(
                507,
                error(
                    "NO_SPACE",
                    json!({ "reason": "install", "needed_bytes": 100, "free_bytes": 40, "missing_bytes": 60 })
                )
            ),
            DeployRefusal::NoSpaceToInstall {
                needed_bytes: 100.0,
                free_bytes: 40.0,
                missing_bytes: 60.0
            }
        );
        // Le manque de place de l'éditeur garde sa case (`low_disk`).
        assert_eq!(
            read(
                507,
                error(
                    "NO_SPACE",
                    json!({ "reason": "low_disk", "free_bytes": 1024 })
                )
            ),
            DeployRefusal::NoSpace {
                free_bytes: Some(1024.0)
            }
        );
        let shown = format!("{:?}", read(409, error("FICHE_CHANGED", json!(null))));
        assert!(!shown.contains("SENTINEL"));
    }

    /// La forme réelle de `GET /catalog/terraria` : ce que l'agent écrit.
    fn terraria() -> Value {
        json!({
            "card": { "id": "terraria", "name": "Terraria", "summary": "Le serveur officiel de Terraria, sans mod.",
                      "version": 2, "status": "preview", "installable": true },
            "digest": "ab".repeat(32),
            "image": { "repository": "ryshe/terraria", "tag": "vanilla-1.4.5.8" },
            "owner_uid": 0, "owner_gid": 0,
            "variants": [],
            "ports": [ { "id": "game", "container": 7777, "protocol": "tcp", "role": "game",
                         "publish": "always", "default_host": 7777 } ],
            "folders": [ { "id": "world", "role": "world", "target": "/root/.local/share/Terraria/Worlds", "backup": true } ],
            "agreements": [],
            "questions": [
                { "id": "world_name", "label": "Nom du monde", "help": "h", "secret": false, "proposed": "Monde",
                  "kind": "text", "charset": "file_stem", "max_len": 100, "min_len": 0, "frozen_after_creation": true },
                { "id": "password", "label": "Mot de passe du serveur", "help": "h", "secret": true,
                  "kind": "secret", "max_len": 100, "proposed": "SENTINEL-SECRET-DEFAULT" },
                { "id": "world_size", "label": "Taille du monde", "help": "h", "secret": false, "proposed": "3",
                  "kind": "choice", "choices": [ { "value": "1", "label": "Petit" }, { "value": "3", "label": "Grand" } ] },
                { "id": "max_players", "label": "Nombre de joueurs", "help": "h", "secret": false, "proposed": "8",
                  "kind": "int", "min": 1, "max": 255 },
                { "id": "future", "label": "?", "help": "", "secret": false, "kind": "from_the_future" }
            ],
            "resources": {
                "cpu": { "min_millis": null, "recommended_millis": null, "evidence": "unmeasured" },
                "memory": { "min_bytes": null, "recommended_bytes": null, "evidence": "unmeasured" },
                "disk": { "install_bytes": null, "evidence": { "publisher": { "url": "https://example.org/size" } } }
            }
        })
    }

    #[test]
    fn the_detail_keeps_every_figure_with_its_proof_and_a_secret_never_carries_a_proposal() {
        let detail: FicheDetail = serde_json::from_value(terraria()).unwrap();
        let dto = FicheDetailDto::from(&detail);
        assert_eq!(dto.card.version, 2);
        assert_eq!(dto.image.digest, None);
        assert_eq!(dto.ports[0].default_host, Some(7777));
        assert!(dto.folders[0].backup);
        assert_eq!(dto.questions.len(), 5);
        let password = &dto.questions[1];
        assert!(password.secret && password.proposed.is_none());
        assert_eq!(dto.questions[2].choices.len(), 2);
        assert_eq!(
            (dto.questions[3].min, dto.questions[3].max),
            (Some(1.0), Some(255.0))
        );
        assert!(dto.questions[0].frozen_after_creation);
        // Un genre d'une version plus récente se lit « inconnu », sans erreur.
        assert_eq!(format!("{:?}", dto.questions[4].kind), "Unknown");
        assert!(matches!(
            dto.resources.disk.evidence,
            FicheEvidenceDto::Publisher { .. }
        ));
        assert!(!format!("{dto:?}").contains("SENTINEL"));
        assert_eq!(dto.unreadable, 0);
    }

    #[test]
    fn an_element_the_client_cannot_read_is_counted_so_the_fiche_is_not_installed_blindly() {
        let mut body = terraria();
        body["ports"] = json!([{ "id": 7 }]);
        let detail: FicheDetail = serde_json::from_value(body).unwrap();
        let dto = FicheDetailDto::from(&detail);
        assert!(dto.ports.is_empty());
        assert_eq!(dto.unreadable, 1);
    }
}
