# Spike Manifest

## Ideas

### opencode-backend
Make baude drive opencode sessions alongside Claude Code via a backend/driver abstraction.
The gating unknown is prompt-mode permission approval: baude's `--permission-prompt-tool
mcp__baude__approve` flow (remote phone approval through the daemon) has no flag equivalent
in opencode, so the backend is only viable if a permission request can be held pending and
answered remotely — either through the server API (`/event` SSE + reply endpoint) or a
plugin `permission.ask` hook with a deferred decision.

**Requirements:**
- Refactor first extracts a backend seam while supporting ONLY Claude Code (no opencode driver yet)
- Remote approval must support both approve and reject, with the tool provably not executing on reject
- The future opencode driver talks to `opencode serve` via HTTP + SSE directly — no plugins, no seeded JS artifacts
- Driver wire-schema assumptions must be written against observed behavior per pinned opencode version and documented in code comments (the bundled SDK types drift from the live server), mirroring the `hook.rs`/`bridge.rs` convention

## Spikes

| # | Idea | Name | Type | Validates | Verdict | Tags |
|---|------|------|------|-----------|---------|------|
| 001a | opencode-backend | permission-reply-server-api | comparison | Given `permission: bash=ask` and a running `opencode serve`, when the agent requests a bash tool call, then `permission.asked` arrives on `/event` and `POST /session/{id}/permissions/{permissionID}` with `once`/`reject` resolves it (tool runs / provably does not run) | VALIDATED ✓ | [opencode, permissions, sse, server-api] |
| 001b | opencode-backend | permission-reply-plugin-hook | comparison | Given a `.opencode/plugins/` plugin registering `permission.ask`, when the agent requests a bash tool call, then a deferred async decision (allow/deny resolved seconds later) is honored | INVALIDATED ✗ (hook never fires on 1.18.16; use server API) | [opencode, permissions, plugin] |
