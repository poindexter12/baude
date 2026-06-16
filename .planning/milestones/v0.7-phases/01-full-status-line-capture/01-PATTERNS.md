# Phase 1: Full Status-Line Capture - Pattern Map

**Mapped:** 2026-06-15
**Files analyzed:** 3 modified (no net-new source files)
**Analogs found:** 3 / 3 (all in-file analogs — every change extends an existing, well-factored pattern in the same file)

## File Classification

| Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---------------|------|-----------|----------------|---------------|
| `baude-core/src/bridge.rs` | service (CLI capture/writer) | transform (stdin JSON → bridge file JSON) | **in-file**: `window()` helper + `json!{}` writer at `bridge.rs:32-67` | exact |
| `baude-core/src/meta.rs` | model + reader | file-I/O (bridge file → `ClaudeMeta`) | **in-file**: `read_bridge_file()` rate-window reads at `meta.rs:243-269` | exact |
| `baude/src/ui.rs` | component (TUI overlay) | request-response (render `ClaudeMeta` → ratatui lines) | **in-file**: local `Modal::Info` `lines` builder at `ui.rs:834-895` | exact |

**Key insight for the planner:** This phase has no cross-file analog hunting to do. Every new field follows a pattern that already exists *within the same file being edited*. The job is to mirror the local idiom, not import a foreign one. The nested-object analog the planner most needs — "how is an existing nested sub-object parsed and persisted" — is the rate-limit window (`rate_limits.five_hour.{used_percentage,resets_at}`), handled by `window()` on the writer side and the `window` closure on the reader side. Copy those shapes for the new nested fields (`model.display_name`, `effort.level`, `thinking.enabled`, `pr.{number,url,review_state}`, `worktree.{name,path,branch}`, `vim.mode`).

## Pattern Assignments

### `baude-core/src/bridge.rs` (service, transform) — STL-01 + STL-02 schema stamp

**Analog:** in-file `window()` (lines 30-48) and the `json!{}` bridge writer (lines 57-69).

**Imports pattern** (lines 19-24) — already present, no new imports needed; `Value` is already in scope for the nested-object guards (`Value::Null`):
```rust
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use serde_json::{json, Value};
use crate::meta::now_unix_ms;
```

**Nested-object parse analog — the load-bearing pattern** (lines 30-48). This is the closest existing analog for the new nested fields. Note: it (a) checks the sub-object exists before indexing, (b) returns `Value::Null` when absent, (c) uses snake-first / camel-fallback `.or_else()` per leaf:
```rust
fn window(v: &Value, snake: &str, camel: &str) -> Value {
    let w = if v["rate_limits"][snake].is_object() {
        &v["rate_limits"][snake]
    } else {
        &v["rate_limits"][camel]
    };
    if !w.is_object() {
        return Value::Null;
    }
    let pct = w["used_percentage"]
        .as_f64()
        .or_else(|| w["utilization"].as_f64());
    json!({
        "used_pct": pct,
        "resets_at": w["resets_at"].as_u64().or_else(|| w["resetsAt"].as_u64()),
    })
}
```
Mirror this `is_object()`-guard + `Value::Null`-fallback + `json!{}` shape for the new `pr` and `worktree` objects. Scalar nested leaves (`model.display_name`, `effort.level`, `thinking.enabled`, `vim.mode`) need no guard — index straight to the leaf with `.as_*()`, which yields `None`/`null` if any segment is absent.

**Additive `json!{}` writer analog** (lines 57-68) — extend this literal in place; do not restructure. Add `"schema": 2` and the six new fields alongside the existing four. Keep everything inside the existing best-effort `if let Ok(v) = ...` / `if let Some(sid) = ...` block (lines 57-58) so capture failure never aborts the `--wrap` delegation (lines 71-87):
```rust
if let Ok(v) = serde_json::from_str::<Value>(&input) {
    if let Some(sid) = v["session_id"].as_str() {
        let bridge = json!({
            "session_id": sid,
            "updated_unix_ms": now_unix_ms(),
            "cost_usd": v["cost"]["total_cost_usd"].as_f64(),
            "context_used_pct": v["context_window"]["used_percentage"].as_f64(),
            "five_hour": window(&v, "five_hour", "fiveHour"),
            "seven_day": window(&v, "seven_day", "sevenDay"),
        });
        let _ = std::fs::write(bridge_path(sid), bridge.to_string());
    }
}
```

