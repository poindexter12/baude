---
phase: 04-remote-permission-approval
plan: 02
subsystem: api
tags: [permission-mode, mcp, json-rpc, stdio-bridge, axum, tokio, security, rust]

# Dependency graph
requires:
  - phase: 04-remote-permission-approval
    provides: "04-01: BAUDE_PERMISSION_MODE flag selection + .mcp.json seeding registering the permission-mcp stdio server at both spawn sites (the registration this plan's bridge fulfils)"
provides:
  - "baude_core::permission JSON-RPC transport: parse_frame (Content-Length + line framing), parse_tool_call (input/parameters/tool_input fallbacks), build_approve_result (allow echoes input / non-allow coerces to deny), rpc_response/rpc_error, dispatch_rpc (initialize/tools/list/tools/call), run_permission_mcp (blocking stdio loop), permission_url_from_event_url, permission_timeout_s, decide_with_timeout (deny-on-timeout rule)"
  - "permission-mcp subcommand arm in BOTH baude/src/main.rs and bauded/src/main.rs (byte-identical; Pitfall-2 no-second-daemon fix); the blocking POST-then-long-poll bridge with deny-on-timeout + fail-closed-on-no-daemon"
  - "daemon Session.pending_permission + permission_decision state; Manager set_pending/pending/decision/resolve_pending + per-session Notify wake (await outside the lock); GET + POST /sessions/{id}/permission routes"
affects:
  - "04-03 (waiting_reason + notified_permission distinct push — reads the pending-permission state this plan sets)"
  - "04-04 (PWA approve/deny card — fetches GET /permission and POSTs the decision)"

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Pure-transform/binary-IO split extended to the permission bridge: all JSON-RPC framing + MCP transforms + the deny-on-timeout rule live in baude-core (no HTTP dep); each binary's run_permission_mcp injects only the ureq round-trip closure (the dispatch_hook precedent)"
    - "Single security-critical rule shared across binaries: permission_timeout_s + decide_with_timeout live in baude-core so both baude and bauded bridges use one tested deny-on-timeout rule (never diverge)"
    - "Decide-under-lock / await-outside-lock for the permission wait: Manager sets/clears pending state under the lock and fires a per-session tokio::sync::Notify; the GET long-poll awaits it OUTSIDE the lock (Pitfall 4)"
    - "Dual-purpose POST /permission: a body with `decision` resolves (PWA), a body with `tool` registers pending (bridge) — one route, shape-selected"

key-files:
  created: []
  modified:
    - baude-core/src/permission.rs
    - baude-core/src/session.rs
    - baude/src/app.rs
    - baude/src/main.rs
    - bauded/src/main.rs
    - bauded/src/manager.rs
    - bauded/src/api.rs

key-decisions:
  - "parse_frame / parse_tool_call / build_approve_result kept ISOLATED as the only encoders of the ASSUMED §C/§D wire contract, so the §F CONTRACT UAT can correct framing/fields/envelope cheaply if live 2.1.178 diverges (RESEARCH §F)"
  - "permission_timeout_s + decide_with_timeout placed in baude-core (not bauded) so the security-critical deny-on-timeout rule is one tested function shared by both binaries' bridges; baude cannot import bauded::manager"
  - "baude-core carries NO HTTP dependency — run_permission_mcp takes an injected resolver closure; the ureq POST-then-long-poll stays in each binary (the dispatch_hook split). Resolver duplicated byte-identically in both binaries, the run_hook precedent"
  - "GET /permission exposes both the pending request AND (after resolve) the decision keyed by request_id, so the bridge's long-poll reads `decision`+`request_id` and ignores a stale prior-turn decision"
  - "POST /permission is dual-purpose (bridge sets pending / PWA resolves), selected by the presence of `decision`; an unknown decision value is a 400, never allow (T-04-05)"
  - "Bridge writes Content-Length-framed replies (LSP-style) — the framing baude advertises; the §F UAT confirms 2.1.178 accepts it (Assumption A4)"

patterns-established:
  - "Pattern: inject the network round-trip as a closure so the JSON-RPC protocol loop is unit-tested over mock stdin/stdout with no live peer"
  - "Pattern: isolate the unverified wire-contract functions behind a human-verify CONTRACT gate so a divergence is a 3-function fix, not a rewrite"

requirements-completed: []  # PERM-01 (transport half) + PERM-02 code complete, but NOT marked done — gated by the §F CONTRACT human-verify UAT (Task 4) which confirms the live wire shape before prompt mode ships.

# Metrics
duration: 35min
completed: 2026-06-15
---

# Phase 4 Plan 2: prompt-mode permission transport (permission-mcp bridge + daemon pending state + /permission routes) Summary

