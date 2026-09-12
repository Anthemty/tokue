// store.rs — SQLite store for usage history and the last-good snapshot cache.
//
// Two jobs, one file (~/.config/ocg/ocg.db, WAL):
//
//   samples    one row per provider/account/metric per refresh: the time series
//              that answers questions a single snapshot cannot ("how much did I
//              burn today?"). Window samples carry reset_at so consumption can
//              be summed per limit window instead of per clock hour.
//   snapshots  the newest payload per account, so a restart paints instantly and
//              a failed fetch can still show the last known numbers.
//
// All SQL lives in functions taking a &Connection; the process-wide handle is a
// Mutex<Option<Connection>> so tests can run against an in-memory database.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use rusqlite::{params, Connection, OptionalExtension};

use crate::config;
use crate::fetch_codex::Snapshot;

const SCHEMA_VERSION: i64 = 1;
/// History older than this is dropped on write.
const RETENTION_DAYS: i64 = 90;
/// Samples kept for a load-cache lookup (matches the old JSON cache rule).
const CACHE_MAX_AGE_SECS: i64 = 24 * 3600;

static DB: LazyLock<Mutex<Option<Connection>>> = LazyLock::new(|| Mutex::new(open()));

fn db_path() -> Option<PathBuf> {
    config::config_dir_path().ok().map(|d| d.join("ocg.db"))
}

fn open() -> Option<Connection> {
    let path = db_path()?;
    let conn = Connection::open(&path).ok()?;
    // WAL keeps the UI thread's reads from blocking behind the refresh thread.
    let _ = conn.pragma_update(None, "journal_mode", "WAL");
    let _ = conn.pragma_update(None, "synchronous", "NORMAL");
    migrate(&conn).ok()?;
    import_legacy_cache(&conn);
    Some(conn)
}

/// Run `f` against the shared connection; None when the store is unavailable.
/// Failures are reported once per process: silently dropping history would be
/// worse than a line on stderr.
fn with_db<T>(f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Option<T> {
    let guard = DB.lock().ok()?;
    let conn = guard.as_ref()?;
    match f(conn) {
        Ok(value) => Some(value),
        Err(err) => {
            use std::sync::atomic::{AtomicBool, Ordering};
            static REPORTED: AtomicBool = AtomicBool::new(false);
            if !REPORTED.swap(true, Ordering::Relaxed) {
                eprintln!("ocg: sqlite error: {}", err);
            }
            None
        }
    }
}

/// Where the database lives (for diagnostics).
pub fn location() -> Option<PathBuf> {
    db_path()
}

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS meta (
             key   TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS samples (
             ts           INTEGER NOT NULL,
             provider     TEXT    NOT NULL,
             account      TEXT    NOT NULL,
             metric       TEXT    NOT NULL,
             used_percent INTEGER,
             reset_at     INTEGER,
             window_secs  INTEGER,
             detail       TEXT,
             label        TEXT
         );
         CREATE INDEX IF NOT EXISTS samples_lookup
             ON samples(provider, account, metric, ts);
         CREATE TABLE IF NOT EXISTS snapshots (
             provider TEXT    NOT NULL,
             account  TEXT    NOT NULL,
             ts       INTEGER NOT NULL,
             payload  TEXT    NOT NULL,
             PRIMARY KEY (provider, account)
         );",
    )?;
    conn.execute(
        "INSERT INTO meta(key, value) VALUES('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![SCHEMA_VERSION.to_string()],
    )?;
    Ok(())
}

fn meta_get(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| r.get(0))
        .optional()
        .ok()
        .flatten()
}

fn meta_set(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO meta(key, value) VALUES(?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Writes
// ---------------------------------------------------------------------------

/// Sample one window of one account.
#[allow(clippy::too_many_arguments)]
fn insert_sample(
    conn: &Connection,
    ts: i64,
    provider: &str,
    account: &str,
    metric: &str,
    used_percent: Option<i32>,
    reset_at: Option<i64>,
    window_secs: Option<i64>,
    detail: &str,
    label: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO samples(ts, provider, account, metric, used_percent, reset_at, window_secs, detail, label)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            ts,
            provider,
            account,
            metric,
            used_percent,
            reset_at,
            window_secs,
            detail,
            label
        ],
    )?;
    Ok(())
}

