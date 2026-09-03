# Requirements: baude v2.0 Local TUI Dogfood Release

**Defined:** 2026-08-30
**Core Value:** You can see at a glance which of your many coding-agent sessions needs you next and act on it from the terminal or phone.

## v2.0 Requirements

### Repository Admission

- [x] **REPO-01**: User can open a main checkout, subdirectory, symlink, or linked worktree and get exactly one canonical repository parent in the active workspace.
- [x] **REPO-02**: Opening or cloning a repository ensures exactly one usable default-branch session using the active workspace backend.
- [x] **REPO-03**: Reopening an already registered repository focuses or reopens its existing default-branch child instead of creating a duplicate parent or session.
- [x] **REPO-04**: If the main checkout is not on the resolved default branch, baude preserves it and creates or reuses a separate managed default-branch worktree.
- [x] **REPO-05**: User can see both the existing main checkout and the managed default-branch worktree beneath the repository.
- [x] **REPO-06**: If the default branch cannot be resolved safely from local Git data, baude reports an actionable state without switching branches, fetching, or guessing.

### Hierarchy and Branches

- [x] **HIER-01**: User can view repository parents with main-checkout and worktree children in the local TUI.
- [x] **HIER-02**: Repository parents remain visible when none of their children has a running agent session.
- [x] **HIER-03**: Repository parents are ordered by name, while checkout and worktree children are ordered oldest-first by a persisted first-seen timestamp.
- [x] **HIER-04**: Waiting, working, archive, and other session status changes do not reorder children or hide which session needs attention.

### Worktree Lifecycle

- [x] **WORK-01**: User can create a valid named branch or activate an eligible existing local branch as a managed worktree from repository context.
- [x] **WORK-02**: Baude refuses invalid refs, path collisions, and branches already checked out elsewhere without bypassing Git safeguards.
- [x] **WORK-03**: User can close a worktree session while retaining its checkout and hierarchy child for later reopening.
- [x] **WORK-04**: User can reopen a retained main-checkout or worktree child in the active workspace backend.
- [x] **WORK-05**: User can remove a clean managed worktree through a distinct confirmed action without deleting its branch.
- [x] **WORK-06**: Dirty, conflicted, locked, submodule-unsafe, or indeterminate worktree state blocks removal before the running session or persisted child is changed.

### Persistence and Reconciliation

- [x] **PERS-01**: Repository membership, child ownership, managed status, branch, ordering timestamp, and relevant UI and session state survive restart per workspace.
- [x] **PERS-02**: Existing flat local and daemon session state migrates idempotently into repository parents without losing valid Claude Code or OpenCode sessions.
- [x] **PERS-03**: Baude reconciles persisted repository and worktree intent against Git topology before reuse, activation, removal, or launch.
- [x] **PERS-04**: State updates are atomic, and malformed or partially written state is surfaced rather than replaced with an empty hierarchy.

### Shared Lifecycle Core

- [x] **CORE-01**: One `baude-core` lifecycle protocol/state machine is authoritative for Git topology decisions, persistence commit stages, and agent and shell effects; App and Manager do not independently decide lifecycle transitions.
- [x] **CORE-02**: One explicit legal transition table governs all protected lifecycle and recovery states, and every illegal transition is rejected without changing Git, durable state, or owned processes.
- [x] **CORE-03**: Before a destructive or replacement effect, baude durably writes ahead the exact agent and shell process ownership needed for recovery and never forgets either process until teardown is confirmed or durable successor ownership exists.
- [x] **CORE-04**: App and Manager implement the same shared lifecycle effect contract and pass mirrored contract tests for success, every persistence boundary, effect failure, rollback, and restart recovery.
- [x] **CORE-05**: Explicit typed lifecycle candidates and their provenance are saved durably; no generic runtime overlay is used to reconstruct activation, teardown, removal, or rollback intent.
- [x] **CORE-06**: Startup recovery and rollback execute only legal shared-core transitions and converge to truthful Git, persistence, agent, and shell ownership without duplicate or orphaned runtimes.

### Interaction and Surface Parity

- [x] **SURF-01**: Local TUI repository, checkout, and worktree selections expose context-aware actions, hints, and confirmations naming the actual target.
- [x] **SURF-02**: Existing open, clone, shell, editor, resume, archive, attention, and session-cycle behavior remains available for applicable hierarchy children.
- [x] **SURF-05**: Older flat daemon and session APIs remain non-destructive compatibility projections during the v2.0 transition.

### Dogfood Release Gate

