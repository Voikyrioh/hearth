//! Transferts reprenables entre le client et l'agent (HRT-95, ADR-0048, BR-FILES-019, BR-MOD-015).
//!
//! Un fichier de toute taille arrive dans la **zone d'attente** de l'agent (hors de tout dossier de service) :
//!
//! - `POST /deposits` (acte `FileWrite` `0x16`, couvert par l'élévation, droit « gérer les services ») ouvre un
//!   dépôt : `201 { transfer_id, chunk_bytes, window, expires_at }` ;
//! - `GET /transfers/{id}` : le WebSocket du transfert (`auth`, `open`, puis des trames binaires de 4 Mio
//!   vérifiées une à une) ;
//! - `GET /deposits/{id}` : l'état du dépôt, pour le retrouver après une coupure ;
//! - `DELETE /transfers/{id}` : abandonne le dépôt et retire la partie reçue.
//!
//! **Un message de texte** est un JSON de 4 096 octets au plus ; **une trame binaire** fait `CHUNK_BYTES` de
//! données plus [`FRAME_HEADER_BYTES`] d'en-tête au plus : `[index u64 big-endian][SHA-256 du morceau][données]`.
//! L'empreinte de l'ensemble, `tree_digest`, est le SHA-256 de la suite des empreintes de morceaux ([`chunk_digest`]) :
//! elle se calcule au fil de l'eau des deux côtés et se reprend sans relire le fichier.
//!
//! Rien d'ici ne porte un nom de fichier du client au-delà de l'ouverture, ni un chemin du serveur. Évolution :
//! toute énumération lue tolère une valeur inconnue (`Unknown`), tout champ ajouté plus tard est facultatif.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Capacité annoncée par `GET /hello` quand l'agent sait recevoir un transfert (HRT-95).
pub use crate::api::service_ops::CAPABILITY_SERVICE_TRANSFERS;

/// Taille d'un morceau : 4 Mio de données (le dernier peut être plus court).
pub const CHUNK_BYTES: u32 = 4 * 1024 * 1024;
/// Combien de morceaux non accusés l'émetteur a au plus en vol (8, soit 32 Mio).
pub const WINDOW_CHUNKS: u32 = 8;
/// Taille d'une empreinte SHA-256.
pub const DIGEST_LEN: usize = 32;
/// L'en-tête d'une trame binaire : l'index du morceau (8 octets) et son empreinte (32 octets).
pub const FRAME_HEADER_BYTES: usize = 8 + DIGEST_LEN;
/// Une trame binaire au plus : un morceau plein et 64 octets d'en-tête (marge d'évolution).
pub const MAX_BINARY_FRAME_BYTES: usize = CHUNK_BYTES as usize + 64;
/// Un message de texte (JSON) du socket de transfert au plus.
pub const MAX_TEXT_FRAME_BYTES: usize = 4096;
/// Un envoi interrompu se reprend pendant 24 heures, après quoi la partie reçue est supprimée (BR-FILES-019).
pub const RESUME_WINDOW_S: u64 = 24 * 60 * 60;
/// Un socket muet pendant 60 secondes est fermé ; l'envoi reste reprenable.
pub const SOCKET_IDLE_S: u64 = 60;
/// Transferts en cours (un socket ouvert) pour tout l'agent.
pub const MAX_ACTIVE_TRANSFERS: usize = 4;
/// Transferts en cours pour un même compte.
pub const MAX_ACTIVE_PER_ACCOUNT: usize = 2;
/// Dépôts ouverts non terminés (reprenables ou en attente d'un consommateur) pour un même compte : une borne
/// de la zone d'attente, pas du débit.
pub const MAX_OPEN_PER_ACCOUNT: usize = 8;
/// Le nom d'un fichier : 1 à 255 octets (BR-FILES-005).
pub const NAME_MAX_BYTES: usize = 255;
/// Espace à laisser libre après toute écriture : le plus grand de 1 Gio et de 5 % du disque (BR-FILES-013).
pub const RESERVE_MIN_BYTES: u64 = 1 << 30;
/// La part du disque à laisser libre, en centièmes (5 %).
pub const RESERVE_PERCENT: u64 = 5;

/// Le nombre de morceaux d'un fichier de `size` octets (zéro octet : zéro morceau).
pub const fn chunk_count(size: u64) -> u64 {
    size.div_ceil(CHUNK_BYTES as u64)
}

