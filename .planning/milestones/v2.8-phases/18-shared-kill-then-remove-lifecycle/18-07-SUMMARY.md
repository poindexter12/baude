---
phase: 18-shared-kill-then-remove-lifecycle
plan: 07
subsystem: testing
tags: [rust, gate, linux, ci, docker]

# Dependency graph
requires:
  - phase: 18-shared-kill-then-remove-lifecycle
    provides: the complete phase tree at f1351d2 from plans 18-01 through 18-06 and 18-08, including Pty::kill_and_wait, Session::kill_and_wait, removal::remove and the removal::tests, pty::tests and session::tests live-runtime cases
provides:
  - scripts/gate.sh green on macOS at f1351d2 with every step's exit code read directly
  - the same gate script green inside rust:1-bookworm as uid 1000 at f1351d2, proving the kill, reap and procfs identity paths on Linux, with the four required live-runtime tests named in the log
  - a settled diagnosis of why the verbatim scripts/gate.sh --linux lane exits 1 on this macOS host, recorded in deferred-items.md and WINDOWS.md with the one-line fix it needs
affects: [19-tui-remove-action, 20-daemon-and-pwa-parity, scripts/gate.sh maintenance]

# Actuals (#2632) - estimateTokens scale, chars/4 over the realized diff
actuals:
  tokens: 3100
  tasks: 2
  commits: 0
plan_head_before: f1351d24faf74cec0ad4d357ff196b8bbc87b823
plan_head_after: f1351d24faf74cec0ad4d357ff196b8bbc87b823

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "A gate run is captured by redirecting the whole script to a log and appending its own exit status on the next line; the status is read from that line, never from a pipe"
    - "A gate failure inside the Linux container is bisected by re-running the failing step sequence in fresh target dirs in the same image and uid, varying one factor per run"

key-files:
  created:
    - .planning/phases/18-shared-kill-then-remove-lifecycle/deferred-items.md
  modified: []

key-decisions:
  - "No phase file was changed: both gates report the phase tree green at f1351d2, so the plan lands no fix commit and the SUMMARY is the only artifact"
  - "The verbatim scripts/gate.sh --linux exit 1 is an environmental defect of the gate's bind-mounted target dir on Docker Desktop, not a Linux defect in the phase code; gate.sh is outside this plan's allowlist and the plan directs container failures to the orchestrator, so the fix is recorded in deferred-items.md and WINDOWS.md rather than applied"
  - "The Linux proof was taken by running the unmodified gate script inside the same image as uid 1000 with only CARGO_TARGET_DIR moved to container-local disk, so every gate step still ran and the one delta is the directory the race lives in"

patterns-established:
  - "Linux gate on a macOS host: until gate.sh keeps the container target dir off the bind mount, run the container the way the --linux branch does with CARGO_TARGET_DIR=/tmp/linux-gate/target and read exit= from the redirected log"

requirements-completed: [KILL-01, KILL-02, KILL-03]

coverage:
  - id: D1
    description: "scripts/gate.sh exits 0 on macOS at f1351d2: fmt, clippy -D warnings, commit-message self-test and origin/main..HEAD range, real-roots self-test, isolated single-threaded suite, real-roots after check"
    requirement: KILL-01
    verification:
      - kind: integration
        ref: "scripts/gate.sh, exit=0 line of macos-gate.log, 00:47:54Z to 01:03:25Z"
        status: pass
    human_judgment: false
  - id: D2
    description: "The same gate script exits 0 inside rust:1-bookworm as uid 1000 at f1351d2, with removal::tests::live_runtime_group_is_extinct_after_remove, removal::tests::surviving_runtime_leaves_row_and_disk, pty::tests::group_extinction_after_leader_exit and session::tests::group_extinction_error_path_does_not_take_child_lock all ok on Linux procfs"
    requirement: KILL-03
    verification:
      - kind: integration
        ref: "bash scripts/gate.sh inside baude-gate with CARGO_TARGET_DIR=/tmp/linux-gate/target, exit=0 line of linux-evidence.log, 01:13:58Z to 01:16:01Z"
        status: pass
    human_judgment: false
  - id: D3
    description: "The verbatim scripts/gate.sh --linux invocation exits 0 on this host"
    requirement: KILL-03
    verification:
      - kind: integration
        ref: "scripts/gate.sh --linux, exit=1 line of linux-gate.log and linux-gate-retry.log"
        status: fail
    human_judgment: true
    rationale: "Fails in the isolated cargo test step with E0463 for already-built crates, a Docker Desktop bind-mount visibility race, reproduced in isolation and absent with the target dir on container disk; the phase code is proven green on Linux by D2, but the gate script needs a one-line change outside this plan's allowlist before the literal command passes here. A human decides whether to land that change in scripts/gate.sh before ship"

