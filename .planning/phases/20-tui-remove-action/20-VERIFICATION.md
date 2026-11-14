---
phase: 20-tui-remove-action
verified: 2026-11-13T21:56:02-08:00
head: f934e6474e54f7e195919e7fe4cf9d5779d5d66b
status: human_needed
score: 18/19 must-haves verified
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
  - "baude/src/remote.rs"
  - "baude/src/ui.rs"
covered_digest: "v4:sha256:3995b467c2f965f2939186429d687bd3970664885052c0f3cc9090582591c673"
behavior_unverified: 0
overrides_applied: 2
overrides:
  - must_have: "D on a remote row yields a footer refusal naming Phase 21"
    reason: "Superseded by Phase 21 D-12: D on a remote row previews and removes through the owning daemon"
    accepted_by: "Joe Seymour"
    accepted_at: "2026-11-13"
  - must_have: "remote-row footers do not list D remove*"
    reason: "Superseded by Phase 21 D-12: the remote footer lists D remove* because the action now exists there"
    accepted_by: "Joe Seymour"
    accepted_at: "2026-11-13"
re_verification:
  previous_status: passed
  previous_score: 19/19
  previous_head: f1f6d64
  gaps_closed: []
  gaps_remaining: []
  regressions: []
  superseded:
    - "20-02 truth 1, remote clause: D on a remote row yields a footer refusal naming Phase 21"
    - "20-03 truth 2, remote clause: remote-row footers do not list D"
  not_reverified:
    - "20-04 truth 4: scripts/gate.sh and scripts/gate.sh --linux are green"
superseded:
  - truth: "D on a remote row yields a footer refusal naming Phase 21 (20-02 truth 1, second clause)"
    superseded_by: "Phase 21 decision D-12"
    evidence: ".planning/phases/21-daemon-and-pwa-parity/21-CONTEXT.md:33 'The TUI's remote-row refusal becomes a real action'; baude/src/app.rs:344-353; SidebarRefusal::RemoteRemove and its refusal text no longer exist"
  - truth: "Remote-row footers do not list D (20-03 truth 2, second clause)"
    superseded_by: "Phase 21 decision D-12"
    evidence: "baude/src/ui.rs:1470-1472 remote footer is 'enter attach · r restart · x close · D remove* · ? help'; pinned by ui::tests::hierarchy_copy_contract_matches_ui_spec_for_empty_pending_success_and_hints"
human_verification:
  - test: "Run scripts/gate.sh and scripts/gate.sh --linux at HEAD f934e64 and read each exit code directly"
    expected: "Both exit 0 with 'gate: all steps passed'"
    why_human: "The verifier was instructed not to run the full gate. The prior evidence is for f1f6d64, and app.rs, ui.rs and remote.rs have changed by about 6,100 lines since"
human_verification_waived:
  - test: "Run baude at 59x20 and at 120+ columns, select a repository with several worktrees, press D, scroll with j/k, press Esc; then view the footer on each row kind"
    expected: "The red-bordered 'remove repository' modal stays within 80% of the screen height, '+N more' reads cleanly at the narrow width, the footer shows 'D rm' (narrow) or 'D remove*' (wide) beside x, w and X without truncation hiding the D hint"
    disposition: "waived by the owner and NOT performed"
    waived_by: "Joe Seymour"
    waived_at: "2026-11-13"
    note: "Nobody ran this check. Legibility at either size is unconfirmed by a person; the render tests that pin text and row counts at both sizes are the only evidence"
    source: ".planning/phases/21-daemon-and-pwa-parity/.replan-cycle-3/ORCH-STATE.md at commit 2460601"
---

# Phase 20: TUI Remove Action Verification Report

**Phase Goal:** The user can remove any row kind from the sidebar through a confirmation that names exactly what will be killed, dropped and deleted, and a failure leaves a visible, retryable state.
**Verified:** 2026-11-13 (HEAD f934e6474e54f7e195919e7fe4cf9d5779d5d66b, branch `gsd/phase-18-shared-kill-then-remove-lifecycle`)
**Status:** human_needed
**Re-verification:** Yes. The prior report (f1f6d64, 2026-11-05) predates the Phase 20 review fixes and all of Phase 21.

