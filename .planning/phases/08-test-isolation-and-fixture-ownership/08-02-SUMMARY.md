---
phase: 08-test-isolation-and-fixture-ownership
plan: 02
subsystem: testing
tags: [rust, test-isolation, claude-config, web-push, vapid, config-resolver, tdd]

# Dependency graph
requires:
  - phase: 08
    plan: 01
    provides: "TestRedirect, NoFixtureRoot, assert_contained, and the cross-crate test-support feature"
provides:
  - "meta::claude_config_dir() is redirect-aware and containment-asserting — the ~/.claude resolver can no longer be reached from a test"
  - "meta::real_claude_config_dir() — the ungated verbatim CLAUDE_CONFIG_DIR chain, kept separate from persist's and git's XDG chains"
  - "bauded has exactly one config-directory resolver: baude_core::persist::config_dir()"
  - "First store-location coverage for the push subsystem (VAPID keypair + subscription store), which previously had none"
affects: [08-03, 08-06, 08-08]

actuals:
  tokens: 4560  # chars/4 over the realized diff: `git diff 8730fbb..HEAD | wc -c` = 18243
  tasks: 2
  commits: 4  # MEASURED: `git rev-list --count 8730fbb..HEAD` at SUMMARY write (excludes this docs commit)
plan_head_before: 8730fbb1450dc719ccb5211fe8260951a0d9c9ad

tech-stack:
  added: []
  patterns:
    - "Resolver split: private real_*() holding the verbatim original chain, public resolver gated on redirect + assert_contained"
    - "Isolation by ambient redirect rather than by parameter, so production call sites keep their signatures and gain no _at variant"
    - "Every isolation test asserts the RESOLVED PATH before any filesystem I/O, so its RED fails on a comparison instead of reading the developer's real directory"
    - "Duplicate resolvers are deleted rather than individually guarded — one guarded resolver plus one unguarded copy is an unguarded subsystem"

key-files:
  created: []
  modified:
    - baude-core/src/meta.rs
    - bauded/src/push.rs

key-decisions:
  - "meta::claude_config_dir keeps its CLAUDE_CONFIG_DIR head and \".\" tail in real_claude_config_dir rather than being folded into a shared helper with persist's and git's XDG chains — the three resolvers share a shape but not their heads or tails, so collapsing them would change production behavior."
  - "bauded's private config_base() was byte-identical to persist::config_base() (same XDG_CONFIG_HOME head, same ~/.config and \".\" fallbacks, same .join(\"baude\") tail), so routing through the shared resolver leaves every production path where it was: the already-written VAPID key is read, never rotated (T-08-08 accepted)."
  - "The push RED was executed against a scratch XDG_CONFIG_HOME pinned on the TEST BINARY, not on the cargo invocation: in the pre-fix state the subsystem writes a real VAPID private key at load, and the phase's hard constraint forbids a test touching ~/.config/baude."
  - "XDG_CONFIG_HOME could not be pinned on `cargo test` itself — the repo's mise/account-cli toolchain wrapper resolves its account config through that variable and aborts the build. The binary was built first with the normal environment, then run with the pinned one."
  - "store_isolation_ tests assert positively that both files land inside the held fixture root and never look at, derive, or fingerprint the real config dir; proving the negative across the whole suite is plan 06's external observer."

patterns-established:
  - "Pattern: a duplicated real-root resolver in a downstream crate is deleted, not guarded"
  - "Pattern: a RED whose pre-fix code path writes to a real user root is run against an environment-pinned binary, and the pin is recorded in the evidence record"

requirements-completed: [TISO-01, TISO-03]

