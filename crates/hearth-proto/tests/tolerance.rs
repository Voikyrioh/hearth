//! Garde de tolérance du protocole (FIX:01M4VR6Y4Q4AVQVT9FCME7Z2SB, BR-COMPAT-001).
//!
//! L'agent et le client évoluent séparément : chaque côté TOLÈRE ce qu'il ne connaît pas dans ce qu'il LIT
//! de l'autre. Ce test échoue si une énumération du protocole refuse une valeur inventée, si une structure
//! lue refuse un champ en trop, ou si un type nouveau n'a pas été classé.
//!
//! Mécanisme :
//! 1. le code source de `src/` est lu : toute `pub enum` qui dérive `Deserialize` doit figurer dans `ENUMS`,
//!    et toute entrée de `ENUMS` doit exister (liste tenue à la main, contrôlée dans les deux sens) ;
//! 2. chaque entrée est jouée avec une valeur inventée (`"__valeur_du_futur__"`, ou un `type` inventé pour
//!    un message à champ `type`) : `Tolerant` doit la lire ET tomber sur la variante `Unknown` ;
//!    `Strict` (ce que l'AGENT lit du client : une commande) et `Cascade` (enveloppe lue après examen du
//!    `type`) doivent la refuser, ce qui force un choix conscient quand ça change ;
//! 3. aucune structure ne refuse un champ inconnu : ni `deny_unknown_fields`, ni `Deserialize` écrit à la
//!    main dans `hearth-proto`, `hearth-link` et le client Tauri ; quelques
//!    structures sont aussi jouées avec un champ en trop.
//!
//! Une valeur ajoutée à une énumération « Tolérante » ne casse donc jamais un client installé ; une
//! commande inconnue reste un refus propre (l'agent ne devine jamais une action).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fmt::Debug;
use std::path::{Path, PathBuf};

use hearth_proto::admin_act::TrustLevel;
use hearth_proto::api::accounts::{AccountInfo, RoleName};
use hearth_proto::api::audit::{OriginKindName, OutcomeName};
use hearth_proto::api::deploy::{DeployPlace, DeployProtocol, MountStatus, NameStatus, PortStatus};
use hearth_proto::api::metrics::HistoryWindow;
use hearth_proto::api::operations::OperationStatus as HttpOperationStatus;
use hearth_proto::api::reauth::ReauthMode;
use hearth_proto::api::security::{AttackModeEnd, AttackModeState, SessionDevice};
use hearth_proto::api::service_logs::{
    GapReason, LogSource, LogState, LogsClientMessage, LogsErrorCode, LogsServerMessage, MarkerKind,
};
use hearth_proto::api::service_ops::{
    OperationCause, OperationChoice, OperationKind, OperationStatus as OpsOperationStatus,
    StepStateView,
};
use hearth_proto::api::service_spec::DriftView;
use hearth_proto::api::services::{
    AdoptionResult, EngineState, InitSystem, MountKind, PortProtocol, PortScope, RestartMode,
    ServiceDriver, ServiceOrigin, ServiceState,
};
use hearth_proto::api::sessions::{ChallengePurpose, DeviceStatus};
use hearth_proto::api::update::{UpdateOutcome, UpdateProgress, UpdateReason, UpdateStep};
use hearth_proto::error::{ErrorBody, ErrorCode, UpgradeTarget};
use hearth_proto::stream::{
    ClientMessage, SecurityMessage, ServerMessage, ServicesMessage, SessionNotice, SignedAuth,
    Topic, UpdateMessage,
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

/// La valeur que le futur pourrait envoyer.
const FUTURE: &str = "__valeur_du_futur__";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Lue de l'autre côté : une valeur inconnue se lit `Unknown` (jamais un rejet).
    Tolerant,
    /// Lue par l'AGENT depuis le client (une commande) : l'inconnu est refusé au décodage.
    Strict,
    /// Enveloppe d'un message du flux, lue seulement après examen du `type` (voir `decode_frame` de
    /// `hearth-link`) : l'inconnu est refusé ici et retombe sur `ServerMessage::Unknown`.
    Cascade,
}

struct Entry {
    /// `fichier::Type`, le fichier relatif à `src/`.
    key: &'static str,
    kind: Kind,
    /// Le message est à champ `type` (l'inconnu s'écrit `{"type": …}`).
    tagged: bool,
    read: fn(Value) -> Result<String, String>,
}

fn read<T: DeserializeOwned + Debug>(value: Value) -> Result<String, String> {
    serde_json::from_value::<T>(value)
        .map(|parsed| format!("{parsed:?}"))
        .map_err(|error| error.to_string())
}

macro_rules! entry {
    ($key:literal, $kind:ident, $ty:ty) => {
        Entry {
            key: $key,
            kind: Kind::$kind,
            tagged: false,
            read: read::<$ty>,
        }
    };
    ($key:literal, $kind:ident, tagged $ty:ty) => {
        Entry {
            key: $key,
            kind: Kind::$kind,
            tagged: true,
            read: read::<$ty>,
        }
    };
}

