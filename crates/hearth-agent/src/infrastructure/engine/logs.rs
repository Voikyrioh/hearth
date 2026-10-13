//! Lecture des journaux d'un conteneur : `GET /containers/{id}/logs` (HRT-66, BR-SLOG-*).
//!
//! Écrite à la main sur le socket, comme la sonde (`bollard::ping`), et non par `Docker::logs` de
//! `bollard` 0.21.1, pour deux raisons établies à la lecture de son décodeur (`read.rs`) :
//! - une ligne de terminal qui ne se termine pas s'accumule SANS borne (le décodeur ne rend rien tant
//!   qu'il n'a pas vu de fin de ligne), et un programme écrit ce qu'il veut dans ses journaux ;
//! - à la fin du flux, les octets restants sont écrits dans une trace de niveau `debug` : du contenu de
//!   journal dans une trace, ce que BR-SLOG-019 interdit.
//!
//! (Son décodeur lit aussi un en-tête de huit octets dès que le premier octet d'un tampon vaut 0, 1 ou 2, et
//! en tire une longueur jusqu'à 4 Gio ; avec les horodatages que cette requête demande toujours, chaque
//! message commence par un chiffre, donc ce chemin n'est pas atteignable comme cela : ce n'est PAS la raison
//! du remplacement.) Ici, tout est borné : en-tête de réponse, ligne de taille de morceau, trame (256 Kio),
//! ligne en cours d'assemblage (`RAW_LINE_CAP`), lignes prêtes à une lecture (un seul tampon de 16 Kio
//! décodé à la fois), et délai d'inactivité d'une lecture sans suivi.
//!
//! Deux formes de flux, selon le conteneur :
//! - sans terminal, des trames de huit octets d'en-tête (`[type, 0, 0, 0, longueur sur 4 octets]`),
//!   `type` 1 pour la sortie normale et 2 pour la sortie d'erreur ;
//! - avec terminal, des octets bruts : une seule sortie.
//!
//! Chaque message commence par l'horodatage du moteur (`timestamps=1`). Aucun texte de ligne n'entre dans
//! une trace, une erreur ou un `Debug` : ce fichier n'écrit rien nulle part.

use std::collections::VecDeque;
use std::time::Duration;

use futures_util::{StreamExt, stream};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::bollard::Target;
use crate::application::ports::{EngineError, LogQuery, LogStream};
use crate::domain::services::logs::{
    LineAssembler, LogLine, LogSource, RAW_LINE_CAP, split_timestamp,
};

/// Délai pour obtenir les en-têtes de la réponse (le moteur répond tout de suite, même à un suivi).
const HEAD_TIMEOUT: Duration = Duration::from_secs(10);
/// Taille maximale des en-têtes de la réponse.
const MAX_HEAD_BYTES: usize = 16 * 1024;
/// Octets lus à la fois sur le socket.
const READ_BYTES: usize = 16 * 1024;
/// Taille maximale d'une trame du moteur : au-delà, le flux est refusé (le moteur coupe les
/// messages à 16 Kio, l'horodatage compris).
const MAX_FRAME_BYTES: usize = 256 * 1024;
/// Longueur maximale d'une ligne de taille de morceau (`1f4;ext\r\n`).
const MAX_CHUNK_LINE: usize = 64;
/// Silence maximal d'une lecture SANS suivi (un fichier déjà écrit) : un moteur figé ne garde ni la
/// lecture ni sa place. Un suivi, lui, peut se taire aussi longtemps que le service.
const IDLE_READ_TIMEOUT: Duration = Duration::from_secs(30);
/// Le moteur coupe un message de journal à 16 Kio de texte et horodate chaque morceau (de mémoire de son
/// code, à prouver sur un vrai moteur : job `engine`).
const ENGINE_MESSAGE_BYTES: usize = 16 * 1024;

trait Conn: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Conn for T {}

/// Ouvre la lecture. Le flux se connecte à sa première lecture : l'abandonner avant ne fait rien.
pub(super) fn open(target: Target, id: String, query: LogQuery) -> LogStream {
    stream::once(async move {
        let conn = connect(&target).await?;
        Reader::open_over(conn, &id, query).await
    })
    .flat_map(|opened| match opened {
        Ok(reader) => stream::unfold(reader, Reader::step).boxed(),
        Err(error) => stream::once(async move { Err(error) }).boxed(),
    })
    .boxed()
}

