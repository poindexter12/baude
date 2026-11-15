# Phase 20: TUI Remove Action - Research

**Researched:** 2026-11-04
**Domain:** ratatui TUI key dispatch, modal confirmation and state integration over the shared `baude-core::removal` operation (Rust, no new dependencies)
**Confidence:** HIGH for the core API and integration points (read from source this session); MEDIUM for fixture naming (located by grep, not fully read)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### Key Binding and Reach
- The action is bound to `D` (shift-d), unbound today. `X` keeps its meaning of verified-safe managed-worktree removal with the clean-tree preflight; the Phase 18 force path gets its own key rather than changing what `X` does.
- `D` is offered on repository, main checkout, managed worktree, unmanaged checkout, standalone and Unavailable rows. An Unavailable row with a missing worktree is the "already removed" case Phase 18 reports; recovery state does not block it. Remote rows refuse with a notice, since remote removal arrives with Phase 21.
- `D` works on a row with a live runtime. The confirmation states that the runtime will be killed; the user is not asked to `x` close first.
- The action is appended to every row kind's context footer line and to the `?` help overlay beside `x`, `w` and `X`, using the existing `*` marker for destructive keys.

#### Confirmation Content
- The modal is built from `preview_removal`'s `RemovalReport`, the same data the real run produces. Title names the row kind. Sections: rows dropped (every child listed for a repository parent), runtimes killed (count plus row names), paths deleted, paths kept, and the Phase 19 clearings (bindings, markers, suppression entries). A path is never shown under deleted unless the report lists it there.
- Keys match the existing `ConfirmRemoveWorktree` modal: `Enter` or `y` confirms, `Esc` or `n` cancels. Cancel is non-mutating.
- The modal grows to at most 80% of the screen height. When the child list overflows, `j`/`k` scroll it and a hidden remainder is rendered as "+N more", so the total count is never hidden.
- A new `Modal::ConfirmRemove { target, report }` variant is created when `D` is pressed. On confirm the real run recomputes the row set; if it differs from the previewed report, the modal refreshes with the new report and asks again instead of removing.

#### Failure, Retry and Aftermath
- A kill, disk or save failure returns `Err` from the core before any row drops. App returns the moved `Session` handles to its registry (the existing `restore_removed_runtime` path), keeps every row visible, and the modal stays open showing the typed error (surviving pids, failing path, save error) with `enter retry · esc cancel`.
- Clearing failures after the committed save do not fail the removal. The rows are gone; a non-blocking context-footer notice names what could not be cleared.
- On success the rows vanish from the hierarchy in the same tick the save returns. Selection moves to the next surviving row: the parent when a child was removed, otherwise the next repository. The footer confirms with counts of rows, runtimes killed and paths deleted.
- Tests are App-level with cloned-repo fixtures and a real spawned process group per Phase 18's doubles: `D` on each row kind opens a modal whose lines match the preview report; confirm removes the rows from the hierarchy and the state file in one save; a forced kill failure and a forced save failure each leave the rows visible with the error, and a retry then succeeds.

### Claude's Discretion
- Exact modal line layout, section headings and the help-overlay wording.
- How `App` moves `Session` handles into `RowRuntime` values and back on error, provided no handle is lost on `Err`.
- Whether the preview-versus-confirm recheck compares `rows_removed` only or the whole report.

