# Design: Repository root as a baude launch context

Status: **Proposed** (design only, not implemented)
Author: design pass, 2026-08-24

## Assumptions

- "Iterate on the folders" first means discover and navigate repositories, not
  immediately run the same prompt in every repository.
- Opening a live AI session remains an explicit user action.
- A common launch root is two levels above repositories, for example
  `~/Code/github.com/<owner>/<repo>`.

## Current behavior

`baude [<repo-dir>]` already accepts any directory. Startup only auto-opens the
launch directory when `git rev-parse --show-toplevel` succeeds
(`baude/src/app.rs:392`). A non-repo launch root therefore produces an empty
sidebar, and `n` prefills that root for manual path entry.

The important constraint is that baude has no passive repository model. Every
local sidebar row is a `Session`, and constructing one immediately spawns the AI
CLI in a PTY (`baude/src/app.rs:522`). Treating every discovered repository as a
session would unexpectedly start many processes, consume resources, and resume
or mutate state across unrelated repositories.

## Recommendation

Treat a non-repo launch directory as a **repository collection root**, not as a
session and not as a batch-execution target.

On `baude ~/Code/github.com`:

1. Restore persisted sessions normally.
2. Discover repositories below the launch root without opening them.
3. If the launch root is itself a repository, preserve today's auto-open
   behavior and do not scan beneath it.
4. If it is not a repository, make `n` open a filterable repository picker over
   the discovered roots.
5. `Enter` opens exactly one selected repository as a normal session.

This gives the root-directory workflow value without changing session
semantics or introducing accidental fan-out.

## Discovery rules

- Default maximum depth: 2 relative to the collection root.
- A `.git` directory **or file** identifies a repository root; the file form is
  required for worktrees.
- Stop descending after finding a repository so submodules and nested checkout
  internals are not added implicitly.
- Do not follow symlinks.
- Skip hidden directories and common generated trees such as `target`,
  `node_modules`, and `vendor` unless explicitly configured.
- Canonicalize and deduplicate results.
- Sort by relative path for stable picker order.
- Run discovery off the UI thread and expose scanning/failed state without
  blocking input.

The depth should be configurable (`repo_scan_depth`, where `0` disables
discovery), but the first implementation should avoid include/exclude glob
machinery until a concrete need appears.

## Minimal data model

Do not persist discovered repositories and do not add placeholder `Session`
objects. Keep an in-memory catalog on `App`:

```rust
struct RepoEntry {
    root: PathBuf,
    relative: PathBuf,
}
```

The catalog is derived state and can be rescanned each launch. Existing session
persistence remains unchanged. Repositories already represented by a local or
remote session can be marked as open or filtered from the picker.

## UI shape

- Empty non-repo launch: show `No sessions - press n to choose a repository
  under <root>` rather than implying baude failed to start.
- `n`: filterable list of discovered repositories, plus a final "enter another
  path or clone URL" route preserving current behavior.
- Optional refresh action inside the picker; no new global chord.
- The sidebar continues to show only active or archived sessions.

The picker naturally composes with the fuzzy-switcher work proposed in
`docs/plans/tier-4-ergonomics.md`; one reusable list/filter modal should serve
both rather than adding a one-off repository browser.

## What not to implement yet

Do not add "open all repositories" to the discovery feature. Opening means
spawning one AI CLI per repository, so even a modest root can create dozens of
processes. More importantly, discovering repositories and applying a task to
many repositories are different user intents.

If cross-repository work becomes concrete, design it as a separate **batch**
workflow with:

- explicit repository multi-selection;
- one prompt or operation shown before execution;
- a configurable concurrency limit, default 1;
- per-repository result and failure isolation;
- cancellation and a final summary;
- no implicit worktree creation, commits, or pushes.

That workflow may create normal sessions as workers, but should never be
triggered merely by launching baude from a parent directory.

## Implementation slices

### Slice 1: Collection-root startup

- Add repository discovery as a pure function in `baude-core/src/git.rs` with
  temp-directory tests for depth, `.git` files, nested repositories, ignored
  directories, and symlinks.
- Store the derived catalog and scan result in `App`.
- Improve the empty-state copy for non-repo launch roots.

### Slice 2: Repository picker

- Generalize the existing input modal into a filterable candidate list.
- Route `n` through discovered repositories while retaining arbitrary paths and
  clone URL input.
- Open only the selected repository through the existing
  `open_repo_session` path, preserving local/daemon behavior.

### Slice 3: Explicit collections, only if needed

- Add configured roots or an explicit repository manifest when scan depth and
  filtering prove insufficient.
- Keep this configuration outside individual repositories because the
  collection root may not itself be version-controlled.

## Decision

Proceed with collection-root discovery and a repository picker when this moves
to implementation. Defer batch iteration until there is a concrete operation
that needs to run across repositories; do not couple batch execution to startup.
