---
phase: 06-safe-managed-worktree-lifecycle
plan: 01
subsystem: repository-lifecycle
tags: [rust, git-worktree, tdd, durable-state, runtime-dispatch]

requires:
  - phase: 05-01
    provides: Canonical Git discovery and verified default-branch resolution
  - phase: 05-02
    provides: Durable repository aggregate and atomic persistence
  - phase: 05-03
    provides: Save-before-spawn checkout-key runtime ownership
provides:
  - Literal local branch classification with remote-only rejection
  - Verified default-based new branch and exact existing-local worktree activation
  - Shared repository reservation and durable activation transition contracts
  - App and Manager adapters that persist before one checkout-key runtime spawn
affects: [06-02, 06-04, 06-05, phase-07-hierarchy, phase-08-daemon-parity]

tech-stack:
  added: []
  patterns: [argv-only Git activation, RAII repository reservations, shared durable transition, save-before-spawn]

key-files:
  created: [baude-core/src/lifecycle.rs]
  modified: [baude-core/src/git.rs, baude-core/src/lib.rs, baude/src/app.rs, bauded/src/manager.rs]

key-decisions:
  - "Branch text is accepted only when Git validates it without expansion, then exact refs/heads identity and fresh inventory determine new, local, or occupied behavior."
  - "Repository mutations use cloneable RAII reservations keyed by RepositoryKey; readable branch labels and paths never provide mutation identity."
  - "App and Manager consume one shared durable activation transition and associate the resulting runtime only by CheckoutKey after persistence succeeds."

patterns-established:
  - "Activation follows fresh discovery, literal classification, explicit non-force Git add or occupied reuse, postcondition verification, aggregate validation, durable save, then runtime focus/spawn."
  - "Externally occupied worktrees may be registered and opened but remain managed_by_baude=false."

requirements-completed: [WORK-01]

duration: 13min
completed: 2026-08-30
---

# Phase 6 Plan 1: Verified Managed Branch Activation Summary

**Literal branch requests now create from the verified default, activate unchanged local refs, or reuse occupied worktrees through one durable App/Manager lifecycle contract**

## Performance

- **Duration:** 13 min
- **Started:** 2026-08-30T20:15:26Z
- **Completed:** 2026-08-30T20:28:09Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments

- Added Git-owned literal branch validation that rejects previous-checkout expansion and remote-only requests before mutation.
- New branches start at the freshly commit-verified default local ref regardless of which repository child supplied context; existing local branches retain their OID.
- Occupied branches reuse fresh inventory records without a second `worktree add`, and external checkouts remain unmanaged.
- Added shared repository-key reservations and aggregate transitions used by both App and Manager.
- Both runtime owners validate and persist one active checkout before spawn, reuse existing checkout-key runtimes, and return typed busy outcomes.

## Task Commits

Each task followed RED/GREEN TDD, followed by one plan-level refactor:

1. **Task 1 RED: branch activation and reservation contracts** - `df2fb0d` (test)
2. **Task 1 GREEN: verified literal branch activation** - `ec5926e` (feat)
3. **Task 2 RED: App and Manager owner contracts** - `39ba743` (test)
4. **Task 2 GREEN: durable owner activation adapters** - `ea70706` (feat)
5. **REFACTOR: inspectable forbidden-form command contract** - `475f149` (refactor)

## Files Created/Modified

- `baude-core/src/lifecycle.rs` - UI-free activation requests, outcomes, durable transition recording, and release-on-drop repository reservations.
- `baude-core/src/git.rs` - Literal branch classification, explicit create/activate/reuse operations, stable managed paths, postcondition checks, and real-Git tests.
- `baude-core/src/lib.rs` - Exports the shared lifecycle module.
- `baude/src/app.rs` - Routes local branch input through durable shared activation and checkout-key runtime focus/spawn.
- `bauded/src/manager.rs` - Routes compatibility branch creation through the same transition without adding API surface.

## Decisions Made

