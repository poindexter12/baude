# Cross-AI Plan Review — Phase 21: Daemon and PWA Parity

**Review mode:** source-grounded. Every `file:line` below was opened and checked against the plan text. Where a claim could not be settled from source it is labelled an open question rather than asserted.

---

## 21-01

### 1. Summary

A well-anchored tracer. The plan wraps `baude_core::removal::{preview_removal, remove}` in two `Manager` methods and two routes, borrowing the Phase 20 TUI's custody/recheck/rollback posture wholesale. Its riskiest compile-time assumption — that the core report can be used as an axum *extractor*, not just a response — holds: `RemovalReport` derives `Deserialize` at `baude-core/src/removal.rs:179`, as do every nested type it contains (`KilledRuntime` :130, `PersistenceOutcome` :139, `ClearedItem` :162, `ClearingFailure` :171). So `Json<RemovalReport>` works and neither this plan nor 21-06 needs to touch `baude-core`, which is why both can keep their narrow `files_modified`. The main gap is an unmapped-in-practice error class (`RuntimeHandleMissing`) that is reachable on the daemon specifically, and which no plan in the phase drives.

### 2. Strengths

- **The `RemovalError` mapping is exhaustive and spelled correctly.** Plan line 81 names `UnknownTarget`, `Persist`, `RuntimeSurvived`, `DiskRemoval`, `RuntimeHandleMissing`, `RuntimeIdentityMismatch`, `Validation` — exactly the seven variants declared at `baude-core/src/removal.rs:235-258`, with no variant left to fall through to a wrong default. `RemovalError` is `Debug + Display` only (no Serde, verified :235), which is why the plan uses Display for every plain-text body — correct.
- **The `Busy → 503` addition is wired to a real mechanism.** `RepositoryReservations::reserve` is at `baude-core/src/lifecycle.rs:2839` and does return `Err(LifecycleOutcome::Busy { repository })` when the repository is already held (:2843-2846), and `Manager` already owns a `repository_reservations: RepositoryReservations` field (`bauded/src/manager.rs:129`). The reasoning for 503-not-409 ("D-03 reserves 409 bodies for the fresh-report JSON") is sound and keeps the one-JSON-error-body rule intact.
- **Both save-closure seam names are right, on both sides of the fence.** The TUI's is `persistence_root_for_test` (`baude/src/app.rs:666`, used in the removal save closure at `:4033`); the daemon's is `persistence_target_for_test: Option<(PathBuf, String)>` (`bauded/src/manager.rs:138`). The plan uses the first in its read_first (line 67) and the second in its action (line 79). `persist::save_current_at_test` (`baude-core/src/persist.rs:473`) and `persist::save_for_workspace_status` (:1137) both exist as named.
- **`removals_preview_leaves_the_state_file_untouched` is provable, not aspirational.** Core's module doc (`removal.rs:20-30`) states preview "never signals a process, never runs git and never writes a file", with read-only `symlink_metadata` probes plus a suppression-store read. Byte-identical state file is the right assertion.
- **Test-harness citations are exact.** `api_scope` :721, `app` :753, `body_json` :759, `post_json` :768, `initialized_repo` :815 (plan cites 721-830 ✓); `exited_tracked_manager` :936 (cites 936-955 ✓); `flat_session_api_remains_a_non_hierarchical_compatibility_projection` :965 (cites 965-1040 ✓). `restore_at` is at `manager.rs:511` and is private, which Task 2 correctly anticipates.
- **T-21-01's disposition is honest rather than decorative**: the daemon derives paths from `repository_state` alone and compares the echoed sets only.

### 3. Concerns

- **MEDIUM — `RuntimeHandleMissing` is reachable on the daemon and nothing covers it.** `preflight` requires "one handle per row, no handle for a row without a persisted runtime" (`removal.rs:426-429` region; variant doc at :245-248). `collect_row_runtimes` resolves handles through `runtime_checkouts` (`manager.rs:128`), which only maps checkouts the daemon spawned. But `restore_loaded` (`manager.rs:526-570`) loads persisted state verbatim and then only *reopens* checkouts matching its lifecycle filter — a row whose `reopen_checkout` returns `Err` logs and continues (`:562`), and `Ok(_)` non-Reopened outcomes are silently skipped (`:561`). Such a row stays in `repository_state` with no `runtime_checkouts` entry. If its persisted `owned_runtime` is still set, removal returns `RuntimeHandleMissing` → a 500 with developer-facing text and no client recourse. **Open question:** whether `reconcile_teardown_recoveries` (`:541`) or exit detection clears the stale `owned_runtime` on that path — I could not settle that from the ranges I read. Either way, the plan does not say, and no test in 21-01 or 21-03 drives it.
- **MEDIUM — `PersistenceOutcome` is internally tagged; the assertion is loose.** `#[serde(rename_all = "snake_case", tag = "outcome")]` at `removal.rs:139-146` means the JSON is `{"outcome":"saved"}` and `{"outcome":"saved_degraded","detail":"…"}`, not a bare `"saved"`. Task 1's "`persistence` equal to the serialized `saved` outcome" (line 73) is vague enough to be written wrong first time, and 21-05's fixtures inherit the same shape.
- **LOW — preview holds no reservation, which reads as a contract violation until you work it out.** Core states callers hold `RepositoryReservations` "for every repository in the set from before `select_rows` until this function has returned" (`removal.rs:461-464`). `Manager::preview_removal` is a bare `&self` delegate. In practice this is fine — it runs under the global `Arc<Mutex<Manager>>` (`manager.rs:38`, `lock()` at `:106`), so it is already atomic against every other daemon mutation, and `remove_rows` reserves *before* its recheck and before core's internal `select_rows`. But an executor reading core's doc comment will think the preview path is non-compliant. One sentence recording why it is safe would prevent a wrong "fix".

