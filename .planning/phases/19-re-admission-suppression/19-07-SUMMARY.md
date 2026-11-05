---
phase: 19-re-admission-suppression
plan: 07
subsystem: ci-gate
tags: [rust, gate, linux, ci]

requires:
  - phase: 19-re-admission-suppression
    plan: 06
    provides: the finished phase tree with relaunch and breadcrumb proofs
provides:
  - a green scripts/gate.sh on macOS for the Phase 19 tree
  - a green scripts/gate.sh --linux in rust:1-bookworm as uid 1000 on the same tree, through the verbatim lane this time
  - estate-level proof for D-08 and D-15 that the suite touched only redirected roots on both platforms
affects: [20-tui-remove-action, phase-19 PR]

actuals:
  tokens: 0
  tasks: 2
  commits: 6
plan_head_before: 4ec8989ce6285882e10db1b34285cbfaf4a1706d
plan_head_after: 09dd8c4498bee7d2977cdc1a9d1ac674f816f44c

tech-stack:
  added: []
  patterns:
    - "Gate runs redirected to a scratchpad log with the exit status echoed after the command, never piped through tail or grep"

key-files:
  created:
    - .planning/phases/19-re-admission-suppression/19-07-SUMMARY.md
  modified: []

key-decisions:
  - "No phase file was changed: both gates report the tree green at 4ec8989, so the plan lands no fix commit"
  - "Six docs-only phase 20 planning commits landed concurrently under .planning/ while the gates ran; the tree outside .planning is byte-identical between 4ec8989 and 09dd8c4, and the commit-message range check was re-run at 09dd8c4, so the gate verdicts stand for the final HEAD without a second full run"

patterns-established:
  - "The verbatim scripts/gate.sh --linux lane passed on this macOS host; the Phase 18 bind-mount race did not recur, so the manual container variant was not needed"

requirements-completed: [READM-01, READM-02, READM-03, READM-04, READM-05]

duration: 50 min
completed: 2026-11-05
status: complete
---

# Phase 19 Plan 07: Gate on Both Platforms Summary

**Both gates green on the untouched phase tree: scripts/gate.sh exit 0 on macOS and scripts/gate.sh --linux exit 0 in rust:1-bookworm as uid 1000, both against 4ec8989, with no fix commit required and the real worktrees root and config dir untouched on both platforms**

## Performance

- Duration: about 50 min, dominated by the two isolated single-threaded suites running concurrently
- Started: 2026-11-05T05:58:16Z
- Tasks: 2 of 2
- Fix commits: 0

## Gate Record

| Command | Ran against | Exit | Log |
|---|---|---|---|
| `scripts/gate.sh` | 4ec8989ce6285882e10db1b34285cbfaf4a1706d | 0 | scratchpad `gate-macos.log` |
| `scripts/gate.sh --linux` | 4ec8989ce6285882e10db1b34285cbfaf4a1706d | 0 | scratchpad `gate-linux.log` |
| `scripts/check-commit-message.sh --range origin/main..HEAD` | 09dd8c4498bee7d2977cdc1a9d1ac674f816f44c | 0 | foreground |

Each gate ran as `{ git rev-parse HEAD; scripts/gate.sh [--linux]; echo "gate exit $?"; } > log 2>&1`, so the exit status is the script's own, read from the line after its output. Every step ran: fmt, clippy with -D warnings, commit-message self-test and range check, real-roots self-test, real-roots before, isolated `cargo test -- --test-threads=1`, real-roots after. macOS suite: 256 + 560 + 107 tests passed, 1 ignored. The Linux after-check reported all four real roots untouched.

## Task Commits

No task produced a change to the phase tree. `commits: 6` is measured from the ledger but none of the six are this plan's: they are the concurrent phase 20 planning commits (6ecaed1, d934bf0, 5f82217, c9d9a57, 4a178b4, 09dd8c4), all under `.planning/`. `git diff --stat 4ec8989 09dd8c4 -- . ':!.planning'` is empty.

## Files Created/Modified

- `.planning/phases/19-re-admission-suppression/19-07-SUMMARY.md`

`.planning/config.json` was dirty before this plan started and was deliberately left dirty: not staged, committed or reverted.

## Deviations from Plan

None in the code. One process note: the plan asks for both gates on one `git rev-parse HEAD`. Both ran on 4ec8989; HEAD advanced to 09dd8c4 during the runs through another session's docs-only commits. Because the compiled tree is identical and the only commit-reading gate step was re-run on the new HEAD and passed, the gates were not re-run in full.

## Issues Encountered

None. The Phase 18 Docker Desktop bind-mount race on the container target dir did not recur; the verbatim `--linux` lane passed without the manual variant. `target/linux-gate` remains, as after any `--linux` run.

## Must-haves Self-check

- macOS gate exit 0 on the phase's final compiled tree: met
- Linux gate exit 0 as uid 1000 on the same tree: met
- every commit in origin/main..HEAD passes check-commit-message.sh: met at 09dd8c4
- no fixture leaked into the real worktrees root or config dir: met on both platforms, the estate-level proof for D-08 and D-15
- prohibition kept: no gate step judged through a pipe

## TDD Gate Compliance

Not applicable: `type: execute`, no `tdd="true"` task.

## Known Stubs

None.

## Self-Check: PASSED

- 19-07-SUMMARY.md exists
- 4ec8989 and 09dd8c4 are ancestors of HEAD
