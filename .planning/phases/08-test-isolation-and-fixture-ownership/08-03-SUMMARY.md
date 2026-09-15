---
phase: 08-test-isolation-and-fixture-ownership
plan: 03
subsystem: testing
tags: [rust, test-isolation, workspace-identity, thread-local, fixture-ownership, concurrency, tdd]

# Dependency graph
requires:
  - phase: 08
    plan: 01
    provides: "TestRedirect (incl. the declared-but-unused workspace field), workspace_override(), replace_workspace_override(), assert_contained, and the cross-crate test-support feature"
provides:
  - "workspace::override_for_test(&Config, hint) -> TestRedirect — a per-fixture &'static Workspace identity held on the calling thread"
  - "Reader-only workspace::active() under test-support: it consults the thread-local override and panics on provenance, never on containment alone"
  - "workspace::initialize(&Config, hint) — config injected by parameter; neither initialize nor active() reads config from disk"
  - "persist::config_read_count_for_test() — a thread-local config-read counter usable as a zero-reads instrument"
  - "Root+identity fixture owners in every baude/src/app.rs, baude/src/ui.rs and bauded/src/api.rs App/router site (UiFixture, IsolationScope, ApiScope, extended AdmissionRepo and FixtureRepo)"
  - "App::config_for_test(&self) -> &Config — a test-only accessor for asserting which config an App actually loaded"
  - "Explicit workspace initialization at the bauded daemon entry point, ahead of its first reader"
affects: [08-04, 08-06, 08-08]

actuals:
  tokens: 16601  # chars/4 over the realized diff: `git diff 95da94a..HEAD | wc -c` = 66402
  tasks: 2
  commits: 9  # MEASURED: `git rev-list --count 95da94a41a377ba709cbde999b9694a13e1fb544..HEAD` at SUMMARY write (excludes this docs commit)
plan_head_before: 95da94a41a377ba709cbde999b9694a13e1fb544

tech-stack:
  added: []
  patterns:
    - "Thread-local override consulted ahead of a retained process-wide OnceLock: production keeps the cache, tests never reach it"
    - "Box::leak per fixture identity so active() keeps returning &'static Workspace with no call-site change; bounded by fixture count and confined to cfg(any(test, feature = \"test-support\"))"
    - "Provenance over containment: a support-build active() with no override panics even when the cache is seeded and every derived path is contained"
    - "Acquisition order root-first / identity-second for locals (reverse drop), identity-field-first in owner structs (declaration drop) — the two orders are inverses and both restore identity while its own root is still installed"
    - "Fixture helpers return their owner (hierarchy_fixture -> (UiFixture, App, RepositoryKey); app() -> (ApiScope, Router)) instead of dropping it at the helper boundary"
    - "Shared helpers assert their caller owns a scope (exited_tracked_manager) rather than silently installing one"

key-files:
  created: []
  modified:
    - baude-core/src/workspace.rs
    - baude-core/src/persist.rs
    - baude-core/src/testing.rs
    - baude-core/src/git.rs
    - baude/src/main.rs
    - baude/src/app.rs
    - baude/src/ui.rs
    - bauded/src/main.rs
    - bauded/src/api.rs

key-decisions:
  - "ACTIVE: OnceLock<Workspace> is RETAINED as the production fallback rather than deleted. Production startup still seeds it once via get_or_init; only the support build routes through the thread-local. Deleting it would have changed production identity lifetime, which this plan's success criteria do not ask for."
  - "Each fixture identity is Box::leak'd. D-07 fixes active()'s return type at &'static Workspace, so a per-fixture identity has to outlive the fixture in the type system. The leak is bounded by fixture count, never runs in a production build (cfg(any(test, feature = \"test-support\"))), and is the accepted disposition of threat T-08-07."
  - "Support-build active() panics on PROVENANCE, not containment (D-08). It panics with IDENTITY_ESCAPE_PANIC_MARKER when no override is held even if ACTIVE is populated and every path it would derive is contained — a contained-but-inherited identity is exactly the cross-fixture bleed this plan exists to stop, and containment alone cannot detect it."
  - "admission_repo installs a LITERAL per-fixture workspace name (= the fixture name), giving real path separation between concurrent app fixtures; initialized_repo keeps the literal \"claude\" because several API tests pair their state through workspace::resolve(Some(\"claude\")) + persist_at_for_test and would desynchronize under a per-fixture name."
  - "The TUI's early dispatch arms (statusline, hook, permission-mcp, --version, --help) are deliberately left uninitialized. grep confirms bridge.rs, hook.rs and permission.rs contain zero workspace::/backend::/persist::/active() references, so initializing them would add a config read to Claude Code's critical path for no reader."
  - "The two ui_fixture_isolation_ regressions are committed #[ignore]d. They construct an App, which starts a UsagePoller that is not contained until plan 08-08; running them now would violate the phase's hard constraint against touching a real user root. The plan's own <verification> already assigns them to 08-08."
  - "The five unowned lifecycle::tests::* containment failures were DEFERRED, not fixed. They were measured identical at this plan's RED commit (f6a3b1c), and baude-core/src/lifecycle.rs appears in no phase-08 plan's files_modified."

