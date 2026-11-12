---
phase: 21-daemon-and-pwa-parity
plan: 10
subsystem: api
tags: [rust, axum, daemon, removal, rows, standalone, suppression, git, fixtures]

requires:
  - phase: 21-01
    provides: POST /removals/preview and POST /removals, Manager::remove_rows, the removal fixtures and the two-checkout fixture with a real bare origin
  - phase: 21-02
    provides: GET /rows and Manager::rows on the merged state_for_save view
  - phase: 21-09
    provides: Manager::admitted_checkout_role, the row_kinds_ fixtures, cloned_repo, removal_root and removal_daemon
provides:
  - route_plain_admission, the five-route table a plain POST /sessions follows
  - Standalone rows on the daemon with runtime custody through launch, close, restart, restore and removal
  - The suppression lift on explicit admission, in Manager::create and nowhere else
  - Plain admission of a hand-made linked worktree as an unmanaged checkout, which returned 503 before
  - folder_fixture, recording_folder_fixture, restarted_daemon, removed_over_routes, add_by_hand_worktree and create_pre_plan_folder_session_for_test
affects: [21-03, 21-04, 21-05, 21-08, 21-11]

actuals:
  tokens: 32489
  tasks: 3
  commits: 5
plan_head_before: aebb3e1fdb56f621cbbd01468e8fe0cfc400ee29
plan_head_after: d53a39b6dd45ef86acf4d232d0707a039f8a962d

tech-stack:
  added: []
  patterns:
    - "Every suppressed path in a test is the path of the SuppressionAdded item in the executed report, deserialized into core's RemovalReport; no test rebuilds it"
    - "Each pair of readings of one suppressed path is taken with the directory in the same state, because folder_key canonicalizes only while the directory exists"
    - "The lift runs in one place, the wrapper around the admission body, so an arm cannot return a session without it and an error arm cannot reach it"
    - "The managed flag of a checkout row is never derived from the worktree fact; a plain admission is unmanaged by construction"

key-files:
  created: []
  modified:
    - bauded/src/manager.rs
    - bauded/src/api.rs

key-decisions:
  - "A plain POST /sessions takes the first matching route of five: reuse a standalone row, the checkout arm for a discovered repository, join a stored repository that already holds the path, admit a standalone row, or refuse"
  - "DELETE /sessions/{id} on a folder session ends the runtime and keeps the row, closed; a repeated POST for that folder reopens the same row"
  - "An explicit admission through Manager::create lifts the suppression entry for the path the admitted row stores; restore, reopen, the restart route and legacy migration lift nothing"
  - "A plain admission takes the repository path and the worktree fact from git discovery, not from git rev-parse --show-toplevel, and always stores the checkout as unmanaged"
  - "delete_teardown_failure_is_500_and_unknown_id_stays_404 and real_atomic_persistence_failures_are_503_for_every_mutation were moved from /tmp onto a real repository with every assertion untouched; standalone DELETE got its own failure-mapping tests in Task 2"

requirements-completed: []
requirements-advanced: [API-01, API-03]

coverage:
  - deliverable: "A plain folder opened over POST /sessions is one standalone row, removed over the two removal routes with the folder kept"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#standalone_folder_session_is_removed_end_to_end"
        status: pass
  - deliverable: "An explicit admission lifts the suppression entry for exactly the path it admits; a refused request and a recreated branch lift nothing"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#suppression_explicit_admission_lifts_the_exact_path"
        status: pass
  - deliverable: "Removing a repository parent drops its child's suppression entry"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#suppression_parent_removal_drops_the_child_entry"
        status: pass
  - deliverable: "A daemon restart admits no row for a removed path and lifts nothing"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#suppression_restart_admits_nothing_and_lifts_nothing"
        status: pass
  - deliverable: "A hand-made linked worktree is admitted by path as an unmanaged checkout and removed with its directory kept"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#by_hand_worktree_is_admitted_unmanaged_and_removed_with_its_directory_kept"
        status: pass
  - deliverable: "A folder held by the earlier row shape keeps that shape until its repository row is removed, then becomes a standalone row"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#standalone_earlier_folder_row_keeps_its_shape_and_is_removable"
        status: pass
  - deliverable: "A worktree folder whose repository was deleted is admitted as a standalone row and removed with its folder kept"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#standalone_stale_worktree_folder_is_admitted_without_a_repository"
        status: pass

