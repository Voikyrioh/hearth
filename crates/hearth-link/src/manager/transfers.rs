//! Envoyer un fichier dans la zone d'attente de l'agent (HRT-96, HRT-95, ADR-0048, `BR-FILES-019`, `BR-MOD-015`).
//!
//! - **Ouvrir** un dépôt (`POST /deposits`) est un ACTE d'administration confirmé (`FileWrite` `0x16`), couvert par
//!   l'élévation : `execute_act` construit la preuve ; l'acte signé est reconstruit depuis cette requête par
//!   `domain/act.rs` (le dernier nom, la taille ; jamais un chemin ni un octet).
//! - **Envoyer** passe par le WebSocket du transfert : des morceaux de 4 Mio lus par [`UploadSource`] (la coquille
//!   lit le fichier local elle-même, la page n'en voit jamais le contenu), jamais plus de [`WINDOW_CHUNKS`] non
//!   accusés en vol, l'empreinte de chaque morceau et celle de l'ensemble calculées ICI. Un morceau est lu, envoyé,
//!   puis oublié : la mémoire est celle d'un morceau, quelle que soit la taille du fichier.
//! - **Reprendre** est un geste (jamais seul) : `upload` rouvre un socket, l'agent annonce son offset durable, les
//!   morceaux déjà reçus ne sont pas renvoyés (ils sont seulement relus localement pour l'empreinte d'ensemble).
//! - **Annuler** : le drapeau d'annulation ferme le socket à la prochaine occasion (au plus un quart de seconde).
//!   L'abandon sur le serveur (`DELETE /transfers/{id}`) est un autre geste (`abandon_deposit`).
//!
//! Rien de ce qui passe ici ne se trace ni ne s'écrit : ni le nom du fichier, ni son contenu, ni le mot de passe.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use hearth_proto::api::transfers::{
    DepositOpened, DepositRequest, DepositView, TransferAbandoned, TransferErrorCode, TreeHasher,
    UploadServerMessage, WINDOW_CHUNKS, chunk_count, chunk_len, digest_of,
};
use hearth_proto::stream::SignedAuth;
use time::OffsetDateTime;
use tokio::time::timeout;
use zeroize::Zeroize;

use super::{ActionOutcome, ActionRequest, LinkManager};
use crate::domain::secret::Secret;
use crate::domain::server::ServerId;
use crate::error::LinkError;
use crate::ports::transport::{ApiRequest, Method, TransportError, UploadFrame};

/// Pas plus long que cela sans rien recevoir de l'agent : le lien est jugé coupé (l'agent ferme à 60 s).
const SILENCE: Duration = Duration::from_secs(45);
/// La cadence à laquelle l'annulation est regardée pendant qu'on attend l'agent.
const CANCEL_POLL: Duration = Duration::from_millis(250);

/// Le fichier à envoyer, lu morceau par morceau. La coquille l'implémente sur le fichier local.
#[async_trait]
pub trait UploadSource: Send {
    /// La taille du fichier, en octets (au moins 1).
    fn size(&self) -> u64;

    /// Les octets du morceau `index` : exactement `chunk_len(len, index)` octets. `Err` : le fichier n'est plus
    /// lisible, ou n'a plus la taille annoncée.
    async fn read_chunk(&mut self, index: u64) -> io::Result<Vec<u8>>;
}

/// Pourquoi l'agent ou le fichier refuse de continuer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadRefusal {
    /// Le disque du serveur n'a plus la place (`BR-FILES-013`) : la partie reçue reste, l'envoi se reprend quand de la
    /// place revient.
    NoSpace {
        missing_bytes: u64,
        free_after_bytes: u64,
    },
    /// Trop de transferts en cours sur le serveur ou pour ce compte.
    TooMany,
    /// Le compte n'a plus le droit de gérer les services.
    MissingRight,
    /// L'agent a refusé le fichier (empreinte d'ensemble, écriture) : recommencer.
    Rejected,
    /// Le fichier local n'est plus lisible.
    SourceUnreadable,
    /// Le fichier local n'a plus la taille annoncée (il a changé pendant l'envoi).
    SourceChanged,
}

