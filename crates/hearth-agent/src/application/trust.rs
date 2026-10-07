//! Identité d'appareil : le défi, la preuve de possession de la clé, l'inscription d'un poste dans
//! la transaction de la connexion par mot de passe, la liste et le retrait des postes (HRT-22,
//! ADR-0023, BR-TRUST-003 à 005, 007, 022).
//!
//! **Dans cette version, la clé ne change aucune décision d'accès.** Elle est enregistrée, prouvée et
//! listée ; la règle « 2 critères sur 3 » qui s'en servira (HRT-24) n'existe pas encore. Une preuve
//! absente, illisible ou fausse n'est jamais une erreur : la connexion se déroule comme sans clé.
//!
//! **Jamais d'inscription par une session seule** : la seule fonction qui inscrit un poste
//! (`TrustService::on_login`) n'est appelée que par la connexion par mot de passe accordée, dans sa
//! transaction.

use std::sync::{Arc, Mutex, PoisonError};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hearth_proto::api::sessions::{ChallengePurpose, ChallengeResponse, DeviceProof, DeviceStatus};
use hearth_proto::device_proof::{
    ALGORITHM_ED25519, Binding, CHALLENGE_LEN, CHALLENGE_TTL_S, PUBLIC_KEY_LEN, SIGNATURE_LEN,
    key_id, signing_bytes,
};
use hearth_proto::fingerprint::Fingerprint;
use subtle::ConstantTimeEq;
use thiserror::Error;
use time::OffsetDateTime;

use super::audit::{AuditTrail, Pending};
use super::ports::{
    ChallengeCrypto, Clock, CryptoError, DeviceRepo, IdGen, MonotonicClock, ProofVerifier, Store,
    StoreError, UnitOfWork,
};
use super::sessions::{ClientInfo, LoginError};
use crate::domain::accounts::{Account, AccountId};
use crate::domain::audit::{Actor, AuditAction, AuditEvent, Origin, Outcome, Target};
use crate::domain::known_address::{self, canonical};
use crate::domain::sessions::SessionId;
use crate::domain::trust::{
    Challenge, ConsumedChallenges, DeviceId, Enrollment, NewDevice, TrustedDevice, check_challenge,
    device_name, has_small_order, judge_enrollment, mac_input,
};

/// Une clé dont la preuve est valide : authentique, fraîche, pas encore consommée, signée sous cette clé.
/// Ne dit pas encore qu'elle est inscrite : c'est l'affaire de la transaction qui l'utilise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedKey {
    pub key_id: String,
    pub public_key: [u8; PUBLIC_KEY_LEN],
    /// Le nonce du défi signé : retenu (`TrustService::consume`) seulement quand la preuve sert.
    pub nonce: [u8; 16],
}

/// Ce que la connexion a fait d'une clé prouvée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceLogin {
    /// La valeur du champ `device` de la réponse ; `None` : rien à dire (clé d'un autre compte).
    pub status: Option<DeviceStatus>,
    /// Le poste auquel rattacher la session ; `None` si aucun poste n'est reconnu.
    pub device: Option<DeviceId>,
}

/// Un poste tel que la liste le montre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceView {
    pub id: DeviceId,
    pub name: String,
    pub created_at: OffsetDateTime,
    pub last_proved_at: OffsetDateTime,
    pub last_addr: String,
    pub current: bool,
}

