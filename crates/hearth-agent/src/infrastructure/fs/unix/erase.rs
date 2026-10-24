//! Effacement d'un arbre par descripteurs, par lots, sans jamais suivre un lien (HRT-85, HRT-179,
//! `BR-DEL-007`, `BR-DEL-030`).
//!
//! # Comment
//!
//! - **Mémoire bornée** : les noms d'un dossier se lisent par lots de [`BATCH`] au plus (jamais le dossier
//!   en entier), chaque lot est retiré puis le dossier est relu depuis le début. Un dossier de plusieurs
//!   millions de fichiers coûte [`BATCH`] noms en mémoire, pas des millions. Retirer pendant qu'on lit fait
//!   sauter des entrées : c'est pourquoi on relit, jamais l'inverse.
//! - **Aucune boucle sans fin** : un nom qui refuse de partir est retenu (`stuck`) et ne revient plus dans
//!   les lots de CE dossier ; un passage qui ne trouve plus de nom neuf ferme le dossier. Un service qui
//!   créerait des fichiers pendant l'effacement est borné par [`MAX_BATCHES`].
//! - **Jamais d'arrêt à la première erreur** : ce qui part part, ce qui reste est compté
//!   ([`Erased::left`]), une seule fois.
//! - **Un lien est retiré comme lien** (`unlinkat` sans `REMOVEDIR`), sa cible n'est pas touchée ; un
//!   dossier remplacé par autre chose pendant le parcours est retiré sous CE nom ; un autre périphérique
//!   (point de montage) n'est pas descendu ; au-delà de [`MAX_DEPTH`] niveaux, rien n'est descendu.

use std::collections::HashSet;
use std::ffi::{CStr, CString};
use std::os::fd::OwnedFd;

use rustix::fs::{AtFlags, Dir, FileType, Mode, fstat, openat, statat, unlinkat};
use rustix::io::Errno;

use super::{OPEN_DIR, dev_of, file_type, fs_error};
use crate::application::ports::{Erased, FsError};
use crate::domain::files::MAX_DEPTH;

/// Combien de noms au plus sont tenus en mémoire pour un dossier.
pub(crate) const BATCH: usize = 1024;

/// Combien de lots au plus pour un seul dossier : de quoi effacer plus d'un milliard d'entrées, mais pas
/// d'attendre indéfiniment un service qui en créerait à mesure.
const MAX_BATCHES: u64 = 1 << 20;

/// Ce qu'il est advenu d'une entrée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fate {
    /// Partie (ou déjà partie).
    Gone,
    /// Une feuille qui reste : comptée dans `left` par l'appelant, une seule fois.
    StuckLeaf,
    /// Un dossier qui reste parce que des éléments de dessous restent : ils sont déjà comptés.
    StuckDir,
}

/// L'effaceur : le périphérique de la racine, le décompte, le suivi.
pub(super) struct Eraser<'p> {
    dev: u128,
    tally: Erased,
    progress: &'p mut dyn FnMut(u64),
}

impl<'p> Eraser<'p> {
    pub(super) fn new(dev: u128, progress: &'p mut dyn FnMut(u64)) -> Self {
        Self {
            dev,
            tally: Erased::default(),
            progress,
        }
    }

    /// Le décompte final, après un dernier signe de vie du suivi.
    pub(super) fn finish(self) -> Erased {
        (self.progress)(self.tally.removed);
        self.tally
    }

    fn note(&mut self, error: FsError) {
        self.tally.first_error.get_or_insert(error);
    }

    fn stuck(&mut self, error: FsError) -> Fate {
        self.note(error);
        Fate::StuckLeaf
    }

    /// Retire UNE entrée de `dir` : fichier, lien, ou arbre. Ce qui reste est compté dans le décompte.
    pub(super) fn erase_entry(&mut self, dir: &OwnedFd, name: &CStr, depth: usize) {
        if self.entry(dir, name, depth) == Fate::StuckLeaf {
            self.tally.left += 1;
        }
    }

