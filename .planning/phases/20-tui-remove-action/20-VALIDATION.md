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
| 20-TBD | TBD | TBD | TUI-01 | — | `D` yields `RemoveProject` on each local kind, `Refuse` on remote; footer and help list it | unit (App + ui render) | `cargo test -p baude sidebar_action_d` / `cargo test -p baude footer` | ❌ W0 | ⬜ pending |
| 20-TBD | TBD | TBD | TUI-02 | — | Modal lines equal preview report fields on each row kind, including live runtime | App-level, cloned-repo fixture + real process group | `cargo test -p baude confirm_remove_modal` | ❌ W0 | ⬜ pending |
| 20-TBD | TBD | TBD | TUI-03 | — | Repository parent lists every child; confirm drops all in one save | App-level | `cargo test -p baude remove_repository_parent` | ❌ W0 | ⬜ pending |
| 20-TBD | TBD | TBD | TUI-04 | — | Forced kill and save failures keep rows visible with error; retry succeeds; no handle lost | App-level | `cargo test -p baude remove_project_failure` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] Expose or locate Phase 18 process-group and teardown doubles for `baude` tests (research A1)
- [ ] Confirm `atomic_failure_for_test` injection reaches the `remove` save closure (research A2)
- [ ] New test functions in existing `app.rs` and `ui.rs` test modules (no new framework)

---

## Manual-Only Verifications

- Visual check of the confirmation modal and footer at a narrow terminal width.
