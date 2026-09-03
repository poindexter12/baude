---
phase: 01-full-status-line-capture
verified: 2026-06-15T00:00:00Z
status: human_needed
score: 4/4 must-haves verified
overrides_applied: 0
human_verification:

  - test: "Run baude, select a local session whose bridge file has effort/thinking/pr present (or seed /tmp/baude-usage-<sid>.json with those fields), press `i`."
    expected: "The local info overlay shows `effort`, `thinking` (on/off), and `pr (#N (state))` rows; rows are OMITTED (not shown as `—`) when those fields are absent; no `vim mode` row appears."
    why_human: "No headless ratatui render harness exists in-repo (per plans' human_verify_mode: end-of-phase). The code path is statically verified and wired (ui.rs:871-884 → ClaudeMeta → poll → read_bridge_file), but the on-screen render of a ratatui Paragraph cannot be asserted programmatically without launching the TUI."
audit_acknowledged:
  milestone: v2.0
  at: 2026-09-03
  status: human_needed
---

# Phase 1: Full Status-Line Capture Verification Report

**Phase Goal:** The `baude statusline` bridge becomes the authoritative source for the full useful Claude status-line payload, and the new fields surface in the info overlay.
**Verified:** 2026-06-15
**Status:** human_needed
**Re-verification:** No — initial verification
**Branch verified:** `gsd/phase-01-full-status-line-capture` @ `508a8ac` (working tree clean)

## Goal Achievement

### Observable Truths

| #   | Truth | Status | Evidence |
| --- | ----- | ------ | -------- |
| 1   | After a managed session runs, `/tmp/baude-usage-<sessionId>.json` contains model, effort, thinking, pr, worktree, and vim.mode (each present only when emitted) alongside cost/context/rate-limit fields | ✓ VERIFIED | `build_bridge` (bridge.rs:61-107) emits all six new keys + `schema:2` + legacy fields. Runtime spot-check: piped a payload through the real `baude statusline` binary → file contained `model`, `effort:"high"`, `thinking:true`, nested `pr`, nested `worktree`, `vim_mode:"NORMAL"`, `cost_usd:2.5`, with `url`/`path` null because not supplied (present-only-when-emitted holds). 7 bridge tests pass. |
| 2   | The bridge JSON carries `schema: 2` and an older reader still parses it without error — new fields optional/additive | ✓ VERIFIED | `"schema": 2` stamped (bridge.rs:89); spot-check file showed `schema:2`. Reader uses untyped `Value` accessors only — no `#[derive(Deserialize)]`, no branch on `schema`. `reads_legacy_bridge` (new reader reads old file → new fields None) and `does_not_branch_on_schema` (schema:99 still read) both pass. WR-01/WR-02 fixes did not regress: `reads_legacy_bridge` + `present_then_absent_bridge_fields_clear` + `model_bridge_wins_then_survives` all green. |
| 3   | Mixed snake_case/camelCase payloads are both parsed (window() tolerance) | ✓ VERIFIED | `window()` (bridge.rs:32-48) unchanged, tries snake then camel for the object, `used_percentage`/`utilization` and `resets_at`/`resetsAt` leaves. `pr.review_state` has a defensive `.or_else(p["reviewState"])` (bridge.rs:68-70). `snake_camel_tolerated` test asserts `fiveHour`/`utilization`/`resetsAt` and `pr.reviewState` all parse. |
| 4   | Selecting a session and pressing `i` shows effort, thinking mode, and PR state in the LOCAL info overlay (vim.mode captured but NOT rendered; remote out of scope) | ✓ VERIFIED (code path) / ⚠️ render = human-check | Local `Modal::Info` branch (`app.selected()`, ui.rs:834+) pushes conditional `effort`/`thinking`/`pr` rows (ui.rs:871-884) guarded by `if let Some`. No `vim_mode` row anywhere in ui.rs. Remote branch (ui.rs:780-832) untouched — no new rows. Data flows: session.rs:139 `meta.poll()` → read_bridge_file (meta.rs:124) populates effort/thinking/pr → overlay reads `s.meta`. **On-screen render is the human-check item** (no headless ratatui harness). |

