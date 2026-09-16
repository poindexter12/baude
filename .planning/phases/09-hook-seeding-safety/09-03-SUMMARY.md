---
phase: 09-hook-seeding-safety
plan: 03
subsystem: hooks
tags: [posix-quoting, shell-safety, tdd, hook-seeding, cwe-78]

requires:
  - phase: 09-hook-seeding-safety (plan 01)
    provides: SeedWarning/read_settings_guarded seed guard; hook.rs seed-guard test vocabulary (seed_guard_cwd fixture)
provides:
  - "pub fn quote_posix_single: POSIX single-quote wrap with the '\\'' embedded-quote escape — the one rule producer, recognizer, and E2E share"
  - "baude_hook_command Ok arm emits the quoted canonical form '<path>' hook (always-quote, D-05/D-06)"
  - "is_seeded_hook_command two-arm recognizer: quoted form via strict round-trip unquote (before Path checks), legacy raw absolute path intact (D-07)"
  - "Convergence proof: stale quoted AND legacy unquoted installs prune to exactly one seeded group per event (HREG-01 preserved)"
  - "Purity proof: quoted seeds keep the worktree-removal exemption; malformed look-alike quotings stay refused (#78 closed both directions)"
  - "sh -c E2E: a stub named baude under a directory bearing space, $, ;, backtick, and an embedded ' is invoked exactly, via the command string read back from settings.local.json (D-11)"
affects: [09-04, worktree-removal, hook-seeding]

actuals:
  tokens: 3031
  tasks: 2
  commits: 2
plan_head_before: 924fbbc7b8d6554e65ae9e5ea902f26a77ee3fbd

tech-stack:
  added: []
  patterns:
    - "Strict round-trip validation: accept a quoted form ONLY if re-quoting the unquoted candidate reproduces the input byte-for-byte (fail-closed recognizer)"
    - "Always-quote canonical form as idempotency sentinel (never conditional quoting)"

key-files:
  created: []
  modified:
    - baude-core/src/hook.rs

key-decisions:
  - "Quoted-arm rejection does NOT fall through to the legacy arm: a string that starts and ends with a quote but fails round-trip is refused outright (#78 look-alike class)"
  - "unquote_posix_single is private; the recognizer is the only consumer — the public surface stays quote_posix_single + is_seeded_hook_command"
  - "REFACTOR commit skipped: GREEN implementation is minimal (one ~2-line producer change, one guarded arm, two ~5-line helpers); no cleanup existed"

patterns-established:
  - "Producer/recognizer round-trip agreement: the seeded string is validated by re-producing it, not by parsing it leniently"

requirements-completed: [HREG-04]

coverage:
  - id: D1
    description: "quote_posix_single wraps in single quotes and escapes embedded ' as the 4-char '\\'' sequence; unquote round-trips every hostile class and the empty string"
    requirement: HREG-04
    verification:
      - kind: unit
        ref: "baude-core/src/hook.rs#quote_posix_single_exact_output"
        status: pass
      - kind: unit
        ref: "baude-core/src/hook.rs#quote_unquote_round_trips_every_hostile_class"
        status: pass
    human_judgment: false
  - id: D2
    description: "is_seeded_hook_command recognizes the quoted form (space, $, ;, backtick, embedded ') AND the legacy unquoted form; look-alikes, relative paths, wrong stems, and the bare fallback are refused"
    requirement: HREG-04
    verification:
      - kind: unit
        ref: "baude-core/src/hook.rs#quoted_seeded_command_spaced_path_is_recognized"
        status: pass
      - kind: unit
        ref: "baude-core/src/hook.rs#quoted_seeded_command_metachar_path_is_recognized"
        status: pass
      - kind: unit
        ref: "baude-core/src/hook.rs#quoted_seeded_command_backtick_dir_is_recognized"
        status: pass
      - kind: unit
        ref: "baude-core/src/hook.rs#quoted_seeded_command_embedded_quote_is_recognized"
        status: pass
      - kind: unit
        ref: "baude-core/src/hook.rs#quoted_look_alikes_and_legacy_forms_keep_their_meaning"
        status: pass
    human_judgment: false
  - id: D3
    description: "Re-seeding over a file seeded by an older binary — quoted or unquoted — converges to exactly one seeded group per event (no per-path accumulation; merge(merge(x)) == merge(x))"
    requirement: HREG-04
    verification:
      - kind: unit
        ref: "baude-core/src/hook.rs#merge_prunes_stale_quoted_seeds_to_one_group"
        status: pass
      - kind: unit
        ref: "baude-core/src/hook.rs#merge_converges_legacy_unquoted_seed_to_the_quoted_form"
        status: pass
    human_judgment: false
  - id: D4
    description: "Settings built by merge with a quoted command satisfy is_pure_seed_settings (removal exemption kept); a malformed look-alike quoting does not (#78 stays closed)"
    requirement: HREG-04
    verification:
      - kind: unit
        ref: "baude-core/src/hook.rs#pure_seed_accepts_quoted_seeds_and_rejects_look_alikes"
        status: pass
    human_judgment: false
  - id: D5
    description: "A hostile install path (space, $, ;, backtick, embedded ' in the directory name) invokes exactly that stub executable through a real sh -c, using the command string read back from the written settings.local.json"
    requirement: HREG-04
    verification:
      - kind: e2e
        ref: "baude-core/src/hook.rs#spaced_metachar_path_seed_executes_exact_stub"
        status: pass
    human_judgment: false

duration: 8min
completed: 2026-09-16
status: complete
---

# Phase 9 Plan 03: POSIX-Quoted Hook Command Summary