**A hand-rolled stdio JSON-RPC `permission-mcp` MCP server (in both binaries) that blocks on Claude's critical path POSTing each unresolved tool-permission decision to the daemon and long-polling for a human `allow`/`deny` — deny-on-timeout, never auto-allow — backed by daemon `pending_permission` state and `GET`/`POST /sessions/{id}/permission`, with the live wire contract gated by a mandatory §F human-verify UAT.**

## Performance

- **Duration:** ~35 min
- **Tasks:** 3 of 4 automatable code tasks complete; Task 4 is the §F CONTRACT human-verify gate (pending — requires live `claude` 2.1.178)
- **Files modified:** 7

## Status: CHECKPOINT REACHED (Task 4 — §F CONTRACT human-verify gate)

Tasks 1–3 (all automatable code + tests + CI triad) are complete and committed. **Task 4 is a `checkpoint:human-verify` gate (`gate="blocking-human"`) that CANNOT run headlessly** — it requires a live `claude` 2.1.178 session invoking the seeded `mcp__baude__approve` tool to confirm the real request/response wire shape (framing + field names + accepted response envelope; RESEARCH §C/§D/§F, MEDIUM confidence, claude-code #1175). No live-claude frame captures were fabricated. The bridge currently implements the ASSUMED contract; the parse/result functions are isolated so the UAT can correct them cheaply.

## Accomplishments
- **baude_core::permission JSON-RPC + MCP transport (Task 1):** `parse_frame` (Content-Length LSP-style AND bare line framing, partial→None, never-panic), `parse_tool_call` (`tool_name` + `input` with `parameters`/`tool_input` fallbacks, tolerant of missing `tool_use_id`), `build_approve_result` (allow echoes `updatedInput`; any non-`allow` string coerces to `deny` — deny-default), `rpc_response`/`rpc_error`. All isolated so the §F UAT corrects them cheaply.
- **Daemon pending state + routes (Task 2):** `Session.pending_permission` + `permission_decision` (opaque JSON in baude-core); `Manager::set_pending`/`pending`/`decision`/`resolve_pending` (all 404 on unknown id) + per-session `tokio::sync::Notify`; `GET /sessions/{id}/permission` (pending request, or resolved decision, or `null`; optional bounded `?wait` long-poll awaited OUTSIDE the lock); `POST /sessions/{id}/permission` (dual-purpose: bridge sets pending / PWA resolves; `decision ∈ {allow,deny}` else 400; 404 unknown id).
- **permission-mcp bridge in BOTH binaries (Task 3):** `dispatch_rpc` answers `initialize`/`tools/list`/`tools/call` (notifications get no reply; unknown method → -32601); `run_permission_mcp` blocking stdio loop; byte-identical `permission-mcp` arm in `baude/src/main.rs` and `bauded/src/main.rs` (Pitfall-2 fix — `bauded permission-mcp` must NOT boot a second daemon). The resolver POSTs the pending request then long-polls GET until a decision for THIS `request_id` arrives or the deadline denies (`decide_with_timeout`); an absent `$BAUDE_EVENT_URL` fails CLOSED to deny.
- **Security-critical controls enforced + unit-tested:** deny-on-timeout (T-04-04/V4), deny-on-no-daemon, `decision` validation 400 (T-04-05), untyped never-panic frame parse (T-04-06), await-outside-lock (T-04-07), `Path<u64>` id (T-04-09).

## Task Commits

1. **Task 1 (RED): failing tests for JSON-RPC framing + MCP transforms** — `5f3210d` (test)
2. **Task 1 (GREEN): JSON-RPC framing + MCP approve-result builder** — `f3557a0` (feat)
3. **Task 2 (RED): failing tests for pending-permission state + routes** — `0b7961d` (test)
4. **Task 2 (GREEN): pending-permission state + GET/POST /permission routes** — `bf1e180` (feat)
5. **Task 3: permission-mcp stdio bridge in both binaries (deny-on-timeout)** — `981ef4c` (feat)

_Tasks 1 and 2 were TDD (RED test() → GREEN feat()). Task 3 (non-TDD auto) folded the `dispatch_rpc`/`run_permission_mcp` unit tests into its feat commit and moved `permission_timeout_s`/`decide_with_timeout` into baude-core (shared rule)._

## Files Created/Modified
- `baude-core/src/permission.rs` — added `parse_frame`, `parse_tool_call`, `build_approve_result`, `rpc_response`/`rpc_error`, `dispatch_rpc`, `run_permission_mcp`, `permission_url_from_event_url`, `permission_timeout_s`, `decide_with_timeout` + a full test suite (40 permission:: tests).
- `baude-core/src/session.rs` — `Session.pending_permission` + `permission_decision` (opaque `serde_json::Value`, keeping baude-core free of the daemon permission type).
- `baude/src/app.rs` — TUI Session literal initializes the two new fields `None`.
- `baude/src/main.rs` — `run_permission_mcp` bridge + the `permission-mcp` dispatch arm.
- `bauded/src/main.rs` — byte-identical `run_permission_mcp` bridge + the `permission-mcp` dispatch arm (Pitfall 2).
- `bauded/src/manager.rs` — `PendingPermission`/`PermissionDecision`/`PermissionView` types; `set_pending`/`pending`/`decision`/`resolve_pending`/`permission_notify`; `remove` cleans the notify map; daemon Session literal initializes the new fields.
- `bauded/src/api.rs` — `get_permission` (bounded long-poll) + `post_permission` (dual-purpose) handlers + the route; integration tests.

## Decisions Made
- See `key-decisions` frontmatter. The load-bearing one: the three wire-contract functions are deliberately isolated so the §F UAT divergence is a cheap fix.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `permission_timeout_s` + `decide_with_timeout` relocated from bauded::manager to baude-core::permission**
- **Found during:** Task 3 (wiring the bridge in `baude/src/main.rs`).
- **Issue:** The plan placed the deny-on-timeout helpers + their tests in `bauded/src/manager.rs` (Task 2). But the bridge also lives in `baude/src/main.rs`, which cannot import `bauded::manager`. Leaving them in manager made them dead code (clippy `-D dead-code` failed the wave gate) and would have forced the `baude` bridge to duplicate the security-critical rule, risking divergence.
- **Fix:** Moved both functions into `baude_core::permission` (the shared crate both binaries already use); updated the manager tests to call the baude-core versions. The deny-on-timeout rule is now one tested function used by both bridges.
- **Files modified:** `baude-core/src/permission.rs`, `bauded/src/manager.rs`.
- **Verification:** `cargo test -p baude-core permission::` + `cargo test -p bauded manager::` green; `cargo clippy --workspace --all-targets -- -D warnings` clean.
- **Committed in:** `981ef4c` (Task 3 commit).

**2. [Rule 2 - Missing Critical] GET /permission long-poll + dual-purpose POST so the bridge can register pending state over the documented route**
- **Found during:** Task 2/3 integration.
- **Issue:** The plan's POST handler resolved decisions, but the bridge (Task 3) needs to POST the *pending request* to `/sessions/{id}/permission` (RESEARCH §architecture + Task 3 action). With only a decision-POST, `set_pending`/`permission_notify` were unreachable from the daemon (dead code) and the bridge had no route to register pending state.
- **Fix:** Made `post_permission` dual-purpose (a `decision` body resolves; a `tool` body registers pending) and made `get_permission` a bounded long-poll (`?wait`) that awaits the per-session `Notify` OUTSIDE the lock — giving the bridge instant wakeups and exercising the wake path. The decision-validation (400 on unknown) is unchanged.
- **Files modified:** `bauded/src/api.rs`.
- **Verification:** `cargo test -p bauded permission` (GET/POST round-trip, 400 on bad decision, 404 on unknown id) green.
- **Committed in:** `bf1e180` (Task 2 commit) for the route shape; the bridge consumer in `981ef4c`.

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 missing-critical).
**Impact on plan:** Both keep the security-critical rule single-sourced and make the documented bridge↔daemon round-trip actually reachable. No scope creep — the wire contract remains gated by Task 4.

