---
phase: 22-validation-and-v2-8-0-release
verified: 2026-11-16T16:30:00Z
status: passed
score: 5/5 must-haves verified
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
  - "baude/src/ui.rs"
  - "bauded/src/api.rs"
  - "bauded/web/removal-controller.js"
  - "bauded/web/removal.js"
covered_digest: "v4:sha256:f7261bfda8685ad51bad1dc4e4468b3ceb3e40dcdef9c5e9e3ac5fe5f8d2cedc"
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: human_needed
  previous_score: 4/5
  reason: "Stale fingerprint: Phase 22.1 (PR 131, merge 24de8db) changed covered source files after the 2026-11-15 report"
  gaps_closed:
    - "SC3b / REL-02: v2.8.0 is released through release-please (was pending the owner's merge of PR 129)"
  gaps_remaining: []
  regressions: []
---

# Phase 22: Validation and v2.8.0 Release Verification Report

**Phase Goal:** Removal is proven end to end on realistic fixtures for every row kind with and without a live runtime, documented, and shipped as v2.8.0 through release-please.
**Verified:** 2026-11-16T16:30:00Z, against HEAD `ec065ea` on branch `gsd/v2.8-close-out`. That branch was cut from origin/main `24de8db`. `git diff --stat 24de8db HEAD` touches only `.planning/` files. The working tree differs from HEAD only in `.planning/REQUIREMENTS.md` and this report.
**Status:** passed
**Re-verification:** Yes. The 2026-11-15 report (`9c257b3`) went stale because Phase 22.1 reworked removal, the daemon, the PWA and the test fixtures in covered files. Every truth below was re-checked against the current tree.

## What Phase 22.1 changed in Phase 22's evidence

I diffed each Phase 22 test body between `f714bf7` (the v2.8.0 feature merge) and HEAD.

