# Phase 9: Hook Seeding Safety - Research

**Researched:** 2026-09-15
**Domain:** Rust file-safety guards, POSIX shell quoting, warning propagation across a core-crate/binary seam, file-lock regression tests
**Confidence:** HIGH (codebase claims read this session; one external claim MEDIUM)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Unsafe-File Handling (HREG-03)**
- The "leave untouched + warn" path triggers on any read failure, JSON parse failure, **or a root that parses but is not an object** — the current code coerces all three to `json!({})` and overwrites (`hook.rs:270-273`), and a non-object root is user content too. A missing file remains a normal fresh seed.
- Seed functions return a structured warning value; callers surface it — the TUI in the session UI, `bauded` at warn-level logging. baude-core itself does no printing. The warning names the affected file (actionable per success criterion 1).
- The same guard covers `.mcp.json` (`seed_mcp_config` in `backend/claude.rs:110-122` repeats the clobber pattern); identical warning shape for both files.
- A write failure after a successful merge never aborts the spawn (best-effort contract preserved) but fires the same warning path naming the file.

**Command Quoting (HREG-04)**
- POSIX single-quoting of the executable path (embedded `'` escaped as `'\''`), yielding `'<path>' hook` — one rule neutralizes space, `$`, `;`, and backtick.
- Quote **always**, not conditionally — one canonical form keeps the idempotency sentinel deterministic and the recognizer simple.
- `is_seeded_hook_command` accepts **both** the new quoted form and the legacy unquoted absolute-path form, so entries seeded by older binaries are still pruned — quoting does not reintroduce the per-path accumulation HREG-01 fixed (success criterion 3).
- The bare `"baude hook"` fallback stays unquoted (names no path, carries no metacharacters, never pruned).
- Whether the `permission-mcp` command in `.mcp.json` also needs quoting depends on whether Claude Code launches MCP stdio commands through a shell — planning research must answer this; quote it only if it goes through a shell, otherwise leave as-is. **(Answered below: it does NOT go through a shell — leave `.mcp.json` as-is.)**

**Regression Coverage (criterion 4)**
- Malformed-settings coverage: unit tests on the new guard in `hook.rs` PLUS an app-level spawn test through Phase-8 fixtures proving the malformed file is byte-identical after a spawn attempt and the warning surfaced.
- Spaced-path coverage is end to end: seed with a spaced/metachar executable path, execute the seeded command line through a real `sh -c` against a stub executable, and assert exactly that executable ran — not a string-form assertion.
- The two v2.1.3 inspection-only lock behaviors become automated tests under Phase-8 isolation: (a) reopening a workspace whose lock file remains on disk after the OS lock was released succeeds; (b) `bauded` encountering a held lock refuses with the pid diagnostic and recovery guidance.
- Placement: lock tests live beside the persist lock module and in `bauded` manager tests; hook tests in `hook.rs` units plus app-level integration.

### Claude's Discretion
- Exact shape of the returned warning type (enum vs struct, one type shared by both seed sites or two).
- How the TUI presents the warning (status line, toast, session message) — follow existing TUI warning conventions.
- Whether `seed_settings` and `seed_mcp_config` share a common guarded-read helper.

### Deferred Ideas (OUT OF SCOPE)
None — discussion stayed within phase scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| HREG-03 | User retains existing settings unchanged when seeding cannot safely parse or update them; actionable warning instead of silent replacement | Clobber sites read verbatim (`hook.rs:266-277`, `backend/claude.rs:110-122`); warning-propagation seam mapped end to end: `Backend::prepare_cwd` signature (`backend/mod.rs:110`), all four call sites, TUI surface precedent (`app.rs:3594-3602` `warn_prompt_mode_without_daemon` → `self.set_message`), bauded surface precedent (`manager.rs:654/720` `eprintln!("save state: {error}")`) |
| HREG-04 (remainder) | Spaced/metachar install path invokes exactly that executable, safely and idempotently | Defect site read (`hook.rs:94` unquoted `format!`); recognizer + all consumers read (`hook.rs:111-240`); quoting algorithm specified with unquote inverse; `.mcp.json` shell question answered (direct spawn — no quoting); E2E `sh -c` stub-executable test design under Phase-8 fixtures; lock-test seams located (`persist.rs:568-609`, existing patterns `persist.rs:1633-1678`, `ManagerFixture` `manager.rs:2652`) |
</phase_requirements>

## Summary

