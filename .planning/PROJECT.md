# baude

## What This Is

baude is a Rust workspace for running and orchestrating many coding-agent
sessions at once. Its ratatui **TUI** (`baude`) manages Claude Code and OpenCode
sessions across repositories and git worktrees, while a headless **daemon**
(`bauded`) owns sessions over REST/SSE/WebSocket and a phone-first **PWA** makes
them available remotely. It is single-user and self-hosted, secured by binding
a Tailscale/VPN interface rather than an auth layer.

## Core Value

You can see at a glance which of your many coding-agent sessions needs you next —
and act on it — whether you're at the terminal or on your phone.

## Current Milestone: v2.0 Repository Worktree Management

**Goal:** Make repositories persistent, navigable UI parents so the active backend can work on the default branch immediately and create isolated worktrees for parallel work.

**Target features:**
- Opening or cloning a repository creates a persistent repository parent and launches its default-branch session using the active workspace backend
- Worktree sessions appear beneath their repository parent and can be created from named branches
- Managed worktrees can be closed or safely removed, with removal blocked while uncommitted changes exist
- Repository and worktree actions use context-aware shortcuts
- Repository hierarchy and worktree metadata persist across restart

## Requirements

### Validated

<!-- Shipped and confirmed valuable. -->

- ✓ Multi-session TUI with stable sidebar order, in-place waiting flash, and live status (waiting/working/exited) — v0.1–v0.2
- ✓ Git-worktree sessions for parallel work in one repo; keep/remove on close — v0.1
- ✓ Per-session shell pane and "open folder in editor" — v0.2
- ✓ Live Claude metadata from disk (model, context %, permission mode, tokens, GSD state) — v0.3
- ✓ Usage/cost panel: per-session cost, today/week via ccusage, 5h + weekly rate-limit windows; `baude statusline` bridge — v0.3
- ✓ Headless `bauded` daemon: REST + SSE, sessions survive client detach, restore via `claude --continue` — v0.4
- ✓ Containerized deploy: Dockerfile + compose behind a Tailscale sidecar (VPN-only) — v0.4
- ✓ Phone PWA: triage list, chat with live SSE, queued-message bubbles, terminal-peek drawer, interrupt, create/kill/restart — v0.4
- ✓ TUI attaches to remote daemon sessions over WebSocket (raw PTY) — v0.5
- ✓ Web Push notifications when a session waits or exits — v0.5
- ✓ Idle-session archiving: auto after 30m, manual everywhere — v0.6
- ✓ Full Claude Code status-line payload capture (model, effort, thinking, PR, worktree, vim) via the schema:2 bridge — v0.7
- ✓ Hook-driven working/waiting/done state (Claude Code hooks; silence heuristic demoted to a labeled `StateSource` fallback) — v0.7
- ✓ Live per-session tool-activity timeline (capped ring → `GET /activity` + SSE) in the PWA and TUI — v0.7
- ✓ Remote tool-permission approve/deny from the phone (opt-in `prompt` mode via `--permission-prompt-tool` MCP bridge; distinct push) — v0.7
- ✓ Backend-isolated Claude Code and OpenCode workspaces, commands, metadata, and session pools — v0.8-v0.14

> v0.7 code-complete; data paths Claude-validated live (4 integration bugs found + fixed). Pending human UATs before public ship: hook-state flip visual (BL-01), PWA activity-strip + TUI `v` overlay visuals, live-`claude` `--permission-prompt-tool` MCP wire contract, first-phone Web Push. Tracked in `.planning/STATE.md` Deferred Items + per-phase UAT.md.

### Active

<!-- Current milestone scope. -->

- [ ] Repositories are persistent parent entries with their default branch opened automatically in the active backend
- [ ] Worktrees are created and displayed beneath their repository for parallel work
- [ ] Managed worktrees can be closed or removed safely without discarding dirty changes
- [ ] Open, clone, session, and worktree shortcuts behave according to repository context
- [ ] Repository/worktree hierarchy survives restart

### Out of Scope

<!-- Explicit boundaries. -->

- Multi-user / auth layer — security model is "bind the VPN interface"; single-user by design
- Native Claude remote (claude.ai/code, `--remote-control`) as the backend — baude owns its own stack
- Supporting coding agents other than Claude Code and OpenCode — backend support is intentionally explicit
- Remote vt100 rendering as the primary remote UX — the message/chat model is the core; raw PTY is an escape hatch

## Context

- Mature codebase at **v0.14.0**; public repo `github.com/poindexter12/baude`, MIT.
- Cargo workspace: `baude-core/` (pty, session, meta, persist, git, bridge — no UI deps), `baude/` (ratatui TUI), `bauded/` (axum daemon + embedded PWA).
- Distributed as prebuilt binaries via `mise`/`ubi` (release.yml builds 4 targets) and a multi-arch `ghcr.io/poindexter12/bauded` image.
- CI gates on `cargo fmt --check` + `clippy -D warnings` + tests — all three must pass before push.
- The active workspace binds a backend and keeps Claude Code and OpenCode session pools, commands, state files, and daemon ports isolated.
- Worktree creation/removal and dirty-state checks already exist in `baude-core/src/git.rs`; v2.0 changes the product model from a flat session list to a persistent repository hierarchy.

## Constraints

- **Tech stack**: Rust (ratatui TUI, axum/tokio daemon, portable-pty + vt100); vanilla JS/CSS PWA embedded in the binary with no build step — keep it that way.
- **Security**: VPN/Tailscale-only; no auth layer is added. New endpoints inherit this model.
- **Compatibility**: backend-specific integrations must tolerate upstream schema drift; pin verified Claude Code and OpenCode versions in comments where wire assumptions are made.
- **Safety**: managed sessions run `--dangerously-skip-permissions` for unattended work; any permission-prompting mode is opt-in and must not become the unattended default.
- **No regressions**: stable sidebar order and the dual-source (session-file + silence-fallback) waiting logic are hard-won; changes must preserve current behavior as a labeled fallback.

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| GSD-track baude starting at v0.7 (lean scaffold, no full re-interview) | Codebase is mature and well-understood; a full new-project interview would re-derive known facts | ✓ Good — v0.7 shipped 4 phases through the full GSD chain |
| Prefer first-party Claude data (status-line JSON, hooks) over inference | Accuracy + unlocks PR/effort/activity data for free | ✓ Good — hooks now drive state; silence is a labeled fallback (v0.7) |
| Local hook transport via per-session event files; HTTP only in the daemon | Matches existing `meta.rs`/bridge file-tail patterns; avoids a new bind for the TUI | ✓ Good — one event model serves file-tail + daemon POST (v0.7) |
| Permission-prompt mode is opt-in; `skip` stays default | Unattended overnight runs must not block on phone approval | ✓ Good — fail-safe default-stays-skip + deny-on-timeout, security-reviewed (v0.7) |
| `--permission-prompt-tool` requires a stdio MCP server (not a plain command) | Pinned by v0.7 research; baude hand-rolls a 3-method JSON-RPC server in both binaries, no new deps | ⚠️ Revisit — wire contract is MEDIUM-confidence (claude-code #1175); confirm against live claude 2.1.178 before public ship |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-08-30 after starting v2.0 Repository Worktree Management*
