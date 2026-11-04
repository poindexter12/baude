---
phase: 19-re-admission-suppression
plan: 05
subsystem: tui-admission
tags: [rust, tui, admission, suppression, tdd]

# Dependency graph
requires:
  - phase: 19-re-admission-suppression
    provides: "19-02 suppression store: is_suppressed, lift, suppress, file_path under persist::config_dir(); 19-04 writes the entries this plan consults"
  - phase: 08-workspace-identity
    provides: "App::admit_repository and admit_existing_worktrees, the inventory sweep every launch inside a known repository runs"
provides:
  - "admit_existing_worktrees consults suppression::is_suppressed right after the owned check and skips a suppressed path; the sweep is otherwise unchanged"
  - "App::lift_suppression(&mut self, path: &Path): unit return, keyed on git::repo_root(path) with the path itself as fallback, set_message on an io::Error"
  - "admit_repository lifts after discover_repository and before the existing_key match, so every local route reaching it, LaunchDirectory, Open and CloneCompletion, lifts exactly its own path before either sweep site"
  - "admit_standalone lifts the canonical folder for symmetry"
affects: [19-06, 19-07, 21-daemon-removal]

# Actuals (#2632)
actuals:
  tokens: 4185
  tasks: 3
  commits: 2
plan_head_before: 3245a823faef0058644e7bdde7d7385eafe0bbc5
plan_head_after: f541c6c2e02a80ac69c11e3a77605fa390dfcbd5

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Advisory store consult inside an admission loop: one continue, fully qualified call, no error path"
    - "Lift-before-sweep: the lift sits upstream of the branch that reaches both admit_existing_worktrees calls, so no route needs its own lift code"

key-files:
  created: []
  modified:
    - baude/src/app.rs

key-decisions:
  - "The lift key is git::repo_root of the admitted path, so a subfolder launch inside a removed checkout lifts that checkout and the parent root never lifts a child"
  - "A failed lift leaves the entry in place and the sweep still skips it; the admission itself returns Ok with the repository's rows intact and the failure is a message, which is what D-09 asks for"
  - "Test fixtures canonicalise worktree paths up front so they compare equal to the paths the sweep records from git worktree list under the macOS temp symlink"

patterns-established:
  - "SuppressionFixture: a pushed admission_repo with two linked branch worktrees beside the managed root, one seeded in the store the way plan 04 writes it"

requirements-completed: [READM-03, READM-04]

coverage:
  - id: D1
    description: "With an entry for one linked worktree, admitting the root admits the primary and the other worktree but no row for the suppressed path, and the entry stays"
    requirement: READM-03
    verification:
      - kind: unit
        ref: "cargo test -p baude app::tests::suppressed_worktree_is_skipped_by_admission_sweep -- --test-threads=1"
        status: pass
    human_judgment: false
  - id: D2
    description: "admit_repository with the suppressed worktree's own path lifts the entry and admits it in the same pass with a CheckoutKey and first_seen_order above every pre-existing one, resume_id None, no owned runtime, lifecycle Inactive"
    requirement: READM-04
    verification:
      - kind: unit
        ref: "cargo test -p baude app::tests::admitting_suppressed_path_lifts_entry_and_admits_fresh_identity -- --test-threads=1"
        status: pass
    human_judgment: false
  - id: D3
    description: "Admitting the parent root, twice, and a subfolder of the parent, leaves the child's entry and keeps the child absent"
    requirement: READM-04
    verification:
      - kind: unit
        ref: "cargo test -p baude app::tests::admitting_parent_root_does_not_lift_child_entry -- --test-threads=1"
        status: pass
    human_judgment: false
  - id: D4
    description: "A second App launched from a subfolder inside the suppressed checkout lifts it through restore and LaunchDirectory; open_repo_session_via with CloneCompletion into a suppressed clone destination lifts that destination"
    requirement: READM-04
    verification:
      - kind: unit
        ref: "cargo test -p baude app::tests::launch_dir_and_clone_completion_routes_lift_suppression -- --test-threads=1"
        status: pass
    human_judgment: false
  - id: D5
    description: "With a directory at the merge lock path, admit_repository of the suppressed path returns Ok with a runtime, the row count is unchanged, the entry stays, and the last message names the lift failure"
    requirement: READM-03
    verification:
      - kind: unit
        ref: "cargo test -p baude app::tests::lift_failure_is_a_message_not_an_admission_error -- --test-threads=1"
        status: pass
    human_judgment: false

