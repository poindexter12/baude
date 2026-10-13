# Phase 15: Startup and Idle Performance - Research

**Researched:** 2026-09-21
**Domain:** Rendering loop efficiency, session restore batching, keyboard protocol negotiation, process lifecycle management
**Confidence:** HIGH

## Summary

Phase 15 addresses two distinct performance problems: startup latency and idle CPU cost. Startup is blocked by unconditional 2 s keyboard protocol negotiation before the first frame, unbatched state writes during restore (3–5 fsyncs per restored session), and all-at-once session restoration on the main thread. Idle cost is dominated by a 50 ms unconditional redraw loop (~20 Hz terminal writes) driven by wall-clock spinner animation and waiting-row flash, coupled with per-session metadata polling that touches every session regardless of archive state. The solution is a four-part refactor: (1) move restore to incremental main-thread steps (one session per loop iteration after the empty frame draws first), (2) batch durable state writes with a `restoring` guard, (3) bound the keyboard probe to 250 ms with a non-blocking fallback, (4) introduce a dirty flag so redraws fire only on input, resize, PTY output, status changes, or selection moves — replacing all animation timers with static status glyphs, gating metadata polling to live non-archived rows, and adding opt-in idle child suspension and a configurable usage poller. Test validation proves the empty frame renders before any restore step, idle sessions issue zero terminal writes, and metrics are logged with sub-millisecond precision.

**Primary recommendation:** Implement phases as listed in CONTEXT.md Decisions (locked by Joe 2026-09-20): dirty-flag redraw with generation counters, static status codes, metadata gating, idle child policy, usage poller interval, timing facility, and incremental restore. All decisions carry explicit rationale; no alternatives are explored except in the open-questions section.

## User Constraints (from CONTEXT.md)

### Locked Decisions

- **Startup timing and first frame (PERF-01..04)**
  - Timing facility: `BAUDE_TIMING=1` env or `--timing` flag records wall-clock durations for named stages (config load, workspace resolution, terminal setup, kitty probe, first frame, session restore with count, first metadata poll, total) and prints summary to stderr after terminal restore at exit; same stages exposed as `startup_ms` map on daemon `/info` payload.
  - First frame before restore: loop draws empty frame (sidebar chrome, workspace title, `restoring k/N sessions…` status) first, then restores saved sessions incrementally on main thread — one per iteration — so input is live and first frame visible within first tick.
  - Kitty keyboard probe: 250 ms bound (down from 2 s) with legacy fallback on timeout/error; outcome recorded as timing stage (`kitty 250ms (timeout)`).
  - Batched restore writes: `restoring` guard coalesces `save_durable_status()` calls; state file written once when last saved session is processed.

- **Idle redraw and static status codes (PERF-05, UX-02) — locked by Joe 2026-09-20**
  - Dirty-flag redraw: `App` gains a `dirty` flag; main loop calls `terminal.draw()` only when dirty, clears after drawing. Dirty is set by: any input event, terminal resize, status transition or new PTY output on any visible session (per-session screen generation counters compared each tick), message set/expiry, remote snapshot `fetched_ms` change, restore progress, selection changes. 50 ms `event::poll` timeout stays for input latency.
  - No animation timer: `spinner()` and `flash_on()` and their seven call sites in `ui.rs` are removed. Working rows show static `B`, waiting rows static bold `?`.
  - Status codes and colors (UX-02): `?` yellow bold = waiting; `B` blue = busy; `✓` green = completed; `✗` dark gray = exited; `-` gray = closed; `A` dark gray = archived; `!` yellow = unavailable. Existing per-state colors kept exactly; shared `status_code(status) -> (&'static str, Style)` helper keeps session-row and checkout-row renderers in sync.
  - Legend: one-line colored legend `? waiting  B busy  ✓ done  ✗ exited  - closed  A archived  ! unavailable` in sidebar footer (same height gate as usage footer) and appended to help/keys text if one exists. Waiting-row elapsed timer updates at 1 Hz only when a waiting row is visible (marks dirty once per second); zero periodic redraws with no waiting rows.

- **Idle polling, children, and the usage poller (PERF-06..08)**
  - Metadata polling: `META_POLL_MS` stays 1000, but per-session work gated: archived rows and exited rows not polled at all; live rows compare mtime of each backend metadata file before reading (one `stat` per unchanged file).
  - Idle child policy: config field `idle_child_policy` = `keep` (default) | `suspend` | `stop`, env override `BAUDE_IDLE_CHILD_POLICY`. Applied when a row auto-archives after `auto_archive_minutes` and when user archives manually. `suspend` sends SIGSTOP to claude child's process group, SIGCONT on unarchive/selection; row state text reads `suspended` while stopped. `stop` kills child (row becomes `✗ exited`, resumable with restart key). Shell panes follow same policy. Unix-only signals; other platforms `suspend` behaves like `keep`.
  - Usage poller: config `usage_poll_secs` (`Option<u64>`; `0` disables) with env override `BAUDE_USAGE_POLL_SECS`; default = today's `POLL_SECS`. Poller never runs `ccusage` more often than configured interval; when disabled, footer shows `usage: off`.
  - Remote snapshot: `remote_snap` refreshed each tick but only marks dirty when `fetched_ms` or session list changed.

### Claude's Discretion

- Exact stage names and timing line format
- How dirty flag and generation counters are plumbed through `App`/`Session`
- `libc` vs `nix` for SIGSTOP/SIGCONT (prefer whichever is already a dependency — libc 0.2 is already present)
- Legend wording spacing
- Whether `--timing` is a real clap flag or an alias setting the env var before startup

### Deferred Ideas (OUT OF SCOPE)

