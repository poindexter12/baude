---
phase: 22-validation-and-v2-8-0-release
plan: 05
subsystem: release
tags: [release, gate, pull-request, release-please]

requires:
  - phase: 22-validation-and-v2-8-0-release
    provides: "plans 22-01 to 22-04 and 22-06 to 22-08 committed; phases 18 to 20 re-verified at cd3a3d4"
provides:
  - "Both gates green on cd3a3d4, the commit on the remote branch"
  - "One open pull request to main, number 129, titled feat with no scope and no parentheses"
affects: [v2.8.0 release, owner merge, release-please]

tech-stack:
  added: []
  patterns:
    - "A sha:ref refspec in a zsh command needs braces: an unbraced $H followed by :refs is read as the :r modifier"

key-files:
  created:
    - .planning/phases/22-validation-and-v2-8-0-release/22-05-SUMMARY.md
  modified: []

key-decisions:
  - "The Linux gate passed verbatim, so the bind-mount workaround was not used and the via word is verbatim"
  - "The push chain was rerun once with the refspec variables braced after zsh mangled the literal form; nothing had reached the remote and HEAD had not moved, so the gates stayed valid"
  - "Upstream tracking was left unset: git does not record it for a sha source, and setting it by hand was outside the plan"

requirements-completed: []

metrics:
  duration: 19 min
  completed: 2026-11-14

status: complete

actuals:
  tokens: 1976
  tasks: 3
  commits: 0
plan_head_before: cd3a3d4aec9e857c71c59c72d8336a016b5c64a8
plan_head_after: cd3a3d4aec9e857c71c59c72d8336a016b5c64a8
---

# Phase 22 Plan 05: Gate, Push and Pull Request Summary

**Both gates exit 0 on cd3a3d4, that commit is on the remote branch, and pull request 129 to main is open. v2.8.0 is unreleased: release pending the owner's merge.**

## Status of the release

**v2.8.0 is unreleased.** Open item: the owner reviews and merges https://github.com/poindexter12/baude/pull/129, release-please then opens the 2.8.0 release pull request, and that one merges itself after its soak. The owner confirms with `gh release view v2.8.0`. Nothing here merged, approved, enabled auto-merge, tagged or published. REL-01 and REL-02 are not marked complete by this plan; the "released" half of success criterion 3 is the owner's.

## Performance

- **Duration:** 19 min
- **Started:** 2026-11-14T17:44:34Z
- **Completed:** 2026-11-14T18:03Z
- **Tasks:** 3
- **Tracked files modified:** 0 before this summary

## Gate evidence

HEAD at start was `cd3a3d4aec9e857c71c59c72d8336a016b5c64a8`, as expected, and it never moved before the push.

| Gate | Command | Result line | Log |
|------|---------|-------------|-----|
| Host, macOS | `scripts/gate.sh` | `sha=cd3a3d4aec9e857c71c59c72d8336a016b5c64a8 exit=0` in `target/gsd-22/05-mac.result` | `target/gsd-22/05-mac.log` |
| Linux | `scripts/gate.sh --linux` | `sha=cd3a3d4aec9e857c71c59c72d8336a016b5c64a8 exit=0` in `target/gsd-22/05-linux.result` | `target/gsd-22/05-linux.log` |

- Via word in `target/gsd-22/05-linux-via.txt`: `verbatim`. The bind-mount race did not occur, so no rerun and no workaround run happened. Neither log contains `E0463` or `failed to terminate exact PTY process group`.
- Both logs end with `gate: all steps passed`. Suites on both platforms: 360, 572 with 1 ignored, 179, 16, 14, 2, 1 and 1 passed, 0 failed.
- Precondition, counted: `18-`, `19-` and `20-VERIFICATION.md` read `status: passed`, 3 of 3, tracked and committed. Checked before the gates, in the Task 1 verify and again inside the push chain.
- Task 1 verify printed `mac=0 linux=0 linux_passed=1 via=0 clean=0 passed=3` and exited 0.

## Message checks

