---
phase: 05-durable-repository-admission
verified: 2026-08-30T19:25:59Z
status: human_needed
score: 4/5 must-haves verified
overrides_applied: 0
human_verification:
  - test: "Exercise successful repository admission and repeat admission with real Claude Code and OpenCode workspaces"
    expected: "Main, nested, symlink, and linked-worktree entry paths converge on one parent and one usable default-branch session; repeat open focuses the live session or resumes the retained exited session without duplication."
    why_human: "Source and stand-in tests verify dispatch and wiring, but the suite deliberately does not launch either real external coding-agent CLI."
  - test: "Inspect actionable unavailable-default feedback in the local TUI"
    expected: "The message names the local remote-HEAD/default-state problem, gives a practical recovery action, launches nothing, and remains readable for the user."
    why_human: "The exact message and no-launch branch are code-verified, but presentation and recovery clarity require a user-facing check."
---

# Phase 5: Durable Repository Admission Verification Report

**Phase Goal:** Users can admit a repository once and reliably return to a durable, Git-reconciled parent with one usable active-backend session on the resolved default branch, without baude mutating the existing checkout.
**Verified:** 2026-08-30T19:25:59Z
**Status:** human_needed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Roadmap truth | Status | Evidence |
|---|---|---|---|
| 1 | Main checkout, subdirectory, symlink, and linked-worktree opens converge on one canonical parent and focus/reopen one active-backend default session. | ? UNCERTAIN | Canonical identity is exercised by five real-Git tests in `git.rs:1288-1370`. All local routes call `admit_repository` (`app.rs:538-543`, `1778-1821`); stable checkout/runtime dispatch is wired at `598-797`. Successful real Claude Code/OpenCode focus and resume still require human execution. |
| 2 | A non-default main checkout remains untouched while a separate default worktree is reused or created, with no silent switch/fetch. | ✓ VERIFIED | `ensure_default_worktree` inventories main/linked worktrees before `git worktree add` (`git.rs:834-943`). The real-Git test at `1490-1541` compares main HEAD, status, tracked bytes, and index bytes before/after. Source inspection found no fetch/switch/network remote-discovery command on this path. |
| 3 | Unsafe or unresolved local default state produces actionable failure without guessing, switching, fetching, or launching. | ✓ VERIFIED | Typed unavailable cases and recovery text are defined at `git.rs:474-536`; resolution accepts only exact, commit-verified local remote symbolic HEADs at `609-768`. App persists unavailable health and returns before checkout/session ensure at `app.rs:633-650`. Four focused default-branch tests passed. |
| 4 | Repository/child ownership, managed state, branch, order, and session state survive independently per workspace, including migrated Claude/OpenCode sessions. | ✓ VERIFIED | Durable records contain all required fields (`repository.rs:94-127`); workspace-primary/fallback selection and migration are strict (`persist.rs:180-252`, `403-515`). Current round-trip, non-UTF-8 path, three legacy migration, and daemon migration tests passed. |
| 5 | Persisted intent is Git-reconciled before action; malformed state is surfaced, and successful state changes are atomic. | ✓ VERIFIED | Reconciliation compares common directory, canonical path, full branch, detached/locked/prunable facts (`git.rs:421-463`) and is called before App ensure/restart (`app.rs:739-857`, `2030-2039`) and daemon restore/restart (`manager.rs:334-445`, `1081-1111`). Strict loads and sibling-temp write/flush/sync/rename/directory-sync are implemented at `persist.rs:180-252`, `310-385`. Atomic and malformed-state tests passed. |

**Score:** 4/5 roadmap truths fully automated; the remaining truth is implemented and test-supported but needs live backend UAT.

### Required Artifacts

