# Roadmap: baude

## Milestones

- ✅ **v0.7 Session Visibility** — Phases 1-4 (shipped 2026-07-02) ([archive](milestones/v0.7-ROADMAP.md))
- ✅ **v2.0 Local TUI Dogfood Release** — Phases 5-7 (shipped 2026-09-03) ([archive](milestones/v2.0-ROADMAP.md))
- ✅ **v2.2 Reliability and Terminal Usability** — Phases 8-12 (shipped 2026-09-19 as v2.2.0) ([archive](milestones/v2.2-ROADMAP.md))
- ✅ **v2.3 Launch Defaults and Startup Speed** — Phases 13-17 (shipped 2026-09-22 as v2.3.0) ([archive](milestones/v2.3-ROADMAP.md))

v2.1.0 through v2.1.5 were release-please point releases between v2.0 and v2.2 and do not add roadmap phases. The same holds after v2.3: v2.3.1 through v2.6.0 shipped from the backlog and from sidequest tickets rather than from GSD phases, so no roadmap phase covers them.

No milestone is currently in progress.

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

## Backlog

See `.planning/BACKLOG.md`:

- **BL-01** — sidebar "idle"/status accuracy (addressed by v0.7 Phase 2; confirm in UAT)
- **BL-02** — model / permission-mode / planning-mode not shown for every session (Phase 1 follow-up)
- **BL-03** — wire GSD phase/state into the sidebar (new feature idea)

---
*Last updated: 2026-10-13 after v2.3 milestone closeout.*
