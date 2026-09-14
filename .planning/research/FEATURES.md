# Feature Research

**Domain:** Rust terminal TUI for repository-centered AI sessions
**Milestone:** baude v2.2 Reliability and Terminal Usability
**Researched:** 2026-09-08
**Confidence:** HIGH for the approved scope and safety boundaries; MEDIUM for terminal-specific interaction details that require smoke testing

## Feature Landscape

### Table Stakes (Users Expect These)

These are release-blocking behaviors for v2.2. They are deliberately expressed as user-observable contracts rather than implementation claims.

| Feature | Why Expected | Complexity | User-observable behavior / implementation notes |
|---------|--------------|------------|-----------------------------------------------|
| Idempotent owned hook seeding | Reopening a session must not grow settings or run duplicate hooks | HIGH | Repeated seed/update leaves one baude-owned entry per intended group and preserves ordering/settings outside that owned group. A baude-owned seed is distinguishable from custom or mixed content. An exactly-one owned entry is not permission to delete a user's hook. |
| Custom and mixed hook preservation | Users may have hooks sharing the same event with baude | HIGH | Custom-only groups remain byte-for-byte or semantically unchanged. Mixed groups retain custom commands and settings while baude updates only its marked contribution. Ambiguous/unmarked entries are preserved rather than guessed to be owned. |
| Explicit state-lock admission refusal | A second baude must not load misleading state or overwrite a live owner | MEDIUM | If loading/admission cannot safely establish ownership, baude refuses the operation, exits or remains out of the mutation path, and states the lock/path and recovery action. It does not silently present stale state as current and does not offer a read-only mode in this milestone. PID may be shown as diagnostic context, never as authority. |
| Test isolation with zero user-data mutation | Running the suite must be safe on a developer machine and in CI | HIGH | Tests use unique fixture roots, isolated config/state/environment, and separate Git worktrees. Parallel tests cannot share global HOME, state files, ports, or mutable repositories. Every test proves cleanup or leaves only its fixture; user repositories and pre-existing data are unchanged. |
| Safe OSC 8 and bare-URL activation | Users expect terminal links to work without turning output into a command channel | HIGH | On supported terminals, labeled OSC 8 links and recognized bare `http`/`https` URLs activate only after an explicit user gesture. The displayed label may differ from the target, so preview/copy exposes the actual target. No shell interpolation or command execution is involved. Unsupported or malformed targets render as ordinary text. |
| Link interaction preserves terminal use | Mouse reporting, scrollback, wrapping, and selection are normal terminal workflows | HIGH | Link hit-testing follows the rendered cell and wrapped/scrollback coordinates. Selection and copy remain possible; a selection drag is not accidentally opened. Child mouse reporting is respected, with a documented modifier-click or menu path where the outer terminal intercepts plain clicks. If the terminal intercepts the modifier, baude cannot promise activation and must not steal the gesture. |
| Shift+Enter newline on capable terminals | Multiline agent input must not submit accidentally | HIGH | On terminals that report the distinguishing key event, Shift+Enter inserts a newline and ordinary Enter retains its current submit behavior. The TUI enables only a supported progressive keyboard mode, handles the event unambiguously, and restores the prior terminal mode on every exit path. Unsupported terminals retain the documented legacy fallback; normal Enter is unchanged. |
| v2.2 release evidence | Reliability fixes are incomplete without proof across environments | MEDIUM | Tests and CI pass, plus manual terminal smoke covers links, selection/scrollback, child mouse reporting, modifier interception, Shift+Enter, normal Enter, and terminal restoration. Release v2.2.0 is not published until these checks pass. |

### Differentiators (Optional Enhancements)

Valuable after the atomic contracts work; none is required to expand v2.2 scope.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Target preview and explicit copy action | Makes a labeled link trustworthy before opening | MEDIUM | Show/copy the hidden URI from a context menu or equivalent safe gesture. Keep this separate from opening and do not add file-link schemes in this milestone. |
| Clear link affordance and unsupported-terminal guidance | Helps users understand why a link is or is not clickable | LOW | Hover/underline or status text may identify a supported link. Explain fallback without claiming universal terminal support. |
| Structured lock diagnostics | Reduces recovery time when admission is refused | LOW | Include state path, observed owner metadata, and suggested repair steps. Do not turn PID liveness into an ownership decision. |
| Fixture cleanup preview | Builds confidence when a test needs cleanup | MEDIUM | A test helper may report intended fixture paths and require explicit approval for cleanup. It must never infer that a missing `.git` directory makes deletion safe. |

