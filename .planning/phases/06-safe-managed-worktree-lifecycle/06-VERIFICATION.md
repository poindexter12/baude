---
phase: 06-safe-managed-worktree-lifecycle
verified: 2026-09-01T00:00:00Z
status: human_needed
score: 19/21 must-haves verified
behavior_unverified: 1
overrides_applied: 0
behavior_unverified_items:
  - truth: "Protected recovery states cannot be overwritten by occupied activation reuse (SC2, 'reuse or activation cannot overwrite or bypass them')"
    test: "Put a checkout into a protected state (e.g. TeardownPending or RemovalTombstone), then activate the same branch again from repository context so the engine's occupied-reuse rebind path hits the protected occupant"
    expected: "Activation is refused with LifecycleError::OccupiedProtected naming the occupying checkout and its cause; the protected checkout's health/lifecycle is unchanged; App shows the typed 'protected … state' refusal (app.rs activation_refusal)"
    why_human: "The guard exists (baude-core/src/lifecycle.rs:1858-1870, added post-review in b661784) and is wired to a typed App refusal (baude/src/app.rs:4148), but no test in the workspace references OccupiedProtected — reopen-bypass is tested (tombstone/teardown-pending tests) but the occupied-rebind overwrite vector is not"
human_verification:
  - test: "Adjudicate the three stale Critical findings in 06-REVIEW.md (issues_found) whose fix report 06-REVIEW-FIX.md says none_fixed, against the current-tree evidence below, and record the disposition (re-run the deep review or annotate the artifacts)"
    expected: "CR-01: closed by b661784 (OccupiedProtected guard, untested — see behavior_unverified item). CR-02: refuted in the current tree — manager::tests::lifecycle_close_manager_persistence_failure_retains_child_and_parent (bauded/src/manager.rs:3264) opens a shell and asserts BOTH original agent and shell PIDs still live with lifecycle Running after an injected pre-replacement close-save failure, and asserts resume_id retention on the committed branch; the file is byte-identical to the review snapshot, so either the finding was wrong or the pinning test proves the scenario cannot occur. CR-03: the flagged App cleanup path was rewritten (app.rs changed ~3400 lines since the review snapshot); typed StoppedActiveRecovery{agent_restarted, shell_restarted} is persisted and app::tests::lifecycle_remove_clean_local_stop_git_and_compensation_failures_preserve_context plus lifecycle_remove_local_partial_teardown_is_durable_and_retryable pass"
    why_human: "SC5 requires 'clean review … no unresolved lifecycle ownership gaps'; the review artifacts formally stand unresolved even though code evidence contradicts them — closing a Critical review finding as stale is a reviewer/owner decision, not a verifier decision"
  - test: "Confirm Linux/runtime certification (draft PR #56 CI: Linux gate/release matrix, descendant process-group extinction) is green before checking off CORE-03/CORE-06 and Phase 6 completion"
    expected: "Linux CI matrix green on PR #56; VALIDATION morning-gate rows flip to observed"
    why_human: "cfg(linux) process-identity path (fixed in 0c24995) cannot be executed on this macOS host; VALIDATION.md explicitly scopes this as a pending certification gate outside local implementation evidence"
---

# Phase 6: Shared Lifecycle Core Refactor — Verification Report

**Phase Goal:** One enforceable `baude-core` lifecycle ownership protocol/state machine governs Git topology, persistence commit stages, exact agent and shell process ownership, protected recovery transitions, startup recovery, and rollback, while App and Manager remain thin effect adapters with equivalent contracts.
**Verified:** 2026-09-01 (against HEAD b5e99d2 on gsd/phase-07-local-tui-dogfood-release)
**Status:** human_needed
**Re-verification:** No — initial verification (no prior VERIFICATION.md)

Verification was goal-backward against the current codebase. SUMMARY claims were not trusted: every truth below was checked by reading the production code and by running the exact named tests in-process (27 targeted exact-test runs plus one full workspace run, all green on macOS).

## Goal Achievement

### Observable Truths — ROADMAP Success Criteria

