//! Global usage costs via `ccusage` (https://ccusage.com), polled on a
//! background thread. A `ccusage daily|weekly --json` run scans every Claude
//! transcript on disk (~2s), far too slow for the draw loop — so a worker
//! refreshes a shared snapshot once a minute and the UI only ever reads the
//! cache. If ccusage isn't installed the fields stay `None` and the sidebar
//! shows em-dashes.

use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;

const POLL_SECS: u64 = 60;
/// Back off when ccusage is missing/broken — don't spawn a failing process
/// every minute forever.
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
}

fn fetch() -> UsageCosts {
    UsageCosts {
        today_usd: total_cost("daily", &local_today()),
        week_usd: total_cost("weekly", ""),
    }
}

/// Local YYYY-MM-DD. Shelled out to `date` because pulling in chrono for one
/// string isn't worth it; ccusage groups by local date the same way.
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
