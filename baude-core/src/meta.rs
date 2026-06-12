//! Claude Code session metadata, read from the artifacts Claude writes to
//! disk: `$CLAUDE_CONFIG_DIR/sessions/<pid>.json` (live busy/idle status),
//! the session transcript JSONL (model, token usage, permission mode),
//! `/tmp/claude-ctx-<sessionId>.json` (context %, written by statusline
//! hooks), and `.planning/STATE.md` (GSD project state).

use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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

#[derive(Default, Clone)]
pub struct GsdState {
    pub milestone: Option<String>,
    pub status: Option<String>,
    pub active_phase: Option<String>,
    pub next_action: Option<String>,
    pub percent: Option<u8>,
    pub phase_line: Option<String>,
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
