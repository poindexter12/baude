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
use std::time::Duration;

#[cfg(not(test))]
use serde_json::Value;

const POLL_SECS: u64 = 60;
/// Back off when ccusage is missing/broken — don't spawn a failing process
/// every minute forever.
const FAIL_POLL_SECS: u64 = 300;

/// What the poller does for a configured interval (PERF-08). Pure, so the
/// decision is testable while the worker thread stays `cfg(not(test))`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PollerPlan {
    Disabled,
    Enabled {
        interval: Duration,
        /// Sleep after a failed `ccusage` run: never shorter than the
        /// configured interval, so a slow schedule is honored on failure too.
        failure_backoff: Duration,
    },
}

pub(crate) fn poller_plan(usage_poll_secs: Option<u64>) -> PollerPlan {
    match usage_poll_secs {
        Some(0) => PollerPlan::Disabled,
        secs => {
            let interval = Duration::from_secs(secs.unwrap_or(POLL_SECS));
            PollerPlan::Enabled {
                interval,
                failure_backoff: interval.max(Duration::from_secs(FAIL_POLL_SECS)),
            }
        }
    }
}

#[derive(Default, Clone)]
pub struct UsageCosts {
    pub today_usd: Option<f64>,
    pub week_usd: Option<f64>,
}

pub struct UsagePoller {
    data: Arc<Mutex<UsageCosts>>,
    disabled: bool,
}

impl UsagePoller {
    /// The inert test implementation: an empty snapshot and nothing else.
    ///
    /// No thread, no `date`, no `ccusage`. Same signature as the live one, so
    /// no caller — and no fixture — can tell them apart or bypass this.
    #[cfg(test)]
    pub fn start(config_poll_secs: Option<u64>) -> UsagePoller {
        UsagePoller {
            data: Arc::new(Mutex::new(UsageCosts::default())),
            disabled: poller_plan(config_poll_secs) == PollerPlan::Disabled,
        }
    }

    #[cfg(not(test))]
    pub fn start(config_poll_secs: Option<u64>) -> UsagePoller {
        let data = Arc::new(Mutex::new(UsageCosts::default()));
        let (interval, failure_backoff) = match poller_plan(config_poll_secs) {
            PollerPlan::Disabled => {
                return UsagePoller {
                    data,
                    disabled: true,
                }
            }
            PollerPlan::Enabled {
                interval,
                failure_backoff,
            } => (interval, failure_backoff),
        };
        let shared = Arc::clone(&data);
        std::thread::spawn(move || loop {
            let costs = fetch();
            let ok = costs.today_usd.is_some() || costs.week_usd.is_some();
            if let Ok(mut d) = shared.lock() {
                *d = costs;
            }
            std::thread::sleep(if ok { interval } else { failure_backoff });
        });
        UsagePoller {
            data,
            disabled: false,
        }
    }
    /// True when `usage_poll_secs` is 0: no worker runs and the footer says so.
    pub fn is_disabled(&self) -> bool {
        self.disabled
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
        assert_eq!(poller_plan(Some(0)), PollerPlan::Disabled);
        let poller = UsagePoller::start(Some(0));
        assert!(poller.is_disabled(), "Some(0) disables the poller");
        assert!(
            poller.is_inert_for_test(),
            "no worker holds the shared data"
        );
    }

    #[test]
    fn poll_disabled_returns_inert_poller() {
        let poller = UsagePoller::start(Some(0));
        let costs = poller.costs();
        assert!(costs.today_usd.is_none() && costs.week_usd.is_none());
        assert!(poller.is_disabled());
    }

    #[test]
    fn poll_enabled_spawns_thread() {
        // The worker itself is compiled out under test; the plan it would run is not.
        assert_eq!(
            poller_plan(None),
            PollerPlan::Enabled {
                interval: Duration::from_secs(POLL_SECS),
                failure_backoff: Duration::from_secs(FAIL_POLL_SECS),
            }
        );
        assert_eq!(
            poller_plan(Some(30)),
            PollerPlan::Enabled {
                interval: Duration::from_secs(30),
                failure_backoff: Duration::from_secs(FAIL_POLL_SECS),
            },
            "a short interval keeps the default failure back-off"
        );
        assert_eq!(
            poller_plan(Some(600)),
            PollerPlan::Enabled {
                interval: Duration::from_secs(600),
                failure_backoff: Duration::from_secs(600),
            },
            "failure back-off never runs ccusage more often than the configured interval"
        );
        assert!(!UsagePoller::start(Some(30)).is_disabled());
    }
}
