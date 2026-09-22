# Roadmap: baude

## Milestones

- ✅ **v0.7 Session Visibility** — Phases 1-4 (shipped 2026-07-02) ([archive](milestones/v0.7-ROADMAP.md))
- ✅ **v2.0 Local TUI Dogfood Release** — Phases 5-7 (shipped 2026-09-03) ([archive](milestones/v2.0-ROADMAP.md))
- ✅ **v2.2 Reliability and Terminal Usability** — Phases 8-12 (shipped 2026-09-19 as v2.2.0) ([archive](milestones/v2.2-ROADMAP.md))
- 🚧 **v2.3 Launch Defaults and Startup Speed** — Phases 13-17 (in progress)

v2.1.0 through v2.1.5 were release-please point releases between v2.0 and v2.2 and do not add roadmap phases.

## Phases

<details>
<summary>✅ v0.7 Session Visibility (Phases 1-4) — SHIPPED 2026-07-02</summary>

- [x] Phase 1: Session Metadata (see milestones/v0.7-ROADMAP.md)
- [x] Phase 2: Working/Waiting Signal
- [x] Phase 3: Session Actions
- [x] Phase 4: Remote Permission Approval

</details>

<details>
<summary>✅ v2.0 Local TUI Dogfood Release (Phases 5-7) — SHIPPED 2026-09-03</summary>

- [x] Phase 5: Durable Repository Admission (3/3 plans) — completed 2026-08-30
- [x] Phase 6: Shared Lifecycle Core Refactor (7/7 plans) — completed 2026-09-02
- [x] Phase 7: Local TUI Dogfood Release (6/6 plans) — completed 2026-09-03

Published as the `v2.0.0-beta` prerelease bootstrap and handed to
release-please at `v2.0.0-beta.1`. Full phase detail:
[milestones/v2.0-ROADMAP.md](milestones/v2.0-ROADMAP.md).

</details>

<details>
<summary>✅ v2.2 Reliability and Terminal Usability (Phases 8-12) — SHIPPED 2026-09-19</summary>

- [x] Phase 8: Test Isolation and Fixture Ownership (8/8 plans) — completed 2026-09-16
- [x] Phase 9: Hook Seeding Safety (4/4 plans) — completed 2026-09-15
- [x] Phase 10: Clickable Terminal Links (4/4 plans) — completed 2026-09-16
- [x] Phase 11: Negotiated Multiline Input (4/4 plans) — completed 2026-09-16
- [x] Phase 12: Validation and v2.2.0 Release (5/5 plans) — completed 2026-09-19 (closed by override; see MILESTONES.md Known Gaps)

</details>

## v2.3 Launch Defaults and Startup Speed (Phases 13-17)

- [x] **Phase 13: Workspace Derivation and New-Session/Open Defaults** - Opening baude from any folder uses the right workspace and repository (completed 2026-09-20)
- [x] **Phase 14: Managed Worktree Identity** - Managed worktrees carry stable repository identity preventing cross-repo collisions (completed 2026-09-21)
- [x] **Phase 15: Startup and Idle Performance** - Startup is measurable and fast, and an idle baude stops burning CPU and battery (completed 2026-09-22)
- [ ] **Phase 16: Pane Focus UX** - Pane focus is remembered across session switches
- [ ] **Phase 17: Validation and v2.3.0 Release** - v2.3.0 is published after tests, CI, and smoke validation

## Phase Details

### Phase 13: Workspace Derivation and New-Session/Open Defaults

**Goal**: Opening baude from any folder automatically uses the right workspace and repository, with recorded defaults and explicit config still winning.

**Depends on**: Nothing (first phase of this milestone)

**Requirements**: WSPC-01, WSPC-02, WSPC-03, WSPC-04, WSPC-05, OPEN-01, OPEN-02, OPEN-03, OPEN-04

**Success Criteria** (what must be TRUE):

  1. User launches baude from a subfolder of a repository and the correct workspace is used (either from a recorded folder binding or derived from the repo root folder name)
  2. The new-session (`n`) prompt prefills the git toplevel when launched inside a repo, or the configured `new_session_dir` when launched outside any repository
  3. Explicit `BAUDE_WORKSPACE`, config `workspace`, and `BAUDE_BACKEND` env/config still override derivation and recorded bindings
  4. Launching baude from different subfolders of the same repository and from its root all admit the same repository row (no duplicates)
  5. The README documents the full derivation rule, precedence chain, and examples for workspace and default-path selection
  6. The top title of the TUI names the active workspace and how it was chosen, or shows a `(blank)` placeholder when none applies, so the landing workspace is visible at a glance

**Plans**: 3/3 plans executed
**Wave 1**

- [x] 13-01-PLAN.md — Implement workspace derivation and TUI title end-to-end

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 13-02-PLAN.md — Comprehensive unit and integration tests for resolution, prefill, and deduplication

**Wave 3** *(blocked on Wave 2 completion)*

- [x] 13-03-PLAN.md — Daemon parity and README documentation