#[derive(Debug, Error)]
pub enum TrustError {
    #[error(transparent)]
    Crypto(#[from] CryptoError),
}

#[derive(Debug, Error)]
pub enum RemoveError {
    /// Pas de poste à ce numéro chez cet appelant (ou numéro absurde) : la même réponse dans les
    /// deux cas, pour ne rien dire des postes des autres.
    #[error("Poste introuvable")]
    NotFound,
    /// Le poste d'où part la requête ne se retire pas depuis lui-même.
    #[error("C'est le poste que tu utilises : retire-le depuis un autre poste")]
    IsCurrent,
    /// La session courante n'a pas de poste inscrit : ce poste ne peut rien retirer.
    #[error("Ce poste n'a pas de clé enregistrée : retire un poste depuis un poste qui en a une")]
    DeviceRequired,
    /// Preuve de clé manquante, illisible, périmée, rejouée, d'un autre compte ou d'un autre poste
    /// visé, ou qui n'est pas celle du poste courant.
    #[error("La preuve de la clé de ce poste est absente ou invalide")]
    ProofInvalid,
    /// Mot de passe refusé par le chemin de la connexion (même compteurs, même ralentissement).
    #[error(transparent)]
    Password(Box<LoginError>),
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Pourquoi la preuve de clé d'une activation ou d'une désactivation du mode attaque est refusée
/// (HRT-25). L'appelant est déjà authentifié (session valide, administrateur) : le dire n'est pas un
/// oracle.
#[derive(Debug, Error)]
pub enum AttackProofError {
    /// Aucune preuve n'accompagne la requête.
    #[error("Aucune preuve de clé n'accompagne la requête")]
    Missing,
    /// Illisible, périmée, rejouée, d'un autre usage ou de l'autre geste, ou clé non inscrite pour ce
    /// compte.
    #[error("La preuve de la clé de ce poste est invalide")]
    Invalid,
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Longueur maximale d'un identifiant de poste accepté dans une route (un ULID en fait 26) : au-delà
/// ce n'est pas un identifiant, inutile de le chercher.
const MAX_DEVICE_ID_LEN: usize = 64;

pub struct TrustService {
    devices: Arc<dyn DeviceRepo>,
    store: Arc<dyn Store>,
    verifier: Arc<dyn ProofVerifier>,
    crypto: Arc<dyn ChallengeCrypto>,
    monotonic: Arc<dyn MonotonicClock>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGen>,
    /// L'empreinte du certificat que le client a épinglé : celle que la preuve signe.
    fingerprint: Fingerprint,
    trail: Arc<AuditTrail>,
    /// Les défis dont la preuve a été validée : en mémoire, perdus au redémarrage (les défis en
    /// cours le sont aussi : la clé du code change).
    consumed: Mutex<ConsumedChallenges>,
}

impl TrustService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        devices: Arc<dyn DeviceRepo>,
        store: Arc<dyn Store>,
        verifier: Arc<dyn ProofVerifier>,
        crypto: Arc<dyn ChallengeCrypto>,
        monotonic: Arc<dyn MonotonicClock>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGen>,
        fingerprint: Fingerprint,
        trail: Arc<AuditTrail>,
    ) -> Self {
        Self {
            devices,
            store,
            verifier,
            crypto,
            monotonic,
            clock,
            ids,
            fingerprint,
            trail,
            consumed: Mutex::new(ConsumedChallenges::default()),
        }
    }

    fn now_ms(&self) -> u64 {
        u64::try_from(self.monotonic.elapsed().whole_milliseconds()).unwrap_or(0)
    }

    /// Émet un défi pour cet identifiant (existant ou non), cet usage et cette adresse. **Aucune
    /// lecture en base, aucun état retenu** : un flot de demandes ne remplit rien et ne peut priver
    /// personne de son défi ; la réponse est la même pour tout identifiant.
    pub fn issue_challenge(
        &self,
        username: &str,
        purpose: ChallengePurpose,
        addr: &str,
    ) -> Result<ChallengeResponse, TrustError> {
        let nonce = self.crypto.nonce()?;
        let emitted_ms = self.now_ms();
        let mac = self.crypto.mac(&mac_input(
            usage_of(purpose),
            &nonce,
            emitted_ms,
            username,
            addr,
        ));
        let challenge = Challenge {
            nonce,
            emitted_ms,
            mac,
        };
        Ok(ChallengeResponse {
            challenge: STANDARD.encode(challenge.to_bytes()),
            expires_in_s: CHALLENGE_TTL_S,
        })
    }

