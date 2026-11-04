---
phase: 19-re-admission-suppression
plan: 03
subsystem: marker
tags: [rust, marker, managed-root, baude-core, tdd]

# Dependency graph
requires:
  - phase: 08-workspace-identity
    provides: "marker::write_marker and read_marker, the fail-closed ownership marker inside repository-<physical_key>"
provides:
  - "marker::clear_owned(dir, owner_common_dir) -> io::Result<MarkerClear>: owner-checked marker deletion followed by non-recursive directory removal"
  - "ClearRefusal { ForeignOwner, Invalid(InvalidReason), DirectoryIsSymlink }"
  - "MarkerOutcome { Removed, Missing, Refused(ClearRefusal) }"
  - "DirectoryOutcome { Removed, Missing, NotEmpty, Untouched }"
  - "MarkerClear { marker, directory }"
  - "private MARKER_FILE constant shared by write_marker, read_marker and clear_owned"
affects: [19-04, 19-07, 20-removal-confirmation]

# Actuals (#2632)
actuals:
  tokens: 3627
  tasks: 3
  commits: 2
plan_head_before: 05197dd6bd77d29267345ceb7bef90d28636fba5
plan_head_after: 691438ba4907f5747f8ec255082f405a8cec001d

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Inverse of an atomic writer reuses the writer's reader: clear_owned goes through read_marker so symlink, oversize, malformed and scheme-too-new stay in one place"
    - "Typed refusal instead of error: a marker that is not provably ours is reported as Refused and left in place, never deleted on a guess"

key-files:
  created: []
  modified:
    - baude-core/src/marker.rs

key-decisions:
  - "The marker file name was hoisted into MARKER_FILE during the GREEN step rather than deferred to Task 3, so the refactor task had nothing left to do"
  - "A refusal short-circuits with directory Untouched; remove_dir is only attempted once the marker is Removed or Missing"
  - "Test fixtures name .baude-marker.json literally so they assert the on-disk file name independently of the constant"

patterns-established:
  - "Owner bytes compared with == on Vec<u8> against &[u8], the same comparison write_marker uses for AlreadyOwned"

requirements-completed: [READM-02]

coverage:
  - id: D1
    description: "Owned marker in an empty directory: marker Removed, directory Removed, read_marker Missing, dir gone; an empty directory with no marker: Missing plus Removed"
    requirement: READM-02
    verification:
      - kind: unit
        ref: "cargo test -p baude-core marker::tests::clear_owned_removes_owned_marker_and_empty_directory -- --test-threads=1"
        status: pass
    human_judgment: false
  - id: D2
    description: "Foreign owner bytes: Refused ForeignOwner, directory Untouched, marker bytes byte-identical, read_marker still Valid"
    requirement: READM-02
    verification:
      - kind: unit
        ref: "cargo test -p baude-core marker::tests::clear_owned_refuses_foreign_marker_and_leaves_directory -- --test-threads=1"
        status: pass
    human_judgment: false
  - id: D3
    description: "Malformed marker refused as Invalid Malformed with bytes unchanged; symlinked marker refused as Invalid Symlink with the link and its target surviving"
    requirement: READM-02
    verification:
      - kind: unit
        ref: "cargo test -p baude-core marker::tests::clear_owned_refuses_invalid_and_symlinked_marker -- --test-threads=1"
        status: pass
    human_judgment: false
  - id: D4
    description: "Missing directory, including the repository- path a legacy empty physical_key resolves to: Missing plus Missing, parent untouched"
    requirement: READM-02
    verification:
      - kind: unit
        ref: "cargo test -p baude-core marker::tests::clear_owned_missing_directory_is_a_noop -- --test-threads=1"
        status: pass
    human_judgment: false
  - id: D5
    description: "Owned marker plus a subdirectory: marker Removed, directory NotEmpty, the subdirectory and its file survive"
    requirement: READM-02
    verification:
      - kind: unit
        ref: "cargo test -p baude-core marker::tests::clear_owned_keeps_non_empty_directory_after_marker_removal -- --test-threads=1"
        status: pass
    human_judgment: false
  - id: D6
    description: "Directory that is a symlink: Refused DirectoryIsSymlink, directory Untouched, the link, its target directory and the target's marker all survive"
    requirement: READM-02
    verification:
      - kind: unit
        ref: "cargo test -p baude-core marker::tests::clear_owned_refuses_symlinked_directory -- --test-threads=1"
        status: pass
    human_judgment: false

