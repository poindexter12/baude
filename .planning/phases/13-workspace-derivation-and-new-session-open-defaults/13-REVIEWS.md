---
phase: 13
reviewers: [codex]
reviewed_at: 2026-09-19T20:40:00Z
plans_reviewed: [13-01-PLAN.md, 13-02-PLAN.md, 13-03-PLAN.md]
models:
  codex: "gpt-5.6-sol"
model_sources:
  codex: "banner"
---

# Cross-AI Plan Review — Phase 13

<!-- gsd:plan-revision-conflicts:begin -->
## Plan-Revision Conflicts
<!-- gsd:plan-revision-conflicts:end -->

Overall verdict: not converged. The plans have good requirement coverage and a sensible shared-resolver direction, but several executable blockers remain. Cycle 3 should not proceed unchanged: 13-01 and 13-03 would either fail to compile or leave `workspace::active()` uninitialized, and 13-02/13-03 contain test filters that can pass while running zero tests.

## 13-01

### Summary

The plan correctly identifies the main integration seams: folder memory, repository-root derivation, workspace resolution, startup recording, new-session prefill, and title rendering. However, its proposed API transition is incompatible with the live workspace initialization model. As written, it bypasses the process-wide workspace cache, uses nonexistent fields/signatures, and omits a source file that must change when `Workspace` gains a field.

### Strengths

- Repository admission already deduplicates by canonical Git common directory, so OPEN-03 is built on the right identity mechanism: `discover_repository()` canonicalizes `--git-common-dir`, and `admit_repository()` matches `observed_common_dir` before allocating a repository key. [git.rs:321](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/git.rs:321), [app.rs:1914](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/app.rs:1914)
- Recording at repository root rather than launch subfolder is the correct stabilization mechanism. The current implementation records the exact launch directory, explaining why this change is necessary. [folder_workspace.rs:114](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/folder_workspace.rs:114), [main.rs:398](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/main.rs:398)
- Moving `BAUDE_BACKEND` below binding/config/derivation is explicitly addressed at both lookup and resolver seams. Currently it suppresses folder lookup and hints. [folder_workspace.rs:67](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/folder_workspace.rs:67), [workspace.rs:125](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/workspace.rs:125)
- The new-session change is tightly scoped. The current modal selects `new_session_dir` before launch context, while `git::repo_root()` already provides the required toplevel operation. [app.rs:4057](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/app.rs:4057), [git.rs:1734](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/git.rs:1734)

### Concerns

- **HIGH — Replacing `initialize()` with `resolve_with_context()` breaks the active workspace cache.** `initialize()` is the sole production writer to `ACTIVE`; `active()` panics if that initialization did not occur. The next startup step, `ensure_daemon()`, immediately reads `active()`. A pure `resolve_with_context() -> Workspace` cannot replace it. [workspace.rs:269](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/workspace.rs:269), [workspace.rs:304](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/workspace.rs:304), [main.rs:414](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/main.rs:414), [main.rs:28](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/main.rs:28)
- **HIGH — The claimed backward-compatible `resolve_with_hint` signature is not backward compatible.** The live function takes five arguments—workspace env, backend env, hint, config, and warning callback—and is also used by `resolve`, fixtures, and tests. The plan describes a two-argument wrapper instead. [workspace.rs:116](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/workspace.rs:116), [workspace.rs:132](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/workspace.rs:132), [workspace.rs:230](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/workspace.rs:230)
- **HIGH — The recording call references a nonexistent `config.root`.** `Config` contains user options but no filesystem root; the existing startup obtains the root through `persist::config_dir()`. In addition, `record()` expects `Option<&Path>`, not a plain path. [persist.rs:940](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/persist.rs:940), [main.rs:358](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/main.rs:358), [folder_workspace.rs:118](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/folder_workspace.rs:118)
- **HIGH — Adding `Workspace.source` requires another production file modification.** `worktree_scan.rs` constructs `Workspace` directly with a struct literal; adding a required field causes compilation failure. It is absent from this plan’s `files_modified`. [worktree_scan.rs:430](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/worktree_scan.rs:430)
- **HIGH — The prescribed ownership of `LaunchPlan` fields produces a partial-move problem.** The plan moves `plan.hint` and `plan.repo_root` into `WorkspaceLaunchContext`, then later reads `plan.repo_root` for recording. Because both are owned values, the later access will not compile without cloning, borrowing, or destructuring into separate locals. The current `LaunchPlan` already owns its hint, and the proposed root is likewise owned. [folder_workspace.rs:59](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/folder_workspace.rs:59)
- **MEDIUM — `title_label()` does not necessarily show the workspace name.** The plan builds the title from `display_label()`, but that method intentionally returns `Claude Code` or `opencode` when workspace name equals backend, and returns `name · backend` for custom names. That is different from the required active workspace name. [workspace.rs:73](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/workspace.rs:73)
- **MEDIUM — Source classification is internally inconsistent.** The task logic says no hint/no repo becomes `Default`, while 13-02 expects `BAUDE_BACKEND` and config backend to be `Explicit`. The live resolver treats those values as workspace-name inputs, so this needs an explicit decision rather than an implicit fallback. [workspace.rs:144](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/workspace.rs:144)

