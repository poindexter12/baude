# Phase 21: Daemon and PWA Parity - Context

**Gathered:** 2026-11-05
**Status:** Ready for planning

<domain>
## Phase Boundary

The same kill-then-remove operation that Phase 18 built in `baude-core` and Phase 20 exposed in the TUI becomes available over the `bauded` REST API with daemon-wide SSE updates, and from the PWA with a confirmation equivalent to the TUI's. Covers API-01, API-02, API-03. The TUI's remote-row refusal ("remote removal arrives with Phase 21") is replaced by a real action through the daemon. Daemon admission is wired to the Phase 19 suppression store through the shared `baude-core` helper. Out of scope: end-to-end fixture validation across every row kind, docs and the v2.8.0 release (Phase 22).

</domain>

<decisions>
## Implementation Decisions

### REST Removal Contract
- **D-01:** `POST /removals` executes and `POST /removals/preview` previews; the request body is the core `RemovalTarget` JSON (`{"repository": k}`, `{"checkout": k}`, `{"standalone": k}`), one route family for all four row kinds, mirroring the preview/execute split Phase 18 locked.
- **D-02:** The response body is the core `RemovalReport` serialized verbatim as 200 JSON; it already names rows removed, runtimes killed, paths deleted and kept, and clearings, and 18-CONTEXT fixed that the REST response reuses this report.
- **D-03:** Errors keep the daemon's existing plain-text `(StatusCode, String)` convention: 404 unknown target, 409 when the recheck row set differs from the preview (body is the fresh `RemovalReport` JSON), 500 runtime survived or disk failure, 503 persistence, matching `mutation_error`.
- **D-04:** Recheck before execute over REST: the execute body echoes the preview's `rows_removed` (and the runtimes killed and paths deleted sets, per WR-01); the daemon recomputes under the lock and returns 409 with the fresh report when any set changed, so the client re-shows it exactly like TUI D-08.

### List-Level SSE and Client Updates
- **D-05:** A new daemon-wide `GET /events` SSE stream backed by a `tokio::sync::broadcast` channel owned by `Manager`; the two existing per-session streams (`/stream`, `/activity-stream`) are untouched.
- **D-06:** One `rows_removed` event whose data is the `RemovalReport` (the same JSON the REST call returned), so a listener drops rows by `RowKey` without a refetch.
- **D-07:** `Manager` publishes after the removal's persist stage succeeds, inside the same lock, so no listener can observe a removed row resurrect.
- **D-08:** The PWA subscribes to `/events` and drops rows by key immediately, keeping the existing 3 s poll as reconciliation; TUI remote rows drop on the same event through the existing remote client path.

### PWA Row Reach and Hierarchy
- **D-09:** Add a read-only `GET /rows` hierarchy projection (repository → checkouts, plus standalone roots) carrying core keys, and extend `SessionInfo` with its `checkout_key`. The flat `/sessions` array stays a non-hierarchical compatibility projection (Phase 7 decision), so the existing test `flat_session_api_remains_a_non_hierarchical_compatibility_projection` keeps passing.
  - **Resolution (2026-11-11, user): Option B.** `checkout_key` is served ONLY from `GET /rows`; clients join on `session_id`; the `/sessions` forbidden-field test at `bauded/src/api.rs:1002-1010` stays unchanged and keeps passing. `SessionInfo` is not extended.
- **D-10:** The PWA offers remove on all four kinds: a repository group header removes every child; each checkout/worktree row and each standalone row removes itself. A row with no durable key shows no remove.
- **D-11:** `bauded` admission is wired through the same `baude-core` suppression helper the TUI uses (19 D-11) in this phase, since the daemon now removes rows and must not re-admit them. The suppression write rule is inherited unchanged (19 D-10: entry only for a checkout removed while its repository row remains).
- **D-12:** The TUI's remote-row refusal becomes a real action: `D` on a remote row calls the daemon's preview and execute endpoints and renders the same modal from the returned report, with the same recheck and failure posture.

