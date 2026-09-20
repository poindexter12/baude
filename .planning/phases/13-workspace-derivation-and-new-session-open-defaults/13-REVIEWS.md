---
phase: 13
reviewers: [codex]
reviewed_at: 2026-09-20T04:15:30Z
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

## Convergence Cycle 2

Reviewed against repository commit `8f08072`. Source access was available.

**Overall verdict: not yet converged.** Most cycle-1 findings are now represented in the plan text, but several mechanisms still conflict with the live code or with locked decisions. The largest blockers are:

- `BAUDE_BACKEND` still suppresses `plan_launch`, so the proposed precedence cannot work.
- Non-repository launches are still planned to record bindings, contrary to the phase decision.
- The daemon plan assumes a startup lock that does not exist.
- The proposed daemon "parity" test calls the same core function twice and cannot detect daemon wiring failures.

### 13-01

#### Summary

The replanning substantially improves the tracer by adding repository-root data flow, workspace-source tracking, root-keyed persistence, title rendering, and repository-aware new-session defaults. Cycle-1 findings R1–R3 and R5 are directly addressed in the text. However, R4 is not operationally solved because the earlier `folder_workspace::plan_launch` gate still suppresses binding lookup and derivation whenever `BAUDE_BACKEND` is present. The recording policy also contradicts the locked non-git behavior.

#### Strengths

- Repository-root derivation now has a plausible data path: `LaunchPlan.repo_root` is proposed where the current structure only carries `hint` and `notes` at `baude-core/src/folder_workspace.rs:61-65`. This directly improves cycle-1 R1.
- Recording after the TUI lock is correctly aligned with the current startup order. The lock is claimed at `baude/src/main.rs:370-396`, while folder memory is written at `baude/src/main.rs:398-411`. This addresses R2's durability concern for the TUI.
- Source tracking is a sensible solution to R3/R5. The current `Workspace` contains no provenance at `baude-core/src/workspace.rs:46-54`, so adding `WorkspaceSource` is necessary to distinguish bound, derived, explicit, and implicit-default outcomes.
- The proposed `n` prefill change targets the correct method. Current behavior prefers `new_session_dir` at `baude/src/app.rs:4057-4064`, while the reusable repository-root helper already exists at `baude-core/src/git.rs:1734-1738`.
- OPEN-03 rests on a real deduplication mechanism: repository admission compares canonical `common_dir` values at `baude/src/app.rs:1914-1937`, and discovery canonicalizes that directory at `baude-core/src/git.rs:321-353`.
- The home-directory resolver is fixture-aware. `persist::home_dir()` honors `TestRedirect` at `baude-core/src/persist.rs:914-922`, supporting cycle-1 R7/R9 when actually used.

#### Concerns

- **HIGH — Cycle-1 R4 is only half incorporated.** The plan removes hint suppression inside `workspace::resolve_with_hint`, but `plan_launch` itself immediately returns an empty plan if `backend_env.is_some()` at `baude-core/src/folder_workspace.rs:71-83`. Consequently, no ancestor binding or repository root reaches the resolver, and the promised order `binding/config/derived > BAUDE_BACKEND` remains impossible. The plan only names the suppression at `workspace.rs:139`; it must also change this earlier gate and its module documentation at `folder_workspace.rs:12-16`.
- **HIGH — Non-repository recording contradicts the locked decision and the plan's own prohibition.** The plan says to use `repo_root` when present and otherwise retain launch-directory recording. Current recording writes any supplied folder at `baude-core/src/folder_workspace.rs:114-128`, and the TUI calls it for every folder-context launch at `baude/src/main.rs:398-411`. That would continue creating bindings for previously unbound non-git folders, despite the decision that such launches derive and record nothing.
- **HIGH — The proposed resolver API change is not dependency-safe.** Derivation requires passing `repo_root` through `initialize`, whose current public signature accepts only `hint` at `baude-core/src/workspace.rs:276-285`. Plan 13-01 changes the TUI but not `bauded`, which still calls `initialize(&config, None)` at `bauded/src/main.rs:181-182`. Replacing the signature would leave the workspace uncompilable until 13-03 unless a backward-compatible wrapper or new API is specified.
- **MEDIUM — `config backend` provenance is undefined.** The plan says every precedence level gets a source, but assigns config `workspace` to `Explicit` while saying config `backend` merely "uses backend, not a separate source." Current config backend can determine the workspace name at `baude-core/src/workspace.rs:144-150`. It should be explicitly classified, particularly because WSPC-05 distinguishes configured selection from a truly blank default.
- **MEDIUM — The title composition does not literally replace the default workspace with `(blank)`.** `display_label()` returns `Claude Code` for the default workspace at `baude-core/src/workspace.rs:73-83`. Formatting `display_label() + "(blank)"` therefore renders `Claude Code (blank)`, not a blank workspace identity. A dedicated title-label method would make the requirement unambiguous.
- **MEDIUM — Derivation ownership is contradictory.** The plan first says `plan_launch` derives through `workspace::sanitize`, then says the resolver derives from `repo_root`. Today `sanitize` is private to `workspace.rs` at `baude-core/src/workspace.rs:99-111`, so sibling-module derivation will not compile unless visibility changes. One layer should own derivation.
- **MEDIUM — The smoke/isolation story remains inconsistent.** The automated check only runs the pre-existing exact-match round-trip test. The later instruction launches a real `baude`, while also asserting `TestRedirect` protects the real config. A normal binary launch does not install the fixture redirect used by tests.
- **LOW — `git::repo_root` is not itself canonicalizing.** It converts command output directly to `PathBuf` at `baude-core/src/git.rs:1734-1738`. Before using it as the durable binding key, the implementation should canonicalize it or document why canonical launch-directory execution guarantees the required form.

