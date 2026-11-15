# Phase 21: Daemon and PWA Parity - Pattern Map

**Mapped:** 2026-11-06
**Files analyzed:** 7 (6 modified, 1 optional new test module)
**Analogs found:** 7 / 7
**Analog gate:** every path below is git-TRACKED source (`git ls-files` verified); no gitignored mirror paths.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `bauded/src/api.rs` — `/removals`, `/removals/preview` handlers + `removal_error` mapping | route/controller | request-response | `bauded/src/api.rs` `create_session` / `delete_session` + `mutation_error` (same file, `:110-192`) | exact |
| `bauded/src/api.rs` — `GET /events` | route/controller | streaming (SSE pub-sub) | `bauded/src/api.rs` `activity_stream` (`:652-687`) | exact (role + flow) |
| `bauded/src/api.rs` — `GET /rows` | route/controller | request-response (read projection) | `bauded/src/api.rs` `list_sessions` (`:151-153`) | exact |
| `bauded/src/api.rs` tests (or new `bauded/src/api/removal_tests.rs`) | test | request-response | `api.rs` tests `flat_session_api_remains_a_non_hierarchical_compatibility_projection` (`:964-1010`) + `real_atomic_persistence_failures_are_503_for_every_mutation` (`:1130-1180`) | exact |
| `bauded/src/manager.rs` — `preview_removal`, `remove_rows`, broadcast sender, `subscribe_events` | service | CRUD + pub-sub | `baude/src/app.rs` `open_remove_project_modal` / `collect_row_runtimes` / `restore_row_runtimes` / `confirm_remove_project` (`:3908-4086`) **plus** `bauded/src/manager.rs` `confirm_remove_worktree` (`:1599-1700`) and `ManagerLifecycleEffects::persist_lifecycle` (`:312-324`) | exact (Phase 20 TUI path is the proven shape; manager supplies the lock/persist idiom) |
| `bauded/src/manager.rs` — `SessionInfo.checkout_key` + `/rows` projection | model/serializer | transform | `bauded/src/manager.rs` `SessionInfo` (`:181-230`) + `session_info()` (`:2616-2683`) | exact |
| `bauded/src/manager.rs` — admission suppression guard | service | CRUD | `baude-core/src/suppression.rs` `is_suppressed` (`:76`) / `lift` (`:84`) / `suppress` (`:98`), called from `Manager::create` (`:813-905`) | role-match (no daemon-side suppression call site exists yet) |
| `bauded/web/app.js` — remove modal, `dropRowsByKey`, `/rows` render | component | event-driven + request-response | `bauded/web/app.js` `openModal`/`closeModal` (`:560-587`), `openActivity` EventSource (`:236-275`), `deleteSession` (`:530-540`), `renderList` (`:596-644`) | exact |
| `bauded/web/style.css` — modal scroll box | config/style | — | `bauded/web/style.css` `#modal .box` (`:310-334`), `.act-feed` scroll block (`:211-212`) | exact |
| `baude/src/remote.rs` — remote preview/execute calls + `/events` subscriber | service (HTTP client) | request-response + streaming | `bauded`-facing `RemotePoller::create` (`:225-259`) and `delete` (`:198-204`), `short_err` (`:271-279`) | exact |
| `baude/src/app.rs` — remote `D` arm replacing the refusal | controller (UI state) | request-response | `baude/src/app.rs` keymap arm `Modal::ConfirmRemove` (`:5795-5867`) + refusal site `SidebarRefusal::RemoteRemove` (`:299-300`, `:344`, `:1328-1330`) | exact |
| `bauded/Cargo.toml` | config | — | n/a — **no change needed** (see Shared Patterns → Dependencies) | n/a |

## Pattern Assignments

### `bauded/src/api.rs` — `POST /removals/preview` and `POST /removals` (route, request-response)

**Analog:** `bauded/src/api.rs` (same file; copy the local conventions, do not invent new ones)

**Router registration** (`:21-48`) — append to the existing chain, before `.with_state(state)`:
```rust
pub fn router(state: Shared) -> Router {
    Router::new()
        .merge(crate::web::router())
        .route("/info", get(info))
        .route("/sessions", get(list_sessions).post(create_session))
        // ... existing per-session routes ...
        .route("/sessions/{id}/activity-stream", get(activity_stream))
        .with_state(state)
}
```