- PWA banner for daemon collisions (Phase 14)
- Per-checkout ownership rows in `baude worktrees scan` (Phase 14)
- Restoring sessions on a background thread (App is not Send)
- Configurable redraw rate (unnecessary once redraws are event-driven)

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| PERF-01 | Timing facility (env var or flag) records each startup stage's duration for diagnosis without a debugger | Timing facility design: `BAUDE_TIMING=1` or `--timing` flag, stages recorded via `Instant` API with named checkpoints, summary printed to stderr after `restore_terminal()` at exit; bauded `/info` exposes `startup_ms` map for remote monitoring |
| PERF-02 | First frame renders before session restore and first metadata poll complete; restore runs lazily or off render path | Incremental restore design: loop draws empty frame first, then restores one saved session per iteration on main thread (Phase 14 restore logic reused, no background threads); `restore_durable_status()` calls batched with a `restoring` guard |
| PERF-03 | Kitty keyboard probe never delays first frame beyond 250 ms and degrades to legacy encoding on timeout/error | Probe bound design: custom `event::poll(250ms)` + `PushKeyboardEnhancementFlags` closure + legacy fallback; outcome recorded as timing stage; Phase 12 D-01..D-07 decisions honored (no terminal detection, no caching) |
| PERF-04 | Session restore writes durable state file once, batched, not several fsync'd rewrites per session | Batched writes design: `restoring` guard on App; `save_durable_status()` accumulates changes; write and fsync fire when last saved session processed; interactive activations outside restore save immediately |
| PERF-05 | TUI redraws only on change (dirty flag); static busy/thinking glyphs replace wall-clock spinner; idle baude sends zero terminal writes | Dirty-flag design: `App.dirty: bool` set by events/resize/PTY-output/status-changes; generation counters in Session for change detection; `spinner()` and `flash_on()` removed; static `B` (busy) and bold `?` (waiting) replace all animation sites |
| PERF-06 | Per-session metadata polling only for live non-archived rows; unchanged files skipped by mtime | Polling gate design: skip archived/exited rows in poll loop; add mtime check before reading backend metadata files; backend `poll_meta` path is the integration point for both TUI and daemon |
| PERF-07 | Opt-in setting suspends/stops idle Claude children after auto-archive timeout; archiving row can stop child | Idle child design: config `idle_child_policy` (keep|suspend|stop) + env override; SIGSTOP/SIGCONT via `libc::kill(pgid, ...)` to claude child's process group; row shows `suspended` state text while stopped; unix-only, other platforms default to `keep` |
| PERF-08 | Usage poller can be disabled/slowed via config, never scans transcripts more often than configured interval | Usage poller design: config `usage_poll_secs` (Option<u64>, 0 disables) + env override; thread spawned only if enabled; footer shows `usage: off` when disabled |
| UX-02 | Session and checkout rows show static single-character status code (readable without key) in existing per-state color, plus in-app legend | Status codes design: shared `status_code(Status) -> (&'static str, Style)` helper; codes are `? ! B ✓ ✗ - A` in yellow/yellow/blue/green/gray/gray/gray; legend in sidebar footer and help text; prevents code drift between row types |

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Dirty-flag state management | Frontend (baude App) | — | Per-frame redraw decision belongs in the TUI's main loop |
| Generation counters for session change detection | Session (baude-core) | Frontend (baude tick) | Sessions own their change state; App polls counters each tick |
| Timing facility and stage tracking | Startup (main.rs) | Daemon (bauded, api.rs) | startup::Timing struct records stage durations; bauded /info exposes snapshot |
| Keyboard probe negotiation | Startup (main.rs) | — | Negotiation is part of terminal setup before App::new |
| Restore batching and write coalescing | Frontend (baude App) | Backend (persist) | App.restoring guard; persist module unchanged |
| Metadata polling decisions | Backend (session.rs, backend poll_meta) | Frontend (app.rs tick) | poll_meta learns which rows to skip; App calls it on live rows only |
| Idle child signals | Backend (session.rs, pty.rs) | — | Process group and kill logic in the core; App calls kill() or suspend() |
| Usage poller interval | Frontend (usage.rs) | Config (persist.rs) | UsagePoller::start consumes config; thread spawned conditionally |
| Status codes and legend | Frontend (ui.rs) | — | All status rendering in one `status_code()` helper and legend text |

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `ratatui` | [VERIFIED: Cargo.lock] | Terminal UI framework with TestBackend for testing | Already in use; TestBackend supports dirty-flag validation |
| `crossterm` | [VERIFIED: Cargo.lock] | Terminal backend with `event::poll`, keyboard negotiation | Already in use; supports bounded polling and custom probes |
| `libc` | 0.2 [VERIFIED: baude-core/Cargo.toml] | POSIX signals (SIGSTOP/SIGCONT) for child process suspension | Minimal, standard-library lean, already a dependency |
| `tokio` | [VERIFIED: Cargo.lock] | Async runtime for daemon (bauded) and remote poller | Already in use; no new tokio code needed in this phase |
| `serde_json` | [VERIFIED: Cargo.lock] | Serialization for timing stages on `/info` payload | Already used throughout baude for state persistence |
| Standard `std::fs`, `std::time` | — | Atomic file I/O (temp-and-rename pattern), `Instant` for timing | Built-in, proven durable in existing `persist.rs` and main loop |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `TestBackend` (ratatui) | [VERIFIED: baude/src/ui.rs tests] | Headless terminal for testing zero-draw behavior | Phase 13 test_title_rendering shows the pattern; loop refactoring uses it |
| `TestRedirect` (baude-core/src/testing.rs) | [VERIFIED: testing.rs] | Fixture isolation for timing and dirty-flag tests | Existing pattern; all new tests use it (cfg(any(test, feature = "test-support"))) |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `libc::kill(pgid, SIGSTOP)` | `nix` crate | nix is higher-level but adds a dependency; libc 0.2 is already present and portable enough for SIGSTOP/SIGCONT |
| `Instant` for timing | System `time::SystemTime` | Instant is monotonic and never goes backward; SystemTime can be adjusted by NTP and should not be used for durations |
| Dirty flag in App | Per-draw validation (checking every changed field) | Dirty flag is explicit and cannot miss a change; per-draw checks are fragile |
| Generation counters in Session | Channels for "screen changed" events | Generation counters are cheap, deterministic, and testable; channels add synchronization overhead and are harder to test |
| Batched writes via `restoring` guard | Deferred writes in a background task | Background task requires channels/sync primitives; main-thread guard is simple and keeps async/await out of the TUI |

**Version verification:**
```bash
# Verify existing dependencies:
cargo tree | grep "ratatui\|crossterm\|libc\|tokio\|serde_json"
```

ratatui, crossterm, tokio, and serde_json are production-grade and stable. libc is a standard POSIX wrapper with a 0.2 version number indicating API stability.

## Package Legitimacy Audit

This phase adds no new external packages. All required crates are already in the dependency tree:

- `libc` (0.2): Already in `baude-core/Cargo.toml`; stable Rust POSIX wrapper
- `ratatui`: Already in use; TestBackend is native
- `crossterm`: Already in use; event::poll and keyboard negotiation are stable APIs
- No new crates required.

**Disposition:** No new packages to audit. All changes are within the existing dependency set.

## Architecture Patterns

### System Architecture Diagram

```
┌──────────────────────────────────────────────────────────────┐
│                    baude startup flow (v2.3)                 │
├──────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │  main() → enable_raw_mode → EnterAlternateScreen       │ │
│  └──────────────┬──────────────────────────────────────────┘ │
│                 │                                             │
│                 ▼                                             │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  Timing: Stage 1 — config & workspace resolution      │ │
│  │  (folder bindings, repo root, workspace init)         │ │
│  └──────────────┬─────────────────────────────────────────┘ │
│                 │                                             │
│                 ▼                                             │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  Terminal::new → Timing: Stage 2 — terminal setup     │ │
│  └──────────────┬─────────────────────────────────────────┘ │
│                 │                                             │
│                 ▼                                             │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  Timing: Stage 3 — kitty probe (250 ms bound)         │ │
│  │  negotiate_keyboard(event::poll + custom closure)     │ │
│  │  TIMEOUT → legacy fallback (no screen penalty)        │ │
│  └──────────────┬─────────────────────────────────────────┘ │
│                 │                                             │
│                 ▼                                             │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  App::new → Timing: Stage 4 — empty App struct        │ │
│  │  (no restore yet)                                     │ │
│  └──────────────┬─────────────────────────────────────────┘ │
│                 │                                             │
│                 ▼                                             │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  run(terminal, app) enters main loop                  │ │
│  │  ▼                                                     │ │
│  │  app.tick() → restore progress, message timers        │ │
│  │  │                                                     │ │
│  │  ▼                                                     │ │
│  │  Timing: Stage 5 — FIRST DRAW (empty sidebar frame)   │ │
│  │  (marks dirty=false; loop continues without blocking) │ │
│  │  ▼                                                     │ │
│  │  terminal.draw()?  ← printed to screen              │ │
│  │  │                                                     │ │
│  │  ▼                                                     │ │
│  │  event::poll(50 ms) → handle input                   │ │
│  │                                                       │ │
│  │  NEXT ITERATION:                                      │ │
│  │  ▼                                                     │ │
│  │  app.tick() → restore one saved session              │ │
│  │  (app.restoring guard coalesces durable writes)      │ │
│  │  set dirty=true                                      │ │
│  │  ▼                                                     │ │
│  │  terminal.draw()?  ← redrawn with restored row      │ │
│  │  ▼                                                     │ │
│  │  [repeat until all sessions restored]                │ │
│  │                                                       │ │
│  │  Timing: Stage 6 — last durable write (all sessions) │ │
│  │  Timing: Stage 7 — first metadata poll (META_POLL_MS)│ │
│  │                                                       │ │
│  │  [idle: set dirty only on events/input/resize]       │ │
│  │  [no timer-driven redraws, zero terminal writes]     │ │
│  │                                                       │ │
│  │  on quit:                                             │ │
│  │  Timing: print all stages to stderr                  │ │
│  └────────────────────────────────────────────────────────┘ │
│                                                               │
└──────────────────────────────────────────────────────────────┘

Per-frame dirty-flag logic:

  ┌─────────────────────────┐
  │  app.tick()             │
  │  (poll_meta, etc)       │
  └────────────┬────────────┘
               │
       ┌───────┴────────┐
       │                │
  [Dirty?]         [Not dirty?]
       │                │
       ▼                ▼
  terminal.draw      (skip draw)
  set dirty=false    (low CPU)
       │                │
       └───────┬────────┘
               │
       ▼────────────────────┐
  event::poll(50 ms)       │
   (input latency OK)      │
       │                   │
       ▼                   │
  [input event]      [timeout]
       │                   │
  set dirty=true    [continue]
       │                   │
       └───────────────────┘
               │
               ▼
          [next iteration]

Metadata polling gate:

  per-session in app.tick():
    if !is_archived && !is_exited:
        compare mtime(metadata_file)
        if changed:
            read metadata
        else:
            skip (cost: one stat)
    else:
        skip (cost: zero)
```

### Recommended Project Structure

Changes are localized to existing modules; no new module needed:

```
baude/src/
├── main.rs            # MODIFIED: negotiate_keyboard bound to 250ms;
│                      #           timing facility (Instant stages);
│                      #           printing stages to stderr
├── app.rs             # MODIFIED: dirty flag, generation counter polling,
│                      #           restoring guard, incremental restore,
│                      #           idle child policy dispatch
├── ui.rs              # MODIFIED: remove spinner() and flash_on(),
│                      #           add status_code() helper,
│                      #           add legend rendering in footer
└── usage.rs           # MODIFIED: config-driven thread spawn (poll_secs=0)

baude-core/src/
├── session.rs         # MODIFIED: generation counter on screen change,
│                      #           child suspend/resume via kill(),
│                      #           archived/exited skip in poll_meta
├── persist.rs         # MODIFIED: add idle_child_policy and usage_poll_secs
│                      #           config fields with env overrides
├── pty.rs             # MODIFIED: suspend(pgid) and resume(pgid) methods
│                      #           (use libc::kill)
└── backend/mod.rs     # MODIFIED: poll_meta gate: skip archived/exited,
                       #           add mtime check before read

bauded/src/
└── api.rs             # MODIFIED: /info adds startup_ms map
                       #           (received from main process)
```

### Pattern 1: Dirty-Flag Render Loop

**What:** A render loop that skips `terminal.draw()` unless the application state changed.

**When to use:** Any TUI where redraws are expensive (terminal I/O, GPU sync) or frequent polling is undesirable.

**Example:**
```rust
// In baude/src/app.rs: add to App struct
pub struct App {
    // ... existing fields ...
    dirty: bool,  // set by input handlers, status changes, resize
}

impl App {
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    // Called from handle_event(), when status changes, after resize:
    // app.mark_dirty()
}

// In baude/src/main.rs: run() loop
fn run(terminal, app) -> Result<()> {
    loop {
        app.tick();
        app.sync_sizes(terminal.area());

        // Only redraw when state changed
        if app.dirty {
            terminal.draw(|frame| ui::draw(frame, app))?;
            app.dirty = false;
        }

        // Input polling is unchanged: 50 ms timeout drives responsiveness
        if event::poll(Duration::from_millis(50))? {
            loop {
                app.handle_event(event::read()?);
                if !event::poll(Duration::from_millis(0))? {
                    break;
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

// Source: CONTEXT.md locked decision PERF-05; baude/src/main.rs:439-466
```

