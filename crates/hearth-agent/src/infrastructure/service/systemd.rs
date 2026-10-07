//! Le service par une unité systemd (`/etc/systemd/system/hearth-agent.service`).
//!
//! L'unité ne contient aucun secret (ni mot de passe ni haché : l'agent les lit dans sa base).
//! `systemctl` est lancé avec une liste d'arguments, jamais par un interpréteur de commandes ;
//! chaque chemin écrit dans l'unité est validé (absolu, caractères sûrs) : un chemin ne peut pas
//! y injecter une directive. Voir ADR-0012 pour le choix de root et du durcissement.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::domain::install::UNIT_TEMP_EXTENSION;
use crate::infrastructure::install::scrub::scrubbed;

use crate::application::ports::{
    ServiceError, ServiceKind, ServiceManager, ServiceSpec, ServiceState,
};

pub const UNIT_NAME: &str = "hearth-agent.service";
pub const UNIT_DIR: &str = "/etc/systemd/system";

pub struct Systemd {
    unit_path: PathBuf,
    systemctl: OsString,
}

impl Systemd {
    /// Le systemd de la machine : unité dans `/etc/systemd/system`, `systemctl` du `PATH`.
    pub fn system() -> Self {
        Self::new(
            Path::new(UNIT_DIR).join(UNIT_NAME),
            OsString::from("systemctl"),
        )
    }

    /// Chemin de l'unité et programme `systemctl` explicites (tests : faux `systemctl`).
    pub fn new(unit_path: PathBuf, systemctl: OsString) -> Self {
        Self {
            unit_path,
            systemctl,
        }
    }

    /// systemd tourne comme système d'initialisation de cette machine.
    pub fn is_running_here() -> bool {
        Path::new("/run/systemd/system").is_dir()
    }

    fn run(&self, args: &[&str]) -> Result<(), ServiceError> {
        let output = scrubbed(&self.systemctl)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(|source| ServiceError::Command {
                command: args.join(" "),
                detail: format!("lancement impossible : {source}"),
            })?;
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(ServiceError::Command {
                command: args.join(" "),
                detail: format!("{} : {}", output.status, stderr.trim()),
            })
        }
    }

    fn unit_error(&self, source: std::io::Error) -> ServiceError {
        ServiceError::Unit {
            path: self.unit_path.display().to_string(),
            source,
        }
    }

    /// Écrit l'unité par un fichier voisin puis un renommage : jamais d'unité à moitié écrite.
    fn write_unit(&self, text: &str) -> Result<(), ServiceError> {
        let temporary = self.unit_path.with_extension(UNIT_TEMP_EXTENSION);
        let write = || -> std::io::Result<()> {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o644);
            }
            let mut file = options.open(&temporary)?;
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            std::fs::rename(&temporary, &self.unit_path)
        };
        write().map_err(|source| {
            let _ = std::fs::remove_file(&temporary);
            self.unit_error(source)
        })
    }
}

/// La ligne `ActiveState=...` de `systemctl show`, rapportée à l'état du service.
fn parse_active_state(output: &str) -> ServiceState {
    let value = output
        .lines()
        .find_map(|line| line.strip_prefix("ActiveState="))
        .map(str::trim);
    match value {
        Some("active" | "reloading") => ServiceState::Active,
        Some("activating" | "deactivating") => ServiceState::Activating,
        Some("inactive") => ServiceState::Inactive,
        Some("failed") => ServiceState::Failed,
        _ => ServiceState::Unknown,
    }
}

/// Un chemin dans une unité : absolu, sans espace ni caractère qui compte pour systemd.
fn safe_path(path: &Path) -> Result<&str, ServiceError> {
    let text = path.to_str().unwrap_or("");
    let safe = path.is_absolute()
        && !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/_.-+@:".contains(c));
    if safe {
        Ok(text)
    } else {
        Err(ServiceError::Command {
            command: "unité".to_owned(),
            detail: format!("chemin refusé dans l'unité : {}", path.display()),
        })
    }
}