This phase is a contained safety fix inside code that was read line-by-line this session. There are three work strands: (1) replace the swallow-and-coerce reads in `seed_settings` and `seed_mcp_config` with a guarded read that returns a structured warning instead of overwriting unreadable/unparseable/non-object user files; (2) quote the seeded hook command with POSIX single quotes and teach `is_seeded_hook_command` to unquote before its absolute-path/stem check, keeping the legacy unquoted form recognized; (3) add the four regression tests (malformed file unit + app-level, spaced-path E2E through `sh -c`, and the two v2.1.3 lock behaviors).

The one genuinely open research question from CONTEXT.md — whether `.mcp.json`'s `permission-mcp` command goes through a shell — resolves to **no**: Claude Code spawns stdio MCP servers directly as child processes (`command` + `args` argv-style, evidenced by Windows requiring an explicit `cmd /c` wrapper for `.cmd` launchers). Therefore `.mcp.json` must be left **unquoted**: its `command` field is data passed as argv[0], and inserting quote characters would break the direct spawn. Only `settings.local.json` hook commands (which ARE shell-executed — project-verified 2026-09-13) get quoting.

No new dependencies are needed. The quoting rule is locked and tiny (single-quote wrap, `'` → `'\''`); a local pure function with exhaustive unit tests is smaller than adopting a crate. All warning plumbing follows existing precedents already in the codebase.

**Primary recommendation:** Change `Backend::prepare_cwd` to return warnings (`fn prepare_cwd(&self, cwd: &Path) -> Vec<SeedWarning>`), implement one shared guarded-read helper used by both seed sites, add pure functions `quote_posix_single(&str) -> String` / an unquoting arm inside `is_seeded_hook_command`, and leave `.mcp.json` unquoted.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Guarded read + merge + warning VALUE | baude-core (`hook.rs`, `backend/claude.rs`) | — | Established pattern: "baude-core does not print; binaries own presentation" (CONTEXT.md) |
| Warning PRESENTATION (TUI) | `baude` binary (`app.rs`) | — | Precedent `app.rs:3594-3602`: `eprintln!` once-per-process + `self.set_message(MSG.into())` |
| Warning PRESENTATION (daemon) | `bauded` binary (`manager.rs`) | — | Precedent: `eprintln!` is bauded's only warning channel (`manager.rs:654`, `:720`, `main.rs:222`) — there is no `tracing`/`log` crate in bauded; "warn-level logging" in CONTEXT.md maps to `eprintln!` today |
| Hook command quoting + recognizer | baude-core (`hook.rs`) | — | Both producer (`baude_hook_command`) and consumer (`is_seeded_hook_command`) live in the same module; the quoted string is the idempotency sentinel |
| Lock regression tests | baude-core `persist.rs` tests + `bauded` manager tests | — | Locked placement decision; existing patterns at `persist.rs:1633-1678` and `ManagerFixture` (`manager.rs:2652-2715`) |

## Standard Stack

### Core

No new libraries. This phase modifies existing code using already-present dependencies (`serde_json` for values, `std::fs`, `std::process::Command` for the E2E test).

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `serde_json` | already a workspace dep | settings/mcp JSON values | already in use at every touched site [VERIFIED: baude-core/src/hook.rs:270-276 uses `serde_json::from_str::<Value>`] |
| `std` (`fs`, `process`, `os`) | stable toolchain | guarded reads, `sh -c` E2E test, file locks | zero-dep; `std::fs::TryLockError` already used [VERIFIED: baude-core/src/persist.rs:591-598, quoted below] |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| local ~10-line `quote_posix_single` fn | `shlex` crate (`try_quote`) | `shlex` appears only transitively in Cargo.lock (`Cargo.lock:2224` [VERIFIED: grep, not a direct dep of baude-core/baude/bauded — `grep shlex */Cargo.toml` returned nothing]). The locked decision specifies the exact algorithm (single-quote wrap, `'` → `'\''`), so a crate adds a supply-chain surface for zero expressiveness. Recommend the local function. |

**Installation:** none.

## Package Legitimacy Audit

No external packages are installed by this phase. **Packages removed due to [SLOP] verdict:** none. **Packages flagged [SUS]:** none. (If the planner elects `shlex` despite the recommendation above, it must run the legitimacy gate then; `shlex` is a long-established crates.io crate but was not verified this session because it is not recommended.)

## Architecture Patterns

### System Architecture Diagram

