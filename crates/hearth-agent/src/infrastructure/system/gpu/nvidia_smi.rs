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
use std::io::ErrorKind;
use std::process::Stdio;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::task::JoinHandle;

use super::round1;
use crate::application::ports::GpuProbe;
use crate::domain::machine::GpuIdentity;
use crate::domain::metrics::GpuReading;

/// Champs demandés, dans l'ordre où `parse_line` les lit.
const QUERY: &str = "index,name,utilization.gpu,memory.used,memory.total,temperature.gpu";

/// Une ligne plus vieille que ça n'est plus une mesure : la carte ne répond plus.
const STALE_AFTER: Duration = Duration::from_secs(3);

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

/// Dernière ligne de chaque carte, par indice.
#[derive(Default)]
struct Latest(Mutex<BTreeMap<u32, Entry>>);

impl Latest {
    fn put(&self, line: SmiLine, at: Instant) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(line.index, Entry { at, line });
    }

    fn clear(&self) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    /// Les lignes encore fraîches à `now`, par indice.
    fn fresh(&self, now: Instant) -> Vec<SmiLine> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .filter(|entry| now.saturating_duration_since(entry.at) <= STALE_AFTER)
            .map(|entry| entry.line.clone())
            .collect()
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
        let args = vec![
            OsString::from(format!("--query-gpu={QUERY}")),
            OsString::from("--format=csv,noheader,nounits"),
            OsString::from("-l"),
            OsString::from("1"),
        ];
        Self::start_command("nvidia-smi".into(), args, RESTART_MIN)
    }

    /// Lance une commande quelconque qui produit les mêmes lignes (tests).
    pub fn start_command(program: OsString, args: Vec<OsString>, first_restart: Duration) -> Self {
        let latest = Arc::new(Latest::default());
        let task = tokio::spawn(supervise(program, args, latest.clone(), first_restart));
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
    fn detect(&self) -> Vec<GpuIdentity> {
        self.lines()
            .into_iter()
            .map(|line| GpuIdentity {
                memory_total_bytes: line.memory_total_mib.map(|mib| mib * MIB),
                name: line.name,
            })
            .collect()
    }

    fn sample(&self) -> Vec<GpuReading> {
        self.lines().iter().map(SmiLine::reading).collect()
    }
}

enum Run {
    /// `nvidia-smi` n'existe pas sur cette machine : inutile de réessayer.
    NotInstalled,
    /// Il a tourné et produit des lignes avant de s'arrêter.
    Produced,
    /// Il s'est arrêté sans rien produire (pilote absent, erreur).
    Failed,
}

/// Lance, lit, relance : tant que la sonde vit.
async fn supervise(program: OsString, args: Vec<OsString>, latest: Arc<Latest>, first: Duration) {
    let mut delay = first;
    loop {
        match run_once(&program, &args, &latest).await {
            Run::NotInstalled => {
                tracing::debug!("nvidia-smi absent : pas de carte NVIDIA mesurée");
                return;
            }
            Run::Produced => delay = first,
            Run::Failed => {}
        }
        // Les dernières valeurs ne sont plus des mesures : la carte disparaît le temps de la relance.
        latest.clear();
        tracing::debug!(
            retry_in_s = delay.as_secs(),
            "nvidia-smi arrêté, relance prévue"
        );
        tokio::time::sleep(delay).await;
        delay = next_backoff(delay).max(first);
    }
}

async fn run_once(program: &OsString, args: &[OsString], latest: &Latest) -> Run {
    let spawned = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::NotFound | ErrorKind::PermissionDenied
            ) =>
        {
            return Run::NotInstalled;
        }
        Err(error) => {
            tracing::debug!(%error, "lancement de nvidia-smi impossible");
            return Run::Failed;
        }
    };
    let Some(stdout) = child.stdout.take() else {
        return Run::Failed;
    };
    let mut lines = BufReader::new(stdout).lines();
    let mut produced = false;
    while let Ok(Some(line)) = lines.next_line().await {
        if let Some(parsed) = parse_line(&line) {
            latest.put(parsed, Instant::now());
            produced = true;
        }
    }
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

    #[tokio::test]
    async fn a_missing_nvidia_smi_means_no_card_and_no_retry() {
        let probe = NvidiaSmiProbe::start_command(
            "hearth-test-no-such-program".into(),
            vec![],
            Duration::from_millis(10),
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(probe.detect().is_empty());
        assert!(probe.sample().is_empty());
        assert!(probe.task.is_finished(), "pas de relance sans l'outil");
    }

    /// Un faux `nvidia-smi` : un script qui imprime deux cartes en boucle, une fois sur deux sans
    /// mesure de température.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_running_process_feeds_the_samples_and_is_restarted_when_it_dies() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("runs");
        let script = format!(
            "echo x >> {marker}; echo '0, GPU zero, 10, 100, 1000, 50'; echo '1, GPU un, 20, 200, 2000, [N/A]'; sleep 0.2",
            marker = marker.display()
        );
        let probe = NvidiaSmiProbe::start_command(
            "sh".into(),
            vec!["-c".into(), script.into()],
            Duration::from_millis(20),
        );
        tokio::time::sleep(Duration::from_millis(150)).await;
        let readings = probe.sample();
        assert_eq!(readings.len(), 2);
        assert_eq!(readings[0].temp_c, Some(50.0));
        assert_eq!(readings[1].temp_c, None);
        assert_eq!(
            probe.detect()[1].memory_total_bytes,
            Some(2000 * 1024 * 1024)
        );
        // Le processus s'arrête de lui-même : il est relancé.
        tokio::time::sleep(Duration::from_millis(600)).await;
        let runs = std::fs::read_to_string(&marker).unwrap().lines().count();
        assert!(runs >= 2, "relancé : {runs} lancement(s)");
    }
}
