//! `ids.lock` : le registre des identifiants de réglage jamais publiés (conception technique 12.4).
//!
//! Un identifiant publié ne disparaît pas par oubli : retiré de `[[setting]]`, il passe en `[[retired]]`
//! avec sa raison, et son état dans `ids.lock` devient `retired`. `cargo xtask catalog-check` applique
//! ces règles ; elles sont ici, en fonctions pures, pour être testées sans disque.
//!
//! Forme du fichier (une ligne par identifiant, TOML) :
//!
//! ```toml
//! ids = [
//!   { id = "world_name", key = "worldname", channel = "config", state = "active", since = 1 },
//!   { id = "old", key = "OldKey", channel = "config", state = "retired", since = 1, retired_in = 3 },
//! ]
//! ```
//!
//! Une opération `rename = [ancien, nouveau]` d'un `[[migration]]` déclare soit le changement de CLÉ du jeu
//! d'un identifiant conservé, soit le changement d'IDENTIFIANT d'un réglage (l'ancien disparaît de
//! `[[setting]]` sans passer par `[[retired]]`, le nouveau existe).

use std::collections::BTreeSet;
use std::fmt;

use serde::Deserialize;

use super::error::{FicheRefusee, Fichier, Motif};
use super::limits::MAX_FILES_BYTES;
use super::model::Fiche;
use super::schema::fiche::MigrationOp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockState {
    Active,
    Retired,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedId {
    pub id: String,
    pub key: String,
    pub channel: String,
    pub state: LockState,
    pub since: u32,
    pub retired_in: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdsLock {
    #[serde(default)]
    pub ids: Vec<LockedId>,
}

/// Ce que `catalog-check` reproche à une fiche.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    /// L'identifiant est dans `ids.lock` mais plus ni dans `[[setting]]` ni dans `[[retired]]`.
    Disappeared { id: String },
    /// La clé du jeu a changé sans `rename` d'un `[[migration]]`.
    KeyChanged {
        id: String,
        was: String,
        now: String,
    },
    /// Le canal du réglage a changé.
    ChannelChanged { id: String },
    /// Un identifiant retiré dans `ids.lock` réapparaît en `[[setting]]`.
    Reused { id: String },
    /// Retiré dans la fiche, mais `ids.lock` le dit encore actif.
    LockOutdated { id: String },
    /// Présent dans la fiche mais absent de `ids.lock` : la ligne à ajouter est donnée.
    NotLocked { id: String, line: String },
    /// `ids.lock` est incohérent avec lui-même ou avec la version de la fiche.
    BadLock { id: String, why: &'static str },
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disappeared { id } => write!(
                f,
                "l'identifiant `{id}` est dans ids.lock mais n'est plus ni un [[setting]] ni un [[retired]] (un réglage ne se retire pas par oubli : écris-le en [[retired]] avec sa raison)"
            ),
            Self::KeyChanged { id, was, now } => write!(
                f,
                "la clé de `{id}` est passée de `{was}` à `{now}` sans `rename = [\"{was}\", \"{now}\"]` dans un [[migration]]"
            ),
            Self::ChannelChanged { id } => write!(f, "le canal de `{id}` a changé depuis ids.lock"),
            Self::Reused { id } => write!(
                f,
                "l'identifiant `{id}` a été retiré (ids.lock) puis réutilisé : un identifiant retiré ne se réutilise jamais"
            ),
            Self::LockOutdated { id } => write!(
                f,
                "`{id}` est en [[retired]] mais ids.lock le dit encore `active` : passe son état à `retired` avec `retired_in`"
            ),
            Self::NotLocked { id, line } => {
                write!(
                    f,
                    "l'identifiant `{id}` n'est pas dans ids.lock ; ligne à ajouter : {line}"
                )
            }
            Self::BadLock { id, why } => write!(f, "ids.lock, `{id}` : {why}"),
        }
    }
}

