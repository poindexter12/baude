---
phase: 18-shared-kill-then-remove-lifecycle
verified: 2026-11-14
verified_at_commit: ca822383ad0e40b84ea9833b4c404b27cfd7c688
status: passed
score: 12/13 must-haves verified (1 deferred to plan 22-05)
covered_files:
  - ".planning/phases/18-shared-kill-then-remove-lifecycle/18-01-PLAN.md"
  - ".planning/phases/18-shared-kill-then-remove-lifecycle/18-01-SUMMARY.md"
  - ".planning/phases/18-shared-kill-then-remove-lifecycle/18-02-PLAN.md"
  - ".planning/phases/18-shared-kill-then-remove-lifecycle/18-02-SUMMARY.md"
  - ".planning/phases/18-shared-kill-then-remove-lifecycle/18-03-PLAN.md"
  - ".planning/phases/18-shared-kill-then-remove-lifecycle/18-03-SUMMARY.md"
  - ".planning/phases/18-shared-kill-then-remove-lifecycle/18-04-PLAN.md"
  - ".planning/phases/18-shared-kill-then-remove-lifecycle/18-04-SUMMARY.md"
  - ".planning/phases/18-shared-kill-then-remove-lifecycle/18-05-PLAN.md"
  - ".planning/phases/18-shared-kill-then-remove-lifecycle/18-05-SUMMARY.md"
  - ".planning/phases/18-shared-kill-then-remove-lifecycle/18-06-PLAN.md"
  - ".planning/phases/18-shared-kill-then-remove-lifecycle/18-06-SUMMARY.md"
  - ".planning/phases/18-shared-kill-then-remove-lifecycle/18-07-PLAN.md"
  - ".planning/phases/18-shared-kill-then-remove-lifecycle/18-07-SUMMARY.md"
  - ".planning/phases/18-shared-kill-then-remove-lifecycle/18-08-PLAN.md"
  - ".planning/phases/18-shared-kill-then-remove-lifecycle/18-08-SUMMARY.md"
  - "baude-core/src/git.rs"
  - "baude-core/src/lib.rs"
  - "baude-core/src/pty.rs"
  - "baude-core/src/removal.rs"
  - "baude-core/src/repository.rs"
  - "baude-core/src/session.rs"
  - "baude-core/src/testing.rs"
covered_digest: "v4:sha256:044859d8ffbfc3b3e5a7b4361e84abd3b784c481b2d057415fcff32c43f1abfb"
behavior_unverified: 0
overrides_applied: 0
re_verification:
  date: 2026-11-14
  head: ca822383ad0e40b84ea9833b4c404b27cfd7c688
  note: "Re-verified on 2026-11-14 at HEAD ca822383ad0e40b84ea9833b4c404b27cfd7c688 after gap closure by plan 22-06 (c4cdd47, 44c2267, d2411a1)."
  previous_status: gaps_found
  previous_score: 11/13
  previous_commit: f934e6474e54f7e195919e7fe4cf9d5779d5d66b
  gaps_closed:
    - "Truth 13: Pty::kill_and_wait reaches process-group extinction even when the PTY leader has already exited, and is idempotent on an extinct group (BL-11, macOS EPERM on a zombie-only group)"
  gaps_remaining: []
  regressions: []
deferred:
  - truth: "Truth 12: scripts/gate.sh green on macOS and --linux on the same tree (plan 18-07)"
    addressed_in: "Phase 22, plan 22-05"
    evidence: "22-05-PLAN.md must-have: 'scripts/gate.sh and the Linux gate both exited 0 on one commit, and that commit is the one on the remote branch'; the plan keeps 05-mac.result and 05-linux.result. Not run in this pass, by instruction. No 22-05-SUMMARY.md exists yet, so the truth is open, not proven."
