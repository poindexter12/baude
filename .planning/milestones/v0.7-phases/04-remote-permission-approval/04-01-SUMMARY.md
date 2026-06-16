---
phase: 04-remote-permission-approval
plan: 01
subsystem: infra
tags: [permission-mode, mcp, claude-cli, spawn, security, rust]

# Dependency graph
requires:
  - phase: 02-hook-driven-status
    provides: "seed_settings/merge_hook_settings non-clobbering idempotent seed pattern + current_exe() command resolution (mirrored for the .mcp.json seed)"
provides:
  - "BAUDE_PERMISSION_MODE = skip | prompt spawn-flag selection (default skip), the PERM-01 security-critical gate"
  - "baude_core::permission pure module: permission_flag / permission_flag_for / is_prompt_mode / mcp_server_config / merge_mcp_config / mcp_config_path"
  - "prompt-mode .mcp.json seeding (non-clobbering) registering the permission-mcp stdio server at both spawn sites"
affects:
  - "04-02 (permission-mcp subcommand + daemon pending state + GET/POST /permission — consumes the seeded registration)"
  - "04-03 (waiting_reason + notified_permission push)"
  - "04-04 (PWA approve/deny card)"

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "env-read/pure split: permission_flag (reads env) delegates to permission_flag_for (pure, testable, race-free) — mirrors hook.rs core/binary split"
    - "pure non-clobbering merge (merge_mcp_config) in core; filesystem read/write in the binaries (mirrors merge_hook_settings vs seed_settings)"

key-files:
  created:
    - baude-core/src/permission.rs
  modified:
    - baude-core/src/lib.rs
    - bauded/src/manager.rs
    - baude/src/app.rs
    - bauded/src/api.rs

key-decisions:
  - "permission_flag defaults to --dangerously-skip-permissions; prompt reachable ONLY by the exact literal \"prompt\" (unset / unrecognized / case-mismatch all fail safe to skip) — PERM-01 / T-04-01"
  - "Added an env-free permission_flag_for(mode, base_cmd) seam so branch tests never mutate the process-global BAUDE_PERMISSION_MODE (which races concurrent PTY-spawn tests that read it at spawn time)"
  - "Permission flag is appended to the base cmd unconditionally (mode-driven, not command-sniffing) BEFORE the export/exec wrap, so it survives the --continue || exec resume fallback (WR-01)"
  - "merge_mcp_config lives in core (pure) and is shared by both spawn sites' seed_mcp_config; only mcpServers.baude is set, sibling servers + other top-level keys survive (T-04-03)"

patterns-established:
  - "Pattern: pure mode-selector + env-reading wrapper, so the security-critical default is unit-tested without env races"
  - "Pattern: prompt-mode-only .mcp.json seed, best-effort + idempotent + re-seeded on restore (the seed_settings posture)"

requirements-completed: [PERM-01]

# Metrics
duration: 10min
completed: 2026-06-15
---

# Phase 4 Plan 1: BAUDE_PERMISSION_MODE Spawn-Flag Selection Summary

**Per-deploy `BAUDE_PERMISSION_MODE = skip | prompt` (default `skip`) selects exactly one permission flag for the spawned `claude` command at both spawn sites, with prompt mode additionally seeding a non-clobbering `.mcp.json` registering the `permission-mcp` stdio server — the PERM-01 security-critical default-stays-skip gate.**

## Performance

- **Duration:** 10 min
- **Started:** 2026-06-15T23:41:46Z
- **Completed:** 2026-06-15T23:51:48Z
- **Tasks:** 2 completed
- **Files modified:** 5 (1 created, 4 modified)

## Accomplishments
- New pure, unit-tested `baude-core::permission` module holds the security-critical mode selection + flag/`.mcp.json` logic (no HTTP, no spawn, no fs writes — the hook.rs posture).
- `permission_flag` fail-safe default: unset / `"skip"` / any unrecognized value / case-mismatch all return `--dangerously-skip-permissions`; only the exact literal `"prompt"` returns `--permission-prompt-tool mcp__baude__approve`. The two flags are never both appended, and an operator-set permission flag is never doubled (no-double-add).
- Both spawn sites (daemon `bauded/src/manager.rs::spawn`, TUI `baude/src/app.rs` add_session) append the selected flag to the base cmd and, in prompt mode only, seed a non-clobbering `.mcp.json` whose command is `current_exe() + " permission-mcp"`.

## Task Commits

1. **Task 1 (RED): failing tests for permission_flag + mcp config** - `9a28b57` (test)
2. **Task 1 (GREEN): implement permission_flag selector + .mcp.json builder** - `3c1265a` (feat)
3. **Task 2: wire permission_flag + .mcp.json seeding into both spawn sites** - `6ad8856` (feat)

_Task 1 was TDD (RED `test()` → GREEN `feat()`); no refactor commit needed. Task 2 folded the supporting core additions (`permission_flag_for` seam, `merge_mcp_config`) into its feat commit since they exist to serve the wiring._

