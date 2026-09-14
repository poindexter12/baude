# Requirements: baude v2.2 Reliability and Terminal Usability

**Defined:** 2026-09-08
**Target release:** v2.2.0
**Baseline:** v2.1.0
**Core Value:** See which coding-agent session needs attention and act from the terminal or phone.

## v2.2 Requirements

Approved scope: GitHub #70, #71, #72, clickable terminal links, and Shift+Enter. Target preview and copy-link actions were explicitly selected during requirements scoping. Proxy-monitor integration is deferred.

### Test Isolation (#72)

- [ ] **TISO-01**: A developer can run repository/worktree tests with every created repository, worktree, config, and state file confined to a unique test-owned temporary root.
- [ ] **TISO-02**: A developer can run those tests concurrently without changing the parent process's HOME/XDG environment or sharing cached workspace identity between fixtures.
- [ ] **TISO-03**: A developer receives a failing test when a tested creation path attempts to escape its fixture root, without writing to the real user data directory.
- [ ] **TISO-04**: A developer can preview suspected historical test-worktree leaks without deleting them; removal outside newly created test fixtures requires separate approval and verified ownership, never a missing gitdir alone.

### Hook Registration (#70)

- [ ] **HREG-01**: Opening or reopening sessions from different baude/bauded executable paths converges each of the four lifecycle events to one recognized baude-owned registration using the current executable.
- [ ] **HREG-02**: A user's custom hooks, mixed groups, ambiguous registrations, and unrelated settings remain unchanged during owned-registration reconciliation.
- [ ] **HREG-03**: A user retains existing settings unchanged when seeding cannot safely parse or update them and receives an actionable warning instead of silent replacement with empty settings.
- [ ] **HREG-04**: A user can launch baude from a path containing spaces or shell metacharacters and have the seeded hook invoke that exact executable safely and idempotently.

### Workspace Lock Diagnostics (#71)

- [ ] **WLOCK-01**: Starting a second instance for an already-owned workspace reports explicit lock contention before accepting session operations, rather than starting in a misleading degraded state.
- [ ] **WLOCK-02**: A second instance cannot replace another owner's state, remove its lock, or interfere with its live sessions.
- [ ] **WLOCK-03**: A user sees the affected workspace/state path, owner PID when reliably recorded, and a recovery action such as closing the other instance or selecting another workspace; PID metadata is diagnostic only.
- [ ] **WLOCK-04**: A user can reopen the workspace after the OS lock is released even if its lock file remains, while malformed or unreadable state is still reported distinctly and preserved.

### Clickable Terminal Links

- [ ] **LINK-01**: A user can activate a labeled OSC8 HTTP(S) link emitted in an agent or shell pane, opening its target rather than interpreting its visible label as a URL.
- [ ] **LINK-02**: A user can activate a bare HTTP(S) URL, including one soft-wrapped across terminal rows, without adding surrounding prose punctuation or dropping valid URL characters.
- [ ] **LINK-03**: Links remain associated with the correct rendered cells through scrolling, scrollback, wrapping, resizing, overwrites, and erasure, in both local and attached remote TUI terminals.
- [ ] **LINK-04**: A user can activate links with a documented explicit gesture while ordinary text selection, drag-copy, scrolling, and supported child mouse behavior remain usable; output alone never opens a link.
- [ ] **LINK-05**: A user can inspect the actual destination of a labeled link before opening it.
- [ ] **LINK-06**: A user can copy a link's actual destination without opening it.
- [ ] **LINK-07**: A user sees unsupported, malformed, or control-character-bearing targets as non-activatable text; v2.2 opens only validated HTTP(S) destinations.
- [ ] **LINK-08**: Opening an allowed link passes the target as data to the platform opener, never shell code, and an opener failure leaves the session running with a useful error.

### Multiline Input

- [ ] **TKEY-01**: A user can press Shift+Enter to insert a newline without submitting in Claude/claudex prompts on documented, tested terminal paths that report the modifier distinctly.
- [ ] **TKEY-02**: Ordinary Enter, Ctrl-C, navigation keys, and existing baude shortcuts retain their behavior after enhanced keyboard handling is enabled.
- [ ] **TKEY-03**: A user of a terminal that cannot distinguish Shift+Enter retains legacy input behavior and receives documented multiline-input setup or fallback guidance; baude does not guess a missing modifier.
- [ ] **TKEY-04**: The outer terminal's previous keyboard mode is restored on normal exit and application-controlled failure/suspend paths, and correctly re-established on resume.
- [ ] **TKEY-05**: Keyboard capability negotiation cannot indefinitely block startup or input, and enhanced sequences are sent only on a verified supported outer-terminal/child input path.