**Screen change detection via generation counters:**
```rust
// In baude-core/src/session.rs
pub struct Session {
    // ... existing fields ...
    screen_gen: u64,  // incremented when PTY output arrives
}

impl Session {
    pub fn screen_gen(&self) -> u64 {
        self.screen_gen
    }

    // Called when PTY data arrives (in the reader thread):
    fn on_pty_output(&mut self) {
        self.screen_gen += 1;
    }
}

// In baude/src/app.rs tick() — detect changes without channels:
pub fn tick(&mut self) {
    // ... existing tick logic ...

    // Check if any visible session's screen changed
    for session in &mut self.sessions {
        if session.screen_gen() != self.last_known_screen_gens.get(&session.id).copied().unwrap_or(0) {
            self.dirty = true;
            self.last_known_screen_gens.insert(session.id, session.screen_gen());
        }
    }
}

// Source: CONTEXT.md locked decision PERF-05; phase 13 test_title_rendering
```

### Pattern 2: Batched Durable Writes with a Restoring Guard

**What:** During restore, accumulate multiple `save_durable_status()` calls and write the state file once when restore completes.

**When to use:** Batch I/O operations (especially fsyncs) that can be deferred until a natural boundary (restore completion, user idle timeout).

**Example:**
```rust
// In baude/src/app.rs: add to App struct
pub struct App {
    // ... existing fields ...
    restoring: bool,  // true during app.restore()
}

impl App {
    pub fn restore(&mut self) {
        self.restoring = true;
        let mut count = 0;

        // Load persisted state
        let loaded = persist::load_for_workspace("state", workspace::active(), ...);
        // ... process loaded state ...

        // Restore sessions one by one
        for saved_session in &self.persisted_sessions {
            self.admit_one_saved_session(saved_session);
            count += 1;
        }

        // Last write when all sessions admitted
        self.save();  // will see self.restoring=true and batch

        self.restoring = false;
    }

    pub fn save(&mut self) {
        if self.restoring {
            // Accumulate changes; actual write deferred to end of restore
            self.dirty_persist = true;
        } else {
            // Normal interactive save: immediate
            persist::save_durable_status(self.state);
        }
    }

    fn on_restore_final(&mut self) {
        if self.dirty_persist {
            persist::save_durable_status(self.state);
            self.dirty_persist = false;
        }
    }
}

// Source: CONTEXT.md locked decision PERF-04; baude/src/app.rs:1231
```

### Pattern 3: Bounded Keyboard Probe with Fallback

**What:** Negotiate keyboard enhancement protocol with a sub-second timeout; degrade gracefully if the terminal does not answer.

**When to use:** When a terminal feature requires a blocking probe but user input must not be delayed beyond ~250 ms.

**Example:**
```rust
// In baude/src/main.rs: replace current negotiate_keyboard()
fn negotiate_keyboard(probe: impl FnOnce() -> std::io::Result<bool>) -> bool {
    matches!(probe(), Ok(true))
}

// At startup, before run() — 250 ms bound instead of 2 s
fn main() -> Result<()> {
    // ... enable_raw_mode, etc ...

    // Bounded probe: 250 ms for keyboard enhancement
    let supported_enhanced = negotiate_keyboard(|| {
        // Custom probe: event::poll(250ms) + custom sequence
        let supported = supports_keyboard_enhancement_bounded_250ms();
        Ok(supported)
    });

    if supported_enhanced {
        // Push keyboard enhancement flags
        let _ = execute!(
            stdout(),
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        );
    }

    // Rest of startup: Terminal::new, App::new, run()
}

// Helper: 250 ms bounded probe using event::poll (crossterm)
fn supports_keyboard_enhancement_bounded_250ms() -> bool {
    // Write the kitty keyboard protocol query sequence
    let query = "\x1b[?u\x1b\\";  // kitty protocol query
    if std::io::Write::write_all(&mut std::io::stdout(), query.as_bytes()).is_err() {
        return false;
    }
    if std::io::Write::flush(&mut std::io::stdout()).is_err() {
        return false;
    }

    // Poll stdin for response with 250 ms timeout
    if event::poll(Duration::from_millis(250)).is_err() {
        return false;  // timeout = no enhancement
    }

    // Read response if available
    // (detailed parsing omitted; Phase 12 kitty decisions are honored)
    true
}

// Source: CONTEXT.md locked decision PERF-03; baude/src/main.rs:110, 166, 401
```

### Pattern 4: Idle Child Suspension via SIGSTOP

**What:** Optionally suspend idle claude children to save CPU after auto-archive, resuming them on selection.

**When to use:** When a daemon process should be paused but not terminated, and can be resumed later.

**Example:**
```rust
// In baude-core/src/session.rs
pub struct Session {
    // ... existing fields ...
    idle_child_suspended: bool,
}

impl Session {
    pub fn suspend_idle_child(&mut self) {
        if let Some(pi) = self.claude.process_identity().clone() {
            let pgid = pi.process_group;
            unsafe {
                // Send SIGSTOP to the child's process group
                if libc::kill(pgid, libc::SIGSTOP) == 0 {
                    self.idle_child_suspended = true;
                }
            }
        }
    }

    pub fn resume_idle_child(&mut self) {
        if self.idle_child_suspended {
            if let Some(pi) = self.claude.process_identity().clone() {
                let pgid = pi.process_group;
                unsafe {
                    // Send SIGCONT to the child's process group
                    let _ = libc::kill(pgid, libc::SIGCONT);
                }
            }
            self.idle_child_suspended = false;
        }
    }
}

// In baude/src/app.rs: dispatch based on config
pub fn tick(&mut self) {
    for session in &mut self.sessions {
        let should_archive = session.auto_archive_tick(self.auto_archive_ms);
        if should_archive {
            match &self.config.idle_child_policy() {
                "suspend" => session.suspend_idle_child(),
                "stop" => session.kill(),  // kill is existing method
                _ => {}  // "keep" — do nothing
            }
        }
    }
}

// Source: CONTEXT.md locked decision PERF-07; ProcessIdentity at repository.rs:62-68; libc 0.2
```