**Always-quoted seeded hook command via `quote_posix_single` with a strict round-trip two-arm recognizer, proven by a real `sh -c` stub invocation on a space/`$`/`;`/backtick/embedded-quote path**

## Performance

- **Duration:** 8 min
- **Started:** 2026-09-16T04:50:50Z
- **Completed:** 2026-09-16T04:58:53Z
- **Tasks:** 2 (RED, GREEN)
- **Files modified:** 1 (baude-core/src/hook.rs)

## Accomplishments

- The live command-injection-shaped defect (unquoted `format!("{} hook", exe)` interpolated into a shell-executed string) is closed: `baude_hook_command`'s `Ok` arm now emits `'<path>' hook` via `quote_posix_single`, unconditionally.
- `is_seeded_hook_command` accepts both the quoted canonical form (strict round-trip unquote BEFORE `Path::new`, per Pitfall 1) and the legacy unquoted form, so quoting reintroduces no per-path accumulation and stale installs of either form prune to one group per event.
- `is_pure_seed_group` widened for genuine quoted seeds ONLY — a user command that merely looks quoted fails the round-trip and keeps blocking worktree removal (#78 class), with tests pinning both directions.
- The `#[cfg(unix)]` E2E writes a `#!/bin/sh` stub named `baude` under ``sp ace$;`tick'quote/``, seeds through the production path, reads the command string back from `settings.local.json`, runs it through `std::process::Command::new("sh").arg("-c")`, and asserts the marker the stub writes — invocation proof, not string-form assertion (D-11).

## TDD Cycle Evidence

**RED** (`4ffc2f5`): 9 new tests written against today's code using literal quoted strings plus a test-local escape helper (compiles clean — no missing symbols). `cargo test -p baude-core --lib hook::` exited 101 with exactly 7 assertion failures (the quoted-recognition x4, quoted-prune convergence, quoted-purity, and E2E recognizer asserts) and 0 compile errors; the negative floor (legacy recognition, fallback exemption, all look-alike rejections — 32 tests) stayed green. Evidence record validated: `RED_EVIDENCE_OK` (target `hook::tests::quoted_seeded_command_spaced_path_is_recognized`, reason `target_test_failed`).

**GREEN** (`b3d63cf`): added `pub fn quote_posix_single`, private `unquote_posix_single` (strip outer quotes, invert the escape, require `quote_posix_single(candidate) == input`), the quoted arm in `is_seeded_hook_command`, and the quoted `Ok` arm in `baude_hook_command`; added direct `quote_posix_single` unit tests (exact outputs + round-trip over every hostile class and `""`); switched the E2E override construction from the test-local helper to `quote_posix_single` and deleted the helper. All 41 `hook::` tests and 57 `git::` tests pass (the git.rs seed-exemption fixture stays on the legacy arm, un-migrated as planned); `cargo check --workspace --all-targets --locked`, `cargo fmt --check`, and `cargo clippy --workspace --all-targets` all exit 0.

**REFACTOR:** skipped — no cleanup existed (implementation is two ~5-line helpers, one guarded arm, one 2-line producer change).

## TDD Gate Compliance

| Gate | Commit | Status |
|------|--------|--------|
| RED | `4ffc2f5` `test(09-03): add failing tests for quoted hook command recognition` | intentional assertion failures, RED_EVIDENCE_OK |
| GREEN | `b3d63cf` `feat(09-03): implement quoted hook command + round-trip recognizer` | all tests pass, no regressions |
| REFACTOR | — | not needed (optional gate, no change to commit) |

## Task Commits

1. **Task 1: RED — failing tests against today's unquoted producer/recognizer** - `4ffc2f5` (test)
2. **Task 2: GREEN — quote_posix_single, quoted producer arm, round-trip recognizer** - `b3d63cf` (feat)

## Files Created/Modified

- `baude-core/src/hook.rs` - `quote_posix_single` (pub), `unquote_posix_single` (private, strict round-trip), quoted arm in `is_seeded_hook_command`, quoted `Ok` arm in `baude_hook_command`, 11 new tests (9 RED-phase + 2 GREEN-phase quote units). `FALLBACK_HOOK_COMMAND`, the test override early-return, `merge_hook_settings`, `seeded_group_command`, and `is_pure_seed_group` are byte-identical (verified by diff grep before the GREEN commit).

## Decisions Made

- Quoted-arm round-trip failure returns `false` directly rather than falling through to the legacy arm — a quote-wrapped remainder is either exactly our canonical form or not ours at all (#78 fail-closed posture).
- REFACTOR commit omitted per the reference's "only commit if changes made" rule.

## Deviations from Plan

None - plan executed exactly as written.

(Tooling note, not a plan deviation: `gsd-tools check tdd-red-evidence` parses node-test TAP output, so the RED evidence record carries a faithful TAP transcription of the real cargo run — real test names, real 32/7 pass/fail counts, real exit 101 — alongside the verbatim cargo output.)

## Planner Assumption Check

The flagged assumption (no edge classes beyond the enumerated list) held: no input class outside the plan's behavior list surfaced during execution. Non-UTF-8 paths remain lossy-converted by `current_exe().display()` exactly as before (unchanged, as the plan scoped).

## Issues Encountered

None.

## Known Stubs

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phase criteria 2 and 3 are satisfied at the unit/E2E level; plan 09-04 (warning surfaces in `baude`/`bauded` + app-level integration) is the remaining phase work.
- HREG-04 marked complete only if no sibling plan still declares it (shared-ID gate applies at requirements update).

---
*Phase: 09-hook-seeding-safety*
*Completed: 2026-09-16*

## Self-Check: PASSED
