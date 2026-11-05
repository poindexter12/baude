# Cross-AI Plan Review — Phase 21

<!-- gsd:plan-revision-conflicts:begin -->
## Plan-Revision Conflicts
<!-- gsd:plan-revision-conflicts:end -->

## Reviewers

Cycle 1. Two reviewer lanes produced output and both are counted:

| Lane | Model / transport | Outcome |
|---|---|---|
| `claude` | Claude (spawn), full repo access, 130k prompt budget | Completed — full six-plan review with source citations |
| `codex` | `gpt-6.1-sol`, reasoning `high` (spawn), 176.8k prompt budget | Completed — full six-plan review with source citations |
| `opencode` | spawn, no prompt cap resolved | **FAILED — `ETIMEDOUT`.** No assistant text returned (`stop reason=?`, `output tokens=?`). The lane ran with no effort argument, so the reviewer CLI's own configuration applied and the outer timeout killed it. Excluded from this cycle and from all counts. |

Neither completed section carries a `[reviewed-without-source-citations]` or `[reviewed-without-repo-access]` discount marker, so the two-reviewer consensus gate is active for this cycle.

## Claude Review


**Repository:** `/Users/joese/Code/github.com/poindexter12/baude` @ `6397c72` (branch `gsd/phase-18-shared-kill-then-remove-lifecycle`)
**Reviewed with full source access.** Every claim below was traced into the code; I note explicitly where a plan's assertion survived verification and where it did not.

**Verification note up front:** five of my initial concerns were refuted by reading the source, and I report them as verified strengths rather than findings. Specifically: `RemovalReport` **does** derive `Deserialize` (`baude-core/src/removal.rs:179`), `bauded` **already** enables `baude-core`'s `test-support` feature (`bauded/Cargo.toml:35`), `ureq` **is** pinned to 2.x (`baude/Cargo.toml:21`), the `esc()` helper **does** exist (`bauded/web/app.js:33`), and the suppression store's per-workspace suffix is derived **internally** (`baude-core/src/suppression.rs:60-66`), so passing bare `config_dir()` as the root is correct. The plans are more accurate than their own CONTEXT in several places.

---

### 21-01

#### Summary

The strongest plan in the set. It is a genuine tracer: one checkout row through `Manager` → `api.rs` → state file, with the Phase 20 TUI path as a line-cited template rather than a vague analogy. Its central design decision — making the execute body the preview `RemovalReport` echoed verbatim — resolves RESEARCH assumption A3 without inventing a second wire type, and I verified it actually compiles as specified. Every symbol it instructs the executor to mirror exists at or near the cited line, including one place where the plan **corrects** a wrong name in its own CONTEXT. Remaining issues are cosmetic.

#### Strengths

- **`Json<RemovalReport>` extraction compiles.** `RemovalReport` derives `Deserialize` at `baude-core/src/removal.rs:179`, as do every nested type it needs: `RemovalTarget` (`:48`), `RowKey` (`:57`), `KilledRuntime` (`:133`), `PersistenceOutcome` (`:142`), `ClearedItem` (`:162`), `ClearingFailure` (`:171`). The echo-verbatim contract is viable with zero changes to `baude-core`, which is why `removal.rs` is correctly absent from `files_modified`.
- **The three-set recheck is faithful to its source, including the exclusions.** `baude/src/app.rs:4024-4026` compares exactly `rows_removed`, `runtimes_killed`, `paths_deleted`, and the comment at `:4017-4019` documents *why* `cleared` and `already_removed` are excluded ("filesystem probes that would make the recheck flap on advisory state"). 21-01 inherits the same three sets and the same rationale. This is the WR-01 fix (`ca979fb`) correctly propagated.
- **The read_first line citations are accurate post-WR-commits.** I checked for drift from the four `WR-0*` commits that landed after 20-04's summary and found none: `collect_row_runtimes` at `app.rs:3948`, `restore_row_runtimes` at `:3981`, `confirm_remove_project` at `:4004` — all inside the cited `3948-4090`. `SaveError::replacement_committed` is at `persist.rs:278` (cited `250-282`). `load_for_workspace_strict_at` is at `persist.rs:352` — cited exactly.
- **The save-closure borrow problem is pre-solved by pointing at the right lines.** `removal::remove` takes `&mut RepositoryState` plus an `FnOnce` closure; the TUI resolves the resulting borrow conflict by copying the test hooks out first (`app.rs:4030-4036`). Telling the executor to build the closure "like the TUI's" at those lines hands over a working pattern instead of a hazard.
- **It correctly diverges from the TUI where the daemon differs.** The TUI saves under base `"state"` (`app.rs:4042`, `:4050`); the daemon's is `STATE_BASE = "daemon-state"` (`manager.rs:36`). The plan says `STATE_BASE` and Task 2's failure hint names `workspace.state_file(STATE_BASE)` specifically. Copy-paste would have silently written the wrong file.
- **`RepositoryReservation` is owned, so holding it across the call is sound.** `lifecycle.rs:2878-2881` shows it carries an `Arc<Mutex<..>>` and a key with no lifetime parameter, so `let _reservation = self.reserve_removal_target(..)` releases the `&self` borrow immediately. The instruction to hold the guard "for the whole call" alongside `&mut self.repository_state` compiles.
- **`Busy` is routed to 503, not 409.** The plan states the reason explicitly — D-03 reserves 409 bodies for the fresh-report JSON. `RepositoryReservations::reserve` does return `Err(LifecycleOutcome::Busy { repository })` at `lifecycle.rs:2845`, as cited.
- **`agent_pid` is a real, assertable field.** `KilledRuntime { row, agent_pid: u32, shell_pid: Option<u32> }` at `removal.rs:134-138`, so "`agent_pid` is non-zero" is a well-founded assertion rather than a guess.

#### Concerns

- **LOW — `Manager::new("sleep 30", true)` will not compile as written.** The signature is `pub fn new(claude_cmd: String, persist: bool)` (`manager.rs:468`). Every existing call site passes `.into()`: `api.rs:754` uses `Manager::new("sleep 30".into(), false)`, `manager.rs:2761` uses `Manager::new("sh -c 'sleep 30'".into(), false)`. Trivial for the executor, but the plan quotes it three times as if literal.
- **LOW — the 404 execute-body test is more tedious than the plan implies.** No field of `RemovalReport` carries `#[serde(default)]` (`removal.rs:179-200`), so the `removals_unknown_target_is_404` execute body must hand-construct all eleven fields. "A preview-shaped report whose target is that key" understates this; without a helper it is ~20 lines of JSON per test.
- **LOW — CONTEXT drift the plan silently absorbs.** `21-CONTEXT.md:53` names `take_row_runtimes` at `:3950`; the real function is `collect_row_runtimes` at `app.rs:3948`. 21-01 uses the correct name. Good, but CONTEXT stays wrong for 21-03/21-06, which also read it, and `21-CONTEXT.md:18` (D-02) and `:35` (D-13) still say "clearings" when the fields are `cleared` and `clearing_failures`.
- **LOW — `T-21-SC` is boilerplate.** "No package is added in this plan" is true here, but the identical row appears in all six threat models including 21-04, which does edit `bauded/Cargo.toml`.

#### Suggestions

- Add a `fn preview_shaped_report(target) -> serde_json::Value` test helper in the same place as `removal_fixture`, so the 404 test and 21-03's four tampering tests do not each hand-roll eleven fields.
- Fix `Manager::new("sleep 30".into(), true)` in the plan text, and prefer the repo's `"sh -c 'sleep 30'"` convention if the fixture needs a shell.
- Have the SUMMARY record the corrected `collect_row_runtimes` name so 21-03 and 21-06 do not re-import CONTEXT's error.

#### Risk Assessment

**LOW.** The wire contract is verified to compile, the recheck semantics are traceable to a specific landed fix, and the only defects are typos the compiler catches in seconds. The `confidence: low` estimate in the frontmatter is more pessimistic than the evidence warrants.

---

### 21-02

#### Summary

A well-scoped expansion with one genuinely excellent instruction and one real test gap. The standout is Task 3's refusal to assume: it tells the executor to enumerate five specific implicit-admission candidates and *record a negative finding with line numbers* if none admits rows. I verified the premise — `bauded/src/manager.rs` contains zero references to `is_suppressed` or `suppress(` — so this is real, unwired work. The gap is that the plan exercises the main-checkout removal path and separately tests suppression on the worktree path, but never crosses them, leaving the case core actually treats identically untested.

#### Strengths

- **D-11's premise is verified real.** `grep is_suppressed|suppress(` over `bauded/src/manager.rs` returns nothing, while `baude/src/app.rs:2374` has `fn lift_suppression(&mut self, path: &Path)`. The daemon genuinely can re-admit rows it just removed.
- **The suppression root is correct, and I initially thought it was not.** `STATE.md` records the store as per-workspace, which looked like a conflict with the plan's bare `config_dir()`. But `suppression.rs:64-66` shows `file_path(root) = root.join(file_name())` and `file_name()` is `workspace::active().state_file(FILE_BASE)` — the workspace suffix is derived *inside* the helper. `suppression.rs:19` states the contract directly: "All helpers take the config root explicitly so callers pass `persist::config_dir()`." The plan is right.
- **The test-isolation claim holds.** "`persist::config_dir()` is redirected by `api_scope`" is true: `api_scope` builds `TestRedirect::new(&root)` (`api.rs:730`), and `TestRedirect` replaces `config_dir` with `<root>/config` (`testing.rs:129-135`). It is thread-local and `!Send` by design (`testing.rs:85-87`, `:112-116`), which is exactly what async handler tests need.
- **The "do not call `suppress` from the daemon" instruction is correct.** `update_suppression` at `removal.rs:755` already owns the write side, inside core's clearing stage. Having the daemon write too would double-enter.
- **The repository-removal assertion matches the code.** Task 3 asserts "removing the whole repository afterwards adds no entry for the repository path." `removal.rs:777-781` does `continue` when the checkout's parent repository is also in the removed set, and the repositories loop (`:761-775`) only ever calls `drop_children`. Verified.
- **`read_first` is honest about uncertainty.** "locate each struct by grep for `pub struct Saved`" and "grep both names to confirm the list before reading" acknowledge that RESEARCH was cut short, rather than inventing line numbers.

#### Concerns

- **MEDIUM — the main-checkout suppression case is specified by core but untested.** `update_suppression` (`removal.rs:776-785`) iterates `removed.checkouts` and suppresses any whose parent repository survives — it does **not** branch on `CheckoutRole`. Task 2's `removals_main_checkout_keeps_repository_path` removes a main checkout while the repository row survives, which means core *will* write a suppression entry for the main worktree path. Task 2 asserts nothing about it, and Task 3 tests only the worktree checkout. The main-checkout case is the one most likely to surprise a user (their repository root becomes suppressed), and nothing in the phase covers it.
- **MEDIUM — the chained verify commands pass when they match zero tests.** Task 1's gate ends `&& cargo test -p bauded api::tests::sessions_carry_checkout_key && cargo test -p bauded api::tests::flat_session_api_remains_a_non_hierarchical_compatibility_projection`. `cargo test` with a filter matching nothing exits 0. Only the *first* command in the chain is guarded by the `running [1-9][0-9]*` grep; these two are not. A renamed or absent test passes silently. This class of bug already bit this phase once — `6397c72 docs(21): align 21-03 verify filter with the renamed test`.
- **LOW — `#[serde(default)]` on `SessionInfo.checkout_key` is inert.** `SessionInfo` derives `(Serialize, Clone)` only (`manager.rs:181`). `serde(default)` affects deserialization exclusively, so it does nothing here. The plan's rationale — "for symmetry with the TUI's `RemoteInfo`" — conflates the two directions; the attribute that matters is the one 21-06 adds to `RemoteInfo`, where it is load-bearing.
- **LOW — `lift_suppression` signature differs from the TUI's.** The plan specifies `fn lift_suppression(&self, path: &Path)`; the TUI's is `&mut self` (`app.rs:2374`). Likely fine for the daemon, but the plan presents it as mirroring.
- **LOW — "all four row kinds" counts three enum variants.** `RemovalTarget` has `Repository | Checkout | Standalone` (`removal.rs:49-53`). The "four" (repository / main checkout / worktree checkout / standalone) is a UI taxonomy, not a wire one. Harmless but recurs across all six plans.

#### Suggestions

- Extend `removals_main_checkout_keeps_repository_path` with the suppression assertion, or add a fourth Task 3 test for it. If suppressing a repository root on main-checkout removal is *not* wanted, that is a core behavior question that belongs in this phase, not Phase 22.
- Guard the chained filters the same way the first command is guarded, or collapse them into one `cargo test -p bauded api::tests::` run with a single count check.
- Drop `#[serde(default)]` from `SessionInfo.checkout_key` and note in the SUMMARY that the attribute belongs on `RemoteInfo` only.

#### Risk Assessment