**Target shape after extension** (per RESEARCH.md Code Examples, grounded in the above):
```rust
let pr = {
    let p = &v["pr"];
    if p.is_object() {
        json!({
            "number": p["number"].as_u64(),
            "url": p["url"].as_str(),
            "review_state": p["review_state"].as_str().or_else(|| p["reviewState"].as_str()),
        })
    } else { Value::Null }
};
let worktree = {
    let w = &v["worktree"];
    if w.is_object() {
        json!({ "name": w["name"].as_str(), "path": w["path"].as_str(), "branch": w["branch"].as_str() })
    } else { Value::Null }
};
let bridge = json!({
    "schema": 2,
    "session_id": sid,
    "updated_unix_ms": now_unix_ms(),
    "cost_usd": v["cost"]["total_cost_usd"].as_f64(),
    "context_used_pct": v["context_window"]["used_percentage"].as_f64(),
    "five_hour": window(&v, "five_hour", "fiveHour"),
    "seven_day": window(&v, "seven_day", "sevenDay"),
    "model": v["model"]["display_name"].as_str().or_else(|| v["model"]["id"].as_str()),
    "effort": v["effort"]["level"].as_str(),
    "thinking": v["thinking"]["enabled"].as_bool(),
    "pr": pr,
    "worktree": worktree,
    "vim_mode": v["vim"]["mode"].as_str(),
});
```

**Test pattern** (Wave 0 — `bridge.rs` has no `#[cfg(test)]` module today). Add one mirroring the workspace idiom from `bauded/src/transcript.rs:255-320`: inline raw-string JSON fixtures (`r#"{...}"#`), temp files keyed by `std::process::id()`, `std::fs::remove_dir_all(&dir).ok()` teardown. To exercise `run()`'s writer without stdin, factor the `json!{}` build into a small testable `fn build_bridge(v: &Value) -> Value` and assert on its output (full payload, minimal payload → nulls/absent, snake/camel tolerance, `schema == 2`).

---

### `baude-core/src/meta.rs` (model + reader, file-I/O) — STL-02

**Analog:** the `ClaudeMeta` struct (lines 64-88), the `RateWindow` sub-struct (lines 46-52), and the `read_bridge_file()` reader (lines 243-269).

**Optional-field struct analog** (lines 64-88) — every field on `ClaudeMeta` is already `Option<...>` (or a `Default` sub-struct). Add the new fields the same way. `#[derive(Default)]` on the struct means new `Option` fields need no manual init:
```rust
#[derive(Default)]
pub struct ClaudeMeta {
    pub model: Option<String>,
    pub permission_mode: Option<String>,
    pub session_cost_usd: Option<f64>,
    pub rate_5h: Option<RateWindow>,
    pub rate_week: Option<RateWindow>,
    // ... add: effort, thinking, vim_mode, pr, worktree (all Option/Default)
}
```

**Sub-struct analog for `pr`/`worktree`** — `RateWindow` (lines 46-52) is the in-file template for a small typed reader-side sub-struct. Note `#[derive(Default, Clone, Copy)]` and all-`Option` fields:
```rust
#[derive(Default, Clone, Copy)]
pub struct RateWindow {
    pub used_pct: Option<f64>,
    pub resets_at_unix_s: Option<u64>,
}
```
Model `PrInfo`/`WorktreeInfo` on this (use `Clone` not `Copy` since they hold `Option<String>`). A typed sub-struct is fine on the reader side — the back-compat constraint is about the on-disk `Value` parsing, not `ClaudeMeta`'s in-memory shape.

