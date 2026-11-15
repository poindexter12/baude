---
phase: 19-re-admission-suppression
verified: 2026-11-14T09:43:23-08:00
head: ca822383ad0e40b84ea9833b4c404b27cfd7c688
status: passed
score: 40/43 must-haves verified, 3 gate truths deferred to plan 22-05
covered_files:
  - .planning/phases/19-re-admission-suppression/19-01-PLAN.md
  - .planning/phases/19-re-admission-suppression/19-01-SUMMARY.md
  - .planning/phases/19-re-admission-suppression/19-02-PLAN.md
  - .planning/phases/19-re-admission-suppression/19-02-SUMMARY.md
  - .planning/phases/19-re-admission-suppression/19-03-PLAN.md
  - .planning/phases/19-re-admission-suppression/19-03-SUMMARY.md
  - .planning/phases/19-re-admission-suppression/19-04-PLAN.md
  - .planning/phases/19-re-admission-suppression/19-04-SUMMARY.md
  - .planning/phases/19-re-admission-suppression/19-05-PLAN.md
  - .planning/phases/19-re-admission-suppression/19-05-SUMMARY.md
  - .planning/phases/19-re-admission-suppression/19-06-PLAN.md
  - .planning/phases/19-re-admission-suppression/19-06-SUMMARY.md
  - .planning/phases/19-re-admission-suppression/19-07-PLAN.md
  - .planning/phases/19-re-admission-suppression/19-07-SUMMARY.md
  - baude-core/src/breadcrumbs.rs
  - baude-core/src/folder_workspace.rs
  - baude-core/src/lib.rs
  - baude-core/src/marker.rs
  - baude-core/src/removal.rs
  - baude-core/src/suppression.rs
  - baude/src/app.rs
  - bauded/src/manager.rs
covered_digest: "v4:sha256:5c13debb55cd9dbef9f2e5ce521fc57670d4d55beefe6c14be6015d5b3043290"
behavior_unverified: 0
overrides_applied: 0
re_verification:
  date: 2026-11-14
  head: ca822383ad0e40b84ea9833b4c404b27cfd7c688
  previous_status: gaps_found
  previous_score: 39/43
  previous_head: f934e6474e54f7e195919e7fe4cf9d5779d5d66b
  gaps_closed:
    - "The session-start sweep, worktree scan, marker recovery and reconcile do not re-admit the removed repository, checkout, worktree or standalone row across a TUI relaunch (ROADMAP SC2, READM-03, READM-04)"
  gaps_remaining: []
  regressions: []
deferred:
  - truth: "scripts/gate.sh exits 0 on macOS on the final tree"
    addressed_in: "Phase 22, plan 22-05"
    evidence: "Plan 22-05 runs scripts/gate.sh and scripts/gate.sh --linux on the exact pushed commit and keeps the result files. Not run here by instruction. Neither verified nor failed."
  - truth: "scripts/gate.sh --linux exits 0 on the same tree"
    addressed_in: "Phase 22, plan 22-05"
    evidence: "Plan 22-05 runs scripts/gate.sh and scripts/gate.sh --linux on the exact pushed commit and keeps the result files. Not run here by instruction. Neither verified nor failed."
  - truth: "No fixture directory leaks into the real worktrees root or config dir"
    addressed_in: "Phase 22, plan 22-05"
    evidence: "Plan 22-05 runs scripts/gate.sh and scripts/gate.sh --linux on the exact pushed commit and keeps the result files. Not run here by instruction. Neither verified nor failed."
advisory:
  - finding: "A removed baude-made default checkout whose folder was deleted has no one-gesture way back: n, c or a launch on the repository does not derive it again. It returns after a worktree is put back at the old path and that folder is opened, or after the repository row is removed and admitted again."
    category: other
    reason: "READM-04 holds as written, see Requirements Coverage. Recorded so the product owner can confirm the two-step route is intended."
    evidence_status: "tested: removed_primary_managed_returns_when_its_folder_is_back, removed_primary_managed_returns_after_repository_readmission"
  - finding: "While a removed-primary entry stands, launching baude inside a sibling worktree, or inside the clone itself when the removed primary was a baude-made worktree, starts no session and shows a message instead."
    category: other
    reason: "Outside READM-03 and READM-04. It is the shipped behaviour and is asserted by the tests; noted because a launch that starts nothing is a visible cost."
    evidence_status: "tested: removed_primary_stays_removed_on_sibling_relaunch asserts zero sessions"
---

# Phase 19: Re-admission Suppression Verification Report

**Phase Goal:** A removed project stays gone across reconcile and relaunch because removal clears its folder-workspace binding and marker ownership, and every rediscovery path respects that, while `n`, `c` and launching inside the folder still admit it fresh.
**Verified:** 2026-11-13 at HEAD `f934e6474e54f7e195919e7fe4cf9d5779d5d66b` (branch `gsd/phase-18-shared-kill-then-remove-lifecycle`, with Phases 20 and 21 and the Phase 21 review-fix pass on top)
**Status:** passed
**Re-verification:** Yes, after gap closure, 2026-11-14

