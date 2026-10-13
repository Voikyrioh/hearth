//! Journaux d'un service : règles pures (HRT-66, ADR-0038, BR-SLOG-007, 008, 009, 012 à 014, 019, 021).
//!
//! Aucune E/S ici. Le moteur envoie des morceaux d'octets ; ce module en fait des lignes bornées et
//! nettoyées ([`LineAssembler`]), leur donne un curseur de reprise ([`RankTracker`], [`ResumeFilter`]),
//! les range dans un anneau borné qui perd les plus anciennes sans jamais bloquer personne ([`LogRing`])
//! et décide quand un lot part ([`flush_due`]).
//!
//! Le texte d'une ligne est du contenu sensible : il ne figure dans AUCUN `Debug` de ce module, et rien
//! ici ne le garde plus longtemps que l'anneau d'une lecture (BR-SLOG-019, BR-SLOG-021).

use std::collections::VecDeque;
use std::fmt;
use std::time::Duration;

pub use hearth_proto::api::service_logs::LogSource;
use hearth_proto::api::service_logs::{
    BATCH_BYTES, BATCH_LINES, BATCH_WINDOW_MS, GapReason, LogCursor, LogGap, LogMarker,
    LogsErrorCode, LogsStatus, RING_BYTES, RING_LINES,
};
use hearth_proto::log_text;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::state::{Observation, RunState};
use crate::domain::accounts::{AccountId, Role, ServicesRight};

/// Octets bruts gardés pour UNE ligne en cours d'assemblage, par sortie. Au-delà, les octets sont
/// comptés et jetés : une ligne sans fin ne fait jamais grossir la mémoire (BR-SLOG-013). Large devant
/// `MAX_LINE_CHARS` (10 000 caractères, jusqu'à 40 000 octets) : une ligne pleine de codes de couleur
/// garde de quoi remplir ses 10 000 caractères visibles.
pub const RAW_LINE_CAP: usize = 64 * 1024;

/// Poids ajouté au texte d'une ligne dans l'anneau (la ligne, son heure, son curseur).
const LINE_OVERHEAD: usize = 48;

/// Un curseur de reprise plus vieux que cela n'est pas repris : l'agent répond par `gap { resume_lost }`
/// puis les 1 000 dernières lignes, comme pour une ligne de reprise disparue (BR-SLOG-014). 5 minutes :
/// de quoi couvrir la coupure attendue d'une mise à jour de l'agent (2 minutes) et un basculement de
/// réseau, pas une mise en veille. Sans cette borne, un curseur forgé à zéro ferait relire au moteur tout
/// le journal d'un conteneur, à la demande d'un compte en lecture seule, dix fois de front.
pub const RESUME_MAX_AGE_MS: i64 = 5 * 60 * 1_000;

/// Lignes que le moteur peut rendre à l'agent pour atteindre la ligne d'un curseur de reprise (celles
/// d'avant, sautées). Au-delà, la ligne du curseur est tenue pour perdue : 4 anneaux. Rien n'est encore
/// envoyé à ce stade, donc rien ne se répète au client. Le rang d'un curseur plus grand que cette borne ne
/// peut pas être celui d'une ligne que l'agent a rendue : le curseur est tenu pour illisible.
pub const RESUME_MAX_LINES: usize = 4 * RING_LINES;

/// Messages que le moteur rend au plus pour TOUTE requête qui porte `since` (reprise, relecture après le
/// curseur) : de quoi sauter jusqu'à [`RESUME_MAX_LINES`] lignes puis remplir un anneau. Sans ce `tail`, le
/// moteur décode tout le journal qu'il garde à chaque ouverture. Une fenêtre pleine qui commence dans la
/// milliseconde du curseur est une reprise perdue : les rangs y seraient sous-estimés et des lignes non
/// reçues seraient sautées sans trou (BR-SLOG-014).
pub const RESUME_WINDOW_LINES: u32 = 25_000;

/// Écart toléré entre l'heure de l'agent et celle d'un curseur, ou entre deux lignes du moteur : au-delà
/// vers l'avant, le curseur est « dans le futur » (forgé, ou horloge reculée) ; au-delà vers l'arrière
/// entre deux lignes, l'horloge a reculé (BR-SLOG-014, limite connue).
pub const CLOCK_SKEW_MS: i64 = 1_000;

/// Lignes d'un conteneur lu depuis son début (conteneur recréé) : jamais « tout », au plus un anneau.
pub const FROM_START_LINES: u32 = 5_000;

/// Plafond des états et repères en attente dans un anneau (ils ne sont jamais évincés par le débit de
/// lignes, mais ils sont bornés : le plus ancien état, remplacé par les suivants, part le premier).
const MAX_RING_EVENTS: usize = 256;

/// Une ligne telle que le moteur la donne, déjà assemblée, nettoyée et bornée.
#[derive(Clone, PartialEq, Eq)]
pub struct LogLine {
    /// Heure du moteur, millisecondes depuis l'époque.
    pub at_ms: i64,
    pub source: LogSource,
    pub text: String,
    /// Si la ligne a été coupée : son nombre total de caractères.
    pub cut: Option<u32>,
}

impl LogLine {
    /// Ce que cette ligne pèse dans l'anneau et dans un lot.
    pub fn weight(&self) -> usize {
        self.text.len() + LINE_OVERHEAD
    }
}

impl fmt::Debug for LogLine {
    /// Le texte n'est jamais montré : sa longueur seulement.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LogLine")
            .field("at_ms", &self.at_ms)
            .field("source", &self.source)
            .field("chars", &self.text.chars().count())
            .field("cut", &self.cut)
            .finish()
    }
}

