# Phase 6: Safe Managed Worktree Lifecycle - Context

**Gathered:** 2026-08-30
**Status:** Ready for planning

<domain>
## Phase Boundary

Implement the shared, Git-verified lifecycle for creating or activating managed branch worktrees, closing and reopening their agent sessions, and removing only clean safe worktrees while preserving user work and repository context. Hierarchy rendering and branch-row UI are deferred to Phase 7; remote and PWA adapters are deferred to Phases 8 and 9.

</domain>

<decisions>
## Implementation Decisions

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

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- Phase 5 introduced canonical `RepositoryDiscovery`, durable `RepositoryState`/checkout keys, active intent, strict persistence, and App/Manager reconciliation.
- `baude-core/src/git.rs` already centralizes topology, default worktree ensure, dirty checks, and non-force worktree removal.
- Existing App close modal and daemon manager methods provide current session/process teardown paths that can be redirected through the shared lifecycle contract.

### Established Patterns
- Persist durable intent before spawning, use checkout keys rather than paths/names for runtime ownership, and reconcile Git before launch or mutation.
- Persistence errors remain typed through manager/API boundaries and must preserve memory/process/disk consistency.
- Git subprocesses use argv arrays and stable porcelain output; no force flags or direct recursive deletion are allowed.

### Integration Points
- Extend core Git APIs for ref validation, full inventory, result-valued cleanliness, and verified create/remove postconditions.
- Add shared repository mutation decisions/types in `baude-core` without UI dependencies.
- Route local App and daemon Manager create/close/reopen/remove through the same invariants while leaving surface-specific interaction to later phases.

</code_context>

<specifics>
## Specific Ideas

- A removed worktree's branch should immediately become eligible for Phase 7's dormant-branch presentation and reactivation.
- A blocked removal must leave the currently running agent available so the user can clean or resolve the checkout.

</specifics>

<deferred>
## Deferred Ideas

- Dormant branch rendering and safe merged-branch deletion are Phase 7.
- Remote TUI API presentation and PWA actions are Phases 8 and 9.
- Remote-only tracking branch creation, branch deletion during worktree removal, force removal, and automatic cleanup remain out of scope.

</deferred>