**Handler shape — JSON body in, Json out, mapped error** (`:173-192`):
```rust
async fn create_session(
    State(state): State<Shared>,
    Json(body): Json<CreateBody>,
) -> Result<(StatusCode, Json<SessionInfo>), ApiError> {
    let info = lock(&state)
        .create(&body.repo, body.worktree.as_deref(), body.name.as_deref())
        .map_err(|error| mutation_error(error, StatusCode::BAD_REQUEST))?;
    crate::permission_bridge::watch_if_needed(&state, info.id);
    Ok((StatusCode::CREATED, Json(info)))
}

async fn delete_session(
    State(state): State<Shared>,
    Path(id): Path<u64>,
) -> Result<StatusCode, ApiError> {
    lock(&state)
        .remove(id)
        .map_err(|error| mutation_error(error, StatusCode::NOT_FOUND))?;
    Ok(StatusCode::NO_CONTENT)
}
```
Request bodies are plain `#[derive(Deserialize)]` structs declared immediately above their handler (`:165-171`, `:194-198`, `:237-241`). The execute body (D-04/WR-01) follows that: one struct holding `target: RemovalTarget` plus the echoed expected sets. `RemovalTarget` needs no wrapper — it is already `Deserialize` + `#[serde(rename_all = "snake_case")]` (`baude-core/src/removal.rs:47-53`).

**Error mapping pattern** (`:104-118`) — the new `removal_error` must mirror this exactly, since `RemovalError` is NOT `MutationError`:
```rust
type ApiError = (StatusCode, String);

fn not_found(e: anyhow::Error) -> ApiError {
    (StatusCode::NOT_FOUND, e.to_string())
}

fn mutation_error(error: MutationError, fallback: StatusCode) -> ApiError {
    match error {
        MutationError::Persistence(error) => (StatusCode::SERVICE_UNAVAILABLE, error.to_string()),
        // The session exists but its runtime would not stop: a server-side
        // failure, never the caller's "not found"/"bad request" fallback.
        MutationError::Teardown(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
        MutationError::Domain(error) => (fallback, error.to_string()),
    }
}
```
`RemovalError` variants to map (`baude-core/src/removal.rs:234-255`, `Display` at `:257-286`, no `Serialize`):
`UnknownTarget` → 404 · `Persist{..}` → 503 · `RuntimeSurvived{..}` / `RuntimeHandleMissing` / `RuntimeIdentityMismatch` / `DiskRemoval{..}` → 500 · `Validation(..)` → 500. Body is always `error.to_string()` plain text (D-03). The 409 drift body is the only JSON error body.

**409 drift response:** return a type that carries both status and a JSON body. The file already imports `axum::response::IntoResponse` (`:11`) and uses `(StatusCode, &str).into_response()` in `push_router` (`:81`); the equivalent here is `(StatusCode::CONFLICT, Json(fresh_report)).into_response()`.

---

### `bauded/src/api.rs` — `GET /events` (route, streaming)

**Analog:** `activity_stream` (`:652-687`) — the second, newer of the two SSE handlers.

**Full handler shape to copy** (`:652-687`):
```rust
async fn activity_stream(
    State(state): State<Shared>,
    Path(id): Path<u64>,
) -> Result<Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    lock(&state).event_path(id).map_err(not_found)?;

    let stream = async_stream::stream! {
        // ... loop ...
                for ev in tail.read_new(&path) {
                    let data = serde_json::to_string(&ev).unwrap_or_default();
                    yield Ok(Event::default().event("message").data(data));
                }
            tokio::time::sleep(Duration::from_millis(STREAM_POLL_MS)).await;
    };
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
```

**Correction to RESEARCH.md (Pattern 2):** do **not** reach for `futures_util::stream::unfold` or `tokio-stream`. `async-stream = "0.3"` is already a runtime dependency (`bauded/Cargo.toml:12`) and both existing SSE handlers use `async_stream::stream!`; `futures-util` is a **dev-dependency only** (`Cargo.toml:36`), so using it in a handler would require promoting it to a runtime dep for no gain. The `/events` body is the same `async_stream::stream!` block with `rx.recv().await` in place of the file tail:
```rust
let mut rx = lock(&state).subscribe_events();
let stream = async_stream::stream! {
    loop {
        match rx.recv().await {
            Ok(ev) => yield Ok(Event::default().event("rows_removed").data(ev.json())),
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
};
Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
```
Note the existing handlers name the event `"message"`; D-06 names this one `rows_removed`, so the PWA must use `addEventListener("rows_removed", …)`, not `onmessage` (which only fires for unnamed/`message` events).

**`Event` imports are already present** (`:10`): `use axum::response::sse::{Event, KeepAlive, Sse};` — and `std::convert::Infallible` (`:4`), `std::time::Duration` (`:6`).

---

### `bauded/src/api.rs` — `GET /rows` (route, request-response)

**Analog:** `list_sessions` (`:151-153`) — the whole handler:
```rust
async fn list_sessions(State(state): State<Shared>) -> Json<Vec<SessionInfo>> {
    Json(lock(&state).list())
}
```
`/rows` is the same single line against a new `Manager` projection method. No `ApiError` — a read projection of owned state cannot fail.

