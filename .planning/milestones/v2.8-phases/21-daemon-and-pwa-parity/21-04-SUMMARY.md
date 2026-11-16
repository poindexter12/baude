---
phase: 21-daemon-and-pwa-parity
plan: 04
subsystem: api
tags: [rust, axum, daemon, sse, events, broadcast, removal, fixtures]

requires:
  - phase: 21-01
    provides: POST /removals/preview and POST /removals, Manager::remove_rows, the removal fixtures
  - phase: 21-02
    provides: GET /rows, the only route that serves checkout_key
  - phase: 21-10
    provides: standalone rows with runtime custody, folder_fixture
  - phase: 21-03
    provides: the single committed return of remove_rows, the status matrix seams, assert_refused, assert_drift, assert_executed
provides:
  - GET /events, a daemon-wide SSE stream carrying one rows_removed message per committed removal
  - DaemonEvent, EVENTS_CAPACITY, Manager::subscribe_events and the single publish call in Manager::remove_rows
  - Test helpers open_events, next_sse_frame, expect_rows_removed, expect_silence, expect_exactly_one, rows_lists
  - Test seam publish_for_test
  - A characterization test for the unknown-session 404 of GET /sessions/{id}/stream
affects: [21-05, 21-08, 21-11]

actuals:
  tokens: 7869
  tasks: 3
  commits: 4
plan_head_before: 7857f6d06c1d4d3126a1b37e66919659f885142a
plan_head_after: c26d0ea2a5027ee434d7dd1dac23ac9f83b6138a

tech-stack:
  added: []
  patterns:
    - "Every events assertion reads a frame, or the absence of one, off the real response body of GET /events under a deadline; no test takes its own receiver"
    - "A refusal is proven silent on a stream that is then proven alive: each failing case ends in a 200 that delivers exactly one message on the same body"
    - "A green test is shown able to fail by a mutation that is run, logged and reverted before the commit"

key-files:
  created: []
  modified:
    - bauded/Cargo.toml
    - bauded/src/manager.rs
    - bauded/src/api.rs

key-decisions:
  - "The publish sits on the one Ok return of Manager::remove_rows, after core's durable save and the registry cleanup, so saved and saved_degraded both publish and no Err path can"
  - "A lagging subscriber silently skips the oldest events and keeps its stream; capacity is 64 and no lag notice is sent"
  - "An event whose report cannot be serialized is skipped, not sent with empty data; the subscriber's poll of GET /rows covers the gap"
  - "next_sse_frame panics when the body ends: the daemon ends this stream only at shutdown, so an end inside a test is never an empty read"

requirements-completed: []
requirements-advanced: [API-02]

coverage:
  - deliverable: "One checkout removal over REST reaches an open subscriber as one rows_removed message equal to the 200 body, and the row is gone from /rows and from a fresh restore"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#events_removal_over_rest_reaches_a_subscriber"
        status: pass
  - deliverable: "Two subscribers each read the one message; a late subscriber reads none"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#events_two_subscribers_each_receive_one_message"
        status: pass
      - kind: test
        ref: "bauded/src/api.rs#events_no_history_for_a_late_subscriber"
        status: pass
  - deliverable: "A standalone row and a repository parent with two children each publish one message"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#events_cover_standalone_and_repository_rows"
        status: pass
  - deliverable: "404, 409, 500 and the three driven 503 causes publish nothing; each retryable one then publishes exactly one"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#events_silent_on_drift_409"
        status: pass
      - kind: test
        ref: "bauded/src/api.rs#events_silent_on_runtime_survived_500"
        status: pass
      - kind: test
        ref: "bauded/src/api.rs#events_silent_on_persistence_503"
        status: pass
      - kind: test
        ref: "bauded/src/api.rs#events_silent_on_404_and_pre_kill_503"
        status: pass
  - deliverable: "A saved_degraded commit publishes exactly one message carrying that outcome"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#events_saved_degraded_publishes_exactly_one"
        status: pass
  - deliverable: "A subscriber more than 64 events behind keeps its stream and still reads a later real removal"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#events_route_survives_lag"
        status: pass
  - deliverable: "The two per-session streams answer as before"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#stream_guards_unknown_session"
        status: pass
      - kind: test
        ref: "bauded/src/api.rs#activity_stream_guards_known_and_unknown"
        status: pass

metrics:
  duration: 17min
  completed: 2026-11-12
status: complete
---

# Phase 21 Plan 04: Daemon-wide events stream Summary

