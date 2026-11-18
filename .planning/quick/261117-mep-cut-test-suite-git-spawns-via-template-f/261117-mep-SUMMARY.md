---
phase: 261117-mep-cut-test-suite-git-spawns-via-template-f
plan: 01
subsystem: testing
tags: [git, nextest, fixtures, test-performance, ci]

requires: []
provides:
  - "measure-git-spawns.sh: counted and timed gate runs, per-spawn bench, spawn report by subcommand, class and test"
  - "baude_core::testing::committed_repo and pushed_repo, built once per run under BAUDE_GIT_TEMPLATE_DIR and copied"
  - "baude_core::testing::inventory_path, a worktree path from an existing snapshot instead of a fresh discovery"
  - "BAUDE_GIT_TEMPLATE_DIR wired into scripts/gate.sh isolated() and the CI suite step"
affects: [test-suite, scripts/gate.sh, ci.yml]

actuals:
  tokens: 11783
  tasks: 3
  commits: 4

plan_head_before: 24ddf35add27624bfa395054a3d2517397130026
plan_head_after: 3d112618ddfd8035b09be2313dc4c0fa8b5688ce
commits: 4

tech-stack:
  added: []
  patterns:
    - "Fixture repositories of a common shape come from committed_repo or pushed_repo, never an inline init, add, commit sequence"
    - "A per-run template directory beside the suite TMPDIR, finished only when its READY marker exists, renamed into place atomically"

key-files:
  created:
    - .planning/quick/261117-mep-cut-test-suite-git-spawns-via-template-f/measure-git-spawns.sh
  modified:
    - baude-core/src/testing.rs
    - baude-core/src/git.rs
    - baude-core/src/lifecycle.rs
    - baude-core/src/worktree_scan.rs
    - baude-core/src/removal.rs
    - baude/src/app.rs
    - bauded/src/api.rs
    - scripts/gate.sh
    - .github/workflows/ci.yml

key-decisions:
  - "c is the bench run the plan defines, taken after the baseline timed runs: 154.85 ms per spawn. The bench swung from 35 to 155 ms across four runs on this host under load, and a regression of per-test duration on per-test spawns in the baseline timed run gives about 141 ms per spawn inside the suite, which corroborates the higher figure"
  - "The template is built only for sites the counted log shows built at least 5 times per run; sites under that, git_init_root and every unborn init stay in-place git"
  - "A pushed template records its own origin URL in its READY marker, and every copy rewrites exactly that one url line, asserting the count is 1"
  - "A copy goes only to an absent or empty destination on the template's filesystem, and refuses symlinks and special files; anything else builds in place"

requirements-completed: [QUICK-261117-mep]

duration: about 1h 50m
completed: 2026-11-17
status: complete
---

# Quick 261117-mep: Cut test-suite git spawns via template fixtures Summary

**Suite git spawns down from 12,251 to 10,361 and fixture-construction spawns from 2,381 to 544, by writing fixture identities directly, starting GitFixture repos on their branch, reusing one discovery snapshot in cloned topologies, and copying plain and pushed fixture repositories from a per-run template instead of running git init, add, commit and push in every test.**

## Performance

- **Duration:** about 1h 50m
- **Started:** 2026-11-17T16:25Z (approximately, first baseline gate run 16:28 local)
- **Completed:** 2026-11-17T18:16 local
- **Tasks:** 3 of 3
- **Files modified:** 9, plus the measurement script

## Measurements

All counts come from the same counting wrapper through the same `scripts/gate.sh` invocation. Timed runs are plain `bash scripts/gate.sh` with no wrapper. The host was heavily loaded throughout, load average 30 to 44 on 10 cores, with endpoint-security agents scanning every exec, so wall times are noisy; spawn counts are near-deterministic.

