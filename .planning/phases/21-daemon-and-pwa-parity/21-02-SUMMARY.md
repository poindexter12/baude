---
phase: 21-daemon-and-pwa-parity
plan: 02
subsystem: api
tags: [rust, axum, bauded, rows, hierarchy, wire-contract]

requires:
  - phase: 21-01
    provides: removal_fixture, removal_fixture_two_checkouts, checkout_key_for_runtime_for_test, POST /removals and its preview
provides:
  - GET /rows on the bauded router, the key to name index for every durable row
  - Manager::rows, RowsView, RepositoryRowView, CheckoutRowView, StandaloneRowView, Manager::session_id_for_checkout
  - api test helpers rows_bytes, rows_body, rows_checkout, assert_rows_fields and the pinned field name constants
affects: [21-03, 21-05, 21-07, 21-08, 21-09, 21-10]

tech-stack:
  added: []
  patterns:
    - "Read projections over durable rows go through state_for_save, the merged view, never raw repository_state"
    - "Wire contract tests assert the exact key set of an entry, so a missing name and an extra name both fail"

key-files:
  created: []
  modified:
    - bauded/src/manager.rs
    - bauded/src/api.rs

key-decisions:
  - "D-09 Option B, resolved by the user 2026-11-11: checkout_key is served only from GET /rows, clients join on session_id, SessionInfo gains no field"
  - "Manager::rows projects over state_for_save, not raw repository_state, because the raw rows report an archived live session as archived false"
  - "The rows concurrency test joins both requests on the test thread in both poll orders, because the fixture containment redirect is thread-local"

requirements-completed: []
requirements-advanced: [API-03]

coverage:
  - deliverable: "GET /rows serves the pinned hierarchy projection with every field name asserted on a real fixture entry"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#rows_projection_names_every_row_by_key"
        status: pass
  - deliverable: "A parent's children resolve to a key and a name with no live session"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#rows_parent_lists_children_with_no_live_session"
        status: pass
  - deliverable: "The projection is a pure idempotent read"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#rows_projection_is_a_pure_idempotent_read"
        status: pass
  - deliverable: "An archived row reads as archived on the index"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#rows_reports_an_archived_row_as_archived"
        status: pass
  - deliverable: "A key from a removal report resolves to a label through GET /rows alone"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#rows_resolves_a_label_for_a_key_with_no_sessions_field"
        status: pass
  - deliverable: "A read joined with a removal is never half-applied"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#rows_concurrent_with_a_removal_is_never_half_applied"
        status: pass
  - deliverable: "The /sessions compatibility projection is unchanged under D-09 Option B"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#flat_session_api_remains_a_non_hierarchical_compatibility_projection"
        status: pass

duration: 7min
completed: 2026-11-11
status: complete
actuals:
  tokens: 5672
  tasks: 3
  commits: 3
plan_head_before: 08cb29c71e657e10316deba6692d5e22870bcebf
plan_head_after: b49e2d7ed16b6f7a9510035fafb9db00147552e2
---

# Phase 21 Plan 02: GET /rows key to name index Summary

**`GET /rows` on bauded: a repository to checkout hierarchy plus standalone roots, each entry carrying its durable key, display name, path and live session id, read from the merged save view so it never lags a live session's archived flag**

## Performance

- **Duration:** 7 min
- **Started:** 2026-11-11T21:01:37Z
- **Completed:** 2026-11-11T21:09:14Z
- **Tasks:** 3
- **Files modified:** 2

## Accomplishments

- `GET /rows` is the authoritative `RowKey` to label index. A client holding only a key from a `RemovalReport` resolves a display name and a path with no `/sessions` read.
- D-09 Option B is recorded above `SessionInfo`, which gained no field. The `/sessions` compatibility test is unmodified and passes with all eight forbidden names.
- A parent's children keep their key, name, branch and path after their runtime is gone, with `session_id` null.
- The read is byte-for-byte idempotent and leaves the state file untouched.

## D-09 Option B

Resolved by the user on 2026-11-11 and recorded in 21-CONTEXT.md: `checkout_key` is served only from `GET /rows`, clients join a `/rows` checkout entry to a `/sessions` row on `session_id`, which equals `SessionInfo::id`. `SessionInfo` gained no field; `session_info`, `list` and `info` are untouched; `baude/src/remote.rs` is not in this plan's diff.

### Task 1 compatibility baseline