# Metrics
duration: 33 min
completed: 2026-11-04
status: complete
---

# Phase 18 Plan 07: Gate on Both Platforms Summary

**Both gates green on the untouched phase tree at f1351d2: macOS exit 0 through scripts/gate.sh, and the same script exit 0 inside rust:1-bookworm as uid 1000 once its target dir is off the Docker Desktop bind mount, with the verbatim --linux lane's exit 1 traced to that mount and recorded for a gate.sh fix**

## Performance

- **Duration:** 33 min
- **Started:** 2026-11-04T00:46:29Z
- **Completed:** 2026-11-04T01:19:40Z
- **Tasks:** 2
- **Files modified:** 0 in the phase tree; 1 planning note created

## Accomplishments
- `scripts/gate.sh` on macOS: `exit=0` on `f1351d2`, all steps green. Suites: 249 + 525 + 107 + 16 + 14 + 2 + 1 + 1 passed, 0 failed, 1 ignored outside the phase files; the 249-test baude-core suite took 552 s single-threaded because its live-runtime fixtures wait on real signals. No clippy warning, no commit-message failure across the 45 commits in `origin/main..HEAD`, both real-roots checks pass.
- The unmodified gate script inside `rust:1-bookworm` as uid 1000: `exit=0` on the same `f1351d2`, same suite counts, every step from fmt through the after-check, in 2 min 3 s. The log names all four required Linux proofs: `removal::tests::live_runtime_group_is_extinct_after_remove ... ok`, `removal::tests::surviving_runtime_leaves_row_and_disk ... ok`, `pty::tests::group_extinction_after_leader_exit ... ok`, `session::tests::group_extinction_error_path_does_not_take_child_lock ... ok`.
- No `#[ignore]` in `removal.rs`, `pty.rs` or `session.rs` (0, 0, 0); `git rev-list --count` of `(test|feat|fix|refactor)(18-0[1-8]):` subjects is 19, above the required 14; `git status --porcelain -- baude-core/` is empty.
- The verbatim `scripts/gate.sh --linux` lane was diagnosed instead of retried blindly. Details under Issues Encountered and in `deferred-items.md`.

## Task Commits

No task produced a change to the phase tree, so no task commit exists. `plan_head_before` and `plan_head_after` are both `f1351d2`, and `commits: 0` is measured from the ledger. The SUMMARY, `deferred-items.md`, STATE.md, ROADMAP.md and REQUIREMENTS.md land in the plan's docs commit.

## Files Created/Modified
- `.planning/phases/18-shared-kill-then-remove-lifecycle/deferred-items.md` - the Linux gate lane defect, its bisection and the suggested one-line `scripts/gate.sh` fix

## Decisions Made
- Both gate results describe the same commit because no fix was needed, so the plan's "re-run macOS after the last Linux fix" rule had nothing to trigger; the macOS result from Task 1 stands as the final one.
- `scripts/gate.sh` was not edited. The plan names it an unchanged artifact, limits fixes to the six phase files, and sends container failures to the orchestrator; the suggested fix is one line in the `docker run` invocation and is written up for whoever owns that change.

## Deviations from Plan

None in the code: no fix commit was required and no phase file was touched. One process deviation: Task 2's acceptance criterion "`scripts/gate.sh --linux` exits 0" is not met by the literal command on this host, and the Linux proof was taken through the unmodified gate script inside the same container with the target dir relocated. See Issues Encountered for why that is the gate's defect and not the phase's.

