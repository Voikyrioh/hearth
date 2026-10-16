//! `cargo xtask release-preflight` (HRT-172, BR-RELEASE-004, 006 et 008) : les contrôles qui refusent une
//! publication AVANT toute construction. Aucun secret, aucune écriture (hors les sorties du job).
//!
//! Ordre (conception « publier une version » 7.2) : 1. la branche ; 2. la concordance des numéros dans
//! les fichiers du dépôt ; 3. les notes ; 4. la clé publique de développement ; 5. `HEAD` est le commit
//! lancé ; 6. l'état publié (manifeste public, étiquette, release GitHub). Le premier refus arrête tout
//! et dit le message de la spec fonctionnelle (section 4) ; les contrôles suivants ne sont pas lancés
//! (aucun appel réseau si la branche, les numéros ou les notes sont mauvais).
//!
//! `--versions-only` ne fait que 2 et 4, sans réseau ni variable : c'est ce que la CI lance à chaque PR,
//! pour qu'un numéro discordant soit vu à la PR de montée de version et non au lancement du flux.
//!
//! Variables lues (flux de publication) : `HEARTH_REF` (`github.ref`), `HEARTH_SHA` (`github.sha`),
//! `HEARTH_NOTES`, `GH_TOKEN` (jeton automatique du job, passé à `curl` par l'entrée standard, jamais
//! en argument). Sorties (`$GITHUB_OUTPUT`) : `version`, `commit`.
//!
//! Doctrine de lecture (BR-RELEASE-001, leçon de HRT-171) : une ABSENCE ne se déduit jamais d'un texte
//! d'erreur. Manifeste et release se lisent par code HTTP exact (200 présent, 404 absent, tout autre
//! code ou réponse illisible = arrêt), l'étiquette par le code de sortie de `git ls-remote`.

use std::path::Path;

use serde_json::Value;

use crate::ci;
use crate::host::{Call, CmdOutput, Host, System};
use crate::release_core::*;

/// Le dépôt public dont la release est lue.
const REPOSITORY: &str = "Voikyrioh/hearth";
const CLIENT_KEY: &str = "apps/desktop/src-tauri/update-key.pub";
const AGENT_KEY: &str = "crates/hearth-agent/update-key.pub";
/// Longueur maximale des notes, en caractères Unicode.
pub const NOTES_MAX: usize = 2000;
/// Les paquets du dépôt dont `Cargo.lock` doit suivre `[workspace.package]`.
const LOCKED_PACKAGES: [&str; 5] = [
    "hearth-agent",
    "hearth-desktop",
    "hearth-link",
    "hearth-proto",
    "xtask",
];
const CARGO_TOML: &str = "Cargo.toml";
const CARGO_LOCK: &str = "Cargo.lock";
const PACKAGE_JSON: &str = "apps/desktop/package.json";
const PACKAGE_LOCK: &str = "apps/desktop/package-lock.json";
const TAURI_CONF: &str = "apps/desktop/src-tauri/tauri.conf.json";

// --- 1. la branche ------------------------------------------------------------------------------

/// BR-RELEASE-004 : seule la tête de `main` se publie (`github.ref` d'un lancement à la main).
pub fn check_ref(git_ref: &str) -> Result<(), String> {
    if git_ref == "refs/heads/main" {
        Ok(())
    } else {
        Err("La publication ne part que de main.".to_owned())
    }
}

// --- 2. les numéros -----------------------------------------------------------------------------

/// Ce qu'on attend d'un fichier : le numéro du dépôt, ou aucun numéro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expect {
    Same,
    Absent,
}

/// Un fichier (ou un champ) et le numéro qu'on y a lu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionEntry {
    pub label: String,
    pub found: Option<String>,
    pub expect: Expect,
}

fn entry(label: &str, found: Option<String>, expect: Expect) -> VersionEntry {
    VersionEntry {
        label: label.to_owned(),
        found,
        expect,
    }
}

/// Les versions des paquets du dépôt dans `Cargo.lock` (bloc `[[package]]`, ligne `name` puis `version`).
fn locked_versions(lock: &str) -> Vec<(&'static str, Option<String>)> {
    let mut found: Vec<(&'static str, Option<String>)> =
        LOCKED_PACKAGES.iter().map(|name| (*name, None)).collect();
    let mut current: Option<usize> = None;
    for line in lock.lines() {
        let line = line.trim();
        if line == "[[package]]" {
            current = None;
        } else if let Some(name) = line
            .strip_prefix("name = \"")
            .and_then(|rest| rest.strip_suffix('"'))
        {
            current = LOCKED_PACKAGES.iter().position(|known| *known == name);
        } else if let (Some(at), Some(version)) = (
            current,
            line.strip_prefix("version = \"")
                .and_then(|rest| rest.strip_suffix('"')),
        ) {
            found[at].1 = Some(version.to_owned());
            current = None;
        }
    }
    found
}

/// Un fichier qui porte plusieurs numéros : une seule ligne s'ils sont tous les mêmes, une par numéro
/// sinon (le message doit dire LEQUEL diffère).
fn grouped(label: &str, parts: Vec<(String, Option<String>)>) -> Vec<VersionEntry> {
    let all_equal = parts.windows(2).all(|pair| pair[0].1 == pair[1].1);
    if all_equal {
        let found = parts.into_iter().next().and_then(|part| part.1);
        vec![entry(label, found, Expect::Same)]
    } else {
        parts
            .into_iter()
            .map(|(part, found)| entry(&format!("{label} ({part})"), found, Expect::Same))
            .collect()
    }
}

/// Les cinq fichiers qui portent (ou ne doivent pas porter) le numéro de version, dans l'ordre de la
/// conception, source d'abord. `read` rend le texte d'un fichier du dépôt (chemin relatif) ou rien.
pub fn version_files(read: &dyn Fn(&str) -> Option<String>) -> Vec<VersionEntry> {
    let json = |path: &str| read(path).and_then(|text| serde_json::from_str::<Value>(&text).ok());
    let text_of = |value: &Value| value.as_str().map(str::to_owned);
    let mut entries = vec![entry(
        CARGO_TOML,
        read(CARGO_TOML).and_then(|text| workspace_version(&text)),
        Expect::Same,
    )];
    let locked = read(CARGO_LOCK)
        .map(|text| locked_versions(&text))
        .unwrap_or_else(|| LOCKED_PACKAGES.iter().map(|name| (*name, None)).collect());
    entries.extend(grouped(
        CARGO_LOCK,
        locked
            .into_iter()
            .map(|(name, found)| (name.to_owned(), found))
            .collect(),
    ));
    let package = json(PACKAGE_JSON);
    entries.push(entry(
        PACKAGE_JSON,
        package
            .as_ref()
            .and_then(|value| text_of(&value["version"])),
        Expect::Same,
    ));
    let lock = json(PACKAGE_LOCK);
    entries.extend(grouped(
        PACKAGE_LOCK,
        vec![
            (
                "version".to_owned(),
                lock.as_ref().and_then(|value| text_of(&value["version"])),
            ),
            (
                "packages[\"\"]".to_owned(),
                lock.as_ref()
                    .and_then(|value| text_of(&value["packages"][""]["version"])),
            ),
        ],
    ));
    // Tauri prend la version du crate si le champ manque ; un champ la ferait prévaloir sur Cargo.
    entries.push(entry(
        TAURI_CONF,
        json(TAURI_CONF).and_then(|value| text_of(&value["version"])),
        Expect::Absent,
    ));
    entries
}

/// BR-RELEASE-006 : tous les fichiers portent le numéro de `Cargo.toml` (source unique), `tauri.conf.json`
/// n'en porte aucun, et ce numéro est `X.Y.Z` strict. Rend le numéro.
pub fn check_versions(entries: &[VersionEntry]) -> Result<String, String> {
    let source = entries
        .first()
        .and_then(|first| first.found.clone())
        .filter(|_| {
            entries
                .first()
                .is_some_and(|first| first.label == CARGO_TOML)
        });
    let agrees = |entry: &VersionEntry| match entry.expect {
        Expect::Same => entry.found.is_some() && entry.found == source,
        Expect::Absent => entry.found.is_none(),
    };
    if source.is_none() || !entries.iter().all(agrees) {
        let list = entries
            .iter()
            .map(|entry| match (&entry.found, entry.expect) {
                (Some(found), _) => format!("{} → {found}", entry.label),
                (None, _) => format!("{} → aucun numéro", entry.label),
            })
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "Les numéros de version du dépôt ne concordent pas : {list}."
        ));
    }
    let version = source.unwrap_or_default();
    check_version(&version)?;
    Ok(version)
}

