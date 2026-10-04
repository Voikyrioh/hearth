//! Configuration de l'agent : valeurs par défaut, puis `agent.toml`, puis variables `HEARTH_*`,
//! puis options de la ligne de commande (la dernière couche l'emporte).

use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};

use hearth_proto::product::DEFAULT_PORT;
use serde::Deserialize;
use thiserror::Error;

pub const ENV_CONFIG: &str = "HEARTH_CONFIG";
pub const ENV_PORT: &str = "HEARTH_PORT";
pub const ENV_LISTEN_ADDR: &str = "HEARTH_LISTEN_ADDR";
pub const ENV_DATA_DIR: &str = "HEARTH_DATA_DIR";
pub const ENV_MANAGED: &str = "HEARTH_MANAGED";

/// Configuration effective, prête à l'emploi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentConfig {
    pub listen_addr: IpAddr,
    pub port: u16,
    pub data_dir: PathBuf,
    /// Installation gérée de l'extérieur (pas de mise à jour automatique).
    pub managed: bool,
}

/// Options venues de la ligne de commande.
#[derive(Debug, Clone, Default)]
pub struct CliOverrides {
    pub config_path: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("fichier de configuration {0} introuvable")]
    FileNotFound(PathBuf),
    #[error("lecture de {path} impossible : {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("fichier de configuration {path} invalide : {message}")]
    Parse { path: PathBuf, message: String },
    #[error("valeur invalide pour {name} : {value:?}")]
    BadEnv { name: &'static str, value: String },
    #[error("dossier de données par défaut introuvable : définis --data-dir ou HEARTH_DATA_DIR")]
    NoDefaultDataDir,
}

/// Contenu de `agent.toml` : tout est facultatif.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    listen_addr: Option<IpAddr>,
    port: Option<u16>,
    data_dir: Option<PathBuf>,
    managed: Option<bool>,
}

/// Assemble la configuration. `env` donne accès aux variables d'environnement
/// (injectable pour les tests).
pub fn load(
    cli: &CliOverrides,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<AgentConfig, ConfigError> {
    let file = read_file_config(cli, env)?;

    let mut listen_addr = file
        .listen_addr
        .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    let mut port = file.port.unwrap_or(DEFAULT_PORT);
    let mut managed = file.managed.unwrap_or(false);
    let mut data_dir = file.data_dir;

    if let Some(value) = env(ENV_LISTEN_ADDR) {
        listen_addr = parse_env(ENV_LISTEN_ADDR, &value)?;
    }
    if let Some(value) = env(ENV_PORT) {
        port = parse_env(ENV_PORT, &value)?;
    }
    if let Some(value) = env(ENV_MANAGED) {
        managed = parse_bool(ENV_MANAGED, &value)?;
    }
    if let Some(value) = env(ENV_DATA_DIR) {
        data_dir = Some(PathBuf::from(value));
    }
    if let Some(dir) = &cli.data_dir {
        data_dir = Some(dir.clone());
    }

    let data_dir = match data_dir {
        Some(dir) => dir,
        None => default_data_dir(env)?,
    };
    Ok(AgentConfig {
        listen_addr,
        port,
        data_dir,
        managed,
    })
}

fn read_file_config(
    cli: &CliOverrides,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<FileConfig, ConfigError> {
    // Un chemin demandé explicitement doit exister ; le chemin par défaut est facultatif.
    let explicit = cli
        .config_path
        .clone()
        .or_else(|| env(ENV_CONFIG).map(PathBuf::from));
    let (path, required) = match explicit {
        Some(path) => (path, true),
        None => match default_config_path(env) {
            Some(path) => (path, false),
            None => return Ok(FileConfig::default()),
        },
    };

    match std::fs::read_to_string(&path) {
        Ok(text) => parse_file(&path, &text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if required {
                Err(ConfigError::FileNotFound(path))
            } else {
                Ok(FileConfig::default())
            }
        }
        Err(source) => Err(ConfigError::Read { path, source }),
    }
}

fn parse_file(path: &Path, text: &str) -> Result<FileConfig, ConfigError> {
    toml::from_str(text).map_err(|e| ConfigError::Parse {
        path: path.to_owned(),
        message: e.to_string(),
    })
}

fn parse_env<T: std::str::FromStr>(name: &'static str, value: &str) -> Result<T, ConfigError> {
    value.trim().parse().map_err(|_| ConfigError::BadEnv {
        name,
        value: value.to_owned(),
    })
}

fn parse_bool(name: &'static str, value: &str) -> Result<bool, ConfigError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(ConfigError::BadEnv {
            name,
            value: value.to_owned(),
        }),
    }
}

/// Linux : `/var/lib/hearth`.
#[cfg(not(windows))]
fn default_data_dir(_env: &dyn Fn(&str) -> Option<String>) -> Result<PathBuf, ConfigError> {
    Ok(PathBuf::from("/var/lib/hearth"))
}

/// Windows (mode dev) : `hearth-agent` dans le dossier de données local de l'utilisateur.
#[cfg(windows)]
fn default_data_dir(env: &dyn Fn(&str) -> Option<String>) -> Result<PathBuf, ConfigError> {
    env("LOCALAPPDATA")
        .map(|base| PathBuf::from(base).join("hearth-agent"))
        .ok_or(ConfigError::NoDefaultDataDir)
}

/// Linux : `/etc/hearth/agent.toml`.
#[cfg(not(windows))]
fn default_config_path(_env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    Some(PathBuf::from("/etc/hearth/agent.toml"))
}