metrics:
  duration: "Tasks 1 and 2 on 2026-11-11; Task 3 and close about 65 min on 2026-11-12"
  completed: 2026-11-12
status: complete
---

# Phase 21 Plan 10: Standalone rows on the daemon Summary

A plain folder opened over `POST /sessions` is now one standalone row with owned runtime custody,
removable over the two removal routes, and an explicit daemon admission lifts the suppression entry
for exactly the path it admits.

## What was built

### Task 1: store a plain folder session as a standalone row (`4236eeb`)

- `route_plain_admission` is the five-route table of the wire contract and reads nothing but its
  arguments.
- The `runtime_standalones` registry and `RuntimeOwner` sit beside `runtime_checkouts`; a folder
  launch records `Launching` then `Running` with its owned runtime on the row.
- `record_checkout_intent` takes a join key and no longer stores a repository row for a folder git
  does not report as one.
- `GET /rows` serves the standalone session id, and `remove_rows` hands the standalone runtime to
  core and drops its registry entry.
- Added `folder_fixture`, `standalone_folder_session_is_removed_end_to_end` and
  `plain_admission_routes_by_what_git_reports`.

### Task 2: keep folder session custody through close, restart and restore (`6e027da`)

- `close_standalone` ends a folder session and keeps its row, `Inactive` with no owned runtime.
- `DELETE` on a folder session answers 500 on a failed teardown and 503 on a failed save.
- `reopen_standalone` relaunches a closed or exited row under its key by the resume rule; a
  repeated `POST` for a closed folder reopens its row.
- `restore_standalones` ends the runtime a restart finds recorded and relaunches live rows.
- An early dead targeted resume of a folder session clears the row's resume id.
- `state_for_save` and the removal save merge live standalone metadata through one helper.

### Task 3: lift a suppression entry on explicit admission (`58b0553`, `d53a39b`)

- `Manager::create` wraps the admission body, now `admit`. After `admit` returns a session, a
  persisting manager calls `lift_admission_suppression` once with `stored_row_path`: the
  standalone row's `canonical_path` or the checkout row's `observed_path`. An error from `admit`
  returns before the lift. A store error is logged and never returned.
- No other function calls `suppression::lift`, and nothing in `bauded` calls `suppress` outside a
  test. No `is_suppressed` guard was added.
- `create_pre_plan_folder_session_for_test` stores and launches the row shape a daemon before this
  plan wrote for a folder, from the allocation lines Task 1 removed. It is used by
  `standalone_earlier_folder_row_keeps_its_shape_and_is_removable`, so it carries no dead code
  allowance.
- Six tests from the plan plus one pinned by-hand worktree test. Final counts: 4 `suppression_`
  tests and 11 `standalone_` tests. The plan's verification says exactly nine `standalone_` tests;
  the two extra are the standalone `DELETE` failure-mapping tests Task 2 added.

## RED gates

| Gate | Marker | Record | Result |
|------|--------|--------|--------|
| 1 | `21-10 standalone pin` | `/tmp/baude-21-10-red.log`, status 101 | Red on its marker. Recorded during Task 1 under this name, not `red-1`; read back at plan close, not re-run |
| 2 | `21-10 custody pin` | `/tmp/baude-21-10-red-2.log`, status 101 | Red on its marker, two custody pin assertions |
| 3 | `21-10 suppression pin` | `/tmp/baude-21-10-red-3.log`, status 101 | Red on its marker before the lift was written |
| by-hand | `21-10 by-hand worktree pin` | `/tmp/baude-21-10-red-byhand.log`, status 101 | Red on its marker before the admission fix |

