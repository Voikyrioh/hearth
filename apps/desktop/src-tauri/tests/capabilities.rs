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
        "serverId: string, username: string, password: string, role: RoleDto, adminPassword: string | null",
    ),
    (
        "changeAccountRole",
        "serverId: string, accountId: string, role: RoleDto, adminPassword: string | null",
    ),
    (
        "setAccountPassword",
        "serverId: string, accountId: string, password: string, adminPassword: string",
    ),
    (
        "changeOwnPassword",
        "serverId: string, current: string, password: string, keepAddress: boolean",
    ),
    (
        "closeAccountSessions",
        "serverId: string, accountId: string, adminPassword: string | null",
    ),
    (
        "deleteAccount",
        "serverId: string, accountId: string, confirmation: string | null, adminPassword: string | null",
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
    // HRT-26 : sécurité. `password` est le mot de passe actuel (enveloppé dans un `Secret`, jamais
    // gardé) ; la preuve de la clé, le défi et la signature n'ont ni paramètre ni commande.
    ("getSecurity", "serverId: string"),
    ("listSecurityStates", ""),
    (
        "setAttackMode",
        "serverId: string, active: boolean, password: string",
    ),
    ("getNotifyOnSecurityAlert", ""),
    ("setNotifyOnSecurityAlert", "enabled: boolean"),
    ("getAgentUpdate", "serverId: string"),
    (
        "updateAgent",
        "serverId: string, version: string, adminPassword: string",
    ),
    ("ackAgentResult", "serverId: string, at: string"),
    // HRT-30 : la confirmation des actes. `adminPassword` / `password` sont les mots de passe de
    // confirmation (enveloppés dans un `Secret`, jamais gardés) ; la preuve de la clé, le défi et la
    // signature n'ont ni paramètre ni commande. `reauthCovers` ne parle pas à l'agent.
    ("getReauthState", "serverId: string"),
    (
        "reauthCovers",
        "kind: AdminActKindDto, role: \"admin\" | \"readonly\" | null",
    ),
    (
        "setReauthSetting",
        "serverId: string, mode: ReauthModeDto, password: string",
    ),
];

/// Les commandes qui portent UN ACTE D'ADMINISTRATION, avec le paramètre qui porte le mot de passe de
/// confirmation (HRT-30, BR-TRUST-036, 037). Liste fermée : une commande de plus qui modifie le serveur
/// sans figurer ici, ou une commande d'ici sans son mot de passe, fait échouer le test. Le retrait d'un
/// poste (`removeTrustedDevice`) garde son contrat livré (clé du poste courant) et son mot de passe
/// `password`.
const ADMIN_COMMANDS: &[(&str, &str)] = &[
    ("createAccount", "adminPassword"),
    ("changeAccountRole", "adminPassword"),
    ("setAccountPassword", "adminPassword"),
    ("deleteAccount", "adminPassword"),
    ("closeAccountSessions", "adminPassword"),
    ("updateAgent", "adminPassword"),
    ("setAttackMode", "password"),
    ("changeOwnPassword", "current"),
    ("setReauthSetting", "password"),
    ("removeTrustedDevice", "password"),
];

/// Les commandes qui ne modifient RIEN chez l'agent : lectures, réglages locaux, abonnements.
const NOT_AN_ADMIN_ACT: &[&str] = &[
    "getSettings",
    "setLaunchAtStartup",
    "getAppVersion",
    "openLogsFolder",
    "logFrontendError",
    "getNotifyOnLinkChange",
    "setNotifyOnLinkChange",
    "setDisplayedServer",
    "listServers",
    "listLinkStates",
    "probeServer",
    "addAndLogin",
    "login",
    "logout",
    "retryNow",
    "acceptFingerprint",
    "updateServer",
    "removeServer",
    "forgetCredentials",
    "listFingerprintAlerts",
    "listLinkNotices",
    "ackLinkNotices",
    "listUnreadOperations",
    "ackUnreadOperations",
    "readAudit",
    "exportAudit",
    "getUpdateState",
    "checkForUpdates",
    "postponeUpdate",
    "installUpdate",
    "checkAccountInput",
    "listAccounts",
    "listTrustedDevices",
    "getSecurity",
    "listSecurityStates",
    "getNotifyOnSecurityAlert",
    "setNotifyOnSecurityAlert",
    "getAgentUpdate",
    "ackAgentResult",
    "getReauthState",
    "reauthCovers",
];

/// Toute commande exposée est soit un acte d'administration de la liste fermée (avec son mot de passe de
/// confirmation, de type `string` : jamais une valeur par défaut qui ferait partir l'acte sans lui), soit
/// une commande qui ne modifie rien chez l'agent. Une commande NOUVELLE n'est ni l'un ni l'autre : le test
/// échoue, il faut la classer.
#[test]
fn every_command_is_a_listed_admin_act_with_its_password_or_a_named_non_act() {
    for (name, args) in EXPECTED {
        let admin = ADMIN_COMMANDS.iter().find(|(command, _)| command == name);
        let free = NOT_AN_ADMIN_ACT.contains(name);
        assert!(
            admin.is_some() != free,
            "la commande {name} doit être classée UNE fois : acte d'administration (ADMIN_COMMANDS) ou \
             commande qui ne modifie rien chez l'agent (NOT_AN_ADMIN_ACT)"
        );
        if let Some((_, password)) = admin {
            assert!(
                args.split(", ")
                    .any(|arg| arg.starts_with(&format!("{password}: string"))),
                "l'acte {name} doit porter son mot de passe de confirmation `{password}` : {args}"
            );
        }
    }
    for (name, _) in ADMIN_COMMANDS {
        assert!(
            EXPECTED.iter().any(|(command, _)| command == name),
            "{name} figure dans ADMIN_COMMANDS sans commande"
        );
    }
}

/// Aucun code de la coquille n'envoie un acte d'administration par `LinkManager::execute` (qui le refuse de
/// toute façon) ni par `execute_raw` (réservé aux tests de la liaison) : la seule porte est `execute_act`.
#[test]
fn the_shell_reaches_the_agent_for_an_act_only_through_the_confirmed_door() {
    fn walk(dir: &std::path::Path, hits: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, hits);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                for (number, line) in text.lines().enumerate() {
                    let code = line.split("//").next().unwrap_or("");
                    if code.contains(".execute(") || code.contains(".execute_raw(") {
                        hits.push(format!("{}:{}", path.display(), number + 1));
                    }
                }
            }
        }
    }
    let mut hits = Vec::new();
    walk(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut hits,
    );
    assert!(
        hits.is_empty(),
        "un acte d'administration part par `execute_act` seulement : {hits:?}"
    );
}

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
