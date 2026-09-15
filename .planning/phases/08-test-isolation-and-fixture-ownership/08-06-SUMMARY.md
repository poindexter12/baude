---
phase: 08-test-isolation-and-fixture-ownership
plan: 06
subsystem: testing
tags: [rust, bauded, fixtures, raii, ci, python, observability, containment]

# Dependency graph
requires:
  - phase: 08-01
    provides: "`baude_core::testing::TestRedirect` (one guard for config dir, state dir, managed worktrees root, hook command and Claude config dir) and `assert_contained`, the resolution-only escape guard this plan's migrations answer to"
  - phase: 08-02
    provides: "the redirected push/VAPID storage that made `bauded`'s own daemon state fixture-bound"
  - phase: 08-03
    provides: "`workspace::override_for_test(&Config, hint) -> TestRedirect` and the owner convention (identity declared before root in a struct, acquired after root in a body) this plan's three new owners follow"
  - phase: 08-08
    provides: "inert ambient App readers and `Pty::spawn_registered_with`'s contained-root precondition, which is what made every PTY-touching Manager case need an owner"
provides:
  - "`ManagerFixture` — one construction that establishes a process-unique root, the unified redirect and a literal workspace identity, holding both guards as fields"
  - "`scripts/assert-real-roots-untouched.sh` — a before/after fingerprint of the three real roots with its own 29-check synthetic self-test"
  - "A CI bracket around the full serial suite that runs the `after` comparison even when the suite fails"
  - "`api_scope` in bauded/src/api.rs and `LifecycleFixture` in baude-core/src/lifecycle.rs — the two owners the broad run proved were missing"
affects: [08-07, ship-gate, ci]

actuals:
  tokens: 23476
  tasks: 2
  commits: 9
  plan_head_before: 8a92712adcba74cef20a62a2b0eb73371abf11b7

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "An owner struct holds every guard as a FIELD; an unbound guard arms and disarms in the same statement and leaves the fixture running unredirected"
    - "Struct fields drop in declaration order and locals in reverse declaration order, so an owner declares `_identity` before `_redirect` while a test body acquires root first and identity second"
    - "A fixture helper returns the OWNER, never `(root, workspace)` — the old shape dropped its redirect at the return and left every caller unredirected"
    - "Suite-level containment is asserted by an observer EXTERNAL to the test process; no-write measurement is recorded separately from the in-process no-read evidence"
    - "Each gate's exit status is captured into its own variable; nothing is piped, because a pipeline reports the last stage and masks the real failure"

key-files:
  created:
    - scripts/assert-real-roots-untouched.sh
  modified:
    - bauded/src/manager.rs
    - .github/workflows/ci.yml
    - bauded/src/api.rs
    - baude-core/src/lifecycle.rs
    - baude/src/app.rs

key-decisions:
  - "`ManagerFixture` holds `_identity` and `_redirect` as fields in that declaration order, so nested fixtures unwind in reverse construction order (D-01, D-04, D-05, D-08, D-18)"
  - "The two diverging preamble helpers (`persistence_fixture`, `removal_manager`) now return the fixture owner rather than `(root, workspace)`; the old return shape was itself the leak"
  - "The observer resolves each root through its own precedence chain in Python rather than shell defaulting, because `var_os` presence (including an empty value) is not `${VAR:-default}`"
  - "Snapshots are written outside every observed root, under BAUDE_ROOT_SNAPSHOT_DIR, then RUNNER_TEMP, then the system temp dir"
  - "A root absent before and after is a pass; a root created during the run is a failure — creating one of those directories is exactly the leak this phase closes"
  - "The `after` CI step is `if: always()` and guarded on the before-snapshot having succeeded, so a run that fails AND leaks is still caught"
  - "The child subprocesses of the self-test are launched with an explicit env map pinning BAUDE_ASSERT_PYTHON and BAUDE_ASSERT_BASH, because PATH `bash`/`python3` are mise shims that need state a synthetic HOME does not have"

patterns-established:
  - "`fixture_isolation_` as the narrow filter name for helper ownership/identity regressions, matching `worker_isolation_` and `ui_fixture_isolation_`"
  - "A local rehearsal of the observer runs with XDG_CONFIG_HOME/XDG_DATA_HOME/CLAUDE_CONFIG_DIR pointed at a throwaway fixture tree, so the developer's real roots are never even read; `assert_contained` is unaffected because it keys on BAUDE_TEST_FIXTURE_ROOT, which is independent of XDG"

