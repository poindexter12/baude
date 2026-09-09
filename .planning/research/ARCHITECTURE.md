# Architecture Research

**Domain:** Reliability and terminal usability in baude's Rust TUI/daemon workspace
**Researched:** 2026-09-08
**Confidence:** HIGH for current boundaries; MEDIUM for OSC8 and modified-key terminal protocol details

## Standard Architecture

### System Overview

```text
Claude/OpenCode child PTY ──bytes──> one vt100 screen model ──> draw_term
          │                                  │                    │
          └── hook/config + lifecycle        └── terminal annotations (links)

TUI input ──> outer crossterm mode/key decoder ──> child protocol bytes
          └── mouse hit/drag selection ──> safe URL opener or clipboard

App restore / bauded restore ──> core persistence + held workspace lock
                                      └── typed contention/error projection
```

Keep local and remote rendering on the same terminal-model contract. `Pty` currently owns an `Arc<Mutex<vt100::Parser>>` and processes child bytes in its reader thread (`baude-core/src/pty.rs:20-31, 164-188`). `RemoteAttach` independently owns the same parser shape and processes WebSocket bytes (`baude/src/remote.rs:203-212, 244-281`). `ui::draw_term` is the existing single renderer (`baude/src/ui.rs:1140-1214`). Do not introduce a second text/screen model or replace vt100 merely to add links.

### Component Responsibilities

| Component | Status | Responsibility |
|---|---|---|
| `baude-core/src/hook.rs` | Modified | Normalize one baude-owned registration per event while preserving user and mixed groups; retain best-effort seed semantics. |
| `baude-core/src/backend/claude.rs:68-78` | Existing seam | Single shared seed entry point used by both TUI and daemon spawn paths. |
| `baude-core/src/persist.rs:164-186, 461-482` | Modified | Classify `WouldBlock` from the held state lock as explicit contention, without deleting or stealing the lock. |
| `baude/src/app.rs:1186-1274` / `bauded/src/manager.rs:456-530` | Modified | Preserve state on restore failure and project actionable lock guidance instead of generic admission degradation. |
| `baude-core/src/pty.rs` + `baude/src/remote.rs` | Modified minimally | Feed the shared terminal-model/annotation adapter; preserve PTY and WebSocket APIs. |
| `baude/src/terminal.rs` | New | UI-facing terminal annotation and capability policy, if the adapter cannot remain in core. |
| `baude/src/keys.rs:3-127` | Modified | Pure key encoding with an explicit modified-enter capability/fallback; add exhaustive keymap tests. |
| `baude/src/main.rs:97-104, 329-350` | Modified | Own outer terminal protocol guard and guaranteed restoration. |
| `baude/src/app.rs:3759-3917, 5250-5388` | Modified | Route key/mouse gestures, link hit testing, selection, child mouse tracking, and safe opening. |
| `baude/src/ui.rs:1140-1214` | Modified | Render vt100 cells plus annotation styling/OSC8 output without changing cell geometry. |

## Recommended Project Structure

```text
baude-core/src/
├── hook.rs       # idempotent event-group merge and seed
├── persist.rs    # typed lock contention and injected-root persistence
└── pty.rs        # child byte stream and vt100 ownership
baude/src/
├── terminal.rs   # NEW: annotation/capability adapter, if needed
├── keys.rs       # pure child-protocol encoder
├── app.rs        # gesture routing and restore messages
├── ui.rs         # one vt100 projection plus link overlay
└── main.rs       # outer terminal mode guard
bauded/src/
└── manager.rs    # daemon restore/persistence projection
```

The existing test seams are sufficient foundations: `App.persistence_root_for_test` (`app.rs:521, 1189-1204`), `Manager.persistence_target_for_test` (`manager.rs:99, 680-688`), `persist::load_for_workspace_strict_at` and `save_current_at_test` (`persist.rs:296-447`), and fixture roots built under `temp_dir` (`persist.rs:943-951`). Extend these seams before adding terminal tests.

## Architectural Patterns

### Pattern 1: Typed Lock Contention, Not Degraded Admission

`hold_state_lock` uses a process-held lock and `try_lock` (`persist.rs:450-481`), but all failures currently become `LoadError::Read`; both restore paths then emit a generic blocked message (`app.rs:1206-1214`, `manager.rs:490-500`). Add `LoadError::Locked { path, lock_path }` at the persistence boundary by classifying `io::ErrorKind::WouldBlock`. Keep the lock process-held for the lifetime of the owner. The UI/daemon can say another baude instance owns this workspace state, name the lock file, and advise stopping the owner or choosing another workspace. Never fall back to an empty state, remove the lock, or overwrite the owner.

### Pattern 2: One Shared Hook Registration with Structural Ownership

The shared `ClaudeBackend::prepare_cwd` already calls `hook::seed_settings` for both binaries. Make the merge algorithm identify baude-owned entries structurally and reconcile exactly one entry per event. Remove only duplicate baude entries, never a containing user group; preserve custom/mixed groups and all sibling settings. Prefer a stable ownership marker or narrowly recognized command identity over treating every command ending in ` hook` as baude-owned. Keep `current_exe()` resolution and best-effort spawn behavior. The hook subcommand and event transport remain unchanged.

### Pattern 3: One Screen Model, Metadata-Only Link Sidecar