# Metrics
duration: 3min
completed: 2026-11-04
status: complete
---

# Phase 19 Plan 03: Owner-checked marker clear Summary

**`marker::clear_owned` releases a managed repository directory by deleting `.baude-marker.json` only when `read_marker` proves the owner bytes match, then removing the directory with non-recursive `remove_dir`, and reports every other case as a typed refusal instead of guessing.**

## Performance

- **Duration:** about 3 min of wall clock across the two commits
- **Started:** 2026-11-04T18:12:42Z
- **Completed:** 2026-11-04T18:15:30Z
- **Tasks:** 3 (RED, GREEN, REFACTOR with no refactor needed)
- **Files modified:** 1

## Accomplishments

- `clear_owned` checks `symlink_metadata(dir)` first: `NotFound` is `{ Missing, Missing }` and a symlink is `{ Refused(DirectoryIsSymlink), Untouched }` before anything is read through it (D-08).
- The owner check reuses `read_marker`, so the symlink, oversize, malformed and scheme-too-new rules live in one place; only a `Valid` marker whose `canonical_common_dir` byte-equals the owner is deleted, matching `write_marker`'s refusal posture (D-06).
- Directory removal is `std::fs::remove_dir` only; `DirectoryNotEmpty` becomes `DirectoryOutcome::NotEmpty` for the caller to report and is never retried recursively (D-07). The production region of `marker.rs` contains no `remove_dir_all`.
- Four outcome types with `Clone, Debug, Eq, PartialEq` give plan 04 a fully typed value to map onto `ClearedItem` and `ClearingFailure`.
- Six tests cover the whole behaviour table and assert on filesystem state: marker bytes before and after, `read_marker`, `exists`, `is_dir` and `symlink_metadata` of the links.

## Task Commits

1. **Task 1 RED: failing clear_owned tests** - `471fc7f` (test)
2. **Task 2 GREEN: clear_owned and the outcome types** - `691438b` (feat)
3. **Task 3 REFACTOR:** no commit; condition did not hold

**Plan metadata:** recorded in the final docs commit.

## TDD Gate Compliance

| Gate | Commit | Evidence |
|------|--------|----------|
| RED | `471fc7f` | `cargo test -p baude-core marker::tests::clear_owned -- --test-threads=1` exit 101: E0425 `clear_owned` x8, E0422 `MarkerClear` x7, E0433 `MarkerOutcome` x8, `DirectoryOutcome` x8, `ClearRefusal` x4 |
| GREEN | `691438b` | `cargo test -p baude-core marker::tests -- --test-threads=1` exit 0, `running 15 tests`, `test result: ok. 15 passed` |
| REFACTOR | skipped by design | `MARKER_FILE` was hoisted in GREEN; `cargo fmt --check` and `cargo clippy -p baude-core --all-targets -- -D warnings` were already clean, so there was nothing to change |

## Verification Results

Every exit code below was captured directly from the command, never through a pipe.

