//! `cargo xtask release-upload [--confirm] [--allow-rollback]` (HRT-51, conception de la distribution
//! 8.2) : dépose au bucket privé le dossier assemblé par `release-stage`.
//!
//! - SANS `--confirm` : ESSAI À BLANC. Affiche chaque écriture prévue (clé d'objet, somme, taille),
//!   n'écrit rien et ne lance AUCUNE commande (ni `aws`, ni `curl`).
//! - AVEC `--confirm`, dans cet ordre, jamais un autre :
//!   1. les fichiers de la version (`releases/X.Y.Z/*`). AVANT toute écriture, chaque clé est cherchée :
//!      absente, elle sera écrite ; présente avec le MÊME contenu, elle est passée ; présente avec un
//!      AUTRE contenu, REFUS et arrêt sans rien avoir écrit (une version déposée ne se réécrit jamais,
//!      BR-RELEASE-001). Toujours avant toute écriture : lecture du manifeste en place, refus si sa
//!      version est SUPÉRIEURE à celle déposée (sauf `--allow-rollback`) ;
//!   2. relecture de chaque fichier par l'adresse publique `https://hearth.voikyrioh.fr/api/releases/…`
//!      (Cloudflare, Traefik, API, bucket) et comparaison des sommes, avec une patience de 5 minutes ;
//!   3. le script `releases/install.sh`, relu ensuite dans le bucket ;
//!   4. le manifeste `releases/manifest.json`, EN DERNIER, seulement si toutes les relectures sont
//!      bonnes (BR-RELEASE-002), relu ensuite dans le bucket.
//!
//! Chaque objet est déposé depuis les OCTETS VÉRIFIÉS en mémoire (entrée standard de
//! `aws s3 cp - s3://…`), jamais relus du disque.
//!
//! Le bucket est atteint par l'outil `aws` (`--endpoint-url https://s3.fr-par.scw.cloud`) ; les
//! identifiants sont lus dans l'environnement par `aws` lui-même : ce module ne les lit, ne les affiche
//! et ne les passe JAMAIS en argument. Toute exécution externe (`aws`, `curl`) et toute attente passent
//! par `Host`, injecté : les tests n'ont ni réseau ni `aws`. Arguments en liste, aucun interpréteur.
//!
//! La relecture publique passe par `curl` (livré avec Windows 10 et suivants) plutôt que par un client
//! HTTP en dépendance : `xtask` n'en a pas, et la relecture doit de toute façon traverser la chaîne
//! publique entière.

use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use crate::client_release::root;
use crate::host::{Call, CmdOutput, Host, System};
use crate::release_core::{
    AGENT_BINARY, DOWNLOAD_PREFIX, check_version, installer_file_name, workspace_version,
};
use crate::release_stage::*;

pub const ENDPOINT: &str = "https://s3.fr-par.scw.cloud";
pub const REGION: &str = "fr-par";
pub const BUCKET: &str = "voikyrioh-hearth-releases";
/// Patience de la relecture publique (cache Cloudflare, démarrage de l'API) : 5 minutes.
pub const PATIENCE: Duration = Duration::from_secs(300);
/// Pause entre deux tours de relecture.
pub const POLL: Duration = Duration::from_secs(10);
/// Variables posées sur le processus `aws` (pas des secrets) : l'API a coupé les sommes de contrôle
/// facultatives pour ce stockage compatible S3 (`s3.bucket.ts`), l'outil de dépôt fait de même (AWS CLI
/// v2 récente : somme CRC en fin de corps par défaut).
pub const AWS_ENV: [(&str, &str); 2] = [
    ("AWS_REQUEST_CHECKSUM_CALCULATION", "when_required"),
    ("AWS_RESPONSE_CHECKSUM_VALIDATION", "when_required"),
];
/// La phrase qui prouve l'absence d'une clé : celle de `aws s3api head-object` pour un 404 (ligne
/// entière, texte exact). Relevée contre Scaleway (fr-par) : AWS CLI 2.27.0 l'écrit seule sur sa ligne,
/// AWS CLI 2.38.1 la fait précéder de `ABSENT_PREFIX`. Aucune autre forme n'est acceptée.
pub const ABSENT_PHRASE: &str =
    "An error occurred (404) when calling the HeadObject operation: Not Found";
/// Préfixe que les AWS CLI récents (relevé sur 2.38.1) ajoutent à la ligne d'erreur, exactement.
pub const ABSENT_PREFIX: &str = "aws: [ERROR]: ";
/// Code de sortie de `aws` pour une erreur renvoyée par le service (un 404 en est une).
pub const SERVICE_ERROR_CODE: i32 = 254;
/// Bornes de `curl` pour chaque relecture publique (l'installateur pèse des dizaines de Mo).
const CURL_CONNECT_TIMEOUT: &str = "15";
const CURL_MAX_TIME: &str = "120";

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
    pub sum: String,
    pub size: usize,
    pub content_type: &'static str,
    pub kind: Kind,
}

/// Options de `release-upload`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options<'a> {
    /// Sans elle : essai à blanc.
    pub confirm: bool,
    /// Autorise à remplacer un manifeste annonçant une version SUPÉRIEURE (retour arrière voulu).
    pub allow_rollback: bool,
    /// Dossier assemblé réellement lu (cité dans la commande manuelle de secours).
    pub stage_dir: &'a str,
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