async fn connect(target: &Target) -> Result<Box<dyn Conn>, EngineError> {
    let failed = |error: std::io::Error| match error.kind() {
        std::io::ErrorKind::PermissionDenied => EngineError::Denied,
        _ => EngineError::Unreachable,
    };
    match target {
        #[cfg(unix)]
        Target::Unix(path) => {
            let stream = tokio::net::UnixStream::connect(path)
                .await
                .map_err(failed)?;
            Ok(Box::new(stream))
        }
        #[cfg(not(unix))]
        Target::Unix(_) => Err(EngineError::Unreachable),
        Target::Http(address) => {
            let stream = tokio::net::TcpStream::connect(address)
                .await
                .map_err(failed)?;
            Ok(Box::new(stream))
        }
    }
}

/// Un identifiant ou un nom de conteneur sûr dans un chemin : jamais d'octet qui change la requête.
fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

/// La requête. Les deux sorties, les horodatages du moteur, et `since` en secondes entières.
fn request(id: &str, query: LogQuery) -> String {
    let mut path = format!(
        "/containers/{id}/logs?stdout=1&stderr=1&timestamps=1&follow={}&tail=",
        u8::from(query.follow)
    );
    match query.tail {
        Some(count) => path.push_str(&count.to_string()),
        None => path.push_str("all"),
    }
    if let Some(since_ms) = query.since_ms {
        path.push_str(&format!("&since={}", since_ms.div_euclid(1000).max(0)));
    }
    format!("GET {path} HTTP/1.1\r\nHost: docker\r\nConnection: close\r\nAccept: */*\r\n\r\n")
}

#[derive(Default)]
struct ByteBuf {
    data: Vec<u8>,
    pos: usize,
}

impl ByteBuf {
    fn extend(&mut self, bytes: &[u8]) {
        if self.pos > 0 && self.pos * 2 >= self.data.len() {
            self.data.drain(..self.pos);
            self.pos = 0;
        }
        self.data.extend_from_slice(bytes);
    }

    fn unread(&self) -> &[u8] {
        &self.data[self.pos..]
    }

    fn consume(&mut self, count: usize) {
        self.pos = (self.pos + count).min(self.data.len());
        if self.pos == self.data.len() {
            self.data.clear();
            self.pos = 0;
        }
    }
}

enum Chunk {
    Size,
    Data(usize),
    Crlf,
    Done,
}

enum Body {
    Chunked(Chunk),
    Length(usize),
    UntilClose,
}

struct Reader {
    conn: Box<dyn Conn>,
    tty: bool,
    /// La lecture suit le service (sinon elle finit seule, sous délai d'inactivité).
    follow: bool,
    /// Silence toléré d'une lecture sans suivi.
    idle: Duration,
    body_mode: Body,
    /// Octets du corps HTTP pas encore décodés.
    wire: ByteBuf,
    /// Octets du corps décodés (trames, ou flux brut).
    frames: ByteBuf,
    assembler: LineAssembler,
    ready: VecDeque<LogLine>,
    scratch: Vec<u8>,
    /// Dernière heure lue : celle des morceaux d'une ligne coupée par le moteur, qui n'en portent pas.
    stamp: i64,
    /// Terminal : le prochain octet est précédé d'un horodatage du moteur (début de ligne, ou début d'un
    /// morceau de 16 Kio d'une longue ligne).
    tty_expects_stamp: bool,
    /// Terminal : octets de texte de la ligne en cours (hors horodatages).
    tty_line_bytes: usize,
    body_finished: bool,
    eof: bool,
    done: bool,
    /// Les octets reçus avec les en-têtes ont été décodés.
    primed: bool,
}

