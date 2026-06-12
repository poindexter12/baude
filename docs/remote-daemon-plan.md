# Remote daemon plan (bauded)

High-level plan for splitting baude into a headless session daemon + thin
frontends, so sessions run on a remote box (compose stack, Tailscale sidecar)
and can be driven from a phone without being home.

## Decisions already made

- **No native Claude remote** (claude.ai/code, teleport, etc.). We own the stack.
- **Security = VPN.** Tailscale sidecar in the compose stack; daemon binds the
  tailnet interface. No public exposure, no auth layer needed beyond that.
- **Message-posting model, not terminal streaming.** Clients POST messages to
  sessions and read structured conversation history. No remote vt100
  rendering, no multi-client resize problem, no xterm.js. A raw "terminal
  peek" view is a possible later escape hatch, not part of the core.
- **Remote sessions run bypass/acceptEdits** so TUI permission prompts mostly
  never happen. The rare stuck-on-a-menu case is deferred (see open questions).

## Transcript JSONL findings (the read-side contract)

Verified against live transcripts in `$CLAUDE_CONFIG_DIR/projects/<encoded-cwd>/<sessionId>.jsonl`
(Claude Code 2.1.174). One JSON object per line. Record `type`s observed:

| type | what it is | API use |
|------|-----------|---------|
| `user` | user message. `message.content` is a **string** for typed prompts (`promptSource: "typed"`), or a **list of `tool_result` blocks** for tool results. `isMeta: true` = injected context, skip it. | chat history (typed only); tool results linkable via `tool_use_id` |
| `assistant` | one API call's response. `message.content` blocks: `text`, `thinking`, `tool_use`. Also `message.model`, `message.usage` (full token/cache breakdown), `stop_reason`. Several per turn. | chat history + per-message usage |
| `ai-title` | `aiTitle` — Claude's own session title | session list display name |
| `last-prompt` | last typed prompt + `leafUuid` | cheap "what's it working on" preview |
| `queue-operation` | enqueue/dequeue of messages typed while busy | shows queued messages in UI |
| `permission-mode` / `mode` | mode changes mid-session | metadata |
| `system` | hook summaries etc. (`subtype` field) | mostly skip |
| `file-history-snapshot`, `attachment` | internals | skip |

Threading: every `user`/`assistant` record has `uuid`/`parentUuid` and a
`timestamp`; file order is sufficient for a linear chat view. `isSidechain:
true` marks subagent traffic — filter out (or group later).

**Key insight:** `src/meta.rs` already resolves the transcript path (via
`sessions/<pid>.json` → sessionId, with cwd+startedAt fallback) and already
does incremental offset-tracked tailing (`read_transcript_tail`). The daemon's
message stream is an extension of that code, not new ground. Busy/waiting
status also already comes from `sessions/<pid>.json` (authoritative) with the
output-silence heuristic as fallback.

Write side: POST message → write text + `\r` to the session PTY (`Pty::write_input`).
If the session is busy, Claude Code queues it natively (visible as
`queue-operation` records). No protocol needed.

## Target architecture

```
baude/  (cargo workspace)
├── baude-core/    pty.rs, session.rs, meta.rs, persist.rs, git.rs (today's code, no UI deps)
├── bauded/        axum daemon: REST + SSE over baude-core; owns sessions; systemd/compose
└── baude/         existing ratatui TUI (unchanged at first; later optionally a bauded client)
```

### bauded API sketch

- `GET  /sessions` — id, name, title (ai-title), status (waiting/busy/exited),
  waiting_for_ms, model, context %, permission mode, branch, GSD state, cost
- `POST /sessions` `{repo, worktree?, name?}` — spawn (claude `--continue` semantics as today)
- `DELETE /sessions/:id`
- `GET  /sessions/:id/messages?after=<uuid>` — parsed transcript as chat messages
  (typed user msgs, assistant text, compact tool-call summaries)
- `GET  /sessions/:id/stream` — SSE live tail of the same
- `POST /sessions/:id/messages` `{text}` — inject into PTY
- `POST /sessions/:id/interrupt` — send Esc (stop current work)
- maybe `POST /sessions/:id/keys` `{bytes}` — raw escape hatch for menus

Lifecycle inversion vs today: daemon does **not** `kill_all()` when clients
detach — sessions keep running unattended. `--continue` restore logic covers
daemon restarts (already written).

## Phases

1. **Workspace split** ✅ — extract `baude-core` (pty, session, meta, persist,
   git). Pure refactor, TUI behavior unchanged, CI still green.
2. **bauded** ✅ — axum + tokio daemon over baude-core. REST + SSE endpoints
   above. Transcript→chat-message parser (the only genuinely new logic).
   Sessions survive detach. Daemon state lives in its own
   `daemon-state.json` so it never clobbers the TUI's sessions. Default bind
   `127.0.0.1:8642` (`--bind` / `BAUDED_BIND` for the tailnet interface).
3. **Containerize** — Dockerfile (claude CLI + git + bauded), compose stack
   with Tailscale sidecar (`network_mode: service:tailscale`, optional
   `tailscale serve` for tailnet HTTPS). Volumes: repos, `~/.claude`
   (login + transcripts persist), git automation SSH key. portable-pty is
   fine in Linux containers.
4. **Phone frontend (PWA)** — triage-first: session list showing who's
   waiting and for how long, tap into chat view, post messages.
   Terminal rendering deliberately out of scope.
5. **Later / optional** — TUI as bauded client; push notifications on
   waiting; raw terminal peek; interactive-menu handling.

## Open questions (deferred by choice)

- Phone input for TUI-widget prompts (permission dialogs, option menus) —
  mitigated by bypass mode; revisit after Phase 4.
- Push notifications (iOS Web Push from PWA vs native app) — revisit at Phase 4/5.
- Whether the local TUI converts to a daemon client or stays standalone.
- Statusline bridge (`/tmp/baude-usage-*.json`, `ccusage`) inside the
  container — works in principle, verify in Phase 3.