| # | Truth | Status | Evidence |
| --- | ----- | ------ | -------- |
| SC1 | Same lifecycle request → same legal transition, persistence boundary, effect order, typed refusal through App and Manager (mirrored contract tests) | ✓ VERIFIED | `app::tests::lifecycle_protocol_contract_app_vectors` (baude/src/app.rs:4961) and `manager::tests::lifecycle_protocol_contract_manager_vectors` (bauded/src/manager.rs:2548) both iterate `canonical_lifecycle_contract_vectors()` (Activate/Launch/Close/ProtectedRefusal) plus injected `AdapterFailureScript::Persist(1)` and `Effect(1)`, comparing normalized traces and final lifecycle against `run_canonical_lifecycle_contract`. Both ran and passed. |
| SC2 | Protected recovery states (teardown-pending, removal tombstone, activation recovery, stopped-active recovery) move only through one explicit legal transition table; reuse/activation cannot overwrite or bypass them | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED (one clause) | All four states exist as typed `UnavailableCause` variants (repository.rs:107-159). `lifecycle::tests::lifecycle_protocol_core_legal_transition_table`, `removal_tombstone_cannot_reopen_or_regain_management`, `teardown_pending_cannot_reopen_and_completes_inactive_before_explicit_reopen`, and `persist::tests::lifecycle_schema_v1_migrates_protected_states_to_v3` all pass. The occupied-reuse OVERWRITE guard (`OccupiedProtected` refusal, lifecycle.rs:1867) is present and wired but has zero test coverage — see behavior_unverified item. |
| SC3 | Before any destructive/replacement effect, exact agent and shell ownership is durable; neither adapter can forget a possibly-live process until confirmed teardown or durable successor | ✓ VERIFIED | Pre-exec gate is real: PTY child blocks on a private stdin token read (`GATE_SCRIPT`, pty.rs:77-78); `register()` durably records `ProcessIdentity` (pid/pgid/session/start-time) BEFORE the token is written; registration failure kills the child (pty.rs:145-157). Whole-group extinction via `libc::kill(-pgid, …)` with TERM→KILL escalation (pty.rs:352-417). Tests passed: `pty::tests::pre_exec_registration_gate_owner_death_and_release`, `session::tests::lifecycle_process_contract_exact_ownership`, `lifecycle_remove_local/manager_partial_teardown_is_durable_and_retryable` (both adapters), and `lifecycle_close_manager_persistence_failure_retains_child_and_parent` (both agent and shell PIDs live after failed pre-replacement close-save). |
| SC4 | Crash/injected failure at any persistence/effect boundary recovers on startup or rolls back to truthful Git/durable-state/agent/shell ownership without a duplicate or orphaned runtime | ✓ VERIFIED | `lifecycle::tests::lifecycle_startup_recovery_is_idempotent` (asserts the recovery program empties and stays empty), mirrored Persist/Effect injection vectors (SC1 tests), `lifecycle_creation_rollback_{local,manager}_precommit_save_failure_has_no_partial_child` and `…committed_save_and_spawn_failures_retain_retry_child` (enumerated), `app::tests::lifecycle_remove_clean_local_rechecks_after_stop_and_compensates_a_race`, `…stop_git_and_compensation_failures_preserve_context`, and `activation_recovery_reuses_unchanged_preexisting_worktree_after_pending_save_crash` all passed in-process. |
| SC5 | Typed lifecycle candidates and provenance survive restart without a generic runtime overlay; clean review plus phase verification find no unresolved lifecycle ownership gaps | ? UNCERTAIN (review clause) | First clause VERIFIED: `blocked_activation_retry_round_trips_all_provenance`, schema v1→v3 and v2→v3 migration tests, `standalone_active_runtime_is_restored_with_exact_recorded_teardown`, `manager_restore_reconciles_current_git_before_spawn_and_persists_failure` (enumerated); no generic runtime overlay exists (only unrelated UI/PR "overlay" strings in meta.rs/ui.rs). Second clause NOT satisfiable by the verifier: 06-REVIEW.md stands at `issues_found` (3 Critical) and 06-REVIEW-FIX.md at `none_fixed`. Current-tree evidence contradicts all three findings (see human_verification), but the artifact disposition is unresolved. |

### Observable Truths — Plan must_haves (06-01 … 06-07)