### Deferred Ideas (OUT OF SCOPE)
- Removing remote rows from the TUI: Phase 21 with the REST and PWA removal.
- Phase 18 and 19 open items remain where they are logged: `scripts/gate.sh --linux` target dir, 18/19 VALIDATION drafts.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| TUI-01 | The remove action is available on repository, main checkout, managed worktree, unmanaged checkout and standalone rows, and the sidebar footer or help lists it alongside `x`, `w` and `X` | `sidebar_action` already matches on `ActionKind::{Repository, Main, Managed, External, Standalone, Unavailable}` for `X` (app.rs:331-345); a `D` arm maps every local kind to a new `SidebarAction::RemoveProject`. Footer strings at ui.rs:1490-1517 and help line at ui.rs:2330 are the two places to extend. |
| TUI-02 | Confirmation names row kind, exact rows dropped, whether a live runtime is killed, and paths deleted vs kept | `preview_removal` returns a `RemovalReport` with `rows_removed`, `runtimes_killed`, `paths_deleted`, `paths_kept`, `already_removed`, `cleared` (removal.rs:180-198, 359-366). Render those fields verbatim; never derive a path list from anything else. |
| TUI-03 | Repository parent lists every child in confirmation and removes them all on confirm | `select_rows` for `RemovalTarget::Repository` expands the parent into per-child `PlannedRow`s (removal.rs:306-312); `rows_removed` therefore already carries every child. `remove` applies `remove_rows` on a cloned candidate state in one batch (removal.rs:436-450). |
| TUI-04 | Failure at any stage leaves rows visible with an error notice; user can retry | `remove` runs `preflight` and mutates only a `candidate = state.clone()` (removal.rs:445-446); every `RemovalError` variant (removal.rs:235-256) is typed and returned before `state` changes. App's existing `atomic_failure_for_test` hook (app.rs:604) is the save-failure injection point; `restore_removed_runtime` (app.rs:3632) is the handle-restoration path. |
</phase_requirements>

## Summary

Phase 20 is a pure integration phase: no new crates, no new core logic. Everything the modal must show and everything the confirm must do already exists in `baude-core/src/removal.rs` behind two functions, `preview_removal(&state, target) -> Result<RemovalReport, RemovalError>` and `remove(&mut state, target, &mut Vec<RowRuntime>, save) -> Result<RemovalReport, RemovalError>`. The TUI work is (1) a `D` arm in `sidebar_action` that yields a new `SidebarAction::RemoveProject` for every local `ActionKind`, (2) a `Modal::ConfirmRemove { target, report }` variant plus a renderer that prints the report's fields, (3) a `confirm_remove_project` method on `App` that moves live `Session` handles into `RowRuntime`s, calls `remove` with the existing atomic save closure, and on `Err` puts the handles back and leaves the modal open with the error, and (4) footer and help string updates.

The existing `X` flow (`prepare_remove_worktree` / `confirm_remove_worktree` / `Modal::ConfirmRemoveWorktree`, app.rs:3714-3760 and 5292-5300) is the template to copy structurally, but not to extend: it is built on `lifecycle::RemovalConfirmation` and the clean-tree preflight, and the locked decision keeps `X` unchanged. The new flow is a sibling, not a modification.

The main risks are all in `App`, not in the core: losing a `Session` handle when `remove` returns `Err` (the core takes `&mut Vec<RowRuntime>` and must leave handles in it on failure; App must drain them back into its registry), the preview-versus-confirm drift check, and selection after rows vanish. Tests must be App-level against cloned-repo fixtures and a real process group, asserting on rendered modal lines and the state file, per CLAUDE.md.

**Primary recommendation:** Build `D` as a sibling of `X` over `preview_removal`/`remove`, render the modal purely from `RemovalReport` fields, and keep every `Session` handle accounted for on `Err` by draining the `runtimes` vector back into `App` before showing the error.

## Project Constraints (from CLAUDE.md)

