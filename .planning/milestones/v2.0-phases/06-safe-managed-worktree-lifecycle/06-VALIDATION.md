---
phase: "06"
slug: "safe-managed-worktree-lifecycle"
status: validated
nyquist_compliant: true
wave_0_complete: false
created: "2026-08-30"
---

# Phase 06 - Validation Strategy

> Wave 6 implementation feedback contract. Local implementation may finish and produce 06-07-SUMMARY.md tonight; certification and phase completion remain pending until morning.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness via Cargo 1.98.0 |
| **Config file** | Workspace `Cargo.toml`; inline unit and contract tests in tracked source files |
| **Filtered-test rule** | First run Cargo `-- --list`, then `rg -x` the exact expected `module::test: test`, then run that exact test with Cargo `--exact`; a zero-test Cargo success is never evidence |
| **Overnight local gate** | `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and unchanged `Cargo.toml`/`Cargo.lock` |
| **Certification state** | Linux/runtime matrix, descendant group extinction, independent deep review, phase verification, Nyquist approval, requirements, and Phase 6 completion pending until morning |

---

## Sampling Rate

- **After 06-07-01:** Run both exact Task 1 list/assert/run pairs.
- **After 06-07-02:** Run all three exact Task 2 list/assert/run pairs.
- **After 06-07-03:** Run both mirrored exact tests, the named historical exact tests, and the full overnight local gate.
- **Feedback latency:** Run the focused exact tests before the full workspace suite.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirements | Threat Refs | Exact Planned Tests | Files | Status |
|---------|------|------|--------------|-------------|---------------------|-------|--------|
| 06-07-01 | 07 | 6 | CORE-01, CORE-02, CORE-05 | T-06-01, T-06-04 | `lifecycle::tests::lifecycle_protocol_core_legal_transition_table`; `persist::tests::lifecycle_schema_v1_migrates_protected_states_to_v3`; `persist::tests::schema_v2_migrates_strictly_to_v3` | `baude-core/src/lifecycle.rs`; `baude-core/src/persist.rs` | ✅ green locally |
| 06-07-02 | 07 | 6 | CORE-03, CORE-06 | T-06-02, T-06-03, T-06-05 | `session::tests::lifecycle_process_contract_exact_ownership`; `pty::tests::pre_exec_registration_gate_owner_death_and_release`; `lifecycle::tests::lifecycle_startup_recovery_is_idempotent` | `baude-core/src/session.rs`; `baude-core/src/pty.rs`; `baude-core/src/lifecycle.rs` | ✅ green locally |
| 06-07-03 | 07 | 6 | CORE-01, CORE-03, CORE-04, CORE-06 | T-06-01 through T-06-06 | `app::tests::lifecycle_protocol_contract_app_vectors`; `manager::tests::lifecycle_protocol_contract_manager_vectors`; historical exact tests named in PLAN | `baude-core/src/lifecycle.rs`; `baude/src/app.rs`; `bauded/src/manager.rs` | ✅ green locally |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Exact Automated Commands

### 06-07-01

<automated>cargo test -p baude-core -- --list | rg -x 'lifecycle::tests::lifecycle_protocol_core_legal_transition_table: test' &amp;&amp; cargo test -p baude-core lifecycle::tests::lifecycle_protocol_core_legal_transition_table -- --exact --nocapture</automated>
<fails_when>The exact reducer test is absent, `rg -x` matches zero tests, or the exact test exits nonzero.</fails_when>

<automated>cargo test -p baude-core -- --list | rg -x 'persist::tests::lifecycle_schema_v1_migrates_protected_states_to_v3: test' &amp;&amp; cargo test -p baude-core persist::tests::lifecycle_schema_v1_migrates_protected_states_to_v3 -- --exact --nocapture &amp;&amp; cargo test -p baude-core -- --list | rg -x 'persist::tests::schema_v2_migrates_strictly_to_v3: test' &amp;&amp; cargo test -p baude-core persist::tests::schema_v2_migrates_strictly_to_v3 -- --exact --nocapture</automated>
<fails_when>The exact migration test is absent, `rg -x` matches zero tests, or the exact test exits nonzero.</fails_when>

### 06-07-02

<automated>cargo test -p baude-core -- --list | rg -x 'session::tests::lifecycle_process_contract_exact_ownership: test' &amp;&amp; cargo test -p baude-core session::tests::lifecycle_process_contract_exact_ownership -- --exact --nocapture</automated>
<fails_when>The exact ownership test is absent, `rg -x` matches zero tests, or the exact test exits nonzero.</fails_when>

<automated>cargo test -p baude-core -- --list | rg -x 'pty::tests::pre_exec_registration_gate_owner_death_and_release: test' &amp;&amp; cargo test -p baude-core pty::tests::pre_exec_registration_gate_owner_death_and_release -- --exact --nocapture</automated>
<fails_when>The exact pre-exec gate test is absent, `rg -x` matches zero tests, or the exact test exits nonzero.</fails_when>

<automated>cargo test -p baude-core -- --list | rg -x 'lifecycle::tests::lifecycle_startup_recovery_is_idempotent: test' &amp;&amp; cargo test -p baude-core lifecycle::tests::lifecycle_startup_recovery_is_idempotent -- --exact --nocapture</automated>
<fails_when>The exact startup recovery test is absent, `rg -x` matches zero tests, or the exact test exits nonzero.</fails_when>

### 06-07-03

<automated>cargo test -p baude -- --list | rg -x 'app::tests::lifecycle_protocol_contract_app_vectors: test' &amp;&amp; cargo test -p baude app::tests::lifecycle_protocol_contract_app_vectors -- --exact --nocapture</automated>
<fails_when>The exact App vector test is absent, `rg -x` matches zero tests, or the exact test exits nonzero.</fails_when>

<automated>cargo test -p bauded -- --list | rg -x 'manager::tests::lifecycle_protocol_contract_manager_vectors: test' &amp;&amp; cargo test -p bauded manager::tests::lifecycle_protocol_contract_manager_vectors -- --exact --nocapture</automated>
<fails_when>The exact Manager vector test is absent, `rg -x` matches zero tests, or the exact test exits nonzero.</fails_when>

<automated>cargo test -p baude -- --list | rg -x 'app::tests::lifecycle_create_activate_local_persists_once_and_reuses_runtime: test' &amp;&amp; cargo test -p baude app::tests::lifecycle_create_activate_local_persists_once_and_reuses_runtime -- --exact --nocapture</automated>
<fails_when>The exact activation regression test is absent, `rg -x` matches zero tests, or the exact test exits nonzero.</fails_when>

<automated>cargo test -p baude -- --list | rg -x 'app::tests::lifecycle_remove_clean_local_rechecks_after_stop_and_compensates_a_race: test' &amp;&amp; cargo test -p baude app::tests::lifecycle_remove_clean_local_rechecks_after_stop_and_compensates_a_race -- --exact --nocapture</automated>
<fails_when>The exact removal regression test is absent, `rg -x` matches zero tests, or the exact test exits nonzero.</fails_when>

<automated>cargo test -p bauded -- --list | rg -x 'api::tests::real_atomic_persistence_failures_are_503_for_every_mutation: test' &amp;&amp; cargo test -p bauded api::tests::real_atomic_persistence_failures_are_503_for_every_mutation -- --exact --nocapture</automated>
<fails_when>The exact flat compatibility test is absent, `rg -x` matches zero tests, or the exact test exits nonzero.</fails_when>

<automated>cargo fmt --all -- --check</automated>
<fails_when>Formatting differs or the command exits nonzero.</fails_when>

<automated>cargo clippy --all-targets -- -D warnings</automated>
<fails_when>Clippy emits a warning/error or exits nonzero.</fails_when>

<automated>cargo test</automated>
<fails_when>Any workspace test fails or the command exits nonzero.</fails_when>

<automated>git diff --exit-code -- Cargo.toml Cargo.lock</automated>
<fails_when>A manifest/lockfile changed, a dependency was added, or the command exits nonzero.</fails_when>

---

## Wave 6 Test Additions

- [x] `CanonicalLifecycleVector`, `canonical_lifecycle_contract_vectors()`, `normalize_lifecycle_trace()`, legal-transition tests, and startup recovery tests in `baude-core/src/lifecycle.rs`.
- [x] Schema-v1 protected-state fixtures in `baude-core/src/persist.rs`.
- [x] Exact agent/shell ownership fixtures in `baude-core/src/session.rs`.
- [x] Selected private-stdin pre-exec gate and negative-PGID whole-group extinction fixtures in `baude-core/src/pty.rs`.
- [x] Mirrored App/Manager exact-vector tests in `baude/src/app.rs` and `bauded/src/manager.rs`.

## Observed Local Evidence (2026-08-30)

- All exact list/assert/run commands for tasks 06-07-01 through 06-07-03 passed on macOS.
- Mirrored App and Manager canonical success and injected persistence/effect failure traces passed.
- `cargo fmt --all -- --check` passed.
- `cargo clippy --all-targets -- -D warnings` passed.
- `cargo test` passed: 32 App tests, 212 core tests, 78 daemon tests, and 0 doc-test failures.
- `git diff --exit-code -- Cargo.toml Cargo.lock` passed; no dependency or manifest changes.
- This is local implementation evidence only. Linux/runtime certification and independent review remain pending.

---

## Morning Certification Gates (Pending)

| Gate | Requirements | Pending Evidence | Completion Effect |
|------|--------------|------------------|-------------------|
| Linux synchronized gate/release matrix and descendant process-group extinction | CORE-03, CORE-06 | Normal Linux runtime/CI execution | Blocks cross-platform claim and requirement completion, not tonight's implementation/summary |
| Independent deep lifecycle review | CORE-01 through CORE-06 | Zero unresolved Critical/High findings | Blocks Phase 6 completion only |
| Phase verification | CORE-01 through CORE-06 | All six requirements verified from implementation and test evidence | Blocks requirement checkoff and Phase 6 completion |
| Nyquist approval | CORE-01 through CORE-06 | Validation audit passes and frontmatter is updated from evidence | Blocks validation approval and Phase 6 completion |

Do not push, open a PR, publish, or mark Phase 6/CORE requirements complete as part of overnight implementation.

---

## Validation Sign-Off

- [x] Exactly tasks 06-07-01 through 06-07-03 are mapped, all in wave 6.
- [x] Every filtered verification lists tests and `rg -x` asserts an exact planned name before Cargo runs with `--exact`.
- [x] Every runnable PLAN command has an immediately following `<fails_when>`.
- [x] Exact test additions implemented and observed green locally.
- [x] Full overnight local gate observed green.
- [ ] Linux/runtime certification observed green.
- [ ] Independent deep review observed clean.
- [ ] Phase verification observed clean.
- [ ] `nyquist_compliant: true` set only after the morning evidence exists.

**Approval:** pending

## Validation Audit 2026-09-02

| Metric | Count |
|--------|-------|
| Gaps found | 1 |
| Resolved | 1 |
| Escalated | 0 |

Every exact per-task command re-executed green against commit `f3766c3`. The
one gap was nominal, not behavioral: the phase-07 schema-v3 bump renamed
`lifecycle_schema_v2_migrates_protected_states` into
`lifecycle_schema_v1_migrates_protected_states_to_v3` plus
`schema_v2_migrates_strictly_to_v3`, both green; the map and exact commands
above now name them. `cargo fmt --check` and the manifest/lockfile gate also
pass. Linux certification for these same suites is recorded in PR #56 run
33641979151 (see 07-UAT-EVIDENCE.md, 2026-09-02 section).
