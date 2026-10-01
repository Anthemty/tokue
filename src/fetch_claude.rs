// fetch_claude.rs — Claude (claude.ai subscription) usage, for every account
// OCG can reach:
//
//   * Claude Code's own login, read but never written — from the keychain item
//     "Claude Code-credentials" or ~/.claude/.credentials.json. Its refresh
//     token is Claude Code's to rotate: racing it would sign the CLI out, so
//     an expired token is reported, not refreshed.
//   * Accounts signed in from OCG (signin.rs), kept in accounts/claude.json.
//     Those tokens are OCG's own grant, so OCG refreshes them itself.
//
// Usage is the endpoint Claude Code's own `/usage` reads:
//   GET https://api.anthropic.com/api/oauth/usage
//   Authorization: Bearer <accessToken>, anthropic-beta: oauth-2025-04-20

use std::collections::HashMap;
use std::process::Command;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use crate::accounts;
use crate::config::Config;
use crate::providers::{
    reset_detail, resolve_cli, run_cli, scratch_dir, window_meter, ProviderFetchResult,
    UsageMeter,
};
use crate::signin::{CLAUDE_CLIENT_ID, CLAUDE_TOKEN_URL};

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const PROFILE_URL: &str = "https://api.anthropic.com/api/oauth/profile";
/// How long a cached reading stays worth showing. The windows themselves run
/// 5h/7d, so a reading older than this tells the user nothing useful.
const CACHE_MAX_AGE_SECS: i64 = 6 * 3600;
const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";
/// Renew a saved account's token when less than this is left.
const REFRESH_MARGIN_MS: i64 = 5 * 60 * 1000;

/// Per account: the last usage payload that came back, and when. The
/// endpoint rate-limits hard (429 with a `retry-after`), so a refresh that
/// is turned away shows this again rather than blanking the card.
static LAST_GOOD: LazyLock<Mutex<HashMap<String, (i64, serde_json::Value)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Per account: the unix second before which the endpoint asked us not to
/// call again. Per account, so one account being limited does not silence
/// another.
static RETRY_AFTER: LazyLock<Mutex<HashMap<String, i64>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

struct Login {
    access_token: String,
    /// Unix milliseconds.
    expires_at: Option<i64>,
    subscription: String,
}

fn parse_login(json: &str) -> Option<Login> {
    let v: serde_json::Value = serde_json::from_str(json.trim()).ok()?;
    let oauth = v.get("claudeAiOauth")?;
    let access_token = oauth.get("accessToken")?.as_str()?.to_string();
    if access_token.is_empty() {
        return None;
    }
    Some(Login {
        access_token,
        expires_at: oauth.get("expiresAt").and_then(|e| e.as_i64()),
        subscription: oauth
            .get("subscriptionType")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string(),
    })
}

fn login_from_file() -> Option<Login> {
    let file = dirs::home_dir()?.join(".claude").join(".credentials.json");
    parse_login(&std::fs::read_to_string(file).ok()?)
}

/// macOS: Claude Code keeps the login in the keychain. `security` prompts the
/// user once to allow this app; "Always Allow" makes it silent after.
fn login_from_keychain() -> Option<Login> {
    let out = Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", KEYCHAIN_SERVICE, "-w"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_login(&String::from_utf8_lossy(&out.stdout))
}

/// Whichever copy of Claude Code's login is freshest. On macOS it maintains
/// the keychain item, but a `.credentials.json` from an older install can
/// linger with a long-expired token.
fn read_login() -> Option<Login> {
    [login_from_keychain(), login_from_file()]
        .into_iter()
        .flatten()
        .max_by_key(|l| l.expires_at.unwrap_or(0))
}

/// Who Claude Code is signed in as, from its settings file (not a credential):
/// (account uuid, email).
fn own_identity() -> Option<(String, String)> {
    let path = dirs::home_dir()?.join(".claude.json");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    let acct = v.get("oauthAccount")?;
    let s = |k: &str| acct.get(k).and_then(|x| x.as_str()).unwrap_or_default().trim().to_string();
    Some((s("accountUuid"), s("emailAddress")))
}

