---
phase: 21-daemon-and-pwa-parity
fixed_at: 2026-11-14T02:05:17Z
review_path: .planning/phases/21-daemon-and-pwa-parity/21-REVIEW.md
iteration: 1
fix_scope: critical_warning
findings_in_scope: 6
fixed: 5
skipped: 1
status: partial
---

# Phase 21: Code Review Fix Report

**Fixed at:** 2026-11-14T02:05:17Z
**Source review:** .planning/phases/21-daemon-and-pwa-parity/21-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 6 (WR-01 to WR-06; the review has no Critical findings and the Info findings were out of scope)
- Fixed: 5 (WR-01, WR-02, WR-03, WR-04, WR-05)
- Skipped: 1 (WR-06, deferred: it needs more than the small change that was allowed)

Every finding was checked against the source before any edit. All five fixed findings held as written. Each fix has a test that was run red before the change and green after it. Work was done on `gsd/phase-18-shared-kill-then-remove-lifecycle` in the main checkout, with no worktree and no push.

## Fixed Issues

### WR-01: Retry button is disabled after a failed preview, so the controller's retry path is unreachable

**Status:** fixed
**Files modified:** `bauded/web/app.js`, `bauded/web/removal-controller.js`, `bauded/web/tests/removal-controller.test.js`
**Commit:** ace5c05
**Verified against source:** holds. The reviewer had not re-verified this one. The page disabled the action button whenever no report was held, and a failed preview holds none, so the button read retry and could not be pressed.
**Applied fix:** The button's label and disabled state now come from one function in the controller module, `confirmControl`, which keeps the button pressable in the error phase. `app.js` reads it instead of deciding for itself, so the rule is under test instead of living in untested page code.
**Tests:** `failed_preview_leaves_retry_pressable_and_retry_asks_for_the_preview_again`, `confirm_button_follows_the_phase_through_an_execute_and_its_retry`. Red before, green after.

### WR-02: Removal notice and clearing failures overwrite each other in a single toast

**Status:** fixed
**Files modified:** `bauded/web/removal-controller.js`, `bauded/web/tests/removal-controller.test.js`
**Commit:** e9f1f90
**Verified against source:** holds. The reviewer had not re-verified this one. `notify` is bound to a toast that holds one line, and the controller sent the notice and then one notification per clearing failure.
**Applied fix:** One notification per removal. The clearing failures are counted and joined after the notice, led in the way the TUI's `removal_notice` leads them in.
**Tests:** `removal_with_clearing_failures_shows_every_line_in_the_one_toast`, which models the one-line toast and asserts on what is left on screen. Red before, green after.

### WR-03: A standalone row is left recording a dead runtime when the release save fails

**Status:** fixed: requires human verification (state handling)
**Files modified:** `bauded/src/manager.rs`, `bauded/src/api.rs`
**Commit:** 6a282fa
**Verified against source:** holds. When the save that marks a standalone launch as running failed, the daemon stopped the agent and shell it had just started and returned the error, but the row went on recording them.
**Applied fix:** Once both stops are confirmed the row returns to Active with no owned runtime, the way a failed launch already does. A stop that cannot be confirmed keeps the record, so a later reopen can still end that runtime by its identity.
**Tests:** `standalone_open_whose_running_save_fails_leaves_no_recorded_runtime`, through the real router with a real folder and a real child. It reaches the third save through a new test-only window on the injected save failure, `fail_saves_after_for_test`. Red before, green after.
**For the human check:** the branch that keeps the record when a stop cannot be confirmed is reasoned from the code, not exercised by the new test.

### WR-04: A committed remote removal whose 200 body cannot be read is shown as a refusal and resent on Enter

**Status:** fixed: requires human verification (logic)
**Files modified:** `baude/src/remote.rs`, `baude/src/app.rs`
**Commit:** 13a134a
**Verified against source:** holds. A 200 to `POST /removals` whose body did not parse, or ran past the 1 MiB cap, became `RemovalReply::Refused { status: 200 }`, and the TUI kept the confirmation up with the same report behind Enter.
**Applied fix:** For an execute, an unusable 200 body is now the same as no answer: the worker reads `/rows` once, then the footer says the row was removed if the daemon no longer lists it, or the outcome is shown as unknown and Enter previews again. The execute is never resent. For a preview it stays a refusal, since a preview asked for nothing to change.
**Tests:** `ticket_execute_200_with_a_body_this_build_cannot_use_reconciles_through_rows` covers three shapes: a newer daemon's report with the row gone, a report past the cap with the row gone, and a newer daemon's report with the row still listed. `ticket_preview_200_with_a_body_this_build_cannot_use_is_a_refusal` pins the preview side. `remote_remove_committed_answer_from_a_newer_daemon_is_not_offered_as_a_retry` drives the whole TUI path. Red before, green after.
**Existing test changed:** `removal_oversize_json_body_is_refused` pinned the old refusal. It is now `removal_oversize_json_body_on_an_execute_200_is_an_unknown_outcome` and pins the new outcome, with the size cap assertions kept.

### WR-05: The TUI drops the daemon's `saved_degraded` outcome on a successful remote removal

