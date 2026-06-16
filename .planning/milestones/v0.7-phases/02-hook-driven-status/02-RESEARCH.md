# Phase 2: Hook-Driven Status - Research

**Researched:** 2026-06-15
**Domain:** Claude Code hooks integration + Rust JSON merge / file-tail / state precedence
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Hook Seeding & settings.json Merge**
- Seed baude's hook set into the per-session `.claude/settings.local.json` in the session's cwd/worktree — gitignored by Claude convention, merges over the user's global config, never modifies committed project settings or `~/.claude/settings.json`.
- Merge by reading existing JSON (if any) and **appending** baude's entries into each hook event's array, guarded by a **sentinel marker** so re-spawn/restart is idempotent and never duplicates entries. Preserve all existing keys — user hooks and a user `statusLine` survive intact. Apply `bridge.rs` schema-drift tolerance (untyped `serde_json::Value`, snake/camel safe; never panic on a minimal/odd file).
- Add a new **`baude hook`** subcommand wired to all four events. Claude invokes it per event; it reads hook JSON from **stdin** (`session_id`, `hook_event_name`, `tool_name`) and appends one normalized event line.
- **No cleanup on session close** — seeding is idempotent and additive.

**Event Model & State Derivation**
- Event lines are **schema-versioned JSONL**: `{schema:1, ts, session_id, event, tool?}`, read with **untyped `serde_json::Value` accessors** (no `#[derive(Deserialize)]`, no branching on schema).
- Event→state mapping: `UserPromptSubmit` → **Busy**; `Stop` → **Waiting**; `Notification` → **Waiting** (needs-input/permission); `PostToolUse` → stay Busy and **record the last tool name**.
- Keep `Status { Waiting, Busy, Exited }` enum **unchanged** — "done" == `Waiting`. Add a separate **`StateSource`** reason field, not new enum variants.
- **Capture** last tool name + timestamp in `meta` and surface it **minimally** in the info overlay (Phase 1 capture-but-render-lightly). Rich timeline rendering deferred to Phase 3.

**Transport & Fallback Labeling**
- One `baude hook` command chooses transport at runtime: **POST** to `$BAUDE_EVENT_URL` when set (daemon seeds it at spawn), else **append** to `/tmp/baude-events-<sid>.jsonl` (TUI-local path).
- Daemon exposes **`POST /sessions/{id}/event`**: resolves baude id → Claude `session_id`, feeds the event into the **same consume path** (append to the per-session `/tmp` file tailed by `meta.rs`), returns 204. One event model serves both transports.
- **Precedence**: hook events (freshest) **>** Claude session-file status **>** silence heuristic (labeled fallback). Hooks win when present and recent.
- Add **`StateSource { Hook, SessionFile, Silence }`** label on the status. With no/stale hook events, fall back to the **existing dual-source (session-file + silence) logic unchanged** — no v0.6.1 regression, fallback labeled.

### Claude's Discretion
- Exact sentinel-marker shape, event-file offset/tailing details (mirror `read_transcript_tail()`), `StateSource` rendering location in the overlay, and staleness thresholds are at Claude's discretion, guided by existing `meta.rs` conventions.

### Deferred Ideas (OUT OF SCOPE)
- Live per-session tool-activity timeline UI (PWA + TUI) — **Phase 3** (ACT).
- Remote permission approval / opt-in permission-prompt mode — **Phase 4** (PERM).
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| HOOK-01 | On spawn, seed a hook set into the managed session's settings by merging into existing arrays — never clobbering a user's hooks or statusLine | `## Architecture Patterns` → Pattern 1 (idempotent deep-merge with sentinel); confirmed `.claude/settings.local.json` is the correct gitignored local-override target (CITED Claude settings docs); `## Code Examples` → merge function; `## Common Pitfalls` → Pitfall 1 (clobber), Pitfall 2 (non-idempotent merge) |
| HOOK-02 | State derives from Claude Code hooks (UserPromptSubmit→busy, Stop→waiting, Notification→waiting-for-permission, PostToolUse→ran tool X); silence heuristic remains a labeled fallback | Verified hook stdin schema for CLI v2.1.177 (`## Standard Stack` → hook event reference); `## Architecture Patterns` → Pattern 3 (precedence + StateSource); existing `session.rs::status()` dual-source logic mapped in `## Code Examples` |
| HOOK-03 | Events transport via per-session file-tail (`/tmp/baude-events-<sid>.jsonl`) for TUI-local and `POST /sessions/{id}/event` in the daemon; one event model serves both | `## Architecture Patterns` → Pattern 2 (dual transport converging on /tmp tail); `read_transcript_tail()` offset-tail pattern to copy; `ureq` already present in both binaries for the POST path; new axum route slots into `api.rs::router()` |
</phase_requirements>

## Summary

This phase replaces baude's PTY-output-silence state heuristic with first-party Claude Code hook events, while preserving the silence path as a labeled fallback. The work is almost entirely **internal-codebase engineering** — there are no new external libraries to evaluate. Everything needed (`serde_json::Value`, `std::fs` append, `ureq` for the daemon POST, the existing offset-tracked `read_transcript_tail()` tail pattern) is already in the workspace and battle-tested by Phase 1.

The single external dependency is the **Claude Code hooks contract**: the `settings.json` `hooks` object structure and the JSON delivered on a hook command's stdin. Both are verified against the installed CLI (`claude --version` → **2.1.177**, identical to the version pinned in `bridge.rs`) and the official hooks reference. Critically: `Notification` *does* fire for permission prompts (`notification_type: "permission_prompt"`), common stdin fields are `session_id` / `hook_event_name` / `cwd` / `transcript_path` / `permission_mode`, and tool events add `tool_name` / `tool_input` / `tool_response`. `.claude/settings.local.json` is a documented, auto-gitignored local-override layer — the correct seed target.

