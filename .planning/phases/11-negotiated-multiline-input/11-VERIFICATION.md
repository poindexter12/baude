---
phase: 11-negotiated-multiline-input
verified: 2026-09-16T16:05:00Z
status: passed
score: 14/15 must-haves verified
covered_files:
  - .planning/REQUIREMENTS.md
  - .planning/phases/11-negotiated-multiline-input/11-01-PLAN.md
  - .planning/phases/11-negotiated-multiline-input/11-01-SUMMARY.md
  - .planning/phases/11-negotiated-multiline-input/11-02-PLAN.md
  - .planning/phases/11-negotiated-multiline-input/11-02-SUMMARY.md
  - .planning/phases/11-negotiated-multiline-input/11-03-PLAN.md
  - .planning/phases/11-negotiated-multiline-input/11-03-SUMMARY.md
  - .planning/phases/11-negotiated-multiline-input/11-04-PLAN.md
  - .planning/phases/11-negotiated-multiline-input/11-04-SUMMARY.md
  - README.md
  - baude-core/src/pty.rs
  - baude/src/app.rs
  - baude/src/keys.rs
  - baude/src/main.rs
  - baude/src/ui.rs
  - vendor/vt100/src/screen.rs
  - vendor/vt100/tests/kitty_keyboard.rs
covered_digest: "v1:sha256:f62eab48e233b5b3e166f917f1bacfe5b07b489b0041793eb837677a7a783f0a"
behavior_unverified: 1
overrides_applied: 0
behavior_unverified_items:
  - truth: "Interleaved key events during or after negotiation never emit an enhanced sequence on an unverified path (11-01 backstop truth)"
    test: "On a kitty-capable terminal, mash keys during baude startup (the ~0-2 s probe window) and immediately after; then run baude inside a plain terminal (probe false) and press Shift+Enter plus assorted keys in both panes"
    expected: "No CSI-u byte sequence (ESC [ 13;2u or any ESC [ ...u) ever reaches a child that did not itself push kitty flags; startup never hangs"
    why_human: "Backstop-tier truth: the encoding half is test-proven (corpus + composed matrix — unverified ctx always yields legacy bytes), but the timing half (probe owns the event source in the single-threaded pre-loop window; no event can interleave) is a structural argument no test exercises. Presence + wiring cannot certify a runtime interleaving invariant."
human_verification:
  - test: "Backstop truth — interleaved events on an unverified path (see behavior_unverified_items above)"
    expected: "No enhanced bytes on any unverified path; startup never blocks on the probe"
    why_human: "verification: backstop marker in 11-01-PLAN must_haves; requires directly observed runtime behavior"
  - test: "Prohibition (flagged-unverified): 'baude never guesses a missing modifier' — LLM-judge verdict: UPHELD, non-authoritative (no timing heuristics anywhere in the Enter arm; legacy terminals deliver Shift+Enter as plain Enter and it submits; README states this truthfully)"
    expected: "Human confirms no modifier inference exists on any input path"
    why_human: "Planner marked verification: flagged-unverified — must not be silently absorbed into a pass"
  - test: "Prohibition (flagged-unverified): 'enhanced bytes are never sent on an unverified path' — LLM-judge verdict: UPHELD, non-authoritative (kitty_child gate test-proven; note review IN-01: the outer-probe leg is enforced indirectly via the gated push, not read on the input path)"
    expected: "Human accepts the indirect outer-leg enforcement documented in 11-REVIEW.md IN-01"
    why_human: "Planner marked verification: flagged-unverified"
  - test: "Prohibition (flagged-unverified): 'keyboard mode is never left pushed after any controlled exit' — LLM-judge verdict: UPHELD, non-authoritative (pop-iff-pushed byte-offset tests; panic hook, normal exit, and the WR-01 Terminal::new Err path all route through restore_terminal)"
    expected: "Human confirms exit paths are exhaustively covered; real-terminal residue check folds into Phase 12 dogfood per D-08"
    why_human: "Planner marked verification: flagged-unverified"
  - test: "Prohibition (flagged-unverified): 'legacy key bytes never change' — LLM-judge verdict: UPHELD, non-authoritative (57-row corpus x both panes frozen; review diffed 3ab66a8..HEAD — only the Shift+Enter arm changed)"
    expected: "Human accepts the corpus as the byte-freeze authority"
    why_human: "Planner marked verification: flagged-unverified"
  - test: "Prohibition (flagged-unverified): 'negotiation never blocks startup or input indefinitely' — LLM-judge verdict: UPHELD, non-authoritative (single pre-loop probe; bound is crossterm's internal 2000 ms deadline — a library property, not locally testable)"
    expected: "Human accepts the crossterm-internal bound as sufficient (per RESEARCH Pitfall 1)"
    why_human: "Planner marked verification: flagged-unverified; the 2 s bound lives inside the dependency"
  - test: "Prohibition (flagged-unverified): 'baude never answers the child's CSI ? u probe on the vt parser path' — LLM-judge verdict: UPHELD, non-authoritative (no reply channel exists in the fork; query_probe regression test green; csi_dispatch '?' arm untouched)"
    expected: "Human confirms observation-without-advertisement stays the fork contract"
    why_human: "Planner marked verification: flagged-unverified"
