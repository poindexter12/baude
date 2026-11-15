---
phase: 21
reviewers: [claude, codex]
reviewed_at: 2026-11-05T21:23:00Z
plans_reviewed: [21-01-PLAN.md, 21-02-PLAN.md, 21-03-PLAN.md, 21-04-PLAN.md, 21-05-PLAN.md, 21-06-PLAN.md]
models:
  claude: "unknown"
  codex: "gpt-6.1-sol (reasoning=high)"
model_sources:
  claude: "unknown"
  codex: "banner"
---

# Cross-AI Plan Review — Phase 21

<!-- gsd:plan-revision-conflicts:begin -->
## Plan-Revision Conflicts
<!-- gsd:plan-revision-conflicts:end -->

## Reviewers

**Cycle 2.** Both selected lanes produced a complete six-plan review and both are counted.

| Lane | Model / transport | Outcome |
|---|---|---|
| `claude` | Claude (spawn), full repo access, 130k prompt budget | Completed — all six plans, 83 `file:line` citations |
| `codex` | `gpt-6.1-sol`, reasoning `high` (spawn), 176.8k prompt budget | Completed — all six plans, 146 `file:line` citations |

Plan-coverage check: **both lanes complete** (6 of 6 ids each), so no `plan_coverage` key is emitted.

Neither section carries a `[reviewed-without-source-citations]` or `[reviewed-without-repo-access]` discount marker, so the **two-reviewer consensus gate is active** for this cycle.

**Standing fact for this cycle: the plans are byte-identical to cycle 1.** `git diff f0d298f -- .planning/phases/21-daemon-and-pwa-parity/` is empty, and the last plan-touching commit is `6397c72`, which predates cycle 1's review commit `f0d298f`. No cycle-1 finding has been incorporated, deferred or rejected in any PLAN.md. Every cycle-1 concern that the reviewers raised again is therefore **still unresolved**, and nothing in this cycle can be scored as resolved.

## Claude Review


**Review mode:** source-grounded. Every `file:line` below was opened and checked against the plan text. Where a claim could not be settled from source it is labelled an open question rather than asserted.

---

### 21-01

#### 1. Summary

A well-anchored tracer. The plan wraps `baude_core::removal::{preview_removal, remove}` in two `Manager` methods and two routes, borrowing the Phase 20 TUI's custody/recheck/rollback posture wholesale. Its riskiest compile-time assumption — that the core report can be used as an axum *extractor*, not just a response — holds: `RemovalReport` derives `Deserialize` at `baude-core/src/removal.rs:179`, as do every nested type it contains (`KilledRuntime` :130, `PersistenceOutcome` :139, `ClearedItem` :162, `ClearingFailure` :171). So `Json<RemovalReport>` works and neither this plan nor 21-06 needs to touch `baude-core`, which is why both can keep their narrow `files_modified`. The main gap is an unmapped-in-practice error class (`RuntimeHandleMissing`) that is reachable on the daemon specifically, and which no plan in the phase drives.

#### 2. Strengths

- **The `RemovalError` mapping is exhaustive and spelled correctly.** Plan line 81 names `UnknownTarget`, `Persist`, `RuntimeSurvived`, `DiskRemoval`, `RuntimeHandleMissing`, `RuntimeIdentityMismatch`, `Validation` — exactly the seven variants declared at `baude-core/src/removal.rs:235-258`, with no variant left to fall through to a wrong default. `RemovalError` is `Debug + Display` only (no Serde, verified :235), which is why the plan uses Display for every plain-text body — correct.
- **The `Busy → 503` addition is wired to a real mechanism.** `RepositoryReservations::reserve` is at `baude-core/src/lifecycle.rs:2839` and does return `Err(LifecycleOutcome::Busy { repository })` when the repository is already held (:2843-2846), and `Manager` already owns a `repository_reservations: RepositoryReservations` field (`bauded/src/manager.rs:129`). The reasoning for 503-not-409 ("D-03 reserves 409 bodies for the fresh-report JSON") is sound and keeps the one-JSON-error-body rule intact.
- **Both save-closure seam names are right, on both sides of the fence.** The TUI's is `persistence_root_for_test` (`baude/src/app.rs:666`, used in the removal save closure at `:4033`); the daemon's is `persistence_target_for_test: Option<(PathBuf, String)>` (`bauded/src/manager.rs:138`). The plan uses the first in its read_first (line 67) and the second in its action (line 79). `persist::save_current_at_test` (`baude-core/src/persist.rs:473`) and `persist::save_for_workspace_status` (:1137) both exist as named.
- **`removals_preview_leaves_the_state_file_untouched` is provable, not aspirational.** Core's module doc (`removal.rs:20-30`) states preview "never signals a process, never runs git and never writes a file", with read-only `symlink_metadata` probes plus a suppression-store read. Byte-identical state file is the right assertion.
- **Test-harness citations are exact.** `api_scope` :721, `app` :753, `body_json` :759, `post_json` :768, `initialized_repo` :815 (plan cites 721-830 ✓); `exited_tracked_manager` :936 (cites 936-955 ✓); `flat_session_api_remains_a_non_hierarchical_compatibility_projection` :965 (cites 965-1040 ✓). `restore_at` is at `manager.rs:511` and is private, which Task 2 correctly anticipates.
- **T-21-01's disposition is honest rather than decorative**: the daemon derives paths from `repository_state` alone and compares the echoed sets only.

#### 3. Concerns

- **MEDIUM — `RuntimeHandleMissing` is reachable on the daemon and nothing covers it.** `preflight` requires "one handle per row, no handle for a row without a persisted runtime" (`removal.rs:426-429` region; variant doc at :245-248). `collect_row_runtimes` resolves handles through `runtime_checkouts` (`manager.rs:128`), which only maps checkouts the daemon spawned. But `restore_loaded` (`manager.rs:526-570`) loads persisted state verbatim and then only *reopens* checkouts matching its lifecycle filter — a row whose `reopen_checkout` returns `Err` logs and continues (`:562`), and `Ok(_)` non-Reopened outcomes are silently skipped (`:561`). Such a row stays in `repository_state` with no `runtime_checkouts` entry. If its persisted `owned_runtime` is still set, removal returns `RuntimeHandleMissing` → a 500 with developer-facing text and no client recourse. **Open question:** whether `reconcile_teardown_recoveries` (`:541`) or exit detection clears the stale `owned_runtime` on that path — I could not settle that from the ranges I read. Either way, the plan does not say, and no test in 21-01 or 21-03 drives it.
- **MEDIUM — `PersistenceOutcome` is internally tagged; the assertion is loose.** `#[serde(rename_all = "snake_case", tag = "outcome")]` at `removal.rs:139-146` means the JSON is `{"outcome":"saved"}` and `{"outcome":"saved_degraded","detail":"…"}`, not a bare `"saved"`. Task 1's "`persistence` equal to the serialized `saved` outcome" (line 73) is vague enough to be written wrong first time, and 21-05's fixtures inherit the same shape.
- **LOW — preview holds no reservation, which reads as a contract violation until you work it out.** Core states callers hold `RepositoryReservations` "for every repository in the set from before `select_rows` until this function has returned" (`removal.rs:461-464`). `Manager::preview_removal` is a bare `&self` delegate. In practice this is fine — it runs under the global `Arc<Mutex<Manager>>` (`manager.rs:38`, `lock()` at `:106`), so it is already atomic against every other daemon mutation, and `remove_rows` reserves *before* its recheck and before core's internal `select_rows`. But an executor reading core's doc comment will think the preview path is non-compliant. One sentence recording why it is safe would prevent a wrong "fix".

#### 4. Suggestions

- Add a fourth test driving `RuntimeHandleMissing` (seed a checkout row with a persisted `owned_runtime`, drop the `runtime_checkouts` entry via a `#[cfg(test)]` seam) and decide whether its status should be 409/503 "retry after daemon restart" rather than a bare 500.
- Pin the `persistence` assertion to `json!({"outcome":"saved"})` so 21-05's fixtures have a verified shape to copy.
- Record in the SUMMARY the one-line rationale for the unreserved preview, citing `removal.rs:461` and `manager.rs:106`.

#### 5. Risk Assessment

**LOW-MEDIUM.** The load-bearing assumptions verify; the contract is narrow; the fixture harness it copies already exists and already isolates. The one real gap (`RuntimeHandleMissing`) degrades to an ugly 500 rather than data loss.

---

### 21-02

#### 1. Summary

The widest plan in the phase and the one whose source grounding is most impressive — its four `git::discover_repository` call-site line numbers are exact, and its instruction to record a *negative* finding if none of them admits implicitly is a genuinely good anti-hallucination device. The suppression test isolation question (does the daemon test harness redirect `config_dir()`?) resolves in the plan's favour. Two issues: a serde attribute placed on the wrong struct, and an unstated path-keying hazard that will make the suppression assertion fail in a confusing way.

#### 2. Strengths