requirements-completed: [TISO-01, TISO-02, TISO-03]

coverage:
  - id: D1
    description: "A `bauded` fixture is one construction that establishes root, redirects and workspace identity together, and the redirect it establishes cannot outlive or under-live it (D-04, D-18)"
    requirement: TISO-01
    verification:
      - kind: unit
        ref: "bauded/src/manager.rs#manager::tests::fixture_isolation_two_fixtures_have_distinct_roots, ::fixture_isolation_redirect_outlives_the_construction_statement, ::fixture_isolation_installs_a_literal_workspace_identity, ::fixture_isolation_nested_drop_restores_the_enclosing_fixture, ::fixture_isolation_drop_removes_its_root"
        status: pass
    human_judgment: false
  - id: D2
    description: "Every Manager case that reaches an ambient reader or any PTY creation/restart/open-shell path owns a fixture; no preamble copy remains to be copied an eleventh time (T-08-21)"
    requirement: TISO-01
    verification:
      - kind: unit
        ref: "cargo test -p bauded --bins manager:: — 47 passed / 0 failed (was 8 passed / 34 failed before the migration)"
        status: pass
    human_judgment: false
  - id: D3
    description: "Each fixture holds a literal workspace identity of its own; a test without an override panics rather than reusing another fixture's process cache (T-08-09)"
    requirement: TISO-02
    verification:
      - kind: unit
        ref: "bauded/src/manager.rs#manager::tests::fixture_isolation_installs_a_literal_workspace_identity, ::fixture_isolation_nested_drop_restores_the_enclosing_fixture"
        status: pass
    human_judgment: false
  - id: D4
    description: "The observer resolves all three roots through the app's exact precedence chains, including CLAUDE_CONFIG_DIR winning over HOME with no XDG lookup and no `baude` suffix, empty-but-present override values, injected passwd-home fallback, and the terminal no-home fallbacks (T-08-20)"
    requirement: TISO-03
    verification:
      - kind: integration
        ref: "bash scripts/assert-real-roots-untouched.sh --self-test — 29 checks, 0 failures (resolver_table 8 cases, comparison_table 6 brackets, snapshot_location_table, resolution_change_table)"
        status: pass
    human_judgment: false
  - id: D5
    description: "A full serial suite run changes none of the three real roots, and the suite status and the observer status are reported independently (T-08-20, T-08-22)"
    requirement: TISO-03
    verification:
      - kind: integration
        ref: "before=0 / `cargo test -- --test-threads=1`=0 (497 passed, 0 failed, 0 ignored) / after=0 — PASS: the run left all three real roots untouched"
        status: pass
    human_judgment: false
  - id: D6
    description: "Concurrency does not change containment: both downstream binaries run unserialized inside the same before/after bracket"
    requirement: TISO-03
    verification:
      - kind: integration
        ref: "before=0 / `cargo test -p baude --bins`=0 (74 passed) + `cargo test -p bauded --bins`=0 (91 passed) / after=0"
        status: pass
    human_judgment: false
  - id: D7
    description: "A leak is reported by NAME — which root changed and which entries differ — not as a bare hash mismatch (T-08-22)"
    requirement: TISO-03
    verification:
      - kind: integration
        ref: "scripts/assert-real-roots-untouched.sh#comparison_table — the six synthetic brackets (unchanged, created, removed, modified, absent-both, resolved-differently) each assert the named root and the printed added/removed/modified entries"
        status: pass
    human_judgment: false
  - id: D8
    description: "The local rehearsal of this observer never read, wrote or fingerprinted the developer's real roots, and nothing pruned or deleted any of the ~1433 historical leak candidates"
    requirement: TISO-03
    verification: []
    human_judgment: true
    rationale: "A negative about the real filesystem cannot be proven from inside the suite. The evidence is that every rehearsal command ran under an explicit `env` map pointing XDG_CONFIG_HOME, XDG_DATA_HOME and CLAUDE_CONFIG_DIR at a scratchpad tree — the observer's own output names the fixture paths it compared — and that this plan added no prune caller."

# Metrics
duration: 58min
completed: 2026-09-15
status: complete
---

# Phase 08 Plan 06: Suite-Level Real-Root Assertion and the `bauded` Fixture Owner Summary

