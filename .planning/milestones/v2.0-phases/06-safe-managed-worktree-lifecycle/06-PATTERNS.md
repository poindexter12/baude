# Phase 06: Shared Lifecycle Core Refactor - Pattern Map

**Mapped:** 2026-08-30
**Files analyzed:** 8 tracked source files
**Analogs found:** 7 / 8 (the effect-driving engine is new and has only composite seams)
**Corrective target:** 06-07 architecture from `06-RESEARCH.md`

All proposed source paths below were verified with `git ls-files --error-unmatch` on 2026-08-30. Tests remain inline in these tracked files; do not add a parallel lifecycle module or test tree.

## File Classification

| New/Modified File | Role | Data Flow | Closest Current Analog | Match Quality |
|---|---|---|---|---|
| `baude-core/src/repository.rs` | model / store | transform | same file: strict durable aggregate and exact `ProcessIdentity` | exact extension seam |
| `baude-core/src/lifecycle.rs` | service / reducer / protocol | event-driven + request-response + transform | same file's typed plans, refusal checks, recovery, and reservations | composite role-match; no effect driver yet |
| `baude-core/src/persist.rs` | service / migration | file-I/O + transform | same file's legacy migration, strict loader, and commit-stage save | exact extension seam |
| `baude-core/src/session.rs` | service | process-I/O + request-response | same file's two-process teardown and exact recorded recovery | exact extension seam |
| `baude-core/src/pty.rs` | service / process owner | streaming + process-I/O | same file's identity-at-spawn and confirmed stop/wait | exact extension seam; no owner-death guard yet |
| `baude/src/app.rs` | controller / adapter / live-handle store | event-driven + process-I/O + file-I/O | same file's persistence and runtime-handle effects | role-match; orchestration must be removed |
| `bauded/src/manager.rs` | service / adapter / live-handle store | request-response + process-I/O + file-I/O | same file's typed persistence boundary and runtime effects | role-match; orchestration must be removed |
| `bauded/src/api.rs` | route / compatibility adapter | request-response | same file's `MutationError` to HTTP mapping | exact compatibility seam; modify only if serialized outcomes change |

## Pattern Assignments

### `baude-core/src/repository.rs` (model/store, transform)

**Analog:** strict durable types in the same file.

**Imports and durable-type style** (`repository.rs:3-12`):

```rust
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RepositoryKey(u64);
```

**Exact process identity pattern** (`repository.rs:49-59`):

```rust
/// Durable authority for one PTY-owned process group. A numeric PID alone is
/// never sufficient because it can be reused after the original child exits.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub start_time: u64,
    pub process_group: i32,
    pub session: i32,
}
```

Copy this derive/strict-serde style for `RuntimeGeneration`, `OwnedRuntime`, `ShellOwnership`, and operation-specific candidate structs. `OwnedRuntime` must always contain an exact agent identity and either `ShellOwnership::Closed` or an exact shell identity.

**Schema seam to replace** (`repository.rs:166-179`):

```rust
pub struct SavedCheckout {
    // ...
    pub active_intent: bool,
    pub session: RetainedSessionState,
    pub health: CheckoutHealth,
}
```

Do not preserve this independently mutable boolean product. Replace `active_intent + CheckoutHealth` with one adjacently tagged `CheckoutLifecycle`; derive presentation health and desired activity from it. Keep `#[serde(deny_unknown_fields)]` on structs and use an explicit enum tag/content representation. Extend `RepositoryState::validate` (`repository.rs:302-421`) so impossible ownership and candidate combinations fail closed.

---

### `baude-core/src/lifecycle.rs` (shared reducer/protocol, event-driven)

**Composite analog:** typed requests/effects in this file, strict refusal in `plan_reopen`, and RAII repository reservations. There is no current analog for an engine that actually drives effects; implement that from the corrective research contract rather than copying App or Manager orchestration.

**UI-free imports and typed contract style** (`lifecycle.rs:1-17`):

```rust
//! Shared, UI-free repository lifecycle contracts.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::backend::SpawnMode;
use crate::repository::{
    AllocationError, CheckoutHealth, CheckoutKey, CheckoutRole, PersistedPath, RepositoryHealth,
    RepositoryKey, RepositoryState, RetainedSessionState, SavedCheckout, SavedRepository,
    UnavailableCause, ValidationError,
};
```