| Metric | Baseline 24ddf35 | After collapse 57b0b71 | Final |
|---|---|---|---|
| Suite git spawns | 12,251 | 11,877 | 10,361 |
| Outside-suite spawns | 40 | 41 | 42 |
| Fixture-class spawns | 2,381 | 2,089 | 544 |
| Ambiguous-class spawns, clone, worktree add, symbolic-ref with target | 617 | 617 | 617 |
| Probe-class spawns | 9,253 | 9,171 | 9,200 |
| Template-eligible E: init, add, commit, remote add, push | 1,911 | 1,911 | 366 |
| `init` | 635 | 635 | 120 |
| `commit` | 475 | 475 | 70 |
| `add` | 472 | 472 | 67 |
| `remote add` | 174 | 174 | 64 |
| `push` | 155 | 155 | 45 |
| `config <arg>` | 220 | 20 | 20 |
| `branch -M` | 92 | 0 | 0 |
| `rev-parse` | 4,110 | 4,069 | 4,077 |
| `worktree list` | 2,588 | 2,547 | 2,547 |
| Timed wall 1 | 753.006 s | not timed | 241.864 s |
| Timed wall 2 | 613.540 s | not timed | 244.499 s |
| Mean timed wall | 683.273 s | not timed | 243.181 s |
| Sum of PASS durations, run 1 and 2 | 3,005.1 s, 2,441.1 s | not timed | 963.5 s, 975.2 s |
| Counted-run wall, wrapper on PATH | 895.814 s | 530.615 s | 349.587 s |
| Tests run and skipped | 1,189 run, 1 skipped | 1,189 run, 1 skipped | 1,193 run, 1 skipped |

The counted walls are not comparable timings: the wrapper adds a `/bin/sh` per spawn and the host load moved between runs. The four extra tests in the final run are the four new template tests in `testing.rs`. Skipped stays at 1 and the `#[ignore]` count at 2.

**Wall time is confounded by host load and does not show the saving by itself.** The baseline timed runs ran at load average 30 to 44, the final timed runs at about 13 to 15, so 683 s against 243 s mostly measures the machine. To separate the two, one more pair ran back to back after the final runs: the baseline commit 24ddf35 from a temporary detached worktree, then the final tree.

| A/B pair, back to back | Nextest wall | Sum of PASS durations | Load during the run |
|---|---|---|---|
| Baseline 24ddf35 | 262.518 s, 1,189 run, 1 skipped | 1,046.2 s | about 15 |
| Final 3d11261 | 308.787 s, 1,193 run, 1 skipped | 1,231.8 s | rose to 32 midway |

Load spiked again during the second half of the pair, so it does not settle the question either. The closest clean comparison is the Task 3 tracer, the 97 `baude-core git::` tests run back to back with the wrapper on PATH: 27.8 s and 1,230 spawns without the template, 23.3 s and 960 spawns with it. At the recorded c the 1,837 fixture spawns removed across the suite come to about 71 s of thread time at four threads; at the quiet-host bench of 35 ms, about 16 s. The spawn counts are the reliable result.

Remaining suite spawns are dominated by production discovery: `rev-parse --path-format=absolute --git-common-dir --show-toplevel` (2,328 at baseline) plus `worktree list --porcelain -z` (2,524) are the two calls `discover_repository` makes, about 40 percent of every spawn the suite starts. That is production code and out of this plan's scope; it is the next lever.

### Per-spawn cost c

| Run | init+add+commit x50 | c | rev-parse HEAD |
|---|---|---|---|
| smoke run before the baseline gate, scratch root | 5.270 s | 35.13 ms | 13.95 ms |
| `bench baseline`, after the timed runs, the recorded c | 23.227 s | 154.85 ms | 25.08 ms |
| repeat | 14.387 s | 95.92 ms | 20.51 ms |
| repeat | 9.399 s | 62.66 ms | 12.54 ms |
| `bench final`, at load about 11 | 5.327 s | 35.51 ms | 8.44 ms |

In-suite cost, from a least-squares fit of each test's duration on its spawn count in the baseline timed run 1: 141 ms per spawn over the 92 `baude-core git::` tests, with an intercept of -0.09 s, so those tests are almost entirely git time; 153 ms over all baude-core tests and 171 ms over the whole suite.