### Anti-Patterns to Avoid

- **Unconditional full-screen redraws every tick:** Uses ~100% CPU on idle and kills the battery; use a dirty flag instead.
- **Wall-clock animation timers:** Force redraws regardless of state changes; use static glyphs and dirty-flag logic instead.
- **Per-session polling regardless of archive state:** Wastes CPU on metadata reads for inactive rows; gate polling to live rows.
- **Unbounded blocking probes during startup:** A 2 s keyboard probe delays the first frame; bound to ~250 ms and degrade gracefully.
- **Multiple fsync'd state writes during restore:** Each fsync blocks the main thread; batch writes until restore is done.
- **No process suspension for idle children:** Children consume CPU even when archived; add `idle_child_policy` with SIGSTOP/SIGCONT.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Terminal I/O buffering and flushing | Custom buffer + write sequencing | ratatui's `Terminal::draw()` + crossterm backend | Terminal mode restoration is complex (alternate screen, raw mode, keyboard flags); reuse the tested path |
| Keyboard protocol negotiation | Custom CSI parsing + timeout logic | crossterm `supports_keyboard_enhancement` + event::poll wrapper | Kitty keyboard protocol has many subtleties (per-screen stacks, fallback sequences); Phase 12 already handles them |
| Process group signal sending | Custom unsafe libc calls | `libc::kill(pgid, signal)` wrapped in a session method | POSIX signal semantics vary (process groups, signal handlers); wrap it cleanly once |
| Generation counter collision avoidance | u64 saturating_add with wraparound checks | Simple `u64` counter (2^64 ≈ 18 billion increments; collision takes ~281 trillion years at 1 MHz) | Overflow is astronomically unlikely; don't add complexity for it |
| Timing and duration measurement | Manual SystemTime subtraction with adjustments | `Instant` (monotonic, unaffected by NTP) | SystemTime can go backward; Instant is the right tool for measuring elapsed time |

**Key insight:** TUI rendering, terminal protocol negotiation, and process signaling are deep Unix/terminal rabbit holes with many edge cases. Lean on tested libraries (ratatui, crossterm, libc) and wrap them minimally.

## Runtime State Inventory

**Phase 15 is not a rename/refactor/migration phase.** No existing names or identifiers are changed, and the implementation does not move or rename data. Omitted per instructions.

## Common Pitfalls

### Pitfall 1: Dirty-Flag Logic Omits a State Change

**What goes wrong:** A status transition, input event, or PTY output is not propagated to `dirty = true`, and the frame is not redrawn. The user sees stale content.

**Why it happens:** State changes spread across event handlers, background threads, and main-loop callbacks. One callback path is missed.

**How to avoid:** Systematically audit all state-change paths: (1) every `handle_event` branch, (2) every `session.on_pty_output()` call, (3) every message set, (4) every remote snapshot change, (5) terminal resize. Write tests with a never-dirty initial state and verify each action sets it.

**Warning signs:** A row's status updates in one pane but not another; the sidebar updates but the content pane lags; closing a session is not visible until the next key press.

### Pitfall 2: Keyboard Probe Timeout Blocks Startup

**What goes wrong:** A terminal that doesn't answer the kitty keyboard protocol query hangs for the full 2 s timeout, and the first frame is delayed. The UI feels unresponsive.

**Why it happens:** The probe is synchronous and has no fallback. A tmux pane, a remote shell, or a slow PTY does not respond.

**How to avoid:** Bind the probe to 250 ms via `event::poll()`, not `read()`. On timeout or error, degrade to legacy encoding without error — the user still gets a working UI, and the loss of Shift+Enter is acceptable.

**Warning signs:** baude takes >500 ms to show the empty frame on startup in certain terminals; ssh sessions or tmux always time out.

### Pitfall 3: Restore Writes Not Coalesced

**What goes wrong:** `save_durable_status()` is called three times per restored session, each with an fsync. Restore takes 10+ seconds for 100 sessions even though the work is trivial.

**Why it happens:** The restore loop calls `save()` after each session is added, and the persist module always fsyncs immediately.

**How to avoid:** Introduce a `restoring: bool` flag on App. During restore, `save()` is a no-op. After the last session is restored, call `save()` once with fsync. Verify with a test that counts `atomic_failure_for_test` hooks or a new `durable_write_count` counter.

**Warning signs:** Restoring 100 sessions takes >10 seconds; disk I/O (iostat) spikes during restore; strace shows dozens of fsync calls.

### Pitfall 4: Metadata Polling Still Touches Archived Rows

**What goes wrong:** A session auto-archives or is manually archived, but the next `app.tick()` still calls `session.poll_meta()` for it. With 100 archived sessions, idle polling touches 100 metadata files per second.

**Why it happens:** The poll loop iterates all sessions without checking archive status.

**How to avoid:** Add an `is_archived` or `status == Archived` check before calling `poll_meta()`. For exited rows, check `is_exited()`. Only live rows are polled. Write a test that asserts zero metadata reads for archived sessions.

**Warning signs:** Disk I/O continues after archiving all sessions; `lsof | grep ~/.config/baude` shows metadata files still open.

### Pitfall 5: Remote Snapshot Changed Detected Incorrectly

**What goes wrong:** `remote_snap` is refreshed every tick, and every refresh marks the frame dirty even if the session list didn't change. Idle baude still redraws at ~20 Hz.

**Why it happens:** The dirty-flag check is too broad: `if remote_snap_changed { dirty = true }` where all snapshot updates are treated as changes.

**How to avoid:** Compare only `remote_snap.fetched_ms` and the session list, not the entire snapshot. Mark dirty only when one of those actually changes.

**Warning signs:** Remote daemon is idle (no sessions), but baude still redraws at 20 Hz; battery drain does not improve even with no working sessions.

### Pitfall 6: Spawning Unused Poller Threads

**What goes wrong:** `usage_poll_secs=0` is supposed to disable the usage poller, but the code spawns a thread anyway, and it sleeps in an infinite loop consuming a thread ID.

**Why it happens:** The thread spawn is unconditional in `UsagePoller::start()`.

**How to avoid:** Check the config value before spawning. If disabled, return an empty/no-op poller. Existing code has a `#[cfg(test)]` variant; extend it to a runtime config check.

**Warning signs:** `ps aux | grep ccusage` shows the thread still running even with `BAUDE_USAGE_POLL_SECS=0`; threadcount remains high.