### 4. Suggestions

- Add a fourth test driving `RuntimeHandleMissing` (seed a checkout row with a persisted `owned_runtime`, drop the `runtime_checkouts` entry via a `#[cfg(test)]` seam) and decide whether its status should be 409/503 "retry after daemon restart" rather than a bare 500.
- Pin the `persistence` assertion to `json!({"outcome":"saved"})` so 21-05's fixtures have a verified shape to copy.
- Record in the SUMMARY the one-line rationale for the unreserved preview, citing `removal.rs:461` and `manager.rs:106`.

### 5. Risk Assessment

**LOW-MEDIUM.** The load-bearing assumptions verify; the contract is narrow; the fixture harness it copies already exists and already isolates. The one real gap (`RuntimeHandleMissing`) degrades to an ugly 500 rather than data loss.

---

## 21-02

### 1. Summary

The widest plan in the phase and the one whose source grounding is most impressive — its four `git::discover_repository` call-site line numbers are exact, and its instruction to record a *negative* finding if none of them admits implicitly is a genuinely good anti-hallucination device. The suppression test isolation question (does the daemon test harness redirect `config_dir()`?) resolves in the plan's favour. Two issues: a serde attribute placed on the wrong struct, and an unstated path-keying hazard that will make the suppression assertion fail in a confusing way.

### 2. Strengths

- **The test-isolation claim is verified.** `api_scope` (`bauded/src/api.rs:721`) installs `baude_core::testing::TestRedirect::new(&root)` plus a workspace override (`:731-740`), and `persist::config_dir()` honours that redirect — proven by core's own `config_dir_honours_redirect` test at `baude-core/src/persist.rs:1265-1272`. So Task 3's "`persist::config_dir()` is redirected by `api_scope`, so the tests read the store under the test root" (line 112) is **correct**, and the suppression tests will not write the developer's real `~/.config/baude`. The harness was built for exactly this: `api_scope` is `#[must_use]` (`:742`) and `app()` carries the comment "Returning the bare router would compile and run the case against the developer's real workspace and data dir" (`:789-791`).
- **The two-checkout fixture is constructible exactly as written.** `worktree: Option<String>` is a real `/sessions` body field (`api.rs:169`) forwarded as `create(&body.repo, body.worktree.as_deref(), …)` (`:178`).
- **Every implicit-admission pointer is exact.** `git::discover_repository` callers at `manager.rs:921, 1073, 1347, 1418` — all four correct. `reconcile_legacy_session` at `:155-156`, correct. `Manager::create` at `:813`, `record_checkout_intent` at `:1065` — correct. `list` :2393 / `info` :2397 / `session_info` :2616 — correct.
- **The suppression API is as described.** `is_suppressed` (`suppression.rs:76`), `lift` (`:84`), `suppress` (`:98`), `drop_children` (`:113`). `lift` already returns `Ok(false)` without writing when absent (`:84-96`), so the "advisory, never fails the mutation" posture is the function's own contract, not something the daemon must bolt on. The TUI template the plan tells the executor to grep is real: `App::lift_suppression` (`baude/src/app.rs:2374`) and the sweep guard `suppression::is_suppressed(&persist::config_dir(), &record.path)` (`:2604`).
- **`seed_standalone_for_test` is buildable from `bauded`.** `SavedStandaloneSession::new` is `pub` (`baude-core/src/repository.rs:464`) and `RetainedStandaloneSessionState` has all-`pub` fields (`:311-322`). The cited "test-only constructor shape" at repository.rs:1225-1245 is real — `fn standalone(...)` at `:1229` matches the plan's description verbatim — and the plan correctly treats it as a *shape to copy* (it lives in core's own `#[cfg(test)] mod tests`, so it is not callable cross-crate).

### 3. Concerns

