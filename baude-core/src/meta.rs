//! Claude Code session metadata, read from the artifacts Claude writes to
//! disk: `$CLAUDE_CONFIG_DIR/sessions/<pid>.json` (live busy/idle status),
//! the session transcript JSONL (model, token usage, permission mode),
//! `/tmp/claude-ctx-<sessionId>.json` (context %, written by statusline
//! hooks), and `.planning/STATE.md` (GSD project state).

use std::collections::VecDeque;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The config dir the spawned claude processes will use (inherited env).
pub fn claude_config_dir() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".claude")))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Claude Code encodes a project cwd into a directory name by replacing
/// every non-alphanumeric character with '-'.
fn encode_path(p: &Path) -> String {
    p.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

#[derive(Default, Clone)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_create: u64,
}

/// One account rate-limit window (5-hour block or 7-day) as captured from
/// the statusline payload by `baude statusline`.
#[derive(Default, Clone, Copy)]
pub struct RateWindow {
    pub used_pct: Option<f64>,
    pub resets_at_unix_s: Option<u64>,
}

/// Pull-request info captured from the bridge file's `pr` object.
#[derive(Default, Clone)]
pub struct PrInfo {
    pub number: Option<u64>,
    pub url: Option<String>,
    pub review_state: Option<String>,
}

/// Worktree info captured from the bridge file's `worktree` object.
#[derive(Default, Clone)]
pub struct WorktreeInfo {
    pub name: Option<String>,
    pub path: Option<String>,
    pub branch: Option<String>,
}

#[derive(Default, Clone)]
pub struct GsdState {
    pub milestone: Option<String>,
    pub status: Option<String>,
    pub active_phase: Option<String>,
    pub next_action: Option<String>,
    pub percent: Option<u8>,
    pub phase_line: Option<String>,
}

/// Drop-oldest cap for the per-session tool-activity ring buffer
/// (`ClaudeMeta::activity`). Older events are evicted once the ring exceeds
/// this size, keeping the buffer bounded regardless of event volume (T-03-01).
pub const ACTIVITY_CAP: usize = 200;

/// One Claude Code hook event retained in the per-session activity ring
/// (`ClaudeMeta::activity`). Serialized to JSON for the daemon's activity
/// endpoints and the TUI/PWA timeline views: `{event, tool?,
/// notification_type?, ts}`. `tool` is present for `PostToolUse`,
/// `notification_type` for `Notification`; both are omitted from JSON when
/// `None`. This is the single serde-`Serialize` type produced by `ClaudeMeta`
/// (the struct itself is intentionally NOT serializable).
#[derive(Default, Clone, Serialize, Deserialize)]
pub struct HookEvent {
    /// Event kind: `PostToolUse` | `UserPromptSubmit` | `Stop` | `Notification`.
    pub event: String,
    /// Tool name, present for `PostToolUse` events.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// Notification type, present for `Notification` events.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification_type: Option<String>,
    /// Event timestamp, unix milliseconds.
    pub ts: u64,
}

#[derive(Default)]
pub struct ClaudeMeta {
    pub session_id: Option<String>,
    transcript: Option<PathBuf>,
    offset: u64,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
    pub last_usage: Option<Usage>,
    pub totals: Usage,
    pub context_used_pct: Option<u8>,
    /// (busy, status_updated_at unix ms) from Claude's own session file.
    pub claude_status: Option<(bool, u64)>,
    /// (busy, event_ts unix ms) derived from the Claude Code hook event
    /// stream (`/tmp/baude-events-<sid>.jsonl`). Higher precedence than
    /// `claude_status` while fresh (see `session.rs::status_with_source`).
    /// Event-driven: only ever set on a new event, never cleared by absence.
    pub hook_status: Option<(bool, u64)>,
    /// (tool_name, event_ts unix ms) of the most recent PostToolUse event.
    pub last_tool: Option<(String, u64)>,
    /// (notification_type, event_ts unix ms) of the most recent Notification
    /// event. Captured for a future Phase 4 `waiting_reason`; intentionally
    /// NOT exposed as a structured SessionInfo field this phase.
    pub last_notification: Option<(String, u64)>,
    /// Byte offset of the last fully-consumed line in the event stream.
    /// Separate from `offset` (the transcript tail) so the two tails never
    /// interfere.
    offset_events: u64,
    /// Resolved event-file path the `offset_events` cursor belongs to. When the
    /// `session_id` rotates mid-session the event-file path flips; tracking it
    /// here lets `read_event_tail` reset the offset on a path change so a stale
    /// (possibly larger) offset can't permanently short-circuit the new file's
    /// events (WR-03), mirroring the transcript tail's reset in
    /// `resolve_transcript`.
    event_path: Option<PathBuf>,
    /// Capped (drop-oldest) ring of recent hook events for the activity
    /// timeline (ACT-01). Appended to by `read_event_tail` as it tails the
    /// event stream, bounded at `ACTIVITY_CAP`, and cleared on event-path
    /// rotation (WR-03) so it never carries stale cross-session events. Private
    /// like `offset_events`/`event_path`; read via the `activity()` accessor.
    activity: VecDeque<HookEvent>,
    pub gsd: Option<GsdState>,
    /// Live session cost from the `baude statusline` bridge file.
    pub session_cost_usd: Option<f64>,
    /// Account rate-limit windows from the bridge file, with the bridge's
    /// write timestamp so the freshest session wins across the app.
    pub rate_5h: Option<RateWindow>,
    pub rate_week: Option<RateWindow>,
    pub rate_updated_unix_ms: u64,
    /// Current git branch of the session's cwd (read from .git/HEAD).
    pub git_branch: Option<String>,
    /// Claude's AI-generated session title (`ai-title` transcript records).
    pub title: Option<String>,
    /// Reasoning-effort level from the bridge file (e.g. "high").
    pub effort: Option<String>,
    /// Whether extended thinking is enabled, from the bridge file.
    pub thinking: Option<bool>,
    /// Vim editing mode from the bridge file. Captured but not rendered.
    pub vim_mode: Option<String>,
    /// Pull-request info from the bridge file's `pr` object.
    pub pr: Option<PrInfo>,
    /// Worktree info from the bridge file's `worktree` object.
    pub worktree: Option<WorktreeInfo>,
}

