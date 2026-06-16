---
phase: 04-remote-permission-approval
plan: 03
subsystem: notifications
tags: [waiting-reason, web-push, notifier, session-info, serde, rust, perm-04]

# Dependency graph
requires:
  - phase: 04-remote-permission-approval
    provides: "04-02: daemon Session.pending_permission state + GET/POST /sessions/{id}/permission routes (the pending state this plan's waiting_reason/push key off)"
provides:
  - "baude_core::permission::waiting_reason(last_notification, waiting) — pure total mapper to permission/input/none"
  - "SessionInfo.waiting_reason populated in session_info() from meta.last_notification + waiting status"
  - "RemoteInfo.waiting_reason mirror (#[serde(default)]) + TUI remote-info overlay reader"
  - "Notifier.notified_permission distinct-push set: one lean permission push, re-armed on resolve, mutually exclusive with the generic waiting push"
affects:
  - "04-04 (PWA approve/deny card — gates on waiting_reason === 'permission')"

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "waiting_reason is a pure, total, panic-free mapper in baude-core (the security-critical-rule-in-core precedent) so the daemon's SessionInfo derivation and any future consumer share one tested function"
    - "Notifier debounce-set extended: notified_permission mirrors notified_waiting/notified_exited (insert-once + retain-prune + edge re-arm) — the distinct push is one more trigger, not a new protocol (Web Push send path untouched)"
    - "Permission and generic-waiting pushes are mutually exclusive: a single is_permission gate routes a waiting session to exactly one branch"
    - "RemoteInfo mirror with #[serde(default)] (state_source/last_tool/activity precedent) keeps an older daemon deserializing to None"

key-files:
  created: []
  modified:
    - baude-core/src/permission.rs
    - bauded/src/manager.rs
    - bauded/src/notify.rs
    - baude/src/remote.rs
    - baude/src/ui.rs

key-decisions:
  - "Push body stays LEAN: title '<name> needs permission' + generic 'wants to run a tool — approve?' — NO pending-tool-name field added to SessionInfo (plan W2 decision + CONTEXT). The phone fetches tool/input detail from GET /permission, keeping the body that traverses the external push service free of action detail (T-04-10)."
  - "waiting_reason 'permission' fires even when the waiting flag is false — a pending permission is itself a waiting state and the notification is the authority (so the card/push surface as soon as the hook lands)."
  - "session_info() maps 'none' -> None so the JSON stays lean and the PWA/push key off the presence of the 'permission'/'input' strings."
  - "RemoteInfo.waiting_reason given a reader in the TUI remote-info overlay (a 'waiting' row) — without it clippy -D warnings fails on dead_code; this also makes the reason observable in the remote TUI (Rule-3 + Rule-2)."

patterns-established:
  - "Distinct-push set + edge re-arm: a new attention class (permission) slots alongside notified_waiting/notified_exited with the same insert-once/prune/re-arm discipline and no change to the Web Push send/encryption path."

requirements-completed: [PERM-04]

# Metrics
duration: 4min
completed: 2026-06-15
---

# Phase 4 Plan 3: waiting_reason + distinct permission push Summary

**A pure `waiting_reason` mapper in `baude_core::permission` (permission/input/none) populated on `SessionInfo` (and mirrored on `RemoteInfo`) from the already-captured `last_notification`, plus a `notified_permission` debounce set in the `Notifier` that fires ONE lean distinct permission push — re-armed when the permission resolves and mutually exclusive with the generic waiting push (PERM-04).**

## Performance

- **Duration:** ~4 min
- **Tasks:** 2 of 2 complete (both TDD: RED test() → GREEN feat())
- **Files modified:** 5

## Accomplishments

- **`waiting_reason` mapper (Task 1, baude-core):** `pub fn waiting_reason(Option<&(String,u64)>, bool) -> &'static str` — a notification type CONTAINING `"permission"` → `"permission"`; else `waiting` → `"input"`; else `"none"`. Total, panic-free over any/odd notification type (T-04-11).
- **`SessionInfo.waiting_reason` (Task 1, manager.rs):** populated in `session_info()` from `s.meta.last_notification` + `(status == Waiting)`, mapping `"none"` → `None` so the JSON stays lean.
- **`RemoteInfo.waiting_reason` mirror (Task 1, remote.rs):** `#[serde(default)]` for back-compat against an older daemon; surfaced as a `waiting` row in the TUI remote-info overlay (`ui.rs`) — the field reader.
- **Distinct permission push (Task 2, notify.rs):** `notified_permission: HashSet<u64>` on `Notifier` (+ retain-prune); a `waiting_reason == "permission"` session fires ONE lean Notification (`"<name> needs permission"` / `"wants to run a tool — approve?"`), debounced via the set, re-armed when `waiting_reason` flips away. The generic waiting push is gated behind the non-permission case so the two never both fire for one session.
- **Rule-3 test-constructor fix:** `notify.rs` `#[cfg(test)] fn info()` literal got `waiting_reason: None` (the recurring 02-03/03-02 fix) in the same change that added the field, so `cargo test -p bauded` compiles.

## Task Commits

1. **Task 1 (RED): failing tests for waiting_reason mapper** — `15c7990` (test)
2. **Task 1 (GREEN): waiting_reason on SessionInfo + RemoteInfo mirror** — `aed0932` (feat)
3. **Task 2 (RED): failing tests for distinct permission push** — `ddae220` (test)
4. **Task 2 (GREEN): distinct permission push via notified_permission** — `8348437` (feat)

## Files Created/Modified