// --- 3. les notes -------------------------------------------------------------------------------

/// Spec fonctionnelle section 2 : non vides une fois les espaces retirés, 2000 caractères au plus
/// (caractères Unicode, pas octets).
pub fn check_notes(notes: &str) -> Result<(), String> {
    let trimmed = notes.trim();
    if trimmed.is_empty() {
        return Err("Les notes de version sont requises.".to_owned());
    }
    if trimmed.chars().count() > NOTES_MAX {
        return Err(format!(
            "Les notes de version dépassent {NOTES_MAX} caractères."
        ));
    }
    Ok(())
}

// --- 4. la clé de développement -----------------------------------------------------------------

/// Aucune clé publique de développement dans le dépôt : ni celle du client ni celle de l'agent.
pub fn check_keys(read: &dyn Fn(&str) -> Option<String>) -> Result<(), String> {
    for path in [CLIENT_KEY, AGENT_KEY] {
        let key = read(path).ok_or_else(|| format!("{path} est illisible"))?;
        check_key(&key).map_err(|error| {
            if path == CLIENT_KEY {
                error
            } else {
                format!("{path} : {error}")
            }
        })?;
    }
    Ok(())
}

// --- 5. HEAD ------------------------------------------------------------------------------------

/// Le dépôt extrait est le commit lancé (`github.sha`) : tout le flux construit, signe et étiquette
/// CE commit (règle « même commit » du flux, HRT-175).
pub fn check_head(head: &str, sha: &str) -> Result<(), String> {
    let hex = |text: &str| text.len() == 40 && text.bytes().all(|b| b.is_ascii_hexdigit());
    if !hex(sha) {
        return Err(
            "Le commit lancé (HEARTH_SHA) est absent ou illisible : publication arrêtée."
                .to_owned(),
        );
    }
    if head.eq_ignore_ascii_case(sha) {
        Ok(())
    } else {
        let short = |text: &str| text.chars().take(12).collect::<String>();
        Err(format!(
            "Le dépôt extrait ({}) n'est pas le commit lancé ({}) : publication arrêtée.",
            short(head),
            short(sha)
        ))
    }
}

// --- 6. l'état publié ---------------------------------------------------------------------------

/// Ce que le manifeste PUBLIC annonce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Manifest {
    /// Aucune version publiée (404).
    Absent,
    /// La plus récente des versions annoncées (client, agent).
    Version(String),
    /// Réponse qu'on ne sait pas lire : jamais prise pour une absence.
    Unreadable(String),
}

fn triple_of(version: &str) -> Option<(u64, u64, u64)> {
    check_version(version).ok()?;
    let mut parts = version.split('.').map(|part| part.parse::<u64>().ok());
    Some((parts.next()??, parts.next()??, parts.next()??))
}

/// BR-RELEASE-008 : table de la conception 9.2. « Publiée » = le manifeste l'annonce ET la release
/// existe ; une étiquette (ou une release) sans manifeste qui l'annonce est une anomalie.
pub fn published_state(
    version: &str,
    manifest: &Manifest,
    tag: bool,
    release: bool,
) -> Result<(), String> {
    let announced = match manifest {
        Manifest::Unreadable(cause) => {
            return Err(format!(
                "Le manifeste public n'a pas pu être lu ({cause}) : publication arrêtée."
            ));
        }
        Manifest::Absent => None,
        Manifest::Version(found) => Some(found.as_str()),
    };
    match announced {
        Some(found) if triple_of(found) > triple_of(version) => Err(format!(
            "Le manifeste annonce déjà une version plus récente ({found}) que celle du dépôt ({version})."
        )),
        Some(found) if found == version && release => {
            Err(format!("La version {version} est déjà publiée."))
        }
        Some(found) if found == version => Err(format!(
            "La version {version} est annoncée mais sa release GitHub manque : relancez l'étape en échec de l'exécution d'origine."
        )),
        _ if tag || release => Err(format!(
            "L'étiquette v{version} existe mais le manifeste ne l'annonce pas : à examiner à la main."
        )),
        _ => Ok(()),
    }
}

/// Une cause lisible et bornée : première ligne, 120 caractères.
fn short(text: &str) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    let mut cut: String = line.chars().take(120).collect();
    if line.chars().count() > 120 {
        cut.push('…');
    }
    cut
}

/// Bornes de `curl` : connexion 15 s, réponse 60 s. Pas de `--fail` (le corps d'un 404 est lu),
/// pas de `--location` (une redirection est un code autre que 200 : arrêt), HTTPS 1.2 minimum.
fn curl_args(extra: &[&str], url: &str) -> Vec<String> {
    let mut args: Vec<String> = [
        "--disable",
        "--silent",
        "--show-error",
        "--proto",
        "=https",
        "--tlsv1.2",
        "--connect-timeout",
        "15",
        "--max-time",
        "60",
        "--write-out",
        "\n%{http_code}",
    ]
    .iter()
    .map(|arg| (*arg).to_owned())
    .collect();
    args.extend(extra.iter().map(|arg| (*arg).to_owned()));
    args.push(url.to_owned());
    args
}

