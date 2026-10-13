//! Dossiers du serveur qu'un service ne peut pas monter (HRT-75, BR-DEPLOY-012).
//!
//! Règle pure : des chemins Unix en texte, aucune E/S. Ce que le système de fichiers sait (qu'un nom est
//! un lien symbolique, quels comptes ont quel dossier personnel) arrive par des paramètres : une
//! fonction de sonde pour [`resolve`], des textes pour [`FolderPolicy`].
//!
//! FIX:01M4V4G0ZPWMKE6WCYKPF2Q3SM (docs/bugs/FIX-01M4V4G0ZPWMKE6WCYKPF2Q3SM.md) : la liste laissait passer
//! `/usr`, `/nix`, `/run`, `/var/spool` et le `.ssh` d'un compte. Les dossiers du système ci-dessous, les
//! dossiers secrets de chaque compte et le « tout dossier qui contient l'un d'eux » en sont la correction.
//!
//! Un conteneur tourne en root par défaut : monter `/usr`, `/nix`, `/run`, le `.ssh` d'un compte ou un
//! dossier qui le contient, en écriture, suffit pour obtenir les pouvoirs de l'administrateur du
//! serveur. Cette liste est une CEINTURE : la barrière est la liste d'autorisation des dossiers
//! ouverts aux services (HRT-70). Elle ne protège pas d'une image choisie par un administrateur qui fait ce qu'elle veut des
//! dossiers qu'on lui donne.
//!
//! Comment un chemin est jugé :
//! 1. [`resolve`] le rend absolu et canonique : `.`, `..` et barres en double sont résolus composant par
//!    composant AVEC les liens symboliques (un `..` après un lien remonte depuis la cible du lien, pas
//!    depuis le texte), au plus [`MAX_LINK_HOPS`] liens suivis ;
//! 2. [`FolderPolicy::is_protected`] compare ce chemin canonique composant par composant (jamais par
//!    préfixe de texte : `/etcetera` n'est pas sous `/etc`), sans tenir compte de la casse (refuser
//!    trop vaut mieux que de se laisser tromper par un système de fichiers qui ignore la casse).
//!
//! Un chemin est refusé quand il est égal à un dossier protégé, sous lui, OU le contient (monter `/home`
//! donnerait accès au `.ssh` de chaque compte). Seule exception : la racine de données Hearth, dont les
//! sous-dossiers ne se montent que par l'emplacement « géré par Hearth » : égal ou
//! contenant seulement.

use super::limits::MAX_PATH_CHARS;

/// Le message, mot pour mot, de tout refus : il ne montre aucun chemin (BR-DEPLOY-012).
pub const PROTECTED_MESSAGE: &str =
    "Ce dossier est protégé : il ne peut pas être monté dans un service.";

/// Message d'un chemin qui n'est pas absolu ou pas lisible.
pub const NOT_A_PATH_MESSAGE: &str = "Le chemin du dossier doit être un chemin absolu.";

/// Liens symboliques suivis au plus pour un chemin (comme le noyau : 40).
pub const MAX_LINK_HOPS: usize = 40;

// FIX:01M4V4G0ZPWMKE6WCYKPF2Q3SM
/// Dossiers du système refusés, avec tout ce qu'ils contiennent et tout dossier qui les contient
/// (BR-DEPLOY-012 et son amendement du 2026-10-13). `/tmp` et `/var/tmp` y sont parce que l'unité de
/// l'agent a `PrivateTmp=yes` : l'agent ne voit pas le vrai `/tmp` et ne peut rien y vérifier.
pub const SYSTEM_FOLDERS: [&str; 17] = [
    "/etc",
    "/boot",
    "/proc",
    "/sys",
    "/dev",
    "/root",
    "/tmp",
    "/var/tmp",
    "/usr",
    "/bin",
    "/sbin",
    "/lib",
    "/lib64",
    "/nix",
    "/run",
    "/var/run",
    "/var/spool",
];

// FIX:01M4V4G0ZPWMKE6WCYKPF2Q3SM
/// Dossiers que chaque compte du serveur garde dans son dossier personnel : une clé d'accès ou une
/// clé de signature ajoutée là donne le compte (BR-DEPLOY-012).
pub const ACCOUNT_SECRET_FOLDERS: [&str; 2] = [".ssh", ".gnupg"];

/// Pourquoi un chemin n'est pas jugé : jamais le chemin lui-même.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FolderRefusal {
    /// Dossier protégé, ou chemin qu'on ne peut pas lire sans se laisser tromper (trop de liens, lien
    /// illisible) : refusé comme protégé.
    #[error("{}", PROTECTED_MESSAGE)]
    Protected,
    /// Chemin vide, relatif, avec un octet nul ou trop long.
    #[error("{}", NOT_A_PATH_MESSAGE)]
    NotAnAbsolutePath,
}