impl Reader {
    /// Envoie la requête, lit et juge l'en-tête de la réponse.
    async fn open_over(
        mut conn: Box<dyn Conn>,
        id: &str,
        query: LogQuery,
    ) -> Result<Self, EngineError> {
        if !is_safe_id(id) {
            return Err(EngineError::NotFound);
        }
        let head = tokio::time::timeout(HEAD_TIMEOUT, async {
            conn.write_all(request(id, query).as_bytes())
                .await
                .map_err(|_| EngineError::Unreachable)?;
            read_head(&mut *conn).await
        })
        .await
        .map_err(|_| EngineError::Timeout)??;
        let (status, body_mode, rest) = head;
        match status {
            200..=299 => {}
            404 => return Err(EngineError::NotFound),
            // « configured logging driver does not support reading » : le pilote `none`.
            501 => return Err(EngineError::Unsupported),
            401 | 403 => return Err(EngineError::Denied),
            502..=504 => return Err(EngineError::Unreachable),
            _ => return Err(EngineError::Refused),
        }
        let mut wire = ByteBuf::default();
        wire.extend(&rest);
        Ok(Self {
            conn,
            tty: query.tty,
            follow: query.follow,
            idle: IDLE_READ_TIMEOUT,
            body_mode,
            wire,
            frames: ByteBuf::default(),
            assembler: LineAssembler::new(),
            ready: VecDeque::new(),
            scratch: vec![0; READ_BYTES],
            stamp: 0,
            tty_expects_stamp: true,
            tty_line_bytes: 0,
            body_finished: false,
            eof: false,
            done: false,
            primed: false,
        })
    }

    async fn step(mut self) -> Option<(Result<LogLine, EngineError>, Self)> {
        loop {
            if let Some(line) = self.ready.pop_front() {
                return Some((Ok(line), self));
            }
            if self.done {
                return None;
            }
            if let Err(error) = self.fill().await {
                self.done = true;
                return Some((Err(error), self));
            }
        }
    }

    /// Lit un morceau du socket et en tire des lignes. À la fin du flux, la dernière ligne part.
    async fn fill(&mut self) -> Result<(), EngineError> {
        // Les octets du corps arrivés avec les en-têtes sont décodés AVANT toute lecture : un service qui
        // n'a écrit qu'un message ne fait pas attendre ce message jusqu'à la prochaine écriture.
        if !self.primed {
            self.primed = true;
            if !self.wire.unread().is_empty() {
                self.process()?;
                if !self.ready.is_empty() || self.done {
                    return Ok(());
                }
            }
        }
        if !self.eof && !self.body_finished {
            let read = if self.follow {
                self.conn.read(&mut self.scratch).await
            } else {
                tokio::time::timeout(self.idle, self.conn.read(&mut self.scratch))
                    .await
                    .map_err(|_| EngineError::Timeout)?
            }
            .map_err(|_| EngineError::Unreachable)?;
            if read == 0 {
                self.eof = true;
            } else {
                self.wire.extend(&self.scratch[..read]);
            }
        }
        self.process()
    }

    /// Décode ce qui est déjà reçu ; à la fin du flux, la dernière ligne part.
    fn process(&mut self) -> Result<(), EngineError> {
        self.decode_wire()?;
        self.demux()?;
        if self.eof || self.body_finished {
            if self.tty {
                let rest = self.frames.unread().to_vec();
                self.frames.consume(rest.len());
                self.feed_tty_piece(&rest);
            }
            let mut lines = Vec::new();
            self.assembler.finish(&mut lines);
            self.ready.extend(lines);
            self.done = true;
        }
        Ok(())
    }

