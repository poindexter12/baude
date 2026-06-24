# Roadmap: baude

## Milestones

- ✅ **v0.7 Native Claude Integration** — Phases 1-4 (shipped 2026-06-16, code-complete; human UATs deferred)
- 📋 **Next** — TBD (run `/gsd-new-milestone`)

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

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Full Status-Line Capture | v0.7 | 3/3 | Complete | 2026-06-15 |
| 2. Hook-Driven Status | v0.7 | 3/3 | Complete | 2026-06-15 |
| 3. Tool-Activity Timeline | v0.7 | 4/4 | Complete | 2026-06-15 |
| 4. Remote Permission Approval | v0.7 | 4/4 | Complete | 2026-06-16 |

## Backlog

See `.planning/BACKLOG.md`:
- **BL-01** — sidebar "idle"/status accuracy (addressed by v0.7 Phase 2; confirm in UAT)
- **BL-02** — model / permission-mode / planning-mode not shown for every session (Phase 1 follow-up)
- **BL-03** — wire GSD phase/state into the sidebar (new feature idea)