/// La taille du morceau `index` d'un fichier de `size` octets ; `0` au-delà du dernier.
pub const fn chunk_len(size: u64, index: u64) -> u64 {
    let start = index.saturating_mul(CHUNK_BYTES as u64);
    if start >= size {
        0
    } else if size - start > CHUNK_BYTES as u64 {
        CHUNK_BYTES as u64
    } else {
        size - start
    }
}

/// L'empreinte d'un morceau.
pub fn digest_of(chunk: &[u8]) -> [u8; DIGEST_LEN] {
    Sha256::digest(chunk).into()
}

/// L'empreinte de la suite des empreintes de morceaux (`tree_digest`) : source unique de l'agent et de la liaison.
pub fn chunk_digest(digests: &[[u8; DIGEST_LEN]]) -> [u8; DIGEST_LEN] {
    let mut hasher = TreeHasher::new();
    for digest in digests {
        hasher.push(digest);
    }
    hasher.finish()
}

/// `chunk_digest` au fil de l'eau : on y pousse l'empreinte de chaque morceau, dans l'ordre.
#[derive(Clone, Default)]
pub struct TreeHasher(Sha256);

impl TreeHasher {
    pub fn new() -> Self {
        Self(Sha256::new())
    }

    pub fn push(&mut self, chunk_digest: &[u8; DIGEST_LEN]) {
        self.0.update(chunk_digest);
    }

    pub fn finish(self) -> [u8; DIGEST_LEN] {
        self.0.finalize().into()
    }
}

/// Pourquoi une trame binaire est refusée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    /// Plus courte que l'en-tête.
    Short,
    /// Plus longue que `MAX_BINARY_FRAME_BYTES`.
    Long,
}

/// Une trame binaire lue : l'index, l'empreinte annoncée et les données.
#[derive(Debug, PartialEq, Eq)]
pub struct Frame<'a> {
    pub index: u64,
    pub digest: [u8; DIGEST_LEN],
    pub data: &'a [u8],
}

/// Écrit une trame binaire : `[index][empreinte][données]`.
pub fn encode_frame(index: u64, digest: &[u8; DIGEST_LEN], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(FRAME_HEADER_BYTES + data.len());
    out.extend_from_slice(&index.to_be_bytes());
    out.extend_from_slice(digest);
    out.extend_from_slice(data);
    out
}

/// Lit une trame binaire sans la copier.
pub fn decode_frame(bytes: &[u8]) -> Result<Frame<'_>, FrameError> {
    if bytes.len() > MAX_BINARY_FRAME_BYTES {
        return Err(FrameError::Long);
    }
    if bytes.len() < FRAME_HEADER_BYTES {
        return Err(FrameError::Short);
    }
    let (head, data) = bytes.split_at(FRAME_HEADER_BYTES);
    let mut index = [0u8; 8];
    index.copy_from_slice(&head[..8]);
    let mut digest = [0u8; DIGEST_LEN];
    digest.copy_from_slice(&head[8..]);
    Ok(Frame {
        index: u64::from_be_bytes(index),
        digest,
        data,
    })
}

/// Une empreinte en hexadécimal minuscule.
pub fn hex_of(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Relit une empreinte écrite par [`hex_of`] (64 chiffres, majuscules acceptées).
pub fn digest_from_hex(text: &str) -> Option<[u8; DIGEST_LEN]> {
    if text.len() != DIGEST_LEN * 2 || !text.is_ascii() {
        return None;
    }
    let mut out = [0u8; DIGEST_LEN];
    for (i, pair) in text.as_bytes().chunks(2).enumerate() {
        let high = char::from(pair[0]).to_digit(16)?;
        let low = char::from(pair[1]).to_digit(16)?;
        out[i] = u8::try_from(high * 16 + low).ok()?;
    }
    Some(out)
}

/// Corps de `POST /deposits`. Le membre `reauth` voyage à côté.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DepositRequest {
    /// Le dernier nom du fichier choisi (jamais un chemin) : sert à juger la demande et à la signer ; l'agent ne le
    /// garde pas (BR-MOD-015 : ce qui décide du genre d'un fichier est sa lecture, jamais son nom).
    pub name: String,
    /// La taille exacte du fichier, en octets (au moins 1).
    pub size: u64,
}

/// Réponse de `POST /deposits` (`201`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DepositOpened {
    pub transfer_id: String,
    pub chunk_bytes: u32,
    pub window: u32,
    /// Fin de la reprise possible (RFC 3339, UTC) : 24 heures après la dernière activité.
    pub expires_at: String,
}