/// L'ordre des écritures : les fichiers de la version, le script, le manifeste. Une clé dont le fichier
/// manque est une ERREUR (jamais retirée en silence).
pub fn plan(version: &str, files: &Files) -> Result<Vec<PlannedWrite>, String> {
    let mut keys: Vec<(String, Kind)> = release_file_names(version)
        .iter()
        .map(|name| (version_key(version, name), Kind::Version))
        .collect();
    keys.push((SCRIPT_KEY.to_owned(), Kind::Script));
    keys.push((MANIFEST_KEY.to_owned(), Kind::Manifest));
    keys.into_iter()
        .map(|(key, kind)| {
            let bytes = files.get(&key).ok_or_else(|| {
                format!("{key} manque dans le dossier assemblé : rien n'est déposé, relance release-stage")
            })?;
            Ok(PlannedWrite {
                sum: sha256_hex(bytes),
                size: bytes.len(),
                content_type: content_type(&key),
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

/// Lance `aws` avec les variables de somme de contrôle et, pour un dépôt, les octets en entrée.
fn aws(host: &dyn Host, argv: &[String], input: Option<&[u8]>) -> Result<CmdOutput, String> {
    let mut call = Call::new("aws", argv).env(&AWS_ENV);
    if let Some(input) = input {
        call = call.input(input);
    }
    host.run(&call)
}

/// État d'une clé dans le bucket.
#[derive(Debug, PartialEq, Eq)]
enum Remote {
    Absent,
    Same,
    /// Présente avec une autre somme (celle relue).
    Different(String),
}

/// `head-object` est un 404 ? Seulement si `aws` sort avec le code des erreurs de service ET qu'une
/// ligne de sa sortie d'erreur est EXACTEMENT la phrase attendue, seule ou précédée exactement de
/// `ABSENT_PREFIX` (rien d'autre devant, rien derrière). Tout le reste (403, délai, réseau,
/// texte qui contient « 404 » ou « not found » ailleurs) n'est PAS une absence.
fn proves_absence(output: &CmdOutput) -> bool {
    output.code == Some(SERVICE_ERROR_CODE)
        && output
            .stderr
            .lines()
            // FIX:01M52VPMJ3KKEN1MP5G1C63QQN
            .any(|line| {
                let line = line.trim();
                line == ABSENT_PHRASE || line.strip_prefix(ABSENT_PREFIX) == Some(ABSENT_PHRASE)
            })
}

fn code_text(output: &CmdOutput) -> String {
    output.code.map_or_else(
        || "tué par un signal".to_owned(),
        |code| format!("code {code}"),
    )
}

/// Le contenu de la clé dans le bucket : `None` = absente (prouvée, voir `proves_absence`), sinon
/// `aws s3api head-object` puis `aws s3 cp s3://… -` : le contenu est RELU et haché (l'ETag d'un
/// dépôt en plusieurs parties n'est pas une somme du contenu). Toute autre issue est une erreur.
fn fetch(host: &dyn Host, key: &str) -> Result<Option<Vec<u8>>, String> {
    let mut head = args(&["s3api", "head-object", "--bucket", BUCKET, "--key", key]);
    head.extend(aws_common());
    let found = aws(host, &head, None)?;
    if !found.success {
        if proves_absence(&found) {
            return Ok(None);
        }
        return Err(format!(
            "l'état de {key} dans le bucket est impossible à établir ({}, {}). Vérifie les variables AWS_ACCESS_KEY_ID et AWS_SECRET_ACCESS_KEY de l'environnement (clé de publication de Voiky), le droit de LECTURE d'objet de cette clé (sans droit de lister, une clé absente répond 403 et non 404), puis le réseau et {ENDPOINT}",
            code_text(&found),
            short(&found.stderr)
        ));
    }
    let mut get = args(&["s3", "cp", &format!("s3://{BUCKET}/{key}"), "-"]);
    get.extend(aws_common());
    get.push("--only-show-errors".to_owned());
    let read = aws(host, &get, None)?;
    if !read.success {
        return Err(format!(
            "{key} existe mais n'a pas pu être relue ({}, {})",
            code_text(&read),
            short(&read.stderr)
        ));
    }
    Ok(Some(read.stdout))
}

fn remote_state(host: &dyn Host, write: &PlannedWrite) -> Result<Remote, String> {
    Ok(match fetch(host, &write.key)? {
        None => Remote::Absent,
        Some(bytes) => {
            let found = sha256_hex(&bytes);
            if found == write.sum {
                Remote::Same
            } else {
                Remote::Different(found)
            }
        }
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

/// Dépose les OCTETS VÉRIFIÉS (entrée standard), pas le fichier du disque.
fn put(host: &dyn Host, write: &PlannedWrite, bytes: &[u8]) -> Result<(), String> {
    let mut put = args(&["s3", "cp", "-", &format!("s3://{BUCKET}/{}", write.key)]);
    put.extend(aws_common());
    put.extend(args(&[
        "--content-type",
        write.content_type,
        "--only-show-errors",
    ]));
    let done = aws(host, &put, Some(bytes))?;
    if done.success {
        Ok(())
    } else {
        Err(format!(
            "l'écriture de {} a échoué ({}, {}). Relance la même commande : ce qui est déjà déposé à l'identique est passé",
            write.key,
            code_text(&done),
            short(&done.stderr)
        ))
    }
}

/// Ce qu'il faut faire quand un objet mutable relu n'est pas celui qu'on vient de déposer : relancer la
/// même commande (elle redépose script et manifeste depuis les octets vérifiés, puis les relit), sinon
/// la commande manuelle complète, depuis le dossier assemblé réellement lu.
fn restore_hint(write: &PlannedWrite, stage_dir: &str) -> String {
    let env = AWS_ENV
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join(" ");
    let stage_dir = stage_dir.trim_end_matches(['/', '\\']);
    format!(
        "Premier remède : relance la même commande (elle redépose {key} depuis les octets vérifiés du dossier assemblé et le relit). Sinon, à la main, avec ces deux variables d'environnement posées ({env}) : aws s3 cp {stage_dir}/{key} s3://{BUCKET}/{key} --endpoint-url {ENDPOINT} --region {REGION} --content-type \"{ct}\" (fichier de sha256 {sum}, {size} octets)",
        key = write.key,
        sum = write.sum,
        size = write.size,
        ct = write.content_type
    )
}

/// Relit par le BUCKET (donc sans cache) un objet mutable qu'on vient d'écrire : seule preuve que
/// l'annonce est bonne. Sinon, une erreur qui dit exactement quoi remettre.
fn verify_written(host: &dyn Host, write: &PlannedWrite, stage_dir: &str) -> Result<(), String> {
    let consequence = match write.kind {
        Kind::Script => {
            "Le script servi par /api/install.sh peut être FAUX tant que ce n'est pas remis ; le manifeste n'est PAS écrit"
        }
        _ => "L'annonce peut être fausse tant que ce n'est pas remis",
    };
    let state = remote_state(host, write).map_err(|error| {
        format!(
            "après l'écriture de {} : {error}. {consequence}. {}",
            write.key,
            restore_hint(write, stage_dir)
        )
    })?;
    match state {
        Remote::Same => Ok(()),
        Remote::Absent => Err(format!(
            "{} est introuvable dans le bucket juste après son écriture. {consequence}. {}",
            write.key,
            restore_hint(write, stage_dir)
        )),
        Remote::Different(found) => Err(format!(
            "{} relu dans le bucket (sha256 {found}) n'est pas celui déposé (sha256 {}). {consequence}. {}",
            write.key,
            write.sum,
            restore_hint(write, stage_dir)
        )),
    }
}

/// Relit chaque fichier de la version par l'adresse publique jusqu'à ce que toutes les sommes soient
/// bonnes, ou que la patience soit épuisée (vérifiée aussi à l'intérieur d'un tour).
fn read_back(
    host: &dyn Host,
    version: &str,
    writes: &[PlannedWrite],
    say: &mut dyn FnMut(String),
) -> Result<(), String> {
    let start = host.elapsed();
    let expired = || host.elapsed().saturating_sub(start) >= PATIENCE;
    let mut pending: Vec<(&PlannedWrite, String)> = writes
        .iter()
        .filter(|write| write.kind == Kind::Version)
        .map(|write| (write, String::from("pas encore relu")))
        .collect();
    loop {
        let mut out_of_time = false;
        for (write, reason) in &mut pending {
            let name = write.key.rsplit('/').next().unwrap_or(&write.key);
            let mut curl = args(&[
                "--disable",
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
                "--connect-timeout",
                CURL_CONNECT_TIMEOUT,
                "--max-time",
                CURL_MAX_TIME,
            ]);
            curl.push(format!("{DOWNLOAD_PREFIX}{version}/{name}"));
            *reason = match host.run(&Call::new("curl", &curl)) {
                Ok(read) if read.success && sha256_hex(&read.stdout) == write.sum => String::new(),
                Ok(read) if read.success => {
                    "la somme relue n'est pas celle du fichier déposé".to_owned()
                }
                Ok(read) => short(&read.stderr),
                Err(error) => return Err(error),
            };
            if expired() {
                out_of_time = true;
                break;
            }
        }
        let before = pending.len();
        pending.retain(|(_, reason)| !reason.is_empty());
        if pending.is_empty() {
            return Ok(());
        }
        if pending.len() != before {
            say(format!("relu : {} fichier(s) restant(s)", pending.len()));
        }
        if out_of_time || expired() {
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

/// `X.Y.Z` -> nombres (la forme a déjà été vérifiée par `check_version`).
fn triple(version: &str) -> Result<(u64, u64, u64), String> {
    check_version(version)?;
    let mut parts = version.split('.').map(|part| {
        part.parse::<u64>()
            .map_err(|_| format!("version « {version} » trop grande"))
    });
    match (parts.next(), parts.next(), parts.next()) {
        (Some(major), Some(minor), Some(patch)) => Ok((major?, minor?, patch?)),
        _ => Err(format!("version « {version} » : attendu X.Y.Z")),
    }
}

/// Lit le manifeste EN PLACE dans le bucket (avant toute écriture) : refus si sa version, client ou
/// agent, est supérieure à celle déposée, sauf retour arrière explicite. Absent = premier dépôt.
/// Rend les versions annoncées plus récentes que celle déposée (vide hors retour arrière accepté) : le
/// retour arrière accepté doit laisser une ligne, que l'appelant affiche avant la première écriture.
fn check_not_older(
    host: &dyn Host,
    version: &str,
    allow_rollback: bool,
) -> Result<Vec<String>, String> {
    let Some(bytes) = fetch(host, MANIFEST_KEY)? else {
        return Ok(Vec::new());
    };
    let unreadable = |detail: &str| {
        format!(
            "le manifeste en place ({MANIFEST_KEY}) est illisible ({detail}) : impossible de savoir quelle version est annoncée. Rien n'a été écrit. Corrige-le à la console du bucket, puis relance"
        )
    };
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|error| unreadable(&error.to_string()))?;
    let deposited = triple(version)?;
    let mut newer = Vec::new();
    let fields = [
        ("version (client)", value["version"].as_str(), true),
        ("agent.version", value["agent"]["version"].as_str(), false),
    ];
    for (label, found, required) in fields {
        match found {
            Some(found) => {
                let found_triple = triple(found).map_err(|error| unreadable(&error))?;
                if found_triple > deposited {
                    newer.push(format!("{label} {found}"));
                }
            }
            None if required => return Err(unreadable(&format!("{label} manque"))),
            None => {}
        }
    }
    if newer.is_empty() || allow_rollback {
        return Ok(newer);
    }
    Err(format!(
        "REFUS : le manifeste en place annonce {} alors que la version déposée est {version}. Un dépôt ne ramène pas la version annoncée en arrière par accident (dossier assemblé resté sous target/ après un retour sur un ancien commit ?). Rien n'a été écrit. Si le retour arrière est voulu, relance avec --allow-rollback",
        newer.join(" et ")
    ))
}

/// Dépose (ou, sans `confirm`, décrit) la version. `say` reçoit chaque ligne de compte rendu.
pub fn publish(
    host: &dyn Host,
    version: &str,
    files: &Files,
    options: Options,
    say: &mut dyn FnMut(String),
) -> Result<(), String> {
    let writes = plan(version, files)?;
    // Le compte exact : les fichiers de la liste fermée, le script et le manifeste.
    let expected = release_file_names(version).len() + 2;
    if writes.len() != expected {
        return Err(format!(
            "{} écriture(s) prévue(s) au lieu de {expected} : rien n'est déposé",
            writes.len()
        ));
    }
    if !options.confirm {
        say(format!(
            "ESSAI À BLANC : rien n'est écrit et ni aws ni curl ne sont lancés (seul git, en lecture, vérifie l'arbre et le commit). Ajoute --confirm pour déposer. Bucket s3://{BUCKET} ({ENDPOINT})"
        ));
        say(
            "avant toute écriture : lecture du manifeste en place, refus si sa version est supérieure à celle déposée (--allow-rollback pour un retour arrière voulu)"
                .to_owned(),
        );
        if options.allow_rollback {
            say(
                "--allow-rollback est ARMÉ : si le manifeste en place annonce une version supérieure à celle déposée, il sera remplacé sans refus (le dépôt réel affichera alors une ligne « RETOUR ARRIÈRE » avant sa première écriture)"
                    .to_owned(),
            );
        }
        for (at, write) in writes.iter().enumerate() {
            let note = match write.kind {
                Kind::Version => {
                    "refus si la clé existe avec un autre contenu, passée si identique"
                }
                Kind::Script => {
                    "après la relecture publique des fichiers de la version, puis relu dans le bucket"
                }
                Kind::Manifest => {
                    "EN DERNIER, seulement si toutes les relectures sont bonnes, puis relu dans le bucket"
                }
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
    let nothing_written = |error: String| format!("{error}. Rien n'a été écrit");
    let mut state = Vec::new();
    let mut refused = Vec::new();
    for write in writes.iter().filter(|write| write.kind == Kind::Version) {
        let remote = remote_state(host, write).map_err(nothing_written)?;
        if matches!(remote, Remote::Different(_)) {
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
    // 1a bis. Le manifeste en place n'annonce pas une version plus récente.
    let newer = check_not_older(host, version, options.allow_rollback).map_err(|error| {
        if error.contains("Rien n'a été écrit") {
            error
        } else {
            nothing_written(error)
        }
    })?;
    if !newer.is_empty() {
        say(format!(
            "RETOUR ARRIÈRE : le manifeste en place annonce {}, il sera remplacé par {version}",
            newer.join(" et ")
        ));
    }
    // 1b. Écriture de ce qui manque, depuis les octets vérifiés.
    for (write, remote) in &state {
        if *remote == Remote::Same {
            say(format!("déjà présent, identique : {}", write.key));
        } else {
            put(host, write, &files[&write.key])?;
            say(format!("écrit : {}", write.key));
        }
    }
    // 2. Relecture publique de tous les fichiers de la version.
    read_back(host, version, &writes, say)?;
    say("relecture publique : toutes les sommes sont bonnes".to_owned());
    // 3 et 4. Le script, puis le manifeste EN DERNIER, chacun relu dans le bucket (sans cache).
    for kind in [Kind::Script, Kind::Manifest] {
        for write in writes.iter().filter(|write| write.kind == kind) {
            put(host, write, &files[&write.key])?;
            say(format!("écrit : {}", write.key));
            verify_written(host, write, options.stage_dir)?;
            say(format!("relu dans le bucket, identique : {}", write.key));
        }
    }
    let agent_sum = writes
        .iter()
        .find(|write| write.key == version_key(version, AGENT_BINARY))
        .map(|write| write.sum.clone())
        .ok_or("somme de l'agent introuvable")?;
    say(format!(
        "version {version} annoncée : le script et le manifeste relus dans le bucket sont ceux déposés."
    ));
    say(
        "À relire à la main (le cache de Cloudflare peut retarder `latest` de 5 minutes au plus) :"
            .to_owned(),
    );
    say(format!(
        "  {DOWNLOAD_PREFIX}latest : identique, octet pour octet, à releases/manifest.json du dossier assemblé"
    ));
    say(format!(
        "  {DOWNLOAD_PREFIX}latest/agent : UNE ligne, exactement : {version} {agent_sum} {version}/{AGENT_BINARY}"
    ));
    say(format!(
        "  {DOWNLOAD_PREFIX}latest/installer : 302 vers /api/releases/{version}/{}",
        installer_file_name(version)
    ));
    say(format!(
        "puis poser l'étiquette v{version} sur le commit construit (hors heures employeur)"
    ));
    Ok(())
}

/// Les arguments de `release-upload`, lus UNE seule fois.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct UploadArgs {
    pub confirm: bool,
    pub allow_rollback: bool,
    pub stage_dir: Option<String>,
}

pub fn parse_upload_args(args: &[String]) -> Result<UploadArgs, String> {
    check_flags(args, &["--stage-dir"], &["--confirm", "--allow-rollback"])?;
    let mut parsed = UploadArgs::default();
    let mut at = 0;
    while at < args.len() {
        match args[at].as_str() {
            // La valeur est la suivante, quelle qu'elle soit : `--stage-dir --confirm` n'arme rien.
            "--stage-dir" => {
                parsed.stage_dir = args.get(at + 1).cloned();
                at += 2;
            }
            "--confirm" => {
                parsed.confirm = true;
                at += 1;
            }
            "--allow-rollback" => {
                parsed.allow_rollback = true;
                at += 1;
            }
            _ => at += 1,
        }
    }
    Ok(parsed)
}

/// `cargo xtask release-upload [--confirm] [--allow-rollback] [--stage-dir D]`.
pub fn run_upload(args: &[String]) -> Result<(), String> {
    upload_with(&System::new(), &root()?, args, &mut |line| {
        println!("{line}")
    })
}

/// Le corps de `release-upload`, avec l'exécuteur et la racine injectés. La garde de l'arbre et du
/// commit (HRT-60) passe avant la vérification et avant tout appel de `publish` (donc de `aws` et
/// `curl`), essai à blanc compris.
pub fn upload_with(
    host: &dyn Host,
    root: &Path,
    args: &[String],
    say: &mut dyn FnMut(String),
) -> Result<(), String> {
    let parsed = parse_upload_args(args)?;
    let stage_dir = parsed
        .stage_dir
        .map_or_else(|| root.join(STAGE_DIR), |dir| Path::new(&dir).to_path_buf());
    let files = read_stage_dir(&stage_dir)?;
    // HRT-60, essai à blanc compris : arbre propre et commit identique à celui assemblé (git, lecture seule).
    let commit = check_stage_commit(host, root, read_stage_commit(&stage_dir))?;
    say(format!(
        "arbre propre, commit en place identique à celui assemblé : {commit}"
    ));
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
        host,
        &version,
        &files,
        Options {
            confirm: parsed.confirm,
            allow_rollback: parsed.allow_rollback,
            stage_dir: &stage_dir.to_string_lossy(),
        },
        say,
    )
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::collections::BTreeMap;

    use super::*;
    use crate::release_stage::fixtures::*;

    const CONFIRM: Options = Options {
        confirm: true,
        allow_rollback: false,
        stage_dir: "target/release-stage",
    };

    const ROLLBACK: Options = Options {
        confirm: true,
        allow_rollback: true,
        stage_dir: "target/release-stage",
    };

    /// Ce que le faux a reçu pour un appel.
    #[derive(Debug, Clone)]
    struct Seen {
        program: String,
        args: Vec<String>,
        input: Option<Vec<u8>>,
        env: Vec<(String, String)>,
    }

    /// Faux monde : un bucket en mémoire qui peut échouer, mentir ou répondre autre chose qu'un 404, une
    /// adresse publique qui le relit, une horloge qui n'avance que par `sleep` ou par la durée d'un appel
    /// `curl`. Enregistre chaque appel.
    struct Fake {
        calls: RefCell<Vec<Seen>>,
        bucket: RefCell<BTreeMap<String, Vec<u8>>>,
        public: Cell<Public>,
        /// Réponse de `head-object` pour une clé ABSENTE (code de sortie, sortie d'erreur).
        absent_answer: RefCell<(Option<i32>, String)>,
        /// La lecture `cp s3://… -` d'une clé présente échoue.
        get_error: RefCell<Option<String>>,
        /// L'écriture numéro N (à partir de 1) échoue une fois.
        fail_put_number: Cell<Option<usize>>,
        puts_seen: Cell<usize>,
        /// Clé -> octets réellement stockés À LA PLACE de ceux qu'on a donnés.
        store_instead: RefCell<BTreeMap<String, Vec<u8>>>,
        /// Durée que prend chaque appel `curl` (horloge).
        curl_takes: Cell<Duration>,
        clock: Cell<Duration>,
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Public {
        Ok,
        Down,
        Corrupt,
    }

    fn not_found() -> (Option<i32>, String) {
        (Some(SERVICE_ERROR_CODE), ABSENT_PHRASE.to_owned())
    }

    impl Fake {
        fn new() -> Self {
            Self {
                calls: RefCell::new(Vec::new()),
                bucket: RefCell::new(BTreeMap::new()),
                public: Cell::new(Public::Ok),
                absent_answer: RefCell::new(not_found()),
                get_error: RefCell::new(None),
                fail_put_number: Cell::new(None),
                puts_seen: Cell::new(0),
                store_instead: RefCell::new(BTreeMap::new()),
                curl_takes: Cell::new(Duration::ZERO),
                clock: Cell::new(Duration::ZERO),
            }
        }

        fn is_put(seen: &Seen) -> bool {
            seen.program == "aws"
                && seen.args.first().map(String::as_str) == Some("s3")
                && seen.args.get(1).map(String::as_str) == Some("cp")
                && seen.args.get(2).map(String::as_str) == Some("-")
        }

        fn key_of(url: &str) -> String {
            url.trim_start_matches(&format!("s3://{BUCKET}/"))
                .to_owned()
        }

        /// Les clés écrites, dans l'ordre.
        fn writes(&self) -> Vec<String> {
            self.calls
                .borrow()
                .iter()
                .filter(|seen| Self::is_put(seen))
                .map(|seen| Self::key_of(&seen.args[3]))
                .collect()
        }

        fn puts(&self) -> Vec<Seen> {
            self.calls
                .borrow()
                .iter()
                .filter(|seen| Self::is_put(seen))
                .cloned()
                .collect()
        }

        fn log(&self) -> Vec<String> {
            self.calls
                .borrow()
                .iter()
                .map(|seen| {
                    if seen.program == "curl" {
                        format!("curl {}", seen.args.last().cloned().unwrap_or_default())
                    } else if seen.args.get(1).map(String::as_str) == Some("head-object") {
                        format!("head {}", seen.args[5])
                    } else if Self::is_put(seen) {
                        format!("put {}", Self::key_of(&seen.args[3]))
                    } else {
                        format!("get {}", Self::key_of(&seen.args[2]))
                    }
                })
                .collect()
        }

        fn insert(&self, key: &str, bytes: &[u8]) {
            self.bucket
                .borrow_mut()
                .insert(key.to_owned(), bytes.to_vec());
        }
    }

    fn ok(stdout: Vec<u8>) -> Result<CmdOutput, String> {
        Ok(CmdOutput {
            success: true,
            code: Some(0),
            stdout,
            stderr: String::new(),
        })
    }

    fn fail(code: Option<i32>, stderr: &str) -> Result<CmdOutput, String> {
        Ok(CmdOutput {
            success: false,
            code,
            stdout: Vec::new(),
            stderr: stderr.to_owned(),
        })
    }

    impl Host for Fake {
        fn run(&self, call: &Call) -> Result<CmdOutput, String> {
            self.calls.borrow_mut().push(Seen {
                program: call.program.to_owned(),
                args: call.args.to_vec(),
                input: call.input.map(<[u8]>::to_vec),
                env: call
                    .env
                    .iter()
                    .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                    .collect(),
            });
            let args = call.args;
            if call.program == "curl" {
                self.clock.set(self.clock.get() + self.curl_takes.get());
                let url = args.last().cloned().unwrap_or_default();
                let key = url.replace("https://hearth.voikyrioh.fr/api/", "");
                return match (self.public.get(), self.bucket.borrow().get(&key)) {
                    (Public::Ok, Some(bytes)) => ok(bytes.clone()),
                    (Public::Corrupt, Some(_)) => ok(b"autre chose".to_vec()),
                    _ => fail(Some(22), "curl: (22) The requested URL returned error: 404"),
                };
            }
            assert_eq!(call.program, "aws");
            assert!(
                args.windows(2)
                    .any(|pair| pair[0] == "--endpoint-url" && pair[1] == ENDPOINT),
                "l'endpoint Scaleway est toujours donné : {args:?}"
            );
            match (args[0].as_str(), args[1].as_str()) {
                ("s3api", "head-object") => {
                    if self.bucket.borrow().contains_key(&args[5]) {
                        ok(b"{}".to_vec())
                    } else {
                        let (code, text) = self.absent_answer.borrow().clone();
                        fail(code, &text)
                    }
                }
                ("s3", "cp") if args[2].starts_with("s3://") => {
                    if let Some(error) = self.get_error.borrow().clone() {
                        return fail(Some(SERVICE_ERROR_CODE), &error);
                    }
                    assert_eq!(args[3], "-");
                    ok(self
                        .bucket
                        .borrow()
                        .get(&Self::key_of(&args[2]))
                        .cloned()
                        .unwrap_or_default())
                }
                ("s3", "cp") => {
                    assert_eq!(args[2], "-", "le dépôt part de l'entrée standard");
                    let number = self.puts_seen.get() + 1;
                    self.puts_seen.set(number);
                    if self.fail_put_number.get() == Some(number) {
                        self.fail_put_number.set(None);
                        return fail(
                            Some(1),
                            "Connection was closed before we received a valid response",
                        );
                    }
                    let key = Self::key_of(&args[3]);
                    let given = call.input.ok_or("dépôt sans entrée")?.to_vec();
                    let stored = self
                        .store_instead
                        .borrow()
                        .get(&key)
                        .cloned()
                        .unwrap_or(given);
                    self.bucket.borrow_mut().insert(key, stored);
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

    fn setup() -> (Files, Fake) {
        (Fixture::new().staged(), Fake::new())
    }

    fn run_with(
        fake: &dyn Host,
        files: &Files,
        options: Options,
    ) -> (Result<(), String>, Vec<String>) {
        let mut lines = Vec::new();
        let result = publish(fake, VERSION, files, options, &mut |line| lines.push(line));
        (result, lines)
    }

    fn run(fake: &dyn Host, files: &Files, confirm: bool) -> (Result<(), String>, Vec<String>) {
        run_with(
            fake,
            files,
            Options {
                confirm,
                allow_rollback: false,
                stage_dir: "target/release-stage",
            },
        )
    }

    fn manifest_of(version: &str, agent_version: Option<&str>) -> Vec<u8> {
        let mut value = serde_json::json!({ "version": version });
        if let Some(agent) = agent_version {
            value["agent"] = serde_json::json!({ "version": agent });
        }
        value.to_string().into_bytes()
    }

    #[test]
    fn the_dry_run_shows_every_write_and_calls_nothing() {
        let (files, fake) = setup();
        let (result, lines) = run(&fake, &files, false);
        result.unwrap();
        assert!(fake.calls.borrow().is_empty(), "ni aws ni curl");
        assert!(fake.bucket.borrow().is_empty());
        let text = lines.join("\n");
        assert!(text.contains("ESSAI À BLANC") && text.contains("--confirm"));
        assert!(text.contains("--allow-rollback"));
        for write in plan(VERSION, &files).unwrap() {
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
        let (files, fake) = setup();
        let (result, _) = run(&fake, &files, true);
        result.unwrap();
        let log = fake.log();
        let versioned: Vec<String> = release_file_names(VERSION)
            .iter()
            .map(|name| format!("put {}", version_key(VERSION, name)))
            .collect();
        let script = format!("put {SCRIPT_KEY}");
        let manifest = format!("put {MANIFEST_KEY}");
        // Le manifeste est le dernier DÉPÔT (il est ensuite relu dans le bucket).
        let last_put = log
            .iter()
            .rposition(|line| line.starts_with("put "))
            .unwrap();
        assert_eq!(log[last_put], manifest, "le manifeste est écrit EN DERNIER");
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
        // Chaque clé de la version est cherchée AVANT la première écriture, puis le manifeste en place.
        let first_put = log
            .iter()
            .position(|line| line.starts_with("put "))
            .unwrap();
        let before: Vec<&String> = log[..first_put]
            .iter()
            .filter(|line| line.starts_with("head "))
            .collect();
        assert_eq!(before.len(), 9, "8 clés de version + le manifeste en place");
        assert_eq!(before[8], &format!("head {MANIFEST_KEY}"));
        // Les identifiants ne passent jamais par un argument.
        for seen in fake.calls.borrow().iter() {
            for arg in &seen.args {
                let lower = arg.to_lowercase();
                assert!(
                    !lower.contains("access_key") && !lower.contains("secret"),
                    "{arg}"
                );
            }
        }
    }

    #[test]
    fn every_object_is_written_from_the_verified_bytes_with_its_content_type() {
        let (files, fake) = setup();
        run(&fake, &files, true).0.unwrap();
        let puts = fake.puts();
        assert_eq!(puts.len(), 10);
        for seen in &puts {
            let key = Fake::key_of(&seen.args[3]);
            // Les octets vérifiés en mémoire, donnés par l'entrée standard (jamais un fichier du disque).
            assert_eq!(seen.args[2], "-", "{key}");
            assert_eq!(seen.input.as_deref(), Some(files[&key].as_slice()), "{key}");
            // Le type de contenu de CHAQUE écriture.
            let wanted = content_type(&key);
            assert!(
                seen.args
                    .windows(2)
                    .any(|pair| pair[0] == "--content-type" && pair[1] == wanted),
                "{key} : {wanted} attendu dans {:?}",
                seen.args
            );
        }
        let type_of = |key: &str| {
            let seen = puts
                .iter()
                .find(|seen| Fake::key_of(&seen.args[3]) == key)
                .unwrap();
            seen.args
                .windows(2)
                .find(|pair| pair[0] == "--content-type")
                .map(|pair| pair[1].clone())
                .unwrap()
        };
        assert_eq!(type_of(MANIFEST_KEY), "application/json");
        assert_eq!(type_of(SCRIPT_KEY), "text/x-shellscript; charset=utf-8");
        assert_eq!(
            type_of(&version_key(VERSION, "Hearth_0.1.0_x64-setup.exe.sig")),
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            type_of(&version_key(VERSION, AGENT_BINARY)),
            "application/octet-stream"
        );
    }

    #[test]
    fn aws_gets_the_checksum_variables_and_curl_gets_timeouts() {
        let (files, fake) = setup();
        run(&fake, &files, true).0.unwrap();
        let mut curls = 0;
        for seen in fake.calls.borrow().iter() {
            if seen.program == "aws" {
                for (name, value) in AWS_ENV {
                    assert!(
                        seen.env.contains(&(name.to_owned(), value.to_owned())),
                        "{name} sur {:?}",
                        seen.args
                    );
                }
            } else {
                curls += 1;
                assert_eq!(seen.args[0], "--disable", "ni .curlrc ni _curlrc");
                let has = |name: &str, value: &str| {
                    seen.args
                        .windows(2)
                        .any(|pair| pair[0] == name && pair[1] == value)
                };
                assert!(has("--connect-timeout", "15") && has("--max-time", "120"));
                assert!(has("--proto", "=https") && has("--proto-redir", "=https"));
            }
        }
        assert_eq!(curls, 8);
    }

    #[test]
    fn a_key_already_there_with_the_same_content_is_skipped() {
        let (files, fake) = setup();
        let kept = version_key(VERSION, "Hearth_0.1.0_x64-setup.exe");
        fake.insert(&kept, &files[&kept]);
        let (result, lines) = run(&fake, &files, true);
        result.unwrap();
        assert!(!fake.writes().contains(&kept), "passée, pas réécrite");
        assert!(
            lines
                .iter()
                .any(|line| line.contains("déjà présent") && line.contains(&kept))
        );
        assert_eq!(fake.writes().len(), 9);
        assert_eq!(fake.writes().last(), Some(&MANIFEST_KEY.to_owned()));
    }

    #[test]
    fn a_key_already_there_with_another_content_is_refused_before_any_write() {
        let (files, fake) = setup();
        // La clé 3 sur 8 (après des clés absentes) a un autre contenu : rien ne doit être écrit avant.
        let taken = version_key(VERSION, AGENT_BINARY);
        fake.insert(&taken, b"un autre binaire");
        let (result, lines) = run(&fake, &files, true);
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

    /// Tout ce qui n'est pas la phrase exacte d'un 404 de `head-object` arrête l'outil AVANT toute écriture :
    /// « état impossible à établir », jamais « absente par défaut ».
    fn assert_stops_before_any_write(code: Option<i32>, text: &str) {
        let (files, fake) = setup();
        *fake.absent_answer.borrow_mut() = (code, text.to_owned());
        let (result, lines) = run(&fake, &files, true);
        let error = result.unwrap_err();
        assert!(
            error.contains("impossible à établir")
                && error.contains("AWS_ACCESS_KEY_ID")
                && error.contains("Rien n'a été écrit"),
            "{text} : {error}"
        );
        assert!(fake.writes().is_empty(), "{text} : {:?}", fake.writes());
        assert!(lines.is_empty() && fake.bucket.borrow().is_empty());
    }

    #[test]
    fn an_absent_key_answering_403_stops_before_any_write_and_says_what_to_check() {
        // Sans droit de lister, S3 répond 403 pour une clé absente : l'outil s'arrête.
        assert_stops_before_any_write(
            Some(SERVICE_ERROR_CODE),
            "An error occurred (403) when calling the HeadObject operation: Forbidden",
        );
        let (files, fake) = setup();
        *fake.absent_answer.borrow_mut() = (
            Some(SERVICE_ERROR_CODE),
            "An error occurred (403) when calling the HeadObject operation: Forbidden".to_owned(),
        );
        let error = run(&fake, &files, true).0.unwrap_err();
        assert!(
            error.contains("code 254") && error.contains("LECTURE"),
            "{error}"
        );
    }

    #[test]
    fn a_timeout_whose_text_contains_404_is_not_an_absence() {
        // Le message cite l'adresse, donc la clé, donc un numéro de version qui contient « 404 ».
        assert_stops_before_any_write(
            Some(255),
            "Connect timeout on endpoint URL: \"https://s3.fr-par.scw.cloud/voikyrioh-hearth-releases/releases/1.404.0/SHA256SUMS\"",
        );
        // Même code de service, mais pas la phrase d'un 404.
        assert_stops_before_any_write(
            Some(SERVICE_ERROR_CODE),
            "An error occurred (500) when calling the HeadObject operation: Internal Error releases/1.404.0/x not found",
        );
    }

    #[test]
    fn a_not_found_outside_a_404_is_not_an_absence() {
        assert_stops_before_any_write(Some(255), "Could not connect: host not found");
        assert_stops_before_any_write(
            Some(SERVICE_ERROR_CODE),
            "An error occurred (NoSuchBucket) when calling the HeadObject operation: The specified bucket does not exist (not found)",
        );
        // La bonne phrase mais pas le bon code de sortie : arrêt aussi.
        assert_stops_before_any_write(Some(255), ABSENT_PHRASE);
        assert_stops_before_any_write(None, ABSENT_PHRASE);
        // La phrase noyée dans une autre ligne n'est pas la phrase.
        assert_stops_before_any_write(
            Some(SERVICE_ERROR_CODE),
            &format!("proxy: {ABSENT_PHRASE} (via le mandataire)"),
        );
    }

    #[test]
    fn the_exact_404_line_is_the_only_proof_of_absence() {
        let (files, fake) = setup();
        // Précédée d'un avertissement sur une autre ligne : la ligne entière est la phrase.
        *fake.absent_answer.borrow_mut() = (
            Some(SERVICE_ERROR_CODE),
            format!("avertissement\n\n{ABSENT_PHRASE}\n"),
        );
        run(&fake, &files, true).0.unwrap();
        assert_eq!(fake.writes().len(), 10);
    }

    #[test]
    fn the_prefixed_404_line_of_a_recent_aws_cli_is_an_absence() {
        // AWS CLI 2.38.1 : la même phrase précédée de `aws: [ERROR]: `.
        let (files, fake) = setup();
        *fake.absent_answer.borrow_mut() = (
            Some(SERVICE_ERROR_CODE),
            format!(
                "
{ABSENT_PREFIX}{ABSENT_PHRASE}
"
            ),
        );
        run(&fake, &files, true).0.unwrap();
        assert_eq!(fake.writes().len(), 10);
    }

    #[test]
    fn only_the_exact_prefix_is_accepted_before_the_404_line() {
        for line in [
            format!("aws: [WARNING]: {ABSENT_PHRASE}"),
            format!("aws:[ERROR]: {ABSENT_PHRASE}"),
            format!("aws: [ERROR]:  {ABSENT_PHRASE}"),
            format!("x aws: [ERROR]: {ABSENT_PHRASE}"),
            format!("aws: [ERROR]: aws: [ERROR]: {ABSENT_PHRASE}"),
            format!("aws: [ERROR]: {ABSENT_PHRASE} (proxy)"),
            "aws: [ERROR]: An error occurred (403) when calling the HeadObject operation: Forbidden"
                .to_owned(),
        ] {
            assert_stops_before_any_write(Some(SERVICE_ERROR_CODE), &line);
        }
        assert_stops_before_any_write(Some(255), &format!("{ABSENT_PREFIX}{ABSENT_PHRASE}"));
    }

    #[test]
    fn a_read_failing_after_a_successful_head_stops_before_any_write() {
        let (files, fake) = setup();
        let taken = version_key(VERSION, AGENT_BINARY);
        fake.insert(&taken, &files[&taken]);
        *fake.get_error.borrow_mut() = Some(
            "An error occurred (AccessDenied) when calling the GetObject operation".to_owned(),
        );
        let (result, _) = run(&fake, &files, true);
        let error = result.unwrap_err();
        assert!(
            error.contains(&taken)
                && error.contains("n'a pas pu être relue")
                && error.contains("Rien n'a été écrit"),
            "{error}"
        );
        assert!(fake.writes().is_empty());
    }

    #[test]
    fn a_write_failing_in_the_middle_leaves_a_clean_state_and_the_rerun_resumes() {
        let (files, fake) = setup();
        fake.fail_put_number.set(Some(5));
        let (result, lines) = run(&fake, &files, true);
        let error = result.unwrap_err();
        assert!(
            error.contains("l'écriture de") && error.contains("Relance la même commande"),
            "{error}"
        );
        // 4 fichiers déposés, le 5e a échoué ; ni script ni manifeste, aucune relecture.
        assert_eq!(fake.writes().len(), 5);
        assert_eq!(fake.bucket.borrow().len(), 4);
        assert!(!fake.bucket.borrow().contains_key(MANIFEST_KEY));
        assert!(!fake.bucket.borrow().contains_key(SCRIPT_KEY));
        assert!(!fake.log().iter().any(|line| line.starts_with("curl ")));
        assert_eq!(lines.len(), 4, "quatre « écrit : »");

        // Relance : les 4 déjà déposés sont passés, le reste est écrit, le manifeste en dernier.
        fake.calls.borrow_mut().clear();
        let (result, lines) = run(&fake, &files, true);
        result.unwrap();
        assert_eq!(
            lines
                .iter()
                .filter(|l| l.starts_with("déjà présent"))
                .count(),
            4
        );
        let writes = fake.writes();
        assert_eq!(writes.len(), 6, "4 fichiers manquants + script + manifeste");
        assert_eq!(writes.last(), Some(&MANIFEST_KEY.to_owned()));
        assert_eq!(fake.bucket.borrow().len(), 10);
    }

    #[test]
    fn the_mutable_objects_are_read_back_from_the_bucket_and_a_wrong_one_says_what_to_put_back() {
        // Le stockage garde autre chose que ce qu'on lui donne : seule la relecture par le bucket le voit.
        let (files, fake) = setup();
        fake.store_instead
            .borrow_mut()
            .insert(MANIFEST_KEY.to_owned(), b"{\"version\":\"9.9.9\"}".to_vec());
        let (result, lines) = run(&fake, &files, true);
        let error = result.unwrap_err();
        assert!(
            error.contains(MANIFEST_KEY)
                && error.contains("n'est pas celui déposé")
                && error.contains("Premier remède : relance la même commande")
                && error.contains(&sha256_hex(&files[MANIFEST_KEY]))
                && error.contains("aws s3 cp target/release-stage/releases/manifest.json")
                && error.contains("--region fr-par")
                && error.contains("AWS_REQUEST_CHECKSUM_CALCULATION=when_required")
                && error.contains("AWS_RESPONSE_CHECKSUM_VALIDATION=when_required"),
            "{error}"
        );
        assert!(!lines.iter().any(|line| line.contains("annoncée")));

        // Le script : l'erreur le dit, et le manifeste n'est PAS écrit.
        let (files, fake) = setup();
        fake.store_instead
            .borrow_mut()
            .insert(SCRIPT_KEY.to_owned(), b"#!/bin/sh\nrm -rf /\n".to_vec());
        let error = run(&fake, &files, true).0.unwrap_err();
        assert!(
            error.contains(SCRIPT_KEY)
                && error.contains("FAUX")
                && error.contains("Premier remède")
                && error.contains("le manifeste n'est PAS écrit"),
            "{error}"
        );
        assert!(!fake.writes().contains(&MANIFEST_KEY.to_owned()));
        assert!(!fake.bucket.borrow().contains_key(MANIFEST_KEY));
    }

    #[test]
    fn the_end_of_the_run_shows_the_expected_pointer_line_and_the_three_addresses() {
        let (files, fake) = setup();
        let (result, lines) = run(&fake, &files, true);
        result.unwrap();
        let text = lines.join("\n");
        let agent_sum = sha256_hex(&files[&version_key(VERSION, AGENT_BINARY)]);
        assert!(
            text.contains(&format!(
                "0.1.0 {agent_sum} 0.1.0/hearth-agent-linux-x86_64"
            )),
            "{text}"
        );
        for address in ["latest", "latest/agent", "latest/installer"] {
            assert!(
                text.contains(&format!(
                    "https://hearth.voikyrioh.fr/api/releases/{address}"
                )),
                "{address}"
            );
        }
        assert!(text.contains("relu dans le bucket, identique : releases/manifest.json"));
        assert!(text.contains("relu dans le bucket, identique : releases/install.sh"));
    }

    #[test]
    fn when_the_public_readback_fails_the_manifest_and_the_script_are_not_written() {
        let (files, fake) = setup();
        fake.public.set(Public::Down);
        let (result, _) = run(&fake, &files, true);
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
    fn the_patience_is_also_enforced_inside_a_round_when_a_call_hangs() {
        // Chaque relecture pend jusqu'à `--max-time` (120 s) puis échoue : sans borne dans le tour, les
        // 8 fichiers feraient 16 minutes avant la première vérification.
        let (files, fake) = setup();
        fake.public.set(Public::Down);
        fake.curl_takes.set(Duration::from_secs(120));
        let (result, _) = run(&fake, &files, true);
        let error = result.unwrap_err();
        assert!(error.contains("relecture publique impossible"), "{error}");
        let curls = fake.log().iter().filter(|l| l.starts_with("curl ")).count();
        assert_eq!(curls, 3, "120 s, 240 s, 360 s : arrêt au troisième appel");
        assert_eq!(
            fake.clock.get(),
            Duration::from_secs(360),
            "aucune pause de plus"
        );
        assert!(!fake.bucket.borrow().contains_key(MANIFEST_KEY));
    }

    #[test]
    fn a_wrong_sum_on_the_public_readback_means_no_manifest() {
        let (files, fake) = setup();
        fake.public.set(Public::Corrupt);
        let (result, _) = run(&fake, &files, true);
        let error = result.unwrap_err();
        assert!(error.contains("la somme relue"), "{error}");
        assert!(!fake.bucket.borrow().contains_key(MANIFEST_KEY));
    }

    #[test]
    fn the_readback_waits_for_a_slow_cache_then_succeeds() {
        struct Slow(Fake, Cell<u32>);
        impl Host for Slow {
            fn run(&self, call: &Call) -> Result<CmdOutput, String> {
                if call.program == "curl" {
                    self.1.set(self.1.get() + 1);
                    // Les 8 premières relectures (tout le premier tour) échouent.
                    if self.1.get() <= 8 {
                        return fail(Some(22), "curl: (22) 404");
                    }
                }
                self.0.run(call)
            }
            fn elapsed(&self) -> Duration {
                self.0.elapsed()
            }
            fn sleep(&self, duration: Duration) {
                self.0.sleep(duration);
            }
        }
        let (files, fake) = setup();
        let slow = Slow(fake, Cell::new(0));
        let mut lines = Vec::new();
        publish(&slow, VERSION, &files, CONFIRM, &mut |line| {
            lines.push(line)
        })
        .unwrap();
        assert_eq!(slow.0.clock.get(), POLL, "un seul tour d'attente");
        assert!(slow.0.bucket.borrow().contains_key(MANIFEST_KEY));
    }

    #[test]
    fn a_rerun_after_a_complete_deposit_writes_only_the_two_mutable_objects() {
        let (files, fake) = setup();
        run(&fake, &files, true).0.unwrap();
        fake.calls.borrow_mut().clear();
        run(&fake, &files, true).0.unwrap();
        assert_eq!(fake.writes(), [SCRIPT_KEY, MANIFEST_KEY]);
    }

    #[test]
    fn a_manifest_in_place_announcing_a_newer_version_is_refused_before_any_write() {
        // Le client est plus récent.
        let (files, fake) = setup();
        fake.insert(MANIFEST_KEY, &manifest_of("0.2.0", Some("0.1.0")));
        let error = run(&fake, &files, true).0.unwrap_err();
        assert!(
            error.contains("REFUS")
                && error.contains("version (client) 0.2.0")
                && error.contains("--allow-rollback")
                && error.contains("Rien n'a été écrit"),
            "{error}"
        );
        assert!(fake.writes().is_empty());
        // L'agent est plus récent, le client non.
        let (files, fake) = setup();
        fake.insert(MANIFEST_KEY, &manifest_of("0.1.0", Some("0.1.1")));
        let error = run(&fake, &files, true).0.unwrap_err();
        assert!(error.contains("agent.version 0.1.1"), "{error}");
        assert!(fake.writes().is_empty());
    }

    #[test]
    fn the_explicit_rollback_option_lets_an_older_version_replace_the_manifest() {
        let (files, fake) = setup();
        fake.insert(MANIFEST_KEY, &manifest_of("0.2.0", Some("0.2.0")));
        let (result, lines) = run_with(&fake, &files, ROLLBACK);
        result.unwrap();
        assert_eq!(fake.writes().last(), Some(&MANIFEST_KEY.to_owned()));
        assert_eq!(fake.bucket.borrow()[MANIFEST_KEY], files[MANIFEST_KEY]);
        // Le retour arrière accepté laisse une ligne, avant la première écriture.
        let at = lines
            .iter()
            .position(|line| line.starts_with("RETOUR ARRIÈRE"))
            .expect("ligne RETOUR ARRIÈRE");
        assert!(
            lines[at].contains("version (client) 0.2.0 et agent.version 0.2.0")
                && lines[at].contains(&format!("remplacé par {VERSION}")),
            "{}",
            lines[at]
        );
        assert!(lines[..at].iter().all(|line| !line.starts_with("écrit")));
        // Un dépôt normal n'en laisse pas.
        let (files, fake) = setup();
        let (_, lines) = run(&fake, &files, true);
        assert!(!lines.iter().any(|line| line.contains("RETOUR ARRIÈRE")));
    }

    #[test]
    fn the_dry_run_says_when_the_rollback_option_is_armed() {
        let (files, fake) = setup();
        let (result, lines) = run_with(
            &fake,
            &files,
            Options {
                confirm: false,
                ..ROLLBACK
            },
        );
        result.unwrap();
        assert!(fake.calls.borrow().is_empty());
        assert!(lines.iter().any(|line| line.contains("ARMÉ")));
        let (_, plain) = run(&fake, &files, false);
        assert!(!plain.iter().any(|line| line.contains("ARMÉ")));
    }

    #[test]
    fn the_rollback_option_does_not_excuse_an_unreadable_manifest_in_place() {
        for placed in [
            b"pas du json".to_vec(),
            b"{}".to_vec(),
            manifest_of("bientot", None),
        ] {
            let (files, fake) = setup();
            fake.insert(MANIFEST_KEY, &placed);
            let error = run_with(&fake, &files, ROLLBACK).0.unwrap_err();
            assert!(error.contains("illisible"), "{error}");
            assert!(fake.writes().is_empty());
        }
    }

    #[test]
    fn an_equal_older_or_absent_manifest_in_place_is_accepted_and_an_unreadable_one_stops() {
        // Absent : premier dépôt (c'est le cas de tous les autres tests). Plus ancien, égal : accepté.
        for placed in [
            manifest_of("0.0.9", Some("0.0.9")),
            manifest_of("0.1.0", None),
        ] {
            let (files, fake) = setup();
            fake.insert(MANIFEST_KEY, &placed);
            run(&fake, &files, true).0.unwrap();
        }
        // Illisible : on ne sait pas ce qui est annoncé, arrêt avant toute écriture.
        for placed in [
            b"pas du json".to_vec(),
            b"{}".to_vec(),
            manifest_of("bientot", None),
        ] {
            let (files, fake) = setup();
            fake.insert(MANIFEST_KEY, &placed);
            let error = run(&fake, &files, true).0.unwrap_err();
            assert!(error.contains("illisible"), "{error}");
            assert!(fake.writes().is_empty());
        }
    }

    #[test]
    fn a_missing_file_is_an_error_and_never_a_silently_shorter_deposit() {
        let (mut files, fake) = setup();
        files.remove(MANIFEST_KEY);
        assert!(plan(VERSION, &files).unwrap_err().contains(MANIFEST_KEY));
        let (result, _) = run(&fake, &files, true);
        assert!(result.unwrap_err().contains("rien n'est déposé"));
        assert!(fake.calls.borrow().is_empty(), "aucune commande");
        let (result, _) = run(&fake, &files, false);
        assert!(result.is_err(), "l'essai à blanc le dit aussi");
        // Le compte exact : 10 écritures pour 8 noms de la liste fermée.
        let (files, _) = setup();
        assert_eq!(plan(VERSION, &files).unwrap().len(), 10);
    }

    #[test]
    fn the_flags_are_read_once_and_a_value_is_never_taken_for_a_flag() {
        let list = |items: &[&str]| items.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            parse_upload_args(&list(&[])).unwrap(),
            UploadArgs::default()
        );
        let parsed = parse_upload_args(&list(&[
            "--confirm",
            "--allow-rollback",
            "--stage-dir",
            "d",
        ]))
        .unwrap();
        assert!(parsed.confirm && parsed.allow_rollback);
        assert_eq!(parsed.stage_dir.as_deref(), Some("d"));
        // `--stage-dir --confirm` : « --confirm » est la valeur du dossier, rien n'est armé.
        let parsed = parse_upload_args(&list(&["--stage-dir", "--confirm"])).unwrap();
        assert!(!parsed.confirm);
        assert_eq!(parsed.stage_dir.as_deref(), Some("--confirm"));
        assert!(parse_upload_args(&list(&["--confirmer"])).is_err());
        assert!(parse_upload_args(&list(&["--stage-dir"])).is_err());
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

    #[test]
    fn the_upload_guard_refuses_before_any_aws_or_curl_with_and_without_confirm() {
        use crate::release_stage::fixtures::{TreeFake, tmp_dir};
        let root = tmp_dir("upload_guard");
        let stage = root.join("stage");
        let files = Fixture::new().staged();
        write_stage_dir(&stage, &files).unwrap();
        write_stage_commit(&stage, "abc1234def").unwrap();
        let stage_text = stage.to_string_lossy().into_owned();
        let flags = |confirm: bool| {
            let mut args = vec!["--stage-dir".to_owned(), stage_text.clone()];
            if confirm {
                args.push("--confirm".to_owned());
            }
            args
        };
        let mut untracked = TreeFake::new("", "abc1234def");
        untracked.untracked = "crates/hearth-agent/migrations/0005_x.sql".to_owned();
        let cases = [
            ("README.md", TreeFake::new(" M README.md", "abc1234def")),
            ("0005_x.sql", untracked),
            (
                "n'est pas celui qui a été assemblé",
                TreeFake::new("", "fff0000"),
            ),
        ];
        for confirm in [false, true] {
            for (expected, fake) in &cases {
                let error = upload_with(fake, &root, &flags(confirm), &mut |_| {}).unwrap_err();
                assert!(error.contains(expected), "{expected} : {error}");
                assert!(fake.only_git_ran(), "ni aws ni curl : {:?}", fake.programs);
            }
        }
        // Enregistrement absent : refus, sans même lancer git.
        std::fs::remove_file(stage.join(COMMIT_FILE)).unwrap();
        for confirm in [false, true] {
            let fake = TreeFake::new("", "abc1234def");
            let error = upload_with(&fake, &root, &flags(confirm), &mut |_| {}).unwrap_err();
            assert!(error.contains("COMMIT"), "{error}");
            assert!(fake.programs.borrow().is_empty());
        }
        // Tout concorde : la garde laisse passer, l'échec vient plus loin (pas de Cargo.toml ici).
        write_stage_commit(&stage, "abc1234def").unwrap();
        let fake = TreeFake::new("", "abc1234def");
        let error = upload_with(&fake, &root, &flags(false), &mut |_| {}).unwrap_err();
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            error.contains("Cargo.toml") && !error.contains("assemblé ("),
            "{error}"
        );
        assert!(fake.only_git_ran());
    }
}
