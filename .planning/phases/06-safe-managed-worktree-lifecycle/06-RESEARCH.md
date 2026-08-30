# Phase 6: Safe Managed Worktree Lifecycle - Research

**Researched:** 2026-08-30
**Domain:** Rust orchestration over Git linked-worktree creation, retained agent sessions, fail-closed removal, and durable rollback
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

### Worktree Creation
- Create new named branches from the repository's resolved default branch/primary checkout, regardless of which child supplied repository context.
- Activate eligible existing local branches only; remote-only branches must first become explicit local branches outside this phase.
- If a branch is already checked out in a worktree belonging to the repository, register/focus that existing worktree instead of forcing another checkout.
- Allocate managed paths from the durable repository key plus a sanitized branch label and collision suffix; verify any candidate through Git inventory before reuse.

### Close and Reopen
- Closing retains the checkout child, branch, first-seen order, shell/archive settings, and conversation-resume metadata while setting active intent false.
- Reopening reconciles Git first, durably records active intent, then launches/resumes through the active backend.
- Externally moved or branch-changed retained children become unavailable and cannot launch until topology is explicitly reconciled or repaired.
- Repeated or concurrent reopen requests reserve by durable checkout key and focus/return one runtime.

### Safe Removal
- Removal is permitted only for a verified baude-managed linked worktree whose tracked, untracked, conflicted, submodule, lock, and Git-status state is conclusively clean and safe.
- After preflight and confirmation, stop the agent immediately before a second preflight and plain Git removal; failure retains or restores durable runtime intent and user context.
- Serialize create, activate, reopen, and remove mutations per repository, then rediscover/recheck immediately before mutation.
- Successful removal deletes checkout membership/runtime but retains the local branch as dormant and always preserves the repository parent.

### the agent's Discretion
- Exact typed error taxonomy, mutation reservation representation, and rollback mechanism may follow current core/App/Manager patterns.
- Exact managed path collision suffix format is discretionary if stable, filesystem-safe, and verified against Git before reuse.

