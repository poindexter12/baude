# Phase 21: Daemon and PWA Parity - Research

**Researched:** 2026-11-05
**Domain:** axum 0.8 REST + SSE in `bauded`, vanilla-JS PWA, shared `baude-core` removal
**Confidence:** MEDIUM (session was context-limited; code pointers below are from CONTEXT.md unless tagged VERIFIED)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### REST Removal Contract
- `POST /removals` executes and `POST /removals/preview` previews; the request body is the core `RemovalTarget` JSON (`{"repository": k}`, `{"checkout": k}`, `{"standalone": k}`), one route family for all four row kinds, mirroring the preview/execute split Phase 18 locked.
- The response body is the core `RemovalReport` serialized verbatim as 200 JSON; it already names rows removed, runtimes killed, paths deleted and kept, and clearings, and 18-CONTEXT fixed that the REST response reuses this report.
- Errors keep the daemon's existing plain-text `(StatusCode, String)` convention: 404 unknown target, 409 when the recheck row set differs from the preview (body is the fresh `RemovalReport` JSON), 500 runtime survived or disk failure, 503 persistence, matching `mutation_error`.
- Recheck before execute over REST: the execute body echoes the preview's `rows_removed` (and the runtimes killed and paths deleted sets, per WR-01); the daemon recomputes under the lock and returns 409 with the fresh report when any set changed, so the client re-shows it exactly like TUI D-08.

#### List-Level SSE and Client Updates
- A new daemon-wide `GET /events` SSE stream backed by a `tokio::sync::broadcast` channel owned by `Manager`; the two existing per-session streams (`/stream`, `/activity-stream`) are untouched.
- One `rows_removed` event whose data is the `RemovalReport` (the same JSON the REST call returned), so a listener drops rows by `RowKey` without a refetch.
- `Manager` publishes after the removal's persist stage succeeds, inside the same lock, so no listener can observe a removed row resurrect.
- The PWA subscribes to `/events` and drops rows by key immediately, keeping the existing 3 s poll as reconciliation; TUI remote rows drop on the same event through the existing remote client path.

#### PWA Row Reach and Hierarchy
- Add a read-only `GET /rows` hierarchy projection (repository → checkouts, plus standalone roots) carrying core keys, and extend `SessionInfo` with its `checkout_key`. The flat `/sessions` array stays a non-hierarchical compatibility projection (Phase 7 decision), so the existing test `flat_session_api_remains_a_non_hierarchical_compatibility_projection` keeps passing.
- The PWA offers remove on all four kinds: a repository group header removes every child; each checkout/worktree row and each standalone row removes itself. A row with no durable key shows no remove.
- `bauded` admission is wired through the same `baude-core` suppression helper the TUI uses (19 D-11) in this phase, since the daemon now removes rows and must not re-admit them. The suppression write rule is inherited unchanged (19 D-10: entry only for a checkout removed while its repository row remains).
- The TUI's remote-row refusal becomes a real action: `D` on a remote row calls the daemon's preview and execute endpoints and renders the same modal from the returned report, with the same recheck and failure posture.

