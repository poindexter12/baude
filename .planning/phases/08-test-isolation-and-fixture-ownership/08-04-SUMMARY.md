---
phase: 08-test-isolation-and-fixture-ownership
plan: 04
subsystem: worktree-hygiene
tags: [rust, leak-preview, evidence-model, fail-closed, read-only-scan, symlink-refusal, tdd]

# Dependency graph
requires:
  - phase: 08
    plan: 01
    provides: "git::real_worktrees_base() — the ungated production worktrees-root resolver this scanner deliberately targets"
  - phase: 08
    plan: 03
    provides: "Per-fixture workspace identity, so a scanner fixture cannot inherit a developer identity while resolving synthetic roots"
provides:
  - "baude-core::worktree_scan — production module (no test-support cfg) holding the whole leak-preview evidence model"
  - "Evidence enum (9 variants) with an exhaustive, wildcard-free blocking_role() table — the single place removal authorization can ever widen"
  - "Verdict::{Live, Indeterminate, Removable} where Removable carries a RemovalProof (workspaces_checked + ClearingSignal + observed evidence), not a bare flag"
  - "worktree_scan::classify(Vec<Evidence>) -> Verdict — the agreed predicate, pure, filesystem-free, unit-testable"
  - "worktree_scan::scan_at(&ScanRoots) -> Result<ScanReport, ScanError> — read-only three-level enumeration against injected roots"
  - "worktree_scan::scan() — production wrapper over git::real_worktrees_base() + persist::config_dir()"
  - "git::worktree_inventory(&Path) -> Result<Vec<WorktreeRecord>, RepositoryDiscoveryError> (pub(crate)) — git's canonicalized worktree inventory, askable about a path that is ABSENT from it"
  - "persist::load_named_at widened to pub(crate) — the strict non-locking state read plan 05 needs to honour the no-write contract"
  - "All scan types derive Serialize/Deserialize for plan 05's report transport"
affects: [08-05, 08-07]

actuals:
  tokens: 12376  # chars/4 over the realized diff: `git diff 68954e7..HEAD | wc -c` = 49505
  tasks: 3
  commits: 4  # MEASURED: `git rev-list --count 68954e78ff1efe7f440dc79886f7c9b2f44ded6c..HEAD` at SUMMARY write (excludes this docs commit)
plan_head_before: 68954e78ff1efe7f440dc79886f7c9b2f44ded6c

tech-stack:
  added: []
  patterns:
    - "Three-way fail-closed verdict with Indeterminate as the fall-through for every unmatched case, including the empty evidence list"
    - "Blockers split by ROLE — ProvesLive (ReferencedByState, ContainsCheckout) -> Live; PreventsConclusion (IsSymlink, StateUnreadable) -> Indeterminate — evaluated first and returning outright"
    - "Exhaustive wildcard-free match in blocking_role(): a future Evidence variant is a compile error, never a silent authorization widening"
    - "Removable carries a re-derivable RemovalProof so plan 05 can re-verify at prune time and compare against the scan-time proof"
    - "std::fs::symlink_metadata for every classification decision; a symlinked candidate is recorded and refused without ever resolving its target"
    - "Canonicalize the base once, then refuse any candidate whose resolved path leaves it (starts_with containment)"
    - "Round-trip key parse: format!(\"repository-{key}\") must equal the directory name, so repository-007 and repository-+7 are shape mismatches rather than accepted keys"
    - "Hand-written three-level read_dir walk instead of a generic recursive walker, so the shape filter bounds how much filesystem the tool ever touches"
    - "Read-only contract asserted by instrument, not by review: a before/after tree snapshot capturing path|file_type|len|mtime for both synthetic roots"

key-files:
  created:
    - baude-core/src/worktree_scan.rs
  modified:
    - baude-core/src/lib.rs
    - baude-core/src/git.rs
    - baude-core/src/persist.rs

