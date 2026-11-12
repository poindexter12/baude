---
phase: 21-daemon-and-pwa-parity
plan: 08
subsystem: tui-remote-client
tags: [rust, ureq, tui, remote, removal, sse, events, tombstones, fixtures]

requires:
  - phase: 21-01
    provides: POST /removals/preview and POST /removals, the RemovalTarget and RemovalReport wire types
  - phase: 21-02
    provides: GET /rows, the only route that serves checkout_key
  - phase: 21-03
    provides: the status matrix, 404 gone, 409 fresh report, 500 and 503 text
  - phase: 21-04
    provides: GET /events, the rows_removed frame shape and the 15 s keep-alive
provides:
  - RemoteRows and its row types, an index of GET /rows joined on session_id
  - RemovalReply, RemovalAnswer, RemovalTicket and REMOVAL_TIMEOUT
  - RemotePoller preview_removal, execute_removal, spawn_preview_removal, spawn_execute_removal, rows, refresh_rows, stop and Drop
  - An events thread that applies rows_removed inside remote.rs, and tombstone arbitration against stale polls
  - Crate-visible Timeouts and the cfg test seams idle, idle_with and test_double
affects: [21-06, 21-11]

actuals:
  tokens: 32610
  tasks: 3
  commits: 6
plan_head_before: cbd7208de9bdedc839353ab8de09eeca53fe6b17
plan_head_after: 2eaafcccf15d306e36c784a8ba586f1ae8b8a8e8

tech-stack:
  added: []
  patterns:
    - "Every daemon double is a real local TCP listener routed by method and path, so a background poll can never consume a reply scripted for a removal"
    - "Arbitration between a removal and a poll is two pure functions over shared state, tested with explicit Instants and no socket"
    - "Production waits are injected through one Timeouts value, so a test waits milliseconds for what production waits 60 s or 35 s for"

key-files:
  created: []
  modified:
    - baude/src/remote.rs

key-decisions:
  - "A removal round trip runs under REMOVAL_TIMEOUT of 60 s on a worker thread; every other call stays blocking at 3 s"
  - "An execute that was sent and got no answer is Indeterminate, reconciled by one fresh GET /rows and never resent"
  - "A removed row is held out by a tombstone dated with Instant until a poll cycle that started after the removal has been applied; from then on the daemon's read wins"
  - "A preview 404 has no report in hand, so the worker drops the target's own row key and nothing else"
  - "The events reader holds one connection through timed-out reads; ureq 2.12.1 survived them on the first design"

requirements-completed: []
requirements-advanced: [API-01, API-02]

coverage:
  - deliverable: "Removal replies are typed from the status and the body is read every time"
    human_judgment: false
    verification:
      - kind: test
        ref: "baude/src/remote.rs#removal_preview_returns_the_report_the_daemon_sent"
        status: pass
      - kind: test
        ref: "baude/src/remote.rs#removal_execute_409_returns_the_fresh_report_as_drift"
        status: pass
      - kind: test
        ref: "baude/src/remote.rs#removal_404_is_gone_not_refused"
        status: pass
      - kind: test
        ref: "baude/src/remote.rs#removal_execute_503_carries_the_daemon_text"
        status: pass
  - deliverable: "A removal slower than the 3 s list timeout still returns its report, off the draw thread (HIGH 9)"
    human_judgment: false
    verification:
      - kind: test
        ref: "baude/src/remote.rs#ticket_execute_outlasting_the_request_timeout_still_returns_its_report"
        status: pass
      - kind: test
        ref: "baude/src/remote.rs#ticket_spawn_returns_before_the_daemon_answers"
        status: pass
  - deliverable: "An unanswered execute is Indeterminate, never Refused, and is sent exactly once"
    human_judgment: false
    verification:
      - kind: test
        ref: "baude/src/remote.rs#ticket_execute_timeout_is_indeterminate_and_reconciles_through_rows"
        status: pass
      - kind: test
        ref: "baude/src/remote.rs#removal_closed_without_an_answer_is_unreachable_for_preview_and_indeterminate_for_execute"
        status: pass
  - deliverable: "A poll fetched before a removal and applied after it does not bring the row back; a poll started after it does (HIGH 10)"
    human_judgment: false
    verification:
      - kind: test
        ref: "baude/src/remote.rs#snapshot_stale_poll_after_an_event_does_not_resurrect_the_row"
        status: pass
      - kind: test
        ref: "baude/src/remote.rs#snapshot_fresh_poll_prunes_the_tombstone_and_the_daemon_wins"
        status: pass
      - kind: test
        ref: "baude/src/remote.rs#events_drop_reaches_the_snapshot_before_a_slow_poll_lands"
        status: pass
  - deliverable: "Row names resolve from the rows index even with no live session (HIGH 6, TUI share)"
    human_judgment: false
    verification:
      - kind: test
        ref: "baude/src/remote.rs#removal_target_is_found_only_through_the_rows_join"
        status: pass
      - kind: test
        ref: "baude/src/remote.rs#ticket_answer_carries_rows_fetched_after_the_response"
        status: pass
  - deliverable: "The events connection survives timed-out reads, closes within 2 s of drop and is reopened after total silence"
    human_judgment: false
    verification:
      - kind: test
        ref: "baude/src/remote.rs#events_frame_split_by_a_pause_longer_than_the_slice_arrives_once"
        status: pass
      - kind: test
        ref: "baude/src/remote.rs#events_drop_closes_the_connection_within_two_seconds"
        status: pass
      - kind: test
        ref: "baude/src/remote.rs#events_total_silence_past_the_limit_reconnects"
        status: pass

