---
phase: 18-shared-kill-then-remove-lifecycle
plan: 03
subsystem: repository
tags: [rust, repository, aggregate, validation, tdd]

# Dependency graph
requires:
  - phase: 06-shared-lifecycle-core-refactor
    provides: RepositoryState aggregate, SavedRepository, SavedCheckout, SavedStandaloneSession, validate and validate_for_save
provides:
  - RemovedRows { repositories, checkouts, standalone_sessions } in baude-core/src/repository.rs
  - RepositoryState::remove_rows removing any combination of repository, checkout and standalone rows by key in one order-preserving mutation, counters untouched, no validation inside
  - remove_standalone_session now delegates to remove_rows with its signature unchanged
  - test helpers repository_at and checkout_at that build a validating multi-repository topology
affects: [18-04, 18-05, 18-06, phase-20-tui, phase-21-daemon]

# Actuals (#2632) - estimateTokens scale, chars/4 over the realized diff
actuals:
  tokens: 2631
  tasks: 3
  commits: 3
plan_head_before: 3f9cfb7daba07dbb27314d18f16ac284e5348083
plan_head_after: 5e1e157e7558b490ba106183d9e273e6077aaeac

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Batch aggregate mutation through std::mem::take plus into_iter().partition so removed rows come back in state order with no index shifting"
    - "Mutation performs no validation; the caller validates the whole clone once with validate() or validate_for_save() before a save"

key-files:
  created: []
  modified:
    - baude-core/src/repository.rs

key-decisions:
  - "remove_standalone_session delegates to remove_rows so one mutation path serves every row kind; valid state forbids duplicate standalone keys so pop() of the partition equals the former position-based removal"
  - "Removal test seeds allocate every key and order through the aggregate's allocators and assert validate() Ok both in the shared fixture and in each test before remove_rows, so a fixture defect fails at the seed"

patterns-established:
  - "repository_at and checkout_at override identity, role, path, repo root and branch together, mirroring plan 01's session consistency rules so a seed can only fail validate() for the topology reason a test intends"

requirements-completed: [RMV-04, RMV-06]

coverage:
  - id: D1
    description: "remove_rows removes a repository parent and every checkout keyed to it in one call; afterwards no row carries that repository_key, validate() passes and all four counters keep their values"
    requirement: RMV-04
    verification:
      - kind: unit
        ref: "baude-core/src/repository.rs#remove_rows_repository_with_children_leaves_no_row_with_that_key_and_validates"
        status: pass
    human_judgment: false
  - id: D2
    description: "Removing only the parent while children remain leaves a state that validate() rejects as DanglingRepositoryKey, proving the partial set can never be saved"
    requirement: RMV-06
    verification:
      - kind: unit
        ref: "baude-core/src/repository.rs#remove_rows_parent_only_is_rejected_by_validate_as_dangling"
        status: pass
    human_judgment: false
  - id: D3
    description: "Unknown repository, checkout and standalone keys are inert: empty RemovedRows, state equal to its pre-call clone, counters unchanged"
    requirement: RMV-04
    verification:
      - kind: unit
        ref: "baude-core/src/repository.rs#remove_rows_unknown_keys_are_no_ops_and_counters_unchanged"
        status: pass
    human_judgment: false
  - id: D4
    description: "A standalone row is removed and returned in standalone_sessions with the remaining state still valid"
    requirement: RMV-04
    verification:
      - kind: unit
        ref: "baude-core/src/repository.rs#remove_rows_removes_standalone_and_returns_it"
        status: pass
    human_judgment: false

# Metrics
duration: 4 min
completed: 2026-11-03
status: complete
---

# Phase 18 Plan 03: Batch Row Removal on the RepositoryState Aggregate Summary

**`RepositoryState::remove_rows` removes repositories, checkouts and standalone sessions by key in one order-preserving partition and returns the removed rows as `RemovedRows`, leaving validation to the caller so plan 04 can clone, mutate, `validate_for_save()` and save the whole row set once**

## Performance

- **Duration:** 4 min
- **Started:** 2026-11-03T22:37:03Z
- **Completed:** 2026-11-03T22:41:41Z
- **Tasks:** 3 (RED, GREEN, REFACTOR applied)
- **Files modified:** 1

## Accomplishments
- `RemovedRows` and `remove_rows(&mut self, &[RepositoryKey], &[CheckoutKey], &[StandaloneKey]) -> RemovedRows` land directly after `remove_standalone_session`; each Vec is partitioned with `std::mem::take` plus `into_iter().partition`, so removed rows return in state order and the kept rows keep theirs
- The four `next_*` counters are never read or written inside the mutation (acceptance grep over the function body prints 0) and no validation runs inside; the doc comment names both `validate()` and `validate_for_save()` as the caller's gate and explains why the full parent-plus-children set must go in one call
- `remove_standalone_session` keeps its exact signature and `Option<SavedStandaloneSession>` return but now delegates to `remove_rows`, so there is one removal mutation for every row kind
- Test module gains `repository_at` and `checkout_at` helpers plus a `removal_fixture` that builds repository k (c1 `PrimaryDefault` at `/k-default`, c2 `ManagedBranch` at `/k-feature`), repository j (c3 `PrimaryDefault` at `/j-default`) and standalone s at `/notes`, with every key and order taken from the allocators; the fixture and each test assert `validate()` is `Ok(())` before the call