coverage:
  - id: D1
    description: "A fixture holding a TestRedirect sees meta::claude_config_dir() resolve inside its fixture root, and nested scopes restore the outer root on drop (D-03, D-11)"
    requirement: TISO-01
    verification:
      - kind: unit
        ref: "baude-core/src/meta.rs#meta::tests::claude_config_dir_follows_the_thread_redirect"
        status: pass
      - kind: unit
        ref: "baude-core/src/meta.rs#meta::tests::claude_config_dir_nested_scopes_restore_the_outer_root"
        status: pass
    human_judgment: false
  - id: D2
    description: "Both ClaudeMeta::poll call sites (session file and transcript) follow one redirect, with neither call site reshaped and neither gaining an _at variant"
    requirement: TISO-01
    verification:
      - kind: unit
        ref: "baude-core/src/meta.rs#meta::tests::claude_config_dir_poll_reads_redirected_session_data"
        status: pass
      - kind: other
        ref: "git diff -U0 4a06cf5^..4a06cf5 -- baude-core/src/meta.rs => neither line 247 nor line 291 appears in the diff"
        status: pass
    human_judgment: false
  - id: D3
    description: "Resolving the claude config dir with no redirect and no fixture root aborts the test instead of reaching the developer's real ~/.claude (D-03, D-11)"
    requirement: TISO-03
    verification:
      - kind: unit
        ref: "baude-core/src/meta.rs#meta::tests::claude_config_dir_unguarded_resolution_panics"
        status: pass
      - kind: unit
        ref: "baude-core/src/meta.rs#meta::tests::claude_config_dir_escape_returns_after_the_last_guard_drops"
        status: pass
    human_judgment: false
  - id: D4
    description: "Constructing a PushState under a redirect writes the VAPID keypair and the subscription store inside the fixture root, for persist = true and persist = false alike (D-02, D-11)"
    requirement: TISO-01
    verification:
      - kind: unit
        ref: "bauded/src/push.rs#push::tests::store_isolation_persists_both_stores_inside_the_fixture_root"
        status: pass
      - kind: unit
        ref: "bauded/src/push.rs#push::tests::store_isolation_contains_the_key_even_when_persist_is_false"
        status: pass
      - kind: unit
        ref: "bauded/src/push.rs#push::tests::store_isolation_nested_roots_do_not_interfere"
        status: pass
    human_judgment: false
  - id: D5
    description: "bauded has exactly one config-directory resolver, and it is persist::config_dir() (D-02)"
    requirement: TISO-01
    verification:
      - kind: other
        ref: "grep -rn 'XDG_CONFIG_HOME|dirs::home_dir|config_base' bauded/src/ => no resolver in push.rs (one explanatory comment); manager.rs's dirs::home_dir is expand_tilde, user-typed-path expansion, not a store resolver"
        status: pass
      - kind: unit
        ref: "bauded/src/push.rs#push::tests::store_isolation_denies_unredirected_load"
        status: pass
    human_judgment: false
  - id: D6
    description: "The VAPID key file format, encoding, and generation path are byte-for-byte unchanged, so the already-written key is read rather than rotated (T-08-08)"
    verification:
      - kind: other
        ref: "git diff -U0 da53845^..da53845 -- bauded/src/push.rs => the only non-test production changes are three call-site substitutions; VapidOnDisk, SecretKey::random, the base64 encoding, VAPID_FILE and SUBS_FILE are untouched"
        status: pass
      - kind: unit
        ref: "bauded/src/push.rs#push::tests::{encrypt_round_trips, vapid_header_shape} — pre-existing crypto tests, unmodified, still passing"
        status: pass
    human_judgment: false

# Metrics
duration: 16min
completed: 2026-09-14
status: complete
---

# Phase 08 Plan 02: Claude Config and Push Store Isolation Summary

**The two remaining resolvers that could reach a real user root from a test are closed: `meta::claude_config_dir()` now follows the thread redirect and aborts on escape, and `bauded`'s duplicate config resolver is deleted so the daemon's VAPID keypair and subscription store land inside the fixture root.**

## Performance

