---
phase: "07"
slug: "local-tui-dogfood-release"
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-08-31"
---

# Phase 07 - Validation Strategy

> Implementation evidence can mature overnight; certification and human dogfood remain pending until morning.

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness via Cargo 1.98.0; Ratatui 0.30 `TestBackend`/`Buffer` |
| **Config file** | Workspace manifests; inline unit and integration tests |
| **Quick run command** | Guarded exact test from the per-task map |
| **Full suite command** | `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` |
| **Estimated runtime** | ~180 seconds |

## Sampling Rate

- **After every task commit:** Run the guarded exact new test plus directly affected lifecycle regression tests.
- **After every plan wave:** Run the full suite command.
- **Before implementation summary:** Run final-gate sampling: full suite, package checks, and host artifact builds must pass. This deliberately exceeds the focused-feedback budget and is not a per-task quick loop.
- **Before phase certification:** Supported CI artifact matrix, manual dogfood, Linux/runtime checks, independent review, and phase verification must pass.
- **Max focused feedback latency:** 60 seconds for guarded exact tests; final-gate sampling is explicitly exempt and may exceed 30 seconds.

## Exact Test Contract

Every filtered test command must first prove the exact test exists:

```bash
name='hierarchy::tests::local_hierarchy_orders_parents_and_children_by_durable_identity'
cargo test -p baude -- --list | rg -Fx -- "$name: test"
cargo test -p baude "$name" -- --exact --nocapture
```

| Requirement | Exact test |
|-------------|------------|
| REPO-05, HIER-01, HIER-02, HIER-03 | `hierarchy::tests::local_hierarchy_orders_parents_and_children_by_durable_identity` |
| REPO-05, HIER-01, HIER-02 | `ui::tests::hierarchy_tracer_renders_real_app_parent_and_child` |
| HIER-04 | `hierarchy::tests::local_hierarchy_order_ignores_runtime_and_session_status` |
| HIER-01, HIER-02, HIER-03 | `hierarchy::tests::local_hierarchy_selection_survives_refresh_and_removal_falls_back_locally` — same durable key across in-process refresh/status transitions; after restart first local parent, else first flat remote row, else none; no persisted selection claim |
| WORK-01 through WORK-06, SURF-01, SURF-02 | `app::tests::hierarchy_action_matrix_dispatches_only_authorized_local_actions` |
| SURF-05 | `app::tests::hierarchy_flat_remote_compatibility_has_no_local_parent_or_remove_action` |
| HIER-01, SURF-01 | `app::tests::hierarchy_resize_never_sends_zero_dimensions_and_transfers_hidden_shell_focus` |
| REL-01 | `app::tests::local_tui_dogfood_real_git_flow_survives_restart_without_duplicates` |
| HIER-01 through HIER-04, SURF-01 | `ui::tests::hierarchy_viewport_matrix_renders_without_panic_and_preserves_semantics` |
| WORK-03, WORK-05, SURF-01 | `ui::tests::hierarchy_modals_name_exact_targets_and_distinguish_close_from_remove` |
| SURF-01, SURF-02 | `ui::tests::hierarchy_copy_contract_matches_ui_spec_for_empty_pending_success_and_hints` — literal empty heading/body, persistence error, reopening pending, close success, three branch-success variants, remove success, both repository no-runtime variants, all full-width hints, and both narrow retained-child hints |
| HIER-01, HIER-03, HIER-04 | `ui::tests::hierarchy_unicode_width_scroll_and_selection_band_are_cell_correct` |
| SURF-05 | `api::tests::flat_session_api_remains_a_non_hierarchical_compatibility_projection` |
| REL-02 | Exact package/release-please/CI assertions in tasks `07-05-01` through `07-05-03` |
| REL-03 | Exact docs assertions and final-gate sampling in task `07-06-01` and `07-06-02` |

## Exact Task and Wave Mapping

