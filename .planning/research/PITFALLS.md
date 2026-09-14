# Pitfalls Research

**Domain:** baude v2.2 reliability and terminal usability
**Researched:** 2026-09-08
**Confidence:** HIGH for project evidence and protocol rules; MEDIUM for cross-terminal behavior

## Critical Pitfalls

### 1. Hook ownership is inferred from an exact command string

**What goes wrong:** Re-spawning after a binary upgrade appends another baude hook because `current_exe()` changed. Conversely, matching only `file_name` or `file_stem` deletes or rewrites a user's executable with arguments, a wrapper, or a similarly named command. A mixed group containing user hooks is mistaken for a baude-only group.

**Why it happens:** `baude_hook_command()` formats an unquoted path plus ` hook` (`baude-core/src/hook.rs:67-79`), while `merge_hook_settings()` uses exact command equality (`:81-120`). Hook commands are shell strings, not argv records; paths can contain spaces and user commands can contain arguments.

**How to avoid:** Use a single explicit ownership predicate that recognizes only baude's executable plus the literal `hook` subcommand, with shell-safe quoting or an argv-safe registration mechanism. Treat executable identity changes as replacement of the owned entry, not an excuse to append. Remove only an independently proven baude entry; preserve every mixed group and unknown command. Match the full executable identity, never `file_stem` alone.

**Warning signs:** Duplicate baude entries after a release; generated `"/path with spaces/baude hook"` that the shell cannot execute; tests passing only `/abs/path/baude hook`; a predicate using `starts_with`, `file_name`, or `file_stem`; one event group disappearing when it also contains a user hook.

