//! Ce que l'agent sait du serveur pour adapter l'aide sur le moteur de conteneurs : la distribution
//! (`/etc/os-release`), le système d'init, le compte de l'agent. Lecture seule, sans commande lancée.

use std::path::Path;

use crate::application::ports::{HostDescription, HostEnvironment};

pub struct SystemHostEnvironment;

impl HostEnvironment for SystemHostEnvironment {
    fn describe(&self) -> HostDescription {
        let os_release = std::fs::read_to_string("/etc/os-release")
            .or_else(|_| std::fs::read_to_string("/usr/lib/os-release"))
            .unwrap_or_default();
        let (os_id, os_like) = parse_os_release(&os_release);
        let (agent_account, agent_is_admin) = agent_account();
        HostDescription {
            declarative: os_id == "nixos",
            os_id,
            os_like,
            systemd: Path::new("/run/systemd/system").is_dir(),
            agent_account,
            agent_is_admin,
        }
    }
}

/// `ID` et `ID_LIKE` d'un fichier `os-release` (valeurs éventuellement entre guillemets).
pub fn parse_os_release(text: &str) -> (String, Vec<String>) {
    let mut id = String::new();
    let mut like = Vec::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches(['"', '\'']);
        match key.trim() {
            "ID" => id = value.to_lowercase(),
            "ID_LIKE" => {
                like = value
                    .split_whitespace()
                    .map(|word| word.to_lowercase())
                    .collect();
            }
            _ => {}
        }
    }
    (id, like)
}

/// L'identifiant réel de l'utilisateur d'un `/proc/self/status`.
pub fn parse_uid(status: &str) -> Option<u32> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// Le nom du compte d'identifiant `uid` dans un `/etc/passwd`.
pub fn account_name(passwd: &str, uid: u32) -> Option<String> {
    passwd.lines().find_map(|line| {
        let mut fields = line.split(':');
        let name = fields.next()?;
        let _password = fields.next()?;
        let id: u32 = fields.next()?.parse().ok()?;
        (id == uid).then(|| name.to_owned())
    })
}

/// Le compte sous lequel tourne l'agent, et s'il est l'administrateur du serveur (identifiant 0).
fn agent_account() -> (String, bool) {
    let Some(uid) = std::fs::read_to_string("/proc/self/status")
        .ok()
        .as_deref()
        .and_then(parse_uid)
    else {
        return (String::new(), false);
    };
    let passwd = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
    let name = account_name(&passwd, uid).unwrap_or_else(|| uid.to_string());
    (name, uid == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debian_and_nixos_are_recognised() {
        let debian =
            "PRETTY_NAME=\"Debian GNU/Linux 12 (bookworm)\"\nID=debian\nVERSION_ID=\"12\"\n";
        assert_eq!(parse_os_release(debian), ("debian".to_owned(), vec![]));
        let ubuntu = "ID=ubuntu\nID_LIKE=debian\n";
        assert_eq!(
            parse_os_release(ubuntu),
            ("ubuntu".to_owned(), vec!["debian".to_owned()])
        );
        let nixos = "ID=nixos\nVARIANT_ID=\"\"\n";
        assert_eq!(parse_os_release(nixos).0, "nixos");
        let rocky = "ID=\"rocky\"\nID_LIKE=\"rhel centos fedora\"\n";
        assert_eq!(
            parse_os_release(rocky),
            (
                "rocky".to_owned(),
                vec!["rhel".into(), "centos".into(), "fedora".into()]
            )
        );
        assert_eq!(parse_os_release(""), (String::new(), vec![]));
    }

    #[test]
    fn the_account_comes_from_the_real_uid() {
        let status = "Name:\thearth-agent\nUid:\t0\t0\t0\t0\nGid:\t0\t0\t0\t0\n";
        assert_eq!(parse_uid(status), Some(0));
        assert_eq!(parse_uid("Name: x\n"), None);
        let passwd = "root:x:0:0:root:/root:/bin/bash\nhearth:x:1001:1001::/home/hearth:/bin/sh\n";
        assert_eq!(account_name(passwd, 0).as_deref(), Some("root"));
        assert_eq!(account_name(passwd, 1001).as_deref(), Some("hearth"));
        assert_eq!(account_name(passwd, 42), None);
    }
}
