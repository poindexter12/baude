# Phase 6: Shared Lifecycle Core Refactor - Corrective Research

**Researched:** 2026-08-30
**Domain:** Rust durable lifecycle protocol, Git worktree orchestration, PTY process ownership, crash recovery
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

### Worktree Creation
- Create new named branches from the repository's resolved default branch/primary checkout, regardless of which child supplied repository context.
- Activate eligible existing local branches only; remote-only branches must first become explicit local branches outside this phase.
- If a branch is already checked out in a worktree belonging to the repository, register/focus that existing worktree instead of forcing another checkout.
- Allocate managed paths from the durable repository key plus a sanitized branch label and collision suffix; verify any candidate through Git inventory before reuse.

### Close and Reopen
- Closing retains the checkout child, branch, first-seen order, shell/archive settings, and conversation-resume metadata while setting active intent false.
- Reopening reconciles Git first, durably records active intent, then launches/resumes through the active backend.
- Externally moved or branch-changed retained children become unavailable and cannot launch until topology is explicitly reconciled or repaired.
- Repeated or concurrent reopen requests reserve by durable checkout key and focus/return one runtime.

### Safe Removal
- Removal is permitted only for a verified baude-managed linked worktree whose tracked, untracked, conflicted, submodule, lock, and Git-status state is conclusively clean and safe.
- After preflight and confirmation, stop the agent immediately before a second preflight and plain Git removal; failure retains or restores durable runtime intent and user context.
- Serialize create, activate, reopen, and remove mutations per repository, then rediscover/recheck immediately before mutation.
- Successful removal deletes checkout membership/runtime but retains the local branch as dormant and always preserves the repository parent.

### the agent's Discretion
- Exact typed error taxonomy, mutation reservation representation, and rollback mechanism may follow current core/App/Manager patterns.
- Exact managed path collision suffix format is discretionary if stable, filesystem-safe, and verified against Git before reuse.

### Deferred Ideas (OUT OF SCOPE)

- Dormant branch rendering and safe merged-branch deletion are Phase 7.
- Remote TUI API presentation and PWA actions are Phases 8 and 9.
- Remote-only tracking branch creation, branch deletion during worktree removal, force removal, and automatic cleanup remain out of scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| CORE-01 | One core protocol owns topology decisions, persistence stages, and agent/shell effects. | Use one `LifecycleEngine<E: LifecycleEffects>` in `baude-core`; App and Manager submit requests and execute typed effects only. [VERIFIED: `.planning/REQUIREMENTS.md`; codebase inspection of `app.rs` and `manager.rs`] |
| CORE-02 | One explicit legal transition table governs protected states. | Replace the independently mutable `active_intent` + `health` pair with one tagged `CheckoutLifecycle` enum and one exhaustive reducer. [VERIFIED: `baude-core/src/repository.rs:63-129,166-179`; `baude-core/src/lifecycle.rs:530-608,942-1185`] |
| CORE-03 | Exact agent and shell ownership is durable before destructive/replacement effects. | Add `OwnedRuntime` with non-optional identities for every live process and persist a typed teardown/replacement candidate before signaling either process. [VERIFIED: `baude-core/src/repository.rs:49-59`; `baude-core/src/session.rs:333-381`; `06-REVIEW.md` CR-02/CR-03] |
| CORE-04 | App and Manager implement one effect contract and pass mirrored tests. | Drive both owners through the same engine and compare canonical transition/effect traces for every success and injected failure vector. [VERIFIED: duplicated current flows in `baude/src/app.rs:916-1209,1490-1829` and `bauded/src/manager.rs:763-920,1207-1552`] |
| CORE-05 | Typed durable candidates and provenance replace generic runtime overlays. | Persist operation-specific activation, launch, teardown, removal, and rollback candidates; prohibit reconstructing an operation from `active_intent`, `health`, or `runtime_checkouts`. [VERIFIED: `baude-core/src/repository.rs:63-115`; `baude-core/src/lifecycle.rs:951-984,1151-1185`] |
| CORE-06 | Startup and rollback use only legal transitions and converge without duplicate/orphan runtimes. | Recover process-bearing states first, then removal, activation, topology, and launch; every recovery step re-enters the same reducer and effect runner. [VERIFIED: current unsafe startup order at `baude/src/app.rs:535-545` and `bauded/src/manager.rs:357-379`; `06-REVIEW.md` CR-01] |
</phase_requirements>

## Summary

The corrective plan must be a refactor, not three local patches. Current `baude-core::lifecycle` supplies useful decisions and helpers, but App and Manager still own transaction order, persistence rollback, runtime restart, runtime-map deletion, and recovery sequencing. The two owners contain near-parallel activation, close, reopen, and removal transactions, and those copies have already diverged in shell capture and failed-cleanup behavior. [VERIFIED: `baude/src/app.rs:916-1209,1490-1829,2877-2920`; `bauded/src/manager.rs:763-920,1207-1552,1885-1895`; `06-REVIEW.md` CR-02/CR-03]

The smallest coherent correction is one durable, tagged checkout lifecycle plus one shared engine that emits typed effects and consumes typed acknowledgements. App and Manager should retain only surface behavior, storage location/configuration, PTY container access, and focus/notification details. They must not set lifecycle fields, choose rollback states, order startup recoveries, or remove runtime associations independently. [VERIFIED: current direct mutations in `baude-core/src/lifecycle.rs:233-343,530-608,942-1185`; duplicated adapter orchestration cited above]