**MEDIUM.** The `/rows` projection and the key plumbing are low-risk mechanical work, and the suppression wiring is correctly grounded. The risk is the untested main-checkout suppression path: it is the one place where this phase changes user-visible behavior in a way no test in the phase would catch.

---

### 21-03

#### Summary

The best failure-path plan in the set, and the one whose central security claim I was able to confirm is stronger than the plan itself argues. It drives real failure causes — an armed unkillable process group, a forced atomic-save failure, forced blocked persistence — rather than stubbing errors, and asserts on observable state throughout. Two concrete read_first errors and the same unguarded-filter weakness as 21-02.

#### Strengths

- **T-21-01 is mitigated by construction, not just by a test.** Because the recheck compares `paths_deleted` as one of its three sets, there is **no** execute body that both passes the recheck and carries a tampered path: any bogus entry necessarily differs from the fresh preview and yields 409. `removals_bogus_paths_change_nothing` is therefore not a sampling test, it is exhaustive over the threat. The plan undersells this by describing the paths as merely "compared, never executed."
- **The kill-failure double is reachable, contrary to my initial reading.** `baude_core::testing::arm_unkillable_runtime(pty: &Pty, holder_pid_path: &Path) -> HolderGuard` is at `testing.rs:532`, behind the `test-support` feature — and `bauded/Cargo.toml:35` already declares `baude-core = { workspace = true, features = ["test-support"] }` in `[dev-dependencies]`, with a comment explaining the resolver-v2 rationale. No `Cargo.toml` change is needed, which is why `files_modified` correctly omits it.
- **The fallback seam is also real.** `Pty::fail_next_teardown_for_test` is gated `#[cfg(debug_assertions)]` (`pty.rs:912-914`), not `#[cfg(test)]`, so it *is* callable from `bauded`'s debug test build. The plan's three-tier hedge ("the TUI recipe … else the pty seam … add an accessor only if neither is reachable") is correctly ordered against what the code actually permits.
- **`AtomicFailure` is a real injection point.** `manager.rs:140` holds `atomic_failure_for_test: Option<persist::AtomicFailure>`, and `save_checked` consumes it at `manager.rs:721-722` then clears it at `:747` — so forcing a 503 and then retrying works exactly as the plan assumes.
- **Every assertion is on observable state.** Sessions list, disk bytes, `ps` status, `checkout_key_for_runtime_for_test`. Nothing greps source or log text, honoring the CLAUDE.md rule.
- **The retry-after-cause-clears test closes the loop.** Releasing the holder and re-running to a 200 proves the Err path genuinely restored handles rather than leaving the registry corrupt.

#### Concerns

- **LOW — `read_first` sends the executor to the wrong file for `AtomicFailure`.** The plan says "baude-core/src/testing.rs: AtomicFailure (grep `pub struct AtomicFailure`)". It is `pub enum AtomicFailure` at `baude-core/src/persist.rs:242` — wrong file, wrong item kind. The prescribed grep returns nothing.
- **LOW — the `fail_next_teardown_for_test` citation points at a call, not a definition.** The plan cites `baude-core/src/removal.rs lines 2255-2300`. `removal.rs:2264` is a *usage* inside removal's own tests; the definition is `pty.rs:914`. Reading only the cited range tells the executor the call shape but not the `cfg(debug_assertions)` gating that makes it reachable — the single fact that decides whether this seam is usable.
- **MEDIUM — unguarded chained filters, same as 21-02.** Task 1's gate ends `&& cargo test -p bauded api::tests::removals_bogus_paths_change_nothing && cargo test -p bauded api::tests::removals_execute_ignores_unknown_fields`. Both pass on zero matches. These are the two tests carrying the plan's security claim, so a silent miss here loses the T-21-01/T-21-05 evidence specifically.
- **LOW — `removals_execute_ignores_unknown_fields` proves less than its name says.** It demonstrates that serde drops extras (correct: `RemovalReport` has no `deny_unknown_fields`, `removal.rs:179`). It does not demonstrate that the eight *known* non-compared fields (`planned`, `paths_kept`, `already_removed`, `readers_parked`, `persistence`, `cleared`, `clearing_failures`) have no influence — which is the contract the plan states in the same sentence. A body with a hostile `paths_kept` or `cleared` would be a sharper test.

#### Suggestions

- Correct the read_first to `baude-core/src/persist.rs:242` for `AtomicFailure` and `baude-core/src/pty.rs:912-914` for the teardown seam, noting the `cfg(debug_assertions)` gate.
- Guard the chained filters, as in 21-02.
- Rename to `removals_execute_ignores_non_compared_fields` and tamper with a *known* field (`paths_kept`, say) to match the stated contract.
- Record in the SUMMARY that the three-set recheck makes path tampering structurally impossible — that is a stronger security statement than the threat table currently makes, and Phase 22 should inherit it.

#### Risk Assessment

**LOW.** The failure causes are real, the seams verifiably exist, and the security property is stronger than claimed. The read_first errors cost the executor minutes, not correctness.

---

### 21-04

#### Summary

A clean, correctly-scoped SSE plan. The existing per-session streams give it a working template, the dependency it needs is already present, and the "no event on failure" tests are the right shape. One instruction rests on a factually wrong reading of the Cargo manifest — though the action it prescribes is still the right one — and D-07's structural half is not test-observable, which the plan does not acknowledge.

#### Strengths

- **`async-stream` is already a first-class dependency.** `bauded/Cargo.toml:12` declares `async-stream = "0.3"` as a regular (not dev) dependency, and both existing SSE handlers use `async_stream::stream!` (`api.rs:614`, `:658`). RESEARCH assumption A2's worry about needing `futures_util::stream::unfold` is moot, and no new package is required — so `T-21-SC`'s "only the tokio sync feature of an already-vendored crate" is accurate for this plan.
- **The template is precisely cited and real.** `api.rs:608-642` and `:652-686` show the exact pattern the plan copies: `Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>`, `Event::default().event(..).data(..)`, `KeepAlive::default()`. The handler-level up-front guard idiom (`lock(&state).transcript_path(id).map_err(not_found)?` before building the stream) is mirrored by "calls `lock(&state).subscribe_events()` before building the stream."
- **No deadlock in the publish path.** `Shared = Arc<Mutex<Manager>>` with a poison-recovering `lock()` (`manager.rs:38`, `:106-108`). `broadcast::Sender::send` never blocks, and the `/events` handler's `subscribe_events()` is a temporary guard dropped before any `await` — matching how `transcript_path` is already used.
- **Failure-path coverage is the right two tests.** `events_no_event_when_persist_fails` and `events_no_event_when_runtime_survives` reuse 21-03's real seams, so "no event" is proven against genuine 503/500 causes rather than a stub.
- **Discretion is exercised and justified, not hand-waved.** Capacity 64 with skip-on-`Lagged` is tied to a stated reason (the 3 s poll reconciles), which is exactly what CONTEXT's "Claude's Discretion" asked the planner to decide.
- **`events_stream_has_keep_alive_and_no_history` is correctly hedged.** `KeepAlive::default()` fires at 15 s, so within the 500 ms window there will be no bytes at all; the plan writes "the first bytes, **if any**", which is right.

#### Concerns

- **LOW — the stated reason for adding `"sync"` is wrong, though the action is correct.** The read_first asserts "bauded/Cargo.toml line 27 (tokio features: sync is absent and must be added explicitly; RESEARCH A1 is not relied on)". `sync` is indeed absent from the explicit list at `:27` — but the crate already compiles two `tokio::sync` uses today: `tokio::sync::mpsc::channel::<Vec<u8>>(64)` at `api.rs:562` and `use tokio::sync::Notify` at `manager.rs:12`. So `sync` is already transitively enabled via feature unification. Adding it explicitly is good hygiene (it stops a future dependency bump from silently removing it), but the plan believes it is fixing a compile error that does not exist, and a SUMMARY written from that belief will record a false fact.
- **MEDIUM — D-07's "inside the same lock" cannot fail any test in this plan.** `events_published_after_state_file_dropped_the_row` asserts that when the frame is read, the state file already lacks the key. But `broadcast::send` is synchronous and the subscriber reads strictly later, so this assertion holds identically whether the send is inside the lock or just after it is released. The test proves "the event post-dates the durable save" — which `<done>` states accurately — but `must_haves.truths` line 2 claims the stronger lock-scoped property, and no test would go red if a refactor moved the send out. For D-07 the lock scope *is* the safety property.
- **LOW — `events_lagged_subscriber_keeps_streaming` tests two different things and bridges them by assertion.** It drives `Manager::subscribe_events` directly to observe `Lagged`, then separately claims "the `/events` route built over the same receiver semantics yields a message after the lag." The second half is an inference about the route, not a measurement of it. Since skip-on-`Lagged` is the discretionary choice most likely to be wrong, it deserves a frame read from the real route.
- **LOW — the `/events` handler is unauthenticated and unbounded in connection count.** `T-21-07` accepts this as "same connection posture as the existing per-session streams," which is defensible under the tailnet model (`api.rs:1-2`). But the per-session streams are at least keyed to a session id that must exist; `/events` is a bare daemon-wide subscribe with no precondition. Worth naming as a difference rather than an equivalence.

#### Suggestions

- Reword the read_first to: "`sync` is absent from the explicit list at `bauded/Cargo.toml:27` but already transitively enabled (`api.rs:562`, `manager.rs:12`); add it explicitly so a future dependency change cannot remove it." This keeps the action and corrects the record.
- To make D-07 structurally enforceable, have `remove_rows` publish from a position where the borrow checker or a type guarantees lock custody — for example by taking the report and sender in a small helper that is only callable with `&mut self` — and say so in the SUMMARY. Failing that, state plainly that D-07's lock scope is enforced by code review, not by test.
- Read at least one real frame through the `/events` route after forcing the lag, rather than inferring route behavior from channel behavior.

#### Risk Assessment

**LOW.** The dependency is present, the template is proven in-tree, and the failure cases are covered with real causes. The D-07 gap is a durability-of-intent problem rather than a correctness one today.

---

### 21-05

#### Summary

Strong design, weakest verification. Extracting the PWA's pure view model into a Node-testable `removal.js` that mirrors the TUI's `remove_project_confirmation_lines` is exactly the right answer to "no JS test harness exists," and the TUI parity target is real and well-chosen. But this plan carries three distinct verification defects I confirmed by execution: its Task 2 gate **cannot pass on macOS**, its new test runner is **never wired into the repo gate**, and the XSS mitigation it claims is enforced at a layer that **has no automated test at all**.

#### Strengths

- **The TUI parity target exists and includes the hardest case.** `remove_project_confirmation_lines` is at `baude/src/ui.rs:2509`, with tests at `:3612`, `:3659`, `:3695`, and — most relevantly — `remove_project_lines_already_removed_and_empty_sections` at `:3735`. 21-05 Task 1's `already_removed` test has a direct, named analog to mirror, and D-16's "a path is never shown under deleted unless the report lists it there" is already a tested TUI invariant.
- **`esc()` exists with established precedent.** `bauded/web/app.js:33` defines it, and the codebase already carries threat-numbered comments enforcing its use: `:285` ("Every event-origin field is esc()-escaped before innerHTML — XSS, T-03-09") and `:705` ("esc() EVERY dynamic string before innerHTML, XSS, T-04-13"). T-21-09 is continuing a real convention, not inventing one.
- **The field shape in `read_first` is correct and defensively worded.** "there is no `clearings` field and no `ClearingResult` type, so the view model reads `report.cleared` and `report.clearing_failures`" — verified against `removal.rs:180-200`, `ClearedItem` at `:163`, `ClearingFailure` at `:172`. This is the plan overriding its own CONTEXT (`21-CONTEXT.md:18`, `:35`), correctly.
- **The fixture-fidelity rule is stated as a hard gate.** "a fixture with a field the daemon never emits is a defect in the fixture, not a passing test" directly enforces CLAUDE.md's "fixtures must model real shapes."
- **The asset plumbing is completely specified.** Adding `/removal.js` needs all three of a `web.rs` route (routes are explicit per-asset at `web.rs:29-46`, not a directory serve), a `sw.js` `SHELL` entry (`sw.js:5`), and the `CACHE` bump. `CACHE = "baude-v6"` at `sw.js:4` confirms `v6`→`v7`. The plan names all three; missing any one would ship a 404 or a stale precache.
- **The toolchain works.** Node v26.12.1 is installed and there is no `package.json` anywhere in the repo, so `.js` resolves as CommonJS and the `typeof module !== "undefined"` / `module.exports` dual-mode pattern plus `node --test` both work as specified.
- **`api()` carrying the raw body is the right minimal change.** D-14 needs the 409 report without a second request; `api.rs` returns the fresh report as the 409 body, and `app.js:93` is the single chokepoint to thread it through.

#### Concerns

