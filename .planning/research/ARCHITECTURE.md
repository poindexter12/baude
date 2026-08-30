# Architecture Research

**Domain:** Repository hierarchy and Git worktree session management in an existing Rust TUI/daemon workspace
**Researched:** 2026-08-30
**Confidence:** HIGH for integration with the current code; MEDIUM for the product meaning of “default branch”

## Standard Architecture

### System Overview

The minimal design is to add a repository aggregate above the existing `Session`, not to turn repositories into fake sessions and not to replace the session engine. A repository is persistent metadata and an ownership boundary; PTYs, status, metadata polling, archiving, notifications, and backend behavior remain session concerns.

```text
┌────────────────────────────────────────────────────────────────────┐
│ Clients                                                            │
│  ┌───────────────────────┐       ┌───────────────────────────────┐ │
│  │ baude local hierarchy │       │ baude remote hierarchy / PWA │ │
│  │ selection + rendering │       │ REST poll + PTY WebSocket    │ │
│  └───────────┬───────────┘       └──────────────┬────────────────┘ │
├──────────────┼──────────────────────────────────┼──────────────────┤
│ Ownership / orchestration                                          │
│  ┌───────────▼────────────┐       ┌──────────────▼───────────────┐ │
│  │ App                    │       │ bauded Manager               │ │
│  │ local repositories     │       │ daemon repositories         │ │
│  │ local sessions         │       │ daemon-owned sessions       │ │
│  └───────────┬────────────┘       └──────────────┬───────────────┘ │
├──────────────┴───────────────────────────────────┴──────────────────┤
│ baude-core                                                         │
│  ┌────────────────┐  ┌────────────────┐  ┌──────────────────────┐ │
│  │ repository     │  │ git discovery  │  │ session / backend    │ │
│  │ parent model   │  │ + safe actions │  │ PTY + metadata       │ │
│  └───────┬────────┘  └───────┬────────┘  └──────────────────────┘ │
│          └────────────────────┼─────────────────────────────────── │
│                         persistence                               │
│              state-<workspace>.json / daemon-state-<workspace>.json│
└────────────────────────────────────────────────────────────────────┘
```

Repository ownership stays workspace-local. A Claude workspace and an OpenCode workspace may register the same filesystem repository independently, just as they currently maintain isolated session pools and state files. Local and daemon repository collections also stay separate: equal-looking paths refer to different host filesystems and must not be merged by the TUI.

### Core Invariants

1. A repository parent is unique within one owner (`App` or `Manager`) by its canonical **main worktree path**, not by display name.
2. A repository has exactly one main/default session whose `cwd` is the main worktree. Opening the same repository is idempotent and selects or returns that session.
3. Every linked-worktree session belongs to the repository whose main worktree Git reports. Existing `Session.repo_root` is the relation key; no second parent pointer is needed.
4. Git is authoritative for current worktree path, branch, detached/locked/prunable state, and main-vs-linked classification. Persistence records intent and UI membership, not a second Git database.
5. Repository parents own no PTY and never contribute to waiting/busy counts, notifications, usage totals, or archive transitions.
6. A remove-worktree operation validates cleanliness before killing/removing the session. “Close and keep” and “remove from disk” are distinct commands in both local and daemon paths.
7. Backend choice remains process/workspace-wide. Repository operations call the same active backend spawn path already used by sessions; repository records never carry or override a backend.

### Component Responsibilities

