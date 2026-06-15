---
phase: 01-full-status-line-capture
plan: 01
subsystem: cli
tags: [rust, serde_json, statusline, claude-code, json-bridge, tdd]

# Dependency graph
requires:
  - phase: pre-existing baude-core/src/bridge.rs
    provides: window() snake/camel helper, json! best-effort writer, --wrap delegation
provides:
  - "fn build_bridge(v: &Value) -> Value — testable bridge-JSON factory factored out of run()"
  - "schema:2 stamp on the bridge file (STL-02 writer half)"
  - "six new captured fields: model, effort, thinking, pr (number/url/review_state), worktree (name/path/branch), vim_mode"
  - "#[cfg(test)] mod tests in bridge.rs (net-new test module; baude-core Wave 0 gap closed for bridge)"
affects: [01-02 meta.rs reader, 01-03 ui.rs overlay, daemon/PWA Tier 2 remote parity]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Factor json! writer into a free fn for unit-testability without stdin"
    - "serde_json::Value accessors (never #[derive(Deserialize)]) for free back-compat"
    - "is_object()-guard -> Value::Null fallback for nested sub-objects (mirrors window())"
    - "snake-first / camel-fallback .or_else() per drift-prone leaf"

key-files:
  created: []
  modified:
    - baude-core/src/bridge.rs

key-decisions:
  - "build_bridge sets session_id from v[\"session_id\"].as_str() so the fn is total (no panic on minimal/empty payload); run() still gates the disk write on a present sid"
  - "vim_mode is captured and persisted but never rendered (locked scope: capture-but-don't-render)"
  - "No #[derive(Deserialize)] and no branching on schema — Value-accessor tolerance is the STL-02 back-compat guarantee"

patterns-established:
  - "Pattern: testable JSON-writer factory (build_bridge) returning Value, asserted with inline raw-string fixtures"
  - "Pattern: nested-object capture via is_object() guard + json! sub-object, Value::Null when absent"

requirements-completed: [STL-01, STL-02]

# Metrics
duration: 12min
completed: 2026-06-15
---

# Phase 1 Plan 01: Full Status-Line Capture Summary

**`baude statusline` now captures the full useful Claude Code payload — model, effort, thinking, pr, worktree, vim_mode — alongside the existing four fields, stamps the bridge JSON with `schema: 2`, and proves it with a net-new `build_bridge` unit-test module.**

## Performance

- **Duration:** ~12 min
- **Started:** 2026-06-15T18:14:43Z
- **Completed:** 2026-06-15
- **Tasks:** 1 (TDD: RED + GREEN, no REFACTOR needed)
- **Files modified:** 1

## Accomplishments
- Factored the bridge-JSON construction out of `run()` into a free, stdin-free, unit-testable `fn build_bridge(v: &Value) -> Value`.
- Added `schema: 2` (STL-02 writer half) plus the six new captured fields with the exact nested-object reads the research/pattern map specified.
- Added a `#[cfg(test)] mod tests` (7 tests) covering schema, full payload, model id-fallback, minimal payload (no panic), snake/camel tolerance, nested-not-scalar reads, and empty-object safety.
- All three CI gates green for baude-core: `cargo test -p baude-core`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`.

## Task Commits

Each step was committed atomically (TDD gate sequence):

1. **Task 1 (RED): failing tests for full status-line capture** - `43083fd` (test)
2. **Task 1 (GREEN): capture full status-line payload with schema:2** - `b68148f` (feat)

REFACTOR was not required — clippy/fmt were clean after GREEN (the one rustfmt reflow was folded into the GREEN commit before committing).

**Plan metadata:** committed separately with this SUMMARY, STATE.md, ROADMAP.md.

## Files Created/Modified
- `baude-core/src/bridge.rs` - Added `build_bridge(v: &Value) -> Value`; `run()` now writes `build_bridge(&v).to_string()` inside its existing best-effort block; added `schema:2` + `model`/`effort`/`thinking`/`pr`/`worktree`/`vim_mode`; added `#[cfg(test)] mod tests`. `window()` left unchanged.

## Decisions Made
- **`session_id` read inside `build_bridge`:** the fn pulls `v["session_id"].as_str()` itself so it is total and never panics on a minimal/empty payload (the `minimal_payload_ok` and `never_panics_on_empty_object` tests rely on this). `run()` still gates the actual disk write on `if let Some(sid)`, so the on-disk behavior is identical to before.
- **vim_mode persisted, not rendered:** captured per STL-01 but deliberately not surfaced anywhere (rendering is a later plan's concern).
- **No `#[derive(Deserialize)]`, no `schema` branching:** kept the untyped `Value`-accessor pattern, which is what makes STL-02 back-compat free.

## Deviations from Plan

None - plan executed exactly as written. (The plan's `build_bridge` signature kept `session_id` as a parameterless read within the fn rather than threading `sid` through — this matches the plan's `build_bridge(v: &Value) -> Value` contract verbatim and the research target shape.)

## Issues Encountered
None. RED failed to compile as expected (`no build_bridge in bridge`), GREEN passed all 7 tests on first run, and rustfmt requested one reflow on a test assertion which was applied and re-verified clean.

## User Setup Required
None - no external service configuration required. No `Cargo.toml` changes; no new dependencies.

## Next Phase Readiness
- The bridge file is now the authoritative source of `model`/`effort`/`thinking`/`pr`/`worktree`/`vim_mode` plus `schema:2`.
- **Ready for Plan 01-02** (`meta.rs` reader): it can now read these keys via the same `Value`-accessor pattern; the `PrInfo`/`WorktreeInfo` reader-side sub-structs and `ClaudeMeta` fields are the next contract.
- **Ready for Plan 01-03** (`ui.rs` overlay): effort/thinking/pr rows depend on the `meta.rs` reader from 01-02.
- No blockers.

## Self-Check: PASSED

- FOUND: baude-core/src/bridge.rs (build_bridge + mod tests present)
- FOUND commit 43083fd (RED test)
- FOUND commit b68148f (GREEN feat)
- baude-core test suite: 9 passed, 0 failed
- clippy --all-targets -D warnings: clean
- cargo fmt --check: clean

---
*Phase: 01-full-status-line-capture*
*Completed: 2026-06-15*
