---
phase: 09-hook-seeding-safety
verified: 2026-09-18T00:00:00Z
status: passed
human_items_accepted: "2026-09-18 by Joe Seymour — accepted, NOT observed; see Acceptance Record"
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
covered_digest: "v1:sha256:e50829f5ff6468f1bc07ab04c1fd909ff8c48e6540d9550d25339c498727ee0d"
behavior_unverified: 1
overrides_applied: 0
re_verification:
  previous_status: human_needed
  previous_score: 2/5
  previous_verified: 2026-09-15T00:00:00Z
  reason: "covered_digest went stale — .planning/REQUIREMENTS.md and baude/src/app.rs both changed after it was written (phases 10, 11, 12). Not a reported regression."
  gaps_closed: []
  gaps_remaining: []
  regressions: []
  notes:
    - "The stale report was internally inconsistent: frontmatter said status: passed, body said Status: human_needed. Per the decision tree, a non-empty human_verification section forces human_needed. Corrected here; previous_score above restates its own 15/16 headline as recorded (15/16 truths, 1 behavior-unverified)."
    - "Phase-9 commit hashes cited in the SUMMARYs (0f84e41, 807fa47, 4ffc2f5, 00b3f9c) are NOT ancestors of this HEAD — the phase-9 history was rebased. Subject-identical commits exist (WR-01 fix is now 290391f). Evidence below is bound to code and tests at HEAD, not to those hashes."
behavior_unverified_items:
  - truth: "Concurrent or interrupted seeding never truncates or destroys a user's existing settings file (backstop, 09-01)"
    test: "Kill/interrupt a baude spawn mid-seed over an existing valid settings.local.json (or race two seeds)"
    expected: "The user's settings file is never left truncated or partially written"
    why_human: "verification: backstop — non-inferable. seed_settings still writes via a single non-atomic std::fs::write (hook.rs:381); no test exercises interruption or concurrency. The truth's own parenthetical accepts 'best-effort single write', so this is a residual-risk acceptance decision, not a code gap. Unchanged at this HEAD."
coincidental_reliance_items:
  - truth: "TUI operator sees a warning naming the affected file after a spawn attempt over an unsafe settings file"
    reason: incidental-ordering
    harden: "The app-level tests read app.message AFTER the full spawn attempt; they pass because nothing later on the stubbed-'true' failure path calls set_message (review IN-04). Pin the guard's distinctive suffix in the assertion, or assert immediately after seeding. NOTE: phases 10-11 added ~986 lines to app.rs and the tests still pass — IN-04's predicted break did not materialize, but the reliance remains unhardened."
human_verification:
  - test: "Accept or reject the backstop residual: interrupt a spawn mid-seed (kill -9 during seed_settings) over a valid existing settings.local.json"
    expected: "File is not left truncated; or the residual (single non-atomic fs::write) is explicitly accepted as best-effort per the truth's own wording"
    why_human: "backstop-tier truth — no automated test can exercise write interruption; presence+wiring never qualifies as evidence for this tier"
  - test: "Run bauded from a terminal, spawn a session whose cwd holds a malformed .claude/settings.local.json, and watch the daemon's stderr"
    expected: "A line 'seed warning: <path>: could not parse settings ... existing settings left untouched ...' appears on each spawn/restart"
    why_human: "09-04 SUMMARY coverage D3 is explicitly human_judgment: the eprintln wiring is source- and compile-verified (manager.rs:1161-1162, :2143-2144), but no automated test asserts the daemon's stderr at runtime, and visibility depends on how bauded is launched (stderr destination)"
  - test: "In prompt mode, spawn into a cwd where BOTH settings.local.json and .mcp.json are malformed; read the TUI message line"
    expected: "One aggregate message naming BOTH files (joined with ' | '), and both warnings reach stderr"
    why_human: "The WR-01 fix (warn_seed_failures aggregation + per-file stderr dedup) is present and wired at both TUI call sites, but the existing seed_warning_ tests exercise only single-warning spawns — the two-warning aggregate is still unexercised by any test at this HEAD"
  - test: "Prohibition sign-off: confirm the six flagged-unverified plan prohibitions (listed in the Prohibitions table below) with their recorded non-authoritative HOLDS verdicts"
    expected: "Human checkpoint confirms each HOLDS verdict; each has named passing enforcement tests recorded as evidence"
    why_human: "All six prohibitions carry disposition: flagged-unverified with no resolved status; autonomous verify records NON-AUTHORITATIVE verdicts only — never a silent pass"
