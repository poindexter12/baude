---
phase: 20-tui-remove-action
verified: 2026-11-05T09:03:05Z
status: human_needed
score: 19/19 must-haves verified
covered_files:
  - ".planning/phases/20-tui-remove-action/20-01-PLAN.md"
  - ".planning/phases/20-tui-remove-action/20-01-SUMMARY.md"
  - ".planning/phases/20-tui-remove-action/20-02-PLAN.md"
  - ".planning/phases/20-tui-remove-action/20-02-SUMMARY.md"
  - ".planning/phases/20-tui-remove-action/20-03-PLAN.md"
  - ".planning/phases/20-tui-remove-action/20-03-SUMMARY.md"
  - ".planning/phases/20-tui-remove-action/20-04-PLAN.md"
  - ".planning/phases/20-tui-remove-action/20-04-SUMMARY.md"
  - "baude-core/src/removal.rs"
  - "baude-core/src/testing.rs"
  - "baude/src/app.rs"
  - "baude/src/ui.rs"
covered_digest: "v4:sha256:3c425e4be8e884bfb10781e3313fa8ee2db460eb2aa82bcbf0139e53b5c6b37d"
behavior_unverified: 0
overrides_applied: 0
decision_coverage:
  honored: 0
  total: 0
  not_honored: []
human_verification:
  - test: "Run baude in a terminal narrower than 80 columns (for example 59x20) and wider than 120, select a repository with several worktrees, press D, scroll with j/k, press Esc; then look at the sidebar footer on each row kind"
    expected: "The red-bordered `remove repository` modal fits inside 80% of the screen height, the `+N more` entry reads cleanly at the narrow width, the footer shows `D rm` (narrow) or `D remove*` (wide) beside x, w and X without truncation that hides the D hint"
    why_human: "Render tests at 100x30 and 59x20 pin the text and row count (ui.rs confirm_remove_modal_renders_report_lines_within_80_percent_height, hierarchy_copy_contract_matches_ui_spec_for_empty_pending_success_and_hints) but cannot judge legibility or layout feel; 20-VALIDATION.md lists this as the phase's manual-only check"
---

# Phase 20: TUI Remove Action Verification Report

**Phase Goal:** The user can remove any row kind from the sidebar through a confirmation that names exactly what will be killed, dropped and deleted, and a failure leaves a visible, retryable state.
**Verified:** 2026-11-05T09:03:05Z
**Status:** human_needed
**Re-verification:** No — initial verification

Branch `gsd/phase-18-shared-kill-then-remove-lifecycle`, HEAD f1f6d64. All 13 phase commits resolve (`verify.commits` exit 0). The phase diff touches four source files only: `baude/src/app.rs` (+1555), `baude/src/ui.rs` (+650), `baude-core/src/testing.rs` (+135), `baude-core/src/removal.rs` (64 lines, moving the parked-reader double into `testing.rs`). No `TBD`/`FIXME`/`XXX`/`TODO`/`todo!`/`unimplemented!`/`#[ignore]` was added in the phase diff, and no test reads source files.

## Goal Achievement

### Observable Truths

Roadmap success criteria (contract) first, then plan truths that add detail.

