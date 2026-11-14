---
phase: "22"
slug: "validation-and-v2-8-0-release"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-11-13"
---

# Phase 22 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Source: 22-RESEARCH.md "Validation Architecture" (line 465 onward) and the `<verify>` blocks of plans 22-01 to 22-07.
> Plans 22-06 and 22-07 were added on 2026-11-14 as gap closure. Verifying phases 18 and 19 reproduced two product defects, and the owner decided both are fixed here before the validation tests run.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` with in-crate `mod tests`; `node --test` for `bauded/web/tests`, run by the gate |
| **Config file** | none |
| **Quick run command** | per crate touched: `cargo test -p baude-core --lib removal::tests -- --test-threads=1`, `cargo test -p bauded --bin bauded api::tests::rel01_ -- --test-threads=1`, `cargo test -p baude app::tests::rel01_ -- --test-threads=1`; for the two fixes: `cargo test -p baude-core --lib pty::tests::kill_and_wait_reaps_unreaped_leader_group -- --exact`, `cargo test -p baude app::tests::removed_primary_stays_removed_on_sibling_relaunch -- --exact` |
| **Full suite command** | `scripts/gate.sh`, then `scripts/gate.sh --linux` |
| **Estimated runtime** | not measured at planning time; the gates run long enough that plan 22-05 starts each in the background and waits |

Every exit code is read directly. No test or gate command is piped into `tail` or `grep` to be judged by `$?`, and no pass check greps for a test name followed by `ok`. Each task's command echoes its exit codes on one line and then tests them.

---

## Sampling Rate

- **After every task commit:** the task's own `<automated>` command. The last task of each code plan adds `cargo fmt --check -p <crate>`, `cargo clippy -p <crate> --all-targets -- -D warnings`, and a check that every diff hunk sits inside `mod tests`. The format check is per crate so that plans sharing a wave do not fail on each other's half-written edits; the workspace-wide check runs in the gate.
- **Wave 1, the two fixes:** plans 22-06 and 22-07 each commit a failing test first and keep that run's log (`06-red.log`, `07-red.log`, each with exit 101), then commit the fix. Plan 22-06 touches platform code, so its last task runs `scripts/gate.sh --linux` and records `linux=0` before the backlog is edited. No host gate run is planned for wave 1.
- **Wave 1 cross-talk:** both plans compile `baude-core`, plan 22-07 holds a red test between its two commits, and plan 22-06's Linux gate reads the whole working tree. Each plan names the other's files and waits and reruns on a failure that sits in them. Running the two plans one after the other, 22-06 first, removes the wait.
- **After wave 2:** no separate gate run is planned. Wave 3 is plan 22-05, whose first task is the host gate and then the Linux gate on the final commit.
- **Before the push:** both gates exited 0 on the exact commit being pushed, then the commit-message range check, the title check and the squash-body simulation.
- **Before `/gsd-verify-work`:** both gate result files name the pushed commit with `exit=0`.
- **Max feedback latency:** not measured.

---

## Per-Task Verification Map

The command column gives the core of each command. The full form, with the list-count and exit-code tests, is in the plan's `<verify>` block. Evidence files go under git-ignored `target/gsd-22/`.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 22-06-01 | 06 | 1 | REL-01, closing Phase 18 truth 13 | T-22-13, T-22-14 | the leader is read as a zombie from `ps` immediately before the call; the red log is kept | red then green, in-crate | `cargo test -p baude-core --lib pty::tests::kill_and_wait_reaps_unreaped_leader_group -- --exact`; listed tests equal 1; `06-red.log` from a run that exited 101 | the task creates the test | ⬜ pending |
| 22-06-02 | 06 | 1 | REL-01 | T-22-13 | the probe still counts `EPERM` as alive; `group_disposition` and its tests untouched | unit pins plus regression | `cargo test -p baude-core --lib -- --test-threads=1`; fmt; clippy | the task creates the tests | ⬜ pending |
| 22-06-03 | 06 | 1 | REL-01 | T-22-13 | the backlog is edited only after the recorded `linux=0` | gate, then docs | `scripts/gate.sh --linux`; status file line `linux=0` count equals 1; BL-11 status line count equals 1; old unconfirmed-cause sentence count equals 0 | ✅ `scripts/gate.sh` | ⬜ pending |
| 22-07-01 | 07 | 1 | REL-01, closing Phase 19 SC2 (READM-03, READM-04) | T-22-16, T-22-17 | the entry is written by a real removal, never seeded; no key allocated and no worktree created when derivation is skipped | red then green, in-crate | `cargo test -p baude app::tests::removed_primary_stays_removed_on_sibling_relaunch -- --exact`; `cargo test -p baude-core --lib default_worktree`; listed tests equal 1; `07-red.log` from a run that exited 101 | the task creates the test | ⬜ pending |
| 22-07-02 | 07 | 1 | REL-01 | T-22-17 | launching inside the removed folder, `n` and `c` each lift the entry and admit a fresh row | integration plus regression | `cargo test -p baude -- --test-threads=1 removed_ relaunch_ suppress admitting_`; fmt; clippy; listed `removed_primary_` tests equal 5 | the task creates the tests | ⬜ pending |
| 22-01-01 | 01 | 2 | REL-01 | T-22-01, T-22-02 | fixture stays under the redirected root; every gone assertion has a present-before twin | integration, in-crate | `cargo test -p baude-core --lib removal::tests::end_state_matrix_ -- --test-threads=1`; listed tests equal 1 | the task creates the test | ⬜ pending |
| 22-01-02 | 01 | 2 | REL-01 | T-22-01, T-22-02 | as above | integration, in-crate | same filter; listed tests equal 2 | the task creates the tests | ⬜ pending |
| 22-01-03 | 01 | 2 | REL-01 | T-22-01, T-22-02 | as above; product code outside `mod tests` unchanged | integration plus regression | `cargo test -p baude-core --lib removal::tests -- --test-threads=1`; fmt; clippy; diff hunks at or after line 1170; listed tests equal 4 | the task creates the tests | ⬜ pending |
| 22-02-01 | 02 | 2 | REL-01 | T-22-03, T-22-04, T-22-05 | listener bound to `127.0.0.1:0` only; socket reads under a timeout | integration over a real socket | `cargo test -p bauded --bin bauded api::tests::rel01_ -- --test-threads=1`; listed tests equal 1 | the task creates the test | ⬜ pending |
| 22-02-02 | 02 | 2 | REL-01 | T-22-03, T-22-05 | config dir asserted under the fixture root; no observation from the report alone | integration plus regression | `cargo test -p bauded --bin bauded -- --test-threads=1 api::tests::rel01_ api::tests::events_`; fmt; clippy; hunks at or after line 802; `Cargo.lock` and `bauded/Cargo.toml` unchanged; listed tests equal 2, or 1 when `02-g6-covered.txt` names the covering test | the task creates the test or records the covering one | ⬜ pending |
| 22-03-01 | 03 | 2 | REL-01 | T-22-06, T-22-07 | config dir asserted under the fixture root; after-state read from the process table and files | integration, in-crate | `cargo test -p baude app::tests::rel01_ -- --test-threads=1`; listed tests equal 1, or 0 when `03-main-covered.txt` names the covering test | the task creates the test or records the covering one | ⬜ pending |
| 22-03-02 | 03 | 2 | REL-01 | T-22-06, T-22-07 | as above | integration plus regression | `rel01_` with the module's existing removal tests; fmt; clippy; hunks at or after line 8341; listed tests equal 2 minus each covered cell | the task creates the test or records the covering one | ⬜ pending |
| 22-04-01 | 04 | 2 | REL-02 | T-22-08 | README states only deletes and keeps traced to a file and line | docs, counted greps with recorded baselines | `## Removing projects` heading count equals 1; `D` key row count equals 1; no file but `README.md` changed since the base commit, not counting `.planning` and the three files plans 22-01 to 22-03 own | ✅ `README.md` | ⬜ pending |
| 22-04-02 | 04 | 2 | REL-02 | T-22-08, T-22-09 | no claim of a browser or live-stream check that did not happen | docs, counted greps with recorded baselines | counts for the two removal endpoints, `GET /rows`, `GET /events`, `rows_removed`, and `D` inside the remote-sessions section; `bash scripts/check-commit-message.sh --range <base>..HEAD` | ✅ `README.md` | ⬜ pending |
| 22-05-01 | 05 | 3 | REL-01, REL-02 | T-22-11 | both gates exit 0 on one commit; tree clean outside `.planning` | gate | `scripts/gate.sh`; `scripts/gate.sh --linux`; each result file equals `sha=<HEAD> exit=0` | ✅ `scripts/gate.sh` | ⬜ pending |
| 22-05-02 | 05 | 3 | REL-02 | T-22-10 | expected GitHub login; SSH remote; no breaking or `Release-As` marker | message guards | `bash scripts/check-commit-message.sh --range origin/main..HEAD`; the same checker on the title file and the squash-body file; marker count equals 0 | ✅ `scripts/check-commit-message.sh` | ⬜ pending |
| 22-05-03 | 05 | 3 | REL-02 | T-22-11, T-22-12 | remote commit equals the gated commit; nothing merged, tagged or published | remote state | `git ls-remote origin refs/heads/<branch>` against both result files; `gh pr list` open with the fixed title and merged count 0; `git tag --list v2.8.0` empty | ✅ evidence from Tasks 1 and 2 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