Keep request/event/effect/acknowledgement/outcome types in core and free of App IDs, UI messages, Axum types, or live `Session` handles.

**Explicit ordered-effect pattern** (`lifecycle.rs:59-73`):

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseEffect {
    SnapshotRuntime,
    SaveInactiveIntent,
    StopRuntime,
}

pub struct ClosePlan {
    pub checkout: CheckoutKey,
    pub effects: [CloseEffect; 3],
    pub outcome: LifecycleOutcome,
}
```

Promote this from advisory plans to enforcement: `LifecycleEngine<E: LifecycleEffects>` repeatedly applies one exhaustive reducer transition, invokes only the emitted effect, records its acknowledgement, and re-enters the reducer. App/Manager must not manually reorder the list as they do today.

**Protected-state refusal pattern** (`lifecycle.rs:550-562`):

```rust
if let CheckoutHealth::Unavailable(
    cause @ (UnavailableCause::RemovalTombstone(_)
    | UnavailableCause::TeardownPending { .. }
    | UnavailableCause::PendingActivation { .. }
    | UnavailableCause::ActivationRecovery { .. }
    | UnavailableCause::StoppedActiveRecovery { .. }),
) = &state.checkouts[checkout_index].health
{
    return Err(ReopenBlocked {
        checkout: request.checkout,
        cause: cause.clone(),
    });
}
```

Generalize this into the exhaustive legal transition table. Every unlisted `(CheckoutLifecycle, LifecycleEvent)` returns typed `IllegalTransition` with **no persist, Git, spawn, stop, focus, or forget effect**. This closes CR-01 only if occupied `Activate/Reuse` goes through the same reducer; do not retain a public setter that can force `Available`.

**Current CR-01 overwrite seam—not a pattern to copy** (`lifecycle.rs:1081-1127`, also `1314-1332`): current occupied reuse checks path/ref, then assigns `active_intent = true` and `health = Available` on an existing checkout. Replace this branch with reducer admission that permits only `Inactive` or same-checkout `Running`; reject teardown, removal, activation-recovery, rollback, and other protected states unchanged.

**Recovery acknowledgement pattern** (`lifecycle.rs:426-459`):

```rust
match crate::session::finish_recorded_teardown(/* exact recorded identities */) {
    Ok(()) => {
        checkout.active_intent = false;
        checkout.health = CheckoutHealth::Available;
        // validate and report Completed
    }
    Err(error) => {
        // retain updated exact identities and per-process stop observations
        checkout.health = CheckoutHealth::Unavailable(UnavailableCause::TeardownPending { /* ... */ });
        // validate and report Pending
    }
}
```

Retain the consume-acknowledgement-and-persist-new-evidence shape, but route both branches through the new tagged state reducer. Recovery must not mutate aggregate fields directly outside it.

**RAII serialization pattern** (`lifecycle.rs:1498-1561`):

```rust
pub fn reserve_reopen(
    &self,
    repository: RepositoryKey,
    checkout: CheckoutKey,
) -> Result<RepositoryReservation, LifecycleOutcome> {
    // same-checkout reopen => ReopenPending; other held mutation => Busy
    // otherwise insert reservation
}

impl Drop for RepositoryReservation {
    fn drop(&mut self) {
        self.held.lock().unwrap_or_else(|error| error.into_inner())
            .remove(&self.repository);
    }
}
```

Reuse repository serialization and checkout-specific reopen coalescing. The engine owns reservation duration across fresh inspection, candidate persistence, effect, acknowledgement, and final persistence.

**Canonical test-vector style** (`lifecycle.rs:2129-2161`):

```rust
let vectors = [
    (ReopenRuntime::Live { id: 7 }, ReopenDispatch::Focus { id: 7 }),
    (ReopenRuntime::Exited { id: 8 }, ReopenDispatch::Restart { id: 8 }),
    (ReopenRuntime::Absent, ReopenDispatch::Spawn),
];
for (runtime, expected) in vectors {
    // construct state, drive request, assert state and ordered effect
}
```

Extend this into `LifecycleTrace` records of `(state_before, event, persist_stage, effect, acknowledgement, state_after)`. Normalize away surface runtime IDs. Core owns the vector definitions; App and Manager run the same vectors through their real effect adapters.

---

### `baude-core/src/persist.rs` (schema migration, file-I/O + transform)

**Analog:** existing strict load/migrate/save pipeline and atomic commit-stage reporting.

**Strict loader and migration boundary** (`persist.rs:206-251`):

```rust
let value: serde_json::Value = serde_json::from_slice(&bytes)
    .map_err(|error| LoadError::Malformed { /* ... */ })?;
