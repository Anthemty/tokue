// codex_accounts.rs — the ChatGPT logins Codex usage is read for.
//
// One account is Codex's own: the login in ~/.codex/auth.json, which the Codex
// CLI uses and refreshes. Every other account lives in ocg's store,
// ~/.config/ocg/accounts/codex.json, in exactly the auth.json shape `codex
// login` writes, and ocg refreshes those itself. A refresh token is single-use
// and rotates, so each one has exactly one holder: switching an account into
// Codex moves it out of the store, and moves Codex's previous account in.
//
// Accounts are identified by user × workspace (`chatgpt_account_user_id`),
// never by `chatgpt_account_id` alone: that is the workspace, which two people
// in one ChatGPT Team share.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::{CodexAccount, Config};

/// Codex CLI's own OAuth client, as `codex login` signs in with it.
pub const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
/// Refresh a saved account's access token when less than this is left.
const REFRESH_MARGIN_SECS: i64 = 300;

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

/// One account as the fetcher and the panel see it.
#[derive(Clone)]
pub struct AccountIdentity {
    /// Stable id, the same whether the account is Codex's or saved.
    pub key: String,
    /// True for the account ~/.codex holds (Codex's own).
    pub active: bool,
    /// Custom label from config, or "" when unset.
    pub label: String,
    pub email: String,
    pub plan: String,
    pub account_id: String,
    pub access_token: String,
    /// Access-token expiry as a Unix timestamp, when the JWT carries one.
    pub exp: Option<i64>,
    /// Non-None when the account cannot be used (unreadable, signed out…).
    pub problem: Option<String>,
}

impl AccountIdentity {
    /// Display name: custom label, else the login email, else the key.
    pub fn display_name(&self) -> String {
        if !self.label.is_empty() {
            self.label.clone()
        } else if !self.email.is_empty() {
            self.email.clone()
        } else {
            self.key.clone()
        }
    }

    /// Where the account lives, for tooltips and debug output.
    pub fn place(&self) -> &'static str {
        if self.active {
            "in Codex"
        } else {
            "saved"
        }
    }
}

/// What an auth.json blob says about its login.
#[derive(Clone, Debug, PartialEq)]
pub struct Parsed {
    pub key: String,
    pub email: String,
    pub plan: String,
    pub account_id: String,
    pub access_token: String,
    pub exp: Option<i64>,
}

/// Read a Codex auth.json blob: identity from the access token's claims.
pub fn parse_auth(auth: &serde_json::Value) -> Result<Parsed, String> {
    let tokens = auth.get("tokens").ok_or("no ChatGPT login (no tokens)")?;
    let access_token = str_field(tokens, "access_token");
    if access_token.is_empty() {
        return Err("no ChatGPT login (empty access token)".to_string());
    }
    let claims = jwt_claims(&access_token).ok_or("access token is not a readable JWT")?;
    let ns = "https://api.openai.com/auth";
    let mut account_id = str_field(tokens, "account_id");
    if account_id.is_empty() {
        account_id = claim_string(&claims, ns, "chatgpt_account_id").unwrap_or_default();
    }
    let email = claim_string(&claims, "https://api.openai.com/profile", "email")
        .or_else(|| claims.get("email").and_then(|v| v.as_str()).map(String::from))
        .unwrap_or_default();
    let user = claim_string(&claims, ns, "chatgpt_user_id")
        .or_else(|| claim_string(&claims, ns, "user_id"))
        .unwrap_or_else(|| email.clone());
    let key_id = claim_string(&claims, ns, "chatgpt_account_user_id")
        .unwrap_or_else(|| format!("{}__{}", user, account_id));
    if key_id.trim_matches('_').is_empty() {
        return Err("login names no account".to_string());
    }
    Ok(Parsed {
        key: format!("codex:{}", key_id),
        email,
        plan: claim_string(&claims, ns, "chatgpt_plan_type").unwrap_or_default(),
        account_id,
        access_token,
        exp: claims.get("exp").and_then(|v| v.as_i64()),
    })
}

