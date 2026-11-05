---
phase: 20
plan: 03
status: complete
subsystem: tui
tags: [remove, modal, footer, help, render-tests]
requirements_addressed: [TUI-01, TUI-02]
requires: [20-01, 20-02]
provides:
  - "ConfirmRemove modal render tests at 100x30 and 59x20"
  - "D listed on every local footer arm and in the ? help overlay; remote footer pinned without D"
affects: [20-04]
commits:
  - sha: 9df1aaa
    subject: "test(20-03): prove ConfirmRemove modal renders at 80% height with scroll"
  - sha: 6d18557
    subject: "feat(20-03): render D confirmation modal and list D in footer and help"
commits_count: 2
plan_head_before: 0f1df14fbc54ce56bcfe9b85b6b64fd4b977b162
plan_head_after: 6d18557
files_modified:
  - baude/src/ui.rs
  - baude/src/app.rs
key-decisions:
  - "Compact (<80 col) footer arms list D as `D rm`; wide arms list `D remove*`"
  - "Help line for D trimmed to fit the 58-column overlay inner width"
actuals:
  tokens: 7000
  tasks: 2
  commits: 2
metrics:
  duration: "about 40 minutes wall clock, dominated by two full-crate test runs"
  completed: 2026-11-05
---

# Phase 20 Plan 03: Modal render and footer Summary

The `D` confirmation modal is proven on a TestBackend buffer at 80% height with scroll and the retry legend, and `D` is listed on every local row kind's footer and in the `?` help overlay while remote rows stay unchanged.

## What was built

**Task 1 (9df1aaa), `baude/src/ui.rs` tests only.** The `Modal::ConfirmRemove` draw arm from plan 20-02 needed no fix. Added:

- `confirm_remove_fixture(children, error)`: a repository with N managed worktrees installed through `install_hierarchy_state_for_test`, selected on the parent, in `Modal::ConfirmRemove` for `RemovalTarget::Repository` with a report listing the parent and every child.
- `modal_rows(&Buffer)`: the screen rows whose cells at the 76-wide centred modal's left and right border columns are both box glyphs, which measures the modal's height without counting sidebar or pane borders.
- `first_dropped_row(&str)`: the trimmed text under the `rows dropped (N)` heading, sliced to the modal's columns.
- `confirm_remove_modal_renders_report_lines_within_80_percent_height`: 20 children on 100x30 show the title, `Remove this repository?`, `rows dropped (21)`, at most 24 modal rows (23 measured), `+11 more`, and the first row equals `removal_row_label(&rows[0])`; after `scroll = 5` the first row equals `removal_row_label(&rows[5])`, the heading count is unchanged, `+6 more` appears and the modal height is unchanged.
- `confirm_remove_modal_renders_error_and_retry_legend`: `error: Some("state file is locked")` renders `error: state file is locked` and `enter retry · esc cancel`, not `y/enter remove`. The same app at 59x20 (the plan's narrow-width check, recorded as an assertion instead of a manual note) still shows the prompt, the legend and `+2 more` with the modal shrunk to 59 columns and capped at 16 rows.

**Task 2 (6d18557), `baude/src/ui.rs` and one test in `baude/src/app.rs`.**

- Wide footer arms (width >= 80) now carry `D remove*`: after `X remove*` where present, after `x close` where present, otherwise before `? help`. Covered: Repository, all three Unavailable capability arms, both Standalone arms, and the four `_` arms. 19 occurrences of `D remove*` in `ui.rs` (acceptance asked for at least 10).
- Compact arms (width < 80, the `? more` variants) carry `D rm` in the same slot. The shared `"i details · ? help"` arm is now `"i details · D remove* · ? help"` for both widths since it fits.
- Remote arm `"enter attach · r restart · x close · ? help"` untouched; `action_status_hint` table test now builds `ActionSelection::Remote` and asserts the hint equals that literal and contains no `D remove` at both 160 and 59 columns.
- Help overlay: two lines after the `X` pair, `  D removes row, kills runtime, deletes managed worktrees` and `              repository parents remove every child`; rect height 43 to 45 and its sync comment updated; `rendered.contains("D removes")` asserted beside the `X removes` assertion.
- Every exact-string footer test updated to the new literals.

## Verification results

| Command | Exit |
|---|---|
| `cargo test -p baude ui::tests::confirm_remove_modal_renders -- --test-threads=1` | 0 (2 passed) |
| `cargo test -p baude ui::tests -- --test-threads=1` | 0 (37 passed) |
| `cargo fmt --check` | 0 |
| `cargo clippy -p baude --all-targets -- -D warnings` | 0 |
| `cargo test -p baude` (full crate, before the two fixes below) | 101: 268 passed, 2 failed |
| `cargo test -p baude status_bar_counts_use_codes -- --test-threads=1` (after fix) | 0 (1 passed) |

Exits were captured from `${pipestatus[1]}` on the cargo process, not from the trailing `grep`/`tail`.

## Deviations

1. **[Rule 1 - Bug] Compact footer `D remove*` overflowed the 40-column bar.** The plan prescribed `D remove*` in the compact `? more` variants too. The status bar budgets `width - 1` cells for the hint; at 40 columns that is 39, and `enter reopen · X remove · D remove* · ? more` is 42, which clipped the `? more` tail and failed `hierarchy_viewport_matrix_renders_without_panic_and_preserves_semantics` at 40x12. The compact bar already abbreviates (`X remove` not `X remove*`, `r reopen` not `r recheck and reopen`), so compact arms use `D rm` (longest result 39 cells). Wide arms keep `D remove*`. Commit 6d18557.
2. **[Rule 1 - Bug] Plan's first help line did not fit the overlay.** `  D removes the row, kills its runtime, deletes managed worktrees` is 65 cells; the help overlay is 60 wide with a 58-cell interior, so the tail would have been silently clipped. Trimmed to `  D removes row, kills runtime, deletes managed worktrees` (57 cells). The second line is the plan's text verbatim. Commit 6d18557.
3. **[Rule 3 - Blocking] `app::tests::status_bar_counts_use_codes` lost its right-side chips.** Its 220-column backend fit the old 63-cell hint plus the fixture's 110-cell tmp path plus the counter chips with two cells to spare; the 75-cell hint pushed the chips off the bar, which is the bar's designed overflow order. The test asserts counter codes, not width, so the backend is 260 columns with a comment. Commit 6d18557.
4. **Remote negative placed in the table test, not at the plan's line 3158.** The line the plan cited is the standalone plain-folder test (`!sidebar.contains("X remove")`), which is a local row and must list D. There was no remote footer test in `ui::tests`, so the negative was added to the `action_status_hint` table test with a `ActionSelection::Remote` view.
5. Task 1 had a dedicated `test(20-03)` commit; the plan prescribed only the Task 2 subject, which was used verbatim.

No package installs; `Cargo.toml` untouched.

## Known Stubs

None.

## Threat Flags

None. Footer and help strings name keys only; the modal still draws only lines from `remove_project_confirmation_lines` over the report.

## Handoff notes for plan 20-04

- Footer literals to reuse in app-side tests: wide managed retained `enter reopen · X remove* · D remove* · e edit · i info · ? help`; wide live managed `enter attach · x close · X remove* · D remove* · t shell · e edit · a archive · ? help`; compact managed `enter reopen · X remove · D rm · ? more`; standalone retained wide `enter reopen · e edit · i info · g gsd · D remove* · ? help`; remote unchanged.
- Help overlay height is now 45 rows; adding rows means bumping `centered(area, 60, 45)` and its comment.
- `ui::tests::confirm_remove_fixture(children, error)`, `modal_rows`, and `first_dropped_row` are reusable for any further modal render coverage. `modal_rows` assumes a screen at least 76 columns wide.
- The post-removal footer notice wording from `App::removal_notice` was not touched by this plan; if 20-04 pins it, it is still owned by `app.rs`.

## Self-Check: PASSED

- `baude/src/ui.rs` and `baude/src/app.rs` present and modified.
- Commits `9df1aaa` and `6d18557` are ancestors of HEAD on `gsd/phase-18-shared-kill-then-remove-lifecycle`.
- `commits: 2` measured by `git rev-list --count 0f1df14..HEAD` before the docs commit.