// ---------------------------------------------------------------------------------------------
// Assemblage des lignes
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
struct Pending {
    buf: Vec<u8>,
    /// Octets au-delà de `RAW_LINE_CAP`, comptés et jetés.
    dropped: usize,
    /// Heure du premier morceau de la ligne.
    at_ms: Option<i64>,
    /// Le dernier octet était un retour chariot : un `\n` qui suit n'est pas une deuxième fin de ligne.
    after_cr: bool,
}

/// Fabrique des lignes depuis des morceaux d'octets, une sortie à la fois. Une ligne se termine par
/// `\n`, par `\r\n` ou par un `\r` seul (une barre de progression qui réécrit sa ligne donne une suite
/// de lignes, BR-SLOG-013). Un octet invalide devient « � ». Le texte est nettoyé
/// ([`log_text::clean`]) puis coupé à `MAX_LINE_CHARS` ; ce qui dépasse est compté, jamais gardé.
#[derive(Default)]
pub struct LineAssembler {
    out: Pending,
    err: Pending,
}

impl LineAssembler {
    pub fn new() -> Self {
        Self::default()
    }

    fn pending(&mut self, source: LogSource) -> &mut Pending {
        match source {
            // `Unknown` n'est jamais produit ici : il suit la sortie normale.
            LogSource::Out | LogSource::Unknown => &mut self.out,
            LogSource::Err => &mut self.err,
        }
    }

    /// Ajoute des octets de cette sortie, reçus à cette heure ; les lignes terminées sont ajoutées à `lines`.
    pub fn push(&mut self, source: LogSource, at_ms: i64, data: &[u8], lines: &mut Vec<LogLine>) {
        let pending = self.pending(source);
        for &byte in data {
            if pending.after_cr {
                pending.after_cr = false;
                if byte == b'\n' {
                    continue;
                }
            }
            match byte {
                b'\n' => emit(pending, source, at_ms, lines),
                b'\r' => {
                    emit(pending, source, at_ms, lines);
                    pending.after_cr = true;
                }
                _ => {
                    pending.at_ms.get_or_insert(at_ms);
                    if pending.buf.len() < RAW_LINE_CAP {
                        pending.buf.push(byte);
                    } else if byte & 0xC0 != 0x80 {
                        // Les caractères sont comptés, pas les octets : un octet de continuation UTF-8
                        // n'est pas un caractère de plus (`cut`, BR-SLOG-013).
                        pending.dropped = pending.dropped.saturating_add(1);
                    }
                }
            }
        }
    }

    /// La fin du flux : une dernière ligne sans fin de ligne part telle quelle.
    pub fn finish(&mut self, lines: &mut Vec<LogLine>) {
        for source in [LogSource::Out, LogSource::Err] {
            let pending = self.pending(source);
            if !pending.buf.is_empty() || pending.dropped > 0 || pending.at_ms.is_some() {
                let at = pending.at_ms.unwrap_or_default();
                emit(pending, source, at, lines);
            }
        }
    }
}

fn emit(pending: &mut Pending, source: LogSource, fallback_ms: i64, lines: &mut Vec<LogLine>) {
    let at_ms = pending.at_ms.take().unwrap_or(fallback_ms);
    let cleaned = log_text::clean(&String::from_utf8_lossy(&pending.buf));
    let (text, cut) = log_text::bound(&cleaned, pending.dropped);
    pending.buf.clear();
    pending.dropped = 0;
    lines.push(LogLine {
        at_ms,
        source,
        text,
        cut,
    });
}

/// L'horodatage que le moteur écrit devant chaque message (`timestamps=1` :
/// `2026-10-13T10:00:00.123456789Z `). Rend l'heure en millisecondes et la longueur à sauter. `None` si
/// `data` ne commence pas par un horodatage de cette forme : le morceau est alors du texte (la
/// suite d'une ligne coupée par le moteur n'en porte pas toujours).
pub fn split_timestamp(data: &[u8]) -> Option<(i64, usize)> {
    let space = data.iter().take(40).position(|byte| *byte == b' ')?;
    let text = std::str::from_utf8(&data[..space]).ok()?;
    if text.len() < 20 || !text.ends_with('Z') {
        return None;
    }
    let parsed = OffsetDateTime::parse(text, &Rfc3339).ok()?;
    let ms = i64::try_from(parsed.unix_timestamp_nanos() / 1_000_000).ok()?;
    Some((ms, space + 1))
}

// ---------------------------------------------------------------------------------------------
// Curseur de reprise
// ---------------------------------------------------------------------------------------------

/// Donne à chaque ligne son curseur : son heure (jamais en arrière de la précédente) et son rang parmi
/// les lignes de la même milliseconde, dans l'ordre du moteur (BR-SLOG-014).
#[derive(Debug, Default)]
pub struct RankTracker {
    last: Option<(i64, u32)>,
    rewound: bool,
}

impl RankTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// L'heure du moteur a reculé de plus de [`CLOCK_SKEW_MS`] depuis la plus haute vue : les curseurs
    /// suivants sont ramenés à cette heure haute, donc une relecture `since` ne les retrouverait pas
    /// (BR-SLOG-014). L'appelant ne reprend alors pas par le curseur.
    pub fn rewound(&self) -> bool {
        self.rewound
    }

    pub fn next(&mut self, at_ms: i64) -> LogCursor {
        if let Some((last, _)) = self.last
            && at_ms < last.saturating_sub(CLOCK_SKEW_MS)
        {
            self.rewound = true;
        }
        let ts = match self.last {
            Some((last, _)) if at_ms < last => last,
            _ => at_ms,
        };
        let rank = match self.last {
            Some((last, rank)) if last == ts => rank.saturating_add(1),
            _ => 0,
        };
        self.last = Some((ts, rank));
        LogCursor { ts, rank }
    }
}