## Verdict

Re-verified 2026-11-14 at `ca822383ad0e40b84ea9833b4c404b27cfd7c688` after gap closure by plans 22-07 and 22-08. Status: **passed**. 40 of 43 truths verified; the 3 gate truths are deferred to plan 22-05, see Deferred Items.

Gap 1 is closed in code, not only in the summaries. `removal::plan_suppression` now writes a removed primary's entry through `suppression::suppress_primary`, which sets a `primary` flag on the entry. `App::admit_repository` (`baude/src/app.rs:2632-2650`) asks `suppression::removed_primary` for the repository's common dir whenever no `PrimaryDefault` row exists, and when an entry answers it sweeps the other worktrees, saves, sets a message and returns `Ok(None)` before any checkout key is allocated and before `git::ensure_default_worktree` runs. The lift at `app.rs:2520` runs first, so an admission of exactly the removed path still derives the primary in the same pass.

No regression: every test the previous report cited by name passed in this run, with the full `baude` suite at exit 0.

Evidence labels used below: **test** means a named test I ran at HEAD and read the exit code of directly, **code** means read at HEAD with file and line, **not verified** means neither.

## Goal Achievement

### Roadmap success criteria

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| SC1 | After removal, the launch-dir binding walk no longer resolves the removed folder to a workspace and marker recovery does not restore the row | VERIFIED | **test** `removal::tests::clear_stage_removes_repository_binding_and_reports_it` asserts `folder_workspace::find_binding` returns `None` for the removed repo after `removal::remove`. **code** `folder_workspace::clear` at `baude-core/src/folder_workspace.rs:210`, called from `clear_bindings` at `baude-core/src/removal.rs:683`; stage order at `removal.rs:455-456` (`persist_stage` then `clear_stage`). Marker half: **code** no marker reader admits a row. The readers are `lifecycle.rs:1807`, `1931`, `2038` (collision and directory ownership) and `worktree_scan.rs:862` (the CLI scan in `baude/src/main.rs`), none of which pushes a row. |
| SC2 | The session-start sweep, worktree scan, marker recovery and reconcile do not re-admit the removed repository, checkout, worktree or standalone row across a TUI relaunch | VERIFIED | Four row kinds as before: **test** `app::tests::removed_rows_stay_removed_across_relaunch`. Removed primary, the former gap: **test** `removed_primary_stays_removed_on_sibling_relaunch` (cloned fixture, relaunch inside the sibling worktree: no row at the removed path, no `PrimaryDefault` row, `git worktree list` unchanged, entry kept, key allocator unmoved, zero sessions, message names the folder) and `removed_primary_managed_is_not_recreated_on_sibling_relaunch` (baude-made default worktree, folder deleted by the removal, no `git worktree add` on relaunch). **code** `app.rs:2632-2650`, `suppression.rs` `removed_primary`, `removal.rs` `suppress_primary` call. |
| SC3 | `n`, `c`, or launching baude inside the folder admits the project again as a fresh row with no stale identity | VERIFIED | **test** `relaunch_inside_removed_checkout_readmits_it_fresh` (launch dir, entry written by a real removal), `launch_dir_and_clone_completion_routes_lift_suppression` (launch dir and `c`, real `git clone`), `admitting_suppressed_path_lifts_entry_and_admits_fresh_identity`. **code** all three routes reach `admit_repository` through `open_repo_session_via`: `LaunchDirectory` at `baude/src/app.rs:1663`, `Open` at `6541`, `CloneCompletion` at `6618`, admission at `6573`, lift at `2518` before both sweep sites (`2618`, `2689`). The `n` route itself is covered by **code** only: the test calls `admit_repository` directly, not the modal. |
| SC4 | Other rows' bindings, markers and worktrees are unchanged by a removal, including repositories sharing the same workspace | VERIFIED | **test** `removal::tests::removal_leaves_neighbour_bindings_markers_worktrees_and_suppressions_byte_identical`, plus `clear_stage_removes_repository_binding_and_reports_it` (neighbour sharing the workspace value survives) and `folder_workspace::tests::clear_removes_only_the_named_key`. **code** every clear is keyed by an exact `folder_key`, by the removed row's stored `physical_key` (`removal.rs:627-638`), or by the removed repository's common dir (`removal.rs:763`). |

### Deferred Items

Three gate truths of plan 19-07 are deferred to Phase 22 plan 22-05. They are not counted as verified and not counted as failed. `scripts/gate.sh` was not run in this verification, by instruction.

