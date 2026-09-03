# Project Research Summary

**Project:** baude v2.0 Repository Worktree Management
**Domain:** Repository-centered AI session and managed Git worktree orchestration
**Researched:** 2026-08-30
**Confidence:** HIGH

## Executive Summary

Baude v2.0 should replace the flat, process-centered sidebar model with a durable repository aggregate whose children are live or reopenable sessions rooted in the main checkout and baude-managed linked worktrees. Experts build this by keeping Git authoritative for repository topology and safety, keeping product membership and UI state in versioned persistence, and treating PTYs as children rather than as repository identity. Opening or cloning a repository must be idempotent, preserve a parent after its session closes, and ensure a primary session through the active workspace backend without silently changing the user's checkout.

The recommended implementation changes no dependencies: extend `baude-core` with byte-safe Git discovery, shared repository/worktree value types, typed lifecycle errors, and an explicit migration; retain Serde JSON with atomic replacement; and project the hierarchy into one flat, stable ratatui row model. Managed worktrees must be created only after branch validation and Git inventory checks, and removal must be a fail-closed transaction: inspect before teardown, never force, let Git remove its own metadata, and commit persistence only after success. The same domain operations must power local and daemon paths so remote mode cannot weaken safety or reinterpret client-local paths.

The main risks are identity errors, ambiguous “default branch” semantics, lossy migration, deletion races, and local/daemon behavior drift. Mitigate them by using Git common-directory/worktree inventory plus persisted opaque membership keys, specifying an offline and non-destructive default-branch policy in requirements, preserving malformed/legacy state through atomic migration, serializing mutations per repository, and testing a parity matrix against temporary real repositories. UI hierarchy should come only after the domain and lifecycle contracts are stable.

## Key Findings

### Recommended Stack

No crate, framework, or lockfile upgrade is justified for v2.0. This milestone is a domain-model and Git-orchestration change. Continue invoking the installed Git CLI with argument arrays, preserve machine output as bytes where paths are involved, and keep the current workspace/backend boundaries. See [STACK.md](./STACK.md).

**Core technologies:**
- **Rust 2021 workspace and standard library:** domain models, subprocesses, worker threads, path handling, and atomic same-directory file replacement — all required primitives already exist.
- **System Git CLI:** canonical identity, default/current branch facts, worktree inventory, validation, dirty status, creation, and removal — preserves native config, credentials, and Git's safety rules.
- **`serde` 1.0.228 + `serde_json` 1.0.150:** workspace-scoped, versioned hierarchy persistence and explicit migration — already integrated and sufficient for small single-writer state.
- **`ratatui` 0.30.2:** parent/child rows, modals, and contextual hints — one-level flattening needs no tree widget.
- **Existing threads/channels and `anyhow`:** keep blocking Git work off render/async paths and preserve stderr/context in errors — no Tokio runtime is needed in the TUI.

**Critical compatibility requirements:** use `git worktree list --porcelain -z`; validate branches with `git check-ref-format --branch`; avoid lossy UTF-8 conversion for paths; keep new persisted fields backward compatible; and do not bundle dependency upgrades into this milestone.

### Expected Features

The feature set is a coherent safety contract, not merely nested rendering. See [FEATURES.md](./FEATURES.md).

**Must have (v2.0 table stakes):**
- **Persistent repository parents:** survive closed/exited sessions and deduplicate open, clone, subdirectory, symlink, and linked-worktree admission.
- **Primary/default-branch active-backend session:** first admission ensures one usable session through the active workspace backend; repeated admission focuses or reopens it rather than spawning duplicates.
- **Explicit nested hierarchy:** main/default and managed-worktree children appear under one parent with stable ordering and unchanged child attention/status behavior.
- **Verified named-branch worktree creation:** support new and existing branches, collision-proof managed paths, explicit bases, and Git-native checked-out-elsewhere refusal.
- **Separate close and remove lifecycles:** closing a session retains the child checkout; removal is a distinct confirmed operation.
- **Fail-closed safe removal:** staged, unstaged, untracked, conflicted, submodule, unknown, locked, or otherwise unsafe states block before session teardown; no `--force` or recursive deletion exists.
- **Typed contextual selection and shortcuts:** parent and child actions resolve through stable IDs; hints and confirmations state exactly what will happen.
- **Versioned persistence migration and reconciliation:** retain sessionless parents, migrate flat workspace state idempotently, preserve valid UI/session metadata, and show missing or changed worktrees as degraded rather than silently dropping them.
- **Local/daemon parity:** active backend/workspace isolation, identity, create/reuse, close/keep, clean removal, dirty refusal, restart, and typed errors have the same semantics on both owners.