impl ClaudeMeta {
    pub fn poll(&mut self, cwd: &Path, pid: Option<u32>, spawn_unix_ms: u64, repo_root: &Path) {
        self.poll_session_file(cwd, pid, spawn_unix_ms);
        self.resolve_transcript(cwd, spawn_unix_ms);
        self.read_transcript_tail();
        // GSD ctx file first; the baude bridge file overrides when present
        // (it carries Claude's exact used_percentage).
        self.read_context_file();
        self.read_bridge_file();
        self.read_event_tail();
        self.gsd = parse_gsd(repo_root);
        self.git_branch = read_git_branch(cwd);
    }

    /// Find this session's `sessions/<pid>.json`. Exact pid match wins
    /// (sessions spawned with `exec claude` — the child IS claude); otherwise
    /// match by cwd and pick the file whose start time is closest to ours.
    fn poll_session_file(&mut self, cwd: &Path, pid: Option<u32>, spawn_unix_ms: u64) {
        let dir = claude_config_dir().join("sessions");
        if let Some(pid) = pid {
            if let Some(v) = read_json(&dir.join(format!("{pid}.json"))) {
                self.apply_session_file(&v);
                return;
            }
        }
        let cwd_str = cwd.to_string_lossy();
        let Ok(entries) = fs::read_dir(&dir) else {
            return;
        };
        let mut best: Option<(u64, Value)> = None;
        for entry in entries.flatten() {
            let Some(v) = read_json(&entry.path()) else {
                continue;
            };
            if v["cwd"].as_str() != Some(cwd_str.as_ref()) {
                continue;
            }
            let started = v["startedAt"].as_u64().unwrap_or(0);
            // Ignore session files that predate this baude session.
            if started + 20_000 < spawn_unix_ms {
                continue;
            }
            let dist = started.abs_diff(spawn_unix_ms);
            if best.as_ref().map(|(d, _)| dist < *d).unwrap_or(true) {
                best = Some((dist, v));
            }
        }
        if let Some((_, v)) = best {
            self.apply_session_file(&v);
        }
    }

    fn apply_session_file(&mut self, v: &Value) {
        if let Some(sid) = v["sessionId"].as_str() {
            self.session_id = Some(sid.to_string());
        }
        if let (Some(status), Some(at)) = (v["status"].as_str(), v["statusUpdatedAt"].as_u64()) {
            self.claude_status = Some((status == "busy", at));
        }
    }

    fn resolve_transcript(&mut self, cwd: &Path, spawn_unix_ms: u64) {
        let project_dir = claude_config_dir().join("projects").join(encode_path(cwd));
        let path = if let Some(sid) = &self.session_id {
            let p = project_dir.join(format!("{sid}.jsonl"));
            p.exists().then_some(p)
        } else {
            // No session file — fall back to the newest transcript started
            // after this session spawned.
            newest_jsonl(&project_dir, spawn_unix_ms)
        };
        if path != self.transcript {
            self.transcript = path;
            self.offset = 0;
            self.totals = Usage::default();
            self.last_usage = None;
            self.title = None;
            // Transcript-derived values must not survive a transcript switch
            // (e.g. the cwd fallback matched a foreign session before the
            // pid-based session file appeared).
            self.model = None;
            self.permission_mode = None;
        }
    }

    /// Path of the resolved transcript JSONL, if one has been found.
    pub fn transcript_path(&self) -> Option<&Path> {
        self.transcript.as_deref()
    }

    /// The per-session tool-activity ring (oldest at front, newest at back),
    /// bounded at `ACTIVITY_CAP`. The single source of truth for the activity
    /// timeline; the TUI reads it directly and the daemon serves it from the
    /// `ClaudeMeta` it already holds.
    pub fn activity(&self) -> &VecDeque<HookEvent> {
        &self.activity
    }

    /// Incrementally parse new transcript lines since the last poll.
    fn read_transcript_tail(&mut self) {
        let Some(path) = &self.transcript else {
            return;
        };
        let Ok(mut f) = fs::File::open(path) else {
            return;
        };
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        if len <= self.offset {
            return;
        }
        if f.seek(SeekFrom::Start(self.offset)).is_err() {
            return;
        }
        let mut buf = String::new();
        if f.read_to_string(&mut buf).is_err() {
            return;
        }
        // Only consume complete lines; a partial trailing line is re-read
        // on the next poll.
        let consumed = match buf.rfind('\n') {
            Some(i) => i + 1,
            None => return,
        };
        for line in buf[..consumed].lines() {
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if let Some(mode) = v["permissionMode"].as_str() {
                self.permission_mode = Some(mode.to_string());
            }
            if v["type"].as_str() == Some("ai-title") {
                if let Some(t) = v["aiTitle"].as_str() {
                    self.title = Some(t.to_string());
                }
            }
            if v["type"].as_str() == Some("assistant") {
                let msg = &v["message"];
                if let Some(model) = msg["model"].as_str() {
                    self.model = Some(model.to_string());
                }
                let u = &msg["usage"];
                if u.is_object() {
                    let usage = Usage {
                        input: u["input_tokens"].as_u64().unwrap_or(0),
                        output: u["output_tokens"].as_u64().unwrap_or(0),
                        cache_read: u["cache_read_input_tokens"].as_u64().unwrap_or(0),
                        cache_create: u["cache_creation_input_tokens"].as_u64().unwrap_or(0),
                    };
                    self.totals.input += usage.input;
                    self.totals.output += usage.output;
                    self.totals.cache_read += usage.cache_read;
                    self.totals.cache_create += usage.cache_create;
                    self.last_usage = Some(usage);
                }
            }
        }
        self.offset += consumed as u64;
    }