/// Le texte de l'unité. Pas de secret : seulement des chemins validés.
pub fn render_unit(spec: &ServiceSpec) -> Result<String, ServiceError> {
    let binary = safe_path(&spec.binary)?;
    let config = safe_path(&spec.config)?;
    let data_dir = safe_path(&spec.data_dir)?;
    let binary_dir = spec
        .binary
        .parent()
        .and_then(|dir| safe_path(dir).ok())
        .unwrap_or("/usr/local/bin");
    // FIX:01M460G9KDDNZAJE3NTJSP49T4 : `CAP_MKNOD` est gardée dans `CapabilityBoundingSet` (voir plus bas).
    Ok(format!(
        "\
[Unit]
Description=Agent Hearth
Documentation=https://github.com/Voikyrioh/hearth
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart={binary} serve --config {config}
Restart=always
RestartSec=5
# Les fichiers créés par l'agent ne sont lisibles que par root.
UMask=0077
# Garde-fous, pas une barrière (ADR-0012) : l'agent tourne en root parce qu'il pilotera Docker
# (son socket) et l'alimentation. Ce que l'unité retire : les capacités sans rapport avec ces
# besoins (SYS_ADMIN, SYS_PTRACE, NET_ADMIN, SYS_MODULE...), les appels système hors d'un service
# ordinaire, les espaces de noms, les familles d'adresses inutiles, l'écriture dans /usr, /etc et
# /boot, et le gain de privilèges. Ce qu'elle ne retire pas : root garde l'accès à tout fichier
# (DAC_OVERRIDE) et au socket de Docker, c'est-à-dire, en pratique, à la machine. CAP_MKNOD est
# gardée : sans écran, le premier `nvidia-smi` crée /dev/nvidia* lui-même (mknod), sinon la
# carte graphique n'est jamais vue.
NoNewPrivileges=yes
CapabilityBoundingSet=CAP_CHOWN CAP_DAC_OVERRIDE CAP_DAC_READ_SEARCH CAP_FOWNER CAP_KILL CAP_SETUID CAP_SETGID CAP_NET_BIND_SERVICE CAP_NET_RAW CAP_SYS_BOOT CAP_MKNOD
RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6 AF_NETLINK
RestrictNamespaces=yes
SystemCallFilter=@system-service
SystemCallErrorNumber=EPERM
ProtectSystem=full
ProtectHome=read-only
ReadWritePaths={data_dir} {binary_dir}
PrivateTmp=yes
ProtectKernelModules=yes
ProtectKernelLogs=yes
ProtectControlGroups=yes
ProtectClock=yes
RestrictSUIDSGID=yes
RestrictRealtime=yes
LockPersonality=yes
SystemCallArchitectures=native

[Install]
WantedBy=multi-user.target
"
    ))
}

impl ServiceManager for Systemd {
    fn kind(&self) -> ServiceKind {
        ServiceKind::Systemd
    }

    fn is_installed(&self) -> Result<bool, ServiceError> {
        Ok(self.unit_path.exists())
    }

    fn is_active(&self) -> Result<bool, ServiceError> {
        if !self.unit_path.exists() {
            return Ok(false);
        }
        let status = scrubbed(&self.systemctl)
            .args(["is-active", "--quiet", UNIT_NAME])
            .stdin(Stdio::null())
            .status()
            .map_err(|source| ServiceError::Command {
                command: format!("is-active {UNIT_NAME}"),
                detail: format!("lancement impossible : {source}"),
            })?;
        Ok(status.success())
    }

    /// `systemctl show` : `ActiveState` dit si le service tourne, s'il se relance après un échec
    /// (`activating`) ou s'il a été arrêté (`inactive`). Jamais d'erreur : un système qui ne répond
    /// pas donne `Unknown`, que personne ne prend pour un arrêt voulu.
    fn state(&self) -> ServiceState {
        if !self.unit_path.exists() {
            return ServiceState::Inactive;
        }
        let Ok(output) = scrubbed(&self.systemctl)
            .args(["show", "--property=ActiveState", UNIT_NAME])
            .stdin(Stdio::null())
            .output()
        else {
            return ServiceState::Unknown;
        };
        if !output.status.success() {
            return ServiceState::Unknown;
        }
        parse_active_state(&String::from_utf8_lossy(&output.stdout))
    }

    fn is_enabled(&self) -> Result<bool, ServiceError> {
        if !self.unit_path.exists() {
            return Ok(false);
        }
        let status = scrubbed(&self.systemctl)
            .args(["is-enabled", "--quiet", UNIT_NAME])
            .stdin(Stdio::null())
            .status()
            .map_err(|source| ServiceError::Command {
                command: format!("is-enabled {UNIT_NAME}"),
                detail: format!("lancement impossible : {source}"),
            })?;
        Ok(status.success())
    }

