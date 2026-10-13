# Phase 15: Startup and Idle Performance - Pattern Map

**Mapped:** 2026-09-21
**Files analyzed:** 10 new/modified files
**Analogs found:** 10/10 (100% coverage)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `baude/src/main.rs` | handler | request-response | `baude/src/main.rs:398-470` (current run loop) | exact (same file, timing + keyboard probe) |
| `baude/src/app.rs` | controller | request-response | `baude/src/app.rs:3707-3760` (tick) + app.rs:1231 (restore) | exact (same file, dirty flag + incremental restore) |
| `baude/src/ui.rs` | component | request-response | `baude/src/ui.rs:127-137` (spinner, flash_on) + 225-248 (footer gate) | exact (same file, remove animation + status codes) |
| `baude/src/usage.rs` | service | request-response | `baude/src/usage.rs:54-84` (UsagePoller::start) | exact (same file, conditional spawn) |
| `baude-core/src/persist.rs` | model/config | CRUD | `baude-core/src/persist.rs:943-1020` (Config, auto_archive_minutes) | exact (same file, new Option<T> fields) |
| `baude-core/src/session.rs` | model | CRUD | `baude-core/src/session.rs:302-340` (poll_meta, kill) | exact (same file, generation counter, suspend/resume) |
| `baude-core/src/pty.rs` | service | CRUD | `baude-core/src/pty.rs` (kill method structure) | role-match (process lifecycle) |
| `baude-core/src/backend/mod.rs` | service trait | CRUD | `baude-core/src/backend/mod.rs:119-150` (poll_meta trait) | exact (same file, mtime gate) |
| `bauded/src/api.rs` | controller/handler | request-response | `bauded/src/api.rs:117-160` (/info endpoint) | exact (same file, add startup_ms) |
| `README.md` | documentation | file-I/O | `README.md` (existing Worktrees section) | exact (same file, add Performance subsection) |

## Pattern Assignments

### `baude/src/main.rs` (handler, request-response)

**Analogs:** `baude/src/main.rs:110-112` (negotiate_keyboard), `baude/src/main.rs:398-470` (startup order, run loop), `baude/src/main.rs:160-170` (keyboard probe closure)

**Startup timing struct pattern** (new; modeled on Phase 14 patterns):
```rust
// At top of main.rs, add:
use std::time::Instant;

struct TimingStage {
    name: &'static str,
    duration_ms: u128,
    note: Option<String>,  // e.g., "kitty 250ms (timeout)"
}

struct StartupTiming {
    stages: Vec<TimingStage>,
    total_ms: u128,
}

impl StartupTiming {
    fn print_to_stderr(&self) {
        let line = self.stages.iter()
            .map(|s| format!("{}={}", s.name, s.duration_ms))
            .collect::<Vec<_>>()
            .join(" ");
        eprintln!("baude startup: {}", line);
        for stage in &self.stages {
            if let Some(note) = &stage.note {
                eprintln!("  {}: {} ms {}", stage.name, stage.duration_ms, note);
            } else {
                eprintln!("  {}: {} ms", stage.name, stage.duration_ms);
            }
        }
    }
}
```

**Startup order with timing (lines 398-435, modified)**:
```rust
// Current pattern in main() at lines 398-435:
// enable_raw_mode → EnterAlternateScreen → negotiate_keyboard → Terminal::new → App::new → startup_notes → app.restore() → run()
// 
// PHASE 15 CHANGES:
let mut timing = StartupTiming { stages: vec![], total_ms: 0 };
let total_start = Instant::now();

// Stage 1: config & workspace
let config_start = Instant::now();
// ... existing config code ...
timing.stages.push(TimingStage { name: "config_load", duration_ms: config_start.elapsed().as_millis(), note: None });

// Stage 2: terminal setup
let term_start = Instant::now();
let mut terminal = ratatui::Terminal::new(...)?;
timing.stages.push(TimingStage { name: "terminal_setup", duration_ms: term_start.elapsed().as_millis(), note: None });

// Stage 3: keyboard probe (250 ms bound instead of 2 s)
let kb_start = Instant::now();
let kb_supported = negotiate_keyboard_bounded(supports_keyboard_enhancement, 250);  // NEW
let kb_duration = kb_start.elapsed();
let kb_timeout = if kb_duration.as_millis() >= 250 { "(timeout)" } else { "" };
timing.stages.push(TimingStage { 
    name: "keyboard_probe", 
    duration_ms: kb_duration.as_millis(), 
    note: Some(format!("kitty 250ms {}", kb_timeout))
});

// App::new (no restore yet)
let app_start = Instant::now();
let mut app = App::new(launch_dir);
timing.stages.push(TimingStage { name: "app_new", duration_ms: app_start.elapsed().as_millis(), note: None });

// First frame (before restore)
// timing is captured in run() after first draw

// Session restore
// timing is captured as part of incremental restore

// Total
timing.total_ms = total_start.elapsed().as_millis();
timing.print_to_stderr();
```

