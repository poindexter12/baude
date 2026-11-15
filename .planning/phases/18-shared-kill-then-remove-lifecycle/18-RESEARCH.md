# Phase 18: Shared Kill-Then-Remove Lifecycle - Research

**Researched:** 2026-11-02
**Domain:** Rust process-group teardown, git worktree removal, atomic state persistence inside `baude-core` (Cargo workspace, no new dependencies)
**Confidence:** HIGH (every structural claim read from source this session with file:line; the only external claims are git documentation, cited)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### Kill Semantics
- The kill unit per row is the whole session: `Session::kill_and_wait` stops the agent PTY and the shell PTY. It already escalates SIGTERM to SIGKILL per exact process group and reaps the child (KILL-01, KILL-02).
- No new grace timer or config knob. Reuse the existing `wait_for_group_extinction` bound in `baude-core/src/pty.rs`.
- A runtime that survives SIGKILL returns a structured error carrying the surviving pids. The row and the state file are untouched; the caller surfaces the error (KILL-03).
- Parent removal is all-or-nothing at the kill stage: if any child's kill fails, abort before any disk or state change. Nothing is dropped and the error names the child.

#### Disk Removal Rules
- Managed worktree children are deleted after the kill with the force path (`git worktree remove --force`), bypassing the clean-tree preflight in `remove_verified_worktree`. The user asked for removal explicitly, so unclean trees and a runtime that held the tree as cwd do not block it (RMV-01).
- If `git worktree remove` fails and the path is inside baude's managed worktree root, fall back to `git worktree prune` plus deleting the directory. Never delete anything outside the managed root.
- Main checkout, unmanaged checkout and standalone rows drop the row and its runtime only. The directory and every local branch are untouched (RMV-02, RMV-03, RMV-07). Managed worktree branches are also never deleted.
- Baude's own markers and breadcrumbs inside a kept folder are left in place in this phase. Clearing ownership is Phase 19.
- Removing a row whose worktree directory is already gone succeeds as already removed. The row drops and the report records that no path was deleted.

#### Operation Shape and Atomicity
- One public entry point in `baude-core` takes a removal target (repository, checkout, worktree or standalone row) and returns a report listing rows removed, runtimes killed and paths deleted, or a typed error. Phase 21's REST response reuses this report (RMV-05).
- Stage order: kill every targeted runtime, then delete worktree directories, then one atomic state save for the whole row set through the existing temp-file-plus-rename path in `persist.rs` (RMV-06). No per-child persistence.
- The gap between disk delete and the state save is accepted: the prior state is intact in the file and the existing startup reconcile marks a missing worktree unavailable. No intent journal.
- A side-effect-free preview function computes the same report before acting, so the Phase 20 TUI and Phase 21 PWA confirmations describe exactly what will happen.

### Claude's Discretion
- Exact names of the target enum, report struct and error type.
- Test process doubles for live-runtime fixtures, as long as they spawn a real process group and tests assert its extinction on fixture output, per the repo's fixture-realism rule.
- Whether repository parent removal reuses the existing `RepositoryState` child removal helpers or adds a dedicated batch method, provided the single atomic save holds.

### Deferred Ideas (OUT OF SCOPE)
- Clearing folder-workspace bindings and marker ownership on removal: Phase 19.
- Any confirmation UI or REST endpoint: Phases 20 and 21.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description (from REQUIREMENTS.md traceability, Phase 18) | Research Support |
|----|-------------|------------------|
| KILL-01 | Removing a row with a live runtime terminates the owned process group (SIGTERM then SIGKILL) | `Pty::kill_and_wait` (pty.rs:744-791) already does identity-checked `kill(-pgid, SIGTERM)` then `SIGKILL`; `Session::kill_and_wait` (session.rs:478-512) composes agent + shell. Reuse directly. See Architecture Pattern 1. |
| KILL-02 | No zombie or orphan attributable to the row remains, on macOS and Linux | `kill_and_wait` reaps via `child.wait()` after group extinction (pty.rs:781-788); `wait_for_group_extinction` polls `kill(-pgid, 0)` 50 x 10ms (pty.rs:830-838). Existing `assert_reaped_promptly`/`is_zombie` test helpers (pty.rs:1341-1350) assert reaping on fixture output. `gate.sh --linux` covers Linux. |
| KILL-03 | A runtime that survives escalation leaves the row in place with a typed error, never drops the row | `SessionTeardownError` (session.rs:516-523) carries `agent_pid`, `shell_pid`, identities, `*_stopped` flags and `detail`. New op must call `Session::kill_and_wait` directly, NOT `lifecycle::destructive_teardown` (which mutates state via `mark_teardown_pending`, lifecycle.rs:987-1000). See Pitfall 1. |
| RMV-01 | Removing a managed worktree child deletes its directory even when unclean or held as cwd | Needs a new force path in `git.rs` beside `remove_verified_worktree` (git.rs:2710-2715) because `verify_removal_postconditions` and `topology_blockers` are private. `git worktree remove --force` semantics [CITED: git-scm.com/docs/git-worktree]. See Pattern 3. |
| RMV-02 | Removing a main checkout leaves directory and branches untouched | Role `CheckoutRole::Main` (repository.rs:284-288); op drops the row only. Reconcile asserts via fixture: dir exists, `git branch --list` unchanged. |
| RMV-03 | Removing a standalone row leaves the folder untouched | `RepositoryState::remove_standalone_session(StandaloneKey)` (repository.rs:870-879) already exists; op kills runtime then drops row. |
| RMV-04 | Removing a repository parent removes each child by per-child rules, then the parent | No existing batch removal; `RepositoryState` has only `remove_standalone_session`. Add a batch `remove_rows` on `RepositoryState` (discretion granted). `validate()` enforces `DanglingRepositoryKey` (repository.rs:940-944) so child-without-parent is impossible in a saved file. |
| RMV-05 | One lifecycle operation composed from existing kill, close and managed-worktree removal stages; report reusable by REST | Report struct derives `Serialize`/`Deserialize` (serde already a dep, baude-core/Cargo.toml:24). Compose from `Session::kill_and_wait`, new `git::remove_managed_worktree_forced`, `persist::save_*_status`. |
| RMV-06 | Persist atomically: one save for the whole row set | `atomic_save_current` (persist.rs:640-706): temp file `create_new`, `sync_all`, `rename`, parent dir `sync_all`; `SaveError::replacement_committed()` (persist.rs:278-280) distinguishes pre/post-rename failure. Callers: TUI `save_durable_status` (app.rs:1946-1958), daemon `save_for_workspace_status` (manager.rs:722-728). |
| RMV-07 | Never delete a main checkout, standalone folder or local branch; tests on cloned and `git init` fixtures | `git worktree remove` does not touch `refs/heads` [CITED: git-scm.com/docs/git-worktree]; existing `cloned_topology` (lifecycle.rs:4290-4325) and `git init -b main` fixtures (lifecycle.rs:3271 etc.) are the shapes to reuse. |
</phase_requirements>

