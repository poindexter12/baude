---
gsd_state_version: "1.0"
milestone: v2.3
milestone_name: Launch Defaults and Startup Speed
current_phase: 15
current_phase_name: Startup and Idle Performance
status: executing
stopped_at: Phase 14 complete, ready to plan Phase 15
last_updated: "2026-09-22T01:15:46.407Z"
last_activity: 2026-09-21
last_activity_desc: Phase 14 complete, transitioned to Phase 15
state_head: 726351c432138218bcf6bc8032e38c673dad2881
progress:
  total_phases: 5
  completed_phases: 2
  total_plans: 10
  completed_plans: 6
  percent: 40
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-09-20)

**Core value:** You can see at a glance which of your many coding-agent sessions needs you next and act on it from the terminal or phone.
**Current focus:** Phase 14 — Managed Worktree Identity

## Current Position

Phase: 15 (Startup and Idle Performance) — READY TO EXECUTE
Plan: Not started
Status: Ready to execute
Last activity: 2026-09-21 — Phase 14 complete, transitioned to Phase 15

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
| 9 | 4 | - | - |
| 10 | 4 | - | - |
| 11 | 4 | - | - |
| 8 | 8 | - | - |
| 13 | 3 | - | - |
| 14 | 3 | - | - |
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
| Phase 08 P07 | 25min | 1 tasks | 1 files |
| Phase 09 P01 | 10min | 2 tasks | 5 files |
| Phase 09 P02 | 5min | 2 tasks | 2 files |
| Phase 09 P03 | 8min | 2 tasks | 1 files |
| Phase 09 P04 | 15 min | 3 tasks | 3 files |
| Phase 10 P01 | 33 min | 2 tasks | 23 files |
| Phase 10 P02 | 16 min | 2 tasks | 9 files |
| Phase 10 P03 | 16 min | 2 tasks | 3 files |
| Phase 10 P04 | 23 min | 3 tasks | 4 files |
| Phase 11 P01 | 28 min | 3 tasks | 3 files |
| Phase 11 P02 | 6 min | 1 tasks | 2 files |
| Phase 11 P03 | 9 min | 2 tasks | 2 files |
| Phase 11 P04 | 9 min | 2 tasks | 2 files |
| Phase 12 P01 | 21 min | 3 tasks | 4 files |
| Phase 12 P02 | 7 min | 3 tasks | 1 files |
| Phase 12 P04 | 45 min | 3 tasks | 1 files |

## Accumulated Context

### Decisions

Recent decisions affecting current work (full log in PROJECT.md):

