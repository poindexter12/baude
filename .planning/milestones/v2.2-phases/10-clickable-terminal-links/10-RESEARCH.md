# Phase 10: Clickable Terminal Links - Research

**Researched:** 2026-09-15
**Domain:** Terminal emulation (OSC 8 hyperlinks), TUI interaction, URL validation, platform openers
**Confidence:** HIGH (pipeline mapping and crate internals verified from source this session)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### Activation Gesture & Interaction Model
- The explicit gesture is a keyboard **link-hint mode**: a documented key
  opens an overlay that labels the visible links; selection, drag-copy,
  scrolling, and child mouse behavior are untouched because no new mouse
  capture is introduced. Works identically over remote attach.
- The hint overlay shows each link's **actual destination** (never just the
  OSC8 label). Per-link actions: Enter opens, `c`/`y` copies the destination,
  Esc dismisses.
- LINK-05 (inspect before open) is satisfied structurally: the overlay always
  displays the destination before Enter can open — no extra confirmation
  dialog.
- Mouse modifier-click is at Claude's discretion: keyboard hint-mode is the
  committed deliverable; add click activation only if research shows no
  conflict with terminal selection and child-mouse passthrough. Output alone
  never opens a link either way.

#### Detection & Cell Metadata
- OSC8 sequences are parsed in the terminal-state layer; each cell carries a
  link id, so scrolling, scrollback, wrapping, resizing, overwrites, and
  erasure inherit correctness from the grid model (LINK-03).
- Bare URLs are detected with a scheme-anchored `http(s)://` scan over
  rendered rows, joining soft-wrapped continuations across row boundaries and
  stripping unbalanced trailing prose punctuation (`.,;:!?)]}'"`) (LINK-02).
- Bare-URL detection runs **on demand at gesture time** against the current
  grid/scrollback view — no persistent per-frame link index. OSC8 ids persist
  in cells as terminal state.
- Remote attach uses the same path: remote PTY bytes flow through the
  identical terminal-state parser, so link metadata behaves the same in
  attached TUI terminals.

#### Validation & Opening
- Only parsed `http`/`https` URLs are activatable. Targets bearing control
  characters (checked pre- and post-percent-decode), whitespace, or failing
  URL parsing render as plain non-activatable text (LINK-07).
- Opening passes the URL as argv data: `Command::new("open")` on macOS /
  `xdg-open` on Linux, `.arg(url)`, spawned detached and non-blocking — never
  through a shell (LINK-08).
- Opener failure surfaces through the existing `set_message` warning path and
  the session keeps running.
- Opening always happens on the machine running the TUI (local opener), even
  when attached to a remote daemon.

### Claude's Discretion
- Exact hint-mode keybind (must not collide with existing chords; document
  in the help overlay).
- Hint label scheme (letters vs numbers), overlay layout, and max links shown.
- Whether modifier-click ships in v2.2 (per Area 1 decision above).
- Which crate layer owns the OSC8 parser extension (existing vt processing
  in-tree vs vendored parser), guided by research.

