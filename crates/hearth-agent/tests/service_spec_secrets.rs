//! La description durable des services et le scellement de ses secrets (HRT-83, ADR-0044, BR-EDIT-034,
//! BR-EDIT-035), de bout en bout contre la VRAIE base SQLite, la VRAIE clé en fichier et le VRAI
//! algorithme, sans moteur.
//!
//! Le test sentinelle passe par le chemin d'initialisation du journal de PRODUCTION (`logging::
//! try_init_with`, filtre de sécurité et pont `log` compris) au niveau le plus bavard, et balaie le contenu
//! BRUT du fichier de base, de son journal d'écriture (`-wal`) et de son index partagé (`-shm`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "support/tmp.rs"]
mod tmp;

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use hearth_agent::application::ports::{Clock, KeyCheckRepo, ServiceSpecRepo};
use hearth_agent::application::secret_vault::{KeyStatus, SecretVault};
use hearth_agent::application::service_specs::{ServiceSpecs, SpecServiceError};
use hearth_agent::domain::install::SERVICES_KEY_FILE;
use hearth_agent::domain::sealed::{AnswerId, KEY_LEN, KeyProblem};
use hearth_agent::domain::secret::Secret;
use hearth_agent::domain::services::key::ServiceKey;
use hearth_agent::domain::services::spec::{
    AnswerEntry, EnvEntry, SecretsAccess, SpecBody, SpecFiche, SpecImage, SpecMount, SpecMountKind,
    SpecPort, SpecProtocol, SpecRestart, SpecSecrets, SpecSettings,
};
use hearth_agent::infrastructure::logging::{self, LogFormat};
use hearth_agent::infrastructure::sealer::RingSealerFactory;
use hearth_agent::infrastructure::services_key::FileServicesKeyStore;
use hearth_agent::infrastructure::sqlite::{Database, SqliteSpecRepo};
use sqlx::SqlitePool;
use time::OffsetDateTime;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::writer::BoxMakeWriter;

const SENTINEL_VALUE: &str = "SENTINEL-VALUE-7f3a91c2e4";
const SENTINEL_NAME: &str = "SENTINEL_NAME_STEAM_PASSWORD_55ab";
const SENTINEL_COMMAND: &str = "--token=SENTINEL-COMMAND-b0c8d1";
const SENTINEL_ANSWER: &str = "SENTINEL-ANSWER-hunter2-9e12";
/// Non secrets : doivent, eux, se retrouver dans la base (le témoin qui prouve que le balayage voit).
const WITNESS_NAME: &str = "Terraria-Witness-Name";
const WITNESS_IMAGE: &str = "ghcr.io/witness/image";

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH + time::Duration::days(20_000)
    }
}

struct Env {
    dir: tmp::TestDir,
    db: Database,
    repo: Arc<SqliteSpecRepo>,
    vault: Arc<SecretVault>,
    specs: ServiceSpecs,
}

fn build(dir: &Path, pool: &SqlitePool) -> (Arc<SqliteSpecRepo>, Arc<SecretVault>, ServiceSpecs) {
    let repo = Arc::new(SqliteSpecRepo::new(pool.clone()));
    let vault = Arc::new(SecretVault::new(
        Arc::new(FileServicesKeyStore::new(dir)),
        repo.clone() as Arc<dyn KeyCheckRepo>,
        Arc::new(RingSealerFactory),
    ));
    let specs = ServiceSpecs::new(
        repo.clone() as Arc<dyn ServiceSpecRepo>,
        vault.clone(),
        Arc::new(FixedClock),
    );
    (repo, vault, specs)
}

impl Env {
    async fn new() -> Self {
        let dir = tmp::tempdir().unwrap();
        let db = Database::open(dir.path()).await.unwrap();
        let (repo, vault, specs) = build(dir.path(), db.pool());
        Self {
            dir,
            db,
            repo,
            vault,
            specs,
        }
    }

    /// Un « redémarrage de l'agent » : nouveaux objets sur la même base et le même dossier.
    fn restarted(&self) -> (Arc<SecretVault>, ServiceSpecs) {
        let (_, vault, specs) = build(self.dir.path(), self.db.pool());
        (vault, specs)
    }

    fn key_path(&self) -> std::path::PathBuf {
        self.dir.path().join(SERVICES_KEY_FILE)
    }
}

fn key(n: u32) -> ServiceKey {
    ServiceKey::parse(&format!("h_{n:026}")).unwrap()
}

fn body(name: &str) -> SpecBody {
    SpecBody {
        name: name.to_owned(),
        image: SpecImage {
            repository: WITNESS_IMAGE.into(),
            tag: "1.4.4".into(),
            digest: None,
        },
        ports: vec![SpecPort {
            host_port: 7777,
            container_port: 7777,
            protocol: SpecProtocol::Tcp,
        }],
        mounts: vec![SpecMount {
            kind: SpecMountKind::Managed,
            source: "world".into(),
            target: "/data".into(),
            read_only: false,
        }],
        restart: SpecRestart::UnlessStopped,
        stop_grace_s: Some(30),
        stdin_open: false,
        cpu_millis: Some(500),
        memory_bytes: Some(1 << 30),
        settings: SpecSettings {
            disk_alert_bytes: Some(5 << 30),
            public_address: Some("jeu.example.org".into()),
            backup_ignored: Vec::new(),
        },
        fiche: Some(SpecFiche {
            id: "terraria".into(),
            version: 2,
            digest: "sha256:abc".into(),
        }),
    }
}

