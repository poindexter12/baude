---
phase: 07-local-tui-dogfood-release
verified: 2026-09-02T02:40:24Z
status: human_needed
score: 11/12 must-haves verified
behavior_unverified: 0
overrides_applied: 1
overrides:
  - must_have: "Phase 7 readiness changes and commands neither modify nor invoke the existing publication workflows; readiness is verified without publishing or pushing a release"
    reason: "Superseded by a deliberate user decision (2026-09-01): commit 52ff447 evolved the release workflows for a beta channel, and the v2.0.0-beta GitHub prerelease was published at 6014b63 with 4-platform tarballs (both binaries) plus SHA256SUMS. Confirmed live via gh release view. This supersedes the older 'no release was pushed' handoff note."
    accepted_by: "Joe (user decision, per orchestrator context and 07-UAT-EVIDENCE.md 2026-09-01 section)"
    accepted_at: "2026-09-01T00:00:00Z"
human_verification:
  - test: "Decide the restart selection-initialization contract when a standalone row is present: launch baude with one repository (with an available checkout) and one standalone folder whose basename sorts alphabetically before the repository; restart."
    expected: "A product decision. Current code (hierarchy.rs initial_selection + selectable_local_ids + alphabetical basename top-level sort) selects the standalone row. Current docs (README lines 14-16, docs/local-tui-dogfood.md section 7 line 137) promise 'first available local checkout'. Either amend the docs to say 'first selectable local row in rendered order (which may be a standalone session)' or change initial_selection to prefer checkouts over standalone rows."
    why_human: "This is a doc-vs-code product-intent decision, not a defect I can adjudicate. Both behaviors are deterministic and tested; only the intended contract is ambiguous. The 2026-09-01 evidence explicitly escalated this to phase verification."
  - test: "Fix or accept the README beta-pin example: README line 56 pins mise version 2.0.0-beta.1 with prerelease = true."
    expected: "gh release list shows only v2.0.0-beta exists (no v2.0.0-beta.1), so a user following the example today gets an install failure. Either change the example to 2.0.0-beta, or accept it as forward-looking for the release-please-cut 2.0.0-beta.1 (release-please-config.json release-as is 2.0.0-beta.1)."
    why_human: "Depends on the release-channel intent: whether the manually published v2.0.0-beta or the upcoming automated 2.0.0-beta.1 is the install target users should pin."
  - test: "Confirm CHANGELOG staleness is acceptable: the '2.0.0-beta readiness' section still says 'the publication decision remain[s] pending'."
    expected: "The prerelease is now published; the sentence is stale. Likely self-healing when release-please regenerates the changelog on the next release PR, but confirm that is the plan."
    why_human: "Whether to hand-fix now or let release-please rewrite it is a workflow preference."
---

# Phase 7: Local TUI Dogfood Release Verification Report

**Phase Goal:** Local TUI users can dogfood durable repository parents and default sessions, create or activate eligible local branch worktrees, close and reopen them, and safely remove them through stable context-aware UI, with the source tree ready for target `v2.0.0-beta`.
**Verified:** 2026-09-02T02:40:24Z (against HEAD b5e99d2, clean working tree; only untracked `opencode.json`)
**Status:** human_needed
**Re-verification:** No — initial verification

## Context accounted for (not drift)

Four post-plan developments were verified as deliberate, user-approved evolution rather than unexplained drift:

