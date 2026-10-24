//! Supprimer un service : ce qui est effacé, ce qui est gardé et pourquoi (HRT-85, BR-DEL-004 à BR-DEL-008).
//!
//! Fonctions pures : aucune lecture du disque ni du moteur. L'appelant donne la description du service, la
//! racine des dossiers gérés et ce que les AUTRES conteneurs montent ; le domaine répond, dossier par dossier,
//! s'il s'efface ou pourquoi il reste.
//!
//! **Le principe est de ne jamais perdre une donnée sans un choix explicite** : un doute vaut « conservé ».
//! - un dossier du serveur (monté depuis ailleurs que la racine de données de Hearth) n'est jamais effacé,
//!   même choisi au déploiement (BR-DEL-006) ;
//! - un dossier géré qu'un autre conteneur monte, ou dont un parent ou un enfant est monté par un autre
//!   conteneur, est conservé (BR-DEL-008) ;
//! - un dossier géré dont le nom ne se juge pas comme un composant de chemin sûr est conservé et signalé
//!   (BR-DEL-007 : « traite comme non géré tout dossier dont le chemin sort de la racine ») ;
//! - si un autre conteneur a plus de montages que l'agent n'en lit, on ne peut pas dire qu'il n'utilise pas le
//!   dossier : conservé (« non vérifiable »).

use super::spec::{SpecBody, SpecMountKind};
use super::visibility::Holder;
use crate::domain::files::RelPath;

/// Un dossier géré par Hearth pour ce service : un sous-dossier de `<racine>/<clé>/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedFolder {
    /// Le nom du sous-dossier, relatif à la racine du service (un seul composant, jugé).
    pub rel: RelPath,
    /// Où il est monté dans le service.
    pub target: String,
    /// Son chemin réel sur le serveur (`<racine>/<clé>/<rel>`), pour la fenêtre et la comparaison avec les
    /// montages des autres conteneurs. Jamais écrit dans le journal d'activité ni dans une trace.
    pub host_path: String,
}

/// Les montages de la description, rangés selon ce que la suppression peut en faire.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MountSplit {
    /// Les dossiers gérés par Hearth : candidats à l'effacement.
    pub managed: Vec<ManagedFolder>,
    /// Les dossiers du serveur montés dans le service : jamais effacés (BR-DEL-006). Leur chemin est celui du
    /// serveur.
    pub server_folders: Vec<String>,
    /// Des dossiers « gérés » dont le nom n'est pas un composant sûr : conservés, jamais touchés. Leur chemin
    /// (reconstitué) sert à les dire à la fenêtre.
    pub unmanaged: Vec<String>,
}

/// Range les montages de la description. `managed_root` : `<données>/services` (sans `/` final).
pub fn split_mounts(body: &SpecBody, managed_root: &str, key: &str) -> MountSplit {
    let root = managed_root.trim_end_matches('/');
    let mut split = MountSplit::default();
    for mount in &body.mounts {
        match mount.kind {
            SpecMountKind::HostFolder => split.server_folders.push(mount.source.clone()),
            SpecMountKind::Managed => {
                let host_path = format!("{root}/{key}/{}", mount.source);
                match single_component(&mount.source) {
                    Some(rel) => {
                        // Le même dossier monté deux fois (deux cibles) n'est effacé qu'une fois.
                        if !split.managed.iter().any(|folder| folder.rel == rel) {
                            split.managed.push(ManagedFolder {
                                rel,
                                target: mount.target.clone(),
                                host_path,
                            });
                        }
                    }
                    None => split.unmanaged.push(host_path),
                }
            }
        }
    }
    split
}

/// Un dossier géré s'écrit en UN composant (`BR-DEPLOY-010` : chemin aplati) : tout autre est refusé, aucun
/// `..` ni `/` ne passe.
fn single_component(source: &str) -> Option<RelPath> {
    let rel = RelPath::parse(source).ok()?;
    (rel.components().count() == 1).then_some(rel)
}

