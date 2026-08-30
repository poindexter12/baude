# Roadmap: baude

## Milestones

- ✅ **v0.7 Native Claude Integration** — Phases 1-4 (shipped 2026-06-16, code-complete; human UATs deferred)
- 📋 **v2.0 Repository Worktree Management** — Phases 5-9 (planned)

## Overview

v2.0 replaces the flat session list with durable repository parents and Git-reconciled checkout, worktree, and dormant-branch children. The milestone first establishes non-destructive repository admission and persistence, then adds a fail-closed managed-worktree lifecycle, and finally delivers the complete hierarchy and action contract through the local TUI, daemon-backed remote TUI, and phone PWA.

## Phases

<details>
<summary>✅ v0.7 Native Claude Integration (Phases 1-4) — SHIPPED 2026-06-16</summary>

- [x] Phase 1: Full Status-Line Capture (3/3 plans) — completed 2026-06-15
- [x] Phase 2: Hook-Driven Status (3/3 plans) — completed 2026-06-15
- [x] Phase 3: Tool-Activity Timeline (4/4 plans) — completed 2026-06-15
- [x] Phase 4: Remote Permission Approval (4/4 plans) — completed 2026-06-16

Full detail: `milestones/v0.7-ROADMAP.md` · Audit: `milestones/v0.7-MILESTONE-AUDIT.md` (tech_debt — code-complete, integration clean, human UATs deferred).

**Deferred human UATs** (see `STATE.md` Deferred Items + per-phase `UAT.md`): hook-state flip visual (BL-01), PWA activity-strip + TUI `v` overlay visuals, live-`claude` `--permission-prompt-tool` MCP wire contract, first-phone Web Push.

</details>

### 📋 v2.0 Repository Worktree Management (Planned)

**Milestone Goal:** Make repositories persistent, navigable parents so the active backend can work on the resolved default branch immediately and create isolated worktrees for parallel work without silently mutating Git state.

- [x] **Phase 5: Durable Repository Admission** - Canonical repositories, the approved default-branch contract, migration, and atomic persistence work per workspace. (completed 2026-08-30)
- [ ] **Phase 6: Safe Managed Worktree Lifecycle** - Users can create, retain, reopen, and safely remove managed worktrees under Git-verified safeguards.
- [ ] **Phase 7: Local Repository Hierarchy & Branch Control** - The local TUI presents stable repository children, dormant branches, and context-aware actions.
- [ ] **Phase 8: Daemon & Remote TUI Parity** - Daemon-owned repositories and the remote TUI use the same identity and lifecycle semantics without breaking flat clients.
- [ ] **Phase 9: PWA Hierarchy & Cross-Surface Completion** - The PWA exposes the full hierarchy and action set, completing local, remote, and phone parity.

## Phase Details

### Phase 5: Durable Repository Admission

**Goal**: Users can admit a repository once and reliably return to a durable, Git-reconciled parent with one usable active-backend session on the resolved default branch, without baude mutating the existing checkout.
**Depends on**: Phase 4
**Requirements**: REPO-01, REPO-02, REPO-03, REPO-04, REPO-06, PERS-01, PERS-02, PERS-03, PERS-04
**Success Criteria** (what must be TRUE):

  1. Opening a main checkout, subdirectory, symlink, or linked worktree repeatedly produces one canonical repository parent and focuses or reopens one usable default-branch session in the active workspace backend.
  2. When the main checkout is not on the resolved default branch, it remains untouched while baude creates or reuses a separate managed default-branch worktree; baude never silently switches or fetches.
  3. When local Git data cannot safely resolve a default branch, the user receives an actionable error and no branch is guessed, checkout switched, or network fetch attempted.
  4. Repository ownership, children, managed state, branch, first-seen ordering, and relevant session state survive restart independently in each workspace, including valid migrated Claude Code and OpenCode sessions.
  5. Before reuse or mutation, persisted intent is reconciled with current Git topology; malformed or partial state is surfaced rather than silently replaced, and successful state changes are atomic.

**Plans**: 3 plans

- [x] 05-01-PLAN.md
- [x] 05-02-PLAN.md
- [x] 05-03-PLAN.md

### Phase 6: Safe Managed Worktree Lifecycle

**Goal**: Users can run parallel work in managed branch worktrees and clean them up without baude bypassing Git safeguards or discarding work.
**Depends on**: Phase 5
**Requirements**: WORK-01, WORK-02, WORK-03, WORK-04, WORK-05, WORK-06
**Success Criteria** (what must be TRUE):

  1. From repository context, the user can create a valid named branch or activate an eligible existing local branch as a managed worktree with a session in the active workspace backend.
  2. Invalid refs, managed-path collisions, and branches checked out elsewhere are refused with an actionable explanation and no partial child or session left behind.
  3. The user can close a worktree session while retaining its checkout, then reopen that worktree or the main checkout later in the active backend.
  4. A distinct confirmed remove action removes a clean managed worktree while retaining its branch.
  5. Dirty, conflicted, locked, submodule-unsafe, or indeterminate state blocks removal before the running session or persisted child changes, leaving the user's work and context intact.

**Plans**: 6 plans