| Component | Change | Responsibility | Implementation |
|-----------|--------|----------------|----------------|
| `baude-core/src/repository.rs` | **New** | Shared repository parent value type | `Repository { id, name, root }`; keep it PTY-free and UI-free |
| `baude-core/src/git.rs` | **Modified** | Discover canonical repository/worktree topology and perform safe Git operations | Add `discover_repository`, `list_worktrees`, `current_branch`, branch validation; parse `git worktree list --porcelain -z` |
| `baude-core/src/persist.rs` | **Modified** | Persist repository membership alongside sessions and migrate legacy flat state | Add `SavedRepository`; add `repositories` with `#[serde(default)]`; derive parents from old sessions when absent |
| `baude-core/src/session.rs` | **Unchanged structurally** | Continue owning PTY, live status, metadata, shell, archive and permission state | Retain `repo_root`, `branch`, and `is_worktree`; do not introduce a parent enum or repository PTY |
| `baude/src/app.rs` | **Modified** | Own local repository parents, create/select flattened hierarchy, dispatch context actions, route remote actions | Add `repositories`, repository IDs, idempotent `open_repository`, and repository/session selection variants |
| `baude/src/ui.rs` | **Modified** | Render parent and nested child rows without changing session status visuals | Render from flattened sidebar nodes; parent is one line, session remains two lines and is indented |
| `baude/src/remote.rs` | **Modified** | Poll daemon repository hierarchy with old-daemon fallback; expose new worktree/remove calls | Add `RemoteRepositoryInfo`; prefer `GET /repositories`, fall back to `GET /sessions` |
| `bauded/src/manager.rs` | **Modified** | Authoritatively own daemon repository parents and all repository/session mutations | Add repository collection and methods `open_repository`, `create_worktree_session`, `forget_repository`, `close_session` |
| `bauded/src/api.rs` | **Modified** | Add repository-oriented REST surface while preserving session endpoints | Add `/repositories` routes and safe worktree removal option; retain existing `/sessions` contracts |
| PWA code | **No required hierarchy change for first cut** | Continue consuming flat `/sessions` | New API is additive; hierarchy can be adopted later without blocking v2.0 TUI/daemon parity |

## Recommended Project Structure

```text
baude-core/src/
├── repository.rs        # NEW: shared PTY-free repository parent
├── git.rs               # MODIFY: topology discovery + create/remove safety
├── persist.rs           # MODIFY: repository records + legacy migration
├── session.rs           # KEEP: existing live child model
└── lib.rs               # MODIFY: export repository module

baude/src/
├── app.rs               # MODIFY: local/remote hierarchy and actions
├── remote.rs            # MODIFY: repository API client and fallback
└── ui.rs                # MODIFY: flattened hierarchical rendering

bauded/src/
├── manager.rs           # MODIFY: daemon repository aggregate owner
└── api.rs               # MODIFY: repository and close/remove endpoints
```

### Structure Rationale

- **One new core module only:** the parent model is shared, but `App` and `Manager` retain their existing orchestration roles. A generic repository service would have to abstract local rendering, daemon locking, API serialization, and PTY spawning and would add more indirection than value.
- **Git facts remain in `git.rs`:** callers should receive one parsed discovery result instead of composing `repo_root`, branch, and worktree-list commands differently in the TUI and daemon.
- **Persistence remains one aggregate file per owner/workspace:** repositories and sessions must be saved atomically together so parent-child membership cannot drift between files.
- **Session remains a concrete live process:** introducing `enum Node { Repository, Session }` inside core session code would contaminate status and PTY paths with parent-only cases.

## Recommended Data Model

```rust
// baude-core/src/repository.rs
pub struct Repository {
    pub id: u64,          // runtime identity; not persisted
    pub name: String,     // display name derived from root
    pub root: PathBuf,    // canonical main worktree path
}

// baude-core/src/git.rs
pub struct RepositoryDiscovery {
    pub main_worktree: PathBuf,
    pub selected_worktree: PathBuf,
    pub selected_branch: Option<String>,
    pub selected_is_linked: bool,
    pub worktrees: Vec<WorktreeInfo>,
}

pub struct WorktreeInfo {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub detached: bool,
    pub locked: bool,
    pub prunable: bool,
}

// baude-core/src/persist.rs
pub struct State {
    #[serde(default)]
    pub repositories: Vec<SavedRepository>,
    #[serde(default)]
    pub sessions: Vec<SavedSession>,
}

pub struct SavedRepository {
    pub root: PathBuf,
}
```

Do not persist runtime repository IDs, display names, aggregate status, or derived branch state. On restore, repositories receive fresh IDs and sessions attach by normalized `SavedSession.repo_root == Repository.root`. Existing session fields already preserve the worktree path, branch hint, worktree classification, shell state, and archive state.

### Meaning of “Default Session”

Use **the main worktree session** as the default child. Its branch label is whatever `git worktree list` reports for that main worktree. Do not silently switch the user’s existing main worktree to `origin/HEAD`: that can fail when another branch is checked out or when changes are present and would make “open repository” a destructive action.