/// L'identité d'un dossier sur le serveur : (périphérique, inode). Deux chemins qui désignent le même dossier
/// (lien, montage « bind », `//`) ont la même identité.
pub type FileId = (u64, u64);

/// Ce qu'un AUTRE conteneur monte : ses dossiers du serveur (le chemin que le moteur donne, et sa forme
/// canonique quand l'appelant a pu la lire), leur identité, et si la liste est complète (l'agent n'en lit qu'un
/// nombre borné par conteneur).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderUse {
    /// Qui c'est, tel que la fenêtre peut le dire : un service publié par son nom, un conteneur créé hors de
    /// Hearth sans nom (BR-ADOPT-002).
    pub holder: Holder,
    /// Les chemins du serveur que ce conteneur monte.
    pub sources: Vec<String>,
    /// Les identités de ceux qui existent.
    pub identities: Vec<FileId>,
    /// La liste est entière. `false` : des montages ne sont pas lus, on ne sait pas.
    pub complete: bool,
}

/// Le dossier géré dont on juge le sort : ses chemins (celui de la description et sa forme canonique) et son
/// identité quand il existe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnFolder {
    pub paths: Vec<String>,
    pub identity: Option<FileId>,
}

/// Ce qu'il advient d'un dossier géré.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fate {
    /// Il s'efface (la case est cochée).
    Erase,
    /// Conservé : un autre conteneur l'utilise (BR-DEL-008). Les noms des services concernés ; un conteneur
    /// créé hors de Hearth est dit sans nom.
    Shared {
        services: Vec<String>,
        unmanaged: bool,
    },
    /// Conservé : on ne peut pas prouver qu'aucun autre conteneur ne l'utilise (liste des conteneurs absente ou
    /// pas prête, montages non lus).
    Unverifiable,
}

/// Un chemin ramené à sa forme lexicale : `//`, `/./` et `/..` résolus, sans `/` final. Ne lit pas le disque.
pub fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    format!("/{}", parts.join("/"))
}

/// Deux chemins du serveur se recouvrent-ils (le même, l'un dans l'autre) ?
fn overlaps(a: &str, b: &str) -> bool {
    let (a, b) = (normalize(a), normalize(b));
    let inside = |inner: &str, outer: &str| {
        outer == "/"
            || inner == outer
            || inner
                .strip_prefix(outer)
                .is_some_and(|rest| rest.starts_with('/'))
    };
    inside(&a, &b) || inside(&b, &a)
}

