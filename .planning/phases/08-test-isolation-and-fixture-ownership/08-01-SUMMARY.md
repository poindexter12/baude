---
phase: 08-test-isolation-and-fixture-ownership
plan: 01
subsystem: testing
tags: [rust, cargo-features, test-isolation, thread-local, raii, rustfmt, tdd]

# Dependency graph
requires:
  - phase: 07
    provides: worktree lifecycle and hook seeding paths whose fixtures produced the leak
provides:
  - "baude_core::testing — one RAII TestRedirect unifying the worktrees base, config dir, claude config dir, and hook command"
  - "A cross-crate `test-support` cargo feature that carries the guard into baude's and bauded's test binaries"
  - "assert_contained — a containment predicate that aborts any test resolving a real user path"
  - "NoFixtureRoot — a thread-local escape probe for #[should_panic] tests that mutates no environment variable"
  - "Deletion of the ad-hoc setters (set_worktrees_base_for_test, set_hook_command_for_test) and the REQUIRE_WORKTREES_OVERRIDE arming flag"
  - "Fixture owner types (AdmissionRepo, FixtureRepo) that keep a redirect alive for a whole test body"
affects: [08-02, 08-03, 08-04, 08-05, 08-06, 08-07, 08-08]

actuals:
  tokens: 17185
  tasks: 3
  commits: 5  # `git rev-list --count 2d64765..HEAD` — 4 task commits + this plan's docs commit
plan_head_before: 2d64765eafb9700e58f1f7a8a729411a258df505

tech-stack:
  added: []
  patterns:
    - "Cross-crate test support via a cargo feature enabled from downstream [dev-dependencies], not #[cfg(test)]"
    - "RAII guard returning an owner value the caller must bind; #[must_use] on both the guard and every helper that returns one"
    - "Containment predicate (did this resolve inside the fixture root?) rather than override-presence"
    - "Thread-local probes instead of process-wide env mutation, so parallel tests stay independent"

key-files:
  created:
    - baude-core/src/testing.rs
  modified:
    - baude-core/Cargo.toml
    - baude/Cargo.toml
    - bauded/Cargo.toml
    - baude-core/src/lib.rs
    - baude-core/src/persist.rs
    - baude-core/src/git.rs
    - baude-core/src/hook.rs
    - baude/src/app.rs
    - bauded/src/api.rs
    - bauded/src/manager.rs

key-decisions:
  - "The guard is gated on cfg(any(test, feature = \"test-support\")), not cfg(test): rustc --test sets `test` per crate, so a cfg(test)-only guard would be absent from exactly the two binaries that leaked."
  - "assert_contained checks containment, not override-presence, so it needs no arming state and holds for the first test in a binary as much as the thousandth (D-09)."
  - "git::worktrees_base split into an ungated real_worktrees_base (production leak scanner, plan 04) and a redirected, containment-asserting resolver."
  - "app.rs and api.rs fixture helpers return owner structs (AdmissionRepo, FixtureRepo); a bare TestRedirect return value would drop in the same statement and leave the fixture unredirected while still compiling."
  - "removal_app's tuple leads with the AdmissionRepo so the redirect owner must be destructured into a named binding."
  - "Task 3's RED was produced by an intentional mutation (removing assert_contained from persist::config_base) rather than by absence of the subject, because the gate the tests observe landed in tasks 1-2."

patterns-established:
  - "Pattern: fixture helpers return an owner, never a path alone — the redirect's lifetime is the fixture's lifetime"
  - "Pattern: escape tests use a thread-local NoFixtureRoot probe, never set_var/remove_var, so they need no serial flag"
  - "Pattern: production resolvers that must reach the real root (the leak scanner) are ungated and separately named"

requirements-completed: [TISO-01, TISO-03]

