//! `HostPaths` sur le système de fichiers de l'hôte (HRT-85, BR-DEL-008) : `canonicalize` et `stat` du chemin et de
//! tous ses parents, chacun borné dans le temps (un partage réseau mort ne fait pas pendre une suppression).

use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use crate::application::ports::{HostPaths, Resolved};
use crate::domain::services::delete::FileId;

/// Le temps laissé à la lecture d'un chemin (parents compris).
const READ_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemHostPaths;

impl HostPaths for SystemHostPaths {
    fn resolve(&self, path: &str) -> Resolved {
        let owned = path.to_owned();
        within(READ_TIMEOUT, move || read(&owned)).unwrap_or(Resolved::Unreadable)
    }
}

/// Fait `work` sur un fil à part et n'attend que `timeout`. `None` : pas de réponse à temps (le fil reste pris
/// par l'appel bloqué, l'agent ne l'attend plus).
fn within<T: Send + 'static>(
    timeout: Duration,
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (sender, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("host-paths".into())
        .spawn(move || {
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
        let answer = within(Duration::from_millis(100), || {
            std::thread::sleep(Duration::from_secs(5));
            1
        });
        assert_eq!(answer, None);
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(within(Duration::from_secs(5), || 7), Some(7));
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