    fn entry(&mut self, dir: &OwnedFd, name: &CStr, depth: usize) -> Fate {
        let stat = match statat(dir, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(stat) => stat,
            Err(Errno::NOENT) => return Fate::Gone,
            Err(errno) => return self.stuck(fs_error(errno)),
        };
        if file_type(&stat) != FileType::Directory {
            return self.unlink(dir, name);
        }
        if depth >= MAX_DEPTH {
            return self.stuck(FsError::Refused);
        }
        let child = match openat(dir, name, OPEN_DIR, Mode::empty()) {
            Ok(child) => child,
            // Remplacé par autre chose qu'un dossier entre-temps : on retire CE nom, sans le suivre.
            Err(Errno::NOTDIR | Errno::LOOP) => return self.unlink(dir, name),
            Err(Errno::NOENT) => return Fate::Gone,
            Err(errno) => return self.stuck(fs_error(errno)),
        };
        match fstat(&child) {
            Ok(opened) if dev_of(&opened) == self.dev => {}
            // Un point de montage : jamais descendu, il reste.
            Ok(_) => return self.stuck(FsError::CrossesDevice),
            Err(errno) => return self.stuck(fs_error(errno)),
        }
        let left_before = self.tally.left;
        self.contents(&child, depth + 1);
        drop(child);
        match unlinkat(dir, name, AtFlags::REMOVEDIR) {
            Ok(()) => {
                self.tally.removed += 1;
                Fate::Gone
            }
            Err(Errno::NOENT) => Fate::Gone,
            Err(errno) => {
                self.note(fs_error(errno));
                // Des éléments de dessous restent et sont déjà comptés ; sinon le dossier lui-même l'est.
                if self.tally.left > left_before {
                    Fate::StuckDir
                } else {
                    Fate::StuckLeaf
                }
            }
        }
    }

    fn unlink(&mut self, dir: &OwnedFd, name: &CStr) -> Fate {
        match unlinkat(dir, name, AtFlags::empty()) {
            Ok(()) => {
                self.tally.removed += 1;
                Fate::Gone
            }
            Err(Errno::NOENT) => Fate::Gone,
            Err(errno) => self.stuck(fs_error(errno)),
        }
    }

    /// Vide un dossier, lot par lot.
    pub(super) fn contents(&mut self, dir: &OwnedFd, depth: usize) {
        let mut stuck: HashSet<CString> = HashSet::new();
        for _ in 0..MAX_BATCHES {
            let names = match next_batch(dir, &stuck) {
                Ok(names) => names,
                Err(error) => {
                    // Le dossier ne se relit pas : ce qu'il contient reste, on ne sait pas combien.
                    self.note(error);
                    self.tally.left += 1;
                    return;
                }
            };
            if names.is_empty() {
                return;
            }
            for name in names {
                match self.entry(dir, &name, depth) {
                    Fate::Gone => {}
                    Fate::StuckLeaf => {
                        self.tally.left += 1;
                        stuck.insert(name);
                    }
                    Fate::StuckDir => {
                        stuck.insert(name);
                    }
                }
            }
            (self.progress)(self.tally.removed);
        }
        // Le service crée des entrées aussi vite qu'on les retire : on s'arrête, ce qui reste est dit.
        self.note(FsError::Refused);
        self.tally.left += 1;
    }
}

/// Les `BATCH` prochains noms du dossier, hors `.`, `..` et ceux que l'on sait coincés. Relit depuis le
/// début : retirer pendant qu'on lit fait sauter des entrées.
fn next_batch(dir: &OwnedFd, stuck: &HashSet<CString>) -> Result<Vec<CString>, FsError> {
    let mut reader = Dir::read_from(dir).map_err(fs_error)?;
    reader.rewind();
    let mut out = Vec::new();
    for entry in reader {
        let entry = entry.map_err(fs_error)?;
        let name = entry.file_name();
        if name.to_bytes() == b"." || name.to_bytes() == b".." || stuck.contains(name) {
            continue;
        }
        out.push(name.to_owned());
        if out.len() >= BATCH {
            break;
        }
    }
    Ok(out)
}

/// Abaisse la priorité du fil appelant : le plus bas niveau d'ordonnancement, que le noyau reprend pour
/// les entrées-sorties quand aucune classe n'est posée. Augmenter sa valeur de « nice » est toujours permis ;
/// le fil est dédié, il disparaît avec le geste. Un échec ne change rien au résultat.
pub(super) fn lower_priority() {
    let _ = rustix::process::setpriority_process(None, 19);
}