- **The test-isolation claim is verified.** `api_scope` (`bauded/src/api.rs:721`) installs `baude_core::testing::TestRedirect::new(&root)` plus a workspace override (`:731-740`), and `persist::config_dir()` honours that redirect — proven by core's own `config_dir_honours_redirect` test at `baude-core/src/persist.rs:1265-1272`. So Task 3's "`persist::config_dir()` is redirected by `api_scope`, so the tests read the store under the test root" (line 112) is **correct**, and the suppression tests will not write the developer's real `~/.config/baude`. The harness was built for exactly this: `api_scope` is `#[must_use]` (`:742`) and `app()` carries the comment "Returning the bare router would compile and run the case against the developer's real workspace and data dir" (`:789-791`).
- **The two-checkout fixture is constructible exactly as written.** `worktree: Option<String>` is a real `/sessions` body field (`api.rs:169`) forwarded as `create(&body.repo, body.worktree.as_deref(), …)` (`:178`).
- **Every implicit-admission pointer is exact.** `git::discover_repository` callers at `manager.rs:921, 1073, 1347, 1418` — all four correct. `reconcile_legacy_session` at `:155-156`, correct. `Manager::create` at `:813`, `record_checkout_intent` at `:1065` — correct. `list` :2393 / `info` :2397 / `session_info` :2616 — correct.
- **The suppression API is as described.** `is_suppressed` (`suppression.rs:76`), `lift` (`:84`), `suppress` (`:98`), `drop_children` (`:113`). `lift` already returns `Ok(false)` without writing when absent (`:84-96`), so the "advisory, never fails the mutation" posture is the function's own contract, not something the daemon must bolt on. The TUI template the plan tells the executor to grep is real: `App::lift_suppression` (`baude/src/app.rs:2374`) and the sweep guard `suppression::is_suppressed(&persist::config_dir(), &record.path)` (`:2604`).
- **`seed_standalone_for_test` is buildable from `bauded`.** `SavedStandaloneSession::new` is `pub` (`baude-core/src/repository.rs:464`) and `RetainedStandaloneSessionState` has all-`pub` fields (`:311-322`). The cited "test-only constructor shape" at repository.rs:1225-1245 is real — `fn standalone(...)` at `:1229` matches the plan's description verbatim — and the plan correctly treats it as a *shape to copy* (it lives in core's own `#[cfg(test)] mod tests`, so it is not callable cross-crate).

#### 3. Concerns

- **MEDIUM — `#[serde(default)]` on `SessionInfo.checkout_key` is on the wrong struct.** `SessionInfo` is `#[derive(Serialize, Clone)]` (`manager.rs:180-181`) — it never deserializes, so `default` buys nothing there. The attribute that actually delivers back-compat is on the *client* type, `RemoteInfo` in `baude/src/remote.rs:21`, which already uses `#[serde(default)]` on eight fields for precisely this reason (`:33-77`, each with a back-compat comment). 21-06 Task 1 gets it right (plan line 63). 21-02's stated rationale — "for symmetry with the TUI's `RemoteInfo`" (line 66) — misreads the mechanism: these are two different types on opposite ends of the wire. At best inert, at worst a serde derive error.
- **MEDIUM — the suppression assertion's path keying is unspecified and will fail confusingly.** `is_suppressed` is an **exact `folder_key` match** — "a parent or sibling entry does not suppress it" (`suppression.rs:74-78`). The test asserts `is_suppressed(&config_dir(), &worktree_path)` but never says which path form core keyed. Core plans one `SuppressionAdded` per checkout row whose parent repository survives (`removal.rs:330-333` doc), keyed off the row's observed path. If the test uses a canonicalised or symlink-resolved variant, the lookup returns false and reads as "the write side is broken" rather than "the test used the wrong path".
- **LOW — the parent-removal GC half is asserted backwards.** Task 3 asserts that removing the repository "adds no entry for the repository path", but core also plans `SuppressionDropped` per repository row that has children in the store (`removal.rs:331`), i.e. removing the parent *garbage-collects* the child entry via `drop_children` (`suppression.rs:113`). The user-visible behaviour — the previously-suppressed worktree entry is now gone — is never asserted.
- **LOW — 21-05 depends on a SUMMARY that does not exist yet.** The `/rows` field names are declared a wire contract with `reversibility: costly` (correct), but their only record is "note the `/rows` field names verbatim" in the 21-02 SUMMARY (line 151), which 21-05 Task 2 read_firsts (line 93). Fine in wave order; brittle if that SUMMARY is terse.

#### 4. Suggestions

- Move the `#[serde(default)]` instruction out of 21-02 and note that 21-06 owns the deserialize side.
- Name the exact path form in the suppression assertion — read it back out of the preview report's `cleared` entry (`ClearedItem { kind: SuppressionAdded, path }`, `removal.rs:162-166`) rather than reconstructing it in the test.
- Add the GC assertion: after the parent removal, `is_suppressed(&config_dir(), &worktree_path)` is `false`.
- Put the `/rows` field list in the plan itself as a code block, not only in a future SUMMARY.

#### 5. Risk Assessment

**MEDIUM.** Three tasks, three files, a new wire contract, and a write into a store shared with the TUI. Isolation is verified and the admission audit is well-specified; the risk is concentrated in Task 3, where a path-keying mismatch costs debugging time, not data.

---

### 21-03

#### 1. Summary

The failure-posture plan, and the one most faithful to the project's test rules ("keep every assertion on observable state … never on log text"; "No test asserts by grepping source"). Each of 404/409/500/503 gets a real driver rather than a stubbed error, and the injection seams it needs are reachable. The significant weakness is that its headline security test does not prove the property its threat register claims.

#### 2. Strengths

- **The kill-failure seam genuinely is reachable cross-crate.** `fail_next_teardown_for_test` is `pub` behind `#[cfg(debug_assertions)]` at `baude-core/src/pty.rs:912-914` — *not* `#[cfg(test)]` — so `bauded`'s debug-profile tests can call it. Combined with the daemon-side `atomic_failure_for_test` (`manager.rs:140`, set via `persist_at_for_test` `:2514`) and `persistence_blocked` (`:131`), all three failure classes have a real injection point.
- **`removals_execute_ignores_unknown_fields` is sound.** `RemovalTarget` (`removal.rs:48-49`), `RowKey` (`:57-58`) and `RemovalReport` (`:179-180`) carry no `deny_unknown_fields`, so serde does drop extras, and the test's framing — "no field other than `target`, `rows_removed`, `runtimes_killed`, `paths_deleted` influences behaviour" — is the right contract to state.
- **The 500 body assertion will hold.** `RemovalError::RuntimeSurvived`'s Display is `"row {row:?}: runtime survived the kill stage: {error}"` (`removal.rs:272-274`), so asserting the body names the row and "survived" is safe.
- **The retry-after-cause-clears shape is borrowed from a path already proven.** Phase 20's `remove_project_failure_kill_keeps_rows_and_handles_then_retry_succeeds` is cited as the template, and all four Phase 20 SUMMARYs exist on disk, so the recipe is real.

#### 3. Concerns

- **MEDIUM — `removals_bogus_paths_change_nothing` proves the weaker of two properties.** The test expects **409** (plan line 64). Replacing `paths_deleted` with a sibling path makes the echoed set differ from the fresh preview, so *drift* fires and the body is rejected before any path comparison matters. That proves "a tampered body is rejected" — true and worth having — but it does **not** prove T-21-05's actual claim that "echoed sets are compared, never executed". The property that needs proving is that a body which passes the drift check still cannot influence the filesystem: execute with the three sets matching exactly but `paths_kept`, `already_removed`, `cleared` and `persistence` replaced with garbage pointing at a real sibling directory, assert **200**, and assert the sibling still exists.
- **MEDIUM — the kill-seam citation points at a usage example, not the API.** Plan line 81 cites "`baude-core/src/removal.rs` lines 2255-2300 (`fail_next_teardown_for_test` seam on the pty)". The function is at `pty.rs:914`; `removal.rs:2264` and `:2314` are call sites *inside core's own `#[cfg(test)] mod tests`*, on a pty the executor cannot reach from `bauded`. The plan's own fallback ("add a `#[cfg(test)] Manager::session_pty_for_test(id)` accessor only if neither is reachable") rescues the task, so this costs context, not correctness.
- **MEDIUM — `removals_drift_body_is_accepted_on_resend` codifies replay as a feature without saying so.** The 409 body is a *preview* report (`planned: true`); resending it executes. That is the intended contract, but it means a captured 409 body stays executable indefinitely, because the only freshness check is set equality. Benign on a tailnet; it should be recorded in the SUMMARY as deliberate idempotency rather than left implicit.
- **LOW — three of the seven error arms are mapped but never driven.** `RuntimeHandleMissing`, `RuntimeIdentityMismatch` and `Validation` all route to 500 in 21-01 with no test here. `RuntimeHandleMissing` is the reachable one (see 21-01).
- **LOW — the 500 body is developer-facing.** `{row:?}` formatting (`removal.rs:260-281`) yields text like `Checkout(CheckoutKey(3))`. 21-05 D-15 renders `e.message` verbatim in the user's modal and 21-06 does the same in the TUI. Fine as a daemon contract; see the cross-plan note.

#### 4. Suggestions

- Split the tampering test in two: keep the 409 drift case, and add the 200 case above as the real T-21-05 evidence.
- Repoint the seam citation to `baude-core/src/pty.rs:912-914` and note the `debug_assertions` gate explicitly, since a release-profile test run would lose it.
- Add the `RuntimeHandleMissing` case here rather than deferring it.