    /// Du corps HTTP (éventuellement en morceaux) aux octets du flux.
    fn decode_wire(&mut self) -> Result<(), EngineError> {
        loop {
            match &mut self.body_mode {
                Body::UntilClose => {
                    let bytes = self.wire.unread().to_vec();
                    self.wire.consume(bytes.len());
                    self.frames.extend(&bytes);
                    return Ok(());
                }
                Body::Length(remaining) => {
                    let take = (*remaining).min(self.wire.unread().len());
                    let bytes = self.wire.unread()[..take].to_vec();
                    self.wire.consume(take);
                    self.frames.extend(&bytes);
                    *remaining -= take;
                    if *remaining == 0 {
                        self.body_finished = true;
                    }
                    return Ok(());
                }
                Body::Chunked(state) => match state {
                    Chunk::Size => {
                        let unread = self.wire.unread();
                        let Some(end) = unread.iter().position(|byte| *byte == b'\n') else {
                            if unread.len() > MAX_CHUNK_LINE {
                                return Err(EngineError::Refused);
                            }
                            return Ok(());
                        };
                        if end > MAX_CHUNK_LINE {
                            return Err(EngineError::Refused);
                        }
                        let line = &unread[..end];
                        let digits = line
                            .split(|byte| matches!(byte, b';' | b'\r'))
                            .next()
                            .unwrap_or_default();
                        let size = std::str::from_utf8(digits)
                            .ok()
                            .and_then(|text| usize::from_str_radix(text.trim(), 16).ok())
                            .ok_or(EngineError::Refused)?;
                        self.wire.consume(end + 1);
                        *state = if size == 0 {
                            Chunk::Done
                        } else {
                            Chunk::Data(size)
                        };
                    }
                    Chunk::Data(remaining) => {
                        let available = self.wire.unread().len();
                        if available == 0 {
                            return Ok(());
                        }
                        let take = (*remaining).min(available);
                        let bytes = self.wire.unread()[..take].to_vec();
                        self.wire.consume(take);
                        self.frames.extend(&bytes);
                        *remaining -= take;
                        if *remaining == 0 {
                            *state = Chunk::Crlf;
                        }
                    }
                    Chunk::Crlf => {
                        let unread = self.wire.unread();
                        if unread.len() < 2 {
                            return Ok(());
                        }
                        if &unread[..2] != b"\r\n" {
                            return Err(EngineError::Refused);
                        }
                        self.wire.consume(2);
                        *state = Chunk::Size;
                    }
                    Chunk::Done => {
                        self.body_finished = true;
                        return Ok(());
                    }
                },
            }
        }
    }

    /// Des octets du flux aux lignes.
    fn demux(&mut self) -> Result<(), EngineError> {
        if self.tty {
            return self.demux_tty();
        }
        loop {
            let unread = self.frames.unread();
            if unread.len() < 8 {
                return Ok(());
            }
            let kind = unread[0];
            let length = u32::from_be_bytes([unread[4], unread[5], unread[6], unread[7]]) as usize;
            if unread[1..4] != [0, 0, 0] || kind > 2 || length > MAX_FRAME_BYTES {
                return Err(EngineError::Refused);
            }
            if unread.len() < 8 + length {
                return Ok(());
            }
            let source = if kind == 2 {
                LogSource::Err
            } else {
                LogSource::Out
            };
            let payload = unread[8..8 + length].to_vec();
            self.frames.consume(8 + length);
            self.feed(source, &payload);
        }
    }

    /// Terminal : des octets bruts, une ligne par `\n`, chacune précédée de son horodatage.
    fn demux_tty(&mut self) -> Result<(), EngineError> {
        loop {
            let unread = self.frames.unread();
            let piece_len = match unread.iter().position(|byte| *byte == b'\n') {
                Some(end) => end + 1,
                // Pas de fin de ligne : on attend la suite, sauf si la ligne en cours dépasse déjà de quoi
                // en garder le début (le reste est compté puis jeté par l'assembleur).
                None if unread.len() >= RAW_LINE_CAP => unread.len(),
                None => return Ok(()),
            };
            let piece = unread[..piece_len].to_vec();
            self.frames.consume(piece_len);
            self.feed_tty_piece(&piece);
        }
    }

    /// Un morceau du flux brut d'un terminal. Le moteur horodate chaque message ; sa limite est de 16 Kio de
    /// texte, donc une longue ligne porte un horodatage tous les 16 Kio. Il est lu et retiré à chaque
    /// frontière de 16 Kio (et au début de chaque ligne) : il ne reste pas dans le texte, ne coupe pas un
    /// caractère en deux et ne gonfle pas le total d'une ligne coupée.
    fn feed_tty_piece(&mut self, piece: &[u8]) {
        let mut rest = piece;
        while !rest.is_empty() {
            if self.tty_expects_stamp {
                self.tty_expects_stamp = false;
                if let Some((at_ms, skip)) = split_timestamp(rest) {
                    // Le premier horodatage d'une ligne est celui de la ligne ; ceux des morceaux
                    // suivants ne la datent pas de nouveau.
                    if self.tty_line_bytes == 0 {
                        self.stamp = at_ms;
                    }
                    rest = &rest[skip..];
                    continue;
                }
            }
            let room = ENGINE_MESSAGE_BYTES - self.tty_line_bytes % ENGINE_MESSAGE_BYTES;
            let window = &rest[..room.min(rest.len())];
            let take = match window.iter().position(|byte| *byte == b'\n') {
                Some(end) => end + 1,
                None => window.len(),
            };
            let (head, tail) = rest.split_at(take);
            let mut lines = Vec::new();
            self.assembler
                .push(LogSource::Out, self.stamp, head, &mut lines);
            self.ready.extend(lines);
            if head.ends_with(b"\n") {
                self.tty_line_bytes = 0;
                self.tty_expects_stamp = true;
            } else {
                self.tty_line_bytes += head.len();
                if self.tty_line_bytes.is_multiple_of(ENGINE_MESSAGE_BYTES) {
                    self.tty_expects_stamp = true;
                }
            }
            rest = tail;
        }
    }