| # | Item | Addressed In | Evidence |
|---|------|--------------|----------|
| 1 | `scripts/gate.sh` exits 0 on macOS on the final tree | Phase 22, plan 22-05 | 22-05-PLAN.md runs the host gate and then the Linux gate on the exact commit that is pushed and keeps `05-mac.result` and `05-linux.result` |
| 2 | `scripts/gate.sh --linux` exits 0 on the same tree | Phase 22, plan 22-05 | 22-05-PLAN.md runs the host gate and then the Linux gate on the exact commit that is pushed and keeps `05-mac.result` and `05-linux.result` |
| 3 | No fixture directory leaks into the real worktrees root or config dir | Phase 22, plan 22-05 | 22-05-PLAN.md runs the host gate and then the Linux gate on the exact commit that is pushed and keeps `05-mac.result` and `05-linux.result` |

### Advisory (New Scope, Unevidenced as Defects)

Reported, not blocking. Both are tested, intended behaviour; they are listed because they bound how READM-04 is read.

| # | Finding | Category | Why Advisory |
|---|---------|----------|--------------|
| 1 | A removed baude-made default checkout whose folder was deleted has no one-gesture way back. It returns when a worktree is put back at the old path and that folder is opened, or when the repository row is removed and the repository admitted again | other | READM-04 holds as written; the two-step route is a product choice to confirm |
| 2 | While a removed-primary entry stands, a launch inside a sibling worktree starts no session and shows a message | other | Outside READM-03 and READM-04; asserted by the tests as shipped behaviour |

### Gap 1 (CLOSED 2026-11-14): a removed primary checkout returned on a relaunch inside a sibling worktree

**Closed by** `754665e` (fix), with red tests in `7a1b0e6` and the ways back pinned in `0413f7e`, `084af67`, `1058fc4`, `e3e7cd2`. Read at `ca822383ad0e40b84ea9833b4c404b27cfd7c688` and run:

- **Stays removed.** `removed_primary_stays_removed_on_sibling_relaunch`, `removed_primary_managed_is_not_recreated_on_sibling_relaunch`, `removed_primary_returns_through_open_route` (its first half: opening a sibling through `n` derives no primary and spends no key).
- **Comes back only by its own path (D-12).** `removed_primary_returns_on_relaunch_inside_its_folder`, `removed_primary_returns_through_open_route`, `removed_primary_returns_through_clone_completion_route`. The lift key is `git::repo_root` of the admitted path, so a sibling or the parent root lifts nothing; `admitting_parent_root_does_not_lift_child_entry` still passes.
- **Folder deleted.** `removed_primary_managed_returns_when_its_folder_is_back` and `removed_primary_managed_returns_after_repository_readmission`. Removing the repository row drops its entries, the existing garbage collection, and the next admission derives a primary with a new `CheckoutKey` and `first_seen_order`.
- **Store compatibility.** `primary_fact_follows_its_entry`, `entries_without_the_primary_member_load`, `removed_primary_store_without_the_fact_still_loads`: the flag is omitted when false and an entry without it loads and does not answer.
- **Daemon.** `bauded/src/manager.rs` has no `PrimaryDefault` derivation (grep at HEAD: no `PrimaryDefault`, no `ensure_default_worktree`), so the route does not exist there.

**D-12 exact-path lift: honoured.** The consult can only stop a derivation; the entry leaves only by an explicit admission of exactly that path or by removal of the repository row.

The text below is the 2026-11-13 finding at `f934e6474e54f7e195919e7fe4cf9d5779d5d66b`, kept for the record. Its line numbers and its override suggestion are superseded.


- **Where:** `baude/src/app.rs:2585-2593` finds no `PrimaryDefault` row for the repository; `baude/src/app.rs:2624-2688` then allocates a new `CheckoutKey` and pushes a new `PrimaryDefault` row at the path `git::ensure_default_worktree` returns, and `ensure_primary` at `2691` launches it. Nothing on that route calls `suppression::is_suppressed`. The only consult is at `baude/src/app.rs:2737` inside `admit_existing_worktrees`.
- **Why the entry exists but does nothing:** `plan_suppression` at `baude-core/src/removal.rs:589-606` writes an entry for every checkout removed while its parent survives, whatever its role.
- **Reachable from the UI:** `SelId::Checkout(key)` maps to `RemovalTarget::Checkout(key)` at `baude/src/app.rs:4003`, so `d` on the main checkout row performs exactly this removal.
- **Reproduction:** I added one test to a scratch copy of the tree (never to the repo) that uses the phase's own `relaunch_fixture` (a real clone with a linked and an unmanaged worktree), removes the primary checkout row through `removal::remove`, then builds a second `App` with the launch dir set to the kept linked worktree and calls `restore()`. `cargo test` exit code 101:

  ```
  PROBE before: primary key=CheckoutKey(1) role=PrimaryDefault
  PROBE after removal: row_gone=true suppressed=true repositories=1
  PROBE after relaunch inside sibling .../wt-linked: row_back=Some((CheckoutKey(4), "PrimaryDefault", 5)) still_suppressed=true message=None
  panicked: removed primary checkout came back on a relaunch inside a sibling worktree
  ```

  The scratch copy differs from HEAD only by those 51 added lines in `baude/src/app.rs`. The probe source is at `/private/tmp/claude-503/-Users-joese-Code-github-com-poindexter12-baude/a2f6da22-a53d-4447-9fb2-62cf58c24430/scratchpad/probe_test.rs` and its log at `/private/tmp/claude-503/-Users-joese-Code-github-com-poindexter12-baude/a2f6da22-a53d-4447-9fb2-62cf58c24430/scratchpad/probe.log`.