deferred:
  - truth: "Shift+Enter inserts a newline end-to-end on a real kitty-capable terminal (live dogfood, outer terminal residue check)"
    addressed_in: "Phase 12"
    evidence: "11-04-PLAN planner assumptions: 'Live end-to-end verification on a real terminal folds into Phase 12 SHIP-03 per CONTEXT'; 11-01-PLAN D-08: 'real-terminal residue → Phase 12'"
---

# Phase 11: Negotiated Multiline Input Verification Report

**Phase Goal:** Users can insert newlines with Shift+Enter on verified terminal paths without changing existing key behavior or leaking terminal modes.
**Verified:** 2026-09-16T16:05:00Z
**Status:** human_needed
**Re-verification:** No — initial verification

## Goal Achievement

### ROADMAP Success Criteria

| # | Success Criterion | Status | Evidence |
|---|-------------------|--------|----------|
| 1 | Shift+Enter inserts a newline without submitting Claude/claudex prompts on documented, tested terminal paths | ✓ VERIFIED | `keys.rs:60-81` Shift+Enter arm: `\x1b\r` fallback insert to Claude pane, `\x1b[13;2u` only when `ctx.kitty_child`; composed-matrix test through the real derivation (`forward_ctx_tests`, 4/4 green); real-terminal dogfood deferred to Phase 12 (see deferred) |
| 2 | Ordinary Enter, Ctrl-C, navigation keys, existing baude shortcuts retain behavior | ✓ VERIFIED | 57-row `legacy_corpus()` x both panes frozen (`keys` suite 8/8 green); 11-REVIEW diffed pre-phase `3ab66a8` vs HEAD — only the Shift+Enter arm changed (and pre-phase was fail-OPEN: unconditional CSI-u; the phase closed that hole) |
| 3 | Terminals that cannot distinguish Shift+Enter retain legacy behavior with documented setup/fallback guidance | ✓ VERIFIED | `negotiate_keyboard` Ok(false)/Err → legacy, non-fatal (`keyboard_negotiation` 4/4); `write_restore_sequence(w,false)` byte-identical to pre-phase; help overlay row (`ui.rs:2215-2216`, render test green); README `## Keys` row + `### Shift+Enter newlines` caveat with verified terminals, exact legacy behavior, Claude Code/OpenCode fallbacks, tmux `extended-keys` guidance |
| 4 | Prior outer keyboard mode restored on controlled exit/failure/suspend paths, re-established on resume | ✓ VERIFIED | Pop-iff-pushed via `KEYBOARD_ENHANCED.swap`; pop queued BEFORE LeaveAlternateScreen (byte-offset asserts); shared `restore_terminal()` serves panic hook (`main.rs:422`), normal exit (`main.rs:474`), and the WR-01-fixed `Terminal::new` Err path (`main.rs:453-460`); double-restore pops exactly once. Suspend leg VACUOUS by design — baude has no SIGTSTP handling (documented at `main.rs:135-139`, per D-06/RESEARCH Q5) |
| 5 | Negotiation cannot block startup/input indefinitely; enhanced sequences only on a verified outer/child path | ✓ VERIFIED | Single-shot probe at `main.rs:440`, pre-loop, before `run()`; bounded by crossterm's internal 2000 ms deadline (RESEARCH Pitfall 1, documented in code); Err/false → legacy. Child leg: `kitty_child = screen.kitty_keyboard() != 0` observed-push gate, fail-closed at every fallback (fresh parser, pop, poisoned lock, RIS, alt-screen exit). Outer leg enforced indirectly via the gated push (review IN-01, accepted) — the backstop timing half routes to human verification |

