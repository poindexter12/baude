---
phase: 11-negotiated-multiline-input
plan: 04
subsystem: terminal-input
tags: [kitty-keyboard, vt100, csi-u, pty, remote-attach, tdd]

requires:
  - phase: 11-negotiated-multiline-input (plan 11-01)
    provides: "EncodeCtx + encode_key Shift+Enter decision matrix; forward_key construction sites with the fail-closed kitty_child: false constant"
  - phase: 11-negotiated-multiline-input (plan 11-02)
    provides: "Screen::kitty_keyboard() -> u16 observed-push accessor in the vt100 fork"
provides:
  - "encode_ctx(screen, to_shell) -> EncodeCtx — single ctx producer for both forward_key branches; kitty_child = screen.kitty_keyboard() != 0 (D-04 child leg live)"
  - "subscribe() snapshot replays an active child kitty push (CSI > flags u) so remote mirror parsers converge on child kitty state (TKEY-05 across attach)"
  - "forward_ctx_tests: derivation + composed Shift+Enter matrix through the real path; pty.rs kitty round-trip tests pinning replay convergence and zero-byte inactive snapshots"
affects: [12-release-gate]

actuals:
  tokens: 2613
  tasks: 2
  commits: 4
plan_head_before: 4503eb73782f9512fbdbc9ba0c59980201045e35

tech-stack:
  added: []
  patterns:
    - "Screen-state-derived encode context: one module-level helper reads all child input modes (DECCKM + kitty stack) under the caller's existing parser lock"
    - "Snapshot mode-replay extension mirrored byte-for-byte in a pure test helper so test/production construction drift is the guarded failure mode"

key-files:
  created: []
  modified:
    - baude/src/app.rs
    - baude-core/src/pty.rs

key-decisions:
  - "RED scaffolding (both tasks follow the 11-01/11-02 precedent): behavior-preserving stubs landed in the test commits so RED failures are genuine assertions, not compile errors (#3770) — Task 1's plan text had expected a compile-failure RED"
  - "Task 1 RED already rewired both forward_key branches through the stubbed helper (byte-identical to the 11-01 constant) so GREEN is the single-line observed-push read"
  - "Task 2's test mirror extracted as subscribe_snapshot_bytes(screen) inside the test module (not production) — keeps the plan's inline-production shape while giving both round-trip tests one mirror to drift-check"

patterns-established:
  - "Poisoned parser lock degrades to the full-legacy EncodeCtx (app_cursor false, kitty_child false) — fail-closed at every fallback"

requirements-completed: [TKEY-01, TKEY-02, TKEY-05]

coverage:
  - id: D1
    description: "forward_key derives kitty_child from the child's observed kitty push (true after CSI > 1 u, false fresh/after pop, independent of DECCKM), and the composed Shift+Enter matrix through the real derivation yields CSI 13;2u / ESC CR / CR per the D-04/D-09 decision table"
    requirement: TKEY-01
    verification:
      - kind: unit
        ref: "cargo test -p baude forward_ctx # observed_push_yields_kitty_child_true_fresh_parser_false + pop_returns_kitty_child_to_unverified + app_cursor_and_kitty_child_are_independent_modes + composed_shift_enter_matrix_through_real_derivation"
        status: pass
    human_judgment: false
  - id: D2
    description: "No key other than Shift+Enter changed bytes: the 11-01 legacy corpus (57 rows x both panes) still passes against the live derivation"
    requirement: TKEY-02
    verification:
      - kind: unit
        ref: "cargo test -p baude keys # legacy_corpus_bytes_are_frozen + matrix + idempotency"
        status: pass
    human_judgment: false
  - id: D3
    description: "Local and remote-attach forwarding derive identical contexts: encode_ctx is the only EncodeCtx producer in forward_key (remote parity by construction; the two remaining literals are the plan-prescribed poisoned-lock fail-closed fallbacks)"
    requirement: TKEY-02
    verification:
      - kind: other
        ref: "grep -n 'EncodeCtx {' baude/src/app.rs -> only the two unwrap_or fallbacks and the helper body remain; both branches call encode_ctx(p.screen(), to_shell)"
        status: pass
    human_judgment: false
  - id: D4
    description: "A remote mirror parser converges on the child's active kitty state from the subscribe snapshot; a non-kitty child's snapshot carries zero replay bytes"
    requirement: TKEY-05
    verification:
      - kind: unit
        ref: "cargo test -p baude-core subscribe # subscribe_snapshot_replays_active_kitty_push + subscribe_snapshot_inactive_kitty_adds_no_bytes + pre-attach-links + snapshot_then_live_bytes"
        status: pass
    human_judgment: false
  - id: D5
    description: "Live end-to-end Shift+Enter on a real kitty-capable terminal against a real Claude child"
    requirement: TKEY-05
    verification: []
    human_judgment: true
    rationale: "Deliberately deferred to Phase 12 SHIP-03 per CONTEXT (injected-seams-over-live-spawns): the byte-level derivation and replay are test-proven, but real-terminal behavior needs the Phase 12 validation pass"

duration: 9min
completed: 2026-09-16
status: complete
---

# Phase 11 Plan 04: Child-Verification Live Wiring Summary

**forward_key's kitty_child now reads the child's observed CSI > u push via encode_ctx (one derivation for local and remote branches), and subscribe() replays an active push so remote mirrors converge — enhanced Shift+Enter is reachable only on the doubly-verified outer+child path**

## Performance

- **Duration:** 9 min
- **Started:** 2026-09-16T15:00:17Z
- **Completed:** 2026-09-16T15:09:46Z
- **Tasks:** 2 (both TDD: RED → GREEN)
- **Files modified:** 2

