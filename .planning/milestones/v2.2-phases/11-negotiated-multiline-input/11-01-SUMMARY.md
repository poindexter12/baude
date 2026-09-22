---
phase: 11-negotiated-multiline-input
plan: 01
subsystem: terminal-input
tags: [kitty-keyboard, crossterm, shift-enter, pty, tdd]

requires:
  - phase: 10-link-handling
    provides: "injected side-effect seam precedents (spawn_opener, route_event) reused as the probe/restore seam shape"
provides:
  - "EncodeCtx { app_cursor, kitty_child, to_shell } — the per-keystroke encode context threading child input mode + destination pane into encode_key"
  - "negotiate_keyboard(probe) — bounded single-shot outer-terminal kitty probe seam; Ok(true) is the only enhanced path"
  - "KEYBOARD_ENHANCED static AtomicBool — pushed-flags marker readable from the 'static panic hook"
  - "write_restore_sequence(w, pop_enhanced) — single restore emission path; conditional pop before LeaveAlternateScreen"
  - "TKEY-02 legacy byte corpus: every encode_key arm frozen byte-for-byte across both panes"
affects: [11-02, 11-03, 11-04, 12-release-gate]

actuals:
  tokens: 4981
  tasks: 3
  commits: 5
plan_head_before: 7fc8d28e6063f9ede5cbcf320e7eb176642c4922

tech-stack:
  added: []
  patterns:
    - "EncodeCtx struct threads child input-mode state to encode_key instead of sibling positional bools"
    - "Injected FnOnce probe seam for terminal capability negotiation (mirrors hook.rs route_event)"
    - "Generic Write-targeted emission seam (write_restore_sequence) for byte-offset restore-order tests"

key-files:
  created: []
  modified:
    - baude/src/keys.rs
    - baude/src/main.rs
    - baude/src/app.rs

key-decisions:
  - "RED-phase scaffolding: EncodeCtx + signature change and an always-false negotiate_keyboard stub landed IN the test commit (behavior-preserving) so all RED tests fail on assertions, not compile errors (#3770 INVALID_RED rule); plan had expected Test 4 to fail-to-compile"
  - "RED evidence for the Rust run recorded as a faithful TAP transcription (names/counts/exit verbatim from cargo) because check tdd-red-evidence parses node TAP only; verdict RED_EVIDENCE_OK"
  - "Shell-pane Shift+Enter degrades to plain CR (ESC CR is meta-CR to readline); Claude-pane fallback is ESC CR; CSI-u only under ctx.kitty_child (D-09)"

patterns-established:
  - "Pane-invariance freeze: every legacy corpus row asserted under both to_shell values with identical bytes"
  - "Pop-iff-pushed via AtomicBool::swap so double restore emits the pop exactly once"

requirements-completed: [TKEY-01, TKEY-02, TKEY-03, TKEY-04, TKEY-05]

coverage:
  - id: D1
    description: "Shift+Enter on the verified path encodes ESC CR toward the Claude-pane child; CSI-u passthrough only under a kitty_child ctx; plain CR to the shell pane"
    requirement: TKEY-01
    verification:
      - kind: unit
        ref: "cargo test -p baude keys # keys::tests::shift_enter_legacy_child_claude_pane_inserts_esc_cr + shell/kitty matrix + idempotency"
        status: pass
    human_judgment: false
  - id: D2
    description: "Every non-Shift+Enter key's child bytes frozen byte-for-byte across both panes and both DECCKM states"
    requirement: TKEY-02
    verification:
      - kind: unit
        ref: "cargo test -p baude keys # keys::tests::legacy_corpus_bytes_are_frozen"
        status: pass
    human_judgment: false
  - id: D3
    description: "Probe Ok(false)/Err paths are legacy and non-fatal; legacy restore emission byte-identical to pre-phase"
    requirement: TKEY-03
    verification:
      - kind: unit
        ref: "cargo test -p baude keyboard_negotiation # probe_failure_and_refusal_are_legacy_never_fatal + legacy_restore_emits_no_pop_and_matches_pre_phase_bytes"
        status: pass
    human_judgment: false
  - id: D4
    description: "Pop emitted iff pushed, ordered before the alternate-screen leave, on the shared restore path serving panic/error/normal exits; double restore pops once"
    requirement: TKEY-04
    verification:
      - kind: unit
        ref: "cargo test -p baude keyboard_negotiation # pop_precedes_alternate_screen_leave_when_pushed + double_restore_pops_exactly_once"
        status: pass
    human_judgment: false
  - id: D5
    description: "Startup probe runs exactly once, pre-loop, bounded by crossterm's internal 2000 ms deadline — cannot block input indefinitely"
    requirement: TKEY-05
    verification:
      - kind: unit
        ref: "cargo test -p baude keys # keys::tests::negotiate_keyboard_gates_on_probe_result (Err == legacy)"
        status: pass
    human_judgment: true
    rationale: "The single-shot pre-loop placement and the 2 s bound are structural (call-site position + crossterm internals) — tests prove the Err→legacy contract, but 'never blocks startup on a real terminal' needs the Phase 12 real-terminal validation"

duration: 28min
completed: 2026-09-16
status: complete
---

# Phase 11 Plan 01: Negotiated Shift+Enter Tracer Summary

**End-to-end negotiated multiline path: bounded kitty probe seam → gated DISAMBIGUATE push → EncodeCtx-driven Shift+Enter (ESC CR to Claude pane, CR to shell, CSI-u only for kitty children) → conditional pop in the shared restore path, plus a full legacy byte-freeze corpus**

## Performance

