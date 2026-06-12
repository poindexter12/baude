# baude

A TUI for running multiple Claude Code sessions in one terminal.

Start it from any git repo and it spawns a `claude` session there. Add more
repos, or spin up isolated git-worktree sessions for parallel work in the same
repo. A shell pane at the session's folder is one keystroke away. The sidebar
sorts sessions by **who is waiting for your input** — longest-waiting on top,
with a wait timer — so you always know which session needs you next.

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
cargo install --path .
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

Two chords total; everything else passes straight through to Claude.

| Key | Where | Action |
|-----|-------|--------|
| `ctrl+q` | anywhere | step out to the sidebar |
| `ctrl+\` | attached | toggle shell pane (opening focuses it) |
| `enter` | sidebar | attach to selected session |
| `j/k` `↑/↓` | sidebar | select session |
| `t` | sidebar | toggle shell pane |
| `i` | sidebar | session info — model, tokens, context, permission mode |
| `g` | sidebar | GSD project state (`.planning/STATE.md`) |
| `n` | sidebar | new session (enter a repo path) |
| `w` | sidebar | new worktree session for selected repo |
| `r` | sidebar | restart an exited claude |
| `x` | sidebar | close session (worktree sessions ask keep/remove) |
| `?` | sidebar | help |
| `q` | sidebar | quit |

## Status icons (sidebar sort order)

- `●` waiting for your input — sorted to the top, longest wait first
- `◐` working
- `✗` exited (`r` to restart)

Waiting is detected from PTY output silence: Claude streams spinner output
continuously while working, so ~2s of quiet means it's your turn.

## Worktrees

`w` creates a worktree under `~/.local/share/baude/worktrees/<repo>/<branch>`
(new branch, or checks out an existing one) and starts a claude session in it.
Closing a worktree session asks whether to keep or remove the worktree;
worktrees with uncommitted changes are never removed.

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
{ "claude_cmd": "claude --dangerously-skip-permissions" }
```

- `claude_cmd` — command to run per session, default `claude`.
- `BAUDE_CLAUDE_CMD` env var overrides the config file.
