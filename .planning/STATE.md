---
gsd_state_version: "1.0"
milestone: v2.2
milestone_name: Reliability and Terminal Usability
current_phase: 08
current_phase_name: Test Isolation and Fixture Ownership
status: executing
stopped_at: Completed 08-06-PLAN.md
last_updated: "2026-09-15T17:36:09.743Z"
last_activity: 2026-09-15
last_activity_desc: Phase 08 execution started
state_head: 5777f753b30f790b09570c1be651a13070ec8e86
progress:
  total_phases: 5
  completed_phases: 0
  total_plans: 8
  completed_plans: 7
  percent: 0
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-08-30)

**Core value:** You can see at a glance which of your many coding-agent sessions needs you next and act on it from the terminal or phone.
**Current focus:** Phase 08 — Test Isolation and Fixture Ownership

## Current Position

Phase: 08 (Test Isolation and Fixture Ownership) — EXECUTING
Plan: 8 of 8
Status: Ready to execute
Last activity: 2026-09-15 — Plans 01-03 complete (per-fixture workspace identity landed)

Six of the twelve Phase 8/9 requirements shipped in v2.1.2-v2.1.5 ahead of execution (HREG-01, HREG-02, WLOCK-01 through WLOCK-04). Four are partial and two never started; those six gaps are what Phases 8 and 9 now cover. Per-requirement evidence is in REQUIREMENTS.md.

## Performance Metrics

**Velocity:**

- v2.0 plans completed: 16
- Prior milestone: 14 plans completed across 4 phases
- Average duration: 19 min
- Total execution time: 301 min

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 5. Durable Repository Admission | 3/3 | 38 min | 13 min |
| 6. Shared Lifecycle Core Refactor | 7/7 locally implemented | 145 min execution history | 21 min |
| 7. Local TUI Dogfood Release | 6/6 locally implemented | 118 min | 20 min |
**Per-Plan Metrics:**

| Plan | Duration | Tasks | Files |
|------|----------|-------|-------|
| Phase 05 P01 | 10m | 2 tasks | 1 files |
| Phase 05 P02 | 14min | 3 tasks | 3 files |
| Phase 5 P3 | 14min | 3 tasks | 4 files |
| Phase 06 P01 | 13min | 2 tasks | 5 files |
| Phase 06 P02 | 12min | 2 tasks | 4 files |
| Phase 06 P03 | 11min | 2 tasks | 1 files |
| Phase 06 P04 | 13min | 2 tasks | 6 files |
| Phase 06 P05 | 11min | 3 tasks | 7 files |
| Phase 06 P06 | 16min | 3 tasks | 5 files |
| Phase 06 P07 | 69m | 3 tasks | 10 files |
| Phase 07 P01 | 17min | 2 tasks | 5 files |
| Phase 07 P02 | 34min | 2 tasks | 4 files |
| Phase 07 P03 | 30min | 2 tasks | 2 files |
| Phase 07 P04 | 16min | 2 tasks | 2 files |
| Phase 07 P05 | 9min | 3 tasks | 7 files |
| Phase 07 P06 | 12min | 2 tasks | 4 files |
| Phase 08 P01 | 26min | 3 tasks | 11 files |
| Phase 08 P02 | 16min | 2 tasks | 2 files |
| Phase 08 P03 | 45min | 2 tasks | 9 files |
| Phase 08 P04 | 70min | 3 tasks | 4 files |
| Phase 08 P08 | ~3h | 2 tasks | 4 files |
| Phase 08 P05 | 39min | 3 tasks | 1 files |
| Phase 08 P06 | 58min | 2 tasks | 6 files |

## Accumulated Context

### Decisions

Recent decisions affecting current work (full log in PROJECT.md):

