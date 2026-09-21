# Phase 15: Startup and Idle Performance - Context

**Gathered:** 2026-09-21
**Status:** Ready for planning
**Mode:** Smart discuss, autonomous (`--auto`): recommended answers accepted and logged; two areas carry decisions Joe made explicitly on 2026-09-20 (PERF-05 static glyphs, UX-02 colored letter codes) and those are locked.

<domain>
## Phase Boundary

Startup is measurable and reaches the first frame quickly without blocking on external services, and an idle baude with many sessions costs near-zero CPU: no unconditional or timer-driven redraws, no polling of dead rows, an opt-in policy for idle Claude children, and a configurable usage poller. Status indicators become static single-character codes in their existing colors with an in-app legend. Requirements: PERF-01..08, UX-02. Out of scope: PWA changes, per-checkout scan ownership, background-thread session restore (App is not Send), any change to the dual-source waiting logic or stable sidebar order (PROJECT.md non-regression rule).

</domain>

<decisions>
## Implementation Decisions

### Startup timing and first frame (PERF-01..04)
- Timing facility: `BAUDE_TIMING=1` (env) or `baude --timing` records wall-clock durations for named stages — config load, workspace resolution, terminal setup, kitty probe, first frame, session restore (with session count), first metadata poll, total — and prints them as one summary line plus one line per stage to stderr after the terminal is restored at exit, so a slow launch is diagnosable without a debugger. The same stage list is exposed as a `startup_ms` map on the daemon `/info` payload for bauded's own startup. No timing output unless enabled.
- First frame before restore: `App::restore()` no longer runs before the loop. The loop draws the empty frame (sidebar chrome, workspace title, a `restoring k/N sessions…` status line) first, then restores saved sessions incrementally on the main thread — one saved session (admit + spawn) per loop iteration until done — so input is live and the frame is visible within the first tick. Restore order and the resulting sidebar order are unchanged from today.
- Kitty keyboard probe: `negotiate_keyboard` keeps its closure shape but the probe is bounded to 250 ms (down from 2 s); on timeout or error baude degrades to legacy encoding exactly as it does for a `false` answer, and records the outcome as a timing stage (`kitty 250ms (timeout)`). No persisted cache of the answer; terminals differ per launch.
- Batched restore writes: while restore is in progress a `restoring` guard coalesces `save_durable_status()` calls; the state file is written once when the last saved session has been processed (and once more only if a later interactive action changes state). Interactive activations outside restore keep saving immediately. Failure semantics of the durable write are unchanged; a coalesced failure surfaces the same message once.

### Idle redraw and static status codes (PERF-05, UX-02) — locked by Joe 2026-09-20
- Dirty-flag redraw: `App` gains a `dirty` flag; the main loop calls `terminal.draw` only when dirty and clears it after drawing. Dirty is set by: any input event, terminal resize, a status transition or new PTY output on any visible session (per-session screen generation counters compared each tick, not channels), message set/expiry, remote snapshot change (`fetched_ms` changed), restore progress, and selection changes. The 50 ms `event::poll` timeout stays for input latency; with nothing dirty the loop issues no terminal writes.
- No animation timer: `spinner()` and `flash_on()` and their seven call sites in `ui.rs` are removed. Working/busy rows show a static `B`, waiting rows a static bold `?`; nothing pulses.
- Status codes and colors (UX-02): `?` yellow bold = waiting for your input (question or permission); `B` blue = busy; `✓` green = completed turn, your move; `✗` dark gray = claude exited; `-` gray = closed checkout with no live session; `A` dark gray = archived; `!` yellow = unavailable/missing. Existing per-state colors are kept exactly; the letter is the icon. Both the session-row and checkout-row renderers use one shared `status_code(status) -> (&'static str, Style)` helper so the two cannot drift.
- Legend: a one-line colored legend `? waiting  B busy  ✓ done  ✗ exited  - closed  A archived  ! unavailable` is rendered in the sidebar footer when the footer has room (same height gate as the usage footer), and the same line is appended to the help/keys text if one exists. The waiting-row elapsed timer keeps updating, but at 1 Hz and only while a waiting row is visible (it marks dirty once per second); with no waiting rows there are zero periodic redraws.

