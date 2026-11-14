---
phase: 18-shared-kill-then-remove-lifecycle
verified: 2026-11-13
verified_at_commit: f934e6474e54f7e195919e7fe4cf9d5779d5d66b
status: gaps_found
score: 11/13 must-haves verified
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
covered_digest: "v4:sha256:1a6d472416da840a5bbb6fbf069940f0a833b570d5b25dfc0de3e2923f46a09b"
behavior_unverified: 0
overrides_applied: 0
re_verification: true
re_verification_detail:
  previous_status: passed
  previous_score: 13/13
  previous_commit: abba527
  gaps_closed: []
  gaps_remaining: []
  regressions:
    - "Plan 08: Pty::kill_and_wait on an exited, not yet reaped leader fails on macOS with EPERM (BL-11). Not a regression introduced after abba527: baude-core/src/pty.rs is byte-identical since that commit. The prior report did not exercise this state."
gaps:
  - truth: "Pty::kill_and_wait reaches process-group extinction even when the PTY leader has already exited, and is idempotent on an extinct group (plan 18-08; feeds KILL-01, KILL-02, KILL-03)"
    status: partial
    reason: "On macOS, kill(-pgid, SIGTERM) returns EPERM when the recorded group holds only an exited leader that nobody has reaped yet. signal_group tolerates ESRCH only, so kill_and_wait returns 'failed to terminate exact PTY process group: Operation not permitted (os error 1)' for a runtime that is already dead. removal::remove wraps that as RemovalError::RuntimeSurvived and leaves the row in place. Fails closed (nothing deleted, nothing leaked, a retry 200 ms later succeeds), but removal and session teardown fail intermittently for a row whose agent has just exited. This is BL-11. Reproduced through the real Pty::kill_and_wait: 77 of 90 calls made inside the window failed with that exact message."
    artifacts:
      - path: "baude-core/src/pty.rs"
        issue: "signal_group (1073-1087) returns Err for every errno except ESRCH. Line 1081 is the ESRCH-only tolerance. The error surfaces at 879-880 in extinguish_group."
      - path: "baude-core/src/pty.rs"
        issue: "process_group_exists (1089-1093) counts EPERM as 'group alive', and group_disposition routes an exited or zombie-only group to Signal (968, 978). The doc comment at 955-956 says such a group 'is signalled and reaped like any extinct-but-unreaped group', which is false on macOS."
      - path: "baude-core/src/pty.rs"
        issue: "Plan 18-08 (213e26e) removed the up-front child.try_wait() that reaped an exited leader and returned Ok before any signal, so an unreaped zombie leader now reaches signal_group. kill_and_wait reaps only after extinguish_group returns Ok (848-868)."
      - path: "baude-core/src/testing.rs"
        issue: "await_parked_holder (479-500) polls pty.is_exited(), which reaps the leader before the test calls remove or kill_and_wait. Every in-tree exited-leader test therefore starts from a reaped leader (group already ESRCH) or a group with a live descendant. No test calls kill_and_wait on an exited, unreaped, leader-only group."
    missing:
      - "In signal_group or extinguish_group, treat EPERM from kill(-pgid, sig) as 'no signalable member' and fall through to wait_for_group_extinction, which already reaps through try_lock and proves extinction by ESRCH before Ok is returned. Reaping first is not sufficient on its own: in one reproduction run EPERM persisted across an is_exited() call, so the leader can be mid-exit and not yet waitable."
      - "A pty::tests case that spawns a leader which exits by itself, does not call is_exited(), and asserts kill_and_wait returns Ok with the group extinct and the leader reaped. Per CLAUDE.md Tests: the fixture must model the unreaped state production produces."
      - "scripts/gate.sh --linux on the fix commit, since it touches platform process code."
not_reverified:
  - truth: "scripts/gate.sh green on macOS and --linux on the same tree (plan 18-07)"
    reason: "Full gate not run in this pass, by instruction. Last proof is abba527. gate.sh itself changed since (PWA suites added)."
    test: "Run scripts/gate.sh and scripts/gate.sh --linux at HEAD, reading each exit code directly"
    expected: "Both exit 0"
    why_human: "Out of scope for this pass; Phase 22 owns the release gate"
