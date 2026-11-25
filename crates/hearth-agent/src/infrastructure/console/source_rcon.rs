//! Client du protocole RCON « Source » (HRT-267, ADR-0067 D8, BR-CONSOLE-001, 007) : Minecraft, Valheim... parlent ce
//! protocole de quatorze octets d'en-tête, aucune crate de plus.
//!
//! Paquet : `[longueur i32 LE][identifiant i32 LE][type i32 LE][corps][0][0]`, la longueur comptant tout après elle
//! (`10 + corps`). Types : `3` authentification, `2` réponse d'authentification (côté serveur) ET commande (côté
//! client), `0` réponse à une commande.
//!
//! Le conteneur est hostile : tout est borné. Longueur annoncée de 10 à 4 110 octets, sinon coupure ; corps de
//! 4 096 octets par paquet ; réponse totale de 16 Kio (coupée, `truncated`) ; fin de réponse = aucun octet pendant
//! `quiet` après le dernier paquet complet ; délai total `reply`. Un paquet d'un autre identifiant ou d'un autre
//! type est ignoré (puis le délai joue). Le texte est décodé en UTF-8 avec remplacement, les codes `§x` retirés, les
//! caractères de contrôle remplacés par une espace sauf `\n`. Ni la ligne, ni le mot de passe, ni la réponse ne
//! sont tracés.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use zeroize::Zeroize;

use crate::application::ports::{
    ConsoleBudget, ConsoleError, ConsoleReply, GameConsole, MAX_REPLY_BYTES,
};
use crate::domain::secret::Secret;

const TYPE_RESPONSE: i32 = 0;
const TYPE_COMMAND: i32 = 2;
const TYPE_AUTH: i32 = 3;
/// La longueur annoncée d'un paquet : au moins l'en-tête et les deux zéros, au plus 10 plus 4 100 octets.
const MIN_LEN: i32 = 10;
const MAX_LEN: i32 = 4110;

pub struct SourceRcon;

/// Un paquet lu.
struct Packet {
    id: i32,
    kind: i32,
    body: Vec<u8>,
}

fn encode(id: i32, kind: i32, body: &[u8]) -> Vec<u8> {
    let len = i32::try_from(10 + body.len()).unwrap_or(i32::MAX);
    let mut out = Vec::with_capacity(14 + body.len());
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&id.to_le_bytes());
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(body);
    out.extend_from_slice(&[0, 0]);
    out
}

/// Lit UN paquet complet, jusqu'à `deadline`. `Err(None)` : le délai est passé ; `Err(Some(e))` : l'échange est coupé.
async fn read_packet(
    stream: &mut TcpStream,
    deadline: Instant,
) -> Result<Packet, Option<ConsoleError>> {
    let remaining = |deadline: Instant| deadline.saturating_duration_since(Instant::now());
    let mut head = [0u8; 4];
    match tokio::time::timeout(remaining(deadline), stream.read_exact(&mut head)).await {
        Err(_) => return Err(None),
        Ok(Err(_)) => return Err(Some(ConsoleError::Unreachable)),
        Ok(Ok(_)) => {}
    }
    let len = i32::from_le_bytes(head);
    if !(MIN_LEN..=MAX_LEN).contains(&len) {
        return Err(Some(ConsoleError::Protocol));
    }
    let len = usize::try_from(len).map_err(|_| Some(ConsoleError::Protocol))?;
    let mut rest = vec![0u8; len];
    match tokio::time::timeout(remaining(deadline), stream.read_exact(&mut rest)).await {
        Err(_) => return Err(None),
        Ok(Err(_)) => return Err(Some(ConsoleError::Unreachable)),
        Ok(Ok(_)) => {}
    }
    let id = i32::from_le_bytes([rest[0], rest[1], rest[2], rest[3]]);
    let kind = i32::from_le_bytes([rest[4], rest[5], rest[6], rest[7]]);
    // Le corps s'arrête au premier zéro (le dernier octet et l'avant-dernier sont les deux zéros de fin).
    let body = rest[8..len.saturating_sub(2).max(8)].to_vec();
    Ok(Packet { id, kind, body })
}

