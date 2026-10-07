//! Les permissions de la fenêtre principale (build de production) : la liste blanche des commandes
//! est exacte, et aucune commande générique qui laisserait la WebView appeler une route quelconque
//! de l'agent n'existe (ADR-0013, ADR-0016) : une action = une commande typée, méthode et chemin
//! construits côté Rust.
//!
//! Deux gardes. Le test par NOM de permission n'est qu'un fil de détente (une commande générique
//! rebaptisée y échapperait). La garde STRUCTURELLE lit les signatures des commandes générées dans
//! `bindings.ts` : aucune commande exposée à la fenêtre n'a de paramètre libre qui désigne une
//! route, une méthode, un corps ou une adresse à joindre (`path`, `method`, `body`, `url`…).
#![allow(clippy::unwrap_used, clippy::expect_used)]

fn permissions() -> Vec<String> {
    let json = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/capabilities/default.json"
    ))
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn no_generic_action_command_is_allowed_to_the_webview() {
    for permission in permissions() {
        for forbidden in ["run-action", "run_action", "execute", "request", "http"] {
            assert!(
                !permission.contains(forbidden),
                "permission générique interdite : {permission}"
            );
        }
    }
}

#[test]
fn every_permission_is_a_known_command_or_the_event_listener() {
    let manifest =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/build.rs")).unwrap();
    for permission in permissions() {
        if let Some(command) = permission.strip_prefix("allow-") {
            let snake = command.replace('-', "_");
            assert!(
                manifest.contains(&format!("\"{snake}\"")),
                "permission sans commande déclarée dans build.rs : {permission}"
            );
        } else {
            assert!(
                permission.starts_with("core:event:"),
                "permission inattendue : {permission}"
            );
        }
    }
}

#[test]
fn no_exposed_command_takes_a_free_route_method_or_body() {
    let bindings =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings.ts"))
            .unwrap();
    let mut seen = 0;
    for line in bindings.lines().filter(|l| l.contains("__TAURI_INVOKE")) {
        // `nomCommande: (a: T, b: U) => ...`
        let Some(args) = line
            .split_once(": (")
            .and_then(|(_, rest)| rest.split_once(") =>"))
            .map(|(args, _)| args)
        else {
            continue;
        };
        seen += 1;
        for arg in args.split(',') {
            let name = arg.split(':').next().unwrap_or("").trim();
            for forbidden in [
                "path", "method", "body", "url", "route", "request", "headers", "endpoint",
            ] {
                assert!(
                    !name.eq_ignore_ascii_case(forbidden),
                    "paramètre libre vers l'agent dans la commande : {line}"
                );
            }
        }
    }
    assert!(
        seen > 20,
        "les commandes de bindings.ts n'ont pas été lues ({seen})"
    );
}

/// La liste EXPECTED des commandes de la fenêtre et de leurs paramètres TYPÉS : toute commande ou tout
/// paramètre ajouté, retiré ou retypé fait échouer ce test, qui force une relecture explicite (la garde
/// par NOM de paramètre ne voit pas un `string` libre qui n'a pas un nom interdit). Règle de relecture
/// d'un paramètre `string` : c'est un identifiant que la coquille VALIDE côté Rust (serveur du carnet,
/// compte, version qu'elle compare à ce qu'elle tient) ; jamais une adresse, une signature, une somme, une
/// date ou une valeur que la coquille écrirait telle quelle sur le disque ou enverrait telle quelle.
const EXPECTED: &[(&str, &str)] = &[
    ("getSettings", ""),
    ("setLaunchAtStartup", "enabled: boolean"),
    ("getAppVersion", ""),
    ("openLogsFolder", ""),
    ("logFrontendError", "source: string, message: string"),
    ("getNotifyOnLinkChange", ""),
    ("setNotifyOnLinkChange", "enabled: boolean"),
    ("setDisplayedServer", "serverId: string | null"),
    ("listServers", ""),
    ("listLinkStates", ""),
    ("probeServer", "host: string, port: number | null"),
    ("addAndLogin", "input: AddServerInput"),
    (
        "login",
        "serverId: string, username: string, password: string, remember: boolean",
    ),
    ("logout", "serverId: string"),
    ("retryNow", "serverId: string"),
    ("acceptFingerprint", "serverId: string, fingerprint: string"),
    (
        "updateServer",
        "serverId: string, name: string, color: number, host: string, port: number | null, fingerprint: string | null",
    ),
    ("removeServer", "serverId: string"),
    ("forgetCredentials", "serverId: string"),
    ("listFingerprintAlerts", ""),
    ("listLinkNotices", ""),
    ("ackLinkNotices", "ids: number[]"),
    ("listUnreadOperations", ""),
    ("ackUnreadOperations", "opIds: string[]"),
    (
        "readAudit",
        "serverId: string, filter: AuditFilterDto, before: number | null",
    ),
    ("exportAudit", "serverId: string, filter: AuditFilterDto"),
    ("getUpdateState", ""),
    ("checkForUpdates", ""),
    ("postponeUpdate", ""),
    ("installUpdate", ""),
    ("checkAccountInput", "username: string, password: string"),
    ("listAccounts", "serverId: string"),
    (
        "createAccount",
        "serverId: string, username: string, password: string, role: RoleDto",
    ),
    (
        "changeAccountRole",
        "serverId: string, accountId: string, role: RoleDto",
    ),
    (
        "setAccountPassword",
        "serverId: string, accountId: string, password: string",
    ),
    (
        "changeOwnPassword",
        "serverId: string, current: string, password: string",
    ),
    (
        "closeAccountSessions",
        "serverId: string, accountId: string",
    ),
    (
        "deleteAccount",
        "serverId: string, accountId: string, confirmation: string | null",
    ),
    // HRT-23 : postes de confiance. `deviceId` est un identifiant rendu par la liste, que la coquille
    // valide (lettres et chiffres) avant de le placer dans un chemin ; `password` est le mot de passe
    // actuel, enveloppé dans un `Secret` et jamais gardé. Aucune commande ne crée, n'exporte ni ne
    // signe : la clé d'appareil n'a pas de commande.
    ("listTrustedDevices", "serverId: string"),
    (
        "removeTrustedDevice",
        "serverId: string, deviceId: string, password: string",
    ),
    ("getAgentUpdate", "serverId: string"),
    ("updateAgent", "serverId: string, version: string"),
    ("ackAgentResult", "serverId: string, at: string"),
];

#[test]
fn the_commands_and_their_typed_parameters_are_exactly_the_reviewed_list() {
    let bindings =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings.ts"))
            .unwrap();
    let mut found: Vec<(String, String)> = bindings
        .lines()
        .filter(|l| l.contains("__TAURI_INVOKE"))
        .filter_map(|line| {
            let line = line.trim();
            let (name, rest) = line.split_once(": (")?;
            let (args, _) = rest.split_once(") =>")?;
            Some((name.to_owned(), args.to_owned()))
        })
        .collect();
    let mut expected: Vec<(String, String)> = EXPECTED
        .iter()
        .map(|(n, a)| ((*n).to_owned(), (*a).to_owned()))
        .collect();
    found.sort();
    expected.sort();
    assert_eq!(
        found, expected,
        "une commande ou un paramètre a changé : relis-le (aucun texte libre écrit tel quel, aucune adresse, signature, somme) puis mets à jour EXPECTED"
    );
}