**Roadmap contract:** 5/5 success criteria supported in code; SC5's backstop invariant and the six planner-flagged prohibitions route to human review (below).

### Observable Truths (merged PLAN must_haves)

| # | Truth (plan) | Status | Evidence |
|---|--------------|--------|----------|
| 1 | Shift+Enter on kitty-capable outer writes ESC CR to child, not bare CR (11-01) | ✓ VERIFIED | `shift_enter_legacy_child_claude_pane_inserts_esc_cr` green; wired via `forward_key` → `encode_ctx` → `encode_key` |
| 2 | Repeated Shift+Enter idempotent, no state accumulation (11-01) | ✓ VERIFIED | `shift_enter_encoding_is_idempotent` green; `encode_key` pure fn |
| 3 | Interleaved events during/after negotiation never emit enhanced on unverified path (11-01, backstop) | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | Encoding half test-proven (corpus: every kitty_child:false ctx yields legacy bytes); timing half (probe owns event source pre-loop) is structural only — no test exercises interleaving. Routed to human verification |
| 4 | Probe Ok(false)/Err pushes nothing; child bytes byte-identical to pre-phase (11-01) | ✓ VERIFIED | `probe_failure_and_refusal_are_legacy_never_fatal` + legacy-restore byte-identity test green; corpus locks bytes |
| 5 | restore_terminal pops iff pushed, before alt-screen leave, on the one shared function (11-01) | ✓ VERIFIED | `write_restore_sequence` (`main.rs:118-129`) queues pop first; byte-offset asserts green; all three exit classes + WR-01 init-error path call `restore_terminal()` |
| 6 | Probe runs exactly once, pre-loop, bounded (11-01) | ✓ VERIFIED | Sole call site `main.rs:440`, after EnterAlternateScreen, before `run()`; bound is crossterm-internal (declared precondition, documented `main.rs:434-439`) |
| 7 | vt100 fork tracks child kitty state: `>u` push, `<u` saturating pop, `=u` set (11-02) | ✓ VERIFIED | `screen.rs:1765-1782` csi_dispatch arms; `kitty_keyboard` test target 14/14 green |
| 8 | `kitty_keyboard()` returns current flags; 0 default and after full pop (11-02) | ✓ VERIFIED | `screen.rs:728-730` reads active per-screen stack top; fresh/pop/RIS cases green |
| 9 | Hostile counts cannot panic/exhaust: pop saturates, depth capped at 32 (11-02) | ✓ VERIFIED | `KITTY_STACK_MAX=32` oldest-eviction (`screen.rs:1390-1396`), `truncate(len - min(n,len))` pop; hostile cases green |
| 10 | Child's CSI ? u query stays UNANSWERED — no reply path (11-02) | ✓ VERIFIED | No reply channel anywhere in fork; `?` intermediate arm untouched (`screen.rs:1759-1763` fallthrough); regression test green |
| 11 | Help overlay shows shift+enter binding + README pointer (11-03) | ✓ VERIFIED | `ui.rs:2215-2216` two-line row; `help_overlay_lists_shift_enter` render test asserts buffer content + closing line (clip guard) |
| 12 | README documents supported terminals, legacy behavior, fallbacks (11-03) | ✓ VERIFIED | `README.md:120` Keys row; `README.md:148-166` caveat: verified terminals, legacy submit behavior, backslash-Enter//terminal-setup/ctrl+j fallbacks, tmux extended-keys; no TERM-detection claims |
| 13 | CSI 13;2u ONLY after observed child push; else ESC CR / CR (11-04) | ✓ VERIFIED | `encode_ctx` (`app.rs:5716-5721`): `kitty_child: screen.kitty_keyboard() != 0`; `composed_shift_enter_matrix_through_real_derivation` green |
| 14 | Local and remote-attach forward_key branches share one derivation helper (11-04) | ✓ VERIFIED | Both branches (`app.rs:3938-3946`, `app.rs:3962-3970`) call `encode_ctx` under their parser locks with fail-closed `unwrap_or` legacy fallback |
| 15 | Remote mirror converges on child kitty state via subscribe snapshot replay (11-04) | ✓ VERIFIED | Full-depth per-screen replay (`pty.rs:410-412` main stack pre-switch, `pty.rs:438-442` alternate stack post-switch, WR-03 fix); `subscribe` suite 6/6 green incl. nested-depth and mid-alt-screen round-trips; inactive child adds zero bytes |

