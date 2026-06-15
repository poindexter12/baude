# Tier 2 — The diff / review loop (close the biggest functional gap)

**Theme:** git / PR workflow · **Effort:** M–L · **Fit:** good · **Do after Tier 1.**

## Goal

Turn baude from a session *viewer* into a session *reviewer*. Today
`baude-core/src/git.rs` does worktree CRUD and a dirty check and nothing
else — no diff, commit, or PR surface anywhere. Every serious competitor
(Conductor, Cursor, cmux) treats diff review as the *primary* interaction. This
tier adds a structured diff view, turns inline comments into follow-up prompts,
and surfaces PR state in the sidebar.

## Why this matters

- It is baude's biggest *functional* gap, not just an ergonomic one. The whole
  point of parallel worktree sessions is producing changes you then review.
- It compounds the mobile story: reviewing a diff on your phone and replying
  "fix the error handling in this hunk" is the natural endpoint of remote
  triage.
- PR state is nearly free once Tier 1's bridge captures the `pr` object.

## Current state in the code

- `git.rs`: `repo_root()`, `create_worktree()`, `is_dirty()`,
  `remove_worktree()`. The `git()` helper shells out via `git -C <repo> <args>`
  — adding diff/status/log is a few more thin wrappers in the same shape.
- No diff is parsed or rendered anywhere. The closest thing is the PWA
  **terminal-peek** drawer (`GET /sessions/{id}/screen`), which is raw vt100
  text, not a structured diff.
- `transcript.rs` already parses `tool_use` blocks into compact summaries —
  Edit/Write tool calls carry the diff content but it's collapsed to a summary
  string today.

## Work breakdown

### 2a. Git read surface in `baude-core` (Effort S)

New thin wrappers next to the existing ones:

- `status(repo) -> Vec<FileStatus>` (porcelain v2 parse: path, staged/unstaged,
  added/modified/deleted/untracked).
- `diff(repo, opts) -> String` — unified diff; opts for staged vs working,
  vs a base ref (`origin/main`), single file.
- `current_branch(repo)`, `ahead_behind(repo, base)`, `log(repo, n)`.
- All read-only; no mutation in this sub-task.

### 2b. Diff endpoints + parser (Effort M)

- `GET /sessions/{id}/diff?base=&path=&staged=` → returns either raw unified
  diff or a parsed structure.
- A small unified-diff parser (hunks, line types, old/new line numbers) — lives
  in `baude-core` so both TUI and daemon use it. Unit-test against fixtures the
  way `transcript.rs` is tested.
- `GET /sessions/{id}/status` → `Vec<FileStatus>` for the file tree.

### 2c. Diff viewer — PWA (Effort M)

- New route `#/sessions/{id}/diff`: a file list (status icons) → tap a file →
  unified diff with syntax-ish coloring (added/removed/context). Vanilla
  JS/CSS, no build step, matching the existing PWA.
- Base selector: working changes vs `origin/<default>` vs last commit.
- Reachable from the chat view header and from the triage list.

### 2d. Diff viewer — TUI (Effort M)

- New overlay (suggest key `d` in the sidebar) showing the selected session's
  diff. Render unified diff with ratatui styled spans; `j/k` scroll, file
  switching with `[`/`]` or a file list pane.
- For worktree sessions this is the headline feature; for normal sessions it
  diffs the working tree.

### 2e. Inline comment → queued follow-up prompt (Effort M, the differentiator)

- In the PWA diff view, a comment box on a hunk/line. Submitting composes a
  prompt with file + line context ("In `src/foo.rs` around line 42 you wrote X
  — <comment>") and POSTs it to the existing
  `POST /sessions/{id}/messages` (which already queues if busy).
- Optionally batch multiple comments into one review message ("address these N
  comments") before sending.
- This is Conductor's standout feature; it reuses baude's entire existing
  message-injection path — the only new work is composing the contextual prompt.

### 2f. PR lifecycle in the sidebar (Effort M)

- **Display (near-free after Tier 1):** render the `pr` object from the bridge
  — number, state, CI status, review state — as a sidebar row / PWA badge.
- **Actions (uses `gh`):** open PR (push branch + `gh pr create`), draft a
  description (ask the session to write it), push fix commits. Shell out to
  `gh` the same way `git.rs` shells to `git`; gate behind config since not
  every deploy has `gh` + auth.

## API / data-contract changes

- `baude-core`: new `git::status/diff/current_branch/ahead_behind/log` + a
  `diff` parser module.
- New routes: `GET /sessions/{id}/diff`, `GET /sessions/{id}/status`,
  and (2f) `POST /sessions/{id}/pr` actions.
- `SessionInfo`: +`dirty: bool`, +`ahead`/`behind` counts (cheap, from 2a).

## Risks & open questions

- **Diff size**: large diffs over SSE/HTTP — paginate by file; don't ship a
  10k-line diff to a phone at once.
- **Binary / rename / mode-change** hunks — the parser must degrade gracefully
  (show "binary file changed", skip).
- **`gh` dependency** for 2f — keep PR *display* (read-only, from the bridge)
  separate from PR *actions* (needs `gh` + auth). The container already has an
  ssh automation-key path; document the `gh` token requirement.
- **Worktree vs repo**: a normal (non-worktree) session shares its working tree
  with your editor — diffing is fine, but committing from baude could surprise
  you. Keep commit/PR actions worktree-first.

## Definition of done

- A session's changed files and their unified diffs render in both PWA and TUI.
- A comment on a diff hunk in the PWA reaches the session as a queued prompt.
- PR number + CI/review state show in the sidebar for sessions that have a PR.
- (Stretch) open-PR / push-fix actions work where `gh` is configured.