key-decisions:
  - "The locked human predicate (decision B, recorded 2026-09-14 as option `as-proposed`) is implemented VERBATIM: Removable requires ShapeMatch AND NotReferencedByState AND (Empty OR GitDisownsIt); hard blockers are ReferencedByState / ContainsCheckout / IsSymlink / StateUnreadable; NoGitdir is recorded but deliberately inert. No re-prompt, no reinterpretation."
  - "Blockers are evaluated FIRST and return outright, so no quantity of clearing signals can override one. They are further split by role: ProvesLive yields Verdict::Live, PreventsConclusion yields Verdict::Indeterminate. Neither can ever reach Removable."
  - "blocking_role() matches every Evidence variant explicitly with NO wildcard arm. That is deliberate: authorization can only widen in that one table, and a future variant added without classifying it will refuse to compile."
  - "Removable carries RemovalProof { workspaces_checked, clearing: ClearingSignal, observed: Vec<Evidence> } rather than a boolean. Decision C requires prune to re-derive evidence and MATCH the scan-time proof; a bare flag gives it nothing to compare."
  - "git::worktree_inventory was extracted from discover_repository because discover_repository CANNOT answer the disownment question — it rejects an input absent from the inventory with SelectedWorktreeMissing, and that absence IS disownment. Without the extraction, GitDisownsIt would have been unreachable code."
  - "parse_worktree_porcelain was NOT widened to pub(crate). The plan said 'as needed'; worktree_inventory returns already-parsed WorktreeRecords, so direct access to the parser was unnecessary. Keeping it private preserves the narrower seam."
  - "persist::load_named_at was widened (not load_for_workspace_strict_at) because the latter calls hold_state_lock, which creates or opens a lock file and would violate the D-16 no-write contract on the developer's real data root."
  - "MAX_EMPTY_DEPTH = 16 bounds the empty-at-every-level recursion. The check also short-circuits on the first non-directory entry, so a deep live checkout costs one stat, not a full descent."
  - "A failed git lookup is recorded as NOTHING, never as disownment. Only a successful inventory query that genuinely omits the path produces GitDisownsIt — pinned by a_failed_git_lookup_is_not_a_disownment."

patterns-established:
  - "Pattern: an authorization predicate lives in one pure function over a data-only evidence list, so the rule that permits deletion is auditable without a filesystem"
  - "Pattern: blockers classified by ROLE (proves-live vs prevents-conclusion) rather than as one undifferentiated deny-list — the two produce different verdicts and the distinction is what keeps 'I could not tell' out of 'it is alive'"
  - "Pattern: a no-wildcard match is load-bearing safety, not style — the compiler becomes the reviewer for anyone adding evidence later"
  - "Pattern: prove a read-only contract with a before/after tree snapshot (path, type, len, mtime) rather than asserting it in a comment"

requirements-completed: []