- **Why the suite missed it:** `remove_four_rows` (`baude/src/app.rs:10822`) removes the linked worktree, the unmanaged checkout, the standalone and the second repository, and deliberately keeps the clone's primary alive. No test removes a primary and relaunches from anywhere but the removed folder.
- **Not a later-phase item:** Phase 22's criteria are integration coverage, docs and the release. None names this route, so the gap is not deferred.

If the product decision is that a repository row always carries a primary and the main checkout is not independently removable, this needs an explicit override rather than a silent pass, and Phase 20's remove action on the main checkout row then contradicts it:

```yaml
overrides:
  - must_have: "The session-start sweep, worktree scan, marker recovery and reconcile do not re-admit the removed repository, checkout, worktree or standalone row across a TUI relaunch"
    reason: "{why re-deriving a removed primary is acceptable}"
    accepted_by: "{name}"
    accepted_at: "{ISO timestamp}"
```

### Plan must-have truths

All 39 plan truths were checked. Rows are grouped where one test pins several.

| Plan | Truth (abridged) | Status | Evidence |
|------|------------------|--------|----------|
| 01 | Repository removal clears `folder_key(observed_main_worktree)` and `find_binding` returns None | VERIFIED | **test** `clear_stage_removes_repository_binding_and_reports_it` |
| 01 | Checkout and standalone keys cleared by exact key; a neighbour bound to the same workspace is byte-identical | VERIFIED | **test** same test plus the `clear-own-keys` case on `cloned_topology`; `folder_workspace::tests::clear_removes_only_the_named_key` |
| 01 | Standalone removal with no binding leaves the file byte-identical, and absent stays absent | VERIFIED | **test** `standalone_without_binding_leaves_bindings_file_byte_identical` |
| 01 | Clear is a fourth stage after `persist_stage`; an I/O failure is reported, `remove` stays Ok and Saved | VERIFIED | **test** `binding_clear_failure_is_reported_not_raised`; **code** `removal.rs:455-456` |
| 01 | `preview_removal` names the same Binding items the run attempts | VERIFIED | **test** `clear_stage_removes_repository_binding_and_reports_it` asserts `preview.cleared == report.cleared` |
| 01 | `RemovalReport` with the new fields round-trips serde | VERIFIED | **test** `preview_has_no_side_effects_and_report_round_trips_serde` |
| 01 | Launching inside the folder re-records the binding through unchanged `start_workspace` step 5 | VERIFIED | **code** `baude-core/src/launch.rs:110` calls `folder_workspace::record`; no special case added |
| 02 | Per-workspace advisory file beside breadcrumbs, `schema_version`, state file untouched | VERIFIED | **test** `suppression::tests::file_is_named_per_workspace_beside_breadcrumbs`; **code** `suppression.rs:29-64`, production region has no `persist::` reference |
| 02 | `is_suppressed` is the single consult helper, false on absent or corrupt, never errors | VERIFIED | **test** `corrupt_file_reads_as_empty_and_is_replaced_on_next_write`; **code** `suppression.rs:76` returns `bool` |
| 02 | `lift` removes exactly one key | VERIFIED | **test** `lift_removes_only_the_exact_key`, `suppress_then_consult_then_lift_round_trip` |
| 02 | `suppress` and `drop_children` go through `locked_merge_write` | VERIFIED | **test** `drop_children_removes_entries_of_one_common_dir_only`; **code** `suppression.rs:89`, `104`, `123` |
| 02 | Nothing prunes an entry because its path stopped existing | VERIFIED | **code** the module's only writers are `lift`, `suppress`, `drop_children` (`suppression.rs:84-123`) |
| 03 | `clear_owned` deletes the marker only for a `Valid` marker with matching owner bytes; foreign, invalid and symlinked markers are refused | VERIFIED | **test** `clear_owned_removes_owned_marker_and_empty_directory`, `clear_owned_refuses_foreign_marker_and_leaves_directory`, `clear_owned_refuses_invalid_and_symlinked_marker`; **code** `marker.rs:324-332` |
| 03 | Directory removed non-recursively; non-empty is reported `NotEmpty` | VERIFIED | **test** `clear_owned_keeps_non_empty_directory_after_marker_removal`; **code** `marker.rs:340` uses `remove_dir` |
| 03 | A symlinked directory is refused before anything is read | VERIFIED | **test** `clear_owned_refuses_symlinked_directory`; **code** `marker.rs:307-313` |
| 03 | A missing directory yields `Missing`/`Missing` with no error | VERIFIED | **test** `clear_owned_missing_directory_is_a_noop` |
| 03 | After a clear, `read_marker` is `Missing` and the directory is gone | VERIFIED | **test** `clear_owned_removes_owned_marker_and_empty_directory` |
| 04 | `clear_stage` calls `clear_owned` on `repository-<physical_key>` from the stored key, never a recomputed digest | VERIFIED | **test** `repository_removal_clears_owned_marker_and_directory`, `repository_dir_comes_from_stored_physical_key`; **code** `removal.rs:627-638`, `714` |
| 04 | A checkout-only removal never touches the parent's marker or directory | VERIFIED | **test** `child_removal_keeps_parent_marker_and_writes_suppression` |
| 04 | Marker refusal and non-empty directory land in `clearing_failures`; `remove` stays Ok | VERIFIED | **test** `marker_refusal_and_non_empty_dir_are_reported_not_raised` |
| 04 | Entry written only for a checkout whose repository survives; repository removal drops its children's entries; standalone writes none | VERIFIED | **test** `child_removal_keeps_parent_marker_and_writes_suppression`, `repository_removal_drops_child_suppression_and_standalone_writes_none`; **code** `removal.rs:589-606`, `763`, `795` |
| 04 | Everything the stage touches is scoped to the removed set | VERIFIED | **test** `removal_leaves_neighbour_bindings_markers_worktrees_and_suppressions_byte_identical` |
| 04 | `preview_removal` names markers, directories and entries the run clears | VERIFIED | **test** `assert_preview_covers` at `removal.rs:4358`, called in six of the tests above |
| 04 | Nothing is deleted inside a kept main checkout, unmanaged checkout or standalone folder | VERIFIED | **code** production regions of `removal.rs` and `marker.rs` contain no `remove_dir_all`; the stage's only filesystem writes are under the config dir and `repository-<key>` in the managed root |
| 05 | `admit_existing_worktrees` consults the store right after the `owned` check | VERIFIED | **test** `suppressed_worktree_is_skipped_by_admission_sweep`; **code** `baude/src/app.rs:2730-2739` |
| 05 | An explicit admission of exactly that path lifts before the sweep, on `n`, `c` and launch dir | VERIFIED | As SC3 |
| 05 | Admitting the parent root does not lift a child's entry | VERIFIED | **test** `admitting_parent_root_does_not_lift_child_entry`; **code** `app.rs:2507-2512` keys on `git::repo_root(path)` |
| 05 | The fresh row carries no identity from the removed one | VERIFIED | **test** `admitting_suppressed_path_lifts_entry_and_admits_fresh_identity`, `relaunch_inside_removed_checkout_readmits_it_fresh` |
| 05 | A lift I/O error is a message and the admission continues | VERIFIED | **test** `lift_failure_is_a_message_not_an_admission_error` |
| 05 | `admit_standalone` lifts the canonical path | VERIFIED | **code** `baude/src/app.rs:2220` |
| 06 | Across a relaunch, a removed repository, unmanaged checkout, managed worktree and standalone row are all absent | VERIFIED | **test** `removed_rows_stay_removed_across_relaunch`. True as worded; the row kind it does not list is the gap. |
| 06 | On that relaunch the two reconcile passes, legacy reconciliation and `startup_recovery_program` produce nothing | VERIFIED | **test** same test, `app.rs:10991-10998` |
| 06 | Relaunching inside the removed unmanaged checkout admits it fresh in that restore | VERIFIED | **test** `relaunch_inside_removed_checkout_readmits_it_fresh` |
| 06 | `FolderContext::flush` drops the removed path's crumb; no eager clear in the removal stage | VERIFIED | **test** `breadcrumbs::tests::flush_drops_crumb_of_removed_path`; **code** the TUI flushes in `App::save` at `baude/src/app.rs:2070-2072`; `removal.rs` has no `FolderContext` reference |
| 06 | Fixtures are cloned and `git init` shapes, assertions on fixture output and state files | VERIFIED | **code** `relaunch_fixture` at `app.rs:10703` builds on `admission_repo_cloned` and adds a `git init` second repository (`10733-10740`) and a non-git standalone; assertions read rows and files. See fixture notes for the limits. |
| 07 | `scripts/gate.sh` exits 0 on macOS on the final tree | DEFERRED (plan 22-05) | Not run, by instruction. Phase 22 plan 22-05 runs it on the pushed commit and keeps the result files. Neither verified nor failed. |
| 07 | `scripts/gate.sh --linux` exits 0 on the same tree | DEFERRED (plan 22-05) | Not run, by instruction. Phase 22 plan 22-05 runs it on the pushed commit and keeps the result files. Neither verified nor failed. |
| 07 | Every phase commit passes `scripts/check-commit-message.sh` | VERIFIED | **test** `scripts/check-commit-message.sh --range origin/main..HEAD` exit code 0 |
| 07 | No fixture directory leaks into the real worktrees root or config dir | DEFERRED (plan 22-05) | Not run, by instruction. Phase 22 plan 22-05 runs it on the pushed commit and keeps the result files. Neither verified nor failed. |