**Score:** 14/15 truths verified (1 present, behavior-unverified)

### Deferred Items

| # | Item | Addressed In | Evidence |
|---|------|--------------|----------|
| 1 | Live end-to-end Shift+Enter on a real kitty-capable terminal (dogfood + outer-terminal residue check) | Phase 12 | 11-04-PLAN: "Live end-to-end verification on a real terminal folds into Phase 12 SHIP-03 per CONTEXT"; 11-01-PLAN D-08: "real-terminal residue → Phase 12" |

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `baude/src/keys.rs` | EncodeCtx, rewritten Enter arm, corpus + matrix tests (`contains: EncodeCtx`) | ✓ VERIFIED | 335 lines; `EncodeCtx` at line 7; conditional Shift+Enter arm at 60-81; 8 tests incl. 57-row corpus x2 panes |
| `baude/src/main.rs` | negotiate_keyboard, KEYBOARD_ENHANCED, gated push, write_restore_sequence, pop in restore (`contains: KEYBOARD_ENHANCED`) | ✓ VERIFIED | All present (lines 104, 110, 118, 131, 440-448); `keyboard_negotiation_tests` module with byte-offset restore-order asserts |
| `baude/src/app.rs` | encode_ctx helper; forward_key reads kitty_keyboard() in both branches (`contains: kitty_keyboard`) | ✓ VERIFIED | `encode_ctx` at 5716; both branches wired; `forward_ctx_tests` 4/4 |
| `vendor/vt100/src/screen.rs` | kitty stack + csi_dispatch arms + accessor (`contains: kitty_keyboard`) | ✓ VERIFIED | Per-screen stacks (WR-02 fix), `kitty_keyboard()`, `kitty_main_stack()`/`kitty_alternate_stack()` (WR-03), BAUDE FORK markers throughout |
| `vendor/vt100/tests/kitty_keyboard.rs` | byte-in/state-out integration tests | ✓ VERIFIED | 14 tests green (11 original + 3 WR-02 per-screen cases) |
| `baude-core/src/pty.rs` | subscribe() replays active child kitty push (`contains: kitty_keyboard`) | ✓ VERIFIED | Full-depth per-screen replay in `subscribe()`; 6 subscribe tests green |
| `baude/src/ui.rs` | shift+enter help row + render test (`contains: shift+enter`) | ✓ VERIFIED | Row + continuation line at 2215-2216; `help_overlay_lists_shift_enter` green |
| `README.md` | Keys row + supported-terminals/fallback caveat (`contains: shift+enter`) | ✓ VERIFIED | Row at line 120; caveat subsection at 148-166 with extended-keys guidance |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `baude/src/app.rs` | `baude/src/keys.rs` | forward_key builds EncodeCtx, calls `encode_key(&key, ctx)` | ✓ WIRED | `app.rs:3947`, `app.rs:3971` — both branches |
| `baude/src/main.rs` | `baude/src/main.rs` | restore_terminal drives write_restore_sequence with swapped static | ✓ WIRED | `main.rs:133-140`; panic hook 422, exit 474, WR-01 path 457 all call restore_terminal |
| `baude/src/app.rs` | `vendor/vt100/src/screen.rs` | encode_ctx reads `screen.kitty_keyboard() != 0` | ✓ WIRED | `app.rs:5719` |
| `baude-core/src/pty.rs` | `vendor/vt100/src/screen.rs` | subscribe() conditions replay on kitty stack accessors | ✓ WIRED | `pty.rs:410`, `pty.rs:439` (full-depth, per-screen — supersedes the plan's single-push shape per WR-03) |
| `vendor/vt100/tests/kitty_keyboard.rs` | `vendor/vt100/src/screen.rs` | Parser::process + `kitty_keyboard()` asserts | ✓ WIRED | 14 passing byte-in/state-out cases |
| `baude/src/ui.rs` | `README.md` | help line points at README | ✓ WIRED | `ui.rs:2216`: "(kitty-capable terminals; see README)" |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Shift+Enter matrix + legacy byte corpus | `cargo test -p baude keys` | 8 passed, 0 failed | ✓ PASS |
| Probe edges + restore ordering + double-restore | `cargo test -p baude keyboard_negotiation` | 4 passed, 0 failed | ✓ PASS |
| ctx derivation + composed matrix | `cargo test -p baude forward_ctx` | 4 passed, 0 failed | ✓ PASS |
| Help overlay render | `cargo test -p baude help_overlay` | 1 passed, 0 failed | ✓ PASS |
| Fork kitty tracking + hostile input + unanswered probe | `cargo test -p vt100 --test kitty_keyboard` | 14 passed, 0 failed | ✓ PASS |
| Fork regression (full vt100 suite) | `cargo test -p vt100` | all targets green (16 + 14 + unit + doc) | ✓ PASS |
| Subscribe replay round-trips | `cargo test -p baude-core subscribe` | 6 passed, 0 failed | ✓ PASS |

### Probe Execution

No `scripts/*/tests/probe-*.sh` probes exist in this repository and no PLAN declares any — N/A.

### Requirements Coverage

| Requirement | Source Plans | Description | Status | Evidence |
|-------------|--------------|-------------|--------|----------|
| TKEY-01 | 11-01, 11-02, 11-04 | Shift+Enter inserts newline without submitting on tested paths | ✓ SATISFIED | Encode arm + composed matrix + idempotency tests; enhanced only behind observed-push gate |
| TKEY-02 | 11-01, 11-04 | Enter/Ctrl-C/nav/shortcuts retain behavior | ✓ SATISFIED | Frozen corpus (57 rows x both panes); review byte-diff confirms only Shift+Enter arm changed |
| TKEY-03 | 11-01, 11-03 | Legacy terminals keep behavior; documented guidance; no modifier guessing | ✓ SATISFIED | Probe false/Err → legacy tests; help overlay + README caveat; no heuristics in Enter arm |
| TKEY-04 | 11-01 | Prior keyboard mode restored on exit/failure/suspend; re-established on resume | ✓ SATISFIED | Pop-iff-pushed + ordering tests on the shared restore path + WR-01; suspend leg vacuous by design (no SIGTSTP handling exists — documented) |
| TKEY-05 | 11-01, 11-02, 11-04 | Negotiation cannot block; enhanced only on verified path | ✓ SATISFIED | Bounded single-shot pre-loop probe; dual-gate (outer push + observed child push); subscribe replay keeps remote parity |

No orphaned requirements: REQUIREMENTS.md maps exactly TKEY-01..TKEY-05 to Phase 11; all five appear in plan frontmatter.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `vendor/vt100/src/screen.rs` | 1490 | `XXX` comment | ℹ️ Info | Pre-existing upstream vt100 code — zero XXX additions in the phase diff (`git diff 3ab66a8..HEAD` contains none); not a phase debt marker |

No TODO/FIXME/PLACEHOLDER/stub patterns in any phase-modified file. Review items IN-02 (Relaxed ordering on KEYBOARD_ENHANCED), IN-03 (duplicated negotiate_keyboard test in keys.rs), IN-04 (duplicated fail-closed fallback literal) remain open as accepted Info-level findings — none block the goal.

### Code Review Cross-Check

11-REVIEW.md found 0 Critical, 3 Warnings — all three verified FIXED in code, not just claimed:

- **WR-01** (`64550bc`): `Terminal::new` Err now routes through `restore_terminal()` before propagating — confirmed at `main.rs:453-460` with explanatory comment.
- **WR-02** (`29bee2d`): per-screen kitty stacks (`kitty_stack` + `kitty_alternate_stack`), alternate stack cleared on activation (`screen.rs:837-841`), `kitty_keyboard()` reads the active screen's stack — confirmed; 3 new fork tests green.
- **WR-03** (`b49f57f`): subscribe replays full per-screen stack depth via new fork accessors, main stack before `?1049h`, alternate after — confirmed at `pty.rs:398-442`; nested-depth and mid-alt-screen round-trip tests green.

### Human Verification Required

#### 1. Backstop truth: no enhanced bytes on an unverified path under interleaving

**Test:** On a kitty-capable terminal, mash keys during baude startup and immediately after; then run baude in a plain (non-kitty) terminal and press Shift+Enter plus assorted keys in both panes.
**Expected:** No CSI-u sequence ever reaches a child that did not push kitty flags; startup never hangs on the probe.
**Why human:** `verification: backstop` in 11-01-PLAN. The encoding half is test-proven; the timing half (single-threaded pre-loop probe window) is structural and unexercised by any test.

#### 2-7. Six planner-flagged prohibitions (`verification: flagged-unverified`)

All six carry non-authoritative UPHELD LLM-judge verdicts backed by tests and code reading (detailed in frontmatter), but the planner explicitly flagged them as requiring human resolution — they cannot be silently absorbed into a pass:

1. baude never guesses a missing modifier — UPHELD (no heuristics in the Enter arm)
2. enhanced bytes never sent on an unverified path — UPHELD (note IN-01's indirect outer-leg enforcement)
3. keyboard mode never left pushed after any controlled exit — UPHELD (incl. WR-01 path)
4. legacy key bytes never change — UPHELD (frozen corpus + review byte-diff)
5. negotiation never blocks startup/input indefinitely — UPHELD (bound is crossterm-internal, not locally testable)
6. baude never answers the child's CSI ? u probe — UPHELD (no reply channel; regression-tested)

Most of these fold naturally into the Phase 12 real-terminal dogfood session.

### Gaps Summary

No gaps. All 15 plan truths are implemented and wired; 14 are behaviorally verified by passing named test suites, and 1 (the backstop interleaving invariant) is present-but-behavior-unverified by design of its `backstop` marker. All 5 ROADMAP success criteria are supported in code. All three code-review warnings are verified fixed in the codebase. The suspend leg of TKEY-04 is vacuous by verified design (no SIGTSTP handling exists), and the real-terminal dogfood is explicitly deferred to Phase 12 with plan-text evidence. Status is `human_needed` solely because the backstop truth and the six planner-flagged prohibitions require explicit human resolution.

---

_Verified: 2026-09-16T16:05:00Z_
_Verifier: Claude (gsd-verifier)_
