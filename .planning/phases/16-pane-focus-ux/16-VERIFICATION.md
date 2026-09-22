---
phase: 16-pane-focus-ux
verified: 2026-09-22T13:30:00Z
status: passed
score: 2/2 must-haves verified
covered_files:
  - .planning/phases/16-pane-focus-ux/16-01-PLAN.md
  - .planning/phases/16-pane-focus-ux/16-01-SUMMARY.md
  - baude-core/src/session.rs
  - baude/src/app.rs
  - baude/src/ui.rs
  - README.md
covered_digest: v1:sha256:fe3ee1eff41df6c12ec6f29f41053e832e830194d7724221ef4bf967da9a3481
re_verification: false
overrides_applied: 0
behavior_unverified: 0
---

# Phase 16: Pane Focus UX Verification Report

**Phase Goal:** Users can keep consistent pane focus (Claude or shell) across session switches.

**Verified:** 2026-09-22
**Status:** PASSED
**Re-verification:** No

## Goal Achievement Summary

Both ROADMAP success criteria verified. Phase implements per-session pane focus memory and directional focus movement (alt+↑/↓). The orchestrator corrected three defects from the wave agent's initial work: toggle vs directional keys, session-local memory, and test coverage via handle_key. Current implementation is correct.

## Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Pane focus is remembered when user switches to a different session and back | ✓ VERIFIED | `baude-core/src/session.rs:99` — `pub pane_focus_shell: bool` field (in-memory only per comment lines 96-99); `baude/src/app.rs:4196-4210` — `remember_pane_focus()` saves `pane_focus_shell` on focus change; `baude/src/app.rs:4211-4226` — `restore_pane_focus()` restores from `pane_focus_shell` on every activation path; `baude/src/app.rs:11802-11834` — test `pane_focus_survives_a_visit_to_a_session_without_a_shell` drives the #89 round-trip through `handle_key` and proves persistence across session switches |
| 2 | A keyboard shortcut moves focus between Claude pane and shell pane while both visible | ✓ VERIFIED | `baude/src/app.rs:4160-4168` — `handle_key()` dispatches `alt+KeyCode::Up` to `move_pane_focus(false)` (agent pane) and `alt+KeyCode::Down` to `move_pane_focus(true)` (shell pane); `baude/src/app.rs:4228-4250` — `move_pane_focus(to_shell: bool)` is directional (not toggle), checks shell exists, no-ops on sidebar/remote; `baude/src/app.rs:11764-11784` — test `alt_arrows_move_pane_focus_by_direction_not_toggle` verifies down always lands on shell, up always on agent, not a toggle; `baude/src/ui.rs:2353` — help overlay documents `alt+↑/↓`; `README.md:121` — Keys table documents `alt+↑/↓` |

## Code-Level Verification

### Criterion 1: Pane Focus Remembered Across Session Switches

**Expected behavior:** When user switches sessions with alt+← or alt+→ or j/k selection, the target session restores which pane had focus when last active.

**Evidence:**

- **Field storage (in-memory):** `baude-core/src/session.rs:99` — `pub pane_focus_shell: bool` marked as UX-01 with comment: "In-memory only: it is deliberately absent from the persisted state" (per 16-CONTEXT.md D5)
- **Recording on focus change:** `baude/src/app.rs:4196-4210` — `remember_pane_focus()` method:
  ```rust
  fn remember_pane_focus(&mut self) {
      let shell = match self.focus {
          Focus::Shell => true,
          Focus::Claude => false,
          Focus::Sidebar => return,  // Sidebar focus recorded as no-op
      };
      if let Some(session) = self.selected_mut() {
          session.pane_focus_shell = shell;
      }
  }
  ```
- **Restoration on activation:** `baude/src/app.rs:4211-4226` — `restore_pane_focus()` method:
  ```rust
  fn restore_pane_focus(&mut self) {
      let wants_shell = self
          .selected()
          .is_some_and(|s| s.pane_focus_shell && s.shell_open && s.shell.is_some());
      self.focus = if wants_shell {
          Focus::Shell
      } else {
          Focus::Claude
      };
  }
  ```
  Falls back to Claude if shell is not open or doesn't exist.