This is MEDIUM confidence because the milestone wording says “default branch.” If product requirements specifically mean the remote’s advertised default branch rather than the main worktree’s current branch, that needs a phase-specific decision. The safe implementation would require either an explicit checkout confirmation or another linked worktree; it must not be hidden inside discovery.

## Architectural Patterns

### Pattern 1: Repository Aggregate with Existing Sessions as Children

**What:** `App` and `Manager` each own `Vec<Repository>` and `Vec<Session>`. `Session.repo_root` associates a child with a parent. The parent controls creation/forget actions; the child remains independently addressable for chat, PTY, restart, archive, and notifications.

**When to use:** For all local and daemon repository operations.

**Trade-offs:** This introduces a small amount of lookup-by-path. It avoids invasive changes to the mature session engine and keeps all current session APIs viable. At the expected single-user scale, linear scans are preferable to new maps and synchronization complexity.

### Pattern 2: Discover, Normalize, Then Mutate

**What:** Every open/create path first asks Git for topology, normalizes to the main worktree, checks the owner’s repository collection, and only then spawns or mutates.

```text
user path (main, linked worktree, or subdirectory)
    ↓
git rev-parse --show-toplevel
    ↓
git worktree list --porcelain -z
    ↓
canonical main worktree + selected worktree + branch facts
    ↓
find-or-create Repository → find-or-spawn default Session
```

**When to use:** Startup launch-directory detection, `n` open, clone completion, daemon `POST /repositories`, legacy migration, and worktree creation.

**Trade-offs:** Each operation runs a few short Git subprocesses. That is acceptable for interactive repository creation and safer than interpreting `.git` files/directories. Discovery should not run every frame.

The current `repo_root()` only returns the selected worktree’s top-level path. That is insufficient when the user opens a linked worktree: `--show-toplevel` identifies that linked tree, not the main parent. The new discovery function should parse `git worktree list --porcelain -z`; Git documents this format as stable for scripts and documents the main worktree as the first record.

### Pattern 3: Flattened Sidebar View Model

**What:** Keep hierarchical storage but generate an ordered, flat sequence for keyboard movement and rendering.

```rust
enum SelId {
    LocalRepository(u64),
    LocalSession(u64),
    RemoteRepository(u64),
    RemoteSession(u64),
}

enum SidebarNode<'a> {
    Repository { id: SelId, repo: &'a Repository },
    Session { id: SelId, depth: u8 },
    Section(&'static str), // render-only, not selectable
}
```

**When to use:** `ordered_ids`, `move_selection`, cycling, sidebar draw, context-sensitive keys, and selection repair after deletion/polling.

**Trade-offs:** Rendering and navigation share one ordering function, preventing row/order drift. Repository rows need explicit content behavior because they have no PTY. Use Enter on a repository to select/attach its default session; use `w` to create a child worktree; use `e` to open the main root; reserve `x` for forgetting the repository after confirmation.

Ordering should be deterministic:

1. local repository groups, alphabetically by repository name then root;
2. each default/main session first;
3. linked-worktree children alphabetically by branch/path;
4. remote repository groups in a distinct remote section;
5. archived children after active children within their repository, with fully archived repository groups after active groups.

Waiting sessions must continue flashing in place. Do not status-sort children.

### Pattern 4: Additive Daemon API with Compatibility Adapters

**What:** Add repository-native routes and preserve existing flat session routes.

Recommended API:

| Method | Route | Semantics |
|--------|-------|-----------|
| `GET` | `/repositories` | Nested `RepositoryInfo[]`, each with its sessions |
| `POST` | `/repositories` | Register canonical repo and ensure/return its default session; idempotent |
| `DELETE` | `/repositories/{repo_id}` | Kill children and forget parent; never delete the Git repository |
| `POST` | `/repositories/{repo_id}/worktrees` | Validate branch, create/reuse managed worktree, spawn child session |
| `DELETE` | `/sessions/{id}` | Existing behavior: close session, keep linked worktree |
| `DELETE` | `/sessions/{id}?remove_worktree=true` | Remove only a clean linked worktree; return `409 Conflict` without closing when dirty |

