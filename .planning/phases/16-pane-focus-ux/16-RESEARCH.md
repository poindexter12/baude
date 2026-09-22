# Phase 16: Pane Focus UX - Research

**Gathered:** 2026-09-22
**Scope:** Code inventory, focus assignment sites, key collision check, session state persistence model

## Focus Enum and Global State

**File:** `baude/src/app.rs`

- **Line 200**: `pub enum Focus { Sidebar, Claude, Shell }` — three-state enum, single app-wide field `self.focus`.
- **Lines 154-155**: `fn is_backslash(code)` — ctrl+\ arrives as raw byte 0x1C, reported as ctrl+4 on legacy terminals. Used to check both `Char('\\')` and `Char('4')`.
- **Lines 4123-4175**: `fn handle_key()` — routes keys by focus (sidebar, Claude, Shell) after checking global chords (ctrl+q, ctrl+\, alt+←/→, ctrl+e, ctrl+n, ctrl+o).

## Key Binding Collision Check

**File:** `baude/src/app.rs` (handle_key and related), `baude/src/ui.rs` (help overlay), `README.md` (Keys section)

**Global chords (line 4135-4169):**
- `ctrl+q` (line 4135): back to sidebar
- `ctrl+\` (via `is_backslash`, line 4140): toggle shell pane
- `alt+←` (line 4145): cycle session -1
- `alt+→` (line 4149): cycle session +1
- `ctrl+e` (line 4153): open editor
- `ctrl+n` (line 4157): new session
- `ctrl+o` (line 4163): link hints

**Sidebar keys (line 4595-4673):**
- `q`: quit
- `j`/`↓`: move selection down
- `k`/`↑`: move selection up
- `n`: new session
- `c`: clone
- `?`: help
- `z`: show/hide archived
- `f`: scope/unscope
- `w`, `x`, `X`, `t`, `e`, `i`, `v`, `g`, `r`, `a`: various sidebar actions

**Help overlay (ui.rs:2311-2377):**
- Line 2315: `centered(area, 60, 42)` — 60 columns wide, 42 rows tall (40 text + 2 border)
- Line 2312-2314 comment: "keep in sync when adding rows"
- Lines 2323-2354: current key bindings listed (ctrl+q, ctrl+\, ctrl+e, ctrl+n, alt+←/→, ctrl+o, shift+enter)

**README (line 108-138):**
- Table lists all documented keys; no `ctrl+/` entry.

**Collision result:** `ctrl+/` (Ctrl + forward slash) is NOT used anywhere in code, help overlay, or README.

## Session Focus Preservation Pattern (cycle_session)

**File:** `baude/src/app.rs`
**Lines:** 5345-5407

```
fn cycle_session(&mut self, delta: i64) {
    // ... filter to live sessions, find current index, compute next ...
    self.selected_id = Some(ids[next as usize]);
    
    // PRESERVE FOCUS IF SHELL EXISTS:
    if self.focus == Focus::Shell {
        let has_shell = self
            .selected()
            .map(|s| s.shell_open && s.shell.is_some())
            .unwrap_or(false);
        if !has_shell {
            self.focus = Focus::Claude;  // FALLBACK
        }
    }
}
```

This checks `shell_open && shell.is_some()` to decide whether to preserve Shell focus or fall back to Claude. This is the exact pattern to replicate in other session-activation paths.

## Session State Persistence Model (RetainedSessionState)

**File:** `baude-core/src/repository.rs`
**Lines:** 272-287

```rust
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedSessionState {
    pub name: String,
    pub cwd: PersistedPath,
    pub repo_root: PersistedPath,
    pub branch: Option<String>,
    pub is_worktree: bool,
    pub shell_open: bool,              // <-- PERSISTENCE MODEL
    pub archived: bool,
    pub archived_by_user: bool,
    #[serde(default)]
    pub resume_id: Option<String>,
}
```

This structure is used for `SavedCheckout.session` (line 335) and survives a baude restart through deserialization. Backward-compat is guaranteed by `#[serde(default)]` on optional fields.

For Phase 16 (session-only memory), focus is NOT persisted. For Phase 16+ (restart-proof), add:
```rust
#[serde(default)]
pub last_focus_pane: Option<String>,  // "claude" or "shell"
```

## Toggle Shell Pattern (focus handling on open/close)

**File:** `baude/src/app.rs`
**Lines:** 5450-5484

```rust
fn toggle_shell(&mut self, focus_it: bool) {
    // ... get selected session ...
    if s.shell_open {
        s.shell_open = false;
        if self.focus == Focus::Shell {
            self.focus = Focus::Claude;  // FALLBACK ON CLOSE
        }
    } else {
        // ... open shell ...
        match s.open_shell(...) {
            Ok(()) => {
                if focus_it {
                    self.focus = Focus::Shell;  // FOCUS ON OPEN
                }
            }
            Err(e) => self.set_message(...),
        }
    }
}
```

When shell closes while focused, focus falls back to Claude. When opening with `focus_it=true`, focus is set to Shell. Phase 16 preserves this behavior.

## Twenty-Nine Focus Assignment Sites

