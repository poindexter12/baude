---
schema_version: 1
open_count: 6
waived_count: 0
fixed_count: 5
total_count: 11
last_updated: 2026-09-16T15:09:46.693Z
---

# Broken Windows Ledger

> Cross-phase defect register. With `workflow.windows_enforce` enabled, `/gsd-ship` blocks while `open_count > 0`.
> Waive with `gsd-tools windows waive <id> "<reason>"` (reason required).
> Mark fixed with `gsd-tools windows fixed <id>`.

| id | phase | kind | file | line | description | status | reason | recorded_at | resolved_at |
|----|-------|------|------|------|-------------|--------|--------|-------------|-------------|
| 1 | 08 | unrun-verify | baude/src/ui.rs |  | ui_fixture_isolation_after_helper_return and ui_fixture_isolation_nested_restore are #[ignore]d until plan 08-08 contains the UsagePoller | fixed |  | 2026-09-15T14:49:18.515Z | 2026-09-15T17:34:25.822Z |
| 2 | 08 | unmet-truth | baude-core/src/lifecycle.rs |  | five lifecycle::tests::* fixtures hold no TestRedirect and fail plan-01 containment; deferred to plan 08-06 | fixed |  | 2026-09-15T14:49:18.921Z | 2026-09-15T17:34:26.211Z |
| 3 | 08 | deviation | baude-core/src/git.rs |  | 08-04: worktree_inventory extracted from discover_repository (Rule 3) — discover_repository cannot answer the disownment question, so GitDisownsIt would have been unreachable | open |  | 2026-09-15T15:13:58.758Z |  |
| 4 | 08 | deviation | baude-core/src/git.rs |  | 08-04 v2.2 FOLLOW-UP: empty-parent worktree leak is a LIVE production defect in ensure_default_worktree and activate_branch_with_post_add_hook — nothing removes the repository-<key> parent on failure; 'zero candidates' must never be an exit criterion | open |  | 2026-09-15T15:13:59.247Z |  |
| 5 | 08 | unrun-verify | .planning/phases/08-test-isolation-and-fixture-ownership/08-08-PLAN.md |  | Full workspace test suite deliberately not run in 08-08: the plan forbids broad runs before 08-06 task 1 completes manager ownership | fixed |  | 2026-09-15T15:38:04.984Z | 2026-09-15T17:34:26.610Z |
| 6 | 08 | deviation | baude/src/app.rs | 5077 | App::open_editor (and the pbcopy spawn at 5442) still spawn uncontained subprocesses with the inherited environment; not test-reachable today, no guard prevents it. 10-01 adds the link opener (spawn_opener) as a third member: injected-by-construction — production call site passes spawn_opener; every test passes a closure spy; not test-reachable | open |  | 2026-09-15T15:38:12.573Z |  |
| 7 | 08 | deviation | baude-core/src/worktree_scan.rs |  | 08-05: decision C's gitdir-routing clause is structurally unreachable; prune REFUSES a gitdir-bearing candidate instead of routing it to git's verified-removal path | open |  | 2026-09-15T16:31:45.745Z |  |
| 8 | 08 | deviation | .planning/REQUIREMENTS.md |  | 08-05: TISO-04 left partial, not complete — plan 08-07 still owns the CLI preview surface the requirement's wording promises | open |  | 2026-09-15T16:31:46.217Z |  |
| 9 | 08 | stub | baude/src/main.rs |  | `--json --prune` rejection path is fail-closed but untested; a future serializable prune account should replace the rejection and test both | open |  | 2026-09-15T18:05:06.320Z |  |
| 10 | 10 | stub | baude/src/links.rs |  | collect_bare_links is a planned seam returning no candidates until plan 10-03 wires the bare-URL pass (wrap-join + punctuation trim) | fixed |  | 2026-09-16T07:24:23.865Z | 2026-09-16T08:05:24.696Z |
| 11 | 11 | stub | baude/src/app.rs |  | forward_key hardcodes EncodeCtx.kitty_child=false (fail-closed per D-04); intentional — plan 11-04 replaces it with the vt100 kitty_keyboard() observed-push read | fixed |  | 2026-09-16T14:34:38.004Z | 2026-09-16T15:09:46.693Z |

