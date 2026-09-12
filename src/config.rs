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
    /// Refresh interval override, in minutes (default 15).
    #[serde(rename = "refresh_minutes", default, skip_serializing_if = "Option::is_none")]
    pub refresh_minutes: Option<u32>,
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

/// Codex: one or more ChatGPT subscription logins, each in its own CODEX_HOME
/// (~/.codex, ~/.codex2, …). An empty `accounts` list means "auto-discover".
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct CodexConfig {
    #[serde(default)]
    pub accounts: Vec<CodexAccount>,
    /// Per-provider refresh interval override, in minutes.
    #[serde(rename = "refresh_minutes", default, skip_serializing_if = "Option::is_none")]
    pub refresh_minutes: Option<u32>,
    /// Show the workspace spend-control row. Off by default: it is a 0-credit
    /// cap on most plans and only adds noise.
    #[serde(rename = "show_spend", default)]
    pub show_spend: bool,
    /// Read the panel meters as quota left instead of quota used. Defaults to
    /// remaining quota; the menu bar badge always reports used severity.
    #[serde(rename = "show_remaining", default = "default_true")]
    pub show_remaining: bool,
    /// Show a per-account "Today" row (quota burned since local midnight,
    /// computed from the SQLite history). Off by default.
    #[serde(rename = "show_today", default)]
    pub show_today: bool,
    /// Legacy API-billing fields (pre-subscription builds). Loaded so old
    /// configs keep parsing, dropped on the next save. `allow(dead_code)`
    /// because nothing reads them any more.
    #[allow(dead_code)]
    #[serde(rename = "api_key", default, skip_serializing)]
    pub api_key: String,
    #[allow(dead_code)]
    #[serde(rename = "org_id", default, skip_serializing)]
    pub org_id: String,
}

/// One ChatGPT login: a CODEX_HOME path plus optional display name.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct CodexAccount {
    /// Home directory, `~` allowed.
    pub home: String,
    #[serde(default)]
    pub label: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
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

/// ~/.config/ocg — also holds ocg.db (usage history + last-good snapshots).
pub fn config_dir_path() -> io::Result<PathBuf> {
    config_dir()
}

/// ~/.config/ocg/cache — legacy JSON location, migrated into ocg.db on first run.
pub fn cache_dir() -> io::Result<PathBuf> {
    let dir = config_dir()?.join("cache");
    fs::create_dir_all(&dir)?;
    Ok(dir)
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