**Should have (competitive behavior within the core):**
- **Repository as a durable control surface:** decouple “known repository” from “running agent.”
- **Agent-aware worktrees:** retain shell/editor, resume, archive, waiting/working, metadata, and conversation behavior on every child.
- **Git reconciliation with operator context preservation:** failed actions retain a navigable child/session and explain recovery.
- **Selection-derived action semantics:** repository-scoped actions work from either a parent or any child.

**Defer until after v2.0 validation:**
- Collapsible groups and read-only adoption/display of unmanaged external worktrees.
- Forget-repository bulk semantics, rich dirty summaries, worktree repair/move/lock controls, and branch deletion.
- Full Git GUI operations, automatic fetch/stash/commit/reset/clean, and bulk actions.
- PWA hierarchy redesign may remain an additive follow-up, but existing PWA/session endpoints must not gain weaker or destructive semantics.

### Architecture Approach

Add a PTY-free repository aggregate above existing sessions and keep storage, runtime ownership, and presentation separate. `App` and daemon `Manager` each own host/workspace-scoped repositories and sessions; `baude-core` owns shared discovery and lifecycle semantics; persistence records baude intent while Git records live topology; and the TUI/API are projections. The stronger cross-file recommendation is to persist opaque repository/worktree keys while retaining canonical main path/common-directory observations as attributes: this supports collision-free managed paths and degraded/moved state, while Git revalidation prevents stale IDs from authorizing operations. See [ARCHITECTURE.md](./ARCHITECTURE.md).

**Major components:**
1. **Core repository/worktree model:** stable membership keys, canonical main path, managed ownership, health state, and PTY-free parent metadata.
2. **Core Git discovery and lifecycle:** common-directory identity, NUL-delimited inventory parsing, typed default/branch state, validated create/reuse, dirty safety, and non-force removal.
3. **Versioned aggregate persistence:** workspace-local repository, worktree, and session records; legacy migration; reconciliation; backup/error retention; atomic write/rename.
4. **Local `App` aggregate:** idempotent repository admission, active-backend session ensure/reopen, contextual action dispatch, and nonblocking operation progress.
5. **Flattened sidebar projection:** one typed local/remote parent/child sequence shared by rendering, movement, selection repair, actions, and help text.
6. **Daemon `Manager` and additive API:** server-authoritative IDs and mutations, repository-native routes, typed safety conflicts, old session-route compatibility, and no lock held across blocking/awaited work.

**Patterns to enforce:** discover/normalize before mutation; reserve under lock, act outside it, then commit under lock; persist baude intent but reconcile against Git facts; scope identity to owner/host/workspace; and calculate notifications/status from sessions only.

### Critical Pitfalls

1. **Using checkout paths, basenames, or branch slugs as identity** — resolve Git common-directory/worktree membership, persist opaque keys, and use collision-free storage components independent of display labels.
2. **Guessing or forcing the default branch** — define a typed offline discovery policy; never fetch or switch on open; attach an already-safe checkout or require explicit resolution when ambiguous.
3. **Reimplementing Git worktree rules** — validate refs, inspect stable porcelain inventory, distinguish new/existing/already-checked-out/error cases, and never trust directory existence or retry every failure.
4. **Fail-open or teardown-first removal** — only `Clean` permits removal; recheck before plain `git worktree remove`; retain session, child, and metadata on any error.
5. **Lossy persistence migration** — use an explicit schema version, real legacy fixtures, atomic replacement and backup/error retention; never convert malformed input into empty state.
6. **Duplicate default sessions and mutation races** — enforce one owner/worktree/session-role key with pending reservations and per-repository mutation serialization.
7. **Session-only selection or local-only safety** — use stable typed parent/child/owner selection and one shared lifecycle contract across local TUI, daemon, remote TUI, REST, and retained compatibility endpoints.