**Score:** 4/4 truths verified (criterion 4 code-complete and wired; visual render routed to human verification).

### Required Artifacts

| Artifact | Expected | Status | Details |
| -------- | -------- | ------ | ------- |
| `baude-core/src/bridge.rs` | `build_bridge(v: &Value) -> Value`; schema:2; six new captured fields; `#[cfg(test)] mod tests` | ✓ VERIFIED | `fn build_bridge` at line 61, called by `run()` at line 118. All six fields + schema:2 present. 7 tests in `mod tests` (line 142). |
| `baude-core/src/meta.rs` | `PrInfo`/`WorktreeInfo`; new Option fields on ClaudeMeta; extended `read_bridge_file`; tests | ✓ VERIFIED | `struct PrInfo` (line 56), `struct WorktreeInfo` (line 64); fields effort/thinking/vim_mode/pr/worktree on ClaudeMeta (lines 105-113); `read_bridge_file` reads them (lines 303-324); 7 tests in `mod tests` (line 481). |
| `baude/src/ui.rs` | effort/thinking/pr conditional rows in local Modal::Info branch | ✓ VERIFIED | `row("effort"...)`, `row("thinking"...)`, `row("pr"...)` at lines 871-884, in the `app.selected()` local branch, guarded by `if let Some`. |

### Key Link Verification

| From | To | Via | Status | Details |
| ---- | -- | --- | ------ | ------- |
| bridge.rs::run | bridge.rs::build_bridge | `run()` writes `build_bridge(&v).to_string()` to `bridge_path(sid)` | ✓ WIRED | bridge.rs:118 |
| build_bridge | statusLine nested objects | indexes `effort.level`, `pr.*`, `thinking.enabled`, `model.display_name`, `vim.mode` | ✓ WIRED | bridge.rs:97-105; `nested_read_not_scalar` test proves `effort.level` indexing |
| meta.rs::read_bridge_file | bridge file keys | reads effort/thinking/vim_mode/pr/worktree/model via `.as_*()` | ✓ WIRED | meta.rs:303-324 |
| read_bridge_file | self.model | `if let Some(m) = v["model"]` — bridge wins, transcript survives otherwise | ✓ WIRED | meta.rs:310-312; precedence preserved, NOT made unconditional (correct) |
| ui.rs local Modal::Info | ClaudeMeta.effort/thinking/pr | conditional `lines.push(row(...))` guarded by `if let Some(pr) = &m.pr` etc. | ✓ WIRED | ui.rs:877 |
| session.rs::poll | meta.rs::poll → read_bridge_file | local session refresh drives the reader | ✓ WIRED | session.rs:139 → meta.rs:124 |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
| -------- | ------------- | ------ | ------------------ | ------ |
| ui.rs overlay rows | `m.effort` / `m.thinking` / `m.pr` | `s.meta` ← `meta.poll()` ← `read_bridge_file()` ← `/tmp/baude-usage-<sid>.json` ← `build_bridge()` | ✓ Yes — runtime spot-check confirmed the writer emits real nested values; reader populates the same keys; overlay reads them from `s.meta` | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| -------- | ------- | ------ | ------ |
| `baude statusline` writes full bridge file | piped a full payload through `cargo run -p baude -- statusline`, inspected `/tmp/baude-usage-<sid>.json` | File contained `schema:2`, model, effort, thinking, nested pr, nested worktree, vim_mode, cost_usd; absent leaves (`url`/`path`) null | ✓ PASS |
| Bridge writer tests | `cargo test -p baude-core` (bridge module) | 7/7 pass | ✓ PASS |
| Reader tests incl. back-compat + WR fixes | `cargo test -p baude-core` (meta module) | 7/7 pass | ✓ PASS |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| ----------- | ----------- | ----------- | ------ | -------- |
| STL-01 | 01-01 | Bridge persists full payload (model/effort/thinking/pr/worktree/vim.mode), optional, snake/camel tolerant | ✓ SATISFIED | bridge.rs:88-106; runtime spot-check + 7 tests |
| STL-02 | 01-01, 01-02 | Bridge JSON versioned `schema:2`; meta.rs reader gains optional fields without breaking existing readers | ✓ SATISFIED | bridge.rs:89; meta.rs:303-324; back-compat tests both directions; WR-01/WR-02 fixes present and green |
| STL-03 | 01-03 | `i` overlay surfaces effort/thinking/PR for selected session | ✓ SATISFIED (code) / human-render | ui.rs:871-884 local branch; render is the human-check item |