- **MEDIUM — `#[serde(default)]` on `SessionInfo.checkout_key` is on the wrong struct.** `SessionInfo` is `#[derive(Serialize, Clone)]` (`manager.rs:180-181`) — it never deserializes, so `default` buys nothing there. The attribute that actually delivers back-compat is on the *client* type, `RemoteInfo` in `baude/src/remote.rs:21`, which already uses `#[serde(default)]` on eight fields for precisely this reason (`:33-77`, each with a back-compat comment). 21-06 Task 1 gets it right (plan line 63). 21-02's stated rationale — "for symmetry with the TUI's `RemoteInfo`" (line 66) — misreads the mechanism: these are two different types on opposite ends of the wire. At best inert, at worst a serde derive error.
- **MEDIUM — the suppression assertion's path keying is unspecified and will fail confusingly.** `is_suppressed` is an **exact `folder_key` match** — "a parent or sibling entry does not suppress it" (`suppression.rs:74-78`). The test asserts `is_suppressed(&config_dir(), &worktree_path)` but never says which path form core keyed. Core plans one `SuppressionAdded` per checkout row whose parent repository survives (`removal.rs:330-333` doc), keyed off the row's observed path. If the test uses a canonicalised or symlink-resolved variant, the lookup returns false and reads as "the write side is broken" rather than "the test used the wrong path".
- **LOW — the parent-removal GC half is asserted backwards.** Task 3 asserts that removing the repository "adds no entry for the repository path", but core also plans `SuppressionDropped` per repository row that has children in the store (`removal.rs:331`), i.e. removing the parent *garbage-collects* the child entry via `drop_children` (`suppression.rs:113`). The user-visible behaviour — the previously-suppressed worktree entry is now gone — is never asserted.
- **LOW — 21-05 depends on a SUMMARY that does not exist yet.** The `/rows` field names are declared a wire contract with `reversibility: costly` (correct), but their only record is "note the `/rows` field names verbatim" in the 21-02 SUMMARY (line 151), which 21-05 Task 2 read_firsts (line 93). Fine in wave order; brittle if that SUMMARY is terse.

### 4. Suggestions

- Move the `#[serde(default)]` instruction out of 21-02 and note that 21-06 owns the deserialize side.
- Name the exact path form in the suppression assertion — read it back out of the preview report's `cleared` entry (`ClearedItem { kind: SuppressionAdded, path }`, `removal.rs:162-166`) rather than reconstructing it in the test.
- Add the GC assertion: after the parent removal, `is_suppressed(&config_dir(), &worktree_path)` is `false`.
- Put the `/rows` field list in the plan itself as a code block, not only in a future SUMMARY.

### 5. Risk Assessment

**MEDIUM.** Three tasks, three files, a new wire contract, and a write into a store shared with the TUI. Isolation is verified and the admission audit is well-specified; the risk is concentrated in Task 3, where a path-keying mismatch costs debugging time, not data.

---

## 21-03

### 1. Summary

The failure-posture plan, and the one most faithful to the project's test rules ("keep every assertion on observable state … never on log text"; "No test asserts by grepping source"). Each of 404/409/500/503 gets a real driver rather than a stubbed error, and the injection seams it needs are reachable. The significant weakness is that its headline security test does not prove the property its threat register claims.

### 2. Strengths

- **The kill-failure seam genuinely is reachable cross-crate.** `fail_next_teardown_for_test` is `pub` behind `#[cfg(debug_assertions)]` at `baude-core/src/pty.rs:912-914` — *not* `#[cfg(test)]` — so `bauded`'s debug-profile tests can call it. Combined with the daemon-side `atomic_failure_for_test` (`manager.rs:140`, set via `persist_at_for_test` `:2514`) and `persistence_blocked` (`:131`), all three failure classes have a real injection point.
- **`removals_execute_ignores_unknown_fields` is sound.** `RemovalTarget` (`removal.rs:48-49`), `RowKey` (`:57-58`) and `RemovalReport` (`:179-180`) carry no `deny_unknown_fields`, so serde does drop extras, and the test's framing — "no field other than `target`, `rows_removed`, `runtimes_killed`, `paths_deleted` influences behaviour" — is the right contract to state.
- **The 500 body assertion will hold.** `RemovalError::RuntimeSurvived`'s Display is `"row {row:?}: runtime survived the kill stage: {error}"` (`removal.rs:272-274`), so asserting the body names the row and "survived" is safe.
- **The retry-after-cause-clears shape is borrowed from a path already proven.** Phase 20's `remove_project_failure_kill_keeps_rows_and_handles_then_retry_succeeds` is cited as the template, and all four Phase 20 SUMMARYs exist on disk, so the recipe is real.

### 3. Concerns

- **MEDIUM — `removals_bogus_paths_change_nothing` proves the weaker of two properties.** The test expects **409** (plan line 64). Replacing `paths_deleted` with a sibling path makes the echoed set differ from the fresh preview, so *drift* fires and the body is rejected before any path comparison matters. That proves "a tampered body is rejected" — true and worth having — but it does **not** prove T-21-05's actual claim that "echoed sets are compared, never executed". The property that needs proving is that a body which passes the drift check still cannot influence the filesystem: execute with the three sets matching exactly but `paths_kept`, `already_removed`, `cleared` and `persistence` replaced with garbage pointing at a real sibling directory, assert **200**, and assert the sibling still exists.
- **MEDIUM — the kill-seam citation points at a usage example, not the API.** Plan line 81 cites "`baude-core/src/removal.rs` lines 2255-2300 (`fail_next_teardown_for_test` seam on the pty)". The function is at `pty.rs:914`; `removal.rs:2264` and `:2314` are call sites *inside core's own `#[cfg(test)] mod tests`*, on a pty the executor cannot reach from `bauded`. The plan's own fallback ("add a `#[cfg(test)] Manager::session_pty_for_test(id)` accessor only if neither is reachable") rescues the task, so this costs context, not correctness.
- **MEDIUM — `removals_drift_body_is_accepted_on_resend` codifies replay as a feature without saying so.** The 409 body is a *preview* report (`planned: true`); resending it executes. That is the intended contract, but it means a captured 409 body stays executable indefinitely, because the only freshness check is set equality. Benign on a tailnet; it should be recorded in the SUMMARY as deliberate idempotency rather than left implicit.
- **LOW — three of the seven error arms are mapped but never driven.** `RuntimeHandleMissing`, `RuntimeIdentityMismatch` and `Validation` all route to 500 in 21-01 with no test here. `RuntimeHandleMissing` is the reachable one (see 21-01).
- **LOW — the 500 body is developer-facing.** `{row:?}` formatting (`removal.rs:260-281`) yields text like `Checkout(CheckoutKey(3))`. 21-05 D-15 renders `e.message` verbatim in the user's modal and 21-06 does the same in the TUI. Fine as a daemon contract; see the cross-plan note.

