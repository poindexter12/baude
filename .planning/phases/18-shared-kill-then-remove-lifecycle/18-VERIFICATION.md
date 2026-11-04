---
phase: 18-shared-kill-then-remove-lifecycle
verified: 2026-11-03T19:05:00Z
status: passed
score: 13/13 must-haves verified
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
covered_digest: "v4:sha256:28e6680ffa1314d3bb81272ea3429d82eb235973d2d5a1f70b9212a10464e125"
behavior_unverified: 0
overrides_applied: 0
behavior_unverified_items: []
human_verification: []
gate_proofs:
  - gate: "scripts/gate.sh on macOS"
    commit: abba527
    exit: 0
    log: "scratchpad/gate-abba527.log, last line GATE_EXIT=0"
    suites: "baude-core 537 passed 1 ignored, integration 249 passed, bauded 107 passed, remaining binaries 16, 14, 2, 1, 1 passed, 0 failed"
  - gate: "scripts/gate.sh in rust:1-bookworm as uid 1000"
    commit: abba527
    exit: 0
    log: "scratchpad/linux-gate-abba527.log, last line LINUX_GATE_EXIT=0"
    suites: "baude-core 537 passed 1 ignored, integration 249 passed, bauded 107 passed, remaining binaries 16, 14, 2, 1, 1 passed, 0 failed"
    note: "Unmodified scripts/gate.sh run through the same docker invocation as --linux except CARGO_TARGET_DIR=/tmp/linux-gate/target, which keeps the build off the Docker Desktop bind mount. Covers the /proc arm of pty::group_members added by 7c7ecab."
re_verified: 2026-11-03T21:40:00Z
deferred:
  - truth: "The TUI and the daemon both call the shared removal path"
    addressed_in: "Phases 20 and 21"
    evidence: "18-CONTEXT.md Phase Boundary: 'The TUI (Phase 20) and daemon (Phase 21) call this one path.' RMV-05's phase-18 obligation is the shared baude-core operation and its serializable report, both present."
  - truth: "Re-admission suppression: clearing folder-workspace bindings and marker ownership on removal"
    addressed_in: "Phase 19"
    evidence: "18-CONTEXT.md: 'Baude's own markers and breadcrumbs inside a kept folder are left in place in this phase. Clearing ownership is Phase 19.'"
advisory:
  - finding: "scripts/gate.sh --linux keeps the container's CARGO_TARGET_DIR on the Docker Desktop bind mount, which makes the lane unrunnable verbatim on a macOS host"
    category: other
    reason: "Recorded in deferred-items.md and WINDOWS.md entries 13 and 14 with a one-line fix (named volume or container-local target dir). Out of plan 07's file allowlist; excluded from this phase's gaps by explicit instruction."
    evidence_status: "diagnosed, fix not applied"
---

# Phase 18: Shared Kill-Then-Remove Lifecycle Verification Report