```
spawn request (TUI app.rs:2841 / :5245, daemon manager.rs:1158 / :2136)
        │
        ▼
Backend::prepare_cwd(cwd)  ──────────────  backend/mod.rs:110 (trait — signature changes)
        │                                   backend/claude.rs:74-79 (impl)
        ├──► hook::seed_settings(cwd) ─► guarded read ──ok──► merge_hook_settings ─► write
        │         │                        │ read err / parse err / non-object      │ write err
        │         │                        ▼                                        ▼
        │         └────────────── SeedWarning { file, reason } ◄────────────────────┘
        │
        └──► seed_mcp_config(cwd) [prompt mode only] ─► same guard ─► merge_mcp_config ─► write
                  │
                  └────────────── SeedWarning (identical shape)
        │
        ▼
Vec<SeedWarning> returned to caller
        ├── TUI: self.set_message(...) (+ eprintln once)   [app.rs precedent :3594-3602]
        └── bauded: eprintln!("...")                        [manager.rs precedent :654/:720]
```

### Verbatim source of truth — the two clobber sites

`seed_settings` [VERIFIED: baude-core/src/hook.rs:266-277, read this session]:

```rust
pub fn seed_settings(cwd: &std::path::Path) {
    let dir = cwd.join(".claude");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("settings.local.json");
    let existing = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .unwrap_or_else(|| json!({}));
    let command = baude_hook_command();
    let merged = merge_hook_settings(&existing, &command);
    let _ = std::fs::write(&path, merged.to_string());
}
```

`seed_mcp_config` [VERIFIED: baude-core/src/backend/claude.rs:110-122, read this session]:

```rust
fn seed_mcp_config(cwd: &Path) {
    let exe = match std::env::current_exe() {
        Ok(p) => p.display().to_string(),
        Err(_) => return, // can't resolve the bridge command — best-effort skip.
    };
    let path = crate::permission::mcp_config_path(cwd);
    let existing = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let merged = crate::permission::merge_mcp_config(&existing, &exe);
    let _ = std::fs::write(&path, merged.to_string());
}
```

The quoting defect [VERIFIED: baude-core/src/hook.rs:93-96]:

```rust
    match std::env::current_exe() {
        Ok(p) => format!("{} hook", p.display()),
        Err(_) => FALLBACK_HOOK_COMMAND.to_string(),
    }
```

The recognizer that must learn the quoted form [VERIFIED: baude-core/src/hook.rs:111-121]:

```rust
pub fn is_seeded_hook_command(command: &str) -> bool {
    let Some(path) = command.strip_suffix(" hook") else {
        return false;
    };
    let path = std::path::Path::new(path);
    path.is_absolute()
        && matches!(
            path.file_stem().and_then(|stem| stem.to_str()),
            Some("baude" | "bauded")
        )
}
```

### Pattern 1: Guarded read returning a structured warning

**What:** Replace `.ok()`-swallowing with an explicit four-way disposition. Missing file → fresh seed. Read error, parse error, or parsed-but-not-object → return warning, touch nothing. Write error after merge → return warning, spawn continues.

**When to use:** Both seed sites; the locked decision leaves "shared helper vs two copies" to discretion — a shared helper in `hook.rs` (or a small new module) is recommended since the two bodies above are near-identical, but `seed_mcp_config` additionally pre-resolves `current_exe`.

**Skeleton** (values quoted from sources above; error taxonomy is design, marked as such):

```rust
// Design sketch — reasons per locked decision (read fail / parse fail / non-object root / write fail)
pub struct SeedWarning {
    pub file: std::path::PathBuf,      // criterion 1: warning names the file
    pub reason: SeedWarningReason,
}
pub enum SeedWarningReason { Unreadable(std::io::Error), Unparseable(serde_json::Error), NonObjectRoot, WriteFailed(std::io::Error) }

fn read_settings_guarded(path: &Path) -> Result<Value, SeedWarning> {
    match std::fs::read_to_string(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})), // fresh seed
        Err(e) => Err(SeedWarning { file: path.into(), reason: SeedWarningReason::Unreadable(e) }),
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Err(e) => Err(SeedWarning { file: path.into(), reason: SeedWarningReason::Unparseable(e) }),
            Ok(v) if !v.is_object() => Err(SeedWarning { file: path.into(), reason: SeedWarningReason::NonObjectRoot }),
            Ok(v) => Ok(v),
        },
    }
}
```

