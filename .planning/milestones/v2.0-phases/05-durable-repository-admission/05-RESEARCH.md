# Phase 5: Durable Repository Admission - Research

**Researched:** 2026-08-30
**Domain:** Rust repository identity, Git worktree admission, durable JSON migration, and idempotent session orchestration
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

### Default-Branch Resolution
- Resolve the default from Git's locally recorded remote symbolic HEAD, preferring the configured upstream remote and then `origin`; never infer a branch from conventional names.
- If the resolved default branch is already checked out in another linked worktree, reuse and register that worktree.
- If the main checkout already has the default branch, use it as the primary child instead of creating a redundant worktree.
- For unresolved, detached, or unborn default state, retain the repository parent in an actionable unavailable-primary state and launch nothing.

### Repository Identity
- Establish repository membership from Git's common directory and the first main-worktree record in `git worktree list --porcelain -z`.
- Persist an opaque repository key with observed canonical Git paths, and revalidate path facts before mutation.
- Resolve subdirectories through Git and canonicalize existing paths so symlink and path aliases deduplicate.
- Persist the same filesystem repository independently in each Claude Code or OpenCode workspace; there is no global cross-workspace registry.

### Persistence and Migration
- Migrate legacy flat sessions by reconciled Git identity, preserving every valid session field and assigning deterministic parent/child first-seen order; migration must be idempotent.
- Preserve malformed state, surface a blocking load error, and never overwrite it with an empty hierarchy.
- Save through a flushed sibling temporary file followed by atomic rename; failed writes retain the old file.
- Retain missing or externally changed parent/child metadata in an unavailable state for later reconciliation rather than silently pruning it.

### Primary Session Lifecycle
- Launch the default-branch primary session immediately after successful repository admission or clone using the active workspace backend.
- Reopening focuses a live primary or restarts/resumes an exited retained child; it never creates duplicate primary sessions.
- Closing a primary session retains the repository parent and checkout child for explicit reopening.
- Restore hierarchy first, then ensure a primary session only for repositories that previously had an active primary; intentionally sessionless parents remain idle.

### the agent's Discretion
- Exact opaque key representation, error type organization, and internal collection shape may follow existing Rust conventions as long as identity remains stable and workspace-scoped.
- Exact unavailable-state wording may follow existing TUI message style, provided the cause and recovery action are clear.