## Issues Encountered
- clippy `while_let_loop` on the stdio drain loop — rewrote `loop { let Some(..) = .. else break }` as `while let Some(..) = ..` (style only).

## Threat Model Compliance
- **T-04-04 (EoP — deny-on-timeout):** `decide_with_timeout` returns `deny` when the deadline passes with no decision; the bridge also fails closed (deny) when `$BAUDE_EVENT_URL`/the daemon is absent. Pinned by `timeout_with_no_decision_resolves_to_deny` + `permission_url_derives_from_event_url` (None → deny path). SECURITY-CRITICAL.
- **T-04-05 (EoP — decision validation):** `post_permission` rejects any `decision` ≠ `allow`/`deny` with 400; `build_approve_result`/`resolve_pending` coerce any non-`allow` to deny. Pinned by `permission_get_post_round_trip_and_validation` (400 case) + `build_approve_result_unknown_behavior_coerces_to_deny`.
- **T-04-06 (Tampering/DoS — frame parse):** `parse_frame`/`parse_tool_call` are untyped-`Value`-or-skip; pinned by `parse_frame_never_panics_on_garbage`, `parse_tool_call_empty_never_panics`.
- **T-04-07 (DoS — manager lock):** set/clear under the lock; the GET long-poll awaits the `Notify` outside it. Pinned by `resolve_notifies_a_registered_waiter`.
- **T-04-09 (Tampering — id):** `Path<u64>` rejects non-numeric ids; unknown numeric → 404. Pinned by the 404 cases in the api tests.