/// Corps et code HTTP d'une réponse de `curl --write-out "\n%{http_code}"`.
fn http_reply(output: &CmdOutput) -> Result<(u16, &[u8]), String> {
    if !output.success {
        return Err(format!("curl : {}", short(&output.stderr)));
    }
    let at = output
        .stdout
        .iter()
        .rposition(|byte| *byte == b'\n')
        .ok_or("réponse sans code HTTP")?;
    let code = std::str::from_utf8(&output.stdout[at + 1..])
        .ok()
        .and_then(|text| text.trim().parse::<u16>().ok())
        .ok_or("code HTTP illisible")?;
    Ok((code, &output.stdout[..at]))
}

/// Ce que le manifeste public (`GET …/api/releases/latest`) annonce. 200 + JSON dont `version` (et
/// `agent.version` s'il existe) est `X.Y.Z` = la plus récente des deux ; 404 = aucune version ; tout
/// le reste (autre code, page HTML d'un défi Cloudflare, JSON d'une autre forme, curl en échec) =
/// illisible.
pub fn read_public_manifest(host: &dyn Host) -> Manifest {
    let url = format!("{DOWNLOAD_PREFIX}latest");
    let output = match host.run(&Call::new("curl", &curl_args(&[], &url))) {
        Ok(output) => output,
        Err(error) => return Manifest::Unreadable(short(&error)),
    };
    let (code, body) = match http_reply(&output) {
        Ok(reply) => reply,
        Err(error) => return Manifest::Unreadable(error),
    };
    match code {
        404 => Manifest::Absent,
        200 => parse_manifest(body),
        other => Manifest::Unreadable(format!("code HTTP {other}")),
    }
}

fn parse_manifest(body: &[u8]) -> Manifest {
    let unreadable = |cause: &str| Manifest::Unreadable(cause.to_owned());
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return unreadable("la réponse n'est pas du JSON");
    };
    let Some(client) = value["version"].as_str().filter(|v| triple_of(v).is_some()) else {
        return unreadable("« version » absent ou qui n'est pas X.Y.Z");
    };
    let mut newest = client;
    match &value["agent"]["version"] {
        Value::Null => {}
        Value::String(agent) if triple_of(agent).is_some() => {
            if triple_of(agent) > triple_of(newest) {
                newest = agent;
            }
        }
        _ => return unreadable("« agent.version » n'est pas X.Y.Z"),
    }
    Manifest::Version(newest.to_owned())
}

/// La release GitHub `vX.Y.Z` existe-t-elle ? `GET /repos/{dépôt}/releases/tags/vX.Y.Z` : 200 + le
/// `tag_name` demandé = oui, 404 = non, tout le reste = erreur. Le jeton (s'il y en a un) passe par
/// l'entrée standard de `curl` (`--config -`), jamais par un argument ; il n'apparaît dans aucun message.
pub fn lookup_release(host: &dyn Host, version: &str, token: &str) -> Result<bool, String> {
    check_version(version)?;
    let tag = format!("v{version}");
    let url = format!("https://api.github.com/repos/{REPOSITORY}/releases/tags/{tag}");
    let mut extra = vec![
        "--header",
        "Accept: application/vnd.github+json",
        "--header",
        "X-GitHub-Api-Version: 2022-11-28",
        "--user-agent",
        "hearth-xtask",
    ];
    let config;
    let mut call_input: Option<Vec<u8>> = None;
    if !token.is_empty() {
        if !token
            .bytes()
            .all(|b| b.is_ascii_graphic() && b != b'"' && b != b'\\')
        {
            return Err("le jeton GitHub (GH_TOKEN) contient un caractère inutilisable".to_owned());
        }
        config = format!("header = \"Authorization: Bearer {token}\"\n");
        call_input = Some(config.clone().into_bytes());
        extra.extend(["--config", "-"]);
    }
    let args = curl_args(&extra, &url);
    let mut call = Call::new("curl", &args);
    if let Some(input) = call_input.as_deref() {
        call = call.input(input);
    }
    let output = host.run(&call)?;
    let (code, body) = http_reply(&output).map_err(|error| format!("release {tag} : {error}"))?;
    match code {
        404 => Ok(false),
        200 => {
            let value: Value = serde_json::from_slice(body)
                .map_err(|_| format!("release {tag} : réponse illisible"))?;
            if value["tag_name"] == tag.as_str() {
                Ok(true)
            } else {
                Err(format!(
                    "release {tag} : la réponse n'est pas celle de cette étiquette"
                ))
            }
        }
        other => Err(format!("release {tag} : code HTTP {other}")),
    }
}

fn git_call(host: &dyn Host, root: &Path, args: &[&str]) -> Result<CmdOutput, String> {
    let mut argv = vec!["-C".to_owned(), root.to_string_lossy().into_owned()];
    argv.extend(args.iter().map(|arg| (*arg).to_owned()));
    let env = [("GIT_OPTIONAL_LOCKS", "0")];
    host.run(&Call::new("git", &argv).env(&env))
}

/// Le commit du dépôt extrait (`git rev-parse HEAD`).
pub fn head_commit(host: &dyn Host, root: &Path) -> Result<String, String> {
    let output = git_call(host, root, &["rev-parse", "HEAD"])?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if output.success && !text.is_empty() {
        Ok(text)
    } else {
        Err(format!("git rev-parse HEAD : {}", short(&output.stderr)))
    }
}

/// L'étiquette `vX.Y.Z` existe-t-elle sur `origin` ? `git ls-remote --exit-code --tags` : 0 = oui,
/// 2 = non, tout autre code = erreur (jamais « absente par défaut »).
pub fn lookup_tag(host: &dyn Host, root: &Path, version: &str) -> Result<bool, String> {
    check_version(version)?;
    let reference = format!("refs/tags/v{version}");
    let output = git_call(
        host,
        root,
        &["ls-remote", "--exit-code", "--tags", "origin", &reference],
    )?;
    match output.code {
        Some(0) => Ok(true),
        Some(2) => Ok(false),
        other => Err(format!(
            "git ls-remote : code {other:?} {}",
            short(&output.stderr)
        )),
    }
}

// --- orchestration ------------------------------------------------------------------------------

/// Ce que le flux transmet aux jobs suivants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub version: String,
    /// Le commit lancé ; absent avec `--versions-only`.
    pub commit: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Inputs {
    pub git_ref: String,
    pub sha: String,
    pub notes: String,
    pub token: String,
    pub versions_only: bool,
}