patterns-established:
  - "Pattern: a process-wide OnceLock is bypassed for tests by a thread-local checked first, not deleted — production lifetime is preserved while tests get per-fixture identity"
  - "Pattern: isolation guards assert PROVENANCE (who installed this identity) rather than only CONTAINMENT (where its paths point)"
  - "Pattern: a fixture helper that constructs an App returns its RAII owner to the caller; dropping the owner at the helper boundary silently un-isolates every line after the call"
  - "Pattern: struct-field drop order (declaration order) is the exact inverse of local drop order (reverse declaration), so an owner struct declares identity BEFORE root while a test body acquires root BEFORE identity"

requirements-completed: [TISO-01, TISO-02, TISO-03]

coverage:
  - id: D1
    description: "Two fixtures running concurrently on different threads each resolve their own workspace identity and neither observes the other's (D-05)"
    requirement: TISO-02
    verification:
      - kind: unit
        ref: "baude-core/src/workspace.rs#workspace::tests::concurrent_fixtures_resolve_independent_identities"
        status: pass
      - kind: unit
        ref: "baude/src/app.rs#app::tests::fixture_identity_isolation::concurrent_admission_fixtures_keep_independent_identities"
        status: pass
      - kind: unit
        ref: "bauded/src/api.rs#api::tests::fixture_identity_isolation::concurrent_api_fixtures_keep_independent_identities"
        status: pass
    human_judgment: false
  - id: D2
    description: "workspace::active() keeps its signature and &'static Workspace return type, so no call site changed (D-07)"
    requirement: TISO-02
    verification:
      - kind: other
        ref: "git diff -U0 95da94a..HEAD -- baude/src/app.rs bauded/src/api.rs baude/src/ui.rs => no active() call site appears in the diff; the only production signature change in the whole plan is initialize(), and its single TUI call site is baude/src/main.rs:299"
        status: pass
      - kind: build
        ref: "cargo check --workspace --all-targets --locked"
        status: pass
    human_judgment: false
  - id: D3
    description: "A test reaching active() without a workspace override panics even if the production cache has been seeded and its paths are contained (D-08)"
    requirement: TISO-03
    verification:
      - kind: unit
        ref: "baude-core/src/workspace.rs#workspace::tests::active_without_an_override_panics_even_when_contained"
        status: pass
      - kind: unit
        ref: "baude-core/src/workspace.rs#workspace::tests::a_seeded_cache_is_not_observable_without_an_override (re-exec'd child)"
        status: pass
      - kind: unit
        ref: "baude/src/app.rs#app::tests::fixture_identity_isolation::an_override_free_probe_cannot_inherit_a_fixture_identity"
        status: pass
      - kind: unit
        ref: "bauded/src/api.rs#api::tests::fixture_identity_isolation::an_override_free_probe_cannot_inherit_a_fixture_identity"
        status: pass
    human_judgment: false
  - id: D4
    description: "initialize(&config, hint) and every arm of reader-only active() perform zero config reads (D-06, D-07)"
    requirement: TISO-02
    verification:
      - kind: unit
        ref: "baude-core/src/workspace.rs#workspace::tests::initialize_uses_injected_config — same-thread delta on persist::config_read_count_for_test() with a positive control"
        status: pass
      - kind: other
        ref: "grep -n 'load_config' baude-core/src/workspace.rs => no occurrence in initialize() or active()"
        status: pass
    human_judgment: false
  - id: D5
    description: "The managed worktree path composition is unchanged by per-fixture identity (T-08-11)"
    requirement: TISO-02
    verification:
      - kind: unit
        ref: "baude-core/src/workspace.rs#workspace::tests::managed_worktree_path_composition_is_unchanged"
        status: pass
      - kind: unit
        ref: "baude-core/src/git.rs#git::tests::lifecycle::creation_safety::durable_keys_not_labels_supply_bounded_path_identity (migrated to owned root + identity)"
        status: pass
    human_judgment: false
  - id: D6
    description: "Every App fixture — including UI rendering and helper-returned Apps — holds root and literal-identity isolation from before construction through final caller use (D-01, D-05, D-08)"
    requirement: TISO-01
    verification:
      - kind: unit
        ref: "baude/src/app.rs — 39 App::new sites, all under an owner (AdmissionRepo, removal_app, or isolation_scope); cargo test -p baude --bins fixture_identity_isolation exit 0"
        status: pass
      - kind: unit
        ref: "baude/src/app.rs#app::tests::fixture_identity_isolation::dropping_a_fixture_owner_restores_the_enclosing_identity"
        status: pass
      - kind: other
        ref: "baude/src/ui.rs — 5 App sites, all owned by a UiFixture; hierarchy_fixture returns (UiFixture, App, RepositoryKey) so the owner outlives the helper"
        status: pass
      - kind: unit
        ref: "baude/src/ui.rs#ui::tests::ui_fixture_isolation_after_helper_return, ui::tests::ui_fixture_isolation_nested_restore"
        status: deferred
    human_judgment: false
  - id: D7
    description: "Production startup resolves identity explicitly, once, from a config loaded once, ahead of the first reader — in both binaries"
    requirement: TISO-02
    verification:
      - kind: other
        ref: "baude/src/main.rs:299 — workspace::initialize(&config, plan.hint.as_deref()) on the already-loaded config"
        status: pass
      - kind: other
        ref: "bauded/src/main.rs:181-182 — load_config() + initialize(&config, None) after bind resolution, before Manager::new / default_claude_cmd / restore / the poll thread / every handler"
        status: pass
      - kind: build
        ref: "cargo check --workspace --all-targets --locked"
        status: pass
    human_judgment: false

