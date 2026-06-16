# Phase 4: Remote Permission Approval - Pattern Map

**Mapped:** 2026-06-15
**Files analyzed:** 9 (1 new core module, 8 modified)
**Analogs found:** 9 / 9 (every piece is a re-application of an existing baude pattern)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `baude-core/src/permission.rs` (NEW) | utility (pure transforms) | transform / request-response | `baude-core/src/hook.rs` (`dispatch_hook`/`build_event`) | exact (role + posture) |
| `baude/src/main.rs` (`permission-mcp` arm) | route (subcommand dispatch) | streaming (stdio JSON-RPC) | `baude/src/main.rs::run_hook` (`hook` arm) | role-match (blocks vs exit-0) |
| `bauded/src/main.rs` (`permission-mcp` arm) | route (subcommand dispatch) | streaming (stdio JSON-RPC) | `bauded/src/main.rs::run_hook` (`hook` arm, ~74) | role-match (Pitfall 2 trap) |
| `bauded/src/manager.rs` (spawn flag) | config (spawn cmd build) | request-response | `spawn_command` (~126) + `default_claude_cmd` (~146) | exact |
| `baude/src/app.rs` (spawn flag) | config (spawn cmd build) | request-response | `claude_cmd` (~236) | exact |
| `bauded/src/manager.rs` (pending state + `waiting_reason`) | service (session state) | event-driven | `ingest_event`/`session(id)?` (~387) + `session_info` (~635) | exact |
| `bauded/src/api.rs` (GET/POST `/permission`) | controller | request-response (CRUD) | `interrupt` (~213) + `post_event` (~256) + `get_session` (~109) | exact |
| `bauded/src/notify.rs` (distinct push) | service | event-driven / pub-sub | `Notifier::tick` (~42) + `notified_waiting` + `Notification::to_json` (~28) | exact |
| `bauded/web/app.js` (approve/deny card) | component | request-response | `renderChat` (~561) + `sendMessage`/`interrupt` POST (~286/305) + `esc` (~32) | exact |

Cross-cutting touch (no new file): `baude/src/remote.rs::RemoteInfo` (~21) gains a `#[serde(default)] waiting_reason: Option<String>` mirror of the new `SessionInfo` field.

## Pattern Assignments

### `baude-core/src/permission.rs` (NEW — utility, pure transform)

**Analog:** `baude-core/src/hook.rs` — the pure-transform-in-core / network-in-binary split, untyped `Value` posture, never-panic discipline.

**Module-doc + untyped posture** (hook.rs:16-25): every field read via `serde_json::Value` accessors, never typed `Deserialize`, so unknown/absent keys are tolerated. Replicate verbatim for the MCP request parse (research §C: read `input` with `parameters`/`tool_input` fallbacks, tolerate missing `tool_use_id`).

**Pure `Value -> Value` builder** (hook.rs:55-65) — the model for the MCP `approve`-result builder (research §D):
```rust
pub fn build_event(v: &Value) -> Value {
    json!({
        "schema": 1,
        "ts": now_unix_ms(),
        "session_id": v["session_id"].as_str(),
        "event": v["hook_event_name"].as_str(),
        "tool": v["tool_name"].as_str(),
        "notification_type": v["notification_type"].as_str(),
    })
}
```
New analog: `build_approve_result(behavior, updated_input, message) -> Value` producing `{"content":[{"type":"text","text":<JSON string>}]}` (§D), and `parse_tool_call(params: &Value) -> (tool, input)` reading `tool_name` + `input`/`parameters`/`tool_input`.

**`dispatch_hook` — the core/binary boundary** (hook.rs:204-212): the pure dispatch takes a `post` closure so `baude-core` carries no HTTP dependency. The permission dispatch mirrors this — keep frame-parse + result-build pure here; the stdin loop + `ureq` POST/poll live in the binaries.
```rust
pub fn dispatch_hook<F>(input: &str, url: Option<&str>, post: F)
where F: FnOnce(&str, &str) -> bool { /* parse Value, build line, route */ }
```