- v2.0: Repository identity is canonical across main checkouts, subdirectories, symlinks, and linked worktrees.
- v2.0: If the main checkout is not on the resolved default, preserve and show it; create or reuse a separate managed default-branch worktree.
- v2.0: Opening never silently switches branches, fetches, or guesses a default branch.
- v2.0: Worktree removal and dormant-branch deletion fail closed on unsafe or indeterminate Git state.
- v2.0 (superseded scope): Full repository hierarchy and applicable actions were originally planned for local TUI, remote TUI, and PWA.
- v2.0 scope revision: Ship one shared lifecycle authority plus a local-TUI dogfood slice; defer dormant branch rows/deletion and remote/PWA hierarchy, and target `v2.0.0-beta` readiness without publishing.
- [Phase 05]: Repository identity uses the canonical common directory plus Git's main-first worktree inventory; show-toplevel only selects an inventory member.
- [Phase 05]: Default resolution prefers the main branch upstream remote, then origin, and requires exact commit-verified local remote HEAD targets.
- [Phase 05]: Managed default creation verifies full refs, uses exact branch semantics, and rediscovery proves common directory, path, and branch.
- [Phase 5]: Repository and checkout keys are persisted monotonic u64 newtypes scoped to one workspace state file.
- [Phase 5]: Legacy migration accepts reconciled identity as an injected value and never infers baude ownership from is_worktree.
- [Phase 5]: Only NotFound is first-run state; malformed, unsupported, unreadable, and invalid aggregates are path-aware blocking errors.
- [Phase 5]: Primary runtime dispatch uses durable active intent plus a stable checkout-key runtime association, never display name or cwd.
- [Phase 5]: Checkout reuse requires fresh common-directory, canonical-path, full-ref, and unlocked/non-prunable reconciliation.
- [Phase 5]: App and daemon load failures block automatic saves and subsequent process launches until state evidence is repaired.
- [Phase 6]: Branch text is accepted only after literal Git validation, exact local-ref classification, and fresh inventory checks.
- [Phase 6]: Repository lifecycle mutations reserve by durable RepositoryKey and release through RAII guards.
- [Phase 6]: App and Manager persist shared activation transitions before associating runtimes by CheckoutKey.
- [Phase 6]: Managed branch labels are bounded display components only; durable repository and checkout keys supply filesystem identity.
- [Phase 6]: Pre-replacement creation failures compensate only newly added managed worktrees through verified plain Git removal while retaining the branch.
- [Phase 6]: Committed-save and spawn failures retain one durable active child for retry without a runtime association.
- [Phase 6]: Empty valid porcelain-v2 output is the only status-clean observation; malformed output and command failure remain indeterminate.
- [Phase 6]: Removal authorization requires fresh exact managed linked topology and yields only an opaque path/parent/ref/OID target.
- [Phase 6]: Any recursive submodule record blocks non-force worktree removal.
- [Phase 06]: Retained conversation IDs are optional opaque strings with an explicit serde default and never participate in path or ownership identity. — Preserves backend conversation context without weakening durable repository identity.
- [Phase 06]: Close plans snapshot runtime context, save inactive intent, then stop exactly one checkout-key runtime while retaining checkout and repository membership. — Makes save-before-stop ordering shared and non-destructive across App and Manager.
- [Phase 06]: Pre-replacement close failures restore memory and leave the runtime live; post-replacement directory-sync failures keep inactive memory, stop the runtime, and mark persistence dirty. — Keeps memory aligned with the atomic replacement commit boundary.
- [Phase 06]: Targeted resume IDs travel only as opaque PTY environment data referenced by a fixed quoted variable. — Prevents persisted backend data from becoming shell syntax.
- [Phase 06]: Reopen persists active intent only after fresh exact checkout reconciliation and before one runtime effect. — Blocks stale topology and duplicate launch state.
- [Phase 06]: Same-checkout reopen reservations return pending while conflicting repository mutations remain busy. — Allows one checkout-key runtime path under repeated requests.
- [Phase 6]: The first safe-removal preflight supplies target-naming confirmation data but its verified Git token is discarded; confirmation always obtains a new token after runtime stop.
- [Phase 6]: Failures before plain Git removal restore one runtime when one was active, while postcondition or persistence failures after Git commitment never recreate topology.
- [Phase 6]: A pre-replacement final-save failure keeps an unavailable recovery child in memory while old durable context remains on disk; a committed replacement keeps child deletion in memory.
- [Phase 06]: LifecycleCandidate is opaque and only LifecycleEngine selects checkout lifecycle and owned-runtime candidates.
- [Phase 06]: Pre-replacement persistence failure preserves the existing runtime; committed replacement continues the authorized effect and records dirty durability.
- [Phase 06]: Tracked App and Manager restarts use registered lifecycle launch events before PTY release.
- [Phase 07]: Structural hierarchy and ordering come only from persisted repository state; runtime, status, and archive facts are decoration joins. — Prevents volatile process churn from changing structural identity or order.
- [Phase 07 dogfood]: Normal navigation prefers checkout/worktree rows and visits a repository parent only when that repository has no available checkout. — Keeps repository context visible without making duplicate parent/checkout stops the common path.
- [Phase 07 dogfood]: Invalid local selection and restart prefer the first available checkout, then a repository parent with no available checkout, then flat remote rows. — Keeps local context deterministic while making the worktree the primary item.
- [Phase 07 dogfood]: Existing non-Git folders are schema-v3 standalone root rows with a separate durable key/runtime map and no Git authority. — Preserves exact runtime ownership without weakening repository/checkout invariants or inventing fake topology.
- [Phase 07]: Only eligible inactive retained checkouts expose RetryReopen; only implemented activation, teardown, and stopped-active paths expose RetryRecovery. — Keeps every presented retry capability paired with a concrete safe App dispatcher.
- [Phase 07]: Lowercase x is retained close only; Shift+X alone enters separately confirmed, freshly rechecked managed-worktree removal. — Prevents ordinary close intent from escalating into physical Git topology removal.
- [Phase 07]: Local action authority resolves durable RepositoryKey and CheckoutKey rather than presentation or runtime absence. — Keeps stale glyphs, labels, status, and volatile runtime decoration non-authoritative.
- [Phase 07]: Hierarchy strings are clipped by terminal cell clusters with branch/path tails preserved and no new dependency.
- [Phase 07]: Responsive layout exposes explicit pane visibility so hidden single-pane content is never resized or used as an input target.
- [Phase 07]: Full and narrow hierarchy hints derive only from ActionView lifecycle capability.
- [Phase 07]: Real-Git dogfood runs in an exact child test process so fixture-scoped HOME/XDG isolation cannot race parallel workspace tests.
- [Phase 07]: Restart selects the first rendered local repository parent; retained child reopening requires explicit durable CheckoutKey reselection.
- [Phase 07]: Daemon compatibility remains a flat SessionInfo array with retained-close DELETE and no hierarchy or remove-worktree route.
- [Phase 07]: Published history remains exactly 0.14.0 while source and proposal metadata target 2.0.0-beta.
- [Phase 07]: Artifact readiness copies only the supported target and two-binary archive shape, with read-only contents permission and no publication authority.
- [Phase 07]: The beta is described only as a local source-readiness target; stable remote install guidance remains at v0.14.0.
- [Phase 07]: Morning UAT evidence is created only from observed commands, screenshots, and certification outcomes; implementation creates no placeholder evidence file.
- [Phase 08]: Phase 8 guard is gated on cfg(any(test, feature = "test-support")), not cfg(test): rustc --test sets `test` per crate, so a cfg(test)-only guard is absent from baude's and bauded's test binaries — the two that leaked (RESEARCH Deviation 1).
- [Phase 08]: assert_contained is a containment predicate (did this resolve inside BAUDE_TEST_FIXTURE_ROOT?), not an override-presence check, so it needs no arming state and holds for the first test in a binary (D-09).
- [Phase 08]: Test fixture helpers return owner structs (AdmissionRepo, FixtureRepo), never a bare TestRedirect or a path alone — a returned guard drops in the same statement and leaves the fixture unredirected while still compiling.
- [Phase 08]: meta::claude_config_dir isolates by ambient redirect, not by parameter: both ClaudeMeta::poll call sites keep their signatures and neither gains an _at variant (D-03).
- [Phase 08]: The three real-root resolvers (meta CLAUDE_CONFIG_DIR, persist XDG+baude, git XDG+/tmp tail) stay separate — same shape, different heads and tails; collapsing them would change production behavior.
- [Phase 08]: bauded's duplicate config resolver was deleted rather than separately guarded; its chain was byte-identical to persist's, so production paths and the already-written VAPID key are unchanged (T-08-08 still accepted).
- [Phase 08]: Phase 08 plan 03: workspace identity resolves from a thread-local &'static override consulted ahead of the retained ACTIVE OnceLock; the OnceLock stays as the production fallback so production identity lifetime is unchanged.
- [Phase 08]: Phase 08 plan 03: support-build workspace::active() panics on PROVENANCE, not containment — no override held means panic even when the cache is seeded and every derived path is contained (D-08).
- [Phase 08]: Phase 08 plan 03: each fixture identity is Box::leak'd to satisfy active()'s &'static return type (D-07); bounded by fixture count, confined to cfg(any(test, feature = "test-support")), the accepted disposition of threat T-08-07.
- [Phase 08]: Phase 08 plan 03: the TUI's statusline/hook/permission-mcp/--version/--help arms stay uninitialized — bridge.rs, hook.rs and permission.rs reach zero identity readers, so initializing them would add a config read to Claude Code's critical path for no reader.
- [Phase 08]: 08-04: Removal authorization implemented verbatim from locked decision B — Removable requires ShapeMatch AND NotReferencedByState AND (Empty OR GitDisownsIt); NoGitdir recorded but inert
- [Phase 08]: 08-04: Evidence::blocking_role() matches all nine variants with NO wildcard arm — a future variant is a compile error, never a silent authorization widening
- [Phase 08]: 08-04: Blockers split by role — ProvesLive (ReferencedByState, ContainsCheckout) -> Live; PreventsConclusion (IsSymlink, StateUnreadable) -> Indeterminate; neither can reach Removable
- [Phase 08]: 08-04: Removable carries RemovalProof (workspaces_checked + ClearingSignal + observed evidence) so decision C's prune-time re-derivation has something to match against
- [Phase 08]: 08-04: git::worktree_inventory extracted pub(crate) because discover_repository rejects an absent path with SelectedWorktreeMissing — that absence IS disownment, so GitDisownsIt would have been unreachable
- [Phase 08]: 08-04: persist::load_named_at widened (not load_for_workspace_strict_at) — the latter calls hold_state_lock, which writes, violating the D-16 read-only scan contract
- [Phase 08]: 08-08: worker/subprocess escapes are closed by COMPILING the worker out of test builds (cfg(test) UsagePoller::start, cfg(not(test)) ccusage/date helpers), not by disabling it at runtime — a detached worker outlives its handle
- [Phase 08]: 08-08: App::new disables the remote-selection EXPRESSION under cfg(test) rather than nulling app.remote afterwards, and pins desktop_notify_enabled false in test builds
- [Phase 08]: 08-08: one support-only pty::configure_test_child at the single spawn convergence point owns child-env policy: env_clear, caller env first, protected roots/shell/startup keys last, /bin/bash --noprofile --norc -i; production $SHELL -il verbatim under cfg(not)
- [Phase 08]: 08-08: no broad test run — the plan forbids it before 08-06 task 1 lands manager ownership; the dispatch's full-suite criterion is deferred to 08-06 task 2 and logged to WINDOWS.md as unrun-verify
- [Phase 08]: 08-05: prune re-derives every fact and requires the re-derived RemovalProof to EQUAL the approved one, so a candidate that newly qualifies is refused as firmly as one that stopped qualifying (decision C)
- [Phase 08]: 08-05: candidate records in a transported ScanReport carry relative path components only — never an absolute path — so an edited report cannot name a directory outside the base the pruning process resolved for itself (T-08-25)
- [Phase 08]: 08-05: an incomplete state inventory withholds clearance from EVERY candidate in the scan, not just the affected workspace (T-08-16)
- [Phase 08]: 08-05 DEVIATION: decision C's gitdir-routing clause is structurally unreachable, so prune REFUSES a gitdir-bearing candidate (RefusalReason::GitdirPresent) instead of routing it to git's verified-removal path — strictly narrower than authorized
- [Phase 08]: 08-06: a fixture helper returns the OWNER, never (root, workspace) — the tuple shape dropped its TestRedirect at the return and left every caller unredirected
- [Phase 08]: 08-06: suite-level containment is asserted by an observer EXTERNAL to the test process (scripts/assert-real-roots-untouched.sh), with the CI after-step running even when the suite fails
- [Phase 08]: 08-06: TISO-01/02/03 marked delivered; TISO-04 stays partial for plan 08-07, and the uncontained open_editor/pbcopy spawns stay open as WINDOWS entry 6