/// Où en est un dépôt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DepositState {
    /// Des morceaux manquent : le client peut reprendre.
    Receiving,
    /// Tout est reçu et vérifié ; le fichier attend son consommateur dans la zone d'attente.
    Complete,
    #[serde(other)]
    Unknown,
}

/// Réponse de `GET /deposits/{id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DepositView {
    pub transfer_id: String,
    pub state: DepositState,
    pub size: u64,
    /// Octets reçus ET écrits (le prochain morceau attendu commence ici).
    pub received_bytes: u64,
    pub expires_at: String,
    /// Pour un dépôt complet : `tree_digest` en hexadécimal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree_digest: Option<String>,
}

/// Réponse de `DELETE /transfers/{id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferAbandoned {
    pub removed: bool,
}

/// Messages du client vers l'agent sur le socket d'un envoi (texte).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UploadClientMessage {
    /// Ouvre l'envoi (ou le reprend : l'agent annonce son offset durable).
    Open {},
    /// Tout est envoyé : l'empreinte de la suite des empreintes de morceaux, en hexadécimal.
    Finish {
        tree_digest: String,
    },
    Ping {
        n: u64,
    },
}

/// Les erreurs du socket d'un transfert. Un code, jamais un texte du système de fichiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransferErrorCode {
    /// Jeton refusé, ou premier message qui n'est pas `auth`.
    Unauthenticated,
    /// Message illisible, trame trop grande, ou `open` attendu et absent.
    BadRequest,
    /// Ce transfert n'existe pas, n'est pas à ce compte, ou n'est plus un envoi en cours.
    TransferNotFound,
    /// Plus rien à reprendre : 24 heures sans activité, la partie reçue est supprimée.
    TransferExpired,
    /// Quatre transferts en cours pour l'agent, ou deux pour ce compte.
    TransferLimit,
    /// Le compte n'a plus le droit de gérer les services.
    MissingRight,
    /// Le disque n'a plus la place d'écrire la suite (`missing_bytes`, `free_after_bytes`).
    NoSpace,
    /// Un morceau reçu ne correspond pas à son empreinte : il n'est pas écrit ; reprendre renvoie ce morceau.
    ChunkCorrupt,
    /// Un morceau hors d'ordre, de mauvaise taille ou au-delà de la fenêtre.
    ChunkSequence,
    /// `finish` avant la fin, ou l'empreinte de l'ensemble ne correspond pas.
    TreeMismatch,
    /// L'écriture a échoué sur le serveur.
    WriteFailed,
    /// Un code que ce client ne connaît pas : une erreur terminale générique.
    #[serde(other)]
    Unknown,
}

/// L'erreur d'un socket de transfert, avec de quoi dire quoi faire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferError {
    pub code: TransferErrorCode,
    /// `NO_SPACE` : ce qui manque pour écrire la suite.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missing_bytes: Option<u64>,
    /// `NO_SPACE` : ce qu'il resterait de libre.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_after_bytes: Option<u64>,
    /// `CHUNK_CORRUPT`, `CHUNK_SEQUENCE` : l'index du morceau attendu.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_index: Option<u64>,
}

impl TransferError {
    pub const fn new(code: TransferErrorCode) -> Self {
        Self {
            code,
            missing_bytes: None,
            free_after_bytes: None,
            expected_index: None,
        }
    }
}