    /// Un message du moteur : son horodatage est lu et retiré, le reste va à l'assembleur.
    fn feed(&mut self, source: LogSource, payload: &[u8]) {
        let (at_ms, text) = match split_timestamp(payload) {
            Some((at_ms, skip)) => (at_ms, &payload[skip..]),
            None => (self.stamp, payload),
        };
        self.stamp = at_ms;
        let mut lines = Vec::new();
        self.assembler.push(source, at_ms, text, &mut lines);
        self.ready.extend(lines);
    }
}

/// Lit jusqu'à la fin des en-têtes de la réponse : le statut, la forme du corps, et les octets du corps
/// déjà reçus.
async fn read_head(conn: &mut dyn Conn) -> Result<(u16, Body, Vec<u8>), EngineError> {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 1024];
    let end = loop {
        if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break position;
        }
        if bytes.len() > MAX_HEAD_BYTES {
            return Err(EngineError::Refused);
        }
        let read = conn
            .read(&mut chunk)
            .await
            .map_err(|_| EngineError::Unreachable)?;
        if read == 0 {
            return Err(EngineError::Unreachable);
        }
        bytes.extend_from_slice(&chunk[..read]);
    };
    let head = String::from_utf8_lossy(&bytes[..end]).into_owned();
    let rest = bytes[end + 4..].to_vec();
    let mut lines = head.split("\r\n");
    let status: u16 = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .ok_or(EngineError::Refused)?;
    let mut mode = Body::UntilClose;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let (name, value) = (name.trim().to_ascii_lowercase(), value.trim());
        if name == "transfer-encoding" && value.to_ascii_lowercase().contains("chunked") {
            mode = Body::Chunked(Chunk::Size);
        } else if name == "content-length"
            && !matches!(mode, Body::Chunked(_))
            && let Ok(length) = value.parse::<usize>()
        {
            mode = Body::Length(length);
        }
    }
    Ok((status, mode, rest))
}

#[cfg(test)]
mod tests {
    use futures_util::TryStreamExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

    use super::*;

    const FOLLOW: LogQuery = LogQuery {
        follow: false,
        tail: Some(1000),
        since_ms: None,
        tty: false,
    };

    fn frame(kind: u8, text: &str) -> Vec<u8> {
        let mut bytes = vec![kind, 0, 0, 0];
        bytes.extend_from_slice(&u32::try_from(text.len()).unwrap().to_be_bytes());
        bytes.extend_from_slice(text.as_bytes());
        bytes
    }

    fn chunked(parts: &[&[u8]]) -> Vec<u8> {
        let mut bytes = b"HTTP/1.1 200 OK\r\nContent-Type: application/vnd.docker.raw-stream\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
        for part in parts {
            bytes.extend_from_slice(format!("{:x}\r\n", part.len()).as_bytes());
            bytes.extend_from_slice(part);
            bytes.extend_from_slice(b"\r\n");
        }
        bytes.extend_from_slice(b"0\r\n\r\n");
        bytes
    }

    /// Un « moteur » qui répond cette suite d'octets à la requête, puis ferme.
    async fn serve(
        response: Vec<u8>,
        slice: usize,
    ) -> (Box<dyn Conn>, tokio::task::JoinHandle<String>) {
        let (client, mut server): (DuplexStream, DuplexStream) = tokio::io::duplex(1 << 20);
        let task = tokio::spawn(async move {
            let mut request = vec![0u8; 4096];
            let read = server.read(&mut request).await.unwrap();
            let text = String::from_utf8_lossy(&request[..read]).into_owned();
            for part in response.chunks(slice) {
                // Le lecteur peut avoir fini (ou refusé la réponse) avant la fin de l'écriture.
                if server.write_all(part).await.is_err() {
                    break;
                }
                tokio::task::yield_now().await;
            }
            let _ = server.shutdown().await;
            text
        });
        (Box::new(client), task)
    }

