---
phase: 15
reviewers: [codex]
reviewed_at: 2026-09-21T17:56:00Z
plans_reviewed: [15-01-PLAN.md, 15-02-PLAN.md, 15-03-PLAN.md, 15-04-PLAN.md]
models:
  codex: "gpt-5.6-sol"
model_sources:
  codex: "flag"
---

# Cross-AI Plan Review — Phase 15: Startup and Idle Performance

<!-- gsd:plan-revision-conflicts:begin -->
## Plan-Revision Conflicts
<!-- gsd:plan-revision-conflicts:end -->

## 15-01

**Verdict**: Close, with clarifications needed.

### Strengths

- Dirty-flag gate on `terminal.draw()` is a standard and effective pattern for eliminating unconditional redraws.
- Screen generation counters provide lock-free change detection without channels, fitting the TUI's single-threaded render loop.
- Timing infrastructure with `Instant` checkpoints is straightforward and provides the diagnostic precision required by PERF-01.
- Test inventory is concrete: dirty-flag setting/clearing, generation-counter polling, idle-zero-draws validation, and timing output format checks are all verifiable.

### Concerns

- **Waiting-row timer scope:** The plan states "update waiting-row timer at 1 Hz only when waiting rows visible" but does not specify whether this is guarded by the dirty flag or whether it runs unconditionally. If tick() runs unconditionally every 50 ms, the 1 Hz update alone would generate 20 timer calls per second even with the dirty flag gated. Clarify whether `last_waiting_update` is checked inside the dirty-flag-marking section (preferred) or outside it.
- **Remote snapshot batching:** The plan promises "remote snapshot changes only set dirty when fetched_ms or session count changed (not every tick)" but does not define the `remote_idle` tracking or the condition that recognizes a no-op fetch. Specify how `fetched_ms` is recorded, what "fetched_ms changed" means given that it updates every fetch, and how extraneous snapshot polls are distinguished from genuine state changes.

### Suggestions

- Add an explicit section to the must-haves confirming that `app.tick()` is called every 50 ms unconditionally (as part of event polling), but `terminal.draw()` is called only on dirty, reducing the effective redraw frequency from ~20 Hz to 1-4 Hz depending on activity.
- In the remote snapshot verification, show the actual data flow: snapshot fetch → compare old/new → set dirty only if session list or meaningful field changed (not if only metadata timestamps changed).

### Risk Assessment

**LOW.** The dirty-flag design is well-established and the implementation is straightforward. The clarifications above are non-blocking scope questions, not architectural risk.

---

## 15-02

**Verdict**: Requires major revision (BLOCKING).

### Strengths

- Incremental restore (one session per loop iteration) is a correct approach to unbind restore from the render path.
- 250 ms keyboard probe timeout is a reasonable bound that prevents indefinite blocking.
- Batching durable state writes until restore completes addresses the fsync storm identified in the phase goal.

### Blocking Concerns

1. **PTY registration invariant violation (CRITICAL):** A generic "restoring guard" that suppresses `save_durable_status()` calls breaks the existing crash-safety contract. The codebase treats PTY registration as atomic: children are spawned paused, their exact `ProcessIdentity` is read, and that identity is durably recorded before the child unpauses ([baude-core/src/pty.rs:245](baude-core/src/pty.rs:245), [baude-core/src/pty.rs:321](baude-core/src/pty.rs:321)). If a crash occurs after a child unpauses but before its identity is durably written, the next restart will orphan the process. **Solution required**: Collect all children in paused state, batch-record all identities once, then unpause all. This cannot be done with the current `Pty::spawn_registered_with` API because it unpause immediately inside the callback. The plan must redesign either the callback contract or introduce a two-phase commit.

2. **Daemon `/info` cannot expose TUI startup timings (CRITICAL):** The daemon runs in a separate process and reports its own state—not the TUI's. Startup times recorded in `main()` are not visible to `bauded`. Either:
   - Remove the daemon artifact claim and measure only TUI timing via `BAUDE_TIMING=1`, or
   - Define a TUI-to-daemon IPC protocol to report startup metrics, or
   - Measure daemon startup separately with its own timing instrumentation.
   As written, the artifact does not exist. Pick one approach.