## Summary

Every building block this phase needs already exists in `baude-core` and was read this session: an identity-verified, group-scoped SIGTERM-then-SIGKILL kill with reaping (`Pty::kill_and_wait`), a two-process composition returning a pid-carrying error (`Session::kill_and_wait` / `SessionTeardownError`), a verified non-force worktree remover with postcondition checks (`git::remove_verified_worktree`), a crash-safe temp-plus-rename state writer (`persist::atomic_save_current`), a validated schema-v3 aggregate with parent/child linkage (`RepositoryState`), and a startup reconcile that already turns a missing worktree directory into `Protected(Missing)`. The work of Phase 18 is composition: one new module (recommend `baude-core/src/removal.rs`) that selects rows for a target, kills every owned runtime, force-removes managed worktree directories, mutates a cloned `RepositoryState`, saves it once, and commits the clone only on success.

Three findings change how the planner should shape tasks. First, two helpers the CONTEXT names as reusable are private to `git.rs` (`remove_verified_worktree_with_post_remove_hook`, `verify_removal_postconditions`, `topology_blockers`), so the force path must be added inside `git.rs` rather than in the new module. Second, the existing TUI removal flow (app.rs:3790-3899) is a two-save protocol (persist a `RemovalTombstone` before Git mutation, then save again after) and the existing kill boundary `lifecycle::destructive_teardown` writes `TeardownPending` into state on failure; both contradict the locked "one save, state untouched on kill failure" decisions, so the new operation must call `Session::kill_and_wait` directly and must not reuse `revoke_removal_authority`/`destructive_teardown`. Third, `RepositoryState::validate()` rejects a child without its parent (`DanglingRepositoryKey`) but permits a parent with zero children; the SC5 crash-consistency guarantee therefore rests entirely on the single save, not on validation, which is exactly what the locked stage order provides.

**Primary recommendation:** Add `baude-core/src/removal.rs` exposing `preview_removal(&RepositoryState, RemovalTarget) -> Result<RemovalReport, RemovalError>` and `remove(&mut RepositoryState, RemovalTarget, &mut [RowRuntime], save: impl FnOnce(&StateFile) -> Result<(), SaveError>) -> Result<RemovalReport, RemovalError>`; add `git::remove_managed_worktree_forced` beside `remove_verified_worktree`; add `RepositoryState::remove_rows`; test with real `Pty::spawn` children that trap `TERM` so escalation is exercised, and with `Pty::fail_next_teardown_for_test` for the survivor path.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Select rows for a removal target (parent fan-out, child kinds) | `baude-core` (domain: `repository.rs` + new `removal.rs`) | — | Parent/child linkage is `SavedCheckout.repository_key` (repository.rs:350); only core knows the aggregate. |
| Kill owned runtime (SIGTERM/SIGKILL, reap) | `baude-core::pty` / `session` | — | Already the single stop boundary; the TUI and daemon hold `Session` values but never signal directly. |
| Force-remove managed worktree directory, prune fallback, managed-root containment | `baude-core::git` | — | Private postcondition helpers live there; `worktrees_base()` containment is `pub(crate)` (git.rs:1787-1797). |
| Atomic persistence of the removed row set | `baude-core::persist` | caller supplies the save closure | TUI and daemon pick different save entry points with test hooks (app.rs:1946, manager.rs:722); core must not hard-code one. |
| Preview (side-effect-free report) | `baude-core::removal` | TUI (Phase 20), PWA (Phase 21) render it | Same selection logic as the real run; no process, disk or state effect. |
| Serializable report / typed error | `baude-core::removal` | `bauded` REST (Phase 21) | serde derive; REST reuses the struct verbatim (RMV-05). |
| Concurrency guard per repository | `baude-core::lifecycle::RepositoryReservations` (lifecycle.rs:2828-2855) | TUI/daemon call sites | Existing reservation is taken by callers (app.rs:3704-3708); keep that at the caller, document it. |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `libc` | 0.2 (already in baude-core/Cargo.toml:22) | `kill(-pgid, sig)`, `ESRCH`/`EPERM` probes | Already used by `signal_group`/`process_group_exists` (pty.rs:812-828). [VERIFIED: baude-core/Cargo.toml:22] |
| `portable-pty` | 0.8 (Cargo.toml:23) | PTY child spawn and `Child::try_wait`/`wait` reaping | Already the child handle `kill_and_wait` reaps through. [VERIFIED: baude-core/Cargo.toml:23] |
| `serde` + `serde_json` | 1 (workspace Cargo.toml:13-14) | `Serialize`/`Deserialize` on the report and target so REST can reuse them | Every persisted/row type already derives both. [VERIFIED: Cargo.toml:13-14] |
| `anyhow` | 1 (Cargo.toml:11) | Wrapping io errors inside typed enums where the codebase already does | Existing `SaveError.source: anyhow::Error` (persist.rs:254-257). [VERIFIED: Cargo.toml:11] |

### Supporting
None new. The phase introduces no dependency.

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Direct `git worktree remove --force` via `std::process::Command` (existing pattern, git.rs:2686-2700) | `git2`/`gix` crates | Rejected: no git library is in the tree; the codebase runs the git binary everywhere and its postcondition checks parse porcelain. Adding a crate violates "compose from existing stages". |
| New `removal.rs` module | Growing `lifecycle.rs` (already 5614 lines) | `lifecycle.rs` is dominated by the single-checkout removal protocol with tombstones; a new module keeps the one-save protocol from being confused with the two-save one. |

