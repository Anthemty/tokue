// codex_accounts.rs — discover Codex account homes and read their ChatGPT
// OAuth credentials, without ever writing to them.
//
// A "home" is a CODEX_HOME directory (~/.codex, ~/.codex2, …) containing an
// auth.json written by `codex login`. Each home holds one independent OAuth
// session: the JWT claims give us the account email, plan and expiry offline,
// so the panel can render account labels before the first network call.

use std::fs;
use std::path::{Path, PathBuf};

use crate::config::{CodexAccount, Config};

/// One usable (or unusable) login found in a Codex home.
#[derive(Clone)]
pub struct AccountIdentity {
    /// Absolute path of the home directory.
    pub home: String,
    /// `~`-abbreviated path, for display.
    pub home_display: String,
    /// Custom label from config, or "" when unset.
    pub label: String,
    pub email: String,
    pub plan: String,
    pub account_id: String,
    pub access_token: String,
    /// Access-token expiry as a Unix timestamp, when the JWT carries one.
    pub exp: Option<i64>,
    /// Set when this home holds the same ChatGPT account as an earlier home.
    pub duplicate_of: Option<String>,
    /// Non-None when the home cannot be used (not logged in, token expired…).
    pub problem: Option<String>,
}

impl AccountIdentity {
    /// Display name: custom label, else the login email, else the home path.
    pub fn display_name(&self) -> String {
        if !self.label.is_empty() {
            self.label.clone()
        } else if !self.email.is_empty() {
            self.email.clone()
        } else {
            self.home_display.clone()
        }
    }
}

/// Human label for a `chatgpt_plan_type` claim.
pub fn plan_display(plan: &str) -> String {
    match plan {
        "" => String::new(),
        "plus" => "Plus".to_string(),
        "pro" => "Pro".to_string(),
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

/// `~/.codex`, `~/.codex2`, `~/.codex3` … in that order.
fn home_sort_key(path: &Path) -> (u32, String) {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let suffix = name.strip_prefix(".codex").unwrap_or("");
    let index = if suffix.is_empty() {
        1
    } else {
        suffix.parse::<u32>().unwrap_or(9999)
    };
    (index, name)
}

/// All `~/.codex*` directories holding a ChatGPT login, in stable order.
pub fn discover_homes() -> Vec<PathBuf> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return Vec::new(),
    };
    let entries = match fs::read_dir(&home) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .map(|n| n.to_string_lossy().starts_with(".codex"))
                    .unwrap_or(false)
                && p.join("auth.json").is_file()
        })
        .collect();
    out.sort_by_key(|p| home_sort_key(p));
    out
}

/// The accounts to display: the configured list, or every discovered home when
/// config has none (auto-discovery mode).
pub fn effective_accounts(cfg: &Config) -> Vec<CodexAccount> {
    if !cfg.codex.accounts.is_empty() {
        return cfg.codex.accounts.clone();
    }
    discover_homes()
        .into_iter()
        .map(|p| CodexAccount {
            home: p.to_string_lossy().to_string(),
            label: String::new(),
            enabled: true,
        })
        .collect()
}

/// Load and parse every enabled account, flagging duplicate logins.
pub fn load_enabled(cfg: &Config) -> Vec<AccountIdentity> {
    let specs: Vec<CodexAccount> = effective_accounts(cfg)
        .into_iter()
        .filter(|a| a.enabled)
        .collect();

    let mut out: Vec<AccountIdentity> = Vec::with_capacity(specs.len());
    for spec in specs {
        out.push(load(&spec.home, &spec.label));
    }

    // Flag homes holding the same ChatGPT account as an earlier home.
    let mut seen: Vec<(String, String)> = Vec::new(); // (account_id, home_display)
    for acct in out.iter_mut() {
        if acct.account_id.is_empty() {
            continue;
        }
        if let Some((_, first_home)) = seen.iter().find(|(id, _)| *id == acct.account_id) {
            acct.duplicate_of = Some(first_home.clone());
        } else {
            seen.push((acct.account_id.clone(), acct.home_display.clone()));
        }
    }
    out
}

/// Read one home's auth.json and derive identity from the access-token JWT.
/// Never writes anything.
pub fn load(home: &str, label: &str) -> AccountIdentity {
    let expanded = expand_home(home);
    let display = display_path(&expanded);
    let mut acct = AccountIdentity {
        home: expanded.clone(),
        home_display: display,
        label: label.to_string(),
        email: String::new(),
        plan: String::new(),
        account_id: String::new(),
        access_token: String::new(),
        exp: None,
        duplicate_of: None,
        problem: None,
    };

    let path = Path::new(&expanded).join("auth.json");
    let data = match fs::read(&path) {
        Ok(d) => d,
        Err(e) => {
            acct.problem = Some(format!("cannot read auth.json: {}", e));
            return acct;
        }
    };
    let json: serde_json::Value = match serde_json::from_slice(&data) {
        Ok(v) => v,
        Err(e) => {
            acct.problem = Some(format!("auth.json is not valid JSON: {}", e));
            return acct;
        }
    };

    // Two on-disk shapes: Codex CLI (`tokens`) and the pi/feynman agents
    // (`openai-codex`).
    if let Some(tokens) = json.get("tokens") {
        acct.account_id = str_field(tokens, "account_id");
        acct.access_token = str_field(tokens, "access_token");
    } else if let Some(oc) = json.get("openai-codex") {
        acct.account_id = str_field(oc, "accountId");
        acct.access_token = str_field(oc, "access");
    }

    if acct.access_token.is_empty() {
        acct.problem = Some("no ChatGPT login in this home".to_string());
        return acct;
    }

    if let Some(claims) = jwt_claims(&acct.access_token) {
        acct.email = claim_string(
            &claims,
            "https://api.openai.com/profile",
            "email",
        )
        .or_else(|| claims.get("email").and_then(|v| v.as_str()).map(String::from))
        .unwrap_or_default();
        acct.plan = claim_string(&claims, "https://api.openai.com/auth", "chatgpt_plan_type")
            .unwrap_or_default();
        acct.exp = claims.get("exp").and_then(|v| v.as_i64());
    }

    if acct.exp.map(|exp| exp < now_unix()).unwrap_or(false) {
        acct.problem = Some("access token expired — run codex in this home to refresh".to_string());
    }
    acct
}

