---
phase: 13
reviewers: [codex]
reviewed_at: 2026-09-20T03:00:53Z
plans_reviewed: [13-01-PLAN.md, 13-02-PLAN.md, 13-03-PLAN.md]
models:
  codex: "unknown"
model_sources:
  codex: "unknown"
---

# Cross-AI Plan Review — Phase 13

## 13-01

### Summary

The plan identifies the correct modules, but its proposed data flow cannot implement repository-derived workspaces as written. The current startup API carries only one undifferentiated folder hint, records the launch directory rather than the repository root, and suppresses that hint when `BAUDE_BACKEND` is set. Because `baude/src/main.rs` is excluded from the files modified, the plan cannot supply or persist the additional derivation information required by WSPC-02.

### Strengths

- The new-session change targets the correct seam. `open_new_session_modal()` currently prefers `new_session_dir` and otherwise uses `launch_dir`, so inserting `git::repo_root(&self.launch_dir)` there directly addresses OPEN-01/02 ([app.rs:4057](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/app.rs:4057)). The existing helper invokes `git rev-parse --show-toplevel` ([git.rs:1734](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/git.rs:1734)).
- The proposed ancestor lookup builds on the correct canonical key mechanism: `folder_key()` canonicalizes before serializing the path ([breadcrumbs.rs:114](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/breadcrumbs.rs:114)).
- The UI location is appropriate. The sidebar block owns the existing top title ([ui.rs:218](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/ui.rs:218)), while the status line already displays `workspace::active().display_label()` ([ui.rs:1275](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/ui.rs:1275)).
- The existing TUI startup claims the workspace state lock before recording folder memory ([main.rs:370](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/main.rs:370), [main.rs:398](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/main.rs:398)), which is a sound ordering to preserve.
- Workspace sanitization already implements the required ASCII-safe substitution policy ([workspace.rs:99](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/workspace.rs:99)).

### Concerns

- **HIGH — Repository derivation has no executable data path.** `LaunchPlan` currently contains only `hint` and `notes` ([folder_workspace.rs:59](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/folder_workspace.rs:59)); `plan_launch()` receives neither a repository root nor a home boundary ([folder_workspace.rs:71](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/folder_workspace.rs:71)). The plan does not modify `baude/src/main.rs`, even though that is where the repository root must be discovered and where the resolved inputs are assembled.
- **HIGH — Derived bindings would be written under the wrong key.** Startup currently records `launch_dir -> workspace` ([main.rs:406](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/main.rs:406)). WSPC-02 requires a derived binding keyed by the repository root. Calling `find_binding()` does not change the recording target, and the plan explicitly says recording remains unchanged.
- **HIGH — The proposed resolver cannot distinguish a bound workspace from a derived workspace.** `resolve_with_hint()` accepts only `Option<&str>` ([workspace.rs:132](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/workspace.rs:132)). Passing either result as `hint` makes it impossible to assign `WorkspaceSource::Bound` versus `WorkspaceSource::Derived`. The action mentions “derived repo root provided,” but proposes no parameter or structured input for it.
- **HIGH — The existing precedence contradicts the locked Phase 13 chain.** `plan_launch()` refuses to consult memory whenever `BAUDE_BACKEND` exists ([folder_workspace.rs:78](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/folder_workspace.rs:78)), and `resolve_with_hint()` also suppresses hints under `BAUDE_BACKEND` ([workspace.rs:139](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/workspace.rs:139)). The declared new order places a folder binding and derivation ahead of `BAUDE_BACKEND`. Plan 01 neither explicitly removes both suppressions nor tests the changed rule.
- **HIGH — The default title violates WSPC-05.** Formatting `display_label() + display_hint()` will render `Claude Code (default)`, because `display_label()` maps the implicit `claude` workspace to “Claude Code” ([workspace.rs:73](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/workspace.rs:73)). The requirement calls for `(blank)` when no explicit, bound, or derived workspace applies.
- **MEDIUM — Source classification is underspecified.** The enum has only `Explicit`, `Bound`, `Derived`, and `Default`, but the action does not clearly classify config `workspace`, `BAUDE_BACKEND`, or config `backend`. This matters because the context says explicit config should be visible, while backend-only fallback should not masquerade as repository derivation.
- **MEDIUM — Home-boundary resolution must use the guarded resolver.** The repository already provides `persist::home_dir()`, which honors `TestRedirect` ([persist.rs:904](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/persist.rs:904)). The plan should explicitly use this instead of `dirs::home_dir()` or an unthreaded ambient home.
- **MEDIUM — The automated verification does not exercise the tracer.** It runs one pre-existing exact-match memory test ([folder_workspace.rs:147](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/folder_workspace.rs:147)). It does not compile the TUI changes, test derivation, inspect the title, or exercise the modal.
- **MEDIUM — The manual smoke procedure mutates real user state.** Launching production `baude` from the supplied worktree invokes the real config root and records folder memory after locking ([main.rs:360](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/main.rs:360), [main.rs:406](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/main.rs:406)). This conflicts with the verification statement that `TestRedirect` was active.

