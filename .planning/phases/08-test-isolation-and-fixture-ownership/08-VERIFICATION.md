---
phase: 08-test-isolation-and-fixture-ownership
verified: 2026-09-15T20:11:43Z
status: human_needed
score: 4/4 must-haves verified
covered_files:
  - ".github/workflows/ci.yml"
  - ".planning/REQUIREMENTS.md"
  - ".planning/phases/08-test-isolation-and-fixture-ownership/08-01-PLAN.md"
  - ".planning/phases/08-test-isolation-and-fixture-ownership/08-01-SUMMARY.md"
  - ".planning/phases/08-test-isolation-and-fixture-ownership/08-02-PLAN.md"
  - ".planning/phases/08-test-isolation-and-fixture-ownership/08-02-SUMMARY.md"
  - ".planning/phases/08-test-isolation-and-fixture-ownership/08-03-PLAN.md"
  - ".planning/phases/08-test-isolation-and-fixture-ownership/08-03-SUMMARY.md"
  - ".planning/phases/08-test-isolation-and-fixture-ownership/08-04-PLAN.md"
  - ".planning/phases/08-test-isolation-and-fixture-ownership/08-04-SUMMARY.md"
  - ".planning/phases/08-test-isolation-and-fixture-ownership/08-05-PLAN.md"
  - ".planning/phases/08-test-isolation-and-fixture-ownership/08-05-SUMMARY.md"
  - ".planning/phases/08-test-isolation-and-fixture-ownership/08-06-PLAN.md"
  - ".planning/phases/08-test-isolation-and-fixture-ownership/08-06-SUMMARY.md"
  - ".planning/phases/08-test-isolation-and-fixture-ownership/08-07-PLAN.md"
  - ".planning/phases/08-test-isolation-and-fixture-ownership/08-07-SUMMARY.md"
  - ".planning/phases/08-test-isolation-and-fixture-ownership/08-08-PLAN.md"
  - ".planning/phases/08-test-isolation-and-fixture-ownership/08-08-SUMMARY.md"
  - "baude-core/Cargo.toml"
  - "baude-core/src/git.rs"
  - "baude-core/src/hook.rs"
  - "baude-core/src/lib.rs"
  - "baude-core/src/lifecycle.rs"
  - "baude-core/src/meta.rs"
  - "baude-core/src/persist.rs"
  - "baude-core/src/pty.rs"
  - "baude-core/src/testing.rs"
  - "baude-core/src/workspace.rs"
  - "baude-core/src/worktree_scan.rs"
  - "baude/Cargo.toml"
  - "baude/src/app.rs"
  - "baude/src/main.rs"
  - "baude/src/ui.rs"
  - "baude/src/usage.rs"
  - "bauded/Cargo.toml"
  - "bauded/src/api.rs"
  - "bauded/src/main.rs"
  - "bauded/src/manager.rs"
  - "bauded/src/push.rs"
  - "scripts/assert-real-roots-untouched.sh"
covered_digest: "v1:sha256:b4d04dcd1541abd269796a81eb247bf56b6835476a2d8ab264fce0cd667190c7"
behavior_unverified: 0
overrides_applied: 0
human_verification:
  - test: "Run the `before` / `cargo test -- --test-threads=1` / `after` triple from `scripts/assert-real-roots-untouched.sh` on this machine, against the DEVELOPER'S REAL roots."
    expected: "before exit 0, suite exit 0, after exit 0 — all three real roots (`~/.config/baude`, `~/.claude`, `~/.local/share/baude/worktrees`) byte-identical across the suite run."
    why_human: "The verifier is under a hard constraint forbidding it to read, write, observe or fingerprint the developer's real roots — which is precisely what this observer does. The verifier's substitute run (below) pointed XDG_CONFIG_HOME / XDG_DATA_HOME / CLAUDE_CONFIG_DIR at scratchpad stand-ins; that proves the suite writes nothing through the real-root resolver chain, but it does not exercise the `dirs::home_dir()` fallback that applies when those variables are unset, and CI's green signal is weaker than a local one because CI runners have no populated real roots. VALIDATION.md declares this manual-only for the same reason."
  - test: "Run `cargo run -q -p baude -- worktrees scan` against the developer's real ~1433-entry managed-worktree root. No `--prune`, no `--yes`."
    expected: "A grouped summary prints; exit 0; nothing is removed (`find ~/.local/share/baude/worktrees -mindepth 2 -maxdepth 2` identical before and after); no directory holding a real checkout is reported removable."
    why_human: "Same hard constraint — the verifier may not read the real data root, and a read-only scan of it was explicitly not required. The dataset (the two live `claude` checkouts that must not clear) exists only on this machine and cannot be committed as a fixture. 08-07-SUMMARY.md records the executor having run exactly this (1433 candidates: 0 removable, 150 live, 1283 indeterminate; state inventory INCOMPLETE), which is an attested result, not verifier-observed evidence."