/// Ce qu'une relecture fait d'une ligne.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Déjà reçue (avant le curseur, ou le curseur lui-même) : sautée.
    Skip,
    /// Nouvelle : envoyée.
    Take,
    /// La ligne du curseur n'existe plus (le moteur a fait tourner ses fichiers) : on ne sait pas ce qui
    /// manque, la lecture repart des dernières lignes (BR-SLOG-014).
    Lost,
}

/// Saute ce que le client a déjà reçu : toutes les lignes jusqu'à celle du curseur comprise. Ni doublon
/// ni perte tant que cette ligne existe.
#[derive(Debug)]
pub struct ResumeFilter {
    target: LogCursor,
    passed: bool,
}

impl ResumeFilter {
    pub fn new(target: LogCursor) -> Self {
        Self {
            target,
            passed: false,
        }
    }

    /// Juge la ligne de ce curseur (calculé sur la relecture, `RankTracker` neuf).
    pub fn judge(&mut self, line: LogCursor) -> Verdict {
        if self.passed {
            return Verdict::Take;
        }
        let order = (line.ts, line.rank).cmp(&(self.target.ts, self.target.rank));
        match order {
            std::cmp::Ordering::Less => Verdict::Skip,
            std::cmp::Ordering::Equal => {
                self.passed = true;
                Verdict::Skip
            }
            std::cmp::Ordering::Greater => Verdict::Lost,
        }
    }

    /// La ligne du curseur a été vue. Une relecture qui finit sans l'avoir vue l'a perdue.
    pub fn passed(&self) -> bool {
        self.passed
    }
}

/// Ce que l'agent fait d'un curseur de reprise venu du client (jamais cru sur parole).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeCheck {
    /// Le curseur est plausible : l'agent relit à partir de lui, relecture bornée par
    /// [`RESUME_MAX_LINES`].
    Usable,
    /// Trop ancien, dans le futur, ou illisible : la reprise est perdue (`gap { resume_lost }`, puis les
    /// 1 000 dernières lignes), sans rien demander de large au moteur.
    Lost,
}

/// Juge un curseur de reprise à l'heure `now_ms` de l'agent (BR-SLOG-014).
pub fn check_resume(cursor: LogCursor, now_ms: i64) -> ResumeCheck {
    let rank_ok = usize::try_from(cursor.rank).is_ok_and(|rank| rank <= RESUME_MAX_LINES);
    let age = now_ms.saturating_sub(cursor.ts);
    if cursor.ts < 0 || !rank_ok || !(-CLOCK_SKEW_MS..=RESUME_MAX_AGE_MS).contains(&age) {
        ResumeCheck::Lost
    } else {
        ResumeCheck::Usable
    }
}

// ---------------------------------------------------------------------------------------------
// Anneau borné
// ---------------------------------------------------------------------------------------------

/// Une ligne et son curseur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub line: LogLine,
    pub cursor: LogCursor,
    /// La ligne fait partie de la première page (l'historique lu à l'ouverture) : si l'anneau doit
    /// l'évincer, le trou dit `first_page`, pas `slow` (BR-SLOG-001).
    pub history: bool,
}

/// Ce qui n'est pas une ligne et prend sa place dans l'ordre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogEvent {
    Status(LogsStatus),
    Marker(LogMarker),
    /// Erreur terminale : rien ne suit.
    Error(LogsErrorCode),
}

enum Item {
    Line(Entry),
    Gap(LogGap),
    Event(LogEvent),
}

/// Un morceau de ce qui part vers le client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Lines(Vec<Entry>),
    Gap(LogGap),
    Event(LogEvent),
}

/// L'anneau d'une lecture : au plus `max_lines` lignes et `max_bytes` de texte. Quand il déborde, les
/// lignes les plus anciennes sont retirées, comptées, et remplacées à leur place par un
/// `gap { slow }` : le producteur (qui lit le moteur) n'attend jamais le consommateur (qui écrit sur le
/// socket). Les lignes, les repères et les états gardent leur ordre.
pub struct LogRing {
    items: VecDeque<Item>,
    lines: usize,
    bytes: usize,
    max_lines: usize,
    max_bytes: usize,
    lost: u64,
}

impl Default for LogRing {
    fn default() -> Self {
        Self::new(RING_LINES, RING_BYTES)
    }
}

impl LogRing {
    pub fn new(max_lines: usize, max_bytes: usize) -> Self {
        Self {
            items: VecDeque::new(),
            lines: 0,
            bytes: 0,
            max_lines,
            max_bytes,
            lost: 0,
        }
    }

    pub fn push_line(&mut self, entry: Entry) {
        self.lines += 1;
        self.bytes += entry.line.weight();
        self.items.push_back(Item::Line(entry));
        while (self.lines > self.max_lines || self.bytes > self.max_bytes) && self.evict_oldest() {}
    }

    pub fn push_gap(&mut self, gap: LogGap) {
        self.items.push_back(Item::Gap(gap));
    }

    pub fn push_event(&mut self, event: LogEvent) {
        self.items.push_back(Item::Event(event));
        self.bound_events();
    }

    /// Les états et repères ne sont jamais évincés par le débit de lignes, mais leur nombre est borné : au
    /// plafond, le plus ancien état (remplacé par les suivants) part, à défaut le plus ancien repère.
    fn bound_events(&mut self) {
        let count = self
            .items
            .iter()
            .filter(|item| matches!(item, Item::Event(LogEvent::Status(_) | LogEvent::Marker(_))))
            .count();
        if count <= MAX_RING_EVENTS {
            return;
        }
        let oldest = self
            .items
            .iter()
            .position(|item| matches!(item, Item::Event(LogEvent::Status(_))))
            .or_else(|| {
                self.items
                    .iter()
                    .position(|item| matches!(item, Item::Event(LogEvent::Marker(_))))
            });
        if let Some(index) = oldest {
            self.items.remove(index);
        }
    }

