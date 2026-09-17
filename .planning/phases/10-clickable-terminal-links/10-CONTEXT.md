# Phase 10: Clickable Terminal Links - Context

**Gathered:** 2026-09-15
**Status:** Ready for planning

<domain>
## Phase Boundary

Users can inspect, copy, and explicitly open validated HTTP(S) destinations
while terminal rendering, selection, and remote attachment remain
authoritative and safe.

In scope: OSC8 labeled-link capture with per-cell metadata (LINK-01, LINK-03),
bare-URL detection including soft-wrap joining and punctuation trimming
(LINK-02), an explicit activation gesture with preview and copy (LINK-04,
LINK-05, LINK-06), target validation refusing non-HTTP(S)/malformed/
control-bearing targets (LINK-07), and argv-data opening with non-fatal
failure handling (LINK-08).

Out of scope: Shift+Enter multiline input (Phase 11), the release gate
(Phase 12), schemes beyond http/https, and remote-side opening.

</domain>

<decisions>
## Implementation Decisions

### Activation Gesture & Interaction Model
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

### Detection & Cell Metadata
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

### Validation & Opening
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

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `baude/src/app.rs` has existing mouse handling and the `set_message`
  warning surface (`warn_prompt_mode_without_daemon` precedent, plus Phase
  9's `warn_seed_failures` aggregation pattern).
- Phase 8 fixture isolation (`TestRedirect`, `BAUDE_TEST_FIXTURE_ROOT`) for
  parser/interaction tests; Phase 7 established exact-child-process patterns
  for real-terminal tests.
- Existing PTY/terminal state pipeline (pty.rs, TUI render path) — research
  must map where cells and scrollback live and where OSC dispatch happens.
- Keyboard chord infrastructure from v0.9-v0.11 (ctrl+e/n/x chords) for the
  hint-mode keybind.

### Established Patterns
- baude-core stays print-free; binaries own presentation.
- Failures never kill a session; warnings surface via set_message/eprintln
  precedents.
- Safety-critical paths fail closed (Phase 6/8 removal authorization
  precedents).

### Integration Points
- Terminal-state/grid model for per-cell link ids.
- TUI event loop for the hint-mode key and overlay rendering.
- Clipboard: existing pbcopy/copy path (WINDOWS entry 6 notes an uncontained
  pbcopy spawn — reuse, don't duplicate).

</code_context>

<specifics>
## Specific Ideas

- LINK-01's core distinction: an OSC8 link's visible label must never be
  treated as the URL — the target parameter is the destination.
- Success criterion 3 explicitly includes attached remote terminals — tests
  should cover the attach path's parser, not only local PTY.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>