````json
[
  {
    "id": 1,
    "kind": "unrun-verify",
    "phase": "08",
    "file": "baude/src/ui.rs",
    "line": null,
    "description": "ui_fixture_isolation_after_helper_return and ui_fixture_isolation_nested_restore are #[ignore]d until plan 08-08 contains the UsagePoller",
    "status": "fixed",
    "reason": "",
    "recorded_at": "2026-09-15T14:49:18.515Z",
    "resolved_at": "2026-09-15T17:34:25.822Z",
    "milestone": "v2.2"
  },
  {
    "id": 2,
    "kind": "unmet-truth",
    "phase": "08",
    "file": "baude-core/src/lifecycle.rs",
    "line": null,
    "description": "five lifecycle::tests::* fixtures hold no TestRedirect and fail plan-01 containment; deferred to plan 08-06",
    "status": "fixed",
    "reason": "",
    "recorded_at": "2026-09-15T14:49:18.921Z",
    "resolved_at": "2026-09-15T17:34:26.211Z",
    "milestone": "v2.2"
  },
  {
    "id": 3,
    "kind": "deviation",
    "phase": "08",
    "file": "baude-core/src/git.rs",
    "line": null,
    "description": "08-04: worktree_inventory extracted from discover_repository (Rule 3) — discover_repository cannot answer the disownment question, so GitDisownsIt would have been unreachable",
    "status": "open",
    "reason": "",
    "recorded_at": "2026-09-15T15:13:58.758Z",
    "resolved_at": null,
    "milestone": "v2.2"
  },
  {
    "id": 4,
    "kind": "deviation",
    "phase": "08",
    "file": "baude-core/src/git.rs",
    "line": null,
    "description": "08-04 v2.2 FOLLOW-UP: empty-parent worktree leak is a LIVE production defect in ensure_default_worktree and activate_branch_with_post_add_hook — nothing removes the repository-<key> parent on failure; 'zero candidates' must never be an exit criterion",
    "status": "open",
    "reason": "",
    "recorded_at": "2026-09-15T15:13:59.247Z",
    "resolved_at": null,
    "milestone": "v2.2"
  },
  {
    "id": 5,
    "kind": "unrun-verify",
    "phase": "08",
    "file": ".planning/phases/08-test-isolation-and-fixture-ownership/08-08-PLAN.md",
    "line": null,
    "description": "Full workspace test suite deliberately not run in 08-08: the plan forbids broad runs before 08-06 task 1 completes manager ownership",
    "status": "fixed",
    "reason": "",
    "recorded_at": "2026-09-15T15:38:04.984Z",
    "resolved_at": "2026-09-15T17:34:26.610Z",
    "milestone": "v2.2"
  },
  {
    "id": 6,
    "kind": "deviation",
    "phase": "08",
    "file": "baude/src/app.rs",
    "line": 5077,
    "description": "App::open_editor (and the pbcopy spawn at 5442) still spawn uncontained subprocesses with the inherited environment; not test-reachable today, no guard prevents it. 10-01 adds the link opener (spawn_opener) as a third member: injected-by-construction — production call site passes spawn_opener; every test passes a closure spy; not test-reachable",
    "status": "open",
    "reason": "",
    "recorded_at": "2026-09-15T15:38:12.573Z",
    "resolved_at": null,
    "milestone": "v2.2"
  },
  {
    "id": 7,
    "kind": "deviation",
    "phase": "08",
    "file": "baude-core/src/worktree_scan.rs",
    "line": null,
    "description": "08-05: decision C's gitdir-routing clause is structurally unreachable; prune REFUSES a gitdir-bearing candidate instead of routing it to git's verified-removal path",
    "status": "open",
    "reason": "",
    "recorded_at": "2026-09-15T16:31:45.745Z",
    "resolved_at": null,
    "milestone": "v2.2"
  },
  {
    "id": 8,
    "kind": "deviation",
    "phase": "08",
    "file": ".planning/REQUIREMENTS.md",
    "line": null,
    "description": "08-05: TISO-04 left partial, not complete — plan 08-07 still owns the CLI preview surface the requirement's wording promises",
    "status": "open",
    "reason": "",
    "recorded_at": "2026-09-15T16:31:46.217Z",
    "resolved_at": null,
    "milestone": "v2.2"
  },
  {
    "id": 9,
    "kind": "stub",
    "phase": "08",
    "file": "baude/src/main.rs",
    "line": null,
    "description": "`--json --prune` rejection path is fail-closed but untested; a future serializable prune account should replace the rejection and test both",
    "status": "open",
    "reason": "",
    "recorded_at": "2026-09-15T18:05:06.320Z",
    "resolved_at": null,
    "milestone": "v2.2"
  },
  {
    "id": 10,
    "kind": "stub",
    "phase": "10",
    "file": "baude/src/links.rs",
    "line": null,
    "description": "collect_bare_links is a planned seam returning no candidates until plan 10-03 wires the bare-URL pass (wrap-join + punctuation trim)",
    "status": "fixed",
    "reason": "",
    "recorded_at": "2026-09-16T07:24:23.865Z",
    "resolved_at": "2026-09-16T08:05:24.696Z",
    "milestone": "v2.2"
  },
  {
    "id": 11,
    "kind": "stub",
    "phase": "11",
    "file": "baude/src/app.rs",
    "line": null,
    "description": "forward_key hardcodes EncodeCtx.kitty_child=false (fail-closed per D-04); intentional — plan 11-04 replaces it with the vt100 kitty_keyboard() observed-push read",
    "status": "fixed",
    "reason": "",
    "recorded_at": "2026-09-16T14:34:38.004Z",
    "resolved_at": "2026-09-16T15:09:46.693Z",
    "milestone": "v2.2"
  }
]
````
