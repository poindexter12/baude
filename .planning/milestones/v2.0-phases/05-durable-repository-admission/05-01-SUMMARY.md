---
phase: 05-durable-repository-admission
plan: 01
subsystem: git
tags: [rust, git-worktree, repository-identity, tdd, byte-safe]

# Dependency graph
requires: []
provides:
  - Canonical repository snapshots from common-directory and main-first worktree inventory
  - Offline verified default-branch resolution with actionable unavailable states
  - Inventory-first default-worktree reuse or verified managed creation
  - Isolated real-Git fixture for repository admission tests
affects: [05-02, 05-03, repository-admission, persistence-reconciliation]

# Tech tracking
tech-stack:
  added: []
  patterns: [argv-only Git commands, NUL-delimited byte parsing, typed fail-closed errors, rediscovery after mutation]

key-files:
  created: []
  modified: [baude-core/src/git.rs]

key-decisions:
  - "Repository identity is the canonical common directory plus Git's main-first worktree inventory; show-toplevel is used only to select and verify an inventory member."
  - "Default resolution reads the main worktree branch's upstream remote before deduplicated origin fallback and accepts only exact, commit-verified remote symbolic HEAD targets."
  - "Managed creation checks out a verified local branch name or creates it from the exact verified remote source, then rediscovery proves path, common directory, and full branch ref."

patterns-established:
  - "Topology output remains bytes through NUL parsing; only refs and diagnostics are decoded as text."
  - "Git mutations succeed only after inventory checks and are followed by authoritative rediscovery."

requirements-completed: [REPO-01, REPO-04, REPO-06]

# Metrics
duration: 10min
completed: 2026-08-30
---

# Phase 5 Plan 1: Durable Git Repository Admission Summary

**Byte-safe canonical repository discovery with offline remote-HEAD resolution and non-destructive, inventory-verified default-worktree reuse or creation**

## Performance

- **Duration:** 10 min
- **Started:** 2026-08-30T17:25:43Z
- **Completed:** 2026-08-30T17:35:12Z
- **Tasks:** 2
- **Files modified:** 1

## Accomplishments

- Main checkouts, nested paths, symlinks, and linked worktrees converge on one canonical common-directory identity while same-basename repositories remain distinct.
- NUL-delimited porcelain parsing preserves spaces and newlines in paths and rejects malformed or partial topology.
- Default resolution is local-only, preserves slash branch names, prefers the main branch's upstream remote, and returns typed detached, unborn, missing, malformed, dangling, or unsupported states.
- Default checkout ensure reuses main or linked inventory records before creating a separate managed worktree and proves the result through rediscovery without changing the main checkout.

## Task Commits

Each task followed mandatory RED/GREEN TDD commits:

1. **Task 1 RED: canonical repository discovery tests** - `3c030bc` (test)
2. **Task 1 GREEN: canonical repository discovery** - `1575b2f` (feat)
3. **Task 2 RED: offline default and worktree tests** - `5fcaad4` (test)
4. **Task 2 GREEN: default resolution and worktree ensure** - `6446f89` (feat)

## Files Created/Modified

- `baude-core/src/git.rs` - Adds public admission types and APIs, typed errors, byte-safe Git boundaries, and isolated real-Git test matrices.

## Decisions Made

- Kept legacy `repo_root`, `create_worktree`, clone, dirty-check, and removal APIs source-compatible while making the new admission functions authoritative for Phase 5.
- Canonicalized only existing inputs and existing inventory paths; malformed or missing topology fails closed rather than inferring `.git` layout.
- Used full refs for verification and comparison, but the verified slash-preserving local branch name for `git worktree add` so Git attaches the checkout rather than detaching it.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Prevented full local ref from creating a detached worktree**
- **Found during:** Task 2 GREEN worktree creation matrix
- **Issue:** Passing `refs/heads/team/default` as the worktree add commit-ish produced a detached checkout on the tested Git version.
- **Fix:** Continue verifying and comparing the full ref, but pass the exact verified slash-preserving local branch name to `git worktree add`.
- **Files modified:** `baude-core/src/git.rs`
- **Verification:** `creates_verified_default_without_mutating_main_checkout` observes `refs/heads/team/default` and the complete worktree filter passes.
- **Committed in:** `6446f89`

**2. [Rule 1 - Bug] Removed duplicate branch creation from linked-worktree fixture**
- **Found during:** Task 2 GREEN linked-worktree reuse matrix
- **Issue:** The test created `team/default` before calling a fixture helper that also creates the requested branch.
- **Fix:** Let the fixture helper create the branch exactly once.
- **Files modified:** `baude-core/src/git.rs`
- **Verification:** `reuses_main_then_existing_linked_worktree` passes.
- **Committed in:** `6446f89`

**3. [Rule 3 - Blocking] Corrected Rust ownership and lint blockers**
- **Found during:** Task 2 GREEN compile and clippy gates
- **Issue:** A borrowed remote target was moved into the result, and Clippy rejected an unnecessary lazy `ok_or_else` closure.
- **Fix:** Own the stripped branch before moving the target and use `ok_or` for the selected-worktree error.
- **Files modified:** `baude-core/src/git.rs`
- **Verification:** `cargo clippy -p baude-core --all-targets -- -D warnings` passes.
- **Committed in:** `6446f89`

**4. [Rule 3 - Blocking] Ignored regenerated GSD machine-state projection**
- **Found during:** Plan metadata updates
- **Issue:** The state handler generated `.planning/state.json` as an untracked projection of canonical planning artifacts.
- **Fix:** Added the regeneratable projection to `.gitignore` while preserving unrelated untracked files.
- **Files modified:** `.gitignore`
- **Verification:** `git status --short` no longer reports `.planning/state.json`.
- **Committed in:** plan metadata commit

---

**Total deviations:** 4 auto-fixed (2 Rule 1 bugs, 2 Rule 3 blockers)
**Impact on plan:** All fixes were required for correct attached-worktree behavior and CI compliance; no scope was added.

## Issues Encountered

- The full workspace gate reaches the expected Wave 0 integration gap after concurrent Plan 05-02 changed persistence APIs: `baude/src/app.rs` and `bauded/src/manager.rs` remain on legacy signatures until dependent Plan 05-03 adapts them. This plan did not alter those consumers; its required `baude-core` Git format, clippy, and test gates pass.

## User Setup Required

None - no external service configuration or dependency installation required.

## TDD Gate Compliance

- RED commit `3c030bc` precedes GREEN commit `1575b2f` for Task 1.
- RED commit `5fcaad4` precedes GREEN commit `6446f89` for Task 2.

## Next Phase Readiness

- Plans 05-02 and 05-03 can consume `RepositorySnapshot`, `DefaultBranchUnavailable`, `DefaultWorktreeOutcome`, and the three admission entry points.
- No package, manifest, lockfile, endpoint, authentication path, or schema threat surface was added.

## Self-Check: PASSED

- `baude-core/src/git.rs` and this summary exist.
- RED/GREEN commits `3c030bc`, `1575b2f`, `5fcaad4`, and `6446f89` are present in Git history.

---
*Phase: 05-durable-repository-admission*
*Completed: 2026-08-30*
