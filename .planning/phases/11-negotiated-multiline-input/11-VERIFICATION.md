---
phase: 11-negotiated-multiline-input
verified: 2026-09-18T00:00:00Z
status: passed
human_items_accepted: "2026-09-18 by Joe Seymour — accepted, NOT observed; see Acceptance Record"
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
covered_digest: "v1:sha256:b5bb9812488ed3ad6c90ef3a5de332f92b120a8ae9bc1b10b50bb9c638dda36e"
behavior_unverified: 1
overrides_applied: 0
re_verification:
  previous_status: "human_needed (body) / passed (frontmatter) — the prior report was self-contradictory; resolved here to human_needed, which the prior body text and its own Gaps Summary both support"
  previous_score: 14/15
  trigger: "covered_digest went stale — three covered files changed after it was computed: .planning/REQUIREMENTS.md (bookkeeping), baude/src/ui.rs (plan 12-01 clippy fix), README.md (plan 12-02 SHIP-02 restructure)"
  head_verified: ee6fa3c25733f7f499acc11a4ac150ac3bcfa712
  gaps_closed: []
  gaps_remaining: []
  regressions: []
  staleness_findings:
    - file: baude/src/ui.rs
      change: "b4d482a replaced `links.len().min(10).max(1)` with `links.len().clamp(1, 10)` at ui.rs:2120, inside draw_modal's Modal::Links arm"
      impact: "NONE on phase 11. Behavior-preserving for every usize (max(min(x,10),1) == clamp(1,10); bounds are constants so clamp's panic path is unreachable) — already proven in the phase-10 re-verification and cited here rather than re-derived. The edit is ~95 lines above the help-overlay work and touches a different match arm; the shift+enter rows at ui.rs:2215-2216 and the centered(area, 60, 39) clip budget at ui.rs:2177 are untouched. help_overlay_lists_shift_enter re-run green."
    - file: README.md
      change: "12-02 (cc914e2, 35dd0e6, da50929) restructured the terminal-support prose and MOVED the tested-terminals list out of the kitty-keyboard paragraph into its own `### Tested terminals` section at README.md:222"
      impact: "NONE on TKEY-03. All four guidance elements survived intact in `### Shift+Enter newlines` (README.md:149-168) and the Keys row at README.md:121. The moved list did not orphan the guidance: the section now carries an explicit in-document link, `[Tested terminals](#tested-terminals)` (README.md:154), whose anchor resolves to the new `### Tested terminals` heading. Discoverability is preserved, and the moved section names shift+enter first among the negotiated gestures (README.md:224)."
    - file: .planning/REQUIREMENTS.md
      change: "bookkeeping only — TKEY-01..TKEY-05 still present, still mapped to Phase 11, all Complete"
      impact: "NONE"
  core_files_undisturbed: "Confirmed, not assumed. git log -1 per file: keys.rs → fbcbb01 (11-01), main.rs → 1d7ece6 (WR-01), app.rs → 9c7b6a3 (11-04), screen.rs → a3200e6 (WR-03), pty.rs → a3200e6 (WR-03), kitty_keyboard.rs → b64f04e (WR-02). No post-phase-11 commit touches any of the six."
  new_finding: "The phase-11 deferral to Phase 12 came back UNRESOLVED — see carried_forward_unresolved below. This is new information that did not exist at the prior verification."
carried_forward_unresolved:
  - item: "Real-terminal confirmation that Shift+Enter inserts a newline and ordinary Enter still submits (TKEY-01 / TKEY-02 live leg)"
    previously: "Listed as `deferred` → Phase 12 in the prior report (11-04-PLAN: 'Live end-to-end verification on a real terminal folds into Phase 12 SHIP-03'; 11-01-PLAN D-08)"
    now: "NO LONGER a valid deferral. Phase 12's smoke session ran and returned a SIGNED GAP covering exactly this leg. 12-SMOKE-EVIDENCE.md legs 8 and 9 are recorded as 'Not individually reported', covered only by a set-level attestation ('all works flawlessly'), and gap 3 states plainly: 'Legs 8-9 may not have been exercised. They require a claude pane; the smoke instance ran under BAUDE_WORKSPACE=smoke, which starts empty. Whether a session was added first was not confirmed.' Signed off by Joe Seymour on 2026-09-16, and that same gap explicitly names phase 11: 'phase 11's own verification routed the real-terminal leg here — so if these two were not run, that routing is still open.'"
    status: "OPEN — promoted out of `deferred` into human verification. TKEY-01 and TKEY-02 rest on automated coverage plus a set-level bulk attestation, NOT on a confirmed live keystroke."
prior_human_acceptance:
  date: 2026-09-16
  gate: "autonomous validation gate"
  decision: "Maintainer presented with the backstop timing claim and the six flagged prohibitions; responded 'All good — continue'"
  recorded_as: "ACCEPTANCE (a decision), NOT evidence. It does not raise the verified score, does not convert the backstop truth to VERIFIED, and was never formalized as an `overrides:` entry. The score below is identical to what it would be without the acceptance."
