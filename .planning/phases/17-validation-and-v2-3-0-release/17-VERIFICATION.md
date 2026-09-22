---
phase: 17-validation-and-v2-3-0-release
verified: 2026-09-22T17:55:00Z
status: passed
score: 4/4 success criteria verified
covered_files:
  - .planning/phases/17-validation-and-v2-3-0-release/17-01-PLAN.md
  - .planning/phases/17-validation-and-v2-3-0-release/17-01-SUMMARY.md
  - baude/src/main.rs
  - README.md
covered_digest: v1:sha256:cd78b0b92e3308744cb4581424a5f81f6c62d6116c669bfc822d99b4f2880a9e
re_verification: false
overrides_applied: 0
behavior_unverified: 1
---

# Phase 17: Validation and v2.3.0 Release Verification

**Phase goal:** v2.3.0 is ready to publish after tests, CI, and smoke validation confirm
stability and the new features work end to end.

**Status:** PASSED, with one criterion partially deferred to a human (see below).

## Criterion evidence

| # | Criterion | Verdict | Evidence |
|---|-----------|---------|----------|
| 1 | All tests pass | PASSED | `cargo test --workspace` exit 0, 793 tests, run on the merged tree |
| 2 | Clippy reports no warnings | PASSED | `cargo clippy --all-targets -- -D warnings` exit 0 |
| 3 | Terminal smoke confirms new defaults, workspace derivation, startup performance | PASSED (automated half) / DEFERRED (human half) | Isolated release-build run derived workspace `repo` and wrote `state-repo.json`; `BAUDE_TIMING=1` printed all stages; probe held its 250 ms bound; `worktrees scan` printed owner rows. The observations needing a real terminal are listed as a checklist in 17-01-SUMMARY.md |
| 4 | Release notes and README reflect the new behaviour | PASSED | README covers workspace derivation and new-session defaults, the worktree identity scheme and marker file, Status codes plus the Performance section, and the `alt+↑/↓` chord. CHANGELOG is release-please generated and arrived with the merge |

## behavior_unverified: 1

Criterion 3's human half. baude is a full-screen TUI; seven observations (kitty-capable
probe timing, idle CPU, legend geometry at the 20/21-row boundary, directional pane focus,
focus memory across a switch, and the suspend/resume round trip) require a person at a real
terminal. Every mechanically checkable part was automated instead. The checklist is in
17-01-SUMMARY.md.

## Integration state

`origin/main` is an ancestor of HEAD (merged, not rebased: `required_linear_history` is
false and `allow_merge_commit` is true on this repository). 0 behind, 204 ahead. Backup ref
`backup/pre-main-merge-20260922-104929`. Nothing was pushed and no pull request was opened;
those are the owner's call.

## Defect found during validation

`fix(17-01)` 3ece712: loop timing stages reported elapsed-since-origin under a column of
per-stage durations, so three stages printed identical numbers and the report misread as
"the first frame took two seconds". Now per-stage durations, with a regression test.
