//! `HostPaths` sur le système de fichiers de l'hôte (HRT-85, BR-DEL-008) : `canonicalize` et `stat`.

use std::path::Path;

use crate::application::ports::HostPaths;
use crate::domain::services::delete::FileId;

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemHostPaths;

impl HostPaths for SystemHostPaths {
    fn canonical(&self, path: &str) -> Option<String> {
        std::fs::canonicalize(Path::new(path))
            .ok()
            .and_then(|canonical| canonical.to_str().map(str::to_owned))
            // Windows (poste de développement) : `canonicalize` rend la forme `\?\C:\...` ; l'agent ne tourne
            // que sous Linux, mais ses tests tournent ici.
            .map(|canonical| match canonical.strip_prefix(r"\\?\") {
                Some(plain) => plain.to_owned(),
                None => canonical,
            })
    }

    #[cfg(unix)]
    fn identity(&self, path: &str) -> Option<FileId> {
        use std::os::unix::fs::MetadataExt;
        let meta = std::fs::metadata(Path::new(path)).ok()?;
        meta.is_dir().then(|| (meta.dev(), meta.ino()))
    }

    #[cfg(not(unix))]
    fn identity(&self, _path: &str) -> Option<FileId> {
        None
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use std::os::unix::fs::symlink;

    use super::*;

    #[test]
    fn a_link_has_the_canonical_path_and_the_identity_of_its_target() {
        let dir = crate::infrastructure::data_dir::private_tempdir();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let link = dir.path().join("link");
        symlink(&real, &link).unwrap();
        let paths = SystemHostPaths;
        let real_text = real.to_str().unwrap();
        let link_text = link.to_str().unwrap();
        assert_eq!(paths.canonical(link_text), paths.canonical(real_text));
        assert!(paths.identity(real_text).is_some());
        assert_eq!(paths.identity(link_text), paths.identity(real_text));
        assert_eq!(paths.canonical("/definitely/not/here"), None);
        assert_eq!(paths.identity("/definitely/not/here"), None);
    }
}