/// Les contrôles dans l'ordre de la conception 7.2. Le premier refus rend son message et arrête : les
/// lectures réseau ne sont lancées qu'une fois la branche, les numéros, les notes, la clé et `HEAD`
/// acceptés.
pub fn preflight(
    host: &dyn Host,
    root: &Path,
    read: &dyn Fn(&str) -> Option<String>,
    inputs: &Inputs,
) -> Result<Outcome, String> {
    if inputs.versions_only {
        let version = check_versions(&version_files(read))?;
        check_keys(read)?;
        return Ok(Outcome {
            version,
            commit: None,
        });
    }
    check_ref(&inputs.git_ref)?;
    let version = check_versions(&version_files(read))?;
    check_notes(&inputs.notes)?;
    check_keys(read)?;
    check_head(&head_commit(host, root)?, &inputs.sha)?;
    let manifest = read_public_manifest(host);
    if matches!(manifest, Manifest::Unreadable(_)) {
        published_state(&version, &manifest, false, false)?;
    }
    let tag = lookup_tag(host, root, &version).map_err(|cause| {
        format!(
            "L'étiquette v{version} n'a pas pu être cherchée ({}) : publication arrêtée.",
            short(&cause)
        )
    })?;
    let release = lookup_release(host, &version, &inputs.token).map_err(|cause| {
        format!(
            "La release GitHub v{version} n'a pas pu être cherchée ({}) : publication arrêtée.",
            short(&cause)
        )
    })?;
    published_state(&version, &manifest, tag, release)?;
    Ok(Outcome {
        version,
        commit: Some(inputs.sha.to_ascii_lowercase()),
    })
}

