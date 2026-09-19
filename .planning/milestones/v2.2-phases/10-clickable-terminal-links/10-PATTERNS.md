# Phase 10: Clickable Terminal Links - Pattern Map

**Mapped:** 2026-09-15
**Files analyzed:** 10 new/modified files
**Analogs found:** 9 / 10 (vendored-fork import has a wiring analog but no in-repo vendoring precedent)

All analog paths below verified git-tracked (`git ls-files`) this session. No `.gsd/` mirror paths are referenced anywhere in this document.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `vendor/vt100/src/screen.rs` (fork: OSC 8 arm, intern table, `link_target()`) | vendored parser (terminal-state) | streaming/transform | vt100 0.15.2 `osc_dispatch` (registry source, quoted in RESEARCH.md) | exact — extending the file itself |
| `vendor/vt100/src/attrs.rs` (fork: `link: Option<u16>`) | vendored parser | transform | vt100 0.15.2 `Attrs` struct | exact |
| `vendor/vt100/src/cell.rs` (fork: `link_id()`, link-stripping `clear`) | vendored parser | transform | vt100 0.15.2 `Cell` | exact |
| `vendor/vt100/src/grid.rs` (fork: OSC 8 re-emission in formatted output) | vendored parser | streaming | vt100 0.15.2 formatted writer | exact |
| `baude/src/links.rs` (NEW: `collect_links`, `validate_http_url`, `DetectedLink`) | utility (pure fns + tests) | transform (gesture-time scan) | `baude/src/app.rs` `mod clipboard_tests` (5491-5524) + selection-copy wrap path (5456-5468) | role-match |
| `baude/src/app.rs` — `Modal::LinkHints` variant + `handle_modal_key` arm | TUI state machine | event-driven (key dispatch) | existing `Modal` enum (386-411) + `handle_modal_key` (4374+) | exact |
| `baude/src/app.rs` — hint chord in `handle_key` | TUI event handler | event-driven | ctrl+e / ctrl+n chords (3892-3901) | exact |
| `baude/src/app.rs` — `spawn_opener` + `activate_link` (injected closure) | service (process spawn) | request-response (fire-and-forget) | `open_editor` (5082-5123) + `hook::route_event` (hook.rs:433-449) | exact (composite) |
| `baude/src/ui.rs` — `draw_modal` arm for `LinkHints` + help line | component (render) | request-response (frame draw) | `Modal::Help` arm (2114-2171) | exact |
| `Cargo.toml` / `baude-core/Cargo.toml` (workspace member + path dep) | config | — | root `Cargo.toml` workspace block + `baude-core/Cargo.toml:26` | role-match (no prior vendor/ precedent) |
| `baude-core/src/pty.rs` round-trip test (LINK-03 remote parity) | test | streaming | `subscribe()` snapshot (pty.rs:390-410) as the behavior under test | exact |

## Pattern Assignments

### `vendor/vt100/*` (vendored fork, terminal-state layer)

**Analog:** the vt100 0.15.2 registry source itself (local cache: `~/.cargo/registry/src/…/vt100-0.15.2`) — copy it into `vendor/vt100/` verbatim (MIT license file retained), then apply the four-file diff. RESEARCH.md Pattern 2 contains the verified `osc_dispatch` and `Attrs` excerpts (screen.rs:1670-1684, attrs.rs:27-32) and the ready OSC 8 dispatch-arm code (RESEARCH.md "Code Examples" — rejoin `params[2..]` with `b";"`).

**Wiring pattern (copy exactly):**
- `Cargo.toml:3` — `members = ["baude-core", "baude", "bauded"]` → append `"vendor/vt100"`.
- `baude-core/Cargo.toml:26` — `vt100 = "0.15"` → `vt100 = { path = "../vendor/vt100" }`.
- Re-export seam is already in place and must not change: `baude-core/src/lib.rs:6` — `pub use vt100;`. Downstream imports (`use baude_core::vt100;` in app.rs:5493) keep working because the fork keeps package name `vt100`.

**Test pattern:** carry over the upstream test suite as the regression floor; add hyperlink tests shaped like the parser-level test in RESEARCH.md Code Examples (pure `vt100::Parser::new` + `process(b"...")` + assertions — same shape as `clipboard_tests` below).

---

### `baude/src/links.rs` (NEW module: utility, gesture-time transform)

**Module registration** — copy `baude/src/main.rs:1-7` pattern (flat `mod` list):
```rust
mod app;
mod hierarchy;
mod keys;
...
```
Add `mod links;` there.