    /// Usage bridge file written by `baude statusline`:
    /// /tmp/baude-usage-<sessionId>.json — session cost, context %, and the
    /// account rate-limit windows (the only local source for those).
    fn read_bridge_file(&mut self) {
        let Some(sid) = &self.session_id else {
            return;
        };
        let Some(v) = read_json(&PathBuf::from(crate::bridge::bridge_path(sid))) else {
            return;
        };
        if let Some(cost) = v["cost_usd"].as_f64() {
            self.session_cost_usd = Some(cost);
        }
        if let Some(pct) = v["context_used_pct"].as_f64() {
            self.context_used_pct = Some((pct.round() as u64).min(100) as u8);
        }
        self.rate_updated_unix_ms = v["updated_unix_ms"].as_u64().unwrap_or(0);
        let window = |w: &Value| -> Option<RateWindow> {
            w.is_object().then(|| RateWindow {
                used_pct: w["used_pct"].as_f64(),
                resets_at_unix_s: w["resets_at"].as_u64(),
            })
        };
        if let Some(w) = window(&v["five_hour"]) {
            self.rate_5h = Some(w);
        }
        if let Some(w) = window(&v["seven_day"]) {
            self.rate_week = Some(w);
        }
        // STL-02 reader half: additive fields. Read every field optionally —
        // never branch on `schema`. Type mismatches yield None (no panic).
        //
        // These bridge-derived fields are assigned UNCONDITIONALLY: the bridge
        // is the source of truth for session state that comes and goes (a PR
        // merges, effort/vim toggle off). An absent/null/wrong-type value
        // yields None and clears the field, so the overlay never shows stale
        // state after Claude stops emitting it (WR-01).
        self.effort = v["effort"].as_str().map(str::to_string);
        self.thinking = v["thinking"].as_bool();
        self.vim_mode = v["vim_mode"].as_str().map(str::to_string);
        // Model precedence is the deliberate exception: the bridge value wins
        // when present, but the guard preserves the transcript-derived model
        // when the bridge omits it (poll() reads the transcript before the
        // bridge — load-bearing). Do NOT make this unconditional.
        if let Some(m) = v["model"].as_str() {
            self.model = Some(m.to_string());
        }
        let p = &v["pr"];
        self.pr = p.is_object().then(|| PrInfo {
            number: p["number"].as_u64(),
            url: p["url"].as_str().map(str::to_string),
            review_state: p["review_state"].as_str().map(str::to_string),
        });
        let w = &v["worktree"];
        self.worktree = w.is_object().then(|| WorktreeInfo {
            name: w["name"].as_str().map(str::to_string),
            path: w["path"].as_str().map(str::to_string),
            branch: w["branch"].as_str().map(str::to_string),
        });
    }

    /// Incrementally parse new hook-event lines since the last poll. Mirrors
    /// `read_transcript_tail`'s offset-tracked, complete-lines-only machinery
    /// but tracks a separate `offset_events` so the transcript tail is never
    /// disturbed. Events drive `hook_status`/`last_tool`/`last_notification`.
    /// Untyped `Value` accessors throughout — a malformed line is skipped and
    /// never panics (T-02-05).
    fn read_event_tail(&mut self) {
        let Some(sid) = &self.session_id else {
            return;
        };
        let path = PathBuf::from(crate::hook::event_path(sid));
        // The session_id can rotate mid-session (a resumed/rotated Claude
        // session), flipping the resolved event-file path. Reset the offset and
        // the event-derived state when that happens so a stale (possibly larger)
        // offset cannot permanently short-circuit the new file's events (WR-03).
        // Mirrors the transcript tail's reset in `resolve_transcript`.
        if self.event_path.as_ref() != Some(&path) {
            self.event_path = Some(path.clone());
            self.offset_events = 0;
            self.hook_status = None;
            self.last_tool = None;
            self.last_notification = None;
            self.activity.clear();
        }
        let Ok(mut f) = fs::File::open(&path) else {
            return;
        };
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        if len <= self.offset_events {
            return;
        }
        if f.seek(SeekFrom::Start(self.offset_events)).is_err() {
            return;
        }
        let mut buf = String::new();
        if f.read_to_string(&mut buf).is_err() {
            return;
        }
        // Only consume complete lines; a partial trailing line is re-read on
        // the next poll.
        let consumed = match buf.rfind('\n') {
            Some(i) => i + 1,
            None => return,
        };
        for line in buf[..consumed].lines() {
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let ts = v["ts"].as_u64().unwrap_or(0);
            match v["event"].as_str() {
                Some("UserPromptSubmit") => self.hook_status = Some((true, ts)),
                Some("Stop") => self.hook_status = Some((false, ts)),
                Some("Notification") => {
                    self.hook_status = Some((false, ts));
                    if let Some(nt) = v["notification_type"].as_str() {
                        self.last_notification = Some((nt.to_string(), ts));
                    }
                }
                Some("PostToolUse") => {
                    self.hook_status = Some((true, ts));
                    if let Some(t) = v["tool"].as_str() {
                        self.last_tool = Some((t.to_string(), ts));
                    }
                }
                _ => {}
            }
            // Append to the activity ring from the SAME parsed line. An
            // event-less line is skipped (same guard the match keys on); the
            // ring is bounded drop-oldest at `ACTIVITY_CAP` (T-03-01).
            if let Some(ev) = v["event"].as_str() {
                self.activity.push_back(HookEvent {
                    event: ev.to_string(),
                    tool: v["tool"].as_str().map(str::to_string),
                    notification_type: v["notification_type"].as_str().map(str::to_string),
                    ts,
                });
                if self.activity.len() > ACTIVITY_CAP {
                    self.activity.pop_front();
                }
            }
        }
        self.offset_events += consumed as u64;
    }