- Kept the exact full ref authoritative for validation, occupancy, OID checks, and postconditions, but passed its already-validated literal short name to existing-branch `worktree add` so Git 2.50 creates an attached checkout rather than a detached one.
- Allocated managed paths from workspace, repository key, sanitized branch label, and checkout key; the checkout key supplies uniqueness while the label remains readable only.
- Preserved the existing flat daemon response while making its internal branch operation use the shared lifecycle outcome.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Preserved attached HEAD for existing local activation**
- **Found during:** Task 1 GREEN
- **Issue:** Passing `refs/heads/<name>` directly to `git worktree add` detaches HEAD on the verified Git 2.50 behavior already observed in Phase 5.
- **Fix:** Exact full-ref classification and commit verification remain authoritative, while the validated literal branch spelling is passed to the add command.
- **Files modified:** `baude-core/src/git.rs`
- **Verification:** The real-Git existing-local test proves symbolic branch identity and unchanged OID.
- **Committed in:** `ec5926e`

**2. [Rule 3 - Blocking] Isolated managed-path fixtures across repeated test processes**
- **Found during:** Task 2 GREEN verification
- **Issue:** Durable key `1` reused the same global managed test path after a prior fixture repository had been removed, correctly triggering collision refusal on rerun.
- **Fix:** Test aggregates seed process-scoped repository keys and remove linked fixtures through plain Git worktree removal.
- **Files modified:** `baude/src/app.rs`, `bauded/src/manager.rs`
- **Verification:** Focused owner tests and two complete workspace gates pass repeatedly.
- **Committed in:** `ea70706`

**3. [Rule 3 - Blocking] Normalized pre-phase state for SDK progress handlers**
- **Found during:** Plan metadata update
- **Issue:** `STATE.md` still represented the current plan as `Not started`, which the SDK could not parse, and its progress projection subsequently reported zero despite four of nine planned v2.0 plans being complete.
- **Fix:** Normalized the current plan to the parseable phase position before advancing, then reconciled milestone and current-phase progress with the roadmap and summary counts.
- **Files modified:** `.planning/STATE.md`
- **Verification:** State now points to Plan 2 of 6, records the 06-01 metric and decisions, and agrees with ROADMAP's 1/6 Phase 6 count.
- **Committed in:** plan metadata commit

---

**Total deviations:** 3 auto-fixed (1 Rule 1 bug, 2 Rule 3 blocking issues)
**Impact on plan:** Both fixes preserve the required attached-branch semantics and deterministic verification without expanding product scope.

## Issues Encountered

- Context7 CLI was unavailable, so implementation behavior was checked against the official Git worktree, check-ref-format, and show-ref documentation cited by phase research.

## User Setup Required

None - no dependencies, manifests, external services, or configuration changes were added.

## TDD Gate Compliance

- RED `df2fb0d` precedes GREEN `ec5926e` for Task 1.
- RED `39ba743` precedes GREEN `ea70706` for Task 2.
- Refactor `475f149` preserves all focused and workspace tests.

## Verification

- `cargo test -p baude-core git::tests::lifecycle::branch_activation -- --nocapture` - 5 passed.
- `cargo test lifecycle_create_activate -- --nocapture` - App and Manager contracts passed.
- `cargo test -p baude-core lifecycle::tests -- --nocapture` - 2 passed.
- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` - 262 tests passed (21 baude, 171 baude-core, 70 bauded).
- `Cargo.toml` and `Cargo.lock` are unchanged.
- No production caller remains on legacy `git::create_worktree`.

## Known Stubs

None.

## Self-Check: PASSED

- All five created/modified source files and this summary exist.
- RED/GREEN/refactor commits `df2fb0d`, `ec5926e`, `39ba743`, `ea70706`, and `475f149` are present in Git history.

## Next Phase Readiness

- Plan 06-02 can extend the shared reservation/path contract with collision, concurrency, and rollback failure matrices.
- Close, reopen, and removal plans can reuse `RepositoryReservations`, durable checkout identity, and shared lifecycle outcomes.
- No new endpoint, authentication, dependency, schema, or unplanned trust-boundary surface was introduced.

---
*Phase: 06-safe-managed-worktree-lifecycle*
*Completed: 2026-08-30*
