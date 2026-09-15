---
schema_version: 1
open_count: 4
waived_count: 0
fixed_count: 0
total_count: 4
last_updated: 2026-09-15T15:13:59.247Z
---

# Broken Windows Ledger

> Cross-phase defect register. With `workflow.windows_enforce` enabled, `/gsd-ship` blocks while `open_count > 0`.
> Waive with `gsd-tools windows waive <id> "<reason>"` (reason required).
> Mark fixed with `gsd-tools windows fixed <id>`.

| id | phase | kind | file | line | description | status | reason | recorded_at | resolved_at |
|----|-------|------|------|------|-------------|--------|--------|-------------|-------------|
| 1 | 08 | unrun-verify | baude/src/ui.rs |  | ui_fixture_isolation_after_helper_return and ui_fixture_isolation_nested_restore are #[ignore]d until plan 08-08 contains the UsagePoller | open |  | 2026-09-15T14:49:18.515Z |  |
| 2 | 08 | unmet-truth | baude-core/src/lifecycle.rs |  | five lifecycle::tests::* fixtures hold no TestRedirect and fail plan-01 containment; deferred to plan 08-06 | open |  | 2026-09-15T14:49:18.921Z |  |
| 3 | 08 | deviation | baude-core/src/git.rs |  | 08-04: worktree_inventory extracted from discover_repository (Rule 3) — discover_repository cannot answer the disownment question, so GitDisownsIt would have been unreachable | open |  | 2026-09-15T15:13:58.758Z |  |
| 4 | 08 | deviation | baude-core/src/git.rs |  | 08-04 v2.2 FOLLOW-UP: empty-parent worktree leak is a LIVE production defect in ensure_default_worktree and activate_branch_with_post_add_hook — nothing removes the repository-<key> parent on failure; 'zero candidates' must never be an exit criterion | open |  | 2026-09-15T15:13:59.247Z |  |

````json
[
  {
    "id": 1,
    "kind": "unrun-verify",
    "phase": "08",
    "file": "baude/src/ui.rs",
    "line": null,
    "description": "ui_fixture_isolation_after_helper_return and ui_fixture_isolation_nested_restore are #[ignore]d until plan 08-08 contains the UsagePoller",
    "status": "open",
    "reason": "",
    "recorded_at": "2026-09-15T14:49:18.515Z",
    "resolved_at": null,
    "milestone": "v2.2"
  },
  {
    "id": 2,
    "kind": "unmet-truth",
    "phase": "08",
    "file": "baude-core/src/lifecycle.rs",
    "line": null,
    "description": "five lifecycle::tests::* fixtures hold no TestRedirect and fail plan-01 containment; deferred to plan 08-06",
    "status": "open",
    "reason": "",
    "recorded_at": "2026-09-15T14:49:18.921Z",
    "resolved_at": null,
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
  }
]
````