if let Some(version) = value.get("schema_version") {
    let version = version.as_u64().ok_or_else(/* malformed */)?;
    if version != u64::from(SCHEMA_VERSION) {
        return Err(LoadError::UnsupportedVersion { /* ... */ });
    }
    let current: StateFile = serde_json::from_value(value)
        .map_err(|error| LoadError::Malformed { /* ... */ })?;
    current.state.validate().map_err(/* invalid */)?;
    return Ok(LoadOutcome::Current(current));
}
// checked conversion, validate, atomically save migrated bytes
```

Raise `SCHEMA_VERSION` to 2 and add private DTOs that exactly mirror schema v1. Dispatch version 1 to a checked `v1 -> v2` conversion, validate v2, atomically save it, then return migrated state. Preserve the separate pre-schema legacy migration. Do not add `#[serde(default)]` to the new lifecycle field.

**Migration idempotence test pattern** (`persist.rs:855-915`):

```rust
let outcome = migrate_for_workspace_at(&root, base, &workspace, reconcile_legacy).unwrap();
let LoadOutcome::Legacy(migrated) = outcome else { panic!("expected migrated legacy state") };
let first_bytes = std::fs::read(&primary_path).unwrap();
let second = migrate_for_workspace_at(&root, base, &workspace, reconcile_legacy).unwrap();
assert_eq!(second, LoadOutcome::Current(migrated));
assert_eq!(std::fs::read(&primary_path).unwrap(), first_bytes);
```

Copy this fixture/idempotence structure for every schema-v1 `UnavailableCause`, both available/active combinations, malformed ownership, both App and daemon state base names, and legacy unsuffixed fallback.

**Commit acknowledgement pattern** (`persist.rs:343-384`):

```rust
let mut replacement_committed = false;
// write + flush + file sync
std::fs::rename(&temporary, &destination)?;
replacement_committed = true;
// directory sync
attempt.map_err(|source| SaveError {
    replacement_committed,
    source,
})
```

Expose this boundary through the effect acknowledgement. Pre-replacement failure may restore prior memory state without forgetting handles. Post-replacement failure means memory follows candidate bytes and reports degraded durability.

---

### `baude-core/src/session.rs` (exact two-process service)

**Analog:** current agent-and-shell teardown plus PID-reuse-safe recovery.

**Attempt and report both processes** (`session.rs:330-379`):

```rust
pub fn kill_and_wait(&mut self) -> Result<(), SessionTeardownError> {
    let claude = self.claude.kill_and_wait().err();
    let shell = self.shell.as_mut().and_then(|shell| shell.kill_and_wait().err());
    match (claude, shell) {
        (None, None) => Ok(()),
        (claude, shell) => Err(SessionTeardownError {
            agent_pid: self.claude.pid(),
            shell_pid: self.shell.as_ref().and_then(Pty::pid),
            agent_identity: Some(self.claude.process_identity().clone()),
            shell_identity: self.shell.as_ref().map(|shell| shell.process_identity().clone()),
            agent_stopped: claude.is_none(),
            shell_stopped: shell.is_none(),
            // ...
        }),
    }
}
```

Factor a runtime snapshot/stop/restore report around this exact two-process shape. A successful `RestoreRuntime` acknowledgement requires the requested agent and shell both live and returns their exact identities; it is never a boolean-only success.

**Exact recorded-owner gate** (`session.rs:546-615`):