/// Record a Codex account: both window samples plus the payload snapshot.
pub fn record_codex(home: &str, label: &str, snapshot: &Snapshot) {
    let payload = match serde_json::to_string(snapshot) {
        Ok(p) => p,
        Err(_) => return,
    };
    let ts = if snapshot.fetched_at > 0 { snapshot.fetched_at } else { now_unix() };
    let account = home.to_string();
    let label = label.to_string();
    let snapshot = snapshot.clone();

    with_db(move |conn| {
        // Window samples keyed by slot, not by displayed name: the window length
        // can change (5h/7d) while the slot stays primary/secondary.
        for (metric, window) in
            [("primary", snapshot.primary.as_ref()), ("secondary", snapshot.secondary.as_ref())]
        {
            if let Some(window) = window {
                insert_sample(
                    conn,
                    ts,
                    "codex",
                    &account,
                    metric,
                    Some(window.used_percent),
                    window.reset_at,
                    window.window_secs,
                    &snapshot.email,
                    &label,
                )?;
            }
        }
        // One line per refresh, so "how much did this account move" is queryable
        // without touching the window rows.
        insert_sample(
            conn,
            ts,
            "codex",
            &account,
            "snapshot",
            Some(snapshot.criticality()),
            None,
            None,
            snapshot.reached_note.as_deref().unwrap_or(""),
            &label,
        )?;

        conn.execute(
            "INSERT INTO snapshots(provider, account, ts, payload) VALUES('codex', ?1, ?2, ?3)
             ON CONFLICT(provider, account) DO UPDATE SET ts = excluded.ts, payload = excluded.payload",
            params![account, ts, payload],
        )?;

        let cutoff = now_unix() - RETENTION_DAYS * 86400;
        conn.execute("DELETE FROM samples WHERE ts < ?1", params![cutoff])?;
        Ok(())
    });
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

/// Last good snapshot per home, for the "cached" fallback path.
pub fn last_codex_snapshots() -> HashMap<String, Snapshot> {
    with_db(|conn| {
        let mut out = HashMap::new();
        let mut stmt =
            conn.prepare("SELECT account, ts, payload FROM snapshots WHERE provider = 'codex'")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?))
        })?;
        for row in rows.flatten() {
            let (account, ts, payload) = row;
            if let Ok(mut snap) = serde_json::from_str::<Snapshot>(&payload) {
                if snap.fetched_at == 0 {
                    snap.fetched_at = ts;
                }
                out.insert(account, snap);
            }
        }
        Ok(out)
    })
    .unwrap_or_default()
}

/// Quota consumed per limit window since `since`, in percent-of-window units.
///
/// Walks the samples in order and adds each rise:
///
///   * same window  → only a rise counts (a drop is a stale or corrected read)
///   * new window   → the reading itself counts, since the window opened inside
///                    the range and everything in it happened after `since`
///   * no baseline  → only counted when the window itself started inside the
///                    range; otherwise the reading includes unknown history
///                    (a 5h window that opened before midnight, say) and is
///                    skipped rather than over-reported.
pub fn consumed_since(
    conn: &Connection,
    account: &str,
    metric: &str,
    since: i64,
) -> rusqlite::Result<f64> {
    let baseline: Option<(i32, Option<i64>)> = conn
        .query_row(
            "SELECT used_percent, reset_at FROM samples
              WHERE provider = 'codex' AND account = ?1 AND metric = ?2 AND ts < ?3
              ORDER BY ts DESC LIMIT 1",
            params![account, metric, since],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;

    let mut stmt = conn.prepare(
        "SELECT used_percent, reset_at, window_secs FROM samples
          WHERE provider = 'codex' AND account = ?1 AND metric = ?2 AND ts >= ?3
          ORDER BY ts ASC",
    )?;
    let rows = stmt.query_map(params![account, metric, since], |r| {
        Ok((
            r.get::<_, i32>(0)?,
            r.get::<_, Option<i64>>(1)?,
            r.get::<_, Option<i64>>(2)?,
        ))
    })?;

    let mut total = 0.0f64;
    let mut prev: Option<(i32, Option<i64>)> = baseline;
    for row in rows.flatten() {
        let (used, reset, window_secs) = row;
        match prev {
            Some((prev_used, prev_reset)) if prev_reset == reset => {
                let delta = used - prev_used;
                if delta > 0 {
                    total += f64::from(delta);
                }
            }
            Some(_) => {
                // The window rolled over inside the range.
                total += f64::from(used.max(0));
            }
            None => {
                if window_opened_after(reset, window_secs, since) {
                    total += f64::from(used.max(0));
                }
            }
        }
        prev = Some((used, reset));
    }
    Ok(total)
}

/// Did this limit window open at or after `since`?
fn window_opened_after(reset_at: Option<i64>, window_secs: Option<i64>, since: i64) -> bool {
    match (reset_at, window_secs) {
        (Some(reset), Some(secs)) => reset - secs >= since,
        _ => false,
    }
}

/// Per-account consumption since a timestamp, keyed by home.
pub fn today_by_account(since: i64) -> HashMap<String, f64> {
    with_db(|conn| {
        let mut accounts: Vec<String> = Vec::new();
        {
            let mut stmt = conn
                .prepare("SELECT DISTINCT account FROM samples WHERE provider = 'codex'")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            for row in rows.flatten() {
                accounts.push(row);
            }
        }
        let mut out = HashMap::new();
        for account in accounts {
            let consumed = consumed_since(conn, &account, "primary", since)?;
            out.insert(account, consumed);
        }
        Ok(out)
    })
    .unwrap_or_default()
}

/// Row counts and time range, for `--once stats` and diagnostics.
pub fn summary() -> serde_json::Value {
    with_db(|conn| {
        let samples: i64 = conn.query_row("SELECT COUNT(*) FROM samples", [], |r| r.get(0))?;
        let accounts: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT account) FROM samples WHERE provider = 'codex'",
            [],
            |r| r.get(0),
        )?;
        let first: Option<i64> =
            conn.query_row("SELECT MIN(ts) FROM samples", [], |r| r.get(0)).ok().flatten();
        let last: Option<i64> =
            conn.query_row("SELECT MAX(ts) FROM samples", [], |r| r.get(0)).ok().flatten();
        Ok(serde_json::json!({
            "samples": samples,
            "accounts": accounts,
            "first_ts": first,
            "last_ts": last,
        }))
    })
    .unwrap_or(serde_json::Value::Null)
}