**Score:** 39/43 truths verified. 1 failed (SC2). 3 not verified, all gate runs excluded from this verification by instruction. 0 present but behavior-unverified.

### Prohibitions

All plan prohibitions carry a named test or a grep as enforcement, so none is flagged unverified except the gate one.

| Plan | Prohibition (abridged) | Result | Evidence |
|------|------------------------|--------|----------|
| 01 | No binding matched by workspace value | Held | **test** `clear_stage_removes_repository_binding_and_reports_it` |
| 01 | Clearing stage never returns Err, never re-saves | Held | **test** `binding_clear_failure_is_reported_not_raised` |
| 01 | `clear` never creates the file and never writes for an absent key | Held | **test** `standalone_without_binding_leaves_bindings_file_byte_identical` |
| 02 | Suppression store never touches the state file | Held | **code** production `persist::` count in `suppression.rs` is 0 |
| 02 | Corrupt or absent file reads as empty | Held | **test** `corrupt_file_reads_as_empty_and_is_replaced_on_next_write` |
| 02 | `lift` removes only the exact key | Held | **test** `lift_removes_only_the_exact_key` |
| 03 | No recursive deletion in `marker.rs` production code | Held | **code** production `remove_dir_all` count is 0 |
| 03 | A foreign marker is never deleted | Held | **test** `clear_owned_refuses_foreign_marker_and_leaves_directory` |
| 03 | A symlinked directory or marker is never followed | Held | **test** `clear_owned_refuses_symlinked_directory`, `clear_owned_refuses_invalid_and_symlinked_marker` |
| 04 | No marker or directory touched for a checkout-only removal | Held | **test** `child_removal_keeps_parent_marker_and_writes_suppression` |
| 04 | Repository directory never recomputed from a digest; a key with a separator or `..` never becomes a path | Held | **test** `repository_dir_comes_from_stored_physical_key`; **code** `removal.rs:627-638` |
| 04 | Clearing stage never fails the removal and never recurses | Held | **test** `marker_refusal_and_non_empty_dir_are_reported_not_raised`; **code** production `remove_dir_all` count in `removal.rs` is 0 |
| 05 | The consult only ever removes a candidate from the sweep | Held | **test** `suppressed_worktree_is_skipped_by_admission_sweep` |
| 05 | Admitting the parent root never lifts a child entry | Held | **test** `admitting_parent_root_does_not_lift_child_entry` |
| 05 | A lift failure never becomes an admission error | Held | **test** `lift_failure_is_a_message_not_an_admission_error` |
| 06 | No eager breadcrumb clearing in the removal stage | Held | **code** `FolderContext` count in `removal.rs` is 0 |
| 06 | Relaunch tests never touch the real config dir or managed root | Unverified, flagged | **code** `remove_project_clearing_failure_is_a_footer_notice_not_an_error` asserts the config dir sits under the fixture root; the gate leak check that proves it estate-wide was not run |
| 07 | Gate steps never judged through a pipe | Unverified, flagged | Gate not run. Every test in this report was run with output redirected to a log and the exit code read directly. |