```rust
fn identity_still_matches(expected: &ProcessIdentity, inspect: &mut impl FnMut(/* ... */))
    -> Result<bool, String>
{
    Ok(inspect(expected.pid)?.as_ref() == Some(expected))
}

if !identity_still_matches(&identity, &mut inspect)? { return Ok(()); }
// signal exact process group, reinspect, escalate, and confirm absence
```

Reuse this immediately-before-signal identity verification. Never signal by PID alone and never treat a replaced PID as the recorded child.

---

### `baude-core/src/pty.rs` (process owner, streaming/process-I/O)

**Analog:** identity establishment before returning a live PTY and confirmed stop/wait.

**Identity-at-spawn pattern** (`pty.rs:79-114`):

```rust
let mut child = pair.slave.spawn_command(cmd).context("failed to spawn command in pty")?;
let identity = child.process_id()
    .ok_or_else(|| anyhow::anyhow!("PTY child did not expose a process id"))
    .and_then(|pid| crate::session::inspect_process_identity(pid) /* ... */);
let identity = match identity {
    Ok(identity) if identity.process_group == identity.pid as i32
        && identity.session == identity.pid as i32 => identity,
    Ok(identity) => { let _ = child.kill(); let _ = child.wait(); /* fail */ }
    Err(error) => { let _ = child.kill(); let _ = child.wait(); return Err(error); }
};
```

Spawn/restore effects should return `OwnedRuntime` assembled from identities captured here, not from later PID lookups.

**Confirmed cleanup pattern** (`pty.rs:286-328`):

```rust
pub fn kill_and_wait(&mut self) -> Result<()> {
    if child.try_wait()?.is_some() { return Ok(()); }
    if let Err(kill_error) = child.kill() {
        if child.try_wait()?.is_some() { return Ok(()); }
        return Err(kill_error).context("failed to kill PTY child");
    }
    child.wait().context("failed to wait for PTY child")?;
    Ok(())
}
```

Do not use `kill()` or handle drop as lifecycle acknowledgement. Add the abrupt owner-death characterization test here first. No existing code proves that dropping the PTY/master kills the process group; if it does not, add a tested guard or registration handshake in this tracked file.

---

### `baude/src/app.rs` (thin local `LifecycleEffects` adapter)

**Analog:** App's concrete persistence, PTY/session access, and surface focus behavior. Do not copy its transaction sequencing.

**Persistence effect seam** (`app.rs:571-621`): App already reports `SaveError` with the commit stage and writes through `StateFile::new`. Keep that storage configuration and injected atomic-failure seam, but persist the exact engine candidate. Remove lifecycle-time overlay reconstruction from `runtime_checkouts`; the engine's candidate is authoritative.

**Actual shell snapshot pattern** (`app.rs:588-602`):

```rust
checkout.session = RetainedSessionState {
    name: session.name.clone(),
    // ...
    shell_open: session.shell_open,
    archived: session.archived,
    archived_by_user: session.archived_by_user,
    resume_id: session.meta.session_id.clone()
        .or_else(|| checkout.session.resume_id.clone()),
};
```

Use the same actual shell observation in `snapshot_runtime`, but return a typed snapshot/`OwnedRuntime` acknowledgement to core.

**Two-process restoration pattern** (`app.rs:2877-2910`): App restarts the agent, conditionally opens the shell, verifies both are non-exited, and succeeds only when both requested processes are live. Move that semantic contract behind `LifecycleEffects::spawn_runtime/restore_runtime` and return exact identities.

**CR-03 seam—not to copy** (`app.rs:2911-2920`):

```rust
if let Some(session) = self.session_mut(id) {
    let _ = session.kill_and_wait();
}
Err(RuntimeRestartFailure { agent_restarted, shell_restarted, /* ... */ })
```

Never ignore this result, remove the runtime map, or persist identity-free booleans. Before cleanup, the engine must persist `TeardownPending { owned_runtime, successor }`; retain the handle mapping until confirmed stop or a durable successor `OwnedRuntime` supersedes it.

**Current startup order—not to copy** (`app.rs:535-545`): activation recovery runs before teardown recovery, then `active_restore_checkouts` launches from boolean intent. Replace the body with one engine-provided startup recovery program.

