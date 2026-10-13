---
phase: 13
reviewers: [codex]
reviewed_at: 2026-09-19T21:33:00Z
plans_reviewed: [13-01-PLAN.md, 13-02-PLAN.md, 13-03-PLAN.md]
models:
  codex: "gpt-5.6-sol (reasoning=high)"
model_sources:
  codex: "banner"
---

# Cross-AI Plan Review — Phase 13

<!-- gsd:plan-revision-conflicts:begin -->
## Plan-Revision Conflicts
<!-- gsd:plan-revision-conflicts:end -->

## 13-01

### Summary

The tracer identifies the right architectural seams—shared resolution, repository-root derivation, source-aware titles, and repo-root prefills—but its startup-lock design is unsafe as written. The proposed helper would lock a filename unrelated to either real state file and would weaken the TUI’s existing refusal behavior. Workspace-source precedence and the test-build initialization model also need to be reconciled before execution.

### Strengths

- Centralizing launch resolution is justified. The TUI currently performs folder lookup, initialization, locking, and recording inline, while `bauded` only initializes without a folder hint. A shared resolver would remove real divergence between [baude/src/main.rs:351](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/main.rs:351) and [bauded/src/main.rs:174](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/src/main.rs:174).
- The repository deduplication premise is correct: `admit_repository` compares canonical Git common directories, not launch paths, at [baude/src/app.rs:1914](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/app.rs:1914). `discover_repository` canonicalizes both the input and common directory at [baude-core/src/git.rs:322](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/git.rs:322).
- The plan correctly notices that adding `Workspace.source` requires updating non-resolver struct literals such as [baude-core/src/worktree_scan.rs:431](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/worktree_scan.rs:431).
- The proposed `n` prefill change targets the correct method. Its present chain selects `new_session_dir` before the launch directory and has no Git-root check at [baude/src/app.rs:4057](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/app.rs:4057).
- Reusing the existing sidebar chrome is appropriately scoped; the current title is constructed at [baude/src/ui.rs:218](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/ui.rs:218).

### Concerns

- **HIGH — The proposed lock name does not protect either real state file.** `claim_workspace_state_lock(base, ws)` locks the file produced by `ws.state_file(base)` at [baude-core/src/persist.rs:536](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/persist.rs:536). The plan hardcodes `"workspace-state"`, while the TUI uses `"state"` and the daemon uses `"daemon-state"` ([baude/src/main.rs:377](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/main.rs:377), [bauded/src/manager.rs:36](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/src/manager.rs:36)). That creates a third, irrelevant lock and leaves the actual files to be claimed later by restore.
- **HIGH — Continuing after `StateLockError::Held` regresses validated single-writer behavior.** Today the TUI exits immediately and names the holder at [baude/src/main.rs:370](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/main.rs:370). If startup continues, restore reports blocked persistence because the real loader also claims the state lock at [baude-core/src/persist.rs:338](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/persist.rs:338). This recreates the degraded-start behavior that issue #71 removed.
- **HIGH — Derivation precedence contradicts itself.** The plan says derivation occurs only when `backend_env` is absent, but its required order places derived repository names before `BAUDE_BACKEND`. The current resolver combines workspace-name selection and backend selection at [baude-core/src/workspace.rs:144](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/workspace.rs:144) and [baude-core/src/workspace.rs:159](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/workspace.rs:159); the new implementation must explicitly separate those decisions.
- **HIGH — The proposed startup tests conflict with the test-support identity model.** Production uses a process-wide `OnceLock`, but test builds deliberately do not write it ([baude-core/src/workspace.rs:199](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/workspace.rs:199)). Test initialization instead requires an already-live workspace override and otherwise asserts at [baude-core/src/workspace.rs:288](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/workspace.rs:288). “Call `start_workspace`, then assert ACTIVE was seeded” is therefore not a valid in-process unit-test contract.
- **MEDIUM — `StartedWorkspace` cannot own the lock as described.** The lock API returns `Result<(), _>` and retains file handles in the global `HELD_STATE_LOCKS` map ([baude-core/src/persist.rs:470](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/persist.rs:470), [baude-core/src/persist.rs:607](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/persist.rs:607)). A `lock_held: bool` reports status but is not an RAII guard, so the plan’s “returned to caller” and “must not drop” language is inaccurate.
- **MEDIUM — Repository-root canonicality is assumed, not guaranteed by the helper.** `git::repo_root` returns Git stdout directly without canonicalizing it at [baude-core/src/git.rs:1734](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/git.rs:1734). Binding keys canonicalize later, but the plan should make the stored `repo_root` contract explicit.
- **MEDIUM — Home-boundary correctness needs canonical paths on both sides.** Launch directories are canonicalized, while `persist::home_dir()` returns the redirected or OS home path without canonicalization at [baude-core/src/persist.rs:914](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/persist.rs:914). A symlinked home can defeat a simple `current == home` stopping condition.
- **MEDIUM — Plan ownership is contradictory.** Plan 13-01 says the daemon should call the new helper, but `bauded/src/main.rs` is neither in its modified-file list nor scheduled until 13-03. This makes it unclear whether wave 1 must compile with one or two adopters.