# Metrics
duration: 33min
completed: 2026-11-04
status: complete
---

# Phase 19 Plan 05: Suppression Consult and Lift on Admission Summary

**The TUI's worktree inventory sweep now skips any path with a suppression entry, and every explicit local admission of exactly that path, `n`, `c` or the launch dir, lifts the entry before the sweep so the fresh row comes back in the same pass with new identity and no carried runtime.**

## Performance

- **Duration:** about 33 min
- **Started:** 2026-11-04T20:22:26Z
- **Completed:** 2026-11-04T20:55:13Z
- **Tasks:** 3 (RED, GREEN, REFACTOR with no refactor needed)
- **Files modified:** 1

## Accomplishments

- `admit_existing_worktrees` gained one `continue` after the `owned` check: `suppression::is_suppressed(&persist::config_dir(), &record.path)`. The sweep can only lose a candidate here, never gain one (D-11).
- `App::lift_suppression(&mut self, path: &Path)` returns unit. It keys on `git::repo_root(path)` with the path as fallback, calls `suppression::lift`, and turns an `io::Error` into `set_message("suppression lift failed: ...")`. No `?`, so no admission path can propagate it (D-09).
- `admit_repository` calls it immediately after `discover_repository` and before the `existing_key` match, upstream of both `admit_existing_worktrees` calls, so `LaunchDirectory`, `Open` and `CloneCompletion` all lift through the single converge point in `open_repo_session_via` (D-12, Pitfall 6). `admit_standalone` lifts the canonical folder for symmetry.
- Five `App` tests over a real pushed fixture with two linked worktrees prove the skip, the three lift routes, the parent-root exclusion and the advisory failure posture, asserting on `repository_state` rows, the on-disk store and the message, never on source text.

## Task Commits

1. **Task 1 RED: failing consult and lift tests** - `3c11ff1` (test)
2. **Task 2 GREEN: consult and lift helper** - `f541c6c` (feat)
3. **Task 3 REFACTOR:** no commit; `cargo fmt --check` and clippy were clean after GREEN

## TDD Gate Compliance

| Gate | Commit | Evidence |
|------|--------|----------|
| RED | `3c11ff1` | five-test run exit 101: 0 passed, 5 failed, each on a behaviour assertion (four on the suppressed row being swept in, one on the missing lift-failure message) |
| GREEN | `f541c6c` | `cargo test -p baude app::tests -- --test-threads=1` exit 0: 98 passed, 0 failed |
| REFACTOR | skipped by design | `cargo fmt --check` exit 0, `cargo clippy -p baude --all-targets -- -D warnings` exit 0, nothing to change |

## Test Commands and Exit Codes

| Command | Exit | Result |
|---------|------|--------|
| `cargo test -p baude -- --test-threads=1 <five test names>` (RED, after fixture fix) | 101 | 0 passed, 5 failed on behaviour |
| `cargo test -p baude app::tests -- --test-threads=1` (GREEN) | 0 | 98 passed, 0 failed, 1461 s |
| `cargo fmt --check` | 0 | clean |
| `cargo clippy -p baude --all-targets -- -D warnings` | 0 | clean |
| `grep -c 'suppression::is_suppressed(' baude/src/app.rs` | n/a | 3 (one production consult, two test helper uses) |
| `awk '/fn admit_existing_worktrees/,/allocate_checkout_key/' ... \| grep -c 'is_suppressed'` | n/a | 1 |
| `grep -c 'fn lift_suppression(&mut self, path: &Path) {'` | n/a | 1 |
| `grep -c 'self.lift_suppression('` | n/a | 2 |
| `awk '/pub fn admit_repository/,/let repository_key = match/' ... \| grep -c 'lift_suppression'` | n/a | 1 |
| five `fn <test>` names grep | n/a | 5 |

## Must-haves Self-check

