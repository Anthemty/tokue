// fetch_codex.rs — ChatGPT subscription usage for one or more accounts.
//
// Takes each account's OAuth access token — Codex's own from ~/.codex (never
// refreshed here), saved ones from ocg's store (see codex_accounts.rs) — and
// asks the same private endpoint the Codex CLI itself uses:
//
//   GET https://chatgpt.com/backend-api/wham/usage
//   Authorization: Bearer <access_token>
//   ChatGPT-Account-Id: <account_id>
//
// Fallbacks, in order:
//   1. `curl` (native TLS) when reqwest's rustls handshake is refused — some
//      Cloudflare edges fingerprint the TLS stack.
//   2. `codex app-server` JSON-RPC `account/rateLimits/read` when the access
//      token is expired or the server rejects it. The CLI then performs the
//      OAuth refresh itself and persists the rotated tokens, so this app never
//      races the CLI over a single-use refresh token.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::codex_accounts::{self, AccountIdentity};
use crate::config::Config;
use crate::providers::{format_duration, reset_clock, resolve_cli, run_cli, scratch_dir, ProviderFetchResult, UsageMeter};
use crate::store;

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const UA: &str = "codex-cli";
const ORIGINATOR: &str = "codex_cli_rs";
/// Refresh via the CLI when the token has less than this left.
const TOKEN_MARGIN_SECS: i64 = 120;
/// Local midnight, the start of the "Today" window.
pub fn start_of_today() -> i64 {
    use chrono::{Local, TimeZone};
    let now = Local::now();
    now.date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|naive| Local.from_local_datetime(&naive).single())
        .map(|dt| dt.timestamp())
        .unwrap_or_else(|| now.timestamp() - 86400)
}

// ---------------------------------------------------------------------------
// Snapshot model (normalised across the HTTP and RPC shapes)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Window {
    pub used_percent: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_secs: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_after_secs: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_at: Option<i64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SpendLimit {
    pub used_percent: i32,
    #[serde(default)]
    pub limit: String,
    #[serde(default)]
    pub used: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_at: Option<i64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub plan: String,
    #[serde(default)]
    pub account_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<Window>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary: Option<Window>,
    #[serde(default)]
    pub has_credits: bool,
    #[serde(default)]
    pub unlimited: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balance: Option<String>,
    #[serde(default)]
    pub limit_reached: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reached_note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_credits: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spend: Option<SpendLimit>,
    /// Unix seconds when this snapshot was obtained.
    #[serde(default)]
    pub fetched_at: i64,
    /// How it was obtained: http | curl | rpc.
    #[serde(default)]
    pub source: String,
}

