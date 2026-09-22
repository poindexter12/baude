//! Global usage costs via `ccusage` (https://ccusage.com), polled on a
//! background thread. A `ccusage daily|weekly --json` run scans every Claude
//! transcript on disk (~2s), far too slow for the draw loop — so a worker
//! refreshes a shared snapshot once a minute and the UI only ever reads the
//! cache. If ccusage isn't installed the fields stay `None` and the sidebar
//! shows em-dashes.
//!
//! # Why the worker is COMPILED OUT of test builds
//!
//! "Scans every Claude transcript on disk" is the whole problem. The scan
//! happens inside `ccusage`, reached through the *inherited environment*, on a
//! detached thread that never sees the fixture's thread-local redirects. No
//! resolver guard can intercept it and no filesystem no-write observer can see
//! it, because it is a READ of the developer's real data (#72).
//!
//! So [`UsagePoller::start`] has two implementations behind one signature. The
//! `cfg(test)` one builds the empty snapshot and returns; the worker loop, the
//! polling constants and every subprocess helper exist only in non-test builds.
//! This protects EVERY `App` fixture, not a selected helper — an `App` cannot
//! opt back in, and neither can a test that constructs a `UsagePoller`
//! directly. Starting the real worker and discarding its handle afterwards
//! would not have worked: the loop is detached and outlives the handle, so it
//! can read before anything discards it.

#[cfg(not(test))]
use std::process::Command;
use std::sync::{Arc, Mutex};
#[cfg(not(test))]
use std::time::Duration;

#[cfg(not(test))]
use serde_json::Value;

#[cfg(not(test))]
const POLL_SECS: u64 = 60;
/// Back off when ccusage is missing/broken — don't spawn a failing process
/// every minute forever.
#[cfg(not(test))]
const FAIL_POLL_SECS: u64 = 300;

#[derive(Default, Clone)]
pub struct UsageCosts {
    pub today_usd: Option<f64>,
    pub week_usd: Option<f64>,
}

pub struct UsagePoller {
    data: Arc<Mutex<UsageCosts>>,
}

impl UsagePoller {
    /// The inert test implementation: an empty snapshot and nothing else.
    ///
    /// No thread, no `date`, no `ccusage`. Same signature as the live one, so
    /// no caller — and no fixture — can tell them apart or bypass this.
    #[cfg(test)]
    pub fn start() -> UsagePoller {
        UsagePoller {
            data: Arc::new(Mutex::new(UsageCosts::default())),
        }
    }

    #[cfg(not(test))]
    pub fn start() -> UsagePoller {
        let data = Arc::new(Mutex::new(UsageCosts::default()));
        let shared = Arc::clone(&data);
        std::thread::spawn(move || loop {
            let costs = fetch();
            let ok = costs.today_usd.is_some() || costs.week_usd.is_some();
            if let Ok(mut d) = shared.lock() {
                *d = costs;
            }
            std::thread::sleep(Duration::from_secs(if ok {
                POLL_SECS
            } else {
                FAIL_POLL_SECS
            }));
        });
        UsagePoller { data }
    }

    pub fn costs(&self) -> UsageCosts {
        self.data.lock().map(|d| d.clone()).unwrap_or_default()
    }

    /// True when NOTHING but this poller holds the snapshot.
    ///
    /// A live worker clones the `Arc` before it is spawned and holds it for the
    /// process lifetime, so a strong count of one is a compile-independent
    /// statement that no background thread exists to publish into it later.
    /// That is why the regression asserts this rather than sleeping: a sleep
    /// only proves nothing happened *yet*.
    #[cfg(test)]
    pub fn is_inert_for_test(&self) -> bool {
        Arc::strong_count(&self.data) == 1
    }
}

#[cfg(not(test))]
fn fetch() -> UsageCosts {
    UsageCosts {
        today_usd: total_cost("daily", &local_today()),
        week_usd: total_cost("weekly", ""),
    }
}

/// Local YYYY-MM-DD. Shelled out to `date` because pulling in chrono for one
/// string isn't worth it; ccusage groups by local date the same way.
#[cfg(not(test))]
fn local_today() -> String {
    Command::new("date")
        .arg("+%F")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Run `ccusage <granularity> --json -O` and return the matching period's
/// totalCost. For "weekly" the last entry is the current week; for "daily"
/// the entry must match today's date (the last entry is yesterday when
/// nothing has run yet today — report 0, not yesterday's bill).
#[cfg(not(test))]
fn total_cost(granularity: &str, today: &str) -> Option<f64> {
    let out = Command::new("ccusage")
        .args([granularity, "--json", "-O"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v: Value = serde_json::from_slice(&out.stdout).ok()?;
    let rows = v[granularity].as_array()?;
    if granularity == "daily" {
        return Some(
            rows.iter()
                .find(|r| r["period"].as_str() == Some(today))
                .and_then(|r| r["totalCost"].as_f64())
                .unwrap_or(0.0),
        );
    }
    rows.last()?["totalCost"].as_f64()
}

pub fn human_cost(usd: Option<f64>) -> String {
    match usd {
        Some(c) if c >= 1000.0 => format!("${:.1}k", c / 1000.0),
        Some(c) => format!("${c:.2}"),
        None => "—".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_disabled_spawns_no_thread() {
        // UsagePoller with disabled config should not spawn thread
        todo!("Test UsagePoller::start(Some(0)) spawns no thread")
    }

    #[test]
    fn poll_disabled_returns_inert_poller() {
        // Disabled poller should still be valid
        todo!("Test disabled poller is inert but doesn't panic")
    }

    #[test]
    fn poll_enabled_spawns_thread() {
        // UsagePoller with enabled config should spawn thread
        todo!("Test UsagePoller::start(Some(30)) spawns thread")
    }
}
