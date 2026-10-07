//! La clé d'appareil dans le lien (HRT-23, ADR-0023, BR-TRUST-003, 005, 026) : création à la
//! première connexion par mot de passe réussie vers un serveur, défi demandé et signé avant la
//! connexion et avant chaque authentification du flux, retrait d'un poste de confiance (mot de
//! passe ET preuve de la clé, Q16).
//!
//! Règles de ce module :
//! - la clé privée ne sort pas d'ici : aucune fonction publique ne la rend, ne la journalise ni ne
//!   la met dans une erreur ;
//! - la clé n'est écrite au coffre qu'APRÈS la réponse `201` d'une connexion (`Authenticated::new_key`) :
//!   une connexion refusée ne laisse aucune clé orpheline ;
//! - un agent qui ne connaît pas le défi (`404`), un défi qui échoue, un coffre en panne : la
//!   connexion continue SANS preuve, jamais une erreur propre à la clé, jamais une boucle.

use hearth_proto::api::devices::{DevicesResponse, MAX_DEVICES_PER_ACCOUNT};
use hearth_proto::api::sessions::{
    ChallengePurpose, ChallengeRequest, DeviceLoginRequest, DeviceProof, DeviceStatus,
    LoginResponse,
};
use hearth_proto::device_proof::{Binding, TOKEN_HASH_LEN};
use hearth_proto::fingerprint::Fingerprint;
use ring::digest::{SHA256, digest};
use serde_json::json;
use tokio::time::timeout;

use super::{ActionOutcome, ActionRequest, Deps, LinkManager, attempt};
use crate::adapters::device_key::DeviceKey;
use crate::domain::secret::Secret;
use crate::domain::server::ServerId;
use crate::error::{InputField, LinkError};
use crate::ports::transport::{Method, Pin, Target, TransportError};
use crate::ports::vault::SecretKind;

/// Ce que le coffre dit de la clé de ce serveur.
pub(crate) enum KeyState {
    /// Le coffre est en panne (ou la clé rangée est illisible et ne peut pas être remplacée) : aucune
    /// preuve, aucune création ; la connexion continue sans clé.
    Unavailable,
    Present(DeviceKey),
    /// Aucune clé encore : une sera créée si l'agent connaît le défi.
    Absent,
}

pub(crate) fn load_key(deps: &Deps, id: &ServerId) -> KeyState {
    match deps.vault.get(id, SecretKind::DeviceKey) {
        Ok(Some(secret)) => match DeviceKey::from_secret(&secret) {
            Ok(key) => KeyState::Present(key),
            Err(error) => {
                // Une entrée abîmée est remplacée à la prochaine connexion par mot de passe
                // (l'agent réinscrit : `enrolled`). Rien de la clé n'est écrit au journal.
                tracing::warn!(server = %id, %error, "clé d'appareil illisible, elle sera recréée");
                KeyState::Absent
            }
        },
        Ok(None) => KeyState::Absent,
        Err(error) => {
            tracing::warn!(server = %id, %error, "coffre illisible : connexion sans clé");
            KeyState::Unavailable
        }
    }
}

fn pinned(target: &Target) -> Option<Fingerprint> {
    match target.pin {
        Pin::Pinned(fingerprint) => Some(fingerprint),
        Pin::Probe => None,
    }
}

/// Ce que l'agent a répondu à une demande de défi.
pub(crate) enum ChallengeAnswer {
    Issued(String),
    /// Agent d'avant la clé (`404`) : la clé n'est pas prise en charge.
    Unsupported,
    /// Coupure, délai, refus : on ne sait pas.
    Unavailable,
}