3. **Wave ownership overlap (STRUCTURAL):** Plan 15-01 claims to deliver "first empty frame renders BEFORE any session restore" ([15-01 must-haves](15-01-PLAN.md)), while 15-02 claims to "implement non-blocking first-frame rendering by deferring session restore to the main loop." These are the same work. Clarify which plan owns the startup restructuring. If 15-02 depends on 15-01, the responsibility cannot overlap.

### Non-Blocking Concerns

- **Restore is more complex than the plan describes:** The method at [baude/src/app.rs:1276](baude/src/app.rs:1276) performs teardown, activation recovery, standalones, primary checkouts, launch directory admission, breadcrumbs, and selection—not a simple list iteration. `RestoreProgress` tracking a single current-index is insufficient. Design an explicit work queue with states (e.g., `enum RestoreStep { SetupPhase, AdmitSessions { remaining }, Cleanup }`) and specify failure/ordering semantics. Show the actual state machine in the task action.

- **Keyboard probe mechanism is unclear:** The plan says "uses `event::poll(timeout)` instead of blocking `read()`" but does not show how the probe closure then reads the response without blocking. A bounded poll waiting for data is correct, but the probe closure must be non-blocking (or the timeout is ineffective). Show the exact code path and explain how ESC sequence parsing does not block on partial input.

### Suggestions

- For PERF-04, start with a design doc showing the PTY gate release algorithm. It is not a small clarification.
- If `RestoreProgress` is simple (count + index), show a concrete test case with 5+ saved sessions and verify the state machine's progression through tick().
- Add a "State Machine Verification" subsection showing the path from `restore()` through `restore_one_session()` loop to final state.

### Risk Assessment

**HIGH (BLOCKING).** The PTY registration invariant violation is a crash-safety regression. The daemon artifact does not exist. The wave overlap indicates incomplete requirements negotiation with 15-01. This plan cannot be executed as written.

---

## 15-03

**Verdict**: Requires major revision (BLOCKING).

### Strengths

- The current hot path is accurately identified: polling every session on every tick scales badly ([baude/src/app.rs:3714](baude/src/app.rs:3714), [bauded/src/manager.rs:2347](bauded/src/manager.rs:2347)).
- The exited-session skip is effective: exited sessions are already filtered centrally in `Session::poll_meta` ([baude-core/src/session.rs:302](baude-core/src/session.rs:302)), so adding an archived check there catches both TUI and daemon.
- Usage polling can be made optional: the structure already supports conditional startup ([baude/src/app.rs:765](baude/src/app.rs:765)).

### Blocking Concerns

1. **Raw `pty.suspend(pgid)` is unsafe without identity verification (CRITICAL):** The codebase already treats PID reuse as a serious hazard: before signaling a process group, code verifies the complete `ProcessIdentity` (state, cmdline, cwd) to ensure the child has not exited and been replaced ([baude-core/src/session.rs:641](baude-core/src/session.rs:641), [baude-core/src/session.rs:655](baude-core/src/session.rs:655)). A raw `pty.suspend(pgid)` accepting only a pgid would send SIGSTOP to the wrong process group if the original child exited and a new process accidentally reused the same pgid. **Solution required**: `suspend_idle_child()` must accept and verify a `ProcessIdentity` reference before calling `pty.suspend()`. Show the verification call in the action section.

2. **Daemon archive behavior completely omitted (CRITICAL):** Remote archive is implemented in the daemon at `Manager::set_archived` ([bauded/src/api.rs:286](bauded/src/api.rs:286), [bauded/src/manager.rs:2362](bauded/src/manager.rs:2362)). Neither [bauded/src/api.rs](bauded/src/api.rs) nor [bauded/src/manager.rs](bauded/src/manager.rs) is in this plan's file list. Idle child policy must apply consistently when the TUI archives a session and when the daemon receives a remote archive request. Either extend the plan to cover the daemon, or explicitly defer daemon idle policy to a later phase and document the gap.

