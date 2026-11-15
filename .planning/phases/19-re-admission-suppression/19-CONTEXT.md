# Phase 19: Re-admission Suppression - Context

**Gathered:** 2026-11-03
**Status:** Ready for planning
**Mode:** `--auto` (gray areas auto-resolved to the recommended default; rationale recorded per decision)

<domain>
## Phase Boundary

A removed row stays gone across reconcile and TUI relaunch. Phase 18's `removal::remove` already kills, deletes managed worktree directories and persists the removed set in one save; this phase adds the clearing that stops every rediscovery path from bringing the row back: the folder-workspace binding in `<config>/folder-workspaces.json`, baude's ownership marker in the managed repository directory, and the worktree inventory sweep that `App::admit_repository` runs on every launch inside a known repository. `n`, `c`, and launching baude inside the removed folder still admit the project, as a fresh row with new keys and no carried identity. Other rows, including other repositories in the same workspace, are untouched.

In scope: READM-01, READM-02, READM-03, READM-04, READM-05. Core changes in `baude-core` plus the TUI admission seam in `baude/src/app.rs`, with relaunch tests.

Out of scope: the TUI remove key and confirmation (Phase 20), the REST endpoint and PWA action and the daemon's own admission wiring (Phase 21), any deletion inside a user's folder (RMV-07 still holds), release notes (Phase 22).

</domain>

<decisions>
## Implementation Decisions

### Folder-workspace binding (READM-01, READM-05)
- **D-01:** Removal deletes `folder-workspaces.json` entries whose key equals a removed row's canonical path: the repository's `observed_main_worktree`, every removed checkout's `observed_path`, and a standalone row's `canonical_path`. Keys are matched exactly with `breadcrumbs::folder_key`; nothing is matched by workspace value, so other folders bound to the same workspace keep their entries. The write goes through the existing `locked_merge_write` read-merge-write path so a parallel TUI never loses its own entry.
- **D-02:** Launching baude inside the folder afterwards re-records the binding through the unchanged `launch::start_workspace` step 5. No special case: the fresh binding is the normal one.
- **D-03:** No binding is written for a non-git standalone folder today, because `plan_launch` only derives `repo_root` for git trees. Clearing is a no-op there and the test asserts the file is byte-identical after a standalone removal.

### Marker ownership (READM-02, READM-05)
- **D-04:** Marker ownership is cleared exactly when a repository row leaves the state. Removing a single child while the repository row remains never touches the parent's marker, because the surviving rows still own the directory.
- **D-05:** The directory is `git::worktrees_base()/<workspace>/repository-<physical_key>` using `SavedRepository::physical_key` from the removed row, never a recomputed digest, so a suffixed collision directory such as `repository-<digest>-2` is the one cleared. Legacy rows whose `physical_key` is a bare counter are resolved the same way, with the directory name taken from the field.
- **D-06:** The marker is deleted only when `marker::read_marker` returns `Valid` and its `canonical_common_dir` equals the removed repository's `observed_common_dir`. A `Missing` marker is a no-op, and a foreign, `Invalid`, or symlink marker is left untouched and reported, mirroring `write_marker`'s refusal to overwrite what it does not own. — **Reversibility:** costly — a marker deleted for the wrong owner makes the next admission of that other repository allocate a needless suffixed directory and orphans its worktrees, and the Phase 8 worktree scan refuses to clean that up by design.
- **D-07:** After the marker is gone the repository directory is removed with a non-recursive `remove_dir`. Phase 18's disk stage has already deleted the managed worktrees, so an empty directory is the expected case. A non-empty directory is left in place and named in the report; nothing recursive runs here, and nothing outside the managed root is touched. The Phase 18 `.<name>.baude-removal` sidecar is a different artefact; it is already removed on a successful forced removal and this phase does not add handling for it.
- **D-08:** Nothing is deleted inside a kept main checkout, unmanaged checkout or standalone folder. The only artefacts this phase clears live under baude's config dir and its managed worktree root.

