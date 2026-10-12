//! `cargo xtask release-upload [--confirm]` (HRT-51, conception de la distribution 8.2) : dépose au bucket
//! privé le dossier assemblé par `release-stage`.
//!
//! - SANS `--confirm` : ESSAI À BLANC. Affiche chaque écriture prévue (clé d'objet, somme, taille),
//!   n'écrit rien et ne lance AUCUNE commande (ni `aws`, ni `curl`).
//! - AVEC `--confirm`, dans cet ordre, jamais un autre :
//!   1. les fichiers de la version (`releases/X.Y.Z/*`). AVANT toute écriture, chaque clé est cherchée :
//!      absente, elle sera écrite ; présente avec le MÊME contenu, elle est passée ; présente avec un
//!      AUTRE contenu, REFUS et arrêt sans rien avoir écrit (une version déposée ne se réécrit jamais,
//!      BR-RELEASE-001) ;
//!   2. relecture de chaque fichier par l'adresse publique `https://hearth.voikyrioh.fr/api/releases/…`
//!      (Cloudflare, Traefik, API, bucket) et comparaison des sommes, avec une patience de 5 minutes ;
//!   3. le script `releases/install.sh` ;
//!   4. le manifeste `releases/manifest.json`, EN DERNIER, seulement si toutes les relectures sont
//!      bonnes (BR-RELEASE-002).
//!
//! Le bucket est atteint par l'outil `aws` (`--endpoint-url https://s3.fr-par.scw.cloud`) ; les
//! identifiants sont lus dans l'environnement par `aws` lui-même : ce module ne les lit, ne les affiche
//! et ne les passe JAMAIS en argument. Toute exécution externe (`aws`, `curl`) et toute attente passent
//! par `Host`, injecté : les tests n'ont ni réseau ni `aws`. Arguments en liste, aucun interpréteur.
//!
//! La relecture publique passe par `curl` (livré avec Windows 10 et suivants) plutôt que par un client
//! HTTP en dépendance : `xtask` n'en a pas, et la relecture doit de toute façon traverser la chaîne
//! publique entière.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::client_release::{option, root};
use crate::release_core::{DOWNLOAD_PREFIX, check_version, workspace_version};
use crate::release_stage::*;

pub const ENDPOINT: &str = "https://s3.fr-par.scw.cloud";
pub const REGION: &str = "fr-par";
pub const BUCKET: &str = "voikyrioh-hearth-releases";
/// Patience de la relecture publique (cache Cloudflare, démarrage de l'API) : 5 minutes.
pub const PATIENCE: Duration = Duration::from_secs(300);
/// Pause entre deux tours de relecture.
pub const POLL: Duration = Duration::from_secs(10);

/// Résultat d'une commande externe.
#[derive(Debug, Clone)]
pub struct CmdOutput {
    pub success: bool,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

/// Tout ce que `release-upload` demande au monde extérieur : lancer un programme (arguments en liste),
/// lire l'horloge, attendre. Les tests le remplacent par un faux qui enregistre les appels.
pub trait Host {
    /// `Err` = le programme n'a pas pu être lancé ; un échec du programme est un `CmdOutput` non réussi.
    fn run(&self, program: &str, args: &[String]) -> Result<CmdOutput, String>;
    fn elapsed(&self) -> Duration;
    fn sleep(&self, duration: Duration);
}

/// Le vrai monde : processus du système, horloge, `thread::sleep`.
pub struct System {
    start: Instant,
}

impl System {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl Host for System {
    fn run(&self, program: &str, args: &[String]) -> Result<CmdOutput, String> {
        let output = std::process::Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    format!(
                        "« {program} » est introuvable : installe-le et mets-le dans le PATH (AWS CLI v2 pour aws ; curl est livré avec Windows 10 et suivants)"
                    )
                } else {
                    format!("{program} : {error}")
                }
            })?;
        Ok(CmdOutput {
            success: output.status.success(),
            stdout: output.stdout,
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }

    fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Fichier d'une version : immuable dès le dépôt.
    Version,
    /// `releases/install.sh` : mutable, écrit après les relectures.
    Script,
    /// `releases/manifest.json` : mutable, écrit EN DERNIER.
    Manifest,
}

/// Une écriture prévue.
#[derive(Debug, Clone)]
pub struct PlannedWrite {
    pub key: String,
    pub path: PathBuf,
    pub sum: String,
    pub size: usize,
    pub content_type: &'static str,
    pub kind: Kind,
}

/// Le type de contenu posé au dépôt, d'après le nom du fichier (même table que `contentTypeFor` de
/// `release-files.ts`, que l'API applique de toute façon à la réponse).
pub fn content_type(key: &str) -> &'static str {
    let name = key.rsplit('/').next().unwrap_or(key);
    if name == MANIFEST_NAME {
        "application/json"
    } else if name == SCRIPT_NAME {
        "text/x-shellscript; charset=utf-8"
    } else if name.ends_with(".sig")
        || name.ends_with(".minisig")
        || name.ends_with(".sha256")
        || name == SUMS_NAME
    {
        "text/plain; charset=utf-8"
    } else {
        "application/octet-stream"
    }
}