The no-regression path is well-bounded: `session.rs::status()` is a 16-line function with one branch point (`meta.claude_status`). Adding a higher-precedence hook source plus a `StateSource` label is an additive change that leaves the silence fallback untouched. The hardest correctness work is the **idempotent settings merge** (HOOK-01's explicit acceptance criterion) and faithfully mirroring the WR-01 "clear-when-absent" discipline from Phase 1 for the new hook-derived fields.

**Primary recommendation:** Build a pure, testable `build_event(stdin: &Value) -> Value` + `merge_hook_settings(existing: &Value) -> Value` pair in `baude-core` (mirroring `build_bridge`), dispatch `baude hook` from `baude/src/main.rs` (which has `ureq` for the POST path), tail `/tmp/baude-events-<sid>.jsonl` in `meta.rs` after `read_bridge_file()`, and layer a `StateSource`-labeled hook source ahead of the existing `claude_status` branch in `session.rs::status()`.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Hook settings seeding | baude-core (pure merge fn) | TUI `app.rs` / daemon `manager.rs` (I/O + call site) | Pure JSON transform is testable in core (like `build_bridge`); the actual file write happens at the spawn integration point that knows the cwd |
| `baude hook` event emission | baude-core (pure `build_event`) | `baude/src/main.rs` (dispatch + transport) | Event-line construction is a pure `Value→Value` transform; HTTP POST needs `ureq`, which lives in the binary, not core |
| Event transport selection | `baude` binary (`baude hook` runtime) | — | `$BAUDE_EVENT_URL` presence is a runtime decision in the hook subcommand |
| Daemon event ingest route | bauded `api.rs` | bauded `manager.rs` (id resolution) | New REST endpoint inherits the existing axum router + tailnet security model |
| Event tail + state consume | baude-core `meta.rs` | baude-core `session.rs` (precedence) | Tail is an extension of the existing poll-and-offset file readers; state derivation is the `status()` function |
| `StateSource` surfacing | baude-core `session.rs` / `meta.rs` | TUI overlay + `SessionInfo` (daemon) | State label is computed in core, rendered minimally in the overlay and exposed on `SessionInfo` |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `serde_json` | 1 (workspace) | Untyped `Value` parse/merge/emit of hook JSON and event lines | Already the project's universal Claude-data accessor; STL-02 back-compat discipline depends on `Value` over `Deserialize` |
| `std::fs` / `std::io` | std | Read settings, O_APPEND event lines, offset-tracked tail | `read_transcript_tail()` already proves the pattern; no crate needed |
| `ureq` | 2 (`features=["json"]`) | `baude hook` POST to `$BAUDE_EVENT_URL` (daemon transport) | Already a dependency of **both** `baude` and `bauded` binaries (Web Push uses it). No new dep. |
| `axum` | 0.8 | `POST /sessions/{id}/event` daemon route | Existing daemon router |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `portable-pty` | 0.8 | Already spawns claude via `sh -c "<cmd>"` | Env injection (`$BAUDE_EVENT_URL`) rides on the command string — see Pitfall 4 |

### Claude Code Hook Event Reference (verified v2.1.177)

`[VERIFIED: claude --version → 2.1.177]` + `[CITED: code.claude.com/docs/en/hooks]`

**settings.json `hooks` structure** — top-level `hooks` object keyed by event name (PascalCase), each mapping to an array of matcher-groups:

```json
{
  "hooks": {
    "PostToolUse": [
      { "matcher": "*", "hooks": [ { "type": "command", "command": "baude hook" } ] }
    ]
  }
}
```

- `matcher`: for tool events filters on `tool_name` (`"Bash"`, `"Edit|Write"`, or regex); `"*"`/`""`/omitted = match all. `UserPromptSubmit`/`Stop`/`Notification` are not tool events — a single no-matcher group is fine.
- Inner hook entry: `{ "type": "command", "command": "<cmd>" }`. Optional `timeout` (default 600s for command), `args` (exec form). baude uses `type:"command"`, `command:"baude hook"`.

**stdin JSON delivered to a command hook** (snake_case):

| Field | Type | Events | Notes |
|-------|------|--------|-------|
| `session_id` | string | all | Claude session id — keys the `/tmp/baude-events-<sid>.jsonl` file |
| `hook_event_name` | string | all | `"UserPromptSubmit"`, `"Stop"`, `"Notification"`, `"PostToolUse"`, … |
| `cwd` | string | all | session working dir |
| `transcript_path` | string | all | path to transcript JSONL |
| `permission_mode` | string | all | `default`/`plan`/`acceptEdits`/`bypassPermissions`/… |
| `tool_name` | string | PreToolUse, PostToolUse, … | e.g. `"Bash"`, `"Edit"`, `mcp__x__y` |
| `tool_input` | object | tool events | tool args |
| `tool_response` | any | PostToolUse | tool result |
| `prompt` | string | UserPromptSubmit | the submitted text |
| `notification_type` | string | Notification | **`permission_prompt`** / `idle_prompt` / `elicitation_dialog` / `auth_success` / … |
| `message` | string | Notification | human-readable text |

**Key facts for this phase:**
- `Notification` **does** fire for permission prompts — distinguished by `notification_type == "permission_prompt"`. `[CITED: code.claude.com/docs/en/hooks]` This is what HOOK-02's "waiting for permission/input" label keys off (and what Phase 4 PERM will build on).
- `Stop` has no event-specific fields beyond the common set — map purely on `hook_event_name`.
- A command hook reads stdin, signals via exit code (0 = success). baude's hook should **exit 0 unconditionally** and never block Claude (mirror `bridge::run`'s best-effort posture).

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `type:"command"` (`baude hook`) | `type:"http"` hook posting directly to daemon | HTTP hooks exist in v2.1.177, but a command hook keeps one code path for TUI-local (no daemon, no bind) and daemon alike; the decision locks the command approach |
| Per-session `/tmp` file | Unix socket / named pipe | File-tail reuses the exact `read_transcript_tail()` offset pattern and the no-watch poll loop; a socket adds a new lifecycle to manage |
| New `Status` variant for "needs permission" | `StateSource` label + reason | Locked: keep `Status` enum stable to avoid UI churn and honor no-regression |

