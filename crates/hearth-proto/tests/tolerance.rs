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

use hearth_proto::api::accounts::{AccountInfo, RoleName};
use hearth_proto::api::audit::{OriginKindName, OutcomeName};
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

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
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

/// Les `pub enum` qui dérivent `Deserialize` dans `src/`, en `fichier::Type`.
fn scanned_enums() -> Vec<String> {
    let src = root().join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    let mut found = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).expect("lecture");
        let relative = file
            .strip_prefix(&src)
            .expect("préfixe")
            .to_string_lossy()
            .replace('\\', "/");
        let lines: Vec<&str> = text.lines().collect();
        for (index, line) in lines.iter().enumerate() {
            let Some(rest) = line.strip_prefix("pub enum ") else {
                continue;
            };
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let mut derives_deserialize = false;
            let mut back = index;
            while back > 0 {
                back -= 1;
                let above = lines[back].trim_start();
                if above.starts_with("#[") || above.starts_with("//") {
                    derives_deserialize |=
                        above.starts_with("#[derive") && above.contains("Deserialize");
                } else {
                    break;
                }
            }
            if derives_deserialize {
                found.push(format!("{relative}::{name}"));
            }
        }
    }
    found.sort();
    found
}

#[test]
fn every_deserialized_enum_of_the_protocol_is_classified_and_every_entry_exists() {
    let scanned = scanned_enums();
    let mut listed: Vec<String> = enums().iter().map(|entry| entry.key.to_owned()).collect();
    listed.sort();
    let unclassified: Vec<&String> = scanned.iter().filter(|key| !listed.contains(key)).collect();
    assert!(
        unclassified.is_empty(),
        "énumération(s) lue(s) du fil, non classée(s) dans `enums()` (ajoute-la avec sa variante `Unknown` \
         `#[serde(other)]`, ou classe-la `Strict` si l'agent la lit comme une commande) : {unclassified:?}"
    );
    let stale: Vec<&String> = listed.iter().filter(|key| !scanned.contains(key)).collect();
    assert!(
        stale.is_empty(),
        "entrée(s) de `enums()` sans énumération correspondante (renommée ? sans `Deserialize` ?) : {stale:?}"
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

/// Aucune structure lue ne refuse un champ inconnu ni ne se lit par un `Deserialize` écrit à la main :
/// vrai tant que ces constructions n'existent pas dans les trois endroits qui décodent du réseau.
#[test]
fn nothing_refuses_an_unknown_field_in_what_is_read_from_the_network() {
    let base = root();
    let directories = [
        base.join("src"),
        base.join("../hearth-link/src"),
        base.join("../../apps/desktop/src-tauri/src"),
    ];
    // Les chaînes recherchées sont coupées pour que ce fichier ne se trouve pas lui-même.
    let forbidden = [
        concat!("deny_unknown", "_fields"),
        concat!("impl<'de> ", "Deserialize<'de> for"),
        concat!("impl<'de> ", "serde::Deserialize<'de> for"),
    ];
    let mut offenders = Vec::new();
    for directory in directories {
        let mut files = Vec::new();
        rust_files(&directory, &mut files);
        for file in files {
            let text = std::fs::read_to_string(&file).expect("lecture");
            for pattern in forbidden {
                if text.contains(pattern) {
                    offenders.push(format!("{} : `{pattern}`", file.display()));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "une structure lue du réseau refuserait un champ en trop ou se lirait à la main : {offenders:?}"
    );
}
