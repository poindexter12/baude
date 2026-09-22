---
phase: 16-pane-focus-ux
reviewed: 2026-09-22T00:00:00Z
depth: standard
files_reviewed: 5
files_reviewed_list:
  - README.md
  - baude-core/src/session.rs
  - baude/src/app.rs
  - baude/src/ui.rs
  - bauded/src/manager.rs
findings:
  critical: 1
  warning: 0
  info: 0
  total: 1
status: issues_found
---

# Phase 16: Pane Focus UX — Code Review Report

**Reviewed:** 2026-09-22  
**Depth:** standard  
**Files Reviewed:** 5  
**Status:** issues_found

## Summary

Phase 16 implements per-session pane focus memory (UX-01): sessions remember whether they were last used in the Claude or shell pane, and restore that focus when the user returns to them. The feature adds `Session::pane_focus_shell`, functions to remember/restore focus on session selection, directional keybindings (`alt+↑/↓`), and tests validating the round-trip behavior.

The implementation has one critical bug: the `Restart` branch in the lifecycle dispatch for `reopen_checkout` fails to call `restore_pane_focus()` after changing `selected_id`, leaving the focus in whatever state it was before the restart rather than restoring what the session was left on. This breaks the symmetry with the `Focus` and `Spawn` branches and violates the UX-01 contract.

All other tests pass, initialization sites correctly set `pane_focus_shell: false`, help overlay height is accurate, and key dispatch placement is correct.

## Critical Issues

### CR-01: Restart dispatch branch skips restore_pane_focus()

**File:** `baude/src/app.rs:2658-2666`

**Issue:** The `Restart` branch in the lifecycle dispatch for `reopen_checkout` changes `selected_id` without calling `restore_pane_focus()`. 

In the same function, the `Focus` branch (line 2650-2657) calls `restore_pane_focus()` after changing `selected_id`, and the `Spawn` branch (line 2668-2687) calls it as well. The `Restart` branch is the only one that omits it:

```rust
lifecycle::ReopenDispatch::Restart { id } => {
    self.restart_session_with_mode(id, plan.mode)?;
    self.selected_id = Some(SelId::Checkout(checkout_key));
    // Missing: self.restore_pane_focus();
    Ok(LifecycleOutcome::Reopened {
        checkout: checkout_key,
        runtime: id,
    })
}
```

This means when a session is restarted (e.g., via the `r` key after Claude exits), the pane focus from the previous session or selection persists instead of being restored to what the restarted session was last left on. This reintroduces the issue #89 bug for the restart case.

**Fix:**
```rust
lifecycle::ReopenDispatch::Restart { id } => {
    self.restart_session_with_mode(id, plan.mode)?;
    self.selected_id = Some(SelId::Checkout(checkout_key));
    // UX-01: land on the pane this session was last left on.
    self.restore_pane_focus();
    Ok(LifecycleOutcome::Reopened {
        checkout: checkout_key,
        runtime: id,
    })
}
```

---

_Reviewed: 2026-09-22_  
_Reviewer: Claude (gsd-code-reviewer)_  
_Depth: standard_
