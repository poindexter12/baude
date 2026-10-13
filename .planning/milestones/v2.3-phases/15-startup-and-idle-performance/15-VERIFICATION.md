---
phase: 15-startup-and-idle-performance
verified: 2026-09-22T08:00:00Z
status: passed
score: 9/9 must-haves verified
covered_files:
  - .planning/phases/15-startup-and-idle-performance/15-01-PLAN.md
  - .planning/phases/15-startup-and-idle-performance/15-01-SUMMARY.md
  - .planning/phases/15-startup-and-idle-performance/15-02-PLAN.md
  - .planning/phases/15-startup-and-idle-performance/15-02-SUMMARY.md
  - .planning/phases/15-startup-and-idle-performance/15-03-PLAN.md
  - .planning/phases/15-startup-and-idle-performance/15-03-SUMMARY.md
  - .planning/phases/15-startup-and-idle-performance/15-04-PLAN.md
  - .planning/phases/15-startup-and-idle-performance/15-04-SUMMARY.md
  - README.md
  - baude-core/src/backend/mod.rs
  - baude-core/src/meta.rs
  - baude-core/src/persist.rs
  - baude-core/src/pty.rs
  - baude-core/src/session.rs
  - baude/src/app.rs
  - baude/src/main.rs
  - baude/src/ui.rs
  - baude/src/usage.rs
  - bauded/src/api.rs
  - bauded/src/main.rs
  - bauded/src/manager.rs
covered_digest: v1:sha256:45852649bbeecc595fec2b9ba0793e4a3d62a65ead2864e755dbc9b3741f814b
re_verification: false
overrides_applied: 0
behavior_unverified: 0
---

# Phase 15: Startup and Idle Performance Verification Report

**Phase Goal:** Startup is measurable and reaches the first frame quickly without blocking on external services, and an idle baude with many sessions costs near-zero CPU: no unconditional or timer-driven redraws, no polling of dead rows, and an opt-in way to suspend idle Claude children.

**Verified:** 2026-09-22
**Status:** PASSED
**Re-verification:** No

## Goal Achievement Summary

All 9 success criteria verified in code. All 9 requirements (PERF-01 through PERF-08, UX-02) implemented and tested. Four CI gates passing: fmt, clippy, build, test (787 passed).

## Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | PERF-01: A timing facility logs each startup stage duration via env var or flag | ✓ VERIFIED | `baude/src/main.rs:385-386` — `BAUDE_TIMING=1` env check; `:546-548` — `probe_keyboard_enhancement` called with 250ms bound; `:641-649` — `StartupTiming::print_to_stderr()` method |
| 2 | PERF-02: First frame renders before session restore and first metadata poll | ✓ VERIFIED | `baude/src/main.rs:780-794` — `if !app.first_frame_drawn { app.first_frame_drawn = true; if !*restore_started { app.restore(); } }` gates restore start to first draw completion; first empty frame renders in draw before `app.restore()` is called |
| 3 | PERF-03: Keyboard probe never delays first frame beyond 250ms and degrades to legacy encoding | ✓ VERIFIED | `baude/src/main.rs:173-214` — `probe_keyboard_enhancement()` has deadline at line 180, returns `Ok(false)` on timeout at `:185-186`; `:546-548` calls it with `Duration::from_millis(250)` |
| 4 | PERF-04: Session restore writes durable state file once, batched | ✓ VERIFIED | `baude/src/app.rs:553` — `deferring_saves: bool` field; `:1323` — set to true during restore; `:1390-1394` — `finish_restore_phase_a()` calls `save_durable_status()` once; `:1675-1676` — `if self.deferring_saves` defers writes |
| 5 | PERF-05: No terminal writes when idle; working and waiting rows use static status glyphs | ✓ VERIFIED | `baude/src/main.rs:772-775` — `if app.dirty { terminal.draw(...); app.dirty = false }` gates draws to dirty flag only; `baude/src/ui.rs:140-155` — `status_code()` returns static chars (`?` `B` `✓` `✗` `-` `A` `!`); no `spinner()` or `flash_on()` functions exist |
| 6 | PERF-06: Idle polling skips archived/exited rows and unchanged files by mtime | ✓ VERIFIED | `baude/src/app.rs:1244-1250` — `if s.archived \|\| s.claude.is_exited() { continue; } s.poll_meta();` skips poll for archived/exited; `baude-core/src/meta.rs` — mtime fields (`last_session_mtime`, `last_events_mtime`) gate reads |
| 7 | PERF-07: Idle child suspension via SIGSTOP/SIGCONT with identity verification | ✓ VERIFIED | `baude-core/src/session.rs:366-383` — `suspend_idle_child()` and `resume_idle_child()` methods; `:414-437` — `apply_idle_child_policy()` calls suspend on archive or resume on unarchive; `baude-core/src/pty.rs` — `suspend()/resume()` use libc::kill with identity check |
| 8 | PERF-08: Usage poller disabled when configured to 0, respects config interval | ✓ VERIFIED | `baude/src/usage.rs:51-62` — `poller_plan(usage_poll_secs)` returns `PollerPlan::Disabled` when `Some(0)`; `:81-96` — `UsagePoller::start()` respects disable flag, doesn't spawn thread; `baude/src/app.rs:776` — `config.usage_poll_secs()` passed to poller |
| 9 | UX-02: Static single-character status codes in existing colors with in-app legend | ✓ VERIFIED | `baude/src/ui.rs:183-194` — `LEGEND` array with 7 statuses; `:140-155` — `status_code()` returns static chars with correct colors (yellow bold for `?`, blue for `B`, green for `✓`, etc.); `:200-217` — `legend_line()` renders colored legend; `:351-353` — legend rendered in footer |

## Code-Level Verification

### PERF-01: Timing Facility

**Expected behavior:** `BAUDE_TIMING=1` enables timing output showing startup stages.

**Evidence:**
- `baude/src/main.rs:385-386`: Env var check `std::env::var("BAUDE_TIMING").ok().as_deref() == Some("1")`
- `:540-564`: Stages recorded: config_load, workspace_resolution, terminal_setup, keyboard_probe (with timeout note if ≥250ms), app_new, first_frame
- `:616`: `timing.print_to_stderr()` called when enabled
- `:647-649`: `print_to_stderr()` method outputs summary line and per-stage detail to stderr
- **Status:** ✓ VERIFIED

### PERF-02: First Frame Before Restore

**Expected behavior:** Empty sidebar frame renders before any session restore or metadata poll starts.

**Evidence:**
- `baude/src/main.rs:772-794`: Run loop draws first, then gates restore to `first_frame_drawn`
- `baude/src/app.rs:504`: `first_frame_drawn: bool` field
- `:1277`: `App::restore()` initializes restore queue without spawning
- `:798-800`: `restore_step()` called only after first frame
- **Status:** ✓ VERIFIED

### PERF-03: Keyboard Probe Bounded to 250ms

**Expected behavior:** Terminal probe completes within 250ms; timeout degrades to legacy encoding.

**Evidence:**
- `baude/src/main.rs:173-214`: `probe_keyboard_enhancement()` with deadline enforcement
  - `:180`: `let deadline = Instant::now() + bound;`
  - `:184-186`: Returns `Ok(false)` when deadline elapsed
  - `:189`: `io.read_with_timeout(remaining)?` respects timeout
- `:546-548`: Called with `Duration::from_millis(250)`
- `:559-563`: Timeout note recorded in timing stages
- **Status:** ✓ VERIFIED

### PERF-04: Durable Save Once Per Restore

**Expected behavior:** Restore batches all saves into one fsync'd write.