### Deferred Ideas (OUT OF SCOPE)
None — discussion stayed within phase scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| LINK-01 | Activate a labeled OSC8 HTTP(S) link, opening its target rather than its visible label | vt100 0.15.2 drops OSC 8 today (verified at source); vendored-fork extension point identified; OSC 8 format verified (target is param 2+, label is ordinary cell text) |
| LINK-02 | Activate a bare HTTP(S) URL incl. soft-wrapped ones without prose punctuation | `Screen::row_wrapped(row)` is public API (vt100 screen.rs:606); selection copy already relies on the same wrap metadata (app.rs:5460-5463) |
| LINK-03 | Links stay on correct cells through scroll/scrollback/wrap/resize/overwrite/erasure, local AND attached remote | Per-cell link id in forked `Attrs` inherits every grid behavior; remote attach feeds an identical parser (remote.rs:244, 278-281); snapshot-replay gap found at pty.rs:402 with fix path |
| LINK-04 | Documented explicit gesture; selection/drag-copy/scroll/child-mouse unaffected; output alone never opens | Modal key interception at app.rs:3864 runs before PTY forwarding; occupied chords enumerated; help overlay location identified (ui.rs:2142-2151) |
| LINK-05 | Inspect actual destination before opening | Hint overlay renders destination per locked decision; Modal infrastructure at app.rs:386 |
| LINK-06 | Copy destination without opening | Existing `App::copy_to_clipboard` (app.rs:5476-5488) reused; WINDOWS entry 6 containment note honored (reuse, don't duplicate) |
| LINK-07 | Non-HTTP(S)/malformed/control-bearing targets are non-activatable | `url` 2.5.8 + `percent-encoding` already in dependency graph; validation function specified with pre/post-decode control checks |
| LINK-08 | Open passes target as argv data, never shell; opener failure non-fatal | `open_editor` (app.rs:5082-5123) and `route_event` injected-post (hook.rs:431-433) precedents; opener spy pattern specified |
</phase_requirements>

## Summary

baude's terminal state is entirely owned by the `vt100` crate (v0.15.2, vte 0.11.1 backend). Every pane — local claude, local shell, and remote attach — is a `vt100::Parser` fed raw PTY bytes; the TUI renders by reading `Screen::cell(row, col)` directly and copies selections via `contents_between`, which already trusts vt100's row-wrap metadata. This is exactly the "screen model stays authoritative" architecture the requirements demand — and vt100 0.15.2 **silently discards OSC 8**: its `osc_dispatch` handles only OSC 0/1/2 (window title). Upstream vt100 0.16.2 (July 2025) still has no hyperlink support and introduces breaking API changes, so upgrading buys nothing.

The cheapest correct extension point is a **vendored fork of vt100 0.15.2** (MIT, ~8 source files) added as a workspace path dependency: add an OSC 8 arm to `osc_dispatch`, a `link: Option<u16>` field on `Attrs` (an index into a per-Screen interned URI table), and two additive public accessors. Because link identity rides on `Attrs`, every LINK-03 behavior — scrollback, wrap, resize, overwrite, erasure — inherits correctness from the existing grid model with no new bookkeeping. baude does not publish to crates.io (release = GitHub tarballs + container, verified in release.yml), so a path-dep fork has no publishing cost. One real gap was found: the remote-attach redraw snapshot (`Pty::subscribe`, pty.rs:402) replays `contents_formatted()`, which does not re-emit OSC 8 — links printed *before* an attach would be invisible to the remote parser unless the fork also extends the formatted-output path.

Everything else reuses existing infrastructure: hint mode is a new `Modal` variant (modal keys are intercepted before any byte reaches the child, structurally guaranteeing "output alone never opens"), copy reuses `App::copy_to_clipboard`, failure surfaces through `set_message`, validation uses the `url` crate (already in the dependency graph via ureq — zero new packages), and the opener follows the `open_editor` detached-spawn precedent with the `route_event` injected-closure pattern making it testable without opening anything.

**Primary recommendation:** Vendor vt100 0.15.2 into `vendor/vt100` and extend it with per-cell OSC 8 link ids; implement hint mode as a `Modal::LinkHints` variant; validate with the `url` crate promoted to a direct dependency; open via platform opener argv-spawn behind an injected closure.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| OSC 8 parsing + per-cell link ids | vendored `vt100` fork (terminal-state) | — | Locked: parsed in the terminal-state layer; cells carry ids so grid behaviors inherit correctness |
| Link table (id → URI) | vendored `vt100` fork (`Screen`) | — | URIs must survive scrollback with their cells; the Screen owns cell lifetime |
| Bare-URL scan (gesture-time) | `baude` TUI (pure fn over `&Screen`) | — | Locked: on-demand at gesture time, no persistent index; needs wrap metadata already public on Screen |
| URL validation | `baude` TUI (pure fn) | — | Consumes both OSC8 targets and bare-URL candidates; presentation-adjacent policy, binaries own presentation |
| Hint overlay + key handling | `baude` TUI (`Modal` + `ui::draw_modal`) | — | Existing modal system intercepts keys before PTY forwarding (app.rs:3864) |
| Copy destination | `baude` TUI (`App::copy_to_clipboard`) | — | Reuse existing pbcopy path (WINDOWS entry 6: reuse, don't duplicate) |
| Opening | `baude` TUI → platform opener (`open`/`xdg-open`) | — | Locked: always local to the TUI machine, argv data, detached |
| Remote attach fidelity | `baude-core` (`Pty::subscribe` snapshot) + fork's formatted output | `baude` remote.rs parser | Live bytes already pass OSC 8 verbatim; the snapshot path needs OSC 8 re-emission |

**Explicitly NOT involved:** `bauded` (no daemon-side changes — opening is local; the websocket already ships raw bytes), persistence (link state is runtime-only terminal state).

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `vt100` (vendored fork) | 0.15.2 + baude OSC8 extension | Terminal state, grid, scrollback, now link ids | Already the in-tree screen model; REQUIREMENTS forbids engine replacement ("Prefer a verified narrow parser extension") |
| `url` | 2.5.8 (already in Cargo.lock via ureq) | LINK-07 parse + scheme allowlist | WHATWG URL implementation from servo; 14.3M weekly downloads; promoting a transitive dep adds zero packages |
| `percent-encoding` | 2.x (already in Cargo.lock via url) | Post-decode control-char check | Same rust-url project; the decode step the locked decision requires |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `ratatui` | 0.30 (existing) | Hint overlay rendering | `Clear` + `Paragraph` modal precedent (ui.rs:1496, 2114) |
| existing `Command` spawn precedents | std | Opener + clipboard | `open_editor` (app.rs:5082) and `copy_to_clipboard` (app.rs:5476) |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Vendored vt100 0.15.2 fork | Upgrade to vt100 0.16.2 | 0.16 still has NO OSC 8 (verified: docs.rs Cell API + changelog); breaking changes (`set_scrollback` moved to `Screen`, `Cell::contents` → `&str`, `Perform` removed) for zero link benefit. Could be combined later, not now |
| Vendored vt100 fork | `tattoy-wezterm-term` / wezterm's term crate (has hyperlinks) | Full terminal-engine replacement — explicitly out of scope in REQUIREMENTS.md ("Full terminal-engine replacement… requires a new scope decision") |
| Vendored vt100 fork | Byte-stream OSC 8 pre-scanner wrapping `Parser::process` | Cannot map byte positions to cells without duplicating cursor state — reimplements terminal state, violating "Keep the existing screen model authoritative" |
| `url` crate | Hand-rolled scheme/charset check | WHATWG parsing has dozens of edge cases (percent-decoding, IDN, userinfo tricks like `http://evil@good.com`); see Don't Hand-Roll |
| Fork published under new crate name | In-repo path dependency | baude does not `cargo publish` (verified: release.yml uploads tarballs only) — path dep is simpler and keeps the fork reviewable in-tree |

**Installation:**
```bash
# No new external packages. Promote existing transitive deps to direct in baude/Cargo.toml:
#   url = "2"
#   percent-encoding = "2"
# Vendor the fork:
#   vendor/vt100/  (copy of vt100 0.15.2 source, MIT license file retained)
#   [workspace] members += "vendor/vt100"
#   baude-core: vt100 = { path = "../vendor/vt100" }
```

**Version verification (2026-09-15):**
- `cargo search vt100` → latest published `vt100 = "0.16.2"`; in-tree is 0.15.2 `[VERIFIED: Cargo.lock:2819-2828 — `name = "vt100"` / `version = "0.15.2"`]`; baude-core declares `vt100 = "0.15"` `[VERIFIED: baude-core/Cargo.toml:26 — `vt100 = "0.15"`]`
- `cargo search url` → `url = "2.5.8"`, matching the locked version `[VERIFIED: Cargo.lock:2771-2772 — `name = "url"` / `version = "2.5.8"`]`
- `cargo tree -i url` → sole path is `url v2.5.8 └── ureq v2.12.1` into all three crates `[VERIFIED: cargo tree output this session]`

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| url | crates.io | since 2014-11 | 14.4M/wk | github.com/servo/rust-url | OK | Approved (already transitive via ureq) |
| percent-encoding | crates.io | since 2017-06 | 15.1M/wk | github.com/servo/rust-url | OK | Approved (already transitive via url) |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

The vendored vt100 fork is not a registry install — it is a source copy of the already-audited, already-shipping vt100 0.15.2 (MIT, Copyright (c) 2016 Jesse Luehrs `[VERIFIED: registry source LICENSE]`).

## Architecture Patterns

### System Architecture Diagram

```
                         LOCAL SESSION                                REMOTE ATTACH
  child process (claude/shell)                             bauded daemon
        │ raw PTY bytes                                          │ raw PTY bytes over websocket
        ▼                                                        ▼
  Pty reader thread ──────────────┐                        RemoteAttach IO thread
  (pty.rs:345-368)                │ broadcast raw bytes    (remote.rs:253-295)
        │ p.process(&buf[..n])    │ to subscribers               │ p.process(&bytes)
        ▼  (pty.rs:359)           │ (pty.rs:361)                 ▼  (remote.rs:280)
  vt100::Parser (FORKED) ◄────────┴──[snapshot: contents_formatted, pty.rs:402
  grid + 2000-row scrollback          ⚠ must re-emit OSC 8 in fork]
  + NEW: per-cell link id ────────► identical forked vt100::Parser (remote.rs:244)
        │
        │ Screen::cell(row,col) / row_wrapped(row) / link_target(id)
        ▼
  ┌─ render path ─────────────┐   ┌─ gesture path (NEW) ────────────────────────┐
  │ ui::draw_term             │   │ hint-mode chord (app.rs handle_key, before   │
  │ (ui.rs:1140, called at    │   │ forward_key) → collect_links(&Screen):       │
  │ 1051 claude / 1078 shell /│   │   1. OSC8 runs: cells sharing link id        │
  │ 1112 remote)              │   │   2. bare URLs: wrap-joined row scan         │
  └───────────────────────────┘   │   3. validate_http_url() each target         │
                                  │ → Modal::LinkHints { links }                 │
                                  │   Enter → spawn_opener(url) [injected]       │
                                  │   c/y  → App::copy_to_clipboard (app.rs:5476)│
                                  │   Esc  → Modal::None                         │
                                  │ open/copy failure → set_message (app.rs:3586)│
                                  └──────────────────────────────────────────────┘
                                          │ Enter on validated link only
                                          ▼
                                  Command::new("open"|"xdg-open").arg(url)
                                  detached, stdio null — ALWAYS on TUI machine
```

### Recommended Project Structure
```
vendor/vt100/            # forked vt100 0.15.2 (workspace member; MIT license retained)
├── src/attrs.rs         # + link: Option<u16> on Attrs
├── src/screen.rs        # + OSC 8 arm in osc_dispatch; link intern table; link_target()
├── src/cell.rs          # + Cell::link_id()
├── src/grid.rs          # + OSC 8 emission in write_contents_formatted/diff (attach snapshot)
└── src/row.rs           # unchanged (wrapped() already exists)
baude-core/src/pty.rs    # unchanged feed path; subscribe() snapshot now carries OSC 8 via fork
baude/src/links.rs       # NEW: collect_links(&Screen), validate_http_url(), DetectedLink
baude/src/app.rs         # Modal::LinkHints variant; hint chord; activate/copy dispatch; opener fn
baude/src/ui.rs          # draw_modal arm for LinkHints; help overlay line for the new chord
```

### Pattern 1: Terminal-state pipeline (verified map)

**What:** All three panes converge on `vt100::Parser`; extend the parser, and every pane gets links.

- Local PTY: parser created `vt100::Parser::new(rows, cols, 2000)` `[VERIFIED: baude-core/src/pty.rs:335]`; reader thread feeds it: `p.process(&buf[..n]);` and broadcasts raw bytes to attach subscribers under the same lock: `subs.retain(|s| s.send(buf[..n].to_vec()).is_ok());` `[VERIFIED: baude-core/src/pty.rs:358-361]`
- Remote attach: identical parser `vt100::Parser::new(rows, cols, 2000)` `[VERIFIED: baude/src/remote.rs:244]` fed from the websocket: `Ok(tungstenite::Message::Binary(bytes)) => { … p.process(&bytes); }` `[VERIFIED: baude/src/remote.rs:278-281]`. Same-path assumption CONFIRMED — OSC 8 bytes arriving after attach reach the remote parser verbatim.
- Attach snapshot gap: a subscriber joining later gets a redraw built from `bytes.extend_from_slice(&screen.contents_formatted());` `[VERIFIED: baude-core/src/pty.rs:402]` — stock `contents_formatted` emits SGR only, so pre-attach links would be lost remotely unless the fork re-emits OSC 8 there (see Pitfall 3).
- Re-export seam: `pub use vt100;` `[VERIFIED: baude-core/src/lib.rs:6]` — the fork keeps package name `vt100`, so no downstream import changes.

### Pattern 2: OSC 8 handling in the fork

**What:** vt100 0.15.2's OSC dispatch handles only title sequences and drops the rest:

```rust
// [VERIFIED: vt100-0.15.2/src/screen.rs:1670-1684, read this session]
fn osc_dispatch(&mut self, params: &[&[u8]], _bel_terminated: bool) {
    match (params.get(0), params.get(1)) {
        (Some(&b"0"), Some(s)) => self.osc0(s),
        (Some(&b"1"), Some(s)) => self.osc1(s),
        (Some(&b"2"), Some(s)) => self.osc2(s),
        _ => { /* logged as unhandled, dropped */ }
    }
}
```

**Extension:** add an arm for `Some(&b"8")`. The OSC 8 wire format is `ESC ] 8 ; params ; URI ST` with `ST` = `ESC \` (BEL accepted as legacy — vte handles both terminators and passes `_bel_terminated`); params are `key=value` pairs separated by `:`, only `id` defined; an empty URI closes the link; switching links without closing is legal; the link is "a cell attribute, like color or bold" `[CITED: gist.github.com/egmontkob/eb114294efbcd5adb1944c9f3cb5feda]`. vte splits the OSC string on `;`, so a URI containing literal `;` arrives split across `params[2..]` — **rejoin `params[2..]` with `b";"`** before storing.

**Cell attribute:** `Attrs` is a small Copy struct:

```rust
// [VERIFIED: vt100-0.15.2/src/attrs.rs:27-32, read this session]
#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Attrs {
    pub fgcolor: Color,
    pub bgcolor: Color,
    pub mode: u8,
}
```

Add `pub link: Option<u16>` — an index into a `Vec<(String, String)>` (id-param, URI) interned on `Screen`, deduplicated by `(id, uri)` per the spec's grouping rule (same URI + same nonempty id = one logical link; this is also how soft-wrapped OSC 8 fragments unify). Cells copied into scrollback keep their `Attrs`, so link ids survive scrollback for free; the intern table only grows and must be capped (see Pitfall 6).

### Pattern 3: Hint mode as a Modal variant

**What:** The `Modal` enum is the existing overlay state machine `[VERIFIED: baude/src/app.rs:386-411 — variants `None, Help, Info, Gsd, Activity, Input{…}, ConfirmKill{…}, ConfirmCloseWorktree{…}, ConfirmRemoveWorktree{…}`]`. `handle_key` dispatches to `handle_modal_key` **before** any global chord or PTY forwarding:

```rust
// [VERIFIED: baude/src/app.rs:3862-3867]
fn handle_key(&mut self, key: KeyEvent) {
    self.selection = None;
    if !matches!(self.modal, Modal::None) {
        self.handle_modal_key(key);
        return;
    }
```

A `Modal::LinkHints { links: Vec<DetectedLink>, selected: usize }` variant therefore (a) swallows every key while open — Enter/c/y/Esc handled, everything else ignored or dismissed, nothing forwarded to the child; and (b) can only be *entered* via an explicit chord — the structural guarantee for "output alone never opens a link" (LINK-04). Rendering slots into `draw_modal`, which runs last in the frame `[VERIFIED: baude/src/ui.rs:99-117 — `draw_modal(frame, app);` is the final call in `draw`]`, using the `Clear` + bordered `Paragraph` precedent of `Modal::Help` (ui.rs:2114-2171).

**When to use:** hint chord pressed while focus is Claude or Shell pane with a live parser (local session or active attach). No session / no links → `set_message("no links visible")`.

**Overlay layout recommendation (discretion):** a bottom-anchored list panel — `[a] https://actual-destination.example/…` per row, selected row highlighted, destination middle-truncated for display but copied/opened in full. A list panel shows destinations naturally (LINK-05), handles >26 links by scrolling, and avoids painting labels over wide-glyph cells. In-place vimium-style labels are viable later (draw_term writes the buffer cell-by-cell, ui.rs:1160-1202) but are not needed to satisfy any requirement.

### Pattern 4: Gesture-time link collection

**What:** One pure function `collect_links(screen: &vt100::Screen) -> Vec<DetectedLink>` run when the chord fires, against the pane's parser at its current scroll offset (the same `set_scrollback(offset)` … `set_scrollback(0)` bracket the selection-copy and render paths already use `[VERIFIED: baude/src/app.rs:5456-5464; baude/src/ui.rs:1154, 1213]`).

1. **OSC 8 pass:** walk visible cells; group maximal runs sharing `link_id`; dedupe whole-screen by interned `(id, uri)`; resolve destination via `screen.link_target(id)`. The visible **label text is never the destination** (LINK-01).
2. **Bare-URL pass:** build logical lines by joining row `r+1` onto row `r` while `screen.row_wrapped(r)` — public API: `pub fn row_wrapped(&self, row: u16) -> bool` `[VERIFIED: vt100-0.15.2/src/screen.rs:606]`; this is exact, not heuristic — the same metadata selection copy trusts: "vt100's row wrap metadata is authoritative here: contents_between omits only newlines after rows marked as terminal continuations" `[VERIFIED: baude/src/app.rs:5460-5462]`. Scan each logical line for case-insensitive `http://`/`https://`, extend across the RFC 3986 charset, then strip unbalanced trailing `.,;:!?)]}'"` (locked list). Track (row, col) spans during joining so hints can point at cells.
3. **Validate** every candidate (both passes) with `validate_http_url`; failures are simply not collected — they remain plain rendered text (LINK-07).
4. Cells inside an OSC 8 run are excluded from the bare-URL pass (the explicit link wins; prevents double entries when a program prints the URL as its own label).

### Anti-Patterns to Avoid
- **Parsing OSC 8 outside the terminal-state layer** (byte-stream side-scanner): duplicates cursor/wrap/scroll state — the exact "independently reimplementing terminal state" the Execution Constraints forbid.
- **Treating the OSC8 label as the URL:** the destination is the sequence's URI parameter; the label is ordinary cell text (LINK-01's core distinction).
- **Validating after `url::Url::parse` only:** the WHATWG algorithm strips ASCII tab/newline and trims C0-control/space before parsing `[ASSUMED — see Assumptions A2]`, so a control-bearing target can parse "clean". The locked pre-parse raw-string control/whitespace rejection must run first.
- **Opening via `sh -c`** (the `open_editor` precedent at app.rs:5110-5114 uses `sh -c` for editor commands with args): links must use direct `Command::new(opener).arg(url)` — locked, and LINK-08.
- **A persistent per-frame link index:** locked out; scan at gesture time only.
- **Mouse capture changes:** modifier-click is discretionary and NOT recommended for v2.2 — `MouseEventKind::Down(Left)` unconditionally begins a selection today `[VERIFIED: baude/src/app.rs:5403-5422]` and click events are never forwarded to the child, so adding modifier-click is *possible* without disturbing passthrough, but it adds terminal-dependent modifier-reporting variance for zero requirement coverage. Ship keyboard hints; revisit later.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| URL parsing/scheme allowlist | regex or string-split validator | `url` crate (`Url::parse`, `.scheme()`) | WHATWG edge cases: `http://user:pass@host`, IDN/punycode, percent-decoding, default-port normalization, `\` treated as `/` — a hand validator will disagree with what the browser actually opens |
| Percent-decoding | manual `%XX` loop | `percent_encoding::percent_decode_str` | already in-graph; handles invalid sequences and UTF-8 correctness |
| Escape-sequence parsing | OSC 8 byte scanner in baude | vte (inside the fork) | vte already tokenizes OSC with both ST and BEL terminators, UTF-8 mid-sequence, chunked reads across `process()` calls |
| Soft-wrap join | column-width heuristics ("line is full ⇒ wrapped") | `Screen::row_wrapped` | explicit newline at exactly column `cols` is indistinguishable by inspection; vt100 records the real wrap flag per row |
| Terminal grid/scrollback link tracking | shadow grid keyed by (row,col) | link id on `Attrs` in the fork | overwrite/erase/scroll/resize all mutate cells; an external map desynchronizes on every one of them |

**Key insight:** every hard problem in this phase is a *state-tracking* problem the vt100 grid already solves for colors and bold. Making the link one more cell attribute converts LINK-03's six sub-behaviors from features-to-build into properties-inherited.

## Common Pitfalls

### Pitfall 1: OSC8 label spoofing is the threat model, not an edge case
**What goes wrong:** `\e]8;;https://evil.example\e\\github.com/safe\e]8;;\e\\` renders as "github.com/safe".
**Why it happens:** the label is arbitrary cell text; only the URI parameter is the destination.
**How to avoid:** overlay always shows the parsed destination (locked); the label never appears as the target anywhere in the data model.
**Warning signs:** any code path that reads cell *contents* to determine an OSC8 destination.

