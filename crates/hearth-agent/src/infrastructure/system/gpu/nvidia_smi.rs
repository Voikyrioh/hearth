//! Cartes NVIDIA par l'outil `nvidia-smi` en sous-processus (ADR-0003 : pas de NVML dans le
//! binaire statique).
//!
//! **Un seul** sous-processus, lancé en mode continu (`-l 1` : une ligne par carte et par
//! seconde) et lu par une tâche de fond ; `sample` ne fait que relire la dernière ligne de
//! chaque carte, sans rien lancer. S'il meurt, il est relancé avec un espacement croissant (5 s à
//! 60 s). Absent de la machine (`nvidia-smi` introuvable), on ne réessaie pas : pas de carte
//! NVIDIA, ni erreur ni valeur inventée.
//!
//! Chaque valeur est indépendante : `[N/A]` ou `[Not Supported]` rend un champ absent, pas une
//! ligne perdue (BR-DASH-007, BR-DASH-008).

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::task::JoinHandle;
use tokio::time::timeout;

use super::round1;
use crate::application::ports::GpuProbe;
use crate::domain::machine::GpuIdentity;
use crate::domain::metrics::GpuReading;

/// Champs demandés, dans l'ordre où `parse_line` les lit.
const QUERY: &str = "index,name,utilization.gpu,memory.used,memory.total,temperature.gpu";

/// Une ligne plus vieille que ça n'est plus une mesure : la carte ne répond plus.
const STALE_AFTER: Duration = Duration::from_secs(3);

/// Variable d'environnement : chemin explicite de `nvidia-smi`.
pub const NVIDIA_SMI_VAR: &str = "HEARTH_NVIDIA_SMI";

/// Emplacements essayés quand `nvidia-smi` n'est pas dans le `PATH` (NixOS, installations).
const KNOWN_PATHS: &[&str] = &[
    "/run/current-system/sw/bin/nvidia-smi",
    "/usr/bin/nvidia-smi",
    "/usr/local/bin/nvidia-smi",
];

/// Aucune ligne pendant ce délai : le processus est tué et relancé.
const SILENCE: Duration = Duration::from_secs(10);

/// Délai entre deux recherches de `nvidia-smi` tant qu'il est introuvable.
const RESCAN: Duration = Duration::from_secs(60);

const RESTART_MIN: Duration = Duration::from_secs(5);
const RESTART_MAX: Duration = Duration::from_secs(60);

const MIB: u64 = 1024 * 1024;

/// Une ligne lue, champs encore en unités de `nvidia-smi` (Mio).
#[derive(Debug, Clone, PartialEq)]
pub struct SmiLine {
    pub index: u32,
    pub name: String,
    pub load_percent: Option<f32>,
    pub memory_used_mib: Option<u64>,
    pub memory_total_mib: Option<u64>,
    pub temp_c: Option<f32>,
}

impl SmiLine {
    fn reading(&self) -> GpuReading {
        GpuReading {
            name: self.name.clone(),
            load_percent: self.load_percent,
            memory_used_bytes: self.memory_used_mib.map(|mib| mib * MIB),
            memory_total_bytes: self.memory_total_mib.map(|mib| mib * MIB),
            temp_c: self.temp_c,
        }
    }
}

/// Un champ numérique : `[N/A]`, `[Not Supported]`, vide ou illisible = absent.
fn number(field: &str) -> Option<f64> {
    let field = field.trim();
    if field.starts_with('[') || field.eq_ignore_ascii_case("n/a") {
        return None;
    }
    field.parse::<f64>().ok().filter(|value| value.is_finite())
}

/// Analyse une ligne de `--format=csv,noheader,nounits`. `None` si la ligne n'en est pas une
/// (message d'erreur de l'outil, ligne vide, indice illisible). Le nom d'une carte peut
/// contenir des virgules : il est tout ce qui sépare l'indice des quatre dernières valeurs.
pub fn parse_line(line: &str) -> Option<SmiLine> {
    let fields: Vec<&str> = line.split(',').map(str::trim).collect();
    if fields.len() < 6 {
        return None;
    }
    let index = fields[0].parse::<u32>().ok()?;
    let tail = fields.len() - 4;
    let name = fields[1..tail].join(", ");
    if name.is_empty() {
        return None;
    }
    let mib = |field: &str| {
        number(field)
            .filter(|value| *value >= 0.0)
            .map(|value| value.round() as u64)
    };
    Some(SmiLine {
        index,
        name,
        load_percent: number(fields[tail]).map(|value| round1(value.clamp(0.0, 100.0) as f32)),
        memory_used_mib: mib(fields[tail + 1]),
        memory_total_mib: mib(fields[tail + 2]),
        // Hors de -50 à 150 °C, c'est un capteur défaillant, pas une température.
        temp_c: number(fields[tail + 3])
            .filter(|value| (-50.0..=150.0).contains(value))
            .map(|value| round1(value as f32)),
    })
}