coverage:
  - id: D1
    description: "A candidate whose only evidence is a matching path shape is Indeterminate, never Removable (D-14 corrected)"
    requirement: TISO-04
    verification:
      - kind: unit
        ref: "baude-core/src/worktree_scan.rs#worktree_scan::tests::verdict::shape_alone_is_indeterminate"
        status: pass
      - kind: unit
        ref: "baude-core/src/worktree_scan.rs#worktree_scan::tests::verdict::a_shape_match_without_the_state_check_is_indeterminate"
        status: pass
    human_judgment: false
  - id: D2
    description: "A candidate whose only additional evidence is a missing gitdir is Indeterminate — the literal TISO-04 prohibition"
    requirement: TISO-04
    verification:
      - kind: unit
        ref: "baude-core/src/worktree_scan.rs#worktree_scan::tests::verdict::shape_plus_a_missing_gitdir_is_indeterminate"
        status: pass
      - kind: unit
        ref: "baude-core/src/worktree_scan.rs#worktree_scan::tests::verdict::a_missing_gitdir_cannot_stand_in_for_the_clearing_signal"
        status: pass
    human_judgment: false
  - id: D3
    description: "A candidate that is a symlink, or reached through one, is refused outright and never classified Removable"
    requirement: TISO-04
    verification:
      - kind: unit
        ref: "baude-core/src/worktree_scan.rs#worktree_scan::tests::verdict::a_symlink_blocks_every_clearing_signal"
        status: pass
      - kind: unit
        ref: "baude-core/src/worktree_scan.rs#worktree_scan::tests::enumeration::a_symlink_candidate_is_recorded_and_never_removable (#[cfg(unix)])"
        status: pass
      - kind: unit
        ref: "baude-core/src/worktree_scan.rs#worktree_scan::tests::enumeration::a_symlinked_workspace_is_not_descended (#[cfg(unix)])"
        status: pass
    human_judgment: false
  - id: D4
    description: "Enumeration reads the worktrees root and writes nothing and removes nothing (D-16, threat T-08-14)"
    requirement: TISO-04
    verification:
      - kind: unit
        ref: "baude-core/src/worktree_scan.rs#worktree_scan::tests::enumeration::a_scan_leaves_both_roots_unchanged — before/after tree_snapshot over path|file_type|len|mtime for BOTH the worktrees base and the config dir"
        status: pass
      - kind: other
        ref: "grep of the classification path: no create_dir/create_dir_all/File::create/write/remove/hold_state_lock; load_named_at (non-locking) chosen over load_for_workspace_strict_at precisely because the latter locks"
        status: pass
    human_judgment: false
  - id: D5
    description: "Every verdict carries the evidence that produced it (threat T-08-15)"
    requirement: TISO-04
    verification:
      - kind: unit
        ref: "baude-core/src/worktree_scan.rs#worktree_scan::tests::verdict::removable_carries_the_evidence_that_cleared_it"
        status: pass
      - kind: other
        ref: "Verdict::Live { evidence } and Verdict::Indeterminate { evidence } both carry Vec<Evidence>; Removable carries RemovalProof — there is no evidence-free verdict constructor in the type"
        status: pass
    human_judgment: false
  - id: D6
    description: "A hard blocker overrides any number of clearing signals"
    requirement: TISO-04
    verification:
      - kind: unit
        ref: "baude-core/src/worktree_scan.rs#worktree_scan::tests::verdict::a_state_reference_blocks_every_clearing_signal"
        status: pass
      - kind: unit
        ref: "baude-core/src/worktree_scan.rs#worktree_scan::tests::verdict::a_contained_checkout_blocks_every_clearing_signal"
        status: pass
      - kind: unit
        ref: "baude-core/src/worktree_scan.rs#worktree_scan::tests::verdict::an_unreadable_state_file_blocks_every_clearing_signal"
        status: pass
    human_judgment: false
  - id: D7
    description: "An empty evidence list yields Indeterminate"
    requirement: TISO-04
    verification:
      - kind: unit
        ref: "baude-core/src/worktree_scan.rs#worktree_scan::tests::verdict::an_empty_evidence_list_is_indeterminate"
        status: pass
    human_judgment: false
  - id: D8
    description: "Enumeration returns exactly the second-level repository-<u64> directories and skips the rest, including an overflowing key (threat T-08-13)"
    requirement: TISO-04
    verification:
      - kind: unit
        ref: "baude-core/src/worktree_scan.rs#worktree_scan::tests::enumeration::returns_one_candidate_per_shaped_directory — fixture carries repository-0, repository-1, repository-18446744073709551616 (u64 overflow), repository-007, repository-, repo-3, a plain file, and a nested child"
        status: pass
      - kind: unit
        ref: "baude-core/src/worktree_scan.rs#worktree_scan::tests::enumeration::a_missing_base_yields_an_empty_report"
        status: pass
    human_judgment: false
  - id: D9
    description: "Git disownment is recorded only when git actually spoke; a failed lookup is not a disownment"
    requirement: TISO-04
    verification:
      - kind: unit
        ref: "baude-core/src/worktree_scan.rs#worktree_scan::tests::enumeration::git_disownment_is_recorded_only_when_git_actually_spoke"
        status: pass
      - kind: unit
        ref: "baude-core/src/worktree_scan.rs#worktree_scan::tests::enumeration::a_failed_git_lookup_is_not_a_disownment"
        status: pass
    human_judgment: false
  - id: D10
    description: "No scanned candidate can reach Removable in this plan, because plan 04 emits no NotReferencedByState"
    requirement: TISO-04
    verification:
      - kind: unit
        ref: "baude-core/src/worktree_scan.rs#worktree_scan::tests::enumeration::no_scanned_candidate_is_removable_without_state_evidence"
        status: pass
    human_judgment: false

# Metrics
duration: 70min
completed: 2026-09-15
status: complete
---

# Phase 08 Plan 04: Leak-Preview Enumeration and Evidence Model Summary

**`baude-core::worktree_scan` can now walk the real managed worktrees root read-only and classify every `repository-<u64>` candidate through a fail-closed three-way verdict whose `Removable` branch is gated by the exact human-agreed predicate — so the two signals that all 1433 live candidates satisfy, matching path shape and a missing gitdir, are structurally incapable of authorizing a deletion.**