### Pitfall 2: vte splits the OSC string on `;` — naive `params[1]`/`params[2]` handling corrupts URIs
**What goes wrong:** `https://ex.com/a;b` arrives as `params = ["8", "", "https://ex.com/a", "b"]`; taking `params[2]` alone truncates the URL.
**Why it happens:** `osc_dispatch(params: &[&[u8]], …)` receives `;`-split fields `[VERIFIED: vt100-0.15.2/src/screen.rs:1670]`.
**How to avoid:** rejoin `params[2..]` with `;`. Hard limit: `const MAX_OSC_PARAMS: usize = 16;` `[VERIFIED: vte-0.11.1/src/lib.rs:54]` and at the 16th separator vte returns without recording it (`MAX_OSC_PARAMS => return` in `osc_put`, lib.rs:252) — a URI with ≥14 literal semicolons loses separators. Accept and document; spec guidance is to percent-encode `;` anyway `[CITED: egmontkob gist]`. Note the OSC buffer itself is a `Vec` (unbounded) in std builds — `#[cfg(not(feature = "no_std"))] osc_raw: Vec<u8>` `[VERIFIED: vte-0.11.1/src/lib.rs:84-85]` — so long URLs are not truncated by vte; cap them yourself (Pitfall 6).

### Pitfall 3: remote attach snapshot loses pre-attach links
**What goes wrong:** attach to a session whose links were printed before you connected → remote hint mode finds no OSC8 links; success criterion 3 fails for the remote path.
**Why it happens:** `subscribe()` replays `screen.contents_formatted()` `[VERIFIED: baude-core/src/pty.rs:402]`, which reconstructs SGR only; live bytes after attach are fine (raw broadcast, pty.rs:361).
**How to avoid:** in the fork, emit `\x1b]8;;URI\x1b\\` when the formatted-output writer crosses a cell whose link id differs from the previous cell's, and the close sequence when leaving. Add a round-trip test: process OSC8 → `contents_formatted()` → feed to fresh parser → assert same link ids.
**Warning signs:** remote tests that only cover post-attach output.