/// The account behind a token, as Claude Code asks for it after signing in.
pub fn profile(access_token: &str) -> Result<serde_json::Value, String> {
    crate::signin::get_json(PROFILE_URL, access_token, None)
}

/// Claude's organization types, named the way Claude Code names its plans.
pub fn plan_of_org_type(org_type: &str) -> String {
    match org_type {
        "claude_max" => "max",
        "claude_pro" => "pro",
        "claude_enterprise" => "enterprise",
        "claude_team" => "team",
        _ => "",
    }
    .to_string()
}

fn plan_display(subscription: &str) -> String {
    match subscription {
        "max" => "Max".to_string(),
        "pro" => "Pro".to_string(),
        "team" => "Team".to_string(),
        "enterprise" => "Enterprise".to_string(),
        "" => String::new(),
        other => {
            let mut c = other.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        }
    }
}

/// One rate-limit window as the API reports it.
struct Window {
    used_percent: i32,
    reset_at: Option<i64>,
}

fn parse_window(v: Option<&serde_json::Value>) -> Option<Window> {
    let v = v?;
    if v.is_null() {
        return None;
    }
    let used = v.get("utilization")?.as_f64()?;
    let reset_at = v
        .get("resets_at")
        .and_then(|r| r.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.timestamp());
    Some(Window { used_percent: used.round().clamp(0.0, 100.0) as i32, reset_at })
}

/// Every account, for Preferences. Claude Code's own is recognised from its
/// settings file rather than the keychain — this runs on every state push.
pub fn listed() -> Vec<crate::accounts::Listed> {
    let mut out = Vec::new();
    let own = own_identity().filter(|(u, e)| !u.is_empty() || !e.is_empty());
    let own_key = own.as_ref().map(|(u, e)| format!("claude:{}", if u.is_empty() { e } else { u }));
    if let (Some((_, email)), Some(key)) = (&own, &own_key) {
        out.push(crate::accounts::Listed { key: key.clone(), name: email.clone(), plan: String::new(), own: true, problem: None });
    }
    for s in accounts::list("claude") {
        if Some(&s.key) != own_key.as_ref() {
            out.push(crate::accounts::Listed { key: s.key, name: s.name, plan: s.plan, own: false, problem: s.lapsed });
        }
    }
    out
}

/// One account to read usage for.
struct Account {
    key: String,
    name: String,
    plan: String,
    /// Claude Code's own login (so the CLI can start its window).
    own: bool,
    problem: Option<String>,
}

pub fn fetch(cfg: &Config) -> ProviderFetchResult {
    let mut list: Vec<Account> = Vec::new();
    let own = read_login();
    let own_key = own.as_ref().map(|_| {
        let (uuid, email) = own_identity().unwrap_or_default();
        format!("claude:{}", if uuid.is_empty() { email } else { uuid })
    });
    if let (Some(login), Some(key)) = (&own, &own_key) {
        let expired = login.expires_at.map(|e| e / 1000 <= chrono::Utc::now().timestamp()).unwrap_or(false);
        list.push(Account {
            key: key.clone(),
            name: own_identity().map(|(_, e)| e).filter(|e| !e.is_empty()).unwrap_or_else(|| "Claude Code".into()),
            plan: login.subscription.clone(),
            own: true,
            problem: expired.then(|| "login expired — open Claude Code once to refresh it".to_string()),
        });
    }
    for saved in accounts::list("claude") {
        if Some(&saved.key) == own_key.as_ref() {
            continue; // the same account as Claude Code's: read that one
        }
        list.push(Account {
            key: saved.key.clone(),
            name: saved.name.clone(),
            plan: saved.plan.clone(),
            own: false,
            problem: saved.lapsed.clone(),
        });
    }
    list.retain(|a| cfg.account_shown("claude", &a.key));
    if list.is_empty() {
        return ProviderFetchResult::err("not logged in — sign in to Claude Code, or add an account in Preferences");
    }

    let mut meters: Vec<UsageMeter> = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    let mut criticality = 0;
    for acct in &list {
        let label = cfg.account_label("claude", &acct.key);
        let title = if label.is_empty() { acct.name.clone() } else { label };
        let reading = match &acct.problem {
            Some(p) => Err(p.clone()),
            None => read_usage(&acct.key, |force| token_for(acct, &own, force)),
        };
        match reading {
            Ok((v, stale)) => {
                let r = build(&v, &title, &acct.plan, cfg.show_remaining(), stale);
                if r.err.is_none() {
                    criticality = criticality.max(r.criticality);
                }
                for mut m in r.meters {
                    m.key = Some(acct.key.clone());
                    m.tag = acct.own.then(|| "in Claude Code".to_string());
                    m.can_start = acct.own;
                    meters.push(m);
                }
                if let Some(e) = r.err {
                    failures.push(e);
                }
            }
            Err(e) => {
                let mut m = UsageMeter::grouped(title, "Login", 0, e.clone());
                m.key = Some(acct.key.clone());
                m.severity = None;
                m.informational = true;
                m.tag = acct.own.then(|| "in Claude Code".to_string());
                let badge = plan_display(&acct.plan);
                m.badge = (!badge.is_empty()).then_some(badge);
                meters.push(m);
                failures.push(e);
            }
        }
    }
    if failures.len() == list.len() && meters.iter().all(|m| m.informational) {
        return ProviderFetchResult::err(failures.join("; "));
    }
    ProviderFetchResult::ok(criticality, meters)
}