    /// Vérifie une preuve : algorithme, longueurs, défi authentique pour cet usage, cet identifiant
    /// et cette adresse, frais, jamais rejoué, signature valide **sous la clé fournie** (le même
    /// travail que l'identifiant existe ou non, que la clé soit connue ou non). `None` à la moindre
    /// anomalie, sans panique ; rien n'est dit à l'appelant de la raison.
    ///
    /// Un défi dont la preuve est valide est consommé ici : il ne servira plus.
    pub fn verify(
        &self,
        proof: &DeviceProof,
        binding: Binding<'_>,
        username: &str,
        addr: &str,
    ) -> Option<VerifiedKey> {
        let refuse = |reason: &'static str| {
            // Adresse et raison, jamais l'identifiant saisi ni la clé (BR-AUDIT-005).
            tracing::warn!(%addr, reason, "preuve de clé refusée");
            None
        };
        if proof.algorithm != ALGORITHM_ED25519 {
            return refuse("algorithm");
        }
        let Some(public_key) = decode_exact::<PUBLIC_KEY_LEN>(&proof.public_key) else {
            return refuse("malformed");
        };
        // Une clé de petit ordre vérifie n'importe quel message sous une signature fixe : jamais
        // vérifiée, donc jamais inscrite ni reconnue (`ring` ne la refuse pas).
        if has_small_order(&public_key) {
            return refuse("weak_key");
        }
        let Some(signature) = decode_exact::<SIGNATURE_LEN>(&proof.signature) else {
            return refuse("malformed");
        };
        let Some(challenge_bytes) = decode_exact::<CHALLENGE_LEN>(&proof.challenge) else {
            return refuse("malformed");
        };
        let Some(challenge) = Challenge::parse(&challenge_bytes) else {
            return refuse("malformed");
        };
        let expected = self.crypto.mac(&mac_input(
            binding.usage(),
            &challenge.nonce,
            challenge.emitted_ms,
            username,
            addr,
        ));
        let now_ms = self.now_ms();
        let fresh = {
            let consumed = self.consumed.lock().unwrap_or_else(PoisonError::into_inner);
            check_challenge(&challenge, &expected, now_ms, &consumed)
        };
        if !fresh {
            return refuse("challenge");
        }
        let message = signing_bytes(binding, &self.fingerprint, username, &challenge_bytes);
        if !self
            .verifier
            .verify(&proof.algorithm, &public_key, &message, &signature)
        {
            return refuse("signature");
        }
        // **Aucun état n'est écrit ici** : cette fonction sert des requêtes non authentifiées, et tout
        // le monde détient une clé jetable. Le défi n'est retenu que quand la preuve SERT
        // (`consume`, appelée par la connexion accordée ou par la session valide).
        Some(VerifiedKey {
            key_id: key_id(&public_key),
            public_key,
            nonce: challenge.nonce,
        })
    }

    /// Retient le défi de cette preuve au nom du compte, au moment où elle sert : mot de passe juste et
    /// signature valide, ou session valide et signature valide sous une clé inscrite de ce compte.
    /// `false` : le défi a déjà servi (une requête concurrente a gagné), ou le compte occupe déjà sa part
    /// de la table (`MAX_CONSUMED_PER_OWNER`) ; la preuve compte alors pour absente. Seuls des comptes
    /// authentifiés remplissent la table : un appareil anonyme n'y occupe aucune place.
    pub(super) fn consume(&self, owner: &AccountId, key: &VerifiedKey) -> bool {
        self.consumed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .consume(owner.as_str(), key.nonce, self.now_ms())
    }

