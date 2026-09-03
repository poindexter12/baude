# Pitfalls Research

**Domain:** Persistent repository parents and managed Git worktree lifecycle in an existing TUI/daemon
**Researched:** 2026-08-30
**Confidence:** HIGH

## Critical Pitfalls

### Pitfall 1: Treating a session path as repository identity

**What goes wrong:**
A main worktree and each linked worktree become separate repository parents, or two unrelated repositories with the same directory name share a managed-worktree directory. Moving a checkout, opening it through a symlink, or restoring a path with different normalization can create another parent and another default-branch session.

**Why it happens:**
The current model stores `cwd`, `repo_root`, `branch`, and `is_worktree` on every session. `repo_root()` uses `rev-parse --show-toplevel`, which deliberately returns the current worktree's top level, not the shared repository. `create_worktree()` keys storage only by repository basename and a lossy sanitized branch. Git instead distinguishes the per-worktree Git directory from the common Git directory and exposes authoritative worktree membership through `git worktree list --porcelain -z`.

**How to avoid:**
- Introduce persisted, opaque `RepositoryId` and `WorktreeId` values; never use display names or vector positions as identity.
- On admission, canonicalize the input path, resolve `git rev-parse --path-format=absolute --git-common-dir`, and reconcile it with `git worktree list --porcelain -z`.
- Persist the main worktree path separately from repository identity. A repository parent must survive a temporarily missing path and report that state rather than silently disappear.
- Derive child ownership from Git's worktree inventory, not from path-prefix tests. Managed/unmanaged is baude metadata layered on top of Git membership.
- Use a collision-resistant repository directory key, such as an opaque ID, and an opaque worktree directory component. Keep branch names only as labels.

**Warning signs:**
- The same repository appears twice after opening a linked worktree or symlink.
- Two repositories named `api` map to the same `$XDG_DATA_HOME/baude/worktrees/api` directory.
- `foo/bar` and `foo-bar` map to one path.
- Code infers a parent with `Path::starts_with`, `file_name()`, or `sanitize(branch)`.

**Phase to address:**
**Phase 1 — repository domain model, identity, and migration.** Identity must be stable before persistence, UI hierarchy, or lifecycle operations depend on it.

---

### Pitfall 2: Guessing the default branch or destructively forcing it

**What goes wrong:**
Opening an existing repository starts a session on whichever branch happens to be checked out, hard-codes `main`/`master`, performs a network query unexpectedly, or switches a dirty main worktree and disrupts the user's work. Detached, unborn, no-remote, multiple-remote, renamed-remote, and stale `origin/HEAD` cases produce incorrect or unsafe behavior.

**Why it happens:**
Fresh `git clone` checks out the branch selected by the remote's `HEAD`, but an existing checkout's current `HEAD` is not proof of its default branch. `refs/remotes/<remote>/HEAD` is optional and cached; `git remote set-head --auto` queries the remote. The remote need not be named `origin`, and a local-only repository may have no remote default at all.

**How to avoid:**
- Define and document a deterministic, offline-first discovery policy. Recommended order: fresh clone's checked-out symbolic `HEAD`; a configured primary remote's `refs/remotes/<remote>/HEAD`; an unambiguous remote-HEAD symbolic ref; current local symbolic `HEAD` as a fallback; otherwise require user choice.
- Return a typed result such as `Known(branch, source) | Ambiguous(candidates) | Detached | Unborn`, not a guessed string.
- Never run `switch`, `checkout`, `reset`, `pull`, or `fetch` merely to open a repository parent.
- If the discovered default branch is already checked out in a registered worktree, attach the default session to that worktree. If not, create an explicit linked worktree or ask the user rather than mutating a dirty main checkout.
- Surface the discovery source and stale/ambiguous state in diagnostics. Make a network refresh an explicit action.

**Warning signs:**
- Tests cover only `origin/main`.
- Opening a repo changes `HEAD` or working files.
- `git branch --show-current` is called “default branch.”
- Discovery invokes `remote show` without `-n`, causing hangs or credential prompts.
- Empty repositories or detached HEADs collapse to a blank branch and still launch.

**Phase to address:**
**Phase 1 — repository discovery contract**, with integration cases completed in **Phase 2 — Git lifecycle**.

---