deferred:
  - truth: "`App::open_editor` (baude/src/app.rs:5050) spawns the developer's configured editor with an inherited environment, outside every redirect."
    addressed_in: "Not scheduled — carried in deferred-items.md"
    evidence: "Not reachable from any test today: the only callers are `open_editor_for_selection` (app.rs:3981, key handler) and `SidebarAction::Editor` dispatch (app.rs:4315); no test in the harness invokes either. Confirmed by grep — the four `open_editor` references in app.rs are the definition, the two production callers, and the internal call. Outside every phase-08 plan's `must_haves`; it touches neither `~/.config/baude` nor `~/.claude`, so it falsifies no Success Criterion."
---

# Phase 8: Test Isolation and Fixture Ownership — Verification Report

**Phase Goal:** Running the suite cannot read or write the developer's real config, state, or `~/.claude`, and suspected historical leaks can be inspected before anyone deletes anything.

**Verified:** 2026-09-15T20:11:43Z
**Status:** human_needed
**Re-verification:** No — initial verification

## Evidence Basis

Every behavioral claim below traces to ONE full workspace suite run the verifier
executed itself:

```
env XDG_CONFIG_HOME=<scratch>/roots/config \
    XDG_DATA_HOME=<scratch>/roots/data \
    CLAUDE_CONFIG_DIR=<scratch>/roots/claude \
    cargo test --workspace -- --test-threads=1
```

| Target | Result |
|--------|--------|
| `baude` unittests (`src/main.rs`) | 102 passed; 0 failed; 0 ignored |
| `baude-core` unittests (`src/lib.rs`) | 333 passed; 0 failed; 0 ignored |
| `bauded` unittests (`src/main.rs`) | 92 passed; 0 failed; 0 ignored |
| Doc-tests `baude_core` | 0 passed; 0 failed |
| **Total** | **527 passed; 0 failed; 0 ignored — exit 0** |

(One additional `1 passed; 101 filtered out` line is the re-exec'd dogfood/PTY
child running a single filtered case inside the `baude` binary, not a fourth
target.)

The three XDG variables were redirected at scratchpad stand-ins so that ANY
escape past the guard would land in the scratchpad rather than the developer's
real roots — the same containment the 08-06 rehearsal used. This is the only way
to obtain behavioral evidence under the standing constraint that the verifier
must never touch a real user root.

**The stand-in roots were empty after the run.** `find <scratch>/roots -mindepth 1`
returned exactly the three directories created before the run and zero children.
A 527-test serial suite wrote nothing whatsoever through the real-root resolver
chain.

The verifier ran no `--prune`, read no real user root, switched no branch, pushed
nothing, and modified no source. `.gsd/` and `.planning/milestone.lock` remain
untracked and untouched.

## Goal Achievement