## Performance

- **Duration:** ~70 min (first task commit through close-out; the `discover_repository` disownment investigation accounted for roughly 15 min of it)
- **Tasks:** 3 of 3 (task 1 was a pre-resolved decision checkpoint carrying no code)
- **Files created:** 1 (`baude-core/src/worktree_scan.rs`, 1123 lines)
- **Files modified:** 3 (`lib.rs`, `git.rs`, `persist.rs`) — exactly the plan's `files_modified` set, nothing beyond it
- **Commits:** 4 (measured: `git rev-list --count 68954e7..HEAD` — 2 RED + 2 GREEN; this docs commit is additional)

## Accomplishments

- **The agreed predicate is implemented verbatim and is auditable in one function.**
  `classify(Vec<Evidence>) -> Verdict` is pure, takes no filesystem, and reads as the rule it
  encodes: blockers first (returning outright), then `ShapeMatch` **and**
  `NotReferencedByState` **and** a clearing signal that is `Empty` or `GitDisownsIt`, then
  fall through to `Indeterminate`. Twelve unit tests state TISO-04's wording directly, in
  English test names, so a future auditor checks the rule without reading an implementation.
- **Authorization can widen in exactly one place, and the compiler guards it.**
  `Evidence::blocking_role()` matches all nine variants explicitly with **no wildcard arm**.
  Adding a tenth variant without classifying it is a build failure. The alternative — a
  `_ => None` catch-all — would silently treat every future signal as harmless, which is the
  precise mechanism by which a fail-closed predicate quietly becomes fail-open.
- **Blockers are split by role, because "it is alive" and "I could not tell" are different
  answers.** `ReferencedByState` and `ContainsCheckout` *prove liveness* → `Verdict::Live`.
  `IsSymlink` and `StateUnreadable` *prevent a conclusion* → `Verdict::Indeterminate`. Neither
  group can reach `Removable`, but collapsing them would let the report claim knowledge it does
  not have.
- **`Removable` carries a re-derivable proof, not a flag.** `RemovalProof` records the
  workspaces actually checked, which clearing signal fired (with the owning repository path when
  it was git disownment), and the full observed evidence list. Locked decision C requires prune
  to re-derive all evidence and *match* the scan-time proof; a boolean would give it nothing to
  compare against, and a newly-qualifying candidate would slip through as "still removable".
- **The read-only contract is instrumented, not asserted.** `a_scan_leaves_both_roots_unchanged`
  snapshots every entry under **both** synthetic roots as `path|file_type|len|mtime`, scans, and
  re-snapshots. That catches a lock file, a probe directory, and an mtime bump from an
  accidental open-for-write — the three ways D-16 would be violated without anyone noticing.
- **Symlinks are refused before their targets are ever resolved.** `candidate_evidence` calls
  `std::fs::symlink_metadata` first; a link records `IsSymlink` and returns immediately. A
  symlinked *workspace* directory is not descended at all. The base is canonicalized once and
  every resolved candidate must still `starts_with` it, so a link pointing outside the managed
  root cannot be classified on its target's properties while a later removal acts on the link.
- **Git can now be asked about a path it has disowned.** `discover_repository` structurally
  cannot answer that question (see *Deviations*); `git::worktree_inventory` was extracted so the
  scanner can query the canonicalized inventory directly and observe a genuine absence.

## Task Commits

| Task | Name | Commit | Files |
| ---- | ---- | ------ | ----- |
| 1 | Agree the removal-authorization predicate | — (pre-resolved checkpoint, `2d64765`) | none |
| 2 (RED) | Pin the removal-authorization predicate before implementing it | `8156792` | `baude-core/src/lib.rs`, `baude-core/src/worktree_scan.rs` |
| 2 (GREEN) | Implement the agreed removal-authorization predicate | `afcf97b` | `baude-core/src/worktree_scan.rs` |
| 3 (RED) | Pin read-only enumeration of the managed worktrees root | `1b5a84d` | `baude-core/src/worktree_scan.rs` |
| 3 (GREEN) | Enumerate and classify the managed worktrees root read-only | `5a89a0f` | `baude-core/src/worktree_scan.rs`, `baude-core/src/git.rs`, `baude-core/src/persist.rs` |

## Files Created/Modified

**Created**

