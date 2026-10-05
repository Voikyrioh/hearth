//! Adaptateurs du port `ServiceManager` : `systemd` (une unité écrite par l'agent) et `none`
//! (installation gérée : l'agent n'écrit rien).

mod none;
mod systemd;

pub use none::Unmanaged;
pub use systemd::{Systemd, UNIT_NAME, render_unit};