**`waiting_reason` derivation** (meta.rs:446-447 already captures `last_notification`): add a pure mapper here (research §"waiting_reason derivation"):
```rust
// last_notification is Option<(String, u64)> set on Notification events (meta.rs:447)
pub fn waiting_reason(last_notification: Option<&(String, u64)>, waiting: bool) -> &'static str {
    match last_notification {
        Some((nt, _)) if nt.contains("permission") => "permission",
        _ if waiting => "input",
        _ => "none",
    }
}
```

**Test posture** (hook.rs:222-296): `#[cfg(test)] mod tests` with `*_never_panics` cases on malformed input. Mirror for the MCP parse + result builder (Validation Test Map §C/§D).

---

### `baude/src/main.rs` + `bauded/src/main.rs` (route — `permission-mcp` subcommand)

**Analog:** the two byte-identical `run_hook` functions and their dispatch arms.

**TUI dispatch arm** (`baude/src/main.rs:75-77`):
```rust
if args.get(1).map(String::as_str) == Some("hook") {
    run_hook();
}
```

**Daemon dispatch arm — the Pitfall-2 trap** (`bauded/src/main.rs:68-75`). The doc-comment here IS the warning the research flags: without the arm, `bauded permission-mcp` boots a second daemon.
```rust
// `bauded hook` — the daemon seeds its own `current_exe()` (= `bauded`) as the
// hook command ... so the daemon binary MUST handle the `hook` subcommand.
// Without this arm, `bauded hook` falls through and boots a *second daemon*.
Some("hook") => run_hook(),
```

**`run_hook` body — the bounded `ureq` agent the bridge extends** (`baude/src/main.rs:40-53`, byte-identical at `bauded/src/main.rs:31-44`):
```rust
fn run_hook() -> ! {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let url = std::env::var("BAUDE_EVENT_URL").ok();
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_millis(500))
        .timeout(std::time::Duration::from_secs(2))
        .build();
    baude_core::hook::dispatch_hook(&input, url.as_deref(), |url, line| {
        agent.post(url).send_string(line).is_ok()
    });
    std::process::exit(0);   // ALWAYS exit 0 — hook never blocks Claude
}
```
**Critical contrast to document (research §G, CONTEXT specifics):** `run_hook` is fire-and-forget exit-0; `run_permission_mcp` BLOCKS on Claude's critical path — it reads stdio JSON-RPC frames, POSTs the pending request, then long-polls `GET /sessions/{id}/permission` until resolved OR a deadline (`BAUDE_PERMISSION_TIMEOUT_S`, default ~120s) → **deny on timeout, never allow**. The bridge process blocks; that is fine (short-lived child). Add a **byte-identical arm to BOTH binaries** and keep shared framing/transform in `baude-core::permission` (the `dispatch_hook` split).

**Bounded blocking POST-then-poll** (research Code Examples, derived from `run_hook`): same `AgentBuilder`, longer read timeout, a `loop` with `deadline = Instant::now() + timeout`, `std::thread::sleep(500ms)` between polls, `break "deny"` on deadline.

---

### `bauded/src/manager.rs` — spawn flag selection (config, PERM-01)

**Analog:** `spawn_command` (126-133) + `default_claude_cmd` (146-151); the `export BAUDE_EVENT_URL=...; <inner>` prefix is the model for appending a flag to the inner base cmd.

**`spawn_command` (126-133)** — append the permission flag to `base_cmd` BEFORE the `export …; {inner}` wrap so it survives the `--continue || exec` resume fallback (WR-01):
```rust
fn spawn_command(base_cmd: &str, event_url: &str, resume: bool) -> String {
    let inner = if resume {
        format!("{base_cmd} --continue 2>/dev/null || exec {base_cmd}")
    } else {
        format!("exec {base_cmd}")
    };
    format!("export BAUDE_EVENT_URL={event_url}; {inner}")
}
```