- `baude-core/src/permission.rs` — added `pub fn waiting_reason` + 2 unit tests (mapping matrix + tolerant case-sensitive substring).
- `bauded/src/manager.rs` — `SessionInfo.waiting_reason` field; populated in `session_info()`; `session_info_sets_waiting_reason_permission` test.
- `bauded/src/notify.rs` — `notified_permission` set (+ retain-prune); the `"waiting" if is_permission` distinct-push branch + re-arm; test-constructor `waiting_reason: None`; `permission_fires_distinct_push_once_and_not_generic` + `permission_re_arms_after_resolve` tests.
- `baude/src/remote.rs` — `RemoteInfo.waiting_reason` (`#[serde(default)]`).
- `baude/src/ui.rs` — `waiting` row in the remote-info overlay (the RemoteInfo field reader).

## Decisions Made

- See `key-decisions` frontmatter. Load-bearing: the push body stays LEAN (no tool name in `SessionInfo` / push) — detail comes from `GET /permission` (T-04-10); `waiting_reason == "permission"` fires regardless of the waiting flag (the notification is the authority).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking / Rule 2 - Missing Critical] `RemoteInfo.waiting_reason` given a reader in the TUI remote-info overlay**
- **Found during:** Task 1 (adding the `RemoteInfo` mirror).
- **Issue:** Adding `RemoteInfo.waiting_reason` with no consumer triggers `field is never read` (`dead_code`), which fails the wave gate `cargo clippy --workspace --all-targets -- -D warnings`. The plan scopes the PWA consumer to 04-04, but `RemoteInfo` is consumed by the TUI (`baude/src/ui.rs`), where the sibling mirror fields (`state_source`/`last_tool`/`activity`) already have readers.
- **Fix:** Added a conditional `waiting` row to the remote-info overlay (mirroring the `last_tool` row at `ui.rs:903`), so the field is read and the reason is observable in the remote TUI.
- **Files modified:** `baude/src/ui.rs`.
- **Verification:** `cargo build -p baude` warning-free; `cargo clippy --workspace --all-targets -- -D warnings` clean.
- **Committed in:** `aed0932` (Task 1 GREEN).

**2. [Rule 3 - Blocking] notify.rs test-constructor `waiting_reason: None` applied in Task 1 (not Task 2)**
- **Found during:** Task 1 (adding `SessionInfo.waiting_reason`).
- **Issue:** The plan sequences the `#[cfg(test)] fn info()` constructor fix as Task 2's first step, but adding the field in Task 1 breaks `cargo test -p bauded` compile immediately — Task 1's own manager test runs under `-p bauded` and cannot be verified until the constructor compiles.
- **Fix:** Added `waiting_reason: None` to the `notify.rs` test constructor in the Task 1 GREEN commit so Task 1 is independently verifiable. Functionally identical to the plan; only the commit boundary moved.
- **Files modified:** `bauded/src/notify.rs`.
- **Committed in:** `aed0932` (Task 1 GREEN).

---

**Total deviations:** 2 auto-fixed (both blocking compile/lint). No scope creep — both keep the wave gate green and the new field reachable.

## Threat Model Compliance

- **T-04-10 (Information Disclosure — push payload):** the distinct push body is lean (`"wants to run a tool — approve?"` + sid tag); no tool name/input embedded — the detail is fetched via `GET /permission` over the bind, not across the external push service. Pinned by `permission_fires_distinct_push_once_and_not_generic` (title/body assertions, no tool detail).
- **T-04-11 (Tampering/DoS — mapper):** `waiting_reason` is a total `match` over `Option<&(String,u64)>` returning a valid enum string for any input, never panicking. Pinned by `waiting_reason_maps_permission_input_none` + `waiting_reason_tolerant_permission_substring`.
- **T-04-12 (DoS — debounce):** the permission push fires once per edge via `notified_permission.insert`; re-armed only when `waiting_reason` flips away — no push storm; mutually exclusive with the generic waiting push. Pinned by `permission_re_arms_after_resolve` + the no-duplicate-while-pending assertion.

## Known Stubs

- None. All fields are wired end to end (mapper → SessionInfo → push trigger; RemoteInfo mirror → TUI reader). The PWA consumer is intentionally scoped to 04-04 (the field/push are already populated for it to read).

## Verification (automated, all green)

- `cargo test -p baude-core permission::tests::waiting_reason*` — 2 passed (mapping matrix + tolerant substring).
- `cargo test -p bauded session_info_sets_waiting_reason_permission` — passed (permission_prompt → "permission").
- `cargo test -p bauded notify::` — 6 passed (distinct push once, no generic for it, no duplicate while pending, re-arm after resolve, plus the 4 prior debounce tests still green).
- **CI triad green:** `cargo fmt --check`; `cargo clippy --workspace --all-targets -- -D warnings` (exit 0); `cargo test --workspace` (100 baude-core + 57 bauded + 2 baude + doc — all pass).

## Next Phase Readiness

- 04-04 (PWA approve/deny card) can gate on `waiting_reason === "permission"` now — the field is populated on `SessionInfo`/`RemoteInfo` and the distinct push fires.
- Phone-verification of Web Push remains a separate manual milestone (additive trigger only — the send/encryption path is untouched).

## Self-Check: PASSED

- FOUND: `baude-core/src/permission.rs` (waiting_reason)
- FOUND: `bauded/src/notify.rs` (notified_permission)
- FOUND: `.planning/phases/04-remote-permission-approval/04-03-SUMMARY.md`
- FOUND commits: `15c7990`, `aed0932`, `ddae220`, `8348437`

---
*Phase: 04-remote-permission-approval*
*Completed: 2026-06-15*