- **`removal.rs` matrix:** 22.1 changed no matrix code. The `removal.rs` hunks inside `mod tests` are at 1340, 1374, 1872 and 1953: `TempTree` and `RemovalFixture` now own a `testing::FixtureRoot` in place of a manual temp dir and `Drop`. A block of new tests was also added at 3125. `read_end_state`, `assert_gone`/`assert_kept`/`assert_added` and the four `end_state_matrix_` bodies only moved.
- **`api.rs` `rel01_` tests:** each one gained exactly one line, `let _reapers = crate::manager::lock(&shared).runtime_reapers_for_test();`. The assertions did not change.
- **`app.rs` `rel01_confirm_remove_` and `removed_primary_` tests:** only teardown changed. `kill_all_and_reap(&mut app)` replaces `app.kill_all()` followed by `remove_dir_all(&fx.root)`. The assertions did not change.
- **Product wiring:** the 22-07 consult still runs before key allocation. `suppress_primary` is still written from `update_suppression`.

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | SC1 / REL-01: integration tests cover each row kind with and without a live runtime, on cloned and `git init` fixtures, and assert on fixture output that process, worktree, binding and marker state are gone | VERIFIED | `baude-core/src/removal.rs:5662,5679,5697,5718` holds the four `end_state_matrix_` tests. Run with `--nocapture`, they printed 16 distinct `CELL:` labels. Repository parent and main checkout: cloned and git-init, live and none. Managed worktree: ManagedBranch on cloned and git-init, plus PrimaryDefault on cloned, live and none. Standalone: live and none. Cloned cells use `cloned_topology` (`testing.rs:386`; used at `removal.rs:4799`), which is a real `git clone`. `read_end_state` (`removal.rs:4985`) reads `ps` state, group extinction, dir listings, `git worktree list`, a branch snapshot, the bindings file, the marker file, the suppression store and the saved state from disk. `assert_gone` (`removal.rs:5054`) requires presence before the removal at 5060. Daemon side: `bauded/src/api.rs:8796` serves `axum::serve` on `TcpListener::bind("127.0.0.1:0")` and covers `/rows`, `/removals/preview`, `/removals`, an `/events` `rows_removed` frame, and then a fresh `restore_at`. `api.rs:8984` reads a cloned parent removal back from disk and pins the marker as absent. TUI side: `baude/src/app.rs:15904` and `app.rs:16026`. All of these passed in this run (see spot-checks). |
| 2 | SC2 / REL-02 docs: README and PWA docs describe the remove action, its confirmation, and what is and is not deleted | VERIFIED | `README.md:397` `## Removing projects` covers the `D` key, the confirmation (411), the deletes and keeps table (432-445), what baude forgets (472-502), and what happens after the default checkout is removed (504). The `D` row in the Keys table is at `README.md:153`. `D` on remote rows is at `README.md:911-937`. The endpoint rows are at `README.md:981-984`. `### Removing rows over the API` (986-1036) has the status table, including the 22.1 503 text, the `repository busy:`/`folder busy:` bodies and `saved_degraded`. `### Removing rows in the PWA` (1038-1086) has the 22.1 titles and `other sessions`. I spot-checked each quoted string against code. Forced remove argv: `git.rs:2944-2951`. TUI title and footers: `ui.rs:2753,2847,2849`. PWA titles: `removal.js:233-240`, with standalone mapped to `standalone session` at `removal.js:148`. Result line: `removal.js:328-329`. `already removed` and `removed elsewhere`: `removal-controller.js:246,255`. `other sessions` heading: `removal-controller.js:574`. 503 bodies: `api.rs:260,269`. Default-checkout messages: `app.rs:2643,2647`. All match. |
| 3 | SC3a: `scripts/gate.sh` and `scripts/gate.sh --linux` pass | VERIFIED | `target/gsd-22.1/gate-result.txt` reads `head=356e9b9… mac=0 linux=0`, and both `gate-mac.log` and `gate-linux.log` end `gate: all steps passed`. `356e9b9` is the PR 131 head. `git diff 356e9b9 24de8db` outside `.planning/` is empty, and `24de8db..HEAD` touches only `.planning/`, so the gated tree is the current tree. `gh pr checks 131`: all 11 checks pass, including `check (macos-15)`, `check (ubuntu-22.04)` and `docker`. The `ci` push run on `main` at `24de8db` concluded `success`. Not re-run here, as instructed. |
| 4 | SC3b / REL-02: v2.8.0 is released through release-please | VERIFIED | `gh release view v2.8.0`: tag `v2.8.0`, not draft, not prerelease, published `2026-11-15T21:24:10Z`, author `release-please-baude[bot]`, target `30089e4`. PR 129 is MERGED as `f714bf7` (2026-11-15T16:11:06Z). Release PR 130 `chore(main): release 2.8.0`, authored by `app/release-please-baude`, is MERGED as `30089e4`, and `git rev-list -n1 v2.8.0` is `30089e4`. |
| 5 | Gap closures 22-06/07/08: BL-11 fixed; a removed primary stays removed on sibling relaunch, folder kept or deleted; ways back and old store files work | VERIFIED | `pty::tests::kill_and_wait_reaps_unreaped_leader_group` (`pty.rs:2140`) passed. BL-11 reads `✅ RESOLVED 2026-11-14 via plan 22-06` (`BACKLOG.md:471-475`). `suppress_primary` (`suppression.rs:148`) is written from `update_suppression` at `removal.rs:930`, which is product code ahead of `mod tests` at 1309. `removed_primary` (`suppression.rs:130`) is consulted at `app.rs:2635`, before `allocate_checkout_key` at `app.rs:2654`. All 8 `removed_primary_` app tests passed (`app.rs:11251-11819`): 2 from 22-07 and 6 from 22-08, as the plans name them. So did `suppression::tests::entries_without_the_primary_member_load` (`suppression.rs:435`). The previous report's "10 `removed_primary_` tests" counted the two `rel01_confirm_remove_` tests that ran under the same filter. The real count is 8, and it was 8 at `f714bf7` too. |

**Score:** 5/5 truths verified, 0 present but behavior-unverified.

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `baude-core/src/removal.rs` | end-state reader and 4 matrix tests | VERIFIED | `read_end_state` at 4985 and the matrix at 5662-5729, all inside `mod tests` (starts at 1309). 22.1 touched only fixture ownership in this file's tests |
| `bauded/src/api.rs` | 2 `rel01_` daemon tests | VERIFIED | At 8796 and 8984, inside `mod tests` (starts at 911). 22.1 added one reaper guard line to each |
| `baude/src/app.rs` | 2 `rel01_confirm_remove_` tests plus the 22-07 consult | VERIFIED | Tests at 15904 and 16026. The consult is at 2635 |
| `baude-core/src/pty.rs`, `testing.rs`, `suppression.rs` | BL-11 fix, fixtures, removed-primary fact | VERIFIED | `parked_reader_command` is at `testing.rs:494`. 22.1 added `FixtureRoot` at `testing.rs:593` and `RuntimeReaper` at `testing.rs:720`. Tests pass |
| `baude-core/src/git.rs` | `is_not_repository_stderr` | VERIFIED | Defined at 328 and wired into `discover_repository` at 398 and into the daemon test at `api.rs:5707`. `both_git_wordings_for_a_stale_gitfile_mean_no_repository` (338) passes |
| `README.md` | Removing projects section, daemon and PWA removal | VERIFIED | See truth 2 |

### Key Link Verification

