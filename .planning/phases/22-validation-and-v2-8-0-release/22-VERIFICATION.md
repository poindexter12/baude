---
phase: 22-validation-and-v2-8-0-release
verified: 2026-11-15T05:10:00Z
status: human_needed
score: 4/5 must-haves verified
covered_files:
  - ".github/workflows/ci.yml"
  - ".github/workflows/release.yml"
  - ".planning/BACKLOG.md"
  - ".planning/phases/22-validation-and-v2-8-0-release/22-01-PLAN.md"
  - ".planning/phases/22-validation-and-v2-8-0-release/22-01-SUMMARY.md"
  - ".planning/phases/22-validation-and-v2-8-0-release/22-02-PLAN.md"
  - ".planning/phases/22-validation-and-v2-8-0-release/22-02-SUMMARY.md"
  - ".planning/phases/22-validation-and-v2-8-0-release/22-03-PLAN.md"
  - ".planning/phases/22-validation-and-v2-8-0-release/22-03-SUMMARY.md"
  - ".planning/phases/22-validation-and-v2-8-0-release/22-04-PLAN.md"
  - ".planning/phases/22-validation-and-v2-8-0-release/22-04-SUMMARY.md"
  - ".planning/phases/22-validation-and-v2-8-0-release/22-05-PLAN.md"
  - ".planning/phases/22-validation-and-v2-8-0-release/22-05-SUMMARY.md"
  - ".planning/phases/22-validation-and-v2-8-0-release/22-06-PLAN.md"
  - ".planning/phases/22-validation-and-v2-8-0-release/22-06-SUMMARY.md"
  - ".planning/phases/22-validation-and-v2-8-0-release/22-07-PLAN.md"
  - ".planning/phases/22-validation-and-v2-8-0-release/22-07-SUMMARY.md"
  - ".planning/phases/22-validation-and-v2-8-0-release/22-08-PLAN.md"
  - ".planning/phases/22-validation-and-v2-8-0-release/22-08-SUMMARY.md"
  - "README.md"
  - "baude-core/src/git.rs"
  - "baude-core/src/pty.rs"
  - "baude-core/src/removal.rs"
  - "baude-core/src/suppression.rs"
  - "baude-core/src/testing.rs"
  - "baude/src/app.rs"
  - "bauded/src/api.rs"
covered_digest: "v4:sha256:e1f4358623fafbf43a3b52fd56ec8fd09e9609f7c7910918e47546b5241bd31d"
behavior_unverified: 0
overrides_applied: 0
human_verification:
  - test: "Owner merges PR 129 (https://github.com/poindexter12/baude/pull/129) to main; release-please then opens the 2.8.0 release PR, which merges after its soak. Confirm with `gh release view v2.8.0`."
    expected: "A published v2.8.0 GitHub Release and tag cut by release-please, with no hand-edited changelog or version and no Release-As footer."
    why_human: "By owner decision no plan merges to protected main, tags or publishes. At verification time PR 129 is OPEN, mergedAt null, head 9c257b3, and no v2.8* tag exists."
---

# Phase 22: Validation and v2.8.0 Release Verification Report