coincidental_reliance_items:
  - truth: "Plan 04 readers_parked reporting and plan 08 parked-reader join (removal::tests::reader_parked_on_out_of_group_slave_holder_is_reported, pty::tests::group_extinction_leaves_reader_parked_for_out_of_group_slave_holder)"
    reason: fixture-only
    harden: "The fixture reaps the leader through is_exited() before the call under test. Add a variant that leaves the leader unreaped, which is what the gap above needs anyway."
deferred_resolved:
  - truth: "The TUI and the daemon both call the shared removal path (RMV-05)"
    was_deferred_to: "Phases 20 and 21"
    evidence: "Now wired at HEAD: baude/src/app.rs:4510 and bauded/src/manager.rs:1189 call removal::remove; previews at baude/src/app.rs:4064, 4480 and bauded/src/manager.rs:1115."
advisory:
  - finding: "BL-11 is recorded in .planning/BACKLOG.md as deferred from v2.8 by owner decision on 2026-11-13. This report confirms its cause; it does not overrule that decision."
    category: other
    reason: "If the deferral stands, accept it with the override block in the Gaps Summary so this phase reads passed with one override."
    evidence_status: "reproduced, deterministic artifact in scratchpad"
---

# Phase 18: Shared Kill-Then-Remove Lifecycle Verification Report

**Phase Goal:** `baude-core` can remove any row kind in one operation that kills the owned runtime first, removes managed worktrees from disk, never deletes a main checkout, standalone folder or local branch, and persists atomically, so the TUI and daemon share one removal path.
**Verified:** 2026-11-13
**Status:** gaps_found
**Re-verification:** Yes. The prior report (`passed`, 13/13, HEAD `abba527`, 2026-11-03) was stale after phases 19, 20, 21 and a review-fix pass.
**Code verified at:** HEAD `f934e6474e54f7e195919e7fe4cf9d5779d5d66b` on `gsd/phase-18-shared-kill-then-remove-lifecycle`. No source file was modified by this pass.

## What changed under the phase since `abba527`

`git diff --stat abba527..HEAD` over the covered files:

| File | Change | Consequence for this report |
|------|--------|-----------------------------|
| `baude-core/src/pty.rs`, `git.rs`, `session.rs`, `repository.rs`, `lifecycle.rs`, `persist.rs` | byte-identical | Code evidence stands; tests re-run at HEAD |
| `baude-core/src/removal.rs` | +1429 / -75 | Phase 19 added `planned_clearings` and a fourth `clear_stage`. The 75 removed lines are doc text, imports, the `report_from_set` signature and test helpers moved to `testing.rs`. Stage order re-read below |
| `baude-core/src/testing.rs` | +135 | Parked-reader and unkillable-runtime doubles moved here (phase 20) |
| `scripts/gate.sh` | +15 | PWA suites run first; cargo steps unchanged |

## Goal Achievement

### Observable Truths

