// providers.rs — provider identifiers, labels, and the shared result types.

use serde::Serialize;

/// Provider identifiers (wire format, matching the Go constants).
pub const OPENCODE: &str = "opencode";
pub const DEEPSEEK: &str = "deepseek";
pub const MINIMAX: &str = "minimax";
pub const CODEX: &str = "codex";

/// All known providers, in sidebar order.
pub const PROVIDERS: &[&str] = &[OPENCODE, DEEPSEEK, MINIMAX, CODEX];

/// Display label for a provider id.
pub fn label(id: &str) -> &'static str {
    match id {
        OPENCODE => "OpenCode Go",
        DEEPSEEK => "DeepSeek",
        MINIMAX => "MiniMax",
        CODEX => "Codex",
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
}

impl UsageMeter {
    /// Meter without a group header (single-account providers).
    pub fn new(label: impl Into<String>, percent: i32, detail: impl Into<String>) -> Self {
        Self { label: label.into(), percent, detail: detail.into(), group: None }
    }

    /// Meter under a group header.
    pub fn grouped(
        group: impl Into<String>,
        label: impl Into<String>,
        percent: i32,
        detail: impl Into<String>,
    ) -> Self {
        Self { label: label.into(), percent, detail: detail.into(), group: Some(group.into()) }
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