**Phase Goal:** `baude-core` can remove any row kind in one operation that kills the owned runtime first, removes managed worktrees from disk, never deletes a main checkout, standalone folder or local branch, and persists atomically, so the TUI and daemon share one removal path.
**Verified:** 2026-11-03T19:05:00Z
**Status:** passed
**Re-verification:** Yes — the two human items resolved by full gate runs at HEAD `abba527` on both platforms (see Gate Proofs at HEAD)
**Code verified at:** HEAD `abba527`, working tree clean. All claims below were read from code at HEAD, not from SUMMARY.md.

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | SC1 — removing a row with a live runtime terminates the owned process group SIGTERM then SIGKILL on macOS **and Linux**, no zombie or orphan | ✓ VERIFIED | macOS proven: `removal::tests::live_runtime_group_is_extinct_after_remove`, `agent_and_shell_are_both_extinct_after_remove`, `pty::tests::group_extinction_after_leader_exit` all ran green at HEAD (exit 0 captured directly). Linux half proven at HEAD: the full gate in `rust:1-bookworm` as uid 1000 exited 0 at `abba527` with 537 core tests passed, exercising `pty::group_members` has a `cfg(target_os = "linux")` `/proc` arm (`pty.rs:1019-1035`) added in 7c7ecab. See Gate Proofs at HEAD. |
| 2 | SC2 — a runtime that survives escalation leaves the row in place with an error instead of dropping the row and leaking the process | ✓ VERIFIED | `removal::tests::surviving_runtime_leaves_row_and_disk` and `parent_kill_failure_aborts_before_disk_and_state` green at HEAD. `remove` (`removal.rs:396`) returns `RemovalError::RuntimeSurvived` from `kill_stage` before `disk_stage` or `persist_stage` is reached, so no state, file or directory change precedes the error. |
| 3 | SC3 — a managed worktree child is deleted even when unclean or runtime-held cwd; a main checkout or standalone row keeps its directory and every local branch; proven on cloned and `git init` fixtures | ✓ VERIFIED | `managed_worktree_unclean_and_cwd_held_is_deleted`, `git_init_unclean_cwd_held_managed_is_deleted_and_main_kept`, `main_checkout_keeps_directory_and_branches`, `standalone_keeps_folder`, `unmanaged_checkout_keeps_directory`, `live_main_checkout_runtime_is_killed_and_directory_kept`, `live_standalone_runtime_is_killed_and_folder_kept` — all green at HEAD. `disk_action_for` (`removal.rs:715`) returns `KeepDirectory` for `(Main, _)` and `(_, false)`; `forced_remove_arguments` (`git.rs:2889`) is exactly `worktree remove --force --force --`, no branch verb. |
| 4 | SC4 — removing a repository parent removes each child by the per-child rules then the parent, in one operation composed from the existing kill, close and managed-worktree removal stages | ✓ VERIFIED | `repository_removes_children_then_parent` green at HEAD. `select_rows` (`removal.rs:267`) fans out children in state order then pushes the parent last; `remove` composes `Session::kill_and_wait`, `git::remove_managed_worktree_forced`, `RepositoryState::remove_rows` and the caller's `persist` save. `grep -c 'destructive_teardown\|revoke_removal_authority\|RemovalTombstone\|mark_teardown_pending' removal.rs` = 0, so it is not a parallel protocol. |
| 5 | SC5 — a crash mid-removal leaves either the prior state or the fully removed state, never a parent without children or a child without its parent | ✓ VERIFIED | `crash_before_save_reconciles_missing`, `repository_crash_mid_fan_out_keeps_parent_and_children_in_file`, `repository_disk_failure_mid_fan_out_leaves_state_and_file_untouched`, `save_closure_is_called_exactly_once_for_parent_with_children` green at HEAD. `persist_stage` (`removal.rs:655`) calls `save` once with the whole candidate and replaces `*state` only on `Ok` or `replacement_committed()`. |
| 6 | Plan 01 — side-effect-free `preview_removal`, per-kind classification, serde round-trip, typed `UnknownTarget` | ✓ VERIFIED | `preview_has_no_side_effects_and_report_round_trips_serde`, `remove_report_matches_preview_rows`, `select_rows_*` (6 tests) all present and green. `RemovalTarget`/`RowKey`/`DiskAction`/`PlannedRow`/`RemovalSet`/`KilledRuntime`/`PersistenceOutcome`/`RemovalReport`/`DiskRemovalFailure*`/`RemovalError` all defined with `Serialize, Deserialize` (`removal.rs:42-200`). `PlannedRow.runtime` is the whole `OwnedRuntime`. |
| 7 | Plan 02 — forced managed-worktree removal with containment, live identity re-proof, marker retry, bare/foreign/submodule guards | ✓ VERIFIED | 30 `git::tests::forced_removal::*` tests present; `vanished_main_checkout_still_removes_managed_child`, `missing_target_removal_keeps_sibling_whose_directory_is_absent`, `worktree_with_submodule_gitdirs_is_refused_untouched`, `symlinked_managed_root_is_followed`, `forced_arguments_never_touch_branches` ran green at HEAD. `grep '"prune"'` finds no `git worktree prune` call (CR-01 fix confirmed in code at `git.rs:3374-3390`). |
| 8 | Plan 03 — `RepositoryState::remove_rows` batch removal; parent-only rejected by `validate`; unknown keys no-ops; counters untouched | ✓ VERIFIED | `remove_rows` at `repository.rs:896`; `RemovedRows` at `:523`. All 4 `repository::tests::remove_rows_*` tests present. No `next_*_key` assignment inside `remove_rows`. |
| 9 | Plan 04 — preflight full-identity match before any signal, candidate `validate_for_save` before any irreversible effect, one save, handles consumed only on success, `readers_parked` reported | ✓ VERIFIED | `remove` (`removal.rs:396-423`) order read directly: `select_rows` → `preflight` → clone → `remove_rows` → `validate_for_save` → `kill_stage` → `disk_stage` → `persist_stage`. `missing_runtime_handle_is_typed_error_before_any_kill`, `mismatched_runtime_identity_is_refused_before_any_kill`, `validation_failure_is_detected_before_any_kill_or_delete`, `reader_parked_on_out_of_group_slave_holder_is_reported` green at HEAD (exit 0 captured directly). |
| 10 | Plan 05 — every persistence failure mode: rename, sync, directory-sync degraded, refused row, retry after a failed save | ✓ VERIFIED | All 9 named plan-05 tests exist; `rename_failure_keeps_prior_state_then_retry_reports_already_removed`, `sync_failure_keeps_prior_state_and_file_bytes`, `directory_sync_failure_commits_state_and_reports_degraded`, `refused_row_failure_names_row_and_keeps_prior_state` enumerated and the subset run was green. `PersistenceOutcome::SavedDegraded` is the only committing error branch (`removal.rs:674`). |
| 11 | Plan 06 — keep rules with and without live runtimes, unmanaged and already-removed rows, parent fan-out on both fixture kinds | ✓ VERIFIED | `testing.rs` provides `git_in` (`:311`), `ClonedTopology` (`:334`), `cloned_topology` (`:353`). All 11 named plan-06 tests exist; 7 ran green at HEAD. `git log --grep='^(test\|fix)\(18-06\):' -- lifecycle.rs` = 0 and `git diff --stat 73fa674..HEAD -- lifecycle.rs` is empty, so the lifecycle-helpers prohibition held. |
| 12 | Plan 07 — `scripts/gate.sh` green on macOS and `--linux` green in `rust:1-bookworm`, both on the phase's final tree | ✓ VERIFIED | 18-07-SUMMARY.md records both green at `f1351d2`. Six review-fix commits (`40642a7`, `ff192e9`, `0ddf485`, `b925c73`, `89271fe`, `7c7ecab`) landed after it, touching `git.rs`, `pty.rs` and `removal.rs`. Both gates re-run at HEAD `abba527`: macOS `scripts/gate.sh` exit 0 and the unmodified gate in `rust:1-bookworm` as uid 1000 exit 0, each exit read directly from the redirected log. Also confirmed at HEAD: `cargo fmt --all --check` exit 0; `grep -c '#[ignore'` = 0 in `removal.rs`, `pty.rs`, `session.rs`; `grep -c 'grace\|GRACE' pty.rs` = 0. See Gate Proofs at HEAD. |
| 13 | Plan 08 — `Pty::kill_and_wait` group extinction after leader exit, pid-reuse `ForeignOwner` disposition, reader join or parked report, lock-free error arm | ✓ VERIFIED | `GroupDisposition { Signal, Extinct, ForeignOwner, Unverifiable }` (`pty.rs:926-934`); `group_disposition` (`:957`) returns `Unverifiable` on inspection error and `ForeignOwner` when no live member sits in the recorded session (WR-01 fix); `reader_finished` (`:601`). `group_extinction_joins_reader`, `group_extinction_leaves_reader_parked_for_out_of_group_slave_holder`, `session::tests::group_extinction_drains_agent_and_shell_readers`, `group_extinction_after_leader_exit`, `group_disposition_requires_a_member_in_the_recorded_session` all green at HEAD (exit 0 captured directly). |

