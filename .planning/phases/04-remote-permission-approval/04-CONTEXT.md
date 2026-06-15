# Phase 4: Remote Permission Approval - Context

**Gathered:** 2026-06-15
**Status:** Ready for planning

<domain>
## Phase Boundary

From the phone, a pending tool-permission request can be approved or denied,
gated behind an opt-in per-deploy mode, with its own distinct push. Requirements
PERM-01..04. Builds on the Phase 2 hook/`Notification` stream and the existing
v0.5 Web Push path. `skip` mode (today's unattended `--dangerously-skip-permissions`
behavior) stays the default — `prompt` is strictly opt-in and must never become
the unattended default. This is the final phase of the v0.7 milestone.

</domain>

<decisions>
## Implementation Decisions

### Permission Mode & Spawn Wiring
- A per-deploy **`BAUDE_PERMISSION_MODE = skip | prompt`** env var (matches the
  `BAUDE_CLAUDE_CMD`/`BAUDED_BIND` convention), **default `skip`**.
- `skip` → append **`--dangerously-skip-permissions`** to the base claude command
  (unless the base cmd already carries a permission flag — don't double-add);
  `prompt` → append **`--permission-prompt-tool <tool>`**.
- `prompt` routes permission checks to a **baude bridge** that Claude invokes —
  mirroring the `baude hook` bridge: it forwards the request to the daemon and
  returns the decision. **The exact `--permission-prompt-tool` contract (MCP tool
  name vs command, the request/response JSON shape) is RESEARCH-GATED** — research
  must pin Claude Code's actual contract before the planner commits the transport.
- Decision shape: **`allow | deny`** plus an optional **`scope`** passthrough (the
  tool input may carry scope); keep minimal — no rich once/session/always UI.

### Daemon Permission State & API
- Pending state lives on the **daemon `Session`** as
  `pending_permission: Option<PendingPermission { request_id, tool, input, ts }>`
  (`Manager` owns set/resolve), since this is daemon-mediated.
- **`GET /sessions/{id}/permission`** returns the pending request (or 204/null);
  **`POST /sessions/{id}/permission { decision: allow|deny, scope? }`** resolves it.
- Unblock: the bridge POSTs the request, then **waits (bounded poll/long-poll) on
  the daemon until a decision is POSTed**, then returns it to Claude.
- **Deny on timeout** after a long phone-approval window (never auto-allow — the
  safe default). The exact window is configurable and research-tuned.

### waiting_reason & Distinct Push (PERM-04)
- Add a **`waiting_reason` enum `{ permission, input, none }`** on `SessionInfo`,
  derived from `last_notification` (a recent `permission_prompt` → `permission`,
  else `input` when waiting, else `none`).
- A pending permission fires a **distinct push** via a separate
  **`notified_permission`** set in `Notifier`, with a title/body describing the
  action (e.g. "wants to run `rm -rf build/` — approve?"), **re-armed when the
  permission resolves**. Separate from the generic "waiting" push.
- Push payload stays **lean** (`sid` + a permission marker); the PWA fetches
  `GET /permission` for the tool/input details.
- The distinct push **builds on the existing v0.5 Web Push path** (additive).
  Phone-verification of Web Push is a **separate manual step** — flagged, NOT a
  blocker for implementing this phase.

### PWA Approve/Deny Card
- A card **above the composer** in the chat view appears while
  `waiting_reason === "permission"` AND a pending permission exists; it shows the
  tool + an input summary with **Approve / Deny** buttons.
- The PWA learns of a pending permission via **push/SSE (`waiting_reason=permission`)
  and on chat open**, then fetches `GET /permission` for details.
- On resolve: **`POST /permission` → optimistic card removal + refetch** (the card
  disappears once resolved — SC3).
- **Deny denies the single tool call** (Claude continues with the tool denied) —
  it does NOT kill/interrupt the session.

### Claude's Discretion
- The exact `PendingPermission`/`request_id` shape, the bridge's poll cadence and
  timeout window, the `--permission-prompt-tool` wiring details, and the card's
  input-summary formatting are at Claude's discretion, constrained by the
  research-pinned Claude Code contract and existing hook/notify/PWA conventions.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `bauded/src/manager.rs:126-133` `spawn_command()` (+ `default_claude_cmd` 146-151)
  and `baude/src/app.rs:236-241` `claude_cmd()` — where the permission flag is
  chosen. NOTE: nothing appends `--dangerously-skip-permissions` today (it's only
  a comment in `persist.rs:41`), so `skip` mode makes the default explicit.
- `baude-core/src/meta.rs:398-474` `read_event_tail` already captures
  `last_notification: Option<(String,u64)>` (130) on `Notification` events —
  "Carried so Phase 4 can distinguish permission vs idle prompts."
- `bauded/src/manager.rs:635-665` `session_info()` builds `SessionInfo` (gets the
  new `waiting_reason`); `session.rs:24-31` `StateSource`.
- `bauded/src/api.rs:21-43` router; `interrupt` (213-218) / `post_event` (256-262)
  are the `Path<u64>` GET/POST handler analogs for the permission routes.
- `bauded/src/notify.rs:42-94` `Notifier::tick()` + `Notification`/`to_json` (22-39),
  `push.rs:247-269` `send()` — the distinct push slots in alongside `notified_waiting`.
- `bauded/web/app.js:561-656` `renderChat()`; POST action pattern at 293/308
  (send/interrupt) — the approve/deny card + `api(.../permission, {method:POST})`.
- `baude-core/src/hook.rs:204-212` `dispatch_hook` + `baude/src/main.rs:40-53` /
  `bauded/src/main.rs:31-44` `run_hook` — the bridge analog for a
  `permission-prompt` subcommand (stdin request → daemon POST → blocked decision).

### Established Patterns
- Bridge subcommands (`hook`) read stdin, route to `$BAUDE_EVENT_URL`/daemon with a
  bounded ureq agent, best-effort. The permission bridge mirrors this but must BLOCK
  for a decision (with a timeout) rather than fire-and-forget.
- Notifier debounces per-status with `notified_*` sets, re-armed on edge.
- PWA vanilla JS, no build step; `esc()` all dynamic strings (XSS).

### Integration Points
- Spawn flag selection (manager.rs + app.rs); `permission-prompt` subcommand in both
  binaries; daemon permission routes + `Manager` pending state; `waiting_reason` on
  SessionInfo + RemoteInfo; distinct push in Notifier; PWA card.

</code_context>

<specifics>
## Specific Ideas

- `skip` MUST stay the unattended default — `prompt` is opt-in per deploy; a
  regression that makes prompt the default would block overnight runs (explicit
  PROJECT non-negotiable).
- Deny-on-timeout, never auto-allow — safety over convenience for the unattended
  blocking case.
- The permission bridge is the one place a baude subcommand BLOCKS on Claude's
  critical path with a long timeout — unlike the always-exit-0 hook; document the
  contrast.

</specifics>

<deferred>
## Deferred Ideas

- Rich permission scopes (once/session/always allow-lists), permission history /
  audit log, and a TUI approve/deny surface (this phase is phone/PWA-first per the
  ROADMAP) — out of scope unless research/UAT surfaces a need.
- First real-phone Web Push verification — a separate manual milestone task, not
  part of this phase's code.

</deferred>