// ---------------------------------------------------------------------------
// One-time import of the pre-SQLite JSON cache
// ---------------------------------------------------------------------------

fn import_legacy_cache(conn: &Connection) {
    let path = match config::cache_dir() {
        Ok(dir) => dir.join("codex.json"),
        Err(_) => return,
    };
    if !path.is_file() {
        return;
    }
    if meta_get(conn, "legacy_cache_imported").is_some() {
        return;
    }

    #[derive(serde::Deserialize)]
    struct CacheFile {
        #[serde(default)]
        accounts: HashMap<String, Snapshot>,
    }

    if let Ok(data) = std::fs::read(&path) {
        if let Ok(cache) = serde_json::from_slice::<CacheFile>(&data) {
            let mut imported = 0;
            for (home, snapshot) in &cache.accounts {
                record_codex_into(conn, home, "", snapshot);
                imported += 1;
            }
            let _ = meta_set(conn, "legacy_cache_imported", &imported.to_string());
        }
    }
    // Keep the old file under a name that makes the migration obvious.
    let _ = std::fs::rename(&path, path.with_extension("json.migrated"));
}

/// Connection-scoped variant of [`record_codex`], used during import and tests.
fn record_codex_into(conn: &Connection, home: &str, label: &str, snapshot: &Snapshot) {
    let ts = if snapshot.fetched_at > 0 { snapshot.fetched_at } else { now_unix() };
    for (metric, window) in
        [("primary", snapshot.primary.as_ref()), ("secondary", snapshot.secondary.as_ref())]
    {
        if let Some(window) = window {
            let _ = insert_sample(
                conn,
                ts,
                "codex",
                home,
                metric,
                Some(window.used_percent),
                window.reset_at,
                window.window_secs,
                &snapshot.email,
                label,
            );
        }
    }
    if let Ok(payload) = serde_json::to_string(snapshot) {
        let _ = conn.execute(
            "INSERT INTO snapshots(provider, account, ts, payload) VALUES('codex', ?1, ?2, ?3)
             ON CONFLICT(provider, account) DO UPDATE SET ts = excluded.ts, payload = excluded.payload",
            params![home, ts, payload],
        );
    }
}

pub fn now_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