/// Comment un envoi se termine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadEnd {
    /// Le fichier est dans la zone d'attente, vérifié : `tree_digest` est l'empreinte d'ensemble.
    Done {
        tree_digest: [u8; 32],
    },
    /// Annulé par le drapeau : le socket est fermé, la partie reçue reste sur le serveur (24 heures).
    Cancelled,
    /// Le lien est coupé : « Reprendre l'envoi » est un geste du compte, rien ne repart seul.
    Interrupted,
    /// Plus rien à reprendre sur le serveur : abandonné, expiré, ou repris par un autre socket.
    Gone,
    Refused(UploadRefusal),
}

fn refusal_of(error: &hearth_proto::api::transfers::TransferError) -> UploadEnd {
    match error.code {
        TransferErrorCode::NoSpace => UploadEnd::Refused(UploadRefusal::NoSpace {
            missing_bytes: error.missing_bytes.unwrap_or(0),
            free_after_bytes: error.free_after_bytes.unwrap_or(0),
        }),
        TransferErrorCode::TransferNotFound | TransferErrorCode::TransferExpired => UploadEnd::Gone,
        TransferErrorCode::TransferLimit => UploadEnd::Refused(UploadRefusal::TooMany),
        TransferErrorCode::MissingRight | TransferErrorCode::Unauthenticated => {
            UploadEnd::Refused(UploadRefusal::MissingRight)
        }
        // Un morceau abîmé en route : rien n'est écrit, la reprise le renvoie (jamais seule).
        TransferErrorCode::ChunkCorrupt => UploadEnd::Interrupted,
        TransferErrorCode::BadRequest
        | TransferErrorCode::ChunkSequence
        | TransferErrorCode::TreeMismatch
        | TransferErrorCode::WriteFailed
        | TransferErrorCode::Unknown => UploadEnd::Refused(UploadRefusal::Rejected),
    }
}

impl LinkManager {
    /// Ouvre un dépôt (`POST /deposits`, acte `FileWrite`). `password` : le mot de passe de confirmation, selon
    /// l'état de l'élévation (jamais gardé). La réponse de l'agent (`201` ou refus typé) est rendue telle quelle.
    pub async fn open_deposit(
        &self,
        id: &ServerId,
        request: &DepositRequest,
        password: Option<&Secret>,
    ) -> Result<ActionOutcome, LinkError> {
        let body = serde_json::to_value(request)
            .map_err(|_| LinkError::Protocol("demande de dépôt illisible".into()))?;
        let action = ActionRequest {
            method: Method::Post,
            path: "/deposits".to_owned(),
            body: Some(body),
        };
        self.execute_act(id, action, password).await
    }

    /// L'état d'un dépôt de ce compte (`GET /deposits/{id}`), pour le retrouver après une coupure.
    pub async fn deposit_view(
        &self,
        id: &ServerId,
        transfer_id: &str,
    ) -> Result<DepositView, LinkError> {
        let path = deposit_path(transfer_id)?;
        let (target, token) = self.credentials(id)?;
        let body = self.typed_get(&target, &token, path).await?;
        serde_json::from_value(body)
            .map_err(|_| LinkError::Protocol("état du dépôt illisible".into()))
    }