Signature ripple: `seed_settings(cwd) -> Vec<SeedWarning>` (or `Option`), `seed_mcp_config` likewise, and the trait method `fn prepare_cwd(&self, cwd: &Path);` at `backend/mod.rs:110` [VERIFIED: grep + surrounding trait read] becomes `-> Vec<SeedWarning>`. Every implementor of `Backend` must be updated — the planner should `grep -n "fn prepare_cwd" baude-core/src/backend/` to enumerate implementors (only `claude.rs` was read this session; other backend files were not opened, so treat "claude is the only non-trivial implementor" as [ASSUMED]).

**Callers to update** [VERIFIED: grep this session]:
- `baude/src/app.rs:2841` and `baude/src/app.rs:5245` — TUI. Surface per precedent `warn_prompt_mode_without_daemon` at `app.rs:3594-3602` [VERIFIED: read this session]: `eprintln!` guarded by a once-flag plus `self.set_message(MSG.into())`. Seed warnings should at minimum call `self.set_message(...)` naming the file; the once-per-process `static WARNED` pattern exists if dedup is wanted.
- `bauded/src/manager.rs:1158` and `bauded/src/manager.rs:2136` — daemon. Surface per precedent: `eprintln!("save state: {error}")` at `manager.rs:654`/`:720` [VERIFIED: grep + context read]. Note: bauded has **no logging crate** (`grep tracing|log::|env_logger bauded/src/main.rs` → only `eprintln!`), so CONTEXT.md's "warn-level logging" concretely means a prefixed `eprintln!`.

### Pattern 2: Always-quote with a recognizer that unquotes

**Producer** — quote inside `baude_hook_command`'s `Ok` arm only (fallback stays bare per locked decision):

```rust
// Design sketch
fn quote_posix_single(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}
// Ok(p) => format!("{} hook", quote_posix_single(&p.display().to_string())),
```

**Recognizer** — `strip_suffix(" hook")` first (unchanged), then a two-arm path extraction: if the remainder starts AND ends with `'`, invert the quoting (`'\''` → `'`, strip outer quotes) and require that the unquoted result round-trips (`quote_posix_single(unquoted) == original`) to reject malformed quotings; otherwise treat the remainder as the legacy raw path. Then the existing `is_absolute()` + stem `baude|bauded` check applies to the extracted path.

