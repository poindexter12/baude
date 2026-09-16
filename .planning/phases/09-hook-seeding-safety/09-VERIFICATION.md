---
phase: 09-hook-seeding-safety
verified: 2026-09-15T00:00:00Z
status: passed
score: 15/16 must-haves verified
covered_files:
  - .planning/REQUIREMENTS.md
  - .planning/phases/09-hook-seeding-safety/09-01-PLAN.md
  - .planning/phases/09-hook-seeding-safety/09-01-SUMMARY.md
  - .planning/phases/09-hook-seeding-safety/09-02-PLAN.md
  - .planning/phases/09-hook-seeding-safety/09-02-SUMMARY.md
  - .planning/phases/09-hook-seeding-safety/09-03-PLAN.md
  - .planning/phases/09-hook-seeding-safety/09-03-SUMMARY.md
  - .planning/phases/09-hook-seeding-safety/09-04-PLAN.md
  - .planning/phases/09-hook-seeding-safety/09-04-SUMMARY.md
  - baude-core/src/backend/claude.rs
  - baude-core/src/backend/mod.rs
  - baude-core/src/backend/opencode.rs
  - baude-core/src/hook.rs
  - baude-core/src/persist.rs
  - baude/src/app.rs
  - bauded/src/manager.rs
covered_digest: "v1:sha256:685f1d3a9597cab6ec95454dec4347b7291e5ea65affcc11039e1733c92c7680"
behavior_unverified: 1
overrides_applied: 0
behavior_unverified_items:
  - truth: "Concurrent or interrupted seeding never truncates or destroys a user's existing settings file (backstop, 09-01)"
    test: "Kill/interrupt a baude spawn mid-seed over an existing valid settings.local.json (or race two seeds)"
    expected: "The user's settings file is never left truncated or partially written"
    why_human: "verification: backstop — non-inferable. seed_settings writes via a single non-atomic std::fs::write (truncate-then-write); no test exercises interruption or concurrency. The truth's own parenthetical accepts 'best-effort single write', so this is a residual-risk acceptance decision, not a code gap."
coincidental_reliance_items:
  - truth: "TUI operator sees a warning naming the affected file after a spawn attempt over an unsafe settings file"
    reason: incidental-ordering
    harden: "The app-level tests read app.message AFTER the full spawn attempt; they pass because nothing later on the stubbed-'true' failure path calls set_message (review IN-04). Pin the guard's distinctive suffix in the assertion or assert immediately after seeding."
human_verification:
  - test: "Accept or reject the backstop residual: interrupt a spawn mid-seed (kill -9 during seed_settings) over a valid existing settings.local.json"
    expected: "File is not left truncated; or the residual (single non-atomic fs::write) is explicitly accepted as best-effort per the truth's own wording"
    why_human: "backstop-tier truth — no automated test can exercise write interruption; presence+wiring never qualifies as evidence for this tier"
  - test: "Run bauded from a terminal, spawn a session whose cwd holds a malformed .claude/settings.local.json, and watch the daemon's stderr"
    expected: "A line 'seed warning: <path>: could not parse settings ... existing settings left untouched ...' appears on each spawn/restart"
    why_human: "09-04 SUMMARY coverage D3 is explicitly human_judgment: the eprintln wiring is source- and compile-verified, but no automated test asserts the daemon's stderr at runtime, and visibility depends on how bauded is launched (stderr destination)"
  - test: "In prompt mode, spawn into a cwd where BOTH settings.local.json and .mcp.json are malformed; read the TUI message line"
    expected: "One aggregate message naming BOTH files (joined with ' | '), and both warnings reach stderr"
    why_human: "The WR-01 fix (warn_seed_failures aggregation + per-file stderr dedup, commit 0f84e41) is present and wired at both TUI call sites, but the existing seed_warning_ tests exercise only single-warning spawns — the two-warning aggregate is unexercised by any test"
  - test: "Prohibition sign-off: confirm the six flagged-unverified plan prohibitions (listed in the Prohibitions table below) with their recorded non-authoritative HOLDS verdicts"
    expected: "Human checkpoint confirms each HOLDS verdict; each has named passing enforcement tests recorded as evidence"
    why_human: "All six prohibitions carry disposition: flagged-unverified with no resolved status; autonomous verify records NON-AUTHORITATIVE verdicts only — never a silent pass"
---

# Phase 9: Hook Seeding Safety Verification Report

**Phase Goal:** Seeding a project's hooks never destroys a user's existing settings and never emits a command string the shell will mis-execute.
**Verified:** 2026-09-15
**Status:** human_needed
**Re-verification:** No — initial verification