**Existing adapter test seam** (`app.rs:3879-3960`): preserve real persistence injection and live PID assertions. Convert these to shared vectors and add open-shell restore failure, cleanup refusal, every commit stage, and protected occupied-reuse cases. Assert normalized engine trace plus final durable state and exact agent/shell ownership.

---

### `bauded/src/manager.rs` (thin daemon `LifecycleEffects` adapter)

**Analog:** typed persistence/error boundary and concrete session container. Do not preserve independently selected transitions.

**Commit-stage storage effect** (`manager.rs:568-598`):

```rust
let state = StateFile::new(self.state_for_save());
let saved = persist::save_for_workspace_status(/* ... */, &state);
match saved {
    Ok(()) => { self.persistence_dirty = false; self.persistence_error = None; Ok(()) }
    Err(error) => { self.persistence_dirty = true; self.persistence_error = Some(error.to_string()); Err(error) }
}
```

Retain status reporting and test injection, but accept the exact candidate supplied by the engine. `state_for_save` must not overwrite lifecycle candidate fields.

**CR-02 seams—not to copy** (`manager.rs:637-667`, `1473-1552`, `1885-1895`): Manager currently serializes `shell_open: false`, snapshots close with `shell_open: false`, and `restart_with_mode` replaces only `s.claude`. Implement the same typed two-process effect contract as App: snapshot actual `session.shell_open`; restore agent and requested shell; verify both; return exact identities. There must be no Manager-only interpretation of partial restore.

**Current startup order—not to copy** (`manager.rs:357-379`): activation recovery precedes teardown and launch scans `active_intent`. Replace with the same engine startup program App executes.

**Mirrored test seam** (`manager.rs:2927-3059`): preserve isolated state roots, `AtomicFailure`, live PID checks, shell failure injection, and exact retained context. Run the same canonical vectors as App and assert byte-equivalent normalized traces. Add the missing Rename failure with an open shell and assert new agent and shell identities are live.

---

### `bauded/src/api.rs` (route compatibility adapter)

**Analog:** existing typed Manager error mapping.

**Error/status pattern** (`api.rs:104-115`):

```rust
type ApiError = (StatusCode, String);

fn mutation_error(error: MutationError, fallback: StatusCode) -> ApiError {
    match error {
        MutationError::Persistence(error) => (StatusCode::SERVICE_UNAVAILABLE, error.to_string()),
        MutationError::Domain(error) => (fallback, error.to_string()),
    }
}
```

Keep API behavior as a presentation mapping over Manager outcomes. Existing create/delete/restart handlers (`api.rs:154-173,251-269`) should continue delegating. Modify this file only for compatibility assertions or typed status mapping necessitated by changed serialized behavior; do not add Phase 8/9 lifecycle endpoints.

## Shared Core Ownership and Data Flow

```text
App request / Manager request / startup
  -> LifecycleEngine::drive(request)
  -> reduce(current CheckoutLifecycle, event)
       illegal -> typed refusal and zero effects
       legal   -> typed durable candidate + exactly one next effect
  -> adapter performs effect only
       Persist(candidate) / InspectGit / MutateGit
       SnapshotRuntime / SpawnOrRestore / StopOwned / Focus / ForgetConfirmed
  -> typed acknowledgement (including commit stage and exact process identities)
  -> reducer consumes acknowledgement
  -> trace record + next transition/effect, until stable outcome
  -> adapter-only focus/message/HTTP presentation
```

Core owns topology decisions, legal transitions, persistence stages, rollback successor selection, exact durable runtime ownership, forget authorization, and startup ordering. App/Manager own storage location, live `Session` lookup, PTY creation, focus/notification, and surface error presentation only.

## Shared Patterns

### Tagged Lifecycle and Typed Provenance

Apply to `repository.rs`, `lifecycle.rs`, and `persist.rs`. Use one tagged `CheckoutLifecycle` and operation-specific activation, launch, teardown, removal, committed-removal, and rollback candidates. Never reconstruct an interrupted operation from generic health, booleans, or `runtime_checkouts`.

### Exact Agent/Shell Ownership

Apply to `repository.rs`, `lifecycle.rs`, `session.rs`, `pty.rs`, `app.rs`, and `manager.rs`. Persist exact ownership before destructive/replacement effects. Forget a generation only after a confirmed all-stopped acknowledgement or after a durable successor `OwnedRuntime` supersedes it.