| Truth | Status | Verified by |
|-------|--------|-------------|
| Sweep consults `is_suppressed` after the `owned` check and skips the path | met | D1, awk gate prints 1 |
| `n`, `c` and a launch dir with that `repo_root` all reach `admit_repository`, which lifts after discovery and before both sweep calls | met | D2, D4, awk gate on the `existing_key` match |
| Parent root does not lift a child entry; key is the admitted path's own `repo_root` | met | D3 |
| Fresh row: new `CheckoutKey` and `first_seen_order` above all prior, `resume_id` None, no `owned_runtime`, Inactive | met | D2 |
| Lift I/O error is a message and the admission continues | met | D5; helper returns unit, no `?` |
| `admit_standalone` lifts the canonical folder | met | second `self.lift_suppression(` call site in `admit_standalone` |

Prohibitions: no new call site admits a worktree (D1 asserts the suppressed path absent while siblings are admitted); parent admission never lifts a child (D3); a lift failure never becomes an admission error (D5 and the unit-returning helper).

## Files Created/Modified

- `baude/src/app.rs` - `use baude_core::suppression;`, the consult in `admit_existing_worktrees`, `lift_suppression` and its two call sites, `SuppressionFixture` plus five helpers and five tests in `app::tests`

## Decisions Made

- **Lift key is `repo_root` of the admitted path.** A launch from `checkout/src` lifts `checkout`; admitting `repo/src` lifts `repo`, never a child.
- **Failed lift leaves the entry and the sweep still skips it.** D-09 says the admission must not fail, not that the row must appear without the write; behaviour 5's "row is admitted" is read as the repository's admission completing with its rows intact and a runtime handed back, which is what the test asserts.
- **Fixture paths are canonicalised up front** so `checkout_row` comparisons match the `/private/var` paths the sweep records (Pitfall 3).

## Deviations from Plan

### Verification command shape

**1. [Verification] Task 1's verify command passes five filters before `--`, which cargo rejects as unexpected arguments**
- **Found during:** Task 1 RED run
- **Issue:** `cargo test -p baude a b c d e -- --test-threads=1` exits 1 with `unexpected argument` before any test runs.
- **Fix:** Ran the same five filters after `--`, where libtest accepts several: `cargo test -p baude -- --test-threads=1 a b c d e`. Same tests, same single-threaded run.
- **Files modified:** none

### Auto-fixed Issues

**1. [Rule 1 - Bug] Test 5's lock injection collided with the real lock file**
- **Found during:** Task 1 RED run
- **Issue:** `suppress` leaves `.suppressed-admissions-<ws>.json.lock` behind, so `create_dir_all` at that path failed with AlreadyExists in the fixture step rather than failing on behaviour.
- **Fix:** The test removes the file before creating the directory, matching plan 01's injection against a fresh root.
- **Files modified:** baude/src/app.rs
- **Commit:** `3c11ff1` (fixed before the RED commit)

---

**Total deviations:** 1 verification-shape note, 1 fixture fix inside this plan's own tests
**Impact on plan:** None; the five behaviours and three prohibitions hold as written.

## Issues Encountered

- `cargo test -p baude app::tests -- --test-threads=1` takes about 24 minutes on this machine; each admission test builds a real pushed repository and spawns a stand-in runtime. A re-exec'd child test prints its own `test result` line mid-run, so a watcher must key on the parent's exit rather than that string.

## User Setup Required

None.

## Known Stubs

None.

## Threat Flags

None beyond the plan's register. T-19-13 is covered by D2 and D4 (any explicit admission of the exact path lifts a planted entry); T-19-14 by D5; no `Cargo.toml` change.

## Next Phase Readiness

- 19-06 can rely on the sweep skipping suppressed paths and on explicit admission lifting them for its relaunch tests.
- 19-07 owns `scripts/gate.sh`; this plan ran only the per-task test, fmt and clippy commands.
- Phase 21's daemon admission calls the same `suppression::is_suppressed` and `suppression::lift` helpers; nothing TUI-specific was added to the store.

---
*Phase: 19-re-admission-suppression*
*Completed: 2026-11-04*

## Self-Check: PASSED

- Commits 3c11ff1 and f541c6c are ancestors of HEAD
- baude/src/app.rs modified; 19-05-SUMMARY.md present
