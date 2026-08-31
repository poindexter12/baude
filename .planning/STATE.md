---
gsd_state_version: 1.0
milestone: v2.0
milestone_name: Local TUI Dogfood Release
current_phase: 6
current_phase_name: shared lifecycle core refactor
status: verification
stopped_at: Completed 07-01-PLAN.md; Phase 6 certification and Phase 7 UAT remain pending
last_updated: "2026-08-31T08:03:21.185Z"
last_activity: 2026-08-30
last_activity_desc: completed shared lifecycle engine cutover and full local gate; certification pending
state_head: 5252f4f099320b8ffbb0f11b04d1cf08198b69ab
progress:
  total_phases: 3
  completed_phases: 1
  total_plans: 16
  completed_plans: 11
  percent: 33
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-08-30)

**Core value:** You can see at a glance which of your many coding-agent sessions needs you next and act on it from the terminal or phone.
**Current focus:** Phase 6 — Shared Lifecycle Core Refactor

## Current Position

Phase: 6 (2 of 3 v2.0 phases) — shared lifecycle core refactor
Plan: 06-07 local implementation complete; 06-01 through 06-06 retained as execution history
Status: Local implementation green; Phase 6 awaits Linux/runtime certification, independent review, phase verification, and Nyquist approval
Last activity: 2026-08-30 — completed shared lifecycle engine cutover and full local gate

Progress: [███░░░░░░░] 33% (1 of 3 milestone phases complete)

## Performance Metrics

**Velocity:**

- v2.0 plans completed: 11
- Prior milestone: 14 plans completed across 4 phases
- Average duration: 18 min
- Total execution time: 200 min

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 5. Durable Repository Admission | 3/3 | 38 min | 13 min |
| 6. Shared Lifecycle Core Refactor | 7/7 locally implemented | 145 min execution history | 21 min |
| 7. Local TUI Dogfood Release | 1/6 | 17 min | 17 min |
**Per-Plan Metrics:**

| Plan | Duration | Tasks | Files |
|------|----------|-------|-------|
| Phase 05 P01 | 10m | 2 tasks | 1 files |
| Phase 05 P02 | 14min | 3 tasks | 3 files |
| Phase 5 P3 | 14min | 3 tasks | 4 files |
| Phase 06 P01 | 13min | 2 tasks | 5 files |
| Phase 06 P02 | 12min | 2 tasks | 4 files |
| Phase 06 P03 | 11min | 2 tasks | 1 files |
| Phase 06 P04 | 13min | 2 tasks | 6 files |
| Phase 06 P05 | 11min | 3 tasks | 7 files |
| Phase 06 P06 | 16min | 3 tasks | 5 files |
| Phase 06 P07 | 69m | 3 tasks | 10 files |
| Phase 07 P01 | 17min | 2 tasks | 5 files |

## Accumulated Context

### Decisions

Recent decisions affecting current work (full log in PROJECT.md):

- v2.0: Repository identity is canonical across main checkouts, subdirectories, symlinks, and linked worktrees.
- v2.0: If the main checkout is not on the resolved default, preserve and show it; create or reuse a separate managed default-branch worktree.
- v2.0: Opening never silently switches branches, fetches, or guesses a default branch.
- v2.0: Worktree removal and dormant-branch deletion fail closed on unsafe or indeterminate Git state.
- v2.0 (superseded scope): Full repository hierarchy and applicable actions were originally planned for local TUI, remote TUI, and PWA.
- v2.0 scope revision: Ship one shared lifecycle authority plus a local-TUI dogfood slice; defer dormant branch rows/deletion and remote/PWA hierarchy, and target `v2.0.0-beta` readiness without publishing.
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
- [Phase 6]: Managed branch labels are bounded display components only; durable repository and checkout keys supply filesystem identity.
- [Phase 6]: Pre-replacement creation failures compensate only newly added managed worktrees through verified plain Git removal while retaining the branch.
- [Phase 6]: Committed-save and spawn failures retain one durable active child for retry without a runtime association.
- [Phase 6]: Empty valid porcelain-v2 output is the only status-clean observation; malformed output and command failure remain indeterminate.
- [Phase 6]: Removal authorization requires fresh exact managed linked topology and yields only an opaque path/parent/ref/OID target.
- [Phase 6]: Any recursive submodule record blocks non-force worktree removal.
- [Phase 06]: Retained conversation IDs are optional opaque strings with an explicit serde default and never participate in path or ownership identity. — Preserves backend conversation context without weakening durable repository identity.
- [Phase 06]: Close plans snapshot runtime context, save inactive intent, then stop exactly one checkout-key runtime while retaining checkout and repository membership. — Makes save-before-stop ordering shared and non-destructive across App and Manager.
- [Phase 06]: Pre-replacement close failures restore memory and leave the runtime live; post-replacement directory-sync failures keep inactive memory, stop the runtime, and mark persistence dirty. — Keeps memory aligned with the atomic replacement commit boundary.
- [Phase 06]: Targeted resume IDs travel only as opaque PTY environment data referenced by a fixed quoted variable. — Prevents persisted backend data from becoming shell syntax.
- [Phase 06]: Reopen persists active intent only after fresh exact checkout reconciliation and before one runtime effect. — Blocks stale topology and duplicate launch state.
- [Phase 06]: Same-checkout reopen reservations return pending while conflicting repository mutations remain busy. — Allows one checkout-key runtime path under repeated requests.
- [Phase 6]: The first safe-removal preflight supplies target-naming confirmation data but its verified Git token is discarded; confirmation always obtains a new token after runtime stop.
- [Phase 6]: Failures before plain Git removal restore one runtime when one was active, while postcondition or persistence failures after Git commitment never recreate topology.
- [Phase 6]: A pre-replacement final-save failure keeps an unavailable recovery child in memory while old durable context remains on disk; a committed replacement keeps child deletion in memory.
- [Phase 06]: LifecycleCandidate is opaque and only LifecycleEngine selects checkout lifecycle and owned-runtime candidates.
- [Phase 06]: Pre-replacement persistence failure preserves the existing runtime; committed replacement continues the authorized effect and records dirty durability.
- [Phase 06]: Tracked App and Manager restarts use registered lifecycle launch events before PTY release.
- [Phase 07]: Structural hierarchy and ordering come only from persisted repository state; runtime, status, and archive facts are decoration joins. — Prevents volatile process churn from changing structural identity or order.
- [Phase 07]: Normal navigation visits repository parents while cycle navigation visits only actionable checkout and remote rows. — Parents remain discoverable without becoming action targets.
- [Phase 07]: Invalid local selection falls back within its repository before its parent; restart prefers the first local parent before remote rows. — Keeps local context deterministic across refresh, removal, and restart.

### Pending Todos

- Run Linux/runtime certification, independent deep lifecycle review, phase verification, and Nyquist validation for Phase 6.

### Blockers/Concerns

- Corrective 06-07 closes prior CR-01 through CR-03 locally, but an independent deep review must confirm zero unresolved Critical/High findings.
- Linux synchronized gate/release and descendant process-group extinction remain uncertified.
- CORE requirement checkoff and Phase 6 completion remain blocked on certification, phase verification, and Nyquist approval.

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

Last session: 2026-08-31T08:03:21.091Z
Stopped at: Completed 07-01-PLAN.md; Phase 6 certification and Phase 7 UAT remain pending
Resume file: None