    /// Une ligne de ce poids entre-t-elle sans rien évincer ? (L'historique attend cette place.)
    pub fn has_room_for(&self, weight: usize) -> bool {
        self.lines < self.max_lines && self.bytes.saturating_add(weight) <= self.max_bytes
    }

    /// Retire la plus ancienne ligne et l'ajoute au trou voisin de même raison (ou en crée un à sa place) :
    /// `first_page` pour une ligne de la première page, `slow` pour le direct.
    fn evict_oldest(&mut self) -> bool {
        let Some(index) = self
            .items
            .iter()
            .position(|item| matches!(item, Item::Line(_)))
        else {
            return false;
        };
        let mut reason = GapReason::Slow;
        if let Some(Item::Line(entry)) = self.items.get(index) {
            self.lines -= 1;
            self.bytes = self.bytes.saturating_sub(entry.line.weight());
            if entry.history {
                reason = GapReason::FirstPage;
            }
        }
        self.lost += 1;
        if index > 0
            && let Some(Item::Gap(gap)) = self.items.get_mut(index - 1)
            && gap.reason == reason
        {
            gap.count = Some(gap.count.unwrap_or(0) + 1);
            self.items.remove(index);
        } else if let Some(Item::Gap(gap)) = self.items.get_mut(index + 1)
            && gap.reason == reason
        {
            gap.count = Some(gap.count.unwrap_or(0) + 1);
            self.items.remove(index);
        } else if let Some(slot) = self.items.get_mut(index) {
            *slot = Item::Gap(LogGap {
                count: Some(1),
                reason,
            });
        }
        true
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Lignes en attente.
    pub fn lines(&self) -> usize {
        self.lines
    }

    /// Poids des lignes en attente.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Le prochain élément à partir est-il une ligne ? (un repère ou un état part sans attendre.)
    pub fn head_is_line(&self) -> bool {
        matches!(self.items.front(), Some(Item::Line(_)))
    }

    /// Combien de lignes cet anneau a perdues depuis sa création.
    pub fn lost(&self) -> u64 {
        self.lost
    }

    /// Le prochain morceau : un état, un repère, un trou, ou un lot de lignes contiguës (au plus
    /// `max_lines` lignes et `max_bytes` de poids, une ligne au moins).
    pub fn pop_step(&mut self, max_lines: usize, max_bytes: usize) -> Option<Step> {
        match self.items.pop_front()? {
            Item::Gap(gap) => Some(Step::Gap(gap)),
            Item::Event(event) => Some(Step::Event(event)),
            Item::Line(first) => {
                let mut weight = first.line.weight();
                let mut taken = vec![first];
                while taken.len() < max_lines {
                    let fits = match self.items.front() {
                        Some(Item::Line(next)) => weight + next.line.weight() <= max_bytes,
                        _ => false,
                    };
                    if !fits {
                        break;
                    }
                    if let Some(Item::Line(next)) = self.items.pop_front() {
                        weight += next.line.weight();
                        taken.push(next);
                    }
                }
                self.lines -= taken.len();
                self.bytes = self.bytes.saturating_sub(weight);
                Some(Step::Lines(taken))
            }
        }
    }
}

/// Un lot part-il maintenant ? Un état, un repère ou un trou partent tout de suite ; des lignes
/// partent à 100 ms d'attente, ou dès 500 lignes, ou dès 256 Kio (BR-SLOG-003, BR-SLOG-007).
pub fn flush_due(head_is_line: bool, lines: usize, bytes: usize, waited: Duration) -> bool {
    !head_is_line
        || lines >= BATCH_LINES
        || bytes >= BATCH_BYTES
        || waited >= Duration::from_millis(BATCH_WINDOW_MS)
}

// ---------------------------------------------------------------------------------------------
// Qui lit quoi
// ---------------------------------------------------------------------------------------------

/// Le conteneur courant d'un service, réduit à ce que la lecture des journaux en sait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogContainer {
    /// Identifiant complet.
    pub id: String,
    /// Qui a créé le service (décision de lecture, voir [`may_read`]).
    pub origin: LogOrigin,
    pub run_state: RunState,
    /// Lancé avec un terminal : une seule sortie, flux brut (BR-SLOG-012).
    pub tty: bool,
    /// Le pilote de journaux du conteneur permet de les relire (pas `none`, BR-SLOG-017).
    pub readable: bool,
    pub created_ms: Option<i64>,
    pub started_ms: Option<i64>,
    pub finished_ms: Option<i64>,
    pub exit_code: Option<i64>,
}

fn millis(at: OffsetDateTime) -> i64 {
    i64::try_from(at.unix_timestamp_nanos() / 1_000_000).unwrap_or(i64::MAX)
}

impl LogContainer {
    /// Ce que la lecture des journaux retient d'un conteneur observé.
    pub fn from_observation(observation: &Observation) -> Self {
        Self {
            id: observation.id.clone(),
            // Dans cette tranche, tout service est créé hors de Hearth (BR-SVC-003). HRT-79 (adoption)
            // et la tranche « déployer » (créé par Hearth) remplaceront cette ligne par l'origine publiée.
            origin: LogOrigin::External,
            run_state: observation.run_state,
            tty: observation.tty,
            readable: observation.log_driver.as_deref() != Some("none"),
            created_ms: observation.created_at.map(millis),
            started_ms: observation.started_at.map(millis),
            finished_ms: observation.finished_at.map(millis),
            exit_code: observation.exit_code,
        }
    }

    /// Le conteneur tourne (ou est en pause, ou relancé par le moteur) : ses journaux avancent.
    pub fn is_live(&self) -> bool {
        matches!(
            self.run_state,
            RunState::Running | RunState::Paused | RunState::Restarting
        )
    }

