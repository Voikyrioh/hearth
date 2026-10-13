//! BR-GRANT-005 : aucune route (ni aucun code de l'agent en fonctionnement) n'écrit la colonne du droit
//! « gérer les services ». Ce test lit le texte BRUT des sources (`src/**/*.rs`), des migrations
//! (`migrations/*.sql`) et des requêtes vérifiées (`.sqlx/*.json`), commentaires retirés, sans jamais
//! interpréter les guillemets : (1) seuls les fichiers d'une liste fermée peuvent nommer la colonne ;
//! (2) aucune instruction `UPDATE … SET`, `INSERT … INTO`, `REPLACE INTO`, `ALTER TABLE`, `DO UPDATE` ne
//! la nomme hors de l'écrivain ; (3) une migration ne la nomme que pour la créer avec le défaut « aucun »
//! (la migration `0010`) ; (4) la seule requête vérifiée non `SELECT` qui la nomme est celle de l'écrivain.
//! Il double le test de balayage des routes (`http_api.rs`), qui, lui, détecte l'écriture à l'exécution
//! par des déclencheurs SQLite. Limite dite : un SQL dont le nom de la colonne est lui-même recomposé
//! (`format!("services_{}", …)`) échappe à la lecture ; le balayage par déclencheurs le rattrape.
//!
//! Le droit ne change que sur le serveur, par la commande locale `account services` et par la question de
//! l'installation (HRT-77). Elles passent par UNE requête, dans UN fichier, qui est la SEULE entrée de
//! `ALLOWED_WRITERS` : cette liste est le point de revue. Un dernier test borne qui PEUT l'appeler : le cas
//! d'usage `ServicesRightService` et son accès à l'écriture ne sont nommés que par les fichiers de la
//! ligne de commande et de l'installation, jamais par le serveur HTTP.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

/// Les fichiers autorisés à écrire la colonne : l'UNIQUE requête de la commande du serveur (HRT-77).
const ALLOWED_WRITERS: &[&str] = &["infrastructure/sqlite/services_right_repo.rs"];

/// Les fichiers de `src` qui peuvent NOMMER la colonne (mot entier `services_right`, hors commentaires) :
/// la lecture (requêtes de compte), le champ du compte dans le domaine et les vues, et l'écrivain. Un
/// fichier qui la nomme sans figurer ici fait échouer le test : ajouter une ligne est un point de revue.
const ALLOWED_NAMERS: &[&str] = &[
    "app.rs",
    "application/mod.rs",
    "entrypoint/account.rs",
    "application/accounts.rs",
    "application/ports/unit_of_work.rs",
    "application/services_right.rs",
    "domain/accounts/account.rs",
    // Lecture seule (HRT-66) : le `Viewer` des journaux porte le droit relu avec la session.
    "domain/services/logs.rs",
    "entrypoint/ws/service_logs.rs",
    "entrypoint/http/accounts.rs",
    "entrypoint/http/auth.rs",
    "entrypoint/http/wire.rs",
    "infrastructure/sqlite/account_repo.rs",
    "infrastructure/sqlite/services_right_repo.rs",
    "infrastructure/sqlite/store.rs",
];

/// La seule migration qui nomme la colonne, et la seule instruction qu'elle peut y consacrer : la
/// création, avec le défaut « aucun » (BR-GRANT-002).
const COLUMN_MIGRATION: &str = "0010_services_right.sql";
const COLUMN_CREATION: &str = "alter table accounts add column services_right text not null default 'none' check (services_right in ('none', 'manage'))";

/// La seule requête non `SELECT` qui nomme la colonne dans `.sqlx/` (requêtes vérifiées à la compilation).
const ALLOWED_QUERY: &str = "UPDATE accounts SET services_right = ? WHERE id = ?";

/// Qui peut nommer l'écriture et le cas d'usage qui la porte (sources, hors commentaires) :
/// (identifiant, fichiers autorisés). Le serveur HTTP (`entrypoint/http/`) n'est dans aucune liste.
const ALLOWED_CALLERS: &[(&str, &[&str])] = &[
    (
        "set_services_right",
        &[
            "application/ports/services_right_repo.rs",
            "application/services_right.rs",
            "infrastructure/sqlite/services_right_repo.rs",
        ],
    ),
    (".services_right()", &["application/services_right.rs"]),
    (
        "ServicesRightService",
        &[
            "app.rs",
            "app/install.rs",
            "application/services_right.rs",
            "entrypoint/account.rs",
        ],
    ),
    ("services_right_service", &["app.rs", "app/install.rs"]),
];

fn files_with_extension(dir: &Path, extension: &str, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_with_extension(&path, extension, out);
        } else if path.extension().is_some_and(|ext| ext == extension) {
            out.push(path);
        }
    }
}

fn relative_to(root: &Path, file: &Path) -> String {
    file.strip_prefix(root)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/")
}