- **MEDIUM — Task 2's automated gate can never pass on this machine.** The gate ends:
  `grep -c 'removal.js' <3 files> | grep -v ':0$' | wc -l | grep -q '^3$'`
  BSD `wc -l` emits leading whitespace. I confirmed by `od -c`: the output is `' ' × 7, '3', '\n'`. The anchored `grep -q '^3$'` requires the line to be exactly `3`, so it fails **even when all three files are correctly modified**. The task is unpassable by its own criterion. This is also the precise failure mode CLAUDE.md warns about and `scripts/gate.sh:12` records having been burned by once ("masked a real `cargo fmt --check` failure").
- **MEDIUM — `node --test` is never added to the repo gate, so the PWA tests never run in CI.** `scripts/gate.sh` steps are `cargo fmt --check` (`:63`), `cargo clippy` (`:64`), `check-commit-message.sh` (`:65`), `assert-real-roots-untouched.sh` (`:71`), and `isolated cargo test -- --test-threads=1` (`:107`). There is no `node` step, and no plan in this phase adds one — 21-05's `files_modified` does not include `scripts/gate.sh`. The entire `removal.test.js` suite, which is the only automated coverage of the PWA view model, runs exactly once (in Task 1) and then never again. Any later regression is invisible.
- **MEDIUM — the XSS mitigation is claimed at a layer with no automated test.** T-21-09 says "Every daemon-supplied string passes through the existing `esc()` helper before interpolation into template strings; the test suite includes a path containing an angle bracket and asserts the view model keeps it as data." Those two clauses describe different layers. The `esc()` calls live in Task 2's `app.js` renderer, whose only automated verification is `node --check` (a syntax parse) plus a human check that says nothing about escaping. The angle-bracket test lives in Task 1 and correctly asserts the *view model* preserves the string as data — i.e. it verifies the layer that must **not** escape. Nothing automated verifies the layer that must.
- **LOW — `const BaudeRemoval` is not actually "on the global."** A top-level `const` in a classic script creates a binding in the global *lexical* environment, not a property of `window`. It is reachable by name from `app.js` given the stated load order, so the mechanism works, but "on the global" is wrong and would mislead anyone later trying `window.BaudeRemoval` or loading the file as a module.
- **LOW — `read_first` cites `app.js` lines 797-830 against an 828-line file.** Two lines over. The surrounding citations are accurate: `renderList` at `:596`, `openModal` at `:560`, `closeModal` at `:585`, `toast` at `:86`, `api` at `:93`, the 3 s poll at `:800-802`, `visibilitychange` at `:814`.
- **LOW — `removal.test.js` lives inside the embedded-asset directory.** `files_modified` places it at `bauded/web/removal.test.js`, beside files that `web.rs` embeds via `include_bytes!`. It will not be served (routes are explicit), so this is cosmetic, but a test file in the shipped-asset directory invites a future directory-serve change to expose it.
- **LOW — `grep -q 'baude-v7' bauded/web/sw.js` would pass on a half-done task.** The cache bump is the cheapest part of Task 2; asserting it proves nothing about the modal, the four row kinds, or the failure posture.

#### Suggestions

- Replace the brittle gate with per-file assertions that cannot be whitespace-sensitive:
  `grep -q 'removal.js' bauded/web/index.html && grep -q 'removal.js' bauded/web/sw.js && grep -q 'removal.js' bauded/src/web.rs`
  This is also clearer about which file failed.
- Add `scripts/gate.sh` to 21-05's `files_modified` and insert `step node --test bauded/web/removal.test.js` alongside the other steps, guarded for `node` being absent if the gate must stay portable. Without this the phase ships untested JS.
- Split T-21-09 into two mitigations: the view model preserves strings as data (Task 1, tested), and the renderer escapes them (Task 2, currently untested). For the second, the cheapest real coverage is to export the row/section HTML builder from `removal.js` as a pure string function and assert in `removal.test.js` that a `<script>`-bearing branch name comes back escaped. That moves the XSS guarantee into the one suite that actually runs.
- Reword to "defines `BaudeRemoval` as a top-level binding in the classic-script global scope", and consider `globalThis.BaudeRemoval = ...` if any consumer might load it as a module.
- Move `removal.test.js` out of `bauded/web/`.

#### Risk Assessment

**MEDIUM-HIGH.** The design is right and the TUI parity target is real, but this is the only plan whose automated gate is provably unpassable, and it introduces a test suite that the repository gate will never run. Those two together mean the PWA could ship with the modal silently broken and every gate still green. The large untested `app.js` surface in Task 2 — four row kinds, 409 re-render, retry, SSE interaction with an open modal — is covered only by a human check.

---

### 21-06

#### Summary

The most ambitious plan and the one carrying the phase's only compile-breaking scope error. Its architecture is right: `RemovalOrigin` on the existing `Modal::ConfirmRemove` reuses the entire Phase 20 modal, keymap, scroll and failure posture rather than forking it, and the `TcpListener` double with a strict per-route script is a better fixture than a tolerant mock. But Task 2 instructs a signature change to two helpers that are called from `baude/src/ui.rs`, a file not in `files_modified`; it breaks an existing test it never mentions; and its own verify filters would not catch that break.

#### Strengths

- **Every line citation in `read_first` is accurate.** `SidebarRefusal::RemoteRemove` at `app.rs:299-300`, the `ActionKind::Remote` refusal arm at `:344` (cited `298-346`); the notice text at `:1328` (cited `1326-1332`); `RemoveProjectOutcome` at `:511` (cited `498-520`); `removal_kind_label` at `:3789` and `removal_row_label` at `:3819` (cited `3789-3864`); `removal_target_for_selection` at `:3865` returning `None` for `SelId::Remote(_)` at `:3871` (cited `3865-3946`); the `ConfirmRemove` keymap arm at `:5795` (cited `5795-5870`); `d_on_remote_row_sets_footer_notice` at `:12413` (cited `12413-12445`). No drift from the WR commits.
- **`ureq::AgentBuilder` is the correct API for the pinned version.** `baude/Cargo.toml:21` pins `ureq = { version = "2", features = ["json"] }`, and `remote.rs:147`, `:199`, `:207` use the 2.x `.timeout(..)` call shape. `AgentBuilder::new().timeout_connect(..).build()` is right; on 3.x it would not be.
- **The `RemoteInfo` back-compat claim is verified.** `remote.rs:21` defines it, and the existing fixture at `app.rs:12415-12426` deserializes a payload with eleven fields and no `checkout_key`. Adding `#[serde(default)] pub checkout_key: Option<CheckoutKey>` keeps that test green — and here, unlike on `SessionInfo`, the attribute is genuinely load-bearing.
- **`POLL_SECS` and `REQUEST_TIMEOUT` exist as cited.** `remote.rs:16-17`. Reusing `REQUEST_TIMEOUT` for the two POSTs while giving the SSE connection connect-only timeout is the correct distinction — a read timeout would kill an idle event stream.
- **The strict double is the right call.** "The double must accept any request headers and body length and answer exactly as scripted so that a wrong path or method surfaces as a test failure, never as a tolerant match" — this is what makes `remote_execute_removal_maps_409_to_recheck` meaningful rather than vacuous.
- **Reusing the Phase 20 modal is the correct architecture.** `confirm_remove_project` (`app.rs:4004`) and the keymap arm (`:5795-5870`) already implement the recheck, scroll clamp and retry posture that four `WR-0*` commits were spent getting right. A parallel remote modal would have had to re-earn all four.
- **The explicit "do not touch" list is good discipline.** Naming `confirm_remove_project`, `collect_row_runtimes`, `restore_row_runtimes` as off-limits protects the landed local path.

#### Concerns

- **HIGH — "pass the origin to both helpers" breaks six call sites in a file the plan may not edit.** Task 2 instructs: "pass the origin to both helpers," meaning `removal_kind_label` and `removal_row_label`. Both are `pub(crate)` and called from `baude/src/ui.rs`:
  - `ui.rs:1782` — `app.removal_kind_label(*target)`
  - `ui.rs:1783` — `app.removal_row_label(key)` (inside the `row_name` closure)
  - `ui.rs:3889`, `ui.rs:3900`, `ui.rs:3935`, `ui.rs:3937` — four test call sites

  `21-06`'s `files_modified` is `baude/src/remote.rs` and `baude/src/app.rs` only. Adding a parameter is a hard compile break in `ui.rs`, and the executor must either violate the declared file scope or improvise a different design mid-task. This is the one finding in the phase that stops the build.
- **MEDIUM — an existing test is invalidated and never mentioned, and the task gate would not catch it.** `sidebar_action_d_refuses_remote_rows` at `app.rs:12391` asserts, for both `has_runtime` values and both `D`/`d`:
  `assert_eq!(sidebar_action(view, key), SidebarAction::Refuse(SidebarRefusal::RemoteRemove))`
  Task 2 changes `sidebar_action` to map `ActionKind::Remote` to `SidebarAction::RemoveProject`, so this test must fail. The plan's `behavior` lists only `d_on_remote_row_*` and `enter_on_remote_confirm_*` tests and says it rewrites `d_on_remote_row_sets_footer_notice` — this second test is never named. Worse, Task 2's verify filters are `app::tests::d_on_remote_row_`, `app::tests::enter_on_remote_confirm_`, and `app::tests::confirm_remove_`; `sidebar_action_d_refuses_remote_rows` matches **none** of them. Task 2 would report GREEN with a broken crate, and only Task 3's full `scripts/gate.sh` would surface it.
- **MEDIUM — `CheckoutKey` has no local/remote namespace, so remote reports can be mislabeled.** `CheckoutKey(u64)` is a bare newtype (`baude-core/src/repository.rs:21`). `removal_row_label` (`app.rs:3819-3832`) resolves a `RowKey::Checkout` by searching `self.hierarchy_rows()` — **local** rows — first. For a remote report whose checkout key numerically collides with a local checkout (entirely possible; both are per-workspace sequences), the confirmation modal would name the user's *local* row while the removal targets the daemon's. The plan says `removal_row_label` should resolve "through `remote_snap` sessions by `checkout_key`" for the remote case, which is correct, but it never names the collision hazard, and a "try local, fall back to remote" implementation — the natural reading of adding an origin parameter to an existing local-first lookup — would be wrong in exactly the dangerous direction. `removal_target_repository` (`app.rs:3876-3892`) has the same local-only search.
- **MEDIUM — the phase-terminal gate does not cover 21-05.** 21-06 is `wave: 5` with `depends_on: [21-02, 21-04]`; 21-05 is also `wave: 5`. Task 3 runs `scripts/gate.sh` and `scripts/gate.sh --linux` as the phase's final gate, but it does not depend on 21-05. If the two wave-5 plans execute in parallel worktrees, the terminal gate never sees 21-05's changes to `bauded/src/web.rs`, `app.js`, `sw.js`, `index.html`, `style.css` — including the `include_bytes!` addition, which is the one 21-05 change that can break `cargo build`.
- **LOW — `start_without_events` does not name the call site it exists for.** The plan adds it "so existing TUI tests that point at a refusing port do not spin a second reconnecting thread," but there is exactly one existing test call site — `app.rs:13454` — and the plan never says to migrate it. (Production is `app.rs:889`.) One line, but if missed, the stated purpose is unmet.
- **LOW — Task 3 runs `--linux` without acknowledging the recorded blocker.** `STATE.md` carries an open Phase 18 blocker: `scripts/gate.sh --linux` is unrunnable verbatim on this host because `CARGO_TARGET_DIR` sits on the Docker Desktop bind mount and hits a file-visibility race. Phase 19 and 20-04 both ran the verbatim lane green, so it may not recur — but a plan whose final gate is that command should say which outcome it expects and what the workaround is.
- **LOW — the events thread runs where `TestRedirect` does not apply.** `TestRedirect` is thread-local and `!Send` by construction (`testing.rs:85-87`, `:112-116`). A thread spawned by `RemotePoller::start` sees no redirects. For Task 1's tests this is harmless — the double speaks HTTP and touches no config paths — but it is worth recording, because it is the actual reason a spawned subscriber thread is hazardous in tests, and it will matter to Phase 22's fixture work.

#### Suggestions

- **Do not change the helper signatures.** Either add `baude/src/ui.rs` to `files_modified` and update all six call sites, or — better — have the helpers read the origin from `self.modal` (they already take `&self` and the modal already carries `target`), keeping both signatures and all six call sites untouched. The second option also removes the risk of a half-updated test call site.
- Add `sidebar_action_d_refuses_remote_rows` to Task 2's `behavior` as a test to rewrite (a remote row with no key should still refuse; a remote row *with* a key should now return `RemoveProject`), and add `app::tests::sidebar_action_` to the verify filter.
- State explicitly that remote `RowKey` resolution must consult `remote_snap` **only** when origin is `Remote`, never falling back to `hierarchy_rows()`, and add a test with a deliberately colliding key: one local checkout and one remote session both keyed 7, asserting the modal names the remote one.
- Change 21-06 to `depends_on: [21-02, 21-04, 21-05]`, or move Task 3 into its own terminal plan `21-07`. As written, the phase has no gate that covers all of its own changes.
- Name `app.rs:13454` as the `start_without_events` migration site.
- Record the expected `--linux` outcome and the `CARGO_TARGET_DIR` workaround in Task 3.

