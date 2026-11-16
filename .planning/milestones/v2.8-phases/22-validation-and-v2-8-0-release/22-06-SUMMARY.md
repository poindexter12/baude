---
phase: 22-validation-and-v2-8-0-release
plan: 06
subsystem: pty
status: complete
tags: [pty, process-group, macos, eperm, bl-11, gap-closure]
requires:
  - "Phase 18 plan 18-08: group extinction in Pty::kill_and_wait"
provides:
  - "EPERM tolerance in the PTY group signal step"
  - "Pure errno rules for the signal step and the liveness probe"
  - "Injectable TERM, wait, KILL, wait sequence"
  - "BL-11 resolved"
affects:
  - "Phase 18 truth 13, ready for re-verification"
  - "KILL-01, KILL-02, KILL-03"
tech-stack:
  added: []
  patterns:
    - "Hold the child mutex without waiting to keep an exited leader unreaped in a test"
    - "Pure errno rules beside the syscall wrappers, table-tested"
key-files:
  created: []
  modified:
    - baude-core/src/pty.rs
    - baude-core/src/testing.rs
    - .planning/BACKLOG.md
decisions:
  - "The red test holds the child mutex instead of using parked_reader_command, by coordinator decision, because the planned fixture cannot hold the window on macOS"
  - "EPERM from the group signal is tolerated like ESRCH; only the liveness probe decides extinction and it keeps EPERM as alive"
requirements: [REL-01]
metrics:
  duration: "27 min"
  completed: 2026-11-14
  tasks: 3
  files: 3
actuals:
  tokens: 4144
  tasks: 3
  commits: 5
plan_head_before: bfe98b6f329a657bd4b11dc39beaf85828c52025
plan_head_after: 129f78fd390e05bca26ef7e8a08c2269638d18d0
---

# Phase 22 Plan 06: Tolerate EPERM from the PTY group signal Summary

`signal_group` now treats `EPERM` like `ESRCH`, so `Pty::kill_and_wait` reaches extinction for an exited, unreaped leader on macOS, while the liveness probe still counts `EPERM` as alive and remains the only thing that reports a group gone.

Phase 18 truth 13 is ready for re-verification.

## What changed

- **Fix.** `signal_group` returns Ok for `EPERM` as it already did for `ESRCH`. On macOS a group whose only member is an exited, unreaped leader answers `EPERM` to every signal; before the fix that error returned from `extinguish_group` before `wait_for_group_extinction`, the only code that reaps the leader, could run.
- **Pure rules.** `signal_step_proceeds` holds the signal errno rule and `group_probe_alive` holds the probe rule. `signal_group` and `process_group_exists` keep their signatures and behaviour.
- **Injectable sequence.** `extinguish_with` takes the signal step and the extinction check as parameters. `extinguish_group` calls it with `signal_group` and `wait_for_group_extinction`. The three error texts are byte-identical.
- **Docs.** The `group_disposition` doc comment no longer claims an all-zombie group is simply signalled. `await_parked_holder` now warns that it reaps the leader.
- **Backlog.** BL-11 carries the confirmed cause, the owner reversal of 2026-11-13 and a RESOLVED status line.

`group_disposition`, its two existing tests, the reap block in `kill_and_wait`, the child mutex protocol and the reader thread are unchanged.

## Evidence

| Check | Result |
|---|---|
| Red, before the fix, exit 101 | `kill_and_wait failed on an exited, unreaped leader: failed to terminate exact PTY process group: Operation not permitted (os error 1)` |
| Green, after the fix | exit 0, 1 passed, 0.21 s |
| Green repeated 20 times in a row | 20 passed, 0 failed |
| Bite check, probe rule made to report `EPERM` as gone | exit 101: `EPERM is alive: the group has members this process cannot signal` in `group_probe_errno_rule`, and `a group that stays live must not be reported gone` in `extinction_sequence_reports_unsignalable_live_group_as_live`; restored afterwards |
| `cargo test -p baude-core --lib -- --test-threads=1` | exit 0, 566 passed, 0 failed, 1 ignored |
| `cargo fmt --check -p baude-core` | exit 0 |
| `cargo clippy -p baude-core --all-targets -- -D warnings` | exit 0 |
| `scripts/gate.sh --linux` | `linux=0` |
| `scripts/gate.sh` on macOS | `macos=0` |
| `scripts/check-commit-message.sh --range bfe98b6..HEAD` | exit 0 |

Both gates ran on the tree at `d2411a1`, which holds every source change of this plan. The only later commit of this plan before the summary touches `.planning/BACKLOG.md`.

