---
phase: 20
plan: 04
status: complete
subsystem: tui-remove-action
tags: [removal, failure-paths, test-double, gates]
requirements_addressed: [TUI-04, TUI-03]
requires: [20-02, 20-03]
provides:
  - baude_core::testing::spawn_unkillable_runtime and arm_unkillable_runtime, test-support gated
  - baude_core::testing::parked_reader_command, await_parked_holder, HolderGuard, holder_pgid
  - App-level proof of D-09 and D-10 under kill, save and clearing failure
affects: [baude-core/src/testing.rs, baude-core/src/removal.rs, baude/src/app.rs]
tech-stack:
  added: []
  patterns:
    - failure doubles are real process groups plus a one-shot injection, never a mocked kill
    - gates judged by their own exit captured to a file, never through a pipe
key-files:
  created: []
  modified:
    - baude-core/src/testing.rs
    - baude-core/src/removal.rs
    - baude/src/app.rs
files_modified:
  - baude-core/src/testing.rs
  - baude-core/src/removal.rs
  - baude/src/app.rs
commits:
  - sha: c7e90b1
    subject: "test(20-04): expose unkillable runtime double through baude-core testing"
  - sha: 29ac670
    subject: "test(20-04): run the parked reader double as a script so backends can append flags"
  - sha: 41f9b07
    subject: "test(20-04): prove D failure paths keep rows visible and retry succeeds"
plan_head_before: ad20c44cdf7b63c6f74b423bb3182f27691cb974
plan_head_after: 41f9b07
decisions:
  - The unkillable double is a real process group with a real out-of-group slave holder whose first teardown is the Phase 18 injection naming the surviving pid, because nothing portable outlives SIGKILL and macOS killpg skips zombies that Linux counts.
  - The parked reader command is written as a script beside the pid file and launched as sh script, so a backend that appends flags to the configured command still runs the fragment.
  - No chore gates-green commit, because no gate fix was needed.
metrics:
  duration: about 2h including both gates
  completed: 2026-11-05
actuals:
  tokens: 6800
  tasks: 3
  commits: 3
---

# Phase 20 Plan 04: Failure Paths and Gates Summary

Kill, save and clearing failures of the D remove action proven at App level against a real spawned process group and the atomic save injection, with the unkillable runtime double promoted into baude-core testing; both repository gates green on the branch.

## What was built

**Task 1, commit c7e90b1.** `HolderGuard`, `holder_pgid` and `spawn_parked_reader_runtime` moved out of the removal test module in `baude-core/src/removal.rs` into `baude_core::testing` under the test-support gate, so the baude crate can reuse them. Added `parked_reader_command` and `await_parked_holder` for callers that spawn through their own runtime path, and `arm_unkillable_runtime` plus `spawn_unkillable_runtime`: a real process group with an out-of-group slave holder whose first teardown is the Phase 18 injection naming the surviving pid. The removal tests are unchanged in outcome.

**Task 2, commits 29ac670 and 41f9b07.** Four tests in `baude/src/app.rs` over the plan 02 D flow, all asserting on fixture output:

- `remove_project_failure_kill_keeps_rows_and_handles_then_retry_succeeds`: the kill failure returns before any row drops, the registry still holds the session handle, and the retry extinguishes the real process group (T-20-09).
- `remove_project_failure_save_keeps_rows_then_retry_succeeds`: with `atomic_failure_for_test` armed the row stays in state and hierarchy with the typed error in the modal, and the retry removes it in one save (T-20-10).
- `remove_project_clearing_failure_is_a_footer_notice_not_an_error`: a clearing failure after the committed save is a footer notice naming the uncleared item, and removal is reported done (T-20-11).
- `remove_project_recheck_refreshes_modal_when_row_set_changes`: a row set that changed behind the modal refreshes the report instead of removing.

**Task 3.** Both gates run bare on the final HEAD 41f9b07, exits captured directly. No fix was needed, so no `chore(20-04): phase 20 gates green` commit exists.

## Verification results

| Check | Command | Exit | Evidence |
|---|---|---|---|
| Task 1 verify | `cargo test -p baude-core removal::tests -- --test-threads=1 && cargo clippy -p baude-core --all-targets -- -D warnings` | 0 | 49 passed, 0 failed; clippy clean |
| Task 2 verify | `cargo test -p baude app::tests::remove_project_ -- --test-threads=1` | 0 | 4 passed, 0 failed, 27.75s |
| macOS gate | `scripts/gate.sh` | 0 | `gate: all steps passed`; log `/private/tmp/claude-503/-Users-joese-Code-github-com-poindexter12-baude/7e2bf3a3-31ef-4b4e-806d-1a7cbf278d4a/scratchpad/gate-macos.log` |
| Linux gate | `scripts/gate.sh --linux` | 0 | `gate: all steps passed`; log `/private/tmp/claude-503/-Users-joese-Code-github-com-poindexter12-baude/7e2bf3a3-31ef-4b4e-806d-1a7cbf278d4a/scratchpad/gate-linux.log` |
| Commit messages | `bash scripts/check-commit-message.sh --range origin/main..HEAD` | 0 | no findings |

Each gate was redirected to its log and its exit read from the shell variable on the next line, never through a pipe.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Parked reader double could not run under the App spawn path**
- **Found during:** Task 2, first run of the kill failure test hung waiting for the holder to park.
- **Issue:** The App composes its spawn as `exec <command> --port N --hostname 127.0.0.1`. The double was a bare shell fragment ending in `exit 0`, so the appended flags became trailing arguments to `exit` and the holder never parked.
- **Fix:** `parked_reader_command` writes the fragment beside the pid file and returns `sh <script>`; `await_parked_holder` now reports the leader state and pty backlog when the holder never parks so the next failure is diagnosable.
- **Files modified:** baude-core/src/testing.rs
- **Commit:** 29ac670 (an extra commit not prescribed by the plan; the two prescribed Task 1 and Task 2 subjects are unchanged)

**2. [Formatting] `cargo fmt` applied before the Task 1 commit** after `cargo fmt --check` flagged the moved block. No logic change.

### Not done, by design

- `chore(20-04): phase 20 gates green` was not committed: the plan reserves it for the case where a gate needed a fix, and none did.
- STATE.md and ROADMAP.md untouched per the dispatch; the pre-existing dirty `.planning/config.json` and `.planning/STATE.md` were left unstaged.

## Known Stubs

None.

## Threat Flags

None. All three mitigations the plan's threat model assigned to this plan (T-20-09, T-20-10, T-20-11) are covered by the named tests; no new surface was introduced.

## Handoff

- `baude_core::testing::spawn_unkillable_runtime(fx, branch)` and `arm_unkillable_runtime` are the reusable double; the baude crate reaches them through the test-support feature.
- `parked_reader_command` returns `sh <script>`, so it is safe for any backend that appends flags to the configured command.
- Test names above are the proof for D-09 and D-10; the ConfirmRemove retry path is exercised through the real `confirm_remove_project` and the registry, not through a mock.

## Self-Check: PASSED

Commits c7e90b1, 29ac670, 41f9b07 are ancestors of HEAD; all modified files and this SUMMARY exist on disk.
