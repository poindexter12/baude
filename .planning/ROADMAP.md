# Roadmap: baude

## Milestones

- ✅ **v0.7 Native Claude Integration** — Phases 1-4 (shipped 2026-06-16, code-complete; human UATs deferred)
- 🚧 **v2.0 Local TUI Dogfood Release** — Phases 5-7 (in progress)

## Overview

v2.0 delivers a release-ready local-TUI dogfood slice on top of durable repository admission. After preserving the completed admission foundation, it centralizes all lifecycle ownership and recovery in one enforceable `baude-core` protocol, then exposes the safe end-to-end repository and managed-worktree flow in the local TUI for target `v2.0.0-beta`. Dormant branch rows/deletion and remote/PWA hierarchy parity remain future scope.

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

### 🚧 v2.0 Local TUI Dogfood Release (In Progress)

**Milestone Goal:** Make the local TUI safe and complete enough to dogfood persistent repository and managed-worktree sessions, backed by one shared lifecycle authority and release readiness for `v2.0.0-beta` without publishing it.

- [x] **Phase 5: Durable Repository Admission** - Canonical repositories, the approved default-branch contract, migration, and atomic persistence work per workspace. (completed 2026-08-30)
- [ ] **Phase 6: Shared Lifecycle Core Refactor** - One enforceable core protocol owns lifecycle transitions, persistence stages, exact processes, recovery, and rollback across thin App and Manager adapters.
- [ ] **Phase 7: Local TUI Dogfood Release** - Local users can complete the stable repository/worktree flow and verify `v2.0.0-beta` release readiness.

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

### Phase 6: Shared Lifecycle Core Refactor

**Goal**: One enforceable `baude-core` lifecycle ownership protocol/state machine governs Git topology, persistence commit stages, exact agent and shell process ownership, protected recovery transitions, startup recovery, and rollback, while App and Manager remain thin effect adapters with equivalent contracts.
**Depends on**: Phase 5
**Requirements**: CORE-01, CORE-02, CORE-03, CORE-04, CORE-05, CORE-06
**Success Criteria** (what must be TRUE):

  1. The same lifecycle request produces the same legal transition, persistence boundary, effect order, and typed refusal through App and Manager, as demonstrated by mirrored contract tests.
  2. Protected recovery states—including teardown-pending, removal tombstone, activation recovery, and stopped-active recovery—can move only through one explicit legal transition table; reuse or activation cannot overwrite or bypass them.
  3. Before any destructive or replacement effect, exact agent and shell ownership is durable, and neither adapter can forget a possibly live process until teardown is confirmed or durable successor ownership exists.
  4. A crash or injected failure at any persistence/effect boundary recovers on startup or rolls back to truthful Git, durable-state, agent, and shell ownership without a duplicate or orphaned runtime.
  5. Explicit typed lifecycle candidates and provenance survive restart without a generic runtime overlay, and clean review plus phase verification find no unresolved lifecycle ownership gaps.

**Plans**: 7/7 plans locally implemented; certification and phase completion pending

Plans:

- [x] 06-01-PLAN.md — Trace valid named/local branch activation through shared Git, lifecycle, App, and Manager semantics. (execution history)
- [x] 06-02-PLAN.md — Fail closed on invalid refs, path collisions, partial failures, and concurrent creation. (execution history)
- [x] 06-03-PLAN.md — Prove result-valued dirty, conflict, topology, lock, and submodule removal blockers. (execution history)
- [x] 06-04-PLAN.md — Close sessions only after retaining complete durable checkout and conversation context. (execution history)
- [x] 06-05-PLAN.md — Reconcile and reopen retained checkouts with secure targeted backend resume and one runtime. (execution history)
- [x] 06-06-PLAN.md — Confirm and execute double-preflight clean removal while preserving branch, parent, and recovery context. (execution history)
- [x] 06-07-PLAN.md — Correct lifecycle ownership into one shared core state machine. (local implementation complete; certification pending)

### Phase 7: Local TUI Dogfood Release

**Goal**: Local TUI users can dogfood durable repository parents and default sessions, create or activate eligible local branch worktrees, close and reopen them, and safely remove them through stable context-aware UI, with the source tree ready for target `v2.0.0-beta`.
**Depends on**: Phase 6
**Requirements**: REPO-05, HIER-01, HIER-02, HIER-03, HIER-04, WORK-01, WORK-02, WORK-03, WORK-04, WORK-05, WORK-06, SURF-01, SURF-02, SURF-05, REL-01, REL-02, REL-03
**Success Criteria** (what must be TRUE):

  1. The local TUI keeps repository parents visible without running sessions and shows the main checkout, any separate managed default-branch worktree, and other checkout/worktree children beneath the correct parent.
  2. Repository parents remain name-ordered and checkout/worktree children remain persisted oldest-first across restart; waiting, working, archive, and attention changes identify the right child without reordering it.
  3. From repository or eligible child context, the user can create a valid named branch or activate an eligible existing local branch, close and reopen the checkout, and distinctly confirm removal of a clean managed worktree while its branch is retained.
  4. Context actions name the actual target, invalid or unsafe operations fail without partial state or lost work, applicable existing session actions remain available, and older flat daemon/session APIs remain non-destructive compatibility projections without adding daemon hierarchy.
  5. An isolated local end-to-end dogfood run passes across restart, and formatting, lint, tests, package/artifact checks, version metadata, and release documentation are ready for `v2.0.0-beta` without publishing or pushing a release.

**Plans**: 5 plans
**UI hint**: yes

Plans:

- [ ] 07-01-PLAN.md — Trace durable repository/checkout hierarchy and selection through real App data into Ratatui.
- [ ] 07-02-PLAN.md — Dispatch capability-gated create, close, reopen, and safe-remove lifecycle actions.
- [ ] 07-03-PLAN.md — Complete exact responsive rows, hints, modals, Unicode width, focus, and preserved local actions.
- [ ] 07-04-PLAN.md — Prove isolated real-Git restart/dedup dogfood and flat daemon/session compatibility.
- [ ] 07-05-PLAN.md — Synchronize beta readiness metadata/docs/CI and run the full non-publishing local gate.

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Full Status-Line Capture | v0.7 | 3/3 | Complete | 2026-06-15 |
| 2. Hook-Driven Status | v0.7 | 3/3 | Complete | 2026-06-15 |
| 3. Tool-Activity Timeline | v0.7 | 4/4 | Complete | 2026-06-15 |
| 4. Remote Permission Approval | v0.7 | 4/4 | Complete | 2026-06-16 |
| 5. Durable Repository Admission | v2.0 | 3/3 | Complete    | 2026-08-30 |
| 6. Shared Lifecycle Core Refactor | v2.0 | 7/7 | In Progress | - |
| 7. Local TUI Dogfood Release | v2.0 | 0/TBD | Not started | - |

## Backlog

See `.planning/BACKLOG.md`:

- **BL-01** — sidebar "idle"/status accuracy (addressed by v0.7 Phase 2; confirm in UAT)
- **BL-02** — model / permission-mode / planning-mode not shown for every session (Phase 1 follow-up)
- **BL-03** — wire GSD phase/state into the sidebar (new feature idea)
- **Future v2.x scope** — dormant branch rows and safe branch deletion (HIER-05, HIER-06, WORK-07), daemon/remote hierarchy parity (SURF-03), and PWA hierarchy/actions (SURF-04)
