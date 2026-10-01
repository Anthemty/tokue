// providers.rs — provider identifiers, labels, and the shared result types.

use serde::Serialize;

/// Provider identifiers (wire format, matching the Go constants).
pub const OPENCODE: &str = "opencode";
pub const DEEPSEEK: &str = "deepseek";
pub const MINIMAX: &str = "minimax";
pub const CODEX: &str = "codex";
pub const COMMANDCODE: &str = "commandcode";
pub const CLAUDE: &str = "claude";

/// All known providers, in sidebar order.
pub const PROVIDERS: &[&str] = &[OPENCODE, DEEPSEEK, MINIMAX, CODEX, COMMANDCODE, CLAUDE];

/// Display label for a provider id.
pub fn label(id: &str) -> &'static str {
    match id {
        OPENCODE => "OpenCode Go",
        DEEPSEEK => "DeepSeek",
        MINIMAX => "MiniMax",
        CODEX => "Codex",
        COMMANDCODE => "Command Code",
        CLAUDE => "Claude",
        _ => "Unknown",
    }
}

/// A single usage row pushed to the UI progress bar renderer.
#[derive(Clone, Default, Serialize)]
pub struct UsageMeter {
    pub label: String,
    pub percent: i32,
    pub detail: String,
    /// Heading this meter belongs under (multi-account providers). The UI draws
    /// a section header whenever the group changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Identity of the account this meter belongs to (a CODEX_HOME path), so the
    /// UI can hide/show one account without asking the backend.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// How close this meter is to its limit, 0-100, always measured as *used*
    /// even when `percent` shows what is left. Drives the amber/red thresholds:
    /// remaining < 30% (severity > 70) amber, remaining < 10% (severity > 90) red.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<i32>,
    /// Small badge in the card header (e.g. the plan name).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
    /// Quiet note at the right of the card header (e.g. "in Codex").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// The card offers "start the window now" (see goStartWindow).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub can_start: bool,
    /// True for rows that report a balance or count rather than a fraction of
    /// a limit (e.g. DeepSeek's granted/topped-up balances, Command Code's
    /// credit summary). `percent` is a meaningless placeholder on these, so
    /// the UI drops the "0%" suffix instead of printing it.
    #[serde(default)]
    pub informational: bool,
}

impl UsageMeter {
    /// Meter without a group header (single-account providers).
    pub fn new(label: impl Into<String>, percent: i32, detail: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            percent,
            detail: detail.into(),
            group: None,
            key: None,
            severity: Some(percent.clamp(0, 100)),
            badge: None,
            tag: None,
            can_start: false,
            informational: false,
        }
    }

    /// Meter under a group header.
    pub fn grouped(
        group: impl Into<String>,
        label: impl Into<String>,
        percent: i32,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            percent,
            detail: detail.into(),
            group: Some(group.into()),
            key: None,
            severity: Some(percent.clamp(0, 100)),
            badge: None,
            tag: None,
            can_start: false,
            informational: false,
        }
    }
}

/// The outcome of fetching one provider. `Err` is `Some` on failure.
pub struct ProviderFetchResult {
    /// 0-100, drives the menu bar icon.
    pub criticality: i32,
    pub err: Option<String>,
    /// Structured rows for progress rendering (sent to UI).
    pub meters: Vec<UsageMeter>,
    /// Optional multi-line breakdown for the status-item tooltip.
    pub summary: Option<String>,
}

impl ProviderFetchResult {
    pub fn ok(criticality: i32, meters: Vec<UsageMeter>) -> Self {
        Self { criticality, err: None, meters, summary: None }
    }

    /// Success carrying a tooltip breakdown (used by multi-account providers).
    pub fn ok_detailed(criticality: i32, meters: Vec<UsageMeter>, summary: String) -> Self {
        Self { criticality, err: None, meters, summary: Some(summary) }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self { criticality: 0, err: Some(msg.into()), meters: Vec::new(), summary: None }
    }
}