    async fn read_all(
        response: Vec<u8>,
        slice: usize,
        query: LogQuery,
    ) -> (Result<Vec<LogLine>, EngineError>, String) {
        let (conn, task) = serve(response, slice).await;
        let result = match Reader::open_over(conn, "abc123", query).await {
            Ok(reader) => stream::unfold(reader, Reader::step).try_collect().await,
            Err(error) => Err(error),
        };
        (result, task.await.unwrap())
    }

    #[tokio::test]
    async fn multiplexed_frames_become_stamped_lines_of_the_right_output() {
        let body = [
            frame(1, "2026-10-13T10:00:00.000000001Z premiere\n"),
            frame(2, "2026-10-13T10:00:00.000000002Z erreur\n"),
            frame(1, "2026-10-13T10:00:01.500000000Z \x1b[32mverte\x1b[0m\r\n"),
        ]
        .concat();
        // Octet par octet : les trames, les morceaux et les lignes tombent n'importe où.
        for slice in [1, 7, 4096] {
            let (lines, request) =
                read_all(chunked(&[&body[..30], &body[30..]]), slice, FOLLOW).await;
            let lines = lines.unwrap();
            let shown: Vec<_> = lines
                .iter()
                .map(|line| (line.at_ms, line.source, line.text.as_str()))
                .collect();
            assert_eq!(
                shown,
                [
                    (1_791_885_600_000, LogSource::Out, "premiere"),
                    (1_791_885_600_000, LogSource::Err, "erreur"),
                    (1_791_885_601_500, LogSource::Out, "verte"),
                ],
                "tranches de {slice}"
            );
            assert!(request.starts_with("GET /containers/abc123/logs?stdout=1&stderr=1&timestamps=1&follow=0&tail=1000 HTTP/1.1\r\n"), "{request}");
        }
    }