### Non-Blocking Concerns

- **PERF-07 is incomplete:** The plan specifies only "on auto-archive with policy=suspend, send SIGSTOP." It does not specify:
  - The `stop` action (SIGKILL vs SIGTERM, immediate vs delayed)
  - Stopping on manual archive (does it apply the same policy?)
  - When `resume_idle_child()` is called (on unarchive only, or other paths?)
  - Failure handling (what if SIGSTOP fails, e.g., process already exited?)
  - UI representation (how do users see a suspended vs active child state?)
  - Add tasks for each missing piece.

- **mtime gate is underspecified:** Claude polling touches the session file, transcript discovery and tail, context file, bridge file, event file, `.planning/STATE.md`, and Git HEAD ([baude-core/src/meta.rs:223](baude-core/src/meta.rs:223)). Some paths use offsets (transcript/event tails), others are full file reads (session state). The plan says "unchanged metadata file costs one stat (not a read)" but does not define:
  - Which fields require mtime tracking and which use offsets?
  - Is there one mtime per file or per logical entity (session, transcript, etc.)?
  - How are directory scans (e.g., transcript discovery) handled?
  - When does the cache invalidate (replacement, truncation, deletion, session-id rotation)?
  - **Task required**: Break down meta.rs polling into separate cache keys and invalidation rules.

- **OpenCode backend is excluded:** The opencode backend polls HTTP, not metadata files ([baude-core/src/backend/opencode.rs:178](baude-core/src/backend/opencode.rs:178)). The plan applies only to Claude. Either extend it to opencode with appropriate HTTP-caching semantics, or explicitly document that PERF-06 does not apply to opencode and explain the deadline.

- **`idle_child_policy: Option<String>` allows silent invalid values:** Use a serde enum with known variants (keep, suspend, stop) and define behavior when config is invalid (e.g., warn and default to "keep"). Add a test for invalid value rejection.

- **Unit tests cannot verify production behavior:** The usage poller is conditionally compiled out in tests ([baude/src/usage.rs:51](baude/src/usage.rs:51)). Tests for configurable intervals need an injectable worker or a clock seam so production behavior is actually tested, not just configuration parsing. Add a TestClock or thread-spawn injection point.

### Suggestions

- Start with a table: "Metadata Sources and Cache Invalidation," one row per file/entity, columns for read-offset vs mtime, cache-hit condition, invalidation rule, test case.
- Diagram the polling hot path: session list → for-each session → skip archived/exited → poll_meta → check mtime → maybe read. Show where each guard applies.
- For suspend/resume, add state transitions: `Active → Suspend(identity) → Suspended → Resume(identity) → Active`. Define error paths.

### Risk Assessment

**HIGH (BLOCKING).** The PID reuse hazard is a correctness regression. The daemon behavior gap means the phase's goal cannot be achieved for remote sessions. The mtime cache is under-specified and cannot be implemented without significant redesign effort.

---

## 15-04

**Verdict**: Close, with source-accurate corrections required.

### Strengths

- The proposed codes and colors map cleanly onto the existing status branches for checkout, standalone, and live/remote session rows ([baude/src/ui.rs:470](baude/src/ui.rs:470), [baude/src/ui.rs:587](baude/src/ui.rs:587), [baude/src/ui.rs:796](baude/src/ui.rs:796)).
- A shared `status_code()` helper prevents code drift between session and checkout renderers.
- The existing rendered-buffer test fixture is suitable for verifying exact glyphs and styles across viewport sizes ([baude/src/ui.rs:2511](baude/src/ui.rs:2511)).
- Help already contains a status legend, so legend expansion is a straightforward replacement ([baude/src/ui.rs:2246](baude/src/ui.rs:2246)).

### Concerns

