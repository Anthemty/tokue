// config.rs — persisted configuration (~/.config/ocg/config.json).
//
// Faithful port of ocg.go's Config, loadConfig, saveConfig, and the legacy
// single-provider migration.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::providers;
use crate::store;

pub const CONFIG_KEY: &str = "config";

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
    /// Per-provider enable switches; a missing entry means enabled.
    #[serde(rename = "provider_enabled", default)]
    pub provider_enabled: std::collections::BTreeMap<String, bool>,
}

impl Config {
    pub fn provider_enabled(&self, id: &str) -> bool {
        self.provider_enabled.get(id).copied().unwrap_or(true)
    }

    pub fn set_provider_enabled(&mut self, id: &str, enabled: bool) {
        self.provider_enabled.insert(id.to_string(), enabled);
    }

    /// First enabled provider, for falling back off a disabled active one.
    pub fn first_enabled_provider(&self) -> Option<&'static str> {
        providers::PROVIDERS
            .iter()
            .copied()
            .find(|id| self.provider_enabled(id))
    }
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
    /// Show the "Reset credits" row (free window resets the plan grants). Off
    /// by default: it is not quota and most accounts never have any.
    #[serde(rename = "show_reset_credits", default)]
    pub show_reset_credits: bool,
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

/// Load config from the SQLite store, migrating the legacy config.json once.
/// Never fails: on any error returns a default config with
/// active_provider = opencode (matches Go loadCfg).
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
    // 1. The database is the source of truth.
    if let Some(json) = store::load_config(CONFIG_KEY) {
        let mut cfg: Config = serde_json::from_str(&json).unwrap_or_else(|_| default_config());
        normalise(&mut cfg);
        return Ok(cfg);
    }

    // 2. One-time migration from the legacy config.json.
    let path = config_path()?;
    let data = match fs::read(&path) {
        Ok(d) => d,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(default_config()),
        Err(e) => return Err(e),
    };
    let mut cfg: Config = serde_json::from_slice(&data).unwrap_or_else(|_| default_config());
    if let Ok(old) = serde_json::from_slice::<LegacyConfig>(&data) {
        if !old.workspace_id.is_empty() || !old.auth_cookie.is_empty() {
            cfg.opencode.workspace_id = old.workspace_id;
            cfg.opencode.auth_cookie = old.auth_cookie;
        }
    }
    normalise(&mut cfg);
    let _ = save(&mut cfg);
    let _ = fs::rename(&path, path.with_extension("json.migrated"));
    Ok(cfg)
}

fn normalise(cfg: &mut Config) {
    if cfg.active_provider.is_empty() {
        cfg.active_provider = providers::OPENCODE.to_string();
    }
}

/// Save config into the database as pretty JSON.
pub fn save(cfg: &mut Config) -> io::Result<()> {
    normalise(cfg);
    let data = serde_json::to_string_pretty(cfg)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    store::save_config(CONFIG_KEY, &data);
    Ok(())
}
