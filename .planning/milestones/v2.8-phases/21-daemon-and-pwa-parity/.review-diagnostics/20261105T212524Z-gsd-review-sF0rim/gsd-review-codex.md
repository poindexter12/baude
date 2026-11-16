Source inspection covered all six plans. This is a static review; I did not run the Rust build or test suites in the read-only workspace.

## 21-01

**Summary**

The daemon wrapper follows the right ownership boundary, but two implementation details need correction: saving the core candidate can overwrite newer metadata on surviving sessions, and the proposed fixtures do not reliably exercise live runtimes.

**Strengths**

- Reusing core preserves the existing kill → disk → persistence → clearing sequence. Core retains all supplied handles on failure and consumes them after commit, matching the proposed custody wrapper. See [removal.rs:407](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/removal.rs:407) and [removal.rs:436](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/removal.rs:436).
- Recomputing the report before execution matches the TUI’s existing comparison of rows, runtimes and deleted paths. See [app.rs:4015](/Users/joese/Code/github.com/poindexter12/baude/baude/src/app.rs:4015).

**Concerns**

- **HIGH — Removal can overwrite surviving sessions’ saved metadata.** Ordinary daemon saves call `state_for_save()`, which copies current runtime metadata into the persisted state. Core removal instead clones its supplied `RepositoryState` and saves that candidate directly. For example, archiving session B changes its runtime and saves the merged projection; subsequently removing A through the proposed closure can persist B’s older archive value. See [manager.rs:719](/Users/joese/Code/github.com/poindexter12/baude/bauded/src/manager.rs:719), [manager.rs:779](/Users/joese/Code/github.com/poindexter12/baude/bauded/src/manager.rs:779), [manager.rs:2469](/Users/joese/Code/github.com/poindexter12/baude/bauded/src/manager.rs:2469), and [removal.rs:445](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/removal.rs:445).
- **HIGH — Bare `sleep 30` is an invalid live-runtime fixture.** The daemon resolves permission flags before spawning, and the default resolver appends `--dangerously-skip-permissions`. Bare `sleep` rejects that argument and exits. The existing Docker smoke test explicitly documents this problem and uses a shell wrapper. A nonzero persisted PID and a listed session do not establish liveness. See [manager.rs:1231](/Users/joese/Code/github.com/poindexter12/baude/bauded/src/manager.rs:1231), [permission.rs:98](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/permission.rs:98), and [ci.yml:78](/Users/joese/Code/github.com/poindexter12/baude/.github/workflows/ci.yml:78).
- **MEDIUM — The custom save path omits persistence-status bookkeeping.** The plan updates `persistence_dirty` on success but does not specify maintaining `persistence_error` on failure or clearing it after recovery. `/info` exposes both fields, while ordinary `save_checked()` maintains them together. See [manager.rs:729](/Users/joese/Code/github.com/poindexter12/baude/bauded/src/manager.rs:729) and [manager.rs:752](/Users/joese/Code/github.com/poindexter12/baude/bauded/src/manager.rs:752).

**Suggestions**

- Merge current survivor metadata into the removal candidate while preserving core’s rollback behavior.
- Add a regression test: archive B, remove A, reload the state, and verify B remains archived.
- Use an argument-tolerant command double and assert the owned process is alive before execution and gone afterward.
- Specify persistence-status updates for failure, recovery and `SavedDegraded`.

**Risk Assessment: HIGH.** The proposed persistence adapter can regress unrelated rows, and the tracer’s live-process proof is currently unreliable.

## 21-02

**Summary**

The hierarchy projection is necessary, but this plan contains a direct compatibility-test contradiction and assumes daemon row classifications and standalone support that the source does not provide.

**Strengths**

- A separate hierarchy projection fits the actual durable model: repository parents, checkout children and standalone rows already have distinct core identities. See [repository.rs:336](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/repository.rs:336), [repository.rs:348](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/repository.rs:348), and [repository.rs:453](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/repository.rs:453).
- Leaving suppression writes in core avoids duplicating the surviving-parent rule. Core already adds entries only for removed checkouts whose repository remains. See [removal.rs:752](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/removal.rs:752).

