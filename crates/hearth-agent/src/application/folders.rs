//! Le jugement d'un dossier à monter dans un service (HRT-75, BR-DEPLOY-012).
//!
//! Le domaine ([`crate::domain::services::folders`]) tient la règle ; ici on lui donne ce que le serveur
//! sait À CE MOMENT : les liens symboliques, la liste des comptes. Rien n'est gardé d'un appel à l'autre.
//! La liste des comptes est relue à chaque vérification (BR-DEPLOY-012 : un compte ajouté après le
//! démarrage est couvert) ; si elle est illisible, le dossier est refusé.
//!
//! Cette liste de refus est une CEINTURE ; la barrière est la liste d'autorisation des dossiers
//! ouverts aux services (HRT-70).

use super::ports::HostFolders;
use crate::domain::services::{
    ACCOUNT_SECRET_FOLDERS, FolderPath, FolderPolicy, FolderRefusal, Probe, resolve,
};

/// Ce que l'agent sait de lui-même et de son moteur, en chemins absolus (BR-DEPLOY-012).
#[derive(Debug, Clone, Default)]
pub struct FolderFacts {
    /// Le socket de communication du moteur.
    pub engine_socket: String,
    /// Le dossier de données du moteur.
    pub engine_data_dir: String,
    /// La racine de données Hearth.
    pub hearth_data_root: String,
    /// Données, programme et configuration de l'agent.
    pub agent_paths: Vec<String>,
}

/// Construit la liste des dossiers refusés telle qu'elle est maintenant : la liste fermée du système, les
/// dossiers de l'agent et du moteur, `.ssh` et `.gnupg` de chaque compte. Chaque chemin est protégé sous
/// sa forme écrite ET sous sa forme résolue (liens symboliques). `None` : les comptes n'ont pas pu être
/// lus.
pub fn build_policy(host: &dyn HostFolders, facts: &FolderFacts) -> Option<FolderPolicy> {
    let homes = host.account_homes()?;
    let mut policy = FolderPolicy::system();
    let mut probe = |path: &str| host.probe(path);
    for path in [&facts.engine_socket, &facts.engine_data_dir]
        .into_iter()
        .chain(&facts.agent_paths)
    {
        for form in forms(path, &mut probe) {
            policy.protect_tree(&form);
        }
    }
    for form in forms(&facts.hearth_data_root, &mut probe) {
        policy.protect_at_or_above(&form);
    }
    for home in homes {
        for home in forms(&home, &mut probe) {
            for name in ACCOUNT_SECRET_FOLDERS {
                let secret = home.join(name);
                policy.protect_tree(&secret);
                // le dossier secret peut lui-même être un lien : sa cible est protégée aussi
                if let Ok(target) = resolve(secret.as_str(), &mut probe) {
                    policy.protect_tree(&target);
                }
            }
        }
    }
    Some(policy)
}

/// Un chemin sous sa forme écrite (nettoyée) et sous sa forme résolue ; un chemin relatif n'en donne
/// aucune (il n'y a rien à protéger à un endroit qu'on ne sait pas nommer).
fn forms(path: &str, probe: &mut dyn FnMut(&str) -> Probe) -> Vec<FolderPath> {
    let mut out = Vec::new();
    // forme écrite : aucun lien n'est suivi
    if let Ok(written) = resolve(path, &mut |_| Probe::Plain) {
        out.push(written);
    }
    if let Ok(resolved) = resolve(path, probe)
        && !out.contains(&resolved)
    {
        out.push(resolved);
    }
    out
}

