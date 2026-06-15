# Roadmap: baude — v0.7 Native Claude integration

## Overview

v0.7 replaces baude's inferred session state with first-party Claude Code data
and builds remote interactivity on top of it. The work flows along a hard
dependency chain: first make the status-line bridge capture the full payload
(cheap, independent, foundational), then make working/waiting/done state
authoritative by driving it from Claude Code hooks (the keystone), then surface
the hook stream as a live tool-activity feed in both clients, and finally let
the phone approve or deny pending permission prompts — which reuses the hook
layer's `Notification` event and the opt-in `prompt` mode. Source plan:
`docs/plans/tier-1-native-claude-integration.md`.

## Phases

**Phase Numbering:**

- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: Full Status-Line Capture** - Bridge persists Claude's whole status-line payload; info overlay shows effort/thinking/PR (completed 2026-06-15)
- [x] **Phase 2: Hook-Driven Status** - Working/waiting/done state comes from Claude Code hooks, with silence heuristic as labeled fallback (completed 2026-06-15)
- [ ] **Phase 3: Tool-Activity Timeline** - Live per-session tool feed renders in the PWA and the TUI
- [ ] **Phase 4: Remote Permission Approval** - Approve/deny a session's pending permission prompt from the phone, with a distinct push

## Phase Details

### Phase 1: Full Status-Line Capture

**Goal**: The `baude statusline` bridge becomes the authoritative source for the full useful Claude status-line payload, and the new fields surface in the info overlay.
**Depends on**: Nothing (first phase)
**Requirements**: STL-01, STL-02, STL-03
**Success Criteria** (what must be TRUE):

  1. After a managed session runs, `/tmp/baude-usage-<sessionId>.json` contains model, effort, thinking, pr, worktree, and vim.mode (each present only when Claude emitted it), in addition to today's cost/context/rate-limit fields.
  2. The bridge JSON carries `schema: 2`, and a pre-existing reader (an older `meta.rs` build) still parses it without error — the new fields are optional and additive.
  3. Mixed snake_case/camelCase payloads from different Claude Code versions are both parsed correctly (same tolerance as `bridge.rs::window()` today).
  4. Selecting a session and pressing `i` shows that session's effort, thinking mode, and PR state in the info overlay.

**Plans**: 3 plans
Plans:
**Wave 1**

- [x] 01-01-PLAN.md — Capture full status-line payload (model/effort/thinking/pr/worktree/vim) + schema:2 in bridge.rs (TDD)

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 01-02-PLAN.md — Grow ClaudeMeta + read_bridge_file with the new optional fields, back-compat both directions (TDD)

**Wave 3** *(blocked on Wave 2 completion)*

- [x] 01-03-PLAN.md — Surface effort/thinking/pr rows in the local `i` info overlay (ui.rs)

### Phase 2: Hook-Driven Status

**Goal**: A managed session's working/waiting/done state is derived from Claude Code hook events rather than PTY-output silence, with the silence heuristic preserved only as a labeled fallback.
**Depends on**: Phase 1
**Requirements**: HOOK-01, HOOK-02, HOOK-03
**Success Criteria** (what must be TRUE):

  1. On spawn, baude seeds its hook set into the managed session's `settings.json` by merging into existing arrays — a session that already has user-defined hooks or a statusLine keeps them intact (verified by inspecting settings.json after spawn).
  2. A session shows "working" the moment a prompt is submitted (`UserPromptSubmit`), flips to "waiting"/"done" on `Stop`, and reports the last tool it ran (`PostToolUse`) — all without the silence timer firing.
  3. The same event model is consumed from a per-session file-tail (`/tmp/baude-events-<sid>.jsonl`) for TUI-local sessions and from `POST /sessions/{id}/event` in the daemon.
  4. With hooks disabled or unavailable, the session still reaches correct waiting state via the dual-source silence fallback, and that fallback is labeled as such in the state source — no regression from v0.6.1 behavior.

**Plans**: 3 plans
**UI hint**: yes

Plans:

**Wave 1**