### Suggestions

- Add `initialize_with_context(config, context) -> &'static Workspace`, preserving the existing production and test-support cache behavior. Make `initialize(config, hint)` a compatibility wrapper.
- Preserve the current five-argument pure resolver API, or introduce `resolve_with_context(ws_env, backend_env, context, config, warn)` and keep `resolve_with_hint(...)` unchanged.
- Use `memory_root = persist::config_dir()` and pass `Some(&memory_root)` to `record()`.
- Destructure the launch plan before resolution, cloning only if truly necessary:
  `let LaunchPlan { hint, repo_root, notes } = plan;`
- Update `worktree_scan.rs`, preferably through a constructor that prevents future struct-literal breakage.
- Build the title from `Workspace.name`; append backend display separately only if desired.
- Define all seven source outcomes in one table, including `BAUDE_BACKEND` and config backend.

### Risk Assessment

**HIGH.** The pure-resolver substitution would panic during startup, and the live signatures and struct construction guarantee compilation failures unless the plan is corrected.

## 13-02

### Summary

The planned test matrix is broad and generally maps well to WSPC-01/02/03/05 and OPEN-01/02/03. Its most serious problem is mechanical: most verification filters do not match the proposed test names, so Cargo can report success after running zero tests. Some “end-to-end” cases also stop below the binary startup seams where the highest-risk bugs reside.

### Strengths

- The ancestor-walk matrix covers nearest-wins, home inclusion, home boundary, outside-home behavior, and missing bindings. This fills the narrow exact-match coverage in the current implementation. [folder_workspace.rs:71](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/folder_workspace.rs:71)
- The full precedence matrix is necessary because existing tests explicitly encode the old rule that `BAUDE_BACKEND` suppresses hints. [workspace.rs:756](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/workspace.rs:756)
- The root-versus-subfolder admission test targets the real deduplication predicate rather than path-string equality. [app.rs:1915](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/app.rs:1915)
- Rendering through the existing `TestBackend` helper is an appropriate way to validate visible title text. [ui.rs:2482](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/ui.rs:2482)

### Concerns

- **HIGH — The automated filters will run zero matching tests.** For example, the plan names `test_find_binding_at_parent` but invokes the filter `folder_workspace::tests::find_binding`; the intervening `test_` means the substring does not match. The same mismatch affects `test_precedence_matrix...`, `test_display_hint...`, `test_open_new_session...`, and `test_daemon_startup...`. Cargo exits successfully when zero tests match.
- **MEDIUM — The WSPC-02 “end-to-end” test bypasses the actual startup integration.** A test inside `folder_workspace.rs` that manually calls planning, resolution, recording, and planning again cannot catch the 13-01 failure to install `workspace::active()` or its incorrect recording root/signature. The real integration currently occurs in `baude/src/main.rs`. [main.rs:349](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/main.rs:349)
- **MEDIUM — Environment mutation is unnecessary and risks parallel-test interference.** Both `plan_launch()` and the pure workspace resolver already accept environment values as parameters. Tests should pass `Some("opencode")` rather than mutate `BAUDE_BACKEND` globally. [folder_workspace.rs:71](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/folder_workspace.rs:71), [workspace.rs:132](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/workspace.rs:132)
- **MEDIUM — The empty-name guard is not actually verified.** `test_sanitize_empty_input` proves only that sanitization returns an empty string. It does not prove resolution falls through or that no empty binding is persisted.
- **MEDIUM — Source fixture support is underspecified.** The current `override_for_test()` accepts only config and a binding hint, so it cannot construct a derived source. “Extend it or add a fixture seam” is too open-ended for a final-cycle plan. [workspace.rs:242](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/workspace.rs:242)
- **LOW — The deduplication assertion should exercise the public admission route.** Direct `admit_repository()` verifies identity reuse, but focus behavior is completed through `open_repo_session_via()`, which sets focus and records context after admission. [app.rs:4727](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/app.rs:4727)

### Suggestions

- Use filters guaranteed to match, such as `find_binding`, `test_precedence_matrix`, or exact names including `test_`. Add `-- --exact` where appropriate.
- Require each verification to show a nonzero test count, or run the complete module/package test target.
- Add a startup helper that performs “plan → initialize-with-context → lock → record” and test that helper directly.
- Pass env inputs explicitly rather than calling `set_var`.
- Add a test proving an empty derived name neither becomes the workspace nor reaches `record()`.
- Define one explicit test-only constructor for `WorkspaceSource` variants.
- Test selection/focus via `open_repo_session_via()` in addition to direct repository-count assertions.

### Risk Assessment

**MEDIUM.** The proposed behavioral coverage is good, but the current commands provide false-green results and the tests miss the startup-cache and recording integration where the largest failures can occur.