Gate 3 output, from the tree at `58b0553` plus the tests and before the lift:

```
test api::tests::suppression_explicit_admission_lifts_the_exact_path ... FAILED
test api::tests::suppression_parent_removal_drops_the_child_entry ... ok
test api::tests::suppression_restart_admits_nothing_and_lifts_nothing ... ok
test api::tests::suppression_standalone_removal_writes_no_entry ... ok
21-10 suppression pin: an explicit POST /sessions admitted <root>/by-hand and its suppression entry is still in the store
test result: FAILED. 3 passed; 1 failed
```

The other five plan tests pin behaviour that is inherited or was built by Tasks 1 and 2. They passed
from the moment they compiled, as the plan expected, and no failure was contrived for them.

Gate 3 was recorded twice. The first record went red with status 101 but without the marker, on the
defect described under Deviations, and was invalid. It was overwritten by the valid record above
after the admission fix landed.

By-hand gate output, before the fix:

```
21-10 by-hand worktree pin: POST /sessions for the hand-made linked worktree <root>/by-hand was refused: invalid repository state: RetainedRepositoryPathMismatch(CheckoutKey(1))
  left: 503
 right: 201
```

## Verification

Run on the committed tree `d53a39b`, each status captured directly:

| Command | Status |
|---------|--------|
| `cargo fmt --all --check` | 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 |
| `cargo test -p bauded -- --test-threads=1` | 0, 149 passed, 0 failed |

The first run of the three ended fmt 1, clippy 0, test 0 with 149 passed: `rustfmt` wanted two
calls in the new tests wrapped. `cargo fmt --all` was applied and all three were run again; the
table is that second run.

Baseline before Task 3 was 142. The by-hand test adds one and the plan's tests add six.
`baude-core`, `baude`, `bauded/web` and every `Cargo.toml` are absent from the plan's diff. The
`/sessions` compatibility test is unedited.

`scripts/gate.sh` and `scripts/gate.sh --linux` were not run. They belong to 21-11.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug, coordinator decision] A hand-made linked worktree could not be opened by path**

- **Found during:** Task 3, on the first RED run.
- **Issue:** `POST /sessions {"repo": <dir>}` for a worktree made with `git worktree add` returned
  503 with `RetainedRepositoryPathMismatch`. The plain arm of `Manager::create` took the session's
  repository path from `git rev-parse --show-toplevel`, which answers with the linked worktree
  itself, and stored the worktree fact as false. Core validation requires the repository's main
  worktree and a true worktree fact for any checkout that is not the main worktree, so the save
  was refused. The same lines are in `aebb3e1`, so the defect predates this plan. It failed with
  and without a stored repository row.
- **Decision:** execution stopped and reported. The coordinator chose to fix admission and not to
  re-fixture the test.
- **Fix:** for a discovered path the repository path is `snapshot.main_worktree` and the worktree
  fact is `selected_worktree.path != main_worktree`. `record_checkout_intent` no longer passes the
  worktree fact as the managed flag; a plain admission always stores `managed_by_baude` false, so
  removing the row keeps the user's directory. Before the fix the worktree fact was always false
  on this arm, so no existing row changes.
- **Wire effect:** for such a session `/sessions` now reports `repo_root` as the main worktree and
  `is_worktree` true, the values the `worktree` arm already reports for a managed worktree.
- **Unchanged:** the `worktree` arm, the non-persisting manager, and every pre-existing test.
  No change in `baude-core`.
- **Files modified:** `bauded/src/manager.rs`, `bauded/src/api.rs`
- **Commit:** `58b0553`

**2. The test-only entry and the by-hand helpers landed in two commits**

The admission fix had to land before the suppression pin was reachable, so it was committed on its
own with its pinned test, then gate 3 was recorded, then the lift.

