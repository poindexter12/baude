# Phase 8: Test Isolation and Fixture Ownership - Context

**Gathered:** 2026-09-13
**Status:** Ready for planning

<domain>
## Phase Boundary

Running the suite cannot read or write the developer's real config, state, or
`~/.claude`, and suspected historical leaks can be inspected before anyone
deletes anything.

In scope: the remainder of TISO-01 (config redirect incl. `bauded` push/VAPID),
TISO-02 (per-fixture workspace identity), TISO-03 (escape guard scope and
arming), and all of TISO-04 (leak preview/removal tooling).

Out of scope: hook seeding safety (Phase 9), terminal link/input work (Phases
10-11), and the release gate (Phase 12).

</domain>

<decisions>
## Implementation Decisions

### Redirect Mechanism
- The new config redirect is an **RAII guard type** that resets on drop. The two
  existing redirects are bare `thread_local!` + setter with no reset
  (`git.rs:1727`, `hook.rs:91`), which is why arming leaks across tests within a
  binary.
- **Refactor `bauded/src/push.rs` to call `persist::config_dir()`** before adding
  the redirect. It currently duplicates `config_base()` at `push.rs:27-33`, so a
  baude-core-only hook would not cover VAPID keys (`push.rs:24`) or subscriptions
  (`push.rs:189`).
- `meta::claude_config_dir()` (`meta.rs:24`) is isolated by **thread-local
  redirect**, consistent with the guard above. This avoids reshaping
  `ClaudeMeta::poll`'s two call sites (`meta.rs:217`, `meta.rs:261`), which have
  no `_at` variant today.
- **Unify all four redirects under one guard struct** — worktrees base, hook
  command, config dir, claude config dir. One place to arm, one place to reset.
  This is what fixes the per-binary arming gap in Success Criterion 3.

### Workspace Identity
- Replace the process-wide `ACTIVE: OnceLock<Workspace>` (`workspace.rs:199`)
  with a **thread-local override checked first, `OnceLock` retained as the
  production fallback**. Same pattern as the redirect guard.
- **Inject config into `initialize()`** as a parameter. Production passes
  `load_config()`; fixtures pass a literal. This removes the real-config read
  from the identity path entirely, rather than merely redirecting it.
- **`active()` keeps its current signature** (`workspace.rs:221`). The resolution
  change is internal, so there is no call-site churn and no risk to the path
  composition at `git.rs:1778` (`managed_default_worktree_path`).
- A test that reaches `active()` with no override set **panics via the escape
  guard**. Silent fallback to config-derived identity is exactly what pins the
  process today.

