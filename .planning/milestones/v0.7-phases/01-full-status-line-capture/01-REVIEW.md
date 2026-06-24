---
phase: 01-full-status-line-capture
reviewed: 2026-06-15T00:00:00Z
depth: standard
files_reviewed: 3
files_reviewed_list:
  - baude-core/src/bridge.rs
  - baude-core/src/meta.rs
  - baude/src/ui.rs
findings:
  critical: 0
  warning: 2
  info: 3
  total: 5
status: issues
---

# Phase 1: Code Review Report

**Reviewed:** 2026-06-15
**Depth:** standard (per-file analysis with Rust-specific checks)
**Files Reviewed:** 3
**Status:** issues_found

## Summary

The diff faithfully implements the research design. The back-compat constraint (STL-02) is met cleanly: no `#[derive(Deserialize)]` on the bridge file, no reader branching on `schema`, every new field optional, and all reads go through `serde_json::Value` `.as_*()` accessors that yield `None` on absent/wrong-type rather than panicking. Nested-object indexing is correct (`effort.level`, `pr.number`, `thinking.enabled`, etc.). The snake/camel tolerance is preserved, the v2.1.177 version-pin comment is present, and the model-precedence guard correctly preserves a transcript-derived model when the bridge omits it.

I verified the build is clippy-clean (`cargo clippy -p baude-core -p baude --all-targets -- -D warnings`) and all 13 new tests pass (`cargo test -p baude-core`). I traced `poll()` ordering to confirm the load-bearing "transcript read before bridge" claim (meta.rs:120 then :124) — it holds.

No Critical issues. Two Warnings concern stale state on the reader side (bridge-derived fields are never cleared once set) and a missing precedence test through the real transcript path. Three Info items cover layout grouping, an unrendered field, and a missing edge-case test.

## Warnings

### WR-01: Bridge-derived fields are never cleared — stale `pr`/`effort`/`thinking`/`vim_mode` linger after they disappear from the payload

**File:** `baude-core/src/meta.rs:295-327`
**Issue:** `read_bridge_file` only ever *sets* the new fields (`self.effort = Some(...)`, `self.pr = Some(...)`, etc.) inside `if let Some(...)` / `if p.is_object()` guards. It never resets them to `None`. `poll()` calls `read_bridge_file` on every tick, but `effort`, `thinking`, `vim_mode`, `pr`, and `worktree` are not reset anywhere in the poll cycle (unlike `model`/`permission_mode`, which `resolve_transcript` explicitly clears on a transcript switch at meta.rs:195-196). The research's own Pitfall 2 flags that `pr` "disappears on merge/close" and `effort`/`vim` come and go with session state. Concretely: once a PR is captured, then merged (Claude stops emitting `pr`), the next bridge write produces `"pr": null`; `v["pr"].is_object()` is false, so `self.pr` retains the old, now-incorrect PR number/state and the info overlay keeps showing a closed PR for the life of the session.

This matches the *existing* convention for the legacy bridge fields (`session_cost_usd`, `context_used_pct`, `rate_5h`) which are also set-only, so it is not a regression in pattern — but `pr` presence is semantically meaningful state (open vs. gone), where staleness produces visibly wrong UI rather than benign lag. Classified Warning, not Critical: no crash or data loss, trusted local writer, self-heals only if Claude re-emits.

