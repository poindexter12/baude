---
phase: 11-negotiated-multiline-input
reviewed: 2026-09-16T15:21:32Z
depth: standard
files_reviewed: 6
files_reviewed_list:
  - baude-core/src/pty.rs
  - baude/src/app.rs
  - baude/src/keys.rs
  - baude/src/main.rs
  - baude/src/ui.rs
  - vendor/vt100/src/screen.rs
findings:
  critical: 0
  warning: 3
  info: 4
  total: 7
status: issues_found
---

# Phase 11: Code Review Report

**Reviewed:** 2026-09-16T15:21:32Z
**Depth:** standard
**Files Reviewed:** 6 (plus `vendor/vt100/tests/kitty_keyboard.rs` as supporting evidence)
**Status:** issues_found

## Summary

Reviewed the phase-11 negotiated-multiline-input diff (`3ab66a8..HEAD`) adversarially against the phase's safety invariants. The core invariants hold:

- **Fail-closed child gate (D-04):** `encode_ctx` (app.rs:5716) reads `kitty_keyboard() != 0`; fresh parser, fully-popped stack, and poisoned parser lock (both `forward_key` branches, app.rs:3938-3946/3961-3970) all degrade to a legacy `EncodeCtx`. Probe `Err`/`Ok(false)` both yield legacy via `negotiate_keyboard` (main.rs). Verified by reading each path, not just the tests.
- **Legacy byte-identity (TKEY-02):** diffed pre-phase `keys.rs` at `3ab66a8` against HEAD — the only behavioral change is the Shift+Enter arm. Notably the pre-phase code emitted `\x1b[13;2u` *unconditionally* for Shift+Enter (fail-open); the phase closes that hole. All other arms (Enter/Ctrl chords/nav/tilde/F-keys/chars) are byte-identical, and the expanded frozen corpus locks them across both panes.
- **Pop-before-LeaveAlternateScreen:** `write_restore_sequence` queues the pop first, single flush; the panic hook routes through `restore_terminal`; the `AtomicBool` swap makes a double restore pop exactly once. (One uncovered early-return path — WR-01.)
- **No probe answer:** the vt100 fork has no `CSI ? u` arm and no reply channel; `screen.rs` only dispatches `>`/`<`/`=` intermediates with final `u`. `CSI > 0 c` correctly falls through to the debug-log path.
- **Stack bounds:** push evicts oldest at `KITTY_STACK_MAX` (32); pop saturates via `min`; vte caps params at `u16`. `ris()` rebuilds the whole `Screen` via `*self = Self::new(..)`, so RIS clears the kitty stack (fail-closed after a full terminal reset).
- **Subscribe replay:** inactive children (`kitty == 0`) add zero bytes — byte-identical snapshot to pre-phase; poisoned lock returns an empty snapshot.

Targeted test suites pass (`vt100` kitty tests 11/11; `baude-core` subscribe tests 4/4; `baude` keys/forward_ctx/negotiation/help tests 16/16).

Remaining findings are two genuine residual-state gaps in the fail-closed story and one leak on an error path, plus minor quality items.

## Warnings

### WR-01: `Terminal::new(...)?` after the keyboard push bypasses `restore_terminal` on Err