- **Duration:** 16 min (plan start 2026-09-15T02:52:18Z → close-out 2026-09-15T03:09Z)
- **Tasks:** 2 of 2
- **Files modified:** 2 (exactly the plan's `files_modified` set)
- **Commits:** 4 (measured: `git rev-list --count 8730fbb..HEAD`, 2 RED + 2 GREEN; this docs commit is additional)

## Accomplishments

- **Closed the `~/.claude` resolver.** `meta::claude_config_dir()` split into an ungated
  `real_claude_config_dir()` holding the verbatim `CLAUDE_CONFIG_DIR` → `~/.claude` → `"."`
  chain, plus a public resolver that returns the thread redirect when one is held and calls
  `assert_contained` otherwise. Both `ClaudeMeta::poll` call sites — the session file and the
  transcript — are textually unchanged, which is the point of isolating by ambient redirect
  rather than by parameter (D-03).
- **Deleted `bauded`'s second config resolver.** `push.rs` carried its own copy of the XDG
  chain, which made the daemon the one subsystem a `TestRedirect` could not contain: it wrote
  a real VAPID signing key and a real subscription store into `~/.config/baude` from the test
  suite (#72). All three call sites — the key path in `Vapid::load_or_generate`, the read in
  `PushState::load`, and the write in `PushState::save` — now go through
  `baude_core::persist::config_dir()` (D-02).
- **Gave the push subsystem its first store-location coverage.** It previously had none, which
  is exactly why nothing noticed the leak: the only push tests were crypto round-trips that
  construct a `Vapid` value directly and never touch the filesystem. Four
  `store_isolation_` cases now pin both files to the held fixture root.
- **Covered `persist = false` explicitly.** The plan's research flagged it and the test now
  records it: `PushState::load(false)` suppresses subscription reads and writes but still calls
  `Vapid::load_or_generate()` unconditionally, so it writes a key regardless. Passing `false`
  is not a substitute for the redirect, and
  `store_isolation_contains_the_key_even_when_persist_is_false` fails loudly if anyone later
  assumes it is.

## Task Commits

| Task | Name | Commit | Files |
| ---- | ---- | ------ | ----- |
| 1 (RED) | Failing guards for a redirect-aware `claude_config_dir` | `6fd3ecf` | `baude-core/src/meta.rs` (+128) |
| 1 (GREEN) | Make `meta::claude_config_dir` redirect-aware and contained | `4a06cf5` | `baude-core/src/meta.rs` (+32 −2) |
| 2 (RED) | Failing store-isolation guards for the push subsystem | `7189283` | `bauded/src/push.rs` (+160) |
| 2 (GREEN) | Route the push stores through the shared config resolver | `da53845` | `bauded/src/push.rs` (+10 −11) |

## Files Created/Modified

**Modified**

- `baude-core/src/meta.rs` — `real_claude_config_dir()` extracted; `claude_config_dir()` gated
  on `crate::testing::claude_config_dir_override()` with `assert_contained` on the real path;
  five `claude_config_dir_*` tests added.
- `bauded/src/push.rs` — private `config_base()` deleted (replaced by a comment recording why
  the duplicate existed and why removing it is path-preserving); three call sites rerouted;
  the now test-only `use std::path::PathBuf` moved into the test module; four
  `store_isolation_*` tests added.

## Verification

Wave 2 runs only the named contained tests; the broader meta, push, downstream-binary,
dogfood and workspace suites plus clippy belong to plan 06 (plan `<verification>`). Every
number below is from a run executed in this session with its exit code captured directly.

| Gate | Command | Result |
| ---- | ------- | ------ |
| Task 1 | `cargo test -p baude-core --lib meta::tests::claude_config_dir_` | exit 0 — **5 passed, 0 failed**, 251 filtered out |
| Task 2 | `cargo test -p bauded --bins push::tests::store_isolation_` | exit 0 — **4 passed, 0 failed**, 80 filtered out |
| Push module (crypto regression) | `cargo test -p bauded --bins push::tests::` | exit 0 — **6 passed, 0 failed**, 78 filtered out |
| Formatting | `cargo fmt --all --check` | exit 0 |
| Compile | `cargo check --workspace --all-targets --locked` | exit 0 |

The 6-vs-4 difference is the two pre-existing crypto tests (`encrypt_round_trips`,
`vapid_header_shape`), unmodified and still green.

## TDD Gate Compliance

Both tasks are behavior-adding and both ran a full RED → GREEN cycle with validated evidence.

| Task | RED commit | Measured RED | Evidence verdict | GREEN commit |
| ---- | ---------- | ------------ | ---------------- | ------------ |
| 1 | `6fd3ecf` | exit 101 — 0 passed, 5 failed | `RED_EVIDENCE_OK` (`target_test_failed`, target `meta::tests::claude_config_dir_follows_the_thread_redirect`) | `4a06cf5` |
| 2 | `7189283` | exit 101 — 0 passed, 4 failed | `RED_EVIDENCE_OK` (`target_test_failed`, target `push::tests::store_isolation_persists_both_stores_inside_the_fixture_root`) | `da53845` |

Both REDs were intentional — the subject behavior was absent, not mutated in. Task 1's RED
failed on path comparisons (`left: /Users/joese/.poindexter/claude, right:
/nonexistent/baude-meta-claude-redirect/claude`); Task 2's RED failed on the absence of the
key and store files at the redirected paths, plus `did not panic as expected` for the escape
case.

**Task 2's RED needed containment of its own.** In the pre-fix state `PushState::load` reaches
the ambient config dir and *writes a VAPID private key there* — running that RED naively would
have read and written `~/.config/baude`, committing the exact leak the task exists to close.
The RED was therefore run against a test binary with `XDG_CONFIG_HOME` pinned to a scratch
directory, which is the first entry in the duplicated chain. The run is auditable: after it,
the scratch directory contained `baude/daemon-vapid.json` and `baude/daemon-push.json` — the
leak reproduced under containment. The pin is recorded verbatim in the evidence record's
`command` field.

## Decisions Made

- **The three real-root resolvers stay separate.** `meta` (`CLAUDE_CONFIG_DIR` → `~/.claude` →
  `"."`), `persist` (`XDG_CONFIG_HOME` → `~/.config` → `"."` + `baude`), and `git` (XDG with a
  `/tmp` tail) share a shape but not their heads or tails. `real_claude_config_dir` carries a
  doc comment saying so, because the next reader's instinct will be to deduplicate them.
- **`bauded`'s duplicate was deleted, not guarded.** Adding a second guarded resolver would
  have left two places to keep in sync; one guarded resolver plus one unguarded copy is an
  unguarded subsystem.
- **Production paths are provably unchanged.** The deleted chain and `persist::real_config_base`
  are token-for-token the same, so the daemon still resolves `~/.config/baude/daemon-vapid.json`
  and reads the existing key. T-08-08 (already-written key) stays accepted; rotation remains
  deferred and was not attempted.
- **The `store_isolation_` tests prove a positive only.** They assert both files appear inside
  the held fixture root. None of them reads, derives, stats, or fingerprints the real config
  dir — a test that looked there to prove a negative would commit the leak it is checking for.
  The whole-suite no-write assertion is plan 06's external observer.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `XDG_CONFIG_HOME` could not be pinned on the `cargo test` invocation**

- **Found during:** Task 2 RED
- **Issue:** Pinning the variable on `cargo test` aborted the build before compiling —
  `account-cli env: no account 'poindexter'` / `mise ERROR ... exited with code 1`. The repo's
  mise wrapper sources an account-env script that resolves `~/.config/agent-accounts/` through
  `XDG_CONFIG_HOME`, so overriding it breaks the toolchain shim, not the test.
- **Fix:** Split the run — `cargo test -p bauded --bins --no-run --message-format=json` with
  the normal environment (exit 0), then execute the resulting binary directly with the pinned
  variable. The GREEN gate uses the plan's verbatim unpinned command, because a contained GREEN
  must be safe with no environmental help.
- **Files modified:** none (execution procedure only)
- **Commit:** n/a

**2. [Rule 3 - Blocking] `use std::path::PathBuf` became test-only in `push.rs`**

- **Found during:** Task 2 GREEN
- **Issue:** Deleting `config_base()` left the crate-level import used only by the test
  module's `isolated_push_root` helper, which would warn on a non-test build.
- **Fix:** Moved the import into `mod tests`.
- **Files modified:** `bauded/src/push.rs`
- **Commit:** `da53845`

### Planned-but-adjusted

None. Both tasks ran as written, including the plan's instruction to cover `persist = false`
explicitly and to keep the existing crypto tests unchanged.

## Issues Encountered

- **The mise/account-cli wrapper is environment-sensitive in a way that bites env-pinned test
  runs.** Any future plan that needs to pin `XDG_*` or `HOME` for a test must pin it on the
  built binary, not on the cargo command. `HOME` in particular must not be pinned on cargo at
  all — `CARGO_HOME` derives from it and the whole registry would be re-fetched.
- **`CLAUDE_CONFIG_DIR` is set to a real path in this development shell**
  (`/Users/joese/.poindexter/claude`). That is precisely the condition plan 01 Task 3 handled by
  pinning the variable on the dogfood child's `Command`: without it, a re-exec'd child inherits
  the developer's real Claude directory and trips `assert_contained`. Task 1's RED output shows
  the variable winning the chain, which is the intended precedence.

## Known Stubs

None. No `TODO`, `FIXME`, `todo!`, `unimplemented!`, or `#[ignore]` was introduced in either
file.

## Threat Flags

None. No new network endpoint, auth path, file-access pattern, or schema at a trust boundary
was introduced — the change is strictly a removal of one path-resolution site and the addition
of tests.

## Safety

No file under a real user root was read, written, created, or removed during this execution.

- Task 1's tests assert the resolved path *before* any filesystem call, so the RED failed on a
  comparison and `ClaudeMeta::poll` never ran against the real `~/.claude`. The real path
  appears in the RED output only as an assertion's left-hand value — resolution, not I/O.
- Task 2's RED was environment-pinned to a scratch directory (above), so the pre-fix VAPID key
  write landed there; the developer's real `~/.config/baude` was never read or written, and the
  real VAPID private key was never loaded into a test process (T-08-01).
- All fixture roots are synthetic (`/nonexistent/...`) or under `std::env::temp_dir()`, and the
  temp ones are removed at the end of each case.
- Historical leak cleanup remained preview-only; nothing was pruned or deleted.
- git remained on SSH; no push, no branch switch, no PR. The untracked `.gsd/` and
  `.planning/milestone.lock` were left untouched.

## User Setup Required

None. No package was installed and no external service configuration is required.

## Next Phase Readiness

- **Plan 03** (UI fixtures / workspace identity) can rely on `claude_config_dir` being
  redirect-aware; `Redirects.claude_config_dir` is now consumed rather than merely declared.
- **Plan 06** owns the integration acceptance: the broad meta and push suites, the downstream
  binary suites, the dogfood regression, clippy, and the external no-write observer. Until it
  lands, a full-suite run will still fail wherever an unmigrated consumer resolves a real path
  — that is the guard working, as plan 01 recorded.
- **Plan 08** (usage worker isolation) is unaffected by this plan's surface.

## Requirements Bookkeeping

`requirements-completed` above copies this plan's `requirements:` frontmatter, as the SUMMARY
contract requires. **REQUIREMENTS.md was deliberately NOT checked off.** TISO-01 and TISO-03
are phase-wide requirements that this plan advances but does not finish — their entries carry
a `(partial)` suffix, and the standing STATE.md blocker from plan 01 says not to mark either
complete before plan 06 lands the remaining consumer migrations. Checking them off after 2 of
8 plans would be a false completion claim.

## Self-Check: PASSED

Both modified files exist on disk and all four task commits (`6fd3ecf`, `4a06cf5`, `7189283`,
`da53845`) resolve in `git log`.

---
*Phase: 08-test-isolation-and-fixture-ownership*
*Completed: 2026-09-14*