### Pitfall 4: erase/clear inheriting the current link
**What goes wrong:** program opens a link, later clears the screen/line; cleared cells silently carry the link id — phantom links on blank cells.
**Why it happens:** vt100 fills erased cells with the *current* drawing attrs (bgcolor semantics); if `link` lives in `Attrs`, fills inherit it.
**How to avoid:** in the fork, strip `link` from the attrs used for every `Cell::clear`/fill call site (`pub(crate) fn clear(&mut self, attrs: …)` `[VERIFIED: vt100-0.15.2/src/cell.rs:60]`). Test: open link → `\e[2J` → assert no cell has a link id.
**Warning signs:** hint mode listing links on visually empty regions.

### Pitfall 5: the hint chord must be a ctrl/alt chord, and the namespace is tight
**What goes wrong:** a plain key (e.g. `f`) in Claude/Shell focus is forwarded to the child (`Focus::Claude => self.forward_key(key, false)` `[VERIFIED: baude/src/app.rs:3902-3906]`) — it would type into the agent instead of opening hints; or a chord steals something the child shell needs (ctrl+l = clear, ctrl+r = history search).
**Why it happens:** occupied global chords: ctrl+q, ctrl+\ (which crossterm reports as ctrl+4 — `matches!(code, KeyCode::Char('\\') | KeyCode::Char('4'))` `[VERIFIED: baude/src/app.rs:151-154]`), ctrl+e, ctrl+n, alt+←/→ `[VERIFIED: baude/src/app.rs:3874-3900]`.
**How to avoid:** recommend **ctrl+o** ("open"; readline's operate-and-get-next is rarely used `[ASSUMED]`). Alternatives if rejected: ctrl+g, ctrl+t. Document in the help overlay's "global (any pane)" section `[VERIFIED: baude/src/ui.rs:2142-2151]` (SHIP-02 will cite it).
**Warning signs:** dogfood reports of a shell/agent feature that stopped responding.

### Pitfall 6: unbounded link intern table
**What goes wrong:** a long-running agent printing thousands of unique URLs grows the Screen's URI table without bound (cells in the 2000-row scrollback may reference old ids, so entries can't be freed by frame).
**How to avoid:** intern with a `(id, uri)` → index map; cap entries (u16 index space; recommend refusing new links past ~10k with `link = None`) and cap URI length at 2083 bytes, matching VTE/iTerm2 `[CITED: egmontkob gist]`.
**Warning signs:** memory growth proportional to session output with OSC8-heavy tools.