### Pending Todos

- Run Linux/runtime certification, independent deep lifecycle review, phase verification, and Nyquist validation for Phase 6.
- Run manual wide/narrow TUI dogfood, supported CI/Linux/runtime certification, independent review, Phase 7 verification, Nyquist/UI-SPEC approval, requirement/phase completion, audit/cleanup, and publication decision.
- Live-dogfood schema-v3 standalone non-Git admission, canonical deduplication, close/reopen, missing-folder recovery, and Git-action refusals.

### Blockers/Concerns

- Corrective 06-07 closes prior CR-01 through CR-03 locally, but an independent deep review must confirm zero unresolved Critical/High findings.
- Linux synchronized gate/release and descendant process-group extinction remain uncertified.
- CORE requirement checkoff and Phase 6 completion remain blocked on certification, phase verification, and Nyquist approval.
- REL-03 and remaining Phase 7 requirement/phase completion remain blocked on morning dogfood, certification, review, verification, and approvals.
- TISO-01 and TISO-03 remain PARTIAL after 08-01: requirements.mark-complete reports both not_found in REQUIREMENTS.md (entries carry a '(partial)' suffix) and neither is fully delivered until plans 02/03/06 migrate the remaining consumers. Do not check them off before plan 06.
- Five baude-core/src/lifecycle.rs tests fail plan-01 containment (no TestRedirect held); measured pre-existing at 08-03's RED commit f6a3b1c. lifecycle.rs is in no phase-08 plan's files_modified — deferred to 08-06, which cannot go green until they hold a root and a literal identity. See 08 deferred-items.md.
- ui::tests::ui_fixture_isolation_after_helper_return and ui_fixture_isolation_nested_restore are committed #[ignore]d: they build an App whose UsagePoller is uncontained until 08-08. Plan 08-08 task 1 must remove the attribute and run them.
- 08-08 left the full workspace test suite unrun (plan forbids broad runs before 08-06 task 1 completes manager ownership). Real pass/fail numbers for the whole suite are owed by 08-06 task 2.

