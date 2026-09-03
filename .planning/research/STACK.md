# Stack Research

**Domain:** Repository/worktree management in the existing Rust ratatui application
**Researched:** 2026-08-30
**Confidence:** HIGH

## Recommendation

**Add no crates and change no framework.** The v2.0 features are a data-model and Git-orchestration change, not a stack change. Keep Git itself as the authority for repository identity, branch validity, worktree inventory, dirty state, and removal safety; keep Serde JSON as the workspace-scoped persistence format; render the hierarchy with existing ratatui primitives.

Do not upgrade dependencies as part of this milestone. The versions below are the resolved versions already in `Cargo.lock`, not proposed upgrade work.

## Recommended Stack

### Core Technologies

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| Rust workspace + standard library | Edition 2021; toolchain policy unchanged | Repository model, process execution, paths, background clone work, atomic file replacement | Existing boundaries already put Git and persistence in `baude-core` and UI state in `baude`; `std::process`, `std::path`, and `std::fs` cover all new requirements. |
| System Git CLI | Existing runtime prerequisite; verified locally with 2.50.1; current official docs cover 2.55.0 | Clone, identify repositories/default branches, validate branch names, enumerate/add/remove worktrees, and detect dirty trees | Git's porcelain commands preserve native config, credentials, worktree metadata, ref formats, and safety checks. This is less risky than introducing a second Git implementation. |
| `serde` + `serde_json` | 1.0.228 + 1.0.150 (locked) | Versioned, backward-compatible repository hierarchy persistence | Already used in `baude-core/src/persist.rs`; `#[serde(default)]` supports additive migration from the flat v0.14 state. |
| `ratatui` | 0.30.2 (locked) | Nested repository/worktree rows, selection, modals, and context-sensitive shortcut hints | The hierarchy needs only row flattening, indentation, and styling. Existing `Line`, `Span`, `Paragraph`, and modal code are sufficient; no tree-widget crate is needed. |

### Supporting Libraries

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `anyhow` | 1.0.102 (locked) | Propagate Git, filesystem, migration, and spawn failures with context | Keep at the `baude-core` command boundary and TUI action boundary. Preserve command stderr in errors. |
| `dirs` | 5.0.1 (locked) | Resolve existing XDG config/data roots | Continue using it for state and managed-worktree roots; do not introduce another platform-directory crate in this milestone. |

### Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| Temporary real Git repositories in tests | Validate branch names, worktree inventory, dirty removal, collisions, and migration end-to-end | Exercise the system `git` executable; do not mock Git's worktree semantics. Include spaces and non-UTF-8 paths where supported, duplicate repo basenames, slash-containing branches, detached HEAD, missing paths, and dirty/untracked files. |
| Existing CI gates | Prevent regressions | Keep `cargo fmt --check`, `cargo clippy -- -D warnings`, and workspace tests. Add no separate build system. |

## Integration Points and Minimal Changes

### `baude-core/src/git.rs`

Extend the existing command wrapper rather than adding a Git library.

- Add a byte-preserving command path for NUL-delimited machine output; the current `String::from_utf8_lossy(...).trim()` helper is not suitable for arbitrary worktree paths.
- Discover linked worktrees with `git worktree list --porcelain -z`. Git documents this as stable across versions and recommends combining porcelain with `-z`. Parse records into a small core type containing path, branch/detached state, lock state, and prunable state.
- Use `git rev-parse --git-common-dir` (with an absolute/canonical form) as repository identity when reconciling a path opened from either the main tree or a linked tree. Do not inspect `.git` internals directly.
- Use `git symbolic-ref --quiet --short HEAD` for the checked-out branch. A normal `git clone` already checks out the remote repository's active/default branch. For an existing checkout, resolve a configured remote `HEAD` when available, but do **not** silently switch a dirty main worktree merely to satisfy “default branch”; surface the ambiguity or use an already-registered default-branch worktree. This is a behavior decision, not a dependency gap.
- Validate user branch input with `git check-ref-format --branch` before creating a path. The current character replacement is not branch validation and aliases names such as `feature/a` and `feature-a`.
- Stop keying managed worktree directories only by repository basename. Use a persisted repository identity/directory key plus a collision-free child key. This needs a model change, not `uuid`, `sha`, or URL-slug crates.
- Choose “create branch” versus “attach existing branch” explicitly (for example, check `refs/heads/<branch>` first) instead of trying `-b` and treating every failure as “branch exists.” Preserve the first real error.
- Keep `git worktree remove` without `--force`. Git itself refuses unclean linked worktrees. Make the preflight dirty check return `Result<bool>` and fail closed on command error; the current `unwrap_or(false)` converts “could not inspect” into “clean.” Use porcelain output including untracked files. For background checks, Git recommends `--no-optional-locks` to avoid index-lock contention.
- Never remove directories with `std::fs::remove_dir_all`; Git must remove its linked-worktree administrative metadata.