    fn enable(&self) -> Result<(), ServiceError> {
        self.run(&["enable", UNIT_NAME])
    }

    fn install(&self, spec: &ServiceSpec) -> Result<(), ServiceError> {
        let text = render_unit(spec)?;
        self.write_unit(&text)?;
        self.run(&["daemon-reload"])?;
        self.run(&["enable", "--now", UNIT_NAME])
    }

    fn unit_text(&self) -> Result<Option<String>, ServiceError> {
        match std::fs::read_to_string(&self.unit_path) {
            Ok(text) => Ok(Some(text)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(self.unit_error(error)),
        }
    }

    fn restore_unit(&self, text: &str) -> Result<(), ServiceError> {
        self.write_unit(text)?;
        self.run(&["daemon-reload"])
    }

    fn restart(&self) -> Result<(), ServiceError> {
        self.run(&["restart", UNIT_NAME])
    }

    fn stop(&self) -> Result<(), ServiceError> {
        if !self.unit_path.exists() {
            return Ok(());
        }
        self.run(&["stop", UNIT_NAME])
    }

    fn disable(&self) -> Result<(), ServiceError> {
        if !self.unit_path.exists() {
            return Ok(());
        }
        self.run(&["disable", UNIT_NAME])
    }

    fn remove(&self) -> Result<(), ServiceError> {
        // Un fichier voisin resté d'une écriture interrompue part avec l'unité.
        let _ = std::fs::remove_file(self.unit_path.with_extension(UNIT_TEMP_EXTENSION));
        match std::fs::remove_file(&self.unit_path) {
            Ok(()) => self.run(&["daemon-reload"]),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(self.unit_error(error)),
        }
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn spec() -> ServiceSpec {
        ServiceSpec {
            binary: PathBuf::from("/usr/local/bin/hearth-agent"),
            config: PathBuf::from("/etc/hearth/agent.toml"),
            data_dir: PathBuf::from("/var/lib/hearth"),
        }
    }

    /// Un faux `systemctl` : enregistre chaque appel, et garde l'état « actif » dans un fichier.
    struct Fake {
        dir: crate::test_tmp::TestDir,
        systemd: Systemd,
    }

    impl Fake {
        fn new() -> Self {
            let dir = crate::test_tmp::tempdir().expect("dossier temporaire");
            let script = dir.path().join("systemctl");
            let log = dir.path().join("calls.log");
            let active = dir.path().join("active");
            std::fs::write(
                &script,
                format!(
                    "#!/bin/sh\necho \"$*\" >> {log}\ncase \"$1\" in\n  is-active) [ -e {active} ] ;;\n  enable) touch {active} ;;\n  stop) rm -f {active} ;;\n  restart) touch {active} ;;\n  fail) exit 1 ;;\n  show) echo \"ActiveState=$(cat {state} 2>/dev/null || echo inactive)\" ;;\n  *) ;;\nesac\n",
                    log = log.display(),
                    active = active.display(),
                    state = dir.path().join("show-state").display()
                ),
            )
            .expect("script");
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
                .expect("droits");
            let systemd = Systemd::new(
                dir.path().join("hearth-agent.service"),
                script.into_os_string(),
            );
            Self { dir, systemd }
        }

        fn calls(&self) -> Vec<String> {
            std::fs::read_to_string(self.dir.path().join("calls.log"))
                .unwrap_or_default()
                .lines()
                .map(str::to_owned)
                .collect()
        }

        fn unit(&self) -> String {
            std::fs::read_to_string(self.dir.path().join("hearth-agent.service")).expect("unité")
        }
    }

    #[test]
    fn the_unit_restarts_always_and_starts_with_the_system() {
        let text = render_unit(&spec()).unwrap();
        assert!(text.contains("Restart=always"));
        assert!(text.contains("RestartSec=5"));
        assert!(text.contains("WantedBy=multi-user.target"));
        assert!(text.contains(
            "ExecStart=/usr/local/bin/hearth-agent serve --config /etc/hearth/agent.toml"
        ));
        assert!(text.contains("After=network-online.target"));
        assert!(text.contains("ReadWritePaths=/var/lib/hearth /usr/local/bin"));
    }

    #[test]
    fn the_unit_carries_the_hardening_directives() {
        let text = render_unit(&spec()).unwrap();
        for directive in [
            "NoNewPrivileges=yes",
            "CapabilityBoundingSet=CAP_CHOWN",
            "RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6 AF_NETLINK",
            "RestrictNamespaces=yes",
            "SystemCallFilter=@system-service",
            "ProtectSystem=full",
            "ProtectHome=read-only",
            "PrivateTmp=yes",
            "UMask=0077",
            "RestrictSUIDSGID=yes",
        ] {
            assert!(text.contains(directive), "{directive}");
        }
    }

    #[test]
    fn the_unit_never_holds_a_secret_or_an_environment() {
        let text = render_unit(&spec()).unwrap();
        for forbidden in ["PASSWORD", "HASH", "Environment", "argon2", "Secret"] {
            assert!(!text.contains(forbidden), "{forbidden}");
        }
    }

    #[test]
    fn the_capability_set_leaves_out_what_the_agent_will_never_need() {
        let text = render_unit(&spec()).unwrap();
        let line = text
            .lines()
            .find(|line| line.starts_with("CapabilityBoundingSet="))
            .expect("ligne des capacités");
        for dropped in [
            "CAP_SYS_ADMIN",
            "CAP_SYS_PTRACE",
            "CAP_NET_ADMIN",
            "CAP_SYS_MODULE",
            "CAP_SYS_RAWIO",
        ] {
            assert!(!line.contains(dropped), "{dropped}");
        }
    }

    #[test]
    fn the_capability_set_keeps_mknod_for_the_first_nvidia_smi_on_a_headless_server() {
        let text = render_unit(&spec()).unwrap();
        let line = text
            .lines()
            .find(|line| line.starts_with("CapabilityBoundingSet="))
            .expect("ligne des capacités");
        assert!(line.contains("CAP_MKNOD"));
    }

    #[test]
    fn removing_the_unit_also_removes_a_leftover_temporary_unit() {
        let fake = Fake::new();
        fake.systemd.install(&spec()).unwrap();
        let leftover = fake.dir.path().join("hearth-agent.service.new");
        std::fs::write(&leftover, "à moitié écrite").unwrap();
        fake.systemd.remove().unwrap();
        assert!(!leftover.exists());
    }

    #[test]
    fn enabled_follows_the_unit_and_enable_does_not_start() {
        let fake = Fake::new();
        assert!(!fake.systemd.is_enabled().unwrap(), "pas d'unité");
        fake.systemd.install(&spec()).unwrap();
        fake.systemd.stop().unwrap();
        fake.systemd.enable().unwrap();
        assert_eq!(
            fake.calls().last().map(String::as_str),
            Some("enable hearth-agent.service")
        );
    }

    #[test]
    fn the_unit_text_can_be_read_and_restored_as_it_was() {
        let fake = Fake::new();
        assert_eq!(fake.systemd.unit_text().unwrap(), None);
        fake.systemd.install(&spec()).unwrap();
        let before = fake.systemd.unit_text().unwrap().expect("unité");
        let mut other = spec();
        other.config = PathBuf::from("/etc/hearth/other.toml");
        fake.systemd.install(&other).unwrap();
        assert_ne!(fake.systemd.unit_text().unwrap().unwrap(), before);
        fake.systemd.restore_unit(&before).unwrap();
        assert_eq!(fake.systemd.unit_text().unwrap().unwrap(), before);
        assert_eq!(
            fake.calls().last().map(String::as_str),
            Some("daemon-reload")
        );
    }

    #[test]
    fn a_path_cannot_inject_a_directive() {
        for bad in [
            "/usr/local/bin/x\nExecStartPre=/bin/evil",
            "/usr/local/bin/x y",
            "relative/path",
            "/usr/local/bin/$HOME",
            "",
        ] {
            let mut evil = spec();
            evil.binary = PathBuf::from(bad);
            assert!(render_unit(&evil).is_err(), "{bad:?}");
            let mut evil = spec();
            evil.data_dir = PathBuf::from(bad);
            assert!(render_unit(&evil).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn install_writes_the_unit_reloads_and_enables_now() {
        let fake = Fake::new();
        assert!(!fake.systemd.is_installed().unwrap());
        fake.systemd.install(&spec()).unwrap();
        assert!(fake.systemd.is_installed().unwrap());
        assert_eq!(
            fake.calls(),
            ["daemon-reload", "enable --now hearth-agent.service"]
        );
        assert!(fake.unit().contains("Restart=always"));
        let mode = std::fs::metadata(fake.dir.path().join("hearth-agent.service"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o644);
        assert!(
            !fake.dir.path().join("hearth-agent.service.new").exists(),
            "pas de fichier temporaire"
        );
    }

    #[test]
    fn the_service_state_tells_a_stop_from_a_service_that_keeps_failing() {
        for (text, expected) in [
            (
                "ActiveState=active
",
                ServiceState::Active,
            ),
            (
                "ActiveState=activating
",
                ServiceState::Activating,
            ),
            (
                "ActiveState=inactive
",
                ServiceState::Inactive,
            ),
            (
                "ActiveState=failed
",
                ServiceState::Failed,
            ),
            (
                "ActiveState=quelque-chose
",
                ServiceState::Unknown,
            ),
            ("", ServiceState::Unknown),
        ] {
            assert_eq!(parse_active_state(text), expected, "{text:?}");
        }
    }

    #[test]
    fn the_state_is_read_with_systemctl_show_and_a_silent_system_is_unknown() {
        let fake = Fake::new();
        assert_eq!(fake.systemd.state(), ServiceState::Inactive, "pas d'unité");
        fake.systemd.install(&spec()).unwrap();
        for (text, expected) in [
            ("activating", ServiceState::Activating),
            ("failed", ServiceState::Failed),
            ("inactive", ServiceState::Inactive),
            ("active", ServiceState::Active),
        ] {
            std::fs::write(fake.dir.path().join("show-state"), text).unwrap();
            assert_eq!(fake.systemd.state(), expected, "{text}");
            assert_eq!(
                fake.calls().last().map(String::as_str),
                Some("show --property=ActiveState hearth-agent.service")
            );
        }
        let broken = Systemd::new(
            fake.dir.path().join("hearth-agent.service"),
            OsString::from("/bin/false"),
        );
        assert_eq!(broken.state(), ServiceState::Unknown);
    }

    #[test]
    fn is_active_follows_the_service() {
        let fake = Fake::new();
        assert!(!fake.systemd.is_active().unwrap(), "pas d'unité");
        fake.systemd.install(&spec()).unwrap();
        assert!(fake.systemd.is_active().unwrap());
        fake.systemd.stop().unwrap();
        assert!(!fake.systemd.is_active().unwrap());
    }

    #[test]
    fn restart_stop_disable_and_remove_call_systemctl_in_order() {
        let fake = Fake::new();
        fake.systemd.install(&spec()).unwrap();
        fake.systemd.restart().unwrap();
        fake.systemd.stop().unwrap();
        fake.systemd.disable().unwrap();
        fake.systemd.remove().unwrap();
        assert_eq!(
            fake.calls(),
            [
                "daemon-reload",
                "enable --now hearth-agent.service",
                "restart hearth-agent.service",
                "stop hearth-agent.service",
                "disable hearth-agent.service",
                "daemon-reload",
            ]
        );
        assert!(!fake.systemd.is_installed().unwrap());
    }

    #[test]
    fn stopping_disabling_or_removing_without_a_unit_does_nothing_and_is_not_an_error() {
        let fake = Fake::new();
        fake.systemd.stop().unwrap();
        fake.systemd.disable().unwrap();
        fake.systemd.remove().unwrap();
        assert!(fake.calls().is_empty());
    }

    #[test]
    fn a_failing_systemctl_is_an_error_naming_the_command() {
        let fake = Fake::new();
        let broken = Systemd::new(
            fake.dir.path().join("hearth-agent.service"),
            OsString::from("/bin/false"),
        );
        let error = broken.install(&spec()).unwrap_err();
        assert!(error.to_string().contains("daemon-reload"), "{error}");
    }

    #[test]
    fn a_missing_systemctl_is_an_error_not_a_panic() {
        let dir = crate::test_tmp::tempdir().unwrap();
        let systemd = Systemd::new(
            dir.path().join("hearth-agent.service"),
            OsString::from("/nonexistent/systemctl"),
        );
        let error = systemd.install(&spec()).unwrap_err();
        assert!(
            error.to_string().contains("lancement impossible"),
            "{error}"
        );
    }
}