/// True when a snapshot is recent enough to be shown as a fallback.
pub fn snapshot_is_fresh(snapshot: &Snapshot) -> bool {
    snapshot.fetched_at > 0 && now_unix() - snapshot.fetched_at < CACHE_MAX_AGE_SECS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch_codex::{Snapshot, Window};

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn
    }

    fn snap(ts: i64, primary: i32, primary_reset: i64, secondary: i32) -> Snapshot {
        Snapshot {
            email: "a@example.com".to_string(),
            plan: "plus".to_string(),
            fetched_at: ts,
            primary: Some(Window {
                used_percent: primary,
                reset_at: Some(primary_reset),
                window_secs: Some(18000),
                ..Default::default()
            }),
            secondary: Some(Window {
                used_percent: secondary,
                reset_at: Some(primary_reset + 500000),
                window_secs: Some(604800),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn migrate_is_idempotent() {
        let conn = db();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        assert_eq!(meta_get(&conn, "schema_version").as_deref(), Some("1"));
    }

    #[test]
    fn records_windows_and_payload() {
        let conn = db();
        record_codex_into(&conn, "~/.codex", "main", &snap(1000, 10, 5000, 20));
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM samples WHERE metric IN ('primary','secondary')", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(rows, 2);
        let (label, window): (String, i64) = conn
            .query_row(
                "SELECT label, window_secs FROM samples WHERE metric = 'primary'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(label, "main");
        assert_eq!(window, 18000);
        let payload: String = conn
            .query_row("SELECT payload FROM snapshots WHERE account = '~/.codex'", [], |r| r.get(0))
            .unwrap();
        assert!(payload.contains("a@example.com"));
    }

    #[test]
    fn snapshot_upsert_keeps_one_row_per_account() {
        let conn = db();
        record_codex_into(&conn, "~/.codex", "", &snap(1000, 10, 5000, 20));
        record_codex_into(&conn, "~/.codex", "", &snap(2000, 30, 5000, 25));
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
        let ts: i64 = conn.query_row("SELECT ts FROM snapshots", [], |r| r.get(0)).unwrap();
        assert_eq!(ts, 2000);
    }

    #[test]
    fn consumption_sums_rises_within_a_window() {
        let conn = db();
        // window opened at 9000-18000 = long before the range, so the first
        // reading carries unknown history and is skipped
        for (ts, used) in [(100, 5), (200, 12), (300, 12), (400, 30)] {
            record_codex_into(&conn, "h", "", &snap(ts, used, 9000, 1));
        }
        // rises inside the range: +7, +0, +18
        assert_eq!(consumed_since(&conn, "h", "primary", 100).unwrap(), 25.0);
    }

    #[test]
    fn a_window_that_opens_inside_the_range_counts_its_reading() {
        let conn = db();
        // reset 20000 with a 18000s window → opened at 2000, after since = 1000
        record_codex_into(&conn, "h", "", &snap(2000, 5, 20000, 1));
        record_codex_into(&conn, "h", "", &snap(2500, 9, 20000, 1));
        assert_eq!(consumed_since(&conn, "h", "primary", 1000).unwrap(), 9.0);
    }

    #[test]
    fn consumption_uses_a_baseline_before_the_range() {
        let conn = db();
        record_codex_into(&conn, "h", "", &snap(100, 40, 9000, 1));
        record_codex_into(&conn, "h", "", &snap(300, 55, 9000, 1));
        // range starts at 200: baseline 40 -> +15
        assert_eq!(consumed_since(&conn, "h", "primary", 200).unwrap(), 15.0);
    }

    #[test]
    fn a_new_window_counts_the_fresh_reading() {
        let conn = db();
        record_codex_into(&conn, "h", "", &snap(100, 80, 9000, 1));
        record_codex_into(&conn, "h", "", &snap(300, 0, 9500, 1)); // window rolled over
        record_codex_into(&conn, "h", "", &snap(400, 20, 9500, 1));
        // rolled over: the 20% of the new window counts; the 80% is old quota
        assert_eq!(consumed_since(&conn, "h", "primary", 100).unwrap(), 20.0);
    }

    #[test]
    fn a_drop_inside_one_window_is_not_negative() {
        let conn = db();
        record_codex_into(&conn, "h", "", &snap(100, 50, 9000, 1));
        record_codex_into(&conn, "h", "", &snap(200, 20, 9000, 1));
        record_codex_into(&conn, "h", "", &snap(300, 30, 9000, 1));
        assert_eq!(consumed_since(&conn, "h", "primary", 100).unwrap(), 10.0);
    }

    #[test]
    fn no_samples_means_zero() {
        let conn = db();
        assert_eq!(consumed_since(&conn, "missing", "primary", 0).unwrap(), 0.0);
    }
}
