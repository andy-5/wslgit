use serde::Deserialize;
use std::env;
use std::path::PathBuf;

#[derive(Debug)]
pub struct Config {
    pub host: String,
    pub root: String,
    pub control_persist: String,
    pub socket_dir: PathBuf,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
}

#[derive(Deserialize, Default)]
struct ConfigFile {
    host: Option<String>,
    root: Option<String>,
    control_persist: Option<String>,
    socket_dir: Option<String>,
    port: Option<u16>,
    identity_file: Option<String>,
}

impl Config {
    pub fn load() -> Result<Self, String> {
        let mut file_cfg = ConfigFile::default();

        if let Some(path) = config_file_path() {
            if path.exists() {
                let contents = std::fs::read_to_string(&path)
                    .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
                file_cfg = toml::from_str(&contents)
                    .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))?;
            }
        }

        // Env vars override config file values
        let host = env::var("SSHGIT_HOST")
            .ok()
            .or(file_cfg.host)
            .ok_or("'host' not configured — set SSHGIT_HOST or add 'host' to ~/.config/sshgit/config.toml")?;

        let root = env::var("SSHGIT_ROOT")
            .ok()
            .or(file_cfg.root)
            .ok_or("'root' not configured — set SSHGIT_ROOT or add 'root' to ~/.config/sshgit/config.toml")?;

        let control_persist = env::var("SSHGIT_CONTROL_PERSIST")
            .ok()
            .or(file_cfg.control_persist)
            .unwrap_or_else(|| "10m".to_string());

        let socket_dir = env::var("SSHGIT_SOCKET_DIR")
            .ok()
            .or(file_cfg.socket_dir)
            .map(|s| expand_tilde(&s))
            .unwrap_or_else(default_socket_dir);

        Ok(Config {
            host,
            root,
            control_persist,
            socket_dir,
            port: file_cfg.port,
            identity_file: file_cfg.identity_file,
        })
    }
}

fn home_dir() -> Option<PathBuf> {
    env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .ok()
        .map(PathBuf::from)
}

fn default_socket_dir() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ssh")
        .join("sshgit")
}

fn config_file_path() -> Option<PathBuf> {
    let config_home = if let Ok(xdg) = env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg)
    } else {
        home_dir()?.join(".config")
    };
    Some(config_home.join("sshgit").join("config.toml"))
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(rest)
    } else {
        PathBuf::from(path)
    }
}
