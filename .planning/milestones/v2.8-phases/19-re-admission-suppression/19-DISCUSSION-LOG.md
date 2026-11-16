# Phase 19: Re-admission Suppression - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-11-03
**Phase:** 19-re-admission-suppression
**Mode:** `--auto`; every question below was resolved by Claude to the recommended default with no user prompt.
**Areas discussed:** Binding clear scope, Marker clear trigger and safety, Inventory sweep suppression, Fresh-row identity, Stage order and failure handling, Relaunch verification

---

## Binding clear scope

| Option | Description | Selected |
|--------|-------------|----------|
| Exact key match per removed path | Delete `folder-workspaces.json` entries whose key equals a removed row's canonical path; never match by workspace value | ✓ |
| Clear every entry bound to the workspace | Simpler, but wipes other repositories sharing the workspace, breaking READM-05 | |
| Prefix match on the removed folder | Would also drop subfolder entries, but none are recorded because `record` keys by `repo_root` | |

**Choice:** Exact key match. **Notes:** Standalone non-git folders have no binding today; the clear is a no-op and the test asserts the file is unchanged.

---

## Marker clear trigger and safety

| Option | Description | Selected |
|--------|-------------|----------|
| Clear only when the repository row leaves state; verify owner before delete; non-recursive dir removal | Marker deleted only if `read_marker` is `Valid` and names the removed common dir; empty dir removed with `remove_dir`; non-empty left and reported | ✓ |
| Clear on any child removal | Would strip ownership while surviving rows still use the directory | |
| Recursive delete of the repository directory | Fast cleanup, but violates the managed-root containment rules and the Phase 8 scan's refusal to guess | |

**Choice:** First option. **Notes:** Use `SavedRepository::physical_key` for the directory name so suffixed collision directories are handled; `.<name>.baude-removal` sidecars are Phase 18's and not touched here.

---

## Inventory sweep suppression

| Option | Description | Selected |
|--------|-------------|----------|
| Per-workspace advisory suppression file consulted by `admit_existing_worktrees`; entries only for checkout rows removed while the parent stays; lifted by explicit admission of that path | New file beside breadcrumbs, same read-merge-write posture; state schema untouched | ✓ |
| Run the inventory sweep only on first admission of a repository | No new file, but externally created worktrees would stop appearing on relaunch, a regression in existing behaviour | |
| Record removed paths in `state-<workspace>.json` | Schema-locked file with `deny_unknown_fields`; a one-way change | |
| Do nothing; accept re-admission of unmanaged children on relaunch inside the parent | Violates READM-03 as written | |

**Choice:** First option. **Notes:** Repository-parent and standalone removals write no entries because nothing sweeps those back and re-admission inside the folder should be a full fresh admission.

---

## Fresh-row identity

| Option | Description | Selected |
|--------|-------------|----------|
| New keys and `first_seen_order` from counters, no `resume_id`, no runtime, origin re-resolved; rely on `Breadcrumbs::flush` prune for crumbs | Matches what the existing admission already does; one test proves the crumb is gone | ✓ |
| Eagerly clear breadcrumb crumbs on removal | More work now; deferred unless Phase 21's daemon path needs it | |

**Choice:** First option.

---

## Stage order and failure handling

| Option | Description | Selected |
|--------|-------------|----------|
| Fourth best-effort stage after `persist_stage`, outcomes in `RemovalReport`, preview names what will be cleared | State file stays the source of truth; advisory I/O failure never fails a committed removal | ✓ |
| Clear before the save and fail the removal on error | Turns an advisory-file hiccup into a failed removal with runtimes already dead | |
| Separate operation the caller runs after `remove` | Splits the contract Phase 20 and 21 call into two steps | |

**Choice:** First option.

---

## Relaunch verification

| Option | Description | Selected |
|--------|-------------|----------|
| Second `App` over the same redirected config dir and state, `restore()`, assert absence and silent reconcile; cloned and `git init` fixtures | Models a real relaunch per the fixture-realism rule | ✓ |
| Assert only on the state file after removal | Does not exercise the sweep that re-admits | |

**Choice:** First option.

## Claude's Discretion

- Names of the suppression file, structs and report fields.
- One clearing function or three.
- Whether suppression entries are pruned when the path disappears from disk.
- Exact placement of the lift call before `admit_existing_worktrees`.

## Deferred Ideas

- Daemon wiring of the suppression consult helper: Phase 21.
- Confirmation display of cleared items: Phase 20.
- Eager breadcrumb clearing if Phase 21 needs it.
- `scripts/gate.sh --linux` bind-mount fix from Phase 18 deferred-items.
- Unpackaged opencode spike manifest, unrelated to this phase.
