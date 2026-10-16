//! `cargo xtask release-stage` (HRT-51, conception de la distribution 8.2 et 10.2) : assemble une
//! version en local, sous `target/release-stage/`, après avoir REVÉRIFIÉ tout ce que le client, l'agent
//! et l'API refuseraient. Rien n'est écrit tant que toute la vérification n'a pas réussi, rien n'est
//! écrit ailleurs que sous `target/release-stage/`, aucun réseau, aucun secret.
//!
//! Entrées : le dossier de l'artefact du flux `publish-client` (`Hearth_X.Y.Z_x64-setup.exe`, son
//! `.sig`, `latest.json`, `notes.txt`), le binaire de l'agent et sa signature minisign faite à la main
//! (commentaire signé `version:X.Y.Z`), `deploy/install.sh`.
//!
//! Sortie (clés d'objet du bucket, section 6 de la conception) :
//!
//! ```text
//! releases/manifest.json            le manifeste, écrit EN DERNIER par release-upload
//! releases/install.sh               le script (mutable)
//! releases/X.Y.Z/                   les 8 fichiers de la liste fermée de l'API, immuables
//! ```
//!
//! À côté de `releases/` (donc jamais déposé) : `COMMIT`, le commit de l'arbre propre dont la
//! version est assemblée (HRT-60). `release-upload` refuse si le commit en place a changé.
//!
//! La liste fermée, la forme du manifeste et le chemin du pointeur de l'agent sont ceux de
//! `hearth-releases-api` : `src/service/release-files.ts` (noms, clés d'objet) et
//! `src/service/manifest.ts` (lecture du manifeste, `latestInstallerPath`, `agentPointer`).

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::client_release::{option, required, root};
use crate::host::{Call, Host, System};
use crate::release_core::*;

pub const MANIFEST_NAME: &str = "manifest.json";
pub const SCRIPT_NAME: &str = "install.sh";
pub const SUMS_NAME: &str = "SHA256SUMS";
/// Clé du manifeste dans le bucket (`MANIFEST_KEY` de `release-files.ts`) : « pointeur de version courante ».
pub const MANIFEST_KEY: &str = "releases/manifest.json";
/// Clé du script dans le bucket (`INSTALL_SCRIPT_KEY` de `release-files.ts`).
pub const SCRIPT_KEY: &str = "releases/install.sh";
/// Notes de version : 8 000 caractères au plus (conception 5.4).
const NOTES_MAX_CHARS: usize = 8_000;
/// L'API refuse un manifeste de plus de 256 Kio (`MAX_MANIFEST_BYTES` de `manifest.ts`).
const MANIFEST_MAX_BYTES: usize = 256 * 1024;

/// Fichier à la racine du dossier d'assemblage (hors `releases/`, donc jamais déposé) qui garde le commit
/// de l'arbre propre assemblé (HRT-60).
pub const COMMIT_FILE: &str = "COMMIT";
/// Nombre de fichiers nommés dans un refus d'arbre modifié.
const DIRTY_LIST_MAX: usize = 10;

/// Les fichiers d'une version : clé d'objet (`releases/...`, barres obliques) vers contenu.
pub type Files = BTreeMap<String, Vec<u8>>;

pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// La liste FERMÉE des noms d'une version, dans l'ordre de dépôt (`releaseFileNames` de
/// `release-files.ts`).
pub fn release_file_names(version: &str) -> Vec<String> {
    vec![
        installer_file_name(version),
        format!("{}.sig", installer_file_name(version)),
        AGENT_BINARY.to_owned(),
        format!("{AGENT_BINARY}.minisig"),
        format!("{AGENT_BINARY}.sha256"),
        SUMS_NAME.to_owned(),
        MANIFEST_NAME.to_owned(),
        SCRIPT_NAME.to_owned(),
    ]
}

/// Clé d'objet d'un fichier de version : `releases/{version}/{nom}`.
pub fn version_key(version: &str, name: &str) -> String {
    format!("releases/{version}/{name}")
}

/// Ce que `release-stage` assemble.
pub struct StageInput<'a> {
    pub version: &'a str,
    /// `Cargo.toml` racine : seule source du numéro de version.
    pub cargo_toml: &'a str,
    /// `apps/desktop/src-tauri/update-key.pub`.
    pub client_key: &'a str,
    /// `crates/hearth-agent/update-key.pub`.
    pub agent_key: &'a str,
    pub installer: &'a [u8],
    /// Contenu du `.sig` de l'installateur (base64 d'une signature minisign).
    pub installer_signature: &'a str,
    /// `latest.json` du flux `publish-client` (client seul).
    pub latest_json: &'a str,
    pub notes: &'a str,
    pub agent: &'a [u8],
    /// Le `.minisig` de l'agent (texte, ou son base64).
    pub agent_signature: &'a str,
    pub install_script: &'a [u8],
}

/// Assemble les fichiers puis les REVÉRIFIE par `verify_stage`, le même code que `release-upload`
/// rejoue avant de déposer.
pub fn build_stage(input: &StageInput) -> Result<Files, String> {
    let version = input.version;
    check_version(version)?;
    check_repository_version(version, input.cargo_toml)?;
    check_key(input.client_key).map_err(|error| format!("clé du client : {error}"))?;
    check_key(input.agent_key).map_err(|error| format!("clé de l'agent : {error}"))?;

    let installer_name = installer_file_name(version);
    let installer_url = format!("{DOWNLOAD_PREFIX}{version}/{installer_name}");
    let signature = input.installer_signature.trim();
    let latest: Value = serde_json::from_str(input.latest_json)
        .map_err(|error| format!("latest.json illisible : {error}"))?;
    if latest["version"] != version {
        return Err(format!(
            "latest.json annonce la version {} et non {version} : mauvais artefact de publish-client ?",
            latest["version"]
        ));
    }
    if latest["platforms"][TARGET]["signature"]
        .as_str()
        .map(str::trim)
        != Some(signature)
    {
        return Err("la signature de latest.json n'est pas celle du fichier .sig : artefact mélangé, relance publish-client".to_owned());
    }
    if latest["platforms"][TARGET]["url"].as_str() != Some(&installer_url) {
        return Err(format!(
            "l'adresse de l'installateur dans latest.json doit être exactement {installer_url}"
        ));
    }
    let pub_date = latest["pub_date"]
        .as_str()
        .ok_or("latest.json n'a pas de pub_date")?;

    let client_manifest = manifest(version, input.notes, signature, &installer_url, pub_date)?;
    let agent_url = format!("{DOWNLOAD_PREFIX}{version}/{AGENT_BINARY}");
    let agent_sum = sha256_hex(input.agent);
    let section = agent_section(
        DOWNLOAD_PREFIX,
        version,
        input.agent_signature,
        &agent_sum,
        &agent_url,
    )?;
    let minisig = section["platforms"][AGENT_TARGET]["signature"]
        .as_str()
        .ok_or("signature de l'agent illisible")?
        .to_owned();
    let manifest_text = format!("{}\n", add_agent_section(&client_manifest, section)?);

    let mut files = Files::new();
    let mut put = |key: String, bytes: Vec<u8>| {
        files.insert(key, bytes);
    };
    put(
        version_key(version, &installer_name),
        input.installer.to_vec(),
    );
    put(
        version_key(version, &format!("{installer_name}.sig")),
        format!("{signature}\n").into_bytes(),
    );
    put(version_key(version, AGENT_BINARY), input.agent.to_vec());
    put(
        version_key(version, &format!("{AGENT_BINARY}.minisig")),
        format!("{minisig}\n").into_bytes(),
    );
    put(
        version_key(version, &format!("{AGENT_BINARY}.sha256")),
        format!("{agent_sum}  {AGENT_BINARY}\n").into_bytes(),
    );
    put(
        version_key(version, MANIFEST_NAME),
        manifest_text.clone().into_bytes(),
    );
    put(
        version_key(version, SCRIPT_NAME),
        input.install_script.to_vec(),
    );
    put(MANIFEST_KEY.to_owned(), manifest_text.into_bytes());
    put(SCRIPT_KEY.to_owned(), input.install_script.to_vec());
    let sums = sums_text(&files, version)?;
    files.insert(version_key(version, SUMS_NAME), sums.into_bytes());

    let checked = verify_stage(&files, input.client_key, input.agent_key, input.cargo_toml)?;
    debug_assert_eq!(checked, version);
    Ok(files)
}

/// `SHA256SUMS` au format de `sha256sum` : somme, deux espaces, nom ; trié par nom (ordre des octets,
/// comme `LC_ALL=C sort`) ; TOUS les fichiers de la version sauf lui-même.
pub fn sums_text(files: &Files, version: &str) -> Result<String, String> {
    let mut names: Vec<String> = release_file_names(version)
        .into_iter()
        .filter(|name| name != SUMS_NAME)
        .collect();
    names.sort();
    let mut text = String::new();
    for name in names {
        let bytes = files
            .get(&version_key(version, &name))
            .ok_or_else(|| format!("{name} manque dans la version {version}"))?;
        text.push_str(&format!("{}  {name}\n", sha256_hex(bytes)));
    }
    Ok(text)
}

fn text_of<'a>(files: &'a Files, key: &str) -> Result<&'a str, String> {
    let bytes = files.get(key).ok_or_else(|| format!("{key} manque"))?;
    std::str::from_utf8(bytes).map_err(|_| format!("{key} n'est pas du texte"))
}

/// Les clés d'un objet JSON sont EXACTEMENT `expected` (la forme que l'API lit : ni plus, ni moins).
fn exact_keys<'a>(
    value: &'a Value,
    path: &str,
    expected: &[&str],
) -> Result<&'a serde_json::Map<String, Value>, String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("manifeste : {path} doit être un objet"))?;
    let found: BTreeSet<&str> = object.keys().map(String::as_str).collect();
    let wanted: BTreeSet<&str> = expected.iter().copied().collect();
    if found == wanted {
        Ok(object)
    } else {
        Err(format!(
            "manifeste : {path} doit porter exactement les champs {} (trouvé : {})",
            expected.join(", "),
            found.into_iter().collect::<Vec<_>>().join(", ")
        ))
    }
}

fn string_at<'a>(
    object: &'a serde_json::Map<String, Value>,
    field: &str,
) -> Result<&'a str, String> {
    object
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("manifeste : {field} doit être un texte"))
}

/// Revérifie un dossier assemblé : ce que `release-stage` vient d'écrire, ou ce que `release-upload`
/// relit sur le disque avant de déposer. Rend la version. Refuse :
/// - un nom hors de la liste fermée de l'API, un fichier manquant, plus d'un dossier de version ;
/// - une version qui n'est pas celle du dépôt ;
/// - une signature du client (clé du dépôt) ou de l'agent (clé embarquée, version signée) fausse ;
/// - une somme (`SHA256SUMS`, `.sha256`, manifeste) qui ne correspond pas aux octets ;
/// - un manifeste d'une autre forme que celle de l'API, ou des adresses autres que
///   `{préfixe}{X.Y.Z}/{fichier}` ;
/// - un agent rangé ailleurs que dans le dossier de SA version (BR-INSTALL-013 : le pointeur doit
///   valoir `VERSION_DE_L_AGENT/hearth-agent-linux-x86_64`).
pub fn verify_stage(
    files: &Files,
    client_key: &str,
    agent_key: &str,
    cargo_toml: &str,
) -> Result<String, String> {
    let version = single_version_dir(files)?;
    check_repository_version(&version, cargo_toml)?;

    let names = release_file_names(&version);
    let prefix = format!("releases/{version}/");
    for key in files.keys() {
        if let Some(name) = key.strip_prefix(&prefix)
            && !names.iter().any(|allowed| allowed == name)
        {
            return Err(format!(
                "{key} : « {name} » n'est pas dans la liste fermée de l'API ({}) : retire ce fichier",
                names.join(", ")
            ));
        }
    }
    for name in &names {
        if !files.contains_key(&version_key(&version, name)) {
            return Err(format!(
                "{} manque : relance release-stage",
                version_key(&version, name)
            ));
        }
    }
    for key in [MANIFEST_KEY, SCRIPT_KEY] {
        if !files.contains_key(key) {
            return Err(format!("{key} manque : relance release-stage"));
        }
    }
    if files[MANIFEST_KEY] != files[&version_key(&version, MANIFEST_NAME)] {
        return Err(
            "releases/manifest.json et la copie du dossier de la version diffèrent".to_owned(),
        );
    }
    if files[SCRIPT_KEY] != files[&version_key(&version, SCRIPT_NAME)] {
        return Err(
            "releases/install.sh et la copie du dossier de la version diffèrent".to_owned(),
        );
    }

    check_install_script(&files[SCRIPT_KEY])?;
    check_sums(files, &version)?;

    let installer_name = installer_file_name(&version);
    verify_signature(
        client_key,
        text_of(
            files,
            &version_key(&version, &format!("{installer_name}.sig")),
        )?,
        &files[&version_key(&version, &installer_name)],
        &version,
    )
    .map_err(|error| format!("signature du client : {error}"))?;
    verify_agent_signature(
        agent_key,
        text_of(
            files,
            &version_key(&version, &format!("{AGENT_BINARY}.minisig")),
        )?,
        &files[&version_key(&version, AGENT_BINARY)],
        &version,
    )
    .map_err(|error| format!("signature de l'agent : {error}"))?;

    check_manifest(files, &version)?;
    Ok(version)
}