### 4. Suggestions

- Split the tampering test in two: keep the 409 drift case, and add the 200 case above as the real T-21-05 evidence.
- Repoint the seam citation to `baude-core/src/pty.rs:912-914` and note the `debug_assertions` gate explicitly, since a release-profile test run would lose it.
- Add the `RuntimeHandleMissing` case here rather than deferring it.

### 5. Risk Assessment

**LOW.** Tests-only plus minimal `#[cfg(test)]` seams, over code 21-01 already landed. The worst outcome is a security claim that outruns its evidence — which this review flags before it reaches a SUMMARY.

### Note

The `RemovalReport` serde shape the plan relies on is confirmed: `cleared: Vec<ClearedItem>` and `clearing_failures: Vec<ClearingFailure>` (`removal.rs:191-197`), no `clearings` field, exactly as 21-05 states.

---

## 21-04

### 1. Summary

The sharpest plan in the set. It catches a factual error in its own RESEARCH, cites the dependency line to the number, and picks the only streaming approach that actually compiles in this crate. Its two weaknesses are both of the same kind: two tests claim to prove mechanisms their fixtures cannot reach, and in both cases the *safe consequence* is what gets proven. That is a documentation problem, not a correctness one, but the `must_haves` truths are written as if the mechanisms were proven.

### 2. Strengths

- **The Cargo claim is exact and RESEARCH is correctly overridden.** `bauded/Cargo.toml:27` is `tokio = { version = "1", features = ["macros", "net", "rt-multi-thread", "signal", "time"] }` — `"sync"` is genuinely absent. The plan says so *at the line* (read_first line 61: "line 27 … sync is absent and must be added explicitly; RESEARCH A1 is not relied on"), the feature is the only Cargo change, and `bauded/Cargo.toml` is correctly in `files_modified`.
- **`async_stream` is not just a stylistic match — it is the only option.** `async-stream = "0.3"` and `futures-core = "0.3"` are `[dependencies]`; **`futures-util = "0.3"` is a `[dev-dependencies]` entry only.** RESEARCH's `futures_util::stream::unfold` sketch would not compile in a production handler. The plan sidesteps it by mirroring the two existing handlers, which do exactly `async_stream::stream! { … }` returning `Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>` with `KeepAlive::default()` (`api.rs:608-642` `stream`, `:652-686` `activity_stream` — the read_first range "600-690" lands on both). Right answer; the plan just never says RESEARCH was wrong here.
- **No new test dependency either.** `http_body_util::BodyExt` is already imported in the test module (`api.rs:697`), and `tower::ServiceExt` (`:698`) is there for `oneshot`.
- **Subscribing before building the stream closes the lost-event race**, and the plan states that as the reason (line 69) rather than leaving it as incidental ordering.
- **The D-07 publish point is correct for the right reason.** Core's `remove` replaces in-memory state and consumes handles "only after that save returns `Ok` or reports that the replacement file was committed" (`removal.rs:426-429`), so an event sent on the `Ok` arm after `remove` returns always post-dates durability. Capacity 64 + skip-on-lag is justified by the clients' existing polls, and T-21-07's "bounded capacity … so a slow subscriber cannot grow daemon memory" is accurate.

### 3. Concerns

- **MEDIUM — `events_published_after_state_file_dropped_the_row` cannot prove the in-lock send.** It asserts that "at the moment the `rows_removed` message is read … `GET /sessions` already omits it" (line 88). But `GET /sessions` takes the same `Arc<Mutex<Manager>>` (`lock()`, `manager.rs:106`), so the test thread cannot observe anything *while* the guard is held — it necessarily reads post-guard state. The test proves the safe consequence (no event ever precedes the drop), which is the property that matters; the `must_haves` truth at line 21 ("inside the same lock, so a `GET /sessions` that observes the event can never list the removed row") is stronger than the fixture. Assert the consequence and prove the in-lock placement structurally (the `send` sits on the `&mut self` path before the guard drops).
- **MEDIUM — `events_lagged_subscriber_keeps_streaming`'s second clause is untested by its own fixture.** It drives `Manager::subscribe_events()` *directly* and asserts `Lagged` then `Ok` — which tests `tokio::sync::broadcast`, not the handler. The clause "and the `/events` route … yields a message after the lag rather than ending" (line 89) needs the route's body in the picture: overflow a receiver the handler owns and read a frame off the response body. As written, the handler's `continue`-on-`Lagged` is asserted by inspection only.
- **LOW — `events_stream_has_keep_alive_and_no_history` will not see a keep-alive.** `KeepAlive::default()` in axum emits its comment on a ~15 s interval, so within the test's 500 ms window the realistic observation is *zero bytes*, not "the first bytes, if any, are a keep-alive comment" (line 66). Reword to "the 500 ms timeout elapses with no complete SSE message" and tolerate an empty read.
- **LOW — "`/stream` and `/activity-stream` are untouched" is a `must_haves` truth with no test.** Line 22 claims it; the verification block only says they "still pass". Both have existing tests — name them in the verify command so the claim is backed.