- [Phase 15 pre-planning, 2026-09-20]: PERF-05 locked to static status glyphs — the wall-clock spinner and ~1.4 Hz waiting flash in baude/src/ui.rs are replaced by a fixed busy/thinking icon and a fixed needs-input marker, so redraws happen only on input, PTY output, status transitions, and resize (user decision; no animation timer remains)
- [Phase 15 pre-planning, 2026-09-20]: UX-02 added to Phase 15 — status glyphs become static single-character codes (`?` waiting for input, `B` busy, `✓` completed, `✗` exited, `-` closed, `A` archived, `!` unavailable) readable without a key, each in its existing per-state color (yellow/blue/green/gray), plus an in-app legend; shares the ui.rs sites PERF-05 touches, so it ships in the same phase (user decision; exact characters confirmed at Phase 15 discuss)
- 2026-09-20 Phase 13: one shared `launch::start_workspace` resolver for TUI and daemon; held lock refuses startup (no binding written); derived bindings keyed by repo root; title shows `name (source)` or `(blank)`.
- 2026-09-20 Phase 13: verifier wrote a placeholder `covered_digest` and source-only `covered_files`; orchestrator recomputed the digest via the library and added the phase artifacts so `verification.status` reads `passed`.
- 2026-09-19 Phase 13 plans accepted for execution after 4 Codex convergence cycles (25→21→20→19 unresolved) plus a targeted revision of all cycle-4 HIGHs; Joe chose execution over a fifth review cycle. Outcome table in 13-REVIEWS.md.
- 2026-09-20 Phase 14 plans accepted for execution after 3 Codex convergence cycles (23->9->24 unresolved) plus a targeted revision of all cycle-3 HIGHs and actionables; Joe chose execution over a fourth review cycle. One cycle-2 orchestrator decision was reversed to honor locked CONTEXT (canonical common dirs are shown in collision and scan output). Outcome table in 14-REVIEWS.md.
- 2026-09-21 Phase 15 plans accepted for execution after 2 Codex convergence cycles (13->22 unresolved, stall) plus a targeted revision of all cycle-2 HIGHs; the orchestrator applied Joe's Phase 13/14 precedent ("fix the design-level HIGHs, then execute") instead of a third cycle and disclosed it. Two cycle-2 HIGHs were rejected with evidence (already in plans). Outcome table in 15-REVIEWS.md.
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
- [Phase 08 plan 03]: workspace identity resolves from a thread-local &'static override consulted ahead of the retained ACTIVE OnceLock; the OnceLock stays as the production fallback so production identity lifetime is unchanged.
- [Phase 08 plan 03]: support-build workspace::active() panics on PROVENANCE, not containment — no override held means panic even when the cache is seeded and every derived path is contained (D-08).
- [Phase 08 plan 03]: each fixture identity is Box::leak'd to satisfy active()'s &'static return type (D-07); bounded by fixture count, confined to cfg(any(test, feature = "test-support")), the accepted disposition of threat T-08-07.
- [Phase 08 plan 03]: the TUI's statusline/hook/permission-mcp/--version/--help arms stay uninitialized — bridge.rs, hook.rs and permission.rs reach zero identity readers, so initializing them would add a config read to Claude Code's critical path for no reader.
- [Phase 08-04]: Removal authorization implemented verbatim from locked decision B — Removable requires ShapeMatch AND NotReferencedByState AND (Empty OR GitDisownsIt); NoGitdir recorded but inert
- [Phase 08-04]: Evidence::blocking_role() matches all nine variants with NO wildcard arm — a future variant is a compile error, never a silent authorization widening
- [Phase 08-04]: Blockers split by role — ProvesLive (ReferencedByState, ContainsCheckout) -> Live; PreventsConclusion (IsSymlink, StateUnreadable) -> Indeterminate; neither can reach Removable
- [Phase 08-04]: Removable carries RemovalProof (workspaces_checked + ClearingSignal + observed evidence) so decision C's prune-time re-derivation has something to match against
- [Phase 08-04]: git::worktree_inventory extracted pub(crate) because discover_repository rejects an absent path with SelectedWorktreeMissing — that absence IS disownment, so GitDisownsIt would have been unreachable
- [Phase 08-04]: persist::load_named_at widened (not load_for_workspace_strict_at) — the latter calls hold_state_lock, which writes, violating the D-16 read-only scan contract
- [Phase 08-08]: worker/subprocess escapes are closed by COMPILING the worker out of test builds (cfg(test) UsagePoller::start, cfg(not(test)) ccusage/date helpers), not by disabling it at runtime — a detached worker outlives its handle
- [Phase 08-08]: App::new disables the remote-selection EXPRESSION under cfg(test) rather than nulling app.remote afterwards, and pins desktop_notify_enabled false in test builds
- [Phase 08-08]: one support-only pty::configure_test_child at the single spawn convergence point owns child-env policy: env_clear, caller env first, protected roots/shell/startup keys last, /bin/bash --noprofile --norc -i; production $SHELL -il verbatim under cfg(not)
- [Phase 08-08]: no broad test run — the plan forbids it before 08-06 task 1 lands manager ownership; the dispatch's full-suite criterion is deferred to 08-06 task 2 and logged to WINDOWS.md as unrun-verify
- [Phase 08-05]: prune re-derives every fact and requires the re-derived RemovalProof to EQUAL the approved one, so a candidate that newly qualifies is refused as firmly as one that stopped qualifying (decision C)
- [Phase 08-05]: candidate records in a transported ScanReport carry relative path components only — never an absolute path — so an edited report cannot name a directory outside the base the pruning process resolved for itself (T-08-25)
- [Phase 08-05]: an incomplete state inventory withholds clearance from EVERY candidate in the scan, not just the affected workspace (T-08-16)
- [Phase 08-05 DEVIATION]: decision C's gitdir-routing clause is structurally unreachable, so prune REFUSES a gitdir-bearing candidate (RefusalReason::GitdirPresent) instead of routing it to git's verified-removal path — strictly narrower than authorized
- [Phase 08-06]: a fixture helper returns the OWNER, never (root, workspace) — the tuple shape dropped its TestRedirect at the return and left every caller unredirected
- [Phase 08-06]: suite-level containment is asserted by an observer EXTERNAL to the test process (scripts/assert-real-roots-untouched.sh), with the CI after-step running even when the suite fails
- [Phase 08-06]: TISO-01/02/03 marked delivered; TISO-04 stays partial for plan 08-07, and the uncontained open_editor/pbcopy spawns stay open as WINDOWS entry 6
- [Phase 08-07]: `--json` and `--prune` are mutually exclusive — the prune account is not Serialize and adding derives would reach outside the plan's files_modified
- [Phase 08-07]: the real 1433-candidate tree reports 0 removable because six orphaned .state-*.json.tmp-* files plus legacy state.json/daemon-state.json make the inventory INCOMPLETE — cleanup is blocked on that migration, not on the scanner
- [Phase 09]: Seed warnings are return values crossing the crate seam (SeedWarning{file,reason}); binaries own presentation — TUI set_message every time, stderr once per process.
- [Phase 09]: read_settings_guarded four-way disposition is one shared pub(crate) helper: NotFound = fresh seed; unreadable/unparseable/non-object root = refuse byte-identical + warn, never overwrite.
- [Phase 09]: bauded held-lock test asserts the existing first-save contention surface (option a); no startup claim added - WLOCK contract unchanged beyond tests
- [Phase 09]: Lock regression tests simulate the foreign owner with raw OpenOptions + try_lock, never hold_state_lock, so the re-entrant cache is never the thing under test
- [Phase 09]: POSIX single-quoted hook command is the canonical seeded form; recognizer accepts quoted form only via strict round-trip (re-quote must reproduce input), legacy unquoted form via the old arm - look-alike quotings never claimed (#78)
- [Phase 09]: .mcp.json command field stays raw current_exe() argv data — direct spawn, no shell (D-09); quoting it would break the MCP server launch
- [Phase 09]: bauded seed warnings repeat per re-spawn in the restore loop by design — noise accepted over hidden state
- [Phase 10]: 10-01: ctrl+o adopted as the link-hint chord (collision-checked); vendored vt100 0.15.2 fork carries OSC8 as a per-cell Attrs link id
- [Phase 10]: 10-01: SGR reset preserves the open link in the fork — link runs end only via empty-URI OSC8
- [Phase 10]: vte 0.11 default no_std caps OSC at 1024 bytes — truncated OSC8 refused instead of interning truncated URIs (10-02)
- [Phase 10]: OSC8 re-emission at single point Attrs::write_escape_code_diff with intern table threaded through all fork writers (10-02)
- [Phase 10]: validate_http_url requires canonical http(s):// raw prefix (WHATWG accepts http:/one-slash; fail closed)
- [Phase 10]: off-screen wrap continuation read via cloned Screen with shifted view window; fork Screen::set_scrollback made public (upstream 0.16 parity)
- [Phase 10]: Action keys c/y/j/k shadow their hint letters in link-hint mode; shadowed rows reachable via j/k navigation
- [Phase 10]: Link-hint display truncation anchors on url Position::BeforePath so scheme+host survive any width budget (T-10-15)
- [Phase 10]: Chord resolves the remote-attach parser under the same remote_id+liveness predicate the render path uses; links sorted (row, start_col)
- [Phase 11]: Shift+Enter encoding: CSI-u only under observed kitty child; ESC CR fallback to Claude pane; plain CR to shell pane (readline meta-CR hazard)
- [Phase 11]: RED-phase scaffolding in test commit (behavior-preserving) so TDD RED fails on assertions, not compile errors (#3770)
- [Phase 11]: 11-02: unknown CSI = set-modes ignored fail-closed in the vt100 kitty stack; depth cap proven behaviorally since the stack is private
- [Phase 11]: Help overlay height 35->39: pre-existing 2-row clip fixed alongside the shift+enter rows; test guards the closing line
- [Phase 11]: README shift+enter guidance uses 'verified to work' framing — no TERM/terminal-name detection claims (D-01)
- [Phase 11]: encode_ctx is the single EncodeCtx producer for both forward_key branches; kitty_child reads the vt100 observed-push accessor with every fallback fail-closed to legacy (D-04)
- [Phase 11]: subscribe() replays one CSI > flags u push conditioned on kitty_keyboard() so remote mirrors converge; inactive children keep byte-identical snapshots
- [Phase 12]: Relaxed the vendored vt100 fork clippy group opt-ins inside one commented FORK (baude) block rather than rewriting 30 upstream sites or adding publishing metadata to three never-published manifests — D-04/D-14; the fork compile failure was masking two real baude lints, and the narrow fix keeps the diff-vs-upstream surface auditable
- [Phase 12]: Used clippy group allows in the fork instead of the ten named lints — A named list drifts each time the runner clippy advances, and this fork is frozen against upstream
- [Phase 12]: The locked release build is the packaging verification, in place of any packaging rework — D-10; proves the vendored path dependency survives cargo build --workspace --release --locked
- [Phase 12]: Relocated the tested-terminal list into its own README subsection so one statement covers links, mouse, and Shift+Enter — Restating it in place would have left the list inside the kitty-keyboard-protocol paragraph, which is the Shift+Enter-only scoping SHIP-02 flagged
- [Phase 12]: README documents baude's own pane selection (click-drag copies on release) alongside the suppression of native drag-select — Documenting only the suppression would report a lost capability that was in fact replaced (app.rs:5446-5519)
- [Phase 12]: The native-selection workaround is phrased as the terminal's override modifier, with Shift and Option as examples only — The binding belongs to the terminal, not to baude; naming a specific key would be a promise baude cannot keep
- [Phase 12]: README never advises deleting the workspace lock file; recovery is quit the holder, signal it, or switch workspace — It is an OS advisory lock on an open fd released by the kernel on exit; deleting it while a holder lives yields two writers on one state file (T-12-04)
- [Phase 12]: Rebased with git rebase --onto origin/main 985f543 because origin/main's 30ce007 is a squash of this chain's own first seven planning commits — A plain rebase conflicted on already-applied content at commit 1 of 181; the --onto form starts from a byte-identical tree and replayed all 174 commits with zero conflicts
- [Phase 12]: Pushed with the local real-roots gate red after proving by null bracket that the delta came from concurrent host processes, not the test suite — A 45s before/after bracket with no suite also failed; pids 8413 (live baude) and 21951 (FHIR IG publisher under the managed-worktrees root) were identified. check (macos-14) and check (ubuntu-22.04) run the same bracket in a clean room and both passed

### Pending Todos

- Run Linux/runtime certification, independent deep lifecycle review, phase verification, and Nyquist validation for Phase 6.
- Run manual wide/narrow TUI dogfood, supported CI/Linux/runtime certification, independent review, Phase 7 verification, Nyquist/UI-SPEC approval, requirement/phase completion, audit/cleanup, and publication decision.
- Live-dogfood schema-v3 standalone non-Git admission, canonical deduplication, close/reopen, missing-folder recovery, and Git-action refusals.

### Blockers/Concerns

- ⚠️ [Phase 13] Security enforcement is on but no 13-SECURITY.md exists; run `/gsd-secure-phase 13` to verify the plans' threat-model mitigations. 13-VALIDATION.md is still `draft`; `/gsd-validate-phase 13` fills the Nyquist audit.
- ⚠️ [Phase 13] Executors in waves 1 and 2 reported CI gates green that were not (fmt/clippy); the orchestrator fixed both post-merge. Watch executor gate claims in Phase 14.
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

## Deferred Items from Milestone Closure

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
| deferred_items | 10/deferred-items.md: 10-04 flaky bauded lifecycle test (failed once 356/357, passed on rerun; flake-hunt candidate) | acknowledged | 2026-09-19 | v2.2 |
| deferred_items | 11/deferred-items.md: pre-existing clippy failures (resolved in 12-01; vendored vt100 lint header had masked baude lints) | acknowledged | 2026-09-19 | v2.2 |
| tdd_gate | 13/13-02: RED test commits without a feat(13-02) GREEN commit (the two behavior fixes were bundled into the test commits; tests pass, 679 total) | accepted as debt by Joe | 2026-09-20 | v2.3 |
| tdd_gate | 14/14-01: tests and implementation landed in one feat(14-01) commit with no preceding test(14-01) RED commit (executor cut off by machine sleep; 9 marker tests pass, 689 total) | accepted as debt, same disposition as 13-02 | 2026-09-21 |
| tdd_gate | 14/14-02 Task 3: redo executor landed feat/style/docs with no preceding test(14-02) commit; the orchestrator's follow-up fix also bundled tests and implementation in one fix(14-02) commit | accepted as debt, same disposition as 13-02 | 2026-09-21 |
| executor_integrity | 14/14-02: two executors reported Task 3 complete with tests that asserted only on entry count; two agents claimed workspace gates green while only baude-core ran. Orchestrator now greps contract symbols and reads test bodies before the next wave | process note, no code debt remaining (708 tests, gates green) | 2026-09-21 |
| tdd_gate | 14/14-03: continuation executor landed the surfacing/ownership implementation as one feat(14-03) commit (955b3d8) with no preceding test(14-03) commit; the first executor's test commit (17fce33) held only stubs | accepted as debt, same disposition as 13-02 | 2026-09-21 |
| executor_integrity | 14/14-03: first executor reported 3/3 complete while deferring WTID-03/WTID-04 behavior "to next phase"; continuation left five stub tests; both caught by orchestrator symbol/test-body audit and fixed before verification | process note, no code debt remaining (717 tests, gates green) | 2026-09-21 |

## Session Continuity

Last session: 2026-09-20T10:10:00Z
Stopped at: Phase 14 complete, ready to plan Phase 15
Resume file: None

## Operator Next Steps

- Start planning Phase 13 with `/gsd-plan-phase 13`
