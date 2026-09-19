# Phase 9: Hook Seeding Safety - Context

**Gathered:** 2026-09-15
**Status:** Ready for planning

<domain>
## Phase Boundary

Seeding a project's hooks never destroys a user's existing settings and never
emits a command string the shell will mis-execute.

In scope: the HREG-03 unsafe-file guard for `.claude/settings.local.json` and
`.mcp.json` (leave untouched + actionable warning instead of overwriting with
the seed alone), the HREG-04 remainder (quoting the seeded hook command so a
path bearing a space, `$`, `;`, or backtick invokes exactly that executable),
a recognizer that matches the quoted form without reintroducing HREG-01's
per-path accumulation, and regression tests covering the malformed-settings
file, a spaced install path end to end, and the two lock behaviors verified
only by inspection in v2.1.3.

Out of scope: terminal link/input work (Phases 10-11), the release gate
(Phase 12), and any change to the already-delivered HREG-01/HREG-02 pruning
semantics or WLOCK-01..04 lock contract beyond adding tests.

</domain>

<decisions>
## Implementation Decisions

### Unsafe-File Handling (HREG-03)
- The "leave untouched + warn" path triggers on any read failure, JSON parse
  failure, **or a root that parses but is not an object** — the current code
  coerces all three to `json!({})` and overwrites (`hook.rs:270-273`), and a
  non-object root is user content too. A missing file remains a normal fresh
  seed.
- Seed functions return a structured warning value; callers surface it — the
  TUI in the session UI, `bauded` at warn-level logging. baude-core itself
  does no printing. The warning names the affected file (actionable per
  success criterion 1).
- The same guard covers `.mcp.json` (`seed_mcp_config` in
  `backend/claude.rs:110-122` repeats the clobber pattern); identical warning
  shape for both files.
- A write failure after a successful merge never aborts the spawn (best-effort
  contract preserved) but fires the same warning path naming the file.

### Command Quoting (HREG-04)
- POSIX single-quoting of the executable path (embedded `'` escaped as
  `'\''`), yielding `'<path>' hook` — one rule neutralizes space, `$`, `;`,
  and backtick.
- Quote **always**, not conditionally — one canonical form keeps the
  idempotency sentinel deterministic and the recognizer simple.
- `is_seeded_hook_command` accepts **both** the new quoted form and the legacy
  unquoted absolute-path form, so entries seeded by older binaries are still
  pruned — quoting does not reintroduce the per-path accumulation HREG-01
  fixed (success criterion 3).
- The bare `"baude hook"` fallback stays unquoted (names no path, carries no
  metacharacters, never pruned).
- Whether the `permission-mcp` command in `.mcp.json` also needs quoting
  depends on whether Claude Code launches MCP stdio commands through a shell —
  planning research must answer this; quote it only if it goes through a
  shell, otherwise leave as-is.

### Regression Coverage (criterion 4)
- Malformed-settings coverage: unit tests on the new guard in `hook.rs` PLUS
  an app-level spawn test through Phase-8 fixtures proving the malformed file
  is byte-identical after a spawn attempt and the warning surfaced.
- Spaced-path coverage is end to end: seed with a spaced/metachar executable
  path, execute the seeded command line through a real `sh -c` against a stub
  executable, and assert exactly that executable ran — not a string-form
  assertion.
- The two v2.1.3 inspection-only lock behaviors become automated tests under
  Phase-8 isolation: (a) reopening a workspace whose lock file remains on disk
  after the OS lock was released succeeds; (b) `bauded` encountering a held
  lock refuses with the pid diagnostic and recovery guidance.
- Placement: lock tests live beside the persist lock module and in `bauded`
  manager tests; hook tests in `hook.rs` units plus app-level integration.

### Claude's Discretion
- Exact shape of the returned warning type (enum vs struct, one type shared by
  both seed sites or two).
- How the TUI presents the warning (status line, toast, session message) —
  follow existing TUI warning conventions.
- Whether `seed_settings` and `seed_mcp_config` share a common guarded-read
  helper.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `seed_settings` (`baude-core/src/hook.rs:266-277`) — the clobber site: reads,
  `.ok()`-swallows, coerces to `{}`, `let _ =` writes.
- `baude_hook_command` (`hook.rs:88-97`) — unquoted `format!("{} hook", ...)`;
  the live defect. Test override via `crate::testing::hook_command_override()`.
- `is_seeded_hook_command` (`hook.rs:111-121`) — strips ` hook` suffix, checks
  absolute path with `baude`/`bauded` stem; must learn the quoted form.
- `seeded_group_command` / `merge_hook_settings` / `is_pure_seed_settings`
  (`hook.rs:129-240`) — pruning + purity predicates that consume the
  recognizer; `is_pure_seed_group` gates worktree removal (a false positive
  deletes a user's `.claude`).
- `seed_mcp_config` (`baude-core/src/backend/claude.rs:110-122`) — same
  swallow-and-coerce pattern for `.mcp.json`; calls
  `permission::merge_mcp_config`.
- Phase-8 fixture infrastructure: `TestRedirect` unified guard, owner-struct
  fixtures, `BAUDE_TEST_FIXTURE_ROOT` containment assertions — regression
  tests must run under it.
- Workspace lock: `try_lock`-based contention (WLOCK-01..04, shipped v2.1.3)
  in persist; `release_state_lock_for_test` escape hatch.

### Established Patterns
- Best-effort seeding contract: a seed failure must never abort a spawn
  (documented on `seed_settings` and `seed_mcp_config`).
- Test isolation is thread-local RAII redirect, never env mutation (Phase 8;
  commit 725c558 rationale).
- baude-core does not print; binaries own presentation (bauded logs, TUI
  renders).
- Recognizer changes are safety-critical: `is_pure_seed_group` governs
  worktree-removal exemption (#78 postmortem).

### Integration Points
- `ClaudeBackend::prepare_cwd` (`backend/claude.rs:74-79`) — the single seam
  both seeds run through on every spawn (TUI and daemon).
- TUI warning surface: wherever `App` already renders spawn-path notices.
- `bauded` logging for daemon-side warnings.

</code_context>

<specifics>
## Specific Ideas

- Success criterion 2's verification note is load-bearing: hook commands ARE
  executed through a shell (verified 2026-09-13), so quoting is a correctness
  fix, not hardening.
- The spaced-path E2E must prove invocation, not formatting: a stub executable
  at a hostile path actually runs when the seeded string passes through
  `sh -c`.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>