`GET /events` now serves one named `rows_removed` SSE message per committed `POST /removals`, fed
by a 64-slot broadcast channel the manager owns and published from the single `Ok` return of
`Manager::remove_rows`, with ten tests that read every frame off the real response body.

## What was built

- `bauded/Cargo.toml`: `sync` added to the tokio feature list. One changed line. `Cargo.lock` has
  no diff against the base `7857f6d`.
- `bauded/src/manager.rs`: `pub const EVENTS_CAPACITY: usize = 64`, `pub enum DaemonEvent` with
  the one variant `RowsRemoved(RemovalReport)` and its `name()` and `data()`, the `events` field,
  `Manager::subscribe_events`, the private `Manager::publish`, the `cfg(test)`
  `publish_for_test`, and one `publish` call in `remove_rows`.
- `bauded/src/api.rs`: `async fn events`, the route `/events` beside `/info`, and the tests.
  `async fn stream` and `async fn activity_stream` have no changed line.

`Manager::remove_rows` had exactly one committed return path, as 21-03 recorded. Nothing had to be
fixed: all six matrix tests were green on their first run, so no `fix` commit exists.

## The wire contract as served, for 21-05 and 21-08

| Item | Value |
|---|---|
| Route | `GET /events`, no parameters, 200, `content-type: text/event-stream` |
| Event name | `rows_removed`, exactly. It is a named event: a browser needs `addEventListener("rows_removed", ...)`, and `onmessage` never fires for it |
| Framing, as observed | `event: rows_removed\n` then `data: <json>\n` then a blank line. The `event:` line comes first. One `data:` line. No `id:` line and no `retry:` line |
| Data | The `RemovalReport` JSON from the same serde derive as the `POST /removals` 200 body. The tests compare the two as parsed JSON and find them equal; byte equality is not asserted |
| Cardinality | One message per committed removal, `saved` or `saved_degraded`, whatever the row kind. A repository removal lists its children and the parent in one message. None for a preview or for 404, 409, 500, 503 |
| Publisher | Only `POST /removals`. `DELETE /sessions/{id}` and every other route publish nothing |
| Keep-alive | `KeepAlive::default()`. In axum 0.8.9 `KeepAlive::new` sets `max_interval: Duration::from_secs(15)` and `event: Event::DEFAULT_KEEP_ALIVE`, whose bytes are `:\n\n`. So the keep-alive is a comment frame with empty text, the three bytes `:`, newline, newline, sent after at most 15 s with no other frame. Not witnessed by a test, read from the pinned source. A client read timeout must exceed 15 s |
| Versions resolved | axum `0.8.9`, tokio `1.52.3`, from `Cargo.lock` |
| History | None. A subscriber sees only removals that commit after its `GET /events` returned its response headers |

**What a client sees when it lags.** Nothing. A subscriber more than 64 events behind loses the
oldest ones without any notice frame, the stream stays open, and delivery continues in order from
the oldest retained event. In the lag test 80 flood messages were followed by one real removal; the
reader got a strictly increasing suffix that did not start at 0 and ended at 79, then the real
message. A client cannot detect the loss from the stream, so it must keep polling `GET /rows`.

**What a client sees when the daemon restarts.** The stream ends: the sender is dropped with the
manager and the handler's loop breaks on `RecvError::Closed`, or the connection simply closes with
the process. This was read from the code and is not witnessed by a test. A reconnect opens a fresh
subscription with no replay. `Last-Event-ID` is ignored because no `id:` is ever sent, and no
`retry:` is sent, so a browser `EventSource` reconnects on its own default delay. Removals that
committed during the gap are never announced: reconcile with `GET /rows` after every reconnect.

No ordering is promised between the message and the `POST /removals` response reaching the same
client. The initiating client may see its own event first.

### One real frame, bytes as read

Read off the response body in `events_removal_over_rest_reaches_a_subscriber`, shown as a Rust
debug string so the newlines are visible:

```
"event: rows_removed\ndata: {\"target\":{\"checkout\":1},\"planned\":false,\"rows_removed\":[{\"checkout\":1}],\"runtimes_killed\":[{\"row\":{\"checkout\":1},\"agent_pid\":91134,\"shell_pid\":null}],\"paths_deleted\":[],\"paths_kept\":[\"/private/var/folders/q1/335pbzl13sz825k87q4ywl300000gq/T/bauded-removal-events-tracer-90069/repo\"],\"already_removed\":[],\"readers_parked\":[],\"persistence\":{\"outcome\":\"saved\"},\"cleared\":[{\"kind\":\"suppression_added\",\"path\":\"/private/var/folders/q1/335pbzl13sz825k87q4ywl300000gq/T/bauded-removal-events-tracer-90069/repo\"}],\"clearing_failures\":[]}\n\n"
```