### Pitfall 3: Reimplementing linked-worktree rules with path existence and retry-any-error logic

**What goes wrong:**
Baude “reuses” an arbitrary existing directory, attaches the wrong branch, masks permission/disk/corruption failures as “branch already exists,” or tries to check out a branch that Git has already checked out elsewhere. Stale Git administrative records and locked/prunable worktrees are mishandled.

**Why it happens:**
The current helper returns success whenever the target directory exists. Otherwise it tries `worktree add -b`, then treats every failure as evidence that the branch exists and retries without `-b`. Git permits one non-forced checkout of a local branch across linked worktrees, records linked-worktree metadata under the common Git directory, and has explicit `locked`, `prunable`, `repair`, and `prune` states.

**How to avoid:**
- Validate requested branch names with `git check-ref-format --branch` before deriving any path.
- Query local refs and `git worktree list --porcelain -z` first. Distinguish: new local branch, existing local branch, unique remote-tracking branch, branch already checked out, detached worktree, stale registration, and invalid branch.
- Choose the start point explicitly. A new branch should start from the repository's selected default/base ref, not whatever `HEAD` happened to mean in the caller's worktree.
- Parse Git exit status/stderr into domain errors; never retry all errors as a different operation.
- Reuse a directory only when Git's inventory proves it belongs to this common repository and the expected branch. Otherwise fail closed with repair guidance.
- Do not use `--force` in normal lifecycle code. Respect locked worktrees and expose `repair`/`prune` as explicit recovery, not automatic destructive cleanup.
- Keep branch deletion separate from worktree removal; removing a worktree must not imply deleting its branch.

**Warning signs:**
- `if dir.exists() { return Ok(dir) }` remains in creation code.
- Any failed `-b` command triggers an unconditional second `worktree add`.
- `--force`, direct `.git/worktrees` edits, or `rm -rf` appears in the happy path.
- A worktree's persisted `branch` differs from `symbolic-ref --short HEAD`.

**Phase to address:**
**Phase 2 — authoritative Git worktree lifecycle and typed errors.**

---

### Pitfall 4: Dirty-check failure opens the deletion gate

**What goes wrong:**
A Git error is interpreted as “clean,” the agent process is killed before the user learns removal is blocked, or files change between the preflight and removal. Untracked files, submodule dirt, conflicts, and in-progress Git operations are overlooked. The UI may remove the session record while the worktree remains, leaving confusing orphaned state.

**Why it happens:**
The current `is_dirty()` returns `false` on every Git error. The TUI's remove path closes the session before checking dirtiness. Although `git worktree remove` itself refuses an unclean worktree without `--force`, that final safeguard does not preserve session continuity and does not turn preflight failure into a safe user experience. `git status` can also contend for the index when run in background automation.

**How to avoid:**
- Replace `bool` with `Result<WorktreeSafety>` containing at least `Clean`, `Dirty { staged, unstaged, untracked, conflicts, submodules }`, `OperationInProgress`, and `Unknown(error)`; only `Clean` enables removal.
- Use stable machine output (`git --no-optional-locks status --porcelain=v2 -z --untracked-files=all`) and do not let user config hide submodule changes. Treat unknown as blocked.
- Sequence removal as one domain operation: reserve/mark removing; stop accepting input; request process termination; wait for exit with timeout; re-check safety; invoke plain `git worktree remove`; verify Git inventory and path; then delete the session/worktree metadata and persist.
- If any step fails, retain or restore the child record with an actionable error. “Close session, keep worktree” must remain a distinct safe action.
- Serialize mutations for a repository so create/remove cannot race. Re-check immediately before Git removal; rely on Git's non-force refusal as the final guard.
- Explicitly document ignored generated files: Git does not consider ignored files dirty. If baude promises preservation of all files, add a separate filesystem policy; otherwise promise preservation of Git-visible uncommitted work only.

**Warning signs:**
- A safety function contains `unwrap_or(false)` or maps command failure to clean.
- The session disappears before the remove command succeeds.
- Tests mock only modified tracked files, not staged, untracked, conflicts, submodules, missing paths, and Git command failures.
- Removal code contains `--force` or removes directories directly.

**Phase to address:**
**Phase 2 — safe close/remove transaction.** This is a release-blocking safety gate.