```
cargo test -p bauded api::tests::flat_session_api_remains_a_non_hierarchical_compatibility_projection -- --test-threads=1 --exact
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 113 filtered out
STATUS=0
```

The same test passes unmodified inside Task 3's `api::tests::` run, 34 passed.

## RED gate

Recorded with `Manager::rows` as a `todo!()` body, before any implementation:

```
cargo test -p bauded api::tests::rows_ -- --test-threads=1 > /tmp/baude-21-02-red.log 2>&1
RED=101  REACHED=0

thread 'api::tests::rows_projection_names_every_row_by_key' panicked at bauded/src/manager.rs:1126:9:
not yet implemented
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 114 filtered out
```

RED is the first failing run, not a separate commit, per the plan and the Phase 20 convention.

## Task Commits

1. **Task 1: record D-09 Option B at SessionInfo** - `5dadf54` (docs)
2. **Task 2: Manager::rows and GET /rows** - `51b4366` (feat)
3. **Task 3: the wire decision and the concurrency edge** - `b49e2d7` (test)

## The `GET /rows` response shape

One JSON object, two arrays, never a bare array. Status is always 200; the handler has no error path.

Top level:

| Field | Type | Source |
|---|---|---|
| `repositories` | array of repository entries | sorted by `first_seen_order` |
| `standalone` | array of standalone entries | sorted by `first_seen_order` |

Repository parent entry, `RepositoryRowView`:

| Field | Type | Source |
|---|---|---|
| `repository_key` | u64 | `SavedRepository.key.get()` |
| `display_name` | string | file name of `observed_main_worktree`, or the full path for a root path |
| `path` | string | `observed_main_worktree`, absolute, not tilde-shortened |
| `health` | object | `RepositoryHealth` verbatim: `{"status":"available"}` or `{"status":"unavailable","cause":..}` |
| `checkouts` | array of checkout entries | this parent's children, sorted by `first_seen_order` |

Checkout child entry, `CheckoutRowView`:

| Field | Type | Source |
|---|---|---|
| `checkout_key` | u64 | `SavedCheckout.key.get()` |
| `repository_key` | u64 | the parent key, repeated |
| `display_name` | string | `session.name`, live value when a runtime exists |
| `branch` | string or null | `observed_branch`, verbatim and unnormalized |
| `path` | string | `observed_path` |
| `role` | string | `CheckoutRole` verbatim: `main`, `primary_default`, `managed_branch` |
| `managed_by_baude` | bool | stored value |
| `archived` | bool | `session.archived`, live value when a runtime exists |
| `session_id` | u64 or null | `runtime_checkouts`, null with no live runtime; equals `/sessions` `id` |

Standalone root entry, `StandaloneRowView`:

| Field | Type | Source |
|---|---|---|
| `standalone_key` | u64 | `SavedStandaloneSession.key.get()` |
| `display_name` | string | `session.name` |
| `path` | string | `canonical_path` |
| `archived` | bool | `session.archived` |
| `session_id` | u64 or null | always null until 21-10 adds the standalone runtime registry |

Not on the projection: `resume_id`, `claude_session_id`, `owned_runtime`, `lifecycle`, and any `removable` flag. The tests assert the exact key set of every entry, so an extra field fails as surely as a missing one.

### A real body, from `removal_fixture_two_checkouts`

Captured with `--nocapture` from `rows_projection_names_every_row_by_key`. It is pretty-printed through `serde_json::Value`, so keys read alphabetically here; on the wire they follow struct field order. Clients must not depend on key order.

```json
{
  "repositories": [
    {
      "checkouts": [
        {
          "archived": false,
          "branch": "refs/heads/main",
          "checkout_key": 1,
          "display_name": "repo",
          "managed_by_baude": false,
          "path": "/private/var/folders/q1/335pbzl13sz825k87q4ywl300000gq/T/bauded-removal-rows-names-27642/repo",
          "repository_key": 1,
          "role": "managed_branch",
          "session_id": 1
        },
        {
          "archived": false,
          "branch": "refs/heads/topic",
          "checkout_key": 2,
          "display_name": "repo:topic",
          "managed_by_baude": true,
          "path": "/private/var/folders/q1/335pbzl13sz825k87q4ywl300000gq/T/bauded-removal-rows-names-27642/data/baude/worktrees/claude/repository-1/topic-2",
          "repository_key": 1,
          "role": "managed_branch",
          "session_id": 2
        }
      ],
      "display_name": "repo",
      "health": {
        "status": "available"
      },
      "path": "/private/var/folders/q1/335pbzl13sz825k87q4ywl300000gq/T/bauded-removal-rows-names-27642/repo",
      "repository_key": 1
    }
  ],
  "standalone": []
}
```