    /// Ce que la connexion par mot de passe **accordée** fait d'une clé prouvée, dans sa transaction :
    /// inscrit le poste (jamais ailleurs), date sa preuve, lie l'adresse retenue au poste, journalise
    /// l'inscription. L'adresse doit déjà être retenue pour le compte (la connexion vient de
    /// l'apprendre).
    pub(super) async fn on_login(
        &self,
        tx: &mut dyn UnitOfWork,
        journal: &mut Pending,
        key: &VerifiedKey,
        account: &Account,
        client: &ClientInfo,
        now: OffsetDateTime,
    ) -> Result<DeviceLogin, StoreError> {
        let existing = tx.devices().find_by_key(&key.key_id).await?;
        // La clé inscrite est comparée à celle de la preuve (à temps constant) : l'empreinte de 16
        // octets retrouve le poste, elle ne dit pas que c'est la même clé.
        let owned_here = existing.as_ref().is_some_and(|device| {
            device.account == account.id && bool::from(device.public_key.ct_eq(&key.public_key))
        });
        let owned_elsewhere = existing.is_some() && !owned_here;
        let count = tx.devices().count(&account.id).await?;
        let frozen = tx.devices().enrollment_frozen().await?;
        let addr = canonical(&client.addr);
        let device = match judge_enrollment(owned_here, owned_elsewhere, count, frozen) {
            Enrollment::Proven => {
                let Some(device) = existing else {
                    return Ok(DeviceLogin {
                        status: None,
                        device: None,
                    });
                };
                // La preuve sert : le défi est retenu. Perdu (rejeu concurrent) : comme sans clé.
                if !self.consume(&account.id, key) {
                    return Ok(DeviceLogin {
                        status: None,
                        device: None,
                    });
                }
                tx.devices().prove(&device.id, now, &addr).await?;
                Some((DeviceStatus::Proven, device.id))
            }
            Enrollment::Enroll => {
                if has_small_order(&key.public_key) || !self.consume(&account.id, key) {
                    return Ok(DeviceLogin {
                        status: None,
                        device: None,
                    });
                }
                let id = DeviceId::new(self.ids.new_id());
                tx.devices()
                    .insert(&NewDevice {
                        id: id.clone(),
                        account: account.id.clone(),
                        key_id: key.key_id.clone(),
                        public_key: key.public_key,
                        name: device_name(Some(&client.name)),
                        now,
                        addr: addr.clone(),
                    })
                    .await?;
                journal
                    .record(
                        tx,
                        AuditEvent::new(
                            now,
                            Actor::new(
                                Some(account.username.clone()),
                                Origin::client(Some(&client.name), &client.addr),
                            ),
                            AuditAction::DeviceEnroll,
                            Target::None,
                            Outcome::Succeeded,
                        ),
                    )
                    .await?;
                Some((DeviceStatus::Enrolled, id))
            }
            Enrollment::Limit => {
                return Ok(DeviceLogin {
                    status: Some(DeviceStatus::Limit),
                    device: None,
                });
            }
            Enrollment::Deferred => {
                return Ok(DeviceLogin {
                    status: Some(DeviceStatus::Deferred),
                    device: None,
                });
            }
            Enrollment::Foreign => {
                return Ok(DeviceLogin {
                    status: None,
                    device: None,
                });
            }
        };
        let Some((status, id)) = device else {
            return Ok(DeviceLogin {
                status: None,
                device: None,
            });
        };
        tx.known_addresses()
            .bind_device(&account.id, &addr, &id, now)
            .await?;
        Ok(DeviceLogin {
            status: Some(status),
            device: Some(id),
        })
    }

    /// Une session valide accompagnée d'une preuve de clé valide fait **retenir l'adresse**
    /// (BR-TRUST-007) : dans la transaction du renouvellement de la session. `false` si la clé
    /// n'est pas inscrite pour ce compte (rien n'est écrit). N'inscrit jamais rien.
    pub(super) async fn on_session_proof(
        &self,
        tx: &mut dyn UnitOfWork,
        key: &VerifiedKey,
        account: &AccountId,
        session: &SessionId,
        addr: &str,
        now: OffsetDateTime,
    ) -> Result<bool, StoreError> {
        let Some(device) = tx.devices().find_by_key(&key.key_id).await? else {
            return Ok(false);
        };
        if device.account != *account || !bool::from(device.public_key.ct_eq(&key.public_key)) {
            return Ok(false);
        }
        // La preuve sert (session valide et clé inscrite de ce compte) : le défi est retenu.
        if !self.consume(account, key) {
            return Ok(false);
        }
        let addr = canonical(addr);
        tx.devices().prove(&device.id, now, &addr).await?;
        let list = tx.known_addresses().of_account(account).await?;
        let list = known_address::learn(list, &addr, now);
        tx.known_addresses().replace(account, &list).await?;
        tx.known_addresses()
            .bind_device(account, &addr, &device.id, now)
            .await?;
        tx.sessions().bind_device(session, &device.id).await?;
        Ok(true)
    }