/// Délai avant la prochaine relance, après `current` : il double jusqu'au plafond.
pub fn next_backoff(current: Duration) -> Duration {
    (current * 2).min(RESTART_MAX)
}

struct Entry {
    at: Instant,
    line: SmiLine,
}

#[derive(Default)]
struct State {
    /// Dernière ligne de chaque carte, par indice.
    lines: BTreeMap<u32, Entry>,
    /// Cartes déjà vues : l'identité ne dépend pas de la fraîcheur des lignes (une relance ne fait
    /// pas disparaître la carte, seules ses mesures deviennent absentes).
    known: BTreeMap<u32, GpuIdentity>,
}

#[derive(Default)]
struct Latest(Mutex<State>);

impl Latest {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn put(&self, line: SmiLine, at: Instant) {
        let mut state = self.state();
        state.known.insert(
            line.index,
            GpuIdentity {
                name: line.name.clone(),
                memory_total_bytes: line.memory_total_mib.map(|mib| mib * MIB),
            },
        );
        state.lines.insert(line.index, Entry { at, line });
    }

    /// Oublie les mesures (pas l'identité des cartes).
    fn clear(&self) {
        self.state().lines.clear();
    }

    /// Les lignes encore fraîches à `now`, par indice.
    fn fresh(&self, now: Instant) -> Vec<SmiLine> {
        self.state()
            .lines
            .values()
            .filter(|entry| now.saturating_duration_since(entry.at) <= STALE_AFTER)
            .map(|entry| entry.line.clone())
            .collect()
    }

    fn identities(&self) -> Vec<GpuIdentity> {
        self.state().known.values().cloned().collect()
    }
}

/// Où trouver `nvidia-smi`, dans l'ordre : la variable `HEARTH_NVIDIA_SMI` (chemin explicite), le
/// `PATH`, puis les emplacements connus (NixOS ne met pas `nvidia-smi` dans le `PATH` d'un
/// service). `env` lit une variable d'environnement, `exists` dit si un fichier existe : injectés
/// pour les tests.
pub fn locate(
    env: &dyn Fn(&str) -> Option<OsString>,
    exists: &dyn Fn(&Path) -> bool,
) -> Option<PathBuf> {
    if let Some(explicit) = env(NVIDIA_SMI_VAR).filter(|value| !value.is_empty()) {
        let explicit = PathBuf::from(explicit);
        if exists(&explicit) {
            return Some(explicit);
        }
    }
    if let Some(path) = env("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join("nvidia-smi");
            if exists(&candidate) {
                return Some(candidate);
            }
        }
    }
    KNOWN_PATHS
        .iter()
        .map(PathBuf::from)
        .find(|candidate| exists(candidate))
}

/// Recherche réelle, sur la machine.
fn locate_on_this_machine() -> Option<OsString> {
    locate(&|name| std::env::var_os(name), &|path| path.is_file()).map(PathBuf::into_os_string)
}

/// Comment la sonde trouve et lance son processus, et ses délais : réglables pour les tests.
pub struct Launch {
    /// Chemin du programme, `None` s'il est introuvable (la recherche est refaite plus tard).
    pub locate: Arc<dyn Fn() -> Option<OsString> + Send + Sync>,
    pub args: Vec<OsString>,
    /// Premier délai avant une relance (double jusqu'à 60 s).
    pub first_restart: Duration,
    /// Délai entre deux recherches d'un programme introuvable.
    pub rescan: Duration,
    /// Aucune ligne pendant ce délai : le processus est tué puis relancé.
    pub silence: Duration,
}