**Pure-test pattern** — copy `baude/src/app.rs:5491-5524` (`mod clipboard_tests`): tests build a parser inline, no fs/PTY/fixture, outside every Phase-8 guarded resolver:
```rust
#[cfg(test)]
mod clipboard_tests {
    use baude_core::vt100;

    fn selected(input: &str, rows: u16, cols: u16, end_row: u16, end_col: u16) -> String {
        let mut parser = vt100::Parser::new(rows, cols, 0);
        parser.process(input.as_bytes());
        parser.screen().contents_between(0, 0, end_row, end_col)
    }
    ...
}
```
`links.rs` tests (LINK-02 wrap-join, LINK-07 validation matrix) follow this exact shape: build a `vt100::Parser`, feed bytes, call the pure function on `parser.screen()`.

**Wrap-joining authority** — copy the trust model from `baude/src/app.rs:5456-5468` (selection copy):
```rust
if let Some(parser) = parser {
    if let Ok(mut p) = parser.lock() {
        p.set_scrollback(scroll);
        let screen = p.screen();
        // vt100's row wrap metadata is authoritative here:
        // contents_between omits only newlines after rows marked
        // as terminal continuations and preserves explicit ones.
        let text = screen.contents_between(sr, sc, er, ec + 1);
        p.set_scrollback(0);
        ...
    }
}
```
Two things to copy: (1) the `set_scrollback(scroll)` … work … `set_scrollback(0)` bracket around any screen read at the pane's current scroll offset (the hint-mode collection call site in app.rs must do the same); (2) `screen.row_wrapped(row)` (public, vt100 screen.rs:606) as the *only* wrap signal — never column-width heuristics. `collect_links(&Screen)` itself is a pure fn taking `&vt100::Screen`; the lock/scrollback bracketing stays in app.rs at the call site, mirroring how selection copy splits responsibility.

**Validation** — `validate_http_url` is fully specified in RESEARCH.md Code Examples (pre-decode control/whitespace reject → percent-decode re-check → `url::Url::parse` → `http|https` allowlist). Dependencies: add `url = "2"` and `percent-encoding = "2"` to `baude/Cargo.toml` `[dependencies]` (both already in Cargo.lock via ureq — zero new packages). Follow the existing dependency block style at `baude/Cargo.toml:10-20` (comment WHY when a dep is deliberate).

---

### `baude/src/app.rs` — `Modal::LinkHints` variant

**Analog:** `Modal` enum, `baude/src/app.rs:386-411`. Data-carrying variants use named struct fields with doc comments:
```rust
pub enum Modal {
    None,
    Help,
    ...
    Input {
        kind: InputKind,
        title: String,
        buf: String,
        /// Tab-completion candidates shown under the input.
        candidates: Vec<String>,
    },
    ConfirmKill { id: SelId },
    ...
}
```
Add `LinkHints { links: Vec<links::DetectedLink>, selected: usize }` in the same style.

**Interception guarantee** — the structural core of LINK-04, `app.rs:3862-3867` (do not disturb):
```rust
fn handle_key(&mut self, key: KeyEvent) {
    self.selection = None;
    if !matches!(self.modal, Modal::None) {
        self.handle_modal_key(key);
        return;
    }
```
Any `Modal` variant automatically swallows every key before `forward_key` (3904-3905) can reach the child — no new interception code needed.

**Modal key handling** — copy the `handle_modal_key` dispatch shape, `app.rs:4374-4413`. Two sub-patterns to reuse:
- Dismiss-on-Esc: `KeyCode::Esc => self.modal = Modal::None` (4385).
- Take-ownership-on-Enter: `let modal = std::mem::replace(&mut self.modal, Modal::None); if let Modal::Input { .. } = modal { ... }` (4412-4413) — use this exact `mem::replace` move for Enter (open) and `c`/`y` (copy) so the borrow of `self.modal` ends before calling `&mut self` methods like `set_message`.

**Hint chord** — copy the chord shape from `app.rs:3892-3901` (placed in the "Global chords" block, before the `match self.focus`):
```rust
if ctrl && matches!(key.code, KeyCode::Char('e')) {
    self.open_editor_for_selection();
    return;
}
```
New chord (recommended ctrl+o per RESEARCH.md Pitfall 5; occupied set: ctrl+q, ctrl+\/ctrl+4 via `is_backslash` app.rs:151-154, ctrl+e, ctrl+n, alt+←/→) follows the same `if ctrl && matches!(...) { ...; return; }` shape. Note the flow guard: only open hint mode when focus is Claude/Shell with a live parser; otherwise `set_message("no links visible".into())`.

