# Phase 11: Negotiated Multiline Input - Pattern Map

**Mapped:** 2026-09-16
**Files analyzed:** 8 (7 modified, 1 created)
**Analogs found:** 8 / 8

All analog paths below verified git-tracked (`git ls-files`) — no `.gsd/` mirror paths.

## File Classification

| New/Modified File | Change | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|--------|------|-----------|----------------|---------------|
| `baude/src/main.rs` | modify | config (process lifecycle) | request-response (terminal init/restore) | `baude/src/main.rs:98-106, 386-399` (self: restore_terminal + panic hook + init) | exact |
| `baude/src/keys.rs` | modify | utility (pure encoder) | transform (KeyEvent → bytes) | `baude/src/keys.rs:43-52` (self: the arm being replaced) + `cursor_key` DECCKM-context precedent | exact |
| `baude/src/app.rs` | modify | controller (event routing) | event-driven | `baude/src/app.rs:3923-3964` (self: forward_key's `application_cursor()` read) | exact |
| `baude/src/ui.rs` | modify | component (help overlay) | request-response (render) | `baude/src/ui.rs:2173-2229` (`Modal::Help` paragraph) | exact |
| `vendor/vt100/src/screen.rs` | modify | service (parser state) | streaming (byte-in/state-out) | `screen.rs:1264-1345` (decset/decrst) + `screen.rs:677-693` (mode accessors) | exact |
| `vendor/vt100/tests/kitty_keyboard.rs` | **create** | test | streaming (byte-in/state-out) | `vendor/vt100/tests/hyperlink.rs` | exact |
| `baude-core/src/pty.rs` | modify | service (PTY session) | streaming (mode replay) | `baude-core/src/pty.rs:390-422` (self: subscribe snapshot) | exact |
| `README.md` | modify | docs | — | `README.md:108-146` (`## Keys` section + terminal caveat prose) | exact |

New `#[cfg(test)]` modules (keys.rs corpus, main.rs negotiation/restore seams) follow the in-repo injected-spy test precedents under Shared Patterns.

## Pattern Assignments

### `baude/src/main.rs` (lifecycle, request-response)

**Analog:** itself — the phase joins the existing single restore point.

**Existing imports** (main.rs:88-94) — extend these `ratatui::crossterm` re-export imports (never `crossterm` directly; it is not a direct dependency):

```rust
use ratatui::crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
```

Add: `event::{KeyboardEnhancementFlags, PushKeyboardEnhancementFlags, PopKeyboardEnhancementFlags}`, `terminal::supports_keyboard_enhancement`.

**Restore pattern to extend** (main.rs:98-106) — pop becomes the FIRST emission, conditional on a `static AtomicBool` (the panic hook is a `'static` closure and cannot see `App`):

```rust
fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(
        stdout(),
        DisableMouseCapture,
        DisableBracketedPaste,
        LeaveAlternateScreen
    );
}
```

Note every command is `let _ =` — failures during restore are swallowed, never propagated. Keep that discipline for the pop.

**Panic-hook wiring** (main.rs:386-390) — do not duplicate; the pop inherits this path for free by living inside `restore_terminal()`:

```rust
let default_hook = std::panic::take_hook();
std::panic::set_hook(Box::new(move |info| {
    restore_terminal();
    default_hook(info);
}));
```

**Init ordering** (main.rs:392-399): `enable_raw_mode()?` → `execute!(stdout(), EnterAlternateScreen, EnableBracketedPaste, EnableMouseCapture)?` → `Terminal::new`. The probe + push insert AFTER line 398 (after `EnterAlternateScreen`, so push and pop hit the same per-screen kitty stack) and BEFORE `run(...)` at 409 — the probe must own the event source, and nothing calls `event::poll`/`event::read` until `run`'s loop (main.rs:431-433).

**Normal-exit ordering** (main.rs:409-414): `run` result held, then `app.save(); app.kill_all(); restore_terminal();` then `result` propagated — error exits restore too. Nothing to change here; it routes through the same function.

**Startup-message pattern** (main.rs:404-407) — if the probe outcome should surface as a banner, follow the startup-notes precedent: `app.set_message(note)` before `app.restore()`.

---

### `baude/src/keys.rs` (utility, transform)

**Analog:** itself. The whole file is one pure function + two helpers; the phase rewrites exactly one arm and widens the context input.

**Current signature and modifier extraction** (keys.rs:6-12):

```rust
pub fn encode_key(key: &KeyEvent, app_cursor: bool) -> Vec<u8> {
    let mods = key.modifiers;
    let ctrl = mods.contains(KeyModifiers::CONTROL);
    let alt = mods.contains(KeyModifiers::ALT);
    let shift = mods.contains(KeyModifiers::SHIFT);
    let mut out: Vec<u8> = Vec::new();
```

The signature grows child context (RESEARCH recommends a context struct, e.g. `EncodeCtx { app_cursor, kitty_child, to_shell }` — planner's call; one call-site pair in `forward_key` either way).

**The arm being REPLACED** (keys.rs:43-52) — currently unconditional, the Pitfall-3 trap ("looks already done"):

```rust
KeyCode::Enter => {
    if shift && !alt && !ctrl {
        out.extend_from_slice(b"\x1b[13;2u");
    } else {
        if alt {
            out.push(0x1b);
        }
        out.push(b'\r');
    }
}
```

New shape per RESEARCH Q3 decision: `kitty_child → b"\x1b[13;2u"`, claude pane → `b"\x1b\r"`, shell pane → `b'\r'`. The `else` branch and every other arm stay byte-identical (TKEY-02).

**Context-conditional encoding precedent** (keys.rs:107-119, `cursor_key`) — this is the file's existing pattern for "child mode changes bytes", exactly what the new arm mirrors:

```rust
fn cursor_key(out: &mut Vec<u8>, ch: u8, mods: KeyModifiers, app_cursor: bool) {
    let m = modifier_code(mods);
    if m > 1 {
        out.extend_from_slice(format!("\x1b[1;{m}").as_bytes());
        out.push(ch);
    } else if app_cursor {
        out.extend_from_slice(b"\x1bO");
        out.push(ch);
    } else {
        out.extend_from_slice(b"\x1b[");
        out.push(ch);
    }
}
```

**Tests:** keys.rs has NO `#[cfg(test)]` module today (verified). Add one; corpus rows to freeze come straight from the arms at keys.rs:14-91 (Ctrl-chars table 17-27, Enter/Alt+Enter 43-52, Backspace 53-58, Tab/BackTab 59-60, Esc 61, arrows/Home/End via `cursor_key` both DECCKM states 62-67, tilde keys 68-71, F1-F12 72-86). Test-module shape precedent: `mod clipboard_tests` in `baude/src/app.rs:5754` (in-crate inline `#[cfg(test)]`, pure, no PTY).

---

### `baude/src/app.rs` (controller, event-driven)

**Analog:** itself — two self-precedents, one injection precedent.

**Key routing context** (app.rs:3860-3921): `handle_event` filters `KeyEventKind::Release` (app.rs:3862 — already correct for kitty; DISAMBIGUATE alone introduces no Release events), `handle_key` runs modal routing then global chords (Ctrl+Q, Ctrl+\, Alt+←/→, Ctrl+E, Ctrl+N, Ctrl+O at 3881-3915), then:

```rust
match self.focus {
    Focus::Sidebar => self.handle_sidebar_key(key),
    Focus::Claude => self.forward_key(key, false),
    Focus::Shell => self.forward_key(key, true),
}
```

`forward_key`'s `to_shell: bool` parameter (app.rs:3923) is already the pane discriminator the new encoding context needs.

**Per-child mode read — the DECCKM precedent to copy** (app.rs:3953-3958). Add `kitty_keyboard() != 0` beside `application_cursor()`, same lock-map-unwrap shape:

```rust
let app_cursor = pty
    .parser
    .lock()
    .map(|p| p.screen().application_cursor())
    .unwrap_or(false);
let bytes = encode_key(&key, app_cursor);
pty.write_input(&bytes);
```

**Remote-attach branch does the same through the mirror parser** (app.rs:3930-3942) — it must gain the identical `kitty_keyboard()` read or remote sessions diverge:

```rust
if let Some(SelId::Remote(id)) = self.selected_id {
    let Some(a) = &self.attach else { return };
    if a.remote_id != id {
        return;
    }
    let app_cursor = a
        .parser
        .lock()
        .map(|p| p.screen().application_cursor())
        .unwrap_or(false);
    a.write_input(&encode_key(&key, app_cursor));
    return;
}
```

**Import** (app.rs:29): `use crate::keys::encode_key;` — extend if a context struct is added.

**Non-fatal failure pattern** (app.rs:5688-5691, `activate_link`) — the established "failures never kill a session" shape, both arms resolve to `set_message`:

```rust
match open(url.as_str()) {
    Ok(()) => self.set_message(format!("opening {}", display_truncated(url))),
    Err(e) => self.set_message(format!("open failed: {e} — session unaffected")),
}
```

If App carries the probe outcome for help text, it is a plain field set once at startup (Claude's discretion per CONTEXT); the pop decision itself must NOT live on App (panic hook, see main.rs section).

---

### `baude/src/ui.rs` (component, render)

**Analog:** `Modal::Help` arm, ui.rs:2173-2229.

**Pattern** — a fixed `Paragraph::new(vec![Line...])` in a centered cyan-bordered block; sections are a bold `Span::styled` header followed by `Line::raw` rows with two-space-indented key column:

```rust
Modal::Help => {
    let rect = centered(area, 60, 35);
    frame.render_widget(Clear, rect);
    let dim = Style::default().fg(Color::DarkGray);
    let p = Paragraph::new(vec![
        Line::from(Span::styled(
            "select repository or checkout",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::raw("  j/k ↑/↓     select repository or checkout"),
        // ...
        Line::from(Span::styled("press any key to close", dim)),
    ])
```

The guidance line ("shift+enter: newline …") belongs in the `"global (any pane)"` section (ui.rs:2201-2211) as another `Line::raw`. The overlay height is fixed (`centered(area, 60, 35)` at ui.rs:2174) — adding lines may need the 35 bumped; check rendering. Help modal dismissal is any-key (app.rs:4399-4401).

---

### `vendor/vt100/src/screen.rs` (parser state, streaming)

**Analog:** three in-file precedents.

**The drop point being extended** (screen.rs:1635-1655) — `csi_dispatch`'s intermediate match. `Some(b'?')` handles only `J/K/h/l` (this is why the child's `CSI ? u` probe dies unanswered — do NOT change that, per RESEARCH anti-pattern); `Some(i)` catch-all debug-logs everything else, which is where the child's `CSI > u` / `CSI < u` / `CSI = u` vanish today:

```rust
Some(b'?') => match c {
    'J' => self.decsed(canonicalize_params_1(params, 0)),
    'K' => self.decsel(canonicalize_params_1(params, 0)),
    'h' => self.decset(params),
    'l' => self.decrst(params),
    _ => { /* debug log "unhandled csi sequence: CSI ? ..." */ }
},
Some(i) => { /* debug log — CSI >u / <u / =u land here today */ }
```

New arms for `Some(b'>')`, `Some(b'<')`, `Some(b'=')` matching `c == 'u'` slot in here; keep the debug-log fallthrough for other finals under those intermediates.

**Param-handling precedent** (screen.rs:1264-1308, `decset`) — canonicalized params, exhaustive match, debug-log default; `canonicalize_params_1(params, default)` is the established single-numeric-param helper (used throughout csi_dispatch, e.g. screen.rs:1609-1624):

```rust
fn decset(&mut self, params: &vte::Params) {
    for param in params {
        match param {
            &[1] => self.set_mode(MODE_APPLICATION_CURSOR),
            &[2004] => self.set_mode(MODE_BRACKETED_PASTE),
            ns => { /* debug log "unhandled DECSET mode" */ }
        }
    }
}
```

**Accessor precedent to mirror** (screen.rs:677-693) — `kitty_keyboard()` copies this shape (`#[must_use]`, doc comment, simple state read):

```rust
/// Returns whether the terminal should be in application cursor mode.
#[must_use]
pub fn application_cursor(&self) -> bool {
    self.mode(MODE_APPLICATION_CURSOR)
}
```

The mode bits live in a `u8` bitfield (`const MODE_*` at screen.rs:4-8, `set_mode`/`clear_mode`/`mode` at 783-793). The kitty state is a small stack, not a bit — a new `Vec<u16>`-or-array field on `Screen`, but the accessor surface mirrors the above.

**Fail-closed posture precedent** (screen.rs:1668-1702, the OSC 8 fork arm) — the security pattern the new handlers must copy: hard caps as named consts, attacker-controlled counts saturate, degradation is silent and safe. Kitty stack: cap depth at 32 (spec), saturating pop, no allocation proportional to params. Mark additions with a `// BAUDE FORK (kitty keyboard):` comment, matching the `// BAUDE FORK (OSC 8):` convention (screen.rs:1664).

---

### `vendor/vt100/tests/kitty_keyboard.rs` (test, streaming) — NEW FILE

**Analog:** `vendor/vt100/tests/hyperlink.rs` (whole file, 47 lines).

Copy its structure exactly — fork-banner doc comment, one named inner `mod`, pure `Parser::new` + `process(bytes)` + screen-state asserts, no fs/PTY:

```rust
//! BAUDE FORK (OSC 8): hyperlink behavior of the vendored parser.
//!
//! Pure parser tests — no fs, no PTY (same shape as baude's clipboard_tests).

mod hyperlink {
    #[test]
    fn target_not_label_and_wrap_survival() {
        let mut parser = vt100::Parser::new(4, 8, 50);
        parser.process(b"\x1b]8;;https://real.example/x\x1b\\click here\x1b]8;;\x1b\\done");
        let screen = parser.screen();
        // asserts on screen state ...
    }
}
```

Kitty tests feed `b"\x1b[>1u"` / `b"\x1b[<1u"` / `b"\x1b[=1;1u"` and assert `screen.kitty_keyboard()` (default 0, push/pop stack behavior, saturating pop with huge n, depth cap). Runs via `cargo test -p vt100`.

---

### `baude-core/src/pty.rs` (service, mode replay)

**Analog:** itself — `subscribe()`'s snapshot mode-replay list (pty.rs:390-422).

**The exact list to extend** (pty.rs:403-416) — replay an active child kitty push (`\x1b[>{flags}u`) alongside these so remote-attach mirror parsers converge on the same `kitty_keyboard()` state:

```rust
// Terminal modes aren't part of contents_formatted; replay
// the ones claude relies on.
if screen.application_cursor() {
    bytes.extend_from_slice(b"\x1b[?1h");
}
if screen.application_keypad() {
    bytes.extend_from_slice(b"\x1b=");
}
if screen.bracketed_paste() {
    bytes.extend_from_slice(b"\x1b[?2004h");
}
if screen.hide_cursor() {
    bytes.extend_from_slice(b"\x1b[?25l");
}
```

Pattern: literal escape bytes, condition on the accessor, append to the snapshot Vec. `write_input` (pty.rs:442-448) is untouched — `forward_key` already routes through it.

---

### `README.md` (docs)

**Analog:** the `## Keys` section (README.md:108-146).

Pattern: a `| Key | Where | Action |` table followed by short caveat prose — the existing terminal-configuration caveat (README.md:141-145, "alt+←/→ needs your terminal to send Option/Alt as a modifier — on macOS Terminal and iTerm2 enable this with …") is the exact register for the Shift+Enter supported-terminals + tmux extended-keys fallback guidance. Add a `shift+enter` row to the table plus a caveat paragraph (or small subsection) after it.

## Shared Patterns

### Injected side-effect seam (testability)
**Sources:** `baude-core/src/hook.rs:433-450` (`route_event`'s injected `post: FnOnce(&str, &str) -> bool`), `baude/src/app.rs:5612-5616` (`handle_link_hints_key<F, C>` injected open/copy sinks), production call site `baude/src/app.rs:4406-4408` (real fns injected at the one call site; tests pass recording closures).
**Apply to:** the negotiation seam in main.rs and the restore-sequence seam.

```rust
// hook.rs:433-435 — the canonical shape
pub fn route_event<F>(url: Option<&str>, sid: &str, line: &str, post: F)
where
    F: FnOnce(&str, &str) -> bool,
```
```rust
// app.rs:4406-4408 — production injects real effects; tests inject spies
Modal::LinkHints { .. } => {
    self.handle_link_hints_key(key, spawn_opener, Self::copy_to_clipboard)
}
```

RESEARCH Pattern 1 (`fn negotiate_keyboard(probe: impl FnOnce() -> io::Result<bool>) -> bool` with `supports_keyboard_enhancement` injected at the call site) is this pattern applied to the probe; `Err == unsupported == legacy`.

### Guard-style single restore point
**Source:** `baude/src/main.rs:98-106` (restore_terminal) + 386-390 (panic hook) + 413 (normal exit).
**Apply to:** keyboard-enhancement pop. One function, three callers already wired; the pop is a conditional first step gated on a `static AtomicBool` (use `swap(false, ..)` so a double restore emits the pop once). Pop ordered before `LeaveAlternateScreen` (per-screen kitty stacks).

### Failures never kill a session
**Sources:** `baude/src/app.rs:5688-5691` (`activate_link` → `set_message` on both arms), restore_terminal's `let _ =` discipline, hook.rs dispatch ("caller ALWAYS exits 0").
**Apply to:** probe `Err`/timeout (→ legacy, optional informational message), push failure (→ don't set the static), pop failure (ignored).

### Fail-closed bounded parsing (vt100 fork)
**Source:** `vendor/vt100/src/screen.rs:1668-1702` (OSC 8: named-const caps, silent safe degradation) and `canonicalize_params_1` usage throughout csi_dispatch.
**Apply to:** kitty stack handlers — depth cap 32, saturating pop, canonicalized numeric params.

### Byte-corpus / pure-state tests, no live spawns
**Sources:** `vendor/vt100/tests/hyperlink.rs` (pure parser byte-in/state-out), `baude/src/app.rs:5754+` (`mod clipboard_tests` inline `#[cfg(test)]` with closure spies), Phase 8 fixture PTY (`baude/Cargo.toml:32` — `baude-core = { workspace = true, features = ["test-support"] }` dev-dependency) for forward_key integration tests.
**Apply to:** keys.rs corpus module, main.rs seam tests, vt100 kitty tests, forward_key fixture tests.

## No Analog Found

None — every file in scope has an exact or in-file analog. Two near-gaps, both covered:

| Concern | Nearest precedent | Note |
|---------|-------------------|------|
| `static AtomicBool` process-global flag | none in baude/src (new) | Trivial stdlib idiom; forced by the panic hook's `'static` closure (RESEARCH Pitfall 5). RESEARCH Pattern 1/2 give the exact shape |
| keys.rs test module | `app.rs:5754` clipboard_tests (in-crate inline tests) | keys.rs itself has no tests today — Wave 0 gap per RESEARCH |

## Metadata

**Analog search scope:** `baude/src/` (main.rs, app.rs, keys.rs, ui.rs), `baude-core/src/` (pty.rs, hook.rs), `vendor/vt100/src/` (screen.rs), `vendor/vt100/tests/`, README.md
**Files scanned:** 9 read (targeted ranges), all cited paths git-tracked
**Pattern extraction date:** 2026-09-16