**Concerns**

- **HIGH — The unchanged compatibility test cannot pass.** It explicitly includes `checkout_key` in its forbidden-field list. Adding that field to every `SessionInfo` contradicts the plan’s requirement to preserve the test unchanged. See [api.rs:1002](/Users/joese/Code/github.com/poindexter12/baude/bauded/src/api.rs:1002).
- **HIGH — Real standalone parity is missing.** A persisted plain folder session currently goes through `record_checkout_intent()`, which creates a repository and checkout even when Git discovery fails. Manager has only a checkout runtime registry, and restore iterates checkouts. Seeding an inactive standalone directly in a test proves removal of synthetic state, but does not make actual daemon folder sessions reachable as standalone rows. A standalone with persisted ownership also requires a runtime handle at core preflight. See [manager.rs:879](/Users/joese/Code/github.com/poindexter12/baude/bauded/src/manager.rs:879), [manager.rs:1073](/Users/joese/Code/github.com/poindexter12/baude/bauded/src/manager.rs:1073), [manager.rs:128](/Users/joese/Code/github.com/poindexter12/baude/bauded/src/manager.rs:128), and [removal.rs:862](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/removal.rs:862).
- **MEDIUM — The role and branch projection is underspecified.** Plain creation currently assigns `CheckoutRole::ManagedBranch`, including for a main checkout. Core roles are `Main`, `PrimaryDefault` and `ManagedBranch`; observed branches can contain `refs/heads/topic`. The tests expect `"main"`, `"worktree"` and `"topic"` without defining the normalization or admission correction. See [manager.rs:1110](/Users/joese/Code/github.com/poindexter12/baude/bauded/src/manager.rs:1110), [repository.rs:284](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/repository.rs:284), and [lifecycle.rs:2163](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/lifecycle.rs:2163).
- **MEDIUM — Recreating a branch does not necessarily exercise suppression lifting.** Managed paths include the newly allocated checkout key. Removing and recreating `"topic"` normally produces a different path, while suppression matches exact paths. Checking that the new path is unsuppressed can pass without lifting anything. See [git.rs:1823](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/git.rs:1823), [lifecycle.rs:2078](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/lifecycle.rs:2078), and [suppression.rs:73](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/suppression.rs:73).

**Suggestions**

- Amend the compatibility test deliberately: permit the additive identity field while retaining assertions against hierarchy nesting.
- Add production standalone admission and runtime custody, with an HTTP-created live folder fixture.
- Define projection normalization and correct main-checkout classification explicitly.
- Prove suppression lifting by seeding the exact subsequently admitted path; distinguish it from branch recreation at a fresh path.

**Risk Assessment: HIGH.** As written, the plan cannot satisfy its unchanged test and does not deliver standalone parity for normal daemon sessions.

## 21-03

**Summary**

The failure-path coverage is useful and appropriately observable. It needs additional cases for irreversible partial progress, committed-but-degraded persistence, and runtime-only drift.

**Strengths**

- The tampered-path test checks the correct boundary: core derives disk actions from durable checkout ownership rather than request paths. See [removal.rs:1073](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/removal.rs:1073).
- A suitable teardown injection seam already exists in Manager, so no public PTY accessor is necessary. See [manager.rs:2525](/Users/joese/Code/github.com/poindexter12/baude/bauded/src/manager.rs:2525).

**Concerns**

- **MEDIUM — Restoring handles does not reverse completed effects.** Core explicitly allows earlier runtimes to be dead after a parent-removal abort. Disk removal also precedes persistence, so a 503 can leave a worktree deleted while its row remains. The planned single-session assertions do not exercise these retry conditions. See [removal.rs:429](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/removal.rs:429), [removal.rs:454](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/removal.rs:454), and the existing core retry fixture at [removal.rs:2822](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/removal.rs:2822).
- **MEDIUM — Committed replacement failure is untested at the daemon boundary.** Core converts an after-replacement save error into successful `SavedDegraded`; it must not become a 503 with restored handles. The proposed tests cover only before-replacement failure. See [removal.rs:1039](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/removal.rs:1039).
- **MEDIUM — Runtime-only drift is not independently tested.** Adding a child exercises row-set drift, and tampering exercises path drift. Neither catches an implementation that omits the runtime comparison when rows and paths remain unchanged. The reference TUI compares all three independently. See [app.rs:4023](/Users/joese/Code/github.com/poindexter12/baude/baude/src/app.rs:4023).

