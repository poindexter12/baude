---
phase: 21-daemon-and-pwa-parity
plan: 09
subsystem: api
tags: [rust, axum, daemon, removal, rows, git, fixtures]

requires:
  - phase: 21-01
    provides: POST /removals/preview and POST /removals, Manager::remove_rows, the removal fixtures and the two-checkout fixture with a real bare origin
  - phase: 21-02
    provides: GET /rows and its ten pinned checkout field names, Manager::rows on the merged state_for_save view
provides:
  - Manager::admitted_checkout_role, the one admission rule that stores main for git's main worktree
  - short_branch_name, the rows-only projection of the stored full ref
  - Ten row_kinds_ HTTP tests proving repository parent and main checkout removal over REST
  - cloned_repo, removal_root, removal_daemon and push_to_bare_origin test fixtures
  - A changed restart contract for a main checkout, which now follows a branch switch
affects: [21-10, 21-03, 21-04, 21-05, 21-08, 21-11]

tech-stack:
  added: []
  patterns:
    - "Every expected role, branch and path in a row_kinds_ test is read back from git or from GET /rows on the fixture, never written as a literal"
    - "The stored observed_branch stays git's full ref; the short name exists in the rows projection only"
    - "A test seam that reproduces an old stored shape says so in its doc comment and does nothing else"

key-files:
  created: []
  modified:
    - bauded/src/manager.rs
    - bauded/src/api.rs

key-decisions:
  - "A checkout is stored as main only when git discovery succeeded, the discovered worktree is the repository's main worktree, and no row of that repository already holds main; every other admission is managed_branch"
  - "Restart on a main checkout now follows a branch switch and returns 202, because core gives a main row a following binding; a managed worktree row stays pinned and still returns 409"
  - "GET /rows serves the short branch name by stripping one leading refs/heads/ prefix at read time; the stored ref is never rewritten and a detached HEAD reads null"
  - "Rows written by an earlier daemon are not re-roled; a main checkout stored as managed_branch is served verbatim and its directory is still kept"
  - "The git test helper captures its output so libtest result lines are not split by git's own stdout"

requirements-completed: []
requirements-advanced: [API-01]

coverage:
  - deliverable: "A repository parent with a main checkout and a managed worktree is removed over the two removal routes in one save"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#row_kinds_repository_parent_drops_three_rows_in_one_save"
        status: pass
  - deliverable: "GET /rows serves the branch name git reports, on a real clone with a slash-bearing branch, and the stored ref stays full"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#row_kinds_branch_is_the_short_name_git_reports"
        status: pass
  - deliverable: "A detached HEAD is admitted as main and reads a null branch"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#row_kinds_detached_head_reads_a_null_branch"
        status: pass
  - deliverable: "A second plain session on one main checkout is admitted and does not break the save path"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#row_kinds_second_plain_session_on_a_main_checkout_is_admitted"
        status: pass
  - deliverable: "Removing the main checkout row alone keeps its directory, its repository row and its sibling row"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#row_kinds_main_checkout_removal_keeps_the_directory"
        status: pass
  - deliverable: "A repository removal leaves a sibling repository whose name extends it untouched"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#row_kinds_parent_removal_leaves_a_sibling_repository_untouched"
        status: pass
  - deliverable: "A repository removal racing a removal of its own child commits exactly one"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#row_kinds_parent_removal_racing_a_child_removal_commits_one"
        status: pass
  - deliverable: "A rows read racing a repository removal is all or nothing"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#row_kinds_rows_read_racing_a_parent_removal_is_all_or_nothing"
        status: pass
  - deliverable: "A repository removal converges after its worktree was removed by hand"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#row_kinds_parent_removal_converges_after_the_worktree_was_removed_by_hand"
        status: pass
  - deliverable: "The stored shape earlier daemons wrote is served verbatim and is safe to remove"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#row_kinds_legacy_mis_roled_main_row_keeps_its_directory"
        status: pass
  - deliverable: "Restart stays refused on a pinned worktree row and follows a branch switch on a main checkout"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#restart_api_refuses_checkout_branch_change"
        status: pass
      - kind: test
        ref: "bauded/src/api.rs#restart_api_follows_a_branch_switch_on_a_main_checkout"
        status: pass

