//! Les transferts reprenables, de bout en bout (HRT-95, ADR-0048, BR-FILES-013, BR-FILES-019, BR-MOD-015) : vrai
//! agent en HTTPS, vraie base, vrai WebSocket, fausse zone d'attente. Un fichier de plusieurs morceaux de 4 Mio
//! arrive dans la zone d'attente, vérifié morceau par morceau, repris après une coupure sans relire ce qui est
//! déjà reçu, et n'est JAMAIS visible sous son nom définitif tant qu'il est incomplet. Le parcours réel du disque
//! (inodes, liens, renommage sans remplacement) est dans `infrastructure/transfer_files.rs` (Linux).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use hearth_agent::application::transfers::{DepositZone, TransferError};
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::transfers::TransferId;
use hearth_agent::entrypoint::ws::StreamSettings;
use hearth_proto::api::transfers::{
    CHUNK_BYTES, DepositOpened, DepositState, DepositView, TransferAbandoned, TransferErrorCode,
    UploadClientMessage, UploadServerMessage, WINDOW_CHUNKS, chunk_count, chunk_digest, chunk_len,
    digest_of, encode_frame, hex_of,
};
use serde_json::{Value, json};
use support::device::Actor;
use support::engine::{fast_timings, parts_supervised};
use support::fake_engine::FakeEngine;
use support::fake_supervisor::FakeSupervisor;
use support::fake_transfer_files::FakeTransferFiles;
use support::https::{self, Agent, Wire};
use support::probe::fast_stream;
use support::ws::{End, WsClient, open_transfer};
use support::{Env, env};

/// Un nom de fichier : il ne doit apparaître NULLE PART hors de la requête (ni base, ni journal, ni réponse).
const NAME: &str = "sentinelle-nom-fichier-77ac.zip";

struct Bench {
    env: Env,
    files: Arc<FakeTransferFiles>,
    agent: Agent,
    /// Administrateur avec le droit de gérer les services.
    ada: Actor,
    /// Administrateur SANS le droit.
    bob: Actor,
    /// Lecture seule AVEC le droit.
    dan: Actor,
}

async fn bench() -> Bench {
    bench_with(|_| {}).await
}

async fn bench_with(change: impl FnOnce(&mut hearth_agent::app::EngineParts)) -> Bench {
    bench_over(change, fast_stream()).await
}

/// Comme `bench`, avec ces délais de flux.
async fn bench_streamed(stream: StreamSettings) -> Bench {
    bench_over(|_| {}, stream).await
}

async fn bench_over(
    change: impl FnOnce(&mut hearth_agent::app::EngineParts),
    stream: StreamSettings,
) -> Bench {
    let env = env().await;
    let files = Arc::new(FakeTransferFiles::new());
    let mut parts = parts_supervised(
        Arc::new(FakeEngine::new()),
        Arc::new(FakeSupervisor::new()),
        fast_timings(),
    );
    parts.transfers.files = Some(files.clone());
    change(&mut parts);
    let agent =
        https::start_engined_metered(&env, parts, support::probe::metering_with(stream)).await;
    let ada = agent.actor(&env, "ada", Role::Admin).await;
    let bob = agent.actor(&env, "bob", Role::Admin).await;
    let dan = agent.actor(&env, "dan", Role::ReadOnly).await;
    env.grant_services("ada").await;
    env.grant_services("dan").await;
    Bench {
        env,
        files,
        agent,
        ada,
        bob,
        dan,
    }
}

/// Des octets déterministes, différents d'un morceau à l'autre.
fn data(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|i| u8::try_from(i % 251).unwrap().wrapping_add(seed))
        .collect()
}

fn chunks(bytes: &[u8]) -> Vec<&[u8]> {
    bytes.chunks(CHUNK_BYTES as usize).collect()
}

fn tree_of(bytes: &[u8]) -> [u8; 32] {
    let digests: Vec<_> = chunks(bytes).iter().map(|c| digest_of(c)).collect();
    chunk_digest(&digests)
}

