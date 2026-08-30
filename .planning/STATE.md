---
gsd_state_version: 1.0
milestone: v2.0
milestone_name: Repository Worktree Management (Planned)
current_phase: 6
current_phase_name: safe managed worktree lifecycle
status: executing
stopped_at: Completed 06-01-PLAN.md
last_updated: "2026-08-30T20:30:45.239Z"
last_activity: 2026-08-30
state_head: 475f14911c5f0d4b443dcba064586a55f23ebb3a
progress:
  total_phases: 5
  completed_phases: 1
  total_plans: 9
  completed_plans: 4
  percent: 44
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-08-30)

**Core value:** You can see at a glance which of your many coding-agent sessions needs you next and act on it from the terminal or phone.
**Current focus:** Phase 6 — Safe Managed Worktree Lifecycle

## Current Position

Phase: 6 of 9 (safe managed worktree lifecycle)
Plan: 2 of 6
Status: Ready to execute
Last activity: 2026-08-30

Progress: [██░░░░░░░░] 17%

## Performance Metrics

**Velocity:**

- v2.0 plans completed: 4
- Prior milestone: 14 plans completed across 4 phases
- Average duration: 13 min
- Total execution time: 51 min

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 5. Durable Repository Admission | 3/3 | 38 min | 13 min |
| 6. Safe Managed Worktree Lifecycle | 1/6 | 13 min | 13 min |
| 7. Local Repository Hierarchy & Branch Control | 0/TBD | - | - |
| 8. Daemon & Remote TUI Parity | 0/TBD | - | - |
| 9. PWA Hierarchy & Cross-Surface Completion | 0/TBD | - | - |
**Per-Plan Metrics:**

| Plan | Duration | Tasks | Files |
|------|----------|-------|-------|
| Phase 05 P01 | 10m | 2 tasks | 1 files |
| Phase 05 P02 | 14min | 3 tasks | 3 files |
| Phase 5 P3 | 14min | 3 tasks | 4 files |
| Phase 06 P01 | 13min | 2 tasks | 5 files |

## Accumulated Context

### Decisions

Recent decisions affecting current work (full log in PROJECT.md):

- v2.0: Repository identity is canonical across main checkouts, subdirectories, symlinks, and linked worktrees.
- v2.0: If the main checkout is not on the resolved default, preserve and show it; create or reuse a separate managed default-branch worktree.
- v2.0: Opening never silently switches branches, fetches, or guesses a default branch.
- v2.0: Worktree removal and dormant-branch deletion fail closed on unsafe or indeterminate Git state.
- v2.0: Full repository hierarchy and applicable actions ship in local TUI, remote TUI, and PWA.
- [Phase 05]: Repository identity uses the canonical common directory plus Git's main-first worktree inventory; show-toplevel only selects an inventory member.
- [Phase 05]: Default resolution prefers the main branch upstream remote, then origin, and requires exact commit-verified local remote HEAD targets.
- [Phase 05]: Managed default creation verifies full refs, uses exact branch semantics, and rediscovery proves common directory, path, and branch.
- [Phase 5]: Repository and checkout keys are persisted monotonic u64 newtypes scoped to one workspace state file.
- [Phase 5]: Legacy migration accepts reconciled identity as an injected value and never infers baude ownership from is_worktree.
- [Phase 5]: Only NotFound is first-run state; malformed, unsupported, unreadable, and invalid aggregates are path-aware blocking errors.
- [Phase 5]: Primary runtime dispatch uses durable active intent plus a stable checkout-key runtime association, never display name or cwd.
- [Phase 5]: Checkout reuse requires fresh common-directory, canonical-path, full-ref, and unlocked/non-prunable reconciliation.
- [Phase 5]: App and daemon load failures block automatic saves and subsequent process launches until state evidence is repaired.
- [Phase 6]: Branch text is accepted only after literal Git validation, exact local-ref classification, and fresh inventory checks.
- [Phase 6]: Repository lifecycle mutations reserve by durable RepositoryKey and release through RAII guards.
- [Phase 6]: App and Manager persist shared activation transitions before associating runtimes by CheckoutKey.

### Pending Todos

None yet.

### Blockers/Concerns

- Phase 5 must preserve malformed and legacy state while migrating both Claude Code and OpenCode workspace data idempotently.
- Phase 6 must treat dirty-check errors, submodule uncertainty, locking, and topology races as removal blockers before changing session or persisted state.
- Phases 8-9 must keep daemon IDs authoritative and prevent client-local paths or compatibility APIs from weakening lifecycle safety.

## Deferred Items

Items carried forward from the v0.7 close (code-complete; human-only verification):

| Category | Item | Status |
|----------|------|--------|
| UAT | Phase 1 info overlay effort/thinking/PR rows | pending |
| UAT | Phase 3 PWA activity strip and TUI `v` overlay | pending |
| UAT | Phase 4 live `claude` permission MCP wire contract | pending |
| UAT | Phase 4 PWA approve/deny card and distinct push | pending |
| verification | Phase 1/3/4 human-needed verification artifacts | pending |
| deferred | First-real-phone Web Push verification from v0.5 | pending |

## Session Continuity

Last session: 2026-08-30T20:30:38.464Z
Stopped at: Completed 06-01-PLAN.md
Resume file: None