/// Garde sur le script déposé : il sera lancé en root sur les serveurs. Non vide, texte, première ligne
/// `#!/bin/sh` (sans retour chariot), il lit le pointeur `releases/latest/agent` de l'API et nomme le
/// binaire `hearth-agent-linux-x86_64`. Un script qui ne remplit pas ces conditions est l'ancien script
/// (celui d'avant la lecture du pointeur), un fichier vide ou un mauvais fichier.
pub fn check_install_script(script: &[u8]) -> Result<(), String> {
    if script.is_empty() {
        return Err("le script d'installation est vide".to_owned());
    }
    let text = std::str::from_utf8(script)
        .map_err(|_| "le script d'installation n'est pas du texte".to_owned())?;
    if text.split('\n').next() != Some("#!/bin/sh") {
        return Err("le script d'installation doit commencer par la ligne #!/bin/sh (sans retour chariot Windows)".to_owned());
    }
    for needle in ["releases/latest/agent", AGENT_BINARY] {
        if !text.contains(needle) {
            return Err(format!(
                "le script d'installation ne contient pas « {needle} » : c'est un ancien script ou un mauvais fichier (la lecture du pointeur releases/latest/agent arrive avec la PR de install.sh, à fusionner AVANT ce dépôt). Donne le bon par --install-script"
            ));
        }
    }
    Ok(())
}

/// Lance `git -C <root> ...` en lecture seule via l'exécuteur injecté. Un échec n'est jamais lu comme
/// « arbre propre » : il remonte en erreur.
fn git_text(host: &dyn Host, root: &Path, list: &[&str], why: &str) -> Result<String, String> {
    let mut argv = vec!["-C".to_owned(), root.to_string_lossy().into_owned()];
    argv.extend(list.iter().map(|arg| (*arg).to_owned()));
    // GIT_OPTIONAL_LOCKS=0 : `git status` ne réécrit pas l'index, la lecture reste une lecture.
    let env = [("GIT_OPTIONAL_LOCKS", "0")];
    let output = host.run(&Call::new("git", &argv).env(&env))?;
    if output.success {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(format!(
            "git {} a échoué ({}) : {why}",
            list.join(" "),
            output.stderr.chars().take(200).collect::<String>()
        ))
    }
}

/// Ce que la construction de l'agent lit sur le disque (`cargo build -p hearth-agent` dans un conteneur
/// qui monte le dépôt, voir `agent.rs`) : le manifeste et le verrou du workspace, la chaîne d'outils, la
/// configuration de Cargo, les requêtes SQLx hors ligne, et les crates (sources, `build.rs`, migrations
/// embarquées par `sqlx::migrate!`, clé publique de mise à jour). Un fichier NON SUIVI dans ces chemins
/// entre dans le binaire (une migration, un `build.rs`) : il est refusé. `target/`, `dist`, `.env`,
/// `*.log` (ignorés par `.gitignore`) et tout ce qui est hors de ces chemins (artefact du client,
/// signatures sous `target/dist/`) sont tolérés. `deploy/` n'y est pas : `install.sh` part de `HEAD`.
pub const BUILD_INPUTS: [&str; 6] = [
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    ".cargo",
    ".sqlx",
    "crates",
];

/// Les fichiers SUIVIS qui diffèrent de `HEAD` (modifiés, indexés, supprimés).
/// `git status --porcelain --untracked-files=no`, lecture seule. Vide = rien de suivi n'a changé.
pub fn tracked_changes(host: &dyn Host, root: &Path) -> Result<Vec<String>, String> {
    let out = git_text(
        host,
        root,
        &["status", "--porcelain", "--untracked-files=no"],
        "état de l'arbre inconnu, jamais traité comme propre",
    )?;
    Ok(out
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.trim_end().to_owned())
        .collect())
}

/// Les fichiers non suivis et non ignorés sous les entrées de la construction (`BUILD_INPUTS`).
pub fn untracked_inputs(host: &dyn Host, root: &Path) -> Result<Vec<String>, String> {
    untracked_under(host, root, &BUILD_INPUTS)
}

/// Les fichiers non suivis et non ignorés sous ces chemins.
pub fn untracked_under(
    host: &dyn Host,
    root: &Path,
    paths: &[&str],
) -> Result<Vec<String>, String> {
    let mut list = vec!["ls-files", "--others", "--exclude-standard", "--"];
    list.extend(paths);
    let out = git_text(
        host,
        root,
        &list,
        "fichiers non suivis inconnus, jamais traités comme absents",
    )?;
    Ok(out
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| format!("non suivi {}", line.trim_end()))
        .collect())
}

/// Les fichiers IGNORÉS sous les entrées de la construction. Rien d'ignoré n'y vit légitimement, et
/// l'ignorance peut venir du poste (`.git/info/exclude`, `core.excludesFile`), pas du commit : ranger
/// une migration d'essai ainsi la cacherait de `git status` sans l'ôter du binaire. Refusés.
pub fn ignored_inputs(host: &dyn Host, root: &Path) -> Result<Vec<String>, String> {
    ignored_under(host, root, &BUILD_INPUTS)
}

/// Les fichiers ignorés sous ces chemins (ou motifs git).
pub fn ignored_under(host: &dyn Host, root: &Path, paths: &[&str]) -> Result<Vec<String>, String> {
    let mut list = vec![
        "ls-files",
        "--others",
        "--ignored",
        "--exclude-standard",
        "--",
    ];
    list.extend(paths);
    let out = git_text(
        host,
        root,
        &list,
        "fichiers ignorés inconnus, jamais traités comme absents",
    )?;
    Ok(out
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| format!("ignoré {}", line.trim_end()))
        .collect())
}

/// Les fichiers que git est prié de ne pas regarder (`skip-worktree`, lettre `S` de `git ls-files -v`,
/// ou `assume-unchanged`, lettre minuscule) : modifiés sur le disque, absents de `git status`.
pub fn hidden_changes(host: &dyn Host, root: &Path) -> Result<Vec<String>, String> {
    let out = git_text(
        host,
        root,
        &["ls-files", "-v"],
        "fichiers masqués à git inconnus, jamais traités comme absents",
    )?;
    Ok(out
        .lines()
        .filter_map(|line| {
            let tag = line.chars().next()?;
            (tag == 'S' || tag.is_ascii_lowercase()).then(|| {
                format!(
                    "masqué à git ({tag}) {}",
                    line.get(2..).unwrap_or("").trim_end()
                )
            })
        })
        .collect())
}

/// Tout ce qui sépare l'arbre de `HEAD` pour la construction : fichiers suivis modifiés, fichiers non
/// suivis des entrées de la construction, fichiers masqués à git. Vide = l'arbre est le commit.
pub fn tree_problems(host: &dyn Host, root: &Path) -> Result<Vec<String>, String> {
    let mut problems = tracked_changes(host, root)?;
    problems.extend(untracked_inputs(host, root)?);
    problems.extend(ignored_inputs(host, root)?);
    problems.extend(hidden_changes(host, root)?);
    Ok(problems)
}

/// Entrées de la construction du CLIENT en plus de `BUILD_INPUTS` : le front et la coquille Tauri
/// (`apps/desktop`), dont les sources et les `capabilities` lues par glob entrent dans l'installateur.
pub const CLIENT_EXTRA_INPUTS: [&str; 1] = ["apps/desktop"];

/// Fichiers d'environnement que vite lit à la construction (`.env`, `.env.local`, `.env.production`,
/// `.env.production.local`…) : ignorés par le dépôt mais ils entrent dans l'installateur (un `.env`
/// développement y activerait des panneaux simulés). Refusés s'ils existent.
pub const CLIENT_ENV_FILES: &str = "apps/desktop/.env*";

/// Comme `tree_problems`, pour le client : les non suivis sont cherchés aussi sous `apps/desktop`. Les
/// IGNORÉS ne le sont pas sous `apps/desktop` (`node_modules`, `dist` y vivent légitimement, ignorés par
/// le dépôt) : seulement sous `BUILD_INPUTS` et, sous `apps/desktop`, les fichiers `.env*`.
pub fn client_tree_problems(host: &dyn Host, root: &Path) -> Result<Vec<String>, String> {
    let mut problems = tracked_changes(host, root)?;
    let mut paths = BUILD_INPUTS.to_vec();
    paths.extend(CLIENT_EXTRA_INPUTS);
    problems.extend(untracked_under(host, root, &paths)?);
    problems.extend(ignored_inputs(host, root)?);
    problems.extend(ignored_under(host, root, &[CLIENT_ENV_FILES])?);
    problems.extend(hidden_changes(host, root)?);
    Ok(problems)
}

/// Le commit de `HEAD` (`git rev-parse HEAD`).
pub fn head_commit(host: &dyn Host, root: &Path) -> Result<String, String> {
    let out = git_text(
        host,
        root,
        &["rev-parse", "HEAD"],
        "commit en place inconnu",
    )?;
    let commit = out.trim().to_owned();
    if commit.is_empty() || !commit.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "git rev-parse HEAD a rendu « {commit} » : pas un commit"
        ));
    }
    Ok(commit)
}

/// Refuse un arbre qui n'est pas son commit (HRT-60) : fichier suivi modifié (indexé ou non), fichier non
/// suivi dans les entrées de la construction, fichier masqué à git. Les autres fichiers non suivis sont
/// tolérés. Contrôle fait AU MOMENT de l'assemblage ou du dépôt : il ne prouve pas que le binaire donné
/// sort de ce commit (c'est l'enregistrement de construction, `check_agent_build`). Rend le commit en
/// place, lu une seule fois.
pub fn check_clean_tree(host: &dyn Host, root: &Path) -> Result<String, String> {
    let problems = tree_problems(host, root)?;
    if !problems.is_empty() {
        let shown = problems
            .iter()
            .take(DIRTY_LIST_MAX)
            .cloned()
            .collect::<Vec<_>>()
            .join(" ; ");
        let more = match problems.len().saturating_sub(DIRTY_LIST_MAX) {
            0 => String::new(),
            n => format!(" ; et {n} autre(s)"),
        };
        return Err(format!(
            "l'arbre de travail n'est pas le commit en place ({shown}{more}) : committe ou supprime ces fichiers (git stash -u pour les non suivis), retire skip-worktree et assume-unchanged, reconstruis l'agent, puis relance. Un binaire signé doit sortir du commit exact qui sera étiqueté"
        ));
    }
    head_commit(host, root)
}

/// Refuse un binaire d'agent qui ne sort pas du commit assemblé (HRT-60) : `record` est le texte de
/// `<binaire>.build`, écrit par `cargo xtask agent` après une construction réussie (voir `agent.rs`).
/// Somme, commit et état de l'arbre doivent correspondre. Fichier en clair, falsifiable à la main :
/// une garde contre l'erreur, pas contre la malveillance.
pub fn check_agent_build(record: &str, agent: &[u8], head: &str) -> Result<(), String> {
    let record = crate::agent::BuildRecord::parse(record)?;
    let sha = sha256_hex(agent);
    if record.sha256 != sha {
        return Err(format!(
            "le binaire de l'agent (sha256 {sha}) n'est pas celui que cargo xtask agent a construit (sha256 {}) : reconstruis-le et signe-le de nouveau",
            record.sha256
        ));
    }
    if record.commit != head {
        return Err(format!(
            "le binaire de l'agent a été construit sur le commit {} et HEAD est {head} : reviens sur {} ou reconstruis l'agent ici",
            record.commit, record.commit
        ));
    }
    if record.tree != "clean" {
        return Err(format!(
            "le binaire de l'agent a été construit sur un arbre « {} » (pas propre, ou état inconnu) : reconstruis-le sur un arbre propre et signe-le de nouveau",
            record.tree
        ));
    }
    Ok(())
}