**Total deviations:** 0 auto-fixed. **Impact on plan:** the phase's SC1 claim, kill works on macOS and Linux, is proven on both platforms by the gate script; the only thing not proven is that the host's Docker bind mount can serve as a cargo target dir.

## Issues Encountered

**`scripts/gate.sh --linux` exits 1 in the isolated `cargo test` step with `error[E0463]: can't find crate for ...` while compiling a dependency.** This is the failure 18-08 reported and attributed to stale cargo state. It is not stale state: `target/linux-gate` was cleared before the first run here and the step failed the same way at 01:03:47Z to 01:05:47Z, naming `http`, `sha1` and `thiserror` inside `tungstenite 0.29.0`; a verbatim retry on the resulting cache failed again at 01:16:40Z to 01:17:31Z, naming `baude_core` and `ratatui`. Every named crate had compiled earlier in the same build and its rlib and rmeta were on disk.

Bisection in the same `baude-gate` image as uid 1000, fresh target dir each time:

| Run | Target dir | Sequence | Result |
|-----|-----------|----------|--------|
| A | bind mount | `cargo test --no-run` | pass |
| B | bind mount | `cargo clippy --all-targets` then `cargo test --no-run` | fail, `E0463` for `hmac` |
| C | bind mount, `CARGO_BUILD_PIPELINED=false` | same as B | fail, `E0463` for `nix`, `serial`, `filedescriptor` |
| D | container-local `/tmp` | same as B | pass |
| evidence | container-local `/tmp`, everything else as the `--linux` branch runs it | `bash scripts/gate.sh` | `exit=0`, all suites green |

Check-mode and build-mode artifacts for the same crate carry different hashes (`libhmac-ab9d…rmeta` from clippy next to `libhmac-e0e8…rlib` and `.rmeta` from the test build), so nothing is overwritten; rustc cannot see a file cargo has just written in a `deps/` directory clippy already doubled in size. That is a file-visibility race in Docker Desktop's bind mount, and the `--linux` lane is the only place this project compiles through one; CI's `check (ubuntu)` job builds on the runner's own disk, which is why this has never shown up there.

Recorded in `deferred-items.md` and as an `unmet-truth` entry in `.planning/WINDOWS.md` against `scripts/gate.sh`. The remedy is to keep the container's `CARGO_TARGET_DIR` off the bind mount, for example a named volume mounted at `/work/target/linux-gate/target`, with `CARGO_HOME` left on the mount so the registry cache persists. Until then, the Linux gate on a macOS host is the evidence command above.

Other notes:
- `git fetch origin main` failed on this session's SSH lane, so the `origin/main..HEAD` range used by the commit-message check is the local tracking ref. The range covers the 45 branch commits and the check passed in both environments.
- The three experiment target dirs under `target/` were removed after the bisection; `target/linux-gate` from the failed verbatim runs remains, as it would after any `--linux` run.

## TDD Gate Compliance

Not applicable: the plan is `type: execute` with no `tdd="true"` task and no RED or GREEN commit. `gsd_run check tdd-red-evidence` has no cargo adapter in any case.

## Known Stubs
None.

## Threat Flags
None beyond the plan's threat model. T-18-16 is mitigated by the Linux evidence run as a non-root user; T-18-17 by both real-roots brackets passing on both platforms; T-18-29 by `check-commit-message.sh --range origin/main..HEAD` passing with no commit amended; T-18-SC by no crate or image change, the container is the gate's own `baude-gate` build of `rust:1-bookworm`.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Phase 18 is complete: eight plans summarised, KILL-01, KILL-02 and KILL-03 all ready and marked, the phase tree green on both platforms at `f1351d2`.
- Before the phase ships, someone with authority over `scripts/gate.sh` should land the target-dir relocation so `scripts/gate.sh --linux` passes verbatim on a macOS host; the WINDOWS.md entry blocks `/gsd-ship` until it is resolved.

---
*Phase: 18-shared-kill-then-remove-lifecycle*
*Completed: 2026-11-04*