**Phase Goal:** Removal is proven end to end on realistic fixtures for every row kind with and without a live runtime, documented, and shipped as v2.8.0 through release-please.
**Verified:** 2026-11-15T05:10:00Z, against HEAD `9c257b3cd1b1e346d2dbf056ef2d209e2abb8c1b` on branch `gsd/phase-18-shared-kill-then-remove-lifecycle`. The working tree was clean apart from `.planning/STATE.md`.
**Status:** human_needed
**Re-verification:** No, this is the initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | SC1 / REL-01: integration tests cover each row kind with and without a live runtime, on cloned and `git init` fixtures, and assert on fixture output that process, worktree, binding and marker state are gone | VERIFIED | `baude-core/src/removal.rs:5388-5455` has four `end_state_matrix_` tests with 16 cells. Repository parent and main checkout run 2 shapes x 2 runtimes; managed worktree runs ManagedBranch on cloned and git init plus PrimaryDefault on cloned, x 2 runtimes; standalone runs x 2 runtimes. Cloned cells use `cloned_topology` (`testing.rs:356`), a real `git clone` of an upstream. `read_end_state` (`removal.rs:4711`) reads `ps` state, group liveness, dir listings, `git worktree list --porcelain`, branch snapshot, bindings file, marker file, suppression store and saved state from disk. Every `assert_gone` also requires presence before the removal (`removal.rs:4780`), and `RemovalReport` is never used as an observation. The daemon side is covered by `api.rs:7038` (real TCP `axum::serve`: `/rows`, `/removals/preview`, `/removals`, an `/events` `rows_removed` frame, then a fresh `Manager::restore_at`) and `api.rs:7225` (cloned parent removal read from disk). The TUI side is covered by `app.rs:15889` and `app.rs:16012`. All of these passed in this run. |
| 2 | SC2 / REL-02 docs: README and PWA docs describe the remove action, its confirmation, and what is and is not deleted | VERIFIED | `README.md:397` `## Removing projects` covers the `D` key, the confirmation, the deletes and keeps table, what baude forgets, and what happens after the default checkout is removed. The `D` row in the Keys table is at `README.md:153`. `D` on remote rows is in the Remote sessions section. bauded endpoints `GET /rows`, `POST /removals/preview`, `POST /removals` and `GET /events` with `rows_removed` are at `README.md:970-1005`. The PWA confirmation is at `README.md:1007-1045`. Spot-checked against code: the forced remove argv is at `git.rs:2947-2955`; the TUI title, footer and retry footer are at `ui.rs:2753,2847-2849`; the PWA title template is at `removal.js:223`; `already removed` and `removed elsewhere` are at `removal-controller.js:246,255`; the two default-checkout messages are at `app.rs:2642-2646`. No claim of a browser run was found. |
| 3 | SC3a: `scripts/gate.sh` and `scripts/gate.sh --linux` pass | VERIFIED | `target/gsd-22/05-mac.result` and `05-linux.result` both read `sha=cd3a3d4… exit=0`. `ci-fix-gates.txt` reads `mac=0 linux=0 head=0148168…`. For `9c257b3`, `ci-runner-mac.log` (20:32, after the 20:19 commit) ends `gate: all steps passed`, but it has no sha-stamped exit file. The `0148168..9c257b3` diff touches only `.github/workflows/ci.yml` and `release.yml`, and `gh pr checks 129` (exit 0) shows all 11 checks passing at head `9c257b3`, including `check (macos-15)`, `check (ubuntu-22.04)` and `docker`. |
| 4 | SC3b / REL-02: v2.8.0 is released through release-please | UNCERTAIN (pending owner) | Not shipped yet, as the owner decided. PR 129 is OPEN, `mergedAt: null`, title `feat: remove projects from the TUI, the daemon and the PWA`, base `main`. `git tag --list 'v2.8*'` is empty. This is a release step, not a code gap. It goes to human verification. |
| 5 | Gap closures 22-06/07/08: BL-11 fixed; a removed primary stays removed on sibling relaunch, folder kept or deleted; ways back and old store files work | VERIFIED | `pty::tests::kill_and_wait_reaps_unreaped_leader_group` passed. BL-11 reads RESOLVED (`BACKLOG.md`, fix `44c2267`). `suppress_primary` is written from `removal.rs:798`. `removed_primary` is consulted at `app.rs:2634`, before `allocate_checkout_key` at `app.rs:2653`. All 10 `removed_primary_` app tests passed, plus `suppression::tests::entries_without_the_primary_member_load`. |

**Score:** 4/5 truths verified, 0 present but behavior-unverified, 1 pending the owner's merge and release.

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `baude-core/src/removal.rs` | end-state reader and 4 matrix tests | VERIFIED | Substantive; wave 2 hunks all at line 4387 or later, inside `mod tests` (starts at 1177) |
| `bauded/src/api.rs` | 2 `rel01_` daemon tests | VERIFIED | Hunks at line 3996 or later, inside `mod tests` (starts at 802); the 3996 hunk is the CI fix routing a test precondition through `is_not_repository_stderr` |
| `baude/src/app.rs` | 2 `rel01_confirm_remove_` tests plus the 22-07 consult | VERIFIED | Wave 2 hunks at 15870 or later; the 22-07 product consult is at 2634 |
| `baude-core/src/pty.rs`, `testing.rs`, `suppression.rs` | BL-11 fix, fixtures, removed-primary fact | VERIFIED | Tests pass |
| `baude-core/src/git.rs` | `is_not_repository_stderr` (CI fix `0148168`) | VERIFIED | Defined at 328; wired into `discover_repository` at 401 and into the daemon test at `api.rs:4001`; unit test passes |
| `README.md` | Removing projects section, daemon and PWA removal | VERIFIED | See truth 2 |