### Observable Truths (ROADMAP Success Criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Config resolution (`persist::config_dir`, `meta::claude_config_dir`) accepts a test redirect, and no test run reads or writes the real `~/.config/baude` or `~/.claude` — including `bauded` push-subscription and VAPID key storage | ✓ VERIFIED | `persist.rs:866-881` and `meta.rs:50-59` each check the thread-local override first, then `assert_contained` the real value. State files derive from the same `config_base()` (`persist.rs:540, 1054, 1085, 1101, 1107, 1111`), so one redirect covers config + state + locks. `push.rs`'s private `config_base()` is gone — both stores now resolve `baude_core::persist::config_dir()` (`push.rs:53, 190, 208`). Behavioral: 4 `store_isolation_*` and 5 `claude_config_dir_*` cases passed; stand-in roots empty after 527 tests. |
| 2 | Workspace identity is resolvable per fixture rather than through a process-wide `OnceLock` seeded from the developer's real environment, so concurrent fixtures cannot share or race one identity | ✓ VERIFIED | `workspace.rs:297-302` — the support-build `initialize` writes only the thread's `TestRedirect` scope and **never** `ACTIVE`; `workspace.rs:326-334` — support-build `active()` reads the override or panics, never the cache. Behavioral: `concurrent_fixtures_resolve_independent_identities` (two barrier-synchronised threads, both identities live at once, each observing only its own, asserted through `managed_default_worktree_path`) passed; `initialize_uses_injected_config` passed with a positive-control read counter proving zero config reads AND `ACTIVE.get().is_none()`. |
| 3 | The escape guard covers config, state, and `~/.claude` paths as it already covers managed worktrees, and is armed independently of whether some earlier fixture in the same test binary happened to arm it | ✓ VERIFIED | Four guarded resolvers call `assert_contained`: managed worktrees (`git.rs:1772`), config/state (`persist.rs:873`), `~/.claude` (`meta.rs:57`), home (`persist.rs:921`). Arming is structural, not stateful: `assert_contained` asks "did this land inside the fixture root?", with no arming flag anywhere — the old `REQUIRE_WORKTREES_OVERRIDE` / `set_*_for_test` setters return **0** grep hits repo-wide. Behavioral: `unguarded_resolution_panics` passed in ALL THREE test binaries — `testing::tests` + `meta::tests::claude_config_dir_unguarded_resolution_panics` (baude-core), `app::tests` (baude), `manager::tests` (bauded). |
| 4 | A developer can enumerate and preview suspected leaked test worktrees under the real data root without deleting them; removal requires verified ownership plus separate approval, and a missing gitdir alone never authorizes it | ✓ VERIFIED | `worktree_scan::classify` (`worktree_scan.rs:213-262`) consults blockers first, then requires `ShapeMatch` AND `NotReferencedByState` AND a `ClearingSignal`; `ClearingSignal` (`:174-178`) admits exactly `Empty` and `GitDisownsIt` — `NoGitdir` is not a member. `prune_at` (`:1276-1386`) re-derives every fact through `scan_at` and requires the fresh proof to equal the approved one. CLI needs `--prune` AND `--report <file>` AND `--yes` (`main.rs:570-613`). Behavioral: 20 `worktrees_cli_tests::*` cases passed, including `prune_acts_on_the_saved_report_rather_than_a_fresh_scan` and `prune_without_yes_re_verifies_and_removes_nothing`. |

**Score:** 4/4 truths verified (0 present, behavior-unverified)

### Plan-Level Must-Haves