# Metrics
duration: 45min
completed: 2026-09-15
status: complete
---

# Phase 08 Plan 03: Per-Fixture Workspace Identity Summary

**The last process-wide cache in the isolation path is bypassed for tests: `workspace::active()` now resolves a `&'static Workspace` from a thread-local override installed by the fixture that owns it, so concurrent fixtures cannot share, race, or inherit an identity seeded from the developer's real environment — and every `App` site in `app.rs`, `ui.rs` and `api.rs` now owns both a root and an identity.**

## Performance

- **Duration:** ~45 min (first task commit 2026-09-15T14:13:52Z → close-out 2026-09-15T14:46Z is 32 min of committed work; plan load, the owner census and the pre-existing-failure measurement preceded it)
- **Tasks:** 2 of 2
- **Files modified:** 9 (the plan's 8 `files_modified` plus `baude-core/src/testing.rs`, whose `replace_workspace_override` had to be un-dead-coded)
- **Commits:** 9 (measured: `git rev-list --count 95da94a..HEAD` — 3 RED + 6 GREEN; this docs commit is additional)

## Accomplishments

- **Identity became per-fixture without changing `active()`.** A thread-local
  `&'static Workspace` override is consulted *before* `ACTIVE: OnceLock<Workspace>`, which is
  retained as the production fallback. `active()` keeps its signature and its `&'static`
  return type (D-07), so not one of its call sites changed — the only production signature
  change in the entire plan is `initialize`, whose single TUI caller is one line.
- **Provenance is enforced, not just containment.** Under `cfg(any(test, feature =
  "test-support"))`, `active()` panics with `IDENTITY_ESCAPE_PANIC_MARKER` when no override is
  held — *even when* `ACTIVE` is populated and every path it would derive lives inside a
  fixture root (D-08). A contained-but-inherited identity is precisely the cross-fixture bleed
  this plan exists to stop, and no containment check can see it. Four tests across three
  crates pin that behavior, including a re-exec'd child that proves a seeded cache is not
  observable from an override-free thread.
- **Concurrency is proven, not asserted.** `concurrent_fixtures_resolve_independent_identities`
  (core) plus `concurrent_admission_fixtures_keep_independent_identities` (app) and its API
  twin run two threads through an `Arc<Barrier>` so both identities are installed
  simultaneously, then check each thread still sees its own. A thread-local that was
  accidentally a `static mut` would fail these; a sequential test would not.
- **Config is injected, and the zero-reads claim is instrumented.** `initialize(&Config, hint)`
  takes config by parameter and neither it nor `active()` touches disk (D-06). The claim is
  measured rather than reviewed: a thread-local `CONFIG_READS` counter in `persist.rs`, read
  through `config_read_count_for_test()`, is sampled as a same-thread delta with a positive
  control so a counter that never increments cannot pass the test by being broken.
- **Both binaries now resolve identity explicitly, ahead of their first reader.** The TUI
  passes its already-loaded config and folder hint; `bauded` loads config once after bind
  resolution and initializes before `default_claude_cmd`, `Manager::new`, `restore`, the
  metadata poll thread and every handler. Precedence is unchanged — `initialize` still
  consults `BAUDE_WORKSPACE`/`BAUDE_BACKEND` ahead of config, exactly as the lazy resolution
  did.
- **Every `App` construction site gained an owner.** 39 sites in `app.rs`, 5 in `ui.rs`, plus
  the API router and manager helpers: 44 test sites in total, each holding a root guard and a
  literal identity guard from before construction through final caller use. `hierarchy_fixture`
  and `app()` now *return* their owners, because dropping an owner at the helper boundary
  un-isolates every line after the call — the failure mode that is invisible until a test
  writes somewhere real.

## Task Commits

| Task | Name | Commit | Files |
| ---- | ---- | ------ | ----- |
| 1 (RED) | Failing per-fixture workspace identity guards | `f6a3b1c` | `baude-core/src/workspace.rs`, `baude-core/src/persist.rs`, `baude-core/src/testing.rs`, `baude/src/main.rs` |
| 1 (GREEN) | Resolve workspace identity per fixture ahead of the production cache | `c85fd91` | `baude-core/src/workspace.rs` |
| 1 (migration) | Give the durable-key path test its own root and identity guards | `c860e26` | `baude-core/src/git.rs` |
| 2 (RED) | Failing owner-identity isolation filters for app and API | `5265d04` | `baude/src/app.rs`, `bauded/src/api.rs` |
| 2 (GREEN) | Give app and API fixture owners their own literal identities | `35e07d9` | `baude/src/app.rs`, `bauded/src/api.rs` |
| 2 | Initialize workspace identity at the daemon entry point | `bd4c333` | `bauded/src/main.rs` |
| 2 | Give every remaining `app.rs` App site a root and identity owner | `aff04f1` | `baude/src/app.rs` |
| 2 | Own root and identity in every UI fixture | `9ba5a17` | `baude/src/ui.rs`, `baude/src/app.rs` |
| 2 | Make the API router and manager helpers demand an isolation owner | `5ca0ab0` | `bauded/src/api.rs` |

## Files Created/Modified

**Modified**

- `baude-core/src/workspace.rs` (+424 −1) — `IDENTITY_ESCAPE_PANIC_MARKER`, `leak_identity`,
  `override_for_test(&Config, hint) -> TestRedirect`; `initialize` and `active()` both
  cfg-split (production seeds/reads `ACTIVE`, support build leaks and installs/reads the
  thread-local). Seven tests added.
- `baude-core/src/persist.rs` (+21) — thread-local `CONFIG_READS` counter incremented inside
  `load_config()`, plus `config_read_count_for_test()`.
- `baude-core/src/testing.rs` (+8 −3) — `replace_workspace_override` un-dead-coded; it is the
  mutator `override_for_test` installs through.
- `baude-core/src/git.rs` (+15) — `durable_keys_not_labels_supply_bounded_path_identity`
  migrated to an owned root + literal identity (root first, identity second).
- `baude/src/main.rs` (+1 −1) — `initialize(&config, plan.hint.as_deref())`; the early
  dispatch arms are untouched by design.
- `bauded/src/main.rs` (+10) — `load_config()` + `initialize(&config, None)` before the first
  reader, with a comment recording which readers depend on it and why the hint is `None`.
- `baude/src/app.rs` (+196) — `AdmissionRepo` gained `_identity` (declared first);
  `IsolationScope`/`isolation_scope(label)` added and wired into 10 previously unguarded
  tests; the dogfood child branch gained its own identity; `config_for_test` accessor;
  `mod fixture_identity_isolation` with 3 tests.
- `baude/src/ui.rs` (+194 −1) — `UiFixture` (root, `_identity`, `_redirect`) with `new`,
  `with_config`, `root`, `config_path`; `hierarchy_fixture` returns its owner and all six
  callers destructure it; four direct sites migrated; two `ui_fixture_isolation_*`
  regressions added, `#[ignore]`d for 08-08.
- `bauded/src/api.rs` (+182 −1) — `FixtureRepo` gained `_identity` (declared first);
  `initialized_repo` delegates to `initialized_repo_in_workspace`; `ApiScope`/`api_scope`;
  `app()` returns `(ApiScope, Router)` and five callers updated;
  `exited_tracked_manager` now asserts the caller owns a scope; `mod
  fixture_identity_isolation` with 2 tests.

## Verification

Wave 2 runs the task's named owner-only identity filters plus the compile-only workspace
check. Broad `baude`/`bauded` suites, the dogfood regression and clippy belong to plan 06; the
`ui_fixture_isolation_` regressions belong to 08-08 after its worker containment. Every number
below is from a run executed in this session with its exit code captured directly (never
through a pipe).

| Gate | Command | Result |
| ---- | ------- | ------ |
| Task 1 RED | `cargo test -p baude-core --lib workspace::` | exit 101 — **14 passed, 7 failed**, 242 filtered out |
| Task 1 GREEN | `cargo test -p baude-core --lib workspace::` | exit 0 — **21 passed, 0 failed**, 242 filtered out |
| Task 1 migration | `cargo test -p baude-core --lib durable_keys_not_labels` | exit 0 — **1 passed** |
| Task 2 RED (app) | `cargo test -p baude --bins fixture_identity_isolation` | exit 101 — **1 passed, 2 failed** |
| Task 2 RED (API) | `cargo test -p bauded --bins fixture_identity_isolation` | exit 101 — **1 passed, 1 failed** |
| Task 2 verify (core) | `cargo test -p baude-core --lib workspace::` | exit 0 — **21 passed, 0 failed** |
| Task 2 verify (app) | `cargo test -p baude --bins fixture_identity_isolation` | exit 0 — **3 passed, 0 failed**, 70 filtered out |
| Task 2 verify (API) | `cargo test -p bauded --bins fixture_identity_isolation` | exit 0 — **2 passed, 0 failed**, 84 filtered out |
| UI regressions present | `cargo test -p baude --bins ui_fixture_isolation -- --list` | exit 0 — both tests listed (`#[ignore]`d for 08-08) |
| Compile | `cargo check --workspace --all-targets --locked` | exit 0 |
| Formatting | `cargo fmt --all -- --check` | exit 0 |

**Whole-crate context (not a plan gate):** `cargo test -p baude-core --lib` → exit 101,
**258 passed, 5 failed**. All five failures are `lifecycle::tests::*` containment escapes that
were measured identical at this plan's RED commit `f6a3b1c` — see *Deferred Issues* below.

The broad `-p baude` / `-p bauded` suites were deliberately **not** run. Constructing an `App`
outside the named filters starts a `UsagePoller` that is not contained until plan 08-08, and
reaching it would violate the phase's hard constraint against touching a real user root.

## TDD Gate Compliance

Both tasks are behavior-adding and both ran a full RED → GREEN cycle.

| Task | RED commit | Measured RED | Evidence verdict | GREEN commit |
| ---- | ---------- | ------------ | ---------------- | ------------ |
| 1 | `f6a3b1c` | exit 101 — 14 passed, 7 failed | intentional RED (subject absent) | `c85fd91` |
| 2 | `5265d04` | exit 101 — 1 passed, 2 failed (app) + 1 passed, 1 failed (API) | `RED_EVIDENCE_OK` (`target_test_failed`, target `app::tests::fixture_identity_isolation::concurrent_admission_fixtures_keep_independent_identities`; 5 tests, 2 pass, 3 fail) | `35e07d9` |

Both REDs were intentional — the subject behavior was absent, not mutated in. Task 1's RED
failed because `override_for_test` did not exist as a behavior: the identity installed on one
thread was not visible to `active()` at all, and the override-free probe returned the seeded
cache instead of panicking. Task 2's RED failed because `AdmissionRepo`/`FixtureRepo` held no
identity, so both fixtures resolved the same workspace and the probe inherited it.

**Why the RED commits carry scaffolding.** A pure "call a function that does not exist" RED
does not compile, and a build failure emits no TAP-parseable failing test — there is nothing
for `gsd-tools check tdd-red-evidence` to validate and no way to distinguish an intentional
RED from a typo. The RED commits therefore contain the test-support surface (signatures,
counter, the un-dead-coded mutator) so the binary builds and the assertions fail *on behavior*.
Every behavioral line lives in the GREEN commit.

## Decisions Made

- **The `OnceLock` stays.** It is retained as the production fallback rather than deleted.
  Production startup still seeds it exactly once; only the support build routes through the
  thread-local. Deleting it would have changed production identity lifetime, which none of
  the plan's success criteria ask for, in a commit whose stated purpose is test isolation.
- **`Box::leak` is the price of `&'static`.** D-07 fixes `active()`'s return type, so a
  per-fixture identity must outlive its fixture in the type system. The leak is bounded by
  fixture count, is confined to `cfg(any(test, feature = "test-support"))`, and never runs in
  a production build. This is threat T-08-07's accepted disposition, recorded here so the
  next reader does not "fix" it into a dangling reference.
- **Drop order is the whole design, and it is asymmetric.** Locals drop in *reverse*
  declaration order; struct fields drop in *declaration* order. So a test body acquires root
  first and identity second, while an owner struct declares `_identity` *before* `_redirect`.
  Both spellings mean the same thing: the identity is restored while the root it was resolved
  under is still installed. Getting either backwards restores an identity into a
  already-reverted root, which fails loudly but confusingly.
- **Per-fixture literal names in `app.rs`, `"claude"` in `api.rs`.** `admission_repo` installs
  the fixture's own name as the workspace, which gives genuine path separation between
  concurrent app fixtures (and `app.rs` contains no hardcoded `"claude"`). `initialized_repo`
  keeps the literal `"claude"` because several API tests pair their persisted state through
  `workspace::resolve(Some("claude"))` + `persist_at_for_test`; a per-fixture name there would
  desynchronize the pair without adding isolation the root guard does not already provide.
- **`exited_tracked_manager` asserts rather than installs.** A shared helper that quietly
  installs its own scope would let a caller pass while owning nothing, and the next caller
  would inherit whichever scope happened to still be live. It now fails with a message naming
  the fix ("bind an `initialized_repo` (or `api_scope`) owner before calling it").
- **The TUI's early arms stay uninitialized.** `statusline` → `bridge::run`, `hook` →
  `hook::dispatch_hook`, `permission-mcp` → `permission::run_permission_mcp`, plus
  `--version`/`--help`, all return before the config load. `grep` confirms `bridge.rs`,
  `hook.rs` and `permission.rs` contain zero `workspace::`/`backend::`/`persist::`/`active()`
  references — initializing them would add a config read to Claude Code's critical path for no
  reader.

### Early-arm dispositions (`baude` and `bauded`)

| Arm | Dispatches to | Identity readers reached | Disposition |
| --- | ------------- | ------------------------ | ----------- |
| `statusline` | `bridge::run` | none (`grep` → 0 hits) | not initialized |
| `hook` | `hook::dispatch_hook` | none (`grep` → 0 hits) | not initialized |
| `permission-mcp` | `permission::run_permission_mcp` | none (`grep` → 0 hits) | not initialized |
| `--version` / `-V` | prints and returns | none | not initialized |
| `--help` / `-h` | prints and returns | none | not initialized |
| main TUI path | `App::new` and everything after | many | `initialize(&config, plan.hint)` at `baude/src/main.rs:299` |
| `bauded` main path | `Manager::new`, `restore`, poll thread, handlers | many | `initialize(&config, None)` at `bauded/src/main.rs:182` |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `initialize`'s signature change broke `baude/src/main.rs:299`**

- **Found during:** Task 1 RED
- **Issue:** Changing `initialize` to take `&Config` by parameter (D-06) made its single
  existing call site a compile error, which would have prevented the RED binary from building
  at all — and with it, any TAP-parseable failing test.
- **Fix:** Updated the call site to
  `baude_core::workspace::initialize(&config, plan.hint.as_deref())`, passing the config the
  TUI had already loaded. This is the same one line Task 2 would otherwise have written; it is
  behavior-preserving (the config and hint are identical to what the lazy path derived).
- **Files modified:** `baude/src/main.rs`
- **Commit:** `f6a3b1c`

**2. [Rule 3 - Blocking] `replace_workspace_override` was dead code in plan 01's `testing.rs`**

- **Found during:** Task 1 GREEN
- **Issue:** Plan 01 declared the `Redirects.workspace` field and the `workspace_override()`
  reader, and the plan's `<interface_from_prior_plan>` states "`testing.rs` does not need
  editing" — but the mutator this plan installs through was `#[allow(dead_code)]`d and not
  reachable from `workspace.rs`.
- **Fix:** Un-dead-coded `replace_workspace_override` and made it visible to the workspace
  module. `baude-core/src/testing.rs` is consequently a 9th modified file, one beyond the
  plan's `files_modified` set.
- **Files modified:** `baude-core/src/testing.rs`
- **Commit:** `f6a3b1c`

### Planned-but-adjusted

**3. The two `ui_fixture_isolation_*` regressions are committed `#[ignore]`d**

The plan asks for both tests by name and separately states that they "are exercised by 08-08
after its worker containment, not by this wave's filters" — but it does not say by what
mechanism they are kept out of this wave's runs. Both construct an `App`, and an `App`
constructs a `UsagePoller` that is not contained until plan 08-08. Running them here would
reach a real user root, which hard constraint 1 forbids. They therefore carry
`#[ignore = "runs an App (and its UsagePoller); executed by plan 08-08 task 1"]`. Their
presence and their names are verified by `--list` (exit 0, both listed); 08-08 task 1 removes
the attribute. This is the only `#[ignore]` this plan introduces.

**4. RED commits carry test-support scaffolding**

Documented under *TDD Gate Compliance* above: a missing subject alone yields a build failure,
not a failing test, so the RED commits include the signatures and accessors needed to compile
while every behavioral line stays in GREEN.

## Deferred Issues

**Five `lifecycle::tests::*` containment failures — deferred to plan 08-06.** Logged in
`.planning/phases/08-test-isolation-and-fixture-ownership/deferred-items.md`.

- `blocked_activation_retry_round_trips_all_provenance`
- `occupied_protected_checkout_blocks_activation_recovery_merge`
- `occupied_protected_checkout_refuses_activation_overwrite`
- `pending_activation_recovery_distinguishes_absent_and_exact_git_facts`
- `post_verification_compensation_failure_records_typed_recovery_child`

Each panics in plan 01's `assert_contained` because `baude-core/src/lifecycle.rs`'s test module
holds zero `TestRedirect`s. **They are pre-existing, and that was measured, not assumed:** the
identical set was reproduced at this plan's RED commit `f6a3b1c` — `cargo test -p baude-core
--lib lifecycle` → exit 101, 41 passed / 6 failed. The sixth failure there
(`git::tests::lifecycle::creation_safety::durable_keys_not_labels_supply_bounded_path_identity`)
*was* in this plan's file set and is fixed in `c860e26`.

They were not fixed here because `lifecycle.rs` appears in no phase-08 plan's `files_modified`
(08-01 through 08-08), so fixing it would be unbudgeted scope expansion into a file another
plan may still restructure. Containment status is **SAFE**: `assert_contained` panics rather
than letting the resolution through, so these are resolution-only escapes — no file under a
real user root is read or written. Suggested owner is plan 08-06, which already owns the
cross-cutting `scripts/assert-real-roots-untouched.sh` CI gate and cannot go green while these
five fail.

## Issues Encountered

- **An over-broad owner regex hid four unguarded tests.** The first census counted
  `context_fixture` as an owner, but it only builds a `RepositoryState` and installs nothing.
  Narrowing the pattern to `admission_repo|removal_app\(|TestRedirect|override_for_test|
  isolation_scope` surfaced 14 gap sites across 11 functions. Any future audit of fixture
  ownership should enumerate the *installing* constructs explicitly rather than matching
  anything that looks like a fixture helper.
- **Verifying "pre-existing" required restoring the RED state without `git stash`.** The
  phase's constraints forbid branch switches, and `git stash` is a shared-ref hazard. The
  committed RED versions of `workspace.rs`/`testing.rs` were written into the working tree
  from `git show`, the lifecycle filter re-run, and the GREEN copies (kept in the session
  scratchpad) restored afterwards — a measurement with no ref mutation.
- **The plan ledger at `.git/gsd-plan-head-before-08-03` was stale**, holding plan 02's base
  (`8730fbb`). It was overwritten with this plan's real base (`95da94a`), and `commits: 9` was
  measured from the corrected value.

## Known Stubs

None. No `TODO`, `FIXME`, `todo!(...)`, or `unimplemented!(...)` was introduced. The two
`#[ignore]`d `ui_fixture_isolation_*` tests are complete, fully written regressions gated on
plan 08-08's worker containment, not stubs — they are recorded in the broken-windows ledger as
`unrun-verify` and 08-08 task 1 removes the attribute.

## Threat Flags

None new. `T-08-07` (`Box::leak` of fixture identities) is the plan's own accepted threat and
is implemented as dispositioned: leaked identities are bounded by fixture count and confined
to `cfg(any(test, feature = "test-support"))`, so no production build allocates one. No
network endpoint, auth path, file-access pattern, or trust-boundary schema was introduced.

## Safety

No file under a real user root was read, written, created, or removed during this execution.

- Every fixture root is synthetic (`/nonexistent/...`) or under `std::env::temp_dir()` with a
  pid+sequence suffix, so concurrent fixtures cannot collide.
- Post-run mtime check: `~/.local/share/baude` (2026-06-11), its
  `worktrees/{claude,opencode,prerelease}` children, and `~/.config/baude` (06:54, before this
  session's first edit) are all unchanged.
- The broad `-p baude` / `-p bauded` suites were not run precisely because an uncontained
  `UsagePoller` would reach a real root before plan 08-08 lands.
- The five deferred `lifecycle` failures are resolution-only escapes caught by a panic — no
  real-root I/O occurred in them either.
- git stayed on SSH; no push, no branch switch, no PR, no `git stash`, no `git clean`. The
  untracked `.gsd/` and `.planning/milestone.lock` were left untouched.

## User Setup Required

None. No package was installed and no external service configuration is required.

## Next Phase Readiness

- **Plan 08-08** (usage worker isolation) is now unblocked on the identity side and inherits a
  concrete first task: remove the `#[ignore]` from `ui::tests::ui_fixture_isolation_after_helper_return`
  and `ui::tests::ui_fixture_isolation_nested_restore` and run them.
- **Plan 08-06** (integration acceptance / CI gate) inherits the five `lifecycle.rs` fixtures
  as documented deferred work; it cannot go green until they hold a root and a literal
  identity, same pattern as `c860e26`.
- **Any new test that constructs an `App`, a router, or reaches `workspace::active()`** must
  now own both a root and an identity. Forgetting either fails fast with a named marker rather
  than silently writing somewhere real — that is the point.

## Requirements Bookkeeping

`requirements-completed` above copies this plan's `requirements:` frontmatter, as the SUMMARY
contract requires. **REQUIREMENTS.md was deliberately NOT checked off.** TISO-01, TISO-02 and
TISO-03 are phase-wide requirements that this plan advances substantially but does not finish —
the standing STATE.md blocker from plans 01 and 02 says not to mark them complete before plan
06 lands the remaining consumer migrations, and five `lifecycle.rs` fixtures plus plan 08-08's
worker containment are still outstanding. Checking them off after 3 of 8 plans would be a false
completion claim.

## Self-Check: PASSED

All nine modified files exist on disk, and all nine task commits (`f6a3b1c`, `c85fd91`,
`c860e26`, `5265d04`, `35e07d9`, `bd4c333`, `aff04f1`, `9ba5a17`, `5ca0ab0`) resolve in
`git log`. `commits: 9` was re-measured from the corrected ledger base at SUMMARY write:
`git rev-list --count 95da94a41a377ba709cbde999b9694a13e1fb544..HEAD` → 9.

---
*Phase: 08-test-isolation-and-fixture-ownership*
*Completed: 2026-09-15*
