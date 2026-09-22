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

Generated with Claude Code

Co-Authored-By: iArx Claude Code <claude-code@iarx.com>
