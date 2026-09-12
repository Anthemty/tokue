// config.rs — persisted configuration (~/.config/ocg/config.json).
//
// Faithful port of ocg.go's Config, loadConfig, saveConfig, and the legacy
// single-provider migration.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::providers;

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(rename = "active_provider", default)]
    pub active_provider: String,
    #[serde(default)]
    pub opencode: OpenCodeConfig,
    #[serde(default)]
    pub deepseek: DeepSeekConfig,
    #[serde(default)]
    pub minimax: MinimaxConfig,
    #[serde(default)]
    pub codex: CodexConfig,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct OpenCodeConfig {
    #[serde(rename = "workspace_id", default)]
    pub workspace_id: String,
    #[serde(rename = "auth_cookie", default)]
    pub auth_cookie: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct DeepSeekConfig {
    #[serde(rename = "api_key", default)]
    pub api_key: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct MinimaxConfig {
    #[serde(rename = "api_key", default)]
    pub api_key: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct CodexConfig {
    #[serde(rename = "api_key", default)]
    pub api_key: String,
    #[serde(rename = "org_id", default)]
    pub org_id: String,
}

/// Legacy single-provider format: workspace_id / auth_cookie at the top level.
#[derive(Deserialize, Default)]
struct LegacyConfig {
    #[serde(rename = "workspace_id", default)]
    workspace_id: String,
    #[serde(rename = "auth_cookie", default)]
    auth_cookie: String,
}

fn config_dir() -> io::Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no home dir"))?;
    let dir = home.join(".config").join("ocg");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn config_path() -> io::Result<PathBuf> {
    Ok(config_dir()?.join("config.json"))
}

/// Load config, migrating legacy format if needed. Never fails: on any error
/// returns a default config with active_provider = opencode (matches Go loadCfg).
pub fn load() -> Config {
    match load_inner() {
        Ok(cfg) => cfg,
        Err(_) => default_config(),
    }
}

fn default_config() -> Config {
    Config { active_provider: providers::OPENCODE.to_string(), ..Default::default() }
}

fn load_inner() -> io::Result<Config> {
    let path = config_path()?;
    let data = match fs::read(&path) {
        Ok(d) => d,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(default_config()),
        Err(e) => return Err(e),
    };

    // Try new format first.
    let mut cfg: Config = match serde_json::from_slice(&data) {
        Ok(c) => c,
        Err(_) => return Ok(default_config()),
    };

    // Detect legacy top-level workspace_id/auth_cookie and migrate.
    if let Ok(old) = serde_json::from_slice::<LegacyConfig>(&data) {
        if !old.workspace_id.is_empty() || !old.auth_cookie.is_empty() {
            cfg.opencode.workspace_id = old.workspace_id;
            cfg.opencode.auth_cookie = old.auth_cookie;
            if cfg.active_provider.is_empty() {
                cfg.active_provider = providers::OPENCODE.to_string();
            }
            let _ = save(&mut cfg); // persist migrated form
        }
    }

    if cfg.active_provider.is_empty() {
        cfg.active_provider = providers::OPENCODE.to_string();
    }
    Ok(cfg)
}

/// Save config (2-space indented JSON, 0600). Ensures active_provider is set.
pub fn save(cfg: &mut Config) -> io::Result<()> {
    if cfg.active_provider.is_empty() {
        cfg.active_provider = providers::OPENCODE.to_string();
    }
    let path = config_path()?;
    let data = serde_json::to_vec_pretty(cfg).unwrap_or_default();
    // 2-space indentation (serde_json::to_vec_pretty already uses 2 spaces).
    fs::write(path, data)?;
    Ok(())
}
