---
phase: 18
reviewers: [codex, opencode, qwen, cursor, antigravity, ollama]
convergence_cycle: 2
cycle_1_commit: 0070c74
plans_commit: fb84cf5
reviewed_at: 2026-11-02T17:32:10Z
plans_reviewed: [18-01-PLAN.md, 18-02-PLAN.md, 18-03-PLAN.md, 18-04-PLAN.md, 18-05-PLAN.md, 18-06-PLAN.md, 18-07-PLAN.md, 18-08-PLAN.md]
models:
  codex: "gpt-6.1-sol (reasoning=high)"
  opencode: "unknown"
  qwen: "github-copilot/claude-sonnet-4.6 via opencode"
  cursor: "unknown"
  antigravity: "unknown"
  ollama: "qwen3:8b"
model_sources:
  codex: "banner"
  opencode: "unknown"
  qwen: "reviewer tag in output"
  cursor: "unknown"
  antigravity: "unknown"
  ollama: "requested"
lane_status:
  codex: "reviewed"
  opencode: "skipped: OpenCode returned no assistant text (agent ended its turn with no final message, #1936)"
  qwen: "skipped: lane ran but emitted only the discount marker and a reviewer tag, no review body (cycle 2 stderr: Copilot rejected model claude-sonnet-4.6 for the opencode integrator)"
  cursor: "reviewed"
  antigravity: "skipped: headless run auto-denied the read_file permission, no review recovered"
  ollama: "skipped: HTTP request to the local qwen3:8b server aborted on timeout"
plan_coverage:
  qwen:
    total: 8
    missing: ["18-01", "18-02", "18-03", "18-04", "18-05", "18-06", "18-07", "18-08"]
---

# Cross-AI Plan Review — Phase 18

<!-- gsd:plan-revision-conflicts:begin -->
## Plan-Revision Conflicts
<!-- gsd:plan-revision-conflicts:end -->

## Codex Review

Read-only source review completed for all **7 plans**. No files were modified. Tests and gates were not run; findings below come from tracing the current source and proposed implementation.

## 18-01

**Summary:** Shared selection and preview are a sound foundation, but the dependency declaration prevents independent compilation, and the runtime model loses information needed for an accurate confirmation.

**Strengths**

- The target model matches the existing aggregate: checkouts reference repository keys, while standalone rows have separate identities. This supports one shared selection function. Evidence: [repository.rs:348](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/repository.rs:348), [repository.rs:453](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/repository.rs:453).
- Giving `Main` precedence over `managed_by_baude` correctly preserves the checkout directory even if its management flag is inconsistent. Both facts already exist independently on the saved row. Evidence: [repository.rs:350](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/repository.rs:350).

**Concerns**

- **HIGH — Undeclared compilation dependency.** `RemovalError` directly references `git::ForcedRemovalError`, supplied by 18-02, while 18-01 declares no dependency. The suggested type alias still references the missing type and cannot solve this. Evidence: [18-01-PLAN.md:92](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/18-shared-kill-then-remove-lifecycle/18-01-PLAN.md:92); the current Git removal implementation contains no such type at [git.rs:2710](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/git.rs:2710).
- **MEDIUM — Preview discards shell ownership.** `PlannedRow.runtime` retains only the agent, and preview always reports `shell_pid: None`. Existing `OwnedRuntime` explicitly records an owned shell, and removal stops both PTYs. The confirmation therefore cannot describe the complete runtime action. Evidence: [repository.rs:98](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/repository.rs:98), [session.rs:478](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/session.rs:478).
- **MEDIUM — The specified identity expression does not compile.** `owned_runtime().map(|r| r.agent)` moves from a borrowed value; `ProcessIdentity` implements `Clone`, not `Copy`. Evidence: [repository.rs:60](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/repository.rs:60), [repository.rs:402](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/repository.rs:402).

**Suggestions**

- Make 18-01 depend on 18-02, or introduce the Git-dependent error variant in 18-04.
- Carry the complete `OwnedRuntime`, including shell identity and generation; clone borrowed identities.
- Distinguish a missing directory from an inspection error instead of treating every negative `exists()` result as “already removed.”

**Risk Assessment: MEDIUM.** The selection design is manageable, but its API and dependency issues should be corrected before downstream implementation.

## 18-02

**Summary:** Containment and postcondition checks are valuable, but containment alone is insufficient authority for recursive deletion. The fallback can delete a replacement main checkout or another repository inside the managed root.

**Strengths**

