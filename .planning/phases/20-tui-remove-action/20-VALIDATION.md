---
phase: "20"
slug: "tui-remove-action"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-11-04"
---

# Phase 20 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `cargo test` (unit tests in `#[cfg(test)] mod tests` inside `app.rs`, `ui.rs`, `removal.rs`) |
| **Config file** | none; `scripts/gate.sh` is the suite |
| **Quick run command** | `cargo test -p baude remove_project` |
| **Full suite command** | `scripts/gate.sh` |
| **Estimated runtime** | ~60 seconds quick, several minutes full |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p baude remove_project`
- **After every plan wave:** Run `scripts/gate.sh`
- **Before `/gsd-verify-work`:** `scripts/gate.sh` and `scripts/gate.sh --linux` must be green
- **Max feedback latency:** ~60 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 20-02-T1, 20-03-T2 | 02, 03 | 2, 3 | TUI-01 | T-20-03, T-20-06 | `D` yields `RemoveProject` on each local kind, `Refuse` on remote; footer and help list it | unit (App + ui render) | `cargo test -p baude app::tests::sidebar_action_d -- --test-threads=1` / `cargo test -p baude ui::tests -- --test-threads=1` | ❌ W0 | ⬜ pending |
| 20-01-T1..T3, 20-02-T2, 20-03-T1 | 01, 02, 03 | 1, 2, 3 | TUI-02 | T-20-01, T-20-07 | Modal lines equal preview report fields on each row kind, including live runtime | App-level, cloned-repo fixture + real process group | `cargo test -p baude ui::tests::remove_project_lines -- --test-threads=1` / `cargo test -p baude app::tests::d_opens_confirm_remove_modal -- --test-threads=1` | ❌ W0 | ⬜ pending |
| 20-02-T3 | 02 | 2 | TUI-03 | T-20-02, T-20-04 | Repository parent lists every child; confirm drops all in one save | App-level | `cargo test -p baude app::tests::confirm_remove_repository_parent_drops_every_child_in_one_save -- --test-threads=1` | ❌ W0 | ⬜ pending |
| 20-04-T1, 20-04-T2 | 04 | 4 | TUI-04 | T-20-05, T-20-09, T-20-10 | Forced kill and save failures keep rows visible with error; retry succeeds; no handle lost | App-level | `cargo test -p baude app::tests::remove_project_ -- --test-threads=1` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] Expose or locate Phase 18 process-group and teardown doubles for `baude` tests (research A1)
- [ ] Confirm `atomic_failure_for_test` injection reaches the `remove` save closure (research A2)
- [ ] New test functions in existing `app.rs` and `ui.rs` test modules (no new framework)

---

## Manual-Only Verifications

- Visual check of the confirmation modal and footer at a narrow terminal width.