metrics:
  duration: 96min
  completed: 2026-11-12
status: complete
---

# Phase 21 Plan 08: Remote removal client and events subscription Summary

The TUI's remote client now reads removal response bodies into a typed `RemovalReply`, runs removal
round trips on a worker thread under a 60 s timeout, and applies the daemon's `rows_removed` events
through tombstones so a poll fetched before a removal cannot bring the row back.

Nothing is wired to the TUI. `baude/src/remote.rs` is the only file changed and no dependency was
added. 21-06 wires `D`, the modal and the notices against the surface below.

## Commits

| Task | Step | Commit | Subject |
|---|---|---|---|
| 1 | RED | `dbb6904` | test(21-08): add failing remote removal client tests |
| 1 | GREEN | `a1e0bfd` | feat(21-08): read removal response bodies in the remote client |
| 2 | RED | `4a9c576` | test(21-08): add failing removal ticket and timeout tests |
| 2 | GREEN | `91c0e13` | feat(21-08): run remote removals off the draw thread under their own timeout |
| 3 | RED | `d955cf6` | test(21-08): add failing events and snapshot arbitration tests |
| 3 | GREEN | `2eaafcc` | feat(21-08): events subscription and tombstone-filtered snapshots |

## Public surface of `remote.rs` as shipped, for 21-06

Unchanged and frozen: `RemoteInfo` and `RemoteSnapshot` keep exactly their field sets;
`RemotePoller::start(base: String) -> RemotePoller`, `snapshot(&self) -> RemoteSnapshot`, `delete`,
`restart`, `create`, `set_archived`, and the public field `RemotePoller.base`. Those four session
calls still block for up to 3 s on the calling thread, as before.

