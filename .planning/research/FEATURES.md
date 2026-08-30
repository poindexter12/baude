# Feature Research

**Domain:** Repository-centered AI session and Git worktree management
**Milestone:** baude v2.0 Repository Worktree Management
**Researched:** 2026-08-30
**Confidence:** HIGH for Git behavior and the existing baude baseline; MEDIUM for interaction details that still require product decisions

## Feature Landscape

### Table Stakes (Users Expect These)

Missing any P1 item makes the new hierarchy feel cosmetic or unsafe rather than repository-centered.

| Feature | Why Expected | Complexity | User-observable behavior / implementation notes |
|---------|--------------|------------|-----------------------------------------------|
| Persistent repository parent | A repository must remain navigable even when no agent session is running | HIGH | Opening or cloning registers one canonical parent. Closing its session does not remove the parent. Reopening the same repository selects/reuses it rather than creating duplicates. Persist parents separately from child sessions, scoped to the active workspace. |
| Automatic primary/default-branch session | “Open repository” should immediately produce a usable agent, not an empty folder node | HIGH | On first registration, start one session in the main worktree using the active workspace backend (Claude Code or OpenCode). Clone naturally checks out the remote’s active/default branch. For an existing checkout, report the actual checked-out branch and never silently switch a dirty or non-default checkout; an unresolved default branch is an actionable error/state, not a guessed `main`. |
| Explicit repository → session/worktree hierarchy | Parallel work must be visually attributable to its repository | HIGH | Render each repository once, with its primary session and linked managed worktrees indented beneath it. Keep repositories and siblings in stable order; waiting/working changes must not reorder rows. A repository should aggregate child attention without hiding which child needs input. |
| Canonical repository identity | Opening the main checkout, a subdirectory, or one of its linked worktrees should resolve to the same repository | HIGH | Identify the repository through Git’s common/main worktree metadata, not only `rev-parse --show-toplevel` or path text. Path aliases/symlinks and opening an existing child must not create a second parent. |
| Named-branch worktree creation | A worktree manager must support both new topic branches and existing branches | MEDIUM | From either a repository or one of its children, “new worktree” targets the same parent. Validate the branch before mutation. Create a new branch from an explicit, predictable base; attach an existing local branch or uniquely matching remote branch when valid. Surface Git’s “already checked out elsewhere” refusal; never bypass it with `--force`. |
| Verified managed path and collision handling | Reusing a directory solely because it exists can attach an agent to the wrong files | MEDIUM | Managed worktrees live under baude’s data directory, but an existing directory is reused only if `git worktree list --porcelain -z` confirms it belongs to this repository and the intended branch. Otherwise creation stops with an actionable conflict. |
| Separate “close session” and “remove worktree” lifecycles | Process lifetime and filesystem lifetime are different user intents | MEDIUM | Closing stops the agent but keeps the worktree child and files available for reopening. Removing is a separate confirmed operation that removes a managed linked worktree and its persisted child. A kept worktree can launch a fresh backend session later. |
| Dirty-aware, fail-closed removal | Users expect staged, unstaged, and untracked work never to disappear | HIGH | Before stopping a session or mutating persistence for a remove request, check porcelain status. If any staged, unstaged, unmerged, submodule, or untracked change exists, block removal and leave the session/worktree entry intact. If status cannot be determined, treat it as “unknown/unsafe” and block. Ignored files alone do not count as dirty, matching Git. |
| Git-native removal safeguards | baude should strengthen, not bypass, Git’s safety model | MEDIUM | Use plain `git worktree remove` without `--force`. Locked, dirty, submodule-bearing, missing, or otherwise invalid worktrees produce a clear retained-state message. Never delete a linked worktree directory directly. The main worktree is never removable as a child. |
| Context-aware actions and hints | The same key should act on the selected object, not on hidden session assumptions | MEDIUM | Repository selection offers open/reopen primary session and create-worktree actions. Worktree selection offers attach/reopen, close session, and remove worktree. Session-only actions are disabled with an explanation when a parent is selected. Status-bar/help hints reflect the current selection. Global open/clone remain globally available. |
| Hierarchy-aware selection and navigation | A nested sidebar is unusable if cycling and focus land on phantom or inappropriate rows | HIGH | Up/down traverses visible parent and child rows predictably; attach/cycle targets runnable sessions, not inert parents, unless Enter on a parent deliberately opens its primary session. After removal, selection moves to a nearby sibling or its parent. Existing global pane shortcuts retain their meaning. |
| Durable hierarchy and metadata | Restart must preserve the user’s repository mental model | HIGH | Persist repository identity/path plus child path, branch, managed status, and session UI state. Restore parents even when they have no live session. Reconcile saved children against Git’s current worktree registry; do not blindly trust stale JSON. |
| Backward-compatible migration from flat sessions | Existing users already have saved main and worktree sessions | HIGH | On first v2.0 load, group legacy `SavedSession` records by canonical repository, preserve shell/archive state and branches, and write the new schema without losing valid sessions. Migration is idempotent and workspace-specific. |
| Honest missing/moved state | Git worktrees can be moved or deleted outside baude | MEDIUM | A missing repository/worktree remains visible as unavailable with its path and recovery/removal action; it is not silently discarded on restore. Where safe, explain Git’s `worktree repair`/prune condition rather than performing destructive cleanup automatically. |
| Backend/workspace consistency | Repository hierarchy must not undo baude’s hard backend isolation | MEDIUM | Automatic and reopened sessions always use the active workspace’s bound backend and state file. Repository metadata may recur in different workspaces, but child session pools never cross-wire Claude and OpenCode. |