| # | Truth (plan) | Status | Evidence |
| --- | ----------- | ------ | -------- |
| 1 | 06-01: Valid named branch created from verified default branch and opened in active backend | ✓ VERIFIED | `git::tests::lifecycle::branch_activation::new_branch_from_child_starts_at_freshly_verified_default`, `add_commands_are_explicit_and_never_force_reset_fetch_or_delete` (enumerated, and in passing full suite); `lifecycle::prepare_activation`/`execute_activation` wired from both App and Manager (`activate_branch` refs: app 22, manager 11). |
| 2 | 06-01: Eligible existing local branch activates without reset; occupied same-repo branch reuses its checkout | ✓ VERIFIED | `existing_local_branch_activates_without_resetting_oid`, `occupied_branch_reuses_inventory_record_without_second_add` (real-Git tests, passing in full suite). |
| 3 | 06-02: Invalid refs, path collisions, unsafe occupancy refused with no partial child/runtime | ✓ VERIFIED | `remote_only_and_previous_checkout_shorthand_are_rejected`, `rejects_unregistered_managed_path_collision`, typed `PathCollision` in git.rs; `lifecycle_creation_rollback_*_precommit_save_failure_has_no_partial_child` in both adapters. |
| 4 | 06-02: Concurrent creation uses fresh topology; no duplicate paths/children/runtimes | ✓ VERIFIED | `RepositoryReservations` per-repository serialization + rediscovery before add (lifecycle.rs `reserve`/`discover_repository` in the activation path); `reopen_reservation_allows_only_one_same_checkout_spawn_path` passed. |
| 5 | 06-03: Every unsafe Git state (tracked/staged/unstaged/untracked/ignored/conflicted/submodule/locked/prunable/malformed/indeterminate) blocks removal | ✓ VERIFIED | Result-valued `inspect_removal` + `RemovalSafety`/`RemovalBlocker` taxonomy in git.rs with a real-Git matrix: `only_empty_valid_status_is_clean_and_malformed_output_fails_closed`, `staged_and_unstaged_changes_are_distinct_blockers`, `untracked_ignored_conflicted_and_unusual_names_block`, `every_submodule_row_blocks_…`, `main_external_locked_detached_and_stale_facts_never_become_safe`, `process_start_and_nonzero_status_are_indeterminate` (all in passing suite). Post-plan deviation: baude's OWN pure seed files (`.claude/settings.local.json`, `.mcp.json`) are exempted when provably pure seed (commit 6014b63, user-approved 2026-09-01); pinned by `baude_seeded_artifacts_are_exempt_and_cleared_by_plain_removal`, `hook::tests::pure_seed_settings_predicate_accepts_only_baude_seeds`, `permission::tests::pure_seed_mcp_predicate_accepts_only_baude_seed` — all run and passed. A user-modified seed fails its predicate and keeps blocking. |
| 6 | 06-03: Preflight cannot report safe unless exact managed linked-worktree ownership and every inspection are conclusive | ✓ VERIFIED | `only_exact_managed_linked_topology_produces_an_opaque_target`; fail-closed porcelain=v2 parsing; indeterminate states block. |
| 7 | 06-04: Close stops only its runtime after durable inactive intent is committed | ✓ VERIFIED | Engine orders persist-before-effect (LifecycleEngine::drive; lifecycle.rs:191 comment "recovery must see the candidate that authorized the effect"); `lifecycle_close_local_obeys_persistence_commit_boundary` and `close_preserves_hierarchy_and_orders_snapshot_save_before_stop` ran and passed. |
| 8 | 06-04: Retained checkout preserves branch, first-seen order, shell/archive settings, exact conversation-resume metadata | ✓ VERIFIED | `RetainedSessionState` with opaque `resume_id` (defaulted for old schema: `close_schema_defaults_missing_resume_id_and_round_trips_opaque_value` passed); `lifecycle_close_local_snapshots_resume_context_and_retains_hierarchy` and `lifecycle_close_manager_success_retains_exact_child_context` ran and passed; snapshot preserves `shell_open: session.shell_open` (manager.rs retained_runtime_snapshot). |
| 9 | 06-04: Pre-commit persistence failure leaves running session and durable child unchanged | ✓ VERIFIED | `lifecycle_close_manager_persistence_failure_retains_child_and_parent` (manager.rs:3264) injects `AtomicFailure::Rename` pre-replacement and asserts SAME agent AND shell PIDs live, lifecycle still `Running`, state equal to before — ran and passed. |
| 10 | 06-05: Retained checkout reconciles before durable activation; exactly one runtime | ✓ VERIFIED | `reopen_blocks_every_unavailable_topology_before_active_intent`, `reopen_saves_active_intent_before_deterministic_runtime_dispatch`, `reopen_reservation_allows_only_one_same_checkout_spawn_path` — first two ran and passed, third in passing suite; `reconcile_checkout` wired in both adapters. |
| 11 | 06-05: Claude Code and OpenCode target retained conversation ID; directory-latest only when no ID observed | ✓ VERIFIED | Typed `SpawnMode::ResumeId`/`ContinueLatest` in backend/mod.rs; manager restore selects `ResumeId(saved.resume_id)` falling back to `ContinueLatest` (manager.rs:1474-1478); `backend::claude::tests::targeted_resume_is_opaque_environment_data` and `backend::opencode::tests::spawn_cmd_pins_port_and_resume_flag` (passing suite); opaque env transport via `Pty::spawn_registered_with(env)` — no shell interpolation of the resume ID. |
| 12 | 06-05: Moved/branch-changed/detached/locked/prunable/identity-changed retained children remain unavailable, do not launch | ✓ VERIFIED | `git::tests::reconciliation::missing_changed_detached_and_locked_checkouts_fail_closed` (passing suite); `reopen_blocks_every_unavailable_topology_before_active_intent` ran and passed; `lifecycle_capabilities_expose_only_dispatchable_reopen_and_recovery_actions` gates dispatch. |
| 13 | 06-06: Distinct confirmation removes only clean verified baude-managed linked worktree; never deletes branch or parent | ✓ VERIFIED | UI `Modal::ConfirmRemoveWorktree` (ui.rs:1557) consumes a fresh `RemovalConfirmation` token from `prepare_remove_worktree` (both adapters); `plain_remove_preserves_exact_branch_parent_and_sibling` and `ui::tests::hierarchy_modals_name_exact_targets_and_distinguish_close_from_remove` (passing suite). |
| 14 | 06-06: Preflight runs live and again immediately after stop; second-stage/Git failure restores one usable runtime and retains context | ✓ VERIFIED | Manager/App `confirm_remove_worktree`: snapshot → RequestClose/teardown → `inspect_confirmed_removal` (post-stop re-preflight) → `revoke_removal_authority` (durable) → `execute_verified_removal` → postconditions, with `compensate_failed_removal` restoring the runtime including `saved.shell_open` (manager.rs:1540-1700, app.rs:2791-2830, 3835, 4019). `lifecycle_remove_clean_local_rechecks_after_stop_and_compensates_a_race` and `…stop_git_and_compensation_failures_preserve_context` ran and passed. |
| 15 | 06-06: Postconditions prove exact checkout absent, branch and parent remain, truthful commit stage reported | ✓ VERIFIED | `remove_verified` postconditions in git.rs; `changed_branch_oid_after_git_remove_is_visible_degradation`, `externally_recreated_git_worktree_is_reported_and_preserved`, `recreated_path_is_reported_and_never_recursively_deleted` (passing suite); degraded outcome `TopologyCommittedStateDegraded` persisted with truthful stage (manager.rs). |
| 16 | 06-07: Local source passes formatting and all workspace tests (Clippy/Linux cert pending externally) | ✓ VERIFIED | Run by verifier on HEAD b5e99d2: `cargo fmt --all -- --check` → clean; full `cargo test` → exit 0, 53 (baude) + 218 (baude-core) + 79 (bauded) = 350 tests, 0 failures. Clippy not re-run by verifier (CI-covered); Linux certification deferred to PR #56. |

