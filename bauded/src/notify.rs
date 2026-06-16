//! Decides *when* to push: a session that has been waiting on the user for
//! a debounce window (cutting spinner-flicker noise), or one that exited.
//! Pure state machine over `SessionInfo` snapshots — the sending happens
//! elsewhere, outside any locks.

use std::collections::{HashMap, HashSet};

use crate::manager::SessionInfo;

/// Don't notify for blips — claude often pauses briefly mid-turn.
const WAITING_DEBOUNCE_MS: u64 = 10_000;

#[derive(Default)]
pub struct Notifier {
    /// Sessions already notified for their current waiting stretch.
    notified_waiting: HashSet<u64>,
    /// Sessions already notified as exited.
    notified_exited: HashSet<u64>,
    last_status: HashMap<u64, String>,
}

pub struct Notification {
    pub title: String,
    pub body: String,
    pub sid: u64,
}

impl Notification {
    pub fn to_json(&self) -> Vec<u8> {
        serde_json::json!({
            "title": self.title,
            "body": self.body,
            "tag": format!("baude-{}", self.sid),
            "sid": self.sid,
        })
        .to_string()
        .into_bytes()
    }
}

impl Notifier {
    pub fn tick(&mut self, sessions: &[SessionInfo]) -> Vec<Notification> {
        let mut out = Vec::new();
        let live: HashSet<u64> = sessions.iter().map(|s| s.id).collect();
        self.notified_waiting.retain(|id| live.contains(id));
        self.notified_exited.retain(|id| live.contains(id));
        self.last_status.retain(|id, _| live.contains(id));

        for s in sessions {
            // Archived means "stop demanding my attention" — mute it, but
            // keep last_status current so unarchiving doesn't false-fire.
            if s.archived {
                self.notified_waiting.remove(&s.id);
                self.last_status.insert(s.id, s.status.to_string());
                continue;
            }
            match s.status {
                "waiting" => {
                    let waited = s.waiting_for_ms.unwrap_or(0);
                    if waited >= WAITING_DEBOUNCE_MS && self.notified_waiting.insert(s.id) {
                        out.push(Notification {
                            title: format!("{} is waiting for you", s.name),
                            body: s.title.clone().unwrap_or_default(),
                            sid: s.id,
                        });
                    }
                }
                "exited" => {
                    self.notified_waiting.remove(&s.id);
                    // Only sessions seen alive before count — don't fire for
                    // something that was already dead when we started.
                    let was_alive = self
                        .last_status
                        .get(&s.id)
                        .map(|p| p != "exited")
                        .unwrap_or(false);
                    if was_alive && self.notified_exited.insert(s.id) {
                        out.push(Notification {
                            title: format!("{} exited", s.name),
                            body: "claude is no longer running — tap to restart".into(),
                            sid: s.id,
                        });
                    }
                }
                _ => {
                    // busy: a new turn started; re-arm the waiting notifier
                    self.notified_waiting.remove(&s.id);
                    self.notified_exited.remove(&s.id);
                }
            }
            self.last_status.insert(s.id, s.status.to_string());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(id: u64, status: &'static str, waiting_ms: Option<u64>) -> SessionInfo {
        SessionInfo {
            id,
            name: format!("s{id}"),
            title: None,
            status,
            state_source: "silence",
            last_tool: None,
            waiting_for_ms: waiting_ms,
            waiting_reason: None,
            model: None,
            permission_mode: None,
            context_used_pct: None,
            branch: None,
            cwd: String::new(),
            repo_root: String::new(),
            is_worktree: false,
            gsd_milestone: None,
            gsd_phase: None,
            session_cost_usd: None,
            claude_session_id: None,
            archived: false,
            activity: vec![],
        }
    }

    #[test]
    fn archived_sessions_are_muted() {
        let mut n = Notifier::default();
        let mut s = info(1, "waiting", Some(60_000));
        s.archived = true;
        assert!(n.tick(&[s]).is_empty());
    }

    #[test]
    fn waiting_debounces_and_fires_once() {
        let mut n = Notifier::default();
        // brief waits never fire
        assert!(n.tick(&[info(1, "waiting", Some(2_000))]).is_empty());
        // past the debounce: fires exactly once
        let fired = n.tick(&[info(1, "waiting", Some(11_000))]);
        assert_eq!(fired.len(), 1);
        assert!(fired[0].title.contains("waiting"));
        assert!(n.tick(&[info(1, "waiting", Some(60_000))]).is_empty());
        // a new busy turn re-arms it
        assert!(n.tick(&[info(1, "busy", None)]).is_empty());
        assert_eq!(n.tick(&[info(1, "waiting", Some(20_000))]).len(), 1);
    }

    #[test]
    fn exited_fires_once_and_only_after_being_seen_alive() {
        let mut n = Notifier::default();
        // first sighting already exited: no notification
        assert!(n.tick(&[info(1, "exited", None)]).is_empty());
        assert!(n.tick(&[info(1, "exited", None)]).is_empty());
        // alive then exited: one notification
        let mut n = Notifier::default();
        n.tick(&[info(2, "busy", None)]);
        let fired = n.tick(&[info(2, "exited", None)]);
        assert_eq!(fired.len(), 1);
        assert!(fired[0].title.contains("exited"));
        assert!(n.tick(&[info(2, "exited", None)]).is_empty());
    }

    #[test]
    fn deleted_sessions_are_forgotten() {
        let mut n = Notifier::default();
        n.tick(&[info(1, "waiting", Some(20_000))]);
        // session disappears, then a new one reuses the id
        n.tick(&[]);
        assert_eq!(n.tick(&[info(1, "waiting", Some(20_000))]).len(), 1);
    }
}