### Pitfall 7: `url::Url::parse` is more permissive than the raw string
**What goes wrong:** relying on parse failure to reject control characters or whitespace — WHATWG parsing strips tab/newline and trims leading/trailing C0/space before parsing `[ASSUMED — A2]`, and percent-encoded controls (`%00`, `%0A`) parse fine.
**How to avoid:** locked decision already mandates it: reject raw strings containing control chars or whitespace BEFORE parse; percent-decode and re-check AFTER parse. Also compare display: show the *parsed/normalized* URL (`parsed.as_str()`) in the overlay so what the user inspects is what argv receives.
**Warning signs:** tests only feeding well-formed URLs.

### Pitfall 8: spawn success ≠ open success; dropped Child = zombie
**What goes wrong:** `xdg-open` may exit non-zero long after `spawn()` returns Ok (no handler for scheme); the `Ok` branch reports "opening…" for a failed open. Separately, dropping the `Child` without waiting leaves a zombie until baude exits.
**Why it happens:** detached-and-non-blocking is locked; you cannot synchronously observe the opener's exit.
**How to avoid:** treat `spawn()` Err as the LINK-08 failure surface (`set_message`, session continues); for exit status, reap on a detached thread (`std::thread::spawn(move || { let _ = child.wait(); })`) and optionally surface a late non-zero exit via a message queue. The `open_editor`/`copy_to_clipboard` precedents also drop children unwaited `[VERIFIED: baude/src/app.rs:5110-5122, 5478-5487]` — matching precedent (drop) is acceptable; the reaper thread is a cheap improvement. Note: a validated http(s) URL always begins `http`, so it can never be parsed by the opener as a `-flag` — argument-injection via argv position is structurally excluded.
**Warning signs:** "opening…" message with no browser action and no error.

