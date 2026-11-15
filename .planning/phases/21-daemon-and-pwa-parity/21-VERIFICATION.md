---
phase: 21-daemon-and-pwa-parity
verified: 2026-11-13T23:50:43Z
status: passed
status_basis: "owner sign-off 2026-11-13 without live checks; the human_verification items below were not performed"
score: 1/3 must-haves verified
covered_files: [".github/workflows/ci.yml",".planning/phases/21-daemon-and-pwa-parity/21-01-PLAN.md",".planning/phases/21-daemon-and-pwa-parity/21-01-SUMMARY.md",".planning/phases/21-daemon-and-pwa-parity/21-02-PLAN.md",".planning/phases/21-daemon-and-pwa-parity/21-02-SUMMARY.md",".planning/phases/21-daemon-and-pwa-parity/21-03-PLAN.md",".planning/phases/21-daemon-and-pwa-parity/21-03-SUMMARY.md",".planning/phases/21-daemon-and-pwa-parity/21-04-PLAN.md",".planning/phases/21-daemon-and-pwa-parity/21-04-SUMMARY.md",".planning/phases/21-daemon-and-pwa-parity/21-05-PLAN.md",".planning/phases/21-daemon-and-pwa-parity/21-05-SUMMARY.md",".planning/phases/21-daemon-and-pwa-parity/21-06-PLAN.md",".planning/phases/21-daemon-and-pwa-parity/21-06-SUMMARY.md",".planning/phases/21-daemon-and-pwa-parity/21-07-PLAN.md",".planning/phases/21-daemon-and-pwa-parity/21-07-SUMMARY.md",".planning/phases/21-daemon-and-pwa-parity/21-08-PLAN.md",".planning/phases/21-daemon-and-pwa-parity/21-08-SUMMARY.md",".planning/phases/21-daemon-and-pwa-parity/21-09-PLAN.md",".planning/phases/21-daemon-and-pwa-parity/21-09-SUMMARY.md",".planning/phases/21-daemon-and-pwa-parity/21-10-PLAN.md",".planning/phases/21-daemon-and-pwa-parity/21-10-SUMMARY.md",".planning/phases/21-daemon-and-pwa-parity/21-11-PLAN.md",".planning/phases/21-daemon-and-pwa-parity/21-11-SUMMARY.md","baude/src/app.rs","baude/src/remote.rs","baude/src/ui.rs","bauded/src/api.rs","bauded/src/manager.rs","bauded/src/web.rs","bauded/web/app.js","bauded/web/removal-controller.js","bauded/web/removal.js","bauded/web/sw.js","scripts/check-web-suites.sh","scripts/gate.sh"]
covered_digest: "v4:sha256:8264ce4de28fb4eb3fc8582c1e3b9a66aab6937f45718704dfca943e705ad6c5"
behavior_unverified: 2
overrides_applied: 0
behavior_unverified_items:
  - truth: "A connected TUI or PWA drops the removed rows on the SSE event without a refresh"
    test: "With a PWA tab and a remote TUI both connected, remove a row from a third client and watch both lists"
    expected: "The row leaves both lists on the rows_removed event, before the 3 s poll, and does not come back"
    why_human: "The daemon publish is test-witnessed; the live client drop was never observed by an executor or by this verifier. Owner sign-off carries no per-step observations."
  - truth: "The PWA offers remove on the same row kinds with a confirmation equivalent to the TUI's and completes it through the REST endpoint"
    test: "In a browser, open remove on a repository header, a main checkout, a worktree and a standalone row; confirm one of each"
    expected: "Modal built from the preview report with rows, runtimes, paths deleted, paths kept and clearings; confirm removes; 409 re-renders in place; 500 and 503 keep the modal open with Retry"
    why_human: "The PWA was never exercised in a browser by any executor. Node suites drive the modules against fakes only."
