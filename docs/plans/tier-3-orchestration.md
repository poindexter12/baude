# Tier 3 — Orchestration (highest conceptual ceiling)

**Theme:** orchestration · **Effort:** M (each) · **Fit:** good · **After Tiers 1–2.**

## Goal

Move baude from "create a session" to "run a backlog / run a fleet." Three
related capabilities: **race N sessions on one prompt and pick a winner**, a
**prompt/task queue** (Dispatcher), and **scheduled / event-triggered session
launch**. The daemon already runs sessions unattended and survives detach — it
just never *launches* on its own (it only archives).

## Why this is the ceiling

- baude already spins parallel worktree sessions; race-N is a thin layer on top
  and is the single most-cited multi-agent feature across the landscape.
- The daemon is a long-lived process that owns sessions — the natural host for
  a queue and a scheduler. No new infrastructure.
- This is where baude stops being a manager of *your* typing and starts doing
  work while you're away — but bounded, single-user, and VPN-local (no
  cloud-autonomy arms race).

## Current state in the code

- `POST /sessions {repo, worktree?, name?}` spawns one session; worktree
  creation already lives in `git.rs::create_worktree`.
- Messages queue *natively* inside Claude Code when a session is busy
  (`queue-operation` records, surfaced via `GET /sessions/{id}/queue`). There
  is no *baude-level* queue across sessions or one that fans out.
- `manager.rs` polls sessions on a tick and runs the auto-archive logic — the
  obvious home for a scheduler tick.
- Nothing launches a session except an explicit user/API create.

## Work breakdown

### 3a. Race-N-and-pick-winner (Effort M)

The flagship. Run one prompt across N parallel worktree sessions, compare, keep
one.

- **Launch**: `POST /races {repo, prompt, n, variation?}` →
  creates N worktree sessions on generated branches
  (`race/<slug>/<1..n>`), sends the prompt to each. `variation` can vary
  model / effort / thinking per arm (even Claude-only this is useful).
- **Group model**: a `Race` aggregate in `manager.rs` tying the N session ids
  together; sessions gain an optional `race_id` (also useful for Tier 4
  grouping). Surface as a grouped block in the sidebar/PWA.
- **Compare**: reuse Tier 2's diff endpoints — show the N diffs side by side
  (PWA) or switchable (TUI).
- **Pick**: `POST /races/{id}/pick {session_id}` → keep that worktree, offer to
  remove the losers (respecting `is_dirty` — never auto-remove dirty trees,
  same rule as today's worktree close).
- **Depends on Tier 2** for the comparison surface; without it you'd be
  eyeballing terminals.

### 3b. Prompt / task queue — "Dispatcher" (Effort M)

- A daemon-owned queue of prompts, each targeting either an existing session or
  a *spec* for a new one (`{repo, worktree?}`).
- `POST /queue {target | spec, prompt}`, `GET /queue`, `DELETE /queue/{id}`,
  reorder.
- Drains FIFO: dispatch next item when its target session goes idle (the
  manager already knows idle/busy per tick). For new-session specs, spawn then
  send.
- Distinct from Claude's *native* in-session queue (which we already surface) —
  this is *cross-session* and *baude-owned*.
- Persist the queue in `daemon-state.json` so it survives restart.

### 3c. Scheduled / event-triggered launch (Effort M)

- **Cron**: config-defined jobs (`{schedule, repo, worktree?, prompt}`) — e.g.
  nightly dep bump, weekly changelog draft. A scheduler tick in `manager.rs`
  (it already ticks for archive) checks due jobs and enqueues them via 3b.
- **Webhook**: a new bound endpoint `POST /hooks/github` (or generic) that, on
  a matching event (issue labeled `claude`, PR comment `@baude fix`), launches
  a session with a templated prompt. Stays VPN-local unless you choose to
  expose it via `tailscale serve`.
- Reuses Tier 1's stuck-agent signal so an unattended scheduled run that hangs
  notifies you instead of silently sitting.
- **Safety gate** (zellij-style): scheduled/webhook launches should default to
  *prepared but not auto-fired* unless explicitly marked autonomous — an agent
  isn't a shell; don't auto-run destructive prompts overnight without opt-in.

## API / data-contract changes

- New: `POST /races`, `GET /races`, `POST /races/{id}/pick`.
- New: `POST/GET/DELETE /queue`.
- New: `POST /hooks/...`, cron jobs in config / `daemon-state.json`.
- `SessionInfo`: +`race_id: Option<...>`, +`origin` (`manual` | `queue` |
  `cron` | `webhook`) for provenance display.

## Risks & open questions

- **Cost blow-up**: N parallel sessions + scheduled launches multiply spend.
  Surface aggregate cost prominently and consider a per-day budget cap
  (cheap given the existing cost panel) before shipping autonomous launch.
- **Worktree sprawl**: race arms create branches/worktrees; need cleanup of
  losers and stale race branches.
- **Unattended safety**: webhook/cron launching `--dangerously-skip-permissions`
  sessions is powerful and dangerous. The Tier 1 permission-prompt mode + the
  "prepared not fired" gate are the mitigations; decide the default posture
  explicitly per deploy.
- **Scope creep toward a CI system** — keep this single-user and bounded; it's
  a personal fleet, not a build farm.

## Definition of done

- One prompt can launch N worktree sessions as a named race, their diffs
  compared, a winner kept and losers cleaned up.
- A cross-session prompt queue drains as sessions free up and survives restart.
- A cron job and/or a GitHub webhook can launch a templated session, with a
  safety gate against unattended auto-fire.