### Pitfall 9: new uncontained subprocess vs Phase 8 containment
**What goes wrong:** the opener adds a third uncontained spawn alongside the two already logged: "App::open_editor (and the pbcopy spawn at 5442) still spawn uncontained subprocesses with the inherited environment; not test-reachable today, no guard prevents it" `[VERIFIED: .planning/WINDOWS.md entry 6]`.
**How to avoid:** inject the spawn (Pattern: `route_event`'s "post is injected so the routing/fallback decision is unit-testable" `[VERIFIED: baude-core/src/hook.rs:431-433]`) so tests NEVER reach a real spawn; update WINDOWS entry 6 to name the opener as a third member, or record its injected-by-construction test unreachability.
**Warning signs:** a test that opens a real browser; `assert-real-roots-untouched.sh` regressions.

## Code Examples

### OSC 8 dispatch arm (fork, screen.rs)
```rust
// Format: ESC ] 8 ; params ; URI ST  — params are `:`-separated key=value, only `id` defined.
// Source: egmontkob/Hyperlinks gist (de-facto spec); vte param splitting verified in source.
(Some(&b"8"), Some(link_params)) => {
    // vte split the OSC string on ';' — everything from index 2 on is the URI.
    let uri: Vec<u8> = params[2..].join(&b';');
    if uri.is_empty() {
        self.attrs.link = None;                       // OSC 8 ; ; ST closes the link
    } else if let Ok(uri) = String::from_utf8(uri) {
        let id = parse_id_param(link_params);          // "id=x:foo=y" -> "x" (may be empty)
        self.attrs.link = self.intern_link(id, uri);   // dedupe by (id, uri); None past caps
    }
}
```

### Validation (baude/src/links.rs)
```rust
/// LINK-07: only parsed http/https, no control chars pre- or post-percent-decode,
/// no whitespace. Returns the normalized URL that will be displayed AND passed to argv.
pub fn validate_http_url(raw: &str) -> Option<url::Url> {
    if raw.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None;                                   // pre-decode check on the raw string
    }
    let decoded = percent_encoding::percent_decode_str(raw)
        .decode_utf8()
        .ok()?;
    if decoded.chars().any(char::is_control) {
        return None;                                   // post-decode check
    }
    let parsed = url::Url::parse(raw).ok()?;
    matches!(parsed.scheme(), "http" | "https").then_some(parsed)
}
```

### Opener with injected spawn (baude/src/app.rs; pattern from hook.rs route_event)
```rust
#[cfg(target_os = "macos")]
const OPENER: &str = "open";
#[cfg(not(target_os = "macos"))]
const OPENER: &str = "xdg-open";

/// LINK-08: URL is argv DATA — never shell text. Detached, stdio null (open_editor precedent).
fn spawn_opener(url: &str) -> std::io::Result<()> {
    let child = Command::new(OPENER)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    std::thread::spawn(move || { let mut child = child; let _ = child.wait(); }); // reap
    Ok(())
}

/// `open` is injected so activation is unit-testable without opening anything —
/// the same seam shape as hook::route_event's injected `post` (hook.rs:431-433).
fn activate_link<F: FnOnce(&str) -> std::io::Result<()>>(&mut self, url: &url::Url, open: F) {
    match open(url.as_str()) {
        Ok(()) => self.set_message(format!("opening {}", display_truncated(url))),
        Err(e) => self.set_message(format!("open failed: {e} — session unaffected")),
    }
}
```

### Parser-level test shape (precedent: clipboard_tests, app.rs:5491-5524 — no fixture needed)
```rust
// Pure: no fs, no PTY, no HOME — outside every Phase-8 guarded resolver.
#[test]
fn osc8_target_not_label_and_wrap_survival() {
    let mut parser = vt100::Parser::new(4, 10, 50);
    parser.process(b"\x1b]8;;https://real.example/x\x1b\\click here\x1b]8;;\x1b\\");
    let screen = parser.screen();
    let id = screen.cell(0, 0).unwrap().link_id().expect("labeled cell carries link id");
    assert_eq!(screen.link_target(id), Some("https://real.example/x")); // LINK-01
    // "click here" wraps at col 10 onto row 1; both fragments share the id (LINK-03)
    assert_eq!(screen.cell(1, 0).unwrap().link_id(), Some(id));
    assert!(screen.row_wrapped(0));
}
```

### Bare-URL wrap join (gesture-time scan core)
```rust
// Exact join via public wrap metadata — same authority selection copy uses (app.rs:5460).
let mut logical = String::new();
let mut spans: Vec<(u16, u16, u16)> = Vec::new(); // (row, start_col, end_col) per fragment
for row in 0..rows {
    push_row_text(screen, row, &mut logical, &mut spans);
    if !screen.row_wrapped(row) {
        scan_line_for_urls(&logical, &spans, &mut out); // scheme-anchored, then trim `.,;:!?)]}'"`
        logical.clear(); spans.clear();
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Auto-linkify on hover/click (terminal owns gesture) | Explicit gesture + preview (hint mode) for embedded TUIs | — | Embedded terminal (baude) cannot delegate to the outer terminal's OSC8 handling: ratatui redraws cells, the outer terminal never sees the child's OSC 8 |
| vt100 0.15.2 (in-tree) | vt100 0.16.2 upstream (2025-07): callback API, OSC 52, breaking moves of `set_size`/`set_scrollback` to `Screen` | 0.16.0, 2025-07-08 | Still no OSC 8 → fork 0.15.2 now; a future 0.16 rebase is optional and unrelated to links |
| BEL-terminated OSC (xterm legacy) | ST (`ESC \`) preferred; both accepted | ongoing | vte already accepts both; no work needed |

**Deprecated/outdated:**
- vt100 0.16 removed `Screen: vte::Perform` and `Cell::Default` — irrelevant to the 0.15.2 fork but rules out "just upgrade" as a links strategy.

## Runtime State Inventory

Not a rename/refactor/migration phase — section not required. For completeness: link state is runtime-only terminal state (never persisted); no stored data, service config, OS registrations, secrets, or build artifacts are touched. Verified: no persistence schema change anywhere in this design.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `open`(macOS)/`xdg-open`(Linux) treat a single http(s) argv argument as "open in default browser" | Opener | Low — universal platform convention; failure path (set_message, session continues) covers a broken opener anyway |
| A2 | WHATWG/`url` crate strips ASCII tab/newline and trims C0/space before parsing (making pre-parse rejection load-bearing) | Pitfall 7 | None if wrong in either direction — the locked pre-parse rejection runs regardless; assumption only motivates ordering |
| A3 | ctrl+o is rarely used by child programs (readline operate-and-get-next) | Pitfall 5 | Low — keybind is Claude's discretion; discuss/plan can pick another chord; documented in help overlay either way |
| A4 | vte's >16-param OSC behavior merges subsequent bytes into the last param with separators dropped | Pitfall 2 | Low — `MAX_OSC_PARAMS`/`return` verified in source; only the precise merge shape is inferred. Worst case: a ≥14-semicolon URI validates to a wrong-but-still-validated URL; preview shows it before open |
| A5 | Dockerfile's runtime image lacks `xdg-open` | Environment | None — container runs bauded (opening is TUI-local by locked decision); TUI-in-container hits the non-fatal LINK-08 error path |

## Open Questions (RESOLVED)

1. **Ship the `contents_formatted` OSC 8 re-emission in this phase, or accept the pre-attach remote gap?** — RESOLVED: shipped this phase — plan 10-02 (contents_formatted OSC8 re-emission + pty snapshot round-trip).
   - What we know: live attach bytes carry OSC 8 correctly; only the join-snapshot path loses them (pty.rs:402). Success criterion 3 names "attached remote" explicitly.
   - What's unclear: whether the criterion is read as "links printed while attached" (satisfied without it) or "any visible link after attach" (requires it).
   - Recommendation: ship it — it is ~30 lines in the fork's formatted-writer plus one round-trip test, and it makes the remote path unconditionally equivalent.
2. **Scan extent for bare URLs: visible rows only, or visible + wrapped continuations crossing the view edge?** — RESOLVED: bounded off-screen wrap continuation adopted — plan 10-03.
   - What we know: hints label *visible* links; a URL soft-wrapped across the bottom edge has an off-screen tail; `set_scrollback` can address any scrollback row.
   - Recommendation: scan the visible window, but follow `row_wrapped` continuations beyond the edge (bounded, e.g. ≤4 extra rows) so the joined URL is complete even when its tail is off-screen; hint anchors stay on the visible fragment.
3. **Exact keybind** — ctrl+o recommended; final choice is planner/user discretion (must be documented in the help overlay and must not collide with the verified occupied set: ctrl+q, ctrl+\/ctrl+4, ctrl+e, ctrl+n, alt+←/→). — RESOLVED: ctrl+o adopted, collision-checked — plan 10-01.
4. **Fork hygiene in CI** — vendored crate as workspace member means `cargo fmt`/`clippy` touch it. Recommendation: run fmt once on vendored code at import, keep upstream tests, add a `vendor/vt100/README.md` recording the upstream commit/version and the baude-specific diff surface. — RESOLVED: fmt-at-import + fork README provenance — plan 10-01 Task 2.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `open` (macOS opener) | LINK-08 open path | ✓ | /usr/bin/open | — |
| `pbcopy` | LINK-06 copy path (existing) | ✓ | /usr/bin/pbcopy | — |
| `xdg-open` (Linux opener) | LINK-08 on Linux | ✗ on this dev machine (expected — macOS) | — | Non-fatal error path (set_message) IS the designed fallback; Linux smoke owed by SHIP-03 |
| cargo / rustc | build + tests | ✓ | 1.98.1 | — |
| crates.io network | vendoring the fork source | ✓ (source already in local registry cache: `~/.cargo/registry/src/…/vt100-0.15.2`) | 0.15.2 | copy from local cache |

**Missing dependencies with no fallback:** none.
**Missing dependencies with fallback:** `xdg-open` absence at runtime → LINK-08's designed non-fatal failure surface.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (built-in harness), workspace-wide; Phase-8 containment observer `scripts/assert-real-roots-untouched.sh` |
| Config file | Cargo.toml workspace (none needed for tests) |
| Quick run command | `cargo test -p baude links::` (new module) / `cargo test -p vt100` (fork) |
| Full suite command | `cargo test --workspace` (497+ tests; serial run bracketed by the real-roots observer in CI) |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| LINK-01 | OSC8 target (not label) captured per cell | unit (fork + baude) | `cargo test -p vt100 hyperlink` | ❌ Wave 0 |
| LINK-02 | wrap-joined bare URL, punctuation trimmed | unit (pure fn over Screen) | `cargo test -p baude links::bare_url` | ❌ Wave 0 |
| LINK-03 | ids survive scroll/scrollback/wrap/resize/overwrite/erase; remote parity | unit (fork grid ops) + round-trip (`contents_formatted` → fresh parser, the remote-snapshot equivalence) | `cargo test -p vt100 link_` + `cargo test -p baude-core pty::` | ❌ Wave 0 |
| LINK-04 | chord opens hint modal; modal swallows keys; no PTY forwarding while open | unit (App key dispatch; App::new("/not-a-repository") precedent at app.rs:6397) | `cargo test -p baude link_hints` | ❌ Wave 0 |
| LINK-05 | overlay model contains destination string | unit (overlay model, not pixels) | `cargo test -p baude link_hints::preview` | ❌ Wave 0 |
| LINK-06 | `c`/`y` dispatches destination to copy path | unit with injected copy sink | `cargo test -p baude link_hints::copy` | ❌ Wave 0 |
| LINK-07 | rejection matrix: schemes, control chars pre/post decode, whitespace, malformed | unit (validate_http_url table test) | `cargo test -p baude links::validate` | ❌ Wave 0 |
| LINK-08 | argv-data spawn; spawn Err → set_message, session running | unit with injected opener closure (route_event pattern) | `cargo test -p baude link_open` | ❌ Wave 0 |

Manual-only residue (feeds SHIP-03, not this phase's gate): real-browser opening, real terminal selection interplay — cannot be asserted headlessly; everything up to the `spawn` boundary is automated via injection.

### Sampling Rate
- **Per task commit:** `cargo test -p vt100 && cargo test -p baude links::` (< 30s)
- **Per wave merge:** `cargo test --workspace` + `cargo fmt --check` + `cargo clippy`
- **Phase gate:** full suite green under the real-roots observer before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `vendor/vt100/` — vendored fork with upstream test suite carried over (upstream tests are the regression floor; OSC8 tests added on top)
- [ ] `baude/src/links.rs` with `#[cfg(test)]` module — covers LINK-02, LINK-07
- [ ] fork hyperlink test module — covers LINK-01, LINK-03 grid behaviors
- [ ] `baude-core` subscribe round-trip test — covers LINK-03 remote parity
- [ ] Framework install: none (cargo test built-in)

**Note on Phase-8 isolation:** parser/validation/scan tests are pure (no fs, no PTY, no guarded resolver) — they need no `TestRedirect`/`BAUDE_TEST_FIXTURE_ROOT` and cannot escape by construction, matching the existing `clipboard_tests` precedent (app.rs:5491-5524). App-level tests follow existing `App::new` patterns; the opener is injected so no test reaches a real spawn (keeps WINDOWS entry 6 from growing a test-reachable member).

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | — |
| V3 Session Management | no | — |
| V4 Access Control | no | — |
| V5 Input Validation | yes | `url` crate parse + scheme allowlist + pre/post-decode control rejection (LINK-07) |
| V6 Cryptography | no | — |
| V12 (OS command execution) | yes | argv-data spawn, never shell (LINK-08, locked); mirrors Phase 9's D-09 ".mcp.json command stays raw argv data — direct spawn, no shell" precedent |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| OSC8 label spoofing (label ≠ target) | Spoofing | Overlay shows parsed destination; label never used as target (locked; LINK-05 structural) |
| Scheme smuggling (`file:`, `javascript:`, custom handlers) | Elevation | Allowlist exactly `http`/`https` after real parse (LINK-07) |
| Control/escape chars in target (terminal injection, argv trickery) | Tampering | Reject pre- AND post-percent-decode; display only the validated normalized form |
| Shell metacharacter injection | Elevation | `Command::new(opener).arg(url)` — URL is argv data; no shell ever sees it |
| Opener flag injection (`-...` as argv[1]) | Elevation | Structurally excluded: validated URLs begin `http`; no `--` needed but harmless to add |
| Resource exhaustion (giant/million URIs) | DoS | URI cap 2083 bytes; intern-table cap; scan only at gesture time |
| IDN homograph (`https://аpple.com`) | Spoofing | Residual/accepted for v2.2: overlay shows `url` crate's normalized form (punycode host in `as_str()` after parse); no confusable detection — record as accepted residual |
| Fail-open on validation error | — | Fail closed: any parse/validation failure → non-activatable plain text (project pattern: "safety-critical paths fail closed") |

## Project Constraints (from CLAUDE.md)

No `./CLAUDE.md` or `./.claude/CLAUDE.md` exists in this repository (verified this session). Governing constraints come from `.planning/REQUIREMENTS.md` Execution Constraints instead:
- "Keep the existing screen model authoritative. Link metadata must follow that model rather than independently reimplementing terminal state."
- Out of scope: "Full terminal-engine replacement — Prefer a verified narrow parser extension."
- Established patterns (CONTEXT.md): baude-core stays print-free, binaries own presentation; failures never kill a session; safety-critical paths fail closed.

## Sources

### Primary (HIGH confidence)
- In-repo source read this session: `baude-core/src/pty.rs` (parser feed, subscribe snapshot), `baude/src/remote.rs` (attach path), `baude/src/app.rs` (Modal, chords, clipboard, selection/wrap, open_editor, set_message/warn precedents), `baude/src/ui.rs` (draw/draw_term/draw_modal/help), `baude-core/src/hook.rs` (injected-post pattern), `baude-core/src/lib.rs`, Cargo.toml/Cargo.lock, `.planning/WINDOWS.md`, `.github/workflows/release.yml`
- Vendored registry source read this session: `vt100-0.15.2/src/{screen,attrs,cell,row}.rs`, `vte-0.11.1/src/lib.rs`
- crates.io via `cargo search`/`cargo tree` + package-legitimacy seam (url, percent-encoding: OK)

### Secondary (MEDIUM confidence)
- docs.rs vt100 0.16.2 `Cell` API (no hyperlink methods) — cross-checked with upstream CHANGELOG (github.com/doy/vt100-rust): no OSC 8 through 0.16.2; 0.15→0.16 breaking changes enumerated
- OSC 8 de-facto spec: gist.github.com/egmontkob/eb114294efbcd5adb1944c9f3cb5feda (format, id grouping, 2083-byte VTE/iTerm2 cap, ST vs BEL, 32-126 byte range)

### Tertiary (LOW confidence)
- Training knowledge flagged in Assumptions Log (A1-A3): opener argv convention, WHATWG whitespace stripping detail, ctrl+o availability

## Metadata

**Confidence breakdown:**
- Pipeline map / extension point: HIGH — every claim read from source at file:line this session
- Standard stack: HIGH — versions verified against Cargo.lock and crates.io; zero new external packages
- Fork design details (Attrs field, erase-strip, formatted re-emission): MEDIUM-HIGH — extension points verified in source; exact diff shape is design, validated by the fork's carried-over test suite
- Pitfalls: HIGH for source-verified (2,3,4,5,9), MEDIUM for behavior-inferred (vte param merge, opener exit semantics)

**Research date:** 2026-09-15
**Valid until:** ~2026-10-15 (stable domain; vt100 upstream moves slowly — recheck only if a vt100 release adds hyperlinks)