---

### `bauded/src/manager.rs` — `preview_removal` / `remove_rows` (service, CRUD + pub-sub)

**Primary analog:** `baude/src/app.rs:3908-4086` (the Phase 20 TUI path, already proven by Phase 20's tests). **Secondary analog:** `bauded/src/manager.rs:1599-1700` (`confirm_remove_worktree`) for the daemon's own `runtime_checkouts` handling.

**Preview + reservation pattern** (`baude/src/app.rs:3910-3941`):
```rust
let reservation = match self.reserve_removal_target(target) {
    Ok(reservation) => reservation,
    Err(error) => { /* refuse */ return; }
};
let preview = baude_core::removal::preview_removal(&self.repository_state, target);
drop(reservation);
```
`confirm_remove_worktree` shows the daemon-side reservation idiom (`:1603-1606`):
```rust
let _reservation = self
    .repository_reservations
    .reserve(confirmation.repository())
    .map_err(|outcome| lifecycle::RemovalFailure::Inspection(format!("{outcome:?}")))?;
```

**Runtime custody — take** (`baude/src/app.rs:3948-3976`). The daemon's registry differs: it has `runtime_checkouts: HashMap<CheckoutKey, u64>` (`manager.rs:128`) but no `runtime_standalones`; resolve the session index out of `self.sessions` the same way, and look the id up exactly as `confirm_remove_worktree` does (`manager.rs:1608`): `let runtime_id = self.runtime_checkouts.get(&checkout).copied();`
```rust
for row in &report.rows_removed {
    let runtime_id = match row {
        RowKey::Checkout(key) => self.runtime_checkouts.get(key).copied(),
        RowKey::Standalone(key) => self.runtime_standalones.get(key).copied(),
        RowKey::Repository(_) => None,
    };
    let Some(runtime_id) = runtime_id else { continue };
    let Some(index) = self.sessions.iter().position(|session| session.id == runtime_id)
        else { continue };
    runtimes.push(RowRuntime { row: *row, session: self.sessions.remove(index) });
}
```

**Runtime custody — restore on `Err`** (`baude/src/app.rs:3981-3995`):
```rust
fn restore_row_runtimes(&mut self, runtimes: Vec<baude_core::removal::RowRuntime>) {
    for runtime in runtimes {
        // The `runtime_checkouts` entry was never dropped, so the handle slots
        // straight back under its row; the registry keeps id order.
        let session = runtime.session;
        let index = self
            .sessions
            .iter()
            .position(|existing| existing.id > session.id)
            .unwrap_or(self.sessions.len());
        self.sessions.insert(index, session);
    }
}
```

**Recheck comparison (D-04 / WR-01) — copy the three-field comparison verbatim** (`baude/src/app.rs:4016-4028`):
```rust
// D-08: the report is recomputed at confirm time; a difference in what the
// run would do (the rows, the runtimes it kills, the paths it deletes)
// refreshes the modal instead of removing. `cleared` and `already_removed`
// are filesystem probes that would make the recheck flap on advisory state,
// so they are not compared.
let fresh = removal::preview_removal(&self.repository_state, target)?;
let action_changed = fresh.rows_removed != previewed.rows_removed
    || fresh.runtimes_killed != previewed.runtimes_killed
    || fresh.paths_deleted != previewed.paths_deleted;
if action_changed {
    return Ok(RemoveProjectOutcome::Recheck(fresh));
}
```
The REST execute body echoes exactly these three field names (`rows_removed`, `runtimes_killed`, `paths_deleted`) so the PWA can forward the preview response without remapping — this resolves RESEARCH.md assumption A3.

**Core call + registry cleanup + outcome** (`baude/src/app.rs:4029-4086`):
```rust
let result = removal::remove(&mut self.repository_state, target, &mut runtimes, save);
let report = match result {
    Ok(report) => report,
    Err(error) => {
        // D-09: `*state` is untouched and every handle is still in the Vec;
        // put them all back before the modal is reinstalled.
        self.restore_row_runtimes(runtimes);
        return Err(error.into());
    }
};
for row in &report.rows_removed {
    match row {
        RowKey::Checkout(key) => { self.runtime_checkouts.remove(key); }
        RowKey::Standalone(key) => { self.runtime_standalones.remove(key); }
        RowKey::Repository(_) => {}
    }
}
// Handles the core did not consume stay in the registry.
self.restore_row_runtimes(runtimes);
self.persistence_dirty =
    matches!(report.persistence, PersistenceOutcome::SavedDegraded { .. });
```

**Save closure:** the TUI passes a `move |file: &StateFile|` closure with test-failure injection (`baude/src/app.rs:4036-4051`). The daemon equivalent is the manager's own save path; `Manager` already owns `save_checked()` and the `#[cfg(test)] persistence_target_for_test` / `atomic_failure_for_test` fields (`manager.rs:137-140`), which the api tests drive via `persist_at_for_test(&root, &workspace, Some(failure))` (`api.rs:1154-1158`). Build the closure over those, not over a new mechanism.

**Rollback idiom under the lock** (`manager.rs:312-324`) — the one authoritative shape for a state mutation that persists:
```rust
let before = self.manager.repository_state.clone();
candidate.apply(&mut self.manager.repository_state)?;
if let Err(error) = self.manager.save_checked() {
    if error.replacement_committed() {
        return Ok(());
    } else {
        self.manager.repository_state = before;
    }
    return Err(anyhow::Error::new(error));
}
Ok(())
```
Note `baude_core::removal::remove` already owns its candidate/validate/save staging (`removal.rs:441-469`) and leaves `*state` untouched on `Err`, so the daemon wrapper must NOT re-implement the before-clone around the core call — it supplies the save closure and does the registry bookkeeping only. `Manager::create` (`:864-891`) shows the one place the clone-and-roll-back is still written by hand, for comparison.

**Publish-under-lock (D-07):** `broadcast::Sender::send` goes immediately after the `Ok(report)` arm above, still inside the `&mut self` method (the handler holds the `MutexGuard` for the whole call via `lock(&state)`, `manager.rs:106-108`), and its `SendError` (zero receivers) is discarded with `let _ =`.

**Dependency note (resolves RESEARCH.md A1):** `tokio::sync` is already reachable in this crate — `manager.rs:12` has `use tokio::sync::Notify;` and `api.rs:562` uses `tokio::sync::mpsc::channel`. No `Cargo.toml` change for `broadcast`.

---

### `bauded/src/manager.rs` — `SessionInfo.checkout_key` + `/rows` projection (model, transform)

**Analog:** `SessionInfo` (`:181-230`) and its builder `session_info()` (`:2616-2683`).

**Field conventions** (`:205-214`) — flat optionals with a comment naming the back-compat reason:
```rust
/// Flat optionals (not a nested object) so an older PWA/TUI client just
/// ignores them and `#[serde(default)]` back-compat on `RemoteInfo` is free.
pub rate_5h_used_pct: Option<u8>,
pub rate_5h_resets_at_unix_s: Option<u64>,
pub branch: Option<String>,
pub cwd: String,
pub repo_root: String,
pub is_worktree: bool,
```
`SessionInfo` is `#[derive(Serialize, Clone)]` (`:181`) — add `checkout_key: Option<CheckoutKey>` as one more flat optional and populate it in `session_info()`, whose every field is a plain expression off `&Session` (`:2618-2682`). Because `session_info` takes only `&Session` while `runtime_checkouts` lives on `Manager`, the key has to be threaded in by the caller (`list`/`info`, `:2393-2397`) or looked up there — the planner must pick one and say which.

**Compatibility constraint, with the exact test that enforces it** (`api.rs:1002-1010`): the flat `/sessions` projection test asserts the array elements carry **none** of:
```rust
for forbidden in [
    "repository",
    "repository_key",
    "checkout",
    "checkout_key",
    "parent",
    "children",
    "hierarchy",
    "worktrees",
```
**This is a direct conflict with D-09** (`extend SessionInfo with its checkout_key`). `flat_session_api_remains_a_non_hierarchical_compatibility_projection` fails the moment `checkout_key` is serialized on `SessionInfo`. The planner must resolve it explicitly — either amend that forbidden list (narrowing the test to hierarchy-shaped keys only: `parent`, `children`, `hierarchy`, `worktrees`) with a comment recording why a leaf key is not hierarchy, or carry `checkout_key` only on the `/rows` projection type. Do not let an executor discover this at test time.

---

### `bauded/src/manager.rs` — admission suppression guard (service, CRUD)

**Analog (library side):** `baude-core/src/suppression.rs` — signatures:
```rust
pub fn is_suppressed(root: &Path, path: &Path) -> bool            // :76
pub fn lift(root: &Path, path: &Path) -> io::Result<bool>         // :84
pub fn suppress(root: &Path, path: &Path, common_dir: &Path, now_ms: u64) -> io::Result<()>  // :98
pub fn drop_children(root: &Path, common_dir: &Path) -> io::Result<usize>                    // :113
```

**Call-site analog:** `Manager::create` (`:813-831`) — guards run before any effect, and each returns a `MutationError::Domain` built from `anyhow!`:
```rust
pub fn create(
    &mut self,
    repo: &str,
    worktree: Option<&str>,
    name: Option<&str>,
) -> MutationResult<SessionInfo> {
    if self.persistence_blocked {
        return Err(persist::SaveError::before_replacement(anyhow!(
            "daemon persistence is blocked after a state load failure"
        ))
        .into());
    }
    // Guarded: a `~`-prefixed `repo` in a request body would otherwise
    // stat the developer's real home from inside this crate's test binary.
    let repo = persist::expand_tilde(repo);
    let repo = repo.canonicalize().unwrap_or(repo);
    if !repo.is_dir() {
        return Err(anyhow!("not a directory: {}", repo.display()).into());
    }
```
Add the `is_suppressed` check in that guard block (after canonicalization, since suppression keys on the canonical path), and the `lift` call on an explicit user-driven admission. Implicit admission paths to grep and guard: `Manager::restore` (`:496`), `reconcile_legacy_session` (`:155`), `reopen_checkout` (`:2032`). No existing daemon call site exists, so this is the one role-match rather than exact analog — the core helper is the pattern, the guard placement is `create`'s.

---

### `bauded/web/app.js` — remove modal + `/events` subscriber (component, event-driven)

**Analog:** same file. Four excerpts to copy.

**1. `api()` helper — the error shape every handler must branch on** (`:93-105`):
```js
async function api(path, opts = {}) {
  const res = await fetch(path, {
    headers: opts.body ? { "content-type": "application/json" } : {},
    ...opts,
  });
  state.online = true;
  if (!res.ok) {
    const text = (await res.text().catch(() => "")) || res.statusText;
    throw Object.assign(new Error(text), { status: res.status });
  }
  const ct = res.headers.get("content-type") || "";
  return ct.includes("json") ? res.json() : null;
}
```
A 409's JSON body therefore arrives as `e.message` **text** and must be `JSON.parse`d in that one branch (D-14).

**2. Modal — hand-rolled, backdrop-cancel, wired after append** (`:560-587`). Copy this structure exactly; `#modal` CSS already positions it:
```js
function openModal() {
  document.getElementById("modal")?.remove();
  const div = document.createElement("div");
  div.id = "modal";
  div.innerHTML = `
    <form class="box" id="newform">
      <h2>New session</h2>
      ...
      <div class="actions">
        <button type="button" class="ghost" id="cancelnew">cancel</button>
        <button type="submit" class="primary">start</button>
      </div>
    </form>`;
  document.body.appendChild(div);
  div.addEventListener("click", (e) => { if (e.target === div) closeModal(); });
  document.getElementById("cancelnew").onclick = closeModal;
  document.getElementById("newform").onsubmit = createSession;
  document.getElementById("f-repo").focus();
}

function closeModal() {
  document.getElementById("modal")?.remove();
}
```
Gaps the removal modal must fill that this analog does not have: **Escape to cancel** (no `keydown` handler exists anywhere in `app.js` for the modal — D-13 requires one) and **`.danger` styling** (the kill button uses `class="iconbtn danger"`, `:727`).

**3. Mutation + toast + refresh** (`:507-540`). `deleteSession` is also the native-`confirm()` call site that D-13 forbids copying:
```js
async function toggleArchive() {
  const s = session(state.sid);
  if (!s) return;
  try {
    await api(`/sessions/${state.sid}/${s.archived ? "unarchive" : "archive"}`, { method: "POST" });
    toast(s.archived ? "unarchived" : "archived — parked at the bottom");
    refresh();
  } catch (e) {
    toast(`archive failed: ${e.message}`);
  }
}

async function deleteSession() {
  const s = session(state.sid);
  if (!s || !confirm(`Kill session "${s.name}"?`)) return;   // <-- NOT the pattern to copy (D-13)
  ...
}
```
POST with a JSON body (`:549-552`): `api("/sessions", { method: "POST", body: JSON.stringify({ ... }) })`.

**4. SSE subscriber with teardown** (`:236-254`) — the newer of the two, and the one with the CLOSED-state fix:
```js
async function openActivity(sid) {
  state.aesBuffer = [];
  const es = new EventSource(`/sessions/${sid}/activity-stream`);
  state.aes = es;
  es.onmessage = (ev) => {
    if (state.sid !== sid) return;
    const e = JSON.parse(ev.data);
    if (state.aesBuffer) state.aesBuffer.push(e);
    else { state.activity.push(e); render(); scrollActivity(); }
  };
  es.onerror = () => {
    // EventSource auto-reconnects; for a deleted session the reconnect 404s and
    // would churn forever. When the browser has given up (CLOSED), tear it down
    // instead of letting it retry against a session that will never return (WR-02).
    if (es.readyState === EventSource.CLOSED) {
      es.close();
      if (state.aes === es) state.aes = null;
    }
  };
```
Differences for `/events`: it is **daemon-wide**, so it is opened once at boot next to `startTimers(); route();` (`:827-828`) and NOT torn down by `closeStream()` (`:177-186`, which exists to stop per-session streams leaking across views); and because D-06 names the event, use `es.addEventListener("rows_removed", …)` rather than `onmessage`.

**5. State + list render** (`:8-29`, `:596-644`). New keys go on the single `state` object with an inline comment, same as `activity`/`aes`/`pendingPermission`. `renderList` is the `innerHTML` + `querySelectorAll` rewiring idiom and shows the **mandatory `esc()` on every dynamic string** (`:33-37`, `:612-616`); a removal modal renders report paths and branch names, which are file-system data — `esc()` all of them. The visibility/poll reconciliation D-08 keeps is `:798-807` + `:814-820`.

---

### `bauded/web/style.css` — modal scroll box (config)

**Analog:** `#modal` block (`:304-334`) for the shell, and the activity feed for the inner scroll:
```css
#modal .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 18px; }
#modal .actions button { padding: 8px 16px; border-radius: 8px; }
#modal .actions .primary { background: var(--cyan); color: #06262e; font-weight: 700; }
#modal .actions .ghost { color: var(--dim); }
```
The "long child lists scroll inside the modal box" rule (D-16) copies the existing capped-scroll idiom (`:211-212`, also `:147`/`:153`):
```css
  max-height: 30vh;
  overflow-y: auto;
```
Colors come from the existing CSS variables (`var(--cyan)`, `var(--dim)`); the destructive accent should reuse whatever `.danger` already resolves to rather than a new literal.

---

### `baude/src/remote.rs` — remote removal client (service, request-response)

**Analog:** same file. The daemon client is blocking `ureq` on a background thread, not async.

**Mutation call with JSON body + error shortening** (`:252-259`, `:271-279`):
```rust
let body = serde_json::json!({ "repo": repo, "worktree": worktree, "name": name });
ureq::post(&format!("{}/sessions", self.base))
    .timeout(REQUEST_TIMEOUT)
    .set("Content-Type", "application/json")
    .send_string(&body.to_string())
    .map(|_| ())
    .map_err(|e| short_err(&e))

fn short_err(e: &ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, _) => format!("daemon said {code}"),
        ureq::Error::Transport(t) => t
            .message()
            .map(str::to_string)
            .unwrap_or_else(|| "connection failed".into()),
    }
}
```
**Gap the planner must close:** every existing method discards the body (`.map(|_| ())`), and `short_err` discards the response body for a `Status` error. Removal needs the opposite on both counts — the 200 body is the `RemovalReport` to render, and the 409 body is the fresh report. So the new methods deserialize `into_json::<RemovalReport>()` (the typed-JSON idiom is `:151`, `:221`) and the error path must special-case 409 by reading the response out of `ureq::Error::Status(409, response)` before falling back to `short_err`.

**Background polling thread + shared snapshot** (`:139-192`) is the analog for the `/events` subscriber: a `std::thread::spawn` loop writing into an `Arc<Mutex<_>>` the UI reads via `snapshot()` (`:194-196`). An SSE consumer is the same thread shape reading a streaming response instead of sleeping on `POLL_SECS`.

---

### `baude/src/app.rs` — remote `D` arm (controller, request-response)

**Analog:** the local `Modal::ConfirmRemove` keymap arm (`:5795-5867`) — the three-outcome match the remote path must reproduce:
```rust
match self.confirm_remove_project(target, &report) {
    Ok(RemoveProjectOutcome::Removed(report)) => {
        self.set_message(Self::removal_notice(&report));
    }
    Ok(RemoveProjectOutcome::Recheck(fresh)) => {
        self.modal = Modal::ConfirmRemove {
            target, report: fresh, scroll: 0, error: None,
        };
        self.set_message(
            "rows changed since the preview; review the refreshed confirmation".into(),
        );
    }
    Err(error) => {
        // D-09: rows and handles are intact; keep the
        // modal open with the typed error for retry.
        self.modal = Modal::ConfirmRemove {
            target, report, scroll, error: Some(error.to_string()),
        };
        self.dirty = true;
    }
}
```
A 409 from the daemon maps to the `Recheck` arm, a 500/503 to the `Err` arm. Scroll handling (`:5837-5865`) and `n`/`Esc` cancel (`:5836`) are reused unchanged.

**Refusal sites to replace** (resolves RESEARCH.md A5 — three sites, all confirmed on this branch):
```rust
// :299-300  enum variant doc + name
    /// `D` on a remote row: remote removal arrives with the daemon (Phase 21).
    RemoteRemove,

// :343-350  the dispatch that refuses
        return match view.kind {
            ActionKind::Remote => SidebarAction::Refuse(SidebarRefusal::RemoteRemove),
            ActionKind::Repository
            | ActionKind::Main
            | ActionKind::Managed
            | ActionKind::External
            | ActionKind::Standalone
            | ActionKind::Unavailable => SidebarAction::RemoveProject,

// :1328-1330  the message
            SidebarRefusal::RemoteRemove => format!(
                "Cannot remove “{target}”: remote rows are removed from the daemon and PWA in a later release. Nothing was changed."
            ),
```
Removing the variant means the `ActionKind::Remote` arm routes to a new remote action instead, and the `SidebarRefusal` match at `:1328` loses an arm (the match is exhaustive, so the compiler names every site).

**Modal section parity:** the PWA modal (D-13/D-16) must render the same sections in the same order as the TUI generator `baude/src/ui.rs:2509-2617`: title `Remove this {kind}?` → `rows dropped (N)` with every row named (`+N more` only when clipped) → `runtimes killed (N)` with `pid X shell Y` per row, `none` when empty → `paths deleted (N)` / `none` → `paths kept (N)` / `none` → `already removed (N)` only when non-empty → `cleared: N bindings, N markers, N suppression entries` only when non-empty → `error: {text}` when present. The "a path is never shown under deleted unless the report lists it there" rule falls out of reading `report.paths_deleted` directly, as that generator does.

---

### `bauded/src/api.rs` tests (test, request-response)

**Analog:** the api tests module (`:689-1180`).

**Mandatory isolation owners** — every case binds a named owner first, enforced by `#[must_use]` messages and a runtime assert:
```rust
// :751-757
#[must_use = "the leading ApiScope isolates this router; bind it to a named local that \
              outlives every handler await"]
fn app() -> (ApiScope, axum::Router) {
    let scope = api_scope("router");
    let router = super::router(Arc::new(Mutex::new(Manager::new("sleep 30".into(), false))));
    (scope, router)
}

// :936-954
fn exited_tracked_manager(repo: &Path) -> (crate::manager::Shared, u64) {
    assert!(
        baude_core::testing::workspace_override().is_some(),
        "exited_tracked_manager requires a caller-owned isolation scope: bind an \
         initialized_repo (or api_scope) owner before calling it"
    );
    let state = Arc::new(Mutex::new(Manager::new("true".into(), false)));
    let id = crate::manager::lock(&state)
        .create(repo.to_str().unwrap(), None, None)
        .unwrap()
        .id;
    crate::manager::lock(&state).track_runtime_for_test(id);
    ...
}
```

**Persisting-manager case shape** (`:964-998`) — the template for every `/removals` test, since removal needs `persist: true`:
```rust
let root = std::env::temp_dir().join(format!("bauded-flat-api-compatibility-{}", std::process::id()));
let _ = std::fs::remove_dir_all(&root);
std::fs::create_dir_all(&root).unwrap();
let fixture = initialized_repo(&root, "repo");
let repo = fixture.path().canonicalize().unwrap();
let workspace = baude_core::workspace::resolve(
    Some("claude"), None, &baude_core::persist::Config::default(), |_| {},
);
let state = Arc::new(Mutex::new(Manager::new("sleep 30".into(), true)));
crate::manager::lock(&state).persist_at_for_test(&root, &workspace, None);
let app = super::router(Arc::clone(&state));

let create = app.clone().oneshot(post_json(
    "/sessions",
    &serde_json::json!({ "repo": repo, "name": "flat-main" }).to_string(),
)).await.unwrap();
assert_eq!(create.status(), StatusCode::CREATED);
let created = body_json(create).await;
```
Helpers available: `get(path)` (`:764`), `post_json(path, json)` (`:768`), `body_json(res)` (`:759`), `git(repo, args)` (`:775`), `initialized_repo` / `initialized_repo_in_workspace` (`:815`, `:823`).

**503 / failure-injection case** (`:1114-1175`) — the pattern for the 503 and the runtime-restore tests:
```rust
async fn assert_status(
    response: axum::response::Response,
    expected: StatusCode,
    leg: &str,
    failure: &baude_core::persist::AtomicFailure,
) {
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(status, expected, "{leg} under {failure:?}: body={}", String::from_utf8_lossy(&body));
}

for (failure, committed) in [(AtomicFailure::Rename, false), (AtomicFailure::DirectorySync, true)] {
    ...
    crate::manager::lock(&create_state).persist_at_for_test(&create_root, &workspace, Some(failure));
    let response = super::router(Arc::clone(&create_state))
        .oneshot(post_json("/sessions", r#"{"repo":"/tmp"}"#)).await.unwrap();
    assert_status(response, StatusCode::SERVICE_UNAVAILABLE, "POST /sessions", &failure).await;
    assert!(crate::manager::lock(&create_state).list().is_empty());
```
Pitfall-2 coverage (runtime handles not leaked on error) is this plus `assert!(!lock(&state).list().is_empty())` after the 503.

**Wave-0 gap with no analog:** nothing in the repo reads a frame off an SSE response body — both SSE handlers are untested through `oneshot`. The `/events` test needs a new helper (read the first `data:` frame off `res.into_body()` with a timeout). The nearest shapes to borrow from are `body_json` (`:759-762`, `BodyExt::collect`, which would hang on an infinite stream, so it cannot be reused as-is) and the PTY websocket test module's live-socket driving (`pty_ws_tests`, referenced at `:709-711`).

## Shared Patterns

### State access and locking
**Source:** `bauded/src/manager.rs:106-108`, used by every handler in `api.rs`
**Apply to:** every new handler
```rust
pub fn lock(shared: &Shared) -> MutexGuard<'_, Manager> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}
```
`Shared = Arc<Mutex<Manager>>`. Handlers are `async` but the guard is a std `MutexGuard` — never hold it across an `.await`. This is exactly why D-07's publish-under-lock works and why the broadcast `send` must be inside the `Manager` method rather than in the handler after the call returns.

### Error mapping
**Source:** `bauded/src/api.rs:104-118`
**Apply to:** every new handler
Plain-text `(StatusCode, String)`, built by a single named mapping function per error type, never `Json(err)` (`RemovalError` is not `Serialize`). The 409 drift body is the documented single exception.

### SSE construction
**Source:** `bauded/src/api.rs:608-643` and `:652-687`
**Apply to:** `GET /events`
`async_stream::stream!` + `Sse::new(stream).keep_alive(KeepAlive::default())`, returned as `Result<Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>, ApiError>`. `async-stream` and `futures-core` are runtime deps; `futures-util` is dev-only.

### PWA XSS discipline
**Source:** `bauded/web/app.js:33-37`, applied at `:612-616`, `:709-717`
**Apply to:** the removal modal and the `/rows` hierarchy render
```js
function esc(s) {
  return String(s).replace(/[&<>"']/g, (c) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  })[c]);
}
```
The permission card comment (`:704-706`) is the precedent: "esc() EVERY dynamic string before innerHTML (XSS, T-04-13)". Report paths, branch names and clearing error strings are all dynamic.

### Test isolation
**Source:** `bauded/src/api.rs:703-757`, `:784-850`, `:931-954`
**Apply to:** every new test
A named `ApiScope` or `FixtureRepo` owner bound before the router is built and held across every `await`. Real `git init` fixtures, real `Manager`, `oneshot` through the router — no mocks and no source greps (repo CLAUDE.md).

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| SSE frame-reading test helper (in `bauded/src/api.rs` tests) | test | streaming | No existing test reads an SSE response body; `body_json`'s `collect()` would hang on an open stream |
| PWA removal modal keyboard (Escape) handling | component | event-driven | `app.js` has no modal `keydown` handler anywhere — D-13's Escape-cancel has no precedent in this file |
| Daemon-side suppression call site | service | CRUD | `baude-core/src/suppression.rs` is used by the TUI only; `Manager::create`'s guard block is the placement analog, not a suppression analog |

## Conflicts the Planner Must Resolve

1. **D-09 vs. an existing passing test.** Adding `checkout_key` to `SessionInfo` breaks `flat_session_api_remains_a_non_hierarchical_compatibility_projection` (`bauded/src/api.rs:1002-1010` lists `"checkout_key"` as forbidden) — the same test CONTEXT.md D-09 promises keeps passing. Pick: narrow the forbidden list to hierarchy-shaped keys, or keep `checkout_key` off `/sessions` and only on `/rows`.
2. **RESEARCH.md Pattern 2 names the wrong adapter.** `futures_util::stream::unfold` is dev-only in this crate; `async_stream::stream!` is the runtime-dep convention of both existing SSE handlers. (Also resolves A1: `tokio::sync` already compiles here, and A2: `tokio-stream` is not needed.)
3. **`baude/src/remote.rs` discards response bodies everywhere.** Both new remote methods need typed 200 bodies and a 409 body read out of `ureq::Error::Status(409, response)`; `short_err` cannot be reused unchanged for the removal calls.

## Metadata

**Analog search scope:** `bauded/src/`, `bauded/web/`, `baude/src/`, `baude-core/src/`, `bauded/Cargo.toml`
**Files scanned:** 10 (all git-tracked)
**Pattern extraction date:** 2026-11-06
