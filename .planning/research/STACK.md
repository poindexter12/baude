# Stack Research

**Domain:** Reliability fixes and terminal usability in the existing Rust ratatui/VT application
**Researched:** 2026-09-08
**Confidence:** MEDIUM

## Recommendation

Keep the current terminal stack and add no general-purpose dependency. The only stack-level change justified by the scope is a small, pinned `vt100` fork/patch based on 0.15.2 that retains OSC 8 hyperlink state per terminal cell (including scrollback/overwrite behavior) and exposes it to the existing renderer. Do not replace the parser with `alacritty_terminal`, `justerm`, or another terminal engine, and do not assume upgrading `vt100` or `ratatui` supplies links.

Use the existing `crossterm` 0.29.0 keyboard-enhancement API, gated by its support query, to negotiate modified Enter. Keep a legacy path: when negotiation is unavailable or a terminal reports plain CR for both Enter variants, Shift+Enter is not distinguishable and must not be guessed.

## Recommended Stack

### Core Technologies

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| Rust standard library | Existing Edition 2021 toolchain | Hook idempotency, lock diagnostics, fixture paths, URL validation, platform launcher | All five reliability items are filesystem/process/state-boundary work; `std::process::Command` with separate arguments avoids shell expansion. |
| `ratatui` | 0.30.2 locked | Existing frame, buffer, selection, and terminal lifecycle | `Buffer::Cell` has symbol/style/diff fields but no link metadata. An upgrade is not a link solution and would expand regression scope. |
| `crossterm` (via ratatui) | 0.29.0 locked | Raw mode, event input, keyboard enhancement negotiation | `PushKeyboardEnhancementFlags`, `PopKeyboardEnhancementFlags`, `DISAMBIGUATE_ESCAPE_CODES`, and `supports_keyboard_enhancement()` are already the targeted APIs. |
| `vt100` with a local targeted patch | 0.15.2 API baseline; upstream latest docs 0.16.2 | Parse PTY output while retaining OSC 8 target metadata | Current `Cell` exposes contents and visual attributes only. A parser-level cell attribute is the least disruptive way to support both local and remote parsers. |

### Supporting Libraries

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `serde` / `serde_json` | Existing lockfile versions | Structured hook/settings and lock-owner diagnostics where already used | Preserve existing persistence boundaries; no new persistence crate. |
| Existing `dirs`, `ureq`, and `url` lockfile presence | Existing | Existing paths/network code; not a reason to add a dependency | Use only where already part of a product boundary. URL opening itself needs no HTTP client. |

### Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| Rust standard-library fixture directories | Isolate Git worktree tests | Unique per test using a temp root plus process/atomic uniqueness; pass paths explicitly rather than mutating global HOME/XDG. |
| Existing Cargo CI gates | Validate the patch | Keep `fmt`, `clippy -D warnings`, workspace tests, and terminal smoke tests. Do not install or run a second terminal stack. |

## Installation

No new crates are recommended. The `vt100` change should be a pinned workspace patch/fork, not an unreviewed floating Git dependency. Keep the lockfile stable except for that intentional patch.

```bash
# No installation. Existing workspace dependencies remain in place.
```

## Alternatives Considered

| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|-------------------------|
| Targeted `vt100` fork/patch plus a thin ratatui presenter bridge | Upgrade `vt100` to 0.16.2 | Only for unrelated parser fixes; current 0.16.2 docs still show no hyperlink cell API and it changes `vte`/unicode-width dependencies. |
| Targeted `vt100` fork/patch | `alacritty_terminal`, `justerm`, or another full engine | Only if baude intentionally redesigns PTY, scrollback, selection, and rendering ownership. That is outside v2.2. |
| Existing ratatui 0.30.2 with a narrow presenter bridge | Fork/upgrade ratatui for a general link field | Only if links become a reusable application-wide widget feature; current `Cell` has no metadata slot, so avoid framework-wide change for one terminal pane. |
| `crossterm` 0.29 keyboard enhancement | Hand-emitting/querying CSI-u only | Use only if crossterm cannot surface a needed event. Its support query and push/pop already cover negotiation and restoration. |

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| `osc8`, URL-parser, opener, tempfile, file-lock, or test-env crates solely for this milestone | Adds dependencies without solving ratatui/vt100 metadata integration; syntax, validation, launch, and fixture isolation are narrow enough for existing code and `std`. | Targeted parser/presenter patch and standard-library helpers. |
| Shell-based URL opening (`sh -c`, `open "$url"`, `xdg-open` through a shell) | User-controlled or PTY-derived text can become shell syntax; it violates the no-evaluation requirement. | Strict `http`/`https` validation, then `Command::new(platform_launcher).arg(url)`. Opening is only on an explicit user gesture. |
| Automatic opening of OSC 8 `file:`, `ssh:`, `mailto:`, or arbitrary schemes | Remote PTY output is untrusted and OSC 8 permits arbitrary URI schemes. | Expose/retain metadata but permit baude's opener only for absolute HTTP(S), failing closed. |
| Blind `PushKeyboardEnhancementFlags` without cleanup | Leaves terminal keyboard mode changed after panic, suspend, or early return. | Support-gated push with a scoped cleanup path and pop before leaving alternate screen/raw mode. |
| Treating `CSI 13;2u` as universally available | Kitty's protocol is opt-in and terminal/OS input paths differ; legacy CR cannot identify Shift+Enter. | Use crossterm support negotiation, parse the reported modifier, and retain Enter fallback when absent. |
| Process-global environment mutation in parallel tests | `std::env` is global and races can contaminate unrelated fixture/worktree tests. | Explicit injectable roots, unique fixture directories, and a narrowly serialized environment test lane if unavoidable. |

