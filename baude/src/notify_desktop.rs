//! macOS desktop notifications for the TUI: a banner when a session flips to
//! needing you — waiting on input past a debounce, a pending tool permission,
//! a finished turn, or an exit. Built for multi-clauding: walk away from N
//! sessions in one baude; the banner names WHICH one blocked.
//!
//! The when-to-fire state machine mirrors `bauded/src/notify.rs` (the Web
//! Push notifier) so desktop and phone agree on semantics: 10s waiting
//! debounce re-armed per busy turn, a distinct permission banner (mutually
//! exclusive with the generic waiting one), a gentle completed banner only on
//! the edge out of a genuinely-busy turn, exited only for a session
//! previously seen alive, and archived sessions muted. Covers BOTH local
//! sessions and the remote (daemon) rows the sidebar shows.
//!
//! Posting is `osascript -e 'display notification …'` — the same mechanism
//! as Joe's Claude Code `Notification` hook banner — spawned on a throwaway
//! thread so the ~100ms osascript round-trip never stalls the render loop.
//! Non-macOS builds compile but post nothing.

use std::collections::{HashMap, HashSet};

/// Don't notify for blips — the CLI often pauses briefly mid-turn.
const WAITING_DEBOUNCE_MS: u64 = 10_000;

/// One sidebar row (local or remote), flattened to what the notifier needs.
pub struct Row {
    /// Stable key across ticks: `l<id>` for local sessions, `r<id>` remote.
    pub key: String,
    pub name: String,
    /// "waiting" | "busy" | "completed" | "exited".
    pub status: &'static str,
    pub waiting_for_ms: Option<u64>,
    /// "permission" routes the distinct permission banner.
    pub waiting_reason: Option<String>,
    pub archived: bool,
}

pub struct Banner {
    pub title: String,
    pub body: String,
    /// macOS notification sound name; `None` posts silently (the calm
    /// completed/exited banners).
    pub sound: Option<&'static str>,
}

#[derive(Default)]
pub struct DesktopNotifier {
    notified_waiting: HashSet<String>,
    notified_permission: HashSet<String>,
    notified_completed: HashSet<String>,
    notified_exited: HashSet<String>,
    last_status: HashMap<String, &'static str>,
}

impl DesktopNotifier {
    /// Advance the state machine one tick; returns the banners to post.
    pub fn tick(&mut self, rows: &[Row]) -> Vec<Banner> {
        let mut out = Vec::new();
        let live: HashSet<&str> = rows.iter().map(|r| r.key.as_str()).collect();
        self.notified_waiting.retain(|k| live.contains(k.as_str()));
        self.notified_permission
            .retain(|k| live.contains(k.as_str()));
        self.notified_completed
            .retain(|k| live.contains(k.as_str()));
        self.notified_exited.retain(|k| live.contains(k.as_str()));
        self.last_status.retain(|k, _| live.contains(k.as_str()));

        for r in rows {
            let prev = self.last_status.insert(r.key.clone(), r.status);
            // Archived means "stop demanding my attention" — mute, but keep
            // last_status current so unarchiving doesn't false-fire.
            if r.archived {
                self.notified_waiting.remove(&r.key);
                self.notified_permission.remove(&r.key);
                self.notified_completed.remove(&r.key);
                continue;
            }
            let is_permission = r.waiting_reason.as_deref() == Some("permission");
            if !is_permission {
                self.notified_permission.remove(&r.key);
            }
            match r.status {
                "waiting" if is_permission => {
                    if self.notified_permission.insert(r.key.clone()) {
                        out.push(Banner {
                            title: format!("{} needs permission", r.name),
                            body: "wants to run a tool — approve?".into(),
                            sound: Some("Glass"),
                        });
                    }
                }
                "waiting" => {
                    let waited = r.waiting_for_ms.unwrap_or(0);
                    if waited >= WAITING_DEBOUNCE_MS && self.notified_waiting.insert(r.key.clone())
                    {
                        out.push(Banner {
                            title: format!("{} is waiting for you", r.name),
                            body: String::new(),
                            sound: Some("Glass"),
                        });
                    }
                }
                "completed" => {
                    // Only on the edge out of a genuinely busy turn — never on
                    // first sighting already-completed (e.g. TUI restart).
                    if prev == Some("busy") && self.notified_completed.insert(r.key.clone()) {
                        out.push(Banner {
                            title: format!("{} finished", r.name),
                            body: String::new(),
                            sound: None,
                        });
                    }
                }
                "exited" => {
                    // Only for a session previously seen alive this run.
                    let was_alive = matches!(prev, Some(p) if p != "exited");
                    if was_alive && self.notified_exited.insert(r.key.clone()) {
                        out.push(Banner {
                            title: format!("{} exited", r.name),
                            body: String::new(),
                            sound: None,
                        });
                    }
                }
                _ => {
                    // Busy: re-arm the per-turn notifiers.
                    self.notified_waiting.remove(&r.key);
                    self.notified_completed.remove(&r.key);
                }
            }
        }
        out
    }
}