### 4. Suggestions

- Add one sentence to the SUMMARY recording that RESEARCH's `futures_util::stream::unfold` pattern is unusable here because `futures-util` is a dev-dependency, so Phase 22 does not re-derive it.
- Reword the two over-claiming `must_haves` truths to match what the fixtures prove, and move the in-lock property to a code comment at the `send` site.
- Name the existing `stream`/`activity_stream` tests in Task 2's verify chain.

### 5. Risk Assessment

**LOW.** One feature flag on an already-vendored crate, one enum, one channel, one route built from a pattern that exists twice in the same file. The failure mode of a mis-sequenced publish is a transiently stale client row that the 3 s poll reconciles.

---

## 21-05

### 1. Summary

The most ambitious plan — five files, a new test runtime for the repo, and the only `human-check` gate in the phase — and the one that most clearly reads source rather than inheriting CONTEXT's wording. It is also the only plan whose verify commands violate an explicit project rule, twice, and its headline security mitigation (T-21-09, XSS) has no automated evidence at all because the test it points to exercises the wrong layer.

### 2. Strengths

- **It corrects the contract against source instead of trusting CONTEXT.** Line 67: "there is no `clearings` field and no `ClearingResult` type, so the view model reads `report.cleared` and `report.clearing_failures`." Verified: `RemovalReport` has `cleared: Vec<ClearedItem>` and `clearing_failures: Vec<ClearingFailure>` (`removal.rs:191-197`) and no `clearings`. 21-CONTEXT.md D-02 and D-13 both say "clearings" loosely. This is the behaviour a plan should show.
- **The `already_removed` rule matches core's own semantics.** `DiskAction::AlreadyRemoved` is documented as a "Reporting label only" (`removal.rs:~155`) and `already_removed: Vec<PathBuf>` is its own field (`:186`), so "it is never shown under paths deleted" is a real invariant, not a UI preference.
- **Every PWA anchor is real and tight.** `esc()` at `app.js:33` — so T-21-09 names an existing helper rather than a wish. `toast` `:86`, `api` `:93` (cites 85-105 ✓); `refresh` `:137` (cites 137-150 ✓); `openModal` `:560` / `closeModal` `:585` (cites 520-600 ✓); the per-session `EventSource`s at `:199` and `:238` (cites 190-245 ✓); the poll/tick `setInterval`s at `:800`/`:803` and `visibilitychange` at `:814` (cites 790-810 and 797-830 ✓).
- **The service-worker instruction is exactly right.** `sw.js:4` is `const CACHE = "baude-v6"` and `:5` is the `SHELL` array — so "bump `CACHE` … from `baude-v6` to `baude-v7`" is correct to the literal, and the stated reason (installed clients would otherwise serve the stale precache) is the direct consequence of `sw.js:8` `caches.open(CACHE).then(c => c.addAll(SHELL))`. `bauded/src/web.rs:27-47` is the `asset!` route list with `app.js` at `:30-33`, so `/removal.js` is a one-line mirror.
- **New test capability at zero dependency cost.** RESEARCH A6 said there is no JS harness; `node:test`/`node:assert/strict` are runtime built-ins, and the `typeof module !== "undefined"` guard keeps the no-build-step constraint intact. `baude/src/ui.rs:2509 remove_project_confirmation_lines` exists as the section-order template the plan tells the executor to grep.
- **Fixture discipline is stated as a rule, not a hope**: "a fixture with a field the daemon never emits is a defect in the fixture, not a passing test" (line 74).

### 3. Concerns