## What changed since the prior report

18 commits touched the covered files between f1f6d64 and HEAD: four Phase 20 review fixes (ca979fb WR-01, 824a4b2 WR-03, f99e095 WR-02, 09fcd33 WR-04) and fourteen Phase 21 commits (21-06, 21-08, WR-04, WR-05). Diff: `baude/src/app.rs` +2128, `baude/src/ui.rs` +739, `baude/src/remote.rs` +3505. `baude-core/` is byte-identical to f1f6d64 (`git diff --quiet f1f6d64..HEAD -- baude-core/` exit 0), so what a local removal kills, drops, deletes and keeps is decided by unchanged core code.

The prior report's frontmatter said `status: passed` while its body said `human_needed` and listed one human item. This report's frontmatter and body agree.

## Goal Achievement

Legend for the Method column: **T** = verified by a test this verifier ran at HEAD; **R** = verified by reading code at HEAD; **N** = not verified.

All tests below ran in one invocation, exit code read directly from the unpiped command: `cargo test -p baude -- --test-threads=1 sidebar_action_d d_opens_confirm_remove d_on_ confirm_remove esc_on_confirm_remove remove_project hierarchy_copy_contract hierarchy_viewport_matrix help_overlay removal_text` → exit 0, `32 passed; 0 failed; 0 ignored`, 99.34s.

### Observable Truths

