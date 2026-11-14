---
phase: 22-validation-and-v2-8-0-release
plan: 04
subsystem: documentation
tags: [removal, readme, bauded, pwa, remote-rows, docs-only, REL-02]
status: complete
requires:
  - the removed default checkout behaviour of plans 22-07 and 22-08
  - the daemon events and removal routes as left by plan 22-02
provides:
  - a `## Removing projects` section in the README covering the `D` key, its confirmation, what is deleted and kept per row kind, what baude forgets, and the ways back after a default checkout is removed
  - a `D` row in the Keys table and a corrected standalone paragraph
  - remote-row removal steps in `## Remote sessions in the TUI`
  - four endpoint rows, a status table and a PWA subsection in `## bauded (experimental)`
affects:
  - plan 22-05, which runs the full gates on the final tree
  - the owner's manual review of the README diff, which REL-02 names
tech-stack:
  added: []
  patterns:
    - every sentence added is traced to a source line or a passing test in the table below
key-files:
  created: []
  modified:
    - README.md
decisions:
  - 400 is left out of the removal status table because the only 400 found comes from POST /sessions
  - 409 and 503 are scoped to POST /removals because the preview handler returns only 200, 404 and 500
  - the PWA paragraph warns that neither the main tag nor the worktree tag says what a removal deletes, since the daemon stores every non-main checkout as managed_branch whether or not baude manages it
  - unclaimed sessions are described as listed after the groups with no remove control, not as an other sessions group, because the page never passes the keyless option
  - the sidebar hint asterisk is not mentioned because ui.rs defines no meaning for it
  - the sibling relaunch behaviour got its own subsection, not one sentence, as the orchestrator directed
  - REL-02 is not marked complete here and STATE.md, ROADMAP.md and REQUIREMENTS.md are untouched, as the orchestrator directed
metrics:
  duration: 12m
  completed: 2026-11-14
  tasks: 2
  files: 1
actuals:
  tokens: 3900
  tasks: 2
  commits: 2
plan_head_before: 6c3d33c8c2929e33b7624b7e0add5205fbdc001f
plan_head_after: a7d43fbb4e070140d822b23be2d2b899f90bf161
---

# Phase 22 Plan 04: README and PWA docs for project removal Summary

The README now documents the `D` remove action for the TUI, remote rows, the daemon API and the PWA, with the deletes and keeps written from `disk_action_for` and every claim traced to a source line.

## Commits

| Task | Commit | Message |
|------|--------|---------|
| 1 | 6b61bf7 | docs(22-04): document the D remove action in the README |
| 2 | a7d43fb | docs(22-04): document removal over the daemon API, in the PWA and on remote rows |

Base recorded at start: `6c3d33c`, as expected. Only `README.md` changed: 233 insertions and 2 deletions across the two commits.

## Sections changed in README.md

| Lines | Section | Change |
|-------|---------|--------|
| 22-26 | intro, standalone paragraph | `X` stays unavailable; `D` removes the row and leaves the folder untouched |
| 153 | Keys table | new `D` row after `X` |
| 397-520 | `## Removing projects` | new section, placed after `## Worktrees` and before `## Usage panel` |
| 900-926 | `## Remote sessions in the TUI` | `D` on remote rows, step by step, with the outcomes |
| 970-973 | bauded endpoint table | `GET /rows`, `POST /removals/preview`, `POST /removals`, `GET /events` |
| 975-1005 | `### Removing rows over the API` | new: the two calls, the status table, `saved_degraded`, the event stream |
| 1007-1045 | `### Removing rows in the PWA` | new: remove control, tags, confirmation, retry, events |

## The two details Task 1 had to settle

