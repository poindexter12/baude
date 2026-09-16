# Requirements: baude v2.2 Reliability and Terminal Usability

**Defined:** 2026-09-08
**Target release:** v2.2.0
**Baseline:** v2.1.5 (re-baselined 2026-09-13; original baseline v2.1.0)
**Core Value:** See which coding-agent session needs attention and act from the terminal or phone.

## v2.2 Requirements

Approved scope: GitHub #70, #71, #72, clickable terminal links, and Shift+Enter. Target preview and copy-link actions were explicitly selected during requirements scoping. Proxy-monitor integration is deferred.

**Reconciliation 2026-09-13.** Requirements were defined against v2.1.0. Releases v2.1.2 through v2.1.5 then shipped fixes for the same issues (#70, #71, #72, #78) ahead of milestone execution. Each requirement below was re-verified against code on main at v2.1.5; delivered ones are checked with their evidence, and the surviving gaps are what Phases 8 and 9 now cover. Issue closure did not by itself count as delivery — six of twelve requirements shipped, four are partial, and two were never started.

### Test Isolation (#72)

- [x] **TISO-01** (delivered plan 08-06): A developer can run repository/worktree tests with every created repository, worktree, config, and state file confined to a unique test-owned temporary root.
  - Shipped v2.1.4: managed worktrees and repos are confined; state is redirectable (`git.rs:1744`, `app.rs:5551`, `app.rs:1202`).
  - Closed by plan 08-01: `baude_core::testing::TestRedirect` redirects the config dir, state dir, managed worktrees root, hook command and Claude config dir in one guard; plan 08-02 redirected `bauded/src/push.rs`'s VAPID storage (regressions at `push.rs:390`, `:432`, `:479`).
  - Closed by plan 08-06: `ManagerFixture` makes a `bauded` fixture one construction holding both guards as fields, and `scripts/assert-real-roots-untouched.sh` proves a full 497-test serial run creates or modifies none of the three real roots.
- [x] **TISO-02** (delivered plan 08-06): A developer can run those tests concurrently without changing the parent process's HOME/XDG environment or sharing cached workspace identity between fixtures.
  - Shipped: no parent-process HOME/XDG mutation; both test redirects are thread-local (`git.rs:1727`, `hook.rs:80`).
  - Closed by plan 08-03: `workspace::override_for_test(&Config, hint)` gives each fixture a literal identity in the same thread-local redirect storage, and `active()` panics in a support build with no override rather than falling back to the process cache.
  - Closed by plan 08-06: both downstream binaries run UNSERIALIZED inside the observer's before/after bracket with no root change, so concurrency does not alter containment.
- [x] **TISO-03** (delivered plan 08-06): A developer receives a failing test when a tested creation path attempts to escape its fixture root, without writing to the real user data directory.
  - Shipped v2.1.4: `REQUIRE_WORKTREES_OVERRIDE` asserts on managed-worktree escape (`git.rs:1736`), with a containment test (`app.rs:5635`).
  - Closed by plan 08-01: `assert_contained` (`testing.rs:278`) covers the config, state, Claude and managed-worktrees resolutions, keys on `BAUDE_TEST_FIXTURE_ROOT` (independent of XDG, so it arms in every test binary — proven for both downstream binaries by `c52be5b`), and panics at resolution, so an escape performs no real-root I/O.
  - Closed by plan 08-06: the guard demonstrably fired across the first broad run, catching 22 `manager`, 8 `api`, 5 `lifecycle` and 2 `app` escapes, all since owned; the suite-level observer asserts the aggregate claim the per-resolver guards only constrain.
  - Residual (tracked, not part of this requirement): `App::open_editor` (`app.rs:5077`) and `copy_to_clipboard` (`app.rs:5442`) spawn subprocesses with the inherited environment rather than through the contained launcher. Neither is test-reachable; WINDOWS entry 6.
- [x] **TISO-04**: A developer can preview suspected historical test-worktree leaks without deleting them; removal outside newly created test fixtures requires separate approval and verified ownership, never a missing gitdir alone.
  - Shipped plan 08-04: read-only enumeration and the `Evidence`/`Verdict` classification, with a missing gitdir explicitly unable to clear a candidate (`worktree_scan.rs`).
  - Shipped plan 08-05: state cross-referencing as ownership-negative evidence, and `prune_at(roots, report, confirmed)` — full re-derivation plus proof equality plus a non-defaulting confirmation parameter.
  - Gap: no CLI surface yet, so a developer still cannot *run* a preview. Plan 08-07 exposes both through `baude worktrees` with `--prune` and a separate `--yes`.

### Hook Registration (#70)

- [x] **HREG-01** (delivered v2.1.2): Opening or reopening sessions from different baude/bauded executable paths converges each of the four lifecycle events to one recognized baude-owned registration using the current executable.
- [x] **HREG-02** (delivered v2.1.2): A user's custom hooks, mixed groups, ambiguous registrations, and unrelated settings remain unchanged during owned-registration reconciliation.
- [x] **HREG-03** (not started): A user retains existing settings unchanged when seeding cannot safely parse or update them and receives an actionable warning instead of silent replacement with empty settings.
  - `seed_settings` degrades an unparseable file to `json!({})` and overwrites it (`hook.rs:282-288`); every fs call is `let _ =` and no warning path exists. `backend/claude.rs:114-121` repeats the pattern for `.mcp.json`.
- [x] **HREG-04** (delivered plan 09-03): A user can launch baude from a path containing spaces or shell metacharacters and have the seeded hook invoke that exact executable safely and idempotently.
  - `baude_hook_command` now emits the POSIX single-quoted canonical form via `quote_posix_single`; `is_seeded_hook_command` accepts quoted (strict round-trip) and legacy forms; `sh -c` E2E proves exact-executable invocation on a space/`$`/`;`/backtick/embedded-`'` path.

### Workspace Lock Diagnostics (#71)

- [x] **WLOCK-01** (delivered v2.1.3; TUI refuses before session operations (`main.rs:301-327`). `bauded` still learns of contention at first save rather than at startup): Starting a second instance for an already-owned workspace reports explicit lock contention before accepting session operations, rather than starting in a misleading degraded state.
- [x] **WLOCK-02** (delivered v2.1.3; every write claims the lock before creating a temp (`persist.rs:624`) and no code removes a lock file): A second instance cannot replace another owner's state, remove its lock, or interfere with its live sessions.
- [x] **WLOCK-03** (delivered v2.1.3; message names workspace, pid, lock path and recovery (`main.rs:311-326`), pid is diagnostic only): A user sees the affected workspace/state path, owner PID when reliably recorded, and a recovery action such as closing the other instance or selecting another workspace; PID metadata is diagnostic only.
- [x] **WLOCK-04** (delivered v2.1.3; contention is decided by `try_lock` alone, never file existence (`persist.rs:579-594`). The reopen-with-leftover-lock-file sequence is pinned by `persist::tests::reopen_claims_lock_when_leftover_file_outlives_released_os_lock` since plan 09-02): A user can reopen the workspace after the OS lock is released even if its lock file remains, while malformed or unreadable state is still reported distinctly and preserved.

### Clickable Terminal Links

- [x] **LINK-01**: A user can activate a labeled OSC8 HTTP(S) link emitted in an agent or shell pane, opening its target rather than interpreting its visible label as a URL.
- [x] **LINK-02**: A user can activate a bare HTTP(S) URL, including one soft-wrapped across terminal rows, without adding surrounding prose punctuation or dropping valid URL characters.
- [x] **LINK-03**: Links remain associated with the correct rendered cells through scrolling, scrollback, wrapping, resizing, overwrites, and erasure, in both local and attached remote TUI terminals.
- [x] **LINK-04**: A user can activate links with a documented explicit gesture while ordinary text selection, drag-copy, scrolling, and supported child mouse behavior remain usable; output alone never opens a link.
- [x] **LINK-05**: A user can inspect the actual destination of a labeled link before opening it.
- [x] **LINK-06**: A user can copy a link's actual destination without opening it.
- [x] **LINK-07**: A user sees unsupported, malformed, or control-character-bearing targets as non-activatable text; v2.2 opens only validated HTTP(S) destinations.
- [x] **LINK-08**: Opening an allowed link passes the target as data to the platform opener, never shell code, and an opener failure leaves the session running with a useful error.

### Multiline Input

- [x] **TKEY-01**: A user can press Shift+Enter to insert a newline without submitting in Claude/claudex prompts on documented, tested terminal paths that report the modifier distinctly.
- [x] **TKEY-02**: Ordinary Enter, Ctrl-C, navigation keys, and existing baude shortcuts retain their behavior after enhanced keyboard handling is enabled.
- [x] **TKEY-03**: A user of a terminal that cannot distinguish Shift+Enter retains legacy input behavior and receives documented multiline-input setup or fallback guidance; baude does not guess a missing modifier.
- [x] **TKEY-04**: The outer terminal's previous keyboard mode is restored on normal exit and application-controlled failure/suspend paths, and correctly re-established on resume.
- [x] **TKEY-05**: Keyboard capability negotiation cannot indefinitely block startup or input, and enhanced sequences are sent only on a verified supported outer-terminal/child input path.

### Verification and Release

- [ ] **SHIP-01**: A maintainer can run focused regression tests plus workspace tests, fmt, clippy, and the existing supported-platform CI checks successfully after test isolation is in place.
- [x] **SHIP-02**: A user can find documented link activation/preview/copy gestures, tested terminal support, Shift+Enter setup or fallback, and workspace-lock recovery instructions.
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

Each v2.2 requirement maps to exactly one roadmap phase. Continue after archived Phase 7; do not reset numbering. Status reflects verification against main at v2.1.5 on 2026-09-13, not issue closure.

| Requirement | Phase | Status |
|-------------|-------|--------|
| TISO-01 | Phase 8 | Delivered plan 08-06 |
| TISO-02 | Phase 8 | Delivered plan 08-06 |
| TISO-03 | Phase 8 | Delivered plan 08-06 |
| TISO-04 | Phase 8 | Delivered plan 08-07 |
| HREG-01 | Phase 9 | Delivered v2.1.2 |
| HREG-02 | Phase 9 | Delivered v2.1.2 |
| HREG-03 | Phase 9 | Complete |
| HREG-04 | Phase 9 | Delivered plan 09-03 |
| WLOCK-01 | Phase 9 | Delivered v2.1.3 |
| WLOCK-02 | Phase 9 | Delivered v2.1.3 |
| WLOCK-03 | Phase 9 | Delivered v2.1.3 |
| WLOCK-04 | Phase 9 | Delivered v2.1.3 |
| LINK-01 | Phase 10 | Complete |
| LINK-02 | Phase 10 | Complete |
| LINK-03 | Phase 10 | Complete |
| LINK-04 | Phase 10 | Complete |
| LINK-05 | Phase 10 | Complete |
| LINK-06 | Phase 10 | Complete |
| LINK-07 | Phase 10 | Complete |
| LINK-08 | Phase 10 | Complete |
| TKEY-01 | Phase 11 | Complete |
| TKEY-02 | Phase 11 | Complete |
| TKEY-03 | Phase 11 | Complete |
| TKEY-04 | Phase 11 | Complete |
| TKEY-05 | Phase 11 | Complete |
| SHIP-01 | Phase 12 | Pending |
| SHIP-02 | Phase 12 | Complete |
| SHIP-03 | Phase 12 | Pending |
| SHIP-04 | Phase 12 | Pending |

**Coverage:**

- v2.2 requirements: 29 total
- Mapped to phases: 29
- Unmapped: 0
- Delivered before execution: 6 (HREG-01, HREG-02, WLOCK-01 through WLOCK-04)
- Delivered during execution: 5 (TISO-01, TISO-02, TISO-03 — plan 08-06; TISO-04 — plan 08-07; HREG-04 — plan 09-03)
- Open: 20

---
*Requirements defined: 2026-09-08*
*Last updated: 2026-09-15 — TISO-04 closed by plan 08-07 (`baude worktrees scan` preview surface)*
