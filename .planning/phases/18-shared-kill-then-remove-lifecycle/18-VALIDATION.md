---
phase: "18"
slug: "shared-kill-then-remove-lifecycle"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-11-02"
---

# Phase 18 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (Rust built-in harness; in-module `#[cfg(test)]` suites, `test-support` feature for `baude_core::testing`) |
| **Config file** | none (Cargo workspace; `scripts/gate.sh` is the authoritative runner and already exists) |
| **Quick run command** | `cargo test -p baude-core removal::tests -- --test-threads=1` |
| **Full suite command** | `scripts/gate.sh` (macOS) and `scripts/gate.sh --linux` (rust:1-bookworm, uid 1000) for the signal and procfs paths |
| **Estimated runtime** | quick ~30-60 s once live process doubles exist (5 s wait budgets per kill); `scripts/gate.sh` several minutes; `--linux` adds the container build |

---

## Sampling Rate

- **After every task commit:** Run the task's own `<automated>` command (the plan-local filter: `removal::tests`, `git::tests::forced_removal`, or `repository::tests`), always with `-- --test-threads=1`
- **After every plan wave:** Run `cargo test -p baude-core -- --test-threads=1` and `cargo clippy -p baude-core --all-targets -- -D warnings`
- **Before `/gsd-verify-work`:** `scripts/gate.sh` and `scripts/gate.sh --linux` must both exit 0, read directly, never through a pipe (plan 18-07)
- **Max feedback latency:** 60 seconds for the quick command

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 18-01-01 | 01 | 1 | RMV-05, RMV-04, RMV-02, RMV-03 | T-18-01, T-18-02, T-18-03 | preview_removal performs no I/O; Main and Standalone rows classify as KeepDirectory | unit (TDD RED) | `cargo test -p baude-core removal::tests -- --test-threads=1` (expects 6 failed, 0 compile errors) | ❌ created by this task (`baude-core/src/removal.rs` is new) | ⬜ pending |
| 18-01-02 | 01 | 1 | RMV-05, RMV-04, RMV-02, RMV-03 | T-18-01, T-18-02, T-18-03 | select_rows rejects unknown targets with a typed error; disk rule is a data classification | unit (TDD GREEN) | `cargo test -p baude-core removal::tests -- --test-threads=1` | ✅ | ⬜ pending |
| 18-01-03 | 01 | 1 | RMV-05 | — | no behavior change | unit (REFACTOR) | `cargo test -p baude-core removal::tests -- --test-threads=1 && cargo fmt --check` | ✅ | ⬜ pending |
| 18-02-01 | 02 | 1 | RMV-01, RMV-07 | T-18-04, T-18-05, T-18-06, T-18-07 | path outside worktrees_base() is refused before any filesystem call; no refs/heads entry changes | unit (TDD RED) | `cargo test -p baude-core git::tests::forced_removal -- --test-threads=1` (expects RED) | ✅ `baude-core/src/git.rs` | ⬜ pending |
| 18-02-02 | 02 | 1 | RMV-01, RMV-07 | T-18-04, T-18-05, T-18-06, T-18-07 | forced remove plus prune fallback only inside the managed root; postcondition read from git worktree list | unit (TDD GREEN) | `cargo test -p baude-core git::tests::forced_removal -- --test-threads=1` | ✅ | ⬜ pending |
| 18-02-03 | 02 | 1 | RMV-01 | — | verified path behavior unchanged | unit (REFACTOR) | `cargo test -p baude-core git::tests::forced_removal git::tests::lifecycle -- --test-threads=1 && cargo fmt --check` | ✅ | ⬜ pending |
| 18-03-01 | 03 | 1 | RMV-04, RMV-06 | T-18-08 | batch removal leaves validate() Ok; no dangling child or orphaned parent | unit (TDD RED) | `cargo test -p baude-core repository::tests::remove_rows -- --test-threads=1` (expects RED) | ✅ `baude-core/src/repository.rs` | ⬜ pending |
| 18-03-02 | 03 | 1 | RMV-04, RMV-06 | T-18-08 | remove_rows returns exactly the removed rows for the report | unit (TDD GREEN) | `cargo test -p baude-core repository::tests -- --test-threads=1` | ✅ | ⬜ pending |
| 18-03-03 | 03 | 1 | RMV-06 | — | remove_standalone_session keeps its contract | unit (REFACTOR) | `cargo test -p baude-core repository::tests -- --test-threads=1 && cargo fmt --check` | ✅ | ⬜ pending |
| 18-04-01 | 04 | 2 | KILL-01, KILL-02, KILL-03, RMV-06, RMV-05 | T-18-09, T-18-10, T-18-11 | handle pid must match persisted runtime before any signal; survivor leaves row, file and directory untouched | integration, live process doubles (TDD RED) | `cargo test -p baude-core removal::tests -- --test-threads=1` (expects 5 failed, 6 passed) | ✅ | ⬜ pending |
| 18-04-02 | 04 | 2 | KILL-01, KILL-02, KILL-03, RMV-06, RMV-05 | T-18-09, T-18-10, T-18-11 | kill, then disk, then one save; state committed only after save Ok or replacement_committed | integration (TDD GREEN) | `cargo test -p baude-core removal::tests -- --test-threads=1` | ✅ | ⬜ pending |
| 18-04-03 | 04 | 2 | RMV-05 | — | report assembly shared, no behavior change | integration (REFACTOR) | `cargo test -p baude-core removal::tests -- --test-threads=1 && cargo fmt --check` | ✅ | ⬜ pending |
| 18-05-01 | 05 | 3 | RMV-06 | T-18-12, T-18-13 | rename failure keeps prior state and file bytes; directory-sync failure surfaces SavedDegraded; one save per row set | integration, injected AtomicFailure | `cargo test -p baude-core removal::tests -- --test-threads=1` | ✅ | ⬜ pending |
| 18-05-02 | 05 | 3 | RMV-04, RMV-06 | T-18-12 | crash before save leaves parent and child both present and validate() Ok; reconcile marks Protected(Missing); retry completes | integration | `cargo test -p baude-core removal::tests::crash_before_save_reconciles_missing -- --test-threads=1` | ✅ | ⬜ pending |
| 18-06-01 | 06 | 4 | RMV-01, RMV-02, RMV-03, RMV-04, RMV-07 | T-18-14 | cloned fixture allocates worktrees only under the redirected managed root | fixture | `cargo test -p baude-core removal::tests::cloned_topology_places_worktrees_under_managed_root -- --test-threads=1 && cargo clippy -p baude-core --all-targets -- -D warnings` | ✅ `baude-core/src/testing.rs` | ⬜ pending |
| 18-06-02 | 06 | 4 | RMV-02, RMV-03, RMV-07, RMV-01 | T-18-14, T-18-15 | main checkout and standalone directories survive; branch snapshot identical before and after | integration, cloned and git init fixtures | `cargo test -p baude-core removal::tests -- --test-threads=1` | ✅ | ⬜ pending |
| 18-06-03 | 06 | 4 | RMV-01, RMV-04, RMV-07, KILL-01 | T-18-14, T-18-15 | unclean cwd-held managed worktree deleted with group extinct; parent fan-out keeps main and every branch | integration, cloned fixture | `cargo test -p baude-core removal::tests -- --test-threads=1` | ✅ | ⬜ pending |
| 18-07-01 | 07 | 5 | KILL-01, KILL-02, KILL-03 | T-18-17 | gate leak check: no fixture outside redirected roots; no test ignored | gate | `scripts/gate.sh` | ✅ `scripts/gate.sh` | ⬜ pending |
| 18-07-02 | 07 | 5 | KILL-01, KILL-02, KILL-03 | T-18-16 | kill and reap proven on Linux procfs as uid 1000 | gate (container) | `scripts/gate.sh --linux` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements. No `<automated>` command carries a `MISSING — Wave 0` sentinel: every TDD plan's RED task authors its tests inside the module it changes (`removal::tests`, `git::tests::forced_removal`, `repository::tests::remove_rows`), `baude-core/src/removal.rs` is created by task 18-01-01, and the shared fixtures (`RemovalFixture` in 18-04-01, `testing::cloned_topology` in 18-06-01) are delivered by the plans that first need them. `scripts/gate.sh` already exists.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| — | — | — | — |

All phase behaviors have automated verification. The only environment dependency is Docker for `scripts/gate.sh --linux` (plan 18-07 task 2 precondition); if Docker is unavailable the Linux gate is reported as a blocker, not skipped.

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies (19 tasks, 19 `<automated>` commands, each with `<fails_when>`)
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references (none emitted)
- [x] No watch-mode flags
- [x] Feedback latency < 60s for the quick command
- [ ] `nyquist_compliant: true` set in frontmatter (set by validate-phase after execution)

**Approval:** pending
