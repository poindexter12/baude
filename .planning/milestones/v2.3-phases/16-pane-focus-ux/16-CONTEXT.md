# Phase 16: Pane Focus UX - Context

**Gathered:** 2026-09-22
**Status:** Ready for planning
**Mode:** Planning-only discovery; design questions answered below

<domain>
## Phase Boundary

Pane focus (Claude vs shell) is remembered when the user switches sessions and returns, and a keyboard shortcut swaps focus between the two panes while both are visible. Requirements: UX-01. Out of scope: focus memory surviving a baude restart (session-only for Phase 16; persist-on-exit is deferred), sidebar focus cycling, remote pane navigation (remote rows have no shell pane).

</domain>

<decisions>
## Design Decisions

### Where the remembered pane lives (D1)
Session-local field, persisted through `RetainedSessionState` (mirroring `shell_open: bool`), not an app-level map. Reason: focus is per-session state — each session remembers which pane it had focus when last active. This matches the model of `shell_open` and avoids app-level map growth (100+ sessions = 100+ map entries). Remote sessions are excluded: `toggle_shell` (app.rs:5450) handles only Standalone and local checkout rows, and `RemoteAttach` (baude/src/remote.rs:290) carries a single `parser`, so a remote row has no shell pane to focus. Attaching a remote always focuses Claude and the swap key is a no-op there. The daemon is not involved: pane focus is TUI-local state and no field is added to `SessionInfo`.