/// Le texte d'une réponse : UTF-8 avec remplacement, codes `§x` retirés, contrôles remplacés par une espace sauf `\n`.
fn clean(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{a7}' {
            // Un code de couleur : `§` et le caractère qui le suit.
            let _ = chars.next();
            continue;
        }
        if c == '\n' {
            out.push('\n');
        } else if c.is_control() || hearth_proto::console::is_risky_char(c) {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

#[async_trait]
impl GameConsole for SourceRcon {
    async fn exchange(
        &self,
        address: SocketAddr,
        secret: &Secret,
        line: &str,
        budget: ConsoleBudget,
    ) -> Result<ConsoleReply, ConsoleError> {
        let mut stream = tokio::time::timeout(budget.connect, TcpStream::connect(address))
            .await
            .map_err(|_| ConsoleError::Unreachable)?
            .map_err(|_| ConsoleError::Unreachable)?;
        let auth_id: i32 = 1 + i32::from(rand_byte());
        let command_id: i32 = 1000 + i32::from(rand_byte());
        // Authentification : le mot de passe passe dans le paquet, puis le tampon est effacé.
        let mut auth = encode(auth_id, TYPE_AUTH, secret.expose().as_bytes());
        let sent = stream.write_all(&auth).await;
        auth.zeroize();
        sent.map_err(|_| ConsoleError::Unreachable)?;
        let auth_deadline = Instant::now() + budget.reply.min(Duration::from_secs(10));
        loop {
            let packet = match read_packet(&mut stream, auth_deadline).await {
                Ok(packet) => packet,
                Err(None) => return Err(ConsoleError::Unreachable),
                Err(Some(error)) => return Err(error),
            };
            // Une réponse vide de type 0 précède parfois celle de l'authentification : ignorée.
            if packet.kind != TYPE_COMMAND {
                continue;
            }
            if packet.id == -1 {
                return Err(ConsoleError::AuthRefused);
            }
            if packet.id == auth_id {
                break;
            }
        }
        // La commande.
        stream
            .write_all(&encode(command_id, TYPE_COMMAND, line.as_bytes()))
            .await
            .map_err(|_| ConsoleError::Unreachable)?;
        let deadline = Instant::now() + budget.reply;
        let mut collected: Vec<u8> = Vec::new();
        let mut truncated = false;
        let mut answered = false;
        loop {
            // Après un paquet complet, la fin de la réponse est le silence de `quiet` ; avant, le délai total.
            let until = if answered {
                deadline.min(Instant::now() + budget.quiet)
            } else {
                deadline
            };
            match read_packet(&mut stream, until).await {
                Ok(packet) => {
                    if packet.kind != TYPE_RESPONSE || packet.id != command_id {
                        continue;
                    }
                    answered = true;
                    let room = MAX_REPLY_BYTES.saturating_sub(collected.len());
                    if packet.body.len() > room {
                        collected.extend_from_slice(&packet.body[..room]);
                        truncated = true;
                        break;
                    }
                    collected.extend_from_slice(&packet.body);
                }
                Err(None) => {
                    if answered {
                        break;
                    }
                    return Err(ConsoleError::NoAnswer);
                }
                Err(Some(error)) => {
                    // Une coupure après une réponse reçue : ce qui a été reçu est rendu.
                    if answered && error == ConsoleError::Unreachable {
                        break;
                    }
                    return Err(error);
                }
            }
        }
        let _ = stream.shutdown().await;
        Ok(ConsoleReply {
            text: clean(&collected),
            truncated,
        })
    }
}

/// Un octet au hasard pour les identifiants de paquet (jamais un secret : l'identifiant est en clair sur le fil).
fn rand_byte() -> u8 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(7);
    u8::try_from(nanos % 251).unwrap_or(7)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_packet_is_encoded_with_its_length_id_type_body_and_two_zeros() {
        let bytes = encode(7, TYPE_COMMAND, b"list");
        assert_eq!(&bytes[..4], &14i32.to_le_bytes());
        assert_eq!(&bytes[4..8], &7i32.to_le_bytes());
        assert_eq!(&bytes[8..12], &2i32.to_le_bytes());
        assert_eq!(&bytes[12..], b"list\0\0");
    }

    #[test]
    fn a_reply_loses_its_colour_codes_and_its_control_characters() {
        assert_eq!(
            clean("\u{a7}cRouge\u{a7}r normal".as_bytes()),
            "Rouge normal"
        );
        assert_eq!(clean(b"a\tb\x07c\nd"), "a b c\nd");
        assert_eq!(clean(&[0xff, 0xfe, b'x']), "\u{fffd}\u{fffd}x");
        assert_eq!(clean("a\u{202e}b".as_bytes()), "a b");
    }
}