impl FolderRefusal {
    /// Le texte montré à l'utilisateur.
    pub fn message(self) -> &'static str {
        match self {
            Self::Protected => PROTECTED_MESSAGE,
            Self::NotAnAbsolutePath => NOT_A_PATH_MESSAGE,
        }
    }
}

/// Ce que le système de fichiers dit d'un chemin absolu déjà résolu jusque-là.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    /// Pas un lien symbolique (dossier, fichier, ou n'existe pas).
    Plain,
    /// Un lien symbolique vers cette cible (absolue ou relative au dossier du lien).
    Link(String),
    /// Impossible de savoir (droit refusé, erreur) : le chemin est refusé, jamais cru.
    Unreadable,
}

/// Un chemin absolu canonique : sans `.`, `..`, barre en double ni barre finale (hors racine).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderPath(String);

impl FolderPath {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn from_parts(parts: &[String]) -> Self {
        if parts.is_empty() {
            Self("/".to_owned())
        } else {
            Self(format!("/{}", parts.join("/")))
        }
    }

    /// Les composants, repliés pour la comparaison.
    fn folded(&self) -> Vec<String> {
        self.0
            .split('/')
            .filter(|part| !part.is_empty())
            .map(str::to_lowercase)
            .collect()
    }

    /// Ce chemin suivi d'un composant (`<home>` + `.ssh`).
    pub fn join(&self, name: &str) -> Self {
        let mut parts: Vec<String> = self
            .0
            .split('/')
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect();
        parts.push(name.to_owned());
        Self::from_parts(&parts)
    }
}

fn components(path: &str) -> impl Iterator<Item = String> + '_ {
    path.split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
}

/// Rend `raw` absolu et canonique, liens symboliques résolus par `probe` (appelée avec chaque chemin
/// absolu déjà résolu, du plus court au plus long). Un composant qui n'existe pas est gardé tel quel : le
/// détour par un chemin inexistant ne change rien (le moteur le créera là où le texte le dit, ou le
/// refusera).
pub fn resolve(
    raw: &str,
    probe: &mut dyn FnMut(&str) -> Probe,
) -> Result<FolderPath, FolderRefusal> {
    if !raw.starts_with('/') || raw.contains('\0') || raw.chars().count() > MAX_PATH_CHARS {
        return Err(FolderRefusal::NotAnAbsolutePath);
    }
    let mut pending: Vec<String> = components(raw).collect();
    pending.reverse(); // pop() rend le prochain composant
    let mut resolved: Vec<String> = Vec::new();
    let mut hops = 0usize;
    while let Some(part) = pending.pop() {
        match part.as_str() {
            "." => continue,
            ".." => {
                resolved.pop();
                continue;
            }
            _ => {}
        }
        resolved.push(part);
        let here = FolderPath::from_parts(&resolved);
        match probe(here.as_str()) {
            Probe::Plain => {}
            Probe::Unreadable => return Err(FolderRefusal::Protected),
            Probe::Link(target) => {
                hops += 1;
                if hops > MAX_LINK_HOPS || target.contains('\0') || target.is_empty() {
                    return Err(FolderRefusal::Protected);
                }
                resolved.pop();
                if target.starts_with('/') {
                    resolved.clear();
                }
                let mut rest: Vec<String> = components(&target).collect();
                rest.reverse();
                pending.extend(rest);
            }
        }
    }
    Ok(FolderPath::from_parts(&resolved))
}

/// Les dossiers refusés, tels que l'agent les connaît à ce moment.
#[derive(Debug, Clone, Default)]
pub struct FolderPolicy {
    /// Refusés avec ce qu'ils contiennent et ce qui les contient.
    trees: Vec<Vec<String>>,
    /// Refusés seulement égaux ou contenant (la racine de données Hearth).
    at_or_above: Vec<Vec<String>>,
}

impl FolderPolicy {
    /// La liste fermée du système (`SYSTEM_FOLDERS`) et la racine `/` (via le « contient »).
    pub fn system() -> Self {
        let mut policy = Self::default();
        for folder in SYSTEM_FOLDERS {
            policy.protect_tree(&FolderPath(folder.to_owned()));
        }
        policy
    }

    /// Protège ce dossier, ce qu'il contient et ce qui le contient.
    pub fn protect_tree(&mut self, path: &FolderPath) {
        self.trees.push(path.folded());
    }