- **Incorrect animation site count:** The plan claims "replace all seven `spinner()` and `flash_on()` call sites" but there are exactly **five**: three `spinner()` calls at [baude/src/ui.rs:473](baude/src/ui.rs:473), [baude/src/ui.rs:480](baude/src/ui.rs:480), [baude/src/ui.rs:589](baude/src/ui.rs:589) and two `flash_on()` calls at [baude/src/ui.rs:794](baude/src/ui.rs:794), [baude/src/ui.rs:807](baude/src/ui.rs:807). Update the must-have and test count to 5, not 7.

- **Status code mapping is ambiguous:** The plan says "create `fn status_code(status: Status)` but `Status` exists in two forms: `session::Status` (for live/remote sessions) and `hierarchy::LocalStatus` (for checkouts). Archived live/remote rows are represented by a separate `archived: bool` field, not a status variant. A single helper cannot accept both without an adapter. Either define a small UI-only enum (e.g., `RowStatus { code, archived }`) or explicitly show adapters for each call site.

- **Sidebar footer height interaction:** The sidebar already reserves a six-row usage footer ([baude/src/ui.rs:231](baude/src/ui.rs:231), [baude/src/ui.rs:915](baude/src/ui.rs:915)). Adding a legend requires updating footer height and boundary tests so session rows are not unexpectedly displaced. Verify that the footer constraints still hold when both usage and legend are rendered, and that session row tests adjust their expected viewport placement.

- **Test coverage is incomplete:** The plan lists tests for "status_code_matches_spec" and "legend_rendered_in_footer" but does not specify coverage for:
  - Every code across all contexts: checkout (7+ codes), standalone (5+ codes), local session (5+ codes), remote session (5+ codes), archived override, unknown-remote-status fallback.
  - Exact color/style matching for each code.
  - Legend visibility at different viewport heights (footer too small, etc.).
  - Removal of all animation glyphs (5 spinner/flash calls verified gone).
  - Re-add test cases to cover all seven contexts.

- **README changes must be comprehensive:** The plan says "add a 'Performance' subsection" but must also **replace** the existing animated-symbol references in the Getting Started section ([README.md:29](README.md:29)) and any screenshots or diagrams showing spinners/flashes ([README.md:235](README.md:235)). Audit the entire README for symbol references and update them.

### Suggestions

- For the status enum: define `pub enum UiStatus { Waiting, Busy, Done, Exited, Closed, Archived, Unknown }` and show the mapping from `session::Status` + `archived` to `UiStatus` in a table in the task action.
- For footer height: show the formula `footer_height = usage_height + legend_height` and how it affects session list height. Include a visual diagram.
- For README: create a separate task (15-04B?) for documentation replacement so it is clearly distinct from the code changes.

### Risk Assessment

**MEDIUM.** The core animation replacement is straightforward, but source-accurate fixes (correct call site count, enum adapters, footer constraints, comprehensive tests, README audit) are required. The current plan is close but will fail implementation without these corrections. Not blocking, but incomplete.

---

## Consensus Summary

### Agreed Strengths

- All plans correctly identify the performance bottlenecks: unconditional redraws at 20 Hz, synchronous session restore, unbounded keyboard probe, per-session polling regardless of archive state.
- The phase is well-scoped and well-sequenced: dirty flag → incremental restore → polling gates → UI codes.
- TDD structure and test-first approach are sound across all plans.

### Agreed Concerns

- **Wave 2 plans (15-02 and 15-03) require major restructuring before execution.** Both have blocking safety/correctness issues (PTY invariant, process identity verification, daemon behavior) that cannot be fixed incrementally. Recommend: schedule a 1-day design review before proceeding, focusing on PTY state machine and daemon behavior contracts.
- **15-04 is close but source-proof-reading is needed:** The call-site count is wrong by 2, and the status enum requires explicit adapters for two different source types. These are catches-on-implementation, not blockers, but they will surface during review.
- **Cross-plan responsibility overlaps (15-01 vs 15-02 for startup split, 15-02 vs 15-03 for daemon)** indicate that requirements were negotiated in serial rather than defined together. Recommend: create a unified "Startup and Restore State Machine" document before replanning.

### Divergent Views

None. Codex review was the only reviewer.

---

## Cycle 2 Review (Codex — 2026-09-21)

