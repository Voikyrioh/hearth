//! La suite de contrat du port `ContainerEngine` : UNE suite, trois exécutants.
//!
//! - le faux moteur (`FakeEngine`, `tests/engine_fake.rs`) ;
//! - un faux démon HTTP local qui rejoue des réponses de la forme documentée de l'API
//!   (`tests/engine_fake_daemon.rs`, par `BollardEngine` sur `Target::Http`) ;
//! - un vrai moteur, en CI Linux seulement (`tests/engine_real.rs`, `--ignored`).
//!
//! C'est ce qui empêche le faux de mentir : ce que le faux promet, le vrai le tient, sinon le job
//! `engine` de la CI échoue. La suite LIT par le port (liste, inspection, événements) ; les écritures dont
//! elle a besoin pour préparer son décor (créer, démarrer, arrêter, supprimer) sont faites par l'exécutant.
//! Les journaux (HRT-66) y sont lus aussi : fin, dernières lignes, reprise par curseur, suivi, terminal,
//! ligne géante, pilote `none`, conteneur inconnu. Les trois ordres que le port écrit (HRT-64 : démarrer,
//! arrêter, redémarrer) y sont éprouvés par le port lui-même (`writes`) : déjà dans l'état, délai de grâce et
//! code 137, arrêt propre, port pris, conteneur inconnu. Les cinq que le déploiement ajoute (HRT-68 : `image_present`,
//! `pull`, `create`, `remove`, `info`) y sont éprouvés de même (`deploys`) : récupération avec progression et
//! abandon, échecs d'image typés (image inconnue, version inconnue), création depuis le plan fermé (étiquettes,
//! limites, `unless-stopped`, ports, montage, variables), nom déjà pris, port pris au démarrage, suppression.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use hearth_agent::application::ports::{ContainerEngine, EngineError, LogQuery};
use hearth_agent::domain::secret::Secret;
use hearth_agent::domain::services::logs::{
    LogLine, LogSource, RESUME_WINDOW_LINES, RankTracker, ResumeFilter, Verdict,
};
use hearth_agent::domain::services::plan::{Bind, ContainerPlan};
use hearth_agent::domain::services::spec::{SpecPort, SpecProtocol, SpecRestart};
use hearth_agent::domain::services::{
    EngineState, Observation, PortProtocol, ProbeOutcome, RunState, classify_probe,
};
use hearth_agent::domain::services::{KeyKind, MountType, ServiceKey};

/// Ce que l'exécutant fait faire à son moteur pour préparer le décor.
#[derive(Debug, Clone)]
pub struct Spec {
    pub name: String,
    /// Un port publié : (port du serveur, port du conteneur), en TCP.
    pub publish: Option<(u16, u16)>,
    /// `NOM=valeur` : une variable d'environnement SENTINELLE, qui ne doit jamais ressortir.
    pub env: Vec<String>,
    pub run: Run,
    /// Lancé avec un terminal : flux brut, une seule sortie.
    pub tty: bool,
    /// Pilote de journaux `none` : ils ne se relisent pas.
    pub log_driver_none: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Run {
    /// Tourne jusqu'à ce qu'on l'arrête, et IGNORE l'ordre d'arrêt (comme un `sleep` qui est le processus 1 d'un
    /// conteneur) : le moteur le tue à l'échéance du délai de grâce, code 137.
    Forever,
    /// Tourne jusqu'à ce qu'on l'arrête et y répond : sort de lui-même, code 0, dès l'ordre d'arrêt.
    Obedient,
    /// Sort tout de suite avec ce code.
    ExitsWith(i64),
}

#[async_trait]
pub trait Harness: Send + Sync {
    fn engine(&self) -> Arc<dyn ContainerEngine>;
    /// Crée le conteneur (sans le démarrer) et rend son identifiant complet.
    async fn create(&self, spec: &Spec) -> String;
    async fn start(&self, id: &str);
    /// Arrête un conteneur qui tourne.
    async fn stop(&self, id: &str);
    async fn remove(&self, id: &str);
    /// Le conteneur (qui tourne) écrit cette ligne sur cette sortie.
    async fn print(&self, id: &str, source: LogSource, text: &str);