### Differentiators (Competitive Advantage)

These features reinforce baude’s core value: managing attention across many coding agents, not merely wrapping `git worktree`.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Repository as a durable control surface | Users can park and resume work without equating “repository exists” with “agent process is alive” | HIGH | This is the central v2.0 differentiator over the current flat session list. Parent aggregate state should show child counts/attention while preserving stable order. |
| Zero-friction default-branch agent | Opening/cloning lands directly in a backend-ready primary session | MEDIUM | Reuses current backend spawn/prepare/resume behavior. Idempotence is essential: repeated open must focus or reopen, not spawn duplicate primary sessions. |
| Agent-aware nested worktrees | Each branch checkout is paired with waiting/working/exited metadata, shell/editor actions, and conversation continuity | HIGH | Git CLI lists worktrees but does not orchestrate coding agents. Existing baude session status, metadata, shell pane, and editor integration should work unchanged on every child. |
| Safety that preserves operator context | A failed remove leaves both files and the current session available to resolve the issue | MEDIUM | Improves the current flow, which closes the session before discovering dirtiness. Preflight first, then confirm/stop/remove/persist; on failure retain or restore the navigable child. |
| Selection-derived shortcut semantics | Users can operate from either a parent or child without manually navigating to a “special” main session first | MEDIUM | Resolve every repository-scoped action through selected row → repository parent. Show the effective action in hints and confirmation text. Avoid overloaded keys whose destructive meaning changes silently. |
| Reconciliation with Git as source of truth | Persisted intent survives restart while actual worktree state remains trustworthy | HIGH | JSON owns baude metadata; `git worktree list --porcelain -z` owns linked-worktree existence, branch, lock, detached, and prunable state. Explicitly display conflicts instead of silently choosing one source. |