impl Snapshot {
    /// The account's one headline percentage (0-100), forced to 100 on a
    /// limit. Prefers the primary (5h) window over the secondary (weekly)
    /// one when both are present — the shorter window is the more
    /// immediately actionable number, not whichever happens to read higher.
    pub fn criticality(&self) -> i32 {
        if self.limit_reached {
            return 100;
        }
        let pct = self
            .primary
            .as_ref()
            .map(|w| w.used_percent)
            .or_else(|| self.secondary.as_ref().map(|w| w.used_percent))
            .unwrap_or(0);
        pct.clamp(0, 100)
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

fn as_i64_field(v: &serde_json::Value, key: &str) -> Option<i64> {
    match v.get(key) {
        Some(serde_json::Value::Number(n)) => n.as_i64(),
        Some(serde_json::Value::String(s)) => s.parse::<i64>().ok(),
        _ => None,
    }
}

fn as_str_field(v: &serde_json::Value, key: &str) -> Option<String> {
    match v.get(key) {
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(serde_json::Value::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

fn parse_window(v: &serde_json::Value) -> Option<Window> {
    if !v.is_object() {
        return None;
    }
    let used = as_i64_field(v, "used_percent")
        .or_else(|| as_i64_field(v, "usedPercent"))
        .unwrap_or(0);
    let secs = as_i64_field(v, "limit_window_seconds")
        .or_else(|| as_i64_field(v, "windowDurationMins").map(|m| m * 60));
    let reset_after = as_i64_field(v, "reset_after_seconds");
    let reset_at = as_i64_field(v, "reset_at").or_else(|| as_i64_field(v, "resetsAt"));
    Some(Window {
        used_percent: used.clamp(0, 100) as i32,
        window_secs: secs,
        reset_after_secs: reset_after,
        reset_at,
    })
}

/// Parse the `GET /wham/usage` payload.
fn parse_wham(v: &serde_json::Value) -> Result<Snapshot, String> {
    let rate = v.get("rate_limit").cloned().unwrap_or(serde_json::Value::Null);
    if rate.is_null() {
        return Err("no rate limit data for this account".to_string());
    }
    let credits = v.get("credits").cloned().unwrap_or(serde_json::Value::Null);
    let spend = v.get("spend_control").and_then(|s| s.get("individual_limit"));
    let reached = v.get("rate_limit_reached_type").and_then(|r| r.get("type"));

    Ok(Snapshot {
        email: as_str_field(v, "email").unwrap_or_default(),
        plan: as_str_field(v, "plan_type").unwrap_or_default(),
        account_id: as_str_field(v, "account_id").unwrap_or_default(),
        primary: rate.get("primary_window").and_then(parse_window),
        secondary: rate.get("secondary_window").and_then(parse_window),
        has_credits: credits.get("has_credits").and_then(|b| b.as_bool()).unwrap_or(false),
        unlimited: credits.get("unlimited").and_then(|b| b.as_bool()).unwrap_or(false),
        balance: as_str_field(&credits, "balance"),
        limit_reached: rate.get("limit_reached").and_then(|b| b.as_bool()).unwrap_or(false),
        reached_note: reached
            .and_then(|t| t.as_str())
            .map(reached_note)
            .or_else(|| {
                v.get("rate_limit_upsell")
                    .and_then(|u| u.get("title"))
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string())
            }),
        // How many resets the account HOLDS (`available_count`), not how many
        // it could spend this instant (`applicable_available_count`, which is
        // 0 whenever the account is not currently rate-limited and so hid the
        // row entirely for accounts that do own resets).
        reset_credits: v
            .get("rate_limit_reset_credits")
            .and_then(|r| as_i64_field(r, "available_count"))
            .or_else(|| {
                v.get("rate_limit_reset_credits")
                    .and_then(|r| as_i64_field(r, "applicable_available_count"))
            }),
        spend: spend.and_then(parse_spend),
        fetched_at: now_unix(),
        source: "http".to_string(),
    })
}

fn parse_spend(v: &serde_json::Value) -> Option<SpendLimit> {
    if !v.is_object() {
        return None;
    }
    Some(SpendLimit {
        used_percent: as_i64_field(v, "used_percent").unwrap_or(0).clamp(0, 100) as i32,
        limit: as_str_field(v, "limit").unwrap_or_default(),
        used: as_str_field(v, "used").unwrap_or_default(),
        reset_at: as_i64_field(v, "reset_at"),
    })
}

/// Parse the `account/rateLimits/read` RPC result.
fn parse_rpc(v: &serde_json::Value) -> Result<Snapshot, String> {
    let rate = v
        .get("rateLimits")
        .or_else(|| v.get("rateLimitsByLimitId").and_then(|m| m.get("codex")))
        .ok_or_else(|| "no rate limits in CLI response".to_string())?;
    let credits = rate.get("credits");
    Ok(Snapshot {
        email: String::new(),
        plan: rate
            .get("planType")
            .and_then(|p| p.as_str())
            .unwrap_or_default()
            .to_string(),
        account_id: as_str_field(v, "accountId").unwrap_or_default(),
        primary: rate.get("primary").and_then(parse_window),
        secondary: rate.get("secondary").and_then(parse_window),
        has_credits: credits
            .and_then(|c| c.get("hasCredits"))
            .and_then(|b| b.as_bool())
            .unwrap_or(false),
        unlimited: credits
            .and_then(|c| c.get("unlimited"))
            .and_then(|b| b.as_bool())
            .unwrap_or(false),
        balance: credits.and_then(|c| as_str_field(c, "balance")),
        limit_reached: rate.get("rateLimitReachedType").map(|r| !r.is_null()).unwrap_or(false),
        reached_note: rate
            .get("rateLimitReachedType")
            .and_then(|t| t.as_str())
            .map(reached_note),
        reset_credits: v
            .get("rateLimitResetCredits")
            .and_then(|r| as_i64_field(r, "availableCount")),
        spend: rate.get("individualLimit").and_then(parse_spend),
        fetched_at: now_unix(),
        source: "rpc".to_string(),
    })
}

/// Turn a backend reason code into short display text.
fn reached_note(code: &str) -> String {
    match code {
        "rate_limit_reached" => "rate limit reached".to_string(),
        "workspace_owner_credits_depleted" => "workspace out of credits".to_string(),
        "workspace_member_credits_depleted" => "out of credits".to_string(),
        "workspace_owner_usage_limit_reached" => "workspace usage limit".to_string(),
        "workspace_member_usage_limit_reached" => "usage limit reached".to_string(),
        other => other.replace('_', " "),
    }
}

// ---------------------------------------------------------------------------
// Transports
// ---------------------------------------------------------------------------

enum FetchError {
    /// Token rejected/expired — the CLI should refresh, so try the RPC path.
    Unauthorized(String),
    Other(String),
}

fn http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent(UA)
        .build()
        .map_err(|e| format!("client: {}", e))
}

fn http_fetch(
    client: &reqwest::blocking::Client,
    acct: &AccountIdentity,
) -> Result<Snapshot, FetchError> {
    let resp = client
        .get(USAGE_URL)
        .header("Authorization", format!("Bearer {}", acct.access_token))
        .header("ChatGPT-Account-Id", acct.account_id.as_str())
        .header("originator", ORIGINATOR)
        .header("Accept", "application/json")
        .send()
        .map_err(|e| FetchError::Other(format!("request failed: {}", e)))?;

    let status = resp.status();
    let body = resp
        .text()
        .map_err(|e| FetchError::Other(format!("read body: {}", e)))?;

    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        // A Cloudflare challenge also lands here; the caller tries curl next.
        return Err(FetchError::Unauthorized(format!(
            "HTTP {} {}",
            status.as_u16(),
            body.trim().chars().take(120).collect::<String>()
        )));
    }
    if !status.is_success() {
        return Err(FetchError::Other(format!(
            "HTTP {} {}",
            status.as_u16(),
            body.trim().chars().take(160).collect::<String>()
        )));
    }

    let json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| FetchError::Other(format!("parse: {}", e)))?;
    parse_wham(&json).map_err(FetchError::Other)
}

/// Fallback transport: the system curl (native TLS, no fingerprint surprises).
/// Headers go in via --config - on stdin so tokens never reach the process table.
fn curl_fetch(acct: &AccountIdentity) -> Result<Snapshot, String> {
    let config_text = format!(
        "header = \"Authorization: Bearer {}\"\nheader = \"ChatGPT-Account-Id: {}\"\nheader = \"User-Agent: {}\"\nheader = \"originator: {}\"\nheader = \"Accept: application/json\"\n",
        acct.access_token, acct.account_id, UA, ORIGINATOR
    );

    let mut child = Command::new("/usr/bin/curl")
        .args(["-sS", "--max-time", "20", "-w", "\n%{http_code}", "--config", "-", USAGE_URL])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("curl: {}", e))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(config_text.as_bytes())
            .map_err(|e| format!("curl stdin: {}", e))?;
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("curl wait: {}", e))?;

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let (body, code) = stdout.rsplit_once('\n').unwrap_or(("", ""));
    let code = code.trim();
    if code.is_empty() && !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("curl failed: {}", err.trim().chars().take(120).collect::<String>()));
    }
    if code == "401" || code == "403" {
        return Err(format!("HTTP {} (curl)", code));
    }
    if !code.starts_with('2') {
        return Err(format!(
            "HTTP {} {}",
            code,
            body.trim().chars().take(120).collect::<String>()
        ));
    }

    let json: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("parse: {}", e))?;
    let mut snap = parse_wham(&json)?;
    snap.source = "curl".to_string();
    Ok(snap)
}

