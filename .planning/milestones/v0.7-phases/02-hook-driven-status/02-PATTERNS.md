# Phase 2: Hook-Driven Status - Pattern Map

**Mapped:** 2026-06-15
**Files analyzed:** 7 (1 new, 6 modified)
**Analogs found:** 7 / 7

## Resolved Open Question (Research OQ#1)

There is **NO existing Rust settings-seeding code** in `app.rs` / `manager.rs`. The only statusLine seeding lives in **`docker-entrypoint.sh:4-11`** (shell, container-only, `printf`-writes a fresh `$CLAUDE_CONFIG_DIR/settings.json` if absent). The `statusLine` block documented in `bridge.rs:11-17` and `README.md:126-131` is documentation/manual-config, not a Rust write path.

**Implication for the planner:** The idempotent deep-merge `merge_hook_settings()` is genuinely new Rust code (no Rust analog to extend). The shell entrypoint's "never clobber an existing settings.json" *intent* is the precedent, but the Rust implementation must be built fresh, modeled structurally on `build_bridge` (pure `Value -> Value`). The settings file target also differs: entrypoint writes the global `settings.json`; this phase writes the per-session `.claude/settings.local.json` in each session cwd (a new write site at the two spawn integration points).

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `baude-core/src/hook.rs` (NEW) | core / transform | transform (Value→Value) + file-I/O (append) | `baude-core/src/bridge.rs` | exact (same crate, same pure-fn + path-helper + run-dispatch shape) |
| `baude-core/src/meta.rs` (MOD) | core / consumer | file-I/O (offset-tracked tail) | `meta.rs::read_transcript_tail` / `read_bridge_file` (self) | exact (extend existing pattern in place) |
| `baude-core/src/session.rs` (MOD) | core / state-derivation | request-response (sync compute) | `session.rs::status()` (self) | exact (prepend a branch to existing dual-source logic) |
| `baude/src/main.rs` (MOD) | binary / dispatch entrypoint | request-response (stdin→file/POST) | `main.rs:29-36` `statusline` dispatch arm | exact |
| `baude/src/app.rs` (MOD) | TUI / spawn integration | file-I/O (seed before spawn) | `app.rs::add_session` (self, 394-453) | exact (call site, no analog needed beyond the spawn block) |
| `bauded/src/api.rs` (MOD) | daemon / route | request-response (POST→204) | `api.rs::interrupt` / `post_keys` / `archive` (`Path<u64>`→StatusCode) | exact |
| `bauded/src/manager.rs` (MOD) | daemon / spawn + ingest + SessionInfo | event-driven (ingest) + file-I/O (seed + env inject) | `manager.rs::spawn` (231-234) + `SessionInfo`/`session_info` builder | exact |

## Pattern Assignments

### `baude-core/src/hook.rs` (NEW — core transform + append)