    /// Context usage bridge file written by statusline hooks (e.g. the GSD
    /// statusline): /tmp/claude-ctx-<sessionId>.json.
    fn read_context_file(&mut self) {
        let Some(sid) = &self.session_id else {
            return;
        };
        let Some(v) = read_json(&PathBuf::from(format!("/tmp/claude-ctx-{sid}.json"))) else {
            return;
        };
        if let Some(used) = v["used_pct"].as_u64() {
            self.context_used_pct = Some(used.min(100) as u8);
        } else if let Some(rem) = v["remaining_percentage"].as_u64() {
            self.context_used_pct = Some((100 - rem.min(100)) as u8);
        }
    }
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

fn newest_jsonl(dir: &Path, since_unix_ms: u64) -> Option<PathBuf> {
    let mut best: Option<(u64, PathBuf)> = None;
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e != "jsonl").unwrap_or(true) {
            continue;
        }
        let mtime = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        if mtime + 10_000 < since_unix_ms {
            continue;
        }
        if best.as_ref().map(|(m, _)| mtime > *m).unwrap_or(true) {
            best = Some((mtime, path));
        }
    }
    best.map(|(_, p)| p)
}

/// Parse GSD state from `.planning/STATE.md` frontmatter + body.
pub fn parse_gsd(repo_root: &Path) -> Option<GsdState> {
    let text = fs::read_to_string(repo_root.join(".planning").join("STATE.md")).ok()?;
    let mut g = GsdState::default();
    let unquote = |s: &str| s.trim().trim_matches(['"', '\'']).to_string();
    let mut in_frontmatter = false;
    for (i, line) in text.lines().enumerate() {
        let t = line.trim();
        if t == "---" {
            if i == 0 {
                in_frontmatter = true;
                continue;
            }
            in_frontmatter = false;
            continue;
        }
        if in_frontmatter {
            if let Some(v) = t.strip_prefix("milestone:") {
                g.milestone = Some(unquote(v));
            } else if let Some(v) = t.strip_prefix("status:") {
                g.status = Some(unquote(v));
            } else if let Some(v) = t.strip_prefix("active_phase:") {
                g.active_phase = Some(unquote(v));
            } else if let Some(v) = t.strip_prefix("next_action:") {
                g.next_action = Some(unquote(v));
            } else if let Some(v) = t.strip_prefix("percent:") {
                g.percent = v.trim().parse().ok();
            }
        } else if t.starts_with("Phase:") || t.starts_with("Progress:") {
            let existing = g.phase_line.take();
            g.phase_line = Some(match existing {
                Some(e) => format!("{e} · {t}"),
                None => t.to_string(),
            });
        }
    }
    Some(g)
}

/// Current branch from .git/HEAD without spawning git. Handles worktrees,
/// where `.git` is a file pointing at the real gitdir.
fn read_git_branch(cwd: &Path) -> Option<String> {
    let mut dir = cwd.to_path_buf();
    let head = loop {
        let dotgit = dir.join(".git");
        if dotgit.is_dir() {
            break dotgit.join("HEAD");
        }
        if dotgit.is_file() {
            // worktree: "gitdir: /path/to/main/.git/worktrees/<name>"
            let text = fs::read_to_string(&dotgit).ok()?;
            let gitdir = text.strip_prefix("gitdir:")?.trim();
            break PathBuf::from(gitdir).join("HEAD");
        }
        if !dir.pop() {
            return None;
        }
    };
    let head = fs::read_to_string(head).ok()?;
    head.trim()
        .strip_prefix("ref: refs/heads/")
        .map(|b| b.to_string())
        .or(Some("detached".into()))
}

/// "in 46m" / "in 3d 4h" until a unix-seconds timestamp.
pub fn human_until(unix_s: u64) -> String {
    let now_s = now_unix_ms() / 1000;
    let secs = unix_s.saturating_sub(now_s);
    if secs == 0 {
        return "now".into();
    }
    let (d, h, m) = (secs / 86_400, (secs % 86_400) / 3600, (secs % 3600) / 60);
    if d > 0 {
        format!("in {d}d {h}h")
    } else if h > 0 {
        format!("in {h}h {m}m")
    } else {
        format!("in {}m", m.max(1))
    }
}

pub fn human_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Short display form of a model id: "claude-fable-5" → "fable-5".
pub fn short_model(model: &str) -> String {
    model.strip_prefix("claude-").unwrap_or(model).to_string()
}