**Installation:** No new dependencies. `ureq` and `serde_json` are already in `baude`/`bauded`; `baude-core` stays HTTP-free (the POST lives in the `baude` binary).

**Version verification:**
- `claude --version` → `2.1.177 (Claude Code)` — matches the version pinned in `bridge.rs::build_bridge`. Pin the same in a `baude hook` doc-comment. `[VERIFIED: claude --version]`
- `ureq` 2.x, `serde_json` 1.x, `axum` 0.8 — all present in `Cargo.toml`, no version change needed. `[VERIFIED: Cargo.toml]`

## Package Legitimacy Audit

> No external packages are introduced in this phase. All required crates (`serde_json`, `ureq`, `axum`, `portable-pty`, `dirs`, `anyhow`) are pre-existing workspace dependencies shipped in v0.6.1.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| (none — no new deps) | — | — | — | — | — | — |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

## Architecture Patterns

### System Architecture Diagram

```
                          ┌──────────────────────────────────────┐
  baude spawns session →  │  seed .claude/settings.local.json     │
  (app.rs / manager.rs)   │  idempotent deep-merge baude hooks    │
                          │  (sentinel-guarded; user hooks kept)  │
                          └──────────────────────────────────────┘
                                          │
                       claude runs; each lifecycle event:
                                          │
              ┌───────────────────────────┴───────────────────────────┐
              │  claude invokes `baude hook` (command hook)             │
              │  hook JSON → stdin (session_id, hook_event_name, tool)  │
              └───────────────────────────┬───────────────────────────┘
                                          │  build_event() → {schema:1,ts,session_id,event,tool?}
                          ┌───────────────┴────────────────┐
                $BAUDE_EVENT_URL set?            $BAUDE_EVENT_URL unset (TUI-local)
                          │ yes                              │ no
                          ▼                                  ▼
          ureq POST $BAUDE_EVENT_URL          append line → /tmp/baude-events-<sid>.jsonl
                          │                                  │
                          ▼                                  │
        daemon POST /sessions/{id}/event                     │
        resolve baude id → claude sid                        │
        append line → /tmp/baude-events-<sid>.jsonl ◄────────┘  (both transports converge)
                          │
                          ▼
        meta.rs poll loop (no watch): read_event_tail() after read_bridge_file()
        offset-tracked tail → latest event + last_tool + state-source timestamp
                          │
                          ▼
        session.rs::status() precedence:  Hook (fresh) > SessionFile > Silence
        StateSource{Hook,SessionFile,Silence} label → SessionInfo / TUI overlay
```

### Recommended Project Structure
```
baude-core/src/
├── hook.rs          # NEW: build_event(&Value)->Value, merge_hook_settings(&Value)->Value,
│                    #      event_path(sid), append_event(); pure fns + temp-file tests
│                    #      (mirrors bridge.rs structure exactly)
├── meta.rs          # +read_event_tail(): offset-tracked tail of /tmp/baude-events-<sid>.jsonl
│                    #  +ClaudeMeta fields: hook_status:Option<(bool,u64)>, last_tool:Option<(String,u64)>
├── session.rs       # status() gains hook precedence + StateSource; +state_source()
baude/src/
├── main.rs          # dispatch `baude hook` (like `baude statusline`) before TUI init
├── app.rs           # add_session(): call seed before Pty::spawn
bauded/src/
├── api.rs           # +POST /sessions/{id}/event route
├── manager.rs       # spawn(): seed settings + inject $BAUDE_EVENT_URL into cmd; +ingest_event();
│                    #  SessionInfo: +state_source, +last_tool, +waiting_reason(optional)
```

### Pattern 1: Idempotent sentinel-guarded hook merge (HOOK-01)
**What:** Read existing `settings.local.json` (or `{}` if absent), deep-merge baude's hook entries into each event's array, tag each baude-inserted entry with a sentinel so re-runs are no-ops.
**When to use:** Every spawn (TUI and daemon). Must never duplicate, never clobber user entries.
**Example:**
```rust
// Source: pattern derived from bridge.rs (untyped Value) + Claude hooks schema (CITED).
const SENTINEL: &str = "baude hook"; // the command string IS the sentinel —
                                     // an entry is "ours" iff command == "baude hook".
const EVENTS: &[&str] = &["UserPromptSubmit", "Stop", "Notification", "PostToolUse"];

/// Pure: take whatever JSON is in settings.local.json, return the merged JSON.
/// Never panics on a minimal/odd file (Value accessors throughout).
fn merge_hook_settings(existing: &Value) -> Value {
    let mut root = existing.clone();
    if !root.is_object() { root = json!({}); }       // tolerate non-object/empty file
    let obj = root.as_object_mut().unwrap();
    let hooks = obj.entry("hooks").or_insert_with(|| json!({}));
    if !hooks.is_object() { *hooks = json!({}); }
    for ev in EVENTS {
        let arr = hooks.as_object_mut().unwrap()
            .entry(*ev).or_insert_with(|| json!([]));
        if !arr.is_array() { continue; }             // user put a non-array? leave it, skip
        let groups = arr.as_array_mut().unwrap();
        let already = groups.iter().any(|g| {
            g["hooks"].as_array().is_some_and(|inner|
                inner.iter().any(|h| h["command"].as_str() == Some(SENTINEL)))
        });
        if !already {
            groups.push(json!({
                "hooks": [ { "type": "command", "command": SENTINEL } ]
            }));
        }
    }
    root  // user's statusLine and any other keys are untouched (we only entered "hooks")
}
```
**Why the command-string sentinel:** no extra marker field to drift; idempotency check and the inserted entry share one source of truth. Re-spawn / `restart` re-runs the merge harmlessly.

