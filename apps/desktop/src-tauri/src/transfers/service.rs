//! Les transferts de la coquille : le carnet des envois en cours, leur exécution en tâche de fond et la lecture du
//! fichier local (HRT-96, HRT-95, `BR-FILES-019`, `BR-MOD-015`, ADR-0048).
//!
//! - **La coquille lit le fichier** (`FileSource`, un morceau de 4 Mio à la fois, jamais le fichier entier) : la
//!   page ne reçoit qu'un chemin au départ et des nombres ensuite. Un fichier qui change de taille pendant l'envoi
//!   est refusé (`SourceChanged`), jamais envoyé tronqué.
//! - **Un transfert continue quand on quitte l'onglet** : la tâche vit dans la coquille, le panneau « Transferts »
//!   relit le carnet (`TransferBook::list`).
//! - **Rien ne repart seul** : un lien coupé laisse l'état « interrompu » ; reprendre est un geste (`resume`).
//! - **Annuler** ferme le socket puis abandonne le dépôt sur le serveur (la partie reçue est retirée).
//!
//! Le nom du fichier n'est ni tracé ni écrit sur le disque ; seuls des nombres le sont.

use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::{ActionOutcome, LinkError, LinkManager, UploadEnd, UploadRefusal, UploadSource};
use hearth_proto::api::transfers::{CHUNK_BYTES, DepositOpened, DepositRequest, chunk_len};
use hearth_proto::error::{ErrorBody, ErrorCode};
use serde_json::Value;

use super::dto::{
    DepositOutcome, DepositRefusal, TransferDto, TransferRefusalDto, TransferStatusDto,
};
use crate::link_dto::LinkFailure;
use crate::reauth::wire::{Confirmation, confirmation_of};

/// Un transfert du carnet : ce que le panneau montre, plus ce qu'il faut pour le reprendre.
struct Entry {
    dto: TransferDto,
    path: PathBuf,
    cancel: Arc<AtomicBool>,
    running: bool,
}

/// Les envois de cette session, du plus ancien au plus récent. Rien n'est écrit sur le disque : un redémarrage de
/// l'application oublie la liste (la partie reçue reste reprenable 24 heures côté serveur, mais plus depuis ici).
#[derive(Default)]
pub struct TransferBook {
    entries: Mutex<Vec<Entry>>,
}

impl TransferBook {
    fn lock(&self) -> MutexGuard<'_, Vec<Entry>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn list(&self) -> Vec<TransferDto> {
        self.lock().iter().map(|entry| entry.dto.clone()).collect()
    }

    fn insert(&self, entry: Entry) {
        self.lock().push(entry);
    }

    fn with<T>(&self, id: &str, change: impl FnOnce(&mut Entry) -> T) -> Option<T> {
        self.lock()
            .iter_mut()
            .find(|entry| entry.dto.id == id)
            .map(change)
    }

    /// Les octets durables ne reculent jamais.
    fn progress(&self, id: &str, sent: u64) {
        self.with(id, |entry| {
            let sent = sent as f64;
            if sent > entry.dto.sent {
                entry.dto.sent = sent;
            }
        });
    }

    /// Retire un transfert fini (réussi, annulé ou échoué) de la liste. Un transfert qui envoie encore reste.
    pub fn dismiss(&self, id: &str) {
        self.lock()
            .retain(|entry| entry.dto.id != id || entry.running || is_open(entry.dto.status));
    }

    fn finish(&self, id: &str, end: &UploadEnd) {
        self.with(id, |entry| {
            entry.running = false;
            let (status, refusal) = match end {
                UploadEnd::Done { .. } => {
                    entry.dto.sent = entry.dto.size;
                    (TransferStatusDto::Done, None)
                }
                UploadEnd::Cancelled => (TransferStatusDto::Cancelled, None),
                UploadEnd::Interrupted => (TransferStatusDto::Interrupted, None),
                UploadEnd::Gone => (TransferStatusDto::Failed, Some(TransferRefusalDto::Gone)),
                // Pas de place, trop de transferts : la partie reçue reste, l'envoi se reprend plus tard.
                UploadEnd::Refused(
                    refusal @ (UploadRefusal::NoSpace { .. } | UploadRefusal::TooMany),
                ) => (
                    TransferStatusDto::Interrupted,
                    Some(TransferRefusalDto::from(refusal)),
                ),
                UploadEnd::Refused(refusal) => (
                    TransferStatusDto::Failed,
                    Some(TransferRefusalDto::from(refusal)),
                ),
            };
            entry.dto.status = status;
            entry.dto.refusal = refusal;
        });
    }

    fn fail(&self, id: &str, status: TransferStatusDto, refusal: Option<TransferRefusalDto>) {
        self.with(id, |entry| {
            entry.running = false;
            entry.dto.status = status;
            entry.dto.refusal = refusal;
        });
    }
}