### Suggestions

- Split the helper into:

  1. A pure `resolve_launch` returning the launch plan and resolved workspace.
  2. Per-binary lock/record orchestration using the correct base (`state` or `daemon-state`) and contention policy.

- Preserve the TUI’s current `Held => exit` behavior. `Io => continue degraded` may remain consistent with the current code.
- Define workspace-name selection separately from backend binding. In particular, a derived name should still win when `BAUDE_BACKEND` only chooses that derived workspace’s backend.
- Decide one source classification for all seven precedence levels, including `BAUDE_BACKEND` and config `backend`, and encode it in a table-driven resolver test.
- Canonicalize both the Git root and home boundary before walking or comparing.
- Test the pure resolver in-process; reserve production cache/startup assertions for subprocess tests or a dedicated test-only startup seam.

### Risk Assessment

**HIGH.** The intended feature architecture is sound, but the lock base and contention policy can invalidate the single-writer guarantee. The precedence and test-cache contradictions are also likely to produce either incorrect behavior or an unexecutable test plan.

## 13-02

### Summary

The test inventory is broad and requirement-oriented, especially around precedence and deduplication. However, the first task’s verification cannot execute the promised startup test, and the proposed lock-refusal cases cannot be produced reliably in one process with the current re-entrant lock implementation.

### Strengths

- The ancestor-walk cases cover nearest-wins, home inclusion, home stopping, no-match, and outside-home traversal—good coverage for the proposed extension to the current exact lookup at [baude-core/src/folder_workspace.rs:84](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/folder_workspace.rs:84).
- Replacing the old backend-suppression expectation is necessary. The existing test explicitly asserts that `BAUDE_BACKEND` suppresses hints at [baude-core/src/workspace.rs:756](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/workspace.rs:756).
- The root-versus-subfolder admission test exercises the correct identity mechanism: repository rows are matched by `observed_common_dir` at [baude/src/app.rs:1917](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/app.rs:1917).
- Avoiding process-wide environment mutation is appropriate because the test harness is parallel and the repository already uses thread-local redirects for isolation at [baude-core/src/testing.rs:82](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/testing.rs:82).

### Concerns