**Evidence:**
- `baude/src/app.rs:553`: `deferring_saves: bool` field
- `:1323`: Set true at restore start
- `:1390-1394`: `finish_restore_phase_a()` clears flag and calls `save_durable_status()` once
- `:1675-1676`: `save_durable_status()` checks `if self.deferring_saves` and returns early without writing
- `:1391-1394`: After flag cleared, subsequent saves write immediately
- **Status:** ✓ VERIFIED

### PERF-05: Static Status Codes, No Animation

**Expected behavior:** No spinner/flash_on functions; status codes are static single characters.

**Evidence:**
- `baude/src/ui.rs:140-155`: `status_code()` function returns static tuple:
  - `"?"` (yellow bold) for Waiting
  - `"B"` (blue) for Busy
  - `"✓"` (green) for Completed
  - `"✗"` (dark gray) for Exited
  - `"-"` (gray) for Closed
  - `"A"` (dark gray) for Archived
  - `"!"` (yellow) for Unavailable
- Grep confirms no `fn spinner` or `fn flash_on` in ui.rs
- `baude/src/main.rs:772-775`: `if app.dirty { terminal.draw(...) }` gates terminal writes to dirty flag only
- **Status:** ✓ VERIFIED

### PERF-06: Polling Gates and mtime Checks

**Expected behavior:** Archived/exited sessions never polled; live sessions check mtime before reading.

**Evidence:**
- `baude/src/app.rs:1244-1250`: Loop over sessions:
  ```rust
  if s.archived || s.claude.is_exited() {
      changed |= s.auto_archive_tick(self.auto_archive_ms);
      continue;  // ← skips poll_meta()
  }
  s.poll_meta();
  ```
- `baude-core/src/meta.rs`: Metadata cache fields track mtime (`last_session_mtime`, `last_events_mtime`)
- Backend implementations check mtime before reading files
- **Status:** ✓ VERIFIED

### PERF-07: Idle Child Policy with Identity Verification

**Expected behavior:** Suspend/stop idle children after auto-archive; resume on unarchive; verify PID and start_time before signaling.

**Evidence:**
- `baude-core/src/session.rs:366-383`: `suspend_idle_child()` and `resume_idle_child()` methods
- `:414-437`: `apply_idle_child_policy(policy)` called on archive/unarchive
  - Suspend: calls `self.suspend_idle_child()`
  - Stop: calls `self.stop_idle_child()` (kill)
  - Keep: no-op
  - Unarchive with suspended: calls `self.resume_idle_child()`
- `baude-core/src/pty.rs`: `suspend()/resume()` methods verify identity before SIGSTOP/SIGCONT
- `baude-core/src/persist.rs:idle_child_policy` config field with env override
- **Status:** ✓ VERIFIED

### PERF-08: Configurable Usage Poller

**Expected behavior:** `usage_poll_secs=0` disables poller thread; configurable interval.

**Evidence:**
- `baude/src/usage.rs:51-62`: `poller_plan(usage_poll_secs)` function
  - Returns `PollerPlan::Disabled` when `Some(0)`
  - Returns `PollerPlan::Enabled { interval, failure_backoff }` otherwise
- `:81-86`: Test implementation respects disabled flag
- `:88-100`: Live implementation doesn't spawn thread if disabled
- `baude/src/app.rs:776`: `config.usage_poll_secs()` passed to `UsagePoller::start()`
- `:842-849`: `usage_poll_disabled()` method checks poller state
- **Status:** ✓ VERIFIED

### UX-02: Status Code Legend and Rendering

**Expected behavior:** Static codes in existing colors; legend rendered in footer when space permits.

**Evidence:**
- `baude/src/ui.rs:183-194`: `LEGEND` constant with all 7 statuses and labels
- `:196`: `LEGEND_TEXT` constant with full legend string
- `:200-217`: `legend_line(width)` function builds colored legend line, stops before overflow
- `:318-353`: Footer rendering allocates space for legend (`:321-335` carves legend_area)
- `:351-353`: Legend rendered when area available
- **Status:** ✓ VERIFIED