| #   | Truth | Status | Method | Evidence at HEAD |
| --- | ----- | ------ | ------ | ---------------- |
| 1 | SC1: Remove reachable on repository, main, managed, unmanaged, standalone rows; footer or help lists it beside `x`, `w`, `X` | ✓ VERIFIED | T + R | `sidebar_action` app.rs:337-355 maps `D` on Repository/Main/Managed/External/Standalone/Unavailable to `SidebarAction::RemoveProject` (app.rs:333); dispatched at app.rs:6088. Footer literals ui.rs:1462-1525 carry `D rm` / `D remove*` on every local arm, byte-identical to f1f6d64 for every local arm (function diffed; the only changed line is the remote arm). Help overlay ui.rs:2422-2426 lists `D` directly under `x` and `X`. Tests: `sidebar_action_d_yields_remove_project_on_every_local_kind` (app.rs:12827, body unchanged), `hierarchy_copy_contract_matches_ui_spec_for_empty_pending_success_and_hints` (ui.rs:3333), `hierarchy_viewport_matrix_renders_without_panic_and_preserves_semantics` (ui.rs:3085, asserts `D removes` at ui.rs:3146) |
| 2 | SC2: Confirmation names row kind, rows dropped, whether a live runtime is killed, paths deleted vs kept; repository parent lists every child | ✓ VERIFIED | T + R | `remove_project_confirmation_lines` ui.rs:2736-2853 is byte-identical to f1f6d64 (function diffed, no change). Modal built from `preview_removal` at app.rs:4064-4074. Tests `d_opens_confirm_remove_modal_with_preview_report_on_each_row_kind` (app.rs:14363) and `d_on_row_with_live_runtime_reports_runtime_killed` (app.rs:14456), both bodies unchanged |
| 3 | SC3: On confirm the rows disappear from sidebar and state file in one step | ✓ VERIFIED | T + R | `confirm_remove_project` app.rs:4462-4544 runs `removal::remove` with one atomic `save` closure, then drops runtime mappings. Tests `confirm_remove_managed_worktree_drops_row_in_one_save` (app.rs:14492) and `confirm_remove_repository_parent_drops_every_child_in_one_save` (app.rs:14598), both bodies unchanged |
| 4 | SC4: Failure at any stage leaves rows visible with an error notice and the user can retry | ✓ VERIFIED | T + R | Err arm app.rs:4519-4524 calls `restore_row_runtimes` then returns; key handler app.rs:6285-6296 reinstalls `Modal::ConfirmRemove` with `error` and `origin: Local`. Tests `remove_project_failure_kill_keeps_rows_and_handles_then_retry_succeeds` (app.rs:14789) and `remove_project_failure_save_keeps_rows_then_retry_succeeds` (app.rs:14921). Both gained `app.tick()` calls between failure and retry (WR-04); no assertion was removed |
| 5 | 20-01: Modal lines come only from report fields (D-05) | ✓ VERIFIED | T + R | ui.rs:2757-2840 iterates `report.rows_removed`, `runtimes_killed`, `paths_deleted`, `paths_kept`, `already_removed`, `cleared`; `remove_project_lines_sections_come_from_report_fields` (ui.rs:3846, unchanged) |
| 6 | 20-01: Repository parent lists every child; j/k scroll; hidden remainder renders `+N more` (D-07, TUI-03) | ✓ VERIFIED | T + R | ui.rs:2757-2767; `remove_project_lines_repository_parent_lists_every_child_and_scrolls` (ui.rs:3893, unchanged) |
| 7 | 20-01: A path never appears under deleted unless `report.paths_deleted` lists it | ✓ VERIFIED | T + R | ui.rs:2781-2787 iterates only `report.paths_deleted`; test of truth 5 |
| 8 | 20-01: Legend `enter retry · esc cancel` with error, `y/enter remove · n/esc cancel` without | ✓ VERIFIED | T + R | ui.rs:2846-2851; `remove_project_lines_error_switches_legend_to_retry` (ui.rs:3929, unchanged) |
| 9 | 20-02: `D` on every local kind yields `RemoveProject`; `D` on a remote row yields a footer refusal naming Phase 21 (D-02, TUI-01) | PASSED (override): local clause verified, remote clause superseded by Phase 21 D-12, accepted by Joe Seymour on 2026-11-13 | T + R | Local clause: truth 1 evidence. Remote clause is false at HEAD: app.rs:344-353 returns `RemoveProject` for `ActionKind::Remote`, `SidebarRefusal::RemoteRemove` and its text are gone, tests `sidebar_action_d_refuses_remote_rows` and `d_on_remote_row_sets_footer_notice` were deleted and replaced by `sidebar_action_d_yields_remove_project_on_remote_rows` (app.rs:12882) and `d_on_remote_row_previews_then_removes_through_the_daemon` (app.rs:13087), both passing. Deliberate: Phase 21 D-12. See "Superseded clauses" |
| 10 | 20-02: `D` opens `Modal::ConfirmRemove` with `preview_removal`'s report, including a row with a live runtime (D-03, D-05) | ✓ VERIFIED | T + R | app.rs:4044-4081; the remote branch at app.rs:4046-4049 returns before the local path and is taken only for `SelId::Remote`. Tests of truth 2 |
| 11 | 20-02: Enter/y runs `removal::remove` with the atomic save; rows leave hierarchy and state file in one save; footer reports counts (D-08, D-11) | ✓ VERIFIED | T + R | app.rs:4494-4510 save closure, `removal_notice` app.rs:4549-4580, handler app.rs:6267-6271; tests of truth 3 |
| 12 | 20-02: On Err every `RowRuntime` handle returns to the registry, rows stay visible, modal reinstalled with error (D-09) | ✓ VERIFIED | T + R | `restore_row_runtimes` app.rs:4439-4453; kill-failure test asserts handle count and process identity |
| 13 | 20-02: Esc/n closes without mutation; j/k adjust scroll (D-06, D-07) | ✓ VERIFIED | T + R | app.rs:6301-6330; `esc_on_confirm_remove_is_non_mutating` (app.rs:14562, unchanged); scroll driven through `handle_modal_key` by `confirm_remove_modal_scroll_past_end_then_up_moves_one_row` (ui.rs:4143, added by WR-02) |
| 14 | 20-03: ConfirmRemove modal draws at most 80% of screen height with current scroll (D-07) | ✓ VERIFIED | T + R | `remove_project_modal_max_height` ui.rs:2516-2520, used at ui.rs:1806 and 1837-1839. `confirm_remove_modal_renders_report_lines_within_80_percent_height` (ui.rs:4101) was tightened by WR-03 from `<= 24` to `== 24` of 30 rows |
| 15 | 20-03: Every local footer line and `?` help list `D` beside x, w, X; remote-row footers do not (D-04) | PASSED (override): local clause verified, remote clause superseded by Phase 21 D-12, accepted by Joe Seymour on 2026-11-13 | T + R | Local clause: truth 1 evidence. Remote clause is false at HEAD: ui.rs:1470-1472 returns `enter attach · r restart · x close · D remove* · ? help`, and the test that asserted the absence now asserts that exact string at widths 100 and 59. Deliberate: Phase 21 D-12 |
| 16 | 20-04: Forced kill failure returns Err before any row drops; surviving pids shown; every handle back; retry succeeds after group dies | ✓ VERIFIED | T + R | `spawn_unkillable_runtime` / `arm_unkillable_runtime` baude-core/src/testing.rs:532, 546 (unchanged); kill test of truth 4 |
| 17 | 20-04: Forced save failure leaves rows visible with save error and retry legend; clearing injection and Enter removes | ✓ VERIFIED | T + R | app.rs:4511-4516 clears `atomic_failure_for_test` after one attempt; save test of truth 4 |
| 18 | 20-04: Clearing failures after the committed save do not fail removal; footer notice (D-10) | ✓ VERIFIED | T + R | app.rs:4560-4578; `remove_project_clearing_failure_is_a_footer_notice_not_an_error` (app.rs:15031, unchanged) |
| 19 | 20-04: `scripts/gate.sh` and `scripts/gate.sh --linux` are green | ? UNCERTAIN | N | Not run: the verification brief excluded the full gate. The prior pass was for f1f6d64 and does not carry to HEAD. No gate result for f934e64 was found in the phase 20 or 21 artifacts |

