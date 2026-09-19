---
phase: 09-hook-seeding-safety
plan: 01
subsystem: hooks
tags: [rust, serde_json, hook-seeding, hreg-03, seed-warning]

requires:
  - phase: 08-test-isolation
    provides: TestRedirect fixture isolation and hook_command_override conventions the seed tests build on
provides:
  - SeedWarning/SeedWarningReason value types with Display naming the affected file (baude-core/src/hook.rs)
  - read_settings_guarded pub(crate) four-way disposition helper (shared with seed_mcp_config in 09-04)
  - seed_settings -> Vec<SeedWarning>; refuses to overwrite unreadable/unparseable/non-object settings files
  - Backend::prepare_cwd -> Vec<SeedWarning> trait ripple (claude.rs real, opencode.rs Vec::new())
  - App::warn_seed_failure TUI surface wired at the add-session spawn path (app.rs:2841)
  - seed_guard_* unit-test disposition matrix (Unreadable, Unparseable, NonObjectRoot, WriteFailed, fresh seed, repeat-refusal byte stability)
affects: [09-02, 09-03, 09-04, hook-seeding, backend-trait]

actuals:
  tokens: 5096
  tasks: 2
  commits: 3
plan_head_before: 08f6bb731faf4bebb37fda2727d3141467a81dff

tech-stack:
  added: []
  patterns:
    - "Seed warnings are return values crossing the crate seam; binaries own presentation (D-02)"
    - "Guarded settings read: NotFound = fresh seed; every other read/parse/shape failure = refuse + warn, never overwrite (D-01)"

key-files:
  created: []
  modified:
    - baude-core/src/hook.rs
    - baude-core/src/backend/mod.rs
    - baude-core/src/backend/claude.rs
    - baude-core/src/backend/opencode.rs
    - baude/src/app.rs

key-decisions:
  - "read_settings_guarded is pub(crate) — one shared helper for both seed sites (CONTEXT discretion resolved: shared)"
  - "seed_settings NOT #[must_use]: the test-only caller at git.rs:3922 legitimately ignores the return"
  - "warn_seed_failure: set_message every time, stderr eprintln once per process via a shared AtomicBool (stderr dedup only)"
  - "Write-failure test trips on a 0o444 file, not the plan's 0o555 dir alone — dir write perm does not block truncating an existing entry on unix"

patterns-established:
  - "SeedWarning shape: { file: PathBuf, reason: SeedWarningReason } — 09-04 reuses it verbatim for .mcp.json"
  - "Refusal tests assert byte-identity via std::fs::read Vec<u8>, never JSON round-trip"

requirements-completed: [HREG-03]

coverage:
  - id: D1
    description: "Unsafe settings.local.json (unreadable/unparseable/non-object) survives seed_settings byte-identical with a structured warning (D-01)"
    requirement: HREG-03
    verification:
      - kind: unit
        ref: "baude-core/src/hook.rs#seed_guard_unparseable_file_left_untouched_and_warned"
        status: pass
      - kind: unit
        ref: "baude-core/src/hook.rs#seed_guard_unreadable_path_left_untouched"
        status: pass
      - kind: unit
        ref: "baude-core/src/hook.rs#seed_guard_non_object_root_left_untouched"
        status: pass
    human_judgment: false
  - id: D2
    description: "Missing settings file still gets a fresh seed with no warning; re-seed is idempotent and warning-free"
    requirement: HREG-03
    verification:
      - kind: unit
        ref: "baude-core/src/hook.rs#seed_guard_missing_file_fresh_seed_no_warning"
        status: pass
      - kind: unit
        ref: "baude-core/src/hook.rs#seed_settings_writes_idempotent_merge"
        status: pass
    human_judgment: false
  - id: D3
    description: "Write failure after a successful merge warns (WriteFailed) without aborting; refused files stay byte-stable across repeated attempts (D-04)"
    requirement: HREG-03
    verification:
      - kind: unit
        ref: "baude-core/src/hook.rs#seed_guard_write_failure_warns_and_preserves_original"
        status: pass
      - kind: unit
        ref: "baude-core/src/hook.rs#seed_guard_refusal_repeats_byte_stable"
        status: pass
    human_judgment: false
  - id: D4
    description: "Warning value crosses the crate seam: Backend::prepare_cwd -> Vec<SeedWarning>, whole workspace compiles against the new trait"
    requirement: HREG-03
    verification:
      - kind: other
        ref: "cargo check --workspace --all-targets --locked"
        status: pass
    human_judgment: false
  - id: D5
    description: "TUI operator sees a warning naming the affected file at the add-session spawn path (warn_seed_failure -> set_message; D-02)"
    requirement: HREG-03
    verification:
      - kind: unit
        ref: "baude-core/src/hook.rs#seed_guard_display_names_file (Display names the file; TUI wiring compile-verified only)"
        status: pass
    human_judgment: true
    rationale: "The app-level spawn test proving the surfaced message through the real add-session path is explicitly plan 09-04's half of D-10; here the wiring is compile-verified and follows the warn_prompt_mode_without_daemon analog verbatim."

duration: 10min
completed: 2026-09-16
status: complete
---

# Phase 9 Plan 01: Hook Seeding Safety Tracer Summary

**Guarded seed_settings refuses to overwrite unreadable/unparseable/non-object settings.local.json, returning SeedWarning values wired end to end through Backend::prepare_cwd to the TUI add-session spawn path (HREG-03)**

## Performance

