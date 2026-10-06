//! Cas d'usage de la mise à jour de l'agent : lire l'état (`view`) et demander la mise à jour
//! (`start`). Sans Tauri : les commandes ne font que les appeler. La cible vient du flux de versions
//! lu par le CLIENT (`update::service::UpdateService::agent_target`), jamais de l'interface. Les
//! règles de rôle, de signature, de somme et d'adresse sont à l'AGENT ; ici seulement ce que le
//! client sait avant d'envoyer : rien n'est proposé ni envoyé sans cible, ni qui rétrograde, ni qui
//! n'est plus celle que l'utilisateur a vue.

use hearth_link::domain::server::ServerId;
use hearth_link::{ActionOutcome, LinkManager};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::domain::{AgentTarget, is_newer};
use super::dto::{
    AgentAvailableDto, AgentUpdateOutcome, AgentUpdateRefusal, AgentUpdateResultDto,
    AgentUpdateView,
};
use super::wire;
use crate::link_dto::LinkFailure;

/// Un résultat plus récent que cela est annoncé comme un message (BR-UPDATE-017).
pub const RECENT_RESULT_SECONDS: i64 = 24 * 60 * 60;

/// L'identifiant d'un serveur du carnet reçu de l'interface.
pub fn server(text: &str) -> Result<ServerId, LinkFailure> {
    ServerId::parse(text).map_err(|_| LinkFailure::UnknownServer)
}

/// Le résultat date-t-il de moins de 24 h (`now` en secondes depuis l'époque) ? Une date illisible
/// ou dans le futur n'est pas « récente ».
pub fn is_recent(at: &str, now: i64) -> bool {
    OffsetDateTime::parse(at, &Rfc3339).is_ok_and(|at| {
        let age = now - at.unix_timestamp();
        (0..=RECENT_RESULT_SECONDS).contains(&age)
    })
}

/// L'état de la mise à jour de l'agent de ce serveur : `GET /agent/update` puis le dernier résultat
/// (`GET /agent/update/last`, BR-UPDATE-017), et la version disponible dans le flux, calculée ici.
/// Une lecture : pas de suivi ; hors « Connecté », rien n'est envoyé.
pub async fn view(
    manager: &LinkManager,
    id: &ServerId,
    target: Option<&AgentTarget>,
    now: i64,
) -> Result<AgentUpdateView, LinkFailure> {
    let status = manager.agent_update_status(id).await?;
    // Le dernier résultat est relu à part (c'est lui que le retour du lien veut voir) ; si cette
    // seconde lecture échoue, celui de l'état fait foi.
    let last = manager
        .agent_update_last(id)
        .await
        .unwrap_or_else(|_| status.last.clone());
    let last = last.map(|result| {
        let recent = is_recent(&result.at, now);
        AgentUpdateResultDto::new(&result, recent)
    });
    let available = target
        .filter(|target| is_newer(&status.current, target))
        .map(|target| AgentAvailableDto {
            version: target.version().to_string(),
        });
    Ok(AgentUpdateView::new(&status, last, available))
}

/// Demande la mise à jour de l'agent vers `version`, la version que l'utilisateur a vue : la cible
/// envoyée est celle que le client retient (jamais une valeur de l'interface), et seulement si c'est
/// bien cette version. Une action, suivie par `LinkManager::execute` : clé d'opération, « résultat
/// inconnu » à la coupure, jamais rejouée.
pub async fn start(
    manager: &LinkManager,
    id: &ServerId,
    target: Option<&AgentTarget>,
    version: &str,
) -> Result<AgentUpdateOutcome, LinkFailure> {
    let Some(target) = target else {
        return Ok(refused(AgentUpdateRefusal::NoTarget));
    };
    if semver::Version::parse(version.trim()).ok().as_ref() != Some(target.version()) {
        return Ok(refused(AgentUpdateRefusal::TargetChanged));
    }
    // Jamais de rétrogradation : la version de l'agent se lit chez lui, juste avant d'envoyer.
    let status = manager.agent_update_status(id).await?;
    if !is_newer(&status.current, target) {
        return Ok(refused(AgentUpdateRefusal::NotNewer));
    }
    send(manager, id, target).await
}

/// Envoie la demande (la cible est déjà validée) : l'issue est celle de l'agent, ou « inconnue » si
/// le lien tombe avant sa réponse. Jamais rejouée.
pub async fn send(
    manager: &LinkManager,
    id: &ServerId,
    target: &AgentTarget,
) -> Result<AgentUpdateOutcome, LinkFailure> {
    match manager.execute(id, wire::start(target)?).await? {
        ActionOutcome::Completed { status, body, .. } => wire::interpret(status, &body),
        ActionOutcome::ResultUnknown { id } => Ok(AgentUpdateOutcome::Unknown {
            op_id: id.as_str().to_owned(),
        }),
    }
}

fn refused(refusal: AgentUpdateRefusal) -> AgentUpdateOutcome {
    AgentUpdateOutcome::Refused { refusal }
}
