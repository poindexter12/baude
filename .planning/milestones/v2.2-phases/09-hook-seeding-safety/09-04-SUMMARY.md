---
phase: 09-hook-seeding-safety
plan: 04
subsystem: hooks
tags: [mcp-json, seed-warning, rust, serde_json, hreg-03]

requires:
  - phase: 09-hook-seeding-safety (plan 01)
    provides: SeedWarning/SeedWarningReason types, read_settings_guarded helper, prepare_cwd Vec<SeedWarning> signature, warn_seed_failure TUI helper
  - phase: 09-hook-seeding-safety (plan 02)
    provides: lock regression coverage (Wave 1 dependency)
provides:
  - Guarded seed_mcp_config returning Vec<SeedWarning> — .mcp.json unreadable/unparseable/non-object roots left byte-identical with a warning (D-03)
  - prepare_cwd collecting warnings from BOTH seeds in prompt mode
  - Seed-warning surfaces on all four spawn paths (TUI add + restart, daemon spawn + restart)
  - App-level malformed-settings integration tests under Phase-8 fixtures (D-10 app half)
  - Test-pinned raw-argv .mcp.json command field (D-09, no quoting)
affects: [phase-12 release gate, verify-work phase 09]

actuals:
  tokens: 3600
  tasks: 3
  commits: 5
plan_head_before: 4cc793c24071f7851838804976dd03dc46b48c23

tech-stack:
  added: []
  patterns:
    - "Shared guarded-read helper (hook::read_settings_guarded) reused across both seed sites for identical warning shape"
    - "bauded warning channel = prefixed eprintln (save-state precedent); TUI = warn_seed_failure/set_message"

key-files:
  created: []
  modified:
    - baude-core/src/backend/claude.rs
    - baude/src/app.rs
    - bauded/src/manager.rs

key-decisions:
  - ".mcp.json command field stays raw current_exe() argv data — Claude Code spawns stdio MCP servers directly, no shell (D-09); pinned by mcp_command_is_raw_argv_not_shell_quoted"
  - "No dedup of bauded seed warnings across the restore loop — a warning per re-spawn is the signal (RESEARCH Open Question 2 resolved: noise over hidden state)"
  - "TDD RED committed with the Vec<SeedWarning> signature scaffolded (empty-Vec stub, clobber behavior preserved) so the RED commit compiles and fails on behavior, not on types"

patterns-established:
  - "Seed warnings are fire-and-forget on every spawn path: no early return, no Result change (D-04)"

requirements-completed: [HREG-03]

coverage:
  - id: D1
    description: "Guarded .mcp.json seeding — unreadable/unparseable/non-object roots left byte-identical with a SeedWarning naming the file; write failure warns without aborting"
    requirement: HREG-03
    verification:
      - kind: unit
        ref: "baude-core/src/backend/claude.rs#mcp_guard_unparseable_left_untouched_and_warned"
        status: pass
      - kind: unit
        ref: "baude-core/src/backend/claude.rs#mcp_guard_non_object_root_left_untouched"
        status: pass
      - kind: unit
        ref: "baude-core/src/backend/claude.rs#mcp_guard_missing_file_fresh_seed"
        status: pass
    human_judgment: false
  - id: D2
    description: ".mcp.json command field remains raw unquoted argv data (D-09 — direct spawn, no shell)"
    requirement: HREG-03
    verification:
      - kind: unit
        ref: "baude-core/src/backend/claude.rs#mcp_command_is_raw_argv_not_shell_quoted"
        status: pass
    human_judgment: false
  - id: D3
    description: "All four spawn paths surface seed warnings (TUI add + restart via warn_seed_failure; bauded spawn + restart via 'seed warning:' eprintln) without blocking the spawn"
    requirement: HREG-03
    verification:
      - kind: integration
        ref: "cargo check --workspace --all-targets --locked + source assertion: all 4 be.prepare_cwd(&cwd) occurrences feed a warning consumer"
        status: pass
      - kind: unit
        ref: "bauded/src/manager.rs#manager::tests::fixture_isolation_ (smoke, 5 passed)"
        status: pass
    human_judgment: true
    rationale: "The bauded stderr surface is wired per the save-state precedent and compile-verified, but no automated test asserts the daemon's stderr output at runtime — live-daemon warning visibility is a judgment check"
  - id: D4
    description: "App-level spawn attempt over malformed settings.local.json leaves the file byte-identical and surfaces a TUI message naming it (D-10 app half)"
    requirement: HREG-03
    verification:
      - kind: integration
        ref: "baude/src/app.rs#app::tests::seed_warning_malformed_settings_survives_spawn_attempt"
        status: pass
      - kind: integration
        ref: "baude/src/app.rs#app::tests::seed_warning_non_object_settings_survives_spawn_attempt"
        status: pass
    human_judgment: false