The matching 200 body is the JSON on the `data:` line: the test asserts the two parse equal.

```json
{"target":{"checkout":1},"planned":false,"rows_removed":[{"checkout":1}],"runtimes_killed":[{"row":{"checkout":1},"agent_pid":91134,"shell_pid":null}],"paths_deleted":[],"paths_kept":["<root>/repo"],"already_removed":[],"readers_parked":[],"persistence":{"outcome":"saved"},"cleared":[{"kind":"suppression_added","path":"<root>/repo"}],"clearing_failures":[]}
```

The message carries no names and no `checkout_key` on a session. A client resolves a key to a
name, a branch or a session id through `GET /rows`, and must do it before it drops the row from
its own list, since the row is already gone from `/rows` when the message can be read.

## RED and mutant evidence

**Tracer RED**, `/tmp/baude-21-04-t1-red.log`, exit status 101, taken before any production edit:

```
thread 'api::tests::events_removal_over_rest_reaches_a_subscriber' panicked at bauded/src/api.rs:6081:9:
assertion `left == right` failed: 21-04 events pin: GET /events must open a stream
  left: 404
 right: 200
```

**Early-send mutant**, `/tmp/baude-21-04-t2-mutant.log`, exit status 101. The `publish` call was
moved above the `removal::remove` call, sending the fresh preview, then reverted with
`git checkout -- bauded/src/manager.rs`:

```
thread 'api::tests::events_silent_on_persistence_503' panicked at bauded/src/api.rs:6303:9:
21-04 early-send pin: a 503 removed nothing: a message was published where none belongs: Some(SseFrame { event: "rows_removed", data: "{\"target\":{\"checkout\":2},\"planned\":true, ...
```

**Lag mutant**, `/tmp/baude-21-04-t3-mutant.log`, exit status 101. The `Lagged` arm was changed to
`break`, then the file was restored and compared byte for byte with the copy taken before:

```
thread 'api::tests::events_route_survives_lag' panicked at bauded/src/api.rs:6166:29:
21-04 events pin: the events stream ended
```

It failed on end of stream, which the plan accepts in place of the `21-04 lag pin` marker.

## Verification

| Check | Result |
|---|---|
| `cargo test -p bauded api::tests::events_ -- --test-threads=1` | exit 0, ten `events_` tests ok |
| `stream_guards_unknown_session`, passing at HEAD before the production edit and after | exit 0, 1 passed |
| `activity_stream_guards_known_and_unknown`, unedited | exit 0, 1 passed |
| `cargo fmt --all --check` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo test -p bauded -- --test-threads=1` | exit 0, 174 passed, 0 failed |
| `matrix_` tests ok | 14 |
| `removals_`, `row_kinds_`, `standalone_` tests ok, Task 2 crate run | 7, 10, 11 |
| `Cargo.lock` against the base | no diff |
| `bauded/web`, `baude/src`, `baude-core/src` against the base | no diff |

The whole suite ran twice, after Task 2 with 173 passed and after Task 3 with 174 passed. The open
macOS flake on `DELETE /sessions/{id}` did not appear in either run.

## Reusable test helpers

In the `tests` module of `bauded/src/api.rs`:

```rust
struct EventsBody { body: Body, pending: Vec<u8> }
struct SseFrame { event: String, data: String, raw: String }