- **HIGH — Task 1’s command filters out its own startup test.** The test is specified in `launch.rs::tests`, but the command runs only `folder_workspace::tests`. Since `launch.rs` is a separate module and does not currently exist, that invocation cannot run `test_wspc02_start_workspace_derives_persists_recalls`; the plan then greps for a different name, `test_wspc02_end_to_end`.
- **HIGH — A same-process `Held` test will not work.** `hold_state_lock` treats a path already held by this process as successful at [baude-core/src/persist.rs:574](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/persist.rs:574). Testing genuine contention therefore requires another process or a test-only injected lock failure.
- **HIGH — “ACTIVE was cached” remains incompatible with support builds.** `active()` reads a thread-local fixture override in tests at [baude-core/src/workspace.rs:319](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/workspace.rs:319), not the production cache. The planned assertion must be rewritten around that behavior.
- **MEDIUM — The precedence expectations disagree with 13-01.** This plan classifies config backend as `Explicit`, while 13-01 says config backend is not a separate source and otherwise describes fallback as `Default`. Tests cannot stabilize an undefined contract.
- **MEDIUM — Repository admission is not a lightweight row-only operation.** It resolves the default branch, may create a managed worktree, persists state, and activates a runtime at [baude/src/app.rs:1949](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/app.rs:1949) through [baude/src/app.rs:2052](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/app.rs:2052). The test needs the existing spawn/persistence stubs, not merely a temporary Git repository.
- **LOW — This is regression testing rather than strict TDD.** Most implementation is delivered in 13-01, so the described RED→GREEN loop in wave 2 cannot drive the feature’s initial design.

### Suggestions

- Run launch and folder-workspace tests separately, using their exact module paths and exact test names.
- Use a subprocess for real lock contention, matching existing production semantics, or add a narrow test-only failure injection at the lock boundary.
- Test source resolution through pure `resolve_with_context`; separately test fixture initialization through `override_for_test_with_source`.
- Reuse existing app fixture helpers and spawn stubs when testing `admit_repository`.
- Add a linked-worktree case for `OPEN-01`, since `git rev-parse --show-toplevel` intentionally returns the selected worktree root, as noted at [baude/src/app.rs:2814](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/app.rs:2814).

### Risk Assessment

**MEDIUM-HIGH.** Coverage goals are strong, but several promised tests either will not run or cannot stimulate the claimed behavior with existing APIs. Correcting the lock and identity seams would reduce this to medium or low.

## 13-03

### Summary

Daemon parity and documentation belong in this phase, and exposing provenance through `/info` is the right API direction. The plan does not yet trace that response into persistent remote UI state, however, and it inherits the critical startup-lock defect from 13-01.

### Strengths

- `/info` is the correct endpoint to extend: it already represents daemon workspace identity and currently returns workspace, backend, version, and persistence state at [bauded/src/api.rs:117](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/src/api.rs:117).
- Adding source provenance is backward-compatible for JSON consumers that ignore unknown fields. The existing remote client already treats missing newer session fields through `#[serde(default)]`, as demonstrated at [baude/src/remote.rs:30](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/remote.rs:30).
- The documentation work correctly targets stale text: README currently states that either workspace or backend environment variables suppress folder memory at [README.md:287](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/README.md:287).
- Clone destination behavior is genuinely isolated from new-session prefill. It remains in `prompt_clone_dest`, using `clone_base_dir`, at [baude/src/app.rs:4701](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/app.rs:4701).

### Concerns