- **HIGH — both automated verify commands judge through a pipe, which the project forbids.** Task 2 (line 99): `grep -c 'removal.js' … | grep -v ':0$' | wc -l | grep -q '^3$'` — the chain's exit status is the last stage's, so an earlier failure is invisible, and `cargo build -p bauded` sits behind it after `&&`. Task 3 (line 117): `grep -c "EventSource('/events')…" bauded/web/app.js | grep -qv '^0$'` — besides the pipe, `grep -c` on a single file emits exactly one line, so `grep -qv '^0$'` succeeds for a count of 1 **and** for a count of 99, and for any non-zero count whatever the pattern matched. 21-06 Task 3 shows the correct form (`STATUS=$?` read on the next line); these two should follow it. 20-04-SUMMARY already records the convention: "Each gate was redirected to its log and its exit read from the shell variable on the next line, never through a pipe."
- **MEDIUM — T-21-09 (XSS) has no automated evidence.** `confirmationView` returns plain data (`{title, sections:[{label, lines}]}`, line 71), so the `node --test` suite can only assert that the view model *preserves* an angle bracket — the opposite of the property at risk. The real surface is the `innerHTML` interpolation in Task 2, which has no check, no per-site instruction to apply `esc()`, and no test. The register claims "Every daemon-supplied string passes through the existing `esc()` helper"; nothing in the plan makes that true or detects it becoming false. The modal renders daemon-supplied paths, branches and session names, so this is the phase's weakest mitigation-to-evidence link.
- **MEDIUM — the modal is 420px wide.** `style.css:310-317` sets `#modal .box { … max-width: 420px }`. D-16 requires a repository parent to list every child by name and branch, plus `paths_deleted`/`paths_kept`/`already_removed`/`cleared` — full filesystem paths. The plan adds `.sections { max-height: 60vh; overflow-y: auto }` for vertical scroll but never widens the box and never says how paths are shortened. The TUI solves this with `tilde_path` (`baude/src/ui.rs:2646`); the PWA has no equivalent in scope.
- **MEDIUM — Task 2's read_first names a function that does not exist.** `renderList` is at `app.js:596` (just outside the cited "600-700"), and there is **no** `rowHtml` function anywhere in `app.js`. This is the five-file task with the largest blast radius; a wrong pointer costs the most context exactly here.
- **MEDIUM — `removal.test.js` lands inside the served asset directory.** `web.rs:20` is `include_bytes!(concat!("../web/", $path))`. The test file will not ship (no route registered), but putting it in `bauded/web/` invites a future asset glob or `SHELL` entry to pick it up. `bauded/web/tests/` or `bauded/tests/web/` costs nothing.
- **LOW — `e.body` is ambiguous.** Task 2 says `api()` throws with "`body` (the raw text)"; `key_links` line 35 says "the 409 handler reads `e.body` JSON". `api()` at `app.js:93-105` currently throws with `.status` and the plain-text body as the message. Say whether `e.body` is the raw string (then `JSON.parse(e.body)`) or pre-parsed.
- **LOW — no `scripts/gate.sh` anywhere in this plan.** Verification is `node --test`, `node --check`, `cargo build -p bauded`. See the cross-plan note: this matters because 21-05 and 21-06 share a wave.

### 4. Suggestions

- Rewrite both verify commands as separate statements with `$?` read on the next line; replace the three-file registration check with three independent `grep -q` calls.
- Give T-21-09 real evidence: have `confirmationView` return escaped strings and assert `&lt;` in the output, **or** add a `renderConfirmation(view) → html` pure function in `removal.js` that does the interpolation, and test *that* — it is the only way to cover the DOM layer with `node --test` and no browser.
- Widen `#modal .box` for the removal modal (a `.box.wide` variant) and add a `tildePath()` helper to `removal.js` mirroring `ui.rs:2646`.
- Repoint Task 2's read_first at `app.js:596` and drop `rowHtml`.
- Move `removal.test.js` out of the asset directory.

### 5. Risk Assessment

**MEDIUM-HIGH.** The largest surface, the newest tooling, the only human gate, two broken verify commands, and an unevidenced XSS mitigation. Nothing here can corrupt state — the daemon owns every mutation and recomputes everything — so the risk is shipped UI defects and a security claim without a test, not data loss.

---

## 21-06

### 1. Summary

The best-cited plan in the phase: all eight of its `app.rs` ranges contain exactly what it says they contain, and its reuse strategy (an `origin` flag on the existing modal, with an explicit "do not touch `confirm_remove_project`") is the right way to get two backends behind one confirmation. It carries four MEDIUM concerns, all of them about things the plan does not mention rather than things it gets wrong: an existing test it invalidates, a blocking call in the key path, a thread with no shutdown, and a label path that cannot satisfy D-16 for parent removals.

### 2. Strengths

- **Citation accuracy is near-perfect.** `SidebarRefusal` `:275` with `RemoteRemove` `:299-300`; `sidebar_action` `:336` and its `ActionKind::Remote => SidebarAction::Refuse(SidebarRefusal::RemoteRemove)` arm `:344`; the refusal notice `:1328`; `Modal::ConfirmRemove` `:501` and `RemoveProjectOutcome` `:511`; `removal_kind_label` `:3789` / `removal_row_label` `:3819`; `removal_target_for_selection` `:3865` / `open_remove_project_modal` `:3910`; the keymap arm `:5795`; `d_on_remote_row_sets_footer_notice` `:12413`. Every cited range checks out.
- **It resolves a stale cross-artifact name correctly.** The TUI helper is `collect_row_runtimes` (`app.rs:3948`) with `restore_row_runtimes` at `:3981`; 21-CONTEXT.md:53 calls the first `take_row_runtimes`. 21-01 and 21-06 both use the real name — CONTEXT is the stale artifact, not the plans.
- **`#[serde(default)] pub checkout_key: Option<CheckoutKey>` on `RemoteInfo` is the right placement** and follows an eight-precedent in-file convention (`remote.rs:33-77`), each already commented with its back-compat reason.
- **No `baude-core` change is needed for the 409 parse**, because `RemovalReport` derives `Deserialize` (`removal.rs:179`) — so `files_modified` being just `remote.rs` + `app.rs` is honest.
- **`start_without_events` anticipates a real problem**: existing TUI tests point `RemotePoller` at a refusing port, and a second reconnecting thread per test would be a thread storm.
- **The double is specified strictly**: "answer exactly as scripted so that a wrong path or method surfaces as a test failure, never as a tolerant match" — matching the project's fixture rule.
- **Task 3 is the only gate task in the phase that gets the convention right** (`STATUS=$?` on the next line, no pipe, line 114), and it correctly runs `--linux` because the subscriber thread and `TcpListener` double are platform-sensitive.