fn codex_binary() -> String {
    resolve_cli("codex", "TOKUE_CODEX_BIN")
}

/// Start an account's 5h window on demand. The window only begins with the
/// first real request, so a user who wants it to end at a time of their own
/// choosing (lunch, end of day) can kick it off now with the cheapest request
/// there is: one `codex exec` in an empty scratch directory, read-only
/// sandbox, session not persisted. The CLI owns the OAuth tokens, so this
/// goes through exactly the same login the account already uses.
pub fn start_window(key: &str) -> Result<(), String> {
    let run_in = |home: &std::path::Path| -> Result<(), String> {
        let scratch = scratch_dir()?;
        let mut cmd = Command::new(codex_binary());
        cmd.args(["exec", "--ephemeral", "--skip-git-repo-check", "--sandbox", "read-only", "--cd"])
            .arg(&scratch)
            .arg("Reply with the single word OK and nothing else.")
            .env("CODEX_HOME", home);
        run_cli(cmd, Duration::from_secs(120))
    };
    if codex_accounts::is_active(key) {
        run_in(&codex_accounts::codex_home())
    } else {
        // A saved account is lent to the CLI in a throwaway home; whatever
        // it refreshes comes back to the store.
        codex_accounts::with_borrowed_home(key, run_in)
    }
}

/// Ask the Codex CLI for the same numbers over `codex app-server`. The CLI owns
/// the OAuth refresh, so a stale access token is refreshed on its side. Only
/// for Codex's own account: a saved one is refreshed by ocg (saved_token).
fn rpc_fetch(acct: &AccountIdentity) -> Result<Snapshot, String> {
    let mut child: Child = Command::new(codex_binary())
        .arg("app-server")
        .env("CODEX_HOME", codex_accounts::codex_home())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot start codex app-server: {}", e))?;

    let result = rpc_exchange(&mut child);
    let _ = child.kill();
    let _ = child.wait();
    let mut snap = result?;
    if snap.email.is_empty() {
        snap.email = acct.email.clone();
    }
    if snap.account_id.is_empty() {
        snap.account_id = acct.account_id.clone();
    }
    if snap.plan.is_empty() {
        snap.plan = acct.plan.clone();
    }
    Ok(snap)
}