| Item | As built | Blocks? |
|---|---|---|
| `pub const REMOVAL_TIMEOUT: Duration` | 60 s | n/a |
| `pub struct RemoteRows` | `repositories: Vec<RemoteRepositoryRow>`, `standalone: Vec<RemoteStandaloneRow>`; `Deserialize + Clone + Default + PartialEq + Debug` | n/a |
| `pub struct RemoteRepositoryRow` | `repository_key: RepositoryKey`, `display_name: String`, `path: String`, `health: serde_json::Value`, `checkouts: Vec<RemoteCheckoutRow>` | n/a |
| `pub struct RemoteCheckoutRow` | `checkout_key: CheckoutKey`, `repository_key: RepositoryKey`, `display_name: String`, `branch: Option<String>`, `path: String`, `role: serde_json::Value`, `managed_by_baude: bool`, `archived: bool`, `session_id: Option<u64>` | n/a |
| `pub struct RemoteStandaloneRow` | `standalone_key: StandaloneKey`, `display_name: String`, `path: String`, `archived: bool`, `session_id: Option<u64>` | n/a |
| `pub struct RemoteRowLabel` | `display_name: String`, `branch: Option<String>`, `path: String`, `session_id: Option<u64>`; `Clone + Debug + PartialEq + Eq` | n/a |
| `RemoteRows::target_for_session(&self, session_id: u64) -> Option<RemovalTarget>` | the checkout or standalone row that owns that live session; `None` means offer no remove | no |
| `RemoteRows::repository_target_for_session(&self, session_id: u64) -> Option<RemovalTarget>` | the parent repository of that session's checkout | no |
| `RemoteRows::label(&self, key: &RowKey) -> Option<RemoteRowLabel>` | resolves a key with or without a live session | no |
| `RemotePoller::rows(&self) -> RemoteRows` | tombstone-filtered clone of the polled index; a per-tick read | no, one mutex lock |
| `RemotePoller::refresh_rows(&self) -> Result<RemoteRows, String>` | one `GET /rows` | yes, up to 3 s; never on the draw thread |
| `RemotePoller::spawn_preview_removal(&self, target: RemovalTarget) -> RemovalTicket` | starts a worker and returns at once | no |
| `RemotePoller::spawn_execute_removal(&self, previewed: RemovalReport) -> RemovalTicket` | starts a worker and returns at once; the request is sent exactly once | no |
| `pub struct RemovalTicket` with `try_take(&self) -> Option<RemovalAnswer>` | `None` while pending, `Some` once, `None` after it was taken | no |
| `pub struct RemovalAnswer` | `reply: RemovalReply`, `rows: Option<RemoteRows>`; `rows` is `Some` after a 200 preview or a 409 when the refetch worked | n/a |
| `pub enum RemovalReply` | six variants, below | n/a |
| `RemotePoller::preview_removal(&self, target: &RemovalTarget) -> RemovalReply` | tests and the workers | yes, up to 60 s |
| `RemotePoller::execute_removal(&self, previewed: &RemovalReport) -> RemovalReply` | tests and the workers; does no rows bookkeeping, the ticket path does | yes, up to 60 s |
| `RemotePoller::stop(&self)` and `impl Drop for RemotePoller` | sets the stop flag for the poll and events threads; safe to call twice | no |
| `pub(crate) struct Timeouts` | `pub(crate)` fields `request`, `removal`, `events_slice`, `events_silence`, each a `Duration`; `Clone + Copy + Debug + Default`; the default is the production values | n/a |
| `#[cfg(test)] pub(crate) fn RemotePoller::idle(base: String) -> RemotePoller` | starts no thread | no |
| `#[cfg(test)] pub(crate) fn RemotePoller::idle_with(base: String, timeouts: Timeouts) -> RemotePoller` | `idle` with injected waits | no |
| `#[cfg(test)] pub(crate) mod test_double` | `serve(routes: Vec<Route>) -> Double`; `route(method: &str, path: &str, replies: Vec<Reply>) -> Route`; `Double::{base() -> String, request_count() -> usize, requests() -> Vec<Recorded>, unscripted() -> Vec<Recorded>, streams() -> Vec<Streamed>}`; `Recorded { method, path, content_type, body }`; `Streamed { opened: Instant, closed: Option<Instant> }` | n/a |
| `test_double::Reply` | `json(status: u16, body: impl Into<String>)`, `text(status: u16, body: impl Into<String>)`, `.delayed(delay: Duration)`, `closed_unanswered()`, `held(hold: Duration)`, `stream(chunks: Vec<(Duration, String)>)` | n/a |

`RemovalReply` variants:

| Variant | When | What 21-06 does |
|---|---|---|
| `Report(RemovalReport)` | 200 | preview: open the modal; execute: close and notice. The report may carry `persistence.outcome: saved_degraded` |
| `Drift(RemovalReport)` | 409 whose body parsed as a fresh report | re-install the modal from the fresh report |
| `Gone { message: String }` | 404 | close and say the row was already removed; never a retry. On the ticket path the rows are already dropped |
| `Refused { status: u16, message: String }` | any other status, a 409 or 200 whose body is not a report, an oversize body | keep the modal open, `Enter` retries. A 503 busy message still starts `repository busy:` |
| `Unreachable { message: String }` | a preview that got no answer; nothing was asked to change | footer notice |
| `Indeterminate { message: String, rows_gone: bool }` | an execute that was sent and got no answer | see the wording obligations below |

Every daemon-supplied `message` has had control characters removed, whitespace runs collapsed and
is capped at 512 characters. Report bodies are capped at 1 MiB, rows bodies at 4 MiB, error text at
4 KiB.

Events are applied inside `remote.rs`. 21-06 does nothing to consume `/events`: a dropped row stops
appearing in `snapshot()` and `rows()`.

## What 21-06 must know

- **Use the tickets, never the blocking pair, on the draw thread.** `spawn_*` returns at once;
  poll `try_take` on the draw tick. The ticket workers also do the bookkeeping: an execute 200, an
  execute 404 and a reconciled timeout drop the echoed rows from `snapshot()` and `rows()` before
  the answer is delivered. The blocking `execute_removal` does not.
- **Wording obligations for `Indeterminate`.** With `rows_gone: true` treat the removal as done.
  With `rows_gone: false` say the outcome is unknown and offer a fresh preview. Never say the
  removal failed and never resend the same execute: the request left the machine.