All automated evidence was reproduced in the verifier's own process — no SUMMARY claim was taken on trust. Named tests were run per package; the full workspace suite was run exactly once, bracketed by `scripts/assert-real-roots-untouched.sh` (the phase's declared gate): **554 passed, 0 failed** (1+104+356+93 across targets), all four real roots untouched (PASS). The post-review WR-01 fix (commit `0f84e41`) is confirmed in code, not just in the review note.

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
| --- | ----- | ------ | -------- |
| 1 | (SC1) An unsafe `settings.local.json` (unreadable/unparseable/non-object) survives a spawn attempt byte-identical — never replaced with the seed alone | ✓ VERIFIED | `read_settings_guarded` (hook.rs:402-420) is the single read path; every `Err` returns before any write (hook.rs:373-378). Byte-identity tests `seed_guard_unparseable/unreadable/non_object_root_*` passed in verifier run (41/41 hook::) |
| 2 | (SC1) An unsafe `.mcp.json` is left byte-identical with a warning naming the file (D-03) | ✓ VERIFIED | `seed_mcp_config` routes through the same shared guard (claude.rs:127-130); `mcp_guard_unparseable/non_object/missing_*` passed (9/9 backend::claude::) |
| 3 | (SC1) The user receives an actionable warning naming the file, on every spawn path | ✓ VERIFIED (coincidental-reliance) | All 4 `prepare_cwd` call sites consume warnings: app.rs:2842-2843 and :5279-5280 via `warn_seed_failures` (aggregate set_message + per-file stderr dedup, post-WR-01); manager.rs:1161-1163 and :2143-2145 via `eprintln!("seed warning: {warning}")`. Display names the file + remediation (hook.rs:330-351). App-level tests `seed_warning_*` (2 passed) assert the message contains the path after a REAL `add_standalone_session_with_mode` spawn — but bind to last-message state (review IN-04, incidental ordering; see advisory) |
| 4 | (SC2) The seeded hook command quotes the executable path so a path with space, `$`, `;`, backtick invokes exactly that executable through a shell | ✓ VERIFIED | `baude_hook_command` Ok arm calls `quote_posix_single` (hook.rs:98); E2E `spaced_metachar_path_seed_executes_exact_stub` seeds through the production path into ``sp ace$;`tick'quote/baude``, executes the string read back from settings.local.json via real `sh -c`, asserts the stub's marker — passed in verifier run |
| 5 | (SC3) The recognizer matches the quoted form — quoting does not reintroduce HREG-01 per-path accumulation | ✓ VERIFIED | Two-arm `is_seeded_hook_command` (hook.rs:141-167), unquote-before-Path-checks; `merge_prunes_stale_quoted_seeds_to_one_group` + `merge_converges_legacy_unquoted_seed_to_the_quoted_form` passed |
| 6 | (SC4) Regression tests: malformed settings, spaced path E2E, leftover-lock reopen, held lock — all exist and pass | ✓ VERIFIED | All four run green by name in verifier's process: `seed_warning_malformed_*` (baude), `spaced_metachar_path_seed_executes_exact_stub` (hook), `reopen_claims_lock_when_leftover_file_outlives_released_os_lock` (persist, 1 passed), `held_lock_save_refuses_with_pid_diagnostic` (bauded, 1 passed) |
| 7 | A missing settings file still gets a fresh seed with no warning | ✓ VERIFIED | `NotFound → Ok(json!({}))` (hook.rs:409); `seed_guard_missing_file_fresh_seed_no_warning` + `mcp_guard_missing_file_fresh_seed` passed |
| 8 | Refused files stay byte-stable across repeated seed attempts; re-seed is idempotent | ✓ VERIFIED | `seed_guard_refusal_repeats_byte_stable`, `seed_settings_writes_idempotent_merge` passed |
| 9 | A user command that merely looks quoted is never claimed as baude's seed (#78 class) | ✓ VERIFIED | Strict round-trip `unquote_posix_single` (hook.rs:123-127); quoted-arm failure returns false without legacy fall-through (hook.rs:154-157); `quoted_look_alikes_and_legacy_forms_keep_their_meaning`, `pure_seed_accepts_quoted_seeds_and_rejects_look_alikes` passed. Review traced hostile classes by hand |
| 10 | The bare `baude hook` fallback stays unquoted and is never pruned (D-08) | ✓ VERIFIED | `FALLBACK_HOOK_COMMAND` byte-identical (hook.rs:71,99); recognizer deliberately excludes it (no absolute path); pinned by look-alike test |
| 11 | `.mcp.json` command field stays raw unquoted argv data (D-09) | ✓ VERIFIED | `exe` reaches `merge_mcp_config` unmodified (claude.rs:132); zero `quote_posix_single` occurrences in claude.rs; `mcp_command_is_raw_argv_not_shell_quoted` passed |
| 12 | A seeding failure never aborts a spawn in either binary (D-04) | ✓ VERIFIED | All four call sites continue unconditionally (no early return, no Result change); `seed_warning_*` tests reach post-spawn assertions after seeding over malformed files; write-failure path returns `vec![WriteFailed]`, never panics (`seed_guard_write_failure_warns_and_preserves_original` passed) |
| 13 | Reopening a workspace whose lock file outlives the released OS lock succeeds and re-stamps the current pid (WLOCK-04) | ✓ VERIFIED | persist.rs:1686+ test uses raw OpenOptions+try_lock (never `hold_state_lock` — re-entrant cache bypassed per Pitfall 6), fake pid 999999, asserts reopen Ok + current-pid re-stamp; passed |
| 14 | A bauded save under a foreign-held lock surfaces the holder pid + lock path and never removes the other owner's lock file (WLOCK-01/02/03) | ✓ VERIFIED | manager.rs:3037+ test: foreign holder pid 424242 held across `save_checked`; asserts "already owns this workspace", "pid 424242", lock file name, `!replacement_committed()`, and lock file + stamp intact; passed |
| 15 | Both lock tests run under Phase-8 isolation, touching no real user root (D-13) | ✓ VERIFIED | `isolated_root` / `ManagerFixture`; independently confirmed by the real-roots bracket: full suite ran with all four real roots fingerprint-unchanged |
| 16 | Concurrent/interrupted seeding never truncates or destroys an existing settings file (backstop) | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED (insufficient_spec) | `verification: backstop` — non-inferable. Write is a single non-atomic `std::fs::write` (hook.rs:381); no test exercises interruption/concurrency; presence+wiring never qualifies at this tier. Routed to human verification |

**Score:** 15/16 truths verified (1 present, behavior-unverified — backstop tier)

### Required Artifacts

| Artifact | Expected | Status | Details |
| -------- | -------- | ------ | ------- |
| `baude-core/src/hook.rs` | SeedWarning/SeedWarningReason, read_settings_guarded, seed_settings→Vec<SeedWarning>, quote_posix_single, two-arm recognizer, full test set | ✓ VERIFIED | All symbols present at hook.rs:111/123/141/307/316/368/402; contains-patterns `pub struct SeedWarning`, `quote_posix_single` match; 41 hook:: tests pass |
| `baude-core/src/backend/mod.rs` | `prepare_cwd -> Vec<crate::hook::SeedWarning>` | ✓ VERIFIED | mod.rs:114, exact signature |
| `baude-core/src/backend/claude.rs` | guarded seed_mcp_config; prepare_cwd collects both seeds | ✓ VERIFIED | claude.rs:74-80 (extend in prompt mode), :118-141; contains `read_settings_guarded` |
| `baude-core/src/backend/opencode.rs` | trait ripple: `Vec::new()` body | ✓ VERIFIED | opencode.rs:174 |
| `baude/src/app.rs` | warn surface at add + restart paths; `seed_warning_*` app-level tests | ✓ VERIFIED | `warn_seed_failures` (post-WR-01 aggregate, app.rs:3615-3634) consumed at :2843 and :5280; tests at :9526/:9531 pass. Note: plan's `warn_seed_failure` was renamed `warn_seed_failures` by the accepted WR-01 fix — an improvement, not a gap |
| `bauded/src/manager.rs` | "seed warning:" eprintln at both daemon spawn paths; held_lock test | ✓ VERIFIED | manager.rs:1161-1163, :2143-2145; test at :3037 passes; contains `seed warning` and `held_lock` |
| `baude-core/src/persist.rs` | leftover-lock reopen regression test | ✓ VERIFIED | persist.rs:1686; contains `leftover`; passes |

### Key Link Verification

| From | To | Via | Status | Details |
| ---- | -- | --- | ------ | ------- |
| hook.rs | backend/claude.rs | ClaudeBackend::prepare_cwd returns seed_settings warnings | ✓ WIRED | claude.rs:75 `crate::hook::seed_settings(cwd)` |
| backend/mod.rs | baude/src/app.rs | add-session path consumes Vec<SeedWarning> | ✓ WIRED | app.rs:2842-2843 `warn_seed_failures(&seed_warnings)` |
| backend/mod.rs | baude/src/app.rs (restart) | restart path consumes warnings identically | ✓ WIRED | app.rs:5279-5280 |
| bauded/manager.rs | backend/mod.rs | both daemon call sites consume prepare_cwd's Vec | ✓ WIRED | manager.rs:1161, :2143 — loop + prefixed eprintln |
| backend/claude.rs | hook.rs | seed_mcp_config reuses hook::read_settings_guarded | ✓ WIRED | claude.rs:127 |
| baude_hook_command | is_seeded_hook_command | quoted string is the idempotency sentinel; round-trip agreement | ✓ WIRED | Both call quote_posix_single; E2E asserts producer output recognized |
| is_seeded_hook_command | is_pure_seed_group | purity predicate consumes recognizer (gates worktree removal, #78) | ✓ WIRED | hook.rs:283-286 |
| manager.rs | persist.rs | save_checked → atomic_save_current → hold_state_lock | ✓ WIRED | held_lock test drives save_checked to StateLockError::Held |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
| -------- | ------------- | ------ | ------------------ | ------ |
| app.rs TUI message | `app.message` | SeedWarning::Display ← read_settings_guarded's real io/serde error | Yes — app-level test asserts real path text in message | ✓ FLOWING |
| bauded stderr | `eprintln!("seed warning: {warning}")` | same Display, real prepare_cwd return | Yes (source-verified; runtime visibility → human item 2) | ✓ FLOWING |
| settings.local.json | seeded `command` string | quote_posix_single(current_exe/override) | Yes — E2E reads it back from disk and executes it | ✓ FLOWING |

### Behavioral Spot-Checks

All run in the verifier's own process:

| Behavior | Command | Result | Status |
| -------- | ------- | ------ | ------ |
| Guard matrix + quoting + sh -c E2E | `cargo test -p baude-core --lib hook::` | 41 passed, 0 failed | ✓ PASS |
| Leftover-lock reopen | `cargo test -p baude-core --lib persist::tests::reopen_claims_lock_when_leftover_file_outlives_released_os_lock` | 1 passed | ✓ PASS |
| .mcp.json guard + raw argv | `cargo test -p baude-core --lib backend::claude::` | 9 passed | ✓ PASS |
| bauded held-lock diagnostic | `cargo test -p bauded --bins manager::tests::held_lock_` | 1 passed (filter matched) | ✓ PASS |
| App-level malformed-settings spawn | `cargo test -p baude --bins seed_warning_` | 2 passed (filter matched) | ✓ PASS |
| All 13 SUMMARY-claimed commits exist (incl. WR-01 fix 0f84e41, RED commits 807fa47/4ffc2f5/00b3f9c) | `git log -1` each | all present, subjects match | ✓ PASS |

### Probe Execution

No `scripts/*/tests/probe-*.sh` probes exist. The phase's declared gate is the real-roots bracket, executed once by the verifier:

| Probe | Command | Result | Status |
| ----- | ------- | ------ | ------ |
| Real-roots bracket around full suite | `assert-real-roots-untouched.sh before` → `cargo test --workspace` → `... after` | 554 passed / 0 failed; "PASS: the run left all four real roots untouched." | PASS |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| ----------- | ----------- | ----------- | ------ | -------- |
| HREG-03 | 09-01, 09-04 | Settings retained unchanged when seeding cannot safely parse/update; actionable warning instead of silent replacement | ✓ SATISFIED | Truths 1-3, 7-8, 12; both clobber sites guarded via one shared helper |
| HREG-04 | 09-03 | Launch from a path with spaces/metacharacters; seeded hook invokes that exact executable safely and idempotently | ✓ SATISFIED | Truths 4-5, 9-10; sh -c E2E invocation proof |
| WLOCK-01 | 09-02 | Second-instance contention reported explicitly | ✓ SATISFIED | held_lock test pins the existing first-save surface (option (a) per plan; startup residual stays documented in REQUIREMENTS.md) |
| WLOCK-03 | 09-02 | Diagnostic names workspace/pid/lock path | ✓ SATISFIED | held_lock test asserts pid 424242 + lock file name in Display |
| WLOCK-04 | 09-02 | Reopen after OS lock release despite leftover file | ✓ SATISFIED | reopen test; REQUIREMENTS.md row already updated to cite it |
| HREG-01, HREG-02, WLOCK-02 | (no plan claims) | Mapped to Phase 9 in REQUIREMENTS.md but pre-delivered (v2.1.2/v2.1.3) | ✓ ACCOUNTED FOR | Not orphaned work: HREG-01's no-accumulation is actively preserved by this phase's convergence tests (SC3); WLOCK-02's never-remove-lock contract is asserted inside the held_lock test; HREG-02 pre-delivered, untouched by this phase |

### Prohibitions (must-NOT, all `disposition: flagged-unverified`)

Non-authoritative LLM-judge verdicts recorded per autonomous-verify rules; human sign-off requested (human item 4). None is silently passed.

| # | Prohibition (plan) | Verdict | Enforcement evidence (run green by verifier) |
| --- | ---------------- | ------- | -------------------------------------------- |
| 1 | Guard MUST NOT overwrite a settings file it could not read+parse to an object (09-01) | HOLDS (non-authoritative) | Err-before-any-write in read_settings_guarded callers; byte-identity tests seed_guard_* / mcp_guard_* |
| 2 | Seeding MUST NOT abort a spawn on any read/parse/write failure (09-01, 09-04) | HOLDS (non-authoritative) | No early return at any of 4 call sites; seed_warning_* spawn attempts continue; WriteFailed returns instead of panicking |
| 3 | A test MUST NOT "fix" contention by deleting a lock file (09-02) | HOLDS (non-authoritative) | held_lock test asserts the foreign lock file + stamp survive the refused save |
| 4 | is_pure_seed_group MUST NOT widen (#78 class) (09-03) | HOLDS (non-authoritative) | Strict round-trip; quoted-arm failure returns false without legacy fall-through; pure_seed_accepts_quoted_seeds_and_rejects_look_alikes; review hand-traced hostile classes |
| 5 | .mcp.json permission-mcp command MUST NOT be quoted (09-04) | HOLDS (non-authoritative) | mcp_command_is_raw_argv_not_shell_quoted; zero quote_posix_single occurrences in claude.rs |
| 6 | Seeding MUST NOT abort — restated in 09-04 | HOLDS (non-authoritative) | Same as #2 |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| — | — | No TBD/FIXME/XXX/TODO/placeholder/stub patterns in any of the 7 modified files | — | — |

Review Info items IN-01..IN-04 (silent current_exe skip, duplicated bauded loop, mirrored private lock-path format, last-message-state test binding) noted as ℹ️ Info — none blocks the phase goal; IN-04 is captured as the coincidental-reliance advisory on truth 3.

### Human Verification Required

#### 1. Backstop residual: interrupted/concurrent seed write

**Test:** Kill a spawn mid-seed (or race two seeds) over an existing valid `settings.local.json`.
**Expected:** File never left truncated — or the residual is explicitly accepted (the truth's own wording concedes "best-effort single write of fully merged JSON"; the write at hook.rs:381 is a non-atomic `std::fs::write`).
**Why human:** backstop-tier truth — non-inferable from presence/wiring; no test exercises interruption. Accepting the residual (or requesting temp+rename atomicity in a later phase) is a human decision.

#### 2. Live bauded stderr visibility

**Test:** Run bauded from a terminal; spawn a session whose cwd holds `{not json` in `.claude/settings.local.json`; watch stderr.
**Expected:** `seed warning: <path>: could not parse settings ... existing settings left untouched ...` per spawn/restart.
**Why human:** 09-04's own coverage marks this human_judgment — wiring is source-verified but no test asserts daemon stderr at runtime, and visibility depends on how bauded is launched.

#### 3. Multi-warning TUI aggregation (WR-01 fix)

**Test:** Prompt mode, cwd with BOTH files malformed; read the TUI message line.
**Expected:** One aggregate message naming both files (joined `" | "`); both warnings on stderr.
**Why human:** The fix is present and wired at both call sites, but all existing tests exercise single-warning spawns only.

#### 4. Prohibition sign-off

**Test:** Confirm the six non-authoritative HOLDS verdicts in the Prohibitions table.
**Expected:** Human checkpoint accepts each (all carry named passing enforcement tests).
**Why human:** All six remain `disposition: flagged-unverified`; autonomous verify may never silently pass them.

### Gaps Summary

No gaps. Every ROADMAP success criterion is achieved with verifier-reproduced evidence: both clobber sites share one fail-closed guard with byte-identity tests at unit AND app level; the quoted hook command is proven by real `sh -c` invocation on a space/`$`/`;`/backtick/embedded-quote path; the strict round-trip recognizer prevents both per-path accumulation (SC3) and #78-class false claims; all four SC4 regression tests exist and pass; the post-review WR-01 aggregation fix is confirmed in code. Status is human_needed solely because of one backstop-tier truth (interrupted-write residual acceptance), three runtime-visibility judgment checks, and the mandated prohibition sign-off — none indicates missing or stubbed implementation.

---

_Verified: 2026-09-15_
_Verifier: Claude (gsd-verifier)_