**Installation:** none (`cargo build` unchanged).

## Package Legitimacy Audit

No external packages are added by this phase. All referenced crates (`libc`, `portable-pty`, `serde`, `serde_json`, `anyhow`) are already declared in `baude-core/Cargo.toml` and resolved in `Cargo.lock`. Legitimacy gate not applicable.

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

## Architecture Patterns

### System Architecture Diagram

```
 RemovalTarget ──────────────┐
 (Repository | Checkout |    │
  Standalone)                ▼
                   ┌─────────────────────┐
                   │ select_rows(&state) │  pure: parent → every child (by repository_key)
                   │  → RemovalSet       │        checkout → that row; standalone → that row
                   └─────────┬───────────┘
                             │  (preview_removal stops here and renders the report)
                             ▼
           ┌──────────────────────────────────────┐
  Stage 1  │ for each row with a live runtime:    │  Session::kill_and_wait
           │   SIGTERM group → wait ≤500ms →      │  (pty.rs:773-780, 50×10ms)
           │   SIGKILL group → wait ≤500ms → reap │
           └───────┬──────────────────────┬───────┘
                   │ all extinct          │ any SessionTeardownError
                   ▼                      ▼
                                  Err(RemovalError::RuntimeSurvived{row, pids,..})
                                  ── NO disk change, NO state change, NO save ──
           ┌──────────────────────────────────────┐
  Stage 2  │ for each managed worktree child:     │  git::remove_managed_worktree_forced
           │   path missing? → record "already    │   (new, in git.rs)
           │   removed"                           │
           │   else `git -C <main> worktree       │
           │   remove --force -- <path>`          │
           │   on refusal AND path ⊂ managed root:│
           │     fs::remove_dir_all + worktree    │
           │     prune; else Err(DiskRemoval)     │
           └───────┬──────────────────────────────┘
                   ▼
           ┌──────────────────────────────────────┐
  Stage 3  │ next = state.clone()                 │
           │ next.remove_rows(&set); next.validate│
           │ save(&StateFile{SCHEMA_VERSION,next})│  persist::atomic_save_current
           │  Ok → *state = next; Ok(report)      │   (temp create_new → write → fsync →
           │  Err(!replacement_committed) → keep  │    rename → dir fsync)
           │       old state, Err(Persist)        │
           │  Err(replacement_committed) → commit │
           │       next, report degraded          │
           └──────────────────────────────────────┘
                   │
                   ▼
            RemovalReport { rows_removed, runtimes_killed, paths_deleted, already_removed }

  Crash between Stage 2 and Stage 3 → file still holds prior rows → startup
  `git::reconcile_checkout` returns `Missing` (git.rs:488-491) →
  `record_checkout_reconciliation` sets `Protected(Missing)` (lifecycle.rs:586-636).
```

### Recommended Project Structure

```
baude-core/src/
├── removal.rs          # NEW: RemovalTarget, RemovalSet, RemovalReport, RemovalError,
│                       #      preview_removal(), remove(); tests with live Pty children
├── git.rs              # ADD: remove_managed_worktree_forced() + ForcedRemovalError,
│                       #      next to remove_verified_worktree (≈ line 2710); reuse
│                       #      private verify_removal_postconditions / topology checks
├── repository.rs       # ADD: RepositoryState::remove_rows(&RemovalSet) -> RemovedRows
│                       #      (generalizes remove_standalone_session, repository.rs:870)
├── lib.rs              # ADD: pub mod removal;
└── (pty.rs, session.rs, persist.rs unchanged)
```

### Pattern 1: Kill stage reuses `Session::kill_and_wait` verbatim and treats its error as abort

**What:** For each selected row that has a `Session` handle, call `session.kill_and_wait()`. Collect every `Err(SessionTeardownError)`; if any, return before stage 2.
**When to use:** Always; this is the locked kill unit.
**Source facts (read this session):**

```rust
// baude-core/src/session.rs:478-512 (verbatim shape)
pub fn kill_and_wait(&mut self) -> std::result::Result<(), SessionTeardownError> {
    let claude = self.claude.kill_and_wait().err();
    let shell = self.shell.as_mut().and_then(|shell| shell.kill_and_wait().err());
    match (claude, shell) {
        (None, None) => Ok(()),
        (claude, shell) => { /* builds SessionTeardownError with
            agent_pid, shell_pid, agent_identity, shell_identity,
            agent_stopped, shell_stopped, detail: failures.join("; ") */ }
    }
}

// baude-core/src/session.rs:516-523
pub struct SessionTeardownError {
    pub agent_pid: Option<u32>,
    pub shell_pid: Option<u32>,
    pub agent_identity: Option<ProcessIdentity>,
    pub shell_identity: Option<ProcessIdentity>,
    pub agent_stopped: bool,
    pub shell_stopped: bool,
    pub detail: String,
}
```

`Pty::kill_and_wait` (pty.rs:744-791) is idempotent: it returns `Ok` early if `try_wait` reports exit or if the live identity no longer matches the recorded `ProcessIdentity` (pid reuse guard, pty.rs:766-772), then `signal_group(SIGTERM)`, `wait_for_group_extinction`, `signal_group(SIGKILL)`, `wait_for_group_extinction`, and finally `child.wait()` to reap (pty.rs:773-790). `signal_group` treats `ESRCH` as success (pty.rs:812-826). `wait_for_group_extinction` is `for _ in 0..50 { try_wait; if !process_group_exists {return true}; sleep 10ms }` (pty.rs:830-838), so worst case per signal is about 500 ms and per PTY about 1 s. Nothing here is macOS- or Linux-specific; the only `cfg(unix)` gate is on the `suspend` path (pty.rs:672). [VERIFIED: baude-core/src/pty.rs:744-838]

**Rust shape for the runtime handles:** the core has no registry of live `Session`s (the TUI keeps `sessions: Vec<Session>` plus `runtime_checkouts: HashMap<CheckoutKey, u64>`, app.rs:3715-3722; the daemon keeps its own `sessions` in `Manager`, manager.rs:1302). Pass handles in:

```rust
pub enum RowKey { Checkout(CheckoutKey), Standalone(StandaloneKey) }
pub struct RowRuntime<'a> { pub row: RowKey, pub session: &'a mut Session }
```

A row whose `owned_runtime()` is `Some` but has no handle in the slice is a caller bug worth a typed error (`RemovalError::RuntimeHandleMissing(RowKey)`) rather than a silent leak. `SavedCheckout::owned_runtime()` is at repository.rs:402 and `SavedStandaloneSession::owned_runtime()` at repository.rs:486.

### Pattern 2: Row selection is pure and shared by preview and execute

**What:** `select_rows(&RepositoryState, &RemovalTarget) -> Result<RemovalSet, RemovalError>` resolves a `Repository(RepositoryKey)` target to the parent plus every `SavedCheckout` with that `repository_key`, a `Checkout(CheckoutKey)` to one row, and a `Standalone(StandaloneKey)` to one row. For each checkout it classifies the disk action:

| Row facts (repository.rs:348-365) | Disk action |
|---|---|
| `role == CheckoutRole::Main` | keep directory (RMV-02) |
| `managed_by_baude == true` and role `PrimaryDefault`/`ManagedBranch` | force-remove directory (RMV-01) |
| `managed_by_baude == false` and role != Main ("unmanaged checkout", TUI `ActionKind::External`, app.rs:338) | keep directory (RMV-07) |
| standalone (`SavedStandaloneSession`, repository.rs:453-461) | keep directory (RMV-03) |

Row kinds quoted verbatim: `pub enum CheckoutRole { Main, PrimaryDefault, ManagedBranch }` (repository.rs:284-288) [VERIFIED: baude-core/src/repository.rs:284-288]. Parent/child linkage is `pub repository_key: RepositoryKey` on `SavedCheckout` (repository.rs:350) and `pub repositories: Vec<SavedRepository>`, `pub checkouts: Vec<SavedCheckout>`, `pub standalone_sessions: Vec<SavedStandaloneSession>` on `RepositoryState` (repository.rs:510-518) [VERIFIED: baude-core/src/repository.rs:510-518].

**Already-removed detection:** if `!observed_path.to_path_buf().exists()` the report records the path under `already_removed` and stage 2 skips it (locked decision). Note a row can already be `Protected(UnavailableCause::Missing)` from reconcile; removal of such a row must still succeed.

### Pattern 3: Force removal lives in `git.rs` so it can reuse private postconditions

**What:** Add `pub fn remove_managed_worktree_forced(main_worktree: &Path, path: &Path) -> Result<ForcedRemoval, ForcedRemovalError>` beside `remove_verified_worktree` (git.rs:2710-2715).
**Why there:** `remove_verified_worktree_with_post_remove_hook` is `fn` (not `pub`) at git.rs:2681, and `verify_removal_postconditions` (git.rs:2622) and `topology_blockers` are private. The CONTEXT lists the hook variant as reusable; it is reusable only from inside `git.rs`. [VERIFIED: baude-core/src/git.rs:2681-2715]

