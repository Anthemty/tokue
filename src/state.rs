// state.rs — in-memory cache of fetch results, shared between the background
// refresh thread and the FFI callbacks (main queue).
//
// Two locks mirror the Go version:
//   - CACHE: guards provider_cache + last_updated (RwLock — many readers)
//   - REFRESH: serialises refresh cycles so fetches don't stack (Mutex)

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{LazyLock, Mutex, RwLock};

use chrono::{DateTime, Local};

use crate::providers::ProviderFetchResult;

/// Global cache of the latest fetch result per provider.
pub static PROVIDER_CACHE: LazyLock<RwLock<HashMap<String, ProviderFetchResult>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Wall-clock time of the last successful refresh cycle.
pub static LAST_UPDATED: LazyLock<RwLock<Option<DateTime<Local>>>> =
    LazyLock::new(|| RwLock::new(None));

/// Serialises refresh cycles so concurrent fetch requests don't stack
/// (replaces Go's refreshMu).
pub static REFRESH_MU: Mutex<()> = Mutex::new(());

/// Consecutive cycles in which every provider failed — drives backoff.
static FAILED_CYCLES: AtomicU32 = AtomicU32::new(0);

/// Record how a cycle went (all providers failed ⇒ back off).
pub fn record_cycle(failed: usize, total: usize) {
    if total > 0 && failed == total {
        FAILED_CYCLES.fetch_add(1, Ordering::Relaxed);
    } else {
        FAILED_CYCLES.store(0, Ordering::Relaxed);
    }
}

/// Number of consecutive fully-failed cycles.
pub fn consecutive_failed_cycles() -> u32 {
    FAILED_CYCLES.load(Ordering::Relaxed)
}

/// Format last_updated as "HH:MM:SS" (local), or "" if never.
/// Matches Go's lastUpdated.Format("15:04:05").
pub fn updated_at_string() -> String {
    let lu = LAST_UPDATED.read().unwrap();
    match *lu {
        None => String::new(),
        Some(dt) => dt.format("%H:%M:%S").to_string(),
    }
}