- Cargo workspace: `baude-core` (no UI deps), `baude` (ratatui TUI), `bauded` (axum daemon). Nothing in this phase touches `bauded`.
- Run `scripts/gate.sh` before every push. Use `scripts/gate.sh --linux` for anything touching platform code (process groups, procfs, file timing). Never judge a gate step by piping to `tail`/`grep` and reading `$?`.
- Every change lands by PR to protected `main`; release-please owns versions. Never nest parentheses in a commit body, squash message or PR title (`scripts/check-commit-message.sh`, enable with `git config core.hooksPath .githooks`).
- Tests: build fixtures in the shapes real users produce (cloned vs `git init` repos, stale runtime rows, command doubles that accept every appended argument), assert on fixture output, never by grepping source.
- Orchestrator rules for this phase: key `D` is the binding; `X` keeps its verified-safe meaning; `workflow.tdd_mode: true` in `.planning/config.json` so each plan writes failing tests first.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Row selection, kill, disk removal, persistence, clearings | `baude-core::removal` | — | Already implemented in Phases 18/19; the TUI must not reimplement any stage |
| Key dispatch (`D`), refusal of remote rows | `baude` TUI (`app.rs` `sidebar_action`) | — | Same place `X` is dispatched today |
| Confirmation content | `baude-core` (`RemovalReport`) | `baude` TUI (`ui.rs` renderer) | Data comes from the core; the TUI only formats |
| Session handle custody across the call | `baude` TUI (`App`) | `baude-core` (`RowRuntime` vector) | App owns the registry; core borrows handles through `&mut Vec<RowRuntime>` |
| Atomic state save | `baude` TUI's existing persist closure | `baude-core::persist` | `remove` takes `save: impl FnOnce(&StateFile) -> Result<(), SaveError>` so App's path is reused |
| Selection and hierarchy refresh after success | `baude` TUI (`App`) | — | Pure view state |
| Daemon / PWA removal | Out of scope (Phase 21) | — | Deferred by CONTEXT.md |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `baude-core::removal` | workspace (in-repo) | `RemovalTarget`, `preview_removal`, `remove`, `RemovalReport`, `RemovalError`, `RowRuntime` | The shared operation Phases 18/19 built for exactly this consumer `[VERIFIED: baude-core/src/removal.rs:49-56, 180-198, 235-256, 359-375, 436-450]` |
| `ratatui` + `crossterm` | already in `baude/Cargo.toml` | Modal rendering, key events | Existing TUI stack; `KeyCode::Char('X')` with `KeyModifiers::SHIFT` is the pattern in use `[VERIFIED: baude/src/app.rs:332-333]` |

### Supporting
None new. No external packages are installed in this phase, so the Package Legitimacy Audit is not applicable.

**Installation:** none.

## Package Legitimacy Audit

Not applicable: this phase adds no dependencies. `[VERIFIED: baude/Cargo.toml dev-dependencies read this session show only the workspace `baude-core` with `test-support`]`

## Architecture Patterns

### System Architecture Diagram

```
key event (D, shift)
   │
   ▼
sidebar_action(view, key) ──► ActionKind remote? ──yes──► SidebarAction::Refuse(RemoteRemove) ──► refuse_sidebar_action ──► context footer notice
   │ no
   ▼
SidebarAction::RemoveProject
   │
   ▼
App::prepare_remove_project(target)
   │  RemovalTarget from selected row (Repository / Checkout / Standalone)
   ▼
baude_core::removal::preview_removal(&repository_state, target)
   │  Ok(report)                              Err(e) ─► footer notice (UnknownTarget etc.)
   ▼
Modal::ConfirmRemove { target, report, scroll, error: None }
   │  Enter/y                                  Esc/n ─► Modal::None (no mutation)
   ▼
App::confirm_remove_project(target)
   ├─ recompute preview; rows_removed differs? ─yes─► replace modal report, ask again
   ├─ drain Session handles for report.rows_removed into Vec<RowRuntime>
   ├─ removal::remove(&mut state, target, &mut runtimes, |file| atomic save)
   │     ├─ preflight (handle present, identity match)
   │     ├─ kill stage (SIGTERM→SIGKILL per Phase 18)
   │     ├─ disk stage (managed worktrees only)
   │     ├─ persist stage (one save of the candidate state)
   │     └─ clear stage (bindings, markers, suppression) ─ failures recorded, not fatal
   ├─ Err(e) ─► return every remaining RowRuntime to registry ─► modal stays open with error + "enter retry · esc cancel"
   └─ Ok(report) ─► drop remaining handles, rebuild hierarchy, move selection, footer counts,
                    footer notice for report.clearing_failures if non-empty
```

### Recommended Project Structure

No new files are required. Changes land in:

```
baude/src/app.rs      # SidebarAction::RemoveProject, Modal::ConfirmRemove, prepare/confirm_remove_project, handle_key arm, tests
baude/src/ui.rs       # draw_modal arm, remove_project_confirmation_lines, footer hint strings, help overlay line, render tests
```

If `app.rs` growth is a concern (it is 14,281 lines), a `baude/src/app/remove_project.rs` submodule holding the two App methods and their tests is reasonable, but it is discretionary.

### Pattern 1: Sibling of the `X` flow, not an extension

