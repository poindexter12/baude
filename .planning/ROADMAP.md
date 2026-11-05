# Roadmap: baude

## Milestones

- ✅ **v0.7 Session Visibility** — Phases 1-4 (shipped 2026-07-02) ([archive](milestones/v0.7-ROADMAP.md))
- ✅ **v2.0 Local TUI Dogfood Release** — Phases 5-7 (shipped 2026-09-03) ([archive](milestones/v2.0-ROADMAP.md))
- ✅ **v2.2 Reliability and Terminal Usability** — Phases 8-12 (shipped 2026-09-19 as v2.2.0) ([archive](milestones/v2.2-ROADMAP.md))
- ✅ **v2.3 Launch Defaults and Startup Speed** — Phases 13-17 (shipped 2026-09-22 as v2.3.0) ([archive](milestones/v2.3-ROADMAP.md))
- 🚧 **v2.8 Project Removal** — Phases 18-22 (in progress, started 2026-11-01)

v2.1.0 through v2.1.5 were release-please point releases between v2.0 and v2.2 and do not add roadmap phases. The same holds after v2.3: v2.3.1 through v2.6.0 shipped from the backlog and from sidequest tickets rather than from GSD phases, so no roadmap phase covers them.

**Current milestone:** v2.8 Project Removal — Phases 18-22. Requirements in `.planning/REQUIREMENTS.md`; research brief in `.planning/research/v2.8-research-brief.md`.

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

<details>
<summary>✅ v2.3 Launch Defaults and Startup Speed (Phases 13-17) — SHIPPED 2026-09-22</summary>

- [x] Phase 13: Workspace Derivation and New-Session/Open Defaults (3/3 plans) — completed 2026-09-20
- [x] Phase 14: Managed Worktree Identity (3/3 plans) — completed 2026-09-21
- [x] Phase 15: Startup and Idle Performance (4/4 plans) — completed 2026-09-22
- [x] Phase 16: Pane Focus UX (1/1 plans) — completed 2026-09-22
- [x] Phase 17: Validation and v2.3.0 Release (1/1 plans) — completed 2026-09-22