Three things in that body are for later plans, and none is claimed correct here:

- **21-09:** the main checkout reads `role: "managed_branch"` with `managed_by_baude: false`, and both branches read in the `refs/heads/` form. The field names are fixed; the values are 21-09's to correct. This plan's tests assert presence and type only for `branch` and `role`.
- **21-07 and 21-05:** a worktree's `display_name` is `repo:topic`, the session name, not the branch. The repository parent's `display_name` and the main checkout's are both `repo`.
- **21-10:** `standalone` is `[]` on every fixture today. The five standalone names are compiled into `StandaloneRowView` but no test has seen a populated entry.

## What later plans need

### Production surface, `bauded/src/manager.rs`

```rust
#[derive(Serialize, Clone)] pub struct RowsView { pub repositories: Vec<RepositoryRowView>, pub standalone: Vec<StandaloneRowView> }
#[derive(Serialize, Clone)] pub struct RepositoryRowView { repository_key: u64, display_name: String, path: String, health: RepositoryHealth, checkouts: Vec<CheckoutRowView> }
#[derive(Serialize, Clone)] pub struct CheckoutRowView { checkout_key: u64, repository_key: u64, display_name: String, branch: Option<String>, path: String, role: CheckoutRole, managed_by_baude: bool, archived: bool, session_id: Option<u64> }
#[derive(Serialize, Clone)] pub struct StandaloneRowView { standalone_key: u64, display_name: String, path: String, archived: bool, session_id: Option<u64> }
pub fn rows(&self) -> RowsView
fn session_id_for_checkout(&self, key: CheckoutKey) -> Option<u64>
```

All view fields are `pub`. The view structs sit directly above `SessionInfo`; `rows` and `session_id_for_checkout` sit directly below `repository_path`.

`bauded/src/api.rs`: `.route("/rows", get(list_rows))` is the last route before `.with_state(state)`, and `async fn list_rows(State(state): State<Shared>) -> Json<RowsView>` sits directly below `list_sessions`. No `ApiError`, no await under the guard.

### `rows` reads the merged view

`rows` iterates `self.state_for_save()`, the clone of `repository_state` with `merge_live_runtime_metadata` applied. It does not read `repository_state` directly. 21-10 must keep that when it fills standalone `session_id`: if standalone rows get a live override path, extend the merge, not `rows`. A row removed through `POST /removals` is gone from `/rows` in the very next read. The parent stays with `checkouts: []`.

After `DELETE /sessions/{id}` the durable row stays in `/rows` with `session_id: null` and its name, branch and path intact.

### Test helpers, inline `mod tests` of `bauded/src/api.rs`

```rust
const ROWS_REPOSITORY_FIELDS: [&str; 5]   // sorted
const ROWS_CHECKOUT_FIELDS: [&str; 9]     // sorted
async fn rows_bytes(app: &Router) -> Vec<u8>              // asserts 200
async fn rows_body(app: &Router) -> serde_json::Value
fn assert_rows_fields(entry: &serde_json::Value, pinned: &[&str])   // exact key set
fn rows_checkout(rows: &serde_json::Value, key: CheckoutKey) -> Option<serde_json::Value>
```

21-10 should add a `ROWS_STANDALONE_FIELDS` constant and extend the `standalone` assertion in `rows_projection_names_every_row_by_key`, which names that deferral in a comment.

### The two shapes the concurrency test observed

`rows_concurrent_with_a_removal_is_never_half_applied` runs twice on the single-checkout `removal_fixture`, once per `tokio::join!` poll order. Both candidates are observed bodies, the pre shape read before the join and the post shape read after it.

- Read polled first: the raced read saw the **pre-removal** projection.
- Removal polled first: the raced read saw the **post-removal** projection.

Pre-removal, one checkout under the parent:

```json
{"repositories":[{"checkouts":[{"archived":false,"branch":"refs/heads/main","checkout_key":1,"display_name":"repo","managed_by_baude":false,"path":"<root>/repo","repository_key":1,"role":"managed_branch","session_id":1}],"display_name":"repo","health":{"status":"available"},"path":"<root>/repo","repository_key":1}],"standalone":[]}
```

Post-removal, the parent kept with no children:

```json
{"repositories":[{"checkouts":[],"display_name":"repo","health":{"status":"available"},"path":"<root>/repo","repository_key":1}],"standalone":[]}
```

`<root>` stands for the fixture temp root; the rest is verbatim. No third shape was seen, and no production change was needed. Limit of this proof: both requests run on one thread, so the interleave is cooperative, not preemptive. Removing a repository's last checkout leaves an empty parent in `/rows`; 21-05, 21-07 and 21-08 must render or hide a childless parent deliberately.

## Observed test counts

- `cargo test -p bauded api::tests::rows_ -- --test-threads=1`: 4 passed at the Task 2 commit, 6 at the Task 3 commit
- `cargo test -p bauded api::tests:: -- --test-threads=1`: 34 passed, 0 failed, 86 filtered out
- `cargo test -p bauded`: 120 passed, 0 failed, up from 114 before this plan
- `cargo fmt --all --check`: exit 0
- `cargo clippy --workspace --all-targets -- -D warnings`: exit 0

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `rows` reads the merged save view, not raw `repository_state`**
- **Found during:** Task 2
- **Issue:** The plan says `rows` "reads `self.repository_state` only" and takes `archived` from `session.archived`. Built that way, a live session archived through `POST /sessions/{id}/archive` read back `"archived": false`, because the durable row's retained metadata is refreshed only at save, through `merge_live_runtime_metadata`. That breaks the plan's first prohibition. Observed, not inferred: `rows_reports_an_archived_row_as_archived` failed against the plan-literal body with `left: Bool(false) right: true`.
- **Fix:** `rows` iterates `self.state_for_save()`. Still a pure read with no allocation of keys and no save; the idempotency test passes over it.
- **Files modified:** bauded/src/manager.rs
- **Verification:** the archived test fails on the raw read and passes on the merged one
- **Commit:** 51b4366

**2. [Addition] Test `rows_reports_an_archived_row_as_archived`**
- Not in the plan's test list. It is the only assertion behind prohibition 1 and it is what found deviation 1.

**3. [Strengthening] Exact key set instead of per-name presence**
- The plan asks for `entry.get("<name>").is_some()` per pinned name. `assert_rows_fields` compares the sorted key set for equality, which covers presence and also fails on an extra field such as a conversation identifier.

**4. [Test shape] The concurrency test joins in both orders, on one thread**
- **Found during:** Task 3
- **Issue:** A first attempt used a multi-thread runtime with `tokio::spawn` to make the race preemptive. Core's containment guard panicked on the worker thread, `config dir resolved to the real user path`, because `TestRedirect` is thread-local. The guard fired before any real path was touched.
- **Fix:** `tokio::join!` on the test thread, as the plan wrote it, run once per poll order so both candidates are exercised without a sleep.
- **Commit:** b49e2d7

**5. [Addition] One `println!` of the `/rows` body in `rows_projection_names_every_row_by_key`**
- Captured by default; under `--nocapture` it gives client plans a real body to copy.

**6. [Bookkeeping] API-03 left pending**
- The plan frontmatter lists `requirements: [API-03]`. API-03 is shared with later plans in this phase, so it is not marked complete in REQUIREMENTS.md.

**Total deviations:** 1 auto-fixed bug, 5 recorded additions or adjustments. **Impact:** the wire contract is exactly as pinned; only the read source inside `rows` differs from the plan text.

## TDD Gate Compliance

- RED: recorded run, `RED=101`, log carries `not yet implemented`. Not a separate commit.
- GREEN: `51b4366` (feat).
- REFACTOR: none needed.

## Issues Encountered

None beyond the deviations above.

## Known Stubs

None. Standalone `session_id` is null by construction and owned by 21-10, as the plan states; it is a documented deferral, not a stub that blocks this plan's goal.

## Next Phase Readiness

- 21-05, 21-07 and 21-08 can write fixtures against the body above. Join on `session_id`, never on a name.
- 21-09 owns `role` and `branch` value correctness; the main checkout currently reads `managed_branch`.
- 21-10 fills standalone `session_id` and must keep `rows` on the merged view.
- 21-03 inherits nothing new from this plan beyond the empty parent shape after a last-child removal.

## Self-Check: PASSED

- FOUND: bauded/src/manager.rs, bauded/src/api.rs
- FOUND: 5dadf54, 51b4366, b49e2d7 as ancestors of HEAD
- `git rev-list --count 08cb29c..HEAD` measured 3 before the metadata commit
