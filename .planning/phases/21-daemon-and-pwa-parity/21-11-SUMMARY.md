---
phase: 21-daemon-and-pwa-parity
plan: 11
subsystem: gate
tags: [gate, linux-gate, node-test, human-verify, pwa, tui, removal]
requires:
  - phase: 21-05
    provides: PWA removal controller and its node suite, 40 tests
  - phase: 21-06
    provides: D on a remote row in the TUI
  - phase: 21-07
    provides: removal.js, its node suite of 29 tests, and the check-web-suites gate step
  - phase: 21-08
    provides: remote removal client and the events subscriber
provides:
  - host gate and Linux gate green over the final tree of the phase at 6cb3941
  - node pass count read against the recorded floor
  - owner sign-off on the live checkpoint
affects: []
tech-stack:
  added: []
  patterns: []
key-files:
  created:
    - .planning/phases/21-daemon-and-pwa-parity/21-11-SUMMARY.md
  modified:
    - .planning/REQUIREMENTS.md
    - .planning/ROADMAP.md
    - .planning/STATE.md
key-decisions:
  - "Node pass floor for 21-11 is 69, the sum of 29 from 21-07 and 40 from 21-05, not the plan-time figure that assumed 21-07 shipped 20"
  - "The live checkpoint is recorded as verified on the owner's sign-off alone, with no per-step results, because none were given"
requirements-completed: [API-01, API-02, API-03]
metrics:
  duration: "about 3 h wall on 2026-11-13, of which about 45 min host gate and 4 min Linux gate; the rest was waiting at the checkpoint"
  completed: 2026-11-13
status: complete
actuals:
  tokens: 2021
  tasks: 3
  commits: 0
commits: 0
plan_head_before: 6cb3941b167266141ec3628bf3c06c276a23bfc8
plan_head_after: 6cb3941b167266141ec3628bf3c06c276a23bfc8
---

# Phase 21 Plan 11: Phase gate and live checkpoint Summary

Host and Linux gates green over the final tree at `6cb3941`, node suites at 69 passing against a
floor of 69, and the live PWA and TUI checkpoint closed on the owner's sign-off with no per-step
results.

This plan changed no source file. `commits: 0` is the count before the documentation commit that
carries this file; the plan prescribes no other commit.

## Gated commit

`6cb3941b167266141ec3628bf3c06c276a23bfc8` on `gsd/phase-18-shared-kill-then-remove-lifecycle`,
for both gate runs. `git status --porcelain -- baude baude-core bauded scripts Cargo.toml Cargo.lock`
printed nothing before Task 1, after Task 1, after Task 2 and after teardown.

## Gate statuses

Each status was assigned on its own statement directly after its command. No verdict was read
through a pipe.

| Variable | Command | Status |
| -------- | ------- | ------ |
| `GATE` | `scripts/gate.sh` | 0 |
| `WEB` | `check-web-suites: passed` present in the host gate log | 0 |
| `NODE` | `node --test --test-reporter=tap` over both suites | 0 |
| `CHECK` | `node --check bauded/web/app.js` | 0 |
| `LINUX` | `scripts/gate.sh --linux` | 0 |

Both gate logs contain `gate: all steps passed` and `check-web-suites: passed`, and neither has a
line starting `FAIL (`. Logs: `/tmp/baude-21-11-gate.log`, `/tmp/baude-21-11-linux.log`,
`/tmp/baude-21-11-node.log`.

## Counts

| Suite | Host | Linux |
| ----- | ---- | ----- |
| `baude` | 345 passed, 0 failed, 1683.72 s | 345 passed, 0 failed, 80.47 s |
| `baude-core` | 560 passed, 0 failed, 1 ignored, 630.85 s | 560 passed, 0 failed, 1 ignored, 50.82 s |
| `bauded` | 176 passed, 0 failed, 340.78 s | 176 passed, 0 failed, 42.22 s |
| node, both PWA suites | `# pass 69`, `# fail 0`, node v26.12.1 | `pass 69`, node v26.10.0 |

Node pass floor: **69**, written to `/tmp/baude-21-11-pass-floor` as the plan directs. It is the
final count recorded in `21-07-SUMMARY.md`, 29, plus the final count recorded in
`21-05-SUMMARY.md`, 40. The plan-time text assumed 21-07 shipped 20 tests; 21-05 had already
recomputed the whole-directory floor to 68. Measured 69, so the floor is met exactly.