### Not fixed, observed

- A removed checkout row leaves the `.claude` directory the daemon wrote into the user's directory.
  The by-hand test compares the user's regular files by content and does not assert on it.
- Route 3, the join to a stored repository, still passes the request path as the repository path.
  For the earlier folder shape the two are equal and the test passes. A stored checkout whose
  repository's main worktree is a different path was not exercised.

## Known issues

**Open teardown flake, not fixed here and no assertion loosened.** On macOS,
`DELETE /sessions/{id}` on a repository row intermittently returns 500 with
`failed to terminate exact PTY process group`. Seen in 1 of 3 and 1 of 5 full
`cargo test -p bauded -- --test-threads=1` runs after Tasks 1 and 2, in
`real_atomic_persistence_failures_are_503_for_every_mutation` and
`flat_session_api_remains_a_non_hierarchical_compatibility_projection`. At `aebb3e1` it was 0 of 5,
where one run instead failed
`poll_clears_an_immediately_dead_targeted_resume_and_authorizes_reopen`. Suspected cause:
`signal_group` in `baude-core/src/pty.rs` tolerates ESRCH but not the EPERM macOS returns for a
group whose leader exited unreaped. It did not appear in the three full crate runs of this session: 143 passed at `58b0553`, and
149 passed twice on the Task 3 tree.

## Open decisions for the user

These are recorded, not settled.

1. **Rows earlier daemons stored for a folder are not converted.** They are served verbatim, keep
   working through route 3, and are removable with the folder kept. Converting them at restore is
   a state migration this plan does not make.
2. **Legacy flat-file migration still yields an unavailable repository for a folder.** Core's
   `LegacyReconciliation` has no standalone outcome; changing it is a core contract change.
3. **One session per folder is a behaviour change.** A second `POST /sessions` on one folder now
   returns the live session with the unchanged success status. Refusing it is the alternative.
7. **The TUI and the daemon share one suppression store and keep separate state files.** A path the
   TUI removed can still be a row in `daemon-state`, and the daemon relaunches it at restart. The
   TUI lifts the worktree root of the path it admits; the daemon lifts the stored path of the row.
   They can differ for a subdirectory of a worktree.
14. **A suppression entry's key depends on whether the directory existed when it was written.**
    `folder_key` canonicalizes only while the directory exists. An entry for a deleted managed
    worktree is keyed on the stored spelling and can stop matching once a directory exists there
    again. That is core's key; this plan neither changes it nor works around it.

## What later plans need

- **21-03 and 21-04:** a refused plain `POST /sessions` for a path that is not a directory returns
  400 with a plain text body. A discovery error other than not-a-repository is also a domain error
  on the same mapping. `checkout_key` and `standalone_key` are served only from `GET /rows`;
  clients join on `session_id`.
- A second `POST /sessions` for a live folder returns the existing session with 201, not a new one.
- A hand-made linked worktree opened by path is a `managed_branch` checkout with
  `managed_by_baude` false; removing it keeps the directory and writes a suppression entry, and
  opening it again by path lifts that entry.
- Requirements API-01, API-02 and API-03 stay pending.

## Commits

| Commit | Message |
|--------|---------|
| `4236eeb` | feat(21-10): store a plain folder session as a standalone row and remove it over REST |
| `6e027da` | feat(21-10): keep folder session custody through close, restart and restore |
| `58b0553` | fix(21-10): admit a hand-made linked worktree as an unmanaged checkout |
| `d53a39b` | feat(21-10): lift a suppression entry on explicit daemon admission |

The measured count of 5 from `aebb3e1` includes `1c25c7c`, a phase-level docs commit made between
Task 2 and Task 3.

## Self-Check: PASSED

The four commits are ancestors of HEAD, the two source files and this summary exist, and the stub
scan of the plan diff found only the fixture file name `todo.txt`.