coincidental_reliance_items:
  - truth: "Plan 04 readers_parked reporting (removal::tests::reader_parked_on_out_of_group_slave_holder_is_reported, pty::tests::group_extinction_leaves_reader_parked_for_out_of_group_slave_holder)"
    reason: fixture-only
    harden: "testing::await_parked_holder still reaps the leader through is_exited() before the call under test, and its doc comment now says so. The unreaped state is covered at the Pty layer by kill_and_wait_reaps_unreaped_leader_group, not at the removal layer."
advisory:
  - finding: "The in-tree BL-11 test proves the wait-until-reapable path. The production-shaped path, child mutex free and the first try_lock reaping the zombie, is proven only by an out-of-tree run in this pass, 94 of 94 in-window calls Ok."
    category: other
    reason: "The production window is a scheduling race and cannot be pinned deterministically in-tree. Recorded so the next reader knows which evidence covers which path."
    evidence_status: "reproduced, logs under target/gsd-22/verify-18/"
  - finding: "The red test c4cdd47 is red on macOS only. On Linux a zombie-only group accepts the signal, so the test passes before and after the fix. Linux proof of the fix commit rides on the deferred gate, plan 22-05."
    category: other
    reason: "The fix touches platform process code and CLAUDE.md requires scripts/gate.sh --linux for that."
    evidence_status: "code read; Linux not run in this pass"
---

# Phase 18: Shared Kill-Then-Remove Lifecycle Verification Report

**Phase Goal:** `baude-core` can remove any row kind in one operation that kills the owned runtime first, removes managed worktrees from disk, never deletes a main checkout, standalone folder or local branch, and persists atomically, so the TUI and daemon share one removal path.
**Verified:** 2026-11-14
**Status:** passed, 12 of 13 verified, truth 12 deferred to plan 22-05
**Re-verification:** Yes, after gap closure. Prior report: `gaps_found`, 11/13, HEAD `f934e64`, 2026-11-13.
**Code verified at:** HEAD `ca822383ad0e40b84ea9833b4c404b27cfd7c688` on `gsd/phase-18-shared-kill-then-remove-lifecycle`. No source file was modified by this pass. 33 commits landed since `f934e64`.

## What changed under the phase since `f934e64`

| File | Change | Consequence for this report |
|------|--------|-----------------------------|
| `baude-core/src/pty.rs` | +277 / -24 (plan 22-06) | The truth 13 fix and its six tests. Read in full below |
| `baude-core/src/testing.rs` | +4 / -2 | Doc comment on `await_parked_holder` only |
| `baude-core/src/removal.rs` | +1078 | One production hunk (`update_suppression`, `:793-804`, plan 22-07): a removed primary is written with `suppress_primary`. It sits in the clear stage, after the save. The rest is plan 22-01 tests |
| `git.rs`, `repository.rs`, `session.rs`, `lib.rs`, `lifecycle.rs`, `persist.rs`, `bauded/src/manager.rs` | byte-identical | Code evidence stands; tests re-run at HEAD |
| `baude/src/app.rs` | +1027 | Call sites moved; wiring re-located below |

## Goal Achievement

### Observable Truths

