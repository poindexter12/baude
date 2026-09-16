---
phase: 09-hook-seeding-safety
reviewed: 2026-09-15T00:00:00Z
depth: standard
files_reviewed: 7
files_reviewed_list:
  - baude-core/src/backend/claude.rs
  - baude-core/src/backend/mod.rs
  - baude-core/src/backend/opencode.rs
  - baude-core/src/hook.rs
  - baude-core/src/persist.rs
  - baude/src/app.rs
  - bauded/src/manager.rs
findings:
  critical: 0
  warning: 1
  info: 4
  total: 5
status: issues_found
---

# Phase 9: Code Review Report

**Reviewed:** 2026-09-15
**Depth:** standard
**Files Reviewed:** 7
**Status:** issues_found

## Summary

Reviewed the HREG-03 unsafe-settings guard (`SeedWarning`, `read_settings_guarded`, guarded `seed_settings`/`seed_mcp_config`, warning surfaces on all four spawn paths) and HREG-04 quoting (`quote_posix_single`, quoted producer arm, both-form recognizer), plus the WLOCK regression tests, against `git diff bedb32d..HEAD`.

The safety-critical invariants hold under adversarial tracing:

- **#78 direction-of-error is correct.** `is_seeded_hook_command` requires `strip_suffix(" hook")` + either a strict `unquote_posix_single` round-trip (quoted arm) or a raw absolute path (legacy arm), then an absolute-path + `baude`/`bauded` file-stem check. I traced the hostile classes by hand: doubled outer quotes (`''/opt/baude''` → inner `'/opt/baude'` re-quotes to a different string → rejected), raw un-escaped inner quote (rejected by round-trip), unterminated quote (falls to legacy arm, not absolute → rejected), lone-quote remainder `'` (len < 2 → legacy arm → rejected), empty quoted `''` (unquotes to `""` → not absolute → rejected). A malformed quoted look-alike does NOT fall through to the legacy arm (explicit `return false`), so it can never be pruned or counted toward the removal exemption. Every failure mode errs toward "not ours" — the safe direction for `is_pure_seed_group`.
- **The guard never overwrites what it cannot parse.** `read_settings_guarded` is the single read path for both `settings.local.json` and `.mcp.json`; `NotFound` is the only branch that proceeds to a write, and every `Err` returns before any filesystem mutation. The `.claude`-as-file case surfaces as `Unreadable` (NotADirectory), not a clobber.
- **Seeding never aborts a spawn.** All four call sites (`app.rs` add + restart, `manager.rs` spawn + restart) consume `Vec<SeedWarning>` and continue unconditionally.
- **`.mcp.json` command stays raw argv** (D-09): `exe` reaches `merge_mcp_config` unquoted, and `mcp_command_is_raw_argv_not_shell_quoted` pins it.
- **`merge_hook_settings` no-clobber posture survives the new pruning**: non-object roots coerced only in the returned value (original file already guarded upstream), non-array event values skipped, mixed/matcher/user groups excluded by `seeded_group_command`'s exact-shape check (`obj.len() != 1`, `entry.len() != 2`), indexing on odd shapes returns `Null` rather than panicking.

One warning on the TUI warning-surface losing warnings, plus four informational items.

## Warnings

### WR-01: Multi-warning spawns lose warnings on both TUI and stderr surfaces

**Status:** FIXED (commit 0f84e41) — `warn_seed_failure` replaced by `warn_seed_failures(&[SeedWarning])`: one aggregate `set_message` joining every warning with `" | "` (no last-wins loss), and the stderr once-guard re-keyed per affected file via a `Mutex<BTreeSet<PathBuf>>` (a later warning about a different file always reaches stderr). Both call sites (~2841, ~5262) updated; `seed_warning_` integration tests green unchanged in substance.
**File:** `baude/src/app.rs:3607-3618` (also call sites at ~2841 and ~5262)
**Issue:** `warn_seed_failure` has two lossy behaviors that compound when one `prepare_cwd` returns more than one warning (possible in prompt mode: a `settings.local.json` warning AND a `.mcp.json` warning in the same spawn):

1. `self.set_message(warning.to_string())` is last-wins — the loop calls it once per warning, so only the final warning (the `.mcp.json` one) is ever visible in the TUI. The `settings.local.json` warning is overwritten before a single frame renders it.
2. The `static WARNED: AtomicBool` dedups stderr across ALL seed warnings for the process lifetime, keyed on nothing. So the first warning ever (say, workspace A's `settings.local.json`) claims the flag, and a later, entirely different warning (workspace B's `.mcp.json`, hours later in the same TUI process) never reaches stderr at all — and if it also gets overwritten per (1), it can be lost from both surfaces.