    #[tokio::test]
    async fn a_line_cut_by_the_engine_in_two_frames_is_one_line_with_the_time_of_its_start() {
        let body = [
            frame(1, "2026-10-13T10:00:00.100000000Z debut "),
            frame(1, "2026-10-13T10:00:00.200000000Z fin\n"),
        ]
        .concat();
        let (lines, _) = read_all(chunked(&[&body]), 4096, FOLLOW).await;
        let lines = lines.unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "debut fin");
        assert_eq!(lines[0].at_ms, 1_791_885_600_100);
    }

    #[tokio::test]
    async fn a_terminal_container_gives_one_raw_stream_even_when_a_line_starts_with_a_control_byte()
    {
        // Un programme écrit ce qu'il veut : des octets de contrôle et invalides juste après l'horodatage du
        // moteur ne sont jamais lus comme l'en-tête d'une trame, la forme du flux vient du seul drapeau
        // `tty` du registre.
        let body = b"2026-10-13T10:00:00.000000000Z \x01\x01\x01\x01\xff\xff\xff\xff boum\r\n\
                     2026-10-13T10:00:01.000000000Z deuxieme\r\n\
                     2026-10-13T10:00:02.000000000Z sans fin"
            .to_vec();
        let query = LogQuery {
            tty: true,
            ..FOLLOW
        };
        for slice in [1, 5, 4096] {
            let (lines, _) = read_all(chunked(&[&body]), slice, query).await;
            let lines = lines.unwrap();
            let texts: Vec<_> = lines.iter().map(|line| line.text.as_str()).collect();
            assert_eq!(
                texts,
                [
                    "\u{fffd}\u{fffd}\u{fffd}\u{fffd} boum",
                    "deuxieme",
                    "sans fin"
                ],
                "tranches de {slice}"
            );
            assert!(lines.iter().all(|line| line.source == LogSource::Out));
            assert_eq!(lines[1].at_ms, 1_791_885_601_000);
        }
    }

    #[tokio::test]
    async fn a_huge_terminal_line_is_bounded_and_counted_never_buffered() {
        let mut body = b"2026-10-13T10:00:00.000000000Z ".to_vec();
        body.extend(std::iter::repeat_n(b'z', 3 << 20));
        body.extend_from_slice(b"\n2026-10-13T10:00:01.000000000Z apres\n");
        let query = LogQuery {
            tty: true,
            ..FOLLOW
        };
        let (lines, _) = read_all(chunked(&[&body]), 65536, query).await;
        let lines = lines.unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text.len(), 10_000);
        assert!(lines[0].cut.is_some_and(|total| total >= 3 << 20));
        assert_eq!(lines[1].text, "apres");
    }

    #[tokio::test]
    async fn the_engine_stamps_inside_a_long_terminal_line_do_not_end_up_in_the_text() {
        // Le moteur coupe un message à 16 Kio et horodate chaque morceau : 40 000 octets de `é` font trois
        // morceaux, dont la frontière tombe ici au milieu d'un caractère de deux octets ou entre deux.
        let line = "é".repeat(20_000);
        let bytes = line.as_bytes();
        let mut body = Vec::new();
        for (n, piece) in bytes.chunks(ENGINE_MESSAGE_BYTES).enumerate() {
            body.extend_from_slice(format!("2026-10-13T10:00:0{n}.000000000Z ").as_bytes());
            body.extend_from_slice(piece);
        }
        body.extend_from_slice(b"\n2026-10-13T10:00:09.000000000Z apres\n");
        let query = LogQuery {
            tty: true,
            ..FOLLOW
        };
        for slice in [1, 4096, 1 << 20] {
            let (lines, _) = read_all(chunked(&[&body]), slice, query).await;
            let lines = lines.unwrap();
            assert_eq!(lines.len(), 2, "tranches de {slice}");
            assert!(
                lines[0].text.chars().all(|c| c == 'é'),
                "aucun horodatage dans le texte"
            );
            assert_eq!(lines[0].text.chars().count(), 10_000);
            assert_eq!(
                lines[0].cut,
                Some(20_000),
                "le total ne compte pas les horodatages"
            );
            assert_eq!(
                lines[0].at_ms, 1_791_885_600_000,
                "l'heure du début de la ligne"
            );
            assert_eq!(lines[1].text, "apres");
        }
    }

    #[tokio::test]
    async fn a_message_that_came_with_the_headers_is_not_held_back_until_the_next_write() {
        // Un suivi dont le moteur envoie en-têtes et premier message d'un seul tenant, puis se tait.
        let (client, mut server): (DuplexStream, DuplexStream) = tokio::io::duplex(1 << 16);
        let task = tokio::spawn(async move {
            let mut request = vec![0u8; 4096];
            let _ = server.read(&mut request).await;
            let message = frame(1, "2026-10-13T10:00:00.000000000Z un seul message\n");
            let mut reply = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
            reply.extend_from_slice(format!("{:x}\r\n", message.len()).as_bytes());
            reply.extend_from_slice(&message);
            reply.extend_from_slice(b"\r\n");
            let _ = server.write_all(&reply).await;
            tokio::time::sleep(Duration::from_secs(3_600)).await;
            drop(server);
        });
        let query = LogQuery {
            follow: true,
            ..FOLLOW
        };
        let reader = Reader::open_over(Box::new(client), "abc123", query)
            .await
            .unwrap();
        let mut lines = Box::pin(stream::unfold(reader, Reader::step));
        let first = tokio::time::timeout(Duration::from_secs(5), lines.next())
            .await
            .expect("la ligne attend une écriture qui ne vient pas")
            .expect("une ligne")
            .unwrap();
        assert_eq!(first.text, "un seul message");
        task.abort();
    }

    #[tokio::test]
    async fn a_read_without_follow_gives_up_on_an_engine_that_goes_silent() {
        // Un moteur qui répond l'en-tête puis ne dit plus rien et ne ferme pas.
        let (client, mut server): (DuplexStream, DuplexStream) = tokio::io::duplex(1 << 16);
        let task = tokio::spawn(async move {
            let mut request = vec![0u8; 4096];
            let _ = server.read(&mut request).await;
            let _ = server
                .write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
                .await;
            tokio::time::sleep(Duration::from_secs(3_600)).await;
            drop(server);
        });
        let mut reader = Reader::open_over(Box::new(client), "abc123", FOLLOW)
            .await
            .unwrap();
        reader.idle = Duration::from_millis(100);
        let result: Result<Vec<LogLine>, EngineError> =
            stream::unfold(reader, Reader::step).try_collect().await;
        assert_eq!(result.expect_err("abandon"), EngineError::Timeout);
        task.abort();
    }

    #[tokio::test]
    async fn a_body_without_chunks_is_read_by_length_or_until_the_connection_closes() {
        let body = frame(1, "2026-10-13T10:00:00.000000000Z ok\n");
        let mut with_length =
            format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len()).into_bytes();
        with_length.extend_from_slice(&body);
        let (lines, _) = read_all(with_length, 3, FOLLOW).await;
        assert_eq!(lines.unwrap().len(), 1);
        let mut until_close = b"HTTP/1.1 200 OK\r\n\r\n".to_vec();
        until_close.extend_from_slice(&body);
        let (lines, _) = read_all(until_close, 3, FOLLOW).await;
        assert_eq!(lines.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn the_engine_statuses_are_translated_and_its_text_is_dropped() {
        for (status, expected) in [
            (404, EngineError::NotFound),
            (501, EngineError::Unsupported),
            (403, EngineError::Denied),
            (503, EngineError::Unreachable),
            (500, EngineError::Refused),
        ] {
            let reply = format!(
                "HTTP/1.1 {status} X\r\nContent-Length: 20\r\n\r\n{{\"message\":\"SECRET\"}}"
            );
            let (result, _) = read_all(reply.into_bytes(), 4096, FOLLOW).await;
            let error = result.expect_err("refus");
            assert_eq!(error, expected, "{status}");
            assert!(!format!("{error} {error:?}").contains("SECRET"));
        }
    }

    #[tokio::test]
    async fn a_frame_header_that_lies_is_refused_before_anything_is_buffered() {
        // Taille de trame énorme.
        let mut huge = vec![1, 0, 0, 0];
        huge.extend_from_slice(&u32::MAX.to_be_bytes());
        let (result, _) = read_all(chunked(&[&huge]), 4096, FOLLOW).await;
        assert_eq!(result.expect_err("refusé"), EngineError::Refused);
        // Type de flux inconnu.
        let (result, _) = read_all(chunked(&[&frame(9, "x")]), 4096, FOLLOW).await;
        assert_eq!(result.expect_err("refusé"), EngineError::Refused);
        // Octets de remplissage non nuls.
        let (result, _) = read_all(chunked(&[&[1, 7, 0, 0, 0, 0, 0, 1, b'x']]), 4096, FOLLOW).await;
        assert_eq!(result.expect_err("refusé"), EngineError::Refused);
    }

    #[tokio::test]
    async fn a_chunk_size_line_that_never_ends_is_refused() {
        let mut reply = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
        reply.extend(std::iter::repeat_n(b'f', 10_000));
        let (result, _) = read_all(reply, 4096, FOLLOW).await;
        assert_eq!(result.expect_err("refusé"), EngineError::Refused);
    }

    #[test]
    fn the_request_names_only_safe_things_and_rounds_since_down_to_the_second() {
        let query = LogQuery {
            follow: true,
            tail: None,
            since_ms: Some(1_791_885_601_999),
            tty: false,
        };
        assert_eq!(
            request("abc", query),
            "GET /containers/abc/logs?stdout=1&stderr=1&timestamps=1&follow=1&tail=all&since=1791885601 HTTP/1.1\r\nHost: docker\r\nConnection: close\r\nAccept: */*\r\n\r\n"
        );
        assert!(is_safe_id("0123abcd"));
        assert!(is_safe_id("my_container-1.0"));
        for bad in ["", "a b", "a/b", "a?x=1", "a\r\nHost: x", "a%2f"] {
            assert!(!is_safe_id(bad), "{bad:?}");
        }
    }

    #[tokio::test]
    async fn an_unsafe_identifier_never_reaches_the_socket() {
        let (conn, _task) = serve(Vec::new(), 1).await;
        let result = Reader::open_over(conn, "x\r\nHost: y", FOLLOW).await;
        assert_eq!(result.err(), Some(EngineError::NotFound));
    }
}