Task 22-06-01 can only show red on macOS: Linux signals a zombie-only group without error. The Linux gate in task 22-06-03 therefore shows the fix breaks nothing there; it does not show the fix.

BL-11, a failure with the text "failed to terminate exact PTY process group" on macOS session teardown, was this phase's known flaky signature. Plan 22-06 fixes its cause in wave 1. From wave 2 on that text is a defect: the executor stops and returns, and plan 22-02 says so. Inside wave 1 only, plan 22-07 may meet the text before the `fix(22-06)` commit lands, and waits for that commit. No test gains a retry.

Plan 22-05 allows no rerun of the host gate and lists BL-11 as fixed in the pull request body. Its Task 1 verify and its push guard each count exactly three of `18-`, `19-` and `20-VERIFICATION.md` reading `status: passed`; the gate truths of those phases are discharged by 22-05's own gate result files.

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements. `cargo test`, the in-crate fixtures, both gate modes and the commit-message checker are already in the tree.

The research lists the core end-state matrix and the real-HTTP `/events` test as Wave 0 gaps. They are the wave 2 deliverables of plans 22-01 and 22-02, not scaffolds for a later task. Each task that adds a test verifies it by a listed-test count whose baseline at planning time is 0, so no `<automated>` block refers to a missing file.

The tests of plans 22-01 to 22-03 are characterization tests of shipped behaviour and are expected to pass on first run. They have no RED commit; a bite check in each task stands in for it. A red result on first run is a product defect: the executor stops, records it and returns to the orchestrator.