    /// Abandonne le dépôt de ce compte et retire la partie reçue (`DELETE /transfers/{id}`, exception nommée de la
    /// liste des actes : le droit suffit). `removed` faux : il n'y avait plus rien.
    pub async fn abandon_deposit(
        &self,
        id: &ServerId,
        transfer_id: &str,
    ) -> Result<TransferAbandoned, LinkError> {
        let path = abandon_path(transfer_id)?;
        let (target, token) = self.credentials(id)?;
        let request = ApiRequest {
            method: Method::Delete,
            path,
            body: None,
            idempotency_key: None,
        };
        let limit = self.inner.deps.config.request_timeout;
        let response = timeout(
            limit,
            self.inner.deps.transport.request(&target, &token, &request),
        )
        .await
        .map_err(|_| LinkError::Timeout)??;
        if !(200..300).contains(&response.status) {
            return Err(super::audit::rejection_at(&response.body, response.status));
        }
        serde_json::from_value(response.body)
            .map_err(|_| LinkError::Protocol("abandon du dépôt illisible".into()))
    }

    /// Envoie le fichier de `source` vers le dépôt `transfer_id`, de l'offset que l'agent annonce. `progress`
    /// reçoit les octets durables sur le serveur (jamais en arrière). `cancel` ferme le socket au plus tard
    /// dans le quart de seconde. Jamais rejoué : un lien coupé rend `Interrupted`, et la reprise est un nouvel appel.
    pub async fn upload(
        &self,
        id: &ServerId,
        transfer_id: &str,
        source: &mut dyn UploadSource,
        cancel: &AtomicBool,
        progress: &mut (dyn FnMut(u64) + Send),
    ) -> Result<UploadEnd, LinkError> {
        let _ = deposit_path(transfer_id)?;
        let (target, token) = self.credentials(id)?;
        let (_, shared) = self.handle(id)?;
        let deps = &self.inner.deps;
        let proof =
            super::device::session_proof(deps, &target, id, &shared.record().username, &token)
                .await
                .map_err(|_| LinkError::NotConnected)?;
        let opened = timeout(
            deps.config.attempt_timeout,
            deps.transport.open_transfer(&target, transfer_id),
        )
        .await;
        let mut conn = match opened {
            Err(_) => return Ok(UploadEnd::Interrupted),
            Ok(Ok(conn)) => conn,
            Ok(Err(TransportError::Api(api))) => {
                return Err(LinkError::Rejected(api.code));
            }
            Ok(Err(_)) => return Ok(UploadEnd::Interrupted),
        };
        let auth = SignedAuth::Auth {
            token: token.expose().to_owned(),
            device: proof,
        };
        let sent = conn.send_auth(&auth).await;
        // La copie du jeton faite pour le message est effacée dès qu'il est parti.
        let SignedAuth::Auth {
            token: mut copy, ..
        } = auth;
        copy.zeroize();
        if sent.is_err() || conn.send_open().await.is_err() {
            return Ok(UploadEnd::Interrupted);
        }

        let size = source.size();
        let total = chunk_count(size);
        let mut digests: Vec<[u8; 32]> = Vec::new();
        let mut next: u64;
        // 1. `ready` : l'offset durable de l'agent.
        let offset = loop {
            if cancel.load(Ordering::SeqCst) {
                return Ok(UploadEnd::Cancelled);
            }
            match timeout(CANCEL_POLL, conn.recv()).await {
                Err(_) => continue,
                Ok(Err(_)) => return Ok(UploadEnd::Interrupted),
                Ok(Ok(UploadFrame::Other)) => {}
                Ok(Ok(UploadFrame::Message(message))) => match *message {
                    UploadServerMessage::Ready { offset, .. } => break offset,
                    UploadServerMessage::Done {
                        size: done,
                        tree_digest,
                        ..
                    } if done == size => {
                        // Déjà complet côté serveur : rien à envoyer.
                        progress(size);
                        let digest = hearth_proto::api::transfers::digest_from_hex(&tree_digest)
                            .unwrap_or([0; 32]);
                        return Ok(UploadEnd::Done {
                            tree_digest: digest,
                        });
                    }
                    UploadServerMessage::Error(error) => return Ok(refusal_of(&error)),
                    _ => {}
                },
            }
        };
        // Un offset qui n'est pas un début de morceau, ou au-delà du fichier : l'agent et le fichier ne parlent plus
        // du même fichier.
        let chunk = u64::from(hearth_proto::api::transfers::CHUNK_BYTES);
        if offset > size || (offset % chunk != 0 && offset != size) {
            return Ok(UploadEnd::Refused(UploadRefusal::SourceChanged));
        }
        next = offset.div_ceil(chunk);
        // 2. L'empreinte de ce que l'agent a déjà : relue LOCALEMENT, jamais renvoyée.
        for index in 0..next {
            if cancel.load(Ordering::SeqCst) {
                return Ok(UploadEnd::Cancelled);
            }
            match read_checked(source, index, size).await {
                Ok(data) => digests.push(digest_of(&data)),
                Err(refusal) => return Ok(UploadEnd::Refused(refusal)),
            }
        }
        progress(offset);

        let mut inflight: u32 = 0;
        let mut finishing = false;
        let mut last_heard = Instant::now();
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Ok(UploadEnd::Cancelled);
            }
            // 3. Remplit la fenêtre.
            while !finishing && next < total && inflight < WINDOW_CHUNKS {
                if cancel.load(Ordering::SeqCst) {
                    return Ok(UploadEnd::Cancelled);
                }
                let data = match read_checked(source, next, size).await {
                    Ok(data) => data,
                    Err(refusal) => return Ok(UploadEnd::Refused(refusal)),
                };
                let digest = digest_of(&data);
                if digests.len() as u64 == next {
                    digests.push(digest);
                }
                if conn.send_chunk(next, &digest, &data).await.is_err() {
                    return Ok(UploadEnd::Interrupted);
                }
                next += 1;
                inflight += 1;
            }
            if !finishing && next == total && inflight == 0 {
                let mut hasher = TreeHasher::new();
                for digest in &digests {
                    hasher.push(digest);
                }
                if conn.send_finish(&hasher.finish()).await.is_err() {
                    return Ok(UploadEnd::Interrupted);
                }
                finishing = true;
            }
            // 4. Attend l'agent.
            match timeout(CANCEL_POLL, conn.recv()).await {
                Err(_) => {
                    if last_heard.elapsed() > SILENCE {
                        return Ok(UploadEnd::Interrupted);
                    }
                }
                Ok(Err(_)) => return Ok(UploadEnd::Interrupted),
                Ok(Ok(UploadFrame::Other)) => last_heard = Instant::now(),
                Ok(Ok(UploadFrame::Message(message))) => {
                    last_heard = Instant::now();
                    match *message {
                        UploadServerMessage::Ack { offset, .. } => {
                            inflight = inflight.saturating_sub(1);
                            progress(offset);
                        }
                        UploadServerMessage::Done {
                            size: done,
                            tree_digest,
                            ..
                        } => {
                            if done != size || !finishing {
                                return Ok(UploadEnd::Refused(UploadRefusal::Rejected));
                            }
                            progress(size);
                            let digest =
                                hearth_proto::api::transfers::digest_from_hex(&tree_digest)
                                    .unwrap_or([0; 32]);
                            return Ok(UploadEnd::Done {
                                tree_digest: digest,
                            });
                        }
                        UploadServerMessage::Error(error) => return Ok(refusal_of(&error)),
                        UploadServerMessage::Ready { .. }
                        | UploadServerMessage::Pong { .. }
                        | UploadServerMessage::Unknown => {}
                    }
                }
            }
        }
    }
}