---

### `baude/src/app.rs` — `spawn_opener` + `activate_link`

**Analog 1 (spawn shape):** `open_editor`, `app.rs:5082-5123`. Copy the stdio/spawn/message skeleton — but NOT the `sh -c` invocation (5110-5114), which is the documented anti-pattern for links (LINK-08 requires direct argv):
```rust
match Command::new("sh")            // <- for links: Command::new("open"|"xdg-open").arg(url)
    ...
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()
{
    Ok(_) => self.set_message(format!("opening {cmd}")),
    Err(e) => self.set_message(format!("editor: {e}")),
}
```
Copy: `Stdio::null()` on all three handles, `spawn()` non-blocking, `Ok`/`Err` both mapped to `set_message`, session continues on either branch. Replace: shell indirection with `Command::new(OPENER).arg(url)` (URL is argv data). RESEARCH.md Code Examples has the complete `spawn_opener` including the reaper thread (`std::thread::spawn(move || { let _ = child.wait(); })`) — a deliberate improvement over the analog's dropped-child behavior (Pitfall 8).

**Analog 2 (testability seam):** `route_event`, `baude-core/src/hook.rs:433-449`:
```rust
/// `post` is injected so the routing/fallback decision is unit-testable without
/// a live network peer. The transport call itself stays in the binary.
pub fn route_event<F>(url: Option<&str>, sid: &str, line: &str, post: F)
where
    F: FnOnce(&str, &str) -> bool,
{
    match url {
        Some(url) => {
            let posted = post(url, line);
            if !posted && !sid.is_empty() { let _ = append_event(sid, line); }
        }
        None => { ... }
    }
}
```
Copy the whole idiom: generic `F: FnOnce(...)` parameter, doc comment explaining WHY it is injected, real spawn passed at the production call site, closure spy in tests. `activate_link<F: FnOnce(&str) -> std::io::Result<()>>(&mut self, url, open)` in RESEARCH.md Code Examples is this pattern applied. Tests must never reach a real spawn (keeps `.planning/WINDOWS.md` entry 6 from growing a test-reachable member — update that entry to name the opener as injected-by-construction).

**Warning surface** — copy `set_message` usage, `app.rs:3586-3588`:
```rust
pub fn set_message(&mut self, msg: String) {
    self.message = Some((msg, now_ms() + MESSAGE_TTL_MS));
}
```
Note the last-wins caveat documented at `warn_seed_failures` (app.rs:3606-3634): if multiple warnings can occur in one action, aggregate into ONE `set_message` call (join with `" | "`), never loop per-warning. Opener failure is single-shot, so a plain `set_message(format!("open failed: {e} — session unaffected"))` matches the `open_editor` precedent.

---

### `baude/src/ui.rs` — `draw_modal` arm + help line

**Analog:** `Modal::Help` arm, `ui.rs:2114-2171`, inside `fn draw_modal(frame: &mut Frame, app: &App)` (ui.rs:1496). `draw_modal` is the final call in `draw` (ui.rs:99-117), so the overlay paints over the terminal panes for free. Copy the render skeleton:
```rust
Modal::Help => {
    let rect = centered(area, 60, 35);
    frame.render_widget(Clear, rect);
    let dim = Style::default().fg(Color::DarkGray);
    let p = Paragraph::new(vec![
        Line::from(Span::styled("select repository or checkout",
            Style::default().add_modifier(Modifier::BOLD))),
        Line::raw("  j/k ↑/↓     select repository or checkout"),
        ...
        Line::from(Span::styled("press any key to close", dim)),
    ])
    .block(Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" help "));
    frame.render_widget(p, rect);
}
```
Copy: `centered(...)` rect helper, `Clear` before content, `Paragraph` of `Line`s, rounded cyan-bordered `Block` with a ` title `. For `LinkHints`, each link renders as one `Line` (`[a] https://destination…`, selected row styled with `Modifier::BOLD` or reversed), destination middle-truncated for display only. Match quality is exact — the confirm modals (`Modal::ConfirmRemoveWorktree`, ui.rs:1607) show the same skeleton with dynamic per-item content if a closer data-driven example is wanted.