The host suite ran roughly twenty times slower than the same suite in the Linux container. That
is recorded as measured, not investigated.

## Flake

The known macOS flake, `DELETE /sessions/{id}` answering 500 with
`failed to terminate exact PTY process group`, did not appear in either gate run. No gate was
re-run.

## Live checkpoint, Task 3

### Preparation

`cargo build -p bauded -p baude` exited 0. The built `target/debug/bauded` ran on
`127.0.0.1:51723` only, under `HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and `CLAUDE_CONFIG_DIR`
pointed at one `mktemp -d` root, with an argument-tolerant shell script as the session command.
The fixture was created through `POST /sessions`: R1 as a real `git clone` with `r1-main`,
`r1-wt-a` and `r1-wt-b`; R2 with `r2-main` and `r2-wt`; standalone `folder-a` and `folder-b`.
`GET /rows` and `GET /sessions` showed all seven rows with a live session before hand-over.

### Outcome

**Verified by the owner's sign-off.** The executor observed none of the steps. Both answers
reached the executor through the orchestrator and are quoted as relayed:

1. "approved"
2. "mark them as verified and move on bruh"

No per-step result or observation was given for any of steps 1 to 17, so none is recorded here.

At sign-off the fixture daemon's `GET /rows` still listed R1 with three checkouts, R2 with two,
and `folder-b`.

Step 18, an installed PWA picks up the `baude-v7` service worker cache and loads both new
scripts, was added by the orchestrator and is covered by the same sign-off.

### Teardown

The root was confirmed to be the `mktemp` directory created for this fixture before anything was
deleted. Pid 17935 was confirmed to be that `bauded` and was stopped; no fixture agent process
remained afterwards. The root and its pointer file under `/tmp` were deleted, the root no longer
exists, and nothing listens on port 51723.

## Deviations from Plan

### 1. The tracer verify was run once, not twice

Task 1 is a `tracer` task whose action and verify are the same command. It was executed once,
end to end, with exit 0. It was not run a second time as a separate feedback gate, because a
second 45 minute host run would add no evidence.

### 2. Gates were waited on through a status file

The harness moves a foreground command to the background after about three minutes. Each gate
was therefore started as `scripts/gate.sh ... > log 2>&1; STATUS=$?; echo "$STATUS" > file`, so
the status was still assigned on its own statement directly after the command, and then read
back from the file.

### 3. No per-name grep was substituted

The plan's verify commands for 21-11 contain no per-name `::tests::NAME ... ok` grep, so the
defect recorded in `21-06-SUMMARY.md` did not apply and nothing was substituted.

### 4. The checkpoint closed without per-step answers

The plan's acceptance criteria ask for each of the 17 steps to be listed as passed, failed or
skipped. The owner signed off the checkpoint as a whole instead. The steps are therefore not
listed individually, and no step is recorded as individually passed.

## Open items, unowned

From the plan's success criteria:

- The API-02 edge that the probe reports as `unclassified` stays `unresolved`. 21-04 carries it.
- Whether unshortened PWA paths are acceptable. `/info` serves no home, so paths print in full.

Handed over at close:

- The intermittent macOS `DELETE /sessions/{id}` 500 teardown flake, suspected `EPERM` in
  `signal_group` in `baude-core/src/pty.rs`. About one in five full single-threaded runs.
- Standalone removal leaves the daemon-written `.claude` directory behind.
- Unclaimed sessions are listed without the "other sessions" heading.
- State files from earlier daemons still serve `managed_branch` for a main checkout.
- The 15 s keep-alive is not witnessed by any test.
- The CI `check` job with the node steps is unproven until the first push.
- Plan verify greps in 21-06 miss test names on a green run.

## Requirements

API-01, API-02 and API-03 are marked complete on the two green gates and the owner's sign-off.

## Known Stubs

None. This plan created no source.

## Self-Check: PASSED

- FOUND: `.planning/phases/21-daemon-and-pwa-parity/21-11-SUMMARY.md`
- FOUND: `6cb3941` as HEAD when both gates ran and when this file was written
- No task commit to check: the plan prescribes none