- **The comment at `baude/src/app.rs:890-893` is now untrue.** It says a detached `RemotePoller`
  worker outlives the handle. Dropping the poller now stops both threads: within 100 ms when
  asleep, within one 3 s request timeout mid-poll, within one 500 ms read slice on the events
  stream. In-flight removal workers still finish. 21-06 corrects the comment.
- **`RemotePoller` now implements `Drop`**, so no field can be moved out of one. Nothing in
  `app.rs` does today; the crate's test targets compile.
- **A request with no response, from a test outside `remote.rs`.** Either script the route with
  `Reply::closed_unanswered()` and use `RemotePoller::idle(double.base())`, with nothing to inject;
  or script it with `Reply::held(Duration::from_secs(1))` and build the poller with
  `RemotePoller::idle_with(double.base(), Timeouts { removal: Duration::from_millis(200), ..Timeouts::default() })`.
  A preview arrives as `Unreachable`, an execute as `Indeterminate`. With `held` the execute worker
  then reads `GET /rows` once, so script that route too.
- **`rows()` is empty until the first poll lands, and stays empty against a daemon without
  `GET /rows`.** `target_for_session` then returns `None`, which means no remove is offered.
- **`dead_code` allowances** sit on `REMOVAL_TIMEOUT`, the `RemoteRows` impl, `RemovalReply`,
  `RemovalAnswer`, `RemovalTicket`, `rows`, `refresh_rows`, the two blocking calls and the two
  `spawn_*` calls. 21-06 removes each one it wires.

## Recorded facts the plan asked for

- **Keep-alive and silence limit.** 21-04-SUMMARY records the keep-alive as the comment `:` and a
  blank line every 15 s, read from axum 0.8.9 source and not witnessed by a test. It is the named
  constant `EVENTS_KEEP_ALIVE`. `EVENTS_SILENCE` is twice that plus 5 s, which is 35 s.
  `EVENTS_SLICE`, one blocking read, is 500 ms.
- **Request bodies, confirmed against the 21-01 handlers.** Preview posts the `RemovalTarget` JSON
  to `/removals/preview`, read by `Json(target): Json<RemovalTarget>` at `bauded/src/api.rs:158`.
  Execute posts the previewed `RemovalReport` JSON, unchanged, to `/removals`, read by
  `Json(previewed): Json<RemovalReport>` at `bauded/src/api.rs:174`. Both send
  `Content-Type: application/json`.
- **The split-frame test passed on the first design.**
  `events_frame_split_by_a_pause_longer_than_the_slice_arrives_once` was green on the first run of
  the implementation: one connection, three timed-out reads between the two lines of a frame, one
  report, and the double recorded no close. The `ureq` 2.12.1 chunked body reader survives a
  timed-out read, so the plan's architectural stop did not trigger.
- **Poll order.** Each cycle takes its start `Instant`, then reads `/sessions`, `/rows` and `/info`
  in that order. A failed `/sessions` only sets `ok` to false. A failed `/rows`, an older daemon
  included, keeps the sessions and stores an empty index.

## How the timeout is tested without waiting 60 s

`Timeouts` carries the four waits and `RemotePoller` holds one. Production builds it from
`Timeouts::default()`. Tests pass a short `removal`, `events_slice` or `events_silence` by struct
update, through `idle_with` for the removal tests and through the private `start_with` for the
threaded events tests. `removal_timeout_exceeds_the_request_timeout` pins the production default at
60 s. The slow daemon is `Reply::held` or `Reply::delayed` on a real listener; the daemon that
closes mid-stream is `Reply::closed_unanswered`; the event stream is `Reply::stream`, sent chunked.
The longest single test is under 5 s and the whole `remote::tests` filter ran in 9 s.

## RED evidence

Each RED step was a compile failure of the test target, status 101, recorded before the production
edit. The logs are `/tmp/baude-21-08-t1-red.log`, `/tmp/baude-21-08-t2-red.log` and
`/tmp/baude-21-08-t3-red.log`, each with a `.status` file beside it.

Task 1, status 101:

```
error[E0433]: cannot find type `RemovalReply` in this scope
error[E0599]: no method named `execute_removal` found for struct `remote::RemotePoller` in the current scope
error[E0599]: no method named `preview_removal` found for struct `remote::RemotePoller` in the current scope
error[E0433]: cannot find type `RemoteRows` in this scope
error: could not compile `baude` (bin "baude" test) due to 35 previous errors
```

Task 2, status 101:

```
error[E0433]: cannot find type `Timeouts` in this scope
error[E0599]: no method named `spawn_execute_removal` found for struct `remote::RemotePoller` in the current scope
error[E0599]: no method named `rows` found for struct `remote::RemotePoller` in the current scope
error[E0425]: cannot find value `REMOVAL_TIMEOUT` in this scope
error[E0425]: cannot find type `RemovalTicket` in this scope
error: could not compile `baude` (bin "baude" test) due to 44 previous errors
```

Task 3, status 101 for the `snapshot_` filter and 101 for the `events_` filter:

```
error[E0599]: no method named `apply_poll` found for struct `remote::Shared` in the current scope
error[E0433]: cannot find type `SseParser` in this scope
error[E0599]: no associated function or constant named `start_with` found for struct `remote::RemotePoller` in the current scope
error[E0061]: this method takes 1 argument but 2 arguments were supplied
error[E0425]: cannot find function `fetch_poll` in this scope
error: could not compile `baude` (bin "baude" test) due to 27 previous errors
```

## Verification

Every status was captured in its own variable; none was read through a pipe.

| Gate | Exit | Result |
|---|---|---|
| `cargo fmt --all --check` | 0 | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | clean |
| `cargo test -p baude` | 0 | 320 passed, 0 failed. Baseline measured before the first edit: 276 passed. The 44 added are this plan's tests in `remote::tests` |
| `cargo test -p bauded` | 0 | 176 passed, 0 failed, on the first run. The known `failed to terminate exact PTY process group` flake did not appear |

Task gates, all exit 0: `remote::tests::removal_` and the Task 2 `ticket_` filter; `remote::tests::snapshot_`
6 passed; `remote::tests::events_` 11 passed; `remote::tests` single-threaded 47 passed in 9 s, the
three tests present at HEAD intact; `app::tests::sidebar_action_d` 2 passed; `cargo test -p baude --no-run`.
`git log --name-only --grep='(21-08):'` over `baude`, `bauded`, `baude-core`, `Cargo.toml` and
`Cargo.lock` names one file, `baude/src/remote.rs`.

No pre-existing test changed outcome and none was edited.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Task 1 RED tests revised once before the recorded RED run**
- **Found during:** Task 1
- **Issue:** The first draft of the tests called `RemotePoller::idle` inline. With `idle` missing
  the compiler could not type the poller, so the first compile failure named the missing types and
  `idle` and never the missing `preview_removal` and `execute_removal`.
- **Fix:** A test helper `idle_poller` spells out the `RemotePoller` return type, and the RED run
  was repeated. The recorded log and status are from the corrected tests; the earlier log is kept
  as `/tmp/baude-21-08-t1-red-first.log`. No production code existed yet.
- **Files modified:** baude/src/remote.rs, tests only
- **Commit:** `dbb6904`

### Choices the plan left open

- **`Timeouts.events_slice` and `events_silence` landed in Task 2** with `dead_code` allowances,
  because the Task 2 tests build `Timeouts` by struct update. Task 3 removed both allowances when
  the events thread started reading them.
- **`start_with(base, timeouts)` is a private seam.** `RemotePoller::start` delegates to it with
  the production values, and the threaded events tests call it with short waits. It is not part of
  the surface for 21-06.
- **A preview 404 drops the target's own row key** through a private `drop_rows`, since no report
  exists to hand to `apply_removed`. An execute 404 and a reconciled timeout pass the previewed
  report to `apply_removed`.
- **Silence is measured as time since the last byte**, not as a counter of timed-out reads. The
  effect is the one the plan describes.

### Known limits, not fixed here

- The events agent's 500 ms read timeout also covers the wait for the response head. A daemon
  slower than that to answer `GET /events` is retried every 3 s and the client falls back to the
  rows poll, which still converges.
- `scripts/gate.sh --linux` was not run. The plan assigns it to 21-11. The read-timeout branch
  matches both `WouldBlock` and `TimedOut`.

## Requirements

API-01 and API-02 are advanced, not completed, and stay pending in REQUIREMENTS.md: no task in this
plan marks one, and neither is user-reachable until 21-06 wires the TUI. API-03 is untouched.

## Known Stubs

None. The `dead_code` allowances above are finished code awaiting its caller in 21-06.

## Self-Check: PASSED

The six commits `dbb6904`, `a1e0bfd`, `4a9c576`, `91c0e13`, `d955cf6` and `2eaafcc` are ancestors of HEAD, and
`git rev-list --count` from the recorded base gives 6. `baude/src/remote.rs` and this file exist.