/// L'ordre des écritures : les fichiers de la version, le script, le manifeste.
pub fn plan(stage_dir: &Path, version: &str, files: &Files) -> Vec<PlannedWrite> {
    let mut keys: Vec<(String, Kind)> = release_file_names(version)
        .iter()
        .map(|name| (version_key(version, name), Kind::Version))
        .collect();
    keys.push((SCRIPT_KEY.to_owned(), Kind::Script));
    keys.push((MANIFEST_KEY.to_owned(), Kind::Manifest));
    keys.into_iter()
        .filter_map(|(key, kind)| {
            let bytes = files.get(&key)?;
            let path = key
                .split('/')
                .fold(stage_dir.to_path_buf(), |path, part| path.join(part));
            Some(PlannedWrite {
                sum: sha256_hex(bytes),
                size: bytes.len(),
                content_type: content_type(&key),
                path,
                key,
                kind,
            })
        })
        .collect()
}

fn aws_common() -> Vec<String> {
    ["--endpoint-url", ENDPOINT, "--region", REGION]
        .iter()
        .map(|arg| (*arg).to_owned())
        .collect()
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|arg| (*arg).to_owned()).collect()
}

/// État d'une clé dans le bucket.
#[derive(Debug, PartialEq, Eq)]
enum Remote {
    Absent,
    Same,
    Different,
}

/// `aws s3api head-object` puis, si la clé existe, `aws s3 cp s3://… -` : le contenu est RELU et haché
/// (l'ETag d'un dépôt en plusieurs parties n'est pas une somme du contenu).
fn remote_state(host: &dyn Host, write: &PlannedWrite) -> Result<Remote, String> {
    let mut head = args(&[
        "s3api",
        "head-object",
        "--bucket",
        BUCKET,
        "--key",
        &write.key,
    ]);
    head.extend(aws_common());
    let found = host.run("aws", &head)?;
    if !found.success {
        let reason = found.stderr.to_lowercase();
        if reason.contains("404") || reason.contains("not found") || reason.contains("nosuchkey") {
            return Ok(Remote::Absent);
        }
        return Err(format!(
            "impossible de savoir si {} existe déjà ({}). Rien n'a été écrit. Vérifie les variables AWS_ACCESS_KEY_ID et AWS_SECRET_ACCESS_KEY de l'environnement (clé de publication de Voiky) et le droit de lecture du bucket",
            write.key,
            short(&found.stderr)
        ));
    }
    let mut get = args(&["s3", "cp", &format!("s3://{BUCKET}/{}", write.key), "-"]);
    get.extend(aws_common());
    get.push("--only-show-errors".to_owned());
    let read = host.run("aws", &get)?;
    if !read.success {
        return Err(format!(
            "{} existe mais n'a pas pu être relue ({}). Rien n'a été écrit",
            write.key,
            short(&read.stderr)
        ));
    }
    Ok(if sha256_hex(&read.stdout) == write.sum {
        Remote::Same
    } else {
        Remote::Different
    })
}