#### Cycle-1 Disposition

| Finding | Assessment |
|---|---|
| R1 repository-root data path | Addressed in text |
| R2 root-keyed recording | Addressed for repositories |
| R3 provenance tracking | Addressed |
| R4 precedence reorder | **Not fully addressed**; `plan_launch` still suppresses on backend env |
| R5 blank default hint | Partially addressed; final title composition remains ambiguous |
| R6 source mapping | Partially addressed; config backend unspecified |
| R7 home resolution | Addressed if `persist::home_dir()` is mandated |
| R8 meaningful verification | Partially addressed; tracer verification still tests old behavior |
| R9 fixture containment | Partially addressed; manual smoke is not fixture-isolated |

#### Suggestions

- Change the initial `plan_launch` gate to suppress only for `BAUDE_WORKSPACE`, not `BAUDE_BACKEND`.
- Introduce an explicit shared input such as `WorkspaceLaunchContext { bound_hint, repo_root }`, while retaining the existing `initialize(config, hint)` wrapper for callers migrated later.
- Define a source-aware recording rule. At minimum, do not fall back to recording an unbound non-git `launch_dir`.
- Assign a provenance value to config `backend`.
- Add a dedicated `workspace_title_label()` that returns either `"<workspace> (<source>)"` or `(blank)`.
- Replace the old tracer test with an isolated test that exercises planning, resolution, post-lock recording, recall, and rendered title.

#### Risk Assessment

**HIGH.** The central precedence rule cannot work through the current `plan_launch` gate, and the proposed API migration can temporarily break `bauded`. The non-git recording behavior also violates a locked phase decision.

### 13-02

#### Summary

The TDD plan is markedly stronger than cycle 1: it adds a full precedence matrix, end-to-end derivation/recall, UI rendering coverage, prefill cases, and one-instance repository deduplication. R1–R4 and R7–R8 are materially improved. Its main weakness is that some tests exercise lower-level functions without covering the wiring faults that currently defeat the behavior, and one home-boundary expectation conflicts with the specified ancestor walk.

#### Strengths

- The precedence matrix directly targets outdated tests such as `hint_outranks_config_defaults_but_never_env`, which currently asserts backend-env suppression at `baude-core/src/workspace.rs:756-783`.
- The end-to-end WSPC-02 case is better than isolated lookup tests because current `plan_launch` only performs an exact match at `baude-core/src/folder_workspace.rs:84-97`.
- UI rendering can be tested with the existing `TestBackend` helper at `baude/src/ui.rs:2482-2495`.
- The plan correctly uses one `App` instance for root/subfolder deduplication. The live implementation deduplicates against the instance's `repository_state` through canonical common-dir comparison at `baude/src/app.rs:1914-1937`.
- The prefill tests target the exact current fallback logic at `baude/src/app.rs:4057-4064`.
- The plan's separate Cargo invocations use valid single-filter syntax, addressing cycle-1 R8.

#### Concerns