| Wave | Task | Exact contract/evidence |
|------|------|-------------------------|
| 1 | `07-01-01` | Guarded hierarchy projection plus `ui::tests::hierarchy_tracer_renders_real_app_parent_and_child`; tracer reaches real App/Ratatui before expansion. |
| 1 | `07-01-02` | Guarded volatile-order and in-process selection/restart-initialization contract. |
| 2 | `07-02-01` | Guarded lifecycle capability projection. |
| 2 | `07-02-02` | Guarded exhaustive action matrix plus Phase 6 close/reopen/remove regressions. |
| 3 | `07-03-01` | Guarded viewport, Unicode-cell, resize/focus contracts. |
| 3 | `07-03-02` | Guarded modal and full copy-contract tests plus action regression. |
| 4 | `07-04-01` | Guarded isolated real-Git restart/dedup flow with deterministic restart selection and explicit reselection. |
| 4 | `07-04-02` | Guarded flat App/daemon compatibility and atomic persistence regression. |
| 5 | `07-05-01` | Exact Cargo/package metadata assertions. |
| 5 | `07-05-02` | Exact release-please prerelease proposal assertions. |
| 5 | `07-05-03` | Exact four-target non-publishing CI contract assertions. |
| 6 | `07-06-01` | Exact readiness-only README/changelog/runbook assertions. |
| 6 | `07-06-02` | Final-gate sampling and evidence-only validation update. |

## Wave 0 Requirements

- [ ] `baude/src/hierarchy.rs` projection fixtures and exact hierarchy tests.
- [ ] `ui::tests::hierarchy_tracer_renders_real_app_parent_and_child` in `baude/src/ui.rs`, mapped to `07-01-01`/Wave 1 as the first production tracer.
- [ ] Core lifecycle capability test in `baude-core/src/lifecycle.rs`.
- [ ] App action, resize, compatibility, and real-Git dogfood tests.
- [ ] Ratatui viewport, modal, Unicode-width, selection, and `ui::tests::hierarchy_copy_contract_matches_ui_spec_for_empty_pending_success_and_hints` tests.
- [ ] Flat daemon compatibility test.
- [ ] CI/package/version readiness checks and isolated manual dogfood runbook.

## Deferred Certification

| Gate | Status | Morning action |
|------|--------|----------------|
| Manual wide/narrow local TUI dogfood | pending | Execute the isolated runbook and record evidence. |
| Linux/runtime certification | pending | Run supported CI/runtime matrix, including Phase 6 process registration checks. |
| Independent deep review | pending | Review Phase 6 and Phase 7 changed source. |
| Phase verification and Nyquist approval | pending | Run verification only after the above evidence is available. |
| Release publication decision | pending | Readiness only; do not publish or push a release without explicit authorization. |

Certification runs in the morning. Record evidence in `.planning/phases/07-local-tui-dogfood-release/07-UAT-EVIDENCE.md`, creating that file only when screenshots, commands, and observed outcomes actually exist.

## Validation Sign-Off

- [ ] Every task has guarded exact automated verification.
- [ ] Sampling continuity has no three-task gap.
- [ ] Wave 0 covers every missing exact test.
- [ ] Full local implementation gate passes.
- [ ] Deferred certification gates pass.
- [ ] `nyquist_compliant: true` is set only from observed evidence.

**Approval:** pending

## Observed Local Evidence

### Plan 07-01: Durable hierarchy projection and navigation

- `cargo fmt --all -- --check` — passed 2026-08-31.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed 2026-08-31.
- `cargo test --workspace` — passed 2026-08-31: 37 `baude`, 212 `baude-core`, and 78 `bauded` tests.
- Guarded exact hierarchy tests passed:
  - `hierarchy::tests::local_hierarchy_orders_parents_and_children_by_durable_identity`
  - `hierarchy::tests::local_hierarchy_order_ignores_runtime_and_session_status`
  - `hierarchy::tests::local_hierarchy_selection_survives_refresh_and_removal_falls_back_locally`
  - `ui::tests::hierarchy_tracer_renders_real_app_parent_and_child`
  - `app::tests::hierarchy_navigation_visits_parents_cycles_children_and_retains_selection_on_ctrl_q`

This is implementation evidence only. Human dogfood/UAT and release certification remain pending in Plans 07-04 and 07-05.