fn short(text: &str) -> String {
    let text: String = text.chars().take(300).collect();
    if text.is_empty() {
        "aucun message".to_owned()
    } else {
        text
    }
}

fn put(host: &dyn Host, write: &PlannedWrite) -> Result<(), String> {
    let mut put = args(&["s3", "cp"]);
    put.push(write.path.to_string_lossy().into_owned());
    put.push(format!("s3://{BUCKET}/{}", write.key));
    put.extend(aws_common());
    put.extend(args(&[
        "--content-type",
        write.content_type,
        "--only-show-errors",
    ]));
    let done = host.run("aws", &put)?;
    if done.success {
        Ok(())
    } else {
        Err(format!(
            "l'écriture de {} a échoué ({}). Relance la même commande : ce qui est déjà déposé à l'identique est passé",
            write.key,
            short(&done.stderr)
        ))
    }
}

/// Relit chaque fichier de la version par l'adresse publique jusqu'à ce que toutes les sommes soient
/// bonnes, ou que la patience soit épuisée.
fn read_back(
    host: &dyn Host,
    version: &str,
    writes: &[PlannedWrite],
    say: &mut dyn FnMut(String),
) -> Result<(), String> {
    let start = host.elapsed();
    let mut pending: Vec<(&PlannedWrite, String)> = writes
        .iter()
        .filter(|write| write.kind == Kind::Version)
        .map(|write| (write, String::from("pas encore relu")))
        .collect();
    loop {
        for (write, reason) in &mut pending {
            let name = write.key.rsplit('/').next().unwrap_or(&write.key);
            let url = format!("{DOWNLOAD_PREFIX}{version}/{name}");
            let mut curl = args(&[
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--max-redirs",
                "5",
                "--proto",
                "=https",
                "--proto-redir",
                "=https",
                "--tlsv1.2",
            ]);
            curl.push(url);
            *reason = match host.run("curl", &curl) {
                Ok(read) if read.success && sha256_hex(&read.stdout) == write.sum => String::new(),
                Ok(read) if read.success => {
                    "la somme relue n'est pas celle du fichier déposé".to_owned()
                }
                Ok(read) => short(&read.stderr),
                Err(error) => return Err(error),
            };
        }
        let before = pending.len();
        pending.retain(|(_, reason)| !reason.is_empty());
        if pending.is_empty() {
            return Ok(());
        }
        if pending.len() != before {
            say(format!("relu : {} fichier(s) restant(s)", pending.len()));
        }
        if host.elapsed().saturating_sub(start) >= PATIENCE {
            let list = pending
                .iter()
                .map(|(write, reason)| format!("{} ({reason})", write.key))
                .collect::<Vec<_>>()
                .join(" ; ");
            return Err(format!(
                "relecture publique impossible en {} minutes : {list}. Le manifeste n'est PAS écrit, rien n'est annoncé. Vérifie hearth.voikyrioh.fr/api (Cloudflare, Traefik, API) puis relance la même commande : les fichiers déjà déposés sont passés",
                PATIENCE.as_secs() / 60
            ));
        }
        say(format!(
            "relecture publique : {} fichier(s) pas encore bon(s), nouvel essai dans {} s",
            pending.len(),
            POLL.as_secs()
        ));
        host.sleep(POLL);
    }
}