1. **59e67b6** — auto-populate existing linked worktrees as inactive checkout rows on repository open; main worktree takes the Main role when the primary lives elsewhere. Implemented in `App::admit_existing_worktrees` (baude/src/app.rs:1681,1693); pinned by `admit_repository_populates_existing_worktrees_as_inactive_rows` (app.rs:6016) — **run, passes**.
2. **6014b63** — seed-exempt safe removal. `SEED_ARTIFACTS` pure-content predicates for `.claude/settings.local.json` and `.mcp.json` in baude-core/src/git.rs:2063-2123; a user-modified seed fails its predicate and keeps blocking (fail-closed preserved). docs/local-tui-dogfood.md section 9 updated; the 2026-09-01 UAT re-certified the new semantics with the seed file verified present immediately before removal.
3. **v2.0.0-beta prerelease published** at exactly 6014b63. Verified live: `gh release view v2.0.0-beta` returns `isPrerelease: true`, target `6014b63a...`, and 4 tarball assets (aarch64/x86_64 x apple-darwin/unknown-linux-gnu) plus SHA256SUMS.txt; release.yml packs both `baude` and `bauded` into each tarball. Recorded as an accepted override on the "without publishing" plan truth.
4. **Linux CI certification** runs separately on draft PR #56 — out of scope here, listed under deferred/pending.

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| 1 | (SC1) Repository parents stay visible without running sessions; main checkout, separate managed default worktree, and other children render beneath the correct parent | ✓ VERIFIED | hierarchy.rs projection (1082 lines, 18 fns); named tests run and passing: `local_hierarchy_orders_parents_and_children_by_durable_identity`, `admit_repository_populates_existing_worktrees_as_inactive_rows`; 2026-09-01 scripted-PTY evidence at 6014b63 (step 5) |
| 2 | (SC2) Parents name-ordered, children persisted oldest-first across restart; status/archive/attention changes never reorder | ✓ VERIFIED | Sort by lowercase basename then path (hierarchy.rs:544-563); children by `(first_seen_order, key)` (hierarchy.rs:409); `local_tui_dogfood_real_git_flow_survives_restart_without_duplicates` **run, passes** (asserts persisted order across restart); `local_hierarchy_order_ignores_runtime_and_session_status` exists pinning decoration-immunity |
| 3 | (SC3) Create/activate branch worktree, close and reopen, distinctly confirmed clean removal with branch retained | ✓ VERIFIED | `hierarchy_action_matrix_dispatches_only_authorized_local_actions`, `hierarchy_modals_name_exact_targets_and_distinguish_close_from_remove`, and the dogfood restart test all **run, pass**; 2026-09-01 evidence steps 6/7/9: branch created once, idempotent `w`, close retains child, reopen exact-once, removal names `refs/heads/feature/dogfood-beta`, branch survives at seed commit |
| 4 | (SC4) Actions name the actual target; invalid/unsafe ops refuse with zero partial state; existing session actions preserved; flat daemon/session APIs remain non-destructive projections | ✓ VERIFIED | Action-matrix and modal tests pass; `flat_session_api_remains_a_non_hierarchical_compatibility_projection` (bauded/src/api.rs:745) **run, passes**; api.rs route table has only flat `/sessions*` routes, no hierarchy or remove-worktree endpoint; DELETE remains retained close |
| 5 | (SC5) Isolated e2e dogfood passes across restart; fmt/lint/tests/package/version metadata/release docs ready for v2.0.0-beta | ✓ VERIFIED (publication clause: PASSED override) | Dogfood test passes in-process; 2026-09-01 clean-tree scripted-PTY re-certification at exact tag commit 6014b63 (`cargo build --release --locked` + isolated `cargo install` both reporting 2.0.0-beta); publication clause superseded by user decision (see overrides) |
| 6 | Selection is durable-key identity (RepositoryKey/CheckoutKey/StandaloneKey), never runtime IDs/labels/row indexes | ✓ VERIFIED | `SelId`/`LocalRowId`/`SelectionTarget` throughout hierarchy.rs and app.rs; `reconcile_selection` keeps the same durable key across refresh; dogfood test asserts explicit durable-key reselection |
| 7 | Restart selection matches the documented selection contract | ? UNCERTAIN (human decision) | Code: `initial_selection` (hierarchy.rs:185) = first selectable row in alphabetical rendered order — a standalone row wins when its basename sorts first. Docs: README:14-16 and runbook section 7:137 promise "first available local checkout". **Explicit assessment: with a standalone row sorting first, observed behavior literally contradicts the phase's documented contract.** It violates no roadmap success criterion (none addresses standalone initialization priority; standalone sessions postdate the SCs). Note the 2026-09-01 run's "standalone sorted first" was itself alphabetical coincidence (`plain-folder` < `repository`), not a hard rule — the docs sentence is wrong only conditionally |
| 8 | Capability-gated dispatch uses explicit core `LifecycleCapability` / `RetryReopen` / `RetryRecovery`, never glyphs or stale row state | ✓ VERIFIED | `LifecycleCapability` in baude-core/src/lifecycle.rs (2973 lines, 15 tests); `RetryReopen|RetryRecovery` present in both lifecycle.rs (6 hits) and hierarchy.rs (2 hits); `ActionView` typed mapping; action-matrix test passes |
| 9 | Exact 160x40/100x30/79x24/59x20/40x12 matrix never panics; tiny-size focus transfer; Unicode cell width not bytes | ✓ VERIFIED | `hierarchy_viewport_matrix_renders_without_panic_and_preserves_semantics` (ui.rs:2275, iterates exactly those five sizes) **run, passes**; `cell_width` via ratatui `Line::width` + grapheme clusters (ui.rs:163-207); `sync_sizes` tiny-rect tests at 40x12 (app.rs:5806-5842); 2026-09-01 fresh 40x12 launch evidence |
| 10 | Isolated real-Git flow: admit, create/activate, close, restart App, reopen exactly once, safe remove retaining branch, no duplicate keys, no writes outside fixture | ✓ VERIFIED | `local_tui_dogfood_real_git_flow_survives_restart_without_duplicates` (app.rs:6829) **run, passes (6.11s)** over a real bare origin + isolated state root; `active_launch_repository_restart_focuses_restored_runtime_without_duplicate_spawn` (app.rs:5981) pins the ContradictoryLifecycle regression found in live UAT |
| 11 | All crates, path dep, lockfile, binaries target 2.0.0-beta; release-please beta prerelease config; CI mirrors 4 supported targets non-publishing; last-published manifest 0.14.0 | ✓ VERIFIED | All three Cargo.tomls `version = "2.0.0-beta"`; root `baude-core = { path, version = "=2.0.0-beta" }`; Cargo.lock all three at 2.0.0-beta; config: `versioning: prerelease`, `prerelease: true`, `prerelease-type: beta`, `release-as: 2.0.0-beta.1` (evolved from `2.0.0-beta` by 52ff447 so later betas auto-increment — part of the approved publication work); `.release-please-manifest.json` still `{".": "0.14.0"}`; ci.yml matrix has all four targets, builds `--locked`, tars both binaries, uploads as workflow artifacts only |
| 12 | README, CHANGELOG, runbook consistently describe v2.0.0-beta; standalone sessions documented; final local gate recorded | ✓ VERIFIED (2 doc warnings) | README documents hierarchy, checkout-first navigation, restart selection, standalone sessions, beta install, links runbook; docs/local-tui-dogfood.md (258 lines) covers isolation, wide/narrow, standalone (section 8), seed-exempt removal (section 9), evidence checklist naming 07-UAT-EVIDENCE.md; CHANGELOG has "2.0.0-beta readiness" section. Warnings: README pin example `2.0.0-beta.1` names a not-yet-existing release; CHANGELOG "publication decision remain pending" is now stale |