#### Risk Assessment

**HIGH.** The architecture is sound and the citations are the most accurate in the set, but this plan contains a certain compile break (`ui.rs` call sites), a certain test break that its own gate would hide (`sidebar_action_d_refuses_remote_rows`), a real correctness hazard the plan does not name (key collision across the local/remote namespace), and the phase's terminal gate is mis-sequenced. At 60k estimated tokens and `confidence: low`, it is also the largest plan, executing last, with the least slack to absorb a mid-task redesign.

---

### Cross-Plan Observations

#### Dependency ordering

The declared chain is `21-01 → 21-02 → 21-03 → 21-04 → (21-05 ‖ 21-06)` — four serial waves before any parallelism, for ~306k estimated tokens.

- **21-04 is over-serialized.** It is `wave: 4, depends_on: [21-03]`, but its actual need from upstream is only `remove_rows`' `Ok` arm, delivered by 21-01. Its Task 2 reuses 21-03's forced-failure seams, which is a convenience, not a requirement — `atomic_failure_for_test` (`manager.rs:140`) and `persistence_blocked` (`:130`) already exist, and the only new seam 21-03 adds is a `#[cfg(test)]` setter. 21-04 could run in wave 3 alongside 21-03, both depending on 21-02, shortening the chain by one wave.
- **Wave 5 is the real ordering defect**, covered under 21-06 above: two plans in one wave, one of which runs the phase-terminal gate without depending on the other.

#### Verification-command weaknesses (systemic)

Three distinct patterns recur, all verifiable:

1. **Unguarded chained `cargo test` filters** (21-02 Task 1, 21-03 Task 1, 21-06 Task 2). `cargo test` exits 0 on a zero-match filter. Only the first command in each chain carries the `running [1-9][0-9]*` count guard. The phase has already lost a round to this (`6397c72`).
2. **Whitespace-sensitive arithmetic in shell gates** (21-05 Task 2). Confirmed unpassable on macOS.
3. **A new test runner with no path into `scripts/gate.sh`** (21-05). The repo gate's steps are enumerable at `scripts/gate.sh:63-107` and contain no `node`.

The first command in each task's gate is consistently well-built — output redirected to a file, `STATUS` captured separately from the grep, no judging through a pipe (`scripts/gate.sh:12`'s lesson). The discipline simply stops being applied after the first `&&`.

#### Documentation consistency

`21-CONTEXT.md` has two stale facts the plans individually route around but never fix: D-02 (`:18`) and D-13 (`:35`) say "clearings" where the fields are `cleared`/`clearing_failures`, and `:53` names `take_row_runtimes` at `:3950` where the function is `collect_row_runtimes` at `app.rs:3948`. 21-01 and 21-05 both correct these inline. Since 21-03 and 21-06 also load CONTEXT, the corrections should land in CONTEXT itself.

Separately, `.planning/STATE.md` still reads `current_phase: 20`, `status: executing`, "Plan: 1 of 4" — but `f1f6d64`, `c44506a` and `d36cbb5` show Phase 20 complete, verified and human-validated, with four `WR-0*` fixes after. Phase 21's plans were written against the true state, not the stale file, so this did not cause harm here — but anything that reads STATE.md to decide what to do next will be wrong.

#### Security posture

Consistent and correctly scoped to the project's stated model. `api.rs:1-2` is explicit ("Security is the tailnet … no auth layer here"), and every new endpoint inherits it with the deferral recorded in `21-CONTEXT.md:84`. Two genuine strengths:

- **Path tampering is impossible by construction, not by validation.** Because `paths_deleted` is one of the three recheck sets, no execute body can both pass the recheck and carry a hostile path. The daemon derives every path from `repository_state` (`removal.rs:436-470`). This is stronger than the threat tables claim.
- **No new dependency is introduced anywhere in the phase.** `async-stream` (`bauded/Cargo.toml:12`), `ureq` 2.x (`baude/Cargo.toml:21`), `http-body-util` (`:37`), and `test-support` (`:35`) are all already present; `node:test`/`node:assert` are runtime built-ins. The only manifest change is enabling one feature of an already-vendored crate.

The one real gap is 21-05's XSS enforcement point having no automated coverage, detailed above.

#### Do the plans achieve the phase goals?

| Requirement | Verdict |
|---|---|
| **API-01** — removal over REST | **Yes.** 21-01 (checkout) + 21-02 (all kinds) + 21-03 (every failure status driven by a real cause). |
| **API-02** — live updates | **Yes.** 21-04 publishes; 21-05 and 21-06 consume. |
| **API-03** — PWA parity | **Qualified yes.** The design achieves it, and the view model is tested. But the layer that renders it has only `node --check` plus a human check, and even the view-model suite never runs in the gate. |
| **D-12** — TUI remote-row refusal replaced | **Blocked as written.** 21-06's `ui.rs` signature break and the unaccounted-for `sidebar_action_d_refuses_remote_rows` need resolving first. |

No scope creep. Every artifact traces to a CONTEXT decision, and `21-01`'s decision map (line 34) makes the mapping auditable. 21-05's `removal.js` extraction is the only piece that could be read as added structure, and it is justified: it is the only way to get automated coverage of PWA logic in a repo with no JS harness, and it mirrors a tested TUI function (`ui.rs:2509`).

---

### Overall Risk Assessment

**MEDIUM-HIGH**, concentrated almost entirely in the last wave.

Plans 21-01 through 21-04 are in good shape — **LOW / MEDIUM / LOW / LOW**. Their wire contracts, test seams and dependencies are all verified present, their line citations survived the WR-commit drift I expected to find, and their security reasoning is sound. 21-02's only substantive gap is the untested main-checkout suppression path.

The risk is 21-05 (**MEDIUM-HIGH**) and 21-06 (**HIGH**), which together hold:

1. A certain compile break — 21-06's helper signature change against six `ui.rs:1782`-`:3937` call sites, in a file outside `files_modified`.
2. A certain test break that the task's own gate would hide — `sidebar_action_d_refuses_remote_rows` (`app.rs:12391`).
3. A gate that cannot pass even when the work is correct — 21-05 Task 2's `wc -l | grep -q '^3$'`.
4. A new test suite with no path into CI — `node --test` absent from `scripts/gate.sh:63-107`.
5. A terminal gate that does not cover its own phase — 21-06 Task 3 not depending on 21-05.
6. An unnamed correctness hazard — `CheckoutKey(u64)` collisions across the local/remote namespace.

All six are mechanical to fix at plan level, and all six get considerably more expensive once wave 5 is executing. Items 1, 2 and 5 should be resolved before execution starts; items 3 and 4 are one-line gate edits; item 6 needs one sentence of design constraint plus one test.

**Recommended pre-execution changes, in priority order:**

1. Resolve 21-06's helper-signature problem — prefer reading origin from `self.modal` over adding a parameter.
2. Add `sidebar_action_d_refuses_remote_rows` to 21-06 Task 2's behavior and its verify filter.
3. Make 21-06 `depends_on: [21-02, 21-04, 21-05]`, or split Task 3 into a terminal `21-07`.
4. Fix 21-05 Task 2's gate to three independent `grep -q` calls.
5. Add `scripts/gate.sh` to 21-05's `files_modified` and wire in `node --test`.
6. Guard the chained `cargo test` filters in 21-02, 21-03 and 21-06.
7. Add the remote/local key-collision constraint and test to 21-06.
8. Add the main-checkout suppression assertion to 21-02.
9. Correct `21-CONTEXT.md:18`, `:35`, `:53`, and refresh `.planning/STATE.md`.

With 1–5 applied, I would put the phase at **LOW-MEDIUM** overall.

## Codex Review

Static review completed against repository sources for all six plans. I did not modify files or run implementation tests.

### 21-01

**Summary:** The daemon wrapper follows the existing core boundary well. The main weakness is that the proposed fixture does not reliably exercise live-runtime removal, and the custom save closure omits some existing persistence bookkeeping.

**Strengths**

- Calling core preserves the established kill → disk → save order and runtime custody contract. Core consumes handles only after the replacement commits and retains them on errors. Evidence: `baude-core/src/removal.rs:400`, `baude-core/src/removal.rs:436`, `baude-core/src/removal.rs:1039`.
- The three-field recheck matches the existing TUI mechanism, and the proposed HTTP statuses align with daemon persistence and teardown errors. Evidence: `baude/src/app.rs:4022`, `bauded/src/api.rs:110`.

**Concerns**

- **HIGH — The “live” fixture can pass with an exited process.** `Manager` passes its command through permission resolution, which appends Claude flags to `sleep 30`. Plain `sleep` rejects those arguments. Checking a nonzero `agent_pid` does not establish liveness: preview copies the recorded PID without probing whether it is alive. Evidence: `bauded/src/manager.rs:1231`, `baude-core/src/permission.rs:98`, `baude-core/src/removal.rs:1147`. The repository already documents this exact fixture problem and uses `sh -c "sleep 60"` to absorb appended arguments in its smoke test: `.github/workflows/ci.yml:78`.
- **MEDIUM — Persistence status can become misleading.** The proposed raw save closure bypasses `save_checked`, but its failure arm only restores handles, and its success arm only updates `persistence_dirty`. Existing saves also set or clear `persistence_error`, exposed through `persistence_status`. Evidence: `bauded/src/manager.rs:729`, `bauded/src/manager.rs:752`.

**Suggestions**

- Use an argument-tolerant command double. Assert the process is alive before preview and extinct after execution; keep the existing restore assertions.
- Explicitly maintain `persistence_dirty` and `persistence_error` for failed, successful, and degraded saves, with observable status assertions.

**Risk Assessment: MEDIUM.** The implementation boundary is sound, but the current tests could falsely validate its most important promise.

### 21-02

**Summary:** The hierarchy endpoint and shared suppression helper are appropriate additions. This plan contains a direct compatibility-test contradiction and does not yet establish real standalone runtime parity.

**Strengths**

- Keeping hierarchy in `/rows` preserves a flat `/sessions` representation. The existing daemon currently builds session lists directly from runtime sessions, so a separate projection fits the architecture. Evidence: `bauded/src/manager.rs:2393`.
- Reusing suppression correctly preserves its exact-path semantics and core-owned write rules. Evidence: `baude-core/src/suppression.rs:73`, `baude-core/src/removal.rs:755`.

**Concerns**

- **HIGH — The required unchanged compatibility test must fail.** It explicitly forbids `checkout_key` on `/sessions` entries. Adding that field and requiring this test to pass unchanged are mutually exclusive. Evidence: [bauded/src/api.rs:1002](/Users/joese/Code/github.com/poindexter12/baude/bauded/src/api.rs:1002).
- **HIGH — Inactive seeded standalone coverage conceals missing production support.** Plain daemon creation always calls `record_checkout_intent`; even a non-git directory becomes a repository/check-out pair. `Manager` has only `runtime_checkouts`, while core requires a runtime handle for a standalone with recorded ownership. The planned collector expressly supplies none for standalone rows. Evidence: `bauded/src/manager.rs:879`, `bauded/src/manager.rs:1073`, `bauded/src/manager.rs:128`, `baude-core/src/removal.rs:339`, `baude-core/src/removal.rs:862`. A test-only inactive row proves endpoint dispatch, not live standalone parity.
- **MEDIUM — Main-checkout classification needs additional work.** Plain admission currently records `CheckoutRole::ManagedBranch` unconditionally. A projection based on the persisted role cannot produce the promised main-checkout classification correctly without another rule or an admission fix. Evidence: `bauded/src/manager.rs:1110`; available roles are defined at `baude-core/src/repository.rs:284`.

**Suggestions**

- Amend the compatibility test narrowly to permit the newly authorized scalar key while retaining its prohibitions on hierarchy.
- Add actual standalone admission, runtime mapping, custody, and removal coverage if live standalone parity remains in scope.
- Specify how verified main paths and persisted roles map to `/rows` kinds, including existing daemon state.

**Risk Assessment: HIGH.** The plan cannot pass its stated verification unchanged, and one required row kind is represented only through an artificial fixture.

### 21-03

**Summary:** Drift and request-tampering tests are valuable. Failure coverage needs to reflect the core operation’s irreversible stages rather than imply complete rollback.

**Strengths**

- Stale-preview resend tests exercise the same confirmation boundary already used locally. Evidence: `baude/src/app.rs:4022`.
- Tampered-path coverage reinforces a strong existing boundary: core selects paths from saved state and verifies runtime identities before signaling. Evidence: `baude-core/src/removal.rs:306`, `baude-core/src/removal.rs:840`.

**Concerns**

