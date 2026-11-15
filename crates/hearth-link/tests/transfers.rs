//! Envoyer un fichier dans la zone d'attente de l'agent, vu par la liaison, contre un VRAI agent (TLS 1.3, SQLite, vrai
//! WebSocket, fausse zone d'attente) : le dépôt s'ouvre par un acte confirmé, les morceaux de 4 Mio partent avec leur
//! empreinte, l'envoi se reprend là où l'agent l'annonce et sans renvoyer ce qu'il a reçu, annuler ferme le socket sans
//! rien retirer, abandonner retire la partie reçue (HRT-96, HRT-95, `BR-FILES-019`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

#[path = "../../hearth-agent/tests/support/fake_transfer_files.rs"]
mod fake_transfer_files;

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use hearth_link::domain::secret::Secret;
use hearth_link::{ActionOutcome, UploadEnd, UploadRefusal, UploadSource};
use hearth_proto::api::transfers::{
    CHUNK_BYTES, DepositOpened, DepositRequest, chunk_digest, chunk_len, digest_of,
};
use support::agent::fake_engine::FakeEngine;
use support::{Options, PASSWORD, World};

use fake_transfer_files::FakeTransferFiles;

/// Des octets déterministes, différents d'un morceau à l'autre.
fn data(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|i| u8::try_from(i % 251).unwrap().wrapping_add(seed))
        .collect()
}

fn tree_of(bytes: &[u8]) -> [u8; 32] {
    let digests: Vec<_> = bytes.chunks(CHUNK_BYTES as usize).map(digest_of).collect();
    chunk_digest(&digests)
}

/// Un fichier en mémoire, lu un morceau à la fois (comme la coquille lit le fichier local).
struct MemSource {
    bytes: Vec<u8>,
    /// Les morceaux lus, dans l'ordre : pour prouver qu'une reprise ne les relit que pour l'empreinte.
    reads: Vec<u64>,
    /// Le fichier « a changé » : chaque morceau lu est tronqué d'un octet.
    shrunk: bool,
}

impl MemSource {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            reads: Vec::new(),
            shrunk: false,
        }
    }
}

#[async_trait]
impl UploadSource for MemSource {
    fn size(&self) -> u64 {
        self.bytes.len() as u64
    }

    async fn read_chunk(&mut self, index: u64) -> io::Result<Vec<u8>> {
        self.reads.push(index);
        let size = self.bytes.len() as u64;
        let start = usize::try_from(index * u64::from(CHUNK_BYTES)).unwrap();
        let mut end = start + usize::try_from(chunk_len(size, index)).unwrap();
        if self.shrunk {
            end -= 1;
        }
        Ok(self.bytes[start..end].to_vec())
    }
}

struct Bench {
    world: World,
    files: Arc<FakeTransferFiles>,
}

async fn bench() -> Bench {
    let files = Arc::new(FakeTransferFiles::new());
    let world = World::connected(Options {
        device_key: true,
        engine: Some(Arc::new(FakeEngine::new())),
        transfers: Some(files.clone()),
        ..Options::default()
    })
    .await;
    world.agent.grant_services("marie").await;
    Bench { world, files }
}

impl Bench {
    async fn open(&self, size: u64) -> DepositOpened {
        let request = DepositRequest {
            name: "pack.zip".into(),
            size,
        };
        let outcome = self
            .world
            .manager
            .open_deposit(&self.world.id, &request, Some(&Secret::from(PASSWORD)))
            .await
            .expect("acte");
        let ActionOutcome::Completed { status, body, .. } = outcome else {
            panic!("réponse attendue");
        };
        assert_eq!(status, 201, "{body}");
        serde_json::from_value(body).expect("DepositOpened")
    }

    async fn upload(
        &self,
        transfer_id: &str,
        source: &mut MemSource,
        cancel: &AtomicBool,
        mut progress: impl FnMut(u64) + Send,
    ) -> UploadEnd {
        self.world
            .manager
            .upload(&self.world.id, transfer_id, source, cancel, &mut progress)
            .await
            .expect("envoi")
    }
}

fn appends(files: &FakeTransferFiles, id: &str, index: u64) -> usize {
    let wanted = format!("append {id} {index}");
    files.writes().iter().filter(|w| **w == wanted).count()
}

#[tokio::test]
async fn a_file_of_three_chunks_goes_through_an_acted_deposit_and_arrives_verified() {
    let b = bench().await;
    let bytes = data(2 * CHUNK_BYTES as usize + 777, 4);
    let opened = b.open(bytes.len() as u64).await;
    let mut source = MemSource::new(bytes.clone());
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let end = {
        let cancel = AtomicBool::new(false);
        let sink = seen.clone();
        b.upload(&opened.transfer_id, &mut source, &cancel, move |sent| {
            sink.lock().unwrap().push(sent);
        })
        .await
    };
    let seen = seen.lock().unwrap().clone();
    assert_eq!(
        end,
        UploadEnd::Done {
            tree_digest: tree_of(&bytes)
        }
    );
    // La progression ne recule jamais et finit à la taille du fichier.
    assert!(seen.windows(2).all(|pair| pair[0] <= pair[1]), "{seen:?}");
    assert_eq!(seen.last().copied(), Some(bytes.len() as u64));
    // Le fichier est arrivé tel quel, sous son nom définitif, et chaque morceau n'a été lu qu'une fois.
    let name = format!("{}.bin", opened.transfer_id);
    assert_eq!(b.files.bytes_of(&name).unwrap(), bytes);
    assert_eq!(source.reads, vec![0, 1, 2]);
}