/// Dépose (ou, sans `confirm`, décrit) la version. `say` reçoit chaque ligne de compte rendu.
pub fn publish(
    host: &dyn Host,
    stage_dir: &Path,
    version: &str,
    files: &Files,
    confirm: bool,
    say: &mut dyn FnMut(String),
) -> Result<(), String> {
    let writes = plan(stage_dir, version, files);
    if !confirm {
        say(format!(
            "ESSAI À BLANC : rien n'est écrit et aucune commande n'est lancée (ni aws, ni curl). Ajoute --confirm pour déposer. Bucket s3://{BUCKET} ({ENDPOINT})"
        ));
        for (at, write) in writes.iter().enumerate() {
            let note = match write.kind {
                Kind::Version => {
                    "refus si la clé existe avec un autre contenu, passée si identique"
                }
                Kind::Script => "après la relecture publique des fichiers de la version",
                Kind::Manifest => "EN DERNIER, seulement si toutes les relectures sont bonnes",
            };
            say(format!(
                "{:>2}. écrire {}  sha256={}  {} octets  [{}]  ({note})",
                at + 1,
                write.key,
                write.sum,
                write.size,
                write.content_type
            ));
            if write.kind == Kind::Version
                && writes
                    .get(at + 1)
                    .is_none_or(|next| next.kind != Kind::Version)
            {
                say(format!(
                    "    puis relire les {} fichiers de la version sous {DOWNLOAD_PREFIX}{version}/ (jusqu'à {} min) et comparer les sommes",
                    writes.iter().filter(|w| w.kind == Kind::Version).count(),
                    PATIENCE.as_secs() / 60
                ));
            }
        }
        return Ok(());
    }

    // 1a. Aucune écriture avant d'avoir vu l'état de TOUTES les clés de la version.
    let mut state = Vec::new();
    let mut refused = Vec::new();
    for write in writes.iter().filter(|write| write.kind == Kind::Version) {
        let remote = remote_state(host, write)?;
        if remote == Remote::Different {
            refused.push(write.key.clone());
        }
        state.push((write, remote));
    }
    if !refused.is_empty() {
        return Err(format!(
            "REFUS : {} existe déjà dans le bucket avec un autre contenu. Une version déposée ne se réécrit jamais. Aucune écriture n'a eu lieu. Si le numéro {version} a déjà été signé puis déposé en partie, il est brûlé : passe à la version suivante (Cargo.toml et apps/desktop/package.json), resigne, relance release-stage",
            refused.join(", ")
        ));
    }
    // 1b. Écriture de ce qui manque.
    for (write, remote) in &state {
        if *remote == Remote::Same {
            say(format!("déjà présent, identique : {}", write.key));
        } else {
            put(host, write)?;
            say(format!("écrit : {}", write.key));
        }
    }
    // 2. Relecture publique de tous les fichiers de la version.
    read_back(host, version, &writes, say)?;
    say("relecture publique : toutes les sommes sont bonnes".to_owned());
    // 3 et 4. Le script, puis le manifeste EN DERNIER.
    for kind in [Kind::Script, Kind::Manifest] {
        for write in writes.iter().filter(|write| write.kind == kind) {
            put(host, write)?;
            say(format!("écrit : {}", write.key));
        }
    }
    say(format!(
        "version {version} annoncée. Reste à faire à la main : relire {DOWNLOAD_PREFIX}latest (cache de 5 minutes au plus), puis poser l'étiquette v{version} sur le commit construit (hors heures employeur)"
    ));
    Ok(())
}