---

### Pitfall 5: Persisting the new hierarchy as a lossy extension of the flat session list

**What goes wrong:**
Repository parents disappear when their sessions close, children attach to the wrong parent after restart, a schema change causes all state to load as empty, or a crash truncates JSON and silently resets the workspace. Migration respawns duplicate sessions or accidentally claims pre-existing worktrees as baude-managed.

**Why it happens:**
Current state is only `Vec<SavedSession>`. Deserialization and I/O failures both become `State::default()`, so incompatibility and corruption are indistinguishable from a first run. Saves write directly to the destination. New non-defaulted fields would make legacy files fail to deserialize, while inferring repositories afresh on every restore can change identity.

**How to avoid:**
- Create a versioned envelope with separate `repositories`, `worktrees`, and `sessions`. References use stable IDs; paths and Git observations are attributes.
- Add `#[serde(default)]` only where absence has a clear migration meaning. Implement explicit v1-flat to v2-hierarchy migration, write tests using real legacy fixtures, and retain a backup until the migrated state has been read successfully.
- Save atomically: serialize and validate, write a same-directory temp file, flush/sync as appropriate, rename, and preserve the previous valid file. Surface parse/write errors; never silently replace corrupt state with empty state.
- Reconcile at startup: persisted intent says what baude manages; Git inventory says what exists now. Missing, moved, branch-changed, locked, and externally removed worktrees become visible degraded states, not dropped rows or automatic deletion.
- Migration should group legacy sessions by common Git directory, deduplicate equivalent paths, preserve archive/shell state, and mark old worktrees managed only when there is strong evidence (for example, under baude's managed root plus verified Git membership).
- Make restore idempotent. It must not auto-create another default session when a matching live/restored session already exists.

**Warning signs:**
- `serde_json::from_str(...).ok().unwrap_or_default()` remains the only load path.
- New persisted fields lack migration fixtures.
- Repository parents are reconstructed solely by grouping current session strings.
- A crash during save produces a zero-byte or partial primary file.
- Startup logs “restored 0” after a schema change without an error.

**Phase to address:**
**Phase 1 — versioned persistence and migration**, before UI work; **Phase 5 — restart/recovery verification** hardens it.

---

### Pitfall 6: Using display names to tolerate duplicate sessions instead of preventing them

**What goes wrong:**
Repeated Open, clone completion, startup restore, concurrent API requests, or TUI-plus-daemon routing launches multiple agents in the same checkout. They can edit the same files concurrently while the UI merely labels them `repo`, `repo (2)`, and so on.

**Why it happens:**
Both local and daemon managers currently enforce only unique display names. Creation has no identity uniqueness constraint or idempotency key. With persistent parents, “open repository” also implies “ensure default session,” creating more paths to the same spawn.

**How to avoid:**
- Define a session uniqueness key for this milestone: `(workspace/backend owner, worktree_id, session role)`. Opening a repository should be an idempotent ensure operation that focuses/returns the existing default session.
- Reserve the key before performing Git or PTY work. Concurrent requests for the same key should join the pending operation or return conflict, not spawn twice.
- Add API idempotency for create/open actions, or at minimum enforce uniqueness server-side under the repository mutation coordinator.
- Do not merge local and remote entities solely by path string; paths are host-scoped. The daemon is authoritative for daemon-owned repository/session identities.
- Keep duplicate display names legal across different repositories, but show enough parent context to distinguish them.

**Warning signs:**
- `unique_name()` is the only duplicate defense.
- Double-pressing Open creates `(2)`.
- Two simultaneous `POST /sessions` calls both succeed for one checkout.
- One worktree has multiple live PTYs without an explicit multi-session feature.

**Phase to address:**
**Phase 1 — identity/invariants** and **Phase 2 — atomic ensure/create**; verify remote idempotency in **Phase 4**.

---

### Pitfall 7: Holding the global manager/UI lock across Git, filesystem, network, or process work

**What goes wrong:**
One slow clone, status scan, worktree add/remove, process shutdown, or remote default-branch query freezes all sessions and HTTP handlers. Conversely, moving work outside the lock without a reservation allows duplicate creates and remove/create races.

**Why it happens:**
The daemon exposes `Arc<Mutex<Manager>>`, and current synchronous `create()` performs repository discovery, `git worktree add`, preparation, and PTY spawn as one method likely called while the manager is locked. The existing clone path already recognizes that clone must leave the UI thread, but worktree operations can be equally slow. Git's own docs warn background `status` can contend on index locks unless optional locks are disabled.

**How to avoid:**
- Use the established rule “decide under lock, act outside it, commit under lock.” Under the lock, validate invariants and insert a `PendingOperation` reservation keyed by repository/worktree/session identity. Release it for Git/process work, then reacquire to commit or roll back.
- Serialize destructive Git mutations per repository, not globally. Read-only probes may run concurrently with `--no-optional-locks` where appropriate.
- Put blocking `std::process::Command`, status scans, waits, and filesystem traversal on worker threads/`spawn_blocking`, never the Tokio executor or TUI event loop.
- Give operations IDs, cancellation semantics, timeouts where safe, and visible progress/errors. Persist only committed intent, or persist an explicit recoverable pending state.
- Test with barriers so two creates and create-vs-remove interleave deterministically.

**Warning signs:**
- A `MutexGuard<Manager>` remains live across `Command::output`, process wait, clone, or PTY spawn.
- The whole TUI stops repainting during worktree creation or dirty checks.
- Fixes simply drop the lock without adding pending reservations.
- Intermittent `index.lock`, “already checked out,” or duplicate-session failures appear under rapid input.

**Phase to address:**
**Phase 2 — operation coordinator and lifecycle state machine**, then **Phase 4 — daemon async parity/load tests**.

---

### Pitfall 8: Bolting parent rows onto session-only selection

**What goes wrong:**
Keyboard actions target the previous child when a repository parent is visibly selected; collapse/reorder makes index-based selection jump; Enter, editor, close, archive, restart, and worktree shortcuts perform nonsensical or destructive operations. Async remote refresh can invalidate a selection mid-modal.

**Why it happens:**
The current `SelId` represents only local or remote sessions, and nearly every action assumes selection resolves to a session. Ordering is a flat active-local/active-remote/archive list. A hierarchy introduces non-session rows, expansion state, contextual actions, and repository-scoped commands.

**How to avoid:**
- Define a typed selection target: host scope plus `Repository(id)` or `Session(id)` (and, if independently selectable, `Worktree(id)`). Resolve it at action execution time.
- Build one flattened `VisibleRow` projection from the domain tree and use it for rendering, keyboard movement, mouse hit-testing, and help text. Store selected stable ID, never row index.
- Specify a context-action matrix before coding: repository parent (`Enter` expand/focus default, `w` create child, `x` remove parent only when allowed); child session (`Enter` attach, `x` close/keep/remove worktree); remote equivalents; unavailable actions disabled with reasons.
- Parent close must never recursively remove dirty children through a generic confirmation. Summarize affected children and require all safety checks.
- On deletion/collapse/refresh, choose a deterministic neighbor: next visible sibling, previous sibling, parent, then first row. Bind confirmation modals to stable IDs and revalidate after confirmation.
- Preserve the hard-won “waiting flashes in place” behavior; status updates must not reorder the hierarchy.

**Warning signs:**
- New parent rows use fake session IDs or sentinel numbers.
- Rendering and navigation independently flatten the tree.
- Handlers call `selected()` and silently do nothing or act on stale data for a parent.
- Selection is stored as `usize`.
- Help text lists one key meaning while context changes its destructive effect without an explicit label.

**Phase to address:**
**Phase 3 — hierarchical projection, typed selection, and context-action matrix.**

---

### Pitfall 9: Implementing lifecycle safety only in the local TUI

**What goes wrong:**
Local removal blocks dirty worktrees, but daemon/PWA deletion merely kills a session or removes a worktree without the same check. The TUI sends a local path to a remote daemon where it means something else. A feature appears complete in terminal testing but remote users see flat rows, wrong shortcuts, or weaker guarantees.

**Why it happens:**
Current local worktree close has keep/remove choices, while daemon `remove()` only kills and drops a session. Remote session data exposes session fields but not persistent repository entities or lifecycle operation state. In configured-daemon mode, creation is routed remotely, and remote paths live on the daemon host.

**How to avoid:**
- Put repository discovery, worktree inventory, dirty policy, and lifecycle state transitions in `baude-core` or a shared service contract. TUI and daemon call the same semantics; clients do not reproduce safety decisions.
- Make the daemon the authority for remote repositories. APIs use repository/worktree IDs after admission, not arbitrary client-local paths for follow-up actions.
- Separate endpoints/actions for `close session`, `remove managed worktree`, and `forget repository`; return typed conflict/safety errors and operation status.
- Extend daemon DTOs and remote client models with stable repository/worktree IDs and hierarchy. Older clients must ignore additive fields; destructive endpoints should not silently change old `DELETE /sessions/{id}` semantics.
- Test a parity matrix across local TUI, remote TUI, REST, and PWA for open, create, close-keep, remove-clean, reject-dirty, restart, restore, missing path, and duplicate request.

**Warning signs:**
- Safety checks are methods on `App` only.
- Daemon delete still has no worktree-aware mode while UI claims “remove.”
- Remote requests carry paths selected on the TUI host.
- Local and daemon code have separate branch-discovery algorithms.
- PWA tests cover session deletion but not worktree keep/remove distinctions.

**Phase to address:**
**Phase 4 — daemon/API/PWA parity**, after the shared core lifecycle is stable in Phase 2.

---

### Pitfall 10: Trusting persisted metadata over live Git, or live Git over management intent

**What goes wrong:**
Externally moved/deleted worktrees remain clickable; externally changed branches keep stale labels; a stale registration is automatically pruned; or every external worktree is suddenly treated as baude-managed and offered for deletion. Restore may spawn a process in a directory that exists but no longer belongs to the expected repository.

**Why it happens:**
There are two sources of truth with different responsibilities. Git owns current worktree/ref topology. Baude owns repository display/order, which worktrees it created, session association, archive state, and whether a missing entity should remain visible for recovery. Collapsing either source into the other loses necessary information.

**How to avoid:**
- Reconcile rather than overwrite: compare persisted intent with `worktree list --porcelain -z`, common-dir identity, and per-worktree symbolic HEAD at startup and before destructive actions.
- Model health explicitly: `Ready`, `Missing`, `MovedCandidate`, `BranchChanged`, `Locked`, `Prunable`, `Detached`, `InvalidRepo`, `BusyOperation`.
- Never auto-prune, auto-repair, auto-delete directories, or auto-adopt unmanaged worktrees. Offer explicit repair/relink/forget actions with previews.
- Validate `cwd` against expected repository/worktree identity before spawning or restarting. Directory existence alone is insufficient.
- Refresh branch labels from Git, but retain a diagnostic record of the persisted expectation when they disagree.

**Warning signs:**
- Restore checks only `saved.cwd.exists()`.
- Missing entries silently vanish from saved state on the next save.
- Startup automatically invokes `worktree prune`.
- `is_worktree: bool` alone grants permission to delete a path.

**Phase to address:**
**Phase 1 — reconciliation model**, **Phase 2 — pre-operation revalidation**, and **Phase 5 — recovery/UAT scenarios**.

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Keep `Vec<SavedSession>` and derive parents at render time | Small schema change | Parents cannot persist independently; migration and ownership remain ambiguous | Never for v2.0 |
| Use path or display name as ID | Easy lookup | Breaks on moves, symlinks, collisions, remote hosts, and duplicate names | Never |
| Keep `bool is_dirty` | Simple confirmation branch | Git failures become unsafe answers; no actionable UX | Never |
| Retry a different Git command after any failure | Fewer probes | Hides real failures and creates wrong refs/worktrees | Never |
| Save JSON directly to the primary file | Minimal code | Crash can destroy all hierarchy metadata | Only in tests with persistence disabled |
| Duplicate core logic in TUI and daemon | Faster first UI demo | Safety and behavior drift across local/remote | Never for lifecycle rules |
| Auto-run `prune`, `repair`, or `--force` | Makes demos self-heal | Can discard recovery evidence or override Git safeguards | Never automatically; explicit recovery action only |
| Poll dirty state every frame | Live indicator | Slow repositories freeze UI and contend on the index | Never; event/TTL refresh and on-demand preflight |

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| Git worktree inventory | Parse human `git worktree list` columns | Parse `git worktree list --porcelain -z`; retain locked, prunable, bare, and detached states |
| Git branch input | Sanitize arbitrary text into a branch/path | Validate with `git check-ref-format --branch`; use an opaque path component independent of the branch label |
| Default branch | Assume `origin/main` or current HEAD | Use a documented typed discovery chain; no network or checkout side effect on open |
| Dirty state | Use human `status` or suppress untracked files | Use `--no-optional-locks status --porcelain=v2 -z --untracked-files=all`; treat errors as blocked |
| Git repository identity | Use `--show-toplevel` | Resolve absolute common Git dir plus authoritative worktree inventory; scope identity to host/workspace |
| Clone completion | Treat `.git` directory existence as success | Verify repository/common-dir, valid HEAD/default state, and destination identity; `.git` may be a file |
| Daemon REST | Reuse session DELETE for worktree deletion | Keep close, remove-worktree, and forget-repository as explicit operations with typed conflicts |
| Remote TUI | Send local filesystem paths | Send daemon-issued repository/worktree IDs; paths shown are informational and daemon-host scoped |
| Submodules | Promise full support because status sees some dirt | Preserve safety, but flag linked worktrees of superprojects with submodules as a research/test area; Git documents incomplete support |
| Process shutdown | Kill then immediately delete | Stop input, terminate, wait/reap, re-check, remove, verify, then commit metadata |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Running `git status` synchronously during render/input | Keystroke lag and frozen spinner | Worker/`spawn_blocking`, TTL cache, on-demand safety refresh | One large monorepo or slow filesystem |
| Holding global manager mutex during Git/process operations | All API requests and session polling stall | Reservation state + per-repository coordinator + act outside lock | First slow Git command or concurrent client |
| Rebuilding and sorting hierarchy separately in each UI path | Selection jumps; excess allocations | Single stable `VisibleRow` projection reused for render/navigation/hit-test | Dozens of repos/sessions or frequent remote polls |
| Full network remote query on every open/refresh | Credential prompts and long hangs | Offline cached symbolic refs; explicit refresh action | Offline/VPN loss or slow SSH auth |
| Saving whole state on every poll | I/O churn and corruption exposure | Save only committed model changes; debounce noncritical UI state; atomic replacement | Frequent status/meta ticks |

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Passing branch names as options | A branch beginning with `-` can alter command interpretation | Validate with `check-ref-format --branch`; use `--` where the Git command supports it; pass argv without shell interpolation |
| Building managed paths directly from branch/repo text | Traversal, collisions, or deletion outside managed root | Opaque IDs; canonical containment check; verify Git membership before removal |
| Shell-interpolating repository paths or refs | Command injection through user-controlled names | `Command` argument arrays only; no `sh -c` for Git lifecycle |
| Trusting client-provided remote paths | Operate on an unintended daemon-host directory | Admit once under server policy, then use server-issued IDs and revalidate identity |
| Adding `--force` to make removal reliable | Dirty or locked worktrees can be destroyed | Never force in managed happy path; fail closed and provide explicit manual recovery guidance |

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| Parent and default session look like duplicate rows | Users cannot tell container from runnable agent | Distinct glyph/indentation; parent summary; clear default-child label |
| One `x` key means kill, close, remove, and forget | Destructive ambiguity | Context-specific prompt naming the exact entity/path and whether files remain |
| Dirty refusal appears only after session is killed | User loses continuity while files remain | Preflight first; keep session live on blocked/unknown; offer close-and-keep separately |
| Async create/clone steals selection | Typing goes to a new session | Stable ID selection; completion only selects when sidebar policy says so |
| Missing worktrees silently disappear | User cannot repair moved storage | Keep degraded row with relink/forget guidance |
| Remote path looks local | Editor/open actions confuse or target wrong host | Label remote host scope and disable local editor action with a precise reason |
| Ambiguous default silently picks one | Agent works on wrong base | Show candidates/source and require selection once, then persist the choice |

## "Looks Done But Isn't" Checklist

- [ ] **Repository identity:** Opening the main worktree, linked worktree, symlinked path, and same-basename second repository yields the correct distinct/stable parents.
- [ ] **Default branch:** Verify fresh clone, renamed default, non-`origin` remote, multiple remotes, stale/missing remote HEAD, detached HEAD, unborn repo, and local-only repo without mutating files or HEAD.
- [ ] **Creation:** Verify new local branch, existing local branch, unique remote branch, branch already checked out, invalid ref, lossy-name collision, pre-existing directory, locked/prunable record, and two concurrent creates.
- [ ] **Dirty safety:** Verify staged, unstaged, untracked, conflict, dirty submodule, in-progress operation, Git error, disappearing path, and mutation between preflight/removal all block safely.
- [ ] **Removal:** Verify process has exited, plain Git removal succeeds, inventory/path postconditions hold, branch remains, and metadata changes only after success.
- [ ] **Persistence:** Migrate real v1 fixtures; survive restart, corrupt/truncated JSON, missing/moved worktrees, branch changed externally, and interrupted atomic save without dropping parents.
- [ ] **No duplicates:** Repeated open, repeated clone completion, restart restore, rapid double keypress, and concurrent POST return/focus one default session.
- [ ] **Selection:** Parent/child, collapsed/expanded, deleted selected row, async remote refresh, modal confirmation, mouse, and keyboard all resolve the same stable target.
- [ ] **Daemon parity:** Local TUI, remote TUI, REST, and PWA share create/keep/remove/reject-dirty semantics and typed errors.
- [ ] **Host scope:** No local client path is mistaken for a daemon-host path; follow-up actions use daemon IDs.
- [ ] **UI responsiveness:** Large-repo status, clone, worktree add/remove, process wait, and remote discovery do not block the TUI or Tokio executor/global manager lock.
- [ ] **Recovery:** Locked, prunable, missing, and moved entries are visible and recoverable; nothing runs prune/repair/force automatically.

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Duplicate parent/session | MEDIUM | Stop the later duplicate safely; resolve both paths to common-dir/worktree identity; merge metadata by stable IDs; retain one session; atomic save |
| Stale/missing worktree path | LOW–MEDIUM | Keep degraded row; inspect `worktree list --porcelain`; offer relink/`worktree repair` or forget; never prune automatically |
| Existing directory collision | MEDIUM | Do not delete it; verify whether Git registers it; choose a new opaque managed path or explicitly adopt after validation |
| Corrupt state file | MEDIUM | Stop automatic overwrite; retain corrupt file; load last valid backup; reconcile with Git; write a new versioned state only after review/validation |
| Dirty removal attempted | LOW if force was not used | Keep/restart session, show categorized dirty state, let user commit/stash/remove files, then rerun preflight |
| Metadata removed but worktree remains | MEDIUM | Discover it in Git inventory as unmanaged/orphaned; verify prior baude ID/path evidence; explicitly re-adopt or keep unmanaged |
| Files deleted with force | HIGH | Stop writes; recover from commits/reflogs/backups/editor history; untracked content may be unrecoverable—hence force must be absent |
| Main/linked worktree moved externally | MEDIUM | Use explicit `git worktree repair` flow, update canonical paths, revalidate common-dir identity, then persist |

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Wrong repository identity | Phase 1: domain model and persistence | Common-dir/symlink/same-name fixtures produce stable correct IDs |
| Wrong/default branch side effects | Phase 1 discovery + Phase 2 lifecycle | Default-branch matrix passes with no HEAD/worktree mutation |
| Linked-worktree rule reimplementation | Phase 2: Git lifecycle | Real temporary repos cover checked-out, remote, locked, prunable, invalid, and collision cases |
| Dirty-check fail-open | Phase 2: safe removal | Every dirty/error/race case blocks; metadata/session retained on failure |
| Lossy migration/corrupt persistence | Phase 1 + Phase 5 recovery | Legacy fixture migration and crash/corruption restart tests pass |
| Duplicate sessions | Phase 1 invariant + Phase 2 operations | Repeated/concurrent ensure operations yield one PTY/session key |
| Global-lock blocking/races | Phase 2 + Phase 4 daemon load | Barrier tests prove reservations; slow Git does not block unrelated polling/requests |
| Parent selection/action ambiguity | Phase 3: hierarchical TUI | Action matrix tests and UAT target stable IDs under reorder/collapse/delete |
| Local/remote parity gap | Phase 4: daemon/API/PWA | Shared parity suite passes for all lifecycle and safety operations |
| Stale metadata reconciliation | Phase 1 + Phase 5 | Missing/moved/changed/locked worktrees render degraded and recover without auto-destruction |

### Recommended phase ordering

1. **Domain model, repository discovery, versioned persistence, and reconciliation** — establishes identity, migration, and invariants before they leak into UI/API contracts.
2. **Shared Git lifecycle and operation coordinator** — implements validated create, idempotent session ensure, safe close/remove, typed errors, and nonblocking concurrency.
3. **TUI hierarchy and contextual selection/actions** — projects the stable model without embedding lifecycle logic in UI handlers.
4. **Daemon, remote TUI, REST, and PWA parity** — exposes the same core semantics with host-scoped IDs and async operation reporting.
5. **Restart, recovery, race, and cross-surface UAT hardening** — exercises stale metadata, interrupted persistence, external Git changes, duplicates, and dirty-removal matrices.

## Sources

### Official Git documentation — HIGH confidence

- [git-worktree](https://git-scm.com/docs/git-worktree) — linked/main worktrees, one-branch checkout safeguard, clean-only removal, locks, prune/repair, porcelain `-z`, common/per-worktree refs, submodule limitation. Current page reviewed 2026-08-30; manual last updated for Git 2.54.0 (2026-04-20).
- [git-rev-parse](https://git-scm.com/docs/git-rev-parse) — `--show-toplevel`, `--git-common-dir`, absolute path format, symbolic/verified refs. Current page reviewed 2026-08-30.
- [git-symbolic-ref](https://git-scm.com/docs/git-symbolic-ref) — symbolic HEAD and detached-HEAD exit behavior. Current page reviewed 2026-08-30.
- [git-remote](https://git-scm.com/docs/git-remote) — optional remote default branch, `refs/remotes/<remote>/HEAD`, cached vs queried remote information. Current page reviewed 2026-08-30; manual last updated for Git 2.53.0 (2026-02-02).
- [git-clone](https://git-scm.com/docs/git-clone) — clone checks out the remote's active branch, origin name is configurable, empty destination requirements. Current page reviewed 2026-08-30; manual last updated for Git 2.54.0 (2026-04-20).
- [git-check-ref-format](https://git-scm.com/docs/git-check-ref-format) — authoritative branch-name validation and option-like branch concerns. Current page reviewed 2026-08-30.
- [git-status](https://git-scm.com/docs/git-status) — porcelain formats, untracked/submodule state, background optional-lock warning, large-tree cost. Current page reviewed 2026-08-30; manual last updated for Git 2.53.0 (2026-02-02).
- [git-for-each-ref](https://git-scm.com/docs/git-for-each-ref) — `worktreepath`, symbolic refs, and structured ref inspection. Current page reviewed 2026-08-30.

### Project evidence — HIGH confidence

- `.planning/PROJECT.md` — v2.0 goals, safety requirement, local/daemon/PWA architecture, and existing flat-to-hierarchy transition.
- `baude-core/src/git.rs` — current basename/sanitized worktree path, directory-exists reuse, retry-any-error creation, fail-open dirty check, and plain Git removal.
- `baude-core/src/persist.rs` — flat saved-session schema, silent load-default behavior, direct non-atomic writes, workspace-specific state files.
- `baude/src/app.rs` — session-only selection, flat ordering, clone background path, local close-before-dirty-check flow, remote routing, and context-key assumptions.
- `bauded/src/manager.rs` — global mutex ownership, flat daemon persistence/restore, duplicate-name suffixing, synchronous create/spawn, session-only deletion, and daemon DTO shape.

### Confidence notes and open research flags

- **HIGH:** Git worktree constraints, dirty-removal refusal, porcelain formats, common-dir distinction, remote-HEAD behavior, and the cited current-code hazards are directly documented or observed in source.
- **MEDIUM:** The exact fallback policy for repositories with no authoritative default branch is a product decision; the recommended policy is safe and deterministic but must be confirmed during Phase 1.
- **MEDIUM:** Superprojects containing submodules need a focused Phase 2 spike if baude intends more than safe refusal/preservation; Git explicitly describes multiple-checkout support as incomplete.

---
*Pitfalls research for: baude v2.0 Repository Worktree Management*
*Researched: 2026-08-30*
