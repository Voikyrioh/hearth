//! Le verrou exclusif entre processus sur un fichier (`flock`), partagé par l'installation, la
//! mise à jour et l'identité TLS. Le noyau le relâche si le processus meurt : aucun nettoyage à
//! faire après un arrêt brutal.
//!
//! Il est relâché **explicitement** à la destruction, pas seulement par la fermeture du
//! descripteur. Un `flock` appartient à la description de fichier ouverte, pas au descripteur. Si
//! un autre thread lance un sous-processus à cet instant, l'enfant porte une copie du descripteur
//! entre le `fork` et l'`exec` (qui le ferme : `O_CLOEXEC`) ; fermer le nôtre ne suffit alors pas,
//! le verrou reste pris le temps de cet `exec`, et l'opération suivante est refusée à tort.
//! `unlock` agit sur la description : il libère le verrou quelles que soient les copies vivantes.
//! (FIX:01M46N01GMK08NXQHCZ28A2KQ1, `docs/bugs/FIX-01M46N01GMK08NXQHCZ28A2KQ1.md`.)

use std::fs::{File, TryLockError};
use std::io;
use std::thread;
use std::time::{Duration, Instant};

/// Pourquoi le verrou n'a pas été pris.
#[derive(Debug)]
pub enum LockError {
    /// Un autre processus (ou une autre description de fichier) le tient, y compris après avoir
    /// attendu.
    Busy,
    /// Le système a refusé de verrouiller.
    Io(io::Error),
}

/// Le verrou tenu : tant que cet objet existe, aucun autre processus ne le prend.
#[derive(Debug)]
pub struct HeldLock(File);

impl HeldLock {
    /// Prend le verrou exclusif sur `file`. Sans attente (`patience` nulle), un essai ; sinon un
    /// essai toutes les `retry` jusqu'à `patience`. Sans succès, le fichier est refermé.
    pub fn acquire(file: File, patience: Duration, retry: Duration) -> Result<Self, LockError> {
        let deadline = Instant::now() + patience;
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Self(file)),
                Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                    // Les appelants sont synchrones : une courte attente bloquante suffit.
                    thread::sleep(retry);
                }
                Err(TryLockError::WouldBlock) => return Err(LockError::Busy),
                Err(TryLockError::Error(error)) => return Err(LockError::Io(error)),
            }
        }
    }

    /// Une copie du descripteur, comme celle que porte un enfant entre `fork` et `exec` : les tests
    /// vérifient que le verrou est libre alors qu'elle vit encore.
    #[cfg(test)]
    pub fn descriptor_copy(&self) -> File {
        self.0.try_clone().expect("copie du descripteur")
    }
}

impl Drop for HeldLock {
    fn drop(&mut self) {
        // FIX:01M46N01GMK08NXQHCZ28A2KQ1 : relâche le verrou même si un enfant en cours de
        // lancement garde encore une copie du descripteur. Échec ignoré : la fermeture qui suit le
        // relâche de toute façon, et on ne panique pas dans un `drop`.
        let _ = self.0.unlock();
    }
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;
    use std::path::Path;

    use super::*;

    fn open(path: &Path) -> File {
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(path)
            .expect("ouverture")
    }

    /// FIX:01M46N01GMK08NXQHCZ28A2KQ1 : le verrou est relâché alors qu'une copie de son descripteur
    /// est encore vivante.
    #[test]
    fn the_lock_is_released_while_a_copy_of_its_descriptor_is_still_alive() {
        let dir = tempfile::tempdir().expect("dossier");
        let path = dir.path().join("a.lock");
        let held = HeldLock::acquire(open(&path), Duration::ZERO, Duration::ZERO).expect("pris");
        let copy = held.descriptor_copy();
        drop(held);
        assert!(
            HeldLock::acquire(open(&path), Duration::ZERO, Duration::ZERO).is_ok(),
            "libre alors que la copie vit encore"
        );
        drop(copy);
    }

    #[test]
    fn a_lock_held_elsewhere_is_busy_and_free_again_once_dropped() {
        let dir = tempfile::tempdir().expect("dossier");
        let path = dir.path().join("b.lock");
        let held = HeldLock::acquire(open(&path), Duration::ZERO, Duration::ZERO).expect("pris");
        let refused = HeldLock::acquire(open(&path), Duration::ZERO, Duration::ZERO);
        assert!(matches!(refused, Err(LockError::Busy)), "{refused:?}");
        drop(held);
        assert!(HeldLock::acquire(open(&path), Duration::ZERO, Duration::ZERO).is_ok());
    }

    #[test]
    fn a_lock_released_during_the_patience_is_taken() {
        let dir = tempfile::tempdir().expect("dossier");
        let path = dir.path().join("c.lock");
        let held = HeldLock::acquire(open(&path), Duration::ZERO, Duration::ZERO).expect("pris");
        let releaser = thread::spawn(move || {
            thread::sleep(Duration::from_millis(30));
            drop(held);
        });
        let taken = HeldLock::acquire(
            open(&path),
            Duration::from_secs(10),
            Duration::from_millis(5),
        );
        assert!(taken.is_ok(), "{taken:?}");
        releaser.join().expect("fil");
    }
}