One construction now establishes a `bauded` fixture's root, redirects and workspace identity, and a
scripted before/after observer wired into CI proves a whole suite run leaves the real config dir,
the real `~/.claude` and the real managed-worktrees root byte-identical.

## Performance

| Metric | Value |
|--------|-------|
| Duration | 58 min |
| Tasks | 2 |
| Commits | 9 |
| Files created | 1 |
| Files modified | 5 |

## Accomplishments

**Task 1 — one fixture construction replaces ten copied preambles.** `ManagerFixture` follows the
`GitFixture` convention already in `baude-core/src/git.rs`: an `AtomicU64` producing a
process-unique root name, construction that creates the root and takes a label, and a `Drop` that
removes the root and swallows its errors. It holds `_identity` and `_redirect` as **fields**, in
that declaration order — struct fields drop in declaration order, the inverse of locals, so the
workspace identity is restored while the root redirect it was resolved under is still installed.
Thirty-eight constructions now exist: the ten original preambles, twenty-one further Manager cases
an audit found reaching an ambient reader or a PTY path, and seven inside the five
`fixture_isolation_` helper regressions.

**Task 2 — the phase goal is now asserted, not assumed.** `scripts/assert-real-roots-untouched.sh`
is a bash wrapper around embedded Python 3 stdlib. It re-implements each of the three precedence
chains exactly (including `Path::join` semantics on plain strings, because `PathBuf::from("")` stays
empty where `Path("")` normalizes to `"."`), fingerprints each root as a sorted listing of entries
with sizes and mtimes, hashes it, and stores the snapshot outside every observed root. `before`
records; `after` recomputes and names the root that changed along with the differing entries. A
`--self-test` mode proves all of that against synthetic trees only. CI gained three steps: the
self-test, the `before` fingerprint, and an `if: always()` `after` comparison guarded on the
snapshot having been taken.

**The deferred broad run happened, and it found four unmigrated consumer groups.** This plan was the
phase's first broad-test boundary. Running it surfaced twenty-two `bauded::manager` failures, eight
`bauded::api` failures, the five `baude-core::lifecycle` escapes deferred to this plan, and three
`baude` app failures. All are fixed; the full serial suite is now 497 passed / 0 failed / 0 ignored.

## Task Commits

| Task | Commit | Message |
|------|--------|---------|
| 1 (RED) | `26560d8` | test(08-06): pin the bauded fixture helper before it holds any guard |
| 1 (GREEN) | `6003652` | feat(08-06): hold the redirect and identity as fixture fields |
| 1 | `f277afc` | test(08-06): replace the ten bauded fixture preambles with one construction |
| 1 | `54e2c68` | test(08-06): give every remaining Manager test a fixture owner |
| 2 | `bb3ccf0` | test(08-06): give the eight unowned API cases an isolation scope |
| 2 | `3f98851` | test(08-06): give the five lifecycle activation cases a fixture owner |
| 2 | `2ad1e61` | style(08-06): drop the needless workspace borrows the fixture change left |
| 2 | `fb849b5` | feat(08-06): assert a suite run leaves the three real roots untouched |
| 2 | `5777f75` | test(08-06): give the last three app cases a working fixture and stand-in |

## TDD Gate Compliance

Task 1 is behavior-adding and ran the full cycle:

| Gate | Evidence |
|------|----------|
| RED | `26560d8` — `cargo test -p bauded --bins manager::tests::fixture_isolation_` exited 101 with **2 passed / 3 failed**. The three failures were intentional: the helper existed but let its guards drop at the end of `new`, so the redirect-lifetime, identity and nested-restore assertions failed on real behavior, not on a missing symbol. |
| GREEN | `6003652` — same filter, exit 0, **5 passed / 0 failed**, by moving both guards into fields. |
| REFACTOR | `f277afc`, `54e2c68` — call-site migration with the filter and then `manager::` green throughout; `2ad1e61` is a pure style follow-up with no behavior change. |

Task 2 creates a script and two CI steps and migrates test ownership; it adds no library behavior,
so it is exempt. `bb3ccf0`, `3f98851` and `5777f75` are test-only. Every commit is scoped `(08-06)`.

## Files Created/Modified