**Suggestions**

- Add a parent fixture where one child stops before another fails, then verify all handles remain addressable and retry succeeds.
- Cover managed-worktree deletion followed by persistence failure and refreshed retry.
- Add `DirectorySync` failure → 200 `SavedDegraded`, plus runtime-only drift → 409.
- Reuse `fail_next_agent_teardown_for_test()`.

**Risk Assessment: MEDIUM.** The basic mappings are sound; the gaps concern the failure transitions most likely to expose adapter mistakes.

## 21-04

**Summary**

The broadcast design fits the existing SSE implementation. The main weaknesses are an overstated ordering guarantee and tests that do not fully establish publication timing or route-level lag behavior.

**Strengths**

- Reusing `async_stream`, axum SSE and keep-alives follows both existing session streams and requires no new stream adapter dependency. See [api.rs:614](/Users/joese/Code/github.com/poindexter12/baude/bauded/src/api.rs:614) and [bauded/Cargo.toml:12](/Users/joese/Code/github.com/poindexter12/baude/bauded/Cargo.toml:12).
- Publishing after core returns successfully includes both normal commit and committed replacement with degraded durability. Core establishes that distinction before returning. See [removal.rs:1039](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/removal.rs:1039).

**Concerns**

- **MEDIUM — Publishing under the lock does not prevent client resurrection.** It orders server mutations, but an earlier GET response can still arrive or be applied after the SSE event. Both current clients replace their snapshots unconditionally. See [app.js:137](/Users/joese/Code/github.com/poindexter12/baude/bauded/web/app.js:137) and [remote.rs:147](/Users/joese/Code/github.com/poindexter12/baude/baude/src/remote.rs:147).
- **MEDIUM — The ordering test can pass with an incorrectly early send.** The plan awaits POST completion before reading the event. An event queued before persistence would still be read after the file changed, satisfying the proposed assertion. See [21-04-PLAN.md:65](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/21-daemon-and-pwa-parity/21-04-PLAN.md:65) and [21-04-PLAN.md:88](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/21-daemon-and-pwa-parity/21-04-PLAN.md:88).
- **MEDIUM — Direct receiver lag testing does not prove the SSE bridge continues.** Seeing `Lagged` followed by `Ok` on a broadcast receiver tests channel behavior. The route’s own error-handling loop must be driven through lag. See [21-04-PLAN.md:89](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/21-daemon-and-pwa-parity/21-04-PLAN.md:89).

**Suggestions**

- State the lock guarantee narrowly and require stale-response protection in both clients.
- Observe the receiver while an injected save boundary remains incomplete.
- Overflow an actual `/events` subscription before reading its body.
- Verify `SavedDegraded` emits exactly one event.

**Risk Assessment: MEDIUM.** The mechanism is straightforward, but current tests overstate the ordering properties they prove.

## 21-05

**Summary**

Separating pure report transformations from browser wiring is useful. The plan needs stronger handling and tests for asynchronous refreshes, newly introduced confirmation rows, and actual DOM behavior.

**Strengths**

- The view model accounts for the real report fields, including `already_removed`, `cleared` and `clearing_failures`. These have distinct semantics in core and should remain distinct in the confirmation. See [removal.rs:180](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/removal.rs:180).
- Asset routing, script ordering and service-worker registration are included. They are necessary because assets are explicitly embedded and the shell list is explicit. See [web.rs:27](/Users/joese/Code/github.com/poindexter12/baude/bauded/src/web.rs:27), [index.html:20](/Users/joese/Code/github.com/poindexter12/baude/bauded/web/index.html:20), and [sw.js:5](/Users/joese/Code/github.com/poindexter12/baude/bauded/web/sw.js:5).