/// Human label for a `chatgpt_plan_type` claim.
pub fn plan_display(plan: &str) -> String {
    match plan {
        "" => String::new(),
        "plus" => "Plus".to_string(),
        "pro" => "Pro".to_string(),
        "prolite" => "Pro Lite".to_string(),
        "team" => "Team".to_string(),
        "business" => "Business".to_string(),
        "free" => "Free".to_string(),
        other => {
            let mut c = other.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Where things live
// ---------------------------------------------------------------------------

/// Codex's own home. The app is started from Finder, so CODEX_HOME is never
/// set for it; ~/.codex is where Codex keeps its login by default.
pub fn codex_home() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".codex")
}

fn active_auth_path() -> PathBuf {
    codex_home().join("auth.json")
}

fn store_dir() -> PathBuf {
    crate::config::data_dir().join("accounts")
}

fn store_path() -> PathBuf {
    store_dir().join("codex.json")
}

/// One saved account in ~/.config/ocg/accounts/codex.json.
#[derive(Clone, Serialize, Deserialize)]
pub struct Login {
    pub key: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub plan: String,
    #[serde(default)]
    pub account_id: String,
    /// The login exactly as Codex's auth.json holds it.
    pub auth: serde_json::Value,
    /// Set when OpenAI refused this login's refresh: it is gone, and has to be
    /// signed in again. Checked before every fetch, so a dead token is not
    /// sent again each refresh. A new sign-in replaces the whole entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lapsed: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct StoreFile {
    #[serde(default)]
    accounts: Vec<Login>,
}

/// Guards read-modify-write of the store and swaps of ~/.codex/auth.json.
/// Held only around file I/O, never across a network call.
static FILE_LOCK: Mutex<()> = Mutex::new(());

/// One lock per account, held while its refresh token is in use (refresh, or
/// a borrowed CODEX_HOME), so two holders never spend the same token.
static KEY_LOCKS: LazyLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn key_lock(key: &str) -> Arc<Mutex<()>> {
    KEY_LOCKS
        .lock()
        .unwrap()
        .entry(key.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

fn read_store_at(path: &Path) -> Vec<Login> {
    fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<StoreFile>(&b).ok())
        .map(|f| f.accounts)
        .unwrap_or_default()
}

fn write_store_at(path: &Path, accounts: &[Login]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    let body = serde_json::to_vec_pretty(&StoreFile { accounts: accounts.to_vec() })
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    write_private(path, &body)
}

pub fn read_store() -> Vec<Login> {
    read_store_at(&store_path())
}

/// Write `bytes` to `path` so a crash never leaves it half-written — a
/// rotated refresh token lost mid-write is a login lost — and so only this
/// user can read it.
pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.write_all(b"\n")?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

// ---------------------------------------------------------------------------
// Listing
// ---------------------------------------------------------------------------

fn setting<'a>(cfg: &'a Config, key: &str) -> Option<&'a CodexAccount> {
    cfg.codex.accounts.iter().find(|a| a.key == key)
}

fn identity(parsed: &Parsed, active: bool, cfg: &Config) -> AccountIdentity {
    let now = chrono::Utc::now().timestamp();
    // A saved account's token is renewed on demand, so only Codex's own can
    // be "expired" in a way the user has to act on.
    let problem = (active && parsed.exp.map(|e| e < now).unwrap_or(false))
        .then(|| "access token expired — run codex once to refresh it".to_string());
    AccountIdentity {
        key: parsed.key.clone(),
        active,
        label: setting(cfg, &parsed.key).map(|s| s.label.clone()).unwrap_or_default(),
        email: parsed.email.clone(),
        plan: parsed.plan.clone(),
        account_id: parsed.account_id.clone(),
        access_token: parsed.access_token.clone(),
        exp: parsed.exp,
        problem,
    }
}