/// Refuse un installateur du client qui ne sort pas du commit assemblé (HRT-173, BR-RELEASE-007) :
/// `record` est le texte de `<installateur>.build`, écrit par `cargo xtask client-record` après la
/// construction. Somme, commit et état de l'arbre doivent correspondre, comme pour l'agent. Fichier en
/// clair, falsifiable à la main : une garde contre l'erreur (installateur d'un autre commit glissé dans
/// le dossier), pas contre la malveillance.
pub fn check_client_build(record: &str, installer: &[u8], head: &str) -> Result<(), String> {
    let record = crate::agent::BuildRecord::parse_with(
        record,
        "reconstruis le client puis relance cargo xtask client-record",
    )?;
    let sha = sha256_hex(installer);
    if record.sha256 != sha {
        return Err(format!(
            "l'installateur du client (sha256 {sha}) n'est pas celui que cargo xtask client-record a enregistré (sha256 {}) : reconstruis-le et signe-le de nouveau",
            record.sha256
        ));
    }
    if record.commit != head {
        return Err(format!(
            "l'installateur du client a été construit sur le commit {} et HEAD est {head} : le client et l'agent d'une version sortent du même commit. Reviens sur {} ou reconstruis le client ici",
            record.commit, record.commit
        ));
    }
    if record.tree != "clean" {
        return Err(format!(
            "l'installateur du client a été construit sur un arbre « {} » (pas propre, ou état inconnu) : reconstruis-le sur un arbre propre et signe-le de nouveau",
            record.tree
        ));
    }
    Ok(())
}

/// Écrit le commit assemblé à côté de `releases/` (jamais déposé).
pub fn write_stage_commit(dir: &Path, commit: &str) -> Result<(), String> {
    let path = dir.join(COMMIT_FILE);
    std::fs::write(&path, format!("{commit}\n"))
        .map_err(|error| format!("{} : {error}", path.display()))
}

/// Relit le commit enregistré par `release-stage`. Absent ou illisible : refus (on ne devine pas).
pub fn read_stage_commit(dir: &Path) -> Result<String, String> {
    let path = dir.join(COMMIT_FILE);
    let text = std::fs::read_to_string(&path).map_err(|error| {
        format!(
            "{} illisible ({error}) : le commit assemblé est inconnu, relance cargo xtask release-stage",
            path.display()
        )
    })?;
    let commit = text.trim().to_owned();
    if commit.is_empty() || !commit.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "{} ne contient pas un commit : relance cargo xtask release-stage",
            path.display()
        ));
    }
    Ok(commit)
}

/// Avant tout dépôt (essai à blanc compris) : l'arbre est propre ET son commit est celui qui a été
/// assemblé. Seul `git` en lecture est lancé, jamais `aws` ni `curl`. Rend le commit.
pub fn check_stage_commit(
    host: &dyn Host,
    root: &Path,
    recorded: Result<String, String>,
) -> Result<String, String> {
    let recorded = recorded?;
    let current = check_clean_tree(host, root)?;
    if current != recorded {
        return Err(format!(
            "le commit en place ({current}) n'est pas celui qui a été assemblé ({recorded}) : reviens sur {recorded} (git checkout), ou relance cargo xtask release-stage ici"
        ));
    }
    Ok(current)
}

/// Sans `--install-script`, le script déposé est `deploy/install.sh` du dépôt : il ne doit pas différer
/// de `HEAD` (modifié, indexé ou non suivi). Commandes git en lecture seule, via l'exécuteur injecté.
/// Rend le commit de `HEAD`, que l'appelant affiche.
pub fn check_script_is_committed(host: &dyn Host, root: &Path) -> Result<String, String> {
    let git = |list: &[&str]| -> Result<String, String> {
        git_text(host, root, list, "impossible de vérifier deploy/install.sh")
            .map(|text| text.trim().to_owned())
    };
    let status = git(&["status", "--porcelain", "--", "deploy/install.sh"])?;
    if !status.is_empty() {
        return Err(format!(
            "deploy/install.sh diffère de HEAD ({status}) : committe-le, ou donne le fichier voulu par --install-script. Un script lancé en root ne part pas d'un fichier non committé"
        ));
    }
    git(&["rev-parse", "HEAD"])
}

/// Les octets de `deploy/install.sh` tels que dans `HEAD` (`git show`, lecture seule) : le fichier du
/// commit affiché, pas celui du disque (`skip-worktree`, lecture décalée).
fn committed_script_bytes(host: &dyn Host, root: &Path, commit: &str) -> Result<Vec<u8>, String> {
    let spec = format!("{commit}:deploy/install.sh");
    let argv = vec![
        "-C".to_owned(),
        root.to_string_lossy().into_owned(),
        "show".to_owned(),
        spec.clone(),
    ];
    let env = [("GIT_OPTIONAL_LOCKS", "0")];
    let output = host.run(&Call::new("git", &argv).env(&env))?;
    if output.success {
        Ok(output.stdout)
    } else {
        Err(format!(
            "git show {spec} a échoué ({}) : impossible de lire le script du commit",
            output.stderr.chars().take(200).collect::<String>()
        ))
    }
}

/// `expected_commit` est le commit déjà lu par `check_clean_tree` : le script est celui du même `HEAD`.
/// Choisit les octets du script à déposer et la ligne qui le dit. Sans option, ou si l'option désigne
/// `deploy/install.sh` du dépôt (comparaison après `canonicalize`), le script doit être celui de `HEAD`
/// et ce sont les octets de `HEAD` qui partent. Tout autre fichier est lu sur le disque, non comparé.
pub fn select_install_script(
    host: &dyn Host,
    root: &Path,
    expected_commit: &str,
    given: Option<&str>,
    read: &dyn Fn(&Path) -> Result<Vec<u8>, String>,
    canonical: &dyn Fn(&Path) -> Option<std::path::PathBuf>,
) -> Result<(Vec<u8>, String), String> {
    let is_repo_script = given.is_none_or(|path| {
        let repo = canonical(&root.join("deploy/install.sh"));
        repo.is_some() && canonical(Path::new(path)) == repo
    });
    if !is_repo_script {
        let path = given.unwrap_or_default();
        let bytes = read(Path::new(path))?;
        return Ok((
            bytes,
            format!("script : {path} (donné par --install-script, non comparé à HEAD)"),
        ));
    }
    let commit = check_script_is_committed(host, root)?;
    if commit != expected_commit {
        return Err(format!(
            "HEAD a changé pendant l'assemblage ({expected_commit} puis {commit}) : relance cargo xtask release-stage"
        ));
    }
    let bytes = committed_script_bytes(host, root, &commit)?;
    Ok((
        bytes,
        format!("script : deploy/install.sh tel que dans le commit {commit}"),
    ))
}

/// Un seul dossier de version sous `releases/`, et seulement les deux objets mutables à côté.
fn single_version_dir(files: &Files) -> Result<String, String> {
    let mut dirs = BTreeSet::new();
    for key in files.keys() {
        let rest = key
            .strip_prefix("releases/")
            .ok_or_else(|| format!("{key} : hors de releases/"))?;
        match rest.split_once('/') {
            None if rest == MANIFEST_NAME || rest == SCRIPT_NAME => {}
            None => {
                return Err(format!(
                    "{key} : hors de la liste fermée de l'API (seuls releases/manifest.json et releases/install.sh vivent à la racine)"
                ));
            }
            Some((dir, name)) if !dir.is_empty() && !name.is_empty() && !name.contains('/') => {
                dirs.insert(dir.to_owned());
            }
            Some(_) => return Err(format!("{key} : chemin inattendu")),
        }
    }
    let mut dirs = dirs.into_iter();
    match (dirs.next(), dirs.next()) {
        (Some(version), None) => {
            check_version(&version)?;
            Ok(version)
        }
        (None, _) => Err("aucun dossier de version sous releases/".to_owned()),
        (Some(first), Some(second)) => Err(format!(
            "plusieurs dossiers de version ({first}, {second}…) : une seule version à la fois, et l'agent doit être rangé dans le dossier de SA version"
        )),
    }
}

fn check_sums(files: &Files, version: &str) -> Result<(), String> {
    let expected = sums_text(files, version)?;
    let actual = text_of(files, &version_key(version, SUMS_NAME))?;
    if actual != expected {
        let listed: BTreeMap<&str, &str> = actual
            .lines()
            .filter_map(|line| line.split_once("  "))
            .map(|(sum, name)| (name, sum))
            .collect();
        for line in expected.lines() {
            if let Some((sum, name)) = line.split_once("  ")
                && listed.get(name) != Some(&sum)
            {
                return Err(format!(
                    "SHA256SUMS : la somme de {name} ne correspond pas au fichier (attendu {sum}) : relance release-stage"
                ));
            }
        }
        return Err("SHA256SUMS n'est pas au format de sha256sum (somme, deux espaces, nom), trié, tous les fichiers de la version : relance release-stage".to_owned());
    }
    let agent_sum = format!(
        "{}  {AGENT_BINARY}\n",
        sha256_hex(&files[&version_key(version, AGENT_BINARY)])
    );
    if text_of(
        files,
        &version_key(version, &format!("{AGENT_BINARY}.sha256")),
    )? != agent_sum
    {
        return Err(format!(
            "{AGENT_BINARY}.sha256 ne correspond pas au binaire de l'agent"
        ));
    }
    Ok(())
}

/// La forme exacte du manifeste que `manifest.ts` de l'API lit, ses adresses, et le rangement de
/// l'agent dans le dossier de sa version.
fn check_manifest(files: &Files, version: &str) -> Result<(), String> {
    let bytes = &files[MANIFEST_KEY];
    if bytes.len() > MANIFEST_MAX_BYTES {
        return Err("manifeste de plus de 256 Kio : l'API le refuserait".to_owned());
    }
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("releases/manifest.json illisible : {error}"))?;
    let top = exact_keys(
        &value,
        "la racine",
        &["version", "notes", "pub_date", "platforms", "agent"],
    )?;

    // L'agent d'abord : c'est la règle que le ticket HRT-51 ajoute (BR-INSTALL-013).
    let agent = exact_keys(&top["agent"], "agent", &["version", "platforms"])?;
    let agent_version = string_at(agent, "version")?;
    if agent_version != version {
        return Err(format!(
            "agent.version ({agent_version}) diffère du dossier où le binaire est rangé (releases/{version}/) : l'agent doit être rangé dans le dossier de SA version, sinon install.sh refuse le pointeur"
        ));
    }
    let agent_platforms = exact_keys(&agent["platforms"], "agent.platforms", &[AGENT_TARGET])?;
    let entry = exact_keys(
        &agent_platforms[AGENT_TARGET],
        "agent.platforms.linux-x86_64",
        &["url", "signature", "sha256"],
    )?;
    let agent_url = format!("{DOWNLOAD_PREFIX}{agent_version}/{AGENT_BINARY}");
    if string_at(entry, "url")? != agent_url {
        return Err(format!(
            "agent : l'adresse doit être exactement {agent_url}"
        ));
    }
    if string_at(entry, "sha256")? != sha256_hex(&files[&version_key(version, AGENT_BINARY)]) {
        return Err("agent : la somme du manifeste n'est pas celle du binaire".to_owned());
    }
    let minisig = text_of(
        files,
        &version_key(version, &format!("{AGENT_BINARY}.minisig")),
    )?;
    if string_at(entry, "signature")?.trim() != minisig.trim() {
        return Err("agent : la signature du manifeste n'est pas celle du .minisig".to_owned());
    }

    if string_at(top, "version")? != version {
        return Err(format!("manifeste : version différente de {version}"));
    }
    if string_at(top, "notes")?.chars().count() > NOTES_MAX_CHARS {
        return Err(format!(
            "notes de version : {NOTES_MAX_CHARS} caractères au plus"
        ));
    }
    let pub_date = string_at(top, "pub_date")?;
    if time::OffsetDateTime::parse(pub_date, &time::format_description::well_known::Rfc3339)
        .is_err()
    {
        return Err(format!("pub_date « {pub_date} » : attendu RFC 3339"));
    }
    let platforms = exact_keys(&top["platforms"], "platforms", &[TARGET])?;
    let client = exact_keys(
        &platforms[TARGET],
        "platforms.windows-x86_64",
        &["signature", "url"],
    )?;
    let installer_name = installer_file_name(version);
    let installer_url = format!("{DOWNLOAD_PREFIX}{version}/{installer_name}");
    if string_at(client, "url")? != installer_url {
        return Err(format!(
            "client : l'adresse doit être exactement {installer_url}"
        ));
    }
    let sig = text_of(
        files,
        &version_key(version, &format!("{installer_name}.sig")),
    )?;
    if string_at(client, "signature")?.trim() != sig.trim() {
        return Err("client : la signature du manifeste n'est pas celle du .sig".to_owned());
    }
    Ok(())
}