| File | Change |
|------|--------|
| `scripts/assert-real-roots-untouched.sh` | **created** — 820 lines; `before`/`after`/`--self-test` modes |
| `bauded/src/manager.rs` | `ManagerFixture` plus 38 constructions replacing ten preambles |
| `.github/workflows/ci.yml` | self-test step, `before` step, `if: always()` `after` step |
| `bauded/src/api.rs` | `ApiScope`/`api_scope` (`pub(super)`), eight unowned cases given a scope |
| `baude-core/src/lifecycle.rs` | `LifecycleFixture` plus the five deferred activation cases |
| `baude/src/app.rs` | two unowned `App::new` sites given a scope; one broken `sleep` stand-in fixed |

## Decisions Made

1. **The helper returns the owner, never `(root, workspace)`.** `persistence_fixture` and
   `removal_manager` were the two preamble copies that diverged, and their divergence *was* the
   leak: both built a `TestRedirect`, returned a tuple, and dropped the guard at the return, so
   every caller ran unredirected while looking correct. Both now return the fixture.
2. **The observer mirrors `var_os` presence, not shell defaulting.** `${VAR:-default}` treats an
   empty value as absent; `std::env::var_os` does not. The Python resolver distinguishes them, and
   the self-test covers the empty-but-present case for all three chains.
3. **A different `after` resolution is itself a failure.** The before snapshot records the resolved
   root paths, so a run that changes where a root resolves is reported rather than silently
   compared against a different directory.
4. **Local rehearsal runs against a fixture tree, never the developer's roots.** The plan's verify
   command observes "the real roots"; on a developer machine that would mean reading
   `~/.config/baude` and `~/.claude`, which this phase's constraints forbid. Every rehearsal command
   ran under an explicit `env` map redirecting the three chains into the session scratchpad. This
   does not weaken anything: `assert_contained` keys on `BAUDE_TEST_FIXTURE_ROOT`, which is
   independent of XDG, so in-process containment was judged exactly as CI will judge it.
5. **TISO-01, TISO-02 and TISO-03 are marked complete** (see "Requirements" below).

## Deviations from Plan

### 1. [Rule 3 — blocking] Eight `bauded::api` cases had no owner

**Found during:** Task 2, first broad `cargo test -p bauded --bins` (83 passed / 8 failed).
**Issue:** these cases build `Manager::new(...)` directly instead of through the `app()` helper, so
they reached the ambient config read with nothing installed and tripped `assert_contained`.
**Fix:** `ApiScope`/`api_scope(label)` in `api.rs`'s `tests` module, made `pub(super)` because
`pty_ws_tests` is a **sibling** of `tests`, not a descendant, and drives the same handlers over a
real socket. Eight cases plus the websocket case now bind one.
**Files:** `bauded/src/api.rs`. **Commit:** `bb3ccf0`. **Result:** 91 passed / 0 failed.

### 2. [Rule 3 — blocking] The five `lifecycle::tests::*` escapes deferred to this plan

**Found during:** Task 2, `cargo test -p baude-core --lib` (327 passed / 5 failed).
**Issue:** WINDOWS entry 2 — five activation cases held no `TestRedirect`.
**Fix:** a `LifecycleFixture` owner following the same convention as `ManagerFixture`, with one
addition: the root is **canonicalized**. Activation compares a composed managed path against the
path Git reports for the same worktree, and on macOS `std::env::temp_dir()` is `/var/…`, a symlink
to `/private/var/…`; without canonicalization
`pending_activation_recovery_distinguishes_absent_and_exact_git_facts` returned `Blocked` where it
expected `Finalized`. The five trailing manual `remove_dir_all` calls are gone — `Drop` owns them.
**Files:** `baude-core/src/lifecycle.rs`. **Commit:** `3f98851`. **Result:** 332 passed / 0 failed.

### 3. [Rule 3 — blocking] Two `baude::app` cases had no owner, and one had a broken stand-in