**What:** Copy the shape of `prepare_remove_worktree` / `confirm_remove_worktree` / `Modal::ConfirmRemoveWorktree` and the `handle_key` arm at app.rs:5292-5300, but back it with `removal::preview_removal`/`removal::remove` instead of `lifecycle::prepare_removal`.
**When to use:** Always for this phase. The locked decision forbids changing `X`.
**Key observed details** `[VERIFIED: baude/src/app.rs:3714-3745, 5292-5300]`:
- `prepare_remove_worktree` refuses when `self.persistence_dirty || self.repository_state.has_pending_activation()` and reserves the repository via `self.repository_reservations.reserve(...)`. The new flow should take the same reservation to avoid racing a concurrent lifecycle op, but per the locked decision it must NOT refuse on recovery state for Unavailable rows (that is the "already removed" case).
- The key arm does `std::mem::replace(&mut self.modal, Modal::None)` before calling confirm. For the new flow, on `Err` the modal must be put back with the error attached, so either keep the modal in place and borrow the target out, or re-create it on `Err`.

### Pattern 2: Render only `RemovalReport` fields

**What:** The modal body is a pure function `fn remove_project_confirmation_lines(report: &RemovalReport, row_names: &dyn Fn(RowKey) -> String, width: usize) -> Vec<String>`, mirroring `remove_confirmation_lines` at ui.rs:2393.
**Fields and suggested headings** `[VERIFIED: baude-core/src/removal.rs:180-198]`:

```rust
pub struct RemovalReport {
    pub target: RemovalTarget,
    pub planned: bool,
    pub rows_removed: Vec<RowKey>,
    pub runtimes_killed: Vec<KilledRuntime>,
    pub paths_deleted: Vec<PathBuf>,
    pub paths_kept: Vec<PathBuf>,
    pub already_removed: Vec<PathBuf>,
    pub readers_parked: Vec<RowKey>,
    pub persistence: PersistenceOutcome,
    pub cleared: Vec<ClearedItem>,
    pub clearing_failures: Vec<ClearingFailure>,
}
```

| Section | Source field | Note |
|---------|--------------|------|
| Title | `target` → row kind name | `RemovalTarget::{Repository, Checkout, Standalone}`; for `Checkout` resolve kind (main / managed / unmanaged / unavailable) from the saved checkout |
| Rows dropped | `rows_removed` | Every child appears here for a repository parent (TUI-03). Scrollable, "+N more" when truncated |
| Runtimes killed | `runtimes_killed` (`row`, `agent_pid`, `shell_pid`) | In the preview `planned == true`; count plus row names |
| Paths deleted | `paths_deleted` | Only this field, never derived |
| Paths kept | `paths_kept` | |
| Already removed | `already_removed` | Unavailable-row case |
| Clearings | `cleared` by `ClearingKind::{Binding, Marker, RepositoryDir, SuppressionAdded, SuppressionDropped}` | Phase 19 fields |

`ClearingKind` variants verbatim `[VERIFIED: baude-core/src/removal.rs:152-160]`:
```rust
pub enum ClearingKind {
    Binding,
    Marker,
    RepositoryDir,
    SuppressionAdded,
    SuppressionDropped,
}
```

### Pattern 3: Session handle custody

**What:** `remove` takes `runtimes: &mut Vec<RowRuntime>` where `RowRuntime { row: RowKey, session: crate::session::Session }` `[VERIFIED: baude-core/src/removal.rs:371-375, 436-442]`. App must move each live `Session` whose row appears in `report.rows_removed` out of its registry into that vector before the call, and on `Err` move every element still in the vector back.
**Recommendation:** Write one `fn take_runtimes_for(&mut self, rows: &[RowKey]) -> Vec<RowRuntime>` and one `fn restore_runtimes(&mut self, runtimes: Vec<RowRuntime>)`; assert in tests that after a forced failure the registry length equals its pre-call length. `restore_removed_runtime` (app.rs:3632) takes a `RetainedSessionState` for a single checkout; it is the precedent, not necessarily the exact function to call, because the new flow may restore several rows and standalone rows. Verify in the plan whether the kill stage drains killed handles out of the vector on success and leaves un-killed ones on `Err` (read `kill_stage` at removal.rs:916 and `preflight` at removal.rs:840 before writing the restore helper).

### Pattern 4: Preview-versus-confirm drift check