### 3. Concerns

- **MEDIUM — an existing test is invalidated and never mentioned.** `baude/src/app.rs:12391 fn sidebar_action_d_refuses_remote_rows()` asserts exactly the mapping Task 2 inverts ("map `ActionKind::Remote` with the D key to `SidebarAction::RemoveProject` instead of the refusal", line 95). The plan rewrites `d_on_remote_row_sets_footer_notice` (`:12413`) but says nothing about `:12391`, and none of Task 2's three verify filters (`d_on_remote_row_`, `enter_on_remote_confirm_`, `confirm_remove_`) matches `sidebar_action_d_refuses_remote_rows`. It will go red only in Task 3's full gate, at the end of the phase, attributed to nothing.
- **MEDIUM — `D` on a remote row puts a blocking HTTP round trip in the TUI key path.** `preview_removal` and `execute_removal` are `ureq` calls bounded by `REQUEST_TIMEOUT` (`remote.rs:17`, 3 s) issued from the key/draw thread, so a slow or hung daemon freezes the UI for up to 3 s per press — twice (preview, then confirm). This is consistent with the existing `delete`/`restart`/`create` precedent (`remote.rs:198-275`), so it is not novel; but removal is the first path where a *modal opens on the result*, and the plan says nothing about it.
- **MEDIUM — the events thread has no reachable shutdown.** Line 67/71 specify "no read timeout" and reconnect-after-`POLL_SECS`. `RemotePoller` already carries an `AtomicBool` stop flag (`remote.rs:6`), but a thread blocked reading a live `/events` socket will not observe it until the daemon writes or closes — and with `KeepAlive::default()` the next write can be ~15 s away. Switching daemons or quitting leaks a thread holding a socket for up to a keep-alive interval. T-21-11 addresses only the reconnect interval, not the blocked read.
- **MEDIUM — remote `removal_row_label` cannot satisfy D-16 for a parent removal.** The plan resolves `RowKey::Checkout` "through `remote_snap` sessions by `checkout_key` to the session name, falling back to the key debug text" (line 95). A repository parent's `rows_removed` includes checkout rows with no live daemon session (saved but not running) — those have no `RemoteInfo` and render as `Checkout(CheckoutKey(3))`. D-16 requires "every child listed by name and branch". The daemon serves exactly the missing data at `GET /rows` (21-02), but 21-06 never fetches it; `RemotePoller` only polls `/sessions` (`remote.rs:107-200`). The PWA does join `/rows` (21-05 `key_links` line 34) — so the two clients will not be at parity, which is the phase's own goal.
- **LOW — Task 2 asserts a counter Task 1 does not define.** "the double's script is empty and the test asserts it saw zero requests" (line 89), but Task 1 specifies `serve(script) -> (base_url, JoinHandle)` with no request count. The counter has to be in Task 1's `test_double` API.
- **LOW — the `ureq` API is major-version-specific.** `AgentBuilder::new().timeout_connect(…)` is `ureq` 2.x (`ureq = { version = "2", features = ["json"] }`), correct for the pin, but worth noting as a 3.x migration point.

### 4. Suggestions