**Score:** 18/19 truths verified, 2 of them by override (0 present, behavior-unverified). Truths 9 and 15 each carry a superseded clause, accepted as an override by Joe Seymour on 2026-11-13; truth 19, the gate, was not re-run. The verifier's own count before the overrides was 16/19.

All four roadmap success criteria (truths 1 to 4) hold. Nothing in the roadmap contract or in TUI-01 to TUI-04 mentions remote rows, so both superseded clauses are plan-level detail.

### Superseded clauses

Truths 9 and 15 each carry a second clause that Phase 20 wrote as a holding position: remote rows refuse `D` "naming Phase 21". Phase 21 then made that refusal a real action by locked decision D-12 (`.planning/phases/21-daemon-and-pwa-parity/21-CONTEXT.md:33`). The clauses are therefore false at HEAD on purpose, and restoring them would revert Phase 21. They are reported as superseded, not as gaps. The verifier left them out of its 16/19 count; with the overrides below accepted, each counts as PASSED (override).

The plan artifact `SidebarRefusal::RemoteRemove` (20-02) is gone for the same reason.

Accepted by Joe Seymour on 2026-11-13. This block is now in this file's frontmatter, with `overrides_applied: 2`:

```yaml
overrides:
  - must_have: "D on a remote row yields a footer refusal naming Phase 21"
    reason: "Superseded by Phase 21 D-12: D on a remote row previews and removes through the owning daemon"
    accepted_by: "Joe Seymour"
    accepted_at: "2026-11-13"
  - must_have: "remote-row footers do not list D remove*"
    reason: "Superseded by Phase 21 D-12: the remote footer lists D remove* because the action now exists there"
    accepted_by: "Joe Seymour"
    accepted_at: "2026-11-13"
```

### Moved and renamed tests (specific risk checked)

Each Phase 20 test body was extracted from f1f6d64 and from HEAD and diffed.