human_verification:
  - test: "Repeat the 17-step live checkpoint from 21-11 and record a result per step"
    expected: "Each step observed and written down, including that removed rows are gone from GET /rows afterwards"
    why_human: "21-11-SUMMARY records sign-off only, no per-step results, and that the fixture daemon still listed most rows at sign-off"
  - test: "PWA removal in a real browser on all four row kinds, including 409, 500 and 503 paths"
    expected: "As in behavior_unverified_items entry 2"
    why_human: "Visual and interaction behaviour; no browser run exists"
  - test: "Live SSE row drop in the PWA and in a remote TUI"
    expected: "As in behavior_unverified_items entry 1"
    why_human: "Real-time client behaviour"
  - test: "Review the two fixture asserts removed in 21-06 from baude/src tests: main_row.role equals PrimaryDefault, and runtime_checkouts contains the clone's primary"
    expected: "Each removal is traced to a named test and justified, or the assert is restored"
    why_human: "The verifier saw the removed lines in git diff 78ce5ec..265572f but did not trace them to a test or a reason"
  - test: "Leave /events idle past 15 s through a proxy or the tailnet"
    expected: "Keep-alive comment arrives and the stream stays open"
    why_human: "The 15 s keep-alive is not test-witnessed"
  - test: "First push: confirm the CI check job runs the node suites green"
    expected: "check-web-suites step passes on the runner"
    why_human: "setup-node and the step are present in ci.yml but unproven until a push"
---

# Phase 21: Daemon and PWA Parity Verification Report

**Phase Goal:** The same removal is available over the daemon REST API with SSE updates and from the PWA with an equivalent confirmation, through the shared `baude-core` operation.
**Verified:** 2026-11-13T23:50:43Z
**Status:** human_needed
**Re-verification:** No, initial verification

## Scope and limits of this verification

This pass ran with very little context budget. It is narrower than a full goal-backward pass and
says so here rather than implying coverage it does not have.

- **Read in code:** the four routes in `bauded/src/api.rs`, the tail of `Manager::remove_rows` in
  `bauded/src/manager.rs` (lines 1172 to 1233), and grep-level wiring for `web.rs`, `app.js`,
  `remote.rs`, `ui.rs`, `gate.sh` and `ci.yml`.
- **Run:** `cargo test -p bauded --bin bauded -- matrix_ events_ flat_session_api`, exit 0, 27 passed
  0 failed; `bash scripts/check-web-suites.sh`, exit 0, 69 tests 69 pass. Exit codes captured directly.
- **Not done:** the baude and baude-core suites were not rerun; the orchestrator reports them green
  on `6cb3941`. `removal.js`, `removal-controller.js`, `remote.rs` and the TUI `D` path in `app.rs`
  were not read line by line. The GSD reference files and the PLAN `must_haves` blocks were not
  loaded, so the must-haves below are the three roadmap success criteria only; `verify.artifacts`
  and `verify.key-links` were not run. No test body was read in full: the spot-check of the `matrix_`
  and `events_` tests is by name, assert count and one assert listing.

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
| - | ----- | ------ | -------- |
| 1 | REST removal endpoint accepts repository, checkout, worktree and standalone rows, calls the Phase 18 operation, returns rows removed, runtimes killed and paths deleted | VERIFIED | Routes `/removals/preview`, `/removals` at `api.rs:52-53`. `Manager::preview_removal` calls `removal::preview_removal` at `manager.rs:1105`; `remove_rows` calls `removal::remove` at `manager.rs:1179` and returns the core report. On `Err` handles are restored at `:1185`. 14 `matrix_` tests ran green covering 404, 409, 500, 503, saved_degraded 200 and a standalone row; `events_cover_standalone_and_repository_rows` ran green. |
| 2 | A connected TUI or PWA drops the removed rows on the SSE event without a refresh | PRESENT_BEHAVIOR_UNVERIFIED | Daemon half is witnessed: `/events` at `api.rs:29`, broadcast sender at `manager.rs:272`, and 6 `events_` tests ran green, including one subscriber, two subscribers, no history, saved_degraded publishes once, and lag survival. Client half is present and wired (`app.js:35-36`, `:148-153`; `remote.rs` carries rows, events and removal calls) but the live drop was never observed. |
| 3 | The PWA offers remove on the same row kinds with a confirmation equivalent to the TUI's and completes it through the REST endpoint | PRESENT_BEHAVIOR_UNVERIFIED | `removal.js` and `removal-controller.js` are served by `web.rs:37-42` with a load-order test; `app.js` holds the controller; node suites pass 69 of 69. No browser run exists. |