| #   | Truth   | Status     | Evidence       |
| --- | ------- | ---------- | -------------- |
| 1 | SC1: Remove action reachable on repository, main, managed, unmanaged, standalone rows; footer or help lists it beside `x`, `w`, `X` | ✓ VERIFIED | `sidebar_action` app.rs:337-350 maps `D` on Repository/Main/Managed/External/Standalone/Unavailable to `SidebarAction::RemoveProject`; dispatched at app.rs:5622. Footer literals ui.rs:1472-1518 carry `D rm`/`D remove*` on every local arm; help overlay ui.rs:2384 lists `D` directly under `x` and `X`. Tests `sidebar_action_d_yields_remove_project_on_every_local_kind`, `hierarchy_copy_contract_matches_ui_spec_for_empty_pending_success_and_hints` (pins footer strings incl. remote lacking D at ui.rs:3182), `hierarchy_viewport_matrix_renders_without_panic_and_preserves_semantics` (asserts rendered help contains `D removes`) all pass |
| 2 | SC2: Confirmation names row kind, rows dropped, whether a live runtime is killed, paths deleted vs kept; repository parent lists every child | ✓ VERIFIED | `remove_project_confirmation_lines` ui.rs:2480-2598 emits `Remove this {kind}?`, `rows dropped (n)`, `runtimes killed (n)` with pids, `paths deleted (n)`, `paths kept (n)`, `already removed`, `cleared:` — every section iterates a `RemovalReport` field, no TUI recomputation. Modal built from `preview_removal` at app.rs:3917-3930. App tests `d_opens_confirm_remove_modal_with_preview_report_on_each_row_kind` (asserts modal report `==` `preview_removal` output on 5 kinds plus a relaunched Unavailable row), `d_on_row_with_live_runtime_reports_runtime_killed` (asserts the named `agent_pid` is the fixture's live pid and is still live after preview) pass |
| 3 | SC3: On confirm the rows disappear from sidebar and state file in one step | ✓ VERIFIED | `confirm_remove_project` app.rs:3999-4073 calls `removal::remove` with one atomic `save` closure then drops runtime mappings. Tests `confirm_remove_managed_worktree_drops_row_in_one_save` and `confirm_remove_repository_parent_drops_every_child_in_one_save` assert hierarchy rows gone, `saved_state(&fx)` lacks every child, `save_attempts_for_test == before + 1`, selection moved, footer counts. Both pass |
| 4 | SC4: Failure at any stage leaves rows visible with an error notice and the user can retry | ✓ VERIFIED | Err arm app.rs:4048-4053 calls `restore_row_runtimes` then returns; key handler app.rs:5810-5818 reinstalls `Modal::ConfirmRemove` with `error`. Legend switches to `enter retry · esc cancel` (ui.rs:2590-2593). Tests `remove_project_failure_kill_keeps_rows_and_handles_then_retry_succeeds` (real armed process group; asserts surviving pid in error, row still in hierarchy and state file, state-file bytes unchanged, zero saves, handle identity restored, then retry removes) and `remove_project_failure_save_keeps_rows_then_retry_succeeds` (`AtomicFailure::Rename`; asserts `could not persist`/`rename` in error, row intact, retry removes) pass |
| 5 | 20-01: Modal lines come only from report fields (D-05) | ✓ VERIFIED | ui.rs:2480-2598 as above; `remove_project_lines_sections_come_from_report_fields` passes |
| 6 | 20-01: Repository parent lists every child; j/k scroll; hidden remainder renders `+N more` (D-07, TUI-03) | ✓ VERIFIED | ui.rs:2501-2512 heading count is the full list, window clamps, `+{hidden} more`; `remove_project_lines_repository_parent_lists_every_child_and_scrolls` asserts `+3 more`/`+1 more` windows; passes |
| 7 | 20-01: A path never appears under deleted unless `report.paths_deleted` lists it | ✓ VERIFIED | ui.rs:2526-2532 iterates only `report.paths_deleted`; test 5 above |
| 8 | 20-01: Legend `enter retry · esc cancel` with error, `y/enter remove · n/esc cancel` without | ✓ VERIFIED | ui.rs:2590-2594; `remove_project_lines_error_switches_legend_to_retry` passes |
| 9 | 20-02: `D` on every local kind yields `RemoveProject`; remote yields footer refusal naming Phase 21 (D-02, TUI-01) | ✓ VERIFIED | app.rs:344 `Refuse(SidebarRefusal::RemoteRemove)`; refusal text app.rs:1323 ("remote rows are removed from the daemon and PWA in a later release. Nothing was changed."). `sidebar_action_d_refuses_remote_rows`, `d_on_remote_row_sets_footer_notice` pass |
| 10 | 20-02: `D` opens `Modal::ConfirmRemove` with `preview_removal`'s report, including a row with a live runtime (D-03, D-05) | ✓ VERIFIED | app.rs:3903-3937; truths 2 tests |
| 11 | 20-02: Enter/y runs `removal::remove` with the atomic save; rows leave hierarchy and state file in one save; footer reports counts (D-08, D-11) | ✓ VERIFIED | app.rs:4024-4040 save closure, `removal_notice` app.rs:4078-4110; truth 3 tests assert `message.contains("1 row") && contains("1 runtime")` |
| 12 | 20-02: On Err every `RowRuntime` handle returns to the registry, rows stay visible, modal reinstalled with error (D-09) | ✓ VERIFIED | app.rs:3976-3990 `restore_row_runtimes`; kill test asserts `app.sessions.len() == sessions_before` and `process_identity()` equality |
| 13 | 20-02: Esc/n closes without mutation; j/k adjust scroll (D-06, D-07) | ✓ VERIFIED | app.rs:5823-5836; `esc_on_confirm_remove_is_non_mutating` passes; scroll asserted by `confirm_remove_modal_renders_report_lines_within_80_percent_height` (first visible row changes, `+11 more` → `+6 more`) |
| 14 | 20-03: ConfirmRemove modal draws at most 80% of screen height with current scroll (D-07) | ✓ VERIFIED | ui.rs:1787-1789 `area.height * 80 / 100`, height `min(max_height)` ui.rs:1813-1815; render test asserts `occupied.len() <= 24` of 30 and scroll changes the first row; passes |
| 15 | 20-03: Every local footer line and `?` help list D beside x, w, X; remote footers do not (D-04) | ✓ VERIFIED | truth 1 evidence; ui.rs:3182-3183 asserts remote hint lacks `D remove` |
| 16 | 20-04: Forced kill failure returns Err before any row drops; surviving pids shown; every handle back; retry succeeds after group dies | ✓ VERIFIED | `spawn_unkillable_runtime`/`arm_unkillable_runtime` testing.rs:532-546 (real out-of-group holder); kill test above passes (74.8s targeted run) |
| 17 | 20-04: Forced save failure leaves rows visible with save error and retry legend; clearing injection and Enter removes | ✓ VERIFIED | app.rs:4041-4046 clears `atomic_failure_for_test` after one attempt; save-failure test passes |
| 18 | 20-04: Clearing failures after the committed save do not fail removal; footer notice (D-10) | ✓ VERIFIED | app.rs:4090-4108 appends `could not clear N: ...`; `remove_project_clearing_failure_is_a_footer_notice_not_an_error` asserts `Modal::None`, path gone, message names the clearing failure; passes |
| 19 | 20-04: `scripts/gate.sh` and `scripts/gate.sh --linux` are green | ✓ VERIFIED | Verifier ran `scripts/gate.sh` on HEAD f1f6d64: exit 0, `gate: all steps passed`, 0 FAILED (log in verifier scratchpad `gate.log`). `--linux`: not re-run by the verifier; artifact `gate-linux.log` (mtime 2026-11-05 00:41 PST, after 41f9b07 at 00:38 PST; f1f6d64 is docs-only) ends `gate: all steps passed`, 974 tests 0 failed |

**Score:** 19/19 truths verified (0 present, behavior-unverified)

### Locked-decision checks

| Decision | Status | Evidence |
| -------- | ------ | -------- |
| D is a sibling of X; X untouched | ✓ | `D` branch precedes the `X` branch in `sidebar_action` (app.rs:337-362) and returns before it; the phase diff's only `remove_worktree` touch in app.rs is a doc comment (`reserve_removal_target`). `ConfirmRemoveWorktree` arm ui.rs:1746 unchanged |
| Remote rows refuse | ✓ | app.rs:344, 1323; tests pass |
| Modal content comes from RemovalReport fields | ✓ | ui.rs:2480-2598; key link verified by reading: no path or row is computed TUI-side |
| Failures keep rows visible with retryable modal | ✓ | truths 4, 16, 17 |
| `persistence_dirty` refusal not copied | ✓ | Guard at app.rs:5601-5610 enumerates `Branch | Close | RetryReopen | Remove` only; `RemoveProject` is dispatched past it |
| D-08 recheck | ✓ | app.rs:4011-4016 compares `rows_removed` (discretion allowed); `remove_project_recheck_refreshes_modal_when_row_set_changes` asserts no save ran and footer says `rows changed since the preview`; passes |

### Required Artifacts

`verify.artifacts` returned 0 artifacts for every plan (exit 1) because the plans declare artifacts as prose strings, not `{path, provides}` objects; verified manually.

| Artifact | Expected    | Status | Details |
| -------- | ----------- | ------ | ------- |
| `baude/src/ui.rs: remove_project_confirmation_lines` | pure report-to-lines renderer | ✓ VERIFIED | ui.rs:2480-2598, 118 lines, called from `draw_modal` ui.rs:1804 |
| `baude/src/app.rs: SidebarAction::RemoveProject, SidebarRefusal::RemoteRemove, Modal::ConfirmRemove, prepare (open_remove_project_modal), confirm_remove_project, collect_row_runtimes, restore_row_runtimes` | dispatch, modal, custody | ✓ VERIFIED | app.rs:300, 332, 501, 3905, 3943, 3976, 3999; all reached from `handle_sidebar_key`/`handle_modal_key` |
| `baude/src/ui.rs: draw_modal ConfirmRemove arm; footer `D remove*`; help D lines` | render + hints | ✓ VERIFIED | ui.rs:1776-1826, 1472-1518, 2384-2385 |
| `baude-core/src/testing.rs: spawn_unkillable_runtime` (test-support gated) | failure double | ✓ VERIFIED | testing.rs:532, 546 under module gate `cfg(any(test, feature = "test-support"))` (testing.rs:15); `baude/Cargo.toml:33` enables `test-support` |
| `baude/src/app.rs: failure and retry tests` | App-level proofs | ✓ VERIFIED | app.rs:12967, 13094, 13200, 13270; cloned-repo fixture via `relaunch_fixture` → `admission_repo_cloned` |

### Key Link Verification

`verify.key-links` returned 0 links per plan (prose strings); verified by reading.

| From | To  | Via | Status | Details |
| ---- | --- | --- | ------ | ------- |
| `remove_project_confirmation_lines` | `baude_core::removal::RemovalReport` | field iteration only | ✓ WIRED | ui.rs:2500-2580 |
| `confirm_remove_project` | `removal::remove` + `persist` atomic save consuming `atomic_failure_for_test` | closure app.rs:4024-4040 | ✓ WIRED | `persist::save_current_at_test(..., save_failure, ...)` in test cfg, `save_for_workspace_status` otherwise |
| Err arm | `restore_row_runtimes` before modal reinstall | app.rs:4048-4053 → 5810-5818 | ✓ WIRED | kill test asserts handle count and identity |
| `draw_modal` ConfirmRemove arm | `remove_project_confirmation_lines(report, kind_label, row_name, error, scroll, max_rows, width)` | ui.rs:1804-1812 | ✓ WIRED | signature matches 7 args |
| App failure tests | `baude_core::testing` double via `test-support` | app.rs:12970 `use baude_core::testing::{arm_unkillable_runtime, ...}` | ✓ WIRED | compiled and ran |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
| -------- | ------------- | ------ | ------------------ | ------ |
| ConfirmRemove modal | `report` | `preview_removal(&self.repository_state, target)` app.rs:3917 | yes, tests compare to a fresh preview | ✓ FLOWING |
| footer notice | `removal_notice(&report)` | `removal::remove` return value | yes | ✓ FLOWING |
| modal error line | `error.to_string()` | `RemovalError`/reservation error from core | yes, tests assert pid and `rename` text | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| -------- | ------- | ------ | ------ |
| All D/remove_project App and ui tests | `cargo test -p baude -- --test-threads=1 remove_project d_opens_confirm_remove d_on_ confirm_remove esc_on_confirm_remove sidebar_action_d action_status_hint help_overlay footer` | 23 passed, 0 failed, exit 0, 74.80s | ✓ PASS |
| Footer and help pins | `cargo test -p baude -- --exact ui::tests::hierarchy_copy_contract_matches_ui_spec_for_empty_pending_success_and_hints ui::tests::hierarchy_viewport_matrix_renders_without_panic_and_preserves_semantics` | 2 passed, exit 0 | ✓ PASS |
| Full gate (single run) | `scripts/gate.sh` | exit 0, `gate: all steps passed` | ✓ PASS |

### Probe Execution

Not applicable: no `scripts/*/tests/probe-*.sh` declared or present for this phase.

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| ----------- | ---------- | ----------- | ------ | -------- |
| TUI-01 | 20-02, 20-03 | Remove on all local row kinds; footer/help lists it | ✓ SATISFIED | truths 1, 9, 15 |
| TUI-02 | 20-01, 20-02, 20-03 | Confirmation names kind, rows, runtime kill, paths deleted vs kept | ✓ SATISFIED | truths 2, 5, 7, 10 |
| TUI-03 | 20-01, 20-02, 20-04 | Repository parent lists every child; removes all on confirm | ✓ SATISFIED | truths 3, 6, 11 |
| TUI-04 | 20-02, 20-04 | Failure leaves rows visible with error; retry | ✓ SATISFIED | truths 4, 12, 16, 17 |

No orphaned requirements: REQUIREMENTS.md maps exactly TUI-01..04 to Phase 20.

### Decision Coverage

No trackable decisions in CONTEXT.md (`check.decision-coverage-verify` skipped, non-blocking). The D-01..D-11 decisions were checked by hand in "Locked-decision checks" above.

### Test Quality Audit

- No disabled tests in the phase diff (`#[ignore]` count 0 in added lines).
- No circular fixtures: expected modal content comes from `preview_removal` (the same core the real run uses, by design D-05), while state-file, hierarchy, pid liveness, save-count and directory assertions are independent observations.
- Fixtures: cloned repos (`admission_repo_cloned`), real spawned agents, a real out-of-group process holder for the kill failure, `AtomicFailure::Rename` injected at the persist boundary. Assertions are on App state, rendered buffers and the state file, never on source text.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| none | - | - | - | Diff-scoped scan of added lines found no debt markers, placeholders, or empty implementations |

### Human Verification Required

#### 1. Narrow and wide terminal visual check

**Test:** Run baude at 59x20 and at 120+ columns, select a repository with several worktrees, press `D`, scroll with `j`/`k`, press `Esc`; then view the footer on each row kind.
**Expected:** Modal within 80% height, `+N more` legible, footer shows `D rm` (narrow) or `D remove*` (wide) beside `x`, `w`, `X`.
**Why human:** Render tests pin text and row counts at both sizes but cannot judge legibility or layout; 20-VALIDATION.md names this as the phase's manual-only verification.

### Gaps Summary

No gaps. Every roadmap success criterion and plan truth is implemented, wired, and proven by a fixture-backed test that the verifier ran. The single human item is the visual check the phase itself scheduled as manual-only.

---

_Verified: 2026-11-05T09:03:05Z_
_Verifier: Claude (gsd-verifier)_