### Idle polling, children, and the usage poller (PERF-06..08)
- Metadata polling: `META_POLL_MS` stays 1000, but per-session work is gated: archived rows and exited rows are not polled at all, and live rows compare the mtime of each backend metadata file (session file, hook event file) before reading it, so an unchanged file costs one `stat`. The gate lives in the backend `poll_meta` path so both backends benefit.
- Idle child policy: one opt-in config field `idle_child_policy` = `keep` (default, today's behavior) | `suspend` | `stop`, env override `BAUDE_IDLE_CHILD_POLICY`. Applied when a row auto-archives after `auto_archive_minutes` and when the user archives a row manually. `suspend` sends SIGSTOP to the claude child's process group and SIGCONT on unarchive or selection; the row's state text reads `suspended` while stopped. `stop` kills the claude child (row becomes `✗ exited`, resumable with the existing restart key). Shell panes follow the same policy. Unix-only signals; on other platforms `suspend` behaves like `keep` with a startup note.
- Usage poller: config `usage_poll_secs` (`Option<u64>`; `0` disables) with env override `BAUDE_USAGE_POLL_SECS`; default equals today's `POLL_SECS`. The poller never runs `ccusage` more often than the configured interval, keeps its existing back-off when ccusage is missing, and when disabled the footer shows `usage: off` instead of blanks.
- Remote snapshot: `remote_snap` is refreshed each tick as today but only marks dirty when `fetched_ms` or the session list changed, so an idle remote does not cause redraws.

### Measurement and acceptance
- Idle test: the loop body is refactored into a `step(app, terminal) -> Stepped { drew: bool }` function so a headless test with a `TestBackend` can drive N idle sessions for many ticks and assert zero draws after the first, and assert exactly one draw after an injected status change or key event.
- Startup tests: an ordered event log in tests proves the first draw precedes the first restore step; `negotiate_keyboard` is tested with a never-answering probe closure and must return within the bound; restore of N saved sessions performs exactly one durable save (count via the existing `atomic_failure_for_test` hook or a save counter).
- Poll tests: an unchanged metadata file is not re-read (mtime gate), an archived row is not polled; `idle_child_policy=suspend` leaves the child stopped (process state `T` on Unix) and `unarchive` resumes it; `usage_poll_secs=0` spawns no poller thread.
- Docs: README gains a short "Performance" subsection listing `BAUDE_TIMING`, `idle_child_policy`, `usage_poll_secs`, the status legend, and what "idle costs nothing" means; the shipped CLAUDE.md conventions (four CI gates, TestRedirect, no `set_var`) apply.

### Claude's Discretion
- Exact stage names and the timing line format; how the dirty flag and generation counters are plumbed through `App`/`Session`; `libc` vs `nix` for SIGSTOP/SIGCONT (prefer whichever is already a dependency); legend wording spacing; whether `--timing` is a real clap flag or an alias that sets the env var before startup.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `baude/src/main.rs:398-460` — startup order today: `enable_raw_mode` → `negotiate_keyboard(supports_keyboard_enhancement)` (probe timeout 2 s at `main.rs:166`) → `Terminal::new` → `App::new` → startup notes → `app.restore()` → `run()`. The `run` loop at `:446-465` calls `app.tick()`, `sync_sizes`, then `terminal.draw` unconditionally every iteration with a 50 ms `event::poll`, i.e. ~20 Hz of full-frame terminal writes when idle.
- `baude/src/app.rs:3707-3760` `App::tick`: message expiry, `poll_pending_clones`, per-session `poll_meta` + `auto_archive_tick` every `META_POLL_MS` (1000, `app.rs:35`) with `self.save()` when anything auto-archived, `remote_snap = r.snapshot()` every tick, folder-context flush, desktop notify, selection reconcile.
- `baude/src/app.rs:1231` `App::restore` → `restore_standalones` (`:1834`) and the activation paths that call `save_durable_status()` at `:2164`, `:2199`, `:2230`, `:2261` (several fsync'd full rewrites per restored session); `atomic_failure_for_test` at `:527` is an existing test hook on the durable write.
- `baude-core/src/session.rs:302` `Session::poll_meta` delegates to `backend::active().poll_meta(&mut meta, cwd, pid, spawn_unix_ms, repo_root)`; `Session::kill` (`:323`) and `process_identity()` give access to the claude/shell child identity (pid) for signals; `baude-core/src/pty.rs:515` `Pty::kill`.
- `baude/src/usage.rs:64-84` `UsagePoller::start` spawns a thread looping `fetch()` then `sleep(POLL_SECS | FAIL_POLL_SECS)`; a `#[cfg(test)]` no-thread variant exists at `:54`.
- `baude/src/ui.rs:127-137` `spinner()` (130 ms wall-clock frames) and `flash_on()` (~1.4 Hz); call sites at `:473-480`, `:589-590`, `:784-800`, `:858` (7 total) plus the usage footer at `:906` and the footer height gate at `:231-248`.
- `baude-core/src/persist.rs` `Config`: `Option<T>` fields with env overrides (`auto_archive_minutes` / `BAUDED_AUTO_ARCHIVE_MIN`, `daemon_url` / `BAUDE_DAEMON_URL`) — the pattern for `idle_child_policy` and `usage_poll_secs`.
- Phase 13 `startup_notes` (main.rs → `app.set_message`) and Phase 14 `collision_line` show how one-line notes reach the status line.

### Established Patterns
- Tests run under `TestRedirect`/`LifecycleFixture`; no real `~/.config/baude`; no `std::env::set_var`; config values come from `persist::Config` with env overrides.
- Hard-won behaviors that must not regress (PROJECT.md): stable sidebar order and the dual-source waiting logic; the waiting-row timer and needs-input signal stay, only the pulsing goes.
- `ui.rs` tests render into a ratatui `TestBackend` buffer (Phase 13 `test_title_rendering`), the pattern for legend and status-code tests.

### Integration Points
- `main.rs` run loop and startup sequence; `App` (dirty flag, restore state machine, tick gating); `ui.rs` (status codes, legend, footer); `usage.rs` (interval); `persist.rs` `Config` (two new fields); `session.rs`/`pty.rs` (suspend/stop); backend `poll_meta` (mtime gate); `bauded/src/api.rs` `/info` (`startup_ms`); README Performance subsection.

</code_context>

<specifics>
## Specific Ideas

- Joe (2026-09-20): "baude has gotten slow to load" and "baude appears to destroy my battery life … only able to use my machine for like 45 minutes"; the diagnosis behind PERF-05..08 was the unconditional ~20 Hz redraw driving iTerm2 to ~70% CPU while idle, idle claude children never suspended, and per-session polling that scales with the session count.
- Joe (2026-09-20): "instead of animating, it can just be a busy icon or thinking icon, that would prevent the need to redraw at all unless state is changed" → PERF-05 static glyphs, no animation timer.
- Joe (2026-09-20): "make a letter instead too that's a little more indicative … it's hard to know what empty, full, blue, etc mean without a key" and "keep the colors of those items though too, blue, gray, yellow, etc … so icons" → UX-02: `?` `B` `✓` `✗` `-` `A` `!` in their existing colors plus a legend.

</specifics>

<deferred>
## Deferred Ideas

- PWA banner for daemon collisions (deferred in Phase 14; the remote TUI header shows the count).
- Per-checkout ownership rows in `baude worktrees scan` (Phase 14 ships per-repository-directory ownership).
- Restoring sessions on a background thread (App is not Send; incremental main-thread restore is the chosen shape).
- A configurable redraw rate: unnecessary once redraws are event-driven.

</deferred>
