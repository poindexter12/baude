---
phase: 11-negotiated-multiline-input
plan: 02
subsystem: terminal-emulation
tags: [kitty-keyboard, vt100, csi-u, pty, tdd]

requires:
  - phase: 10-clickable-terminal-links
    provides: "vt100 fork conventions: BAUDE FORK markers, named-const caps, fail-closed OSC 8 posture copied for the kitty stack"
provides:
  - "Screen::kitty_keyboard() -> u16 — top-of-stack kitty flags (0 = legacy); the TKEY-05 child-verification signal for plan 11-04's forward_key gate"
  - "kitty_stack field + KITTY_STACK_MAX=32 const + csi_dispatch arms for CSI > u (push, oldest-eviction), CSI < u (saturating pop), CSI = u (set replace/OR/AND-NOT)"
  - "vendor/vt100/tests/kitty_keyboard.rs — 11-case byte-in/state-out suite pinning push/pop/set, hostile-input bounds, and the unanswered-probe invariant"
affects: [11-04, 12-release-gate]

actuals:
  tokens: 2667
  tasks: 1
  commits: 2
plan_head_before: 26af97f0168e2eb86f0d6d2c960929f0dc74f853

tech-stack:
  added: []
  patterns:
    - "Kitty-stack tracking mirrors the fork's OSC 8 fail-closed posture: named-const cap, saturating arithmetic, silent safe degradation"
    - "Single tracking stack (not kitty's per-screen stacks) — observation-only divergence documented on the accessor"

key-files:
  created:
    - vendor/vt100/tests/kitty_keyboard.rs
  modified:
    - vendor/vt100/src/screen.rs

key-decisions:
  - "RED shape per plan: do-nothing 0-stub accessor landed in the test commit so failures are real assertions, not compile errors (#3770); compile-failure run recorded as context only"
  - "Unknown CSI = modes (outside 1..=3) are ignored fail-closed — no state change, no entry established"
  - "Depth cap proven behaviorally (pop 31 of 40 pushes exposes push #9, one more pop empties) since the stack is private"

patterns-established:
  - "Kitty stack ops route through private kitty_push/kitty_pop/kitty_set handlers using the existing canonicalize_params helpers"

requirements-completed: [TKEY-01, TKEY-05]

coverage:
  - id: D1
    description: "vt100 fork tracks the child's kitty keyboard state: CSI > u pushes, CSI < u pops (saturating), CSI = u sets; kitty_keyboard() reports top-of-stack flags with 0 default"
    requirement: TKEY-05
    verification:
      - kind: integration
        ref: "cargo test -p vt100 --test kitty_keyboard # push/pop/set/nested/restore cases"
        status: pass
    human_judgment: false
  - id: D2
    description: "Hostile child output cannot panic or exhaust memory: huge pop saturates, push depth capped at 32 with oldest-eviction, zero attacker-proportional allocation (T-11-05)"
    requirement: TKEY-05
    verification:
      - kind: integration
        ref: "cargo test -p vt100 --test kitty_keyboard # huge_pop_on_empty_stack_saturates + push_depth_capped_at_32_with_oldest_eviction"
        status: pass
    human_judgment: false
  - id: D3
    description: "The child's CSI ? u query probe stays unanswered — no reply path exists or was added (T-11-06, observation without advertisement)"
    requirement: TKEY-01
    verification:
      - kind: integration
        ref: "cargo test -p vt100 --test kitty_keyboard # query_probe_stays_unanswered"
        status: pass
      - kind: other
        ref: "git diff 93d8ebc..53a3287 shows Some(b'?') arm untouched; grep confirms no write/reply call in additions"
        status: pass
    human_judgment: false
  - id: D4
    description: "No fork regressions: existing hyperlink/link-fidelity/unit tests unaffected by the new field and dispatch arms"
    verification:
      - kind: integration
        ref: "cargo test -p vt100 # full suite + cargo check --workspace --all-targets"
        status: pass
    human_judgment: false

duration: 6min
completed: 2026-09-16
status: complete
---

# Phase 11 Plan 02: vt100 Fork Kitty-Stack Tracking Summary

**Screen::kitty_keyboard() accessor backed by a 32-entry bounded stack tracking the child's CSI > u / CSI < u / CSI = u kitty keyboard ops, with the CSI ? u probe left deliberately unanswered — the child-side verification signal for plan 11-04's enhanced-passthrough gate**

## Performance

- **Duration:** 6 min
- **Started:** 2026-09-16T14:39:42Z
- **Completed:** 2026-09-16T14:45:23Z
- **Tasks:** 1 (TDD: RED → GREEN)
- **Files modified:** 2