/// Le code d'un source sans ses commentaires : lignes `//` (documentation comprise) et blocs `/* */`
/// pour Rust, `--` jusqu'à la fin de ligne pour SQL. Rien d'autre n'est interprété : surtout pas les
/// guillemets, qu'un `\"` ou un `'"'` rend illisibles.
fn code_of(source: &str, sql: bool) -> String {
    let mut text = source.to_owned();
    if !sql {
        while let Some(start) = text.find("/*") {
            let end = text[start..]
                .find("*/")
                .map_or(text.len(), |offset| start + offset + 2);
            text.replace_range(start..end, " ");
        }
    }
    text.lines()
        .map(|line| {
            if sql {
                line.split("--").next().unwrap_or("")
            } else if line.trim_start().starts_with("//") {
                ""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Les mots (lettres, chiffres, `_`), en minuscules : la ponctuation, les guillemets et les sauts de
/// ligne ne comptent pas, donc un SQL découpé en plusieurs littéraux reste lisible.
fn words(code: &str) -> Vec<String> {
    code.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

/// Le mot entier `services_right` (pas `set_services_right`, pas `services_right_repo`).
fn names_the_column(code: &str) -> bool {
    words(code).iter().any(|word| word == "services_right")
}

/// Le début d'une instruction qui écrit dans une table : `UPDATE t SET`, `INSERT [OR x] INTO`,
/// `REPLACE INTO`, `ALTER TABLE`, `DO UPDATE` (clause d'un `ON CONFLICT`).
fn starts_a_write(words: &[String], at: usize) -> bool {
    let word = |offset: usize| words.get(at + offset).map(String::as_str);
    match word(0) {
        Some("update") => word(2) == Some("set") || word(1) == Some("set"),
        Some("insert") => {
            word(1) == Some("into") || (word(1) == Some("or") && word(3) == Some("into"))
        }
        Some("replace") => word(1) == Some("into"),
        Some("alter") => word(1) == Some("table"),
        Some("do") => word(1) == Some("update"),
        _ => false,
    }
}

/// Une écriture dont les mots suivants (une instruction tient largement dans cette fenêtre) nomment la
/// colonne. Rend l'extrait fautif.
fn writes_the_column(code: &str) -> Option<String> {
    const WINDOW: usize = 60;
    let words = words(code);
    (0..words.len())
        .filter(|&at| starts_a_write(&words, at))
        .find(|&at| {
            words[at..words.len().min(at + WINDOW)]
                .iter()
                .any(|word| word == "services_right")
        })
        .map(|at| words[at..words.len().min(at + 12)].join(" "))
}

#[test]
fn the_detector_recognises_writes_whatever_the_quoting_and_the_case() {
    // Le détecteur lui-même : sans ces témoins, un détecteur cassé laisserait tout passer.
    for write in [
        r#"sqlx::query("UPDATE accounts SET services_right = 'manage' WHERE id = ?")"#,
        r#"sqlx::query("INSERT INTO accounts (id, services_right) VALUES (?, ?)")"#,
        r#"  insert or replace into accounts (services_right) values (?)"#,
        // Guillemet échappé et caractère guillemet AVANT le SQL : la lecture par parité des guillemets
        // inversait tout ; la lecture par mots n'y est pas sensible.
        r#"let quote = '"'; let a = "\""; let sql = "update accounts set services_right = ?";"#,
        // SQL construit en plusieurs littéraux.
        r#"concat!("UPDATE accounts ", "SET ", "services_right = ?")"#,
        // Instruction sur plusieurs lignes, `ON CONFLICT`.
        "INSERT INTO accounts (id)\n VALUES (?)\n ON CONFLICT (id) DO UPDATE SET services_right = 'manage'",
        "REPLACE INTO accounts (id, services_right) VALUES (?, ?)",
        "ALTER TABLE accounts RENAME COLUMN services_right TO x",
    ] {
        assert!(writes_the_column(write).is_some(), "non vu : {write}");
    }
    for read in [
        "SELECT id, services_right FROM accounts WHERE id = ?",
        "let right = account.services_right; refresh(right);",
        "INSERT INTO accounts (id, username) VALUES (?, ?)",
        "UPDATE accounts SET role = ? WHERE id = ?",
    ] {
        assert!(writes_the_column(read).is_none(), "faux positif : {read}");
    }
    // Le mot entier seulement.
    assert!(names_the_column("a.services_right = b"));
    assert!(!names_the_column(
        "set_services_right(x); services_right_repo"
    ));
    // Commentaires : ignorés, guillemets compris.
    assert_eq!(
        code_of("// un \"commentaire\" services_right\nlet a = 1;", false).trim(),
        "let a = 1;"
    );
    assert!(!names_the_column(&code_of(
        "/* services_right */ let a = 1;",
        false
    )));
    assert!(!names_the_column(&code_of(
        "SELECT 1; -- services_right",
        true
    )));
}

#[test]
fn no_code_of_the_agent_writes_the_services_right_and_few_files_name_it() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    files_with_extension(&src, "rs", &mut files);
    assert!(files.len() > 50, "les sources sont bien lues");
    let mut namers = Vec::new();
    for file in files {
        let relative = relative_to(&src, &file);
        let code = code_of(&std::fs::read_to_string(&file).unwrap(), false);
        if names_the_column(&code) {
            assert!(
                ALLOWED_NAMERS.contains(&relative.as_str()),
                "{relative} nomme `services_right` (BR-GRANT-005) : seuls les fichiers de ALLOWED_NAMERS le peuvent ; ajouter un fichier est un point de revue"
            );
            namers.push(relative.clone());
        }
        if let Some(extract) = writes_the_column(&code)
            && !ALLOWED_WRITERS.contains(&relative.as_str())
        {
            panic!(
                "{relative} écrit `services_right` (BR-GRANT-005) : le droit ne se donne que sur le serveur (HRT-77) : {extract}"
            );
        }
    }
    // Chaque fichier de la liste existe encore et nomme bien la colonne : une liste périmée ne protège rien.
    for allowed in ALLOWED_NAMERS {
        assert!(
            namers.iter().any(|namer| namer == allowed),
            "{allowed} ne nomme plus la colonne : le retirer de ALLOWED_NAMERS"
        );
    }
}

#[test]
fn no_migration_but_the_creation_names_the_services_right() {
    let migrations = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    let mut files = Vec::new();
    files_with_extension(&migrations, "sql", &mut files);
    assert!(files.len() >= 10, "les migrations sont bien lues");
    let creation = words(COLUMN_CREATION).join(" ");
    let mut creations = 0;
    for file in files {
        let relative = relative_to(&migrations, &file);
        let code = code_of(&std::fs::read_to_string(&file).unwrap(), true);
        // En SQL, `;` termine une instruction : le découpage ne dépend d'aucun guillemet.
        for statement in code.split(';').filter(|s| names_the_column(s)) {
            assert!(
                relative == COLUMN_MIGRATION && words(statement).join(" ") == creation,
                "{relative} nomme `services_right` hors de la création avec le défaut « aucun » (une migration qui l'écrit accorderait le droit à la mise à jour de l'agent, BR-GRANT-005) : {}",
                statement.trim()
            );
            creations += 1;
        }
    }
    assert_eq!(creations, 1, "la création de la colonne, une seule fois");
}

#[test]
fn the_checked_queries_only_read_the_services_right_but_for_the_one_allowed() {
    let sqlx = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.sqlx");
    let mut files = Vec::new();
    files_with_extension(&sqlx, "json", &mut files);
    assert!(files.len() > 50, "les requêtes vérifiées sont bien lues");
    let (mut reads, mut writes) = (0, 0);
    for file in files {
        let entry: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        let query = entry["query"].as_str().unwrap();
        if !names_the_column(query) {
            continue;
        }
        if query
            .trim_start()
            .to_ascii_uppercase()
            .starts_with("SELECT")
        {
            reads += 1;
        } else {
            assert_eq!(query, ALLOWED_QUERY, "{}", file.display());
            writes += 1;
        }
    }
    assert!(reads >= 3, "les requêtes de compte lisent la colonne");
    assert_eq!(writes, 1, "une seule requête vérifiée l'écrit");
}

#[test]
fn only_the_command_line_and_the_installation_can_reach_the_write() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    files_with_extension(&src, "rs", &mut files);
    let mut seen = vec![0_usize; ALLOWED_CALLERS.len()];
    for file in files {
        let relative = relative_to(&src, &file);
        let code = code_of(&std::fs::read_to_string(&file).unwrap(), false);
        for (index, (identifier, allowed)) in ALLOWED_CALLERS.iter().enumerate() {
            if code.contains(identifier) {
                seen[index] += 1;
                assert!(
                    allowed.contains(&relative.as_str()),
                    "{relative} nomme `{identifier}` : seuls la commande du serveur et l'installation peuvent donner le droit (BR-GRANT-005) ; ajouter un fichier à cette liste est un point de revue"
                );
            }
        }
    }
    assert!(
        seen.iter().all(|count| *count > 0),
        "chaque identifiant surveillé existe encore : {seen:?}"
    );
    // Le serveur HTTP ne reçoit jamais ce cas d'usage : il n'est pas dans `Services`.
    let state = std::fs::read_to_string(src.join("app.rs")).unwrap();
    let assembled = state
        .split("pub struct Services")
        .nth(1)
        .and_then(|rest| rest.split('}').next())
        .unwrap();
    assert!(
        !assembled.contains("ServicesRight") && !assembled.contains("rights"),
        "`Services`, remis au serveur HTTP, ne porte pas le cas d'usage du droit"
    );
}