/// Messages de l'agent vers le client sur le socket d'un envoi (texte).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UploadServerMessage {
    /// L'envoi est ouvert : reprendre à `offset` (octets durables, toujours multiple de `chunk_bytes` hors fin),
    /// avec `window` morceaux en vol au plus.
    Ready {
        offset: u64,
        window: u32,
    },
    /// Un morceau est vérifié ET écrit : `offset` octets sont durables ; la fenêtre se rouvre.
    Ack {
        offset: u64,
        window: u32,
    },
    /// L'envoi est complet : le fichier est dans la zone d'attente. `verified` : l'empreinte de l'ensemble
    /// correspond (toujours vrai ici : un envoi qui ne correspond pas est une erreur `TREE_MISMATCH`).
    Done {
        size: u64,
        verified: bool,
        tree_digest: String,
    },
    Pong {
        n: u64,
    },
    Error(TransferError),
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_cover_the_file_exactly() {
        assert_eq!(chunk_count(0), 0);
        assert_eq!(chunk_count(1), 1);
        assert_eq!(chunk_count(u64::from(CHUNK_BYTES)), 1);
        assert_eq!(chunk_count(u64::from(CHUNK_BYTES) + 1), 2);
        let size = u64::from(CHUNK_BYTES) * 3 + 17;
        let total: u64 = (0..chunk_count(size)).map(|i| chunk_len(size, i)).sum();
        assert_eq!(total, size);
        assert_eq!(chunk_len(size, 3), 17);
        assert_eq!(chunk_len(size, 4), 0);
        // Pas de dépassement près de la borne.
        assert_eq!(chunk_len(u64::MAX, u64::MAX), 0);
    }

    #[test]
    fn the_tree_digest_is_the_digest_of_the_chunk_digests_and_resumes_from_the_list() {
        let a = digest_of(b"a");
        let b = digest_of(b"b");
        let c = digest_of(b"c");
        let mut flat = Vec::new();
        for d in [a, b, c] {
            flat.extend_from_slice(&d);
        }
        let expected: [u8; 32] = Sha256::digest(&flat).into();
        assert_eq!(chunk_digest(&[a, b, c]), expected);
        // Au fil de l'eau, après une reprise depuis la liste gardée.
        let mut hasher = TreeHasher::new();
        for d in [a, b] {
            hasher.push(&d);
        }
        hasher.push(&c);
        assert_eq!(hasher.finish(), expected);
        // L'ordre compte.
        assert_ne!(chunk_digest(&[b, a, c]), expected);
    }

    #[test]
    fn a_frame_round_trips_and_refuses_what_is_too_short_or_too_long() {
        let digest = digest_of(b"data");
        let bytes = encode_frame(7, &digest, b"data");
        let frame = decode_frame(&bytes).expect("trame");
        assert_eq!(frame.index, 7);
        assert_eq!(frame.digest, digest);
        assert_eq!(frame.data, b"data");
        assert_eq!(
            decode_frame(&bytes[..FRAME_HEADER_BYTES - 1]),
            Err(FrameError::Short)
        );
        assert_eq!(
            decode_frame(&vec![0u8; MAX_BINARY_FRAME_BYTES + 1]),
            Err(FrameError::Long)
        );
        // Une trame sans donnée est lisible (le jugement revient à l'agent).
        assert!(
            decode_frame(&bytes[..FRAME_HEADER_BYTES])
                .expect("vide")
                .data
                .is_empty()
        );
    }

    #[test]
    fn hex_digests_round_trip_and_refuse_garbage() {
        let digest = digest_of(b"x");
        assert_eq!(digest_from_hex(&hex_of(&digest)), Some(digest));
        assert_eq!(
            digest_from_hex(&hex_of(&digest).to_ascii_uppercase()),
            Some(digest)
        );
        assert_eq!(digest_from_hex("zz"), None);
        assert_eq!(digest_from_hex(&"g".repeat(64)), None);
        // Des caractères sur plusieurs octets ne font jamais paniquer le découpage.
        assert_eq!(digest_from_hex(&"é".repeat(32)), None);
    }

    #[test]
    fn socket_messages_use_their_wire_names_and_tolerate_the_future() {
        let ready = serde_json::to_value(UploadServerMessage::Ready {
            offset: 8,
            window: 8,
        })
        .unwrap();
        assert_eq!(
            ready,
            serde_json::json!({ "type": "ready", "offset": 8, "window": 8 })
        );
        let error = serde_json::to_value(UploadServerMessage::Error(TransferError {
            code: TransferErrorCode::NoSpace,
            missing_bytes: Some(5),
            free_after_bytes: Some(1),
            expected_index: None,
        }))
        .unwrap();
        assert_eq!(
            error,
            serde_json::json!({ "type": "error", "code": "NO_SPACE", "missing_bytes": 5, "free_after_bytes": 1 })
        );
        assert_eq!(
            serde_json::from_value::<UploadServerMessage>(serde_json::json!({ "type": "later" }))
                .unwrap(),
            UploadServerMessage::Unknown
        );
        assert_eq!(
            serde_json::from_str::<UploadClientMessage>(r#"{"type":"open"}"#).unwrap(),
            UploadClientMessage::Open {}
        );
        assert!(serde_json::from_str::<UploadClientMessage>(r#"{"type":"later"}"#).is_err());
    }
}