**Convergence Status**: Plans were revised in commits e2910a2, 21ca40b, 726351c. Each plan now carries a Cycle 1 Review Dispositions ledger. Codex reviewed against current plan text with source-grounding; cycle 1 findings explicitly dispositioned as incorporated or rejected with rationale are treated as resolved. This cycle assesses whether revisions addressed cycle 1 concerns and whether plans are now executable.

### 15-01 — Dirty-Flag Render Gate

**Cycle 1 Disposition**: Waiting-timer scope and remote-snapshot batching clarification concerns were marked incorporated. Plans revised to narrow waiting-timer update to dirty-flag section and refine remote snapshot comparison.

**Cycle 2 Findings**:

Cycle 1's conceptual clarifications were addressed, but the implementation architecture still has critical flaws that block execution:

1. **CRITICAL — Generation counter ownership is unsafe for PTY threads.** The plan places `screen_generation: u64` on `Session`, expecting the background PTY thread to mutate it ([pty.rs:340](pty.rs:340)). That thread owns cloned `Arc`s for parser and timestamps but has no write access to `Session`. Without refactoring to `Arc<AtomicU64>` on `Pty` or a message channel, the mutation is impossible. Cycle 1 identified this gap; the revision did not fix it. **Blocking until the thread-safe mechanism is shown in the action.**

2. **CRITICAL — First-frame ordering is not guaranteed.** The loop calls `app.tick()` before drawing ([main.rs:443](main.rs:443)). Plan 15-02 places restore in `tick()`, so the first restore step precedes the first frame unless an explicit `first_frame_drawn` gate is added. Cycle 1 noted the contradiction; the revision still does not resolve it.

3. **MEDIUM — Remote snapshot comparison is still uncompilable.** `RemoteInfo` and `RemoteSnapshot` do not implement `PartialEq`. The revised plan adds normalization intent but does not update the actual enum derivations or show a tested comparison function. Cycle 1 flagged the pseudocode; code anchors confirm it still won't compile.

4. **MEDIUM — Timing events have no ownership channel.** The plan says `app.tick()` records restore completion and first metadata poll, but `App` still has no access to `StartupTiming` (which lives in `run()`). Cycle 1 suggested a `TickOutcome` abstraction; the revision still omits it.

**Result**: Partially resolved. The waiting-timer and remote-snapshot clarifications from cycle 1 are reflected in the text, but the critical issues (PTY thread safety, first-frame ordering, event channel) remain unaddressed. **Status: BLOCKING.**

### 15-02 — Non-Blocking Session Restore and Keyboard Probe

**Cycle 1 Disposition**: PTY durability and daemon-process concerns were marked incorporated into intent; actual implementation details were deferred. Plan revised to recognize crash-safety invariant and propose per-session restore queuing.

**Cycle 2 Findings**:

Cycle 1 identified that the implementation cannot use the existing `Pty::spawn_registered_with` API, which pauses only momentarily. The revision recognizes this but still proposes the same unsafe pattern:

1. **CRITICAL — The proposed "paused spawn" API does not exist and cannot be synthesized.** The current `Pty::spawn_registered_with` calls the callback and writes the gate token immediately inside the method ([pty.rs:321](pty.rs:321)). There is no way to collect paused runtimes for batch recording. The plan's pseudocode calls `restore_phase_a()` returning paused `Pty` objects, but the registration callback is not under the plan's control. Cycle 1 stated this; revision still does not redesign the API contract.

2. **CRITICAL — Restore still runs before first draw.** The loop sequence is `tick → draw`. Plan 15-02 puts Phase A (spawn) in the first tick. That is before the draw, contradicting cycle 1's requirement for "first frame before restore." Cycle 1 explicitly blocked on this; revision does not fix the order.

3. **CRITICAL — Phase A is not incremental.** Spawning all saved Claude and shell children synchronously in one tick blocks the input loop for seconds. Cycle 1 locked the requirement: "one saved session's spawn/admission work per iteration." The revision still does not show per-session batching within Phase A.