#### 5. Risk Assessment

**LOW.** Tests-only plus minimal `#[cfg(test)]` seams, over code 21-01 already landed. The worst outcome is a security claim that outruns its evidence — which this review flags before it reaches a SUMMARY.

#### Note

The `RemovalReport` serde shape the plan relies on is confirmed: `cleared: Vec<ClearedItem>` and `clearing_failures: Vec<ClearingFailure>` (`removal.rs:191-197`), no `clearings` field, exactly as 21-05 states.

---

### 21-04

#### 1. Summary

The sharpest plan in the set. It catches a factual error in its own RESEARCH, cites the dependency line to the number, and picks the only streaming approach that actually compiles in this crate. Its two weaknesses are both of the same kind: two tests claim to prove mechanisms their fixtures cannot reach, and in both cases the *safe consequence* is what gets proven. That is a documentation problem, not a correctness one, but the `must_haves` truths are written as if the mechanisms were proven.

#### 2. Strengths

- **The Cargo claim is exact and RESEARCH is correctly overridden.** `bauded/Cargo.toml:27` is `tokio = { version = "1", features = ["macros", "net", "rt-multi-thread", "signal", "time"] }` — `"sync"` is genuinely absent. The plan says so *at the line* (read_first line 61: "line 27 … sync is absent and must be added explicitly; RESEARCH A1 is not relied on"), the feature is the only Cargo change, and `bauded/Cargo.toml` is correctly in `files_modified`.
- **`async_stream` is not just a stylistic match — it is the only option.** `async-stream = "0.3"` and `futures-core = "0.3"` are `[dependencies]`; **`futures-util = "0.3"` is a `[dev-dependencies]` entry only.** RESEARCH's `futures_util::stream::unfold` sketch would not compile in a production handler. The plan sidesteps it by mirroring the two existing handlers, which do exactly `async_stream::stream! { … }` returning `Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>` with `KeepAlive::default()` (`api.rs:608-642` `stream`, `:652-686` `activity_stream` — the read_first range "600-690" lands on both). Right answer; the plan just never says RESEARCH was wrong here.
- **No new test dependency either.** `http_body_util::BodyExt` is already imported in the test module (`api.rs:697`), and `tower::ServiceExt` (`:698`) is there for `oneshot`.
- **Subscribing before building the stream closes the lost-event race**, and the plan states that as the reason (line 69) rather than leaving it as incidental ordering.
- **The D-07 publish point is correct for the right reason.** Core's `remove` replaces in-memory state and consumes handles "only after that save returns `Ok` or reports that the replacement file was committed" (`removal.rs:426-429`), so an event sent on the `Ok` arm after `remove` returns always post-dates durability. Capacity 64 + skip-on-lag is justified by the clients' existing polls, and T-21-07's "bounded capacity … so a slow subscriber cannot grow daemon memory" is accurate.

#### 3. Concerns

- **MEDIUM — `events_published_after_state_file_dropped_the_row` cannot prove the in-lock send.** It asserts that "at the moment the `rows_removed` message is read … `GET /sessions` already omits it" (line 88). But `GET /sessions` takes the same `Arc<Mutex<Manager>>` (`lock()`, `manager.rs:106`), so the test thread cannot observe anything *while* the guard is held — it necessarily reads post-guard state. The test proves the safe consequence (no event ever precedes the drop), which is the property that matters; the `must_haves` truth at line 21 ("inside the same lock, so a `GET /sessions` that observes the event can never list the removed row") is stronger than the fixture. Assert the consequence and prove the in-lock placement structurally (the `send` sits on the `&mut self` path before the guard drops).
- **MEDIUM — `events_lagged_subscriber_keeps_streaming`'s second clause is untested by its own fixture.** It drives `Manager::subscribe_events()` *directly* and asserts `Lagged` then `Ok` — which tests `tokio::sync::broadcast`, not the handler. The clause "and the `/events` route … yields a message after the lag rather than ending" (line 89) needs the route's body in the picture: overflow a receiver the handler owns and read a frame off the response body. As written, the handler's `continue`-on-`Lagged` is asserted by inspection only.
- **LOW — `events_stream_has_keep_alive_and_no_history` will not see a keep-alive.** `KeepAlive::default()` in axum emits its comment on a ~15 s interval, so within the test's 500 ms window the realistic observation is *zero bytes*, not "the first bytes, if any, are a keep-alive comment" (line 66). Reword to "the 500 ms timeout elapses with no complete SSE message" and tolerate an empty read.
- **LOW — "`/stream` and `/activity-stream` are untouched" is a `must_haves` truth with no test.** Line 22 claims it; the verification block only says they "still pass". Both have existing tests — name them in the verify command so the claim is backed.

#### 4. Suggestions

- Add one sentence to the SUMMARY recording that RESEARCH's `futures_util::stream::unfold` pattern is unusable here because `futures-util` is a dev-dependency, so Phase 22 does not re-derive it.
- Reword the two over-claiming `must_haves` truths to match what the fixtures prove, and move the in-lock property to a code comment at the `send` site.
- Name the existing `stream`/`activity_stream` tests in Task 2's verify chain.

#### 5. Risk Assessment

**LOW.** One feature flag on an already-vendored crate, one enum, one channel, one route built from a pattern that exists twice in the same file. The failure mode of a mis-sequenced publish is a transiently stale client row that the 3 s poll reconciles.

---

### 21-05

#### 1. Summary

The most ambitious plan — five files, a new test runtime for the repo, and the only `human-check` gate in the phase — and the one that most clearly reads source rather than inheriting CONTEXT's wording. It is also the only plan whose verify commands violate an explicit project rule, twice, and its headline security mitigation (T-21-09, XSS) has no automated evidence at all because the test it points to exercises the wrong layer.

#### 2. Strengths

- **It corrects the contract against source instead of trusting CONTEXT.** Line 67: "there is no `clearings` field and no `ClearingResult` type, so the view model reads `report.cleared` and `report.clearing_failures`." Verified: `RemovalReport` has `cleared: Vec<ClearedItem>` and `clearing_failures: Vec<ClearingFailure>` (`removal.rs:191-197`) and no `clearings`. 21-CONTEXT.md D-02 and D-13 both say "clearings" loosely. This is the behaviour a plan should show.
- **The `already_removed` rule matches core's own semantics.** `DiskAction::AlreadyRemoved` is documented as a "Reporting label only" (`removal.rs:~155`) and `already_removed: Vec<PathBuf>` is its own field (`:186`), so "it is never shown under paths deleted" is a real invariant, not a UI preference.
- **Every PWA anchor is real and tight.** `esc()` at `app.js:33` — so T-21-09 names an existing helper rather than a wish. `toast` `:86`, `api` `:93` (cites 85-105 ✓); `refresh` `:137` (cites 137-150 ✓); `openModal` `:560` / `closeModal` `:585` (cites 520-600 ✓); the per-session `EventSource`s at `:199` and `:238` (cites 190-245 ✓); the poll/tick `setInterval`s at `:800`/`:803` and `visibilitychange` at `:814` (cites 790-810 and 797-830 ✓).
- **The service-worker instruction is exactly right.** `sw.js:4` is `const CACHE = "baude-v6"` and `:5` is the `SHELL` array — so "bump `CACHE` … from `baude-v6` to `baude-v7`" is correct to the literal, and the stated reason (installed clients would otherwise serve the stale precache) is the direct consequence of `sw.js:8` `caches.open(CACHE).then(c => c.addAll(SHELL))`. `bauded/src/web.rs:27-47` is the `asset!` route list with `app.js` at `:30-33`, so `/removal.js` is a one-line mirror.
- **New test capability at zero dependency cost.** RESEARCH A6 said there is no JS harness; `node:test`/`node:assert/strict` are runtime built-ins, and the `typeof module !== "undefined"` guard keeps the no-build-step constraint intact. `baude/src/ui.rs:2509 remove_project_confirmation_lines` exists as the section-order template the plan tells the executor to grep.
- **Fixture discipline is stated as a rule, not a hope**: "a fixture with a field the daemon never emits is a defect in the fixture, not a passing test" (line 74).

#### 3. Concerns