**Score:** 1/3 truths verified (2 present, behavior-unverified)

Live PWA and TUI behaviour is **owner-attested, not independently observed**. `21-11-SUMMARY.md`
records the 17-step checkpoint as verified on sign-off alone, with no per-step results, and notes
that the fixture daemon's `GET /rows` still listed most rows at sign-off. That record is not
evidence that removals completed during the checkpoint.

### Decision conformance

| Decision | Status | Evidence |
| -------- | ------ | -------- |
| D-09 Option B, `/sessions` compatibility test unmodified and passing | VERIFIED by run | `flat_session_api_remains_a_non_hierarchical_compatibility_projection` at `api.rs:1106` ran green. Not diffed against its pre-phase text. |
| D-12 TUI refusal replaced | VERIFIED by grep | No "arrives with Phase 21" string remains in `baude/src` or `bauded/src`; `ui.rs` hints now carry `D remove*` on remote rows. |
| D-07 publish after save, inside the lock | Read, not test-observable | The comment at `manager.rs:1230-1233` states this is a property of code position and that no test observes it. |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| -------- | ------- | ------ | ------ |
| REST status matrix, events publish, sessions projection | `cargo test -p bauded --bin bauded -- matrix_ events_ flat_session_api` | exit 0, 27 passed | PASS |
| PWA module suites | `bash scripts/check-web-suites.sh` | exit 0, 69 pass, 0 fail | PASS |

Spot-check of what tests assert: each of the 14 `matrix_` tests holds between 5 and 19 assert
lines, and their names match the D-03 matrix. `events_removal_over_rest_reaches_a_subscriber`
asserts the row is listed before, the 200 body names the row, the announced event is compared, and
the row is absent from `/rows` after. Bodies were not read in full, so whether every assert matches
its plan's claim remains unchecked.

### Probe Execution

Not run. No probe discovery was performed.

### Requirements Coverage

| Requirement | Status | Evidence |
| ----------- | ------ | -------- |
| API-01 | SATISFIED | Truth 1 |
| API-02 | NEEDS HUMAN | Daemon emission witnessed; client drop unobserved |
| API-03 | NEEDS HUMAN | Present and wired; no browser run |

REQUIREMENTS.md already marks all three Complete. For API-02 and API-03 that is ahead of the evidence.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| `scripts/gate.sh` | 98 | `XXXXXX` | Info | mktemp template, not a debt marker |
| `baude/src` tests, 21-06 | n/a | Two fixture asserts removed without a traced reason | Warning | Possible loss of fixture realism checks. A third removed assert belonged to the retired refusal notice and is expected under D-12, as are the two removed hint asserts. |

### Known open items, reported as given, not investigated here

- Intermittent macOS `DELETE /sessions/{id}` 500 teardown flake, suspected EPERM in `signal_group` in `baude-core/src/pty.rs`.
- Standalone removal leaves a daemon-written `.claude` directory.
- `app.js` lists unclaimed sessions without the "other sessions" heading; grep for that string in `app.js` found nothing.
- Older state files still serve `managed_branch` for a main checkout.
- The 15 s SSE keep-alive is not test-witnessed.
- The CI `check` job with node is unproven until first push; `ci.yml:27-31` has the setup and the step.

None of these is a later-phase deferral confirmed against the roadmap; Step 9b filtering was not run.

### Gaps Summary

No truth failed on the evidence gathered. Two of three truths rest on live client behaviour that
nobody has observed and recorded, so the phase cannot be called passed. The honest reading is that
the daemon side of the goal is proven by tests and the PWA and TUI side is built, wired, unit-tested
against fakes, and attested by the owner without observations.

---

_Verified: 2026-11-13T23:50:43Z_
_Verifier: Claude (gsd-verifier)_
