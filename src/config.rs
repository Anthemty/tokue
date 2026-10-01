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
    /// UI language: "" follows the system, else "en" or "zh-Hans".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub language: String,
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
    /// Which card (a meter's `key`, e.g. a Codex account's CODEX_HOME) drives
    /// the menu bar badge for a provider with more than one — provider id ->
    /// selected key. A missing entry means "no pin", i.e. keep using the
    /// provider's own overall (window-priority) criticality.
    #[serde(rename = "selected_keys", default)]
    pub selected_keys: std::collections::BTreeMap<String, String>,
    /// Per-account display settings (label, shown) for providers other than
    /// Codex, which keeps its own under `codex.accounts`: provider id -> list.
    #[serde(rename = "account_settings", default)]
    pub account_settings: std::collections::BTreeMap<String, Vec<CodexAccount>>,
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

    /// Whether meters read as quota left rather than quota used. The setting
    /// is presented once, for the whole popover, and every provider honours
    /// it; it is stored under `codex` only because that is where it began.
    pub fn show_remaining(&self) -> bool {
        self.codex.show_remaining
    }

    /// Background refresh interval in minutes: the Preferences value, else the
    /// legacy Codex-only override, else 15.
    pub fn effective_refresh_minutes(&self) -> u32 {
        self.refresh_minutes
            .or(self.codex.refresh_minutes)
            .unwrap_or(15)
            .clamp(1, 240)
    }

    fn account_list(&self, provider: &str) -> &[CodexAccount] {
        if provider == providers::CODEX {
            &self.codex.accounts
        } else {
            self.account_settings.get(provider).map(|v| v.as_slice()).unwrap_or(&[])
        }
    }

    /// Whether an account is shown in the panel (default: yes).
    pub fn account_shown(&self, provider: &str, key: &str) -> bool {
        self.account_list(provider).iter().find(|a| a.key == key).map(|a| a.enabled).unwrap_or(true)
    }

    /// The user's own name for an account, or "".
    pub fn account_label(&self, provider: &str, key: &str) -> String {
        self.account_list(provider)
            .iter()
            .find(|a| a.key == key)
            .map(|a| a.label.clone())
            .unwrap_or_default()
    }

    /// Record an account's label and whether it is shown.
    pub fn set_account(&mut self, provider: &str, key: &str, label: &str, shown: bool) {
        let list = if provider == providers::CODEX {
            &mut self.codex.accounts
        } else {
            self.account_settings.entry(provider.to_string()).or_default()
        };
        match list.iter_mut().find(|a| a.key == key) {
            Some(a) => {
                a.label = label.to_string();
                a.enabled = shown;
            }
            None => list.push(CodexAccount { key: key.to_string(), label: label.to_string(), enabled: shown }),
        }
    }

    /// Forget an account's display settings, and unpin it.
    pub fn forget_account(&mut self, provider: &str, key: &str) {
        if provider == providers::CODEX {
            self.codex.accounts.retain(|a| a.key != key);
        } else if let Some(list) = self.account_settings.get_mut(provider) {
            list.retain(|a| a.key != key);
        }
        if self.selected_key(provider) == Some(key) {
            self.set_selected_key(provider, None);
        }
    }

    pub fn selected_key(&self, provider: &str) -> Option<&str> {
        self.selected_keys.get(provider).map(|s| s.as_str())
    }

    /// `None` (or an empty key) clears the pin — the provider goes back to
    /// reporting its own overall criticality.
    pub fn set_selected_key(&mut self, provider: &str, key: Option<&str>) {
        match key {
            Some(k) if !k.is_empty() => {
                self.selected_keys.insert(provider.to_string(), k.to_string());
            }
            _ => {
                self.selected_keys.remove(provider);
            }
        }
    }
}

/// OpenCode Go: a subscription billed monthly, reached through the same
/// gateway the models use, with an API key from the Zen console.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct OpenCodeConfig {
    #[serde(rename = "api_key", default)]
    pub api_key: String,
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

/// Codex: ChatGPT subscription usage. `accounts` holds per-account display
/// settings only; an account missing from it is shown, unlabelled.
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