    /// Les postes du compte ; `current` marque celui de la session de l'appelant.
    pub async fn list(
        &self,
        account: &AccountId,
        session: &SessionId,
    ) -> Result<Vec<DeviceView>, StoreError> {
        let current = self.devices.of_session(session).await?;
        Ok(self
            .devices
            .of_account(account)
            .await?
            .into_iter()
            .map(|device: TrustedDevice| DeviceView {
                current: current.as_ref() == Some(&device.id),
                id: device.id,
                name: device.name,
                created_at: device.created_at,
                last_proved_at: device.last_proved_at,
                last_addr: device.last_addr,
            })
            .collect())
    }

    /// Premier temps d'un retrait (Q16 : mot de passe ET clé privée) : la preuve de possession de la clé
    /// du poste **courant**, inscrite pour ce compte, pour l'usage « retrait » et pour CE poste visé
    /// (`target`), liée au jeton de la session. **N'écrit rien** : le défi n'est consommé qu'une fois le
    /// retrait réussi (`remove_proven`). Sans poste inscrit pour la session courante (client ancien) :
    /// `DeviceRequired` ; toute autre anomalie : `ProofInvalid`.
    #[allow(clippy::too_many_arguments)]
    pub async fn verify_removal(
        &self,
        account: &AccountId,
        username: &str,
        session: &SessionId,
        token_hash: &[u8; 32],
        target: &str,
        proof: Option<&DeviceProof>,
        addr: &str,
    ) -> Result<VerifiedKey, RemoveError> {
        if target.is_empty() || target.len() > MAX_DEVICE_ID_LEN {
            return Err(RemoveError::NotFound);
        }
        let Some(current) = self.devices.of_session(session).await? else {
            return Err(RemoveError::DeviceRequired);
        };
        let proof = proof.ok_or(RemoveError::ProofInvalid)?;
        let key = self
            .verify(
                proof,
                Binding::DeviceRemoval { token_hash, target },
                username,
                addr,
            )
            .ok_or(RemoveError::ProofInvalid)?;
        // La clé prouvée est celle du poste courant, inscrit pour ce compte (clé publique comparée).
        let device = self
            .devices
            .find_by_key(&key.key_id)
            .await?
            .ok_or(RemoveError::ProofInvalid)?;
        if device.id != current
            || device.account != *account
            || !bool::from(device.public_key.ct_eq(&key.public_key))
        {
            return Err(RemoveError::ProofInvalid);
        }
        Ok(key)
    }

    /// Preuve de possession d'une clé INSCRITE pour le compte de l'appelant, pour activer ou désactiver le
    /// mode attaque depuis le client (Q14 point 3) : usage `0x03`, liée au jeton de la session et à la
    /// valeur demandée (une preuve d'activation ne désactive pas, et inversement). **N'écrit rien** : le
    /// défi n'est consommé (`consume`) qu'une fois le changement fait.
    pub async fn verify_attack_mode(
        &self,
        account: &AccountId,
        username: &str,
        token_hash: &[u8; 32],
        activate: bool,
        proof: Option<&DeviceProof>,
        addr: &str,
    ) -> Result<VerifiedKey, AttackProofError> {
        let proof = proof.ok_or(AttackProofError::Missing)?;
        let key = self
            .verify(
                proof,
                Binding::AttackMode {
                    token_hash,
                    activate,
                },
                username,
                addr,
            )
            .ok_or(AttackProofError::Invalid)?;
        let device = self
            .devices
            .find_by_key(&key.key_id)
            .await?
            .ok_or(AttackProofError::Invalid)?;
        if device.account != *account || !bool::from(device.public_key.ct_eq(&key.public_key)) {
            return Err(AttackProofError::Invalid);
        }
        Ok(key)
    }

    /// Retire le poste après que `verify_removal` et le mot de passe ont réussi, puis consomme le défi :
    /// **seulement si le retrait a réussi**.
    pub async fn remove_proven(
        &self,
        account: &AccountId,
        current: &SessionId,
        id: &str,
        by: &Actor,
        proven: &VerifiedKey,
    ) -> Result<(), RemoveError> {
        self.remove(account, current, id, by).await?;
        // Le retour de `consume` est ignoré à dessein : le poste n'existe plus, un défi déjà pris par
        // une requête concurrente n'ouvre plus rien.
        self.consume(account, proven);
        Ok(())
    }