/// Every account, Codex's own first, each with whether it is shown.
pub fn list(cfg: &Config) -> Vec<(AccountIdentity, bool)> {
    let enabled = |key: &str| setting(cfg, key).map(|s| s.enabled).unwrap_or(true);
    let mut out: Vec<(AccountIdentity, bool)> = Vec::new();
    let active = read_json(&active_auth_path()).and_then(|a| parse_auth(&a).ok());
    if let Some(p) = &active {
        out.push((identity(p, true, cfg), enabled(&p.key)));
    }
    for login in read_store() {
        if out.iter().any(|(a, _)| a.key == login.key) {
            continue;
        }
        let acct = match parse_auth(&login.auth) {
            Ok(p) => AccountIdentity { problem: login.lapsed.clone(), ..identity(&p, false, cfg) },
            Err(e) => AccountIdentity {
                key: login.key.clone(),
                active: false,
                label: setting(cfg, &login.key).map(|s| s.label.clone()).unwrap_or_default(),
                email: login.email.clone(),
                plan: login.plan.clone(),
                account_id: login.account_id.clone(),
                access_token: String::new(),
                exp: None,
                problem: Some(e),
            },
        };
        let on = enabled(&acct.key);
        out.push((acct, on));
    }
    out
}

/// Whether `key` is the account Codex itself is signed in to.
pub fn is_active(key: &str) -> bool {
    read_json(&active_auth_path())
        .and_then(|a| parse_auth(&a).ok())
        .map(|p| p.key == key)
        .unwrap_or(false)
}

/// The accounts shown in the panel.
pub fn load_enabled(cfg: &Config) -> Vec<AccountIdentity> {
    list(cfg).into_iter().filter(|(_, on)| *on).map(|(a, _)| a).collect()
}

/// True when the access token is valid for at least `margin_secs` more seconds.
pub fn token_usable(acct: &AccountIdentity, margin_secs: i64) -> bool {
    match acct.exp {
        Some(exp) => exp - chrono::Utc::now().timestamp() > margin_secs,
        None => true,
    }
}

// ---------------------------------------------------------------------------
// Tokens of saved accounts
// ---------------------------------------------------------------------------

/// A saved account's access token, renewed first when it is about to expire
/// (or when `force`d, after the server turned the current one away).
pub fn saved_token(key: &str, force: bool) -> Result<String, String> {
    let lock = key_lock(key);
    let _held = lock.lock().unwrap();
    let login = read_store()
        .into_iter()
        .find(|l| l.key == key)
        .ok_or("this account is no longer saved")?;
    let parsed = parse_auth(&login.auth)?;
    let now = chrono::Utc::now().timestamp();
    if !force && parsed.exp.map(|e| e - now > REFRESH_MARGIN_SECS).unwrap_or(true) {
        return Ok(parsed.access_token);
    }
    if let Some(why) = login.lapsed {
        return Err(why);
    }
    let mut auth = login.auth.clone();
    if let Err(e) = refresh(&mut auth) {
        if e.starts_with(SIGNED_OUT) {
            mark_lapsed(key, &e);
        }
        return Err(e);
    }
    let fresh = parse_auth(&auth)?;
    replace_saved_auth(key, auth);
    Ok(fresh.access_token)
}

const SIGNED_OUT: &str = "signed out";

fn mark_lapsed(key: &str, why: &str) {
    let _held = FILE_LOCK.lock().unwrap();
    let mut accounts = read_store();
    if let Some(l) = accounts.iter_mut().find(|l| l.key == key) {
        l.lapsed = Some(why.to_string());
        let _ = write_store_at(&store_path(), &accounts);
    }
}

fn replace_saved_auth(key: &str, auth: serde_json::Value) {
    let _held = FILE_LOCK.lock().unwrap();
    let mut accounts = read_store();
    if let Some(l) = accounts.iter_mut().find(|l| l.key == key) {
        l.auth = auth;
        let _ = write_store_at(&store_path(), &accounts);
    }
}