### Anti-Features (Commonly Requested, Often Problematic)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| Force-remove dirty or locked worktrees | “I know what I’m doing” cleanup is faster | Contradicts the milestone’s data-safety promise; `--force` can discard tracked and untracked work and twice-force locked trees | Block removal. Let the user clean/stash/unlock in the shell, then retry. |
| Automatically stash, commit, reset, or clean before removal | Makes cleanup appear one-click | Changes repository history/state, creates hard-to-find stashes, and can still mishandle untracked/submodule data | Report why removal is blocked and open/focus the worktree shell. |
| Silently switch the main checkout to the detected default branch | Guarantees a literal default-branch session | Can disrupt a user’s active branch and dirty files; default-branch detection can be ambiguous or stale | Use the existing main checkout safely, show branch mismatch, and require explicit user action to switch. Clone uses the remote’s active branch naturally. |
| Force the same branch into multiple worktrees | Allows duplicate agents on one branch | Git deliberately rejects this because two working trees would update the same branch ref and create confusing state | Focus the existing worktree or require a new branch. |
| Delete the branch when removing its worktree | Feels like complete cleanup | Worktree removal and branch deletion are independent; the branch may contain unmerged commits or be needed remotely | Remove only the linked worktree. Leave branch deletion to explicit Git tooling/future separately guarded flow. |
| Adopt or delete every externally created worktree automatically | Makes the tree “complete” without configuration | Ownership is unclear; baude could delete paths it did not create or create unwanted agents | Display discovered external worktrees as unmanaged/read-only only if later needed; v2.0 should manage explicitly registered/created children. |
| Treat a repository parent as another fake session | Minimizes model changes | Recreates duplicate rows, ambiguous close semantics, and cannot preserve an empty repository parent | Introduce explicit repository and child entities; adapt session rendering beneath them. |
| Cascade-delete a repository and all children | Convenient bulk cleanup | One dirty/unknown child creates data-loss and partial-failure hazards; removing the main worktree is invalid | “Forget repository” may be added later only when children are retained or independently preflighted. Not required for v2.0. |
| Hide waiting children inside collapsed parents | Saves sidebar space | Undermines baude’s primary promise to show which agent needs attention | Keep children visible in v2.0; if collapse is added later, parents must retain unmistakable aggregate attention and cycling must still reach waiting children. |
| Full Git GUI (fetch/pull/merge/rebase/PR/branch deletion) | Repository parents invite broader Git controls | Large scope, duplicated tooling, and destructive state transitions distract from agent orchestration | Limit v2.0 to register/clone, launch, create/list, close, and safe remove. Use shell/editor for general Git operations. |
| Automatic fetch on every open/startup | Keeps branch data fresh | Adds latency, network/auth failures, and surprising remote mutation to local navigation | Use local refs for v2.0; let users fetch explicitly. Clearly report when a requested remote branch is unavailable. |
| Filesystem polling as the only source of worktree truth | Seems simpler than parsing Git | A directory can exist while Git metadata is stale, belong to another repo, or map to another branch | Reconcile through stable Git porcelain output; use filesystem existence only as one signal. |
| Immediate TUI/daemon/PWA feature parity | A single conceptual model everywhere is appealing | The milestone is already a substantial local model/UI migration; remote shell/editor/worktree constraints differ | Make the core repository model serializable and reusable, but ship the focused TUI behavior first unless roadmap requirements explicitly demand remote parity. |

## Behavior Contract

The roadmap should plan and test these externally visible scenarios, not only internal data structures.

1. **Open existing repository:** one parent appears; one primary session starts with the active backend; repeating open focuses/reuses it.
2. **Clone repository:** progress remains non-blocking; on success one parent appears and the clone’s checked-out default branch session starts; failures leave no half-registered parent.
3. **Open from a linked worktree/subdirectory:** baude selects the existing parent and relevant child rather than registering another repository.
4. **Create new branch worktree:** a managed child appears under the selected repository and starts an agent; its location/branch is visible.
5. **Open existing branch:** succeeds only if Git permits checkout; a branch already checked out elsewhere points the user to that existing worktree.
6. **Close primary session:** agent stops, repository remains; Enter/reopen starts a fresh/resumed primary session as supported by the backend.
7. **Close worktree session:** agent stops, worktree and child remain; it can be reopened.
8. **Remove clean managed worktree:** confirmation names repository, branch, and path; session stops; Git removes the linked worktree; child metadata disappears.
9. **Remove dirty/unknown worktree:** removal is blocked before session teardown; files, child, and current selection remain; message states whether changes or status failure blocked it.
10. **Restart baude:** repository order/hierarchy and saved UI state return; existing sessions resume according to current backend behavior; sessionless parents remain.
11. **External deletion/move:** restore shows an unavailable/conflicted child with recovery guidance instead of silently dropping it or launching in the wrong path.
12. **Legacy state upgrade:** all valid flat sessions reappear under exactly one parent per repository with no backend/workspace leakage.