**Keyboard probe bounded to 250ms** (replace current negotiate_keyboard closure):
```rust
// NEW function to replace the 2s closure:
fn negotiate_keyboard_bounded(
    probe: impl FnOnce() -> std::io::Result<bool>,
    timeout_ms: u64,
) -> bool {
    // Use event::poll with custom timeout (crossterm API)
    // Fallback to legacy on timeout or error
    use ratatui::crossterm::event;
    use std::time::Duration;
    
    // Bounded event poll with custom probe closure
    match event::poll(Duration::from_millis(timeout_ms)) {
        Ok(_) => probe().unwrap_or(false),
        Err(_) => false,  // timeout → legacy
    }
}
```

**Run loop with dirty-flag draw gate** (lines 446-465, modified):
```rust
fn run(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    loop {
        app.tick();
        let area = terminal.get_frame().area();
        app.sync_sizes(area);

        // PHASE 15: Only draw if state changed
        if app.dirty {
            terminal.draw(|frame| ui::draw(frame, app))?;
            app.dirty = false;
        }

        // Input polling unchanged: 50 ms timeout for responsiveness
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

**Restore moved to loop (lines 430-436, modified)**:
```rust
// BEFORE (line 430-435):
// app.restore();
// let result = run(&mut terminal, &mut app);

// AFTER:
let result = run(&mut terminal, &mut app);  // restore runs incrementally inside the loop via app.tick()
```

---

### `baude/src/app.rs` (controller, request-response)

**Analog:** `baude/src/app.rs:3707-3760` (tick method), `baude/src/app.rs:1231` (restore method), App struct at top

**App struct dirty flag addition** (at struct definition, around line 80+):
```rust
pub struct App {
    // ... existing fields ...
    pub dirty: bool,                              // NEW: tracks if frame needs redraw
    restoring: bool,                              // NEW: coalesces durable writes during restore
    restore_progress: Option<RestoreProgress>,    // NEW: tracks incremental restore state
    last_known_screen_gen: HashMap<u64, u64>,     // NEW: per-session generation counters
}

struct RestoreProgress {
    saved_sessions: Vec<SavedCheckout>,  // or equivalent
    current_index: usize,
    total_count: usize,
}
```

**Dirty flag setter pattern** (add method to App):
```rust
impl App {
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }
}