| From | To | Via | Status |
|------|----|-----|--------|
| `removal.rs` matrix | `testing.rs` | `cloned_topology` (`removal.rs:4799`) | WIRED |
| `removal.rs` `update_suppression` (887) | `suppression.rs` | `suppress_primary` (line 930) | WIRED |
| `app.rs` admission | `suppression.rs` | `removed_primary` (line 2635, before key allocation at 2654) | WIRED |
| `git.rs` `discover_repository` | `is_not_repository_stderr` | match guard at line 398 | WIRED |
| `api.rs` rel01 test | `axum::serve` on `TcpListener::bind("127.0.0.1:0")` | real socket | WIRED |
| `api.rs`, `app.rs` rel01 tests | `folder_workspace::` | seeded with `record`, read back from the redirected config dir | WIRED |

### Behavioral Spot-Checks

Logs are in `/private/tmp/claude-503/-Users-joese-Code-github-com-poindexter12-baude/a2f6da22-a53d-4447-9fb2-62cf58c24430/scratchpad/`. Each exit was captured directly, never through a pipe.

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Core matrix, BL-11, old store, git wordings | `cargo test -p baude-core --lib -- --test-threads=1 end_state_matrix_ kill_and_wait_reaps_unreaped_leader_group entries_without_the_primary_member_load both_git_wordings_for_a_stale_gitfile_mean_no_repository` | exit 0; 7 passed, 0 failed | PASS |
| Matrix cell count | same matrix filter with `--nocapture` | exit 0; 16 distinct `CELL:` labels | PASS |
| Daemon rel01 | `cargo test -p bauded -- --test-threads=1 rel01_` | exit 0; 2 passed, 0 failed | PASS |
| TUI rel01 and removed primary | `cargo test -p baude --bin baude -- --test-threads=1 rel01_confirm_remove_ removed_primary_` | exit 0; 10 passed (2 + 8), 0 failed | PASS |
| CI on the current tree | `gh pr checks 131`; `gh run list --commit 24de8db` | 11/11 pass; `ci` push on main `success` | PASS |
| Release | `gh release view v2.8.0`; `gh pr view 129/130` | published by release-please, both PRs merged | PASS |

`scripts/gate.sh` and full suites were not run, as instructed. Gate results are cited from `target/gsd-22.1/`.

### Probe Execution

None declared for this phase. No `scripts/*/tests/probe-*.sh` is referenced in the plans.

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| REL-01 | 22-01, 02, 03, 05, 06, 07, 08 | Integration tests per row kind, live and none, cloned and git init, asserting on fixture output | SATISFIED | Truths 1 and 5 |
| REL-02 | 22-04, 22-05 | v2.8.0 ships through release-please with README and PWA docs | SATISFIED | Truths 2 and 4 |

No orphaned requirements. The traceability table maps only REL-01 and REL-02 to Phase 22. The working tree has both checked and marked Complete in `.planning/REQUIREMENTS.md`, which is consistent with this report.

### Anti-Patterns Found

None. I scanned the added lines in the covered source files and workflows from the first 22-06 commit `c4cdd47^` to HEAD, which includes all of 22.1, and found no TBD, FIXME, XXX, TODO or HACK.

### Human Verification Required

None. The v2.8.0 release item from the previous report is resolved (truth 4).

### Notes (informational, not gaps)

- v2.8.0 is tagged at `30089e4`. The 22.1 cleanup (`24de8db`) landed after it and is not part of v2.8.0. It ships in the next release-please release. Phase 22's truths hold on both trees: the release-shipped code was verified on 2026-11-15, and this run covers the current tree.
- `22-VALIDATION.md` is still `status: draft`, `nyquist_compliant: false`. That is a validation bookkeeping item, not a goal gap.
- A remote TUI receiving `rows_removed` live remains unobserved, per `.planning/v2.8-MILESTONE-AUDIT.md:58`. SC1 does not require it.

### Gaps Summary

No code, documentation or release gaps. All five truths hold against the current tree.

### Resolution, 2026-11-16

- **v2.8.0 release:** PR 129 merged to `main` as `f714bf7`. release-please opened release PR 130, which merged, and published v2.8.0 at 2026-11-15T21:24:10Z with no hand-edited version or changelog. Truth 4 and REL-02 are satisfied.
- **Browser note:** the owner exercised the PWA against a sandboxed daemon on 2026-11-15, recorded in `.planning/v2.8-MILESTONE-AUDIT.md`, and again for the Phase 22.1 changes on 2026-11-16.

---

_Verified: 2026-11-16T16:30:00Z (re-verification; initial 2026-11-15T05:10:00Z)_
_Verifier: Claude (gsd-verifier)_