**Score:** 11/13 truths verified (2 present, behavior-unverified)

### Deferred Items

| # | Item | Addressed In | Evidence |
|---|------|-------------|----------|
| 1 | TUI and daemon both call the shared removal path | Phases 20 and 21 | 18-CONTEXT.md Phase Boundary names Phase 20 (TUI) and Phase 21 (daemon) as the callers. RMV-05's phase-18 obligation is the shared `baude-core` operation and its serializable report, both verified present. |
| 2 | Clearing folder-workspace bindings and marker ownership on removal | Phase 19 | 18-CONTEXT.md: markers and breadcrumbs inside a kept folder are deliberately left in place this phase. |

### Advisory (New Scope, Unevidenced)

| # | Finding | Category | Why Advisory |
|---|---------|----------|--------------|
| 1 | `scripts/gate.sh --linux` keeps the container `CARGO_TARGET_DIR` on the Docker Desktop bind mount, so the verbatim lane is unrunnable on a macOS host | other | Diagnosed in deferred-items.md and WINDOWS.md 13/14 with a one-line fix; outside plan 07's allowlist and excluded from this phase's gaps by explicit instruction |

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `baude-core/src/removal.rs` | Vocabulary, `select_rows`, `preview_removal`, `remove`, `RowRuntime`, stage functions | ✓ VERIFIED | 3576 lines (786 production, rest tests). Every type and function named across plans 01, 04, 05 present and read. |
| `baude-core/src/lib.rs` | `pub mod removal;` | ✓ VERIFIED | Line 21. |
| `baude-core/src/git.rs` | `ForcedRemoval*`, `remove_managed_worktree_forced`, containment, marker and guard helpers | ✓ VERIFIED | All named symbols present (`:2717`-`:3545`). `submodule_guard` and `resolve_managed_root` added by the review fixes; no `git worktree prune` call remains. |
| `baude-core/src/repository.rs` | `RemovedRows`, `RepositoryState::remove_rows` | ✓ VERIFIED | `:523` and `:896`. |
| `baude-core/src/pty.rs` | `GroupDisposition`, `group_disposition`, `group_members`, `reader_finished`, retained reader handle | ✓ VERIFIED | `:601`, `:610`, `:926`, `:957`, `:1019`/`:1039`/`:1066` (Linux/macOS/other arms). |
| `baude-core/src/session.rs` | `kill_and_wait` contract, pids from `process_identity()`, two new tests | ✓ VERIFIED | `:495`; error arm reads `self.claude.process_identity().pid` at `:516-520`, never `Pty::pid`. |
| `baude-core/src/testing.rs` | `git_in`, `ClonedTopology`, `cloned_topology` | ✓ VERIFIED | `:311`, `:334`, `:353`; `worktrees_base_override` at `:196` redirects the managed root. |
| `scripts/gate.sh` | Authoritative phase gate (unchanged; plan 07 runs it) | ✓ WIRED | Unchanged and present; run green at HEAD on both platforms (see truth 12). |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `removal::select_rows` | `repository::RepositoryState` | pure read of repositories, checkouts, standalone_sessions, role, `managed_by_baude`, `owned_runtime` | ✓ WIRED | `removal.rs:267-316` reads all three collections; one `symlink_metadata` probe only, via `probe_disk`. |
| `removal::remove` | `session::Session::kill_and_wait` | kill stage; `SessionTeardownError` wrapped as `RuntimeSurvived` | ✓ WIRED | `removal.rs:557-562`. |
| `removal::remove` | `git::remove_managed_worktree_forced` | disk stage for `RemoveManagedWorktree` and `AlreadyRemoved` | ✓ WIRED | `removal.rs:604`, called with the owner **common dir** (WR-03 fix), not the main worktree. |
| `removal::remove` | `repository::remove_rows` + `validate_for_save` + caller save closure | clone, mutate, validate before kill, save once | ✓ WIRED | `removal.rs:408-421`. |
| `removal::persist_stage` | `persist::SaveError::replacement_committed` | the only committing degraded branch | ✓ WIRED | `removal.rs:674-676`. |
| `git::remove_managed_worktree_forced` | `git::worktrees_base` / `testing::worktrees_base_override` | canonicalized containment before any effect | ✓ WIRED | `managed_root_contains` (`git.rs:3004`) is the first statement of the function. |
| `pty::kill_and_wait` | `signal_group` + `wait_for_group_extinction` | exited-leader branch routes through TERM-then-KILL | ✓ WIRED | `pty.rs:810-896`; `Signal` is the only arm reaching `signal_group`. |
| `session::kill_and_wait` | `pty::Pty::process_identity` | error arm reads pids lock-free | ✓ WIRED | `session.rs:516-520`. |
| `scripts/gate.sh --linux` | `pty::kill_and_wait` / `inspect_process_identity` on Linux procfs | removal live-runtime cases in the container | ✓ WIRED | Exercised at HEAD `abba527` through the container variant with the target dir off the bind mount; the Linux `group_members` arm added at `7c7ecab` ran green. |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|--------|--------------------|--------|
| `removal.rs` | `RemovalReport.rows_removed` | `set.rows` from `select_rows` reading `RepositoryState` | ✓ | ✓ FLOWING |
| `removal.rs` | `RemovalReport.runtimes_killed` | real run: pids from `handle.session.*.process_identity()`; preview: persisted `OwnedRuntime` | ✓ | ✓ FLOWING |
| `removal.rs` | `RemovalReport.paths_deleted` / `already_removed` | `disk_stage` branches on `ForcedRemoval.deleted_directory` from the helper's own `symlink_metadata`, not on the plan (WR-05 fix) | ✓ | ✓ FLOWING |
| `removal.rs` | `RemovalReport.readers_parked` | `Pty::reader_finished()` polled on each owned handle before it drops | ✓ | ✓ FLOWING |
| `removal.rs` | `RemovalReport.persistence` | the caller's `save` closure result over `persist::StateFile` | ✓ | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Phase tests exist and are not ignored | `cargo test -p baude-core --lib -- --list` | 127 phase-relevant tests enumerated; every test name declared in plans 01-06 and 08 present | ✓ PASS |
| 23 load-bearing invariants pass at HEAD | `cargo test -p baude-core --lib -- --exact <23 names>` | `test result: ok. 23 passed; 0 failed` | ✓ PASS |
| Kill / reaper / reader invariants pass at HEAD (exit captured directly) | `cargo test -p baude-core --lib -- --exact <6 kill/reader names> > log` | `EXIT=0`; `6 passed; 0 failed` | ✓ PASS |
| Formatting clean at HEAD | `cargo fmt --all --check` | exit 0 | ✓ PASS |
| Full macOS gate at HEAD | `scripts/gate.sh` | `GATE_EXIT=0`; 537 core, 249 integration, 107 bauded passed, 0 failed | ✓ PASS |
| Linux gate at HEAD | `scripts/gate.sh` in `rust:1-bookworm` as uid 1000, `CARGO_TARGET_DIR=/tmp/linux-gate/target` | `LINUX_GATE_EXIT=0`; 537 core, 249 integration, 107 bauded passed, 0 failed | ✓ PASS |