- **Duration:** 28 min
- **Started:** 2026-09-16T14:05:55Z
- **Completed:** 2026-09-16T14:34:37Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments
- Replaced the latent Shift+Enter misfire (unconditional CSI-u to children that never negotiated) with the D-09 conditional: CSI-u iff `ctx.kitty_child`, ESC CR fallback insert to the Claude pane, plain CR to the shell pane
- `negotiate_keyboard` probe seam + `KEYBOARD_ENHANCED` static + gated `DISAMBIGUATE_ESCAPE_CODES` push in the single-threaded pre-loop window after `EnterAlternateScreen`
- `write_restore_sequence` seam: pop-before-alternate-screen-leave proven by byte-offset assertions; double-restore safety via `AtomicBool::swap`; panic/error/normal exits all route through it unchanged
- TKEY-02 corpus: 57 base rows × both panes freeze every encode_key arm (ctrl chords, arrows in both DECCKM states, modified CSI forms, tilde keys, F1–F12, UTF-8 chars)

## Task Commits

1. **Task 1 (tracer, RED): failing Shift+Enter negotiation tests** - `46a8c84` (test) — RED evidence `RED_EVIDENCE_OK` (`target_test_failed`, exit 101)
2. **Task 1 (tracer, GREEN): probe, gated push, encode, pop** - `2a4636a` (feat) — tracer verified end-to-end post-commit (keys tests + workspace check re-run green)
3. **Task 2: legacy byte corpus + matrix + idempotency** - `9feaea4` (test)
4. **Task 3: write_restore_sequence seam + edge tests** - `3996274` (refactor)
5. **Fix: rustfmt import wrap** - `0e2271d` (style)

## Files Created/Modified
- `baude/src/keys.rs` - `EncodeCtx`, rewritten Enter arm, `keys::tests` (4 behavior tests + corpus + matrix + idempotency)
- `baude/src/main.rs` - `negotiate_keyboard`, `KEYBOARD_ENHANCED`, gated push call site, `write_restore_sequence`, `keyboard_negotiation_tests`
- `baude/src/app.rs` - `forward_key` builds `EncodeCtx` in both local and remote-attach branches (`kitty_child` fail-closed false)

## Decisions Made
- RED scaffolding decision (see key-decisions): signature/struct groundwork in the test commit with behavior untouched, so RED failures are genuine assertions per #3770
- Shell-pane degradation to plain CR kept out of research doubt: ESC CR is readline meta-CR and must not reach bash
- Task 3 committed as `refactor` (seam extraction + tests in one atomic change); plan-level RED gate already satisfied by Task 1's `test(11-01)` commit

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] RED shape adjusted: compile-failure RED converted to assertion RED**
- **Found during:** Task 1 (RED phase)
- **Issue:** Plan expected Test 4 to "fail to compile" and Tests 1–3 to use a not-yet-existing `EncodeCtx`; a compile error blocks the whole test target, which is INVALID_RED under #3770 and would yield no assertion evidence for Tests 1–2
- **Fix:** RED commit carries behavior-preserving scaffolding (EncodeCtx + signature change with the old unconditional CSI-u arm intact; always-false `negotiate_keyboard` stub) so Tests 1, 2, 4 fail on real assertions; verified `RED_EVIDENCE_OK`
- **Files modified:** baude/src/keys.rs, baude/src/main.rs, baude/src/app.rs
- **Verification:** `check tdd-red-evidence` → `RED_EVIDENCE_OK` / `target_test_failed`; cargo exit 101 with 3 named assertion failures
- **Committed in:** 46a8c84

**2. [Rule 1 - Bug] rustfmt drift in edited imports**
- **Found during:** Post-task verification (`cargo fmt --check`, CI-enforced)
- **Issue:** Import wrap in main.rs diverged from rustfmt output
- **Fix:** `cargo fmt -p baude`, tests re-run green
- **Files modified:** baude/src/main.rs
- **Verification:** `cargo fmt --check` exit 0
- **Committed in:** 0e2271d

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug)
**Impact on plan:** None on scope or behavior — RED discipline strengthened, formatting aligned with CI. No scope creep.

## Known Stubs

- `baude/src/app.rs` (forward_key, both branches): `kitty_child: false` hardcoded — **intentional, fail-closed per D-04** (an unobserved child push is by definition unverified). Plan 11-04 replaces the constant with the vt100 `kitty_keyboard()` observed-push read. Logged to `.planning/WINDOWS.md`. Does not block this plan's goal (the tracer path is the legacy-child ESC CR path, fully wired).

## Issues Encountered
- `cargo clippy --all-targets -- -D warnings` (CI invocation) fails locally on pre-existing issues (vendored vt100, package-metadata lints) unrelated to this plan's files — zero clippy diagnostics in keys.rs/main.rs/app.rs. Logged to `deferred-items.md`, not fixed (scope boundary).
- `gsd_run check tdd-red-evidence` parses node TAP only; cargo libtest output required a faithful TAP transcription (verbatim names/counts/exit) to record RED evidence.

## TDD Gate Compliance

RED → GREEN → REFACTOR sequence intact: `test(11-01)` 46a8c84 precedes `feat(11-01)` 2a4636a; optional `refactor(11-01)` 3996274 present with tests green. RED evidence verified `RED_EVIDENCE_OK`. No violations.

## Authentication Gates

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Ready for 11-02 (wave 2): EncodeCtx and the negotiation/restore seams are stable interfaces for expansion
- 11-04 must replace `kitty_child: false` with the vt100 observed-push accessor (tracked stub)
- Real-terminal residue/latency validation deliberately deferred to Phase 12 (D-08)

## Self-Check: PASSED

All 3 modified files exist on disk; all 5 commit hashes present in git log; `commits: 5` measured from ledger base 7fc8d28e (`git rev-list --count`).

---
*Phase: 11-negotiated-multiline-input*
*Completed: 2026-09-16*
