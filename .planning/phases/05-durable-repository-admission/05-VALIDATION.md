---
phase: 5
slug: durable-repository-admission
status: draft
nyquist_compliant: true
wave_0_complete: false
created: 2026-08-30
---

# Phase 5 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness via Cargo |
| **Config file** | Workspace `Cargo.toml` and inline `#[cfg(test)]` modules |
| **Quick run command** | `cargo test -p baude-core git:: && cargo test -p baude-core persist::` |
| **Full suite command** | `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` |
| **Estimated runtime** | ~90 seconds |

## Sampling Rate

- **After every task commit:** Run the touched crate/module test filter.
- **After every plan wave:** Run `cargo test`.
- **Before verification:** Run `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.
- **Max feedback latency:** 120 seconds.

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 05-01-01 | 01 | 0 | REPO-01 | T-05-01, T-05-04 | Canonical Git identity rejects path alias confusion and malformed porcelain without retaining partial topology | real-Git integration | `cargo test -p baude-core git::tests::admission_identity` | ❌ W0 | ⬜ pending |
| 05-01-02 | 01 | 0 | REPO-04, REPO-06 | T-05-02, T-05-03 | Default resolution never switches, fetches, or guesses; worktree reuse/create is inventory-verified | real-Git integration | `cargo test -p baude-core git::tests::default_branch && cargo test -p baude-core git::tests::default_worktree` | ❌ W0 | ⬜ pending |
| 05-02-01 | 02 | 0 | PERS-01 | T-05-05, T-05-08 | Validated workspace-scoped durable records round-trip without cross-wiring | unit | `cargo test -p baude-core persist::tests::current_round_trip` | ❌ W0 | ⬜ pending |
| 05-02-02 | 02 | 0 | PERS-02 | T-05-07, T-05-08 | Selected legacy state migrates once by reconciled identity without field loss or fallback merging | fixture/integration | `cargo test -p baude-core persist::tests::legacy_migration` | ❌ W0 | ⬜ pending |
| 05-02-03 | 02 | 0 | PERS-04 | T-05-05, T-05-06 | Malformed state is blocking and failed writes preserve prior bytes | unit/integration | `cargo test -p baude-core persist::tests::atomic` | ❌ W0 | ⬜ pending |
| 05-03-01 | 03 | 1 | REPO-02, REPO-03 | T-05-10, T-05-12, T-05-13 | Admission saves before spawn and ensures at most one active-backend primary process | unit/integration | `cargo test -p baude repository_admission` | ❌ W0 | ⬜ pending |
| 05-03-02 | 03 | 1 | PERS-03 | T-05-09, T-05-11 | Reconciliation blocks stale identity and malformed persisted evidence before launch or mutation | real-Git + App integration | `cargo test -p baude-core git::tests::reconciliation` | ❌ W0 | ⬜ pending |
| 05-03-03 | 03 | 1 | REPO-02, REPO-03, PERS-03 | T-05-10, T-05-11, T-05-13 | All local routes converge while Manager strict-load failures block save/spawn and backend identity remains workspace-selected | unit/integration | `cargo test -p baude admission_routes && cargo test -p bauded manager_persistence` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

## Wave 0 Requirements

- [ ] Add standard-library temporary Git repository fixtures in `baude-core/src/git.rs` tests.
- [ ] Add injectable persistence roots so tests never touch user config.
- [ ] Add complete legacy JSON fixtures for local and daemon state.
- [ ] Extract a pure primary-session dispatch decision or injectable spawn seam.
- [ ] Add an atomic-save failure seam proving destination preservation.

## Manual-Only Verifications

All phase behaviors have automated verification.

## Validation Sign-Off

- [x] All tasks have automated verification or Wave 0 dependencies.
- [x] Sampling continuity prevents three consecutive tasks without automated verification.
- [x] Wave 0 covers all missing references.
- [x] No watch-mode flags.
- [x] Feedback latency target is below 120 seconds.
- [x] Task 05-03-03 uses focused App/Manager filters targeted below 30 seconds; full `cargo test` remains a wave/plan gate.
- [x] `nyquist_compliant: true` is set.

**Approval:** approved 2026-08-30