### Probe Execution

| Probe | Command | Result | Status |
|-------|---------|--------|--------|
| n/a | — | No `scripts/*/tests/probe-*.sh` exists in this repo and no plan declares one | n/a |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| KILL-01 | 18-04, 18-05, 18-06, 18-07, 18-08 | Terminates the owned group SIGTERM then SIGKILL before any row or file drops, on macOS and Linux | ✓ SATISFIED | Proven green at HEAD on both platforms: macOS gate exit 0 and the Linux container gate exit 0, the latter covering the `/proc` `group_members` arm. |
| KILL-02 | 18-04, 18-06, 18-07, 18-08 | PTYs and reader threads closed, no zombie or orphan attributable to the row | ✓ SATISFIED | `group_extinction_joins_reader`, `reader_parked_on_out_of_group_slave_holder_is_reported`, `session::tests::group_extinction_drains_agent_and_shell_readers` green at HEAD. The documented scope carve-out (out-of-group slave holder reported in `readers_parked`) is implemented in `persist_stage`, matching REQUIREMENTS.md line 25. |
| KILL-03 | 18-04, 18-05, 18-07 | A surviving runtime leaves the row in place with a visible error | ✓ SATISFIED | `surviving_runtime_leaves_row_and_disk`, `parent_kill_failure_aborts_before_disk_and_state` green at HEAD; `RemovalError::RuntimeSurvived` carries the row and the boxed `SessionTeardownError`. |
| RMV-01 | 18-02, 18-06 | A managed worktree child with a live runtime or unclean tree is removed from disk | ✓ SATISFIED | `managed_worktree_unclean_and_cwd_held_is_deleted`, `inside_root_unclean_tree_is_removed_and_branch_kept`, `locked_worktree_inside_root_is_removed`. |
| RMV-02 | 18-01, 18-06 | Removing a main checkout drops the row and runtime without deleting the directory or any branch | ✓ SATISFIED | `main_checkout_keeps_directory_and_branches`, `live_main_checkout_runtime_is_killed_and_directory_kept`; `disk_action_for` returns `KeepDirectory` for `CheckoutRole::Main`. |
| RMV-03 | 18-01, 18-06 | Removing a standalone folder row drops the row and runtime without deleting the folder | ✓ SATISFIED | `standalone_keeps_folder`, `live_standalone_runtime_is_killed_and_folder_kept`; `select_rows` gives every standalone row `DiskAction::KeepDirectory`. |
| RMV-04 | 18-01, 18-03, 18-05, 18-06 | Removing a repository parent removes every child by the per-child rules then the parent | ✓ SATISFIED | `repository_removes_children_then_parent`, `remove_rows_repository_with_children_leaves_no_row_with_that_key_and_validates`, `remove_rows_parent_only_is_rejected_by_validate_as_dangling`. |
| RMV-05 | 18-01, 18-04 | A shared `baude-core` lifecycle operation composed from the existing stages, not a parallel path | ✓ SATISFIED | One public `removal::remove` + `preview_removal`; composes `Session::kill_and_wait`, `git::remove_managed_worktree_forced`, `RepositoryState::remove_rows`, `persist`. `RemovalReport` is fully serde round-tripped for the Phase 21 REST contract. TUI/daemon call sites are Phase 20/21 by design (see Deferred). |
| RMV-06 | 18-03, 18-04, 18-05 | Atomic state-file write per removed row set; a crash leaves prior or fully removed state | ✓ SATISFIED | One `save` per `remove`, asserted by `save_closure_is_called_exactly_once_for_parent_with_children`; `crash_before_save_reconciles_missing` and `repository_crash_mid_fan_out_keeps_parent_and_children_in_file` load the file and assert `validate()` Ok with every prior row present. |
| RMV-07 | 18-02, 18-06 | Never deletes a main checkout, a standalone folder or a local branch; tests on cloned and `git init` fixtures | ✓ SATISFIED | `forced_arguments_never_touch_branches` green; the forced argv is `worktree remove --force --force --` with no branch verb; `main_worktree_inside_root_is_refused`; keep tests on both fixture kinds (`cloned_topology` and `git_init_unclean_cwd_held_managed_is_deleted_and_main_kept`). |

