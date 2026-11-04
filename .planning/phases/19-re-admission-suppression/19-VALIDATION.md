---
phase: "19"
slug: "re-admission-suppression"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-11-03"
---

# Phase 19 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (Rust built-in harness; in-module `#[cfg(test)]` suites, `baude_core::testing` fixtures such as `TestRedirect`, `cloned_topology` and `git_in`) |
| **Config file** | none (Cargo workspace; `scripts/gate.sh` is the authoritative runner and already exists) |
| **Quick run command** | `cargo test -p baude-core removal::tests -- --test-threads=1` |
| **Full suite command** | `scripts/gate.sh` (macOS) and `scripts/gate.sh --linux` (rust:1-bookworm, uid 1000) for the symlink, `remove_dir` error-kind and lock-file paths |
| **Estimated runtime** | quick command measured at 81 s for the 39 `removal::tests` on the Phase 18 tree (warm cache, `--test-threads=1`); the single-test filters most tasks use finish in a few seconds after the incremental build; `scripts/gate.sh` several minutes; `--linux` adds the container build |

---

## Sampling Rate

- **After every task commit:** Run the task's own `<automated>` command (the plan-local filter: `removal::tests`, `folder_workspace::tests`, `suppression::tests`, `marker::tests`, `breadcrumbs::tests`, or the `-p baude app::tests::<name>` filters for TUI tasks), always with `-- --test-threads=1`, followed by `cargo fmt --check`
- **After every plan wave:** Run `cargo clippy --all-targets -- -D warnings && cargo test -- --test-threads=1` (RESEARCH "Sampling Rate")
- **Before `/gsd-verify-work`:** `scripts/gate.sh` and `scripts/gate.sh --linux` must both exit 0, read directly, never through a pipe (plan 19-07; the Linux lane uses the manual container variant from 18-07-SUMMARY if the bind-mount race recurs)
- **Max feedback latency:** 90 seconds for the quick command

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 19-01-01 | 01 | 1 | READM-01, READM-05 | T-19-01, T-19-02, T-19-03 | binding removed by exact `folder_key` equality only, through `locked_merge_write`; the neighbour entry sharing the workspace value is byte-identical; the stage runs after `persist_stage` and has no `Result` return; preview names the same cleared item | unit, tracer (TDD RED then GREEN) | `cargo test -p baude-core removal::tests::clear_stage_removes_repository_binding_and_reports_it -- --test-threads=1 && cargo test -p baude-core removal::tests::preview_has_no_side_effects_and_report_round_trips_serde -- --test-threads=1` | ✅ `baude-core/src/removal.rs`, `baude-core/src/folder_workspace.rs` (tests authored in-module by the task) | ⬜ pending |
| 19-01-02 | 01 | 1 | READM-01, READM-05 | T-19-01, T-19-03 | checkout and standalone rows clear only their own key; absent key or absent file writes nothing (D-03); a lock-file failure lands in `clearing_failures` with `remove` Ok, `persistence` Saved and the save closure run once | unit (TDD RED then GREEN) | `cargo test -p baude-core removal::tests -- --test-threads=1 && cargo test -p baude-core folder_workspace::tests -- --test-threads=1` | ✅ | ⬜ pending |
| 19-01-03 | 01 | 1 | READM-01 | — | no behaviour change; planned list derived once per entry point; `clear_stage(planned)` carries no unused parameter under `-D warnings` | unit (REFACTOR, conditional) | `cargo test -p baude-core removal::tests -- --test-threads=1 && cargo fmt --check && cargo clippy -p baude-core --all-targets -- -D warnings` | ✅ | ⬜ pending |
| 19-02-01 | 02 | 1 | READM-03, READM-04 | T-19-04, T-19-05, T-19-06 | five tests describe consult, lift, suppress, garbage-collect and corrupt-file degrade before the store exists | unit (TDD RED) | `cargo test -p baude-core suppression::tests -- --test-threads=1; test $? -ne 0` (expects compile errors naming the store items) | ❌ created by this task (`baude-core/src/suppression.rs` is new; registered in `lib.rs` by the same task) | ⬜ pending |
| 19-02-02 | 02 | 1 | READM-03, READM-04 | T-19-04, T-19-05, T-19-06 | `is_suppressed` has no error path and reads a corrupt or absent file as empty; every write goes through `locked_merge_write`; `lift` and `drop_children` write nothing when nothing changes; the module never imports `persist::` | unit (TDD GREEN) | `cargo test -p baude-core suppression::tests -- --test-threads=1` | ✅ | ⬜ pending |
| 19-02-03 | 02 | 1 | READM-03 | — | no behaviour change | unit (REFACTOR, conditional) | `cargo test -p baude-core suppression::tests -- --test-threads=1 && cargo fmt --check && cargo clippy -p baude-core --all-targets -- -D warnings` | ✅ | ⬜ pending |
| 19-03-01 | 03 | 1 | READM-02 | T-19-07, T-19-08, T-19-09 | six tests describe owned, foreign, invalid and symlinked marker, missing directory, non-empty directory and symlinked directory before `clear_owned` exists | unit (TDD RED) | `cargo test -p baude-core marker::tests::clear_owned -- --test-threads=1; test $? -ne 0` | ✅ `baude-core/src/marker.rs` | ⬜ pending |
| 19-03-02 | 03 | 1 | READM-02 | T-19-07, T-19-08, T-19-09 | marker deleted only on `Valid` plus byte-equal owner (D-06); `symlink_metadata` refusal before any read (D-08); non-recursive `remove_dir` with `DirectoryNotEmpty` reported as `NotEmpty`, never retried (D-07) | unit (TDD GREEN) | `cargo test -p baude-core marker::tests -- --test-threads=1` | ✅ | ⬜ pending |
| 19-03-03 | 03 | 1 | READM-02 | — | no behaviour change | unit (REFACTOR, conditional) | `cargo test -p baude-core marker::tests -- --test-threads=1 && cargo fmt --check && cargo clippy -p baude-core --all-targets -- -D warnings` | ✅ | ⬜ pending |
| 19-04-01 | 04 | 2 | READM-02, READM-03, READM-05 | T-19-10, T-19-11, T-19-12 | six tests: owned marker and directory cleared and `lifecycle::ensure_repository` then reports no collision for the cleared directory and allocates the un-suffixed path with a fresh marker; child removal keeps the parent marker and writes a suppression entry; parent removal drops child entries and standalone writes none; refusals and non-empty directories reported, not raised; directory from the stored `physical_key` with `../escape` refused; D-15 neighbours byte-identical | unit, `git init` and cloned fixtures (TDD RED) | `cargo test -p baude-core removal::tests -- --test-threads=1; test $? -ne 0` (expects the six new tests `FAILED`, no compile error) | ✅ | ⬜ pending |
| 19-04-02 | 04 | 2 | READM-02, READM-03, READM-05 | T-19-10, T-19-11, T-19-12 | `repository_dir` single-component guard before any join under the managed root; `clear_stage(removed, state, planned)` maps every marker, directory and suppression outcome into `cleared` or `failures` and never returns early; stage still after `persist_stage`; no `remove_dir_all` in the production region | unit (TDD GREEN) | `cargo test -p baude-core removal::tests -- --test-threads=1` | ✅ | ⬜ pending |
| 19-04-03 | 04 | 2 | READM-02 | — | no behaviour change; orchestrator keeps the stage order if split into per-kind helpers | unit (REFACTOR, conditional) | `cargo test -p baude-core removal::tests -- --test-threads=1 && cargo fmt --check && cargo clippy -p baude-core --all-targets -- -D warnings` | ✅ | ⬜ pending |
| 19-05-01 | 05 | 2 | READM-03, READM-04 | T-19-13, T-19-14 | five `App` tests: the sweep skips a suppressed path; explicit admission of that path lifts the entry and admits a row with fresh identity; the parent root does not lift a child entry; `LaunchDirectory` and `CloneCompletion` lift; a lift failure is a message, not an admission error | integration, `App` over `TestRedirect` (TDD RED) | `cargo test -p baude app::tests::suppressed_worktree_is_skipped_by_admission_sweep app::tests::admitting_suppressed_path_lifts_entry_and_admits_fresh_identity app::tests::admitting_parent_root_does_not_lift_child_entry app::tests::launch_dir_and_clone_completion_routes_lift_suppression app::tests::lift_failure_is_a_message_not_an_admission_error -- --test-threads=1; test $? -ne 0` | ✅ `baude/src/app.rs` | ⬜ pending |
| 19-05-02 | 05 | 2 | READM-03, READM-04 | T-19-13, T-19-14 | the consult after the `owned` check only removes a sweep candidate (D-11); `lift_suppression` returns unit and reports errors with `set_message` (D-09); the lift precedes the `existing_key` match and so both sweep sites (D-12) | integration (TDD GREEN) | `cargo test -p baude app::tests -- --test-threads=1 2>/tmp/baude-19-05-tests.log; STATUS=$?; tail -n 5 /tmp/baude-19-05-tests.log; exit $STATUS` | ✅ | ⬜ pending |
| 19-05-03 | 05 | 2 | READM-04 | — | no behaviour change | lint (REFACTOR, conditional) | `cargo fmt --check && cargo clippy -p baude --all-targets -- -D warnings` | ✅ | ⬜ pending |
| 19-06-01 | 06 | 3 | READM-03 | T-19-15 | a second `App` over the same redirect shows none of the four removed rows; `startup_recovery_program` is empty; the reconcile passes leave the key sets unchanged; the unmanaged worktree directory is still on disk (D-16) | integration, two `App`s | `cargo test -p baude app::tests::removed_rows_stay_removed_across_relaunch -- --test-threads=1` | ✅ | ⬜ pending |
| 19-06-02 | 06 | 3 | READM-04 | T-19-15 | relaunch inside the removed checkout lifts the entry and admits exactly one row with a new key and order, `resume_id` None and no inherited runtime (D-13) | integration, two `App`s | `cargo test -p baude app::tests::relaunch_inside_removed_checkout_readmits_it_fresh -- --test-threads=1` | ✅ | ⬜ pending |
| 19-06-03 | 06 | 3 | READM-03 (D-13 crumb) | — | the existing `flush` prunes the removed path's crumb; no eager crumb clearing added to `removal.rs` | unit | `cargo test -p baude-core breadcrumbs::tests::flush_drops_crumb_of_removed_path -- --test-threads=1` | ✅ `baude-core/src/breadcrumbs.rs` | ⬜ pending |
| 19-07-01 | 07 | 4 | READM-01, READM-02, READM-03, READM-04, READM-05 | T-19-16 | gate leak check: no fixture outside the redirected roots (D-08, D-15 at the estate level); every phase commit subject parses | gate | `scripts/gate.sh` | ✅ `scripts/gate.sh` | ⬜ pending |
| 19-07-02 | 07 | 4 | READM-01, READM-02, READM-03, READM-04, READM-05 | T-19-16 | symlink refusals, `DirectoryNotEmpty` mapping and lock-file semantics proven on Linux as uid 1000; macOS gate re-run on the same HEAD | gate (container) | `scripts/gate.sh --linux` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Requirement Scope Notes