Keep `GET /sessions` as the flat compatibility view used by the PWA and older TUI clients. Keep `POST /sessions` as an adapter: no `worktree` delegates to `open_repository`; with `worktree` it opens/fetches the parent then delegates to repository worktree creation. New response fields added to `SessionInfo` should be optional/defaulted in `RemoteInfo`.

**Trade-offs:** Two representations exist temporarily, but both are projections of one `Manager` state. This is less risky than forcing the PWA and every existing client to adopt hierarchy in the same phase.

## Git Discovery and Safety

### Discovery Commands

| Need | Command | Reason |
|------|---------|--------|
| Selected working tree root | `git -C <path> rev-parse --show-toplevel` | Handles subdirectories and linked worktrees |
| Repository topology | `git -C <root> worktree list --porcelain -z` | Stable machine format; identifies main first and linked paths/branches/flags |
| Current branch fallback | `git -C <worktree> symbolic-ref -q --short HEAD` | Cleanly returns nonzero for detached HEAD |
| Shared repository identity, if needed | `git -C <root> rev-parse --path-format=absolute --git-common-dir` | Same common directory across linked worktrees |
| Branch validation | `git check-ref-format --branch <name>` | Rejects invalid/option-like branch names before path creation |
| Dirty check | `git -C <worktree> status --porcelain` | Includes tracked and untracked changes, matching removal safety intent |

Do not inspect `.git` directly. In a linked worktree it is a file pointing into the main repository’s administrative area, and Git explicitly recommends using its commands rather than assuming `$GIT_DIR`/`$GIT_COMMON_DIR` layout.

### Create/Reuse Rules

`create_worktree` currently returns success whenever its computed directory exists, without proving that Git knows the directory or that it is attached to the requested branch. Replace that shortcut with topology lookup:

1. Validate the branch.
2. List registered worktrees.
3. If that branch is already attached, return its registered path and let the owner select/reuse its session.
4. If the target path exists but is not registered, fail; never adopt an arbitrary directory silently.
5. Otherwise run `git worktree add` and rediscover to verify the resulting path/branch.

The managed directory currently keys only by repository basename, so two repositories named `api` collide. Use a deterministic key derived from the canonical main root/common directory in the managed path (for example a small stable hex hash plus sanitized basename), while continuing to honor already persisted worktree paths. Do not relocate existing worktrees during migration.

### Close/Remove Rules

```text
Close and keep:
  kill PTY → remove SavedSession/runtime Session → keep Git worktree

Remove from disk:
  validate linked child → check dirty → if dirty: 409/error, no mutation
                    ↓ clean
  kill PTY → git worktree remove → remove Session → save aggregate
```

The current local flow removes the session before checking `is_dirty`; that violates “removal blocked” because a dirty worktree still loses its running session. Move validation before `remove_session`. The daemon currently has no remove-worktree operation; route it through the same `Manager::close_session(id, RemoveMode)` decision so local and remote semantics match.

Git itself refuses removal of an unclean worktree without `--force`; retain the explicit precheck for a useful UI/API error and still call `git worktree remove` without force as defense in depth.

## Data Flow

### Local Open / Clone Flow

```text
`n` path or completed clone
    ↓
App::open_repository(path)
    ↓
git::discover_repository(path)
    ↓
find Repository by canonical main_worktree
    ├─ found → find/select default session
    └─ absent → create Repository → add_session(main_worktree, ...)
                                      ↓
                           backend::active().prepare_cwd/spawn_plan
                                      ↓
                               persist aggregate
```

Clone remains asynchronous in `App`; only its completion target changes from `open_repo_session` to `open_repository`. The backend spawn path is unchanged.

### Local Worktree Flow

```text
select repository or any child → resolve parent main root
    ↓ `w`, branch input
git validate/list/create/rediscover
    ↓
App::add_session(worktree_path, main_root, branch, true, ...)
    ↓
save repositories + sessions → select child
```

Context behavior should resolve a repository from either its parent row or any child row, so `w` remains useful without forcing the user to move selection to the parent.

### Daemon Open / Worktree Flow