    /// Protège ce dossier et ce qui le contient, pas ses sous-dossiers.
    pub fn protect_at_or_above(&mut self, path: &FolderPath) {
        self.at_or_above.push(path.folded());
    }

    /// Ce dossier canonique est-il refusé ? Une liste vide n'existe pas : `system()` couvre toujours la
    /// racine.
    pub fn is_protected(&self, path: &FolderPath) -> bool {
        let wanted = path.folded();
        self.trees
            .iter()
            .any(|entry| starts_with(&wanted, entry) || starts_with(entry, &wanted))
            || self
                .at_or_above
                .iter()
                .any(|entry| starts_with(entry, &wanted))
    }
}

/// `whole` commence par `prefix`, composant par composant.
fn starts_with(whole: &[String], prefix: &[String]) -> bool {
    whole.len() >= prefix.len() && whole.iter().zip(prefix).all(|(a, b)| a == b)
}

/// Les dossiers personnels lus dans le texte d'un fichier de comptes (`/etc/passwd`, sept champs
/// séparés par `:`, le sixième est le dossier). Une ligne vide, un commentaire, une ligne mal formée,
/// un dossier vide ou relatif sont ignorés : un compte sans dossier personnel ne fait rien échouer.
pub fn home_folders(passwd: &str) -> Vec<String> {
    let mut homes: Vec<String> = Vec::new();
    for line in passwd.lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(home) = line.split(':').nth(5) else {
            continue;
        };
        if home.starts_with('/') && !home.contains('\0') && !homes.iter().any(|known| known == home)
        {
            homes.push(home.to_owned());
        }
    }
    homes
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    /// Un système de fichiers de test : seulement des liens symboliques.
    fn links(table: &[(&str, &str)]) -> HashMap<String, String> {
        table
            .iter()
            .map(|(from, to)| ((*from).to_owned(), (*to).to_owned()))
            .collect()
    }

    fn resolve_in(raw: &str, table: &HashMap<String, String>) -> Result<FolderPath, FolderRefusal> {
        resolve(raw, &mut |path| match table.get(path) {
            Some(target) => Probe::Link(target.clone()),
            None => Probe::Plain,
        })
    }

    fn plain(raw: &str) -> String {
        resolve_in(raw, &HashMap::new())
            .map(|path| path.as_str().to_owned())
            .unwrap_or_else(|e| format!("{e:?}"))
    }

    /// Une politique à la manière de l'agent d'un serveur : deux comptes, un sans `.ssh` réel.
    fn policy() -> FolderPolicy {
        let mut policy = FolderPolicy::system();
        for path in [
            "/var/run/docker.sock",
            "/var/lib/docker",
            "/var/lib/hearth",
            "/usr/local/bin/hearth-agent",
            "/etc/hearth",
        ] {
            policy.protect_tree(&FolderPath(path.to_owned()));
        }
        policy.protect_at_or_above(&FolderPath("/var/lib/hearth-data".to_owned()));
        for home in ["/home/alice", "/home/bob", "/srv/games/runner"] {
            let home = FolderPath(home.to_owned());
            for name in ACCOUNT_SECRET_FOLDERS {
                policy.protect_tree(&home.join(name));
            }
        }
        policy
    }

    fn refused(raw: &str) -> bool {
        let path = resolve_in(raw, &HashMap::new()).expect("chemin valable");
        policy().is_protected(&path)
    }

    #[test]
    fn resolve_cleans_dots_double_slashes_and_trailing_slashes() {
        for (raw, expected) in [
            ("/", "/"),
            ("//", "/"),
            ("/a/b", "/a/b"),
            ("/a//b///c/", "/a/b/c"),
            ("/a/./b/.", "/a/b"),
            ("/a/b/../c", "/a/c"),
            ("/a/b/../../..", "/"),
            ("/..", "/"),
            ("/../../etc", "/etc"),
            ("/home/alice/../bob", "/home/bob"),
            ("/a/../", "/"),
        ] {
            assert_eq!(plain(raw), expected, "pour {raw:?}");
        }
    }

    #[test]
    fn resolve_refuses_what_is_not_an_absolute_path() {
        let long = format!("/{}", "a".repeat(MAX_PATH_CHARS));
        for raw in [
            "",
            "etc",
            "./etc",
            "../etc",
            "~/x",
            "a/b",
            "/a\0b",
            long.as_str(),
        ] {
            assert_eq!(
                resolve_in(raw, &HashMap::new()),
                Err(FolderRefusal::NotAnAbsolutePath),
                "pour {raw:?}"
            );
        }
    }

    #[test]
    fn resolve_follows_symbolic_links_component_by_component() {
        let table = links(&[
            ("/data/shortcut", "/usr"),
            ("/data/relative", "../usr/share"),
            ("/data/chain", "/data/shortcut"),
            ("/bin", "usr/bin"),
            ("/var/run", "/run"),
        ]);
        for (raw, expected) in [
            ("/data/shortcut", "/usr"),
            ("/data/shortcut/lib", "/usr/lib"),
            ("/data/relative", "/usr/share"),
            ("/data/chain/local", "/usr/local"),
            ("/bin", "/usr/bin"),
            ("/var/run/docker.sock", "/run/docker.sock"),
            // un `..` après un lien remonte depuis la CIBLE du lien, pas depuis le texte
            ("/data/shortcut/..", "/"),
            ("/data/relative/../..", "/"),
            // le lien est suivi même sous un détour
            ("/data/x/../shortcut", "/usr"),
        ] {
            let path = resolve_in(raw, &table).expect("résolu");
            assert_eq!(path.as_str(), expected, "pour {raw:?}");
        }
    }

    #[test]
    fn a_loop_of_links_is_refused_as_protected_and_a_long_honest_chain_is_not() {
        let table = links(&[("/a", "/b"), ("/b", "/a")]);
        assert_eq!(resolve_in("/a", &table), Err(FolderRefusal::Protected));
        assert_eq!(
            resolve_in("/self", &links(&[("/self", "/self")])),
            Err(FolderRefusal::Protected)
        );
        // 5 liens à la suite : sans problème.
        let chain = links(&[
            ("/l1", "/l2"),
            ("/l2", "/l3"),
            ("/l3", "/l4"),
            ("/l4", "/l5"),
            ("/l5", "/ok"),
        ]);
        assert_eq!(
            resolve_in("/l1", &chain).map(|p| p.as_str().to_owned()),
            Ok("/ok".to_owned())
        );
    }

    #[test]
    fn an_unreadable_component_refuses_the_path() {
        let result = resolve("/data/locked/x", &mut |path| {
            if path == "/data/locked" {
                Probe::Unreadable
            } else {
                Probe::Plain
            }
        });
        assert_eq!(result, Err(FolderRefusal::Protected));
    }

    #[test]
    fn every_system_folder_is_refused_one_by_one() {
        for folder in SYSTEM_FOLDERS {
            assert!(refused(folder), "{folder} devrait être refusé");
            assert!(
                refused(&format!("{folder}/")),
                "{folder}/ devrait être refusé"
            );
            assert!(
                refused(&format!("{folder}/sub/dir")),
                "{folder}/sub/dir devrait être refusé"
            );
        }
        assert!(refused("/"));
    }

    #[test]
    fn the_folders_added_by_the_amendment_are_each_refused() {
        for folder in [
            "/usr",
            "/bin",
            "/sbin",
            "/lib",
            "/lib64",
            "/nix",
            "/run",
            "/var/run",
            "/var/spool",
        ] {
            assert!(refused(folder), "{folder}");
        }
        assert!(refused("/usr/share/doc"));
        assert!(refused("/nix/store/abc-hello"));
        assert!(refused("/run/user/1000"));
        assert!(refused("/var/spool/cron"));
    }

    #[test]
    fn the_ssh_and_gnupg_folders_of_every_account_are_refused() {
        for path in [
            "/home/alice/.ssh",
            "/home/alice/.gnupg",
            "/home/bob/.ssh",
            "/home/bob/.gnupg",
            "/home/alice/.ssh/authorized_keys",
            "/home/bob/.gnupg/private-keys-v1.d",
            "/srv/games/runner/.ssh",
        ] {
            assert!(refused(path), "{path}");
        }
        // un autre dossier du même compte reste permis, tant qu'il ne contient pas un dossier secret
        assert!(!refused("/home/alice/games"));
        assert!(!refused("/home/alice/.ssh2"));
        assert!(!refused("/home/alice/ssh"));
    }

    #[test]
    fn a_folder_that_contains_a_protected_one_is_refused() {
        for (parent, why) in [
            ("/home/alice", "contient .ssh"),
            ("/home/bob", "contient .gnupg"),
            ("/home", "contient le dossier personnel d'un compte"),
            ("/var", "contient /var/lib/hearth"),
            ("/var/lib", "contient /var/lib/hearth"),
            ("/srv/games/runner", "contient .ssh"),
            (
                "/var/lib/hearth-data",
                "la racine de données Hearth elle-même",
            ),
        ] {
            assert!(refused(parent), "{parent} : {why}");
        }
        // le parent d'un dossier protégé de l'agent
        assert!(refused("/usr/local/bin"));
        assert!(refused("/etc"));
    }

    #[test]
    fn the_subfolders_of_the_hearth_data_root_are_not_refused_by_the_list() {
        // elles ne se montent que par l'emplacement géré : ce n'est pas le rôle de la liste
        assert!(!refused("/var/lib/hearth-data/mon-service/world"));
    }

    #[test]
    fn an_unrelated_folder_is_accepted_so_the_refusal_does_not_become_total() {
        for path in [
            "/mnt/data",
            "/opt/games",
            "/srv/minecraft",
            "/data",
            "/home/alice/games",
            "/var/lib/other",
            "/var/lib/hearthx",
            "/var/lib/hearth-dataz",
            "/var/log/games",
            "/etcetera",
            "/usr2",
            "/rootfs",
            "/var/spoolx",
            "/runner",
            "/libs",
        ] {
            assert!(!refused(path), "{path} devrait être accepté");
        }
    }

    #[test]
    fn a_detour_through_dot_dot_or_double_slashes_does_not_hide_a_protected_folder() {
        for raw in [
            "/mnt/../usr",
            "/mnt/data/../../etc/passwd",
            "//usr//lib",
            "/usr/./lib/../../etc",
            "/../usr",
            "/home/alice/games/../.ssh",
            "/home/alice/./.ssh/",
            "/var/lib/../spool",
            "/mnt/..",
        ] {
            assert!(refused(raw), "{raw}");
        }
    }

    #[test]
    fn a_symbolic_link_to_a_protected_folder_is_refused() {
        let table = links(&[
            ("/mnt/data/escape", "/usr"),
            ("/mnt/data/rel", "../../etc"),
            ("/mnt/data/key", "/home/alice/.ssh"),
            ("/mnt/data/home", "/home"),
            ("/mnt/data/hop", "/mnt/data/escape"),
        ]);
        for raw in [
            "/mnt/data/escape",
            "/mnt/data/escape/lib",
            "/mnt/data/rel",
            "/mnt/data/key",
            "/mnt/data/home",
            "/mnt/data/hop",
            "/mnt/data/escape/..",
        ] {
            let path = resolve_in(raw, &table).expect("résolu");
            assert!(policy().is_protected(&path), "{raw} -> {}", path.as_str());
        }
        // un lien vers un dossier sans rapport reste accepté
        let ok = links(&[("/mnt/data/fine", "/opt/games")]);
        let path = resolve_in("/mnt/data/fine", &ok).expect("résolu");
        assert!(!policy().is_protected(&path));
    }

    #[test]
    fn the_case_does_not_hide_a_protected_folder() {
        for path in [
            "/ETC",
            "/Usr/Lib",
            "/NIX",
            "/Var/Spool",
            "/home/Alice/.SSH",
            "/HOME",
            "/var/lib/Hearth",
        ] {
            assert!(refused(path), "{path}");
        }
        assert!(!refused("/Mnt/Data"));
    }

    #[test]
    fn the_refusal_messages_are_fixed_and_show_no_path() {
        assert_eq!(
            FolderRefusal::Protected.to_string(),
            "Ce dossier est protégé : il ne peut pas être monté dans un service."
        );
        assert_eq!(FolderRefusal::Protected.message(), PROTECTED_MESSAGE);
        assert_eq!(
            FolderRefusal::NotAnAbsolutePath.to_string(),
            NOT_A_PATH_MESSAGE
        );
    }

    #[test]
    fn home_folders_reads_every_account_and_skips_what_has_no_home() {
        let passwd = "\
root:x:0:0:root:/root:/bin/bash
# un commentaire
daemon:x:1:1:daemon:/usr/sbin:/usr/sbin/nologin
nobody:x:65534:65534:nobody:/nonexistent:/usr/sbin/nologin
alice:x:1000:1000:Alice,,,:/home/alice:/bin/bash
nohome:x:1001:1001::/:/bin/false
empty:x:1002:1002:::/bin/false
relative:x:1003:1003::home/relative:/bin/false
short:x:1004

twin:x:1005:1005::/home/alice:/bin/bash
";
        assert_eq!(
            home_folders(passwd),
            vec!["/root", "/usr/sbin", "/nonexistent", "/home/alice", "/"]
        );
        assert!(home_folders("").is_empty());
    }

    #[test]
    fn strange_paths_never_panic() {
        // robustesse : des chemins étranges ne font que se faire refuser ou résoudre
        for raw in [
            "/\u{202e}x",
            "/a b/c d",
            "/é/ü",
            "/a/\u{0}",
            "/..../x",
            "/.../..",
            "//////",
        ] {
            let _ = resolve_in(raw, &HashMap::new());
        }
    }
}