### Key Link Verification

| From | To | Via | Status |
|------|----|-----|--------|
| `removal.rs` matrix | `testing.rs` | `cloned_topology` | WIRED |
| `removal.rs` `update_suppression` | `suppression.rs` | `suppress_primary` (line 798) | WIRED |
| `app.rs` admission | `suppression.rs` | `removed_primary` (line 2634, before key allocation at 2653) | WIRED |
| `git.rs` `discover_repository` | `is_not_repository_stderr` | match guard at line 401 | WIRED |
| `api.rs` rel01 test | `axum::serve` on `TcpListener::bind("127.0.0.1:0")` | real socket | WIRED |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Whole workspace | `cargo test --workspace -- --test-threads=1 > target/gsd-22/verify-22/workspace-tests.log 2>&1`, exit read directly into `workspace-tests.exit` | `exit=0`; 1147 passed, 0 failed, 1 ignored (`meta::tests::pane_claude_stand_in`, a stand-in process by design). baude 360+1, baude-core 574, bauded 179, vt100 33 | PASS |
| Phase tests by name, from the same log | read from the saved log, no rerun | all 4 `end_state_matrix_`, both `rel01_` api tests, both `rel01_confirm_remove_` app tests, all 10 `removed_primary_`, `kill_and_wait_reaps_unreaped_leader_group`, `entries_without_the_primary_member_load`, `both_git_wordings_for_a_stale_gitfile_mean_no_repository` present; no line reads FAILED | PASS |
| CI on PR 129 | `gh pr checks 129` | exit 0; 11/11 pass at `9c257b3` | PASS |
| Release state | `gh pr view 129`, `git tag --list 'v2.8*'` | OPEN, not merged; no tag | pending owner |

`scripts/gate.sh` was not run, per instructions. Gate results are cited from `target/gsd-22/`.

### Probe Execution

None declared for this phase. No `scripts/*/tests/probe-*.sh` is referenced in the plans.

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| REL-01 | 22-01, 02, 03, 05, 06, 07, 08 | Integration tests per row kind, live and none, cloned and git init, asserting on fixture output | SATISFIED | Truths 1 and 5 |
| REL-02 | 22-04, 22-05 | v2.8.0 ships through release-please with README and PWA docs | PARTIAL: the docs are satisfied; the release is pending the owner's merge | Truths 2 and 4 |

No orphaned requirements. The traceability table maps only REL-01 and REL-02 to Phase 22.

### Anti-Patterns Found

None. Added lines in the phase-changed source files have no TBD, FIXME, XXX, TODO or HACK, scanned from the first 22-06 commit `c4cdd47^` to HEAD.

### Human Verification Required

### 1. v2.8.0 release

**Test:** Merge PR 129 to `main`. Let release-please open the 2.8.0 release PR and let it merge after its soak. Then run `gh release view v2.8.0`.
**Expected:** v2.8.0 is published by release-please, with no hand-edited version or changelog.
**Why human:** The owner decided that nothing in this phase merges, tags or publishes.

### Notes (informational, not gaps)

- The PWA has not been exercised in a browser, and a live `/events` client drop has not been observed. Both were carried over from Phase 21. The 22-CONTEXT deferred browser-level PWA tests and treats the real-socket `/events` test as the daemon-side stand-in. The README makes no browser claim. They are not part of SC1 as written.
- The host gate on `9c257b3` is recorded only as the log line `gate: all steps passed`, with no `sha=… exit=…` file. The commit changes only workflow YAML, and CI passed on it, so this does not weaken truth 3.
- `22-VALIDATION.md` is still `status: draft`, `nyquist_compliant: false`. That is a validation bookkeeping item, not a goal gap.

### Gaps Summary

No code or documentation gaps. The only open item is the owner's merge of PR 129 and the release-please release of v2.8.0 that follows.

---

_Verified: 2026-11-15T05:10:00Z_
_Verifier: Claude (gsd-verifier)_