**Concerns**

- **HIGH — A pending refresh can restore removed rows.** `refresh()` assigns the fetched session array directly. Adding `/rows` alongside it does not coordinate those responses with SSE or execute success. A pre-removal response applied afterward can repopulate the list until another poll. See [app.js:137](/Users/joese/Code/github.com/poindexter12/baude/bauded/web/app.js:137).
- **HIGH — A 409 report can contain children absent from the name lookup.** `RemovalReport` carries row keys, without names or branches. Re-rendering a fresh report against stale `state.rows` cannot list newly added children by name and branch as required. See [removal.rs:180](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/removal.rs:180) and the planned re-render at [21-05-PLAN.md:96](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/21-daemon-and-pwa-parity/21-05-PLAN.md:96).
- **MEDIUM — Pure-data tests do not validate DOM escaping or controller behavior.** Preserving `<` in a view-model value does not prove the renderer escapes it. Existing cards are also outer `<button>` elements, so placing a remove button inside them would create invalid nested controls or conflicting navigation. See [app.js:33](/Users/joese/Code/github.com/poindexter12/baude/bauded/web/app.js:33), [app.js:612](/Users/joese/Code/github.com/poindexter12/baude/bauded/web/app.js:612), and [app.js:641](/Users/joese/Code/github.com/poindexter12/baude/bauded/web/app.js:641).
- **MEDIUM — Local success races with “removed elsewhere.”** The daemon publishes before the POST completes, so the initiating tab may receive its own event first. The planned unconditional modal-close notification can misclassify its own operation; repeated confirmation and late responses are unspecified. See [21-05-PLAN.md:114](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/21-daemon-and-pwa-parity/21-05-PLAN.md:114).
- **MEDIUM — New JS tests are outside the repository gates.** `gate.sh` runs Rust checks and shell guards, while the plan also uses source greps as verification despite the repository explicitly forbidding source-grep assertions. See [gate.sh:63](/Users/joese/Code/github.com/poindexter12/baude/scripts/gate.sh:63), [gate.sh:107](/Users/joese/Code/github.com/poindexter12/baude/scripts/gate.sh:107), and [CLAUDE.md:24](/Users/joese/Code/github.com/poindexter12/baude/CLAUDE.md:24).

**Suggestions**

- Invalidate pending refresh generations on removal; discard older responses and launch reconciliation.
- Refresh label metadata after 409 while keeping destructive scope authoritative from the report.
- Use sibling navigation/action controls and test their event behavior.
- Track an in-flight confirmation, disable duplicate submits, and make POST/SSE completion idempotent.
- Add controller/rendering tests for these races and escaping, and run the JS suite in CI with Node provisioned.

**Risk Assessment: HIGH.** The happy path is specified well, but normal request ordering can violate live removal and confirmation completeness.

## 21-06

**Summary**

Reusing the modal is appropriate, but the file inventory is incomplete, the inherited timeout is too short for some valid removals, and the test double conflicts with the poller it intends to test.

**Strengths**

- The existing App tick already detects snapshot changes, reconciles selection and closes an attach whose session disappeared. Updating the shared remote snapshot can reuse this path. See [app.rs:4888](/Users/joese/Code/github.com/poindexter12/baude/baude/src/app.rs:4888) and [app.rs:4926](/Users/joese/Code/github.com/poindexter12/baude/baude/src/app.rs:4926).
- Typed handling of 409 and preservation of daemon error bodies improves on the current client’s status-only error reporting. See [remote.rs:271](/Users/joese/Code/github.com/poindexter12/baude/baude/src/remote.rs:271).

**Concerns**