/// True when the access token is valid for at least `margin_secs` more seconds.
pub fn token_usable(acct: &AccountIdentity, margin_secs: i64) -> bool {
    match acct.exp {
        Some(exp) => exp - now_unix() > margin_secs,
        None => true,
    }
}

fn now_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

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
    if path == "~" {
        return dirs::home_dir()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string());
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().to_string();
        }
    }
    path.to_string()
}

/// Collapse the home prefix back to `~` for display.
pub fn display_path(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let h = home.to_string_lossy().to_string();
        if let Some(rest) = path.strip_prefix(&h) {
            return format!("~{}", rest);
        }
    }
    path.to_string()
}

/// Decode the payload segment of a JWT. Signatures are not verified — we only
/// read claims we already trust because the file is our own credential store.
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fake_jwt(payload: serde_json::Value) -> String {
        let enc = |v: &serde_json::Value| {
            let raw = serde_json::to_vec(v).unwrap();
            let mut s = String::new();
            const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
            for chunk in raw.chunks(3) {
                let b = [
                    chunk[0],
                    *chunk.get(1).unwrap_or(&0),
                    *chunk.get(2).unwrap_or(&0),
                ];
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
        };
        format!("{}.{}.sig", enc(&json!({"alg": "RS256"})), enc(&payload))
    }

    #[test]
    fn decodes_base64url_with_padding_and_plain() {
        assert_eq!(b64url_decode("aGVsbG8").unwrap(), b"hello");
        assert_eq!(b64url_decode("aGVsbG8=").unwrap(), b"hello");
        assert!(b64url_decode("!!").is_none());
    }

    #[test]
    fn reads_nested_claims() {
        let jwt = fake_jwt(json!({
            "exp": 1_800_000_000i64,
            "https://api.openai.com/auth": {"chatgpt_plan_type": "plus"},
            "https://api.openai.com/profile": {"email": "user@example.com"},
        }));
        let claims = jwt_claims(&jwt).unwrap();
        assert_eq!(claims["exp"].as_i64(), Some(1_800_000_000));
        assert_eq!(
            claim_string(&claims, "https://api.openai.com/auth", "chatgpt_plan_type").as_deref(),
            Some("plus")
        );
        assert_eq!(
            claim_string(&claims, "https://api.openai.com/profile", "email").as_deref(),
            Some("user@example.com")
        );
    }

    #[test]
    fn reads_flat_namespaced_claims() {
        let jwt = fake_jwt(json!({
            "https://api.openai.com/auth.chatgpt_plan_type": "team",
            "https://api.openai.com/profile.email": "flat@example.com"
        }));
        let claims = jwt_claims(&jwt).unwrap();
        assert_eq!(
            claim_string(&claims, "https://api.openai.com/auth", "chatgpt_plan_type").as_deref(),
            Some("team")
        );
        assert_eq!(
            claim_string(&claims, "https://api.openai.com/profile", "email").as_deref(),
            Some("flat@example.com")
        );
    }

    #[test]
    fn missing_token_reports_problem() {
        let dir = std::env::temp_dir().join(format!("ocg-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("auth.json"), br#"{"auth_mode":"apikey","OPENAI_API_KEY":"sk-x"}"#).unwrap();
        let acct = load(dir.to_string_lossy().as_ref(), "");
        assert!(acct.problem.is_some());
        assert!(acct.access_token.is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn plan_labels() {
        assert_eq!(plan_display("plus"), "Plus");
        assert_eq!(plan_display("self_serve_business_usage_based"), "Self_serve_business_usage_based");
        assert_eq!(plan_display(""), "");
    }

    #[test]
    fn token_usable_respects_margin() {
        let mut acct = AccountIdentity {
            home: String::new(),
            home_display: String::new(),
            label: String::new(),
            email: String::new(),
            plan: String::new(),
            account_id: String::new(),
            access_token: String::new(),
            exp: Some(now_unix() + 600),
            duplicate_of: None,
            problem: None,
        };
        assert!(token_usable(&acct, 120));
        acct.exp = Some(now_unix() + 60);
        assert!(!token_usable(&acct, 120));
        acct.exp = None;
        assert!(token_usable(&acct, 120));
    }

    #[test]
    fn display_path_round_trips_home() {
        if let Some(home) = dirs::home_dir() {
            let p = home.join(".codex2");
            assert_eq!(display_path(&p.to_string_lossy()), "~/.codex2");
            assert_eq!(expand_home("~/.codex2"), p.to_string_lossy());
        }
    }
}
