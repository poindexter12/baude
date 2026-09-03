---
phase: 06-safe-managed-worktree-lifecycle
plan: 03
subsystem: repository-lifecycle
tags: [rust, git-worktree, porcelain-v2, submodules, tdd, fail-closed]

requires:
  - phase: 06-02
    provides: Collision-safe managed worktree creation and verified plain-Git compensation
provides:
  - Strict NUL-delimited porcelain-v2 removal status parsing with typed blockers
  - Fresh managed linked-worktree ownership and topology proof
  - Recursive submodule rejection and opaque verified removal targets
  - Plain non-force removal primitive restricted to verified targets
affects: [06-06, phase-07-hierarchy, phase-08-daemon-parity]

tech-stack:
  added: []
  patterns: [result-valued safety oracle, byte-safe machine parsing, opaque capability token, double topology observation]

key-files:
  created: []
  modified: [baude-core/src/git.rs]

key-decisions:
  - "Empty, valid porcelain-v2 output is the only status-clean observation; every malformed record or subprocess failure remains an InspectionError."
  - "Removal authorization requires persisted management claims to match fresh common-directory, canonical path, full-ref, linked, unlocked, and non-prunable Git facts."
  - "Any recursive submodule record blocks non-force removal, and only an opaque target captures the exact path, parent, branch ref, and branch OID for later mutation."

patterns-established:
  - "Removal inspection parses paths as bytes while reserving UTF-8 text for Git-owned refs and object IDs."
  - "Safety-sensitive mutation APIs accept an unforgeable verified target rather than an arbitrary path."

requirements-completed: [WORK-06]

duration: 11min
completed: 2026-08-30
---

# Phase 6 Plan 3: Fail-Closed Removal Preflight Summary

**Managed worktree removal now requires conclusive porcelain-v2, recursive-submodule, ownership, topology, branch, and OID proof before an opaque removal target can exist**

## Performance

- **Duration:** 11 min
- **Started:** 2026-08-30T20:47:18Z
- **Completed:** 2026-08-30T20:58:28Z
- **Tasks:** 2
- **Files modified:** 1 source file

## Accomplishments

- Replaced error-to-clean status behavior with typed inspection errors and explicit staged, unstaged, conflict, untracked, ignored, and submodule blockers.
- Added strict byte-safe parsing for documented porcelain-v2 NUL records, including rename pairs and unmerged records.
- Proved persisted baude management against fresh linked-worktree identity, path, full branch ref, lock/prunable state, and branch OID.
- Added recursive submodule parsing where every valid row blocks and malformed, failed, or unstartable commands remain indeterminate.
- Restricted the new plain non-force removal primitive to an opaque verified target.

## Task Commits

Both tasks followed RED then GREEN TDD:

1. **Task 1 RED: removal status contracts** - `df49636` (test)
2. **Task 1 GREEN: fail-closed status parser** - `9f5d660` (feat)
3. **Task 2 RED: topology and submodule contracts** - `72b537c` (test)
4. **Task 2 GREEN: verified removal topology** - `5b7cf76` (feat)

## Files Created/Modified

- `baude-core/src/git.rs` - Typed removal blockers/errors, strict status and submodule parsers, exact ownership preflight, opaque verified target, plain verified remove primitive, and real-Git/failure tests.

## Decisions Made

- Kept pathname authority as raw bytes throughout status parsing so spaces, newlines, and platform-valid unusual names cannot change record boundaries.
- Required both the exact full branch ref and its full commit OID to match checkout `HEAD` before constructing a safe target.
- Repeated topology discovery after status, submodule, and OID inspection; a changed inventory blocks rather than reusing stale proof.
- Kept the legacy boolean helper temporarily fail-closed for the existing App caller; Plan 06-06 owns replacing that out-of-scope orchestration with two result-valued preflights.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Made the legacy compatibility status check fail closed**
- **Found during:** Task 1 GREEN
- **Issue:** The existing helper converted Git inspection failures to clean with `.unwrap_or(false)`.
- **Fix:** Routed it through the strict parser and mapped every inspection error to blocked while the Plan 06-06 caller migration remains pending.
- **Files modified:** `baude-core/src/git.rs`
- **Verification:** Focused command-failure tests pass and production search finds no `.unwrap_or(false)` in `git.rs`.
- **Committed in:** `9f5d660`