pub(crate) async fn ask_challenge(
    deps: &Deps,
    target: &Target,
    username: &str,
    purpose: ChallengePurpose,
) -> ChallengeAnswer {
    let request = ChallengeRequest {
        username: username.to_owned(),
        purpose,
    };
    match timeout(
        deps.config.request_timeout,
        deps.transport.challenge(target, &request),
    )
    .await
    {
        Ok(Ok(response)) => ChallengeAnswer::Issued(response.challenge),
        Ok(Err(TransportError::Api(api))) if matches!(api.status, 404 | 405) => {
            tracing::debug!("agent sans défi : clé d'appareil non prise en charge");
            ChallengeAnswer::Unsupported
        }
        Ok(Err(error)) => {
            tracing::debug!(%error, "défi indisponible : suite sans preuve");
            ChallengeAnswer::Unavailable
        }
        Err(_) => {
            tracing::debug!("défi trop long : suite sans preuve");
            ChallengeAnswer::Unavailable
        }
    }
}

/// Demande un défi. `None` pour TOUT échec (agent ancien `404`, coupure, délai, refus) : l'appelant
/// continue sans preuve. Le défi n'est jamais journalisé.
pub(crate) async fn request_challenge(
    deps: &Deps,
    target: &Target,
    username: &str,
    purpose: ChallengePurpose,
) -> Option<String> {
    match ask_challenge(deps, target, username, purpose).await {
        ChallengeAnswer::Issued(challenge) => Some(challenge),
        ChallengeAnswer::Unsupported | ChallengeAnswer::Unavailable => None,
    }
}

/// SHA-256 du jeton de session, calculé sur les 32 octets que le jeton écrit en hexadécimal
/// (c'est ce que l'agent hache, `SessionToken::hash`). `None` si le jeton n'a pas cette forme.
pub(crate) fn token_hash(token: &str) -> Option<[u8; TOKEN_HASH_LEN]> {
    if token.len() != TOKEN_HASH_LEN * 2 || !token.is_ascii() {
        return None;
    }
    let mut raw = [0_u8; TOKEN_HASH_LEN];
    for (byte, pair) in raw.iter_mut().zip(token.as_bytes().chunks_exact(2)) {
        let pair = std::str::from_utf8(pair).ok()?;
        *byte = u8::from_str_radix(pair, 16).ok()?;
    }
    let hash = digest(&SHA256, &raw);
    let mut out = [0_u8; TOKEN_HASH_LEN];
    out.copy_from_slice(hash.as_ref());
    // Le jeton décodé n'est pas gardé.
    raw.fill(0);
    Some(out)
}

/// Une connexion réussie, et ce qu'elle a fait de la clé.
pub(crate) struct Authenticated {
    pub response: LoginResponse,
    /// Clé créée pour cette connexion : à ranger au coffre, une fois le serveur connu.
    pub new_key: Option<DeviceKey>,
    #[allow(dead_code)]
    pub device: Option<DeviceStatus>,
}

/// `POST /sessions` avec la preuve de la clé quand l'agent la connaît. `id` : le serveur du carnet
/// (absent à la première connexion : la clé ne peut alors qu'être créée). Le mot de passe est
/// effacé après usage.
pub(crate) async fn login(
    deps: &Deps,
    target: &Target,
    id: Option<&ServerId>,
    username: &str,
    password: &Secret,
) -> Result<Authenticated, TransportError> {
    let state = match id {
        Some(id) => load_key(deps, id),
        None => KeyState::Absent,
    };
    let (key, created, challenge) = match state {
        KeyState::Unavailable => (None, false, None),
        KeyState::Present(key) => {
            let challenge =
                request_challenge(deps, target, username, ChallengePurpose::Login).await;
            (Some(key), false, challenge)
        }
        KeyState::Absent => {
            // Une clé n'est créée que si l'agent connaît le défi (sinon elle ne servirait à rien).
            match request_challenge(deps, target, username, ChallengePurpose::Login).await {
                Some(challenge) => match DeviceKey::generate() {
                    Ok(key) => (Some(key), true, Some(challenge)),
                    Err(error) => {
                        tracing::warn!(%error, "clé d'appareil non créée : connexion sans clé");
                        (None, false, None)
                    }
                },
                None => (None, false, None),
            }
        }
    };
    let proof: Option<DeviceProof> = match (&key, &challenge, pinned(target)) {
        (Some(key), Some(challenge), Some(fingerprint)) => {
            key.prove(Binding::Login, &fingerprint, username, challenge)
        }
        _ => None,
    };
    // Sans preuve, la clé créée pour rien est jetée : rien n'est rangé qui ne serait pas inscrit.
    let created = created && proof.is_some();
    let request = DeviceLoginRequest {
        login: attempt::login_request(username, password),
        device: proof,
    };
    let outcome = deps.transport.login_with_device(target, &request).await;
    attempt::wipe(request.login);
    let response = outcome?;
    Ok(Authenticated {
        response: response.login,
        // Gardée seulement si l'agent a PRIS EN COMPTE la clé (inscrite, déjà connue, limite ou gel) : une
        // preuve ignorée ne laisse pas une clé au coffre, la prochaine connexion en crée une.
        new_key: if created && response.device.is_some() {
            key
        } else {
            None
        },
        device: response.device,
    })
}

