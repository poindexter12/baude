---
phase: "8"
slug: "test-isolation-and-fixture-ownership"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: true
wave_0_complete: true
created: "2026-09-13"
---

# Phase 8 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `libtest` (`#[test]` / `#[should_panic]`), cargo workspace, edition 2021 |
| **Config file** | none — `Cargo.toml` workspace at repo root; no external test runner |
| **Quick run command** | `cargo test -p <package> <target-filter> <name-filter>` — see the target-filter table below |
| **Full suite command** | `cargo test -- --test-threads=1` (CI parity, `.github/workflows/ci.yml`) |
| **Estimated runtime** | quick filter ~5-20s warm; full serial suite ~4-6 min (spawns real PTYs and a re-exec'd dogfood child) |

### Target filters — binary-only crates do not accept `--lib`

`baude` and `bauded` have no `src/lib.rs` and no `[lib]` section. `cargo test -p baude --lib`
fails with *"no library targets found in package `baude`"* — a dead gate. Verified in this
environment: `--lib` errors; `--bins` resolves `Executable unittests src/main.rs`.

| Package | Correct form | Why |
|---------|--------------|-----|
| `baude` | `cargo test -p baude --bins [filter]` | binary-only |
| `bauded` | `cargo test -p bauded --bins [filter]` | binary-only |
| `baude-core` | `cargo test -p baude-core --lib [filter]` | real library crate |

### Gate hygiene — never read an exit status through a pipe

`cmd 2>&1 | tail -5` exits with `tail`'s status, so a failed build passes the gate. No
`<automated>` command in this phase pipes a gate into another command. Where two independent
results must both be reported (plan 06 task 2), each status is captured into its own variable
on its own line and tested afterwards.

---

## Sampling Rate

- **After every task commit:** the task's own `<automated>` command (all are filtered; each
  returns in well under 60s warm)
- **After every plan wave:** `cargo test -- --test-threads=1` plus
  `cargo clippy --all-targets -- -D warnings`
- **After wave 3 and wave 4:** additionally
  `bash scripts/assert-real-roots-untouched.sh before` / `after` around the full suite
- **Before `/gsd-verify-work`:** full suite green, clippy clean, `cargo fmt --check` clean,
  `cargo build --workspace --release --locked` succeeds
- **Max feedback latency:** 20 seconds for the per-task filtered commands

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 8-01-01 | 01 | 1 | TISO-01, TISO-03 | T-08-01 / T-08-02 / T-08-03 | Feature gate reaches a downstream test binary; release build selects no test-support code | tracer (end-to-end) | `cargo test -p baude --bins test_support_gate_is_active && cargo test -p baude-core --lib persist:: && cargo build --release -p baude -p bauded` | ✅ existing | ⬜ pending |
| 8-01-02 | 01 | 1 | TISO-01, TISO-03 | T-08-01 / T-08-09 | Unguarded real-path resolution aborts; arming flag removed so there is no pre-arm window | unit | `cargo test -p baude-core --lib testing:: && cargo test -p baude-core --lib git::tests && cargo test -p baude --bins && cargo test -p bauded --bins && cargo clippy --all-targets -- -D warnings` | ✅ existing | ⬜ pending |
| 8-01-03 | 01 | 1 | TISO-03 | T-08-02 / T-08-23 | Escape aborts in all three test binaries; probe is thread-local so it disturbs no concurrent test | unit + integration | `cargo test -p baude --bins && cargo test -p bauded --bins && cargo test -p baude-core --lib && cargo build --workspace --release --locked` | ✅ existing | ⬜ pending |
| 8-02-01 | 02 | 2 | TISO-01, TISO-03 | T-08-04 | `~/.claude` unreachable from a guarded test binary; `CLAUDE_CONFIG_DIR` chain preserved verbatim | unit | `cargo test -p baude-core --lib meta:: && cargo test -p baude --bins local_tui_dogfood && cargo clippy --all-targets -- -D warnings` | ✅ existing | ⬜ pending |
| 8-02-02 | 02 | 2 | TISO-01 | T-08-01 / T-08-08 / T-08-10 | VAPID key and subscription store land inside the fixture root; key format unchanged | unit (new coverage) | `cargo test -p bauded --bins push:: && cargo test -p bauded --bins && cargo clippy --all-targets -- -D warnings` | ❌ W0 — `push::tests` module is created by this task | ⬜ pending |
| 8-03-01 | 03 | 2 | TISO-02, TISO-03 | T-08-05 / T-08-11 | Concurrent fixtures resolve independent identities; managed path composition unchanged | unit (concurrency) | `cargo test -p baude-core --lib workspace:: && cargo test -p baude-core --lib git::tests && cargo clippy --all-targets -- -D warnings` | ✅ existing | ⬜ pending |
| 8-03-02 | 03 | 2 | TISO-02 | T-08-06 / T-08-24 | Identity path performs no config read; bootstrap arm containment-guarded and unreachable in production | unit + counter assertion | `cargo test -p baude-core --lib workspace:: && cargo test -p baude --bins && cargo test -p bauded --bins && cargo clippy --all-targets -- -D warnings` | ✅ existing | ⬜ pending |
| 8-04-01 | 04 | 2 | TISO-04 | T-08-12 | Removal predicate agreed before any code can produce `Removable` | checkpoint:decision | N/A — blocking decision checkpoint | N/A | ⬜ pending |
| 8-04-02 | 04 | 2 | TISO-04 | T-08-12 / T-08-15 | Shape alone and missing-gitdir alone both yield `Indeterminate`; empty evidence yields `Indeterminate` | unit (pure) | `cargo test -p baude-core --lib worktree_scan::tests::verdict && cargo clippy --all-targets -- -D warnings` | ❌ W0 — `worktree_scan.rs` is created by this task | ⬜ pending |
| 8-04-03 | 04 | 2 | TISO-04 | T-08-04 / T-08-13 / T-08-14 | Read-only enumeration; symlink refused; key overflow skipped not fatal; tree byte-identical after scan | unit (filesystem fixture) | `cargo test -p baude-core --lib worktree_scan:: && cargo clippy --all-targets -- -D warnings && cargo test -p baude-core --lib git::tests` | ❌ W0 — created by 8-04-02 | ⬜ pending |
| 8-05-01 | 05 | 3 | TISO-04 | T-08-16 | Unreadable state blocks every candidate in its workspace; absent state is a checked absence | unit | `cargo test -p baude-core --lib worktree_scan:: && cargo clippy --all-targets -- -D warnings` | ✅ after 8-04-02 | ⬜ pending |
| 8-05-02 | 05 | 3 | TISO-04 | T-08-05 / T-08-04 / T-08-18 | Prune re-derives all evidence and requires the proof to match; no confirmation means no removal | checkpoint:decision then unit | `cargo test -p baude-core --lib worktree_scan:: && cargo clippy --all-targets -- -D warnings` | ✅ after 8-04-02 | ⬜ pending |
| 8-06-01 | 06 | 3 | TISO-01, TISO-03 | T-08-09 / T-08-21 | Fixture helper holds the redirect guard as a field for the fixture lifetime | unit (refactor regression) | `cargo test -p bauded --bins manager:: && cargo test -p bauded --bins && cargo clippy --all-targets -- -D warnings` | ✅ existing | ⬜ pending |
| 8-06-02 | 06 | 3 | TISO-01, TISO-02, TISO-03 | T-08-20 / T-08-22 | A full suite run changes none of the three real roots; failure names the root and the entries | suite-level script | `bash scripts/assert-real-roots-untouched.sh before && cargo test -- --test-threads=1; status=$?; bash scripts/assert-real-roots-untouched.sh after; after=$?; test "$status" -eq 0 -a "$after" -eq 0` | ❌ W0 — script created by this task | ⬜ pending |
| 8-07-01 | 07 | 4 | TISO-04 | T-08-17 / T-08-18 / T-08-19 | `scan` is read-only; removal needs two distinct flags; no verb added to `bauded` | unit + manual | `cargo test -p baude --bins && cargo build -p baude && cargo clippy --all-targets -- -D warnings` | ✅ existing | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