    /// Les douze premiers caractères de l'identifiant, ce que le client voit.
    pub fn short_id(&self) -> String {
        self.id.chars().take(12).collect()
    }

    /// Ce curseur est-il d'une exécution antérieure à ce conteneur ?
    pub fn relation_to(&self, cursor: LogCursor) -> Relation {
        if self.created_ms.is_some_and(|created| created > cursor.ts) {
            Relation::NewContainer
        } else if self.started_ms.is_some_and(|started| started > cursor.ts) {
            Relation::Restarted
        } else {
            Relation::Same
        }
    }
}

/// Comment le conteneur courant se rapporte au curseur d'une reprise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    /// Le curseur est une ligne de ce conteneur, avant son dernier démarrage éventuel.
    Same,
    /// Même conteneur, démarré depuis : un repère « redémarré » puis la suite.
    Restarted,
    /// Un autre conteneur (recréé par le système) : un repère puis ses lignes depuis son début.
    NewContainer,
}

/// Qui a créé un service. Aujourd'hui l'agent ne publie que `External` (BR-SVC-003) ; les deux autres
/// existent pour que la décision de lecture ([`may_read`]) n'ait pas à changer de forme quand HRT-79
/// (services adoptés) et la tranche « déployer » (créé par Hearth) arrivent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogOrigin {
    /// Créé par Hearth (clé `h_`).
    Hearth,
    /// Créé hors de Hearth puis adopté par un acte (ADR-0043, brouillon).
    Adopted,
    /// Créé hors de Hearth, pas adopté.
    External,
}

/// Ce que la clé d'un service désigne en ce moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogTarget {
    /// Aucun service de cette clé (disparu, ou jamais vu).
    Missing,
    /// Service listé d'après son unité, dont le conteneur est supprimé à chaque arrêt : le moteur n'a
    /// gardé aucune ligne (BR-SLOG-008).
    NoContainer {
        origin: LogOrigin,
    },
    Container(LogContainer),
}

impl LogTarget {
    /// Qui a créé le service (aucun service, aucune origine).
    pub fn origin(&self) -> Option<LogOrigin> {
        match self {
            Self::Missing => None,
            Self::NoContainer { origin } => Some(*origin),
            Self::Container(container) => Some(container.origin),
        }
    }
}

/// Qui lit : le compte (pas seulement son rôle) tel que la session le montre À CE MOMENT. Les décisions du
/// 2026-10-13 (droit de gérer les services, HRT-76 ; services adoptés, HRT-79) portent sur un droit du
/// compte : `services_right` (BR-GRANT-001) y est porté, relu avec la session ; un retrait du droit
/// change donc le `Viewer` et rejoue la décision. [`may_read`] ne s'en sert pas encore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Viewer {
    pub account: AccountId,
    pub role: Role,
    pub services_right: ServicesRight,
}

/// Pourquoi ce compte ne lit pas ces journaux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadRefusal {
    /// Le service n'est pas ouvert à ce compte (journaux d'un service hors Hearth non adopté, ou compte
    /// sans le droit de gérer les services, à venir : HRT-76, HRT-79).
    NotEnabled,
}

