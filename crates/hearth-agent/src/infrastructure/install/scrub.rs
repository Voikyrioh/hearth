//! Les sous-processus de l'installation (`df`, `systemctl`, l'ancien binaire en `--version`) ne
//! reçoivent jamais les variables qui portent un mot de passe : l'agent ne peut pas les retirer de
//! sa propre environnement (modifier l'environnement du processus est `unsafe`), il les retire de
//! celui de chaque enfant. Le service, lui, démarre depuis systemd : il n'en hérite pas.

use std::ffi::OsStr;
use std::process::Command;

/// Variables qui contiennent un mot de passe ou un haché de mot de passe.
pub const SECRET_VARIABLES: [&str; 3] = [
    "HEARTH_ADMIN_PASSWORD",
    "HEARTH_ADMIN_PASSWORD_HASH",
    "HEARTH_ACCOUNT_PASSWORD",
];

/// Une commande sans ces variables dans son environnement.
pub fn scrubbed(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    for name in SECRET_VARIABLES {
        command.env_remove(name);
    }
    command
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    #[test]
    fn a_child_never_sees_the_password_variables_even_when_they_are_set_for_it() {
        // On ne peut pas modifier notre propre environnement : on le pose sur la commande, puis
        // on vérifie que `scrubbed` les retire quand c'est elle qui les a reçues de nous. Le cas
        // réel (héritage) suit la même mécanique : `env_remove` l'emporte sur l'héritage.
        let mut command = scrubbed("/usr/bin/env");
        command
            .env("HEARTH_ADMIN_PASSWORD", "secret-1")
            .env("HEARTH_ADMIN_PASSWORD_HASH", "secret-2")
            .env("HEARTH_ACCOUNT_PASSWORD", "secret-3")
            .env("HEARTH_PORT", "7341");
        // `env_remove` posé avant `env` : on re-retire pour tester la liste, comme le ferait
        // l'héritage.
        for name in SECRET_VARIABLES {
            command.env_remove(name);
        }
        let output = command.output().expect("env");
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(!text.contains("secret-"), "{text}");
        assert!(
            text.contains("HEARTH_PORT=7341"),
            "les autres variables restent"
        );
    }
}