### Verification and Release

- [ ] **SHIP-01**: A maintainer can run focused regression tests plus workspace tests, fmt, clippy, and the existing supported-platform CI checks successfully after test isolation is in place.
- [ ] **SHIP-02**: A user can find documented link activation/preview/copy gestures, tested terminal support, Shift+Enter setup or fallback, and workspace-lock recovery instructions.
- [ ] **SHIP-03**: A maintainer has recorded real macOS/Linux terminal smoke evidence covering links, selection/scrollback, mouse interaction, Shift+Enter, ordinary Enter, and terminal restoration before release approval.
- [ ] **SHIP-04**: A maintainer can publish v2.2.0 through the existing release workflow after verification passes, with matching release notes/version metadata and the existing supported binary/container distribution outputs.

## Future Requirements

- **PMON-01**: A proxy-backed session can expose read-only request/provider/model/timing/usage telemetry in baude after a usable external telemetry and session-identity contract is established.
- Additional link schemes and richer terminal gestures can be considered after HTTP(S) support is validated; they are not prerequisites for v2.2.

## Out of Scope

| Feature | Reason |
|---------|--------|
| Proxy-monitor integration | Explicitly deferred; upstream telemetry access and session correlation remain unresolved. |
| File, SSH, mailto, or custom URL schemes | Keep activation bounded to HTTP(S) for this release. |
| Automatic cleanup of pre-existing user-data directories | Missing Git metadata does not establish disposable ownership; preview and separate approval are required. |
| Forced lock takeover or a new read-only workspace mode | Preserve the single-writer contract and keep the lock fix focused on safe refusal and recovery guidance. |
| Full terminal-engine replacement | Prefer a verified narrow parser extension; any broader redesign requires a new scope decision. |
| PWA hyperlink/input redesign or repository hierarchy parity | This milestone improves TUI terminal panes and shared safety boundaries, not the phone UI or deferred lifecycle breadth. |
| Universal Shift+Enter detection in legacy terminals | Some terminals send exactly the same bytes as Enter; retain honest fallback guidance. |

## Execution Constraints

- Implement fixture isolation before broad test execution; do not exercise the known leaking worktree paths against the real HOME/XDG directories.
- Keep the existing screen model authoritative. Link metadata must follow that model rather than independently reimplementing terminal state.
- Keep outer terminal capability negotiation separate from child input encoding. Restore state on paths the application can control; no restoration guarantee is made for SIGKILL or power loss.
- Preserve malformed user settings and active-owner state. Existing hook drafts and GitHub issue suggestions are design inputs, not authority to delete user configuration.
- No implementation tests or manual terminal smoke have been performed for these requirements at definition time.

## Traceability

Each v2.2 requirement maps to exactly one proposed roadmap phase; roadmap approval is pending. Continue after archived Phase 7; do not reset numbering.

| Requirement | Phase | Status |
|-------------|-------|--------|
| TISO-01 | Phase 8 | Pending |
| TISO-02 | Phase 8 | Pending |
| TISO-03 | Phase 8 | Pending |
| TISO-04 | Phase 8 | Pending |
| HREG-01 | Phase 9 | Pending |
| HREG-02 | Phase 9 | Pending |
| HREG-03 | Phase 9 | Pending |
| HREG-04 | Phase 9 | Pending |
| WLOCK-01 | Phase 9 | Pending |
| WLOCK-02 | Phase 9 | Pending |
| WLOCK-03 | Phase 9 | Pending |
| WLOCK-04 | Phase 9 | Pending |
| LINK-01 | Phase 10 | Pending |
| LINK-02 | Phase 10 | Pending |
| LINK-03 | Phase 10 | Pending |
| LINK-04 | Phase 10 | Pending |
| LINK-05 | Phase 10 | Pending |
| LINK-06 | Phase 10 | Pending |
| LINK-07 | Phase 10 | Pending |
| LINK-08 | Phase 10 | Pending |
| TKEY-01 | Phase 11 | Pending |
| TKEY-02 | Phase 11 | Pending |
| TKEY-03 | Phase 11 | Pending |
| TKEY-04 | Phase 11 | Pending |
| TKEY-05 | Phase 11 | Pending |
| SHIP-01 | Phase 12 | Pending |
| SHIP-02 | Phase 12 | Pending |
| SHIP-03 | Phase 12 | Pending |
| SHIP-04 | Phase 12 | Pending |

**Coverage:**
- v2.2 requirements: 29 total
- Mapped to phases: 29
- Unmapped: 0

---
*Requirements defined: 2026-09-08*
*Last updated: 2026-09-08 after full requirements approval*