/// Le sort du dossier géré `own` d'après ce que les autres conteneurs montent (BR-DEL-008). `others` à `None` :
/// la liste des conteneurs n'est pas connue (moteur pas prêt) : une liste qu'on n'a pas est un doute, jamais un
/// feu vert.
pub fn fate_of(own: &OwnFolder, others: Option<&[FolderUse]>) -> Fate {
    let Some(others) = others else {
        return Fate::Unverifiable;
    };
    let mut services: Vec<String> = Vec::new();
    let mut unmanaged = false;
    let mut unverifiable = false;
    for other in others {
        let same_place = other
            .sources
            .iter()
            .any(|source| own.paths.iter().any(|path| overlaps(path, source)))
            || own
                .identity
                .is_some_and(|identity| other.identities.contains(&identity));
        if same_place {
            match &other.holder {
                Holder::Service(name) => {
                    if !services.contains(name) {
                        services.push(name.clone());
                    }
                }
                Holder::Unmanaged => unmanaged = true,
            }
        } else if !other.complete {
            unverifiable = true;
        }
    }
    if !services.is_empty() || unmanaged {
        services.sort_by_key(|name| name.to_lowercase());
        return Fate::Shared {
            services,
            unmanaged,
        };
    }
    if unverifiable {
        return Fate::Unverifiable;
    }
    Fate::Erase
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::spec::{SpecImage, SpecMount, SpecRestart, SpecSettings};

    fn body(mounts: Vec<(SpecMountKind, &str, &str)>) -> SpecBody {
        SpecBody {
            name: "jeu".into(),
            image: SpecImage {
                repository: "x".into(),
                tag: "1".into(),
                digest: None,
            },
            ports: Vec::new(),
            mounts: mounts
                .into_iter()
                .map(|(kind, source, target)| SpecMount {
                    kind,
                    source: source.into(),
                    target: target.into(),
                    read_only: false,
                })
                .collect(),
            restart: SpecRestart::UnlessStopped,
            stop_grace_s: None,
            stdin_open: false,
            cpu_millis: Some(500),
            memory_bytes: Some(1 << 28),
            settings: SpecSettings::default(),
            fiche: None,
        }
    }

    const ROOT: &str = "/var/lib/hearth/services";
    const KEY: &str = "h_01J0ABCDEFGHJKMNPQRSTVWXYZ";

    fn used_by(name: &str, sources: &[&str], complete: bool) -> FolderUse {
        FolderUse {
            holder: Holder::Service(name.into()),
            sources: sources.iter().map(|s| (*s).into()).collect(),
            identities: Vec::new(),
            complete,
        }
    }

    fn own(path: &str) -> OwnFolder {
        OwnFolder {
            paths: vec![path.to_owned()],
            identity: None,
        }
    }

    fn fate(path: &str, others: &[FolderUse]) -> Fate {
        fate_of(&own(path), Some(others))
    }

    #[test]
    fn managed_folders_are_split_from_server_folders_and_never_mixed() {
        let split = split_mounts(
            &body(vec![
                (SpecMountKind::Managed, "data", "/data"),
                (SpecMountKind::HostFolder, "/srv/media", "/media"),
                (SpecMountKind::Managed, "usr+share", "/usr/share"),
            ]),
            ROOT,
            KEY,
        );
        let managed: Vec<_> = split.managed.iter().map(|f| f.rel.to_string()).collect();
        assert_eq!(managed, ["data", "usr+share"]);
        assert_eq!(split.managed[0].host_path, format!("{ROOT}/{KEY}/data"));
        assert_eq!(split.server_folders, ["/srv/media"]);
        assert!(split.unmanaged.is_empty());
    }

    #[test]
    fn a_managed_name_that_climbs_or_nests_is_kept_as_unmanaged_never_erased() {
        let split = split_mounts(
            &body(vec![
                (SpecMountKind::Managed, "../escape", "/a"),
                (SpecMountKind::Managed, "a/b", "/b"),
                (SpecMountKind::Managed, "/abs", "/c"),
                (SpecMountKind::Managed, "", "/d"),
            ]),
            ROOT,
            KEY,
        );
        assert!(split.managed.is_empty(), "{:?}", split.managed);
        assert_eq!(split.unmanaged.len(), 4);
    }

    #[test]
    fn the_same_folder_mounted_twice_is_listed_once() {
        let split = split_mounts(
            &body(vec![
                (SpecMountKind::Managed, "data", "/data"),
                (SpecMountKind::Managed, "data", "/other"),
            ]),
            ROOT,
            KEY,
        );
        assert_eq!(split.managed.len(), 1);
    }

    #[test]
    fn a_folder_nobody_else_uses_is_erased() {
        let path = format!("{ROOT}/{KEY}/data");
        assert_eq!(fate(&path, &[]), Fate::Erase);
        assert_eq!(
            fate(&path, &[used_by("autre", &["/srv/elsewhere"], true)]),
            Fate::Erase
        );
    }

    #[test]
    fn the_same_folder_a_parent_or_a_child_used_by_another_container_is_kept() {
        let path = format!("{ROOT}/{KEY}/data");
        for source in [
            path.clone(),
            format!("{path}/"),
            format!("{path}/saves"),
            format!("{ROOT}/{KEY}"),
            ROOT.to_owned(),
            "/var/lib".to_owned(),
        ] {
            assert_eq!(
                fate(&path, &[used_by("sauvegarde", &[source.as_str()], true)]),
                Fate::Shared {
                    services: vec!["sauvegarde".into()],
                    unmanaged: false
                },
                "{source}"
            );
        }
    }

    #[test]
    fn a_sibling_whose_name_merely_starts_the_same_is_not_a_parent() {
        let path = format!("{ROOT}/{KEY}/data");
        assert_eq!(
            fate(&path, &[used_by("x", &[&format!("{path}-old")], true)]),
            Fate::Erase
        );
    }

    #[test]
    fn a_container_made_outside_hearth_is_said_without_a_name() {
        let path = format!("{ROOT}/{KEY}/data");
        let other = FolderUse {
            holder: Holder::Unmanaged,
            sources: vec![path.clone()],
            identities: Vec::new(),
            complete: true,
        };
        assert_eq!(
            fate(&path, &[other]),
            Fate::Shared {
                services: Vec::new(),
                unmanaged: true
            }
        );
    }

    #[test]
    fn services_sharing_the_folder_are_listed_once_in_name_order() {
        let path = format!("{ROOT}/{KEY}/data");
        let fate = fate(
            &path,
            &[
                used_by("zeta", &[&path], true),
                used_by("Alpha", &[&path], true),
                used_by("zeta", &[&path], true),
            ],
        );
        assert_eq!(
            fate,
            Fate::Shared {
                services: vec!["Alpha".into(), "zeta".into()],
                unmanaged: false
            }
        );
    }

    #[test]
    fn a_container_whose_mounts_are_not_all_read_blocks_the_erasure_when_nothing_proves_it_innocent()
     {
        let path = format!("{ROOT}/{KEY}/data");
        assert_eq!(
            fate(&path, &[used_by("gros", &["/srv/x"], false)]),
            Fate::Unverifiable
        );
        // Une utilisation prouvée l'emporte sur le doute.
        assert!(matches!(
            fate(
                &path,
                &[
                    used_by("gros", &["/srv/x"], false),
                    used_by("vu", &[&path], true)
                ]
            ),
            Fate::Shared { .. }
        ));
    }

    #[test]
    fn a_list_of_containers_we_do_not_have_is_a_doubt_never_a_green_light() {
        let path = format!("{ROOT}/{KEY}/data");
        assert_eq!(fate_of(&own(&path), None), Fate::Unverifiable);
    }

    #[test]
    fn paths_are_compared_in_their_lexical_form() {
        let path = format!("{ROOT}/{KEY}/data");
        for source in [
            format!("/var//lib/hearth/./services/{KEY}/data"),
            format!("/var/lib/hearth/services/x/../{KEY}/data/"),
            format!("/var/lib/hearth/services/{KEY}//data/saves"),
        ] {
            assert!(
                matches!(
                    fate(&path, &[used_by("a", &[source.as_str()], true)]),
                    Fate::Shared { .. }
                ),
                "{source}"
            );
        }
        assert_eq!(normalize("/a/../../b//c/."), "/b/c");
    }

    #[test]
    fn the_same_directory_under_another_name_is_found_by_its_identity() {
        // Un lien ou un montage « bind » : le chemin ne dit rien, le périphérique et l'inode oui.
        let mut folder = own("/var/lib/hearth/services/h_x/data");
        folder.identity = Some((7, 42));
        let mut other = used_by("alias", &["/mnt/elsewhere/looks-unrelated"], true);
        other.identities = vec![(7, 42)];
        assert!(matches!(
            fate_of(&folder, Some(&[other.clone()])),
            Fate::Shared { .. }
        ));
        other.identities = vec![(7, 43)];
        assert_eq!(fate_of(&folder, Some(&[other])), Fate::Erase);
    }

    #[test]
    fn the_canonical_form_of_our_folder_is_compared_too() {
        let folder = OwnFolder {
            paths: vec![
                "/srv/link/services/h_x/data".into(),
                "/real/hearth/services/h_x/data".into(),
            ],
            identity: None,
        };
        let other = used_by("b", &["/real/hearth/services/h_x"], true);
        assert!(matches!(
            fate_of(&folder, Some(&[other])),
            Fate::Shared { .. }
        ));
    }
}