### Stop and go arithmetic

- W0 = 683.273 s, so 5 percent of W0 = 34.16 s.
- Task 1 stop rule: P = F0 x c / 1000 / 4 = 2,381 x 154.85 / 1000 / 4 = 92.17 s, 13.5 percent of W0. Not below 5 percent, so Task 2 started. Had the smoke bench's 35.13 ms been used, P would be 20.91 s, 3.1 percent, and the rule would have fired; the in-suite fit at 141 ms gives 83.9 s, 12.3 percent, so the recorded c is the one consistent with the suite itself.
- Task 3 go rule: P3 = E x c / 1000 / 4 = 1,911 x 154.85 / 1000 / 4 = 73.98 s, 10.8 percent of W0. At least 5 percent, so the template was built.

## Fixture inventory

Builds per run come from the baseline counted log where an argv shape identifies the site, otherwise from the call-site count.

| Site | Shape | Spawns per build, baseline | Builds per run | Disposition |
|---|---|---|---|---|
| baude-core/src/git.rs:3815 `GitFixture::repo` | plain committed on `topic` | 6: init, 2 config, add, commit, branch -M | 92 | Task 2 collapse to 3, then Task 3 template, 0 |
| git.rs:3851 `GitFixture::unborn_repo` | unborn | 1 | few | keep real, unborn |
| git.rs:3860 to 3975 remote and upstream helpers | ref and config shaping on an existing repo | 1 to 3 | per test | keep real, interleaved |
| git.rs:3977 `linked_worktree` | linked worktree | 2 | per test | keep real |
| lifecycle.rs:3269, 3347, 3465, 3582, 3653 | plain committed, then branch and worktree add | 5 | 1 each | Task 2 collapse to 3; under 5 builds, not templated |
| lifecycle.rs:3954 `drift_repo` | committed plus linked worktree, 2 discoveries | 4 + worktree add + 4 | 3 | keep, under 5 builds |
| lifecycle.rs:4318 `cloned_topology` | clone plus 2 linked worktrees | 3 upstream + 4 real + 6 discovery | 2 | Task 2 rediscovery, 6 to 2; upstream under 5 builds, not templated |
| testing.rs:408 `cloned_topology` | clone plus 2 linked worktrees | 3 upstream + 4 real + 6 discovery | about 11 | Task 2 rediscovery, 6 to 2; Task 3 upstream template; clone, switch and both worktree adds stay real |
| removal.rs:1888 `RemovalFixture::new` | plain committed on `main` | 3 | 64 | Task 3 template |
| removal.rs:1919 `managed_worktree` | linked worktree | 1 | per test | keep real |
| removal.rs:4586 `init_repo` | plain committed | 3 | 2 | keep, under 5 builds |
| worktree_scan.rs:1960 `git_repo` | plain committed | 3 | 1 | keep, under 5 builds |
| worktree_scan.rs:1978 `git_init_root` | plain committed in a non-empty root | 5 | 1 | Task 2 collapse to 3; never templated |
| worktree_scan.rs:3879 inline | plain committed | 5 | 1 | Task 2 collapse to 3 |
| breadcrumbs.rs:515 `init_repository` | plain committed | 3 | 2 | keep, under 5 builds |
| launch.rs:159, 195 and folder_workspace.rs:493 | unborn | 1 | 1 each | keep real, unborn |
| baude/src/app.rs:8995 `admission_repo` | pushed | 6: 2 init, add, commit, remote add, push | about 115, also under the relaunch, suppression, saved-sessions and stale-restore fixtures | Task 3 pushed template |
| app.rs:9063 `admission_repo_cloned` | pushed plus clone | +1 clone | 3 | clone stays real |
| app.rs:10814 `relaunch_fixture` second repo | plain committed | 3 | about 24 | Task 3 template |
| app.rs:17128 `admission_collision_sets_status` | pushed | 8 | 1 | Task 2 collapse to 6; under 5 builds |
| app.rs:17509 stale restore `IdentityChanged` | plain, identity through `-c` | 3 | per case | keep, interleaved with breaking the row |
| bauded/src/api.rs:1022 `committed_repo` | plain committed on `main` | 3 | 6 | Task 3 template |
| api.rs:1072 `initialized_repo_in_workspace` | plain committed on `main` | 3 | about 10 direct plus every `removal_setup` | Task 3 template |
| api.rs:2420 `push_to_bare_origin` with `removal_fixture_two_checkouts` | adds a pushed origin | 3 | 3 | keep, under 5 builds |
| api.rs:2660 `sibling_repo`, 6645 inline | plain committed | 3 | 2 and 1 | keep, under 5 builds |
| bauded/src/manager.rs:5382 `removal_manager`, 5436 `committed_repo` | pushed, plain | 6, 3 | 2, 3 | keep, under 5 builds |
| manager.rs: 11 inline sequences from 5206 | plain or pushed | 3 or 6 | 1 each | keep, under 5 builds |