Evidence key: **T** = pinned by a test I ran at HEAD, **C** = verified by reading code at HEAD, **N** = not verified in this pass.

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | SC1: removing a row with a live runtime terminates the owned group SIGTERM then SIGKILL on macOS and Linux, no zombie or orphan | ✓ VERIFIED (macOS T; Linux C) | T: `removal::tests::live_runtime_group_is_extinct_after_remove`, `agent_and_shell_are_both_extinct_after_remove`, `pty::tests::group_extinction_after_leader_exit`, `kill_and_wait_confirms_child_exit` green. C: `extinguish_group` (`pty.rs:877-890`) is TERM, 500 ms poll, KILL, 500 ms poll, error. Linux: not run here; the `/proc` arm (`pty.rs:1019-1035`) is byte-identical to the tree the Linux gate passed at `abba527`. Scope note: the truth is about a live runtime. The exited-leader case is truth 13 |
| 2 | SC2: a runtime that survives escalation leaves the row in place with an error | ✓ VERIFIED (T, C) | T: `surviving_runtime_leaves_row_and_disk`, `parent_kill_failure_aborts_before_disk_and_state`. C: `kill_stage` (`removal.rs:916-947`) returns `RuntimeSurvived` before `disk_stage` (`:455`) and `persist_stage` (`:456`). See truth 13 for a false `RuntimeSurvived` on macOS |
| 3 | SC3: managed worktree child deleted even when unclean or cwd-held; main checkout and standalone keep directory and every branch; cloned and `git init` fixtures | ✓ VERIFIED (T, C) | T: `managed_worktree_unclean_and_cwd_held_is_deleted`, `git_init_unclean_cwd_held_managed_is_deleted_and_main_kept`, `main_checkout_keeps_directory_and_branches`, `standalone_keeps_folder`, `unmanaged_checkout_keeps_directory`, `live_main_checkout_runtime_is_killed_and_directory_kept`, `live_standalone_runtime_is_killed_and_folder_kept`, `live_unmanaged_checkout_runtime_is_killed_and_directory_kept`. C: `disk_action_for` (`removal.rs:1088-1093`); standalone rows are `KeepDirectory` (`:348`). Phase 19's `clear_markers` (`:699-750`) only ever targets `worktrees_base()/<workspace>/repository-<key>` through the non-recursive `marker::clear_owned`, so it cannot reach a main checkout or standalone folder |
| 4 | SC4: removing a repository parent removes each child by the per-child rules then the parent, in one operation composed from existing stages | ✓ VERIFIED (T, C) | T: `repository_removes_children_then_parent`, `select_rows_repository_target_fans_out_children_first_parent_last`. C: `select_rows` (`removal.rs:306-356`), `remove` (`:436-469`). Close-protocol symbol count in `removal.rs` is 0 |
| 5 | SC5: a crash mid-removal leaves the prior state or the fully removed state | ✓ VERIFIED (T, C) | T: `crash_before_save_reconciles_missing`, `repository_crash_mid_fan_out_keeps_parent_and_children_in_file`, `repository_disk_failure_mid_fan_out_leaves_state_and_file_untouched`, `save_closure_is_called_exactly_once_for_parent_with_children`. C: `persist_stage` (`removal.rs:1028-1064`) calls `save` once and assigns `*state` only on `Ok` or `replacement_committed()`. `clear_stage` runs after the save and never touches the state file (`:457`, `:662-676`) |
| 6 | Plan 01: side-effect-free `preview_removal`, per-kind classification, serde round-trip, typed `UnknownTarget` | ✓ VERIFIED (T, C) | T: `preview_has_no_side_effects_and_report_round_trips_serde`, `remove_report_matches_preview_rows`, six `select_rows_*` tests. C: phase 19 added reads to the preview (`planned_clearings`, `removal.rs:501-609`): `symlink_metadata` and `breadcrumbs::load_json` (`breadcrumbs.rs:229-238`, read-only). No write, no git, no signal |
| 7 | Plan 02: forced managed-worktree removal with containment, identity re-proof, marker retry, bare, foreign and submodule guards | ✓ VERIFIED (T) | All 30 `git::tests::forced_removal::*` green. `git.rs` unchanged since `abba527`. `"prune"` appears only in two argv-denylist test literals (`git.rs:5338`, `:6388`) |
| 8 | Plan 03: `RepositoryState::remove_rows` batch removal; parent-only rejected; unknown keys no-ops; counters untouched | ✓ VERIFIED (T) | All 4 `repository::tests::remove_rows_*` green. `RemovedRows` `repository.rs:523`, `remove_rows` `:896`. File unchanged since `abba527` |
| 9 | Plan 04: preflight identity match before any signal, candidate `validate_for_save` before any irreversible effect, one save, handles consumed only on success, `readers_parked` reported | ✓ VERIFIED (coincidental-reliance) (T, C) | C: `remove` order at `removal.rs:442-457` is `select_rows`, `planned_clearings` (read-only), `preflight`, clone, `remove_rows`, `validate_for_save`, `kill_stage`, `disk_stage`, `persist_stage`, `clear_stage`. T: `missing_runtime_handle_is_typed_error_before_any_kill`, `mismatched_runtime_identity_is_refused_before_any_kill`, `validation_failure_is_detected_before_any_kill_or_delete`, `vanished_owner_common_dir_is_refused_before_the_kill`, `reader_parked_on_out_of_group_slave_holder_is_reported`. The last one relies on the fixture reaping the leader first; see `coincidental_reliance_items` |
| 10 | Plan 05: every persistence failure mode | ✓ VERIFIED (T) | `rename_failure_keeps_prior_state_then_retry_reports_already_removed`, `sync_failure_keeps_prior_state_and_file_bytes`, `directory_sync_failure_commits_state_and_reports_degraded`, `refused_row_failure_names_row_and_keeps_prior_state` green. `SavedDegraded` is the only committing error branch (`removal.rs:1041`) |
| 11 | Plan 06: keep rules with and without live runtimes, unmanaged and already-removed rows, parent fan-out on both fixture kinds | ✓ VERIFIED (T) | All 11 plan-06 tests present and green, plus `already_removed_managed_row_prunes_git_registration`, `cloned_topology_places_worktrees_under_managed_root`. `testing.rs`: `git_in` `:314`, `ClonedTopology` `:337`, `cloned_topology` `:356`. `lifecycle.rs` diff since `73fa674` is empty |
| 12 | Plan 07: `scripts/gate.sh` green on macOS and `--linux` on the same tree | ? UNCERTAIN (N) | Not run, by instruction. Last proof is `abba527`. Checked at HEAD only: `#[ignore]` count 0 in `removal.rs`, `pty.rs`, `session.rs`; `grace` count 0 in `pty.rs`; the four filtered suites below exit 0 |
| 13 | Plan 08: `Pty::kill_and_wait` reaches group extinction after leader exit, pid-reuse `ForeignOwner`, reader join or parked report, lock-free error arm | ✗ FAILED (partial) | T, green: all 24 `pty::tests::*` and `session::tests::group_extinction_*`. Those cover an exited leader with a live descendant, a reaped leader, pid reuse, reader join and the lock-free error arm. Not covered, and failing on macOS: an exited leader that is the only group member and has not been reaped. `kill_and_wait` returns `failed to terminate exact PTY process group: Operation not permitted (os error 1)`. See BL-11 Root Cause |