**Score:** 19/21 truths verified (1 present-but-behavior-unverified, 1 uncertain pending human review adjudication)

### Required Artifacts

| Artifact | Expected | Status | Details |
| -------- | -------- | ------ | ------- |
| `baude-core/src/lifecycle.rs` | Sole reducer/driver, effect contract, recovery program, canonical vectors, normalization | ✓ VERIFIED | 2973 lines. `reduce_lifecycle`, `LifecycleEngine`, `LifecycleEffects`, `startup_recovery_program`, `canonical_lifecycle_contract_vectors()`, `normalize_lifecycle_trace()`, `RepositoryReservations`, `RemovalConfirmation` all present, substantive, wired from both adapters. |
| `baude-core/src/git.rs` | Branch classification, verified add/reuse, result-valued removal inspection, plain remove, postconditions | ✓ VERIFIED | 4107 lines. `BranchActivation`, `classify_branch`/`activate_branch`, `inspect_removal`, `RemovalSafety`, `RemovalBlocker`, `PathCollision`, `remove_verified` + real-Git test matrix. Seed exemption (6014b63) is content-verified per call, fail-closed on any anomaly. |
| `baude-core/src/repository.rs` | Strict schema CheckoutLifecycle, OwnedRuntime, generation, typed candidates/provenance | ✓ VERIFIED | `CheckoutLifecycle` (Inactive/Active/Activating/Launching/Running/Stopping/RemovalCommitted/Protected), `OwnedRuntime`, `RuntimeGeneration`, `UnavailableCause` with all four protected recovery states carrying exact provenance (pids, identities, created_branch, preexisting owner). |
| `baude-core/src/persist.rs` | Checked schema migration, idempotent atomic rewrite | ✓ VERIFIED | `SCHEMA_VERSION` now 3 (Phase 7 bump — forward drift, not a gap): checked v1→v3 and v2→v3 migration chain with protected-state fixtures; `lifecycle_schema_v1_migrates_protected_states_to_v3` and `schema_v2_migrates_strictly_to_v3` ran and passed (the VALIDATION-named `lifecycle_schema_v2_migrates_protected_states` was renamed accordingly). |
| `baude-core/src/session.rs` | Exact two-process ownership, stop, restore, PID-reuse-safe acknowledgement | ✓ VERIFIED | `kill_and_wait` produces exact agent+shell teardown evidence (pids + `ProcessIdentity`); `lifecycle_process_contract_exact_ownership` ran and passed. cfg(linux) identity path compiles per 0c24995 (behavioral Linux proof deferred to PR #56). |
| `baude-core/src/pty.rs` | Private-stdin pre-exec gate, negative-PGID whole-group teardown | ✓ VERIFIED | `spawn_registered_with`: gate token released only after `register(&identity)` succeeds; identity must own its pgid+session or child is killed; `signal_group(-pgid)` TERM→KILL with extinction polling. `pre_exec_registration_gate_owner_death_and_release` ran and passed. |
| `baude-core/src/backend/mod.rs` | Typed Fresh/ContinueLatest/ResumeId spawn mode | ✓ VERIFIED | `SpawnMode::ResumeId` present, used by both adapters. |
| `baude/src/app.rs` | Thin local LifecycleEffects adapter, mirrored vectors, activation/close/reopen/remove adapters | ✓ VERIFIED | LifecycleEngine/LifecycleEffects wiring; mirrored vector test; prepare/confirm removal; typed refusal copy including `OccupiedProtected` (line 4148) and blocker-specific removal refusals. |
| `baude/src/ui.rs` | Distinct target-naming remove confirmation, typed blocker presentation | ✓ VERIFIED | `Modal::ConfirmRemoveWorktree` (line 1557); `hierarchy_modals_name_exact_targets_and_distinguish_close_from_remove` in passing suite. |
| `bauded/src/manager.rs` | Thin daemon adapter, mirrored vectors, shared safe-remove without Phase 8 API expansion | ✓ VERIFIED | Same engine wiring, mirrored vector test; `prepare_remove_worktree`/`confirm_remove_worktree` marked `#[cfg_attr(not(test), allow(dead_code))]` — internal parity, network entrypoint deliberately deferred to Phase 8 (documented in-source). |
| `06-VALIDATION.md` | Observed local evidence with certification honestly pending | ✓ VERIFIED | Present; all local gates checked, morning gates honestly unchecked. |

### Key Link Verification

| From | To | Via | Status | Details |
| ---- | -- | --- | ------ | ------- |
| baude/src/app.rs | lifecycle.rs | LifecycleEngine requests + typed effect acks | ✓ WIRED | `drive_lifecycle_effect` → `LifecycleEngine::drive`; no direct adapter-side transition decisions found. |
| bauded/src/manager.rs | lifecycle.rs | Same contract, same vectors, same normalization | ✓ WIRED | Identical `drive_lifecycle_effect` shape (manager.rs:322-351); mirrored test compares against the same canonical set. |
| lifecycle.rs | persist.rs | `replacement_committed` preserved through acks | ✓ WIRED | 8 core + 22 app + 12 manager references; post-replacement failures produce degraded-but-truthful outcomes (close test's `committed` branch). |
| lifecycle.rs | pty.rs | Paused gate identity persisted before release; failed persistence stops it | ✓ WIRED | `LaunchRegistered(OwnedRuntime)` driven inside the `register` closure BEFORE token release (manager.rs:1198-1226, app equivalent); registration error kills the paused child. |
| ui.rs | lifecycle.rs | Confirmation consumes fresh preflight token | ✓ WIRED | `RemovalConfirmation` from `prepare_removal`, re-inspected by `inspect_confirmed_removal` after stop — not a cached authorization. |
| lifecycle.rs | git.rs | preflight #1 → stop → preflight #2 → plain remove → postconditions | ✓ WIRED | Read directly in `confirm_remove_worktree` (manager.rs:1540-1700); race compensation test passed. |
| RetainedSessionState.resume_id | Backend::spawn_plan | typed ResumeId | ✓ WIRED | manager.rs:1474-1478 and app equivalents; opaque env transport, no command interpolation. |

### Behavioral Spot-Checks (exact named tests, run in-process)

| Behavior | Test | Status |
| -------- | ---- | ------ |
| One legal transition table | `lifecycle::tests::lifecycle_protocol_core_legal_transition_table` | ✓ PASS |
| Protected-state migration | `persist::tests::lifecycle_schema_v1_migrates_protected_states_to_v3`, `schema_v2_migrates_strictly_to_v3` | ✓ PASS |
| Exact two-process ownership | `session::tests::lifecycle_process_contract_exact_ownership` | ✓ PASS |
| Pre-exec gate + owner death + release | `pty::tests::pre_exec_registration_gate_owner_death_and_release` | ✓ PASS |
| Idempotent startup recovery | `lifecycle::tests::lifecycle_startup_recovery_is_idempotent` | ✓ PASS |
| Mirrored App vectors incl. injected failures | `app::tests::lifecycle_protocol_contract_app_vectors` | ✓ PASS |
| Mirrored Manager vectors incl. injected failures | `manager::tests::lifecycle_protocol_contract_manager_vectors` | ✓ PASS |
| Activation persists once, reuses runtime | `app::tests::lifecycle_create_activate_local_persists_once_and_reuses_runtime` | ✓ PASS |
| Removal rechecks after stop, compensates race | `app::tests::lifecycle_remove_clean_local_rechecks_after_stop_and_compensates_a_race` | ✓ PASS |
| Flat daemon compatibility 503s | `api::tests::real_atomic_persistence_failures_are_503_for_every_mutation` | ✓ PASS |
| Tombstone cannot reopen/regain management | `lifecycle::tests::removal_tombstone_cannot_reopen_or_regain_management` | ✓ PASS |
| Teardown-pending cannot reopen | `lifecycle::tests::teardown_pending_cannot_reopen_and_completes_inactive_before_explicit_reopen` | ✓ PASS |
| Provenance round-trips through blocked retry | `lifecycle::tests::blocked_activation_retry_round_trips_all_provenance` | ✓ PASS |
| Close snapshot-before-stop ordering | `lifecycle::tests::close_preserves_hierarchy_and_orders_snapshot_save_before_stop` | ✓ PASS |
| Reopen saves intent before dispatch | `lifecycle::tests::reopen_saves_active_intent_before_deterministic_runtime_dispatch` | ✓ PASS |
| Seed exemption (post-plan contract, 6014b63) | `git::…::baude_seeded_artifacts_are_exempt_and_cleared_by_plain_removal`, `hook::tests::pure_seed_settings_predicate_accepts_only_baude_seeds`, `permission::tests::pure_seed_mcp_predicate_accepts_only_baude_seed` | ✓ PASS |
| Partial teardown durable + retryable (both adapters) | `app::tests::lifecycle_remove_local_partial_teardown_is_durable_and_retryable`, `manager::tests::lifecycle_remove_manager_partial_teardown_is_durable_and_retryable` | ✓ PASS |
| Restore with exact recorded teardown | `app::tests::standalone_active_runtime_is_restored_with_exact_recorded_teardown` | ✓ PASS |
| Close-save failure keeps agent AND shell live (CR-02 refutation) | `manager::tests::lifecycle_close_manager_persistence_failure_retains_child_and_parent` + `…success_retains_exact_child_context` | ✓ PASS |
| Close persistence commit boundary (App) | `app::tests::lifecycle_close_local_obeys_persistence_commit_boundary`, `…snapshots_resume_context_and_retains_hierarchy` | ✓ PASS |
| Stop/Git/compensation failures preserve context | `app::tests::lifecycle_remove_clean_local_stop_git_and_compensation_failures_preserve_context` | ✓ PASS |
| Activation recovery after pending-save crash | `app::tests::activation_recovery_reuses_unchanged_preexisting_worktree_after_pending_save_crash` | ✓ PASS |
| Ownership cannot move to external path | `app::tests::managed_checkout_ownership_cannot_move_to_an_external_path` | ✓ PASS |
| Full workspace gate | `cargo fmt --all -- --check` + `cargo test` (once) | ✓ PASS — exit 0; 53 + 218 + 79 = 350 tests, 0 failures |

### Probe Execution

No `scripts/*/tests/probe-*.sh` probes exist in this repository and none are declared in the phase plans. SKIPPED (no probes to run).

### Requirements Coverage

| Requirement | Source Plan | Status | Evidence |
| ----------- | ----------- | ------ | -------- |
| CORE-01 (one authoritative core protocol) | 06-07 | ✓ SATISFIED | LifecycleEngine sole reducer; both adapters route through `drive_lifecycle_effect`; mirrored vectors pass. |
| CORE-02 (one legal transition table; illegal rejected without side effects) | 06-07 | ✓ SATISFIED (with SC2 caveat) | Transition-table test + tombstone/teardown bypass tests pass; occupied-rebind overwrite guard untested (behavior_unverified item). |
| CORE-03 (write-ahead exact process ownership) | 06-07 | ✓ SATISFIED (macOS) | Pre-exec gate + ownership + partial-teardown tests pass; Linux group-extinction certification pending on PR #56. |
| CORE-04 (mirrored contract tests for success/failure/rollback/restart) | 06-07 | ✓ SATISFIED | Mirrored vector tests with Persist/Effect injection; rollback and restore tests in both adapters. |
| CORE-05 (typed candidates + provenance; no generic overlay) | 06-07 | ✓ SATISFIED | Typed `UnavailableCause` provenance round-trips; migrations normalize legacy fields from authoritative lifecycle; no overlay reconstruction found. |
| CORE-06 (startup recovery/rollback legal, convergent, no duplicate/orphan) | 06-07 | ✓ SATISFIED (macOS) | Idempotent recovery program; process-first recovery (`startup_recovery_program` precedes activation); restart tests pass. Linux pending. |
| WORK-01..WORK-06 | 06-01..06-06 | Pre-built here; checkoff owned by Phase 7 | REQUIREMENTS.md maps WORK-* to Phase 7; the mechanics verified above (truths 1-15) are the substrate Phase 7's UAT certifies. Not orphaned. |

### Anti-Patterns Found

None. Zero `TBD|FIXME|XXX|HACK|PLACEHOLDER|todo!|unimplemented!` and zero `TODO` in any phase-modified source file. The `#[cfg_attr(not(test), allow(dead_code))]` markers on Manager's remove/close internals are documented Phase 8 deferrals, not stubs. The 06-03 deferred item (legacy `git::is_dirty` App removal route) is fully resolved — `is_dirty` no longer exists anywhere in the workspace.

### Accepted Post-Plan Deviations (do not flag as drift)

1. **Seed-file removal-preflight exemption** (commit 6014b63, user-approved 2026-09-01): baude-seeded `.claude/settings.local.json` / `.mcp.json` no longer block removal when content is provably the pure seed; verified removal deletes pure seeds before the plain non-force `git worktree remove`. Content-verified fresh on every preflight; any user modification fails the predicate and blocks. Pinned by three passing tests.
2. **Schema v2 → v3** (Phase 7 standalone sessions, 82faef9): the 06-07-promised "schema-v1 to schema-v2" migration evolved into a checked v1→v3 / v2→v3 chain; the protected-state migration contract is preserved and tested under the renamed tests.
3. **Linux cfg fix** (0c24995): compile fix in the cfg(linux) process-identity path; certification runs on draft PR #56.

### Human Verification Required

#### 1. Adjudicate the stale Critical review findings (SC5)

**Test:** Review the current-tree evidence for CR-01/CR-02/CR-03 from 06-REVIEW.md (status `issues_found`, fix report `none_fixed`) and record a disposition — re-run the independent deep review or annotate the artifacts.
**Expected:** CR-01 closed by the post-review `OccupiedProtected` guard (b661784); CR-02 refuted by the passing close-persistence-failure test that pins agent AND shell survival (manager.rs:3264 — the flagged file is byte-identical to the review snapshot, so the pinning test either refutes the finding or the finding was wrong); CR-03's flagged App path was rewritten (~3400 lines changed since the review snapshot) with typed `StoppedActiveRecovery` persistence and passing compensation-failure tests.
**Why human:** Closing Critical review findings as stale is a reviewer/owner decision. The VALIDATION morning gate "independent deep lifecycle review — zero unresolved Critical/High findings" is explicitly still open.

#### 2. Exercise the occupied-reuse overwrite guard (SC2)

**Test:** Drive a checkout into a protected state (TeardownPending or RemovalTombstone), then activate the same branch again from repository context.
**Expected:** Typed `OccupiedProtected` refusal naming the occupying checkout and its recovery cause; protected state unchanged; App shows the "protected … state" message. Recommend adding an exact test for this vector (it is the only protected-state entry path without one).
**Why human:** Guard present and wired (lifecycle.rs:1867, app.rs:4148) but zero test coverage for this exact vector.

#### 3. Confirm Linux certification before requirement checkoff

**Test:** Verify draft PR #56 CI (Linux gate/release matrix, descendant process-group extinction) is green.
**Expected:** Green matrix; VALIDATION morning-gate rows updated.
**Why human:** Cannot execute cfg(linux) paths on this macOS host; scoped as an external pending gate by 06-VALIDATION.md.

### Gaps Summary

No blocking gaps. Every promised artifact exists, is substantive, and is wired; every plan truth and roadmap success criterion except two has direct passing behavioral evidence run in this verification (27 exact-test executions plus one full 350-test workspace run, all green on macOS under the default backend). The two open items are certification/process items, not missing implementation: (1) the formally unresolved 06-REVIEW Critical findings, which current-tree code and tests contradict but which need an owner disposition, and (2) the untested `OccupiedProtected` occupied-reuse overwrite vector. Linux runtime certification remains an explicitly scoped external gate (PR #56).

---

_Verified: 2026-09-01_
_Verifier: Claude (gsd-verifier), goal-backward against HEAD b5e99d2_

---

## Addendum — 2026-09-02 adjudication and gap closure

Both items this report routed to a human are now closed:

1. The three standing 06-REVIEW.md Criticals were adjudicated by the owner's
   direction and closed as fixed-in-tree with commit-dated evidence and
   executed tests; see the "Adjudication — 2026-09-02" section of
   06-REVIEW.md. Review status is now `resolved`, satisfying SC5.
2. The `OccupiedProtected` occupied-reuse guard is no longer
   behavior-unverified: `occupied_protected_checkout_refuses_activation_overwrite`
   (execute path, commit `57b7c1c`) and
   `occupied_protected_checkout_blocks_activation_recovery_merge`
   (recovery path, this change) both pass, each asserting the protected
   occupant survives untouched.

Linux runtime certification remains the one external gate (draft PR #56).