4. **HIGH — Keyboard probe is functionally wrong and cycle 1 flagged it.** The plan calls `event::poll(250ms)` before invoking the existing `supports_keyboard_enhancement` probe. But that probe is an ESC sequence query that itself times out internally for 2 seconds ([main.rs:401](main.rs:401)). Waiting before calling it does not bound its operation—the bound is never achieved. Cycle 1 asked for custom raw-FD query/parser; revision describes it in the behavior section but still implements a wrapper around the problematic API in the action.

5. **HIGH — Daemon timing has no task ownership.** The plan requires instrumentation in `bauded/src/main.rs` where config load and listener binding occur ([bauded main.rs:185](bauded main.rs:185)), but neither task owns that file. Cycle 1 noted the missing artifact; revision does not assign it.

6. **MEDIUM — Restore complexity is underestimated.** Current `app.rs:1231` performs teardown recovery, activation recovery, standalones, primary, folder-context, and selection. The plan queues only saved sessions. Cycle 1 suggested explicit `RestoreStep` variants; revision still treats restore as a simple list iteration.

**Result**: Minimally revised. The plan acknowledges the issues at the intent level but the action still attempts the same unsafe pattern. Cycle 1 concerns remain unaddressed in executable form. **Status: BLOCKING.**

### 15-03 — Idle Polling Gates and Child Suspension

**Cycle 1 Disposition**: Archive semantics, metadata sources, and identity verification were marked incorporated. Plan revised to recognize identity invariant and separate archive paths.

**Cycle 2 Findings**:

Cycle 1 flagged that the metadata gate was incorrectly specified and daemon behavior was absent. The revision recognizes these gaps but does not fill them:

1. **CRITICAL — Metadata mtime gate is still unsound.** Claude polling reads session file, transcript, context, bridge, event file, `.planning/STATE.md`, and Git HEAD ([meta.rs:223](meta.rs:223)). Cycle 1 detailed that the plan invents two non-existent file paths and ignores the actual sources. The revision now lists the intent to track "source-specific deletion, truncation, replacement, rotation" but the action still only says "add two mtimes." Actual session files live under the Claude config directory ([meta.rs:246](meta.rs:246)), not in a `cwd/session.json`. The plan cannot gate properly without separating sources and addressing the offset-based reads (transcript/event tails).

2. **CRITICAL — Daemon archive path is still missing.** Cycle 1 noted that daemon auto-archive is in `Manager::set_archived` ([bauded/src/manager.rs:2362](bauded/src/manager.rs:2362)). The plan still omits `bauded/src/api.rs` and `bauded/src/manager.rs` from files_modified, and no task implements daemon-side idle policy. Remote archive behavior cannot converge without it.

3. **HIGH — Process group signaling is unsafe without identity verification.** Cycle 1 required re-inspection of the complete `ProcessIdentity` before calling `libc::kill(-pgid, ...)`. The revised plan says it "recognizes the invariant" but the action still calls `pty.suspend(pgid)` without showing identity checks. The existing safe code at [session.rs:641](session.rs:641) shows the pattern; the plan must cite it.

4. **HIGH — Shell panes are completely omitted.** The locked decision (UX-02 footnote) applies to Claude and shell PTYs. The plan only calls `self.claude.suspend()`. A single `child_suspended` boolean cannot represent partial failure if one PTY succeeds and one fails.

5. **HIGH — Archive transaction semantics are undefined.** If signaling fails after state mutation, no compensation is specified. Cycle 1 asked for: persist archive intent, apply policy, persist runtime state, report results. The revision still does not sequence these steps.

6. **HIGH — Remote wire representation is incomplete.** The plan mentions `SessionInfo.suspended` but does not update `RemoteInfo.suspended` ([remote.rs:20](remote.rs:20)). The PWA cannot display a remote child's suspended state without the wire field.

7. **HIGH — Usage-poller test coverage is unachievable in the current test build.** The poller is compiled out in tests ([usage.rs:51](usage.rs:51)), so the plan's test "proving an enabled thread spawns" cannot run. Cycle 1 asked for an injectable spawner; revision still does not provide one.

