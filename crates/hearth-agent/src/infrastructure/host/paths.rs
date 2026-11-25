//! `HostPaths` sur le système de fichiers de l'hôte (HRT-85, BR-DEL-008) : `canonicalize` et `stat` du chemin et de
//! tous ses parents, chacun borné dans le temps (un partage réseau mort ne fait pas pendre une suppression).

use std::path::Path;
use std::sync::mpsc;
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::application::ports::{HostPaths, Resolved};
use crate::domain::services::delete::FileId;

/// Le temps laissé à la lecture d'un chemin (parents compris).
const READ_TIMEOUT: Duration = Duration::from_secs(3);

/// Combien de lectures peuvent être en cours (ou coincées sur un système de fichiers mort) en même temps.
const MAX_READERS: usize = 8;

/// Le temps qu'une lecture attend une place quand les huit sont prises. Passé ce délai, la réponse est « illisible » :
/// jamais un feu vert, mais jamais non plus un refus immédiat parce que d'autres lectures, saines, finissent.
const WAIT_FOR_SLOT: Duration = Duration::from_secs(10);

/// Les places de lecture : un fil coincé sur un partage mort tient la sienne jusqu'à son retour.
struct Gate {
    taken: Mutex<usize>,
    freed: Condvar,
}

impl Gate {
    const fn new() -> Self {
        Self {
            taken: Mutex::new(0),
            freed: Condvar::new(),
        }
    }