### PWA Confirmation and Failure Handling
- **D-13:** Reuse the hand-rolled `openModal`/`closeModal` pattern in `bauded/web/app.js` (not native `confirm()`), rendered from the `/removals/preview` report with the same sections the TUI shows: title naming the row kind, rows dropped, runtimes killed (count plus names), paths deleted, paths kept, clearings. Confirm and cancel buttons; backdrop click and Escape cancel without mutating anything.
- **D-14:** On a 409 the modal re-renders in place from the fresh report in the response body and stays open; no removal is performed, same as TUI D-08.
- **D-15:** On 500/503 the modal stays open showing the daemon's plain-text error with a Retry button; nothing is dropped client-side until the 200 response or the `rows_removed` SSE event arrives. Clearing failures inside a 200 report surface as a non-blocking toast, matching 19 D-14.
- **D-16:** For a repository parent the modal lists every child checkout by name and branch (the TUI's "every child listed" rule); long lists scroll inside the modal box. A path is never shown under deleted unless the report lists it there.

### Claude's Discretion
- Exact broadcast channel capacity and lag handling for `/events`.
- Whether `GET /rows` and the `SessionInfo.checkout_key` field share one serializer or two.
- Modal markup and CSS, within the existing `style.css` conventions.
- Test layout inside the inline `#[cfg(test)] mod tests` in `bauded/src/api.rs` versus a new module.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `baude-core/src/removal.rs`: `RemovalTarget` (`:49`) and `RowKey` (`:58`) are Serde with `snake_case`, usable as the wire format; `preview_removal` (`:359`) is the side-effect-free dry run; `remove` (`:436`) takes `&mut RepositoryState`, the target, `&mut Vec<RowRuntime>` (`:371`) and a save closure, returning `RemovalReport` (`:180`). `RemovalError` (`:235`) is `Debug` + `Display` only, not Serde.
- TUI call sites in `baude/src/app.rs`: `Modal::ConfirmRemove` (`:501`), preview (`:3925`), runtime custody `take_row_runtimes` (`:3950`) / `restore_row_runtimes` (`:3981`), confirm with recheck (`:3997-4052`), `removal_notice` (`:4091`), keymap arm (`:5795`).
- PWA: `api()` helper (`bauded/web/app.js:93-105`) throws with `.status` and the plain-text body; `openModal`/`closeModal` (`:560-587`); `toast()` (`:85-91`); kill via `DELETE /sessions/{id}` behind native `confirm()` (`:532-534`).

### Established Patterns
- `bauded/src/api.rs`: `router(state: Shared)` (`:21-47`); `Shared = Arc<Mutex<Manager>>` (`manager.rs:38`) with `lock()` (`:106`); handlers call `lock(&state).method(..)`; `ApiError = (StatusCode, String)` (`api.rs:102`) with `mutation_error` mapping `MutationError::{Persistence → 503, Teardown → 500, Domain → fallback}` (`:108-119`).
- Persist pattern in `Manager`: clone `repository_state` to `before`, mutate, `save_checked()`, roll back unless `error.replacement_committed()` (`manager.rs:311-322`).
- SSE today is per-session only (`api.rs:608-642`, `:652-686`), bridging a sync subscriber channel to async with `Event::default().event("message")` and `KeepAlive::default()`. No broadcast channel exists.
- `Manager::runtime_checkouts: HashMap<CheckoutKey, u64>` (`manager.rs:128`) maps core checkout keys to daemon session ids; `Manager::remove` (`:1779`) is the retained-close path.
- PWA is vanilla JS with a single `state` object and `innerHTML` string templates; list refresh polls `/sessions` every 3 s (`app.js:137-139`, `:797-806`); per-session `EventSource` in the chat view (`:199`, `:238`).

### Integration Points
- New routes in `api.rs` `router()`: `/removals`, `/removals/preview`, `/rows`, `/events`.
- `Manager` gains preview/remove methods that resolve `runtime_checkouts` handles into `RowRuntime`s, call core, publish the event, and restore handles on `Err`.
- Daemon admission path (`Manager` session creation / repository admission) consults the Phase 19 suppression helper.
- `SessionInfo` (`manager.rs:182+`) gains `checkout_key`.
- TUI remote client path for `D` on remote rows.
- No auth: security is the tailnet (`api.rs:1-2`); the removal endpoint inherits that posture.

</code_context>

<specifics>
## Specific Ideas

- The PWA confirmation must describe exactly what will happen from the same preview report the TUI uses, never from client-side guesses.
- Error bodies stay plain text for parity with every other daemon route; only the 409 drift response carries JSON, and that is the fresh report.

</specifics>

<deferred>
## Deferred Ideas

- Auth on the removal endpoint beyond the tailnet posture.
- Replacing PWA list polling entirely with SSE.

</deferred>
