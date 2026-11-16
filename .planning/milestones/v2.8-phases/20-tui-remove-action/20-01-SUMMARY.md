---
phase: 20
plan: 01
subsystem: tui
tags: [tui, removal, confirmation, tdd]
status: complete
requirements_addressed: [TUI-02, TUI-03]
dependency_graph:
  requires:
    - baude_core::removal::RemovalReport (phase 18 preview_removal output)
  provides:
    - baude/src/ui.rs: pub(crate) fn remove_project_confirmation_lines
  affects:
    - plan 20-02 (wires the D modal and consumes this renderer)
tech_stack:
  added: []
  patterns:
    - pure line builder over a core report, no App needed to test
    - test keys minted through serde_json because the key newtypes keep their field private
key_files:
  created: []
  modified:
    - baude/src/ui.rs
files_modified:
  - baude/src/ui.rs
commits:
  - sha: 006edb8
    subject: "test(20-01): add failing remove_project_confirmation_lines tests"
  - sha: 92dee00
    subject: "feat(20-01): render remove confirmation lines from RemovalReport"
decisions:
  - "Section layout: unindented count headings `label (n)` with two-space indented entries; `+N more` is an entry under rows dropped so the heading count stays the total"
  - "Empty runtimes killed, paths deleted and paths kept render an indented `none`; already removed and cleared are omitted when empty"
  - "cleared summarises by kind: Binding -> bindings, Marker and RepositoryDir -> markers, SuppressionAdded and SuppressionDropped -> suppression entries"
  - "dead_code allowance is `cfg_attr(not(test))` only, to be dropped by plan 20-02 when the D modal calls the renderer"
  - "No REFACTOR commit: the sibling's labeled closure and legend are not duplicated by the new builder"
metrics:
  duration: ~20 min
  completed: 2026-11-05
  tasks: 3
actuals:
  tokens: 3400
  tasks: 3
  commits: 2
plan_head_before: ed547431861d301764a1e814c9cc45db475c7b5e
plan_head_after: 92dee00
---

# Phase 20 Plan 01: Remove Confirmation Lines Summary

Pure renderer `remove_project_confirmation_lines` in `baude/src/ui.rs` that turns a `RemovalReport` into the `D` modal's lines, with scroll windowing for repository parents and a retry legend on error, covered by four output-asserting tests.

## What was built

`pub(crate) fn remove_project_confirmation_lines(report, kind_label, row_name, error, scroll, max_rows, width) -> Vec<String>` next to `remove_confirmation_lines`. Every section is read from one report field and nothing else (D-05):

- title `Remove this {kind_label}?`
- `rows dropped (n)` with `row_name(key)` per `rows_removed`, windowed by `scroll` and `max_rows`; `scroll` clamps to `len - max_rows` so the last row stays reachable, and the hidden remainder renders as `+N more` while the heading keeps the total (D-07, TUI-03)
- `runtimes killed (n)` naming each row with its agent pid and shell pid, `none` when empty
- `paths deleted (n)` from `paths_deleted` only, `paths kept (n)` from `paths_kept`, both `none` when empty
- `already removed (n)` only when non-empty
- one `cleared: ...` line summarising `cleared` by `ClearingKind` (bindings, markers, suppression entries) only when non-empty
- `error: {text}` when `error` is `Some`
- legend `enter retry · esc cancel` with an error, otherwise `y/enter remove · n/esc cancel` (D-06, D-09)

Every line passes through the existing `truncate(.., width)`. The `X` flow, `remove_confirmation_lines` and `lifecycle::RemovalConfirmation` are untouched (D-01).

Tests live in a `remove_project` helper module inside `ui::tests`: a `report()` builder over struct literals, `checkout()`/`repository()` key minting via `serde_json::from_value` (the key newtypes keep their `u64` private and `serde_json` is already a `baude` dependency), and a `section()` helper that returns the indented entries under a heading so assertions are on rendered output, not on source. Nothing spawns a process or touches disk.

## RED evidence

Task 1 added the four tests against a `todo!()` body and ran the plan's verify command:

```
cargo test -p baude ui::tests::remove_project_lines -- --test-threads=1 2>/tmp/baude-20-01-red.log
STATUS=101
grep -c 'error\[E' /tmp/baude-20-01-red.log  -> 0
test result: FAILED. 0 passed; 4 failed; 0 ignored; 0 measured; 256 filtered out
```

All four panicked at the `todo!()` (`not yet implemented`, `baude/src/ui.rs:2436:5`). `grep -c 'fn remove_project_lines_' baude/src/ui.rs` printed 4 (baseline 0). Committed as `006edb8`.

## Verification results

Exit codes captured directly from the chained commands, never through a pipe.

| Task | Command | Exit | Result |
|------|---------|------|--------|
| 1 RED | `cargo test -p baude ui::tests::remove_project_lines -- --test-threads=1`; `grep -c 'error\[E'`; `test $STATUS -ne 0` | 101 / 0 lines / gate ok | 0 passed, 4 failed |
| 2 GREEN | `cargo test -p baude ui::tests::remove_project_lines -- --test-threads=1 && cargo clippy -p baude --all-targets -- -D warnings` | 0 | 4 passed, 0 failed; clippy clean; `grep -c 'todo!()' baude/src/ui.rs` prints 0 |
| 3 REFACTOR | `cargo test -p baude ui::tests -- --test-threads=1 && cargo fmt --check && cargo clippy -p baude --all-targets -- -D warnings` | 0 | 35 passed, 0 failed (includes `remove_confirmation_names_exact_branch_and_path`); fmt diff empty; clippy clean |

Commit count measured from the plan ledger: `git rev-list --count ed54743..HEAD` = 2.

## TDD Gate Compliance

- RED: `006edb8`, tests compile and fail at runtime on `todo!()`, zero `error[E` lines.
- GREEN: `92dee00`, the four named tests pass, clippy clean under `-D warnings`.
- REFACTOR: no refactor needed. The sibling `remove_confirmation_lines` uses a `label: value` closure and a single fixed legend; the new builder uses count headings with indented entries and a two-way legend. Lifting a shared helper would not remove any duplicated code, and D-01 forbids touching the `X` flow's behaviour, so no commit was made.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] clippy `dead_code` and needless `mut` under `-D warnings`**
- **Found during:** Task 2 verify (first run exited 101 from clippy, tests already green)
- **Issue:** the renderer has no caller until plan 20-02 wires the `D` modal, so the non-test bin build failed `-D warnings` with `function is never used`; a closure was declared `let mut` without needing it.
- **Fix:** `#[cfg_attr(not(test), allow(dead_code))]` with a comment telling plan 20-02 to drop it; removed the `mut`.
- **Files modified:** baude/src/ui.rs
- **Commit:** 92dee00 (folded into the GREEN commit)

**2. [Note] Test keys minted through serde**
- `RepositoryKey`/`CheckoutKey` have no public constructor and no `From<u64>`. The plan's read_first left the choice to the executor; `serde_json::from_value` on the derived `Deserialize` is the public route that needs no repository graph and no disk.

No `Cargo.toml` change, no package installs (T-20-SC).

## Known Stubs

None. The renderer is fully wired to the report; its only pending consumer is plan 20-02.

## Self-Check: PASSED

- `baude/src/ui.rs` contains `pub(crate) fn remove_project_confirmation_lines` and the four `remove_project_lines_*` tests.
- Commits `006edb8` and `92dee00` are ancestors of HEAD on `gsd/phase-18-shared-kill-then-remove-lifecycle`.