## TDD Gate Compliance

| Gate | Evidence |
| ---- | -------- |
| RED | Commit `44e27f5` (`test(18-03): add failing batch row removal tests`). `cargo test -p baude-core repository::tests::remove_rows -- --test-threads=1` exit 101, zero `error[E` lines, 3 failed / 1 passed. Each failure is the planned `assertion left == right failed` on the `RemovedRows` or post-state equality against the no-op stub; `remove_rows_unknown_keys_are_no_ops_and_counters_unchanged` passes against the stub by construction, as the plan requires. Semantic assessment: the target tests executed and failed on the intended assertions for the intended reason. |
| GREEN | Commit `fd2d180` (`feat(18-03): add batch row removal to RepositoryState`). Target: 4 passed, 0 failed. `repository::tests`: 7 passed, 0 failed. `cargo clippy -p baude-core --all-targets -- -D warnings` exit 0 on the first run. Only the function body changed between RED and GREEN. |
| REFACTOR | Commit `5e1e157` (`refactor(18-03): delegate standalone removal to remove_rows`). `repository::tests` 7 passed, `cargo fmt --check` exit 0, clippy exit 0. |

`gsd_run check tdd-red-evidence` returned `INVALID_RED` with reason `invalid_record`: the classifier has no adapter for `cargo test` text output, the same gap plans 18-01 and 18-02 recorded and tdd.md documents as the known Rust gap (#4379). The plan's own RED gate (exit non-zero, no `error[E`, `3 failed` and `1 passed`, dangling test named in the output) was applied instead and passed. The RED commit was amended once before GREEN existed to add an explicit per-test `assert_eq!(before.validate(), Ok(()))` so the seed-validation acceptance grep rose by 7 rather than 3; the RED result was re-run and unchanged (3 failed / 1 passed) before the amend.

## Task Commits

1. **Task 1 (RED): remove_rows tests, helpers, RemovedRows and a no-op stub** - `44e27f5` (test)
2. **Task 2 (GREEN): implement remove_rows** - `fd2d180` (feat)
3. **Task 3 (REFACTOR): delegate remove_standalone_session to remove_rows** - `5e1e157` (refactor)

**Plan metadata:** see the docs commit that lands this file

## Files Created/Modified
- `baude-core/src/repository.rs` - `RemovedRows` before `impl Default for RepositoryState`; `remove_rows` after `remove_standalone_session`; `remove_standalone_session` body now one delegation line; test helpers `repository_at`, `checkout_at`, `removal_fixture`, `counters` and the four `remove_rows_*` tests

## Verification

- `cargo test -p baude-core repository::tests -- --test-threads=1`: 7 passed, 0 failed
- Full `cargo test -p baude-core`: 489 passed, 0 failed, 1 ignored
- `git log --oneline -E --grep='^test\(18-0?3\):'` returns `44e27f5`; `--grep='^feat\(18-0?3\):'` returns `fd2d180`; `--grep='^refactor\(18-0?3\):'` returns `5e1e157`
- All Task 1, 2 and 3 acceptance greps pass: 4 test fns, 1 `pub fn remove_rows`, 2 helpers, seed-validation count 14 vs 7 at HEAD, 0 counter references in the body, doc comment mentions `validate` 4 times and `validate_for_save` once, 1 `pub fn remove_standalone_session`
- No file deletions in any of the three commits

## Decisions Made
- Applied the conditional Task 3 refactor: the only caller of `remove_standalone_session` is a test in the same module, and routing it through `remove_rows` leaves one mutation path. On any state that passes `validate()` standalone keys are unique, so `.pop()` on the partition returns the same row the former `position` lookup did
- `removal_fixture` returns a struct of keys rather than positional tuples so each test destructures only the keys it uses, and a `counters` helper packs the four `next_*` values for the before/after equality the plan's prohibition requires

## Deviations from Plan

None - plan executed exactly as written. No clippy-driven shape change was needed; the helpers and tests passed `-D warnings` on the first run with the `#[allow(clippy::too_many_arguments)]` the existing `SavedCheckout::new` already uses on `checkout_at`.

## Issues Encountered
None.

## Known Stubs
None. The RED-phase no-op body of `remove_rows` was replaced in GREEN.

## Threat Flags
None beyond the plan's threat model. T-18-08 is covered by D2 (dangling parent is rejected by `validate()`); T-18-SC is kept (no `Cargo.toml` change).

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Plan 04 can call `state.clone()`, `remove_rows(&repositories, &checkouts, &standalone)`, `validate_for_save()` and the single save, and use the returned `RemovedRows` for its report
- `remove_rows` is agnostic to the children-first ordering plan 01's `select_rows` produces: it removes every matching key regardless of the order the caller lists them in

---
*Phase: 18-shared-kill-then-remove-lifecycle*
*Completed: 2026-11-03*

## Self-Check: PASSED

- `baude-core/src/repository.rs` exists and carries `pub fn remove_rows`.
- Commits 44e27f5, fd2d180, 5e1e157 are ancestors of HEAD on `gsd/phase-18-shared-kill-then-remove-lifecycle`.