## Implications for Roadmap

Based on dependencies and release risk, use five phases. Treat Phases 1 and 2 as domain/safety gates; do not start hierarchy polish before their contracts and integration tests pass.

### Phase 1: Repository Identity, Primary-Session Contract, and Persistence Migration
**Rationale:** Persistent parents, idempotent open, nested ownership, and safe migration all depend on one canonical admission and identity model. This phase must also settle the only material product ambiguity: what “default branch session” means for an existing checkout.

**Delivers:**
- PTY-free repository/worktree records with stable persisted membership keys and owner/workspace scope.
- Git discovery from main checkout, linked worktree, subdirectory, and symlink; explicit health/default-branch result types.
- Requirement contract for primary session behavior: fresh clone uses clone's checked-out remote default; existing repositories never mutate `HEAD`; an already registered checkout of the resolved default is reused; ambiguity/detached/unborn state is explicit.
- Idempotent `open_repository`/clone completion that ensures one active-backend primary session without backend data in repository records.
- Versioned state envelope, flat-session migration, reconciliation, atomic save, and corrupt-state retention.

**Addresses:** persistent repository parent, canonical identity, automatic active-backend primary session, durable hierarchy, migration, backend/workspace isolation, and honest missing state.

**Avoids:** duplicate parents/sessions, guessed default branches, path identity collisions, lossy migration, silent empty-state recovery, and cross-workspace leakage.

### Phase 2: Shared Managed-Worktree Lifecycle and Safe Removal
**Rationale:** Nested children are trustworthy only after create/reuse/remove behavior is centralized and tested against real Git repositories. This is the release-blocking data-safety phase.

**Delivers:**
- Byte-preserving worktree inventory and branch/ref validation.
- Collision-proof managed path allocation while honoring existing persisted paths.
- Explicit new-branch versus existing-branch/remote-branch flows, deterministic base selection, rediscovery verification, and typed errors.
- Distinct close-session/keep-worktree and remove-managed-worktree operations.
- Result-valued dirty/unknown checks, preflight-before-teardown, recheck, plain Git removal, postcondition verification, and persistence only after success.
- Per-repository operation reservations/coordinator so Git/process work does not block the TUI or daemon mutex and duplicate/racing mutations cannot win.

**Addresses:** named-branch nested worktrees, verified collisions, close/remove separation, fail-closed removal, Git-native safeguards, and context preservation on failure.

**Avoids:** arbitrary directory reuse, retry-any-error behavior, `--force`, direct deletion, dirty-check errors treated as clean, close-before-check, duplicate PTYs, and global-lock stalls.

### Phase 3: Local Hierarchy, Navigation, and Context-Aware Shortcuts
**Rationale:** The UI should consume stable domain operations rather than encode lifecycle rules. Building it after Phases 1–2 keeps destructive semantics out of event handlers.

**Delivers:**
- One flattened `VisibleRow`/sidebar projection with typed repository/session IDs and deterministic selection repair.
- Persistent parent rows with indented primary and managed-worktree children; stable waiting/working flashes and archive ordering.
- Parent/child action matrix for Enter, open/editor, create worktree, close, remove, and disabled actions.
- Context-derived status/help hints and confirmations naming owner, repository, branch, path, and whether files remain.
- Local open, clone, create/reopen, close/keep, remove, restore, shell/editor, resume, archive, and attention behavior wired through shared contracts.

**Addresses:** explicit hierarchy, hierarchy-aware navigation, contextual shortcuts, repository control surface, and agent-aware child behavior.

