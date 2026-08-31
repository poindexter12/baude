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
- **Before implementation summary:** Full suite, package checks, and host artifact builds must pass.
- **Before phase certification:** Supported CI artifact matrix, manual dogfood, Linux/runtime checks, independent review, and phase verification must pass.
- **Max focused feedback latency:** 60 seconds.

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
| HIER-04 | `hierarchy::tests::local_hierarchy_order_ignores_runtime_and_session_status` |
| HIER-01, HIER-02, HIER-03 | `hierarchy::tests::local_hierarchy_selection_survives_refresh_and_removal_falls_back_locally` |
| WORK-01 through WORK-06, SURF-01, SURF-02 | `app::tests::hierarchy_action_matrix_dispatches_only_authorized_local_actions` |
| SURF-05 | `app::tests::hierarchy_flat_remote_compatibility_has_no_local_parent_or_remove_action` |
| HIER-01, SURF-01 | `app::tests::hierarchy_resize_never_sends_zero_dimensions_and_transfers_hidden_shell_focus` |
| REL-01 | `app::tests::local_tui_dogfood_real_git_flow_survives_restart_without_duplicates` |
| HIER-01 through HIER-04, SURF-01 | `ui::tests::hierarchy_viewport_matrix_renders_without_panic_and_preserves_semantics` |
| WORK-03, WORK-05, SURF-01 | `ui::tests::hierarchy_modals_name_exact_targets_and_distinguish_close_from_remove` |
| HIER-01, HIER-03, HIER-04 | `ui::tests::hierarchy_unicode_width_scroll_and_selection_band_are_cell_correct` |
| SURF-05 | `api::tests::flat_session_api_remains_a_non_hierarchical_compatibility_projection` |
| REL-02, REL-03 | Package metadata/version assertions and non-publishing artifact build commands defined by the release-readiness plan |

## Wave 0 Requirements

- [ ] `baude/src/hierarchy.rs` projection fixtures and exact hierarchy tests.
- [ ] Core lifecycle capability test in `baude-core/src/lifecycle.rs`.
- [ ] App action, resize, compatibility, and real-Git dogfood tests.
- [ ] Ratatui viewport, modal, Unicode-width, and selection tests.
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

## Validation Sign-Off

- [ ] Every task has guarded exact automated verification.
- [ ] Sampling continuity has no three-task gap.
- [ ] Wave 0 covers every missing exact test.
- [ ] Full local implementation gate passes.
- [ ] Deferred certification gates pass.
- [ ] `nyquist_compliant: true` is set only from observed evidence.

**Approval:** pending