Evidence key: **T** = test passed in the one `cargo test -p baude-core` run of this pass, **C** = code read at HEAD, **R** = out-of-tree reproduction run in this pass.

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | SC1: removing a row with a live runtime terminates the owned group SIGTERM then SIGKILL on macOS and Linux, no zombie or orphan | ✓ VERIFIED (macOS T; Linux C) | T: `live_runtime_group_is_extinct_after_remove`, `agent_and_shell_are_both_extinct_after_remove`, `group_extinction_after_leader_exit`, `kill_and_wait_confirms_child_exit`. C: `extinguish_group` (`pty.rs:877`) now delegates to `extinguish_with`, which is TERM, extinction wait, KILL, extinction wait, error; the sequence is unchanged. Linux not run here |
| 2 | SC2: a runtime that survives escalation leaves the row in place with an error | ✓ VERIFIED (T, C) | T: `surviving_runtime_leaves_row_and_disk`, `parent_kill_failure_aborts_before_disk_and_state`, `extinction_sequence_reports_unsignalable_live_group_as_live`. C: `kill_stage` (`removal.rs:923`) runs before `disk_stage` and `persist_stage`. The false `RuntimeSurvived` the prior report noted is gone with truth 13 |
| 3 | SC3: managed worktree child deleted even when unclean or cwd-held; main checkout and standalone keep directory and every branch; cloned and `git init` fixtures | ✓ VERIFIED (T, C) | T: all eight tests the prior report named, green. C: `disk_action_for` (`removal.rs:1095`). The new `suppress_primary` call writes a suppression record under the config dir and deletes nothing |
| 4 | SC4: removing a repository parent removes each child by the per-child rules then the parent, in one operation | ✓ VERIFIED (T, C) | T: `repository_removes_children_then_parent`, `select_rows_repository_target_fans_out_children_first_parent_last`. C: `select_rows` (`removal.rs:306`), `remove` (`:436`) |
| 5 | SC5: a crash mid-removal leaves the prior state or the fully removed state | ✓ VERIFIED (T, C) | T: `crash_before_save_reconciles_missing`, `repository_crash_mid_fan_out_keeps_parent_and_children_in_file`, `repository_disk_failure_mid_fan_out_leaves_state_and_file_untouched`, `save_closure_is_called_exactly_once_for_parent_with_children`. C: `persist_stage` (`removal.rs:1035`) |
| 6 | Plan 01: side-effect-free `preview_removal`, per-kind classification, serde round-trip, typed `UnknownTarget` | ✓ VERIFIED (T) | `preview_has_no_side_effects_and_report_round_trips_serde`, `remove_report_matches_preview_rows`, six `select_rows_*` tests |
| 7 | Plan 02: forced managed-worktree removal with containment, identity re-proof, marker retry, bare, foreign and submodule guards | ✓ VERIFIED (T) | All 30 `git::tests::forced_removal::*` green. `git.rs` unchanged |
| 8 | Plan 03: `RepositoryState::remove_rows` batch removal | ✓ VERIFIED (T) | All 4 `remove_rows_*` green. `repository.rs` unchanged |
| 9 | Plan 04: preflight before any signal, candidate `validate_for_save` before any irreversible effect, one save, `readers_parked` reported | ✓ VERIFIED (coincidental-reliance) (T, C) | C: `remove` order at `removal.rs:442-457` re-read: `select_rows`, `planned_clearings`, `preflight`, clone, `remove_rows`, `validate_for_save`, `kill_stage`, `disk_stage`, `persist_stage`, `clear_stage`. T: the five tests the prior report named. See `coincidental_reliance_items` |
| 10 | Plan 05: every persistence failure mode | ✓ VERIFIED (T) | The four persistence-failure tests green. `replacement_committed` is still the only committing error branch (`removal.rs:1048`) |
| 11 | Plan 06: keep rules with and without live runtimes, unmanaged and already-removed rows, parent fan-out on both fixture kinds | ✓ VERIFIED (T) | Plan-06 tests green, plus `already_removed_managed_row_prunes_git_registration`, `cloned_topology_places_worktrees_under_managed_root`. `lifecycle.rs` diff since `73fa674` is empty |
| 12 | Plan 07: `scripts/gate.sh` green on macOS and `--linux` on the same tree | DEFERRED to plan 22-05 | Not run, by instruction. Not verified and not failed. See Deferred Items |
| 13 | Plan 08: `Pty::kill_and_wait` reaches group extinction after leader exit, including an exited, unreaped, leader-only group; pid-reuse `ForeignOwner`; reader join or parked report; lock-free error arm | ✓ VERIFIED (macOS T, R, C; Linux C) | Gap closed. See Truth 13 below |

**Score:** 12/13 truths verified, 0 failed, 1 deferred to plan 22-05 (0 present, behavior-unverified)

### Truth 13: the BL-11 gap is closed