### Inventory sweep suppression (READM-03, READM-04)
- **D-09:** `App::admit_repository` calls `admit_existing_worktrees` on every admission of an already-known repository, including the launch-dir admission in `App::restore`, so a removed unmanaged checkout whose directory survives is re-admitted from `git worktree list` on the next relaunch inside the parent. Bindings and markers do not cover this. Phase 19 therefore adds a per-workspace suppression store: an advisory JSON file beside the breadcrumbs file, keyed by canonical path, with the same posture as `folder_workspace` and `breadcrumbs` (additive schema with `schema_version`, `locked_merge_write`, corrupt file degrades to empty and never fails a launch). The schema-locked `state-<workspace>.json` is not changed. — **Reversibility:** costly — the file is a new on-disk format under the config dir that later versions must keep reading; removing the mechanism means a migration or a documented one-time cleanup.
- **D-10:** A suppression entry is written only for a checkout row removed while its repository row remains in state. Repository-parent removal and standalone removal write no entries: nothing sweeps a repository or a standalone back, and re-admission inside the folder is then an ordinary first admission that shows the full topology, which is what "fresh row" means. When a repository row is later removed, the entries of its children are dropped as garbage.
- **D-11:** `admit_existing_worktrees` consults the store and skips a suppressed path. The consult point is one helper in `baude-core` so the daemon's admission (Phase 21) reads the same file through the same code; wiring `bauded` is Phase 21's job.
- **D-12:** An entry is lifted by an explicit admission of that exact path: `n` with the checkout's path, `c` cloning into it, or launching baude with a launch dir whose `git::repo_root` is that path. Lifting happens before the sweep runs so the fresh row is admitted in that same pass. `n` with the parent repository's root does not lift a child's entry; that is the sweep the entry exists to suppress.
- **D-13:** A fresh row carries no identity from the removed one: new `RepositoryKey`, `CheckoutKey` or `StandaloneKey` and `first_seen_order` from the monotonic counters, `resume_id: None`, no `owned_runtime`, lifecycle starting state, origin re-resolved. The deterministic `physical_key` digest of the common dir is repository identity, not row identity, and may legitimately coincide. Breadcrumb crumbs are path-keyed advisory selection memory; `Breadcrumbs::flush` already drops crumbs whose path is no longer in durable state, and the TUI flushes after removal, so no eager crumb clearing is added. A test proves the removed path's crumb is gone after the flush.

### Stage order and failure handling
- **D-14:** Clearing runs as a fourth stage inside `removal::remove`, after `persist_stage` has committed the save. The state file is the source of truth, so an advisory-file or marker I/O failure must not turn a completed removal into an error. The stage is best-effort: outcomes land in `RemovalReport` as a list of cleared items and a list of clearing failures with path and error, and `preview_removal` names the bindings, markers and suppression entries the real run will clear so Phase 20's confirmation can show them. The crash window between the save and the clearing stage is accepted on the same terms as Phase 18's disk-to-save gap; a binding or marker left behind is residue, not a re-admission, and a missing suppression entry is repaired by removing the row again.
- **D-15:** Everything the stage touches is scoped to the removed set. The test for READM-05 builds two repositories bound to the same workspace plus a third repository with a managed worktree, removes one, and asserts the other bindings, markers, worktree directories and suppression entries are byte-identical.

### Relaunch verification (READM-03)
- **D-16:** "Across a relaunch" is tested by building a second `App` over the same redirected config dir and state file after removal and calling `restore()`, then asserting the removed repository, checkout, worktree and standalone rows are absent and that `reconcile_teardown_recoveries`, `reconcile_activation_recoveries`, legacy reconciliation and `startup_recovery_program` produced nothing for them. Fixtures follow the repo rule: `testing::cloned_topology` and `git init` shapes, assertions on fixture output and state files, never on source text.

### Claude's Discretion
- Names of the suppression file, its structs and the `RemovalReport` fields for cleared items and clearing failures.
- Whether the clearing stage is one function or three (binding, marker, suppression), provided the stage order and the report shape above hold.
- Whether a suppression entry is pruned when its path no longer exists on disk, or kept like `folder-workspaces.json` entries are.
- Exact placement of the lift-on-explicit-admission call inside `admit_repository` and `open_new_session_modal`'s admission path, as long as it runs before `admit_existing_worktrees`.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone and requirements
- `.planning/PROJECT.md` — "Current Milestone: v2.8 Project Removal": removal clears binding and marker ownership; return only via `n`, `c`, or launching inside the folder
- `.planning/REQUIREMENTS.md` lines 40 to 44 — READM-01 to READM-05
- `.planning/ROADMAP.md` "Phase 19: Re-admission Suppression" — goal and the four success criteria

### Phase 18 handoff
- `.planning/phases/18-shared-kill-then-remove-lifecycle/18-CONTEXT.md` — stage order, atomicity, what is never deleted, markers deliberately left for Phase 19
- `.planning/phases/18-shared-kill-then-remove-lifecycle/18-VERIFICATION.md` "Deferred Items" row 2 and "Required Artifacts" — the removal API surface this phase extends
- `.planning/phases/18-shared-kill-then-remove-lifecycle/deferred-items.md` — `scripts/gate.sh --linux` bind-mount race on macOS and the manual container variant

### Repo rules
- `CLAUDE.md` — gate before push, no nested parentheses in commit messages, fixture realism rule
- `.planning/phases/18-shared-kill-then-remove-lifecycle/18-PATTERNS.md` — file and test shapes the Phase 18 executors followed