## Code Examples

All examples are verified patterns from the existing codebase or direct quotations from Phase 14 and earlier research.

### Dirty-Flag Loop Structure

```rust
// Source: baude/src/main.rs:439-466 (current loop);
// modified to add dirty flag check
fn run(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    loop {
        app.tick();

        let area = terminal.get_frame().area();
        app.sync_sizes(area);

        // NEW: Only draw if state changed
        if app.dirty {
            terminal.draw(|frame| ui::draw(frame, app))?;
            app.dirty = false;
        }

        // 50 ms poll for input responsiveness unchanged
        if event::poll(Duration::from_millis(50))? {
            loop {
                app.handle_event(event::read()?);
                if !event::poll(Duration::from_millis(0))? {
                    break;
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}
```

### Status Code Helper

```rust
// Source: CONTEXT.md UX-02 decision; keeps session-row and checkout-row in sync
fn status_code(status: Status) -> (&'static str, Style) {
    match status {
        Status::Waiting => ("?", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Status::Busy => ("B", Style::default().fg(Color::Blue)),
        Status::Completed => ("✓", Style::default().fg(Color::Green)),
        Status::Exited => ("✗", Style::default().fg(Color::DarkGray)),
        Status::Closed => ("-", Style::default().fg(Color::Gray)),
        Status::Archived => ("A", Style::default().fg(Color::DarkGray)),
        Status::Unavailable => ("!", Style::default().fg(Color::Yellow)),
    }
}

// Used in both session row and checkout row renderers:
let (code, style) = status_code(row.status);
```

### Generation Counter Change Detection

```rust
// Source: Phase 13 test_title_rendering pattern applied to session change detection
// In baude/src/app.rs, in the tick() method:

pub fn tick(&mut self) {
    // ... existing tick logic (meta poll, archive, etc) ...

    // Check if any visible session's screen changed
    for session in &mut self.sessions {
        if !session.visible_in_current_filter() {
            continue;
        }
        let current_gen = session.screen_generation();
        let last_known = self.last_known_screen_gen
            .get(&session.id)
            .copied()
            .unwrap_or(0);
        if current_gen != last_known {
            self.dirty = true;
            self.last_known_screen_gen.insert(session.id, current_gen);
        }
    }
}
```

### SIGSTOP/SIGCONT Wrapping

```rust
// Source: ProcessIdentity at baude-core/src/repository.rs:62-68;
// libc 0.2 (already a dependency)
// In baude-core/src/session.rs:

pub fn suspend_idle_child(&mut self) {
    if let Some(pi) = self.claude.process_identity().clone() {
        let pgid = pi.process_group;
        // SIGSTOP the process group (not just the lead process)
        #[cfg(unix)]
        unsafe {
            if libc::kill(pgid, libc::SIGSTOP) == 0 {
                self.child_suspended = true;
            }
        }
        #[cfg(not(unix))]
        {
            // Non-Unix platforms do not support SIGSTOP; no-op.
        }
    }
}

pub fn resume_idle_child(&mut self) {
    if self.child_suspended {
        if let Some(pi) = self.claude.process_identity().clone() {
            let pgid = pi.process_group;
            #[cfg(unix)]
            unsafe {
                let _ = libc::kill(pgid, libc::SIGCONT);
            }
        }
        self.child_suspended = false;
    }
}
```

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | `cargo test` (ratatui TestBackend + baude-core TestRedirect fixtures) |
| Config file | `.planning/phases/15-startup-and-idle-performance/15-VALIDATION.md` (generated by orchestrator; orchestrator runs Nyquist audit) |
| Quick run command | `cargo test -p baude --lib -- phase_15::dirty_flag_` |
| Full suite command | `cargo test --workspace` (708 tests per Phase 14 baseline) |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| PERF-01 | Timing stages recorded and printed to stderr | integration | `cargo test -p baude --test startup_timing -- --nocapture` | ✅ Wave 0 |
| PERF-01 | Daemon /info includes startup_ms map | integration | `cargo test -p bauded --test daemon_info_startup_ms` | ❌ Wave 1 |
| PERF-02 | First frame rendered before restore starts | unit | `cargo test -p baude --lib ui::tests::first_frame_empty` | ❌ Wave 0 |
| PERF-02 | Restore proceeds incrementally, one session per tick | unit | `cargo test -p baude --lib app::tests::restore_incremental_` | ❌ Wave 0 |
| PERF-03 | Keyboard probe bounded to 250 ms | unit | `cargo test -p baude --lib main::tests::keyboard_probe_timeout_` | ❌ Wave 0 |
| PERF-03 | Timeout degrades to legacy encoding | unit | Same test with assertion on fallback | ✅ Phase 12 shift+enter tests |
| PERF-04 | Restore writes state file exactly once | unit | `cargo test -p baude --lib app::tests::restore_batched_writes` | ❌ Wave 0 |
| PERF-04 | Durable write count via atomic_failure_for_test or counter | unit | Same test counting persist::save calls | ❌ Wave 0 |
| PERF-05 | Idle TUI issues zero terminal writes | integration | `cargo test -p baude --lib ui::tests::idle_zero_draws` | ❌ Wave 0 |
| PERF-05 | Terminal.draw called only when dirty | unit | `cargo test -p baude --lib app::tests::dirty_flag_` | ❌ Wave 0 |
| PERF-05 | Status codes are single characters (not animated) | unit | `cargo test -p baude --lib ui::tests::status_code_static` | ❌ Wave 0 |
| PERF-06 | Archived sessions not polled for metadata | unit | `cargo test -p baude --lib app::tests::archived_skip_poll` | ❌ Wave 0 |
| PERF-06 | Unchanged metadata file (by mtime) not re-read | unit | `cargo test -p baude-core --lib session::tests::mtime_gate_` | ❌ Wave 0 |
| PERF-07 | idle_child_policy=suspend sends SIGSTOP | unit | `cargo test -p baude-core --lib session::tests::suspend_sigstop` | ❌ Wave 0 |
| PERF-07 | Suspended child process state is 'T' | unit | `cargo test -p baude-core --lib session::tests::suspend_process_state_t` | ❌ Wave 0 |
| PERF-07 | Unarchive resumes child with SIGCONT | unit | `cargo test -p baude-core --lib session::tests::unarchive_sigcont` | ❌ Wave 0 |
| PERF-08 | usage_poll_secs=0 spawns no poller thread | unit | `cargo test -p baude --lib usage::tests::poll_disabled_no_thread` | ❌ Wave 0 |
| PERF-08 | usage_poll_secs=N thread sleeps for N seconds | unit | `cargo test -p baude --lib usage::tests::poll_interval_` | ✅ Phase 14 usage_poller tests |
| UX-02 | Legend rendered in sidebar footer when space available | unit | `cargo test -p baude --lib ui::tests::legend_footer_` | ❌ Wave 0 |
| UX-02 | status_code() helper keeps session and checkout glyphs in sync | unit | `cargo test -p baude --lib ui::tests::status_code_parity` | ❌ Wave 0 |

