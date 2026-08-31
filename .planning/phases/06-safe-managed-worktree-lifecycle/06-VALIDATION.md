---
phase: "06"
slug: "safe-managed-worktree-lifecycle"
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-08-30"
---

# Phase 06 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness via Cargo 1.98.0 |
| **Config file** | Workspace `Cargo.toml`; inline unit and contract tests in tracked source files |
| **Quick run command** | `cargo test lifecycle_protocol_contract -- --nocapture` |
| **Full suite command** | `cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test` |
| **Estimated runtime** | ~120 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test lifecycle_protocol_contract -- --nocapture`
- **After every plan wave:** Run `cargo test -p baude-core lifecycle::tests -- --nocapture && cargo test lifecycle_ -- --nocapture`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 120 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 06-07-01 | 07 | 1 | CORE-01, CORE-02 | T-06-01 | Protected lifecycle states reject illegal transitions without effects | table-driven unit | `cargo test -p baude-core lifecycle::tests::legal_transition_table -- --nocapture` | ❌ W0 | ⬜ pending |
| 06-07-02 | 07 | 1 | CORE-03, CORE-05 | T-06-02 | Exact agent and shell identities remain durable across teardown and rollback | process and serialization integration | `cargo test -p baude-core lifecycle::tests::process_ownership -- --nocapture && cargo test -p baude-core persist::tests::lifecycle_schema_v2 -- --nocapture` | ❌ W0 | ⬜ pending |
| 06-07-03 | 07 | 2 | CORE-04 | T-06-03 | App and Manager produce identical normalized traces and final ownership | mirrored adapter contract | `cargo test lifecycle_protocol_contract -- --nocapture` | ❌ W0 | ⬜ pending |
| 06-07-04 | 07 | 2 | CORE-06 | T-06-04 | Startup and rollback converge without duplicate or orphaned processes | crash and recovery integration | `cargo test lifecycle_startup_recovery -- --nocapture` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] Canonical legal-transition and trace fixtures in `baude-core/src/lifecycle.rs`
- [ ] Schema-v1 protected-state fixtures in `baude-core/src/persist.rs`
- [ ] Agent, shell, and abrupt-owner-death fixtures in `baude-core/src/session.rs` and `baude-core/src/pty.rs`
- [ ] Mirrored App/Manager scripted effect implementations and identical-vector tests in `baude/src/app.rs` and `bauded/src/manager.rs`

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Linux process identity and owner-death behavior | CORE-03, CORE-06 | No Linux Rust target is installed locally | Require the normal Linux CI job to pass the full suite before phase completion |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
