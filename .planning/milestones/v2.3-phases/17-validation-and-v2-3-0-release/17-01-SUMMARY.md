---
phase: "17"
plan: 01
status: complete
requirements:
  - SHIP-05
completed: 2026-09-22
key_files:
  - baude/src/main.rs
  - README.md
tests_added:
  - loop_timing_stages_are_durations_not_timestamps
---

# Phase 17 Summary: Validation and v2.3.0 Release

## What was done

### 1. CI gates (criteria 1 and 2)

All four run from a clean tree, exit codes captured separately:

| Gate | Exit |
|------|------|
| `cargo fmt --all -- --check` | 0 |
| `cargo clippy --all-targets -- -D warnings` | 0 |
| `cargo build --workspace` | 0 |
| `cargo test --workspace` | 0 (793 tests) |

### 2. Automated smoke checks (criterion 3, automatable half)

Run against a real release build, fully isolated from the user's live environment via
`XDG_CONFIG_HOME` (which `persist::real_config_base` honours) plus a harmless
`BAUDE_CLAUDE_CMD`, so no real agent process was spawned and `~/.config/baude` was never
touched (verified: the live `state.json` mtime was unchanged).

- **Workspace derivation (phase 13) — PASS, end to end.** Launching from a fresh
  `/tmp/.../repo` derived the workspace name `repo` from the repository root folder and
  recorded the binding in `folder-workspaces.json`. State was written to the
  workspace-scoped `state-repo.json`, not the legacy `state.json`.
- **Startup timing (phase 15) — PASS.** `BAUDE_TIMING=1` printed all stages to stderr on
  exit: `config_load`, `workspace_resolution`, `keyboard_probe`, `terminal_setup`,
  `app_new`, `first_frame`, `session_restore` (with session count), `first_metadata_poll`,
  `total`.
- **Bounded keyboard probe (phase 15) — PASS.** Reported
  `keyboard_probe: 251 ms (kitty 250ms (timeout))` under a pty with no kitty support: the
  250 ms bound held.
- **Worktree identity and ownership (phase 14) — PASS.** `baude worktrees scan` ran
  preview-only and printed per-directory owner rows, e.g.
  `claude/repository-14 (owner: nexia-fulfillment (...))`.

### 3. Defect found and fixed during validation

`fix(17-01)` (3ece712): the timing report printed one `ms` column of per-stage durations,
but the three loop stages stored elapsed-since-origin, so they all printed the same number
(`first_frame: 1952`, `session_restore: 1952`, `first_metadata_poll: 1952`). That reads as
"the first frame took two seconds and restoring zero sessions took two more", when it means
each happened 1952 ms in. Misleading in exactly the report that exists to diagnose slow
startup. Each loop stage now reports time since the previous mark; the same run then reads
`first_frame 1748 ms`, `session_restore 0 ms`, `first_metadata_poll 0 ms`.

### 4. Integration with origin/main (merge, not rebase)

Verified live before acting: `main` has `required_linear_history: false` and
`allow_merge_commit: true`, so replaying ~200 commits was unnecessary.

- Backup ref taken first: `backup/pre-main-merge-20260922-104929`.
- `git merge --no-edit origin/main` completed with **no conflicts** (7 files: version
  metadata, `Cargo.lock`, `CHANGELOG.md`, `.release-please-manifest.json`).
- After: `origin/main` is an ancestor of HEAD, 0 behind / 204 ahead, workspace version now
  `2.2.0` (release-please will bump to 2.3.0 in its own PR; no version was hand-edited).
- All four gates re-run green on the merged tree (793 tests).

### 5. Documentation (criterion 4)

README covers all four milestone areas: workspace derivation and new-session defaults
(phase 13), the managed worktree identity scheme with `.baude-marker.json` and collision
suffixing (phase 14), the Status codes section and the Performance section covering timing,
idle behaviour, `idle_child_policy` and `usage_poll_secs` (phase 15), and the `alt+↑/↓`
pane-focus chord (phase 16).

## Not done, and why

- **Nothing was pushed and no pull request was opened.** Those are outward-facing actions
  the repository owner authorises; the plan deliberately excludes them.
- **The manual terminal smoke checklist below was not executed.** baude is a full-screen
  TUI and these observations need a human at a real terminal. Everything mechanically
  checkable was automated instead (see section 2).

## Manual smoke checklist (for a human, in a real terminal)

Run from a real terminal, ideally a kitty-capable one so the probe path differs from the
automated run. Nothing here is destructive.

1. `cd` into a git repository you have never opened with baude, run `baude`, and confirm the
   sidebar title shows a workspace derived from that repository's folder name.
2. Quit with `q`, relaunch from a SUBFOLDER of the same repository, and confirm the same
   workspace is reused (the recorded binding, not a second derivation).
3. `BAUDE_TIMING=1 baude`, quit, and confirm the stage list prints and `keyboard_probe`
   reports well under 250 ms on a kitty-capable terminal (the automated run hit the timeout
   because its pty has no kitty support).
4. With a session running, confirm the sidebar shows the static status codes
   (`?` `B` `✓` `✗` `-` `A` `!`) and that nothing animates while you sit idle. Leave it idle
   a minute and confirm CPU stays near zero for baude and the terminal.
5. Confirm the legend row appears above the usage footer at 21+ rows and disappears at 20.
6. Open the shell pane with `ctrl+\`, press `alt+↑` and `alt+↓`, and confirm focus moves to
   the agent pane and the shell pane respectively (directional, not a toggle).
7. With the shell pane focused, switch to another session and back, and confirm focus
   returns to the shell pane. Then set `idle_child_policy = "suspend"`, let a session
   auto-archive, and confirm the row reads `suspended` and the child shows state `T` in
   `ps`, then resumes when you attach or type.

## Observation for follow-up (not a blocker)

In the isolated harness, `first_frame` measured 1.7 to 3.7 seconds in a release build with
zero saved sessions, while `workspace_resolution` measured about 230 ms. The render loop
draws before it polls for events, so the first paint is not waiting on input. The remaining
cost is either inside the first `app.tick()` or an artifact of the `script`-allocated pty in
the harness; separating those needs a measurement in a real terminal (checklist item 3).
Worth a follow-up issue against the next milestone rather than holding the release.