- `git fetch origin` exit 0. Ahead of `origin/main`: 259. Behind: 0.
- `scripts/check-commit-message.sh --range origin/main..HEAD` exit 0.
- Title file `target/gsd-22/05-title.txt` exit 0. Squash simulation `target/gsd-22/05-squash.txt`, 1291 lines, exit 0.
- Bump check on `target/gsd-22/05-bodies.txt`: 0 breaking or `Release-As` markers, so the bump is the minor one.
- A probe file with nested parentheses made the checker exit 1 through both the file and the stdin form, so the zero exits above are not a checker that ignores its input.
- Identity: `gh api user` printed `poindexter12`; the remote is SSH. No open pull request existed before the push.
- Task 2 verify exited 0.

## Push and pull request

- Push: `git push -u origin` of `cd3a3d4` to `refs/heads/gsd/phase-18-shared-kill-then-remove-lifecycle`, new branch, exit 0, no force flag. `git ls-remote` shows the remote branch at `cd3a3d4aec9e857c71c59c72d8336a016b5c64a8`.
- Upstream tracking: not set. See Deviations.
- Pull request: https://github.com/poindexter12/baude/pull/129, also in `target/gsd-22/05-pr-url.txt`. Title `feat: remove projects from the TUI, the daemon and the PWA`. Base `main`, open, not a draft, auto-merge not requested, no review decision.
- The body passed the checker before creation and again as fetched from GitHub. It names the gated commit, both exit codes and the via word, the fixes of plans 22-06, 22-07 and 22-08, the first runner execution of the node step in CI as known and accepted, the two things never observed, and the release sentence with `release pending`.
- Task 3 verify printed `ls=0 gated=0 list=0 titled=0 body=0 check=0 pending=1 merged=0 gh=0 no_tag=0` and exited 0. No `v2.8.0` tag exists locally or on the remote.
- Local commits not pushed after the push: 0. After this summary commit: 1, which stays local.

## Task Commits

No task changed a tracked file, so no task commit exists. `commits: 0` is measured from the ledger; `plan_head_before` and `plan_head_after` are both `cd3a3d4`. This summary lands in one local docs commit that is not pushed.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] The literal push chain failed in zsh before reaching the remote**
- **Found during:** Task 3
- **Issue:** The harness shell is zsh 5.9. In `"$H:refs/heads/$B"` zsh reads `:r` as a modifier on `$H`, so the refspec became the sha followed by `efs/heads/...` and git answered `src refspec ... does not match any`, chain exit 1. All guards before the push had passed.
- **Fix:** Confirmed with `git ls-remote` that the remote branch did not exist and that HEAD and the tree were unchanged, then ran the same chain once more with the single change `"${H}:refs/heads/${B}"`. Exit 0. One push reached the remote.
- **Files modified:** none
- **Commit:** none

**2. Upstream tracking is not set, although the chain used `-u`**
- git records an upstream only when the pushed source is a branch name, and the plan pushes the sha so the pushed commit is exactly the gated one. `origin/gsd/phase-18-shared-kill-then-remove-lifecycle` exists as a remote-tracking ref at `cd3a3d4`. Left as is; `git branch --set-upstream-to=origin/gsd/phase-18-shared-kill-then-remove-lifecycle` sets it locally if wanted.

**3. The pull request body names plan 22-08**
- The orchestrator asked for it in the "Fixed here" sentence, beyond the plan text.

**4. Bookkeeping left alone on instruction**
- `STATE.md`, `ROADMAP.md` and `REQUIREMENTS.md` were not touched, the ` M .planning/STATE.md` in the tree is as found, and no `WINDOWS.md` entry was appended.

## Issues Encountered

None in the gates. The one issue is the zsh refspec expansion above.

## Open Items

1. The owner's merge of pull request 129 and the release that follows. v2.8.0 is unreleased until then.
2. CI on the pull request was not waited on. It is the first runner execution of the node step.
3. The PWA in a browser and a live event-stream client drop were never observed.
4. Upstream tracking for the branch is unset locally.

## Threat Flags

None. The only surface touched is the push and the pull request in the plan's threat model: T-22-10 checked by login and SSH remote, T-22-11 by the guarded chain and the remote sha comparison, T-22-12 by the verify asserting no merged pull request and no tag.

## Self-Check: PASSED

The summary, both result files, both logs, the via file, the title, squash and URL files all exist. `cd3a3d4` is an ancestor of HEAD and equals the remote branch. The ledger count from `cd3a3d4` to HEAD was 0 when this summary was written.