**Validation evidence:** Fixtures cover spaces/quotes, old and new absolute paths, `baude` versus `bauded`, wrappers, arguments, all four events, mixed groups, and repeated runs. Assert byte-for-byte preservation of unrelated settings and invalid JSON. **Phase:** Hook registration (#70), before release smoke.

### 2. Invalid settings are silently replaced

**What goes wrong:** A malformed or unreadable `.claude/settings.local.json` is treated as `{}` and overwritten, destroying the user's recoverable configuration. A non-array event key must not be coerced into an array merely to install a hook.

**Why it happens:** `seed_settings()` falls back to an empty object on both read and parse failure, then writes the merge (`hook.rs:199-209`); `merge_hook_settings()` intentionally skips non-array event values but does not solve the read/write fallback.

**How to avoid:** Distinguish missing, unreadable, malformed, and valid settings. Install into a missing file; on any existing-file read/parse/write ambiguity, leave it untouched, report a diagnostic, and use the existing silence fallback. Use atomic replacement only after valid JSON is parsed and ownership-preserving merge succeeds.

**Warning signs:** A failed `serde_json::from_str` followed by `json!({})`; file size changes after malformed input; spawn succeeds while the settings error is invisible; tests assert only that a new file exists.

**Validation evidence:** malformed JSON, permission denial, directory-at-file-path, scalar root, `hooks` scalar, and non-array event tests prove the original bytes remain and no user hook is removed. **Phase:** Hook registration (#70).

### 3. Lock metadata is treated as authority or stale locks are deleted

**What goes wrong:** A PID in a lock file is reported as the owner after PID reuse, a stale-looking file is deleted while another process holds the OS lock, or a diagnostic path overwrites another owner's state. A failed save/load leaves the process's lock bookkeeping inconsistent and later recovery cannot proceed.

**Why it happens:** The current lock is an OS `try_lock` held in a process-global map (`persist.rs:450-481`); a future diagnostic PID is only evidence. File existence and PID liveness cannot establish ownership.

**How to avoid:** Acquire the OS advisory lock first and never infer ownership from PID text. PID/start-time metadata is diagnostic only and must be written without truncating the lock file or replacing another owner's file. On contention, report path, PID metadata if readable, and “owner not authoritative”; never unlink or overwrite. Release/retain the guard with clear scope and test process exit recovery through a child process.

**Warning signs:** `exists()` or `kill(pid, 0)` decides stale; `truncate(true)` on the lock path; automatic unlink on timeout; a diagnostic PID is used to force unlock; same-process `OnceLock` makes a second test appear to own a lock.

**Validation evidence:** live child holder, dead PID, reused-looking PID, malformed/empty metadata, stale file with live OS lock, abrupt holder exit, and failed save cases. A contender must fail without changing bytes or lock ownership; after holder exit a fresh process can acquire. **Phase:** State-lock diagnostics (#71), with CI process tests.

### 4. Test isolation mutates process-global environment and workspace state

**What goes wrong:** Tests read another test's HOME/XDG/config, share state or worktrees, or permanently win `workspace::ACTIVE` and make later tests exercise the wrong backend. Parallel tests race on `std::env::set_var`; cleanup then removes a directory belonging to another test.

**Why it happens:** `workspace::ACTIVE` is a process-global `OnceLock` (`workspace.rs:199-223`), and Rust documents `set_var`/`remove_var` as unsound in multithreaded Unix programs. Existing fixtures use process-ID plus a counter and direct temp paths (`git.rs:2486-2499`), while app subprocess tests explicitly set child HOME/XDG (`app.rs:7476-7491`).

**How to avoid:** Inject config/state/worktree roots into APIs and pass HOME/XDG with `Command::env`, not process mutation. Keep OnceLock tests pure or execute environment-sensitive scenarios in isolated child processes. Give every fixture a cryptographically/OS-unique root, record ownership, and cleanup only paths created by that fixture. If `.git`/gitdir is missing, stop cleanup and preserve the directory; never infer that a missing gitdir grants deletion authority.

**Warning signs:** tests call `set_var`/`remove_var` after threads start; `active()` appears in unit fixtures; cleanup is unconditional `remove_dir_all`; fixture cleanup checks only path text; failures vary with test order or `--test-threads`.

**Validation evidence:** two concurrent subprocess fixtures have distinct HOME/XDG/state/worktree roots; a missing `.git` and an unrelated sentinel directory survive cleanup; serial CI is not required to hide races. **Phase:** Fixture isolation (#72), then CI hardening.

### 5. OSC 8 links create spoofing, injection, or grid/selection regressions

**What goes wrong:** A hostile label/control sequence changes terminal presentation, a displayed `https` label opens `file:`/`ssh:`/custom schemes, or URL text is shell-interpolated. Links break across wrapped rows/scrollback, consume unbounded parser memory, or make an in-app drag copy click instead of select. Native terminal clicks and baude mouse handling compete unpredictably.

**Why it happens:** OSC 8 is terminal state, not a Rust string decoration. The protocol requires `OSC 8;params;URI ST`, printable/encoded URI data, and an explicit empty-URI close. The current selection path reads vt100 grid contents (`app.rs:5357-5367`), so link metadata must remain coherent with cells and scrollback rather than be reconstructed from screen text.

**How to avoid:** Emit only validated `http`/`https` destinations, encode URI/parameter bytes, reject C0/C1 controls and unbounded labels, and always close spans. Never use `sh -c`, shell interpolation, or a label as the target; launch with an argv API after a user gesture. Bound link count/length and degrade to plain text when unsupported. Preserve link metadata through wrapping and scrollback, and define whether native terminal hyperlink handling or in-app selection owns a click; do not silently replace selection with a new terminal stack.

**Warning signs:** target parsed from rendered label; `file:`, `javascript:`, `ssh:`, or arbitrary schemes accepted; no close sequence; OSC bytes appear in copied text; one URL works only when unwrapped; mouse-up opens a link instead of copying selected text; large output grows memory without a cap.

**Validation evidence:** OSC8 vectors with spoofed labels, controls, separators, malformed ST, long labels, wrapped URLs, scrollback, and unsupported terminals; verify copied text is unchanged. Smoke native click in a supported terminal and drag selection in baude. **Phase:** Terminal links, followed by terminal smoke.

### 6. Shift+Enter negotiation leaks mode or breaks legacy keys

**What goes wrong:** A terminal that does not support the negotiated protocol receives bytes it renders or inserts; Ctrl-C stops interrupting; suspend/resume or an early panic leaves keyboard mode enabled for the shell. A universal “legacy Shift+Enter” detector misclassifies terminals and turns ordinary Enter into a newline or vice versa.

**Why it happens:** Current encoding emits `ESC[13;2u` for Shift+Enter (`baude/src/keys.rs:43-51`) but negotiation/restoration is a separate terminal lifecycle concern. Kitty's protocol requires push/pop around alternate-screen transitions, has independent main/alternate stacks, and changes Ctrl-C semantics under stronger modes.

**How to avoid:** Negotiate only after a capability/status response, retain legacy Enter and Ctrl-C unless support is confirmed, and use the narrowest supported mode. Push on the exact screen transition and pop on every exit path, including suspend, signal, panic cleanup, and failed initialization. Make fallback explicit and do not claim universal Shift+Enter detection.

**Warning signs:** raw `CSI u` is emitted unconditionally; no `CSI < u` on error; Ctrl-C tests pass only through the app handler, not the PTY; nested alternate screens restore the wrong mode; terminal state remains altered after `q`, suspend, or panic.

**Validation evidence:** protocol-capable and legacy pseudo-terminals, unsupported/no-response timeout, Ctrl-C, Enter, Shift+Enter, suspend/resume, panic/early-exit, and nested main/alternate transitions. Manual smoke on macOS and Linux supported terminals. **Phase:** Shift+Enter, followed by terminal smoke.

## Integration Gotchas

| Integration | Common mistake | Correct approach |
|---|---|---|
| Claude settings | Treat malformed input as empty | Preserve bytes; fail closed and surface diagnostic |
| OS lock | Delete a stale-looking lock file | Test/acquire the OS lock; metadata is advisory only |
| Child fixtures | Set global HOME/XDG | `Command::env` plus injected roots |
| Git cleanup | Delete a fixture when `.git` is absent | Preserve it; cleanup requires ownership and topology proof |
| OSC 8 | Open the rendered label or use a shell | Validate target and launch argv directly on gesture |
| Keyboard protocol | Assume CSI-u support | Query/negotiate, push/pop, then legacy fallback |

## Performance Traps

| Trap | Symptoms | Prevention | When it breaks |
|---|---|---|---|
| Unbounded OSC8 spans/metadata | memory growth and slow redraw | cap links and discard malformed spans | large agent transcripts |
| Rebuilding links from all scrollback each frame | redraw/input lag | attach bounded metadata during parsing | long-running sessions |
| Global env/OnceLock serialization | order-dependent or hung tests | subprocess isolation and dependency injection | parallel CI |

## UX Pitfalls

| Pitfall | User impact | Better approach |
|---|---|---|
| “Lock held by PID” presented as fact | User may kill the wrong process | label PID as diagnostic, show live-lock result |
| Hook repair silently changes settings | lost configuration | preserve file and show actionable error |
| Link click steals selection | copied transcript is wrong | preserve drag selection; native click only on a clear gesture |
| Shift+Enter works only on one terminal | newline appears unreliable | show no error, retain Enter, document supported fallback |

## “Looks Done But Isn’t” Checklist

- [ ] Hook update is idempotent across old/new executable paths and preserves mixed groups, args, invalid files, and user settings.
- [ ] Lock diagnostics never unlink or overwrite a live owner's file; child exit permits recovery.
- [ ] Fixture tests isolate HOME/XDG, workspace roots, OnceLock state, subprocesses, and cleanup ownership.
- [ ] Missing gitdir never authorizes deletion of a real directory.
- [ ] OSC 8 validates schemes and controls, closes spans, bounds memory, preserves wrapped/scrollback selection, and uses no shell.
- [ ] Keyboard mode restores on normal exit, suspend, panic, failed setup, and alternate-screen transitions; Enter/Ctrl-C remain correct in fallback.
- [ ] CI passes macOS-14 and Ubuntu-22.04 (`.github/workflows/ci.yml:9-24`) plus terminal smoke on both supported OS families.

## Pitfall-to-Phase Mapping

| Pitfall | Prevention phase | Verification |
|---|---|---|
| Hook identity/quoting and invalid settings | #70 hook registration | fixture matrix and byte-preservation tests |
| Lock authority/recovery | #71 lock diagnostics | live holder/exit/race subprocess tests |
| Fixture env and unsafe cleanup | #72 fixture isolation | concurrent child fixtures and missing-gitdir sentinel |
| OSC8 spoofing/grid/selection | terminal links | parser vectors, selection tests, native terminal smoke |
| Shift+Enter mode leakage | Shift+Enter | capable/legacy PTY tests and exit/suspend smoke |
| Cross-platform release regressions | release after test/CI/terminal smoke | existing CI matrix plus manual macOS/Linux smoke |

## Sources

- Project evidence: `baude-core/src/hook.rs:67-120,188-209`; `baude-core/src/persist.rs:319-425,450-481`; `baude-core/src/workspace.rs:199-223`; `baude-core/src/git.rs:2078-2121,2411-2435,2486-2499`; `baude/src/keys.rs:1-127`; `baude/src/app.rs:5357-5367,7476-7491`; `.github/workflows/ci.yml:9-24`.
- [OSC 8 original specification](https://gist.github.com/egmontkob/eb114294efbcd5adb1944c9f3cb5feda) — MEDIUM confidence from protocol documentation.
- [Kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/) — MEDIUM confidence; negotiation, stacks, legacy behavior.
- [Rust `std::env::set_var` safety](https://doc.rust-lang.org/std/env/fn.set_var.html) — MEDIUM confidence; process-global environment warning.
- [OSC 8 adoption matrix](https://github.com/Alhadis/OSC8-Adoption) — MEDIUM confidence; interoperability varies.

---
*Pitfalls research for: baude v2.2 Reliability and Terminal Usability*
*Researched: 2026-09-08*