**Found during:** Task 2, the bracketed full-suite rehearsal (suite exited 101 on three app cases).
**Issue (a):** `hierarchy_action_matrix_dispatches_only_authorized_local_actions` and
`hierarchy_resize_never_sends_zero_dimensions_and_transfers_hidden_shell_focus` each build an `App`
before any helper installs a fixture, so the first ambient config read escaped.
**Fix (a):** both bind the existing 08-03 `isolation_scope(label)` owner as their first statement,
so it outlives every `App`.
**Issue (b):** `standalone_admission_dedup_close_reopen_and_missing_are_durable` panicked on
`Option::unwrap()` at `app.rs:8936`, which is **not** a containment failure. Diagnosis: the case set
`claude_cmd = "sleep 30"` **bare**, while every other stand-in in the file uses `sh -c 'sleep 30'`.
The spawn appends `--dangerously-skip-permissions`; a bare `sleep` takes that as a second operand,
fails usage and exits at once. The dedup assertion earlier in the case only ever saw a live runtime
because it *raced the child's death* — the `git init` between the two assertions gave the child time
to die, after which `admit_standalone` closed the dead runtime and spawned a new id, so the captured
`first` no longer existed. Instrumentation confirmed it: `first=1` but `runtimes={Standalone(1): 2}`.
**Fix (b):** the `sh -c` wrapper every sibling uses, which swallows the flag as an ignored
positional, plus a comment recording why the bare form is wrong. The case is now deterministic.
**Files:** `baude/src/app.rs`. **Commit:** `5777f75`. **Result:** 74 passed / 0 failed.

### 4. [Rule 3 — blocking] 34 clippy `needless_borrow` errors from the fixture change

**Found during:** Task 2 integration gates. `f277afc` changed `workspace` from an owned `Workspace`
to `&Workspace`, so every `&workspace` argument became a needless borrow, and CI's gate is
`-D warnings`. Fixed with a negative-lookahead substitution that preserved the three
`&workspace.state_file(…)` occurrences. **Commit:** `2ad1e61`.

### 5. [Rule 2 — correctness] Scope was widened beyond `files_modified`

The plan's `files_modified` names three files. Deviations 1–3 touch `bauded/src/api.rs`,
`baude-core/src/lifecycle.rs` and `baude/src/app.rs`. Each is outside that list, and each blocked
task 2's whole-workspace gate. The plan's instruction was explicit — *"Fail rather than weaken
guards if an unmigrated consumer surfaces"* — so every one was fixed by giving the consumer an
owner, and none by relaxing a guard.

## Issues Encountered

**`${PIPESTATUS[0]}` returned empty.** The interactive shell here is zsh, where the array is
`pipestatus` and `status` is a **read-only** variable. Both gate rehearsals therefore redirect each
command to its own log and read `$?` immediately, and the driver names its variables `suite_rc` /
`before_rc` / `after_rc`. This is the same discipline the plan demands of the verify command.

**Ten of the self-test's 19 checks failed with `mise ERROR … Config files … are not trusted`.** The
self-test launches subprocesses under a synthetic HOME/XDG_DATA_HOME, and `bash`/`python3` on PATH
are mise shims that need state that synthetic tree does not have. Fixed by pinning
`BAUDE_ASSERT_BASH="${BASH:-/bin/bash}"` in the wrapper and `BAUDE_ASSERT_PYTHON: sys.executable`
in the child env map. The rehearsal additionally pins `/usr/bin/python3` rather than the mise-managed
interpreter, which lives under a root this phase forbids touching.

**`cargo fmt --check` and clippy both failed mid-task** on my own edits (a missing space in six
`api.rs` edits; the 34 borrows above). Both are green now.

## Known Stubs

None. No placeholder, empty-value or "coming soon" path was introduced.

## Broken-Windows Ledger

Three entries closed, none opened:

| id | Entry | Closed by |
|----|-------|-----------|
| 1 | the two `#[ignore]`d `ui_fixture_isolation_` tests | both now run — 2 passed, **0 ignored** |
| 2 | the five `lifecycle::tests::*` fixtures holding no `TestRedirect` | `3f98851` — 332 passed / 0 failed |
| 5 | 08-08's deliberately unrun full workspace suite | run here — 497 passed / 0 failed |

Entries 3, 4, 6, 7 and 8 stay open; none is this plan's to close. Every deviation recorded above
was fixed inside this plan, so no new entry was appended.

## Threat Flags

None. This plan adds no network endpoint, auth path or schema change. The one new file is a
read-only observer: it stats and hashes directory listings and writes only its own snapshot, into a
directory it first asserts lies outside every root it observes.

## Requirements

**TISO-01, TISO-02 and TISO-03 are marked complete.** Each requirement's recorded gap is closed and
the closure is now asserted rather than assumed:

- **TISO-01** gap was "config has no test redirect … and `bauded/src/push.rs` writes real VAPID
  keys." `TestRedirect` (08-01) covers the config dir; push storage is redirected and has its own
  regressions (`push.rs` tests at 390/432/479). The suite-level observer now proves a 497-test run
  creates or modifies none of the three roots.
