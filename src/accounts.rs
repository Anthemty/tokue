// accounts.rs — logins OCG signed in to itself, per provider.
//
// ~/.config/ocg/accounts/<provider>.json (Codex keeps its own shape in
// codex.json, see codex_accounts.rs). Each entry is one account: a stable key,
// a name to show, the plan when known, and the credential blob exactly as the
// provider handed it over. The same rules as Codex's store hold: the directory
// is 0700 and the file 0600, every write is atomic (a rotated refresh token
// lost mid-write is a login lost), and one lock per account guards its refresh
// token so it is never spent twice.

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use serde::{Deserialize, Serialize};

use crate::codex_accounts::write_private;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    pub key: String,
    /// Email or user name, for the card title.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub plan: String,
    /// The credential as the provider handed it over (tokens, or an API key).
    pub auth: serde_json::Value,
    /// Set when the provider refused this login's refresh: it has to be
    /// signed in again. A new sign-in replaces the whole entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lapsed: Option<String>,
}

/// One account as Preferences lists it, whatever the provider keeps.
pub struct Listed {
    pub key: String,
    pub name: String,
    pub plan: String,
    /// The CLI's own login (read, never written, not removable here).
    pub own: bool,
    /// Why it cannot be read right now, if it cannot.
    pub problem: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct StoreFile {
    #[serde(default)]
    accounts: Vec<Saved>,
}

static FILE_LOCK: Mutex<()> = Mutex::new(());
static KEY_LOCKS: LazyLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// The lock that guards one account's refresh token.
pub fn key_lock(key: &str) -> Arc<Mutex<()>> {
    KEY_LOCKS.lock().unwrap().entry(key.to_string()).or_insert_with(|| Arc::new(Mutex::new(()))).clone()
}

fn path(provider: &str) -> PathBuf {
    crate::config::data_dir()
        .join("accounts")
        .join(format!("{}.json", provider))
}

fn read_at(path: &Path) -> Vec<Saved> {
    fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<StoreFile>(&b).ok())
        .map(|f| f.accounts)
        .unwrap_or_default()
}

fn write_at(path: &Path, accounts: &[Saved]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    let body = serde_json::to_vec_pretty(&StoreFile { accounts: accounts.to_vec() })
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    write_private(path, &body)
}

/// Read-modify-write the provider's store under the file lock.
fn edit<T>(provider: &str, f: impl FnOnce(&mut Vec<Saved>) -> T) -> Result<T, String> {
    let _held = FILE_LOCK.lock().unwrap();
    let p = path(provider);
    let mut accounts = read_at(&p);
    let out = f(&mut accounts);
    write_at(&p, &accounts).map_err(|e| e.to_string())?;
    Ok(out)
}

pub fn list(provider: &str) -> Vec<Saved> {
    read_at(&path(provider))
}

pub fn get(provider: &str, key: &str) -> Option<Saved> {
    list(provider).into_iter().find(|s| s.key == key)
}

/// Keep a freshly signed-in account; signing in again to one already kept
/// replaces it (and so revives it if it had lapsed).
pub fn upsert(provider: &str, saved: Saved) -> Result<(), String> {
    edit(provider, |accounts| match accounts.iter_mut().find(|s| s.key == saved.key) {
        Some(existing) => *existing = saved,
        None => accounts.push(saved),
    })
}

/// Replace an account's credential after a refresh.
pub fn update_auth(provider: &str, key: &str, auth: serde_json::Value) -> Result<(), String> {
    edit(provider, |accounts| {
        if let Some(s) = accounts.iter_mut().find(|s| s.key == key) {
            s.auth = auth;
        }
    })
}

pub fn mark_lapsed(provider: &str, key: &str, why: &str) {
    let _ = edit(provider, |accounts| {
        if let Some(s) = accounts.iter_mut().find(|s| s.key == key) {
            s.lapsed = Some(why.to_string());
        }
    });
}

/// Forget an account. False when it was not kept here.
pub fn remove(provider: &str, key: &str) -> bool {
    edit(provider, |accounts| {
        let before = accounts.len();
        accounts.retain(|s| s.key != key);
        accounts.len() != before
    })
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_store_is_private_and_a_new_sign_in_replaces_a_lapsed_one() {
        let dir = std::env::temp_dir().join(format!("tokue-accounts-test-{}", std::process::id()));
        let p = dir.join("accounts").join("claude.json");
        let a = Saved {
            key: "claude:a".into(),
            name: "a@x".into(),
            plan: "max".into(),
            auth: serde_json::json!({"refresh": "r1"}),
            lapsed: Some("signed out".into()),
        };
        write_at(&p, &[a.clone()]).unwrap();
        let mut accounts = read_at(&p);
        let fresh = Saved { auth: serde_json::json!({"refresh": "r2"}), lapsed: None, ..a.clone() };
        if let Some(e) = accounts.iter_mut().find(|s| s.key == fresh.key) {
            *e = fresh.clone();
        }
        write_at(&p, &accounts).unwrap();
        assert_eq!(read_at(&p), vec![fresh]);
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&p), 0o600);
        assert_eq!(mode(p.parent().unwrap()), 0o700);
        fs::remove_dir_all(&dir).ok();
    }
}
