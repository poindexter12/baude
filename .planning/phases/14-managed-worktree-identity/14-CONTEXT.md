# Phase 14: Managed Worktree Identity - Context

**Gathered:** 2026-09-20
**Status:** Ready for planning
**Mode:** Smart discuss, autonomous (`--auto`): recommended answers accepted and logged

<domain>
## Phase Boundary

Managed worktrees carry a stable repository identity so two repositories can never resolve to the same managed path (across TUI and daemon state files and after a state reset), existing checkouts are recognized in place with nothing moved or deleted, a detected collision names the owning repository and offers a non-destructive resolution instead of a hard error, and `baude worktrees scan` reports the owning repository per checkout. Requirements: WTID-01..04. Out of this phase: startup performance (Phase 15), pane focus (Phase 16), any change to where the worktree base directory lives.

</domain>

<decisions>
## Implementation Decisions

### Identity scheme
- New repository directories under `~/.local/share/baude/worktrees/<workspace>/` are keyed by a stable digest of the repository's canonical git common dir: `repository-<12 lowercase hex of sha256(canonical common dir)>`. The `repository-` prefix and the `<role>-<key>` checkout segment shape are kept so `worktree_scan` classification keeps working.
- Identity is recorded twice: a small marker file inside each managed repository directory (canonical common dir, scheme version, recorded-at ms) and the existing repository entry in the workspace state file. The marker is the source of truth for ownership when state is missing or reset.
- TUI and daemon derive the key with the same baude-core function, so their paths agree without coordination or a shared counter file.
- After a state file reset the key is recomputed from the common dir and the same directory is found again; no scan is required to recover.

### In-place migration
- Existing `repository-<counter>` directories are recognized in place: the repository's canonical common dir is read from any checkout inside the directory (or from state) and matched; a repository keeps the directory it already has, and a marker is written into it on first admission or scan.
- New checkouts for an already-migrated repository land in that repository's existing directory; only repositories without a directory get a digest-keyed directory.
- Nothing is moved, renamed, re-cloned, or deleted by migration; it writes markers and state entries only. Legacy counter allocation is retired for new repositories but legacy directories stay readable forever.

### Collision handling
- Before creating or adopting a managed path, baude compares the target directory's marker (or the git common dir of a checkout inside it) with the repository being admitted.
- A mismatch is reported as: the path, the owning repository's canonical common dir and display name, and the repository that wanted it. No hard `PathCollision` exit.
- Resolution is non-destructive: the newcomer is allocated a distinct digest-keyed directory and admission continues; the owner's directory is never touched. If the marker is missing and no checkout can prove ownership, the directory is treated as unknown-owner and left alone.
- Collisions surface in the TUI status line and startup notes, and in the daemon log and `/info` payload.