### Deferred Ideas (OUT OF SCOPE)
- Dormant branch rendering and safe merged-branch deletion are Phase 7.
- Remote TUI API presentation and PWA actions are Phases 8 and 9.
- Remote-only tracking branch creation, branch deletion during worktree removal, force removal, and automatic cleanup remain out of scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| WORK-01 | User can create a valid named branch or activate an eligible existing local branch as a managed worktree from repository context. | Git-owned ref validation and exact local-ref classification; new branches start explicitly at the verified local default ref; existing local branches use plain `worktree add`; occupied branches reuse inventory records. [CITED: https://git-scm.com/docs/git-check-ref-format] [CITED: https://git-scm.com/docs/git-show-ref] [CITED: https://git-scm.com/docs/git-worktree] |
| WORK-02 | Baude refuses invalid refs, path collisions, and branches already checked out elsewhere without bypassing Git safeguards. | No `--force`/`-B`; candidate paths are checked against both the filesystem and fresh NUL-delimited Git inventory; Git occupancy remains the final safeguard. Same-repository occupied branches follow the locked reuse decision rather than attempting a duplicate checkout. [CITED: https://git-scm.com/docs/git-worktree] |
| WORK-03 | User can close a worktree session while retaining its checkout and hierarchy child for later reopening. | Save a full retained-session snapshot with `active_intent=false` before process teardown; rollback leaves the live runtime untouched when persistence has not committed. [VERIFIED: codebase `baude-core/src/repository.rs`, `baude-core/src/persist.rs`, `baude/src/app.rs`] |
| WORK-04 | User can reopen a retained main-checkout or worktree child in the active workspace backend. | Generalize Phase 5's checkout-key runtime association and reconcile-before-dispatch flow from primary-only to every retained checkout; persist active intent before resume/spawn. [VERIFIED: codebase `baude/src/app.rs`, `bauded/src/manager.rs`] |
| WORK-05 | User can remove a clean managed worktree through a distinct confirmed action without deleting its branch. | Two preflights, stop-between-preflights, plain `git worktree remove`, and postconditions proving the linked record/path disappeared while the exact local branch and repository parent remain. [CITED: https://git-scm.com/docs/git-worktree] [CITED: https://git-scm.com/docs/git-show-ref] |
| WORK-06 | Dirty, conflicted, locked, submodule-unsafe, or indeterminate worktree state blocks removal before the running session or persisted child is changed. | Result-valued porcelain-v2 status, recursive submodule status, topology/lock checks, unknown-output rejection, and failure-path runtime restoration. [CITED: https://git-scm.com/docs/git-status] [CITED: https://git-scm.com/docs/git-submodule] [CITED: https://git-scm.com/docs/git-worktree] |
</phase_requirements>

## Summary

Phase 6 should replace the remaining legacy worktree helpers with one core lifecycle contract and thin App/Manager runtime adapters. Phase 5 already supplies durable repository/checkout keys, strict persistence, canonical `RepositorySnapshot`, full-ref reconciliation, active intent, and checkout-key-to-runtime maps. The unsafe gaps are now concentrated and concrete: `create_worktree` trusts any existing directory and tries speculative branch creation from the caller's checkout; `is_dirty` converts every Git error into “clean”; the local remove path kills and forgets the runtime before checking safety; daemon create/remove still allocate duplicate children and can delete the repository parent; and retained state does not persist the runtime's conversation/session ID even though both supported CLIs accept targeted resume IDs. [VERIFIED: codebase `baude-core/src/git.rs:987-1023,1115-1124`, `baude-core/src/repository.rs:81-117`, `baude-core/src/backend/mod.rs:74-81`, `baude/src/app.rs:1061-1086,1622-1647,1723-1748`, `bauded/src/manager.rs:540-686,873-913`; local `claude --help`, `opencode --help`]

The lifecycle must separate **Git facts**, **durable intent**, and **runtime effects**. Git owns ref validity, local-ref existence, branch occupancy, worktree topology, status, submodule state, and removal. The repository aggregate owns baude management, stable identity, retained metadata, active intent, and parent preservation. App/Manager own PTY stop/focus/resume/spawn. Every operation reserves its repository, refreshes Git before mutation, and only then applies a typed transition; no UI-specific path may call Git helpers directly. [CITED: https://git-scm.com/docs/git-worktree] [VERIFIED: codebase `baude-core/src/repository.rs`, `baude/src/app.rs`, `bauded/src/manager.rs`]

Removal needs an explicit commit boundary. First preflight occurs while the runtime is untouched. After confirmation, retain durable active intent, stop the runtime, run the same preflight again, invoke **plain** `git worktree remove <path>`, and verify parent/branch/path postconditions. Any failure before Git removal restarts/resumes the stopped runtime and keeps the child. If Git has removed the worktree but the subsequent state save fails, do not recreate or recursively delete anything: retain the old durable context as an unavailable recovery record, surface a committed-topology/degraded-persistence result, and let later reconciliation repair state. [CITED: https://git-scm.com/docs/git-worktree] [VERIFIED: codebase committed/not-committed distinction in `baude-core/src/persist.rs:97-118,310-384`]

**Primary recommendation:** Add a typed `baude-core` lifecycle state machine plus result-valued Git operations, and make both App and Manager execute the same reserve → rediscover → validate → persist/stop/mutate → verify → commit-or-compensate sequence keyed by `RepositoryKey` and `CheckoutKey`. [VERIFIED: Phase 6 locked decisions; codebase Phase 5 seams]

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Ref classification and worktree topology | Core Git adapter (`baude-core::git`) | System Git CLI | Git is authoritative for valid refs, exact local refs, branch occupancy, linked-worktree registration, and non-force safety. [CITED: https://git-scm.com/docs/git-check-ref-format] [CITED: https://git-scm.com/docs/git-worktree] |
| Lifecycle decisions and typed outcomes | Core domain (`baude-core::lifecycle`) | Repository aggregate | Shared pure decisions prevent App and Manager from assigning different meaning to create/close/reopen/remove. [VERIFIED: codebase currently duplicates orchestration in `app.rs` and `manager.rs`] |
| Managed path allocation | Core lifecycle/Git adapter | Filesystem + Git inventory | Durable keys provide stable suffixes; filesystem and Git inventory independently detect collisions, including missing-but-registered worktrees. [CITED: https://git-scm.com/docs/git-worktree] |
| Durable intent and retained metadata | Repository aggregate + persistence | App/Manager owner | `SavedCheckout` already owns management, order, active intent, session settings, and health; atomic save reports whether replacement committed. [VERIFIED: codebase `repository.rs`, `persist.rs`] |
| Local PTY focus/stop/resume/spawn | Local App adapter | Active backend | App owns local session vector, focus, shell geometry, and backend spawn details. [VERIFIED: codebase `baude/src/app.rs`] |
| Daemon PTY focus/stop/resume/spawn | Daemon Manager adapter | Active backend | Manager owns daemon sessions and is already guarded by `Arc<Mutex<Manager>>`; new lifecycle semantics must not add Phase-8 APIs. [VERIFIED: codebase `bauded/src/manager.rs`, `bauded/src/api.rs`; Phase 6 boundary] |
| Confirmation/presentation | Existing local modal | Core preflight result | Confirmation is surface-specific, while eligibility and blockers remain shared typed data. Remote/PWA presentation is deferred. [VERIFIED: codebase `baude/src/ui.rs`; `06-CONTEXT.md`] |

## Standard Stack

### Core
| Library / Tool | Version | Purpose | Why Standard |
|----------------|---------|---------|--------------|
| Rust standard library | Rust 1.98.0; Edition 2021 | Typed enums, `HashMap` reservations, subprocess argv, path handling, rollback state | Existing workspace and installed toolchain cover the phase; no new crate is needed. [VERIFIED: local `rustc --version`; codebase `Cargo.toml`] |
| System Git CLI | 2.50.1 locally | Ref validation/existence, stable worktree inventory, porcelain status, submodule inspection, add/remove | Git owns worktree/ref/status semantics and documents machine formats and safeguards. [VERIFIED: local `git --version`] [CITED: https://git-scm.com/docs/git-worktree] [CITED: https://git-scm.com/docs/git-status] |
| `serde` | 1.0.228 locked | Existing durable lifecycle records and typed health/outcomes | Already backs strict repository state; no schema replacement is required. [VERIFIED: codebase `Cargo.lock`, `repository.rs`] |
| `serde_json` | 1.0.150 locked | Existing workspace/daemon state envelope | Phase 5 persistence is already versioned and atomic. [VERIFIED: codebase `Cargo.lock`, `persist.rs`] |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `anyhow` | 1.0.102 locked | Boundary context around typed lifecycle errors | App/Manager may add display context, but core blocker/commit-stage variants must remain inspectable. [VERIFIED: codebase `Cargo.lock`, current error style] |
| Existing backend abstraction | in-tree | Prepare cwd and produce active Claude/OpenCode resume/spawn plans | Use only after reconciliation and durable active intent. [VERIFIED: codebase `baude-core/src/backend`, `app.rs`, `manager.rs`] |
| Claude Code CLI | 2.1.251 locally | Active Claude backend and targeted conversation resume | Local help documents `--resume [session-id]`; use the retained opaque session ID, with `--continue` only when no ID was ever observed. [VERIFIED: local `claude --version`, `claude --help`] |
| OpenCode CLI | 1.18.25 locally | Active OpenCode backend and targeted session resume | Local help documents `--session <session-id>`; use the retained opaque session ID, with `--continue` only when no ID was ever observed. [VERIFIED: local `opencode --version`, `opencode --help`] |
| Cargo built-in test harness | Cargo 1.98.0 | Unit, real-Git, thread/race, and injected-failure tests | Existing project uses inline test modules and no external test framework. [VERIFIED: local `cargo --version`; codebase test modules] |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Explicit Git commands | Parse `.git/worktrees`, refs, index, and submodule layouts | Contradicts Git guidance and duplicates layout/config/locking behavior. Use Git commands. [CITED: https://git-scm.com/docs/git-worktree] [CITED: https://git-scm.com/docs/git-show-ref] |
| Plain non-force `worktree remove` | `--force` or recursive deletion | Force bypasses clean/submodule/lock safeguards; recursive deletion bypasses Git metadata and can discard ignored/user files. Both are prohibited. [CITED: https://git-scm.com/docs/git-worktree] |
| Per-repository reservation | Only rely on App's event loop or Manager's global mutex | Current serialization is an implementation accident and does not encode same-repository idempotence or future async behavior. Keep an explicit repository/checkout reservation. [VERIFIED: codebase `App` and `Manager` owner shapes; locked decision] |

**Installation:** None. Keep all manifests and `Cargo.lock` unchanged. [VERIFIED: current workspace provides every required primitive]

## Architecture Patterns

### System Architecture Diagram

```text
App modal / Manager compatibility action
  -> resolve stable RepositoryKey + optional CheckoutKey
  -> reserve repository mutation
       -> already reserved same checkout reopen ----> return/focus reserved runtime
       -> conflicting mutation ---------------------> typed Busy result
  -> fresh discover_repository(main/retained path)
       -> identity/topology unavailable ------------> persist health; no Git/runtime mutation
  -> shared lifecycle decision
       -> CREATE / ACTIVATE
            -> Git ref validation + exact local/remote classification
            -> branch already in inventory ---------> register/reuse existing checkout
            -> allocate repo-key/slug/key path
            -> collision check (inventory + disk)
            -> fresh rediscovery + explicit worktree add
            -> verify common-dir/path/full-ref
            -> save child + active intent
            -> focus/resume/spawn via active backend
       -> CLOSE
            -> snapshot runtime metadata into retained child
            -> save active_intent=false
            -> stop runtime; retain checkout and parent
       -> REOPEN
            -> fresh checkout reconciliation
            -> save active_intent=true
            -> focus live OR resume/spawn exactly one runtime
       -> REMOVE
            -> preflight #1 while runtime remains live
            -> request/consume explicit confirmation
            -> stop runtime, retaining active intent/context
            -> preflight #2
            -> plain git worktree remove <exact path>
            -> verify path/inventory absent + local branch/parent present
            -> delete child/runtime association, save; parent retained
            -> on pre-remove failure: resume runtime and retain child
            -> on post-remove save failure: retain unavailable recovery context
  -> release reservation on every return path
```

The create and remove endpoints remain explicit so no convenience behavior can infer a remote branch, default to the caller's `HEAD`, force occupancy, delete a branch, or remove a directory directly. [CITED: https://git-scm.com/docs/git-worktree] [VERIFIED: locked Phase 6 decisions]

### Recommended Project Structure
```text
baude-core/src/
├── lifecycle.rs    # shared request/plan/outcome/blocker types, reservations, state transitions
├── git.rs          # ref classification, managed allocation, status/submodule preflight, add/remove/postconditions
├── repository.rs   # retained checkout invariants and health causes
├── persist.rs      # existing atomic save + committed-stage reporting
└── lib.rs          # exports lifecycle module
baude/src/
└── app.rs          # local modal adapter; runtime callbacks; no direct legacy worktree calls
bauded/src/
├── manager.rs      # same lifecycle adapter for compatibility create/close/reopen/remove semantics
└── api.rs          # no Phase-8 hierarchy/API expansion; preserve compatibility projection
```

This structure keeps Git and lifecycle semantics UI-free while allowing App and Manager to retain different PTY/focus/persistence targets. [VERIFIED: current crate boundaries and Phase 6 boundary]

### Pattern 1: Exact Branch Request Classification
**What:** Validate the literal requested branch with `git check-ref-format --branch`, reject previous-checkout shorthand by requiring stdout to equal the input, construct the exact `refs/heads/<name>`, and classify it as `ExistingLocal`, `RemoteOnly`, or `New`. Use `show-ref --verify --quiet -- <full-ref>` for the local test and structured `for-each-ref` over `refs/remotes/*/<name>` only to reject remote-only requests. Distinguish missing exit status from command failure; failure is never “new.” [CITED: https://git-scm.com/docs/git-check-ref-format] [CITED: https://git-scm.com/docs/git-show-ref] [CITED: https://git-scm.com/docs/git-for-each-ref]

**When to use:** Every create/activate request before allocating keys, paths, child records, or runtimes.

**Command contract:**
```text
git -C <main> check-ref-format --branch <literal-name>
git -C <main> show-ref --verify --quiet -- refs/heads/<literal-name>
git -C <main> for-each-ref --format=%(refname)%00 refs/remotes/*/<literal-name>
```

Do not accept `@{-1}`: official docs state `--branch` expands previous-checkout syntax and may produce a commit rather than a branch. Equality with the literal input makes the durable branch identity explicit. [CITED: https://git-scm.com/docs/git-check-ref-format]

### Pattern 2: Explicit New vs Existing Add
**What:** For `New`, freshly resolve the repository default, verify the exact local default ref as a commit, capture its OID, then run `git worktree add -b <name> <path> <default-full-ref>`. For `ExistingLocal`, run `git worktree add <path> <full-local-ref>`. Never omit the commit-ish, never use `-B`, `--force`, or remote guessing. Git documents that omitted/inexact inputs can create from current `HEAD` or infer a unique remote branch; explicit forms avoid both behaviors. [CITED: https://git-scm.com/docs/git-worktree]

**When to use:** Only after a fresh inventory proves the branch is not already checked out and the candidate path is unused.

```rust
// Source: https://git-scm.com/docs/git-worktree
enum BranchActivation {
    New { name: String, full_ref: String, start_ref: String },
    ExistingLocal { name: String, full_ref: String },
}
```

For a branch already present in the same repository inventory, do not call `worktree add`: map its canonical path/full ref to an existing or newly registered child, preserve `managed_by_baude=false` for an external checkout, persist active intent, and focus/resume/spawn by checkout key. [VERIFIED: locked reuse decision; codebase inventory fields]

### Pattern 3: Collision-Safe Durable Allocation
**What:** Use a stable path such as `<worktrees-base>/<workspace>/repository-<repository-key>/<sanitized-branch>-<checkout-key>`. The checkout key is the collision suffix; the readable label is non-authoritative. Before add, refresh inventory and reject/advance a candidate if either `symlink_metadata` finds any filesystem entry or any Git record names that path, including a missing/prunable record. Persist the exact selected path only after Git postconditions pass. [VERIFIED: durable key model in `repository.rs`; discretion in `06-CONTEXT.md`] [CITED: https://git-scm.com/docs/git-worktree]

Sanitization must produce a non-empty bounded component and cannot establish uniqueness: `feature/a` and `feature-a` may sanitize identically, so uniqueness comes from `CheckoutKey`, not the label. [VERIFIED: current `sanitize` maps multiple inputs to `-`; locked collision requirement]

### Pattern 4: Result-Valued Removal Preflight
**What:** Return `Result<RemovalSafety, InspectionError>`, where `RemovalSafety::Blocked(Vec<RemovalBlocker>)` distinguishes ownership, main/unlinked topology, locked/prunable, tracked, untracked, ignored-untracked, conflict, submodule, and unknown/malformed status. An inspection command error is an explicit blocker/result, never clean. [CITED: https://git-scm.com/docs/git-status] [CITED: https://git-scm.com/docs/git-submodule]

**Recommended checks, in order:**
1. Fresh discovery from the retained path; exact common-dir/path/full branch; record is linked, baude-managed, not locked/prunable. [CITED: https://git-scm.com/docs/git-worktree]
2. `git --no-optional-locks -C <path> status --porcelain=v2 -z --untracked-files=all --ignore-submodules=none --ignored=matching`. Empty output alone is status-clean; parse `1`/`2` as tracked/submodule changes, `u` as conflict, `?` as untracked, `!` as ignored-untracked, and reject unknown record types or malformed NUL fields. Porcelain v2 exposes structured XY/submodule fields; `--ignore-submodules=none` overrides user/config ignores. [CITED: https://git-scm.com/docs/git-status]
3. `git -C <path> submodule status --recursive`. Empty output means no recorded submodule; any valid row blocks non-force removal, while `-`, `+`, and `U` additionally identify uninitialized, commit-mismatched, and conflicted state. Nonzero or malformed output is indeterminate. Git documents that worktrees containing submodules require force, which this phase forbids. [CITED: https://git-scm.com/docs/git-submodule] [CITED: https://git-scm.com/docs/git-worktree]

Ignored files deserve an explicit blocker even though plain Git 2.50.1 removed a test worktree containing ignored files: otherwise baude could report “clean” and delete user/build artifacts that status hides by default. [VERIFIED: local real-Git experiment on 2026-08-30] [CITED: https://git-scm.com/docs/git-status]

### Pattern 5: Double Preflight with Runtime Compensation
**What:** Preflight #1 must complete before confirmation and before any runtime/durable change. After confirmation, capture the runtime association and retained metadata, stop the agent without clearing durable active intent, then run the exact same preflight #2. If #2 or plain removal fails, restore/focus/resume one runtime from the retained child. Only successful postconditions authorize deleting child membership and clearing the runtime map. [VERIFIED: locked removal sequence; Phase 5 checkout-key runtime pattern]

Git's plain remove remains a third safety check: it refuses unclean worktrees, worktrees with submodules, the main worktree, and locked worktrees without force. Baude's checks provide typed explanations and cover ignored/indeterminate state; Git remains the final topology mutation authority. [CITED: https://git-scm.com/docs/git-worktree]

### Pattern 6: Shared Transition Plans, Surface-Specific Effects
**What:** Put requests, preconditions, transition plans, blocker/outcome enums, and aggregate mutation functions in core. A plan names effects (`SaveIntent`, `StopRuntime`, `GitAdd`, `GitRemove`, `Verify`, `SpawnOrFocus`) in a fixed order. App and Manager supply callbacks for persistence and runtime effects but cannot reorder or omit steps. [VERIFIED: current App/Manager duplication and locked shared-semantics requirement]

**Contract examples:**
```rust
// Source: project architecture derived from 06-CONTEXT.md and Phase 5 types.
enum LifecycleOutcome {
    Focused { checkout: CheckoutKey, runtime: u64 },
    Reopened { checkout: CheckoutKey, runtime: u64 },
    Created { checkout: CheckoutKey, runtime: Option<u64> },
    Closed { checkout: CheckoutKey },
    Removed { repository: RepositoryKey, branch_ref: String },
    Blocked(RemovalSafety),
    Busy { repository: RepositoryKey },
    TopologyCommittedStateDegraded { checkout: CheckoutKey },
}
```

Use table-driven contract tests that feed the same initial aggregate/Git facts/runtime facts to both adapters and assert the same transition/outcome. Presentation strings, local focus, daemon IDs, and state filenames may differ. [VERIFIED: App and Manager use different runtime/storage owners but the same core model]

### Pattern 7: Persist an Opaque Targeted Resume ID
**What:** Extend retained session state with an optional opaque conversation/session ID captured from `Session.meta.session_id` before close or remove-stop. Replace the backend's boolean resume input with `Fresh | ContinueLatest | ResumeId(String)`. Claude maps `ResumeId` to `--resume <id>` and OpenCode maps it to `--session <id>`; both local CLIs document these targeted forms. Use directory-scoped `--continue` only as compatibility fallback when no ID was ever observed, not as the primary reopen contract. [VERIFIED: codebase `ClaudeMeta.session_id`, current boolean `Backend::spawn_plan`; local `claude --help`, `opencode --help`]

**Why:** `RetainedSessionState` currently stores name/path/branch/shell/archive fields but no conversation ID. Closing a runtime and later using only “most recent in this directory” can resume a different conversation if another CLI session was created in that checkout. [VERIFIED: codebase `repository.rs:81-117`; local CLI help defines `--continue` as latest and targeted ID flags separately]

Add the optional field with an explicit compatibility default and round-trip test; strict malformed-state handling remains unchanged. The ID is workspace/backend-owned opaque data and must never be used as a filesystem path or treated as repository identity. Because `Pty::spawn` currently executes a composed string through the user's shell, do not concatenate the retained ID into that string: pass it as a `CommandBuilder` environment value and reference a fixed quoted variable (or extend the PTY plan to carry direct argv). [VERIFIED: codebase `repository.rs`, workspace-bound backend model, `pty.rs:31-63`]

### Pattern 8: Explicit Persistence Commit Boundary
**What:** Clone pre-action aggregate/runtime maps, but roll back in-memory state only when `SaveError::replacement_committed()` is false. Once replacement committed, memory must match the replacement even if directory sync reports an error. For a Git mutation followed by save failure, report which side committed; do not pretend the entire operation rolled back. [VERIFIED: codebase `persist::SaveError` and Manager failure tests]

**Removal-specific rule:** Before Git remove, old durable active intent is the recovery source. After Git remove and verified postconditions, attempt to save the child deletion. A pre-replacement save failure leaves disk context intact; keep/re-mark the child unavailable in memory and surface degraded persistence. Never recreate a removed worktree automatically because another process could have changed the branch/path, and never delete the parent. [VERIFIED: locked rollback/context decision; codebase reconciliation model]

### Anti-Patterns to Avoid
- **Legacy `create_worktree` fallback:** speculative `-b`, then existing-branch retry, based on whichever child supplied `repo`; it can create from the wrong `HEAD` and trusts a colliding directory. Replace all lifecycle callers, then remove/deprecate the helper. [VERIFIED: codebase `git.rs:999-1023`]
- **Boolean cleanliness:** `.unwrap_or(false)` turns Git failure into permission to delete. Every command start, exit, decode, and parse failure must be typed indeterminate. [VERIFIED: codebase `git.rs:1115-1119`]
- **Kill then inspect:** current local `r` path calls `remove_session` before `is_dirty`; this directly violates WORK-06. [VERIFIED: codebase `app.rs:1630-1647`]
- **Delete child/parent for close:** close changes active intent and runtime only. Manager's current `remove` deletes the child and any now-unused repository; route compatibility close through retained semantics. [VERIFIED: codebase `manager.rs:873-913`]
- **Latest-session-only reopen:** current backend plans accept only a boolean and use `--continue`; persist the observed opaque session ID and select the CLI's targeted resume form when available. [VERIFIED: codebase `Backend::spawn_plan`, Claude/OpenCode implementations; local CLI help]
- **Mark external reuse as managed:** an already-checked-out external worktree may be registered/focused but must not become removable by baude merely through activation. [VERIFIED: locked managed-only removal and reuse decisions]
- **String/shell Git commands:** use `Command` argv with exact full refs/paths and `--` where command syntax supports it; never interpolate user branch/path into a shell. [VERIFIED: established codebase Git style] [CITED: https://git-scm.com/docs/git-show-ref]
- **Automatic prune/repair/unlock/stash/reset/clean:** all mutate or discard topology/work outside the phase and are explicitly deferred/prohibited. [VERIFIED: requirements out-of-scope table; `06-CONTEXT.md`]

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Branch-name grammar | Regex/sanitizer as ref validation | `git check-ref-format --branch` plus literal-output check | Git owns branch grammar and previous-checkout shorthand behavior. [CITED: https://git-scm.com/docs/git-check-ref-format] |
| Exact local-ref existence | `.git/refs` filesystem scan | `git show-ref --verify --quiet -- <full-ref>` | Supports packed/alternate ref storage and exact paths. [CITED: https://git-scm.com/docs/git-show-ref] |
| Checked-out branch detection | Process scan or path naming | Fresh `worktree list --porcelain -z` inventory | Stable, config-independent, path-safe, and includes locked/prunable facts. [CITED: https://git-scm.com/docs/git-worktree] |
| Dirty/conflict/submodule detection | Recursive filesystem diff | Porcelain-v2 status + recursive submodule status | Covers index/worktree/untracked/conflict/submodule states with machine formats. [CITED: https://git-scm.com/docs/git-status] [CITED: https://git-scm.com/docs/git-submodule] |
| Worktree deletion | `remove_dir_all` | Plain `git worktree remove <exact-path>` | Git removes linked metadata and enforces main/dirty/submodule/lock safeguards. [CITED: https://git-scm.com/docs/git-worktree] |
| Cross-operation database transaction | New database/journal | Existing atomic aggregate + explicit Git/persistence commit-stage outcomes | Filesystem topology and JSON cannot be atomically committed together; truthful compensation is safer than fake rollback. [VERIFIED: codebase persistence contract; locked rollback decision] |
| Runtime uniqueness | Display name/cwd checks | Durable `CheckoutKey` association and reservation | Names can collide and paths can move; keys survive close/reopen. [VERIFIED: Phase 5 implementation in App/Manager] |

**Key insight:** “Safe” is not a boolean status check. It is a protocol in which every authority proves its own fact, every unknown blocks, and each irreversible boundary has an explicit compensating state. [VERIFIED: WORK-02/05/06 and locked decisions]

## Common Pitfalls

### Pitfall 1: `check-ref-format --branch` silently expands `@{-1}`
**What goes wrong:** A user request can resolve to previous checkout state instead of a literal durable branch name. [CITED: https://git-scm.com/docs/git-check-ref-format]  
**Why it happens:** `--branch` is intentionally porcelain-friendly and performs previous-checkout expansion. [CITED: https://git-scm.com/docs/git-check-ref-format]  
**How to avoid:** Require successful output to equal the literal input and store/use the constructed full `refs/heads/...` ref.  
**Warning signs:** The validated stdout is ignored or `@{-n}` appears in persisted branch fields.

### Pitfall 2: New branch starts from the selected child
**What goes wrong:** Creating from a feature child produces a branch with the wrong base. [VERIFIED: current legacy helper invokes Git with the caller-provided repo and omits start-point]  
**Why it happens:** `worktree add -b` defaults omitted commit-ish to `HEAD`. [CITED: https://git-scm.com/docs/git-worktree]  
**How to avoid:** Freshly resolve/verify the repository default from the main record and pass its exact full ref explicitly.  
**Warning signs:** Add commands have no final start-point argument.

### Pitfall 3: Missing path considered available
**What goes wrong:** A prunable or locked Git record owns a path even when `Path::exists()` is false; reuse/add can bypass Git's administrative ownership. [CITED: https://git-scm.com/docs/git-worktree]  
**How to avoid:** Check fresh inventory and filesystem independently before every add.  
**Warning signs:** Candidate selection only calls `exists()`.

### Pitfall 4: Status errors become clean
**What goes wrong:** Missing Git, permissions, index locks, malformed output, or a replaced path can authorize removal. [VERIFIED: current `is_dirty` behavior]  
**How to avoid:** Return typed `InspectionError`/`Unknown` and block on every non-success or parse anomaly.  
**Warning signs:** `bool`, `.ok()`, `.unwrap_or(false)`, or ignored exit status appears in preflight.

### Pitfall 5: Clean superproject hides submodule risk
**What goes wrong:** Config can hide submodule dirtiness, and even a clean worktree containing submodules requires force to remove. [CITED: https://git-scm.com/docs/git-status] [CITED: https://git-scm.com/docs/git-worktree]  
**How to avoid:** Override ignore settings and run recursive submodule status; any submodule blocks this non-force phase.  
**Warning signs:** Only default `git status --porcelain` is run.

### Pitfall 6: Ignored files are silently deleted
**What goes wrong:** Default status omits ignored paths, while plain Git 2.50.1 removed an otherwise clean test worktree containing ignored files. [VERIFIED: local real-Git experiment]  
**How to avoid:** Include `--ignored=matching` and classify `!` records as blockers.  
**Warning signs:** Empty default status is treated as preservation proof.

### Pitfall 7: First preflight is treated as authorization
**What goes wrong:** The running agent can modify files after confirmation but before removal. [VERIFIED: inherent ordering in locked double-preflight decision]  
**How to avoid:** Stop immediately before a fresh second preflight, then use non-force Git removal as the final safeguard.  
**Warning signs:** Preflight result is cached across confirmation/stop.

### Pitfall 8: Failure after stopping strands the user
**What goes wrong:** Second preflight or Git remove fails and the formerly live agent remains gone even though checkout/context were retained. [VERIFIED: locked rollback requirement]  
**How to avoid:** Keep active intent and retained metadata until success; compensate by resume/spawn/focus exactly one runtime.  
**Warning signs:** `active_intent=false` is persisted before second preflight or no compensation outcome exists.

### Pitfall 9: Pretending Git + JSON are one transaction
**What goes wrong:** Git succeeds, save fails, and code either reports full rollback or recursively manipulates topology trying to reconstruct it. [VERIFIED: separate Git/filesystem state and current persistence commit-stage API]  
**How to avoid:** Model `TopologyCommittedStateDegraded`; preserve old context as unavailable and require later reconciliation.  
**Warning signs:** One generic error variant after irreversible Git mutation.

### Pitfall 10: App and Manager drift again
**What goes wrong:** Local TUI preserves a child while daemon DELETE prunes child/parent, or one surface accepts a branch the other refuses. [VERIFIED: current divergent code paths]  
**How to avoid:** Share typed plans/transitions and run adapter contract vectors.  
**Warning signs:** Either adapter directly calls `create_worktree`, `is_dirty`, or `remove_worktree`.

## Code Examples

Verified patterns from official sources and current project contracts:

### Literal Ref Validation and Exact Existence
```rust
// Sources:
// https://git-scm.com/docs/git-check-ref-format
// https://git-scm.com/docs/git-show-ref
fn classify_branch(main: &Path, literal: &str) -> Result<BranchClass, RefError> {
    // Run: git -C main check-ref-format --branch literal
    // Require status=0, one UTF-8 line, and validated == literal.
    // Then construct full = format!("refs/heads/{literal}").
    // Run: git -C main show-ref --verify --quiet -- full.
    // 0 => existing local; documented missing status => inspect remote-only;
    // every other status/start/parse failure => RefError, never New.
    todo!()
}
```

### Result-Valued Safety
```rust
// Sources:
// https://git-scm.com/docs/git-status
// https://git-scm.com/docs/git-submodule
#[derive(Debug, Eq, PartialEq)]
enum RemovalBlocker {
    NotManaged,
    NotLinked,
    Locked,
    Prunable,
    TrackedChanges,
    UntrackedFiles,
    IgnoredFiles,
    Conflicts,
    SubmodulesPresent,
    UnknownStatus(String),
}

enum RemovalSafety {
    Safe(VerifiedRemovalTarget),
    Blocked(Vec<RemovalBlocker>),
}

fn inspect_removal(target: &SavedCheckout) -> Result<RemovalSafety, InspectionError> {
    // Fresh topology, porcelain-v2 -z, then recursive submodule status.
    // No command error or unknown record can produce Safe.
    todo!()
}
```

### Plain Remove and Postconditions
```rust
// Source: https://git-scm.com/docs/git-worktree
let output = Command::new("git")
    .arg("-C")
    .arg(&verified.main_worktree)
    .args(["worktree", "remove", "--"])
    .arg(&verified.path)
    .output()?;
if !output.status.success() {
    return Err(RemoveError::GitRefused { stderr: output.stderr });
}
// Rediscover from the parent and prove:
// - same common_dir and main worktree remain;
// - no inventory record names verified.path;
// - filesystem path is absent (do not delete it if recreated);
// - verified.branch_ref still exists (optionally same captured OID).
```

### Close Before Kill
```rust
// Source: Phase 6 locked close semantics and persist::SaveError contract.
let before = aggregate.clone();
snapshot_runtime_into_checkout(checkout, runtime);
checkout.session.resume_id = runtime.meta.session_id.clone();
checkout.active_intent = false;
match save_status(&aggregate) {
    Ok(()) => stop_and_forget_runtime(checkout.key),
    Err(error) if !error.replacement_committed() => {
        aggregate = before;
        return Err(CloseError::Persistence(error)); // runtime remains live
    }
    Err(error) => {
        // Replacement contains active_intent=false; keep memory aligned, stop,
        // and surface durability degradation rather than rolling memory back.
        stop_and_forget_runtime(checkout.key);
        return Err(CloseError::CommittedButNotDurable(error));
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Human/status-v1 string emptiness | Porcelain v2 `-z` with explicit submodule fields and record kinds | Porcelain v2 is current documented machine format | Enables typed tracked/untracked/conflict/submodule/unknown blockers. [CITED: https://git-scm.com/docs/git-status] |
| `worktree add` convenience inference | Explicit new/existing forms with exact full local/start refs | Current Git docs expose remote guessing and omitted-HEAD conveniences | Prevents remote-only activation and wrong-base creation. [CITED: https://git-scm.com/docs/git-worktree] |
| Directory existence as reuse | Stable porcelain inventory plus filesystem check and verified postconditions | Phase 5 introduced authoritative inventory | Handles aliases and missing-but-registered records safely. [VERIFIED: codebase Phase 5 `discover_repository`] |
| Session deletion as close | Retained checkout + inactive intent + no runtime | Phase 5 primary close established the intent model | Phase 6 generalizes it to main and managed branch children. [VERIFIED: `05-03-SUMMARY.md`] |
| Directory-latest `--continue` only | Persisted opaque session ID with targeted `--resume`/`--session`, latest fallback only when absent | Supported by installed Claude Code 2.1.251 and OpenCode 1.18.25 | Reopens the intended conversation rather than whichever one is newest in the checkout. [VERIFIED: local CLI help; current `ClaudeMeta.session_id`] |
| Boolean dirty check before delete | Two result-valued preflights, runtime compensation, plain remove, postconditions | Phase 6 design | Unknown/racy/locked/submodule state fails closed. [VERIFIED: WORK-05/06 and locked decisions] |
| App/Manager ad hoc lifecycle | Shared core transition plan with adapter effects | Phase 6 design | Same identity, safety, rollback, and branch semantics on both owners. [VERIFIED: Phase 6 integration decision] |

**Deprecated/outdated:** `git::create_worktree`, `git::is_dirty`, direct `git::remove_worktree` calls from UI/Manager orchestration, primary-only `ensure_primary` naming, and daemon `remove` pruning repository parents must not remain on managed lifecycle paths. [VERIFIED: current source and Phase 6 requirements]

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|

All implementation claims were verified against current source/local Git behavior or cited from official Git documentation; no training-only assumptions remain.

## Open Questions (RESOLVED)

1. **How should a save failure after verified Git removal be presented before Phase 7 can render unavailable/dormant children?**
   - What we know: Git topology and JSON cannot commit atomically; old durable state preserves user context but references a removed checkout. [VERIFIED: codebase persistence model and locked context-preservation requirement]
   - What's unclear: Phase 6 has only current message/modal surfaces; hierarchy recovery presentation is deferred. [VERIFIED: phase boundary]
   - **RESOLVED:** Return `TopologyCommittedStateDegraded`, retain an unavailable child/context in memory and disk when possible, set persistence dirty, and show one actionable local/daemon error. Do not recreate or delete anything automatically. Phase 7 may improve presentation but cannot weaken this recovery contract. [VERIFIED: established fail-visible persistence pattern]

2. **Should an externally created occupied worktree be called “managed” in existing compatibility APIs?**
   - What we know: it must be registered/focused, but removal is restricted to `managed_by_baude=true`. [VERIFIED: locked decisions]
   - What's unclear: current flat API has only `is_worktree`, not ownership wording. [VERIFIED: codebase `SessionInfo`]
   - **RESOLVED:** Reuse it with `managed_by_baude=false`; preserve flat `is_worktree=true` compatibility while typed lifecycle outcomes say `ReusedExternal`. Closing/reopening is allowed, but removal is disabled until a future explicit adoption feature. [VERIFIED: ERGO-02 is future scope]

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|-------------|-----------|---------|----------|
| Git CLI | Ref/topology/status/submodule integration and runtime | ✓ | 2.50.1 (Apple Git-155) | None; return typed unsupported/inspection error |
| Rust compiler | Implementation and tests | ✓ | 1.98.0 | Project CI toolchain |
| Cargo | Test/build | ✓ | 1.98.0 | Project CI |
| Claude Code | Claude-backend targeted reopen UAT | ✓ | 2.1.251 | Automated adapter tests assert command plan without launching real CLI |
| OpenCode | OpenCode-backend targeted reopen UAT | ✓ | 1.18.25 | Automated adapter tests assert command plan without launching real CLI |

Availability was probed locally on 2026-08-30. [VERIFIED: local command probes]

**Missing dependencies with no fallback:** None. [VERIFIED: local command probes]

**Missing dependencies with fallback:** None. [VERIFIED: local command probes]

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test harness via Cargo 1.98.0 [VERIFIED: local command probe] |
| Config file | none; workspace `Cargo.toml` and inline `#[cfg(test)]` modules [VERIFIED: codebase] |
| Quick run command | `cargo test -p baude-core git::tests::lifecycle -- --nocapture && cargo test -p baude-core lifecycle::tests -- --nocapture` |
| Full suite command | `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` [VERIFIED: codebase `.github/workflows/ci.yml`] |

The inherited core Git baseline passed 22 focused tests on 2026-08-30; Phase 5 reported 238 workspace tests after completion. [VERIFIED: local focused test run; `05-03-SUMMARY.md`]

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| WORK-01 | literal valid new branch starts at verified default; existing local activates; remote-only rejects; occupied same-repo record reuses | real-Git + core decision | `cargo test -p baude-core git::tests::lifecycle::branch_activation -- --nocapture` | ❌ Wave 0 |
| WORK-02 | invalid refs and shorthand reject; sanitizer/filesystem/stale-inventory collisions reject; no force/partial child/runtime | real-Git + failure injection | `cargo test -p baude-core git::tests::lifecycle::creation_safety -- --nocapture` | ❌ Wave 0 |
| WORK-03 | close snapshots all retained settings plus opaque resume ID, saves inactive intent before stop, retains child/parent, and leaves runtime live on precommit save failure | core + App/Manager adapter contract | `cargo test lifecycle_close` | ❌ Wave 0 |
| WORK-04 | retained main/worktree reconcile; moved/branch-changed/locked block; targeted backend resume; repeated/concurrent reopen returns one runtime | real-Git + thread race + adapter contract | `cargo test lifecycle_reopen` | ❌ Wave 0 |
| WORK-05 | confirmation path uses two preflights and plain remove; exact branch OID/ref and repository parent remain; child/runtime disappear only after postconditions | real-Git + orchestration | `cargo test lifecycle_remove_clean` | ❌ Wave 0 |
| WORK-06 | staged/unstaged/deleted/untracked/ignored/conflict/submodule/locked/prunable/malformed/command-failure cases block before state/runtime mutation | real-Git matrix + parser unit | `cargo test -p baude-core git::tests::lifecycle::removal_preflight -- --nocapture` | ❌ Wave 0 |

### Required Real-Git Matrix

| Domain | Cases | Required assertions |
|--------|-------|---------------------|
| Ref validation | `feature/x`, Unicode valid ref, leading dash, `.lock`, `..`, `@`, `@{-1}`, spaces/control chars | Only literal Git-valid branches pass; no state/path/runtime mutation on rejection. [CITED: https://git-scm.com/docs/git-check-ref-format] |
| Branch class | absent everywhere, existing local, remote-only, same name on multiple remotes, existing local already checked out in main/linked | New uses exact default OID; existing local is not reset; remote-only rejects; occupied record is reused without force. [CITED: https://git-scm.com/docs/git-worktree] |
| Base independence | request from primary, main on non-default, and another feature child | New branch OID always equals captured resolved local default OID, never caller `HEAD`. [VERIFIED: locked creation decision] |
| Managed paths | slash/sanitizer collisions, duplicate repo basenames, pre-existing file/dir/symlink, missing-but-registered prunable path, stable retry | Path includes repository/checkout identity; neither filesystem nor Git ownership is reused accidentally. [CITED: https://git-scm.com/docs/git-worktree] |
| Add failures | Git process start/nonzero, branch created then postcondition mismatch, save pre/post replacement failure, spawn failure | No duplicate runtime/child; newly created worktree compensated with plain remove when safe; branch retained; committed state is not rolled back in memory. [VERIFIED: existing persistence failure semantics] |
| Status | staged add/delete/rename, unstaged edit/delete, untracked file/dir, ignored file/dir, merge conflict, unusual filenames | Correct blocker; NUL parser handles paths; no false clean. [CITED: https://git-scm.com/docs/git-status] |
| Submodule | no submodule, initialized clean, initialized dirty/untracked, uninitialized (`-`), wrong commit (`+`), conflict (`U`), nested recursive | Any recorded submodule blocks non-force removal; malformed/nonzero is unknown. [CITED: https://git-scm.com/docs/git-submodule] [CITED: https://git-scm.com/docs/git-worktree] |
| Topology | main, managed linked, external linked, locked, prunable, detached, moved, branch-changed, replaced common-dir | Only exact available baude-managed linked target can become safe. [CITED: https://git-scm.com/docs/git-worktree] |
| Remove postconditions | clean managed removal, branch with unpushed commits, branch OID capture, parent with other children | Branch/ref remains, parent remains, only exact child/runtime membership is deleted. [VERIFIED: locked removal decisions] |

### Required Race Matrix

| Race | Injection point | Expected result |
|------|-----------------|-----------------|
| Two creates, same repository/different branch | after first reservation | Serialized per repository; second waits/Busy then rediscovers; both can succeed without colliding. [VERIFIED: locked serialization decision] |
| Two creates, same branch | after first classification | One creates/reuses; second rediscovery focuses the same checkout/runtime; no second add. [CITED: https://git-scm.com/docs/git-worktree] |
| Two reopens, same checkout | after active-intent save and before spawn | Checkout reservation returns/focuses one runtime; exactly one spawn attempt. [VERIFIED: locked reopen decision] |
| Create vs remove, same repository | before either Git mutation | Repository reservation serializes; loser rediscovers rather than using stale plan. [VERIFIED: locked serialization decision] |
| Agent dirties after preflight #1 | after confirmation, before stop/#2 | Second preflight blocks; retained intent/context remains and runtime is resumed. [VERIFIED: locked double-preflight decision] |
| External process dirties after #2 | immediately before Git remove | Plain Git remove refuses; runtime is resumed; child/parent remain. [CITED: https://git-scm.com/docs/git-worktree] |
| External lock/move/branch change | between either discovery and mutation | Fresh check or Git refusal blocks; never force/repair/prune. [CITED: https://git-scm.com/docs/git-worktree] |
| External path recreation | after successful remove before postcondition | Postcondition reports degraded/ambiguous state and never recursively deletes recreated path. [VERIFIED: fail-closed phase contract] |

### Required Failure/Consistency Matrix

| Stage | Injected failure | Memory / disk / runtime / Git expectation |
|-------|------------------|-------------------------------------------|
| Close save | write/sync/rename before replacement | Roll back active intent; runtime remains live; checkout/parent unchanged. [VERIFIED: `SaveError` contract] |
| Close save | directory-sync after replacement | Memory follows inactive replacement; stop runtime; surface persistence dirty. [VERIFIED: `replacement_committed()` contract] |
| Reopen reconcile | missing/moved/branch/lock error | Persist unavailable if possible; no active-intent flip or spawn. [VERIFIED: Phase 5 reconciliation pattern] |
| Reopen save | pre-replacement failure | Restore inactive intent; no spawn. [VERIFIED: Phase 5 save-before-spawn pattern] |
| Reopen spawn | backend preparation/PTY failure | Active intent and retained child remain durable for retry; no duplicate map entry. [VERIFIED: Phase 5 spawn-failure pattern] |
| Reopen target | retained ID present/absent for both backends | Claude uses `--resume <id>`, OpenCode uses `--session <id>`; only absent ID uses each backend's latest-session continue form. [VERIFIED: local CLI help] |
| Remove preflight #1 | every blocker/unknown | Runtime, child, parent, and active intent byte-for-byte unchanged. [VERIFIED: WORK-06] |
| Stop runtime | kill/teardown failure | No second preflight/Git remove; preserve/recover runtime and child. [VERIFIED: locked sequence] |
| Remove preflight #2 | dirty/topology/inspection failure | Resume/focus one runtime; preserve durable active intent and child. [VERIFIED: locked rollback decision] |
| Git remove | nonzero/process failure | Resume/focus runtime; preserve child/parent/branch; no direct deletion. [CITED: https://git-scm.com/docs/git-worktree] |
| Postcondition | inventory/path/branch/parent mismatch | Do not delete durable child; mark/surface unavailable/degraded; no automatic repair. [VERIFIED: fail-closed requirements] |
| Final state save | pre-replacement failure after Git success | Old durable context remains; in-memory unavailable recovery child; no runtime can launch missing path; parent/branch preserved. [VERIFIED: locked context preservation] |
| Final state save | post-replacement durability failure | Memory follows child-deleted replacement; report persistence dirty; parent/branch remain. [VERIFIED: `SaveError` contract] |
| Compensation resume | backend spawn failure | Retained active intent/context remain and actionable error reports both original and compensation failure. [VERIFIED: locked rollback intent] |

### App/Manager Shared-Semantics Contract Matrix

Run the same table vectors through core plus each adapter: new branch, existing local, occupied external, close live, reopen absent/live/exited, moved checkout, clean remove, each removal blocker, each save commit stage, spawn failure, and repeated operation. Assert identical `LifecycleOutcome`, aggregate delta, Git command plan, and no-force invariant; allow only runtime ID, focus mechanics, shell support, state filename, and displayed wording to differ. [VERIFIED: current owner-specific differences and locked shared semantics]

### Sampling Rate
- **Per task commit:** relevant focused core or adapter filter, with real-Git tests using unique temporary repositories. [VERIFIED: established Phase 5 test fixture pattern]
- **Per wave merge:** `cargo test`. [VERIFIED: CI workflow]
- **Phase gate:** `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` green on macOS and Ubuntu before `/gsd-verify-work`. [VERIFIED: `.github/workflows/ci.yml`]

### Wave 0 Gaps
- [ ] Add `baude-core/src/lifecycle.rs` test module with shared transition/reservation vectors; no module exists today. [VERIFIED: source glob]
- [ ] Extend the existing `GitFixture` in `git.rs` with local/remote branch, conflict, ignored-file, submodule, lock/prunable, and hookable command-stage helpers. [VERIFIED: current fixture covers admission/default/reconciliation only]
- [ ] Add a Git executor/failure seam around lifecycle commands so command-start, nonzero, malformed output, and between-stage races are deterministic rather than timing sleeps. [VERIFIED: current Git functions invoke `Command` directly]
- [ ] Add optional opaque resume ID compatibility/round-trip fixtures and backend command-plan tests for targeted/fallback resume. [VERIFIED: retained state currently lacks this field; local CLI help]
- [ ] Expose App checkout-key lifecycle test methods and injectable stop/save-stage/spawn hooks; current spawn failure seam exists, but close/remove and committed-save stages are not injectable. [VERIFIED: codebase `App` test fields]
- [ ] Extend Manager test persistence/runtime seams for close/reopen/remove compensation and run the shared contract vectors. [VERIFIED: current Manager has atomic failure injection but lifecycle remains session-ID based]
- [ ] Add an argv recorder/assertion proving lifecycle commands never contain `--force`, `-B`, `prune`, `repair`, `clean`, `reset`, `stash`, branch delete, or recursive filesystem deletion. [VERIFIED: phase prohibitions]

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | Local lifecycle and existing daemon compatibility internals add no authentication surface; remote API changes are Phase 8. [VERIFIED: phase boundary] |
| V3 Session Management | no (ASVS web-session sense) | Agent process lifecycle uses durable checkout keys and runtime reservations, not web auth sessions. [VERIFIED: phase scope] |
| V4 Access Control | yes (local destructive authorization) | Only `managed_by_baude` exact linked children pass remove preflight; external/main children are non-removable. [VERIFIED: locked removal decision] |
| V5 Input Validation | yes | Git validates literal branch refs; fresh inventory validates paths/topology; porcelain/submodule output is parsed fail-closed. [CITED: https://git-scm.com/docs/git-check-ref-format] [CITED: https://git-scm.com/docs/git-status] |
| V6 Cryptography | no | No credential, token, encryption, or signature feature changes. [VERIFIED: phase boundary] |

### Known Threat Patterns for Rust + Git CLI + Local Worktrees

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Option/command injection through branch/path | Tampering / Elevation | `Command` argv only, literal ref validation, exact full refs, and `--` before positional paths where supported; no shell interpolation. [CITED: https://git-scm.com/docs/git-check-ref-format] [CITED: https://git-scm.com/docs/git-show-ref] |
| Shell injection through persisted resume ID | Tampering / Elevation | Keep the ID opaque; carry it as a `CommandBuilder` environment value or direct argv, never concatenate it into the `-c` command string. [VERIFIED: codebase `Pty::spawn` shell execution and new targeted-resume requirement] |
| Path collision or stale registration targets another checkout | Spoofing / Tampering | Durable-key path, filesystem check, fresh stable inventory, and common-dir/path/full-ref postconditions. [CITED: https://git-scm.com/docs/git-worktree] |
| Dirty-check failure authorizes deletion | Tampering / Information loss | Result-valued checks; every command/parse unknown blocks; plain non-force Git remove. [CITED: https://git-scm.com/docs/git-status] [CITED: https://git-scm.com/docs/git-worktree] |
| TOCTOU between confirmation and removal | Tampering | Per-repository reservation, stop agent, second preflight, and Git's own non-force check; compensate on refusal. [VERIFIED: locked design] |
| External checkout is mislabeled as baude-managed | Elevation / Information loss | Reuse/register with `managed_by_baude=false`; management ownership is never inferred from `is_worktree`. [VERIFIED: Phase 5 migration rule and Phase 6 decisions] |
| Persistence failure loses context or duplicates runtime | Tampering / Repudiation | Save-before-spawn/stop, commit-stage-aware rollback, stable checkout map, and unavailable retained recovery records. [VERIFIED: codebase persistence contract] |
| Ignored files deleted by “clean” removal | Information disclosure/loss | Explicit ignored-path status check and blocker before plain remove. [VERIFIED: local Git behavior] [CITED: https://git-scm.com/docs/git-status] |
| Concurrent lifecycle operations use stale topology | Tampering / Denial of Service | Repository reservation plus rediscovery immediately before every mutation; repeated reopen keyed by checkout. [VERIFIED: locked serialization decision] |

## Sources

### Primary (HIGH confidence)
- https://git-scm.com/docs/git-worktree — add convenience/explicit semantics, duplicate branch safeguards, stable porcelain `-z`, lock/prunable facts, plain remove behavior, submodule/main/force restrictions, and post-removal topology.
- https://git-scm.com/docs/git-check-ref-format — branch grammar, stricter branch rules, and `@{-n}` expansion caveat.
- https://git-scm.com/docs/git-show-ref — exact full-ref verification, quiet scripting form, packed-ref-safe existence.
- https://git-scm.com/docs/git-for-each-ref — structured remote-ref enumeration and worktree-related ref fields.
- https://git-scm.com/docs/git-status — porcelain v1/v2 stability, NUL paths, XY/unmerged/untracked/ignored/submodule fields, config override, and `--no-optional-locks` guidance.
- https://git-scm.com/docs/git-submodule — recursive status and `-`/`+`/`U` state prefixes.
- Current source: `baude-core/src/{git,repository,persist,backend}.rs`, `baude/src/app.rs`, `bauded/src/{manager,api}.rs`, manifests, lockfile, CI workflow, Phase 5 summary, requirements, roadmap, and Phase 6 context. [VERIFIED: codebase reads/searches]
- Local Git 2.50.1 ignored-file removal experiment and focused 22-test core Git run on 2026-08-30. [VERIFIED: local command execution]

### Secondary (MEDIUM confidence)
- None.

### Tertiary (LOW confidence)
- None.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — exact installed/locked versions and current manifests were checked; no dependency is added.
- Architecture: HIGH — recommendations extend concrete Phase 5 seams and locked sequence/ownership decisions.
- Git creation/removal semantics: HIGH — checked against current official Git manuals and local Git behavior.
- Pitfalls: HIGH — each maps to current unsafe source, official Git behavior, or an explicit locked race/rollback requirement.
- Validation architecture: HIGH — existing real-Git fixtures/failure seams were inspected and the focused baseline was run.

**Research date:** 2026-08-30  
**Valid until:** 2026-09-29 (30 days; stable Git/Rust domain, but recheck if Phase 5 source or lifecycle decisions change)
