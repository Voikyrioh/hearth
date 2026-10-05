//! Adaptateurs de la mise à jour de l'agent (HRT-17) : téléchargement HTTPS, signature minisign,
//! dossier `update/` et lancement du superviseur, diffusion de la progression.

mod download;
mod feed;
mod host;
mod minisign;

pub use download::HttpsDownloader;
pub use feed::BroadcastUpdateFeed;
pub use host::{FsUpdateHost, Launcher, SUPERVISOR_UNIT};
pub use minisign::{EMBEDDED_PUBLIC_KEY, MinisignVerifier};