### Required Artifacts

`verify.artifacts` returned exit 0 with `all_passed: true` for all seven plans.

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `baude-core/src/folder_workspace.rs` | `clear` | VERIFIED | `:210`; called from `removal.rs:683` |
| `baude-core/src/suppression.rs` | consult, lift, suppress, drop_children | VERIFIED | `:76`, `:84`, `:98`, `:113`; exported at `lib.rs:24` |
| `baude-core/src/marker.rs` | `clear_owned` | VERIFIED | `:306`; called from `removal.rs:714` |
| `baude-core/src/removal.rs` | fourth stage, `ClearedItem`, `ClearingFailure`, preview parity | VERIFIED | `ClearingKind` `:152`, `planned_clearings` `:501`, `clear_stage` `:662`, call `:456` |
| `baude/src/app.rs` | sweep consult, lift on explicit admission | VERIFIED for the sweep, incomplete for the primary derivation | consult `:2737`, lift `:2507-2518`, `:2220`. See Gap 1. |
| `baude-core/src/breadcrumbs.rs` | flush prunes the removed path's crumb | VERIFIED | **test** `flush_drops_crumb_of_removed_path` |
| `scripts/gate.sh` | exists | Exists, not run | |

### Key Link Verification

`verify.key-links` returned exit 1 with 0 of 16 links verified. That is a tool limitation, not a wiring failure: the plans write `from` as `file::symbol` and the verb reports "Source file not found (from: must be a relative file path)". The links were traced by hand.

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `removal::remove` | `folder_workspace::clear` | `clear_stage` after `persist_stage` | WIRED | `removal.rs:455-456`, `683` |
| `removal::clear_stage` | `marker::clear_owned` | one call per removed repository | WIRED | `removal.rs:714` |
| `removal::clear_stage` | `suppression::suppress` / `drop_children` | `update_suppression` | WIRED | `removal.rs:763`, `795` |
| `suppression` writers | `breadcrumbs::locked_merge_write` | lock plus atomic rename | WIRED | `suppression.rs:27`, `89`, `104`, `123` |
| `marker::clear_owned` | `marker::read_marker` | owner check | WIRED | `marker.rs:324` |
| `App::admit_existing_worktrees` | `suppression::is_suppressed` | `continue` after `owned` | WIRED | `app.rs:2737` |
| `App::admit_repository` | `suppression::lift` | `lift_suppression` before both sweeps | WIRED | `app.rs:2518`, `2509` |
| `App::admit_repository` primary derivation | `suppression::is_suppressed` | none | NOT_WIRED | `app.rs:2624-2688`. Not a link any plan declared; it is the missing one. Gap 1. |
| TUI removal | `removal::remove` | `confirm_remove_project` | WIRED | `app.rs:4510`; clearing failures surfaced at `4560` |
| `bauded` admission | `suppression::lift` | `lift_admission_suppression` | WIRED | `bauded/src/manager.rs:1575`, `1606-1607` |
| `bauded` admission | `suppression::is_suppressed` | none | Not applicable as far as read | No `is_suppressed` call and no worktree-inventory iteration found in `manager.rs` below line 3700, so there is no daemon sweep to gate. Phase 19 scoped daemon wiring to Phase 21. The file was grepped, not read whole. |