/// Renew an auth.json blob's tokens in place, as the Codex CLI does.
fn refresh(auth: &mut serde_json::Value) -> Result<(), String> {
    let refresh_token = auth
        .get("tokens")
        .and_then(|t| t.get("refresh_token"))
        .and_then(|r| r.as_str())
        .unwrap_or_default()
        .to_string();
    if refresh_token.is_empty() {
        return Err("no refresh token — sign in to this account again".to_string());
    }
    let body = serde_json::json!({
        "client_id": CLIENT_ID,
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
        "scope": "openid profile email",
    });
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(TOKEN_URL)
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .map_err(|e| format!("token refresh: {}", e))?;
    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    if !status.is_success() {
        return Err(if status.as_u16() == 400 || status.as_u16() == 401 {
            format!("{} — add this account again to sign back in", SIGNED_OUT)
        } else {
            format!("token refresh: HTTP {}", status.as_u16())
        });
    }
    let fresh: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("token refresh: {}", e))?;
    let tokens = auth
        .as_object_mut()
        .ok_or("auth is not an object")?
        .entry("tokens")
        .or_insert_with(|| serde_json::json!({}));
    for field in ["access_token", "id_token", "refresh_token"] {
        if let Some(v) = fresh.get(field).and_then(|v| v.as_str()).filter(|v| !v.is_empty()) {
            tokens[field] = serde_json::Value::String(v.to_string());
        }
    }
    auth["last_refresh"] = serde_json::Value::String(chrono::Utc::now().to_rfc3339());
    Ok(())
}