/// Lit un dossier assemblé (récursif, fichiers ordinaires seulement : un lien est refusé).
pub fn read_stage_dir(dir: &Path) -> Result<Files, String> {
    let mut files = Files::new();
    if !dir.is_dir() {
        return Err(format!(
            "{} n'existe pas : lance d'abord cargo xtask release-stage",
            dir.display()
        ));
    }
    visit(dir, "", &mut files)?;
    if files.is_empty() {
        return Err(format!(
            "{} est vide ou absent : lance d'abord cargo xtask release-stage",
            dir.display()
        ));
    }
    Ok(files)
}

fn visit(dir: &Path, relative: &str, files: &mut Files) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|error| format!("{} : {error}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("{} : {error}", dir.display()))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| format!("{} : nom illisible", dir.display()))?;
        if relative.is_empty() && name == COMMIT_FILE {
            // Le commit enregistré n'est pas un objet du bucket : `read_stage_commit` le lit à part.
            continue;
        }
        let key = if relative.is_empty() {
            name
        } else {
            format!("{relative}/{name}")
        };
        let path = entry.path();
        let kind = std::fs::symlink_metadata(&path)
            .map_err(|error| format!("{} : {error}", path.display()))?
            .file_type();
        if kind.is_dir() {
            visit(&path, &key, files)?;
        } else if kind.is_file() {
            let bytes =
                std::fs::read(&path).map_err(|error| format!("{} : {error}", path.display()))?;
            files.insert(key, bytes);
        } else {
            return Err(format!(
                "{} : ni fichier ni dossier (lien ?) : refusé",
                path.display()
            ));
        }
    }
    Ok(())
}

/// Écrit les fichiers sous `dir` après avoir vidé `dir/releases` (une relance repart de zéro).
pub fn write_stage_dir(dir: &Path, files: &Files) -> Result<(), String> {
    let releases = dir.join("releases");
    if releases.exists() {
        std::fs::remove_dir_all(&releases)
            .map_err(|error| format!("{} : {error}", releases.display()))?;
    }
    // Un commit enregistré par un assemblage précédent ne survit pas à celui-ci.
    let commit = dir.join(COMMIT_FILE);
    if commit.exists() {
        std::fs::remove_file(&commit).map_err(|error| format!("{} : {error}", commit.display()))?;
    }
    for (key, bytes) in files {
        let path = key
            .split('/')
            .fold(dir.to_path_buf(), |path, part| path.join(part));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("{} : {error}", parent.display()))?;
        }
        std::fs::write(&path, bytes).map_err(|error| format!("{} : {error}", path.display()))?;
    }
    Ok(())
}

/// Refuse un argument inconnu : une faute de frappe sur `--confirm` ne doit jamais passer pour un
/// essai à blanc.
pub fn check_flags(args: &[String], valued: &[&str], bare: &[&str]) -> Result<(), String> {
    let mut at = 0;
    while at < args.len() {
        let arg = args[at].as_str();
        if valued.contains(&arg) {
            if at + 1 >= args.len() {
                return Err(format!("{arg} demande une valeur"));
            }
            at += 2;
        } else if bare.contains(&arg) {
            at += 1;
        } else {
            return Err(format!("argument inconnu : {arg}"));
        }
    }
    Ok(())
}

/// Dossier d'assemblage par défaut, relatif à la racine du dépôt.
pub const STAGE_DIR: &str = "target/release-stage";

/// `cargo xtask release-stage --client-dir D --agent-binary F --agent-signature S [--install-script P]`.
pub fn run_stage(args: &[String]) -> Result<(), String> {
    stage_with(&System::new(), &root()?, args, &mut |line| {
        println!("{line}")
    })
}

/// Le corps de `release-stage`, avec l'exécuteur et la racine injectés (les tests y branchent un faux
/// git et un dossier jetable). L'ordre est la promesse : drapeaux, arbre propre, lectures, vérifications,
/// et seulement alors la première écriture.
pub fn stage_with(
    host: &dyn Host,
    root: &Path,
    args: &[String],
    say: &mut dyn FnMut(String),
) -> Result<(), String> {
    check_flags(
        args,
        &[
            "--client-dir",
            "--agent-binary",
            "--agent-signature",
            "--install-script",
        ],
        &[],
    )?;
    // Avant toute lecture et toute écriture : un arbre qui n'est pas son commit ne s'assemble pas (HRT-60).
    let commit = check_clean_tree(host, root)?;
    say(format!(
        "arbre propre (fichiers suivis, entrées de la construction), commit en place : {commit}"
    ));
    let read_text = |path: &Path| {
        std::fs::read_to_string(path).map_err(|error| format!("{} : {error}", path.display()))
    };
    let read_bytes =
        |path: &Path| std::fs::read(path).map_err(|error| format!("{} : {error}", path.display()));
    let cargo = read_text(&root.join("Cargo.toml"))?;
    let version = workspace_version(&cargo).ok_or("version du dépôt illisible")?;
    check_version(&version)?;

    let client_dir = Path::new(&required(args, "--client-dir")?).to_path_buf();
    let installer_name = installer_file_name(&version);
    let installer = read_bytes(&client_dir.join(&installer_name))?;
    // L'installateur doit sortir de CE commit : l'enregistrement de `cargo xtask client-record`, à côté de
    // lui (HRT-173). Jamais assemblé : seuls les fichiers de la liste fermée le sont.
    let client_record = read_text(&crate::agent::record_path(&client_dir.join(&installer_name)))
        .map_err(|error| {
            format!(
                "enregistrement de construction du client absent ou illisible ({error}) : reconstruis le client puis lance cargo xtask client-record sur ce commit, avant la signature"
            )
        })?;
    check_client_build(&client_record, &installer, &commit)?;
    let installer_signature = read_text(&client_dir.join(format!("{installer_name}.sig")))?;
    let latest_json = read_text(&client_dir.join("latest.json"))?;
    let notes = read_text(&client_dir.join("notes.txt"))?;
    let agent_path = required(args, "--agent-binary")?;
    let agent = read_bytes(Path::new(&agent_path))?;
    // Le binaire doit sortir de ce commit : l'enregistrement de `cargo xtask agent`, à côté de lui.
    let record_path = crate::agent::record_path(Path::new(&agent_path));
    let record = read_text(&record_path).map_err(|error| {
        format!(
            "enregistrement de construction de l'agent absent ou illisible ({error}) : reconstruis l'agent avec cargo xtask agent sur ce commit, puis signe-le"
        )
    })?;
    check_agent_build(&record, &agent, &commit)?;
    let agent_signature = read_text(Path::new(&required(args, "--agent-signature")?))?;
    let given = option(args, "--install-script");
    let (install_script, script_note) = select_install_script(
        host,
        root,
        &commit,
        given.as_deref(),
        &|path| read_bytes(path),
        &|path| std::fs::canonicalize(path).ok(),
    )?;
    say(script_note);
    let client_key = read_text(&root.join("apps/desktop/src-tauri/update-key.pub"))?;
    let agent_key = read_text(&root.join("crates/hearth-agent/update-key.pub"))?;

    let files = build_stage(&StageInput {
        version: &version,
        cargo_toml: &cargo,
        client_key: &client_key,
        agent_key: &agent_key,
        installer: &installer,
        installer_signature: &installer_signature,
        latest_json: &latest_json,
        notes: &notes,
        agent: &agent,
        agent_signature: &agent_signature,
        install_script: &install_script,
    })?;
    let stage = root.join(STAGE_DIR);
    write_stage_dir(&stage, &files)?;
    write_stage_commit(&stage, &commit)?;
    say(format!(
        "version {version} assemblée et revérifiée (signatures du client et de l'agent, sommes, adresses, liste fermée, binaire de l'agent construit sur ce commit) sous {STAGE_DIR}/ :"
    ));
    for (key, bytes) in &files {
        say(format!(
            "  {key}  {}  {} octets",
            sha256_hex(bytes),
            bytes.len()
        ));
    }
    say(
        "suite : cargo xtask release-upload (essai à blanc), puis cargo xtask release-upload --confirm"
            .to_owned(),
    );
    Ok(())
}

#[cfg(test)]
pub mod fixtures {
    //! Fabrique d'une version complète signée par des clés JETABLES, générées une fois par exécution des
    //! tests et jamais écrites : aucun secret, aucun fichier, aucun réseau.

    use std::io::Cursor;
    use std::sync::OnceLock;

    use super::*;

    pub const CARGO: &str = "[workspace.package]\nversion = \"0.1.0\"\n";
    pub const VERSION: &str = "0.1.0";
    /// Un script qui passe la garde (lecture du pointeur, nom du binaire) : le dépôt de cette branche a
    /// encore l'ANCIEN script, les tests fournissent le leur.
    pub const SCRIPT: &[u8] =
        b"#!/bin/sh\n# lit /api/releases/latest/agent, installe hearth-agent-linux-x86_64\necho installe\n";

    pub struct Keys {
        pub client_secret: String,
        pub client_public: String,
        pub other_client_secret: String,
        pub agent: minisign::KeyPair,
        pub agent_public: String,
        pub other_agent: minisign::KeyPair,
    }

    fn client_pair() -> (String, String) {
        let keys =
            minisign::KeyPair::generate_encrypted_keypair(Some("mot de passe".to_owned())).unwrap();
        (
            keys.sk.to_box(None).unwrap().to_string(),
            keys.pk.to_box().unwrap().to_string(),
        )
    }