pub fn parse(bytes: &[u8]) -> Result<IdsLock, FicheRefusee> {
    if bytes.len() > MAX_FILES_BYTES {
        return Err(FicheRefusee::new(
            Fichier::IdsLock,
            "",
            Motif::BorneDepassee {
                quoi: "taille de ids.lock (octets)",
                limite: MAX_FILES_BYTES,
                trouve: bytes.len(),
            },
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| {
        FicheRefusee::new(
            Fichier::IdsLock,
            "",
            Motif::Syntaxe {
                detail: "ids.lock n'est pas de l'UTF-8 valide".into(),
            },
        )
    })?;
    toml::from_str::<IdsLock>(text).map_err(|e| super::loader::map_toml(Fichier::IdsLock, text, &e))
}

fn lock_line(
    id: &str,
    key: &str,
    channel: &str,
    state: &str,
    since: u32,
    retired_in: Option<u32>,
) -> String {
    let extra = retired_in
        .map(|v| format!(", retired_in = {v}"))
        .unwrap_or_default();
    format!(
        "{{ id = \"{id}\", key = \"{key}\", channel = \"{channel}\", state = \"{state}\", since = {since}{extra} }},"
    )
}

/// Les règles de `ids.lock` pour une fiche chargée. Liste vide : rien à reprocher.
pub fn check(lock: &IdsLock, fiche: &Fiche) -> Vec<Violation> {
    let mut out = Vec::new();
    let renames: Vec<(&str, &str)> = fiche
        .file
        .migration
        .iter()
        .flat_map(|m| m.ops.iter())
        .filter_map(|op| match op {
            MigrationOp::Rename([from, to]) => Some((from.as_str(), to.as_str())),
            _ => None,
        })
        .collect();

    let mut seen = BTreeSet::new();
    for locked in &lock.ids {
        if !seen.insert(locked.id.as_str()) {
            out.push(Violation::BadLock {
                id: locked.id.clone(),
                why: "identifiant en double",
            });
            continue;
        }
        if locked.since == 0 || locked.since > fiche.file.version {
            out.push(Violation::BadLock {
                id: locked.id.clone(),
                why: "`since` : de 1 à la version de la fiche",
            });
        }
        match (locked.state, locked.retired_in) {
            (LockState::Retired, None) => {
                out.push(Violation::BadLock {
                    id: locked.id.clone(),
                    why: "un identifiant retiré porte `retired_in`",
                });
            }
            (LockState::Active, Some(_)) => {
                out.push(Violation::BadLock {
                    id: locked.id.clone(),
                    why: "`retired_in` n'a de sens qu'avec `state = \"retired\"`",
                });
            }
            _ => {}
        }
        let setting = fiche.setting(&locked.id);
        let retired = fiche.retired_setting(&locked.id);
        match (locked.state, setting, retired) {
            (LockState::Retired, Some(_), _) => out.push(Violation::Reused {
                id: locked.id.clone(),
            }),
            (LockState::Active, Some(s), _) => {
                if s.channel != locked.channel {
                    out.push(Violation::ChannelChanged {
                        id: locked.id.clone(),
                    });
                }
                if s.key != locked.key && !renames.contains(&(locked.key.as_str(), s.key.as_str()))
                {
                    out.push(Violation::KeyChanged {
                        id: locked.id.clone(),
                        was: locked.key.clone(),
                        now: s.key.clone(),
                    });
                }
            }
            (LockState::Active, None, Some(_)) => out.push(Violation::LockOutdated {
                id: locked.id.clone(),
            }),
            (LockState::Retired, None, Some(_)) => {}
            (_, None, None) => {
                // Un identifiant renommé (`rename = [ancien, nouveau]`) peut quitter `[[setting]]`.
                let renamed = renames
                    .iter()
                    .any(|(from, to)| *from == locked.id && fiche.setting(to).is_some());
                if !renamed {
                    out.push(Violation::Disappeared {
                        id: locked.id.clone(),
                    });
                }
            }
        }
    }
    for setting in &fiche.settings {
        if !lock.ids.iter().any(|l| l.id == setting.id) {
            out.push(Violation::NotLocked {
                id: setting.id.clone(),
                line: lock_line(
                    &setting.id,
                    &setting.key,
                    &setting.channel,
                    "active",
                    fiche.file.version,
                    None,
                ),
            });
        }
    }
    for retired in &fiche.retired {
        if !lock.ids.iter().any(|l| l.id == retired.id) {
            out.push(Violation::NotLocked {
                id: retired.id.clone(),
                line: lock_line(
                    &retired.id,
                    &retired.key,
                    &retired.channel,
                    "retired",
                    1,
                    Some(fiche.file.version),
                ),
            });
        }
    }
    out
}