**Reader analog — the load-bearing pattern** (lines 243-269). This is the closest existing analog for reading new fields from the bridge file. Note the early-return guards, the `if let Some(...)` per-field reads (absent/null → field stays `None`, no panic), and especially the nested-object `window` closure that reads a sub-object's leaves:
```rust
fn read_bridge_file(&mut self) {
    let Some(sid) = &self.session_id else { return; };
    let Some(v) = read_json(&PathBuf::from(crate::bridge::bridge_path(sid))) else { return; };
    if let Some(cost) = v["cost_usd"].as_f64() {
        self.session_cost_usd = Some(cost);
    }
    if let Some(pct) = v["context_used_pct"].as_f64() {
        self.context_used_pct = Some((pct.round() as u64).min(100) as u8);
    }
    self.rate_updated_unix_ms = v["updated_unix_ms"].as_u64().unwrap_or(0);
    let window = |w: &Value| -> Option<RateWindow> {
        w.is_object().then(|| RateWindow {
            used_pct: w["used_pct"].as_f64(),
            resets_at_unix_s: w["resets_at"].as_u64(),
        })
    };
    if let Some(w) = window(&v["five_hour"]) {
        self.rate_5h = Some(w);
    }
    if let Some(w) = window(&v["seven_day"]) {
        self.rate_week = Some(w);
    }
}
```
Append the new scalar reads (`effort`, `thinking`, `vim_mode`) as `if let Some(...) = v[..].as_*()` blocks, and read `pr`/`worktree` with an `if v[..].is_object()` guard exactly like the `window` closure does. **Do NOT branch on `schema`** — read every field optionally regardless (a `schema:1` file must still yield the four legacy fields).

**Model precedence — explicit decision needed** (analog: lines 216-220, transcript sets `self.model` from `message.model`). The bridge is now also a model source. `read_bridge_file` runs *after* `read_transcript_tail` in `poll()` (lines 94, 98), so a naive `self.model = v["model"]...` would let the bridge win. Recommended: `if let Some(m) = v["model"].as_str() { self.model = Some(m.to_string()); }` — bridge wins when present, transcript-derived value survives when bridge model is absent (the `if let Some` guard prevents overwriting with `None`).

**Test pattern** (Wave 0 — `meta.rs` has no test module). Same `transcript.rs` idiom. Caveat: `read_bridge_file` is private and keys off `self.session_id` + `bridge_path(sid)` which hardcodes `/tmp/baude-usage-<sid>.json`. To test, write a temp bridge file at the path `bridge_path(<unique-sid>)` returns (using a pid-keyed sid), set `meta.session_id`, call the reader (or a thin pub-in-test helper), then assert. Cover: v2 round-trip (write all fields → all read back), legacy/schema:1 file (missing new fields → legacy present, new `None`), and nested-object reads (`pr` object → `PrInfo` populated; absent `pr` → `None`).

---

### `baude/src/ui.rs` (component, request-response) — STL-03

**Analog:** the **local** `Modal::Info` branch `lines` builder (lines 834-895). Scope is local-only per success criterion 4 ("selecting a session and pressing `i`"); the remote branch (lines 780-832) is out of scope.

**Imports** (lines 1-14) — already present. If `pr.number` (a `u64`) is rendered, no new import is needed (`format!`). No ratatui additions required.

**Row + opt helper analog** (lines 837-844) — reuse these exact closures already defined in the branch; do not redefine:
```rust
let row = |label: &str, value: String| {
    Line::from(vec![
        Span::styled(format!("  {label:<16}"), dim),
        Span::styled(value, val),
    ])
};
let m = &s.meta;
let opt = |v: &Option<String>| v.clone().unwrap_or_else(|| "—".into());
```

**Conditional-row analog — the load-bearing pattern** (lines 871-882). New rows for *optional* fields should be `lines.push(row(...))` guarded by `if let Some(...)`, exactly like the `last_usage` row, so absent fields are omitted (not shown as `—`):
```rust
if let Some(u) = &m.last_usage {
    lines.push(row(
        "last turn",
        format!("in {} · out {} · cache r {} / w {}", /* ... */),
    ));
}
```

**Always-shown-row analog** (lines 845-867) — for reference, the unconditional rows use the `.map(...).unwrap_or_else(|| "—".into())` idiom (e.g. `model`, `permissions`, `context used`). Use this style only if a row should always appear with a `—` placeholder; prefer the conditional-push style for effort/thinking/pr per the research.

**Insertion point:** push the new `effort` / `thinking` / `pr` rows into `lines` *before* the blank-line + usage section. The initial `lines` vec ends with `Line::raw("")` at line 869; insert the conditional pushes immediately after the vec literal (after line 870) and before the `last_usage` block (line 871). Overlay height auto-sizes from `lines.len()` at line 896 — no manual height bump.