## Deferred Items

Items carried forward from the v0.7 close (code-complete; human-only verification):

| Category | Item | Status |
|----------|------|--------|
| UAT | Phase 1 info overlay effort/thinking/PR rows | pending |
| UAT | Phase 3 PWA activity strip and TUI `v` overlay | pending |
| UAT | Phase 4 live `claude` permission MCP wire contract | pending |
| UAT | Phase 4 PWA approve/deny card and distinct push | pending |
| verification | Phase 1/3/4 human-needed verification artifacts | pending |
| deferred | First-real-phone Web Push verification from v0.5 | pending |

## Session Continuity

Last session: 2026-09-15T17:36:09.713Z
Stopped at: Completed 08-06-PLAN.md
Resume file: None

## Deferred Items

Items acknowledged and deferred at milestone close, most recent first:

| Category | Item | Status | Deferred At | Milestone |
|----------|------|--------|-------------|-----------|
| uat_gaps | 05/05-HUMAN-UAT.md | partial | 2026-09-03 | v2.0 |
| uat_gaps | 07/07-UAT-EVIDENCE.md | unknown (evidence log, not scenario UAT) | 2026-09-03 | v2.0 |
| uat_gaps | 01/01-UAT.md (archived v0.7) | testing, 1 pending | 2026-09-03 | v2.0 |
| uat_gaps | 02/02-UAT.md (archived v0.7) | testing | 2026-09-03 | v2.0 |
| uat_gaps | 03/03-UAT.md (archived v0.7) | testing, 1 pending | 2026-09-03 | v2.0 |
| uat_gaps | 04/04-UAT.md (archived v0.7) | testing | 2026-09-03 | v2.0 |
| verification_gaps | 05/05-VERIFICATION.md | human_needed | 2026-09-03 | v2.0 |
| verification_gaps | 01/01-VERIFICATION.md (archived v0.7) | human_needed | 2026-09-03 | v2.0 |
| verification_gaps | 03/03-VERIFICATION.md (archived v0.7) | human_needed | 2026-09-03 | v2.0 |
| verification_gaps | 04/04-VERIFICATION.md (archived v0.7) | human_needed | 2026-09-03 | v2.0 |
| deferred_items | 06/deferred-items.md: legacy App removal route is_dirty note (superseded by 06-06 inspect_removal) | acknowledged | 2026-09-03 | v2.0 |
| deferred_items | 07/deferred-items.md: pre-existing docker smoke shellcheck SC2015/SC2034 | acknowledged | 2026-09-03 | v2.0 |

## Operator Next Steps

- Start the next milestone with /gsd-new-milestone