/// Run `f` with a throwaway CODEX_HOME holding a saved account, so the Codex
/// CLI can act as that account. Whatever the CLI rotates is taken back into
/// the store, and the account's lock is held throughout so nothing else
/// spends its refresh token meanwhile.
pub fn with_borrowed_home<T>(
    key: &str,
    f: impl FnOnce(&Path) -> Result<T, String>,
) -> Result<T, String> {
    let lock = key_lock(key);
    let _held = lock.lock().unwrap();
    let login = read_store()
        .into_iter()
        .find(|l| l.key == key)
        .ok_or("this account is no longer saved")?;
    let home = std::env::temp_dir().join(format!("tokue-codex-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).map_err(|e| e.to_string())?;
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    let auth_path = home.join("auth.json");
    let body = serde_json::to_vec_pretty(&login.auth).map_err(|e| e.to_string())?;
    write_private(&auth_path, &body).map_err(|e| e.to_string())?;

    let result = f(&home);

    if let Some(after) = read_json(&auth_path) {
        if after != login.auth && parse_auth(&after).map(|p| p.key == key).unwrap_or(false) {
            replace_saved_auth(key, after);
        }
    }
    let _ = fs::remove_dir_all(&home);
    result
}

// ---------------------------------------------------------------------------
// Adding, switching, removing
// ---------------------------------------------------------------------------

/// Keep a freshly signed-in login. It becomes Codex's own when Codex has none
/// yet, or already had this account (the new tokens replace the old, so one
/// refresh token stays in one place); otherwise it is saved beside the rest.
/// Returns the email and whether Codex now uses it.
pub fn add_login(auth: serde_json::Value) -> Result<(String, bool), String> {
    let parsed = parse_auth(&auth)?;
    let _held = FILE_LOCK.lock().unwrap();
    let active = read_json(&active_auth_path()).and_then(|a| parse_auth(&a).ok());
    let into_codex = active.as_ref().map(|a| a.key == parsed.key).unwrap_or(true);
    let body = serde_json::to_vec_pretty(&auth).map_err(|e| e.to_string())?;
    let mut accounts = read_store();
    if into_codex {
        fs::create_dir_all(codex_home()).map_err(|e| e.to_string())?;
        write_private(&active_auth_path(), &body).map_err(|e| e.to_string())?;
        accounts.retain(|l| l.key != parsed.key);
    } else {
        upsert(&mut accounts, &parsed, auth);
    }
    write_store_at(&store_path(), &accounts).map_err(|e| e.to_string())?;
    Ok((parsed.email, into_codex))
}

fn upsert(accounts: &mut Vec<Login>, parsed: &Parsed, auth: serde_json::Value) {
    let login = Login {
        key: parsed.key.clone(),
        email: parsed.email.clone(),
        plan: parsed.plan.clone(),
        account_id: parsed.account_id.clone(),
        auth,
        lapsed: None,
    };
    match accounts.iter_mut().find(|l| l.key == parsed.key) {
        Some(existing) => *existing = login,
        None => accounts.push(login),
    }
}

/// Sign Codex in to a saved account. Codex's current account is saved in its
/// place. Codex sessions already running keep the account they started with
/// until they are restarted.
pub fn use_in_codex(key: &str) -> Result<(), String> {
    let lock = key_lock(key);
    let _key_held = lock.lock().unwrap();
    let _held = FILE_LOCK.lock().unwrap();
    let mut accounts = read_store();
    let target = accounts
        .iter()
        .find(|l| l.key == key)
        .cloned()
        .ok_or("this account is no longer saved")?;
    if let Some(why) = &target.lapsed {
        return Err(format!("{} — it would sign Codex out too", why));
    }
    let previous = read_json(&active_auth_path());
    let body = serde_json::to_vec_pretty(&target.auth).map_err(|e| e.to_string())?;
    fs::create_dir_all(codex_home()).map_err(|e| e.to_string())?;
    write_private(&active_auth_path(), &body).map_err(|e| e.to_string())?;
    accounts.retain(|l| l.key != key);
    if let Some(prev) = previous {
        if let Ok(p) = parse_auth(&prev) {
            if p.key != key {
                upsert(&mut accounts, &p, prev);
            }
        }
    }
    write_store_at(&store_path(), &accounts).map_err(|e| e.to_string())
}

/// Forget a saved account. Codex's own is signed out with `codex logout`,
/// not here.
pub fn remove(key: &str) -> Result<(), String> {
    let lock = key_lock(key);
    let _key_held = lock.lock().unwrap();
    let _held = FILE_LOCK.lock().unwrap();
    let mut accounts = read_store();
    let before = accounts.len();
    accounts.retain(|l| l.key != key);
    if accounts.len() == before {
        return Err("only saved accounts can be removed".to_string());
    }
    write_store_at(&store_path(), &accounts).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// One-time move from ~/.codexN homes
// ---------------------------------------------------------------------------

/// What migrating a set of homes decides, worked out before anything is
/// written so it can be tested without a filesystem.
#[derive(Debug, Default, PartialEq)]
pub struct MigrationPlan {
    /// Logins to save, in order (key, parsed-from home).
    pub import: Vec<(String, String)>,
    /// Every home that was read: renamed aside afterwards.
    pub retire: Vec<String>,
    /// Old home path -> account key, for settings, the pin and history.
    pub rekey: Vec<(String, String)>,
}

/// Decide the migration. `active` is Codex's own login; `homes` are the
/// other ~/.codexN directories with the key each holds (None: unreadable,
/// left alone).
pub fn plan_migration(active: Option<&(String, String)>, homes: &[(String, Option<String>)]) -> MigrationPlan {
    let mut plan = MigrationPlan::default();
    if let Some((home, key)) = active {
        plan.rekey.push((home.clone(), key.clone()));
    }
    for (home, key) in homes {
        let Some(key) = key else { continue };
        plan.rekey.push((home.clone(), key.clone()));
        plan.retire.push(home.clone());
        let known = active.map(|(_, k)| k == key).unwrap_or(false)
            || plan.import.iter().any(|(k, _)| k == key);
        if !known {
            plan.import.push((key.clone(), home.clone()));
        }
    }
    plan
}

/// Move ~/.codex2… into the store, once: when the store does not exist yet.
/// Every home read is renamed to `<home>.imported` afterwards (a backup —
/// its tokens stop working once the saved copy is refreshed). Settings, the
/// menu bar pin and usage history move from path keys to account keys.
pub fn migrate_homes(cfg: &mut Config) -> bool {
    if store_path().exists() {
        return false;
    }
    let active_home = codex_home().to_string_lossy().to_string();
    let read_home = |home: &str| -> Option<serde_json::Value> {
        read_json(&Path::new(&expand_home(home)).join("auth.json"))
    };
    let active = read_home(&active_home)
        .and_then(|a| parse_auth(&a).ok())
        .map(|p| (active_home.clone(), p.key));

    let mut candidates: Vec<String> = cfg.codex.accounts.iter().map(|a| expand_home(&a.key)).collect();
    for home in discover_homes() {
        let home = home.to_string_lossy().to_string();
        if !candidates.contains(&home) {
            candidates.push(home);
        }
    }
    candidates.retain(|h| *h != active_home && h.starts_with('/'));
    let mut blobs: HashMap<String, serde_json::Value> = HashMap::new();
    let homes: Vec<(String, Option<String>)> = candidates
        .iter()
        .map(|home| {
            let key = read_home(home).and_then(|auth| {
                let key = parse_auth(&auth).ok()?.key;
                blobs.insert(home.clone(), auth);
                Some(key)
            });
            (home.clone(), key)
        })
        .collect();
    let plan = plan_migration(active.as_ref(), &homes);

    let _held = FILE_LOCK.lock().unwrap();
    let mut accounts: Vec<Login> = Vec::new();
    for (_, home) in &plan.import {
        if let Some(auth) = blobs.get(home) {
            if let Ok(p) = parse_auth(auth) {
                upsert(&mut accounts, &p, auth.clone());
            }
        }
    }
    if write_store_at(&store_path(), &accounts).is_err() {
        return false; // nothing retired: the homes still work as they did
    }

    // Settings: an account is shown if any copy of it was — a duplicate that
    // was switched off only because its twin was shown must not hide the
    // account now that the copies are one. The first label wins.
    let old = std::mem::take(&mut cfg.codex.accounts);
    let mut settings: Vec<CodexAccount> = Vec::new();
    for (home, key) in &plan.rekey {
        let prior = old.iter().find(|a| expand_home(&a.key) == *home);
        let (label, on) = prior.map(|p| (p.label.clone(), p.enabled)).unwrap_or((String::new(), true));
        match settings.iter_mut().find(|s| s.key == *key) {
            Some(s) => {
                s.enabled |= on;
                if s.label.is_empty() {
                    s.label = label;
                }
            }
            None => settings.push(CodexAccount { key: key.clone(), label, enabled: on }),
        }
    }
    cfg.codex.accounts = settings;
    if let Some(pin) = cfg.selected_keys.get("codex").cloned() {
        if let Some((_, key)) = plan.rekey.iter().find(|(h, _)| *h == pin) {
            cfg.selected_keys.insert("codex".to_string(), key.clone());
        }
    }
    for (home, key) in &plan.rekey {
        crate::store::rekey_codex(home, key);
    }
    for home in &plan.retire {
        let mut to = PathBuf::from(format!("{}.imported", home));
        if to.exists() {
            to = PathBuf::from(format!("{}.imported-{}", home, chrono::Utc::now().timestamp()));
        }
        let _ = fs::rename(home, to);
    }
    true
}

/// `~/.codex2`, `~/.codex3` … holding a ChatGPT login.
fn discover_homes() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else { return Vec::new() };
    let Ok(entries) = fs::read_dir(&home) else { return Vec::new() };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            p.is_dir()
                && name.starts_with(".codex")
                && name[".codex".len()..].chars().all(|c| c.is_ascii_digit())
                && p.join("auth.json").is_file()
        })
        .collect();
    out.sort();
    out
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn str_field(v: &serde_json::Value, key: &str) -> String {
    v.get(key).and_then(|x| x.as_str()).unwrap_or_default().to_string()
}

/// Read `claims[namespace][leaf]`, falling back to the flat
/// `"<namespace>.<leaf>"` form some tokens use.
fn claim_string(claims: &serde_json::Value, namespace: &str, leaf: &str) -> Option<String> {
    if let Some(v) = claims.get(namespace).and_then(|ns| ns.get(leaf)).and_then(|v| v.as_str()) {
        return Some(v.to_string());
    }
    let flat = format!("{}.{}", namespace, leaf);
    claims.get(&flat).and_then(|v| v.as_str()).map(String::from)
}

/// Expand a leading `~` to the home directory.
pub fn expand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().to_string();
        }
    }
    path.to_string()
}