- **HIGH — A resolver-only precedence matrix can pass while startup remains wrong.** Because `plan_launch` discards all context when `BAUDE_BACKEND` is set at `baude-core/src/folder_workspace.rs:78-83`, tests must include the complete `plan_launch → resolve` path for the cases "binding beats backend env" and "derived beats backend env." Testing only `resolve_with_hint` would miss the primary 13-01 defect.
- **MEDIUM — `launch_dir == home returns None` is not a generally correct expectation.** The locked rule walks "to `$HOME`," and the proposed algorithm checks the current path before stopping. Therefore a binding recorded exactly at home should be eligible. The test should distinguish "no binding at home" from "binding at home."
- **MEDIUM — The WSPC-02 test does not naturally belong entirely inside `folder_workspace.rs`.** Resolution currently belongs to `workspace.rs`, while post-lock recording occurs in `baude/src/main.rs:370-411`. A folder-module-only test risks simulating rather than testing the actual integration.
- **MEDIUM — UI source fixtures need an explicit construction seam.** The current helper always creates an explicit config workspace at `baude/src/ui.rs:2351-2379`. It cannot naturally create all four sources without extending `override_for_test` or introducing a title-formatting function that accepts a `Workspace`.
- **LOW — Task-level verification filters omit some promised cases.** The Task 1 filter contains `find_binding` but not `test_wspc02_end_to_end_derive_persist_recall`; the Task 2 command runs only baude-core tests, not the UI tests; Task 3's primary command omits the deduplication test. The final full-suite gate mitigates this, but task completion could be declared prematurely.
- **LOW — The empty sanitization case is underspecified.** `sanitize` replaces every invalid character with `-` at `baude-core/src/workspace.rs:99-111`; it only returns empty for empty input. Repository basenames are normally non-empty. If "do not persist empty" is retained as a must-have, the implementation needs an explicit guard and a realistic test.

#### Cycle-1 Disposition

| Finding | Assessment |
|---|---|
| R1 precedence matrix | Addressed, but must include planning layer |
| R2 derive/persist/recall lifecycle | Addressed in intent |
| R3 root/subfolder deduplication | Addressed correctly with one app |
| R4 title rendering tests | Addressed |
| R5 daemon assertion moved | Addressed |
| R6 empty sanitization | Defined, but implementation guard remains vague |
| R7 trailing slash | Addressed |
| R8 valid test commands | Addressed; task filters could be broader |

#### Suggestions

- Make the precedence matrix table drive the shared launch resolver, including `plan_launch`, not just `resolve_with_hint`.
- Change the home test to cover both binding-at-home and no-binding-at-home.
- Extract pure helpers for resolving a `LaunchPlan`, deciding whether and where to record, and formatting the workspace title.
- Give each task's automated command all promised tests, or use a broader module filter.
- In the deduplication test, assert repository count, checkout count, canonical common-dir equality, returned/focused checkout, and selection state separately.

#### Risk Assessment

**MEDIUM.** The test inventory is strong, but it can still certify the resolver while missing the startup gate that prevents production behavior. Correcting that test boundary would lower the risk substantially.

### 13-03

#### Summary

The plan correctly identifies daemon canonicalization, shared planning, recording, README updates, and one-instance repository deduplication. Cycle-1 R3–R5 and R7 are substantially addressed. However, its lock-order mechanism is based on an incorrect reading of the source, and its parity test is tautological. As written, R1, R2, and R6 are not actually resolved.

#### Strengths

- The daemon currently initializes with no folder hint at `bauded/src/main.rs:174-182`, so adding canonicalized `current_dir`, `plan_launch`, and `plan.hint` targets the correct missing integration.
- Canonicalizing daemon `current_dir` explicitly addresses cycle-1 R5 and mirrors the TUI's launch path normalization at `baude/src/main.rs:343-347`.
- README work is well scoped to existing outdated sections. Current documentation still says either environment variable suppresses memory at `README.md:287-296` and describes the old precedence at `README.md:456-463`.
- Clone semantics are genuinely separate from the `n` prefill: clone destination behavior is documented at `README.md:298-313`, while the prefill lives in `open_new_session_modal`.
- The plan correctly retains the one-app repository test from 13-02 rather than inventing cross-instance deduplication.

#### Concerns