8. **MEDIUM — Failure backoff violates the configured interval.** With `usage_poll_secs=600`, the failure path sleeps 300 seconds, running twice as often as configured. The action should sleep the configured interval on failure.

**Result**: Acknowledged but not fixed. Cycle 1 identified the gaps; revision recognizes them in the behavior section but the action remains incomplete. Metadata sources, daemon behavior, identity verification, and testability are all unaddressed. **Status: BLOCKING.**

### 15-04 — Static Status Codes and Legend

**Cycle 1 Disposition**: Animation-site count, adapter detail, and README audit were marked incorporated. Plan revised to identify exactly five animation sites and add enumeration detail.

**Cycle 2 Findings**:

The revised plan correctly counts five animation sites and names the call sites by line. The glyph-to-code mapping is clear. However, the test coverage is still missing and the footer geometry is undefined:

1. **HIGH — Test coverage is promised but not included in the tasks.** The plan says "create `TestBackend` coverage" for "exact glyph/style tests" but both tasks say "no tests needed." Cycle 1 raised this; revision still omits the tests. Without render-buffer tests for glyphs, colors, archived override, unknown fallback, and legend visibility, the feature will fail code review.

2. **MEDIUM — Adapter pseudocode does not match the actual enums.** `LocalStatus` uses `Working`, not `Busy` ([hierarchy.rs:32](hierarchy.rs:32)). The shown match is non-exhaustive. Cycle 1 asked for actual adapters; revision still shows pseudocode.

3. **MEDIUM — Footer geometry is undefined.** The sidebar reserves six rows for usage ([ui.rs:231](ui.rs:231), [ui.rs:934](ui.rs:934)). The legend requires space that either displaces existing footer content or expands the footer. No task defines `legend_area`, `footer_height`, or boundary tests. Cycle 1 flagged this; revision does not specify the geometry.

4. **MEDIUM — Help modal expansion assumes mutability.** The help is a fixed 39-row `Paragraph::new(vec![Line...])` ([ui.rs:2201](ui.rs:2201)). Adding seven legend lines requires updating the line vector, modal height, and clipping tests. The revision's pseudocode does not show these changes.

5. **LOW — Plan metadata inconsistency persists.** Must-haves say five call sites; other sections still reference seven. Cycle 1 flagged this; revision only partially corrected it.

**Result**: Partially addressed. Cycle 1's animation-site count and adapter concerns were mostly incorporated; the test coverage gap and footer geometry remain unspecified. **Status: ACTIONABLE (not blocking, but requires completion before execution).**

### Summary

**Convergence Verdict**: Plans remain at cycle 1 major-revision level. Cycle 2 found that:

- **15-01 and 15-02** still have **critical blocking issues** (PTY thread safety, first-frame ordering, paused-spawn API, daemon omission, incremental restore ordering) that cycle 1 flagged and revision did not fix.
- **15-03** still has **critical blocking issues** (metadata source gates, daemon archive path, identity verification, remote wire, shell PTYs, archive transaction semantics, test build injection) that cycle 1 flagged and revision acknowledged but did not fix.
- **15-04** has **actionable gaps** (promised test coverage omitted, footer geometry undefined, enum adapters pseudocode only) that block execution but are not fundamental design issues.

**Recommendation**: Do not advance to execution. Schedule a day-long design session with the planner to:

1. Redesign PTY state machine (spawn → register → pause/resume → admit) with `PendingPty` or similar.
2. Establish startup loop sequencing: first frame before restore, restore queue order, and explicit `RestoreStep` state machine.
3. Define metadata cache keys per actual source file/entity with invalidation rules.
4. Complete daemon archive and idle-policy paths in both manager and API.
5. Convert 15-04's promised tests into actual task actions with coverage checklist.

Cycle 3 should be scheduled after design revision. Plans are well-intentioned and identify the right problems, but executable detail and safety-critical invariants are still missing.

---

Generated with Claude Code

Co-Authored-By: iArx Claude Code <claude-code@iarx.com>