- **Wiring in all activation paths:**
  - `baude/src/app.rs:5424` — `move_selection()` calls `remember_pane_focus()` before moving
  - `baude/src/app.rs:5473-5475` — `cycle_session()` calls `remember_pane_focus()` on entry and `restore_pane_focus()` after selection change (existing pattern generalized)
  - `baude/src/app.rs:2483, 2564, 2652, 2686, 3292, 3347, 4488` — `restore_pane_focus()` called in activate, reopen, restore paths
- **Test coverage:** `baude/src/app.rs:11802-11834` — Test `pane_focus_survives_a_visit_to_a_session_without_a_shell()`:
  ```rust
  app.handle_key(alt(KeyCode::Down));
  assert_eq!(app.focus, Focus::Shell, "precondition: shell focused");
  // Switch away to repository row (no shell)
  app.selected_id = Some(SelId::Repository(...));
  app.focus = Focus::Claude;
  // Return to checkout
  app.selected_id = Some(SelId::Checkout(...));
  app.restore_pane_focus();
  assert_eq!(app.focus, Focus::Shell, "returning lands on shell pane");
  ```
  This directly tests GitHub #89: leaving a shell-focused session, switching away, and returning should restore shell focus.
- **Status:** ✓ VERIFIED

### Criterion 2: Keyboard Shortcut Moves Focus (Directional, Not Toggle)

**Expected behavior:** `alt+↑` focuses Claude pane (above), `alt+↓` focuses shell pane (below). No-op when shell is not open, when in sidebar, or on remote rows (which have no shell pane).

**Note:** ROADMAP success criterion says "toggles" but the design (16-CONTEXT.md D3) and implementation are directional because the shell sits BELOW the agent pane in `pane_rects`. The orchestrator corrected the wave agent's toggle implementation to directional in commit 40eab2d.

**Evidence:**

- **Key dispatch in handle_key():** `baude/src/app.rs:4160-4168`:
  ```rust
  if alt && matches!(key.code, KeyCode::Up) {
      // The shell is stacked below the agent pane: up means agent.
      self.move_pane_focus(false);
      return;
  }
  if alt && matches!(key.code, KeyCode::Down) {
      self.move_pane_focus(true);
      return;
  }
  ```
- **move_pane_focus() implementation:** `baude/src/app.rs:4228-4250`:
  - Returns early if focus is on remote row (line 4231)
  - Returns early if focus is on sidebar (line 4234)
  - Returns early if no shell open or shell doesn't exist (lines 4237-4241)
  - Sets focus based on `to_shell: bool` parameter (lines 4243-4247)
  - Calls `remember_pane_focus()` to record the change (line 4248)
- **Test: directional, not toggle:** `baude/src/app.rs:11764-11784` — Test `alt_arrows_move_pane_focus_by_direction_not_toggle()`:
  ```rust
  app.handle_key(alt(KeyCode::Down));
  assert_eq!(app.focus, Focus::Shell, "alt+down focuses the shell");
  app.handle_key(alt(KeyCode::Down));
  assert_eq!(app.focus, Focus::Shell, "alt+down AGAIN stays on shell (not a toggle)");
  app.handle_key(alt(KeyCode::Up));
  assert_eq!(app.focus, Focus::Claude, "alt+up focuses the agent pane");
  app.handle_key(alt(KeyCode::Up));
  assert_eq!(app.focus, Focus::Claude, "alt+up AGAIN stays on agent (not a toggle)");
  ```
  This directly refutes a toggle implementation and proves directional behavior.
- **Test: no-op cases:** `baude/src/app.rs:11786-11800` — Test `alt_arrows_are_noops_without_a_shell_and_from_the_sidebar()`:
  - From sidebar, keys are no-op
  - With no shell open, keys are no-op
- **Test: remote rows:** `baude/src/app.rs:11835-11860` — Test `remote_rows_focus_claude_and_ignore_the_pane_keys()`:
  ```rust
  app.selected_id = Some(SelId::Remote(1));
  app.handle_key(alt(KeyCode::Up));
  assert_eq!(app.focus, Focus::Shell, "alt+up is a no-op on a remote row");
  app.handle_key(alt(KeyCode::Down));
  assert_eq!(app.focus, Focus::Shell, "alt+down is a no-op on a remote row");
  ```
  Remote rows have no shell pane, so keys are no-ops.