**Score:** 11/12 truths verified (0 present-but-behavior-unverified)

### Deferred Items

| # | Item | Addressed In | Evidence |
|---|------|-------------|----------|
| 1 | Supported Linux/runtime certification | Draft PR #56 (parallel track) | Orchestrator context: "Linux CI certification is running separately on draft PR #56"; UAT evidence lists it as pending |

### Required Artifacts

| Artifact | Expected | Status | Details |
| -------- | -------- | ------ | ------- |
| `baude/src/hierarchy.rs` | Pure durable hierarchy, ordering, selection reconciliation | ✓ VERIFIED | 1082 lines; `LocalRow`, `initial_selection`, `reconcile_selection`, `reconcile_after_removal`, standalone rows; 4 test fns |
| `baude/src/app.rs` | Projection, durable-key selection, dispatch, resize, dogfood tests | ✓ VERIFIED | 7658 lines, 33 tests incl. both named restart tests and worktree auto-populate |
| `baude/src/ui.rs` | Sidebar tracer, LayoutRects, responsive modes, Unicode-cell layout, modals | ✓ VERIFIED | 2772 lines; `draw_sidebar`, `LayoutRects`, `layout()`, `cell_width`; 9 tests incl. five-size matrix and copy-contract vs UI-SPEC |
| `baude/src/main.rs` | `mod hierarchy` registration | ✓ VERIFIED | main.rs:2 |
| `baude-core/src/lifecycle.rs` | `LifecycleCapability` projection | ✓ VERIFIED | 2973 lines, 15 tests, RetryReopen/RetryRecovery |
| `bauded/src/api.rs` | Flat non-hierarchical compat contract + test | ✓ VERIFIED | Route table flat-only; async compat test at line 745 passes |
| `Cargo.toml` (+3 crate manifests, lock) | Exact `=2.0.0-beta` metadata | ✓ VERIFIED | All consistent; lock synced |
| `release-please-config.json` | Beta prerelease proposal over simple pattern | ✓ VERIFIED | prerelease/beta/release-as present; extra-files cover all four manifests |
| `.github/workflows/ci.yml` | Four-target two-binary artifact readiness | ✓ VERIFIED | All four targets, `--locked`, tar both binaries, artifact upload only (no publication) |
| `docs/local-tui-dogfood.md` | Isolated manual runbook + evidence checklist | ✓ VERIFIED | 258 lines, sections 1-10 + checklist; updated for seed-exempt removal |
| `README.md` | Hierarchy, lifecycle keys, beta install guidance | ✓ VERIFIED | Links runbook; see doc warnings |
| `CHANGELOG.md` | Beta-readiness section | ✓ VERIFIED | "2.0.0-beta readiness" present; staleness warning above |
| `07-VALIDATION.md` | Observed evidence, honest pending items | ✓ VERIFIED | Per-requirement exact-test map; status honestly `draft`/pending certification |