/// A usable access token: Claude Code's as it is, a saved account's renewed
/// first when it is about to expire (or `force`d after a 401).
fn token_for(acct: &Account, own: &Option<Login>, force: bool) -> Result<String, String> {
    if acct.own {
        return own.as_ref().map(|l| l.access_token.clone()).ok_or_else(|| "not logged in".to_string());
    }
    saved_token(&acct.key, force)
}

fn saved_token(key: &str, force: bool) -> Result<String, String> {
    let lock = accounts::key_lock(key);
    let _held = lock.lock().unwrap();
    let saved = accounts::get("claude", key).ok_or("this account is no longer saved")?;
    if let Some(why) = saved.lapsed {
        return Err(why);
    }
    let field = |k: &str| saved.auth.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let expires = saved.auth.get("expiresAt").and_then(|v| v.as_i64()).unwrap_or(0);
    if !force && expires - chrono::Utc::now().timestamp_millis() > REFRESH_MARGIN_MS {
        return Ok(field("accessToken"));
    }
    let reply = crate::signin::post_json(
        CLAUDE_TOKEN_URL,
        &serde_json::json!({
            "grant_type": "refresh_token",
            "refresh_token": field("refreshToken"),
            "client_id": CLAUDE_CLIENT_ID,
        }),
    );
    let tok = match reply {
        Ok(t) => t,
        Err(e) if e.starts_with("HTTP 400") || e.starts_with("HTTP 401") => {
            let why = "signed out — add this account again to sign back in";
            accounts::mark_lapsed("claude", key, why);
            return Err(why.to_string());
        }
        Err(e) => return Err(format!("token refresh: {}", e)),
    };
    let access = tok.get("access_token").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    if access.is_empty() {
        return Err("token refresh: no token in the reply".to_string());
    }
    let mut auth = saved.auth.clone();
    auth["accessToken"] = access.clone().into();
    if let Some(r) = tok.get("refresh_token").and_then(|v| v.as_str()).filter(|r| !r.is_empty()) {
        auth["refreshToken"] = r.into();
    }
    let expires_in = tok.get("expires_in").and_then(|v| v.as_i64()).unwrap_or(3600);
    auth["expiresAt"] = (chrono::Utc::now().timestamp_millis() + expires_in * 1000).into();
    accounts::update_auth("claude", key, auth)?;
    Ok(access)
}