    /// Retire un poste du compte de l'appelant, **sans** vérifier ni mot de passe ni preuve (réservé à
    /// `SessionService::remove_device`, qui les exige, et aux tests d'application) : sa clé, son adresse retenue et ses sessions
    /// (un portable perdu ne garde pas une session vivante). Le poste d'où part la requête ne se
    /// retire pas depuis lui-même. Un poste d'un autre compte est « introuvable ».
    pub async fn remove(
        &self,
        account: &AccountId,
        current: &SessionId,
        id: &str,
        by: &Actor,
    ) -> Result<(), RemoveError> {
        if id.is_empty() || id.len() > MAX_DEVICE_ID_LEN {
            return Err(RemoveError::NotFound);
        }
        let id = DeviceId::new(id);
        let now = self.clock.now();
        let mut tx = self.store.begin().await?;
        let device = tx
            .devices()
            .get(account, &id)
            .await?
            .ok_or(RemoveError::NotFound)?;
        if tx.devices().of_session(current).await?.as_ref() == Some(&device.id) {
            return Err(RemoveError::IsCurrent);
        }
        tx.sessions()
            .close_device(&device.id, account, current, now)
            .await?;
        tx.devices().delete(&device.id).await?;
        let mut journal = Pending::default();
        journal
            .record(
                &mut *tx,
                AuditEvent::new(
                    now,
                    by.clone(),
                    AuditAction::DeviceRemove,
                    crate::domain::audit::ClientName::parse(&device.name)
                        .map_or(Target::None, Target::Device),
                    Outcome::Succeeded,
                ),
            )
            .await?;
        tx.commit().await?;
        journal.publish(&self.trail);
        Ok(())
    }
}

/// Octet d'usage du défi demandé.
fn usage_of(purpose: ChallengePurpose) -> u8 {
    match purpose {
        ChallengePurpose::Login => 0x01,
        ChallengePurpose::Session => 0x02,
        ChallengePurpose::AttackMode => 0x03,
        ChallengePurpose::DeviceRemoval => 0x04,
    }
}

/// Décode du base64 standard en exactement `N` octets ; toute autre longueur ou tout autre
/// alphabet est refusé (jamais de panique sur une entrée mal formée).
fn decode_exact<const N: usize>(text: &str) -> Option<[u8; N]> {
    // Borne avant de décoder : un texte démesuré n'est pas une clé, un défi ou une signature.
    if text.len() > N.div_ceil(3) * 4 + 4 {
        return None;
    }
    let bytes = STANDARD.decode(text.trim()).ok()?;
    bytes.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_usage_bytes_are_the_ones_of_the_signed_message() {
        for (purpose, binding_usage) in [
            (ChallengePurpose::Login, Binding::Login.usage()),
            (
                ChallengePurpose::Session,
                Binding::Session {
                    token_hash: &[0; 32],
                }
                .usage(),
            ),
            (
                ChallengePurpose::AttackMode,
                Binding::AttackMode {
                    token_hash: &[0; 32],
                    activate: true,
                }
                .usage(),
            ),
        ] {
            assert_eq!(usage_of(purpose), binding_usage);
        }
    }

    #[test]
    fn decoding_wants_exactly_the_expected_length_and_never_panics() {
        let good = STANDARD.encode([5_u8; 32]);
        assert_eq!(decode_exact::<32>(&good), Some([5_u8; 32]));
        assert_eq!(decode_exact::<32>(&STANDARD.encode([5_u8; 31])), None);
        assert_eq!(decode_exact::<32>(&STANDARD.encode([5_u8; 33])), None);
        for bad in ["", "!!!", "====", "é", &"A".repeat(100_000), "AAAA\nAAAA"] {
            assert_eq!(
                decode_exact::<32>(bad),
                None,
                "{:?}",
                bad.chars().take(8).collect::<String>()
            );
        }
    }
}