fn rpc_exchange(child: &mut Child) -> Result<Snapshot, String> {
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;

    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    if tx.send(l).is_err() {
                        return;
                    }
                }
                Err(_) => return,
            }
        }
    });

    let deadline = std::time::Instant::now() + Duration::from_secs(45);

    // 1. initialize
    let initialize = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{{\"clientInfo\":{{\"name\":\"tokue\",\"version\":\"{}\"}}}}}}\n",
        env!("CARGO_PKG_VERSION")
    );
    stdin.write_all(initialize.as_bytes()).map_err(|e| e.to_string())?;
    stdin.flush().map_err(|e| e.to_string())?;
    wait_for_id(&rx, 1, deadline).map_err(|e| format!("initialize: {}", e))?;

    // 2. initialized notification + usage read
    stdin
        .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"initialized\"}\n")
        .map_err(|e| e.to_string())?;
    stdin
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"account/rateLimits/read\",\"params\":{}}\n")
        .map_err(|e| e.to_string())?;
    stdin.flush().map_err(|e| e.to_string())?;

    let result = wait_for_id(&rx, 2, deadline)?;
    parse_rpc(&result)
}

/// Read messages until the response with `id` arrives; returns its `result`.
fn wait_for_id(
    rx: &mpsc::Receiver<String>,
    id: i64,
    deadline: std::time::Instant,
) -> Result<serde_json::Value, String> {
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err("timed out waiting for Codex CLI".to_string());
        }
        let line = match rx.recv_timeout(remaining) {
            Ok(l) => l,
            Err(_) => return Err("timed out waiting for Codex CLI".to_string()),
        };
        let msg: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue, // non-JSON chatter on stdout
        };
        if msg.get("id").and_then(|i| i.as_i64()) != Some(id) {
            continue; // notification or another request's response
        }
        if let Some(err) = msg.get("error") {
            let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
            let message = err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error");
            if code == -32601 {
                return Err("this Codex CLI has no account/rateLimits/read".to_string());
            }
            return Err(format!("CLI request failed: {}", message));
        }
        return Ok(msg.get("result").cloned().unwrap_or(serde_json::Value::Null));
    }
}

// ---------------------------------------------------------------------------
// Per-account fetch policy
// ---------------------------------------------------------------------------

fn fetch_account(
    client: &reqwest::blocking::Client,
    acct: &AccountIdentity,
) -> Result<Snapshot, String> {
    if !acct.active {
        return fetch_saved(client, acct);
    }
    // Debug hook: force the CLI path to exercise the refresh fallback.
    if std::env::var("TOKUE_CODEX_FORCE_RPC").is_ok() {
        return rpc_fetch(acct).map_err(|e| format!("CLI usage read failed: {}", e));
    }

    // Expiring token: let the CLI refresh (and read the numbers in one go).
    if !codex_accounts::token_usable(acct, TOKEN_MARGIN_SECS) {
        return rpc_fetch(acct).map_err(|e| format!("token refresh via CLI failed: {}", e));
    }

    match http_fetch(client, acct) {
        Ok(s) => Ok(s),
        Err(FetchError::Unauthorized(e)) => rpc_fetch(acct)
            .map_err(|rpc_err| format!("{}; CLI fallback: {}", e, rpc_err)),
        Err(FetchError::Other(e)) => match curl_fetch(acct) {
            Ok(s) => Ok(s),
            Err(curl_err) => {
                if !codex_accounts::token_usable(acct, TOKEN_MARGIN_SECS) {
                    rpc_fetch(acct).map_err(|rpc_err| format!("{}; CLI fallback: {}", e, rpc_err))
                } else {
                    Err(format!("{}; curl fallback: {}", e, curl_err))
                }
            }
        },
    }
}

/// A saved account: ocg renews its token itself, and once more if the server
/// turns the current one away (revoked early, or refreshed elsewhere).
fn fetch_saved(client: &reqwest::blocking::Client, acct: &AccountIdentity) -> Result<Snapshot, String> {
    let mut acct = acct.clone();
    acct.access_token = codex_accounts::saved_token(&acct.key, false)?;
    match http_fetch(client, &acct) {
        Ok(s) => Ok(s),
        Err(FetchError::Unauthorized(e)) => {
            acct.access_token = codex_accounts::saved_token(&acct.key, true)
                .map_err(|refresh_err| format!("{}; {}", e, refresh_err))?;
            match http_fetch(client, &acct) {
                Ok(s) => Ok(s),
                Err(_) => curl_fetch(&acct),
            }
        }
        Err(FetchError::Other(e)) => {
            curl_fetch(&acct).map_err(|curl_err| format!("{}; curl fallback: {}", e, curl_err))
        }
    }
}