Cross-check against the top-25 lists: the top tests by fixture-class spawns at baseline were the GitFixture-heavy `git::tests::lifecycle::removal_preflight` cases (36, 32), the removal end-state matrix (34, 20, 20), and the app restore case (47); all are covered by the GitFixture, RemovalFixture and admission_repo rows. The suite total of 12,251 is in line with the gate comment's 15,000, so no search for git started by PTY shells on their own PATH was needed; the 40 outside-suite spawns are the gate's own commit-message checks and doctest fixtures.

## Accomplishments

- Task 1: `measure-git-spawns.sh` with `run`, `bench` and `report`; a generated `/bin/sh` git wrapper with the real git path and log directory baked in, a classifier whose ambiguous class holds clone, worktree add and symbolic-ref with a target because production issues those too, and top-25 lists by total and fixture spawns.
- Task 2: no fixture starts a git process for an identity; `GitFixture::repo` runs `init -b topic` and costs 3 spawns instead of 6; both `cloned_topology` helpers take worktree paths from the clone snapshot's inventory through `testing::inventory_path`, after confirming `discover_repository` draws `selected_worktree` from that same inventory by canonical path.
- Task 3: `CommittedRepo`, `committed_repo` and `pushed_repo` in `baude-core/src/testing.rs`, std only; templates keyed by shape and every spec field through `DefaultHasher`; private `.building-` directory, `READY` written last, atomic rename, loser cleans up; four equivalence and reuse tests; wired through `scripts/gate.sh` and the CI suite step.

## Task Commits

1. **Task 1: spawn counter** - `3157d75` perf: add a git spawn counter for the test suite
2. **Task 2: collapse** - `57b0b71` perf: write fixture identities directly and start GitFixture repos on their branch
3. **Task 3: template** - `6ac7218` perf: build plain and pushed fixture repositories once per run and copy them
4. **Task 3: wiring** - `3d11261` ci: give the suite a per-run git fixture template directory

## Removed fixture-step exit checks

No test assertion was removed. These fixture-step git exit checks moved into a helper that panics on failure:

- lifecycle.rs, five sequences: the success asserts on two `git config` calls each, now `set_git_identity`, which panics if `.git/config` is missing or unwritable.
- worktree_scan.rs `git_init_root` and the inline sequence near 3879: `git_ok` on two `git config` calls each, now `set_git_identity`.
- git.rs `GitFixture::repo`: `git_ok` on two `git config` calls and on `branch -M topic`, gone with `init -b topic`; the init, add and commit checks moved into `committed_repo`, which panics with git's stderr.
- app.rs `admission_collision_sets_status`: the `.expect` on two `git config` spawns, which only checked that the process started, now `set_git_identity`.
- testing.rs and lifecycle.rs `cloned_topology`: the `expect` and `unwrap` on two per-worktree discoveries, now `inventory_path`, which panics when the inventory lacks the worktree.
- Task 3 adopters, RemovalFixture::new, admission_repo, relaunch_fixture, api committed_repo and initialized_repo_in_workspace, and the testing.rs cloned_topology upstream: their init, add, commit, remote add and push exit checks moved into `committed_repo` and `pushed_repo`.

