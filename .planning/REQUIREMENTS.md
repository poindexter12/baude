# Requirements: baude v2.0 Repository Worktree Management

**Defined:** 2026-08-30
**Core Value:** You can see at a glance which of your many coding-agent sessions needs you next and act on it from the terminal or phone.

## v2.0 Requirements

### Repository Admission

- [x] **REPO-01**: User can open a main checkout, subdirectory, symlink, or linked worktree and get exactly one canonical repository parent in the active workspace.
- [ ] **REPO-02**: Opening or cloning a repository ensures exactly one usable default-branch session using the active workspace backend.
- [ ] **REPO-03**: Reopening an already registered repository focuses or reopens its existing default-branch child instead of creating a duplicate parent or session.
- [x] **REPO-04**: If the main checkout is not on the resolved default branch, baude preserves it and creates or reuses a separate managed default-branch worktree.
- [ ] **REPO-05**: User can see both the existing main checkout and the managed default-branch worktree beneath the repository.
- [x] **REPO-06**: If the default branch cannot be resolved safely from local Git data, baude reports an actionable state without switching branches, fetching, or guessing.

### Hierarchy and Branches

- [ ] **HIER-01**: User can view repository parents with main checkout, worktree, and dormant-branch children in the local TUI, remote TUI, and PWA.
- [ ] **HIER-02**: Repository parents remain visible when none of their children has a running agent session.
- [ ] **HIER-03**: Repository parents are ordered by name, while checkout and worktree children are ordered oldest-first by a persisted first-seen timestamp.
- [ ] **HIER-04**: Waiting, working, archive, and other session status changes do not reorder children or hide which session needs attention.
- [ ] **HIER-05**: Local branches not checked out in any worktree appear in a distinct dormant state beneath their repository.
- [ ] **HIER-06**: User can activate a dormant branch to create a managed worktree and launch a session with the active workspace backend.

### Worktree Lifecycle

- [ ] **WORK-01**: User can create a valid named branch or activate an eligible existing local branch as a managed worktree from repository context.
- [ ] **WORK-02**: Baude refuses invalid refs, path collisions, and branches already checked out elsewhere without bypassing Git safeguards.
- [ ] **WORK-03**: User can close a worktree session while retaining its checkout and hierarchy child for later reopening.
- [ ] **WORK-04**: User can reopen a retained main-checkout or worktree child in the active workspace backend.
- [ ] **WORK-05**: User can remove a clean managed worktree through a distinct confirmed action without deleting its branch.
- [ ] **WORK-06**: Dirty, conflicted, locked, submodule-unsafe, or indeterminate worktree state blocks removal before the running session or persisted child is changed.
- [ ] **WORK-07**: User can explicitly delete a dormant local branch only when Git confirms it is fully merged and not checked out; unsafe deletion is refused.

### Persistence and Reconciliation

- [x] **PERS-01**: Repository membership, child ownership, managed status, branch, ordering timestamp, and relevant UI and session state survive restart per workspace.
- [x] **PERS-02**: Existing flat local and daemon session state migrates idempotently into repository parents without losing valid Claude Code or OpenCode sessions.
- [ ] **PERS-03**: Baude reconciles persisted repository and worktree intent against Git topology before reuse, activation, removal, or launch.
- [x] **PERS-04**: State updates are atomic, and malformed or partially written state is surfaced rather than replaced with an empty hierarchy.

### Interaction and Surface Parity

- [ ] **SURF-01**: Repository, checkout, worktree, and dormant-branch selections expose context-aware actions, hints, and confirmations naming the actual target.
- [ ] **SURF-02**: Existing open, clone, shell, editor, resume, archive, attention, and session-cycle behavior remains available for applicable hierarchy children.
- [ ] **SURF-03**: Local TUI and daemon-backed remote TUI use the same repository identity, activation, close, safe-removal, and branch-cleanup semantics.
- [ ] **SURF-04**: PWA users can view the repository hierarchy and perform applicable open or reopen, create or activate, close, safe-remove, and safe branch-cleanup actions.
- [ ] **SURF-05**: Older flat daemon and session APIs remain non-destructive compatibility projections during the v2.0 transition.

## Future Requirements

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
| REPO-02 | Phase 5 | Pending |
| REPO-03 | Phase 5 | Pending |
| REPO-04 | Phase 5 | Complete |
| REPO-05 | Phase 7 | Pending |
| REPO-06 | Phase 5 | Complete |
| HIER-01 | Phase 9 | Pending |
| HIER-02 | Phase 7 | Pending |
| HIER-03 | Phase 7 | Pending |
| HIER-04 | Phase 7 | Pending |
| HIER-05 | Phase 7 | Pending |
| HIER-06 | Phase 7 | Pending |
| WORK-01 | Phase 6 | Pending |
| WORK-02 | Phase 6 | Pending |
| WORK-03 | Phase 6 | Pending |
| WORK-04 | Phase 6 | Pending |
| WORK-05 | Phase 6 | Pending |
| WORK-06 | Phase 6 | Pending |
| WORK-07 | Phase 7 | Pending |
| PERS-01 | Phase 5 | Complete |
| PERS-02 | Phase 5 | Complete |
| PERS-03 | Phase 5 | Pending |
| PERS-04 | Phase 5 | Complete |
| SURF-01 | Phase 7 | Pending |
| SURF-02 | Phase 7 | Pending |
| SURF-03 | Phase 8 | Pending |
| SURF-04 | Phase 9 | Pending |
| SURF-05 | Phase 8 | Pending |

**Coverage:**

- v2.0 requirements: 28 total
- Mapped to phases: 28
- Unmapped: 0

---
*Requirements defined: 2026-08-30*
*Last updated: 2026-08-30 after v2.0 roadmap creation*