/// `cargo xtask release-preflight [--versions-only]`.
pub fn run(args: &[String]) -> Result<(), String> {
    let versions_only = match args {
        [] => false,
        [flag] if flag == "--versions-only" => true,
        _ => return Err("usage : release-preflight [--versions-only]".to_owned()),
    };
    let env = |name: &str| std::env::var(name).unwrap_or_default();
    let inputs = Inputs {
        git_ref: env("HEARTH_REF"),
        sha: env("HEARTH_SHA"),
        notes: env("HEARTH_NOTES"),
        token: env("GH_TOKEN"),
        versions_only,
    };
    let root = crate::client_release::root()?;
    let read = |path: &str| std::fs::read_to_string(root.join(path)).ok();
    match preflight(&System::new(), &root, &read, &inputs) {
        Ok(outcome) => {
            ci::output("version", &outcome.version)?;
            if let Some(commit) = &outcome.commit {
                ci::output("commit", commit)?;
            }
            let said = match &outcome.commit {
                Some(commit) => format!(
                    "Contrôles préalables réussis : version {}, commit {commit}.",
                    outcome.version
                ),
                None => format!(
                    "Numéros de version concordants : {}, clés publiques de production.",
                    outcome.version
                ),
            };
            if ci::is_github() {
                ci::notice(&said);
            } else {
                println!("{said}");
            }
            Ok(())
        }
        // Annoncé en annotation : `main` n'a pas à le répéter (message vide = rien à afficher).
        Err(message) if ci::fail(&message) => Err(String::new()),
        Err(message) => Err(message),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::time::Duration;

    use super::*;

    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

    // --- un faux monde ----------------------------------------------------------------------

    /// Réponses : manifeste, GitHub, git. Chaque appel est enregistré (programme + arguments), les
    /// entrées standard à part.
    struct Fake {
        manifest: (Option<i32>, String),
        release: (Option<i32>, String),
        ls_remote: Option<i32>,
        head: String,
        calls: RefCell<Vec<String>>,
        inputs: RefCell<Vec<String>>,
    }

    fn reply(code: u16, body: &str) -> (Option<i32>, String) {
        (Some(0), format!("{body}\n{code}"))
    }

    impl Fake {
        fn new() -> Self {
            Self {
                manifest: reply(404, ""),
                release: reply(404, ""),
                ls_remote: Some(2),
                head: SHA.to_owned(),
                calls: RefCell::new(Vec::new()),
                inputs: RefCell::new(Vec::new()),
            }
        }

        fn names(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }

        fn out(code: Option<i32>, stdout: &str) -> CmdOutput {
            CmdOutput {
                success: code == Some(0),
                code,
                stdout: stdout.as_bytes().to_vec(),
                stderr: if code == Some(0) {
                    String::new()
                } else {
                    "échec".to_owned()
                },
            }
        }
    }

    impl Host for Fake {
        fn run(&self, call: &Call) -> Result<CmdOutput, String> {
            self.calls
                .borrow_mut()
                .push(format!("{} {}", call.program, call.args.join(" ")));
            if let Some(input) = call.input {
                self.inputs
                    .borrow_mut()
                    .push(String::from_utf8_lossy(input).into_owned());
            }
            let last = call.args.last().cloned().unwrap_or_default();
            match call.program {
                "curl" if last.ends_with("/api/releases/latest") => {
                    Ok(Self::out(self.manifest.0, &self.manifest.1))
                }
                "curl" if last.starts_with("https://api.github.com/") => {
                    Ok(Self::out(self.release.0, &self.release.1))
                }
                "git" if call.args.iter().any(|arg| arg == "rev-parse") => {
                    Ok(Self::out(Some(0), &format!("{}\n", self.head)))
                }
                "git" if call.args.iter().any(|arg| arg == "ls-remote") => {
                    Ok(Self::out(self.ls_remote, ""))
                }
                other => Err(format!("appel inattendu : {other} {last}")),
            }
        }
        fn elapsed(&self) -> Duration {
            Duration::ZERO
        }
        fn sleep(&self, _: Duration) {}
    }

    // --- un dépôt de fichiers ---------------------------------------------------------------

    const PROD_KEY: &str = "untrusted comment: minisign public key: 6FAC17371631EAA5\nRWSl6jEW\n";

    fn lock(version: &str) -> String {
        LOCKED_PACKAGES
            .iter()
            .map(|name| format!("[[package]]\nname = \"{name}\"\nversion = \"{version}\"\n"))
            .chain(["[[package]]\nname = \"serde\"\nversion = \"1.0.0\"\n".to_owned()])
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Un dépôt cohérent à `version`, modifiable fichier par fichier.
    #[derive(Clone)]
    struct Repo {
        files: std::collections::BTreeMap<String, String>,
    }

    impl Repo {
        fn at(version: &str) -> Self {
            let mut files = std::collections::BTreeMap::new();
            let mut put = |path: &str, text: String| {
                files.insert(path.to_owned(), text);
            };
            put(
                CARGO_TOML,
                format!(
                    "[workspace]\nmembers = []\n\n[workspace.package]\nversion = \"{version}\"\nedition = \"2024\"\n\n[workspace.dependencies]\nserde = {{ version = \"9.9.9\" }}\n"
                ),
            );
            put(CARGO_LOCK, lock(version));
            put(
                PACKAGE_JSON,
                format!("{{\"name\":\"x\",\"version\":\"{version}\"}}"),
            );
            put(
                PACKAGE_LOCK,
                format!(
                    "{{\"name\":\"x\",\"version\":\"{version}\",\"packages\":{{\"\":{{\"name\":\"x\",\"version\":\"{version}\"}},\"node_modules/y\":{{\"version\":\"9.9.9\"}}}}}}"
                ),
            );
            put(TAURI_CONF, "{\"productName\":\"Hearth\"}".to_owned());
            put(CLIENT_KEY, PROD_KEY.to_owned());
            put(AGENT_KEY, PROD_KEY.to_owned());
            Self { files }
        }

        fn with(mut self, path: &str, text: &str) -> Self {
            self.files.insert(path.to_owned(), text.to_owned());
            self
        }

        fn without(mut self, path: &str) -> Self {
            self.files.remove(path);
            self
        }

        fn read(&self) -> impl Fn(&str) -> Option<String> + '_ {
            |path| self.files.get(path).cloned()
        }

        fn entries(&self) -> Vec<VersionEntry> {
            version_files(&self.read())
        }
    }

    fn full_inputs() -> Inputs {
        Inputs {
            git_ref: "refs/heads/main".to_owned(),
            sha: SHA.to_owned(),
            notes: "Corrections.".to_owned(),
            token: "ghs_abc123".to_owned(),
            versions_only: false,
        }
    }

    fn go(fake: &Fake, repo: &Repo, inputs: &Inputs) -> Result<Outcome, String> {
        preflight(fake, Path::new("."), &repo.read(), inputs)
    }

    // --- 1. la branche ----------------------------------------------------------------------

    #[test]
    fn only_the_head_of_main_is_published() {
        assert!(check_ref("refs/heads/main").is_ok());
        for bad in [
            "refs/heads/feat/HRT-172",
            "refs/heads/main2",
            "refs/heads/Main",
            "refs/tags/v0.1.0",
            "refs/pull/9/merge",
            "main",
            "",
        ] {
            assert_eq!(
                check_ref(bad).unwrap_err(),
                "La publication ne part que de main.",
                "{bad}"
            );
        }
    }

    // --- 2. les numéros ---------------------------------------------------------------------

    #[test]
    fn a_coherent_repository_gives_its_version() {
        assert_eq!(
            check_versions(&Repo::at("0.3.1").entries()).unwrap(),
            "0.3.1"
        );
    }

    #[test]
    fn the_five_locked_packages_and_both_lock_fields_are_read() {
        let entries = Repo::at("0.3.1").entries();
        let labels: Vec<&str> = entries.iter().map(|entry| entry.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                CARGO_TOML,
                CARGO_LOCK,
                PACKAGE_JSON,
                PACKAGE_LOCK,
                TAURI_CONF
            ]
        );
        // Un paquet étranger ou une dépendance à `version = …` ne sont pas pris pour le numéro.
        assert!(
            entries[..4]
                .iter()
                .all(|e| e.found.as_deref() == Some("0.3.1"))
        );
        assert_eq!(entries[4].found, None);
    }

    #[test]
    fn each_file_that_disagrees_is_refused_and_named() {
        let base = Repo::at("0.3.1");
        let cases: Vec<(&str, Repo)> = vec![
            (
                "Cargo.lock (xtask) → 0.3.0",
                base.clone().with(
                    CARGO_LOCK,
                    &lock("0.3.1").replace(
                        "name = \"xtask\"\nversion = \"0.3.1\"",
                        "name = \"xtask\"\nversion = \"0.3.0\"",
                    ),
                ),
            ),
            (
                "apps/desktop/package.json → 0.3.0",
                base.clone().with(PACKAGE_JSON, "{\"version\":\"0.3.0\"}"),
            ),
            (
                "apps/desktop/package-lock.json (version) → 0.3.0",
                base.clone().with(
                    PACKAGE_LOCK,
                    "{\"version\":\"0.3.0\",\"packages\":{\"\":{\"version\":\"0.3.1\"}}}",
                ),
            ),
            (
                "apps/desktop/package-lock.json (packages[\"\"]) → 0.3.0",
                base.clone().with(
                    PACKAGE_LOCK,
                    "{\"version\":\"0.3.1\",\"packages\":{\"\":{\"version\":\"0.3.0\"}}}",
                ),
            ),
            (
                "Cargo.toml → 0.3.0",
                // La source change : tout le reste (resté à 0.3.1) est en désaccord avec elle.
                base.clone()
                    .with(CARGO_TOML, "[workspace.package]\nversion = \"0.3.0\"\n"),
            ),
        ];
        for (expected, repo) in cases {
            let error = check_versions(&repo.entries()).unwrap_err();
            assert!(
                error.starts_with("Les numéros de version du dépôt ne concordent pas : ")
                    && error.ends_with('.')
                    && error.contains(expected),
                "{expected} : {error}"
            );
        }
    }

    #[test]
    fn a_version_in_tauri_conf_is_refused_even_when_it_is_the_right_one() {
        let repo = Repo::at("0.3.1").with(TAURI_CONF, "{\"version\":\"0.3.1\"}");
        let error = check_versions(&repo.entries()).unwrap_err();
        assert!(
            error.contains("apps/desktop/src-tauri/tauri.conf.json → 0.3.1"),
            "{error}"
        );
    }

    #[test]
    fn a_missing_or_garbled_file_is_a_disagreement_not_a_pass() {
        let base = Repo::at("0.3.1");
        for repo in [
            base.clone().without(CARGO_LOCK),
            base.clone().without(PACKAGE_JSON),
            base.clone().without(PACKAGE_LOCK),
            base.clone().with(PACKAGE_JSON, "pas du json"),
            base.clone().with(PACKAGE_LOCK, "{}"),
            base.clone().without(CARGO_TOML),
            base.clone()
                .with(CARGO_TOML, "[package]\nversion = \"0.3.1\"\n"),
            base.clone().with(CARGO_LOCK, "rien"),
        ] {
            let error = check_versions(&repo.entries()).unwrap_err();
            assert!(error.contains("→ aucun numéro"), "{error}");
        }
    }

    #[test]
    fn the_message_lists_every_file_in_the_order_of_the_conception() {
        let repo = Repo::at("0.1.1").with(PACKAGE_JSON, "{\"version\":\"0.1.0\"}");
        assert_eq!(
            check_versions(&repo.entries()).unwrap_err(),
            "Les numéros de version du dépôt ne concordent pas : Cargo.toml → 0.1.1, Cargo.lock → 0.1.1, apps/desktop/package.json → 0.1.0, apps/desktop/package-lock.json → 0.1.1, apps/desktop/src-tauri/tauri.conf.json → aucun numéro."
        );
    }

    #[test]
    fn a_version_that_is_not_strict_is_refused_even_when_every_file_agrees() {
        for bad in ["0.1", "0.1.0-beta.1", "v0.1.0", "01.2.0", "1.2.3.4"] {
            assert!(
                check_versions(&Repo::at(bad).entries())
                    .unwrap_err()
                    .contains("attendu X.Y.Z"),
                "{bad}"
            );
        }
    }

    #[test]
    fn this_repository_passes_the_versions_only_checks() {
        // Le dépôt lui-même : la PR qui monte le numéro d'un fichier sans les autres échoue ici, avant la CI.
        let root = crate::client_release::root().unwrap();
        let read = |path: &str| std::fs::read_to_string(root.join(path)).ok();
        let version = check_versions(&version_files(&read)).unwrap();
        check_keys(&read).unwrap();
        assert!(check_version(&version).is_ok());
    }

    // --- 3. les notes -----------------------------------------------------------------------

    #[test]
    fn empty_or_blank_notes_are_refused() {
        for blank in ["", "   ", "\n\t \r\n", "\u{a0}\u{3000}"] {
            assert_eq!(
                check_notes(blank).unwrap_err(),
                "Les notes de version sont requises.",
                "{blank:?}"
            );
        }
        assert!(check_notes("  Corrections.\n").is_ok());
    }

    #[test]
    fn notes_are_counted_in_characters_after_trimming() {
        assert!(
            check_notes(&"é".repeat(NOTES_MAX)).is_ok(),
            "2000 caractères de 2 octets"
        );
        assert!(check_notes(&format!("  {}  ", "a".repeat(NOTES_MAX))).is_ok());
        assert_eq!(
            check_notes(&"é".repeat(NOTES_MAX + 1)).unwrap_err(),
            "Les notes de version dépassent 2000 caractères."
        );
    }

    // --- 4. la clé de développement ---------------------------------------------------------

    #[test]
    fn a_development_key_is_refused_for_the_client_and_for_the_agent() {
        let dev = "untrusted comment: Hearth DEV public key. No secret key exists.\nRWQabc\n";
        for path in [CLIENT_KEY, AGENT_KEY] {
            let repo = Repo::at("0.3.1").with(path, dev);
            let error = check_keys(&repo.read()).unwrap_err();
            assert!(
                error.contains("DÉVELOPPEMENT") && error.contains("update-key.pub"),
                "{error}"
            );
        }
        assert!(check_keys(&Repo::at("0.3.1").read()).is_ok());
        for path in [CLIENT_KEY, AGENT_KEY] {
            let error = check_keys(&Repo::at("0.3.1").without(path).read()).unwrap_err();
            assert!(
                error.contains(path) && error.contains("illisible"),
                "{error}"
            );
            let error =
                check_keys(&Repo::at("0.3.1").with(path, "n'importe quoi").read()).unwrap_err();
            assert!(error.contains("clé publique minisign"), "{error}");
        }
    }

    // --- 5. HEAD ----------------------------------------------------------------------------

    #[test]
    fn head_must_be_the_launched_commit() {
        assert!(check_head(SHA, SHA).is_ok());
        assert!(check_head(&SHA.to_uppercase(), SHA).is_ok());
        let other = "f".repeat(40);
        let error = check_head(&other, SHA).unwrap_err();
        assert!(
            error.contains("n'est pas le commit lancé") && error.contains("ffffffffffff"),
            "{error}"
        );
        for bad in ["", "abc", "g".repeat(40).as_str()] {
            assert!(
                check_head(SHA, bad).unwrap_err().contains("HEARTH_SHA"),
                "{bad}"
            );
        }
    }

    // --- 6. l'état publié : la table 9.2, une ligne par test --------------------------------

    fn version(text: &str) -> Manifest {
        Manifest::Version(text.to_owned())
    }

    #[test]
    fn nothing_published_and_no_tag_lets_the_release_through() {
        assert!(published_state("0.3.1", &Manifest::Absent, false, false).is_ok());
        assert!(published_state("0.3.1", &version("0.3.0"), false, false).is_ok());
        assert!(published_state("0.10.0", &version("0.9.9"), false, false).is_ok());
    }

    #[test]
    fn an_unreadable_manifest_stops_everything() {
        let error = published_state(
            "0.3.1",
            &Manifest::Unreadable("code HTTP 403".to_owned()),
            false,
            false,
        )
        .unwrap_err();
        assert_eq!(
            error,
            "Le manifeste public n'a pas pu être lu (code HTTP 403) : publication arrêtée."
        );
    }

    #[test]
    fn a_manifest_newer_than_the_repository_is_refused() {
        assert_eq!(
            published_state("0.3.1", &version("0.4.0"), false, false).unwrap_err(),
            "Le manifeste annonce déjà une version plus récente (0.4.0) que celle du dépôt (0.3.1)."
        );
        // Comparaison de nombres, pas de textes.
        assert!(published_state("0.9.0", &version("0.10.0"), false, false).is_err());
        assert!(published_state("0.10.0", &version("0.9.0"), false, false).is_ok());
    }

    #[test]
    fn an_announced_version_with_its_release_is_already_published() {
        assert_eq!(
            published_state("0.3.1", &version("0.3.1"), true, true).unwrap_err(),
            "La version 0.3.1 est déjà publiée."
        );
        // L'étiquette n'est pas nécessaire pour être « publiée » : le manifeste et la release suffisent.
        assert_eq!(
            published_state("0.3.1", &version("0.3.1"), false, true).unwrap_err(),
            "La version 0.3.1 est déjà publiée."
        );
    }

    #[test]
    fn an_announced_version_without_its_release_says_to_rerun_the_original_run() {
        for tag in [false, true] {
            assert_eq!(
                published_state("0.3.1", &version("0.3.1"), tag, false).unwrap_err(),
                "La version 0.3.1 est annoncée mais sa release GitHub manque : relancez l'étape en échec de l'exécution d'origine."
            );
        }
    }

    #[test]
    fn a_tag_or_a_release_the_manifest_does_not_announce_is_an_anomaly() {
        let orphan =
            "L'étiquette v0.3.1 existe mais le manifeste ne l'annonce pas : à examiner à la main.";
        for manifest in [Manifest::Absent, version("0.3.0")] {
            for (tag, release) in [(true, false), (false, true), (true, true)] {
                assert_eq!(
                    published_state("0.3.1", &manifest, tag, release).unwrap_err(),
                    orphan,
                    "{manifest:?} {tag} {release}"
                );
            }
        }
    }

    // --- les lectures -----------------------------------------------------------------------

    fn manifest_of(fake: Fake) -> Manifest {
        read_public_manifest(&fake)
    }

    fn with_manifest(code: Option<i32>, body: &str) -> Fake {
        Fake {
            manifest: (code, body.to_owned()),
            ..Fake::new()
        }
    }

    #[test]
    fn the_public_manifest_is_read_by_exact_http_code() {
        assert_eq!(
            manifest_of(with_manifest(Some(0), "\n404")),
            Manifest::Absent
        );
        let ok = |body: &str| manifest_of(with_manifest(Some(0), &format!("{body}\n200")));
        assert_eq!(ok("{\"version\":\"0.3.1\"}"), version("0.3.1"));
        // Le client et l'agent : la plus récente des deux.
        assert_eq!(
            ok("{\"version\":\"0.3.1\",\"agent\":{\"version\":\"0.3.0\"}}"),
            version("0.3.1")
        );
        assert_eq!(
            ok("{\"version\":\"0.3.0\",\"agent\":{\"version\":\"0.10.0\"}}"),
            version("0.10.0")
        );
    }

    #[test]
    fn anything_but_a_clean_200_or_404_is_unreadable_never_an_absence() {
        let unreadable = |fake: Fake| matches!(manifest_of(fake), Manifest::Unreadable(_));
        // Autres codes : défi Cloudflare, erreur du serveur, redirection, limite.
        for code in [301, 302, 403, 429, 500, 502, 503] {
            assert!(
                unreadable(with_manifest(
                    Some(0),
                    &format!("<html>404 not found</html>\n{code}")
                )),
                "{code}"
            );
        }
        // 200 mais pas le manifeste : page HTML d'un défi, JSON d'une autre forme, versions mal formées.
        for body in [
            "<html>Just a moment...</html>",
            "",
            "{}",
            "{\"version\":\"v0.3.1\"}",
            "{\"version\":3}",
            "{\"version\":\"0.3.1\",\"agent\":{\"version\":\"x\"}}",
            "{\"version\":\"0.3.1\",\"agent\":{\"version\":3}}",
        ] {
            assert!(
                unreadable(with_manifest(Some(0), &format!("{body}\n200"))),
                "{body}"
            );
        }
        // curl lui-même en échec (délai, DNS, TLS) ; réponse sans code.
        assert!(unreadable(with_manifest(Some(28), "")));
        assert!(unreadable(with_manifest(Some(0), "pas de code")));
        assert!(unreadable(with_manifest(Some(0), "\nabc")));
    }

    #[test]
    fn the_manifest_is_asked_over_https_without_following_a_redirect() {
        let fake = Fake::new();
        let _ = read_public_manifest(&fake);
        let call = fake.names().remove(0);
        assert!(call.starts_with("curl --disable "), "{call}");
        assert!(
            call.ends_with("https://hearth.voikyrioh.fr/api/releases/latest"),
            "{call}"
        );
        assert!(
            call.contains("--proto =https")
                && call.contains("--connect-timeout 15")
                && call.contains("--max-time 60")
        );
        assert!(
            !call.contains("--location") && !call.contains("--fail"),
            "{call}"
        );
    }

    fn release_fake(code: Option<i32>, body: &str) -> Fake {
        Fake {
            release: (code, body.to_owned()),
            ..Fake::new()
        }
    }

    #[test]
    fn the_release_is_looked_up_by_http_code_with_the_token_on_standard_input() {
        let found = release_fake(Some(0), "{\"tag_name\":\"v0.3.1\",\"assets\":[]}\n200");
        assert_eq!(lookup_release(&found, "0.3.1", "ghs_abc123"), Ok(true));
        let call = found.names().remove(0);
        assert!(
            call.ends_with("https://api.github.com/repos/Voikyrioh/hearth/releases/tags/v0.3.1"),
            "{call}"
        );
        assert!(
            call.contains("--config -") && !call.contains("ghs_abc123"),
            "le jeton hors des arguments : {call}"
        );
        assert_eq!(
            found.inputs.borrow().as_slice(),
            ["header = \"Authorization: Bearer ghs_abc123\"\n"]
        );
        assert_eq!(
            lookup_release(&release_fake(Some(0), "\n404"), "0.3.1", "ghs_abc123"),
            Ok(false)
        );
    }

    #[test]
    fn without_a_token_the_lookup_is_anonymous() {
        let fake = release_fake(Some(0), "{\"tag_name\":\"v0.3.1\"}\n200");
        assert_eq!(lookup_release(&fake, "0.3.1", ""), Ok(true));
        assert!(fake.inputs.borrow().is_empty());
        assert!(!fake.names()[0].contains("--config"));
    }

    #[test]
    fn a_release_lookup_that_is_not_a_clean_200_or_404_is_an_error() {
        let err = |code: Option<i32>, body: &str| {
            lookup_release(&release_fake(code, body), "0.3.1", "t").unwrap_err()
        };
        for code in [401, 403, 429, 500, 502] {
            assert!(
                err(Some(0), &format!("{{\"message\":\"Not Found\"}}\n{code}"))
                    .contains(&format!("code HTTP {code}"))
            );
        }
        assert!(err(Some(0), "<html>\n200").contains("illisible"));
        assert!(
            err(Some(0), "{\"tag_name\":\"v0.3.0\"}\n200").contains("pas celle de cette étiquette")
        );
        assert!(err(Some(0), "{}\n200").contains("pas celle de cette étiquette"));
        assert!(err(Some(28), "").contains("curl"));
    }

    #[test]
    fn a_token_that_could_break_the_curl_config_is_refused_without_being_echoed() {
        for bad in ["a\"b", "a\\b", "a b", "a\nb", "é"] {
            let fake = Fake::new();
            let error = lookup_release(&fake, "0.3.1", bad).unwrap_err();
            assert!(
                error.contains("GH_TOKEN") && !error.contains(bad),
                "{error}"
            );
            assert!(
                fake.names().is_empty(),
                "aucun appel avec un jeton inutilisable"
            );
        }
    }

    #[test]
    fn the_tag_is_looked_up_by_the_exit_code_of_ls_remote() {
        let tag = |code: Option<i32>| {
            let fake = Fake {
                ls_remote: code,
                ..Fake::new()
            };
            (lookup_tag(&fake, Path::new("."), "0.3.1"), fake.names())
        };
        assert_eq!(tag(Some(0)).0, Ok(true));
        let (absent, calls) = tag(Some(2));
        assert_eq!(absent, Ok(false));
        assert!(
            calls[0].ends_with("ls-remote --exit-code --tags origin refs/tags/v0.3.1"),
            "{calls:?}"
        );
        // Réseau coupé (128), signal, autre code : jamais « absente par défaut ».
        for code in [Some(128), Some(1), None] {
            assert!(tag(code).0.is_err(), "{code:?}");
        }
    }

    // --- l'orchestration --------------------------------------------------------------------

    #[test]
    fn a_valid_unpublished_version_passes_and_gives_its_version_and_commit() {
        let fake = Fake::new();
        let outcome = go(&fake, &Repo::at("0.3.1"), &full_inputs()).unwrap();
        assert_eq!(
            outcome,
            Outcome {
                version: "0.3.1".to_owned(),
                commit: Some(SHA.to_owned())
            }
        );
        let calls = fake.names();
        assert_eq!(calls.len(), 4, "{calls:?}");
        assert!(calls[0].contains("rev-parse HEAD"));
        assert!(calls[1].ends_with("/api/releases/latest"));
        assert!(calls[2].contains("ls-remote") && calls[3].contains("api.github.com"));
    }

    #[test]
    fn every_refusal_is_the_message_of_the_spec_and_no_network_is_touched_before_the_last_checks() {
        let repo = Repo::at("0.3.1");
        let refuse = |inputs: Inputs, repo: &Repo| {
            let fake = Fake::new();
            let error = go(&fake, repo, &inputs).unwrap_err();
            (error, fake.names())
        };
        let (error, calls) = refuse(
            Inputs {
                git_ref: "refs/heads/dev".to_owned(),
                ..full_inputs()
            },
            &repo,
        );
        assert_eq!(error, "La publication ne part que de main.");
        assert!(calls.is_empty());
        let (error, calls) = refuse(
            full_inputs(),
            &repo.clone().with(PACKAGE_JSON, "{\"version\":\"0.3.0\"}"),
        );
        assert!(error.starts_with("Les numéros de version du dépôt ne concordent pas : "));
        assert!(calls.is_empty());
        let (error, calls) = refuse(
            Inputs {
                notes: " \n ".to_owned(),
                ..full_inputs()
            },
            &repo,
        );
        assert_eq!(error, "Les notes de version sont requises.");
        assert!(calls.is_empty());
        let (error, calls) = refuse(
            full_inputs(),
            &repo
                .clone()
                .with(CLIENT_KEY, "untrusted comment: x DEV public key\nRWQ\n"),
        );
        assert!(error.contains("DÉVELOPPEMENT"));
        assert!(calls.is_empty());
        // HEAD n'est pas le commit lancé : un seul appel git, rien de public n'est lu.
        let (error, calls) = refuse(
            Inputs {
                sha: "e".repeat(40),
                ..full_inputs()
            },
            &repo,
        );
        assert!(error.contains("n'est pas le commit lancé"));
        assert_eq!(calls.len(), 1);
    }

    #[test]
    fn the_checks_run_in_the_order_of_the_conception() {
        // Tout est faux à la fois : la branche d'abord, puis les numéros, puis les notes.
        let broken = Repo::at("0.3.1").with(PACKAGE_JSON, "{\"version\":\"0.0.1\"}");
        let all_wrong = Inputs {
            git_ref: "refs/tags/v0.3.1".to_owned(),
            notes: String::new(),
            ..full_inputs()
        };
        let fake = Fake::new();
        assert_eq!(
            go(&fake, &broken, &all_wrong).unwrap_err(),
            "La publication ne part que de main."
        );
        let on_main = Inputs {
            git_ref: "refs/heads/main".to_owned(),
            ..all_wrong.clone()
        };
        assert!(
            go(&fake, &broken, &on_main)
                .unwrap_err()
                .starts_with("Les numéros")
        );
        assert_eq!(
            go(&fake, &Repo::at("0.3.1"), &on_main).unwrap_err(),
            "Les notes de version sont requises."
        );
    }

    #[test]
    fn the_published_state_decides_last_with_the_three_readings() {
        let repo = Repo::at("0.3.1");
        let announced = "{\"version\":\"0.3.1\",\"agent\":{\"version\":\"0.3.1\"}}\n200";
        // Déjà publiée : manifeste + release.
        let fake = Fake {
            manifest: (Some(0), announced.to_owned()),
            release: (Some(0), "{\"tag_name\":\"v0.3.1\"}\n200".to_owned()),
            ls_remote: Some(0),
            ..Fake::new()
        };
        assert_eq!(
            go(&fake, &repo, &full_inputs()).unwrap_err(),
            "La version 0.3.1 est déjà publiée."
        );
        // Étiquette orpheline : rien dans le manifeste, une étiquette sur origin.
        let fake = Fake {
            ls_remote: Some(0),
            ..Fake::new()
        };
        assert_eq!(
            go(&fake, &repo, &full_inputs()).unwrap_err(),
            "L'étiquette v0.3.1 existe mais le manifeste ne l'annonce pas : à examiner à la main."
        );
        // Release orpheline, sans étiquette vue.
        let fake = Fake {
            release: (Some(0), "{\"tag_name\":\"v0.3.1\"}\n200".to_owned()),
            ..Fake::new()
        };
        assert!(
            go(&fake, &repo, &full_inputs())
                .unwrap_err()
                .starts_with("L'étiquette v0.3.1 existe")
        );
    }

    #[test]
    fn an_unreadable_public_state_stops_before_asking_about_the_tag_and_the_release() {
        let fake = with_manifest(Some(0), "<html>Just a moment</html>\n403");
        let error = go(&fake, &Repo::at("0.3.1"), &full_inputs()).unwrap_err();
        assert_eq!(
            error,
            "Le manifeste public n'a pas pu être lu (code HTTP 403) : publication arrêtée."
        );
        assert_eq!(
            fake.names().len(),
            2,
            "git rev-parse puis le manifeste seulement"
        );
    }

    #[test]
    fn a_tag_or_release_lookup_that_fails_stops_the_publication() {
        let fake = Fake {
            ls_remote: Some(128),
            ..Fake::new()
        };
        let error = go(&fake, &Repo::at("0.3.1"), &full_inputs()).unwrap_err();
        assert!(
            error.starts_with("L'étiquette v0.3.1 n'a pas pu être cherchée")
                && error.ends_with("publication arrêtée."),
            "{error}"
        );
        let fake = release_fake(Some(0), "{}\n500");
        let error = go(&fake, &Repo::at("0.3.1"), &full_inputs()).unwrap_err();
        assert!(
            error.starts_with("La release GitHub v0.3.1 n'a pas pu être cherchée")
                && error.contains("code HTTP 500"),
            "{error}"
        );
    }

    #[test]
    fn versions_only_checks_numbers_and_keys_without_the_network_or_the_variables() {
        let fake = Fake::new();
        let inputs = Inputs {
            versions_only: true,
            ..Inputs::default()
        };
        let outcome = go(&fake, &Repo::at("0.3.1"), &inputs).unwrap();
        assert_eq!(
            outcome,
            Outcome {
                version: "0.3.1".to_owned(),
                commit: None
            }
        );
        assert!(fake.names().is_empty(), "ni git, ni curl");
        // Les numéros et la clé sont quand même contrôlés.
        assert!(
            go(
                &fake,
                &Repo::at("0.3.1").with(PACKAGE_JSON, "{\"version\":\"0.3.0\"}"),
                &inputs
            )
            .is_err()
        );
        assert!(
            go(
                &fake,
                &Repo::at("0.3.1").with(AGENT_KEY, "untrusted comment: x DEV public key\nRWQ\n"),
                &inputs
            )
            .is_err()
        );
        // Ni la branche, ni les notes, ni HEAD ne sont demandés.
        assert!(
            go(
                &fake,
                &Repo::at("0.3.1"),
                &Inputs {
                    versions_only: true,
                    git_ref: "x".to_owned(),
                    ..Inputs::default()
                }
            )
            .is_ok()
        );
    }

    #[test]
    fn the_flag_is_the_only_argument() {
        assert!(
            run(&["--autre".to_owned()])
                .unwrap_err()
                .starts_with("usage")
        );
        assert!(run(&["--versions-only".to_owned(), "x".to_owned()]).is_err());
    }
}