#### PWA Confirmation and Failure Handling
- Reuse the hand-rolled `openModal`/`closeModal` pattern in `bauded/web/app.js` (not native `confirm()`), rendered from the `/removals/preview` report with the same sections the TUI shows: title naming the row kind, rows dropped, runtimes killed (count plus names), paths deleted, paths kept, clearings. Confirm and cancel buttons; backdrop click and Escape cancel without mutating anything.
- On a 409 the modal re-renders in place from the fresh report in the response body and stays open; no removal is performed, same as TUI D-08.
- On 500/503 the modal stays open showing the daemon's plain-text error with a Retry button; nothing is dropped client-side until the 200 response or the `rows_removed` SSE event arrives. Clearing failures inside a 200 report surface as a non-blocking toast, matching 19 D-14.
- For a repository parent the modal lists every child checkout by name and branch (the TUI's "every child listed" rule); long lists scroll inside the modal box. A path is never shown under deleted unless the report lists it there.

### Claude's Discretion
- Exact broadcast channel capacity and lag handling for `/events`.
- Whether `GET /rows` and the `SessionInfo.checkout_key` field share one serializer or two.
- Modal markup and CSS, within the existing `style.css` conventions.
- Test layout inside the inline `#[cfg(test)] mod tests` in `bauded/src/api.rs` versus a new module.

### Deferred Ideas (OUT OF SCOPE)
- Auth on the removal endpoint beyond the tailnet posture.
- Replacing PWA list polling entirely with SSE.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| API-01 | REST removal endpoint for all four row kinds calling the same `baude-core` operation, returning rows removed, runtimes killed, paths deleted | REST contract section; `Manager` preview/remove wrapper pattern; `RemovalReport` as verbatim 200 body |
| API-02 | Removal over REST emits SSE so a connected TUI or PWA drops rows without refresh | `/events` broadcast design; publish-under-lock rule; PWA `EventSource` + TUI remote client hook |
| API-03 | PWA offers remove on the same row kinds with a TUI-equivalent confirmation, completing through API-01 | `GET /rows` projection; `openModal` reuse; 409/500/503 posture |
</phase_requirements>

## Summary

Phase 21 is almost entirely plumbing over work that already exists. `baude-core::removal` already defines the wire-shaped types (`RemovalTarget`, `RowKey`, `RemovalReport` are Serde `snake_case`) and the two operations (`preview_removal`, `remove`). The daemon's job is a thin `Manager` layer that (1) resolves `runtime_checkouts` session handles into `Vec<RowRuntime>`, (2) runs core under the existing `Shared = Arc<Mutex<Manager>>` lock using the `before`-clone / `save_checked` / rollback pattern, (3) publishes one `rows_removed` broadcast event after persist succeeds and before the lock drops, and (4) restores runtime handles on `Err`. The API layer maps `RemovalError` (not Serde, `Display` only) onto the existing plain-text `(StatusCode, String)` convention, with the single JSON exception being the 409 drift body.

The two genuinely new mechanisms are the daemon-wide SSE stream and the hierarchy projection. Neither needs a new crate: axum 0.8's `Sse` + `KeepAlive` are already used by the per-session streams, and a `tokio::sync::broadcast::Receiver` can be adapted to a `Stream` either with `tokio-stream` (`BroadcastStream`, needs the `sync` feature) or hand-rolled with `futures_util::stream::unfold` over `rx.recv()`, which avoids a new dependency. The PWA side is vanilla JS: a second `EventSource` on `/events`, a modal rendered from the preview report, and `state` mutation by key.

**Primary recommendation:** Build `Manager::preview_removal` / `Manager::remove_rows` first (TDD against the inline `api.rs` harness: `api_scope`, `app()`, `post_json`, `body_json`, `initialized_repo`, `exited_tracked_manager`, `AtomicFailure`), then routes, then `/events`, then `/rows` + PWA, then the TUI remote path. Keep the admission suppression wiring as its own plan because it touches session creation, not removal.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Kill-then-remove semantics, recheck set comparison | `baude-core` (library) | — | Phase 18 op is the single source of truth; daemon must not reimplement |
| Runtime handle custody, persist, rollback, event publish | `bauded` `Manager` | — | Owns `runtime_checkouts`, `repository_state`, the lock and the broadcast sender |
| HTTP contract, status mapping, JSON bodies | `bauded` `api.rs` | — | Existing `mutation_error` / `ApiError` conventions |
| Daemon-wide SSE fan-out | `bauded` `api.rs` + `Manager` | — | Sender lives in `Manager`; handler subscribes per connection |
| Hierarchy projection `/rows`, `checkout_key` on `SessionInfo` | `bauded` `Manager` serializers | — | `/sessions` stays flat per Phase 7 |
| Confirmation modal, row drop by key, retry/409 re-render | PWA (`bauded/web/app.js`) | — | Browser renders only what the report says |
| Remote `D` action | TUI (`baude/src`) remote client | `bauded` REST | TUI is a REST client here, same modal from the returned report |
| Admission suppression | `bauded` `Manager` admission path | `baude-core::suppression` | Shared helper per 19 D-11 |

## Standard Stack

No new runtime libraries are required. Versions below are from `bauded/Cargo.toml` `[VERIFIED: bauded/Cargo.toml:13-38]` — `axum = { version = "0.8", features = ["ws"] }`, `futures-core = "0.3"`, `futures-util = "0.3"`, `tokio = { version = "1", features = ["macros", "net", "rt-multi-thread", "signal", "time"] }`, `tokio-tungstenite = "0.26"`.

| Library | Version | Purpose | Note |
|---------|---------|---------|------|
| axum | 0.8 | `Sse`, `KeepAlive`, `Json`, router | Already used for per-session streams |
| tokio `sync::broadcast` | tokio 1 (`sync` is part of the default/`macros` build; confirm it compiles, else add the `sync` feature) | Daemon-wide `/events` channel | `[ASSUMED]` that `sync` is already enabled transitively |
| futures-util | 0.3 | `stream::unfold` to adapt `Receiver::recv()` into a `Stream` | Avoids adding `tokio-stream` |
| tokio-stream (optional) | — | `wrappers::BroadcastStream` | Only if the hand-rolled unfold proves awkward; `[ASSUMED]` not currently a dependency (not present in the grep of `Cargo.toml`) |

## Package Legitimacy Audit

No new packages are recommended. If the planner chooses `tokio-stream`, verify with `cargo search tokio-stream` and `gsd_run query package-legitimacy check --ecosystem crates tokio-stream` before adding it (it is the official tokio-rs companion crate `[ASSUMED]`).

## Architecture Patterns

### System Architecture Diagram

```
PWA / TUI-remote                    bauded (axum)                         baude-core
----------------                    -------------                         ----------
POST /removals/preview ---------> handler -> lock(&state)
   body: RemovalTarget               -> Manager::preview_removal -------> preview_removal(&state, target)
                                     <- RemovalReport (200 JSON) <-------- RemovalReport
   render modal from report
POST /removals ------------------> handler -> lock(&state)
   body: {target, expected:         -> Manager::remove_rows:
          rows_removed,                 recompute preview under lock
          runtimes_killed,              if sets differ -> Err(Drift(report)) -> 409 + report JSON
          paths_deleted}                take RowRuntimes from runtime_checkouts
                                        before = repository_state.clone()
                                        remove(&mut state, target, &mut runtimes, save) --> remove(...)
                                        Err -> restore runtimes, roll back unless replacement_committed
                                        Ok  -> broadcast.send(rows_removed(report))   [still under lock]
                                     <- RemovalReport (200 JSON)
GET /events <-------------------- Sse(stream over broadcast::Receiver) + KeepAlive
   on "rows_removed": drop rows by RowKey, no refetch
GET /rows <---------------------- Manager hierarchy projection (repository -> checkouts, standalone roots)
```

### Pattern 1: Manager wrapper with runtime custody and publish-under-lock
**What:** Mirror the TUI's `take_row_runtimes` / `restore_row_runtimes` / confirm-with-recheck sequence (`baude/src/app.rs:3950-4052` per CONTEXT.md `[CITED: 21-CONTEXT.md]`) inside `Manager`, using `runtime_checkouts: HashMap<CheckoutKey, u64>` to find the daemon session for each checkout key, and the `before` clone + `save_checked()` + `replacement_committed()` rollback already used in `manager.rs:311-322` `[CITED: 21-CONTEXT.md]`.
**When to use:** Every mutation path that touches `repository_state`.
**Key rule:** `broadcast::Sender::send` happens after `save_checked` returns `Ok` and before the `MutexGuard` drops. Ignore the `SendError` from `send` when there are zero receivers; it is not a failure.

### Pattern 2: Broadcast receiver as an SSE stream
```rust
// Sketch, [ASSUMED] against axum 0.8 / futures-util 0.3 APIs; verify at compile time.
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream::{self, Stream};
use tokio::sync::broadcast::error::RecvError;

async fn events(State(state): State<Shared>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = lock(&state).subscribe_events();            // broadcast::Receiver<DaemonEvent>
    let stream = stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(ev) => return Some((Ok(ev.into_sse()), rx)),   // Event::default().event("rows_removed").json_data(report)
                Err(RecvError::Lagged(_)) => continue,              // drop lag; the 3 s poll reconciles
                Err(RecvError::Closed) => return None,
            }
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}
```
**Discretion recommendation:** capacity 64; on `Lagged` skip rather than close (the PWA poll and a TUI refetch reconcile). Document that a lagged client may miss an event; this is acceptable because the poll remains.

### Pattern 3: Execute request shape
Keep `RemovalTarget` as the preview body verbatim. For execute, wrap it: `{ "target": <RemovalTarget>, "expected": { "rows_removed": [...], "runtimes_killed": [...], "paths_deleted": [...] } }`. The daemon recomputes `preview_removal` under the lock and compares the three sets (WR-01 posture); mismatch returns `409` with the fresh `RemovalReport` JSON body. `[ASSUMED]` exact field names; planner should fix them to match the `RemovalReport` field names so the PWA can echo the preview response without remapping.

### Pattern 4: Error mapping
`RemovalError` is `Debug + Display` only `[CITED: 21-CONTEXT.md, removal.rs:235]`. Map by variant in `api.rs`: unknown target → 404; runtime survived / disk failure → 500; persistence → 503 (reuse `mutation_error` where the variant is a `MutationError`). Body is `error.to_string()` plain text. Only the drift case returns JSON.

### Anti-Patterns to Avoid
- **Re-deriving the row set client-side:** the modal must render only from the preview report.
- **Publishing after the lock drops:** a `/sessions` poll between persist and publish could resurrect a row; keep `send` inside the guard.
- **Native `confirm()`:** locked out; use `openModal`.
- **Touching `/stream` or `/activity-stream`:** locked untouched.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Removal semantics, recheck comparison | A daemon-side kill/remove | `baude_core::removal::{preview_removal, remove}` | Phase 18 fixtures already prove it |
| Suppression of removed checkouts on admission | A daemon-side denylist | `baude_core::suppression::{is_suppressed, suppress}` `[VERIFIED: baude-core/src/suppression.rs:76,98]` — signatures: `pub fn is_suppressed(root: &Path, path: &Path) -> bool` and `pub fn suppress(root: &Path, path: &Path, common_dir: &Path, now_ms: u64) -> io::Result<()>` | 19 D-11 shared helper |
| SSE framing, keep-alives | Manual `text/event-stream` writes | `axum::response::sse::{Sse, Event, KeepAlive}` | Already the daemon convention |
| Fan-out to many listeners | Per-connection `Vec<Sender>` | `tokio::sync::broadcast` | Locked decision; handles lag and drop |

## Common Pitfalls

### Pitfall 1: Rows resurrect between persist and event
**What goes wrong:** A `/sessions` poll lands after `save_checked` but before the event, or the TUI refetches and sees stale state.
**How to avoid:** Publish under the lock; `/rows` and `/sessions` are served from the same `Manager` so they cannot be stale once the guard drops.

### Pitfall 2: Runtime handles leaked on error
**What goes wrong:** `remove` returns `Err` after some runtimes were taken from `runtime_checkouts`; the daemon forgets live sessions.
**How to avoid:** Mirror TUI `restore_row_runtimes`; test with `exited_tracked_manager` and an `AtomicFailure`-forced persistence failure, asserting the session is still listed.

### Pitfall 3: `RemovalError` not serializable
**How to avoid:** Never `Json(err)`; use `Display`. Only the drift variant carries a report, serialized separately.

### Pitfall 4: Tests that grep source
Repo CLAUDE.md forbids it. Assert via HTTP responses (`body_json`), fixture filesystem state, and the `/events` stream body.

### Pitfall 5: Commit messages with nested parentheses
release-please silently skips them; keep commit bodies flat.

### Pitfall 6: Linux-only behavior
Process kill and file timing are platform code; run `scripts/gate.sh --linux` before the phase ships.

## Code Examples

PWA sketch (vanilla JS, matches `api()` which throws with `.status` and plain-text body `[CITED: 21-CONTEXT.md app.js:93-105]`):

```js
// [ASSUMED] shape; adapt to the existing api() helper
async function confirmRemove(target) {
  let report = await api('POST', '/removals/preview', target);
  const render = (r, err) => openModal(removeModalHtml(r, err)); // sections: rows, runtimes, deleted, kept, clearings
  render(report);
  onConfirm(async () => {
    try {
      const done = await api('POST', '/removals', { target, expected: pick(report, ['rows_removed','runtimes_killed','paths_deleted']) });
      dropRowsByKey(done.rows_removed); closeModal();
      if (done.clearings && done.clearings.some(c => c.error)) toast('Some clearings failed');
    } catch (e) {
      if (e.status === 409) { report = JSON.parse(e.message); render(report); }   // re-show, stay open
      else render(report, e.message);                                            // 500/503: show text + Retry
    }
  });
}
const events = new EventSource('/events');
events.addEventListener('rows_removed', ev => dropRowsByKey(JSON.parse(ev.data).rows_removed));
```

Note: the 409 body is JSON while `api()` currently surfaces bodies as text; the client must `JSON.parse` the error text for that one status.

## Runtime State Inventory

Not a rename/refactor phase. Omitted.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain / cargo | all | assumed (repo builds today) | — | — |
| Docker | `scripts/gate.sh --linux` | not probed this session | — | run plain `scripts/gate.sh` and flag for human |
| Browser | PWA manual check | yes | — | — |

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test harness, inline `#[cfg(test)] mod tests` in `bauded/src/api.rs` `[VERIFIED: bauded/src/api.rs:721-936]` (`api_scope` :721, `app()` :753, `body_json` :759, `post_json` :768, `initialized_repo` :815, `exited_tracked_manager` :936) |
| Quick run command | `cargo test -p bauded removal` |
| Full suite command | `scripts/gate.sh` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| API-01 | preview and execute for repository/checkout/worktree/standalone return `RemovalReport`; 404/409/500/503 mapping; runtime restore on failure | integration (axum `oneshot`) | `cargo test -p bauded removals_` | ❌ Wave 0 |
| API-02 | `/events` emits `rows_removed` with the report after a removal; no event on failure | integration | `cargo test -p bauded events_` | ❌ Wave 0 |
| API-03 | `/rows` projection carries keys; `SessionInfo.checkout_key`; flat `/sessions` test still passes | integration | `cargo test -p bauded rows_` | ❌ Wave 0 (PWA modal itself is manual: no JS test harness exists `[ASSUMED]`) |
| Admission | suppressed checkout is not re-admitted by the daemon | integration | `cargo test -p bauded suppress` | ❌ Wave 0 |

### Sampling Rate
- Per task commit: `cargo test -p bauded`
- Per wave merge: `scripts/gate.sh`
- Phase gate: `scripts/gate.sh --linux` green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] Removal test group in `bauded/src/api.rs` tests (or a new `bauded/src/api/removal_tests.rs` at the planner's discretion)
- [ ] SSE test helper that reads the first event frame from the `/events` body with a timeout
- [ ] PWA: manual checklist (modal sections, Escape/backdrop cancel, 409 re-render, Retry)

## Security Domain

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2/V3 Auth/Session | no (deferred; tailnet posture) | — |
| V4 Access Control | no | tailnet |
| V5 Input Validation | yes | Serde deserialization of `RemovalTarget`; reject unknown fields; paths come only from the daemon's own state, never from the request |
| V6 Cryptography | no | — |

| Pattern | STRIDE | Mitigation |
|---------|--------|------------|
| Client-supplied path deletion | Tampering | Execute body carries keys and expected sets only; the daemon derives paths from its own state |
| Cross-origin removal (CSRF) | Spoofing | Same posture as `DELETE /sessions/{id}` today; note in docs, deferred |

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | tokio `sync` feature is enabled transitively so `broadcast` compiles without a Cargo change | Stack | Add `"sync"` to tokio features |
| A2 | `futures_util::stream::unfold` adapter is sufficient; `tokio-stream` not needed | Stack | Add `tokio-stream` with `sync` feature after legitimacy check |
| A3 | Execute body field names (`target`, `expected.*`) | Pattern 3 | Planner fixes names; PWA echoes report fields |
| A4 | Code line references from CONTEXT.md are current on this branch | throughout | Planner re-greps before citing |
| A5 | TUI refusal string: grep for "arrives with Phase 21" found no match in `baude/src`; wording differs or lives elsewhere | TUI remote path | Planner must grep `Phase 21`/`remote` in `baude/src/app.rs` to locate the refusal |
| A6 | No JS test harness for the PWA | Validation | PWA checks stay manual |

## Open Questions

1. **Where exactly is the TUI remote refusal?** Grep for the literal CONTEXT.md phrase found nothing; locate it by searching `remote` near the `D` keymap arm (`app.rs:5795` per CONTEXT.md).
2. **How does the TUI remote client currently consume daemon lists?** Not read this session; the planner should read the remote client module in `baude/src` before assigning the `/events` consumption task.
3. **Daemon admission entry point:** which `Manager` method admits a repository/checkout (session creation vs rediscovery) needs confirming before wiring `is_suppressed`.

## Sources

### Primary (HIGH confidence)
- `bauded/Cargo.toml` lines 13-38 (dependency versions)
- `baude-core/src/suppression.rs` lines 76, 98 (helper signatures)
- `bauded/src/api.rs` lines 721-936 (test harness function locations)

### Secondary (MEDIUM confidence)
- `.planning/phases/21-daemon-and-pwa-parity/21-CONTEXT.md` code_context section (line references to `removal.rs`, `manager.rs`, `api.rs`, `app.js`)

### Tertiary (LOW confidence)
- axum 0.8 SSE / tokio broadcast usage sketch from training knowledge

## Project Constraints (from CLAUDE.md)
- Run `scripts/gate.sh` before every push; `--linux` for platform code; never judge a gate step through a pipe.
- Every change lands by PR to protected `main`; conventional commits; no nested parentheses in commit bodies or PR titles.
- Fixtures must model real shapes; assert on fixture output, never by grepping source.

## Metadata

**Confidence breakdown:** Stack HIGH (no new deps); Architecture MEDIUM (CONTEXT.md pointers, partial direct verification); Pitfalls MEDIUM.
**Research date:** 2026-11-05. **Valid until:** 2026-12-05.
**Note:** This research was cut short by context exhaustion; the planner should re-read `bauded/src/manager.rs` and the TUI remote client before locking task line references.