Plans:
- [ ] 06-01-PLAN.md — Trace valid named/local branch activation through shared Git, lifecycle, App, and Manager semantics.
- [ ] 06-02-PLAN.md — Fail closed on invalid refs, path collisions, partial failures, and concurrent creation.
- [ ] 06-03-PLAN.md — Prove result-valued dirty, conflict, topology, lock, and submodule removal blockers.
- [ ] 06-04-PLAN.md — Close sessions only after retaining complete durable checkout and conversation context.
- [ ] 06-05-PLAN.md — Reconcile and reopen retained checkouts with secure targeted backend resume and one runtime.
- [ ] 06-06-PLAN.md — Confirm and execute double-preflight clean removal while preserving branch, parent, and recovery context.

### Phase 7: Local Repository Hierarchy & Branch Control

**Goal**: Local TUI users can navigate durable repositories, their ordered checkout and branch children, and the exact actions valid for each selected target.
**Depends on**: Phase 6
**Requirements**: REPO-05, HIER-02, HIER-03, HIER-04, HIER-05, HIER-06, WORK-07, SURF-01, SURF-02
**Success Criteria** (what must be TRUE):

  1. The local TUI keeps repository parents visible without running sessions and shows the existing main checkout, any separate managed default-branch worktree, other worktrees, and dormant local branches beneath the correct parent.
  2. Repository parents are ordered by name; checkout and worktree rows remain oldest-first by persisted first-seen timestamp across restart, and status or attention changes never reorder them.
  3. Selecting a repository, checkout, worktree, or dormant branch exposes only applicable shortcuts, hints, and confirmations, each naming the actual repository, branch, or checkout target.
  4. The user can activate a dormant branch into a managed worktree and can delete a dormant local branch only when Git verifies it is fully merged and not checked out; unsafe deletion is refused.
  5. Existing open, clone, shell, editor, resume, archive, attention, and session-cycle behavior remains available wherever it applies within the hierarchy.

**Plans**: TBD
**UI hint**: yes

### Phase 8: Daemon & Remote TUI Parity

**Goal**: Remote TUI users can manage daemon-hosted repositories with the same identity, active-backend, and fail-closed lifecycle contract as local users while older clients remain safe.
**Depends on**: Phase 7
**Requirements**: SURF-03, SURF-05
**Success Criteria** (what must be TRUE):

  1. From the remote TUI, the user can open or reuse daemon-hosted repositories and perform branch activation, session close or reopen, clean worktree removal, and merged dormant-branch cleanup with the same outcomes and refusals as the local TUI.
  2. Remote actions use daemon-authoritative repository and child identity, so client-local paths cannot select or mutate a daemon-hosted checkout.
  3. Safety failures leave the daemon session, checkout, and persisted child intact and explain why the requested action was refused.
  4. Older flat daemon and session API clients continue to receive a non-destructive compatibility projection during the v2.0 transition.

**Plans**: TBD
**UI hint**: yes

### Phase 9: PWA Hierarchy & Cross-Surface Completion

**Goal**: Phone users can understand and control the same complete repository hierarchy available in both TUIs without weaker Git safety or lost session behavior.
**Depends on**: Phase 8
**Requirements**: HIER-01, SURF-04
**Success Criteria** (what must be TRUE):

  1. Local TUI, remote TUI, and PWA users can all view repository parents with main-checkout, managed-worktree, and dormant-branch children, including repository-name ordering and persisted oldest-first checkout/worktree ordering.
  2. From the PWA, the user can perform each applicable open or reopen, create or activate, close, safe-remove, and Git-verified dormant-branch cleanup action.
  3. PWA actions name the selected target, request confirmation for destructive cleanup, and display the same actionable safety refusals as both TUIs.
  4. Repository parents and retained children remain visible on the phone without running sessions, while waiting, working, archived, and attention states continue to identify the child needing action without reordering the hierarchy.

**Plans**: TBD
**UI hint**: yes

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Full Status-Line Capture | v0.7 | 3/3 | Complete | 2026-06-15 |
| 2. Hook-Driven Status | v0.7 | 3/3 | Complete | 2026-06-15 |
| 3. Tool-Activity Timeline | v0.7 | 4/4 | Complete | 2026-06-15 |
| 4. Remote Permission Approval | v0.7 | 4/4 | Complete | 2026-06-16 |
| 5. Durable Repository Admission | v2.0 | 3/3 | Complete    | 2026-08-30 |
| 6. Safe Managed Worktree Lifecycle | v2.0 | 0/6 | Planned | - |
| 7. Local Repository Hierarchy & Branch Control | v2.0 | 0/TBD | Not started | - |
| 8. Daemon & Remote TUI Parity | v2.0 | 0/TBD | Not started | - |
| 9. PWA Hierarchy & Cross-Surface Completion | v2.0 | 0/TBD | Not started | - |

## Backlog

See `.planning/BACKLOG.md`:

- **BL-01** — sidebar "idle"/status accuracy (addressed by v0.7 Phase 2; confirm in UAT)
- **BL-02** — model / permission-mode / planning-mode not shown for every session (Phase 1 follow-up)
- **BL-03** — wire GSD phase/state into the sidebar (new feature idea)