#[tokio::test]
async fn a_cancelled_upload_resumes_where_the_agent_says_without_resending_what_it_has() {
    let b = bench().await;
    let bytes = data(3 * CHUNK_BYTES as usize + 5, 9);
    let opened = b.open(bytes.len() as u64).await;
    let mut source = MemSource::new(bytes.clone());
    // L'envoi est coupé dès le premier accusé : annuler ferme le socket, rien n'est retiré du serveur.
    let cancel = AtomicBool::new(false);
    let first = b
        .upload(&opened.transfer_id, &mut source, &cancel, |sent| {
            if sent > 0 {
                cancel.store(true, Ordering::SeqCst);
            }
        })
        .await;
    assert_eq!(first, UploadEnd::Cancelled);
    assert!(
        b.files.exists(&format!("{}.part", opened.transfer_id)),
        "la partie reçue reste (reprise possible 24 heures)"
    );
    // La reprise est un nouvel appel : l'agent annonce son offset, les morceaux reçus ne repartent pas.
    let mut again = MemSource::new(bytes.clone());
    let end = b
        .upload(
            &opened.transfer_id,
            &mut again,
            &AtomicBool::new(false),
            |_| {},
        )
        .await;
    assert_eq!(
        end,
        UploadEnd::Done {
            tree_digest: tree_of(&bytes)
        }
    );
    let name = format!("{}.bin", opened.transfer_id);
    assert_eq!(b.files.bytes_of(&name).unwrap(), bytes);
    // Chaque morceau est écrit UNE fois côté agent, malgré les deux envois.
    for index in 0..4 {
        assert_eq!(appends(&b.files, &opened.transfer_id, index), 1, "{index}");
    }
}

#[tokio::test]
async fn a_file_that_changed_size_is_never_sent_truncated() {
    let b = bench().await;
    let bytes = data(CHUNK_BYTES as usize + 3, 1);
    let opened = b.open(bytes.len() as u64).await;
    let mut source = MemSource::new(bytes);
    source.shrunk = true;
    let end = b
        .upload(
            &opened.transfer_id,
            &mut source,
            &AtomicBool::new(false),
            |_| {},
        )
        .await;
    assert_eq!(end, UploadEnd::Refused(UploadRefusal::SourceChanged));
    assert!(
        b.files.writes().iter().all(|w| !w.starts_with("append")),
        "rien n'est parti"
    );
}

#[tokio::test]
async fn abandoning_a_deposit_removes_the_received_part_and_a_second_time_removes_nothing() {
    let b = bench().await;
    let bytes = data(CHUNK_BYTES as usize + 3, 6);
    let opened = b.open(bytes.len() as u64).await;
    let mut source = MemSource::new(bytes);
    let cancel = AtomicBool::new(false);
    let end = b
        .upload(&opened.transfer_id, &mut source, &cancel, |sent| {
            if sent > 0 {
                cancel.store(true, Ordering::SeqCst);
            }
        })
        .await;
    assert_eq!(end, UploadEnd::Cancelled);
    let gone = b
        .world
        .manager
        .abandon_deposit(&b.world.id, &opened.transfer_id)
        .await
        .expect("abandon");
    assert!(gone.removed);
    assert!(b.files.names().is_empty(), "{:?}", b.files.names());
    let again = b
        .world
        .manager
        .abandon_deposit(&b.world.id, &opened.transfer_id)
        .await
        .expect("abandon");
    assert!(!again.removed);
}

#[tokio::test]
async fn a_deposit_that_does_not_fit_is_refused_by_the_agent_and_a_bad_id_never_leaves() {
    let b = bench().await;
    b.files
        .set_space(hearth_agent::application::ports::DiskSpace {
            free: 10 << 30,
            total: 100 << 30,
        });
    let request = DepositRequest {
        name: "pack.zip".into(),
        size: 6 << 30,
    };
    let outcome = b
        .world
        .manager
        .open_deposit(&b.world.id, &request, Some(&Secret::from(PASSWORD)))
        .await
        .expect("acte");
    let ActionOutcome::Completed { status, body, .. } = outcome else {
        panic!("réponse attendue");
    };
    assert_eq!(status, 507, "{body}");
    assert_eq!(body["error"]["details"]["missing_bytes"], 1_u64 << 30);
    // Un identifiant qui n'est pas un ULID n'est jamais mis dans un chemin.
    assert!(
        b.world
            .manager
            .abandon_deposit(&b.world.id, "../x")
            .await
            .is_err()
    );
    let mut source = MemSource::new(data(10, 1));
    assert!(
        b.world
            .manager
            .upload(
                &b.world.id,
                "../x",
                &mut source,
                &AtomicBool::new(false),
                &mut |_| {}
            )
            .await
            .is_err()
    );
}