**Score:** 11/13 truths verified (1 failed, 1 not re-verified in this pass)

### BL-11 Root Cause

The orchestrator asked whether BL-11 ("failed to terminate exact PTY process group" on macOS teardown) has its cause in this phase's code. It does.

1. **Only one site can produce the message.** `grep` finds the string in production code at `baude-core/src/pty.rs:880` only (`baude/src/remote.rs:2064` is a test literal). It wraps `signal_group(process_group, libc::SIGTERM)`.
2. **Only one errno can reach it.** `signal_group` (`pty.rs:1073-1087`) maps `ESRCH` to `Ok` at `:1081` and returns every other error. `kill(2)` with a valid signal can only fail with `ESRCH` or `EPERM`, so the BL-11 failure is `EPERM` by elimination.
3. **macOS returns `EPERM` for a zombie-only group.** Measured on this host (Darwin 27.3.0) with a forked child that calls `setsid` and exits unreaped: `kill(-pgid, 0)` and `kill(-pgid, SIGTERM)` both fail with `EPERM`; after `waitpid` both fail with `ESRCH`. Linux signals a zombie-only group successfully, which is why this is macOS-only.
4. **The code routes exactly that state to a signal.** `process_group_exists` (`pty.rs:1089-1093`) treats `EPERM` as alive. `group_disposition` then returns `Signal` for an exited leader whether the leader is still inspectable (`:968`) or its group has no live member (`:978`). The comment at `:955-956` states the intent: a zombie-only group "is signalled and reaped like any extinct-but-unreaped group". On macOS the signal call is the thing that fails, and the reap block (`:848-868`) is never reached.
5. **Phase 18 opened the path.** Before plan 18-08 (`213e26e`), `kill_and_wait` began with `child.try_wait()` and returned `Ok` for an already exited leader before any signal. 18-08 removed that so SIGTERM is sent before the child mutex is taken. `signal_group`'s `ESRCH`-only tolerance is older (`6d9ebb4`, phase 06), but only since 18-08 does an unreaped exited leader reach it.
6. **Reproduced through the real code.** An out-of-tree binary depending on `baude-core` by path (production cfg, not `test-support`) spawned `Pty::spawn(Some("sleep 0.05"), ..)` 100 times, waited for `kill(-pgid, 0)` to report `EPERM`, and called `kill_and_wait` without calling `is_exited()`:

   ```text
   rounds 100: zombie-only window observed 90, kill_and_wait Err in window 77, Ok in window 13, window missed 10
   first error: failed to terminate exact PTY process group: Operation not permitted (os error 1)
   retry 200 ms later succeeded: 77 of 77
   ```