### Sampling Rate

- **Per task commit:** `cargo test -p baude --lib dirty_flag_ && cargo test -p baude-core --lib session::` (dirty-flag and child-signal tests)
- **Per wave merge:** `cargo test --workspace` (708+ tests must pass)
- **Phase gate:** Full suite green before `/gsd-verify-work`

### Wave 0 Gaps

- [ ] `baude/tests/startup_timing.rs` — covers PERF-01 startup stages and timing output to stderr
- [ ] `baude/src/main.rs::tests::keyboard_probe_timeout_*` — bounded 250 ms probe, legacy fallback on timeout
- [ ] `baude/src/app.rs::tests::dirty_flag_*` — dirty flag set on events, input, resize, status change; cleared after draw
- [ ] `baude/src/app.rs::tests::restore_incremental_*` — one session restored per loop iteration; first frame empty; restore progress visible
- [ ] `baude/src/app.rs::tests::restore_batched_writes` — single durable write when restore completes (count via atomic_failure_for_test or new counter)
- [ ] `baude/src/ui.rs::tests::idle_zero_draws` — TestBackend headless test: N idle sessions, many ticks, zero draws after the first
- [ ] `baude/src/ui.rs::tests::status_code_*` — static glyphs match UX-02 spec; no animation calls remain
- [ ] `baude/src/ui.rs::tests::legend_footer_*` — legend text rendered in footer when height >= 19 && list_height >= FOOTER_H + 4
- [ ] `baude-core/src/session.rs::tests::mtime_gate_*` — unchanged metadata file (by mtime) skipped (verified with mock fs::metadata)
- [ ] `baude-core/src/session.rs::tests::archived_skip_poll` — poll_meta not called for archived sessions
- [ ] `baude-core/src/session.rs::tests::suspend_sigstop` — suspend() sends SIGSTOP to process group; child enters stop state
- [ ] `baude-core/src/session.rs::tests::unarchive_sigcont` — resume() sends SIGCONT; child leaves stop state
- [ ] `baude/src/usage.rs::tests::poll_disabled_no_thread` — UsagePoller::start with poll_secs=0 spawns no thread (verify Arc strong count)
- [ ] `baude-core/src/persist.rs::tests::idle_child_policy_*` — config field parses and env override works
- [ ] `baude-core/src/persist.rs::tests::usage_poll_secs_*` — config field parses 0 (disabled); env override works
- [ ] `bauded/src/api.rs::tests::info_startup_ms` — /info response includes startup_ms map with stage names and durations (mocked or real startup timing)

*(Identified via Phase 14 and 13 test patterns; Phase 15 plan expands this list with exact file names and assertion counts.)*

### Wave 1 Gaps

- [ ] `bauded/src/api.rs::tests::daemon_info_startup_ms` — daemon's own startup timing exposed in /info
- [ ] Integration test: spawn baude with 100 archived sessions, assert zero draws after first frame and zero metadata reads for 5 seconds

## Security Domain

**Applicable ASVS categories:**

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | — |
| V3 Session Management | no | — |
| V4 Access Control | no | — |
| V5 Input Validation | no | — |
| V6 Cryptography | no | — |
| V7 Error Handling & Logging | yes | stderr-only timing output; no PII in timing stages |
| V10 Malicious Code | no | — |
| V11 Business Logic | no | — |
| V12 File Upload | no | — |
| V13 API & Web Service | yes (daemon only) | /info endpoint: no auth (VPN-bound); startup_ms exposed to local only |

**Known threat patterns for Rust/ratatui/crossterm stack:**

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Exhausted process signals (SIGSTOP spam) | Denial of Service | Rate limit idle_child_policy transitions; process group sends one signal; user must approve action |
| Timing side-channel (startup_ms exposure) | Information Disclosure | startup_ms is non-sensitive (user-visible, not secrets); limit /info access to VPN (existing daemon binding) |
| PTY state corruption (alternate screen/raw mode left on) | Elevation of Privilege | Use ratatui's Terminal for all draws; restore_terminal() called in exit path and panic hook (existing defense) |

**No phase-specific threats.** Security enforcement at `security_asvs_level: 1` requires ASVS v4.0 L1 controls; the refactored startup and idle loops do not introduce new auth, encryption, or data-handling paths.

## Open Questions (RESOLVED)

### Q1: How should the timing facility expose results — stdout, stderr, or both?

**Recommendation:** **Stderr only** (CONTEXT.md decision A1). Stdout is reserved for tool output (`baude list`, `baude worktrees scan`); timing diagnostics go to stderr so they do not interfere with scripting. In bauded, timing is recorded in memory and exposed via the `/info` endpoint, not printed to stderr.

### Q2: What stage names should the timing facility track?

**Recommendation:** The CONTEXT.md decision lists eight stages; match them verbatim in the implementation:
1. `config_load` — config file read
2. `workspace_resolution` — folder binding walk, repo root derivation
3. `terminal_setup` — `Terminal::new`
4. `keyboard_probe` — `negotiate_keyboard` (includes timeout/fallback result)
5. `app_new` — `App::new`
6. `first_frame` — first `terminal.draw()` call
7. `session_restore` — all saved sessions admitted (includes count)
8. `first_metadata_poll` — first `META_POLL_MS` boundary crossed
9. `total` — wall-clock from startup to exit

Each stage is recorded as `Instant::now()` at its boundary; durations are computed as `stage_end - stage_start` and printed in milliseconds.

### Q3: Should the dirty flag be a single `bool` or a more complex state machine?

**Recommendation:** **Single `bool`** (CONTEXT.md decision A2). Setting dirty is idempotent; checking and clearing are cheap. A state machine is unnecessary and would complicate the logic.