**Fix:** Mirror the source-of-truth nature of the bridge by assigning unconditionally (let the accessor's `None` clear the field), e.g.:
```rust
// Each field tracks the bridge verbatim: present overwrites, absent clears.
self.effort = v["effort"].as_str().map(str::to_string);
self.thinking = v["thinking"].as_bool();
self.vim_mode = v["vim_mode"].as_str().map(str::to_string);

let p = &v["pr"];
self.pr = p.is_object().then(|| PrInfo {
    number: p["number"].as_u64(),
    url: p["url"].as_str().map(str::to_string),
    review_state: p["review_state"].as_str().map(str::to_string),
});
let w = &v["worktree"];
self.worktree = w.is_object().then(|| WorktreeInfo {
    name: w["name"].as_str().map(str::to_string),
    path: w["path"].as_str().map(str::to_string),
    branch: w["branch"].as_str().map(str::to_string),
});
```
Note `model` must stay on the `if let Some` guard (the bridge-wins-else-keep-transcript precedence depends on it). If this fix lands, add a regression test: capture a `pr`, then read a second bridge file with `"pr": null`, assert `meta.pr.is_none()`.

### WR-02: Precedence test never exercises the real transcript→bridge path it claims to guard

**File:** `baude-core/src/meta.rs:619-639` (`model_bridge_wins_then_survives`)
**Issue:** The test sets `model: Some("from-transcript")` directly on the struct, then calls `read_bridge_file`. It never runs `read_transcript_tail`, so it does not actually verify the load-bearing ordering claim in the production comment (meta.rs:306-308: "poll() reads the transcript before the bridge"). If someone later reorders `poll()` (e.g. moves `read_bridge_file` before `read_transcript_tail`), the bridge value would be clobbered by the transcript at meta.rs:244-245 with no test failure. The test asserts the `read_bridge_file` half of the contract but leaves the `poll()` ordering — the part most likely to silently break — unprotected.

**Fix:** Add a test that drives the ordering through `poll()` (or at minimum a sequenced `read_transcript_tail` then `read_bridge_file`) with both a transcript-emitted `model` and a bridge `model`, asserting the bridge value wins; and a second case where the bridge omits `model`, asserting the transcript value survives. This pins the ordering, not just the final accessor.

## Info

### IN-01: New rows land below the identity/usage separator, regrouping the overlay

**File:** `baude/src/ui.rs:869-884`
**Issue:** The `effort`/`thinking`/`pr` rows are pushed *after* the `Line::raw("")` separator at ui.rs:869, which previously sat between the identity block (session/model/permissions/context) and the usage block (last turn/session total). The new rows now appear below that blank line, visually grouping effort/thinking/pr with the token-usage section rather than with model/permissions where they conceptually belong. Purely cosmetic; the overlay auto-sizes from `lines.len()`.

**Fix:** Insert the three rows *before* the `Line::raw("")` at :869 (i.e., push them right after the `claude session` row) so they group with the identity fields, then keep the blank separator ahead of the usage block.

### IN-02: `worktree` and `vim_mode` are captured but never surfaced anywhere

**File:** `baude-core/src/meta.rs:312-327`, `bridge.rs:105`
**Issue:** `worktree` (full object) and `vim_mode` are parsed and stored in `ClaudeMeta` but read by nothing — not the local overlay, not the remote path. This is intentional per the locked "capture-but-don't-render" scope (research Open Question 2, and the inline comment at bridge.rs:104), so it is not a defect. Flagged only so a future reviewer does not mistake the dead read for an oversight. No action required this phase; revisit when Tier-2 remote parity lands.

**Fix:** None for Phase 1. When surfaced later, add a `worktree`/`vim` row and a corresponding read site.

### IN-03: No test for malformed nested shape (wrong-type leaf) on the reader side

**File:** `baude-core/src/meta.rs:482-639` (test module)
**Issue:** The bridge-writer tests cover empty-object and minimal payloads, but the *reader* tests (`meta::tests`) never feed a bridge file where a new field has the wrong JSON type (e.g. `"effort": 5`, `"pr": "not-an-object"`, `"thinking": "yes"`). The production code handles these correctly via `.as_str()`/`.as_bool()`/`.is_object()` returning `None`/false, but there is no regression guard proving a type-confused bridge file yields `None` rather than a surprise. Cheap insurance given the "untrusted-ish input" classification in the research Security Domain.

**Fix:** Add a reader test with a bridge file like `{"schema":2,"effort":5,"thinking":"yes","pr":"x","worktree":42}` asserting all five fields end up `None`/absent and no panic.

---

_Reviewed: 2026-06-15_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