/// Read one account's usage, honouring its rate-limit gate and falling back
/// to its last good reading. Ok((payload, stale)).
fn read_usage(
    key: &str,
    mut token: impl FnMut(bool) -> Result<String, String>,
) -> Result<(serde_json::Value, bool), String> {
    let now = chrono::Utc::now().timestamp();
    let wait = RETRY_AFTER.lock().unwrap().get(key).copied().unwrap_or(0) - now;
    if wait > 0 {
        return cached_or(key, format!("rate limited — retrying in {}", brief(wait)));
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("request failed: {}", e))?;
    let mut forced = false;
    loop {
        let access = token(forced)?;
        let resp = client
            .get(USAGE_URL)
            .header("Authorization", format!("Bearer {}", access))
            .header("anthropic-beta", "oauth-2025-04-20")
            .header("Accept", "application/json")
            .send()
            .map_err(|e| format!("request failed: {}", e));
        let resp = match resp {
            Ok(r) => r,
            Err(e) => return cached_or(key, e),
        };
        let status = resp.status().as_u16();
        if status == 429 {
            // Respect the endpoint's own backoff (five minutes when it does
            // not say): otherwise every refresh spends a request to be
            // refused, which keeps the limit tripped.
            let secs = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(300)
                .clamp(1, 3600);
            RETRY_AFTER.lock().unwrap().insert(key.to_string(), now + secs);
            return cached_or(key, format!("rate limited — retrying in {}", brief(secs)));
        }
        let body = resp.text().unwrap_or_default();
        if status == 401 || status == 403 {
            if !forced {
                forced = true; // a saved token may have been revoked early: renew once
                continue;
            }
            return Err("login rejected — sign in to this account again".to_string());
        }
        if !(200..300).contains(&status) {
            return cached_or(key, format!("HTTP {}: {}", status, body.chars().take(80).collect::<String>()));
        }
        let v: serde_json::Value = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(e) => return cached_or(key, format!("parse: {}", e)),
        };
        LAST_GOOD.lock().unwrap().insert(key.to_string(), (now, v.clone()));
        return Ok((v, false));
    }
}

/// "3m" / "2h 5m" — short enough for a card's error line.
fn brief(secs: i64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else {
        crate::providers::format_duration(secs)
    }
}

/// The account's last good reading, marked stale, when a refresh could not
/// get through; the error itself only when there is nothing to show.
fn cached_or(key: &str, err: String) -> Result<(serde_json::Value, bool), String> {
    match LAST_GOOD.lock().unwrap().get(key) {
        Some((at, v)) if chrono::Utc::now().timestamp() - at < CACHE_MAX_AGE_SECS => Ok((v.clone(), true)),
        _ => Err(err),
    }
}

fn build(v: &serde_json::Value, title: &str, subscription: &str, left: bool, stale: bool) -> ProviderFetchResult {
    let title = if stale { format!("{} · cached", title) } else { title.to_string() };
    let windows: [(&str, &str); 4] = [
        ("five_hour", "5h"),
        ("seven_day", "Weekly"),
        ("seven_day_opus", "Weekly Opus"),
        ("seven_day_sonnet", "Weekly Sonnet"),
    ];
    let mut meters: Vec<UsageMeter> = Vec::new();
    let mut headline: Option<i32> = None;
    for (field, label) in windows {
        let Some(w) = parse_window(v.get(field)) else { continue };
        let detail = w.reset_at.map(reset_detail).unwrap_or_default();
        let detail = if stale { format!("{} (stale)", detail) } else { detail };
        let mut meter = window_meter(label, w.used_percent, detail, left);
        meter.group = Some(title.clone());
        let badge = plan_display(subscription);
        meter.badge = (!badge.is_empty()).then_some(badge);
        // 5h first, then weekly: the shorter window is the headline.
        headline.get_or_insert(w.used_percent);
        meters.push(meter);
    }
    if meters.is_empty() {
        return ProviderFetchResult::err("no usage windows in response");
    }
    ProviderFetchResult::ok(headline.unwrap_or(0), meters)
}