impl Launch {
    /// `nvidia-smi` en mode continu, trouvé sur la machine.
    pub fn nvidia_smi() -> Self {
        Self {
            locate: Arc::new(locate_on_this_machine),
            args: vec![
                OsString::from(format!("--query-gpu={QUERY}")),
                OsString::from("--format=csv,noheader,nounits"),
                OsString::from("-l"),
                OsString::from("1"),
            ],
            first_restart: RESTART_MIN,
            rescan: RESCAN,
            silence: SILENCE,
        }
    }
}

/// Sonde NVIDIA. Abandonner la sonde arrête le sous-processus.
pub struct NvidiaSmiProbe {
    latest: Arc<Latest>,
    task: JoinHandle<()>,
}

impl NvidiaSmiProbe {
    /// Lance `nvidia-smi` en mode continu. À appeler dans un runtime Tokio.
    pub fn start() -> Self {
        Self::start_with(Launch::nvidia_smi())
    }

    /// Lance une commande quelconque qui produit les mêmes lignes (tests).
    pub fn start_with(launch: Launch) -> Self {
        let latest = Arc::new(Latest::default());
        let task = tokio::spawn(supervise(launch, latest.clone()));
        Self { latest, task }
    }

    fn lines(&self) -> Vec<SmiLine> {
        self.latest.fresh(Instant::now())
    }
}

impl Drop for NvidiaSmiProbe {
    fn drop(&mut self) {
        // La tâche abandonnée libère le sous-processus, tué avec elle (`kill_on_drop`).
        self.task.abort();
    }
}

impl GpuProbe for NvidiaSmiProbe {
    /// Les cartes déjà vues, même pendant une relance.
    fn detect(&self) -> Vec<GpuIdentity> {
        self.latest.identities()
    }

    fn sample(&self) -> Vec<GpuReading> {
        self.lines().iter().map(SmiLine::reading).collect()
    }
}

enum Run {
    /// Il a tourné et produit des lignes avant de s'arrêter.
    Produced,
    /// Il s'est arrêté sans rien produire (pilote absent, erreur) ou n'a pas pu être lancé.
    Failed,
}

/// Cherche, lance, lit, relance : tant que la sonde vit. Introuvable : un message `info` une
/// seule fois, puis une nouvelle recherche à chaque `rescan` (un pilote peut arriver après le
/// démarrage).
async fn supervise(launch: Launch, latest: Arc<Latest>) {
    let mut delay = launch.first_restart;
    let mut warned_missing = false;
    loop {
        // La recherche parcourt le PATH et le disque : hors du runtime asynchrone.
        let locate = launch.locate.clone();
        let found = tokio::task::spawn_blocking(move || locate())
            .await
            .ok()
            .flatten();
        let Some(program) = found else {
            if !warned_missing {
                warned_missing = true;
                tracing::info!(
                    variable = NVIDIA_SMI_VAR,
                    "nvidia-smi introuvable : pas de carte NVIDIA mesurée, nouvelle recherche toutes les 60 s"
                );
            }
            tokio::time::sleep(launch.rescan).await;
            continue;
        };
        if warned_missing {
            warned_missing = false;
            tracing::info!(program = %program.to_string_lossy(), "nvidia-smi trouvé");
        }
        match run_once(&program, &launch, &latest).await {
            Run::Produced => delay = launch.first_restart,
            Run::Failed => {}
        }
        // Les dernières valeurs ne sont plus des mesures : les mesures disparaissent le temps de
        // la relance, pas l'identité des cartes.
        latest.clear();
        tracing::debug!(
            retry_in_s = delay.as_secs_f32(),
            "nvidia-smi arrêté, relance prévue"
        );
        tokio::time::sleep(delay).await;
        delay = next_backoff(delay).max(launch.first_restart);
    }
}