- **Help overlay documentation:** `baude/src/ui.rs:2353` — `"  alt+↑/↓     move focus between claude and shell panes"`
- **README documentation:** `README.md:121` — `| \`alt+↑/↓\` | claude or shell pane | move focus to the pane above/below (shell sits below claude) |`
- **Status:** ✓ VERIFIED

## Test Coverage

All four key tests pass with real, non-vacuous assertions:

| Test | Location | Validates |
|------|----------|-----------|
| `alt_arrows_move_pane_focus_by_direction_not_toggle` | baude/src/app.rs:11764 | Keys are directional, not toggle; down always shell, up always agent |
| `alt_arrows_are_noops_without_a_shell_and_from_the_sidebar` | baude/src/app.rs:11786 | No-ops when shell absent or focus in sidebar |
| `pane_focus_survives_a_visit_to_a_session_without_a_shell` | baude/src/app.rs:11802 | GitHub #89 round-trip: focus restored after visiting session without shell |
| `remote_rows_focus_claude_and_ignore_the_pane_keys` | baude/src/app.rs:11835 | Remote rows have no shell pane; keys are no-ops |

All tests use `handle_key()` (the real entry point) and drive through full session fixtures.

**Test execution:** `cargo test -p baude --bin baude alt_arrows pane_focus remote_rows` returns `test result: ok. 4 passed` (verified above).

## CI Gates

All four gates pass with exit code 0:

| Gate | Result |
|------|--------|
| `cargo fmt --all -- --check` | ✓ PASS |
| `cargo clippy --all-targets -- -D warnings` | ✓ PASS |
| `cargo build --workspace` | ✓ PASS |
| `cargo test -p baude --bin baude` (4 focus tests) | ✓ PASS |

## Requirements Coverage

**UX-01** (GitHub #89): Per-session pane focus remembered across switches, with key to move between panes.
- ✓ **SATISFIED** — `Session.pane_focus_shell` stores state per session, `remember_pane_focus()` records on change, `restore_pane_focus()` restores on activation, `alt+↑/↓` moves focus with tests proving behavior.

## Design Decisions Verified

Per 16-CONTEXT.md:

| Decision | Verification |
|----------|--------------|
| D1: Session-local field (not app-level map) | ✓ `Session.pane_focus_shell: bool` field at baude-core/src/session.rs:99 |
| D2: Fallback to Claude when shell absent | ✓ `restore_pane_focus()` checks `shell_open && shell.is_some()` before setting Shell |
| D3: Keys are directional (up=agent, down=shell) | ✓ `move_pane_focus(to_shell: bool)` directional; test `alt_arrows_move_pane_focus_by_direction_not_toggle` proves it |
| D4: Claude ↔ Shell only (no sidebar cycling) | ✓ `move_pane_focus()` returns early if `focus == Focus::Sidebar` |
| D5: Session-switch-only (not persisted to disk) | ✓ Comment at baude-core/src/session.rs:96-99 "In-memory only: deliberately absent from persisted state" |
| D6: Activation paths restore, deliberate-reset paths don't | ✓ `restore_pane_focus()` called in 7 activation paths; 15+ deliberate-reset paths unchanged |

## Anti-Patterns

No debt markers (TBD, FIXME, XXX) found in modified files. No hardcoded stubs or placeholders in the implementation.

## Known Deviations from Plan

**Plan stated:** "A keyboard shortcut toggles focus between the Claude pane and shell pane" (ROADMAP SC 2)

**Actual implementation:** Keys are directional — `alt+↑` focuses Claude (agent pane), `alt+↓` focuses shell. Not a toggle.

**Rationale (from 16-CONTEXT.md D3):**
- The shell is stacked BELOW the agent pane in `pane_rects` (app.rs:563), so direction is literal.
- `alt+←/→` cycle sessions on the horizontal axis; `alt+↑/↓` extend the same modifier to the vertical axis.
- Directional avoids the legacy-encoding ambiguity of `ctrl+/` (which collides with `ctrl+7`/`ctrl+_`).

**Assessment:** This is the correct behavior per locked design decisions D3, not a bug. The wave agent initially implemented a toggle; the orchestrator corrected it to directional in commit 40eab2d.

---

**Verified by:** Claude Verifier
**Verified:** 2026-09-22 13:30 UTC