advisory:
  - finding: "REQUIREMENTS.md HREG-03 row is annotated '(not started)' while its checkbox is [x] and the tracking table (line 113) says 'Complete'"
    category: other
    reason: "Planning-doc bookkeeping only — no code implication. Resolved by updating the inline parenthetical to cite plans 09-01/09-04, as HREG-04 does."
    evidence_status: "none provided"
---

# Phase 9: Hook Seeding Safety Verification Report

**Phase Goal:** Seeding a project's hooks never destroys a user's existing settings and never emits a command string the shell will mis-execute.
**Verified:** 2026-09-18
**Status:** passed — automated verification passed; the open human-verification items were ACCEPTED by the maintainer on 2026-09-18 rather than observed (see Acceptance Record).
**Re-verification:** **Yes** — re-verification at HEAD `ee6fa3c`, not an initial pass. The 2026-09-15 report went stale on `covered_digest` (mechanical: `.planning/REQUIREMENTS.md` and `baude/src/app.rs` both changed after it was written). This pass re-binds every must-have to code and named tests at the current HEAD and emits a fresh digest.

**Headline: no regression. Phase 9's delivered behavior is intact at this HEAD.**

## Re-verification Scope and Result

The stated risk was that phases 10-12 disturbed phase 9's surface in `app.rs`. That was checked deterministically, not by inspection alone:

| Check | Command | Result |
| ----- | ------- | ------ |
| Which phase-9 files changed after the WR-01 fix | `git diff --stat 290391f..HEAD -- <7 files>` | **Only `baude/src/app.rs`** (+986/-19). The other six — `hook.rs`, `backend/mod.rs`, `backend/claude.rs`, `backend/opencode.rs`, `bauded/src/manager.rs`, `persist.rs` — are **byte-identical** to their post-WR-01 state |
| Did those later edits touch seeding | `git diff 290391f..HEAD -- baude/src/app.rs \| grep -E '^[-+].*(seed\|warn_seed\|prepare_cwd\|SeedWarning)'` | **Zero matching lines.** Phases 10 (OSC8 links) and 11 (keyboard negotiation) added code elsewhere in the file |
| Which commits touched phase-9 files after WR-01 | `git log --oneline 290391f..HEAD -- <7 files>` | 13 commits, all `feat/test/fix/style(10-*)` and `(11-*)`. **No phase-12 commit touches any phase-9 file** — the 12-01 clippy fixes landed elsewhere |
| Source parity with the shipped v2.2 result | `git diff --name-only origin/main..HEAD` | Single entry `12-RELEASE-CHECKLIST.md` (planning doc). The verified source **is** the shipped source |

`warn_seed_failures` survives intact — same aggregate-`set_message` + per-file stderr-dedup body (now `app.rs:3622-3641`), consumed at both TUI spawn paths (`app.rs:2849-2850` add-session, `app.rs:5322-5323` restart). Line numbers drifted from the stale report (3615→3622, 2843→2850, 5280→5323); the logic did not. The two daemon call sites in `manager.rs` (:1161, :2143) are in a byte-identical file.

**Two evidence-integrity corrections to the prior report**, both found this pass:

1. The prior frontmatter said `status: passed` while its own body said `**Status:** human_needed`. With four non-empty `human_verification` items, the decision tree mandates `human_needed`. Corrected.
2. Every commit hash the phase-9 SUMMARYs cite (`0f84e41`, `807fa47`, `4ffc2f5`, `00b3f9c`) **is not an ancestor of this HEAD** — phase 9's history was rebased. `git merge-base --is-ancestor 0f84e41 HEAD` fails. Subject-identical replacements exist (the WR-01 fix is now `290391f`). The prior report's "all 13 SUMMARY-claimed commits exist" spot-check would no longer reproduce. Nothing below depends on a commit hash: every truth is bound to source at HEAD plus a test run in this verifier's process.

## Goal Achievement

### Observable Truths

All test results below were produced by this verifier at HEAD `ee6fa3c` (targeted runs, per-truth). Suite-wide claims cite the shared run at the same HEAD: `cargo test --workspace --locked -- --test-threads=1` → exit 0, 646 results, 0 failed; `cargo fmt --check` → exit 0; `cargo clippy --workspace --all-targets -- -D warnings` → exit 0.

| # | Truth | Status | Evidence at HEAD |
| --- | ----- | ------ | -------- |
| 1 | (SC1) An unsafe `settings.local.json` (unreadable/unparseable/non-object) survives a spawn attempt byte-identical — never replaced with the seed alone | ✓ VERIFIED | `read_settings_guarded` (hook.rs:402) is the single read path; every `Err` returns before any write (hook.rs:373-378). `seed_guard_unparseable_file_left_untouched_and_warned`, `seed_guard_unreadable_path_left_untouched`, `seed_guard_non_object_root_left_untouched` enumerated by name and green in my `hook::` run (41 passed, 0 failed). File byte-identical to post-WR-01 state |
| 2 | (SC1) An unsafe `.mcp.json` is left byte-identical with a warning naming the file (D-03) | ✓ VERIFIED | `seed_mcp_config` routes through the same shared guard (claude.rs:127); `cargo test -p baude-core --lib backend::claude::` → 9 passed, 0 failed. File byte-identical |
| 3 | (SC1) The user receives an actionable warning naming the file, on every spawn path | ✓ VERIFIED (coincidental-reliance) | All 4 `prepare_cwd` call sites still consume warnings: `app.rs:2849-2850` and `:5322-5323` via `warn_seed_failures`; `manager.rs:1161-1162` and `:2143-2144` via `eprintln!("seed warning: {warning}")`. `cargo test -p baude --bins seed_warning_` → 2 passed. Reliance carried forward (review IN-04, last-message ordering) — see advisory; notably it did NOT break under phases 10-11's 986 added app.rs lines |
| 4 | (SC2) The seeded hook command quotes the executable path so a path with space, `$`, `;`, backtick invokes exactly that executable through a shell | ✓ VERIFIED | `baude_hook_command` Ok arm calls `quote_posix_single` (hook.rs:88-99, :111); E2E `spaced_metachar_path_seed_executes_exact_stub` green in my run — it seeds through the production path, reads the string back off disk, and executes it via real `sh -c` |
| 5 | (SC3) The recognizer matches the quoted form — quoting does not reintroduce HREG-01 per-path accumulation | ✓ VERIFIED | Two-arm `is_seeded_hook_command` (hook.rs:141), unquote-before-Path-checks; `merge_prunes_stale_quoted_seeds_to_one_group` + `merge_converges_legacy_unquoted_seed_to_the_quoted_form` enumerated and green |
| 6 | (SC4) Regression tests: malformed settings, spaced path E2E, leftover-lock reopen, held lock — all exist and pass | ✓ VERIFIED | All four run green by name in my process: `seed_warning_malformed_settings_survives_spawn_attempt` + `seed_warning_non_object_settings_survives_spawn_attempt` (baude, 2 passed), `spaced_metachar_path_seed_executes_exact_stub` (hook), `reopen_claims_lock_when_leftover_file_outlives_released_os_lock` (persist, 1 passed), `held_lock_save_refuses_with_pid_diagnostic` (bauded, 1 passed) |
| 7 | A missing settings file still gets a fresh seed with no warning | ✓ VERIFIED | `NotFound → Ok(json!({}))` (hook.rs:409); `seed_guard_missing_file_fresh_seed_no_warning` + `mcp_guard_missing_file_fresh_seed` green |
| 8 | Refused files stay byte-stable across repeated seed attempts; re-seed is idempotent | ✓ VERIFIED | `seed_guard_refusal_repeats_byte_stable`, `seed_settings_writes_idempotent_merge` green |
| 9 | A user command that merely looks quoted is never claimed as baude's seed (#78 class) | ✓ VERIFIED | Strict round-trip `unquote_posix_single` (hook.rs:123); quoted-arm failure returns false without legacy fall-through; `quoted_look_alikes_and_legacy_forms_keep_their_meaning`, `pure_seed_accepts_quoted_seeds_and_rejects_look_alikes` green |
| 10 | The bare `baude hook` fallback stays unquoted and is never pruned (D-08) | ✓ VERIFIED | `FALLBACK_HOOK_COMMAND` byte-identical (hook.rs:71, :99); `is_pure_seed_group` accepts it explicitly (hook.rs:285) while the recognizer excludes it; pinned by the look-alike test |
| 11 | `.mcp.json` command field stays raw unquoted argv data (D-09) | ✓ VERIFIED | `exe` reaches `merge_mcp_config` unmodified; **zero** `quote_posix_single` occurrences in claude.rs (grep at HEAD); `mcp_command_is_raw_argv_not_shell_quoted` green |
| 12 | A seeding failure never aborts a spawn in either binary (D-04) | ✓ VERIFIED | All four call sites continue unconditionally (no early return, no `Result`); `warn_seed_failures` early-returns on empty and never propagates (app.rs:3623-3625); `seed_guard_write_failure_warns_and_preserves_original` green |
| 13 | Reopening a workspace whose lock file outlives the released OS lock succeeds and re-stamps the current pid (WLOCK-04) | ✓ VERIFIED | persist.rs test green by exact name; file byte-identical to post-WR-01 state |
| 14 | A bauded save under a foreign-held lock surfaces the holder pid + lock path and never removes the other owner's lock file (WLOCK-01/02/03) | ✓ VERIFIED | `manager::tests::held_lock_save_refuses_with_pid_diagnostic` green; file byte-identical |
| 15 | Both lock tests run under Phase-8 isolation, touching no real user root (D-13) | ✓ VERIFIED | `isolated_root` / `ManagerFixture` unchanged (both files byte-identical); the shared full-suite run at this HEAD was green with the Phase-8 guards compiler-enforced |
| 16 | Concurrent/interrupted seeding never truncates or destroys an existing settings file (backstop) | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED (insufficient_spec) | `verification: backstop` — non-inferable. Write is still a single non-atomic `std::fs::write` (hook.rs:381); no test exercises interruption/concurrency. Presence+wiring never qualifies at this tier. Routed to human verification (unchanged from prior pass) |

