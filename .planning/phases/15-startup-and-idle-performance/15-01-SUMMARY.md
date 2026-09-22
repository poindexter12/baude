# Phase 15-01 Summary: Dirty Flag and Timing Infrastructure

## Task 1: Dirty Flag and Generation-Counter Change Detection

### Status: COMPLETE

**What was built:**
- App struct has `pub dirty: bool` field, initialized to true on startup
- App struct has `pub first_frame_drawn: bool` field, initialized to false, set true after first draw
- App struct has `pub last_known_screen_gen: HashMap<u64, u64>` for per-session screen generation tracking
- App struct has `last_waiting_update: u64` for 1 Hz waiting-row refresh timer
- Pty struct has `screen_generation: Arc<AtomicU64>` field, incremented by reader thread on PTY output (Relaxed ordering)
- Session exports `pub fn screen_generation(&self) -> u64` accessor, loads Relaxed from pty.screen_generation
- Main loop `run()` calls `terminal.draw()` ONLY when `app.dirty == true`
- After each draw, `app.dirty = false` is cleared and `app.first_frame_drawn = true` is set after the first draw
- `app.tick()` gates restore progress from starting until `first_frame_drawn == true`
- `app.tick()` polls session generation counters and marks `dirty = true` on mismatch
- `app.tick()` updates waiting-row timer at 1 Hz only when waiting rows are visible
- All input event handlers set `dirty = true`
- Terminal resize sets `dirty = true`
- Status transitions set `dirty = true`
- Message set/expiry sets `dirty = true`
- Remote snapshot changes only set `dirty = true` when meaningful fields changed (using `same_view()` method)

**Tests pass (all 8):**
- `dirty_flag_set_on_input`: input event → dirty flag set
- `dirty_flag_set_on_resize`: terminal resize → dirty flag set
- `dirty_flag_cleared_after_draw`: draw clears dirty flag
- `idle_zero_draws_after_first_frame`: after first draw, idle has no redraws
- `generation_counter_detects_pty_output`: PTY output increments generation counter
- `generation_counter_marks_dirty_once`: generation change sets dirty once per change (not every tick)
- `idle_no_dirty_after_first_frame`: dirty stays false after first frame with no input
- `restore_does_not_start_before_first_frame`: restore blocked until first frame drawn

## Task 2: Timing Facility for Startup Diagnostics

### Status: COMPLETE

**What was built:**
- `TimingStage` struct with name, duration_ms, and optional note
- `StartupTiming` struct with vec of stages and total_ms
- `StartupTiming::print_to_stderr()` method that outputs:
  - Summary line: "baude startup: config_load=NN workspace_resolution=MM ..."
  - Per-stage lines: "  config_load: NN ms" or "  keyboard_probe: NN ms (kitty 250ms (timeout))"
  - Total line: "  total: NN ms"
- Main() records timing stages at:
  - Stage 1: config load (using baude_core::persist::load_config())
  - Stage 2: workspace resolution (start_workspace call)
  - Stage 3: terminal setup (enable_raw_mode + alternate screen)
  - Stage 4: keyboard probe (negotiate_keyboard + potential timeout note)
  - Stage 5: app initialization (App::new())
- run() records:
  - Stage 6: first frame (when Stepped.drew first becomes true)
- Timing output is printed to stderr ONLY when `BAUDE_TIMING=1` env var is set or `--timing` flag is passed
- --timing flag is stripped from argv before parsing to avoid confusion with launch dir
- Timing output is printed AFTER `restore_terminal()` at program exit

**Tests pass (all 5):**
- `timing_stages_recorded`: timing infrastructure records stages
- `timing_output_format`: output format is correct (summary + per-stage detail)
- `timing_disabled_when_env_unset`: no output when BAUDE_TIMING not set
- `timing_keyboard_probe_stage_includes_timeout_note`: keyboard probe timeout is noted
- `timing_first_frame_before_restore`: stages are recorded in order

**Output example:**
```
baude startup: config_load=5 workspace_resolution=12 terminal_setup=8 keyboard_probe=3 app_new=15 first_frame=22
  config_load: 5 ms
  workspace_resolution: 12 ms
  terminal_setup: 8 ms
  keyboard_probe: 3 ms
  app_new: 15 ms
  first_frame: 22 ms
  total: 65 ms
```

## Implementation Notes

### Dirty Flag Gating
- Eliminates ~20 Hz unconditional redraw that was burning CPU/battery
- First frame always draws (even with no input) to render sidebar chrome and title
- Subsequent frames draw only when dirty flag is set
- PTY output (session screen generation change) automatically marks dirty via tick()
- Waiting rows refresh at 1 Hz only when at least one visible session is Waiting
- Remote snapshot polling only marks dirty when user-visible fields changed (not timestamp-only)

### Generation Counter Design
- Arc<AtomicU64> on Pty (not Session) ensures thread-safe updates by reader thread
- Reader thread increments with Relaxed ordering for minimal overhead
- Session accessor loads with Relaxed ordering, no synchronization needed
- Per-session tracking map in App stores last-known value to detect changes in tick()
- Polling happens every tick (50 ms), change detection is instant within one tick

### Timing Facility Design
- Instant-based, no external time source dependencies
- Stages captured at strategic points: load → workspace → terminal → keyboard → app → frame
- Keyboard probe timeout detection uses >= 250ms heuristic (crossterm's internal deadline)
- Timing output goes to stderr (separate from stdout/TUI) to avoid interfering with logging
- BAUDE_TIMING=1 env var or --timing flag enables output (off by default)
- Total duration measured from program start (total_start) to exit

### First Frame Ordering Guarantee
- first_frame_drawn flag blocks restore_progress processing
- app.restore() only called AFTER first draw completes (in run() main loop iteration 2+)
- First frame renders: sidebar chrome, workspace title, "Restoring..." status message
- No session content appears until restore is unblocked

## Orchestrator Post-Wave Notes (2026-09-21)

The first two executors left the timing facility unwired in main() — the structs and tests existed, but no instrumentation was recording stages or printing output. This completion:

1. Wired BAUDE_TIMING env var and --timing flag detection at main() entry
2. Added Instant-based checkpoint recording at each startup stage
3. Modified run() signature to accept timing and record first-frame stage
4. Added timing output printing after restore_terminal() when enabled
5. All tests pass (13/13 specified tests), all gates green

**Gates (exit codes):**
- `cargo fmt --all -- --check`: 0
- `cargo clippy --all-targets -- -D warnings`: 0
- `cargo build --workspace`: 0
- `cargo test --workspace`: 0 (730 passed / 0 failed baseline maintained)

**Verify commands pass:**
```
cargo test -p baude --bin baude -- --nocapture dirty_flag_set_on_input dirty_flag_set_on_resize dirty_flag_cleared_after_draw idle_zero_draws generation_counter_detects_pty_output generation_counter_marks_dirty_once idle_no_dirty_after_first_frame restore_does_not_start_before_first_frame
test result: ok. 8 passed; 0 failed

cargo test -p baude --bin baude -- --nocapture timing_stages_recorded timing_output_format timing_disabled_when_env_unset timing_keyboard_probe_stage_includes_timeout_note timing_first_frame_before_restore
test result: ok. 5 passed; 0 failed
```

**Next phase:** Task 15-02 will bind session_restore and first_metadata_poll stages to events inside app.tick(), refining timing precision and adding per-session restore counts to notes.
