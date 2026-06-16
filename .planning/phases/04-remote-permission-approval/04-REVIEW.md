---
phase: 04-remote-permission-approval
reviewed: 2026-06-15T00:00:00Z
depth: standard
files_reviewed: 13
files_reviewed_list:
  - baude-core/src/permission.rs
  - baude-core/src/lib.rs
  - baude-core/src/session.rs
  - baude/src/app.rs
  - baude/src/main.rs
  - baude/src/remote.rs
  - baude/src/ui.rs
  - bauded/src/api.rs
  - bauded/src/main.rs
  - bauded/src/manager.rs
  - bauded/src/notify.rs
  - bauded/web/app.js
  - bauded/web/sw.js
findings:
  critical: 0
  warning: 4
  info: 3
  total: 7
status: issues_found
---

# Phase 4: Code Review Report

**Reviewed:** 2026-06-15
**Depth:** standard
**Files Reviewed:** 13
**Status:** issues_found

## Summary

Phase 4 wires the security-critical remote tool-permission approval path: a
per-deploy `BAUDE_PERMISSION_MODE` flag selector, a hand-rolled JSON-RPC
`permission-mcp` stdio bridge in both binaries, a daemon-mediated
pending/decision store with a long-poll, and a PWA approve/deny card.

I traced every security-critical claim against the actual changed lines and the
core invariants HOLD:

- **PERM-01 fail-safe** (`permission_flag_for`, permission.rs:67-80): only the
  exact literal `Some("prompt")` yields the prompt flag; `None`, `"skip"`, any
  unrecognized value, and any case-variant fall through to
  `--dangerously-skip-permissions`. There is no path where an unrecognized value
  yields prompt or yields neither flag. No-double-add is enforced. Exactly one of
  `{skip, prompt, ""}` is returned, never both. Confirmed safe.
- **Deny-on-timeout / fail-closed** (`decide_with_timeout` permission.rs:335-342,
  `build_approve_result` permission.rs:271-295, the bridge loop in both
  main.rs): no-daemon → deny, deadline → deny, malformed/empty/non-matching
  daemon response → no decision recorded → keep polling → deny on deadline. Any
  non-`"allow"` string coerces to deny, and the echoed input is dropped on a deny
  coercion (no approval-payload leak). I found **no auto-allow path** — no
  CRITICAL.
- **Decision validation (V5)** (`post_permission` api.rs:371-399): `decision`
  must be exactly `allow`|`deny` or it is a 400; unknown id → 404; no 500/panic
  path. `resolve_pending` (manager.rs:565) additionally coerces any non-`allow`
  to deny as defense-in-depth. Confirmed.
- **Wait outside the lock** (`get_permission` api.rs:293-310): the manager
  `MutexGuard` is dropped (the `{ … }` block ends at :300) before
  `notify.notified().await`. No lock is held across the await. Confirmed.
- **JSON-RPC framing** (`parse_frame`/`parse_content_length_frame`/
  `parse_tool_call`): all untyped `serde_json::Value`, all `?`/`.ok()`/
  `unwrap_or_default`; negative/overflowing lengths use `checked_add` and
  `parse::<usize>` → `None`. No panic path on malformed/partial input.
- **Dual-binary** `permission-mcp` arm present in BOTH `baude/src/main.rs:154`
  and `bauded/src/main.rs:146`, each dispatched before the daemonize/TUI
  fall-through. No Phase-2 trap.