**Target shape** (per RESEARCH.md):
```rust
if let Some(e) = &m.effort {
    lines.push(row("effort", e.clone()));
}
if let Some(t) = m.thinking {
    lines.push(row("thinking", if t { "on".into() } else { "off".into() }));
}
if let Some(pr) = &m.pr {
    let n = pr.number.map(|n| format!("#{n}")).unwrap_or_else(|| "?".into());
    let st = pr.review_state.clone().unwrap_or_else(|| "—".into());
    lines.push(row("pr", format!("{n} ({st})")));
}
```

**Test note:** the overlay is rendered inline (no pure-unit boundary). Optional Wave 0 refactor: extract the local `lines` build into a free `fn info_lines(s: &Session) -> Vec<Line>` so rows are assertable without a live terminal. STL-03 is otherwise manual-verified (`cargo build -p baude` + `i` keypress).

## Shared Patterns

### serde_json `Value`-accessor (NOT typed Deserialize)
**Source:** `bridge.rs:57-67` (writer), `meta.rs:243-269` (reader)
**Apply to:** every read of the statusLine payload and the bridge file in both `bridge.rs` and `meta.rs`.
The entire STL-02 back-compat guarantee is an emergent property of using `Value` + `.as_*()` on both sides: unknown keys are ignored, absent/wrong-type keys yield `None`. Do **not** introduce `#[derive(Deserialize)]` on the bridge-file shape — it forces field enumeration and discards the free back-compat.

### Nested-object guard
**Source:** `bridge.rs:32-40` (`is_object()` check → `Value::Null`), `meta.rs:257-262` (`w.is_object().then(|| ...)`)
**Apply to:** `pr` and `worktree` on both writer and reader.
Always check `is_object()` before persisting/reading a sub-object; index straight to leaves for scalar nested fields (`effort.level`, etc.). Reading a nested object as a scalar (`v["effort"].as_str()`) returns `None` — index to the leaf.

### snake-first / camel-fallback
**Source:** `bridge.rs:43,46` (`.or_else(|| w["camelKey"]...)`)
**Apply to:** new fields where drift is plausible (`pr.review_state` → `reviewState`). Defensive only — docs confirm snake_case for all new fields against CLI v2.1.177; the fallback is cheap insurance, not load-bearing.

### Best-effort, never-abort capture
**Source:** `bridge.rs:56-69` (capture wrapped in `if let Ok`/`if let Some`, failure falls through to the `--wrap` delegation at 71-87)
**Apply to:** all new writer reads — keep them inside the existing block; never `unwrap()` or `?` in a way that could abort delegation.

### Optional-everything in `ClaudeMeta`
**Source:** `meta.rs:64-88` (every field `Option`/`Default`), `meta.rs:46-52` (`RateWindow` sub-struct)
**Apply to:** all new `ClaudeMeta` fields and the `PrInfo`/`WorktreeInfo` sub-structs. Claude Code versions and session states differ; never assume presence.

### Inline-JSON-fixture test idiom
**Source:** `bauded/src/transcript.rs:255-320`
**Apply to:** the new `#[cfg(test)] mod tests` in both `bridge.rs` and `meta.rs`.
Raw-string fixtures (`r#"{...}"#`), temp dirs/files keyed by `std::process::id()`, `.ok()` teardown. Note: `baude-core` has only one existing test module today (`pty.rs:215`); these are net-new modules (Wave 0).

## No Analog Found

None. All three change sites extend a pattern that already exists in the same file. The nested-object parse/persist the planner most needs (`pr`, `worktree`) maps directly onto the existing `rate_limits` window handling on both writer and reader sides.

## Metadata

**Analog search scope:** `baude-core/src/{bridge.rs,meta.rs,pty.rs}`, `baude/src/ui.rs`, `bauded/src/transcript.rs` (test idiom).
**Files scanned:** 5 (3 change sites + 2 for shared/test patterns).
**All cited line numbers verified against current `main` (post-commit 7c9cc35).**
**Pattern extraction date:** 2026-06-15