7. **Why the suite is green.** `testing::await_parked_holder` (`testing.rs:479-500`) polls `pty.is_exited()`, which reaps the leader, before any test calls `remove` or `kill_and_wait`. Production reaches the call in the window between the leader's exit and the next exit poll or reader reap. This is the fixture-realism failure mode `CLAUDE.md` names.

Blast radius: fail-closed. `kill_stage` aborts before the disk and persist stages, so no row, file or directory changes and nothing is leaked. The user sees a `RuntimeSurvived` error for a runtime that had already exited, and a retry succeeds. Through the daemon it is a 500.

Reproduction artifacts (session scratchpad, not in the repo):
`/private/tmp/claude-503/-Users-joese-Code-github-com-poindexter12-baude/a2f6da22-a53d-4447-9fb2-62cf58c24430/scratchpad/bl11-repro/src/main.rs`,
`/private/tmp/claude-503/-Users-joese-Code-github-com-poindexter12-baude/a2f6da22-a53d-4447-9fb2-62cf58c24430/scratchpad/bl11-repro/race.log`,
`/private/tmp/claude-503/-Users-joese-Code-github-com-poindexter12-baude/a2f6da22-a53d-4447-9fb2-62cf58c24430/scratchpad/py/zombie_group.py`.

### Deferred Items Now Resolved

| # | Item | Was deferred to | Evidence at HEAD |
|---|------|-----------------|------------------|
| 1 | TUI and daemon both call the shared removal path (RMV-05) | Phases 20, 21 | `baude/src/app.rs:4510` and `bauded/src/manager.rs:1189` call `removal::remove`; previews at `baude/src/app.rs:4064`, `:4480`, `bauded/src/manager.rs:1115` |
| 2 | Clearing bindings and marker ownership on removal | Phase 19 | `clear_stage` (`removal.rs:662-676`) runs after `persist_stage` |

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `baude-core/src/removal.rs` | Vocabulary, `select_rows`, `preview_removal`, `remove`, `RowRuntime`, stages | ✓ VERIFIED | 4861 lines, production to `:1167`. `select_rows` `:306`, `preview_removal` `:359`, `remove` `:436`, `preflight` `:840`, `kill_stage` `:916`, `disk_stage` `:964`, `persist_stage` `:1028` |
| `baude-core/src/lib.rs` | `pub mod removal;` | ✓ VERIFIED | `:21` |
| `baude-core/src/git.rs` | `remove_managed_worktree_forced` and guards | ✓ VERIFIED | `forced_remove_arguments` `:2889`, `managed_root_contains` `:3004`, `submodule_guard` `:3250`, `remove_managed_worktree_forced` `:3418`. Unchanged since `abba527` |
| `baude-core/src/repository.rs` | `RemovedRows`, `remove_rows` | ✓ VERIFIED | `:523`, `:896` |
| `baude-core/src/pty.rs` | `GroupDisposition`, `group_disposition`, `group_members`, `reader_finished` | ⚠️ PRESENT, DEFECT | `:926`, `:957`, `:1020`/`:1040`/`:1067`, `:601`. `signal_group` `:1073-1087` is the gap |
| `baude-core/src/session.rs` | `kill_and_wait` contract | ✓ VERIFIED | `:495`. Unchanged since `abba527` |
| `baude-core/src/testing.rs` | `git_in`, `ClonedTopology`, `cloned_topology` | ✓ VERIFIED | `:314`, `:337`, `:356` |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `removal::remove` | `Session::kill_and_wait` | kill stage | ✓ WIRED | `removal.rs:929-935` |
| `removal::remove` | `git::remove_managed_worktree_forced` | disk stage, owner common dir | ✓ WIRED | `removal.rs:977-978` |
| `removal::remove` | `remove_rows` + `validate_for_save` + caller save | clone, mutate, validate before kill, save once | ✓ WIRED | `removal.rs:445-456` |
| `removal::persist_stage` | `SaveError::replacement_committed` | only committing degraded branch | ✓ WIRED | `removal.rs:1041` |
| `pty::kill_and_wait` | `signal_group` + `wait_for_group_extinction` | `Signal` arm only | ⚠️ WIRED, DEFECT | `pty.rs:840`, `:877-890`; `EPERM` not handled at `:1081` |
| `session::kill_and_wait` | `Pty::process_identity` | lock-free error arm | ✓ WIRED | `session::tests::group_extinction_error_path_does_not_take_child_lock` green |
| TUI | `removal::remove` | `baude/src/app.rs:4510` | ✓ WIRED | Phase 20 |
| Daemon | `removal::remove` | `bauded/src/manager.rs:1189` | ✓ WIRED | Phase 21 |