duration: 15min
completed: 2026-09-16
status: complete
---

# Phase 9 Plan 04: MCP Guard + Warning Surfaces Summary

**.mcp.json seeding guarded via the shared read_settings_guarded helper (byte-identical on unsafe states, raw-argv command pinned by test), with seed warnings surfaced on all four spawn paths and app-level malformed-settings integration tests**

## Performance

- **Duration:** ~15 min
- **Started:** 2026-09-16T05:04:57Z
- **Completed:** 2026-09-16T05:20:00Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments
- `seed_mcp_config` now routes through `hook::read_settings_guarded`: an existing `.mcp.json` that cannot be read, parsed, or that parses to a non-object is left byte-identical with a `SeedWarning` naming the file; a post-merge write failure warns via `WriteFailed` without aborting the spawn (D-03/D-04)
- `prepare_cwd` collects warnings from both seeds in prompt mode; the `.mcp.json` command field is pinned as the exact `current_exe()` string — never quoted — by `mcp_command_is_raw_argv_not_shell_quoted` (D-09)
- The three spawn paths 09-01 left ignoring `prepare_cwd`'s return now surface warnings: TUI restart via `warn_seed_failure`, both bauded call sites via `eprintln!("seed warning: {warning}")` per the save-state precedent (D-02), with no control-flow change to any spawn function (D-04)
- `seed_warning_malformed_settings_survives_spawn_attempt` and `seed_warning_non_object_settings_survives_spawn_attempt` drive a real App standalone-spawn entry point under Phase-8 `TestRedirect` + workspace-override fixtures, asserting byte-identical settings and a TUI message naming the file (D-10 app half)
- Phase gate run early (last plan of phase): full workspace suite green under `scripts/assert-real-roots-untouched.sh` (all four real roots untouched)

## Task Commits

Each task was committed atomically:

1. **Task 1: Guard .mcp.json (TDD)** — `00b3f9c` (test, RED: 2 guard tests failing on the clobber) → `09df38e` (feat, GREEN: guarded implementation) → `e3ae8b1` (style: cargo fmt on the new tests)
2. **Task 2: Surface warnings on three remaining spawn paths** — `659ec60` (feat)
3. **Task 3: App-level malformed-settings integration tests** — `218cf34` (test)

## Files Created/Modified
- `baude-core/src/backend/claude.rs` — guarded `seed_mcp_config -> Vec<SeedWarning>`, `prepare_cwd` collecting both seeds, 4 new `mcp_guard_*`/`mcp_command_*` unit tests
- `baude/src/app.rs` — restart-path `warn_seed_failure` loop; `seed_warning_*` integration tests under Phase-8 fixtures
- `bauded/src/manager.rs` — `seed warning:` eprintln at `spawn_with_mode_internal` and `restart_with_mode`

## Decisions Made
- RED commit scaffolded the `Vec<SeedWarning>` signature with an empty-Vec stub (clobber behavior preserved) so the RED commit compiles and the guard tests fail on *behavior* (0 warnings, file clobbered) rather than on a type error — stronger RED evidence, bisectable history
- Kept `TestRedirect::with_hook_command` in the app tests per the plan even though `TestRedirect::new` already supplies the same legacy-form command — explicit over implicit
- No per-path dedup of bauded warnings across the restore loop (plan-directed; RESEARCH Open Question 2)

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
- The app tests-mod scope does not glob-import the parent's `use baude_core::backend;` binding — referenced `baude_core::backend::SpawnMode::Fresh` by full path instead (two-line fix during Task 3, no behavior impact)

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Phase 9 is complete: all four plans have summaries; HREG-03 (shared with 09-01) and HREG-04 delivered
- Phase criterion 1 fully satisfied: both `settings.local.json` and `.mcp.json` survive unsafe states untouched with actionable warnings on every spawn path of both binaries
- Ready for `/gsd-verify-work 09`, then Phase 10 (terminal links)

## Self-Check: PASSED

All 5 task commits verified in git log; all modified files present on disk; all plan verification commands green (backend::claude:: 9 passed, seed_warning_ 2 passed, fixture_isolation_ 5 passed, workspace check/fmt/clippy clean, full suite under assert-real-roots-untouched.sh PASS).

---
*Phase: 09-hook-seeding-safety*
*Completed: 2026-09-16*
