# Phase 11: Negotiated Multiline Input - Research

**Researched:** 2026-09-16
**Domain:** Terminal keyboard-protocol negotiation (kitty CSI-u), PTY key forwarding, Rust/crossterm TUI
**Confidence:** HIGH (codebase + crossterm source verified this session; child-encoding evidence MEDIUM, cross-checked web)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### Capability Negotiation
- Detection is a real query of the outer terminal (kitty keyboard-protocol
  probe, e.g. crossterm's `supports_keyboard_enhancement`), never a TERM/env
  allowlist guess.
- The query is bounded: it races a short deadline at startup; on timeout the
  legacy path stands. Negotiation can never block startup or input
  indefinitely (TKEY-05).
- On an unsupported terminal, input behavior stays byte-identical to today
  and documented setup/fallback guidance is surfaced (help/README). baude
  never synthesizes a missing modifier (TKEY-03).
- Enhanced sequences flow only when BOTH ends are verified: the outer
  terminal reported the capability AND the child-input path is verified. The
  exact child encoding is a research question.

#### Mode Lifecycle
- Keyboard-enhancement flags are pushed only after successful negotiation and
  popped through the same guard-style restore path used for raw mode /
  alternate screen.
- Suspend pops the flags before handing the terminal back; resume re-pushes
  (no re-query — outer support cannot change mid-session).
- All controlled failure paths (normal exit, panic hook, error exits) restore
  keyboard mode at the same point the screen already restores (TKEY-04).
- Tests assert the pop sequence is emitted on every controlled path;
  real-terminal residue checking folds into Phase 12 validation.

#### Key Behavior
- The child encoding for Shift+Enter is decided by research from what Claude
  Code/claudex actually accept: enhanced passthrough when the child itself
  enabled enhanced mode on the PTY, else the documented fallback insert
  sequence. No hardcoding ahead of evidence.
- Regression guarantee is byte-for-byte: legacy mode forwards Enter, Ctrl-C,
  navigation keys, and existing baude chords unchanged; enhanced mode changes
  ONLY Shift+Enter (TKEY-02).
- Scope is PTY forwarding to Claude/claudex prompts only; baude's own input
  modal is untouched this phase.
- Documentation lands in the help overlay and README: supported terminals and
  fallback guidance.

### Claude's Discretion
- Exact probe implementation and deadline value.
- Where the negotiation result lives (per-App vs per-terminal state).
- The precise fallback guidance wording.

### Deferred Ideas (OUT OF SCOPE)
None — discussion stayed within phase scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| TKEY-01 | Shift+Enter inserts a newline without submitting in Claude/claudex prompts on documented, tested terminal paths | Child encoding decided: `ESC CR` (`\x1b\r`) fallback accepted by both Claude Code and OpenCode without negotiation; `\x1b[13;2u` passthrough when the child verifiably enabled kitty mode (see "Child Encoding Decision") |
| TKEY-02 | Ordinary Enter, Ctrl-C, navigation, existing chords unchanged after enhancement | baude is event-based, not byte-passthrough: `encode_key` (keys.rs) already re-encodes every KeyEvent to legacy bytes, so outer kitty mode cannot change child bytes for any key except the Shift+Enter arm being modified. Table-driven byte-for-byte corpus test locks this |
| TKEY-03 | Unsupported terminal keeps legacy behavior + documented guidance; no modifier guessing | On legacy outers, Shift+Enter arrives as plain Enter KeyEvent (no SHIFT bit) — legacy path is automatic, nothing to synthesize. Guidance goes in help overlay (ui.rs `Modal::Help`) + README |
| TKEY-04 | Keyboard mode restored on normal exit, controlled failure, suspend; re-established on resume | Single restore point exists: `restore_terminal()` main.rs:98-106, called from panic hook (386-390) and normal exit (413). Pop joins there. Suspend leg is VACUOUS today — baude has no Ctrl+Z/SIGTSTP suspend (verified; see "Suspend/Resume Finding") |
| TKEY-05 | Negotiation bounded; enhanced bytes only on verified outer+child path | crossterm 0.29's `supports_keyboard_enhancement()` is internally bounded at 2000 ms (source-verified); called once before the event loop starts. Child verification via new vt100-fork kitty-mode tracking |
</phase_requirements>

## Summary

Phase 11 is smaller and safer than the requirements suggest, because of one
architectural fact verified in source: **baude does not forward raw bytes from
the outer terminal — it consumes crossterm `KeyEvent`s and re-encodes them to
legacy bytes via `encode_key()` (baude/src/keys.rs) before writing to the child
PTY** [VERIFIED: baude/src/app.rs:3923-3964, baude/src/keys.rs:6-91]. Enabling
the kitty protocol on the outer terminal therefore changes only what crossterm
*parses* (it gains the SHIFT bit on Enter); every other key re-encodes to the
identical legacy bytes it produces today. TKEY-02's byte-for-byte guarantee
falls out of the existing design instead of fighting it.

Three findings shape the plan. First, crossterm 0.29.0 (in Cargo.lock, reachable
as `ratatui::crossterm` through ratatui 0.30.2) ships everything needed:
`terminal::supports_keyboard_enhancement()` (a real `CSI ? u` + DA1 probe,
internally bounded at 2000 ms, `Ok(false)` always on Windows), and
`PushKeyboardEnhancementFlags`/`PopKeyboardEnhancementFlags` commands
(`CSI > {bits} u` / `CSI < 1 u`). Second, `encode_key` already contains a
Shift+Enter arm emitting `\x1b[13;2u` unconditionally — currently near-dead code
(legacy outers never deliver SHIFT+Enter) and a latent misfire for users whose
tmux binding sends CSI-u into a non-kitty child; this phase's conditional
replaces it. Third, the child side: Claude Code inside baude will NOT enable
kitty on its PTY (it gates on a terminal-identity allow-list AND requires its
`CSI ? u` probe answered — baude's vt100 fork silently drops that query and
never replies), so the working fallback is `ESC CR` (`\x1b\r`), which Claude
Code treats as newline-insert with no negotiation (it is what its own
`/terminal-setup` installs for xterm.js terminals) and which OpenCode maps to
`input_newline` by default (`alt+return` binding). Enhanced `\x1b[13;2u`
passthrough applies only when the child observably pushed kitty flags — which
requires adding `CSI > u` / `CSI < u` tracking to the vendored vt100 fork,
mirroring the existing `application_cursor()` pattern.

**Primary recommendation:** Probe once at startup with crossterm's
`supports_keyboard_enhancement()` after `enable_raw_mode()`+`EnterAlternateScreen`;
on true, push `DISAMBIGUATE_ESCAPE_CODES` only and record it in a static; pop
first inside `restore_terminal()`. Change `encode_key`'s Shift+Enter arm to:
child-kitty-active → `\x1b[13;2u`; Claude pane otherwise → `\x1b\r`; shell pane →
plain `\r`. Track child kitty state in the vt100 fork.

## Key Findings by Research Question

### Q1 — Terminal init/restore path (exact map)

All in `baude/src/main.rs` [VERIFIED: baude/src/main.rs:88-106, 386-444]:

| Concern | Location | Detail |
|---------|----------|--------|
| Restore function | main.rs:98-106 | `restore_terminal()`: `disable_raw_mode()`, then `execute!(stdout(), DisableMouseCapture, DisableBracketedPaste, LeaveAlternateScreen)` |
| Panic hook | main.rs:386-390 | `std::panic::set_hook` closure calls `restore_terminal()` then the default hook |
| Init | main.rs:392-399 | `enable_raw_mode()?` then `execute!(stdout(), EnterAlternateScreen, EnableBracketedPaste, EnableMouseCapture)?`, then `ratatui::Terminal::new(CrosstermBackend::new(stdout()))` |
| Normal exit | main.rs:409-414 | `run(...)` returns → `app.save()`, `app.kill_all()`, `restore_terminal()` — result propagated after restore, so error exits restore too |
| Event loop | main.rs:421-444 | Poll-based: `event::poll(Duration::from_millis(50))` then a drain loop of `event::read()` → `app.handle_event(...)`. Single-threaded; nothing reads events before `run()` starts |

Verbatim restore body [VERIFIED: baude/src/main.rs:98-106]:

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

**Where Push/Pop join:** Push after main.rs:398 (after `EnterAlternateScreen`,
so flags land on the alternate screen's kitty stack), gated on the probe.
Pop becomes the FIRST emission inside `restore_terminal()` (before
`LeaveAlternateScreen`, so push and pop hit the same per-screen stack — kitty
keeps independent main/alt stacks [CITED: sw.kovidgoyal.net/kitty/keyboard-protocol]).
`restore_terminal()` takes no arguments and is captured by the panic-hook
closure, so the "did we push" flag must be a `static AtomicBool`, not App state.

**crossterm version:** 0.29.0 in Cargo.lock [VERIFIED: Cargo.lock, `name = "crossterm"` / `version = "0.29.0"`],
consumed via `ratatui::crossterm` (ratatui 0.30.2 full re-export — main.rs
already imports `ratatui::crossterm::terminal::{disable_raw_mode, ...}`).
It has both `supports_keyboard_enhancement` [VERIFIED: crossterm-0.29.0/src/terminal.rs:102,
src/terminal/sys/unix.rs:188-190] and the Push/Pop commands
[VERIFIED: crossterm-0.29.0/src/event.rs:493-527].

**Bounded probe feasibility:** yes. The unix implementation writes
`b"\x1B[?u\x1B[c"` to `/dev/tty` (stdout fallback) and loops on
`poll_internal(Some(Duration::from_millis(2000)), &KeyboardEnhancementFlagsFilter)`;
a DA1 reply without a flags reply returns `Ok(None)` → unsupported; total
silence returns `Err` at 2000 ms [VERIFIED: crossterm-0.29.0/src/terminal/sys/unix.rs:214-259,
verbatim: `poll_internal(Some(Duration::from_millis(2000)), &KeyboardEnhancementFlagsFilter)`].
The 2 s deadline is HARDCODED — not parameterizable. The internal event queue
retains non-matching events for the later `event::read` loop, so a user typing
during the probe loses nothing. Windows: `Ok(false)` unconditionally
[VERIFIED: crossterm-0.29.0/src/terminal/sys/windows.rs:73-77].

### Q2 — Key event flow (the design-deciding question)

**baude is event-based, NOT raw-byte passthrough.** Flow
[VERIFIED: baude/src/app.rs:3860-3964]:

1. `handle_event` (app.rs:3860-3867) — matches `Event::Key(key) if key.kind != KeyEventKind::Release`.
2. `handle_key` (app.rs:3869-3921) — modal routing first, then global chords
   (Ctrl+Q, Ctrl+\, Alt+Left/Right, Ctrl+E, Ctrl+N, Ctrl+O), then focus:
   `Focus::Claude => self.forward_key(key, false)`, `Focus::Shell => self.forward_key(key, true)`.
3. `forward_key` (app.rs:3923-3964) — reads child mode from the vt parser
   (`p.screen().application_cursor()`), then `let bytes = encode_key(&key, app_cursor); pty.write_input(&bytes);`.
   The remote-attach branch (app.rs:3930-3942) does the same through the attach
   mirror parser.
4. `encode_key` (keys.rs:6-91) — pure function KeyEvent → legacy VT bytes
   (DECCKM-aware). The Enter arm, verbatim [VERIFIED: baude/src/keys.rs:43-52]:

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

**Consequence:** enabling kitty on the outer terminal does NOT force baude to
translate anything back — crossterm's parser handles CSI-u decoding
unconditionally (with or without a push) and yields normal `KeyEvent`s
[VERIFIED: crossterm-0.29.0/src/event/sys/unix/parse.rs:203 `b'u' => return parse_csi_u_encoded_key_code(buffer)`;
keypad-Enter functional key `57414 => Some(KeyCode::Enter)` at parse.rs:413].
Ctrl+C arriving as `CSI 99;5u` becomes `Char('c')+CONTROL`, and `encode_key`
re-emits `0x03`. Esc arriving as `CSI 27 u` becomes `KeyCode::Esc` → `0x1b`.
The translation layer TKEY-02 needs already exists and is already tested by
production use.

Note the existing Shift+Enter arm already emits `\x1b[13;2u` — unconditionally,
including to a shell pane and to a child that never enabled kitty. This is the
one arm the phase rewrites (see Child Encoding Decision).

### Q3 — Child Encoding Decision (what Claude Code / OpenCode accept)

Evidence chain (all child-behavior claims MEDIUM — cross-checked web, multiple
independent sources; none verifiable by tool in this session):

- Claude Code gates its kitty push (`CSI > 1 u`) on a terminal-identity
  allow-list (`TERM_PROGRAM` ∈ {iTerm.app, kitty, WezTerm, ghostty}, or
  `KITTY_WINDOW_ID`), and reads CSI-u only after its `CSI ? u` probe is
  answered [CITED: github.com/anthropics/claude-code/issues/71700, issues/27868].
- baude's vt100 fork silently drops `CSI ? u` from the child (any CSI with a
  `?` intermediate other than h/l/J/K falls to a debug log; nothing ever writes
  a reply to the child PTY) [VERIFIED: vendor/vt100/src/screen.rs:1635-1645 —
  `Some(b'?')` arm handles only `'J' | 'K' | 'h' | 'l'`, default arm logs
  "unhandled csi sequence"]. Therefore Claude Code inside baude never gets its
  probe answered and never enables kitty on the inner PTY — regardless of the
  inherited `TERM_PROGRAM`. baude sets the child `TERM=xterm-256color`
  [VERIFIED: baude-core/src/pty.rs:156-157 `cmd.env("TERM", "xterm-256color");`].
- `ESC CR` (`\x1b\r`) is read by Claude Code as newline-insert WITHOUT any
  negotiation — it is the binding its own `/terminal-setup` installs for
  xterm.js-based terminals that cannot answer the kitty probe
  [CITED: code.claude.com/docs/en/terminal-config; dev.to/richardbray "Why Shift+Enter doesn't work in Claude Code";
  blog.fsck.com/agent-blog/2026/02/26/terminal-keyboard-protocol/ — "Coder's web terminal maps Shift+Enter to ESC CR, and Claude Code, OpenCode, and Hermes all insert a newline for those bytes"].
- OpenCode's default keybinds map `input_newline` to
  `shift+return, ctrl+return, alt+return, ctrl+j` — `alt+return` IS `ESC CR` in
  legacy encoding, and `ctrl+j` is `0x0a` [CITED: opencode.ai/docs/keybinds/;
  github.com/anomalyco/opencode issues #1941, #30544]. OpenCode's TUI
  (OpenTUI) uses the kitty protocol when the hosting terminal supports it and
  parses `\x1b[13;2u` as shift+return [CITED: github.com/anomalyco/opencode issue #2820].

**Decision (per locked decision, research decides):**

| Situation | Bytes baude writes for Shift+Enter | Why |
|-----------|-----------------------------------|-----|
| Child verifiably pushed kitty flags on the inner PTY | `\x1b[13;2u` | Native enhanced encoding; the child asked for it |
| Claude pane, child NOT kitty-active (the normal case for Claude Code today) | `\x1b\r` (ESC CR) | Accepted as newline-insert by Claude Code AND OpenCode with zero negotiation; identical to the Alt+Enter encoding `encode_key` already produces |
| Shell pane | `\r` (plain CR — Shift degrades to Enter) | Scope is Claude/claudex prompts only; `ESC CR` is meta-CR to readline and must not reach bash |

Do NOT answer the child's `CSI ? u` probe this phase. Replying would advertise
kitty support baude does not implement (a child enabling
`DISAMBIGUATE_ESCAPE_CODES` is then entitled to `CSI 27 u` for Esc,
`CSI 99;5u` for Ctrl+C, etc. — an emulation surface that directly endangers
TKEY-02). Observing pushes without advertising is the contained design.

### Q4 — Observing the child's kitty enable (verified-child mechanism)

The observation point is the vt100 fork's `csi_dispatch`
[VERIFIED: vendor/vt100/src/screen.rs:1606-1657]. Today every CSI with a `>`,
`<`, or `=` intermediate falls into the catch-all `Some(i)` arm and is only
debug-logged — the child's `CSI > flags u` (push), `CSI < n u` (pop), and
`CSI = flags ; mode u` (set) pass through the parser unobserved.

**Mechanism to build:** add kitty-keyboard tracking to the fork, exactly
mirroring the existing mode accessors (`application_cursor()`,
`bracketed_paste()`, `alternate_screen()` — the accessors `forward_key` and
`subscribe()` already consult [VERIFIED: baude/src/app.rs:3953-3957,
baude-core/src/pty.rs:398-416]):

- `Some(b'>')` + `'u'` → push flags onto a small stack (cap depth; kitty spec
  caps at 32 [CITED: sw.kovidgoyal.net/kitty/keyboard-protocol]).
- `Some(b'<')` + `'u'` → pop n (saturating — a huge or hostile n must not panic).
- `Some(b'=')` + `'u'` → set current flags.
- Expose `Screen::kitty_keyboard() -> u8` (current flags; nonzero = active).
- The fork already has an integration-test precedent dir
  (`vendor/vt100/tests/hyperlink.rs`, `link_fidelity.rs`) for byte-in/state-out
  tests [VERIFIED: ls vendor/vt100/tests].

`forward_key` then consults `p.screen().kitty_keyboard() != 0` beside
`application_cursor()`, and `subscribe()`'s mode-replay snapshot should replay
an active push (`\x1b[>{flags}u`) so remote-attach mirror parsers see the same
child state [VERIFIED: the replay list at baude-core/src/pty.rs:403-416 replays
application_cursor/keypad/bracketed_paste/hide_cursor the same way].

In practice today: Claude Code will never trip this (probe unanswered, see Q3);
OpenCode may, since OpenTUI can push without a successful probe [ASSUMED —
whether OpenCode pushes unconditionally inside baude is unverified]. Either
way the mechanism is correct: passthrough only when the push was observed.

### Q5 — Suspend/Resume Finding

**baude implements no suspend.** Verified by exhaustive grep: no `SIGTSTP`, no
signal handler, no suspend keyword in main.rs/app.rs beyond unrelated matches;
Ctrl+Z is not among the global chords (app.rs:3879-3915); in pane focus it
encodes to `0x1a` and forwards to the child; in sidebar focus plain `z` toggles
archived (app.rs:4333) and Ctrl+Z is unbound [VERIFIED: grep over
baude/src/main.rs, baude/src/app.rs]. Raw mode disables ISIG, so the terminal
driver never delivers SIGTSTP for a keypress.

**TKEY-04's suspend leg is therefore vacuous this phase.** Plan should: (a)
document this explicitly (in the phase docs and a code comment at the restore
point) — "baude has no suspend; if one is added, pop before handing the
terminal back and re-push without re-querying"; (b) NOT add a suspend handler
(out of scope — it would be a new feature, not a restore fix). An external
`kill -TSTP` today already stops baude without restoring raw mode or the alt
screen — pre-existing behavior for ALL terminal modes, unchanged by this phase.

### Q6 — Test strategy (see Validation Architecture for the full map)

- `encode_key` is a pure function with no test module today [VERIFIED: grep
  found no `#[cfg(test)]` in keys.rs] — add a table-driven byte-for-byte corpus
  (Enter, Alt+Enter, Ctrl+C, arrows in both DECCKM states, Tab/BackTab, F-keys,
  Ctrl-chords) asserting bytes UNCHANGED, plus the new Shift+Enter matrix
  (child-kitty × pane). The signature grows one input (child kitty state) —
  e.g. a small `EncodeCtx { app_cursor: bool, kitty_child: bool, to_shell: bool }`
  or equivalent; planner's choice.
- Negotiation seam: crossterm's probe needs a real tty — wrap it,
  `fn negotiate(probe: impl FnOnce() -> io::Result<bool>) -> bool`, injected at
  the call site exactly like Phase 10's `spawn_opener` injection
  [VERIFIED: baude/src/app.rs:4402-4407 production call site injects
  `spawn_opener`, tests pass recording closures].
- Restore-path assertion: extract the restore byte sequence into a testable
  seam (e.g. `fn restore_sequence(enhanced: bool) -> Vec<u8>` or a
  write-to-`impl Write` function that `restore_terminal()` drives with real
  stdout). Tests assert `\x1b[<1u` present iff pushed, ordered before
  `LeaveAlternateScreen`'s `\x1b[?1049l`, on the one function both the panic
  hook and normal exit call.
- Child-mode observation: vt100-fork unit/integration tests feeding
  `\x1b[>1u` / `\x1b[<1u` / `\x1b[=1;1u` and asserting `kitty_keyboard()`,
  following `vendor/vt100/tests/hyperlink.rs` style.
- Forwarding tests through the fixture PTY under Phase 8 isolation
  (`test-support` feature, already a dev-dependency of baude
  [VERIFIED: baude/Cargo.toml:32 `baude-core = { workspace = true, features = ["test-support"] }`]).

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Outer-terminal capability probe + push/pop | main.rs startup/restore (process lifecycle) | — | Must bracket the same lifecycle as raw mode/alt screen; panic hook lives here |
| Negotiation result storage | `static AtomicBool` (pushed?) + App field (probe outcome for UI/help) | — | Panic hook is a `'static` closure that cannot see App; UI wants the outcome for guidance |
| KeyEvent → child bytes | `keys.rs::encode_key` (pure) | `app.rs::forward_key` (context supplier) | Purity keeps byte-for-byte tests trivial; forward_key already supplies per-child mode (DECCKM precedent) |
| Child kitty-mode observation | vendored vt100 fork (`Screen`) | `pty.rs::subscribe` (replay) | Same pattern as `application_cursor()`; remote attach mirrors need replay |
| Fallback guidance text | `ui.rs` help overlay + README | — | CONTEXT locked: help overlay and README |

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| crossterm (via `ratatui::crossterm`) | 0.29.0 [VERIFIED: Cargo.lock] | Probe (`supports_keyboard_enhancement`), Push/Pop commands, CSI-u input parsing | Already the event backend; probe + commands + parser all present in the locked version |
| ratatui | 0.30.2 [VERIFIED: Cargo.lock] | Re-exports crossterm; TUI | Existing |
| vendored vt100 fork | workspace path | Child output parsing; gains kitty-mode tracking | Already forked for OSC 8; the mode-accessor pattern exists |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| portable-pty (existing) | in lock | Child PTY | Untouched |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| crossterm's fixed 2 s probe | Hand-rolled probe with shorter deadline | NOT feasible: the `CSI ? u` reply surfaces only as crossterm's `InternalEvent::KeyboardEnhancementFlags`, filtered by `pub(crate)` internals — the public `event::read` API never exposes it [VERIFIED: crossterm-0.29.0/src/terminal/sys/unix.rs:214-218 imports `poll_internal, read_internal, InternalEvent` from crate-private modules]. Accept the 2 s internal bound (satisfies "bounded"); the slow path (a terminal answering neither `CSI ? u` nor DA1) is vanishingly rare since DA1 is universal |
| `\x1b\r` fallback | `\n` (0x0a, Ctrl+J) | Ctrl+J also inserts a newline in both children, but `0x0a` doubles as Enter in many line disciplines and TUIs — higher mis-submit risk. `ESC CR` matches the Alt+Enter encoding `encode_key` already emits |
| Not answering child `CSI ? u` | Reply and emulate kitty on the inner PTY | Full inner-PTY kitty emulation (Esc→`CSI 27u`, Ctrl+C→`CSI 99;5u`, …) once the child enables disambiguate — large TKEY-02 risk for zero gain this phase |

**Installation:** none — no new dependencies. `cargo 1.98.1` / `rustc 1.98.1`
verified on this machine.

## Package Legitimacy Audit

No external packages are installed by this phase. All work uses crates already
in Cargo.lock (crossterm 0.29.0, ratatui 0.30.2) and the in-repo vt100 fork.

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

## Architecture Patterns

### System Architecture Diagram

```
 user keyboard
      │
      ▼
 outer terminal ──(startup: baude probes CSI ? u; on support pushes CSI > 1 u)──┐
      │  kitty ON: Shift+Enter → \x1b[13;2u        kitty OFF: Shift+Enter → \r  │
      ▼                                                                          │
 crossterm parser (parses CSI-u ALWAYS) → KeyEvent{Enter, SHIFT?}                │
      ▼                                                                          │
 app.handle_event → handle_key ──(modal? chords? focus)──► forward_key           │
      ▼                                                                          │
 encode_key(ctx: app_cursor, kitty_child, to_shell)   ◄── vt100 fork Screen      │
      │  Shift+Enter:                                      (kitty_keyboard(),    │
      │   kitty_child → \x1b[13;2u                          application_cursor())│
      │   claude pane → \x1b\r                                   ▲               │
      │   shell pane  → \r                                       │               │
      ▼                                                          │               │
 pty.write_input(bytes) ──► child PTY (claude / opencode) ──output──► vt parser  │
                                                                                 │
 exit / panic / error ──► restore_terminal(): [CSI < 1 u if pushed] ◄────────────┘
                          → DisableMouseCapture → DisableBracketedPaste
                          → LeaveAlternateScreen → disable_raw_mode
```

### Recommended Project Structure

No new files required beyond tests; changes land in existing modules:

```
baude/src/main.rs        # probe call, push, static flag, pop in restore_terminal (+ seam)
baude/src/keys.rs        # encode_key Shift+Enter arm + EncodeCtx + byte corpus tests
baude/src/app.rs         # forward_key supplies kitty_child/to_shell; App stores probe outcome
baude/src/ui.rs          # help overlay guidance line(s)
vendor/vt100/src/screen.rs  # CSI >u/<u/=u tracking + kitty_keyboard() accessor
vendor/vt100/tests/      # kitty-mode tracking tests (hyperlink.rs precedent)
baude-core/src/pty.rs    # subscribe() replay of active kitty push
README.md                # supported terminals + fallback guidance
```

### Pattern 1: Bounded startup negotiation behind a seam

**What:** One probe, once, between terminal init and the event loop; injected
closure for tests.
**When to use:** main.rs:399 (after `EnterAlternateScreen`, before `run`).
**Example:**

```rust
// Pattern only — planner refines. Source: crossterm 0.29 docs + verified source.
use ratatui::crossterm::event::{KeyboardEnhancementFlags, PushKeyboardEnhancementFlags};
use ratatui::crossterm::terminal::supports_keyboard_enhancement;

static KEYBOARD_ENHANCED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Probe is injected so tests never need a tty (spawn_opener precedent).
fn negotiate_keyboard(probe: impl FnOnce() -> std::io::Result<bool>) -> bool {
    // Err (2 s internal timeout, no tty, Windows) == unsupported: legacy stands.
    matches!(probe(), Ok(true))
}

// call site, after EnterAlternateScreen:
if negotiate_keyboard(supports_keyboard_enhancement) {
    // DISAMBIGUATE only: Shift+Enter becomes CSI 13;2u; unmodified Enter/Tab/
    // Backspace stay legacy; no Release/Repeat kinds are introduced.
    if execute!(
        stdout(),
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    )
    .is_ok()
    {
        KEYBOARD_ENHANCED.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
```

### Pattern 2: Pop through the existing single restore point

```rust
// restore_terminal() gains ONE new first step; panic hook + normal exit both
// already route here (main.rs:386-390, 413).
fn restore_terminal() {
    if KEYBOARD_ENHANCED.swap(false, Ordering::Relaxed) {
        let _ = execute!(stdout(), PopKeyboardEnhancementFlags); // emits \x1b[<1u
    }
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), DisableMouseCapture, DisableBracketedPaste, LeaveAlternateScreen);
}
```

Pop BEFORE `LeaveAlternateScreen`: kitty maintains independent flag stacks for
main and alternate screens, so the pop must hit the stack the push landed on
[CITED: sw.kovidgoyal.net/kitty/keyboard-protocol]. Conditional (not
unconditional) pop keeps legacy-terminal output byte-identical to today.

### Pattern 3: Child-mode-aware encoding (DECCKM precedent)

`forward_key` already reads `screen().application_cursor()` per keystroke and
passes it to `encode_key` [VERIFIED: baude/src/app.rs:3953-3958]. Add
`screen().kitty_keyboard() != 0` the same way. Keep `encode_key` pure.

### Anti-Patterns to Avoid

- **Answering the child's `CSI ? u` probe:** advertises an emulation baude does
  not implement; the child may then legitimately expect CSI-u for Esc/Ctrl keys.
- **Pushing more flags than `DISAMBIGUATE_ESCAPE_CODES`:** `REPORT_EVENT_TYPES`
  introduces Repeat/Release kinds and `REPORT_ALL_KEYS_AS_ESCAPE_CODES` changes
  every key's wire form — pure risk, zero requirement.
- **TERM/env allow-list detection:** explicitly forbidden by locked decision.
- **Probing after the event loop starts:** `supports_keyboard_enhancement`
  "will block ... while `event::read`/`event::poll` are being called" — it must
  own the event source; safe only in the single-threaded pre-loop window
  [VERIFIED: doc comment, crossterm-0.29.0/src/terminal/sys/unix.rs:183-186].
- **Sending `\x1b\r` to the shell pane:** meta-CR in readline; degrade
  Shift+Enter to plain `\r` there.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Capability probe | Custom `CSI ? u` write + raw read race | `crossterm::terminal::supports_keyboard_enhancement` | The reply is only surfaced through crate-private internals; a hand-rolled reader would race crossterm's own buffer |
| CSI-u input decoding | Parsing `\x1b[13;2u` from stdin | crossterm's parser (already does it, always) | Verified: parse.rs:203, 497+ |
| Push/pop byte emission | Literal escape strings | `PushKeyboardEnhancementFlags` / `PopKeyboardEnhancementFlags` commands | Byte-exact (`CSI > bits u` / `CSI < 1 u`), Windows-guarded |
| Modifier synthesis on legacy terminals | Timing heuristics on Enter | nothing — locked decision forbids it | TKEY-03 hard line |

**Key insight:** the entire outer-terminal half of this phase is three
already-shipped crossterm calls placed at the right lifecycle points; the real
new code is the child-side observation (vt100 fork) and the one `encode_key` arm.

## Common Pitfalls

### Pitfall 1: Treating the probe deadline as tunable
**What goes wrong:** Plan assumes a "short deadline" parameter exists.
**Why it happens:** CONTEXT leaves the deadline to discretion.
**How to avoid:** crossterm's 2000 ms is hardcoded and non-bypassable (private
internals). Accept it as the bound; document that the common no-support path
returns fast via the DA1 reply, and only a terminal answering neither query
hits 2 s.
**Warning signs:** any plan task that says "make the timeout configurable."

### Pitfall 2: Push/pop on different kitty stacks
**What goes wrong:** flags pushed before `EnterAlternateScreen` (main stack)
but popped after entering/before leaving (alt stack) → mode leaks on terminals
implementing per-screen stacks.
**How to avoid:** push after Enter, pop before Leave (Patterns 1–2).
**Warning signs:** Shift+Enter still enhanced in the shell after baude exits.

### Pitfall 3: Existing `\x1b[13;2u` arm misread as "already done"
**What goes wrong:** keys.rs:43-52 looks like TKEY-01 shipped. It is (a)
unreachable on legacy outers (SHIFT never reported on Enter), (b) wrong for the
shell pane, (c) wrong for a non-kitty child when a tmux `S-Enter` binding
injects CSI-u from outside.
**How to avoid:** the phase REPLACES this arm with the conditional; the legacy
(“else”) branches of every other key stay byte-identical.
**Warning signs:** a plan that only adds the outer push and keeps this arm as-is.

### Pitfall 4: Byte-identity broken by over-flagging
**What goes wrong:** pushing `REPORT_EVENT_TYPES`/`REPORT_ALL_KEYS...` changes
what crossterm delivers (Repeat/Release kinds, CSI-u for unmodified keys);
subtle chord regressions follow.
**How to avoid:** `DISAMBIGUATE_ESCAPE_CODES` only. `handle_event` already
filters `Release`, and no Release/Repeat kinds are generated under disambiguate
alone.

### Pitfall 5: Probe result read from App inside the panic hook
**What goes wrong:** the panic hook closure is installed before `App::new` and
cannot borrow App; storing "pushed" only on App makes the panic-path pop
impossible.
**How to avoid:** `static AtomicBool` for the pop decision; App may additionally
carry the probe outcome for help-overlay text.

### Pitfall 6: Multiplexers and remote attach
**What goes wrong:** tmux/screen/zellij between user and baude answer (or eat)
the probe themselves; enhanced support then reflects the mux, not the terminal.
**How to avoid:** nothing to code — the probe result is authoritative for
whatever baude's stdout talks to, which is exactly the negotiation contract.
Document tmux extended-keys guidance in the README fallback section. For the
remote-attach TUI, the attach-side baude process runs the same startup
negotiation on its own terminal; child kitty state rides the mirror parser
(replay the push in `subscribe()`'s snapshot).

### Pitfall 7: `Err` from the probe treated as fatal
**What goes wrong:** `supports_keyboard_enhancement` returns `Err` on timeout
or missing tty; propagating it kills startup.
**How to avoid:** `Err == unsupported == legacy` (Pattern 1); failures never
kill a session (established project pattern).

## Code Examples

### Rewritten Shift+Enter arm (shape only — planner owns final signature)

```rust
// keys.rs — Source: this research (Q3 decision); legacy arms byte-identical.
KeyCode::Enter => {
    if shift && !alt && !ctrl {
        match ctx.child_newline {
            ChildNewline::KittyActive => out.extend_from_slice(b"\x1b[13;2u"),
            ChildNewline::ClaudePane => out.extend_from_slice(b"\x1b\r"),
            ChildNewline::ShellPane => out.push(b'\r'),
        }
    } else {
        if alt { out.push(0x1b); }
        out.push(b'\r');
    }
}
```

### vt100 fork tracking (csi_dispatch additions)

```rust
// vendor/vt100/src/screen.rs — Source: kitty keyboard protocol spec +
// existing decset/decrst pattern in this file.
Some(b'>') if c == 'u' => self.kitty_push(canonicalize_params_1(params, 1)),
Some(b'<') if c == 'u' => self.kitty_pop(canonicalize_params_1(params, 1)), // saturating
Some(b'=') if c == 'u' => self.kitty_set(params),                            // flags;mode
// Screen accessor mirrors application_cursor():
pub fn kitty_keyboard(&self) -> u16 { self.kitty_stack.last().copied().unwrap_or(0) }
```

### Byte-for-byte corpus test shape

```rust
// keys.rs tests — every (event, ctx) → expected bytes; legacy rows are frozen.
#[test]
fn legacy_bytes_unchanged() {
    for (ev, expected) in legacy_corpus() {
        assert_eq!(encode_key(&ev, legacy_ctx()), expected, "{ev:?}");
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Terminals send `\r` for Enter and Shift+Enter alike | kitty keyboard protocol / CSI-u disambiguation, negotiated per-program | kitty spec 2021→broad adoption 2024-2026 (Ghostty, iTerm2, WezTerm, foot, Alacritty; xterm.js only in 6.1.0-beta) | Negotiation is the correct, now-mainstream mechanism; allow-lists are the discredited alternative (Claude Code's own allow-list gating is a filed bug, #71700) |
| `/terminal-setup` keybinding hacks per terminal | Native Shift+Enter in kitty-capable terminals; `ESC CR` as the no-negotiation fallback | Claude Code 2025-2026 | `ESC CR` is the stable, documented fallback both children accept |

**Deprecated/outdated:** TERM-based capability guessing (explicitly forbidden
by locked decision, and empirically broken — Alacritty reports
`TERM=alacritty` with full CSI-u support).

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Claude Code inserts a newline (does not submit / mis-echo) on receiving `\x1b\r` at its prompt, in the version users run | Q3 / Child Encoding | Shift+Enter falls back to Alt+Enter semantics; if a future Claude Code drops ESC CR handling, Shift+Enter would do whatever Alt+Enter does. Mitigation: SHIP-03 real-terminal smoke (Phase 12) exercises exactly this |
| A2 | OpenCode maps `alt+return` (`ESC CR`) to `input_newline` by default in the shipped version | Q3 | Same fallback risk for the opencode backend; its `ctrl+j` default is a second fallback lever |
| A3 | OpenCode/OpenTUI may push kitty flags on the inner PTY without a successful probe reply (making the passthrough branch reachable) | Q4 | If it never pushes, the enhanced branch is simply dormant — fallback still works; no user-visible harm |
| A4 | Claude Code will not enable kitty on the inner PTY because its probe goes unanswered, even with an allow-listed `TERM_PROGRAM` inherited | Q3/Q4 | If a Claude Code version pushes WITHOUT probe confirmation, baude's new observation mechanism catches it and passthrough applies — the design self-corrects |
| A5 | Unrecognized `CSI < 1 u` written to a non-kitty terminal is silently ignored (relevant only if planner chooses unconditional pop; recommended design pops conditionally, making this moot) | Patterns | Cosmetic garbage on exit in exotic terminals — avoided entirely by the conditional pop |

## Open Questions (RESOLVED)

1. **Exact `EncodeCtx` shape and where `to_shell` folds in**
   - What we know: `forward_key` already distinguishes shell vs claude and reads per-child parser modes.
   - What's unclear: whether to widen `encode_key`'s signature or introduce a context struct.
   - Recommendation: context struct — one call-site change, table tests stay flat. Planner's call.
2. **Help-overlay wording and README placement** — Claude's discretion per CONTEXT; suggest one line in the help overlay ("Shift+Enter: newline (auto-detected; see README for terminal setup)") and a README subsection listing verified terminals (Ghostty, kitty, iTerm2, WezTerm, foot, Alacritty) plus tmux extended-keys guidance.
3. **Replay of child kitty state in `subscribe()`** — recommended for mirror-parser correctness; strictly needed only when a remote child pushed kitty. Low cost; include unless plan pressure demands deferral.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| cargo/rustc | build + tests | ✓ | 1.98.1 / 1.98.1 | — |
| crossterm 0.29.0 | probe, push/pop, CSI-u parse | ✓ (Cargo.lock, registry source present locally) | 0.29.0 | — |
| Real kitty-capable terminal | live smoke only | not needed this phase | — | Deferred to Phase 12 SHIP-03 per CONTEXT ("real-terminal residue checking folds into Phase 12") |

**Missing dependencies with no fallback:** none.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test harness (`cargo test`), inline `#[cfg(test)]` modules + `vendor/vt100/tests/` integration tests |
| Config file | none (workspace Cargo.toml) |
| Quick run command | `cargo test -p baude keys` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| TKEY-01 | Shift+Enter → `\x1b\r` (claude pane, no child kitty) / `\x1b[13;2u` (child kitty) | unit | `cargo test -p baude keys` | ❌ Wave 0 (keys.rs has no test module) |
| TKEY-02 | Legacy corpus byte-identical (Enter, Ctrl-C, arrows both DECCKM states, chords) | unit | `cargo test -p baude keys` | ❌ Wave 0 |
| TKEY-02 | forward_key writes exact bytes to fixture PTY (both panes, remote branch) | integration (test-support) | `cargo test -p baude forward` | ❌ Wave 0 |
| TKEY-03 | Probe-false/Err ⇒ no push, encoding legacy; guidance text present in help overlay | unit | `cargo test -p baude negotiate` + `cargo test -p baude ui` | ❌ Wave 0 |
| TKEY-04 | Restore sequence contains `\x1b[<1u` iff pushed, ordered before alt-screen leave; same function serves panic hook and exit | unit (seam) | `cargo test -p baude restore` | ❌ Wave 0 |
| TKEY-05 | Injected-probe seam: timeout/Err → legacy without blocking; child-verification gate on `kitty_keyboard()` | unit | `cargo test -p baude negotiate` + `cargo test -p vt100` | ❌ Wave 0 |
| TKEY-01/05 | vt100 fork tracks `CSI >u/<u/=u`, saturating pop, accessor default 0 | integration | `cargo test -p vt100` | ❌ Wave 0 (new test file beside hyperlink.rs) |

### Sampling Rate
- **Per task commit:** `cargo test -p baude keys` (plus the touched crate's tests)
- **Per wave merge:** `cargo test --workspace && cargo fmt --check && cargo clippy --workspace`
- **Phase gate:** full suite green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `baude/src/keys.rs` `#[cfg(test)]` module — legacy corpus + Shift+Enter matrix (TKEY-01/02)
- [ ] negotiation/restore seam tests in `baude/src/main.rs` (or a new small module) (TKEY-03/04/05)
- [ ] `vendor/vt100/tests/kitty_keyboard.rs` (TKEY-01/05)
- Framework install: none needed.

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | — |
| V3 Session Management | no | — |
| V4 Access Control | no | — |
| V5 Input Validation | yes | vte-based bounded parsing in the vt100 fork; numeric CSI params canonicalized; new kitty-stack handlers must saturate (bounded stack depth ≤ 32, saturating pop count) |
| V6 Cryptography | no | — |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Malicious child output drives kitty-stack state (deep pushes, huge pop counts) | DoS/Tampering | Cap stack depth, saturating arithmetic, no allocation proportional to attacker params — same fail-closed posture as the fork's OSC 8 handling (screen.rs:1668-1702 precedent) |
| Escape-sequence residue on the outer terminal after crash | Tampering (terminal state) | Pop through the single restore point incl. panic hook (TKEY-04); Phase 12 residue smoke |
| Enhanced bytes sent to a child that never negotiated | Tampering (input spoofing) | The verified-child gate: passthrough only after an observed push (TKEY-05) |

## Project Constraints (from CLAUDE.md)

Repo `./CLAUDE.md`: none found at repo root (project instructions live in
`.planning/` docs and global user CLAUDE.md; nothing phase-constraining beyond
GSD process). Established repo patterns that bind this phase (from CONTEXT +
verified code): failures never kill a session (`set_message` warnings);
guard-style single-restore-point terminal management; byte-level regression
tests with injected seams, never live spawns.

## Sources

### Primary (HIGH confidence — read this session)
- baude codebase: `baude/src/main.rs` (88-106, 386-444), `baude/src/app.rs` (3860-4034, 4402-4407), `baude/src/keys.rs` (6-128), `baude-core/src/pty.rs` (156-157, 335-448), `vendor/vt100/src/screen.rs` (1586-1710), Cargo.lock
- crossterm 0.29.0 registry source: `src/terminal/sys/unix.rs` (183-259), `src/terminal/sys/windows.rs` (73-77), `src/event.rs` (493-527), `src/event/sys/unix/parse.rs` (203, 413, 497+, tests 1215-1277)
- kitty keyboard protocol spec: sw.kovidgoyal.net/kitty/keyboard-protocol (detection, per-screen stacks, push/pop semantics)

### Secondary (MEDIUM confidence — cross-checked WebSearch, ≥2 independent sources)
- Claude Code allow-list gating + probe requirement: github.com/anthropics/claude-code issues #71700, #27868
- `ESC CR` accepted as newline by Claude Code/OpenCode without negotiation: code.claude.com/docs/en/terminal-config; dev.to (richardbray); blog.fsck.com terminal-keyboard-protocol post; wmedia.es; JetBrains IJPL-221848
- OpenCode default keybinds (`input_newline: shift+return, ctrl+return, alt+return, ctrl+j`) and kitty usage: opencode.ai/docs/keybinds/; github.com/anomalyco/opencode #1941, #2820, #30544

### Tertiary (LOW confidence)
- tmux `S-Enter`/extended-keys workarounds: gist.github.com/jftuga (README guidance input only)

## Metadata

**Confidence breakdown:**
- Terminal init/restore + key-flow map: HIGH — every claim read from source this session with line ranges
- crossterm API/bytes/bounds: HIGH — registry source read directly
- Child encoding (`ESC CR` fallback): MEDIUM — multiple independent secondary sources agree; not executable-verified here; gated by A1/A2 and covered by Phase 12 SHIP-03 smoke
- Pitfalls: HIGH for codebase-derived, MEDIUM for terminal-ecosystem behavior

**Research date:** 2026-09-16
**Valid until:** ~2026-10-16 (crossterm/ratatui pinned by lockfile; Claude Code/OpenCode input handling is the fast-moving edge — re-verify A1/A2 at Phase 12 smoke)

> RESOLVED dispositions: Q1 (EncodeCtx shape) — plan 11-01 assumption_delta promotes the struct; Q2 (guidance wording/placement) — plan 11-03 Tasks 1-2; Q3 (subscribe kitty replay) — plan 11-04 Task 2 includes it.