### Behavioral Spot-Checks

Each command ran under the same isolated environment `scripts/gate.sh` builds (`HOME`, `XDG_*`, `CLAUDE_CONFIG_DIR` redirected), single-threaded, with the exit code written to a file by the runner and read from that file, never through a pipe.

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Tests exist | `cargo test -p baude-core --lib -- --list` | exit 0; 561 tests; 49 `removal::`, 30 `forced_removal::`, 24 `pty::`, 4 `remove_rows`, 2 `session::tests::group_extinction_*` | ✓ PASS |
| Removal and forced removal | `cargo test -p baude-core --lib removal:: -- --test-threads=1` | exit 0; `79 passed; 0 failed; 0 ignored` | ✓ PASS |
| PTY kill path | `cargo test -p baude-core --lib pty:: -- --test-threads=1` | exit 0; `24 passed; 0 failed; 0 ignored` | ✓ PASS |
| Batch row removal | `cargo test -p baude-core --lib remove_rows -- --test-threads=1` | exit 0; `4 passed; 0 failed` | ✓ PASS |
| Session teardown | `cargo test -p baude-core --lib session::tests:: -- --test-threads=1` | exit 0; `27 passed; 0 failed` | ✓ PASS |
| Teardown of an exited, unreaped leader | out-of-tree `bl11-repro`, 100 rounds | 77 of 90 in-window calls returned `Err` | ✗ FAIL |
| Full gate, macOS and Linux | `scripts/gate.sh`, `scripts/gate.sh --linux` | not run, by instruction | ? SKIP |

### Probe Execution

No `scripts/*/tests/probe-*.sh` exists and no plan declares one.

### Requirements Coverage

| Requirement | Status | Evidence |
|-------------|--------|----------|
| KILL-01 | ⚠️ PARTIAL | A live runtime is TERM-then-KILL extinguished before any row or file drops (truth 1, T on macOS). An already exited, unreaped runtime fails teardown on macOS (truth 13). Linux not re-run |
| KILL-02 | ⚠️ PARTIAL | Reader join and `readers_parked` proven by test. In the truth 13 failure `kill_and_wait` returns before the reap block, so the zombie leader persists until the next exit poll |
| KILL-03 | ✓ SATISFIED | A survivor leaves the row with a typed error (truth 2). Caveat: truth 13 raises the same error for a runtime that did exit |
| RMV-01 | ✓ SATISFIED | Truth 3, truth 7 |
| RMV-02 | ✓ SATISFIED | Truth 3 |
| RMV-03 | ✓ SATISFIED | Truth 3 |
| RMV-04 | ✓ SATISFIED | Truth 4, truth 8 |
| RMV-05 | ✓ SATISFIED | One `removal::remove`; both callers now wired (see Deferred Items Now Resolved) |
| RMV-06 | ✓ SATISFIED | Truth 5, truth 10 |
| RMV-07 | ✓ SATISFIED | `forced_arguments_never_touch_branches`, `main_worktree_inside_root_is_refused`, keep tests on both fixture kinds |

No orphaned requirements: all ten IDs mapped to Phase 18 in REQUIREMENTS.md are claimed by a plan.

