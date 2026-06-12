# baude

A TUI for running multiple Claude Code sessions in one terminal.

Start it from any git repo and it spawns a `claude` session there. Add more
repos, or spin up isolated git-worktree sessions for parallel work in the same
repo. A shell pane at the session's folder is one keystroke away. Sessions
hold a stable order in the sidebar — when one is **waiting for your input** it
flashes in place (with a wait timer) instead of jumping around, so the list
stays where your eye expects it while still telling you who needs you next.

Each session also surfaces live Claude Code metadata — model, context usage,
permission mode, token counts, and GSD project state — read from the artifacts
Claude writes to disk.

```
╭ baude ──────────────────╮╭ api · waiting · bypass ───────────────╮
│▸ ● api              4m  ││ > claude is waiting for your answer...│
│  fable-5 63% bypass     ││                                       │
│  ● webapp           1m  │╰───────────────────────────────────────╯
│  fable-5 12% ask ph4.5  │╭ shell @ ~/code/api ───────────────────╮
│  ◐ infra                ││ ❯ git diff                            │
│  fable-5 81% bypass     ││                                       │
╰─────────────────────────╯╰───────────────────────────────────────╯
```

## Install

Via [mise](https://mise.jdx.dev) (pulls the prebuilt binary from GitHub releases):

```sh
mise use -g ubi:poindexter12/baude
```

Or from source:

```sh
cargo install --path baude
```

## Usage

```sh
cd ~/code/some-repo
baude            # or: baude /path/to/repo
```

Sessions, worktrees, and shell-pane state persist across restarts
(`~/.config/baude/state.json`). On relaunch each session resumes its most
recent conversation via `claude --continue`.

## Keys

A few global chords work the same everywhere; everything else passes straight
through to Claude.

| Key | Where | Action |
|-----|-------|--------|
| `ctrl+q` | anywhere | step out to the sidebar |
| `ctrl+\` | anywhere | toggle shell pane (opening focuses it) |
| `alt+←/→` | anywhere | cycle to the prev/next session (wraps) |
| `enter` | sidebar | attach to selected session |
| `j/k` `↑/↓` | sidebar | select session |
| `t` | sidebar | open shell pane (focuses it) |
| `e` | sidebar | open the session folder in your editor (`editor_cmd`, default `code`) |
| `i` | sidebar | session info — model, tokens, context, permission mode |
| `g` | sidebar | GSD project state (`.planning/STATE.md`) |
| `n` | sidebar | new session (enter a repo path; `tab` completes directories) |
| `w` | sidebar | new worktree session for selected repo |
| `r` | sidebar | restart an exited claude |
| `x` | sidebar | close session (worktree sessions ask keep/remove) |
| `?` | sidebar | help |
| `q` | sidebar | quit |

`alt+←/→` needs your terminal to send Option/Alt as a modifier — on macOS
Terminal and iTerm2 enable this with "Use Option as Meta key". While attached,
this chord shadows Claude's own alt+←/→ word navigation.

## Status icons

- `●` waiting for your input — flashes in place, with a wait timer
- `◐` working — animated spinner
- `✗` exited (`r` to restart)

Waiting is detected from PTY output silence: Claude streams spinner output
continuously while working, so ~2s of quiet means it's your turn.

## Worktrees

`w` creates a worktree under `~/.local/share/baude/worktrees/<repo>/<branch>`
(new branch, or checks out an existing one) and starts a claude session in it.
Closing a worktree session asks whether to keep or remove the worktree;
worktrees with uncommitted changes are never removed.

## Usage panel

The bottom of the sidebar shows what you're consuming, and the status bar
shows when the limits refill:

```
│ ──────────────────────│
│ sess            $1.23 │   selected session cost (live)
│ today          $63.58 │   all Claude usage today      (ccusage)
│ week          $104.05 │   all Claude usage this week  (ccusage)
│ 5h ▓▓▓▓▓░░░░░ 47%     │   5-hour block — real account rate limit
│ wk ▓▓▓░░░░░░░ 32%     │   weekly window — real account rate limit
╰───────────────────────╯
 hints │ ~/code/api ⎇ main      ● 2 waiting · 5h resets in 46m · wk in 10d
```

- **today/week cost** — from [`ccusage`](https://ccusage.com), polled on a
  background thread every minute. Shows `—` if ccusage isn't installed.
- **session cost + rate-limit %** — Claude Code only exposes these in the
  JSON it pipes to statusLine commands, so baude ships a bridge:
  `baude statusline` captures the payload to `/tmp/baude-usage-<sid>.json`
  and delegates to your real statusline unchanged. Wire it in
  `$CLAUDE_CONFIG_DIR/settings.json`:

  ```json
  "statusLine": {
    "type": "command",
    "command": "baude statusline --wrap '<your existing statusline command>'"
  }
  ```

  No `--wrap` works too (bridge only, no rendered line). Rate-limit data is
  only sent for Pro/Max subscribers, and only after a session's first
  response — rows show `—` until then.

## Session metadata

The second sidebar line and the `i`/`g` overlays are populated from what
Claude Code writes to disk, refreshed every second:

- **busy/idle + model + permission mode + tokens** — Claude's own session
  file (`$CLAUDE_CONFIG_DIR/sessions/<pid>.json`) and the session transcript
  (`$CLAUDE_CONFIG_DIR/projects/<encoded-cwd>/<sessionId>.jsonl`). When the
  session file is present it replaces the output-silence heuristic for
  waiting detection.
- **context used %** — `/tmp/claude-ctx-<sessionId>.json`, a bridge file
  written by statusline hooks (e.g. the GSD statusline). Absent if no hook
  writes it.
- **GSD state** — `.planning/STATE.md` frontmatter in the session's repo.

`CLAUDE_CONFIG_DIR` is resolved from baude's environment (default
`~/.claude`), the same value the spawned claude processes inherit — so
profile setups with multiple config dirs just work if you launch baude from
the profile's shell.

## Configuration

`~/.config/baude/config.json`:

```json
{
  "claude_cmd": "claude --dangerously-skip-permissions",
  "new_session_dir": "~/Code/github.com",
  "editor_cmd": "code"
}
```

- `claude_cmd` — command to run per session, default `claude`.
  `BAUDE_CLAUDE_CMD` env var overrides the config file.
- `new_session_dir` — prefill for the `n` new-session prompt (tab-complete
  from there); defaults to the directory baude was launched from.
- `editor_cmd` — command the sidebar `e` key runs on a session's folder
  (the folder path is appended), default `code`. `BAUDE_EDITOR_CMD` env var
  overrides the config file.
- `daemon_url` — base URL of a remote bauded daemon (e.g.
  `http://bauded:8642`); its sessions appear in the sidebar under a
  `⇄ remote` section. `BAUDE_DAEMON_URL` env var overrides the config file.

## Remote sessions in the TUI

With `daemon_url` set, the daemon's sessions list in the sidebar below your
local ones — same status dots, waiting timers, and metadata. `enter`
attaches: the pane becomes a live raw terminal on the remote session (full
keystroke passthrough over a websocket, resizes follow your pane), so a
session running on your server is indistinguishable from a local one while
attached. `x` kills and `r` restarts remote sessions through the API; shell
panes, worktrees, and the editor key stay local-only.

## bauded (experimental)

A headless daemon (`cargo run -p bauded`) that owns sessions the same way the
TUI does but exposes them over REST + SSE, so thin clients can triage and
chat remotely. Sessions keep running when clients disconnect; daemon restarts
restore them via `claude --continue`. Binds `127.0.0.1:8642` by default
(`--bind` / `BAUDED_BIND`); security model is "bind the VPN interface" — no
auth layer. See `docs/remote-daemon-plan.md`.

The daemon serves a phone-first PWA at `/`: a triage list of sessions (who's
waiting and for how long, model, context %, cost, branch), a chat view with
live updates over SSE, message posting, queued-message bubbles, a terminal
peek drawer for the rare interactive menu, interrupt, and session
create/kill. Open it from any tailnet device and add it to your home screen —
it's installable (manifest + service worker), with no build step and no
external assets.

<p>
  <img src="docs/img/pwa-list.png" width="320" alt="session triage list">
  <img src="docs/img/pwa-chat.png" width="320" alt="chat view">
</p>

| Endpoint | What |
|----------|------|
| `GET /sessions` | session list: status, waiting-for, model, context %, branch, cost |
| `POST /sessions` | `{repo, worktree?, name?}` — spawn (worktree = branch name) |
| `DELETE /sessions/{id}` | kill and remove |
| `GET /sessions/{id}/messages?after=<uuid>` | transcript as chat messages |
| `POST /sessions/{id}/messages` | `{text}` — send a message (queues if busy) |
| `POST /sessions/{id}/interrupt` | Esc — stop current work |
| `POST /sessions/{id}/restart` | respawn claude in an exited session (`--continue`) |
| `GET /sessions/{id}/queue` | messages typed while busy, not yet picked up |
| `GET /sessions/{id}/screen` | plain-text terminal snapshot (menu escape hatch) |
| `POST /sessions/{id}/keys` | `{keys}` — named keys or literal text into the PTY |
| `GET /sessions/{id}/stream` | SSE live tail of new messages |

### Deploy (compose + Tailscale)

The provided `compose.yaml` runs bauded behind a Tailscale sidecar — the
daemon is only reachable over your tailnet, nothing is published to the host:

```sh
cp .env.example .env          # set TS_AUTHKEY
docker compose up -d --build  # or set image: ghcr.io/poindexter12/bauded
docker compose exec -it bauded claude   # log in once; persists in a volume
docker compose exec bauded git clone <url> /repos/<name>
open http://bauded:8642/                # the PWA, from any tailnet device
```

**Full guide — auth-key choices, key-expiry trap, HTTPS for installing the
PWA, updates: [docs/deploy.md](docs/deploy.md).**

The container seeds `statusLine: baude statusline` into the claude config
volume on first run (never overwriting an existing settings.json), so session
cost, context %, and account rate limits flow into the API out of the box.

Sessions run as `claude --dangerously-skip-permissions` (set per-deploy via
`BAUDE_CLAUDE_CMD`) so permission prompts never block unattended work. For
unattended git pushes, uncomment the ssh volume in `compose.yaml` and put an
automation key + config in `./ssh/`.
