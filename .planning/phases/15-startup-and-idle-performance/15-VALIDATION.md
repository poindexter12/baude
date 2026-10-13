---
phase: "15"
slug: "startup-and-idle-performance"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-09-21"
---

# Phase 15 — Validation Strategy

> Per-phase validation contract and measured local evidence. This document records
> figures for milestone closeout; it does not set the phase status or certify Nyquist
> compliance.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `cargo test` workspace |
| **Config file** | `Cargo.toml` |
| **Quick run command** | `cargo test -p baude --bin baude idle_zero_draws_after_first_frame` |
| **Full suite command** | `cargo test --workspace` |
| **Measured runtime** | Focused idle-draw test: 39.55s compile + 1.65s test on 2026-10-12. The workspace suite did not complete within 600s on this machine, so no full-suite duration is claimed. |

---

## Sampling Rate

- **After every task commit:** Run the affected `cargo test` target.
- **After every plan wave:** Run `cargo test --workspace` once when the environment can complete it.
- **Before `/gsd-verify-work`:** Require a green full suite plus `cargo fmt --all --check` and `cargo clippy --all-targets -- -D warnings`.
- **Performance evidence:** Run `BAUDE_TIMING=1 baude` in a contained PTY and `./scripts/idle-cpu.sh` locally; neither is in the default CI path.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | Evidence status |
|---------|------|------|-------------|-----------|-------------------|-----------------|
| 15-01 | 01 | 1 | PERF-01, PERF-02, PERF-05 | unit + PTY timing | `cargo test -p baude --bin baude idle_zero_draws_after_first_frame` | Focused draw assertion passed on 2026-10-12. |
| 15-02 | 02 | 2 | PERF-02, PERF-03, PERF-04 | unit | `cargo test -p baude --bin baude keyboard_probe_timeout_bounded_250ms` | Existing targeted command; not re-run for this measurement addition. |
| 15-03 | 03 | 2 | PERF-06, PERF-07, PERF-08 | unit | `cargo test -p baude-core --lib mtime_unchanged_skip_read` | Existing targeted command; not re-run for this measurement addition. |
| 15-04 | 04 | 3 | UX-02 | unit | `cargo test -p baude --bin baude ui::tests::status_code` | Existing targeted command; not re-run for this measurement addition. |
| SQ-11 | local evidence | — | idle CPU over real-PTY wall clock | real-PTY process sample | `./scripts/idle-cpu.sh` | Passed at 0.000000 CPU-seconds per idle minute over a 61s window. |

*Evidence status is not phase sign-off.*

---

## Measured Startup and Idle Evidence

All measurements below used a release binary on this macOS machine with a newly
created fixture root. `BAUDE_TEST_FIXTURE_ROOT`, `HOME`, `XDG_CONFIG_HOME`,
`XDG_DATA_HOME`, `XDG_STATE_HOME`, `XDG_CACHE_HOME`, and `CLAUDE_CONFIG_DIR`
were redirected beneath that root, which was removed after each run.

### Startup timing

Command: `BAUDE_TIMING=1 target/release/baude` launched through macOS `script`
in a real PTY, with a contained empty state and `q` supplied after startup.

| Stage | Observed time |
|-------|---------------|
| `config_load` | 0 ms |
| `workspace_resolution` | 697 ms |
| `keyboard_probe` | 250 ms (kitty timeout) |
| `terminal_setup` | 250 ms |
| `app_new` | 0 ms |
| `first_frame` | 5,537 ms |
| `session_restore` | 0 ms (0 sessions) |
| `first_metadata_poll` | 0 ms |
| `total` | 6,859 ms |

### Idle CPU timing

Command: `./scripts/idle-cpu.sh`

The script built the release binary, launched it headlessly through a real
macOS PTY, waited 5s past startup and empty-session restore, then read
cumulative process CPU with `ps -o time=` before and after the idle window.

| Measurement | Observed value |
|-------------|----------------|
| Requested idle window | 60s |
| Wall-clock window | 61s |
| Raw process CPU delta | 0.000000s |
| CPU-seconds per idle minute | 0.000000 |
| Local threshold | 0.25 CPU-seconds per idle minute |
| Threshold headroom | 0.25 CPU-seconds per idle minute |

Limits: this is one machine and one terminal, uses wall-clock sampling and
process CPU time, and does not measure battery draw. `idle_zero_draws_after_first_frame`
separately proves zero in-process draws; this tool supplies the missing real-PTY
CPU observation.

---

## Manual-Only Verifications

| Behavior | Requirement | Why manual | Test instructions |
|----------|-------------|------------|-------------------|
| Real-PTY idle CPU | SQ-11 | CPU time and battery behavior depend on terminal and host scheduling. | Run `./scripts/idle-cpu.sh [--seconds N]`; inspect raw delta, normalized rate, threshold verdict, and stated limits. |
| Startup timing | PERF-01 | Timing is host-dependent and is evidence rather than a fixed unit-test assertion. | Run `BAUDE_TIMING=1 baude` with a contained fixture root and capture the nine stderr stages after exit. |

---

## Validation Sign-Off

- [ ] Full workspace suite completes green in the closeout environment.
- [ ] All phase task commands are re-run or otherwise accepted by closeout.
- [ ] `cargo fmt --all --check` and `cargo clippy --all-targets -- -D warnings` pass.
- [ ] Performance figures are reviewed as host-specific evidence.
- [ ] `nyquist_compliant: true` set in frontmatter by the closeout process.

**Approval:** pending