async fn run_once(program: &OsString, launch: &Launch, latest: &Latest) -> Run {
    let spawned = Command::new(program)
        .args(&launch.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) => {
            tracing::debug!(%error, "lancement de nvidia-smi impossible");
            return Run::Failed;
        }
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill().await;
        return Run::Failed;
    };
    let mut lines = BufReader::new(stdout).lines();
    let mut produced = false;
    loop {
        match timeout(launch.silence, lines.next_line()).await {
            Ok(Ok(Some(line))) => {
                if let Some(parsed) = parse_line(&line) {
                    latest.put(parsed, Instant::now());
                    produced = true;
                }
            }
            // Fin normale de la sortie : le processus s'est arrêté.
            Ok(Ok(None)) => break,
            // Erreur de lecture ou silence prolongé : le processus tourne peut-être encore ; on
            // le tue pour le relancer.
            Ok(Err(error)) => {
                tracing::warn!(%error, "lecture de nvidia-smi impossible, relance");
                break;
            }
            Err(_) => {
                tracing::warn!(
                    silence_s = launch.silence.as_secs_f32(),
                    "nvidia-smi muet, relance"
                );
                break;
            }
        }
    }
    // Tué s'il vit encore, puis attendu : jamais de processus zombie.
    let _ = child.kill().await;
    let _ = child.wait().await;
    if produced { Run::Produced } else { Run::Failed }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_line_is_read_and_converted_to_bytes() {
        let line = parse_line("0, NVIDIA GeForce RTX 4090, 37, 1043, 24564, 52").unwrap();
        assert_eq!(line.index, 0);
        assert_eq!(line.name, "NVIDIA GeForce RTX 4090");
        assert_eq!(line.load_percent, Some(37.0));
        assert_eq!(line.memory_used_mib, Some(1043));
        assert_eq!(line.memory_total_mib, Some(24564));
        assert_eq!(line.temp_c, Some(52.0));
        let reading = line.reading();
        assert_eq!(reading.memory_used_bytes, Some(1043 * 1024 * 1024));
        assert_eq!(reading.memory_total_bytes, Some(24564 * 1024 * 1024));
    }

    #[test]
    fn each_unavailable_field_is_absent_and_the_others_survive() {
        let line = parse_line("1, Tesla T4, [N/A], 100, 15360, [N/A]").unwrap();
        assert_eq!(line.load_percent, None);
        assert_eq!(line.temp_c, None);
        assert_eq!(line.memory_used_mib, Some(100));
        assert_eq!(line.memory_total_mib, Some(15360));

        let line = parse_line("0, Quadro, 5, [Not Supported], [Not Supported], 40").unwrap();
        assert_eq!(line.load_percent, Some(5.0));
        assert_eq!(line.memory_used_mib, None);
        assert_eq!(line.memory_total_mib, None);
        assert_eq!(line.temp_c, Some(40.0));

        let line = parse_line("0, GPU, [N/A], [N/A], [N/A], [N/A]").unwrap();
        assert_eq!(line.name, "GPU", "la carte existe même sans aucune mesure");
        assert_eq!(line.load_percent, None);
        assert_eq!(line.memory_used_mib, None);
        assert_eq!(line.temp_c, None);
    }

    #[test]
    fn a_name_with_commas_is_kept_whole() {
        let line = parse_line("2, NVIDIA RTX A6000, Ada, 10, 20, 30, 40").unwrap();
        assert_eq!(line.index, 2);
        assert_eq!(line.name, "NVIDIA RTX A6000, Ada");
        assert_eq!(line.load_percent, Some(10.0));
        assert_eq!(line.temp_c, Some(40.0));
    }

    #[test]
    fn crlf_and_padding_are_tolerated() {
        let line = parse_line("  0 ,  RTX 3060 ,  12 , 500 , 12288 , 61 \r").unwrap();
        assert_eq!(line.name, "RTX 3060");
        assert_eq!(line.temp_c, Some(61.0));
    }

    #[test]
    fn implausible_values_are_absent() {
        let line = parse_line("0, GPU, 250, -5, 100, 9000").unwrap();
        assert_eq!(line.load_percent, Some(100.0), "charge bornée à 100");
        assert_eq!(line.memory_used_mib, None);
        assert_eq!(line.temp_c, None);
        let line = parse_line("0, GPU, NaN, 1, 2, inf").unwrap();
        assert_eq!(line.load_percent, None);
        assert_eq!(line.temp_c, None);
    }

    #[test]
    fn lines_that_are_not_samples_are_ignored() {
        for text in [
            "",
            "   ",
            "NVIDIA-SMI has failed because it couldn't communicate with the NVIDIA driver.",
            "No devices were found",
            "x, GPU, 1, 2, 3, 4",
            "0, , 1, 2, 3, 4",
            "0, GPU, 1, 2, 3",
        ] {
            assert_eq!(parse_line(text), None, "{text:?}");
        }
    }

    #[test]
    fn the_restart_delay_doubles_up_to_a_ceiling() {
        let mut delay = RESTART_MIN;
        let mut seen = vec![delay.as_secs()];
        for _ in 0..6 {
            delay = next_backoff(delay);
            seen.push(delay.as_secs());
        }
        assert_eq!(seen, [5, 10, 20, 40, 60, 60, 60]);
    }

    #[test]
    fn a_card_that_stops_answering_disappears() {
        let latest = Latest::default();
        let t0 = Instant::now();
        latest.put(parse_line("0, A, 1, 2, 3, 4").unwrap(), t0);
        latest.put(
            parse_line("1, B, 1, 2, 3, 4").unwrap(),
            t0 + Duration::from_secs(2),
        );
        let names =
            |now| -> Vec<String> { latest.fresh(now).into_iter().map(|l| l.name).collect() };
        assert_eq!(names(t0 + Duration::from_secs(2)), ["A", "B"]);
        assert_eq!(
            names(t0 + Duration::from_secs(4)),
            ["B"],
            "A n'a rien dit depuis 4 s"
        );
        assert!(names(t0 + Duration::from_secs(10)).is_empty());
        latest.clear();
        assert!(names(t0).is_empty());
    }

    #[test]
    fn the_latest_line_of_a_card_replaces_the_previous_one() {
        let latest = Latest::default();
        let now = Instant::now();
        latest.put(parse_line("0, A, 10, 2, 3, 4").unwrap(), now);
        latest.put(parse_line("0, A, 20, 2, 3, 4").unwrap(), now);
        let lines = latest.fresh(now);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].load_percent, Some(20.0));
    }

    /// Attend qu'une condition devienne vraie (délai large : seul un vrai blocage échoue).
    async fn eventually(what: &str, condition: impl Fn() -> bool) {
        let started = Instant::now();
        while !condition() {
            assert!(
                started.elapsed() < Duration::from_secs(20),
                "délai dépassé : {what}"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    fn fast(
        locate: impl Fn() -> Option<OsString> + Send + Sync + 'static,
        args: Vec<OsString>,
    ) -> Launch {
        Launch {
            locate: Arc::new(locate),
            args,
            first_restart: Duration::from_millis(20),
            rescan: Duration::from_millis(10),
            silence: Duration::from_secs(60),
        }
    }

    #[test]
    fn the_program_is_looked_up_by_variable_then_path_then_known_places() {
        let join = |dir: &str| Path::new(dir).join("nvidia-smi");
        let files = [
            PathBuf::from("/explicit/smi"),
            join("/bin-a"),
            PathBuf::from("/run/current-system/sw/bin/nvidia-smi"),
        ];
        let exists = |path: &Path| files.iter().any(|file| file == path);
        let path_of = |dirs: &[&str]| std::env::join_paths(dirs).unwrap();
        let env = |explicit: Option<&'static str>, path: OsString| {
            move |name: &str| match name {
                "HEARTH_NVIDIA_SMI" => explicit.map(OsString::from),
                "PATH" => Some(path.clone()),
                _ => None,
            }
        };

        let all = env(Some("/explicit/smi"), path_of(&["/bin-a"]));
        assert_eq!(locate(&all, &exists), Some(PathBuf::from("/explicit/smi")));

        // Variable qui pointe dans le vide : on continue la recherche.
        let wrong = env(Some("/nowhere"), path_of(&["/bin-b", "/bin-a"]));
        assert_eq!(locate(&wrong, &exists), Some(join("/bin-a")));

        // Ni variable ni PATH utile (service NixOS) : emplacement connu.
        let nixos = env(None, path_of(&["/bin-c"]));
        assert_eq!(
            locate(&nixos, &exists),
            Some(PathBuf::from("/run/current-system/sw/bin/nvidia-smi"))
        );

        let nothing = |_: &Path| false;
        assert_eq!(locate(&nixos, &nothing), None);
    }

    #[test]
    fn a_card_seen_once_stays_in_the_identity_while_its_measures_go_away() {
        let latest = Latest::default();
        let now = Instant::now();
        latest.put(parse_line("0, RTX, 10, 2, 3, 4").unwrap(), now);
        latest.clear();
        assert!(latest.fresh(now).is_empty());
        let identities = latest.identities();
        assert_eq!(identities.len(), 1);
        assert_eq!(identities[0].name, "RTX");
        // Elle reste aussi quand ses lignes vieillissent.
        latest.put(parse_line("0, RTX, 10, 2, 3, 4").unwrap(), now);
        assert!(latest.fresh(now + Duration::from_secs(30)).is_empty());
        assert_eq!(latest.identities().len(), 1);
    }

    #[tokio::test]
    async fn a_missing_nvidia_smi_means_no_card_and_is_looked_for_again() {
        let searches = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let counter = searches.clone();
        let probe = NvidiaSmiProbe::start_with(fast(
            move || {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                None
            },
            vec![],
        ));
        eventually("nouvelle recherche", || {
            searches.load(std::sync::atomic::Ordering::SeqCst) >= 3
        })
        .await;
        assert!(probe.detect().is_empty());
        assert!(probe.sample().is_empty());
        assert!(!probe.task.is_finished(), "la recherche continue");
    }

    /// Un faux `nvidia-smi` : un script qui imprime deux cartes (la seconde sans température)
    /// puis s'arrête.
    #[cfg(unix)]
    fn script(marker: &Path, tail: &str) -> Vec<OsString> {
        let body = format!(
            "echo $$ >> {marker}; echo '0, GPU zero, 10, 100, 1000, 50'; echo '1, GPU un, 20, 200, 2000, [N/A]'; {tail}",
            marker = marker.display()
        );
        vec!["-c".into(), body.into()]
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_program_that_appears_later_is_found_and_feeds_the_samples() {
        let dir = crate::test_tmp::tempdir().unwrap();
        let marker = dir.path().join("runs");
        let searches = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let counter = searches.clone();
        let probe = NvidiaSmiProbe::start_with(fast(
            move || {
                // Introuvable aux deux premières recherches, puis installé.
                (counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= 2)
                    .then(|| OsString::from("sh"))
            },
            script(&marker, "sleep 30"),
        ));
        eventually("des mesures", || probe.sample().len() == 2).await;
        let readings = probe.sample();
        assert_eq!(readings[0].temp_c, Some(50.0));
        assert_eq!(readings[1].temp_c, None);
        assert_eq!(
            probe.detect()[1].memory_total_bytes,
            Some(2000 * 1024 * 1024)
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_process_that_dies_is_restarted_and_the_cards_stay_known_meanwhile() {
        let dir = crate::test_tmp::tempdir().unwrap();
        let marker = dir.path().join("runs");
        let mut launch = fast(|| Some(OsString::from("sh")), script(&marker, "sleep 0.2"));
        // Relance lente : on observe l'intervalle entre l'arrêt du processus et la relance.
        launch.first_restart = Duration::from_millis(600);
        let probe = NvidiaSmiProbe::start_with(launch);
        eventually("première mesure", || probe.sample().len() == 2).await;
        eventually("processus arrêté, mesures retirées", || {
            probe.sample().is_empty()
        })
        .await;
        // Pendant la relance : plus de mesures, mais les cartes restent dans l'identité.
        assert_eq!(probe.detect().len(), 2);
        eventually("relancé", || {
            std::fs::read_to_string(&marker).unwrap().lines().count() >= 2
        })
        .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_silent_process_is_killed_then_restarted_without_leaving_a_zombie() {
        let dir = crate::test_tmp::tempdir().unwrap();
        let marker = dir.path().join("runs");
        let mut launch = fast(|| Some(OsString::from("sh")), script(&marker, "sleep 60"));
        // Il imprime puis se tait : plus aucune ligne pendant 300 ms.
        launch.silence = Duration::from_millis(300);
        let probe = NvidiaSmiProbe::start_with(launch);
        eventually("deux lancements", || {
            std::fs::read_to_string(&marker)
                .map(|runs| runs.lines().count() >= 2)
                .unwrap_or(false)
        })
        .await;
        // Le premier processus (`sh`, pid écrit dans le marqueur) n'existe plus.
        let first_pid = std::fs::read_to_string(&marker)
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .to_owned();
        let alive = || {
            std::process::Command::new("kill")
                .args(["-0", &first_pid])
                .stderr(std::process::Stdio::null())
                .status()
                .map(|status| status.success())
                .unwrap_or(false)
        };
        eventually("processus tué et attendu", || !alive()).await;
        drop(probe);
    }
}