- Add `sidebar_action_d_refuses_remote_rows` (`app.rs:12391`) to Task 2's behavior list as a rewrite, and widen the verify filter to `app::tests::sidebar_action_d_` so it fails in its own task.
- Have `RemotePoller` also poll `GET /rows` (or cache the rows from the preview report's own structure) so remote labels can name every child by name and branch, closing the D-16 parity gap.
- Set a read timeout on the events agent (e.g. keep-alive interval + margin) so the loop re-checks the stop flag, and record the blocking-key-path tradeoff in the SUMMARY even if it is accepted.
- Add a request counter to `test_double::serve`'s return value in Task 1.

### 5. Risk Assessment

**MEDIUM.** It modifies the phase's largest file (`baude/src/app.rs`, 15,948 lines) and inverts a keymap arm with existing coverage, but the local Phase 20 path is explicitly fenced off and every daemon-side contract it consumes is already landed and tested by waves 1-4. Its own full-gate task is the phase's real safety net.

---

## Cross-plan comparison

**Dependency ordering.** Waves 1→2→3→4 are fully serial, and the chain is longer than the data requires. 21-04 Task 1 (`tokio sync`, `DaemonEvent`, `GET /events`) needs only `remove_rows` from 21-01; the genuine 21-03 dependency is Task 2 alone, which reuses the forced-failure seams. Likewise 21-02 Task 1 (`/rows`, `checkout_key`) has no dependency on 21-01's removal code beyond the test fixture. Splitting 21-04 so its Task 1 runs in wave 2 would shorten the critical path by a wave with no real risk. The wave-5 fan-out (21-05 ∥ 21-06) is correct and the file spaces are disjoint (`bauded/web/*` + `web.rs` vs `baude/*`).

**The gate gap.** `bauded/src/web.rs:20` is `include_bytes!(concat!("../web/", $path))`, so 21-05's new `/removal.js` route and the `bauded/web/removal.js` file must land together or `cargo build -p bauded` fails. 21-06 Task 3 is the only full-gate task in the phase, it sits in the *same wave* as 21-05, and it compiles `bauded`. If the two run concurrently, 21-06's gate can go red on 21-05's half-finished diff; if 21-05 lands second, the phase closes with no gate over the final tree — 21-05's strongest verify is `cargo build -p bauded`. **Recommend a wave-6 gate task owned by neither plan**, running `scripts/gate.sh` and `scripts/gate.sh --linux` with statuses read from variables.

**Pipe-in-gate discipline is inconsistent.** 21-06 Task 3 does it right; 21-05 Tasks 2 and 3 both judge through pipes, and 21-05 Task 3's `grep -c … | grep -qv '^0$'` is additionally a no-op assertion. Every other plan's `cargo test` verify uses the correct redirect-then-`$?` form.

**Error-text register diverges across three clients.** The daemon's 500/503 bodies are `RemovalError` Display strings with `{row:?}` debug formatting (`removal.rs:260-281`), e.g. `row Checkout(CheckoutKey(3)): runtime survived the kill stage: …`. 21-05 D-15 renders `e.message` verbatim in the user's modal; 21-06 renders the error text in the TUI modal. Both therefore show Rust debug text to users. The TUI already owns `removal_row_label` (`app.rs:3819`) and `tilde_path` (`ui.rs:2646`) for exactly this. One decision recorded once beats three divergent renderings discovered in Phase 22.

**Requirements coverage is complete and the shared-ID gate will hold.** API-01 → 21-01/02/03/06; API-02 → 21-04/05/06; API-03 → 21-02/05. No requirement sits in a single plan, so no plan can close one alone and the `requirements.ready-ids` gate stays open until 21-05 and 21-06 both land.

**Source grounding is unusually good, and the misses cluster in one place.** Of roughly forty line citations I checked, the great majority are exact — including all four `git::discover_repository` call sites (`manager.rs:921, 1073, 1347, 1418`), `lifecycle.rs:2839`, `Cargo.toml:27`, `sw.js:4-5`, `persist.rs:352`, `repository.rs:1229`, and every one of 21-06's eight `app.rs` ranges. The three real misses are all in `read_first` ranges: 21-03's pty seam (points at core's own test module, not `pty.rs:914`), 21-05's non-existent `rowHtml`, and 21-05's `renderList` off-by-four. That is the most expensive place to be wrong, since a bad pointer burns executor context before any code is written. Given every plan carries `confidence: low` and RESEARCH was cut short by context exhaustion (STATE.md `stopped_at: context exhaustion at 78% (2026-11-05)`), a standing "re-grep each anchor before reading" instruction would pay for itself — several plans already say this, inconsistently.

**RESEARCH errors are corrected but not recorded.** 21-04 silently overrides A1 (tokio `sync`) and A2 (`futures_util::stream::unfold` — unusable, since `futures-util` is `[dev-dependencies]` only); 21-05 silently overrides A6 (no JS harness) and CONTEXT D-02/D-13's "clearings" wording. All four corrections are right. None is written down, so Phase 22 can re-derive them from the same stale sources.

**Planning bookkeeping has drifted.** `.planning/STATE.md` reports `current_phase: 20`, `status: executing`, `completed_plans: 19` of 25, while all four Phase 20 SUMMARYs exist on disk and Phase 21's plans are written and reviewed. 21-01's `depends_on: []` / `wave: 1` is still safe because Phase 20 really is complete, but the state files will mislead the wave scheduler and anyone resuming cold.

## Overall risk assessment

**MEDIUM.**

What lowers it: the single highest-consequence unknown — whether `RemovalReport` can be used as an axum extractor and parsed by the TUI without touching `baude-core` — resolves in the plans' favour (`removal.rs:179`), which is what keeps 21-01's and 21-06's `files_modified` honest. Every error status is driven by a real cause rather than a stub. The daemon never acts on client-supplied paths. The test harness already isolates `config_dir()` and the workspace (`api.rs:721-742`, proven by `persist.rs:1265`), so the suppression work cannot touch the developer's machine. The core operation is unchanged and already tested in Phase 18, and the TUI's local path is explicitly fenced off in 21-06.

What raises it: ~306k estimated tokens across six `confidence: low` plans; the phase's only full gate sits inside a plan that shares a wave with the one most likely to break the build; two verify commands in 21-05 cannot fail correctly; the XSS mitigation has no automated evidence; and three concrete behavioural gaps are unaddressed — `RuntimeHandleMissing` on the daemon (21-01/21-03), the invalidated `sidebar_action_d_refuses_remote_rows` test (21-06), and the remote-label/D-16 parity gap (21-06).

**The five changes I would make before executing**, in order:

1. Move the phase's full gate into its own wave-6 task (fixes the 21-05/21-06 collision and the no-gate-on-final-tree gap).
2. Rewrite 21-05's two piped verify commands in the `STATUS=$?` form 21-06 Task 3 already uses.
3. Add `sidebar_action_d_refuses_remote_rows` (`app.rs:12391`) to 21-06 Task 2's scope and widen its verify filter.
4. Give T-21-09 a testable seam — move the `innerHTML` interpolation into a pure `renderConfirmation()` in `removal.js` so `node --test` can assert escaping.
5. Add the `RuntimeHandleMissing` case to 21-03 and decide its status code deliberately rather than letting it default to 500.