### `baude-core/src/persist.rs`

- Evolve `State` to persist repository parents explicitly (stable ID/key, canonical main path, display name, default branch, and managed worktree metadata). Keep live PTY/session state separate from repository identity.
- Add `#[serde(default)]` to every newly added field and retain a migration path from `State.sessions`. An explicit small schema version is advisable because this changes shape rather than merely adding a scalar.
- Keep workspace-specific JSON files and the TUI/daemon file separation. A SQLite database adds no value for this single-writer, small-state model.
- Replace direct `std::fs::write` with serialize -> adjacent temporary file -> flush/sync as appropriate -> `std::fs::rename`. The standard library is enough; no `tempfile` or transactional-store dependency is required. Do not silently replace malformed state with an empty hierarchy without retaining/reporting the bad file.

### `baude/src/app.rs`

- Introduce a persistent repository collection distinct from `Vec<Session>`. A repository parent must remain visible even when its default-branch session is closed or exited; a `Session` remains the live backend/PTY child.
- Reuse `backend::active()`, `backend::command_for`, `spawn_plan`, and `prepare_cwd` through the existing `add_session` path for both default-branch and worktree sessions. No Claude/OpenCode SDK or backend-specific repository adapter is needed.
- Flatten repository parents and child sessions into a sidebar row enum for ordering and selection. Route shortcuts by selected row kind rather than adding another input framework.
- Keep clone and potentially slow Git inventory/status operations off the render loop using the existing thread/channel pattern. A Tokio runtime in the TUI is unnecessary.
- Reconcile persisted metadata with Git on restore: persisted state controls product membership/order; Git porcelain controls current path/branch/locked/prunable truth. Missing or externally removed worktrees should be shown/repaired or forgotten deliberately, not blindly respawned.

### `baude/src/ui.rs`

- Render parents and children using indentation/connectors and existing two-line session rows. Repository rows can have one compact line; child rows retain live status/meta behavior.
- Derive help/status hints from selection context so `w`, `x`, `e`, Enter, and session actions describe what they will do. No keybinding or tree-widget dependency is warranted.
- Preserve stable in-place ordering and waiting flashes. Group by persisted repository order first, then stable child order; do not globally status-sort the flattened tree.

### Daemon/remote boundary

The same core repository/worktree types and safety functions should be reused by daemon endpoints. Extend existing DTOs/endpoints only as required to carry parent identity and row kind. Do not create a second Git implementation or backend-specific hierarchy model in `bauded`.

## Installation

No package installation or `Cargo.toml` change is recommended.

```bash
# Intentionally empty: retain the existing workspace dependencies and system git CLI.
cargo test --workspace
```

## Alternatives Considered

| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|-------------------------|
| System Git CLI | `git2`/libgit2 | Only if baude later must operate without a Git executable or needs high-volume in-process object traversal. Neither applies to v2.0, and worktree/config behavior parity would become baude's burden. |
| Serde JSON state | SQLite (`rusqlite`/SQLx) | Only if multiple writers need transactions/queries over large histories. Current workspace-scoped state is small and process-separated. |
| Existing ratatui primitives | A third-party tree widget | Only if future requirements include large-tree virtualization, arbitrary expansion, or mouse drag/drop. One repository/child level does not justify it. |
| Existing thread/channel pattern | Tokio in the TUI | Only if the local TUI is redesigned around many concurrent asynchronous services. A few blocking Git subprocesses do not justify a second execution model. |

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| `git2`, libgit2 bindings, or shelling through `sh -c` | Adds native build/runtime complexity or quoting hazards and can diverge from the user's installed Git behavior/configuration | `std::process::Command` with one argument per Git token and `--` before user-controlled positional values |
| `--force` on `git worktree remove` | Defeats the milestone's no-data-loss invariant | Result-returning dirty preflight plus Git's default refusal to remove unclean trees |
| Manual `.git/worktrees` parsing or recursive deletion | Git supports gitfiles, common dirs, alternate layouts, lock/prunable state, and administrative repair rules | `rev-parse` and `worktree ... --porcelain`/`remove` |
| Repository basename as identity | Two owners/hosts can have the same repository name; current managed paths can collide | Canonical Git common-directory identity plus persisted stable key |
| Branch-name sanitization as validation/identity | Different valid refs collapse to the same directory name, and invalid refs reach a confusing two-attempt command path | `git check-ref-format --branch` plus an independent collision-free storage key |
| New database, UUID, hashing, watcher, tree-widget, or async-runtime crates | None is required by the target behavior; each increases migration and cross-platform surface | Existing Serde JSON, stable persisted IDs/keys, command-time reconciliation, ratatui lines, and threads/channels |
| Dependency upgrades bundled with v2.0 | Expands regression scope without enabling a target feature | Keep the lockfile stable; upgrade in a separate maintenance change |