### Key Link Verification

All 13 key links across the six plans verified by `gsd-tools query verify.key-links` (pattern-in-source) — app→hierarchy projection, ui→app rows/selection, app→core lifecycle dispatch, hierarchy→core typed capability, app→ui layout/resize, ui→hierarchy ActionView, app→core real-Git test wiring, api→Manager flat routes, crate-manifest→workspace version, release-config→manifest, ci→release target/tar parity, README→runbook, runbook→UAT-EVIDENCE. Spot-read confirmed the load-bearing ones (dispatch, initial_selection, seed-exempt preflight) are substantive, not token matches.

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
| -------- | ------------- | ------ | ------------------ | ------ |
| ui.rs sidebar | hierarchy rows | `App` projects real `RepositoryState` + runtime checkouts via `hierarchy::project_local` | Yes (tracer test renders real App parent/child) | ✓ FLOWING |
| Removal preflight | blockers | Fresh `git` status reads per call (`is_pure_seed_artifact` re-reads file content) | Yes | ✓ FLOWING |
| Durable state | schema-v3 JSON | `state-<workspace>.json` persisted, restored across restart in dogfood test | Yes | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| -------- | ------- | ------ | ------ |
| Real-Git restart/dedup/removal flow | `cargo test -p baude local_tui_dogfood_... -- --exact` | ok, 6.11s | ✓ PASS |
| Flat API stays non-hierarchical compat | `cargo test -p bauded flat_session_api_...` | ok | ✓ PASS |
| Auto-populate existing worktrees (59e67b6) | `cargo test -p baude admit_repository_populates_... -- --exact` | ok | ✓ PASS |
| Hierarchy durable ordering | `cargo test -p baude local_hierarchy_orders_... -- --exact` | ok | ✓ PASS |
| Five-size viewport matrix no-panic | `cargo test -p baude hierarchy_viewport_matrix_...` | ok | ✓ PASS |
| Capability-gated action dispatch | `cargo test -p baude hierarchy_action_matrix_...` | ok | ✓ PASS |
| Close vs remove modal distinction | `cargo test -p baude hierarchy_modals_name_exact_targets_...` | ok | ✓ PASS |
| Standalone dedup/close/reopen/missing durability | `cargo test -p baude standalone_admission_dedup_...` | ok | ✓ PASS |
| Published prerelease reality check | `gh release view v2.0.0-beta` | prerelease at 6014b63, 4 tarballs + SHA256SUMS | ✓ PASS |