```text
TUI RemotePoller POST /repositories
    ↓ workspace guard (`GET /info`) remains mandatory
api handler
    ↓ lock Manager only for synchronous mutation
Manager::open_repository / create_worktree_session
    ↓ git discovery + backend spawn + aggregate save
RepositoryInfo / SessionInfo response
    ↓
GET /repositories poll updates remote hierarchy
    ↓
PTY attach and all per-session endpoints remain `/sessions/{id}/...`
```

Git and PTY operations are currently blocking under the daemon mutex. This milestone does not need a concurrency redesign, but handlers must continue the established rule of never holding the manager lock across `.await`. Repository creation is infrequent; avoid introducing an async job system unless clone-over-daemon is added later.

### Restore and Migration Flow

```text
load workspace-scoped State
    ↓
repositories present?
    ├─ yes → discover/normalize each saved root
    └─ no  → derive unique roots from legacy SavedSession.repo_root
    ↓
allocate runtime repository IDs
    ↓
restore existing saved sessions and attach by normalized root
    ↓
for each repository missing a main/default session, spawn one
    ↓
atomic save in new shape (legacy file remains read fallback only)
```

### State Management

```text
Owner aggregate: { repositories, sessions }
       ↓ mutation methods only
App/Manager actions → Git/PTY mutation → update vectors → one save
       ↓ projections
sidebar nodes / RepositoryInfo / flat SessionInfo / notifications
```

Do not let UI rendering or API serialization mutate discovery state. Reconcile only at open, restore, successful create/remove, or an explicit refresh action.

## Persistence and Migration Concerns

1. **Backward-compatible field addition:** `State.repositories` must use `#[serde(default)]`; keep all existing `SavedSession` fields unchanged so v0.14 files deserialize.
2. **One-time parent derivation:** if no repository records exist, deduplicate saved `repo_root` paths after Git discovery. A legacy linked-worktree `repo_root` may already be the intended main root for baude-created sessions, but discovery must correct records created by opening linked worktrees directly.
3. **No persisted IDs:** runtime IDs currently restart from 1. Persisting parent IDs while session IDs remain ephemeral creates false stability. Relationships use normalized paths in the file and runtime IDs in memory/API.
4. **Workspace isolation:** continue `load_for_workspace`/`save_for_workspace` and the separate `state` vs `daemon-state` base names. Never migrate repositories into a global cross-workspace file.
5. **Atomic aggregate write:** while modifying persistence, write JSON to a sibling temporary file, flush/close, then rename. The current direct write plus “parse failure means default state” can turn a partial write into silent loss of every repository/session.
6. **Missing paths:** skip a missing root/session as current restore does, but report it. Do not run `git worktree prune` automatically; a temporarily unavailable worktree can be locked or recoverable.
7. **Existing managed paths:** retain saved `cwd` values. The new collision-proof path scheme applies only to newly created worktrees.
8. **Rollback compatibility:** an old binary will ignore no unknown fields only if Serde is reading into its known struct (the default behavior here), but it will rewrite state without repositories on save. Treat downgrade after first v2 write as unsupported unless explicitly tested.

## Selection and Rendering Details

- Repository rows are selectable but not attachable themselves. Enter redirects to the default child; `e`, `w`, and `x` act on the parent context.
- Session rows retain all current status icons, two-line metadata, waiting timers, selection background, activity/info modals, shell handling, and raw attach behavior.
- Indent child rows and their metadata line by one hierarchy level. Keep the selected gutter aligned across parent and child rows.
- Parent rows should display a folder/repository icon, repository name, and perhaps child count. Do not synthesize a waiting status: duplicate parent alarms would weaken the “which session needs me” signal.
- Repository selection should render a lightweight repository summary or immediately redirect Enter to the default child. Do not attempt to draw a nonexistent PTY.
- `ordered_ids` should become a projection of `sidebar_nodes`; keyboard movement filters selectable nodes from the same projection. This prevents the current risk of rendering headers that navigation does not account for.
- If a remote daemon is offline, keep its stale repository groups visible under the existing offline header, exactly as stale sessions are retained now.
- Status counts, rate windows, desktop notifications, and usage cost remain folds over sessions only.

## Scaling Considerations