- **HIGH — Remote provenance has no complete data path.** `/info` is currently fetched only inside `daemon_workspace()` during session creation, reduced to `Option<String>`, and discarded at [baude/src/remote.rs:131](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/remote.rs:131). `RemoteSnapshot` contains only sessions, fetch time, and online status at [baude/src/remote.rs:67](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/remote.rs:67), while `remote_header` sees only `app.remote_snap` at [baude/src/ui.rs:656](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/ui.rs:656). “Store it in `RemoteContext or similar`” is not executable because no such type exists.
- **HIGH — It inherits the wrong shared lock base and weakened contention policy.** The daemon’s actual state loader uses `daemon-state` at [bauded/src/manager.rs:444](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/src/manager.rs:444). A shared helper claiming `workspace-state` neither protects it nor proves parity.
- **HIGH — The parity test does not test two startup paths.** Both mains are entry-point code, not callable functions. Calling `start_workspace` twice tests one helper twice; it does not verify that both binaries canonicalize arguments, supply the correct lock base, and retain the returned state. Current startup differences are visible in [baude/src/main.rs:343](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/main.rs:343) and [bauded/src/main.rs:166](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/src/main.rs:166).
- **MEDIUM — Remote default formatting does not match the local contract.** Local `title_label()` is planned to render only `(blank)` for default, but the remote format is fixed as `remote: {name} ({source})`, producing `remote: claude (blank)`. That contradicts the stated “matching local title format.”
- **MEDIUM — Older daemon behavior is unspecified.** An older `/info` response lacks `workspace_source`. The new client should deserialize a typed optional/default field and select an explicit fallback rather than assuming the field exists.
- **MEDIUM — Canonicalization behavior differs from the TUI.** The TUI currently falls back to the original path when canonicalization fails at [baude/src/main.rs:343](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/main.rs:343); the daemon action proposes propagating canonicalization failure with `?`. “Same rule for the same launch directory” should use identical behavior.
- **LOW — Documentation verification is mostly keyword-based.** The proposed grep/awk checks cannot prove the seven entries occur in the correct order or that clone documentation was unchanged.

### Suggestions

- Introduce a typed `DaemonInfo` such as `{ workspace, backend, workspace_source: Option<_> }`.
- Fetch `/info` when `RemotePoller` starts or alongside `/sessions`, store the result in `RemoteSnapshot`, and render from that stable snapshot. Preserve the existing cross-workspace guard using the same typed response.
- Define the old-daemon fallback explicitly—likely source unavailable rather than falsely labeling it explicit or derived.
- For genuine binary parity, extract thin TUI/daemon argument-to-startup-input functions or use subprocess smoke tests. Do not call the same helper twice and label it cross-binary coverage.
- Generate README precedence from a numbered list and test its ordered markers, rather than only grepping for words.

### Risk Assessment

**HIGH.** Daemon resolution is necessary, but the current plan lacks the actual `/info → poller → app snapshot → UI` chain and relies on the unsafe lock design from 13-01.

## Cross-plan assessment

Overall risk is **HIGH until revised**. The plans cover every phase requirement conceptually, but three design contracts must be fixed before execution:

1. Lock the actual state base and preserve the TUI’s refusal-on-contention behavior.
2. Specify one non-contradictory workspace/source precedence table, separating workspace naming from backend binding.
3. Align tests with the thread-local test identity system and use subprocesses or injection for real lock contention.

After those corrections, the phase is well-bounded: repository identity and clone behavior already have suitable seams, and the remaining implementation is primarily resolver plumbing, UI labeling, and documentation.

## Convergence Outcome (2026-09-19)

| Cycle | Reviewer | current_high | current_actionable | Replan commit |
|-------|----------|--------------|--------------------|---------------|
| 1 | codex | 14 | 11 | 8f08072 |
| 2 | codex | 8 | 13 | 9f4def0, 320d4a5 |
| 3 | codex | 12 | 8 | 27f36ee, 24aa141 |
| 4 (extra, user-approved) | codex | 10 | 9 | b9b8dda (targeted: all 10 HIGHs) |

Not converged to zero. Each cycle surfaced new source-level mismatches rather than regressions. After cycle 4 Joe chose "fix the design-level HIGHs, then execute" over a fifth cycle: the ten cycle-4 HIGHs were made true in the plans (lock base parameter with `"state"`/`"daemon-state"`, refuse startup on a held lock per #71, behavior assertions instead of cache-seeding checks, corrected test module routing, injected lock-refusal test, complete remote provenance path, two-call-site parity) and the nine actionable items were incorporated or dispositioned in each plan's Review Dispositions ledger. The plan checker passed at b9b8dda. Execution proceeds without a fifth Codex review; executors compile against the real code and the verifier checks must-haves.