## Feature Dependencies

```text
[Canonical repository identity]
    └──requires──> [Git common-dir/main-worktree discovery]
    └──enables──> [Persistent repository parent]
                      ├──requires──> [New versioned persistence schema + migration]
                      ├──enables──> [Hierarchy rendering/navigation]
                      └──enables──> [Automatic primary session]
                                           └──requires──> [Active workspace/backend spawn path]

[Managed worktree inventory]
    └──requires──> [git worktree list --porcelain -z reconciliation]
    ├──enables──> [Verified create/reuse]
    ├──enables──> [Nested child rendering]
    └──enables──> [Safe removal]
                      ├──requires──> [Fail-closed dirty preflight]
                      └──requires──> [Close session vs remove worktree separation]

[Context-aware shortcuts]
    └──requires──> [Typed parent/child selection model]
    └──requires──> [Hierarchy-aware navigation]

[Stable attention display] ──constrains──> [Hierarchy rendering/navigation]
[Backend/workspace isolation] ──constrains──> [Persistence and automatic session launch]
[Force removal / silent checkout] ──conflicts──> [Dirty-safe repository management]
```

### Dependency Notes

- **Canonical identity precedes hierarchy:** grouping by `repo_root` text is insufficient because a linked worktree’s top-level path is not the main worktree. Resolve identity before persisting parents or migrating flat records.
- **Persistence schema precedes sessionless parents:** the current `State { sessions }` cannot represent a repository with no running session, so lifecycle semantics cannot be correct without a parent entity.
- **Typed selection precedes contextual keys:** current `SelId` distinguishes only local/remote sessions. Parent rows need a first-class selection target before help, Enter, `w`, `x`, cycling, or editor/shell behavior can be made predictable.
- **Inventory/reconciliation precedes safe reuse and removal:** Git’s porcelain worktree list is stable for scripts and exposes branch, detached, locked, missing/prunable state. Directory existence alone is unsafe.
- **Dirty preflight must precede teardown:** current close/remove removes the session first and then tests dirtiness. Reverse that order so blocked removal preserves operator context.
- **Stable attention behavior constrains UI work:** repository grouping may alter ordering, but child status changes must not reshuffle rows and archived/collapsed treatment must not hide waiting agents.
- **Core model should remain UI-independent:** repository/worktree records and Git operations belong in `baude-core`; TUI hierarchy and shortcuts consume them. This preserves a later daemon/PWA path without forcing parity into the first phase.

## MVP Definition

### Launch With (v2.0)

- [ ] Canonical, persistent repository parents with idempotent open/clone
- [ ] One automatic primary session per repository using the active workspace backend
- [ ] Stable nested rendering and navigation for primary and managed worktree children
- [ ] Named-branch managed worktree creation with verified collision/branch handling
- [ ] Separate close-session and remove-worktree actions
- [ ] Fail-closed dirty/unknown removal preflight with no force path
- [ ] Context-aware actions, confirmations, hints, and unavailable-action feedback
- [ ] Versioned hierarchy persistence, legacy flat-state migration, and Git reconciliation on restore
- [ ] Regression preservation for status, waiting flash, shell/editor, archive, resume, and backend isolation

### Add After Validation (v2.x)

- [ ] Collapsible repository groups — only if large real-world sidebars demand it and waiting children remain visible/reachable
- [ ] Read-only discovery of unmanaged external worktrees — add when users need a complete inventory; never imply deletion ownership
- [ ] Explicit “forget repository” flow — only with independently preflighted children and unambiguous non-destructive semantics
- [ ] Rich dirty summary (counts/categories) — useful after correctness is proven; a simple safe block is enough initially
- [ ] Daemon/PWA repository hierarchy — after the core representation and API contract settle

### Future Consideration (Post-v2.0)