**What:** On confirm, call `preview_removal` again and compare with the modal's stored report before calling `remove`.
**Recommendation (discretion area):** Compare `rows_removed` and `paths_deleted` only. `runtimes_killed` legitimately changes if a session exited between preview and confirm, and `cleared` is derived from the same rows; comparing the whole report would cause spurious re-asks. If they differ, replace the modal's report and return without mutating.

### Pattern 5: Failure injection in tests

- **Save failure:** `App` already carries `atomic_failure_for_test: Option<persist::AtomicFailure>` (app.rs:604) consumed by the persist path and cleared after one use (app.rs:1507) `[VERIFIED by grep; read lines 604, 894, 1498-1507 before relying on exact semantics]`. Set it, press Enter, assert rows still visible and modal shows the `RemovalError::Persist` message, clear it, press Enter again, assert success.
- **Kill failure:** Phase 18's removal tests spawn a real process group and use a teardown double that survives escalation, producing `RemovalError::RuntimeSurvived` (removal.rs tests module from line 1169). `[ASSUMED]` that the doubles are reachable from `baude` through the `test-support` feature; confirm by reading the Phase 18 test fixtures before planning the TUI test, and if they are private to `baude-core`, expose them under `#[cfg(feature = "test-support")]` rather than duplicating.

### Anti-Patterns to Avoid
- **Deriving "paths deleted" from the row kind in the TUI.** Only `report.paths_deleted` may populate that section (locked decision).
- **Reusing `lifecycle::RemovalConfirmation` or `prepare_removal` for `D`.** They carry the clean-tree preflight and refuse non-managed rows; `D` must offer every local kind.
- **Dropping the modal before calling `remove`.** The `X` arm does `mem::replace(.., Modal::None)` first; for `D` the modal must survive an `Err`.
- **Letting `remove` Err leave handles in a local `Vec<RowRuntime>` that is then dropped.** That is a leaked, still-running agent.
- **Treating `clearing_failures` as a removal failure.** Per the locked decision they produce a footer notice only.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Which rows a parent removal drops | TUI-side child walk | `removal::select_rows` via `preview_removal` | Core already handles unavailable children, standalone rows, and ordering |
| Kill / disk / save ordering | Per-row loop in App | `removal::remove` | Phase 18 proved atomicity and SIGTERM→SIGKILL; the TUI must share one path with Phase 21 |
| Atomic persistence | New save code | Pass App's existing atomic save closure to `remove` | Same closure `X` uses; keeps `atomic_failure_for_test` working |
| Clearing bindings/markers/suppression | TUI code | `remove`'s clear stage | Phase 19 owns it; the report already lists results |

**Key insight:** every piece of removal truth lives in `RemovalReport`; the TUI's only job is to present it faithfully and keep handles safe.

## Common Pitfalls

### Pitfall 1: Session handle leak on Err
**What goes wrong:** `remove` returns `Err`, the local `Vec<RowRuntime>` goes out of scope, the agent keeps running with no row.
**Why it happens:** Rust ownership makes the leak silent; nothing forces restoration.
**How to avoid:** Restore from the vector in every `Err` arm; test asserts registry count unchanged after a forced failure.
**Warning signs:** A test that forces a kill failure and then cannot `x` close the row.

### Pitfall 2: Unavailable rows blocked by recovery guard
**What goes wrong:** Copying `prepare_remove_worktree`'s `persistence_dirty || has_pending_activation` refusal blocks exactly the "already removed" case the locked decision wants `D` to handle.
**How to avoid:** Keep the reservation, drop the recovery refusal for `D`; let `preview_removal` report `already_removed`.

### Pitfall 3: Selection points at a vanished row
**What goes wrong:** `selected_id` still names a removed `CheckoutKey`; footer or pane panics or shows stale data.
**How to avoid:** Capture parent key (or next repository) before `remove`; set `selected_id` after hierarchy rebuild; `confirm_remove_worktree` snapshots `visible_hierarchy_rows()` first (app.rs:3740) and is the precedent.

### Pitfall 4: Modal overflow hides the count
**What goes wrong:** A repository with many children renders past 80% height and the user never sees how many rows go.
**How to avoid:** Cap the list region, keep a `scroll: usize` in the modal variant, render "+N more" from `rows_removed.len() - shown`.