| Artifact | Expected | Status | Details |
|---|---|---|---|
| `baude-core/src/git.rs` | Canonical discovery, offline default resolution, verified default-worktree ensure | ✓ VERIFIED | Exists, substantive, public APIs present, used by App/Manager, and covered by 15 focused real-Git tests. |
| `baude-core/src/repository.rs` | Workspace-scoped durable aggregate | ✓ VERIFIED | Opaque keys, lossless paths, ownership/role/health/session records, validation, and allocation are substantive and consumed by persistence and runtime owners. |
| `baude-core/src/persist.rs` | Strict migration/load and atomic save | ✓ VERIFIED | Current/legacy/missing outcomes, typed blocking errors, one-source migration, lock, atomic replacement, and directory sync are wired to both App and Manager. |
| `baude-core/src/lib.rs` | Repository module export | ✓ VERIFIED | `pub mod repository` at line 16. |
| `baude/src/app.rs` | Local admission, dispatch, restore, close/reopen, route convergence | ✓ VERIFIED | Durable state and checkout-runtime mapping are owned by App; admission saves before spawn and every local route reaches it. |
| `bauded/src/manager.rs` | Strict daemon persistence consumer | ✓ VERIFIED | Handles all load outcomes, blocks after load errors, reconciles before restore/restart, and surfaces typed save failures. |
| `bauded/src/api.rs` | Persistence failure status projection introduced by review fix | ✓ VERIFIED | Typed `MutationError::Persistence` maps to HTTP 503; real atomic-failure API regression passed. |

### Key Link Verification

| From | To | Via | Status | Details |
|---|---|---|---|---|
| `discover_repository` | Git common-dir + porcelain-z inventory | argv-only `Command` and byte parsing | ✓ WIRED | `git.rs:96-357`; parser rejects malformed/partial records. |
| `resolve_default_branch` | local `refs/remotes/<remote>/HEAD` | exact prefix and commit verification | ✓ WIRED | `git.rs:609-768`; upstream remote then deduplicated `origin`. |
| `ensure_default_worktree` | discovered worktree inventory | main/linked reuse, explicit add, rediscovery | ✓ WIRED | `git.rs:834-943`. |
| `StateFile` | `RepositoryState` | serde envelope plus graph validation | ✓ WIRED | `persist.rs:16-30`, `repository.rs:231-351`. |
| workspace load/save | workspace filenames | primary-first fallback and workspace-primary save | ✓ WIRED | `persist.rs:180-252`, `649-684`. |
| `App::admit_repository` | Git + persistence + active backend | discover/resolve/ensure/reconcile/save then `add_session` | ✓ WIRED | `app.rs:598-797`; backend selected at spawn (`989-1025`), not persisted. |
| App/Manager restore | current Git topology | active-intent iteration and fresh reconciliation | ✓ WIRED | `app.rs:499-545`; `manager.rs:302-445`. |

### Data-Flow Trace (Level 4)

| Artifact | Data | Source | Produces real data | Status |
|---|---|---|---|---|
| `App.repository_state` | repositories/checkouts/active intent | strict workspace state load, legacy Git reconciliation, and live admission | Yes | ✓ FLOWING |
| `App.runtime_checkouts` | durable checkout → live session ID | successful active-backend spawn after durable save | Yes | ✓ FLOWING |
| `Manager.repository_state` | daemon durable session ownership | strict workspace load/migration and mutation saves | Yes | ✓ FLOWING |

## Requirements Coverage

| Requirement | Source plan | Status | Evidence |
|---|---|---|---|
| REPO-01 | 05-01 | ✓ SATISFIED | Canonical common-dir/main-record discovery plus main/nested/symlink/linked/same-basename tests. |
| REPO-02 | 05-03 | ? NEEDS HUMAN | Save-before-spawn and active-backend wiring are verified; real Claude/OpenCode usability is manual. |
| REPO-03 | 05-03 | ? NEEDS HUMAN | Stable primary key and focus/restart/spawn/idle dispatch are verified; real live focus/resume is manual. |
| REPO-04 | 05-01 | ✓ SATISFIED | Main/linked reuse and non-mutating separate worktree creation test. |
| REPO-06 | 05-01 | ✓ SATISFIED | Detached, unborn, absent, malformed, and dangling local metadata fail closed. |
| PERS-01 | 05-02 | ✓ SATISFIED | Validated aggregate and independent Claude/OpenCode current-schema round trips. |
| PERS-02 | 05-02 | ✓ SATISFIED | Local and daemon selected-source migration is idempotent, retains all fields, and ignores dormant fallback. |
| PERS-03 | 05-03 | ✓ SATISFIED | Fresh reconciliation before restore, primary ensure, manual restart, and daemon restart; stale topology persists unavailable. |
| PERS-04 | 05-02 | ✓ SATISFIED | Malformed/unsupported/I/O states block; pre-rename failures preserve bytes; successful rename and directory sync are checked. |

