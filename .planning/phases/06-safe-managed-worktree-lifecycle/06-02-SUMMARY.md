---
phase: 06-safe-managed-worktree-lifecycle
plan: 02
subsystem: repository-lifecycle
tags: [rust, git-worktree, tdd, collision-safety, transactional-rollback]

requires:
  - phase: 06-01
    provides: Verified branch activation, durable lifecycle transitions, and repository reservations
provides:
  - Durable-key managed paths with bounded non-authoritative branch labels
  - Fresh inventory and filesystem collision refusal immediately before non-force Git add
  - Commit-aware App and Manager compensation for pre-replacement persistence failures
  - Retryable durable children without duplicate runtimes after committed-save or spawn failures
affects: [06-03, 06-04, 06-05, 06-06, phase-07-hierarchy]

tech-stack:
  added: []
  patterns: [fresh-topology-before-mutation, verified-plain-git-compensation, commit-aware-owner-state]

key-files:
  created: []
  modified: [baude-core/src/git.rs, baude-core/src/lifecycle.rs, baude/src/app.rs, bauded/src/manager.rs]

key-decisions:
  - "Managed branch labels are bounded display components only; RepositoryKey plus CheckoutKey supply stable filesystem identity."
  - "A pre-replacement save failure may compensate only a worktree added by that activation, using plain Git removal and branch/path/inventory postconditions."
  - "After replacement commitment or runtime spawn failure, memory follows durable active intent and retains one child for retry without a runtime-map entry."

patterns-established:
  - "Creation revalidates exact branch class and complete repository inventory after reservation and immediately before Git add."
  - "Lifecycle failures name the persistence, spawn, or compensation stage instead of claiming Git and JSON form one transaction."

requirements-completed: [WORK-02]

duration: 12min
completed: 2026-08-30
---

# Phase 6 Plan 2: Collision-Safe Creation and Commit-Aware Rollback Summary

**Managed worktree activation now rejects every unsafe ref/path fact and compensates uncommitted Git additions without deleting branches or creating duplicate runtimes**

## Performance

- **Duration:** 12 min
- **Started:** 2026-08-30T20:32:18Z
- **Completed:** 2026-08-30T20:44:09Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments

- Added a real-Git creation-safety matrix for malformed literals, file/directory/symlink collisions, missing registered paths, sanitizer collisions, Unicode bounds, malformed inventory, and forbidden argv.
- Revalidated topology and exact branch classification at the final pre-add decision point while keeping durable keys authoritative for managed paths.
- Removed the unreachable speculative legacy creation helper that reused directories and retried branch creation from caller context.
- Added verified plain-Git compensation for pre-replacement save failures, retaining the local branch and restoring prior durable memory only after path and inventory postconditions pass.
- Proved App and Manager retain committed active intent without a runtime after directory-sync or spawn failure, while same-branch retries converge and different branches remain distinct.

## Task Commits

Both tasks followed RED then GREEN TDD:

1. **Task 1 RED: creation safety matrix** - `b43b426` (test)
2. **Task 1 GREEN: collision-safe managed creation** - `838b6d0` (feat)
3. **Task 2 RED: owner rollback contracts** - `3d95fc2` (test)
4. **Task 2 GREEN: commit-aware compensation and retry state** - `39ec427` (feat)

## Files Created/Modified

- `baude-core/src/git.rs` - Exact lone-`@` refusal, fail-closed path inspection, fresh pre-add inventory checks, bounded durable path allocation, and creation-safety tests.
- `baude-core/src/lifecycle.rs` - Typed creation failure stages and branch-preserving verified compensation for uncommitted activation additions.
- `baude/src/app.rs` - Commit-stage-aware persistence, compensation, retry-child semantics, and local failure/race vectors.
- `bauded/src/manager.rs` - Matching compensation and committed-state behavior with deterministic save/spawn failure seams.

## Decisions Made

- Treat lone `@` as invalid lifecycle input even though `git check-ref-format --branch` accepts the porcelain spelling, because it cannot form the exact durable `refs/heads/@` identity required by the lifecycle.
- Canonicalize the candidate through its existing parent before comparing with inventory, so symlinked platform path prefixes cannot hide a registered missing path.
- Compensate only `Created` or `Activated` managed additions; occupied external reuse never authorizes Git removal.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Reconciled SDK-generated progress metadata**
- **Found during:** Plan metadata update
- **Issue:** `state.update-progress` projected 0% and zero completed phases despite five of nine v2.0 plans and Phase 5 being complete; the decision helper also duplicated one phase prefix.
- **Fix:** Reconciled header milestone progress, current-phase progress, velocity totals, and the duplicated decision against summaries and ROADMAP counts.
- **Files modified:** `.planning/STATE.md`
- **Verification:** STATE points to Plan 3 of 6, records 5/9 completed plans and 2/6 Phase 6 plans, and matches ROADMAP.
- **Committed in:** plan metadata commit

---

**Total deviations:** 1 auto-fixed (1 Rule 3 blocking issue)
**Impact on plan:** Documentation-only reconciliation; product scope and implementation are unchanged.

## Issues Encountered

- Git reports lone `@` as valid under `check-ref-format --branch`; the lifecycle adds the stricter exact-ref refusal required by WORK-02.
- Missing linked-worktree paths can retain a canonical inventory spelling different from the original temp-path alias; candidate comparison now resolves the existing parent before occupancy checks.

## User Setup Required

None - no dependencies, manifests, external services, or configuration changes were added.

## TDD Gate Compliance

- RED `b43b426` failed on invalid literal and registered-path safety before GREEN `838b6d0` passed the six-case matrix.
- RED `3d95fc2` left linked worktrees after injected precommit failures before GREEN `39ec427` added verified compensation.
- No separate refactor commit was needed; formatting and Clippy remained clean after both GREEN gates.

## Verification

- `cargo test -p baude-core git::tests::lifecycle::creation_safety -- --nocapture` - 6 passed.
- `cargo test lifecycle_creation_rollback -- --nocapture` - 4 App/Manager stage vectors passed.
- `cargo test lifecycle_create_activate -- --nocapture` - App and Manager convergence/distinct-branch contracts passed.
- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` - 272 tests passed (23 baude, 177 baude-core, 72 bauded).
- No production caller or definition of legacy `git::create_worktree` remains.
- `Cargo.toml` and `Cargo.lock` are unchanged.

## Known Stubs

None.

## Self-Check: PASSED

- All four modified source files and this summary exist.
- RED/GREEN commits `b43b426`, `838b6d0`, `3d95fc2`, and `39ec427` are present in Git history.

## Next Phase Readiness

- Removal preflight can build on fail-closed inventory parsing and the branch-preserving plain-Git compensation pattern.
- Close/reopen plans can reuse typed commit stages and the proven durable-child/no-runtime retry state.
- No new endpoint, authentication path, dependency, schema, or unplanned trust-boundary surface was introduced.

---
*Phase: 06-safe-managed-worktree-lifecycle*
*Completed: 2026-08-30*