**Analog:** `baude-core/src/bridge.rs` (copy the file's entire structure: module doc with pinned CLI version, path helper, pure builder fn, `run`-style dispatch is in the binary not here, `mod tests` with pure-fn JSON fixtures).

**Path-helper pattern** (`bridge.rs:26-28`) — mirror exactly for `event_path(sid)`:
```rust
pub fn bridge_path(session_id: &str) -> String {
    format!("/tmp/baude-usage-{session_id}.json")
}
```
New file: `pub fn event_path(sid: &str) -> String { format!("/tmp/baude-events-{sid}.jsonl") }` (Security note from research: optionally reject `sid` containing `/` or `..` — defense-in-depth, low risk, mirrors existing `bridge_path` posture which does NOT currently guard).

**Pure builder pattern** (`bridge.rs:61-107`) — `build_event` mirrors `build_bridge`: untyped `Value` accessors throughout, `now_unix_ms()` timestamp, snake_case keys verified against CLI 2.1.177, `schema` informational only (readers must not branch). Note the snake/camel `.or_else` tolerance idiom at `bridge.rs:43-46,70,99`:
```rust
.as_str().or_else(|| p["reviewState"].as_str())
```

**Best-effort posture** (`bridge.rs:109-120`) — the model for never breaking Claude:
```rust
pub fn run(wrap: Option<String>) -> i32 {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        return 1;
    }
    // Best-effort capture — never break the user's statusline over it.
    if let Ok(v) = serde_json::from_str::<Value>(&input) {
        if let Some(sid) = v["session_id"].as_str() {
            let _ = std::fs::write(bridge_path(sid), build_bridge(&v).to_string());
        }
    }
```
For `baude hook` the equivalent best-effort write is an **O_APPEND** (concurrent hook processes), per research Pattern 2:
```rust
pub fn append_event(sid: &str, line: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true).append(true).open(event_path(sid))?;
    writeln!(f, "{line}")
}
```

**Test pattern** (`bridge.rs:141-252`) — pure-function JSON fixtures via a `parse(&str) -> Value` helper; assertions on `out["field"].as_str()`. Required Wave-0 tests (preserve user settings, idempotent merge, minimal-file no-panic, build_event mapping, append writes /tmp) map directly onto this shape. Note `never_panics_on_empty_object` (`bridge.rs:248-251`) is the precedent for the "minimal/odd file never panics" merge test.

---

### `baude-core/src/meta.rs` (MOD — offset-tracked event tail)

**Analog:** `read_transcript_tail` (`meta.rs:206-264`) — the offset-tracked incremental JSONL tail to copy verbatim for `read_event_tail`. Slots into `poll()` after `read_bridge_file()` (`poll()` body at `meta.rs:117-127`).

**Offset-tail skeleton** (`meta.rs:206-264`) — copy the open/seek/complete-lines-only/offset-advance machinery exactly:
```rust
fn read_transcript_tail(&mut self) {
    let Some(path) = &self.transcript else { return; };
    let Ok(mut f) = fs::File::open(path) else { return; };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    if len <= self.offset { return; }
    if f.seek(SeekFrom::Start(self.offset)).is_err() { return; }
    let mut buf = String::new();
    if f.read_to_string(&mut buf).is_err() { return; }
    // Only consume complete lines; a partial trailing line is re-read next poll.
    let consumed = match buf.rfind('\n') { Some(i) => i + 1, None => return };
    for line in buf[..consumed].lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        // ... per-line untyped Value handling ...
    }
    self.offset += consumed as u64;
}
```
For `read_event_tail`: add a **separate** `offset_events: u64` field (do NOT reuse `self.offset`, which tracks the transcript). The per-line body is the event→state match from research Code Examples (UserPromptSubmit→`hook_status=Some((true,ts))`, Stop/Notification→`(false,ts)`, PostToolUse→`(true,ts)` + `last_tool`).

**New ClaudeMeta fields** (add alongside `meta.rs:80-114`) — follow the existing `claude_status: Option<(bool, u64)>` shape (`meta.rs:90-91`):
```rust
/// (busy, event_ts unix ms) from the freshest hook event — highest precedence.
pub hook_status: Option<(bool, u64)>,
/// (tool_name, event_ts) from the last PostToolUse — captured, rendered minimally.
pub last_tool: Option<(String, u64)>,
```
Plus `offset_events: u64` (private, like `offset`).

**File-tail-by-sid analog** (`read_bridge_file`, `meta.rs:269-275`) — the guard idiom for "need session_id, build /tmp path, bail if absent":
```rust
fn read_bridge_file(&mut self) {
    let Some(sid) = &self.session_id else { return; };
    let Some(v) = read_json(&PathBuf::from(crate::bridge::bridge_path(sid))) else { return; };
```
`read_event_tail` resolves its path the same way: `crate::hook::event_path(sid)`.

**WR-01 clear-when-absent discipline** (`meta.rs:298-324`) — the hard-won Phase 1 lesson the research flagged. Bridge-derived fields that "come and go" are assigned unconditionally so stale state never sticks. `hook_status`/`last_tool` are event-driven (only ever set on a new event, never cleared by absence), so they do NOT need the unconditional-clear treatment — but a **staleness threshold** in `status()` (below) plays the equivalent role: a long-dead hook event must not pin a wrong state forever (research Pitfall 5).

**Test helper to reuse** (`meta.rs:659-674`, `feed_transcript`) — clone it as `feed_events(meta, suffix, &[lines])` writing `/tmp/baude-test-events-<pid>-<suffix>.jsonl`, set `meta.session_id`, call `read_event_tail`, return path for cleanup. The `model_bridge_wins_then_survives` test (`meta.rs:676-694`) is the precedent for driving state through the REAL seam (not just the accessor) — apply the same rigor to `event_tail_drives_state` and `notification_permission_captured`.

---

### `baude-core/src/session.rs` (MOD — precedence + StateSource)

**Analog:** `Session::status()` (`session.rs:107-123`) — prepend a hook branch ahead of the existing `claude_status` branch; leave the silence fallback (`session.rs:117-122`) **byte-identical** (research Pitfall 3).

**Current status() to extend** (`session.rs:108-123`):
```rust
pub fn status(&self) -> Status {
    if self.claude.is_exited() {
        return Status::Exited;
    }
    // Claude's own session file is authoritative when we found it;
    // otherwise fall back to the output-silence heuristic.
    if let Some((busy, _)) = self.meta.claude_status {
        return if busy { Status::Busy } else { Status::Waiting };
    }
    let last = self.claude.last_output_ms.load(Ordering::Relaxed);
    if now_ms().saturating_sub(last) < BUSY_WINDOW_MS {
        Status::Busy
    } else {
        Status::Waiting
    }
}
```

**Required additive shape** (research Pattern 3) — add `StateSource` enum + `status_with_source()`, keep `status()` total by delegating to `.0`:
```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StateSource { Hook, SessionFile, Silence }

pub fn status_with_source(&self) -> (Status, StateSource) {
    if self.claude.is_exited() { return (Status::Exited, StateSource::Hook); }
    if let Some((busy, at)) = self.meta.hook_status {            // NEW highest precedence
        // staleness guard (discretion): fall through when the event is stale.
        if now_unix_ms().saturating_sub(at) < HOOK_FRESH_MS {
            return (if busy { Status::Busy } else { Status::Waiting }, StateSource::Hook);
        }
    }
    if let Some((busy, _)) = self.meta.claude_status {           // unchanged
        return (if busy { Status::Busy } else { Status::Waiting }, StateSource::SessionFile);
    }
    let last = self.claude.last_output_ms.load(Ordering::Relaxed);  // unchanged silence path
    let s = if now_ms().saturating_sub(last) < BUSY_WINDOW_MS { Status::Busy } else { Status::Waiting };
    (s, StateSource::Silence)
}
pub fn status(&self) -> Status { self.status_with_source().0 }
```

**Enum-derive precedent** (`session.rs:17-25`, the `Status` enum) and **threshold-const precedent** (`session.rs:9-11`, `BUSY_WINDOW_MS`) — define `HOOK_FRESH_MS` next to `BUSY_WINDOW_MS` with the same doc-comment style; pin the chosen value (research A3: a few seconds, tunable, flicker-only-if-wrong).

**waiting_for_ms note** (`session.rs:126-131`) — currently keyed on `claude_status`. If hooks should drive the waiting clock too, extend it parallel to `status_with_source`; otherwise leave unchanged (no-regression). Planner decision; the no-regression-safe default is to leave it.

**Tests:** `session.rs` currently has **no `mod tests`** — adding one is a Wave-0 item. Mirror the `bridge.rs` pure-fixture style but construct a `Session`/`ClaudeMeta` with set `hook_status` / `claude_status` to assert `(Status, StateSource)` per precedence tier.

---

### `baude/src/main.rs` (MOD — `baude hook` dispatch)

**Analog:** the `statusline` dispatch arm (`main.rs:29-36`) — add a sibling `hook` arm BEFORE any terminal setup (Claude invokes it headless):
```rust
if args.get(1).map(String::as_str) == Some("statusline") {
    let wrap = args.iter().position(|a| a == "--wrap")
        .and_then(|i| args.get(i + 1)).cloned();
    std::process::exit(baude_core::bridge::run(wrap));
}
```
New arm (research Pattern 2 — runtime transport choice, always exit 0):
```rust
if args.get(1).map(String::as_str) == Some("hook") {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let v = serde_json::from_str::<Value>(&input).unwrap_or(json!({}));
    let line = baude_core::hook::build_event(&v).to_string();
    let sid = v["session_id"].as_str().unwrap_or_default();
    if let Ok(url) = std::env::var("BAUDE_EVENT_URL") {
        let _ = ureq::post(&url).send_string(&line);       // daemon path
    } else if !sid.is_empty() {
        let _ = baude_core::hook::append_event(sid, &line); // TUI-local path
    }
    std::process::exit(0);  // NEVER block claude
}
```
Note: `baude` binary has `ureq` (research confirmed, Web Push). `baude-core` stays HTTP-free — the POST lives here in the binary, the pure `build_event`/`append_event` live in core.

---

### `baude/src/app.rs` (MOD — TUI spawn seeding)

**Analog:** `App::add_session` (`app.rs:394-453`), specifically the spawn block (`app.rs:418-427`):
```rust
let base = self.claude_cmd();
let cmd = if resume {
    format!("{base} --continue 2>/dev/null || exec {base}")
} else {
    format!("exec {base}")
};
let (rows, cols) = self.claude_spawn_size(shell_open);
let claude = Pty::spawn(Some(&cmd), &cwd, rows, cols)?;
```
**Insert before `Pty::spawn`:** call the seed helper to deep-merge baude's hooks into `cwd/.claude/settings.local.json`. TUI sessions get NO `$BAUDE_EVENT_URL` (so the hook takes the /tmp append path — research Pitfall 4 warning sign). Seed `std::env::current_exe()` absolute path as the hook command, not bare `baude hook` (research A2 — highest-leverage assumption; `baude` may not be on the session PATH).

---

### `bauded/src/api.rs` (MOD — `POST /sessions/{id}/event` route)

**Analog:** the `Path<u64>` → `StatusCode` handlers `interrupt` (`api.rs:188-194`), `archive` (`api.rs:208-211`), `post_keys` (`api.rs:247-257`). Route registration mirrors `api.rs:28-36`.

**Route registration** (`api.rs:19-37`) — add after the `/stream` route (`api.rs:36`):
```rust
.route("/sessions/{id}/stream", get(stream))
// ADD:
.route("/sessions/{id}/event", post(post_event))
```

**Handler shape** (model on `archive`, `api.rs:208-211`, which returns 204 `NO_CONTENT` exactly as this route must):
```rust
async fn archive(State(state): State<Shared>, Path(id): Path<u64>) -> Result<StatusCode, ApiError> {
    lock(&state).set_archived(id, true).map_err(not_found)?;
    Ok(StatusCode::NO_CONTENT)
}
```
New `post_event` takes a raw `body: String` (one JSON event line already built by `baude hook`), calls `lock(&state).ingest_event(id, &body)`, returns 204 on Ok / 404 (`not_found`) on Err. The `not_found` helper at `api.rs:96-98` and `ApiError` type at `api.rs:94` are the established error idiom. V5 input-validation note (research Security): `Path<u64>` already rejects non-numeric ids; parse the body with `Value`/best-effort, never 500.

**Test harness** (`api.rs:549-556`, `pty_websocket_round_trip`) — the in-process `axum::serve` pattern to reuse for `post_event_appends`:
```rust
let state = Arc::new(Mutex::new(Manager::new("bash --norc -i".into(), false)));
let id = lock(&state).create("/tmp", None, None).unwrap().id;
let app = super::router(Arc::clone(&state));
let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
let addr = listener.local_addr().unwrap();
tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
```
Then POST a line to `/sessions/{id}/event`, assert 204 + the /tmp file got the line.

---

### `bauded/src/manager.rs` (MOD — spawn seed + env inject + ingest + SessionInfo)

**Analog (spawn + env inject):** `Manager::spawn` (`manager.rs:203-256`), specifically the command construction (`manager.rs:228-234`):
```rust
let base_cmd = &self.claude_cmd;
let cmd = if resume {
    format!("{base_cmd} --continue 2>/dev/null || exec {base_cmd}")
} else {
    format!("exec {base_cmd}")
};
let claude = Pty::spawn(Some(&cmd), &cwd, ROWS, COLS)?;
```
`Pty::spawn` has **no env-map parameter** (research Pitfall 4) — inject `$BAUDE_EVENT_URL` by prefixing the command string:
```rust
let cmd = format!("BAUDE_EVENT_URL={url} {cmd}");  // url = daemon loopback bind, e.g. http://127.0.0.1:8642/sessions/{id}/event
```
And seed `.claude/settings.local.json` in `cwd` before `Pty::spawn` (same new seed helper as `app.rs`). Note: worktree cwd is the worktree dir (research OQ#2 — seed into the actual cwd). The path that re-exercises the seed is the daemon **restore** path: `Manager::restore` (`manager.rs:117-131`) calls `self.spawn(...)` at `manager.rs:124` on daemon startup, so the seed merge MUST be idempotent (research Pitfall 2). Note that `restart` (`manager.rs:323-334`) does NOT go through `spawn` — it inlines its own `Pty::spawn` (`manager.rs:330`) and never re-seeds, which is harmless (the first spawn already seeded); the idempotency requirement is driven by `restore`, not `restart`.

**Analog (ingest_event):** `post_message` (`manager.rs:289-312`) is the precedent for "resolve session by id, act on it" — but ingest is simpler: resolve baude id → `Session.meta.session_id` (Claude sid), then `crate::hook::append_event(sid, body)` (same consume path the poll loop tails). Use `session(id)` / `session_mut(id)` (`manager.rs:465-477`) for resolution; they already return `Result` with the `"no session {id}"` message the api layer's `not_found` keys off.

**Analog (SessionInfo + builder):** struct at `manager.rs:45-64`, builder `session_info` at `manager.rs:524-545`. Add `state_source: &'static str` (and optionally `last_tool: Option<String>`) following the existing `status: &'static str` precedent (`manager.rs:49`, mapped by `status_str` at `manager.rs:78-84`):
```rust
fn session_info(s: &Session) -> SessionInfo {
    let status = s.status();  // ← switch to status_with_source() to get the label
    SessionInfo {
        // ...
        status: status_str(status),
        // ADD: state_source: source_str(source),
        // ADD: last_tool: s.meta.last_tool.as_ref().map(|(t, _)| t.clone()),
    }
}
```
Write a `source_str(StateSource) -> &'static str` mapper mirroring `status_str` (`manager.rs:78-84`).

---

## Shared Patterns

### Untyped `serde_json::Value` schema-drift discipline
**Source:** `bridge.rs:50-107` (build_bridge), `meta.rs:230-262` (transcript tail)
**Apply to:** `hook.rs::build_event`, `hook.rs::merge_hook_settings`, `meta.rs::read_event_tail`, `api.rs::post_event` body parse
Never `#[derive(Deserialize)]` on hook stdin or event lines. Read every field via `v["key"].as_str()`/`.as_u64()`/`.as_bool()` with `.or_else()` snake/camel fallbacks. Absent/wrong-type → `None`, never panic. `schema` is informational; readers must not branch on it.
```rust
"model": v["model"]["display_name"].as_str().or_else(|| v["model"]["id"].as_str()),
```

### Best-effort, never-block posture
**Source:** `bridge.rs:109-138` (`run` returns codes but writes are `let _ =`)
**Apply to:** `baude hook` dispatch (always `exit(0)`), `append_event`, daemon `post_event` (404 not 500 on bad body)
Every side-effect is `let _ = ...`; a hook failure must never surface in Claude's transcript or block a turn (research anti-pattern: non-zero exit is a blocking signal).

### `/tmp/baude-<kind>-<sid>` per-session file convention
**Source:** `bridge.rs:26-28` (`bridge_path`), consumed at `meta.rs:269-275`
**Apply to:** `hook.rs::event_path` → `/tmp/baude-events-<sid>.jsonl`
Same lifecycle as the existing usage bridge file; auto-created, ephemeral, no migration, stale files harmlessly ignored by the offset tail.

### `Path<u64>` typed extractor + `not_found`/`ApiError` error idiom
**Source:** `api.rs:94-98` (`ApiError`, `not_found`), every `Path<u64>` handler (`api.rs:188-219`)
**Apply to:** `post_event`
The `u64` extractor rejects non-numeric ids at the framework layer (V5 input validation); `not_found` maps `anyhow::Error` → 404.

### In-place additive extension of `poll()` / `status()` (no-regression)
**Source:** `meta.rs::poll` (117-127), `session.rs::status` (108-123)
**Apply to:** `meta.rs` (add `read_event_tail` after `read_bridge_file`), `session.rs` (prepend hook branch)
Extend by adding a call / prepending a branch — never rewrite. Keep public signatures total (`status()` still returns `Status`).

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `merge_hook_settings` (within new `hook.rs`) | core / transform | transform | No Rust settings-merge code exists. Closest *intent* is `docker-entrypoint.sh:4-11` (shell, write-if-absent, global settings.json — NOT a deep-merge, NOT the local file, NOT idempotent over arrays). Build fresh per research Pattern 1, structurally modeled on `build_bridge`'s pure-fn + `Value` discipline. This is the highest-correctness-risk new code (HOOK-01 acceptance criterion). |

## Metadata

**Analog search scope:** `baude-core/src/{bridge,meta,session}.rs`, `baude/src/{main,app}.rs`, `bauded/src/{api,manager}.rs`, `docker-entrypoint.sh`, `README.md`
**Files scanned:** 8 source files + grep across whole workspace for settings/statusline seeding sites
**Pattern extraction date:** 2026-06-15