**The fix (`44c2267`), read in code.** `signal_group` in `baude-core/src/pty.rs` now asks the pure rule `signal_step_proceeds`, which returns true for a zero result, `ESRCH` or `EPERM`. Every other errno is still an error under the same context string. Proceeding means only "go on to the extinction wait". `wait_for_group_extinction` (`pty.rs:1137`) is unchanged: each 10 ms round reaps through `try_lock` plus `try_wait` and returns true only when the probe stops reporting the group, and the probe rule `group_probe_alive` still counts `EPERM` as alive. So tolerating `EPERM` at the signal step cannot turn into reporting a live group gone: a group that answers `EPERM` forever gets TERM, a 500 ms wait, KILL, a 500 ms wait, then `PTY process group N remained live after forced termination`. The fail-closed property the prior report recorded is kept. This is the remedy the prior report asked for, including its warning that reaping first is not enough: a leader that is mid-exit and not yet waitable is retried each round. The stale comment on `group_disposition` is corrected.

**The tests, read in code and run.** Six new `pty::tests`, all green:

- `kill_and_wait_reaps_unreaped_leader_group` drives the real `Pty::kill_and_wait` with no double on the signal path. It asserts the leader is a zombie, asserts `EPERM` from the group on macOS before the call, and never calls `is_exited` or `await_parked_holder`. After the call it asserts `Ok`, that the call returned no earlier than the mutex release, `ESRCH` from the group, no `ps` row, and a second call `Ok`.
- `signal_step_errno_rule`, `group_probe_errno_rule` table-test the two rules, including `EINVAL` and a missing errno as errors.
- `extinction_sequence_reports_unsignalable_live_group_as_live`, `extinction_sequence_stops_after_term_when_extinct`, `extinction_sequence_surfaces_a_signal_error` pin the sequence through `extinguish_with`, which production also calls.

**Is the mutex-held fixture a fair reproduction? Yes, with one limit.** The test holds the child mutex so neither the reader thread nor the kill path can reap, which keeps the zombie for 150 ms instead of a few hundred microseconds.

- Fair: the state under test is the kernel state, and it is the real one. A real leader exits by itself, is a real unreaped zombie, and the real group answers `EPERM`; both are asserted, not assumed. Holding a mutex does not reap and does not change what `kill(2)` returns. Before the fix the call returns the BL-11 error on its first line of work, so the test is red on macOS for the right reason.
- Stricter than production in one way: reaping is impossible for 150 ms, so an `Ok` cannot come from finding the group already gone. The `returned_at >= released_at` assertion pins that. This also covers the mid-exit, not-yet-waitable case.
- The limit: in production the mutex is free during the window, so the first `try_lock` in the wait reaps the zombie. The fixture never takes that path, since after release the blocked reader may be the one that reaps. That path is not pinned in-tree.

**The limit is covered by a run of the real race (R).** The prior report's out-of-tree binary (depends on `baude-core` by path, production cfg, `sleep 0.05` leader, no `is_exited`, no mutex) was rebuilt against HEAD and run again:

```text
prior, f934e64:  rounds 100: zombie-only window observed 90, kill_and_wait Err in window 77, Ok in window 13, window missed 10
now,   ca82238:  rounds 100: zombie-only window observed 94, kill_and_wait Err in window 0, Ok in window 94, window missed 6
now, runs 2 and 3: observed 96 and 95, Err in window 0 and 0, Ok in window 96 and 95 (285 of 285 Ok across the three runs)
```

**Linux.** By code read only. On Linux a zombie-only group accepts the signal, so the new tolerance is not reached on this path and the mutex-held test passes with or without the fix. The Linux gate on the fix commit is part of the deferred truth 12.

### Deferred Items

Not yet met, explicitly owned by a later plan. Informational; does not affect status.