/// TOUTES les énumérations de `hearth-proto` qui dérivent `Deserialize`. Ajouter une énumération au
/// protocole = l'ajouter ici (le test échoue sinon) et lui donner sa variante de repli.
fn enums() -> Vec<Entry> {
    vec![
        // Ce que le CLIENT lit de l'agent (réponses, flux, erreurs, événements).
        entry!("api/accounts.rs::RoleName", Tolerant, RoleName),
        entry!("api/audit.rs::OriginKindName", Tolerant, OriginKindName),
        entry!("api/audit.rs::OutcomeName", Tolerant, OutcomeName),
        entry!("api/metrics.rs::HistoryWindow", Tolerant, HistoryWindow),
        entry!(
            "api/operations.rs::OperationStatus",
            Tolerant,
            HttpOperationStatus
        ),
        entry!("api/reauth.rs::ReauthMode", Tolerant, ReauthMode),
        entry!("admin_act.rs::TrustLevel", Tolerant, TrustLevel),
        entry!(
            "api/security.rs::AttackModeState",
            Tolerant,
            AttackModeState
        ),
        entry!("api/security.rs::AttackModeEnd", Tolerant, AttackModeEnd),
        entry!("api/security.rs::SessionDevice", Tolerant, SessionDevice),
        entry!("api/service_logs.rs::LogSource", Tolerant, LogSource),
        entry!("api/service_logs.rs::LogState", Tolerant, LogState),
        entry!("api/service_logs.rs::GapReason", Tolerant, GapReason),
        entry!("api/service_logs.rs::MarkerKind", Tolerant, MarkerKind),
        entry!(
            "api/service_logs.rs::LogsErrorCode",
            Tolerant,
            LogsErrorCode
        ),
        entry!("api/service_logs.rs::LogsServerMessage", Tolerant, tagged LogsServerMessage),
        entry!("api/service_ops.rs::OperationKind", Tolerant, OperationKind),
        entry!(
            "api/service_ops.rs::OperationStatus",
            Tolerant,
            OpsOperationStatus
        ),
        entry!(
            "api/service_ops.rs::OperationChoice",
            Tolerant,
            OperationChoice
        ),
        entry!(
            "api/service_ops.rs::OperationCause",
            Tolerant,
            OperationCause
        ),
        entry!("api/service_ops.rs::StepStateView", Tolerant, StepStateView),
        entry!("api/service_spec.rs::DriftView", Tolerant, DriftView),
        entry!("api/deploy.rs::NameStatus", Tolerant, NameStatus),
        entry!("api/deploy.rs::PortStatus", Tolerant, PortStatus),
        entry!("api/deploy.rs::MountStatus", Tolerant, MountStatus),
        entry!("api/services.rs::EngineState", Tolerant, EngineState),
        entry!("api/services.rs::InitSystem", Tolerant, InitSystem),
        entry!("api/services.rs::ServiceState", Tolerant, ServiceState),
        entry!("api/services.rs::ServiceOrigin", Tolerant, ServiceOrigin),
        entry!("api/services.rs::ServiceDriver", Tolerant, ServiceDriver),
        entry!("api/services.rs::PortProtocol", Tolerant, PortProtocol),
        entry!("api/services.rs::PortScope", Tolerant, PortScope),
        entry!("api/services.rs::MountKind", Tolerant, MountKind),
        entry!("api/services.rs::RestartMode", Tolerant, RestartMode),
        entry!("api/services.rs::AdoptionResult", Tolerant, AdoptionResult),
        entry!("api/sessions.rs::DeviceStatus", Tolerant, DeviceStatus),
        entry!("api/update.rs::UpdateStep", Tolerant, UpdateStep),
        entry!("api/update.rs::UpdateOutcome", Tolerant, UpdateOutcome),
        entry!("api/update.rs::UpdateReason", Tolerant, UpdateReason),
        entry!("error.rs::ErrorCode", Tolerant, ErrorCode),
        entry!("error.rs::UpgradeTarget", Tolerant, UpgradeTarget),
        entry!("stream.rs::SessionNotice", Tolerant, SessionNotice),
        entry!("stream.rs::ServerMessage", Tolerant, tagged ServerMessage),
        // Lus par l'agent ET par le client (la même énumération sert aux deux sens) : le décodage tolère,
        // c'est le HANDLER de l'agent qui refuse l'inconnu (jamais une action par défaut) ; voir
        // les tests `entrypoint::http::reauth::tests`, `entrypoint::http::wire::tests` et `application::trust::tests` de l'agent.
        entry!(
            "api/sessions.rs::ChallengePurpose",
            Tolerant,
            ChallengePurpose
        ),
        // Ce que l'AGENT lit du client : des commandes. Un inconnu est un refus du message, jamais une action.
        entry!("stream.rs::Topic", Strict, Topic),
        // Déployer (HRT-68) : le protocole d'un port et l'emplacement d'un dossier sont des champs de commande ;
        // un inconnu est refusé au décodage, jamais remplacé par un défaut.
        entry!("api/deploy.rs::DeployProtocol", Strict, DeployProtocol),
        entry!("api/deploy.rs::DeployPlace", Strict, DeployPlace),
        entry!("stream.rs::ClientMessage", Strict, tagged ClientMessage),
        entry!("stream.rs::SignedAuth", Strict, tagged SignedAuth),
        entry!("api/service_logs.rs::LogsClientMessage", Strict, tagged LogsClientMessage),
        // Enveloppes du flux lues après examen du `type` par `hearth-link::decode_frame`.
        entry!("stream.rs::UpdateMessage", Cascade, tagged UpdateMessage),
        entry!("stream.rs::SecurityMessage", Cascade, tagged SecurityMessage),
        entry!("stream.rs::ServicesMessage", Cascade, tagged ServicesMessage),
    ]
}

fn future_value(tagged: bool) -> Value {
    if tagged {
        json!({ "type": FUTURE })
    } else {
        json!(FUTURE)
    }
}