/// Post one banner via osascript, off-thread (never block the render loop).
/// No-op off macOS; a missing/failed osascript is silently ignored — a
/// notification is never worth an error loop.
pub fn post(banner: Banner) {
    if !cfg!(target_os = "macos") {
        return;
    }
    std::thread::spawn(move || {
        let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let mut script = format!(
            "display notification \"{}\" with title \"baude — {}\"",
            esc(&banner.body),
            esc(&banner.title),
        );
        if let Some(sound) = banner.sound {
            script.push_str(&format!(" sound name \"{sound}\""));
        }
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(key: &str, status: &'static str, waited: u64, reason: Option<&str>) -> Row {
        Row {
            key: key.into(),
            name: key.into(),
            status,
            waiting_for_ms: Some(waited),
            waiting_reason: reason.map(str::to_string),
            archived: false,
        }
    }

    #[test]
    fn waiting_debounces_and_rearms_per_turn() {
        let mut n = DesktopNotifier::default();
        // Under the debounce: silent.
        assert!(n.tick(&[row("l1", "waiting", 3_000, None)]).is_empty());
        // Past it: exactly one banner, then silence while still waiting.
        let b = n.tick(&[row("l1", "waiting", 12_000, None)]);
        assert_eq!(b.len(), 1);
        assert!(b[0].title.contains("waiting for you"));
        assert_eq!(b[0].sound, Some("Glass"));
        assert!(n.tick(&[row("l1", "waiting", 20_000, None)]).is_empty());
        // A busy turn re-arms; the next waiting stretch fires again.
        assert!(n.tick(&[row("l1", "busy", 0, None)]).is_empty());
        assert_eq!(n.tick(&[row("l1", "waiting", 15_000, None)]).len(), 1);
    }

    #[test]
    fn permission_is_distinct_immediate_and_exclusive() {
        let mut n = DesktopNotifier::default();
        // No debounce for permissions — they block the agent right now.
        let b = n.tick(&[row("l1", "waiting", 0, Some("permission"))]);
        assert_eq!(b.len(), 1);
        assert!(b[0].title.contains("needs permission"));
        // Still pending: no repeat, and no generic waiting banner either.
        assert!(n
            .tick(&[row("l1", "waiting", 60_000, Some("permission"))])
            .is_empty());
        // Resolved into plain waiting: permission re-arms, generic fires.
        let b = n.tick(&[row("l1", "waiting", 60_000, Some("input"))]);
        assert_eq!(b.len(), 1);
        assert!(b[0].title.contains("waiting for you"));
    }

    #[test]
    fn completed_only_on_busy_edge_and_exited_only_if_seen_alive() {
        let mut n = DesktopNotifier::default();
        // First sighting already completed (TUI restart): silent.
        assert!(n.tick(&[row("l1", "completed", 0, None)]).is_empty());
        // Busy → completed edge: one calm banner, no sound.
        n.tick(&[row("l1", "busy", 0, None)]);
        let b = n.tick(&[row("l1", "completed", 0, None)]);
        assert_eq!(b.len(), 1);
        assert!(b[0].title.contains("finished"));
        assert_eq!(b[0].sound, None);
        // First sighting already exited: silent; alive→exited: one banner.
        let mut n = DesktopNotifier::default();
        assert!(n.tick(&[row("l2", "exited", 0, None)]).is_empty());
        n.tick(&[row("l3", "busy", 0, None)]);
        assert_eq!(n.tick(&[row("l3", "exited", 0, None)]).len(), 1);
    }

    #[test]
    fn archived_is_muted_and_unarchive_does_not_false_fire() {
        let mut n = DesktopNotifier::default();
        let mut r = row("l1", "waiting", 60_000, None);
        r.archived = true;
        assert!(n.tick(&[r]).is_empty());
        // Unarchived and still waiting: fires (fresh attention request).
        assert_eq!(n.tick(&[row("l1", "waiting", 60_000, None)]).len(), 1);
    }

    #[test]
    fn vanished_sessions_are_forgotten() {
        let mut n = DesktopNotifier::default();
        n.tick(&[row("r9", "busy", 0, None)]);
        assert_eq!(n.tick(&[row("r9", "waiting", 15_000, None)]).len(), 1);
        // Row disappears (daemon session deleted), then a NEW session reuses
        // the key: state was purged, it behaves like a fresh session.
        n.tick(&[]);
        n.tick(&[row("r9", "busy", 0, None)]);
        assert_eq!(n.tick(&[row("r9", "waiting", 15_000, None)]).len(), 1);
    }
}