| Scale | Architecture Adjustments |
|-------|--------------------------|
| Current single user, tens of repositories/sessions | Vectors and linear lookup by canonical path/ID are simplest and sufficient |
| Hundreds of repositories/worktrees | Cache the flattened sidebar and add `HashMap<PathBuf, repo_id>` indexes inside each owner; keep persisted shape unchanged |
| Multiple hosts/owners | Introduce an explicit owner/host ID before attempting cross-daemon aggregation; paths alone are not globally meaningful |

### Scaling Priorities

1. **First bottleneck:** repeated rebuilding of large sidebar line buffers every frame. Cache only after measurement; do not preemptively complicate the aggregate.
2. **Second bottleneck:** blocking Git commands under the daemon mutex. Move discovery/create/remove to a command worker with reservation state only if concurrent remote mutations become observable.

## Anti-Patterns

### Anti-Pattern 1: Fake Repository Sessions

**What people do:** Put parent rows in `Vec<Session>` with a dummy PTY or optionalize every session field.

**Why it is wrong:** Status, notifications, archive, shell, resize, metadata, and permission code would all acquire meaningless repository branches.

**Do this instead:** Add a small repository parent collection and flatten only at the presentation boundary.

### Anti-Pattern 2: Treating `--show-toplevel` as Repository Identity

**What people do:** Use `repo_root(path)` for both main and linked worktrees.

**Why it is wrong:** In a linked worktree it returns that worktree’s top level, producing duplicate parents and incorrect removal roots.

**Do this instead:** Discover the selected worktree, then parse the repository’s complete worktree list and normalize to its main record.

### Anti-Pattern 3: Persisting a Nested Runtime Tree

**What people do:** Serialize repository objects containing full session objects and runtime IDs.

**Why it is wrong:** It duplicates session serialization, complicates migration, and couples file shape to API/view shape.

**Do this instead:** Persist two flat record lists in one aggregate; rebuild runtime links by canonical root.

### Anti-Pattern 4: Unifying Local and Remote Parents by Path

**What people do:** Merge `/code/foo` from the TUI host with `/code/foo` from the daemon host.

**Why it is wrong:** They may be unrelated filesystems and different process owners.

**Do this instead:** Keep owner scope in selection identity and render distinct local/remote sections.

### Anti-Pattern 5: Close Before Safety Check

**What people do:** Kill/drop the session, then discover that its worktree is dirty.

**Why it is wrong:** “Removal blocked” still destroys useful process state and surprises the user.

**Do this instead:** Validate target and cleanliness first; mutate PTY, Git, memory, and persistence only after all checks pass.

### Anti-Pattern 6: Reimplementing Git Metadata Parsing

**What people do:** Read `.git`, `HEAD`, or `$GIT_DIR/worktrees` files directly.

**Why it is wrong:** Main and linked worktrees have different private/common administrative paths; paths can be relative, repaired, locked, or prunable.

**Do this instead:** Use `rev-parse`, `symbolic-ref`, and stable worktree porcelain output.

## Integration Points

### External Services

| Service | Integration Pattern | Notes |
|---------|---------------------|-------|
| Git CLI | Blocking subprocesses in `baude-core::git` | Authoritative topology and safety layer; do not parse `.git` internals |
| Claude Code / OpenCode | Existing `backend::active()` spawn/prepare flow | Repository hierarchy must not choose a backend; active workspace remains the isolation boundary |
| Remote `bauded` | REST hierarchy/list/action calls plus existing per-session WebSocket | Run existing workspace guard before repository creation as before session creation |

### Internal Boundaries

| Boundary | Communication | Notes |
|----------|---------------|-------|
| `App` ↔ `git` | Direct synchronous calls for local actions | Clone remains background-threaded; discovery after clone completion |
| `App` ↔ `persist` | Whole aggregate load/save | Local state remains separate from daemon state |
| `App` ↔ `RemotePoller` | REST snapshots/actions | Prefer hierarchy endpoint, preserve old `/sessions` fallback |
| `api` ↔ `Manager` | Short mutex-protected method calls | Never retain guard across `.await` |
| `Manager` ↔ `git` | Centralized daemon-side mutations | Daemon host paths and Git state are authoritative for remote actions |
| repository parent ↔ `Session` | `Session.repo_root == Repository.root` | Main child: `!is_worktree && cwd == root`; linked children: `is_worktree` |
| UI/notifications ↔ owner state | Read-only projections | Parents render hierarchy; only sessions produce attention/status signals |