- **HIGH — `ui.rs` is a required missing file.** Its modal pattern explicitly enumerates every field, calls both label helpers with their current signatures, and constructs the modal in tests. Adding `origin` and changing helper arguments requires updating these sites; the listed two-file change will not compile unchanged. See [ui.rs:1776](/Users/joese/Code/github.com/poindexter12/baude/baude/src/ui.rs:1776) and [ui.rs:3818](/Users/joese/Code/github.com/poindexter12/baude/baude/src/ui.rs:3818).
- **HIGH — The three-second request timeout is insufficient.** Core stops rows sequentially; each session can stop both agent and shell, and each PTY reader join can take two seconds. Valid removal can therefore outlast `REQUEST_TIMEOUT`, producing a client error while the daemon continues and potentially commits. See [remote.rs:17](/Users/joese/Code/github.com/poindexter12/baude/baude/src/remote.rs:17), [removal.rs:916](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/removal.rs:916), [session.rs:495](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/session.rs:495), and [pty.rs:901](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/pty.rs:901).
- **HIGH — Polling can overwrite an SSE drop.** The poller fetches sessions, fetches `/info`, then replaces the complete snapshot. A removal event arriving between those operations can be undone by the already-fetched session array. See [remote.rs:147](/Users/joese/Code/github.com/poindexter12/baude/baude/src/remote.rs:147) and [remote.rs:176](/Users/joese/Code/github.com/poindexter12/baude/baude/src/remote.rs:176).
- **MEDIUM — The scripted HTTP double races background polling.** `start_without_events()` still starts the poller, whose first requests are GET `/sessions` and `/info`. Those can consume a script expecting POST `/removals/preview`. The single base URL also provides no specified way to point polling at a refusing port while SSE uses the double. See [remote.rs:139](/Users/joese/Code/github.com/poindexter12/baude/baude/src/remote.rs:139) and [21-06-PLAN.md:68](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/21-daemon-and-pwa-parity/21-06-PLAN.md:68).
- **MEDIUM — Subscriber lifetime is asserted but not implemented.** `RemotePoller` currently has no shutdown token or owned thread handle; its poll thread loops indefinitely. A second thread with an unlimited blocking read does not automatically share the object’s lifetime. See [remote.rs:133](/Users/joese/Code/github.com/poindexter12/baude/baude/src/remote.rs:133) and [remote.rs:145](/Users/joese/Code/github.com/poindexter12/baude/baude/src/remote.rs:145).

**Suggestions**

- Add `ui.rs` and its modal fixtures to the plan.
- Define a removal-specific timeout and reconcile ambiguous timeout outcomes through SSE or a fresh daemon read.
- Reject stale poll results after an event or successful removal.
- Provide a client constructor with background workers disabled; test polling and streaming separately.
- Add explicit subscriber shutdown and a bounded way to interrupt stalled reads.
- Include `checkout_key` in `RemoteInfo::PartialEq`, which currently controls meaningful snapshot-change detection at [remote.rs:80](/Users/joese/Code/github.com/poindexter12/baude/baude/src/remote.rs:80).

**Risk Assessment: HIGH.** Missing renderer changes block compilation; timeout and snapshot races affect real removal behavior.

## Cross-plan assessment

The shared-core architecture is sound, but the plans need revision before execution. The highest-priority fixes are:

1. Preserve surviving runtime metadata during removal saves.
2. Resolve the compatibility-test contradiction and implement real standalone admission/custody.
3. Replace invalid live-runtime fixtures.
4. Coordinate polling with SSE in both clients.
5. Complete the TUI renderer changes and removal timeout behavior.

The final repository gates also need to depend on **both** client plans. Currently 21-05 and 21-06 share wave 5, while the full gates live inside 21-06; they can finish before the PWA changes land.

Finally, update the scope documents: [REQUIREMENTS.md:82](/Users/joese/Code/github.com/poindexter12/baude/.planning/REQUIREMENTS.md:82) excludes remote TUI removal, while [21-CONTEXT.md:32](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/21-daemon-and-pwa-parity/21-CONTEXT.md:32) explicitly includes it. The newer decision supports proceeding, but the contradiction should be removed.

**Overall risk: HIGH until these concrete gaps are addressed.**
