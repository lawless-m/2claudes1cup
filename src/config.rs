use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    #[serde(default = "default_listen_port")]
    pub listen_port: u16,
    #[serde(default = "default_db_path")]
    pub db_path: String,
    pub token: Option<String>,
}

fn default_listen_addr() -> String {
    "0.0.0.0".to_string()
}
fn default_listen_port() -> u16 {
    9229
}
fn default_db_path() -> String {
    "c2c-messages.db".to_string()
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_listen_addr(),
            listen_port: default_listen_port(),
            db_path: default_db_path(),
            token: None,
        }
    }
}

pub fn load(path: Option<&str>) -> anyhow::Result<Config> {
    match path {
        Some(p) if Path::new(p).exists() => {
            let content = std::fs::read_to_string(p)?;
            Ok(toml::from_str(&content)?)
        }
        _ => Ok(Config {
            server: ServerConfig::default(),
        }),
    }
}