| Test or helper | HEAD location | Change since f1f6d64 |
| -------------- | ------------- | -------------------- |
| `remove_project_app` (holds the `"cloned shape primary"` assert, app.rs:14263-14266) | app.rs:14258 | moved, body identical |
| `confirm_remove_error` | app.rs:14763 | identical |
| `confirm_remove_modal` | app.rs:14336 | one `..` added to a pattern for the new `origin` field |
| `sidebar_action_d_yields_remove_project_on_every_local_kind` | app.rs:12827 | identical |
| `d_opens_confirm_remove_modal_with_preview_report_on_each_row_kind` | app.rs:14363 | identical |
| `d_on_row_with_live_runtime_reports_runtime_killed` | app.rs:14456 | identical |
| `confirm_remove_managed_worktree_drops_row_in_one_save` | app.rs:14492 | identical |
| `esc_on_confirm_remove_is_non_mutating` | app.rs:14562 | identical |
| `confirm_remove_repository_parent_drops_every_child_in_one_save` | app.rs:14598 | identical |
| `remove_project_failure_kill_...` / `remove_project_failure_save_...` | app.rs:14789, 14921 | `app.tick()` added between failure and retry; nothing removed |
| `remove_project_clearing_failure_is_a_footer_notice_not_an_error` | app.rs:15031 | identical |
| `remove_project_recheck_refreshes_modal_when_row_set_changes` | app.rs:15101 | identical |
| `remove_project_lines_*` (four tests) | ui.rs:3846-3969 | identical |
| `confirm_remove_modal_renders_report_lines_within_80_percent_height` | ui.rs:4101 | tightened to `== 24`; `+N more` counts updated |
| `confirm_remove_modal_renders_error_and_retry_legend` | ui.rs:4179 | narrow case now also asserts the error line |
| `sidebar_action_d_refuses_remote_rows` | deleted | replaced by `sidebar_action_d_yields_remove_project_on_remote_rows` (app.rs:12882) |
| `d_on_remote_row_sets_footer_notice` | deleted | replaced by the `d_on_remote_row_*` and `remote_remove_*` Phase 21 tests |

No must-have that still applies lost its pinning test. The only two deleted tests pinned the superseded remote refusal. Tests added since f1f6d64 that strengthen Phase 20: `remove_project_recheck_refreshes_modal_when_runtime_set_changes` (app.rs:15157, WR-01), `confirm_remove_modal_scroll_past_end_then_up_moves_one_row` (ui.rs:4143, WR-02), `removal_text_local_modal_error_line_shows_row_label`.

### Locked-decision checks

| Decision | Status | Method | Evidence |
| -------- | ------ | ------ | -------- |
| D is a sibling of X; X untouched | ✓ | R | `D` branch app.rs:338-355 returns before the `X` branch app.rs:356-374; `X` on Remote still yields `SidebarAction::None`; `ConfirmRemoveWorktree` arm ui.rs:1749 |
| Modal content comes from RemovalReport fields | ✓ | T + R | truth 5 |
| Failures keep rows visible with retryable modal | ✓ | T | truths 4, 16, 17 |
| `persistence_dirty` refusal not copied | ✓ | R | guard app.rs:6067-6078 lists `Branch`, `Close`, `RetryReopen`, `Remove` only |
| D-08 recheck | ✓ | T + R | app.rs:4480-4486 now compares rows, runtimes killed and paths deleted (WR-01, stricter than before); both `remove_project_recheck_*` tests pass |
| Remote rows refuse | superseded | T + R | see "Superseded clauses" |

### Local removal behaviour versus the remote-removal work

| Aspect | Unchanged? | Evidence |
| ------ | ---------- | -------- |
| Key binding | yes | `D` and shift-`d` at app.rs:338-339; local-kind test body identical |
| Confirmation modal content | yes | `remove_project_confirmation_lines` byte-identical to f1f6d64. The draw arm (ui.rs:1779-1851) selects it for `RemovalOrigin::Local` and the title is still ` remove {kind} ` |
| What is and is not deleted | yes | `baude-core/` unchanged since f1f6d64; `confirm_remove_project` still passes the fresh preview's rows to `removal::remove` |
| Footer hints, local rows | yes | every local arm of `action_status_hint` identical; only the remote arm changed |
| Routing between local and remote | new, correct | `open_remove_project_modal` diverts only `SelId::Remote` (app.rs:4046-4049); Enter diverts only `RemovalOrigin::Remote` (app.rs:6261-6266). Local modals are always built with `origin: Local` (app.rs:4073, 6278, 6293) |
| Error line text | changed, additive | the drawn error has row keys replaced by row labels (ui.rs:1809-1812, commit 265572f); the stored error is untouched; pinned by `removal_text_local_modal_error_line_shows_row_label` |