### Phase 14: Managed Worktree Identity

**Goal**: Managed worktrees carry stable repository identity preventing cross-repo collisions and enabling safe in-place migration.

**Depends on**: Phase 13

**Requirements**: WTID-01, WTID-02, WTID-03, WTID-04

**Success Criteria** (what must be TRUE):

  1. Two repositories never resolve to the same managed worktree path, verified across TUI and daemon state files and after a state reset
  2. Existing managed checkouts work unchanged under the new identity scheme (no deletions, no re-cloning, in-place migration)
  3. When a collision is detected, baude names the repository that owns the path and offers non-destructive resolution instead of a hard error
  4. `baude worktrees scan` reports the owning repository for each managed checkout and surface any collisions with ownership info

**Plans**: 3/3 plans executed
**Wave 1**

- [x] 14-01-PLAN.md — Marker module and digest resolution (tracer: end-to-end proof of identity scheme)
- [x] 14-02-PLAN.md — Path composition and collision detection integration

**Wave 3** *(blocked on Wave 1 completion)*

- [x] 14-03-PLAN.md — Admission flows, scan output, and documentation

### Phase 15: Startup and Idle Performance

**Goal**: Startup is measurable and reaches the first frame quickly without blocking on external services, and an idle baude with many sessions costs near-zero CPU: no unconditional or timer-driven redraws, no polling of dead rows, and an opt-in way to suspend idle Claude children.

**Depends on**: Phase 14

**Requirements**: PERF-01, PERF-02, PERF-03, PERF-04, PERF-05, PERF-06, PERF-07, PERF-08, UX-02

**Success Criteria** (what must be TRUE):

  1. A timing facility (env var or flag) logs each startup stage's duration so a slow launch can be diagnosed without a debugger
  2. The first frame renders before session restore and the first metadata poll complete
  3. The kitty keyboard probe never delays the first frame beyond a short bound and degrades to legacy encoding when the terminal does not answer
  4. Session restore writes the durable state file once, batched, instead of several fsync'd full rewrites per restored session
  5. With no input and no session activity, baude issues no terminal writes; working and waiting rows use static status glyphs (no wall-clock spinner or flash), so the only redraw triggers are input, child output, status transitions, and resize
  6. Idle per-session polling touches only live, non-archived rows and skips unchanged files, so CPU no longer grows with the session count
  7. An opt-in setting suspends or stops idle Claude children after the auto-archive timeout, and the user can see which children are suspended
  8. The usage poller can be disabled or slowed from config
  9. Every session and checkout row shows a static single-character status code (`?` waiting, `B` busy, `✓` completed, `✗` exited, `-` closed, `A` archived, `!` unavailable) in its existing per-state color, readable without a key, with an in-app legend; the old circle and spinner glyphs are gone

**Plans**: TBD

- [x] 15-01-PLAN.md
- [x] 15-02-PLAN.md
- [x] 15-03-PLAN.md
- [x] 15-04-PLAN.md

### Phase 16: Pane Focus UX

**Goal**: Users can keep consistent pane focus (Claude or shell) across session switches.

**Depends on**: Phase 15

**Requirements**: UX-01

**Success Criteria** (what must be TRUE):

  1. Pane focus (Claude pane or expanded shell pane) is remembered when the user switches to a different session and back
  2. A keyboard shortcut toggles focus between the Claude pane and shell pane while both are visible

**Plans**: TBD

- [x] 16-01-PLAN.md

### Phase 17: Validation and v2.3.0 Release

**Goal**: v2.3.0 is published after tests, CI, and smoke validation confirm stability and the new features work end-to-end.

**Depends on**: Phase 16

**Requirements**: SHIP-05

**Success Criteria** (what must be TRUE):

  1. All tests pass (`cargo test --workspace`)
  2. Clippy reports no warnings (`cargo clippy -D warnings`)
  3. Terminal smoke test confirms the new defaults, workspace derivation, and startup performance work end-to-end
  4. Release notes and README reflect the new workspace derivation behavior, new-session defaults, worktree identity scheme, and startup improvements

**Plans**: TBD

## Progress Table

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 13. Workspace Derivation and New-Session/Open Defaults | 3/3 | Complete    | 2026-09-20 |
| 14. Managed Worktree Identity | 3/3 | Complete    | 2026-09-21 |
| 15. Startup and Idle Performance | 4/4 | Complete    | 2026-09-22 |
| 16. Pane Focus UX | 1/1 | In Progress|  |
| 17. Validation and v2.3.0 Release | 0/? | Not started | - |

## Backlog

See `.planning/BACKLOG.md`:

- **BL-01** — sidebar "idle"/status accuracy (addressed by v0.7 Phase 2; confirm in UAT)
- **BL-02** — model / permission-mode / planning-mode not shown for every session (Phase 1 follow-up)
- **BL-03** — wire GSD phase/state into the sidebar (new feature idea)

---
*Last updated: 2026-09-19. Roadmap created for v2.3 milestone.*