### Pattern 2: Dual transport converging on one tail (HOOK-03)
**What:** `baude hook` picks POST-vs-append at runtime; the daemon route appends to the same `/tmp` file; `meta.rs` tails it in both worlds.
**When to use:** TUI-local has no daemon → append directly. Daemon-managed sessions POST so the daemon (which owns the `Session`/`ClaudeMeta`) can resolve the id and converge on the same file its poll loop already tails.
**Example:**
```rust
// baude/src/main.rs dispatch (mirrors the `statusline` arm):
if args.get(1).map(String::as_str) == Some("hook") {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let v = serde_json::from_str::<Value>(&input).unwrap_or(json!({}));
    let line = baude_core::hook::build_event(&v).to_string();  // pure
    let sid = v["session_id"].as_str().unwrap_or_default();
    if let Ok(url) = std::env::var("BAUDE_EVENT_URL") {
        let _ = ureq::post(&url).send_string(&line);           // daemon path; best-effort
    } else if !sid.is_empty() {
        let _ = baude_core::hook::append_event(sid, &line);    // TUI-local path
    }
    std::process::exit(0);   // NEVER block claude — always exit 0
}
```
```rust
// baude-core/src/hook.rs — O_APPEND keeps concurrent hook processes from clobbering.
pub fn append_event(sid: &str, line: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true).append(true).open(event_path(sid))?;
    writeln!(f, "{line}")
}
```

### Pattern 3: Precedence + StateSource label (HOOK-02, no-regression)
**What:** Layer a hook source ahead of the existing `claude_status` branch in `status()`, recording which source decided.
**When to use:** Every `status()` call. Hook wins only when present and fresh (staleness threshold at discretion — suggest reusing a small window like `BUSY_WINDOW_MS` semantics, or a few seconds; document the pinned value).
**Example:**
```rust
// session.rs — additive: the silence fallback (lines 117-122) is byte-for-byte preserved.
pub fn status_with_source(&self) -> (Status, StateSource) {
    if self.claude.is_exited() {
        return (Status::Exited, StateSource::Hook); // source irrelevant when exited
    }
    if let Some((busy, _at)) = self.meta.hook_status {          // NEW: highest precedence
        return (if busy { Status::Busy } else { Status::Waiting }, StateSource::Hook);
    }
    if let Some((busy, _)) = self.meta.claude_status {          // unchanged
        return (if busy { Status::Busy } else { Status::Waiting }, StateSource::SessionFile);
    }
    let last = self.claude.last_output_ms.load(Ordering::Relaxed);  // unchanged silence path
    let s = if now_ms().saturating_sub(last) < BUSY_WINDOW_MS { Status::Busy } else { Status::Waiting };
    (s, StateSource::Silence)
}
pub fn status(&self) -> Status { self.status_with_source().0 }   // keep the old API total
```

### Anti-Patterns to Avoid
- **`#[derive(Deserialize)]` on hook stdin or event lines** — breaks STL-02/HOOK schema-drift tolerance the moment Claude adds/renames a field. Use `Value` accessors.
- **Removing/cleaning up seeded hooks on session close** — locked out (race-prone, unnecessary). Seeding is additive + idempotent.
- **Blocking or non-zero exit in `baude hook`** — exit 2 is a *blocking* signal to Claude; any non-zero shows in the transcript. Always exit 0, best-effort (mirror `bridge::run`).
- **Deep-merging into the wrong settings file** — never touch committed `.claude/settings.json` or `~/.claude/settings.json`; only the gitignored per-session `.claude/settings.local.json`.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Incremental JSONL tailing | A fresh file-watch + line-buffer | Copy `read_transcript_tail()` (meta.rs:206-264) | Already handles partial trailing lines, offset tracking, transcript switch — proven |
| Concurrent appends from multiple hook processes | Lock files / mutex | `OpenOptions::append(true)` (O_APPEND) | OS guarantees atomic appends for small writes; no coordination needed |
| HTTP POST in the hook | Raw socket / hyper | `ureq::post` (already a dep) | Synchronous, tiny, already used by Web Push |
| Settings file gitignore | Adding to `.gitignore` ourselves | Nothing — Claude Code auto-gitignores `settings.local.json` | `[CITED: code.claude.com/docs/en/settings]` Claude configures git to ignore it on creation |
| Snake/camel field tolerance | Branching on schema versions | `Value` `.or_else()` fallbacks (bridge.rs::window pattern) | Established drift discipline |

**Key insight:** This phase has almost zero novel infrastructure. The risk is *discipline drift* (typed deserialization creeping in, clobbering user settings, regressing the silence fallback), not missing libraries.