- [x] 02-01-PLAN.md — hook.rs (build_event/merge_hook_settings/append_event) + `baude hook` dispatch + TUI settings.local.json seeding (HOOK-01, HOOK-03)

**Wave 2** *(blocked on Wave 1)*

- [x] 02-02-PLAN.md — read_event_tail + hook_status/last_tool capture + StateSource precedence (Hook>SessionFile>Silence), silence fallback unchanged (HOOK-02)

**Wave 3** *(blocked on Wave 2)*

- [x] 02-03-PLAN.md — daemon POST /sessions/{id}/event + $BAUDE_EVENT_URL injection + daemon seeding + overlay surfacing + end-to-end UAT (HOOK-01, HOOK-03)

### Phase 3: Tool-Activity Timeline

**Goal**: The hook event stream is exposed as a live, capped per-session tool-activity feed that renders in both the PWA and the TUI.
**Depends on**: Phase 2
**Requirements**: ACT-01, ACT-02, ACT-03, ACT-04
**Success Criteria** (what must be TRUE):

  1. `manager.rs` retains a per-session ring buffer of recent tool events capped at ~200; older events are dropped, not unbounded.
  2. `GET /sessions/{id}/activity` returns recent events, and new events arrive live over SSE (standalone channel or folded into `/stream`) without a page reload.
  3. In the PWA chat view, a collapsible activity strip shows the recent tool sequence (e.g. "editing src/foo.rs → running cargo test → …") and updates live.
  4. In the TUI, pressing `v` opens an activity overlay mirroring the same feed for the selected session.

**Plans**: 4 plans
**UI hint**: yes

Plans:

**Wave 1**

- [ ] 03-01-PLAN.md — Capped activity ring + HookEvent serde struct in baude-core (ClaudeMeta.activity, ACTIVITY_CAP) (ACT-01, TDD)

**Wave 2** *(blocked on Wave 1)*

- [ ] 03-02-PLAN.md — Daemon GET /activity + standalone /activity-stream SSE (HookEvent event-line tail) + SessionInfo.activity + notify.rs fix (ACT-02, TDD)

**Wave 3** *(blocked on Wave 2; 03 and 04 run in parallel — no file overlap)*

- [ ] 03-03-PLAN.md — PWA collapsible activity strip (openActivity GET-then-SSE, sw.js cache bump) — manual UAT (ACT-03)
- [ ] 03-04-PLAN.md — TUI `v` Modal::Activity overlay (local meta + remote RemoteInfo.activity) — manual UAT (ACT-04)

### Phase 4: Remote Permission Approval

**Goal**: From the phone, a pending tool-permission request can be approved or denied, gated behind an opt-in per-deploy mode, with its own distinct push.
**Depends on**: Phase 2
**Requirements**: PERM-01, PERM-02, PERM-03, PERM-04
**Success Criteria** (what must be TRUE):

  1. `BAUDE_PERMISSION_MODE` defaults to `skip` (sessions run `--dangerously-skip-permissions`, today's unattended behavior); setting it to `prompt` routes tool calls to baude via `--permission-prompt-tool`.
  2. While a request is pending, `GET /sessions/{id}/permission` returns it, and `POST /sessions/{id}/permission {decision: allow|deny, scope?}` resolves it and unblocks (or denies) the session.
  3. In the PWA chat view, an approve/deny card appears while a permission request is pending and disappears once resolved.
  4. A pending permission (driven by the `Notification` hook and a `waiting_reason` of `permission` on `SessionInfo`) fires a distinct push describing the requested action, separate from the generic "waiting" push.

**Plans**: TBD
**UI hint**: yes

Plans:

- [ ] 04-01: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → 4

(Phase 4 depends on Phase 2, not Phase 3; Phases 3 and 4 are independent of each
other and may be planned/executed in either order once Phase 2 lands.)

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Full Status-Line Capture | 3/3 | Complete   | 2026-06-15 |
| 2. Hook-Driven Status | 3/3 | Complete   | 2026-06-15 |
| 3. Tool-Activity Timeline | 0/4 | Not started | - |
| 4. Remote Permission Approval | 0/TBD | Not started | - |