Plans 22-06 and 22-07 are the exception and follow red, then green. Each starts from a test that fails at HEAD on a realistic fixture: a PTY leader that has exited and that nothing has reaped, and the cloned relaunch fixture after a real removal. The red run's log is kept, the red test is committed alone, and the fix is a second commit. If either test passes at HEAD, or fails for another reason, the executor stops and returns.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| README statements about the remove action, its confirmation, and what is and is not deleted are accurate | REL-02 | prose accuracy cannot be asserted by a command; the greps only prove the sections exist | the owner reads the README diff in the pull request against the traceability table in 22-04-SUMMARY.md |
| v2.8.0 is released through release-please | REL-02 | by owner decision no plan merges, tags or publishes | the owner merges the pull request; release-please opens the 2.8.0 release pull request, which merges itself after its soak; then `gh release view v2.8.0` |
| PWA removal in a browser, a live event-stream client drop, the 15 second keep-alive | REL-01 context, carried from 21-VERIFICATION.md | needs a browser and a long-lived client; out of this phase's plans | not planned here; the pull request body lists them as never observed. The real-socket `/events` test does not replace a browser run |

Criterion 3 is half automated. "Both gates pass" is task 22-05-01. "v2.8.0 is released" stays open when the phase's plans finish and is the owner's to close.

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency measured; not measured at planning time
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