- **Duration:** 10 min
- **Started:** 2026-09-16T04:25:57Z
- **Completed:** 2026-09-16T04:35:31Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments

- Replaced the swallow-and-coerce read in `seed_settings` with `read_settings_guarded`: `NotFound` is the only fresh-seed path; any other read error, parse error, or non-object root leaves the file byte-identical and returns a structured `SeedWarning` (D-01)
- `SeedWarning { file, reason }` + `SeedWarningReason` (Unreadable/Unparseable/NonObjectRoot/WriteFailed) with a `Display` that names the affected file and tells the operator how to act (D-02); baude-core gained zero print macros
- `Backend::prepare_cwd` now returns `Vec<SeedWarning>`; `ClaudeBackend` propagates the seed warnings, opencode returns `Vec::new()`; whole workspace compiles with the unconverted call sites (app.rs:5245, manager.rs:1158/:2136) intentionally ignoring the return until 09-04
- TUI `warn_seed_failure` surfaces each warning via `set_message` (every time) plus a once-per-process stderr echo, consumed at the add-session spawn path (app.rs:2841)
- Full guard disposition matrix unit-tested, including write-failure preservation and byte-stability of refused files across repeated seed attempts

## Task Commits

Each task was committed atomically (TDD: RED then GREEN):

1. **Task 1 RED: failing seed guard tests + type scaffolding** - `807fa47` (test)
2. **Task 1 GREEN: guarded seed_settings + trait ripple + TUI surface** - `04159f1` (feat)
3. **Task 2: complete disposition matrix tests** - `fab886d` (test)

No REFACTOR commit — the GREEN implementation needed no cleanup pass.

## Files Created/Modified

- `baude-core/src/hook.rs` - SeedWarning types, read_settings_guarded, guarded seed_settings, 8 seed_guard_*/extended tests
- `baude-core/src/backend/mod.rs` - trait `prepare_cwd -> Vec<crate::hook::SeedWarning>`
- `baude-core/src/backend/claude.rs` - prepare_cwd returns seed_settings warnings (seed_mcp_config still unguarded until 09-04)
- `baude-core/src/backend/opencode.rs` - prepare_cwd returns Vec::new()
- `baude/src/app.rs` - warn_seed_failure + warning loop at the add-session spawn path

## Decisions Made

- Shared `read_settings_guarded` helper (pub(crate)) rather than two per-site guards — 09-04 reuses it from backend/claude.rs
- Kept `seed_settings` without `#[must_use]` so the fixture caller at git.rs:3922 stays untouched, per plan
- stderr dedup uses one `static WARNED: AtomicBool` shared by all seed warnings; the TUI message shows every time

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Write-failure test mechanism corrected to actually induce the failure**
- **Found during:** Task 2 (seed_guard_write_failure_warns_and_preserves_original)
- **Issue:** The plan specified chmod'ing the `.claude` dir to 0o555 to make the write fail, but on unix directory write permission only gates creating/removing entries — `std::fs::write` truncating an EXISTING `settings.local.json` succeeds in a 0o555 dir, so the test as specified could not fail the write
- **Fix:** The file itself is chmod'd 0o444 (read still succeeds, O_TRUNC open fails with EACCES); the 0o555 dir chmod is kept as belt-and-braces; both restored before cleanup
- **Files modified:** baude-core/src/hook.rs
- **Verification:** Test passes, asserting exactly one WriteFailed warning and original bytes intact
- **Committed in:** fab886d (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug in specified test mechanism)
**Impact on plan:** None on scope — the test pins exactly the behavior the plan required; only the failure-induction mechanism changed.

## TDD Gate Compliance

- **Task 1 (tracer, tdd):** RED `807fa47` (target test `seed_guard_unparseable_file_left_untouched_and_warned` failed on the behavior assertion — 0 warnings returned, file clobbered; `check tdd-red-evidence` verdict `RED_EVIDENCE_OK`) → GREEN `04159f1` (all tests pass, workspace check clean). RED includes the minimal type/signature scaffolding Rust needs to compile the tests; behavior in RED was unchanged (still clobbered).
- **Task 2 (auto, tdd):** The five matrix tests went green immediately — expected, not a violation: the plan's tracer (Task 1) implements the full four-way disposition, and Task 2 pins the remaining branches as characterization tests. Committed as `test(09-01)` `fab886d`.
- **Tracer feedback gate:** auto mode active — tracer `<verify>` re-run green end to end before expansion (`cargo test -p baude-core --lib hook::` 30 passed; `cargo check --workspace --all-targets --locked` exit 0).

## Verification Results

- `cargo test -p baude-core --lib hook::` — 30 passed, 0 failed
- `cargo check --workspace --all-targets --locked` — exit 0 (trait ripple complete, all four call sites compile)
- `cargo fmt --check` — clean; `cargo clippy --workspace --all-targets` — clean, no warnings
- Source assertion: no `eprintln!`/`println!` added anywhere under baude-core/src (D-02)

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- `SeedWarning` shape and `read_settings_guarded` are ready for 09-04's `.mcp.json` guard and the bauded/restart surfaces
- 09-02 (lock regression tests) and 09-03 (quoting) are independent of this plan's seam
- Unconverted `prepare_cwd` call sites (app.rs:5245, manager.rs:1158/:2136) compile while ignoring the return — 09-04 wires them

---
*Phase: 09-hook-seeding-safety*
*Completed: 2026-09-16*

## Self-Check: PASSED