**Score:** 15/16 truths verified (1 present, behavior-unverified — backstop tier). Identical to the prior pass; no truth moved.

### Required Artifacts

| Artifact | Expected | Status | Details at HEAD |
| -------- | -------- | ------ | ------- |
| `baude-core/src/hook.rs` | SeedWarning/SeedWarningReason, read_settings_guarded, seed_settings→Vec<SeedWarning>, quote_posix_single, two-arm recognizer, full test set | ✓ VERIFIED | All symbols present (:71, :88, :111, :123, :141, :283, :307, :368, :402); `pub struct SeedWarning` contains-pattern matches; 41 `hook::` tests green. Byte-identical to post-WR-01 |
| `baude-core/src/backend/mod.rs` | `prepare_cwd -> Vec<crate::hook::SeedWarning>` | ✓ VERIFIED | mod.rs:114, exact signature; contains-pattern `Vec<crate::hook::SeedWarning>` matches. Byte-identical |
| `baude-core/src/backend/claude.rs` | guarded seed_mcp_config; prepare_cwd collects both seeds | ✓ VERIFIED | claude.rs:75 (`seed_settings(cwd)`), :77 (`extend(seed_mcp_config(cwd))` in prompt mode), :118, :127 (`read_settings_guarded`). Byte-identical |
| `baude-core/src/backend/opencode.rs` | trait ripple: `Vec::new()` body | ✓ VERIFIED | opencode.rs:174. Byte-identical |
| `baude/src/app.rs` | warn surface at add + restart paths; `seed_warning_*` app-level tests | ✓ VERIFIED | **The one phase-9 file later phases edited.** `warn_seed_failures` at :3622-3641 (aggregate + per-file dedup, unchanged), consumed at :2850 and :5323; tests at :10462+ green. Plan's contains-pattern `warn_seed_failure` matches as a substring of the WR-01 rename `warn_seed_failures` |
| `bauded/src/manager.rs` | "seed warning:" eprintln at both daemon spawn paths; held_lock test | ✓ VERIFIED | :1161-1162, :2143-2144; held_lock test green. Byte-identical |
| `baude-core/src/persist.rs` | leftover-lock reopen regression test | ✓ VERIFIED | Test green by exact name. Byte-identical |

