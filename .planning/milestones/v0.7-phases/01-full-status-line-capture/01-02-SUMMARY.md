---
phase: 01-full-status-line-capture
plan: 02
subsystem: core
tags: [rust, serde_json, statusline, claude-code, json-bridge, back-compat, tdd]

# Dependency graph
requires:
  - phase: 01-01
    provides: "schema:2 bridge file with model/effort/thinking/vim_mode/pr/worktree keys (writer half)"
provides:
  - "PrInfo and WorktreeInfo reader-side sub-structs (Default, Clone)"
  - "five new additive Option fields on ClaudeMeta: effort, thinking, vim_mode, pr, worktree"
  - "extended read_bridge_file reading the new keys via Value accessors with no schema branching"
  - "explicit model precedence: bridge-wins-when-present, transcript value survives when omitted"
  - "#[cfg(test)] mod tests in meta.rs (net-new test module; baude-core meta coverage gap closed)"
affects: [01-03 ui.rs overlay, daemon/PWA Tier 2 remote parity]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "serde_json::Value accessors (never #[derive(Deserialize)]) for free additive back-compat both directions"
    - "is_object()-guarded nested sub-object read into a typed reader-side sub-struct (mirrors the window closure / RateWindow)"
    - "if let Some guard on bridge model so a None bridge value never clobbers the transcript-derived model"
    - "pid+suffix-keyed bridge_path fixture for testing a private reader that keys off session_id"

key-files:
  created: []
  modified:
    - baude-core/src/meta.rs

key-decisions:
  - "PrInfo/WorktreeInfo are Default+Clone (not Copy) — they hold Option<String>"
  - "Reader never branches on schema; type mismatches yield None via .as_*() (no unwrap on new fields)"
  - "vim_mode is read and persisted into ClaudeMeta but not rendered (locked scope, deferred to a later plan)"
  - "Tests exercise the private read_bridge_file directly via the in-module #[cfg(test)] seam — no public test helper added"

patterns-established:
  - "Pattern: test a private bridge reader by writing a real fixture at bridge_path(unique-sid), setting session_id, calling the reader, asserting, then removing the temp file"

requirements-completed: [STL-02]

# Metrics
duration: 8min
completed: 2026-06-15
---

# Phase 1 Plan 02: Full Status-Line Capture (Reader Half) Summary

**`ClaudeMeta` now reads the full schema:2 bridge payload — effort, thinking, vim_mode, pr, and worktree — as additive optional fields via `Value` accessors, with bridge-wins-when-present model precedence and back-compat proven in both directions by a net-new meta test module.**

## Performance

- **Duration:** ~8 min
- **Completed:** 2026-06-15
- **Tasks:** 1 (TDD: RED + GREEN, no REFACTOR needed)
- **Files modified:** 1

## Accomplishments
- Added reader-side sub-structs `PrInfo { number, url, review_state }` and `WorktreeInfo { name, path, branch }`, both `#[derive(Default, Clone)]`, modeled on the existing `RateWindow`.
- Added five additive `Option` fields to `ClaudeMeta`: `effort`, `thinking`, `vim_mode`, `pr`, `worktree`.
- Extended `read_bridge_file` to read all five via `Value` accessors after the existing rate-window reads, plus explicit model precedence (`if let Some(m) = v["model"].as_str()`), with nested `pr`/`worktree` populated under an `is_object()` guard mirroring the `window` closure.
- Added a net-new `#[cfg(test)] mod tests` (6 tests) covering: v2 round-trip (all new + legacy fields), legacy/schema-absent file (new fields None), schema:99 (no schema branching), pr absent, pr-present-but-review_state-absent, and model precedence (bridge wins, then transcript value survives when bridge omits model).
- All three CI gates green for baude-core: `cargo test -p baude-core` (15 passed), `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`.

## Task Commits

Each step committed atomically (TDD gate sequence):

1. **Task 1 (RED): failing tests for additive ClaudeMeta bridge reads** — `cc9aeb2` (test)
2. **Task 1 (GREEN): grow ClaudeMeta with effort/thinking/vim/pr/worktree from bridge** — `2c902dd` (feat)