**Help documentation line** — add to the "global (any pane)" section, `ui.rs:2142-2151`:
```rust
Line::from(Span::styled("global (any pane)", Style::default().add_modifier(Modifier::BOLD))),
Line::raw("  ctrl+q      back to sidebar"),
Line::raw("  ctrl+\\      toggle shell pane (focuses it)"),
```
New line follows the two-space + padded-key + description format: `Line::raw("  ctrl+o      link hints (inspect/copy/open urls)")`.

---

### `baude-core/src/pty.rs` — round-trip test (no production change expected)

**Behavior under test:** the subscribe snapshot, `pty.rs:390-410` — the fork's `contents_formatted()` must now re-emit OSC 8:
```rust
let screen = p.screen();
let mut bytes = Vec::new();
if screen.alternate_screen() { bytes.extend_from_slice(b"\x1b[?1049h"); }
bytes.extend_from_slice(b"\x1b[2J\x1b[H");
bytes.extend_from_slice(&screen.contents_formatted());
// Terminal modes aren't part of contents_formatted; replay
// the ones claude relies on.
```
The comment at 403-404 is the precedent for what this snapshot deliberately replays vs. omits — the fork moves OSC 8 into the "replayed" set. Round-trip test shape: process OSC 8 into parser A → `contents_formatted()` → feed to fresh parser B → assert equal link ids/targets (pure, no PTY needed — same style as the fork's own tests). Live-byte fidelity needs no test change: the reader thread broadcasts raw bytes verbatim under the parser lock (`pty.rs:358-362`), and remote attach feeds an identical parser (`remote.rs:244, 278-281`, verified in RESEARCH.md).

## Shared Patterns

### Warning surface (non-fatal failures)
**Source:** `baude/src/app.rs:3586-3588` (`set_message`)
**Apply to:** opener failure, empty-link-scan message, copy path if it ever grows error reporting.
Failures never kill the session; both `Ok` and `Err` arms of a spawn resolve to `set_message` (open_editor precedent, app.rs:5120-5121). Aggregate multi-warning actions into one message (warn_seed_failures precedent, app.rs:3628-3633).

### Injected side-effects for testability
**Source:** `baude-core/src/hook.rs:431-435` (route_event's injected `post`)
**Apply to:** `activate_link` (injected opener), link-hint copy dispatch (injected copy sink for the LINK-06 test).
Generic `FnOnce` parameter + doc comment stating why; production call site passes the real spawn; tests pass a recording closure. No test may reach a real `Command::spawn`.

### Uncontained-subprocess reuse
**Source:** `baude/src/app.rs:5476-5488` (`copy_to_clipboard` — pbcopy, piped stdin, null stdout/stderr, errors ignored)
**Apply to:** LINK-06 copy action — call `App::copy_to_clipboard(destination)` directly. Do NOT write a second clipboard spawn (`.planning/WINDOWS.md` entry 6: reuse, don't duplicate; update that entry to record the opener's disposition).

### Scrollback-bracketed screen reads
**Source:** `baude/src/app.rs:5456-5464`
**Apply to:** the hint-chord handler's `collect_links` call site.
`parser.lock()` → `set_scrollback(scroll)` → read screen → `set_scrollback(0)` → drop lock. Never hold the lock across modal state changes or rendering.

### Pure parser tests (Phase-8 isolation by construction)
**Source:** `baude/src/app.rs:5491-5524` (`clipboard_tests`)
**Apply to:** `links.rs` tests, fork hyperlink tests, pty round-trip test.
`vt100::Parser::new` + `process(bytes)` + assert on `Screen` — no fs, no PTY, no `TestRedirect`/`BAUDE_TEST_FIXTURE_ROOT` needed.

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `vendor/vt100/` as a directory/workspace concept | vendored crate | — | No prior vendored crate in the repo. Use RESEARCH.md's vendoring recipe (copy 0.15.2 source + LICENSE from local registry cache, add workspace member, path-dep from baude-core, `vendor/vt100/README.md` recording upstream version + diff surface per Open Question 4). The *wiring* analogs (workspace members list, `vt100 = "0.15"` dep line, `pub use vt100;` re-export) are listed above. |

## Metadata

**Analog search scope:** `baude/src/` (app.rs, ui.rs, main.rs), `baude-core/src/` (pty.rs, hook.rs, lib.rs), workspace Cargo.toml files, `.planning/WINDOWS.md`; vt100 0.15.2 internals via RESEARCH.md's source-verified excerpts.
**Files scanned:** 9 in-repo files (all confirmed git-tracked); vt100/vte registry sources via RESEARCH.md verification.
**Pattern extraction date:** 2026-09-15