(8 single named tests; full suite deliberately not re-run — 07-VALIDATION.md and the 2026-08-31 evidence record fmt/clippy/345-test full-gate passes, and the 2026-09-01 run adds locked release build + isolated install at the exact tag commit.)

### Probe Execution

No `scripts/*/tests/probe-*.sh` probes exist in this repository; no PLAN declares probe scripts. SKIPPED (no probes declared or discovered).

### Requirements Coverage

| Requirement | Source Plan | Status | Evidence |
| ----------- | ----------- | ------ | -------- |
| REPO-05, HIER-01..04 | 07-01 | ✓ SATISFIED | Truths 1, 2, 6; ordering + tracer + decoration-immunity tests |
| WORK-01..06 | 07-02 | ✓ SATISFIED | Truths 3, 8; action matrix + seed-exempt fail-closed preflight + dogfood removal |
| SURF-01, SURF-02 | 07-03 | ✓ SATISFIED | Truths 4, 9; copy-contract test pins UI-SPEC literals |
| SURF-05, REL-01 | 07-04 | ✓ SATISFIED | Truths 4, 10; both named tests run and pass |
| REL-02 | 07-05 | ✓ SATISFIED | Truth 11 |
| REL-03 | 07-06 | ✓ SATISFIED (2 doc warnings) | Truth 12 |

No orphaned requirements: REQUIREMENTS.md maps exactly these 17 IDs to Phase 7, all claimed by plans.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| — | — | No TBD/FIXME/XXX/TODO/HACK/placeholder markers in any phase-modified source file | — | Clean |
| README.md | 56 | mise pin example `2.0.0-beta.1` names a release that does not exist (only `v2.0.0-beta` is published) | ⚠️ Warning | User following the example today gets an install failure |
| CHANGELOG.md | 5-6 | "the publication decision remain pending" now stale post-publication | ℹ️ Info | Likely rewritten by release-please on next release PR |

### Human Verification Required

#### 1. Restart selection-initialization contract with standalone rows

**Test:** With one repository (available checkout) and one standalone folder whose basename sorts before the repository, restart baude.
**Expected:** Product decision needed. Code selects the first selectable row in alphabetical rendered order (the standalone row here); README:14-16 and runbook section 7 promise "first available local checkout". Either amend the docs or make checkouts win initialization.
**Why human:** Explicit assessment: **yes, this literally contradicts the phase's documented selection contract** in the standalone-first case, though it violates no roadmap success criterion (the SCs predate standalone sessions and are silent on initialization priority). Both resolutions are one-line changes; which one reflects intent is a product call the evidence file itself escalated to verification.

#### 2. README beta-pin example accuracy

**Test:** Follow README's mise pin (`version = "2.0.0-beta.1"`).
**Expected:** Fails today; only `v2.0.0-beta` exists. Fix the example or publish 2.0.0-beta.1.
**Why human:** Depends on which beta is the intended user-facing install target.

#### 3. CHANGELOG staleness acceptance

**Test:** Read CHANGELOG "2.0.0-beta readiness" intro.
**Expected:** Confirm the stale "publication pending" sentence will be superseded by release-please or hand-fix it.
**Why human:** Workflow preference.

### Gaps Summary

No blocking gaps. Every roadmap success criterion is achieved in the current codebase with passing behavioral tests and exact-commit scripted-PTY UAT evidence (2026-09-01 at 6014b63, the published tag commit). The one substantive open item is the escalated selection-initialization question — a confirmed doc-vs-code contract mismatch in the standalone-row case that needs a product decision, plus two minor doc-accuracy warnings (README pin example, CHANGELOG staleness) that follow from the deliberate post-docs publication decision. The "without publishing" plan truth is carried as an accepted override reflecting that decision. Linux certification is deferred to draft PR #56.

---

_Verified: 2026-09-02T02:40:24Z_
_Verifier: Claude (gsd-verifier)_