## Accomplishments
- Replaced 11-01's fail-closed `kitty_child: false` constant (WINDOWS ledger entry 11, now fixed) with the real observation: `encode_ctx(screen, to_shell)` reads `screen.kitty_keyboard() != 0` beside `application_cursor()`, and BOTH forward_key branches derive their ctx through this one helper under their existing parser locks — remote sessions cannot diverge (TKEY-02 parity by construction)
- Every fallback stays fail-closed per D-04: fresh parser 0, fully-popped stack 0, poisoned lock degrades to the full-legacy ctx
- Composed matrix test proves the full TKEY-01 decision table through the real derivation path: pushed child → `CSI 13;2u`, legacy Claude pane → `ESC CR`, legacy shell pane → `CR` (D-09, D-10: only Shift+Enter is ctx-gated)
- subscribe()'s mode-replay list gains `CSI > {flags} u` conditioned on the accessor: one replayed push converges a fresh mirror's empty stack on the child's current top-of-stack flags; inactive state appends zero bytes (byte-identical snapshots for non-kitty children)

## TDD Evidence (RED → GREEN)

- **Task 1 RED** (`d3ccac7`, `test(11-04)`): 4 derivation/matrix tests + behavior-preserving scaffolding (stubbed helper, rewired branches). 2 genuine assertion failures (push→true panics; matrix yields `[27,13]` vs expected `[27,91,49,51,59,50,117]`). Evidence `11-04-red-evidence-task1.json` → **RED_EVIDENCE_OK** (`target_test_failed`, exit 101).
- **Task 1 GREEN** (`611ccdc`, `feat(11-04)`): single-line observed-push read. `forward_ctx` 4/4, `keys` 8/8 green.
- **Task 2 RED** (`d5e48bd`, `test(11-04)`): round-trip tests over a `subscribe_snapshot_bytes` mirror of subscribe()'s current construction; active-push convergence fails 0≠1. Evidence `11-04-red-evidence-task2.json` → **RED_EVIDENCE_OK** (`target_test_failed`, exit 101).
- **Task 2 GREEN** (`0b28388`, `feat(11-04)`): replay line added to subscribe() and the test mirror in lockstep. `subscribe` 4/4 green; `cargo test --workspace --locked` green.
- **REFACTOR:** skipped both tasks — no behavior-preserving improvement existed (helper already minimal; replay follows the established mode-replay pattern).

## Task Commits

1. **Task 1 (RED): failing forward_ctx derivation and matrix tests** - `d3ccac7` (test)
2. **Task 1 (GREEN): forward_key consults the observed child kitty push** - `611ccdc` (feat)
3. **Task 2 (RED): failing subscribe kitty-replay round-trip tests** - `d5e48bd` (test)
4. **Task 2 (GREEN): subscribe snapshot replays active child kitty push** - `0b28388` (feat)

## Files Created/Modified
- `baude/src/app.rs` - `encode_ctx` module-level helper; both forward_key branches rewired through it with poisoned-lock fail-closed fallbacks; `forward_ctx_tests` (4 pure vt100-parser tests)
- `baude-core/src/pty.rs` - subscribe() kitty push replay in the mode-replay list; `subscribe_snapshot_bytes` test mirror + 2 round-trip tests

## Decisions Made
- RED shape for Task 1 converted from the plan's compile-failure expectation to assertion RED via behavior-preserving scaffolding (see Deviations) — consistent with 11-01/11-02 and #3770
- Task 2's mirror construction extracted to a test-module helper shared by both round-trip tests (production subscribe() stays inline per the plan's prescribed shape)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Task 1 compile-failure RED converted to assertion RED**
- **Found during:** Task 1 (RED phase)
- **Issue:** Plan said "RED fails to compile (encode_ctx does not exist)" — but a compile failure is INVALID_RED under #3770 (no assertion evidence for the planned behavior)
- **Fix:** RED commit carries behavior-preserving scaffolding: `encode_ctx` stubbed `kitty_child: false` and both forward_key branches rewired through it (byte-identical to the 11-01 constant construction), so 2 of 4 tests fail on genuine planned-behavior assertions
- **Files modified:** baude/src/app.rs
- **Verification:** `check tdd-red-evidence` → `RED_EVIDENCE_OK` / `target_test_failed`, exit 101
- **Committed in:** d3ccac7

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** None on scope or behavior — RED discipline strengthened; GREEN commits carry exactly the planned behavior changes.

## Issues Encountered
- `check tdd-red-evidence` parses node TAP only (known from 11-01/11-02): cargo libtest output transcribed faithfully to TAP (verbatim names/counts/exit) in both evidence records.

## TDD Gate Compliance

RED → GREEN intact for both tasks: `test(11-04)` d3ccac7 precedes `feat(11-04)` 611ccdc; `test(11-04)` d5e48bd precedes `feat(11-04)` 0b28388. Both RED evidence records verified `RED_EVIDENCE_OK`. REFACTOR legitimately omitted (optional, no changes made). No violations.

## Authentication Gates

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Phase 11 complete: all four plans summarized; both ends of TKEY-05's verified path are live (outer probe from 11-01, observed child push from 11-02+11-04)
- WINDOWS ledger entry 11 (the kitty_child stub) marked fixed
- Real-terminal end-to-end validation (residue, latency, live Shift+Enter) deferred to Phase 12 SHIP-03 per CONTEXT/D-08 — coverage entry D5 routes it to human UAT

## Self-Check: PASSED

Both modified files exist on disk; all 4 commit hashes present in git log; `commits: 4` measured from ledger base 4503eb73 (`git rev-list --count`); TDD gate commits (test before feat, both tasks) verified in log.

---
*Phase: 11-negotiated-multiline-input*
*Completed: 2026-09-16*