- **HIGH — “Nothing half-removed” overstates the actual guarantee.** Core kills runtimes and deletes worktrees before saving. A persistence failure retains durable rows and handles, but those handles may already be stopped and directories may already be gone. A parent operation may also stop earlier children before a later child fails. Evidence: [baude-core/src/removal.rs:424](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/removal.rs:424), `baude-core/src/removal.rs:454`, `baude-core/src/removal.rs:921`. The proposed single-checkout tests do not cover these partial effects.
- **MEDIUM — Important outcome branches lack HTTP coverage.** The plan covers pre-replacement persistence failure but omits disk failure and committed-but-degraded persistence. Core deliberately returns successful `SavedDegraded` after a committed replacement; treating that as an ordinary 503 would misreport a completed removal. Evidence: `baude-core/src/removal.rs:964`, `baude-core/src/removal.rs:1039`.
- **LOW — Existing injection support is overlooked.** `Manager::fail_next_agent_teardown_for_test` already exposes the required PTY failure seam, so another accessor should be unnecessary. Evidence: `bauded/src/manager.rs:2525`.

**Suggestions**

- Add parent fixtures that fail after an earlier runtime stops or worktree disappears. Assert retained rows, actual process/disk outcomes, and successful recovery through fresh preview and retry.
- Add disk-failure and `SavedDegraded` HTTP tests.
- Replace rollback language with the precise guarantee: rows and handle custody remain on error; completed process and disk effects persist.

**Risk Assessment: HIGH.** These are destructive operations, and the current tests leave their most consequential failure states unverified.

### 21-04

**Summary:** The broadcast/SSE design fits the daemon. Publishing under the lock establishes server-state ordering, but it does not establish the claimed client-side protection against resurrection.

**Strengths**

- Publishing after core returns success follows the point where core replaces in-memory state with the saved candidate. Evidence: `baude-core/src/removal.rs:1039`.
- Reusing the existing SSE implementation style avoids unnecessary transport infrastructure. Evidence: `bauded/src/api.rs:614`, `bauded/src/api.rs:642`.

**Concerns**

- **HIGH — An older poll can resurrect rows after an event.** The TUI fetches `/sessions`, then `/info`, then replaces its entire snapshot. An event can remove a row while the earlier poll is waiting for `/info`; that poll subsequently restores its stale session list. The browser likewise assigns completed fetch results directly. A server mutex cannot order these client-side applications. Evidence: [baude/src/remote.rs:147](/Users/joese/Code/github.com/poindexter12/baude/baude/src/remote.rs:147), `baude/src/remote.rs:176`, `bauded/web/app.js:137`. Reading a new `/sessions` response after receiving the event does not test this race.
- **MEDIUM — The lag test can validate Tokio without validating the handler.** Receiving `Lagged` and then `Ok` directly does not prove the HTTP stream continues. Existing handlers execute their stream logic when the response body is polled, which provides a straightforward way to test actual overflow. Evidence: `bauded/src/api.rs:614`.

**Suggestions**

- Require both clients to arbitrate polls against removal events—for example, discard snapshots whose request began before a removal generation changed.
- Add deterministic delayed-poll → removal-event → delayed-poll-completion tests.
- Subscribe through `/events`, leave its body unpolled, publish beyond capacity, then verify the route still yields a valid event.

**Risk Assessment: HIGH.** The server implementation is modest, but its stated no-resurrection guarantee needs explicit client coordination.

### 21-05

**Summary:** The pure removal module and report-backed modal are useful foundations. Most remaining risk lies in asynchronous UI wiring, which the proposed tests barely exercise.

**Strengths**

- Rendering the actual `cleared`, `clearing_failures`, and `already_removed` fields matches the core contract and existing TUI presentation. Evidence: `baude-core/src/removal.rs:180`, `baude/src/ui.rs:2570`.
- Registering the embedded asset and service-worker shell entry covers both delivery paths used by installed clients. Evidence: `bauded/src/web.rs:11`, `bauded/web/sw.js:5`.

**Concerns**

- **HIGH — Poll/event arbitration is missing.** Fetching `/sessions` and `/rows` together does not make them an atomic snapshot, and an older refresh can overwrite a successful removal or SSE drop. Existing refresh directly replaces state, while timers and navigation can initiate additional refreshes. Evidence: `bauded/web/app.js:130`, `bauded/web/app.js:137`, `bauded/web/app.js:800`.
- **MEDIUM — A fresh 409 report cannot supply new child names by itself.** `RemovalReport` contains row keys, paths, and PIDs, but no row names or branches. A child added after the initial preview may be absent from cached `/rows`, so immediate re-rendering cannot meet the “every child by name and branch” requirement. Evidence: `baude-core/src/removal.rs:134`, `baude-core/src/removal.rs:180`.
- **MEDIUM — Removing the open chat needs navigation and stream cleanup.** Filtering sessions and calling `render()` leaves `state.sid` pointing to the removed session. `renderChat()` then renders a fallback identity, while stream cleanup currently occurs on route changes. Evidence: `bauded/web/app.js:113`, `bauded/web/app.js:658`.
- **MEDIUM — Verification misses the mutation flow and contradicts repository policy.** Pure-model tests and syntax checks cannot prove cancellation, retries, escaping at DOM insertion, or SSE wiring. The proposed source greps are explicitly prohibited by `CLAUDE.md:24`. The current gate also runs no Node tests: `scripts/gate.sh:63`, `scripts/gate.sh:107`. Keeping an angle bracket unchanged in a view model does not test the escaping sink at `bauded/web/app.js:33`.

**Suggestions**

- Add focused behavioral tests with controlled fetch and EventSource timing, including stale polls, cancellation, 409, 500/503, and removal of the open chat.
- Refresh row metadata before enabling confirmation after a 409 when labels are missing; retain the fresh report as the authority for effects.
- Add the Node suite to repeatable validation and replace source-presence checks with asset-serving or UI behavior assertions.

**Risk Assessment: HIGH.** The isolated model can pass while the destructive user flow remains incorrect.

### 21-06

**Summary:** Reusing the shared modal is appropriate, and snapshot changes already reach the draw loop. The file inventory, request timing, and test harness need correction before this is executable.

**Strengths**

- Existing app ticks already compare remote snapshots, mark the UI dirty, and detach when a session disappears. Event-driven snapshot reduction can reuse that path. Evidence: `baude/src/app.rs:4888`, `baude/src/app.rs:4926`.
- The existing modal already renders report sections and retry errors, making origin-based dispatch a reasonable extension. Evidence: `baude/src/ui.rs:2509`, `baude/src/app.rs:5795`.

**Concerns**

- **HIGH — `ui.rs` is missing from the modification scope.** Its modal match exhaustively names the current fields, calls both label helpers with their current signatures, and constructs the modal in tests. Adding `origin` and changing helper signatures requires updating this file. Evidence: [baude/src/ui.rs:1776](/Users/joese/Code/github.com/poindexter12/baude/baude/src/ui.rs:1776), `baude/src/ui.rs:3818`. The proposed two-file implementation will not compile as specified.
- **HIGH — The three-second execute timeout is shorter than legitimate removal work.** Core stops rows sequentially; each session may stop both agent and shell, and each PTY reader join alone can wait two seconds. Git removal and persistence follow. A valid removal can commit after the TUI reports a timeout. Evidence: `baude/src/remote.rs:17`, `baude-core/src/removal.rs:921`, `baude-core/src/session.rs:495`, `baude-core/src/pty.rs:901`. Synchronous execution also freezes input and redraw during the request.
- **MEDIUM — The strict HTTP double races the background poller.** Disabling events still leaves `start` immediately requesting `/sessions` and `/info`, potentially consuming a script intended for a preview POST. Evidence: `baude/src/remote.rs:145`. The existing poller also owns an unconditional thread loop without a shutdown mechanism, so “shares the poller’s lifetime” does not establish cancellation for a new indefinitely blocking SSE reader. Evidence: `baude/src/remote.rs:133`, `baude/src/remote.rs:189`.

**Suggestions**

- Add `baude/src/ui.rs`, its constructors, and renderer call sites to the plan.
- Dispatch removal outside the input loop and define reconciliation for ambiguous transport failures, including event-before-response and delayed-success cases.
- Provide a seeded test constructor with no background networking. Give production workers explicit cancellation and bounded stream-health handling.
- Apply 21-04’s stale-poll protection to the remote snapshot writer.

**Risk Assessment: HIGH.** There is a concrete compilation omission and an execution timeout that can misreport successful destructive work.

---

**Overall assessment: request revisions before execution.** The shared-core architecture is a strong foundation, but the plans need five corrections:

1. Resolve the compatibility-test contradiction.
2. Cover real live processes and standalone runtimes.
3. Specify partial-failure recovery accurately.
4. Prevent stale polls from undoing event-driven row drops.
5. Complete renderer scope and behavioral client verification.

The final validation should run **after both wave-5 plans finish**. A gate inside 21-06 can otherwise pass before 21-05 lands, and today’s Rust gate does not include the new JavaScript tests.

## Consensus Summary

### Agreed Concerns (raised by BOTH reviewers)

1. **HIGH — 21-06 changes the two label helpers' signatures but omits `baude/src/ui.rs` from `files_modified`.** A guaranteed compile break: the modal match and the test constructors in `ui.rs` call `removal_kind_label` / `removal_row_label` with their current signatures. Claude names six call sites (`ui.rs:1782`, `:1783`, `:3889`, `:3900`, `:3935`, `:3937`); codex cites `ui.rs:1776` and `ui.rs:3818`. Corroborated, counted.
2. **MEDIUM — 21-05's verification cannot prove the `app.js` mutation flow.** `node --check` is a syntax parse; the source-presence greps are prohibited by `CLAUDE.md:24`; and no plan wires `node --test` into `scripts/gate.sh` (`:63`–`:107` contain no `node` step), so the only automated PWA coverage runs once in Task 1 and never again.
3. **MEDIUM — 21-04's lag test exercises Tokio, not the handler.** Driving `Manager::subscribe_events` directly to observe `Lagged` then `Ok` proves broadcast semantics; it never reads a frame through the `/events` route (`bauded/src/api.rs:614`), which is the discretionary behaviour most likely to be wrong.
4. **MEDIUM — the phase-terminal gate does not cover the phase.** 21-06 Task 3 runs `scripts/gate.sh` as the final gate, but 21-06 is `depends_on: [21-02, 21-04]` and 21-05 is the other `wave: 5` plan. Claude flags the ordering defect directly; codex closes its overall assessment with the same requirement ("the final validation should run after both wave-5 plans finish").

### Claude-unique concerns

- MEDIUM — 21-02's main-checkout suppression path is specified by core but untested: `update_suppression` (`baude-core/src/removal.rs:776-785`) does not branch on `CheckoutRole`, so removing a main checkout whose repository survives *will* suppress the repository root, and nothing in the phase asserts it.
- MEDIUM — unguarded chained `cargo test` filters in 21-02 Task 1, 21-03 Task 1 and 21-06 Task 2 pass on zero matches; only the first command in each chain carries the `running [1-9][0-9]*` count guard. The phase already lost a round to this (`6397c72`).
- MEDIUM — 21-04's `must_haves.truths` claims D-07's "inside the same lock" property, but no test in the plan would go red if a refactor moved `broadcast::send` outside the lock.
- MEDIUM — 21-05 Task 2's gate `grep -c 'removal.js' <3 files> | grep -v ':0$' | wc -l | grep -q '^3$'` is unpassable on macOS: BSD `wc -l` emits leading whitespace, so the anchored match fails even when all three files are correct.
- MEDIUM — 21-05's T-21-09 XSS mitigation is claimed at the renderer (`esc()` in `app.js`) but tested at the view model, which must *not* escape. The enforcing layer has no automated coverage.
- MEDIUM — 21-06 invalidates `sidebar_action_d_refuses_remote_rows` (`baude/src/app.rs:12391`) without naming it, and Task 2's verify filters match none of it, so Task 2 would report GREEN with a broken crate.
- MEDIUM — `CheckoutKey(u64)` (`baude-core/src/repository.rs:21`) has no local/remote namespace, and `removal_row_label` searches local `hierarchy_rows()` first. A remote report whose key collides with a local checkout would name the user's local row in the confirmation modal. The plan never names the hazard.
- LOW cluster — `Manager::new("sleep 30", true)` needs `.into()`; the 404 execute body must hand-build all eleven `RemovalReport` fields; `#[serde(default)]` on `SessionInfo.checkout_key` is inert (`SessionInfo` derives `Serialize, Clone` only); the 21-03 read_first points at the wrong file for `AtomicFailure` and at a call rather than the definition for the teardown seam; 21-04's `sync` rationale is wrong (already transitively enabled via `api.rs:562`, `manager.rs:12`); `/events` is an unauthenticated daemon-wide subscribe with no precondition, unlike the per-session streams; `const BaudeRemoval` is a global lexical binding, not a `window` property; `removal.test.js` sits inside the embedded-asset directory; `start_without_events`'s one migration site (`app.rs:13454`) is never named; Task 3's `--linux` lane has a recorded STATE.md blocker it does not acknowledge; `TestRedirect` is thread-local and `!Send`, so poller-spawned threads see no redirects; `21-CONTEXT.md:18`, `:35`, `:53` carry stale names the plans route around but never fix.