No orphaned requirements: all ten IDs mapped to Phase 18 in REQUIREMENTS.md appear in at least one plan's `requirements` field, and all ten are accounted for above.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| — | — | None found | — | `TBD`, `FIXME`, `XXX`, `TODO`, `HACK`, `PLACEHOLDER`, `todo!(`, `unimplemented!` all absent from all seven changed files. `#[ignore]` count 0 in `removal.rs`, `pty.rs`, `session.rs`. No stub returns, no hardcoded empty data feeding the report. |

Prohibition checks (all plan prohibitions verified at HEAD):

| Prohibition | Check | Result |
|-------------|-------|--------|
| `remove` never routes through the close or tombstoned-removal protocols | `grep -c 'destructive_teardown\|revoke_removal_authority\|RemovalTombstone\|mark_teardown_pending' removal.rs` | 0 ✓ |
| No new grace period or timeout knob | `grep -c 'grace\|GRACE' pty.rs` | 0 ✓ |
| No test skipped or ignored to pass a gate | `grep -c '#[ignore' removal.rs pty.rs session.rs` | 0, 0, 0 ✓ |
| The forced argv never contains a branch-mutating verb | read `forced_remove_arguments` (`git.rs:2889`) | `worktree remove --force --force --` ✓ |
| `lifecycle.rs` helpers not modified or moved | `git diff --stat 73fa674..HEAD -- lifecycle.rs` | empty ✓ |
| No repository-wide `git worktree prune` in the forced path (CR-01) | `grep '"prune"' git.rs` | only two argv-denylist test literals; no prune invocation ✓ |
| `remove_rows` never touches the `next_*` counters | read `repository.rs:896-935` | no counter assignment ✓ |