/// `cargo xtask release-upload [--confirm] [--stage-dir D]`.
pub fn run_upload(args: &[String]) -> Result<(), String> {
    check_flags(args, &["--stage-dir"], &["--confirm"])?;
    let confirm = args.iter().any(|arg| arg == "--confirm");
    let root = root()?;
    let stage_dir = option(args, "--stage-dir")
        .map_or_else(|| root.join(STAGE_DIR), |dir| Path::new(&dir).to_path_buf());
    let files = read_stage_dir(&stage_dir)?;
    let read = |path: &str| {
        std::fs::read_to_string(root.join(path)).map_err(|error| format!("{path} : {error}"))
    };
    let cargo = read("Cargo.toml")?;
    let version = verify_stage(
        &files,
        &read("apps/desktop/src-tauri/update-key.pub")?,
        &read("crates/hearth-agent/update-key.pub")?,
        &cargo,
    )
    .map_err(|error| format!("le dossier assemblé ne passe plus la vérification : {error}. Relance cargo xtask release-stage"))?;
    check_version(&version)?;
    if workspace_version(&cargo).as_deref() != Some(version.as_str()) {
        return Err("version du dossier assemblé différente de celle du dépôt".to_owned());
    }
    publish(
        &System::new(),
        &stage_dir,
        &version,
        &files,
        confirm,
        &mut |line| println!("{line}"),
    )
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::collections::BTreeMap;

    use super::*;
    use crate::release_core::AGENT_BINARY;
    use crate::release_stage::fixtures::*;

    const NOT_FOUND: &str =
        "An error occurred (404) when calling the HeadObject operation: Not Found";

    /// Faux monde : un bucket en mémoire, l'adresse publique qui le relit, une horloge qui n'avance que
    /// par `sleep`. Enregistre chaque appel.
    struct Fake {
        calls: RefCell<Vec<(String, Vec<String>)>>,
        bucket: RefCell<BTreeMap<String, Vec<u8>>>,
        /// Chemin virtuel du dossier assemblé -> contenu (aucun disque).
        local: BTreeMap<String, Vec<u8>>,
        public: Cell<Public>,
        head_error: RefCell<Option<String>>,
        clock: Cell<Duration>,
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Public {
        Ok,
        Down,
        Corrupt,
    }

    impl Fake {
        fn new(stage_dir: &Path, files: &Files) -> Self {
            let local = plan(stage_dir, VERSION, files)
                .into_iter()
                .map(|write| {
                    (
                        write.path.to_string_lossy().into_owned(),
                        files[&write.key].clone(),
                    )
                })
                .collect();
            Self {
                calls: RefCell::new(Vec::new()),
                bucket: RefCell::new(BTreeMap::new()),
                local,
                public: Cell::new(Public::Ok),
                head_error: RefCell::new(None),
                clock: Cell::new(Duration::ZERO),
            }
        }

        fn writes(&self) -> Vec<String> {
            self.calls
                .borrow()
                .iter()
                .filter(|(program, args)| {
                    program == "aws"
                        && args.first().map(String::as_str) == Some("s3")
                        && args.get(1).map(String::as_str) == Some("cp")
                        && args.get(3).is_some_and(|dest| dest.starts_with("s3://"))
                })
                .map(|(_, args)| {
                    args[3]
                        .trim_start_matches(&format!("s3://{BUCKET}/"))
                        .to_owned()
                })
                .collect()
        }

        fn log(&self) -> Vec<String> {
            self.calls
                .borrow()
                .iter()
                .map(|(program, args)| {
                    if program == "curl" {
                        format!("curl {}", args.last().cloned().unwrap_or_default())
                    } else if args.get(1).map(String::as_str) == Some("head-object") {
                        format!("head {}", args[5])
                    } else if args.get(3).is_some_and(|d| d.starts_with("s3://")) {
                        format!(
                            "put {}",
                            args[3].trim_start_matches(&format!("s3://{BUCKET}/"))
                        )
                    } else {
                        format!(
                            "get {}",
                            args[2].trim_start_matches(&format!("s3://{BUCKET}/"))
                        )
                    }
                })
                .collect()
        }
    }

    fn ok(stdout: Vec<u8>) -> Result<CmdOutput, String> {
        Ok(CmdOutput {
            success: true,
            stdout,
            stderr: String::new(),
        })
    }

    fn fail(stderr: &str) -> Result<CmdOutput, String> {
        Ok(CmdOutput {
            success: false,
            stdout: Vec::new(),
            stderr: stderr.to_owned(),
        })
    }

    impl Host for Fake {
        fn run(&self, program: &str, args: &[String]) -> Result<CmdOutput, String> {
            self.calls
                .borrow_mut()
                .push((program.to_owned(), args.to_vec()));
            if program == "curl" {
                let url = args.last().cloned().unwrap_or_default();
                let key = url.replace("https://hearth.voikyrioh.fr/api/", "");
                return match (self.public.get(), self.bucket.borrow().get(&key)) {
                    (Public::Ok, Some(bytes)) => ok(bytes.clone()),
                    (Public::Corrupt, Some(_)) => ok(b"autre chose".to_vec()),
                    _ => fail("curl: (22) The requested URL returned error: 404"),
                };
            }
            assert_eq!(program, "aws");
            assert!(
                args.windows(2)
                    .any(|pair| pair[0] == "--endpoint-url" && pair[1] == ENDPOINT),
                "l'endpoint Scaleway est toujours donné : {args:?}"
            );
            match (args[0].as_str(), args[1].as_str()) {
                ("s3api", "head-object") => {
                    if let Some(error) = self.head_error.borrow().clone() {
                        return fail(&error);
                    }
                    if self.bucket.borrow().contains_key(&args[5]) {
                        ok(b"{}".to_vec())
                    } else {
                        fail(NOT_FOUND)
                    }
                }
                ("s3", "cp") if args[2].starts_with("s3://") => {
                    let key = args[2]
                        .trim_start_matches(&format!("s3://{BUCKET}/"))
                        .to_owned();
                    assert_eq!(args[3], "-");
                    ok(self.bucket.borrow().get(&key).cloned().unwrap_or_default())
                }
                ("s3", "cp") => {
                    let bytes = self
                        .local
                        .get(&args[2])
                        .cloned()
                        .ok_or("fichier local inconnu")?;
                    let key = args[3]
                        .trim_start_matches(&format!("s3://{BUCKET}/"))
                        .to_owned();
                    self.bucket.borrow_mut().insert(key, bytes);
                    ok(Vec::new())
                }
                other => Err(format!("appel inattendu : {other:?}")),
            }
        }

        fn elapsed(&self) -> Duration {
            self.clock.get()
        }

        fn sleep(&self, duration: Duration) {
            self.clock.set(self.clock.get() + duration);
        }
    }

    fn setup() -> (PathBuf, Files, Fake) {
        let stage_dir = PathBuf::from("stage-virtuel");
        let files = Fixture::new().staged();
        let fake = Fake::new(&stage_dir, &files);
        (stage_dir, files, fake)
    }

    fn run(
        fake: &Fake,
        stage_dir: &Path,
        files: &Files,
        confirm: bool,
    ) -> (Result<(), String>, Vec<String>) {
        let mut lines = Vec::new();
        let result = publish(fake, stage_dir, VERSION, files, confirm, &mut |line| {
            lines.push(line)
        });
        (result, lines)
    }

    #[test]
    fn the_dry_run_shows_every_write_and_calls_nothing() {
        let (dir, files, fake) = setup();
        let (result, lines) = run(&fake, &dir, &files, false);
        result.unwrap();
        assert!(fake.calls.borrow().is_empty(), "ni aws ni curl");
        assert!(fake.bucket.borrow().is_empty());
        let text = lines.join("\n");
        assert!(text.contains("ESSAI À BLANC") && text.contains("--confirm"));
        for write in plan(&dir, VERSION, &files) {
            assert!(
                text.contains(&write.key) && text.contains(&write.sum),
                "{}",
                write.key
            );
        }
        println!("{text}");
    }

    #[test]
    fn the_writes_come_in_the_order_of_the_conception_with_the_manifest_last() {
        let (dir, files, fake) = setup();
        let (result, _) = run(&fake, &dir, &files, true);
        result.unwrap();
        let log = fake.log();
        let versioned: Vec<String> = release_file_names(VERSION)
            .iter()
            .map(|name| format!("put {}", version_key(VERSION, name)))
            .collect();
        let script = format!("put {SCRIPT_KEY}");
        let manifest = format!("put {MANIFEST_KEY}");
        assert_eq!(
            log.last(),
            Some(&manifest),
            "le manifeste est écrit EN DERNIER"
        );
        let at = |entry: &String| log.iter().position(|line| line == entry).unwrap();
        let last_version_write = versioned.iter().map(at).max().unwrap();
        let first_read_back = log
            .iter()
            .position(|line| line.starts_with("curl "))
            .unwrap();
        let last_read_back = log
            .iter()
            .rposition(|line| line.starts_with("curl "))
            .unwrap();
        assert!(
            last_version_write < first_read_back,
            "les fichiers d'abord, puis la relecture"
        );
        assert!(last_read_back < at(&script), "le script après la relecture");
        assert!(at(&script) < at(&manifest));
        assert_eq!(fake.writes().len(), 10, "8 fichiers + script + manifeste");
        // Chaque clé est cherchée AVANT la première écriture.
        let first_put = log
            .iter()
            .position(|line| line.starts_with("put "))
            .unwrap();
        assert_eq!(
            log[..first_put]
                .iter()
                .filter(|line| line.starts_with("head "))
                .count(),
            8
        );
        // Les identifiants ne passent jamais par un argument ; l'endpoint est celui de Scaleway.
        for (_, args) in fake.calls.borrow().iter() {
            for arg in args {
                let lower = arg.to_lowercase();
                assert!(
                    !lower.contains("access_key") && !lower.contains("secret"),
                    "{arg}"
                );
            }
        }
        // Types de contenu posés.
        let calls = fake.calls.borrow();
        let manifest_put = calls
            .iter()
            .find(|(_, a)| {
                a.get(3)
                    .is_some_and(|d| d.ends_with("releases/manifest.json"))
            })
            .unwrap();
        assert!(
            manifest_put
                .1
                .windows(2)
                .any(|p| p[0] == "--content-type" && p[1] == "application/json")
        );
    }

    #[test]
    fn a_key_already_there_with_the_same_content_is_skipped() {
        let (dir, files, fake) = setup();
        let kept = version_key(VERSION, "Hearth_0.1.0_x64-setup.exe");
        fake.bucket
            .borrow_mut()
            .insert(kept.clone(), files[&kept].clone());
        let (result, lines) = run(&fake, &dir, &files, true);
        result.unwrap();
        assert!(!fake.writes().contains(&kept), "passée, pas réécrite");
        assert!(
            lines
                .iter()
                .any(|line| line.contains("déjà présent") && line.contains(&kept))
        );
        assert_eq!(fake.writes().len(), 9);
        assert_eq!(fake.log().last(), Some(&format!("put {MANIFEST_KEY}")));
    }

    #[test]
    fn a_key_already_there_with_another_content_is_refused_before_any_write() {
        let (dir, files, fake) = setup();
        // La clé 3 sur 8 (après des clés absentes) a un autre contenu : rien ne doit être écrit avant.
        let taken = version_key(VERSION, AGENT_BINARY);
        fake.bucket
            .borrow_mut()
            .insert(taken.clone(), b"un autre binaire".to_vec());
        let (result, lines) = run(&fake, &dir, &files, true);
        let error = result.unwrap_err();
        assert!(
            error.contains("REFUS")
                && error.contains(&taken)
                && error.contains("ne se réécrit jamais"),
            "{error}"
        );
        assert!(
            fake.writes().is_empty(),
            "aucune écriture : {:?}",
            fake.writes()
        );
        assert!(!fake.log().iter().any(|line| line.starts_with("curl ")));
        assert!(lines.is_empty());
        assert_eq!(fake.bucket.borrow().len(), 1, "le bucket n'a pas bougé");
    }

    #[test]
    fn an_unreadable_bucket_stops_before_any_write_and_says_what_to_check() {
        let (dir, files, fake) = setup();
        *fake.head_error.borrow_mut() = Some(
            "An error occurred (403) when calling the HeadObject operation: Forbidden".to_owned(),
        );
        let (result, _) = run(&fake, &dir, &files, true);
        let error = result.unwrap_err();
        assert!(
            error.contains("AWS_ACCESS_KEY_ID") && error.contains("Rien n'a été écrit"),
            "{error}"
        );
        assert!(fake.writes().is_empty());
    }

    #[test]
    fn when_the_public_readback_fails_the_manifest_and_the_script_are_not_written() {
        let (dir, files, fake) = setup();
        fake.public.set(Public::Down);
        let (result, _) = run(&fake, &dir, &files, true);
        let error = result.unwrap_err();
        assert!(error.contains("Le manifeste n'est PAS écrit"), "{error}");
        assert!(
            !fake
                .writes()
                .iter()
                .any(|key| key == MANIFEST_KEY || key == SCRIPT_KEY)
        );
        assert!(!fake.bucket.borrow().contains_key(MANIFEST_KEY));
        // La patience est bornée à 5 minutes : ni moins, ni sans fin.
        assert_eq!(fake.clock.get(), PATIENCE);
        // Les fichiers déjà déposés restent : une relance les passe.
        assert_eq!(fake.writes().len(), 8);
    }

    #[test]
    fn a_wrong_sum_on_the_public_readback_means_no_manifest() {
        let (dir, files, fake) = setup();
        fake.public.set(Public::Corrupt);
        let (result, _) = run(&fake, &dir, &files, true);
        let error = result.unwrap_err();
        assert!(error.contains("la somme relue"), "{error}");
        assert!(!fake.bucket.borrow().contains_key(MANIFEST_KEY));
    }

    #[test]
    fn the_readback_waits_for_a_slow_cache_then_succeeds() {
        struct Slow(Fake, Cell<u32>);
        impl Host for Slow {
            fn run(&self, program: &str, args: &[String]) -> Result<CmdOutput, String> {
                if program == "curl" {
                    self.1.set(self.1.get() + 1);
                    // Les 8 premières relectures (tout le premier tour) échouent.
                    if self.1.get() <= 8 {
                        return fail("curl: (22) 404");
                    }
                }
                self.0.run(program, args)
            }
            fn elapsed(&self) -> Duration {
                self.0.elapsed()
            }
            fn sleep(&self, duration: Duration) {
                self.0.sleep(duration);
            }
        }
        let (dir, files, fake) = setup();
        let slow = Slow(fake, Cell::new(0));
        let mut lines = Vec::new();
        publish(&slow, &dir, VERSION, &files, true, &mut |line| {
            lines.push(line)
        })
        .unwrap();
        assert_eq!(slow.0.clock.get(), POLL, "un seul tour d'attente");
        assert!(slow.0.bucket.borrow().contains_key(MANIFEST_KEY));
    }

    #[test]
    fn a_rerun_after_a_complete_deposit_writes_only_the_two_mutable_objects() {
        let (dir, files, fake) = setup();
        run(&fake, &dir, &files, true).0.unwrap();
        fake.calls.borrow_mut().clear();
        run(&fake, &dir, &files, true).0.unwrap();
        assert_eq!(fake.writes(), [SCRIPT_KEY, MANIFEST_KEY]);
    }

    #[test]
    fn content_types_follow_the_api_table() {
        assert_eq!(content_type("releases/manifest.json"), "application/json");
        assert_eq!(
            content_type("releases/0.1.0/manifest.json"),
            "application/json"
        );
        assert_eq!(
            content_type("releases/install.sh"),
            "text/x-shellscript; charset=utf-8"
        );
        for name in [
            "a.exe.sig",
            "hearth-agent-linux-x86_64.minisig",
            "hearth-agent-linux-x86_64.sha256",
            "SHA256SUMS",
        ] {
            assert_eq!(
                content_type(&format!("releases/0.1.0/{name}")),
                "text/plain; charset=utf-8"
            );
        }
        assert_eq!(
            content_type("releases/0.1.0/hearth-agent-linux-x86_64"),
            "application/octet-stream"
        );
    }
}