## Files Created/Modified
- `baude-core/src/permission.rs` (created) — pure module: `permission_flag` (env wrapper), `permission_flag_for` (env-free pure selector), `is_prompt_mode`, `mcp_server_config`, `merge_mcp_config` (non-clobbering), `mcp_config_path`, plus a full `#[cfg(test)]` suite.
- `baude-core/src/lib.rs` (modified) — registered `pub mod permission;`.
- `bauded/src/manager.rs` (modified) — daemon spawn appends `permission_flag` to base cmd before the export/exec wrap; new `seed_mcp_config(cwd)` (prompt mode only); tests pin default-skip/prompt selection at the spawn-command level + non-clobbering/idempotent seed.
- `baude/src/app.rs` (modified) — TUI spawn appends `permission_flag` at `claude_cmd`; new `seed_mcp_config(cwd)` (prompt mode only).
- `bauded/src/api.rs` (modified) — wrapped the websocket PTY fixture in `sh -c 'exec bash …'` so the appended permission flag is absorbed harmlessly (see Deviations).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Bash PTY test fixtures broke when the spawn site appends the permission flag**
- **Found during:** Task 2 (wave gate `cargo test --workspace`).
- **Issue:** The spawn sites now unconditionally append the mode-selected permission flag to the base command. In production the base command is `claude` (which accepts the flag), but two integration fixtures use `bash --norc -i` as a PTY stand-in — and `bash --norc -i --dangerously-skip-permissions` makes bash treat the flag as a script-file argument and exit immediately (`keys_drive_a_shell_and_screen_reads_back`, `pty_websocket_round_trip` failed with "claude has exited" / "snapshot must be binary").
- **Fix:** Wrapped both bash fixtures as `sh -c 'exec bash --norc -i'`; the appended flag lands as the harmless `$0` of the `sh -c` script while bash still runs interactively. Production behavior is unchanged.
- **Files modified:** `bauded/src/manager.rs`, `bauded/src/api.rs`.
- **Commit:** `6ad8856`.

**2. [Rule 3 - Blocking] Env-mutating spawn-flag test raced concurrent PTY spawns**
- **Found during:** Task 2 (initial full-suite run had intermittent, order-dependent failures even before the fixture fix surfaced).
- **Issue:** `BAUDE_PERMISSION_MODE` is process-global and read at spawn time. A test that `set_var`'d it to `"prompt"`/`"bogus"` to assert branch behavior would, while set, race other tests in the same `bauded` process that spawn real PTYs (which then inherited a wrong/breaking flag).
- **Fix:** Added a pure `permission_flag_for(mode, base_cmd)` seam; the env-reading `permission_flag` delegates to it. All branch-coverage tests (core + the bauded spawn-command test) now exercise `permission_flag_for` with explicit modes and never mutate the global env. Only two minimal, mutex-guarded smoke tests still touch the env (`is_prompt_mode`, the wrapper-delegation check), both in the `baude-core` process which has no PTY-spawn tests.
- **Files modified:** `baude-core/src/permission.rs`, `bauded/src/manager.rs`.
- **Commit:** `6ad8856`.

## Threat Model Compliance
- **T-04-01 (EoP — default skip):** `permission_flag_for` defaults to skip; `prompt` reachable only by the exact literal. Pinned by `permission_flag_for_mode_selection` (unset → skip, unrecognized → skip, case-mismatch → skip) and the bauded spawn-command default-skip test. SECURITY-CRITICAL.
- **T-04-02 (Tampering — no-double-add):** `permission_flag_for` scans `base_cmd` for an existing permission flag and returns `""`; pinned by `permission_flag_for_no_double_add` and `permission_flag_for_returns_exactly_one_known_value` (never both flags).
- **T-04-03 (Tampering — .mcp.json seed):** `merge_mcp_config` sets only `mcpServers.baude`, preserving sibling servers + other keys; idempotent; best-effort (never aborts a spawn). Pinned by `merge_mcp_config_preserves_siblings_and_is_idempotent` + `seed_mcp_config_is_non_clobbering`.

## Known Stubs
- The seeded `.mcp.json` registers `command = current_exe() + " permission-mcp"`; the `permission-mcp` subcommand arm is implemented in **04-02** (Pitfall 2: it must be added to BOTH binaries). This plan intentionally only writes the registration — the forward dependency is documented in PLAN 04-01's `<action>` and the seed function doc-comments. PERM-01 (flag selection) is fully functional today regardless.

## Verification
- `cargo test -p baude-core permission::` — 10 passed (flag selection branches, no-double-add, mutual exclusion, is_prompt_mode, mcp_server_config, merge_mcp_config non-clobbering/idempotent/never-panics, mcp_config_path).
- `cargo test -p bauded` — spawn-command default-skip + prompt selection + non-clobbering `.mcp.json` seed green.
- `cargo build -p baude` — clean.
- Full CI triad green: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (run twice for race confirmation; stable).

## Self-Check: PASSED
- FOUND: `baude-core/src/permission.rs`
- FOUND: `.planning/phases/04-remote-permission-approval/04-01-SUMMARY.md`
- FOUND commits: `9a28b57` (test), `3c1265a` (feat), `6ad8856` (feat)