behavior_unverified_items:
  - truth: "Interleaved key events during or after negotiation never emit an enhanced sequence on an unverified path (11-01 backstop truth)"
    test: "On a kitty-capable terminal, mash keys during baude startup (the ~0-2 s probe window) and immediately after; then run baude inside a plain terminal (probe false) and press Shift+Enter plus assorted keys in both panes"
    expected: "No CSI-u byte sequence (ESC [ 13;2u or any ESC [ ...u) ever reaches a child that did not itself push kitty flags; startup never hangs"
    why_human: "Backstop-tier truth (`verification: backstop` in 11-01-PLAN). The encoding half is test-proven (57-row corpus + composed matrix — every kitty_child:false ctx yields legacy bytes); the timing half (the probe owns the event source in the single-threaded pre-loop window, so no event can interleave) is a structural argument no test exercises. Presence + wiring cannot certify a runtime interleaving invariant. Accepted by the maintainer 2026-09-16 as a decision; still behaviorally unexercised."
human_verification:
  - test: "REAL-TERMINAL LEG (carried forward, UNRESOLVED): in a claude pane on a kitty-capable terminal, press shift+enter, then press enter"
    expected: "shift+enter inserts a newline and submits nothing; ordinary enter submits, behavior unchanged"
    why_human: "Phase 11 routed this to the Phase 12 smoke session; that session returned signed gap 3 saying legs 8-9 may never have been exercised (BAUDE_WORKSPACE=smoke starts with no claude pane, and whether one was added was not confirmed). Requires a live keystroke in a real terminal with a real claude session present. This is the one item where TKEY-01/TKEY-02 lack live confirmation."
  - test: "Backstop truth — interleaved events on an unverified path (see behavior_unverified_items above)"
    expected: "No enhanced bytes on any unverified path; startup never blocks on the probe"
    why_human: "`verification: backstop` marker in 11-01-PLAN must_haves; requires directly observed runtime behavior. Maintainer-accepted 2026-09-16 (decision, not evidence)."
  - test: "Prohibition (flagged-unverified): 'baude never guesses a missing modifier' — LLM-judge verdict: UPHELD, non-authoritative (no timing heuristics anywhere in the Enter arm, keys.rs:60-81; legacy terminals deliver Shift+Enter as plain Enter and it submits; README.md:157-159 states this truthfully)"
    expected: "Human confirms no modifier inference exists on any input path"
    why_human: "Planner marked `verification: flagged-unverified` — must not be silently absorbed into a pass. ACCEPTED by maintainer 2026-09-16."
  - test: "Prohibition (flagged-unverified): 'enhanced bytes are never sent on an unverified path' — LLM-judge verdict: UPHELD, non-authoritative (kitty_child gate test-proven at app.rs:5719; note review IN-01: the outer-probe leg is enforced indirectly via the gated push, not read on the input path)"
    expected: "Human accepts the indirect outer-leg enforcement documented in 11-REVIEW.md IN-01"
    why_human: "Planner marked `verification: flagged-unverified`. ACCEPTED by maintainer 2026-09-16."
  - test: "Prohibition (flagged-unverified): 'keyboard mode is never left pushed after any controlled exit' — LLM-judge verdict: UPHELD, non-authoritative (pop-iff-pushed byte-offset tests; panic hook, normal exit, and the WR-01 Terminal::new Err path at main.rs:453-460 all route through restore_terminal)"
    expected: "Human confirms exit paths are exhaustively covered; real-terminal residue check was routed to the Phase 12 dogfood per D-08"
    why_human: "Planner marked `verification: flagged-unverified`. ACCEPTED by maintainer 2026-09-16. NOTE: the Phase 12 smoke legs 10-12 that would have covered outer-terminal residue carry the same bulk-attestation weakness as legs 8-9."
  - test: "Prohibition (flagged-unverified): 'legacy key bytes never change' — LLM-judge verdict: UPHELD, non-authoritative (57-row corpus x both panes frozen, re-run green at this HEAD; review diffed 3ab66a8..HEAD — only the Shift+Enter arm changed)"
    expected: "Human accepts the corpus as the byte-freeze authority"
    why_human: "Planner marked `verification: flagged-unverified`. ACCEPTED by maintainer 2026-09-16."
  - test: "Prohibition (flagged-unverified): 'negotiation never blocks startup or input indefinitely' — LLM-judge verdict: UPHELD, non-authoritative (single pre-loop probe at main.rs:440; bound is crossterm's internal 2000 ms deadline — a library property, not locally testable)"
    expected: "Human accepts the crossterm-internal bound as sufficient (per RESEARCH Pitfall 1)"
    why_human: "Planner marked `verification: flagged-unverified`; the 2 s bound lives inside the dependency. ACCEPTED by maintainer 2026-09-16."
  - test: "Prohibition (flagged-unverified): 'baude never answers the child's CSI ? u probe on the vt parser path' — LLM-judge verdict: UPHELD, non-authoritative (no reply channel exists in the fork; query_probe regression test green; csi_dispatch '?' arm untouched at screen.rs:1755-1763)"
    expected: "Human confirms observation-without-advertisement stays the fork contract"
    why_human: "Planner marked `verification: flagged-unverified`. ACCEPTED by maintainer 2026-09-16."
advisory: []
---

# Phase 11: Negotiated Multiline Input Verification Report

