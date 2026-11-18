//! Le client RCON de la console (HRT-267, ADR-0067 D8, BR-CONSOLE-001, 007) contre un faux serveur de jeu qui parle (ou
//! ne parle pas) le protocole : réponse simple, découpée, vide, mot de passe refusé, longueurs hors bornes, paquets
//! d'un autre identifiant, flux sans fin, serveur muet, coupure. Aucun de ces cas ne panique ; le mot de passe n'est
//! dans aucune erreur.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::time::Duration;

use hearth_agent::application::ports::{ConsoleBudget, ConsoleError, GameConsole, MAX_REPLY_BYTES};
use hearth_agent::domain::secret::Secret;
use hearth_agent::infrastructure::console::SourceRcon;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const PASSWORD: &str = "sentinelle-mot-de-passe-rcon-71ac";

fn budget() -> ConsoleBudget {
    ConsoleBudget {
        connect: Duration::from_millis(500),
        reply: Duration::from_millis(900),
        quiet: Duration::from_millis(120),
    }
}

fn packet(id: i32, kind: i32, body: &[u8]) -> Vec<u8> {
    let len = i32::try_from(10 + body.len()).unwrap();
    let mut out = Vec::new();
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&id.to_le_bytes());
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(body);
    out.extend_from_slice(&[0, 0]);
    out
}

/// Ce que le serveur lit d'un paquet du client : identifiant, type, corps.
async fn read_packet(stream: &mut TcpStream) -> Option<(i32, i32, Vec<u8>)> {
    let mut head = [0u8; 4];
    stream.read_exact(&mut head).await.ok()?;
    let len = usize::try_from(i32::from_le_bytes(head)).ok()?;
    let mut rest = vec![0u8; len];
    stream.read_exact(&mut rest).await.ok()?;
    let id = i32::from_le_bytes(rest[0..4].try_into().ok()?);
    let kind = i32::from_le_bytes(rest[4..8].try_into().ok()?);
    Some((id, kind, rest[8..len - 2].to_vec()))
}

/// Un faux jeu : accepte UNE connexion, lit l'authentification, la juge (`auth_ok`), puis lit la commande et
/// la confie à `after` avec son identifiant. Rend l'adresse et ce que le serveur a lu (mot de passe, commande).
async fn serve<F, Fut>(
    auth_ok: bool,
    after: F,
) -> (SocketAddr, tokio::task::JoinHandle<(String, String)>)
where
    F: FnOnce(TcpStream, i32) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let (auth_id, kind, password) = read_packet(&mut stream).await.expect("authentification");
        assert_eq!(kind, 3);
        let reply_id = if auth_ok { auth_id } else { -1 };
        stream.write_all(&packet(reply_id, 2, b"")).await.unwrap();
        if !auth_ok {
            return (
                String::from_utf8_lossy(&password).into_owned(),
                String::new(),
            );
        }
        let Some((command_id, kind, command)) = read_packet(&mut stream).await else {
            return (
                String::from_utf8_lossy(&password).into_owned(),
                String::new(),
            );
        };
        assert_eq!(kind, 2);
        after(stream, command_id).await;
        (
            String::from_utf8_lossy(&password).into_owned(),
            String::from_utf8_lossy(&command).into_owned(),
        )
    });
    (address, handle)
}

async fn exchange(
    address: SocketAddr,
) -> Result<hearth_agent::application::ports::ConsoleReply, ConsoleError> {
    SourceRcon
        .exchange(address, &Secret::from(PASSWORD), "list", budget())
        .await
}

#[tokio::test]
async fn a_simple_reply_is_returned_and_the_server_saw_the_password_and_the_command_only_on_the_wire()
 {
    let (address, server) = serve(true, |mut stream, id| async move {
        stream
            .write_all(&packet(
                id,
                0,
                "There are 0 of a max of 20 players online: ".as_bytes(),
            ))
            .await
            .unwrap();
        // Garde la connexion le temps de la fin de réponse.
        tokio::time::sleep(Duration::from_millis(400)).await;
    })
    .await;
    let reply = exchange(address).await.expect("réponse");
    assert_eq!(reply.text, "There are 0 of a max of 20 players online: ");
    assert!(!reply.truncated);
    let (password, command) = server.await.unwrap();
    assert_eq!(
        password, PASSWORD,
        "le mot de passe est dans le paquet d'authentification"
    );
    assert_eq!(command, "list");
}