**`default_claude_cmd` (146-151)** — the exact env-precedence shape `BAUDE_PERMISSION_MODE` must follow (env → config → default), default `skip`:
```rust
pub fn default_claude_cmd() -> String {
    std::env::var("BAUDE_CLAUDE_CMD").ok()
        .or_else(|| persist::load_config().claude_cmd)
        .unwrap_or_else(|| "claude".to_string())
}
```

**New `permission_flag(base_cmd)` helper** (research Pattern 1): default `skip` → `--dangerously-skip-permissions`; `prompt` → `--permission-prompt-tool mcp__baude__approve`; **no-double-add** by scanning `base_cmd` for `--dangerously-skip-permissions`/`--permission-prompt-tool`/`--permission-mode`. **Never append both** (research §E). In `prompt` mode also seed `.mcp.json` + `--mcp-config` alongside the existing `seed_settings(&cwd)` call (manager.rs:283) — see Shared Patterns.

**SECURITY-CRITICAL test** (Validation §PERM-01): `skip` is the hard default; assert default and the mutual-exclusion + no-double-add. Unit-testable like `decide_status` (session.rs:280+) — keep `permission_flag` pure over the base string.

---

### `baude/src/app.rs` — spawn flag selection (config, PERM-01)

**Analog:** `claude_cmd` (236-241) — the TUI's identical env→config→default precedence. The same `permission_flag` selection applies at the TUI spawn site (CONTEXT: both build the command string).
```rust
fn claude_cmd(&self) -> String {
    std::env::var("BAUDE_CLAUDE_CMD").ok()
        .or_else(|| self.config.claude_cmd.clone())
        .unwrap_or_else(|| "claude".to_string())
}
```

---

### `bauded/src/manager.rs` — pending state, set/resolve, `waiting_reason` (service, PERM-02/04)

**Analog:** `Session` struct (`session.rs:47-72`) for added meta state; `ingest_event` + `session(id)?` (manager.rs:387-398) for 404 routing; `session_info` (635-665) for the new `SessionInfo` field.

**`Session` struct (session.rs:47-72)** — add `pending_permission: Option<PendingPermission>` alongside `meta`/`archived`; init `None` in the spawn literal (manager.rs:289-305, next to `meta: ClaudeMeta::default()`). `PendingPermission { request_id: String, tool: String, input: serde_json::Value, ts: u64 }` (research Pattern 2; `ts` drives the timeout deadline).

**404 routing via `self.session(id)?`** (manager.rs:387-389) — `set_pending`/`resolve_pending` mirror this:
```rust
pub fn ingest_event(&mut self, id: u64, body: &str) -> Result<()> {
    let s = self.session(id)?;          // Err -> 404 at the handler via not_found
    ...
}
```
`set_pending(id, p)` stores `Some(p)`; `resolve_pending(id, decision)` clears + signals waiters; `pending(id)` reads for the GET. **Pitfall 4:** set/clear under the lock, but the bridge's wait (and the GET long-poll) await OUTSIDE the lock — mirror `bauded/src/main.rs:92-126` "decide under the lock, send outside it". Prefer a `tokio::sync::Notify`/`watch` per request; a bounded `META_POLL_MS` poll is the acceptable fallback (research Open Q3).

**`SessionInfo` struct (manager.rs:45-73)** — add `pub waiting_reason: Option<String>` (Serialize). **Pitfall:** this new field breaks the `#[cfg(test)] fn info()` test constructor in notify.rs — see that file's note below.

**`session_info` builder (manager.rs:635-665)** — populate the new field from `last_notification` + waiting status, via the pure `baude_core::permission::waiting_reason`:
```rust
fn session_info(s: &Session) -> SessionInfo {
    let (status, source) = s.status_with_source();
    SessionInfo {
        id: s.id,
        ...
        // NEW: waiting_reason: derive from meta.last_notification + (status == Waiting)
    }
}
```

**Re-seed `.mcp.json` on restore** (manager.rs:283 `seed_settings(&cwd)` already runs inside `spawn`, which `restore` (166-195) re-calls): the `prompt`-mode `.mcp.json` seed must sit in the same place so it is re-seeded on every restart, exactly like the hook settings (research Runtime State Inventory).