## Known Stubs
- None functional. The §F wire contract is ASSUMED (RESEARCH §C/§D) and confirmed by the pending Task 4 UAT — not a stub but a gated unknown (see below).

## Verification (automated, all green)
- `cargo test -p baude-core permission::` — 40 passed (framing both ways, parse fallbacks, allow-echo/deny-coerce, rpc envelopes, dispatch_rpc all 3 methods + notification + unknown-method, run_permission_mcp over mock stdin/stdout, url derivation, deny-on-timeout rule).
- `cargo test -p bauded manager::` — 27 passed (set/pending/decision/resolve round-trip + 404, deny-on-timeout, resolve wakes a registered waiter).
- `cargo test -p bauded` (incl. api) — 54 passed (GET pending/null/decision/404, POST allow/deny → 202 + 400 on bad decision + 404 on unknown id).
- **CI triad green:** `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (98 baude-core + 54 bauded + 2 baude + 0 doc — all pass).

## CONTRACT GATE (Task 4) — PENDING human-verify (live `claude` 2.1.178)

The §C/§D `--permission-prompt-tool` wire contract is MEDIUM-confidence (no complete official example — claude-code #1175). Only a live `claude` 2.1.178 invoking the seeded MCP tool confirms the framing + field names + accepted response shape. **This cannot be run headlessly. Do NOT mark PERM-01/PERM-02 done until this passes.**

### Exact manual steps
1. Build: `cargo build --workspace`.
2. Temporarily make `run_permission_mcp` (or a copy) **log raw stdin bytes** of every frame to `/tmp/baude-permmcp-frames.log` and, for `tools/call`, **return a hardcoded `allow`** (skip the daemon round-trip for this UAT only). Keep the edit minimal/revertable — the wire functions to touch are `baude_core::permission::{parse_frame, parse_tool_call, build_approve_result}` + the binary `run_permission_mcp` resolver.
3. In a scratch dir, seed `.mcp.json` (run a prompt-mode baude spawn so 04-01 seeds it, or write it by hand: `{"mcpServers":{"baude":{"command":"<abs path to baude or bauded>","args":["permission-mcp"]}}}`), then run:
   `claude -p --permission-prompt-tool mcp__baude__approve --mcp-config .mcp.json "create a file named hello.txt"`
4. Inspect `/tmp/baude-permmcp-frames.log`. CONFIRM for 2.1.178:
   - **Framing:** `Content-Length:`-headered (LSP) or line-delimited? (Assumption A4 — `parse_frame` already supports both; confirm which is emitted.)
   - **`tools/call` params field names:** tool name under `tool_name`? input under `input` vs `parameters`/`tool_input`? `tool_use_id` present? (Assumption A1.)
   - **Handshake:** did `initialize` + `tools/list` get accepted (Claude did NOT report the MCP server failed to start)? (Pitfall 1/5.)
5. CONFIRM the hardcoded `allow` actually unblocked the tool (the file was created). If the CLI **rejected** the `content[0].text`/JSON.stringify result, try returning the `PermissionResult` object directly (RESEARCH §F fallback) and re-run.
6. If the live shape diverges from the assumed contract, correct ONLY `baude_core::permission::{parse_frame, parse_tool_call, build_approve_result}`, re-run `cargo test -p baude-core permission::`, THEN revert the raw-frame-logging/hardcoded-allow hack so the real daemon round-trip is restored — BEFORE prompt mode is treated as final.

### Resume signal
Type: `approved — contract: framing=<lsp|line>, request=<fields>, response=<text|object>` once the live wire shape is confirmed and the bridge matches it (raw-log/hardcoded-allow hack reverted). Or describe the divergence so the executor can correct `baude_core::permission` before prompt mode ships.

## Next Phase Readiness
- 04-03 (waiting_reason + distinct push) can read `pending_permission` now.
- 04-04 (PWA card) can call `GET`/`POST /sessions/{id}/permission` now.
- **Blocker before prompt mode ships:** the §F CONTRACT UAT (Task 4) must confirm the live 2.1.178 wire shape.

## Self-Check: PASSED
- FOUND: `baude-core/src/permission.rs` (dispatch_rpc, run_permission_mcp, parse_frame)
- FOUND: `.planning/phases/04-remote-permission-approval/04-02-SUMMARY.md`
- FOUND commits: `5f3210d`, `f3557a0`, `0b7961d`, `bf1e180`, `981ef4c`

---
*Phase: 04-remote-permission-approval*
*Completed (code tasks): 2026-06-15 — Task 4 §F CONTRACT gate pending*