No separate Wave 0 pass is needed: every `❌ W0` row above names a file the task itself
creates as its first action, and no task depends on a test file another task was supposed to
scaffold. The framework is cargo's built-in `libtest`, already in use across all three crates.

- `baude-core/src/worktree_scan.rs` — created by task 8-04-02 before its own tests run; 8-04-03 and both plan 05 tasks extend the same module.
- `bauded/src/push.rs` test module — created by task 8-02-02; the push subsystem has no coverage today.
- `scripts/assert-real-roots-untouched.sh` — created by task 8-06-02 together with its CI wiring.
- `baude-core/src/testing.rs` — created by task 8-01-01; every later plan consumes it, which is why plan 01 is the wave-1 tracer.

Sampling continuity check: no three consecutive tasks lack an `<automated>` command. The only
task without one is the plan 04 decision checkpoint (8-04-01), which is bracketed by
automated tasks on both sides.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| `baude worktrees scan` against the developer's real 1433-entry tree reports a grouped summary, removes nothing, and marks no directory holding a real checkout as removable | TISO-04 | The dataset is this machine's real `~/.local/share/baude/worktrees`. It cannot be committed as a fixture, and the two live `claude` directories that must not be cleared exist only here. The automated equivalent runs against a synthetic fixture tree. | Run `cargo run -p baude -- worktrees scan`. Confirm the summary prints; confirm `claude/repository-2` and `claude/repository-14` are not reported removable; confirm `claude/repository-1` and `claude/repository-5` are blocked as referenced by state. Record the observed counts per verdict in `08-07-SUMMARY.md`. |
| A full local `cargo test -- --test-threads=1` leaves the three real roots unchanged | TISO-01, TISO-02, TISO-03 | CI runners have no populated real roots, so the CI green signal is weaker than the local one. The measurement that decides whether the phase goal is met must be taken on a machine that has them. | Run the `before` / suite / `after` triple from task 8-06-02 locally and record in `08-06-SUMMARY.md` whether any root changed. A changed root is a gap-closure finding, not something to work around by loosening the script. |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies — the sole exception is the 8-04-01 decision checkpoint
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references — each is created by the task that first needs it
- [x] No watch-mode flags
- [x] Feedback latency < 20s for per-task commands
- [x] No `--lib` filter on a binary-only crate; no gate read through a pipe
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