### Data-Flow Trace (Level 4)

Not applicable in the rendering sense. The equivalent trace is "does the entry removal writes reach the reader that decides re-admission": yes for the sweep (`removal.rs:795` to `app.rs:2737`, proven end to end by `removed_rows_stay_removed_across_relaunch`, where the entry is written by a real removal), no for the primary derivation (Gap 1).

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Core clearing, marker, suppression, binding and crumb tests | `cargo test -p baude-core` with the 23 Phase 19 test names as filters | exit 0, 23 passed, 0 failed | PASS |
| TUI sweep, lift, relaunch and clearing-failure tests | `cargo test -p baude -- suppressed_worktree_is_skipped_by_admission_sweep admitting_suppressed_path_lifts_entry_and_admits_fresh_identity removed_rows_stay_removed_across_relaunch relaunch_inside_removed_checkout_readmits_it_fresh admitting_parent_root_does_not_lift_child_entry launch_dir_and_clone_completion_routes_lift_suppression lift_failure_is_a_message_not_an_admission_error remove_project_clearing_failure_is_a_footer_notice_not_an_error` | exit 0, 8 passed, 0 failed | PASS |
| 2026-11-13: removed primary probe in a scratch copy | probe test | exit 101 at `f934e64`, superseded by the rows below | SUPERSEDED |
| Full TUI suite, once | `cargo test -p baude -- --test-threads=1 > target/gsd-22/verify-19/baude-test.log 2>&1; echo exit=$?` | exit=0, 361 passed (360 in the `baude` unit binary, 1 in a second test binary), 0 failed, 0 ignored | PASS |
| Core suppression and removal modules | `cargo test -p baude-core --lib -- suppression:: removal::` | exit=0, 90 passed, 0 failed | PASS |
| Core breadcrumbs, marker and folder-workspace modules | `cargo test -p baude-core --lib -- breadcrumbs:: marker:: folder_workspace::` | exit=0, 35 passed, 0 failed | PASS |
| Removed primary stays removed on a relaunch inside a sibling worktree | in the full run above: the 8 `removed_primary_*` tests in `app::tests` | all 8 ran, suite exit 0 with 0 failed | PASS |
| Regression: every test named in the 2026-11-13 report | the 31 test names that report cites, matched against the three logs above; a name counts when the test ran in a suite that exited 0 with 0 failed | 31 of 31 ran and passed | PASS |
| Commit messages parse for release-please | `scripts/check-commit-message.sh --range origin/main..HEAD` | exit 0 | PASS |

### Probe Execution

No `scripts/*/tests/probe-*.sh` probes are declared by this phase. Step 7c: not applicable.

### Requirements Coverage