The doc comment says "the once-flag is shared by ALL seed warnings; only the stderr echo is deduplicated," so behavior (2) is intentional — but combined with (1), the stated D-02 goal ("the operator must SEE it") is not met for the second warning of a two-warning spawn on the stderr surface, and not met for the first warning on the TUI surface. Contrast with `bauded`, which deliberately prints every warning every time.
**Fix:** Aggregate before surfacing so neither channel drops a warning:

```rust
fn warn_seed_failures(&mut self, warnings: &[baude_core::hook::SeedWarning]) {
    if warnings.is_empty() {
        return;
    }
    for warning in warnings {
        eprintln!("baude: {warning}"); // or dedup keyed per warning.file
    }
    let combined = warnings
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" | ");
    self.set_message(combined);
}
```

If the once-per-process stderr dedup must stay, key it on `warning.file` (e.g. a `Mutex<HashSet<PathBuf>>`) instead of a single bool, so a distinct file's warning is never silently swallowed.

## Info

### IN-01: `seed_mcp_config` skips silently on `current_exe()` failure — asymmetric with D-02

**File:** `baude-core/src/backend/claude.rs:119-123`
**Issue:** When `current_exe()` fails, `seed_mcp_config` returns `Vec::new()` — no warning, no `.mcp.json` seed, so a prompt-mode session silently loses the permission-MCP bridge. Every other seeding failure in this phase became visible per D-02; this one stayed a silent skip (pre-existing posture, but the phase's stated goal was "the operator must SEE it"). Note `baude_hook_command` handles the same failure by seeding the `baude hook` fallback rather than skipping.
**Fix:** Add a `SeedWarningReason::ExeUnresolved` (or reuse `Unreadable`) variant and return one warning naming `.mcp.json`, so prompt-mode operators learn why permission prompts never arrive.

### IN-02: Warning-surfacing loop duplicated verbatim in bauded

**File:** `bauded/src/manager.rs:1161-1163` and `bauded/src/manager.rs:2141-2145`
**Issue:** The identical three-line `for warning in be.prepare_cwd(&cwd) { eprintln!("seed warning: {warning}"); }` block appears in both `spawn_with_mode_internal` and the restart path. A future change to the daemon's surface format (e.g. adding the session id, which would materially help an operator running many sessions) must be made twice.
**Fix:** Extract a `fn surface_seed_warnings(be: &dyn Backend, cwd: &Path)` helper or a free function taking the `Vec<SeedWarning>`.

### IN-03: Manager test hand-mirrors persist's private lock-path format

**File:** `bauded/src/manager.rs:3046-3053` (`held_lock_save_refuses_with_pid_diagnostic`)
**Issue:** The test reconstructs the lock path as `".{file_name}.lock"` with a comment admitting it mirrors `persist`'s private `lock_path`. If persist ever changes the format, this test's `holder` locks a file nobody contends on, `save_checked` succeeds, and `unwrap_err()` panics with a confusing message rather than a clear "format drifted" failure — a test-reliability coupling across crate boundaries.
**Fix:** Expose the path shape to tests (e.g. a `#[cfg(feature = "test-support")] pub fn lock_path_for_test(destination: &Path) -> PathBuf` in persist) or assert the format assumption first with a targeted message.

### IN-04: App-level seed-warning test binds to last-message state after a full spawn attempt

**File:** `baude/src/app.rs:9437-9505` (`assert_seed_warning_survives_spawn_attempt`)
**Issue:** The assertion reads `app.message` AFTER `add_standalone_session_with_mode` completes. Any future code on that spawn path that calls `set_message` after `prepare_cwd` (a PTY-failure message, a status line) will overwrite the seed warning and break this test for an unrelated reason — or worse, a future message containing the path coincidentally keeps it green. It passes today only because the stubbed `true` command's failure path happens not to message.
**Fix:** Assert immediately that the message contains the guard's distinctive suffix (e.g. `"existing settings left untouched"`) in addition to the path, and add a comment pinning the "no later set_message on this path" assumption so a legitimate overwrite is diagnosed quickly.

---

_Reviewed: 2026-09-15_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