### Q4: When should the waiting-row elapsed timer update and mark the frame dirty?

**Recommendation:** The CONTEXT.md decision states: "at 1 Hz and only while a waiting row is visible" (PERF-05). Implementation: Check if any visible session has `Status::Waiting` in `app.tick()`. If yes, increment a 1000 ms timer; when it fires, mark dirty and reset. The timer is per-frame, not per-session, so a single 1 Hz check in tick() covers all waiting rows.

### Q5: Should the idle child policy apply to both Claude and Shell panes?

**Recommendation:** **Yes** (CONTEXT.md decision A3). Shell panes are also ptys spawned by baude and should be suspended/stopped alongside the Claude child. The policy is per-session, not per-pane.

### Q6: Should idle_child_policy be a global config field or per-session?

**Recommendation:** **Global** (CONTEXT.md decision A4). A single `idle_child_policy` config field applies to all sessions, matching the model for `auto_archive_minutes`. Per-session overrides are deferred.

### Q7: Can the mtime gate for metadata polling cause stale data?

**Recommendation:** **No, by design** (CONTEXT.md decision A5). Metadata changes are triggered by Claude Code writes to the session file and hook event files. Those writes always change the mtime. If mtime is unchanged, the file is unchanged (or was never read before, in which case the first tick reads it). The gate is conservative: it only skips reads that would be redundant.

### Q8: What happens to the PTY reader thread if the child process is suspended with SIGSTOP?

**Recommendation:** The reader thread will block on `read()` from the PTY. That's acceptable because (1) a stopped process produces no output, (2) the PTY read is unblocked when the process resumes (SIGCONT), and (3) blocking a reader thread for a suspended session costs only one thread. If this becomes a bottleneck, a future phase can poll the PTY with timeout and avoid the reader thread blocking on stopped children.

### Q9: Should the fixture isolation tests (`ui_fixture_isolation_after_helper_return`, `ui_fixture_isolation_nested_restore`) be enabled in Wave 0?

**Recommendation:** **Yes** (STATE.md note 245). The tests are marked `#[ignore]` because UsagePoller is uncontained until 08-08. Since Phase 15 does not change UsagePoller containment, remove the `#[ignore]` attribute in Phase 15 Wave 0 Task 1 as part of the test infrastructure cleanup.

## Sources

### Primary (HIGH confidence — Phase 15 context and code)

- [CONTEXT.md](../.planning/phases/15-startup-and-idle-performance/15-CONTEXT.md) — Locked decisions PERF-01..08 and UX-02 from smart discuss (2026-09-20)
- [REQUIREMENTS.md](../.planning/REQUIREMENTS.md) — Requirements traceability for Phase 15
- [baude/src/main.rs:398-460, 110-112](../../baude/src/main.rs) — Startup order, negotiate_keyboard signature, run loop
- [baude/src/app.rs:3707-3760, 1231, 35](../../baude/src/app.rs) — App::tick, App::restore, META_POLL_MS constant
- [baude/src/ui.rs:127-137, 225-248, 470-490, 785-810](../../baude/src/ui.rs) — spinner(), flash_on(), footer height gate, status rendering sites
- [baude/src/usage.rs:55-84, 35, 39](../../baude/src/usage.rs) — UsagePoller::start, POLL_SECS, FAIL_POLL_SECS
- [baude-core/src/session.rs:302, 323, 332](../../baude-core/src/session.rs) — poll_meta, kill, process_identity
- [baude-core/src/repository.rs:62-68](../../baude-core/src/repository.rs) — ProcessIdentity struct (pid, process_group)
- [baude-core/src/persist.rs:968-969, 1010-1020](../../baude-core/src/persist.rs) — Config auto_archive_minutes pattern, env override helper
- [baude-core/Cargo.toml](../../baude-core/Cargo.toml) — libc 0.2 dependency present
- [bauded/src/api.rs:117-144](../../bauded/src/api.rs) — /info endpoint structure

### Secondary (MEDIUM confidence — Phase 12, 13, 14 context)

- [Phase 14 RESEARCH.md](../.planning/phases/14-managed-worktree-identity/14-RESEARCH.md) — Format example, architecture patterns
- [Phase 12 STATE.md](../.planning/STATE.md) — Kitty keyboard protocol decisions D-01..D-07 (terminal detection prohibited, per-screen stacks, legacy fallback)
- [Phase 13 STATE.md](../.planning/STATE.md) — TestRedirect fixtures, test isolation patterns
- [PROJECT.md](../.planning/PROJECT.md) — "No regressions: stable sidebar order and dual-source waiting logic are hard-won"

### Tertiary (LOW confidence — training knowledge, not verified in this session)

- Ratatui TestBackend API for headless testing (assumed pattern from Phase 13)
- Crossterm event::poll bounded timeout (assumed standard API)
- SIGSTOP/SIGCONT process group behavior (standard POSIX; implementation tested in Wave 0)

## Metadata

**Confidence breakdown:**
- Standard stack & architecture patterns: **HIGH** — All source files read and verified; locked decisions from CONTEXT.md; no alternatives explored.
- Dirty-flag implementation: **HIGH** — Existing code structure (App, tick, draw loop) understood; generation counter pattern proven in Phase 13.
- Timing facility design: **HIGH** — Instant API and stderr output are standard Rust; bauded /info integration is straightforward.
- Keyboard probe bound: **HIGH** — negotiate_keyboard is a simple closure; event::poll is crossterm's standard API; 250 ms is CONTEXT.md decision.
- Idle child signals: **HIGH** — ProcessIdentity struct has process_group; libc 0.2 is a present dependency; SIGSTOP/SIGCONT are standard POSIX.
- Metadata polling gate: **HIGH** — Archive status and mtime checks are simple; backend poll_meta is the shared integration point.
- Usage poller interval: **HIGH** — Pattern matches existing auto_archive_minutes config field; thread spawn is conditional.
- Status codes & legend: **HIGH** — CONTEXT.md locks the glyph set and colors; footer height gate is already in code; ui.rs sites enumerated.
- Validation architecture: **MEDIUM** — Test names and assertions are recommendations based on Phase 14; exact test bodies written during planning.

**Research date:** 2026-09-21
**Valid until:** 2026-09-28 (startup/idle performance is stable domain; no fast-moving APIs; recheck if crossterm or ratatui major versions arrive)