duration: 25min
completed: 2026-11-11
status: complete
actuals:
  tokens: 12183
  tasks: 3
  commits: 3
plan_head_before: 18c9c4dfed48c7cd320ee51c15f8095b103a2e8f
plan_head_after: 3741088100c8946ef6b1966cc9815898dfd2c468
---

# Phase 21 Plan 09: Repository parent and main checkout row kinds Summary

**A repository parent and a main checkout are removable over `POST /removals/preview` and `POST /removals`, with the main checkout admitted as `main` by one stated rule and `/rows` serving the branch name git itself reports.**

## Performance

- **Duration:** about 25 min of execution, in two stretches around one coordinator decision
- **Started:** not captured exactly, because the context was compacted mid-plan; the first RED log is stamped 2026-11-11T21:27:46Z
- **Completed:** 2026-11-11T21:45Z
- **Tasks:** 3
- **Files modified:** 2, `bauded/src/manager.rs` and `bauded/src/api.rs`

## Accomplishments

- API-01 is proven over REST for the repository parent and the main checkout. Three rows leave in one save, two runtimes are killed, only the managed worktree path is deleted, and a fresh restore finds none of them.
- The role label is correct at admission. A plain `POST /sessions` on git's main worktree stores `main`; a managed worktree stores `managed_branch`.
- `/rows` `branch` is the short name, checked against `git symbolic-ref --short -q HEAD` in each row's own directory on a real clone.
- Both clauses of the API-01 concurrency edge hold for a fan-out target, each observed in both issue orders.
- `Manager::remove_rows` and its recheck needed no fix for the fan-out target. No defect was found in the removal path.

## Task Commits

1. **Task 1: one repository parent removed end to end, main checkout admitted as `main`** - `214d53f` (feat). Also carries the two restart tests from the coordinator decision below.
2. **Task 2: `/rows` serves the branch name git reports, main checkout removable** - `47bf3dd` (feat)
3. **Task 3: the API-01 edges for a fan-out target and the pre-plan row shape** - `3741088` (test)

## The admission rule as built

`Manager::admitted_checkout_role(&self, repository_key: RepositoryKey, snapshot: Option<&git::RepositorySnapshot>) -> CheckoutRole`

- `Main` when all three hold: `snapshot` is `Some`, `snapshot.selected_worktree.path == snapshot.main_worktree`, and no checkout row of `repository_key` already holds `Main`.
- `ManagedBranch` in every other case, including the arm where discovery returned nothing. That arm is untouched and remains 21-10's.
- `PrimaryDefault` is never assigned by admission.

Both `record_checkout_intent`, the production admission, and `record_runtime`, the test-only one, take their role from this helper, so they cannot drift.

The third condition is load-bearing. Two plain sessions on one main checkout produce two checkout rows at the same path, the first `main` and the second `managed_branch` with `managed_by_baude` false, and a later save still validates.

## The projection rule as built

`fn short_branch_name(observed: Option<&str>) -> Option<String>` in `bauded/src/manager.rs`, beside the `/rows` view structs.

- `None` maps to `None`. There is no default-branch fallback.
- A value beginning `refs/heads/` loses that one prefix and keeps the rest verbatim, so `refs/heads/chore/workspace` reads `chore/workspace`.
- Any other value is served as stored.

`Manager::rows` applies it to `observed_branch` and still reads `state_for_save()`. No write of `observed_branch` changed, nothing in `baude-core` changed, and no `/rows` view struct gained or lost a field.

## The `/rows` body from the cloned fixture

Pasted whole from `row_kinds_branch_is_the_short_name_git_reports`: a real `git clone` whose main checkout was switched to `chore/workspace`, one plain session and one managed worktree session on `topic`. Client plans can copy these values. Only the temp directory name varies between runs.