### Pitfall 5: Footer string tests
**What goes wrong:** ui.rs has exact-string tests for every footer line (ui.rs:2919-3036) and for `X removes` in help (ui.rs:2706); adding `D remove*` breaks them.
**How to avoid:** Update those assertions in the same task as the strings; new assertions check that `D` appears on each local kind and does not appear on remote rows (compare ui.rs:3158 which asserts `X remove` absent).

### Pitfall 6: Phase 19 still finishing on this branch
Plan 19-07 has no SUMMARY yet; `removal.rs` may still move. Re-read `RemovalReport` and `RemovalError` at plan time; the field list above is HEAD at `6ecaed1`.

## Code Examples

### Dispatch arm (sketch, mirrors app.rs:331-345)
```rust
// Source: pattern from baude/src/app.rs:331-345 (X dispatch); D arm is new
let remove_project_key = matches!(key.code, KeyCode::Char('D'))
    || matches!(key.code, KeyCode::Char('d')) && key.modifiers.contains(KeyModifiers::SHIFT);
if remove_project_key {
    return match view.kind {
        ActionKind::Repository | ActionKind::Main | ActionKind::Managed
        | ActionKind::External | ActionKind::Standalone | ActionKind::Unavailable => {
            SidebarAction::RemoveProject
        }
        // remote kinds: Refuse with a new SidebarRefusal variant naming Phase 21
        _ => SidebarAction::Refuse(SidebarRefusal::RemoteRemove),
    };
}
```
Note: the exact set of `ActionKind` variants beyond those listed above (remote kinds) was not read this session; confirm the enum before writing the arm `[ASSUMED]`.

### Confirm (sketch)
```rust
// Source: API from baude-core/src/removal.rs:359-366, 436-442
pub fn confirm_remove_project(&mut self) {
    let Modal::ConfirmRemove { target, report, .. } = &self.modal else { return };
    let target = *target;
    let fresh = match removal::preview_removal(&self.repository_state, target) {
        Ok(r) => r,
        Err(e) => { self.set_remove_error(e); return; }
    };
    if fresh.rows_removed != report.rows_removed || fresh.paths_deleted != report.paths_deleted {
        self.replace_remove_report(fresh); // ask again
        return;
    }
    let mut runtimes = self.take_runtimes_for(&fresh.rows_removed);
    let result = removal::remove(&mut self.repository_state, target, &mut runtimes, |file| {
        self.save_state_file(file) // the existing atomic save closure
    });
    match result {
        Ok(report) => { drop(runtimes); self.after_remove_success(report); }
        Err(e) => { self.restore_runtimes(runtimes); self.set_remove_error(e); }
    }
}
```
The borrow of `self` inside the save closure while `self.repository_state` is mutably borrowed will not compile as written; the `X` flow solves this already in `confirm_remove_worktree` (app.rs:3736 onward). Read that body and reuse its approach (likely a free function or a cloned config path).

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `X` via `lifecycle::prepare_removal` with clean-tree preflight, managed worktrees only | `D` via `removal::preview_removal`/`remove`, every local row kind, kill-first | Phases 18-20 | Two keys, two meanings; `X` is unchanged |

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Phase 18 process-group and teardown doubles are reachable from `baude` tests via the `test-support` feature | Pattern 5 | Kill-failure TUI test needs a fixture exposed first (small extra task) |
| A2 | `atomic_failure_for_test` is consumed by the same save path `remove`'s closure will call | Pattern 5 | Save-failure test would not trigger; planner should verify by reading app.rs:1498-1510 |
| A3 | The remote `ActionKind` variants are the only ones not listed in the `X` arm | Code Examples | Dispatch arm misses a kind; compile error or missing refusal |
| A4 | RESOLVED (PATTERNS.md): `kill_stage` takes `&mut [RowRuntime]` and kills in place without draining; only `persist_stage` drains, after a successful save, so on any `Err` every supplied handle is still in the vector and the restore helper puts every element back | Pattern 3 | Resolved by reading removal.rs 840, 918, 1032 |

## Open Questions (RESOLVED)