## 13-03

### Summary

Daemon parity and documentation are correctly assigned to a final integration plan, but the daemon task is not executable against the live persistence API. Its lock strategy assumes a returned guard that does not exist, continues after lock refusal into guaranteed persistence failure, and verifies parity with a tautological core-only test. It also explicitly defers daemon source provenance despite the phase decision requiring daemon-backed views to show the same source information.

### Strengths

- The plan correctly recognizes that current daemon startup has no launch-folder planning at all; it initializes with `None` immediately before manager restoration. [bauded/main.rs:174](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/src/main.rs:174)
- Canonicalizing daemon `current_dir()` before repository discovery is appropriate and mirrors TUI startup.
- Recording before any daemon state mutation is directionally sound.
- The README scope is comprehensive and preserves clone destination semantics, which currently live in a separate documented flow. [README.md:298](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/README.md:298)

### Concerns

- **HIGH — Daemon startup repeats the cache-initialization failure from 13-01.** Calling `resolve_with_context()` instead of `initialize()` leaves `workspace::active()` unset; `Manager::new`, `restore()`, API handlers, and polling rely on it. [bauded/main.rs:174](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/src/main.rs:174), [manager.rs:444](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/src/manager.rs:444)
- **HIGH — The proposed lock call is incompatible with the real API.** `claim_workspace_state_lock()` takes `&Workspace`, not `&ws.name`, returns `Result<(), StateLockError>`, and stores the acquired file lock in a process-global map. There is no `_lock_guard` to hold or release around `Manager::restore()`. [persist.rs:531](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/persist.rs:531), [persist.rs:568](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/persist.rs:568)
- **HIGH — Continuing after `Held` is unsafe and contradicts the persistence mechanism.** `Manager::restore()` immediately calls a loader that acquires the same daemon-state lock. If another process holds it, restoration fails, the manager marks persistence blocked, and subsequent saves also fail. The result is a second live daemon that can serve requests but cannot safely persist them. [persist.rs:338](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/persist.rs:338), [manager.rs:474](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/src/manager.rs:474), [persist.rs:611](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/persist.rs:611)
- **HIGH — `config.root` is again nonexistent and `record()` requires `Option<&Path>`.** [persist.rs:940](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/persist.rs:940), [folder_workspace.rs:118](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/folder_workspace.rs:118)
- **HIGH — The parity test is tautological.** Calling `canonicalize → plan_launch → resolve_with_context` twice inside `baude-core` does not exercise `bauded/src/main.rs`, lock handling, recording, or initialization. It would still pass if daemon startup remained unchanged at `initialize(&config, None)`. [bauded/main.rs:181](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/src/main.rs:181)
- **HIGH — WSPC-05 daemon-backed provenance remains uncovered.** `/info` returns workspace name and backend only, and the client reads only the workspace name. The plan’s explicit provenance deferral means a remote/daemon-backed view cannot know whether the daemon chose its workspace explicitly, from a binding, or by derivation. [api.rs:117](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/src/api.rs:117), [remote.rs:131](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/remote.rs:131)
- **MEDIUM — The README rebinding guidance is misleading.** Config `workspace` is below an existing folder binding, so setting it alone cannot override that binding. The existing README reflects the old suppression behavior and requires a careful rewrite. [README.md:287](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/README.md:287)
- **MEDIUM — README grep verification checks presence, not order or accuracy.** A document with an incorrectly ordered precedence list would still pass.

### Suggestions

- Reuse `initialize_with_context()` in daemon startup.
- Call `claim_workspace_state_lock("daemon-state", workspace)`. Treat `Held` as startup refusal, matching the single-writer invariant. Decide explicitly whether `Io` preserves the existing degraded behavior.
- Use `persist::config_dir()` and `Some(&memory_root)` for folder-memory calls.
- Extract a shared `resolve_launch(config, launch_dir, env)` helper in `baude-core`, then have both binaries invoke it. Test each binary-facing wrapper or at least a factored daemon startup helper.
- Add a daemon test proving:
  - ancestor binding selection;
  - derivation and repository-root recording;
  - no recording for non-repository launches;
  - no binding write after state-lock refusal.
- Either add `workspace_source` to `/info` and consume it in the TUI, or explicitly revise WSPC-05/CONTEXT.md. Silent deferral is not convergence.
- Verify README precedence structurally, or review the rendered section rather than relying only on keyword greps.

### Risk Assessment

**HIGH.** The lock and initialization mechanisms do not match the source, the proposed parity test cannot detect incorrect daemon wiring, and one locked phase behavior—daemon-backed workspace provenance—is knowingly left incomplete.

Final recommendation: revise before execution. The minimum convergence fixes are an `initialize_with_context` API, correct persistence/config-root calls, explicit source classification, updates for every `Workspace` constructor, nonzero-test verification, a real daemon-startup test seam, and resolution of the daemon provenance requirement.