### Required Artifacts

| Artifact | Status | Method | Details |
| -------- | ------ | ------ | ------- |
| `baude/src/ui.rs: remove_project_confirmation_lines` | ✓ VERIFIED | T + R | ui.rs:2736; called from `draw_modal` ui.rs:1817 and from `remove_project_modal_max_rows` ui.rs:2535 |
| `baude/src/app.rs: SidebarAction::RemoveProject, Modal::ConfirmRemove, open_remove_project_modal, confirm_remove_project, collect_row_runtimes, restore_row_runtimes` | ✓ VERIFIED | T + R | app.rs:333, 504, 4044, 4462, 4406, 4439. The plan's `prepare_remove_project` name was already `open_remove_project_modal` at the prior verification |
| `baude/src/app.rs: SidebarRefusal::RemoteRemove` | superseded | R | removed by Phase 21 D-12 |
| `baude/src/ui.rs: draw_modal ConfirmRemove arm; footer D literals; help D lines` | ✓ VERIFIED | T + R | ui.rs:1779-1851, 1462-1525, 2424-2425 |
| `baude-core/src/testing.rs: spawn_unkillable_runtime` (test-support gated) | ✓ VERIFIED | T + R | testing.rs:546, module gated at baude-core/src/lib.rs:25; `baude/Cargo.toml:33` enables `test-support` |
| `baude/src/app.rs: failure and retry tests` | ✓ VERIFIED | T | app.rs:14789, 14921, 15031, 15101, 15157 |

### Key Link Verification

| From | To | Status | Method | Details |
| ---- | -- | ------ | ------ | ------- |
| `remove_project_confirmation_lines` | `RemovalReport` fields only | ✓ WIRED | R | ui.rs:2757-2840; no path or row computed TUI-side |
| `confirm_remove_project` | `removal::remove` + atomic save consuming `atomic_failure_for_test` | ✓ WIRED | T + R | app.rs:4488-4516 |
| Err arm | `restore_row_runtimes` before modal reinstall | ✓ WIRED | T + R | app.rs:4522 then app.rs:6288-6294 |
| `draw_modal` ConfirmRemove arm | `remove_project_confirmation_lines(report, kind_label, row_name, error, scroll, max_rows, width)` | ✓ WIRED | T + R | ui.rs:1817-1825, seven arguments |
| App failure tests | `baude_core::testing` double via `test-support` | ✓ WIRED | T | compiled and ran |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Status |
| -------- | ------------- | ------ | ------ |
| ConfirmRemove modal (local) | `report` | `preview_removal(&self.repository_state, target)` app.rs:4064 | ✓ FLOWING |
| footer notice | `removal_notice(&report)` | `removal::remove` return value, app.rs:6270 | ✓ FLOWING |
| modal error line | `error.to_string()` | `RemoveProjectError` from core or reservation, app.rs:6292 | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| -------- | ------- | ------ | ------ |
| Every Phase 20 App and ui pinning test, plus the Phase 21 `d_on_remote_*` replacements | `cargo test -p baude -- --test-threads=1 sidebar_action_d d_opens_confirm_remove d_on_ confirm_remove esc_on_confirm_remove remove_project hierarchy_copy_contract hierarchy_viewport_matrix help_overlay removal_text` | exit 0; 32 passed, 0 failed, 0 ignored; 99.34s | ✓ PASS |
| Full gate | `scripts/gate.sh`, `scripts/gate.sh --linux` | not run (excluded by brief) | ? SKIP |

### Probe Execution