| Plan | Must-have truth (abridged) | Status | Evidence |
|------|---------------------------|--------|----------|
| 08-01 | Unredirected, uncontained resolution fails the test run | ✓ | `unguarded_resolution_panics` ×4 (one per test binary + the meta variant) |
| 08-01 | Same failure on the FIRST test in a binary, no earlier fixture | ✓ | `testing.rs:318-323` — declared first in the module, constructs no `TestRedirect`; containment is a property of the compiled binary, not of execution history |
| 08-01 | One guard value redirects config + state + worktrees + hook together | ✓ | `Redirects` (`testing.rs:49-67`), `TestRedirect::new` (`:128-138`) derives all five from one root |
| 08-01 | Dropping the guard restores the prior resolver values | ✓ | `nested_redirects_restore_the_outer_root`, `nested_hook_command_restores_the_outer_command` both passed |
| 08-01 | A release build contains none of the guard code | ✓ | `cargo tree -p baude -e features,no-dev` → `baude-core feature "default"` only; the dev-edge graph → `baude-core feature "test-support"`. Feature declared in `baude-core/Cargo.toml:16`, enabled only from `[dev-dependencies]` in both binaries |
| 08-02 | `bauded` has exactly one config resolver, `persist::config_dir()` | ✓ | zero `dirs::home_dir()` / `XDG_CONFIG_HOME` live reads in `bauded/src`; `push.rs` config_base deleted |
| 08-03 | `active()` signature unchanged | ✓ | `workspace.rs:312 / 326` — `pub fn active() -> &'static Workspace` in both cfg arms |
| 08-03 | `active()` without an override panics even when contained | ✓ | `active_without_an_override_panics_even_when_contained` passed (root redirect held, still panics) |
| 08-03 | `initialize(&config, hint)` and `active()` perform zero config reads | ✓ | `initialize_uses_injected_config` passed with instrumented counter + positive control |
| 08-04/05 | Shape-match alone → Indeterminate; missing gitdir alone → Indeterminate | ✓ | `classify` clearing set excludes `NoGitdir`; tests at `worktree_scan.rs:1623-1649` assert both, passed in the run |
| 08-05 | Removal only with confirmation passed as an explicit parameter, no removing default | ✓ | `prune_at(roots, preview, confirmed: bool)` — `confirmed` has no default; `prune_without_yes_re_verifies_and_removes_nothing` passed |
| 08-06 | A `bauded` fixture is one construction holding the guard for its lifetime | ✓ | `ManagerFixture` (`manager.rs:2652-2721`) holds `_identity` and `_redirect` as **fields** in declaration order; **38** call sites; 7 `fixture_isolation_*` regressions passed |
| 08-07 | `--help` names the new verb | ✓ | `main.rs:229` — "subcommands: statusline, hook, permission-mcp, worktrees" |
| 08-08 | Constructing an App in a test starts no usage scanner | ✓ | `usage.rs:56-61` — `#[cfg(test)] UsagePoller::start` builds an empty snapshot; the worker loop, poll constants and every subprocess helper are `#[cfg(not(test))]`. `worker_isolation_app_*` passed |
| 08-08 | A test PTY receives only an explicit fixture-owned child environment | ✓ | `worker_isolation_pty_child_environment` passed — re-exec'd child ADVERTISES hostile HOME/XDG/CLAUDE_CONFIG_DIR/SHELL/ZDOTDIR/ENV/BASH_ENV plus executable startup sentinels and a positive control |

### Key Link Verification

| From | To | Via | Status |
|------|----|-----|--------|
| `persist::config_base()` | `testing::config_dir_override()` / `assert_contained()` | `persist.rs:868, 873` | ✓ WIRED |
| `git::worktrees_base()` | `testing::worktrees_base_override()` / `assert_contained()` | `git.rs:1767, 1772` | ✓ WIRED |
| `meta::claude_config_dir()` | `testing::claude_config_dir_override()` / `assert_contained()` | `meta.rs:52, 57` | ✓ WIRED |
| `persist::home_dir()` | `testing::home_dir_override()` / `assert_contained()` | `persist.rs:916, 921` | ✓ WIRED |
| `hook::baude_hook_command()` | `testing::hook_command_override()` | `hook.rs:90` | ✓ WIRED |
| `workspace::active()` | `testing::workspace_override()` → panic (support) / `OnceLock` (prod) | `workspace.rs:313, 327` | ✓ WIRED |
| `push::Vapid::load_or_generate` / `PushState::load` | `persist::config_dir()` | `push.rs:53, 190, 208` | ✓ WIRED |
| `baude/bauded [dev-dependencies]` | `cfg(feature = "test-support")` in baude-core | both Cargo.toml `[dev-dependencies]` | ✓ WIRED |
| `main.rs args.get(1) == "worktrees"` | `worktree_scan::scan_at` / `prune_at` | `main.rs:274-290` via `run_worktrees_at` | ✓ WIRED |
| `worktree_scan` state inventory | `persist::load_named_at` (non-locking) | `worktree_scan.rs:444` | ✓ WIRED |
| CI full-suite step | `scripts/assert-real-roots-untouched.sh` | `ci.yml:22, 29, 43` (self-test / before / `if: always()` after) | ✓ WIRED |