**Phase Goal:** Users can insert newlines with Shift+Enter on verified terminal paths without changing existing key behavior or leaking terminal modes.
**Verified:** 2026-09-18T00:00:00Z
**Status:** passed — automated verification passed; the open human-verification items were ACCEPTED by the maintainer on 2026-09-18 rather than observed (see Acceptance Record).
**Re-verification:** YES — this is a RE-VERIFICATION of phase 11 at current HEAD (`ee6fa3c`), superseding the 2026-09-16 report.

## Why this re-verification ran

The prior report's `covered_digest` went stale for a mechanical reason: the digest is computed over `covered_files`, and three of those paths changed after it was written. Two were source files, so the staleness was not purely cosmetic:

| Changed file | Commit | Nature |
|---|---|---|
| `baude/src/ui.rs` | `b4d482a` (12-01) | `manual_clamp` clippy fix |
| `README.md` | `cc914e2`, `35dd0e6`, `da50929` (12-02) | SHIP-02 documentation restructure |
| `.planning/REQUIREMENTS.md` | `b95d701`, `d828749` | bookkeeping |

Two further corrections this report makes to the prior one:

1. **The prior report contradicted itself.** Its frontmatter said `status: passed` while its body said `**Status:** human_needed` — and its own Gaps Summary closed with "Status is `human_needed`". The body was right. This report states `human_needed` in both places.
2. **The prior report's single `deferred` item came back unresolved.** See below.

**Note on commit hashes.** Phase history was rebased during the release (`git rebase --onto`), so hashes cited in SUMMARY/REVIEW files are unreachable from HEAD. Every claim below is verified by CODE and TEST at this HEAD, with provenance matched by subject line where needed.

## Staleness assessment — did anything regress?

### `baude/src/ui.rs` — clippy fix: NO impact on phase 11

`b4d482a` changed exactly one line, at `ui.rs:2120` inside `draw_modal`'s `Modal::Links` arm:

```rust
-            let list_rows = links.len().min(10).max(1);
+            let list_rows = links.len().clamp(1, 10);
```

