---
phase: 6
slug: safe-managed-worktree-lifecycle
status: planned
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
| **Quick run command** | Run the task-specific filter from the exact map below; each filter targets <30 seconds where practical |
| **Full suite command** | `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` |
| **Estimated runtime** | Focused task filter: <30 seconds where practical; final full suite: ~120 seconds |

## Sampling Rate

- **After every task commit:** Run the exact focused lifecycle filter mapped below (target <30 seconds).
- **After every plan wave:** Run the union of focused filters for plans in that wave; reserve `cargo test` for the final phase gate unless cross-crate changes require it earlier.
- **Before verification:** Run the full formatting, Clippy, and workspace test gate.
- **Focused feedback target:** 30 seconds; full-gate maximum remains 150 seconds.

## Per-Requirement Verification Map

Every final implementation task appears exactly once below and points to the threat control implemented by that task. Requirement IDs may repeat here because this is a task-level validation map; each requirement appears exactly once across PLAN frontmatter.

| Task ID | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | Status |
|---------|-------------|------------|-----------------|-----------|-------------------|--------|
| 06-01-01 | WORK-01 | T-06-01 | Literal valid new/local branches activate from the verified default; occupied same-repository refs reuse without force | real-Git/TDD | `cargo test -p baude-core git::tests::lifecycle::branch_activation -- --nocapture` | ⬜ pending |
| 06-01-02 | WORK-01 | T-06-02, T-06-SC | App and Manager persist one verified checkout before one runtime spawn; manifests remain unchanged | adapter contract/TDD | `cargo test lifecycle_create_activate -- --nocapture` | ⬜ pending |
| 06-02-01 | WORK-02 | T-06-03 | Invalid refs, filesystem/inventory collisions, malformed Git output, and forbidden argv reject before mutation | real-Git/failure TDD | `cargo test -p baude-core git::tests::lifecycle::creation_safety -- --nocapture` | ⬜ pending |
| 06-02-02 | WORK-02 | T-06-04 | Serialized creates and injected Git/save/spawn failures leave no duplicate or ambiguous partial state | race/adapter TDD | `cargo test lifecycle_creation_rollback -- --nocapture` | ⬜ pending |
| 06-03-01 | WORK-06 | T-06-05 | Tracked, untracked, ignored, conflicted, malformed, and command-failing status observations block | parser/real-Git TDD | `cargo test -p baude-core git::tests::lifecycle::removal_preflight -- --nocapture` | ⬜ pending |
| 06-03-02 | WORK-06 | T-06-06 | Main/external/locked/prunable/moved and every submodule uncertainty cannot produce a safe token | topology/submodule TDD | `cargo test -p baude-core git::tests::lifecycle::removal_topology -- --nocapture` | ⬜ pending |
| 06-04-01 | WORK-03 | T-06-07 | Retained schema preserves all close metadata and opaque resume IDs compatibly | schema/transition TDD | `cargo test -p baude-core lifecycle::tests::close -- --nocapture` | ⬜ pending |
| 06-04-02 | WORK-03 | T-06-08 | App/Manager save inactive intent before stop and preserve a live runtime on precommit failure | adapter/failure TDD | `cargo test lifecycle_close -- --nocapture` | ⬜ pending |
| 06-05-01 | WORK-04 | T-06-09 | Targeted resume IDs are opaque process data for both backends and cannot become shell syntax | backend/security TDD | `cargo test -p baude-core backend:: -- --nocapture` | ⬜ pending |
| 06-05-02 | WORK-04 | T-06-10 | Reopen blocks stale topology and plans durable activation before one runtime effect | transition/race TDD | `cargo test -p baude-core lifecycle::tests::reopen -- --nocapture` | ⬜ pending |
| 06-05-03 | WORK-04 | T-06-11 | App/Manager reopen retained main/worktree children with one checkout-key runtime | adapter/race TDD | `cargo test lifecycle_reopen -- --nocapture` | ⬜ pending |
| 06-06-01 | WORK-05 | T-06-12 | Plain verified remove proves path/inventory absence while exact branch and repository parent remain | real-Git/postcondition TDD | `cargo test -p baude-core git::tests::lifecycle::remove_postconditions -- --nocapture` | ⬜ pending |
| 06-06-02 | WORK-05 | T-06-14 | Double preflight, stop-between-checks, compensation, and commit-stage outcomes preserve context | race/orchestration TDD | `cargo test lifecycle_remove_clean -- --nocapture` | ⬜ pending |
| 06-06-03 | WORK-05 | T-06-13 | Distinct target-naming confirmation cancels/blocks without mutation and cannot bypass shared safety | modal/dispatch TDD | `cargo test -p baude remove_confirmation -- --nocapture` | ⬜ pending |

## Wave 0 Requirements

- [ ] 06-01-01 and 06-02-01 create typed ref/path Git fixtures before production behavior.
- [ ] 06-03-01 and 06-03-02 create the status/submodule/topology preflight matrix before production behavior.
- [ ] 06-02-02, 06-04-02, 06-05-03, and 06-06-02 create stage-specific failure seams before production behavior.
- [ ] 06-02-02, 06-05-02, and 06-06-02 create deterministic repository/checkout reservation race tests.
- [ ] 06-01-02, 06-04-02, 06-05-03, and 06-06-02 run shared App/Manager effect vectors.

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