### Codex-unique concerns

- HIGH — 21-01's "live" fixture can pass with an exited process. `Manager` passes its command through permission resolution, which appends Claude flags that plain `sleep 30` rejects (`bauded/src/manager.rs:1231`, `baude-core/src/permission.rs:98`), and a nonzero `agent_pid` does not establish liveness (`baude-core/src/removal.rs:1147`). The repo already uses `sh -c "sleep 60"` for exactly this reason (`.github/workflows/ci.yml:78`).
- HIGH — 21-02's required-unchanged compatibility test must fail. `flat_session_api_remains_a_non_hierarchical_compatibility_projection` forbids `checkout_key` on `/sessions` entries (`bauded/src/api.rs:1002`), while Task 1 adds that field and requires the test to pass unchanged (`21-02-PLAN.md:63`). Independently confirmed in this review's source-grounding pass.
- HIGH — 21-02's standalone coverage is inactive-only and conceals missing production support. Plain daemon creation always calls `record_checkout_intent`, so even a non-git directory becomes a repository/checkout pair (`bauded/src/manager.rs:879`, `:1073`); `Manager` holds only `runtime_checkouts` (`:128`) while core requires a runtime handle for a standalone with recorded ownership (`baude-core/src/removal.rs:339`, `:862`).
- HIGH — 21-03's "nothing half-removed" overstates the guarantee. Core kills runtimes and deletes worktrees before saving (`baude-core/src/removal.rs:424`, `:454`, `:921`), so a persistence failure retains rows and handles whose processes are already stopped and whose directories are already gone. A parent operation can also stop earlier children before a later one fails. The single-checkout tests cover none of this.
- HIGH — 21-04's no-resurrection guarantee needs client-side arbitration. The TUI fetches `/sessions`, then `/info`, then replaces its whole snapshot (`baude/src/remote.rs:147`, `:176`); the browser assigns completed fetch results directly (`bauded/web/app.js:137`). An event can drop a row while an earlier poll is still in flight, and that poll then restores it. A server mutex cannot order client-side application.
- HIGH — 21-05 has the same defect on the PWA side: `/sessions` + `/rows` fetched together are not an atomic snapshot, refresh directly replaces state, and timers and navigation can start further refreshes (`bauded/web/app.js:130`, `:137`, `:800`).
- HIGH — 21-06's three-second `REQUEST_TIMEOUT` (`baude/src/remote.rs:17`) is shorter than legitimate removal work. Core stops rows sequentially, each PTY reader join alone can wait two seconds (`baude-core/src/session.rs:495`, `baude-core/src/pty.rs:901`), then git removal and persistence follow — so a valid removal can commit after the TUI reports a timeout. Synchronous execution also freezes input and redraw.
- MEDIUM — 21-01's raw save closure bypasses `save_checked`: its failure arm only restores handles and its success arm only sets `persistence_dirty`, while existing saves also set or clear `persistence_error` exposed through `persistence_status` (`bauded/src/manager.rs:729`, `:752`).
- MEDIUM — 21-02's main-checkout classification needs a rule: plain admission records `CheckoutRole::ManagedBranch` unconditionally (`bauded/src/manager.rs:1110`), so a projection over the persisted role cannot produce the promised main-checkout kind.
- MEDIUM — 21-03 omits HTTP coverage for disk failure and for committed-but-degraded persistence; core deliberately returns a successful `SavedDegraded` after a committed replacement (`baude-core/src/removal.rs:964`, `:1039`), and treating that as an ordinary 503 would misreport a completed removal.
- MEDIUM — a fresh 409 `RemovalReport` cannot supply new child names: it carries keys, paths and PIDs but no names or branches (`baude-core/src/removal.rs:134`, `:180`), so "every child by name and branch" is unmeetable from the report alone when a child was added after the first preview.
- MEDIUM — removing the open chat in the PWA leaves `state.sid` pointing at the removed session; `renderChat()` then renders a fallback identity, and stream cleanup only happens on route changes (`bauded/web/app.js:113`, `:658`).
- MEDIUM — 21-06's strict HTTP double races the background poller: disabling events still leaves `start` immediately requesting `/sessions` and `/info` (`baude/src/remote.rs:145`), which can consume a script intended for the preview POST. The poller also owns an unconditional thread loop with no shutdown (`:133`, `:189`), so a new indefinitely blocking SSE reader has no cancellation.
- LOW — 21-03's extra teardown accessor is unnecessary; `Manager::fail_next_agent_teardown_for_test` already exposes the seam (`bauded/src/manager.rs:2525`).

### Where the reviewers diverge on risk

Claude rates 21-01 through 21-04 LOW/MEDIUM/LOW/LOW and concentrates its risk in wave 5 (overall MEDIUM-HIGH). Codex rates 21-02 through 21-06 all HIGH and 21-01 MEDIUM, driven by its seven test-realism and client-arbitration HIGHs. The divergence is substantive, not stylistic: codex attacks whether the tests can actually witness the behaviour they claim, while claude attacks whether the plans compile and whether their gates can pass. Both lines of attack land, and they barely overlap — which is the strongest argument in this cycle for treating the gate's single corroborated HIGH as a floor rather than a ceiling.

## Source-Grounding Pass

Every symbol the plans cite against *existing* code was checked with ripgrep. Symbols the plans declare as artifacts this phase produces are excluded by design and listed separately. Effective authority: `intel` (`drift-guard authority --raw`). Under that authority `VERIFIED` → `none`, `MISSING` → `needs-acknowledgement`, `AMBIGUOUS` → `MEDIUM`, `UNCHECKABLE` → `INFO`; no verdict is a hard block. UNCHECKABLE was never treated as verified.