/// Decode the payload segment of a JWT. Signatures are not verified — we only
/// read claims from credentials we already hold.
pub fn jwt_claims(token: &str) -> Option<serde_json::Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = b64url_decode(payload)?;
    serde_json::from_slice(&bytes).ok()
}

/// Minimal base64url decoder (no padding required).
fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            b'=' => continue,
            _ => return None,
        } as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}

/// base64url without padding, as PKCE and JWTs use it.
pub fn b64url_encode(bytes: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut s = String::with_capacity(bytes.len() * 4 / 3 + 2);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        s.push(T[((n >> 18) & 63) as usize] as char);
        s.push(T[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            s.push(T[((n >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            s.push(T[(n & 63) as usize] as char);
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    pub fn fake_jwt(payload: serde_json::Value) -> String {
        let enc = |v: &serde_json::Value| b64url_encode(&serde_json::to_vec(v).unwrap());
        format!("{}.{}.sig", enc(&json!({"alg": "RS256"})), enc(&payload))
    }

    fn auth_for(email: &str, user: &str, workspace: &str, plan: &str) -> serde_json::Value {
        let jwt = fake_jwt(json!({
            "exp": 4_000_000_000i64,
            "https://api.openai.com/auth": {
                "chatgpt_plan_type": plan,
                "chatgpt_user_id": user,
                "chatgpt_account_id": workspace,
                "chatgpt_account_user_id": format!("{}__{}", user, workspace),
            },
            "https://api.openai.com/profile": {"email": email},
        }));
        json!({"auth_mode": "chatgpt", "tokens": {"access_token": jwt, "refresh_token": "r", "account_id": workspace}})
    }

    #[test]
    fn base64url_round_trips() {
        for raw in [&b"hello"[..], b"a", b"ab", b"abc", b"\xff\xfe\xfd\xfc"] {
            assert_eq!(b64url_decode(&b64url_encode(raw)).unwrap(), raw);
        }
        assert_eq!(b64url_encode(b"hello"), "aGVsbG8");
    }

    #[test]
    fn two_people_in_one_team_workspace_are_two_accounts() {
        // stanty.eth and stanty.btc share a ChatGPT Team: same account_id,
        // different users. Keying by account_id alone merged them.
        let eth = parse_auth(&auth_for("eth@x", "user-eth", "ws-team", "team")).unwrap();
        let btc = parse_auth(&auth_for("btc@x", "user-btc", "ws-team", "team")).unwrap();
        assert_eq!(eth.account_id, btc.account_id);
        assert_ne!(eth.key, btc.key);
        assert_eq!(eth.email, "eth@x");
        assert_eq!(eth.plan, "team");
    }

    #[test]
    fn one_person_in_two_workspaces_is_two_accounts() {
        let plus = parse_auth(&auth_for("btc@x", "user-btc", "ws-personal", "prolite")).unwrap();
        let team = parse_auth(&auth_for("btc@x", "user-btc", "ws-team", "team")).unwrap();
        assert_ne!(plus.key, team.key);
    }

    #[test]
    fn a_blob_without_a_chatgpt_login_is_refused() {
        assert!(parse_auth(&json!({"auth_mode": "apikey", "OPENAI_API_KEY": "sk-x"})).is_err());
        assert!(parse_auth(&json!({"tokens": {"access_token": "not-a-jwt"}})).is_err());
    }

    #[test]
    fn migration_imports_each_account_once_and_never_the_codex_one() {
        // The real layout: ~/.codex and ~/.codex5 are the same login.
        let active = ("/h/.codex".to_string(), "codex:ibg".to_string());
        let homes = vec![
            ("/h/.codex2".to_string(), Some("codex:eth-team".to_string())),
            ("/h/.codex3".to_string(), Some("codex:btc-pro".to_string())),
            ("/h/.codex4".to_string(), Some("codex:btc-team".to_string())),
            ("/h/.codex5".to_string(), Some("codex:ibg".to_string())),
            ("/h/.codex6".to_string(), None), // unreadable: left exactly as it is
        ];
        let plan = plan_migration(Some(&active), &homes);
        let imported: Vec<&str> = plan.import.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(imported, vec!["codex:eth-team", "codex:btc-pro", "codex:btc-team"]);
        assert!(plan.retire.contains(&"/h/.codex5".to_string()), "a duplicate is retired too");
        assert!(!plan.retire.contains(&"/h/.codex6".to_string()));
        assert!(!plan.retire.contains(&"/h/.codex".to_string()), "Codex's own home is never touched");
        assert!(plan.rekey.contains(&("/h/.codex5".to_string(), "codex:ibg".to_string())));
    }

    /// Switching rewrites the real ~/.codex/auth.json, so it is exercised
    /// against a throwaway HOME. The only test that touches HOME.
    #[test]
    fn switching_swaps_the_two_logins_and_never_holds_one_twice() {
        let home = std::env::temp_dir().join(format!("tokue-switch-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(home.join(".codex")).unwrap();
        std::env::set_var("HOME", &home);

        let a = auth_for("a@x", "user-a", "ws-a", "plus");
        let b = auth_for("b@x", "user-b", "ws-b", "team");
        let (ka, kb) = (parse_auth(&a).unwrap().key, parse_auth(&b).unwrap().key);
        write_private(&home.join(".codex").join("auth.json"), &serde_json::to_vec(&a).unwrap()).unwrap();
        let mut saved = Vec::new();
        upsert(&mut saved, &parse_auth(&b).unwrap(), b.clone());
        write_store_at(&store_path(), &saved).unwrap();

        let holders = || -> (String, Vec<String>) {
            let active = parse_auth(&read_json(&active_auth_path()).unwrap()).unwrap().key;
            (active, read_store().into_iter().map(|l| l.key).collect())
        };
        assert_eq!(holders(), (ka.clone(), vec![kb.clone()]));

        use_in_codex(&kb).unwrap();
        assert_eq!(holders(), (kb.clone(), vec![ka.clone()]), "B is Codex's, A took its place in the store");
        assert!(is_active(&kb) && !is_active(&ka));
        // The login moved intact, refresh token and all.
        assert_eq!(read_json(&active_auth_path()).unwrap(), b);
        assert_eq!(read_store()[0].auth, a);

        use_in_codex(&ka).unwrap();
        assert_eq!(holders(), (ka.clone(), vec![kb.clone()]), "and back again");

        // A dead login is never put into Codex: that would sign Codex out too.
        mark_lapsed(&kb, "signed out");
        let err = use_in_codex(&kb).unwrap_err();
        assert!(err.contains("signed out"), "{}", err);
        assert_eq!(holders().0, ka, "Codex is untouched by the refusal");

        // Neither Codex's login nor the store is ever readable by anyone else.
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&active_auth_path()), 0o600);
        assert_eq!(mode(&store_path()), 0o600);

        // Only saved accounts can be removed; Codex's own is left alone.
        assert!(remove(&ka).is_err());
        remove(&kb).unwrap();
        assert!(read_store().is_empty());
        assert!(is_active(&ka));
        fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn the_store_is_private_and_survives_a_round_trip() {
        let dir = std::env::temp_dir().join(format!("tokue-store-test-{}", std::process::id()));
        let path = dir.join("accounts").join("codex.json");
        let auth = auth_for("eth@x", "user-eth", "ws-team", "team");
        let p = parse_auth(&auth).unwrap();
        let mut accounts = Vec::new();
        upsert(&mut accounts, &p, auth.clone());
        accounts[0].lapsed = Some("signed out".into());
        upsert(&mut accounts, &p, auth.clone()); // signed in again: replaced, not added
        assert!(accounts[0].lapsed.is_none(), "a new sign-in revives a lapsed account");
        write_store_at(&path, &accounts).unwrap();
        let back = read_store_at(&path);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].key, p.key);
        assert_eq!(back[0].auth, auth);
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(path.parent().unwrap()), 0o700);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn plan_labels() {
        assert_eq!(plan_display("plus"), "Plus");
        assert_eq!(plan_display("prolite"), "Pro Lite");
        assert_eq!(plan_display(""), "");
    }
}