/// Lit un morceau et vérifie sa taille : un fichier qui a changé en route n'est jamais envoyé tronqué.
async fn read_checked(
    source: &mut dyn UploadSource,
    index: u64,
    size: u64,
) -> Result<Vec<u8>, UploadRefusal> {
    let data = source
        .read_chunk(index)
        .await
        .map_err(|_| UploadRefusal::SourceUnreadable)?;
    if u64::try_from(data.len()).ok() != Some(chunk_len(size, index)) {
        return Err(UploadRefusal::SourceChanged);
    }
    Ok(data)
}

/// Le chemin d'un dépôt : l'identifiant doit être un ULID, jamais un segment fourni tel quel.
fn deposit_path(transfer_id: &str) -> Result<String, LinkError> {
    check_id(transfer_id)?;
    Ok(format!("/deposits/{transfer_id}"))
}

fn abandon_path(transfer_id: &str) -> Result<String, LinkError> {
    check_id(transfer_id)?;
    Ok(format!("/transfers/{transfer_id}"))
}

fn check_id(transfer_id: &str) -> Result<(), LinkError> {
    let valid = transfer_id.len() == 26
        && transfer_id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b.is_ascii_uppercase() && !b"ILOU".contains(&b)));
    if valid {
        Ok(())
    } else {
        Err(LinkError::InvalidInput(crate::error::InputField::Filter))
    }
}