/// Juge `raw` : rend le chemin canonique à donner au moteur, ou le refus. Le message du refus ne montre
/// jamais de chemin ([`FolderRefusal::message`]).
pub fn check_mount_folder(
    host: &dyn HostFolders,
    facts: &FolderFacts,
    raw: &str,
) -> Result<FolderPath, FolderRefusal> {
    let canonical = resolve(raw, &mut |path| host.probe(path))?;
    match build_policy(host, facts) {
        Some(policy) if !policy.is_protected(&canonical) => Ok(canonical),
        // protégé, ou comptes illisibles : on ne laisse rien passer sans savoir
        _ => Err(FolderRefusal::Protected),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    /// Un serveur de test : des liens, des comptes, modifiables entre deux vérifications.
    #[derive(Default)]
    struct FakeHost {
        links: HashMap<String, String>,
        homes: Mutex<Option<Vec<String>>>,
        unreadable: Vec<String>,
    }

    impl FakeHost {
        fn with_accounts(homes: &[&str]) -> Self {
            Self {
                homes: Mutex::new(Some(homes.iter().map(|home| (*home).to_owned()).collect())),
                ..Self::default()
            }
        }

        fn link(mut self, from: &str, to: &str) -> Self {
            self.links.insert(from.to_owned(), to.to_owned());
            self
        }

        fn add_account(&self, home: &str) {
            if let Some(homes) = self.homes.lock().expect("verrou").as_mut() {
                homes.push(home.to_owned());
            }
        }
    }

    impl HostFolders for FakeHost {
        fn probe(&self, path: &str) -> Probe {
            if self.unreadable.iter().any(|known| known == path) {
                return Probe::Unreadable;
            }
            match self.links.get(path) {
                Some(target) => Probe::Link(target.clone()),
                None => Probe::Plain,
            }
        }

        fn account_homes(&self) -> Option<Vec<String>> {
            self.homes.lock().expect("verrou").clone()
        }
    }

    fn facts() -> FolderFacts {
        FolderFacts {
            engine_socket: "/var/run/docker.sock".into(),
            engine_data_dir: "/var/lib/docker".into(),
            hearth_data_root: "/var/lib/hearth-data".into(),
            agent_paths: vec![
                "/var/lib/hearth".into(),
                "/usr/local/bin/hearth-agent".into(),
                "/etc/hearth".into(),
            ],
        }
    }

    fn verdict(host: &FakeHost, raw: &str) -> Result<String, FolderRefusal> {
        check_mount_folder(host, &facts(), raw).map(|path| path.as_str().to_owned())
    }

    #[test]
    fn an_unrelated_folder_is_accepted_and_given_back_in_its_canonical_form() {
        let host = FakeHost::with_accounts(&["/home/alice"]);
        assert_eq!(
            verdict(&host, "/srv/games//minecraft/./"),
            Ok("/srv/games/minecraft".into())
        );
        assert_eq!(
            verdict(&host, "/home/alice/games"),
            Ok("/home/alice/games".into())
        );
    }

    #[test]
    fn a_symbolic_link_to_usr_is_refused_even_when_the_link_is_not_in_a_protected_place() {
        let host = FakeHost::with_accounts(&[]).link("/srv/games/escape", "/usr");
        assert_eq!(
            verdict(&host, "/srv/games/escape"),
            Err(FolderRefusal::Protected)
        );
        assert_eq!(
            verdict(&host, "/srv/games/escape/lib"),
            Err(FolderRefusal::Protected)
        );
    }

    #[test]
    fn a_detour_with_dot_dot_after_a_link_is_judged_where_it_really_leads() {
        // /srv/games/up -> /usr/share ; « up/.. » mène à /usr, pas à /srv/games
        let host = FakeHost::with_accounts(&[]).link("/srv/games/up", "/usr/share");
        assert_eq!(
            verdict(&host, "/srv/games/up/.."),
            Err(FolderRefusal::Protected)
        );
    }

    #[test]
    fn the_ssh_folder_of_an_account_and_its_parents_are_refused() {
        let host = FakeHost::with_accounts(&["/home/alice", "/home/bob"]);
        for raw in [
            "/home/alice/.ssh",
            "/home/alice/.gnupg",
            "/home/bob/.ssh/authorized_keys",
            "/home/alice",
            "/home/bob",
            "/home",
            "/home/alice/games/../.ssh",
        ] {
            assert_eq!(verdict(&host, raw), Err(FolderRefusal::Protected), "{raw}");
        }
    }

    #[test]
    fn an_account_added_after_the_start_is_covered_at_the_next_check() {
        let host = FakeHost::with_accounts(&["/home/alice"]);
        assert_eq!(verdict(&host, "/srv/carol"), Ok("/srv/carol".into()));
        host.add_account("/srv/carol");
        assert_eq!(verdict(&host, "/srv/carol"), Err(FolderRefusal::Protected));
        assert_eq!(
            verdict(&host, "/srv/carol/.ssh"),
            Err(FolderRefusal::Protected)
        );
    }

    #[test]
    fn an_account_without_a_home_folder_does_not_make_the_check_fail() {
        // home_folders() n'en rend aucun pour lui ; le serveur peut aussi en rendre un vide ou relatif
        let host = FakeHost::with_accounts(&["", "relative/home", "/nonexistent"]);
        assert_eq!(verdict(&host, "/srv/games"), Ok("/srv/games".into()));
        let nobody = FakeHost::with_accounts(&[]);
        assert_eq!(verdict(&nobody, "/srv/games"), Ok("/srv/games".into()));
    }

    #[test]
    fn an_unreadable_account_list_refuses_instead_of_letting_everything_through() {
        let host = FakeHost::default(); // account_homes() = None
        assert_eq!(verdict(&host, "/srv/games"), Err(FolderRefusal::Protected));
    }

    #[test]
    fn a_home_folder_reached_through_a_link_is_protected_under_both_names() {
        // /home -> /data/homes : le .ssh réel est /data/homes/alice/.ssh
        let host = FakeHost::with_accounts(&["/home/alice"]).link("/home", "/data/homes");
        for raw in [
            "/data/homes/alice/.ssh",
            "/home/alice/.ssh",
            "/data/homes/alice",
        ] {
            assert_eq!(verdict(&host, raw), Err(FolderRefusal::Protected), "{raw}");
        }
        assert_eq!(verdict(&host, "/data/other"), Ok("/data/other".into()));
    }

    #[test]
    fn a_ssh_folder_that_is_itself_a_link_protects_its_target() {
        let host = FakeHost::with_accounts(&["/home/alice"]).link("/home/alice/.ssh", "/srv/keys");
        assert_eq!(verdict(&host, "/srv/keys"), Err(FolderRefusal::Protected));
        assert_eq!(
            verdict(&host, "/srv/keys/id_ed25519"),
            Err(FolderRefusal::Protected)
        );
    }

    #[test]
    fn the_agent_the_engine_and_the_hearth_data_root_are_refused_as_before() {
        let host = FakeHost::with_accounts(&[]);
        for raw in [
            "/var/run/docker.sock",
            "/var/lib/docker",
            "/var/lib/docker/volumes",
            "/var/lib/hearth",
            "/var/lib/hearth/db",
            "/usr/local/bin",
            "/etc/hearth",
            "/var/lib/hearth-data",
            "/var/lib",
            "/var",
            "/",
            "/etc",
            "/tmp",
            "/var/tmp",
            "/proc/1",
        ] {
            assert_eq!(verdict(&host, raw), Err(FolderRefusal::Protected), "{raw}");
        }
        // sous la racine de données : jugé par l'emplacement géré, pas par cette liste
        assert_eq!(
            verdict(&host, "/var/lib/hearth-data/my-service"),
            Ok("/var/lib/hearth-data/my-service".into())
        );
    }

    #[test]
    fn a_relative_or_empty_path_is_not_an_absolute_path() {
        let host = FakeHost::with_accounts(&[]);
        for raw in ["", "usr", "../usr", "./data"] {
            assert_eq!(
                verdict(&host, raw),
                Err(FolderRefusal::NotAnAbsolutePath),
                "{raw:?}"
            );
        }
    }

    #[test]
    fn a_component_the_system_cannot_read_is_refused_as_protected() {
        let host = FakeHost {
            unreadable: vec!["/srv/locked".into()],
            ..FakeHost::with_accounts(&[])
        };
        assert_eq!(
            verdict(&host, "/srv/locked/data"),
            Err(FolderRefusal::Protected)
        );
    }

    #[test]
    fn no_refusal_message_shows_a_path() {
        let host = FakeHost::with_accounts(&["/home/secretaccount"]);
        for raw in ["/home/secretaccount/.ssh", "/usr", "/home/secretaccount"] {
            let refusal = verdict(&host, raw).expect_err("refusé");
            assert_eq!(
                refusal.to_string(),
                "Ce dossier est protégé : il ne peut pas être monté dans un service."
            );
            assert!(!refusal.message().contains("secretaccount"));
        }
    }
}