- **HIGH — both automated verify commands judge through a pipe, which the project forbids.** Task 2 (line 99): `grep -c 'removal.js' … | grep -v ':0$' | wc -l | grep -q '^3$'` — the chain's exit status is the last stage's, so an earlier failure is invisible, and `cargo build -p bauded` sits behind it after `&&`. Task 3 (line 117): `grep -c "EventSource('/events')…" bauded/web/app.js | grep -qv '^0$'` — besides the pipe, `grep -c` on a single file emits exactly one line, so `grep -qv '^0$'` succeeds for a count of 1 **and** for a count of 99, and for any non-zero count whatever the pattern matched. 21-06 Task 3 shows the correct form (`STATUS=$?` read on the next line); these two should follow it. 20-04-SUMMARY already records the convention: "Each gate was redirected to its log and its exit read from the shell variable on the next line, never through a pipe."
- **MEDIUM — T-21-09 (XSS) has no automated evidence.** `confirmationView` returns plain data (`{title, sections:[{label, lines}]}`, line 71), so the `node --test` suite can only assert that the view model *preserves* an angle bracket — the opposite of the property at risk. The real surface is the `innerHTML` interpolation in Task 2, which has no check, no per-site instruction to apply `esc()`, and no test. The register claims "Every daemon-supplied string passes through the existing `esc()` helper"; nothing in the plan makes that true or detects it becoming false. The modal renders daemon-supplied paths, branches and session names, so this is the phase's weakest mitigation-to-evidence link.
- **MEDIUM — the modal is 420px wide.** `style.css:310-317` sets `#modal .box { … max-width: 420px }`. D-16 requires a repository parent to list every child by name and branch, plus `paths_deleted`/`paths_kept`/`already_removed`/`cleared` — full filesystem paths. The plan adds `.sections { max-height: 60vh; overflow-y: auto }` for vertical scroll but never widens the box and never says how paths are shortened. The TUI solves this with `tilde_path` (`baude/src/ui.rs:2646`); the PWA has no equivalent in scope.
- **MEDIUM — Task 2's read_first names a function that does not exist.** `renderList` is at `app.js:596` (just outside the cited "600-700"), and there is **no** `rowHtml` function anywhere in `app.js`. This is the five-file task with the largest blast radius; a wrong pointer costs the most context exactly here.
- **MEDIUM — `removal.test.js` lands inside the served asset directory.** `web.rs:20` is `include_bytes!(concat!("../web/", $path))`. The test file will not ship (no route registered), but putting it in `bauded/web/` invites a future asset glob or `SHELL` entry to pick it up. `bauded/web/tests/` or `bauded/tests/web/` costs nothing.
- **LOW — `e.body` is ambiguous.** Task 2 says `api()` throws with "`body` (the raw text)"; `key_links` line 35 says "the 409 handler reads `e.body` JSON". `api()` at `app.js:93-105` currently throws with `.status` and the plain-text body as the message. Say whether `e.body` is the raw string (then `JSON.parse(e.body)`) or pre-parsed.
- **LOW — no `scripts/gate.sh` anywhere in this plan.** Verification is `node --test`, `node --check`, `cargo build -p bauded`. See the cross-plan note: this matters because 21-05 and 21-06 share a wave.

#### 4. Suggestions

- Rewrite both verify commands as separate statements with `$?` read on the next line; replace the three-file registration check with three independent `grep -q` calls.
- Give T-21-09 real evidence: have `confirmationView` return escaped strings and assert `&lt;` in the output, **or** add a `renderConfirmation(view) → html` pure function in `removal.js` that does the interpolation, and test *that* — it is the only way to cover the DOM layer with `node --test` and no browser.
- Widen `#modal .box` for the removal modal (a `.box.wide` variant) and add a `tildePath()` helper to `removal.js` mirroring `ui.rs:2646`.
- Repoint Task 2's read_first at `app.js:596` and drop `rowHtml`.
- Move `removal.test.js` out of the asset directory.

#### 5. Risk Assessment

**MEDIUM-HIGH.** The largest surface, the newest tooling, the only human gate, two broken verify commands, and an unevidenced XSS mitigation. Nothing here can corrupt state — the daemon owns every mutation and recomputes everything — so the risk is shipped UI defects and a security claim without a test, not data loss.

---

### 21-06

#### 1. Summary

The best-cited plan in the phase: all eight of its `app.rs` ranges contain exactly what it says they contain, and its reuse strategy (an `origin` flag on the existing modal, with an explicit "do not touch `confirm_remove_project`") is the right way to get two backends behind one confirmation. It carries four MEDIUM concerns, all of them about things the plan does not mention rather than things it gets wrong: an existing test it invalidates, a blocking call in the key path, a thread with no shutdown, and a label path that cannot satisfy D-16 for parent removals.

#### 2. Strengths

- **Citation accuracy is near-perfect.** `SidebarRefusal` `:275` with `RemoteRemove` `:299-300`; `sidebar_action` `:336` and its `ActionKind::Remote => SidebarAction::Refuse(SidebarRefusal::RemoteRemove)` arm `:344`; the refusal notice `:1328`; `Modal::ConfirmRemove` `:501` and `RemoveProjectOutcome` `:511`; `removal_kind_label` `:3789` / `removal_row_label` `:3819`; `removal_target_for_selection` `:3865` / `open_remove_project_modal` `:3910`; the keymap arm `:5795`; `d_on_remote_row_sets_footer_notice` `:12413`. Every cited range checks out.
- **It resolves a stale cross-artifact name correctly.** The TUI helper is `collect_row_runtimes` (`app.rs:3948`) with `restore_row_runtimes` at `:3981`; 21-CONTEXT.md:53 calls the first `take_row_runtimes`. 21-01 and 21-06 both use the real name — CONTEXT is the stale artifact, not the plans.
- **`#[serde(default)] pub checkout_key: Option<CheckoutKey>` on `RemoteInfo` is the right placement** and follows an eight-precedent in-file convention (`remote.rs:33-77`), each already commented with its back-compat reason.
- **No `baude-core` change is needed for the 409 parse**, because `RemovalReport` derives `Deserialize` (`removal.rs:179`) — so `files_modified` being just `remote.rs` + `app.rs` is honest.
- **`start_without_events` anticipates a real problem**: existing TUI tests point `RemotePoller` at a refusing port, and a second reconnecting thread per test would be a thread storm.
- **The double is specified strictly**: "answer exactly as scripted so that a wrong path or method surfaces as a test failure, never as a tolerant match" — matching the project's fixture rule.
- **Task 3 is the only gate task in the phase that gets the convention right** (`STATUS=$?` on the next line, no pipe, line 114), and it correctly runs `--linux` because the subscriber thread and `TcpListener` double are platform-sensitive.

#### 3. Concerns

- **MEDIUM — an existing test is invalidated and never mentioned.** `baude/src/app.rs:12391 fn sidebar_action_d_refuses_remote_rows()` asserts exactly the mapping Task 2 inverts ("map `ActionKind::Remote` with the D key to `SidebarAction::RemoveProject` instead of the refusal", line 95). The plan rewrites `d_on_remote_row_sets_footer_notice` (`:12413`) but says nothing about `:12391`, and none of Task 2's three verify filters (`d_on_remote_row_`, `enter_on_remote_confirm_`, `confirm_remove_`) matches `sidebar_action_d_refuses_remote_rows`. It will go red only in Task 3's full gate, at the end of the phase, attributed to nothing.
- **MEDIUM — `D` on a remote row puts a blocking HTTP round trip in the TUI key path.** `preview_removal` and `execute_removal` are `ureq` calls bounded by `REQUEST_TIMEOUT` (`remote.rs:17`, 3 s) issued from the key/draw thread, so a slow or hung daemon freezes the UI for up to 3 s per press — twice (preview, then confirm). This is consistent with the existing `delete`/`restart`/`create` precedent (`remote.rs:198-275`), so it is not novel; but removal is the first path where a *modal opens on the result*, and the plan says nothing about it.
- **MEDIUM — the events thread has no reachable shutdown.** Line 67/71 specify "no read timeout" and reconnect-after-`POLL_SECS`. `RemotePoller` already carries an `AtomicBool` stop flag (`remote.rs:6`), but a thread blocked reading a live `/events` socket will not observe it until the daemon writes or closes — and with `KeepAlive::default()` the next write can be ~15 s away. Switching daemons or quitting leaks a thread holding a socket for up to a keep-alive interval. T-21-11 addresses only the reconnect interval, not the blocked read.
- **MEDIUM — remote `removal_row_label` cannot satisfy D-16 for a parent removal.** The plan resolves `RowKey::Checkout` "through `remote_snap` sessions by `checkout_key` to the session name, falling back to the key debug text" (line 95). A repository parent's `rows_removed` includes checkout rows with no live daemon session (saved but not running) — those have no `RemoteInfo` and render as `Checkout(CheckoutKey(3))`. D-16 requires "every child listed by name and branch". The daemon serves exactly the missing data at `GET /rows` (21-02), but 21-06 never fetches it; `RemotePoller` only polls `/sessions` (`remote.rs:107-200`). The PWA does join `/rows` (21-05 `key_links` line 34) — so the two clients will not be at parity, which is the phase's own goal.
- **LOW — Task 2 asserts a counter Task 1 does not define.** "the double's script is empty and the test asserts it saw zero requests" (line 89), but Task 1 specifies `serve(script) -> (base_url, JoinHandle)` with no request count. The counter has to be in Task 1's `test_double` API.
- **LOW — the `ureq` API is major-version-specific.** `AgentBuilder::new().timeout_connect(…)` is `ureq` 2.x (`ureq = { version = "2", features = ["json"] }`), correct for the pin, but worth noting as a 3.x migration point.

#### 4. Suggestions