This refactor requires a schema migration because current schema version 1 serializes `active_intent` and `CheckoutHealth` directly, while the corrected model must serialize one tagged lifecycle enum with operation-specific candidates. Serde supports explicit internally or adjacently tagged enum representations, and `deny_unknown_fields` preserves the repository's fail-closed load policy. [VERIFIED: `baude-core/src/persist.rs:16-23,206-251`; CITED: https://serde.rs/enum-representations.html; CITED: https://serde.rs/container-attrs.html]

**Primary recommendation:** Implement `LifecycleEngine<E>` and `CheckoutLifecycle` in existing tracked core files, migrate schema v1 to v2 explicitly, route both adapters through the engine, and make the mirrored effect-trace matrix the acceptance gate. [VERIFIED: proposed paths confirmed by `git ls-files` on 2026-08-30]

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Legal transition selection | `baude-core` domain | — | One exhaustive reducer must be the only writer of durable lifecycle state. [VERIFIED: CORE-01/CORE-02] |
| Persistence stage selection | `baude-core` protocol | App/Manager storage effect | Core chooses what candidate must commit and interprets pre/post-replacement acknowledgement; adapters only perform the save. [VERIFIED: `persist::SaveError::replacement_committed` at `persist.rs:98-118`] |
| Git inspection/mutation | `baude-core` Git effect | lifecycle engine | Existing opaque verified Git targets remain authoritative; the engine orders them. [VERIFIED: `lifecycle.rs:153-231`; Phase 06-03/06-06 summaries] |
| Agent and shell ownership | `baude-core` durable model | App/Manager live handle table | Core owns exact identities and legal forget rules; adapters retain in-process `Session` handles. [VERIFIED: `repository.rs:49-59`; `pty.rs:21-29,84-114`] |
| Startup recovery | `baude-core` engine | App/Manager bootstrap | Core returns the ordered recovery program; owners invoke it before ordinary restore/admission. [VERIFIED: current duplicated order at App 535-545 and Manager 357-379] |
| UI focus/messages/API mapping | App or Manager | — | These are surface-specific and do not authorize lifecycle transitions. [VERIFIED: App focus fields and Manager API ownership] |

## Current Control Flow and Review Blockers

### Existing flow

1. **Activation:** each owner prepares a pending child, saves it, calls core Git activation, saves again, then independently focuses/restarts/spawns and mutates `runtime_checkouts`. [VERIFIED: `app.rs:916-1086`; `manager.rs:763-920`]
2. **Close:** each owner snapshots and tears down first, then calls `plan_close`, saves inactive state, and independently restarts on a pre-replacement failure. This execution order contradicts `ClosePlan.effects`, which says save before stop. [VERIFIED: `lifecycle.rs:59-73,270-298`; `app.rs:1768-1829`; `manager.rs:1473-1552`]
3. **Reopen:** core plans active intent and dispatch, but each owner performs save and dispatch separately. [VERIFIED: `lifecycle.rs:530-608`; `app.rs:1098-1209`; `manager.rs` reopen paths]
4. **Removal:** App and Manager each reproduce stop, second inspection, context copying, authority revocation, save rollback, Git removal, tombstoning, child deletion, and final save. [VERIFIED: `app.rs:1620-1765`; `manager.rs:1336-1470`]
5. **Startup:** both owners reconcile activation before teardown, then scan `active_intent` and reopen. [VERIFIED: `app.rs:535-545`; `manager.rs:357-379`]

### Blockers that 06-07 must close

| Blocker | Root cause | Required architectural closure |
|---------|------------|--------------------------------|
| CR-01 occupied reuse overwrites teardown/tombstone recovery | Activation checks path/branch but not the target checkout's protected lifecycle state; activation recovery runs before teardown recovery. [VERIFIED: `lifecycle.rs:1081-1127,1314-1332`; `06-REVIEW.md:47-65`] | The reducer must reject `Activate/Reuse` from every protected state. Process-bearing recovery must run before activation. There must be no public setter that can force `Available`. [VERIFIED: requirement CORE-02/CORE-06] |
| CR-02 Manager loses an open shell on rollback | Manager serializes `shell_open: false` in three paths and restarts only the agent. [VERIFIED: `manager.rs:637-667,1207-1230,1473-1524,1885-1895`; `06-REVIEW.md:68-83`] | Runtime snapshots and restoration belong to one effect contract. A successful `RestoreRuntime` acknowledgement requires both requested processes live and returns both identities. [VERIFIED: App already attempts both at `app.rs:2877-2910`] |
| CR-03 App can orphan a restarted agent during failed cleanup | App ignores `kill_and_wait`, then drops the runtime map and persists identity-free booleans. [VERIFIED: `app.rs:2911-2920`; `repository.rs:104-112`; `06-REVIEW.md:85-99`] | Failed restoration must transition to durable `TeardownPending { owned_runtime, successor }` before cleanup. Forget is legal only after a confirmed stop acknowledgement or a durable successor `OwnedRuntime`. [VERIFIED: CORE-03] |

## Standard Stack

### Core