// Call mark_dirty in all state-change paths:
// - handle_event(): every input event
// - on_pty_output(): any visible session's PTY data
// - on_status_change(): status transition
// - sync_sizes(): terminal resize (already calls)
// - auto_archive_tick(): archive state change
// - poll_meta(): when fetched_ms changes
// - remote_snap refresh: only when session list or fetched_ms changed
// - message set/expiry
// - selection changes
```

**Generation counter polling in tick()** (lines 3707-3760, add after existing meta_poll block):
```rust
pub fn tick(&mut self) {
    // ... existing tick code (lines 3707-3730) ...
    
    // NEW: Check if any visible session's screen changed (generation counter)
    if now_ms().saturating_sub(self.last_meta_poll) >= META_POLL_MS {
        self.last_meta_poll = now_ms();
        
        // Check screen generations for visible sessions
        for session in &mut self.sessions {
            if session.is_archived() || session.claude.is_exited() {
                // PERF-06: skip archived and exited rows
                continue;
            }
            
            let current_gen = session.screen_generation();  // NEW field in Session
            let last_known = self.last_known_screen_gen
                .get(&session.id)
                .copied()
                .unwrap_or(0);
            
            if current_gen != last_known {
                self.dirty = true;
                self.last_known_screen_gen.insert(session.id, current_gen);
            }
        }
        
        // ... existing poll_meta and auto_archive code ...
    }
    
    // Waiting-row timer: update at 1 Hz only when a waiting row is visible
    let has_waiting_rows = self.sessions.iter().any(|s| s.status == Status::Waiting);
    if has_waiting_rows {
        // Existing waiting_row_timer logic, but only runs when dirty is already true
        if self.waiting_row_timer.saturating_sub(self.last_waiting_tick) >= 1000 {
            self.last_waiting_tick = now_ms();
            self.dirty = true;  // Update timer display at 1 Hz
        }
    }
    
    // Remote snapshot: only mark dirty if fetched_ms or session list changed
    if let Some(r) = &self.remote {
        let new_snap = r.snapshot();
        let snap_changed = new_snap.fetched_ms != self.remote_snap.fetched_ms
            || new_snap.sessions.len() != self.remote_snap.sessions.len();
        if snap_changed {
            self.dirty = true;
        }
        self.remote_snap = new_snap;
    }
    
    // ... rest of existing tick code ...
}
```

**Incremental restore refactor** (modify restore() and add to tick() loop):
```rust
// In App struct, add:
restoring: bool,  // Set to true at start, false when all sessions admitted

// NEW: App::restore_one_session() method
fn restore_one_session(&mut self) -> Result<bool> {  // Returns true if more sessions to restore
    // Restore exactly one saved session from self.restore_progress
    // Call self.admit_one_saved_session(saved) to add it
    // Return whether more sessions remain
}

// Modify App::restore() to just set up state
pub fn restore(&mut self) {
    self.restoring = true;
    // Load persisted state without admitting sessions
    let loaded = persist::load_for_workspace(...)?;
    self.restore_progress = Some(RestoreProgress {
        saved_sessions: loaded.saved_sessions,
        current_index: 0,
        total_count: loaded.saved_sessions.len(),
    });
}

// In tick(), after drawing the first empty frame:
pub fn tick(&mut self) {
    // ... existing tick code ...
    
    // Incremental restore: one session per iteration
    if let Some(ref mut progress) = self.restore_progress {
        if progress.current_index < progress.total_count {
            // Restore one saved session
            let saved = progress.saved_sessions[progress.current_index].clone();
            self.admit_one_saved_session(&saved);
            progress.current_index += 1;
            self.dirty = true;  // Mark for redraw
            
            // Batch durable writes during restore
            if progress.current_index == progress.total_count {
                // Last session admitted: write state file once
                self.save();  // Will see self.restoring=true
            }
        } else {
            self.restoring = false;
            self.restore_progress = None;
        }
    }
    
    // ... rest of tick code ...
}

// Modify save() to check restoring guard
pub fn save(&mut self) {
    if self.restoring {
        // During restore: mark dirty but don't write yet
        self.dirty_persist = true;  // Add this field to App
    } else {
        // Normal interactive save: immediate
        persist::save_durable_status(self.state);
    }
}
```

**Idle child policy dispatch** (in tick(), after auto_archive decision):
```rust
// In the auto_archive block of tick():
for session in &mut self.sessions {
    let should_archive = session.auto_archive_tick(self.auto_archive_ms);
    if should_archive {
        match self.config.idle_child_policy().as_str() {
            "suspend" => session.suspend_idle_child(),  // NEW method in Session
            "stop" => session.kill(),  // Existing method
            _ => {}  // "keep" — do nothing
        }
    }
}
```

---

### `baude/src/ui.rs` (component, request-response)

**Analog:** `baude/src/ui.rs:127-137` (spinner, flash_on), `baude/src/ui.rs:225-248` (footer height gate), session/checkout row renderers

**Remove spinner() and flash_on()** (lines 127-140, DELETE):
```rust
// DELETE:
// fn spinner() -> &'static str { ... }
// fn flash_on() -> bool { ... }