/// Windows (mode dev) : `agent.toml` dans le dossier de données par défaut.
#[cfg(windows)]
fn default_config_path(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    default_data_dir(env).ok().map(|dir| dir.join("agent.toml"))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| map.get(name).cloned()
    }

    /// Environnement qui ne dépend pas de la machine : fichier explicite vide, dossier explicite.
    fn isolated(dir: &Path, extra: &[(&str, &str)]) -> (CliOverrides, HashMap<String, String>) {
        let cfg = dir.join("agent.toml");
        if !cfg.exists() {
            std::fs::write(&cfg, "").expect("write");
        }
        let cli = CliOverrides {
            config_path: Some(cfg),
            data_dir: None,
        };
        let env = extra
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        (cli, env)
    }

    fn lookup(map: &HashMap<String, String>) -> impl Fn(&str) -> Option<String> + '_ {
        move |name| map.get(name).cloned()
    }

    #[test]
    fn defaults_apply_without_any_source() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (cli, mut env) = isolated(dir.path(), &[]);
        env.insert("LOCALAPPDATA".into(), "C:/Users/test/AppData/Local".into());
        let config = load(&cli, &lookup(&env)).expect("config");
        assert_eq!(config.port, 7341);
        assert_eq!(config.listen_addr, IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        assert!(!config.managed);
        #[cfg(not(windows))]
        assert_eq!(config.data_dir, PathBuf::from("/var/lib/hearth"));
        #[cfg(windows)]
        assert_eq!(
            config.data_dir,
            PathBuf::from("C:/Users/test/AppData/Local").join("hearth-agent")
        );
    }

    #[test]
    fn file_overrides_defaults() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (cli, env) = isolated(dir.path(), &[]);
        std::fs::write(
            dir.path().join("agent.toml"),
            "port = 9000\nlisten_addr = \"127.0.0.1\"\ndata_dir = \"/srv/h\"\nmanaged = true\n",
        )
        .expect("write");
        let config = load(&cli, &lookup(&env)).expect("config");
        assert_eq!(config.port, 9000);
        assert_eq!(
            config.listen_addr,
            "127.0.0.1".parse::<IpAddr>().expect("ip")
        );
        assert_eq!(config.data_dir, PathBuf::from("/srv/h"));
        assert!(config.managed);
    }

    #[test]
    fn env_overrides_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (cli, _) = isolated(dir.path(), &[]);
        std::fs::write(
            dir.path().join("agent.toml"),
            "port = 9000\nmanaged = true\n",
        )
        .expect("write");
        let env = env_of(&[
            (ENV_PORT, "9100"),
            (ENV_MANAGED, "false"),
            (ENV_LISTEN_ADDR, "::1"),
            (ENV_DATA_DIR, "/env/data"),
        ]);
        let config = load(&cli, &env).expect("config");
        assert_eq!(config.port, 9100);
        assert!(!config.managed);
        assert_eq!(config.listen_addr, "::1".parse::<IpAddr>().expect("ip"));
        assert_eq!(config.data_dir, PathBuf::from("/env/data"));
    }

    #[test]
    fn cli_data_dir_overrides_env_and_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (mut cli, _) = isolated(dir.path(), &[]);
        cli.data_dir = Some(PathBuf::from("/cli/data"));
        let env = env_of(&[(ENV_DATA_DIR, "/env/data")]);
        let config = load(&cli, &env).expect("config");
        assert_eq!(config.data_dir, PathBuf::from("/cli/data"));
    }

    #[test]
    fn config_path_can_come_from_the_environment() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("elsewhere.toml");
        std::fs::write(&path, "port = 9200\n").expect("write");
        let path_text = path.to_string_lossy().into_owned();
        let env = env_of(&[(ENV_CONFIG, path_text.as_str()), (ENV_DATA_DIR, "/d")]);
        let config = load(&CliOverrides::default(), &env).expect("config");
        assert_eq!(config.port, 9200);
    }

    #[test]
    fn explicit_missing_file_is_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cli = CliOverrides {
            config_path: Some(dir.path().join("absent.toml")),
            data_dir: Some(PathBuf::from("/d")),
        };
        let err = load(&cli, &env_of(&[])).expect_err("doit échouer");
        assert!(matches!(err, ConfigError::FileNotFound(_)));
    }

    #[test]
    fn bad_values_are_reported() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (cli, _) = isolated(dir.path(), &[]);
        let err = load(&cli, &env_of(&[(ENV_PORT, "abc"), (ENV_DATA_DIR, "/d")]))
            .expect_err("port invalide");
        assert!(matches!(err, ConfigError::BadEnv { name: ENV_PORT, .. }));
        let err = load(
            &cli,
            &env_of(&[(ENV_MANAGED, "peut-être"), (ENV_DATA_DIR, "/d")]),
        )
        .expect_err("booléen invalide");
        assert!(matches!(
            err,
            ConfigError::BadEnv {
                name: ENV_MANAGED,
                ..
            }
        ));
    }

    #[test]
    fn unknown_keys_in_the_file_are_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (cli, _) = isolated(dir.path(), &[]);
        std::fs::write(dir.path().join("agent.toml"), "prot = 1\n").expect("write");
        let err = load(&cli, &env_of(&[(ENV_DATA_DIR, "/d")])).expect_err("clé inconnue");
        assert!(matches!(err, ConfigError::Parse { .. }));
    }
}