### Key Link Verification

| From | To | Via | Status | Details |
| ---- | -- | --- | ------ | ------- |
| hook.rs | backend/claude.rs | ClaudeBackend::prepare_cwd returns seed_settings warnings | ✓ WIRED | claude.rs:75 `crate::hook::seed_settings(cwd)` — plan pattern `seed_settings\(cwd\)` matches |
| backend/mod.rs | baude/src/app.rs | add-session path consumes Vec<SeedWarning> | ✓ WIRED | app.rs:2849-2850 `warn_seed_failures(&seed_warnings)` |
| backend/mod.rs | baude/src/app.rs (restart) | restart path consumes warnings identically | ✓ WIRED | app.rs:5322-5323 — same helper, same order |
| bauded/manager.rs | backend/mod.rs | both daemon call sites consume prepare_cwd's Vec | ✓ WIRED | manager.rs:1161, :2143 — loop + prefixed eprintln; plan pattern `prepare_cwd\(&cwd\)` matches |
| backend/claude.rs | hook.rs | seed_mcp_config reuses hook::read_settings_guarded | ✓ WIRED | claude.rs:127 |
| baude_hook_command | is_seeded_hook_command | quoted string is the idempotency sentinel; round-trip agreement | ✓ WIRED | Both sides route through `quote_posix_single`/`unquote_posix_single`; E2E asserts producer output is recognized |
| is_seeded_hook_command | is_pure_seed_group | purity predicate consumes the recognizer (gates worktree removal, #78) | ✓ WIRED | hook.rs:283-286 |
| manager.rs | persist.rs | save_checked → atomic_save_current → hold_state_lock | ✓ WIRED | held_lock test drives save_checked to StateLockError::Held |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
| -------- | ------------- | ------ | ------------------ | ------ |
| app.rs TUI message | `app.message` | `SeedWarning::Display` ← `read_settings_guarded`'s real io/serde error, joined with `" \| "` | Yes — app-level test asserts real path text in the message after a REAL spawn entry point | ✓ FLOWING |
| app.rs stderr echo | `eprintln!("baude: {warning}")` | same Display, gated by a per-file `BTreeSet` (not one shared flag) | Yes (source-verified; multi-file aggregate → human item 3) | ✓ FLOWING |
| bauded stderr | `eprintln!("seed warning: {warning}")` | same Display, real prepare_cwd return | Yes (source-verified; runtime visibility → human item 2) | ✓ FLOWING |
| settings.local.json | seeded `command` string | `quote_posix_single(current_exe/override)` | Yes — E2E reads it back off disk and executes it under real `sh -c` | ✓ FLOWING |

### Behavioral Spot-Checks

All run in this verifier's own process at HEAD `ee6fa3c`. The full workspace suite was **not** re-run (shared evidence cited instead, per instruction).

| Behavior | Command | Result | Status |
| -------- | ------- | ------ | ------ |
| Guard matrix + quoting + `sh -c` E2E | `cargo test -p baude-core --lib hook::` | 41 passed, 0 failed | ✓ PASS |
| `.mcp.json` guard + raw argv | `cargo test -p baude-core --lib backend::claude::` | 9 passed, 0 failed | ✓ PASS |
| Leftover-lock reopen | `cargo test -p baude-core --lib persist::tests::reopen_claims_lock_when_leftover_file_outlives_released_os_lock` | 1 passed | ✓ PASS |
| App-level malformed-settings spawn | `cargo test -p baude --bins seed_warning_` | 2 passed | ✓ PASS |
| bauded held-lock diagnostic | `cargo test -p bauded --bins manager::tests::held_lock_` | 1 passed | ✓ PASS |
| Named tests for truths 5/9/10 exist | `cargo test -p baude-core --lib hook:: -- --list` | `merge_prunes_stale_quoted_seeds_to_one_group`, `merge_converges_legacy_unquoted_seed_to_the_quoted_form`, `quoted_look_alikes_and_legacy_forms_keep_their_meaning`, `pure_seed_accepts_quoted_seeds_and_rejects_look_alikes` all enumerated | ✓ PASS |
| Later phases did not disturb the seed surface | `git diff 290391f..HEAD -- baude/src/app.rs \| grep -E '^[-+].*(seed\|warn_seed\|prepare_cwd\|SeedWarning)'` | zero matches | ✓ PASS |
| Six of seven phase-9 files unchanged since WR-01 | `git diff --stat 290391f..HEAD -- <7 files>` | only `app.rs` listed | ✓ PASS |
| Workspace suite / fmt / clippy | (shared run at this HEAD, not re-run here) | 646 results 0 failed; fmt exit 0; clippy `-D warnings` exit 0 | ✓ PASS (cited) |

### Probe Execution

No `scripts/*/tests/probe-*.sh` probes exist (`find scripts -path '*/tests/probe-*.sh'` → empty). The phase's declared real-roots gate was executed by the prior pass around its own full-suite run; this pass deliberately did not re-run the full suite (shared evidence + cargo-lock contention), so the bracket was not re-executed. Its subject matter — Phase-8 isolation in the two lock tests — is covered by truth 15: both files are byte-identical to the state the bracket certified, and the shared suite run at this HEAD is green.

| Probe | Command | Result | Status |
| ----- | ------- | ------ | ------ |
| Real-roots bracket around full suite | `assert-real-roots-untouched.sh` | not re-run this pass (full suite intentionally not re-run) | ? SKIP — covered by byte-identity of the two lock-test files + green shared suite |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| ----------- | ----------- | ----------- | ------ | -------- |
| HREG-03 | 09-01, 09-04 | Settings retained unchanged when seeding cannot safely parse/update; actionable warning instead of silent replacement | ✓ SATISFIED | Truths 1-3, 7-8, 12; both clobber sites still share one fail-closed helper. REQUIREMENTS.md:38 `[x]`, tracking table line 113 `Complete` |
| HREG-04 | 09-03 | Launch from a path with spaces/metacharacters; seeded hook invokes that exact executable safely and idempotently | ✓ SATISFIED | Truths 4-5, 9-10; `sh -c` E2E invocation proof re-run green. REQUIREMENTS.md:40 / table line 114 |
| WLOCK-01 | 09-02 | Second-instance contention reported explicitly | ✓ SATISFIED | held_lock test green; startup residual stays documented in REQUIREMENTS.md |
| WLOCK-03 | 09-02 | Diagnostic names workspace/pid/lock path | ✓ SATISFIED | held_lock test asserts pid + lock file name in Display |
| WLOCK-04 | 09-02 | Reopen after OS lock release despite leftover file | ✓ SATISFIED | reopen test green |
| HREG-01, HREG-02, WLOCK-02 | (no plan claims) | Mapped to Phase 9 in REQUIREMENTS.md but pre-delivered (v2.1.2/v2.1.3) | ✓ ACCOUNTED FOR | Not orphaned: HREG-01's no-accumulation is actively preserved by this phase's convergence tests (truth 5); WLOCK-02's never-remove-lock contract is asserted inside the held_lock test; HREG-02 pre-delivered and untouched |

### Prohibitions (must-NOT, all `disposition: flagged-unverified`)

Non-authoritative LLM-judge verdicts, re-bound to tests re-run at this HEAD. Human sign-off still requested (human item 4). None is silently passed.

| # | Prohibition (plan) | Verdict | Enforcement evidence (green in this pass) |
| --- | ---------------- | ------- | -------------------------------------------- |
| 1 | Guard MUST NOT overwrite a settings file it could not read+parse to an object (09-01) | HOLDS (non-authoritative) | Err-before-any-write in `read_settings_guarded` callers; byte-identity `seed_guard_*` / `mcp_guard_*` tests green |
| 2 | Seeding MUST NOT abort a spawn on any read/parse/write failure (09-01, 09-04) | HOLDS (non-authoritative) | No early return at any of the 4 call sites; `seed_warning_*` spawns continue past seeding; `WriteFailed` returns instead of panicking |
| 3 | A test MUST NOT "fix" contention by deleting a lock file (09-02) | HOLDS (non-authoritative) | held_lock test asserts the foreign lock file + stamp survive the refused save; file byte-identical |
| 4 | is_pure_seed_group MUST NOT widen (#78 class) (09-03) | HOLDS (non-authoritative) | Strict round-trip; quoted-arm failure returns false without legacy fall-through; `pure_seed_accepts_quoted_seeds_and_rejects_look_alikes` green |
| 5 | `.mcp.json` permission-mcp command MUST NOT be quoted (09-04) | HOLDS (non-authoritative) | `mcp_command_is_raw_argv_not_shell_quoted` green; zero `quote_posix_single` occurrences in claude.rs at HEAD |
| 6 | Seeding MUST NOT abort — restated in 09-04 | HOLDS (non-authoritative) | Same as #2 |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| — | — | No `TBD`/`FIXME`/`XXX` debt markers in any of the 7 phase-9 files (grep at HEAD, clean) | — | — |

Review Info items IN-01..IN-04 remain ℹ️ Info — none blocks the phase goal. IN-04 is carried as the coincidental-reliance advisory on truth 3, and is worth re-reading now: it predicted that later `set_message` calls on the spawn path would break the app-level seed tests. Phases 10-11 added 986 lines to `app.rs` and the tests still pass, so the prediction did not materialize — but nothing pins the assertion to the guard's distinctive suffix, so the same latent risk rides into every future `app.rs` change.

One planning-doc nit is recorded under `advisory:`: REQUIREMENTS.md:38 labels HREG-03 "(not started)" while its checkbox is `[x]` and line 113 reads `Complete`. Bookkeeping only.

### Human Verification Required

All four items are carried forward from the prior pass — none was resolved, none was dropped.

#### 1. Backstop residual: interrupted/concurrent seed write

**Test:** Kill a spawn mid-seed (or race two seeds) over an existing valid `settings.local.json`.
**Expected:** File never left truncated — or the residual is explicitly accepted (the truth's own wording concedes "best-effort single write of fully merged JSON"; the write at hook.rs:381 is still a non-atomic `std::fs::write`).
**Why human:** backstop-tier truth — non-inferable from presence/wiring; no test exercises interruption. Accepting the residual (or requesting temp+rename atomicity in a later phase) is a human decision.

#### 2. Live bauded stderr visibility

**Test:** Run bauded from a terminal; spawn a session whose cwd holds `{not json` in `.claude/settings.local.json`; watch stderr.
**Expected:** `seed warning: <path>: could not parse settings ... existing settings left untouched ...` per spawn/restart.
**Why human:** 09-04's own coverage marks this human_judgment — wiring is source-verified (manager.rs:1161, :2143, byte-identical file) but no test asserts daemon stderr at runtime, and visibility depends on how bauded is launched.

#### 3. Multi-warning TUI aggregation (WR-01 fix)

**Test:** Prompt mode, cwd with BOTH `settings.local.json` and `.mcp.json` malformed; read the TUI message line.
**Expected:** One aggregate message naming both files (joined `" | "`); both warnings on stderr.
**Why human:** The fix is present and wired at both call sites, but all existing tests still exercise single-warning spawns only — the two-warning aggregate remains unexercised at this HEAD.

#### 4. Prohibition sign-off

**Test:** Confirm the six non-authoritative HOLDS verdicts in the Prohibitions table.
**Expected:** Human checkpoint accepts each (all carry named passing enforcement tests, re-run green this pass).
**Why human:** All six remain `disposition: flagged-unverified`; autonomous verify may never silently pass them.

### Gaps Summary

**No gaps, and no regression.** The specific risk this re-verification was commissioned to investigate — that phases 10, 11 and 12 disturbed phase 9's `app.rs` surface — is falsified deterministically: `warn_seed_failures` and all four `prepare_cwd` consumers are intact, the later `app.rs` diff contains zero seed-related lines, six of the seven phase-9 files are byte-identical to their post-WR-01 state, and no phase-12 commit touches any of them. Every per-truth test was re-run green in this verifier's own process.

The stale `covered_digest` was exactly what it looked like: mechanical drift from `REQUIREMENTS.md` (rewritten by each phase completion) and `app.rs` (grown by phases 10-11). A fresh digest over the same covered set is recorded above.

Status is `human_needed` — not `passed` — for the same four reasons as the prior pass: one backstop-tier truth (interrupted-write residual acceptance), two runtime-visibility judgment checks, one untested multi-warning aggregate, and the mandated prohibition sign-off. None indicates missing or stubbed implementation. This pass also corrects the prior report's frontmatter, which claimed `passed` while its own body said `human_needed`.

---

_Verified: 2026-09-18 at HEAD `ee6fa3c25733f7f499acc11a4ac150ac3bcfa712`_
_Verifier: Claude (gsd-verifier) — re-verification_

---

## Acceptance Record — 2026-09-18

The open `human_verification` items above were presented to the maintainer and
**accepted**, not observed. They are deliberately left listed rather than
deleted: acceptance is a decision about risk, and a later reader is entitled to
see exactly what was accepted and on what basis.

- **Accepted by:** Joe Seymour, 2026-09-18
- **What was accepted:** every open item in this report's `human_verification`
  block, and the backstop-tier truth that remains `behavior_unverified`.
- **What this does NOT mean:** no item was independently observed as a result of
  this acceptance. The verified score is unchanged by it, `overrides_applied`
  stays as recorded, and the backstop truth is still not counted as verified.

- **What IS independently established** (re-verification at HEAD `ee6fa3c`, this
  pass): no regression in any must_have; the full workspace suite green at 646
  results / 0 failed with `cargo fmt --check` and
  `cargo clippy --workspace --all-targets -- -D warnings` both exit 0; and every
  behavioral claim in the body bound to a targeted test run in the verifier's own
  process. Requirements HREG-03, HREG-04 are satisfied on that automated evidence.

This record exists so "passed" is never mistaken for "observed".
