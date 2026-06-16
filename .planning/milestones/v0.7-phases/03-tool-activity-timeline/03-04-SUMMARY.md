---
phase: 03-tool-activity-timeline
plan: 04
subsystem: ui
tags: [rust, ratatui, tui, serde, hook-events, modal-overlay, remote-poll]

# Dependency graph
requires:
  - phase: 03-tool-activity-timeline
    provides: HookEvent struct + ClaudeMeta.activity() ring (03-01); SessionInfo.activity bounded ~30 on /sessions (03-02)
provides:
  - RemoteInfo.activity Vec<HookEvent> (#[serde(default)], backward-compatible) deserialized from the /sessions poll
  - Modal::Activity variant + `v` dispatch (local OR remote selection) + dismiss-on-any-key arm
  - draw_modal Modal::Activity render arm (remote branch first, then local), newest-at-bottom, render-last-N-that-fit
  - activity_icon/activity_label/activity_age/activity_lines render helpers
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Modal::Activity overlay mirrors Modal::Info's Clear+Paragraph+Block + remote-first/local branch structure"
    - "Render-last-N-that-fit anchored to bottom (no scroll-offset widget); full retrieval deferred to GET /activity (RESEARCH Open Q1)"
    - "RemoteInfo backward-compat field via #[serde(default)] mirroring state_source/last_tool/archived"

key-files:
  created: []
  modified:
    - baude/src/remote.rs
    - baude/src/app.rs
    - baude/src/ui.rs

key-decisions:
  - "Both code tasks landed in the same wave so clippy never saw an unused Modal::Activity variant; Task 1 = state plumbing (remote.rs+app.rs), Task 2 = render arm + serde tests (ui.rs+remote.rs)"
  - "Added two remote.rs serde unit tests (deserialize with AND without activity) — the only automatable seam in this TUI-client plan; ACT-04 open/dismiss is human-verify UAT (no app.rs test seam, per 03-VALIDATION.md)"
  - "activity_age uses a compact s/m/h/d relative format (overlay tone) rather than reusing human_until (which is future-tense 'in Xm')"

patterns-established:
  - "Activity feed derived from one HookEvent model across all surfaces — TUI-local reads s.meta.activity(), TUI-remote reads RemoteInfo.activity bundled in /sessions, no extra round-trip"
  - "Render-last-N with a '… N earlier' marker; the daemon GET endpoint owns full retrieval/paging"

requirements-completed: [ACT-04]

# Metrics
duration: 4min
completed: 2026-06-15
---

# Phase 3 Plan 04: TUI Activity Overlay Summary

**A `v`-triggered `Modal::Activity` overlay rendering the recent tool sequence newest-at-bottom (mirroring the `i` Info overlay), reading `s.meta.activity()` for local sessions and a `#[serde(default)]` `RemoteInfo.activity` bundled into the `/sessions` poll for remote sessions — live-refreshing on the existing draw tick, no extra round-trip.**

## Performance

- **Duration:** ~4 min (autonomous code tasks; UAT pending)
- **Started:** 2026-06-15T21:00Z
- **Completed (code):** 2026-06-15T21:04Z
- **Tasks:** 2 of 3 (Task 3 is a human-verify UAT — pending)
- **Files modified:** 3

## Accomplishments
- Added `#[serde(default)] RemoteInfo.activity: Vec<HookEvent>` deserialized from the daemon's `/sessions` JSON — backward-compatible against an older daemon that omits the field (defaults to empty), mirroring the existing `state_source`/`last_tool`/`archived` pattern.
- Added the `Modal::Activity` variant, the `v` dispatch in `handle_sidebar_key` (opens for a selected local OR remote session, exactly like `i`), and the dismiss-on-any-key arm in `handle_modal_key`.
- Added the `draw_modal` `Modal::Activity` render arm: remote branch first (`r.activity`), then local (`s.meta.activity()`); newest-at-bottom rows of icon + tool/notification-type + compact relative age, render-last-N-that-fit with a "… N earlier" marker, full retrieval deferred to the GET `/sessions/{id}/activity` endpoint (RESEARCH Open Q1).
- Added a `v` line to the Help overlay.
- Added two `remote.rs` serde unit tests proving `RemoteInfo` deserializes with AND without `activity` (T-03-11 backward-compat) — the only automatable seam in this TUI-client plan.

## Task Commits

Each task was committed atomically:

1. **Task 1: RemoteInfo.activity + Modal::Activity variant + `v` dispatch + dismiss** — `63bca7b` (feat)
2. **Task 2: Modal::Activity render arm + remote deserialize tests** — `19c75ee` (feat)
3. **Task 3: TUI activity overlay UAT (local + remote)** — PENDING (human-verify; see below)

**Plan metadata:** pending the post-UAT docs commit.

_Both code tasks landed in the same wave so `clippy -D warnings` never saw an unused `Modal::Activity` variant. The `remote.rs` serde tests were folded into Task 2 (they prove the render arm's remote data source deserializes)._

## Files Created/Modified
- `baude/src/remote.rs` — Imported `baude_core::meta::HookEvent`; added `#[serde(default)] RemoteInfo.activity`; added a `#[cfg(test)]` module with two deserialize tests (with/without `activity`).
- `baude/src/app.rs` — Added the `Modal::Activity` variant; the `v` arm in `handle_sidebar_key`; `Modal::Activity` in the `handle_modal_key` dismiss arm.
- `baude/src/ui.rs` — Imported `now_unix_ms`/`HookEvent`; added `activity_icon`/`activity_label`/`activity_age`/`activity_lines` helpers; added the `draw_modal` `Modal::Activity` arm (remote-first/local); added the `v` Help line (bumped the Help box height 25→26).

## Decisions Made
- `activity_age` uses a compact `now`/`Xs`/`Xm`/`Xh`/`Xd` past-tense format rather than reusing `human_until` (which is future-tense "in Xm") — the overlay shows how long ago each event happened.
- Render-last-N-that-fit (`MAX_ROWS = 24`) anchored to the bottom with a "… N earlier" marker; no scroll-offset widget this phase (RESEARCH Open Q1 first-cut; the GET endpoint owns full retrieval).
- The `DoubleEndedIterator` bound on `activity_lines` was trimmed to plain `ExactSizeIterator<Item = &HookEvent>` (only forward `.skip()` + `.len()` are used) — keeps the helper usable for both `VecDeque::iter()` (local) and `Vec::iter()` (remote).

## Deviations from Plan

None — plan executed exactly as written. (The two `remote.rs` serde tests are the planned "add automatable unit coverage that DOES have a seam" item from the project brief, not unplanned scope.)

## Threat Model Compliance
- **T-03-11 (RemoteInfo deserialize against a daemon that omits `activity`):** mitigated — `#[serde(default)]` on `RemoteInfo.activity` → empty Vec, no deserialize failure; `remote_info_deserializes_without_activity` proves a payload omitting the field deserializes to an empty activity Vec, and `remote_info_deserializes_with_activity` proves a bundled `activity[]` populates the overlay's source Vec.
- **T-03-12 (unbounded remote activity in the overlay):** mitigated — `SessionInfo.activity` is bounded to ~30 at the daemon (plan 02) and the overlay additionally clips to `MAX_ROWS` render-last-N. Response and render are both bounded.
- **T-03-SC (npm/pip/cargo installs):** accept — no installs this phase; `ratatui`/`serde`/`serde_json` are existing vetted workspace crates.

## Verification
- **Automated (green):** `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (100 tests across crates, including the 2 new `remote.rs` deserialize tests; no regressions).
- **Manual (ACT-04) — PENDING human-verify UAT:** the `v` open/dismiss + local/remote render + live refresh. `baude/src/app.rs` has no test seam (`handle_*_key` private, `App::new` needs a real launch_dir, full ratatui render + live remote poll need an interactive terminal), so ACT-04 open/dismiss is human-verify per 03-VALIDATION.md — consistent with Phase-2's TUI UAT. NOT fabricated; cannot run in the automated executor.

## Pending UAT — exact manual steps

> Run from the repo root after `cargo build --workspace`.

**Local session:**
1. `cargo run -p baude` to launch the TUI.
2. Select a LOCAL managed session that has run some tools. Press `v`.
3. Confirm the activity overlay opens, mirrors the `i` overlay style (rounded cyan border, `Clear` backdrop), titled `activity — <name>`, and shows the recent tool sequence newest-at-bottom (icon + tool/notification-type + relative age, e.g. `⚙ Bash   12s`).
4. Drive a tool call in that session and confirm the overlay refreshes live while open (~1s meta tick).
5. Press any key — confirm it dismisses back to the session list (`Modal::None`).

**Remote session:**
6. With `bauded` running and a session spawned via the daemon, select that REMOTE session in the TUI. Press `v`.
7. Confirm the overlay (titled `activity — <name> (remote)`) shows the remote session's recent activity (bundled via `/sessions`, ~30 recent) and refreshes on the ~3s remote poll. Dismiss with any key.

**Negative:**
8. Confirm `v` does nothing when no session is selected (no panic, no empty modal).

**If the remote overlay is empty but local works:** check `SessionInfo.activity` population in plan 02 and `RemoteInfo.activity` `#[serde(default)]` deserialization (the two `remote.rs` tests cover the deserialize side; an empty remote overlay would point at daemon-side population).

**Resume signal:** Type "approved" (or describe issues) to land the metadata/docs commit and complete the plan.

## Known Stubs
None.

## Issues Encountered
- `cargo fmt --check` flagged one over-width `assert_eq!` in the new `remote.rs` test; resolved with `cargo fmt` before committing Task 2. Not a deviation — routine formatting.

## User Setup Required
None — no external service configuration required.

## Next Phase Readiness
- ACT-04 code surface is complete and CI-green; the phase's last open item is the human-verify UAT (local + remote `v` overlay) documented above.
- The full Phase-3 activity feed is now wired end-to-end: ring (01) → daemon GET/SSE + SessionInfo.activity (02) → PWA strip (03) → TUI overlay (04).

## Self-Check: PASSED

- `baude/src/remote.rs` (`pub activity: Vec<HookEvent>`), `baude/src/app.rs` (`Modal::Activity`), `baude/src/ui.rs` (`Modal::Activity` render arm), and `03-04-SUMMARY.md` all present.
- Commits `63bca7b`, `19c75ee` present in git history.
- CI triad green at the checkpoint (fmt/clippy/test).

> Note: Task 3 (human-verify UAT) is intentionally not yet complete — this self-check covers the autonomous code tasks. The plan metadata/state-advance commit lands after the UAT is approved.

---
*Phase: 03-tool-activity-timeline*
*Completed (code): 2026-06-15 — UAT pending*