## Decisions Made

- Recorded c is the plan's own bench, run as `bench baseline` after the timed runs, 154.85 ms. The bench is very noisy on this host, 35 to 155 ms; the in-suite fit, 141 ms, sides with the recorded value, and both rules pass with margin at either of those.
- Sites under 5 builds per run stay in place even when they share a spec with a templated site, as with the lifecycle.rs `cloned_topology` upstream, per the plan's adoption rule.
- The equivalence tests fold loose object file names into one placeholder per object, because a commit id covers its timestamp, so two builds a second apart hold the same tree under different commit ids. Object counts still have to match.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical functionality] Template provenance check in the tests**
- **Found during:** Task 3
- **Issue:** the equivalence tests would also pass if every templated call silently fell back to an in-place build, so they could not show the copy path ran.
- **Fix:** an `only_template` helper asserts exactly one finished template with READY and no building directory, and the reuse test writes a marker into the template after the first build and asserts the second copy carries it. A mutation that skipped the origin URL rewrite made the pushed test fail, as it should.
- **Files modified:** baude-core/src/testing.rs
- **Commit:** 6ac7218

**2. [Rule 3 - Blocking] Borrow error in the first draft of `usable_destination`**
- **Found during:** Task 3 clippy
- **Fix:** rewrote it with `is_ok_and`, and moved the filesystem note to `same_filesystem`, where the check lives.
- **Commit:** 6ac7218

### Flakes seen, unrelated to the change

- Baseline counted run: `bauded api::tests::removals_admission_in_another_repository_survives_the_commit` failed its 22.1-10 window pin, a POST that took 2.19 s during the slow stage, under host load with the wrapper adding a shell per git call. Both baseline timed runs passed. The timing pin is load-sensitive.
- First Task 2 gate run: `assert-no-test-leftovers.sh --self-test` case "an orphan seen across the persist window fails with orphans=1" saw no orphan in its window. Python process watcher under load, no Rust or fixture code involved; the rerun passed with 1,189 run and 1 skipped.

## Issues Encountered

- Host load average stayed between 30 and 44 on 10 cores for the whole run, with endpoint-security agents on every exec, which is why per-spawn cost was 4 to 5 times the plan's "tens of milliseconds". Wall-time deltas should be read with that in mind; the spawn deltas are the reliable result.
- `.github/workflows/test-weights.yml` also runs the suite and was not in the plan's files, so it still builds fixtures in place. Its per-test weights will now overstate fixture cost relative to CI shards that use the template. Worth wiring the same way if the weights drift.

## Next Phase Readiness

- The largest remaining source is production discovery: 4,852 of the baseline suite's spawns were the two `discover_repository` calls. Caching or pairing those, for example reading the inventory once per reconcile, would cut more than everything here.
- To re-measure: `measure-git-spawns.sh run <label> both`, `bench <label>`, `report <label>`; logs live under `${TMPDIR}/git-measure/<label>`.

## Self-Check: PASSED

- FOUND: measure-git-spawns.sh, baude-core/src/testing.rs, scripts/gate.sh, .github/workflows/ci.yml
- FOUND in HEAD's history: 3157d75, 57b0b71, 6ac7218, 3d11261
- `scripts/gate.sh` exited 0 on the committed tree, 3d11261, in the A/B final run, which also ran the commit-message check over all four commits; the three final measurement runs also exited 0
- `git diff --quiet 24ddf35 -- Cargo.lock Cargo.toml baude-core/Cargo.toml baude/Cargo.toml bauded/Cargo.toml` exits 0; `#[ignore]` count 2; `"clone",` and two `"worktree",` argv remain in testing.rs
