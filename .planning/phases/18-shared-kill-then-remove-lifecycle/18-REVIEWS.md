---
phase: 18
reviewers: [codex, opencode, qwen, cursor, antigravity, ollama]
reviewed_at: 2026-11-02T16:23:10Z
plans_reviewed: [18-01-PLAN.md, 18-02-PLAN.md, 18-03-PLAN.md, 18-04-PLAN.md, 18-05-PLAN.md, 18-06-PLAN.md, 18-07-PLAN.md]
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
  qwen: "skipped: lane ran but emitted only the discount marker and a reviewer tag, no review body"
  cursor: "reviewed"
  antigravity: "skipped: headless run auto-denied the read_file permission, no review recovered"
  ollama: "skipped: HTTP request to the local qwen3:8b server aborted on timeout"
plan_coverage:
  qwen:
    total: 7
    missing: ["18-01", "18-02", "18-03", "18-04", "18-05", "18-06", "18-07"]
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
