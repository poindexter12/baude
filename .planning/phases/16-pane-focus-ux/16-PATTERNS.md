# Phase 16: Pane Focus UX - Patterns

**Gathered:** 2026-09-22
**Scope:** Closest existing analogs for focus state preservation, key binding dispatch, and session-local state retention

## Pattern 1: Session-Local State Preservation Across Switches (cycle_session model)

**Location:** `baude/src/app.rs:5345-5407`
**Used for:** Preserving `Focus::Shell` across `alt+←/→` session cycling

**Pattern:**
```rust
fn cycle_session(&mut self, delta: i64) {
    // ... compute next session id ...
    self.selected_id = Some(next_id);
    
    // Check if shell exists in target session
    if self.focus == Focus::Shell {
        let has_shell = self
            .selected()
            .map(|s| s.shell_open && s.shell.is_some())
            .unwrap_or(false);
        if !has_shell {
            self.focus = Focus::Claude;  // Fallback
        }
        // Else: preserve Shell focus (implicit)
    }
}
```

**Application in Phase 16:**
This is the exact behavior to generalize across ALL session-activation paths (select, restore, reopen, unarchive). The check `shell_open && shell.is_some()` is the foundation for remembering focus — if the session has a shell open, preserve the focus; otherwise default to Claude.

## Pattern 2: Focus Fallback on State Change (toggle_shell model)

**Location:** `baude/src/app.rs:5450-5484`
**Used for:** Managing focus when the shell pane is opened or closed

**Pattern:**
```rust
fn toggle_shell(&mut self, focus_it: bool) {
    let s = self.selected_mut();
    if s.shell_open {
        s.shell_open = false;
        if self.focus == Focus::Shell {
            self.focus = Focus::Claude;  // Fallback when closed
        }
    } else {
        s.open_shell(...);
        if focus_it {
            self.focus = Focus::Shell;  // Focus when opened
        }
    }
}
```

**Application in Phase 16:**
This pattern handles the edge case: when shell is closed while focused, fallback to Claude. When shell is opened with `focus_it=true` (from `t` key in sidebar), focus is set to Shell. Phase 16 preserves this logic exactly.

## Pattern 3: Key Binding Dispatch (global chords)

**Location:** `baude/src/app.rs:4123-4175` (handle_key)
**Used for:** Global chord routing (ctrl+q, ctrl+\, alt+arrow, ctrl+e, ctrl+n, ctrl+o)

**Pattern:**
```rust
fn handle_key(&mut self, key: KeyEvent) {
    // ... modal handling first ...
    
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    
    // Global chords (identical in all focus)
    if ctrl && matches!(key.code, KeyCode::Char('q')) {
        self.focus = Focus::Sidebar;
        return;
    }
    if ctrl && is_backslash(key.code) {
        self.toggle_shell(true);
        return;
    }
    // ... more chords ...
    
    // Dispatch by focus
    match self.focus {
        Focus::Sidebar => self.handle_sidebar_key(key),
        Focus::Claude => self.forward_key(key, false),
        Focus::Shell => self.forward_key(key, true),
    }
}
```

**Application in Phase 16:**
Add the pane-focus chords (`alt+↑` / `alt+↓`) as new global chords AFTER checking modal state and BEFORE the focus dispatch, beside the existing `alt+←/→` session-cycling arms. They should:
1. Check if shell is open and visible
2. If yes and focus is Claude: set focus to Shell
3. If yes and focus is Shell: set focus to Claude
4. If no: do nothing (no-op)

```rust
if ctrl && matches!(key.code, KeyCode::Char('/')) {
    self.swap_pane_focus();
    return;
}
```

## Pattern 4: Per-Session Persisted State (shell_open model)

**Location:** `baude-core/src/repository.rs:274-287` (RetainedSessionState)
**Used for:** Persisting `shell_open: bool` across baude restarts

**Pattern:**
```rust
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedSessionState {
    pub name: String,
    pub cwd: PersistedPath,
    pub repo_root: PersistedPath,
    pub branch: Option<String>,
    pub is_worktree: bool,
    pub shell_open: bool,              // <-- Session-local, persisted
    pub archived: bool,
    pub archived_by_user: bool,
    #[serde(default)]
    pub resume_id: Option<String>,
}
```

**Application in Phase 16:**
Phase 16 does NOT persist focus to disk (session-only memory). The runtime `Session` struct (baude-core/src/session.rs) does not yet have a focus field — focus is only on the `App` struct. For Phase 16+ (restart-proof), add to `RetainedSessionState`:

```rust
#[serde(default)]
pub last_focus_pane: Option<String>,  // "claude" or "shell"
```

## Pattern 5: Selection Change Handling (selection_changed method)

**Location:** `baude/src/app.rs:5083-5125`
**Used for:** Managing focus when user selects a new sidebar row via j/k

**Pattern:**
```rust
fn selection_changed(&mut self) {
    // ... reconcile runtime state with selection ...
    
    // When changing sidebar selection while in a pane, return to Claude pane
    // (sidebar selection targets a row, not a pane)
    if !matches!(self.focus, Focus::Sidebar) && self.selected().is_some() {
        self.focus = Focus::Claude;  // Reset focus to start point
    }
}
```

**Application in Phase 16:**
This site (line 5100 and 5110) sets focus to Claude when user selects a different sidebar row from within a pane. This is a deliberate reset (not a restore) and should NOT change in Phase 16.

## Pattern 6: Modal-Confirm and State Transition (apply_activity_selection)

**Location:** `baude/src/app.rs:1850-1960`
**Used for:** Managing focus after a modal action confirms (e.g., selecting a permission choice)

**Pattern:**
```rust
fn apply_activity_selection(&mut self) {
    match activity {
        Activity::PermissionDecision(decision) => {
            // ... apply decision ...
            self.focus = Focus::Claude;  // Focus back to pane after action
        }
        _ => {}
    }
}
```

**Application in Phase 16:**
Modal-confirm paths that conclude with a pane action should restore remembered focus (if shell is open and was previously focused). Lines 1893, 1950 and similar modal-action endpoints.

## Pattern 7: Error Recovery and Fallback (selection_reconcile)

**Location:** `baude/src/app.rs:3261-3350`
**Used for:** Handling focus when the selected session becomes unavailable

**Pattern:**
```rust
fn selection_reconcile(&mut self) {
    if self.selected().is_none() {
        // Selected session disappeared
        self.selected_id = None;
        self.focus = Focus::Sidebar;  // Return to sidebar (deliberate reset)
    }
    if let Some(s) = self.selected_mut() {
        if s.claude.is_exited() && self.focus == Focus::Claude {
            // Session exited while we were focused on it
            self.focus = Focus::Sidebar;  // Back to sidebar (deliberate reset)
        }
    }
}
```

**Application in Phase 16:**
Error recovery paths (session lost, claude exited) should NOT restore focus; they should reset to Sidebar or Claude as appropriate. No change in Phase 16.

## Pattern 8: Runtime Session Identity (SelId)

**Location:** `baude/src/app.rs:208-214`
**Used for:** Keying session lookup across restore and selection changes

**Pattern:**
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelId {
    Repository(RepositoryKey),
    Checkout(CheckoutKey),
    Standalone(StandaloneKey),
    Remote(u64),
}
```

**Application in Phase 16:**
Session focus is per-`SelId`. When cycling or selecting, the focus state is looked up by this key. If storing focus as a runtime map `HashMap<SelId, Focus>`, this is the key. Lookups via `self.runtime_checkouts.get(key)` return the runtime session id, which can be used to store focus associations.

## Pattern 9: Forward Key to Pane (forward_key method)

**Location:** `baude/src/app.rs:4177-4233`
**Used for:** Routing keyboard input to Claude or Shell panes based on focus

**Pattern:**
```rust
fn forward_key(&mut self, key: KeyEvent, to_shell: bool) {
    // ... handle remote attach ...
    let Some(s) = self.selected_mut() else { return };
    let pty = if to_shell {
        match s.shell.as_mut() {
            Some(p) => p,
            None => return,  // Shell not open, key is dropped
        }
    } else {
        &mut s.claude
    };
    pty.write_input(&encode_key(&key, ctx));
}
```

**Application in Phase 16:**
The pane-focus chords (`alt+↑` / `alt+↓`) should NOT forward to the pane — it is a global action handled in handle_key before dispatch. The forward_key path is unaffected by Phase 16.

## Pattern 10: Focus Dispatch (match on focus)

**Location:** `baude/src/app.rs:4170-4174`
**Used for:** Routing key events to the correct handler based on current focus

**Pattern:**
```rust
match self.focus {
    Focus::Sidebar => self.handle_sidebar_key(key),
    Focus::Claude => self.forward_key(key, false),
    Focus::Shell => self.forward_key(key, true),
}
```

**Application in Phase 16:**
The focus-toggle chord must be added BEFORE this dispatch so it is not forwarded to panes. After the global chords (`ctrl+q`, `ctrl+\`, etc.), and before the focus dispatch.