/// La fin de la reprise annoncée à l'ouverture, pour un affichage (RFC 3339 lue en `OffsetDateTime`).
pub fn expires_at(opened: &DepositOpened) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(
        &opened.expires_at,
        &time::format_description::well_known::Rfc3339,
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transfer_id_is_one_ulid_and_anything_else_never_leaves() {
        assert_eq!(
            deposit_path("01J9ZY0G3Q8M2K6W4T7V5N1B9D").as_deref().ok(),
            Some("/deposits/01J9ZY0G3Q8M2K6W4T7V5N1B9D")
        );
        assert_eq!(
            abandon_path("01J9ZY0G3Q8M2K6W4T7V5N1B9D").as_deref().ok(),
            Some("/transfers/01J9ZY0G3Q8M2K6W4T7V5N1B9D")
        );
        for bad in [
            "",
            "../x",
            "01J9ZY0G3Q8M2K6W4T7V5N1B9",
            "01j9zy0g3q8m2k6w4t7v5n1b9d",
            "01J9ZY0G3Q8M2K6W4T7V5N1B9D/../x",
        ] {
            assert!(deposit_path(bad).is_err(), "{bad}");
            assert!(abandon_path(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_signed_act_is_rebuilt_from_the_request_the_link_sends() {
        use crate::domain::act::{Route, classify};
        let body = serde_json::to_value(DepositRequest {
            name: "pack.zip".into(),
            size: 4096,
        })
        .expect("json");
        let Ok(Route::Act(act)) = classify("POST", "/deposits", Some(&body)) else {
            panic!("acte attendu");
        };
        assert_eq!(act.target(), "deposit");
        assert_eq!(
            act.params(),
            vec![b"upload".to_vec(), b"pack.zip".to_vec(), b"4096".to_vec()]
        );
    }

    #[test]
    fn what_the_agent_says_is_told_by_its_code_and_a_corrupt_chunk_never_resumes_alone() {
        use hearth_proto::api::transfers::TransferError;
        let error = |code| refusal_of(&TransferError::new(code));
        assert_eq!(
            error(TransferErrorCode::ChunkCorrupt),
            UploadEnd::Interrupted
        );
        assert_eq!(error(TransferErrorCode::TransferExpired), UploadEnd::Gone);
        assert_eq!(error(TransferErrorCode::TransferNotFound), UploadEnd::Gone);
        assert_eq!(
            error(TransferErrorCode::TransferLimit),
            UploadEnd::Refused(UploadRefusal::TooMany)
        );
        assert_eq!(
            error(TransferErrorCode::MissingRight),
            UploadEnd::Refused(UploadRefusal::MissingRight)
        );
        assert_eq!(
            error(TransferErrorCode::TreeMismatch),
            UploadEnd::Refused(UploadRefusal::Rejected)
        );
        assert_eq!(
            refusal_of(&TransferError {
                missing_bytes: Some(5),
                free_after_bytes: Some(1),
                ..TransferError::new(TransferErrorCode::NoSpace)
            }),
            UploadEnd::Refused(UploadRefusal::NoSpace {
                missing_bytes: 5,
                free_after_bytes: 1
            })
        );
    }
}