## Stack Patterns by Variant

**If OSC 8 is present in a PTY stream:**
- The patched parser tracks the active target and copies it into cells/runs; the presenter emits OSC 8 around contiguous runs while still rendering ordinary text through ratatui.
- The same `baude-core` parser type must serve `pty.rs` local sessions and `remote.rs` WebSocket PTY sessions.
- Plain URL detection is a separate, lower-priority metadata pass over visible text. It must not replace explicit OSC 8 labels.

**If the terminal supports keyboard enhancement:**
- Call `supports_keyboard_enhancement()` before the blocking event loop, then push `DISAMBIGUATE_ESCAPE_CODES`; pop on every normal and exceptional teardown path.
- The kitty protocol defines `CSI > flags u` to request, `CSI ? u` to query, and `CSI ? flags u` as the response. crossterm's query can block or time out, so do not run it mid-loop without a bounded strategy.

**If support is absent or ambiguous:**
- Keep existing CR/legacy input and forward the inner application's current encoding. Do not synthesize a newline from a Shift modifier that was never reported.

## Version Compatibility

| Package | Compatible With | Notes |
|---------|-----------------|-------|
| `ratatui 0.30.2` | `crossterm 0.29.0` | Exact resolved versions in `Cargo.lock`; imports currently use ratatui's crossterm re-export. |
| `vt100 0.15.2` patched | `baude-core` local and remote parser consumers | `baude-core/src/pty.rs` and `baude/src/remote.rs` construct the parser; `baude-core` re-exports it. Keep public existing methods compatible. |
| `crossterm 0.29.0` | kitty-compatible terminals listed by its docs | Support is terminal-dependent; crossterm documents kitty, foot, WezTerm, Alacritty, and others, not all terminals. |
| OSC 8 metadata | ratatui buffer/presenter bridge | Ratatui cells do not currently carry a URL; direct escape injection into a cell symbol is unsafe. Keep metadata outside symbols and emit at the presenter/backend boundary. |

## Sources

- [Repository `Cargo.lock`](../../Cargo.lock) — exact locked `ratatui 0.30.2`, `crossterm 0.29.0`, and `vt100 0.15.2`.
- [Repository `baude/src/ui.rs`](../../baude/src/ui.rs) — custom `draw_term` copies vt100 cells into ratatui cells; this is the metadata boundary.
- [Repository `baude-core/src/pty.rs`](../../baude-core/src/pty.rs) and [remote parser](../../baude/src/remote.rs) — local and remote parser consumers.
- [vt100 0.15.2 Cell API](https://docs.rs/vt100/0.15.2/vt100/struct.Cell.html) and [latest vt100 docs](https://docs.rs/vt100/latest/vt100/) — no documented OSC 8/cell hyperlink API; latest shown as 0.16.2.
- [ratatui 0.30.2 Cell](https://docs.rs/ratatui/0.30.2/ratatui/buffer/struct.Cell.html) and [Buffer](https://docs.rs/ratatui/0.30.2/ratatui/buffer/struct.Buffer.html) — cell fields and buffer contract.
- [crossterm keyboard flags](https://docs.rs/crossterm/0.29.0/crossterm/event/struct.KeyboardEnhancementFlags.html), [push command](https://docs.rs/crossterm/0.29.0/crossterm/event/struct.PushKeyboardEnhancementFlags.html), and [support query](https://docs.rs/crossterm/0.29.0/crossterm/terminal/fn.supports_keyboard_enhancement.html) — verified APIs and timeout caveat.
- [Kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/) — progressive enhancement, query response, CSI-u and legacy fallback.
- [OSC 8 specification](https://gist.github.com/egmontkob/eb114294efbcd5adb1944c9f3cb5feda) and [`osc8` reference](https://docs.rs/osc8/latest/osc8/) — syntax and cell-attribute model; reference only, not a recommended dependency.

---
*Stack research for: baude v2.2 Reliability and Terminal Usability*
*Researched: 2026-09-08*