Not applicable: no `scripts/*/tests/probe-*.sh` is declared for this phase.

### Requirements Coverage

| Requirement | Source Plan | Status | Evidence |
| ----------- | ----------- | ------ | -------- |
| TUI-01 | 20-02, 20-03 | ✓ SATISFIED | truth 1; local clauses of 9 and 15 |
| TUI-02 | 20-01, 20-02, 20-03 | ✓ SATISFIED | truths 2, 5, 7, 10 |
| TUI-03 | 20-01, 20-02, 20-04 | ✓ SATISFIED | truths 3, 6, 11 |
| TUI-04 | 20-02, 20-04 | ✓ SATISFIED | truths 4, 12, 16, 17 |

No orphaned requirements. Note: `.planning/REQUIREMENTS.md:48-51` and `:105-108` still show TUI-01 to TUI-04 unchecked and "Pending", and `.planning/ROADMAP.md:157-160` still shows the four Phase 20 plans unchecked. That is bookkeeping, not a code gap.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| none | - | - | - | No `TBD`, `FIXME`, `XXX`, `todo!` or `unimplemented!` in `baude/src/app.rs`, `baude/src/ui.rs`, `baude-core/src/testing.rs` or `baude-core/src/removal.rs`. The single `#[ignore]` mention (ui.rs:4538) is a comment about plan 08-03 |

### Human Verification Required

One item remains: the gate (item 2). Items 1 and 3 were closed by owner decision after this report was written, and are kept here with their outcome.

#### 1. Accept or reject the Phase 21 supersession

**Outcome:** Accepted by Joe Seymour on 2026-11-13. Both clauses are recorded as PASSED (override) in the frontmatter `overrides` block.
**Test:** Decide whether the two remote-row clauses (truths 9 and 15) are accepted as superseded by Phase 21 D-12; if so, add the overrides block above to this file's frontmatter.
**Expected:** Both recorded as PASSED (override).
**Why human:** An override needs a named acceptor.

#### 2. Gate at HEAD

**Test:** Run `scripts/gate.sh` and `scripts/gate.sh --linux` at f934e64, reading each exit code directly.
**Expected:** Both exit 0 with `gate: all steps passed`.
**Why human:** Excluded from this verification; the last recorded pass is for f1f6d64.

#### 3. Narrow and wide terminal visual check

**Outcome:** Waived by the owner and NOT performed. Joe Seymour waived this check on 2026-11-13. Nobody ran baude at 59x20 or at 120+ columns for it, so legibility at either size is unconfirmed by a person. The render tests that pin text and row counts at both sizes are the only evidence.
**Test:** Run baude at 59x20 and at 120+ columns, select a repository with several worktrees, press `D`, scroll with `j`/`k`, press `Esc`; then view the footer on each row kind.
**Expected:** Modal within 80% height, `+N more` legible, footer shows `D rm` (narrow) or `D remove*` (wide) beside `x`, `w`, `X`.
**Why human:** Render tests pin text and row counts but cannot judge legibility; `20-VALIDATION.md:62` names this as manual-only.

### Gaps Summary

No code gap. Every roadmap success criterion and every plan truth that still applies is implemented, wired and pinned by a test that passed at HEAD, and local removal behaviour is unchanged by the remote-removal work. The verifier listed three items that stopped this short of `passed`: two plan clauses that Phase 21 replaced on purpose and that needed a recorded override, the gate that was not re-run at HEAD, and the manual visual check the phase scheduled and never recorded.

One remains: the gate. The two clauses are now recorded as overrides accepted by Joe Seymour on 2026-11-13. The manual visual check was waived by the owner and NOT performed.

---

_Verified: 2026-11-13 at HEAD f934e64_
_Verifier: Claude (gsd-verifier)_
_Owner decisions recorded 2026-11-14 by the Phase 22 planner: two overrides and one waiver, source `.planning/phases/21-daemon-and-pwa-parity/.replan-cycle-3/ORCH-STATE.md` at commit 2460601. Nothing was re-verified for this edit._