Logs are under `target/gsd-22/`, which is git-ignored: `06-red.log`, `06-green.log`, `06-green-repeat-1.log` to `06-green-repeat-20.log`, `06-bite.log`, `06-task2-test.log`, `06-gate-linux.log`, `06-gate-linux.status`, `06-gate-macos.log`, `06-gate-macos.status`.

## Tests added

All in `baude-core/src/pty.rs`, `mod tests`:

- `kill_and_wait_reaps_unreaped_leader_group`
- `signal_step_errno_rule`
- `group_probe_errno_rule`
- `extinction_sequence_reports_unsignalable_live_group_as_live`
- `extinction_sequence_stops_after_term_when_extinct`
- `extinction_sequence_surfaces_a_signal_error`

The lib suite went from 561 to 567 tests: 566 pass, 1 was already ignored.

## Commits

| Commit | Message |
|---|---|
| `c4cdd47` | test(22-06): reproduce kill_and_wait EPERM on an exited unreaped leader |
| `44c2267` | fix(22-06): tolerate EPERM from the PTY group signal |
| `d2411a1` | test(22-06): pin the EPERM rules of PTY group extinction |
| `129f78f` | docs(22-06): mark BL-11 resolved |

The measured count of 5 between `plan_head_before` and `plan_head_after` includes `564a8ac`, a planner commit that landed in the same tree during this plan and is not part of it.

## TDD Gate Compliance

- **RED:** `c4cdd47`. The test failed with exit 101 on the BL-11 message from `kill_and_wait` itself, after asserting from `ps` that the leader was a zombie and from the group probe that the group answered `EPERM`.
- **GREEN:** `44c2267`. A change confined to `signal_group` turned the same test green.
- **Task 2:** tests and refactor landed together in `d2411a1`, because the tests name functions the refactor introduces. The bite check stands in for a separate red commit.

## Deviations from Plan

### 1. The red fixture holds the child mutex instead of using `parked_reader_command`

- **Found during:** Task 1, first red attempt.
- **Issue:** the planned fixture cannot hold the window on macOS. The first red run exited 101 on the test's own precondition: `leader 32222 never became an unreaped zombie (holder pid recorded: true, ps stat: "")`. A temporary probe that peeked at the leader without reaping showed, in three of three runs, the reader thread finishing as the leader exited, the leader reaped within about 300 microseconds, and the group going from "has members" straight to `ESRCH` without ever answering `EPERM`. The out-of-group holder does not keep the reader parked in `read()` on macOS. The likely mechanism is that macOS revokes the controlling terminal when the session leader exits; that was not confirmed.
- **Resolution:** stopped at the plan's stop rule and returned a checkpoint. The coordinator chose the mutex-held fixture and ruled that the plan's rule bars reaping before the call, which holding the mutex without waiting does not do.
- **What the test does:** it locks a clone of the `pty.child` Arc and never waits on it, so the reader blocks before its `child.wait()` and the zombie stays. It releases the mutex 150 ms into the call.
- **Plainly:** this stretches the real race between the leader's exit and the reader's reap. In production that race is a few hundred microseconds wide. The test does not reproduce it naturally.
- **Stability:** the leader waits on a gate file, so the mutex is held before it can exit regardless of scheduling. The test asserts that `kill_and_wait` returned after the release, which proves it waited for the reap instead of finding the group gone. 20 of 20 runs passed, and it passed inside both full gates.
- **Alternative checked:** the Phase 18 verifier's out-of-tree reproduction spins on the probe and calls on the first `EPERM`. Its own log records 77 failures in 90 window hits and 10 missed windows in 100 rounds, so it is not deterministic and was not used.

### 2. Reading outside the closed list

Three small reads beyond the plan's list: the tail of `group_extinction_after_leader_exit` to find the insertion point, the body of `reader_finished` for the probe, and the verifier's reproduction at the coordinator's request.

### 3. Both gates run

The plan leaves the full macOS gate to plan 22-05. The coordinator asked for both here; both exited 0.

## Open items

- **Existing parked-reader tests on macOS.** The tests built on `await_parked_holder` and `parked_reader_command` may not exercise a parked reader on macOS, for the reason in deviation 1: the reader finishes when the leader exits. Not investigated and not fixed here.
- **Phase 18 truth 13** needs re-verification against this head.

## Known Stubs

None.

## Self-Check: PASSED

- FOUND: `baude-core/src/pty.rs` contains `fn kill_and_wait_reaps_unreaped_leader_group`
- FOUND: `.planning/BACKLOG.md` status line with RESOLVED and plan 22-06
- FOUND: commits `c4cdd47`, `44c2267`, `d2411a1`, `129f78f` in HEAD history
- STATE.md and ROADMAP.md were left alone, as instructed