**Behavior:**
1. Containment: `path.canonicalize()` (or the parent's canonical form when the leaf is gone) must start with `crate::git::worktrees_base()` (pub(crate), git.rs:1787-1797; test redirect honored via `testing::worktrees_base_override`). Outside the root, return `ForcedRemovalError::OutsideManagedRoot` and never touch disk. This is the only place the "never delete outside the managed root" rule is enforced, so it must be tested with a path just outside the root.
2. Run `git -C <main_worktree> worktree remove --force -- <path>` using the existing `Command` pattern (git.rs:2686-2700). The existing non-force arguments are `["worktree", "remove", "--"]` (git.rs:2614-2620); add `"--force"`.
3. On non-zero exit and path inside the managed root: `std::fs::remove_dir_all(path)` then `git -C <main> worktree prune`. The git docs state `prune` removes `$GIT_DIR/worktrees` entries "for worktrees whose working trees are missing", so the directory must be gone before prune runs [CITED: git-scm.com/docs/git-worktree]. If the worktree is locked, `remove` needs `--force` twice and `prune` skips it (lock "prevents its administrative files from being pruned") [CITED: git-scm.com/docs/git-worktree]; see Open Question 1.
4. Postcondition: path absent on disk and absent from `git worktree list --porcelain` for the common dir; the branch ref still resolves to the same OID (branch never deleted, RMV-07). Reuse `verify_removal_postconditions(target)` if a `VerifiedRemovalTarget` can be constructed from `discover_repository` facts (fields: `path, common_dir, main_worktree, branch_ref, branch_oid`, git.rs:1979-1985); otherwise write a small forced-variant postcondition in the same file.

The existing lifecycle tests already run `git worktree remove --force --` as cleanup (lifecycle.rs:3333, 3452, 3574, 3644, 3713, 3775), confirming the tree's own tests consider that command acceptable on a managed fixture. [VERIFIED: baude-core/src/lifecycle.rs:3333]

### Pattern 4: Single atomic save through a caller-supplied closure, commit-on-success

**What:** The operation mutates a clone and hands it to `save`. Only when `save` returns `Ok` does the clone replace `*state`.

```rust
// Shape (names at Claude's discretion)
pub fn remove(
    state: &mut RepositoryState,
    target: RemovalTarget,
    runtimes: &mut [RowRuntime<'_>],
    save: impl FnOnce(&persist::StateFile) -> Result<(), persist::SaveError>,
) -> Result<RemovalReport, RemovalError>
```

`StateFile { pub schema_version: u32, pub state: RepositoryState }` (persist.rs:21-24) with `pub const SCHEMA_VERSION: u32 = 3;` (persist.rs:17) [VERIFIED: baude-core/src/persist.rs:17,21-24]. The TUI will pass `|f| self.save_durable_status_with(f)` (its current `save_durable_status` routes to `save_current_at_test` under test and `save_for_workspace_status` otherwise, app.rs:1946-1958); the daemon passes `persist::save_for_workspace_status(STATE_BASE, workspace::active(), f)` (manager.rs:724).

**Error handling after disk delete:** use `SaveError::replacement_committed()` (persist.rs:278-280). `false` means the rename never happened: keep the old in-memory state and return `RemovalError::Persist` (disk already changed; startup reconcile covers the gap by design). `true` means the file was replaced but the directory fsync failed: commit the new state in memory and flag the report degraded, mirroring app.rs:3879-3899.

**Why `RepositoryState::remove_rows` must exist:** the only removal helper today is `remove_standalone_session` (repository.rs:870-879). `SavedCheckout::set_lifecycle` is `pub(crate)` (repository.rs:414), so any mutation must come from inside baude-core anyway. A batch method that removes checkouts by key, the repository by key, and standalone by key, then returns the removed rows for the report, keeps the clone-validate-save sequence one statement long. `validate()` must pass on `next`; the relevant checks are `DanglingRepositoryKey` (repository.rs:940-944) and `StandaloneCheckoutPathConflict` (repository.rs:540). [VERIFIED: baude-core/src/repository.rs:870-879, 940-944]

### Pattern 5: Preview is the same selection plus a dry classification

`preview_removal(&RepositoryState, RemovalTarget) -> Result<RemovalReport, RemovalError>` runs `select_rows`, fills `runtimes_to_kill` from `owned_runtime()` on each row, `paths_to_delete` from the Pattern 2 table, and `already_removed` from `Path::exists`. It never spawns git, never signals, never writes. The report type is shared; a `planned: bool` field or two thin wrapper types distinguish preview from result.

### Anti-Patterns to Avoid
- **Reusing `lifecycle::destructive_teardown` for the kill stage.** It calls `mark_teardown_pending`, which sets `Protected(TeardownPending{..})` on the row and validates (lifecycle.rs:919-941, 987-1000). KILL-03 and the locked decision require the row untouched on failure. Call `Session::kill_and_wait` directly.
- **Reusing `revoke_removal_authority` / `RemovalTombstone` before Git mutation.** That is the existing two-save protocol (app.rs:3802-3821 saves a tombstone, then saves again at 3877). RMV-06 forbids per-child or pre-mutation saves.
- **Deleting the branch.** Neither `git worktree remove` nor `prune` touches `refs/heads` [CITED: git-scm.com/docs/git-worktree]; never add `git branch -D`. The existing argument guard tests assert `"delete"` and `"-B"` never appear in removal arguments (git.rs:4302, 4510); add the forced arguments to the same style of assertion.
- **Hard-coding the save path in core.** The TUI and daemon use different entry points with test hooks (app.rs:1946, manager.rs:722). Take a closure.
- **Treating `Protected(Missing)` rows as un-removable.** The locked decision says removing a row whose directory is gone succeeds as already removed.
- **Using `std::process::Command::kill()` or `libc::kill(pid, ..)` on a single pid.** The whole point of `signal_group(-pgid)` is the gate shell's children (pty.rs:812-814 comment).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Process-group termination with pid-reuse safety and reaping | A new `kill`/`waitpid` loop | `Pty::kill_and_wait` via `Session::kill_and_wait` (pty.rs:744-791, session.rs:478-512) | Identity check against `inspect_process_identity`, ESRCH handling, bounded wait, reap, and `fail_next_teardown_for_test` injection already exist and are tested (BL-10). |
| Crash-safe state write | `fs::write` to the state path | `persist::save_current_at_status` / `save_for_workspace_status` (persist.rs:334-340, 1137-1143) | `create_new` temp, fsync, rename, dir fsync, state lock, `AtomicFailure` injection for tests (persist.rs:242-250). |
| Managed-root containment | Prefix string compare | `git::worktrees_base()` + canonicalize (git.rs:1787-1797) | Honors the test redirect and the `assert_contained` guard; macOS `/var` vs `/private/var` symlink needs canonicalization (lifecycle.rs:2947-2953 comment). |
| Worktree postcondition proof | Trusting git's exit code | `verify_removal_postconditions` pattern (git.rs:2622) | Exit 0 with a stale `$GIT_DIR/worktrees` entry has happened; the tree parses `worktree list --porcelain`. |
| Fixture isolation | Ad-hoc temp dirs | `LifecycleFixture` (lifecycle.rs:2924-2965) / `testing::TestRedirect::new(root)` (testing.rs:128-139) | Redirects worktrees base, config dir, home; `assert-real-roots-untouched.sh` in the gate fails the run if a test leaks to the real root. |
| Zombie assertion | `ps` parsing in a new way | `assert_reaped_promptly` / `is_zombie` (pty.rs:1341-1350) and `assert_reaped` (session.rs:1489) | Already portable across macOS and Linux in the suite. |

**Key insight:** this phase is a composition of four proven primitives; every custom reimplementation re-opens a bug class (BL-10 zombies, #72 leaked worktrees, the two-save tombstone protocol) that the tree already paid for.

## Common Pitfalls

### Pitfall 1: Kill failure path leaks through state mutation
**What goes wrong:** Using `destructive_teardown` makes a surviving runtime write `Protected(TeardownPending)` and, in the TUI, triggers a save (app.rs:3558-3568). The row then looks closed-ish and the state file changed, violating KILL-03's "row and state file untouched".
**Why it happens:** It is the existing "single typed stop boundary for close and confirmed removal" (lifecycle.rs:984-986), so it is the obvious reuse.
**How to avoid:** Call `Session::kill_and_wait` and wrap `SessionTeardownError` in `RemovalError::RuntimeSurvived { row, error }`. For a parent, iterate children and abort on the first error before any disk work (locked).
**Warning signs:** A test where `Pty::fail_next_teardown_for_test` is armed and the state file's mtime or bytes change.

### Pitfall 2: `git worktree remove --force` refuses a locked worktree
**What goes wrong:** Rows marked `LockedOrPrunable` by reconcile (git.rs:509-511) exist. Single `--force` is refused for a locked tree; the docs require `--force` twice, and `prune` skips locked entries, so the fallback leaves admin files behind. [CITED: git-scm.com/docs/git-worktree]
**How to avoid:** Decide in planning (Open Question 1). Recommended: pass `--force --force` for paths proven inside the managed root (user explicitly asked; the root is baude's own), and run `git worktree unlock` before `prune` in the fallback if locked.
**Warning signs:** `GitRefused` stderr containing "locked".

### Pitfall 3: Deleting the directory before killing
**What goes wrong:** On Unix a live process keeps a deleted cwd; files it writes afterward land in an unlinked directory and `git worktree remove --force` can race new untracked files. Ordering disk before kill also makes `kill_and_wait`'s identity check irrelevant to the worktree.
**How to avoid:** Locked stage order: kill all, then disk, then save. Test "runtime held it as cwd" by spawning the double with `cwd = managed path` and asserting the path is gone and the pid reaped.

### Pitfall 4: Fixture roots on macOS are symlinked
**What goes wrong:** `temp_dir()` is `/var/...` but git reports `/private/var/...`, so path equality and containment checks fail only on macOS.
**How to avoid:** Canonicalize the fixture root as `LifecycleFixture::new` does (lifecycle.rs:2947-2953) and canonicalize before containment in `remove_managed_worktree_forced`. When the leaf is already deleted, canonicalize the parent and re-join the file name.

### Pitfall 5: Validation permits a parent with no children, but not the reverse
**What goes wrong:** Removing children and forgetting the parent passes `validate()`; removing the parent and leaving a child fails with `DanglingRepositoryKey` only at save time, after disk deletion.
**How to avoid:** `remove_rows` removes the whole `RemovalSet` in one method and `validate()`s before `save`. Add a unit test that a `Repository` target yields zero rows with that `repository_key` afterwards.

### Pitfall 6: Test double that dies on SIGTERM never exercises SIGKILL
**What goes wrong:** `sleep 30` exits on SIGTERM, so the escalation branch (pty.rs:775-780) is untested.
**How to avoid:** Spawn `Pty::spawn(Some("trap '' TERM HUP; sleep 30"), cwd, 5, 40)` and sleep ~200 ms before killing so the trap is armed (the exact lesson recorded at pty.rs:1377-1390 and session.rs:1458-1461). Assert extinction with `kill(-pgid, 0)` returning `ESRCH` and `assert_reaped_promptly(pid)`.

### Pitfall 7: Deleting outside the managed root via the fallback
**What goes wrong:** An unmanaged worktree, or a managed row whose `observed_path` was edited, hits the `remove_dir_all` fallback.
**How to avoid:** Containment check precedes any `remove_dir_all`, using `worktrees_base()`; a test places a real linked worktree one directory outside the redirected root and asserts both that it is refused and that it still exists.

### Pitfall 8: Piping gate commands
**What goes wrong:** `cargo test | tail` hides the exit code (CLAUDE.md; memory note).
**How to avoid:** Run `scripts/gate.sh` and `scripts/gate.sh --linux` unpiped; the gate runs `cargo test -- --test-threads=1` in an isolated home (gate.sh:48).

## Code Examples

Verified patterns from this repository:

### Existing kill primitive to compose (do not modify)
```rust
// baude-core/src/pty.rs:773-790
signal_group(self.identity.process_group, libc::SIGTERM)
    .context("failed to terminate exact PTY process group")?;
if !wait_for_group_extinction(self.identity.process_group, child.as_mut()) {
    signal_group(self.identity.process_group, libc::SIGKILL)
        .context("failed to force exact PTY process group")?;
    if !wait_for_group_extinction(self.identity.process_group, child.as_mut()) {
        anyhow::bail!("PTY process group {} remained live after forced termination",
            self.identity.process_group);
    }
}
if child.try_wait().context("...")?.is_none() {
    child.wait().context("failed to wait for PTY child")?;
}
```

### Existing git command pattern to copy for the force path
```rust
// baude-core/src/git.rs:2686-2700 (non-force; add "--force" for the new fn)
let output = Command::new("git")
    .arg("-C")
    .arg(&target.main_worktree)
    .args(arguments)            // ["worktree", "remove", "--"] at git.rs:2614-2620
    .arg(&target.path)
    .output()
    .map_err(RemoveVerifiedError::CommandStart)?;
if !output.status.success() {
    return Err(RemoveVerifiedError::GitRefused {
        status: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    });
}
```

### Existing single-row removal to generalize
```rust
// baude-core/src/repository.rs:870-879
pub fn remove_standalone_session(&mut self, key: StandaloneKey) -> Option<SavedStandaloneSession> {
    let index = self.standalone_sessions.iter().position(|session| session.key == key)?;
    Some(self.standalone_sessions.remove(index))
}
```

### Existing save-result handling to mirror after the single save
```rust
// baude/src/app.rs:3879-3899 (pattern only; core returns the decision, callers act)
match self.save_durable_status() {
    Ok(()) => { self.persistence_dirty = false; Ok(outcome) }
    Err(error) if !error.replacement_committed() => { self.repository_state = before_deletion; /* degraded */ }
    Err(error) => { self.persistence_dirty = true; /* committed but unsynced */ }
}
```

### Live-runtime test double (fixture realism: real process group)
```rust
// Derived from baude-core/src/pty.rs:1389-1395 and session.rs:1443-1489
let fixture = LifecycleFixture::new("remove-live");          // redirects roots
let (upstream, clone, managed) = cloned_topology(&fixture); // lifecycle.rs:4290
let mut agent = Pty::spawn(Some("trap '' TERM HUP; sleep 30"), &managed, 5, 40).unwrap();
let pgid = agent.process_identity().process_group;
std::thread::sleep(Duration::from_millis(200));             // let the trap arm
let mut session = Session { claude: agent, shell: None, cwd: managed.clone(), /* ... as session.rs:1463-1484 */ };
let report = removal::remove(&mut state, RemovalTarget::Checkout(key),
    &mut [RowRuntime { row: RowKey::Checkout(key), session: &mut session }],
    |file| persist::save_current_at_status(fixture.config_root(), "state.json", file)).unwrap();
assert!(!managed.exists());
assert_eq!(unsafe { libc::kill(-pgid, 0) }, -1);            // group extinct
assert_reaped_promptly(pid);
// branch untouched:
assert!(git_in(&clone, &["rev-parse", "--verify", "refs/heads/feature/kept"]).is_ok());
```

### Survivor fixture (KILL-03) without an unkillable process
```rust
// baude-core/src/pty.rs:794-802: debug-only injection consumed by kill_and_wait at 745-753
agent.fail_next_teardown_for_test("injected: group survived SIGKILL");
let err = removal::remove(/* ... */).unwrap_err();
assert!(matches!(err, RemovalError::RuntimeSurvived { .. }));
assert_eq!(fs::read(state_path).unwrap(), before_bytes);    // file untouched
assert!(managed.exists());                                   // disk untouched
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Single-checkout removal with pre-mutation tombstone and two saves (`revoke_removal_authority` → git → `commit_removed_checkout`), TUI only (app.rs:3790-3899) | One operation for any row kind, kill-first, one save, shared by TUI and daemon | This phase (v2.8) | The old protocol remains for the existing `X` key until Phase 20 replaces the call site; both can coexist because the new module does not touch `lifecycle.rs` types. |
| `Pty::kill()` best-effort signal (session.rs:377-384) | `kill_and_wait` with identity check, escalation and reap (BL-10, v2.4.2) | 2026-09 | The new phase must use `kill_and_wait`, never `kill()`. |
| Remove refused for Repository/Main/Standalone/External rows in the sidebar (app.rs:331-345 `SidebarRefusal::*Remove`) | Core supports every kind; the TUI lifts refusals in Phase 20 | Phase 18 (core) / 20 (TUI) | Phase 18 must not edit `sidebar_action`. |

**Deprecated/outdated:** nothing removed in this phase; `lifecycle::prepare_removal`/`execute_verified_removal`/`commit_removed_checkout` stay for the current `X` path.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `kill(-pgid, sig)` and `ESRCH` semantics are identical on macOS and Linux for this use (no OS branch needed in new code) | Pattern 1 | Low: the primitive is unchanged and already exercised by `gate.sh --linux`; new code adds no signal calls. [ASSUMED] |
| A2 | A live process holding the worktree as cwd does not prevent `remove_dir_all` or `git worktree remove --force` on macOS or Linux once that process is dead; the kill stage precedes disk so cwd-holding is moot | Pitfall 3 | Low: covered by the "held as cwd" fixture test. [ASSUMED] |
| A3 | `git worktree prune` with no `--expire` prunes a missing-directory entry immediately (default `gc.worktreePruneExpire` only governs automatic gc, not explicit prune) | Pattern 3 step 3 | Medium: if prune leaves the entry, the postcondition check fails and the op reports `DiskRemoval` while the dir is gone. Verify in the fallback test by asserting `worktree list --porcelain` no longer lists the path. [ASSUMED] |
| A4 | `verify_removal_postconditions` can be reused by constructing a `VerifiedRemovalTarget` from `discover_repository` facts for the forced path | Pattern 3 step 4 | Low: fallback is a forced-variant postcondition in the same file. [ASSUMED] |

## Open Questions (RESOLVED)

1. **Locked worktrees: `--force` once or twice?**
   - What we know: CONTEXT says `--force`; git refuses locked trees without a second `--force` and `prune` skips locked entries [CITED: git-scm.com/docs/git-worktree]. Reconcile already classifies such rows `LockedOrPrunable` (git.rs:509-511).
   - What's unclear: whether the user's "explicit removal" intent extends to a lock the user (or a network-mount guard) placed.
   - Recommendation: for paths inside the managed root, pass `--force --force`; outside the managed root nothing is deleted anyway. Record as a decision in the plan.
   - RESOLVED: plan 18-02 passes `--force --force` inside the managed root and refuses any path outside it with `OutsideManagedRoot`.

2. **Where does `RepositoryReservations` get taken?**
   - What we know: the TUI reserves the repository around prepare/confirm (app.rs:3704-3708, 3716-3719); the daemon imports `RepositoryReservations` (manager.rs:16).
   - Recommendation: keep reservation at the caller (Phases 20/21) and document in the new module's doc comment that callers must hold one for every repository in the `RemovalSet`.
   - RESOLVED: plan 18-04 keeps `RepositoryReservations` at the TUI and daemon callers and documents the requirement in the `removal` module doc comment.

3. **Should the kill stage mark anything in memory when a runtime survives?**
   - What we know: locked decision says row and state file untouched; `mark_teardown_pending` exists for close.
   - Recommendation: return the error only; the caller may choose to run the existing close-recovery path afterward. Do not couple.
   - RESOLVED: plan 18-04 returns a structured survivor error with pids and leaves the row, in-memory state and state file untouched; no `mark_teardown_pending` coupling.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| rustc / cargo | build, tests | ✓ | 1.99.1 | — |
| git | worktree fixtures and removal | ✓ | 2.54.0 (Apple Git-157) | — |
| docker | `scripts/gate.sh --linux` | ✓ | 29.9.2 client | — (Linux gate is required by CONTEXT for signal/procfs paths) |
| `/bin/sh` with `trap` | test process doubles | ✓ (POSIX) | — | — |

**Missing dependencies with no fallback:** none.
**Missing dependencies with fallback:** none.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` (cargo test), no external harness |
| Config file | none; `scripts/gate.sh` is the authoritative runner |
| Quick run command | `cargo test -p baude-core removal::` (new module filter) |
| Full suite command | `scripts/gate.sh` (fmt, clippy `-D warnings`, commit-message check, isolated `cargo test -- --test-threads=1`, real-roots leak check); `scripts/gate.sh --linux` for the Linux signal path |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| KILL-01 | TERM-trapping child's group is extinct after remove | integration (real pty) | `cargo test -p baude-core removal::tests::live_runtime_group_is_extinct_after_remove` | ❌ Wave 0 |
| KILL-02 | No zombie: `assert_reaped_promptly(pid)` after remove, macOS + Linux | integration | same test + `scripts/gate.sh --linux` | ❌ Wave 0 |
| KILL-03 | Injected survivor leaves row, file bytes, and directory unchanged; error names pids | integration (`fail_next_teardown_for_test`) | `cargo test -p baude-core removal::tests::surviving_runtime_leaves_row_and_disk` | ❌ Wave 0 |
| RMV-01 | Unclean managed worktree (untracked + modified) and cwd-held worktree are deleted | integration (`git init` + cloned fixtures) | `cargo test -p baude-core removal::tests::managed_worktree_` | ❌ Wave 0 |
| RMV-01 (fallback) | Git refusal inside managed root falls back to prune + delete; outside root refused and untouched | integration | `cargo test -p baude-core git::tests::forced_removal_` | ❌ Wave 0 |
| RMV-02 | Main checkout removal: dir exists, `git branch --list` identical before/after | integration, both fixtures | `cargo test -p baude-core removal::tests::main_checkout_keeps_directory_and_branches` | ❌ Wave 0 |
| RMV-03 | Standalone removal keeps folder | integration | `cargo test -p baude-core removal::tests::standalone_keeps_folder` | ❌ Wave 0 |
| RMV-04 | Repository target removes all children then parent; no row with that `repository_key` remains | unit on `RepositoryState` + integration | `cargo test -p baude-core removal::tests::repository_removes_children_then_parent` | ❌ Wave 0 |
| RMV-05 | Report serializes round-trip (serde_json) and preview equals result row set | unit | `cargo test -p baude-core removal::tests::report_` | ❌ Wave 0 |
| RMV-06 | Exactly one state write per op (count temp files / mtime once; `AtomicFailure::Rename` injection leaves prior bytes) | integration via `save_current_at_test` | `cargo test -p baude-core removal::tests::single_atomic_save` | ❌ Wave 0 |
| RMV-07 | Branch refs identical after every removal kind on cloned and `git init` fixtures | integration | included in RMV-01/02/03 tests via `rev-parse --verify refs/heads/<b>` | ❌ Wave 0 |
| SC5 | Crash between disk and save: file still parses, reconcile marks `Protected(Missing)` | integration (`AtomicFailure::Write`) | `cargo test -p baude-core removal::tests::crash_before_save_reconciles_missing` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p baude-core removal::` and `cargo test -p baude-core git::tests::forced_removal_`
- **Per wave merge:** `scripts/gate.sh`
- **Phase gate:** `scripts/gate.sh` and `scripts/gate.sh --linux` both green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `baude-core/src/removal.rs` with a `#[cfg(test)] mod tests` reusing `LifecycleFixture`-style redirect (copy or lift `cloned_topology`/`git_in` into `testing.rs` if shared across modules)
- [ ] `git.rs` tests for `remove_managed_worktree_forced` (inside/outside root, locked, already missing)
- [ ] `repository.rs` unit test for `remove_rows` + `validate()`
- [ ] Framework install: none needed

## Security Domain

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | — (local process; REST auth is Phase 21) |
| V3 Session Management | no | — |
| V4 Access Control | yes | Managed-root containment before any `remove_dir_all`; never delete outside `worktrees_base()`; identity-checked signalling prevents killing a reused pid (pty.rs:766-772) |
| V5 Input Validation | yes | `RemovalTarget` keys resolved against `RepositoryState`; unknown key is a typed error; paths come from persisted `observed_path`, never from caller strings |
| V6 Cryptography | no | — |

### Known Threat Patterns for this stack
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Pid reuse: signalling a new process that inherited an old pid | Tampering / DoS | `inspect_process_identity` start-time check already in `Pty::kill_and_wait` (pty.rs:766-772) |
| Path traversal / symlinked `observed_path` pointing outside managed root | Tampering | canonicalize then prefix-check against `worktrees_base()`; refuse and never fall back to `remove_dir_all` |
| Argument injection via branch or path starting with `-` | Tampering | always pass `--` before the path (existing `verified_remove_arguments`, git.rs:2614-2620) |
| Partial write leaving an unparsable state file | DoS | `atomic_save_current` temp + rename (persist.rs:640-706) |

## Sources

### Primary (HIGH confidence)
- `baude-core/src/pty.rs:744-838` — `kill_and_wait`, `signal_group`, `process_group_exists`, `wait_for_group_extinction`, `fail_next_teardown_for_test`
- `baude-core/src/session.rs:59-112, 377-384, 478-523, 1443-1489` — `Session`, `kill`, `kill_and_wait`, `SessionTeardownError`, two-child test
- `baude-core/src/git.rs:399-528, 1757-1835, 1954-2082, 2549-2575, 2614-2620, 2681-2735, 4290-4325` — reconcile, managed root, removal types, `inspect_removal`, arguments, remove functions
- `baude-core/src/persist.rs:17-24, 242-300, 330-342, 640-706, 1114-1152` — schema, `SaveError`, atomic save, wrappers
- `baude-core/src/repository.rs:62-130, 182-215, 284-288, 336-365, 453-461, 510-535, 870-879, 894-962` — row kinds, linkage, `remove_standalone_session`, `validate`
- `baude-core/src/lifecycle.rs:105-160, 586-640, 673-735, 780-900, 919-1000, 1138-1166, 2784-2806, 2828-2855, 2924-2965, 3333` — effects trait, reconciliation, close/removal protocol, teardown, outcomes, reservations, fixtures
- `baude/src/app.rs:331-372, 1946-1958, 3551-3570, 3700-3725, 3790-3899` — key handling, save routing, existing kill-then-remove sequence
- `bauded/src/manager.rs:16, 306-330, 722-728, 1302` — daemon lifecycle effects and save path
- `baude-core/src/testing.rs:107-200`, `baude-core/src/meta.rs:2126-2160, 2260-2275` — redirects, `setsid` process doubles
- `scripts/gate.sh`, `baude-core/Cargo.toml`, `Cargo.toml` — gate steps and dependencies

### Secondary (MEDIUM confidence)
- git-scm.com/docs/git-worktree (fetched this session) — `remove` refusal rules, `--force` twice for locked, `prune` contract, branches untouched. Seam classified the `webfetch` provider as LOW; the source is the official manual so claims are tagged `[CITED]`.

### Tertiary (LOW confidence)
- POSIX `kill(-pgid)` cross-platform equivalence — not fetched; tagged `[ASSUMED]` (A1), mitigated by the unchanged primitive and the Linux gate.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new dependencies; every crate read from Cargo.toml
- Architecture: HIGH — all referenced functions, types and call sites read with line ranges; the two conflicts with existing protocols are documented with lines
- Pitfalls: HIGH for in-repo pitfalls (1, 3-8), MEDIUM for the locked-worktree git behavior (2, cited docs, not executed)

**Research date:** 2026-11-02
**Valid until:** 2026-12-02 (stable internal APIs; re-verify line numbers if `lifecycle.rs` or `git.rs` change before planning)