## Runtime State Inventory

> This is a feature-addition phase, not a rename/refactor. Still, hook seeding mutates on-disk state in managed sessions — inventoried for completeness.

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | New per-session file `/tmp/baude-events-<sid>.jsonl` (created by hook append). Ephemeral, /tmp-scoped, keyed by Claude `session_id` — same lifecycle as the existing `/tmp/baude-usage-<sid>.json` bridge file. | None — auto-created, no migration; stale files harmlessly ignored by offset tail |
| Live service config | Each managed session's `.claude/settings.local.json` gains a baude hooks block. This file lives in the session cwd/worktree, NOT in baude's own repo. Idempotent re-merge on every spawn/restart. | Seed function must be idempotent (Pattern 1); no cleanup (locked) |
| OS-registered state | None — no Task Scheduler / launchd / pm2 registrations involve hook state. | None — verified by scope (no OS service touches this) |
| Secrets/env vars | New env var `$BAUDE_EVENT_URL` injected by the daemon into the spawned claude command string. Not a secret (loopback URL). No secret-store key. | Inject via the `sh -c` command string (Pitfall 4); document the var |
| Build artifacts | None — no package rename; new `hook` subcommand is a code addition compiled into the existing `baude` binary. | None |

## Common Pitfalls

### Pitfall 1: Clobbering a user's existing hooks or statusLine
**What goes wrong:** A naive `settings["hooks"] = baude_hooks` wipes user-defined hooks; replacing the whole file drops a user's `statusLine`.
**Why it happens:** Treating the merge as assignment instead of array-append into existing keys.
**How to avoid:** Pattern 1 — only `.entry().or_insert()` into `hooks.<event>` arrays; never touch sibling keys (`statusLine`, `permissions`, `env`, …). HOOK-01's explicit acceptance criterion; mirror the hard-won statusLine-seeding care.
**Warning signs:** A test that seeds into a file containing a user `statusLine` + a user hook and asserts both survive byte-intact must exist (this is a required Wave-0 test).

### Pitfall 2: Non-idempotent merge duplicates entries on restart
**What goes wrong:** `restart()` (manager.rs:322) and every TUI re-spawn re-runs seeding; without the sentinel check, baude's hook entry is appended again each time, eventually firing N copies per event.
**Why it happens:** No de-dup guard.
**How to avoid:** The command-string sentinel check in Pattern 1 — insert only if no group already contains a `command == "baude hook"` entry.
**Warning signs:** Required test: merge twice, assert exactly one baude entry per event array.

### Pitfall 3: Regressing the silence fallback / sidebar ordering
**What goes wrong:** Refactoring `status()` reorders or alters the silence branch; sidebar order (driven by status + waiting time) shifts; the "labeled fallback" becomes the silent default.
**Why it happens:** Rewriting `status()` instead of prepending a branch.
**How to avoid:** Pattern 3 keeps lines 114-122 of `session.rs` byte-identical, only prepending the `hook_status` branch. Add `StateSource` so a regression to silence is *observable*. Keep `status()` returning `Status` (total, unchanged signature) so all call sites (`auto_archive_tick`, `session_info`, TUI render) are untouched.
**Warning signs:** Required tests: with hook_status set → Hook source; with only claude_status → SessionFile source; with neither → Silence source + the existing silence behavior.