    pub fn keys() -> &'static Keys {
        static KEYS: OnceLock<Keys> = OnceLock::new();
        KEYS.get_or_init(|| {
            let (client_secret, client_public) = client_pair();
            let (other_client_secret, _) = client_pair();
            let agent = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
            let agent_public = agent.pk.to_box().unwrap().to_string();
            Keys {
                client_secret,
                client_public,
                other_client_secret,
                agent,
                agent_public,
                other_agent: minisign::KeyPair::generate_unencrypted_keypair().unwrap(),
            }
        })
    }

    /// Comme `minisign -S -t "<comment>"`.
    pub fn sign_agent(keys: &minisign::KeyPair, data: &[u8], comment: &str) -> String {
        minisign::sign(
            None,
            &keys.sk,
            Cursor::new(data),
            Some(comment),
            Some("signature de test"),
        )
        .unwrap()
        .to_string()
    }

    pub fn sign_client(secret: &str, data: &[u8], version: &str) -> String {
        sign_installer(
            secret,
            "mot de passe",
            data,
            &installer_file_name(version),
            version,
            7,
        )
        .unwrap()
    }

    /// Les entrées d'une publication correcte de la 0.1.0 ; les tests en modifient un champ.
    pub struct Fixture {
        pub installer: Vec<u8>,
        pub installer_signature: String,
        pub latest_json: String,
        pub notes: String,
        pub agent: Vec<u8>,
        pub agent_signature: String,
        pub install_script: Vec<u8>,
    }

    impl Fixture {
        pub fn new() -> Self {
            let keys = keys();
            let installer = b"installateur".to_vec();
            let installer_signature = sign_client(&keys.client_secret, &installer, VERSION);
            let agent = b"agent".to_vec();
            let latest_json = serde_json::json!({
                "version": VERSION,
                "notes": "Corrections.",
                "pub_date": "2026-10-12T19:00:00Z",
                "platforms": { TARGET: {
                    "signature": installer_signature,
                    "url": format!("{DOWNLOAD_PREFIX}{VERSION}/{}", installer_file_name(VERSION)),
                } },
            })
            .to_string();
            Self {
                agent_signature: sign_agent(&keys.agent, &agent, "file:agent\tversion:0.1.0"),
                installer,
                installer_signature,
                latest_json,
                notes: "Corrections.\n".to_owned(),
                agent,
                install_script: SCRIPT.to_vec(),
            }
        }

        pub fn input(&self) -> StageInput<'_> {
            StageInput {
                version: VERSION,
                cargo_toml: CARGO,
                client_key: &keys().client_public,
                agent_key: &keys().agent_public,
                installer: &self.installer,
                installer_signature: &self.installer_signature,
                latest_json: &self.latest_json,
                notes: &self.notes,
                agent: &self.agent,
                agent_signature: &self.agent_signature,
                install_script: &self.install_script,
            }
        }

        pub fn staged(&self) -> Files {
            build_stage(&self.input()).unwrap()
        }
    }

    pub fn verify(files: &Files) -> Result<String, String> {
        verify_stage(files, &keys().client_public, &keys().agent_public, CARGO)
    }

    /// Faux git pour l'état de l'arbre (HRT-60). Répond à `status`, à `ls-files --others` (`untracked`),
    /// à `ls-files -v` (`hidden`), à `rev-parse HEAD` et à `show`, et enregistre chaque appel et chaque
    /// programme lancé (un programme autre que git, `aws` ou `curl`, y est visible).
    pub struct TreeFake {
        pub status: String,
        pub status_fails: bool,
        pub head: String,
        pub head_fails: bool,
        pub untracked: String,
        pub ignored: String,
        pub hidden: String,
        pub cannot_launch: bool,
        pub show: Vec<u8>,
        pub calls: std::cell::RefCell<Vec<Vec<String>>>,
        pub programs: std::cell::RefCell<Vec<String>>,
    }

    impl TreeFake {
        pub fn new(status: &str, head: &str) -> Self {
            Self {
                status: status.to_owned(),
                status_fails: false,
                head: head.to_owned(),
                head_fails: false,
                untracked: String::new(),
                ignored: String::new(),
                hidden: String::new(),
                cannot_launch: false,
                show: SCRIPT.to_vec(),
                calls: Default::default(),
                programs: Default::default(),
            }
        }

        pub fn only_git_ran(&self) -> bool {
            self.programs.borrow().iter().all(|p| p == "git")
        }
    }

    impl Host for TreeFake {
        fn run(&self, call: &Call) -> Result<crate::host::CmdOutput, String> {
            self.programs.borrow_mut().push(call.program.to_owned());
            if self.cannot_launch {
                return Err("« git » est introuvable".to_owned());
            }
            let ok = |stdout: Vec<u8>| crate::host::CmdOutput {
                success: true,
                code: Some(0),
                stdout,
                stderr: String::new(),
            };
            if call.program != "git" {
                return Ok(ok(Vec::new()));
            }
            self.calls.borrow_mut().push(call.args.to_vec());
            let has = |word: &str| call.args.iter().any(|arg| arg == word);
            let (fails, out): (bool, Vec<u8>) = if has("status") {
                (self.status_fails, format!("{}\n", self.status).into_bytes())
            } else if has("ls-files") && has("--ignored") {
                (false, format!("{}\n", self.ignored).into_bytes())
            } else if has("ls-files") && has("--others") {
                (false, format!("{}\n", self.untracked).into_bytes())
            } else if has("ls-files") {
                (false, format!("{}\n", self.hidden).into_bytes())
            } else if has("rev-parse") {
                (self.head_fails, format!("{}\n", self.head).into_bytes())
            } else if has("show") {
                (false, self.show.clone())
            } else {
                panic!("appel git inattendu : {:?}", call.args)
            };
            Ok(crate::host::CmdOutput {
                success: !fails,
                code: Some(if fails { 128 } else { 0 }),
                stdout: if fails { Vec::new() } else { out },
                stderr: if fails {
                    "fatal: boum".to_owned()
                } else {
                    String::new()
                },
            })
        }
        fn elapsed(&self) -> std::time::Duration {
            std::time::Duration::ZERO
        }
        fn sleep(&self, _: std::time::Duration) {}
    }

    /// Un dossier jetable sous les racines que `test-tmp-check` surveille ; à supprimer en fin de test.
    pub fn tmp_dir(name: &str) -> std::path::PathBuf {
        let exe = std::env::current_exe().unwrap();
        crate::test_tmp::tmp_root::roots_for(&exe)[0].join(format!("{name}-{}", std::process::id()))
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;
    use std::path::PathBuf;

    fn key(name: &str) -> String {
        version_key(VERSION, name)
    }

    /// Miroir de `latestInstallerPath` et `agentPointer` de `hearth-releases-api/src/service/manifest.ts`
    /// (lecture du manifeste par l'API) et de `release-files.ts` (liste fermée). Rend le chemin de
    /// l'installateur et la ligne `VERSION SOMME CHEMIN` du pointeur de l'agent, ou ce que l'API
    /// refuserait (502).
    fn api_accepts(manifest: &Value) -> Result<(String, String), String> {
        let prefix = "https://hearth.voikyrioh.fr/api/releases/";
        let semver = |v: &str| v.len() <= 32 && check_version(v).is_ok();
        // latestInstallerPath
        let url = manifest["platforms"]["windows-x86_64"]["url"]
            .as_str()
            .ok_or("installateur : pas d'adresse")?;
        let rest = url
            .strip_prefix(prefix)
            .ok_or("installateur : hors préfixe")?;
        let (dir, file) = rest.split_once('/').ok_or("installateur : forme")?;
        let installed = file
            .strip_prefix("Hearth_")
            .and_then(|f| f.strip_suffix("_x64-setup.exe"))
            .ok_or("installateur : nom")?;
        if dir != installed || !semver(dir) || dir.contains('/') {
            return Err("installateur : version".to_owned());
        }
        // agentPointer
        let agent = &manifest["agent"];
        let entry = &agent["platforms"]["linux-x86_64"];
        let agent_version = agent["version"].as_str().ok_or("agent : version")?;
        let sha = entry["sha256"].as_str().ok_or("agent : somme")?;
        let agent_url = entry["url"].as_str().ok_or("agent : adresse")?;
        if !semver(agent_version) {
            return Err("agent : version".to_owned());
        }
        if sha.len() != 64 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("agent : somme".to_owned());
        }
        let path = agent_url
            .strip_prefix(prefix)
            .ok_or("agent : hors préfixe")?;
        let allowed = |c: char| c.is_ascii_alphanumeric() || "._/-".contains(c);
        if !path.starts_with(|c: char| c.is_ascii_digit())
            || !path.chars().all(allowed)
            || path.contains("..")
        {
            return Err("agent : chemin".to_owned());
        }
        // BR-INSTALL-013 (ticket HRT-51, ajout du 2026-10-12) : install.sh exige ce chemin exact.
        if path != format!("{agent_version}/hearth-agent-linux-x86_64") {
            return Err(
                "agent : le chemin n'est pas VERSION_DE_L_AGENT/hearth-agent-linux-x86_64"
                    .to_owned(),
            );
        }
        Ok((
            format!("/api/releases/{installed}/{file}"),
            format!("{agent_version} {sha} {path}"),
        ))
    }

    #[test]
    fn the_stage_has_the_closed_list_of_the_api_and_the_two_mutable_objects() {
        let files = Fixture::new().staged();
        let mut expected: Vec<String> = release_file_names(VERSION)
            .iter()
            .map(|name| key(name))
            .collect();
        expected.push(MANIFEST_KEY.to_owned());
        expected.push(SCRIPT_KEY.to_owned());
        expected.sort();
        assert_eq!(files.keys().cloned().collect::<Vec<_>>(), expected);
        // La liste fermée de release-files.ts : 8 noms exactement.
        assert_eq!(
            release_file_names(VERSION),
            [
                "Hearth_0.1.0_x64-setup.exe",
                "Hearth_0.1.0_x64-setup.exe.sig",
                "hearth-agent-linux-x86_64",
                "hearth-agent-linux-x86_64.minisig",
                "hearth-agent-linux-x86_64.sha256",
                "SHA256SUMS",
                "manifest.json",
                "install.sh",
            ]
        );
        assert_eq!(verify(&files).unwrap(), VERSION);
    }

    #[test]
    fn sha256sums_is_in_the_format_of_sha256sum_sorted_with_every_other_file() {
        let fixture = Fixture::new();
        let files = fixture.staged();
        let text = String::from_utf8(files[&key(SUMS_NAME)].clone()).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines.len(),
            7,
            "tous les fichiers de la version sauf lui-même"
        );
        assert!(text.ends_with('\n'));
        let names: Vec<&str> = lines
            .iter()
            .map(|line| line.split_once("  ").unwrap().1)
            .collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "trié par nom, octet par octet");
        assert!(!names.contains(&SUMS_NAME));
        let first = lines[0];
        let (sum, name) = first.split_once("  ").unwrap();
        assert_eq!(sum.len(), 64);
        assert_eq!(sum, sha256_hex(&files[&key(name)]));
        assert_eq!(
            String::from_utf8(files[&key("hearth-agent-linux-x86_64.sha256")].clone()).unwrap(),
            format!(
                "{}  hearth-agent-linux-x86_64\n",
                sha256_hex(&fixture.agent)
            )
        );
    }

    /// FIGE la forme du manifeste que `hearth-releases-api` accepte : `src/service/manifest.ts`
    /// (`manifestSchema`, `latestInstallerPath`, `agentPointer`) et `src/service/release-files.ts`.
    /// Si l'API change de forme, ce test doit échouer ici avant qu'une version soit déposée.
    #[test]
    fn the_manifest_has_the_exact_shape_the_api_reads() {
        let fixture = Fixture::new();
        let files = fixture.staged();
        let value: Value = serde_json::from_slice(&files[MANIFEST_KEY]).unwrap();
        let object = value.as_object().unwrap();
        let mut fields: Vec<&str> = object.keys().map(String::as_str).collect();
        fields.sort_unstable();
        assert_eq!(
            fields,
            ["agent", "notes", "platforms", "pub_date", "version"]
        );
        assert_eq!(value["version"], VERSION);
        assert_eq!(value["notes"], "Corrections.");
        assert_eq!(value["pub_date"], "2026-10-12T19:00:00Z");
        assert_eq!(
            value["platforms"]["windows-x86_64"]["url"],
            "https://hearth.voikyrioh.fr/api/releases/0.1.0/Hearth_0.1.0_x64-setup.exe"
        );
        assert!(value["platforms"]["windows-x86_64"]["signature"].is_string());
        assert_eq!(value["agent"]["version"], VERSION);
        assert_eq!(
            value["agent"]["platforms"]["linux-x86_64"]["url"],
            "https://hearth.voikyrioh.fr/api/releases/0.1.0/hearth-agent-linux-x86_64"
        );
        assert_eq!(
            value["agent"]["platforms"]["linux-x86_64"]["sha256"],
            sha256_hex(&fixture.agent)
        );
        // Ce que l'API en dérive.
        let (installer, pointer) = api_accepts(&value).unwrap();
        assert_eq!(installer, "/api/releases/0.1.0/Hearth_0.1.0_x64-setup.exe");
        assert_eq!(
            pointer,
            format!(
                "0.1.0 {} 0.1.0/hearth-agent-linux-x86_64",
                sha256_hex(&fixture.agent)
            )
        );
        // La copie du dossier de la version est le même objet, octet pour octet.
        assert_eq!(files[MANIFEST_KEY], files[&key(MANIFEST_NAME)]);
    }

    #[test]
    fn a_script_that_would_not_read_the_pointer_or_is_empty_is_refused() {
        for (script, expected) in [
            (&b""[..], "vide"),
            // L'ancien script : télécharge depuis GitHub, ne lit pas le pointeur de l'API.
            (
                &b"#!/bin/sh\nURL=\"https://github.com/x/releases/latest/download/hearth-agent-linux-x86_64\"\n"[..],
                "releases/latest/agent",
            ),
            (&b"#!/bin/sh\n# releases/latest/agent\n"[..], "hearth-agent-linux-x86_64"),
            (
                &b"#!/bin/bash\n# releases/latest/agent hearth-agent-linux-x86_64\n"[..],
                "#!/bin/sh",
            ),
            (
                &b"# releases/latest/agent hearth-agent-linux-x86_64\n#!/bin/sh\n"[..],
                "#!/bin/sh",
            ),
            (
                &b"#!/bin/sh\r\n# releases/latest/agent hearth-agent-linux-x86_64\r\n"[..],
                "#!/bin/sh",
            ),
            (&b"\xff\xfe"[..], "pas du texte"),
        ] {
            let mut fixture = Fixture::new();
            fixture.install_script = script.to_vec();
            let error = build_stage(&fixture.input()).unwrap_err();
            assert!(error.contains(expected), "{expected} : {error}");
        }
        // Le même contrôle rejoue à l'envoi : un dossier assemblé dont le script a été remplacé après coup.
        let mut files = Fixture::new().staged();
        files.insert(SCRIPT_KEY.to_owned(), b"#!/bin/sh\necho vide\n".to_vec());
        files.insert(key(SCRIPT_NAME), b"#!/bin/sh\necho vide\n".to_vec());
        let sums = sums_text(&files, VERSION).unwrap();
        files.insert(key(SUMS_NAME), sums.into_bytes());
        assert!(
            verify(&files)
                .unwrap_err()
                .contains("releases/latest/agent")
        );
        // Le bon passe.
        assert!(check_install_script(SCRIPT).is_ok());
    }

    /// Faux git : répond à `status --porcelain` et `rev-parse HEAD`, enregistre les arguments.
    struct GitFake {
        status: &'static str,
        calls: std::cell::RefCell<Vec<Vec<String>>>,
    }

    impl Host for GitFake {
        fn run(&self, call: &Call) -> Result<crate::host::CmdOutput, String> {
            assert_eq!(call.program, "git");
            self.calls.borrow_mut().push(call.args.to_vec());
            if call.args.iter().any(|arg| arg == "show") {
                return Ok(crate::host::CmdOutput {
                    success: true,
                    code: Some(0),
                    stdout: COMMITTED_SCRIPT.to_vec(),
                    stderr: String::new(),
                });
            }
            let out = if call.args.iter().any(|arg| arg == "status") {
                self.status
            } else {
                "abc1234def"
            };
            Ok(crate::host::CmdOutput {
                success: true,
                code: Some(0),
                stdout: format!("{out}\n").into_bytes(),
                stderr: String::new(),
            })
        }
        fn elapsed(&self) -> std::time::Duration {
            std::time::Duration::ZERO
        }
        fn sleep(&self, _: std::time::Duration) {}
    }

    #[test]
    fn the_repository_script_must_be_the_committed_one_and_its_commit_is_shown() {
        let clean = GitFake {
            status: "",
            calls: Default::default(),
        };
        let commit = check_script_is_committed(&clean, Path::new("depot")).unwrap();
        assert_eq!(commit, "abc1234def");
        // Lecture seule, restreint au fichier.
        let calls = clean.calls.borrow();
        assert_eq!(calls.len(), 2);
        assert!(calls[0].ends_with(&[
            "status".to_owned(),
            "--porcelain".to_owned(),
            "--".to_owned(),
            "deploy/install.sh".to_owned()
        ]));
        assert_eq!(calls[1][2..], ["rev-parse", "HEAD"]);
        for call in calls.iter() {
            assert!(
                !call
                    .iter()
                    .any(|arg| ["commit", "add", "checkout", "reset", "stash"]
                        .contains(&arg.as_str())),
                "{call:?}"
            );
        }
        for status in [
            " M deploy/install.sh",
            "M  deploy/install.sh",
            "?? deploy/install.sh",
        ] {
            let dirty = GitFake {
                status,
                calls: Default::default(),
            };
            let error = check_script_is_committed(&dirty, Path::new("depot")).unwrap_err();
            assert!(
                error.contains("diffère de HEAD") && error.contains("--install-script"),
                "{status} : {error}"
            );
        }
    }

    const COMMITTED_SCRIPT: &[u8] = b"#!/bin/sh\n# script du commit\n";

    fn disk(path: &Path) -> Result<Vec<u8>, String> {
        assert_eq!(path, Path::new("autre.sh"), "seul l'autre fichier est lu");
        Ok(b"#!/bin/sh\n# script du disque\n".to_vec())
    }

    /// `depot/deploy/install.sh` et `deploy/install.sh` désignent le même fichier.
    fn canon(path: &Path) -> Option<std::path::PathBuf> {
        let text = path.to_string_lossy().replace('\\', "/");
        match text.as_str() {
            "depot/deploy/install.sh" | "deploy/install.sh" => Some("/R/deploy/install.sh".into()),
            "autre.sh" => Some("/R/autre.sh".into()),
            _ => None,
        }
    }

    #[test]
    fn without_the_option_the_bytes_of_head_are_staged_not_the_disk() {
        let fake = GitFake {
            status: "",
            calls: Default::default(),
        };
        let (bytes, note) =
            select_install_script(&fake, Path::new("depot"), "abc1234def", None, &disk, &canon)
                .unwrap();
        assert_eq!(bytes, COMMITTED_SCRIPT);
        assert!(
            note.contains("abc1234def") && note.contains("commit"),
            "{note}"
        );
        assert!(
            fake.calls
                .borrow()
                .iter()
                .any(|call| call[2..] == ["show", "abc1234def:deploy/install.sh"])
        );
    }

    #[test]
    fn the_option_naming_the_repository_script_keeps_the_committed_check() {
        let dirty = GitFake {
            status: " M deploy/install.sh",
            calls: Default::default(),
        };
        let error = select_install_script(
            &dirty,
            Path::new("depot"),
            "abc1234def",
            Some("deploy/install.sh"),
            &disk,
            &canon,
        )
        .unwrap_err();
        assert!(error.contains("diffère de HEAD"), "{error}");
        let clean = GitFake {
            status: "",
            calls: Default::default(),
        };
        let (bytes, _) = select_install_script(
            &clean,
            Path::new("depot"),
            "abc1234def",
            Some("deploy/install.sh"),
            &disk,
            &canon,
        )
        .unwrap();
        assert_eq!(bytes, COMMITTED_SCRIPT);
    }

    #[test]
    fn another_file_is_read_from_disk_and_not_compared_to_head() {
        let fake = GitFake {
            status: " M deploy/install.sh",
            calls: Default::default(),
        };
        let (bytes, note) = select_install_script(
            &fake,
            Path::new("depot"),
            "abc1234def",
            Some("autre.sh"),
            &disk,
            &canon,
        )
        .unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("disque"));
        assert!(note.contains("non comparé à HEAD"), "{note}");
        assert!(fake.calls.borrow().is_empty(), "aucun appel git");
    }

    #[test]
    fn a_failing_git_show_stops_the_staging() {
        struct NoShow;
        impl Host for NoShow {
            fn run(&self, call: &Call) -> Result<crate::host::CmdOutput, String> {
                let shown = call.args.iter().any(|arg| arg == "show");
                Ok(crate::host::CmdOutput {
                    success: !shown,
                    code: Some(if shown { 128 } else { 0 }),
                    stdout: if call.args.iter().any(|arg| arg == "rev-parse") {
                        b"abc1234def
"
                        .to_vec()
                    } else {
                        Vec::new()
                    },
                    stderr: "fatal: path not in HEAD".to_owned(),
                })
            }
            fn elapsed(&self) -> std::time::Duration {
                std::time::Duration::ZERO
            }
            fn sleep(&self, _: std::time::Duration) {}
        }
        let error = select_install_script(
            &NoShow,
            Path::new("depot"),
            "abc1234def",
            None,
            &disk,
            &canon,
        )
        .unwrap_err();
        assert!(error.contains("git show"), "{error}");
    }

    #[test]
    fn a_git_that_cannot_answer_stops_the_script_check() {
        struct Broken;
        impl Host for Broken {
            fn run(&self, _: &Call) -> Result<crate::host::CmdOutput, String> {
                Ok(crate::host::CmdOutput {
                    success: false,
                    code: Some(128),
                    stdout: Vec::new(),
                    stderr: "fatal: not a git repository".to_owned(),
                })
            }
            fn elapsed(&self) -> std::time::Duration {
                std::time::Duration::ZERO
            }
            fn sleep(&self, _: std::time::Duration) {}
        }
        let error = check_script_is_committed(&Broken, Path::new("depot")).unwrap_err();
        assert!(error.contains("impossible de vérifier"), "{error}");
    }

    #[test]
    fn a_wrong_sum_is_refused_and_names_the_file() {
        let mut files = Fixture::new().staged();
        // Un octet de l'installateur change après coup : la somme listée ne correspond plus.
        files.insert(key("Hearth_0.1.0_x64-setup.exe"), b"installateuR".to_vec());
        let error = verify(&files).unwrap_err();
        assert!(
            error.contains("SHA256SUMS") && error.contains("Hearth_0.1.0_x64-setup.exe"),
            "{error}"
        );
        // SHA256SUMS mal formé (un seul espace) ou non trié.
        let mut files = Fixture::new().staged();
        let text = String::from_utf8(files[&key(SUMS_NAME)].clone()).unwrap();
        files.insert(key(SUMS_NAME), text.replace("  ", " ").into_bytes());
        assert!(verify(&files).is_err());
        let mut files = Fixture::new().staged();
        let text = String::from_utf8(files[&key(SUMS_NAME)].clone()).unwrap();
        let mut lines: Vec<&str> = text.lines().collect();
        lines.reverse();
        files.insert(key(SUMS_NAME), (lines.join("\n") + "\n").into_bytes());
        assert!(verify(&files).unwrap_err().contains("trié"));
    }

    #[test]
    fn a_binary_changed_after_signing_is_refused_by_the_signature() {
        let mut fixture = Fixture::new();
        fixture.agent = b"autre agent".to_vec();
        // La signature date de l'ancien binaire.
        assert!(
            build_stage(&fixture.input())
                .unwrap_err()
                .contains("signature de l'agent")
        );
        let mut fixture = Fixture::new();
        fixture.installer = b"autre installateur".to_vec();
        assert!(
            build_stage(&fixture.input())
                .unwrap_err()
                .contains("signature du client")
        );
    }

    #[test]
    fn a_signature_of_another_key_is_refused_for_the_client_and_for_the_agent() {
        let keys = keys();
        let mut fixture = Fixture::new();
        let signature = sign_client(&keys.other_client_secret, &fixture.installer, VERSION);
        fixture.latest_json = fixture
            .latest_json
            .replace(&fixture.installer_signature, &signature);
        fixture.installer_signature = signature;
        let error = build_stage(&fixture.input()).unwrap_err();
        assert!(
            error.contains("signature du client") && error.contains("update-key.pub"),
            "{error}"
        );

        let mut fixture = Fixture::new();
        fixture.agent_signature = sign_agent(
            &keys.other_agent,
            &fixture.agent,
            "file:agent\tversion:0.1.0",
        );
        let error = build_stage(&fixture.input()).unwrap_err();
        assert!(
            error.contains("signature de l'agent")
                && error.contains("crates/hearth-agent/update-key.pub"),
            "{error}"
        );
    }

    #[test]
    fn a_signed_version_other_than_the_one_of_the_repository_is_refused() {
        let keys = keys();
        for comment in [
            "file:agent\tversion:0.0.9",
            "file:agent",
            "version:0.1.0\tversion:0.1.0",
        ] {
            let mut fixture = Fixture::new();
            fixture.agent_signature = sign_agent(&keys.agent, &fixture.agent, comment);
            let error = build_stage(&fixture.input()).unwrap_err();
            assert!(error.contains("version"), "{comment} : {error}");
        }
        // Le client : installateur signé en 0.0.9, publié en 0.1.0.
        let mut fixture = Fixture::new();
        let signature = sign_client(&keys.client_secret, &fixture.installer, "0.0.9");
        fixture.latest_json = fixture
            .latest_json
            .replace(&fixture.installer_signature, &signature);
        fixture.installer_signature = signature;
        assert!(build_stage(&fixture.input()).unwrap_err().contains("0.0.9"));
        // La version demandée n'est pas celle du dépôt.
        let fixture = Fixture::new();
        let mut input = fixture.input();
        input.cargo_toml = "[workspace.package]\nversion = \"0.2.0\"\n";
        assert!(build_stage(&input).unwrap_err().contains("dépôt"));
    }

    #[test]
    fn a_name_outside_the_closed_list_is_refused() {
        for extra in [
            "README.txt",
            "hearth-agent-linux-x86_64.exe",
            "Hearth_0.2.0_x64-setup.exe",
        ] {
            let mut files = Fixture::new().staged();
            files.insert(key(extra), b"x".to_vec());
            let error = verify(&files).unwrap_err();
            assert!(error.contains("liste fermée"), "{extra} : {error}");
        }
        let mut files = Fixture::new().staged();
        files.insert("releases/notes.txt".to_owned(), b"x".to_vec());
        assert!(verify(&files).unwrap_err().contains("liste fermée"));
        let mut files = Fixture::new().staged();
        files.insert("autre/x".to_owned(), b"x".to_vec());
        assert!(verify(&files).is_err());
        let mut files = Fixture::new().staged();
        files.remove(&key("install.sh"));
        assert!(verify(&files).unwrap_err().contains("manque"));
    }

    #[test]
    fn an_agent_not_stored_in_the_folder_of_its_version_is_refused() {
        // Le manifeste annonce un agent 0.0.9 alors que le binaire est rangé dans releases/0.1.0/.
        let mut files = Fixture::new().staged();
        let mut value: Value = serde_json::from_slice(&files[MANIFEST_KEY]).unwrap();
        value["agent"]["version"] = "0.0.9".into();
        value["agent"]["platforms"]["linux-x86_64"]["url"] =
            "https://hearth.voikyrioh.fr/api/releases/0.0.9/hearth-agent-linux-x86_64".into();
        let bytes = format!("{}\n", serde_json::to_string_pretty(&value).unwrap()).into_bytes();
        files.insert(MANIFEST_KEY.to_owned(), bytes.clone());
        files.insert(key(MANIFEST_NAME), bytes);
        let sums = sums_text(&files, VERSION).unwrap();
        files.insert(key(SUMS_NAME), sums.into_bytes());
        let error = verify(&files).unwrap_err();
        assert!(
            error.contains("dossier de SA version") && error.contains("0.0.9"),
            "{error}"
        );

        // Le manifeste dit 0.1.0 mais l'adresse pointe le dossier d'une autre version.
        let mut files = Fixture::new().staged();
        let mut value: Value = serde_json::from_slice(&files[MANIFEST_KEY]).unwrap();
        value["agent"]["platforms"]["linux-x86_64"]["url"] =
            "https://hearth.voikyrioh.fr/api/releases/0.0.9/hearth-agent-linux-x86_64".into();
        let bytes = format!("{}\n", serde_json::to_string_pretty(&value).unwrap()).into_bytes();
        files.insert(MANIFEST_KEY.to_owned(), bytes.clone());
        files.insert(key(MANIFEST_NAME), bytes);
        let sums = sums_text(&files, VERSION).unwrap();
        files.insert(key(SUMS_NAME), sums.into_bytes());
        assert!(verify(&files).unwrap_err().contains("exactement"));

        // Le binaire (et sa signature) rangés dans le dossier d'une autre version.
        let mut files = Fixture::new().staged();
        let binary = files.remove(&key(AGENT_BINARY)).unwrap();
        files.insert(version_key("0.0.9", AGENT_BINARY), binary);
        let error = verify(&files).unwrap_err();
        assert!(error.contains("plusieurs dossiers de version"), "{error}");
    }

    #[test]
    fn the_manifest_of_another_shape_is_refused() {
        let reshape = |edit: &dyn Fn(&mut Value)| {
            let mut files = Fixture::new().staged();
            let mut value: Value = serde_json::from_slice(&files[MANIFEST_KEY]).unwrap();
            edit(&mut value);
            let bytes = format!("{}\n", serde_json::to_string_pretty(&value).unwrap()).into_bytes();
            files.insert(MANIFEST_KEY.to_owned(), bytes.clone());
            files.insert(key(MANIFEST_NAME), bytes);
            let sums = sums_text(&files, VERSION).unwrap();
            files.insert(key(SUMS_NAME), sums.into_bytes());
            verify(&files)
        };
        assert!(
            reshape(&|v| v["extra"] = 1.into())
                .unwrap_err()
                .contains("champs")
        );
        assert!(
            reshape(&|v| {
                v.as_object_mut().unwrap().remove("notes");
            })
            .is_err()
        );
        assert!(
            reshape(&|v| v["agent"]["platforms"]["linux-arm64"] = serde_json::json!({})).is_err()
        );
        assert!(
            reshape(&|v| v["agent"]["platforms"]["linux-x86_64"]["sha256"] = "ab".into()).is_err()
        );
        assert!(reshape(&|v| v["pub_date"] = "hier".into()).is_err());
        assert!(
            reshape(&|v| v["platforms"]["windows-x86_64"]["url"] =
                "https://github.com/x/Hearth_0.1.0_x64-setup.exe".into())
            .is_err()
        );
        assert!(reshape(&|v| v["notes"] = "é".repeat(8_001).into()).is_err());
        // Contrôle : sans changement, la reconstruction passe.
        assert!(reshape(&|_| {}).is_ok());
    }

    #[test]
    fn the_development_key_and_a_mixed_up_artifact_are_refused() {
        let fixture = Fixture::new();
        let mut input = fixture.input();
        let dev =
            "untrusted comment: Hearth client DEV public key. No secret key exists.\nRWQabc\n";
        input.client_key = dev;
        assert!(build_stage(&input).unwrap_err().contains("DÉVELOPPEMENT"));

        let mut fixture = Fixture::new();
        fixture.latest_json = fixture.latest_json.replace("0.1.0/Hearth", "0.0.9/Hearth");
        assert!(
            build_stage(&fixture.input())
                .unwrap_err()
                .contains("exactement")
        );
        let mut fixture = Fixture::new();
        fixture.latest_json = fixture
            .latest_json
            .replace("\"version\":\"0.1.0\"", "\"version\":\"0.0.9\"");
        assert!(build_stage(&fixture.input()).unwrap_err().contains("0.0.9"));
        let mut fixture = Fixture::new();
        fixture.latest_json = fixture
            .latest_json
            .replace(&fixture.installer_signature, "c2ln");
        assert!(build_stage(&fixture.input()).unwrap_err().contains(".sig"));
    }

    #[test]
    fn a_stage_is_written_and_read_back_identically_and_flags_are_checked() {
        let exe = std::env::current_exe().unwrap();
        let base = crate::test_tmp::tmp_root::roots_for(&exe)[0]
            .join(format!("release_stage_roundtrip-{}", std::process::id()));
        let files = Fixture::new().staged();
        let outcome = (|| {
            write_stage_dir(&base, &files)?;
            // Une relance repart de zéro : un reste d'une autre version ne survit pas.
            std::fs::write(base.join("releases").join("reste.txt"), b"x")
                .map_err(|e| e.to_string())?;
            write_stage_dir(&base, &files)?;
            read_stage_dir(&base)
        })();
        let _ = std::fs::remove_dir_all(&base);
        assert_eq!(outcome.unwrap(), files);

        let args = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert!(check_flags(&args(&["--confirm"]), &["--stage-dir"], &["--confirm"]).is_ok());
        assert!(check_flags(&args(&["--confirmer"]), &["--stage-dir"], &["--confirm"]).is_err());
        assert!(check_flags(&args(&["--stage-dir"]), &["--stage-dir"], &[]).is_err());
    }

    #[test]
    fn a_clean_tree_is_accepted_and_its_commit_returned_with_read_only_calls() {
        let fake = TreeFake::new("", "abc1234def");
        assert_eq!(
            check_clean_tree(&fake, Path::new("depot")).unwrap(),
            "abc1234def"
        );
        let calls = fake.calls.borrow();
        assert_eq!(calls.len(), 5, "{calls:?}");
        assert_eq!(
            calls[0][2..],
            ["status", "--porcelain", "--untracked-files=no"]
        );
        assert_eq!(calls[1][2..4], ["ls-files", "--others"]);
        assert_eq!(calls[2][2..5], ["ls-files", "--others", "--ignored"]);
        assert_eq!(calls[3][2..], ["ls-files", "-v"]);
        assert_eq!(calls[4][2..], ["rev-parse", "HEAD"]);
        assert!(fake.only_git_ran());
    }

    #[test]
    fn a_tracked_change_staged_or_not_is_refused_and_names_the_files() {
        for status in [
            "M  Cargo.toml",
            " M Cargo.toml",
            "MM Cargo.toml",
            " D crates/hearth-agent/update-key.pub",
            "A  nouveau.rs",
        ] {
            let fake = TreeFake::new(status, "abc1234def");
            let error = check_clean_tree(&fake, Path::new("depot")).unwrap_err();
            let file = status.split(' ').next_back().unwrap();
            assert!(
                error.contains(file) && error.contains("committe") && error.contains("stash -u"),
                "{status} : {error}"
            );
        }
    }

    #[test]
    fn the_list_of_changed_files_is_bounded() {
        let status = (0..25)
            .map(|n| format!(" M f{n:02}.rs"))
            .collect::<Vec<_>>()
            .join("\n");
        let error = check_clean_tree(&TreeFake::new(&status, "abc"), Path::new("d")).unwrap_err();
        assert!(
            error.contains("f09.rs") && !error.contains("f10.rs"),
            "{error}"
        );
        assert!(error.contains("et 15 autre(s)"), "{error}");
    }

    #[test]
    fn a_git_that_fails_is_never_read_as_a_clean_tree() {
        let mut fake = TreeFake::new("", "abc1234def");
        fake.status_fails = true;
        let error = check_clean_tree(&fake, Path::new("depot")).unwrap_err();
        assert!(error.contains("jamais traité comme propre"), "{error}");
        let mut fake = TreeFake::new("", "abc1234def");
        fake.head_fails = true;
        assert!(check_clean_tree(&fake, Path::new("depot")).is_err());
        let fake = TreeFake::new("", "pas un commit");
        assert!(check_clean_tree(&fake, Path::new("depot")).is_err());
    }

    #[test]
    fn the_commit_record_is_read_back_and_is_not_part_of_the_deposited_files() {
        let exe = std::env::current_exe().unwrap();
        let base = crate::test_tmp::tmp_root::roots_for(&exe)[0]
            .join(format!("release_stage_commit-{}", std::process::id()));
        let files = Fixture::new().staged();
        let outcome = (|| {
            write_stage_dir(&base, &files)?;
            write_stage_commit(&base, "abc1234def")?;
            let commit = read_stage_commit(&base)?;
            let read = read_stage_dir(&base)?;
            // Un nouvel assemblage efface l'ancien commit.
            write_stage_dir(&base, &files)?;
            let after = read_stage_commit(&base);
            Ok::<_, String>((commit, read, after))
        })();
        let _ = std::fs::remove_dir_all(&base);
        let (commit, read, after) = outcome.unwrap();
        assert_eq!(commit, "abc1234def");
        assert_eq!(read, files, "COMMIT n'est pas un fichier de la version");
        assert!(verify(&read).is_ok());
        assert!(after.is_err());
    }

    #[test]
    fn an_upload_on_another_commit_or_without_record_or_on_a_dirty_tree_is_refused() {
        let root = Path::new("depot");
        let same = TreeFake::new("", "abc1234def");
        assert_eq!(
            check_stage_commit(&same, root, Ok("abc1234def".to_owned())).unwrap(),
            "abc1234def"
        );
        let other = TreeFake::new("", "fff9999aaa");
        let error = check_stage_commit(&other, root, Ok("abc1234def".to_owned())).unwrap_err();
        assert!(
            error.contains("fff9999aaa") && error.contains("abc1234def"),
            "{error}"
        );
        let missing = Err("COMMIT illisible".to_owned());
        let fake = TreeFake::new("", "abc1234def");
        assert!(check_stage_commit(&fake, root, missing).is_err());
        assert!(
            fake.calls.borrow().is_empty(),
            "rien n'est lancé sans enregistrement"
        );
        let dirty = TreeFake::new(" M Cargo.toml", "abc1234def");
        let error = check_stage_commit(&dirty, root, Ok("abc1234def".to_owned())).unwrap_err();
        assert!(error.contains("Cargo.toml"), "{error}");
        let mut broken = TreeFake::new("", "abc1234def");
        broken.status_fails = true;
        assert!(check_stage_commit(&broken, root, Ok("abc1234def".to_owned())).is_err());
    }

    #[test]
    fn a_missing_or_garbled_record_file_is_an_error() {
        let exe = std::env::current_exe().unwrap();
        let base = crate::test_tmp::tmp_root::roots_for(&exe)[0]
            .join(format!("release_stage_commit_bad-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let missing = read_stage_commit(&base);
        std::fs::write(base.join(COMMIT_FILE), "n'importe quoi\n").unwrap();
        let garbled = read_stage_commit(&base);
        let _ = std::fs::remove_dir_all(&base);
        assert!(missing.is_err() && garbled.is_err());
    }

    #[test]
    fn an_untracked_file_among_the_build_inputs_is_refused_and_named() {
        let mut fake = TreeFake::new("", "abc1234def");
        fake.untracked = "crates/hearth-agent/migrations/0005_x.sql".to_owned();
        let error = check_clean_tree(&fake, Path::new("depot")).unwrap_err();
        assert!(
            error.contains("crates/hearth-agent/migrations/0005_x.sql")
                && error.contains("non suivi")
                && error.contains("stash -u"),
            "{error}"
        );
    }

    #[test]
    fn the_untracked_check_is_limited_to_the_build_inputs_and_honours_gitignore() {
        let fake = TreeFake::new("", "abc1234def");
        check_clean_tree(&fake, Path::new("depot")).unwrap();
        let calls = fake.calls.borrow();
        let call = &calls[1];
        assert_eq!(
            call[2..],
            [
                "ls-files",
                "--others",
                "--exclude-standard",
                "--",
                "Cargo.toml",
                "Cargo.lock",
                "rust-toolchain.toml",
                ".cargo",
                ".sqlx",
                "crates"
            ]
        );
    }

    #[test]
    fn an_ignored_file_among_the_build_inputs_is_refused_and_named() {
        let mut fake = TreeFake::new("", "abc1234def");
        fake.ignored = "crates/hearth-agent/migrations/0005_essai.sql".to_owned();
        let error = check_clean_tree(&fake, Path::new("depot")).unwrap_err();
        assert!(
            error.contains("ignoré crates/hearth-agent/migrations/0005_essai.sql"),
            "{error}"
        );
        let calls = fake.calls.borrow();
        let call = calls
            .iter()
            .find(|c| c.contains(&"--ignored".to_owned()))
            .unwrap();
        assert!(
            call.contains(&"--exclude-standard".to_owned())
                && call.ends_with(&["crates".to_owned()])
        );
    }

    #[test]
    fn a_file_hidden_from_git_is_refused() {
        for hidden in ["S crates/hearth-agent/src/lib.rs", "h Cargo.lock"] {
            let mut fake = TreeFake::new("", "abc1234def");
            fake.hidden = format!("H Cargo.toml\n{hidden}");
            let error = check_clean_tree(&fake, Path::new("depot")).unwrap_err();
            let file = hidden.split(' ').next_back().unwrap();
            assert!(
                error.contains(file) && error.contains("masqué") && !error.contains("H Cargo"),
                "{hidden} : {error}"
            );
        }
    }

    #[test]
    fn git_is_asked_without_optional_locks() {
        struct Env(std::cell::RefCell<Vec<bool>>);
        impl Host for Env {
            fn run(&self, call: &Call) -> Result<crate::host::CmdOutput, String> {
                self.0
                    .borrow_mut()
                    .push(call.env.contains(&("GIT_OPTIONAL_LOCKS", "0")));
                Ok(crate::host::CmdOutput {
                    success: true,
                    code: Some(0),
                    stdout: if call.args.iter().any(|arg| arg == "rev-parse") {
                        b"abc
"
                        .to_vec()
                    } else {
                        Vec::new()
                    },
                    stderr: String::new(),
                })
            }
            fn elapsed(&self) -> std::time::Duration {
                std::time::Duration::ZERO
            }
            fn sleep(&self, _: std::time::Duration) {}
        }
        let env = Env(Default::default());
        check_clean_tree(&env, Path::new("depot")).unwrap();
        let seen = env.0.borrow();
        assert_eq!(seen.len(), 5);
        assert!(seen.iter().all(|locked| *locked));
    }

    #[test]
    fn the_script_of_another_head_is_refused() {
        let fake = GitFake {
            status: "",
            calls: Default::default(),
        };
        let error =
            select_install_script(&fake, Path::new("depot"), "fff0000", None, &disk, &canon)
                .unwrap_err();
        assert!(error.contains("HEAD a changé"), "{error}");
    }

    fn record_for(agent: &[u8], commit: &str, tree: &str) -> String {
        crate::agent::BuildRecord {
            commit: commit.to_owned(),
            tree: tree.to_owned(),
            sha256: sha256_hex(agent),
            version: VERSION.to_owned(),
        }
        .render()
    }

    #[test]
    fn an_agent_binary_needs_a_matching_build_record() {
        let agent = b"agent";
        let good = record_for(agent, "abc1234def", "clean");
        assert!(check_agent_build(&good, agent, "abc1234def").is_ok());
        // autre fichier que celui construit
        assert!(
            check_agent_build(&good, b"autre", "abc1234def")
                .unwrap_err()
                .contains("pas celui que cargo xtask agent a construit")
        );
        // autre commit
        assert!(
            check_agent_build(&good, agent, "fff0000")
                .unwrap_err()
                .contains("construit sur le commit abc1234def")
        );
        // arbre pas propre ou inconnu
        for tree in ["dirty", "unknown"] {
            let record = record_for(agent, "abc1234def", tree);
            assert!(
                check_agent_build(&record, agent, "abc1234def")
                    .unwrap_err()
                    .contains(tree)
            );
        }
        // illisible
        assert!(check_agent_build("n'importe quoi", agent, "abc1234def").is_err());
    }

    fn stage_args(client: &Path, agent: &Path, signature: &Path) -> Vec<String> {
        [
            "--client-dir",
            &client.to_string_lossy(),
            "--agent-binary",
            &agent.to_string_lossy(),
            "--agent-signature",
            &signature.to_string_lossy(),
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
    }

    #[test]
    fn the_stage_refuses_a_tree_that_is_not_its_commit_before_writing_anything() {
        let root = tmp_dir("stage_guard");
        std::fs::create_dir_all(&root).unwrap();
        let args = stage_args(&root.join("c"), &root.join("a"), &root.join("s"));
        let mut cases = Vec::new();
        cases.push(("README.md", TreeFake::new(" M README.md", "abc1234def")));
        let mut untracked = TreeFake::new("", "abc1234def");
        untracked.untracked = "crates/hearth-agent/migrations/0005_x.sql".to_owned();
        cases.push(("0005_x.sql", untracked));
        let mut hidden = TreeFake::new("", "abc1234def");
        hidden.hidden = "S Cargo.lock".to_owned();
        cases.push(("Cargo.lock", hidden));
        let mut broken = TreeFake::new("", "abc1234def");
        broken.status_fails = true;
        cases.push(("jamais traité comme propre", broken));
        for (expected, fake) in cases {
            let error = stage_with(&fake, &root, &args, &mut |_| {}).unwrap_err();
            assert!(error.contains(expected), "{expected} : {error}");
            assert!(!root.join(STAGE_DIR).exists(), "rien n'est écrit");
            assert!(fake.only_git_ran());
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Un dépôt jetable avec les entrées d'une version correcte, le dossier du client, l'agent, sa
    /// signature et son enregistrement de construction. Rend (racine, arguments, chemin de l'agent).
    fn staged_repo(
        name: &str,
        record: impl Fn(&[u8]) -> String,
    ) -> (PathBuf, Vec<String>, PathBuf) {
        let root = tmp_dir(name);
        let fixture = Fixture::new();
        let write = |path: PathBuf, bytes: &[u8]| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        };
        write(root.join("Cargo.toml"), CARGO.as_bytes());
        write(
            root.join("apps/desktop/src-tauri/update-key.pub"),
            keys().client_public.as_bytes(),
        );
        write(
            root.join("crates/hearth-agent/update-key.pub"),
            keys().agent_public.as_bytes(),
        );
        let client = root.join("client");
        let installer = installer_file_name(VERSION);
        write(client.join(&installer), &fixture.installer);
        write(
            client.join(format!("{installer}.sig")),
            fixture.installer_signature.as_bytes(),
        );
        write(
            crate::agent::record_path(&client.join(&installer)),
            record_for(&fixture.installer, "abc1234def", "clean").as_bytes(),
        );
        write(client.join("latest.json"), fixture.latest_json.as_bytes());
        write(client.join("notes.txt"), fixture.notes.as_bytes());
        let agent = root.join("dist/hearth-agent");
        write(agent.clone(), &fixture.agent);
        write(
            root.join("dist/hearth-agent.minisig"),
            fixture.agent_signature.as_bytes(),
        );
        write(
            crate::agent::record_path(&agent),
            record(&fixture.agent).as_bytes(),
        );
        let args = stage_args(&client, &agent, &root.join("dist/hearth-agent.minisig"));
        (root, args, agent)
    }

    #[test]
    fn the_stage_of_a_clean_tree_with_a_matching_build_writes_the_commit_record() {
        let (root, args, _) =
            staged_repo("stage_ok", |agent| record_for(agent, "abc1234def", "clean"));
        let fake = TreeFake::new("", "abc1234def");
        let mut lines = Vec::new();
        let outcome = stage_with(&fake, &root, &args, &mut |line| lines.push(line));
        let commit = read_stage_commit(&root.join(STAGE_DIR));
        let files = read_stage_dir(&root.join(STAGE_DIR));
        let record_staged = files
            .as_ref()
            .map(|files| files.keys().any(|key| key.contains("build")))
            .unwrap_or(true);
        let _ = std::fs::remove_dir_all(&root);
        outcome.unwrap();
        assert_eq!(commit.unwrap(), "abc1234def");
        assert!(verify(&files.unwrap()).is_ok());
        assert!(
            !record_staged,
            "l'enregistrement de construction n'est jamais assemblé"
        );
        assert!(lines.iter().any(|line| line.contains("abc1234def")));
    }

    #[test]
    fn the_stage_refuses_an_agent_without_a_matching_build_before_writing() {
        type Case = (&'static str, fn(&[u8]) -> String);
        let cases: [Case; 4] = [
            ("construit sur le commit", |a| {
                record_for(a, "fff0000", "clean")
            }),
            ("pas propre", |a| record_for(a, "abc1234def", "dirty")),
            ("pas celui que cargo xtask agent a construit", |_| {
                record_for(b"un autre binaire", "abc1234def", "clean")
            }),
            ("illisible", |_| "n'importe quoi".to_owned()),
        ];
        for (n, (expected, record)) in cases.into_iter().enumerate() {
            let (root, args, agent) = staged_repo(&format!("stage_ko{n}"), record);
            let fake = TreeFake::new("", "abc1234def");
            let error = stage_with(&fake, &root, &args, &mut |_| {}).unwrap_err();
            let written = root.join(STAGE_DIR).exists();
            // Sans enregistrement du tout.
            std::fs::remove_file(crate::agent::record_path(&agent)).unwrap();
            let missing = stage_with(&fake, &root, &args, &mut |_| {}).unwrap_err();
            let written_after = root.join(STAGE_DIR).exists();
            let _ = std::fs::remove_dir_all(&root);
            assert!(error.contains(expected), "{expected} : {error}");
            assert!(
                missing.contains("enregistrement de construction"),
                "{missing}"
            );
            assert!(!written && !written_after, "rien n'est écrit");
        }
    }

    #[test]
    fn an_installer_needs_a_matching_build_record() {
        let installer = b"installateur";
        let good = record_for(installer, "abc1234def", "clean");
        assert!(check_client_build(&good, installer, "abc1234def").is_ok());
        // autre fichier que celui enregistré
        assert!(
            check_client_build(&good, b"autre", "abc1234def")
                .unwrap_err()
                .contains("pas celui que cargo xtask client-record a enregistré")
        );
        // autre commit que HEAD : client et agent ne sortent pas du même commit
        assert!(
            check_client_build(&good, installer, "fff0000")
                .unwrap_err()
                .contains("construit sur le commit abc1234def")
        );
        // arbre pas propre ou inconnu
        for tree in ["dirty", "unknown"] {
            let record = record_for(installer, "abc1234def", tree);
            assert!(
                check_client_build(&record, installer, "abc1234def")
                    .unwrap_err()
                    .contains(tree)
            );
        }
        // illisible : le conseil est celui du client, pas celui de l'agent
        let error = check_client_build("n'importe quoi", installer, "abc1234def").unwrap_err();
        assert!(
            error.contains("client-record") && !error.contains("cargo xtask agent"),
            "{error}"
        );
    }

    #[test]
    fn the_stage_refuses_an_installer_without_a_matching_build_before_writing() {
        type Case = (&'static str, fn(&[u8]) -> String);
        let cases: [Case; 4] = [
            ("construit sur le commit", |i| {
                record_for(i, "fff0000", "clean")
            }),
            ("pas propre", |i| record_for(i, "abc1234def", "dirty")),
            (
                "pas celui que cargo xtask client-record a enregistré",
                |_| record_for(b"un autre installateur", "abc1234def", "clean"),
            ),
            ("illisible", |_| "n'importe quoi".to_owned()),
        ];
        for (n, (expected, record)) in cases.into_iter().enumerate() {
            let (root, args, _) = staged_repo(&format!("stage_client_ko{n}"), |agent| {
                record_for(agent, "abc1234def", "clean")
            });
            let installer = root.join("client").join(installer_file_name(VERSION));
            let record_file = crate::agent::record_path(&installer);
            std::fs::write(&record_file, record(b"installateur")).unwrap();
            let fake = TreeFake::new("", "abc1234def");
            let error = stage_with(&fake, &root, &args, &mut |_| {}).unwrap_err();
            let written = root.join(STAGE_DIR).exists();
            // Sans enregistrement du client du tout.
            std::fs::remove_file(&record_file).unwrap();
            let missing = stage_with(&fake, &root, &args, &mut |_| {}).unwrap_err();
            let written_after = root.join(STAGE_DIR).exists();
            let _ = std::fs::remove_dir_all(&root);
            assert!(error.contains(expected), "{expected} : {error}");
            assert!(
                missing.contains("enregistrement de construction du client"),
                "{missing}"
            );
            assert!(!written && !written_after, "rien n'est écrit");
        }
    }
}