fn secrets(value: &str) -> SpecSecrets {
    SpecSecrets {
        env: vec![
            EnvEntry {
                name: Secret::from(SENTINEL_NAME),
                value: Secret::from(value),
            },
            EnvEntry {
                name: Secret::from("SECOND"),
                value: Secret::from(format!("{value}-second")),
            },
        ],
        command: Some(vec![Secret::from("run"), Secret::from(SENTINEL_COMMAND)]),
        answers: vec![AnswerEntry {
            id: AnswerId::parse("password").unwrap(),
            value: Secret::from(SENTINEL_ANSWER),
        }],
    }
}

fn opened(access: SecretsAccess) -> SpecSecrets {
    match access {
        SecretsAccess::Opened(secrets) => secrets,
        other => panic!("secrets attendus ouverts, trouvé {other:?}"),
    }
}

fn all_bytes(dir: &Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        // La base et ses compagnons : tout ce que SQLite écrit.
        if path.is_file() && name.starts_with("hearth.db") {
            bytes.extend(std::fs::read(&path).unwrap_or_default());
        }
    }
    bytes
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

/// Les traces de tout le binaire de test, captées par le MÊME chemin d'initialisation que la production,
/// au niveau le plus bavard, avec des directives qui visent le transport lui-même.
fn captured_traces() -> Arc<Mutex<Vec<u8>>> {
    #[derive(Clone)]
    struct Sink(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Sink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Sink {
        type Writer = Sink;
        fn make_writer(&'a self) -> Sink {
            self.clone()
        }
    }
    static TRACES: OnceLock<Arc<Mutex<Vec<u8>>>> = OnceLock::new();
    TRACES
        .get_or_init(|| {
            let buffer = Arc::new(Mutex::new(Vec::new()));
            logging::try_init_with(
                BoxMakeWriter::new(Sink(buffer.clone())),
                LogFormat::Json,
                false,
                EnvFilter::new("trace,sqlx=trace,sqlx::query=trace,hearth_agent=trace"),
            )
            .expect("première installation du journal de ce binaire de test");
            buffer
        })
        .clone()
}

// ---------------------------------------------------------------------------------------------
// BR-EDIT-034 : le test sentinelle
// ---------------------------------------------------------------------------------------------

/// Le balayage n'est pas aveugle : une valeur écrite EN CLAIR dans la base, ou dans une trace, est vue.
/// Sans ce canari, un balayage qui ne lirait rien passerait le test sentinelle pour de mauvaises raisons.
#[tokio::test]
async fn the_sentinel_scan_does_see_a_clear_value_in_the_database_the_journal_and_the_traces() {
    let traces = captured_traces();
    let env = Env::new().await;
    sqlx::query("INSERT INTO meta (key, value) VALUES ('canary', ?)")
        .bind("CANARY-CLEAR-VALUE-31d7")
        .execute(env.db.pool())
        .await
        .unwrap();
    assert!(contains(
        &all_bytes(env.dir.path()),
        "CANARY-CLEAR-VALUE-31d7"
    ));
    tracing::info!("CANARY-TRACE-VALUE-8a40");
    assert!(contains(&traces.lock().unwrap(), "CANARY-TRACE-VALUE-8a40"));
    env.db.pool().close().await;
}

#[tokio::test]
async fn no_secret_value_reaches_a_trace_the_database_its_journal_or_a_debug_output() {
    let traces = captured_traces();
    log::debug!(target: "hearth_witness", "witness-log-line");
    let env = Env::new().await;
    let service = key(1);

    let rev = env
        .specs
        .save(
            &service,
            None,
            body(WITNESS_NAME),
            &secrets(SENTINEL_VALUE),
            "acct1",
        )
        .await
        .unwrap();
    assert_eq!(rev, 1);
    // Une deuxième révision, pour que les anciens scellés restent aussi sous le balayage.
    env.specs
        .save(
            &service,
            Some(1),
            body(WITNESS_NAME),
            &secrets(SENTINEL_VALUE),
            "acct1",
        )
        .await
        .unwrap();

    // Relu : les mêmes champs, les mêmes secrets (et la lecture ne trace rien).
    let (revision, access) = env.specs.read(&service, None).await.unwrap().unwrap();
    assert_eq!(revision.rev, 2);
    assert_eq!(revision.body, body(WITNESS_NAME));
    let back = opened(access);
    assert_eq!(back.env[0].name.expose(), SENTINEL_NAME);
    assert_eq!(back.env[0].value.expose(), SENTINEL_VALUE);
    assert_eq!(back.command.as_ref().unwrap()[1].expose(), SENTINEL_COMMAND);
    assert_eq!(back.answers[0].value.expose(), SENTINEL_ANSWER);

    // Ce que l'agent peut afficher d'un secret descellé : rien.
    let shown = format!("{back:?} {revision:?}");
    // Le dépôt rend des scellés, jamais du clair.
    let stored = env.repo.current(&service).await.unwrap().unwrap();
    let shown = format!("{shown} {:?}", stored.sealed);

    let sentinels = [
        SENTINEL_VALUE,
        SENTINEL_NAME,
        SENTINEL_COMMAND,
        SENTINEL_ANSWER,
    ];

    // 1. Les octets BRUTS de la base, de son journal d'écriture et de son index partagé, base OUVERTE.
    let live = all_bytes(env.dir.path());
    assert!(
        contains(&live, WITNESS_NAME) && contains(&live, WITNESS_IMAGE),
        "témoin : le balayage doit voir ce qui n'est pas secret (base et journal lus)"
    );
    for sentinel in sentinels {
        assert!(
            !contains(&live, sentinel),
            "{sentinel} en clair dans la base ou son journal"
        );
        assert!(
            !contains(shown.as_bytes(), sentinel),
            "{sentinel} dans un Debug"
        );
    }
    // 2. Après un point de contrôle complet (le journal est rejoué dans le fichier de base).
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(env.db.pool())
        .await
        .unwrap();
    let checkpointed = all_bytes(env.dir.path());
    assert!(contains(&checkpointed, WITNESS_NAME));
    for sentinel in sentinels {
        assert!(
            !contains(&checkpointed, sentinel),
            "{sentinel} après point de contrôle"
        );
    }
    // 3. Le fichier de clé ne contient que la clé : aucun secret de service.
    let key_file = std::fs::read(env.key_path()).unwrap();
    assert_eq!(key_file.len(), KEY_LEN);
    // 4. Aucune trace, même au niveau le plus bavard, SQL compris.
    let logged = traces.lock().unwrap().clone();
    assert!(
        contains(&logged, "witness-log-line"),
        "témoin : le suivi doit voir passer les traces"
    );
    for sentinel in sentinels {
        assert!(!contains(&logged, sentinel), "{sentinel} dans une trace");
    }

    env.db.pool().close().await;
}

#[tokio::test]
async fn errors_never_carry_a_secret_even_when_the_key_is_unusable() {
    let traces = captured_traces();
    let env = Env::new().await;
    let service = key(2);
    env.specs
        .save(&service, None, body("A"), &secrets(SENTINEL_VALUE), "acct1")
        .await
        .unwrap();
    // La clé disparaît : les secrets sont illisibles, la fonction refuse, rien n'est dit en clair.
    std::fs::remove_file(env.key_path()).unwrap();
    let (_, restarted) = env.restarted();
    let refusal = restarted
        .save(&key(3), None, body("B"), &secrets(SENTINEL_VALUE), "acct1")
        .await
        .unwrap_err();
    let shown = format!("{refusal} {refusal:?}");
    assert!(matches!(
        refusal,
        SpecServiceError::SecretsUnreadable(KeyProblem::MissingWithSecrets)
    ));
    let (_, access) = restarted.read(&service, None).await.unwrap().unwrap();
    let shown = format!("{shown} {access:?}");
    for sentinel in [
        SENTINEL_VALUE,
        SENTINEL_NAME,
        SENTINEL_COMMAND,
        SENTINEL_ANSWER,
    ] {
        assert!(!shown.contains(sentinel), "{sentinel}");
        assert!(
            !contains(&traces.lock().unwrap(), sentinel),
            "{sentinel} dans une trace"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// BR-EDIT-035 : révisions
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_description_is_numbered_and_the_previous_revision_stays_readable() {
    let env = Env::new().await;
    let service = key(4);
    assert!(env.specs.read(&service, None).await.unwrap().is_none());

    let mut first = body("Terraria");
    first.image.tag = "1.4.3".into();
    assert_eq!(
        env.specs
            .save(&service, None, first.clone(), &secrets("v1"), "acct1")
            .await
            .unwrap(),
        1
    );
    let mut second = body("Terraria");
    second.image.tag = "1.4.4".into();
    second.memory_bytes = Some(2 << 30);
    assert_eq!(
        env.specs
            .save(&service, Some(1), second.clone(), &secrets("v2"), "acct2")
            .await
            .unwrap(),
        2
    );
    assert_eq!(env.specs.revisions(&service).await.unwrap(), [1, 2]);

    let (current, access) = env.specs.read(&service, None).await.unwrap().unwrap();
    assert!(current.current && current.rev == 2 && current.updated_by == "acct2");
    assert_eq!(current.body, second);
    assert_eq!(opened(access).env[0].value.expose(), "v2");

    let (previous, access) = env.specs.read(&service, Some(1)).await.unwrap().unwrap();
    assert!(!previous.current && previous.rev == 1 && previous.updated_by == "acct1");
    assert_eq!(previous.body, first);
    assert_eq!(opened(access).env[0].value.expose(), "v1");

    // L'ancienne révision part quand l'ancien conteneur est supprimé ; la courante reste.
    assert_eq!(env.specs.discard_before(&service, 2).await.unwrap(), 1);
    assert_eq!(env.specs.revisions(&service).await.unwrap(), [2]);
    assert!(env.specs.read(&service, Some(1)).await.unwrap().is_none());
    assert_eq!(
        opened(env.specs.read(&service, None).await.unwrap().unwrap().1).env[0]
            .value
            .expose(),
        "v2"
    );
    let sealed_left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM service_spec_secrets")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(
        sealed_left, 5,
        "les 4 scellés de la révision 2 et son manifeste seulement"
    );
    // Jamais la courante, même demandée.
    assert_eq!(env.specs.discard_before(&service, 99).await.unwrap(), 0);
    assert!(env.specs.read(&service, None).await.unwrap().is_some());
}

#[tokio::test]
async fn a_concurrent_modification_and_a_taken_name_are_refused_not_overwritten() {
    let env = Env::new().await;
    let a = key(5);
    env.specs
        .save(&a, None, body("Terraria"), &SpecSecrets::default(), "acct1")
        .await
        .unwrap();
    // Deux écrivains ont lu la révision 1 : le second perd.
    env.specs
        .save(
            &a,
            Some(1),
            body("Terraria"),
            &SpecSecrets::default(),
            "acct1",
        )
        .await
        .unwrap();
    let late = env
        .specs
        .save(
            &a,
            Some(1),
            body("Terraria"),
            &SpecSecrets::default(),
            "acct2",
        )
        .await;
    assert!(
        matches!(late, Err(SpecServiceError::Conflict { current: Some(2) })),
        "{late:?}"
    );
    // Créer une « première » révision d'un service qui existe déjà est un conflit aussi.
    let again = env
        .specs
        .save(&a, None, body("Terraria"), &SpecSecrets::default(), "acct1")
        .await;
    assert!(matches!(
        again,
        Err(SpecServiceError::Conflict { current: Some(2) })
    ));
    // Un autre service du même nom, à la casse près.
    let b = key(6);
    let taken = env
        .specs
        .save(
            &b,
            None,
            body("  TERRARIA "),
            &SpecSecrets::default(),
            "acct1",
        )
        .await;
    assert!(
        matches!(taken, Err(SpecServiceError::NameTaken)),
        "{taken:?}"
    );
    assert!(
        env.specs.read(&b, None).await.unwrap().is_none(),
        "rien d'écrit à moitié"
    );
    // Après la suppression du premier, le nom est libre.
    env.specs.forget(&a).await.unwrap();
    env.specs
        .save(&b, None, body("Terraria"), &SpecSecrets::default(), "acct1")
        .await
        .unwrap();
    // Seul un service créé par Hearth a une description.
    let adopted = ServiceKey::parse("c_abcdef").unwrap();
    let refused = env
        .specs
        .save(&adopted, None, body("X"), &SpecSecrets::default(), "acct1")
        .await;
    assert!(matches!(refused, Err(SpecServiceError::NotAHearthService)));
    // Les bornes du domaine s'appliquent avant toute écriture.
    let mut huge = body("Y");
    huge.name = "y".repeat(300);
    let invalid = env
        .specs
        .save(&key(7), None, huge, &SpecSecrets::default(), "acct1")
        .await;
    assert!(matches!(invalid, Err(SpecServiceError::Invalid(_))));
}

#[tokio::test]
async fn forgetting_a_service_erases_its_sealed_values() {
    let env = Env::new().await;
    let service = key(8);
    env.specs
        .save(&service, None, body("A"), &secrets("x"), "acct1")
        .await
        .unwrap();
    env.specs
        .save(&service, Some(1), body("A"), &secrets("y"), "acct1")
        .await
        .unwrap();
    assert_eq!(env.specs.forget(&service).await.unwrap(), 2);
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM service_spec_secrets")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(left, 0);
    assert!(env.specs.read(&service, None).await.unwrap().is_none());
}

// ---------------------------------------------------------------------------------------------
// Un scellé ne se recolle pas ailleurs
// ---------------------------------------------------------------------------------------------

async fn blob_of(
    pool: &SqlitePool,
    service: &ServiceKey,
    rev: i64,
    field: &str,
) -> (Vec<u8>, Vec<u8>) {
    sqlx::query_as("SELECT nonce, sealed FROM service_spec_secrets WHERE service_key = ? AND rev = ? AND field = ?")
        .bind(service.as_str())
        .bind(rev)
        .bind(field)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Remplace le scellé d'une ligne par un autre (l'attaquant qui écrit dans la base). Le nonce est unique en
/// base : on libère d'abord celui de la ligne source.
async fn graft(pool: &SqlitePool, from: (&ServiceKey, i64, &str), to: (&ServiceKey, i64, &str)) {
    let (nonce, sealed) = blob_of(pool, from.0, from.1, from.2).await;
    sqlx::query("DELETE FROM service_spec_secrets WHERE service_key = ? AND rev = ? AND field = ?")
        .bind(from.0.as_str())
        .bind(from.1)
        .bind(from.2)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE service_spec_secrets SET nonce = ?, sealed = ? WHERE service_key = ? AND rev = ? AND field = ?")
        .bind(nonce)
        .bind(sealed)
        .bind(to.0.as_str())
        .bind(to.1)
        .bind(to.2)
        .execute(pool)
        .await
        .unwrap();
}

fn assert_tampered(access: SecretsAccess, why: &str) {
    assert!(
        matches!(access, SecretsAccess::Tampered),
        "{why} : {access:?}"
    );
}

#[tokio::test]
async fn a_sealed_value_pasted_in_another_service_revision_or_field_does_not_open() {
    // Autre service.
    let env = Env::new().await;
    let (a, b) = (key(10), key(11));
    env.specs
        .save(&a, None, body("A"), &secrets("value-of-a"), "acct1")
        .await
        .unwrap();
    env.specs
        .save(&b, None, body("B"), &secrets("value-of-b"), "acct1")
        .await
        .unwrap();
    graft(env.db.pool(), (&a, 1, "env:0"), (&b, 1, "env:0")).await;
    assert_tampered(
        env.specs.read(&b, None).await.unwrap().unwrap().1,
        "autre service",
    );

    // Autre champ du même service.
    let env = Env::new().await;
    let a = key(12);
    env.specs
        .save(&a, None, body("A"), &secrets("value-of-a"), "acct1")
        .await
        .unwrap();
    graft(env.db.pool(), (&a, 1, "command"), (&a, 1, "env:1")).await;
    assert_tampered(
        env.specs.read(&a, None).await.unwrap().unwrap().1,
        "autre champ",
    );

    // Autre révision du même service.
    let env = Env::new().await;
    let a = key(13);
    env.specs
        .save(&a, None, body("A"), &secrets("v1"), "acct1")
        .await
        .unwrap();
    env.specs
        .save(&a, Some(1), body("A"), &secrets("v2"), "acct1")
        .await
        .unwrap();
    graft(env.db.pool(), (&a, 1, "env:0"), (&a, 2, "env:0")).await;
    assert_tampered(
        env.specs.read(&a, None).await.unwrap().unwrap().1,
        "autre révision",
    );
    // La révision 1 a perdu son champ : un rang manque, rien n'est deviné.
    assert_tampered(
        env.specs.read(&a, Some(1)).await.unwrap().unwrap().1,
        "rang manquant",
    );

    // Un octet altéré.
    let env = Env::new().await;
    let a = key(14);
    env.specs
        .save(&a, None, body("A"), &secrets("v1"), "acct1")
        .await
        .unwrap();
    let (_, mut sealed) = blob_of(env.db.pool(), &a, 1, "command").await;
    sealed[0] ^= 1;
    sqlx::query("UPDATE service_spec_secrets SET sealed = ? WHERE field = 'command'")
        .bind(sealed)
        .execute(env.db.pool())
        .await
        .unwrap();
    assert_tampered(
        env.specs.read(&a, None).await.unwrap().unwrap().1,
        "octet altéré",
    );
}

#[tokio::test]
async fn two_sealings_of_the_same_value_never_share_a_nonce_or_a_ciphertext() {
    let env = Env::new().await;
    let a = key(15);
    env.specs
        .save(&a, None, body("A"), &secrets("same"), "acct1")
        .await
        .unwrap();
    env.specs
        .save(&a, Some(1), body("A"), &secrets("same"), "acct1")
        .await
        .unwrap();
    let rows: Vec<(Vec<u8>, Vec<u8>)> =
        sqlx::query_as("SELECT nonce, sealed FROM service_spec_secrets")
            .fetch_all(env.db.pool())
            .await
            .unwrap();
    // Quatre champs et le manifeste, deux révisions : dix scellés, aucun nonce ni texte en commun.
    assert_eq!(rows.len(), 10);
    let nonces: std::collections::HashSet<_> = rows.iter().map(|(n, _)| n.clone()).collect();
    let texts: std::collections::HashSet<_> = rows.iter().map(|(_, s)| s.clone()).collect();
    assert_eq!(nonces.len(), 10);
    assert_eq!(texts.len(), 10);
    assert!(rows.iter().all(|(n, _)| n.len() == 12));
}

// ---------------------------------------------------------------------------------------------
// services.key : création, absence, mauvais fichier, mauvaise base
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn an_agent_updated_from_main_starts_without_a_key_and_without_any_description() {
    let env = Env::new().await;
    assert_eq!(env.vault.status().await.unwrap(), KeyStatus::NotNeededYet);
    assert!(env.specs.read(&key(20), None).await.unwrap().is_none());
    assert!(!env.key_path().exists(), "rien n'est créé au démarrage");
    // Une description SANS secret n'a pas besoin de clé : aucun fichier n'apparaît.
    env.specs
        .save(
            &key(20),
            None,
            body("Plain"),
            &SpecSecrets::default(),
            "acct1",
        )
        .await
        .unwrap();
    assert!(!env.key_path().exists());
    assert_eq!(env.vault.status().await.unwrap(), KeyStatus::NotNeededYet);
}

#[tokio::test]
async fn the_key_is_created_at_the_first_secret_in_a_private_file_and_never_kept_in_the_database() {
    let env = Env::new().await;
    env.specs
        .save(&key(21), None, body("A"), &secrets("v"), "acct1")
        .await
        .unwrap();
    let on_disk = std::fs::read(env.key_path()).unwrap();
    assert_eq!(on_disk.len(), KEY_LEN);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(env.key_path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
    // Les octets de la clé ne sont nulle part dans la base ; seule sa vérification (un HMAC) y est.
    let live = all_bytes(env.dir.path());
    assert!(
        !live
            .windows(KEY_LEN)
            .any(|window| window == on_disk.as_slice()),
        "la clé est dans la base"
    );
    let check: String =
        sqlx::query_scalar("SELECT value FROM meta WHERE key = 'services_key_check'")
            .fetch_one(env.db.pool())
            .await
            .unwrap();
    assert_eq!(check.len(), 64);
    // Un redémarrage relit la même clé et ouvre les secrets ; rien n'est recréé.
    let (vault, specs) = env.restarted();
    assert_eq!(vault.status().await.unwrap(), KeyStatus::Ready);
    assert_eq!(
        opened(specs.read(&key(21), None).await.unwrap().unwrap().1).env[0]
            .value
            .expose(),
        "v"
    );
    assert_eq!(std::fs::read(env.key_path()).unwrap(), on_disk);
    // Un second service avec la même clé.
    specs
        .save(&key(22), None, body("B"), &secrets("w"), "acct1")
        .await
        .unwrap();
    assert_eq!(std::fs::read(env.key_path()).unwrap(), on_disk);
}

/// Les cas où la clé ne sert pas : l'agent démarre, DIT que les secrets sont illisibles, ne détruit rien,
/// et ne génère PAS une clé par-dessus.
#[tokio::test]
async fn a_missing_wrong_or_unusable_key_makes_secrets_unreadable_and_destroys_nothing() {
    let env = Env::new().await;
    let service = key(23);
    env.specs
        .save(&service, None, body("A"), &secrets("precious"), "acct1")
        .await
        .unwrap();
    let original_key = std::fs::read(env.key_path()).unwrap();
    let rows_before = all_secret_rows(env.db.pool()).await;

    // 1. Le fichier manque (base restaurée sans sa clé).
    std::fs::remove_file(env.key_path()).unwrap();
    let (vault, specs) = env.restarted();
    assert_eq!(
        vault.status().await.unwrap(),
        KeyStatus::Unavailable(KeyProblem::MissingWithSecrets)
    );
    let (revision, access) = specs.read(&service, None).await.unwrap().unwrap();
    assert_eq!(revision.body, body("A"), "la description elle-même se lit");
    assert!(matches!(
        access,
        SecretsAccess::Unreadable(KeyProblem::MissingWithSecrets)
    ));
    let refused = specs
        .save(&service, Some(1), body("A"), &secrets("new"), "acct1")
        .await;
    assert!(matches!(
        refused,
        Err(SpecServiceError::SecretsUnreadable(
            KeyProblem::MissingWithSecrets
        ))
    ));
    assert!(
        !env.key_path().exists(),
        "aucune clé n'est régénérée par-dessus l'absence"
    );
    assert_eq!(
        all_secret_rows(env.db.pool()).await,
        rows_before,
        "aucun scellé détruit ni réécrit"
    );
    // Une description sans secret s'écrit quand même (la fonction touchée est celle des secrets seulement).
    specs
        .save(
            &key(24),
            None,
            body("Plain"),
            &SpecSecrets::default(),
            "acct1",
        )
        .await
        .unwrap();
    assert!(!env.key_path().exists());

    // 2. Une AUTRE clé (base restaurée avec la clé d'une autre installation).
    std::fs::write(env.key_path(), [0x42_u8; KEY_LEN]).unwrap();
    set_private(&env.key_path());
    let (vault, specs) = env.restarted();
    assert_eq!(
        vault.status().await.unwrap(),
        KeyStatus::Unavailable(KeyProblem::DoesNotMatchDatabase)
    );
    assert!(matches!(
        specs.read(&service, None).await.unwrap().unwrap().1,
        SecretsAccess::Unreadable(KeyProblem::DoesNotMatchDatabase)
    ));
    assert!(
        specs
            .save(&service, Some(1), body("A"), &secrets("new"), "acct1")
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read(env.key_path()).unwrap(),
        [0x42_u8; KEY_LEN],
        "le fichier n'est pas touché"
    );
    assert_eq!(all_secret_rows(env.db.pool()).await, rows_before);

    // 3. Mauvaise taille.
    for content in [&b""[..], &[1_u8; 31][..], &[1_u8; 64][..]] {
        std::fs::write(env.key_path(), content).unwrap();
        set_private(&env.key_path());
        let (vault, specs) = env.restarted();
        assert!(matches!(
            vault.status().await.unwrap(),
            KeyStatus::Unavailable(KeyProblem::WrongSize { len }) if len == content.len() as u64
        ));
        assert!(
            specs
                .save(&service, Some(1), body("A"), &secrets("new"), "acct1")
                .await
                .is_err()
        );
        assert_eq!(std::fs::read(env.key_path()).unwrap(), content);
    }

    // 4. Un dossier à la place du fichier.
    std::fs::remove_file(env.key_path()).unwrap();
    std::fs::create_dir(env.key_path()).unwrap();
    let (vault, _) = env.restarted();
    assert!(matches!(
        vault.status().await.unwrap(),
        KeyStatus::Unavailable(_)
    ));
    assert!(env.key_path().is_dir());
    std::fs::remove_dir(env.key_path()).unwrap();

    // 5. Droits ouverts (Linux).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(env.key_path(), &original_key).unwrap();
        std::fs::set_permissions(env.key_path(), std::fs::Permissions::from_mode(0o644)).unwrap();
        let (vault, specs) = env.restarted();
        assert_eq!(
            vault.status().await.unwrap(),
            KeyStatus::Unavailable(KeyProblem::TooOpen { mode: 0o644 })
        );
        assert!(matches!(
            specs.read(&service, None).await.unwrap().unwrap().1,
            SecretsAccess::Unreadable(KeyProblem::TooOpen { .. })
        ));
        assert_eq!(std::fs::read(env.key_path()).unwrap(), original_key);
        std::fs::remove_file(env.key_path()).unwrap();
    }

    // 6. L'opérateur remet la bonne clé : les secrets reviennent, sans redémarrer l'agent.
    let (vault, specs) = env.restarted();
    assert!(matches!(
        specs.read(&service, None).await.unwrap().unwrap().1,
        SecretsAccess::Unreadable(KeyProblem::MissingWithSecrets)
    ));
    std::fs::write(env.key_path(), &original_key).unwrap();
    set_private(&env.key_path());
    assert_eq!(
        opened(specs.read(&service, None).await.unwrap().unwrap().1).env[0]
            .value
            .expose(),
        "precious"
    );
    assert_eq!(vault.status().await.unwrap(), KeyStatus::Ready);
    assert_eq!(all_secret_rows(env.db.pool()).await, rows_before);
}

#[tokio::test]
async fn a_database_that_once_had_a_key_never_gets_a_new_one_in_silence() {
    let env = Env::new().await;
    let service = key(25);
    env.specs
        .save(&service, None, body("A"), &secrets("v"), "acct1")
        .await
        .unwrap();
    // Plus aucun secret gardé, mais la base se souvient qu'une clé a servi.
    env.specs.forget(&service).await.unwrap();
    std::fs::remove_file(env.key_path()).unwrap();
    let (vault, specs) = env.restarted();
    assert_eq!(
        vault.status().await.unwrap(),
        KeyStatus::Unavailable(KeyProblem::MissingWithSecrets)
    );
    assert!(
        specs
            .save(&key(26), None, body("B"), &secrets("w"), "acct1")
            .await
            .is_err()
    );
    assert!(!env.key_path().exists());
}

#[tokio::test]
async fn a_key_kept_from_before_a_database_rollback_is_adopted_when_nothing_is_sealed() {
    // Retour arrière de la base (BR-UPDATE-029) : la clé créée après sa copie reste sur le disque, la base
    // revient sans secret ni vérification. Rien ne dépend de la clé : on la garde.
    let dir = tmp::tempdir().unwrap();
    // Un dossier de données déjà peuplé doit être privé avant l'ouverture (droits 700 sous Unix).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let planted = [0x17_u8; KEY_LEN];
    std::fs::write(dir.path().join(SERVICES_KEY_FILE), planted).unwrap();
    set_private(&dir.path().join(SERVICES_KEY_FILE));
    let db = Database::open(dir.path()).await.unwrap();
    let (_, vault, specs) = build(dir.path(), db.pool());
    assert_eq!(vault.status().await.unwrap(), KeyStatus::Ready);
    specs
        .save(&key(27), None, body("A"), &secrets("v"), "acct1")
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(dir.path().join(SERVICES_KEY_FILE)).unwrap(),
        planted
    );
    let (_, vault, specs) = build(dir.path(), db.pool());
    assert_eq!(vault.status().await.unwrap(), KeyStatus::Ready);
    assert_eq!(
        opened(specs.read(&key(27), None).await.unwrap().unwrap().1).env[0]
            .value
            .expose(),
        "v"
    );
}

#[tokio::test]
async fn the_key_check_in_the_database_never_gets_replaced() {
    let env = Env::new().await;
    assert_eq!(env.repo.store_key_check("first").await.unwrap(), "first");
    assert_eq!(env.repo.store_key_check("second").await.unwrap(), "first");
    assert_eq!(
        env.repo.stored_key_check().await.unwrap().as_deref(),
        Some("first")
    );
}

fn set_private(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    #[cfg(not(unix))]
    let _ = path;
}

async fn all_secret_rows(pool: &SqlitePool) -> Vec<(String, i64, String, Vec<u8>, Vec<u8>)> {
    sqlx::query_as(
        "SELECT service_key, rev, field, nonce, sealed FROM service_spec_secrets ORDER BY service_key, rev, field",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

// ---------------------------------------------------------------------------------------------
// Une clé illisible ne fait jamais disparaître de secrets (revue HRT-83, point 2)
// ---------------------------------------------------------------------------------------------

/// Écrire une révision SANS secret alors que la courante en garde, ou purger une révision dont on ne sait
/// pas ouvrir les scellés, est refusé tant que la clé ne sert pas : clé absente, puis fausse. La bonne clé
/// remise, les secrets d'origine sont toujours ouvrables, rien n'a bougé en base.
#[tokio::test]
async fn an_unusable_key_never_lets_secrets_disappear_by_a_secretless_revision_or_a_purge() {
    let env = Env::new().await;
    let service = key(40);
    env.specs
        .save(&service, None, body("A"), &secrets("one"), "acct1")
        .await
        .unwrap();
    env.specs
        .save(&service, Some(1), body("A"), &secrets("two"), "acct1")
        .await
        .unwrap();
    let original_key = std::fs::read(env.key_path()).unwrap();
    let rows_before = all_secret_rows(env.db.pool()).await;

    for broken in [None, Some([0x42_u8; KEY_LEN])] {
        match broken {
            None => std::fs::remove_file(env.key_path()).unwrap(),
            Some(wrong) => {
                std::fs::write(env.key_path(), wrong).unwrap();
                set_private(&env.key_path());
            }
        }
        let (_, specs) = env.restarted();
        let refused = specs
            .save(
                &service,
                Some(2),
                body("A"),
                &SpecSecrets::default(),
                "acct1",
            )
            .await;
        assert!(
            matches!(refused, Err(SpecServiceError::SecretsUnreadable(_))),
            "{refused:?}"
        );
        let refused = specs.discard_before(&service, 2).await;
        assert!(
            matches!(refused, Err(SpecServiceError::SecretsUnreadable(_))),
            "{refused:?}"
        );
        assert_eq!(specs.revisions(&service).await.unwrap(), [1, 2]);
        assert_eq!(all_secret_rows(env.db.pool()).await, rows_before);
    }

    // La bonne clé remise : les secrets d'origine s'ouvrent, aux deux révisions.
    std::fs::write(env.key_path(), &original_key).unwrap();
    set_private(&env.key_path());
    let (_, specs) = env.restarted();
    for (rev, value) in [(1, "one"), (2, "two")] {
        let (_, access) = specs.read(&service, Some(rev)).await.unwrap().unwrap();
        assert_eq!(opened(access).env[0].value.expose(), value);
    }
    // Clé en main : retirer volontairement les secrets est permis, et la purge qui suit aussi.
    specs
        .save(
            &service,
            Some(2),
            body("A"),
            &SpecSecrets::default(),
            "acct1",
        )
        .await
        .unwrap();
    assert_eq!(specs.discard_before(&service, 3).await.unwrap(), 2);
}

// ---------------------------------------------------------------------------------------------
// L'ensemble des champs scellés est authentifié par le manifeste (revue HRT-83, point 3)
// ---------------------------------------------------------------------------------------------

async fn tampered(statements: &str) -> SecretsAccess {
    let env = Env::new().await;
    let service = key(41);
    env.specs
        .save(&service, None, body("A"), &secrets("v"), "acct1")
        .await
        .unwrap();
    // Le manifeste est un champ scellé de plus : trois champs de la description (env:0, env:1, command,
    // answer:password) plus lui.
    assert_eq!(all_secret_rows(env.db.pool()).await.len(), 5);
    // Quelqu'un écrit dans la base sans avoir la clé.
    for sql in statements.split(';') {
        sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
            .execute(env.db.pool())
            .await
            .unwrap();
    }
    let (_, specs) = env.restarted();
    specs.read(&service, None).await.unwrap().unwrap().1
}

#[tokio::test]
async fn removing_any_sealed_field_from_the_database_is_a_tampered_description() {
    for (what, sql) in [
        (
            "dernière variable",
            "DELETE FROM service_spec_secrets WHERE field = 'env:1'",
        ),
        (
            "commande",
            "DELETE FROM service_spec_secrets WHERE field = 'command'",
        ),
        (
            "réponse",
            "DELETE FROM service_spec_secrets WHERE field = 'answer:password'",
        ),
        (
            "manifeste",
            "DELETE FROM service_spec_secrets WHERE field = 'manifest'",
        ),
        (
            "tous les scellés sauf le manifeste",
            "DELETE FROM service_spec_secrets WHERE field <> 'manifest'",
        ),
    ] {
        assert!(
            matches!(tampered(sql).await, SecretsAccess::Tampered),
            "retrait : {what}"
        );
    }
}

#[tokio::test]
async fn adding_duplicating_or_permuting_a_sealed_field_is_a_tampered_description() {
    for (what, sql) in [
        (
            "ajout d'un scellé recopié",
            "INSERT INTO service_spec_secrets (service_key, rev, field, nonce, sealed)
             SELECT service_key, rev, 'env:2', randomblob(12), sealed FROM service_spec_secrets
             WHERE field = 'env:1'",
        ),
        (
            "duplication de la commande sous une réponse",
            "INSERT INTO service_spec_secrets (service_key, rev, field, nonce, sealed)
             SELECT service_key, rev, 'answer:copy', randomblob(12), sealed FROM service_spec_secrets
             WHERE field = 'command'",
        ),
        (
            "permutation de deux variables",
            "UPDATE service_spec_secrets SET field = 'tmp' WHERE field = 'env:0';
             UPDATE service_spec_secrets SET field = 'env:0' WHERE field = 'env:1';
             UPDATE service_spec_secrets SET field = 'env:1' WHERE field = 'tmp'",
        ),
        (
            "renommage d'un champ",
            "UPDATE service_spec_secrets SET field = 'answer:other' WHERE field = 'answer:password'",
        ),
    ] {
        assert!(
            matches!(tampered(sql).await, SecretsAccess::Tampered),
            "{what}"
        );
    }
}

/// Une révision intacte s'ouvre toujours en entier, et sa copie plus ancienne reste ouvrable.
#[tokio::test]
async fn an_untouched_revision_opens_with_every_field_and_the_manifest_is_never_a_secret() {
    let env = Env::new().await;
    let service = key(42);
    env.specs
        .save(&service, None, body("A"), &secrets("v"), "acct1")
        .await
        .unwrap();
    let (_, access) = env.specs.read(&service, None).await.unwrap().unwrap();
    let got = opened(access);
    assert_eq!((got.env.len(), got.answers.len()), (2, 1));
    assert!(got.command.is_some());
    // Une description sans secret n'a ni clé ni manifeste.
    env.specs
        .save(&key(43), None, body("B"), &SpecSecrets::default(), "acct1")
        .await
        .unwrap();
    let (_, access) = env.specs.read(&key(43), None).await.unwrap().unwrap();
    assert!(opened(access).env.is_empty());
}