/// One window's meter under the popover-wide used/remaining setting: `percent`
/// flips to what is left, while `severity` always stays the used figure that
/// the amber/red thresholds are defined against, and the label says which way
/// it reads. Without this the two are easy to mix up — an empty bar means
/// "nothing used" in one direction and "nothing left" in the other.
pub fn window_meter(
    label: &str,
    used_percent: i32,
    detail: impl Into<String>,
    show_remaining: bool,
) -> UsageMeter {
    let used = used_percent.clamp(0, 100);
    let mut meter = UsageMeter::new(
        if show_remaining { format!("{} left", label) } else { label.to_string() },
        if show_remaining { 100 - used } else { used },
        detail,
    );
    meter.severity = Some(used);
    meter
}

/// Locate a CLI the user installed. `env_override` wins; then PATH; then
/// the places the usual installers put things — a GUI app launched from
/// Finder inherits only the system PATH, never ~/.local/bin, Homebrew or
/// mise, so a bare name would only ever resolve when run from a terminal.
pub fn resolve_cli(name: &str, env_override: &str) -> String {
    if let Ok(p) = std::env::var(env_override) {
        if !p.is_empty() {
            return p;
        }
    }
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            candidates.push(std::path::Path::new(dir).join(name));
        }
    }
    if let Some(home) = dirs::home_dir() {
        for rel in [
            ".local/bin",
            ".local/share/mise/shims",
            ".npm-global/bin",
            ".volta/bin",
            ".bun/bin",
        ] {
            candidates.push(home.join(rel).join(name));
        }
    }
    candidates.push(std::path::Path::new("/opt/homebrew/bin").join(name));
    candidates.push(std::path::Path::new("/usr/local/bin").join(name));
    candidates
        .into_iter()
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.to_string())
}

/// Run a CLI to completion with a deadline, returning the last few stderr
/// lines on failure so the card can say *why*. Used by the "start window"
/// actions, which just need one cheap request to go through the CLI's own
/// login.
pub fn run_cli(mut cmd: std::process::Command, timeout: std::time::Duration) -> Result<(), String> {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start: {}", e))?;
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    if let Some(err) = child.stderr.take() {
        std::thread::spawn(move || {
            let mut tail: Vec<String> = Vec::new();
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }
                tail.push(line);
                if tail.len() > 6 {
                    tail.remove(0);
                }
            }
            let _ = tx.send(tail.join(" · "));
        });
    }
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                let detail = rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap_or_default();
                return Err(if detail.is_empty() { format!("exited with {}", status) } else { detail });
            }
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("did not finish within {}s", timeout.as_secs()));
            }
            Err(e) => return Err(format!("waiting: {}", e)),
        }
    }
}

/// Empty directory for "start window" requests to run in, so a CLI agent
/// never sees (or touches) a real project.
pub fn scratch_dir() -> Result<std::path::PathBuf, String> {
    let dir = std::env::temp_dir().join("tokue-start-window");
    std::fs::create_dir_all(&dir).map_err(|e| format!("scratch dir: {}", e))?;
    Ok(dir)
}

/// A reset instant as a clock time — "07:37" today, "9/19 16:09" otherwise.
pub fn reset_clock(unix: i64) -> Option<String> {
    use chrono::{Local, TimeZone};
    let dt = Local.timestamp_opt(unix, 0).single()?;
    if dt.date_naive() == Local::now().date_naive() {
        Some(dt.format("%H:%M").to_string())
    } else {
        Some(dt.format("%-m/%-d %H:%M").to_string())
    }
}

/// "→ 07:37 · 4h 40m": the wording every provider uses for when a window
/// resets — clock time, then how long that is from now. Empty once passed.
pub fn reset_detail(reset_at_unix: i64) -> String {
    let secs = reset_at_unix - chrono::Utc::now().timestamp();
    if secs < 0 {
        return String::new();
    }
    let duration = if secs >= 86400 { format!("{}d", secs / 86400) } else { format_duration(secs) };
    match reset_clock(reset_at_unix) {
        Some(c) => format!("→ {} · {}", c, duration),
        None => format!("→ {}", duration),
    }
}

/// Format a duration in seconds as a compact "2d 4h" / "4h 12m" / "12m" string.
/// Seconds are dropped (matches Go formatDuration).
pub fn format_duration(sec: i64) -> String {
    let total_h = sec / 3600;
    let days = total_h / 24;
    let h = total_h % 24;
    let m = (sec % 3600) / 60;
    if days > 0 {
        format!("{}d {}h", days, h)
    } else if h > 0 {
        format!("{}h {}m", h, m)
    } else {
        format!("{}m", m)
    }
}