- `baude-core/src/worktree_scan.rs` (+1123) — module docs explaining why shape and a missing
  gitdir each prove nothing, with the agreed predicate quoted inline. `ReferenceMatch`,
  `Evidence` (9 variants), `BlockingRole`, `ClearingSignal`, `RemovalProof`, `Verdict`, all
  deriving `Clone, Debug, Deserialize, Eq, PartialEq, Serialize` for plan 05's report transport.
  `classify()`. Scan surface: `ScanRoots`, `Candidate`, `ScanReport`, `ScanError` (with `Display`
  + `std::error::Error`), `scan()`, `scan_at()`. Helpers: `sorted_entries`, `repository_key`
  (round-trip parse), `candidate_evidence`, `contents_evidence`, `empty_at_every_level`
  (`MAX_EMPTY_DEPTH = 16`, short-circuits on the first non-directory), `gitdir_evidence`,
  `gitdir_holder`. Tests: `mod tests::verdict` (12) and `mod tests::enumeration` (10, two
  `#[cfg(unix)]`) with `ScanFixture`, `git_ok`, `git_repo`, `tree_snapshot`.

**Modified**

- `baude-core/src/lib.rs` (+1) — `pub mod worktree_scan;`.
- `baude-core/src/git.rs` (+47 −7) — `pub(crate) fn worktree_inventory(&Path) ->
  std::result::Result<Vec<WorktreeRecord>, RepositoryDiscoveryError>` extracted out of
  `discover_repository`, which now calls it. The doc comment records *why* the seam exists: the
  scanner must ask the inventory about a path that may be absent from it, and
  `discover_repository` rejects that case rather than answering.
- `baude-core/src/persist.rs` (+9 −1) — `load_named_at` widened from private to `pub(crate)`
  with a doc comment stating it is a strict **non-locking** read, that `crate::worktree_scan`
  needs it, and that `load_for_workspace_strict_at` calls `hold_state_lock` and would violate
  D-16.

## Verification

Every number below is from a run executed in this session with its exit code captured
**directly** — never through a pipe, which masks the real status.

| Gate | Command | Result |
| ---- | ------- | ------ |
| Task 2 RED | `cargo test -p baude-core --lib worktree_scan::tests::verdict` | exit 101 — **0 passed, 12 failed**, 263 filtered out |
| Task 2 GREEN | `cargo test -p baude-core --lib worktree_scan::tests::verdict` | exit 0 — **12 passed, 0 failed**, 263 filtered out |
| Task 3 RED | `cargo test -p baude-core --lib worktree_scan::tests::enumeration` | exit 101 — **0 passed, 10 failed**, 275 filtered out |
| Task 3 GREEN / plan verify | `cargo test -p baude-core --lib worktree_scan::` | exit 0 — **22 passed, 0 failed**, 263 filtered out |
| Compile | `cargo check --workspace --all-targets --locked` | exit 0 |
| Formatting | `cargo fmt --all -- --check` | exit 0 |
| Lint | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | exit 0 |

**Whole-crate context (not a plan gate):** `cargo test -p baude-core --lib` → exit 101,
**280 passed, 5 failed**. All five failures are the same `lifecycle::tests::*` containment
escapes documented in plan 08-03's summary and `deferred-items.md`, unchanged by this plan and
owned by 08-06. This plan's 22 tests are entirely additive to the 258 that passed at 08-03's
close (258 + 22 = 280).

Per the plan's `<verification>`, only this plan's synthetic scanner filters are gates here.
Broad regression belongs to plan 06 and end-of-phase integration. **No scanner test was run
against a historical developer directory** — every fixture root is under `std::env::temp_dir()`
with a pid+sequence suffix, and `scan()` (the only function that resolves a real root) is never
called from a test.

## TDD Gate Compliance

Task 1 is a decision checkpoint with no code and is exempt. Tasks 2 and 3 are both
behavior-adding and both ran a full RED → GREEN cycle.

| Task | RED commit | Measured RED | Evidence verdict | GREEN commit |
| ---- | ---------- | ------------ | ---------------- | ------------ |
| 2 | `8156792` | exit 101 — 0 passed, 12 failed | intentional RED (subject absent: `classify` was `todo!()`) | `afcf97b` |
| 3 | `1b5a84d` | exit 101 — 0 passed, 10 failed | intentional RED (subject absent: `scan_at` was `todo!()`) | `5a89a0f` |