/// Short display form of a permission mode.
pub fn short_mode(mode: &str) -> &'static str {
    match mode {
        "bypassPermissions" => "bypass",
        "acceptEdits" => "accept",
        "plan" => "plan",
        "default" => "ask",
        _ => "?",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Write a bridge file at the path `read_bridge_file` will look for, build
    /// a `ClaudeMeta` pointed at that session id, run the (private) reader, and
    /// return the populated meta. The fixture path is pid+counter-keyed so
    /// parallel tests never collide. Cleans up the temp file before returning.
    fn read_fixture(suffix: &str, json: &str) -> ClaudeMeta {
        let sid = format!("test-{}-{}", std::process::id(), suffix);
        let path = crate::bridge::bridge_path(&sid);
        fs::write(&path, json).unwrap();
        let mut meta = ClaudeMeta {
            session_id: Some(sid),
            ..Default::default()
        };
        meta.read_bridge_file();
        fs::remove_file(&path).ok();
        meta
    }

    #[test]
    fn reads_v2_bridge() {
        let meta = read_fixture(
            "v2",
            r#"{
                "schema": 2,
                "cost_usd": 1.25,
                "context_used_pct": 42.0,
                "updated_unix_ms": 12345,
                "five_hour": {"used_pct": 10.0, "resets_at": 111},
                "seven_day": {"used_pct": 20.0, "resets_at": 222},
                "model": "Claude Opus 4.8",
                "effort": "high",
                "thinking": true,
                "vim_mode": "NORMAL",
                "pr": {"number": 42, "url": "https://example.com/pr/42", "review_state": "approved"},
                "worktree": {"name": "wt", "path": "/tmp/wt", "branch": "feature/x"}
            }"#,
        );

        // new fields populated
        assert_eq!(meta.effort.as_deref(), Some("high"));
        assert_eq!(meta.thinking, Some(true));
        assert_eq!(meta.vim_mode.as_deref(), Some("NORMAL"));
        assert_eq!(meta.model.as_deref(), Some("Claude Opus 4.8"));

        let pr = meta.pr.as_ref().expect("pr present");
        assert_eq!(pr.number, Some(42));
        assert_eq!(pr.url.as_deref(), Some("https://example.com/pr/42"));
        assert_eq!(pr.review_state.as_deref(), Some("approved"));

        let wt = meta.worktree.as_ref().expect("worktree present");
        assert_eq!(wt.name.as_deref(), Some("wt"));
        assert_eq!(wt.path.as_deref(), Some("/tmp/wt"));
        assert_eq!(wt.branch.as_deref(), Some("feature/x"));

        // legacy fields still read
        assert_eq!(meta.session_cost_usd, Some(1.25));
        assert_eq!(meta.context_used_pct, Some(42));
        assert_eq!(meta.rate_updated_unix_ms, 12345);
        assert_eq!(meta.rate_5h.unwrap().used_pct, Some(10.0));
        assert_eq!(meta.rate_week.unwrap().resets_at_unix_s, Some(222));
    }

    #[test]
    fn reads_legacy_bridge() {
        // schema-absent file with ONLY the four legacy fields.
        let meta = read_fixture(
            "legacy",
            r#"{
                "cost_usd": 0.5,
                "context_used_pct": 12.0,
                "updated_unix_ms": 99,
                "five_hour": {"used_pct": 5.0, "resets_at": 1}
            }"#,
        );

        // legacy fields present
        assert_eq!(meta.session_cost_usd, Some(0.5));
        assert_eq!(meta.context_used_pct, Some(12));
        assert_eq!(meta.rate_5h.unwrap().used_pct, Some(5.0));

        // new fields all None (additive back-compat, new-reader-reads-old-file)
        assert!(meta.effort.is_none());
        assert!(meta.thinking.is_none());
        assert!(meta.vim_mode.is_none());
        assert!(meta.pr.is_none());
        assert!(meta.worktree.is_none());
    }

    #[test]
    fn does_not_branch_on_schema() {
        // schema:99 with new fields present → still read (no `if schema == 2`).
        let meta = read_fixture(
            "schema99",
            r#"{
                "schema": 99,
                "effort": "low",
                "thinking": false,
                "vim_mode": "INSERT",
                "pr": {"number": 1}
            }"#,
        );
        assert_eq!(meta.effort.as_deref(), Some("low"));
        assert_eq!(meta.thinking, Some(false));
        assert_eq!(meta.vim_mode.as_deref(), Some("INSERT"));
        assert_eq!(meta.pr.as_ref().unwrap().number, Some(1));
    }

    #[test]
    fn pr_absent_is_none() {
        let meta = read_fixture("nopr", r#"{"schema": 2, "effort": "medium"}"#);
        assert!(meta.pr.is_none());
        assert!(meta.worktree.is_none());
    }

    #[test]
    fn pr_present_review_state_absent() {
        // pr exists but review_state absent → pr Some, review_state None.
        let meta = read_fixture(
            "pr-partial",
            r#"{"schema": 2, "pr": {"number": 7, "url": "u"}}"#,
        );
        let pr = meta.pr.as_ref().expect("pr present");
        assert_eq!(pr.number, Some(7));
        assert_eq!(pr.url.as_deref(), Some("u"));
        assert!(pr.review_state.is_none());
    }

    #[test]
    fn present_then_absent_bridge_fields_clear() {
        // A bridge that emits pr/effort/thinking/vim_mode, then a later write
        // that drops them, must CLEAR the captured state (WR-01). The classic
        // case: a PR merges, the bridge writes `"pr": null`, and the overlay
        // must stop showing the now-closed PR.
        let sid = format!("test-{}-clear", std::process::id());
        let path = crate::bridge::bridge_path(&sid);
        let mut meta = ClaudeMeta {
            session_id: Some(sid),
            ..Default::default()
        };

        // First tick: everything present.
        fs::write(
            &path,
            r#"{
                "schema": 2,
                "effort": "high",
                "thinking": true,
                "vim_mode": "NORMAL",
                "pr": {"number": 42},
                "worktree": {"name": "wt"}
            }"#,
        )
        .unwrap();
        meta.read_bridge_file();
        assert_eq!(meta.effort.as_deref(), Some("high"));
        assert_eq!(meta.thinking, Some(true));
        assert_eq!(meta.vim_mode.as_deref(), Some("NORMAL"));
        assert_eq!(meta.pr.as_ref().unwrap().number, Some(42));
        assert_eq!(meta.worktree.as_ref().unwrap().name.as_deref(), Some("wt"));

        // Second tick: fields gone (pr explicitly null, others absent).
        fs::write(&path, r#"{"schema": 2, "pr": null}"#).unwrap();
        meta.read_bridge_file();
        fs::remove_file(&path).ok();

        assert!(meta.effort.is_none(), "stale effort lingered");
        assert!(meta.thinking.is_none(), "stale thinking lingered");
        assert!(meta.vim_mode.is_none(), "stale vim_mode lingered");
        assert!(meta.pr.is_none(), "stale pr lingered after null");
        assert!(meta.worktree.is_none(), "stale worktree lingered");
    }

    /// Append JSONL records to a transcript fixture and point `meta` at it,
    /// then drive the real incremental reader. Mirrors how `poll()` feeds
    /// `read_transcript_tail` (offset-tracked tail reads), so tests exercise
    /// the same seam production does. Returns the temp path for cleanup.
    fn feed_transcript(meta: &mut ClaudeMeta, suffix: &str, lines: &[&str]) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "baude-test-transcript-{}-{}.jsonl",
            std::process::id(),
            suffix
        ));
        let mut body = String::new();
        for line in lines {
            body.push_str(line);
            body.push('\n');
        }
        fs::write(&path, body).unwrap();
        meta.transcript = Some(path.clone());
        meta.read_transcript_tail();
        path
    }

    #[test]
    fn model_bridge_wins_then_survives() {
        // Drive the model through the REAL transcript path (read_transcript_tail),
        // the way poll() does, so this guards the load-bearing ordering claim —
        // not just the read_bridge_file accessor (WR-02).

        // Case A: transcript sets the model, then the bridge emits its own —
        // bridge wins (poll order: transcript, then bridge).
        let mut meta = ClaudeMeta::default();
        let tpath = feed_transcript(
            &mut meta,
            "model-wins",
            &[r#"{"type":"assistant","message":{"model":"from-transcript"}}"#],
        );
        assert_eq!(
            meta.model.as_deref(),
            Some("from-transcript"),
            "transcript model not read through the real seam"
        );
        let sid = format!("test-{}-model-wins", std::process::id());
        let bpath = crate::bridge::bridge_path(&sid);
        fs::write(&bpath, r#"{"schema": 2, "model": "from-bridge"}"#).unwrap();
        meta.session_id = Some(sid);
        meta.read_bridge_file();
        fs::remove_file(&bpath).ok();
        fs::remove_file(&tpath).ok();
        assert_eq!(meta.model.as_deref(), Some("from-bridge"));

        // Case B: transcript sets the model, the bridge omits it —
        // the transcript value survives (the model guard prevents clobber).
        let mut meta2 = ClaudeMeta::default();
        let tpath2 = feed_transcript(
            &mut meta2,
            "model-survives",
            &[r#"{"type":"assistant","message":{"model":"keep-me"}}"#],
        );
        assert_eq!(meta2.model.as_deref(), Some("keep-me"));
        let sid2 = format!("test-{}-model-survives", std::process::id());
        let bpath2 = crate::bridge::bridge_path(&sid2);
        fs::write(&bpath2, r#"{"schema": 2, "effort": "high"}"#).unwrap();
        meta2.session_id = Some(sid2);
        meta2.read_bridge_file();
        fs::remove_file(&bpath2).ok();
        fs::remove_file(&tpath2).ok();
        assert_eq!(meta2.model.as_deref(), Some("keep-me"));
    }

    /// Write event lines to the path `read_event_tail` will look for (keyed by
    /// `meta.session_id`), then drive the REAL incremental reader the way
    /// `poll()` does — mirroring the `model_bridge_wins_then_survives`
    /// real-seam rigor. Appends (does not truncate) so multi-tick offset tests
    /// accumulate. Returns the event-file path for cleanup.
    fn feed_events(meta: &mut ClaudeMeta, suffix: &str, lines: &[&str]) -> PathBuf {
        let sid = format!("test-{}-{}", std::process::id(), suffix);
        let path = PathBuf::from(crate::hook::event_path(&sid));
        let mut body = String::new();
        for line in lines {
            body.push_str(line);
            body.push('\n');
        }
        // First feed (meta has no session yet) truncates any stale file from
        // a prior run; subsequent feeds append to extend the same file so the
        // offset semantics are exercised.
        if meta.session_id.is_none() {
            fs::write(&path, &body).unwrap();
        } else {
            let existing = fs::read_to_string(&path).unwrap_or_default();
            fs::write(&path, format!("{existing}{body}")).unwrap();
        }
        meta.session_id = Some(sid);
        meta.read_event_tail();
        path
    }

    #[test]
    fn event_tail_drives_state() {
        // UserPromptSubmit -> Busy; Stop -> Waiting; through the real seam.
        let mut meta = ClaudeMeta::default();
        let path = feed_events(
            &mut meta,
            "drives-state",
            &[r#"{"schema":1,"event":"UserPromptSubmit","ts":100}"#],
        );
        assert_eq!(meta.hook_status, Some((true, 100)));

        // Same session id (deterministic from suffix) — feed appends.
        feed_events(
            &mut meta,
            "drives-state",
            &[r#"{"schema":1,"event":"Stop","ts":200}"#],
        );
        assert_eq!(meta.hook_status, Some((false, 200)));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn notification_permission_captured() {
        // Notification -> Waiting AND notification_type captured in meta
        // (last_notification), not exposed as a structured field.
        let mut meta = ClaudeMeta::default();
        let path = feed_events(
            &mut meta,
            "notif",
            &[
                r#"{"schema":1,"event":"Notification","ts":250,"notification_type":"permission_prompt"}"#,
            ],
        );
        assert_eq!(meta.hook_status, Some((false, 250)));
        assert_eq!(
            meta.last_notification,
            Some(("permission_prompt".to_string(), 250))
        );
        fs::remove_file(&path).ok();
    }

    #[test]
    fn post_tool_use_records_last_tool() {
        // PostToolUse -> stays Busy AND records the tool name + ts.
        let mut meta = ClaudeMeta::default();
        let path = feed_events(
            &mut meta,
            "posttool",
            &[r#"{"schema":1,"event":"PostToolUse","ts":300,"tool":"Bash"}"#],
        );
        assert_eq!(meta.hook_status, Some((true, 300)));
        assert_eq!(meta.last_tool, Some(("Bash".to_string(), 300)));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn event_tail_offset_does_not_reprocess() {
        // A second read over an UNCHANGED file does not re-process lines: the
        // Stop at ts=200 must not be clobbered back to the earlier prompt.
        let mut meta = ClaudeMeta::default();
        let path = feed_events(
            &mut meta,
            "offset",
            &[
                r#"{"schema":1,"event":"UserPromptSubmit","ts":100}"#,
                r#"{"schema":1,"event":"Stop","ts":200}"#,
            ],
        );
        assert_eq!(meta.hook_status, Some((false, 200)));
        let off_after_first = meta.offset_events;
        // Re-read the same file (no new lines) — no change.
        meta.read_event_tail();
        assert_eq!(meta.hook_status, Some((false, 200)));
        assert_eq!(meta.offset_events, off_after_first);
        // The transcript offset is untouched by the event tail.
        assert_eq!(meta.offset, 0);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn event_tail_defers_partial_line() {
        // A trailing line without a newline is NOT consumed until completed.
        let mut meta = ClaudeMeta::default();
        let sid = format!("test-{}-partial", std::process::id());
        let path = PathBuf::from(crate::hook::event_path(&sid));
        meta.session_id = Some(sid);

        // One complete line + a partial (no newline) trailing line.
        fs::write(
            &path,
            "{\"schema\":1,\"event\":\"Stop\",\"ts\":10}\n{\"schema\":1,\"event\":\"UserPromptSubmit\",\"ts\":20",
        )
        .unwrap();
        meta.read_event_tail();
        // Only the complete Stop line consumed.
        assert_eq!(meta.hook_status, Some((false, 10)));

        // Complete the partial line.
        fs::write(
            &path,
            "{\"schema\":1,\"event\":\"Stop\",\"ts\":10}\n{\"schema\":1,\"event\":\"UserPromptSubmit\",\"ts\":20}\n",
        )
        .unwrap();
        meta.read_event_tail();
        assert_eq!(meta.hook_status, Some((true, 20)));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn event_tail_skips_malformed_line() {
        // A malformed JSON line is skipped without panicking; the surrounding
        // valid lines still drive state.
        let mut meta = ClaudeMeta::default();
        let path = feed_events(
            &mut meta,
            "malformed",
            &[
                r#"{"schema":1,"event":"UserPromptSubmit","ts":100}"#,
                "{ this is not json",
                r#"{"schema":1,"event":"Stop","ts":300}"#,
            ],
        );
        assert_eq!(meta.hook_status, Some((false, 300)));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn event_tail_resets_offset_on_session_id_change() {
        // WR-03: when the session_id rotates mid-session the event-file path
        // flips. A stale (here larger) offset must NOT short-circuit the new,
        // shorter file forever — read_event_tail resets the offset and clears
        // event-derived state when the path changes.

        // First session: a longer file that advances the offset well past the
        // length of the second session's file.
        let sid_a = format!("test-{}-reset-a", std::process::id());
        let path_a = PathBuf::from(crate::hook::event_path(&sid_a));
        let mut meta = ClaudeMeta {
            session_id: Some(sid_a),
            ..Default::default()
        };
        let mut body_a = String::new();
        for ts in 0..50 {
            body_a.push_str(&format!(
                r#"{{"schema":1,"event":"PostToolUse","ts":{ts},"tool":"Bash"}}"#
            ));
            body_a.push('\n');
        }
        fs::write(&path_a, &body_a).unwrap();
        meta.read_event_tail();
        assert!(meta.offset_events > 0, "first session advanced the offset");
        assert!(meta.last_tool.is_some());
        let off_a = meta.offset_events;

        // Rotate to a NEW (deterministically distinct, shorter) session id. Its
        // file is far smaller than off_a — without the reset, `len <= offset`
        // would skip it forever.
        let sid_b = format!("test-{}-reset-b", std::process::id());
        let path_b = PathBuf::from(crate::hook::event_path(&sid_b));
        fs::write(
            &path_b,
            "{\"schema\":1,\"event\":\"UserPromptSubmit\",\"ts\":900}\n",
        )
        .unwrap();
        assert!(
            fs::metadata(&path_b).unwrap().len() < off_a,
            "second file must be shorter than the stale offset to prove the bug"
        );
        meta.session_id = Some(sid_b);
        meta.read_event_tail();

        // The new session's event was processed (offset was reset), not dropped.
        assert_eq!(
            meta.hook_status,
            Some((true, 900)),
            "rotated session's events must be read after the offset reset"
        );

        fs::remove_file(&path_a).ok();
        fs::remove_file(&path_b).ok();
    }

    #[test]
    fn activity_ring_caps_drop_oldest() {
        // Feed N = ACTIVITY_CAP + 5 PostToolUse lines; the ring caps at
        // ACTIVITY_CAP, oldest dropped, newest at back.
        let n = ACTIVITY_CAP + 5;
        let mut lines = Vec::with_capacity(n);
        for i in 0..n {
            lines.push(format!(
                r#"{{"schema":1,"event":"PostToolUse","ts":{i},"tool":"Bash"}}"#
            ));
        }
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let mut meta = ClaudeMeta::default();
        let path = feed_events(&mut meta, "ring-cap", &refs);

        assert_eq!(meta.activity().len(), ACTIVITY_CAP);
        // The oldest 5 (ts 0..5) were dropped; front is ts == 5.
        assert_eq!(meta.activity().front().unwrap().ts, 5);
        // Newest is at the back.
        assert_eq!(meta.activity().back().unwrap().ts, (n - 1) as u64);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn activity_ring_records_each_event_in_order() {
        // A mix of event kinds lands one HookEvent per line in arrival order,
        // with tool/notification_type populated only on the relevant kinds.
        let mut meta = ClaudeMeta::default();
        let path = feed_events(
            &mut meta,
            "ring-mix",
            &[
                r#"{"schema":1,"event":"UserPromptSubmit","ts":10}"#,
                r#"{"schema":1,"event":"PostToolUse","ts":20,"tool":"Bash"}"#,
                r#"{"schema":1,"event":"Stop","ts":30}"#,
                r#"{"schema":1,"event":"Notification","ts":40,"notification_type":"permission"}"#,
            ],
        );

        let act = meta.activity();
        assert_eq!(act.len(), 4);

        assert_eq!(act[0].event, "UserPromptSubmit");
        assert_eq!(act[0].tool, None);
        assert_eq!(act[0].notification_type, None);

        assert_eq!(act[1].event, "PostToolUse");
        assert_eq!(act[1].tool.as_deref(), Some("Bash"));
        assert_eq!(act[1].notification_type, None);

        assert_eq!(act[2].event, "Stop");
        assert_eq!(act[2].tool, None);

        assert_eq!(act[3].event, "Notification");
        assert_eq!(act[3].tool, None);
        assert_eq!(act[3].notification_type.as_deref(), Some("permission"));

        fs::remove_file(&path).ok();
    }

    #[test]
    fn activity_ring_clears_on_path_rotation() {
        // Feed session A, then rotate the event path to session B; the ring is
        // cleared at rotation and reflects only B's events (mirrors WR-03).
        let mut meta = ClaudeMeta::default();
        let path_a = feed_events(
            &mut meta,
            "ring-rot-a",
            &[
                r#"{"schema":1,"event":"PostToolUse","ts":100,"tool":"Bash"}"#,
                r#"{"schema":1,"event":"PostToolUse","ts":200,"tool":"Edit"}"#,
            ],
        );
        assert_eq!(meta.activity().len(), 2);

        // Rotate to a NEW, deterministically distinct session id.
        let path_b = feed_events(
            &mut meta,
            "ring-rot-b",
            &[r#"{"schema":1,"event":"UserPromptSubmit","ts":900}"#],
        );

        let act = meta.activity();
        assert_eq!(act.len(), 1, "ring must be cleared on rotation");
        assert_eq!(act[0].event, "UserPromptSubmit");
        assert_eq!(act[0].ts, 900);

        fs::remove_file(&path_a).ok();
        fs::remove_file(&path_b).ok();
    }

    #[test]
    fn activity_ring_skips_malformed_lines() {
        // A non-JSON line and a line missing `event` are both skipped — they do
        // not push into the ring and do not panic.
        let mut meta = ClaudeMeta::default();
        let path = feed_events(
            &mut meta,
            "ring-malformed",
            &[
                r#"{"schema":1,"event":"PostToolUse","ts":10,"tool":"Bash"}"#,
                r#"not json at all"#,
                r#"{"schema":1,"ts":20}"#, // missing `event`
                r#"{"schema":1,"event":"Stop","ts":30}"#,
            ],
        );

        let act = meta.activity();
        assert_eq!(act.len(), 2, "only the two well-formed events are retained");
        assert_eq!(act[0].event, "PostToolUse");
        assert_eq!(act[1].event, "Stop");

        fs::remove_file(&path).ok();
    }

    #[test]
    fn hook_event_serializes_post_tool_use() {
        // PostToolUse: tool present, notification_type omitted (skip_serializing_if).
        let ev = HookEvent {
            event: "PostToolUse".to_string(),
            tool: Some("Bash".to_string()),
            notification_type: None,
            ts: 1_700_000_000_000,
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains(r#""event":"PostToolUse""#), "json: {json}");
        assert!(json.contains(r#""tool":"Bash""#), "json: {json}");
        assert!(json.contains(r#""ts":1700000000000"#), "json: {json}");
        assert!(
            !json.contains("notification_type"),
            "None notification_type must be omitted: {json}"
        );
    }

    #[test]
    fn hook_event_serializes_notification() {
        // Notification: notification_type present, tool omitted.
        let ev = HookEvent {
            event: "Notification".to_string(),
            tool: None,
            notification_type: Some("permission".to_string()),
            ts: 1_700_000_000_001,
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains(r#""event":"Notification""#), "json: {json}");
        assert!(
            json.contains(r#""notification_type":"permission""#),
            "json: {json}"
        );
        assert!(
            !json.contains(r#""tool""#),
            "None tool must be omitted: {json}"
        );
    }

    #[test]
    fn hook_event_round_trips() {
        // Serialize then deserialize; all four fields preserved.
        let ev = HookEvent {
            event: "PostToolUse".to_string(),
            tool: Some("Edit".to_string()),
            notification_type: None,
            ts: 42,
        };
        let json = serde_json::to_string(&ev).unwrap();
        let back: HookEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back.event, ev.event);
        assert_eq!(back.tool, ev.tool);
        assert_eq!(back.notification_type, ev.notification_type);
        assert_eq!(back.ts, ev.ts);
    }
}