No Phase 5 requirement mapped in `REQUIREMENTS.md` is orphaned from the plans.

## Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|---|---|---|---|
| Canonical identity | `cargo test -p baude-core git::tests::admission_identity -- --nocapture` | 5 passed | ✓ PASS |
| Offline default resolution | `cargo test -p baude-core git::tests::default_branch -- --nocapture` | 4 passed | ✓ PASS |
| Non-destructive default worktree | `cargo test -p baude-core git::tests::default_worktree -- --nocapture` | 4 passed | ✓ PASS |
| Git reconciliation | `cargo test -p baude-core git::tests::reconciliation -- --nocapture` | 2 passed | ✓ PASS |
| Aggregate/current persistence | repository + current/non-UTF-8 filters | 3 passed | ✓ PASS |
| Migration | `cargo test -p baude-core persist::tests::legacy_migration -- --nocapture` | 3 passed | ✓ PASS |
| Atomic/fail-visible persistence | `cargo test -p baude-core persist::tests::atomic -- --nocapture` | 2 passed | ✓ PASS |
| App admission and route wiring | repository-admission + admission-routes filters | 9 passed | ✓ PASS |
| Daemon strict persistence/reconciliation | manager persistence + restore reconciliation filters | 3 passed | ✓ PASS |
| Typed API persistence failure | real atomic HTTP regression filter | 1 passed | ✓ PASS |
| Formatting | `cargo fmt --check` | exit 0 | ✓ PASS |
| Lints | `cargo clippy --all-targets -- -D warnings` | exit 0 | ✓ PASS |
| Full workspace | `cargo test` | 253 passed (20 App, 164 core, 69 daemon) | ✓ PASS |

### Probe Execution

No Phase 5 probe is declared in a plan/summary and no phase-specific `probe-*.sh` contract applies.

## Anti-Patterns and Disconfirmation Pass

| Finding | Severity | Impact |
|---|---|---|
| No `TBD`, `FIXME`, `XXX`, `TODO`, `HACK`, placeholder, or empty implementation was found in the modified Phase 5 Rust files. | ℹ️ Info | No blocker marker. |
| `admission_routes_share_one_local_entrypoint...` tests a pure routing predicate rather than invoking all UI routes. | ⚠️ Warning | The test alone is weaker than its name; manual source tracing confirms launch directory, Open/New, and clone completion all reach `admit_repository`. |
| Successful real-backend focus/restart is not exercised by automation; production App tests intentionally cover save and spawn failure using test seams. | ⚠️ Warning | Routed to human verification rather than accepted from summary claims. |
| Post-`git worktree add` rediscovery/verification failure has no dedicated rollback test. | ℹ️ Info | Git mutation is verified before durable admission, but an unusual post-create verification failure may leave a Git-registered worktree for manual recovery; it does not falsify a tested successful-state criterion. |

## Human Verification Required

### 1. Real Active-Backend Admission and Reopen

**Test:** In isolated Claude Code and OpenCode workspaces, open one repository through its main checkout, nested directory, symlink, and linked worktree; repeat while the primary is live, then after it exits.

**Expected:** One durable parent and one default-primary checkout/session exist per workspace. Repeats focus the live session or resume/restart the retained exited session. No duplicate parent, child, or process appears.

**Why human:** The external CLI process contract and interactive focus/resume behavior are deliberately not exercised by the Rust suite.

### 2. Unavailable-Default Message Quality

**Test:** Use a repository with missing/dangling local remote HEAD metadata and open it in the local TUI.

**Expected:** No session launches and no checkout/fetch occurs. The visible message identifies the local metadata problem and a practical recovery action.

**Why human:** Source verifies the branch and message text; presentation clarity and discoverability are user-facing qualities.

## Gaps Summary

No automated blocker was found. All source artifacts are substantive and wired, every assigned requirement has implementation evidence, requirement-specific tests pass, and the full CI triad is green. Final status remains `human_needed` because the roadmap's active-backend focus/reopen outcome depends on real Claude Code/OpenCode interaction and because error-message usability is visual/user-facing.

---

_Verified: 2026-08-30T19:25:59Z_
_Verifier: the agent (gsd-verifier)_