### Deferred Ideas (OUT OF SCOPE)
- Safe managed-worktree create/close/remove semantics are Phase 6.
- Local hierarchy rendering, dormant branches, ordering, and contextual shortcuts are Phase 7.
- Daemon/remote and PWA projections are Phases 8 and 9.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| REPO-01 | Open any checkout alias and produce one canonical repository parent per active workspace. | Common-directory identity, main-worktree inventory, canonical path handling, and workspace-scoped opaque keys. [CITED: https://git-scm.com/docs/git-rev-parse] [CITED: https://git-scm.com/docs/git-worktree] |
| REPO-02 | Opening or cloning ensures exactly one usable default-branch session through the active backend. | Offline remote-HEAD resolution, primary-child role, persisted active intent, and an idempotent ensure operation. [CITED: https://git-scm.com/docs/git-remote] [VERIFIED: codebase `baude/src/app.rs`, `baude-core/src/workspace.rs`] |
| REPO-03 | Reopening focuses or reopens the existing default child. | Uniqueness by `(repository_key, Primary)` and explicit live/exited/idle handling. [VERIFIED: codebase `baude/src/app.rs`] |
| REPO-04 | Preserve a non-default main checkout and create or reuse a separate default worktree. | Git inventory determines occupancy; explicit `git worktree add` creates the separate checkout without switching the main worktree. [CITED: https://git-scm.com/docs/git-worktree] |
| REPO-06 | Unresolvable local default state is actionable and non-mutating. | Typed default resolution returns unavailable rather than guessing, fetching, or switching. [CITED: https://git-scm.com/docs/git-remote] [CITED: https://git-scm.com/docs/git-symbolic-ref] |
| PERS-01 | Repository, child, ordering, managed, UI, and session state survive restart per workspace. | Versioned aggregate records retain opaque keys, observations, first-seen order, session settings, and active intent in existing workspace-specific files. [VERIFIED: codebase `baude-core/src/persist.rs`, `baude-core/src/workspace.rs`] |
| PERS-02 | Flat local and daemon state migrates idempotently without losing valid backend sessions. | Explicit legacy/new decoding, source-order migration, Git reconciliation, and existing state-file fallback precedence. [VERIFIED: codebase `baude-core/src/persist.rs`, `bauded/src/manager.rs`] |
| PERS-03 | Reconcile persisted intent against current Git topology before reuse, activation, removal, or launch. | Discovery snapshots separate baude intent from Git facts and are refreshed before process or Git mutation. [CITED: https://git-scm.com/docs/git-worktree] |
| PERS-04 | Writes are atomic and malformed/partial state is surfaced. | Result-valued load plus same-directory temporary-file write, flush/sync, close, and rename; automatic save is disabled after load failure. [CITED: https://doc.rust-lang.org/std/fs/struct.File.html#method.sync_all] [CITED: https://doc.rust-lang.org/std/fs/fn.rename.html] |
</phase_requirements>

## Summary

Phase 5 should be planned as four dependent contracts: authoritative Git discovery, a durable repository/checkout aggregate, explicit legacy migration and atomic persistence, then one idempotent primary-session admission path. The existing `git.rs`, `persist.rs`, workspace binding, and `App::add_session` are the correct seams, but their current behavior is insufficient: Git output is converted lossily, loads collapse every error to empty state, saves overwrite the destination directly, restore drops missing paths, and repeated admission can spawn duplicate sessions. [VERIFIED: codebase `baude-core/src/git.rs`, `baude-core/src/persist.rs`, `baude/src/app.rs`]

The default branch must come only from a local `refs/remotes/<remote>/HEAD` symbolic ref. To keep admission invariant across entry paths, define “configured upstream remote” as the upstream remote of the **main worktree's current local branch**, then fall back to `origin`; never derive this preference from whichever linked worktree the user happened to open. Resolve the full symbolic target, verify its remote prefix and commit target, and strip only that prefix so slash-containing branch names survive. [CITED: https://git-scm.com/docs/git-for-each-ref] [CITED: https://git-scm.com/docs/git-symbolic-ref] [CITED: https://git-scm.com/docs/git-remote]

The persistence cutover is the highest-risk part. Decode missing files as first run, legacy flat files as a migration input, and malformed/unsupported new files as blocking errors. Migrate only the file selected by existing workspace fallback rules, preserve all legacy session fields, retain unavailable records, and do not launch any process until the migrated/admitted aggregate has been atomically saved. [VERIFIED: codebase `baude-core/src/persist.rs`, `baude/src/app.rs`, `bauded/src/manager.rs`] [CITED: https://doc.rust-lang.org/std/fs/fn.rename.html]

**Primary recommendation:** Build and test `discover_repository` + versioned persistence first, then make every local startup/open/clone completion call one `admit_repository`/`ensure_primary` state machine keyed by persisted repository and primary-child IDs. [VERIFIED: codebase `baude/src/app.rs`]

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Canonical repository/default/worktree discovery | Core domain / Git adapter (`baude-core`) | System Git CLI | One byte-safe, authoritative implementation must serve every owner; Git owns live topology. [CITED: https://git-scm.com/docs/git-worktree] |
| Durable repository and child identity | Core domain / persistence (`baude-core`) | Local App owner | Persistence owns stable keys and intent; `App` owns runtime instances for the active workspace. [VERIFIED: codebase `baude-core/src/lib.rs`, `baude/src/app.rs`] |
| Flat-state migration and atomic state files | Core persistence (`baude-core`) | Local App and daemon Manager callers | Both local and daemon filenames already pass through shared persistence helpers. [VERIFIED: codebase `baude-core/src/persist.rs`, `bauded/src/manager.rs`] |
| Primary-session ensure/focus/restart | Local App orchestration | Existing backend/session layer | `App` decides uniqueness and focus; the active workspace backend prepares and spawns the PTY. [VERIFIED: codebase `baude/src/app.rs`, `baude-core/src/workspace.rs`] |
| Managed default worktree creation/reuse | Core Git adapter | Local App orchestration | Git validates branch occupancy and creates topology; `App` records ownership and starts the session. [CITED: https://git-scm.com/docs/git-worktree] |
| Unavailable-state presentation | Local App message state | Core typed error/health state | Core retains cause and recovery facts; wording follows existing TUI messages. [VERIFIED: codebase `baude/src/app.rs`] |

## Standard Stack

### Core
| Library / Tool | Version | Purpose | Why Standard |
|----------------|---------|---------|--------------|
| Rust standard library | Rust 1.98.0; Edition 2021 | Paths, subprocesses, newtypes, filesystem writes, flush/sync, rename | Installed toolchain and existing workspace provide all required primitives; no new crate is needed. [VERIFIED: local `rustc --version`; codebase `Cargo.toml`] |
| System Git CLI | 2.50.1 locally | Common-dir identity, symbolic refs, worktree inventory/add/reuse | Git documents stable `worktree list --porcelain -z` output and owns worktree safety/config semantics. [VERIFIED: local `git --version`] [CITED: https://git-scm.com/docs/git-worktree] |
| `serde` | 1.0.228 locked | Versioned records and explicit legacy/new decoding | Already used by persistence; `#[serde(default)]` is appropriate only for fields whose absence has defined compatibility meaning. [VERIFIED: codebase `Cargo.lock`] [CITED: https://serde.rs/field-attrs.html] |
| `serde_json` | 1.0.150 locked | Workspace-scoped state envelope | Existing state is JSON and small/single-owner; retaining it avoids an unrelated storage migration. [VERIFIED: codebase `Cargo.lock`, `baude-core/src/persist.rs`] |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `anyhow` | 1.0.102 locked | Add command/path context at application boundaries | Wrap typed core errors for displayed/logged action context; do not erase load/default-state variants. [VERIFIED: codebase `Cargo.lock`, `baude-core/src/git.rs`] |
| `dirs` | 5.0.1 locked | Preserve current XDG config/data roots | Continue resolving state and managed-worktree bases exactly where current users have data. [VERIFIED: codebase `Cargo.lock`, `baude-core/src/persist.rs`, `baude-core/src/git.rs`] |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| System Git CLI | `git2`/libgit2 | Adds a dependency/native surface and would make baude reproduce installed Git's worktree/config behavior; not justified for this phase. [CITED: https://git-scm.com/docs/git-worktree] |
| Serde JSON aggregate | SQLite | Multiple-writer querying is not a phase requirement; changing storage would increase migration risk. [VERIFIED: codebase `.planning/REQUIREMENTS.md`, `baude-core/src/persist.rs`] |
| Standard-library atomic replacement | `tempfile`/atomic-write crate | A unique sibling path, `File`, `sync_all`, and `rename` satisfy the locked sequence without package installation. [CITED: https://doc.rust-lang.org/std/fs/struct.File.html#method.sync_all] [CITED: https://doc.rust-lang.org/std/fs/fn.rename.html] |

**Installation:** None. Keep `Cargo.toml` and `Cargo.lock` unchanged. [VERIFIED: codebase `Cargo.toml`, `.planning/research/STACK.md`]

## Architecture Patterns

### System Architecture Diagram

```text
entry path (launch / Open / clone completion)
  -> canonicalize existing path
  -> Git discovery (common-dir + worktree porcelain)
       -> invalid repository -----------------------> actionable admission error
       -> canonical repository/main worktree
            -> find/create workspace-scoped repository key
            -> resolve local remote symbolic HEAD
                 -> unresolved/detached/unborn -----> persist unavailable primary; launch nothing
                 -> default local branch
                      -> main has branch -----------> reuse main checkout child
                      -> linked worktree has branch -> reuse registered checkout child
                      -> neither -------------------> add managed default worktree; rediscover
                           -> atomic save of repository/child/active intent
                           -> ensure primary session
                                -> live ------------> focus existing
                                -> exited ----------> restart/resume retained child
                                -> absent + active -> spawn through workspace backend
                                -> intentionally idle -> no spawn
```

The diagram's Git branches follow documented main-first inventory, symbolic-ref behavior, and worktree branch-occupancy safeguards. [CITED: https://git-scm.com/docs/git-worktree] [CITED: https://git-scm.com/docs/git-symbolic-ref]

### Recommended Project Structure
```text
baude-core/src/
├── repository.rs   # stable repository/checkout keys, roles, health, persisted-intent value types
├── git.rs          # byte-oriented command output, topology parser, default resolution, default-worktree ensure
├── persist.rs      # versioned envelope, legacy migration, result-valued load, atomic save
└── lib.rs          # exports repository module
baude/src/
└── app.rs          # durable aggregate owner; admit/ensure/focus/restart; startup/open/clone integration
bauded/src/
└── manager.rs      # adapt to result-valued shared persistence/migration without adding Phase-8 hierarchy APIs
```

This keeps the repository model UI-free in `baude-core`, preserves `App` as local runtime owner, and limits daemon work to safe state-format compatibility required by PERS-02. [VERIFIED: codebase `baude-core/src/lib.rs`, `baude/src/app.rs`, `bauded/src/manager.rs`] [VERIFIED: phase boundary in `05-CONTEXT.md`]

### Pattern 1: Persist Intent, Reconcile Git Facts
**What:** Persist stable opaque keys, child ownership/role, baude-managed status, first-seen order, session settings, active intent, and last observed paths/branch. Before launch or mutation, refresh common-dir and `worktree list --porcelain -z`; update health/observations without silently deleting intent. [CITED: https://git-scm.com/docs/git-worktree]

**Recommended records:**
```rust
// Source: project design constrained by 05-CONTEXT.md; serde defaults documented at
// https://serde.rs/field-attrs.html
struct StateFile {
    schema_version: u32,
    next_key: u64,
    repositories: Vec<SavedRepository>,
    children: Vec<SavedCheckout>,
}

struct SavedRepository {
    key: RepositoryKey,             // persisted opaque newtype, not a path hash
    observed_common_dir: PersistedPath,
    observed_main_worktree: PersistedPath,
    first_seen_order: u64,
}

struct SavedCheckout {
    key: CheckoutKey,
    repository_key: RepositoryKey,
    role: CheckoutRole,             // Main | PrimaryDefault | ManagedBranch
    managed_by_baude: bool,
    observed_path: PersistedPath,
    observed_branch: Option<String>,
    first_seen_order: u64,
    active_intent: bool,
    session: SavedSessionState,     // name/shell/archive fields retained even while idle
    health: CheckoutHealth,
}
```

Use monotonically allocated persisted `u64` newtypes for repository and child keys. This follows the project's existing ID convention while remaining opaque, stable, path-independent, and workspace-scoped. [VERIFIED: codebase `baude/src/app.rs`, `bauded/src/manager.rs`] [VERIFIED: discretion in `05-CONTEXT.md`]

### Pattern 2: One Canonical Discovery Snapshot
**What:** Run one discovery function from any existing input path and return canonical input path, common directory, main worktree, all worktree records, selected record, and typed branch/default state. Git documents `--path-format=absolute` as canonical absolute output and lists the main worktree first. [CITED: https://git-scm.com/docs/git-rev-parse] [CITED: https://git-scm.com/docs/git-worktree]

**Commands:**
```text
git -C <canonical-input> rev-parse --path-format=absolute --git-common-dir
git -C <canonical-input> worktree list --porcelain -z
git -C <main-worktree> symbolic-ref -q HEAD
git -C <main-worktree> rev-parse --verify --quiet --end-of-options <main-head-ref>^{commit}
git -C <main-worktree> for-each-ref --format=%(upstream:remotename) <main-head-ref>
git -C <main-worktree> symbolic-ref -q refs/remotes/<preferred>/HEAD
git -C <main-worktree> rev-parse --verify --quiet --end-of-options <resolved-remote-target>^{commit}
```

Keep `Command::output().stdout` as bytes while parsing NUL-delimited fields. Git recommends `-z` specifically so worktree paths containing newlines are unambiguous; on Unix, `OsStringExt::from_vec` constructs an `OsString` from the retained bytes. [CITED: https://git-scm.com/docs/git-worktree] [CITED: https://doc.rust-lang.org/std/os/unix/ffi/trait.OsStringExt.html]

### Pattern 3: Deterministic Offline Default Resolution
**What:** Resolve the main worktree's full `HEAD` symbolic ref and verify that local branch exists; detached or unborn main state returns the locked unavailable result before remote selection. For a valid main branch, ask `for-each-ref` for that ref's `%(upstream:remotename)`; try that remote first, then `origin`, deduplicated. For each candidate, read `refs/remotes/<remote>/HEAD` with `symbolic-ref`, require a target under the exact `refs/remotes/<remote>/` prefix, retain the full suffix as the local branch name, and verify the target ref exists. If no candidate succeeds, return a typed unavailable cause and do not launch. [CITED: https://git-scm.com/docs/git-for-each-ref] [CITED: https://git-scm.com/docs/git-symbolic-ref] [CITED: https://git-scm.com/docs/git-remote] [VERIFIED: locked detached/unborn decision in `05-CONTEXT.md`]

Using the main worktree's branch makes remote preference independent of whether admission started at the main checkout, a subdirectory, a symlink, or a linked worktree. [VERIFIED: requirement REPO-01 and locked identity/default decisions]

Do not call `git remote set-head --auto` or `git remote show` without `-n`: the former queries the remote, and the latter queries remote heads unless `-n` is supplied. [CITED: https://git-scm.com/docs/git-remote]

### Pattern 4: Verified Default Worktree Ensure
**What:** Compare the resolved `refs/heads/<branch>` against every worktree record. Reuse the main record first, otherwise reuse the linked record that already has the branch. Existing external worktrees are registered as children but retain `managed_by_baude = false`. Only when neither exists should baude allocate a path from opaque repository/child keys and call Git. [CITED: https://git-scm.com/docs/git-worktree] [VERIFIED: locked decisions in `05-CONTEXT.md`]

If the local branch exists, use `git worktree add <path> <branch>`. If it does not exist but the already-verified remote target does, explicitly create it from that exact source with `git worktree add --track -b <branch> <path> <remote>/<branch>`. Rediscover and verify common-dir, path, and branch before persistence; never treat mere directory existence as successful reuse. [CITED: https://git-scm.com/docs/git-worktree]

### Pattern 5: Idempotent Primary Ensure
**What:** Make `(RepositoryKey, CheckoutRole::PrimaryDefault)` the uniqueness key, not display name or cwd. Admission first reserves/finds that child, persists active intent, then handles runtime state: focus a live session, replace/restart an exited session with resume enabled, spawn an absent session only when active intent is true, and do nothing for an intentionally idle child. [VERIFIED: locked primary lifecycle in `05-CONTEXT.md`] [VERIFIED: codebase `baude/src/app.rs`]

Local event handling is single-owner today, but all entry points still need one ensure function because startup restore, launch-directory admission, Open, and clone completion are separate call paths. [VERIFIED: codebase `baude/src/app.rs`]

### Pattern 6: Fail-Visible Versioned Load and Atomic Save
**What:** Return `Result<LoadOutcome, LoadError>` where `LoadOutcome` distinguishes `Missing`, `Legacy`, and `Current`. Inspect an explicit `schema_version`; absence means legacy only if the complete legacy shape deserializes. Unsupported versions and malformed JSON block restore and all automatic saves. [VERIFIED: current silent-default hazard in `baude-core/src/persist.rs`] [CITED: https://serde.rs/field-attrs.html]

**Atomic sequence:** serialize in memory; create a unique sibling temp; `write_all`; `flush`; `sync_all`; close; `rename(temp, destination)`; clean up only the temp on pre-rename failure. `sync_all` reports errors otherwise lost on drop, and same-mount `rename` replaces the target subject to documented platform behavior. [CITED: https://doc.rust-lang.org/std/fs/struct.File.html#method.sync_all] [CITED: https://doc.rust-lang.org/std/fs/fn.rename.html]

Do not spawn a newly admitted primary before the aggregate containing its repository, child, and active intent has saved successfully. This prevents a failed persistence write from leaving an untracked live process; a later spawn failure still leaves durable actionable intent for retry. [VERIFIED: locked durability/session decisions in `05-CONTEXT.md`]

### Anti-Patterns to Avoid
- **`--show-toplevel` as repository identity:** it identifies the selected worktree, not shared membership. Use common-dir plus main-first inventory. [CITED: https://git-scm.com/docs/git-rev-parse] [CITED: https://git-scm.com/docs/git-worktree]
- **Path-derived or display-name IDs:** aliases, moves, and duplicate basenames break identity. Persist opaque keys and observed path facts separately. [VERIFIED: locked identity decisions in `05-CONTEXT.md`]
- **String parsing of human worktree output:** user config and unusual paths make it unsafe. Parse `--porcelain -z`. [CITED: https://git-scm.com/docs/git-worktree]
- **Current branch or `main`/`master` as default:** current `HEAD` is not remote default, and conventional names are forbidden by the phase contract. [CITED: https://git-scm.com/docs/git-remote] [VERIFIED: locked default decision]
- **Directory-exists reuse:** Git may not register that directory or branch. Reuse only records proven by inventory. [CITED: https://git-scm.com/docs/git-worktree]
- **Serde defaults for structural corruption:** defaults are for defined missing-field compatibility, not for turning malformed files into empty state. [CITED: https://serde.rs/field-attrs.html]
- **Restore-then-unconditional-save:** current local and daemon restore paths save after loading; after a load error this would destroy evidence unless explicitly gated. [VERIFIED: codebase `baude/src/app.rs`, `bauded/src/manager.rs`]
- **Spawning every durable parent:** only `active_intent` primary children restart; intentional idle survives restart without a PTY. [VERIFIED: locked primary lifecycle]

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Repository/worktree topology | `.git`/gitfile parser | `rev-parse` + `worktree list --porcelain -z` | Git documents common/per-worktree layout and advises commands rather than layout assumptions. [CITED: https://git-scm.com/docs/git-worktree] |
| Default branch discovery | branch-name heuristics or network probe | local remote symbolic `HEAD` | Remote HEAD is optional local state; `set-head --auto` queries the remote. [CITED: https://git-scm.com/docs/git-remote] |
| Branch occupancy | path scans or branch string matching | worktree inventory and plain `worktree add` safeguards | Git refuses a non-forced duplicate branch checkout. [CITED: https://git-scm.com/docs/git-worktree] |
| Backend selection | backend stored on repository records | existing active `Workspace.backend` and `App::add_session` path | Workspace binding already isolates commands and state. [VERIFIED: codebase `baude-core/src/workspace.rs`, `baude/src/app.rs`] |
| Transactional state database | SQLite or journal subsystem | one versioned JSON aggregate + atomic replacement | Current storage is small and workspace/owner separated. [VERIFIED: codebase `baude-core/src/persist.rs`] |
| Custom atomic-rename package | new dependency | `File::sync_all` + same-directory `fs::rename` | Required primitives are in stable Rust. [CITED: https://doc.rust-lang.org/std/fs/struct.File.html#method.sync_all] [CITED: https://doc.rust-lang.org/std/fs/fn.rename.html] |

**Key insight:** Git topology and baude intent are complementary sources of truth. Git must answer “what exists now”; persisted opaque records must answer “what baude knows, owns, orders, and should reopen.” [CITED: https://git-scm.com/docs/git-worktree] [VERIFIED: requirements PERS-01/PERS-03]

## Runtime State Inventory

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | Four flat session files exist: `state.json` (17 rows), `state-claude.json` (12), `state-opencode.json` (6), and `daemon-state.json` (1). Existing fallback rules make `state-claude.json`, `state-opencode.json`, and legacy `daemon-state.json` active; `state.json` is dormant because the Claude primary exists. [VERIFIED: filesystem read; codebase `persist::load_for_workspace`] | Migrate only the source selected by current fallback precedence; never merge dormant legacy and workspace-primary files. Write migrated data to the workspace-specific primary filename. Preserve every valid field and unavailable record. |
| Live service config | No hierarchy lives in an external UI/service database in Phase 5; daemon session intent is the flat `daemon-state*.json` file. [VERIFIED: codebase `bauded/src/manager.rs`, `baude-core/src/persist.rs`] | Adapt daemon load/save to the versioned shared format and blocking load error, but defer repository APIs/projections to Phase 8. |
| OS-registered state | None identified: repository/session admission is not registered with launchd/systemd/Task Scheduler by project code. [VERIFIED: codebase search for persistence/workspace ownership] | None. |
| Secrets/env vars | `BAUDE_WORKSPACE`, `BAUDE_BACKEND`, `XDG_CONFIG_HOME`, and `XDG_DATA_HOME` choose workspace/backend and storage roots; no secret key name or env-var rename is part of the schema migration. [VERIFIED: codebase `workspace.rs`, `persist.rs`, `git.rs`] | Keep names and precedence unchanged; tests must isolate XDG roots and avoid touching real state. |
| Build artifacts | No matching checked-in or discovered `target/**/baude*` artifact was found before the test run; Cargo subsequently built ordinary test artifacts from current source. [VERIFIED: workspace glob; local `cargo test -p baude-core`] | Normal rebuild only; no installed-package data migration. |

The canonical runtime question is answered by the three selected active state files: after source edits, they still contain the legacy flat shape until each owning workspace/daemon successfully migrates and atomically writes its new primary file. [VERIFIED: filesystem read; codebase `persist::load_for_workspace`]

## Migration Contract

1. Resolve the exact source path using current primary-then-legacy fallback rules; never combine both. [VERIFIED: codebase `baude-core/src/persist.rs`]
2. Read bytes and parse explicitly. Missing means empty first run; malformed I/O/JSON means blocking error with the original file untouched. [VERIFIED: requirement PERS-04]
3. For each legacy session in source order, canonicalize an existing cwd, discover common-dir/main topology, and find-or-create one repository record. Missing/changed/invalid paths produce retained unavailable repository/child records rather than dropped sessions. [VERIFIED: locked migration/reconciliation decisions]
4. Find-or-create the checkout child by reconciled worktree identity, preserve `name`, `cwd`, `repo_root`, `branch`, `is_worktree`, `shell_open`, `archived`, and `archived_by_user`, set active intent because legacy records represented restorable sessions, and preserve source order as deterministic first-seen order. [VERIFIED: codebase `SavedSession`; locked migration decision]
5. Mark a legacy worktree baude-managed only when both persisted intent and Git topology prove it; never infer ownership from `is_worktree` alone. [VERIFIED: requirement PERS-03 and locked managed-state persistence]
6. Validate repository/child foreign keys and uniqueness before saving. Atomically write the new workspace-specific primary; retain the legacy fallback file. [VERIFIED: codebase workspace filename rules; requirement PERS-04]
7. Restore hierarchy, then call primary ensure only where migrated/current `active_intent` is true. A second load reads the current schema and creates no new keys, children, or sessions. [VERIFIED: locked restore/idempotence decisions]

## Common Pitfalls

### Pitfall 1: Entry-path-dependent remote preference
**What goes wrong:** Opening the main checkout and a linked worktree can resolve different defaults if each entry branch's upstream is consulted. [VERIFIED: requirement REPO-01 and Git per-worktree HEAD behavior]  
**How to avoid:** Always derive preferred upstream remote from the main worktree record's symbolic local branch, then try `origin`. [CITED: https://git-scm.com/docs/git-worktree] [CITED: https://git-scm.com/docs/git-for-each-ref]  
**Warning signs:** Discovery accepts a “selected branch” argument when resolving repository default.

### Pitfall 2: Stripping branch names incorrectly
**What goes wrong:** Splitting `origin/feature/a` on `/` loses a valid slash-containing branch name or confuses remote names. [CITED: https://git-scm.com/docs/git-symbolic-ref]  
**How to avoid:** Read the full symbolic target and strip the exact `refs/remotes/<remote>/` prefix.  
**Warning signs:** `rsplit('/')`, `splitn`, or `--short` output is used as the branch parser.

### Pitfall 3: Dangling cached remote HEAD
**What goes wrong:** A symbolic remote HEAD can exist while its target remote-tracking ref is absent, so worktree creation later fails or starts from an unintended fallback. Git says setting remote HEAD requires the target branch to exist. [CITED: https://git-scm.com/docs/git-remote]  
**How to avoid:** Verify the exact symbolic target locally before declaring the default resolved; otherwise persist unavailable state.  
**Warning signs:** Any successful `symbolic-ref` immediately becomes `Known` without target verification.

### Pitfall 4: Existing directory treated as a worktree
**What goes wrong:** Current `create_worktree` returns success for any existing computed directory. [VERIFIED: codebase `baude-core/src/git.rs`]  
**How to avoid:** Reuse only a matching Git inventory record; collision with an unregistered directory is an error. [CITED: https://git-scm.com/docs/git-worktree]  
**Warning signs:** `if dir.exists() { return Ok(dir) }` survives.

### Pitfall 5: Malformed load followed by automatic save
**What goes wrong:** Current loaders return default on read/parse failure, and both restore paths save afterward, which can replace corrupt evidence with empty state. [VERIFIED: codebase `baude-core/src/persist.rs`, `baude/src/app.rs`, `bauded/src/manager.rs`]  
**How to avoid:** Propagate a blocking load error and set a “persistence disabled until resolved/reloaded” guard.  
**Warning signs:** `.ok().and_then(...).unwrap_or_default()` or ignored `save()` results remain on state paths.

### Pitfall 6: Migration resurrects dormant legacy data
**What goes wrong:** Merging `state.json` with `state-claude.json` would reintroduce older sessions that current fallback behavior intentionally ignores. [VERIFIED: filesystem inventory; codebase `load_for_workspace`]  
**How to avoid:** Migrate only the selected source file and write the workspace primary.  
**Warning signs:** migration scans every `state*.json` glob.

### Pitfall 7: Parent saved after process spawn
**What goes wrong:** A save failure can leave a live PTY unknown to durable state; retry can spawn a duplicate. [VERIFIED: requirement REPO-03/PERS-04]  
**How to avoid:** Save admitted repository/child/active intent before spawn, then ensure exactly once by child key.  
**Warning signs:** `add_session` precedes the first successful aggregate save for a new admission.

### Pitfall 8: Missing metadata silently pruned
**What goes wrong:** Current restore skips missing cwd entries and then saves, permanently dropping them. [VERIFIED: codebase `baude/src/app.rs`, `bauded/src/manager.rs`]  
**How to avoid:** Keep degraded records and never run automatic `worktree prune`/repair. Git exposes locked/prunable/missing states for explicit recovery. [CITED: https://git-scm.com/docs/git-worktree]  
**Warning signs:** `continue` on `!cwd.exists()` without creating unavailable state.

## Code Examples

### Byte-Oriented Git Command Boundary
```rust
// Source: https://git-scm.com/docs/git-worktree
fn git_output(repo: &Path, args: &[&OsStr]) -> Result<std::process::Output, GitError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()?;
    if output.status.success() {
        Ok(output) // parse stdout bytes; decode stderr only for diagnostics
    } else {
        Err(GitError::Command { status: output.status, stderr: output.stderr })
    }
}
```

### Exact Remote-HEAD Prefix Handling
```rust
// Source: https://git-scm.com/docs/git-symbolic-ref
let head_ref = format!("refs/remotes/{remote}/HEAD");
let target = symbolic_ref_full(main_worktree, &head_ref)?;
let prefix = format!("refs/remotes/{remote}/");
let branch = target
    .strip_prefix(&prefix)
    .filter(|name| !name.is_empty())
    .ok_or(DefaultBranchError::UnexpectedTarget { remote, target })?;
verify_ref(main_worktree, &target)?;
```

### Fail-Visible Load API
```rust
// Source: https://serde.rs/field-attrs.html
enum LoadOutcome {
    Missing(StateFile),
    Migrated(StateFile),
    Current(StateFile),
}

fn load_named(file: &str) -> Result<LoadOutcome, LoadError> {
    // NotFound is the only empty-state case. Other I/O and JSON failures return Err.
    // Inspect schema_version before choosing the strict current or strict legacy decoder.
    todo!()
}
```

### Atomic Replacement
```rust
// Sources:
// https://doc.rust-lang.org/std/fs/struct.File.html#method.sync_all
// https://doc.rust-lang.org/std/fs/fn.rename.html
let bytes = serde_json::to_vec_pretty(state)?;
let temp = unique_sibling_path(path);
let mut file = File::options().write(true).create_new(true).open(&temp)?;
file.write_all(&bytes)?;
file.flush()?;
file.sync_all()?;
drop(file);
std::fs::rename(&temp, path)?;
```

### Idempotent Primary Dispatch
```rust
// Source: locked Phase 5 primary-session lifecycle
match runtime_session_for(primary_child.key) {
    Some(session) if !session.claude.is_exited() => focus(session.id),
    Some(session) => restart_in_child(session.id, /* resume */ true)?,
    None if primary_child.active_intent => spawn_in_child(primary_child.key, true)?,
    None => {} // intentionally sessionless parent remains idle
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Human `git worktree list` parsing | Stable `--porcelain -z` machine format | Git documents current format as stable across versions | Preserve unusual paths and parse flags/records deterministically. [CITED: https://git-scm.com/docs/git-worktree] |
| Inspecting `.git`/symlinks | `rev-parse --git-common-dir` and Git commands | Current Git guidance | Correct main/linked worktree separation and alternate layouts. [CITED: https://git-scm.com/docs/git-rev-parse] [CITED: https://git-scm.com/docs/git-worktree] |
| Session path as identity | Persisted opaque aggregate key + observed Git paths | Phase 5 design | Parent survives path/topology changes and closed sessions. [VERIFIED: locked decisions] |
| Flat sessions, silent default on parse error | Explicit versioned envelope and result-valued load | Phase 5 design | Migration becomes testable; corruption blocks destructive overwrite. [VERIFIED: requirement PERS-02/PERS-04] |
| Direct destination writes | Sibling temp, flush/sync, rename | Phase 5 design | Failed pre-rename writes leave the old destination intact. [CITED: https://doc.rust-lang.org/std/fs/fn.rename.html] |
| Display-name duplicate handling | Primary uniqueness by stable repository/child role | Phase 5 design | Repeated admission focuses/restarts rather than suffixing another session. [VERIFIED: requirement REPO-03] |

**Deprecated/outdated:**
- `repo_root()` as canonical repository identity, direct `std::fs::write`, load-error-to-default, missing-path skip, directory-exists worktree reuse, and `unique_name()` as the only duplicate defense must not remain on Phase-5 admission/persistence paths. [VERIFIED: codebase `git.rs`, `persist.rs`, `app.rs`]

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|

All claims in this research were verified against current project source/runtime state or cited from official Git, Rust, and Serde documentation; no unverified claims remain.

## Open Questions (RESOLVED)

1. **What minimum Git version does baude officially support?**
   - Resolution: Phase 5 supports the currently tested Git baseline: the Git versions exercised by the project's macOS and Ubuntu CI, with local validation on Git 2.50.1. This is a behavioral compatibility floor rather than an unverified lower numeric version claim. Admission must feature-detect the required porcelain behavior, including NUL-delimited `git worktree list --porcelain -z` and the other structured commands used by discovery/default resolution, and return an actionable `DefaultBranchUnavailable`/Git compatibility error when required behavior is absent or malformed instead of falling back to human output, lossy parsing, branch guessing, or mutation. [VERIFIED: local `git --version`; codebase `.github/workflows/ci.yml`] [CITED: https://git-scm.com/docs/git-worktree]

2. **How should non-Unix persisted paths be encoded if Windows support is later added?**
   - Resolution: Phase 5 byte-preserving path behavior is Unix-focused for the current macOS and Ubuntu targets. Implement the byte adapter behind `cfg(unix)` and keep path serialization isolated behind `PersistedPath`; Windows path encoding and its platform adapter are explicitly deferred until Windows becomes a supported target. Do not claim cross-platform byte portability from the Unix representation. [VERIFIED: codebase `.github/workflows/ci.yml`; `.planning/REQUIREMENTS.md`] [CITED: https://doc.rust-lang.org/std/os/unix/ffi/trait.OsStringExt.html]

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|-------------|-----------|---------|----------|
| Git CLI | Identity/default/worktree integration tests and runtime | ✓ | 2.50.1 | None; actionable admission failure |
| Rust compiler | Implementation/tests | ✓ | 1.98.0 | Project CI toolchain |
| Cargo | Test/build | ✓ | 1.98.0 | Project CI |
| Claude Code | Active Claude workspace primary-session UAT | ✓ | 2.1.251 | Automated tests use a harmless stand-in command |
| OpenCode | Active OpenCode workspace primary-session UAT | ✓ | 1.18.25 | Automated tests use a harmless stand-in command |

Availability and versions were probed locally on 2026-08-30. [VERIFIED: local command probes]

**Missing dependencies with no fallback:** None. [VERIFIED: local command probes]

**Missing dependencies with fallback:** None. [VERIFIED: local command probes]

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test harness via Cargo 1.98.0 [VERIFIED: local command probe] |
| Config file | none; workspace `Cargo.toml` and inline `#[cfg(test)]` modules [VERIFIED: codebase glob/source] |
| Quick run command | `cargo test -p baude-core git:: && cargo test -p baude-core persist::` |
| Full suite command | `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` [VERIFIED: codebase `.github/workflows/ci.yml`] |

The current `cargo test -p baude-core` baseline passed 140 tests on 2026-08-30. [VERIFIED: local test run]

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| REPO-01 | Main/subdirectory/symlink/linked paths converge; separate same-basename repos do not | real-Git integration | `cargo test -p baude-core git::tests::admission_identity` | ❌ Wave 0 helper/tests in existing `git.rs` module |
| REPO-02 | Main default reused; separate default worktree created when needed; one primary spawn | unit + real-Git integration | `cargo test -p baude repository_admission` | ❌ Wave 0 App orchestration seam |
| REPO-03 | repeated Open/launch/clone focus live primary or restart exited primary without duplicate | unit | `cargo test -p baude primary_idempotence` | ❌ Wave 0 App test seam |
| REPO-04 | non-default main HEAD/worktree/files remain unchanged while default child is ensured | real-Git integration | `cargo test -p baude-core git::tests::default_worktree` | ❌ Wave 0 |
| REPO-06 | missing/dangling remote HEAD, detached/unborn/no-remote states retain unavailable parent and run no mutating/network command | real-Git integration | `cargo test -p baude-core git::tests::default_branch` | ❌ Wave 0 |
| PERS-01 | repository/child keys, managed flag, branch, order, active intent, and session settings round-trip per workspace | unit | `cargo test -p baude-core persist::tests::current_round_trip` | ❌ Wave 0 |
| PERS-02 | selected local/daemon legacy fixture migrates once, preserves fields/order, and does not merge dormant fallback | fixture + real-Git integration | `cargo test -p baude-core persist::tests::legacy_migration` | ❌ Wave 0 fixtures |
| PERS-03 | missing, moved, branch-changed, detached, locked/prunable facts become degraded and block launch/reuse until reconciled | real-Git integration | `cargo test -p baude-core git::tests::reconciliation` | ❌ Wave 0 |
| PERS-04 | malformed/truncated/unsupported state errors; failed pre-rename write retains old bytes; temp does not become state | unit/integration | `cargo test -p baude-core persist::tests::atomic` | ❌ Wave 0 |

### Required Real-Git Matrix
- Identity: main, nested subdirectory, symlink, linked worktree, path with spaces/newline where supported, and duplicate basenames. [CITED: https://git-scm.com/docs/git-worktree]
- Default: upstream remote preferred over origin, origin fallback, slash branch, missing/dangling remote HEAD, local branch absent but remote target present, default checked out in main/linked/neither, and detached/unborn main states that must remain unavailable. [CITED: https://git-scm.com/docs/git-remote] [CITED: https://git-scm.com/docs/git-symbolic-ref] [VERIFIED: locked detached/unborn decision]
- Reconciliation: missing path, externally changed branch, locked and prunable records, and common-dir mismatch; no automatic prune/repair. [CITED: https://git-scm.com/docs/git-worktree]
- Persistence: missing file, valid legacy, valid current, malformed JSON, unsupported version, unwritable temp, failed rename, old-file preservation, and primary-vs-legacy fallback precedence. [VERIFIED: requirement PERS-02/PERS-04]
- Session ensure: live, exited, absent-active, absent-idle, spawn failure, save failure before spawn, repeated admission, startup plus launch-dir duplicate, and clone completion duplicate. [VERIFIED: locked primary lifecycle; codebase `baude/src/app.rs`]

### Sampling Rate
- **Per task commit:** touched crate/module test filter, normally `cargo test -p baude-core git::` or `cargo test -p baude-core persist::`.
- **Per wave merge:** `cargo test`.
- **Phase gate:** `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` green on macOS and Ubuntu before `/gsd-verify-work`. [VERIFIED: codebase `.github/workflows/ci.yml`]

### Wave 0 Gaps
- [ ] Add a standard-library temporary-repository fixture/helper in the existing `git.rs` test module; tests must set local user config and clean up their unique directory. [VERIFIED: current `git.rs` tests contain no real-repository fixture]
- [ ] Add injectable state-root/path helpers so persistence tests never touch `~/.config/baude`; current private `config_base` reads process environment directly. [VERIFIED: codebase `baude-core/src/persist.rs`]
- [ ] Add legacy JSON fixtures covering all eight `SavedSession` fields and both workspace/daemon base names. [VERIFIED: codebase `SavedSession`, `Manager::STATE_BASE`]
- [ ] Extract a pure primary-dispatch decision or injectable spawn seam so `App` idempotence tests do not launch real PTYs. [VERIFIED: codebase `App::add_session` directly calls `Pty::spawn`]
- [ ] Add an atomic-save failure seam (unique temp/path injection or filesystem permissions) to prove destination preservation. [VERIFIED: requirement PERS-04]

## Security Domain

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | Phase is local/workspace repository admission; project security remains VPN/single-user and adds no endpoint. [VERIFIED: `.planning/PROJECT.md`, phase boundary] |
| V3 Session Management | no (ASVS web-session sense) | Coding-agent process lifecycle is covered by domain idempotence, not web authentication sessions. [VERIFIED: phase boundary] |
| V4 Access Control | no | Daemon/remote authorization/projection is deferred to Phase 8. [VERIFIED: `05-CONTEXT.md`] |
| V5 Input Validation | yes | Existing-path canonicalization, Git-owned topology/ref verification, exact symbolic-ref prefix validation, and typed persistence decoding. [CITED: https://git-scm.com/docs/git-rev-parse] [CITED: https://git-scm.com/docs/git-symbolic-ref] |
| V6 Cryptography | no | No credential, token, encryption, or signature feature is introduced. [VERIFIED: phase scope/requirements] |

### Known Threat Patterns for Rust + Git CLI + Local JSON
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Command/option injection through paths or ref names | Tampering / Elevation | `Command` argv arrays only, no shell interpolation; fixed command options; Git ref verification and exact prefix checks. [VERIFIED: codebase command style] [CITED: https://git-scm.com/docs/git-rev-parse] |
| Symlink/path alias creates duplicate or wrong repository | Spoofing | Canonicalize existing input, then verify common-dir and worktree membership before mutation. [CITED: https://doc.rust-lang.org/std/fs/fn.canonicalize.html] [CITED: https://git-scm.com/docs/git-worktree] |
| Stale persisted path authorizes mutation | Tampering | Re-run discovery and compare repository/child identity immediately before any Git/process mutation. [VERIFIED: requirement PERS-03] |
| Malformed/truncated state erased by startup | Tampering / Repudiation | Blocking typed load error, no auto-save, old file untouched, atomic replacement. [VERIFIED: requirement PERS-04] |
| Duplicate primary processes edit one checkout | Tampering / Denial of Service | Stable role uniqueness plus one ensure path and reservation before spawn. [VERIFIED: requirement REPO-02/REPO-03] |
| Managed path collision crosses repository boundary | Tampering | Allocate path components from opaque repository/child keys and verify rediscovered common-dir before recording. [VERIFIED: locked identity decisions] |

## Sources

### Primary (HIGH confidence)
- https://git-scm.com/docs/git-worktree — main/linked model, main-first stable porcelain `-z`, occupancy safeguards, add semantics, locked/prunable states, and no-layout-assumption guidance.
- https://git-scm.com/docs/git-rev-parse — canonical absolute path output, common directory, and top-level distinction.
- https://git-scm.com/docs/git-symbolic-ref — full symbolic-ref lookup and detached/non-symbolic exit behavior.
- https://git-scm.com/docs/git-remote — optional local remote HEAD, network-query behavior, and target requirements.
- https://git-scm.com/docs/git-for-each-ref — `upstream:remotename`, `symref`, and `worktreepath` structured fields.
- https://git-scm.com/docs/git-clone — initial active-branch checkout and configurable remote name.
- https://doc.rust-lang.org/std/fs/fn.canonicalize.html — existing-path absolute canonicalization and symlink resolution.
- https://doc.rust-lang.org/std/os/unix/ffi/trait.OsStringExt.html — byte-vector/`OsString` conversion on supported Unix targets.
- https://doc.rust-lang.org/std/fs/struct.File.html#method.sync_all — durable file sync and close-error handling.
- https://doc.rust-lang.org/std/fs/fn.rename.html — replacement and same-mount constraint.
- https://serde.rs/field-attrs.html — defined semantics of `#[serde(default)]` and aliases.
- Current source and runtime evidence: `Cargo.toml`, `Cargo.lock`, `.github/workflows/ci.yml`, `baude-core/src/{git,persist,workspace,session}.rs`, `baude/src/app.rs`, `bauded/src/manager.rs`, and selected `~/.config/baude/{state*,daemon-state*}.json` files. [VERIFIED: codebase/filesystem]

### Secondary (MEDIUM confidence)
- None.

### Tertiary (LOW confidence)
- None.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — exact lockfile/tool versions and official standard-library/Git documentation were checked.
- Architecture: HIGH — locked decisions map directly to current core/App seams and authoritative Git behavior.
- Persistence/migration: HIGH — current failure modes and actual flat files were inspected; atomic primitives are officially documented.
- Primary-session orchestration: HIGH contract / MEDIUM test seam — behavior is locked, but `App` currently lacks an injectable PTY spawn seam.
- Pitfalls: HIGH — each critical risk is present in current source or follows directly from official Git behavior.

**Research date:** 2026-08-30
**Valid until:** 2026-09-29 (stable Git/Rust domain; recheck if supported platforms or state contract changes)