**Avoids:** fake repository sessions, stale/index-based selection, hidden action targets, duplicate parent alarms, status-based row reordering, and ambiguous destructive keys.

### Phase 4: Daemon and Remote-TUI Contract Parity
**Rationale:** Remote behavior must use the tested core semantics, but its API and host-scoped identity should stabilize only after local domain behavior is proven. Parity is required for v2.0 even if full PWA hierarchy presentation is deferred.

**Delivers:**
- Daemon repository aggregate and server-issued repository/worktree IDs scoped to daemon host and workspace.
- Additive repository hierarchy/open/create routes plus explicit close-session and remove-worktree operations with typed `409` safety conflicts.
- Existing flat session endpoints as compatibility projections/adapters; old-daemon fallback where non-destructive.
- Remote hierarchy polling/actions and workspace guard reuse; no client-local path authority.
- Shared tests proving dirty refusal leaves the daemon session alive and that blocking Git/process work does not span `.await` or the global manager lock.

**Addresses:** local/daemon parity, remote nested managed worktrees, active-backend isolation, daemon persistence, and compatibility.

**Avoids:** duplicated safety logic, path confusion across hosts, silently changed DELETE semantics, lock-held blocking work, and local-only completion claims.

### Phase 5: Migration, Recovery, and Cross-Surface Release Hardening
**Rationale:** Identity, persistence, Git mutation, UI selection, and remote concurrency interact in failure states that unit tests alone will miss. v2.0 should not ship until the observable behavior contract passes end to end.

**Delivers:**
- Legacy local and daemon fixture migration across Claude and OpenCode workspaces.
- Real-Git matrix for duplicate basenames, slash refs, linked admission, detached/unborn/no-remote repositories, locked/prunable/missing/moved children, untracked/conflicted/submodule dirt, and Git command failures.
- Race tests for repeated open/clone completion, concurrent API requests, create-versus-remove, and mutations between dirty preflight and removal.
- Local/remote parity UAT for open, primary-session ensure, nested create/reuse, close/keep, clean remove, dirty/unknown block, restart, degraded state, offline stale rendering, and old-daemon compatibility.
- Responsiveness and recovery verification, with no automatic prune/repair/adoption or destructive fallback.

**Addresses:** every v2.0 behavior-contract scenario and regression preservation for existing session functionality.

**Avoids:** “looks done” releases that lose state, duplicate agents, hide degraded worktrees, freeze during Git operations, or diverge across owners.

### Phase Ordering Rationale

- Canonical identity and migration precede parents and children because linked-worktree top-level paths are not repository identity and the flat schema cannot represent a sessionless parent.
- The default/primary-session policy is fixed with admission, before UI wording and API contracts encode inconsistent meanings.
- Shared lifecycle and safety precede UI and daemon adapters so neither surface can invent weaker removal semantics.
- Local hierarchy precedes remote projection to validate interaction semantics cheaply, while daemon parity remains a v2.0 requirement rather than an indefinite follow-up.
- Recovery and parity verification close the roadmap because persistence corruption, external Git mutation, and races span all prior components.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 1:** run `/gsd-plan-phase --research-phase 1` to settle default-branch/primary-session semantics, stable persisted ID versus path relationship, missing-parent representation, and downgrade/backup policy.
- **Phase 2:** run focused research or a spike if submodule superprojects are in scope; Git documents incomplete multiple-worktree support. Also validate the oldest supported Git version for required porcelain flags.
- **Phase 4:** research current daemon/PWA compatibility constraints and remote clone ownership if TUI and daemon filesystems may differ; remote clone jobs are otherwise out of scope.