### Persistence Error Handling

Source: `persist.rs:89-118,310-384`. Apply to every candidate save. Before replacement: disk remains prior state; memory may return to prior state without dropping handles. After replacement: memory follows candidate; report degraded durability. Never pretend JSON and Git are one transaction, and never recreate topology after Git removal.

### Startup Recovery Order

One core-generated order for both owners:

1. strict load and v1-to-v2 migration;
2. process-bearing `OwnedRuntime`/teardown recovery;
3. committed or authority-revoked removal recovery;
4. activation candidate recovery with protected-state refusal;
5. ordinary topology-unavailable reconciliation;
6. launch candidates under reservation;
7. only then admit the App launch directory or serve mutation requests.

Run recovery twice in tests to prove idempotence and no duplicate/orphan runtime.

### Mirrored Trace Gate

Core defines vectors and canonical normalization. App and Manager each execute them through their real effect implementation. Compare ordered trace, final v2 bytes/state, Git facts, runtime generation, and exact agent/shell ownership. Required matrices include every legal row, protected state crossed with every request, all persistence stages, crash after every state/effect acknowledgement, migration fixtures, PID reuse, shell-only/agent-only teardown, owner death, and existing real-Git activation/removal tests.

## Three Review Blockers: Required Pattern Closure

| Blocker | Unsafe Current Source | Required Pattern |
|---|---|---|
| CR-01 protected state erased by occupied reuse | `lifecycle.rs:1081-1127,1314-1332`; startup `app.rs:535-545`, `manager.rs:357-379` | reducer rejects reuse from every protected state; process recovery precedes activation; no public force-available setter |
| CR-02 Manager loses open shell | `manager.rs:637-667,1473-1552,1885-1895` | actual two-process snapshot; restore success returns exact identities for both requested processes; same effect semantics as App |
| CR-03 App forgets failed cleanup ownership | `app.rs:2877-2920,1768-1819` | persist exact `TeardownPending` before cleanup; consume stop acknowledgement; retain mapping on failure; no identity-free rollback booleans |

## Patterns That Must Not Be Copied

| Current Pattern | Why It Is Obsolete |
|---|---|
| App/Manager call core planners then choose save/stop/spawn order themselves | Effect lists are advisory and have already diverged. |
| `active_intent` scan at startup | It can launch protected recovery states. |
| assigning `health = Available` during occupied reuse | It destroys teardown/removal/activation/rollback evidence. |
| generic `state_for_save` runtime overlay | It can overwrite exact lifecycle candidates and shell state. |
| boolean `agent_restarted` / `shell_restarted` durable recovery | Booleans are not process authority. |
| ignored `kill_and_wait`, `kill()`, map deletion, or child-handle drop as cleanup | None confirms that the exact process is gone. |
| separate App and Manager restoration helpers with merely similar intent | CR-02 demonstrates behavioral drift; one effect contract must define success. |

## No Analog Found

| File/Concern | Role | Data Flow | Reason / Planner Guidance |
|---|---|---|---|
| `baude-core/src/lifecycle.rs` effect-driving engine | protocol/service | event-driven | Current core emits plans while owners drive effects. Use the `LifecycleEngine<E>` design in `06-RESEARCH.md`; do not copy either owner transaction. |
| `baude-core/src/lifecycle.rs` canonical mirrored trace | test utility | event-driven + transform | Existing table tests are the shape only; normalized cross-owner trace does not exist. |
| `baude-core/src/pty.rs` abrupt owner-death guard | process safety | process-I/O | Current spawn establishes identity but no test proves child death on owner/handle loss. Characterize first, then implement a guard/handshake if needed. |

## Metadata

**Analog search scope:** `baude-core/src`, `baude/src`, `bauded/src`, corrective research, and deep review
**Primary analogs read:** `repository.rs`, `lifecycle.rs`, `persist.rs`, `session.rs`, `pty.rs`, `app.rs`, `manager.rs`, `api.rs`
**Tracked-path verification:** all 8 paths returned by `git ls-files --error-unmatch`
**Pattern extraction date:** 2026-08-30