async fn open_events(app: &Router) -> EventsBody
async fn next_sse_frame(events: &mut EventsBody, within: Duration) -> Option<SseFrame>
async fn expect_rows_removed(events: &mut EventsBody, marker: &str) -> serde_json::Value
async fn expect_silence(events: &mut EventsBody, within: Duration, marker: &str)
async fn expect_exactly_one(events: &mut EventsBody, executed: &serde_json::Value, marker: &str) -> serde_json::Value
fn rows_lists(rows: &serde_json::Value, row: &serde_json::Value) -> bool
```

`next_sse_frame` returns `None` on timeout, skips a frame made only of comment lines, and panics
if the body ends. `expect_rows_removed` waits 5 s and pins the event name, the single data line
and the framing. `rows_lists` takes a row in the report's own shape, such as `{"checkout": 1}`.

## RESEARCH overrides, restated

1. RESEARCH Pattern 2, `futures_util::stream::unfold`, was not used. `futures-util` is a
   dev-dependency. `/events` follows the shipped convention of the two existing SSE handlers:
   `async_stream::stream!` with `Sse::new(..).keep_alive(KeepAlive::default())`. `tokio-stream` was
   not added.
2. Assumption A1, tokio `sync`. `broadcast` already compiled through feature unification from
   hyper. `sync` was added to `bauded/Cargo.toml` anyway, as a declaration of a feature this crate
   uses directly, not as a compile fix. `Cargo.lock` did not change.
3. RESEARCH's rule "send after `save_checked` returns `Ok`" is narrowed to the `Ok` arm of
   `baude_core::removal::remove`, which includes `SavedDegraded`. D-07's "persist stage succeeds"
   is read as "the replacement committed".
4. Capacity 64 and silent skip-on-lag are discretion, not a locked decision.

## Deviations from Plan

None of the deviation rules fired: no bug, no missing critical functionality, no blocker. These
are the places the code differs from the plan's wording.

- `open_events` returns an `EventsBody` holding the body and the bytes not yet consumed, not the
  bare body. A body chunk that carries more than one message would otherwise lose the second.
- `next_sse_frame` panics on end of stream. Returning `None` there would let every silence
  assertion pass on a stream the route had closed.
- `events` skips an event whose report fails to serialize. The plan's sketch, and the two older
  handlers, would send empty data; an empty `rows_removed` message is not a report.
- `publish_for_test` is `pub(crate)`, as the neighbouring seams are, not `pub`.
- `events_no_history_for_a_late_subscriber` also holds an earlier subscriber that reads the
  message, so the late silence is the absence of replay and not of publishing.
- `events_silent_on_persistence_503` also replays the stale body, gets the 409 and asserts silence
  after it, before the fresh retry.
- `events_route_survives_lag` also asserts the newest flood message, number 79, is retained.
- The crate gate ran clippy over the workspace and `cargo fmt --all --check`, as the orchestrator
  asked, which is wider than the plan's `-p bauded`.

## Flagged assumptions, still open

Flagged assumption 1, verbatim from the plan, still unresolved:

> **API-02 edge, `unclassified` by the deterministic probe, status `unresolved`.** The probe text is
> "unclassified — review manually". It is not auto-resolved, no backstop marker is authored for it,
> and it is not dropped. What this plan assumes while it stays open, each a reading a human may
> overturn:
> - "Removal over REST" means `POST /removals` only. `DELETE /sessions/{id}` and the worktree
>   removal route publish nothing, since D-06 defines the event as a `RemovalReport` and those routes
>   produce none.
> - "Drops the rows without a refresh" is satisfied by a connected subscriber. A subscriber that
>   missed a message through a reconnect, a late subscribe or lag converges only through its
>   existing poll, per D-08. No replay, no `id:`, no lag notice.
> - No ordering is promised between the `rows_removed` message and the `POST /removals` response
>   reaching the same client. The initiating client may see its own event first (21-05 handles it).
> A human classifying this edge should say which of the three, if any, API-02 actually requires.

Also still true: the 15 s keep-alive is read from source and not witnessed by a test, the lag
flood is synthetic, and the 503 for blocked persistence is covered by the single publish site
and not by a test of its own.

## Known issues

- The open macOS flake on `DELETE /sessions/{id}` is unchanged and was not seen here. No assertion
  was loosened for it.
- Requirements API-01, API-02 and API-03 stay pending. The plan's final task marks none.

## Commits

- `9c62f5a` test(21-04): add failing end-to-end test for the events stream
- `e702f8f` feat(21-04): publish rows_removed on a daemon-wide events stream
- `0633b20` test(21-04): pin the events publish matrix across row kinds and failures
- `c26d0ea` test(21-04): drive events lag through the route and gate the crate

## Self-Check: PASSED

- FOUND: `bauded/Cargo.toml`, `bauded/src/manager.rs`, `bauded/src/api.rs`, all modified against `7857f6d`
- FOUND: `9c62f5a`, `e702f8f`, `0633b20`, `c26d0ea`, each an ancestor of HEAD
- `git rev-list --count 7857f6d..HEAD` is 4, measured from the ledger `gsd-plan-head-before-21-04`
- `actuals.tokens` is 31476 characters of `git diff 7857f6d -- bauded` divided by 4