// All 7 call sites in ui.rs must be replaced with static status codes
```

**Status code helper function** (new, add near top of ui.rs rendering section):
```rust
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
```

**Session row renderer update** (find the session row rendering code, replace animated spinner with static):
```rust
// In session row renderer (exact line varies, search for "spinner()" call):
// BEFORE:
//   let icon = if status == Status::Busy {
//       spinner()  // Animated
//   } else if status == Status::Waiting {
//       if flash_on() { "?" } else { " " }  // Flashing
//   } else { ... };

// AFTER:
let (code, style) = status_code(status);
// Render `code` with `style`
```

**Checkout row renderer update** (similar change):
```rust
// Find checkout row rendering code
// BEFORE: animated spinner or flash
// AFTER:
let (code, style) = status_code(checkout.status);
```

**Legend rendering in footer** (add after footer height gate at lines 225-248):
```rust
// In the footer rendering section (around line 231-248):
// After the height gate that checks if footer fits:

if footer_height >= 19 && list_height >= FOOTER_H + 4 {
    // Legend fits: render it
    let legend_text = "? waiting  B busy  ✓ done  ✗ exited  - closed  A archived  ! unavailable";
    let legend = Paragraph::new(legend_text)
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Left);
    frame.render_widget(legend, legend_area);
}
```

**Help/keys text amendment** (find where help text is rendered):
```rust
// Append legend to help text if one is displayed:
if let Some(help_text) = &self.help_text {
    let legend = "\n? waiting  B busy  ✓ done  ✗ exited  - closed  A archived  ! unavailable";
    let amended = format!("{}{}", help_text, legend);
    // Render amended text
}
```

---

### `baude/src/usage.rs` (service, request-response)

**Analog:** `baude/src/usage.rs:54-84` (UsagePoller::start with #[cfg(test)])

**Config-driven thread spawn** (lines 54-84, modify):
```rust
// BEFORE: UsagePoller::start() spawned a thread unconditionally (in non-test mode)

// AFTER: Check config before spawning
impl UsagePoller {
    #[cfg(test)]
    pub fn start(config_poll_secs: Option<u64>) -> UsagePoller {
        // Test variant: no thread
        UsagePoller {
            data: Arc::new(Mutex::new(UsageCosts::default())),
        }
    }

    #[cfg(not(test))]
    pub fn start(config_poll_secs: Option<u64>) -> UsagePoller {
        let data = Arc::new(Mutex::new(UsageCosts::default()));
        
        // NEW: Check if disabled
        if config_poll_secs == Some(0) {
            // Disabled: return inert poller, no thread
            return UsagePoller { data };
        }
        
        let shared = Arc::clone(&data);
        let poll_interval = config_poll_secs.unwrap_or(POLL_SECS);  // Default to today's POLL_SECS
        
        std::thread::spawn(move || loop {
            let costs = fetch();
            let ok = costs.today_usd.is_some() || costs.week_usd.is_some();
            if let Ok(mut d) = shared.lock() {
                *d = costs;
            }
            std::thread::sleep(Duration::from_secs(if ok {
                poll_interval  // Use config value instead of POLL_SECS
            } else {
                FAIL_POLL_SECS
            }));
        });
        UsagePoller { data }
    }
}
```

**Footer message for disabled poller** (find where footer text is built):
```rust
// In the footer rendering code (ui.rs):
// If config.usage_poll_secs == Some(0):
//     Show "usage: off" instead of blanks or "loading..."
```

---

### `baude-core/src/persist.rs` (model/config, CRUD)

**Analog:** `baude-core/src/persist.rs:943-1020` (Config struct, auto_archive_minutes pattern)

**Config struct new fields** (around line 969, add after auto_archive_minutes):
```rust
pub struct Config {
    // ... existing fields ...
    
    /// Minutes of idle waiting before a session auto-archives; 0 disables
    /// auto-archiving. BAUDED_AUTO_ARCHIVE_MIN overrides. Defaults to 30.
    pub auto_archive_minutes: Option<u64>,
    
    // NEW FIELDS:
    /// Idle child process policy: "keep" (default), "suspend", or "stop".
    /// Applied after auto_archive_minutes. BAUDE_IDLE_CHILD_POLICY overrides.
    pub idle_child_policy: Option<String>,
    