### Prohibitions

| Prohibition | Check at HEAD | Result |
|-------------|---------------|--------|
| `preview_removal` never signals, runs git or writes | read `planned_clearings` and `load_json`; `preview_has_no_side_effects_and_report_round_trips_serde` | kept |
| `select_rows` never classifies Main, unmanaged or standalone as `RemoveManagedWorktree` | `select_rows_classifies_disk_action_by_role_and_management` | kept |
| `remove_dir_all` never on a path that failed a guard | `forced_removal` refusal tests | kept |
| Forced argv has no branch verb; no `refs/heads` entry deleted | `forced_arguments_never_touch_branches`; `git.rs:2889` | kept |
| `remove_rows` never touches `next_*` counters | `remove_rows_unknown_keys_are_no_ops_and_counters_unchanged` | kept |
| `remove` never routes through the close or tombstone protocols | symbol count in `removal.rs` = 0 | kept |
| Save closure at most once, never after an earlier stage error | `save_closure_is_called_exactly_once_for_parent_with_children` and error-path tests | kept |
| No signal before preflight and validation | three `*_before_any_kill*` tests | kept |
| No new grace or timeout knob | `grep -c 'grace\|GRACE' pty.rs` = 0 | kept |
| A foreign process group is never signalled | `group_extinction_disposition_never_signals_foreign_owner` | kept |
| No test ignored to pass a gate | `#[ignore]` count 0, 0, 0 | kept |
| `lifecycle.rs` helpers not modified | `git diff --stat 73fa674..HEAD -- lifecycle.rs` empty | kept |
| Gate steps never judged through a pipe | this pass read every exit code from a file | kept |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `baude-core/src/pty.rs` | 1081 | `ESRCH`-only tolerance on a group signal | 🛑 Blocker | Truth 13, BL-11 |
| `baude-core/src/pty.rs` | 955-956 | Comment asserts behavior that is false on macOS | ⚠️ Warning | Misleads the next reader of the kill path |
| `baude-core/src/testing.rs` | 479-500 | Fixture reaps the leader before the call under test | ⚠️ Warning | Hides the gap from every exited-leader test |

No `TBD`, `FIXME`, `XXX`, `TODO`, `HACK`, `PLACEHOLDER`, `todo!(` or `unimplemented!` in the seven covered source files.

### Human Verification Required

1. **Gate at HEAD.** Run `scripts/gate.sh` and `scripts/gate.sh --linux`, reading each exit code directly. Expected: both exit 0. Not run here by instruction; Phase 22 owns it.

### Gaps Summary

One gap. Twelve of the thirteen must-haves still hold at HEAD or were not in scope to re-run: the removal pipeline's stage order, keep rules, forced worktree removal, batch row removal, single-save atomicity and every plan prohibition were re-located in code and re-proven by 134 tests run in this pass. Phase 19's additions to `removal.rs` did not disturb them: the preview stayed read-only, and the new clearing stage runs after the save and cannot reach a kept directory.

The gap is BL-11, and its cause is in this phase's code. `Pty::kill_and_wait` fails on macOS when the leader has exited and is the only, unreaped member of its group, because `signal_group` (`baude-core/src/pty.rs:1073-1087`) treats the `EPERM` macOS returns for a zombie-only group as a failure, and plan 18-08 removed the early reap that used to keep that state away from it. It fails closed and a retry succeeds, so nothing is deleted or leaked, but removal and session teardown fail intermittently for a row whose agent has just exited.

**The deferral looks intentional.** BL-11 was deferred from v2.8 by owner decision on 2026-11-13. To accept that for this phase, add to this file's frontmatter:

```yaml
overrides:
  - must_have: "Pty::kill_and_wait reaches process-group extinction even when the PTY leader has already exited, and is idempotent on an extinct group"
    reason: "BL-11: macOS EPERM on a zombie-only group. Fails closed, retry succeeds. Deferred from v2.8 by owner decision 2026-11-13."
    accepted_by: "{name}"
    accepted_at: "{ISO timestamp}"
```

---

_Verified: 2026-11-13_
_Verifier: Claude (gsd-verifier)_