Keep vt100 authoritative for characters, cursor, wrapping, erase, scroll, resize, wide cells, and colors. Add only link metadata: OSC8 state is captured while the same byte stream is processed; bare URLs are discovered from the current vt100 cells during rendering/selection. Do not maintain a second character grid. The adapter must apply link-range shifts for cursor movement, line wrap, erase, scroll, resize, and wide-cell occupancy, then be tested against synthetic ANSI streams. If vt100 exposes no safe extension seam, keep the metadata adapter adjacent to the parser and hide it behind the existing parser access rather than switching stacks.

Both `Pty` and `RemoteAttach` must invoke the same adapter for local and remote bytes. `draw_term` and `handle_mouse` query the same parser snapshot plus metadata. A resize or scrollback change invalidates/recomputes visible URL ranges from that snapshot; it must not reuse terminal-absolute coordinates blindly.

### Pattern 4: Separate Outer Terminal Modes from Child Protocol

`main.rs` owns raw mode, alternate screen, bracketed paste, and mouse capture. Add a scoped keyboard-protocol negotiation/restoration guard there, including panic cleanup. `keys::encode_key` remains a pure child encoder and receives a capability/fallback choice; it must not mutate outer terminal state. For supported terminals, Shift+Enter is encoded as the negotiated modified-enter sequence. For legacy terminals, use the documented newline fallback and never claim the distinction was detected. Add unit tests for key bytes and a real terminal smoke test for enter, resize, panic/quit restoration, and child receipt.

## Data Flow

1. **Test isolation first:** fixture root and per-test child environment (`HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`) are passed to APIs/`Command`; avoid process-global `set_var` in parallel tests. Hook/event paths need injected roots or unique fixture IDs, not real worktree `.claude` files or global `/tmp` names.
2. **Restore:** load through injected/current workspace root → classify lock contention or parse/migration failure → leave durable state untouched on error → project typed guidance in App/Manager.
3. **Hooks:** backend prepare → shared core merge → one owned event group per event → child invokes the binary → existing POST/file event transport.
4. **Terminal:** PTY/WebSocket bytes → shared vt100 adapter → parser snapshot + link metadata → `draw_term`; mouse down/drag preserves selection state, mouse up opens only a validated URL on a click gesture or copies selected text.
5. **Input:** outer crossterm event → app policy (child mouse mode versus link/selection mode) → pure `encode_key`/mouse encoder → local PTY or remote WebSocket.

## Anti-Patterns

- **Generic restore fallback:** treating a lock as malformed/missing state or silently starting with an empty repository set. Use a typed contention error.
- **Lock cleanup as recovery:** deleting `.state-*.lock` or taking over an owner’s lock. Locks are process ownership, not stale cache files.
- **Hook group replacement:** deleting whole event arrays or mixed groups. Reconcile only baude-owned registrations.
- **Parallel terminal model:** reparsing output into a second screen grid for links. Keep vt100 authoritative and metadata-only annotations.
- **Shell URL opening:** interpolating a URL into `sh -c`. Validate supported schemes and pass it as an argument to a platform opener.
- **Unconditional mouse forwarding:** child mouse tracking should win for child gestures; link/selection handling must be explicit and must not steal terminal application clicks.
- **Global test environment mutation:** changing process `HOME`/XDG variables in parallel tests. Prefer injected roots and isolated child-process environments.

## Integration Points

| Boundary | Communication | Notes |
|---|---|---|
| Backend ↔ hook | Direct core call | Existing shared seed seam; TUI and daemon stay symmetric. |
| Persistence ↔ restore | Typed `LoadError` | Preserve public daemon routes; only startup diagnostics/status become more precise. |
| PTY/WebSocket ↔ terminal adapter | Byte stream + synchronized snapshot | Same adapter and invariants for local/remote. |
| App ↔ keys | Pure function | Capability passed in; no terminal writes from key encoder. |
| App ↔ opener | Validated URL argument | User click only; no shell evaluation. |

## Ordered Build Boundaries

1. **Test-root isolation and lock diagnostics:** establish fixture ownership and typed contention before terminal changes; otherwise terminal tests can race global state and obscure failures.
2. **Hook reconciliation:** harden the existing shared seeding seam and test mixed/custom groups plus repeated TUI/daemon-style seeds.
3. **Terminal adapter and rendering metadata:** prove local and remote byte paths share one model; test scroll, erase, resize, wide Unicode, OSC8 labels, bare URLs, and selection.
4. **Outer modes and input:** add capability negotiation/restoration, keymap tests, child protocol tests, and click policy; finish with a real terminal smoke test.
5. **Validation/release:** run focused tests, CI gates, and v2.2.0 smoke/release checks only after the above boundaries are stable.

## Sources

- `.planning/PROJECT.md:17-103` (scope, constraints, and v2.2 boundaries)
- `baude-core/src/hook.rs:67-120, 188-210` (current merge/seed seam)
- `baude-core/src/backend/claude.rs:68-78` (shared TUI/daemon preparation)
- `baude-core/src/persist.rs:164-186, 296-447, 450-564` (load errors, injected roots, lock and atomic save)
- `baude/src/app.rs:1186-1274, 3759-3917, 5250-5388` (restore, input, mouse/selection)
- `bauded/src/manager.rs:456-530, 664-718` (daemon restore and persistence status)
- `baude-core/src/pty.rs:20-31, 164-272`; `baude/src/remote.rs:203-327` (local/remote vt100 paths)
- `baude/src/ui.rs:1140-1214` and `baude/src/keys.rs:3-127` (render and key seams)

---
*Architecture research for: baude v2.2 reliability and terminal usability*
*Researched: 2026-09-08*