**Downstream consumers that get quoting "for free" once the recognizer learns it** [VERIFIED: read this session]:
- `merge_hook_settings` (`hook.rs:163-197`) — pruning uses `seeded_group_command(g).is_some_and(is_seeded_hook_command)` (line 182); idempotency sentinel is exact string equality against `command` (lines 185-189), which the always-quote decision keeps deterministic.
- `is_pure_seed_group` (`hook.rs:237-240`) — `is_seeded_hook_command(command) || command == FALLBACK_HOOK_COMMAND`. **Safety-critical:** this gates worktree removal (#78: a false positive deletes a user's `.claude`). The quoted form must be recognized here too, or a freshly-seeded-then-removed worktree stops qualifying as pure seed; conversely the round-trip check above keeps user-authored look-alike quotings out.
- `FALLBACK_HOOK_COMMAND: &str = "baude hook"` (`hook.rs:71`) — unquoted, never pruned, unchanged.

**Test-override interplay** [VERIFIED: hook.rs:88-92]: `baude_hook_command` returns `crate::testing::hook_command_override()` verbatim before the `current_exe` path. Quoting inside the `Ok(current_exe)` arm means overrides bypass quoting — so Phase-8 fixtures that today supply `<absolute path>/baude hook` must either supply the pre-quoted production shape or the override plumbing must route through the same quoting fn. Recommend: keep the override verbatim (it is documented as supplying "a production-shaped" command) and update fixture strings to the new canonical quoted shape, exercising the real recognizer arm.

### Pattern 3: `.mcp.json` — leave unquoted (research answer)

`.mcp.json` registers the server as separate `command` and `args` fields [VERIFIED: baude-core/src/permission.rs:151-164, read via grep excerpt this session]:

```
/// Returns `{"mcpServers":{"baude":{"command":<exe>,"args":["permission-mcp"]}}}`.
```

Claude Code spawns stdio MCP servers **directly as child processes**, not through a shell: on Windows, `.cmd`-based launchers like `npx` fail without an explicit `cmd /c` wrapper, which is only explicable if `command` is passed to a direct process-spawn call; the resulting process tree is `claude → launcher → server` [CITED: code.claude.com/docs/en/mcp; github.com/anthropics/claude-code/issues/76306]. Confidence MEDIUM — inference from documented behavior, not an explicit official statement about shell usage. **Consequence: do NOT quote the `.mcp.json` `command` field** — it is argv data; adding literal quote characters would make the spawn look for a file whose name contains quotes. A spaced install path already works there because JSON string → argv[0] involves no word-splitting. This satisfies the locked conditional ("quote it only if it goes through a shell, otherwise leave as-is").

By contrast, `settings.local.json` hook commands ARE shell-executed — verified empirically by the project 2026-09-13 [VERIFIED: .planning/REQUIREMENTS.md:42 records the verification; also CONTEXT.md "Specific Ideas"] and consistent with official hooks documentation describing hook `command` values as shell commands [CITED: docs.claude.com Claude Code hooks reference].

### Anti-Patterns to Avoid

- **Conditional quoting (only when path contains metachars):** locked out — two canonical forms would make the idempotency sentinel nondeterministic and double the recognizer surface.
- **Quoting via double quotes or backslash escaping:** `"` still interpolates `$` and backticks; backslash escaping is character-class-dependent. Single-quote is the locked, total rule.
- **Making seeding abort the spawn on failure:** the best-effort contract is documented on both seed fns and locked; warnings inform, never block.
- **Printing from baude-core:** locked out; return values only.
- **Widening `is_seeded_hook_command` loosely (e.g. `trim_matches('\'')`):** #78's postmortem shape — a user command that merely LOOKS quoted must not be claimed as baude's. Use strict round-trip validation.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| General shell-command parsing | a shell tokenizer for the recognizer | strict inverse of your own quoter (round-trip check) | The recognizer only ever needs to accept the two forms baude itself wrote; parsing arbitrary shell is unbounded |
| JSON merging | new merge logic | existing `merge_hook_settings` / `merge_mcp_config` | Both are pure, tested, and non-clobbering already; only the READ path changes |
| Lock simulation in tests | mock filesystems | real `File` + `try_lock()` in-process | Proven pattern at `persist.rs:1633-1656`: two open file descriptions in one process DO conflict under `try_lock` [VERIFIED: test `second_workspace_owner_cannot_load_while_writer_lock_is_held` read this session] |

**Key insight:** the phase's only "library-shaped" problem (quoting) has a locked 3-line answer; everything else is rearranging error flow around existing pure functions.

## Common Pitfalls

### Pitfall 1: Recognizer accepts quoted form but `Path::new` sees quote chars
**What goes wrong:** `strip_suffix(" hook")` leaves `'/path/baude'`; `Path::new("'/path/baude'")` is not absolute (starts with `'`), so a naive change silently never recognizes the new form — reintroducing per-path accumulation (criterion 3 failure) AND breaking `is_pure_seed_group`'s removal exemption.
**How to avoid:** unquote BEFORE the `Path` checks; add unit tests asserting the quoted form of a spaced, `$`-bearing, `;`-bearing, backtick-bearing, and embedded-`'` path is recognized, and that the legacy unquoted form still is.
**Warning signs:** `merge_hook_settings` output containing two seeded groups for one event in a re-seed test.

### Pitfall 2: The `already` sentinel and pruning disagree
**What goes wrong:** pruning drops groups via `is_seeded_hook_command`, then the `already` check (`hook.rs:185-189`) compares exact strings. If an old file holds the LEGACY unquoted command for the CURRENT exe, prune removes it and the quoted form is appended — correct. But if a fixture or user file holds the quoted command while `baude_hook_command()` returns unquoted (e.g. an un-migrated override), the file flips forms each spawn.
**How to avoid:** one canonical producer (always-quote) plus migrated fixture overrides; add an idempotency test: `merge(merge(x)) == merge(x)` with the new command form over a file seeded by the old form.

### Pitfall 3: Treating `NotFound` as an unsafe-read warning
**What goes wrong:** every fresh worktree would warn on first spawn.
**How to avoid:** locked decision — `ErrorKind::NotFound` is the fresh-seed path; only other read errors warn. Unit-test both.

### Pitfall 4: `bauded` lock test asserts a message that does not exist yet
**What goes wrong:** CONTEXT says bauded "refuses with the pid diagnostic and recovery guidance", but today bauded never calls `claim_workspace_state_lock` (grep: zero hits in `bauded/src` [VERIFIED: grep this session]) — it first hits contention inside `atomic_save_current` (`persist.rs:629` `hold_state_lock(...).map_err(SaveError::not_committed)?`) and surfaces `eprintln!("save state: {error}")`. The pid diagnostic lives in `StateLockError`'s `Display` ("another baude (pid {pid}) already owns this workspace; its lock is {path}") [VERIFIED: persist.rs:492-510, quoted range read this session].
**How to avoid:** the planner must decide the assertion target: either (a) test the existing behavior — a manager mutation under a held lock returns/logs an error whose chain contains `StateLockError::Held` with `holder == Some(pid)` — or (b) first add a startup claim to bauded mirroring `baude/src/main.rs:341-360` (WLOCK-01's noted gap: "bauded still learns of contention at first save rather than at startup" [VERIFIED: REQUIREMENTS.md:46]). Option (a) matches "adding tests" scope; (b) is a behavior change the phase boundary ("no change to the WLOCK contract beyond adding tests") arguably excludes. Recommend (a), asserting on the `StateLockError::Held` variant and its `Display` (which carries pid + lock path — the recovery guidance lines live only in the TUI at `main.rs:354-358`).

### Pitfall 5: E2E test formats a hostile path the OS rejects
**What goes wrong:** a stub dir literally named with `/` or NUL can't exist; and the recognizer requires stem `baude`/`bauded`, so the stub must be named `baude`.
**How to avoid:** hostile characters go in the DIRECTORY name (e.g. ``<fixture>/sp ace$;`tick'quote/baude``), stub file named `baude`, `#!/bin/sh` script that writes a marker file, `chmod 0o755`. Include an embedded `'` in the dir name to exercise the `'\''` escape.

### Pitfall 6: Reopen-with-leftover-lock test accidentally hits the re-entrant cache
**What goes wrong:** `hold_state_lock` returns `Ok` early if `HELD_STATE_LOCKS` already has the path (`persist.rs:578-579`) — holding the first claim via `hold_state_lock` then "releasing" only the OS lock would test the cache, not the leftover-file path.
**How to avoid:** simulate the prior owner with a RAW `OpenOptions` file + `try_lock()` (the existing pattern at `persist.rs:1637-1645`), stamp a fake pid, then `drop(lock)` (releases the OS lock, leaves the file), then assert `hold_state_lock(&destination)` returns `Ok(())` and re-stamps the current pid. `release_state_lock_for_test` (`persist.rs:557-566`) exists for cleanup of the second claim.

## Code Examples

### Spaced-path E2E test design (criterion 2, locked "real `sh -c`")

```rust
// In hook.rs tests (or an app-level integration test) under Phase-8 fixtures.
// 1. Build hostile dir INSIDE the fixture root (containment holds):
//    let dir = fixture_root.join("sp ace$;`tick'quote");  // embedded ' exercises '\''
//    let stub = dir.join("baude");                        // stem must be `baude`
//    fs::write(&stub, "#!/bin/sh\nprintf ran > \"$MARKER\"\n"); chmod 755
// 2. Produce the command via the PRODUCTION formatter (not the override):
//    let cmd = /* quote_posix_single(stub) + " hook" — same fn baude_hook_command uses */;
// 3. Recognizer + purity invariants:
//    assert!(is_seeded_hook_command(&cmd));
// 4. Execute EXACTLY as Claude Code would (shell):
//    std::process::Command::new("sh").arg("-c").arg(&cmd)
//        .env("MARKER", marker_path).status();
// 5. Assert the marker exists => that exact executable ran (invocation, not string form).
```

Note: the stub receives argv `["<stub>", "hook"]`; a `#!/bin/sh` stub ignores it. Process spawning here is a plain `Command`, not the PTY path, so Phase-8's `pty::configure_test_child` policy is not in play; filesystem containment is satisfied because the stub and marker live under the fixture root.

### Malformed-settings app-level test (criterion 1)

Seed `.claude/settings.local.json` with bytes like `{not json` (and a second case: valid JSON non-object root, e.g. `[1,2]`), drive the spawn path that reaches `be.prepare_cwd(&cwd)` (`app.rs:2841`), then assert (a) file bytes identical before/after, (b) the returned/surfaced warning names the path. Existing app-level fixture patterns from Phase 8 (owner-struct fixtures holding `TestRedirect`) apply; `hook_command_override` keeps the command production-shaped.

### Existing lock-test vocabulary to reuse [VERIFIED: persist.rs:1633-1678, read this session]

- External-holder simulation: `OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&lock_path)` + `lock.try_lock().unwrap()`.
- Pid assertions: `lock_holder_pid(&lock_path)`, and holders stamp via `writeln!(lock, "{}", std::process::id())` (`persist.rs:600-606`).
- `LoadError::Locked { path, holder }` is the strict-load surface (test at `persist.rs:1650-1654`).

## State of the Art

Nothing version-sensitive. `File::try_lock` / `std::fs::TryLockError` (stabilized Rust 1.89) is already in production use here [VERIFIED: persist.rs:591-598 — `std::fs::TryLockError::WouldBlock` matched in code read this session], so the toolchain already supports everything the tests need.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `ClaudeBackend` is the only `Backend` implementor with a non-trivial `prepare_cwd`; others (if any) can return an empty warning list | Pattern 1 | Compile errors enumerate implementors anyway (trait signature change forces them); risk is planning-effort only |
| A2 | Claude Code spawns `.mcp.json` stdio commands without a shell (MEDIUM confidence — inferred from Windows `cmd /c` requirement and docs; no explicit official "no shell" statement found) | Pattern 3 | If wrong, a spaced install path breaks `permission-mcp` in prompt mode; mitigation: the locked decision already scopes this to "quote only if shell" — a follow-up test (spawn a stub via a `.mcp.json` in a live Claude session) can falsify it during UAT; core behavior (leave as-is) is also today's shipped behavior, so no regression either way |
| A3 | bauded's "warn-level logging" per CONTEXT.md is realized as prefixed `eprintln!` (no logging crate exists in bauded) | Responsibility map | If the planner instead wants a logging crate, that is new scope; recommend against |
| A4 | Official hooks docs describe hook `command` as shell-executed (training knowledge of docs.claude.com; the load-bearing fact is independently project-verified 2026-09-13 in REQUIREMENTS.md:42) | Pattern 3 | None material — the project's own empirical verification governs |

## Open Questions

1. **bauded held-lock test: assert existing first-save behavior or add a startup claim first?**
   - What we know: bauded never calls `claim_workspace_state_lock`; contention surfaces as `SaveError` wrapping `StateLockError::Held` at first save; phase boundary says "no change to the WLOCK contract beyond adding tests."
   - What's unclear: whether "refuses with the pid diagnostic and recovery guidance" (CONTEXT wording) is satisfiable by testing the `Held` Display alone (pid + lock path, no recovery sentence in bauded today).
   - Recommendation: test option (a) — assert `StateLockError::Held { holder: Some(pid) }` reaches bauded's error surface; note in the plan that a startup claim for bauded is out of scope (WLOCK-01's documented residual).

2. **Warning dedup across restore loops.** `bauded`'s `restore()` re-spawns every persisted session at startup (comment at `manager.rs:1150-1157`); a malformed settings file in one cwd would warn once per restart per session. Harmless but noisy; the planner may add a per-path once-guard mirroring `WARNED` in `app.rs:3597`. Discretion area.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `sh` (POSIX shell) | spaced-path E2E test (`sh -c`) | ✓ (macOS `/bin/sh`; present on all supported CI platforms) | — | none needed |
| Rust toolchain with `File::try_lock` | lock tests | ✓ (already compiled in production code, persist.rs:591) | workspace toolchain | — |
| `chmod`/unix permissions for stub executable | E2E test | ✓ (macOS/Linux; tests are unix-targeted like the rest of the suite) | `#[cfg(unix)]` gate the E2E test | — |

**Missing dependencies with no fallback:** none.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | built-in `cargo test` (workspace) |
| Config file | Cargo workspace manifests (no separate test config) |
| Quick run command | `cargo test -p baude-core hook::` / `cargo test -p baude-core persist::` |
| Full suite command | `cargo test --workspace` under `scripts/assert-real-roots-untouched.sh` observer (Phase-8 convention, 497-test serial run per REQUIREMENTS.md TISO-01 evidence) |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| HREG-03 | malformed/unreadable/non-object file left byte-identical + warning value returned | unit | `cargo test -p baude-core hook::` | ❌ Wave 0 (new tests in existing `hook.rs` tests mod) |
| HREG-03 | app-level: spawn attempt leaves malformed file intact, warning surfaced in TUI | integration | `cargo test -p baude app` (targeted test name TBD) | ❌ Wave 0 |
| HREG-03 | `.mcp.json` variant of the guard | unit | `cargo test -p baude-core backend::claude::` or `permission::` | ❌ Wave 0 |
| HREG-04 | quoted command recognized (new form + legacy), idempotent merge, purity predicates | unit | `cargo test -p baude-core hook::` | ❌ Wave 0 |
| HREG-04 | spaced/metachar path E2E through real `sh -c` invokes exact stub | integration (unix) | `cargo test -p baude-core hook::` (E2E test fn) | ❌ Wave 0 |
| WLOCK-04 (test debt) | reopen succeeds with leftover lock file after OS lock released | unit | `cargo test -p baude-core persist::` | ❌ Wave 0 |
| WLOCK-01/03 (test debt) | bauded surface carries `Held` + pid when lock contended | integration | `cargo test -p bauded manager::` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** targeted module run (`cargo test -p baude-core hook::` or `persist::`), plus `cargo fmt --check` and `cargo clippy` per repo convention (SHIP-01).
- **Per wave merge:** `cargo test -p baude-core && cargo test -p baude && cargo test -p bauded`.
- **Phase gate:** full workspace suite green under the real-roots observer before `/gsd-verify-work`.

### Wave 0 Gaps
- New test fns only — all live in existing test modules (`hook.rs` tests, `persist.rs` tests, `manager.rs` tests mod at :2532, app-level tests). No framework install, no new fixture types (Phase-8 `TestRedirect`/owner-struct fixtures and `ManagerFixture` cover isolation).

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | — |
| V3 Session Management | no | — |
| V4 Access Control | no | — |
| V5 Input Validation | yes | POSIX single-quote neutralization of the executable path before shell interpolation (CWE-78 OS command injection via install path); strict round-trip recognizer prevents spoofed "baude-owned" commands |
| V6 Cryptography | no | — |

### Known Threat Patterns for this change

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Shell metacharacters in `current_exe()` path executed by Claude Code's hook shell | Elevation of Privilege / Tampering | Always-quote with `'…'` + `'\''` (locked); E2E proof of exact-executable invocation |
| User config destroyed by seed (data loss) | Tampering (integrity) | Guarded read: never write over a file that could not be read+parsed to an object |
| Recognizer false positive deletes user `.claude` (#78 class) | Tampering | Strict round-trip unquote check; regression tests on `is_pure_seed_group` with user look-alike quoted commands |
| `.mcp.json` over-quoting breaks direct spawn | Denial of Service (self-inflicted) | Leave `command` field unquoted — it is argv data (research answer, A2) |

## Project Constraints (from CLAUDE.md)

No `./CLAUDE.md` or `./.claude/CLAUDE.md` exists in this repository, and no `.claude/skills/` or `.agents/skills/` directory is present [VERIFIED: ls this session]. Binding constraints instead come from REQUIREMENTS.md "Execution Constraints" [VERIFIED: REQUIREMENTS.md:94-100]: preserve malformed user settings and active-owner state; fixture isolation before broad test execution; issue drafts are design inputs, not authority to delete user configuration.

## Sources

### Primary (HIGH confidence — files read this session)
- `baude-core/src/hook.rs` (lines 60-310) — both defects, recognizer, merge, purity predicates, seed
- `baude-core/src/backend/claude.rs` (lines 55-144) — `prepare_cwd`, `seed_mcp_config`, spawn-plan tests
- `baude-core/src/persist.rs` (lines 472-631, 1630-1700) — lock machinery, error types, existing lock tests
- `baude/src/app.rs` (lines 2815-2870, 3590-3602) — TUI seed call site + warning precedent
- `baude/src/main.rs` (lines 336-360) — TUI lock refusal (pid + recovery guidance wording)
- `bauded/src/manager.rs` (grep + lines 1150-1175, 2125-2145) — daemon seed call sites, `eprintln!` convention, `ManagerFixture`
- `.planning/REQUIREMENTS.md`, `09-CONTEXT.md`, `.planning/STATE.md`

### Secondary (MEDIUM confidence)
- [CITED: code.claude.com/docs/en/mcp] + [CITED: github.com/anthropics/claude-code/issues/76306] — stdio MCP servers spawned as direct child processes (Windows `cmd /c` requirement; `claude → launcher → server` process tree), via WebSearch 2026-09-15

### Tertiary (LOW confidence)
- Training knowledge that official hooks docs describe hook commands as shell-executed (A4 — superseded by the project's own 2026-09-13 empirical verification)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new dependencies; every touched API read in source this session
- Architecture: HIGH — warning seam, call sites, and both presentation precedents located with line ranges
- Pitfalls: HIGH for codebase-derived ones; MEDIUM for the `.mcp.json` no-shell conclusion (A2)

**Research date:** 2026-09-15
**Valid until:** 2026-10-15 (stable domain; re-verify line numbers after any main-branch churn in `hook.rs`/`persist.rs`)