| # | Item | Addressed In | Evidence |
|---|------|--------------|----------|
| 1 | Truth 12: `scripts/gate.sh` and `scripts/gate.sh --linux` green on the same tree | Phase 22, plan 22-05 | `22-05-PLAN.md` must-have: "scripts/gate.sh and the Linux gate both exited 0 on one commit, and that commit is the one on the remote branch", with result files kept. Plan 22-05 has no SUMMARY yet. This truth is open until those result files exist; this report does not vouch for it |

### Advisory (New Scope, Unevidenced)

| # | Finding | Category | Why Advisory |
|---|---------|----------|--------------|
| 1 | The production-shaped path of truth 13 (mutex free, first `try_lock` reaps) is proven out-of-tree only | other | A scheduling race cannot be pinned deterministically in-tree; evidence kept under `target/gsd-22/verify-18/` |
| 2 | The red test is red on macOS only; Linux proof rides on plan 22-05 | other | Platform fact, not a defect |

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `baude-core/src/removal.rs` | Vocabulary, `select_rows`, `preview_removal`, `remove`, stages | ✓ VERIFIED | 5937 lines, production to `:1175`. `select_rows` `:306`, `preview_removal` `:359`, `remove` `:436`, `preflight` `:847`, `kill_stage` `:923`, `disk_stage` `:971`, `persist_stage` `:1035` |
| `baude-core/src/lib.rs` | `pub mod removal;` | ✓ VERIFIED | Unchanged |
| `baude-core/src/git.rs` | `remove_managed_worktree_forced` and guards | ✓ VERIFIED | Unchanged; 30 tests green |
| `baude-core/src/repository.rs` | `RemovedRows`, `remove_rows` | ✓ VERIFIED | Unchanged; 4 tests green |
| `baude-core/src/pty.rs` | `GroupDisposition`, `group_disposition`, `group_members`, `reader_finished`, group signal and extinction | ✓ VERIFIED | Prior defect fixed. `kill_and_wait` `:810`, `extinguish_group` `:877`, `group_disposition` `:952`, `wait_for_group_extinction` `:1137` |
| `baude-core/src/session.rs` | `kill_and_wait` contract | ✓ VERIFIED | Unchanged; 27 tests green |
| `baude-core/src/testing.rs` | `git_in`, `ClonedTopology`, `cloned_topology` | ✓ VERIFIED | Doc comment change only |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `removal::remove` | `Session::kill_and_wait` | kill stage | ✓ WIRED | `removal.rs:938` |
| `removal::remove` | `git::remove_managed_worktree_forced` | disk stage | ✓ WIRED | `removal.rs:985` |
| `removal::remove` | `remove_rows` + `validate_for_save` + caller save | clone, mutate, validate before kill, save once | ✓ WIRED | `removal.rs:442-457` |
| `removal::persist_stage` | `SaveError::replacement_committed` | only committing degraded branch | ✓ WIRED | `removal.rs:1048` |
| `pty::kill_and_wait` | `signal_group` + `wait_for_group_extinction` | `Signal` arm, through `extinguish_with` | ✓ WIRED | `pty.rs:840`, `:877`; `EPERM` now handled |
| `session::kill_and_wait` | `Pty::process_identity` | lock-free error arm | ✓ WIRED | `group_extinction_error_path_does_not_take_child_lock` green |
| TUI | `removal::remove` | `baude/src/app.rs:4537` | ✓ WIRED | Previews at `:4091`, `:4507` |
| Daemon | `removal::remove` | `bauded/src/manager.rs:1189` | ✓ WIRED | Preview at `:1115` |

### Behavioral Spot-Checks

