//! La compatibilité d'un mod avec un serveur (HRT-254, BR-MOD-021) : domaine pur, jugée sur ce que le mod dit de
//! lui-même, jamais devinée.

use std::collections::HashSet;

use super::jar::{JarIdentity, JarLoader, ModId};
use super::version_req::{Containment, GameVersion};

/// Le serveur jugé : son moteur (`None` pour Vanilla, Paper, Purpur, qui ne chargent aucun mod) et la version du
/// jeu qui tourne (`None` si inconnue, ou si ce n'est pas une suite de nombres).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerProfile {
    pub loader: Option<JarLoader>,
    pub minecraft: Option<GameVersion>,
}

/// Le verdict sur un mod. Seuls `IncompatibleLoader`, `IncompatibleMinecraft` et `Unrecognized` empêchent d'ajouter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Compatible,
    /// Version du serveur inconnue, ou plage que Hearth ne comprend pas : n'empêche pas d'appliquer.
    Unverified,
    /// Dépendances obligatoires absentes (nommées) : n'empêche pas d'appliquer.
    ToComplete(Vec<ModId>),
    IncompatibleLoader {
        jar: Vec<JarLoader>,
        server: Option<JarLoader>,
    },
    IncompatibleMinecraft,
    /// Deux fichiers actifs du même mod : le jeu refuse de démarrer.
    Duplicate,
    /// Fiche d'identité illisible.
    Unrecognized,
}

impl Verdict {
    /// Ce verdict empêche-t-il d'ajouter le mod ?
    pub fn blocks_adding(&self) -> bool {
        matches!(
            self,
            Self::IncompatibleLoader { .. } | Self::IncompatibleMinecraft | Self::Unrecognized
        )
    }
}

/// Un fichier à juger : ses identités lues et son état.
#[derive(Debug, Clone, Copy)]
pub struct Candidate<'a> {
    pub identities: &'a [JarIdentity],
    pub enabled: bool,
}

/// Le moteur du serveur accepte-t-il un mod écrit pour `jar` ? Quilt charge les mods Fabric ; Forge et NeoForge
/// ne se chargent pas entre eux.
pub fn accepts(server: JarLoader, jar: JarLoader) -> bool {
    server == jar || (server == JarLoader::Quilt && jar == JarLoader::Fabric)
}

/// L'identité qui compte pour ce serveur : celle de `quilt.mod.json` pour Quilt (sinon Fabric), et celle du
/// moteur lui-même ailleurs.
pub fn choose(identities: &[JarIdentity], server: JarLoader) -> Option<&JarIdentity> {
    let by = |loader: JarLoader| identities.iter().find(|i| i.loader == loader);
    match server {
        JarLoader::Quilt => by(JarLoader::Quilt).or_else(|| by(JarLoader::Fabric)),
        other => by(other),
    }
}

/// Juge un lot de fichiers ensemble (doublons et dépendances demandent de voir les autres). Rend un verdict par
/// fichier, dans l'ordre. Un fichier désactivé ne compte ni pour les dépendances ni pour « Installé deux fois ».
pub fn judge_batch(profile: &ServerProfile, items: &[Candidate<'_>]) -> Vec<Verdict> {
    let chosen: Vec<Option<&JarIdentity>> = items
        .iter()
        .map(|item| match profile.loader {
            Some(server) => choose(item.identities, server),
            None => None,
        })
        .collect();
    // Ce que les fichiers actifs fournissent : leur identifiant et ce qu'ils déclarent fournir.
    let mut present: HashSet<&ModId> = HashSet::new();
    for (item, identity) in items.iter().zip(&chosen) {
        if let (true, Some(identity)) = (item.enabled, identity) {
            present.insert(&identity.id);
            present.extend(identity.provides.iter());
        }
    }
    items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            if item.identities.is_empty() {
                return Verdict::Unrecognized;
            }
            let Some(server) = profile.loader else {
                return Verdict::IncompatibleLoader {
                    jar: item.identities.iter().map(|id| id.loader).collect(),
                    server: None,
                };
            };
            let Some(identity) = chosen[i] else {
                return Verdict::IncompatibleLoader {
                    jar: item.identities.iter().map(|id| id.loader).collect(),
                    server: Some(server),
                };
            };
            if identity.minecraft_declared
                && let Some(req) = &identity.minecraft
                && req.contains(profile.minecraft.as_ref()) == Containment::No
            {
                return Verdict::IncompatibleMinecraft;
            }
            if item.enabled {
                let twin = items
                    .iter()
                    .zip(&chosen)
                    .enumerate()
                    .any(|(j, (other, id))| {
                        j != i && other.enabled && id.is_some_and(|o| o.id == identity.id)
                    });
                if twin {
                    return Verdict::Duplicate;
                }
            }
            let missing: Vec<ModId> = identity
                .depends
                .iter()
                .filter(|dep| !present.contains(&dep.id))
                .map(|dep| dep.id.clone())
                .collect();
            if !missing.is_empty() {
                return Verdict::ToComplete(missing);
            }
            let understood = identity.minecraft_declared
                && identity.minecraft.is_some()
                && profile.minecraft.is_some();
            if understood {
                Verdict::Compatible
            } else {
                Verdict::Unverified
            }
        })
        .collect()
}