---

### `bauded/src/api.rs` — GET/POST `/sessions/{id}/permission` (controller, PERM-02)

**Analog:** `interrupt` (213-218, POST `Path<u64>`), `post_event` (256-263, POST body + 404), `get_session` (109-117, GET `Path<u64>` → `Json` or 404). Router pattern (21-43).

**Router (api.rs:30, 39)** — add one route in `router()`:
```rust
.route("/sessions/{id}/permission", get(get_permission).post(post_permission))
```

**GET handler** (model: `get_session` 109-117 + `get_activity` 178-186) — return the pending view or null/204; unknown id → 404 via `not_found`:
```rust
async fn get_session(State(state): State<Shared>, Path(id): Path<u64>)
    -> Result<Json<SessionInfo>, ApiError> {
    lock(&state).info(id).map(Json)
        .ok_or((StatusCode::NOT_FOUND, format!("no session {id}")))
}
```

**POST handler** (model: `interrupt` 213-218 + `post_message` 193-211 for the body+validation shape):
```rust
async fn interrupt(State(state): State<Shared>, Path(id): Path<u64>)
    -> Result<StatusCode, ApiError> {
    lock(&state).interrupt(id).map_err(not_found)?;
    Ok(StatusCode::ACCEPTED)
}
```
New `Decision { decision: String, scope: Option<String> }` deserialized via `Json`. **SECURITY clamp (V5 + deny-default):** validate `decision ∈ {"allow","deny"}` → else `StatusCode::BAD_REQUEST`; **never treat unknown as allow** (research Pattern 3 + Security Domain). `Path<u64>` already rejects non-numeric ids at the framework layer (no 500 path) — same disposition documented at `post_event` (251-255).

**Tests** (model: `post_event_appends_and_404s_unknown` 543-586, `unknown_session_is_404` 505-519, `bad_requests_are_400` 521-541) — assert pending/null, 404 unknown id, 400 on bad decision, and resolve-wakes-waiter / **timeout → deny**.

---

### `bauded/src/notify.rs` — distinct permission push (service, PERM-04)

**Analog:** `Notifier::tick` (42-94), the `notified_waiting`/`notified_exited` `HashSet<u64>` debounce, `Notification` + `to_json` lean payload (22-39).

**Add `notified_permission: HashSet<u64>` to `Notifier` (14-20)** and retain-prune it in `tick` alongside the others (45-47).

**`Notification::to_json` (28-39)** — the lean payload (`sid` + tag); the permission push adds a `kind:"permission"` marker, PWA fetches `GET /permission` for detail:
```rust
pub fn to_json(&self) -> Vec<u8> {
    serde_json::json!({ "title": self.title, "body": self.body,
        "tag": format!("baude-{}", self.sid), "sid": self.sid }).to_string().into_bytes()
}
```

**Debounce + re-arm pattern, in the "waiting" arm (58-66)** — branch on `waiting_reason` (research Pattern 4):
```rust
"waiting" => {
    let waited = s.waiting_for_ms.unwrap_or(0);
    if waited >= WAITING_DEBOUNCE_MS && self.notified_waiting.insert(s.id) {
        out.push(Notification { title: format!("{} is waiting for you", s.name),
            body: s.title.clone().unwrap_or_default(), sid: s.id });
    }
}
```
New: when `s.waiting_reason.as_deref() == Some("permission")`, push a distinct Notification via `notified_permission.insert(s.id)` (title e.g. `"{name} needs permission"`, body a permission summary). **Re-arm** by removing `s.id` from `notified_permission` when `waiting_reason` flips away (the same re-arm the `busy` arm does at 86-89). The send path (`push::send` + `to_json`, driven in `bauded/src/main.rs:104-123`) is unchanged.

