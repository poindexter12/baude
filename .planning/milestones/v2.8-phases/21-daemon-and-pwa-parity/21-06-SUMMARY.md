---
phase: 21-daemon-and-pwa-parity
plan: 06
subsystem: tui
tags: [rust, ratatui, remote-removal, daemon-client, error-text]
requires:
  - phase: 21-08
    provides: remote removal client in baude/src/remote.rs, rows index, removal tickets, test double
provides:
  - D on a remote row previews and removes through the daemon, off the draw thread
  - one tested outcome per daemon answer and for no answer
  - removal error text shows row labels instead of Rust key tokens, local and remote
affects: [21-11]
tech-stack:
  added: []
  patterns:
    - RemovalOrigin beside the Phase 20 confirmation state, Local or Remote
    - worker ticket polled from tick, never a blocking HTTP call on the key path
    - key text replaced at draw time in one left to right pass, stored error left as received
key-files:
  created: []
  modified:
    - baude/src/app.rs
    - baude/src/ui.rs
    - .planning/REQUIREMENTS.md
key-decisions:
  - "API-01 and API-02 recorded as advanced, not complete: 21-11 still carries both and owns the repository gate and the live pairing check, matching what 21-08 recorded"
  - "The plan's per-name grep cannot pass on a green run, so named tests were checked with a script that accepts a bare ok line after fixture output"
  - "removal_text_for_user substitutes at draw time only; the stored error keeps the daemon's text"
requirements-completed: []
requirements-advanced: [API-01, API-02]
metrics:
  duration: "commit span 2026-11-12T10:04-08:00 to 2026-11-13T08:00-08:00 across several sessions; active time not measured"
  completed: 2026-11-13
status: complete
actuals:
  tokens: 28242
  tasks: 3
  commits: 7
commits: 7
plan_head_before: 78ce5ecffbba5f8bd67efead308e5534b0442dd9
plan_head_after: 265572ff6f35880f024855161c807c5d11dbffe9
---

# Phase 21 Plan 06: Remote removal in the TUI Summary

`D` on a remote row now previews and removes through that row's daemon on a worker ticket, with a
tested outcome for 200, 409, 404, 500, 503 and silence, and removal error text that names rows by
label instead of by Rust key token.

## Commits

| Task | Commit | Message |
|------|--------|---------|
| 1 RED | f73f499 | test(21-06): D on a remote row yields the remove action |
| 1 GREEN | 87edb79 | feat(21-06): remove a remote row from the TUI through the daemon |
| 2 RED | 2157639 | test(21-06): pin every daemon answer to a remote removal |
| 2 GREEN | 3b22364 | feat(21-06): handle drift, refusal, gone and no answer for remote removal |
| 3 docs | 666df1f | docs(21-06): narrow the stale out of scope row on remote removal |
| 3 RED | d6fd8d8 | test(21-06): pin the removal error text register |
| 3 GREEN | 265572f | feat(21-06): show row labels in removal error text |

Count measured with `git rev-list --count 78ce5ec..HEAD` at `265572f`: 7.

## Verification, as measured

| Check | Result |
|-------|--------|
| Whole crate, `cargo test -p baude -- --test-threads=1` at `265572f` | exit 0, 345 passed, 0 failed |
| All 46 distinct tests named across the three task verifies, against that whole-crate log | every one ok |
| Task 2 filter run | cargo exit 0, 36 passed, 0 failed |
| Task 3 filter run | cargo exit 0, 45 passed, 0 failed; the nine new tests rerun after the final rustfmt reflow, 9 passed |
| `cargo fmt --check` | exit 0 |
| `cargo clippy -p baude --all-targets -- -D warnings` | exit 0 |
| REQUIREMENTS.md gate | `stale=0 corrected=1` |
| `git diff --name-only 78ce5ec..HEAD` | exactly `baude/src/app.rs`, `baude/src/ui.rs`, `.planning/REQUIREMENTS.md` |

`baude/src/remote.rs`, `baude-core/` and `bauded/` are unchanged and no dependency was added.

Not run here, by the plan's own assignment: `scripts/gate.sh`, `scripts/gate.sh --linux` and the
live pairing of a real TUI with a real `bauded`. All three belong to 21-11.

## Deviations from Plan

### 1. The per-name check in the Task 2 and Task 3 verify commands cannot pass as written

- **Found during:** Task 2 verify.
- **Issue:** the plan checks each named test with `grep -q "::tests::$n \.\.\. ok"`. Under
  `--test-threads=1`, a fixture that runs `git` writes to the real stdout between the test name and
  its outcome, so `ok` lands on a later line by itself. On fully green runs the grep reported no ok
  line for 16 of 19 names in Task 2 and 15 of 30 in Task 3. The `<automated>` commands as written
  therefore exit non-zero even when cargo exits 0 with nothing failing.
- **What was done instead:** a small script accepts either `... ok` on the test line or a bare `ok`
  line before the next `test ` line. It was checked against a log holding a real failure and
  against a name that does not exist, and reported both. With it, both task runs and the
  whole-crate log pass for every named test.