fn is_open(status: TransferStatusDto) -> bool {
    matches!(
        status,
        TransferStatusDto::Sending | TransferStatusDto::Interrupted
    )
}

/// Le fichier local, lu un morceau à la fois, sur un fil bloquant (jamais sur l'exécuteur).
pub struct FileSource {
    path: PathBuf,
    size: u64,
}

#[async_trait]
impl UploadSource for FileSource {
    fn size(&self) -> u64 {
        self.size
    }

    async fn read_chunk(&mut self, index: u64) -> io::Result<Vec<u8>> {
        let (path, size) = (self.path.clone(), self.size);
        tokio::task::spawn_blocking(move || {
            let mut file = std::fs::File::open(&path)?;
            // Un fichier qui n'a plus la taille annoncée a changé : jamais envoyé tronqué ni rallongé.
            if file.metadata()?.len() != size {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "le fichier a changé",
                ));
            }
            let len = usize::try_from(chunk_len(size, index))
                .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
            file.seek(SeekFrom::Start(
                index.saturating_mul(u64::from(CHUNK_BYTES)),
            ))?;
            let mut buffer = vec![0u8; len];
            file.read_exact(&mut buffer)?;
            Ok(buffer)
        })
        .await
        .map_err(io::Error::other)?
    }
}

/// Le fichier choisi : un fichier ordinaire, non vide, au nom lisible (l'agent juge le nom, `BR-FILES-005`). `Err` : la raison, lue par l'interface.
fn inspect(path: &PathBuf) -> Result<(String, u64), DepositRefusal> {
    let meta = std::fs::metadata(path).map_err(|_| DepositRefusal::SourceUnreadable)?;
    if !meta.is_file() || meta.len() == 0 {
        return Err(DepositRefusal::SourceUnreadable);
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(DepositRefusal::NameInvalid)?
        .to_owned();
    Ok((name, meta.len()))
}

/// Ouvre le dépôt (`POST /deposits`, acte `FileWrite`) puis lance l'envoi en tâche de fond. `admin_password` : le mot
/// de passe de confirmation selon l'état de l'élévation (jamais gardé).
pub async fn start(
    manager: &LinkManager,
    book: &Arc<TransferBook>,
    id: &ServerId,
    path: &str,
    admin_password: Option<&Secret>,
) -> Result<DepositOutcome, LinkFailure> {
    let path = PathBuf::from(path);
    let (name, size) = match inspect(&path) {
        Ok(found) => found,
        Err(refusal) => return Ok(DepositOutcome::Refused { refusal }),
    };
    let request = DepositRequest {
        name: name.clone(),
        size,
    };
    match manager.open_deposit(id, &request, admin_password).await? {
        ActionOutcome::Completed { status, body, .. } => match interpret(status, &body)? {
            Ok(opened) => {
                let transfer = TransferDto {
                    id: opened.transfer_id.clone(),
                    server_id: id.as_str().to_owned(),
                    name,
                    size: size as f64,
                    sent: 0.0,
                    status: TransferStatusDto::Sending,
                    refusal: None,
                };
                book.insert(Entry {
                    dto: transfer.clone(),
                    path,
                    cancel: Arc::new(AtomicBool::new(false)),
                    running: false,
                });
                spawn(
                    manager.clone(),
                    book.clone(),
                    id.clone(),
                    transfer.id.clone(),
                );
                Ok(DepositOutcome::Started { transfer })
            }
            Err(refusal) => Ok(DepositOutcome::Refused { refusal }),
        },
        ActionOutcome::ResultUnknown { .. } => Ok(DepositOutcome::Unknown),
    }
}

/// Reprend un envoi interrompu : un geste du compte, jamais automatique. Sans effet si le transfert n'est pas
/// interrompu.
pub fn resume(manager: &LinkManager, book: &Arc<TransferBook>, id: &str) {
    let Some(server) = book.with(id, |entry| {
        if entry.running || entry.dto.status != TransferStatusDto::Interrupted {
            return None;
        }
        entry.cancel.store(false, Ordering::SeqCst);
        entry.dto.status = TransferStatusDto::Sending;
        entry.dto.refusal = None;
        ServerId::parse(&entry.dto.server_id).ok()
    }) else {
        return;
    };
    if let Some(server) = server {
        spawn(manager.clone(), book.clone(), server, id.to_owned());
    }
}

/// Annule un envoi : ferme le socket, puis abandonne le dépôt sur le serveur (la partie reçue est retirée).
pub fn cancel(manager: &LinkManager, book: &Arc<TransferBook>, id: &str) {
    let Some((running, server)) = book.with(id, |entry| {
        entry.cancel.store(true, Ordering::SeqCst);
        (entry.running, ServerId::parse(&entry.dto.server_id).ok())
    }) else {
        return;
    };
    if running {
        // La tâche en cours le voit au quart de seconde, ferme le socket, puis abandonne (voir `spawn`).
        return;
    }
    // Rien n'envoie : abandonne tout de suite.
    let (manager, book, id) = (manager.clone(), book.clone(), id.to_owned());
    tauri::async_runtime::spawn(async move {
        if let Some(server) = server {
            let _ = manager.abandon_deposit(&server, &id).await;
        }
        book.fail(&id, TransferStatusDto::Cancelled, None);
    });
}

fn spawn(manager: LinkManager, book: Arc<TransferBook>, server: ServerId, id: String) {
    let Some((path, size, cancel)) = book.with(&id, |entry| {
        entry.running = true;
        (
            entry.path.clone(),
            entry.dto.size as u64,
            entry.cancel.clone(),
        )
    }) else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        let mut source = FileSource { path, size };
        let progress_book = book.clone();
        let progress_id = id.clone();
        let result = manager
            .upload(&server, &id, &mut source, &cancel, &mut move |sent| {
                progress_book.progress(&progress_id, sent);
            })
            .await;
        match result {
            Ok(end) => {
                book.finish(&id, &end);
                if end == UploadEnd::Cancelled {
                    // L'annulation retire la partie reçue du serveur ; un échec ici n'est pas un message.
                    let _ = manager.abandon_deposit(&server, &id).await;
                }
            }
            // Un refus de l'agent à l'ouverture du socket : pas une coupure.
            Err(LinkError::Rejected(_)) => book.fail(
                &id,
                TransferStatusDto::Failed,
                Some(TransferRefusalDto::Rejected),
            ),
            // Le lien n'est pas « Connecté » (ou tout autre échec local) : interrompu, reprise par un geste.
            Err(_) => book.fail(&id, TransferStatusDto::Interrupted, None),
        }
        // Une annulation demandée pendant l'envoi : le carnet le dit même si le serveur n'a rien à retirer.
        if cancel.load(Ordering::SeqCst) {
            book.fail(&id, TransferStatusDto::Cancelled, None);
        }
    });
}

