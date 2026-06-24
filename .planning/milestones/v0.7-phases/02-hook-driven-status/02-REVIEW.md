---
phase: 02-hook-driven-status
reviewed: 2026-06-15T19:41:31Z
depth: standard
files_reviewed: 11
files_reviewed_list:
  - baude-core/src/hook.rs
  - baude-core/src/lib.rs
  - baude-core/src/meta.rs
  - baude-core/src/session.rs
  - baude/src/app.rs
  - baude/src/main.rs
  - baude/src/remote.rs
  - baude/src/ui.rs
  - bauded/src/api.rs
  - bauded/src/manager.rs
  - bauded/src/notify.rs
findings:
  critical: 0
  warning: 6
  info: 1
  total: 7
status: issues_found
---

# Phase 2: Code Review Report

**Reviewed:** 2026-06-15T19:41:31Z
**Depth:** standard
**Files Reviewed:** 11
**Status:** issues_found

## Summary

Reviewed the Phase 2 (Hook-Driven Status) change set against the phase's design contract: path-traversal sanitization, untyped `serde_json::Value` resilience, no-regression of the v0.6.1 silence fallback, idempotent settings merge, offset-tracked event tailing, and the `$BAUDE_EVENT_URL` injection path.

The security-sensitive surfaces hold up well. `event_path` sanitization (`replace("..","_").replace('/',"_")`) is correct and non-bypassable — neither replacement can re-introduce the other's forbidden substring, so the result is always a single component under `/tmp`. The `Path<u64>` extractor rejects non-numeric ids at the framework layer, unknown sessions return 404 via `not_found`, and every hook/event accessor uses untyped `Value` so malformed input is skipped rather than panicking. `merge_hook_settings` is idempotent and non-clobbering, with thorough tests. `decide_status` preserves the v0.6.1 silence/session-file logic byte-for-byte and only prepends the fresh-hook tier; the `Status` enum is unchanged and `status()` stays total. The `BAUDE_EVENT_URL=` shell prefix uses a fully numeric `event_url(id)`, so there is no command-injection vector there. `cargo build`, `cargo clippy --all-targets`, and the new unit tests all pass.

No blockers. The findings below are correctness, data-loss-under-misconfiguration, robustness, and maintainability issues — several of which undermine guarantees the phase explicitly set out to provide ("never block Claude", "never lose events", "make a silence regression observable").

## Warnings

### WR-01: `BAUDE_EVENT_URL` is unset on the resume `--continue` fallback path

**File:** `bauded/src/manager.rs:265-271`
**Issue:** For resume sessions the command becomes:
```
BAUDE_EVENT_URL=<url> claude --continue 2>/dev/null || exec claude
```
A shell environment-assignment prefix applies **only to the single command it prefixes** — here `claude --continue`. When `--continue` exits non-zero (a fresh directory with no prior conversation, which is the common case the `||` exists to handle), the fallback `exec claude` runs **without** `BAUDE_EVENT_URL`. Its hook children then take the file-append branch in `main.rs` instead of the daemon POST branch. Events still happen to reach the daemon because the daemon tails the same `/tmp` file, so this degrades rather than breaks — but the injected-URL transport silently does not apply on the most common resume path, contradicting the comment's claim that "claude and its hook child both inherit it."
**Fix:** Make the assignment apply to the whole command group, e.g.:
```rust
let inner = if resume {
    format!("{base_cmd} --continue 2>/dev/null || exec {base_cmd}")
} else {
    format!("exec {base_cmd}")
};
// export so it survives the `||` fallback and any sub-exec
let cmd = format!("export BAUDE_EVENT_URL={}; {inner}", event_url(id));
```
(or wrap `inner` in `{ ... }` after the assignment). Confirm interaction with `exec`.

### WR-02: Hardcoded port in `event_url` silently drops all events under a custom `--bind`

**File:** `bauded/src/manager.rs:99-101` (`event_url`) with `baude/src/main.rs:51-54`
**Issue:** `event_url` hardcodes `http://127.0.0.1:8642/...`, but the daemon's listen address is configurable via `--bind` / `BAUDED_BIND` (`main.rs:49-53`). When `bauded` is started on any non-default port, the daemon-spawned hook child has `BAUDE_EVENT_URL` set to the dead `:8642` port. In `main.rs:50-55` the routing is mutually exclusive — when the env var **is** present the code POSTs and never falls back to `append_event`. So the POST fails (connection refused) and the event is **silently lost**; there is no file-tail fallback to catch it. The in-code comment frames this as an accepted out-of-scope limitation, but the consequence is silent data loss of the hook stream — the exact signal Phase 2 exists to deliver — not merely "custom port not honored."
**Fix:** Either thread the real bind addr into `Manager` and build the URL from it, or (cheaper) make the hook fall back to the file-append transport when the POST fails so events are never lost:
```rust
if let Ok(url) = std::env::var("BAUDE_EVENT_URL") {
    let posted = ureq::post(&url).send_string(&line).is_ok();
    if !posted && !sid.is_empty() {
        let _ = baude_core::hook::append_event(sid, &line);
    }
} else if !sid.is_empty() {
    let _ = baude_core::hook::append_event(sid, &line);
}
```

