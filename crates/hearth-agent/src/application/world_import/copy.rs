//! Copier un monde et vérifier la copie, fichier par fichier (HRT-105, `BR-MONDE-004`, `009`, `010`, `011`).
//!
//! La source n'est ouverte qu'en lecture ([`WorldSource`]). Chaque fichier est lu en flux (jamais entier en
//! mémoire), son empreinte SHA-256 calculée à la lecture ; sa taille et sa date sont relevées avant et après
//! (`BR-MONDE-009`). Puis CHAQUE fichier copié est relu dans la destination et son empreinte comparée à celle de
//! la source (`BR-MONDE-010`). Fonction synchrone : l'appelant la fait tourner sur un fil dédié.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use sha2::{Digest, Sha256};

use super::search::Plan;
use crate::application::ports::{WorldFsError, WorldSink, WorldSource};

const CHUNK: usize = 256 * 1024;

/// Ce que l'appelant lit pendant la copie et ce qu'il peut lui demander.
#[derive(Debug, Default)]
pub struct Control {
    /// Demande d'arrêt (annulation, arrêt de l'agent).
    pub cancel: AtomicBool,
    pub files_done: AtomicU64,
    pub bytes_done: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyError {
    Cancelled,
    /// Le plafond de six heures est atteint (`BR-MONDE-011`).
    TimedOut,
    Source(WorldFsError),
    Sink(WorldFsError),
    /// Un fichier copié est absent ou différent de la source (`BR-MONDE-010`).
    Incomplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyReport {
    pub files: u64,
    pub bytes: u64,
    /// Fichiers dont la taille ou la date a changé pendant la copie, ou disparus (`BR-MONDE-009`).
    pub changed: u64,
}

fn io_error(error: &std::io::Error) -> WorldFsError {
    WorldFsError::Io(error.kind())
}

fn check(control: &Control, deadline: Instant) -> Result<(), CopyError> {
    if control.cancel.load(Ordering::SeqCst) {
        return Err(CopyError::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(CopyError::TimedOut);
    }
    Ok(())
}

pub fn copy_world(
    source: &dyn WorldSource,
    sink: &dyn WorldSink,
    plan: &Plan,
    control: &Control,
    deadline: Instant,
) -> Result<CopyReport, CopyError> {
    for dir in &plan.dirs {
        check(control, deadline)?;
        sink.mkdir(dir).map_err(CopyError::Sink)?;
    }
    // L'empreinte de chaque fichier copié ; `None` : le fichier a changé ou disparu, il n'est pas vérifié.
    let mut digests: Vec<Option<([u8; 32], u64)>> = Vec::with_capacity(plan.files.len());
    let mut report = CopyReport {
        files: 0,
        bytes: 0,
        changed: 0,
    };
    let mut buffer = vec![0u8; CHUNK];
    for file in &plan.files {
        check(control, deadline)?;
        let before = match source.stat(&file.src) {
            Ok(stat) => stat,
            Err(WorldFsError::NotFound) => {
                report.changed += 1;
                digests.push(None);
                continue;
            }
            Err(error) => return Err(CopyError::Source(error)),
        };
        let mut reader = match source.open(&file.src) {
            Ok(reader) => reader,
            Err(WorldFsError::NotFound) => {
                report.changed += 1;
                digests.push(None);
                continue;
            }
            Err(error) => return Err(CopyError::Source(error)),
        };
        let mut writer = sink.create(&file.dst).map_err(CopyError::Sink)?;
        let mut hasher = Sha256::new();
        let mut read_total = 0u64;
        loop {
            check(control, deadline)?;
            let n = reader
                .read(&mut buffer)
                .map_err(|e| CopyError::Source(io_error(&e)))?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
            writer
                .write_all(&buffer[..n])
                .map_err(|e| CopyError::Sink(io_error(&e)))?;
            read_total += n as u64;
            control.bytes_done.fetch_add(n as u64, Ordering::SeqCst);
        }
        writer.finish().map_err(|e| CopyError::Sink(io_error(&e)))?;
        drop(reader);
        let after = source.stat(&file.src).ok();
        let unchanged = after
            .is_some_and(|after| after.size == before.size && after.mtime_ns == before.mtime_ns)
            && read_total == before.size;
        report.files += 1;
        report.bytes += read_total;
        control.files_done.fetch_add(1, Ordering::SeqCst);
        if unchanged {
            digests.push(Some((hasher.finalize().into(), read_total)));
        } else {
            report.changed += 1;
            digests.push(None);
        }
    }
    // La relecture de la copie : chaque fichier, taille et empreinte (`BR-MONDE-010`).
    for (file, digest) in plan.files.iter().zip(&digests) {
        let Some((expected, size)) = digest else {
            continue;
        };
        check(control, deadline)?;
        let mut reader = sink.open(&file.dst).map_err(|error| match error {
            WorldFsError::NotFound => CopyError::Incomplete,
            other => CopyError::Sink(other),
        })?;
        let mut hasher = Sha256::new();
        let mut total = 0u64;
        loop {
            check(control, deadline)?;
            let n = reader
                .read(&mut buffer)
                .map_err(|e| CopyError::Sink(io_error(&e)))?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
            total += n as u64;
        }
        let copied: [u8; 32] = hasher.finalize().into();
        if copied != *expected || total != *size {
            return Err(CopyError::Incomplete);
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::super::search::fake::{Node, Sink, Tree};
    use super::super::search::{PlanFile, plan_world};
    use super::*;
    use crate::domain::files::RelPath;
    use crate::domain::worlds::{Shape, WorldRules};

    fn rules() -> WorldRules {
        WorldRules {
            shape: Shape::File {
                suffix: ".wld".into(),
                companions: vec![],
            },
            names_setting: "world_name".into(),
            carried: vec![],
            folder_source: "w".into(),
        }
    }

    fn setup() -> (Tree, Plan) {
        let tree = Tree::default();
        tree.put("W", Node::Dir);
        tree.put_file("W/A.wld", &vec![7u8; 600_000], 5);
        let plan = plan_world(
            &tree,
            &rules(),
            &RelPath::parse("W/A.wld").expect("rel"),
            Duration::from_secs(60),
        )
        .expect("plan");
        (tree, plan)
    }

    fn far() -> Instant {
        Instant::now() + Duration::from_secs(60)
    }

    #[test]
    fn a_copy_is_read_back_and_leaves_the_source_as_it_was() {
        let (tree, plan) = setup();
        let before = tree.snapshot();
        let sink = Sink::default();
        let control = Control::default();
        let report = copy_world(&tree, &sink, &plan, &control, far()).expect("copie");
        assert_eq!(
            report,
            CopyReport {
                files: 1,
                bytes: 600_000,
                changed: 0
            }
        );
        assert_eq!(control.bytes_done.load(Ordering::SeqCst), 600_000);
        assert_eq!(sink.tree.snapshot().len(), 1);
        assert_eq!(tree.snapshot(), before, "la source n'a pas bougé");
    }

    #[test]
    fn a_copy_that_differs_from_the_source_is_incomplete() {
        let (tree, plan) = setup();
        let sink = Sink {
            corrupt: true,
            ..Sink::default()
        };
        let outcome = copy_world(&tree, &sink, &plan, &Control::default(), far());
        assert_eq!(outcome, Err(CopyError::Incomplete));
    }

    #[test]
    fn a_file_that_vanishes_is_noted_changed_not_failed() {
        let (tree, mut plan) = setup();
        plan.files.push(PlanFile {
            src: RelPath::parse("W/gone.wld").expect("rel"),
            dst: RelPath::parse("gone.wld").expect("rel"),
            size: 1,
            mtime_ns: 0,
        });
        let report =
            copy_world(&tree, &Sink::default(), &plan, &Control::default(), far()).expect("copie");
        assert_eq!((report.files, report.changed), (1, 1));
    }

    #[test]
    fn a_cancel_or_a_past_deadline_stops_the_copy() {
        let (tree, plan) = setup();
        let control = Control::default();
        control.cancel.store(true, Ordering::SeqCst);
        assert_eq!(
            copy_world(&tree, &Sink::default(), &plan, &control, far()),
            Err(CopyError::Cancelled)
        );
        assert_eq!(
            copy_world(
                &tree,
                &Sink::default(),
                &plan,
                &Control::default(),
                Instant::now()
            ),
            Err(CopyError::TimedOut)
        );
    }
}