## Accomplishments
- `Screen::kitty_keyboard() -> u16` reports the child's active kitty flags (top of stack; 0 = legacy/fresh/fully-popped) — the D-04/D-09 "child end verified" observation that 11-04 wires into `forward_key` in place of the `kitty_child: false` stub
- Bounded, fail-closed stack semantics copying the fork's OSC 8 posture: `KITTY_STACK_MAX = 32` with kitty-spec oldest-eviction, saturating pop (`CSI < 99999 u` on empty is a no-op), unknown set-modes ignored, zero attacker-proportional allocation (T-11-05)
- The child's `CSI ? u` query probe remains dropped on the debug-log fallthrough — no reply path added, so baude never advertises a kitty emulation it does not implement (T-11-06); `Some(b'?')` arm verified untouched between the RED and GREEN commits
- 11-case byte-in/state-out suite in hyperlink.rs's pure-parser shape, including behavioral proof of the depth cap (pop 31 of 40 pushes exposes push #9)

## TDD Evidence (RED → GREEN → REFACTOR)

- **RED** (`93d8ebc`, `test(11-02)`): 11 cases written first. Initial run without the accessor: 14× E0599 compile errors (recorded as context only — INVALID_RED per #3770). A behavior-preserving 0-stub accessor landed with the tests (as the plan's "stubbing nothing" RED spec directs), yielding 6 genuine assertion failures (`push_sets_flags` 0≠1, nested push 0≠5, pop-restore 0≠1, depth cap 0≠40, set establish 0≠2, set OR 0≠5). Evidence at `11-02-red-evidence.json`; `check tdd-red-evidence` → **RED_EVIDENCE_OK** (`target_test_failed`, exit 101).
- **GREEN** (`53a3287`, `feat(11-02)`): `kitty_stack` field, `KITTY_STACK_MAX` const, `kitty_push`/`kitty_pop`/`kitty_set` handlers, three guarded csi_dispatch arms (`Some(b'>') if c == 'u'` etc.). All 11 cases pass; full `cargo test -p vt100` green; `cargo fmt --check` and clippy clean on new code; `cargo check --workspace --all-targets` green.
- **REFACTOR:** skipped — handlers already share the existing `canonicalize_params_1/2` helpers and have no duplication to extract (plan marks REFACTOR conditional on a behavior-preserving improvement existing; none did).

## Task Commits

1. **Task 1 (RED): failing kitty keyboard tracking suite** - `93d8ebc` (test)
2. **Task 1 (GREEN): kitty keyboard-mode tracking implementation** - `53a3287` (feat)

## Files Created/Modified
- `vendor/vt100/tests/kitty_keyboard.rs` - 11 pure-parser cases: push/pop/set semantics, saturating pop, depth cap with oldest-eviction, unanswered probe, unrelated-final fallthrough
- `vendor/vt100/src/screen.rs` - `kitty_stack` field, `KITTY_STACK_MAX` const, `kitty_keyboard()` accessor, private handlers, csi_dispatch arms — all marked `BAUDE FORK (kitty keyboard)`

## Decisions Made
- Unknown `CSI = flags ; mode u` modes (outside 1..=3) ignored with no state change — fail-closed; an invalid mode on an empty stack does not establish an entry
- Depth-cap assertion is behavioral (pop-count arithmetic) because `kitty_stack` is deliberately private — no test-only accessor added
- Probe test also asserts `screen().contents()` unchanged, pinning that the dropped probe perturbs no observable state

## Deviations from Plan

None - plan executed exactly as written. (The RED compile-failure→assertion-stub two-step is the plan's own RED spec — "record RED evidence on the accessor-missing compile failure plus at least one assertion-level failure by stubbing nothing" — not a deviation.)

## Issues Encountered
- `check tdd-red-evidence` parses node TAP only (known from 11-01): cargo libtest output transcribed faithfully to TAP (verbatim names/counts/exit) in the evidence record.

## TDD Gate Compliance

RED → GREEN sequence intact: `test(11-02)` 93d8ebc precedes `feat(11-02)` 53a3287. RED evidence verified `RED_EVIDENCE_OK` (`target_test_failed`). REFACTOR legitimately omitted (optional, no changes made). No violations.

## Authentication Gates

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- `kitty_keyboard()` is ready for 11-04 to replace the `kitty_child: false` stub in `forward_key` (tracked in `.planning/WINDOWS.md` from 11-01) and for subscribe replay
- No blockers; 11-03 (docs/help) independent of this plan

## Self-Check: PASSED

Both files exist on disk; commits 93d8ebc (test) and 53a3287 (feat) present in git log; commits: 2 measured from ledger base 26af97f0 (git rev-list --count).

---
*Phase: 11-negotiated-multiline-input*
*Completed: 2026-09-16*