/// Les mods déjà là, jugés ensemble.
pub fn judge_installed(profile: &ServerProfile, installed: &[Candidate<'_>]) -> Vec<Verdict> {
    judge_batch(profile, installed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::mods::jar::{Dependency, Environment, ModVersion};
    use crate::domain::mods::version_req::VersionReq;

    fn id(text: &str) -> ModId {
        ModId::parse(text).unwrap()
    }

    fn mod_for(loader: JarLoader, name: &str, mc: Option<&str>, deps: &[&str]) -> JarIdentity {
        JarIdentity {
            loader,
            id: id(name),
            name: None,
            version: ModVersion::parse("1.0"),
            minecraft: mc.and_then(|r| VersionReq::parse_fabric(&[r])),
            minecraft_declared: mc.is_some(),
            depends: deps
                .iter()
                .map(|d| Dependency {
                    id: id(d),
                    range_understood: true,
                })
                .collect(),
            provides: vec![],
            environment: Environment::Unknown,
        }
    }

    fn profile(loader: Option<JarLoader>, mc: Option<&str>) -> ServerProfile {
        ServerProfile {
            loader,
            minecraft: mc.and_then(GameVersion::parse),
        }
    }

    fn one(profile: &ServerProfile, identities: &[JarIdentity]) -> Verdict {
        judge_batch(
            profile,
            &[Candidate {
                identities,
                enabled: true,
            }],
        )
        .remove(0)
    }

    #[test]
    fn the_engine_decides_who_goes_where() {
        use JarLoader::*;
        let table = [
            (Fabric, Fabric, true),
            (Quilt, Fabric, true),
            (Quilt, Quilt, true),
            (Fabric, Quilt, false),
            (Forge, Forge, true),
            (Forge, NeoForge, false),
            (NeoForge, Forge, false),
            (NeoForge, NeoForge, true),
        ];
        for (server, jar, want) in table {
            assert_eq!(accepts(server, jar), want, "{server:?} {jar:?}");
        }
        let both = [
            mod_for(Fabric, "a", Some("*"), &[]),
            mod_for(Quilt, "a", Some("*"), &[]),
        ];
        assert_eq!(choose(&both, Quilt).unwrap().loader, Quilt);
        assert_eq!(choose(&both[..1], Quilt).unwrap().loader, Fabric);
        assert!(choose(&both[1..], Fabric).is_none());
    }

    #[test]
    fn verdicts_in_a_table() {
        let fabric = Some(JarLoader::Fabric);
        let m = mod_for(JarLoader::Fabric, "m", Some("~1.20.1"), &[]);
        assert_eq!(
            one(&profile(fabric, Some("1.20.4")), std::slice::from_ref(&m)),
            Verdict::Compatible
        );
        assert_eq!(
            one(&profile(fabric, Some("1.21")), std::slice::from_ref(&m)),
            Verdict::IncompatibleMinecraft
        );
        assert_eq!(
            one(&profile(fabric, None), std::slice::from_ref(&m)),
            Verdict::Unverified
        );
        assert_eq!(
            one(&profile(fabric, Some("24w14a")), std::slice::from_ref(&m)),
            Verdict::Unverified
        );
        assert_eq!(
            one(&profile(fabric, Some("1.20.1")), &[]),
            Verdict::Unrecognized
        );
        assert_eq!(
            one(
                &profile(Some(JarLoader::Forge), Some("1.20.1")),
                std::slice::from_ref(&m)
            ),
            Verdict::IncompatibleLoader {
                jar: vec![JarLoader::Fabric],
                server: Some(JarLoader::Forge)
            }
        );
        assert!(matches!(
            one(&profile(None, Some("1.20.1")), std::slice::from_ref(&m)),
            Verdict::IncompatibleLoader { server: None, .. }
        ));
        let not_understood = mod_for(JarLoader::Fabric, "n", Some("snap"), &[]);
        assert_eq!(
            one(&profile(fabric, Some("1.20.1")), &[not_understood]),
            Verdict::Unverified
        );
        assert!(Verdict::IncompatibleMinecraft.blocks_adding());
        assert!(!Verdict::ToComplete(vec![]).blocks_adding());
        assert!(!Verdict::Duplicate.blocks_adding());
    }

    #[test]
    fn dependencies_and_duplicates_look_at_the_other_files() {
        let p = profile(Some(JarLoader::Fabric), Some("1.20.1"));
        let needs = [mod_for(JarLoader::Fabric, "a", Some("*"), &["lib"])];
        let mut lib = mod_for(JarLoader::Fabric, "lib", Some("*"), &[]);
        let sets: [(&[JarIdentity], bool); 0] = [];
        let _ = sets;
        // Absente : « À compléter ».
        assert_eq!(
            judge_batch(
                &p,
                &[Candidate {
                    identities: &needs,
                    enabled: true
                }]
            ),
            vec![Verdict::ToComplete(vec![id("lib")])]
        );
        // Présente et active : compatible.
        let libs = [lib.clone()];
        let both = judge_batch(
            &p,
            &[
                Candidate {
                    identities: &needs,
                    enabled: true,
                },
                Candidate {
                    identities: &libs,
                    enabled: true,
                },
            ],
        );
        assert_eq!(both, vec![Verdict::Compatible, Verdict::Compatible]);
        // Désactivée : ne compte pas.
        let off = judge_batch(
            &p,
            &[
                Candidate {
                    identities: &needs,
                    enabled: true,
                },
                Candidate {
                    identities: &libs,
                    enabled: false,
                },
            ],
        );
        assert_eq!(off[0], Verdict::ToComplete(vec![id("lib")]));
        // Fournie par `provides`.
        lib.id = id("other");
        lib.provides = vec![id("lib")];
        let provider = [lib];
        let provided = judge_batch(
            &p,
            &[
                Candidate {
                    identities: &needs,
                    enabled: true,
                },
                Candidate {
                    identities: &provider,
                    enabled: true,
                },
            ],
        );
        assert_eq!(provided[0], Verdict::Compatible);
        // Installé deux fois : les deux lignes ; un doublon désactivé n'en est pas un.
        let a1 = [mod_for(JarLoader::Fabric, "dup", Some("*"), &[])];
        let a2 = [mod_for(JarLoader::Fabric, "dup", Some("*"), &[])];
        let twice = judge_batch(
            &p,
            &[
                Candidate {
                    identities: &a1,
                    enabled: true,
                },
                Candidate {
                    identities: &a2,
                    enabled: true,
                },
            ],
        );
        assert_eq!(twice, vec![Verdict::Duplicate, Verdict::Duplicate]);
        let spare = judge_batch(
            &p,
            &[
                Candidate {
                    identities: &a1,
                    enabled: true,
                },
                Candidate {
                    identities: &a2,
                    enabled: false,
                },
            ],
        );
        assert_eq!(spare, vec![Verdict::Compatible, Verdict::Compatible]);
        assert_eq!(judge_installed(&p, &[]), vec![]);
    }
}