### Fallback when shell is closed or missing (D2)
Focus falls back to Claude, exactly as `cycle_session` (app.rs:5399-5407) already does. When a session switch lands on a target with no open shell, focus is Claude. When the user closes the shell while focused on it, focus goes to Claude (toggle_shell pattern, app.rs:5464). When opening the shell again with `t` or `ctrl+\`, focus goes to Shell (existing `focus_it=true` pattern).

### Key binding for moving focus between panes (D3)
`alt+↑` focuses the pane above (Claude) and `alt+↓` the pane below (the shell). Chosen over `ctrl+/` for three reasons:

1. **Spatially true.** `pane_rects` (app.rs:563) stacks the shell BELOW Claude, so up/down names what actually happens.
2. **Already the family.** `alt+←/→` cycles sessions (app.rs:4145-4152); `alt+↑/↓` extends the same modifier to the other axis, and `alt+↑`/`alt+↓` appear nowhere in `handle_key`, the help overlay, or the README.
3. **No legacy-encoding trap.** `ctrl+/` reaches the terminal as raw byte 0x1F and is indistinguishable from `ctrl+7`/`ctrl+_` in legacy encoding — the same class of problem the codebase already works around for `ctrl+\`, where `is_backslash` (app.rs:153) must match BOTH `Char('\\')` and `Char('4')` because 0x1C is ambiguous. Arrow keys arrive as unambiguous CSI sequences with a modifier parameter, so no dual-match helper is needed.

Inherited caveat, to be documented: `alt+↑/↓` needs the terminal to send Option/Alt as a modifier, exactly as the README already warns for `alt+←/→` (README line 143).

### Cycling behavior: Sidebar → Claude → Shell, or Claude ↔ Shell only (D4)
Claude ↔ Shell only. The requirement says "toggles focus between the Claude pane and the expanded shell pane" — narrow reading: swap the two panes. The sidebar is unreachable from the panes with this key (ctrl+q returns to sidebar). Behavior: `alt+↓` from the Claude pane focuses the shell when one is open; `alt+↑` from the shell focuses Claude. In the sidebar both are no-ops (there is no pane focus to move). With no shell open, both are no-ops.

### Focus memory persistence: restart-proof or session-switch-only (D5)
Session-switch-only (memory only, not persisted to disk) for Phase 16. Reason: the requirement specifies "remembered when the user switches to a different session and back" — the immediate need is session-switch continuity, not baude restart. Persisting to disk requires adding a field to `RetainedSessionState` and `RetainedStandaloneSessionState` with backward-compat serde(default), and test coverage of state-file loading. Session-only is simpler: use a runtime app field `last_focus: HashMap<SelId, Focus>` or `current_session_focus: Focus` that is populated from the session's `shell_open` state on selection and updated on focus changes.

### Exact sites to change focus from "attach to session" vs "deliberate reset" (D6)
Twenty-nine sites identified where `self.focus = Focus::Claude;` or `= Focus::Sidebar;` appear. The plan must classify each:
  - **"Attach to session" paths** (restore remembered focus): activate session, select session, restore from saved state, reopen session, unarchive session, modal confirm-and-open paths. These should restore from the remembered value (shell_open + whether shell is currently open).
  - **"Deliberate reset" paths** (force a specific focus): error recovery, modal close, toggle shell close, shell exited, session failed, sidebar selection from pane, return from modal. These should set focus unconditionally.

The plan task names each changed site and states the reason (one-line).

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `baude/src/app.rs:200` — `pub enum Focus { Sidebar, Claude, Shell }`, single app-wide field `self.focus` (not per-session).
- `cycle_session` (app.rs:5345-5401) — already preserves `Focus::Shell` across session switches, checking `shell_open && shell.is_some()` and falling back to Claude. This is the pattern to generalize.
- `toggle_shell` (app.rs:5450-5484) — sets `focus_it=true` when opening to focus the shell, and falls back to Claude when closing while focused.
- `move_selection` (app.rs:5323) — changes `selected_id` with j/k, does NOT touch focus.
- `Session.shell_open: bool` and `Session.shell: Option<Pty>` — both per-session, persisted in `RetainedSessionState` (baude-core/src/repository.rs:274-287).
- `baude/src/ui.rs:2315` — help modal `centered(area, 60, 42)` with comment "40 paragraph lines + 2 border rows; keep in sync".
- Help overlay (ui.rs:2318-2368) — global chords section at line 2343, four existing lines, new line will fit within height adjustment.
- README "## Keys" section (line 108-138) — table of key bindings, no entry for focus swap yet.

### Established Patterns
- Focus dispatch in `match self.focus { Focus::Sidebar => ..., Focus::Claude => ..., Focus::Shell => ... }` (app.rs:4170-4174, forward_key, handle_paste).
- Session identity across restore is `SelId` (Repository, Checkout, Standalone, Remote) — lookup is `self.runtime_checkouts.get(key)` returning runtime session id.
- Binary-only cargo test syntax: `cargo test -p baude --bin baude -- name1 name2` (no `-p baude --lib`; TestBackend available).

### Integration Points
- `app.rs` handle_key (global chord dispatch), move_selection (sidebar j/k), select (on selection change), restore (on startup), and 29 sites with explicit focus assignment.
- `ui.rs` help modal and status-bar focus indicator (if any).
- `repository.rs` `RetainedSessionState` for per-session persisted state (for Phase 16+ if restart-proof is added; Phase 16 is session-only).

</code_context>

<specifics>
## Phase 16 Specifics

- Joe (2026-09-21 requirement): "no way to move focus between the Claude pane and the expanded shell pane directly; the only way is to toggle the shell pane closed and open again" → UX-01 toggles focus directly.
- Joe (requirement): "switching to another session and back returns focus to the Claude pane, not the shell pane" → focus should be remembered per session across switch-away and switch-back.
- Baseline: `cycle_session` preserves `Focus::Shell` only if target has open shell; Phase 16 generalizes this to all session-activation paths.
- Key binding not yet assigned in code or README; `alt+↑/↓` confirmed free against handle_key, the help overlay, and the README Keys table.

</specifics>

<deferred>
## Deferred Ideas

- Focus memory surviving baude restart (Phase 16+ enhancement; requires `RetainedSessionState` field + back-compat serde).
- Sidebar focus cycling (not in scope; requirement is pane-only).
- Remote pane focus: out of scope by construction, remote rows have no shell pane (see D1).

</deferred>