**Pitfall — test constructor (notify.rs:101-124):** the `#[cfg(test)] fn info(...)` literal constructs every `SessionInfo` field. Adding `waiting_reason` to `SessionInfo` (manager.rs:45) **breaks this constructor's compile** (the recurring 02-03/03-02 Rule-3 fix). Add `waiting_reason: None` to the literal:
```rust
fn info(id: u64, status: &'static str, waiting_ms: Option<u64>) -> SessionInfo {
    SessionInfo { id, name: format!("s{id}"), title: None, status,
        state_source: "silence", last_tool: None, waiting_for_ms: waiting_ms,
        /* ... all existing fields ... */ archived: false, activity: vec![] }
}
```
Add a `notify::permission` test: distinct push fires once via `notified_permission`, re-arms on resolve (Validation §PERM-04).

---

### `bauded/web/app.js` — approve/deny card (component, PERM-03)

**Analog:** `renderChat` (561-656), the `sendMessage`/`interrupt` POST-action shape (286-313), `api()` helper (75), `esc()` (32-36).

**`esc()` (32-36)** — XSS escape EVERY dynamic string (tool name + arbitrary tool input) in the card:
```js
function esc(s) {
  return String(s).replace(/[&<>"']/g, (c) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  })[c]);
}
```

**POST-action shape (`sendMessage` 286-303, `interrupt` 305-313)** — Approve/Deny buttons mirror this `api(url, {method:"POST", body})` + `toast` on error:
```js
async function interrupt() {
  if (state.sid === null) return;
  try {
    await api(`/sessions/${state.sid}/interrupt`, { method: "POST" });
    toast("sent esc");
  } catch (e) { toast(`interrupt failed: ${e.message}`); }
}
```
New `approve()/deny()` → `api(`/sessions/${state.sid}/permission`, {method:"POST", body: JSON.stringify({decision})})`, then optimistic `state.pendingPermission = null; render()` + refetch (CONTEXT SC3: card disappears on resolve). **Deny denies the single tool call only — does NOT interrupt/kill the session.**

**Card placement in `renderChat` (605-628)** — the `$app.innerHTML` template lists `${activityStrip}` then the `<form id="composer">`. Insert the perm card BETWEEN them (research Pattern 5), gated on `s && s.waiting_reason === "permission" && state.pendingPermission`:
```js
${activityStrip}
${permCard}              // <div class="perm-card"> ... Approve/Deny ... </div>
${s && s.status === "exited" ? `...restart...` : `<form id="composer">...`}
```
Wire `onclick` in the same handler block as `escbtn`/`killbtn` (630-641). The PWA learns of a pending permission via push/SSE (`waiting_reason=permission`) and on chat open, then fetches `GET /permission` (use the existing `api()` GET shape as at 223 `api(`/sessions/${sid}/activity?limit=30`)`).

---

## Shared Patterns

### `.mcp.json` / settings seeding for `prompt` mode
**Source:** `baude_core::hook::seed_settings` (hook.rs:147-158) + `merge_hook_settings` (92-121) + `baude_hook_command` (74-79).
**Apply to:** both spawn sites, in `prompt` mode only, alongside the existing `seed_settings(&cwd)` call (manager.rs:283).
```rust
pub fn seed_settings(cwd: &std::path::Path) {
    let dir = cwd.join(".claude");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("settings.local.json");
    let existing = std::fs::read_to_string(&path).ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .unwrap_or_else(|| json!({}));
    let command = baude_hook_command();          // current_exe() + " hook"
    let merged = merge_hook_settings(&existing, &command);
    let _ = std::fs::write(&path, merged.to_string());
}
```
The MCP-server command is `current_exe() + " permission-mcp"` (same `current_exe()` resolution as `baude_hook_command` at 74-79 — so the daemon seeds `bauded permission-mcp`, the Pitfall-2 reason BOTH binaries need the arm). The seed must be **idempotent, non-clobbering, best-effort** (never abort a spawn) and re-run on `restore()` — exactly the `merge_hook_settings` posture (`.entry().or_insert()`, command is the idempotency sentinel).