**Status:** fixed
**Files modified:** `baude/src/app.rs`, `baude/src/remote.rs`
**Commit:** e2170d0
**Verified against source:** holds. `settle_remote_execute` closed a 200 with `removal_notice`, which never reads `report.persistence`.
**Applied fix:** The footer appends `; the daemon could not confirm its state save: <detail>` for a `SavedDegraded` report. Two departures from the suggested patch, both deliberate. The detail core produces is often a bare OS error, such as `Input/output error (os error 5)`, so it gets a lead-in that says what it is about. And the detail is another machine's text drawn in a terminal, so it passes through `sanitize_daemon_text`, which is now `pub(crate)` for that purpose.
**Tests:** `remote_remove_degraded_daemon_save_is_named_in_the_footer` compares a degraded save with a clean one. `remote_remove_degraded_daemon_save_detail_is_cleaned_for_the_footer` sends escape codes, newlines and 5000 characters. Red before, green after.

## Skipped Issues

### WR-06: `POST /removals` runs the whole kill-and-delete on an async worker while holding the global manager mutex

**File:** `bauded/src/api.rs:172-177`
**Status:** skipped: deferred, needs more than a small change
**Original issue:** `execute_removal` takes the `std::sync::Mutex` guard and calls `manager.remove_rows` synchronously on a tokio worker, for as long as it takes to stop a subtree's runtimes and delete its worktrees. Every other handler that calls `lock(&state)` parks its own worker on the same mutex for that time.
**Verified against source:** the code is as described. The hold time and its effect on pollers were not measured by the reviewer and were not measured here.
**Why it was not fixed:** the brief allowed this fix only if it was small and safe, with `spawn_blocking` as the example. Three things rule that out.

1. Measured: the suggested `spawn_blocking` change breaks the existing removal tests. The change was applied as a probe and three existing router tests were run against it: `removals_execute_drops_a_checkout_row_in_one_save`, `removals_execute_survives_a_fresh_restore` and `suppression_parent_removal_drops_the_child_entry`. All three failed with `config dir resolved to the real user path /Users/.../.config/baude during a test; hold a baude_core::testing::TestRedirect on this thread`, or the same for the managed worktree root. The fixture redirects in `baude-core/src/testing.rs` are thread-local on purpose, and `remove_rows` resolves the config dir and the worktrees base while it saves and clears. On a pool thread it has no redirect, and the containment guard stops it before it touches the real directories. The probe was rolled back with `git checkout -- bauded/src/api.rs` and nothing was committed. Making the hop safe means teaching the test harness to carry a redirect across threads, or resolving every path before the hop. Neither is a small change and both reach outside this finding.
2. There is no existing handler to copy. `spawn_blocking` and `block_in_place` do not appear anywhere in `bauded/src`.
3. The small change would not fix what the finding describes. It frees the one worker that runs the removal. Every reader still parks its own worker on the mutex for the whole removal, so a 3 s TUI poll still fails during a long removal, and enough pollers still park every worker. The reviewer says the same: removing that needs the kill and disk stages to run without the guard, with the repository reservation as the exclusion. That is a locking redesign.

**Deferred work:** run the kill and disk stages outside the manager mutex under the repository reservation, and decide how fixture redirects reach a thread the handler hops to. Worth measuring first, since the hold time and the poller effect are still reasoned from the code.

## Verification

All verification ran in the main checkout at `/Users/joese/Code/github.com/poindexter12/baude`, not in an isolated worktree, so the numbers below are reproducible from this tree. Every exit code was read directly from the command, never through a pipe.

Before each commit:

| Finding | `cargo fmt --all -- --check` | `cargo clippy --workspace --all-targets -- -D warnings` | Crate tests | `scripts/check-web-suites.sh` |
|---|---|---|---|---|
| WR-01 | 0 | 0 | `cargo test -p bauded`: 0, 176 passed | 0, 71 passed |
| WR-02 | 0 | 0 | `cargo test -p bauded`: 0, 176 passed | 0, 72 passed |
| WR-03 | 0 | 0 | `cargo test -p bauded`: 0, 177 passed | not needed |
| WR-04 | 0 | 0 | `cargo test -p baude`: 0, 348 passed | not needed |
| WR-05 | 0 | 0 | `cargo test -p baude`: 0, 350 passed | not needed |

The first full `cargo test -p baude` run for WR-04 exited 101 with one failure, the existing oversize test named above that pinned the old behaviour. It was updated as part of the fix and the rerun exited 0.

Gate, run once after the last fix:

| Command | Exit code | Where |
|---|---|---|
| `scripts/gate.sh` | 0 | main checkout, macOS |
| `scripts/gate.sh --linux` | 0 | main checkout mounted into `rust:1-bookworm` as uid 1000 |

`scripts/gate.sh` ended with `gate: all steps passed`. Test counts from that run, at HEAD `e2170d0`:

| Suite | Passed | Failed | Ignored |
|---|---|---|---|
| Web suites, `bauded/web/tests` | 72 | 0 | 0 |
| `baude` | 350 | 0 | 0 |
| `baude-core` | 560 | 0 | 1 |
| `bauded` | 177 | 0 | 0 |
| `vt100`, three integration files and one doc test | 33 | 0 | 0 |

The gate also ran one `baude` test on its own in an isolated step, 1 passed, and its closing check reported that the run left all four real roots untouched.

The Linux gate was run because WR-03 changes what the daemon does after stopping the agent and shell of a standalone launch, which is process code.

`scripts/gate.sh --linux` also ended with `gate: all steps passed`, with the same counts in every suite: web 72, `baude` 350, `baude-core` 560 with 1 ignored, `bauded` 177, `vt100` 33, and no failures.

Not verified here: nothing was pushed, so CI has not run on these commits. The two "requires human verification" fixes, WR-03 and WR-04, pass their tests, and the tests cannot show that the new state handling is the intended behaviour.

---

_Fixed: 2026-11-14T02:05:17Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
