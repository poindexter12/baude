---
phase: 19-re-admission-suppression
plan: 02
subsystem: suppression
tags: [rust, suppression, advisory-store, baude-core, tdd]

# Dependency graph
requires:
  - phase: 13-folder-workspace-memory
    provides: "breadcrumbs::folder_key, load_json and locked_merge_write, the advisory read-merge-write path every per-workspace store uses"
  - phase: 08-workspace-identity
    provides: "Workspace::state_file and workspace::active with the test override"
provides:
  - "suppression::is_suppressed(root, path) -> bool: the single consult helper, exact folder_key match, never errors"
  - "suppression::lift(root, path) -> io::Result<bool>: removes exactly one key, no write when the key or file is absent"
  - "suppression::suppress(root, path, common_dir, now_ms) -> io::Result<()> and drop_children(root, common_dir) -> io::Result<usize>"
  - "SuppressionFile, SuppressionEntry, SCHEMA_VERSION, FILE_BASE = suppressed-admissions, file_path(root)"
affects: [19-04, 19-05, 19-07, 21-daemon-removal]

# Actuals (#2632)
actuals:
  tokens: 3124
  tasks: 3
  commits: 2
plan_head_before: 530b27c0f4b09ee6274f67d8d77ef0a15c923023
plan_head_after: 81db4a2f818e4f871f9bcf0ed5f18caa2b22f712

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Per-workspace advisory store named through Workspace::state_file with an explicit root parameter so callers and tests choose the directory"
    - "Pre-read outside the merge lock so no-op lift and drop_children calls leave the file byte-identical or absent"

key-files:
  created:
    - baude-core/src/suppression.rs
  modified:
    - baude-core/src/lib.rs

key-decisions:
  - "File base name is suppressed-admissions so the on-disk file reads as suppressed-admissions-<workspace>.json beside breadcrumbs-<workspace>.json"
  - "Entries are kept until lifted or garbage-collected; nothing prunes an entry because its path stopped existing on disk"
  - "The shared current pre-read helper was written in the GREEN step, so Task 3 had no refactor to make"
  - "All five helpers plus the two structs are pub because removal.rs, baude app.rs and the Phase 21 daemon all call into the module"

patterns-established:
  - "Store tests pin a workspace identity with workspace::override_for_test over a literal Config and hold a TestRedirect for the test's lifetime"

requirements-completed: [READM-03, READM-04]

coverage:
  - id: D1
    description: "suppress then is_suppressed is true; the file parses with schema_version and one entry keyed by folder_key with the common dir key and timestamp; lift returns true then false with unchanged bytes; lift on an absent file leaves it absent"
    requirement: READM-03
    verification:
      - kind: unit
        ref: "cargo test -p baude-core suppression::tests::suppress_then_consult_then_lift_round_trip -- --test-threads=1"
        status: pass
    human_judgment: false
  - id: D2
    description: "lift removes only the exact key; a sibling under the same repository survives with identical fields"
    requirement: READM-04
    verification:
      - kind: unit
        ref: "cargo test -p baude-core suppression::tests::lift_removes_only_the_exact_key -- --test-threads=1"
        status: pass
    human_judgment: false
  - id: D3
    description: "drop_children removes every entry of one common dir, returns the count, leaves the other common dir's entry equal, and returns 0 without writing on a second call"
    requirement: READM-03
    verification:
      - kind: unit
        ref: "cargo test -p baude-core suppression::tests::drop_children_removes_entries_of_one_common_dir_only -- --test-threads=1"
        status: pass
    human_judgment: false
  - id: D4
    description: "A corrupt file reads as empty from is_suppressed, lift and drop_children, and the next suppress replaces it with a valid file holding only the new entry"
    requirement: READM-03
    verification:
      - kind: unit
        ref: "cargo test -p baude-core suppression::tests::corrupt_file_reads_as_empty_and_is_replaced_on_next_write -- --test-threads=1"
        status: pass
    human_judgment: false
  - id: D5
    description: "file_path is under root, a .json named through state_file with FILE_BASE and the workspace name, and differs from the breadcrumbs file name"
    requirement: READM-03
    verification:
      - kind: unit
        ref: "cargo test -p baude-core suppression::tests::file_is_named_per_workspace_beside_breadcrumbs -- --test-threads=1"
        status: pass
    human_judgment: false