fn code(wire: &Wire) -> String {
    wire.body["error"]["code"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

impl Bench {
    async fn open_as(&self, who: &Actor, name: &str, size: u64) -> Wire {
        self.agent
            .confirmed(
                who,
                "POST",
                "/deposits",
                json!({ "name": name, "size": size }),
            )
            .await
            .send()
            .await
    }

    /// Ouvre un dépôt (ada) et rend son identifiant.
    async fn deposit(&self, size: u64) -> String {
        let reply = self.open_as(&self.ada, NAME, size).await;
        assert_eq!(reply.status, 201, "{}", reply.text);
        let opened: DepositOpened = serde_json::from_value(reply.body).expect("DepositOpened");
        assert_eq!(opened.chunk_bytes, CHUNK_BYTES);
        assert_eq!(opened.window, WINDOW_CHUNKS);
        opened.transfer_id
    }

    /// Attache un socket : `auth`, `open`, puis rend le client et la première réponse.
    async fn attach_as(&self, who: &Actor, id: &str) -> (WsClient, UploadServerMessage) {
        let mut socket = open_transfer(&self.agent, id).await.expect("socket ouvert");
        socket.auth(&who.token).await;
        socket.send_value(&UploadClientMessage::Open {}).await;
        let first = socket.next_upload().await.expect("réponse");
        (socket, first)
    }

    async fn attach(&self, id: &str) -> (WsClient, u64) {
        let (socket, first) = self.attach_as(&self.ada, id).await;
        let UploadServerMessage::Ready { offset, window } = first else {
            panic!("`ready` attendu : {first:?}");
        };
        assert_eq!(window, WINDOW_CHUNKS);
        (socket, offset)
    }

    async fn view(&self, who: &Actor, id: &str) -> Wire {
        self.agent
            .request("GET", &format!("/deposits/{id}"))
            .token(&who.token)
            .send()
            .await
    }

    async fn abandon(&self, who: &Actor, id: &str) -> Wire {
        self.agent
            .request("DELETE", &format!("/transfers/{id}"))
            .token(&who.token)
            .send()
            .await
    }

    /// L'identifiant technique d'un compte, tel que le registre des transferts le range.
    async fn account(&self, username: &str) -> String {
        let (id,): (String,) = sqlx::query_as("SELECT id FROM accounts WHERE username = ?")
            .bind(username)
            .fetch_one(self.env.db.pool())
            .await
            .unwrap();
        id
    }

    /// Les entrées du journal d'activité de l'action : résultat, cible, compte.
    async fn journal(&self, action: &str) -> Vec<(String, Option<String>, Option<String>)> {
        self.agent.flush_journal().await;
        sqlx::query_as(
            "SELECT outcome, target, account FROM audit_events WHERE action = ? ORDER BY id",
        )
        .bind(action)
        .fetch_all(self.env.db.pool())
        .await
        .unwrap()
    }

    /// Tout ce que la base et le journal gardent, en un seul texte : le nom du fichier n'y est pas.
    async fn everything_stored(&self) -> String {
        self.agent.flush_journal().await;
        let transfers: Vec<(String, String, String, String)> =
            sqlx::query_as("SELECT id, account_id, state, tree_digest FROM transfers")
                .fetch_all(self.env.db.pool())
                .await
                .unwrap();
        let audit: Vec<(String, Option<String>, Option<String>)> =
            sqlx::query_as("SELECT action, target, reason FROM audit_events")
                .fetch_all(self.env.db.pool())
                .await
                .unwrap();
        format!("{transfers:?} {audit:?}")
    }
}

/// Envoie les morceaux `range` et lit chaque accusé.
async fn send_chunks(
    socket: &mut WsClient,
    bytes: &[u8],
    range: std::ops::Range<usize>,
) -> Vec<u64> {
    let mut offsets = Vec::new();
    for (index, chunk) in chunks(bytes).into_iter().enumerate() {
        if !range.contains(&index) {
            continue;
        }
        let frame = encode_frame(index as u64, &digest_of(chunk), chunk);
        socket.send_binary(&frame).await;
        match socket.next_upload().await.expect("accusé") {
            UploadServerMessage::Ack { offset, window } => {
                assert_eq!(window, WINDOW_CHUNKS);
                offsets.push(offset);
            }
            other => panic!("`ack` attendu : {other:?}"),
        }
    }
    offsets
}

async fn finish(socket: &mut WsClient, tree: [u8; 32]) -> UploadServerMessage {
    socket
        .send_value(&UploadClientMessage::Finish {
            tree_digest: hex_of(&tree),
        })
        .await;
    socket.next_upload().await.expect("fin")
}

fn error_code(message: &UploadServerMessage) -> TransferErrorCode {
    match message {
        UploadServerMessage::Error(error) => error.code,
        other => panic!("une erreur est attendue : {other:?}"),
    }
}

fn complete_name(id: &str) -> String {
    TransferId::parse(id).unwrap().complete_name()
}

fn part_name(id: &str) -> String {
    TransferId::parse(id).unwrap().part_name()
}

fn appends(files: &FakeTransferFiles, id: &str, index: u64) -> usize {
    let wanted = format!("append {id} {index}");
    files.writes().iter().filter(|w| **w == wanted).count()
}

// ---------------------------------------------------------------------------------------------
// Ouvrir un dépôt : le droit, la demande, l'espace, les bornes
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn opening_a_deposit_needs_the_right_to_manage_services() {
    let b = bench().await;
    // Un administrateur SANS le droit.
    let reply = b.open_as(&b.bob, NAME, 1000).await;
    assert_eq!(reply.status, 403, "{}", reply.text);
    // Un compte en lecture seule AVEC le droit passe (le droit est indépendant du rôle, ADR-0041).
    let reply = b.open_as(&b.dan, NAME, 1000).await;
    assert_eq!(reply.status, 201, "{}", reply.text);
    // Rien n'a été créé pour le refusé.
    assert_eq!(
        b.files.names().len(),
        2,
        "une partie et sa liste, celles de dan"
    );
}

#[tokio::test]
async fn an_invalid_name_or_size_is_refused_without_echoing_the_name() {
    let b = bench().await;
    for (name, size, field) in [
        ("", 10, "name"),
        ("dossier/pack.zip", 10, "name"),
        ("..", 10, "name"),
        (NAME, 0, "size"),
    ] {
        let reply = b.open_as(&b.ada, name, size).await;
        assert_eq!(reply.status, 422, "{name:?} {size} : {}", reply.text);
        assert_eq!(reply.body["error"]["details"]["field"], field);
        assert!(!reply.text.contains("pack.zip"), "{}", reply.text);
    }
    assert!(
        b.files.names().is_empty(),
        "rien n'est créé pour une demande refusée"
    );
}

#[tokio::test]
async fn a_file_that_does_not_fit_is_refused_before_anything_is_created() {
    let b = bench().await;
    b.files
        .set_space(hearth_agent::application::ports::DiskSpace {
            free: 10 * (1 << 30),
            total: 100 * (1 << 30),
        });
    // Il reste 10 Gio, la réserve est de 5 Gio : 6 Gio manquent 1 Gio (BR-FILES-013).
    let reply = b.open_as(&b.ada, NAME, 6 * (1 << 30)).await;
    assert_eq!(reply.status, 507, "{}", reply.text);
    assert_eq!(code(&reply), "NO_SPACE");
    assert_eq!(reply.body["error"]["details"]["missing_bytes"], 1_u64 << 30);
    assert_eq!(
        reply.body["error"]["details"]["free_after_bytes"],
        4_u64 * (1 << 30)
    );
    assert!(b.files.names().is_empty());
    // Ce qui tient passe.
    assert_eq!(b.open_as(&b.ada, NAME, 4 * (1 << 30)).await.status, 201);
}

#[tokio::test]
async fn a_root_that_is_not_usable_refuses_the_deposit() {
    let b = bench().await;
    b.files.refuse_root();
    let reply = b.open_as(&b.ada, NAME, 10).await;
    assert_eq!(reply.status, 422, "{}", reply.text);
    assert_eq!(
        reply.body["error"]["details"]["reason"],
        "transfers_root_unavailable"
    );
}

#[tokio::test]
async fn an_account_holds_at_most_eight_open_deposits() {
    let b = bench().await;
    for _ in 0..8 {
        b.deposit(10).await;
    }
    let reply = b.open_as(&b.ada, NAME, 10).await;
    assert_eq!(reply.status, 429, "{}", reply.text);
    assert_eq!(code(&reply), "TRANSFER_LIMIT");
    assert_eq!(reply.body["error"]["details"]["limit"], "open");
    // Un autre compte a ses propres places.
    assert_eq!(b.open_as(&b.dan, NAME, 10).await.status, 201);
}

// ---------------------------------------------------------------------------------------------
// Envoyer : morceaux vérifiés, fichier jamais visible sous son nom tant qu'il est incomplet
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_file_of_several_chunks_arrives_verified_under_its_final_name_at_once() {
    let b = bench().await;
    let bytes = data(2 * CHUNK_BYTES as usize + 12_345, 3);
    let id = b.deposit(bytes.len() as u64).await;
    let (mut socket, offset) = b.attach(&id).await;
    assert_eq!(offset, 0);
    let offsets = send_chunks(&mut socket, &bytes, 0..2).await;
    assert_eq!(
        offsets,
        vec![u64::from(CHUNK_BYTES), 2 * u64::from(CHUNK_BYTES)]
    );
    // Incomplet : une partie, jamais le nom définitif (BR-FILES-019).
    assert!(b.files.exists(&part_name(&id)));
    assert!(!b.files.exists(&complete_name(&id)));
    let seen: DepositView = serde_json::from_value(b.view(&b.ada, &id).await.body).unwrap();
    assert_eq!(seen.state, DepositState::Receiving);
    assert_eq!(seen.received_bytes, 2 * u64::from(CHUNK_BYTES));
    send_chunks(&mut socket, &bytes, 2..3).await;
    assert!(!b.files.exists(&complete_name(&id)), "pas avant `finish`");
    let tree = tree_of(&bytes);
    let UploadServerMessage::Done {
        size,
        verified,
        tree_digest,
    } = finish(&mut socket, tree).await
    else {
        panic!("`done` attendu");
    };
    assert_eq!((size, verified), (bytes.len() as u64, true));
    assert_eq!(tree_digest, hex_of(&tree));
    // Posé d'un seul geste : plus de partie, le fichier complet est identique octet pour octet.
    assert!(!b.files.exists(&part_name(&id)));
    assert_eq!(b.files.bytes_of(&complete_name(&id)).unwrap(), bytes);
    let seen: DepositView = serde_json::from_value(b.view(&b.ada, &id).await.body).unwrap();
    assert_eq!(seen.state, DepositState::Complete);
    assert_eq!(seen.tree_digest, Some(hex_of(&tree)));
    // Chaque morceau est écrit une fois, jamais plus gros qu'un morceau.
    for index in 0..3 {
        assert_eq!(appends(&b.files, &id, index), 1, "morceau {index}");
    }
    assert!(b.files.biggest_write() <= CHUNK_BYTES as usize);
    // L'ouverture puis la FIN sont consignées sous `service.file.write`, sans nom de fichier ni chemin.
    let journal = b.journal("service.file.write").await;
    assert_eq!(journal.len(), 2, "{journal:?}");
    assert!(
        journal
            .iter()
            .all(|(outcome, _, account)| { outcome == "ok" && account.as_deref() == Some("ada") })
    );
    assert_eq!(journal[0].1.as_deref(), Some("/deposits"));
    assert_eq!(journal[1].1.as_deref(), Some("/transfers/{id}"));
    assert!(!b.everything_stored().await.contains(NAME));
    // Un dépôt complet qu'on rouvre répond `done` tout de suite.
    let (_second, first) = b.attach_as(&b.ada, &id).await;
    assert!(
        matches!(first, UploadServerMessage::Done { verified: true, .. }),
        "{first:?}"
    );
}

#[tokio::test]
async fn an_interrupted_upload_resumes_without_rereading_what_was_received() {
    let b = bench().await;
    let bytes = data(3 * CHUNK_BYTES as usize + 7, 9);
    let id = b.deposit(bytes.len() as u64).await;
    let (mut socket, _) = b.attach(&id).await;
    send_chunks(&mut socket, &bytes, 0..2).await;
    // Le lien coupe, sans trame de fermeture.
    socket.drop_connection();
    // La reprise est demandée : l'agent annonce son offset durable.
    let (mut socket, offset) = b.attach(&id).await;
    assert_eq!(offset, 2 * u64::from(CHUNK_BYTES));
    send_chunks(&mut socket, &bytes, 2..4).await;
    let UploadServerMessage::Done { verified, .. } = finish(&mut socket, tree_of(&bytes)).await
    else {
        panic!("`done` attendu");
    };
    assert!(verified);
    assert_eq!(b.files.bytes_of(&complete_name(&id)).unwrap(), bytes);
    // Les deux premiers morceaux n'ont été écrits qu'UNE fois : la reprise ne les relit ni ne les réécrit.
    for index in 0..4 {
        assert_eq!(appends(&b.files, &id, index), 1, "morceau {index}");
    }
}

#[tokio::test]
async fn a_chunk_half_written_by_a_stop_is_overwritten_on_resume() {
    let b = bench().await;
    let bytes = data(2 * CHUNK_BYTES as usize, 5);
    let id = b.deposit(bytes.len() as u64).await;
    let (mut socket, _) = b.attach(&id).await;
    send_chunks(&mut socket, &bytes, 0..1).await;
    socket.drop_connection();
    // L'agent s'est arrêté au milieu du morceau suivant : des octets de trop, sans empreinte.
    b.files.append_garbage(&part_name(&id), &[0xEE; 1000]);
    let (mut socket, offset) = b.attach(&id).await;
    assert_eq!(offset, u64::from(CHUNK_BYTES));
    send_chunks(&mut socket, &bytes, 1..2).await;
    assert!(matches!(
        finish(&mut socket, tree_of(&bytes)).await,
        UploadServerMessage::Done { .. }
    ));
    assert_eq!(b.files.bytes_of(&complete_name(&id)).unwrap(), bytes);
}

#[tokio::test]
async fn a_part_that_lost_acknowledged_bytes_resumes_from_what_is_really_on_disk() {
    let b = bench().await;
    let bytes = data(3 * CHUNK_BYTES as usize, 7);
    let id = b.deposit(bytes.len() as u64).await;
    let (mut socket, _) = b.attach(&id).await;
    send_chunks(&mut socket, &bytes, 0..2).await;
    socket.drop_connection();
    // Le disque a perdu la fin du deuxième morceau : il n'est plus compté.
    b.files
        .truncate(&part_name(&id), u64::from(CHUNK_BYTES) + 10);
    let (mut socket, offset) = b.attach(&id).await;
    assert_eq!(
        offset,
        u64::from(CHUNK_BYTES),
        "seul le premier morceau est réellement là"
    );
    send_chunks(&mut socket, &bytes, 1..3).await;
    assert!(matches!(
        finish(&mut socket, tree_of(&bytes)).await,
        UploadServerMessage::Done { .. }
    ));
    assert_eq!(b.files.bytes_of(&complete_name(&id)).unwrap(), bytes);
}

#[tokio::test]
async fn a_corrupt_chunk_is_refused_and_never_written() {
    let b = bench().await;
    let bytes = data(2 * CHUNK_BYTES as usize, 1);
    let id = b.deposit(bytes.len() as u64).await;
    let (mut socket, _) = b.attach(&id).await;
    send_chunks(&mut socket, &bytes, 0..1).await;
    // L'empreinte annoncée n'est pas celle des octets.
    let chunk = chunks(&bytes)[1];
    let frame = encode_frame(1, &digest_of(b"autre chose"), chunk);
    socket.send_binary(&frame).await;
    let refused = socket.next_upload().await.expect("erreur");
    assert_eq!(error_code(&refused), TransferErrorCode::ChunkCorrupt);
    let UploadServerMessage::Error(error) = refused else {
        unreachable!()
    };
    assert_eq!(error.expected_index, Some(1));
    assert_eq!(appends(&b.files, &id, 1), 0, "rien n'est écrit");
    // La reprise renvoie CE morceau.
    let (mut socket, offset) = b.attach(&id).await;
    assert_eq!(offset, u64::from(CHUNK_BYTES));
    send_chunks(&mut socket, &bytes, 1..2).await;
    assert!(matches!(
        finish(&mut socket, tree_of(&bytes)).await,
        UploadServerMessage::Done { .. }
    ));
}

#[tokio::test]
async fn a_chunk_out_of_order_or_of_the_wrong_size_is_refused() {
    let b = bench().await;
    let bytes = data(2 * CHUNK_BYTES as usize, 2);
    let id = b.deposit(bytes.len() as u64).await;
    let (mut socket, _) = b.attach(&id).await;
    // Le deuxième morceau avant le premier.
    let chunk = chunks(&bytes)[1];
    socket
        .send_binary(&encode_frame(1, &digest_of(chunk), chunk))
        .await;
    let refused = socket.next_upload().await.expect("erreur");
    assert_eq!(error_code(&refused), TransferErrorCode::ChunkSequence);
    // Le premier morceau, trop court.
    let (mut socket, _) = b.attach(&id).await;
    let short = &chunks(&bytes)[0][..100];
    socket
        .send_binary(&encode_frame(0, &digest_of(short), short))
        .await;
    let refused = socket.next_upload().await.expect("erreur");
    assert_eq!(error_code(&refused), TransferErrorCode::ChunkSequence);
    assert!(
        b.files.writes().iter().all(|w| !w.starts_with("append")),
        "rien n'est écrit"
    );
}

#[tokio::test]
async fn a_frame_bigger_than_a_chunk_closes_the_socket() {
    let b = bench().await;
    let bytes = data(CHUNK_BYTES as usize, 2);
    let id = b.deposit(bytes.len() as u64).await;
    let (mut socket, _) = b.attach(&id).await;
    // Quatre kio de plus que la borne : la trame est refusée par le socket lui-même.
    // Le serveur peut couper pendant l'envoi : l'échec de l'envoi est une réponse comme une autre.
    socket
        .send_binary_lossy(&vec![0u8; CHUNK_BYTES as usize + 4096])
        .await;
    loop {
        match socket.try_next_upload(Duration::from_secs(5)).await {
            Some(Err(End::Closed(_) | End::Dropped)) => break,
            Some(Ok(UploadServerMessage::Error(_))) => {}
            other => panic!("le socket devait se fermer : {other:?}"),
        }
    }
    assert!(b.files.writes().iter().all(|w| !w.starts_with("append")));
}

#[tokio::test]
async fn the_end_needs_every_chunk_and_the_same_tree_digest() {
    let b = bench().await;
    let bytes = data(2 * CHUNK_BYTES as usize, 4);
    let id = b.deposit(bytes.len() as u64).await;
    let (mut socket, _) = b.attach(&id).await;
    send_chunks(&mut socket, &bytes, 0..1).await;
    // Il manque un morceau.
    let refused = finish(&mut socket, tree_of(&bytes)).await;
    assert_eq!(error_code(&refused), TransferErrorCode::TreeMismatch);
    // Tout est là, mais l'empreinte d'ensemble n'est pas la bonne.
    let (mut socket, _) = b.attach(&id).await;
    send_chunks(&mut socket, &bytes, 1..2).await;
    let refused = finish(&mut socket, [7; 32]).await;
    assert_eq!(error_code(&refused), TransferErrorCode::TreeMismatch);
    assert!(
        !b.files.exists(&complete_name(&id)),
        "jamais publié sur une empreinte fausse"
    );
    // La bonne passe.
    let (mut socket, offset) = b.attach(&id).await;
    assert_eq!(offset, bytes.len() as u64);
    assert!(matches!(
        finish(&mut socket, tree_of(&bytes)).await,
        UploadServerMessage::Done { .. }
    ));
}

#[tokio::test]
async fn the_disk_filling_up_mid_upload_stops_the_next_chunk() {
    let b = bench().await;
    let bytes = data(2 * CHUNK_BYTES as usize, 6);
    let id = b.deposit(bytes.len() as u64).await;
    let (mut socket, _) = b.attach(&id).await;
    // La première mesure (celle du premier morceau) passe, ensuite le disque est plein.
    b.files.fill_after(
        1,
        hearth_agent::application::ports::DiskSpace {
            free: 2 << 30,
            total: 100 << 30,
        },
    );
    send_chunks(&mut socket, &bytes, 0..1).await;
    let chunk = chunks(&bytes)[1];
    socket
        .send_binary(&encode_frame(1, &digest_of(chunk), chunk))
        .await;
    let refused = socket.next_upload().await.expect("erreur");
    let UploadServerMessage::Error(error) = refused else {
        panic!("erreur attendue");
    };
    assert_eq!(error.code, TransferErrorCode::NoSpace);
    assert!(error.missing_bytes.unwrap() > 0);
    assert!(error.free_after_bytes.is_some());
    assert_eq!(
        appends(&b.files, &id, 1),
        0,
        "le morceau refusé n'est pas écrit"
    );
}

#[tokio::test]
async fn a_write_failure_is_a_typed_error_never_a_system_text() {
    let b = bench().await;
    let bytes = data(CHUNK_BYTES as usize, 8);
    let id = b.deposit(bytes.len() as u64).await;
    let (mut socket, _) = b.attach(&id).await;
    b.files.fail_writes(true);
    let chunk = chunks(&bytes)[0];
    socket
        .send_binary(&encode_frame(0, &digest_of(chunk), chunk))
        .await;
    let refused = socket.next_upload().await.expect("erreur");
    assert_eq!(error_code(&refused), TransferErrorCode::WriteFailed);
    // Le disque revient : la reprise repart de zéro, sans rien d'écrit.
    b.files.fail_writes(false);
    let (mut socket, offset) = b.attach(&id).await;
    assert_eq!(offset, 0);
    send_chunks(&mut socket, &bytes, 0..1).await;
}

// ---------------------------------------------------------------------------------------------
// Qui peut quoi : un dépôt est à son compte, un socket n'est pas ouvert sans jeton
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn another_account_never_sees_attaches_or_abandons_a_deposit() {
    let b = bench().await;
    let bytes = data(CHUNK_BYTES as usize, 1);
    let id = b.deposit(bytes.len() as u64).await;
    // Dan a le droit mais ce dépôt n'est pas à lui : il n'existe pas pour lui.
    assert_eq!(b.view(&b.dan, &id).await.status, 404);
    let (mut socket, first) = b.attach_as(&b.dan, &id).await;
    assert_eq!(error_code(&first), TransferErrorCode::TransferNotFound);
    assert!(matches!(
        socket.next_upload().await,
        Err(End::Closed(_) | End::Dropped)
    ));
    let reply = b.abandon(&b.dan, &id).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let gone: TransferAbandoned = serde_json::from_value(reply.body).unwrap();
    assert!(!gone.removed, "rien n'a été retiré");
    assert!(b.files.exists(&part_name(&id)), "le dépôt d'ada est intact");
    // Un administrateur sans le droit est refusé par la couche d'accès.
    assert_eq!(b.view(&b.bob, &id).await.status, 403);
    assert_eq!(b.abandon(&b.bob, &id).await.status, 403);
    assert!(b.files.exists(&part_name(&id)));
    // Le consommateur non plus ne prend pas le dépôt d'un autre compte.
    let parsed = TransferId::parse(&id).unwrap();
    let transfers = b.agent.transfers();
    let ada = b.account("ada").await;
    let dan = b.account("dan").await;
    assert_eq!(
        transfers.take(&parsed, &dan).await.err(),
        Some(TransferError::NotFound)
    );
    // Et pas avant qu'il soit complet.
    assert_eq!(
        transfers.take(&parsed, &ada).await.err(),
        Some(TransferError::NotFound)
    );
}

#[tokio::test]
async fn a_socket_without_a_valid_token_or_with_a_wrong_first_message_is_refused() {
    let b = bench().await;
    let id = b.deposit(1000).await;
    // Un jeton qui n'existe pas.
    let mut socket = open_transfer(&b.agent, &id).await.expect("socket");
    socket.auth("pas-un-jeton").await;
    let first = socket.next_upload().await.expect("erreur");
    assert_eq!(error_code(&first), TransferErrorCode::Unauthenticated);
    // Un premier message qui n'est pas `auth`.
    let mut socket = open_transfer(&b.agent, &id).await.expect("socket");
    socket.send_value(&UploadClientMessage::Open {}).await;
    let first = socket.next_upload().await.expect("erreur");
    assert_eq!(error_code(&first), TransferErrorCode::Unauthenticated);
    // Une trame binaire avant `auth`.
    let mut socket = open_transfer(&b.agent, &id).await.expect("socket");
    socket.send_binary(&[1, 2, 3]).await;
    let first = socket.next_upload().await.expect("erreur");
    assert_eq!(error_code(&first), TransferErrorCode::Unauthenticated);
    // Un identifiant qui n'est pas un ULID est refusé avant la mise à niveau (aucun nom de fichier ne s'y glisse).
    assert!(open_transfer(&b.agent, "../../etc/passwd").await.is_err());
    assert!(
        open_transfer(&b.agent, "01j9zy0g3q8m2k6w4t7v5n1b9d")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn abandoning_removes_the_part_and_the_row_and_closes_the_sender() {
    let b = bench().await;
    let bytes = data(2 * CHUNK_BYTES as usize, 3);
    let id = b.deposit(bytes.len() as u64).await;
    let (mut socket, _) = b.attach(&id).await;
    send_chunks(&mut socket, &bytes, 0..1).await;
    let reply = b.abandon(&b.ada, &id).await;
    let gone: TransferAbandoned = serde_json::from_value(reply.body).unwrap();
    assert!(gone.removed);
    assert!(
        b.files.names().is_empty(),
        "partie et liste retirées : {:?}",
        b.files.names()
    );
    assert_eq!(b.view(&b.ada, &id).await.status, 404);
    // L'émetteur encore branché ne peut plus rien écrire.
    let chunk = chunks(&bytes)[1];
    socket
        .send_binary(&encode_frame(1, &digest_of(chunk), chunk))
        .await;
    let refused = socket.next_upload().await.expect("erreur");
    assert_eq!(error_code(&refused), TransferErrorCode::TransferNotFound);
    // Abandonner deux fois n'est pas une erreur.
    let again: TransferAbandoned =
        serde_json::from_value(b.abandon(&b.ada, &id).await.body).unwrap();
    assert!(!again.removed);
}

#[tokio::test]
async fn a_newer_socket_takes_over_and_the_older_one_can_no_longer_write() {
    let b = bench().await;
    let bytes = data(3 * CHUNK_BYTES as usize, 5);
    let id = b.deposit(bytes.len() as u64).await;
    let (mut old, _) = b.attach(&id).await;
    send_chunks(&mut old, &bytes, 0..1).await;
    // Le client reprend alors que l'ancien socket n'est pas encore tombé.
    let (mut new, offset) = b.attach(&id).await;
    assert_eq!(offset, u64::from(CHUNK_BYTES));
    // L'ancien est écarté : son prochain morceau n'est pas écrit.
    let chunk = chunks(&bytes)[1];
    old.send_binary(&encode_frame(1, &digest_of(chunk), chunk))
        .await;
    let refused = old.next_upload().await.expect("erreur");
    assert_eq!(error_code(&refused), TransferErrorCode::TransferNotFound);
    assert_eq!(appends(&b.files, &id, 1), 0);
    send_chunks(&mut new, &bytes, 1..3).await;
    assert!(matches!(
        finish(&mut new, tree_of(&bytes)).await,
        UploadServerMessage::Done { .. }
    ));
    assert_eq!(b.files.bytes_of(&complete_name(&id)).unwrap(), bytes);
}

#[tokio::test]
async fn the_agent_holds_at_most_two_transfers_per_account_and_four_in_all() {
    let b = bench().await;
    let mut sockets = Vec::new();
    for _ in 0..2 {
        let id = b.deposit(1000).await;
        sockets.push(b.attach(&id).await.0);
    }
    // Un troisième envoi du même compte : refusé (`TRANSFER_LIMIT`).
    let third = b.deposit(1000).await;
    let (_socket, first) = b.attach_as(&b.ada, &third).await;
    assert_eq!(error_code(&first), TransferErrorCode::TransferLimit);
    // Un autre compte a ses propres places.
    let reply = b.open_as(&b.dan, NAME, 1000).await;
    let opened: DepositOpened = serde_json::from_value(reply.body).unwrap();
    let (_dan_socket, first) = b.attach_as(&b.dan, &opened.transfer_id).await;
    assert!(
        matches!(first, UploadServerMessage::Ready { .. }),
        "{first:?}"
    );
    // Un socket fermé rend sa place.
    sockets.pop().unwrap().drop_connection();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let (_again, first) = b.attach_as(&b.ada, &third).await;
    assert!(
        matches!(first, UploadServerMessage::Ready { .. }),
        "{first:?}"
    );
}

#[tokio::test]
async fn a_silent_client_loses_its_socket_but_the_upload_stays_resumable() {
    let b = bench_streamed(StreamSettings {
        idle_timeout: Duration::from_secs(1),
        ..fast_stream()
    })
    .await;
    let bytes = data(CHUNK_BYTES as usize + 5, 2);
    let id = b.deposit(bytes.len() as u64).await;
    let (mut socket, _) = b.attach(&id).await;
    send_chunks(&mut socket, &bytes, 0..1).await;
    // Le silence : le délai du flux de test est court ; le socket se ferme de lui-même.
    loop {
        match socket.try_next_upload(Duration::from_secs(10)).await {
            Some(Err(End::Closed(_) | End::Dropped)) => break,
            Some(Ok(_)) => {}
            None => panic!("un socket muet doit être fermé"),
        }
    }
    // Rien n'est perdu : la reprise repart de l'offset durable.
    let (mut socket, offset) = b.attach(&id).await;
    assert_eq!(offset, u64::from(CHUNK_BYTES));
    send_chunks(&mut socket, &bytes, 1..2).await;
    assert!(matches!(
        finish(&mut socket, tree_of(&bytes)).await,
        UploadServerMessage::Done { .. }
    ));
}

// ---------------------------------------------------------------------------------------------
// Vingt-quatre heures : le seul fichier que l'agent retire de la zone d'attente
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_part_not_resumed_in_24_hours_is_removed_and_nothing_else_is_touched() {
    let b = bench().await;
    let bytes = data(2 * CHUNK_BYTES as usize, 4);
    let id = b.deposit(bytes.len() as u64).await;
    let other = b.deposit(100).await;
    let (mut socket, _) = b.attach(&id).await;
    send_chunks(&mut socket, &bytes, 0..1).await;
    socket.drop_connection();
    // Un fichier étranger posé dans la zone : jamais retiré, aucune ligne ne le désigne.
    b.files.replace_by_hand("etranger.bin", b"pas a hearth");
    // Juste avant la fin du délai (2 s en test) : rien ne part.
    b.env.clock.advance(time::Duration::milliseconds(1500));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(b.files.exists(&part_name(&id)));
    // La reprise rafraîchit le délai du dépôt repris, pas celui des autres.
    let (_socket, offset) = b.attach(&id).await;
    assert_eq!(offset, u64::from(CHUNK_BYTES));
    b.env.clock.advance(time::Duration::milliseconds(1500));
    let started = std::time::Instant::now();
    while b.files.exists(&part_name(&other)) {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "le dépôt abandonné devait partir"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(b.files.exists(&part_name(&id)), "le dépôt repris reste");
    assert!(
        b.files.exists("etranger.bin"),
        "un fichier qui n'est pas à Hearth n'est jamais touché"
    );
    assert!(matches!(b.view(&b.ada, &other).await.status, 404 | 410));
    // Le dépôt repris expire à son tour s'il n'est plus touché.
    b.env.clock.advance(time::Duration::seconds(3));
    let started = std::time::Instant::now();
    while b.files.exists(&part_name(&id)) {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "le dépôt expiré devait partir"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        b.files.names().iter().all(|n| n == "etranger.bin"),
        "{:?}",
        b.files.names()
    );
}

#[tokio::test]
async fn an_expired_deposit_cannot_be_resumed_and_a_complete_one_unconsumed_goes_too() {
    let b = bench().await;
    let bytes = data(CHUNK_BYTES as usize, 2);
    let done = b.deposit(bytes.len() as u64).await;
    let (mut socket, _) = b.attach(&done).await;
    send_chunks(&mut socket, &bytes, 0..1).await;
    assert!(matches!(
        finish(&mut socket, tree_of(&bytes)).await,
        UploadServerMessage::Done { .. }
    ));
    b.env.clock.advance(time::Duration::seconds(3));
    let started = std::time::Instant::now();
    while b.files.exists(&complete_name(&done)) {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "le fichier complet jamais consommé devait partir"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(b.files.names().is_empty(), "{:?}", b.files.names());
    // La ligne part juste après les fichiers : « inconnu » (404) ou « expiré » (410), jamais un dépôt vivant.
    assert!(matches!(b.view(&b.ada, &done).await.status, 404 | 410));
}

// ---------------------------------------------------------------------------------------------
// Le consommateur : lire le dépôt complet, puis le libérer (HRT-124, HRT-110)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_consumer_reads_the_complete_deposit_then_releases_it() {
    use std::io::{Read, Seek, SeekFrom};
    let b = bench().await;
    let bytes = data(CHUNK_BYTES as usize + 99, 11);
    let id = b.deposit(bytes.len() as u64).await;
    let parsed = TransferId::parse(&id).unwrap();
    let ada = b.account("ada").await;
    let transfers = b.agent.transfers();
    assert_eq!(
        transfers.take(&parsed, &ada).await.err(),
        Some(TransferError::NotFound),
        "pas avant la fin de l'envoi"
    );
    let (mut socket, _) = b.attach(&id).await;
    send_chunks(&mut socket, &bytes, 0..2).await;
    assert!(matches!(
        finish(&mut socket, tree_of(&bytes)).await,
        UploadServerMessage::Done { .. }
    ));
    let mut taken = transfers.take(&parsed, &ada).await.expect("dépôt complet");
    assert_eq!(taken.size, bytes.len() as u64);
    assert_eq!(taken.tree_digest, tree_of(&bytes));
    // Lisible et déplaçable : une archive se juge par son index de fin.
    taken.reader.seek(SeekFrom::End(-10)).unwrap();
    let mut tail = Vec::new();
    taken.reader.read_to_end(&mut tail).unwrap();
    assert_eq!(tail, bytes[bytes.len() - 10..]);
    drop(taken);
    // Un fichier remplacé à la main entre-temps n'est jamais lu comme le dépôt.
    transfers.release(&parsed).await.expect("libéré");
    assert!(b.files.names().is_empty(), "{:?}", b.files.names());
    assert_eq!(
        transfers.take(&parsed, &ada).await.err(),
        Some(TransferError::NotFound)
    );
    // Libérer deux fois n'est pas une erreur.
    transfers.release(&parsed).await.expect("déjà libéré");
}

#[tokio::test]
async fn a_complete_deposit_replaced_by_hand_is_never_handed_to_the_consumer() {
    let b = bench().await;
    let bytes = data(1000, 12);
    let id = b.deposit(bytes.len() as u64).await;
    let parsed = TransferId::parse(&id).unwrap();
    let ada = b.account("ada").await;
    let (mut socket, _) = b.attach(&id).await;
    send_chunks(&mut socket, &bytes, 0..1).await;
    assert!(matches!(
        finish(&mut socket, tree_of(&bytes)).await,
        UploadServerMessage::Done { .. }
    ));
    b.files
        .replace_by_hand(&complete_name(&id), b"autre fichier");
    assert_eq!(
        b.agent.transfers().take(&parsed, &ada).await.err(),
        Some(TransferError::NotFound)
    );
    // Et la purge ne retire pas non plus un fichier qui n'est plus celui du registre.
    b.agent.transfers().release(&parsed).await.expect("libéré");
    assert!(
        b.files.exists(&complete_name(&id)),
        "le fichier étranger reste"
    );
}

// ---------------------------------------------------------------------------------------------
// Un client lent ou arrêté ne retient rien ; un fichier de plusieurs Gio ne passe pas par la mémoire
// ---------------------------------------------------------------------------------------------

fn resident_bytes() -> u64 {
    use sysinfo::{ProcessesToUpdate, System};
    let pid = sysinfo::get_current_pid().expect("pid");
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    system.process(pid).expect("processus").memory()
}

#[tokio::test]
async fn several_gibibytes_cross_the_agent_without_being_held_in_memory() {
    let b = bench().await;
    b.files.counting_only();
    // 2,5 Gio : 640 morceaux de 4 Mio, le même contenu à chaque tour (seul le rang change l'écriture).
    let total_chunks: u64 = 640;
    let size = total_chunks * u64::from(CHUNK_BYTES);
    let id = b.deposit(size).await;
    let (mut socket, _) = b.attach(&id).await;
    let chunk = data(CHUNK_BYTES as usize, 77);
    let digest = digest_of(&chunk);
    let mut digests = Vec::new();
    let before = resident_bytes();
    let mut peak = before;
    let started = std::time::Instant::now();
    for index in 0..total_chunks {
        // Au plus huit morceaux en vol : un accusé par tour suffit ici, la mémoire de l'agent est UN morceau.
        socket
            .send_binary(&encode_frame(index, &digest, &chunk))
            .await;
        digests.push(digest);
        match socket.next_upload().await.expect("accusé") {
            UploadServerMessage::Ack { offset, .. } => {
                assert_eq!(offset, (index + 1) * u64::from(CHUNK_BYTES));
            }
            other => panic!("`ack` attendu au morceau {index} : {other:?}"),
        }
        if index % 64 == 0 {
            peak = peak.max(resident_bytes());
        }
    }
    let UploadServerMessage::Done { size: done, .. } =
        finish(&mut socket, chunk_digest(&digests)).await
    else {
        panic!("`done` attendu");
    };
    assert_eq!(done, size);
    assert_eq!(b.files.len_of(&complete_name(&id)), Some(size));
    assert!(b.files.biggest_write() <= CHUNK_BYTES as usize);
    // L'agent ET le test (qui n'a que deux morceaux) vivent dans ce processus : bien en dessous de 2,5 Gio.
    let growth = peak.saturating_sub(before);
    assert!(
        growth < 512 << 20,
        "la mémoire a grandi de {} Mio en {:?}",
        growth >> 20,
        started.elapsed()
    );
}

#[test]
fn the_chunk_arithmetic_of_the_test_matches_the_protocol() {
    let size = 3 * u64::from(CHUNK_BYTES) + 1;
    assert_eq!(chunk_count(size), 4);
    assert_eq!(chunk_len(size, 3), 1);
    let _ = Value::Null;
}
