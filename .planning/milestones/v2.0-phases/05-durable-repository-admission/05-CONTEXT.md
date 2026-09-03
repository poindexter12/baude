# Phase 5: Durable Repository Admission - Context

**Gathered:** 2026-08-30
**Status:** Ready for planning

<domain>
## Phase Boundary

Establish canonical, durable repository admission; resolve and ensure one non-destructive default-branch primary session through the active workspace backend; migrate existing flat state; and make persisted repository state atomic and Git-reconciled. Worktree lifecycle controls and hierarchy presentation belong to later phases.

</domain>

<decisions>
## Implementation Decisions

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

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `baude-core/src/git.rs` already centralizes Git subprocess invocation, clone parsing, and worktree creation/removal.
- `baude-core/src/persist.rs` already isolates local/daemon and per-workspace state filenames and preserves current `SavedSession` fields.
- `App::add_session` already resolves the active backend command, prepares the checkout, spawns the PTY, and records session metadata.

### Established Patterns
- Active workspace binding selects Claude Code or OpenCode and keeps commands, daemon ports, and state files isolated.
- Runtime session IDs are allocated monotonically and selection currently uses typed local/remote session IDs.
- Existing restore skips missing paths and direct writes parse failures as default state; both behaviors must be replaced for durable parents.

### Integration Points
- Replace launch-directory and open/clone completion session admission in `baude/src/app.rs` with idempotent repository admission.
- Extend `baude-core/src/git.rs` with byte-safe topology/default-branch discovery and canonical identity.
- Extend `baude-core/src/persist.rs` with versioned repository/child records, explicit load errors, migration, and atomic save.
- Keep backend spawning in existing backend/session paths rather than storing a backend on repository records.

</code_context>

<specifics>
## Specific Ideas

- When the main checkout is not on the default branch, show and preserve it while using a separate managed default-branch worktree as the primary session checkout.
- Opening from the main checkout, a child worktree, a subdirectory, or a symlink must converge on one repository parent.

</specifics>

<deferred>
## Deferred Ideas

- Safe managed-worktree create/close/remove semantics are Phase 6.
- Local hierarchy rendering, dormant branches, ordering, and contextual shortcuts are Phase 7.
- Daemon/remote and PWA projections are Phases 8 and 9.

</deferred>