- **PWA XSS** (app.js:683-693): `pp.tool` and `permSummary(pp.input)` are both
  `esc()`'d before `innerHTML`. `esc()` covers `& < > " '`. Deny POSTs
  `{decision:"deny"}` only (does not kill the session). `sw.js` cache bumped to
  `baude-v4`.

The findings below are robustness/quality issues, none of which break the
deny-default security posture.

## Warnings

### WR-01: `permission-mcp` bridge can serialize itself across sessions in the TUI binary

**File:** `baude/src/main.rs:91-121`, `bauded/src/main.rs:78-108`
**Issue:** The bridge's long-poll loop sleeps `500ms` between GETs and the GET
itself blocks up to `wait=5` seconds. Each `tools/call` blocks the single stdio
loop in `run_permission_mcp` until the human decides or the 120s deadline denies.
This is correct for one session, but the daemon's per-session bridge is a
*separate process per session*, so there's no cross-session stall there. In the
TUI (`baude`) path, however, `prompt` mode is wired but the TUI has **no
in-process resolver UI** — a TUI-spawned `prompt`-mode session's `baude
permission-mcp` child will `permission_url_from_event_url($BAUDE_EVENT_URL)`,
and the TUI never sets `$BAUDE_EVENT_URL` (per app.rs:464 comment "only the
daemon injects that var"). So a TUI `prompt`-mode session fails closed to deny
for **every** tool call with no way to approve — Claude is effectively bricked in
that mode under the TUI. This is fail-safe (deny, not allow) but is a silent
footgun: enabling `BAUDE_PERMISSION_MODE=prompt` under the bare TUI denies all
tools with no operator-visible reason.
**Fix:** Either gate `is_prompt_mode()` `.mcp.json` seeding in the TUI path
(app.rs:469-471) behind a daemon-presence check, or log/emit a one-time warning
at spawn when `prompt` mode is active but no event URL will be injected, so the
operator learns why tools are being denied rather than discovering it as a hang.

### WR-02: Bridge ureq read timeout (5s) equals the server long-poll window (`wait=5`) — guaranteed periodic spurious timeouts

**File:** `baude/src/main.rs:83-86,108`, `bauded/src/main.rs:70-73,95`
**Issue:** The agent is built with `.timeout(Duration::from_secs(5))` (overall
read timeout) and the long-poll GET passes `.query("wait", "5")`. The daemon
(`get_permission`) blocks up to its clamped `wait` (5s here) when a request is
pending with no decision. When the human takes longer than ~5s to decide, the
client read times out at the exact same 5s boundary the server is holding,
producing a race where the GET frequently errors out (`let Ok(resp) = …` falls
through) instead of returning cleanly. It still re-polls and stays correct, but
it converts every long-poll into a timeout-then-retry, defeating the long-poll's
purpose and adding load.
**Fix:** Make the client read timeout strictly larger than the server wait, e.g.
build the agent with `.timeout(Duration::from_secs(timeout_s.min(60) + 2))` or
keep `wait` smaller than the client timeout (e.g. `.query("wait", "4")` with a
5s client timeout). The relationship `client_timeout > server_wait` must hold.

### WR-03: `scope` is accepted, stored, and serialized but never enforced — "allow for session" silently degrades to per-call

**File:** `bauded/src/api.rs:380`, `bauded/src/manager.rs:552-580`, `app.js:372-376`
**Issue:** `post_permission` accepts `scope`, `resolve_pending` stores it in the
decision, and `PermissionView`/`PermissionDecision` serialize it — but nothing
reads `scope` to suppress future prompts. Each new `tools/call` mints a fresh
`request_id` and a fresh pending request, so an `{decision:"allow",
scope:"session"}` only ever allows the single in-flight call. The PWA card
(app.js:368-385) always POSTs `{decision}` with no scope, so the field is dead in
practice. This fails safe (extra prompts, never an unintended allow), but it is
dead/misleading API surface: a client author reading `PermissionView.scope`
would reasonably assume session-scoped allow works.
**Fix:** Either implement scope enforcement (record an allowed tool/scope on the
session and have `set_pending`/the resolver short-circuit a matching subsequent
request) or remove the `scope` field from the request/response types and the
manager until it is wired, to avoid a false contract.

### WR-04: Long-poll missed-wakeup window when a decision lands between unlock and `notified()` registration

**File:** `bauded/src/api.rs:293-309`
**Issue:** The code reads `(notify, pending, decision)` under the lock, drops the
lock, then re-checks `pending.is_some() && decision.is_none()` and only then
calls `notify.notified().await`. The in-code comment (api.rs:304-307)
acknowledges the race: if `resolve_pending` fires `notify_waiters()` in the
window *after* the lock is dropped at :300 but *before* `notified()` is
registered at :308, that wakeup is lost. `tokio::sync::Notify` only stores ONE
permit and only for a waiter registered at notify time; a `notify_waiters()`
with no registered waiter is dropped. The await would then block for the full
`wait` (up to 30s) before the bridge re-polls and picks up the already-recorded
decision. Correctness is preserved (the post-await re-read at :313-314 sees the
decision, and the timeout bounds it) but a resolved permission can appear to hang
for up to the wait window on the unlucky interleaving.
**Fix:** Register the future before dropping the lock, or re-check `decision`
*after* obtaining the `Notified` future but before awaiting (acquire the
`Notified` via `notify.notified()`, pin it, then re-read decision under a brief
lock; if still none, await). Alternatively bound `wait` to a few seconds so the
worst-case hang is small (it is currently clamped to 30s).

## Info

### IN-01: `dispatch_rpc` returns `-32602` for an unknown tool but the doc-comment says `-32601`

**File:** `baude-core/src/permission.rs:380-409`
**Issue:** The `dispatch_rpc` doc (permission.rs:371-384) lists `tools/call`
handling and the module narrative implies a single error code, but an unknown
tool name returns `-32602` (api.rs analog) while an unknown method returns
`-32601`. The test at permission.rs:999 pins `-32602` for the unknown-tool case,
so behavior is intentional, but `-32602` (Invalid params) is a slightly odd
choice for "unknown tool" vs `-32601` (Method not found). Cosmetic; no functional
impact since Claude only ever calls the one registered `approve` tool.
**Fix:** Optional — either document the two distinct codes in the `dispatch_rpc`
comment or use `-32601` for the unknown-tool arm for consistency.

### IN-02: `req_counter: u64` increments per call with no wrap concern, but `request_id` collision across bridge restarts is possible

**File:** `baude/src/main.rs:87,96-97`, `bauded/src/main.rs:74,83-84`
**Issue:** `request_id = format!("{pid}-{counter}")`. PID reuse by the OS across a
bridge process restart within the same daemon lifetime could, in principle,
mint a duplicate `request_id` that matches a stale decision the daemon still
holds for the prior process. In practice `set_pending` clears the prior decision
(manager.rs:520) on every new request, so a stale decision cannot be read by a
new request_id — the collision is defanged. Noting for completeness; no action
required.
**Fix:** None required. If extra hardening is desired, seed the counter with a
process-start nonce (e.g. `now_unix_ms()`) instead of `0`.

### IN-03: `permSummary` truncation slices at a byte/char boundary that can split a multi-byte grapheme

**File:** `bauded/web/app.js:353-362`
**Issue:** `s.slice(0, 139)` is a UTF-16 code-unit slice; for tool input
containing emoji/surrogate pairs this can split a surrogate and render a
replacement char. Purely cosmetic — it runs before `esc()` so there is no XSS
risk, and the string is attacker-influenced only in content, not structure.
**Fix:** Optional — slice on `[...s].slice(0, 139).join("")` to respect code
points, or accept the cosmetic edge case.

---

_Reviewed: 2026-06-15_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