### Suggestions

- Replace the untyped `hint` with a structured resolution input, for example `LaunchSelection::{Bound, Derived}`, plus a `record_path` or `repo_root`.
- Make `plan_launch()` perform one coherent operation:

  1. Find the nearest ancestor binding.
  2. If absent, discover the repository root.
  3. Construct a sanitized derived candidate.
  4. Return both source and repository-root recording target.

- Change TUI startup so derived bindings are recorded against `repo_root`; preserve existing explicit-launch recording semantics separately if desired.
- Encode and test the exact precedence in one resolver: `BAUDE_WORKSPACE > bound > config workspace > derived > BAUDE_BACKEND > config backend > default`.
- Add a title-specific method that renders `(blank)` for the implicit default instead of composing `display_label()` mechanically.
- Replace the verification command with targeted core tests plus `cargo test -p baude ...` for modal/UI behavior. Run manual smoke under isolated `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, and `HOME`.

### Risk Assessment

**HIGH.** The UI and modal portions are straightforward, but the central WSPC-02 mechanism is absent from the proposed interfaces and file list. Executing the plan literally would likely produce ancestor lookup and source labels without reliable repository derivation or root-level persistence.

## 13-02

### Summary

The test plan names many important behaviors, but its tasks do not cover several of its own must-haves. In particular, there is no real precedence/derivation test matrix, no UI rendering test for WSPC-05, and no concrete repository deduplication test. As a result, it would not catch the principal architectural defects in Plan 01.

### Strengths

- Parent, grandparent, nearest-wins, home-boundary, and outside-home cases are the right ancestor-walk matrix.
- The proposed fixtures can use the repository’s guarded home redirect: `TestRedirect::new()` supplies `<fixture>/home`, config, and data roots ([testing.rs:114](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/testing.rs:114)).
- The app already has robust isolated repository fixtures that retain both the filesystem redirect and workspace identity ([app.rs:6866](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/app.rs:6866), [app.rs:6950](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/app.rs:6950)).
- Repository identity is genuinely based on the canonical Git common directory ([git.rs:321](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/git.rs:321)), and `admit_repository()` deduplicates on that value ([app.rs:1914](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/app.rs:1914)).
- Existing re-admission coverage already demonstrates idempotence for the same path ([app.rs:8404](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/app.rs:8404)), providing a useful pattern for the new root-versus-subfolder case.

### Concerns

- **HIGH — The listed precedence behaviors have no corresponding task.** The must-haves require explicit workspace, binding, config, derived, and backend precedence, but Task 2 only checks four `display_hint()` strings. Existing tests encode the old behavior in which `BAUDE_BACKEND` suppresses a hint ([workspace.rs:757](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/workspace.rs:757)). Without replacing that test with the complete Phase 13 matrix, the old rule may remain green.
- **HIGH — WSPC-02 is not tested end-to-end.** No task verifies “no binding → repository-root basename → sanitized derived workspace → root binding persisted.” Testing `find_binding()` alone cannot establish derivation or recording.
- **HIGH — OPEN-03 is promised but not concretely implemented.** Task 3’s RED list contains only modal-prefill cases. The behavior mentions admission integration, but there is no named test, setup, or assertion for repository count, checkout count, runtime count, selection, or focus.
- **HIGH — WSPC-05 lacks a rendering test.** The repository already has a `TestBackend` render helper ([ui.rs:2482](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/ui.rs:2482)), yet `ui.rs` is absent from Plan 02’s files and tasks. A unit test of `display_hint()` would not catch a missing title integration or the incorrect `Claude Code (default)` output.
- **MEDIUM — “Daemon startup” appears in the must-haves but is not exercised.** No daemon file is modified or compiled by the listed tasks. That assertion belongs in Plan 03 and needs a daemon-facing seam.
- **MEDIUM — The sanitization requirements are internally inconsistent.** The plan says to reuse the existing `sanitize()`, which returns an empty string for empty input ([workspace.rs:101](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/workspace.rs:101)), but the must-haves demand a placeholder or hyphen. No task defines the intended new behavior or its compatibility impact.
- **MEDIUM — “Always ends with `/`” changes existing behavior.** The current no-config fallback uses `launch_dir.display()` without a slash ([app.rs:4063](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/app.rs:4063)). The plan should call this an intentional normalization, not “consistency with existing behavior.”
- **MEDIUM — The combined verification command is malformed.** `cargo test` accepts one positional test filter; `cargo test -p baude-core workspace:: folder_workspace:: --lib` should be split into two invocations or run without filters.

### Suggestions

- Add a table-driven resolver test covering every precedence position and expected `WorkspaceSource`, including the declared relationship between binding/derivation and `BAUDE_BACKEND`.
- Add an end-to-end core test that creates a repository with a hostile/unicode basename, resolves from a child directory, verifies the derived name, records at the repository root, and then resolves from a different child through the binding.
- Add a UI render test for explicit, bound, derived, and blank-default titles using the existing `UiFixture` and `render()` helper.
- Implement OPEN-03 using one `App`, not two:

  - Admit the repository root.
  - Admit a child directory through `open_repo_session_via()` or the startup route.
  - Assert one repository, stable checkout/runtime counts, unchanged keys, and selected/focused existing row.

- Remove daemon parity from this plan unless it adds a daemon-specific test seam.
- Decide whether empty workspace sanitization changes globally; if not, remove that unrelated must-have.

### Risk Assessment

**HIGH.** The plan gives an impression of broad coverage, but the actual test tasks omit the most failure-prone behaviors: derivation, precedence, root-level recording, title rendering, and subfolder deduplication.

## 13-03

### Summary

Adding daemon startup parity and updating documentation are appropriate Wave 3 activities, but the daemon work is incomplete and its proposed test is tautological. The plan also requires an `app.rs` integration test while omitting that file from both its frontmatter and task ownership. Most importantly, it explicitly declines to record daemon-derived bindings, contradicting WSPC-04 and the locked context.

### Strengths

- The plan correctly identifies that the daemon currently initializes with no folder hint ([bauded/main.rs:174](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/src/main.rs:174)).
- Reusing the same `baude-core` resolution path is the right architectural direction; both binaries already depend on `workspace::initialize()`.
- The README locations are appropriate. Current folder-memory documentation describes only an exact-folder memory model and the old env suppression rule ([README.md:267](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/README.md:267)), while the configuration section still describes `new_session_dir` as the unconditional prefill ([README.md:407](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/README.md:407)).
- Clone behavior is cleanly separated from the new-session prefill. Clone destination selection already uses `clone_base_dir` in a separate path, and the README documents it independently ([README.md:298](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/README.md:298)).
- The existing daemon `/info` endpoint exposes its workspace, and the TUI has a cross-workspace guard ([api.rs:117](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/src/api.rs:117), [remote.rs:148](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/remote.rs:148)). This provides a useful basis for daemon parity validation.

### Concerns

- **HIGH — Daemon-derived bindings are not recorded.** The locked decision requires `bauded` to record derived bindings through the same code path and file. The plan instead says “Do NOT change … recording logic,” but no daemon recording logic exists: startup only loads config and initializes the workspace ([bauded/main.rs:181](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/src/main.rs:181)).
- **HIGH — The parity test proves nothing about daemon wiring.** Calling `plan_launch()` twice in `baude-core` with identical arguments only proves determinism. It would still pass if `bauded/src/main.rs` never called the function. A daemon helper, binary-level test, or shared startup resolver must be exercised.
- **HIGH — The repository test’s file ownership is inconsistent.** The action requires changes in `baude/src/app.rs`, but `files_modified` and the task’s `<files>` list contain only `bauded/src/main.rs` and `baude-core/src/folder_workspace.rs`. An executor following ownership metadata may omit OPEN-03 entirely.
- **HIGH — The proposed two-app deduplication design is invalid.** Deduplication happens inside one app’s `repository_state` by matching `observed_common_dir` ([app.rs:1917](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/app.rs:1917)). Two fresh app instances do not share that in-memory state, so “the second admit sees the existing row” is false unless the first state is persisted and restored explicitly. Focus behavior also lives in `open_repo_session_via()` ([app.rs:4727](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/app.rs:4727)), not in a cross-instance identity comparison.
- **HIGH — The daemon has no launch-directory setup to mirror.** Its startup parses `--bind` and proceeds directly to config loading ([bauded/main.rs:166](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/src/main.rs:166)). The plan says to act “after daemon canonicalizes launch_dir (if not already done),” but this step does not exist. It must explicitly define `current_dir()` semantics and canonicalization.
- **MEDIUM — Binding write ordering is unresolved for the daemon.** `Manager::restore()` obtains durable state through `load_for_workspace()` and then saves ([manager.rs:444](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/src/manager.rs:444)); state loading claims the workspace lock internally. If daemon recording is added, the plan must ensure a lock refusal does not still teach folder memory.
- **MEDIUM — Documentation may encode an unresolved precedence contradiction.** The plan demands binding and derivation ahead of `BAUDE_BACKEND`, while existing source and tests treat `BAUDE_BACKEND` as suppressing memory ([workspace.rs:125](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/workspace.rs:125)). README work should follow a tested resolver contract, not precede resolution of that discrepancy.
- **LOW — `grep -c "precedence"` is not meaningful documentation validation.** It cannot confirm order, examples, kill-switch semantics, or preservation of clone behavior.

### Suggestions

- Extract a shared launch-resolution function in `baude-core` returning workspace candidate, source, repository root, recording target, and notes. Both binaries should call it.
- In `bauded`, explicitly derive `launch_dir` from `current_dir()`, canonicalize it, run the shared resolver, claim/validate the daemon workspace lock, and only then record a derived binding.
- Test daemon parity through a daemon-specific helper or subprocess that resolves the same fixture launch directory as the TUI helper. Assert workspace name, source, and on-disk root binding.
- Add `baude/src/app.rs` to the plan metadata and run the root/subfolder admission twice in one app.
- Add an API-level check that `/info` reports the derived workspace and consider how the source label is represented for a genuinely remote daemon-backed view.
- Replace the README grep with targeted assertions or a documentation review checklist tied to each required rule and example.

### Risk Assessment

**HIGH.** The documentation task is sound, but the daemon implementation and OPEN-03 validation are not credible as specified. WSPC-04 would remain only partially implemented.

## Cross-plan assessment

The three plans need replanning before execution. The principal issue is that Phase 13 is modeled as an extension of the existing single `hint`, but the requirements introduce two distinct lower-precedence inputs—ancestor binding and repository derivation—with different source labels and recording behavior.

A safer dependency structure would be:

1. **13-01:** Introduce a typed shared launch resolver, exact precedence, repository-root derivation, root binding persistence, and source-aware title formatting.
2. **13-02:** Add the complete resolver matrix, derivation/persistence round trip, UI rendering, modal prefill, and single-app root/subfolder admission tests.
3. **13-03:** Wire the same resolver into `bauded`, add meaningful daemon/API parity coverage, then update README from the tested contract.

**Overall risk: HIGH.** As written, the plans can compile portions of the feature while still failing WSPC-02, WSPC-04, WSPC-05, and OPEN-03.

---

## Consensus Summary

### Agreed Concerns

The review identifies consistent, critical gaps across all three plans:

1. **Repository derivation mechanism is architecturally incomplete (all three plans).** The shared problem is that `LaunchPlan` and `resolve_with_hint()` cannot carry the repository root data needed for WSPC-02 persistence. Plan 01 proposes no interface changes to convey derived bindings. Plans 02 and 03 do not test or wire this missing functionality.

2. **Precedence contradictions are unresolved across the board (all three plans).** The existing code suppresses memory hints under `BAUDE_BACKEND`, while the requirement places derived bindings ahead of `BAUDE_BACKEND`. No plan updates the suppression logic, and test coverage does not exercise the declared new precedence.

3. **Critical success criteria lack execution paths (all three plans).** WSPC-05 (title labeling) is not wired in Plan 01 or tested in Plan 02. WSPC-04 (daemon binding recording) is explicitly declined in Plan 03. OPEN-03 (deduplication) assumes cross-instance state sharing that does not exist.

4. **Workspace source classification is insufficiently typed (Plan 01 → Plans 02–03).** The enum distinguishes `Explicit`, `Bound`, `Derived`, `Default`, but the resolver cannot distinguish how each arose. This blocks title rendering and the ability to suppress defaults while preserving explicit and derived choices.

### Highest-Priority Blockers

The review recommends replanning before execution:

- **Plan 01:** Introduce a structured `LaunchResolution` type carrying workspace, source, repository root, and recording target. Export a shared resolver that handles the complete precedence and captures whether the resolution arose from binding, derivation, or fallback. Do not proceed to UI/modal changes until this foundation is solid.
- **Plan 02:** The current test coverage addresses only fragments of the architecture. Replan to cover complete precedence behavior, repository-root recording + replay, source-aware title rendering, and single-app root/subfolder deduplication.
- **Plan 03:** Do not record daemon-derived bindings without a typed resolver and explicit workspace lock semantics. The parity test must exercise the shared path in `bauded`, not just prove determinism in `baude-core`.

### Key Strengths

- The phase correctly identifies the right seams (startup, modal, title, daemon).
- Existing infrastructure (git root resolution, breadcrumb canonicalization, test fixtures) provides a strong foundation.
- Repository identity deduplication via `observed_common_dir` is sound.

### Risk Assessment

**Overall: HIGH.** The plans can compile and partially function while silently failing multiple locked success criteria. The phase requires architectural clarity before implementation begins.