/// LE point de décision « ce service est-il lisible par ce compte ». Aujourd'hui, les deux rôles lisent
/// tout service que la liste montre (BR-SLOG-011, BR-SLOG-018). Il est rejoué à l'ouverture, à chaque
/// nouvelle résolution du conteneur, à chaque changement du compte vu par la session, et
/// périodiquement pendant une lecture longue (`application::service_logs::LogAccess`) : un droit retiré
/// ferme la lecture en cours. Les règles « droit de gérer les services » (HRT-76) et « un service hors
/// Hearth se lit s'il est adopté » (HRT-79) s'ajouteront ICI, avec `viewer` et `target.origin()`, sans
/// toucher au reste.
///
/// Restriction par le droit (`viewer.services_right`) et par l'adoption : PAS ICI, elle arrive avec
/// HRT-79 (la route passera alors à `Access::Services` dans `CLASSES`). Aujourd'hui les deux rôles lisent
/// tout service publié, avec ou sans le droit.
pub fn may_read(viewer: &Viewer, target: &LogTarget) -> Result<(), ReadRefusal> {
    let _ = (viewer, target.origin());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assemble(chunks: &[(LogSource, &[u8])]) -> Vec<LogLine> {
        let mut assembler = LineAssembler::new();
        let mut lines = Vec::new();
        for (source, data) in chunks {
            assembler.push(*source, 1_000, data, &mut lines);
        }
        assembler.finish(&mut lines);
        lines
    }

    fn texts(lines: &[LogLine]) -> Vec<&str> {
        lines.iter().map(|line| line.text.as_str()).collect()
    }

    #[test]
    fn a_giant_line_is_cut_at_ten_thousand_characters_and_the_rest_is_counted() {
        // 3 x 40 000 caractères en un seul morceau : bien au-delà du plafond brut aussi.
        let giant = "é".repeat(120_000);
        let mut data = giant.into_bytes();
        data.push(b'\n');
        data.extend_from_slice(b"suivante\n");
        let lines = assemble(&[(LogSource::Out, &data)]);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text.chars().count(), 10_000);
        assert!(lines[0].cut.is_some_and(|total| total >= 10_000));
        assert_eq!(lines[1].text, "suivante");
        assert_eq!(lines[1].cut, None);
    }

    #[test]
    fn a_line_just_over_the_limit_reports_its_exact_total() {
        let mut data = "a".repeat(10_025).into_bytes();
        data.push(b'\n');
        let lines = assemble(&[(LogSource::Out, &data)]);
        assert_eq!(lines[0].text.len(), 10_000);
        assert_eq!(lines[0].cut, Some(10_025));
    }

    #[test]
    fn memory_stays_bounded_for_a_line_that_never_ends() {
        let mut assembler = LineAssembler::new();
        let mut lines = Vec::new();
        let chunk = vec![b'x'; 1 << 20];
        for _ in 0..8 {
            assembler.push(LogSource::Out, 1, &chunk, &mut lines);
        }
        assert!(lines.is_empty());
        assert!(assembler.out.buf.len() <= RAW_LINE_CAP);
        assembler.finish(&mut lines);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text.len(), 10_000);
        assert_eq!(lines[0].cut, Some(8 << 20));
    }

    #[test]
    fn lines_end_on_lf_crlf_and_a_lone_cr_and_blank_lines_are_kept() {
        let lines = assemble(&[(LogSource::Out, b"a\nb\r\nc\rd\r\n\r\ne\n\n")]);
        assert_eq!(texts(&lines), ["a", "b", "c", "d", "", "e", ""]);
    }

    #[test]
    fn a_crlf_split_across_two_chunks_is_still_one_line_end() {
        let lines = assemble(&[(LogSource::Out, b"a\r"), (LogSource::Out, b"\nb\n")]);
        assert_eq!(texts(&lines), ["a", "b"]);
    }

    #[test]
    fn a_progress_bar_that_rewrites_its_line_is_a_series_of_lines() {
        let lines = assemble(&[(LogSource::Out, b"10%\r50%\r100%\n")]);
        assert_eq!(texts(&lines), ["10%", "50%", "100%"]);
    }

    #[test]
    fn colour_codes_control_characters_and_bad_bytes_are_cleaned() {
        let lines = assemble(&[(
            LogSource::Out,
            b"\x1b[31mrouge\x1b[0m\x00 ok \xff fin\x07\n",
        )]);
        assert_eq!(texts(&lines), ["rouge ok \u{fffd} fin"]);
    }

    #[test]
    fn the_two_outputs_do_not_mix_their_unfinished_lines() {
        let lines = assemble(&[
            (LogSource::Out, b"sortie "),
            (LogSource::Err, b"erreur\n"),
            (LogSource::Out, b"normale\n"),
        ]);
        assert_eq!(texts(&lines), ["erreur", "sortie normale"]);
        assert_eq!(lines[0].source, LogSource::Err);
        assert_eq!(lines[1].source, LogSource::Out);
    }

    #[test]
    fn a_last_line_without_an_end_is_sent_when_the_stream_ends_and_nothing_else_is() {
        let lines = assemble(&[(LogSource::Out, b"fin sans retour")]);
        assert_eq!(texts(&lines), ["fin sans retour"]);
        let lines = assemble(&[(LogSource::Out, b"a\n")]);
        assert_eq!(texts(&lines), ["a"]);
    }

    #[test]
    fn a_line_keeps_the_time_of_its_first_piece() {
        let mut assembler = LineAssembler::new();
        let mut lines = Vec::new();
        assembler.push(LogSource::Out, 10, b"debut ", &mut lines);
        assembler.push(LogSource::Out, 99, b"suite\n", &mut lines);
        assert_eq!(lines[0].at_ms, 10);
    }

    #[test]
    fn the_engine_timestamp_is_read_and_skipped_only_when_it_is_one() {
        let (ms, skip) = split_timestamp(b"2026-10-13T10:00:00.123456789Z hello\n").unwrap();
        assert_eq!(ms, 1_791_885_600_123);
        assert_eq!(skip, 31);
        assert_eq!(split_timestamp(b"hello world\n"), None);
        assert_eq!(split_timestamp(b"2026-10-13 10:00:00 hello"), None);
        assert_eq!(split_timestamp(b"2026-13-45T99:00:00.000000000Z x"), None);
        assert_eq!(split_timestamp(b""), None);
        // Une ligne vide : l'horodatage puis la fin de ligne.
        let (_, skip) = split_timestamp(b"2026-10-13T10:00:00.000000000Z \n").unwrap();
        assert_eq!(skip, 31);
    }

    #[test]
    fn ranks_count_the_lines_of_the_same_millisecond_and_time_never_goes_back() {
        let mut tracker = RankTracker::new();
        let cursors: Vec<_> = [5, 5, 5, 6, 4, 7].map(|at| tracker.next(at)).to_vec();
        let shown: Vec<_> = cursors.iter().map(|c| (c.ts, c.rank)).collect();
        assert_eq!(shown, [(5, 0), (5, 1), (5, 2), (6, 0), (6, 1), (7, 0)]);
    }

    #[test]
    fn a_resume_skips_up_to_the_cursor_line_included_and_takes_what_follows() {
        let target = LogCursor { ts: 5, rank: 1 };
        let mut filter = ResumeFilter::new(target);
        let mut tracker = RankTracker::new();
        let verdicts: Vec<_> = [4, 5, 5, 5, 6]
            .map(|at| filter.judge(tracker.next(at)))
            .to_vec();
        assert_eq!(
            verdicts,
            [
                Verdict::Skip,
                Verdict::Skip,
                Verdict::Skip,
                Verdict::Take,
                Verdict::Take
            ]
        );
        assert!(filter.passed());
    }

    #[test]
    fn a_resume_whose_line_is_gone_is_lost_not_silently_wrong() {
        let mut filter = ResumeFilter::new(LogCursor { ts: 5, rank: 3 });
        let mut tracker = RankTracker::new();
        assert_eq!(filter.judge(tracker.next(5)), Verdict::Skip);
        assert_eq!(filter.judge(tracker.next(6)), Verdict::Lost);
        assert!(!filter.passed());
    }

    fn entry(text: &str, at: i64, rank: u32) -> Entry {
        Entry {
            line: LogLine {
                at_ms: at,
                source: LogSource::Out,
                text: text.to_owned(),
                cut: None,
            },
            cursor: LogCursor { ts: at, rank },
            history: false,
        }
    }

    fn gap_counts(steps: &[Step]) -> Vec<u64> {
        steps
            .iter()
            .filter_map(|step| match step {
                Step::Gap(gap) => gap.count,
                _ => None,
            })
            .collect()
    }

    fn drain(ring: &mut LogRing) -> Vec<Step> {
        let mut steps = Vec::new();
        while let Some(step) = ring.pop_step(500, 256 * 1024) {
            steps.push(step);
        }
        steps
    }

    #[test]
    fn a_slow_reader_loses_the_oldest_lines_and_a_counted_gap_comes_first() {
        let mut ring = LogRing::new(5, usize::MAX);
        for n in 0..8u32 {
            ring.push_line(entry(&format!("ligne {n}"), 100 + i64::from(n), 0));
        }
        assert_eq!(ring.lost(), 3);
        assert_eq!(ring.lines(), 5);
        let steps = drain(&mut ring);
        assert_eq!(gap_counts(&steps), [3]);
        let Step::Gap(gap) = &steps[0] else {
            panic!("le trou précède les lignes : {steps:?}");
        };
        assert_eq!(gap.reason, GapReason::Slow);
        let Step::Lines(kept) = &steps[1] else {
            panic!("{steps:?}");
        };
        assert_eq!(kept.first().unwrap().line.text, "ligne 3");
        assert_eq!(kept.last().unwrap().line.text, "ligne 7");
        assert!(ring.is_empty());
    }

    #[test]
    fn the_ring_is_bounded_by_text_size_too() {
        let mut ring = LogRing::new(usize::MAX, 10 * (1000 + LINE_OVERHEAD));
        for n in 0..50 {
            ring.push_line(entry(&"x".repeat(1000), n, 0));
        }
        assert_eq!(ring.lines(), 10);
        assert_eq!(ring.lost(), 40);
        assert!(ring.bytes() <= 10 * (1000 + LINE_OVERHEAD));
    }

    #[test]
    fn markers_and_states_keep_their_place_among_lines_and_are_never_lost() {
        let mut ring = LogRing::new(3, usize::MAX);
        ring.push_line(entry("avant 1", 1, 0));
        ring.push_line(entry("avant 2", 2, 0));
        ring.push_event(LogEvent::Marker(LogMarker {
            kind: hearth_proto::api::service_logs::MarkerKind::Restarted,
            at: 50,
        }));
        for n in 0..6 {
            ring.push_line(entry("apres", 100 + n, 0));
        }
        let steps = drain(&mut ring);
        // Le repère reste ; les trous sont devant les lignes perdues, dans l'ordre.
        let marker_at = steps
            .iter()
            .position(|step| matches!(step, Step::Event(LogEvent::Marker(_))))
            .expect("le repère n'est jamais perdu");
        let total_lost: u64 = gap_counts(&steps).iter().sum();
        assert_eq!(total_lost, ring.lost());
        assert_eq!(total_lost, 5);
        assert!(
            marker_at <= 1,
            "le repère passe avant les lignes qui le suivent : {steps:?}"
        );
    }

    #[test]
    fn a_batch_stops_at_five_hundred_lines_or_the_byte_cap_and_always_carries_a_line() {
        let mut ring = LogRing::default();
        for n in 0..1200 {
            ring.push_line(entry("x", n, 0));
        }
        let Some(Step::Lines(first)) = ring.pop_step(500, 256 * 1024) else {
            panic!("un lot de lignes");
        };
        assert_eq!(first.len(), 500);
        assert_eq!(ring.lines(), 700);
        let mut heavy = LogRing::default();
        for n in 0..4 {
            heavy.push_line(entry(&"y".repeat(100_000), n, 0));
        }
        let Some(Step::Lines(batch)) = heavy.pop_step(500, 256 * 1024) else {
            panic!("un lot de lignes");
        };
        assert_eq!(batch.len(), 2, "256 Kio de texte au plus");
        let mut oversized = LogRing::default();
        oversized.push_line(entry(&"z".repeat(300_000), 0, 0));
        let Some(Step::Lines(one)) = oversized.pop_step(500, 256 * 1024) else {
            panic!("un lot de lignes");
        };
        assert_eq!(one.len(), 1, "une ligne seule part toujours");
    }

    #[test]
    fn lines_flush_after_a_hundred_milliseconds_or_at_the_caps_and_events_at_once() {
        let ms = Duration::from_millis;
        assert!(!flush_due(true, 10, 1000, ms(99)));
        assert!(flush_due(true, 10, 1000, ms(100)));
        assert!(flush_due(true, 500, 1000, ms(0)));
        assert!(flush_due(true, 3, 256 * 1024, ms(0)));
        assert!(flush_due(false, 0, 0, ms(0)));
    }

    #[test]
    fn a_producer_at_a_thousand_lines_a_second_against_a_reader_that_never_reads_stays_bounded() {
        // 10 minutes de production à 1 000 lignes par seconde, sans aucune lecture : l'anneau reste
        // plafonné, rien ne bloque, le compte des pertes est exact.
        let mut ring = LogRing::default();
        let produced = 600_000u64;
        for n in 0..produced {
            ring.push_line(entry(
                "une ligne de journal ordinaire",
                i64::try_from(n).unwrap(),
                0,
            ));
        }
        assert_eq!(ring.lines(), RING_LINES);
        assert!(ring.bytes() <= RING_BYTES);
        assert_eq!(ring.lost(), produced - u64::try_from(RING_LINES).unwrap());
        let steps = drain(&mut ring);
        assert_eq!(gap_counts(&steps), [ring.lost()]);
    }

    #[test]
    fn a_container_is_new_restarted_or_the_same_from_the_cursor() {
        let mut container = LogContainer {
            id: "ab".repeat(32),
            origin: LogOrigin::External,
            run_state: RunState::Running,
            tty: false,
            readable: true,
            created_ms: Some(1_000),
            started_ms: Some(2_000),
            finished_ms: None,
            exit_code: None,
        };
        assert_eq!(
            container.relation_to(LogCursor { ts: 2_500, rank: 0 }),
            Relation::Same
        );
        assert_eq!(
            container.relation_to(LogCursor { ts: 1_500, rank: 0 }),
            Relation::Restarted
        );
        assert_eq!(
            container.relation_to(LogCursor { ts: 500, rank: 0 }),
            Relation::NewContainer
        );
        container.created_ms = None;
        container.started_ms = None;
        assert_eq!(
            container.relation_to(LogCursor { ts: 1, rank: 0 }),
            Relation::Same
        );
        assert_eq!(container.short_id().len(), 12);
        assert!(container.is_live());
        container.run_state = RunState::Exited;
        assert!(!container.is_live());
    }

    #[test]
    fn both_roles_read_every_listed_service() {
        // Aujourd'hui le droit ne change rien : sa prise en compte arrive avec HRT-79.
        for (role, services_right) in [Role::Admin, Role::ReadOnly].into_iter().flat_map(|role| {
            [ServicesRight::None, ServicesRight::Manage].map(|right| (role, right))
        }) {
            let viewer = Viewer {
                account: AccountId::new("compte"),
                role,
                services_right,
            };
            for origin in [LogOrigin::Hearth, LogOrigin::Adopted, LogOrigin::External] {
                assert_eq!(
                    may_read(&viewer, &LogTarget::NoContainer { origin }),
                    Ok(())
                );
            }
        }
    }

    #[test]
    fn a_resume_cursor_is_trusted_only_inside_the_window() {
        let now = 1_791_885_600_000;
        let at = |ts, rank| check_resume(LogCursor { ts, rank }, now);
        assert_eq!(at(now - 1_000, 3), ResumeCheck::Usable);
        assert_eq!(at(now - RESUME_MAX_AGE_MS, 0), ResumeCheck::Usable);
        // Trop ancien, forgé à zéro, négatif.
        assert_eq!(at(now - RESUME_MAX_AGE_MS - 1, 0), ResumeCheck::Lost);
        assert_eq!(at(0, 0), ResumeCheck::Lost);
        assert_eq!(at(-5, 0), ResumeCheck::Lost);
        // Dans le futur (au-delà de l'écart toléré), ou absurde.
        assert_eq!(at(now + CLOCK_SKEW_MS, 0), ResumeCheck::Usable);
        assert_eq!(at(now + CLOCK_SKEW_MS + 1, 0), ResumeCheck::Lost);
        assert_eq!(at(i64::MAX, 0), ResumeCheck::Lost);
        assert_eq!(at(i64::MIN, 0), ResumeCheck::Lost);
        // Un rang qu'aucune relecture bornée n'atteindrait.
        let too_far = u32::try_from(RESUME_MAX_LINES).unwrap_or(u32::MAX) + 1;
        assert_eq!(at(now, too_far), ResumeCheck::Lost);
        assert_eq!(at(now, u32::MAX), ResumeCheck::Lost);
    }

    #[test]
    fn the_clock_going_back_by_more_than_a_second_is_seen() {
        let mut tracker = RankTracker::new();
        for at in [10_000, 9_500, 10_200, 9_300] {
            tracker.next(at);
        }
        assert!(!tracker.rewound(), "moins d'une seconde : pas un recul");
        tracker.next(9_199);
        assert!(tracker.rewound());
    }

    #[test]
    fn a_first_page_line_pushed_out_of_the_ring_is_not_called_slow() {
        let mut ring = LogRing::new(3, usize::MAX);
        let history = |text: &str, at: i64| Entry {
            history: true,
            ..entry(text, at, 0)
        };
        for at in 1..=3 {
            ring.push_line(history(&format!("p{at}"), at));
        }
        for at in 4..=7 {
            ring.push_line(entry(&format!("d{at}"), at, 0));
        }
        let steps = drain(&mut ring);
        let gaps: Vec<_> = steps
            .iter()
            .filter_map(|step| match step {
                Step::Gap(gap) => Some((gap.reason, gap.count)),
                _ => None,
            })
            .collect();
        assert_eq!(
            gaps,
            [(GapReason::FirstPage, Some(3)), (GapReason::Slow, Some(1))]
        );
    }

    #[test]
    fn states_and_markers_waiting_in_a_ring_are_bounded_too() {
        let mut ring = LogRing::new(10, usize::MAX);
        for n in 0..(MAX_RING_EVENTS * 3) {
            ring.push_event(LogEvent::Marker(LogMarker {
                kind: hearth_proto::api::service_logs::MarkerKind::Restarted,
                at: i64::try_from(n).unwrap_or(0),
            }));
        }
        assert!(ring.items.len() <= MAX_RING_EVENTS);
    }

    #[test]
    fn the_cut_total_counts_characters_not_bytes() {
        // 70 000 caractères de 2 octets : le total est en caractères, pas en octets.
        let mut data = "é".repeat(70_000).into_bytes();
        data.push(b'\n');
        let lines = assemble(&[(LogSource::Out, &data)]);
        let total = lines[0].cut.unwrap_or_default();
        assert!(
            (69_999..=70_001).contains(&total),
            "au plus un caractère d'écart au bord de la coupe : {total}"
        );
    }

    #[test]
    fn the_text_of_a_line_never_appears_in_debug() {
        let shown = format!("{:?}", entry("SENTINEL-LINE-TEXT", 1, 0));
        assert!(!shown.contains("SENTINEL"), "{shown}");
    }
}