    /// Une petite image qui se lance (`sleep`) : `dépôt:version`.
    fn image(&self) -> String;
    /// Retire l'image du serveur pour forcer une récupération (sans effet si elle n'y est pas).
    async fn forget_image(&self, image: &str);
    /// Une image que le registre ne connaît pas.
    fn missing_image(&self) -> String;
    /// Une version qui n'existe pas dans le dépôt de `image()`.
    fn missing_tag(&self) -> String;
    /// Un dossier du serveur qui existe, à monter dans un conteneur.
    fn scratch_dir(&self) -> String;
}

/// La variable sentinelle : ni son nom ni sa valeur ne doivent ressortir d'un `Debug`. Le nom est lu
/// (détail d'un service), la valeur seulement par `env`.
pub const SENTINEL_NAME: &str = "HEARTH_CONTRACT_SECRET";
pub const SENTINEL_VALUE: &str = "sentinel-7f3a91-do-not-leak";

async fn eventually<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..100 {
        if check().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("{what} : condition jamais atteinte en 10 s");
}

async fn find(engine: &Arc<dyn ContainerEngine>, name: &str) -> Option<Observation> {
    engine
        .list()
        .await
        .expect("liste")
        .observations
        .into_iter()
        .find(|observation| observation.name == name)
}

/// Joue la suite. `prefix` rend les noms uniques (un vrai moteur peut avoir d'autres conteneurs).
pub async fn run(harness: &dyn Harness, prefix: &str) {
    let engine = harness.engine();

    // 1. La sonde d'un moteur qui répond : Docker, version d'API prise en charge, prêt.
    let probe = engine.probe().await;
    assert!(probe.installed, "un moteur qui répond est installé");
    let ProbeOutcome::Answered(announced) = &probe.outcome else {
        panic!("le moteur doit répondre : {probe:?}");
    };
    assert!(!announced.name.is_empty() && !announced.version.is_empty());
    assert_eq!(classify_probe(true, Some(&probe)), EngineState::Ready);

    // 2. Le décor : un service qui tourne (port publié, variable sentinelle), un service jamais
    //    démarré, un service sorti avec le code 3.
    let running_name = format!("{prefix}-running");
    let created_name = format!("{prefix}-created");
    let exited_name = format!("{prefix}-exited");
    let running = harness
        .create(&Spec {
            name: running_name.clone(),
            publish: Some((18_080, 80)),
            env: vec![format!("{SENTINEL_NAME}={SENTINEL_VALUE}")],
            run: Run::Forever,
            tty: false,
            log_driver_none: false,
        })
        .await;
    harness.start(&running).await;
    let created = harness
        .create(&Spec {
            name: created_name.clone(),
            publish: None,
            env: vec![],
            run: Run::Forever,
            tty: false,
            log_driver_none: false,
        })
        .await;
    let exited = harness
        .create(&Spec {
            name: exited_name.clone(),
            publish: None,
            env: vec![],
            run: Run::ExitsWith(3),
            tty: false,
            log_driver_none: false,
        })
        .await;
    harness.start(&exited).await;
    eventually("le service sorti est arrêté", || {
        let engine = engine.clone();
        let name = exited_name.clone();
        async move {
            find(&engine, &name)
                .await
                .is_some_and(|observation| observation.run_state == RunState::Exited)
        }
    })
    .await;

    // 3. La liste contient tout, arrêtés compris, avec ce que la liste montre.
    let listed_running = find(&engine, &running_name)
        .await
        .expect("service en marche");
    assert_eq!(listed_running.id, running);
    assert_eq!(listed_running.run_state, RunState::Running);
    assert!(listed_running.started_at.is_some());
    assert!(
        listed_running
            .ports
            .iter()
            .any(|port| port.host_port == 18_080
                && port.container_port == 80
                && port.protocol == PortProtocol::Tcp),
        "port publié : {:?}",
        listed_running.ports
    );
    assert!(listed_running.image.reference.is_some(), "image nommée");
    assert_eq!(listed_running.image.short_id.len(), 12);

    let listed_created = find(&engine, &created_name)
        .await
        .expect("service jamais démarré");
    assert_eq!(listed_created.run_state, RunState::Created);
    assert_eq!(
        listed_created.started_at, None,
        "jamais démarré : pas de date"
    );

    let listed_exited = find(&engine, &exited_name).await.expect("service sorti");
    assert_eq!(listed_exited.run_state, RunState::Exited);
    assert_eq!(listed_exited.exit_code, Some(3));
    assert!(listed_exited.finished_at.is_some());

    // 4. L'inspection d'un conteneur est celle de la liste ; un inconnu n'est pas une erreur.
    assert_eq!(
        engine.inspect(&running).await.expect("inspection"),
        Some(listed_running.clone())
    );
    assert_eq!(
        engine.inspect(&"0".repeat(64)).await.expect("inspection"),
        None,
        "un conteneur inconnu rend None, pas une erreur"
    );

    // 5. Ni la valeur ni le nom de la variable sentinelle ne sortent de la lecture par `Debug`.
    let everything = format!(
        "{:?} {:?}",
        engine.list().await.expect("liste"),
        engine.inspect(&running).await.expect("inspection")
    );
    assert!(
        !everything.contains(SENTINEL_VALUE),
        "valeur de variable divulguée"
    );
    assert!(
        !everything.contains(SENTINEL_NAME),
        "nom de variable divulgué"
    );

    // 5 bis. Les NOMS des variables sont lus avec la liste (BR-SVC-008), les VALEURS seulement par
    //    `env` (BR-SVC-009), et un conteneur inconnu rend None.
    assert!(
        listed_running
            .env_names
            .iter()
            .any(|name| name.expose() == SENTINEL_NAME),
        "nom de variable lu : {:?}",
        listed_running.env_names
    );
    let entries = engine
        .env(&running)
        .await
        .expect("variables")
        .expect("le conteneur existe");
    assert_eq!(
        entries
            .iter()
            .find(|entry| entry.name.expose() == SENTINEL_NAME)
            .map(|entry| entry.value.expose()),
        Some(SENTINEL_VALUE),
        "valeur révélée"
    );
    assert!(
        engine
            .env(&"0".repeat(64))
            .await
            .expect("variables")
            .is_none(),
        "un conteneur inconnu rend None, pas une erreur"
    );
    let shown = format!("{entries:?}");
    assert!(
        !shown.contains(SENTINEL_VALUE) && !shown.contains(SENTINEL_NAME),
        "Debug ne montre rien"
    );

    // 6. Les événements : l'arrêt du service en marche est annoncé. Le flux se connecte à sa
    //    première lecture : on recommence (démarrer, arrêter) tant qu'aucun événement n'est vu.
    let mut events = engine.events();
    let mut seen = false;
    for _ in 0..5 {
        harness.stop(&running).await;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        while let Ok(Some(event)) = tokio::time::timeout_at(deadline, events.next()).await {
            let event = event.expect("événement");
            if event.container_id == running {
                seen = true;
                break;
            }
        }
        if seen {
            break;
        }
        harness.start(&running).await;
    }
    assert!(seen, "l'arrêt du service doit produire un événement");

    // 7. Conflit de lecture : un conteneur supprimé n'est plus listé et son inspection rend None.
    harness.remove(&created).await;
    assert_eq!(engine.inspect(&created).await.expect("inspection"), None);
    assert!(find(&engine, &created_name).await.is_none());

    // 8. Les journaux (HRT-66).
    logs(harness, &engine, prefix).await;

    // 9. Les ordres que le port écrit (HRT-64).
    writes(harness, &engine, prefix).await;

    // Nettoyage : plus aucun conteneur du décor n'utilise l'image, que la suite suivante retire pour la récupérer.
    harness.remove(&running).await;
    harness.remove(&exited).await;

    // 10. Récupérer, créer, supprimer (HRT-68).
    deploys(harness, &engine, prefix).await;
}

const ALL_LINES: LogQuery = LogQuery {
    follow: false,
    tail: None,
    since_ms: None,
    tty: false,
};

/// Lit un flux de journaux jusqu'à sa fin.
async fn read_logs(
    engine: &Arc<dyn ContainerEngine>,
    id: &str,
    query: LogQuery,
) -> Result<Vec<LogLine>, EngineError> {
    let mut stream = engine.logs(id, query);
    let mut lines = Vec::new();
    while let Some(item) = tokio::time::timeout(Duration::from_secs(20), stream.next())
        .await
        .expect("le flux de journaux ne finit pas")
    {
        lines.push(item?);
    }
    Ok(lines)
}

/// Relit jusqu'à ce que ces textes soient tous là (un vrai moteur écrit ses journaux de façon asynchrone).
async fn read_until(
    engine: &Arc<dyn ContainerEngine>,
    id: &str,
    query: LogQuery,
    wanted: &[&str],
) -> Vec<LogLine> {
    for _ in 0..100 {
        let lines = read_logs(engine, id, query).await.expect("journaux");
        if wanted
            .iter()
            .all(|text| lines.iter().any(|line| line.text == *text))
        {
            return lines;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("journaux : {wanted:?} jamais lus en 10 s");
}

fn texts_of<'a>(lines: &'a [LogLine], wanted: &[&str]) -> Vec<&'a str> {
    lines
        .iter()
        .map(|line| line.text.as_str())
        .filter(|text| wanted.contains(text))
        .collect()
}

/// Le contrat des journaux. Les textes portent le préfixe `contract-` : un vrai moteur n'a pas que nos lignes.
async fn logs(harness: &dyn Harness, engine: &Arc<dyn ContainerEngine>, prefix: &str) {
    let spec = |name: &str, tty: bool, none: bool| Spec {
        name: format!("{prefix}-{name}"),
        publish: None,
        env: vec![],
        run: Run::Forever,
        tty,
        log_driver_none: none,
    };
    let id = harness.create(&spec("logs", false, false)).await;
    harness.start(&id).await;
    harness.print(&id, LogSource::Out, "contract-out-1").await;
    harness.print(&id, LogSource::Err, "contract-err-1").await;
    harness
        .print(&id, LogSource::Out, "\u{1b}[31mcontract-color\u{1b}[0m")
        .await;

    // La fin : les deux sorties, le texte nettoyé, l'ordre du moteur, des heures qui ne reculent pas.
    let lines = read_until(
        engine,
        &id,
        ALL_LINES,
        &["contract-out-1", "contract-err-1", "contract-color"],
    )
    .await;
    let source_of = |text: &str| {
        lines
            .iter()
            .find(|line| line.text == text)
            .map(|l| l.source)
    };
    assert_eq!(source_of("contract-out-1"), Some(LogSource::Out));
    assert_eq!(source_of("contract-err-1"), Some(LogSource::Err));
    assert_eq!(source_of("contract-color"), Some(LogSource::Out));
    assert_eq!(
        texts_of(
            &lines,
            &["contract-out-1", "contract-err-1", "contract-color"]
        ),
        ["contract-out-1", "contract-err-1", "contract-color"]
    );
    // Le moteur horodate chaque sortie à part : l'heure ne recule pas SORTIE PAR SORTIE (deux sorties
    // mêlées peuvent s'inverser de quelques microsecondes, ce que l'agent tolère, BR-SLOG-014).
    for source in [LogSource::Out, LogSource::Err] {
        let times: Vec<i64> = lines
            .iter()
            .filter(|line| line.source == source)
            .map(|line| line.at_ms)
            .collect();
        assert!(
            times.windows(2).all(|pair| pair[0] <= pair[1]),
            "l'heure du moteur ne recule pas ({source:?})"
        );
    }
    assert!(lines.iter().all(|line| line.at_ms > 1_700_000_000_000));

    // Les N dernières lignes.
    let last = read_logs(
        engine,
        &id,
        LogQuery {
            tail: Some(1),
            ..ALL_LINES
        },
    )
    .await
    .expect("journaux");
    assert_eq!(last.len(), 1);
    assert_eq!(last[0].text, "contract-color");

    // La reprise : le curseur de la dernière ligne lue, puis seulement ce qui suit, sans doublon.
    let mut tracker = RankTracker::new();
    let cursors: Vec<_> = lines.iter().map(|line| tracker.next(line.at_ms)).collect();
    let cursor = *cursors.last().expect("au moins une ligne");
    let resume_from = lines.last().expect("au moins une ligne").at_ms;
    harness.print(&id, LogSource::Out, "contract-new-1").await;
    harness.print(&id, LogSource::Out, "contract-new-2").await;
    let mut resumed = Vec::new();
    for _ in 0..100 {
        let again = read_logs(
            engine,
            &id,
            // Même forme que `resume_query` de la production : `since` toujours borné par `tail`.
            LogQuery {
                tail: Some(RESUME_WINDOW_LINES),
                since_ms: Some(resume_from),
                ..ALL_LINES
            },
        )
        .await
        .expect("journaux");
        let (mut filter, mut ranks) = (ResumeFilter::new(cursor), RankTracker::new());
        resumed = again
            .into_iter()
            .filter(|line| filter.judge(ranks.next(line.at_ms)) == Verdict::Take)
            .map(|line| line.text)
            .collect();
        if resumed.len() >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(
        resumed,
        ["contract-new-1", "contract-new-2"],
        "ni doublon ni perte"
    );

    // Le suivi : une ligne écrite après l'ouverture arrive ; une ligne géante est coupée et comptée.
    let mut stream = engine.logs(
        &id,
        LogQuery {
            follow: true,
            tail: Some(0),
            ..ALL_LINES
        },
    );
    // Le flux se connecte à sa première lecture : on l'amorce avant que le service n'écrive.
    assert!(
        tokio::time::timeout(Duration::from_millis(500), stream.next())
            .await
            .is_err(),
        "tail=0 : aucune ancienne ligne"
    );
    harness.print(&id, LogSource::Out, "contract-live-1").await;
    harness
        .print(
            &id,
            LogSource::Out,
            &format!("contract-giant-{}", "g".repeat(30_000)),
        )
        .await;
    let mut seen_live = false;
    let mut giant = None;
    while !(seen_live && giant.is_some()) {
        let line = tokio::time::timeout(Duration::from_secs(15), stream.next())
            .await
            .expect("le suivi reçoit les lignes écrites après l'ouverture")
            .expect("le flux ne finit pas tant que le conteneur tourne")
            .expect("journaux");
        seen_live |= line.text == "contract-live-1";
        if line.text.starts_with("contract-giant-") {
            giant = Some(line);
        }
    }
    let giant = giant.expect("ligne géante");
    assert_eq!(
        giant.text.chars().count(),
        10_000,
        "coupée à 10 000 caractères"
    );
    assert_eq!(giant.cut, Some(30_015), "le total est compté");

    // Le conteneur s'arrête : le suivi se termine, et ses dernières lignes restent lisibles (service arrêté).
    harness.stop(&id).await;
    tokio::time::timeout(Duration::from_secs(20), async {
        while stream.next().await.is_some() {}
    })
    .await
    .expect("le suivi finit quand le conteneur s'arrête");
    let after = read_until(engine, &id, ALL_LINES, &["contract-live-1"]).await;
    assert!(after.iter().any(|line| line.text == "contract-new-2"));

    // Un terminal : un flux brut, les lignes sont lues sans trame.
    let tty = harness.create(&spec("logs-tty", true, false)).await;
    harness.start(&tty).await;
    harness.print(&tty, LogSource::Out, "contract-tty-1").await;
    let lines = read_until(
        engine,
        &tty,
        LogQuery {
            tty: true,
            ..ALL_LINES
        },
        &["contract-tty-1"],
    )
    .await;
    assert!(lines.iter().all(|line| line.source == LogSource::Out));

    // Un pilote `none` : les journaux ne se relisent pas.
    let silent = harness.create(&spec("logs-none", false, true)).await;
    harness.start(&silent).await;
    assert_eq!(
        read_logs(engine, &silent, ALL_LINES).await.err(),
        Some(EngineError::Unsupported)
    );

    // Un conteneur inconnu n'est pas une panne du moteur.
    assert_eq!(
        read_logs(engine, &"0".repeat(64), ALL_LINES).await.err(),
        Some(EngineError::NotFound)
    );

    harness.remove(&id).await;
    harness.remove(&tty).await;
    harness.remove(&silent).await;
}

/// Le conteneur tourne-t-il, d'après le port ?
async fn running(engine: &Arc<dyn ContainerEngine>, id: &str) -> bool {
    engine
        .inspect(id)
        .await
        .expect("inspection")
        .is_some_and(|observation| observation.run_state == RunState::Running)
}

async fn spec_for(name: String, publish: Option<(u16, u16)>, run: Run) -> Spec {
    Spec {
        name,
        publish,
        env: vec![],
        run,
        tty: false,
        log_driver_none: false,
    }
}

/// Les trois ordres du port (HRT-64, BR-LIFE-004, BR-LIFE-009, BR-LIFE-012) : démarrer, arrêter dans le délai ou
/// de force (137), redémarrer, un port pris, un conteneur inconnu. Chaque exécutant les tient pareil.
async fn writes(harness: &dyn Harness, engine: &Arc<dyn ContainerEngine>, prefix: &str) {
    // Démarrer : « en marche » vient du moteur ; déjà en marche, l'ordre réussit sans rien changer.
    let deaf = harness
        .create(&spec_for(format!("{prefix}-w-deaf"), Some((18_090, 80)), Run::Forever).await)
        .await;
    assert!(!running(engine, &deaf).await);
    engine.start(&deaf).await.expect("démarrer");
    assert!(running(engine, &deaf).await);
    engine.start(&deaf).await.expect("déjà en marche : réussit");

    // Un port que le conteneur publie est déjà pris : le démarrage échoue avec son code propre, le conteneur
    // reste arrêté (BR-LIFE-006), et le texte du moteur ne sort pas de l'erreur.
    let taken = harness
        .create(
            &spec_for(
                format!("{prefix}-w-taken"),
                Some((18_090, 80)),
                Run::Forever,
            )
            .await,
        )
        .await;
    let refused = engine.start(&taken).await.expect_err("port pris");
    assert_eq!(refused, EngineError::PortInUse);
    assert!(!format!("{refused} {refused:?}").contains("18090"));
    assert!(!running(engine, &taken).await);
    harness.remove(&taken).await;

    // Arrêter un programme qui ignore l'ordre : le moteur attend le délai de grâce, puis le tue (137).
    let began = tokio::time::Instant::now();
    engine
        .stop(&deaf, Duration::from_secs(1))
        .await
        .expect("arrêter");
    assert!(
        began.elapsed() >= Duration::from_millis(900),
        "le moteur attend le délai de grâce : {:?}",
        began.elapsed()
    );
    let stopped = engine
        .inspect(&deaf)
        .await
        .expect("inspection")
        .expect("existe");
    assert_eq!(stopped.run_state, RunState::Exited);
    assert_eq!(stopped.exit_code, Some(137), "tué à l'échéance du délai");
    // Déjà arrêté : l'ordre réussit sans rien changer.
    engine
        .stop(&deaf, Duration::from_secs(1))
        .await
        .expect("déjà arrêté : réussit");
    harness.remove(&deaf).await;

    // Un programme qui répond à l'ordre sort aussitôt, code 0, bien avant son délai de grâce.
    let polite = harness
        .create(&spec_for(format!("{prefix}-w-polite"), None, Run::Obedient).await)
        .await;
    engine.start(&polite).await.expect("démarrer");
    let before = engine
        .inspect(&polite)
        .await
        .expect("inspection")
        .expect("existe")
        .started_at;
    let began = tokio::time::Instant::now();
    engine
        .restart(&polite, Duration::from_secs(10))
        .await
        .expect("redémarrer");
    assert!(
        began.elapsed() < Duration::from_secs(8),
        "un arrêt propre n'attend pas le délai : {:?}",
        began.elapsed()
    );
    // Redémarrer : un seul ordre, le conteneur tourne de nouveau, d'une exécution neuve.
    let restarted = engine
        .inspect(&polite)
        .await
        .expect("inspection")
        .expect("existe");
    assert_eq!(restarted.run_state, RunState::Running);
    assert!(restarted.started_at > before, "une exécution neuve");
    // Le démarrage garde la fin de l'exécution arrêtée : c'est elle qui date l'arrêt d'un redémarrage
    // (BR-LIFE-009, le code de sortie étant remis à zéro).
    let finished = restarted
        .finished_at
        .expect("la fin de l'exécution arrêtée reste lisible après le redémarrage");
    assert!(
        before.is_some_and(|start| finished >= start)
            && restarted.started_at.is_some_and(|start| finished <= start),
        "fin entre l'ancienne et la nouvelle exécution : {before:?} {finished:?} {:?}",
        restarted.started_at
    );
    let began = tokio::time::Instant::now();
    engine
        .stop(&polite, Duration::from_secs(10))
        .await
        .expect("arrêter");
    assert!(began.elapsed() < Duration::from_secs(8));
    let stopped = engine
        .inspect(&polite)
        .await
        .expect("inspection")
        .expect("existe");
    assert_eq!(stopped.run_state, RunState::Exited);
    assert_eq!(stopped.exit_code, Some(0), "sorti de lui-même");
    harness.remove(&polite).await;

    // Un conteneur qui n'existe pas (ou plus) : « introuvable », pour les trois ordres.
    let nowhere = "f".repeat(64);
    assert_eq!(engine.start(&nowhere).await, Err(EngineError::NotFound));
    assert_eq!(
        engine.stop(&nowhere, Duration::from_secs(1)).await,
        Err(EngineError::NotFound)
    );
    assert_eq!(
        engine.restart(&nowhere, Duration::from_secs(1)).await,
        Err(EngineError::NotFound)
    );
}

/// Un plan de création du contrat : `sleep 300`, une variable sentinelle, un port, un dossier monté, des limites.
fn plan_for(
    harness: &dyn Harness,
    name: &str,
    image: &str,
    host_port: u16,
    key: &ServiceKey,
) -> ContainerPlan {
    let ulid = key.as_str().strip_prefix("h_").unwrap().to_owned();
    ContainerPlan {
        name: name.to_owned(),
        image: image.to_owned(),
        labels: [
            ("io.hearth.managed", "true"),
            ("io.hearth.service-id", ulid.as_str()),
            ("io.hearth.install-id", "contract"),
            ("io.hearth.created-by", "contract"),
            ("io.hearth.spec-rev", "1"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect(),
        env: vec![(Secret::from(SENTINEL_NAME), Secret::from(SENTINEL_VALUE))],
        command: Some(vec![Secret::from("sleep"), Secret::from("300")]),
        ports: vec![SpecPort {
            host_port,
            container_port: 80,
            protocol: SpecProtocol::Tcp,
        }],
        binds: vec![Bind {
            host_path: harness.scratch_dir(),
            target: "/data".to_owned(),
            read_only: false,
        }],
        restart: SpecRestart::UnlessStopped,
        cpu_millis: 500,
        memory_bytes: 64 * 1024 * 1024,
        stdin_open: false,
        stop_grace_s: None,
    }
}

/// Récupérer une image avec sa progression, ses échecs typés et son abandon ; créer un conteneur depuis le plan
/// fermé ; le nom pris, le port pris, la suppression, l'image qui manque (HRT-68, BR-DEPLOY-017, BR-DEPLOY-019).
async fn deploys(harness: &dyn Harness, engine: &Arc<dyn ContainerEngine>, prefix: &str) {
    // Les bornes de la machine : lues sur la machine.
    let info = engine.info().await.expect("info");
    assert!(info.cpus >= 1 && info.memory_bytes > 0, "{info:?}");
    assert!(
        info.storage_dir
            .as_deref()
            .is_some_and(|dir| dir.starts_with('/'))
    );

    let image = harness.image();
    // Récupérer : la progression ne recule jamais, le flux finit, l'image est là ensuite.
    harness.forget_image(&image).await;
    assert!(!engine.image_present(&image).await.expect("présence"));
    let mut pull = engine.pull(&image);
    let mut percents = Vec::new();
    while let Some(item) = pull.next().await {
        let progress = item.expect("récupération");
        if let Some(percent) = progress.percent {
            percents.push(percent);
        }
    }
    assert!(
        percents.windows(2).all(|pair| pair[0] <= pair[1]),
        "la progression ne recule jamais : {percents:?}"
    );
    assert!(engine.image_present(&image).await.expect("présence"));

    // Abandonner une récupération : la connexion se ferme, rien n'est créé, l'image se récupère ensuite.
    harness.forget_image(&image).await;
    let mut abandoned = engine.pull(&image);
    let _ = tokio::time::timeout(Duration::from_secs(5), abandoned.next()).await;
    drop(abandoned);
    let mut again = engine.pull(&image);
    while let Some(item) = again.next().await {
        item.expect("récupération après un abandon");
    }
    assert!(engine.image_present(&image).await.expect("présence"));

    // Les échecs sont typés, jamais le texte du moteur.
    // Un registre public peut limiter le débit (ce n'est pas un défaut testé) : on réessaie un peu plus tard.
    let first_failure = |reference: String, what: &'static str| {
        let engine = engine.clone();
        async move {
            let mut failure = None;
            for attempt in 0..6 {
                let mut pull = engine.pull(&reference);
                let found = pull.next().await.expect("un résultat").expect_err(what);
                if found != EngineError::RegistryRateLimited || attempt == 5 {
                    failure = Some(found);
                    break;
                }
                tokio::time::sleep(Duration::from_secs(20)).await;
            }
            failure.expect("un échec")
        }
    };
    let failure = first_failure(harness.missing_image(), "image inconnue").await;
    assert!(
        matches!(
            failure,
            EngineError::ImageNotFound
                | EngineError::ImagePrivateOrMissing
                | EngineError::TagNotFound
        ),
        "image inconnue : {failure:?}"
    );
    assert!(!format!("{failure} {failure:?}").contains(&harness.missing_image()));
    let failure = first_failure(harness.missing_tag(), "version inconnue").await;
    assert!(
        matches!(
            failure,
            EngineError::TagNotFound
                | EngineError::ImageNotFound
                | EngineError::ImagePrivateOrMissing
        ),
        "version inconnue : {failure:?}"
    );

    // Créer depuis le plan fermé : ni démarré, ni sans étiquettes, ni sans limites.
    let key = ServiceKey::for_hearth("01JZ0000000000000000000001").expect("clé");
    assert_eq!(key.kind(), KeyKind::Hearth);
    let name = format!("{prefix}-d-web");
    let id = engine
        .create(&plan_for(harness, &name, &image, 18_091, &key))
        .await
        .expect("créer");
    let created = engine
        .inspect(&id)
        .await
        .expect("inspection")
        .expect("existe");
    assert_eq!(created.run_state, RunState::Created, "créé, pas démarré");
    assert_eq!(created.name, name);
    assert_eq!(
        created.hearth_key.as_ref(),
        Some(&key),
        "étiquette du service"
    );
    assert_eq!(
        created.restart,
        hearth_agent::domain::services::RestartPolicy::UnlessStopped
    );
    assert_eq!(created.memory_limit_bytes, Some(64 * 1024 * 1024));
    assert_eq!(created.cpu_millis, Some(500));
    assert!(
        created.ports.iter().any(|port| port.host_port == 18_091
            && port.container_port == 80
            && port.protocol == PortProtocol::Tcp),
        "{:?}",
        created.ports
    );
    assert!(
        created
            .mounts
            .iter()
            .any(|mount| mount.kind == MountType::Folder
                && mount.target == "/data"
                && !mount.read_only),
        "{:?}",
        created.mounts
    );
    assert!(!created.env_names.is_empty());
    // Le moteur a reçu la variable et sa valeur : seul lui les a.
    let env = engine.env(&id).await.expect("variables").expect("existe");
    assert!(
        env.iter()
            .any(|entry| entry.value.expose() == SENTINEL_VALUE)
    );
    assert!(created.command.as_ref().is_some_and(|c| !c.is_empty()));

    // Le nom est pris : un second conteneur du même nom est refusé, sans texte du moteur.
    let other = ServiceKey::for_hearth("01JZ0000000000000000000002").expect("clé");
    let taken = engine
        .create(&plan_for(harness, &name, &image, 18_092, &other))
        .await
        .expect_err("nom pris");
    assert_eq!(taken, EngineError::NameTaken);
    assert!(!format!("{taken} {taken:?}").contains(&name));

    // L'image absente : « introuvable », jamais créé.
    let mut no_image = plan_for(
        harness,
        &format!("{prefix}-d-noimage"),
        &image,
        18_093,
        &other,
    );
    no_image.image = format!(
        "{}-never-pulled:1",
        harness.missing_image().split(':').next().unwrap()
    );
    assert_eq!(engine.create(&no_image).await, Err(EngineError::NotFound));

    // Démarrer, puis un second conteneur sur le même port : refusé au démarrage, supprimé ensuite.
    engine.start(&id).await.expect("démarrer");
    assert!(running(engine, &id).await);
    let rival_name = format!("{prefix}-d-rival");
    let rival = engine
        .create(&plan_for(harness, &rival_name, &image, 18_091, &other))
        .await
        .expect("créer le rival");
    assert_eq!(engine.start(&rival).await, Err(EngineError::PortInUse));
    assert!(!running(engine, &rival).await);
    engine.remove(&rival).await.expect("supprimer le rival");
    assert!(engine.inspect(&rival).await.expect("inspection").is_none());
    // Supprimer ce qui n'existe plus réussit : le but est atteint.
    engine.remove(&rival).await.expect("déjà supprimé");

    // Supprimer un conteneur qui tourne (pas de conteneur orphelin).
    engine.remove(&id).await.expect("supprimer");
    assert!(find(engine, &name).await.is_none());
}