### Anti-Features (Commonly Requested, Often Problematic)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|--------------|-----------------|------------|
| Replace the whole hooks array on every spawn | Simplifies serialization | Deletes or rewrites custom hooks and causes accumulation when formats differ | Mark and reconcile only baude-owned seeded groups; preserve custom and mixed content. |
| Delete a user's hook because exactly one baude-owned group remains | “Clean up duplicates” sounds safe | Ownership count does not establish ownership of the enclosing user configuration | Treat exactly-one-owned as already converged; never delete unmarked user content. |
| Treat a PID as lock authority or add read-only lock mode now | PID checks seem actionable and read-only avoids refusal | PIDs can be reused or stale; read-only behavior broadens semantics without solving admission safety | Refuse explicitly, show PID only as diagnostic evidence, and require repair/retry. |
| Run tests against the process HOME or shared repository | Mirrors a real environment | Tests can alter hooks, state locks, worktrees, and user files or race in parallel | Per-test HOME/config/state, unique repositories and worktrees, and explicit cleanup assertions. |
| Delete any test path whose Git directory is missing | Makes cleanup appear robust | Missing metadata may indicate a moved, damaged, or user-owned checkout, not disposable data | Cleanup only known fixture paths after preview and explicit approval; otherwise leave and report. |
| Open links on output or through shell evaluation | Maximum convenience | Output is untrusted input; shell evaluation enables command injection and surprises | Permit only user gestures, validate supported schemes, and pass the target directly to the platform opener. |
| Make every URL scheme clickable | Broad link coverage | Adds file/local-resource and custom-protocol security and product scope | Limit v2.2 to safe, explicitly supported HTTP(S) targets and plain-text fallback. |
| Consume every mouse event for links | Makes hit-testing easy | Breaks child applications, text selection, and terminal scrollback | Respect child mouse reporting and use a documented modifier or context-menu gesture. |
| Always encode Shift+Enter as a guessed escape sequence | Works in one terminal configuration | Outer terminals and child protocols differ; unsupported terminals may submit or corrupt input | Capability-aware progressive protocol, legacy fallback, unchanged Enter, and guaranteed restore. |

## Feature Dependencies

```text
[Per-session isolated test environment]
    ├──requires──> [Hook preservation tests]
    ├──requires──> [Lock admission tests]
    └──requires──> [Worktree cleanup safety tests]

[Ownership classification for seeded hooks]
    └──requires──> [Idempotent hook reconciliation]

[Rendered-cell link metadata + coordinate mapping]
    └──requires──> [OSC 8 and bare-URL activation]
                         ├──requires──> [Scheme validation + direct opener]
                         └──constrained-by──> [selection, scrollback, child mouse reporting]

[Terminal capability/protocol detection]
    └──requires──> [Shift+Enter newline]
                         ├──requires──> [mode push/pop and all-exit restoration]
                         └──constrained-by──> [legacy fallback and outer-terminal interception]
```

### Dependency Notes

- Hook tests must cover baude-owned seeded groups, custom-only groups, and mixed groups before claiming #70 fixed.
- Lock refusal precedes any state hydration or mutation; diagnostics may include PID, but PID is not a proof of ownership.
- Test fixtures are infrastructure for all three reliability issues. Cleanup is a safety contract, not merely a test convenience.
- Link activation needs rendered-cell provenance to distinguish a hidden OSC 8 target from its label and to map wrapped/scrollback coordinates correctly.
- Keyboard enhancement must be scoped to supported terminals and popped/restored on normal exit, error, panic, and signal-related teardown paths that the application controls.

## MVP Definition

### Launch With (v2.2)

- [ ] Idempotent baude-owned hook reconciliation preserving custom and mixed hooks.
- [ ] Explicit lock admission refusal with actionable diagnostics and no PID-based authority.
- [ ] Isolated tests proving no user data, shared HOME, state, or worktree leakage.
- [ ] User-gesture-only OSC 8 and bare HTTP(S) links with safe target handling, selection preservation, and child mouse compatibility.
- [ ] Capability-aware Shift+Enter newline, unchanged normal Enter, terminal restoration, and documented legacy fallback.
- [ ] CI plus manual terminal smoke evidence before publishing v2.2.0.

### Add After Validation (v2.2.x)

- [ ] Better target preview/copy and link affordances, if smoke testing exposes discoverability gaps.
- [ ] More detailed lock and fixture cleanup diagnostics, without changing refusal or cleanup authority.

### Future Consideration (v2+)

- [ ] File links, browser panels, model switching, PWA hierarchy, or other unrelated terminal/product expansion. These are outside the approved milestone.

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Hook ownership and idempotence | HIGH | HIGH | P1 |
| Lock refusal and diagnostics | HIGH | MEDIUM | P1 |
| Test isolation and non-destructive cleanup | HIGH | HIGH | P1 |
| Safe links and selection/mouse behavior | HIGH | HIGH | P1 |
| Shift+Enter with restoration/fallback | HIGH | HIGH | P1 |
| Release tests, CI, and terminal smoke | HIGH | MEDIUM | P1 |
| Target preview/copy affordance | MEDIUM | MEDIUM | P2 |
| Enhanced diagnostics | MEDIUM | LOW | P2 |

**Priority key:** P1 must ship in v2.2.0; P2 follows only if the P1 contract is proven.

## Sources

- **HIGH:** baude `.planning/PROJECT.md`, current v2.1.0 baseline and approved v2.2 scope, read 2026-09-08.
- **HIGH:** Existing baude implementation and tests in `baude/src/app.rs`, `baude-core/src/pty.rs`, and related workspace files, inspected 2026-09-08. These establish current lifecycle, PTY mouse state, and test seams; proposed behavior above remains to be implemented or smoke-tested.
- **HIGH:** Kitty keyboard protocol documentation: https://sw.kovidgoyal.net/kitty/keyboard-protocol/ (progressive mode, ambiguity, push/pop restoration).
- **MEDIUM:** iTerm2 hyperlink feature guidance: https://stage.iterm2.com/feature-reporting/Hyperlinks_in_Terminal_Emulators.html (OSC 8 semantics and gesture/selection safety guidance).
- **MEDIUM:** Contour OSC 8 syntax: https://contour-terminal.org/vt-extensions/clickable-links/.
- **MEDIUM:** Solo terminal documentation: https://soloterm.com/docs/terminal/osc-kitty-protocol and https://soloterm.com/docs/terminal/basics (examples of URI validation, mouse-reporting conflicts, selection, and plain URL detection; not baude requirements).

---
*Feature research for: baude v2.2 reliability and terminal usability*
*Researched: 2026-09-08*
