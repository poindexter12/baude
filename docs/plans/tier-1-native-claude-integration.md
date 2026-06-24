# Tier 1 — Native Claude integration (replace inference with first-party data)

**Theme:** observability · **Effort:** S–M · **Fit:** excellent · **Do first.**

## Goal

Stop *inferring* session state from PTY-output silence and incremental JSONL
parsing where Claude Code now emits the same data event-driven, and harvest the
fields it gives away for free (PR state, worktree, effort, thinking, model
switches, rate-limit `resets_at`). Land a precise working/waiting/done signal
and a live **tool-activity feed**, then complete the mobile story with **remote
permission approval from the PWA**.

## Why this first

- Cheapest wins on the board: most of it is "read what's already there."
- It is the *foundation* the other tiers read from — Tier 2's "PR status in the
  sidebar" and Tier 3's stuck-agent detection both consume the data this tier
  captures.
- Accuracy: the PTY-silence heuristic (~2s quiet = your turn) is the one place
  baude guesses. Hooks make it authoritative.

## Current state in the code

- **Statusline bridge** (`baude-core/src/bridge.rs`): `baude statusline --wrap`
  reads Claude's statusLine JSON from stdin and writes a *subset* to
  `/tmp/baude-usage-<sessionId>.json` — today only `cost_usd`,
  `context_used_pct`, `five_hour`, `seven_day` (each `{used_pct, resets_at}`).
  The full payload Claude pipes in is discarded after extracting those.
- **Waiting detection**: dual-source — Claude's `sessions/<pid>.json`
  (authoritative busy/idle) when present, PTY-output silence as fallback
  (`meta.rs`). No tool-level granularity.
- **Notifications** (`bauded/src/notify.rs`): pure state machine over
  `SessionInfo` snapshots, polled per tick; fires on `waiting ≥10s` (debounced,
  re-armed each busy turn) and on `exited`. No "waiting for *permission*"
  distinction — a permission prompt looks like any other wait.
- **No hooks** are installed into managed sessions today.

## Work breakdown

### 1a. Full status-line capture (Effort S)

Extend `bridge.rs` to persist the whole useful payload, not just four fields.
Add (tolerating snake/camel like `window()` already does):

| New field | Source in payload | Surfaces |
|-----------|-------------------|----------|
| `model` | `model.display_name` / `model.id` | already inferred from JSONL — make bridge authoritative |
| `effort` | `effort` / reasoning effort | new sidebar/info chip |
| `thinking` | `thinking` mode flag | info overlay |
| `pr` | `pr` object (number, state, CI, review) | **Tier 2** sidebar PR row |
| `worktree` | `worktree` path/branch | cross-check baude's own worktree tracking |
| `vim.mode` | `vim.mode` | low priority, info only |

- Bump the bridge JSON schema version (`schema: 2`) so readers can tell.
- `meta.rs` reader gains the new optional fields; `ClaudeMeta` grows them.
- Keep every field `Option` — Claude Code versions differ.

### 1b. Hook-driven status events (Effort M)

Install a small hook set into every managed session and let the daemon/TUI
consume them instead of (or alongside) the silence heuristic.

- **Hooks to register** (per the verified hook contract):
  `UserPromptSubmit` → "turn started", `Stop` → "turn done / waiting",
  `Notification` → "waiting for permission/input", `PostToolUse` → "ran tool X".
- **Transport**: hook command `curl`s a new local endpoint. The daemon binds a
  loopback control port; for TUI-local sessions the bridge can write a per-sid
  event file (`/tmp/baude-events-<sid>.jsonl`) the way the usage bridge already
  does — no network needed locally. Pick one transport for both; the file-tail
  approach matches existing `meta.rs` patterns and needs no new bind.
- **Injection**: write the hook config into the session's
  `CLAUDE_CONFIG_DIR/settings.json` on spawn (the container already seeds
  `statusLine` the same way — reuse that seeding path). Never clobber existing
  user hooks: merge into the array.
- **New event endpoint** (daemon): `POST /sessions/{id}/event {kind, tool?,
  ts}` — or the file-tail equivalent. Updates the session's status without the
  silence timer.

### 1c. Live tool-activity timeline (Effort M, builds on 1b)

- New per-session ring buffer of recent tool events in `manager.rs`
  (`Vec<ToolEvent>` capped at ~200).
- New endpoint `GET /sessions/{id}/activity` + an SSE channel (or fold into the
  existing `/stream`).
- **PWA**: a collapsible activity strip in the chat view ("editing src/foo.rs →
  running cargo test → ...").
- **TUI**: a new overlay (suggest key `v` for "activity/view") mirroring it.

### 1d. Remote permission approval on the PWA (Effort M)

Today you can *interrupt* from the phone but not *approve* a pending tool call.

- Sessions in the daemon currently run `--dangerously-skip-permissions` so
  prompts never block. Add an opt-in mode where a session runs with
  `--permission-prompt-tool` (or SDK `canUseTool`) so prompts route to baude.
- The `Notification` hook from 1b is the "waiting for permission" trigger →
  fire a **distinct push** ("api wants to run `rm -rf build/` — approve?").
- New endpoints: `GET /sessions/{id}/permission` (pending request, if any) and
  `POST /sessions/{id}/permission {decision: allow|deny, scope?}`.
- **PWA**: an approve/deny card in the chat view when a request is pending.
- Per-deploy config: `BAUDE_PERMISSION_MODE = skip | prompt` (default `skip`
  preserves today's unattended behaviour).

## API / data-contract changes

- `bridge.rs` JSON: +`model`, `effort`, `thinking`, `pr`, `worktree`, `schema`.
- `SessionInfo`: +`effort`, `thinking`, `pr` (struct), `waiting_reason`
  (`permission` | `input` | none).
- New routes: `POST /sessions/{id}/event`, `GET /sessions/{id}/activity`,
  `GET`+`POST /sessions/{id}/permission`.

## Risks & open questions

- **Hook schema drift** across Claude Code versions — same tolerance approach
  as `bridge.rs::window()`. Pin the verified version in a comment.
- **Settings.json merge safety** — must never overwrite a user's existing hooks
  or statusline. Reuse the "seed only if absent / merge arrays" rule.
- **Local transport**: file-tail (matches existing code) vs loopback HTTP
  (needs a bind for the TUI). *Recommendation: file-tail locally, HTTP only in
  the daemon where the port already exists.*
- **Permission mode + unattended**: prompting mode means a stuck session blocks
  until you approve from your phone — fine when you're watching, bad overnight.
  Keep `skip` the default; document the trade-off.

## Definition of done

- Bridge captures the full field set; `i` overlay shows effort/thinking/PR.
- A managed session's working/waiting/done state comes from hooks, with the
  silence heuristic as labeled fallback only.
- A tool-activity feed renders in PWA and TUI.
- From the phone, a pending permission request can be approved or denied, and
  it triggers its own push distinct from the generic "waiting" one.