### Escape Guard
- The guard is **armed unconditionally under `#[cfg(test)]`**, not as a side
  effect of `set_worktrees_base_for_test` (`git.rs:1747`). Every test binary is
  armed from the first instruction, independent of fixture order — this is the
  named defect in Success Criterion 3 (`baude-core`'s own tests never arm it).
- **Panic on escape**, as today (`git.rs:1753`). A test that reaches the real
  data dir has already failed; a `Result` would let call sites swallow it.
- The guard covers **all five paths**: worktrees base, config dir, claude config
  dir, state dir, and the push/VAPID store.
- **`#[cfg(test)]`-only compilation.** Zero production cost, and the real data
  dir is the correct target in production.

### Leak Preview and Removal Tooling (TISO-04)
- **CLI subcommand** (`baude worktrees scan` / `--prune`). Scriptable, works
  headless, no TUI state to thread through.
- **Ownership is proven by strict path-shape match** under the real worktrees
  base: `<base>/<workspace>/repository-<u64>/…` exactly as composed at
  `git.rs:1776`. Workspace state has zero record of the leaked directories, so
  ownership cannot be sourced from state.
- **Two-step approval**: `scan` prints; `--prune` requires an explicit `--yes`
  and re-verifies ownership at removal time.
- **Preview only by default, never delete.** Matches the PROJECT.md constraint:
  "never automatically delete real user worktrees; cleanup requires preview and
  manual approval."

### Claude's Discretion
- The re-exec'd real-git dogfood child (`app.rs:7643`) runs as a spawned process,
  so `#[cfg(test)]`-only arming does not reach it. It already receives
  `BAUDE_TEST_FIXTURE_ROOT`. Planning should confirm that is sufficient; if it is
  not, adding a runtime flag is an acceptable deviation to raise rather than a
  settled decision.
- Whether the ~12-line fixture preamble copy-pasted 10 times in
  `bauded/src/manager.rs` (`:2615, 2698, 2729, 2811, 2843, 2933, 3049, 3136,
  3215, 3521`) is extracted into a shared helper as part of this phase, or left
  for a follow-up. Extraction is favoured — it is the natural consumer of the new
  unified guard.
- Exact CLI noun/verb naming for the TISO-04 subcommand.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `WORKTREES_BASE_OVERRIDE` thread-local + `set_worktrees_base_for_test`
  (`git.rs:1727-1748`) — the template the unified guard generalises.
- `HOOK_COMMAND_OVERRIDE` + setter (`hook.rs:91-111`) — second instance of the
  same pattern.
- `persist::load_for_workspace_strict_at` / `save_current_at` / `load_named_at`
  (`persist.rs:316-352`) already take an explicit root; isolation today is by
  parameter threading.
- `persist::release_state_lock_for_test` (`persist.rs:553`) — cleanup escape
  hatch for reusing a fixture root.
- `parse_worktree_porcelain` (`git.rs:211`) and `git worktree list --porcelain`
  (`git.rs:329`) — per-repository enumeration the scan can reuse.
- `remove_verified_worktree*` / `verified_remove_arguments` (`git.rs:2566`,
  `git.rs:2673`) plus `inspect_removal` / `RemovalBlocker` / `RemovalSafety` —
  existing safety analysis for the `--prune` path.
- `GitFixture` with `Drop` cleanup (`git.rs:2709-2790`, `git.rs:2904`);
  `admission_repo` / `admission_repo_cloned` (`app.rs:5551`, `app.rs:5595`);
  `initialized_repo` (`api.rs:710`).

### Established Patterns
- Test isolation is thread-local rather than env mutation, deliberately, because
  cases run in parallel (rationale recorded in commit `725c558`, PR #82).
- Path composition: `managed_default_worktree_path` (`git.rs:1776`) =
  `<base>/<workspace>/repository-<key>/primary-<key>`;
  `managed_branch_worktree_path` (`git.rs:1799`) adds a sanitized 48-char branch
  label via `sanitize` (`git.rs:1785`).
- Real roots resolve `XDG_*` → `dirs::home_dir()` → fallback
  (`persist.rs:840`, `git.rs:1760`).

### Integration Points
- `persist::config_base` / `config_dir` (`persist.rs:840-852`) and its wrappers
  (`persist.rs:947, 978, 994, 1000, 1004`, lock at `:540`).
- `meta::claude_config_dir` (`meta.rs:24`) and callers `meta.rs:217`,
  `meta.rs:261`.
- `bauded/src/push.rs:27` private `config_base()` copy.
- `workspace::initialize` / `active` (`workspace.rs:206-221`); readers include
  `git::managed_default_worktree_path` (`git.rs:1778`), `backend::active`, the
  TUI poll loop, and `Workspace::state_file` (`workspace.rs:63`).
- `App::new` reads the real config at `app.rs:719`.
- Escape assert in `worktrees_base()` (`git.rs:1753-1758`); containment test at
  `app.rs:5635`.

### Greenfield
- No code walks the `~/.local/share/baude/worktrees` tree. There is no
  directory-scan enumeration, no orphan detection, and no prune/GC. All of
  TISO-04 is new.

</code_context>

<specifics>
## Specific Ideas

Live evidence gathered on the development machine 2026-09-13, which the TISO-04
tooling should be validated against:

- **1433** orphaned `repository-N` directories under
  `~/.local/share/baude/worktrees`, totalling **1.2 GB**
  (opencode 1263, prerelease 166, claude 4).
- **0 of 1433** contain a `.git` entry at any level.
- **1286 of 1433** are completely empty directories.
- **0** references to any `repository-N` across all 13 config/state JSON files —
  workspace state has no record of them.
- `git worktree list` reports exactly 1 live worktree (the main checkout).
- Date range 2026-08-30 to 2026-09-13; re-accumulated from ~3 after the manual
  2026-09-13 cleanup, confirming the leak is ongoing rather than historical.

This dataset is the reason ownership must not key on a missing gitdir: every one
of the 1433 has a missing gitdir, so that signal alone would authorize deleting
all of them — precisely what TISO-04 forbids.

Additional real-config contamination observed, evidencing the TISO-01 gap:
- `~/.config/baude/daemon-vapid.json` — real VAPID keypair written by tests.
- Five orphaned `.state-*.json.tmp-<pid>-<n>` files (up to 82 KB) from PIDs
  3640, 8840, 79656, 92861, 71604 — interrupted atomic writes that were never
  cleaned up.

</specifics>

<deferred>
## Deferred Ideas

- Actually deleting the 1433 leaked directories. The tooling built in this phase
  produces the preview; the deletion is a separate, explicitly approved
  operation and is not part of phase execution.
- Cleaning the orphaned `.state-*.json.tmp-*` files in the real config dir, and
  any hardening of the atomic-write cleanup path that produced them.
- Rotating the VAPID keypair that tests wrote into the real config dir.

</deferred>