```json
{
  "repositories": [
    {
      "checkouts": [
        {
          "archived": false,
          "branch": "chore/workspace",
          "checkout_key": 1,
          "display_name": "clone",
          "managed_by_baude": false,
          "path": "/private/var/folders/q1/335pbzl13sz825k87q4ywl300000gq/T/bauded-removal-kinds-branch-64083/clone",
          "repository_key": 1,
          "role": "main",
          "session_id": 1
        },
        {
          "archived": false,
          "branch": "topic",
          "checkout_key": 2,
          "display_name": "clone:topic",
          "managed_by_baude": true,
          "path": "/private/var/folders/q1/335pbzl13sz825k87q4ywl300000gq/T/bauded-removal-kinds-branch-64083/data/baude/worktrees/claude/repository-1/topic-2",
          "repository_key": 1,
          "role": "managed_branch",
          "session_id": 2
        }
      ],
      "display_name": "clone",
      "health": {
        "status": "available"
      },
      "path": "/private/var/folders/q1/335pbzl13sz825k87q4ywl300000gq/T/bauded-removal-kinds-branch-64083/clone",
      "repository_key": 1
    }
  ],
  "standalone": []
}
```

The main row reads `role: "main"` although its branch is not the default branch. The label follows git's main worktree and not a name.

## Helpers and seams added

Production code, `bauded/src/manager.rs`:

| Item | Signature |
|---|---|
| admission rule | `fn admitted_checkout_role(&self, repository_key: RepositoryKey, snapshot: Option<&git::RepositorySnapshot>) -> CheckoutRole` |
| projection | `fn short_branch_name(observed: Option<&str>) -> Option<String>` |
| test seam | `#[cfg(test)] pub(crate) fn stored_observed_branch_for_test(&self, key: CheckoutKey) -> Option<String>` |
| test seam | `#[cfg(test)] pub(crate) fn set_checkout_role_for_test(&mut self, key: CheckoutKey, role: CheckoutRole)` |

Test module, `bauded/src/api.rs`. 21-10 and 21-03 reuse these:

| Helper | Signature | Notes |
|---|---|---|
| `removal_fixture` | `async fn removal_fixture(label: &str) -> (PathBuf, FixtureRepo, Workspace, Shared, Router, u64, CheckoutKey)` | unchanged from 21-01 |
| `removal_setup` | `fn removal_setup(label: &str) -> (PathBuf, FixtureRepo, Workspace, Shared, Router)` | unchanged signature, now built from the two below |
| `removal_root` | `fn removal_root(label: &str) -> PathBuf` | a fresh, empty, isolated root |
| `removal_daemon` | `fn removal_daemon(root: &Path) -> (Workspace, Shared, Router)` | a persisting daemon over repositories the caller already built; a `FixtureRepo` for `root` must be held first |
| `cloned_repo` | `fn cloned_repo(root: &Path) -> (FixtureRepo, PathBuf)` | upstream by `initialized_repo`, one real `git clone`; returns the upstream owner and the canonical clone path |
| `push_to_bare_origin` | `fn push_to_bare_origin(root: &Path, repo: &Path)` | extracted from `removal_fixture_two_checkouts` |
| `exit_fixture_runtime` | `async fn exit_fixture_runtime(fixture: &TwoCheckoutFixture, id: u64, key: CheckoutKey)` | kills the agent process and waits for status `exited` |
| `git_read` | `fn git_read(dir: &Path, args: &[&str]) -> Option<String>` | trimmed stdout, `None` on a non-zero exit |
| `rows_checkout_for_session` | `fn rows_checkout_for_session(rows: &serde_json::Value, id: u64) -> serde_json::Value` | the D-09 Option B join on `session_id` |
| `removal_call` | `async fn removal_call(app: &Router, path: &str, body: &serde_json::Value) -> (StatusCode, String)` | any JSON `POST`, raw body back |
| `preview_target` | `async fn preview_target(app: &Router, target: &serde_json::Value) -> serde_json::Value` | preview any target, asserting 200 |
| `report_paths` | `fn report_paths(report: &serde_json::Value, field: &str) -> Vec<String>` | |
| `child_rows` | `fn child_rows(report: &serde_json::Value) -> Vec<String>` | `rows_removed` without its last element, sorted |
| `session_ids` | `async fn session_ids(app: &Router) -> Vec<u64>` | ids from `GET /sessions`, sorted |
| `rows_has_repository` | `fn rows_has_repository(rows: &serde_json::Value, key: &serde_json::Value) -> bool` | |

## RED observations

**Task 1**, `/tmp/baude-21-09-red.log`, status 101 in `/tmp/baude-21-09-red.status`:

```
thread 'api::tests::row_kinds_repository_parent_drops_three_rows_in_one_save' panicked at bauded/src/api.rs:3028:9:
assertion `left == right` failed: 21-09 role pin: the checkout at the repository path was served as "managed_branch"
  left: String("managed_branch")
 right: "main"
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 120 filtered out
```

**Task 2**, `/tmp/baude-21-09-t2-red.log`, status 101 in `/tmp/baude-21-09-t2-red.status`:

```
thread 'api::tests::row_kinds_branch_is_the_short_name_git_reports' panicked at bauded/src/api.rs:3301:13:
assertion `left == right` failed: 21-09 branch pin: /rows served "refs/heads/chore/workspace" where git reports chore/workspace
  left: String("refs/heads/chore/workspace")
 right: "chore/workspace"
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 122 filtered out
```

Both went red on their own marker and not on a compile error or a fixture precondition.

## Race observations

Each race test runs both issue orders, on a fresh fixture each.

| Test | Order | Outcome |
|---|---|---|
| parent racing child | repository issued first | repository removal 200, child removal 404 |
| parent racing child | child issued first | child removal 200, repository removal 409 |
| rows read racing parent | read issued first | the raced read was byte-equal to the body before |
| rows read racing parent | removal issued first | the raced read was byte-equal to the body after |

The tests assert "not 200" for the losing side and do not pin 404 or 409. The status matrix is 21-03's.

## Observed test counts and exit statuses

| Command | Exit | Result |
|---|---|---|
| `cargo test -p bauded api::tests::row_kinds_ -- --test-threads=1` | 0 | `row_kinds ok: 10` |
| `cargo test -p bauded api::tests::removals_ -- --test-threads=1` | 0 | 7 passed |
| `cargo test -p bauded api::tests::rows_ -- --test-threads=1` | 0 | 6 passed |
| `cargo test -p bauded -- --test-threads=1` | 0 | 131 passed, 0 failed |
| `cargo test -p bauded` | 0 | 131 passed, 0 failed |
| `cargo fmt --all --check` | 0 | clean |
| `cargo clippy -p bauded --all-targets -- -D warnings` | 0 | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | clean |

The baseline was 120. The 11 added are the ten `row_kinds_` tests and `restart_api_follows_a_branch_switch_on_a_main_checkout`.

`scripts/gate.sh` and `scripts/gate.sh --linux` were not run. They are 21-11's.

## Decisions Made

- **Admission stores `main` for git's main worktree.** The rule is in `admitted_checkout_role` and nowhere else.
- **Restart on a main checkout now follows a branch switch.** This is a behavior change, decided by the coordinator. See the first deviation.
- **The short branch name lives in the projection only.** Core compares and re-observes full refs.
- **Stored rows are not re-roled.** Whether to heal rows an earlier daemon wrote, at restore, is still an open decision for the orchestrator. Not healing is reversible.

## Deviations from Plan

### Coordinator decision

**1. Restart on a main checkout now follows a branch switch; a pre-existing test was re-subjected**

- **Found during:** Task 1, by the crate run the plan names as the detector for flagged assumption 2
- **Issue:** the pre-existing `restart_api_refuses_checkout_branch_change` got 202 where it asserted 409. Its subject was a main checkout. Admitted as `main`, that row follows its branch, because core gives `Main` a following binding and `ManagedBranch` a pinned one at `baude-core/src/lifecycle.rs:506`. The old 409 was a side effect of the daemon mis-roling every row as `managed_branch`.
- **What happened:** execution stopped and reported, as the plan requires. The coordinator decided to accept follow-the-branch restart for main checkouts and not to change the admission rule.
- **Fix, as directed:** `restart_api_refuses_checkout_branch_change` keeps its 409 and its name, and its subject is now the managed worktree row of `removal_fixture_two_checkouts`, which is still pinned. A real `git checkout -b` in the worktree, then restart: 409, a body naming `BranchChanged`, both checkout rows unchanged, the stored ref unchanged. A new test, `restart_api_follows_a_branch_switch_on_a_main_checkout`, pins the new behavior: after a real `git switch -c` in the main checkout, restart returns 202 and the stored observed branch is the new branch's full ref, read back from the fixture.
- **Stated plainly:** restart on a main checkout used to be refused with 409 after a branch switch. It is now accepted with 202 and the row adopts the new branch. Production admission changes the same way for every newly admitted main checkout.
- **Plan criterion affected:** the plan's "no test that existed before this plan was edited to accommodate the role change" does not hold for this one test. It was edited on the coordinator's instruction, with coverage kept and extended.
- **Files modified:** `bauded/src/api.rs`, `bauded/src/manager.rs`
- **Commit:** `214d53f`

