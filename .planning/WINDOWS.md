---
schema_version: 1
open_count: 2
waived_count: 0
fixed_count: 0
total_count: 2
last_updated: 2026-09-15T14:49:18.921Z
---

# Broken Windows Ledger

> Cross-phase defect register. With `workflow.windows_enforce` enabled, `/gsd-ship` blocks while `open_count > 0`.
> Waive with `gsd-tools windows waive <id> "<reason>"` (reason required).
> Mark fixed with `gsd-tools windows fixed <id>`.

| id | phase | kind | file | line | description | status | reason | recorded_at | resolved_at |
|----|-------|------|------|------|-------------|--------|--------|-------------|-------------|
| 1 | 08 | unrun-verify | baude/src/ui.rs |  | ui_fixture_isolation_after_helper_return and ui_fixture_isolation_nested_restore are #[ignore]d until plan 08-08 contains the UsagePoller | open |  | 2026-09-15T14:49:18.515Z |  |
| 2 | 08 | unmet-truth | baude-core/src/lifecycle.rs |  | five lifecycle::tests::* fixtures hold no TestRedirect and fail plan-01 containment; deferred to plan 08-06 | open |  | 2026-09-15T14:49:18.921Z |  |

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
  }
]
````