/// Start the 5h window now: one `claude -p` turn with no tools, nothing
/// persisted, hooks and plugins skipped — the cheapest request that goes
/// through Claude Code's own login. Claude's 5h window really does begin
/// with the first message, so this is the case the button was made for.
pub fn start_window() -> Result<(), String> {
    let scratch = scratch_dir()?;
    let mut cmd = Command::new(resolve_cli("claude", "TOKUE_CLAUDE_BIN"));
    cmd.args([
        "-p",
        "Reply with the single word OK and nothing else.",
        "--bare",
        "--no-session-persistence",
        "--tools",
        "",
    ])
    .current_dir(&scratch);
    run_cli(cmd, Duration::from_secs(120))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_claude_code_credentials() {
        let login = parse_login(
            r#"{"claudeAiOauth":{"accessToken":"tok","refreshToken":"r","expiresAt":1789000000000,"scopes":["user:inference"],"subscriptionType":"max"}}"#,
        )
        .unwrap();
        assert_eq!(login.access_token, "tok");
        assert_eq!(login.expires_at, Some(1789000000000));
        assert_eq!(login.subscription, "max");
        assert!(parse_login(r#"{"claudeAiOauth":{"accessToken":""}}"#).is_none());
    }

    #[test]
    fn builds_a_card_with_the_windows_present() {
        let v = serde_json::json!({
            "five_hour": {"utilization": 23.4, "resets_at": "2099-01-01T12:00:00+00:00"},
            "seven_day": {"utilization": 61.0, "resets_at": "2099-01-05T12:00:00+00:00"},
            "seven_day_opus": null
        });
        let r = build(&v, "me@example.com", "max", true, false);
        assert!(r.err.is_none());
        assert_eq!(r.criticality, 23, "5h is the headline");
        let labels: Vec<&str> = r.meters.iter().map(|m| m.label.as_str()).collect();
        assert_eq!(labels, vec!["5h left", "Weekly left"]);
        assert_eq!(r.meters[0].percent, 77);
        assert_eq!(r.meters[0].severity, Some(23));
        assert_eq!(r.meters[0].group.as_deref(), Some("me@example.com"));
        assert_eq!(r.meters[0].badge.as_deref(), Some("Max"));
        assert!(r.meters[0].detail.starts_with("→ "));
    }

    #[test]
    fn a_cached_reading_is_shown_marked_rather_than_blanked() {
        let v = serde_json::json!({"five_hour": {"utilization": 23.4, "resets_at": "2099-01-01T12:00:00+00:00"}});
        let r = build(&v, "me@example.com", "max", true, true);
        assert!(r.err.is_none());
        assert_eq!(r.meters[0].group.as_deref(), Some("me@example.com · cached"));
        assert!(r.meters[0].detail.ends_with("(stale)"));
        assert_eq!(r.meters[0].percent, 77, "the numbers are the last good ones");
    }

    /// With an account's gate closed, nothing is requested — not even a token
    /// — and its own cache answers. Another account's gate is not this one's.
    #[test]
    fn a_closed_rate_limit_gate_serves_that_accounts_cache_without_calling_out() {
        let key = "claude:test-gate";
        let payload = serde_json::json!({"five_hour": {"utilization": 40.0}});
        let now = chrono::Utc::now().timestamp();
        RETRY_AFTER.lock().unwrap().insert(key.into(), now + 600);
        let never = |_: bool| -> Result<String, String> { panic!("no token may be asked for") };

        LAST_GOOD.lock().unwrap().remove(key);
        assert!(read_usage(key, never).unwrap_err().contains("rate limited"));

        LAST_GOOD.lock().unwrap().insert(key.into(), (now, payload.clone()));
        assert_eq!(read_usage(key, never).unwrap(), (payload.clone(), true));

        LAST_GOOD.lock().unwrap().insert(key.into(), (now - CACHE_MAX_AGE_SECS - 1, payload));
        assert!(read_usage(key, never).is_err(), "a reading too old to mean anything is not shown");

        assert_eq!(RETRY_AFTER.lock().unwrap().get("claude:another").copied().unwrap_or(0), 0);
        RETRY_AFTER.lock().unwrap().remove(key);
        LAST_GOOD.lock().unwrap().remove(key);
    }

    #[test]
    fn org_types_name_plans_as_claude_code_does() {
        assert_eq!(plan_of_org_type("claude_max"), "max");
        assert_eq!(plan_of_org_type("claude_pro"), "pro");
        assert_eq!(plan_of_org_type("something_else"), "");
    }

    #[test]
    fn no_windows_is_an_error() {
        assert!(build(&serde_json::json!({}), "x", "", true, false).err.is_some());
    }
}