// ---------------------------------------------------------------------------
// Cache
// ---------------------------------------------------------------------------

// The last-good snapshot cache lives in SQLite (store::last_codex_snapshots).

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn now_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Panel percentage for a window: quota used, or quota left.
fn display_percent(used_percent: i32, show_remaining: bool) -> i32 {
    if show_remaining {
        (100 - used_percent).clamp(0, 100)
    } else {
        used_percent.clamp(0, 100)
    }
}

/// Short window name derived from its length: "5h", "Weekly", "3d".
fn window_label(window_secs: Option<i64>, fallback: &str, show_remaining: bool) -> String {
    let secs = match window_secs {
        Some(s) if s > 0 => s,
        _ => return fallback.to_string(),
    };
    let name = if secs >= 6 * 86400 {
        "Weekly".to_string()
    } else if secs % 86400 == 0 {
        format!("{}d", secs / 86400)
    } else if secs % 3600 == 0 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}m", secs / 60)
    };
    // "43% left" is unambiguous; a bare percentage could mean either.
    if show_remaining {
        format!("{} left", name)
    } else {
        name
    }
}

/// "resets in 3h 12m (14:32)" — clock time only when it is today.
fn reset_detail(w: &Window) -> String {
    let secs = w
        .reset_after_secs
        .or_else(|| w.reset_at.map(|at| at - now_unix()))
        .unwrap_or(-1);
    if secs < 0 {
        return String::new();
    }
    // Panel width is scarce: "→ 07:37 · 4h 40m" beats "resets in 4h 40m (07:37)",
    // and past a day the clock date alone says enough.
    let clock = w.reset_at.and_then(format_reset_clock);
    let duration = if secs >= 86400 {
        format!("{}d", secs / 86400)
    } else {
        format_duration(secs)
    };
    match clock {
        Some(c) => format!("→ {} · {}", c, duration),
        None => format!("→ {}", duration),
    }
}

fn format_reset_clock(unix: i64) -> Option<String> {
    reset_clock(unix)
}

/// How the panel should read the samples, from the Codex settings.
#[derive(Clone, Copy, Default)]
pub struct MeterOptions {
    pub show_spend: bool,
    pub show_remaining: bool,
    pub show_today: bool,
    pub show_reset_credits: bool,
    /// Quota burned since local midnight (percent-of-window units).
    pub today_consumed: Option<f64>,
}

fn meters_for(
    acct: &AccountIdentity,
    snap: &Snapshot,
    stale: bool,
    opts: MeterOptions,
) -> Vec<UsageMeter> {
    let show_remaining = opts.show_remaining;
    let show_spend = opts.show_spend;
    // Stamped on every row: the card's identity for pinning and actions.
    let account_key = acct.key.clone();
    let mut title = if acct.label.is_empty() && !snap.email.is_empty() {
        snap.email.clone()
    } else {
        acct.display_name()
    };
    let plan = if !snap.plan.is_empty() { snap.plan.clone() } else { acct.plan.clone() };
    if stale {
        title.push_str(" · cached");
    }

    let suffix = if stale { " (stale)" } else { "" };
    let mut meters = Vec::new();
    if let Some(primary) = &snap.primary {
        let mut meter = UsageMeter::grouped(
            title.clone(),
            window_label(primary.window_secs, "Primary", show_remaining),
            display_percent(primary.used_percent, show_remaining),
            format!("{}{}", reset_detail(primary), suffix),
        );
        meter.severity = Some(primary.used_percent.clamp(0, 100));
        meters.push(meter);
    }
    if let Some(secondary) = &snap.secondary {
        let mut meter = UsageMeter::grouped(
            title.clone(),
            window_label(secondary.window_secs, "Secondary", show_remaining),
            display_percent(secondary.used_percent, show_remaining),
            format!("{}{}", reset_detail(secondary), suffix),
        );
        meter.severity = Some(secondary.used_percent.clamp(0, 100));
        meters.push(meter);
    }
    if let Some(consumed) = opts.today_consumed.filter(|_| opts.show_today && !stale) {
        let rounded = consumed.round() as i64;
        let shown = rounded.clamp(0, 100) as i32;
        let mut meter = UsageMeter::grouped(
            title.clone(),
            "Today".to_string(),
            shown,
            format!("{}% used today", rounded.max(0)),
        );
        meter.severity = Some(shown);
        meters.push(meter);
    }
    if let Some(spend) = &snap.spend {
        if show_spend && spend.used_percent > 0 {
            meters.push(UsageMeter::grouped(
                title.clone(),
                "Spend".to_string(),
                spend.used_percent,
                format!("{} / {} credits{}", spend.used, spend.limit, suffix),
            ));
        }
    }
    if snap.has_credits || snap.unlimited {
        let detail = if snap.unlimited {
            "unlimited".to_string()
        } else {
            match &snap.balance {
                Some(b) if !b.is_empty() && b != "0" => format!("balance {}{}", b, suffix),
                _ => format!("available{}", suffix),
            }
        };
        meters.push(UsageMeter::grouped(title.clone(), "Credits".to_string(), 0, detail));
    }
    if let Some(count) = snap.reset_credits {
        if opts.show_reset_credits && count > 0 {
            meters.push(UsageMeter::grouped(
                title.clone(),
                "Reset credits".to_string(),
                0,
                format!("{} available", count),
            ));
        }
    }
    if meters.is_empty() {
        meters.push(UsageMeter::grouped(
            title,
            "Usage".to_string(),
            0,
            "no usage data returned".to_string(),
        ));
    }
    let plan_badge = if plan.is_empty() {
        None
    } else {
        Some(codex_accounts::plan_display(&plan))
    };
    let tag = acct.active.then(|| "in Codex".to_string());
    for meter in meters.iter_mut() {
        meter.key = Some(account_key.clone());
        meter.badge = plan_badge.clone();
        meter.tag = tag.clone();
        meter.can_start = true;
        // Credits / reset credits are a balance and a count, not a fraction of
        // a limit: no amber/red, and no "0%" or bar drawn from the placeholder
        // percent — the real value is in the detail ("1 available").
        if meter.label == "Credits" || meter.label == "Reset credits" {
            meter.severity = None;
            meter.informational = true;
        }
    }
    meters
}