### Auto-fixed Issues

**2. [Rule 3 - Blocking] The `git` test helper captured no output, so the plan's count check could not match**

- **Found during:** Task 2 verify
- **Issue:** the verify counts lines matching `^test api::tests::row_kinds_[a-z_]+ \.\.\. ok$`. The helper ran git with an inherited stdout, so git's own output landed between libtest's `test name ...` and its `ok`, splitting the line. All six tests passed and the count read 0.
- **Fix:** `git` now captures output and prints git's stderr only when the command fails. `cloned_repo`'s direct `git clone` does the same. Every assertion is unchanged, and a failing git command now says why.
- **Files modified:** `bauded/src/api.rs`
- **Commit:** `47bf3dd`

### Smaller differences

- **3. `removal_fixture` was not given a parameter.** The plan asked for a way to build the daemon over an already-built repository without copying the manager and router setup. That is `removal_root` plus `removal_daemon`, which `removal_setup` is now built from. `removal_fixture` keeps its 21-01 signature.
- **4. `stored_observed_branch_for_test` landed in Task 1's commit**, not Task 2's, because the new restart test needs it.
- **5. The first Task 1 RED run was a compile error**, the test having been placed in the wrong module. The recorded RED is the valid rerun, red on the `21-09 role pin` marker.
- **6. The two race tests run both issue orders**, as 21-02's rows race does, where the plan described one `join!`. Both outcomes of the parent and child race were observed.
- **7. A pinned restart refusal marks the repository's `health` as unavailable.** The re-subjected restart test therefore compares the two checkout rows before and after, not the whole `/rows` body. This is existing core behavior, observed and not changed.

## TDD Gate Compliance

The plan is `type: execute` with `tdd="true"` tasks, and it specified RED evidence as a recorded log and status file per task with one commit per task. Both RED runs were taken before the production edit and both files are kept. There is no separate `test(...)` commit ahead of each `feat(...)` commit for Tasks 1 and 2, by the plan's own commit names. Task 3 added tests only and needed no production change beyond its `cfg(test)` seam.

## Issues Encountered

None beyond the deviations above.

## Known Stubs

None. `StandaloneRowView.session_id` is still `None` by 21-02's design and is 21-10's to fill.

## Next Phase Readiness

**For 21-10, standalone rows:**
- The no-discovery arm of admission is exactly as it was: a plain folder still lands as `managed_branch`.
- Build a daemon over any directory with `removal_root` and `removal_daemon`. Hold a `FixtureRepo` for the root first.
- A detached main worktree is discovered by git and admitted as `main` with a null branch. Flagged assumption 7 held.

**For 21-03, the drift and failure matrix:**
- Losing-side statuses observed, not pinned: a child removal after its repository was removed returns 404; a repository removal with a stale report after its child was removed returns 409.
- A pinned restart refusal returns 409 with `BranchChanged` in the body and marks the repository unavailable.
- An already-removed worktree path is reported under `already_removed` and is not treated as drift.

**For 21-04 and the client plans 21-05 and 21-08:**
- `/rows` `branch` is the short name or `null`. `role` is `main`, `primary_default` or `managed_branch`; there is no `worktree` value.
- `role` is not the authority for deletion. Read `paths_deleted` and `paths_kept` from the preview.
- A state file from an earlier daemon still serves its main checkout as `managed_branch`. Clients must not assume every repository has a `main` row.

**Open decision for the orchestrator:** whether to heal pre-plan rows at restore.

**Requirements:** API-01 is advanced, not completed. API-01, API-02 and API-03 stay pending for later plans.

## Self-Check: PASSED

- Commits `214d53f`, `47bf3dd` and `3741088` are ancestors of HEAD.
- `bauded/src/manager.rs`, `bauded/src/api.rs` and both RED log and status pairs exist.
- `bauded/src/api.rs` holds ten `fn row_kinds_` tests; the diff since `18c9c4d` names only the two planned files.