REFACTOR not required — clippy and fmt were clean immediately after GREEN.

**Plan metadata:** committed separately with this SUMMARY, STATE.md, ROADMAP.md.

## Files Created/Modified
- `baude-core/src/meta.rs` — added `PrInfo`/`WorktreeInfo` sub-structs; added `effort`/`thinking`/`vim_mode`/`pr`/`worktree` fields to `ClaudeMeta`; extended `read_bridge_file` with the additive reads + model precedence; added `#[cfg(test)] mod tests`. The `read_bridge_file` poll order, `read_json` seam, and all pre-existing logic left unchanged.

## Decisions Made
- **No `#[derive(Deserialize)]`, no `schema` branching:** kept the untyped `Value`-accessor pattern. An older reader still parses a schema:2 file (it ignores the new keys), and the new reader still parses a legacy/schema-absent file (new fields stay `None`) — the `does_not_branch_on_schema` and `reads_legacy_bridge` tests pin both directions.
- **Model precedence is guarded:** `read_bridge_file` runs after `read_transcript_tail` in `poll()`. The `if let Some` guard means the bridge wins when it carries `model` but the transcript value survives when the bridge omits it (the `model_bridge_wins_then_survives` test pins both halves).
- **vim_mode persisted, not rendered:** captured into `ClaudeMeta.vim_mode` per the locked scope, with no UI change in this plan.
- **Private reader tested via in-module seam:** tests live in the same module so they can call the private `read_bridge_file` directly; each writes a real fixture at `bridge_path(unique-sid)` (pid+suffix-keyed to avoid collisions) and removes it afterward.

## Deviations from Plan
None — plan executed exactly as written. The `pr`/`worktree` reads use inline `if v["..."].is_object()` blocks rather than a shared closure (the two sub-structs have different field sets), which matches the plan's "mirroring the `window` closure" intent while keeping each read self-documenting.

## Threat Model Compliance
- **T-01-04 (Tampering — malformed/wrong-type fields):** mitigated. All new reads use `.as_str()` / `.as_bool()` / `.as_u64()` / `is_object()`, which return `None` on type mismatch — the field stays `None`, no panic. No `unwrap()` on any new field. `does_not_branch_on_schema` and `pr_absent_is_none` exercise odd/absent shapes.
- **T-01-SC (dependency tampering):** N/A — no `Cargo.toml` change, zero new dependencies.

## Issues Encountered
None. RED failed to compile as expected (`no field effort/thinking/vim_mode/pr/worktree on ClaudeMeta`, missing sub-structs). GREEN passed all 6 new meta tests (15 total in baude-core) on first run; clippy and fmt were clean with no follow-up edits.

## User Setup Required
None — no external service configuration, no new dependencies.

## Next Phase Readiness
- `ClaudeMeta` now carries `model`/`effort`/`thinking`/`vim_mode`/`pr`/`worktree` from the schema:2 bridge file.
- **Ready for Plan 01-03** (`ui.rs` overlay): the effort/thinking/pr rows can now read these fields off `ClaudeMeta` (note: `vim_mode` is captured but intentionally not rendered).
- No blockers.

## Self-Check: PASSED

- FOUND: baude-core/src/meta.rs (PrInfo, WorktreeInfo, new fields, read_bridge_file extension, mod tests present)
- FOUND commit cc9aeb2 (RED test)
- FOUND commit 2c902dd (GREEN feat)
- baude-core test suite: 15 passed, 0 failed
- clippy --all-targets -D warnings: clean
- cargo fmt --check: clean

## TDD Gate Compliance
- RED gate: `cc9aeb2` `test(01-02): ...` — failing tests committed first.
- GREEN gate: `2c902dd` `feat(01-02): ...` — implementation committed after, all tests passing.
- REFACTOR gate: not required (clippy/fmt clean post-GREEN).

---
*Phase: 01-full-status-line-capture*
*Completed: 2026-06-15*
