//! Cas d'usage des postes de confiance. Sans Tauri : les commandes ne font que l'appeler. Les règles
//! (huit postes, poste courant, preuve) sont à l'AGENT ; rien ici ne décide qui a le droit, un refus
//! est rendu typé.

use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::{ActionOutcome, LinkError, LinkManager};
use hearth_proto::api::devices::{DevicesResponse, removal_refusal};
use hearth_proto::error::{ErrorBody, ErrorCode};
use serde_json::Value;

use super::dto::{DeviceRemovalOutcome, DeviceRemovalRefusal, TrustedDevicesDto};
use crate::link_dto::{InvalidField, LinkFailure};

/// Plus long qu'un identifiant de poste (un ULID : 26 caractères).
const DEVICE_ID_MAX_LEN: usize = 64;

/// L'identifiant d'un serveur du carnet reçu de l'interface.
pub fn server(text: &str) -> Result<ServerId, LinkFailure> {
    ServerId::parse(text).map_err(|_| LinkFailure::UnknownServer)
}

/// Un identifiant de poste sûr à placer dans un chemin : lettres et chiffres, rien d'autre (jamais
/// `/`, `..`, `?`). La bibliothèque le vérifie encore.
pub fn device_id(text: &str) -> Result<&str, LinkFailure> {
    if !text.is_empty()
        && text.len() <= DEVICE_ID_MAX_LEN
        && text.bytes().all(|b| b.is_ascii_alphanumeric())
    {
        Ok(text)
    } else {
        Err(LinkFailure::InvalidInput {
            field: InvalidField::Other,
        })
    }
}

/// Les postes de confiance. Un agent d'avant la fonction répond `404` : `Unsupported`, pas une erreur.
pub async fn list(manager: &LinkManager, id: &ServerId) -> Result<TrustedDevicesDto, LinkFailure> {
    listed(manager.devices_list(id).await)
}

/// Ce que la lecture rend à l'interface : la liste, ou `Unsupported` pour un agent d'avant la clé
/// d'appareil (`404`), ou l'échec typé.
pub fn listed(
    result: Result<DevicesResponse, LinkError>,
) -> Result<TrustedDevicesDto, LinkFailure> {
    match result {
        Ok(list) => Ok(TrustedDevicesDto::Listed {
            devices: list.devices.into_iter().map(Into::into).collect(),
            max: u32::try_from(list.max).unwrap_or(u32::MAX),
        }),
        Err(LinkError::Rejected(Some(ErrorCode::NotFound))) => Ok(TrustedDevicesDto::Unsupported),
        Err(error) => Err(error.into()),
    }
}

/// Retire un poste : le mot de passe actuel ET la preuve de la clé de CE PC (Q16). Le mot de passe
/// n'est pas gardé : il est enveloppé dans un `Secret` dès l'entrée, effacé à la libération.
pub async fn remove(
    manager: &LinkManager,
    id: &ServerId,
    device: &str,
    password: &Secret,
) -> Result<DeviceRemovalOutcome, LinkFailure> {
    let device = device_id(device)?;
    removed(manager.remove_trusted_device(id, device, password).await)
}

/// Ce que le retrait rend à l'interface : « fait », un refus typé, « résultat inconnu » (coupure), ou
/// l'échec typé. Sans clé au coffre, ou face à un agent d'avant la clé (le défi répond `404`), un
/// refus : rien n'est parti.
pub fn removed(
    result: Result<ActionOutcome, LinkError>,
) -> Result<DeviceRemovalOutcome, LinkFailure> {
    match result {
        Ok(ActionOutcome::Completed { status, body, .. }) => interpret(status, &body),
        Ok(ActionOutcome::ResultUnknown { id }) => Ok(DeviceRemovalOutcome::Unknown {
            op_id: id.as_str().to_owned(),
        }),
        Err(LinkError::NoDeviceKey) => Ok(refused(DeviceRemovalRefusal::NoDeviceKey)),
        Err(LinkError::Rejected(Some(ErrorCode::NotFound))) => {
            Ok(refused(DeviceRemovalRefusal::Unsupported))
        }
        Err(error) => Err(error.into()),
    }
}

fn refused(refusal: DeviceRemovalRefusal) -> DeviceRemovalOutcome {
    DeviceRemovalOutcome::Refused { refusal }
}

/// La réponse de l'agent à `DELETE /me/devices/{id}` : `2xx` = retiré, sinon un refus typé d'après
/// le code stable de l'erreur (jamais son texte).
pub fn interpret(status: u16, body: &Value) -> Result<DeviceRemovalOutcome, LinkFailure> {
    if (200..300).contains(&status) {
        return Ok(DeviceRemovalOutcome::Done);
    }
    let Ok(error) = serde_json::from_value::<ErrorBody>(body.clone()) else {
        return Ok(refused(match status {
            404 => DeviceRemovalRefusal::NotFound,
            _ => DeviceRemovalRefusal::Other,
        }));
    };
    let error = error.error;
    Ok(refused(match error.code {
        ErrorCode::ForbiddenRole => return Err(LinkFailure::Forbidden),
        ErrorCode::WrongPassword => DeviceRemovalRefusal::WrongPassword,
        ErrorCode::NotFound => DeviceRemovalRefusal::NotFound,
        ErrorCode::Busy => DeviceRemovalRefusal::Busy,
        ErrorCode::TooManyAttempts => DeviceRemovalRefusal::TooManyAttempts {
            retry_after_s: error.details["retry_after_s"]
                .as_u64()
                .and_then(|seconds| u32::try_from(seconds).ok())
                .unwrap_or(60),
        },
        ErrorCode::Unauthenticated | ErrorCode::SessionExpired => {
            DeviceRemovalRefusal::SessionEnded
        }
        ErrorCode::SessionRevoked => DeviceRemovalRefusal::SessionRevoked,
        ErrorCode::ValidationError => match (
            error.details["field"].as_str(),
            error.details["reason"].as_str(),
        ) {
            (Some("id"), _) => DeviceRemovalRefusal::CurrentDevice,
            (Some("device"), Some(removal_refusal::DEVICE_REQUIRED)) => {
                DeviceRemovalRefusal::NoDeviceKey
            }
            (Some("device"), Some(removal_refusal::PROOF_INVALID)) => {
                DeviceRemovalRefusal::ProofRefused
            }
            _ => DeviceRemovalRefusal::Other,
        },
        _ => DeviceRemovalRefusal::Other,
    }))
}