No orphaned requirements: REQUIREMENTS.md maps READM-01 to READM-05 to Phase 19 and the plans claim all five.

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| READM-01 | 01, 07 | Removal clears the folder-workspace binding | SATISFIED | SC1 |
| READM-02 | 03, 04, 07 | Removal clears marker ownership so marker recovery does not restore the row | SATISFIED | SC1, plans 03 and 04. See fixture note 2. |
| READM-03 | 02, 04, 05, 06, 07; closed by 22-07 | No rediscovery path re-admits the removed row across a relaunch | SATISFIED | SC2. Satisfied as written: both rediscovery routes, the inventory sweep and the primary derivation, consult the store, proven across a real relaunch on a cloned fixture. |
| READM-04 | 02, 05, 06, 07; closed by 22-07, 22-08 | Re-admission only by `n`, `c` or launching inside the folder, as a fresh row | SATISFIED | SC3 and Gap 1 closure. The "only" half holds as written: a sibling launch no longer brings a removed primary back. The "can be admitted again" half holds as written when the removed folder exists: `n`, `c` and a launch inside it each bring it back fresh. When the removal deleted the folder (a baude-made default worktree) it holds through a composed route only: put a worktree back at the old path and open it, or remove the repository row and admit the repository again. Each ends in `n`, `c` or a launch, so the wording is met, but no single gesture does it. See Advisory 1. |
| READM-05 | 01, 04, 07 | Removal does not alter other rows' bindings, markers or worktrees | SATISFIED | SC4 |

REQUIREMENTS.md marks READM-03 and READM-04 `[x]` Complete. That is supported at `ca82238`.

### Fixture realism

The project rule is that escaped bugs come from fixtures friendlier than production. Where that applies here:

1. **Gap 1 is this pattern.** The relaunch fixture is a real clone, which is right, but the removal set was chosen so the primary always survives and every relaunch starts from the clone root or from the removed folder itself.
2. **Marker clearing is tested mostly on hand-written markers.** `owned_repository_dir` (`removal.rs:4271`) calls `marker::write_marker` into a directory named by a key the test picks. Only `repository_dir_comes_from_stored_physical_key` builds its directory through `lifecycle::ensure_repository`. In production the marker is written only when `ensure_repository` creates the repository row (`lifecycle.rs:1655-1790`, reached through `prepare_activation`). A row created by `App::admit_repository` (`app.rs:2538-2545`) or the daemon (`manager.rs:1501`, `1964`, `2711`) carries a bare counter `physical_key` and has no marker, so for those rows the marker clear is a no-op plus a non-recursive `remove_dir` of `repository-<counter>`. That is safe and matches D-05, but no test runs the clearing stage against a row the TUI admitted.
3. **Binding and marker tests in `removal.rs` sit on `git init` repositories.** `RemovalFixture::new` runs `git init` (`removal.rs:1767`), and the READM-05 neighbour test builds all three repositories with `git init`. Only the `clear-own-keys` case uses `cloned_topology`. Bindings are written with the production writer `folder_workspace::record`, so the keys are realistic.
4. **Plan 05's sweep tests seed the entry by hand** (`seed_suppression`, `app.rs:10575`) on a `git init` fixture. The plan 06 relaunch tests cover the same behaviour with an entry written by a real removal on a clone, so this is not a hole.
5. **The suppression file is never stale in any test.** Nothing covers an entry whose path was deleted, moved or re-created as a different repository, or an entry left behind by the crash window D-14 accepts. Plan 02 keeps entries until lifted by design, so a stale entry silently keeps a re-created worktree at the same path out of the sweep until someone admits that exact path. Gap 1 also leaves a stale entry behind for the re-derived primary.
6. **The `n` route is not driven through the modal.** The test calls `admit_repository` directly; the route from the key to that call is read, not exercised.

### Anti-Patterns Found

No `TBD`, `FIXME` or `XXX` in the seven implementation files. No `TODO`, `HACK`, `todo!` or `unimplemented!` in `suppression.rs`, `marker.rs`, `folder_workspace.rs` or `removal.rs`. No stubs: every new function has callers and tests.

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `baude/src/app.rs` | 2624-2688 | Row re-derivation with no suppression consult | BLOCKER | Gap 1 |
| `.planning/REQUIREMENTS.md` | 42-43, 102-103 | READM-03 and READM-04 marked Complete | WARNING | Overstates the phase |
| `.planning/ROADMAP.md` | 73 | Progress line reads "Phase 19 ... (0/? plans)" while line 121 reads "7/7 plans executed" | INFO | Bookkeeping drift only |

### Human Verification Required

None. The gate runs are deferred to plan 22-05, see Deferred Items; they are not human-verification items of this phase. The two advisory notes ask for no test.

### Gaps Summary

No open gaps. The one gap of 2026-11-13, the primary derivation in `App::admit_repository` that never consulted the suppression store, is closed by `754665e` and pinned by ten tests that passed in this run. Three gate truths remain deferred to plan 22-05 and must be read from that plan's result files, not from this report.

---

_Verified: 2026-11-13 at f934e6474e54f7e195919e7fe4cf9d5779d5d66b; re-verified 2026-11-14 at ca822383ad0e40b84ea9833b4c404b27cfd7c688_
_Verifier: Claude (gsd-verifier)_