Published as `v2.3.0` via release-please (PR #91). Records closed out
2026-10-13 as `override_closeout` — every phase had passed verification in
September but read `stale` by then, main having advanced through v2.6.0.
Full phase detail: [milestones/v2.3-ROADMAP.md](milestones/v2.3-ROADMAP.md).
Known gaps: see MILESTONES.md.

</details>

### 🚧 v2.8 Project Removal (Phases 18-22) — IN PROGRESS

**Goal:** Remove a repository, checkout, worktree or standalone row from baude in one action, even while a runtime is live, so the project is gone from the sidebar, the state file and every rediscovery path, with the same removal available over the daemon REST API and from the PWA.

- [x] Phase 18: Shared Kill-Then-Remove Lifecycle (8/8 plans) (completed 2026-11-03)
- [ ] Phase 19: Re-admission Suppression (0/? plans)
- [ ] Phase 20: TUI Remove Action (0/? plans)
- [ ] Phase 21: Daemon and PWA Parity (0/? plans)
- [ ] Phase 22: Validation and v2.8.0 Release (0/? plans)

### Phase 18: Shared Kill-Then-Remove Lifecycle

**Goal**: `baude-core` can remove any row kind in one operation that kills the owned runtime first, removes managed worktrees from disk, never deletes a main checkout, standalone folder or local branch, and persists atomically, so the TUI and daemon share one removal path.

**Depends on**: Nothing (first phase of this milestone)

**Requirements**: KILL-01, KILL-02, KILL-03, RMV-01, RMV-02, RMV-03, RMV-04, RMV-05, RMV-06, RMV-07

**Success Criteria** (what must be TRUE):

  1. Removing a row with a live runtime terminates the owned process group with SIGTERM then SIGKILL on macOS and Linux, and no zombie or orphan attributable to that row remains
  2. A runtime that survives escalation leaves the row in place with an error instead of dropping the row and leaking the process
  3. Removing a managed worktree child deletes its worktree directory even when the tree is unclean or a runtime held it as cwd; removing a main checkout or standalone row leaves the directory and every local branch untouched, and tests on cloned and `git init` fixtures assert each of these
  4. Removing a repository parent removes each child by the per-child rules and then the parent, in one lifecycle operation composed from the existing kill, close and managed-worktree removal stages
  5. A crash mid-removal leaves either the prior state or the fully removed state, never a parent without children or a child without its parent

**Plans**: 8/8 plans complete

Plans:
- [x] 18-01-PLAN.md — `removal.rs` vocabulary, `select_rows` and side-effect-free `preview_removal` for every row kind (TDD)
- [x] 18-02-PLAN.md — `git::remove_managed_worktree_forced` with managed-root containment, prune fallback and postcondition proof (TDD)
- [x] 18-03-PLAN.md — `RepositoryState::remove_rows` batch aggregate mutation (TDD)
- [x] 18-04-PLAN.md — `removal::remove` preflight, validate, kill, disk, single-save orchestration with owning handles and live process doubles (TDD)
- [x] 18-05-PLAN.md — every persistence failure mode, crash and disk failure mid-fan-out, validation-before-kill proof
- [x] 18-06-PLAN.md — keep rules with and without live runtimes, unmanaged and already-removed rows, parent fan-out on cloned and `git init` fixtures
- [x] 18-07-PLAN.md — `scripts/gate.sh` on macOS and `--linux` on the same final commit, gate-driven fixes only
- [x] 18-08-PLAN.md — `Pty::kill_and_wait` group extinction after leader exit, pid-reuse disposition and reader join (TDD, wave 1)

### Phase 19: Re-admission Suppression

**Goal**: A removed project stays gone across reconcile and relaunch because removal clears its folder-workspace binding and marker ownership, and every rediscovery path respects that, while `n`, `c` and launching inside the folder still admit it fresh.

**Depends on**: Phase 18

**Requirements**: READM-01, READM-02, READM-03, READM-04, READM-05

**Success Criteria** (what must be TRUE):

  1. After removal, the launch-dir binding walk no longer resolves the removed folder to a workspace and marker recovery does not restore the row
  2. The session-start sweep, worktree scan, marker recovery and reconcile do not re-admit the removed repository, checkout, worktree or standalone row across a TUI relaunch
  3. `n`, `c`, or launching baude inside the folder admits the project again as a fresh row with no stale identity
  4. Other rows' bindings, markers and worktrees are unchanged by a removal, including repositories sharing the same workspace

**Plans**: 7/7 plans executed

Plans:
**Wave 1**
- [x] 19-01-PLAN.md — binding clear as the fourth `removal::remove` stage with `ClearedItem` report and preview parity (TDD, tracer)
- [x] 19-02-PLAN.md — `suppression.rs` per-workspace advisory store: consult, lift, suppress, drop_children (TDD)
- [x] 19-03-PLAN.md — `marker::clear_owned` owner-checked marker clear with non-recursive directory removal (TDD)

**Wave 2** *(blocked on Wave 1 completion)*
- [x] 19-04-PLAN.md — markers, managed dirs and suppression entries in the clearing stage, D-15 neighbour-safety proof (TDD)
- [x] 19-05-PLAN.md — TUI sweep consult and lift on `n`, `c` and launch-dir admission (TDD)

**Wave 3** *(blocked on Wave 2 completion)*
- [x] 19-06-PLAN.md — relaunch proofs through a second `App`, crumb pruning on flush

**Wave 4** *(blocked on Wave 3 completion)*
- [x] 19-07-PLAN.md — `scripts/gate.sh` on macOS and `--linux` on the same final commit, gate-driven fixes only

### Phase 20: TUI Remove Action

**Goal**: The user can remove any row kind from the sidebar through a confirmation that names exactly what will be killed, dropped and deleted, and a failure leaves a visible, retryable state.

**Depends on**: Phase 19

**Requirements**: TUI-01, TUI-02, TUI-03, TUI-04

**Success Criteria** (what must be TRUE):

  1. The remove action is reachable on repository, main checkout, managed worktree, unmanaged checkout and standalone rows, and the footer or help lists it beside `x`, `w` and `X`
  2. The confirmation names the row kind, the rows to be dropped, whether a live runtime will be killed, and which paths are deleted versus left in place; for a repository parent it lists every child
  3. On confirm the rows disappear from the sidebar and the state file in one step
  4. A failure at any stage leaves the affected rows visible with an error notice and the user can retry

**Plans:** 4 plans

Plans:
- [ ] 20-01-PLAN.md — Pure RemovalReport modal-line renderer with scroll and +N more
- [ ] 20-02-PLAN.md — D dispatch, ConfirmRemove modal, prepare and confirm over removal::remove with handle custody
- [ ] 20-03-PLAN.md — draw_modal arm at 80% height, footer and help strings with exact-string tests
- [ ] 20-04-PLAN.md — Kill and save failure retry proofs, clearing notice, recheck, gates

### Phase 21: Daemon and PWA Parity

**Goal**: The same removal is available over the daemon REST API with SSE updates and from the PWA with an equivalent confirmation, through the shared `baude-core` operation.

**Depends on**: Phase 20

**Requirements**: API-01, API-02, API-03

**Success Criteria** (what must be TRUE):

  1. A REST removal endpoint accepts repository, checkout, worktree and standalone rows, calls the Phase 18 operation, and returns the rows removed, runtimes killed and paths deleted
  2. A connected TUI or PWA drops the removed rows on the SSE event without a refresh
  3. The PWA offers remove on the same row kinds with a confirmation equivalent to the TUI's and completes it through the REST endpoint

**Plans**: 6 plans

Plans:
- [ ] 21-01-PLAN.md — Tracer: Manager preview and remove over core with runtime custody, wired to POST /removals/preview and POST /removals for one checkout row
- [ ] 21-02-PLAN.md — GET /rows hierarchy with core keys, checkout_key on sessions, repository/standalone/main kinds, suppression lift on daemon admission
- [ ] 21-03-PLAN.md — Failure posture over REST: 409 drift with fresh report, 500 and 503 with handles restored, echoed paths never trusted
- [ ] 21-04-PLAN.md — Daemon-wide GET /events SSE stream publishing rows_removed after the durable save
- [ ] 21-05-PLAN.md — PWA remove on all four row kinds with a report-backed modal, 409 re-render, retry, and live row drops on /events
- [ ] 21-06-PLAN.md — TUI D on remote rows through the daemon with the shared modal, plus the remote /events subscriber and repository gates

### Phase 22: Validation and v2.8.0 Release

**Goal**: Removal is proven end to end on realistic fixtures for every row kind with and without a live runtime, documented, and shipped as v2.8.0 through release-please.

**Depends on**: Phase 21

**Requirements**: REL-01, REL-02

**Success Criteria** (what must be TRUE):

  1. Integration tests cover each row kind with a live runtime and without, on cloned and `git init` fixtures, and assert on fixture output that process, worktree, binding and marker state are gone
  2. README and PWA docs describe the remove action, its confirmation, and what is and is not deleted
  3. `scripts/gate.sh` and `scripts/gate.sh --linux` pass and v2.8.0 is released through release-please

**Plans**: TBD

## Backlog

See `.planning/BACKLOG.md`:

- **BL-01** — sidebar "idle"/status accuracy (addressed by v0.7 Phase 2; confirm in UAT)
- **BL-02** — model / permission-mode / planning-mode not shown for every session (Phase 1 follow-up)
- **BL-03** — wire GSD phase/state into the sidebar (new feature idea)

---
*Last updated: 2026-11-02 after v2.8 roadmap creation.*
