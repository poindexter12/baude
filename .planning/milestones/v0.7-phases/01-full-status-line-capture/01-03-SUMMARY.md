---
phase: 01-full-status-line-capture
plan: 03
subsystem: tui
tags: [rust, ratatui, ui, statusline, overlay, claude-meta]

# Dependency graph
requires:
  - phase: 01-02
    provides: "ClaudeMeta.effort/thinking/pr (PrInfo) reader-side fields"
provides:
  - "effort, thinking, and pr conditional rows in the local Modal::Info overlay"
  - "STL-03 complete: pressing `i` on a selected session surfaces effort/thinking/PR state"
affects: [daemon/PWA Tier 2 remote info parity]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "if let Some guard + lines.push(row(...)) to omit a row entirely when the ClaudeMeta field is absent (no — placeholder), mirroring the last_usage conditional-push idiom"
    - "reuse the in-branch row/opt closures rather than redefining them"

key-files:
  created: []
  modified:
    - baude/src/ui.rs

key-decisions:
  - "Rows are conditionally pushed (if let Some) so absent fields are omitted, not shown as —, distinguishing optional new fields from always-present base rows"
  - "thinking renders as on/off; pr renders as #N (review_state) with ? fallback for a missing number and — fallback for a missing review_state"
  - "vim_mode is NOT rendered (captured-but-not-rendered locked scope); remote info branch (lines ~780-832) left untouched"

patterns-established:
  - "Pattern: optional ClaudeMeta fields surface in the local info overlay via if let Some + lines.push(row(...)), omitting the row when None"

requirements-completed: [STL-03]

# Metrics
duration: 5min
completed: 2026-06-15
---

# Phase 1 Plan 03: Full Status-Line Capture (UI Overlay) Summary

**The local `i` info overlay now surfaces a selected session's effort, thinking mode, and PR state as three conditional rows that are omitted entirely when the underlying ClaudeMeta field is absent — completing the user-facing goal of Phase 1 (STL-03).**

## Performance

- **Duration:** ~5 min
- **Completed:** 2026-06-15
- **Tasks:** 1
- **Files modified:** 1

## Accomplishments
- Inserted three conditional rows into the local `Modal::Info` `lines` vec, immediately after the vec literal and before the `last_usage` block:
  - `effort` — `if let Some(e) = &m.effort { lines.push(row("effort", e.clone())); }`
  - `thinking` — renders `on`/`off` from `m.thinking: Option<bool>`
  - `pr` — renders `#N (review_state)`, with `?` when `number` is None and `—` when `review_state` is None, even though `pr` itself is present
- Reused the existing in-branch `row`/`opt` closures (no redefinition) and the `if let Some` conditional-push idiom so absent fields are omitted rather than shown as `—`.
- Overlay auto-sizes from `lines.len()`; no manual height bump needed.
- All CI gates green across the workspace: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo build -p baude`, and `cargo test` (29 passed, 0 failed).

## Task Commits

1. **Task 1: show effort/thinking/pr in local info overlay** — `0637dc3` (feat)

**Plan metadata:** committed separately with this SUMMARY, STATE.md, ROADMAP.md, REQUIREMENTS.md.

## Files Created/Modified
- `baude/src/ui.rs` — added the `effort`/`thinking`/`pr` conditional rows to the local `Modal::Info` branch. The remote info branch, the `last_usage`/`totals` blocks, the `row`/`opt`/`dim`/`val` closures, and all other logic left unchanged.

## Decisions Made
- **Conditional push, not `—` placeholder:** new optional fields follow the `last_usage` analog — the row is only added when the field is `Some`, so absent fields show nothing. Base rows (model, permissions, context) keep their always-present `—` fallback.
- **pr formatting handles partial PrInfo:** a present `pr` with a missing `number` shows `?`; a missing `review_state` shows `—`. This matches the 01-02 reader, which can populate `pr` while leaving `number`/`review_state` as None.
- **vim_mode not rendered:** captured in 01-02 but intentionally omitted from the overlay per the locked STL-03 scope (effort/thinking/pr only).
- **Remote branch untouched:** the `selected_remote()` path (lines ~780-832) and `SessionInfo`/`RemoteInfo` were not modified — remote/PWA parity is deferred to Tier 2.

## Deviations from Plan
None — plan executed exactly as written.

## Threat Model Compliance
- **T-01-07 (Tampering — escapes in field content):** mitigated. Rows are built as ratatui `Span`/`Line` via the existing `row` closure; ratatui renders them as text, so embedded escape sequences are not interpreted. No `print!` of payload strings. `pr.url` is not rendered.
- **T-01-08 (DoS — long field widening overlay):** accepted per plan. `centered(area, 76, ...)` fixes width at 76 and ratatui clips — no new risk beyond existing rows.
- **T-01-SC (dependency tampering):** N/A — no `Cargo.toml` change, zero new dependencies (`format!`/ratatui already in scope).

## Known Stubs
None.

## Issues Encountered
None. Build, clippy (`-D warnings`), fmt, and the full workspace test suite were clean on the first run after the edit.

## User Setup Required
None — no external service configuration, no new dependencies.

## Next Phase Readiness
- STL-03 done: pressing `i` on a selected session shows effort/thinking/PR state in the local overlay, with rows omitted when absent.
- The captured-but-not-rendered `vim_mode` and the `worktree` sub-struct remain available on `ClaudeMeta` for future plans / Tier 2 remote parity.
- End-of-phase human verification (per `human_verify_mode: end-of-phase`) confirms the render: run baude, select a session whose bridge file has effort/thinking/pr, press `i`, and confirm the three rows appear (and are omitted when absent), with vim mode not shown.
- No blockers.

## Self-Check: PASSED

- FOUND: baude/src/ui.rs (effort/thinking/pr conditional rows present in local Modal::Info branch)
- FOUND commit 0637dc3 (feat: show effort/thinking/pr in local info overlay)
- workspace test suite: 29 passed, 0 failed
- clippy --all-targets -D warnings: clean
- cargo fmt --check: clean

---
*Phase: 01-full-status-line-capture*
*Completed: 2026-06-15*