### Pitfall 4: `$BAUDE_EVENT_URL` never reaches the hook process
**What goes wrong:** `Pty::spawn` (pty.rs:34) only sets `TERM`/`COLORTERM` env — there is no env-map parameter. Setting it on baude's own process doesn't propagate to the claude child reliably, and the hook is a grandchild of the PTY shell.
**Why it happens:** Assuming env inheritance through the PTY without injecting it into the spawned command.
**How to avoid:** The daemon builds the claude command as `sh -c "<cmd>"`. Prefix the command string: `format!("BAUDE_EVENT_URL={url} exec {claude_cmd}")` (or set it before the `exec`). claude inherits it; the hook (claude's child) inherits it from claude. Alternatively add an env param to `Pty::spawn` — but the command-string prefix is the smaller, no-signature-change option matching the existing `exec`-string approach (manager.rs:233).
**Warning signs:** TUI-local sessions must NOT have `$BAUDE_EVENT_URL` (so they take the /tmp append path) — only the daemon injects it.

### Pitfall 5: Hook fires before claude_status appears, then "fights" it
**What goes wrong:** Hook says Busy on UserPromptSubmit; a stale session-file says Waiting; flicker if precedence is wrong or staleness unbounded.
**Why it happens:** No freshness bound on hook_status.
**How to avoid:** Hook strictly wins when present (locked precedence), but apply a staleness threshold (discretion) so a long-dead hook event doesn't pin a wrong state forever — fall through to SessionFile/Silence when stale. Stamp each event with `ts` and compare against `now_unix_ms()`.

## Code Examples

### Build the normalized event line (pure, testable — mirrors `build_bridge`)
```rust
// baude-core/src/hook.rs
// Field names verified against Claude Code hooks schema, CLI v2.1.177
// (snake_case: session_id, hook_event_name, tool_name). Untyped Value
// accessors → unknown/absent keys are tolerated (HOOK schema-drift discipline).
use serde_json::{json, Value};
use crate::meta::now_unix_ms;

pub fn build_event(v: &Value) -> Value {
    json!({
        "schema": 1,
        "ts": now_unix_ms(),
        "session_id": v["session_id"].as_str(),
        "event": v["hook_event_name"].as_str(),     // "UserPromptSubmit"|"Stop"|"Notification"|"PostToolUse"
        "tool": v["tool_name"].as_str(),            // present only on tool events
        // notification_type carried so Phase 4 (PERM) can distinguish permission vs idle
        "notification_type": v["notification_type"].as_str(),
    })
}
```

### Consume side: event → state mapping in `meta.rs::read_event_tail`
```rust
// Mirrors read_transcript_tail (offset-tracked, complete-lines-only). Slots in
// after read_bridge_file() in poll(). Untyped Value; never branches on schema.
for line in buf[..consumed].lines() {
    let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
    let ts = v["ts"].as_u64().unwrap_or(0);
    match v["event"].as_str() {
        Some("UserPromptSubmit") => self.hook_status = Some((true, ts)),   // Busy
        Some("Stop")             => self.hook_status = Some((false, ts)),  // Waiting/done
        Some("Notification")     => self.hook_status = Some((false, ts)),  // Waiting (needs input)
        Some("PostToolUse")      => {
            self.hook_status = Some((true, ts));                            // still Busy
            if let Some(t) = v["tool"].as_str() {
                self.last_tool = Some((t.to_string(), ts));                 // record last tool
            }
        }
        _ => {}
    }
}
self.offset_events += consumed as u64;
```

### Daemon route (HOOK-03) — slots into `api.rs::router()` after `/stream`
```rust
.route("/sessions/{id}/event", post(post_event))
// ...
async fn post_event(
    State(state): State<Shared>,
    Path(id): Path<u64>,
    body: String,                 // one JSON event line (already built by `baude hook`)
) -> StatusCode {
    let mut m = lock(&state);
    // resolve baude id -> claude session_id (Session.meta.session_id), append to /tmp file
    match m.ingest_event(id, &body) {           // append to /tmp/baude-events-<sid>.jsonl
        Ok(()) => StatusCode::NO_CONTENT,       // 204 (locked)
        Err(_) => StatusCode::NOT_FOUND,
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| PTY-output-silence heuristic (`BUSY_WINDOW_MS`) as primary state | First-party Claude hook events as primary; silence labeled fallback | This phase (v0.7) | Accurate working/waiting the instant a prompt is submitted; no 2s silence lag |
| Claude session-file `status` as authoritative | Hook > SessionFile > Silence precedence | This phase | Hooks are fresher and event-driven vs. polled file status |
| `Notification` permission prompts inferred from silence | `notification_type == "permission_prompt"` (verified v2.1.177) | This phase / Phase 4 | Unlocks a real "waiting for permission" reason; foundation for PERM-04 push |

**Deprecated/outdated:**
- Nothing removed — the silence path is *retained* as a labeled fallback (no-regression constraint). Older "blessed" examples online still configure hooks in `~/.claude/settings.json`; baude deliberately uses per-session `.claude/settings.local.json` instead (correct for the gitignored, session-scoped seeding model).

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | A `type:"command"` hook with `command:"baude hook"` and no `matcher` fires for non-tool events (UserPromptSubmit/Stop/Notification) exactly once per occurrence. Docs show matcher is for tool events; non-tool events with a single no-matcher group is the documented shape but not directly demonstrated for all four. | Standard Stack / Pattern 1 | Low — if a `matcher:"*"` wrapper is required, add it uniformly; verifiable by a one-shot manual hook test at build time |
| A2 | The `baude` binary is on PATH inside the managed session (so `command:"baude hook"` resolves). Phase 1's statusLine seeding presumably already relies on `baude statusline` resolving — confirm the same PATH assumption holds, or seed an absolute path to the running binary. | Pattern 1 | Medium — if `baude` isn't on the session PATH, hooks silently never fire; mitigate by seeding `std::env::current_exe()` absolute path as the command (recommend this over bare `baude hook`) |
| A3 | Staleness threshold for hook_status precedence can reuse a small fixed window (a few seconds) without flicker against session-file status. | Pattern 3 / Pitfall 5 | Low — tunable; wrong value only causes brief mislabel, never a crash |
| A4 | Daemon-injected `$BAUDE_EVENT_URL` should point at the daemon's own loopback bind (`127.0.0.1:8642` default) so the hook POSTs back to the daemon. | Pattern 2 / Pitfall 4 | Low — if the daemon binds a non-loopback tailnet addr, derive the URL from the actual bind; the hook only needs reachability from the session host (same host) |

**Note:** A2 is the highest-leverage assumption — recommend the planner add a task to seed `current_exe()` absolute path rather than the bare `baude hook` string, and a manual smoke task to confirm a hook actually fires end-to-end against CLI 2.1.177.

## Open Questions

1. **Does Phase 1's existing statusLine seeding already write `.claude/settings.local.json`, or `settings.json`, or the container path?**
   - What we know: CONTEXT references "reuse the container's statusLine-seeding path"; bridge.rs documents the `statusLine` block but the *seeding* site wasn't located in this research pass.
   - What's unclear: whether a settings-seeding helper already exists to extend vs. build fresh.
   - Recommendation: planner's first task should locate any existing settings-seeding code (grep `settings.local.json` / `statusLine` write sites in `app.rs`/`manager.rs`/container scripts) and extend it, so hooks and statusLine seed through one merge path.

2. **Per-session `.claude/settings.local.json` for worktrees — is cwd the worktree dir or the repo root?**
   - What we know: worktree sessions set cwd to the worktree dir (manager.rs:190); Claude reads `.claude/settings.local.json` relative to the project dir.
   - What's unclear: whether Claude resolves project settings from the worktree cwd or the main repo root.
   - Recommendation: seed into the session's actual cwd (worktree dir) — matches where claude runs; verify with one worktree smoke test.

3. **`waiting_reason` on SessionInfo now vs. Phase 4?**
   - What we know: `notification_type:"permission_prompt"` is available now; PERM-04 wants a `waiting_reason` field.
   - What's unclear: whether to surface it this phase or defer the field.
   - Recommendation: capture `notification_type` in the event + `meta` now (cheap), but only render the minimal "waiting (needs input)" label this phase; defer the structured `waiting_reason` API field to Phase 4 to avoid scope creep.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Claude Code CLI | Hook firing (all of HOOK-02) | ✓ | 2.1.177 | — (the silence fallback covers hooks-unavailable at runtime) |
| `cargo` / Rust toolchain | Build + CI gates | ✓ | (workspace edition 2021) | — |
| `ureq` 2.x | daemon POST transport | ✓ (in Cargo.toml) | 2 | — |

**Missing dependencies with no fallback:** none
**Missing dependencies with fallback:** Claude Code hooks at *runtime* — if a deployed session has hooks disabled/unavailable, the dual-source silence fallback (existing, unchanged) keeps state correct (this is HOOK-02's labeled-fallback requirement, not a build blocker).

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` (cargo test) — no external test crate |
| Config file | none — tests live next to production code (`mod tests` blocks) |
| Quick run command | `cargo test -p baude-core hook::` / `cargo test -p baude-core meta::` |
| Full suite command | `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| HOOK-01 | Merge into file with user statusLine + user hook → both survive intact | unit | `cargo test -p baude-core hook::preserves_user_settings` | ❌ Wave 0 |
| HOOK-01 | Merge twice → exactly one baude entry per event (idempotent) | unit | `cargo test -p baude-core hook::merge_idempotent` | ❌ Wave 0 |
| HOOK-01 | Merge into empty/minimal/non-object file → never panics, valid output | unit | `cargo test -p baude-core hook::merge_minimal_ok` | ❌ Wave 0 |
| HOOK-02 | `build_event` maps each hook_event_name → correct schema-1 line | unit | `cargo test -p baude-core hook::build_event_maps_events` | ❌ Wave 0 |
| HOOK-02 | Event tail: UserPromptSubmit→Busy, Stop→Waiting, PostToolUse→Busy+last_tool | unit | `cargo test -p baude-core meta::event_tail_drives_state` | ❌ Wave 0 |
| HOOK-02 | Precedence: hook>session-file>silence; StateSource labeled correctly | unit | `cargo test -p baude-core session::state_source_precedence` | ❌ Wave 0 |
| HOOK-02 | No-regression: with no hook events, silence fallback behaves as v0.6.1 | unit | `cargo test -p baude-core session::silence_fallback_unchanged` | ❌ Wave 0 |
| HOOK-02 | Notification carries `notification_type:"permission_prompt"` through to meta | unit | `cargo test -p baude-core meta::notification_permission_captured` | ❌ Wave 0 |
| HOOK-03 | `POST /sessions/{id}/event` appends to /tmp file, returns 204; same consume path | integration | `cargo test -p bauded api::post_event_appends` | ❌ Wave 0 |
| HOOK-03 | TUI-local append (no `$BAUDE_EVENT_URL`) writes the same /tmp file | unit | `cargo test -p baude-core hook::append_event_writes_tmp` | ❌ Wave 0 |
| HOOK-02 (manual) | End-to-end: a real claude session with seeded hooks flips state without the silence timer | manual | smoke against CLI 2.1.177 (A1/A2 verification) | ❌ Wave 0 (UAT) |

### Sampling Rate
- **Per task commit:** `cargo test -p <crate> <mod>::` for the touched module (sub-second).
- **Per wave merge:** `cargo test` (full workspace test run).
- **Phase gate:** `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` all green (the CI triad) before `/gsd-verify-work`.

### Wave 0 Gaps
- [ ] `baude-core/src/hook.rs` `mod tests` — covers HOOK-01 (merge preserve/idempotent/minimal) + HOOK-02 (`build_event`) + HOOK-03 (append) — pure-fn + temp-file tests mirroring `bridge.rs`/`meta.rs` fixtures
- [ ] `baude-core/src/meta.rs` `mod tests` additions — `read_event_tail` state mapping + last_tool + notification_type (reuse the `feed_transcript`-style temp-file helper)
- [ ] `baude-core/src/session.rs` `mod tests` — StateSource precedence + silence-fallback no-regression (session.rs currently has no test module — adding one is a Wave-0 item)
- [ ] `bauded/src/api.rs` test — `POST /sessions/{id}/event` (reuse the in-process `axum::serve` harness at api.rs:554)
- [ ] Manual UAT smoke (A1/A2) — one real session, confirm hooks fire and state flips without silence timer (record as end-of-phase human-verify per config `human_verify_mode:"end-of-phase"`)

*Framework install: none needed — Rust built-in test harness already in use across the workspace.*

## Security Domain

> `security_enforcement: true`, `security_asvs_level: 1`, `security_block_on: high`. Security model is "bind the VPN/tailnet interface; no auth layer" (PROJECT.md). New endpoints inherit this.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | Single-user, tailnet-bound by design (structural, not deferred) — `POST /sessions/{id}/event` inherits the existing no-auth-but-VPN model like every other route |
| V3 Session Management | no | No web sessions/cookies introduced |
| V4 Access Control | no | No multi-user / RBAC; out of scope structurally |
| V5 Input Validation | **yes** | `POST /sessions/{id}/event` body and hook stdin parsed with `serde_json::Value` (no `Deserialize` panics); reject non-JSON gracefully (best-effort, never 500). Path `{id}` is `u64` (axum typed extractor rejects non-numeric). |
| V6 Cryptography | no | No new crypto; no secrets handled (loopback `$BAUDE_EVENT_URL` is not a secret) |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Untrusted hook stdin / event body crashes the reader | Denial of Service | Untyped `Value` accessors + `unwrap_or` defaults; `baude hook` always exits 0; daemon returns 404 not 500 on bad body |
| Event file path injection via `session_id` (`/tmp/baude-events-<sid>.jsonl`) | Tampering | `session_id` comes from Claude's own hook payload (trusted local source); path is `format!`-built under `/tmp` only. Mirrors the existing `bridge_path(sid)` pattern — no path traversal surface beyond what v0.6.1 already accepts. Consider rejecting `sid` containing `/` or `..` for defense-in-depth (low risk, local-only). |
| New unauthenticated POST endpoint | Spoofing / Tampering | Inherits tailnet binding (`127.0.0.1:8642` default / VPN interface); no new exposure beyond the existing REST surface. ASVS L1 + VPN model is the accepted project baseline. |
| Seeded hook runs arbitrary `command` in user sessions | Elevation of Privilege | baude only inserts its own `baude hook` command; it never copies untrusted commands. User's own hooks are preserved but not introduced by baude. |

**Block-on-high check:** No high-severity findings. The one defense-in-depth note (reject `/`/`..` in `session_id` before building the /tmp path) is low severity and optional, consistent with the existing `bridge_path` posture.

## Sources

### Primary (HIGH confidence)
- `claude --version` → **2.1.177 (Claude Code)** — pins the verified hook schema version `[VERIFIED]`
- Codebase: `baude-core/src/{bridge.rs,meta.rs,session.rs,pty.rs}`, `baude/src/{main.rs,app.rs}`, `bauded/src/{api.rs,manager.rs}`, `Cargo.toml` (all crates) — read directly this session `[VERIFIED: codebase grep/read]`
- code.claude.com/docs/en/hooks — hooks config structure, stdin field schema, `notification_type` for permission prompts `[CITED]`

### Secondary (MEDIUM confidence)
- code.claude.com/docs/en/settings — settings precedence; `.claude/settings.local.json` is gitignored on creation, local override layer `[CITED]`

### Tertiary (LOW confidence)
- General community guides (morphllm, eesel, claudefast) corroborating event names — used only to triangulate, superseded by the official docs above

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new deps; all crates verified present in Cargo.toml
- Hook schema: HIGH — official docs + installed CLI version match the version pinned in bridge.rs
- Architecture / integration points: HIGH — exact line numbers read from current source
- Pitfalls: HIGH — derived from reading the actual `status()`, `Pty::spawn`, and `restart()` code
- A2 (binary on PATH): MEDIUM — recommend seeding `current_exe()` absolute path + a manual smoke test

**Research date:** 2026-06-15
**Valid until:** 2026-07-15 for the codebase facts; hook schema valid until the next Claude Code minor that changes hook payloads (drift-tolerant by design — re-verify `claude --version` at execution time and update the pinned comment if it advanced past 2.1.177).

## RESEARCH COMPLETE

**Phase:** 2 - Hook-Driven Status
**Confidence:** HIGH

### Key Findings
- **No new dependencies.** Everything needed (`serde_json::Value`, `std::fs` O_APPEND, `ureq`, `axum`, the `read_transcript_tail()` offset-tail pattern) already ships in v0.6.1. The phase is internal engineering + one external contract (Claude hooks).
- **Hook schema verified against installed CLI 2.1.177** (matches the version pinned in `bridge.rs`). `Notification` fires for permission prompts via `notification_type:"permission_prompt"`; common stdin fields are `session_id`/`hook_event_name`/`cwd`; tool events add `tool_name`. `.claude/settings.local.json` is a documented, auto-gitignored local-override layer — correct seed target.
- **No-regression path is small and additive:** `session.rs::status()` is 16 lines with one branch; prepend a `hook_status` branch + `StateSource` label, leave the silence fallback byte-identical. Keep `Status` enum and `status()` signature unchanged.
- **Build pure transforms in baude-core** (`build_event`, `merge_hook_settings`, `append_event`) mirroring `build_bridge`; dispatch `baude hook` from `baude/src/main.rs` (which has `ureq` for the POST); tail the /tmp file in `meta.rs` after `read_bridge_file()`.
- **Two highest-leverage risks:** (A2) seed the hook command as `current_exe()` absolute path rather than bare `baude hook` so it resolves on the session PATH; (Pitfall 4) inject `$BAUDE_EVENT_URL` by prefixing the daemon's `sh -c` command string since `Pty::spawn` has no env-map parameter.

### File Created
`.planning/phases/02-hook-driven-status/02-RESEARCH.md`

### Confidence Assessment
| Area | Level | Reason |
|------|-------|--------|
| Standard Stack | HIGH | No new deps; all crates verified in Cargo.toml |
| Architecture | HIGH | Exact integration line numbers read from current source |
| Pitfalls | HIGH | Derived from reading actual status()/Pty::spawn/restart code |

### Open Questions
1. Locate Phase 1's existing statusLine-seeding site to extend one merge path (planner's first task).
2. Worktree settings target — seed into worktree cwd; confirm with one smoke test.
3. `waiting_reason` API field — capture `notification_type` now, defer the structured field to Phase 4.

### Ready for Planning
Research complete. Planner can now create PLAN.md files.