fn problem_meters(acct: &AccountIdentity, problem: &str) -> Vec<UsageMeter> {
    let mut meter = UsageMeter::grouped(acct.display_name(), "Login", 0, problem.to_string());
    meter.key = Some(acct.key.clone());
    meter.severity = None;
    meter.informational = true;
    let plan = codex_accounts::plan_display(&acct.plan);
    meter.badge = (!plan.is_empty()).then_some(plan);
    meter.tag = acct.active.then(|| "in Codex".to_string());
    vec![meter]
}

/// One tooltip line per account: "in Codex  stanty.ibg@gmail.com  Plus  5h 84%  7d 13%".
fn summary_line(
    acct: &AccountIdentity,
    snap: Option<&Snapshot>,
    stale: bool,
    err: Option<&str>,
    show_spend: bool,
    show_remaining: bool,
) -> String {
    let name = match snap {
        Some(s) if acct.label.is_empty() && !s.email.is_empty() => s.email.clone(),
        _ => acct.display_name(),
    };
    let mut line = format!("{}  {}", acct.place(), name);
    match (snap, err) {
        (Some(s), _) => {
            let plan = if !s.plan.is_empty() { s.plan.clone() } else { acct.plan.clone() };
            if !plan.is_empty() {
                line.push_str("  ");
                line.push_str(&codex_accounts::plan_display(&plan));
            }
            if let Some(p) = &s.primary {
                line.push_str(&format!(
                    "  {} {}%",
                    window_label(p.window_secs, "5h", show_remaining),
                    display_percent(p.used_percent, show_remaining)
                ));
            }
            if let Some(sec) = &s.secondary {
                line.push_str(&format!(
                    "  {} {}%",
                    window_label(sec.window_secs, "7d", show_remaining),
                    display_percent(sec.used_percent, show_remaining)
                ));
            }
            if s.limit_reached {
                line.push_str("  (limit reached)");
            }
            if show_spend {
                if let Some(spend) = &s.spend {
                    if spend.used_percent > 0 {
                        line.push_str(&format!("  spend {}/{}", spend.used, spend.limit));
                    }
                }
            }
            if stale {
                line.push_str(" [stale]");
            }
        }
        (None, Some(e)) => {
            line.push_str("  ");
            line.push_str(e);
        }
        (None, None) => {}
    }
    line
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

struct AccountOutcome {
    acct: AccountIdentity,
    snap: Option<Snapshot>,
    err: Option<String>,
    stale: bool,
}

fn collect(cfg: &Config) -> Vec<AccountOutcome> {
    let accounts = codex_accounts::load_enabled(cfg);
    let client = http_client();
    let cache = store::last_codex_snapshots();

    // Fetch accounts in parallel, staggered so we never burst the endpoint.
    let handles: Vec<_> = accounts
        .into_iter()
        .enumerate()
        .map(|(index, acct)| {
            let client = client.clone();
            let cached = cache.get(&acct.key).cloned();
            std::thread::spawn(move || {
                if index > 0 {
                    std::thread::sleep(Duration::from_millis(250 * index as u64));
                }
                if let Some(problem) = acct.problem.clone() {
                    return AccountOutcome { acct, snap: None, err: Some(problem), stale: false };
                }
                let client = match client {
                    Ok(c) => c,
                    Err(e) => {
                        return AccountOutcome { acct, snap: None, err: Some(e), stale: false };
                    }
                };
                match fetch_account(&client, &acct) {
                    Ok(snap) => AccountOutcome { acct, snap: Some(snap), err: None, stale: false },
                    Err(e) => {
                        let fresh_cache = cached.filter(store::snapshot_is_fresh);
                        match fresh_cache {
                            Some(c) => AccountOutcome { acct, snap: Some(c), err: Some(e), stale: true },
                            None => AccountOutcome { acct, snap: None, err: Some(e), stale: false },
                        }
                    }
                }
            })
        })
        .collect();

    let mut out = Vec::with_capacity(handles.len());
    for h in handles {
        if let Ok(r) = h.join() {
            out.push(r);
        }
    }
    out
}

/// Provider entry point: fetch every enabled account and render grouped meters.
pub fn fetch(cfg: &Config) -> ProviderFetchResult {
    let outcomes = collect(cfg);
    if outcomes.is_empty() {
        return ProviderFetchResult::err("no ChatGPT account — sign in to Codex, or add one in Preferences");
    }

    let mut meters: Vec<UsageMeter> = Vec::new();
    let mut summary_lines: Vec<String> = Vec::new();
    let mut criticality = 0;
    let mut failures: Vec<String> = Vec::new();

    // "Today" comes from the SQLite history, not from the current snapshot.
    let today = if cfg.codex.show_today {
        store::today_by_account(start_of_today())
    } else {
        HashMap::new()
    };

    for outcome in &outcomes {
        let opts = MeterOptions {
            show_spend: cfg.codex.show_spend,
            show_remaining: cfg.show_remaining(),
            show_today: cfg.codex.show_today,
            show_reset_credits: cfg.codex.show_reset_credits,
            today_consumed: today.get(&outcome.acct.key).copied(),
        };
        match (&outcome.snap, &outcome.err) {
            (Some(snap), _) => {
                meters.extend(meters_for(&outcome.acct, snap, outcome.stale, opts));
                criticality = criticality.max(snap.criticality());
                summary_lines.push(summary_line(
                    &outcome.acct,
                    Some(snap),
                    outcome.stale,
                    outcome.err.as_deref(),
                    cfg.codex.show_spend,
                    cfg.show_remaining(),
                ));
                if !outcome.stale {
                    store::record_codex(&outcome.acct.key, &outcome.acct.label, snap);
                }
            }
            (None, Some(err)) => {
                meters.extend(problem_meters(&outcome.acct, err));
                summary_lines.push(summary_line(
                    &outcome.acct,
                    None,
                    false,
                    Some(err),
                    cfg.codex.show_spend,
                    cfg.show_remaining(),
                ));
                failures.push(format!("{}: {}", outcome.acct.display_name(), err));
            }
            (None, None) => {}
        }
    }

    // All accounts failed → surface it as a provider-level error.
    if failures.len() == outcomes.len() {
        return ProviderFetchResult::err(failures.join("; "));
    }

    ProviderFetchResult::ok_detailed(criticality, meters, summary_lines.join("\n"))
}

/// Machine-readable per-account report for `ocg --once codex`.
pub fn debug_json(cfg: &Config) -> String {
    let outcomes = collect(cfg);
    let accounts: Vec<serde_json::Value> = outcomes
        .iter()
        .map(|o| {
            serde_json::json!({
                "key": o.acct.key,
                "place": o.acct.place(),
                "label": o.acct.label,
                "session_email": o.acct.email,
                "plan": o.acct.plan,
                "account_id": o.acct.account_id,
                "stale": o.stale,
                "error": o.err,
                "snapshot": o.snap,
            })
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({ "accounts": accounts }))
        .unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_wham_payload() {
        let payload = json!({
            "email": "user@example.com",
            "plan_type": "plus",
            "account_id": "acc-1",
            "rate_limit": {
                "allowed": true,
                "limit_reached": false,
                "primary_window": {"used_percent": 84, "limit_window_seconds": 18000, "reset_after_seconds": 15225, "reset_at": 1789252899i64},
                "secondary_window": {"used_percent": 13, "limit_window_seconds": 604800, "reset_after_seconds": 602025, "reset_at": 1789839699i64}
            },
            "credits": {"has_credits": false, "unlimited": false, "balance": "0"},
            "spend_control": {"reached": false, "individual_limit": null},
            "rate_limit_reached_type": null,
            "rate_limit_reset_credits": {"available_count": 1, "applicable_available_count": 0}
        });
        let snap = parse_wham(&payload).unwrap();
        assert_eq!(snap.plan, "plus");
        assert_eq!(snap.primary.as_ref().unwrap().used_percent, 84);
        assert_eq!(snap.secondary.as_ref().unwrap().used_percent, 13);
        assert_eq!(snap.criticality(), 84);
        // Owns one reset, none applicable right now because the account is not
        // rate-limited — the panel reports what it holds, so the row still shows.
        assert_eq!(snap.reset_credits, Some(1));
        assert!(!snap.limit_reached);
    }

    #[test]
    fn parses_limit_reached_payload() {
        let payload = json!({
            "plan_type": "team",
            "rate_limit": {
                "limit_reached": true,
                "primary_window": {"used_percent": 100, "limit_window_seconds": 18000},
                "secondary_window": {"used_percent": 31, "limit_window_seconds": 604800}
            },
            "rate_limit_reached_type": {"type": "workspace_owner_credits_depleted"},
            "spend_control": {"reached": true, "individual_limit": {"used_percent": 100, "limit": "0", "used": "0.0", "reset_at": 1790812800i64}}
        });
        let snap = parse_wham(&payload).unwrap();
        assert!(snap.limit_reached);
        assert_eq!(snap.criticality(), 100);
        assert_eq!(snap.reached_note.as_deref(), Some("workspace out of credits"));
        assert_eq!(snap.spend.as_ref().unwrap().used_percent, 100);
    }

    #[test]
    fn missing_rate_limit_is_an_error() {
        assert!(parse_wham(&json!({"plan_type": "plus"})).is_err());
    }

    #[test]
    fn parses_rpc_payload() {
        let payload = json!({
            "accountId": "acc-1",
            "rateLimits": {
                "planType": "plus",
                "primary": {"usedPercent": 12, "windowDurationMins": 300, "resetsAt": 1789252899i64},
                "secondary": {"usedPercent": 3, "windowDurationMins": 10080, "resetsAt": 1789839699i64},
                "credits": {"hasCredits": false, "unlimited": false, "balance": null},
                "rateLimitReachedType": null
            },
            "rateLimitResetCredits": {"availableCount": 2}
        });
        let snap = parse_rpc(&payload).unwrap();
        assert_eq!(snap.primary.as_ref().unwrap().used_percent, 12);
        assert_eq!(snap.primary.as_ref().unwrap().window_secs, Some(18000));
        assert_eq!(snap.secondary.as_ref().unwrap().window_secs, Some(604800));
        assert_eq!(snap.reset_credits, Some(2));
        assert_eq!(snap.criticality(), 12);
    }

    #[test]
    fn window_labels_cover_common_spans() {
        assert_eq!(window_label(Some(18000), "Primary", false), "5h");
        assert_eq!(window_label(Some(604800), "Secondary", false), "Weekly");
        assert_eq!(window_label(Some(3 * 86400), "x", false), "3d");
        assert_eq!(window_label(None, "Primary", false), "Primary");
        assert_eq!(window_label(Some(18000), "Primary", true), "5h left");
    }

    #[test]
    fn percent_switches_between_used_and_remaining() {
        assert_eq!(display_percent(57, false), 57);
        assert_eq!(display_percent(57, true), 43);
        assert_eq!(display_percent(100, true), 0);
        assert_eq!(display_percent(0, true), 100);
        // Out-of-range backends must not produce negative bars.
        assert_eq!(display_percent(130, true), 0);
        assert_eq!(display_percent(-5, true), 100);
    }

    #[test]
    fn window_meters_carry_used_severity_in_remaining_mode() {
        let acct = AccountIdentity {
            key: "codex:a".to_string(),
            active: true,
            label: String::new(),
            email: "a@example.com".to_string(),
            plan: "plus".to_string(),
            account_id: "a".to_string(),
            access_token: String::new(),
            exp: None,
            problem: None,
        };
        let snap = Snapshot {
            email: "a@example.com".to_string(),
            primary: Some(Window { used_percent: 92, window_secs: Some(18000), ..Default::default() }),
            ..Default::default()
        };
        let meters = meters_for(
            &acct,
            &snap,
            false,
            MeterOptions { show_remaining: true, ..Default::default() },
        );
        assert_eq!(meters[0].percent, 8, "displays what is left");
        assert_eq!(meters[0].severity, Some(92), "severity stays on the used side");
        assert!(meters[0].label.ends_with("left"));
    }

    #[test]
    fn criticality_ignores_missing_windows() {
        let snap = Snapshot { primary: Some(Window { used_percent: 42, ..Default::default() }), ..Default::default() };
        assert_eq!(snap.criticality(), 42);
        assert_eq!(Snapshot::default().criticality(), 0);
    }
}