Behavior-preserving for every `usize` (`max(min(x,10),1) == clamp(1,10)`; the bounds are constants, so `clamp`'s panic path is unreachable) — proven in the phase-10 re-verification and cited here rather than re-derived.

**It does not touch phase 11's help-overlay work.** The edit sits in the `Modal::Links` arm; phase 11's rows are in the `Modal::Help` arm ~95 lines below, at `ui.rs:2215-2216`, both intact:

```rust
Line::raw("  shift+enter  newline in claude pane"),
Line::raw("               (kitty-capable terminals; see README)"),
```

The clip budget those rows depend on (`centered(area, 60, 39)` at `ui.rs:2177`, with its "keep in sync when adding rows" comment at 2174-2176) is unchanged. `help_overlay_lists_shift_enter` re-run green.

### `README.md` — the one place a regression could have hidden: NO regression

This was the real risk. 12-02 MOVED the tested-terminals list out of the kitty-keyboard-protocol paragraph into its own `### Tested terminals` section (`README.md:222`). TKEY-03 requires "documented multiline-input setup or fallback guidance" — if the move had orphaned that guidance, TKEY-03 would have regressed.

**It did not.** All four elements survived intact, and the move was handled with an explicit cross-reference:

| TKEY-03 element | Location at HEAD | Status |
|---|---|---|
| Keys-table row | `README.md:121` — `` | `shift+enter` | claude pane | insert a newline without submitting | `` | ✓ intact |
| Negotiated, not guessed | `README.md:151-153` — "negotiated at startup through the kitty keyboard protocol… only changes behavior when it answers yes" | ✓ intact |
| Exact legacy behavior + no guessing | `README.md:157-159` — "the terminal reports Shift+Enter as plain Enter, so it submits — exactly the pre-existing behavior. baude never guesses a missing modifier." | ✓ intact |
| Fallbacks | `README.md:161-163` — Claude Code backslash-then-Enter / `/terminal-setup`; opencode `ctrl+j` | ✓ intact |
| tmux guidance | `README.md:165-168` — `set -s extended-keys on` | ✓ intact |

**Discoverability preserved.** The section does not silently lose its terminal list — `README.md:154` now carries an in-document link, `See [Tested terminals](#tested-terminals) below for where that negotiation is exercised.` The anchor resolves to the `### Tested terminals` heading at `README.md:222`, and that section names shift+enter first among the negotiated gestures (`README.md:224`). The `## Reference` closer in `12-SMOKE-EVIDENCE.md:131` independently maps legs 8-9 to `### Shift+Enter newlines`, confirming the section is still the documented authority.

### Phase 11's own core files — undisturbed (confirmed, not assumed)

`git log -1` per file shows every one still ends on a phase-11 commit. No post-phase-11 commit touches any of the six:

| File | Last commit | Subject |
|---|---|---|
| `baude/src/keys.rs` | `fbcbb01` | test(11-01): freeze legacy key byte corpus and Shift+Enter matrix |
| `baude/src/main.rs` | `1d7ece6` | fix(11): WR-01 restore terminal on Terminal::new error after kitty push |
| `baude/src/app.rs` | `9c7b6a3` | feat(11-04): forward_key consults the observed child kitty push |
| `vendor/vt100/src/screen.rs` | `a3200e6` | fix(11): WR-03 replay full per-screen kitty stack depth |
| `baude-core/src/pty.rs` | `a3200e6` | fix(11): WR-03 replay full per-screen kitty stack depth |
| `vendor/vt100/tests/kitty_keyboard.rs` | `b64f04e` | fix(11): WR-02 per-screen kitty stacks |

## Goal Achievement

### ROADMAP Success Criteria

| # | Success Criterion | Status | Evidence at HEAD |
|---|-------------------|--------|------------------|
| 1 | Shift+Enter inserts a newline without submitting Claude/claudex prompts on documented, tested terminal paths | ✓ VERIFIED (code + tests) | `keys.rs:60-81` Shift+Enter arm read at HEAD: `\x1b[13;2u` only when `ctx.kitty_child`, `\x1b\r` fallback to the Claude pane, plain `\r` to the shell pane (ESC CR is meta-CR to readline); composed-matrix test through the real derivation green. **Live keystroke still unconfirmed — see Carried-Forward Unresolved Item.** |
| 2 | Ordinary Enter, Ctrl-C, navigation keys, existing baude shortcuts retain behavior | ✓ VERIFIED (code + tests) | 57-row `legacy_corpus()` x both panes frozen; `cargo test -p baude keys` → 8 passed, 0 failed at this HEAD. The non-shift Enter path is the untouched `else` branch at `keys.rs:76-81`. **Live confirmation carries the same caveat as SC1.** |
| 3 | Terminals that cannot distinguish Shift+Enter retain legacy behavior with documented setup/fallback guidance | ✓ VERIFIED | `negotiate_keyboard` (`main.rs:111-113`) is `matches!(probe(), Ok(true))` — Ok(false)/Err → legacy, non-fatal; `keyboard_negotiation` 4/4 green incl. the byte-identity restore test; help overlay row at `ui.rs:2215-2216`; README guidance re-verified intact post-move (table above) |
| 4 | Prior outer keyboard mode restored on controlled exit/failure/suspend paths, re-established on resume | ✓ VERIFIED | `restore_terminal()` (`main.rs:131-141`) swaps `KEYBOARD_ENHANCED` so a double restore pops exactly once; `write_restore_sequence` queues `PopKeyboardEnhancementFlags` BEFORE `LeaveAlternateScreen` (per-screen stacks demand this ordering); WR-01 init-error path confirmed at `main.rs:453-460`. Suspend leg VACUOUS by design — no SIGTSTP handling exists, documented in-code at `main.rs:135-139` |
| 5 | Negotiation cannot block startup/input indefinitely; enhanced sequences only on a verified outer/child path | ✓ VERIFIED (structural bound) | Single-shot probe at `main.rs:440`, pre-loop, pushed AFTER `EnterAlternateScreen` so push and pop hit the same per-screen stack, DISAMBIGUATE-only; bound is crossterm-internal 2000 ms. Child leg: `kitty_child: screen.kitty_keyboard() != 0` (`app.rs:5719`), fail-closed. Outer leg enforced indirectly via the gated push (review IN-01) — the backstop timing half routes to human verification |

**Roadmap contract:** 5/5 success criteria supported in code and re-confirmed at this HEAD. SC1 and SC2 lack live real-terminal confirmation (see below); SC5's backstop invariant and the six flagged prohibitions carry maintainer acceptance as a decision, not as evidence.

### Observable Truths (merged PLAN must_haves — full list carried forward, nothing dropped)

| # | Truth (plan) | Status | Evidence at HEAD |
|---|--------------|--------|------------------|
| 1 | Shift+Enter on kitty-capable outer writes ESC CR to child, not bare CR (11-01) | ✓ VERIFIED | Arm read at `keys.rs:60-81`; `cargo test -p baude keys` 8/8 green |
| 2 | Repeated Shift+Enter idempotent, no state accumulation (11-01) | ✓ VERIFIED | `encode_key` is a pure fn over `EncodeCtx` (a `Copy` struct); idempotency test green |
| 3 | Interleaved events during/after negotiation never emit enhanced on unverified path (11-01, **backstop**) | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | Encoding half test-proven; timing half structural only — no test exercises interleaving. Behavior-dependent truth (ordering invariant): presence + wiring cannot upgrade it. Routed to human verification |
| 4 | Probe Ok(false)/Err pushes nothing; child bytes byte-identical to pre-phase (11-01) | ✓ VERIFIED | `legacy_restore_emits_no_pop_and_matches_pre_phase_bytes` green in the 4/4 `keyboard_negotiation` run |
| 5 | restore_terminal pops iff pushed, before alt-screen leave, on the one shared function (11-01) | ✓ VERIFIED | `main.rs:118-141` read at HEAD; byte-offset asserts green; all three exit classes + WR-01 path call `restore_terminal()` |
| 6 | Probe runs exactly once, pre-loop, bounded (11-01) | ✓ VERIFIED (declared precondition) | Sole call site `main.rs:440`; bound is crossterm-internal, documented in-code |
| 7 | vt100 fork tracks child kitty state: `>u` push, `<u` saturating pop, `=u` set (11-02) | ✓ VERIFIED | `screen.rs:1774-1786` csi_dispatch arms read at HEAD; `cargo test -p vt100 --test kitty_keyboard` → 14 passed, 0 failed |
| 8 | `kitty_keyboard()` returns current flags; 0 default and after full pop (11-02) | ✓ VERIFIED | `screen.rs:728-730`: `self.active_kitty_stack().last().copied().unwrap_or(0)` |
| 9 | Hostile counts cannot panic/exhaust: pop saturates, depth capped at 32 (11-02) | ✓ VERIFIED | `KITTY_STACK_MAX: usize = 32` (`screen.rs:23`), oldest-eviction at `screen.rs:1391-1396`; hostile cases green |
| 10 | Child's CSI ? u query stays UNANSWERED — no reply path (11-02) | ✓ VERIFIED | `screen.rs:1755-1774`: the `?` intermediate arm falls through to the debug-log default; only `>u`/`<u`/`=u` are handled. No reply channel exists anywhere in the fork |
| 11 | Help overlay shows shift+enter binding + README pointer (11-03) | ✓ VERIFIED | `ui.rs:2215-2216` intact post-clippy-fix; `help_overlay_lists_shift_enter` 1/1 green |
| 12 | README documents supported terminals, legacy behavior, fallbacks (11-03) | ✓ VERIFIED | Re-verified element-by-element after the 12-02 move (table above); anchor link at `README.md:154` keeps the terminal list discoverable |
| 13 | CSI 13;2u ONLY after observed child push; else ESC CR / CR (11-04) | ✓ VERIFIED | `app.rs:5716-5721`; `cargo test -p baude forward_ctx` → 4 passed, 0 failed |
| 14 | Local and remote-attach forward_key branches share one derivation helper (11-04) | ✓ VERIFIED | Both branches read at HEAD (`app.rs:3941`, `app.rs:3965`) call `encode_ctx` under their parser locks, each with an identical fail-closed `unwrap_or` legacy `EncodeCtx { kitty_child: false, .. }` |
| 15 | Remote mirror converges on child kitty state via subscribe snapshot replay (11-04) | ✓ VERIFIED | Full-depth per-screen replay read at `pty.rs:411-413` (main stack, pre-`?1049h`) and `pty.rs:438-442` (alternate stack, post-switch); `cargo test -p baude-core subscribe` → 6 passed, 0 failed |

**Score:** 14/15 truths verified (1 present, behavior-unverified). Identical to the prior score — no regression, and the maintainer acceptance did not raise it.

### Carried-Forward Unresolved Item (was `deferred`, now OPEN)

The prior report carried exactly one `deferred` entry, pointing at Phase 12. **Phase 12 has since run and returned a signed gap on that very item**, so the deferral is no longer valid and is promoted here to an open human-verification item.

| Item | Where it was routed | What came back |
|---|---|---|
| Live end-to-end Shift+Enter on a real terminal (TKEY-01 / TKEY-02) | Phase 12 SHIP-03 smoke session | **UNRESOLVED.** `12-SMOKE-EVIDENCE.md` is a labeled **BULK ATTESTATION**, not a leg-by-leg walkthrough. Legs 8 (Shift+Enter) and 9 (ordinary Enter) both read "Not individually reported. Covered by the observer's set-level attestation 'all works flawlessly'." Both carry the caveat that the smoke instance ran under `BAUDE_WORKSPACE=smoke`, which starts with no claude pane, and "Whether a session was added first was not confirmed." Signed gap 3 names phase 11 directly: "phase 11's own verification routed the real-terminal leg here — so if these two were not run, that routing is still open." Signed off by Joe Seymour, 2026-09-16. The Linux session was never run at all (legs 8-9 DEFERRED there too). |

**Stated plainly: TKEY-01 and TKEY-02 rest on automated coverage plus a set-level attestation, not on a confirmed live keystroke.** The code is present, wired, and test-proven at the byte level; what is missing is one human pressing the key in a real terminal with a real claude session attached.

### Required Artifacts

| Artifact | Expected | Status | Details at HEAD |
|----------|----------|--------|---------|
| `baude/src/keys.rs` | EncodeCtx, rewritten Enter arm, corpus + matrix tests (`contains: EncodeCtx`) | ✓ VERIFIED | `EncodeCtx` at `keys.rs:7` with the `kitty_child` "observed push — fail-closed false until verified" doc comment; conditional arm at 60-81 |
| `baude/src/main.rs` | negotiate_keyboard, KEYBOARD_ENHANCED, gated push, write_restore_sequence, pop in restore (`contains: KEYBOARD_ENHANCED`) | ✓ VERIFIED | Static at 104, `negotiate_keyboard` 111, `write_restore_sequence` 118, `restore_terminal` 131, gated push 440-448 |
| `baude/src/app.rs` | encode_ctx helper; forward_key reads kitty_keyboard() in both branches (`contains: kitty_keyboard`) | ✓ VERIFIED | `encode_ctx` at 5716; both call sites confirmed with fail-closed fallbacks |
| `vendor/vt100/src/screen.rs` | kitty stack + csi_dispatch arms + accessor (`contains: kitty_keyboard`) | ✓ VERIFIED | Per-screen `kitty_stack`/`kitty_alternate_stack` at 112-113, `kitty_keyboard()` 728, `kitty_main_stack()` 737, `kitty_alternate_stack()` 745, `active_kitty_stack{,_mut}` 749/757, alternate clear on activation 841 |
| `vendor/vt100/tests/kitty_keyboard.rs` | byte-in/state-out integration tests | ✓ VERIFIED | 14 tests, all green at this HEAD |
| `baude-core/src/pty.rs` | subscribe() replays active child kitty push (`contains: kitty_keyboard`) | ✓ VERIFIED | Full-depth per-screen replay at 411-442; inactive child adds zero bytes (snapshot stays byte-identical to pre-phase) |
| `baude/src/ui.rs` | shift+enter help row + render test (`contains: shift+enter`) | ✓ VERIFIED | Rows at 2215-2216 intact after the 12-01 clippy edit; test green |
| `README.md` | Keys row + supported-terminals/fallback caveat (`contains: shift+enter`) | ✓ VERIFIED | Row at 121; `### Shift+Enter newlines` at 149-168; anchor link to the relocated `### Tested terminals` at 154 |

### Key Link Verification

| From | To | Via | Status | Details at HEAD |
|------|----|-----|--------|---------|
| `baude/src/app.rs` | `baude/src/keys.rs` | forward_key builds EncodeCtx, calls `encode_key(&key, ctx)` | ✓ WIRED | Remote-attach branch `app.rs:3941` → `a.write_input(&encode_key(&key, ctx))`; local branch `app.rs:3965` → `pty.write_input(&encode_key(&key, ctx))` |
| `baude/src/main.rs` | `baude/src/main.rs` | restore_terminal drives write_restore_sequence with swapped static | ✓ WIRED | `main.rs:133-141`; panic hook, normal exit, and WR-01 error path all call `restore_terminal()` |
| `baude/src/app.rs` | `vendor/vt100/src/screen.rs` | encode_ctx reads `screen.kitty_keyboard() != 0` | ✓ WIRED | `app.rs:5719` |
| `baude-core/src/pty.rs` | `vendor/vt100/src/screen.rs` | subscribe() conditions replay on kitty stack accessors | ✓ WIRED | `pty.rs:411` (`kitty_main_stack()`), `pty.rs:439` (`kitty_alternate_stack()`), both full-depth per WR-03 |
| `vendor/vt100/tests/kitty_keyboard.rs` | `vendor/vt100/src/screen.rs` | Parser::process + `kitty_keyboard()` asserts | ✓ WIRED | 14 passing byte-in/state-out cases |
| `baude/src/ui.rs` | `README.md` | help line points at README | ✓ WIRED | `ui.rs:2216`: "(kitty-capable terminals; see README)" — and the README section it points at survived the 12-02 move |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|---|---|---|---|---|
| `baude/src/keys.rs` | `ctx.kitty_child` | `screen.kitty_keyboard() != 0` via `encode_ctx` — a real parser read, not a literal | ✓ | ✓ FLOWING |
| `vendor/vt100/src/screen.rs` | `active_kitty_stack()` | Mutated only by `kitty_push`/`kitty_pop`/`kitty_set` from real `csi_dispatch` bytes | ✓ | ✓ FLOWING |
| `baude-core/src/pty.rs` | snapshot `bytes` | Iterates the live parser's per-screen stacks | ✓ | ✓ FLOWING |
| `baude/src/main.rs` | `popped` | `KEYBOARD_ENHANCED.swap(false, ..)` — real push outcome, set only when `execute!` returned Ok | ✓ | ✓ FLOWING |

No hardcoded literal, static return, or mock terminates any chain. The `unwrap_or(EncodeCtx { kitty_child: false, .. })` fallbacks are deliberate fail-closed poisoned-lock handling (D-04), not stubs.

### Code Review Cross-Check — the three warnings are STILL present in code

Re-confirmed by reading the code at this HEAD, not by trusting SUMMARY claims:

- **WR-01** — `Terminal::new` Err routes through `restore_terminal()` before propagating. Confirmed `main.rs:453-460`, with the explanatory comment naming the leak class it closes ("a bare `?` here would leak raw mode, the alternate screen, AND the pushed keyboard flags").
- **WR-02** — per-screen kitty stacks. Confirmed: two separate `Vec<u16>` fields (`screen.rs:112-113`), `active_kitty_stack{,_mut}()` selecting on `alternate_screen()` (749-761), and the alternate stack cleared on activation (`screen.rs:841`). Alt-screen exit therefore cannot leak flags into the main screen. 14/14 fork tests green.
- **WR-03** — full-depth per-screen snapshot replay. Confirmed `pty.rs:411-442`: one `ESC [ > flags u` per stack entry oldest-first, main stack emitted BEFORE `?1049h`, alternate stack after. The in-code comment states the failure this fixes: "a single top-of-stack push would collapse an N-deep stack to depth 1 and the next CSI < 1 u would empty the mirror while the source stays kitty." 6/6 subscribe tests green.

### Behavioral Spot-Checks

Targeted runs only. The full workspace suite was NOT re-run — it was already run at this exact HEAD (`cargo test --workspace --locked -- --test-threads=1` → exit 0, 646 results, 0 failed; `cargo fmt --check` exit 0; `cargo clippy --workspace --all-targets -- -D warnings` exit 0).

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Shift+Enter matrix + legacy byte corpus (TKEY-01/02) | `cargo test -p baude keys` | 8 passed, 0 failed | ✓ PASS |
| Probe edges + restore ordering + double-restore (TKEY-03/04) | `cargo test -p baude keyboard_negotiation` | 4 passed, 0 failed | ✓ PASS |
| ctx derivation + composed matrix (TKEY-05 child gate) | `cargo test -p baude forward_ctx` | 4 passed, 0 failed | ✓ PASS |
| Help overlay render, post-clippy-fix | `cargo test -p baude help_overlay` | 1 passed, 0 failed | ✓ PASS |
| Fork kitty tracking + hostile input + unanswered probe | `cargo test -p vt100 --test kitty_keyboard` | 14 passed, 0 failed | ✓ PASS |
| Subscribe replay round-trips (WR-03) | `cargo test -p baude-core subscribe` | 6 passed, 0 failed | ✓ PASS |
| Backstop: no enhanced bytes under event interleaving | — | no test exists | ? SKIP → human |
| Live Shift+Enter in a real terminal | — | requires a real terminal + claude pane | ? SKIP → human |

### Probe Execution

No `scripts/*/tests/probe-*.sh` probes exist in this repository and no PLAN declares any — N/A.

### Requirements Coverage

| Requirement | Source Plans | Description | Status | Evidence |
|-------------|--------------|-------------|--------|----------|
| TKEY-01 | 11-01, 11-02, 11-04 | Shift+Enter inserts newline without submitting on tested paths | ✓ SATISFIED (automated) | Encode arm + composed matrix + idempotency green at HEAD; enhanced only behind the observed-push gate. **Live leg unconfirmed** — smoke gap 3 |
| TKEY-02 | 11-01, 11-04 | Enter/Ctrl-C/nav/shortcuts retain behavior | ✓ SATISFIED (automated) | Frozen 57-row corpus x both panes, green at HEAD. **Live leg unconfirmed** — smoke gap 3 |
| TKEY-03 | 11-01, 11-03 | Legacy terminals keep behavior; documented guidance; no modifier guessing | ✓ SATISFIED | Probe false/Err → legacy tests green; help overlay row intact after the clippy fix; README guidance re-verified element-by-element after the 12-02 restructure |
| TKEY-04 | 11-01 | Prior keyboard mode restored on exit/failure/suspend; re-established on resume | ✓ SATISFIED | Pop-iff-pushed + ordering asserts on the shared restore path + WR-01; suspend leg vacuous by design (no SIGTSTP handling exists — documented in-code) |
| TKEY-05 | 11-01, 11-02, 11-04 | Negotiation cannot block; enhanced only on verified path | ✓ SATISFIED | Bounded single-shot pre-loop probe; dual-gate (outer push + observed child push); subscribe replay keeps remote parity. Backstop timing half → human |

REQUIREMENTS.md maps exactly TKEY-01..TKEY-05 to Phase 11, all still marked Complete; all five appear in plan frontmatter. No orphaned requirements.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `vendor/vt100/src/screen.rs` | 1490 | `XXX really i want to just be able to pass in a default Params` | ℹ️ Info | **Not phase-11 debt.** Provenance established by `git log -L 1490,1490:vendor/vt100/src/screen.rs` → `a60fb6c test(10-01): add failing tracer tests for the OSC8 link slice (RED)`, with `new file mode` — the marker arrived wholesale with the vendored upstream file in phase 10. Phase 11 added zero debt markers. Unchanged from the prior report's classification. |

No `TBD`/`FIXME`/`TODO`/`HACK`/`PLACEHOLDER` in any of the six phase-modified source files. Review items IN-02 (Relaxed ordering on `KEYBOARD_ENHANCED`), IN-03 (duplicated `negotiate_keyboard` test), IN-04 (duplicated fail-closed fallback literal, visible at `app.rs:3942-3946` and `app.rs:3966-3970`) remain open as accepted Info-level findings — none blocks the goal.

**Re-verification evidence gate:** no new 🛑 Blocker was raised, so the gate's evidence requirement is not exercised and the `advisory:` list is empty.

### Prior Human Acceptance (recorded as a decision, not as evidence)

On 2026-09-16 the maintainer was presented with the backstop timing claim and the six flagged prohibitions at an autonomous validation gate and responded "All good — continue."

This report records that as **acceptance**. It is a decision. It is not evidence, and it changes nothing quantitative:

- It does **not** raise the verified score (still 14/15).
- It does **not** convert truth 3 from ⚠️ PRESENT_BEHAVIOR_UNVERIFIED to ✓ VERIFIED — no test was added, so the ordering invariant is still unexercised.
- It was never formalized as an `overrides:` entry, so no `PASSED (override)` credit applies (`overrides_applied: 0`).
- Most consequentially: the acceptance routed the real-terminal leg to Phase 12, and **that routing came back open**. Accepting a deferral does not discharge it.

### Human Verification Required

#### 1. Real-terminal Shift+Enter and ordinary Enter (CARRIED FORWARD — UNRESOLVED)

**Test:** Launch baude in a kitty-capable terminal with a real claude session present (NOT an empty `BAUDE_WORKSPACE=smoke` instance). In the claude pane press `shift+enter`, then press `enter`.
**Expected:** `shift+enter` inserts a newline and submits nothing; `enter` submits, behavior unchanged.
**Why human:** Phase 11 routed this leg to the Phase 12 smoke session. That session returned signed gap 3: legs 8-9 may never have been exercised, because the instance started with no claude pane and nobody confirmed one was added. The Linux session was never run. This is the single item standing between TKEY-01/TKEY-02's automated proof and an observed user-visible behavior.

#### 2. Backstop truth: no enhanced bytes on an unverified path under interleaving

**Test:** On a kitty-capable terminal, mash keys during baude startup (the ~0-2 s probe window) and immediately after; then run baude in a plain terminal and press Shift+Enter plus assorted keys in both panes.
**Expected:** No CSI-u sequence ever reaches a child that did not itself push kitty flags; startup never hangs on the probe.
**Why human:** `verification: backstop` in 11-01-PLAN. The encoding half is test-proven; the timing half is a structural argument about the single-threaded pre-loop window that no test exercises. Accepted 2026-09-16 as a decision; still unexercised.

#### 3-8. Six planner-flagged prohibitions (`verification: flagged-unverified`)

All six carry non-authoritative UPHELD LLM-judge verdicts, each re-confirmed against code at this HEAD, and all six were ACCEPTED by the maintainer on 2026-09-16. They are listed rather than absorbed, because acceptance is a decision and the planner's flag survives it:

1. baude never guesses a missing modifier — UPHELD (no heuristics in the Enter arm; README states the legacy behavior truthfully)
2. enhanced bytes never sent on an unverified path — UPHELD (note IN-01's indirect outer-leg enforcement)
3. keyboard mode never left pushed after any controlled exit — UPHELD (incl. the WR-01 path). NOTE: the Phase 12 smoke legs 10-12 that would have given real-terminal residue confirmation carry the same bulk-attestation weakness as legs 8-9.
4. legacy key bytes never change — UPHELD (frozen corpus, re-run green at this HEAD)
5. negotiation never blocks startup/input indefinitely — UPHELD (bound is crossterm-internal, not locally testable)
6. baude never answers the child's CSI ? u probe — UPHELD (no reply channel; `?` arm falls through; regression-tested)

### Gaps Summary

**No code gaps, and no regressions from the staleness.** All 15 plan truths remain implemented and wired at this HEAD; 14 are behaviorally verified by passing named test suites re-run here; 1 (the backstop interleaving invariant) is present-but-behavior-unverified by design of its `backstop` marker. All 5 ROADMAP success criteria are supported in code. All three code-review warnings (WR-01, WR-02, WR-03) were re-read in the code and are still present.

The two source changes that invalidated the digest are both benign for phase 11: the `ui.rs` clippy edit is behavior-preserving and lands in a different match arm ~95 lines from the help-overlay rows, and the `README.md` restructure preserved every element of TKEY-03's guidance while keeping the relocated terminal list discoverable through an explicit `#tested-terminals` anchor.

**What is genuinely open is not new code debt but an unclosed verification loop.** Phase 11's single deferral — live real-terminal confirmation of Shift+Enter — was routed to Phase 12, and Phase 12 signed a gap saying it may never have been exercised. That item is therefore promoted here from `deferred` to OPEN. Combined with the never-exercised backstop invariant and the six planner-flagged prohibitions (maintainer-accepted as a decision), the status is `human_needed` — the same status the prior report's body reached, now stated consistently in the frontmatter as well.

---

_Verified: 2026-09-18T00:00:00Z (re-verification at HEAD ee6fa3c)_
_Verifier: Claude (gsd-verifier)_

---

## Acceptance Record — 2026-09-18

The open `human_verification` items above were presented to the maintainer and
**accepted**, not observed. They are deliberately left listed rather than
deleted: acceptance is a decision about risk, and a later reader is entitled to
see exactly what was accepted and on what basis.

- **Accepted by:** Joe Seymour, 2026-09-18
- **What was accepted:** every open item in this report's `human_verification`
  block, and the backstop-tier truth that remains `behavior_unverified`.
- **What this does NOT mean:** no item was independently observed as a result of
  this acceptance. The verified score is unchanged by it, `overrides_applied`
  stays as recorded, and the backstop truth is still not counted as verified.
The phase-11 gap remains a gap, accepted rather than closed: TKEY-01 and
  TKEY-02 rest on automated coverage plus a set-level attestation, NOT on a
  confirmed live keystroke. The smoke instance ran under `BAUDE_WORKSPACE=smoke`,
  which starts with no claude pane, and nobody confirmed one was added before
  legs 8-9.

- **What IS independently established** (re-verification at HEAD `ee6fa3c`, this
  pass): no regression in any must_have; the full workspace suite green at 646
  results / 0 failed with `cargo fmt --check` and
  `cargo clippy --workspace --all-targets -- -D warnings` both exit 0; and every
  behavioral claim in the body bound to a targeted test run in the verifier's own
  process. Requirements TKEY-01..TKEY-05 are satisfied on that automated evidence.

This record exists so "passed" is never mistaken for "observed".