/// Display settings for one ChatGPT account. The logins themselves live in
/// ~/.codex and ~/.config/ocg/accounts/codex.json (see codex_accounts.rs).
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct CodexAccount {
    /// The account's key (`codex:<user>__<workspace>`). Configs from before
    /// the account store held a CODEX_HOME path here, under `home`; the
    /// one-time migration rewrites those into keys.
    #[serde(alias = "home")]
    pub key: String,
    #[serde(default)]
    pub label: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Where tokue keeps its data: ~/.config/tokue. Until OCG's data has been
/// moved over (see migrate_from_ocg) it is still read where OCG left it.
pub fn data_dir() -> PathBuf {
    let config = dirs::home_dir().unwrap_or_default().join(".config");
    let new = config.join("tokue");
    let old = config.join("ocg");
    if !new.exists() && old.is_dir() { old } else { new }
}

/// The SQLite store inside data_dir: tokue.db, or ocg.db until moved.
pub fn db_path() -> PathBuf {
    let dir = data_dir();
    let name = if dir.ends_with("ocg") { "ocg.db" } else { "tokue.db" };
    dir.join(name)
}

/// One time: OCG's data becomes tokue's. Run before anything opens the store.
///
/// The directory is renamed, never copied — a saved login's refresh token
/// must have exactly one copy. The database is checkpointed first, so the
/// whole of it is in ocg.db and the file can be renamed on its own; if that
/// cannot be done (a connection is mid-read or mid-write), nothing moves and
/// the data keeps being used where it is, until a later launch. An OCG left
/// running but idle would not be noticed, so it is quit before tokue starts.
pub fn migrate_from_ocg() {
    let config = dirs::home_dir().unwrap_or_default().join(".config");
    match migrate_dir(&config.join("ocg"), &config.join("tokue")) {
        Ok(true) => eprintln!("tokue: moved ~/.config/ocg to ~/.config/tokue"),
        Ok(false) => {}
        Err(e) => eprintln!("tokue: kept ~/.config/ocg for now: {}", e),
    }
}

fn migrate_dir(old: &std::path::Path, new: &std::path::Path) -> Result<bool, String> {
    if new.exists() || !old.is_dir() {
        return Ok(false);
    }
    let db = old.join("ocg.db");
    if db.exists() {
        let conn = rusqlite::Connection::open(&db).map_err(|e| e.to_string())?;
        let busy: i64 = conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        if busy != 0 {
            return Err("ocg.db is in use".into());
        }
        conn.close().map_err(|(_, e)| e.to_string())?;
        if fs::metadata(old.join("ocg.db-wal")).map(|m| m.len() > 0).unwrap_or(false) {
            return Err("ocg.db-wal still holds data".into());
        }
    }
    fs::rename(old, new).map_err(|e| e.to_string())?;
    if new.join("ocg.db").exists() {
        fs::rename(new.join("ocg.db"), new.join("tokue.db")).map_err(|e| e.to_string())?;
        for leftover in ["ocg.db-wal", "ocg.db-shm"] {
            let _ = fs::remove_file(new.join(leftover));
        }
    }
    Ok(true)
}

fn config_dir() -> io::Result<PathBuf> {
    let dir = data_dir();
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn config_path() -> io::Result<PathBuf> {
    Ok(config_dir()?.join("config.json"))
}

/// ~/.config/tokue — also holds tokue.db (usage history + last-good snapshots).
pub fn config_dir_path() -> io::Result<PathBuf> {
    config_dir()
}

/// ~/.config/tokue/cache — legacy JSON location, migrated into the store on first run.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ocg_data_moves_whole_with_its_unflushed_writes() {
        let root = std::env::temp_dir().join(format!("tokue-migrate-test-{}", std::process::id()));
        let (old, new) = (root.join("ocg"), root.join("tokue"));
        fs::create_dir_all(old.join("accounts")).unwrap();
        fs::write(old.join("accounts").join("codex.json"), b"{\"accounts\":[]}").unwrap();
        // A WAL database with a write still in the log: the writer stays open,
        // and auto-checkpointing is off, so the row lives only in ocg.db-wal.
        let writer = rusqlite::Connection::open(old.join("ocg.db")).unwrap();
        writer.pragma_update(None, "journal_mode", "WAL").unwrap();
        writer.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
        writer.execute_batch("CREATE TABLE t(v TEXT); INSERT INTO t VALUES ('kept');").unwrap();
        assert!(fs::metadata(old.join("ocg.db-wal")).unwrap().len() > 0);

        // While it is held open the move does not happen and nothing is lost.
        let mut held = rusqlite::Connection::open(old.join("ocg.db")).unwrap();
        let tx = held.transaction().unwrap();
        tx.query_row("SELECT count(*) FROM t", [], |r| r.get::<_, i64>(0)).unwrap();
        assert!(migrate_dir(&old, &new).is_err());
        assert!(old.join("ocg.db").exists() && !new.exists());
        drop(tx);
        drop(held);
        drop(writer);

        assert_eq!(migrate_dir(&old, &new), Ok(true));
        assert!(!old.exists());
        assert!(new.join("accounts").join("codex.json").exists());
        assert!(!new.join("ocg.db-wal").exists() && !new.join("ocg.db").exists());
        let conn = rusqlite::Connection::open(new.join("tokue.db")).unwrap();
        let v: String = conn.query_row("SELECT v FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(v, "kept");
        // Once moved, it is not done again.
        assert_eq!(migrate_dir(&old, &new), Ok(false));
        fs::remove_dir_all(&root).ok();
    }
}