**2. [Rule 3 - Blocking] Used filesystem-portable unusual-name coverage**
- **Found during:** Task 1 GREEN
- **Issue:** macOS rejected a deliberately invalid UTF-8 filename before Git could observe it.
- **Fix:** Kept invalid metadata bytes in parser refusal tests and used a newline/space pathname for the real-Git NUL-boundary test.
- **Files modified:** `baude-core/src/git.rs`
- **Verification:** The real-Git unusual-name test preserves exact path bytes and passes.
- **Committed in:** `9f5d660`

**3. [Rule 3 - Blocking] Corrected fixture branch and worktree-lock command forms**
- **Found during:** Task 2 GREEN
- **Issue:** Human-readable fixture labels contained spaces, and Git 2.50 requires an explicit worktree argument for lock/unlock.
- **Fix:** Sanitized test branch labels and passed the exact linked path to lock/unlock.
- **Files modified:** `baude-core/src/git.rs`
- **Verification:** All four topology/submodule tests and the 20-test core lifecycle filter pass.
- **Committed in:** `5b7cf76`

**4. [Rule 3 - Blocking] Reconciled SDK-generated progress metadata**
- **Found during:** Plan metadata update
- **Issue:** `state.update-progress` projected zero completed phases and 0% despite seven of nine v2.0 plan summaries existing; concurrent 06-04 completion also made the current plan position stale.
- **Fix:** Reconciled milestone progress, Phase 6's 4/6 summary count, current Plan 5 position, and duration totals against summaries and ROADMAP.
- **Files modified:** `.planning/STATE.md`
- **Verification:** STATE and ROADMAP now agree on seven completed v2.0 plans and four completed Phase 6 plans.
- **Committed in:** plan metadata commit

---

**Total deviations:** 4 auto-fixed (1 Rule 1 bug, 3 Rule 3 blocking issues)
**Impact on plan:** Fixes preserved fail-closed behavior and platform-portable real-Git coverage without expanding source scope.

## Issues Encountered

- Context7 CLI was unavailable; version-specific behavior followed the phase's official Git documentation citations and local Git 2.50 real-command tests.
- An unrelated daemon API atomic-persistence expectation initially failed while concurrent 06-04 was still completing. Its owner committed `96cc2d4`; the exact test and the complete workspace gate then passed without any 06-03 scope change.
- The production App still names the legacy `is_dirty` compatibility helper. It now fails closed, while Plan 06-06 remains responsible for moving inspection before runtime teardown and removing the boolean seam.

## User Setup Required

None - no dependencies, manifests, external services, or configuration changes were added.

## TDD Gate Compliance

- RED `df49636` precedes GREEN `9f5d660` for Task 1.
- RED `72b537c` precedes GREEN `5b7cf76` for Task 2.
- Interleaved 06-04 commits changed only that concurrent plan's files; every 06-03 commit contains only `baude-core/src/git.rs`.

## Verification

- `cargo test -p baude-core git::tests::lifecycle::removal_preflight -- --nocapture` - 5 passed.
- `cargo test -p baude-core git::tests::lifecycle::removal_topology -- --nocapture` - 4 passed.
- `cargo test -p baude-core git::tests::lifecycle -- --nocapture` - 20 passed.
- `cargo clippy -p baude-core --all-targets -- -D warnings` - passed.
- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` - 286 tests passed (25 baude, 188 baude-core, 73 bauded).
- `Cargo.toml` and `Cargo.lock` are unchanged.

## Known Stubs

None.

## Deferred Issues

- See `deferred-items.md` for the unrelated daemon API test failure and the Plan 06-06-owned App caller migration.

## Self-Check: PASSED

- `baude-core/src/git.rs` and this summary exist.
- RED/GREEN commits `df49636`, `9f5d660`, `72b537c`, and `5b7cf76` are present in Git history.

## Next Phase Readiness

- Plan 06-06 can consume `inspect_removal` twice around runtime stop and pass only `VerifiedRemovalTarget` to plain Git removal.
- Blocker variants retain exact byte paths for presentation without granting path authority.
- No new endpoint, authentication path, dependency, schema, or unplanned trust-boundary surface was introduced.

---
*Phase: 06-safe-managed-worktree-lifecycle*
*Completed: 2026-08-30*
