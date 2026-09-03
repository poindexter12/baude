---
phase: 05-durable-repository-admission
status: partial
created: 2026-08-30
deferred_until: end-of-milestone-or-published-build
---

# Phase 5 Human UAT

Human validation was explicitly deferred until the end of the milestone or a published build.

## Pending Checks

### Real Active-Backend Admission and Reopen

- Test isolated Claude Code and OpenCode workspaces by opening one repository through its main checkout, nested directory, symlink, and linked worktree.
- Repeat admission while the primary is live and after it exits.
- Confirm one durable parent and one default-primary checkout/session per workspace, with focus or resume instead of duplication.

### Unavailable-Default Message Quality

- Open a repository with missing or dangling local remote-HEAD metadata in the local TUI.
- Confirm no session, checkout switch, or fetch occurs.
- Confirm the visible message explains the local metadata problem and a practical recovery action.