| Command | Exit | Result |
|---------|------|--------|
| `cargo test -p baude-core marker::tests::clear_owned -- --test-threads=1` (RED) | 101 | 35 compile errors naming the five missing identifiers |
| `cargo test -p baude-core marker::tests -- --test-threads=1` (GREEN) | 0 | 15 passed, 0 failed |
| `cargo test ... marker::tests && cargo fmt --check && cargo clippy -p baude-core --all-targets -- -D warnings` (Task 3) | 0 | 15 passed, no diff, no warnings |
| `grep -c 'fn clear_owned_' baude-core/src/marker.rs` | n/a | 6 |
| `grep -c 'pub fn clear_owned(dir: &Path, owner_common_dir: &\[u8\])'` | n/a | 1 |
| `grep -c 'pub enum MarkerOutcome\|pub enum DirectoryOutcome\|pub enum ClearRefusal\|pub struct MarkerClear'` | n/a | 4 |
| `sed -n '1,/#\[cfg(test)\]/p' baude-core/src/marker.rs \| grep -v '^\s*//' \| grep -c 'remove_dir_all'` | n/a | 0 |
| `grep -c 'ErrorKind::DirectoryNotEmpty'` | n/a | 1 |
| `scripts/check-commit-message.sh <msg>` for both commits | 0 | no nested parentheses |

## Must-haves Self-check

- Marker deleted only on `Valid` plus byte-equal owner; `Missing` is a no-op on the marker; foreign, `Invalid` and symlinked markers are left untouched and reported: yes, D1 to D3.
- Directory removed with non-recursive `remove_dir`; non-empty reported as `NotEmpty` with nothing recursive run: yes, D5 and the `remove_dir_all` grep.
- Symlinked directory refused before any read or removal: yes, D6.
- Missing directory, including the legacy `repository-` path, yields `{ Missing, Missing }` with no error: yes, D4.
- After a successful clear `read_marker` is `Missing` and `dir.exists()` is false, the state `check_collision` reads as no collision: yes, D1.
- Prohibitions: no `remove_dir_all` in the production region, foreign marker never deleted, no symlink followed: all kept, verified by the greps and tests named in the plan frontmatter.

## Files Created/Modified

- `baude-core/src/marker.rs` - `MARKER_FILE` constant used by `write_marker`, `read_marker` and `clear_owned`; `ClearRefusal`, `MarkerOutcome`, `DirectoryOutcome`, `MarkerClear`; `pub fn clear_owned`; test helper `owner_meta` and the six `clear_owned_*` tests.

## Decisions Made

- **Constant hoisted in GREEN.** The plan allowed either the literal or a shared `const`; writing `clear_owned` against `MARKER_FILE` from the start kept all three functions on one name and left Task 3 with nothing to do.
- **Refusal short-circuits before `remove_dir`.** A `Refused` marker returns `directory: Untouched` without attempting removal, so a refused directory can never be removed even if it happened to be empty apart from the marker.
- **Tests keep the literal file name.** Fixtures write and read `.baude-marker.json` by its real name so a typo in the constant would fail the tests rather than hide behind it.

## Deviations from Plan

None - plan executed exactly as written. The RED run also needed a `cargo fmt` pass before commit because the appended tests were hand-formatted; that is formatting of the plan's own test code, not a behaviour change.

## Issues Encountered

None. GREEN passed all 15 marker tests on the first run and clippy was clean without changes.

## User Setup Required

None.

## Known Stubs

None. Nothing calls `clear_owned` yet by design: plan 04 wires it into `clear_stage` with the directory derived from `SavedRepository::physical_key`.

## Threat Flags

None beyond the plan's register. T-19-07, T-19-08 and T-19-09 are each covered by a named test; no `Cargo.toml` change was made.

## Next Phase Readiness

- Plan 04 can call `marker::clear_owned(&dir, repo.observed_common_dir.as_bytes())` with `dir = git::worktrees_base().join(workspace).join(format!("repository-{}", physical_key))` and map `MarkerOutcome::Removed` to `ClearingKind::Marker`, `DirectoryOutcome::Removed` to `ClearingKind::RepositoryDir`, and `Refused` or `NotEmpty` to `ClearingFailure` entries.
- The full `scripts/gate.sh` run stays with plan 19-07; this plan ran the per-crate test, fmt and clippy commands only.

---
*Phase: 19-re-admission-suppression*
*Completed: 2026-11-04*

## Self-Check: PASSED

- Files present: baude-core/src/marker.rs, 19-03-SUMMARY.md
- Commits on HEAD: 471fc7f, 691438b