### Gate Proofs at HEAD

Both items the initial report routed to human verification were resolved by running the full gate at HEAD `abba527` on each platform, with the exit code appended to the redirected log rather than read through a pipe.

| Gate | Invocation | Exit | Suites |
|------|------------|------|--------|
| macOS | `scripts/gate.sh` | `GATE_EXIT=0` | fmt, clippy `-D warnings`, commit-message checks, isolated suite: 537 core passed 1 ignored, 249 integration, 107 bauded, 16, 14, 2, 1, 1 passed, 0 failed; both real-roots checks clean |
| Linux | unmodified `scripts/gate.sh` inside `rust:1-bookworm` as uid 1000, same `docker run` as `--linux` except `CARGO_TARGET_DIR=/tmp/linux-gate/target` | `LINUX_GATE_EXIT=0` | same suites, same counts, 0 failed |

The Linux run executes the `cfg(target_os = "linux")` `/proc` arm of `pty::group_members` introduced by `7c7ecab`, which no earlier Linux gate covered. Both results describe the same commit, satisfying plan 07's must-have. The verbatim `--linux` lane remains blocked by the documented Docker Desktop bind-mount defect, which is recorded as an advisory item and deferred, not a phase gap.

### Gaps Summary

No gaps. Every artifact exists, is substantive, is wired, and the data reaching `RemovalReport` comes from the stages that actually ran rather than from the plan. All ten requirement IDs are satisfied, and every plan prohibition holds at HEAD. The code review's one Critical and five Warnings are confirmed fixed in code at HEAD, not merely claimed: no repository-wide prune remains (CR-01), `group_disposition` proves group membership by session (WR-01), `submodule_guard` refuses an absorbed-submodule worktree (WR-02), every forced git command runs against the common dir with a pre-kill probe (WR-03), a symlinked managed root is re-rooted rather than refused (WR-04), and the disk report follows what the helper did (WR-05). Each fix carries its own fixture test, and the SUMMARY shapes those fixes superseded were checked against the code, not taken on trust.

The initial report's single open question was proof coverage, not implementation: the phase's gate contract wants both platform gates green on the final tree. Both gates have now run green at HEAD `abba527`, including the Linux-only `/proc` arm, so the report is `passed`.

---

_Verified: 2026-11-03T19:05:00Z_
_Verifier: Claude (gsd-verifier)_
