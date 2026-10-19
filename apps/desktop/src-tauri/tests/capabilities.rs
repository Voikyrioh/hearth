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

/// L'autre sens : toute commande déclarée dans `build.rs` est permise à la fenêtre. Sans cela, une commande
/// ajoutée oubliée de `capabilities/default.json` marche en développement et est refusée en production.
#[test]
fn every_declared_command_is_allowed_to_the_window() {
    let manifest =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/build.rs")).unwrap();
    let start = manifest.find("const COMMANDS").unwrap();
    let block = &manifest[start..manifest[start..].find("];").unwrap() + start];
    let allowed = permissions();
    let mut seen = 0;
    for line in block.lines().skip(1) {
        let Some(command) = line
            .trim()
            .strip_prefix('"')
            .and_then(|l| l.split('"').next())
        else {
            continue;
        };
        seen += 1;
        let permission = format!("allow-{}", command.replace('_', "-"));
        assert!(
            allowed.contains(&permission),
            "commande déclarée sans permission dans capabilities/default.json : {command}"
        );
    }
    assert!(
        seen > 50,
        "les commandes de build.rs n'ont pas été lues ({seen})"
    );
}

/// Les noms (`snake_case`) des commandes déclarées dans `build.rs`.
fn declared_in_build() -> std::collections::BTreeSet<String> {
    let manifest =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/build.rs")).unwrap();
    let start = manifest.find("const COMMANDS").unwrap();
    let block = &manifest[start..manifest[start..].find("];").unwrap() + start];
    block
        .lines()
        .skip(1)
        .filter_map(|line| {
            line.trim()
                .strip_prefix('"')
                .and_then(|l| l.split('"').next())
        })
        .map(str::to_owned)
        .collect()
}

/// Les commandes enregistrées dans `lib.rs` (`collect_commands![ ... ]`), par leur dernier segment.
fn registered_in_lib() -> std::collections::BTreeSet<String> {
    let lib = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs")).unwrap();
    let start = lib.find("collect_commands![").unwrap();
    let block = &lib[start..lib[start..].find("])").unwrap() + start];
    block
        .lines()
        .skip(1)
        .filter_map(|line| {
            let path = line.trim().strip_suffix(',')?;
            Some(path.rsplit("::").next()?.to_owned())
        })
        .collect()
}

/// Les fonctions marquées `#[tauri::command]` dans le code de la coquille.
fn marked_as_command() -> std::collections::BTreeSet<String> {
    fn walk(dir: &std::path::Path, found: &mut std::collections::BTreeSet<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, found);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                let mut marked = false;
                for line in text.lines() {
                    let line = line.trim();
                    if line == "#[tauri::command]" {
                        marked = true;
                    } else if marked
                        && let Some(rest) = line
                            .strip_prefix("pub async fn ")
                            .or_else(|| line.strip_prefix("pub fn "))
                    {
                        if let Some(name) = rest.split('(').next() {
                            found.insert(name.trim().to_owned());
                        }
                        marked = false;
                    }
                }
            }
        }
    }
    let mut found = std::collections::BTreeSet::new();
    walk(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut found,
    );
    found
}

/// Les quatre listes sont ÉGALES : les fonctions marquées commande, celles enregistrées dans `lib.rs`, celles
/// déclarées dans `build.rs` et les permissions de `capabilities/default.json`. L'oubli d'une commande dans
/// l'une d'elles (elle marche en développement et est refusée en production, ou l'inverse) est impossible.
#[test]
fn the_four_lists_of_commands_are_the_same() {
    let marked = marked_as_command();
    let registered = registered_in_lib();
    let declared = declared_in_build();
    let allowed: std::collections::BTreeSet<String> = permissions()
        .iter()
        .filter_map(|p| p.strip_prefix("allow-"))
        .map(|c| c.replace('-', "_"))
        .collect();
    assert!(
        marked.len() > 50,
        "commandes marquées non lues ({})",
        marked.len()
    );
    assert_eq!(
        marked, registered,
        "fonctions `#[tauri::command]` et liste de `lib.rs` (`collect_commands!`) diffèrent"
    );
    assert_eq!(
        registered, declared,
        "`lib.rs` et `build.rs` (COMMANDS) diffèrent"
    );
    assert_eq!(
        declared, allowed,
        "`build.rs` et `capabilities/default.json` diffèrent"
    );
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
        "kind: AdminActKindDto, role: \"admin\" | \"readonly\" | \"unknown\" | null",
    ),
    (
        "setReauthSetting",
        "serverId: string, mode: ReauthModeDto, password: string",
    ),
    // HRT-62 : le moteur et les services, en lecture seule. `engineRecheck` relance la détection (`POST
    // /engine/recheck` ne modifie rien) ; `openEngineDocs` n'a aucun paramètre (adresse constante).
    ("listServicesStates", ""),
    ("engineRecheck", "serverId: string"),
    // HRT-78 : le droit « gérer les services » du compte connecté, en lecture seule (aucune commande ne le donne).
    ("readServiceRights", "serverId: string"),
    // HRT-63 : les valeurs des variables d'UN service (`GET /services/{key}/env`, consignée par l'agent, réservée au
    // droit) ; la clé du service est le seul paramètre, la route est construite par la liaison.
    ("revealServiceEnv", "serverId: string, key: string"),
    // HRT-67 : la lecture des journaux d'UN service. `key` est la clé du service ouvert (vérifiée par la liaison,
    // qui construit la route) ; `session` est l'identifiant rendu par `openServiceLogs` (une session inconnue ne fait
    // rien) ; `batches` est un nombre de lots acquittés. Aucune ligne ne revient par une commande : elles arrivent par
    // l'événement `link://service-logs`.
    // `page` est la génération rendue par `announceLogPage` (la coquille la compare à celle de la fenêtre) ;
    // `keepServiceLogs` est le battement de vie de cette page et rend les sessions encore ouvertes.
    ("announceLogPage", ""),
    (
        "openServiceLogs",
        "page: number, serverId: string, key: string",
    ),
    ("closeServiceLogs", "session: string"),
    ("ackServiceLogs", "session: string, batches: number"),
    ("keepServiceLogs", "page: number"),
    ("openEngineDocs", ""),
    // HRT-84 : le suivi des opérations de service. `listServiceOperations` rejoue l'état tenu côté Rust ;
    // `ackServiceOperations` dit quelles issues l'interface a annoncées ; `decideServiceOperation` est le SEUL
    // geste (un acte confirmé : la route est construite par la coquille, le choix est une liste fermée).
    ("listServiceOperations", ""),
    ("ackServiceOperations", "serverId: string, opIds: string[]"),
    (
        "decideServiceOperation",
        "serverId: string, opId: string, choice: OperationChoiceDto, adminPassword: string | null",
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
    ("decideServiceOperation", "adminPassword"),
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
    "listServicesStates",
    "engineRecheck",
    "readServiceRights",
    "revealServiceEnv",
    "announceLogPage",
    "openServiceLogs",
    "closeServiceLogs",
    "ackServiceLogs",
    "keepServiceLogs",
    "openEngineDocs",
    "listServiceOperations",
    "ackServiceOperations",
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