1. **How does `remove` leave `runtimes` on `Err`?**
   - What we know: signature is `&mut Vec<RowRuntime>`; `preflight` checks handle presence.
   - What's unclear: whether killed sessions are drained before a later disk/save failure.
   - Recommendation: first plan task reads `kill_stage`/`preflight` and writes the restore helper to match; test asserts no handle lost.
   - RESOLVED: `kill_stage` and `preflight` never drain the vector; draining happens only in `persist_stage` after the save succeeds. On `Err` every `RowRuntime` is still present, killed ones included. Plan 20-04 Task 1 re-confirms and the App restores every element.
2. **Does the save closure need `&self`?**
   - Recommendation: reuse whatever `confirm_remove_worktree` does today.
   - RESOLVED: no. The closure is built from locals, config or workspace plus a copied `atomic_failure_for_test`, as `save_removal_revocation` does, because `&mut self.repository_state` is borrowed by `remove`. Plan 20-02 Task 2.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain / cargo | build and tests | ✓ (existing workspace, gate passes on branch) | per `rust-toolchain` | — |
| git CLI | cloned-repo fixtures | ✓ (used by Phase 18/19 tests) | — | — |
| Docker | `scripts/gate.sh --linux` | not probed this session | — | Run macOS gate; flag Linux gate for human if Docker absent |

No new external dependencies.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `cargo test` (unit tests in `#[cfg(test)] mod tests` inside `app.rs`, `ui.rs`, `removal.rs`) |
| Config file | none; `scripts/gate.sh` is the suite |
| Quick run command | `cargo test -p baude remove_project` |
| Full suite command | `scripts/gate.sh` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| TUI-01 | `D` yields `RemoveProject` on each local kind, `Refuse` on remote; footer/help strings contain `D remove*` | unit (App + ui render) | `cargo test -p baude sidebar_action_d` / `cargo test -p baude footer` | ❌ Wave 0 (new tests in existing modules) |
| TUI-02 | Modal lines equal preview report fields on each row kind, including live-runtime case | App-level with cloned-repo fixture + real process group | `cargo test -p baude confirm_remove_modal` | ❌ |
| TUI-03 | Repository parent lists every child; confirm drops all from hierarchy and state file in one save | App-level | `cargo test -p baude remove_repository_parent` | ❌ |
| TUI-04 | Forced kill failure and forced save failure leave rows visible with error; retry succeeds; no handle lost | App-level | `cargo test -p baude remove_project_failure` | ❌ |

### Sampling Rate
- **Per task commit:** `cargo test -p baude remove_project`
- **Per wave merge:** `scripts/gate.sh`
- **Phase gate:** `scripts/gate.sh` and `scripts/gate.sh --linux` green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] Expose or locate Phase 18 process-group and teardown doubles for `baude` tests (A1)
- [ ] Confirm `atomic_failure_for_test` injection reaches the `remove` save closure (A2)

## Security Domain

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | local TUI |
| V3 Session Management | no | — |
| V4 Access Control | no | — |
| V5 Input Validation | yes | Only `RemovalTarget` keys from the selected row reach the core; no free-text paths |
| V6 Cryptography | no | — |

### Known Threat Patterns
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Deleting outside the managed root | Tampering | Core `disk_stage` containment from Phase 18; TUI never passes paths, only keys |
| Orphaned process after failed removal | Denial of Service | Handle restoration on `Err` (Pattern 3) |

## Sources

### Primary (HIGH confidence)
- `baude-core/src/removal.rs` lines 49-100, 134-200, 235-256, 306-312, 359-375, 436-450 (read this session)
- `baude/src/app.rs` lines 310-345, 444-478, 3632-3640, 3714-3745, 5292-5300 (read); 604, 1507 (grep)
- `baude/src/ui.rs` lines 1490-1517, 1635, 2330, 2393-2400, 2706, 2919-3036, 3158 (grep and partial read)
- `.planning/phases/20-tui-remove-action/20-CONTEXT.md`, `REQUIREMENTS.md`, `ROADMAP.md`

### Tertiary (LOW confidence)
- Phase 18 test double names and reach (not read; see A1)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH, in-repo API read from source
- Architecture: HIGH for integration points, MEDIUM for handle custody until `kill_stage` is read
- Pitfalls: MEDIUM, derived from observed `X` flow and exact-string footer tests

**Research date:** 2026-11-04
**Valid until:** until Phase 19 plan 19-07 lands; re-read `RemovalReport` then