- [x] **REL-01**: In an isolated real repository, the local TUI completes the end-to-end default-session and branch-worktree flow—open, create or activate, close, reopen, safely remove, and restart—without duplicate parents, children, or runtimes and without losing user work.
- [x] **REL-02**: Formatting, lint, full tests, package checks, and supported release artifact builds pass for the intended `v2.0.0-beta` source state.
- [x] **REL-03**: Version/package metadata, changelog or release notes, and local install/dogfood documentation consistently target `v2.0.0-beta`; readiness is verified without publishing or pushing a release.

## Future Requirements

### Deferred Repository Surfaces

- **HIER-05**: Local branches not checked out in any worktree appear in a distinct dormant state beneath their repository.
- **HIER-06**: User can activate a dormant branch row to create a managed worktree and launch a session with the active workspace backend.
- **WORK-07**: User can explicitly delete a dormant local branch only when Git confirms it is fully merged and not checked out; unsafe deletion is refused.
- **SURF-03**: Local TUI and daemon-backed remote TUI use the same repository identity, activation, close, safe-removal, and branch-cleanup semantics.
- **SURF-04**: PWA users can view the repository hierarchy and perform applicable open or reopen, create or activate, close, safe-remove, and safe branch-cleanup actions.

### Repository Ergonomics

- **ERGO-01**: User can collapse repository groups without hiding or making waiting children unreachable.
- **ERGO-02**: User can see unmanaged external worktrees as read-only children before explicitly adopting them.
- **ERGO-03**: User can forget a repository through a non-destructive flow that never removes the main checkout or cascades through dirty children.
- **ERGO-04**: User can inspect a rich summary of dirty-state categories before resolving blocked cleanup.

### Advanced Git Management

- **GITM-01**: User can move, repair, lock, or unlock worktrees through explicit guarded actions.
- **GITM-02**: User can choose an explicit base revision or upstream when creating a new branch worktree.
- **GITM-03**: User can perform independently preflighted bulk repository or worktree actions.

## Out of Scope

| Feature | Reason |
|---------|--------|
| Force-removing dirty, locked, unknown, or submodule-unsafe worktrees | Contradicts the milestone's data-safety contract |
| Automatically stashing, committing, resetting, or cleaning changes | Mutates user work and history implicitly |
| Silently switching the main checkout or fetching during open | Opening a repository must remain offline and non-destructive |
| Deleting unmerged or checked-out branches | Safe cleanup is restricted to Git-verified merged dormant branches |
| Deleting a branch automatically with its worktree | Worktree and branch lifecycles remain distinct |
| Full Git GUI operations such as fetch, pull, merge, rebase, and PR management | Not required for repository-centered agent orchestration |
| Supporting coding-agent backends other than Claude Code and OpenCode | Backend support remains intentionally explicit |

## Traceability

Every v2.0 requirement maps to exactly one roadmap phase.

| Requirement | Phase | Status |
|-------------|-------|--------|
| REPO-01 | Phase 5 | Complete |
| REPO-02 | Phase 5 | Complete |
| REPO-03 | Phase 5 | Complete |
| REPO-04 | Phase 5 | Complete |
| REPO-06 | Phase 5 | Complete |
| PERS-01 | Phase 5 | Complete |
| PERS-02 | Phase 5 | Complete |
| PERS-03 | Phase 5 | Complete |
| PERS-04 | Phase 5 | Complete |
| CORE-01 | Phase 6 | Pending |
| CORE-02 | Phase 6 | Pending |
| CORE-03 | Phase 6 | Pending |
| CORE-04 | Phase 6 | Pending |
| CORE-05 | Phase 6 | Pending |
| CORE-06 | Phase 6 | Pending |
| REPO-05 | Phase 7 | Pending |
| HIER-01 | Phase 7 | Pending |
| HIER-02 | Phase 7 | Pending |
| HIER-03 | Phase 7 | Pending |
| HIER-04 | Phase 7 | Pending |
| WORK-01 | Phase 7 | Pending |
| WORK-02 | Phase 7 | Pending |
| WORK-03 | Phase 7 | Pending |
| WORK-04 | Phase 7 | Pending |
| WORK-05 | Phase 7 | Pending |
| WORK-06 | Phase 7 | Pending |
| SURF-01 | Phase 7 | Complete |
| SURF-02 | Phase 7 | Complete |
| SURF-05 | Phase 7 | Complete |
| REL-01 | Phase 7 | Complete |
| REL-02 | Phase 7 | Complete |
| REL-03 | Phase 7 | Pending |

**Coverage:**

- Active v2.0 requirements: 32 total
- Mapped to phases: 32
- Unmapped: 0
- Future requirements: 12 total (including 5 IDs deferred by the 2026-08-30 scope decision)

---
*Requirements defined: 2026-08-30*
*Last updated: 2026-08-30 after narrowing v2.0 to shared lifecycle refactoring and a local-TUI dogfood release*