1. The trailing asterisk on the `D remove*` and `X remove*` sidebar hints. The hints are at `baude/src/ui.rs` 1471 to 1521. One search of `ui.rs` found no help or footer text that defines the asterisk. The README does not mention it.
2. The command that deletes a managed worktree. Found: `forced_remove_arguments` at `baude-core/src/git.rs` 2889 to 2897 passes `worktree remove --force --force --`, called from `remove_managed_worktree_forced` at 3418. When that fails the directory is deleted with `remove_dir_all` at 3479. The README names the command exactly as the code passes it and states the fallback.

## Where the code disagreed with the plan

1. **400 is not a removal status.** The plan asked to list 400 only if a removal endpoint returns it. `fn matrix_refused_admission_is_400_plain_text` at `bauded/src/api.rs` 6115 posts to `/sessions`, not to a removal endpoint, and `removal_error` at 130 to 153 has no 400 arm. 400 is left out.
2. **409 and 503 come from `POST /removals` only.** The plan listed them without scope. `preview_removal` at `api.rs` 156 to 167 maps an unknown target to 404 and every other error to 500. The README scopes 409 and 503 to `POST /removals`.
3. **No `other sessions` group on the page.** The plan says unclaimed sessions sit under `other sessions`. `renderRowsList` draws that heading only for `options.keyless`, at `removal-controller.js` 549 to 554, and the page never passes `keyless`: `bauded/web/app.js` 716 to 728 appends unclaimed sessions after the tree with no heading, archived ones under `archived`. The README says they are listed after the groups with no remove control.
4. **The `worktree` tag does not mean managed.** The plan treats the `main` tag as the only misleading one. `admitted_checkout_role` at `bauded/src/manager.rs` 1908 to 1923 stores every checkout that is not the repository's main worktree as `ManagedBranch`, and the comment at 1976 says a plain admission is never managed. So a linked worktree the daemon did not create is tagged `worktree` and kept. The same function never assigns `PrimaryDefault`, so the README states the `main` tag at the page level, from `roleLabel` at `removal-controller.js` 493 to 497, and does not claim the daemon serves a managed default checkout.
5. **The PWA title has three kinds, the TUI has seven.** The plan gives one title form for both. `targetKind` at `bauded/web/removal.js` 144 to 149 yields `repository`, `checkout` or `standalone session`. `removal_kind_label` at `baude/src/app.rs` 3949 to 3973 yields `repository`, `main checkout`, `managed worktree`, `unmanaged checkout`, `unavailable worktree`, `checkout` or `standalone session`. Each list is given in its own section.
6. **`D` on a remote row is narrower than "works on remote rows".** `target_for_session` at `baude/src/remote.rs` 241 to 254 returns a checkout or standalone target only, never a repository, and `open_remote_remove` at `app.rs` 4143 to 4148 sends no request for a session the daemon's index gives no key. Both are in the README.
7. **Line numbers moved.** The disk rule the plan places at `removal.rs` 1088 to 1092 is `disk_action_for` at 1095 to 1100. The remote sentence the orchestrator placed near README 769 is now at 896 to 898, after the new section.
8. **The daemon writes no ownership marker.** Per `22-02-SUMMARY.md`. The marker is mentioned only in the TUI section, with "when they exist". The bauded and PWA text does not mention it.

## Traceability

Every factual claim added, by README line, with its source.