- **TISO-02** gap was "workspace identity is a process-wide `OnceLock` … shared by every fixture,
  with no reset." `workspace::override_for_test` (08-03) gives each fixture a literal identity,
  `active()` panics in support builds without one, and the unserialized both-binaries run inside the
  observer bracket shows concurrency does not change containment.
- **TISO-03** gap was "the guard covers one path only … and the flag is armed per-binary by the
  first fixture, so `baude-core`'s own tests never arm it." `assert_contained` covers config, state,
  Claude and worktrees roots, keys on `BAUDE_TEST_FIXTURE_ROOT` (independent of XDG), and demonstrably
  armed in all three binaries this session — it is what caught the 22 manager, 8 api, 5 lifecycle and
  2 app escapes. Escapes are resolution-only, so no real-root I/O occurs on the failing path.

**TISO-04 remains partial**, owned by plan 08-07 (WINDOWS entry 8).

**Still open and explicitly NOT closed here:** `App::open_editor` (`baude/src/app.rs:5049`, the
`Command::new("sh")` spawn at `:5077`) and `copy_to_clipboard`'s `Command::new("pbcopy")` at `:5442`
both spawn subprocesses with the **inherited** environment rather than through the core launcher
that contains child roots. Neither is test-reachable today, and no guard prevents one becoming so.
This plan does not cover them — its `files_modified` are the fixture helper, the observer and CI, and
the fix belongs with the child-environment containment work, not with a test-ownership migration.
WINDOWS entry 6 stays open for it.

## Gates (real numbers, run 2026-09-15)

Every number below is from an actual run with the exit status read directly.

| Gate | Command | Result |
|------|---------|--------|
| Task 1 RED | `cargo test -p bauded --bins manager::tests::fixture_isolation_` | exit 101 — 2 passed / 3 failed |
| Task 1 GREEN | same | exit 0 — 5 passed / 0 failed |
| Manager regression | `cargo test -p bauded --bins manager::` | 47 passed / 0 failed (from 8/34 before) |
| Observer self-test | `assert-real-roots-untouched.sh --self-test` | exit 0 — **29 checks, 0 failures** |
| Narrow filter | `cargo test -p baude --bins worker_isolation_app_` | exit 0 — 1 passed |
| Narrow filter | `cargo test -p baude --bins ui_fixture_isolation_` | exit 0 — 2 passed, **0 ignored** |
| Narrow filter | `cargo test -p baude-core --lib worker_isolation_pty_` | exit 0 — 2 passed |
| Observer `before` | `assert-real-roots-untouched.sh before` | exit 0 |
| **Full serial suite** | `cargo test -- --test-threads=1` | **exit 0 — 497 passed / 0 failed / 0 ignored** (baude 74, baude-core 332, bauded 91, doc-tests 0) |
| Observer `after` | `assert-real-roots-untouched.sh after` | exit 0 — PASS: all three roots untouched |
| Unserialized bracket | `before` / `cargo test -p baude --bins` + `cargo test -p bauded --bins` / `after` | 0 / 0 + 0 / 0 — 74 + 91 passed |
| clippy | `cargo clippy --all-targets -- -D warnings` | exit 0 |
| fmt | `cargo fmt --check` | exit 0 |
| release | `cargo build --workspace --release --locked` | exit 0 |
| release graph | `cargo tree -p baude -p bauded -e normal,build --format "{p} {f}"` | `baude-core` selects **no** features — `test-support` is dev-only |

## User Setup Required

None.

## Next Phase Readiness

Plan 08-07 (the `baude worktrees` CLI preview surface) is unblocked: it is the last TISO-04 holder,
and the suite it will extend is green with an observer bracketing it. Nothing in this plan ran a
prune, and the historical leak candidates are untouched.

## Self-Check: PASSED

- `scripts/assert-real-roots-untouched.sh` — FOUND
- `.planning/phases/08-test-isolation-and-fixture-ownership/08-06-SUMMARY.md` — FOUND
- Commits `26560d8`, `6003652`, `f277afc`, `54e2c68`, `bb3ccf0`, `3f98851`, `2ad1e61`, `fb849b5`,
  `5777f75` — all FOUND in `git log`
- `git rev-list --count 8a92712..HEAD` = **9**, matching `commits:` in the frontmatter