    /// Prend une place, en attendant au plus `wait` qu'une autre soit rendue. `None` : toujours pleine.
    fn take(&'static self, max: usize, wait: Duration) -> Option<Slot> {
        let deadline = Instant::now() + wait;
        let mut taken = self.taken.lock().unwrap_or_else(PoisonError::into_inner);
        while *taken >= max {
            let left = deadline.checked_duration_since(Instant::now())?;
            if left.is_zero() {
                return None;
            }
            taken = self
                .freed
                .wait_timeout(taken, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        *taken += 1;
        Some(Slot(self))
    }
}

/// Les fils de lecture vivants.
static READERS: Gate = Gate::new();

/// Rend sa place à la fin du fil, même sur une panique.
struct Slot(&'static Gate);

impl Drop for Slot {
    fn drop(&mut self) {
        let mut taken = self.0.taken.lock().unwrap_or_else(PoisonError::into_inner);
        *taken = taken.saturating_sub(1);
        drop(taken);
        self.0.freed.notify_one();
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemHostPaths;

impl HostPaths for SystemHostPaths {
    fn resolve(&self, path: &str) -> Resolved {
        let owned = path.to_owned();
        within(
            &READERS,
            MAX_READERS,
            WAIT_FOR_SLOT,
            READ_TIMEOUT,
            move || read(&owned),
        )
        .unwrap_or(Resolved::Unreadable)
    }
}

/// Fait `work` sur un fil à part et n'attend que `timeout` la réponse, après avoir attendu au plus `wait` une place.
/// `None` : pas de place ou pas de réponse à temps (le fil reste pris par l'appel bloqué, l'agent ne l'attend plus).
// FIX:01M855R2F0XJ3KDBH325R1NR2R : attendre une place au lieu de refuser sur-le-champ au-delà de `max`.
fn within<T: Send + 'static>(
    gate: &'static Gate,
    max: usize,
    wait: Duration,
    timeout: Duration,
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let slot = gate.take(max, wait)?;
    let (sender, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("host-paths".into())
        .spawn(move || {
            let _slot = slot;
            let _ = sender.send(work());
        })
        .ok()?;
    receiver.recv_timeout(timeout).ok()
}

fn read(path: &str) -> Resolved {
    let canonical = match std::fs::canonicalize(Path::new(path)) {
        Ok(canonical) => canonical,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Resolved::Absent,
        Err(_) => return Resolved::Unreadable,
    };
    let mut lineage = Vec::new();
    for ancestor in canonical.ancestors() {
        match std::fs::metadata(ancestor) {
            Ok(meta) => lineage.extend(file_id(&meta)),
            Err(_) => return Resolved::Unreadable,
        }
    }
    let Some(text) = canonical.to_str() else {
        return Resolved::Unreadable;
    };
    // Windows (poste de développement) : `canonicalize` rend la forme `\?\C:\...` ; l'agent ne tourne que sous
    // Linux, mais ses tests tournent ici.
    let canonical = text.strip_prefix(r"\\?\").unwrap_or(text).to_owned();
    Resolved::Found { canonical, lineage }
}

#[cfg(unix)]
fn file_id(meta: &std::fs::Metadata) -> Option<FileId> {
    use std::os::unix::fs::MetadataExt;
    Some((meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn file_id(_meta: &std::fs::Metadata) -> Option<FileId> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_that_never_answers_is_given_up_on_not_waited_for() {
        let started = std::time::Instant::now();
        static GATE: Gate = Gate::new();
        let none = Duration::ZERO;
        let answer = within(&GATE, 8, none, Duration::from_millis(100), || {
            std::thread::sleep(Duration::from_secs(5));
            1
        });
        assert_eq!(answer, None);
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(
            within(&GATE, 8, none, Duration::from_secs(5), || 7),
            Some(7)
        );
    }

    #[test]
    fn a_read_waits_for_a_place_instead_of_being_refused_while_the_others_finish() {
        // FIX:01M855R2F0XJ3KDBH325R1NR2R : deux places, quatre lectures saines de 150 ms qui arrivent ensemble.
        static GATE: Gate = Gate::new();
        let handles: Vec<_> = (0..4)
            .map(|n| {
                std::thread::spawn(move || {
                    within(
                        &GATE,
                        2,
                        Duration::from_secs(5),
                        Duration::from_secs(5),
                        move || {
                            std::thread::sleep(Duration::from_millis(150));
                            n
                        },
                    )
                })
            })
            .collect();
        for (n, handle) in handles.into_iter().enumerate() {
            assert_eq!(handle.join().unwrap(), Some(n), "aucune lecture refusée");
        }
    }

    #[test]
    fn stuck_readers_make_the_next_one_wait_then_give_up_unreadable_not_a_thread_more() {
        static GATE: Gate = Gate::new();
        for _ in 0..2 {
            assert_eq!(
                within(&GATE, 2, Duration::ZERO, Duration::from_millis(20), || {
                    std::thread::sleep(Duration::from_millis(600));
                }),
                None
            );
        }
        // Deux lectures coincées tiennent les deux places : la troisième attend son délai, puis abandonne.
        let started = std::time::Instant::now();
        assert_eq!(
            within(
                &GATE,
                2,
                Duration::from_millis(100),
                Duration::from_secs(5),
                || 1
            ),
            None
        );
        let waited = started.elapsed();
        assert!(waited >= Duration::from_millis(100), "{waited:?}");
        assert!(waited < Duration::from_millis(500), "{waited:?}");
        // Les fils coincés finissent : les places reviennent.
        assert_eq!(
            within(
                &GATE,
                2,
                Duration::from_secs(5),
                Duration::from_secs(5),
                || 1
            ),
            Some(1)
        );
    }

    #[test]
    fn a_missing_path_is_absent_not_unreadable() {
        assert_eq!(
            SystemHostPaths.resolve("/definitely/not/here"),
            Resolved::Absent
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_has_the_canonical_path_and_the_lineage_of_its_target() {
        use std::os::unix::fs::symlink;
        let dir = crate::infrastructure::data_dir::private_tempdir();
        let real = dir.path().join("real").join("inner");
        std::fs::create_dir_all(&real).unwrap();
        let link = dir.path().join("link");
        symlink(&real, &link).unwrap();
        let (real_text, link_text) = (real.to_str().unwrap(), link.to_str().unwrap());
        let (
            Resolved::Found {
                canonical: a,
                lineage: la,
            },
            Resolved::Found {
                canonical: b,
                lineage: lb,
            },
        ) = (
            SystemHostPaths.resolve(real_text),
            SystemHostPaths.resolve(link_text),
        )
        else {
            panic!("les deux chemins existent");
        };
        assert_eq!(a, b);
        assert_eq!(la, lb, "même dossier, même lignée");
        // Le dossier, son parent `real`, puis les parents du dossier temporaire jusqu'à la racine.
        assert!(la.len() >= 3);
        let parent = SystemHostPaths.resolve(real.parent().unwrap().to_str().unwrap());
        let Resolved::Found {
            lineage: parent_lineage,
            ..
        } = parent
        else {
            panic!("le parent existe");
        };
        assert_eq!(
            la[1..],
            parent_lineage[..],
            "la lignée d'un enfant suit celle de son parent"
        );
    }
}