No external specs or ADRs exist for this phase.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `baude-core/src/folder_workspace.rs`: `FILE_NAME`, `FolderWorkspaceFile`, `find_binding`, `record`; writes go through `breadcrumbs::locked_merge_write` and keys come from `breadcrumbs::folder_key`. The clearing write is the same merge path with a `remove` instead of an `insert`.
- `baude-core/src/breadcrumbs.rs`: `folder_key`, `load_json`, `locked_merge_write`, and `Breadcrumbs::flush` at line 198 which already prunes crumbs missing from durable state. The suppression store should be built from these same helpers.
- `baude-core/src/marker.rs`: `read_marker` returning `Missing`, `Valid`, `Invalid`; the symlink and size rules the delete must honour.
- `baude-core/src/lifecycle.rs`: `ensure_repository` at line 1655 writes the marker into `git::worktrees_base()/<workspace>/repository-<digest>`; `check_collision`, `allocate_suffixed_repository_path` and `discover_checkout_owner` near lines 1931 to 2060 are the marker-recovery readers. `startup_recovery_program` at line 417 is the reconcile entry the relaunch test must show is silent for removed rows.
- `baude-core/src/removal.rs`: `remove` at line 396 with `kill_stage`, `disk_stage`, `persist_stage`; `RemovalSet`, `PlannedRow`, `DiskAction::KeepDirectory`, `RemovalReport`, `report_from_set`, `preview_removal`. The fourth stage slots after `persist_stage` and the report gains the clearing fields.
- `baude-core/src/repository.rs`: `SavedRepository::physical_key`, `observed_common_dir`, `observed_main_worktree`; `RepositoryState::remove_rows` and `RemovedRows` give the removed set the clearing stage iterates.
- `baude-core/src/git.rs`: `worktrees_base`, `real_worktrees_base`, `repo_root`; `removal_marker_path` at line 3278 is Phase 18's retry sidecar, not the ownership marker.
- `baude-core/src/worktree_scan.rs`: `discover_owner` at line 862 reads the ownership marker; a cleared directory must read as unowned there.
- `baude-core/src/launch.rs`: `start_workspace` records the binding after the lock claim, step 5; the daemon shares it.
- `baude-core/src/testing.rs`: `cloned_topology`, `git_in`, `worktrees_base_override`, `TestRedirect` for redirecting the config dir and managed root.

### Established Patterns
- Advisory stores beside the state file are read-merge-write under an advisory lock, additive-schema, and degrade to empty on corruption; they never fail a launch. The suppression store follows this exactly.
- The state file is schema-locked with `deny_unknown_fields` on rows; nothing in this phase adds a field there.
- Errors are typed structs carrying paths; best-effort stages report rather than fail.
- Tests build cloned and `git init` fixtures and assert on fixture output and files on disk.

### Integration Points
- `baude/src/app.rs`: `restore` at line 1352 admits `self.launch_dir`; `admit_repository` at line 2292 calls `admit_existing_worktrees` at lines 2393 and 2464; `admit_standalone` at line 2005; `n` opens `open_new_session_modal` at line 5088 and `c` opens the clone input at line 5089. The lift-then-sweep ordering lives here.
- `bauded/src/manager.rs`: `restore` at line 496 is the daemon's parallel path; it must be able to call the same core consult helper in Phase 21.
- `scripts/gate.sh` and `scripts/gate.sh --linux` remain the required gate; the Linux lane needs the manual container variant from Phase 18's deferred-items on a macOS host.

</code_context>

<specifics>
## Specific Ideas

- Treat the three clearing targets as "the places the row could come back from", and name them in the report so Phase 20's confirmation and Phase 21's REST response can list them.
- Keep the fresh-admission path boring: lifting a suppression entry is the only new behaviour; the admission itself is the existing classifier.
- READM-05's sharing case matters because `folder-workspaces.json` is workspace-agnostic and one file for every workspace; a clear by value would wipe neighbours.

</specifics>

<deferred>
## Deferred Ideas

- Wiring the daemon's `Manager::restore` and admission to the suppression consult helper: Phase 21, with the REST and PWA removal.
- Showing cleared bindings, markers and suppression entries in the TUI confirmation: Phase 20 consumes the report fields added here.
- An eager breadcrumb clear on removal, if the daemon path in Phase 21 turns out not to flush breadcrumbs before a TUI relaunch.
- Pruning stale `folder-workspaces.json` entries in general: outside this milestone; entries for detached volumes are legitimate history.
- `scripts/gate.sh --linux` container target dir off the bind mount: logged in Phase 18 deferred-items, still open.
- Unpackaged spike manifest at `.planning/spikes/MANIFEST.md` concerns the opencode backend and is unrelated to this phase; run `/gsd-spike --wrap-up` when that work resumes.

</deferred>

---

*Phase: 19-re-admission-suppression*
*Context gathered: 2026-11-03*