Every exit code was written to a file by the command itself and read from that file, never through a pipe. Outputs are under `target/gsd-22/verify-18/`.

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Whole `baude-core` suite, run once | `cargo test -p baude-core` | exit 0; `572 passed; 0 failed; 1 ignored` in 209.35 s; doc-tests 0 | ✓ PASS |
| Phase suites, counted from that one log | none (grep of the saved log) | `removal::tests::` 53 ok, `forced_removal::` 30 ok, `pty::tests::` 30 ok, `remove_rows_` 4 ok, `session::tests::` 27 ok; 0 not ok | ✓ PASS |
| The 47 tests this report names | none (grep of the saved log) | 47 of 47 `... ok` | ✓ PASS |
| Teardown of an exited, unreaped leader, real race | out-of-tree `bl11-repro`, 100 rounds, built exit 0, run exit 0 | 94 in window, 0 `Err`, 94 `Ok` | ✓ PASS |
| Full gate, macOS and Linux | `scripts/gate.sh`, `scripts/gate.sh --linux` | not run, by instruction | DEFERRED to plan 22-05 |

Notes on the suite run. The one ignored test is `meta::tests::pane_claude_stand_in` (`meta.rs:2225`), a stand-in process launched by other tests, outside this phase's files. Three plan 22-01 tests (`removal::tests::end_state_matrix_*`) ran longer than 60 s each and passed; a second verifier was using the same tree at the time.

### Probe Execution

No `scripts/*/tests/probe-*.sh` exists and no plan declares one.

### Requirements Coverage

| Requirement | Status | Evidence |
|-------------|--------|----------|
| KILL-01 | ✓ SATISFIED (macOS; Linux pending plan 22-05) | Truths 1 and 13 |
| KILL-02 | ✓ SATISFIED | Reader join and `readers_parked` by test; the zombie leader is now reaped inside the call (truth 13) |
| KILL-03 | ✓ SATISFIED | Truth 2; the false survivor error is gone |
| RMV-01 | ✓ SATISFIED | Truths 3, 7 |
| RMV-02 | ✓ SATISFIED | Truth 3 |
| RMV-03 | ✓ SATISFIED | Truth 3 |
| RMV-04 | ✓ SATISFIED | Truths 4, 8 |
| RMV-05 | ✓ SATISFIED | One `removal::remove`; TUI and daemon both call it |
| RMV-06 | ✓ SATISFIED | Truths 5, 10 |
| RMV-07 | ✓ SATISFIED | `forced_arguments_never_touch_branches`, `main_worktree_inside_root_is_refused`, keep tests on both fixture kinds |

No orphaned requirements.

### Prohibitions

All thirteen prohibitions of the prior report were re-checked and are kept. The pinning tests are among the 47 confirmed green. Counts re-read at HEAD: `grace|GRACE` in `pty.rs` = 0; `#[ignore]` in `removal.rs`, `pty.rs`, `session.rs` = 0, 0, 0; `lifecycle.rs` diff since `73fa674` empty. "A foreign process group is never signalled" still holds after the fix: `group_disposition` decides before any signal and is unchanged in logic, and `group_extinction_disposition_never_signals_foreign_owner` is green. "Gate steps never judged through a pipe": kept in this pass.

### Anti-Patterns Found

None. The three findings of the prior report are resolved: the `ESRCH`-only tolerance is fixed, the `group_disposition` comment is corrected, and `await_parked_holder` now documents that it reaps. No `TBD`, `FIXME`, `XXX`, `TODO`, `HACK`, `PLACEHOLDER`, `todo!(` or `unimplemented!` in the seven covered source files.

### Human Verification Required

None for this phase. The gate is not a human item here: it is deferred to plan 22-05, which runs it on the pushed commit and keeps the result files.

### Gaps Summary

No gaps. The one failed truth of the prior report, BL-11, is closed: the signal step tolerates the `EPERM` macOS returns for a zombie-only group, the extinction wait still decides alone whether the group is gone, and a group that stays live still ends in an error. That is proven by an in-tree test on the real kill path and by a re-run of the real race, 0 failures in 94 in-window calls against 77 in 90 before. The other eleven truths were re-located in code and re-proven by the suite at HEAD with no regression. Truth 12, the gates, is neither verified nor failed here and stays open until plan 22-05 records both exit codes.

---

_Verified: 2026-11-14_
_Verifier: Claude (gsd-verifier)_