- [ ] Worktree move/repair/lock controls — valuable edge-case administration, not core agent orchestration
- [ ] Explicit base revision/upstream picker for new branches — defer until simple named-branch flow proves insufficient
- [ ] Bulk repository actions — defer because partial failure and dirty-state UX need separate design

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Canonical repository parent + idempotent open | HIGH | HIGH | P1 |
| Automatic primary/default session | HIGH | MEDIUM | P1 |
| Nested hierarchy and navigation | HIGH | HIGH | P1 |
| Named-branch worktree creation | HIGH | MEDIUM | P1 |
| Close vs remove lifecycle split | HIGH | MEDIUM | P1 |
| Fail-closed dirty removal | HIGH | MEDIUM | P1 |
| Persistence migration and reconciliation | HIGH | HIGH | P1 |
| Context-aware shortcuts/hints | HIGH | MEDIUM | P1 |
| Unavailable/moved-state diagnostics | MEDIUM | MEDIUM | P1 |
| Unmanaged worktree discovery | MEDIUM | HIGH | P2 |
| Collapsible groups | LOW | MEDIUM | P2 |
| Forget repository | MEDIUM | HIGH | P2 |
| Worktree repair/move/lock UI | LOW | HIGH | P3 |
| Full Git operations | LOW for core value | HIGH | P3 / anti-feature |

**Priority key:**
- P1: Must have for v2.0
- P2: Should follow only after the core hierarchy validates
- P3: Future consideration, outside this milestone

## Baseline Feature Analysis

| Capability | Git CLI | baude v0.14 flat model | Recommended v2.0 approach |
|------------|---------|------------------------|---------------------------|
| Repository identity | Main worktree plus linked worktree administrative metadata | Session carries `cwd` and `repo_root`; no persistent parent | Canonical parent resolved through Git common/main-worktree data |
| Default checkout | Clone checks out remote active branch | Opening a path starts a session wherever that path currently points | Auto-start a primary session, but never silently switch an existing checkout |
| Worktree inventory | Stable `worktree list --porcelain -z` including branch/locked/prunable | Only baude-created saved sessions are visible | Reconcile persisted managed intent against Git inventory |
| Branch collision | Refuses a branch checked out elsewhere unless forced | Attempts new branch, then existing branch; blindly reuses an existing managed directory | Preserve refusal, identify existing worktree, verify any path before reuse |
| Dirty removal | Plain remove refuses dirty; force can override | Checks `status --porcelain`, but status error is treated clean and session closes before check | Preflight staged/unstaged/untracked; errors block; never force; preserve session on block |
| Session lifecycle | Not applicable | Closing a worktree session asks keep/remove | Make close and remove distinct persistent child actions |
| Hierarchy | Flat worktree list | Flat alphabetic session list | Stable repository groups with nested child attention/status |
| Persistence | Git owns worktree metadata | JSON stores only sessions | Versioned parent/child state plus Git reconciliation and legacy migration |
| Backend awareness | Not applicable | Active workspace backend already controls spawn | Reuse the same path automatically for primary and child sessions |

## Sources

- **HIGH:** baude `.planning/PROJECT.md` (v2.0 goal, active requirements, constraints), read 2026-08-30.
- **HIGH:** baude `README.md` (shipped flat-session, clone, worktree, shortcut, persistence, workspace/backend behavior), read 2026-08-30.
- **HIGH:** baude `baude/src/app.rs`, `baude/src/ui.rs`, `baude-core/src/git.rs`, and `baude-core/src/persist.rs` (current implementation and migration constraints), read 2026-08-30.
- **HIGH:** Git `git-worktree` documentation, current manual last updated for Git 2.54.0 (2026-04-20): https://git-scm.com/docs/git-worktree
- **HIGH:** Git `git-clone` documentation, current manual last updated for Git 2.54.0 (2026-04-20): https://git-scm.com/docs/git-clone
- **HIGH:** Git `git-status` documentation, current manual last updated for Git 2.53.0 (2026-02-02): https://git-scm.com/docs/git-status
- **MEDIUM:** Product interaction recommendations are deductions from the milestone goal and existing baude behavior; exact key assignments, default-branch mismatch treatment, and remote-client scope should be confirmed during phase discussion.

---
*Feature research for: baude v2.0 repository-centered worktree management*
*Researched: 2026-08-30*