Both REDs were intentional — the subject behavior was absent, not mutated in. Every one of the
12 verdict tests failed at `8156792` because `classify` had a `todo!()` body; every one of the
10 enumeration tests failed at `1b5a84d` because `scan_at` had one.

**Why the RED commits carry scaffolding.** A pure "call a function that does not exist" RED does
not compile, and a build failure emits no TAP-parseable failing test — there is nothing to
distinguish an intentional RED from a typo. The RED commits therefore carry the type
declarations and `todo!()`-bodied signatures so the test binary builds and every assertion fails
on *behavior*. Every behavioral line lives in the corresponding GREEN commit.

## Decisions Made

- **The locked predicate was implemented verbatim, with no reinterpretation.** Decision B was
  answered by the user on 2026-09-14 (`as-proposed`, recorded in `2d64765`). `Removable` requires
  `ShapeMatch` **and** `NotReferencedByState` **and** (`Empty` **or** `GitDisownsIt`);
  `ReferencedByState` / `ContainsCheckout` / `IsSymlink` / `StateUnreadable` always refuse;
  `NoGitdir` is recorded and inert. The module docs quote the predicate so the next reader sees
  the agreed rule next to the code that implements it.
- **Blockers evaluated first, returning outright.** Not "collect clearing signals, then subtract
  blockers" — the blocker scan runs before any clearing logic and returns. That ordering is what
  makes "a hard blocker overrides any number of clearing signals" a structural property rather
  than an emergent one, and three tests pin it.
- **No wildcard in `blocking_role()`.** The exhaustive match is deliberate, load-bearing safety.
  This is the only table in the codebase where removal authorization can widen, and the compiler
  is the reviewer for anyone who adds an `Evidence` variant later.
- **`RemovalProof` over a boolean.** Locked decision C requires prune-time re-derivation that
  *matches* the scan-time proof, refusing even a newly-*qualifying* candidate. That comparison
  needs a concrete payload: which workspaces were checked, which signal cleared it, and the full
  observed list.
- **`worktree_inventory` extracted rather than `discover_repository` reused.** See *Deviations*
  — this was a genuine blocker, not a preference.
- **`parse_worktree_porcelain` left private.** The plan authorized widening it "as needed";
  `worktree_inventory` returns already-parsed, already-canonicalized `WorktreeRecord`s, so the
  parser was never needed directly. The narrower seam is the smaller surface.
- **`load_named_at`, not `load_for_workspace_strict_at`.** The strict-workspace loader calls
  `hold_state_lock`, which creates or opens a lock file under the developer's real config
  directory. That is a write, and D-16 forbids it. `load_named_at` is a strict non-locking read.
- **`MAX_EMPTY_DEPTH = 16` with a first-non-directory short circuit.** A managed worktree tree is
  three levels; 16 is far beyond any legitimate shape while bounding a pathological or adversarial
  nesting. The short circuit means a live checkout costs one `symlink_metadata`, not a descent.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `discover_repository` structurally cannot detect disownment**

- **Found during:** Task 3 GREEN
- **Issue:** The plan's action says to "run `discover_repository` and the porcelain worktree
  listing to determine whether git still registers the path". It cannot answer that.
  `discover_repository` *requires* its input to be a member of git's worktree inventory and
  returns `RepositoryDiscoveryError::SelectedWorktreeMissing` when it is not — which is exactly
  the disowned case. Calling it as written would have made `Evidence::GitDisownsIt` unreachable
  code: every genuinely disowned candidate would surface as a discovery error and be recorded as
  nothing.
- **Investigation:** git's real behavior was verified in the session scratchpad with a `.git`-file
  orphan (`gitdir: <repo>/.git`): `git rev-parse --git-common-dir` returns rc=0 and resolves,
  while `git worktree list --porcelain` lists only the repository. So the inventory *can* be
  queried about an absent path — just not through `discover_repository`.
- **Fix:** Extracted `pub(crate) fn git::worktree_inventory(&Path) -> Result<Vec<WorktreeRecord>,
  RepositoryDiscoveryError>` — the canonicalized inventory query, main worktree first, exactly as
  `discover_repository` already built it. `discover_repository` now calls it (`let worktrees =
  worktree_inventory(&canonical_input)?;`), so its behavior is unchanged and the scanner gets a
  function that answers the absence question. The doc comment records why the seam exists so it
  is not "simplified" back later.