## Dependency-Aware Build Order

1. **Core Git topology and tests**
   - Add parser for `worktree list --porcelain -z`, discovery result, branch validation, and reuse checks.
   - Test main path, subdirectory, linked worktree, detached branch, duplicate basename, existing registered worktree, and dirty removal.
   - Rationale: every later layer depends on one canonical definition of repository membership.

2. **Core repository and persistence schema/migration**
   - Add `repository.rs`, `SavedRepository`, defaulted `State.repositories`, legacy derivation, and atomic save.
   - Test old JSON load, new round trip, workspace-specific filenames, and no ID persistence.
   - Rationale: both local and daemon owners need the same parent model and migration contract.

3. **Daemon Manager aggregate**
   - Add repository ownership, idempotent open, default-session invariant, worktree creation/reuse, and validate-before-close/remove.
   - Keep all existing per-session behavior unchanged.
   - Rationale: APIs should expose tested domain methods, not implement orchestration in handlers.

4. **Daemon API projection and compatibility**
   - Add repository routes and nested `RepositoryInfo`; adapt existing `POST /sessions`; add safe remove mode with `409` on dirty.
   - Test new and old route behavior, workspace isolation, and that dirty removal leaves the session alive.
   - Rationale: establishes the remote contract before changing the TUI client.

5. **Local App aggregate and actions**
   - Replace `open_repo_session` with idempotent `open_repository`; restore parents before sessions; route `n`, clone completion, `w`, `e`, and `x` through repository context.
   - Rationale: reuses proven core behavior and mirrors Manager semantics without touching rendering yet.

6. **Remote client integration**
   - Add hierarchy polling/actions, workspace guard reuse, and old-daemon flat fallback.
   - Rationale: local and remote node sources must exist before unified selection can be built.

7. **Selection projection and rendering**
   - Expand selection identity, implement one flattened sidebar-node order, then render parent/child rows and context help.
   - Re-run stable ordering, waiting flash, archive, selection repair, pane focus, local PTY, and remote attach tests.
   - Rationale: UI comes last because it depends on both local and remote hierarchy shapes and carries the largest regression surface.

8. **End-to-end migration and parity verification**
   - Start from v0.14 local and daemon state files; verify parents/default children after restart in Claude and OpenCode workspaces.
   - Exercise open-main, open-linked, clone, create/reuse branch, close-keep, remove-clean, block-dirty, daemon restart, old-daemon fallback, and offline stale rendering.

## Research Flags

- **Default branch semantics:** Decide whether “default” means the main worktree’s checked-out branch (recommended safe minimum) or remote `origin/HEAD`. This is the only material product ambiguity.
- **Repository forgetting semantics:** Confirm whether `x` on a parent merely forgets/kills children or is disallowed while linked children exist. It must never delete the main checkout.
- **Daemon clone support:** Current clone is performed by the TUI host and then its destination path is sent to the daemon. That only works when TUI and daemon share the filesystem. If remote clone is required, it needs a separate daemon endpoint/background job and is beyond the minimal hierarchy integration.
- **Submodules:** Git documents incomplete worktree support for superprojects with submodules. Do not promise managed worktree behavior there without phase-specific tests.

## Sources

- Existing project architecture and requirements: `.planning/PROJECT.md` (HIGH)
- Existing Git/session/persistence/TUI/daemon sources listed in the research scope, plus `baude/src/remote.rs` (HIGH)
- Existing code graph query, especially `App`, `git.rs`, `create_worktree`, `repo_root`, selection, and UI call relationships (HIGH; extracted from current source)
- Git worktree documentation, current manual: https://git-scm.com/docs/git-worktree (HIGH)
- Git rev-parse documentation, current manual: https://git-scm.com/docs/git-rev-parse (HIGH)
- Git symbolic-ref documentation, current manual: https://git-scm.com/docs/git-symbolic-ref (HIGH)
- Context7 Git HTML docs `/git/htmldocs`, worktree porcelain and rev-parse option definitions (HIGH)

---
*Architecture research for: baude v2.0 repository worktree management*
*Researched: 2026-08-30*