## Acceptance Criteria Coverage

| Criterion | Status | Evidence |
|-----------|--------|----------|
| Timing facility | ✓ | PERF-01 verified in `baude/src/main.rs:385-616` |
| First frame before restore | ✓ | PERF-02 verified in `baude/src/main.rs:780-794` |
| Keyboard probe bound to 250ms | ✓ | PERF-03 verified in `baude/src/main.rs:173-214` |
| Durable save once, batched | ✓ | PERF-04 verified in `baude/src/app.rs:1323-1394` |
| No idle terminal writes, static codes | ✓ | PERF-05 verified in `baude/src/main.rs:772-775` + `baude/src/ui.rs:140-155` |
| Polling gates (archived/exited skip, mtime check) | ✓ | PERF-06 verified in `baude/src/app.rs:1244-1250` |
| Idle child suspension/resume with identity check | ✓ | PERF-07 verified in `baude-core/src/session.rs:366-437` |
| Usage poller configurable (0 = disabled) | ✓ | PERF-08 verified in `baude/src/usage.rs:51-100` |
| Status codes with legend | ✓ | UX-02 verified in `baude/src/ui.rs:140-353` |

## CI Gates Verification

All four gates passing as of last wave completion (15-04, 2026-09-22):

| Gate | Command | Expected | Actual | Status |
|------|---------|----------|--------|--------|
| Formatting | `cargo fmt --all -- --check` | 0 | 0 | ✓ PASS |
| Clippy | `cargo clippy --all-targets -- -D warnings` | 0 | 0 | ✓ PASS |
| Build | `cargo build --workspace` | success | success | ✓ PASS |
| Test | `cargo test --workspace` | 787 passed | 787 passed | ✓ PASS |

## Test Coverage

All requirements backed by unit tests:
- `baude/src/main.rs::tests`: timing facility, first frame ordering
- `baude/src/app.rs::tests`: dirty flag, restore phases, polling gates, idle policy dispatch
- `baude-core/src/session.rs::tests`: suspend/resume with identity verification
- `baude-core/src/pty.rs::tests`: PausedPty, spawn_paused, release, abort
- `baude/src/ui.rs::tests`: status codes, legend rendering, color correctness
- `baude/src/usage.rs::tests`: poller disable option, interval config
- `bauded/src/api.rs::tests`: startup_ms exposure, idle policy parity

## Deviations and Notes

**Planned deviation (accepted in STATE.md):** Phase 15-02 Plan Task 2 implements all saved sessions spawned paused in one batch during the restore step, rather than batches of 8 per iteration. Rationale: first frame is already drawn, spawns are fast, prevents window where paused children are unrecorded.

**Test flake (monitored, hardened):** `daemon_auto_archive_applies_idle_child_policy` timed out on 4s ps-state deadline once in full-suite run (15/15 in isolation). Hardened to 20s with ps row logging. Product assertion (info.suspended) passed in that failure.

**Code review findings (15-REVIEW.md):** All 15 files reviewed clean (no critical/warning/info issues). Safety comments justified; signal verification precedes delivery; mtime gates prevent legitimate files from skipping.

## Summary

Phase 15 achieves all success criteria through four coordinated waves:

1. **15-01**: Dirty flag gating (zero idle terminal writes) and timing infrastructure
2. **15-02**: Two-phase restore with durable-save-before-unpause ordering, 250ms keyboard probe bound, daemon startup timing
3. **15-03**: Metadata polling gates (skip archived/exited), identity-verified suspend/resume, configurable usage poller
4. **15-04**: Static status codes, legend UI, README Performance documentation

All code verified against source; all gates green; all requirements delivered.

---

_Verified: 2026-09-22T08:00:00Z_
_Verifier: Claude (gsd-verifier)_
_Verification Status: PASSED_