**File:** `baude/src/main.rs:450` (the `ratatui::Terminal::new(...)?` immediately after the `PushKeyboardEnhancementFlags` block)
**Issue:** The kitty push executes, `KEYBOARD_ENHANCED` is set, and then `Terminal::new(...)?` can return `Err` and propagate out of `main()` without ever calling `restore_terminal()`. This is a non-panic path, so the panic hook does not fire: the outer terminal is left in raw mode, on the alternate screen, with the keyboard-enhancement flags still pushed. Raw-mode/alt-screen leakage on this path is pre-existing, but the phase added a new piece of leaked terminal state (the pushed flags), and the phase's own invariant is "pop on every controlled exit." T-11-03 (enhanced-flag residue) explicitly targets this failure class.
**Fix:** Route the error through the restore path instead of `?`:
```rust
let mut terminal = match ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(stdout())) {
    Ok(t) => t,
    Err(e) => {
        restore_terminal();
        return Err(e.into());
    }
};
```
(or a small drop-guard struct that calls `restore_terminal()` and is `mem::forget`-disarmed only where `run`'s result path already restores).

### WR-02: Single kitty stack survives alternate-screen exit — stale fail-open child verification

**File:** `vendor/vt100/src/screen.rs:708-720` (accessor + single `kitty_stack`); consumed at `baude/src/app.rs:5716`
**Issue:** Real kitty keeps *independent* keyboard-mode stacks for the main and alternate screens precisely so an alt-screen app that dies without popping cannot poison main-screen input. The fork deliberately uses one stack (documented in the accessor docstring), but the docstring does not name the resulting fail-open consequence: a child app (e.g. neovim in the shell pane) enters the alternate screen, pushes `CSI > 1 u`, then is killed or crashes without popping. The screen returns to the main-screen shell, `kitty_keyboard()` still reads nonzero, `kitty_child` stays true, and Shift+Enter now sends `\x1b[13;2u` into a legacy readline — literal `[13;2u` junk typed at the prompt until the user runs `reset` (RIS is the only recovery path that clears the stack). This is a reachable stale-verification path that violates the phase's fail-closed posture; every other residue path (fresh parser, pop, RIS, poison) was closed.
**Fix:** Clear (or shelve) the kitty stack when the alternate screen is exited, matching kitty's per-screen isolation in the cheapest possible form:
```rust
// in rm() / the DEC private mode 1047/1049 reset handling:
self.kitty_stack.clear();
```
or track two stacks keyed on `MODE_ALTERNATE_SCREEN` and have `kitty_keyboard()` read the active one. If the single-stack divergence is kept as-is, the accessor docstring and 11-VALIDATION should at least record this residual fail-open scenario explicitly.

### WR-03: Subscribe replay collapses stack depth — post-attach pop diverges the mirror from the source

**File:** `baude-core/src/pty.rs:421-426` (kitty replay in `subscribe()`)
**Issue:** The snapshot replays exactly one push carrying the top-of-stack flags, so a source parser with an N-deep stack produces a mirror with a 1-deep stack. The very next live `CSI < 1 u` from a child that had pushed twice (push, nested push — e.g. Claude Code plus an inner tool) leaves the source at the prior nonzero entry while the mirror's stack empties to 0. From then on the remote-attach `forward_key` branch reads `kitty_child == false` and sends the legacy fallback (`\x1b\r`) to a child that is still in kitty mode. The divergence is in the safe direction (enhanced bytes are never sent to a legacy child), but the mirror permanently loses the child-verification signal for the rest of the attach even though the child still has kitty active — remote Shift+Enter silently degrades relative to local.
**Fix:** Replay the full stack, one push per entry, so pops decrement identically on both sides. Expose an iterator on the fork (`pub fn kitty_stack(&self) -> &[u16]`) and:
```rust
for flags in screen.kitty_stack() {
    bytes.extend_from_slice(format!("\x1b[>{flags}u").as_bytes());
}
```
(keeps the inactive-child case byte-identical: an empty stack emits nothing). Alternatively document the depth-collapse and its fail-closed direction at the replay site and in the round-trip test.

## Info

### IN-01: Outer-probe leg of D-04 is not consulted in `forward_key` — enforced only by event availability

**File:** `baude/src/app.rs:5716` (`encode_ctx`), `baude/src/main.rs:104` (`KEYBOARD_ENHANCED`)
**Issue:** D-04/11-04-PLAN state "enhanced sequences only when BOTH ends verified (outer probe AND child push)," but no code on the input path reads the outer-negotiation result — `KEYBOARD_ENHANCED` exists solely for the restore pop. The outer leg is enforced indirectly: without the gated push, terminals don't report Shift+Enter as `Enter+SHIFT`, so the enhanced arm is unreachable. That composition breaks on a terminal configured to force enhanced/modifyOtherKeys reporting regardless of negotiation (crossterm also parses `CSI 27;2;13~` into `Enter+SHIFT`): enhanced bytes could then flow with the probe false. The outcome is still harmless — `kitty_child` is only true when the child itself requested CSI-u — so the safety-relevant gate holds; only the literal "AND" is indirect.
**Fix:** A one-line comment on `encode_ctx` (or in `forward_key`) recording that the outer leg is enforced upstream by the gated push, so a future reader doesn't "fix" the missing check in the wrong direction — or plumb an explicit `outer_enhanced: bool` into `App` if the literal dual gate is wanted.

### IN-02: `KEYBOARD_ENHANCED` uses `Ordering::Relaxed` across the panic-hook path

**File:** `baude/src/main.rs:133,446`
**Issue:** The store happens on the main thread; the panic hook runs on whichever thread panics. With `Relaxed` there is no formal visibility guarantee that a worker-thread panic observes the earlier `store(true)`, in which case the pop would be skipped and the flags leak (the same residue T-11-03 mitigates). In practice cache coherence makes this vanishingly unlikely given the time gap, and the swap still prevents double pops.
**Fix:** Use `Ordering::SeqCst` for the store and swap — zero practical cost, removes the caveat.

### IN-03: `negotiate_keyboard` gate is tested twice, once from the wrong module

**File:** `baude/src/keys.rs:198-207` and `baude/src/main.rs` (`probe_failure_and_refusal_are_legacy_never_fatal`)
**Issue:** `keys.rs`'s test module reaches into `crate::negotiate_keyboard` (a `main.rs` item) and duplicates the exact assertions of `keyboard_negotiation_tests::probe_failure_and_refusal_are_legacy_never_fatal`. A keys-module test asserting a main-module item blurs ownership and doubles the maintenance surface for the same behavior.
**Fix:** Delete `negotiate_keyboard_gates_on_probe_result` from `keys.rs`; keep the copy that lives beside the function.

### IN-04: Fail-closed `EncodeCtx` fallback literal duplicated in both `forward_key` branches

**File:** `baude/src/app.rs:3942-3946` and `3966-3970`
**Issue:** The plan's "single ctx producer" holds for the success path (`encode_ctx`), but the poisoned-lock fallback is a hand-written struct literal repeated in both branches. A future field added to `EncodeCtx` gets three places to update, and the fallback (the fail-closed default the phase leans on) has no single authoritative definition.
**Fix:** Add a constructor and use it in both branches:
```rust
impl EncodeCtx {
    pub fn legacy(to_shell: bool) -> Self {
        Self { app_cursor: false, kitty_child: false, to_shell }
    }
}
// ... .unwrap_or(EncodeCtx::legacy(to_shell))
```

---

_Reviewed: 2026-09-16T15:21:32Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