## Stack Patterns by Variant

**When a repository was just cloned:**
- Treat clone's checked-out `HEAD` as the default-branch session; official Git behavior already forks/checks out the source repository's active branch.
- Register the repository parent only after clone success, then spawn through the active backend.

**When opening an existing repository whose main worktree is not on the remote default:**
- Do not auto-switch if that could disturb user state.
- Reconcile remote `HEAD`, existing linked worktrees, dirty state, and branch occupancy; then prompt/surface the condition or attach an existing safe checkout. No stack addition resolves this policy safely.

**When removing a managed worktree:**
- Stop/close the live session, check dirty state with an error-aware result, and invoke non-force `git worktree remove` only when clean.
- Persist removal only after Git succeeds; on failure keep the child metadata visible/recoverable.

## Version Compatibility

| Package/Tool | Compatible With | Notes |
|--------------|-----------------|-------|
| Git CLI (verified 2.50.1 locally) | Current official Git 2.55 documentation | Required commands are standard Git porcelain. Before declaring a minimum supported Git version, run integration tests against the project's oldest supported Git; no new minimum is necessary for planning this milestone. |
| `serde` 1.0.228 / `serde_json` 1.0.150 | Existing v0.14 state JSON | New fields require `#[serde(default)]`; shape migration from flat sessions should be explicit and covered by fixture tests. |
| `ratatui` 0.30.2 | Existing `baude` UI | Hierarchy rendering uses APIs already present in `ui.rs`; no feature flags or companion crate required. |
| Rust standard library | Existing Edition 2021 workspace | Adjacent-file rename is available, but replacement semantics are platform-specific; keep temp and destination on the same filesystem and test supported release targets. |

## Sources

- [Git `worktree` documentation](https://git-scm.com/docs/git-worktree) — authoritative command semantics, stable `--porcelain -z`, clean-only removal, lock/prunable state, common-dir rules, and warning about submodule limitations. **HIGH confidence.**
- [Git `status` documentation](https://git-scm.com/docs/git-status) — authoritative stable porcelain output, untracked-file behavior, `-z`, and `--no-optional-locks` guidance for background status. **HIGH confidence.**
- [Git `clone` documentation](https://git-scm.com/docs/git-clone) — authoritative statement that normal clone creates and checks out an initial branch from the source's active branch. **HIGH confidence.**
- [Git `symbolic-ref` documentation](https://git-scm.com/docs/git-symbolic-ref) — authoritative branch/HEAD lookup and detached-HEAD exit behavior. **HIGH confidence.**
- [Git `rev-parse` documentation](https://git-scm.com/docs/git-rev-parse) — authoritative top-level/common-dir and path-format discovery; avoids `.git` layout assumptions. **HIGH confidence.**
- [Git `check-ref-format` documentation](https://git-scm.com/docs/git-check-ref-format) — authoritative `--branch` validation rules. **HIGH confidence.**
- [Serde field attributes](https://serde.rs/field-attrs.html) — authoritative `#[serde(default)]` and alias behavior for compatible state evolution. **HIGH confidence.**
- [Rust `std::fs::rename` 1.98.0 documentation](https://doc.rust-lang.org/std/fs/fn.rename.html) — authoritative same-filesystem and platform-specific replacement semantics. **HIGH confidence.**
- Repository `Cargo.toml`, `Cargo.lock`, and requested source files — exact existing dependency versions and integration boundaries. **HIGH confidence.**

---
*Stack research for: baude v2.0 repository worktree management*
*Researched: 2026-08-30*