| README | Claim | Source |
|--------|-------|--------|
| 22-26 | `D` on a standalone folder removes the row, folder untouched | `removal.rs` 346-348, a standalone row is planned with `DiskAction::KeepDirectory` |
| 153, 399-402 | `D` is offered on every row kind, remote rows through the daemon | `baude/src/app.rs` 338, 4071-4075; hints at `ui.rs` 1471-1521 |
| 404-409 | `x` closes the runtime only; `X` needs a clean managed worktree; `D` has no clean check | existing Keys rows; `ui.rs` 2502; `removal.rs` 1095-1100 |
| 413-416 | heading `Remove this <kind>?` and the seven kinds | `ui.rs` 2753; `app.rs` 3949-3973 |
| 416-421 | the four lists with counts, pids, `already removed`, `cleared`, footer, scroll keys | `ui.rs` 2758-2849 |
| 426-430 | recheck on confirm; refreshed confirmation; error keeps the modal with `enter retry · esc cancel` | `app.rs` 4489-4512, 6299-6306, 609-610; `ui.rs` 2847 |
| 434-445 | deletes and keeps per row kind | `removal.rs` 1093-1100 `disk_action_for` |
| 447-455 | a default checkout baude created is a managed worktree and is deleted; how it comes to exist | `removal.rs` 1095-1100; `git.rs` 1189 `DefaultWorktreeOutcome::CreatedManaged`; test `removed_primary_managed_is_not_recreated_on_sibling_relaunch` at `app.rs` 11274 |
| 457-460 | `git worktree remove --force --force -- <path>`, then directory delete on failure | `git.rs` 2889-2897, 3418, 3479 |
| 462-463 | local branches are never deleted | test assertions at `removal.rs` 3305-3316 |
| 465-467 | a managed directory already gone is listed under `already removed` and its row dropped | `removal.rs` 1102-1112 `probe_disk`, 987-994, 1169 |
| 469-470 | runtime killed first; a survivor stops the removal | `removal.rs` 923-939 `kill_stage` before 971 `disk_stage`; `RuntimeSurvived` at 242 |
| 477-491 | bindings, marker, repository directory, suppression added or dropped, per kind | `removal.rs` 154-158 `ClearingKind`, 505-660 clearing plan, 662 `clear_stage`, 800; `suppression.rs` 1-13 |
| 495-501 | a removed default checkout stays removed on a sibling launch or open; no session, other rows kept | tests `removed_primary_stays_removed_on_sibling_relaunch` at `app.rs` 11200 and `removed_primary_managed_is_not_recreated_on_sibling_relaunch` at 11274 |
| 503-506 | message when the folder was kept, and the way back | `app.rs` 2642; tests at 11449 and 11478 |
| 508-516 | message when the folder was deleted, and the two ways back | `app.rs` 2646; tests at 11598 and 11665 |
| 518-519 | the checkout returns as a new row, nothing resumed | test `relaunch_inside_removed_checkout_readmits_it_fresh` at `app.rs` 11041 |
| 900-901 | remote `D` targets the checkout or standalone row owning the session, never a repository | `remote.rs` 241-254 |
| 903-905 | preview on a worker, `remote removal` waiting box, no request without a key | `app.rs` 4135-4169; `remote.rs` 1148-1159; `ui.rs` 1852-1867 |
| 906-909 | same confirmation; previewed report sent back unchanged and once | `app.rs` 4266-4280, 4178-4222; `remote.rs` 564-578, 1161-1176, assertion at 2018 |
| 910-912 | the daemon removes on its own machine; nothing local is touched | `app.rs` 4171-4173, 6289-6293; `api.rs` 169-193 |
| 914-920 | refused or unanswered preview; drift; refused removal; row already gone | `app.rs` 4296-4337, 4369-4398, 609-610 |
| 922-926 | an unanswered removal is never resent; removed if no longer listed; else new preview on `enter` | `app.rs` 4175-4200, 4399-4424; `remote.rs` 396-399 |
| 970 | `GET /rows` shape | `api.rs` 230-234; `manager.rs` 345-358 |
| 971 | preview body is the bare target | `removal.rs` 46-53; `api.rs` 155-167; `removal-controller.js` 290-292 |
| 972 | execute body is the previewed report unchanged | `api.rs` 169-193; `remote.rs` 564-578 |
| 973, 1001-1005 | `rows_removed` per committed removal, same JSON, no history or replay | `api.rs` 726-755 |
| 977-983 | only the target and the three compared sets are read | `api.rs` 169-171; `manager.rs` 1148-1152 |
| 987-993 | status meanings; plain text except the 409 | `api.rs` 128-153, 156-167, 177-192 |
| 995-999 | `saved_degraded`: row, session and runtime gone; status dirty with the detail | test `matrix_failure_committed_replacement_is_a_200_saved_degraded` at `api.rs` 5992-6026, passing |
| 1009-1013 | list layout; a `✕` control per repository, checkout and folder; none for unclaimed sessions | `removal-controller.js` 463-470, 525-548; `app.js` 711-728 |
| 1015-1021 | the tags and why neither says what is deleted; title for every checkout | `removal-controller.js` 493-497; `manager.rs` 1908-1923, 1976; `removal.js` 144-149 |
| 1023-1031 | titles, sections, cancel paths, result line | `removal.js` 190-223, 306-313; `removal-controller.js` 363-366, 202-223; `app.js` 653-665, 964 |
| 1033-1038 | `retry` label; what a press does after a failed preview and after a failed removal; drift; 404 | `removal-controller.js` 566-573, 328-357, 118-120, 264-288, 230-247 |
| 1040-1041 | the page listens on `/events`; `removed elsewhere` | `removal-controller.js` 371-402, 416-433, 255 |
| 1043-1045 | deletes and keeps are the same on the daemon | the daemon calls the core removal: `manager.rs` 199-207, `api.rs` 177 |