# Metrics
duration: 20min
completed: 2026-11-04
status: complete
---

# Phase 19 Plan 02: Per-workspace suppression store Summary

**`baude-core::suppression` is a per-workspace advisory JSON store beside the breadcrumbs file, keyed by `folder_key`, with consult, lift, suppress and garbage-collect helpers that write only through `locked_merge_write` and read a corrupt or absent file as empty.**

## Performance

- **Duration:** about 20 min
- **Started:** 2026-11-04T17:50:00Z (approximate)
- **Completed:** 2026-11-04T18:10:21Z
- **Tasks:** 3 (RED, GREEN, REFACTOR with no refactor needed)
- **Files modified:** 2

## Accomplishments

- `suppressed-admissions-<workspace>.json` under the config root, with `schema_version` and serde defaults so a later field is additive; the schema-locked state file is not touched (D-09).
- `is_suppressed` is the one consult helper: exact `folder_key` match, no error path, so a tampered or corrupt file can only hide a worktree, never fail a launch or admit one (D-11, T-19-04, T-19-05).
- `lift` removes exactly the given key and writes nothing when the key or file is absent, so an untouched file stays byte-identical and an absent file stays absent (D-12).
- `suppress` records `{ repository_common_dir, suppressed_ms }` and `drop_children` retains everything not under the given common dir and returns the count, which is what plan 04 needs for child removal and parent garbage collection (D-10).
- Entries are kept until lifted or garbage-collected; nothing prunes by disk existence (RESEARCH open question 1, resolved at Claude's discretion).

## Task Commits

1. **Task 1 RED: failing suppression store tests** - `812343a` (test)
2. **Task 2 GREEN: per-workspace suppression store** - `81db4a2` (feat)
3. **Task 3 REFACTOR:** no commit; condition did not hold

**Plan metadata:** recorded in the final docs commit.

## TDD Gate Compliance

| Gate | Commit | Evidence |
|------|--------|----------|
| RED | `812343a` | `cargo test -p baude-core suppression::tests -- --test-threads=1` exit 101: E0422 `SuppressionEntry`, E0425 `drop_children`, `file_path`, `is_suppressed`, `lift`, `suppress`, `SuppressionFile`, `FILE_BASE`, `SCHEMA_VERSION` |
| GREEN | `81db4a2` | same command exit 0, `running 5 tests`, `test result: ok. 5 passed` |
| REFACTOR | skipped by design | `lift` and `drop_children` already shared the private `current(root)` pre-read; `cargo fmt --check` and clippy were clean, nothing to change |

## Verification Results

| Command | Exit | Result |
|---------|------|--------|
| `cargo test -p baude-core suppression::tests -- --test-threads=1` (RED) | 101 | compile errors naming the nine missing items |
| `cargo test -p baude-core suppression::tests -- --test-threads=1` (GREEN) | 0 | 5 passed, 0 failed |
| `cargo test ... && cargo fmt --check && cargo clippy -p baude-core --all-targets -- -D warnings` (Task 3) | 0 | 5 passed, no diff, no warnings |
| `grep -c '^pub mod suppression;' baude-core/src/lib.rs` | n/a | 1 |
| five `fn <test>` names grep | n/a | 5 |
| `pub fn is_suppressed(root: &Path, path: &Path) -> bool` / `pub fn lift(root: &Path, path: &Path)` / `pub fn suppress(\|pub fn drop_children(` greps | n/a | 1 / 1 / 2 |
| `grep -c 'locked_merge_write'` | n/a | 4 (import plus three writing helpers) |
| `sed -n '1,/#\[cfg(test)\]/p' ... \| grep -v '^\s*//' \| grep -c 'fs::write'` | n/a | 0 |
| `grep -v '^\s*//' baude-core/src/suppression.rs \| grep -c 'persist::'` | n/a | 1 (see Deviations); production section only: 0 |

## Must-haves Self-check

- Per-workspace advisory file beside breadcrumbs, named through `state_file(FILE_BASE)`, keyed by `folder_key`, `schema_version` plus serde defaults, state file untouched: yes, D1 and D5.
- `is_suppressed` single consult helper, true only on `folder_key(path)` present, false on absent or corrupt, never panics or errors: yes, D1 and D4; the signature returns `bool`.
- `lift` removes exactly that key, `Ok(true)`; `Ok(false)` without writing when the key or file is absent; other entries intact: yes, D1 and D2.
- `suppress` records `{ repository_common_dir, suppressed_ms }` via `locked_merge_write`; `drop_children` removes by `repository_common_dir` and returns the count: yes, D1 and D3.
- Entries kept until lifted or garbage-collected: yes, nothing in the module checks disk existence.

## Files Created/Modified

- `baude-core/src/suppression.rs` - new: module doc with the posture, `SCHEMA_VERSION`, `FILE_BASE`, `SuppressionFile`, `SuppressionEntry`, `file_path`, private `file_name` and `current`, `is_suppressed`, `lift`, `suppress`, `drop_children`, five unit tests.
- `baude-core/src/lib.rs` - `pub mod suppression;` after `session`, before `testing`.

## Decisions Made

- **`FILE_BASE = "suppressed-admissions"`**, naming what the file holds rather than the mechanism, and distinct from `breadcrumbs` by construction.
- **No disk-existence pruning.** Like `folder-workspaces.json`, an entry whose path is gone is legitimate history; it costs one map key and lifts on explicit admission.
- **Pre-read outside the merge lock** for `lift` and `drop_children`, accepting the same check-then-write gap plan 01 accepted for `folder_workspace::clear`; it is what keeps an absent file absent and a no-op call byte-identical.
- **`SuppressionEntry.suppressed_ms` carries `#[serde(default)]`** like `updated_ms` in the analog stores, so a hand-edited entry without it still parses instead of corrupting the whole file.

## Deviations from Plan

### Verification scope refinement

**1. [Verification] The prohibition grep `grep -v '^\s*//' baude-core/src/suppression.rs | grep -c 'persist::'` prints 1, not 0**
- **Found during:** Task 2 acceptance checks
- **Issue:** The test module imports `crate::persist::Config` to build the literal config that `workspace::override_for_test` needs to pin a workspace identity, which is the only way `workspace::active()` resolves under test. The production section of the file has no `persist::` reference, reads nothing but its own file through `load_json`, and never touches the state file, so the prohibition D-09 protects holds.
- **Fix:** None to the code; the verification command is refined to the production section: `sed -n '1,/#\[cfg(test)\]/p' baude-core/src/suppression.rs | grep -v '^\s*//' | grep -c 'persist::'` prints 0.
- **Files modified:** none

### Auto-fixed Issues

None.

---

**Total deviations:** 1 verification-scope note, 0 code changes
**Impact on plan:** None; all five behaviours and the three prohibitions hold.

## Issues Encountered

None. The GREEN implementation passed all five tests on its first run and clippy was clean without changes.

## User Setup Required

None.

## Known Stubs

None. Nothing calls the module yet by design: plan 04 wires `suppress` and `drop_children` into `clear_stage` and plan 05 wires `is_suppressed` and `lift` into the TUI admission path.

## Threat Flags

None beyond the plan's register. The new file is the surface T-19-04 to T-19-06 already name; no new endpoint, auth path or schema change at a trust boundary.

## Next Phase Readiness

- Plan 04 can call `suppress(root, observed_path, observed_common_dir, now_ms)` for a child removed under a surviving parent and `drop_children(root, observed_common_dir)` on parent removal, with `root = persist::config_dir()`.
- Plan 05 can call `lift(root, path)` before `admit_existing_worktrees` and `is_suppressed(root, path)` inside the sweep; Phase 21's daemon consults the same helper.
- The full `scripts/gate.sh` run stays with plan 19-07; this plan ran the per-crate test, fmt and clippy commands only.

---
*Phase: 19-re-admission-suppression*
*Completed: 2026-11-04*

## Self-Check: PASSED

- Files present: baude-core/src/suppression.rs, baude-core/src/lib.rs, 19-02-SUMMARY.md
- Commits on HEAD: 812343a, 81db4a2
