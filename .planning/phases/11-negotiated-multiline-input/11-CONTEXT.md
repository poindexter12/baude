# Phase 11: Negotiated Multiline Input - Context

**Gathered:** 2026-09-16
**Status:** Ready for planning

<domain>
## Phase Boundary

Users can insert newlines with Shift+Enter on verified terminal paths without
changing existing key behavior or leaking terminal modes.

In scope: bounded outer-terminal capability negotiation (TKEY-05), Shift+Enter
newline insertion into Claude/claudex prompts on verified paths (TKEY-01),
byte-identical legacy behavior everywhere else (TKEY-02, TKEY-03), and
keyboard-mode restoration across exit/failure/suspend/resume (TKEY-04).

Out of scope: baude's own input modal newline handling, the release gate
(Phase 12), and any change to link handling (Phase 10).

</domain>

<decisions>
## Implementation Decisions

### Capability Negotiation
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

### Mode Lifecycle
- Keyboard-enhancement flags are pushed only after successful negotiation and
  popped through the same guard-style restore path used for raw mode /
  alternate screen.
- Suspend pops the flags before handing the terminal back; resume re-pushes
  (no re-query — outer support cannot change mid-session).
- All controlled failure paths (normal exit, panic hook, error exits) restore
  keyboard mode at the same point the screen already restores (TKEY-04).
- Tests assert the pop sequence is emitted on every controlled path;
  real-terminal residue checking folds into Phase 12 validation.

### Key Behavior
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

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- Existing terminal setup/teardown (raw mode, alternate screen) and any
  panic-hook restore path in baude/src/main.rs / app.rs — the keyboard-mode
  pop must join it.
- Existing suspend/resume handling if present (ctrl+z) — research must map it.
- PTY write path for child input forwarding (pty.rs / app key handling).
- Phase 8 fixture isolation for input/lifecycle tests; Phase 10's injected
  side-effect seam precedents for testability.
- crossterm (ratatui backend) likely provides the enhancement probe and
  Push/PopKeyboardEnhancementFlags — research confirms version support.

### Established Patterns
- Failures never kill a session; warnings via set_message.
- Guard-style (RAII/single restore point) resource management for terminal
  state.
- Byte-level regression tests with injected seams rather than live spawns.

### Integration Points
- Terminal init/restore path (main.rs/app startup and exit).
- Key event handling and PTY forwarding in app.rs.
- Help overlay + README for guidance text.

</code_context>

<specifics>
## Specific Ideas

- TKEY-03's "baude does not guess a missing modifier" is a hard line: no
  timing heuristics on Enter.
- TKEY-05's "verified outer-terminal/child input path" means both ends —
  outer capability alone is insufficient to start emitting enhanced bytes.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>