- **HIGH — The claimed daemon startup lock does not exist.** `Manager::restore` performs a load, restores sessions, and saves at `bauded/src/manager.rs:442-455`; it does not claim a lock before loading. The repository even documents this debt explicitly: "bauded never claims the workspace lock at startup" at `bauded/src/manager.rs:3031-3035`. The plan's statement that line 444 "loads and locks before recording" is false.
- **HIGH — Cycle-1 R2 is not addressed by calling `plan_launch` twice.** A determinism test of the same core function cannot catch the current daemon defect where `bauded` passes `None` to `initialize` at `bauded/src/main.rs:181-182`. This test would pass today even though daemon parity is absent.
- **HIGH — Recording again falls back to non-repository `launch_dir`.** The proposed `plan.repo_root.as_deref().or(Some(&launch_dir))` would record an unbound non-git daemon launch, contradicting the locked decision that such launches derive and record nothing.
- **HIGH — The recording call is incompatible with the current API.** `folder_workspace::record` currently expects `(root, path, workspace: &str, now_ms)` at `baude-core/src/folder_workspace.rs:118-128`. The proposed call supplies a `Workspace` and omits the timestamp. Plan 13-01 does not say the signature will change.
- **MEDIUM — Lock ownership and error policy are unspecified.** To guarantee "lock refusal prevents binding write," the daemon must explicitly call `claim_workspace_state_lock("daemon-state", workspace)` before recording and before `Manager::restore`. `STATE_BASE` is private inside `manager.rs` at `bauded/src/manager.rs:32-36`, so the plan must either expose a helper or deliberately duplicate the stable base name. It must also specify whether `Held` and `Io` abort startup or degrade.
- **MEDIUM — The daemon provenance shown by the TUI remains local-only.** The title proposal reads `workspace::active()` in the TUI. The daemon exposes only workspace name/backend through `/info` at `bauded/src/api.rs:117-126`; it does not expose whether its workspace was bound or derived. If the locked "daemon-backed views show the daemon's workspace the same way" includes provenance, more API work is required.
- **MEDIUM — Documentation wording is internally inconsistent.** The README action says implicit default is labeled `(default)`, while acceptance requires `(blank)`. The code contract should be fixed first and documented consistently.
- **LOW — The README grep is fragile.** Requiring the entire precedence chain on one line may fail well-formatted wrapped Markdown, while still not verifying examples, daemon parity, title labels, or non-git behavior.

#### Cycle-1 Disposition

| Finding | Assessment |
|---|---|
| R1 daemon recording | Requested, but post-lock mechanism is incorrect |
| R2 parity test | **Not addressed**; determinism is not binary parity |
| R3 ownership metadata | Addressed |
| R4 one-app dedup test | Addressed |
| R5 daemon canonicalization | Addressed |
| R6 lock/write ordering | **Not addressed**; assumed lock is absent |
| R7 docs after tests | Addressed |
| R8 targeted grep | Improved slightly, but still weak |

#### Suggestions

- Add an explicit daemon startup sequence in `bauded/src/main.rs`:
  1. Canonicalize `current_dir`.
  2. Build the shared launch plan.
  3. Resolve the workspace.
  4. Claim `daemon-state` lock.
  5. Record only when the recording policy allows it.
  6. Construct and restore `Manager`.
- Specify error handling for both `StateLockError::Held` and `StateLockError::Io`.
- Replace the tautological parity test with either a shared `resolve_launch_context` helper used directly by both entry points and tested once, plus call-site tests proving both binaries invoke it; or a `bauded` startup helper test using the same fixture inputs as the TUI startup helper.
- Keep daemon recording in `main.rs`, where the launch plan and lock result are both available; do not bury it in `Manager::restore`.
- Use the actual four-argument recording API, or explicitly plan a signature change across both callers.
- Replace the README grep with section-level assertions or a documentation review checklist.

#### Risk Assessment

**HIGH.** The daemon plan relies on a nonexistent startup lock, and its parity test cannot detect whether daemon startup is wired correctly. Those are direct blockers for WSPC-04 and for the claimed safe recording order.

## Final Assessment

Cycle 2 closes many documentation and test-coverage gaps, but four changes are still required before execution:

1. Remove the `BAUDE_BACKEND` short-circuit from `plan_launch`.
2. Define a source-aware recording policy that does not record unbound non-git launches.
3. Make the resolver API migration backward-compatible across waves.
4. Redesign daemon startup around a real `daemon-state` lock and a genuine call-site parity test.

Until those are incorporated, the phase remains **HIGH risk** despite the stronger test plan.

---

*Cycle 2 review completed: 2026-09-20*