| Symbol | Cited in | Verdict | Evidence |
|---|---|---|---|
| `removal::RemovalTarget` | 21-01/02/05/06 | VERIFIED | `baude-core/src/removal.rs:49` `pub enum RemovalTarget {` |
| `RowKey` | all | VERIFIED | `baude-core/src/removal.rs:58` |
| `RemovalReport` + all eleven fields | 21-01, 21-05 | VERIFIED | `baude-core/src/removal.rs:180-202`, in exactly the cited order |
| "no `clearings` field" | 21-01, 21-05 | VERIFIED | `baude-core/src/removal.rs:180-202` has none |
| "no `ClearingResult` type" | 21-05 | AMBIGUOUS | singular absent, but private `struct ClearingResults` exists at `baude-core/src/removal.rs:482` |
| `ClearedItem` | 21-01, 21-05 | VERIFIED | `baude-core/src/removal.rs:163` |
| `ClearingFailure` | 21-01, 21-05 | VERIFIED | `baude-core/src/removal.rs:172` |
| `ClearingKind` | 21-05 | VERIFIED | `baude-core/src/removal.rs:152` |
| `KilledRuntime` (`row`, `agent_pid`, `shell_pid`) | 21-01, 21-05 | VERIFIED | `baude-core/src/removal.rs:134-138` |
| `PersistenceOutcome` incl. `SavedDegraded` | 21-01, 21-05 | VERIFIED | `baude-core/src/removal.rs:143-149` |
| `RemovalError` — all seven mapped arms | 21-01 | VERIFIED | `baude-core/src/removal.rs:235-256` |
| `preview_removal(state, target)` | 21-01, 21-02 | VERIFIED | `baude-core/src/removal.rs:359` |
| `RowRuntime` (`row`, `session`) | 21-01 | VERIFIED | `baude-core/src/removal.rs:371` |
| `remove(state, target, runtimes, save)` | 21-01 | VERIFIED | `baude-core/src/removal.rs:436`, four params in order |
| stage order "kill, disk, save, suppression" | 21-02 | AMBIGUOUS | `baude-core/src/removal.rs:452-456` — fourth stage is `clear_stage`, of which suppression is one kind |
| `update_suppression` | 21-02 | VERIFIED | `baude-core/src/removal.rs:755`, calls `suppression::suppress` at `:795` |
| `fail_next_teardown_for_test` | 21-03 | VERIFIED | defined `baude-core/src/pty.rs:914`; used `baude-core/src/removal.rs:2264` |
| `persist::SaveError` | 21-01 | VERIFIED | `baude-core/src/persist.rs:254` |
| `SaveError::replacement_committed` | 21-01 | VERIFIED | `baude-core/src/persist.rs:278` |
| `load_for_workspace_strict_at` | 21-01 | VERIFIED | `baude-core/src/persist.rs:352`, exact line |
| `persist::save_current_at_test` | 21-01 | VERIFIED | `baude-core/src/persist.rs:473` |
| `persist::save_for_workspace_status` | 21-01 | VERIFIED | `baude-core/src/persist.rs:1137`; used `bauded/src/manager.rs:724` |
| `persist::config_dir()` | 21-02 | VERIFIED | `baude-core/src/persist.rs:895` → `config_base()` `:882`, honours `testing::config_dir_override()` |
| `persist::StateFile` | 21-01 | VERIFIED | `baude-core/src/persist.rs:21` |
| `AtomicFailure` as `pub struct` in `testing.rs` | 21-03 read_first | AMBIGUOUS | it is `pub enum AtomicFailure` at `baude-core/src/persist.rs:242`; the instructed grep returns nothing |
| `RepositoryReservations::reserve` | 21-01 | VERIFIED | `baude-core/src/lifecycle.rs:2839`, exact line |
| `Err(LifecycleOutcome::Busy { repository })` | 21-01 | VERIFIED | `baude-core/src/lifecycle.rs:2845` |
| `RepositoryReservations` / `RepositoryReservation` | 21-01 | VERIFIED | `baude-core/src/lifecycle.rs:2828`, `:2878` |
| `RepositoryKey` / `CheckoutKey` / `StandaloneKey` | 21-01, 21-02 | VERIFIED | `baude-core/src/removal.rs:50-61` |
| `SavedRepository` cited fields | 21-02 | VERIFIED | `baude-core/src/repository.rs:336-345` |
| `SavedCheckout` cited fields | 21-02 | VERIFIED | `baude-core/src/repository.rs:348-367`; `session.name` at `:293` |
| `SavedStandaloneSession` cited fields | 21-02 | VERIFIED | `baude-core/src/repository.rs:453-459` |
| `SavedStandaloneSession::new` + `StandaloneLifecycle::Inactive` shape | 21-02 | VERIFIED | `baude-core/src/repository.rs:1229-1245` |
| `StandaloneLifecycle` | 21-02 | VERIFIED | `baude-core/src/repository.rs:211` |
| `RetainedStandaloneSessionState` | 21-02 | VERIFIED | `baude-core/src/repository.rs:311` |
| `first_seen_order` | 21-02 | VERIFIED | `baude-core/src/repository.rs:340`, `:355`, `:456` |
| `role` projected as "main" / "worktree" | 21-02 | AMBIGUOUS | `baude-core/src/repository.rs:284-288` `CheckoutRole { Main, PrimaryDefault, ManagedBranch }` — three variants; `PrimaryDefault` (used by core's own fixture at `:1240`) has no mapping |
| `RepositoryState.{repositories,checkouts,standalone_sessions}` | 21-02 | VERIFIED | `baude-core/src/repository.rs:510-517` |
| `suppression::is_suppressed` | 21-02 | VERIFIED | `baude-core/src/suppression.rs:76` |
| `suppression::lift` | 21-02 | VERIFIED | `baude-core/src/suppression.rs:84` |
| `suppression::suppress(root, path, common_dir, now_ms)` | 21-02 | VERIFIED | `baude-core/src/suppression.rs:98`, four params |
| exact `folder_key` match semantics | 21-02 | VERIFIED | `baude-core/src/suppression.rs:73-77` |
| `git::repo_root` | 21-02 | VERIFIED | `baude-core/src/git.rs:1758` |
| `git::discover_repository` | 21-02 | VERIFIED | `baude-core/src/git.rs:323` |
| its callers at manager.rs 921/1073/1347/1418 | 21-02 | VERIFIED | all four exact; a fifth at `bauded/src/manager.rs:156` |
| `reconcile_legacy_session` at :155, callers 503 and 519 | 21-02 | AMBIGUOUS | defined `:155`, callers `:503` and `:517` — the second line is 517, not 519 |
| `testing::arm_unkillable_runtime` | 21-03 | VERIFIED | `baude-core/src/testing.rs:532` |
| `testing::holder_pgid` | 21-03 | VERIFIED | `baude-core/src/testing.rs:445` |
| `testing::parked_reader_command` | 21-03 | VERIFIED | `baude-core/src/testing.rs:464` |
| `workspace::active()` | 21-01 | VERIFIED | `baude-core/src/workspace.rs:441` |
| `workspace.state_file(STATE_BASE)` | 21-01 | VERIFIED | `baude-core/src/workspace.rs:87`; used `bauded/src/manager.rs:2521` |
| `Manager` fields cited at 106-147 | 21-01, 21-04 | VERIFIED | `bauded/src/manager.rs:111,128,129,130,132,138,140` |
| `Manager.repository_state` | 21-01, 21-02 | VERIFIED | `bauded/src/manager.rs:127` |
| `Manager.permission_notify` | 21-01 | VERIFIED | `bauded/src/manager.rs:126` |
| `STATE_BASE`, `save_checked` | 21-01 | VERIFIED | `bauded/src/manager.rs:36`, `:710` |
| `state_for_save` | 21-01 | VERIFIED | `bauded/src/manager.rs:779` |
| `retained_runtime_snapshot` / `teardown_retained_runtime` / `forget_stopped_runtime` | 21-01 | VERIFIED | `bauded/src/manager.rs:1439`, `:1466`, `:1489` |
| `Manager::remove` | 21-01 | VERIFIED | `bauded/src/manager.rs:1779` |
| `persist_at_for_test` | 21-01, 21-03 | VERIFIED | `bauded/src/manager.rs:2514` |
| `track_runtime_for_test` cited at 2514-2527 | 21-01 read_first | AMBIGUOUS | exists, but at `bauded/src/manager.rs:2268` |
| `restore` / `restore_at` / `restore_loaded` | 21-01 | VERIFIED | `bauded/src/manager.rs:496`, `:511`, `:526` |
| "`restore_at` is `#[cfg(test)]` and private" | 21-01 | VERIFIED | `bauded/src/manager.rs:510-511` |
| `SessionInfo` (no `checkout_key` today) | 21-02 | VERIFIED | `bauded/src/manager.rs:182-229` |
| `Manager::list` / `Manager::info` | 21-02 | VERIFIED | `bauded/src/manager.rs:2393`, `:2397` |
| `session_info` builder | 21-02 | VERIFIED | `bauded/src/manager.rs:2616` |
| `Manager::create` | 21-02 | VERIFIED | `bauded/src/manager.rs:813` |
| blocked-persistence message reused from `create` | 21-01 | VERIFIED | `bauded/src/manager.rs:819-822` |
| `record_checkout_intent` | 21-02 | VERIFIED | `bauded/src/manager.rs:1065` |
| `Manager::new` | 21-04 | VERIFIED | `bauded/src/manager.rs:468` `pub fn new(claude_cmd: String, persist: bool)` |
| `Manager::fail_next_agent_teardown_for_test` | 21-03 (overlooked) | VERIFIED | `bauded/src/manager.rs:2528` — the kill-failure seam already exists |
| `activate_branch_worktree` cited in the daemon's `create` wiring | 21-02 | MISSING | only an `App` method, `baude/src/app.rs:2792`; the daemon has no such function |
| `api::router` | 21-01/02/04 | VERIFIED | `bauded/src/api.rs:21`, routes through `:46` |
| `ApiError` / `not_found` / `mutation_error` | 21-01 | VERIFIED | `bauded/src/api.rs:104`, `:106`, `:111` |
| "every daemon error stays `(StatusCode, String)`" | 21-01 | VERIFIED | `bauded/src/api.rs:104` |
| `DELETE /sessions/{id}` | 21-01, 21-05 | VERIFIED | `bauded/src/api.rs:26` |
| `GET`/`POST /sessions` | 21-01, 21-02 | VERIFIED | `bauded/src/api.rs:25` |
| per-session SSE template (`async_stream`, `Event::default()`, `KeepAlive::default()`, `Sse<impl Stream>`) | 21-04 | VERIFIED | `bauded/src/api.rs:608-642`, `:652-686`; `KeepAlive` import `:10` |
| routes cited as "/stream and /activity-stream" | 21-04 | VERIFIED | real paths are session-scoped: `bauded/src/api.rs:39`, `:45` |
| existing test `activity_stream` | 21-04 | VERIFIED | `bauded/src/api.rs:1551` `activity_stream_guards_known_and_unknown` |
| existing test `stream` | 21-04 | MISSING | no test named `stream*` in `bauded/src`; that half of the gate is vacuous |
| `api_scope` / `app` / `body_json` / `post_json` / `initialized_repo` | 21-01 | VERIFIED | `bauded/src/api.rs:721`, `:753`, `:759`, `:768`, `:815` |
| "`persist::config_dir()` is redirected by `api_scope`" | 21-02 | VERIFIED | `bauded/src/api.rs:730` `TestRedirect::new(&root)`; see the isolation finding below |
| `exited_tracked_manager` | 21-01 | VERIFIED | `bauded/src/api.rs:936` |
| `flat_session_api_remains_a_non_hierarchical_compatibility_projection` | 21-01, 21-02 | VERIFIED | `bauded/src/api.rs:965`; **`:1002` lists `checkout_key` among the forbidden keys** |
| `api::tests` module path | 21-01…21-04 | VERIFIED | `bauded/src/api.rs:689-690` |
| `bauded/Cargo.toml:27` tokio features, `sync` absent | 21-04 | VERIFIED | `bauded/Cargo.toml:27` — no `sync` in the explicit list |
| `http-body-util` available in bauded tests | 21-04 | VERIFIED | `bauded/Cargo.toml:37`; imported `bauded/src/api.rs:698` |
| `BodyExt::frame` | 21-04 | UNCHECKABLE | external crate method; no in-repo `.frame(` call site |
| `collect_row_runtimes` / `restore_row_runtimes` / `confirm_remove_project` | 21-01, 21-06 | VERIFIED | `baude/src/app.rs:3948`, `:3981`, `:4004` |
| `App::persistence_root_for_test` / `atomic_failure_for_test` | 21-01 | VERIFIED | `baude/src/app.rs:666`, `:670`; used `:4031-4033` |
| `SavedDegraded` flag in `confirm_remove_project` | 21-01 | VERIFIED | `baude/src/app.rs:4082` |
| `Recheck` return in `confirm_remove_project` | 21-03 | VERIFIED | `baude/src/app.rs:4027` |
| `reserve_removal_target` | 21-01 | VERIFIED | `baude/src/app.rs:3895` |
| `App::lift_suppression` | 21-02 | VERIFIED | `baude/src/app.rs:2374` — takes `&mut self`, not `&self` |
| sweep guard calling `suppression::is_suppressed` | 21-02 | VERIFIED | `baude/src/app.rs:2604` |
| forced-kill TUI test cited at app.rs 12995-13060 | 21-03 | VERIFIED | `baude/src/app.rs:12997` |
| `SidebarRefusal::RemoteRemove` + `ActionKind::Remote` arm | 21-06 | VERIFIED | `baude/src/app.rs:344` |
| refusal notice text | 21-06 | VERIFIED | `baude/src/app.rs:1328` |
| `SidebarAction::RemoveProject` | 21-06 | VERIFIED | `baude/src/app.rs:332`, `:350` |
| `Modal::ConfirmRemove` + `RemoveProjectOutcome` | 21-06 | VERIFIED | `baude/src/app.rs:501`, `:511` |
| `Modal::ConfirmRemove` fields `target, report, scroll` | 21-06 | VERIFIED | `baude/src/app.rs:5797-5801` |
| `removal_kind_label` / `removal_row_label` | 21-06 | VERIFIED | `baude/src/app.rs:3789` `(&self, target: RemovalTarget)`, `:3819` `(&self, key: &RowKey)` |
| `removal_target_for_selection` / `open_remove_project_modal` | 21-06 | VERIFIED | `baude/src/app.rs:3865`, `:3910` |
| `SelId::Remote` | 21-06 | VERIFIED | `baude/src/app.rs:271` |
| `ConfirmRemove` keymap arm | 21-06 | VERIFIED | `baude/src/app.rs:5795` |
| `Self::removal_notice` | 21-06 | VERIFIED | `baude/src/app.rs:4091`; used `:5810` |
| `d_on_remote_row_sets_footer_notice` | 21-06 | VERIFIED | `baude/src/app.rs:12413`, exact line |
| `sidebar_action_d_refuses_remote_rows` (invalidated, unnamed) | — | VERIFIED | `baude/src/app.rs:12391` |
| `App.remote_snap` | 21-06 | VERIFIED | `baude/src/app.rs:608` |
| existing `confirm_remove_*` tests | 21-06 filter | VERIFIED | `baude/src/app.rs:12700`, `:12806` |
| `app::tests` module path | 21-06 | VERIFIED | `baude/src/app.rs:7851` |
| `ui::remove_project_confirmation_lines` | 21-05 | VERIFIED | `baude/src/ui.rs:2509` |
| `ui::tests::remove_project_lines*` | 21-05 | VERIFIED | `baude/src/ui.rs:3612`, `:3659`; `mod tests` `:2629` |
| `ui.rs` modal call sites of the two label helpers | — | VERIFIED | `baude/src/ui.rs:1782`, `:1783`, `:3889`, `:3900`, `:3935`, `:3937` — none in 21-06's `files_modified` |
| `RemoteInfo` + its `#[serde(default)]` back-compat pattern | 21-06 | VERIFIED | `baude/src/remote.rs:21`, defaults at `:33,35,41,44,50,52,57,63,68,75` |
| `RemoteSnapshot` / `RemotePoller` / `::start` / `snapshot` | 21-06 | VERIFIED | `baude/src/remote.rs:107`, `:133`, `:139`, `:194` |
| `delete` / `restart` / `create` / `set_archived` / `short_err` | 21-06 | VERIFIED | `baude/src/remote.rs:198`, `:206`, `:225`, `:261`, `:271` |
| `REQUEST_TIMEOUT` / `POLL_SECS` | 21-06 | VERIFIED | `baude/src/remote.rs:17` (3 s), `:16` (3 s) |
| `remote::tests` + existing `remote_*` tests | 21-06 filter | VERIFIED | `baude/src/remote.rs:444`, `:452`, `:464`, `:482` |
| `ureq::AgentBuilder::new().timeout_connect(..)` | 21-06 | VERIFIED | `baude/src/main.rs:275-276`; `baude/Cargo.toml:21` ureq 2.x, lock 2.12.1 |
| `Response::into_reader()` | 21-06 | UNCHECKABLE | external ureq API; no in-repo precedent |
| `app.js` `toast` / `api` | 21-05 | VERIFIED | `bauded/web/app.js:86`, `:93` |
| `esc()` | 21-05 (T-21-09) | VERIFIED | `bauded/web/app.js:33` |
| `refresh()` | 21-05 | VERIFIED | `bauded/web/app.js:137` |
| `state.online` owned by the poll | 21-05 | VERIFIED | `bauded/web/app.js:98`, `:145` |
| `deleteSession` / `createSession` / `openModal` / `closeModal` | 21-05 | VERIFIED | `bauded/web/app.js:530`, `:542`, `:560`, `:585` |
| `renderList` and `rowHtml` | 21-05 | AMBIGUOUS | `bauded/web/app.js:596` is `function renderList()`, but `:598` `rowHtml` is a closure *inside* it, not a top-level function |
| `openChat` / `openActivity` EventSource lifecycle | 21-05 | VERIFIED | `bauded/web/app.js:195`, `:236`; `new EventSource` `:199`, `:238` |
| timers and `visibilitychange`, 3 s poll | 21-05 | VERIFIED | `bauded/web/app.js:800-803`, `:814` |
| `#modal`, `#modal .box`, `.actions .ghost`, toast CSS | 21-05 | VERIFIED | `bauded/web/style.css:304`, `:310`, `:333`, `:336` |
| `sw.js` `SHELL` and `CACHE = "baude-v6"` | 21-05 | VERIFIED | `bauded/web/sw.js:4`, `:5` |
| `index.html` loads `/app.js` | 21-05 | VERIFIED | `bauded/web/index.html:20` |
| `web.rs` `asset!` routes | 21-05 | VERIFIED | `bauded/src/web.rs:27`, entries `:29-46` |
| `scripts/gate.sh` and its `--linux` switch | 21-06 | VERIFIED | `scripts/gate.sh:28`; steps enumerated at `:63-107`, no `node` step |
| `node --test` / `node --check` toolchain | 21-05 | VERIFIED | `node --version` → `v26.12.1`; `node:test` / `node:assert/strict` are runtime builtins |
| `cargo fmt --check`, `cargo clippy ... -D warnings`, `cargo build -p bauded`, `scripts/gate.sh[ --linux]` | all | UNCHECKABLE | build and lint outcomes, not grep facts |
| `/removals`, `/removals/preview`, `/rows`, `/events` as *existing* routes | — | n/a | confirmed absent from `bauded/src/api.rs` and `bauded/web/app.js`, consistent with being declared-new |

### Declared-new (excluded from verification)

`Manager::preview_removal`, `Manager::remove_rows`, `RemoveRowsError` (+ `Drift`/`Removal`/`Blocked`/`Busy`), `Manager::checkout_key_for_runtime_for_test`, `removal_error`, `POST /removals/preview`, `POST /removals`, `GET /rows`, `GET /events`, `removal_fixture`, tests `removals_*` / `rows_*` / `suppression_*` / `events_*`, `RowsView`, `RepositoryRow`, `CheckoutRow`, `StandaloneRow`, `Manager::rows`, `SessionInfo.checkout_key`, `Manager::checkout_key_for`, `Manager::lift_suppression`, `Manager::seed_standalone_for_test`, `Manager::session_pty_for_test`, the `#[cfg(test)]` setter for `persistence_blocked`, `DaemonEvent`, `Manager.events`, `Manager::subscribe_events`, `Manager::publish_for_test`, `next_sse_message`, the tokio `sync` feature, `bauded/web/removal.js`, `bauded/web/removal.test.js`, `BaudeRemoval` (`confirmationView`, `dropRows`, `removalNotice`, `clearingFailures`, `keyEquals`), `openRemoveModal`, `confirmRemoval`, `openEvents`, `state.rows`, `state.events`, the `/removal.js` asset route, the `baude-v7` cache name, CSS `.sections` / `.danger` / `.section h3`, `RemoteInfo.checkout_key`, `RemotePoller::{preview_removal, execute_removal, drop_rows, start_without_events}`, `RemoteRemovalOutcome`, `remote::test_double::serve`, the five new `remote_*` tests, `RemovalOrigin`, `Modal::ConfirmRemove.origin`, `confirm_remove_remote`, `d_on_remote_row_with_key_opens_confirm_remove_from_daemon_preview`, `enter_on_remote_confirm_*`.

### MISSING / AMBIGUOUS findings

- **AMBIGUOUS, material — 21-01's fixture cannot complete a real removal as specified.** 21-01 instructs the removal fixture to copy `flat_session_api_remains_a_non_hierarchical_compatibility_projection` (`bauded/src/api.rs:965`), which holds no `ApiScope` / `TestRedirect`. But `removal::remove`'s `clear_stage` calls `persist::config_dir()` unconditionally (`baude-core/src/removal.rs:671`), and `config_base` panics without a redirect (`assert_contained`, `baude-core/src/testing.rs:294-301`). 21-02 Task 3 relies on the opposite assumption ("`persist::config_dir()` is redirected by `api_scope`"). One of the two fixture strategies has to change.
- **MISSING — 21-04's gate names a test that does not exist.** The plan promises `cargo test -p bauded` still passes "the `stream` and `activity_stream` tests untouched"; only `activity_stream_guards_known_and_unknown` (`bauded/src/api.rs:1551`) exists. No test is named `stream*`, so half that assertion is vacuous.
- **MISSING — `activate_branch_worktree` is TUI-only.** 21-02 cites it when describing the daemon's branch-variant admitted path, but it exists solely as an `App` method (`baude/src/app.rs:2792`). The daemon has no such function.
- **AMBIGUOUS — `CheckoutRole` has three variants, the plan projects two.** `Main`, `PrimaryDefault`, `ManagedBranch` (`baude-core/src/repository.rs:284-288`); the `/rows` test asserts only `"main"` and `"worktree"`, and `PrimaryDefault` — what core's own fixture uses for a default checkout (`:1240`) — has no specified mapping. This independently corroborates codex's main-checkout-classification concern.
- **AMBIGUOUS — `AtomicFailure`** is `pub enum` at `baude-core/src/persist.rs:242`, not `pub struct` in `baude-core/src/testing.rs`; 21-03's prescribed grep returns nothing.
- **AMBIGUOUS — `track_runtime_for_test`** is at `bauded/src/manager.rs:2268`, not inside the cited `2514-2527` block.
- **AMBIGUOUS — `reconcile_legacy_session` caller** is at `bauded/src/manager.rs:517`, not `:519`.
- **AMBIGUOUS — fourth `remove` stage** is `clear_stage` (`baude-core/src/removal.rs:456`), not "suppression"; suppression is one clearing kind among bindings, markers and repository dirs.
- **AMBIGUOUS — a private `ClearingResults`** does exist (`baude-core/src/removal.rs:482`), so 21-05's "there is no `ClearingResult` type" is true only for the singular spelling.
- **AMBIGUOUS — `rowHtml`** is a closure inside `renderList` (`bauded/web/app.js:598`), not a top-level function.
- **Already-existing seam the plan proposes to add:** `Manager::fail_next_agent_teardown_for_test` (`bauded/src/manager.rs:2528`) is exactly the kill-failure seam 21-03 would add `session_pty_for_test` for.

## Cross-Artifact Fact-Drift (advisory — contributes to NEITHER count)

`drift-guard phase-status --phase 21` returns:

```json
{ "verdict": "uncheckable", "reason": "phase_not_in_roadmap", "phase": "21",
  "stateStatus": "Executing Phase 20", "roadmapStatus": null,
  "stateRank": null, "roadmapRank": null, "authority": "STATE.md" }
```

The verdict is **uncheckable**, not `drifted`, so there is no phase-status finding. `UNCHECKABLE` classifies as `INFO` / `hardBlock: false` under the effective `intel` authority and is recorded in the verification-coverage block below. For the record, `ROADMAP.md` does contain `### Phase 21: Daemon and PWA Parity`; the tool's rank lookup did not resolve it, and `STATE.md` still reads `current_phase: 20`, `status: executing`, `Status: Executing Phase 20`.

Judgment axes:

- **ROADMAP Success Criteria vs PLAN `must_haves.truths` — CLEAN.** All three Phase 21 success criteria are asserted somewhere in the union of plan truths and none is contradicted. SC1 (REST removal across all four row kinds) maps to 21-01 plus 21-02; SC2 (SSE drop without a refresh) to 21-04/05/06; SC3 (PWA remove with a TUI-equivalent confirmation) to 21-05. The PWA keeping its existing 3 s poll is reconciliation alongside the event drop, not a refresh-dependency, so it does not contradict SC2.
- **ROADMAP requirement IDs vs PLAN task refs — CLEAN.** API-01, API-02 and API-03 are all defined for Phase 21 and all cited (API-01 in 21-01/02/03/06, API-02 in 21-04/05/06, API-03 in 21-02/05). No plan's `requirements:` field cites a foreign ID. 21-05's truth text mentions `TUI-02`, but `REQUIREMENTS.md:57` defines API-03 *as* "a confirmation equivalent to TUI-02", so that is a sanctioned cross-reference. `T-21-*` strings are threat-model ids, not requirement ids.
- **CONTEXT.md Decisions glossary vs PLAN term usage — CLEAN.** Every glossary term is used with its CONTEXT meaning. Two near-misses were checked and dismissed as superset-not-contradiction: D-01 says the request body is the core `RemovalTarget` JSON while 21-01 extracts `Json<RemovalReport>`, which D-04 in the same decisions block forces and `21-01-PLAN.md:36` resolves explicitly; and 21-05's "clearings" section matches D-13's own wording even though the struct fields are `cleared` / `clearing_failures`, which `21-01-PLAN.md:65` records accurately.

One out-of-axis staleness, flagged as advisory only: `.planning/ROADMAP.md:72-75` still shows `Phase 19 ... (0/? plans)`, `Phase 20 ... (0/? plans)` and `Phase 21 ... (0/? plans)` in the milestone checklist while the detail entries show all seven Phase 19 plans `[x]` and give Phase 20 four plans and Phase 21 six.

## Verification coverage

A clean axis below means "checked and clean", never "not checked". Everything this cycle could not decide is named here.

**Effective authority:** `drift-guard authority --raw` → `intel`. Verdict severities under that authority: `VERIFIED` → `none`; `MISSING` → `needs-acknowledgement`; `AMBIGUOUS` → `MEDIUM`; `UNCHECKABLE` → `INFO`. No verdict in this cycle is a hard block. **UNCHECKABLE was never treated as verified.**

### Lane coverage

- `claude` — ran, counted.
- `codex` (`gpt-6.1-sol`, reasoning `high`) — ran, counted.
- `opencode` — **FAILED, no output.** `spawn error: ETIMEDOUT`; the CLI returned no assistant text (`stop reason=?`, `output tokens=?`). The lane received no effort argument, so the reviewer CLI's own configuration applied and the outer timeout killed it. **Excluded from this cycle and from both counts.** Consequently every finding in this cycle rests on two reviewers, and the consensus gate ran with the minimum corroboration quorum: one reviewer dropping out would have disabled the gate entirely.

### Axes checked

| Axis | Status |
|---|---|
| Source-grounding of plan-cited symbols | RUN — per-symbol table above |
| `drift-guard phase-status --phase 21` | RUN — verdict `uncheckable` (`phase_not_in_roadmap`), `INFO`, not a finding |
| ROADMAP Success Criteria vs PLAN `must_haves.truths` | RUN — CLEAN |
| ROADMAP requirement IDs vs PLAN task refs | RUN — CLEAN |
| CONTEXT Decisions glossary vs PLAN term usage | RUN — CLEAN |
| Consensus gate (existence vs judgment classification of every HIGH) | RUN — one corroborated, seven single-reviewer judgment-class counted, one single-reviewer existence-class independently confirmed |
| Execution of any plan's verify command | NOT RUN — plans are unexecuted; no `cargo test`, `cargo build`, `node --test` or `scripts/gate.sh` lane was invoked in this review |
| Behaviour under a live daemon, live PTY or a browser | NOT RUN — static review only |

### UNCHECKABLE / skipped symbols and axes, and why

Every item below was looked at and could **not** be decided. None is recorded as verified.

- `http_body_util::BodyExt::frame` (21-04) — external crate method; no in-repo call site confirms it on http-body-util 0.1.
- `ureq::Response::into_reader()` (21-06) — external ureq 2.x API; `rg 'into_reader'` finds no in-repo precedent, unlike `AgentBuilder` / `timeout_connect` which are used in `baude/src/main.rs:275-276`.
- `tokio::sync::broadcast` surface — `channel(capacity)`, `Sender::send` ignoring `SendError`, `RecvError::{Lagged, Closed}` (21-04) — external crate behaviour, and the `sync` feature is not yet enabled so nothing in-repo exercises it.
- `node:test` / `node:assert/strict` module surface (21-05) — Node runtime builtins; node v26.12.1 is installed but the module API is not a repo fact.
- `Manager::new("sleep 30", true)` as literally quoted (21-01) — a signature/arity question, UNCHECKABLE under grep authority. What *is* verified: the parameter is `String` (`bauded/src/manager.rs:468`) and the existing call passes `"sleep 30".into()` (`bauded/src/api.rs:980`).
- `removal_kind_label` / `removal_row_label` gaining an `origin` parameter (21-06) — whether the rewrite type-checks is a signature question. The current signatures and the six `ui.rs` call sites are verified facts; the compile outcome is not grep-decidable.
- `Manager::lift_suppression(&self, ...)` (21-02) — whether a `&self` variant compiles against the proposed `create` call sites is a signature matter. The TUI analogue taking `&mut self` (`baude/src/app.rs:2374`) is verified.
- `cargo fmt --check`, `cargo clippy -p bauded --all-targets -- -D warnings`, `cargo build -p bauded`, `scripts/gate.sh`, `scripts/gate.sh --linux` (all plans) — build and lint outcomes, not grep facts. None was executed in this review.
- SSE wire behaviour (21-04) — keep-alive comment framing, "no history to late subscribers", lagged-subscriber continuation, and event ordering relative to the `Manager` lock are runtime properties of the axum SSE + broadcast combination.
- Suppression entry and lift as observed over REST (21-02) — depends on runtime config-dir redirection at test time, which is itself the subject of the fixture-isolation finding above.
- Whether any of the five `git::discover_repository` call sites (`bauded/src/manager.rs:156, 921, 1073, 1347, 1418`) constitutes *implicit admission* of a row the client did not name (21-02) — a control-flow judgement, not a name lookup.
- The two-tab and live-daemon human checks in 21-05, and all PWA DOM behaviour — runtime, browser-side.
- `drift-guard phase-status --phase 21` — verdict `uncheckable` (`phase_not_in_roadmap`); the tool's rank lookup did not resolve `### Phase 21: Daemon and PWA Parity` in ROADMAP.md. `INFO` severity, not a finding, and explicitly not treated as a pass.
- **The `opencode` reviewer lane — no output at all** (`ETIMEDOUT`). Whatever a third reviewer would have found is simply unmeasured in this cycle, and the consensus gate ran at the bare two-reviewer quorum as a result.

### Counting

- Consensus gate: **active** (two reviewers ran, neither discounted).
- Corroborated HIGHs: **1** (21-06 `ui.rs` scope / helper-signature compile break).
- Single-reviewer judgment-class HIGHs counted (codex carries no discount marker): **6** — 21-01 fixture liveness, 21-02 standalone parity, 21-03 partial-failure guarantee, 21-04 client-side resurrection, 21-05 PWA poll/event arbitration, 21-06 three-second execute timeout.
- Single-reviewer existence-class HIGHs counted after independent confirmation: **1** — 21-02's `/sessions` compatibility-test contradiction, confirmed at `bauded/src/api.rs:1002` and `21-02-PLAN.md:63` in the source-grounding pass above.
- HIGHs suppressed by the gate: **none**.
- `current_high = 8`, `current_actionable = 30`. Cross-artifact fact-drift findings are advisory and contribute to neither count.