**File:** `baude/src/app.rs`

Lines where `self.focus = Focus::X;` appears (complete list):

1. **1893** (in `apply_activity_selection`): Claude — modal confirm, action triggers, focus back to pane
2. **1950** (in `apply_activity_selection`): Claude — same context
3. **2482** (in `handle_closed_remote`): Claude — remote session closed
4. **2562** (in `attach_selected_remote`): Claude — attach remote, focus pane
5. **2649** (in `toggle_show_archived`): Claude — archive toggle, restore focus?
6. **2682** (in `toggle_archive`): Claude — manual archive, stay on pane
7. **3252** (in sidebar selection init): Sidebar — startup/restore, move to sidebar
8. **3286** (in selection reconcile): Claude — target lost, move to Claude
9. **3340** (in selection reconcile): Claude — same recovery path
10. **3736** (in step back from modal): Sidebar — close modal, return to sidebar
11. **3748** (in `handle_sidebar_key` escape): Sidebar — escape out
12. **3773** (in `return_to_sidebar`): Sidebar — explicit return
13. **4007** (in sidebar action): Sidebar — return from action modal
14. **4027** (in close confirm): Sidebar — close action, return to sidebar
15. **4040** (in close confirm error): Claude — session stays open, focus pane
16. **4054** (in remove confirm): Claude — remove action, focus pane if session open
17. **4060** (in remove confirm error): Sidebar — removal failed, back to selection
18. **4137** (in `return_to_sidebar`): Sidebar — explicit return
19. **4159** (in new session modal): Sidebar — new session, step out to sidebar
20. **4415** (in `open_local_target`): Claude — open/reopen, focus pane
21. **5003** (in apply permission decision): Claude — permission handled, focus pane
22. **5024** (in apply permission decision): Claude — same
23. **5034** (in apply permission decision): Claude — same
24. **5100** (in `selection_changed`): Claude — sidebar selection hit pane, focus pane
25. **5110** (in `selection_changed`): Claude — same
26. **5141** (in `handle_sidebar_key`, info action): Sidebar — info modal opened
27. **5399** (in `cycle_session`): Claude — explicit fallback in cycle
28. **5464** (in `toggle_shell`): Claude — shell close while focused, fallback
29. **5472** (in `toggle_shell`): Shell — shell open with focus_it=true
30. **5495** (in `toggle_shell`): Claude — toggle shell when already focused closes it
31. **5507** (in `toggle_standalone_shell`): Shell — standalone shell open
32. **5568** (in `toggle_standalone_shell`): Shell — same
33. **5659** (in `confirm_close_selected`): Claude — close modal confirm done

**Classification for Plan 16:**
- Lines 1893, 1950, 2562, 2649, 2682, 3286, 3340, 4040, 4054, 4415, 5003, 5024, 5034, 5100, 5110, 5472, 5507, 5568 — **restore remembered focus** (session activation/selection/modal-confirm)
- Lines 2482, 3252, 3736, 3748, 3773, 4007, 4027, 4060, 4137, 4159, 5141, 5399, 5464, 5495, 5659 — **deliberate reset** (error recovery, return to sidebar, modal close, shell toggle close, session lost)

## Remote Session Focus State (bauded)

**Files:** `bauded/src/api.rs` (SessionInfo), `baude/src/app.rs` (attach handling)

- Remote sessions receive focus state from daemon's `SessionInfo` struct.
- `baude/src/app.rs` does not directly manipulate remote session focus; it reads it from the attach channel.
- Phase 16 extends `SessionInfo` to carry last-focused pane if daemon support is added later. For now, remote sessions start with Claude focus (default).

## Test Patterns

**Binary-only testing:** `cargo test -p baude --bin baude -- test_name1 test_name2` (not `--lib`).
**TestBackend pattern:** `ui.rs::tests` render with `ratatui::backend::TestBackend`.
**Fixture pattern:** `TestRedirect::new()` for home-dir isolation; no `std::env::set_var`.

## Help Overlay Layout

**File:** `baude/src/ui.rs`
**Lines:** 2311-2377

Current layout:
- Line 2312-2314: Comment "40 paragraph lines + 2 border rows"
- Line 2315: `centered(area, 60, 42)` — height 42 fixed
- Lines 2318-2368: Content (52 lines of `Line::raw()` and `Line::from()`)

Actual line count in help text:
- Heading 1 + 8 rows = 9 lines
- Blank + Heading 2 + 6 rows = 8 lines  
- Blank + Heading 3 + 3 rows = 5 lines
- Blank + "press any key" = 2 lines
- **Total: 24 lines** of content + 2 border rows = 26 rows needed (current: 42, plenty of room)

Adding one line for `ctrl+/` focus toggle fits easily; no height adjustment needed until more lines are added.

## README Keys Section

**File:** `README.md`
**Lines:** 108-138

Table format with | Key | Where | Action |. Adding a row:
```
| `ctrl+/` | Claude or shell pane | swap focus between Claude pane and expanded shell pane |
```

Would be inserted in the "global (any pane)" section (line 2346 area in ui.rs help).