- **Not changed:** the plan file. The Task 1 verify has the same grep and 21-11 may too, so the
  planner should know before the next verify of this shape.

### 2. [Rule 1 - Bug] The deadline test raced its own two workers

- **Found during:** Task 2 verify. `remote_remove_silent_worker_resolves_at_the_deadline` failed
  once with `the remote removal never settled`.
- **Fix:** the test now waits for its first preview to reach the double before sending the second.
  Test only. Run alone 15 times afterwards: 15 passed.
- **Commit:** 3b22364

### 3. [Rule 1 - Bug] The unknown outcome line lost its heading on a narrow modal

- **Found during:** Task 2, `remote_removal_modal_unknown_outcome_never_says_failed`.
- **Issue:** the shared truncation cut the line from the left, so the text the test pins was gone.
- **Fix:** that line keeps its heading whole. `baude/src/ui.rs`.
- **Commit:** 3b22364

### 4. RED tests that passed on arrival

- Task 2: four of fourteen, as recorded in `2157639`. They pin behaviour Task 1 already had.
- Task 3: one of nine, `remote_removal_modal_names_every_row_from_the_daemon_index`. It was checked
  by mutation: with `RemoteConfirm::row_label` limited to rows that have a session, it fails. The
  mutation was reverted before the RED commit.

### 5. Task 3 commit order

The plan lists RED, GREEN, docs. The docs commit landed first because it is independent of the
code. Three commits, as planned.

### 6. Requirements not marked complete

The plan frontmatter lists API-01 and API-02. Both are also carried by 21-11, which owns the gate
and the live check, and 21-08 recorded them as advanced only. `requirements.mark-complete` was not
run for this plan.

## Open point for the verifier

The success criterion says the eighteen Phase 20 tests pass with their assertions unchanged. All
eighteen pass. The diff from the plan base removes six lines containing `assert`, all inside
`mod tests`. Three are the planned rewrite of the remote hint assertions in the copy contract
test. The other three are opening lines of multi-line asserts that were not attributed to a named
test in the final session; the two planned Task 1 test rewrites are the likely source, but that is
an inference, not a measurement. `git diff 78ce5ec..265572f -- baude/src` settles it.

## What the plan asked this summary to record

- **`<consumed_api>` as built in 21-08:** no difference found. Checked by signature in
  `baude/src/remote.rs`: `rows`, `target_for_session`, `label` returning `RemoteRowLabel` with
  `display_name`, `branch`, `path`, `session_id`, `spawn_preview_removal`, `spawn_execute_removal`,
  `try_take`, `RemovalAnswer { reply, rows }`, the six `RemovalReply` variants, `REMOVAL_TIMEOUT` at
  60 s, `idle`, `idle_with`, `Reply::closed_unanswered` and `Reply::held`. The as-built table in
  `21-08-SUMMARY.md` was not reread in the final session.
- **How the no-response case was produced:** `Reply::closed_unanswered()` at three sites and
  `Reply::held(..)` at two in `app::tests`. `idle_with` is not used, so no test injects a short
  removal timeout; the deadline test sets the pending removal's deadline directly.
- **Does `remove_project_modal_max_rows` depend on the error:** it takes `error: Option<&str>` and
  counts the lines the error adds. The draw arm now passes the substituted text, so the budget is
  computed from what is drawn. Whether the length of the text, as opposed to its presence, changes
  the count was not established.
- **User-facing strings, read from the code at `265572f`:**
  - 404: `“{label}” was already removed on the daemon.`
  - Execute sent, no answer, row still listed: `the daemon did not answer; “{label}” may or may not
    have been removed`, with the legend `enter preview again · esc close`
  - Esc during an execute wait: `The removal of “{label}” was already sent and cannot be recalled.
    Waiting for the daemon.`
  - Pending modal while an execute is out: `The removal was sent to the daemon and cannot be
    recalled.`
  - A key nothing names: `unknown row`
  - The 200 notice, the preview refusal footer, the footer for a row with no daemon key and the
    notice for an unanswered execute whose row is gone are pinned by their named tests and were not
    transcribed here.
- **Anchors that had moved:** line anchors in the plan were located by grep on the symbol, as the
  plan's reading discipline directs. Individual offsets were not recorded.
- **PWA share of rule 2 of the error-text register:** it is 21-05 Task 2's `userText` in
  `removal-controller.js`, not this plan's. This plan covers both TUI origins only.

## Threat model

Every `mitigate` row from T-21-80 to T-21-88 names tests that are in the 46 checked above. T-21-85
was completed in Task 3: daemon row names lose control characters and are cut at 80 characters.
T-21-89 stays accepted. No new listener, endpoint or file access outside the plan's threat model.

## Known Stubs

None found in the plan's diff.

## Deferred, out of scope

- `.planning/phases/21-daemon-and-pwa-parity/.review-diagnostics/` is untracked and predates this
  plan. Left alone.
- clippy prints a manifest note for the vendored `vt100` about `package.homepage`. Not this plan's.

## Self-Check: PASSED

The three modified files and this summary exist, and all seven commits are ancestors of HEAD.
