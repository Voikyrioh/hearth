//! Primitives par descripteur de dossier, communes aux adaptateurs qui écrivent dans un contenu qu'ils ne
//! maîtrisent pas : l'extraction d'une archive (`archive_fs`, HRT-130, ADR-0055) et l'accès aux fichiers
//! d'un service (`fs`, HRT-145, ADR-0046). Unix seulement.

use std::os::fd::OwnedFd;

use rustix::fs::{Gid, Mode, OFlags, Stat, Uid, fchmod, fchown};
use rustix::io::Errno;
use rustix::process::geteuid;

/// Ouvrir un dossier sans suivre un lien à son dernier composant.
pub(crate) const OPEN_DIR: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

/// Créer un fichier NEUF, sans suivre un lien.
pub(crate) const CREATE_FILE: OFlags = OFlags::WRONLY
    .union(OFlags::CREATE)
    .union(OFlags::EXCL)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

pub(crate) fn mode(bits: u32) -> Mode {
    Mode::from_bits_truncate(bits)
}

/// Pose le propriétaire puis les droits donnés par l'appelant, sur un descripteur : jamais le masque
/// ambiant (`umask`), jamais ce que dit le contenu.
pub(crate) fn apply(fd: &OwnedFd, uid: u32, gid: u32, bits: u32) -> Result<(), Errno> {
    fchown(fd, Some(Uid::from_raw(uid)), Some(Gid::from_raw(gid)))?;
    fchmod(fd, mode(bits))
}

/// Un dossier de confiance : à l'utilisateur EFFECTIF de l'agent, ni le groupe ni les autres n'y
/// écrivent (sinon un autre compte pourrait y substituer un lien entre deux appels).
pub(crate) fn is_trusted(stat: &Stat) -> bool {
    stat.st_uid == geteuid().as_raw() && stat.st_mode & 0o022 == 0
}