### Untyped-`Value`, never-panic posture
**Source:** `hook.rs:16-25` module doc; every reader (`build_event` 55-65, `merge_hook_settings` 92-121, `ingest_event` 389-392).
**Apply to:** the MCP request parse (`tool_name` + `input`/`parameters`/`tool_input` fallbacks, tolerate missing `tool_use_id`) and the `Decision` body. Read via `Value` accessors; an odd/minimal payload yields `null`, never a panic (research §C/§F, Security V5).

### "Decide under the lock, act outside it"
**Source:** `bauded/src/main.rs:92-126` (notifier loop snapshots under the lock, sends over the network outside it).
**Apply to:** the daemon-side permission wait (Pitfall 4) — set/clear `pending_permission` under the manager lock; `await` the resolve (`Notify`/`watch` or bounded poll) OUTSIDE the lock so one pending permission never stalls other sessions.

### Bounded `ureq` agent (blocking client)
**Source:** `run_hook` (`baude/src/main.rs:45-48`) + `push::send` (push.rs:257-263).
**Apply to:** the `permission-mcp` bridge's POST + long-poll — same `AgentBuilder` with `timeout_connect(500ms)`, a longer per-request `timeout`, and a deadline loop that returns **deny** on expiry (research Code Examples). No new HTTP crate.

## No Analog Found

None. Every file is a re-application of an existing baude pattern; the only genuinely novel surface is the stdio JSON-RPC MCP framing (research §G), and even that reuses the `dispatch_hook` pure/binary split + the bounded `ureq` agent. The MCP wire contract (§C/§D/§G) is MEDIUM-confidence and **must be confirmed by the §F `checkpoint:human-verify` UAT before `prompt`-mode spawn-wiring is finalized** — the planner inserts that gate, not a pattern question.

## Metadata

**Analog search scope:** `baude-core/src/{hook,meta,session,permission}.rs`, `baude/src/{main,app,remote}.rs`, `bauded/src/{main,manager,api,notify,push}.rs`, `bauded/web/app.js`.
**Files scanned:** 11 read (targeted ranges), all read-only.
**Pattern extraction date:** 2026-06-15

## PATTERN MAPPING COMPLETE

**Phase:** 4 - Remote Permission Approval
**Files classified:** 9 (1 new core module + 8 modified)
**Analogs found:** 9 / 9

### Coverage
- Files with exact analog: 7
- Files with role-match analog: 2 (the two `permission-mcp` subcommand arms — `run_hook` shape but BLOCKS vs exit-0)
- Files with no analog: 0

### Key Patterns Identified
- Spawn-flag selection mirrors `default_claude_cmd`/`claude_cmd` env-precedence and `spawn_command`'s `export …; {inner}` prefix; default `skip`, no-double-add, never both flags (SECURITY-CRITICAL).
- The `permission-mcp` subcommand is the `hook` subcommand's twin in BOTH binaries (Pitfall 2: missing arm boots a second daemon), but it BLOCKS Claude's critical path with a deny-on-timeout long-poll — the inverse of the always-exit-0 hook.
- Daemon state/API/push/PWA are mechanical re-applications: `session(id)?` 404 routing, `Path<u64>` GET/POST handlers, `Notifier` `notified_*` debounce sets, `Notification::to_json` lean payload, `renderChat` + `esc()` card.
- `.mcp.json` seeding reuses `seed_settings`/`merge_hook_settings` (idempotent, non-clobbering, re-seeded on restore) with command `current_exe() + " permission-mcp"`.
- The new `SessionInfo.waiting_reason` field breaks the `notify.rs` `#[cfg(test)] fn info()` constructor — known Rule-3 compile fix; add `waiting_reason: None`.

### File Created
`.planning/phases/04-remote-permission-approval/04-PATTERNS.md`

### Ready for Planning
Pattern mapping complete. Planner can reference analog file paths + line ranges directly in PLAN.md actions, and MUST insert the §F `checkpoint:human-verify` UAT before finalizing `prompt`-mode spawn-wiring (the one MEDIUM-confidence MCP wire contract).