/// Range la clé créée pour une connexion réussie. Un coffre en échec n'arrête rien : la connexion a
/// réussi, la clé sera recréée à la prochaine connexion par mot de passe.
pub(crate) fn store_new_key(deps: &Deps, id: &ServerId, key: &DeviceKey) {
    if let Err(error) = deps.vault.put(id, SecretKind::DeviceKey, &key.to_secret()) {
        tracing::warn!(server = %id, %error, "clé d'appareil non rangée au coffre");
    }
}

/// Preuve d'ouverture du flux (usage `session`, liée au jeton) ; `None` sans clé, sans défi ou
/// avec un agent ancien : le flux s'authentifie alors par le jeton seul, comme avant.
pub(crate) async fn session_proof(
    deps: &Deps,
    target: &Target,
    id: &ServerId,
    username: &str,
    token: &Secret,
) -> Option<DeviceProof> {
    let KeyState::Present(key) = load_key(deps, id) else {
        return None;
    };
    let fingerprint = pinned(target)?;
    let hash = token_hash(token.expose())?;
    let challenge = request_challenge(deps, target, username, ChallengePurpose::Session).await?;
    key.prove(
        Binding::Session { token_hash: &hash },
        &fingerprint,
        username,
        &challenge,
    )
}

/// Inscription silencieuse de ce PC, au plus UNE fois par exécution (HRT-23) : un client mis à jour
/// a déjà une session mais pas de clé ; si le mot de passe est mémorisé et que l'agent connaît le
/// défi, une reconnexion discrète crée la clé et l'inscrit. Sans mot de passe mémorisé, rien : la
/// page Sécurité le dit. Renvoie vrai si une nouvelle session a été ouverte.
pub(crate) async fn enroll_silently(deps: &Deps, shared: &super::Shared) -> bool {
    use std::sync::atomic::Ordering;
    if shared.enrollment_tried.load(Ordering::SeqCst) {
        return false;
    }
    let id = shared.id();
    if !matches!(load_key(deps, &id), KeyState::Absent) {
        return false;
    }
    if !matches!(deps.vault.get(&id, SecretKind::Password), Ok(Some(_))) {
        return false;
    }
    let username = shared.record().username;
    // Le défi d'abord : un agent ancien ne coûte qu'une requête sans effet, pas une nouvelle session.
    // Tant que l'agent n'a pas répondu clairement (coupure), on réessaiera à la tentative suivante ;
    // dès qu'il a répondu (défi ou « inconnu »), c'est la seule tentative de cette exécution.
    match ask_challenge(deps, &shared.target(), &username, ChallengePurpose::Login).await {
        ChallengeAnswer::Unavailable => return false,
        ChallengeAnswer::Unsupported => {
            shared.enrollment_tried.store(true, Ordering::SeqCst);
            return false;
        }
        ChallengeAnswer::Issued(_) => shared.enrollment_tried.store(true, Ordering::SeqCst),
    }
    matches!(
        attempt::reauthenticate_with_key(deps, shared).await,
        attempt::AttemptResult::Reauthenticated { .. }
    )
}