| Library/tool | Version | Purpose | Why Standard Here |
|--------------|---------|---------|-------------------|
| Rust workspace | edition 2021; rustc/cargo 1.98.0 installed | Exhaustive enums, ownership, tests | Existing project stack; no language or framework change is needed. [VERIFIED: `Cargo.toml`; environment audit] |
| `serde` / `serde_json` | workspace major 1 | Tagged lifecycle serialization and explicit migration DTOs | Already used by all durable state; supports tagged enums and strict unknown-field rejection. [VERIFIED: `Cargo.toml:10-15`; CITED: https://serde.rs/enum-representations.html] |
| Existing `persist` atomic replacement | schema v1 currently | Durable transition commits and commit-stage acknowledgement | Already distinguishes whether rename committed, which the engine needs for truthful rollback. [VERIFIED: `persist.rs:89-118,310-384`] |
| Existing Git and PTY/session modules | repository-local | Verified topology effects and exact process identity | Existing primitives already expose verified removal targets and PID/start-time/process-group/session identity. [VERIFIED: `git.rs`; `repository.rs:49-59`; `pty.rs:84-114,262-289`] |

**Installation:** none. Do not add packages or modify manifests for this refactor. [VERIFIED: all Phase 6 execution summaries report no added dependencies]

## Architecture Patterns

### System Architecture Diagram

```text
App request / Manager request / startup
                 |
                 v
      LifecycleEngine::drive(request)
                 |
                 v
    exhaustive reduce(current_state, event)
       | illegal --------------------> typed refusal; no effects
       |
       v
 typed candidate state + next effect
       |
       +--> Persist(candidate) --> commit-stage ack -------+
       +--> InspectGit ---------> facts/error -------------+
       +--> Spawn/Focus --------> exact runtime ack -------+--> reducer loop
       +--> StopOwned ----------> exact stop ack ----------+
       +--> RemoveVerified -----> Git commit/postcondition +
                 |
                 v
 stable state/outcome + adapter-only presentation
```

The engine must be the only component allowed to convert effect results into aggregate changes. [VERIFIED: this is the enforcement needed to remove current duplicated owner decisions under CORE-01]

### Recommended Project Structure

All proposed source paths below were confirmed tracked with `git ls-files`; do not create a parallel lifecycle module or new test file. [VERIFIED: git index audit, 2026-08-30]

```text
baude-core/src/
├── repository.rs   # v2 durable CheckoutLifecycle, candidates, OwnedRuntime
├── lifecycle.rs    # request/event/effect types, reducer, engine, canonical traces/tests
├── persist.rs      # schema v1 DTO -> v2 migration and strict load/save
├── session.rs      # exact two-process snapshot/stop/restore reports
└── pty.rs          # process identity and owner-death/cleanup guarantees
baude/src/app.rs    # thin LifecycleEffects implementation + mirrored vectors
bauded/src/manager.rs # thin LifecycleEffects implementation + mirrored vectors
bauded/src/api.rs   # compatibility assertions only where serialized behavior changes
```

### Pattern 1: One Tagged Durable State, Not Boolean Products

**What:** Replace `active_intent: bool` plus operation-bearing `UnavailableCause` variants with one adjacently tagged lifecycle enum. Serde's adjacent representation stores an explicit tag and content, avoiding ambiguous untagged matching. [CITED: https://serde.rs/enum-representations.html]

```rust
// Source pattern: https://serde.rs/enum-representations.html
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(tag = "state", content = "candidate", rename_all = "snake_case")]
pub enum CheckoutLifecycle {
    Inactive,
    LaunchPending(LaunchCandidate),
    Running(OwnedRuntime),
    TeardownPending(TeardownCandidate),
    ActivationPending(ActivationCandidate),
    ActivationRecovery(ActivationCandidate),
    RemovalPending(RemovalCandidate),
    RemovalCommitted(RemovalCommittedCandidate),
    RollbackPending(RollbackCandidate),
    Unavailable(TopologyUnavailable),
}
```

`health` should become a derived presentation method, not separately mutable authority. `active_intent` should likewise derive from the stable/candidate state, so contradictory combinations cannot deserialize. [VERIFIED: current contradictory product is structurally possible at `repository.rs:168-179`; Rust's exhaustive enum approach is documented at https://doc.rust-lang.org/book/ch18-03-oo-design-patterns.html]

### Pattern 2: Typed Candidate and Provenance Model

Each operation must carry only its own recovery evidence. [VERIFIED: CORE-05]

| Candidate | Required durable fields |
|-----------|-------------------------|
| `ActivationCandidate` | checkout/repository keys; literal branch; expected managed path/full ref; `created_branch`; exact preexisting owner; verification and compensation evidence; origin request. [VERIFIED: current provenance is spread across `PendingActivation`/`ActivationRecovery` at `repository.rs:83-103`] |
| `LaunchCandidate` | checkout key; `SpawnMode`; retained session; origin (`Activation`, `Reopen`, `Rollback`, `Startup`); prior runtime generation if replacing. [VERIFIED: current mode is reconstructed at `lifecycle.rs:576-580` and adapters] |
| `OwnedRuntime` | monotonically allocated runtime generation; exact agent identity; shell as `Closed` or exact identity; retained session snapshot. No boolean may claim a live process without identity. [VERIFIED: current exact identity type at `repository.rs:49-59`] |
| `TeardownCandidate` | complete `OwnedRuntime`; successor (`Inactive`, `Launch`, or `ContinueRemoval`); stop observations for agent and shell. [VERIFIED: current `TeardownPending` already needs both identities but records them only after failure at `lifecycle.rs:300-379`] |
| `RemovalCandidate` | confirmation identity; captured runtime/session; stage; second-preflight provenance; branch/path/common-dir/main/OID facts needed to interpret post-Git recovery. [VERIFIED: current facts are split between `RemovalConfirmation`, opaque target, and adapter locals at `lifecycle.rs:75-231` and owner removal methods] |
| `RollbackCandidate` | failed operation, final observed agent/shell ownership, intended stable successor, original error and compensation error. [VERIFIED: current `StoppedActiveRecovery` stores only booleans and detail at `repository.rs:104-112`] |

### Pattern 3: One Effect Contract and Driver

```rust
// Source: corrective design grounded in current lifecycle/persist/session APIs.
pub trait LifecycleEffects {
    fn persist(&mut self, next: &RepositoryState) -> Result<(), PersistAck>;
    fn inspect_git(&mut self, request: GitInspection) -> Result<GitFacts, EffectError>;
    fn mutate_git(&mut self, request: GitMutation) -> Result<GitResult, EffectError>;
    fn snapshot_runtime(&mut self, checkout: CheckoutKey) -> Result<RuntimeSnapshot, EffectError>;
    fn spawn_runtime(&mut self, candidate: &LaunchCandidate) -> Result<OwnedRuntime, EffectError>;
    fn stop_owned(&mut self, owned: &OwnedRuntime) -> Result<StopReport, EffectError>;
    fn focus_runtime(&mut self, owned: &OwnedRuntime) -> Result<(), EffectError>;
    fn forget_confirmed(&mut self, generation: RuntimeGeneration);
}
```

The engine, not the trait implementation, decides when these methods may be called. `forget_confirmed` may be emitted only after `StopReport::AllStopped` has itself been durably acknowledged, or after a durable successor `OwnedRuntime` supersedes the generation. [VERIFIED: CORE-03; current violation in CR-03]

### Pattern 4: Durable Process Ownership Before Stop or Replacement

The current code captures exact identities only after `kill_and_wait` fails. Correct ordering is snapshot exact agent and optional shell identities, construct `TeardownCandidate`, persist it, then signal those exact identities. [VERIFIED: `session.rs:333-381`; `lifecycle.rs:366-379`]

Spawn must return an `OwnedRuntime` containing both actual identities. The engine must persist `Running(owned)` before publishing/focusing the runtime; if that save fails, it keeps the live handles and drives the same typed teardown path rather than dropping the map. [VERIFIED: `Pty` captures identity before returning at `pty.rs:84-114`; Rust documents that dropping a child handle does not terminate the child at https://doc.rust-lang.org/std/process/struct.Child.html]

For abrupt owner death between process creation and `Running` persistence, `pty.rs` must enforce an owner-death guard for the PTY process group (or an equivalent tested registration handshake) so a `LaunchPending` restart cannot coexist with an unregistered child. This is not optional under CORE-06 because standard child-handle drop does not kill the process. [CITED: https://doc.rust-lang.org/std/process/struct.Child.html; VERIFIED: CORE-06]

### Pattern 5: Explicit Schema v1 -> v2 Migration

Raise `SCHEMA_VERSION` to 2. Define private v1 DTOs that exactly mirror current bytes, deserialize version 1 into those DTOs, convert each checkout through one checked mapping, validate v2, atomically save v2, and return the migrated state. Do not put `#[serde(default)]` on the new lifecycle field because that would silently erase protected recovery evidence. [VERIFIED: current loader rejects every non-current version at `persist.rs:206-234,687-725`; CITED: https://serde.rs/container-attrs.html]

Required mapping: [VERIFIED: current serialized variants at `repository.rs:63-115`]

| v1 combination | v2 state |
|----------------|----------|
| `Available`, `active_intent=false` | `Inactive` |
| `Available`, `active_intent=true` | `LaunchPending { origin: StartupMigration }` because v1 has no durable live identity after restart |
| `PendingActivation` | `ActivationPending` with all existing provenance |
| `ActivationRecovery` | `ActivationRecovery` with all existing provenance |
| `TeardownPending` | `TeardownPending`; reject any claimed-live process lacking exact identity unless its corresponding stopped flag is true |
| `RemovalTombstone` | `RemovalCommitted`/typed tombstone; never `Unavailable` and never launchable |
| `StoppedActiveRecovery` | checked `RollbackPending`; preserve booleans as legacy observations but do not treat them as ownership |
| ordinary missing/identity/I/O cause | `Unavailable(TopologyUnavailable)` |

Round-trip fixtures must cover every v1 protected state and prove idempotent second load. [VERIFIED: existing migration pattern and idempotence tests at `persist.rs:855-953`]

## Legal Transition Table

Every unlisted pair returns `IllegalTransition { from, request }` without persistence, Git, spawn, stop, focus, or forget effects. [VERIFIED: CORE-02]

| From | Event/request | Persisted next state before effect | Effect / acknowledgement | Final state |
|------|---------------|------------------------------------|--------------------------|-------------|
| `Inactive` | `Reopen` after exact Git facts | `LaunchPending(Reopen)` | spawn -> exact owned identities -> persist | `Running` |
| `Inactive` or eligible stable checkout | `Activate` | `ActivationPending` | Git classify/add/reuse -> ack | `LaunchPending(Activation)` or typed recovery |
| `Running` same checkout | `Reopen/Activate reuse` | unchanged | focus only | `Running` |
| `Running` | `Close` | `TeardownPending { successor: Inactive }` | stop exact agent+shell -> persist | `Inactive` |
| `Running` | confirmed remove | `TeardownPending { successor: ContinueRemoval }` | stop -> second inspect | `RemovalPending` |
| `Inactive` | confirmed remove | `RemovalPending` | second inspect | `RemovalPending` |
| `RemovalPending` before Git | second preflight blocked/Git refused | `RollbackPending` if runtime previously existed, otherwise prior stable state | restore spawn/focus -> persist owned | `Running` or `Inactive` |
| `RemovalPending` authorized | authority revocation | `RemovalPending { stage: AuthorityRevoked }` | plain verified Git remove | `RemovalCommitted` |
| `RemovalCommitted` | postconditions valid | child deletion candidate | persist deletion | checkout absent; parent/branch retained |
| `RemovalCommitted` | postcondition/save degraded | typed committed recovery | inspect/persist only; never recreate topology | committed recovery or absent child |
| `LaunchPending` | spawn success | candidate remains until identities known | persist `Running(OwnedRuntime)` before expose | `Running` |
| `LaunchPending` | spawn/save failure | `TeardownPending` if any process exists, else stable retry candidate | exact cleanup or retry | no unowned process |
| `TeardownPending` | startup/retry | unchanged until exact stop result | finish recorded teardown | encoded successor |
| `ActivationPending/Recovery` | startup/retry | unchanged | reconcile Git, never overwrite another protected checkout | finalized launch, cleared absent, or same recovery |
| any protected state | ordinary reconcile/reuse/reopen | unchanged | none | typed refusal |

## Startup Ordering and Rollback Semantics

### Startup order

1. Load and migrate; on malformed, unsupported, or invalid state, block persistence and launch. [VERIFIED: existing strict policy at `persist.rs:180-251`]
2. Recover every state carrying `OwnedRuntime` or exact teardown identities, including legacy migrated teardown. No activation or ordinary reopen runs first. [VERIFIED: fixes CR-01]
3. Resolve `RemovalCommitted` and authority-revoked states from fresh Git facts; never recreate removed topology. [VERIFIED: existing post-Git policy in Phase 06-06 summary]
4. Reconcile activation candidates; occupied reuse may target only `Inactive` or the same already-`Running` checkout. [VERIFIED: fixes CR-01]
5. Reconcile ordinary topology-unavailable states without changing protected candidates. [VERIFIED: CORE-02]
6. Drive launch candidates and stable desired-active migration candidates one at a time under repository/checkout reservation. [VERIFIED: existing reservation design at `lifecycle.rs:1498-1561`]
7. Only after recovery reaches stable states may App admit the launch directory or Manager serve mutation requests. [VERIFIED: current App admits after restore at `app.rs:547-555`; Manager currently completes restore before serving]

### Rollback rules

- A pre-replacement persistence failure means disk stayed at the prior state; the engine restores memory to that state but does not forget any process handle. [VERIFIED: `SaveError::replacement_committed` contract]
- A post-replacement/directory-sync error means visible bytes may already be the candidate; memory follows the candidate and the engine reports dirty durability rather than pretending rollback. [VERIFIED: `persist.rs:343-384`]
- A failed runtime restoration is not represented by booleans. Any surviving process becomes `OwnedRuntime` or `TeardownPending`; a confirmed-stopped process is absent. [VERIFIED: fixes CR-03]
- If agent restoration succeeds and shell restoration fails, persist teardown ownership for the exact restarted agent and any shell child before cleanup; cleanup failure retains the mapping and candidate. [VERIFIED: fixes CR-03]
- Manager and App must restore `shell_open` from the actual snapshot and success requires both requested live processes. [VERIFIED: fixes CR-02]
- No rollback after Git removal may recreate the worktree. [VERIFIED: locked safe-removal decision and Phase 06-06 policy]

## Mirrored Contract-Test Strategy

Use a canonical `LifecycleTrace` emitted by the shared engine: ordered `(state_before, event, persist_stage, effect, acknowledgement, state_after)` records with opaque surface IDs excluded. App and Manager tests feed equivalent facts/failure scripts through their real `LifecycleEffects` implementations and assert the same normalized trace, final durable state, Git facts, and agent/shell ownership. [VERIFIED: CORE-04]

Required vectors: [VERIFIED: CORE-01 through CORE-06 and all three review blockers]

| Group | Vectors |
|-------|---------|
| Legal/illegal table | Every legal row above; every protected state × `Activate`, `Reuse`, `Reopen`, `Close`, `Remove`; assert illegal requests emit no effect. |
| CR-01 | Occupied owner in `TeardownPending`, removal tombstone/committed, activation recovery, rollback pending, and running state; run at ordinary request and startup. |
| CR-02 | Manager/App close with open shell: success, write/sync/rename/directory-sync failure, agent restart failure, shell restart failure; assert final live PIDs and identities. |
| CR-03 | Agent restored then shell fails; agent cleanup refuses; shell cleanup refuses; recovery save fails before and after replacement; assert no mapping/identity is forgotten. |
| Persistence matrix | Every candidate save at Write, Sync, Rename, DirectorySync; assert disk bytes, memory, trace, and effects. |
| Startup matrix | Crash/restart after every persisted state and after every effect acknowledgement; run recovery twice to prove idempotence and no duplicate spawn. |
| Migration | v1 fixture for every `UnavailableCause` and active/available pair; v2 round trip; second load unchanged; malformed ownership rejected. |
| Process contract | Exact PID reuse refusal, agent-only/shell-only/both teardown, owner-death during launch registration, and no child-handle drop orphan. |
| Git contract | Preserve existing Phase 6 real-Git activation/removal matrices unchanged. |

Test code stays in the existing tracked files: canonical reducer/trace vectors in `baude-core/src/lifecycle.rs`, migration vectors in `persist.rs`, process vectors in `session.rs`/`pty.rs`, and mirrored adapter vectors in `app.rs` and `manager.rs`. [VERIFIED: all paths confirmed tracked]

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Durable file replacement | A second journal format for ordinary state commits | Existing atomic temp-write/sync/rename/directory-sync API | It already reports the decisive replacement boundary. [VERIFIED: `persist.rs:310-384`] |
| Git ownership/safety | Path-based deletion or cached confirmation authorization | Existing discovery, opaque verified removal target, and plain Git removal | Existing real-Git matrices already cover the hard safety cases. [VERIFIED: Phase 06-03/06-06 summaries] |
| Process identity | PID-only ownership or booleans like `agent_restarted` | Existing `ProcessIdentity` plus exact reinspection | PID reuse is already addressed by start time, process group, and session. [VERIFIED: `repository.rs:49-59`; `session.rs`] |
| Separate App/Manager transaction helpers | Two "equivalent" orchestration copies | One generic core engine with effect implementations | The current copies produced CR-02 and CR-03. [VERIFIED: `06-REVIEW.md`] |
| Generic recovery overlay | `health + active_intent + detail` reconstruction | Typed operation candidates | Generic combinations permit protected state erasure. [VERIFIED: CR-01; CORE-05] |

## Runtime State Inventory

This phase is a schema refactor/migration, so repository files alone are not the whole state surface. [VERIFIED: phase scope]

| Category | Items Found | Action Required |
|----------|-------------|-----------------|
| Stored data | Workspace state files `state-<workspace>.json` and `daemon-state-<workspace>.json` contain schema-v1 `RepositoryState`; default workspaces can also have legacy unsuffixed sources. [VERIFIED: `persist.rs:180-251,650-684`; `manager.rs:31-35`] | Add explicit v1->v2 data migration; preserve legacy flat migration; test both owners and idempotence. |
| Live service config | No lifecycle candidate is stored in daemon UI/API configuration; daemon runtime state is in its named state file and in-process Manager. [VERIFIED: `manager.rs:76-103,285-306`] | Stop daemon before real migration verification; startup migration/recovery owns conversion. |
| OS-registered state | Live PTY agent and shell process groups can outlive a dropped child handle; no OS service registration is used for individual sessions. [CITED: https://doc.rust-lang.org/std/process/struct.Child.html; VERIFIED: `pty.rs`] | Add owner-death/registration guarantee and abrupt-crash tests; startup reconciles exact recorded identities. |
| Secrets/env vars | `BAUDE_RESUME_ID` carries opaque targeted resume data but is not durable lifecycle authority. [VERIFIED: Phase 06-05 summary] | No key rename; retain opaque transport. |
| Build artifacts | Rust `target/` may contain stale test binaries after the type/schema refactor. [VERIFIED: Cargo workspace behavior observed during test run] | Normal `cargo test` rebuild is sufficient; no package reinstall. |

## Common Pitfalls

### Pitfall 1: Patching the Three Findings in Place
**What goes wrong:** another effect boundary remains duplicated and diverges later. [VERIFIED: CR-02 arose after App-only parity repair]
**How to avoid:** delete owner-side transition selection and require every lifecycle entrypoint to call the same engine.

### Pitfall 2: Keeping `active_intent` as Parallel Authority
**What goes wrong:** startup scans a boolean and launches a protected checkout. [VERIFIED: `app.rs:42-49,541-545`; `manager.rs:363-379`]
**How to avoid:** derive desired activity from `CheckoutLifecycle`; no startup boolean scan.

### Pitfall 3: Treating an Effect List as Enforcement
**What goes wrong:** `ClosePlan.effects` says save-before-stop while both owners stop first. [VERIFIED: `lifecycle.rs:59-73`; owner close methods]
**How to avoid:** core must execute/drive the effect sequence, not merely document it.

### Pitfall 4: Forgetting Serialization Compatibility
**What goes wrong:** raising/changing types makes every existing schema-v1 file unsupported or malformed. [VERIFIED: current exact-version loader]
**How to avoid:** explicit versioned DTO conversion and atomic migrated save; never default the new state.

### Pitfall 5: Losing Shell State Through Save Overlays
**What goes wrong:** even a correct close snapshot is overwritten by `state_for_save` setting `shell_open=false`. [VERIFIED: `manager.rs:637-667`; `06-REVIEW-FIX.md:35-39`]
**How to avoid:** the engine persists its exact candidate state; generic save code must not overlay runtime fields during a lifecycle transaction.

### Pitfall 6: Dropping a Child Handle as Cleanup
**What goes wrong:** the process can continue after its handle is dropped. [CITED: https://doc.rust-lang.org/std/process/struct.Child.html]
**How to avoid:** exact stop/wait acknowledgement or a tested owner-death guard; never remove the map first.

## Code Examples

### Exhaustive refusal boundary

```rust
// Source: corrective pattern; exhaustive enum-state rationale:
// https://doc.rust-lang.org/book/ch18-03-oo-design-patterns.html
fn reduce(state: CheckoutLifecycle, event: LifecycleEvent)
    -> Result<Transition, IllegalTransition>
{
    match (state, event) {
        (CheckoutLifecycle::Inactive, LifecycleEvent::Reopen(req)) => {
            Ok(Transition::persist_then(
                CheckoutLifecycle::LaunchPending(req.into_candidate()),
                LifecycleEffect::Spawn,
            ))
        }
        (protected, request) => Err(IllegalTransition::new(protected.kind(), request.kind())),
    }
}
```

### Exact two-process teardown candidate

```rust
// Source: current ProcessIdentity model at baude-core/src/repository.rs:49-59
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct OwnedRuntime {
    pub generation: u64,
    pub agent: ProcessIdentity,
    pub shell: ShellOwnership,
    pub retained: RetainedSessionState,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum ShellOwnership {
    Closed,
    Owned(ProcessIdentity),
}
```

## Migration/Serialization Implications

- Incrementing the schema is required because the durable JSON shape and invariants change materially. [VERIFIED: current schema directly embeds old shape]
- Preserve `deny_unknown_fields` on state/candidate structs and tagged enums; malformed state must remain blocking. [VERIFIED: current policy; CITED: https://serde.rs/container-attrs.html]
- Migration must not infer baude management, process liveness, or candidate provenance beyond evidence present in v1. [VERIFIED: existing migration safety decision in STATE.md]
- A v1 active/available checkout is desired-active but has no durable process ownership; migrate it to a startup launch candidate, never `Running`. [VERIFIED: v1 model has no running-process field]
- A v1 teardown record whose process is marked unstopped but lacks identity must remain blocked for human repair, not be treated as stopped. [VERIFIED: CORE-03]
- Keep old execution plans/summaries unchanged; 06-07 is additive corrective history. [VERIFIED: STATE.md and ROADMAP.md]

## State of the Art

| Old approach | Corrective approach | Impact |
|--------------|---------------------|--------|
| Core plans plus adapter orchestration | Core reducer plus effect-driving engine | Effect order becomes enforceable. [VERIFIED: CORE-01] |
| `active_intent` + `health` + runtime map | One tagged durable lifecycle with exact owned runtime | Protected combinations cannot be overwritten by ordinary reuse. [VERIFIED: CORE-02/03] |
| Failure-specific booleans | Typed candidates with operation provenance | Startup can resume the exact interrupted operation. [VERIFIED: CORE-05/06] |
| App/Manager tests with similar names | Same canonical trace matrix through both effect implementations | Parity is behavioral, not aspirational. [VERIFIED: CORE-04] |

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|

All implementation claims are grounded in current repository code, planning requirements/review artifacts, or cited official Rust/Serde documentation; no `[ASSUMED]` package or platform claim is used.

## Open Questions

1. **Which owner-death mechanism will satisfy the abrupt launch crash case on both supported Unix targets?**
   - What we know: dropping a child handle does not terminate it, and CORE-06 forbids orphaned runtimes. [CITED: https://doc.rust-lang.org/std/process/struct.Child.html; VERIFIED: CORE-06]
   - What's unclear: current `portable-pty` master-close behavior has not been proven as a cross-platform kill guarantee in this research session. [VERIFIED: no such test exists in `pty.rs`]
   - Recommendation: 06-07 must start with an abrupt-owner-death characterization test and implement a guard/registration handshake in existing `pty.rs` if master-close alone is insufficient; do not declare CORE-06 complete on injected `Result` failures alone. [VERIFIED: requirement implication]

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|-------------|-----------|---------|----------|
| Rust compiler | build/tests | ✓ | 1.98.0 | — |
| Cargo | workspace tests/lint | ✓ | 1.98.0 | — |
| Git | real worktree contract tests | ✓ | 2.50.1 Apple Git-155 | — |
| Context7 CLI | library documentation lookup | ✗ | — | Official Rust/Serde docs fetched directly. |
| Linux Rust target | Linux process-identity verification | not verified in this session | — | CI/Linux runner required for phase gate. |

**Missing dependencies with no fallback:** Linux runtime verification is required before claiming cross-platform process recovery complete. [VERIFIED: prior review inspected Linux only and reported no Linux target]

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in test harness via Cargo 1.98.0 [VERIFIED: environment and current tests] |
| Config file | Workspace `Cargo.toml`; inline unit/contract tests in tracked source files [VERIFIED: codebase inspection] |
| Quick run command | `cargo test lifecycle_protocol_contract -- --nocapture` |
| Full suite command | `cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test` |

### Phase Requirements -> Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| CORE-01 | Engine is sole transition/effect-order authority | core + mirrored contract | `cargo test lifecycle_protocol_contract -- --nocapture` | ❌ Wave 0 additions inside tracked `lifecycle.rs`, `app.rs`, `manager.rs` |
| CORE-02 | exhaustive legal/illegal transition table | table-driven unit | `cargo test -p baude-core lifecycle::tests::legal_transition_table -- --nocapture` | ❌ Wave 0 |
| CORE-03 | exact agent/shell write-ahead ownership and forget gate | process integration | `cargo test -p baude-core lifecycle::tests::process_ownership -- --nocapture` | ❌ Wave 0 |
| CORE-04 | identical App/Manager traces at every boundary | mirrored integration | `cargo test lifecycle_protocol_contract -- --nocapture` | ❌ Wave 0 |
| CORE-05 | typed candidate round trip and v1 migration | serialization | `cargo test -p baude-core persist::tests::lifecycle_schema_v2 -- --nocapture` | ❌ Wave 0 |
| CORE-06 | idempotent startup/rollback and abrupt owner death | crash/recovery integration | `cargo test lifecycle_startup_recovery -- --nocapture` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test lifecycle_protocol_contract -- --nocapture`
- **Per wave merge:** `cargo test -p baude-core lifecycle::tests -- --nocapture && cargo test lifecycle_ -- --nocapture`
- **Phase gate:** full fmt, Clippy, and workspace tests green; then rerun deep code review and phase verification with zero lifecycle blockers.

### Wave 0 Gaps
- [ ] Canonical legal-transition and trace fixtures in `baude-core/src/lifecycle.rs`.
- [ ] Schema-v1 protected-state fixtures in `baude-core/src/persist.rs`.
- [ ] Agent/shell and abrupt-owner-death fixtures in `baude-core/src/session.rs` and `baude-core/src/pty.rs`.
- [ ] Mirrored App/Manager scripted effect implementations and identical-vector tests in existing adapter files.

**Baseline verified during research:** `cargo test -p baude-core lifecycle::tests -- --nocapture` passed 12 tests; `cargo test lifecycle_close -- --nocapture` passed four owner tests; `cargo test lifecycle_remove_clean -- --nocapture` passed three owner tests. Passing baseline tests do not close the three review blockers. [VERIFIED: commands run 2026-08-30; `06-REVIEW.md`]

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | No authentication surface changes in this phase. [VERIFIED: scope] |
| V3 Session Management | yes (process/session ownership, not web auth sessions) | Durable exact process identity and opaque backend resume IDs. [VERIFIED: `ProcessIdentity`; Phase 06-05] |
| V4 Access Control | yes | Tagged lifecycle state plus opaque Git verification capabilities authorize destructive effects. [VERIFIED: CORE-02 and existing `VerifiedRemovalTarget`] |
| V5 Input Validation | yes | Existing literal Git refs, strict serde state, exact process/Git facts, exhaustive transition refusal. [VERIFIED: Phase 06-01/03 summaries] |
| V6 Cryptography | no | No cryptographic operation is introduced. [VERIFIED: scope] |

### Known Threat Patterns for This Stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Protected recovery overwritten by ordinary reuse | Tampering / Elevation | One tagged state and exhaustive illegal transition refusal. [VERIFIED: CR-01] |
| PID reuse signals an unrelated process | Spoofing / Tampering | PID + start time + process group + session reinspection immediately before signal. [VERIFIED: current `ProcessIdentity`] |
| Agent or shell orphaned after failed rollback | Repudiation / DoS | Durable `OwnedRuntime`/`TeardownPending`, exact stop acknowledgement, forget gate. [VERIFIED: CR-02/CR-03] |
| State bytes and memory disagree after rename | Tampering | Interpret `replacement_committed`; never generic rollback after committed replacement. [VERIFIED: `persist.rs`] |
| Crash after spawn but before registration | DoS / orphaned authority | Tested owner-death guard or registration handshake. [CITED: Rust `Child` docs; VERIFIED: CORE-06] |

## Sources

### Primary (HIGH confidence)
- Current tracked source: `baude-core/src/lifecycle.rs`, `repository.rs`, `persist.rs`, `session.rs`, `pty.rs`, `baude/src/app.rs`, `bauded/src/manager.rs`, `bauded/src/api.rs` — control flow, durable schema, effects, tests, and process identity.
- `.planning/REQUIREMENTS.md`, `STATE.md`, `ROADMAP.md`, `06-CONTEXT.md`, `06-REVIEW.md`, `06-REVIEW-FIX.md`, and 06-01 through 06-06 plans/summaries — locked behavior, history, and blockers.
- https://serde.rs/enum-representations.html — explicit enum representations.
- https://serde.rs/container-attrs.html — tags, defaults, and strict unknown-field handling.
- https://doc.rust-lang.org/book/ch18-03-oo-design-patterns.html — Rust state modeling and type-enforced transitions.
- https://doc.rust-lang.org/std/process/struct.Child.html — child handles are not killed or waited on by `Drop`.

### Secondary (MEDIUM confidence)
- None.

### Tertiary (LOW confidence)
- None.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — unchanged and verified from manifests/environment.
- Architecture: HIGH — derived directly from requirements, duplicated production control flow, and concrete review failures.
- Migration: HIGH — current serialized schema and exact-version loader inspected.
- Owner-death implementation detail: MEDIUM — necessity is verified, but the current PTY master-close behavior still needs the prescribed characterization test.
- Pitfalls: HIGH — each is exhibited by current source or the deep review.

**Research date:** 2026-08-30
**Valid until:** 2026-09-29, or until lifecycle source/schema changes.