coverage:
  - id: D1
    description: "The test-support cargo feature carries baude_core::testing into baude's and bauded's test binaries"
    requirement: TISO-01
    verification:
      - kind: unit
        ref: "baude/src/app.rs#app::tests::test_support_gate_is_active"
        status: pass
    human_judgment: false
  - id: D2
    description: "One TestRedirect value redirects the worktrees base, config dir, claude config dir, and hook command together, and restores all of them on drop"
    requirement: TISO-01
    verification:
      - kind: unit
        ref: "baude-core/src/testing.rs#testing::tests (5 cases: redirect_contains_the_managed_worktree_root, nested_redirects_restore_the_outer_root, nested_hook_command_restores_the_outer_command, real_worktrees_base_is_never_redirected, unguarded_resolution_panics)"
        status: pass
      - kind: unit
        ref: "baude-core/src/persist.rs#persist::tests::config_dir_honours_redirect"
        status: pass
    human_judgment: false
  - id: D3
    description: "A real-path resolution with no redirect fails the test in all three binaries instead of writing to the developer's home directory"
    requirement: TISO-03
    verification:
      - kind: unit
        ref: "baude-core/src/testing.rs#testing::tests::unguarded_resolution_panics"
        status: pass
      - kind: unit
        ref: "baude/src/app.rs#app::tests::unguarded_resolution_panics"
        status: pass
      - kind: unit
        ref: "bauded/src/manager.rs#manager::tests::unguarded_resolution_panics"
        status: pass
    human_judgment: false
  - id: D4
    description: "Every ad-hoc override setter is deleted and all twelve call sites migrated to the unified guard"
    requirement: TISO-01
    verification:
      - kind: other
        ref: "grep -rn 'set_worktrees_base_for_test|set_hook_command_for_test|REQUIRE_WORKTREES_OVERRIDE|WORKTREES_BASE_OVERRIDE|HOOK_COMMAND_OVERRIDE' --include='*.rs' . => 0 occurrences"
        status: pass
      - kind: unit
        ref: "baude/src/app.rs#app::tests::admission_fixture_contains_managed_worktrees, admission_fixture_records_no_remote_head"
        status: pass
      - kind: integration
        ref: "cargo check --workspace --all-targets --locked (exit 0, 0 warnings)"
        status: pass
    human_judgment: false
  - id: D5
    description: "No test-support code is reachable from a release build (D-12)"
    verification:
      - kind: other
        ref: "cargo build --workspace --release --locked (exit 0); cargo tree --edges normal shows no features on baude-core while --edges normal,dev shows test-support; strings target/release/{baude,bauded} contains no escape-panic marker"
        status: pass
    human_judgment: false
  - id: D6
    description: "The dogfood child's environment pins CLAUDE_CONFIG_DIR and XDG_CONFIG_HOME under its fixture root"
    requirement: TISO-03
    verification:
      - kind: e2e
        ref: "baude/src/app.rs#app::tests::local_tui_dogfood_real_git_flow_survives_restart_without_duplicates — edit recorded, execution deferred to plan 06 by the phase's wave-1 constraint (D-17)"
        status: unknown
    human_judgment: false
    rationale: "Not run in wave 1 by plan instruction: the dogfood regression is plan 06's integration acceptance, after plans 02/03 complete the Claude/push resolvers and per-fixture identities."

# Metrics
duration: 26min
completed: 2026-09-14
status: complete
---

# Phase 08 Plan 01: Cross-Crate Test-Support Gate and Unified Redirect Guard Summary

**One `TestRedirect` now owns every fixture path baude resolves, carried into `baude`'s and `bauded`'s test binaries by a cargo feature rather than `cfg(test)`, with an escape guard that aborts any test reaching the developer's real home directory.**

## Performance