#[tokio::test]
async fn a_reply_split_in_three_packets_is_joined() {
    let (address, _) = serve(true, |mut stream, id| async move {
        for part in ["a", "b", "c"] {
            stream
                .write_all(&packet(id, 0, part.as_bytes()))
                .await
                .unwrap();
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    })
    .await;
    assert_eq!(exchange(address).await.unwrap().text, "abc");
}

#[tokio::test]
async fn an_empty_reply_is_an_empty_text() {
    let (address, _) = serve(true, |mut stream, id| async move {
        stream.write_all(&packet(id, 0, b"")).await.unwrap();
        tokio::time::sleep(Duration::from_millis(400)).await;
    })
    .await;
    let reply = exchange(address).await.unwrap();
    assert_eq!(reply.text, "");
    assert!(!reply.truncated);
}

#[tokio::test]
async fn a_refused_password_is_reported_without_the_password() {
    let (address, server) = serve(false, |_, _| async {}).await;
    let error = exchange(address).await.unwrap_err();
    assert_eq!(error, ConsoleError::AuthRefused);
    assert!(!format!("{error:?} {error}").contains(PASSWORD));
    server.await.unwrap();
}

#[tokio::test]
async fn a_length_out_of_bounds_cuts_the_exchange() {
    for len in [-5i32, 0, 9, 4111, 100_000] {
        let (address, _) = serve(true, move |mut stream, _| async move {
            stream.write_all(&len.to_le_bytes()).await.unwrap();
            stream.write_all(&[0u8; 32]).await.unwrap();
            tokio::time::sleep(Duration::from_millis(300)).await;
        })
        .await;
        assert_eq!(
            exchange(address).await.unwrap_err(),
            ConsoleError::Protocol,
            "{len}"
        );
    }
}

#[tokio::test]
async fn a_packet_of_another_identifier_or_type_is_ignored_until_the_deadline() {
    let (address, _) = serve(true, |mut stream, id| async move {
        stream
            .write_all(&packet(id + 1, 0, b"autre"))
            .await
            .unwrap();
        stream
            .write_all(&packet(id, 2, b"mauvais type"))
            .await
            .unwrap();
        stream.write_all(&packet(id, 3, b"encore")).await.unwrap();
        tokio::time::sleep(Duration::from_millis(1500)).await;
    })
    .await;
    assert_eq!(exchange(address).await.unwrap_err(), ConsoleError::NoAnswer);
}

#[tokio::test]
async fn an_endless_stream_is_cut_at_sixteen_kibibytes() {
    let (address, _) = serve(true, |mut stream, id| async move {
        let body = vec![b'x'; 4096];
        for _ in 0..40 {
            if stream.write_all(&packet(id, 0, &body)).await.is_err() {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    })
    .await;
    let reply = exchange(address).await.unwrap();
    assert!(reply.truncated);
    assert_eq!(reply.text.len(), MAX_REPLY_BYTES);
}

#[tokio::test]
async fn a_mute_server_gives_no_answer_and_a_cut_connection_is_unreachable() {
    let (address, _) = serve(true, |stream, _| async move {
        let _kept_open = stream;
        tokio::time::sleep(Duration::from_millis(1500)).await;
    })
    .await;
    assert_eq!(exchange(address).await.unwrap_err(), ConsoleError::NoAnswer);
    // Coupé au milieu d'un en-tête, avant toute réponse.
    let (address, _) = serve(true, |mut stream, _| async move {
        stream.write_all(&[14, 0]).await.unwrap();
    })
    .await;
    assert_eq!(
        exchange(address).await.unwrap_err(),
        ConsoleError::Unreachable
    );
    // Aucun serveur.
    let closed = {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        listener.local_addr().unwrap()
    };
    assert_eq!(
        exchange(closed).await.unwrap_err(),
        ConsoleError::Unreachable
    );
}

#[tokio::test]
async fn colour_codes_and_control_characters_never_reach_the_caller() {
    let (address, _) = serve(true, |mut stream, id| async move {
        stream
            .write_all(&packet(id, 0, "\u{a7}6Bram\u{a7}r a\u{7}\tb".as_bytes()))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(400)).await;
    })
    .await;
    assert_eq!(exchange(address).await.unwrap().text, "Bram a  b");
}