/// Les fichiers `.rs` sous `dir`. Un dossier attendu qui n'existe pas (arborescence déplacée) est un ÉCHEC : un
/// balayage qui ne couvre plus rien ne doit jamais rester vert.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|error| {
        panic!(
            "dossier balayé par le garde introuvable ({}) : {error}. Si l'arborescence a bougé, mets le garde à jour.",
            dir.display()
        )
    });
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Les dossiers de code qui décodent du réseau ou en dépendent, avec le nombre minimal de fichiers attendus (si
/// moins, c'est que le balayage ne voit plus le code : échec).
fn scanned_directories() -> Vec<(PathBuf, usize)> {
    let base = root();
    vec![
        (base.join("src"), 15),
        (base.join("../hearth-link/src"), 15),
        (base.join("../../apps/desktop/src-tauri/src"), 15),
    ]
}

fn files_of(directory: &Path, minimum: usize) -> Vec<PathBuf> {
    let mut files = Vec::new();
    rust_files(directory, &mut files);
    files.retain(|file| !is_test_file(file));
    assert!(
        files.len() >= minimum,
        "le garde ne voit que {} fichier(s) sous {} (au moins {minimum} attendus) : l'arborescence a bougé, le \
         balayage ne couvre plus rien",
        files.len(),
        directory.display()
    );
    files
}

/// Un type (`enum` ou `struct`) trouvé dans un fichier, avec le texte de TOUS ses attributs (une dérivation
/// repliée sur plusieurs lignes par `rustfmt`, `cfg_attr`, `serde(...)`).
struct TypeItem {
    /// `fichier::Type`, le fichier relatif au dossier balayé.
    key: String,
    is_enum: bool,
    attributes: String,
}

impl TypeItem {
    fn derives_deserialize(&self) -> bool {
        self.attributes.contains("derive") && self.attributes.contains("Deserialize")
    }
}

/// Le nom d'un type déclaré sur cette ligne, de toute visibilité (`pub`, `pub(crate)`, `pub(super)`, privée) et
/// à toute indentation (module en ligne).
fn declared_type(line: &str) -> Option<(bool, String)> {
    let mut rest = line.trim_start();
    if let Some(after) = rest.strip_prefix("pub") {
        rest = after.trim_start();
        if rest.starts_with('(') {
            rest = rest[rest.find(')')? + 1..].trim_start();
        }
    }
    let (is_enum, after) = if let Some(after) = rest.strip_prefix("enum ") {
        (true, after)
    } else {
        (false, rest.strip_prefix("struct ")?)
    };
    let name: String = after
        .trim_start()
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some((is_enum, name))
}

/// Les lignes de code de production d'un fichier : numéro (à partir de 0) et texte. Le corps d'un module de tests
/// en ligne (`#[cfg(test)]` puis `mod ... {` jusqu'à son `}` de même indentation) n'est pas lu, mais RIEN d'autre :
/// un type déclaré après ce module reste vu. Un fichier de tests (`tests.rs`, `tests_*.rs`) n'est pas du code de
/// production.
fn production_lines(text: &str) -> Vec<(usize, &str)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        if line.trim() == "#[cfg(test)]" {
            let next = lines.get(index + 1).map_or("", |next| next.trim_start());
            if next.starts_with("mod ") || next.starts_with("pub mod ") {
                if next.trim_end().ends_with(';') {
                    index += 2;
                    continue;
                }
                let indent = line.len() - line.trim_start().len();
                let closing = format!("{}}}", " ".repeat(indent));
                let mut end = index + 2;
                while end < lines.len() && lines[end] != closing {
                    end += 1;
                }
                index = end + 1;
                continue;
            }
        }
        out.push((index, line));
        index += 1;
    }
    out
}

/// Texte canonique d'un morceau de code : sans espace ni retour à la ligne, sans virgule finale ni `|` initial.
/// Deux écritures que seul `rustfmt` sépare (repli sur plusieurs lignes, virgule ajoutée, `|` en tête de ligne)
/// ont le même texte canonique ; rien d'autre ne le fait (un nom, un chemin ou une valeur qui change, si).
fn canonical(text: &str) -> String {
    let mut out: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    for (from, to) in [(",)", ")"), (",]", "]"), (",}", "}")] {
        while out.contains(from) {
            out = out.replace(from, to);
        }
    }
    out.trim_start_matches('|').trim_end_matches(',').to_owned()
}

/// Les lignes de production regroupées en instructions : une instruction repliée par `rustfmt` (parenthèses
/// ouvertes, ligne qui finit par `|` ou `=>`, ligne suivante qui commence par `|`) est une seule ligne logique.
/// Numéro de la première ligne physique (à partir de 0). Au plus 12 lignes physiques par instruction : une
/// parenthèse isolée dans une chaîne ne doit pas avaler le fichier.
fn logical_lines(text: &str) -> Vec<(usize, String)> {
    let physical = production_lines(text);
    let mut out = Vec::new();
    let mut index = 0;
    while index < physical.len() {
        let (number, first) = physical[index];
        index += 1;
        let mut joined = first.trim().to_owned();
        if joined.is_empty() || joined.starts_with("//") {
            continue;
        }
        for _ in 0..12 {
            let depth = joined.matches(['(', '[']).count() as i64
                - joined.matches([')', ']']).count() as i64;
            let next = physical.get(index).map_or("", |(_, line)| line.trim());
            // Les parenthèses ouvertes ne joignent pas un bloc (`{`) ni un bras déjà complet (`=>`) : seulement un motif replié.
            let open_pattern = depth > 0 && !joined.contains('{') && !joined.contains("=>");
            let continues = open_pattern
                || joined.ends_with('|')
                || joined.ends_with("=>")
                || (next.starts_with('|') && !next.starts_with("||"))
                || next.starts_with("=>");
            if !continues || index >= physical.len() {
                break;
            }
            if !next.starts_with("//") {
                joined.push(' ');
                joined.push_str(next);
            }
            index += 1;
        }
        out.push((number, joined));
    }
    out
}