### Scan and tests
- `baude worktrees scan` gains an owner column per managed checkout (repository canonical common dir and display name, from the marker or the checkout's git), flags collisions and unknown owners, and includes an `owner` object in `--json`. Prune safety is unchanged: preview-only by default, saved report plus two opt-ins to remove anything.
- The pinned path-composition test (`workspace.rs` `managed_worktree_path_composition_is_unchanged`) is updated deliberately with a documented behavior change and a companion test that pins the legacy shape as still recognized.
- Tests run under TestRedirect fixtures: digest stability for the same common dir, TUI vs daemon parity, state-reset recovery, legacy counter-dir migration in place, collision detection and report, scan owner output. No test touches the real `~/.config/baude` or `~/.local/share/baude`.

### Claude's Discretion
- Marker file name and JSON shape; digest length beyond 12 hex if collisions among digests must be handled; exact wording of the collision report and scan columns.
- Whether the daemon writes markers itself or defers to the TUI when both are active on one workspace, as long as the write is idempotent and atomic.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `baude-core/src/git.rs:1748-1819` — `worktrees_base()`, `managed_default_worktree_path` (`repository-<key>/primary-<key>`), `managed_branch_worktree_path` (sanitized branch label), `sanitize`; `git.rs:330-353` repository identity = canonicalized `git rev-parse --git-common-dir`; `git.rs:1077-1081` the current hard `EnsureDefaultWorktreeError::PathCollision`.
- `baude-core/src/repository.rs:481-690` — `next_repository_key` (starts at 1), `allocate_repository_key`, `allocate_checkout_key`; `baude/src/app.rs:1914-1946` `admit_repository` key lookup by `observed_common_dir`; `baude-core/src/lifecycle.rs:1307-1365` `ensure_repository`, `prepare_activation`.
- `baude-core/src/worktree_scan.rs` — read-only scan and fail-closed three-way verdict over `repository-<u64>` candidates; `repository_key_from_name` (~1006-1013); cross-workspace state reads (~439-450); `baude worktrees scan [--json] [--prune ...]` in `baude/src/main.rs:547-556`, `1000-1041`.
- `bauded/src/manager.rs:36` `STATE_BASE = "daemon-state"`; the daemon keeps its own key counter today, which is the root cause of the collision.
- Phase 13 added `baude-core/src/launch.rs` (`start_workspace`) — the natural place to run marker adoption at startup if needed.
- Tests to extend: `workspace.rs:528` path-composition pin, `worktree_scan.rs:2471` key-scoped-to-workspace, `git.rs:3414` unregistered-path collision, `app.rs:8348-8482` admission tests.

### Established Patterns
- Repository identity is the canonical git common dir, never a path string or remote URL.
- Destructive tooling is fail-closed: preview first, saved report, two opt-ins (v2.2 leak scan).
- Fixture realism: every path resolves through `TestRedirect`; an escape guard aborts tests reaching the real home.

### Integration Points
- Path composition in `git.rs`; key allocation in `repository.rs`; admission in `app.rs` and `lifecycle.rs`; daemon admission in `bauded/src/manager.rs`; scan output in `worktree_scan.rs` and `baude/src/main.rs`; README Worktrees section (`README.md:316-325`).

</code_context>

<specifics>
## Specific Ideas

- Joe's live tree: `worktrees/claude` 4 checkouts, `worktrees/opencode` 132, `worktrees/prerelease` 15, `worktrees/smoke` 1; this session runs from `worktrees/smoke/repository-1/primary-3`. Migration must leave all of them in place.
- The `smoke/repository-1` directory holds the baude repo itself; a second repo admitted into the `smoke` workspace by the daemon would collide under the current counters. That is the scenario WTID-01 closes.

</specifics>

<deferred>
## Deferred Ideas

- Configurable worktree base directory (out of scope for v2.3).
- Automatic pruning of orphaned managed directories (stays behind the preview-and-opt-in flow).

</deferred>

## Revision Dispositions (Iteration 1)

| Issue | Dimension | Severity | Fix Applied | Disposition |
|-------|-----------|----------|-------------|-------------|
| All plans lack "Artifacts this phase produces" | task_completeness | blocker | Added section to 14-01, 14-02, 14-03 listing new files, functions, types, updated signatures per plan | RESOLVED |
| 14-02 Task 1 done criteria contradicts itself (cargo build succeeds + compiler errors exist) | task_completeness | blocker | Updated done criteria to state `cargo build --workspace` and `cargo test --workspace` succeed with no errors; removed compiler-error parenthetical | RESOLVED |
| 14-02 Task 1 action contradicts itself (says "update all" then "fixed in later tasks") | task_completeness | blocker | Removed "will be updated in later tasks" sentence; added explicit call-site list (grep sources); confirmed all sites updated in this task via artifacts list | RESOLVED |
| 14-02 Tasks 1,2,3 TDD tasks lack "Commits:" sections | task_completeness | warning | Added explicit commit shapes to all three tasks: 1. test(14-02): RED, 2. feat(14-02): GREEN; behavior changes never ride in test() commits | RESOLVED |