## Tests run as evidence

No test was added or changed. These were run to back statements read from tests.

| Command | Exit | Result |
|---------|------|--------|
| `cargo test -p baude -- --test-threads=1 app::tests::removed_primary_ app::tests::relaunch_inside_removed_checkout_readmits_it_fresh` | 0 | 9 passed, log at `target/gsd-22/04-app-tests.log` |
| `cargo test -p baude-core -- --test-threads=1 removal::tests` | 0 | 53 passed, log at `target/gsd-22/04-core-removal-tests.log` |
| `cargo test -p bauded -- matrix_failure_committed_replacement_is_a_200_saved_degraded matrix_refused_admission_is_400_plain_text events_saved_degraded_publishes_exactly_one` | 0 | 3 passed, log at `target/gsd-22/04-bauded-tests.log` |

## Verification

| Step | Echoed line | Exit |
|------|-------------|------|
| Task 1 verify | `section=1 keyrow=1 default_row=12 diff=0 other_files=0` | 0 |
| Task 2 verify, the plan gate | `preview=3 execute=7 rows=4 events=3 event_name=2 remote_d=2 section=1 diff=0 other_files=0 messages=0` | 0 |
| `scripts/check-commit-message.sh --range 6c3d33c..HEAD` | run inside the Task 2 verify as `messages` | 0 |
| em dash and en dash search over added lines | no match | grep exit 1 |

`scripts/gate.sh` was not run, as directed.

## Deviations from Plan

1. **Sibling relaunch is a subsection, not one sentence.** The plan asked for one sentence. The orchestrator asked for the exact messages and the ways back, so `### After removing the default checkout` gives both messages and each way back.
2. **Closed reading list exceeded.** `bauded/web/app.js`, `bauded/src/manager.rs` and parts of `baude-core/src/git.rs` were read beyond the plan's list, to check the PWA row list, the daemon's stored roles and the forced removal. Each found a disagreement or a fact recorded above.
3. **State updates skipped.** `roadmap.update-plan-progress`, the `state.*` handlers and `requirements.mark-complete` were not run, as the orchestrator directed.

No product code or test was changed, and no doc statement needed a code change to be true.

## Open

- The owner's review of the README diff against the traceability table. This is the manual accuracy check REL-02 names, and nothing here stands in for it.
- The PWA text is read from `bauded/web/*.js`. No browser run and no live event stream check was done, and the JS tests under `bauded/web/tests` were not run.
- Disagreement 4 may be worth a look outside this plan: the PWA tags an unmanaged linked worktree `worktree` and a default checkout `main`, so the tag never tells a user whether a directory goes. The README says to read `paths deleted`.

## Known Stubs

None.

## Self-Check: PASSED

- FOUND: README.md with `## Removing projects`
- FOUND: 6b61bf7
- FOUND: a7d43fb