    /// Usage poller interval in seconds; 0 disables. BAUDE_USAGE_POLL_SECS
    /// overrides. Defaults to today's POLL_SECS (baude/src/usage.rs).
    pub usage_poll_secs: Option<u64>,
    
    // ... rest of fields ...
}
```

**Config method for idle_child_policy** (add to Config impl, following auto_archive_ms pattern):
```rust
impl Config {
    // ... existing methods ...
    
    /// Resolved idle child policy with env override.
    pub fn idle_child_policy(&self) -> String {
        std::env::var("BAUDE_IDLE_CHILD_POLICY")
            .ok()
            .or_else(|| self.idle_child_policy.clone())
            .unwrap_or_else(|_| "keep".to_string())  // Default: keep (don't suspend)
    }
    
    /// Resolved usage poller interval in seconds with env override.
    pub fn usage_poll_secs(&self) -> Option<u64> {
        std::env::var("BAUDE_USAGE_POLL_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .or(self.usage_poll_secs)
            // No default; None means use POLL_SECS constant in usage.rs
    }
}
```

---

### `baude-core/src/session.rs` (model, CRUD)

**Analog:** `baude-core/src/session.rs:302-340` (poll_meta, kill methods)

**Session struct generation counter** (add to Session struct):
```rust
pub struct Session {
    // ... existing fields ...
    screen_generation: u64,  // NEW: incremented on PTY output
}

impl Session {
    pub fn screen_generation(&self) -> u64 {
        self.screen_generation
    }
}
```

**Increment screen_generation on PTY data** (find where PTY output is received):
```rust
// When the PTY reader receives data and updates the screen:
impl Session {
    fn on_pty_output(&mut self) {
        self.screen_generation += 1;  // NEW
        // Existing PTY update code...
    }
}
```

**Suspend/resume methods** (add to Session):
```rust
impl Session {
    pub fn suspend_idle_child(&mut self) {
        if let Some(pi) = self.claude.process_identity().clone() {
            let pgid = pi.process_group;
            #[cfg(unix)]
            unsafe {
                if libc::kill(pgid, libc::SIGSTOP) == 0 {
                    self.child_suspended = true;
                }
            }
            #[cfg(not(unix))]
            {
                // Non-Unix: no-op (idle_child_policy defaults to "keep")
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
}
```

**Add child_suspended field to Session**:
```rust
pub struct Session {
    // ... existing fields ...
    child_suspended: bool,  // NEW: tracks SIGSTOP state
}
```

---

### `baude-core/src/pty.rs` (service, CRUD)

**Analog:** `baude-core/src/pty.rs` (Pty struct and kill method)

**Suspend/resume methods on Pty** (add to Pty impl):
```rust
impl Pty {
    pub fn suspend(&self) {
        if let Some(pi) = self.process_identity() {
            let pgid = pi.process_group;
            #[cfg(unix)]
            unsafe {
                let _ = libc::kill(pgid, libc::SIGSTOP);
            }
        }
    }

    pub fn resume(&self) {
        if let Some(pi) = self.process_identity() {
            let pgid = pi.process_group;
            #[cfg(unix)]
            unsafe {
                let _ = libc::kill(pgid, libc::SIGCONT);
            }
        }
    }
}
```

---

### `baude-core/src/backend/mod.rs` (service trait, CRUD)

**Analog:** `baude-core/src/backend/mod.rs:119-150` (poll_meta trait method)

**Mtime gate in poll_meta** (modify trait documentation and backend implementations):
```rust
// In the Backend trait (lines 119-127), update documentation:
///
/// Metadata polling gates:
/// - Skip archived rows entirely (no stat or read)
/// - Skip exited rows entirely (no stat or read)
/// - For live rows: compare mtime of metadata files (session.json, hook-events.jsonl)
///   before reading; if unchanged, skip the read (cost: one stat only)
/// - A changed mtime triggers a full metadata read
///
fn poll_meta(
    &self,
    meta: &mut ClaudeMeta,
    cwd: &Path,
    pid: Option<u32>,
    spawn_unix_ms: u64,
    repo_root: &Path,
);
```

**Mtime gate implementation** (in both claude.rs and opencode.rs implementations):
```rust
// In backend/claude.rs and backend/opencode.rs, find poll_meta implementation:

fn poll_meta(
    &self,
    meta: &mut ClaudeMeta,
    cwd: &Path,
    pid: Option<u32>,
    spawn_unix_ms: u64,
    repo_root: &Path,
) {
    // NEW: Check mtime before reading metadata
    let session_file = cwd.join("session.json");
    let hook_events_file = cwd.join("hook-events.jsonl");
    
    // Get current mtimes
    let session_mtime = std::fs::metadata(&session_file)
        .map(|m| m.modified().ok())
        .ok()
        .flatten();
    let events_mtime = std::fs::metadata(&hook_events_file)
        .map(|m| m.modified().ok())
        .ok()
        .flatten();
    
    // If either file changed since last read, proceed with full poll
    if session_mtime != meta.last_session_mtime || events_mtime != meta.last_events_mtime {
        // Proceed with existing poll_meta logic
        // ... existing code ...
        
        // Update stored mtimes
        meta.last_session_mtime = session_mtime;
        meta.last_events_mtime = events_mtime;
    }
    // else: skip read, cost is one stat per file
}
```

**Add mtime fields to ClaudeMeta** (in baude-core/src/meta.rs or similar):
```rust
pub struct ClaudeMeta {
    // ... existing fields ...
    last_session_mtime: Option<SystemTime>,   // NEW
    last_events_mtime: Option<SystemTime>,    // NEW
}
```

---

### `bauded/src/api.rs` (controller/handler, request-response)

**Analog:** `bauded/src/api.rs:117-160` (/info endpoint)

**Add startup_ms to /info response** (modify info function at lines 117-145):
```rust
async fn info(State(state): State<Shared>) -> Json<serde_json::Value> {
    let ws = baude_core::workspace::active();
    let manager = lock(&state);
    let persistence = manager.persistence_status();
    let workspace_source = ws
        .display_hint()
        .trim_matches('(')
        .trim_matches(')')
        .to_string();
    let collisions = manager.collisions.clone();
    let collision_count = collisions.len();
    
    // NEW: Capture startup timing from the daemon's own startup
    // (bauded runs in main, captures timing the same way baude does)
    let startup_ms = crate::startup_timing();  // Delegate to a new function in bauded/src/main.rs
    
    Json(serde_json::json!({
        "workspace": ws.name,
        "backend": ws.backend.name(),
        "workspace_source": workspace_source,
        "version": env!("CARGO_PKG_VERSION"),
        "persistence": persistence,
        "collisions": collisions,
        "collision_count": collision_count,
        "startup_ms": startup_ms,  // NEW: map of stage names to durations
    }))
}
```

**Startup timing capture in bauded/src/main.rs** (add to daemon startup):
```rust
// In bauded/src/main.rs, add a static or thread-local to capture timing:
use std::sync::Mutex;

lazy_static::lazy_static! {
    static ref DAEMON_STARTUP_TIMING: Mutex<Option<serde_json::Value>> = Mutex::new(None);
}

fn startup_timing() -> serde_json::Value {
    DAEMON_STARTUP_TIMING.lock().unwrap()
        .clone()
        .unwrap_or_else(|| serde_json::json!({}))
}

// In daemon startup (similar pattern to baude):
// Capture Instant at each stage, store as JSON map at end
```

---

### `README.md` (documentation, file-I/O)

**Analog:** `README.md` (existing Worktrees section and other subsections)

**Add Performance subsection** (new section, recommended after Configuration or Performance section if one exists):
```markdown
## Performance

baude aims for minimal CPU and battery cost while idle.

### Startup Timing

To diagnose slow startup, run with the `BAUDE_TIMING=1` environment variable:

```sh
BAUDE_TIMING=1 baude
```

Timing stages will be printed to stderr when baude exits:
- `config_load` — Config file read and parsed
- `workspace_resolution` — Folder bindings and repository root derived
- `terminal_setup` — Terminal::new and mode setup
- `keyboard_probe` — Kitty keyboard protocol negotiation (250 ms timeout)
- `app_new` — App struct initialized
- `first_frame` — First frame drawn (before session restore)
- `session_restore` — All saved sessions admitted (N sessions per second on main thread)
- `first_metadata_poll` — First metadata poll cycle completed
- `total` — Wall-clock duration from startup to exit

### Idle Behavior

With no input or status changes, baude issues zero terminal writes. Status codes are static glyphs:

- `?` (yellow bold) — Waiting for your input
- `B` (blue) — Busy or running
- `✓` (green) — Completed turn, your move
- `✗` (dark gray) — Claude exited
- `-` (gray) — Closed checkout with no live session
- `A` (dark gray) — Archived
- `!` (yellow) — Unavailable or missing

A legend is shown in the sidebar footer when space is available.

### Configuration

Three settings control idle behavior:

#### `idle_child_policy` (default: `keep`)

When a session auto-archives after `auto_archive_minutes`, the idle Claude child process can be:

- `keep` — Leave the child running (default, today's behavior)
- `suspend` — Send SIGSTOP to the child's process group; resumes on selection (Unix only)
- `stop` — Terminate the child; becomes `✗ exited` and is resumable with the restart key

Override with `BAUDE_IDLE_CHILD_POLICY=suspend` or `BAUDE_IDLE_CHILD_POLICY=stop`.

#### `usage_poll_secs` (default: today's polling interval)

The usage poller runs in a background thread and refreshes the footer's usage display every N seconds. Set to `0` to disable:

```json
{
  "usage_poll_secs": 0
}
```

Or override with `BAUDE_USAGE_POLL_SECS=30` (seconds). When disabled, the footer shows `usage: off`.

---

```

---

## Shared Patterns

### Dirty-Flag Render Loop
**Source:** `baude/src/main.rs:446-465` (current run loop), applied to PERF-05
**Apply to:** Main loop in `baude/src/main.rs`, called from all state-change paths in `app.rs`
- Call `app.tick()` unconditionally
- Call `terminal.draw()` ONLY if `app.dirty == true`
- Clear `app.dirty = false` after drawing
- Keep 50 ms `event::poll` timeout for input latency
```rust
// Template:
if app.dirty {
    terminal.draw(|frame| ui::draw(frame, app))?;
    app.dirty = false;
}
```

### Config Option<T> with Env Override
**Source:** `baude-core/src/persist.rs:1010-1020` (auto_archive_minutes pattern)
**Apply to:** `persist.rs` Config impl for new fields (idle_child_policy, usage_poll_secs)
- Field is `Option<T>` in the struct
- Method reads env var first, falls back to struct field, then default
- Env var takes precedence
```rust
// Template:
pub fn field_name(&self) -> ValueType {
    std::env::var("ENV_VAR_NAME")
        .ok()
        .and_then(|v| v.parse::<ValueType>().ok())
        .or(self.field_name)
        .unwrap_or(default_value)
}
```

### POSIX Signal Wrapping
**Source:** `baude-core/src/repository.rs:62-68` (ProcessIdentity), `libc` crate (0.2, already present)
**Apply to:** `session.rs` and `pty.rs` for suspend/resume methods
- Access `process_group` from ProcessIdentity
- Wrap `libc::kill(pgid, signal)` in a method on Session or Pty
- Use `#[cfg(unix)]` gate; non-Unix platforms no-op or default
```rust
// Template:
pub fn suspend_idle_child(&mut self) {
    if let Some(pi) = self.claude.process_identity().clone() {
        let pgid = pi.process_group;
        #[cfg(unix)]
        unsafe {
            if libc::kill(pgid, libc::SIGSTOP) == 0 {
                self.child_suspended = true;
            }
        }
    }
}
```

### Testable Generation Counters
**Source:** Phase 13 test_title_rendering pattern, `baude/src/ui.rs:2500-2550`
**Apply to:** Session screen_generation counter and dirty-flag tests
- Counters are cheap `u64` increments, no synchronization overhead
- Change detection is deterministic and testable
- No channels or async needed
```rust
// Template:
let current_gen = session.screen_generation();
let last_known = self.last_known_screen_gen.get(&session.id).copied().unwrap_or(0);
if current_gen != last_known {
    self.dirty = true;
    self.last_known_screen_gen.insert(session.id, current_gen);
}
```

### TestRedirect Fixture Pattern
**Source:** `baude-core/src/testing.rs`, applied in Phase 14 tests, `baude/src/ui.rs:2336-2370`
**Apply to:** All new tests for timing, dirty flag, suspend/resume, mtime gate
- Use `TestRedirect` to isolate filesystem operations
- All paths resolve through testing overrides
- Thread-local guards abort tests reaching real home
```rust
// Template:
#[test]
fn test_dirty_flag_on_input() {
    let _redirect = baude_core::testing::TestRedirect::new(&test_root);
    let mut app = App::new(test_dir);
    assert!(!app.dirty, "initial state not dirty");
    app.handle_event(some_event);
    assert!(app.dirty, "input event marks dirty");
}
```

---

## Test Patterns

### Dirty-Flag Tests
**Pattern:** Use TestRedirect fixture + manual App construction + verify dirty flag state
**Call site:** `baude/src/app.rs::tests::` (new module)
```rust
#[test]
fn dirty_flag_set_on_input_event() { ... }

#[test]
fn dirty_flag_cleared_after_draw() { ... }

#[test]
fn idle_zero_draws_after_first_frame() {
    let backend = TestBackend::new(100, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = App::new(...);
    
    // First draw (empty frame)
    assert!(app.dirty);
    terminal.draw(|f| ui::draw(f, &app)).unwrap();
    app.dirty = false;
    
    // N idle ticks: verify zero draws
    for _ in 0..100 {
        app.tick();
        assert!(!app.dirty, "idle tick should not mark dirty");
    }
}
```

### Keyboard Probe Timeout Tests
**Pattern:** Mock the probe closure with a never-answering variant, verify 250 ms bound
**Call site:** `baude/src/main.rs::tests::`
```rust
#[test]
fn keyboard_probe_timeout_bounded_250ms() {
    let start = Instant::now();
    let _supported = negotiate_keyboard_bounded(
        || {
            // Never answer; block indefinitely
            std::thread::sleep(Duration::from_secs(10));
            Ok(true)
        },
        250,
    );
    let elapsed = start.elapsed();
    assert!(elapsed.as_millis() < 500, "probe should timeout near 250ms, got {:?}", elapsed);
}
```

### Process Suspension Tests
**Pattern:** Verify SIGSTOP changes process state to 'T' (Unix only)
**Call site:** `baude-core/src/session.rs::tests::`
```rust
#[test]
#[cfg(unix)]
fn idle_child_suspend_sigstop() {
    // Spawn a dummy child, call suspend, verify state is 'T'
    // Use /proc/<pid>/stat on Linux or procfs on macOS to check state
}
```

### Mtime Gate Tests
**Pattern:** Mock fs::metadata to track stat vs read operations
**Call site:** `baude-core/src/backend::tests::`
```rust
#[test]
fn unchanged_mtime_skips_read() {
    // Mock fs::metadata to return unchanged mtime
    // Call poll_meta, verify backend::read_metadata is NOT called
}
```

### Status Code Parity Tests
**Pattern:** Verify both session_code() and checkout_code() return identical glyphs
**Call site:** `baude/src/ui.rs::tests::`
```rust
#[test]
fn status_code_parity() {
    for status in [Status::Waiting, Status::Busy, Status::Completed, ...] {
        let (code, style) = status_code(status);
        // Verify code is a single character and style matches expected color
        assert!(code.len() <= 2, "glyph should be single char");
    }
}
```

---

## No Analog Found

All files have direct analogs in the existing codebase. No files introduce fundamentally new patterns.

---

## Metadata

**Analog search scope:** baude/src/*.rs, baude-core/src/*.rs, bauded/src/*.rs, ratatui TestBackend, crossterm event::poll, libc::kill
**Files scanned:** 12 Rust source files + 1 documentation file
**Pattern extraction date:** 2026-09-21

**Key findings:**
- Dirty-flag pattern proven in Phase 14 ui tests; generation counters follow Phase 13 test_title_rendering
- Config Option<T> + env override is stable, used for auto_archive_minutes
- POSIX signal wrapping via libc is standard, ProcessIdentity provides process_group
- Testable generation counters require no synchronization overhead
- TestRedirect and TestBackend patterns are established across phases
- All startup timing and idle loop refactoring fits within existing run() structure