- Add `sidebar_action_d_refuses_remote_rows` (`app.rs:12391`) to Task 2's behavior list as a rewrite, and widen the verify filter to `app::tests::sidebar_action_d_` so it fails in its own task.
- Have `RemotePoller` also poll `GET /rows` (or cache the rows from the preview report's own structure) so remote labels can name every child by name and branch, closing the D-16 parity gap.
- Set a read timeout on the events agent (e.g. keep-alive interval + margin) so the loop re-checks the stop flag, and record the blocking-key-path tradeoff in the SUMMARY even if it is accepted.
- Add a request counter to `test_double::serve`'s return value in Task 1.

#### 5. Risk Assessment

**MEDIUM.** It modifies the phase's largest file (`baude/src/app.rs`, 15,948 lines) and inverts a keymap arm with existing coverage, but the local Phase 20 path is explicitly fenced off and every daemon-side contract it consumes is already landed and tested by waves 1-4. Its own full-gate task is the phase's real safety net.

---

### Cross-plan comparison

**Dependency ordering.** Waves 1→2→3→4 are fully serial, and the chain is longer than the data requires. 21-04 Task 1 (`tokio sync`, `DaemonEvent`, `GET /events`) needs only `remove_rows` from 21-01; the genuine 21-03 dependency is Task 2 alone, which reuses the forced-failure seams. Likewise 21-02 Task 1 (`/rows`, `checkout_key`) has no dependency on 21-01's removal code beyond the test fixture. Splitting 21-04 so its Task 1 runs in wave 2 would shorten the critical path by a wave with no real risk. The wave-5 fan-out (21-05 ∥ 21-06) is correct and the file spaces are disjoint (`bauded/web/*` + `web.rs` vs `baude/*`).

**The gate gap.** `bauded/src/web.rs:20` is `include_bytes!(concat!("../web/", $path))`, so 21-05's new `/removal.js` route and the `bauded/web/removal.js` file must land together or `cargo build -p bauded` fails. 21-06 Task 3 is the only full-gate task in the phase, it sits in the *same wave* as 21-05, and it compiles `bauded`. If the two run concurrently, 21-06's gate can go red on 21-05's half-finished diff; if 21-05 lands second, the phase closes with no gate over the final tree — 21-05's strongest verify is `cargo build -p bauded`. **Recommend a wave-6 gate task owned by neither plan**, running `scripts/gate.sh` and `scripts/gate.sh --linux` with statuses read from variables.

**Pipe-in-gate discipline is inconsistent.** 21-06 Task 3 does it right; 21-05 Tasks 2 and 3 both judge through pipes, and 21-05 Task 3's `grep -c … | grep -qv '^0$'` is additionally a no-op assertion. Every other plan's `cargo test` verify uses the correct redirect-then-`$?` form.

**Error-text register diverges across three clients.** The daemon's 500/503 bodies are `RemovalError` Display strings with `{row:?}` debug formatting (`removal.rs:260-281`), e.g. `row Checkout(CheckoutKey(3)): runtime survived the kill stage: …`. 21-05 D-15 renders `e.message` verbatim in the user's modal; 21-06 renders the error text in the TUI modal. Both therefore show Rust debug text to users. The TUI already owns `removal_row_label` (`app.rs:3819`) and `tilde_path` (`ui.rs:2646`) for exactly this. One decision recorded once beats three divergent renderings discovered in Phase 22.

**Requirements coverage is complete and the shared-ID gate will hold.** API-01 → 21-01/02/03/06; API-02 → 21-04/05/06; API-03 → 21-02/05. No requirement sits in a single plan, so no plan can close one alone and the `requirements.ready-ids` gate stays open until 21-05 and 21-06 both land.

**Source grounding is unusually good, and the misses cluster in one place.** Of roughly forty line citations I checked, the great majority are exact — including all four `git::discover_repository` call sites (`manager.rs:921, 1073, 1347, 1418`), `lifecycle.rs:2839`, `Cargo.toml:27`, `sw.js:4-5`, `persist.rs:352`, `repository.rs:1229`, and every one of 21-06's eight `app.rs` ranges. The three real misses are all in `read_first` ranges: 21-03's pty seam (points at core's own test module, not `pty.rs:914`), 21-05's non-existent `rowHtml`, and 21-05's `renderList` off-by-four. That is the most expensive place to be wrong, since a bad pointer burns executor context before any code is written. Given every plan carries `confidence: low` and RESEARCH was cut short by context exhaustion (STATE.md `stopped_at: context exhaustion at 78% (2026-11-05)`), a standing "re-grep each anchor before reading" instruction would pay for itself — several plans already say this, inconsistently.

**RESEARCH errors are corrected but not recorded.** 21-04 silently overrides A1 (tokio `sync`) and A2 (`futures_util::stream::unfold` — unusable, since `futures-util` is `[dev-dependencies]` only); 21-05 silently overrides A6 (no JS harness) and CONTEXT D-02/D-13's "clearings" wording. All four corrections are right. None is written down, so Phase 22 can re-derive them from the same stale sources.

**Planning bookkeeping has drifted.** `.planning/STATE.md` reports `current_phase: 20`, `status: executing`, `completed_plans: 19` of 25, while all four Phase 20 SUMMARYs exist on disk and Phase 21's plans are written and reviewed. 21-01's `depends_on: []` / `wave: 1` is still safe because Phase 20 really is complete, but the state files will mislead the wave scheduler and anyone resuming cold.

### Overall risk assessment

**MEDIUM.**

What lowers it: the single highest-consequence unknown — whether `RemovalReport` can be used as an axum extractor and parsed by the TUI without touching `baude-core` — resolves in the plans' favour (`removal.rs:179`), which is what keeps 21-01's and 21-06's `files_modified` honest. Every error status is driven by a real cause rather than a stub. The daemon never acts on client-supplied paths. The test harness already isolates `config_dir()` and the workspace (`api.rs:721-742`, proven by `persist.rs:1265`), so the suppression work cannot touch the developer's machine. The core operation is unchanged and already tested in Phase 18, and the TUI's local path is explicitly fenced off in 21-06.

What raises it: ~306k estimated tokens across six `confidence: low` plans; the phase's only full gate sits inside a plan that shares a wave with the one most likely to break the build; two verify commands in 21-05 cannot fail correctly; the XSS mitigation has no automated evidence; and three concrete behavioural gaps are unaddressed — `RuntimeHandleMissing` on the daemon (21-01/21-03), the invalidated `sidebar_action_d_refuses_remote_rows` test (21-06), and the remote-label/D-16 parity gap (21-06).

**The five changes I would make before executing**, in order:

1. Move the phase's full gate into its own wave-6 task (fixes the 21-05/21-06 collision and the no-gate-on-final-tree gap).
2. Rewrite 21-05's two piped verify commands in the `STATUS=$?` form 21-06 Task 3 already uses.
3. Add `sidebar_action_d_refuses_remote_rows` (`app.rs:12391`) to 21-06 Task 2's scope and widen its verify filter.
4. Give T-21-09 a testable seam — move the `innerHTML` interpolation into a pure `renderConfirmation()` in `removal.js` so `node --test` can assert escaping.
5. Add the `RuntimeHandleMissing` case to 21-03 and decide its status code deliberately rather than letting it default to 500.

---

## Codex Review

Source inspection covered all six plans. This is a static review; I did not run the Rust build or test suites in the read-only workspace.

### 21-01

**Summary**

The daemon wrapper follows the right ownership boundary, but two implementation details need correction: saving the core candidate can overwrite newer metadata on surviving sessions, and the proposed fixtures do not reliably exercise live runtimes.

**Strengths**

- Reusing core preserves the existing kill → disk → persistence → clearing sequence. Core retains all supplied handles on failure and consumes them after commit, matching the proposed custody wrapper. See [removal.rs:407](baude-core/src/removal.rs:407) and [removal.rs:436](baude-core/src/removal.rs:436).
- Recomputing the report before execution matches the TUI’s existing comparison of rows, runtimes and deleted paths. See [app.rs:4015](baude/src/app.rs:4015).

**Concerns**

- **HIGH — Removal can overwrite surviving sessions’ saved metadata.** Ordinary daemon saves call `state_for_save()`, which copies current runtime metadata into the persisted state. Core removal instead clones its supplied `RepositoryState` and saves that candidate directly. For example, archiving session B changes its runtime and saves the merged projection; subsequently removing A through the proposed closure can persist B’s older archive value. See [manager.rs:719](bauded/src/manager.rs:719), [manager.rs:779](bauded/src/manager.rs:779), [manager.rs:2469](bauded/src/manager.rs:2469), and [removal.rs:445](baude-core/src/removal.rs:445).
- **HIGH — Bare `sleep 30` is an invalid live-runtime fixture.** The daemon resolves permission flags before spawning, and the default resolver appends `--dangerously-skip-permissions`. Bare `sleep` rejects that argument and exits. The existing Docker smoke test explicitly documents this problem and uses a shell wrapper. A nonzero persisted PID and a listed session do not establish liveness. See [manager.rs:1231](bauded/src/manager.rs:1231), [permission.rs:98](baude-core/src/permission.rs:98), and [ci.yml:78](.github/workflows/ci.yml:78).
- **MEDIUM — The custom save path omits persistence-status bookkeeping.** The plan updates `persistence_dirty` on success but does not specify maintaining `persistence_error` on failure or clearing it after recovery. `/info` exposes both fields, while ordinary `save_checked()` maintains them together. See [manager.rs:729](bauded/src/manager.rs:729) and [manager.rs:752](bauded/src/manager.rs:752).

**Suggestions**

- Merge current survivor metadata into the removal candidate while preserving core’s rollback behavior.
- Add a regression test: archive B, remove A, reload the state, and verify B remains archived.
- Use an argument-tolerant command double and assert the owned process is alive before execution and gone afterward.
- Specify persistence-status updates for failure, recovery and `SavedDegraded`.

**Risk Assessment: HIGH.** The proposed persistence adapter can regress unrelated rows, and the tracer’s live-process proof is currently unreliable.

### 21-02

**Summary**

The hierarchy projection is necessary, but this plan contains a direct compatibility-test contradiction and assumes daemon row classifications and standalone support that the source does not provide.

**Strengths**

- A separate hierarchy projection fits the actual durable model: repository parents, checkout children and standalone rows already have distinct core identities. See [repository.rs:336](baude-core/src/repository.rs:336), [repository.rs:348](baude-core/src/repository.rs:348), and [repository.rs:453](baude-core/src/repository.rs:453).
- Leaving suppression writes in core avoids duplicating the surviving-parent rule. Core already adds entries only for removed checkouts whose repository remains. See [removal.rs:752](baude-core/src/removal.rs:752).

**Concerns**

- **HIGH — The unchanged compatibility test cannot pass.** It explicitly includes `checkout_key` in its forbidden-field list. Adding that field to every `SessionInfo` contradicts the plan’s requirement to preserve the test unchanged. See [api.rs:1002](bauded/src/api.rs:1002).
- **HIGH — Real standalone parity is missing.** A persisted plain folder session currently goes through `record_checkout_intent()`, which creates a repository and checkout even when Git discovery fails. Manager has only a checkout runtime registry, and restore iterates checkouts. Seeding an inactive standalone directly in a test proves removal of synthetic state, but does not make actual daemon folder sessions reachable as standalone rows. A standalone with persisted ownership also requires a runtime handle at core preflight. See [manager.rs:879](bauded/src/manager.rs:879), [manager.rs:1073](bauded/src/manager.rs:1073), [manager.rs:128](bauded/src/manager.rs:128), and [removal.rs:862](baude-core/src/removal.rs:862).
- **MEDIUM — The role and branch projection is underspecified.** Plain creation currently assigns `CheckoutRole::ManagedBranch`, including for a main checkout. Core roles are `Main`, `PrimaryDefault` and `ManagedBranch`; observed branches can contain `refs/heads/topic`. The tests expect `"main"`, `"worktree"` and `"topic"` without defining the normalization or admission correction. See [manager.rs:1110](bauded/src/manager.rs:1110), [repository.rs:284](baude-core/src/repository.rs:284), and [lifecycle.rs:2163](baude-core/src/lifecycle.rs:2163).
- **MEDIUM — Recreating a branch does not necessarily exercise suppression lifting.** Managed paths include the newly allocated checkout key. Removing and recreating `"topic"` normally produces a different path, while suppression matches exact paths. Checking that the new path is unsuppressed can pass without lifting anything. See [git.rs:1823](baude-core/src/git.rs:1823), [lifecycle.rs:2078](baude-core/src/lifecycle.rs:2078), and [suppression.rs:73](baude-core/src/suppression.rs:73).

**Suggestions**

- Amend the compatibility test deliberately: permit the additive identity field while retaining assertions against hierarchy nesting.
- Add production standalone admission and runtime custody, with an HTTP-created live folder fixture.
- Define projection normalization and correct main-checkout classification explicitly.
- Prove suppression lifting by seeding the exact subsequently admitted path; distinguish it from branch recreation at a fresh path.

**Risk Assessment: HIGH.** As written, the plan cannot satisfy its unchanged test and does not deliver standalone parity for normal daemon sessions.

### 21-03

**Summary**

The failure-path coverage is useful and appropriately observable. It needs additional cases for irreversible partial progress, committed-but-degraded persistence, and runtime-only drift.

**Strengths**

- The tampered-path test checks the correct boundary: core derives disk actions from durable checkout ownership rather than request paths. See [removal.rs:1073](baude-core/src/removal.rs:1073).
- A suitable teardown injection seam already exists in Manager, so no public PTY accessor is necessary. See [manager.rs:2525](bauded/src/manager.rs:2525).

**Concerns**

- **MEDIUM — Restoring handles does not reverse completed effects.** Core explicitly allows earlier runtimes to be dead after a parent-removal abort. Disk removal also precedes persistence, so a 503 can leave a worktree deleted while its row remains. The planned single-session assertions do not exercise these retry conditions. See [removal.rs:429](baude-core/src/removal.rs:429), [removal.rs:454](baude-core/src/removal.rs:454), and the existing core retry fixture at [removal.rs:2822](baude-core/src/removal.rs:2822).
- **MEDIUM — Committed replacement failure is untested at the daemon boundary.** Core converts an after-replacement save error into successful `SavedDegraded`; it must not become a 503 with restored handles. The proposed tests cover only before-replacement failure. See [removal.rs:1039](baude-core/src/removal.rs:1039).
- **MEDIUM — Runtime-only drift is not independently tested.** Adding a child exercises row-set drift, and tampering exercises path drift. Neither catches an implementation that omits the runtime comparison when rows and paths remain unchanged. The reference TUI compares all three independently. See [app.rs:4023](baude/src/app.rs:4023).

**Suggestions**

- Add a parent fixture where one child stops before another fails, then verify all handles remain addressable and retry succeeds.
- Cover managed-worktree deletion followed by persistence failure and refreshed retry.
- Add `DirectorySync` failure → 200 `SavedDegraded`, plus runtime-only drift → 409.
- Reuse `fail_next_agent_teardown_for_test()`.

**Risk Assessment: MEDIUM.** The basic mappings are sound; the gaps concern the failure transitions most likely to expose adapter mistakes.

### 21-04

**Summary**

The broadcast design fits the existing SSE implementation. The main weaknesses are an overstated ordering guarantee and tests that do not fully establish publication timing or route-level lag behavior.

**Strengths**

- Reusing `async_stream`, axum SSE and keep-alives follows both existing session streams and requires no new stream adapter dependency. See [api.rs:614](bauded/src/api.rs:614) and [bauded/Cargo.toml:12](bauded/Cargo.toml:12).
- Publishing after core returns successfully includes both normal commit and committed replacement with degraded durability. Core establishes that distinction before returning. See [removal.rs:1039](baude-core/src/removal.rs:1039).

**Concerns**

- **MEDIUM — Publishing under the lock does not prevent client resurrection.** It orders server mutations, but an earlier GET response can still arrive or be applied after the SSE event. Both current clients replace their snapshots unconditionally. See [app.js:137](bauded/web/app.js:137) and [remote.rs:147](baude/src/remote.rs:147).
- **MEDIUM — The ordering test can pass with an incorrectly early send.** The plan awaits POST completion before reading the event. An event queued before persistence would still be read after the file changed, satisfying the proposed assertion. See [21-04-PLAN.md:65](.planning/phases/21-daemon-and-pwa-parity/21-04-PLAN.md:65) and [21-04-PLAN.md:88](.planning/phases/21-daemon-and-pwa-parity/21-04-PLAN.md:88).
- **MEDIUM — Direct receiver lag testing does not prove the SSE bridge continues.** Seeing `Lagged` followed by `Ok` on a broadcast receiver tests channel behavior. The route’s own error-handling loop must be driven through lag. See [21-04-PLAN.md:89](.planning/phases/21-daemon-and-pwa-parity/21-04-PLAN.md:89).

**Suggestions**

- State the lock guarantee narrowly and require stale-response protection in both clients.
- Observe the receiver while an injected save boundary remains incomplete.
- Overflow an actual `/events` subscription before reading its body.
- Verify `SavedDegraded` emits exactly one event.

**Risk Assessment: MEDIUM.** The mechanism is straightforward, but current tests overstate the ordering properties they prove.

### 21-05

**Summary**

Separating pure report transformations from browser wiring is useful. The plan needs stronger handling and tests for asynchronous refreshes, newly introduced confirmation rows, and actual DOM behavior.

**Strengths**

- The view model accounts for the real report fields, including `already_removed`, `cleared` and `clearing_failures`. These have distinct semantics in core and should remain distinct in the confirmation. See [removal.rs:180](baude-core/src/removal.rs:180).
- Asset routing, script ordering and service-worker registration are included. They are necessary because assets are explicitly embedded and the shell list is explicit. See [web.rs:27](bauded/src/web.rs:27), [index.html:20](bauded/web/index.html:20), and [sw.js:5](bauded/web/sw.js:5).

**Concerns**

- **HIGH — A pending refresh can restore removed rows.** `refresh()` assigns the fetched session array directly. Adding `/rows` alongside it does not coordinate those responses with SSE or execute success. A pre-removal response applied afterward can repopulate the list until another poll. See [app.js:137](bauded/web/app.js:137).
- **HIGH — A 409 report can contain children absent from the name lookup.** `RemovalReport` carries row keys, without names or branches. Re-rendering a fresh report against stale `state.rows` cannot list newly added children by name and branch as required. See [removal.rs:180](baude-core/src/removal.rs:180) and the planned re-render at [21-05-PLAN.md:96](.planning/phases/21-daemon-and-pwa-parity/21-05-PLAN.md:96).
- **MEDIUM — Pure-data tests do not validate DOM escaping or controller behavior.** Preserving `<` in a view-model value does not prove the renderer escapes it. Existing cards are also outer `<button>` elements, so placing a remove button inside them would create invalid nested controls or conflicting navigation. See [app.js:33](bauded/web/app.js:33), [app.js:612](bauded/web/app.js:612), and [app.js:641](bauded/web/app.js:641).
- **MEDIUM — Local success races with “removed elsewhere.”** The daemon publishes before the POST completes, so the initiating tab may receive its own event first. The planned unconditional modal-close notification can misclassify its own operation; repeated confirmation and late responses are unspecified. See [21-05-PLAN.md:114](.planning/phases/21-daemon-and-pwa-parity/21-05-PLAN.md:114).
- **MEDIUM — New JS tests are outside the repository gates.** `gate.sh` runs Rust checks and shell guards, while the plan also uses source greps as verification despite the repository explicitly forbidding source-grep assertions. See [gate.sh:63](scripts/gate.sh:63), [gate.sh:107](scripts/gate.sh:107), and [CLAUDE.md:24](CLAUDE.md:24).

**Suggestions**

- Invalidate pending refresh generations on removal; discard older responses and launch reconciliation.
- Refresh label metadata after 409 while keeping destructive scope authoritative from the report.
- Use sibling navigation/action controls and test their event behavior.
- Track an in-flight confirmation, disable duplicate submits, and make POST/SSE completion idempotent.
- Add controller/rendering tests for these races and escaping, and run the JS suite in CI with Node provisioned.

**Risk Assessment: HIGH.** The happy path is specified well, but normal request ordering can violate live removal and confirmation completeness.

### 21-06

**Summary**

Reusing the modal is appropriate, but the file inventory is incomplete, the inherited timeout is too short for some valid removals, and the test double conflicts with the poller it intends to test.

**Strengths**

- The existing App tick already detects snapshot changes, reconciles selection and closes an attach whose session disappeared. Updating the shared remote snapshot can reuse this path. See [app.rs:4888](baude/src/app.rs:4888) and [app.rs:4926](baude/src/app.rs:4926).
- Typed handling of 409 and preservation of daemon error bodies improves on the current client’s status-only error reporting. See [remote.rs:271](baude/src/remote.rs:271).

**Concerns**

- **HIGH — `ui.rs` is a required missing file.** Its modal pattern explicitly enumerates every field, calls both label helpers with their current signatures, and constructs the modal in tests. Adding `origin` and changing helper arguments requires updating these sites; the listed two-file change will not compile unchanged. See [ui.rs:1776](baude/src/ui.rs:1776) and [ui.rs:3818](baude/src/ui.rs:3818).
- **HIGH — The three-second request timeout is insufficient.** Core stops rows sequentially; each session can stop both agent and shell, and each PTY reader join can take two seconds. Valid removal can therefore outlast `REQUEST_TIMEOUT`, producing a client error while the daemon continues and potentially commits. See [remote.rs:17](baude/src/remote.rs:17), [removal.rs:916](baude-core/src/removal.rs:916), [session.rs:495](baude-core/src/session.rs:495), and [pty.rs:901](baude-core/src/pty.rs:901).
- **HIGH — Polling can overwrite an SSE drop.** The poller fetches sessions, fetches `/info`, then replaces the complete snapshot. A removal event arriving between those operations can be undone by the already-fetched session array. See [remote.rs:147](baude/src/remote.rs:147) and [remote.rs:176](baude/src/remote.rs:176).
- **MEDIUM — The scripted HTTP double races background polling.** `start_without_events()` still starts the poller, whose first requests are GET `/sessions` and `/info`. Those can consume a script expecting POST `/removals/preview`. The single base URL also provides no specified way to point polling at a refusing port while SSE uses the double. See [remote.rs:139](baude/src/remote.rs:139) and [21-06-PLAN.md:68](.planning/phases/21-daemon-and-pwa-parity/21-06-PLAN.md:68).
- **MEDIUM — Subscriber lifetime is asserted but not implemented.** `RemotePoller` currently has no shutdown token or owned thread handle; its poll thread loops indefinitely. A second thread with an unlimited blocking read does not automatically share the object’s lifetime. See [remote.rs:133](baude/src/remote.rs:133) and [remote.rs:145](baude/src/remote.rs:145).

**Suggestions**

- Add `ui.rs` and its modal fixtures to the plan.
- Define a removal-specific timeout and reconcile ambiguous timeout outcomes through SSE or a fresh daemon read.
- Reject stale poll results after an event or successful removal.
- Provide a client constructor with background workers disabled; test polling and streaming separately.
- Add explicit subscriber shutdown and a bounded way to interrupt stalled reads.
- Include `checkout_key` in `RemoteInfo::PartialEq`, which currently controls meaningful snapshot-change detection at [remote.rs:80](baude/src/remote.rs:80).

**Risk Assessment: HIGH.** Missing renderer changes block compilation; timeout and snapshot races affect real removal behavior.

### Cross-plan assessment

The shared-core architecture is sound, but the plans need revision before execution. The highest-priority fixes are:

1. Preserve surviving runtime metadata during removal saves.
2. Resolve the compatibility-test contradiction and implement real standalone admission/custody.
3. Replace invalid live-runtime fixtures.
4. Coordinate polling with SSE in both clients.
5. Complete the TUI renderer changes and removal timeout behavior.

The final repository gates also need to depend on **both** client plans. Currently 21-05 and 21-06 share wave 5, while the full gates live inside 21-06; they can finish before the PWA changes land.

Finally, update the scope documents: [REQUIREMENTS.md:82](.planning/REQUIREMENTS.md:82) excludes remote TUI removal, while [21-CONTEXT.md:32](.planning/phases/21-daemon-and-pwa-parity/21-CONTEXT.md:32) explicitly includes it. The newer decision supports proceeding, but the contradiction should be removed.

**Overall risk: HIGH until these concrete gaps are addressed.**

---

## Consensus Summary

Both lanes read the repo and both agree on the architecture: wrapping `baude-core::removal` behind the daemon is the right ownership boundary, the echo-verbatim `RemovalReport` contract is viable without touching `baude-core`, and the daemon never acts on client-supplied paths. The disagreement is about whether the plans, as written, *compile* and whether their tests can *witness* the behaviour they claim. Claude attacks compile-time and gate-time soundness; codex attacks fixture realism and client-side state arbitration. The two lines of attack barely overlap, which is again the strongest argument for treating the corroborated set as a floor rather than a ceiling.

### Agreed Strengths

- **`Json<RemovalReport>` works as an axum extractor without changing `baude-core`.** Both lanes independently verified the `Deserialize` derives on `RemovalReport` and every nested type (`baude-core/src/removal.rs:179` and the type block below it), which is what keeps 21-01's and 21-06's narrow `files_modified` honest.
- **Reusing core preserves the kill → disk → persistence → clearing sequence and its handle custody.** Core retains supplied handles on failure and consumes them after commit (`baude-core/src/removal.rs:407`, `:436`), matching the proposed wrapper.
- **Recomputing the report before execution mirrors the Phase 20 TUI's three-set recheck** (`baude/src/app.rs:4015`–`4026`), so the drift posture is inherited rather than reinvented.
- **Error-status mapping is driven by real causes, not stubs** — the seven `RemovalError` variants are named correctly and `Busy → 503` is wired to a mechanism that exists (`baude-core/src/lifecycle.rs:2839`).

### Agreed Concerns

1. **HIGH — 21-06's three-second `REQUEST_TIMEOUT` is shorter than legitimate removal work.** Codex rates this HIGH, claude MEDIUM, and the mechanism cited is the same: `baude/src/remote.rs:17` bounds the `ureq` calls issued from the TUI key/draw thread, while core stops rows sequentially and each PTY reader join alone can wait two seconds (`baude-core/src/session.rs:495`, `baude-core/src/pty.rs:901`), followed by git removal and persistence. A valid removal can commit *after* the TUI reports a timeout, and the UI freezes for up to 3 s per press — twice, since preview and confirm are separate round trips. Corroborated, counted.
2. **MEDIUM — 21-05's verification cannot prove the `app.js` mutation flow, and its JS tests sit outside the repository gates.** Claude: 21-05's only automated coverage is `node --test` plus `node --check`, with no `scripts/gate.sh` anywhere in the plan. Codex: `scripts/gate.sh:63`–`:107` runs Rust checks and shell guards only, and the plan leans on source greps despite `CLAUDE.md` forbidding source-grep assertions. Same gap from both sides.
3. **MEDIUM — 21-04's lag test exercises Tokio, not the handler.** Both lanes: driving `Manager::subscribe_events()` directly and asserting `Lagged` then `Ok` tests `tokio::sync::broadcast`, never the route's own `continue`-on-`Lagged` loop. The clause needs a frame read off the response body.
4. **MEDIUM — 21-04's in-lock ordering claim is not provable by the test that claims it.** Claude: `GET /sessions` takes the same `Arc<Mutex<Manager>>`, so the test thread necessarily reads post-guard state. Codex: an event queued *before* persistence would still satisfy the proposed assertion, because the plan awaits POST completion before reading the event. Both conclude the test proves the safe consequence, not the stated property.
5. **MEDIUM — the phase-terminal gate does not cover the phase.** Claude and codex both close on this: 21-06 Task 3 owns the only full `scripts/gate.sh` run, 21-06 is `depends_on: [21-02, 21-04]`, and 21-05 is the other `wave: 5` plan. `bauded/src/web.rs:20` is `include_bytes!`, so 21-05's new `bauded/web/removal.js` and its route must land together or `cargo build -p bauded` fails. If the two run concurrently 21-06's gate can go red on 21-05's half-finished diff; if 21-05 lands second the phase closes with no gate over the final tree. Both recommend a gate task that depends on **both** client plans.
6. **MEDIUM — 21-05's modal cannot render what D-16 requires, and the escaping claim is tested at the wrong layer.** Claude: `style.css:310-317` caps `#modal .box` at 420px and nothing widens it or shortens paths (the TUI has `tilde_path`, `baude/src/ui.rs:2646`; the PWA has no equivalent in scope). Both lanes: `confirmationView` returns plain data, so `node --test` can only assert the view model *preserves* an angle bracket — the opposite of the property at risk. The enforcing `innerHTML` interpolation has no check.

### Divergent Views

- **21-03 and 21-04 risk.** Claude rates both LOW; codex rates both MEDIUM. Codex's two cycle-1 HIGHs on these plans (partial-failure guarantee, client resurrection) are **restated this cycle at MEDIUM**, not withdrawn — the severity moved, the finding did not.
- **Where the client-resurrection race belongs.** Codex files it at HIGH against the two client plans (21-05 `bauded/web/app.js:137`, 21-06 `baude/src/remote.rs:147`/`:176`) and at MEDIUM against 21-04. Claude does not raise client-side arbitration at all this cycle; its 21-04 concerns are about what the ordering tests can witness. This is the single largest blind spot between the two lanes.
- **21-01 overall.** Codex rates it HIGH (persistence adapter can regress unrelated rows; fixtures cannot prove liveness); claude calls it "the strongest plan in the set" at LOW. Both read the same `manager.rs` save path and drew opposite conclusions about whether bypassing `state_for_save()` matters.
- **The `ui.rs` compile break.** Codex raises it HIGH again; claude, which corroborated it in cycle 1, does **not** raise it this cycle. The source-grounding pass below confirms it independently, so the drop in corroboration does not reduce its standing.

## Source-Grounding Pass

Every existence-class HIGH was re-checked against real project source in this cycle. Verdicts are this pass's own, not a reviewer's.

| Claim | Lane | Verdict | Evidence |
|---|---|---|---|
| 21-02's required-unchanged compatibility test cannot pass | codex | **CONFIRMED** | `bauded/src/api.rs` forbidden-field loop lists `"checkout_key"` among the fields that must be absent from `/sessions` entries (inside the `965-1040` region the plan read_firsts at `21-02-PLAN.md:58`). `21-02-PLAN.md:62` adds `checkout_key` to every `/sessions` entry; `:63` requires `flat_session_api_remains_a_non_hierarchical_compatibility_projection` to pass **unchanged**. Direct contradiction — the plan cannot satisfy both. |
| 21-06 omits `baude/src/ui.rs` from `files_modified`, guaranteeing a compile break | codex | **CONFIRMED** | `21-06-PLAN.md:7-9` lists exactly `baude/src/remote.rs` and `baude/src/app.rs`. `21-06-PLAN.md:95` adds an `origin` field to `Modal::ConfirmRemove` and states "every existing constructor site passes `Local`". `baude/src/ui.rs` constructs that modal at `:3818` and matches it at `:1776` and `:3891`, and calls the two label helpers at `:1782`, `:1783`, `:3889`, `:3900`, `:3935`, `:3937`. `grep -rn 'Modal::ConfirmRemove' baude/src/` counts 5 sites in `ui.rs` against 21 in `app.rs`. The construction at `ui.rs:3818` cannot compile without the new field. Codex's cited lines (`:1776`, `:3818`) are the correct sites; its helper-call line numbers are approximate but the substantive claim holds. |
| 21-05's two automated verify commands judge through a pipe | claude | **CONFIRMED** | `21-05-PLAN.md:99` is `… && grep -c 'removal.js' bauded/web/index.html bauded/web/sw.js bauded/src/web.rs \| grep -v ':0$' \| wc -l \| grep -q '^3$' && cargo build -p bauded` — the chain's status is the last stage's, so an earlier failure is invisible and BSD `wc -l` emits leading whitespace that defeats the anchored `^3$`. `21-05-PLAN.md:117` is `… grep -c "EventSource('/events')…" bauded/web/app.js \| grep -qv '^0$' …` — a single-file `grep -c` emits one line, so the assertion passes for any non-zero count whatever the pattern matched. `21-06-PLAN.md:74`, `:98`, `:114` show the correct `STATUS=$?` form, so the discipline exists in the phase and only 21-05 breaks it. The repo `CLAUDE.md` Gate section states "Never judge a gate step by piping it to `tail` or `grep` and reading `$?`", and its Tests section forbids asserting "by grepping source". |
| 21-01's save closure can persist stale metadata for surviving sessions | codex | **CONFIRMED (mechanism)** | `bauded/src/manager.rs:719` builds every ordinary save as `StateFile::new(self.state_for_save())`, and `state_for_save` (`:779`) clones `repository_state` and then merges live runtime metadata back into each checkout from `runtime_checkouts`. `baude-core`'s removal saves the candidate `RepositoryState` it was handed (`baude-core/src/removal.rs:445`), so a closure that writes core's candidate directly skips that merge. Whether a concrete regression lands depends on the closure the executor writes, which is why this is scored as a confirmed mechanism rather than a confirmed defect. |
| 21-01's bare `sleep 30` fixture cannot prove liveness | codex | **CONFIRMED (precedent)** | `baude-core/src/permission.rs:98` and `bauded/src/manager.rs:1231` are the permission-resolution seam codex names, and `.github/workflows/ci.yml:78` uses a shell wrapper for exactly this reason. This is also the repo's own stated test doctrine (`CLAUDE.md`: "command doubles that accept every appended argument"). |

### Cross-artifact fact-drift (advisory — contributes to NEITHER count)

- `.planning/REQUIREMENTS.md:82` lists "Removing remote daemon sessions from the TUI attach view" as explicitly **out of scope**, while `21-CONTEXT.md` D-12 makes it a deliverable and 21-06 implements it. Codex flags the contradiction; the newer decision supports proceeding, but one of the two documents is wrong.
- `.planning/STATE.md` reports `current_phase: 20`, `status: executing`, while all four Phase 20 SUMMARYs exist and Phase 21 is planned and twice-reviewed. Claude flags it. Planning bookkeeping, not a plan defect.

## Verification coverage

### Lane coverage

- `claude` — ran, completed, 6/6 plans, 83 citations, no discount marker. Counted at full weight.
- `codex` — ran, completed, 6/6 plans, 146 citations, no discount marker. Counted at full weight.
- `opencode` was **not selected** this cycle (the invocation was `--claude --codex`), so its cycle-1 `ETIMEDOUT` is neither repeated nor repaired here. A third independent read remains unmeasured.

### Axes checked

- Existence of every symbol, line and plan assertion behind an existence-class HIGH — checked with `grep`/`sed` against the working tree at `f0d298f`.
- Whether the plans changed since cycle 1 — checked with `git diff` and `git log`; they did not.
- Plan-coverage completeness per lane — checked mechanically against the manifest both lanes received.
- Discount-marker presence — checked against each lane's leading blockquote; neither carries one.

### UNCHECKABLE / skipped

- All runtime and browser behaviour: SSE frame timing, the two-tab PWA human check, DOM escaping at the `innerHTML` seam, and the `--linux` gate lane.
- Codex's judgment-class HIGHs on client-side poll/event arbitration were **not** re-derived by running the clients; the cited seams (`bauded/web/app.js:137`, `baude/src/remote.rs:147`, `:176`) were confirmed to exist and to assign fetched snapshots directly, which is the premise, but the race itself is a runtime property.
- `cargo build` / `scripts/gate.sh` were not run — this is a plan review, and the plans are unexecuted.

### Counting

- Consensus gate: **active** (two lanes ran, neither discounted).
- Corroborated HIGHs: **1** — 21-06's three-second `REQUEST_TIMEOUT` (codex HIGH + claude MEDIUM on the same mechanism).
- Single-reviewer existence-class HIGHs counted after independent confirmation: **3** — 21-02's `/sessions` compatibility-test contradiction, 21-06's missing `ui.rs`, 21-05's piped verify commands.
- Single-reviewer judgment-class HIGHs counted (codex carries no discount marker): **6** — 21-01 persistence-adapter metadata overwrite, 21-01 `sleep 30` fixture liveness, 21-02 standalone parity, 21-05 pending-refresh resurrection, 21-05 409 report lacks child names, 21-06 polling overwrites the SSE drop.
- HIGHs suppressed by the gate: **none**.
- Resolved since cycle 1: **none** — the plans are unchanged, so no finding was incorporated, deferred or rejected.
- `current_high = 10`, `current_actionable = 41`. Cross-artifact fact-drift is advisory and contributes to neither count.