impl LinkManager {
    /// Les postes de confiance du compte de la session (`GET /me/devices`, lecture typée, session
    /// seule). Sur un agent d'avant la fonction : `Rejected(Some(NotFound))`.
    pub async fn devices_list(&self, id: &ServerId) -> Result<DevicesResponse, LinkError> {
        let (target, token) = self.credentials(id)?;
        let body = self
            .typed_get(&target, &token, "/me/devices".to_owned())
            .await?;
        let list: DevicesResponse = serde_json::from_value(body)
            .map_err(|e| LinkError::Protocol(format!("liste des postes illisible : {e}")))?;
        Ok(DevicesResponse {
            max: if list.max == 0 {
                MAX_DEVICES_PER_ACCOUNT
            } else {
                list.max
            },
            ..list
        })
    }

    /// Retire un poste de confiance : un ACTE D'ADMINISTRATION (Q16), le mot de passe actuel ET la
    /// preuve de la clé de CE poste (usage `0x04`, liée au jeton et au poste visé). Hors
    /// « Connecté » : `NotConnected` sans rien envoyer. Sans clé au coffre : `NoDeviceKey` sans rien
    /// envoyer. Ensuite c'est une action ordinaire (clé d'opération, jamais rejouée, issue
    /// `ResultUnknown` si le lien tombe). Le mot de passe n'est jamais gardé.
    pub async fn remove_trusted_device(
        &self,
        id: &ServerId,
        device_id: &str,
        password: &Secret,
    ) -> Result<ActionOutcome, LinkError> {
        // Un identifiant de poste est un ULID : lettres et chiffres, jamais un chemin.
        if device_id.is_empty()
            || device_id.len() > 64
            || !device_id.bytes().all(|b| b.is_ascii_alphanumeric())
            || password.is_empty()
        {
            return Err(LinkError::InvalidInput(InputField::Credentials));
        }
        let (target, token) = self.credentials(id)?;
        let deps = &self.inner.deps;
        let KeyState::Present(key) = load_key(deps, id) else {
            return Err(LinkError::NoDeviceKey);
        };
        let (_, shared) = self.handle(id)?;
        let username = shared.record().username;
        let fingerprint = pinned(&target).ok_or(LinkError::NoDeviceKey)?;
        let hash = token_hash(token.expose())
            .ok_or_else(|| LinkError::Protocol("jeton illisible".into()))?;
        let request = ChallengeRequest {
            username: username.clone(),
            purpose: ChallengePurpose::DeviceRemoval,
        };
        let challenge = timeout(
            deps.config.request_timeout,
            deps.transport.challenge(&target, &request),
        )
        .await
        .map_err(|_| LinkError::Timeout)??
        .challenge;
        let proof = key
            .prove(
                Binding::DeviceRemoval {
                    token_hash: &hash,
                    target: device_id,
                },
                &fingerprint,
                &username,
                &challenge,
            )
            .ok_or_else(|| LinkError::Protocol("défi illisible".into()))?;
        let body = json!({
            "password": password.expose(),
            "device": proof,
        });
        self.execute(
            id,
            ActionRequest {
                method: Method::Delete,
                path: format!("/me/devices/{device_id}"),
                body: Some(body),
            },
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_token_hash_is_the_sha256_of_the_decoded_bytes_not_of_the_hex_text() {
        let token = "ab".repeat(32);
        let decoded = digest(&SHA256, &[0xab_u8; 32]);
        assert_eq!(token_hash(&token).unwrap().as_slice(), decoded.as_ref());
        let of_text = digest(&SHA256, token.as_bytes());
        assert_ne!(token_hash(&token).unwrap().as_slice(), of_text.as_ref());
    }

    #[test]
    fn a_token_of_the_wrong_shape_has_no_hash_and_never_panics() {
        for bad in [
            "",
            "ab",
            &"zz".repeat(32),
            &"ab".repeat(31),
            &"ab".repeat(33),
            &"é".repeat(32),
        ] {
            assert_eq!(token_hash(bad), None, "{bad}");
        }
    }
}