- **Duration:** 26 min (first task commit 2026-09-14T19:00:30-07:00 → close-out 2026-09-14T19:26-07:00)
- **Tasks:** 3 of 3
- **Files modified:** 11 (exactly the plan's `files_modified` set)
- **Commits:** 5 (measured: `git rev-list --count 2d64765..HEAD` — 4 task commits plus the plan's docs commit)

This was a resumed execution. A prior executor died on an API spend limit after applying,
but not committing, the `test-support` feature wiring across the three manifests and
`lib.rs`. That work was reviewed as correct and folded into the Task 1 commit rather than
reverted.

## Accomplishments

- **Refuted and replaced the phase's locked mechanism.** `cfg(test)` is set per crate by
  `rustc --test`, so a `#[cfg(test)]`-only guard inside `baude-core` is compiled OUT of
  `baude`'s and `bauded`'s test binaries — precisely the two that leaked 1431 directories
  (#72). The guard is now gated on `cfg(any(test, feature = "test-support"))` with the
  feature enabled from each downstream crate's `[dev-dependencies]`.
- **Unified four scattered redirects into one RAII guard.** `TestRedirect` holds the
  worktrees base, config dir, claude config dir, hook command, and workspace identity as a
  single value, restores the whole enclosing set on drop, and nests: `with_hook_command`
  and `with_workspace` change one field while preserving the rest.
- **Made escape a test failure instead of a silent write.** `assert_contained` panics when
  a resolution lands outside `BAUDE_TEST_FIXTURE_ROOT`. Because it is a containment
  predicate rather than an override-presence check, it needs no arming state and holds
  from the first instruction of a test binary.
- **Deleted the ad-hoc override machinery entirely.** Two thread-local stores, two setter
  functions, and the `REQUIRE_WORKTREES_OVERRIDE: AtomicBool` arming flag are gone; all
  twelve call sites migrated. `grep` for any of the five removed identifiers returns zero.
- **Closed a lifetime hazard the plan predicted.** `admission_repo`, `admission_repo_cloned`,
  `removal_app`, and `initialized_repo` now return owner structs. A helper returning a bare
  `TestRedirect` would compile and drop the guard in the same statement, leaving every
  fixture unredirected — invisible locally, since the tests stay green.
- **Proved the production cost is zero.** The release build graph selects no features on
  `baude-core` over normal edges, and neither release binary contains the escape-panic
  marker string.

## Task Commits

1. **Task 1 (tracer): gate `baude_core::testing` behind `test-support`, redirect `config_base`** — `611df14` (feat)
2. **Task 2 (TDD): expand the guard to the worktrees root and hook command, delete the arming flag**
   - RED — `6c06859` (test)
   - GREEN — `a12723c` (feat)
3. **Task 3 (TDD): prove the gate reaches all three test binaries and none of the shipped ones** — `c52be5b` (test)

**Plan metadata:** see the `docs(08-01)` commit following this summary.

## Files Created/Modified

- `baude-core/src/testing.rs` — **created.** `Redirects`, `TestRedirect`, `NoFixtureRoot`,
  `assert_contained`, the five override readers, and `ESCAPE_PANIC_MARKER`. The module doc
  records why a cargo feature and not `cfg(test)`, why containment and not
  override-presence, and why `persist`'s explicit-root `*_at` APIs must NOT be folded in.
- `baude-core/Cargo.toml` — declares the `test-support` feature.
- `baude/Cargo.toml`, `bauded/Cargo.toml` — `[dev-dependencies] baude-core = { workspace = true, features = ["test-support"] }`.
- `baude-core/src/lib.rs` — `#[cfg(any(test, feature = "test-support"))] pub mod testing;`
- `baude-core/src/persist.rs` — `config_base` split into `real_config_base` plus a redirected,
  containment-asserting resolver; `release_state_lock_for_test` widened from a private
  `#[cfg(test)]` fn to `pub` under the feature so downstream fixtures can reach it.
- `baude-core/src/git.rs` — `real_worktrees_base` (ungated, for plan 04's leak scanner) split
  from the redirected `worktrees_base`; override store, arming flag, and setter deleted.
- `baude-core/src/hook.rs` — `baude_hook_command` reads the unified redirect; its own
  thread-local and setter deleted.
- `baude/src/app.rs` — `AdmissionRepo` owner type; `admission_repo`, `admission_repo_cloned`,
  and `removal_app` return it; ~22 callers retain it in a named binding; the reseeding test
  binds an inner `with_hook_command` scope and asserts its drop restores the enclosing set;
  the dogfood child `Command` gains `CLAUDE_CONFIG_DIR` and `XDG_CONFIG_HOME`; new
  `test_support_gate_is_active` and `unguarded_resolution_panics`.
- `bauded/src/api.rs` — `FixtureRepo` owner returned by `initialized_repo`, held across
  every handler await at all five call sites.
- `bauded/src/manager.rs` — ten fixture preambles reduced to one `TestRedirect` each; new
  `unguarded_resolution_panics`.

## Verification

Every number below is from a run performed in this session, with each gate's exit code
captured directly rather than through a pipe.

| Gate | Exit | Result |
|---|---|---|
| `cargo test -p baude-core --lib testing::` | 0 | 5 passed; 0 failed; 246 filtered out |
| `cargo test -p baude-core --lib persist::tests::config_dir_honours_redirect` | 0 | 1 passed; 0 failed; 250 filtered out |
| `cargo test -p baude --bins test_support_gate_is_active` | 0 | 1 passed; 0 failed; 67 filtered out |
| `cargo test -p baude --bins unguarded_resolution_panics` | 0 | 1 passed; 0 failed; 67 filtered out |
| `cargo test -p bauded --bins unguarded_resolution_panics` | 0 | 1 passed; 0 failed; 79 filtered out |
| `cargo test -p baude-core --lib unguarded_resolution_panics` | 0 | 1 passed; 0 failed; 250 filtered out |
| `cargo test -p baude --bins admission_fixture_` | 0 | 2 passed; 0 failed; 66 filtered out |
| `cargo check --workspace --all-targets --locked` | 0 | 0 warnings |
| `cargo build --workspace --release --locked` | 0 | finished in 1m 06s |
| `cargo fmt --all --check` | 0 | clean |

**D-12 feature-graph check.** `cargo tree -p baude --edges normal --format '{p} {f}'` and the
same for `bauded` list `baude-core` with an EMPTY feature set; adding `,dev` to the edge
filter lists it with `test-support`. `strings target/release/baude` and
`strings target/release/bauded` contain no occurrence of `resolved to the real user path`.

**Setter removal.** `grep -rn` across `*.rs` for `set_worktrees_base_for_test`,
`set_hook_command_for_test`, `REQUIRE_WORKTREES_OVERRIDE`, `WORKTREES_BASE_OVERRIDE`, and
`HOOK_COMMAND_OVERRIDE` returns zero occurrences for all five.

## TDD Gate Compliance

**Task 2 — genuine RED from a missing implementation.** Commit `6c06859` added five tests to
`baude-core/src/testing.rs` against an implementation that did not yet read the unified
redirect. Measured: `cargo test -p baude-core --lib testing::` exit **101**,
`test result: FAILED. 1 passed; 4 failed; 0 ignored; 246 filtered out`. The failure named the
real defect — `unguarded_resolution_panics` did not panic and `git::worktrees_base()` returned
`/Users/joese/.local/share/baude/worktrees`, i.e. the leak itself (resolution only; nothing was
written there). Evidence verdict: `RED_EVIDENCE_OK`. GREEN followed in `a12723c` at exit 0,
5 passed.

**Task 3 — RED from an intentional mutation, disclosed.** The two downstream escape tests
assert a panic produced by the gate that tasks 1 and 2 had already delivered, so they passed
on arrival (measured first: exits 0/0/0, 1 passed each). Rather than record a fabricated RED,
the containment assert was temporarily removed from `persist::config_base` — the exact
mutation these tests exist to detect — and the gate re-run. Measured: `cargo test -p baude
--bins unguarded_resolution_panics` exit **101** with
`note: test did not panic as expected at baude/src/app.rs:5556`, and the same for `bauded`
at `manager.rs:2600`. `baude-core`'s case stayed green because it exercises
`git::worktrees_base`, which the mutation did not touch — a correct negative control. The
guard was restored before committing; `persist.rs` carries no change in `c52be5b`. Evidence
verdict: `RED_EVIDENCE_OK`. "Passes without panicking" is the literal warning sign the plan
names for a vanished downstream gate, so this mutation is the proof that the tests detect it.

## Decisions Made

Recorded in the frontmatter `key-decisions`. The load-bearing ones:

- **Cargo feature over `cfg(test)`** (RESEARCH Deviation 1). This is the correction the whole
  phase depends on; without it the guard would be absent from the two leaking binaries.
- **Containment over override-presence** (D-09). Lets the re-exec'd dogfood child pass on its
  own isolation while still failing any unredirected in-process resolution.
- **Owner structs, not bare guards** (the plan's explicit lifetime warning). `AdmissionRepo`
  additionally carries `with_path` so `admission_repo_cloned` retains the pushed fixture's
  redirect while selecting the clone.
- **`removal_app` returns a 7-tuple led by the owner**, documented so a bare `_` destructure
  is recognizable as a bug rather than a tidy-up.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Dead-code warning on `replace_workspace_override`**
- **Found during:** Task 1
- **Issue:** `replace_workspace_override` is declared now so plan 03 need not edit this module
  (D-04), but nothing consumes it yet; CI runs `cargo clippy --all-targets -- -D warnings`.
- **Fix:** `#[allow(dead_code)]` with a comment naming plan 03 as the consumer.
- **Files modified:** `baude-core/src/testing.rs`
- **Verification:** `cargo check --workspace --all-targets --locked` exit 0, 0 warnings.
- **Committed in:** `611df14`

**2. [Rule 1 - Bug] `initialized_repo` dropped its redirect at function return**
- **Found during:** Task 2
- **Issue:** The bulk migration of `bauded` preambles placed
  `let _redirect = TestRedirect::new(root);` INSIDE `initialized_repo`, so the guard dropped
  when the helper returned and left all five callers unredirected — exactly the failure mode
  the plan warns about, and it compiled cleanly.
- **Fix:** Introduced the `FixtureRepo` owner returned from the helper and bound at each
  caller, `#[must_use]` on the helper.
- **Files modified:** `bauded/src/api.rs`
- **Verification:** `cargo check -p bauded --all-targets --locked` exit 0; all five callers
  now hold a named owner.
- **Committed in:** `a12723c`

**3. [Rule 1 - Bug] Duplicated doc comment and unused imports after setter deletion**
- **Found during:** Task 2
- **Issue:** Removing the setters orphaned `use std::cell::RefCell;` in `git.rs` and
  `hook.rs` and `use std::sync::atomic::{AtomicBool, Ordering};` in `git.rs`, broke one
  remaining call site at `git.rs:3907`, and left a duplicated header on
  `baude_hook_command`'s doc block.
- **Fix:** Removed the three imports, rewrote the call site to
  `TestRedirect::with_hook_command("/opt/baude hook")`, and collapsed the doc comment.
- **Files modified:** `baude-core/src/git.rs`, `baude-core/src/hook.rs`
- **Verification:** `cargo check -p baude-core --all-targets` exit 0 (was 101).
- **Committed in:** `a12723c`

### Planned-but-adjusted

**4. Task 3's RED could not come from a missing implementation.** Disclosed in full under
*TDD Gate Compliance* above. No production code was committed in a mutated state.

**5. `baude-core/src/testing.rs` needed no Task 3 edit.** The plan lists it in Task 3's
`<files>` so the core escape case would carry the shared name suffix; that case already
existed under exactly that name from Task 2, so the filter selects it with no change.

---

**Total deviations:** 3 auto-fixed (1× Rule 3, 2× Rule 1) plus 2 disclosed adjustments.
**Impact on plan:** None on scope. All 11 planned files were modified and no caller was
dropped. Deviation 2 was a defect introduced and corrected within the same task; had it
shipped, the phase would have delivered a guard that silently did nothing for `bauded`.

## Issues Encountered

- **RED evidence schema.** The first evidence record used snake_case keys and was rejected as
  `INVALID_RED (invalid_record)`. The validator expects `{command, targetTest, exitCode,
  output}` with `output` parsed as TAP, and `targetTest` must match a failing test's fully
  qualified name (a bare `unguarded_resolution_panics` was rejected as
  `no_target_test_failure`). Cargo's `test <name> ... ok|FAILED` lines were translated
  mechanically into `ok N - <name>` / `not ok N - <name>` plus the `# tests/pass/fail`
  summary — a lossless format translation of a real run.
- **`TestRedirect::new(root)` vs `(&root)`.** The bulk migration in `manager.rs` moved the
  owned `PathBuf` into the guard, so ten preambles failed to borrow `root` afterwards.
  Corrected to `&root`.
- **`git.rs` shadows `Result`.** Any new function there must spell
  `std::result::Result<_, _>` explicitly.

## Known Stubs

None. No `TODO`, `FIXME`, `todo!`, `unimplemented!`, or `#[ignore]` was introduced in any of
the 11 files.

## Safety

No file under a real user root was read, written, created, or removed during this execution.
The only contact with the real tree was *path resolution*: the Task 2 RED run printed
`/Users/joese/.local/share/baude/worktrees` as the value `git::worktrees_base()` returned
without a redirect, which is the defect the test exists to report. Historical leak cleanup
remained preview-only; nothing was pruned. All fixture roots used by the tests run here are
synthetic (`/nonexistent/...`) or under `std::env::temp_dir()`.

## User Setup Required

None — no external service configuration required. No package was added: the manifest changes
are a feature declaration and dev-dependency wiring of the existing in-repo path dependency
(threat register T-08-SC).

## Next Phase Readiness

**Ready for wave 2.** `baude_core::testing` is the module every remaining Phase 8 plan hangs
off, and its shape is now fixed:

- **Plan 02** (Claude/push resolvers) consumes `claude_config_dir_override()`, already present
  and unused.
- **Plan 03** (per-fixture identity) consumes `TestRedirect::with_workspace` and
  `replace_workspace_override`, both declared with plan 03 named as the consumer so that plan
  needs no edit to this module. Note `replace_workspace_override` asserts a live identity
  scope — injected initialization must never implicitly arm an override.
- **Plan 04** (leak scanner) consumes `git::real_worktrees_base()`, deliberately ungated and
  never redirected, with a test pinning that property.
- **Plan 06** owns the deferred integration acceptance: the dogfood regression (its child
  environment edits are recorded here but unexecuted), the broad fixture suites, and
  `cargo clippy --all-targets -- -D warnings`.

**One concern to carry forward.** `BAUDE_TEST_FIXTURE_ROOT` is unset in a plain `cargo test`,
so `assert_contained` now panics on any unredirected real resolution anywhere in the suite.
That is intended (D-09), and it means broad fixture suites will fail until plans 02, 03, and
06 finish migrating their consumers. A full-suite failure between now and then is the guard
working, not a regression — diagnose it by the escape-panic message, not by weakening the
guard.

## Requirements Bookkeeping

`requirements-completed` above copies this plan's `requirements:` frontmatter, as the SUMMARY
contract requires. **REQUIREMENTS.md was deliberately NOT checked off.**
`requirements.mark-complete TISO-01 TISO-03` reported both as `not_found` — their entries
carry a `(partial)` suffix the matcher does not accept — and, more importantly, both are
phase-wide requirements that plan 01 only partially delivers. Checking them off after 1 of 8
plans would be a false completion claim. Recorded as a STATE.md blocker: do not mark either
complete before plan 06 lands the remaining consumer migrations.

## Self-Check: PASSED

All 11 planned files plus this summary exist on disk; all four task commits
(`611df14`, `6c06859`, `a12723c`, `c52be5b`) resolve in `git log`.

---
*Phase: 08-test-isolation-and-fixture-ownership*
*Completed: 2026-09-14*