/// Le type dont `impl` est le plus proche avant la ligne logique `before` (pour lire `Self`).
fn impl_type(lines: &[(usize, String)], before: usize) -> Option<String> {
    lines[..before].iter().rev().find_map(|(_, line)| {
        let rest = line.strip_prefix("impl")?;
        let rest = rest.trim_start();
        let rest = if rest.starts_with('<') {
            let mut depth = 0_i32;
            let mut end = 0;
            for (i, c) in rest.char_indices() {
                match c {
                    '<' => depth += 1,
                    '>' => {
                        depth -= 1;
                        if depth == 0 {
                            end = i + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            rest[end..].trim_start()
        } else {
            rest
        };
        let target = rest.split(" for ").nth(1).unwrap_or(rest).trim_start();
        let name: String = target
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        (!name.is_empty()).then_some(name)
    })
}

/// Le type dont une ligne nomme la variante `Unknown` (`Self` lu d'après l'`impl` qui précède).
fn unknown_owner(lines: &[(usize, String)], at: usize) -> String {
    let line = &lines[at].1;
    let end = line.find("::Unknown").unwrap_or(0);
    let name: String = line[..end]
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if name == "Self" {
        impl_type(lines, at).unwrap_or(name)
    } else {
        name
    }
}

/// Les lectures personnalisées d'un fichier : (ligne, type, argument canonique). Les attributs sont lus bloc par
/// bloc (crochets équilibrés, donc repliés ou non), dans `serde(...)` comme sous `cfg_attr`, argument par argument ;
/// un attribut placé avant un type est celui du type, un attribut placé avant un champ est celui du type courant.
fn serde_custom_reads(text: &str) -> Vec<(usize, String, String)> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut run: Vec<(usize, String)> = Vec::new();
    let mut attribute = String::new();
    let mut depth = 0_i64;
    let mut start = 0;
    for (number, line) in production_lines(text) {
        let trimmed = line.trim_start();
        if depth > 0 || trimmed.starts_with("#[") {
            if depth == 0 {
                start = number;
                attribute.clear();
            }
            attribute.push_str(trimmed);
            attribute.push(' ');
            depth += trimmed.matches('[').count() as i64 - trimmed.matches(']').count() as i64;
            if depth <= 0 {
                depth = 0;
                for arg in custom_args(&attribute) {
                    run.push((start + 1, arg));
                }
            }
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if let Some((_, name)) = declared_type(trimmed) {
            current = name;
        }
        for (line_number, arg) in run.drain(..) {
            out.push((line_number, current.clone(), arg));
        }
    }
    out
}

/// Les arguments `deserialize_with`, `try_from`, `with`, `remote`, `from` des `serde(...)` d'un attribut.
fn custom_args(attribute: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut rest = attribute;
    while let Some(at) = rest.find("serde(") {
        let body = &rest[at + "serde(".len()..];
        let mut depth = 1_i64;
        let mut end = body.len();
        for (i, c) in body.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i;
                        break;
                    }
                }
                _ => {}
            }
        }
        let inner = &body[..end];
        let mut level = 0_i64;
        let mut from = 0;
        let mut parts = Vec::new();
        for (i, c) in inner.char_indices() {
            match c {
                '(' | '[' => level += 1,
                ')' | ']' => level -= 1,
                ',' if level == 0 => {
                    parts.push(&inner[from..i]);
                    from = i + 1;
                }
                _ => {}
            }
        }
        parts.push(&inner[from..]);
        for part in parts {
            let key = part.split('=').next().unwrap_or("").trim();
            if matches!(
                key,
                "deserialize_with" | "try_from" | "with" | "remote" | "from"
            ) {
                args.push(canonical(part));
            }
        }
        rest = &body[end..];
    }
    args
}

fn is_test_file(file: &Path) -> bool {
    file.file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem == "tests" || stem.starts_with("tests_"))
}