- The redirected root layout is correct: `TestRedirect` supplies `<root>/data`, and `worktrees_base()` appends `baude/worktrees`. Evidence: [testing.rs:128](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/testing.rs:128), [git.rs:1788](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/git.rs:1788).
- Checking both directory absence and Git inventory follows the existing verified-removal approach, which independently checks these postconditions. Evidence: [git.rs:2635](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/git.rs:2635).
- Double force is appropriate for the resolved locked-worktree decision. Git explicitly requires it for locked worktree removal. Evidence: [18-02-PLAN.md:87](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/18-shared-kill-then-remove-lifecycle/18-02-PLAN.md:87), [Git worktree documentation](https://git-scm.com/docs/git-worktree#Documentation/git-worktree.txt---force).

**Concerns**

- **HIGH — The fallback bypasses repository identity and main-checkout protection.** Any Git refusal triggers `remove_dir_all` after a root-prefix check. A stale managed path can now contain another repository or a main checkout. Existing removal explicitly checks common-directory identity, exact worktree identity, and main-worktree status; the new helper accepts no expected common-directory identity. Evidence: [18-02-PLAN.md:89](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/18-shared-kill-then-remove-lifecycle/18-02-PLAN.md:89), [git.rs:2520](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/git.rs:2520). Bypassing cleanliness does not require dropping these ownership checks.
- **HIGH — Root equality and in-root aliases remain dangerous.** `starts_with(root)` accepts the root itself and arbitrary ancestor directories beneath it. Canonicalizing an in-root symlink also changes the deletion target to its destination. The proposed checks do not establish that this destination is the selected managed worktree. Evidence: [18-02-PLAN.md:88](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/18-shared-kill-then-remove-lifecycle/18-02-PLAN.md:88); production worktrees occupy deeper workspace/repository/check-out paths at [git.rs:1802](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/git.rs:1802).
- **MEDIUM — Branch snapshots will reject successful deletion.** Raw `git branch --list` includes `+` for branches checked out in linked worktrees. Removing the worktree legitimately changes that marker while preserving every ref. The planned equality assertion is therefore incorrect. Evidence: [18-02-PLAN.md:75](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/18-shared-kill-then-remove-lifecycle/18-02-PLAN.md:75), [Git branch documentation](https://git-scm.com/docs/git-branch#_description).
- **LOW — The refactor verification command is invalid.** `cargo test` accepts one positional filter, but the command supplies two. Evidence: [18-02-PLAN.md:169](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/18-shared-kill-then-remove-lifecycle/18-02-PLAN.md:169).

**Suggestions**

- Preserve fresh repository ownership and non-main-worktree checks while bypassing only cleanliness and lock restrictions.
- Reject the root, ancestor targets, and ambiguous aliases before deletion. Add fixtures for replacement repositories, replacement main checkouts, and symlinks.
- Compare sorted `refs/heads` names and OIDs rather than decorated branch output.
- Run the two Cargo filters separately.

**Risk Assessment: HIGH.** The proposed fallback can violate the phase’s strongest data-preservation requirement.

## 18-03

**Summary:** This is a suitably small aggregate mutation. Its main problems are executable test instructions, rather than the partitioning approach.

**Strengths**

- Batch mutation followed by validation addresses the actual parent-child invariant: validation rejects a checkout whose repository is absent. Evidence: [repository.rs:941](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/repository.rs:941).
- Preserving allocation counters is correct. Validation requires counters to remain above surviving keys and orders; removal should not recycle identities. Evidence: [repository.rs:1018](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/repository.rs:1018).

**Concerns**

- **MEDIUM — The prescribed builders cannot create the proposed valid topology unchanged.** Every `repository()` has the same common directory, and every `checkout()` has the same path and `PrimaryDefault` role. Two repositories or multiple children built this way violate identity, ownership, or unique-role checks. Evidence: [repository.rs:1135](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/repository.rs:1135), [repository.rs:1146](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/repository.rs:1146), [repository.rs:925](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/repository.rs:925), [repository.rs:976](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/repository.rs:976).
- **LOW — The RED count is impossible as written.** A stub that returns empty results without mutation already satisfies the unknown-key no-op test. Expecting four failures conflicts with that behavior. Evidence: [18-03-PLAN.md:94](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/18-shared-kill-then-remove-lifecycle/18-03-PLAN.md:94), [18-03-PLAN.md:103](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/18-shared-kill-then-remove-lifecycle/18-03-PLAN.md:103).

**Suggestions**

- Parameterize fixture repository identity, checkout path, role, and retained session paths; validate the initial state.
- Require meaningful mutation tests to fail during RED, while allowing already-correct no-op behavior to pass.

**Risk Assessment: LOW.** The production change is straightforward once fixture construction and acceptance criteria are repaired.

## 18-04

**Summary:** The stage ordering and single-save design are appropriate, but the runtime ownership check and assumptions about the existing teardown primitive do not establish KILL-01/02.

**Strengths**

- Calling `Session::kill_and_wait` directly avoids the existing wrapper’s mutation on failure. `destructive_teardown` calls `mark_teardown_pending`, which changes durable row fields. Evidence: [lifecycle.rs:987](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/lifecycle.rs:987), [lifecycle.rs:938](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/lifecycle.rs:938).
- Committing the clone after successful replacement, including degraded post-rename outcomes, matches the persistence API’s explicit distinction. Evidence: [persist.rs:278](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/persist.rs:278), [persist.rs:681](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/persist.rs:681).

**Concerns**

- **HIGH — PID-only handle matching does not prove ownership.** The preflight compares only the agent PID. A replacement handle with the same numeric PID can pass; `Pty` then verifies that handle’s own identity, not the persisted identity. Persisted shell ownership is also unchecked, so a handle with a missing shell can leave that shell alive. Evidence: [18-04-PLAN.md:94](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/18-shared-kill-then-remove-lifecycle/18-04-PLAN.md:94), [repository.rs:58](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/repository.rs:58), [pty.rs:767](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/pty.rs:767).
- **HIGH — An exited leader can bypass process-group cleanup.** `Pty::kill_and_wait` returns success immediately when `try_wait()` reports the direct child exited. It never checks whether descendants remain in the recorded group on that path. Reusing it verbatim therefore does not establish the no-orphan guarantee. Evidence: [pty.rs:759](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/pty.rs:759). The planned continuously live leader fixture does not exercise this case.
- **HIGH — PTY and reader closure is not established.** `RowRuntime` borrows `Session`, so successful removal leaves its master and writer owned by the caller. Reader threads are detached; teardown neither closes these fields nor joins the reader. Evidence: [pty.rs:79](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/pty.rs:79), [pty.rs:140](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/pty.rs:140), [pty.rs:744](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/pty.rs:744).
- **MEDIUM — Live fixtures are not save-valid.** They combine `Inactive` with `Some(OwnedRuntime)`. `validate()` misses this, but initial persistence rejects it through `validate_for_save()`. The survivor tests can fail before invoking removal. Evidence: [18-04-PLAN.md:82](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/18-shared-kill-then-remove-lifecycle/18-04-PLAN.md:82), [repository.rs:745](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/repository.rs:745), [persist.rs:636](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/persist.rs:636).
- **MEDIUM — Cleanup after assertions is not panic-safe.** The RED tests intentionally panic before reaching trailing cleanup. `Pty` retains a child handle but has no production `Drop` teardown implementation. Evidence: [18-04-PLAN.md:126](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/18-shared-kill-then-remove-lifecycle/18-04-PLAN.md:126), [pty.rs:140](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/pty.rs:140).

**Suggestions**

- Match complete persisted agent and shell identities; reject duplicate row handles before signalling.
- Add an exited-leader fixture with a surviving descendant, plus a live shell fixture.
- Define and test who closes PTYs, observes reader completion, and removes caller registry entries.
- Seed live rows as `Running(generation)` and require `validate_for_save()` before each test.
- Use teardown guards that execute during unwinding.

**Risk Assessment: HIGH.** Successful return currently can coexist with surviving processes or retained PTY resources.

## 18-05

**Summary:** The persistence tests address the accepted disk-before-save gap well, but they overstate coverage and do not test interrupted parent removal at its most important boundaries.

**Strengths**

- Rename and directory-sync injections exercise genuinely different commit outcomes. The implementation sets `replacement_committed` immediately after rename and before directory sync. Evidence: [persist.rs:678](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/persist.rs:678).
- The proposed missing-worktree recovery is supported: Git reconciliation returns `Missing`, and lifecycle reconciliation records protection. Evidence: [git.rs:488](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/git.rs:488), [lifecycle.rs:629](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/lifecycle.rs:629).

**Concerns**

- **MEDIUM — “Every AtomicFailure branch” is not covered.** Tests exercise Write, Rename, and DirectorySync, but omit Sync and RefusedRow. Those variants exist and represent distinct rejection points. Evidence: [18-05-PLAN.md:155](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/18-shared-kill-then-remove-lifecycle/18-05-PLAN.md:155), [persist.rs:242](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/persist.rs:242).
- **MEDIUM — Crash consistency is tested only for a checkout target.** The parent-specific guarantee needs failures during `Repository(k)` removal, including after one child’s directory disappears and before another child completes. The current crash test removes only `Checkout(c)`. Evidence: [18-05-PLAN.md:108](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/18-shared-kill-then-remove-lifecycle/18-05-PLAN.md:108).
- **MEDIUM — Validation happens after irreversible effects.** The proposed orchestration constructs and validates the candidate only after killing and deleting. Existing code explicitly documents that `validate()` alone is weaker than save validation. Evidence: [18-04-PLAN.md:96](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/18-shared-kill-then-remove-lifecycle/18-04-PLAN.md:96), [repository.rs:661](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/repository.rs:661).

**Suggestions**

- Add Sync and RefusedRow cases, or narrow the coverage claim.
- Add repository-target failure tests before replacement and after partial disk progress; assert exact prior bytes and successful retry.
- Construct and `validate_for_save()` the candidate before signalling, while retaining the locked kill → disk → save side-effect order.
- Describe the write-failure test as a simulated pre-replacement failure; it does not terminate a process mid-operation.

**Risk Assessment: MEDIUM.** The underlying atomic writer is suitable; the remaining weakness is orchestration and recovery coverage.

## 18-06

**Summary:** The cloned topology materially improves realism. However, two assertions conflict with the planned implementation, and several required row-kind/runtime combinations remain uncovered.

**Strengths**

- The cloned fixture reproduces the existing real-user topology: a working branch in the main checkout, a default worktree on `main`, and a managed feature worktree. Evidence: [lifecycle.rs:4284](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/lifecycle.rs:4284).
- Holding `TestRedirect` throughout the fixture lifetime follows the existing isolation mechanism, whose scope is deliberately thread-local. Evidence: [testing.rs:98](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/testing.rs:98).

**Concerns**

- **HIGH — Already-removed cleanup is unreachable.** The test expects stale Git administration to be pruned, but selection changes the action to `AlreadyRemoved`, and `disk_stage` merely records that path. It never calls the helper containing prune. Evidence: [18-06-PLAN.md:114](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/18-shared-kill-then-remove-lifecycle/18-06-PLAN.md:114), [18-04-PLAN.md:95](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/18-shared-kill-then-remove-lifecycle/18-04-PLAN.md:95). Directory absence does not imply inventory absence; that distinction is documented at [git.rs:291](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/git.rs:291).
- **MEDIUM — Branch snapshot equality has the same decoration defect as 18-02.** The snapshot includes raw `branch --list`, whose `+` markers change when linked worktrees disappear. Evidence: [18-06-PLAN.md:111](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/18-shared-kill-then-remove-lifecycle/18-06-PLAN.md:111), [Git branch documentation](https://git-scm.com/docs/git-branch#_description).
- **MEDIUM — The integration matrix is incomplete.** Main and standalone tests have no runtime; unmanaged checkout removal is absent; shell teardown is absent. These are distinct supported representations, including separate standalone lifecycle/runtime validation. Evidence: [18-06-PLAN.md:112](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/18-shared-kill-then-remove-lifecycle/18-06-PLAN.md:112), [repository.rs:771](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/repository.rs:771).

**Suggestions**

- Keep “directory already absent” as a reporting outcome while still executing scoped administrative cleanup and postcondition checks.
- Snapshot sorted ref names and OIDs only.
- Add live main, live standalone, unmanaged checkout, and agent-plus-shell cases. Verify unrelated rows and directories survive.
- Add at least one parent-removal test containing all supported checkout kinds.

**Risk Assessment: HIGH.** The current operation contradicts its missing-directory test, and the matrix leaves meaningful preservation and shutdown paths unproven.

## 18-07

**Summary:** Using both authoritative gates is appropriate. The completion procedure needs to ensure both platforms validate the final tree and allow fixes in the primitives that actually own defects.

**Strengths**

- The Linux gate really runs as UID 1000 and invokes the same gate inside the container. Evidence: [gate.sh:38](/Users/joese/Code/github.com/poindexter12/baude/scripts/gate.sh:38).
- The gate captures test and root-check statuses independently, so a failed suite cannot suppress leakage checking. This matches CI’s serial suite and subsequent root observation. Evidence: [gate.sh:106](/Users/joese/Code/github.com/poindexter12/baude/scripts/gate.sh:106), [ci.yml:52](/Users/joese/Code/github.com/poindexter12/baude/.github/workflows/ci.yml:52).

**Concerns**

- **MEDIUM — Linux fixes invalidate the earlier macOS result.** Task 2 permits edits after Task 1 passes, but does not require rerunning the host gate. Both gates therefore may not have passed on the final tree. Evidence: [18-07-PLAN.md:101](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/18-shared-kill-then-remove-lifecycle/18-07-PLAN.md:101).
- **MEDIUM — A documentation follow-up cannot repair a bad earlier commit message.** The gate checks every commit in `origin/main..HEAD`; an additional note leaves the offending commit in that range. Evidence: [18-07-PLAN.md:76](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/18-shared-kill-then-remove-lifecycle/18-07-PLAN.md:76), [gate.sh:68](/Users/joese/Code/github.com/poindexter12/baude/scripts/gate.sh:68), [check-commit-message.sh:73](/Users/joese/Code/github.com/poindexter12/baude/scripts/check-commit-message.sh:73).
- **MEDIUM — The permitted fix scope excludes the shutdown primitive.** The plan restricts fixes to four files, but process-group termination and reader ownership reside in `pty.rs` and `session.rs`. A real primitive defect cannot safely be repaired solely in fixture or orchestration code. Evidence: [18-07-PLAN.md:69](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/18-shared-kill-then-remove-lifecycle/18-07-PLAN.md:69), [pty.rs:744](/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/pty.rs:744).

**Suggestions**

- Require both gates to pass on the same final revision; rerun affected validation after any fix.
- Define an actual history-correction or blocking handoff procedure for earlier invalid commit messages.
- Permit justified, reviewed fixes in the process primitives.
- Treat commit counts and source-grep counts as workflow checks, not evidence of behavioral correctness.

**Risk Assessment: MEDIUM.** The gates are useful, but the proposed completion rules can overstate what they validated.

## Cross-plan assessment

**Overall risk: HIGH. Changes are needed before execution.** The principal blockers are:

1. Preserve repository and path ownership checks before fallback deletion.
2. Match complete runtime ownership and close the exited-leader/process-group gap.
3. Define PTY and reader-thread cleanup ownership.
4. Execute Git administrative cleanup for already-missing directories.
5. Repair compilation ordering, fixture lifecycle states, and branch snapshots.

The RED acceptance criteria also need a consistency pass: 18-01’s unconditional `UnknownTarget` stub already passes its unknown-target test, and 18-03’s no-op stub already passes its no-op test. Requiring every new test to fail would reward breaking correct behavior. See [18-01-PLAN.md:122](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/18-shared-kill-then-remove-lifecycle/18-01-PLAN.md:122) and [18-03-PLAN.md:94](/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/18-shared-kill-then-remove-lifecycle/18-03-PLAN.md:94).

The accepted single-save design can remain. These corrections concern deletion authority, shutdown guarantees, and executable validation; they do not require introducing an intent journal or pulling later UI/API phases into Phase 18.

---

## OpenCode Review

_Lane skipped: no review was produced. Diagnostic output preserved verbatim below; this is not a review._

opencode review failed or returned empty output. stderr:

Raw response body:
OpenCode review returned no assistant text (#1936: agent ended its turn with no final message).
Diagnostic: stop reason=?, output tokens=?
Diagnosis: opencode ran with no effort argument, so the reviewer CLI's own configuration applied and exited with status 1.

---

## Qwen Review

_Lane skipped: the CLI ran and returned exit 0, but the only output was the discount marker and a reviewer tag below. No plan was reviewed and no findings exist for this lane; it contributes nothing to the consensus._

> [reviewed-without-source-citations] This reviewer declared source-grounded evidence but cited no file:line source evidence, so it reviewed the pasted plan text only — down-weight its verdict in the Consensus Summary.

[reviewer: github-copilot/claude-sonnet-4.6 via opencode (invoked as 'qwen')]

---

## Cursor Review

## 18-01

### Summary
Solid TDD foundation for the shared removal vocabulary and pure `select_rows` / `preview_removal` path. Classification correctly keys off `CheckoutRole` plus `managed_by_baude` (matching how `topology_blockers` gates management today at `baude-core/src/git.rs:2517-2518`), and the module slot between `pty` and `repository` in `baude-core/src/lib.rs:20-21` is free. The main execution hazard is a false claim of wave-1 independence from plan 02’s `ForcedRemovalError`.

### Strengths
- Disk-action table matches real row facts: `CheckoutRole::{Main, PrimaryDefault, ManagedBranch}` at `repository.rs:284-288`, `managed_by_baude` on `SavedCheckout` at `repository.rs:352`, standalone keep at `repository.rs:453-461`.
- Unknown-target and no-side-effect preview tests match the Phase 20/21 confirmation need and RMV-05.
- `ProcessIdentity` already derives `Serialize`/`Deserialize` (`repository.rs:60-68`), so report serde round-trips are realistic.
- Explicit prohibition on `remove_dir_all` / `Command` / `kill_and_wait` in this plan keeps wave-1 selection pure.

### Concerns
- **HIGH** — `RemovalError::DiskRemoval { error: crate::git::ForcedRemovalError }` cannot compile until plan 02 lands that type (`remove_managed_worktree_forced` / `ForcedRemovalError` are absent from `git.rs` today). Plan 01 is `depends_on: []` and only edits `removal.rs` + `lib.rs`; the suggested local alias still names `crate::git::ForcedRemovalError` and does not help. Parallel wave-1 RED will not build.
- **MEDIUM** — Preview always sets `KilledRuntime.shell_pid: None` from agent-only `owned_runtime().map(|r| r.agent)`, while `OwnedRuntime.shell` can be `ShellOwnership::Owned` (`repository.rs:91-102`) and `Session::kill_and_wait` stops the shell PTY too (`session.rs:478-485`). Phase 20 confirmations built from preview will understate shell kill.
- **LOW** — Cited bands for builders (`1147-1175`) and `SavedCheckout` (`366-410`) drift; builders start ~`1135`, `set_owned_runtime` is at `446-448`. Semantics still match.

### Suggestions
- Make `18-01` `depends_on: [18-02]`, or have plan 01 add a minimal `ForcedRemovalError` stub in `git.rs` before the enum variant, or defer the `DiskRemoval` variant until plan 04.
- Carry optional shell identity into `PlannedRow` / preview `runtimes_killed` from `OwnedRuntime.shell`.

### Risk Assessment
**MEDIUM** — Design is sound; compile-order coupling with 02 is the blocker for autonomous wave-1.

---

## 18-02

### Summary
Well-scoped force-removal primitive beside `remove_verified_worktree` (`git.rs:2710-2714`), with containment before any `remove_dir_all`, double `--force` for locked trees, prune fallback, and branch-untouched assertions. This correctly upgrades RESEARCH’s single-`--force` sketch and matches git’s locked-worktree rules.

### Strengths
- Containment-first order mirrors the critical trust boundary; `worktrees_base()` under `TestRedirect` resolves to `<root>/data/baude/worktrees` (`testing.rs:128-131`, `git.rs:1788-1792`).
- Reuses real fixtures: `GitFixture` / `linked_worktree` (`git.rs:2764+`, `2934-2952`) and `worktree_inventory` (`git.rs:299-319`).
- Argument vector ends with `--` like `verified_remove_arguments` (`git.rs:2614-2620`), blocking option-injection via path.
- Every case asserts branch OID / `branch --list` stability (RMV-07).
- Outside-root refusal test is the right negative for T-18-04 / T-18-07.

### Concerns
- **MEDIUM** — Clean-tree preflight is not inside `remove_verified_worktree` (CONTEXT wording); it lives in `inspect_removal` / `topology_blockers`. The force API is still the right fix, but executors should not expect to “bypass a preflight flag” inside the verified remove function—only to avoid calling the inspect path.
- **LOW** — Fallback `remove_dir_all` after a partially failed `git worktree remove` can race with a locked admin entry; plan mitigates with unlock-then-prune, which matches RESEARCH Open Question 1. Worth watching on Linux gate.
- **LOW** — `ForcedRemoval` does not derive `Serialize` while `ForcedRemovalMethod` does; fine if only nested in non-serde `RemovalError`.

### Suggestions
- Keep postcondition inventory comparison on canonicalized paths (plan already notes macOS `/var` vs `/private/var`).
- Document in the function doc that callers must not use this for Main/unmanaged paths—classification is upstream in `select_rows`.

### Risk Assessment
**LOW** — Highest-leverage disk safety plan; dependencies and fixtures check out against current `git.rs`.

---

## 18-03

### Summary
Narrow, correct aggregate mutation: batch `remove_rows` so plan 04 can clone → mutate → `validate()` → one save. Accurately encodes Pitfall 5: `validate()` rejects dangling children (`DanglingRepositoryKey` at `repository.rs:940-944`) but permits a childless parent.

### Strengths
- Generalizes the only existing primitive, `remove_standalone_session` (`repository.rs:870-879`), without removing it.
- Counters (`next_*` at `repository.rs:510-518`) explicitly untouched—avoids key reuse surprises.
- Parent-only + validate test documents why batch removal exists (SC5 / RMV-06).
- Optional delegation of `remove_standalone_session` through `remove_rows` preserves the `Option` API call sites need.

### Concerns
- **LOW** — Stub RED for the dangling test relies on “remove nothing → validate Ok”; easy to accidentally implement parent-only removal in the stub and invert RED. Acceptance criteria are clear enough.
- **LOW** — `remove_rows` does not call `validate()` internally; safety depends entirely on plan 04’s `persist_stage`. That coupling is intentional but brittle if another caller appears later.

### Suggestions
- Doc-comment that `remove_rows` is unsafe to persist without a subsequent `validate()` (plan already sketches this).
- Consider `debug_assert` or a `remove_rows_validated` wrapper later if non-removal callers appear—out of scope here.

### Risk Assessment
**LOW** — Small surface, matches `validate()` behavior at `repository.rs:937-945`.

---

## 18-04

### Summary
Core orchestration plan: kill via `Session::kill_and_wait` (`session.rs:478-512` → `Pty::kill_and_wait` at `pty.rs:744-794`), disk via plan 02, single save with commit-on-success / `replacement_committed` (`persist.rs:278-280`). Live TERM-trapping fixtures correctly fill the gap that current `kill_and_wait` tests never escalate (plain `sleep 30` only). Deliberate bypass of `LifecycleEngine` / `mark_teardown_pending` matches RESEARCH, not CONTEXT’s older “everything through the engine” line.

### Strengths
- Stage order kill → disk → save matches CONTEXT and avoids Pitfall 3 (cwd-held delete).
- Survivor path uses real `Pty::fail_next_teardown_for_test` (`pty.rs:796-805`, `debug_assertions`), not an unkillable process.
- Handle matching on row key + agent pid, with Pty re-checking full `ProcessIdentity` before signal (`pty.rs:767-772`), addresses pid reuse.
- Parent all-or-nothing at kill stage is explicit; save closure counting pins RMV-06.
- `save_current_at_test` / `SCHEMA_VERSION` / `StateFile` citations match `persist.rs:17`, `473-480`.
- Session literal fields at `session.rs:1463-1484` are complete vs current `Session` (`session.rs:59-111`).

### Concerns
- **HIGH** — `disk_stage` treats `DiskAction::AlreadyRemoved` as bookkeeping only (no `remove_managed_worktree_forced`). RESEARCH locked “stage 2 skips” (`18-RESEARCH.md` Already-removed detection), but plan 02’s missing-dir path is exactly what prunes admin entries. Plan 06 then asserts `git worktree list` no longer lists the path. As written, 04 + 01 + RESEARCH guarantee stale `$GIT_DIR/worktrees` entries for externally deleted trees; 06 will fail or silently drop that assertion.
- **MEDIUM** — Parent kill abort after child 1 succeeds leaves child 1’s process extinct while durable `owned_runtime` and the row remain (`session.rs:478-480` idempotent retry helps, but sidebar/runtime UI can show a live-looking dead row until retry/reconcile). Accepted by CONTEXT; still a sharp edge for Phase 20 error copy.
- **MEDIUM** — Partial disk success then abort (first managed WT deleted, later child `DiskRemoval`) leaves disk ahead of state; plan accepts reconcile/retry. Retry classifies deleted paths as `AlreadyRemoved` and then skips prune—OK if git remove already cleared admin data, bad if fallback only deleted the directory.
- **LOW** — KILL-02 “reader threads closed” is not explicitly done in `kill_and_wait`; the reader exits on EOF after reap (`pty.rs` reader loop). Callers must drop `Session` after success; plan should say so for TUI/daemon.
- **LOW** — `fail_next_teardown_for_test` is `#[cfg(debug_assertions)]` only; fine for default `cargo test` / `gate.sh`, not for a release-mode test run.

### Suggestions
- Change `disk_stage` so `AlreadyRemoved` still calls `remove_managed_worktree_forced` (plan 02 already accepts missing dirs via prune fallback), and keep the report label `already_removed` when the directory was absent pre-call—or stop downgrading in `select_rows` and classify report after the force call.
- Document caller obligations: hold `RepositoryReservations`, drop/unregister `Session` after `Ok`, map `RuntimeSurvived` without tombstone lifecycle writes.

### Risk Assessment
**MEDIUM** — Kill/persist design is strong against code; AlreadyRemoved-vs-prune is a phase-breaking inconsistency with 06.

---

## 18-05

### Summary
Necessary proof of the accepted disk-then-save gap using real `AtomicFailure` injection (`persist.rs:242-251`: `Write`, `Sync`, `Rename`, `DirectorySync`). Maps cleanly onto `replacement_committed()` and existing reconcile helpers.

### Strengths
- Rename failure → prior file bytes + untouched in-memory state; DirectorySync → `SavedDegraded` + committed memory—matches `atomic_save_current` (`persist.rs:665-706`).
- `crash_before_save_reconciles_missing` wires `git::reconcile_checkout` (`git.rs:483-491` → `Missing`) through `lifecycle::record_checkout_reconciliation` (`lifecycle.rs:586-605`) and `unavailable_cause` (`lifecycle.rs:1138-1140`)—proves SC5 on fixture output, not comments.
- Single-save counter for parent-with-children directly guards RMV-06.
- Retry-after-rename-failure expecting `already_removed` is the right user-visible recovery story when the first attempt already deleted the directory via git remove.

### Concerns
- **MEDIUM** — Same AlreadyRemoved skip: after a crash where force remove fully succeeded, retry is fine; after a crash where only `remove_dir_all` ran before prune, retry-as-AlreadyRemoved may leave admin entries. Less likely than the external-delete case but same mechanism.
- **LOW** — `UnavailableCause` citation `repository.rs:170-180` is wrong (`StoppedActiveRecovery` lives there); `UnavailableCause::Missing` is earlier (`repository.rs:107-108` area). Test shape is still correct.
- **LOW** — Task type is `execute` with embedded TDD; fine, but RED/GREEN discipline is softer than plans 01–04.

### Suggestions
- Assert after the reconcile path that `git worktree list` still shows the entry (documenting the gap) or call prune as part of retry—tie to the 04/06 fix.
- Fix line citations for `UnavailableCause` before execution to avoid executor confusion.

### Risk Assessment
**LOW** — Persistence branches match `persist.rs`; depends on 04’s `persist_stage` being implemented as specified.

---

## 18-06

### Summary
Right integration layer for SC3/SC4: cloned topology under the redirected managed root, keep rules for Main/standalone/branches, unclean+cwd-held deletion, parent fan-out. Lifting `cloned_topology` into `testing.rs` (lifecycle’s private copy at `lifecycle.rs:4290-4334` stays put) is the correct reuse move.

### Strengths
- Placing default/managed worktrees under `worktrees_base()` fixes the lifecycle helper’s layout (`lifecycle.rs:4309-4324` uses fixture root paths that would fail containment).
- `branch_snapshot` via `for-each-ref` is stronger than a single branch check (RMV-07).
- Unclean + cwd-held + live trap exercises kill-before-disk and force remove together.
- Parent fan-out asserts report order (children then repository), directories kept/deleted per kind, and one save.

### Concerns
- **HIGH** — `already_removed_managed_directory_drops_row` requires `git worktree list --porcelain` to omit the path after remove, but plans 01/04/RESEARCH skip the force/prune path for `AlreadyRemoved`. Against current code and those plans, this test is red forever or will force a silent plan-04 rewrite mid-wave.
- **MEDIUM** — SC3 text asks both fixture kinds for each rule; Main keep is covered on cloned + git init, but standalone keep is only on a plain folder (fine), and unclean/cwd-held is cloned-only. Acceptable if roadmap SC3 is read as “rules covered across both kinds,” not “every test duplicated.”
- **LOW** — Duplicating `git_in` / topology from lifecycle increases drift risk; prohibition on editing `lifecycle.rs` is good, but a short “keep in sync with lifecycle.rs:4290” comment helps.

### Suggestions
- Resolve AlreadyRemoved with plan 04 before writing this test: call force/prune for missing managed paths, then keep the list assertion.
- Add a git-init unclean managed case if SC3 is interpreted strictly.

### Risk Assessment
**MEDIUM** — Best milestone-facing coverage, blocked on the AlreadyRemoved/prune contradiction.

---

## 18-07

### Summary
Appropriate phase closer: unpiped `scripts/gate.sh` and `scripts/gate.sh --linux` (`scripts/gate.sh:10-15`, `:28-47`), fix-only, no `#[ignore]`. Matches CLAUDE.md and the platform-sensitive kill/procfs paths (`session.rs` Linux `/proc` vs macOS `proc_pidinfo`).

### Strengths
- Redirect-to-log pattern for Linux preserves exit status (unlike piping to `tail`/`grep`).
- Preconditions Docker explicitly; container build failure escalates rather than weakens tests.
- Leak check + `TestRedirect` lifetime align with fixture isolation (`testing.rs` + gate real-roots check).
- Acceptance greps for live-runtime test names tie the Linux gate to KILL-01/03.

### Concerns
- **MEDIUM** — Fully autonomous with `confidence: low` and Docker Desktop required; environment failures become orchestrator blockers with little code recourse.
- **LOW** — Task 1 allows `git commit --amend` for commit-message hook failures; dangerous if HEAD was already pushed—call out “unpushed latest only.”
- **LOW** — Nested-parentheses rule in commit bodies: dynamic `fix(18-07): <gate text>` can accidentally introduce nested `()` from compiler output.

### Suggestions
- Capture gate log paths in the SUMMARY for forensics.
- Prefer follow-up `docs(18-07):` commits over amend when in doubt.

### Risk Assessment
**LOW** for plan quality; **MEDIUM** for execution environment dependence.

---

## Cross-plan comparison

| Topic | Assessment |
|---|---|
| Phase goal coverage | KILL-01/02/03 and RMV-01–07 are traced through 01–06; READM/TUI/API correctly deferred. |
| Dependency graph | 04→01+02+03 and 05→04→06→07 are sound; **01∥02 is not**—01 needs `ForcedRemovalError`. |
| Biggest defect | `AlreadyRemoved` skips prune (01/04/RESEARCH) vs plan 06 list assertion vs plan 02 missing-dir prune capability. |
| LifecycleEngine bypass | Intentional and correct vs `destructive_teardown`/`mark_teardown_pending` (`lifecycle.rs:984-1000`); do not route removal through the engine. |
| Fixture realism | TERM-trap + cloned topology + `AtomicFailure` match the repo’s “assert on fixture output” rule; better than grepping source. |
| Security | Managed-root containment + no branch verbs are the load-bearing controls; preview must not become a path oracle for deletes outside classification. |

## Overall risk assessment

**MEDIUM** overall — Architecture matches the codebase’s real kill, persist, validate, and worktree APIs, and the TDD splits are executable. Two issues will burn autonomous runs if unfixed first: (1) wave-1 compile coupling of `18-01` on `ForcedRemovalError` from `18-02`, and (2) `AlreadyRemoved` skipping `remove_managed_worktree_forced` while `18-06` demands admin-entry prune. Resolve those before execution; remaining risk is mostly Linux-gate environment and the accepted disk-ahead-of-state gap already owned by CONTEXT.

---

## Antigravity Review

_Lane skipped: no review was produced. Diagnostic output preserved verbatim below; this is not a review._

Antigravity review failed or returned empty output.
stderr:
jetski: no output produced — a tool required the "read_file" permission that headless mode cannot prompt for, so it was auto-denied. Add an allow-rule under permissions.allow in settings.json (e.g. read_file(<target>)). Alternatively, re-run with --dangerously-skip-permissions to auto-approve all tools.
An agy session started this run but no review was recovered, so a pre-launch stall is ruled out. See stderr above; a headless run that was auto-denied a tool permission reports the cause and its fix there.

---

## Ollama Review

_Lane skipped: no review was produced. Diagnostic output preserved verbatim below; this is not a review._

ollama review failed or returned empty output. stderr:

Raw response body:
The operation was aborted due to timeout
Diagnosis: ollama is an HTTP lane and carries no reasoning-effort setting.

---

## Consensus Summary

Two grounded reviews ran: Codex (gpt-6.1-sol, reasoning=high) and Cursor. Both cited `file:line` evidence from the live tree and neither carries a `[reviewed-without-source-citations]` or `[reviewed-without-repo-access]` marker, so both count at full weight. OpenCode, Antigravity and Ollama produced no review (stubs above). Qwen returned only a discount marker with no body, so it has no findings to weigh.

The orchestrator's own source-grounding pass (run before the lanes returned) confirms the facts the HIGH findings rest on: `18-01-PLAN.md` declares `depends_on: []` while `RemovalError::DiskRemoval` names `crate::git::ForcedRemovalError`, which does not exist in `baude-core/src/git.rs` until 18-02 creates it; `Pty::kill_and_wait` (`baude-core/src/pty.rs:744-794`) returns `Ok(())` as soon as `try_wait()` reports the leader exited, without inspecting the recorded process group; `Pty` compares the observed identity against its own `self.identity`, not against the persisted `OwnedRuntime`; 18-04's `disk_stage` only records `AlreadyRemoved` paths while 18-06's `already_removed_managed_directory_drops_row` asserts the `git worktree list --porcelain` entry is gone afterward; and 18-02's fallback runs `remove_dir_all` on any non-zero `git worktree remove` exit with managed-root containment as the only guard, whereas the existing verified path (`git.rs:2515-2535`) also checks common-dir identity, exact path, main-worktree and detached state.

Overall: Codex rates the phase **HIGH** risk, Cursor **MEDIUM**. They agree on the two defects that would break autonomous execution outright (wave-1 compile coupling, AlreadyRemoved vs prune). Codex additionally raises four single-reviewer HIGHs on deletion authority and shutdown guarantees that are judgment-class and source-grounded, so they stand.

### Agreed Strengths

- Stage order kill -> disk -> single save matches CONTEXT and the accepted disk-then-save gap; the `replacement_committed()` / `SavedDegraded` split onto `persist.rs` is correct (both reviewers).
- Reusing real primitives rather than inventing new ones: `Session::kill_and_wait`, `Pty::fail_next_teardown_for_test`, `AtomicFailure` injection, `GitFixture::linked_worktree`, `worktree_inventory`, `remove_standalone_session` as the `remove_rows` precedent (both).
- Containment-first ordering and the trailing `--` in the forced-remove argument vector mirror `verified_remove_arguments` and defeat path-as-option injection (both).
- Batch `remove_rows` plus `validate()` correctly encodes Pitfall 5 (dangling children rejected, childless parent allowed) (both).
- Branch-preservation assertions on every disk test and the deliberate bypass of `LifecycleEngine` / `mark_teardown_pending` are the right calls (both).
- 18-07's unpiped gate and redirect-to-log pattern for the Linux gate preserve exit codes per CLAUDE.md (both).

### Agreed Concerns

- **HIGH (18-01, corroborated by both):** `RemovalError::DiskRemoval { error: crate::git::ForcedRemovalError }` cannot compile until 18-02 lands the type, yet 18-01 is `depends_on: []` and runs in wave 1. The suggested local alias still names the missing type. Fix: make 18-01 depend on 18-02, stub `ForcedRemovalError` in 18-01, or move the `DiskRemoval` variant to 18-04.
- **HIGH (18-04 and 18-06, corroborated by both):** `select_rows` downgrades a missing managed directory to `AlreadyRemoved` and `disk_stage` only records it, so `remove_managed_worktree_forced` (the only place the prune fallback lives) is never called. 18-06's `already_removed_managed_directory_drops_row` then demands the `git worktree list --porcelain` entry be gone. As written the test is red forever or forces a mid-wave rewrite of 18-04. Fix: have `disk_stage` still call the forced helper for `AlreadyRemoved` rows (it already tolerates a missing directory) and keep the `already_removed` report label, or drop the list assertion and accept stale admin entries.
- **MEDIUM (18-01, both):** `PlannedRow.runtime` carries only the agent identity and preview always reports `shell_pid: None`, while `OwnedRuntime.shell` can be `ShellOwnership::Owned` and `Session::kill_and_wait` stops the shell PTY too. Phase 20 confirmations built from preview will understate what is killed. Carry optional shell identity into `PlannedRow`.
- **MEDIUM (18-06, both):** the integration matrix is thin: Main and standalone keep-cases have no runtime, no unmanaged-checkout removal, no shell teardown, and unclean/cwd-held is cloned-only (SC3 asks for both fixture kinds).
- **LOW/MEDIUM (18-04, both):** KILL-02 reader-thread and PTY closure is not established by `kill_and_wait`; callers must drop the `Session` after `Ok`. Codex rates this HIGH (see below), Cursor LOW.
- **LOW (18-01, 18-05, both):** several cited line bands drift from the live file (`SavedCheckout` builders, `UnavailableCause`); semantics still match but executors will hunt.
- **LOW (18-07, both):** `git commit --amend` for commit-message hook failures is only safe on an unpushed HEAD, and a follow-up `docs()` commit does not repair an earlier bad message because the hook scans `origin/main..HEAD`.

### Divergent Views

- **PID-only handle matching (18-04).** Codex HIGH: the preflight compares only the agent PID, a replacement handle with the same PID passes, and `Pty` then verifies its own identity rather than the persisted one; persisted shell ownership is unchecked. Cursor lists the same mechanism as a strength ("Pty re-checking full ProcessIdentity before signal addresses pid reuse"). Source agrees with Codex that `Pty` checks `self.identity`, not the row's `OwnedRuntime`; whether that gap matters depends on whether a stale `Session` can be handed in under a reused PID. Worth deciding explicitly in the plan rather than leaving it to the executor.
- **Exited leader bypasses process-group cleanup (18-04).** Codex HIGH, grounded at `pty.rs:759-766`: when `try_wait()` reports the direct child exited, `kill_and_wait` returns `Ok(())` without signalling or waiting on the recorded group, so KILL-01's no-orphan guarantee is not established by reusing it verbatim. Cursor did not examine that branch. Codex's reading of the source is correct.
- **Fallback deletion authority (18-02).** Codex raises two HIGHs: (a) any Git refusal triggers `remove_dir_all` after only a root-prefix check, dropping the common-dir identity, exact-path and main-worktree protections the verified path enforces, so a stale managed path that now holds another repository or a main checkout can be deleted; (b) `starts_with(root)` accepts the root itself and ancestor directories beneath it, and canonicalizing an in-root symlink moves the deletion target to its destination. Cursor treats containment as sufficient ("the load-bearing control") and rates 18-02 LOW. Both are judgment calls on the same source; the plan should state which identity checks the forced path keeps.
- **PTY/reader closure ownership (18-04).** Codex HIGH: `RowRuntime` borrows `Session`, so after `Ok` the caller still owns the master and writer, reader threads are detached, and nothing joins or closes them. Cursor LOW with the same observation. Severity disagreement only; the fix (document or implement drop/unregister after success) is the same.
- **Branch snapshot equality (18-02, 18-06).** Codex MEDIUM: raw `git branch --list` output includes a `+` marker for branches checked out in linked worktrees, which legitimately changes when the worktree is removed, so the byte-equality assertion rejects a correct deletion. Cursor calls the `for-each-ref` OID snapshot a strength and does not notice the `branch --list` half. Codex is right about the `+` decoration; compare refs and OIDs only.
- **RED acceptance criteria (18-01, 18-03).** Codex: the unconditional `UnknownTarget` stub already passes 18-01's unknown-target test and 18-03's no-op stub already passes its no-op test, so "every new test fails" cannot hold. Cursor flags the same 18-03 inversion risk as LOW. Loosen the RED count to the tests that can fail.
- **Fixture validity (18-03, 18-04).** Codex only: 18-03's builders all share one common dir, path and `PrimaryDefault` role so the prescribed topology violates identity and unique-role checks; 18-04's live fixtures pair `Inactive` with `Some(OwnedRuntime)`, which `validate()` allows but `validate_for_save()` rejects, so survivor tests can fail before `remove` is called. Also `owned_runtime().map(|r| r.agent)` moves out of a borrow (`ProcessIdentity` is `Clone`, not `Copy`). Cursor did not check these.
- **18-05 coverage.** Codex only: "every `AtomicFailure` branch" omits `Sync` and `RefusedRow`, and crash consistency is tested only for a `Checkout` target, not a parent `Repository` removal mid-fan-out. Also validation of the candidate state runs only after kill and delete, so a validation failure follows irreversible effects.
- **18-07 scope.** Codex only: Linux fixes after Task 1 do not require rerunning the macOS gate, and the four-file fix allowlist excludes `pty.rs` / `session.rs` where a real shutdown-primitive defect would live. Cursor's concern is environmental (Docker Desktop required, `confidence: low`).

---

# Cross-AI Plan Review — Phase 18, Convergence Cycle 2

Reviewed 2026-11-02T17:32Z against the eight plans at `fb84cf5` (18-01 through 18-08, each carrying a Review Dispositions Ledger). Cycle 1 (above) was committed at `0070c74`. The lane runner was launched exactly once and ran the six lanes sequentially: Codex in 8 min and Cursor in 13 min produced full reviews; OpenCode, Antigravity and Ollama produced no review and Qwen emitted only its marker. Stubs are verbatim.

## Plan-Revision Conflicts (Cycle 2)

None. Both grounded lanes reviewed all eight plans at `fb84cf5`; no plan changed while the lanes ran.

## Codex Review (Cycle 2)

Reviewed all eight plans against repository HEAD `fb84cf5` and the prior review. No files were modified; builds and tests were not run during this read-only review.

### 18-01

**Summary:** The selection and preview design is coherent. The revised text addresses the compilation dependency, borrowed identity, persisted shell ownership, missing-path classification and RED-count findings.

**Strengths**

- Carrying `OwnedRuntime` preserves generation, agent and shell ownership, matching the actual model at [repository.rs:98](baude-core/src/repository.rs:98).
- The removal-owned disk error removes the dependency on an unimplemented Git type. The mechanism claimed by the ledger is present at [18-01-PLAN.md:94](.planning/phases/18-shared-kill-then-remove-lifecycle/18-01-PLAN.md:94).

**Concerns**

- **LOW — An acceptance criterion contradicts required fixture cleanup.** Task 1 requires removing fixture directory trees, while Task 2 requires zero occurrences of `remove_dir_all` anywhere in the module, including tests. Both cannot hold with direct cleanup calls. See [18-01-PLAN.md:125](.planning/phases/18-shared-kill-then-remove-lifecycle/18-01-PLAN.md:125) and [18-01-PLAN.md:165](.planning/phases/18-shared-kill-then-remove-lifecycle/18-01-PLAN.md:165).

**Suggestions**

- Remove the whole-file side-effect grep. Keep the observable preview assertions and inspect the production functions separately.

**Risk Assessment: LOW.** The substantive first-round dispositions are reflected in the revised plan.

### 18-02

**Summary:** Deletion authority is substantially better specified, but retry behavior and the foreign-repository guard still contain important gaps.

**Strengths**

- Common-directory equality, main-worktree exclusion and exact registration checks restore protections present in the existing verified path. Compare [18-02-PLAN.md:98](.planning/phases/18-shared-kill-then-remove-lifecycle/18-02-PLAN.md:98) with [git.rs:2520](baude-core/src/git.rs:2520).
- Root, ancestor and symlink refusal fixtures directly address the original overly broad containment rule. Their required outcomes are explicit at [18-02-PLAN.md:83](.planning/phases/18-shared-kill-then-remove-lifecycle/18-02-PLAN.md:83).

**Concerns**

- **HIGH — A failed deletion can permanently defeat retry.** The helper rejects an existing directory without registration as `NotRegistered` at [18-02-PLAN.md:100](.planning/phases/18-shared-kill-then-remove-lifecycle/18-02-PLAN.md:100). However, Git 2.54 continues to delete the administrative entry even when deleting the worktree directory fails. If the fallback also fails, restoring permissions and retrying encounters an existing, now-unregistered directory and refuses it. This is confirmed by [Git 2.54’s removal implementation](https://github.com/git/git/blob/v2.54.0/builtin/worktree.c#L1328).
- **HIGH — Repository inspection failure authorizes fallback deletion.** `foreign_repository_guard` permits any unsuccessful `rev-parse`, treating it as equivalent to a missing `.git` file. An inspection failure caused by permissions or damaged repository metadata does not establish that replacement user data is safe to delete. The ledger’s foreign-repository protection is therefore incomplete. See [18-02-PLAN.md:99](.planning/phases/18-shared-kill-then-remove-lifecycle/18-02-PLAN.md:99). Existing discovery distinguishes recognized non-repository output from other failures at [git.rs:340](baude-core/src/git.rs:340).
- **MEDIUM — Already-missing paths require their immediate parent to exist.** Canonicalizing only `path.parent()` fails when a workspace/repository ancestor—or the managed root itself—has also disappeared. Such a row should still be removable under the locked already-removed rule. See [18-02-PLAN.md:97](.planning/phases/18-shared-kill-then-remove-lifecycle/18-02-PLAN.md:97). The existing inventory deliberately retains observations whose paths cannot be canonicalized at [git.rs:314](baude-core/src/git.rs:314).

**Suggestions**

- Define how deletion authority survives a failed Git removal that erases registration; add a retry fixture reproducing that outcome.
- Distinguish verified missing Git metadata from inspection errors. Refuse uncertain repository identity.
- Add a missing-ancestor fixture and resolve containment through the nearest existing ancestor without weakening path protections.
- Use the existing absolute-path Git query pattern at [git.rs:335](baude-core/src/git.rs:335) consistently in helpers and fixtures.

**Risk Assessment: HIGH.** Partial filesystem failure can make removal unretryable, and uncertain identity currently permits destructive fallback.

### 18-03

**Summary:** The batch mutation is appropriately small and well separated from persistence. The fixture and RED-count dispositions match the revised task text and current validator.

**Strengths**

- Parameterized fixture identities and roles address real validation constraints: duplicate repository identities and duplicate singleton roles are rejected at [repository.rs:925](baude-core/src/repository.rs:925) and [repository.rs:976](baude-core/src/repository.rs:976).
- The parent-only negative test exercises the actual dangling-child check at [repository.rs:941](baude-core/src/repository.rs:941).
- Counter preservation agrees with the aggregate’s requirement that allocated keys remain below their next counters at [repository.rs:1018](baude-core/src/repository.rs:1018).

**Concerns**

- No new substantive finding. The unchecked mutation contract is explicitly documented at [18-03-PLAN.md:73](.planning/phases/18-shared-kill-then-remove-lifecycle/18-03-PLAN.md:73).

**Suggestions**

- Keep the current scope and caller-validation contract.

**Risk Assessment: LOW.** This plan has clear inputs, deterministic mutation and suitable negative tests.

### 18-04

**Summary:** The revised orchestration has the right validation and commit boundaries. Its remaining weakness is an inconsistent contract between persisted ownership, accepted handles and confirmation output.

**Strengths**

- Candidate validation now precedes irreversible effects at [18-04-PLAN.md:106](.planning/phases/18-shared-kill-then-remove-lifecycle/18-04-PLAN.md:106), using the stronger validator documented at [repository.rs:661](baude-core/src/repository.rs:661).
- Committing state and dropping owned sessions only after replacement succeeds matches the writer’s actual commit boundary at [persist.rs:681](baude-core/src/persist.rs:681).
- Already-missing rows now reach administrative cleanup, resolving the original unreachable-prune finding at [18-04-PLAN.md:108](.planning/phases/18-shared-kill-then-remove-lifecycle/18-04-PLAN.md:108).

**Concerns**

- **MEDIUM — Confirmation can omit processes execution will kill.** Preflight accepts a live shell when persisted ownership says `Closed`; execution also kills supplied handles for rows with no persisted runtime. These exceptions are explicit at [18-04-PLAN.md:105](.planning/phases/18-shared-kill-then-remove-lifecycle/18-04-PLAN.md:105) and [18-04-PLAN.md:107](.planning/phases/18-shared-kill-then-remove-lifecycle/18-04-PLAN.md:107). A state-only preview cannot identify those processes from `OwnedRuntime`, whose complete representation is at [repository.rs:98](baude-core/src/repository.rs:98).
- **MEDIUM — Duplicate rejection is skipped for rows without persisted runtime.** Preflight’s checks apply only to `runtime: Some`, but the kill stage accepts handles for other selected rows. Consequently, the universal handle-verification claim at [18-04-PLAN.md:261](.planning/phases/18-shared-kill-then-remove-lifecycle/18-04-PLAN.md:261) exceeds the actual algorithm.

**Suggestions**

- Establish one runtime snapshot used by both preview and execution, or require synchronization before preview and reject discrepancies.
- Check duplicate handles for every selected row.
- Add explicit tests for an unrecorded runtime and an unexpected live shell, including their confirmation output.

**Risk Assessment: MEDIUM.** The stage ordering is sound, but ownership exceptions undermine the promised exact preview and preflight invariant.

### 18-05

**Summary:** Persistence coverage is materially improved. The new partial-deletion test, however, cannot produce its claimed scenario as written.

**Strengths**

- The revised suite covers all five actual `AtomicFailure` variants at [persist.rs:242](baude-core/src/persist.rs:242), including pre-replacement refusal and post-replacement directory-sync failure.
- The recovery assertions follow real mechanisms: missing checkout detection at [git.rs:488](baude-core/src/git.rs:488) and protection recording at [lifecycle.rs:629](baude-core/src/lifecycle.rs:629).

**Concerns**

- **HIGH — The mid-fan-out fault blocks the first child too.** The test changes the shared managed root to `0o555` before calling removal, then expects the first child’s directory to disappear and the second child to fail. Both directories have that same parent; removing either directory entry requires write permission there. Row ordering does not delay the permission change. See [18-05-PLAN.md:123](.planning/phases/18-shared-kill-then-remove-lifecycle/18-05-PLAN.md:123). The Linux gate deliberately runs without root privileges at [gate.sh:38](scripts/gate.sh:38), so privilege bypass cannot rescue the fixture.
- **MEDIUM — The ledger overstates completed coverage.** It claims the second-child failure after first-child deletion is incorporated, but the prescribed fixture does not reach that boundary. See [18-05-PLAN.md:184](.planning/phases/18-shared-kill-then-remove-lifecycle/18-05-PLAN.md:184).

**Suggestions**

- Put the second child beneath a separate parent and deny writes only there, restoring original permissions through the guard.
- Preserve the partial-progress assertions and repair the fixture.
- Verify the registration left after Git’s failure, then exercise retry against that actual state.

**Risk Assessment: HIGH.** The central new ordering test is invalid and can conceal a real retry defect.

### 18-06

**Summary:** The expanded integration suite addresses most missing cases and uses realistic repository topology. Its coverage claims remain broader than the prescribed matrix.

**Strengths**

- The cloned fixture follows the existing real-user topology at [lifecycle.rs:4284](baude-core/src/lifecycle.rs:4284).
- Holding `TestRedirect` throughout fixture lifetime matches its thread-local ownership requirements at [testing.rs:98](baude-core/src/testing.rs:98).
- Live main and standalone preservation tests are explicitly present at [18-06-PLAN.md:148](.planning/phases/18-shared-kill-then-remove-lifecycle/18-06-PLAN.md:148).

**Concerns**

- **MEDIUM — Live unmanaged removal remains absent.** The unmanaged case has no runtime, and the repository fan-out case contains only Main and managed children. See [18-06-PLAN.md:118](.planning/phases/18-shared-kill-then-remove-lifecycle/18-06-PLAN.md:118) and [18-06-PLAN.md:152](.planning/phases/18-shared-kill-then-remove-lifecycle/18-06-PLAN.md:152).
- **LOW — Both-fixture coverage is overstated.** The `git init` test covers a non-live Main removal and live managed removal; it does not reproduce the full keep/runtime matrix. Its actual scope is explicit at [18-06-PLAN.md:150](.planning/phases/18-shared-kill-then-remove-lifecycle/18-06-PLAN.md:150).

**Suggestions**

- Add a live unmanaged child to a parent-removal fixture and assert its directory and Git registration survive.
- Enumerate the exact fixture/runtime matrix in the success criteria; extend tests where broader coverage is required.

**Risk Assessment: MEDIUM.** Core coverage is much stronger, but the final claims should match the cases actually tested.

### 18-07

**Summary:** The first-round procedural findings are addressed. The plan now validates the final revision on both platforms and permits fixes where shutdown behavior actually lives.

**Strengths**

- The Linux gate genuinely runs as UID 1000 at [gate.sh:38](scripts/gate.sh:38).
- Rerunning macOS after Linux-driven fixes is explicitly required at [18-07-PLAN.md:108](.planning/phases/18-shared-kill-then-remove-lifecycle/18-07-PLAN.md:108).
- The history-correction procedure matches the gate’s whole-range validation: [gate.sh:68](scripts/gate.sh:68) invokes the checker, which visits every non-merge commit at [check-commit-message.sh:73](scripts/check-commit-message.sh:73).

**Concerns**

- No new plan-local blocker. The gate executes the available tests at [gate.sh:107](scripts/gate.sh:107); passing it cannot establish scenarios omitted from those tests.

**Suggestions**

- Preserve the procedure and retain both logs with the tested commit hash.
- Resolve the behavioral and fixture findings before treating the gates as phase-completion evidence.

**Risk Assessment: LOW.** The revised gate procedure is appropriate.

### 18-08

**Summary:** Fixing shutdown inside the primitive is the correct approach. However, the proposed implementation introduces a fail-open identity decision and still permits successful return without completing reader cleanup.

**Strengths**

- The exited-leader fixture targets the actual early-success defect at [pty.rs:759](baude-core/src/pty.rs:759).
- Retaining the reader handle addresses the currently detached spawn at [pty.rs:79](baude-core/src/pty.rs:79).
- Dropping the child guard before joining correctly recognizes that the reader also acquires that lock at [pty.rs:92](baude-core/src/pty.rs:92).

**Concerns**

- **HIGH — Inspection errors become permission to signal.** The plan converts `inspect_process_identity` errors into `None`, and `None + live group` selects `Signal`. See [18-08-PLAN.md:82](.planning/phases/18-shared-kill-then-remove-lifecycle/18-08-PLAN.md:82). The actual inspectors distinguish absence from permission, parsing and other failures at [session.rs:651](baude-core/src/session.rs:651) and [session.rs:767](baude-core/src/session.rs:767). Collapsing these outcomes invalidates the ownership proof.
- **HIGH — Reader cleanup remains incomplete on successful removal.** After two seconds, the plan returns `Ok` with the reader handle retained at [18-08-PLAN.md:83](.planning/phases/18-shared-kill-then-remove-lifecycle/18-08-PLAN.md:83). Dropping that handle detaches the thread; it does not cancel it, as documented by [Rust’s `JoinHandle` contract](https://doc.rust-lang.org/std/thread/struct.JoinHandle.html). The ledger’s deferred disposition at [18-08-PLAN.md:207](.planning/phases/18-shared-kill-then-remove-lifecycle/18-08-PLAN.md:207) therefore leaves KILL-02 unresolved. A process outside the kill group does not make baude’s own reader thread outside the cleanup requirement.
- **HIGH — The shared child mutex can block teardown before signalling.** On reader EOF, existing code performs blocking `child.wait()` while holding the mutex at [pty.rs:92](baude-core/src/pty.rs:92). If the slave descriptors close while the leader remains alive, teardown can block acquiring that same mutex before sending TERM. The proposed sequence retains this ordering at [18-08-PLAN.md:82](.planning/phases/18-shared-kill-then-remove-lifecycle/18-08-PLAN.md:82).
- **MEDIUM — RED cleanup uses the primitive whose failure it demonstrates.** The exited-leader test’s unwinding guard calls unchanged `kill_and_wait`, which returns immediately for the exited leader and leaves its trapping descendant alive. Compare [18-08-PLAN.md:112](.planning/phases/18-shared-kill-then-remove-lifecycle/18-08-PLAN.md:112) with [pty.rs:764](baude-core/src/pty.rs:764).

**Suggestions**

- Preserve inspection errors and refuse teardown when identity cannot be established; test that no signal is sent.
- Make reader cancellation and joining part of successful teardown, or return an error while retaining ownership.
- Remove blocking waits under the shared child mutex and add a closed-slave/live-leader fixture.
- Give the RED fixture independent, fixture-owned descendant cleanup.

**Risk Assessment: HIGH.** Exact ownership and complete teardown are not yet established.

### Overall assessment

**Changes required before convergence.** The single-save architecture and accepted disk-before-save recovery gap can remain.

The principal blockers are:

1. Retry after Git deletes registration but leaves a directory.
2. Inspection failures treated as safe absence, in both repository and process guards.
3. Successful teardown that can leave a detached reader.
4. Blocking reaping under the child mutex.
5. The invalid second-child deletion-failure fixture.

Most ledger dispositions are supported by concrete revisions. The deletion-authority, reader-cleanup and partial-fan-out dispositions remain incomplete; plans 18-03 and 18-07 are otherwise ready.

---

## OpenCode Review (Cycle 2)

_Lane skipped: no review was produced. Diagnostic output preserved verbatim below; this is not a review._

opencode review failed or returned empty output. stderr:

Raw response body:
OpenCode review returned no assistant text (#1936: agent ended its turn with no final message).
Diagnostic: stop reason=?, output tokens=?
Diagnosis: opencode ran with no effort argument, so the reviewer CLI's own configuration applied and exited with status 1.

---

## Qwen Review (Cycle 2)

_Lane ran but produced no review body: only the discount marker and a reviewer tag below. The lane's stderr shows the cause: the Copilot integration rejected model `claude-sonnet-4.6` for the `opencode` integrator. Not a review; no findings to weigh._

> [reviewed-without-source-citations] This reviewer declared source-grounded evidence but cited no file:line source evidence, so it reviewed the pasted plan text only — down-weight its verdict in the Consensus Summary.

[reviewer: github-copilot/claude-sonnet-4.6 via opencode (invoked as 'qwen')]

---

## Cursor Review (Cycle 2)

Source-grounded review of all 8 plans against `baude-core` at the current tree. Disposition ledgers were checked claim-by-claim: no ledger invents a fix absent from plan text. Residual risk is incomplete wiring and one new kill-path semantic hole introduced by plan 08.

### 18-01

**Summary:** Round-1 blockers are fixed in plan text: `DiskRemovalFailure` is owned by `removal.rs` so wave-1 compile no longer depends on plan 02; preview carries full `OwnedRuntime` and `shell_pid`; RED expects 5 failed / 1 passed. The remaining gap is that `common_dir` is declared and consumed by plan 04 but never populated or asserted by Task 2.

**Strengths**
- Classification table matches real row fields: `CheckoutRole::{Main, PrimaryDefault, ManagedBranch}` and `managed_by_baude` at `baude-core/src/repository.rs:284-288` and `:348-352`.
- `DiskRemovalFailure` / `DiskRemovalFailureKind` are removal-owned; acceptance requires `grep -c 'crate::git::ForcedRemovalError' … prints 0`, which restores independent `cargo check -p baude-core` for this plan alone.
- `owned_runtime().cloned()` matches `ProcessIdentity: Clone` not `Copy` (`repository.rs:60-68`).
- `symlink_metadata` + `NotFound`-only downgrade avoids treating permission errors as already-removed.

**Concerns**
- **MEDIUM — `common_dir` is specified on the type but omitted from Task 2.** Feature implementation says `PlannedRow.common_dir` is copied from `observed_common_dir` (`18-01-PLAN.md` feature impl). Behavior bullet and Task 2 action only require `runtime` and `main_worktree` (`18-01-PLAN.md` Task 2 action). No RED test asserts `common_dir`. Plan 04’s disk stage passes that field into `remove_managed_worktree_forced` for identity re-proof (`18-04-PLAN.md` step 5). A literal Task-2 implementation can leave `None` and break T-18-20.
- **LOW — Threat-model text lags the disposition.** Trust boundary still describes `Path::exists` while implementation uses `symlink_metadata`.
- **LOW — Non-`NotFound` inspection is untested.** Plan says permission denied leaves `RemoveManagedWorktree`; nothing pins that.

**Suggestions**
- Add `common_dir: Some(parent.observed_common_dir.to_path_buf())` to the `select_rows` algorithm and Task 2 action; assert it in `select_rows_carries_owned_runtime_identity` or a sibling.
- Align the threat boundary with `symlink_metadata` / `NotFound`.

**Risk Assessment: MEDIUM.** Selection/preview design is sound; the `common_dir` task gap is a real cross-plan defect, not a false ledger claim.

### 18-02

**Summary:** Round-1 critical deletion-authority findings are addressed with concrete guards and thirteen fixture tests. Containment alone is no longer authority; identity, main-worktree exclusion, registration, and foreign-repository checks precede any `remove_dir_all`.

**Strengths**
- Re-runs ownership checks that live today in `topology_blockers` (`git.rs:2509-2544`) while correctly bypassing only cleanliness/lock from `inspect_removal` (`git.rs:2549-2611`). CONTEXT’s “clean preflight inside `remove_verified_worktree`” wording is corrected.
- `managed_root_contains` refuses root equality and symlinks before resolve; ancestors fail registration equality (`18-02-PLAN.md` behavior refusals).
- Branch proof uses `for-each-ref` OID snapshots, matching the Round-1 decoration defect on `git branch --list`.
- Fixture depth matches production: `TestRedirect` → `<root>/data`, `worktrees_base()` → `…/baude/worktrees` (`testing.rs` + `git.rs:1771-1797`), with worktrees under `ws/repo/…` (`git.rs:1802+`).
- No force path exists today (`verified_remove_arguments` is non-force at `git.rs:2614-2620`); this plan correctly adds a new primitive.

**Concerns**
- **LOW — Containment vs authority can be misread.** `managed_root_contains` still accepts in-root ancestors via `starts_with`; refusal depends on later `NotRegistered`. Correct, but easy to implement incompletely if an executor stops at containment.
- **LOW — Unlock-then-prune race remains deferred** to plan 07. Honest, but it is still an operational residue on locked trees.

**Suggestions**
- One sentence on `managed_root_contains`: containment is necessary, not sufficient; registration must still prove the path is the selected worktree.
- Keep the deferred race visible in 07 must_haves.

**Risk Assessment: LOW.** Round-1 HIGH deletion risks are mitigated in plan text with named tests.

### 18-03

**Summary:** Small, correctly scoped aggregate mutation. Round-1 fixture and RED-count problems are fixed: parameterized `repository_at` / `checkout_at`, seed `validate()`, and 3 failed / 1 passed for the no-op stub.

**Strengths**
- Batch remove then validate matches the real invariant: dangling child is `DanglingRepositoryKey` (`repository.rs:941-945`); childless parent is allowed, so parent+children must be removed together.
- Counters preserved — validation requires counters above surviving keys (`repository.rs` counter checks near `:1018`).
- Existing shared builders really collide (`repository.rs:1135+`); parameterization is necessary, not cosmetic.

**Concerns**
- **LOW — `checkout_at` session consistency is thinner than plan 01.** Overrides path/role/branch; if an executor omits `is_worktree` / cwd alignment, seeds can fail `validate()` for unrelated reasons.

**Suggestions**
- Mirror plan 01’s session rules in the `checkout_at` helper description.

**Risk Assessment: LOW.**

### 18-04

**Summary:** Orchestration now matches the locked contract: full identity preflight, `validate_for_save` before kill, owning `Session` handles, `AlreadyRemoved` still calls the force helper, and wave-2 `depends_on` includes plan 08 so KILL-01/02 are not assumed from today’s early-return `Pty::kill_and_wait`.

**Strengths**
- Avoids `destructive_teardown` / `mark_teardown_pending` (`lifecycle.rs:987-999`, `:919-941`), which mutate durable state on kill failure — the opposite of KILL-03.
- Preflight uses full `ProcessIdentity` equality and Owned-shell match before any signal; today’s PID-only sketch is gone.
- Live seeds use `Running` + `OwnedRuntime`, matching `validate_for_save` rejection of `Inactive`+Some(runtime) (`repository.rs:745-764`).
- `LiveHandles` Drop addresses the lack of production `Pty` Drop teardown (`pty.rs:140+`).
- `fail_next_teardown_for_test` fires first inside `kill_and_wait` (`pty.rs:745-754`), so the survivor fixture is real.
- Disk stage calls `remove_managed_worktree_forced` for both `RemoveManagedWorktree` and `AlreadyRemoved`, closing the Round-1 prune hole.

**Concerns**
- **MEDIUM — Depends on 01 actually filling `common_dir`.** Step 5 takes `common_dir` from `PlannedRow`. If 01 leaves it unset, identity re-proof never receives the persisted expected dir.
- **LOW — Kill iteration order vs parent-abort story.** `kill_stage` “iterate handles whose row is in the set” follows Vec order, while `parent_kill_failure_aborts…` assumes c1 is already extinct when c2 fails. Not pinned to `RemovalSet` order.
- **LOW — must_haves list `(pid, start_time, process_group)` but omit `session`.** Impl text says full equality; `ProcessIdentity` includes `session` (`repository.rs:62-68`). Understatement only.

**Suggestions**
- Fix 01 `common_dir` before executing 04.
- Kill in `set.rows` order (lookup handles), and build the parent-failure fixture from that order.
- State must_haves as “full `ProcessIdentity` PartialEq including `session`”.

**Risk Assessment: MEDIUM.** Design is strong; residual risk is cross-plan wiring and kill-order determinism, not Round-1 regressions.

### 18-05

**Summary:** Persistence and ordering proofs now cover every `AtomicFailure` variant, repository mid-fan-out crash/disk failure, and validate-before-kill. Disposition claims match the plan body and align with plan 04’s stage order.

**Strengths**
- All five inject points exist: `Write`, `Sync`, `Rename`, `DirectorySync`, `RefusedRow` (`persist.rs:242-251`); rename vs directory-sync correctly track `replacement_committed` after rename (`persist.rs:678+`).
- Repository mid-fan-out tests address SC5 beyond a single checkout target.
- Task 3 proves validation precedes irreversible effects against a live group (`kill(-pgid,0)==0`), matching plan 04 step 3.

**Concerns**
- **LOW — Validation fixture uses a pre-seeded contradictory survivor**, not a candidate made invalid by `remove_rows` itself. Still proves “nothing irreversible before Validation”.
- **LOW — `0o555` undeletable-directory fixture** may differ Darwin vs Linux; 07 is the backstop.

**Suggestions**
- Keep the dual fix path (04 owns order; 05 may repair). Optional: one removal-produced invalid candidate if such a shape exists.

**Risk Assessment: LOW.**

### 18-06

**Summary:** Integration matrix and keep rules are substantially repaired. Already-removed prune is correctly attributed to plan 04; branch snapshots use for-each-ref; live main/standalone, unmanaged, unclean cwd-held on both fixture kinds, and parent fan-out are present.

**Strengths**
- Cloned topology mirrors real layouts used elsewhere (`lifecycle.rs` cloned fixtures / research).
- `TestRedirect` held for fixture lifetime matches thread-local redirect semantics (`testing.rs:98-198`).
- Unmanaged keep path asserts directory and porcelain inventory survive — distinct from managed delete.
- Live keep rows prove kill-without-delete for Main and Standalone.

**Concerns**
- **LOW — Parent fan-out omits an unmanaged child.** Unmanaged is only a solo `Checkout` target (`unmanaged_checkout_keeps_directory`). A repository target with Main + managed + unmanaged in one call would exercise every disk class together.
- **LOW — Shell-on-cloned-fixture deferred** to plan 04’s operation-level shell test. Justified; SC3 cloned-fixture shell parity remains thin.

**Suggestions**
- Optionally add an unmanaged KeepDirectory child inside `repository_removes_children_then_parent`.

**Risk Assessment: LOW.**

### 18-07

**Summary:** Gate plan now requires both platforms green on the same final `HEAD`, expands the fix allowlist to `pty.rs`/`session.rs`, and treats bad earlier commit messages as amend-only-unpushed or orchestrator blockers. Ledger matches body.

**Strengths**
- `scripts/gate.sh --linux` as uid 1000 matches the real gate (`scripts/gate.sh`).
- macOS re-run after Linux fixes closes the Round-1 “invalidated host result” hole.
- Hand-written subjects piped through `check-commit-message.sh -` match release-please’s nested-paren trap (`CLAUDE.md`, `scripts/check-commit-message.sh`).
- Acceptance requires plan-08 extinction tests on Linux, so a skipped 08 cannot silently pass 07.

**Concerns**
- **LOW — `depends_on: [18-06]` only**; 08 is transitive. Behavior is enforced by acceptance greps, not the dependency graph.

**Suggestions**
- Optional: add `18-08` to `depends_on` for clarity.

**Risk Assessment: LOW.**

### 18-08

**Summary:** Correctly targets the real primitive defect Round-1 found: `Pty::kill_and_wait` returns `Ok(())` when `try_wait()` is `Some` without signalling the group (`pty.rs:759-765`), and the reader `JoinHandle` is dropped at spawn (`pty.rs:79-110`). The new disposition table’s `ForeignOwner` arm overclaims a portable kernel invariant and can skip signalling surviving descendants if PID reuse and orphaned PGID can coexist.

**Strengths**
- Exited-leader fixture (leader gone, `kill(-pgid,0)==0`, trapping descendant) is exactly the path today’s early return misses.
- Deadlock mitigation is real: reader reaps under the child mutex (`pty.rs:92-94`); plan joins after dropping the guard.
- `fail_next_teardown_for_test` preserved so plan 04’s survivor path still works.
- No new grace knob — reuses existing 500 ms waits (`pty.rs:830-839`).
- Wave-1 ownership of `pty.rs`/`session.rs` pairs correctly with plan 07’s expanded allowlist.

**Concerns**
- **HIGH — `ForeignOwner` while `group_alive` can refuse to kill orphans.** Rule (`18-08-PLAN.md`): if `group_alive` and `observed` is `Some(other)` → `ForeignOwner` (no signal), justified by “POSIX/Linux/XNU never reuse a pid while a process group with that id exists.” That invariant is plausible on Linux (`struct pid` refcount) but is asserted for XNU/POSIX without evidence. Distinguisher the table ignores: `observed.process_group` vs `recorded.process_group`. If a reused PID holds a *different* PGID while orphans remain under `recorded.process_group`, the correct action is `Signal`; the plan maps it to `ForeignOwner`. Table case `(recorded, Some(other same pid / different start_time), true) → ForeignOwner` encodes that risk. Threat T-18-24 mitigates foreign signalling, but the current rule over-refuses.
- **MEDIUM — must_have says reader is joined; implementation returns `Ok` after a 2 s bound with `reader_finished()==false`.** Deferred row explains this; must_haves / KILL-02 wording should match the observable timeout path.
- **LOW — Extinct/ForeignOwner reap sentence is garbled** (`if !leader_exited is false` in the kill_and_wait restructure bullet).

**Suggestions**
- Tighten `group_disposition`: if `!group_alive` → `Extinct`; else if observed matches recorded or is `None` or `observed.process_group != recorded.process_group` → `Signal`; else (`observed.pgid == recorded.pgid` and identity differs) → `ForeignOwner`. Update the table test.
- Soften the doc from universal POSIX/XNU to “portable rule uses pgid comparison; Linux pid-namespace refcounting is supporting evidence.”
- Clarify KILL-02: joined on the normal path; timeout leaves `reader_finished` false.

**Risk Assessment: HIGH** until the `ForeignOwner` arm is tightened; otherwise the plan correctly closes the exited-leader hole against `pty.rs:759-765`.

---

### Cross-plan assessment

**Disposition fidelity:** Ledgers for 01–08 are honest. Round-1 HIGHs on ForcedRemovalError compile dep, deletion identity, AlreadyRemoved prune, PID-only preflight, validate-after-effects, and exited-leader/reader gaps are present in plan text. Plan 08 is the right vehicle for the kill-primitive findings.

**Wave graph:** `01∥02∥03∥08` → `04` → `05` → `06` → `07` is coherent. `18-04 depends_on: [18-01, 18-02, 18-03, 18-08]` is correct.

**Remaining blockers before execute**
1. **18-08 ForeignOwner / pid-reuse table** — portable Signal vs ForeignOwner must use `process_group` comparison, not “any unequal identity while group_alive”.
2. **18-01 `common_dir` Task-2 wiring** — populate and assert; plan 04 identity re-proof depends on it.

**Non-blockers worth fixing while editing:** kill-order pin in 04; threat-model / must_have honesty (01 exists probe, 08 reader join bound); optional unmanaged child in parent fan-out.

**Overall risk: MEDIUM.** Round-1 “do not execute” HIGHs on deletion authority and orchestration are largely resolved. Do not execute plan 08’s disposition table as written without the pgid-aware ForeignOwner fix; once that and `common_dir` wiring land, the phase plans are executable.

---

## Antigravity Review (Cycle 2)

_Lane skipped: no review was produced. Diagnostic output preserved verbatim below; this is not a review._

Antigravity review failed or returned empty output.
stderr:
jetski: no output produced — a tool required the "read_file" permission that headless mode cannot prompt for, so it was auto-denied. Add an allow-rule under permissions.allow in settings.json (e.g. read_file(<target>)). Alternatively, re-run with --dangerously-skip-permissions to auto-approve all tools.
An agy session started this run but no review was recovered, so a pre-launch stall is ruled out. See stderr above; a headless run that was auto-denied a tool permission reports the cause and its fix there.

---

## Ollama Review (Cycle 2)

_Lane skipped: no review was produced. Diagnostic output preserved verbatim below; this is not a review._

ollama review failed or returned empty output. stderr:

Raw response body:
The operation was aborted due to timeout
Diagnosis: ollama is an HTTP lane and carries no reasoning-effort setting.

---

## Consensus Summary (Cycle 2)

Two grounded reviews ran again: Codex (gpt-6.1-sol, reasoning=high) and Cursor. Both cite `file:line` evidence from the live tree at `fb84cf5` and neither opens with a `[reviewed-without-source-citations]` or `[reviewed-without-repo-access]` marker, so both count at full weight and the consensus gate engages. OpenCode (no assistant text, #1936), Antigravity (headless `read_file` auto-denied) and Ollama (HTTP timeout against the local `qwen3:8b` server) produced no review; Qwen emitted only the discount marker and a reviewer tag whose stderr shows the cause: the Copilot integration rejected `claude-sonnet-4.6` for the `opencode` integrator. All four stubs are preserved verbatim above and carry no findings.

Both reviewers checked the eight Review Dispositions Ledgers claim by claim and agree they are honest: no ledger names a fix that is absent from plan text. Codex rates 18-02, 18-05 and 18-08 HIGH and the phase "changes required before convergence"; Cursor rates only 18-08 HIGH and the phase MEDIUM. The disagreement is one of scope, not of fact: Cursor stops at "the round-1 fixes are present"; Codex reads the new text one level deeper and finds residual holes in the fixes themselves.

### Cycle 1 HIGH retrospective

| Cycle 1 HIGH | Plan | Status at fb84cf5 | Evidence |
|---|---|---|---|
| `RemovalError::DiskRemoval` named `crate::git::ForcedRemovalError` while 18-01 was `depends_on: []` (both) | 18-01 | RESOLVED | `DiskRemovalFailure` / `DiskRemovalFailureKind` are owned by `removal.rs`; acceptance requires `grep -c 'crate::git::ForcedRemovalError' baude-core/src/removal.rs` to print 0 (both reviewers confirm) |
| `AlreadyRemoved` rows never reached the prune fallback (both) | 18-04, 18-06 | RESOLVED | `disk_stage` calls `remove_managed_worktree_forced` for both `RemoveManagedWorktree` and `AlreadyRemoved`; 18-06 attributes the prune to plan 04 (both confirm) |
| PID-only preflight handle matching (Codex) | 18-04 | RESOLVED, refined | Step 2 compares full `ProcessIdentity` and the persisted `Owned` shell identity before any signal. Codex now raises the remaining exceptions (Closed-shell and no-runtime handles) as MEDIUM, listed below |
| Exited leader bypasses process-group cleanup (Codex) | 18-04 | RESOLVED in design, moved to 18-08 | Plan 08 restructures `Pty::kill_and_wait` around `group_disposition` with an exited-leader fixture; 18-04 now `depends_on: [18-01, 18-02, 18-03, 18-08]`. The new HIGHs on plan 08 are distinct defects in that restructure, counted once below |
| Fallback deletion dropped identity, exact-path and main-worktree protections (Codex) | 18-02 | SUPERSEDED | Identity re-proof, main-worktree exclusion, registration and `foreign_repository_guard` precede `remove_dir_all`. Codex's two new 18-02 HIGHs (retry after Git drops registration; inspection failure passes the guard) are residual holes in that new text and are counted as the live findings, not this row |
| `starts_with(root)` accepted root and in-root ancestors; symlink canonicalized before check (Codex) | 18-02 | RESOLVED | `managed_root_contains` refuses root equality and returns `Symlink` from `symlink_metadata` before anything resolves; ancestors fail registration equality (both confirm) |
| `RowRuntime` borrowed `Session`; readers detached after `Ok` (Codex HIGH, Cursor LOW) | 18-04 | SUPERSEDED | `RowRuntime` now owns the `Session` and `remove` drains and drops handles; plan 08 adds a bounded reader join. Codex's new HIGH on the 2 s bound returning `Ok` with the reader detached is the live residual, corroborated by Cursor's MEDIUM on the same must_have |

Every cycle 1 HIGH is addressed in plan text. Two of the fixes carry residual defects that both reviewers trace to the revised text itself; those are counted once each as cycle 2 HIGHs rather than as still-open cycle 1 items.

### Orchestrator source-grounding of the cycle 2 HIGHs

- **18-02 retry after partial deletion (Codex).** Plan text at `18-02-PLAN.md:100` step (2) confirms "`None` with the directory present returns `NotRegistered`". Git's `builtin/worktree.c` `remove_worktree` ORs the result of deleting the work tree into the result of deleting the admin dir, so a failed directory delete still unregisters the worktree. Confirmed from plan text and Git behavior.
- **18-02 inspection failure passes `foreign_repository_guard` (Codex).** `18-02-PLAN.md:99`: "a failure (no `.git`, or ceiling reached) or an equal common dir passes". Confirmed verbatim; the plan does not distinguish exit status or stderr.
- **18-05 `0o555` fixture (Codex; Cursor LOW on platform variance).** `18-05-PLAN.md:123` sets the managed root to `0o555` and expects `!c1_dir.exists()` with c2 failing. Deleting a directory entry needs write permission on its immediate parent. Managed worktrees live below the root (`git.rs` `managed_default_worktree_path` nests `<workspace>/repository-<key>/`), so the chmod either blocks both children (shared immediate parent) or neither (nested parent, `x` bit still set). In no layout does it produce the asserted "c1 gone, c2 refused" boundary. Confirmed defective.
- **18-08 inspection error treated as `None` (Codex).** `18-08-PLAN.md:82`: "treat an inspection error as `None`", and `None` + live group maps to `Signal` at `:80`. `session.rs:651-654` (Linux) and `:767-771` (macOS) return `Err` for anything other than `NotFound` / `ESRCH`. Confirmed.
- **18-08 reader detached after the 2 s bound (Codex HIGH, Cursor MEDIUM).** `18-08-PLAN.md:83`: "If the bound elapses, put the handle back and return `Ok(())` anyway"; must_have at `:24` and `:235` claims the reader "has finished and been joined". Corroborated.
- **18-08 child mutex held across blocking reap (Codex).** `pty.rs:92-94` performs `child.wait()` under the mutex on reader EOF; `18-08-PLAN.md:82` has `kill_and_wait` "take the child lock" before computing the disposition or sending TERM. Confirmed from source.
- **18-08 `ForeignOwner` over-refusal (Cursor).** `18-08-PLAN.md:80` maps any live group with a non-matching live leader to `ForeignOwner` and asserts a POSIX/Linux/XNU invariant in a doc comment. The table at `:74` never compares `observed.process_group` with `recorded.process_group`. Judgment-class, source-grounded, no marker; counts.

All seven are judgment-class. Neither reviewer carries a discount marker and neither is a diff-only lane, so every single-reviewer HIGH counts under the gate.

### Agreed Strengths

- Ledgers are faithful: every disposition points at text that exists in the plan (both).
- 18-02 now proves identity, main-worktree exclusion and registration before any `remove_dir_all`; containment-first ordering and the trailing `--` are preserved; `for-each-ref` OID snapshots replace `git branch --list` (both).
- 18-04 bypasses `destructive_teardown` / `mark_teardown_pending`, validates the candidate before any signal, uses full `ProcessIdentity` equality, and owns `Session` handles with a panic-safe `LiveHandles` guard (both).
- 18-05 covers all five `AtomicFailure` inject points and the repository mid-fan-out save crash (both).
- 18-07 requires both platforms green on the same final `HEAD`, widens the fix allowlist to `pty.rs` / `session.rs`, and pipes hand-written subjects through `check-commit-message.sh` (both).
- 18-08 targets the real primitive defect at `pty.rs:759-765` with an exited-leader fixture that traps TERM and HUP in a descendant (both).
- Wave graph `01 || 02 || 03 || 08` -> `04` -> `05` -> `06` -> `07` is coherent (both).

### Agreed Concerns

- **18-08 reader join (Codex HIGH, Cursor MEDIUM):** the 2 s bound returns `Ok` with the handle retained, which detaches rather than cancels the thread; must_haves and KILL-02 wording claim a join that the timeout path does not deliver.
- **18-05 `0o555` fixture (Codex HIGH, Cursor LOW):** Codex shows the fixture cannot reach the asserted boundary; Cursor flags Darwin-vs-Linux variance of the same fixture.
- **18-06 unmanaged coverage (Codex MEDIUM, Cursor LOW):** unmanaged checkouts appear only as a solo non-live target; no live unmanaged removal and no unmanaged child in the repository fan-out.

### Divergent Views

- **18-02 risk.** Codex HIGH on retry-after-partial-failure and on inspection failures authorizing fallback deletion; Cursor LOW, judging the round-1 authority fixes complete. Source confirms both Codex readings of the plan text (above).
- **18-08 `ForeignOwner`.** Cursor HIGH: a reused pid holding a different pgid while orphans survive under the recorded pgid is mapped to `ForeignOwner` and never signalled; the fix is to compare pgids. Codex did not examine this arm; its 18-08 HIGHs are on error handling, the join bound and the mutex.
- **18-01 `common_dir`.** Cursor MEDIUM (and the 18-04 dependency on it): `PlannedRow.common_dir` is declared and consumed by 18-04 step 5 but Task 2's action at `18-01-PLAN.md:155` carries only `runtime` and `main_worktree`, and no RED test asserts it. Codex rates 18-01 LOW and did not notice. Confirmed against plan text: Task 2's action omits `common_dir`.
- **18-04 preflight exceptions.** Codex MEDIUM x2: a live shell is accepted when persisted ownership says `Closed`, handles for rows with no persisted runtime are killed without preflight (`18-04-PLAN.md:107`), and duplicate rejection applies only to `runtime: Some` rows, so the "every handle is verified" claim at `:261` overstates. Cursor calls the preflight a strength.

### Current HIGH Concerns

Seven HIGHs, all newly raised against the fb84cf5 text, all counted (gate engaged; two reviewers; no markers; one corroborated).

1. **18-02: a failed directory delete can make the row unretryable.** Git drops the admin entry even when the directory delete fails; the fallback may also fail (permissions); the retry then sees an unregistered existing directory and refuses `NotRegistered`. Needed: treat "unregistered, inside managed root, identity re-proved via the persisted `common_dir` and no foreign repository" as deletable, or record the partial state so the retry can proceed.
2. **18-02: inspection failure authorizes fallback deletion.** `foreign_repository_guard` passes on any `rev-parse` failure. Needed: pass only on the "not a git repository" / ceiling outcome; any other failure (permission, corrupt metadata) refuses with a new error arm.
3. **18-05: the mid-fan-out disk-failure fixture cannot produce its asserted boundary.** `0o555` on the managed root blocks (or fails to block) both children. Needed: make only c2 undeletable (chmod c2's immediate parent, or a non-empty read-only subdirectory inside c2 only), and correct the ledger row at `18-05-PLAN.md:184`.
4. **18-08: inspection errors become permission to signal.** `Err` from `inspect_process_identity` is collapsed to `None`, which selects `Signal`. Needed: propagate the error (refuse to signal) or add an `Unknown` disposition that bails.
5. **18-08: successful teardown can leave a detached reader.** After the 2 s bound `kill_and_wait` returns `Ok` with the handle parked. Needed: either return `Err` on bound expiry or make must_haves, KILL-02 wording and the doc comment state the bounded path honestly and surface `reader_finished()` to plan 04's `RowRuntime` drop. (Corroborated by Cursor.)
6. **18-08: the child mutex is taken before TERM while the reader can block in `child.wait()` under it.** Needed: compute `try_wait` and release the lock before signalling, or move the reader's reap out from under the shared mutex.
7. **18-08: `ForeignOwner` can refuse to kill orphans.** Needed: `group_disposition` compares `observed.process_group` with `recorded.process_group`; only a live leader with the same pgid and a different identity is `ForeignOwner`; soften the kernel-invariant doc comment and update the table test.

### Current Actionable Non-HIGH Concerns

Eleven items that `/gsd-execute-phase` would not see unless they land in a PLAN.md or its ledger.

1. **18-01:** add `common_dir: Some(parent.observed_common_dir.to_path_buf())` to the `select_rows` algorithm and Task 2 action; assert it in `select_rows_carries_owned_runtime_identity` or a sibling (Cursor MEDIUM; 18-04 depends on it).
2. **18-01:** the Task 2 acceptance grep requires zero `remove_dir_all` in `removal.rs` including tests, while Task 1 requires fixture directory cleanup. Scope the grep to non-test code or move cleanup into a `testing.rs` fixture `Drop` (Codex LOW).
3. **18-01:** threat-model row still says `Path::exists` (`18-01-PLAN.md:230`) while the implementation is `symlink_metadata` with a `NotFound`-only downgrade; add a test pinning that a non-`NotFound` probe error leaves `RemoveManagedWorktree` (Cursor LOW x2).
4. **18-02:** `managed_root_contains` canonicalizes only `path.parent()` on `NotFound`; a vanished repository or workspace ancestor fails canonicalization and the already-removed row cannot be pruned. Canonicalize the nearest existing ancestor and re-join the remainder (Codex MEDIUM).
5. **18-04:** state the preflight exceptions in behavior and must_haves (Closed-shell live handle accepted; no-runtime handles killed; duplicate check only for `runtime: Some`), or close them, and correct the universal-verification claim at `:261` (Codex MEDIUM x2).
6. **18-04:** pin `kill_stage` iteration to `set.rows` order and build `parent_kill_failure_aborts...` from that order; must_haves should say full `ProcessIdentity` equality including `session` (Cursor LOW x2).
7. **18-03:** `checkout_at` should mirror plan 01's session consistency rules (`is_worktree`, cwd alignment) so seeds pass `validate()` for the intended reason (Cursor LOW).
8. **18-06:** add a live unmanaged-checkout keep test and an unmanaged `KeepDirectory` child inside `repository_removes_children_then_parent`; reword the both-fixture claim at `18-06-PLAN.md:150` to what the `git init` test covers (Codex MEDIUM + LOW, Cursor LOW).
9. **18-07:** add `18-08` to `depends_on` so the dependency graph, not only the acceptance greps, enforces that 08's Linux extinction tests gate 07 (Cursor LOW).
10. **18-08:** the RED test's unwinding guard calls unchanged `kill_and_wait`, which returns early for the exited leader and leaves the trapping descendant alive; the guard must `kill(-pgid, SIGKILL)` directly (Codex MEDIUM).
11. **18-08:** repair the garbled sentence at `18-08-PLAN.md:82` ("reaps the leader if `!leader_exited` is false and `try_wait` still reports running is impossible here") so an executor is not left guessing (Cursor LOW).

Folded into HIGHs above, not counted separately: the 18-05 ledger overstatement at `:184` (HIGH 3), the 18-08 must_have / KILL-02 wording on the reader join (HIGH 5). Dispositioned already and not counted: the unlock-then-prune race (18-02 ledger, deferred to 07), shell-on-cloned-fixture (18-06 ledger, owned by 04), the pre-seeded validation fixture in 18-05 (reviewer marks optional).

### Overall

Codex: HIGH, "changes required before convergence". Cursor: MEDIUM, "do not execute plan 08's disposition table as written". Consensus: the phase is not yet executable. The round-1 architecture holds and the ledgers are honest, but plans 02, 05 and 08 each carry a defect in their revised text that would either delete the wrong thing, leave a process behind, or encode a test that cannot pass. One more revision round targeting the seven HIGHs and the eleven wiring items above should converge.
