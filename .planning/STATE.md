---
gsd_state_version: 1.0
milestone: v2.0
milestone_name: Repository Worktree Management
status: planning
last_updated: "2026-08-30T15:03:03.882Z"
last_activity: 2026-08-30
progress:
  total_phases: 0
  completed_phases: 0
  total_plans: 0
  completed_plans: 0
  percent: 0
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-06-15)

**Core value:** You can see at a glance which of your many Claude Code sessions needs you next — and act on it — whether at the terminal or on your phone.
**Current focus:** Phase 04 — remote-permission-approval

## Current Position

Phase: Not started (defining requirements)
Plan: —
Status: Defining requirements
Last activity: 2026-08-30 — Milestone v2.0 started

## Accumulated Context

### Decisions

Recent decisions affecting current work (full log in PROJECT.md Key Decisions):

- v0.7: GSD-track baude starting at v0.7 (lean scaffold, no full re-interview)
- v0.7: Prefer first-party Claude data (status-line JSON, hooks) over inference
- v0.7: Local hook transport via per-session event files; HTTP only in the daemon
- v0.7: Permission-prompt mode is opt-in; `skip` stays default
- 01-01: Bridge writer factored into testable `build_bridge(v: &Value) -> Value`; reads `session_id` internally so the fn is total (no panic on minimal payload)
- 01-01: Keep `serde_json::Value` accessors (no `#[derive(Deserialize)]`, no branching on `schema`) — this is the STL-02 back-compat guarantee
- 01-01: `vim_mode` is captured/persisted but never rendered (capture-but-don't-render)
- [Phase ?]: 02-01: command string is the hook-merge idempotency sentinel; seed current_exe() absolute path so the hook resolves regardless of session PATH
- [Phase ?]: Plan 02-02: hook events drive session state; precedence Hook(fresh,5s)>SessionFile>Silence via StateSource, silence fallback byte-identical (HOOK-02)
- [Phase ?]: 02-03: BAUDE_EVENT_URL uses loopback DEFAULT_BIND; custom --bind port not honored this phase (deferred)
- [Phase ?]: 02-03: daemon POST /sessions/{id}/event converges with the TUI file-tail onto one /tmp consume path (HOOK-03)
- 03-01: HookEvent is the only serde-Serialize type from ClaudeMeta; the struct itself stays non-serializable (anti-pattern)
- 03-01: capped (200) drop-oldest VecDeque<HookEvent> ring on ClaudeMeta is the single source of truth — appended in read_event_tail's loop, cleared in the WR-03 rotation block (ACT-01)
- [Phase ?]: 03-02: activity feed served two ways — GET /activity (ring snapshot) + GET /activity-stream (standalone SSE file-tail via dedicated EventTail, never the ChatMessage Tail)
- [Phase ?]: PWA HookEvent.ts is unix-ms; activity strip relative time = humanMs(Date.now() - e.ts)
- [Phase ?]: Activity strip is a second standalone EventSource, closed in closeStream alongside the chat ES (no leak)
- 04-01: permission_flag defaults to skip (--dangerously-skip-permissions); prompt reachable ONLY by exact "prompt" (unset/unrecognized/case-mismatch fail safe to skip) — PERM-01/T-04-01 security-critical
- 04-01: env-free permission_flag_for(mode, base_cmd) seam so branch tests never mutate process-global BAUDE_PERMISSION_MODE (races concurrent PTY spawns that read it)
- 04-01: permission flag appended to base cmd unconditionally (mode-driven, not command-sniffing); merge_mcp_config pure non-clobbering seed (only mcpServers.baude set) shared by both spawn sites
- [Phase 04]: 04-02: permission wire-contract functions (parse_frame/parse_tool_call/build_approve_result) isolated in baude-core so the §F CONTRACT UAT corrects them cheaply
- [Phase 04]: 04-02: deny-on-timeout (decide_with_timeout) + permission_timeout_s live in baude-core, single-sourced by both binaries' bridges (security-critical V4)
- [Phase ?]: Permission push stays lean (no tool name); detail via GET /permission (T-04-10)
- [Phase ?]: 04-04: PWA permission card driven off SessionInfo.waiting_reason in the existing refresh() poll (no new SSE handler); state.pendingPermission holds only a live pending GET /permission view, never a resolved decision
- [Phase ?]: 04-04: Deny POSTs {decision:deny} to /permission (denies the single tool call, session survives) — NOT interrupt/kill (T-04-14); every dynamic card string esc()'d (T-04-13)

### Pending Todos

None yet.

### Blockers/Concerns

- Phase 2 (hooks) must tolerate Claude Code hook schema drift and never clobber user settings.json — same care as `bridge.rs::window()` and the statusLine seeding path.
- Web Push (v0.5) is not yet phone-verified; Phase 4's distinct permission push depends on a working push path.
- 04-02 §F CONTRACT gate: live claude 2.1.178 --permission-prompt-tool wire shape must be confirmed before prompt mode ships (see 04-02-SUMMARY.md CONTRACT GATE)
- 04-04 PWA approve/deny card: PERM-03 human-verify UAT pending (browser + live prompt-mode session + real permission). Also gated by 04-02 §F CONTRACT UAT. Code complete + committed (f60bb3f); not marked done until verified.

## Session Continuity

Last session: 2026-06-30T02:07:46.196Z
Stopped at: context exhaustion at 75% (2026-06-30)
Resume file: None

## Performance Metrics

| Phase | Plan | Duration | Notes |
|-------|------|----------|-------|
| Phase 02 P01 | 18min | 3 tasks | 4 files |
| Phase 02 P02 | 6min | 3 tasks | 2 files |
| Phase 03 P01 | 12min | 2 tasks | 1 file |
| Phase 03 P02 | 5min | 2 tasks | 4 files |
| Phase 03 P03 | 8min | 1 tasks | 3 files |
| Phase 04 P01 | 10min | 2 tasks | 5 files |
| Phase 04 P03 | 4min | 2 tasks | 5 files |
| Phase 04 P04 | 12min | 1 tasks | 3 files |

## Operator Next Steps

- Start the next milestone with /gsd-new-milestone

## Deferred Items

Items acknowledged and deferred at v0.7 milestone close on 2026-06-16 (code-complete; human-only verification):

| Category | Item | Status |
|----------|------|--------|
| UAT | Phase 1 — info overlay render (effort/thinking/PR rows) | pending |
| UAT | Phase 3 — PWA activity strip visual render | pending |
| UAT | Phase 3 — TUI `v` activity overlay visual render | pending |
| UAT | Phase 4 — live `claude` 2.1.178 `--permission-prompt-tool` MCP wire contract (§F gate) | pending |
| UAT | Phase 4 — PWA approve/deny card + distinct push (browser/device) | pending |
| verification | Phase 1 / 3 / 4 VERIFICATION.md = human_needed (code-complete; data paths Claude-validated live) | pending |
| deferred | First-real-phone Web Push verification (carried from v0.5) | pending |

All data paths were drive-validated live by Claude; 4 real integration bugs were found and fixed. The above are visual/live-claude observation gaps, not code gaps. Per-phase detail in each `NN-UAT.md`.