/// Les types d'un fichier, avec leurs attributs complets. Les attributs sont lus bloc par bloc (crochets
/// équilibrés), donc une dérivation sur plusieurs lignes ou une visibilité inhabituelle ne les perd pas. Le
/// module de tests en ligne n'est pas lu (voir `production_lines`).
fn type_items(text: &str, relative: &str) -> Vec<TypeItem> {
    let mut items = Vec::new();
    let mut pending = String::new();
    let mut open_depth = 0_i32;
    for (_, line) in production_lines(text) {
        let trimmed = line.trim_start();
        if open_depth > 0 || trimmed.starts_with("#[") {
            pending.push_str(trimmed);
            pending.push('\n');
            open_depth += trimmed.matches('[').count() as i32 - trimmed.matches(']').count() as i32;
            open_depth = open_depth.max(0);
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if let Some((is_enum, name)) = declared_type(trimmed) {
            items.push(TypeItem {
                key: format!("{relative}::{name}"),
                is_enum,
                attributes: std::mem::take(&mut pending),
            });
        } else {
            pending.clear();
        }
    }
    items
}

fn relative_to(file: &Path, directory: &Path) -> String {
    file.strip_prefix(directory)
        .expect("préfixe")
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

/// Les types qui dérivent `Deserialize` dans un dossier, en `fichier::Type`.
fn deserialized_types(directory: &Path, minimum: usize, enums_only: bool) -> Vec<String> {
    let mut found = Vec::new();
    for file in files_of(directory, minimum) {
        let text = std::fs::read_to_string(&file).expect("lecture");
        for item in type_items(&text, &relative_to(&file, directory)) {
            if item.derives_deserialize() && (item.is_enum || !enums_only) {
                found.push(item.key);
            }
        }
    }
    found.sort();
    found
}

/// Les `enum` qui dérivent `Deserialize` dans `hearth-proto/src`, en `fichier::Type`.
fn scanned_enums() -> Vec<String> {
    deserialized_types(&root().join("src"), 15, true)
}

/// Les énumérations qui dérivent `Deserialize` dans la liaison et la coquille : elles ne sont jamais lues de
/// l'agent (le protocole est dans `hearth-proto`), mais du disque du client ou de la WebView. Chacune est déclarée
/// ici avec sa raison ; une nouvelle fait échouer le garde (la lira-t-on de l'agent ? alors elle est dans
/// `hearth-proto`, tolérante).
const LOCAL_ENUMS: &[(&str, &str)] = &[
    (
        "audit.rs::AuditOutcomeDto",
        "commande de la WebView (filtre)",
    ),
    ("audit.rs::AuditKindDto", "commande de la WebView (filtre)"),
    (
        "audit.rs::AuditOriginKindDto",
        "miroir du DTO, jamais lu de l'agent",
    ),
    (
        "link_dto.rs::RoleDto",
        "commande de la WebView (rôle choisi)",
    ),
    (
        "reauth/dto.rs::ReauthModeDto",
        "commande de la WebView (réglage)",
    ),
    ("reauth/dto.rs::AdminActKindDto", "commande de la WebView"),
    (
        "reauth/dto.rs::TrustLevelDto",
        "commande de la WebView (état annoncé, renvoyé à la coquille) ; lu de l'agent par `hearth_proto::TrustLevel`",
    ),
    (
        "service_ops/dto.rs::OperationChoiceDto",
        "commande de la WebView (choix de la décision) ; lu de l'agent par `hearth_proto::OperationChoice`",
    ),
    (
        "update/dto.rs::UpdatePhase",
        "fichier local du client (update.json)",
    ),
    (
        "update/dto.rs::UpdateFailure",
        "fichier local du client (update.json)",
    ),
    (
        "domain/pending_ops.rs::Phase",
        "carnet des opérations du client (disque)",
    ),
];

#[test]
fn every_deserialized_enum_of_the_protocol_is_classified_and_every_entry_exists() {
    let scanned = scanned_enums();
    let mut listed: Vec<String> = enums().iter().map(|entry| entry.key.to_owned()).collect();
    listed.sort();
    let unclassified: Vec<&String> = scanned.iter().filter(|key| !listed.contains(key)).collect();
    assert!(
        unclassified.is_empty(),
        "énumération(s) lue(s) du fil, non classée(s) dans `enums()` (ajoute-la avec sa variante `Unknown`          `#[serde(other)]`, ou classe-la `Strict` si l'agent la lit comme une commande) : {unclassified:?}"
    );
    let stale: Vec<&String> = listed.iter().filter(|key| !scanned.contains(key)).collect();
    assert!(
        stale.is_empty(),
        "entrée(s) de `enums()` sans énumération correspondante (renommée ? sans `Deserialize` ?) : {stale:?}"
    );
}

#[test]
fn every_enum_deserialized_in_the_link_and_the_shell_is_declared_local() {
    let base = root();
    let mut scanned = Vec::new();
    for (directory, minimum) in [
        (base.join("../hearth-link/src"), 15),
        (base.join("../../apps/desktop/src-tauri/src"), 15),
    ] {
        scanned.extend(deserialized_types(&directory, minimum, true));
    }
    scanned.sort();
    let declared: Vec<String> = LOCAL_ENUMS
        .iter()
        .map(|(key, _)| (*key).to_owned())
        .collect();
    let undeclared: Vec<&String> = scanned
        .iter()
        .filter(|key| !declared.contains(key))
        .collect();
    assert!(
        undeclared.is_empty(),
        "énumération(s) qui dérive(nt) `Deserialize` dans la liaison ou la coquille, non déclarée(s) dans \
         `LOCAL_ENUMS` (lue(s) de l'agent ? alors elle va dans `hearth-proto` avec sa variante `Unknown`) : {undeclared:?}"
    );
    let stale: Vec<&String> = declared
        .iter()
        .filter(|key| !scanned.contains(key))
        .collect();
    assert!(
        stale.is_empty(),
        "entrée(s) de `LOCAL_ENUMS` périmée(s) : {stale:?}"
    );
}

#[test]
fn a_value_invented_by_the_future_is_read_as_unknown_or_refused_as_declared() {
    for entry in enums() {
        let outcome = (entry.read)(future_value(entry.tagged));
        match entry.kind {
            Kind::Tolerant => {
                let shown = outcome.unwrap_or_else(|error| {
                    panic!(
                        "{} REJETTE une valeur inconnue ({error}) : un agent plus récent casserait un \
                         client installé. Ajoute `#[serde(other)] Unknown` et traite la variante.",
                        entry.key
                    )
                });
                // Le système d'init n'a pas d'« inconnu » : son repli `Other` dit déjà « un autre que ceux-là ».
                let fallback = if entry.key.ends_with("::InitSystem") {
                    "Other"
                } else {
                    "Unknown"
                };
                assert_eq!(
                    shown, fallback,
                    "{} accepte une valeur inventée mais ne la lit pas comme `Unknown` (une valeur inconnue \
                     ne doit jamais être prise pour une valeur connue)",
                    entry.key
                );
            }
            Kind::Strict | Kind::Cascade => {
                assert!(
                    outcome.is_err(),
                    "{} est classé strict/cascade mais accepte une valeur inventée : reclasse-le",
                    entry.key
                );
            }
        }
    }
}

#[test]
fn a_known_type_with_an_unknown_value_inside_is_not_rejected_as_a_whole() {
    // Le cas du défaut : un agent plus récent envoie une étape et une issue que ce client ne connaît pas.
    let progress = json!({
        "type": "update",
        "version": "9.9.9",
        "step": FUTURE,
        "percent": null,
        "outcome": FUTURE,
        "reason": FUTURE,
        "champ_du_futur": { "a": 1 }
    });
    let ServerMessage::Update(progress_read) =
        serde_json::from_value::<ServerMessage>(progress.clone()).expect("le message est lu")
    else {
        panic!("variante inattendue");
    };
    assert_eq!(progress_read.step, UpdateStep::Unknown);
    assert_eq!(progress_read.outcome, Some(UpdateOutcome::Unknown));
    assert_eq!(progress_read.reason, Some(UpdateReason::Unknown));
    // Et la même forme lue comme une réponse HTTP (sans `type`).
    let mut http = progress;
    http.as_object_mut().expect("objet").remove("type");
    let read = serde_json::from_value::<UpdateProgress>(http).expect("lue");
    assert_eq!(read.step, UpdateStep::Unknown);

    // Un code d'erreur inconnu garde son message et ses détails.
    let error = serde_json::from_value::<ErrorBody>(json!({
        "error": { "code": FUTURE, "message": "m", "details": { "x": 1 }, "autre": true }
    }))
    .expect("erreur lue");
    assert_eq!(error.error.code, ErrorCode::Unknown);
    assert_eq!(error.error.message, "m");
}

#[test]
fn structures_read_from_the_other_side_accept_an_extra_field() {
    let account = json!({
        "id": "a", "username": "marie", "role": "admin", "champ_du_futur": [1, 2, 3]
    });
    let read = serde_json::from_value::<AccountInfo>(account).expect("lue");
    assert_eq!(read.role, RoleName::Admin);
    assert!(read.rights.is_empty(), "champ optionnel absent toléré");
    let read = serde_json::from_value::<AccountInfo>(json!({
        "id": "a", "username": "marie", "role": FUTURE, "rights": ["services", "futur"]
    }))
    .expect("lue");
    assert_eq!(read.role, RoleName::Unknown);
}

/// Aucune structure lue ne refuse un champ inconnu ni ne se lit par un `Deserialize` écrit à la main, et toute
/// lecture personnalisée (`try_from`, `deserialize_with`, `with`, `remote`, `from`) est déclarée.
#[test]
fn nothing_refuses_an_unknown_field_in_what_is_read_from_the_network() {
    // Les chaînes recherchées sont coupées pour que ce fichier ne se trouve pas lui-même.
    let deny = concat!("deny_unknown", "_fields");
    let mut offenders = Vec::new();
    for (directory, minimum) in scanned_directories() {
        for file in files_of(&directory, minimum) {
            let text = std::fs::read_to_string(&file).expect("lecture");
            let relative = relative_to(&file, &directory);
            if text.contains(deny) {
                offenders.push(format!("{relative} : `{deny}`"));
            }
            // Tout `impl ... Deserialize<'...> for`, quels que soient les génériques et le nom de la durée de vie.
            for (number, line) in production_lines(&text) {
                let trimmed = line.trim_start();
                let hand_written = trimmed.starts_with("impl")
                    && (trimmed.contains("Deserialize<'") || trimmed.contains("Deserialize <'"));
                if hand_written && !HAND_WRITTEN.contains(&relative.as_str()) {
                    offenders.push(format!("{relative}:{}: `{trimmed}`", number + 1));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "une structure lue du réseau refuserait un champ en trop ou se lirait à la main sans être déclarée \
         (`HAND_WRITTEN`, `CUSTOM_READS`) : {offenders:?}"
    );
}

/// Les seuls `Deserialize` écrits à la main du protocole (tolérants par construction, testés dans le module).
const HAND_WRITTEN: &[&str] = &["lenient.rs"];

/// Les lectures personnalisées permises : chaque argument serde `deserialize_with`, `try_from`, `with`, `remote` ou
/// `from` du code balayé doit y figurer, retrouvé par le MODULE (chemin du fichier), le NOM DU TYPE qui le porte et
/// l'argument lui-même (comparé sans espace, donc insensible à un repli de `rustfmt` ; l'attribut peut s'étendre sur
/// plusieurs lignes). Chacune rend une valeur illisible « inconnue » (jamais une valeur plausible) ou ne lit qu'un
/// fichier local.
const CUSTOM_READS: &[(&str, &str, &str)] = &[
    (
        "api/devices.rs",
        "RemoveDeviceRequest",
        "deserialize_with = \"lenient_proof\"",
    ),
    (
        "api/reauth.rs",
        "Reauth",
        "deserialize_with = \"lenient_proof\"",
    ),
    (
        "api/sessions.rs",
        "DeviceLoginRequest",
        "deserialize_with = \"lenient_proof\"",
    ),
    (
        "stream.rs",
        "SignedAuth",
        "deserialize_with = \"crate::api::sessions::lenient_proof\"",
    ),
    (
        "api/reauth.rs",
        "SetReauthRequest",
        "deserialize_with = \"lenient_reauth\"",
    ),
    (
        "api/metrics.rs",
        "TempSample",
        "deserialize_with = \"crate::lenient::measure\"",
    ),
    (
        "api/metrics.rs",
        "Sample",
        "deserialize_with = \"crate::lenient::measure\"",
    ),
    (
        "api/metrics.rs",
        "Sample",
        "deserialize_with = \"crate::lenient::measures\"",
    ),
    (
        "api/metrics.rs",
        "StepPeak",
        "deserialize_with = \"crate::lenient::measure\"",
    ),
    (
        "api/metrics.rs",
        "TempPeak",
        "deserialize_with = \"crate::lenient::measure\"",
    ),
    (
        "api/update.rs",
        "UpdateProgress",
        "deserialize_with = \"crate::lenient::percent\"",
    ),
    (
        "api/service_ops.rs",
        "OperationView",
        "deserialize_with = \"crate::lenient::percent\"",
    ),
    // Fichiers locaux du client (carnet des serveurs, opérations suivies), jamais lus de l'agent.
    (
        "domain/pending_ops.rs",
        "OperationId",
        "try_from = \"String\"",
    ),
    ("domain/server.rs", "ServerId", "try_from = \"String\""),
    (
        "domain/server.rs",
        "ServerRecord",
        "with = \"fingerprint_hex\"",
    ),
];

#[test]
fn every_custom_read_is_declared() {
    let mut undeclared = Vec::new();
    let mut seen = Vec::new();
    for (directory, minimum) in scanned_directories() {
        for file in files_of(&directory, minimum) {
            let text = std::fs::read_to_string(&file).expect("lecture");
            let relative = relative_to(&file, &directory);
            for (number, ty, arg) in serde_custom_reads(&text) {
                let declared = CUSTOM_READS.iter().find(|(path, name, wanted)| {
                    *path == relative && *name == ty && canonical(wanted) == arg
                });
                match declared {
                    Some(entry) => seen.push(*entry),
                    None => undeclared.push(format!("{relative}:{number}: `{ty}` : `{arg}`")),
                }
            }
        }
    }
    assert!(
        undeclared.is_empty(),
        "lecture personnalisée non déclarée dans `CUSTOM_READS` (rend-elle une valeur illisible « inconnue » ? sinon \
         elle rejette tout un message) : {undeclared:?}"
    );
    let stale: Vec<_> = CUSTOM_READS
        .iter()
        .filter(|entry| !seen.contains(entry))
        .collect();
    assert!(
        stale.is_empty(),
        "entrée(s) de `CUSTOM_READS` périmée(s) : {stale:?}"
    );
}

/// Le DEVENIR d'`Unknown` : une valeur inconnue n'est jamais repliée sur une valeur connue sans que ce choix soit
/// écrit ici. Deux formes sont cherchées dans tout le code Rust (agent, liaison, coquille, protocole) :
/// - un motif `X::Unknown | X::Connu` (ou l'inverse) qui lit les deux pareil ;
/// - un bras `X::Unknown => X::Connu` qui la convertit.
///
/// Chaque occurrence acceptée est une DÉCISION, avec sa raison.
const UNKNOWN_FOLDS: &[(&str, &str, &str, &str)] = &[
    (
        "hearth-proto/src/error.rs",
        "ErrorCode",
        "Self::InternalError | Self::Unknown => 500",
        "un code inconnu n'est jamais émis ; son statut, si on le demande, est une erreur serveur",
    ),
    (
        "hearth-proto/src/error.rs",
        "ErrorCode",
        "Self::Unknown => Self::from_status(status)",
        "un code inconnu retombe sur le sens du statut HTTP (point 8), jamais sur un succès",
    ),
    (
        "hearth-agent/src/entrypoint/http/auth.rs",
        "ErrorCode",
        "ErrorCode::InternalError | ErrorCode::Unknown => Some(Outcome::Failed(Reason::Internal))",
        "l'agent n'émet jamais `Unknown` ; s'il arrivait, c'est une erreur interne, jamais un succès",
    ),
    (
        "hearth-agent/src/domain/services/logs.rs",
        "LogSource",
        "LogSource::Out | LogSource::Unknown => &mut self.out",
        "décision de HRT-66 : une source inconnue se traite comme la sortie normale (BR-COMPAT-001, cas limites)",
    ),
    (
        "hearth-link/src/adapters/http_transport.rs",
        "ServerMessage",
        "Ok(ServerMessage::Unknown) | Err(_) => Frame::Other",
        "un type de message inconnu est ignoré, comme une trame illisible : le flux continue (point 2)",
    ),
    (
        "hearth-agent/src/application/update.rs",
        "UpdateOutcome",
        "Some(UpdateOutcome::Unknown) | None => {",
        "une issue que l'agent ne comprend pas a sa propre entrée de journal, ni réussite ni échec (point 7)",
    ),
    (
        "hearth-link/src/manager/attempt.rs",
        "UpgradeTarget",
        "Some(UpgradeTarget::Client | UpgradeTarget::Unknown) | None => UpgradeTarget::Client",
        "une cible inconnue se lit « le client » : le seul côté que l'utilisateur met à jour d'ici (BR-COMPAT-001)",
    ),
    (
        "apps/desktop/src-tauri/src/link_dto.rs",
        "UpgradeTarget",
        "Blocked::IncompatibleVersion(UpgradeTarget::Client | UpgradeTarget::Unknown)",
        "la liaison ne tient jamais `Unknown` (lu « le client » à la réception) ; bras explicite, pas un `_`",
    ),
    (
        "apps/desktop/src-tauri/src/reauth/service.rs",
        "RoleDto",
        "Some(RoleDto::Admin | RoleDto::Unknown) | None => RoleName::Admin",
        "un rôle inconnu se traite comme administrateur : le plus strict (le mot de passe est demandé)",
    ),
    (
        "hearth-proto/src/admin_act.rs",
        "TrustLevel",
        "TrustLevel::Attack | TrustLevel::Unknown => PasswordNeed::Yes",
        "un état de confiance inconnu se traite comme le mode attaque : le plus strict (HRT-194, BR-TRUST-052)",
    ),
];

#[test]
fn an_unknown_value_is_never_folded_into_a_known_one_unless_the_decision_is_written() {
    let base = root();
    let roots = [
        (base.join("src"), "hearth-proto/src"),
        (base.join("../hearth-link/src"), "hearth-link/src"),
        (base.join("../hearth-agent/src"), "hearth-agent/src"),
        (
            base.join("../../apps/desktop/src-tauri/src"),
            "apps/desktop/src-tauri/src",
        ),
    ];
    // Les énumérations tolérantes : leur nom, et celui de leur miroir `Dto`.
    let tolerant: Vec<String> = enums()
        .iter()
        .filter(|entry| entry.kind == Kind::Tolerant)
        .map(|entry| entry.key.rsplit("::").next().expect("nom").to_owned())
        .collect();
    let is_tolerant_path = |text: &str| {
        text.contains("Self::")
            || text.contains("Dto::")
            || tolerant
                .iter()
                .any(|name| text.contains(&format!("{name}::")))
    };
    let mut found = Vec::new();
    let mut accepted = Vec::new();
    for (directory, label) in roots {
        for file in files_of(&directory, 15) {
            let text = std::fs::read_to_string(&file).expect("lecture");
            let relative = format!("{label}/{}", relative_to(&file, &directory));
            let lines = logical_lines(&text);
            for (at, (number, line)) in lines.iter().enumerate() {
                if !line.contains("::Unknown") {
                    continue;
                }
                let pattern = line.split("=>").next().unwrap_or(line);
                // Une fermeture (`(|x| ...)`) n'est pas une alternative de motif.
                let mixes =
                    is_tolerant_path(pattern) && pattern.contains(" | ") && !pattern.contains("(|");
                let converts = line.contains("::Unknown =>")
                    && line.split("=>").nth(1).is_some_and(|right| {
                        let right = right.trim().trim_end_matches(',');
                        is_tolerant_path(right) && !right.contains("::Unknown")
                    });
                if mixes || converts {
                    let owner = unknown_owner(&lines, at);
                    let seen = canonical(&line.replace("Self::", &format!("{owner}::")));
                    match UNKNOWN_FOLDS.iter().find(|(path, ty, wanted, _)| {
                        *path == relative
                            && *ty == owner
                            && seen.contains(&canonical(
                                &wanted.replace("Self::", &format!("{owner}::")),
                            ))
                    }) {
                        Some(entry) => accepted.push(*entry),
                        None => {
                            found.push(format!("{relative}:{}: `{owner}` : `{line}`", number + 1))
                        }
                    }
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "une valeur inconnue est repliée sur une valeur connue sans décision écrite dans `UNKNOWN_FOLDS` (une valeur \
         inconnue n'est JAMAIS prise pour une valeur connue, BR-COMPAT-001 point 4) : {found:?}"
    );
    let stale: Vec<_> = UNKNOWN_FOLDS
        .iter()
        .filter(|entry| !accepted.contains(entry))
        .collect();
    assert!(
        stale.is_empty(),
        "décision(s) de `UNKNOWN_FOLDS` périmée(s) : {stale:?}"
    );
}

/// Le compte des éléments illisibles ne va qu'à l'INTERFACE (liaison, événement, DTO de la coquille) : il n'est jamais
/// écrit sur le fil. Une liste relue puis réécrite redonne le tableau ordinaire, sans champ en plus ; un ancien client
/// lit donc exactement ce qu'il lisait, et `API_VERSION` ne bouge pas (FIX:01M4VR6Y4Q4AVQVT9FCME7Z2SB).
#[test]
fn the_unreadable_count_is_never_written_and_the_api_version_does_not_change() {
    use hearth_proto::api::devices::DevicesResponse;
    use hearth_proto::version::{API_MIN_SUPPORTED, API_VERSION};

    let devices = serde_json::from_value::<DevicesResponse>(json!({
        "devices": [{ "x": 1 }], "max": 8
    }))
    .expect("lue");
    assert_eq!(devices.devices.unreadable(), 1);
    let written = serde_json::to_value(&devices).expect("écrite");
    assert_eq!(written, json!({ "devices": [], "max": 8 }));
    assert!(
        written["devices"].is_array() && written.get("unreadable").is_none(),
        "le compte ne va pas sur le fil"
    );
    assert_eq!((API_VERSION, API_MIN_SUPPORTED), (1, 1));
}

/// Listes et nombres (BR-COMPAT-001 point 10) : un élément illisible d'une liste est écarté et compté, un nombre
/// hors bornes ou nul devient inconnu, jamais borné.
#[test]
fn an_unreadable_element_is_dropped_and_counted_and_a_number_out_of_bounds_is_unknown() {
    use hearth_proto::api::accounts::AccountsResponse;
    use hearth_proto::api::audit::AuditResponse;
    use hearth_proto::api::devices::DevicesResponse;
    use hearth_proto::api::metrics::Sample;

    let accounts = serde_json::from_value::<AccountsResponse>(json!({
        "accounts": [
            { "id": "a", "username": "marie", "role": "admin", "created_at": "2026-10-01T00:00:00Z",
              "last_login_at": null, "sessions_open": 0 },
            { "id": "b", "forme": "nouvelle" },
            "pas un objet"
        ]
    }))
    .expect("la liste est lue malgré des éléments illisibles");
    assert_eq!(accounts.accounts.len(), 1);
    assert_eq!(accounts.accounts.unreadable(), 2);

    let devices = serde_json::from_value::<DevicesResponse>(json!({
        "devices": [{ "x": 1 }, { "y": 2 }, { "z": 3 }], "max": 8
    }))
    .expect("lue");
    assert_eq!(
        (devices.devices.len(), devices.devices.unreadable()),
        (0, 3)
    );

    let audit = serde_json::from_value::<AuditResponse>(
        json!({ "events": [null, 4], "next_before": null }),
    )
    .expect("lue");
    assert_eq!(audit.events.unreadable(), 2);

    // La forme du CHAMP change (un objet à la place du tableau) : c'est un autre protocole, rejeté.
    assert!(
        serde_json::from_value::<AuditResponse>(
            json!({ "events": { "a": 1 }, "next_before": null })
        )
        .is_err()
    );

    // Un nombre hors bornes devient inconnu, jamais borné à une valeur plausible.
    let progress = |percent: Value| {
        serde_json::from_value::<UpdateProgress>(json!({
            "version": "1", "step": "download", "percent": percent, "outcome": null, "reason": null
        }))
        .expect("lue")
    };
    assert_eq!(progress(json!(42)).percent, Some(42));
    for out_of_bounds in [json!(300), json!(101), json!(-1), json!(1.5), json!("40")] {
        assert_eq!(
            progress(out_of_bounds).percent,
            None,
            "jamais borné à une valeur plausible"
        );
    }
    assert_eq!(progress(json!(null)).percent, None);

    // Une mesure nulle (c'est ainsi que `serde_json` écrit NaN) est une mesure inconnue, jamais zéro.
    let sample = serde_json::from_value::<Sample>(json!({
        "at": "2026-10-14T00:00:00Z", "uptime_s": 1, "cpu": null, "cores": [1.0, null],
        "mem": { "used_bytes": 1, "total_bytes": 2, "swap_used_bytes": 0, "swap_total_bytes": 0 },
        "disks": [], "net": null, "gpus": [], "temps": []
    }))
    .expect("lue");
    assert!(sample.cpu.is_nan());
    assert!(sample.cores[1].is_nan());
}