- **Files modified:** `baude-core/src/git.rs`
- **Commit:** `5a89a0f`

**2. [Rule 3 - Blocking] clippy `manual_contains` under `-D warnings`**

- **Found during:** Task 2 GREEN
- **Issue:** `evidence.iter().any(|signal| *signal == Evidence::ShapeMatch)` trips
  `clippy::manual_contains`, and the repo's CI gate is `-D warnings`.
- **Fix:** `evidence.contains(&Evidence::ShapeMatch)`. One `cargo fmt --check` failure in the same
  commit was fixed with `cargo fmt --all`.
- **Files modified:** `baude-core/src/worktree_scan.rs`
- **Commit:** `afcf97b`

### Planned-but-adjusted

**3. `parse_worktree_porcelain` was not widened**

The plan says to widen `parse_worktree_porcelain`, `discover_repository`, and
`persist::load_named_at` to `pub(crate)` "as needed". Only `load_named_at` was widened;
`discover_repository` was superseded by the `worktree_inventory` extraction above, and the
porcelain parser was never needed directly because `worktree_inventory` returns parsed records.
Recorded here so a reviewer comparing the plan's file list against the diff does not read the
absence as an omission.

## Observations Worth Recording

These are properties of the implemented predicate, not deviations — but both would look like
bugs to someone reading the code cold.

- **`GitDisownsIt` is effectively unreachable as a clearing signal in practice.** Any candidate
  that holds a gitdir necessarily contains entries, so `ContainsCheckout` fires, and the
  blockers-first rule makes it `Live` before the clearing clause is ever consulted. `Empty` is
  therefore the operative clearing signal for real data. This is the conservative failure
  direction the plan intends and the predicate is unchanged — but `git_disownment_is_recorded_
  only_when_git_actually_spoke` and `a_failed_git_lookup_is_not_a_disownment` exist so the code
  path stays correct if plan 05's state evidence or a future refinement makes it reachable.
- **No scanned candidate can be `Removable` after this plan.** The scanner emits no
  `NotReferencedByState` — the state cross-reference is plan 05's work — so the second conjunct of
  the predicate is never satisfied by a real scan. `no_scanned_candidate_is_removable_without_
  state_evidence` pins that explicitly, so if plan 05 accidentally lands a half-built state
  check, the test that currently documents an intentional gap becomes the test that catches a
  premature authorization.

## v2.2 Follow-Up: the empty-parent leak is a LIVE production defect

The plan requires this be recorded in the summary, and it is the single most important thing in
this document for anyone reading it later.

The 1286 empty `repository-<key>` directories are **not** historical residue from a fixed bug.
`ensure_default_worktree` and `activate_branch_with_post_add_hook` each create the
`repository-<key>` parent *before* invoking git, nothing removes it when the subsequent step
fails, and **no code anywhere in the codebase removes an empty parent**. The defect is live in
`main` today.

- **Fixing it is out of scope for this phase** and is recorded as a **v2.2 follow-up**.
- **The consequence that matters:** this scanner will keep finding new candidates after phase 8
  ships. **"Zero candidates" must never be treated as an exit criterion** for this phase, for the
  scanner's own acceptance, or for any future cleanup work. A count that drops to zero would mean
  the scanner stopped working, not that the leak stopped happening.

## Known Stubs

None. No `TODO`, `FIXME`, `todo!(...)`, or `unimplemented!(...)` survives in the delivered code —
the two `todo!()` bodies present at the RED commits (`8156792`, `1b5a84d`) are replaced in their
respective GREEN commits, which is the intended RED-scaffolding mechanism described under *TDD
Gate Compliance*. `#[allow(dead_code)]` is not used; the module has no callers yet but every item
is exercised by its own tests.

## Threat Flags

None new. All five of the plan's registered threats are implemented as dispositioned:

| Threat | Severity | Implemented as |
| ------ | -------- | -------------- |
| T-08-12 — `Removable` from weak evidence | critical | Three-way verdict, `Indeterminate` default, blockers-first, no-wildcard `blocking_role()`, predicate agreed before implementation |
| T-08-04 — symlinked candidate | high | `symlink_metadata` everywhere, `IsSymlink` a hard blocker returned before target resolution, canonicalized base + `starts_with` containment refusal |
| T-08-13 — key parse overflow | low | Round-trip `str::parse::<u64>()`; overflow and non-numeric names are shape mismatches that are skipped, never scan failures |
| T-08-14 — scan writing to the real root | high | Zero writes; non-locking `load_named_at`; before/after tree snapshot asserts it |
| T-08-15 — verdict with no recorded reason | medium | Every `Verdict` arm carries evidence; `Removable` carries `RemovalProof` |

