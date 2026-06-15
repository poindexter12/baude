# Tier 4 — TUI / PWA ergonomics (cheap, high daily payoff)

**Theme:** ergonomics · **Effort:** S–M · **Fit:** excellent · **Pull forward anytime.**

## Goal

Make a many-session baude pleasant to live in. As the session count grows, the
linear sidebar and fixed keymap strain. Add a **fuzzy switcher + command
palette**, **tags / grouping / sort / filter**, and **per-repo worktree
bootstrap**. None of these need new architecture — they ride existing structs
and persistence. Independent of Tiers 1–3, so any of these can be pulled
forward when you want a quick high-value win.

## Why it's worth doing despite being "just UX"

- It's the universal power-user pattern (sesh, zellij, k9s) and the biggest
  daily-use payoff for a tool whose whole job is juggling many sessions.
- Everything here is cheap: the session struct and `state.json` already persist
  per-session data; adding a `tags` field and a filter predicate is small.

## Current state in the code

- **Sidebar** (`baude/src/ui.rs`): sessions held in stable creation order
  (`App::ordered_ids`), never reordered; sections are active / `⇄ remote` /
  archived. No search, filter, sort, or grouping.
- **Keys** (`baude/src/app.rs::handle_key`, encoding in `keys.rs`): fixed set
  — `j/k enter t e i g n w r a x ? q` + global chords. No fuzzy entry point, no
  command palette.
- **Session struct** (`session.rs`): `id, name, cwd, repo_root, branch,
  is_worktree, status, shell_open, archived, archived_by_user,
  unarchived_at_ms, spawn_unix_ms`. No tags / labels / last-accessed /
  project grouping. Persisted via `persist.rs` to `state.json` /
  `daemon-state.json`.
- **Worktree create** (`git.rs::create_worktree`): makes the worktree and
  starts claude — no setup hook, so untracked files (`.env`, installed deps)
  don't come across.

## Work breakdown

### 4a. Fuzzy switcher + command palette (Effort M)

- A new modal (suggest `/` opens *filter*, `:` opens *command*) — the only
  genuinely new UI primitive. Reuse the existing `Modal::Input` plumbing for
  text entry + a results list.
- **Filter mode** (`/`): fuzzy match across `name + repo + branch + title`,
  live-narrows the sidebar selection.
- **Command mode** (`:`): typed actions over the matched session —
  `:new <repo>`, `:worktree <branch>`, `:archive`, `:kill`, `:switch <q>`,
  `:tag <name>`. Maps to the same handlers `handle_key` already calls.
- A small fuzzy scorer (subsequence + frecency tiebreak from 4c) in
  `baude-core` so the daemon/PWA can reuse it.
- **PWA**: a search box atop the triage list doing the same name/repo/branch
  filter (pure client-side over the `/sessions` list).

### 4b. Tags / grouping / sort / filter (Effort S–M)

- Add `tags: Vec<String>` (and optional `project: Option<String>`,
  auto-derivable from git remote or `.planning/PROJECT.md`) to the persisted
  session struct. Migration: absent = empty (serde default).
- `SessionInfo` carries them so the PWA sees them too.
- **Sidebar**: optional grouping mode — by repo / project / model / tag —
  toggled by a key (suggest `G`); collapsible group headers. Default stays the
  current stable-order flat list.
- **Sort**: within a group, by wait-time / cost / last-active / creation.
- **Filter**: combine with 4a's `/` (e.g. `/ tag:wip model:opus`).
- Tagging action: `t`… is taken (shell pane) — use the `:tag` command from 4a,
  or a dedicated key.

### 4c. Frecency + quick-switch (Effort S)

- Track `last_attached_unix_ms` per session (set on attach); persist it.
- Frecency-rank the 4a switcher and a "recents" view.
- `last`-style toggle between the two most-recent sessions (suggest a chord,
  e.g. `ctrl+^` or double-tap) — sesh's most-loved feature, tiny effort.
- Number-key jump (`1`–`9`) to the Nth session in the current view.

### 4d. Worktree bootstrap script (Effort S)

- After `create_worktree`, if the repo has a `.baude/setup.sh` (or a
  `bootstrap_cmd` in config), run it in the new worktree before/just-after
  starting claude — copies `.env`, installs deps, runs `direnv allow`, etc.
- Worktrees don't carry untracked files; everyone else (Conductor, Crystal,
  smug) solved this. Small, high-relief annoyance fix.
- Stream its output into the session's shell pane so failures are visible.

## API / data-contract changes

- `session.rs` / persisted state: +`tags`, +`project`, +`last_attached_unix_ms`
  (all serde-defaulted for back-compat).
- `SessionInfo`: +`tags`, +`project`, +`last_attached_ms`.
- New config: `bootstrap_cmd` (or convention `.baude/setup.sh`),
  optional default grouping/sort preference.

## Risks & open questions

- **Keymap pressure**: the single-letter space is filling up. The `:` command
  palette is partly the *answer* to this — push less-common actions into
  commands rather than new chords. Decide which existing keys (if any) move
  into the palette.
- **State migration**: new fields must default cleanly for existing
  `state.json` / `daemon-state.json` — use `#[serde(default)]`, never a
  required field.
- **Grouping vs stable order**: the stable-order sidebar is a deliberate,
  hard-won design (busy sessions used to thrash). Grouping must be an *opt-in
  mode*, not the default, and must preserve stable order within groups.
- **Bootstrap script safety**: running a repo-local `.baude/setup.sh`
  automatically is arbitrary code execution on worktree create — fine for your
  own repos, but gate it (config opt-in) and don't run it for repos you didn't
  add yourself.

## Definition of done

- `/` fuzzy-filters and `:` runs commands over sessions in the TUI; the PWA list
  has a matching search box.
- Sessions can be tagged and the sidebar can group/sort/filter by tag, repo,
  project, or model (opt-in, stable order preserved within groups).
- The switcher is frecency-ranked with a last-two quick-toggle.
- New worktrees run an opt-in bootstrap script so they're ready to work in.