Phases with standard patterns (skip research-phase unless requirements change):
- **Phase 3:** typed tree flattening, stable-ID selection, and contextual action matrices are well understood once the domain contract is fixed; use UI specification/discussion rather than more ecosystem research.
- **Phase 5:** test planning follows the explicit behavior, race, migration, and parity matrices already identified; no new technology research is needed.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Existing lockfile/source boundaries and official Git, Serde, and Rust documentation directly support the no-new-dependency recommendation. |
| Features | HIGH overall; MEDIUM interaction details | Table stakes follow the milestone and current baude behavior; exact default-branch fallback, parent key assignments, and forget behavior require product decisions. |
| Architecture | HIGH integration; MEDIUM default semantics | Current code boundaries strongly support a repository aggregate over existing sessions and shared core lifecycle; whether the primary child must be the main worktree or the resolved remote default remains ambiguous. |
| Pitfalls | HIGH | Critical hazards are evidenced in current code and official Git behavior, with concrete reproduction/test matrices. |

**Overall confidence:** HIGH

### Gaps to Address

- **Meaning of “default-branch session”:** requirements must choose between “main worktree's current branch” and “resolved remote default.” Recommended contract is non-destructive: clone uses checked-out default; existing repos discover offline, reuse an already-safe matching worktree, and surface ambiguity rather than switching/fetching.
- **Persistent identity shape:** research differs on runtime-only repository IDs versus persisted opaque IDs. Prefer persisted opaque repository/worktree keys for durable membership, managed path allocation, missing-state recovery, and daemon APIs, while always revalidating Git common-directory/path facts before mutation.
- **Parent forget behavior:** decide whether v2.0 omits it, disallows it while children exist, or only removes baude metadata/processes. It must never delete the main checkout or cascade-remove linked worktrees.
- **Remote clone ownership:** clarify whether daemon and TUI share a filesystem. If not, daemon-side clone needs its own background operation and should be separately scoped.
- **Submodules:** define support as safe refusal/preservation unless focused real-repository tests prove stronger behavior.
- **State rollback/corruption policy:** specify schema version, backup retention, error surfacing, and whether downgrade after the first v2 write is unsupported.
- **PWA scope:** daemon safety and compatibility are required; full PWA nested presentation can follow only if requirements explicitly include it.

## Sources

### Primary (HIGH confidence)
- [Git worktree documentation](https://git-scm.com/docs/git-worktree) — topology, stable porcelain/NUL output, one-branch checkout safeguards, locks/prunable state, clean-only removal, repair/prune, and submodule caveat.
- [Git rev-parse documentation](https://git-scm.com/docs/git-rev-parse) — top-level versus common-directory discovery and absolute path formats.
- [Git symbolic-ref documentation](https://git-scm.com/docs/git-symbolic-ref) — symbolic branch lookup and detached-HEAD behavior.
- [Git clone documentation](https://git-scm.com/docs/git-clone) — initial checkout from the remote's active branch.
- [Git remote documentation](https://git-scm.com/docs/git-remote) — optional/cached remote HEAD and network-query implications.
- [Git check-ref-format documentation](https://git-scm.com/docs/git-check-ref-format) — authoritative branch validation.
- [Git status documentation](https://git-scm.com/docs/git-status) — porcelain formats, untracked/submodule state, and optional-lock guidance.
- [Serde field attributes](https://serde.rs/field-attrs.html) — compatible defaulted fields.
- [Rust `std::fs::rename`](https://doc.rust-lang.org/std/fs/fn.rename.html) — same-filesystem atomic replacement constraints.
- Current baude project evidence: `.planning/PROJECT.md`, `README.md`, workspace manifests/lockfile, `baude-core/src/{git,persist,session}.rs`, `baude/src/{app,ui,remote}.rs`, and `bauded/src/{manager,api}.rs`.

### Secondary (MEDIUM confidence)
- Product deductions in [FEATURES.md](./FEATURES.md), [ARCHITECTURE.md](./ARCHITECTURE.md), and [PITFALLS.md](./PITFALLS.md) — contextual key assignments, exact primary/default policy, parent forgetting, remote-clone scope, and PWA sequencing require requirements validation.

### Tertiary (LOW confidence)
- None. Unresolved points are explicit product/scope decisions rather than unsupported technical claims.

---
*Research completed: 2026-08-30*
*Ready for roadmap: yes*
