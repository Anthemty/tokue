// providers.rs — provider identifiers, labels, and the shared result types.

use serde::Serialize;

/// Provider identifiers (wire format, matching the Go constants).
pub const OPENCODE: &str = "opencode";
pub const DEEPSEEK: &str = "deepseek";
pub const MINIMAX: &str = "minimax";

/// All known providers, in sidebar order.
pub const PROVIDERS: &[&str] = &[OPENCODE, DEEPSEEK, MINIMAX];

/// Display label for a provider id.
pub fn label(id: &str) -> &'static str {
    match id {
        OPENCODE => "OpenCode Go",
        DEEPSEEK => "DeepSeek",
        MINIMAX => "MiniMax",
        _ => "Unknown",
    }
}

/// A single usage row pushed to the UI progress bar renderer.
#[derive(Clone, Serialize)]
pub struct UsageMeter {
    pub label: String,
    pub percent: i32,
    pub detail: String,
}

/// The outcome of fetching one provider. `Err` is `Some` on failure.
pub struct ProviderFetchResult {
    /// 0-100, drives the menu bar icon.
    pub criticality: i32,
    pub err: Option<String>,
    /// Structured rows for progress rendering (sent to UI).
    pub meters: Vec<UsageMeter>,
}

impl ProviderFetchResult {
    pub fn ok(criticality: i32, meters: Vec<UsageMeter>) -> Self {
        Self { criticality, err: None, meters }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self { criticality: 0, err: Some(msg.into()), meters: Vec::new() }
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