All three declared requirement IDs accounted for. REQUIREMENTS.md traceability maps STL-01/02/03 to Phase 1 only — no orphaned requirements for this phase.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| — | — | None | — | No TODO/FIXME/XXX/TBD/HACK/PLACEHOLDER/unimplemented markers in any of the three modified files. No stub returns; all `Option` reads are real accessor logic. |

### Review Findings Follow-Up

| Finding | Status | Evidence |
| ------- | ------ | -------- |
| WR-01 (stale bridge fields never cleared) | ✓ FIXED | commit 6d75bd8; meta.rs:303-324 now assign unconditionally so absent/null clears; `present_then_absent_bridge_fields_clear` test (meta.rs:611-653) passes; `model` correctly kept on the `if let Some` guard (precedence intact) |
| WR-02 (precedence test bypassed real transcript path) | ✓ FIXED | commit 64a8fe2; `model_bridge_wins_then_survives` (meta.rs:677-721) now drives through `feed_transcript`→`read_transcript_tail`, guarding poll() ordering; passes |
| IN-01 (cosmetic row grouping) | Info — not addressed | Non-blocking; rows sit below the separator. Acceptable. |
| IN-02 (worktree/vim_mode captured, not rendered) | Intentional | Locked "capture-but-don't-render" scope; confirmed no vim_mode/worktree row in ui.rs |
| IN-03 (no wrong-type reader test) | Info — not addressed | Production code handles via `.as_*()`/`is_object()` returning None; non-blocking |

Neither fix regressed STL-02 back-compat: `reads_legacy_bridge`, `does_not_branch_on_schema`, and `model_bridge_wins_then_survives` all pass.

### CI Gates

| Gate | Command | Result |
| ---- | ------- | ------ |
| Format | `cargo fmt --check` | ✓ clean (exit 0) |
| Lint | `cargo clippy --all-targets -- -D warnings` | ✓ clean (zero warnings) |
| Tests | `cargo test` | ✓ 16 baude-core (7 bridge + 7 meta + 2 pty) + 29 daemon/api/etc. all pass; 0 failed |

### Human Verification Required

#### 1. Local `i` info overlay renders effort/thinking/pr

**Test:** Run baude, select a local session whose bridge file has effort/thinking/pr present (or seed `/tmp/baude-usage-<sid>.json` with those fields for the selected session), then press `i`.
**Expected:** The overlay shows `effort`, `thinking` (on/off), and `pr (#N (state))` rows; rows are OMITTED (not shown as `—`) when those fields are absent; no `vim mode` row appears; the remote-session info overlay is unchanged.
**Why human:** No headless ratatui render harness exists in-repo (plans set `human_verify_mode: end-of-phase` for exactly this). The code path, wiring, and data flow are all statically verified and the underlying values are confirmed to flow end-to-end, but the actual on-screen ratatui render must be confirmed visually.

### Gaps Summary

No gaps. All four success criteria are met in code, all three requirement IDs satisfied, all three CI gates green, both code-review warnings (WR-01, WR-02) fixed without regressing STL-02 back-compat, and a runtime spot-check confirmed the bridge writer emits the full payload to disk. The only open item is the visual render of the `i` overlay (criterion 4), which cannot be asserted without launching the TUI — routed to human verification per the phase's documented end-of-phase human-verify mode. Status is `human_needed`, not `passed`, solely because that human-check item exists.

---

_Verified: 2026-06-15_
_Verifier: Claude (gsd-verifier)_