### Data-Flow Trace (Level 4)

| Artifact | Data | Source | Flows | Status |
|----------|------|--------|-------|--------|
| `baude worktrees scan` summary | candidate set | `git::real_worktrees_base()` + `persist::real_config_base()`, resolved by `main.rs:281-284` and handed down explicitly | yes | ✓ FLOWING |
| `ScanReport.candidates[].verdict` | evidence vectors | `contents_evidence` / git inventory / `load_named_at` state read | yes | ✓ FLOWING |
| `PruneReport.outcomes` | re-derived proof | `scan_at(roots)` called fresh inside `prune_at` (`:1343`); the saved report is comparison data only | yes | ✓ FLOWING |
| `UsagePoller::costs()` in tests | `UsageCosts` | `#[cfg(test)]` empty snapshot — **deliberately inert**, asserted by `is_inert_for_test()` (Arc strong_count == 1) | n/a by design | ✓ INTENTIONAL |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full serial suite | `cargo test --workspace -- --test-threads=1` (redirected XDG) | 527 passed / 0 failed / 0 ignored, exit 0 | ✓ PASS |
| Suite writes nothing to real-root chain | `find <scratch>/roots -mindepth 1 \| wc -l` after the run | 3 (the pre-created dirs only, zero children) | ✓ PASS |
| Guard armed in every test binary | grep `unguarded_resolution_panics` in the run log | 4 hits, all `- should panic ... ok`, in `baude`, `baude-core` (×2), `bauded` | ✓ PASS |
| Release graph excludes the guard | `cargo tree -p baude -e features,no-dev` | `baude-core feature "default"` only | ✓ PASS |
| Old ad-hoc setters removed | grep `set_worktrees_base_for_test\|set_hook_command_for_test\|REQUIRE_WORKTREES_OVERRIDE\|*_OVERRIDE` | 0 hits repo-wide | ✓ PASS |
| No unguarded home resolution in the binaries | grep `dirs::home_dir` in `baude/src`, `bauded/src` | 0 live calls (1 comment); `dirs` is not even a dependency of `baude` | ✓ PASS |
| `BAUDE_TEST_FIXTURE_ROOT` never set in-process | grep all usages | only `command.env(...)` on child `Command`s (app.rs:5743, 8248; workspace.rs:616; pty.rs:927) — no `set_var`, so the parent binary is always in the panicking branch | ✓ PASS |
| `worktrees scan` against the REAL tree | — | not run | ? SKIP → human verification |
| `assert-real-roots-untouched.sh before/after` against REAL roots | — | not run | ? SKIP → human verification |

### Probe Execution

No `scripts/*/tests/probe-*.sh` exist in this repository; the phase declares none.
`scripts/assert-real-roots-untouched.sh` is the phase's observer, and running it —
even in `--self-test` mode is safe, but `before`/`after` are not — was ruled out
by the standing constraint against observing real roots. Routed to human
verification above.

### Requirements Coverage

| Requirement | Source Plans | Description | Status | Evidence |
|-------------|--------------|-------------|--------|----------|
| TISO-01 | 08-01, 08-02, 08-03, 08-06, 08-08 | Every created repository, worktree, config and state file confined to a unique test-owned temporary root | ✓ SATISFIED | Truth 1; `ManagerFixture`/`GitFixture`/`ScanFixture` process-unique roots under `temp_dir()`; 527-test run left stand-in roots empty |
| TISO-02 | 08-03, 08-06, 08-08 | Concurrent tests without mutating the parent HOME/XDG or sharing cached workspace identity | ✓ SATISFIED | Truth 2; thread-local redirects, zero `set_var` on any root variable, `concurrent_fixtures_resolve_independent_identities` passed |
| TISO-03 | 08-01, 08-02, 08-03, 08-06, 08-08 | A failing test when a creation path escapes its fixture root, without writing the real data dir | ✓ SATISFIED | Truth 3; `assert_contained` panics **before** returning the real path, so an escape is a resolution-only failure — no write occurs |
| TISO-04 | 08-04, 08-05, 08-07 | Preview leaks without deleting; removal requires separate approval and verified ownership, never a missing gitdir alone | ✓ SATISFIED | Truth 4; `NoGitdir` excluded from `ClearingSignal`; three-flag gate; 20 CLI tests passed |