### WR-03: `offset_events` is never reset when `session_id` changes — stale offset can permanently drop new events

**File:** `baude-core/src/meta.rs:340-399` (`read_event_tail`) vs. `meta.rs:202-213` (transcript-switch reset)
**Issue:** `read_event_tail` keys its file path off `self.session_id` but tracks a persistent `self.offset_events`. `apply_session_file` (`meta.rs:184-186`) reassigns `session_id` unconditionally whenever Claude's session file reports a `sessionId`, so the resolved event-file **path can change** mid-session (e.g. a resumed/rotated Claude session). When the path flips to a different (and possibly shorter) file, `offset_events` keeps the old value; the guard `if len <= self.offset_events { return; }` (line ~353) then short-circuits forever and the new session's events are never processed. The transcript tail handles exactly this hazard by resetting `offset` (and friends) on a path change at `meta.rs:202-213`; the event tail has no equivalent reset. Result: `hook_status`/`last_tool`/`last_notification` go stale and the UI silently falls through to the session-file/silence sources with no new hook signal.
**Fix:** Track the resolved event-file path (or the `session_id` it was computed from) alongside `offset_events`, and reset the offset when it changes:
```rust
let path = PathBuf::from(crate::hook::event_path(sid));
if self.event_path.as_ref() != Some(&path) {
    self.event_path = Some(path.clone());
    self.offset_events = 0;
}
```

### WR-04: Hook POST has no timeout — a stalled connection blocks Claude

**File:** `baude/src/main.rs:52-53`
**Issue:** `ureq::post(&url).send_string(&line)` uses ureq's default agent, which does not impose a tight overall timeout. The `baude hook` process runs synchronously in Claude Code's critical path (Claude waits for the hook to exit before continuing), and the whole module is premised on "ALWAYS exit 0 so a hook failure never blocks Claude" (`main.rs:39-41`). A loopback peer that accepts the connection but stalls (or a wedged daemon) would hang the POST and thus block Claude indefinitely, defeating that guarantee. Connection-refused fails fast, but a slow/hung accept does not.
**Fix:** Use a bounded agent so the hook cannot hang:
```rust
let agent = ureq::AgentBuilder::new()
    .timeout_connect(std::time::Duration::from_millis(500))
    .timeout(std::time::Duration::from_secs(2))
    .build();
let _ = agent.post(&url).send_string(&line);
```

### WR-05: `decide_status` labels every `Exited` session as `StateSource::Hook`

**File:** `baude-core/src/session.rs:140-142`
**Issue:** The exited branch returns `(Status::Exited, StateSource::Hook)` unconditionally, even when no hook event ever drove the session. This `state_source` value is surfaced to the UI (`bauded/src/manager.rs:582`, `baude/src/ui.rs`) for the explicit purpose of "surfacing a regression to the silence fallback." An exited session that never saw a hook will render `state: hook`, which is misleading for the very observability mechanism this field exists to provide. The `Status` is correct; only the source label is wrong.
**Fix:** Carry the source from the underlying tier instead of fabricating `Hook`, or return a neutral label. Simplest: compute the source as if not exited and override only the `Status`:
```rust
let (mut st, src) = /* the hook/session-file/silence decision below */;
if exited { st = Status::Exited; }
(st, src)
```
or add a dedicated `StateSource::Exited` variant rather than reusing `Hook`.

### WR-06: `seed_session_hooks` duplicated verbatim across two crates

**File:** `baude/src/app.rs:43-52` and `bauded/src/manager.rs:589-601`
**Issue:** The file-IO seeding wrapper (create `.claude`, read+parse `settings.local.json`, call `merge_hook_settings`, write back) is duplicated byte-for-byte in the TUI (`app.rs`) and daemon (`manager.rs`) crates. The pure merge already lives in `baude_core::hook`; only this wrapper is copied. Two copies that must stay in lockstep invite divergence — e.g. a future fix to the read/parse/write semantics applied to one and not the other.
**Fix:** Hoist a single `baude_core::hook::seed_settings(cwd: &Path)` (best-effort, returning `()` or a `Result` the callers ignore) and have both crates call it.

## Info

### IN-01: `post_event` body with embedded newlines appends multiple event lines

**File:** `bauded/src/api.rs:233-240` and `bauded/src/manager.rs:357-367` (`ingest_event`)
**Issue:** The POST body is taken as a raw `String` and appended via `append_event(sid, body.trim_end())` (`writeln!`). `trim_end()` only strips trailing whitespace; an embedded `\n` mid-body survives and produces multiple physical lines in `/tmp/baude-events-<sid>.jsonl`, each parsed independently by `read_event_tail`. In the loopback/single-user/no-auth model this is acceptable, but it means one POST can inject several state-changing events. Each injected line must still be valid JSON to affect state (malformed lines are skipped), so the blast radius is small.
**Fix:** Reject or collapse embedded newlines before appending, e.g. take only the first line or replace `\n`/`\r` with spaces:
```rust
let line = body.lines().next().unwrap_or("").trim_end();
baude_core::hook::append_event(&sid, line)...
```

---

_Reviewed: 2026-06-15T19:41:31Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
