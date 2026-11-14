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
> Source: 22-RESEARCH.md "Validation Architecture" (line 465 onward) and the `<verify>` blocks of plans 22-01 to 22-05.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` with in-crate `mod tests`; `node --test` for `bauded/web/tests`, run by the gate |
| **Config file** | none |
| **Quick run command** | per crate touched: `cargo test -p baude-core --lib removal::tests -- --test-threads=1`, `cargo test -p bauded --bin bauded api::tests::rel01_ -- --test-threads=1`, `cargo test -p baude app::tests::rel01_ -- --test-threads=1` |
| **Full suite command** | `scripts/gate.sh`, then `scripts/gate.sh --linux` |
| **Estimated runtime** | not measured at planning time; the gates run long enough that plan 22-05 starts each in the background and waits |

Every exit code is read directly. No test or gate command is piped into `tail` or `grep` to be judged by `$?`, and no pass check greps for a test name followed by `ok`. Each task's command echoes its exit codes on one line and then tests them.

---

## Sampling Rate

- **After every task commit:** the task's own `<automated>` command. The last task of each code plan adds `cargo fmt --check -p <crate>`, `cargo clippy -p <crate> --all-targets -- -D warnings`, and a check that every diff hunk sits inside `mod tests`. The format check is per crate so that plans sharing wave 1 do not fail on each other's half-written edits; the workspace-wide check runs in the gate.
- **After wave 1:** no separate gate run is planned. Wave 2 is plan 22-05, whose first task is the host gate and then the Linux gate on the final commit.
- **Before the push:** both gates exited 0 on the exact commit being pushed, then the commit-message range check, the title check and the squash-body simulation.
- **Before `/gsd-verify-work`:** both gate result files name the pushed commit with `exit=0`.
- **Max feedback latency:** not measured.

---

## Per-Task Verification Map

The command column gives the core of each command. The full form, with the list-count and exit-code tests, is in the plan's `<verify>` block. Evidence files go under git-ignored `target/gsd-22/`.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 22-01-01 | 01 | 1 | REL-01 | T-22-01, T-22-02 | fixture stays under the redirected root; every gone assertion has a present-before twin | integration, in-crate | `cargo test -p baude-core --lib removal::tests::end_state_matrix_ -- --test-threads=1`; listed tests equal 1 | the task creates the test | ⬜ pending |
| 22-01-02 | 01 | 1 | REL-01 | T-22-01, T-22-02 | as above | integration, in-crate | same filter; listed tests equal 2 | the task creates the tests | ⬜ pending |
| 22-01-03 | 01 | 1 | REL-01 | T-22-01, T-22-02 | as above; product code outside `mod tests` unchanged | integration plus regression | `cargo test -p baude-core --lib removal::tests -- --test-threads=1`; fmt; clippy; diff hunks at or after line 1170; listed tests equal 4 | the task creates the tests | ⬜ pending |
| 22-02-01 | 02 | 1 | REL-01 | T-22-03, T-22-04, T-22-05 | listener bound to `127.0.0.1:0` only; socket reads under a timeout | integration over a real socket | `cargo test -p bauded --bin bauded api::tests::rel01_ -- --test-threads=1`; listed tests equal 1 | the task creates the test | ⬜ pending |
| 22-02-02 | 02 | 1 | REL-01 | T-22-03, T-22-05 | config dir asserted under the fixture root; no observation from the report alone | integration plus regression | `cargo test -p bauded --bin bauded -- --test-threads=1 api::tests::rel01_ api::tests::events_`; fmt; clippy; hunks at or after line 802; `Cargo.lock` and `bauded/Cargo.toml` unchanged; listed tests equal 2, or 1 when `02-g6-covered.txt` names the covering test | the task creates the test or records the covering one | ⬜ pending |
| 22-03-01 | 03 | 1 | REL-01 | T-22-06, T-22-07 | config dir asserted under the fixture root; after-state read from the process table and files | integration, in-crate | `cargo test -p baude app::tests::rel01_ -- --test-threads=1`; listed tests equal 1, or 0 when `03-main-covered.txt` names the covering test | the task creates the test or records the covering one | ⬜ pending |
| 22-03-02 | 03 | 1 | REL-01 | T-22-06, T-22-07 | as above | integration plus regression | `rel01_` with the module's existing removal tests; fmt; clippy; hunks at or after line 8341; listed tests equal 2 minus each covered cell | the task creates the test or records the covering one | ⬜ pending |
| 22-04-01 | 04 | 1 | REL-02 | T-22-08 | README states only deletes and keeps traced to a file and line | docs, counted greps with recorded baselines | `## Removing projects` heading count equals 1; `D` key row count equals 1; no file but `README.md` changed since the base commit, not counting `.planning` and the three files plans 22-01 to 22-03 own | ✅ `README.md` | ⬜ pending |
| 22-04-02 | 04 | 1 | REL-02 | T-22-08, T-22-09 | no claim of a browser or live-stream check that did not happen | docs, counted greps with recorded baselines | counts for the two removal endpoints, `GET /rows`, `GET /events`, `rows_removed`, and `D` inside the remote-sessions section; `bash scripts/check-commit-message.sh --range <base>..HEAD` | ✅ `README.md` | ⬜ pending |
| 22-05-01 | 05 | 2 | REL-01, REL-02 | T-22-11 | both gates exit 0 on one commit; tree clean outside `.planning` | gate | `scripts/gate.sh`; `scripts/gate.sh --linux`; each result file equals `sha=<HEAD> exit=0` | ✅ `scripts/gate.sh` | ⬜ pending |
| 22-05-02 | 05 | 2 | REL-02 | T-22-10 | expected GitHub login; SSH remote; no breaking or `Release-As` marker | message guards | `bash scripts/check-commit-message.sh --range origin/main..HEAD`; the same checker on the title file and the squash-body file; marker count equals 0 | ✅ `scripts/check-commit-message.sh` | ⬜ pending |
| 22-05-03 | 05 | 2 | REL-02 | T-22-11, T-22-12 | remote commit equals the gated commit; nothing merged, tagged or published | remote state | `git ls-remote origin refs/heads/<branch>` against both result files; `gh pr list` open with the fixed title and merged count 0; `git tag --list v2.8.0` empty | ✅ evidence from Tasks 1 and 2 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

Known flaky signature: BL-11, a 500 with the text "failed to terminate exact PTY process group" on macOS session teardown. Plans 22-02, 22-03 and 22-05 allow one rerun on exactly that signature and record both results. No test gains a retry.

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements. `cargo test`, the in-crate fixtures, both gate modes and the commit-message checker are already in the tree.

The research lists the core end-state matrix and the real-HTTP `/events` test as Wave 0 gaps. They are the wave 1 deliverables of plans 22-01 and 22-02, not scaffolds for a later task. Each task that adds a test verifies it by a listed-test count whose baseline at planning time is 0, so no `<automated>` block refers to a missing file.

These are characterization tests of shipped behaviour and are expected to pass on first run. No RED commit exists; a bite check in each task stands in for it. A red result on first run is a product defect: the executor stops, records it and returns to the orchestrator.

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