No orphaned requirements: `.planning/REQUIREMENTS.md:108-111` maps exactly TISO-01
through TISO-04 to Phase 8, and every one appears in at least one plan's
`requirements` frontmatter. All four are already marked `[x]` in REQUIREMENTS.md.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| — | — | `TBD` / `FIXME` / `XXX` across all 23 phase-modified source files | — | **0 hits** |
| — | — | `TODO` / `HACK` / `PLACEHOLDER` across the same files | — | **0 hits** |

Debt-marker gate: **clean**. No unreferenced markers were introduced by this phase.

### Human Decisions Honoured (not re-litigated)

| Decision | Verified in code |
|----------|------------------|
| (A) 08-01 spans 11 files as one atomic setter migration | `files_modified` matches the tree; zero legacy setters survive |
| (B) Removable = `ShapeMatch` + `NotReferencedByState` + (`Empty` OR `GitDisownsIt`); `NoGitdir` excluded | `ClearingSignal` (`worktree_scan.rs:174-178`) has exactly two variants; `classify` requires all three clauses |
| (C) Prune re-derives all evidence and requires it to MATCH the scan-time proof; `--prune` AND `--yes`; gitdir-bearing candidates use the verified-removal path | `prune_at:1343` fresh `scan_at`; `prune_one:1463-1480` compares approved vs re-derived proof; `RefusalReason::GitdirPresent` routes onward |

### Notes (informational, no action required)

- **The `GitDisownsIt` clearing arm is currently unreachable.** `contents_evidence`
  yields either `Empty` or `ContainsCheckout`, and `ContainsCheckout` is a
  `ProvesLive` blocker, so no candidate can reach `classify` with `GitDisownsIt`
  as its only clearing signal. This is the known open question already escalated
  to the user in 08-REVIEW.md and it makes the predicate strictly MORE
  conservative, never less. Not raised as a finding.
- **`WORKTREES_EXIT_FAILED` is also returned by two read-only `run_worktrees_scan`
  paths** (`main.rs:947`, `:967`) that print no account, which its prune-scoped
  doc comment does not cover. Recorded in 08-REVIEW.md as sub-finding-grade;
  both paths remove nothing.
- **The two deferred items in `deferred-items.md` are resolved or correctly
  parked.** The five `lifecycle::tests::*` escapes were closed by 08-06 commit
  `3f98851` — `baude-core/src/lifecycle.rs` now holds 3 `TestRedirect`s and all
  five cases passed in the observed run. `App::open_editor` remains unguarded but
  is unreachable from any test (see `deferred` frontmatter).

### Gaps Summary

**None.** All four ROADMAP Success Criteria are met by code that exists, is
substantive, is wired, and — for every behavior-dependent claim — is exercised by
a named test that passed in a run the verifier performed itself. All four
requirement IDs are satisfied. No blocker or warning anti-patterns.

The phase is `human_needed` rather than `passed` for one reason only: two
verifications are, by their own nature, measurements of the developer's real
filesystem, and the verifier operated under a hard constraint forbidding it to
read, write or fingerprint those roots. Both are declared manual-only in
08-VALIDATION.md and both are recorded as executed in the SUMMARYs — but a
SUMMARY claim is not verifier evidence, so they are surfaced for the human to
confirm rather than silently absorbed into a pass.

---

_Verified: 2026-09-15T20:11:43Z_
_Verifier: Claude (gsd-verifier)_
