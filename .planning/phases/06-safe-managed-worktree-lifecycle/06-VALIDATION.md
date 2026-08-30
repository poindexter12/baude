---
phase: 6
slug: safe-managed-worktree-lifecycle
status: draft
nyquist_compliant: true
wave_0_complete: false
created: 2026-08-30
---

# Phase 6 - Validation Strategy

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness via Cargo |
| **Config file** | Workspace `Cargo.toml` and inline `#[cfg(test)]` modules |
| **Quick run command** | `cargo test -p baude-core git::tests::lifecycle -- --nocapture && cargo test -p baude-core lifecycle::tests -- --nocapture` |
| **Full suite command** | `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` |
| **Estimated runtime** | ~120 seconds |

## Sampling Rate

- **After every task commit:** Run the touched lifecycle test filter.
- **After every plan wave:** Run `cargo test`.
- **Before verification:** Run the full formatting, Clippy, and workspace test gate.
- **Max feedback latency:** 150 seconds.

## Per-Requirement Verification Map

The planner must replace provisional task IDs with every final implementation task and preserve these commands.

| Task ID | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | Status |
|---------|-------------|------------|-----------------|-----------|-------------------|--------|
| 06-W0-01 | WORK-01 | T-06-01 | Only literal valid new or eligible local branches activate from the verified default | real-Git | `cargo test -p baude-core git::tests::lifecycle::branch_activation -- --nocapture` | ⬜ pending |
| 06-W0-02 | WORK-02 | T-06-02 | Invalid refs, collisions, and occupied branches leave no partial state/runtime | real-Git/failure | `cargo test -p baude-core git::tests::lifecycle::creation_safety -- --nocapture` | ⬜ pending |
| 06-W0-03 | WORK-03 | T-06-03 | Close preserves child/context and does not stop runtime before durable commit | contract/failure | `cargo test lifecycle_close` | ⬜ pending |
| 06-W0-04 | WORK-04 | T-06-04 | Reopen reconciles and reserves exactly one targeted backend runtime | race/contract | `cargo test lifecycle_reopen` | ⬜ pending |
| 06-W0-05 | WORK-05 | T-06-05 | Clean confirmed removal uses two preflights and retains branch/parent | real-Git/orchestration | `cargo test lifecycle_remove_clean` | ⬜ pending |
| 06-W0-06 | WORK-06 | T-06-06 | Dirty, conflict, submodule, lock, and unknown states fail closed before mutation | real-Git matrix | `cargo test -p baude-core git::tests::lifecycle::removal_preflight -- --nocapture` | ⬜ pending |

## Wave 0 Requirements

- [ ] Typed ref classification and lifecycle Git fixtures.
- [ ] Status/submodule/topology removal-preflight matrix.
- [ ] Failure-injection seams for create, close, reopen, remove, and postcondition persistence.
- [ ] Deterministic per-repository race/reservation tests.
- [ ] App and Manager effect-adapter contract tests for both backends.

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Real Claude Code/OpenCode targeted resume after close | WORK-03, WORK-04 | External interactive CLI behavior | Close and reopen one retained worktree in each workspace and confirm the same conversation resumes without duplicate agents. |

## Validation Sign-Off

- [x] Every requirement has an automated test architecture.
- [x] Wave 0 covers missing fixtures and failure seams.
- [x] No watch-mode flags.
- [x] `nyquist_compliant: true` is set.

**Approval:** approved 2026-08-30
