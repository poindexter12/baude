# Phase 16 Review Fixes

Applied in `0a181c9`.

## CR-01 — Restart dispatch skipped restore_pane_focus() — FIXED

Confirmed against source before fixing: in `reopen_checkout` the `Focus` and `Spawn` branches both
call `restore_pane_focus()` after setting `selected_id`; `Restart` did not. Pressing `r` on an exited
session therefore left focus on whatever pane the previous selection used, which is issue #89 for the
restart case.

Fix: the `Restart` branch now calls `restore_pane_focus()` too, with a comment naming the finding.

Regression test: `app::tests::restarting_an_exited_session_restores_its_pane`. It drives the Restart
dispatch directly (`reopen_checkout` on a checkout whose runtime has exited — lifecycle.rs maps
`ReopenRuntime::Exited` to `ReopenDispatch::Restart`) rather than through `open_local_target`, which
gates on `LifecycleCapability::RetryReopen` and never reaches the branch. Verified the test is real by
removing the fix and re-running: it fails with `left: Claude, right: Shell`.

## Additional defect found while fixing CR-01 — suspend could lie

Not a review finding. The full suite failed on `baude-core` `suspend_process_becomes_stopped` after the
CR-01 fix; investigation showed it also failed 1 in 5 in isolation, so it was a real race rather than
load.

Instrumenting the signal path showed the child is usually still the gate shell part-way through
`exec`ing the real command when the stop lands, and a second window exists in an interactive shell's
job-control startup, where the process stops and then runs again. `kill` returns 0 in both, so
`Pty::suspend` reported success while the child kept running — the session was recorded as suspended
and kept burning battery, which is precisely what `idle_child_policy = suspend` exists to prevent.

Fix: `suspend` re-signals until the process reads stopped on two consecutive probes, or fails with
`child {pid} would not stay stopped`. Supporting change: `process_is_stopped` (macOS `proc_pidinfo`
status, Linux `/proc/<pid>/stat`) plus a compile-time `STOP_PROBE_SUPPORTED` for platforms with
neither; the macOS `proc_pidinfo` plumbing moved to module scope so the identity reader and the probe
share one read.

A false start worth recording: the first draft treated a `None` probe result as success. `None` means
*unknown right now* — a `proc_pidinfo` read can come back short mid-`exec` — so that draft reproduced
the exact bug it was written to catch, reporting a running child as stopped. `None` now keeps polling.

Measured on `suspend_process_becomes_stopped`: 3 failures in 25 runs before, 0 in 30 after.

## Gates after the fixes

| Gate | Exit |
|------|------|
| `cargo fmt --all -- --check` | 0 |
| `cargo clippy --all-targets -- -D warnings` | 0 |
| `cargo build --workspace` | 0 |
| `cargo test --workspace` | 0 (792 tests) |