No network endpoint, auth path, or trust-boundary schema was introduced. The new module reads a
filesystem root the codebase already owned; nothing acts on its output yet.

## Safety

**No file under any real user root was read, written, created, or removed during this execution,
and nothing real was pruned or deleted.**

- `scan()` — the only function that resolves a developer root — is **never called from a test**.
  Every filesystem test calls `scan_at(&ScanRoots)` with two synthetic roots under
  `std::env::temp_dir()`, named `baude-scan-test-<pid>-<seq>` from an `AtomicU64`, so concurrent
  fixtures cannot collide.
- Post-run check: `find ~/.local/share/baude ~/.config/baude -maxdepth 2 -newermt '-60 minutes'`
  returned nothing. `~/.config/baude` mtime is Sep 15 07:53, predating this session's first edit.
- No `/tmp/baude-scan-test-*` leftovers remain; `ScanFixture::drop` cleaned every root.
- **Historical leak cleanup stayed PREVIEW-ONLY.** Not one of the ~1433 real candidates was
  touched. The module contains no removal call of any kind — there is nothing in it that could
  delete a directory even if invoked against the real root.
- git stayed on SSH. No push, no branch switch, no PR, no `git stash`, no `git clean`. The
  untracked `.gsd/` and `.planning/milestone.lock` were left untouched and unstaged.

## User Setup Required

None. No package was installed and no external service configuration is required.

## Next Phase Readiness

- **Plan 08-05** (state cross-reference, prune re-verification, CLI) inherits a complete and
  tested evidence vocabulary. Its concrete first job is to emit `Evidence::NotReferencedByState
  { workspaces_checked }` — and `Evidence::ReferencedByState { workspace, repository_key,
  matched }` — from a complete config-directory inventory read through the now-`pub(crate)`
  `persist::load_named_at`. Until it does, `no_scanned_candidate_is_removable_without_state_
  evidence` holds, by design.
- **Locked decision C is pre-wired.** `RemovalProof` is exactly the payload prune must re-derive
  and compare; `ClearingSignal` carries the owning repository path so a git-disowned candidate
  routes to the existing verified-removal path. All scan types are `Serialize`/`Deserialize`, so
  the report crosses a process boundary intact.
- **Anyone adding an `Evidence` variant** must classify it in `blocking_role()`. The build fails
  otherwise. That is the intended experience, not an inconvenience.
- **Do not treat a falling candidate count as progress** until the v2.2 empty-parent defect above
  is fixed. New candidates will keep appearing.

## Requirements Bookkeeping

`requirements-completed` is deliberately **empty**, and REQUIREMENTS.md was **not** checked off.
TISO-04 is a phase-wide requirement that this plan advances substantially — the evidence model and
the read-only scan are its core — but plans 05 (state cross-reference, prune re-verification, CLI)
and 07 still have to land before it is satisfied. This matches the precedent set by plan 08-03 and
the standing STATE.md blocker: checking a requirement off at 4 of 8 plans would be a false
completion claim.

## Self-Check: PASSED

- `baude-core/src/worktree_scan.rs` exists on disk (1123 lines); `baude-core/src/lib.rs`,
  `baude-core/src/git.rs` and `baude-core/src/persist.rs` all exist and carry this plan's changes.
- All four task commits resolve in `git log`: `8156792`, `afcf97b`, `1b5a84d`, `5a89a0f`.
- `commits: 4` measured at SUMMARY write from the plan ledger
  `.git/gsd-plan-head-before-08-04`: `git rev-list --count
  68954e78ff1efe7f440dc79886f7c9b2f44ded6c..HEAD` → 4.
- `actuals.tokens: 12376` measured as `git diff 68954e7..HEAD | wc -c` = 49505, ÷ 4. The plan
  estimated 45000 against a `confidence: low` — the realized cost is ~27% of the estimate,
  reported unrounded.

---
*Phase: 08-test-isolation-and-fixture-ownership*
*Completed: 2026-09-15*