| Requirement | Scope note | Source |
|-------------|-----------|--------|
| READM-02 | Success criterion 1, "marker recovery does not restore the row", is observed in 19-04-01 test 1 through `lifecycle::ensure_repository`, the production caller of `check_collision`: after the clear it returns no `CollisionReport` and allocates the un-suffixed `repository-<digest>` with a fresh marker. `check_collision` and `worktree_scan::discover_owner` are module-private, so the public admission path is the reader the test exercises; `read_marker` reporting `Missing` (19-03-02) is the precondition, not the observation. | 19-04-PLAN.md Task 1; RESEARCH "Phase Requirements" READM-02 row |

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements. No `<automated>` command carries a `MISSING — Wave 0` sentinel: every TDD plan's RED task authors its tests inside the module it changes (`removal::tests`, `folder_workspace::tests`, `marker::tests`, `breadcrumbs::tests`, `app::tests`), `baude-core/src/suppression.rs` and its `suppression::tests` are created by task 19-02-01, and the shared fixtures (`RemovalFixture`, `testing::cloned_topology`, `admission_repo`, `admission_repo_cloned`, the two-`App` restart pattern) already exist from Phases 8 and 18. `scripts/gate.sh` already exists. Framework install: none.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| — | — | — | — |

All phase behaviors have automated verification. The only environment dependency is Docker for `scripts/gate.sh --linux` (plan 19-07 task 2 precondition); if Docker is unavailable the Linux gate is reported as a blocker, not skipped.

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies (20 tasks across 7 plans, 20 `<automated>` commands, each with `<fails_when>`)
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references (none emitted)
- [x] No watch-mode flags
- [x] Feedback latency < 90s for the quick command (81 s measured)
- [ ] `nyquist_compliant: true` set in frontmatter (set by validate-phase after execution)

**Approval:** pending