/// La réponse de l'agent à `POST /deposits` : `201` avec le dépôt, ou un refus lu par son code. `Err` : un échec
/// comme pour toute lecture (droit retiré, pas un agent).
pub fn interpret(
    status: u16,
    body: &Value,
) -> Result<Result<DepositOpened, DepositRefusal>, LinkFailure> {
    if (200..300).contains(&status) {
        let opened: DepositOpened =
            serde_json::from_value(body.clone()).map_err(|_| LinkFailure::NotAgent)?;
        return Ok(Ok(opened));
    }
    let Ok(parsed) = serde_json::from_value::<ErrorBody>(body.clone()) else {
        return if status == 403 {
            Err(LinkFailure::Forbidden)
        } else {
            Ok(Err(DepositRefusal::Other))
        };
    };
    if let Some(confirmation) = confirmation_of(&parsed)? {
        return Ok(Err(match confirmation {
            Confirmation::WrongPassword => DepositRefusal::WrongPassword,
            Confirmation::PasswordRequired => DepositRefusal::PasswordRequired,
            Confirmation::TooManyAttempts { retry_after_s } => DepositRefusal::TooManyAttempts {
                wait: f64::from(retry_after_s),
            },
            Confirmation::Busy => DepositRefusal::Busy,
        }));
    }
    let code = parsed.error.code.or_from_status(status);
    // Le droit retiré entre-temps : l'interface le relit et le dit.
    if code == ErrorCode::ForbiddenRole {
        return Err(LinkFailure::forbidden(&parsed.error));
    }
    let number = |field: &str| parsed.error.details[field].as_u64().unwrap_or(0) as f64;
    Ok(Err(match code {
        ErrorCode::NoSpace => DepositRefusal::NoSpace {
            missing: number("missing_bytes"),
            remaining: number("free_after_bytes"),
        },
        ErrorCode::TransferLimit => DepositRefusal::TooMany,
        ErrorCode::ValidationError => match parsed.error.details["field"].as_str() {
            Some("name") => DepositRefusal::NameInvalid,
            _ => DepositRefusal::Other,
        },
        _ => DepositRefusal::Other,
    }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn error(code: &str, details: Value) -> Value {
        json!({ "error": { "code": code, "message": "x", "details": details } })
    }

    fn refused(status: u16, body: Value) -> DepositRefusal {
        match interpret(status, &body).unwrap() {
            Err(refusal) => refusal,
            Ok(opened) => panic!("refus attendu, reçu {opened:?}"),
        }
    }

    #[test]
    fn an_opened_deposit_carries_its_identifier_and_nothing_of_the_file() {
        let opened = interpret(
            201,
            &json!({ "transfer_id": "01J9ZY0G3Q8M2K6W4T7V5N1B9D", "chunk_bytes": 4194304, "window": 8, "expires_at": "2026-11-16T00:00:00Z" }),
        )
        .unwrap()
        .unwrap();
        assert_eq!(opened.transfer_id, "01J9ZY0G3Q8M2K6W4T7V5N1B9D");
        assert!(matches!(
            interpret(201, &json!({ "nope": 1 })),
            Err(LinkFailure::NotAgent)
        ));
    }

    #[test]
    fn each_refusal_is_read_by_its_code() {
        assert_eq!(
            refused(
                507,
                error(
                    "NO_SPACE",
                    json!({ "missing_bytes": 5, "free_after_bytes": 1 })
                )
            ),
            DepositRefusal::NoSpace {
                missing: 5.0,
                remaining: 1.0
            }
        );
        assert_eq!(
            refused(429, error("TRANSFER_LIMIT", json!({ "limit": "open" }))),
            DepositRefusal::TooMany
        );
        assert_eq!(
            refused(
                422,
                error(
                    "VALIDATION_ERROR",
                    json!({ "field": "name", "reason": "not_allowed" })
                )
            ),
            DepositRefusal::NameInvalid
        );
        assert_eq!(
            refused(422, error("VALIDATION_ERROR", json!({ "field": "size" }))),
            DepositRefusal::Other
        );
        assert_eq!(
            refused(500, error("INTERNAL_ERROR", Value::Null)),
            DepositRefusal::Other
        );
        // Un code que ce client ne connaît pas : le sens du statut, jamais un succès.
        assert_eq!(
            refused(409, error("TOUT_A_FAIT_NOUVEAU", Value::Null)),
            DepositRefusal::Other
        );
    }

    #[test]
    fn the_book_never_moves_backwards_and_keeps_what_is_still_open() {
        let book = TransferBook::default();
        let entry = |id: &str, status: TransferStatusDto| Entry {
            dto: TransferDto {
                id: id.into(),
                server_id: "s".into(),
                name: "pack.zip".into(),
                size: 100.0,
                sent: 0.0,
                status,
                refusal: None,
            },
            path: PathBuf::from("pack.zip"),
            cancel: Arc::new(AtomicBool::new(false)),
            running: false,
        };
        book.insert(entry("A", TransferStatusDto::Interrupted));
        book.insert(entry("B", TransferStatusDto::Sending));
        book.progress("A", 40);
        book.progress("A", 10);
        assert_eq!(book.list()[0].sent, 40.0);
        // Un transfert encore ouvert ne se retire pas de la liste ; un transfert fini, si.
        book.dismiss("A");
        assert_eq!(book.list().len(), 2);
        book.finish(
            "A",
            &UploadEnd::Done {
                tree_digest: [0; 32],
            },
        );
        assert_eq!(book.list()[0].sent, 100.0);
        book.dismiss("A");
        assert_eq!(book.list().len(), 1);
    }

    #[test]
    fn a_lack_of_space_leaves_the_transfer_resumable_and_a_rejection_does_not() {
        let book = TransferBook::default();
        book.insert(Entry {
            dto: TransferDto {
                id: "A".into(),
                server_id: "s".into(),
                name: "pack.zip".into(),
                size: 100.0,
                sent: 0.0,
                status: TransferStatusDto::Sending,
                refusal: None,
            },
            path: PathBuf::from("pack.zip"),
            cancel: Arc::new(AtomicBool::new(false)),
            running: true,
        });
        book.finish(
            "A",
            &UploadEnd::Refused(UploadRefusal::NoSpace {
                missing_bytes: 5,
                free_after_bytes: 1,
            }),
        );
        let seen = book.list().remove(0);
        assert_eq!(seen.status, TransferStatusDto::Interrupted);
        assert_eq!(
            seen.refusal,
            Some(TransferRefusalDto::NoSpace {
                missing: 5.0,
                remaining: 1.0
            })
        );
        book.finish("A", &UploadEnd::Refused(UploadRefusal::Rejected));
        assert_eq!(book.list()[0].status, TransferStatusDto::Failed);
    }

    #[test]
    fn a_directory_an_empty_file_and_a_missing_file_are_never_sent() {
        let dir = std::env::temp_dir();
        assert_eq!(
            inspect(&dir).err(),
            Some(DepositRefusal::SourceUnreadable),
            "un dossier"
        );
        assert_eq!(
            inspect(&dir.join("hearth-n-existe-pas-0a1b2c")).err(),
            Some(DepositRefusal::SourceUnreadable)
        );
    }
}
