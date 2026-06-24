# Phase 1: Full Status-Line Capture - Research

**Researched:** 2026-06-15
**Domain:** Rust JSON capture/serde back-compat + ratatui TUI overlay; Claude Code statusLine payload contract
**Confidence:** HIGH

## Summary

This phase makes `baude statusline --wrap` (in `baude-core/src/bridge.rs`) the authoritative capture of the *full* useful Claude Code status-line payload — adding `model`, `effort`, `thinking`, `pr`, `worktree`, and `vim.mode` to the four fields it already extracts (`cost_usd`, `context_used_pct`, `five_hour`, `seven_day`) — stamps the bridge JSON with `schema: 2`, grows the `ClaudeMeta` reader in `meta.rs` with optional fields, and surfaces effort/thinking/PR in the `i` info overlay (`baude/src/ui.rs`). All three requirements (STL-01/02/03) touch code that already exists and is well-factored; this is additive work with no architectural change.

The single most important external finding: the official Claude Code statusLine schema (verified against `code.claude.com/docs/en/statusline` and the installed CLI **v2.1.177**) is **snake_case throughout** for the new fields — `model.display_name`/`model.id`, `effort.level`, `thinking.enabled`, `pr.number`/`pr.url`/`pr.review_state`, `worktree.name`/`path`/`branch`, `vim.mode`. They are **nested objects**, not flat keys, and each is independently absent depending on session state. The existing `window()` snake/camel tolerance was introduced specifically because the **`rate_limits`** sub-keys (`five_hour`/`fiveHour`, `resets_at`/`resetsAt`, `used_percentage`/`utilization`) drifted across versions [VERIFIED: baude-core/src/bridge.rs:32-48]. No camelCase variant for the *new* fields is documented, so the snake/camel tolerance for them is a defensive belt-and-suspenders measure, not a documented requirement.

Back-compat (STL-02) is the one hard constraint with a non-obvious failure mode. Both the bridge writer and the `meta.rs` reader already operate on `serde_json::Value` with `.as_*()` accessors and `if let Some(...)` guards — never `#[derive(Deserialize)]` on a typed struct — so adding fields is inherently additive on both sides and an older `meta.rs` build ignores unknown keys for free. The `schema` discriminant is purely informational; readers must continue to treat every field as optional rather than branch on `schema`.

**Primary recommendation:** Extend `bridge.rs::run()` to emit the new nested fields (reusing `Value` accessors with snake-first/camel-fallback like `window()`), add `"schema": 2`, then grow `ClaudeMeta` with `Option<>` fields populated in `read_bridge_file()` using the same `Value`-accessor pattern, and add three rows (`effort`, `thinking`, `pr`) to the local `Modal::Info` branch in `ui.rs`. Do **not** introduce `#[derive(Deserialize)]` structs for the bridge file — it would break the "ignore unknown / tolerate absent" property that makes back-compat free.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Parse Claude Code statusLine JSON from stdin | `baude-core/bridge.rs` (CLI subcommand) | — | Already owns the `baude statusline` entry point; runs inside the managed session as the statusLine command |
| Persist full payload to `/tmp/baude-usage-<sid>.json` | `baude-core/bridge.rs` | — | Single writer of the bridge file today [bridge.rs:67] |
| Read bridge file into session metadata | `baude-core/meta.rs::read_bridge_file` | — | Sole reader; `ClaudeMeta` is the in-memory model both TUI and daemon consume [meta.rs:243-269] |
| Surface effort/thinking/PR in info overlay | `baude/ui.rs` `Modal::Info` (local branch) | `baude/app.rs` (key handler, already wired) | TUI render reads `s.meta` directly; no new plumbing for local sessions [ui.rs:834-907] |
| Expose new fields to remote/PWA | `bauded/manager.rs::SessionInfo` + `baude/remote.rs::RemoteInfo` | — | OUT OF SCOPE for Phase 1 success criteria (which say "selecting a session and pressing `i`" = local); noted for Tier 2 |

## Standard Stack

No new dependencies. This phase uses only what the workspace already pins.

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `serde_json` | workspace-pinned (already a dep of `baude-core`) | Parse statusLine stdin JSON; build bridge file via `json!` macro | Already the only JSON tool in `bridge.rs`/`meta.rs`; `Value` + `.as_*()` accessors are the established pattern [bridge.rs:22, meta.rs:12] |
| `ratatui` | workspace-pinned (TUI dep of `baude`) | Render the `i` info overlay rows | The overlay is built with `Line`/`Span`/`Paragraph` already [ui.rs:779-907] |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `serde_json::Value` accessors | `#[derive(Deserialize)]` typed structs with `#[serde(default)]` | Typed structs are tidier but **break the free back-compat property** — a typed bridge writer/reader must enumerate fields, and a mismatch in nesting (the new fields are nested objects) is easy to get wrong. The `Value` pattern tolerates absent/unknown/null uniformly. **Reject.** |

**Installation:** None — no `Cargo.toml` changes expected. If a task proposes adding a crate, treat it as a red flag.

## Package Legitimacy Audit

Not applicable — this phase installs **no external packages**. All work uses crates already present in the Cargo workspace (`serde_json`, `ratatui`), verified by their existing `use` statements in the files under change [VERIFIED: bridge.rs:22, meta.rs:12, ui.rs:7].

## Architecture Patterns

### System Architecture Diagram

```
Claude Code session (managed by baude)
        │  emits statusLine JSON on stdin every refresh
        │  (after each assistant msg / /compact / perm-mode / vim toggle, 300ms debounce)
        ▼
┌──────────────────────────────────────────────────────────┐
│ baude statusline --wrap '<orig cmd>'   (bridge.rs::run)    │
│   1. read stdin → serde_json::Value                        │
│   2. if v["session_id"] present:                           │
│        build bridge JSON via json!{ ... }                  │
│        + EXISTING: cost_usd, context_used_pct,             │
│                    five_hour, seven_day                    │
│        + NEW (STL-01): model, effort, thinking, pr,        │
│                        worktree, vim                       │
│        + NEW (STL-02): "schema": 2                         │
│        write → /tmp/baude-usage-<session_id>.json          │
│   3. pipe original stdin → wrapped statusline cmd          │
│        (display unchanged — best-effort, never breaks it)  │
└──────────────────────────────────────────────────────────┘
        │  file on disk
        ▼
┌──────────────────────────────────────────────────────────┐
│ ClaudeMeta::poll → read_bridge_file (meta.rs)              │
│   read_json(bridge_path(sid)) → Value                      │
│   EXISTING: session_cost_usd, context_used_pct,            │
│             rate_5h, rate_week, rate_updated_unix_ms       │
│   NEW (STL-02): effort, thinking, pr, worktree, vim_mode   │
│                 (each Option<>, ignores absent/null)       │
└──────────────────────────────────────────────────────────┘
        │  ClaudeMeta in memory (one per Session)
        ├──────────────────────────┐
        ▼                          ▼
┌────────────────────┐   ┌─────────────────────────────────┐
│ baude TUI          │   │ bauded daemon (OUT OF SCOPE here) │
│ ui.rs Modal::Info  │   │ SessionInfo serialize → REST/SSE  │
│ press `i` →        │   │ → PWA / remote TUI RemoteInfo     │
│ rows: model, perm, │   └─────────────────────────────────┘
│   context, NEW:    │
│   effort, thinking,│
│   pr  (STL-03)     │
└────────────────────┘
```

### Recommended Project Structure

No new files. Changes land in existing files:

```
baude-core/src/
├── bridge.rs   # STL-01: extend run() output + add schema:2; add #[cfg(test)] mod
└── meta.rs     # STL-02: grow ClaudeMeta with Option fields; populate in read_bridge_file

baude/src/
└── ui.rs       # STL-03: add effort/thinking/pr rows to local Modal::Info branch
```

### Pattern 1: snake-first / camel-fallback accessor (reuse `window()` idiom)
**What:** Read a value preferring snake_case, falling back to camelCase, tolerating absence.
**When to use:** Every new field read from the statusLine payload in `bridge.rs`.
**Example:**
```rust
// Source: baude-core/src/bridge.rs:41-47 (existing window() helper) [VERIFIED]
let pct = w["used_percentage"]
    .as_f64()
    .or_else(|| w["utilization"].as_f64());
// resets_at snake → resetsAt camel
"resets_at": w["resets_at"].as_u64().or_else(|| w["resetsAt"].as_u64()),
```
Apply the same `.or_else` chaining for any new field that might drift. For fields the docs only define in snake_case (effort/thinking/pr/worktree/vim), snake is primary; a camel fallback is cheap insurance but not load-bearing.

### Pattern 2: additive `json!{}` bridge output
**What:** The bridge writer is a single `json!{}` literal; new keys are added inline.
**When to use:** STL-01 field additions and the `schema` stamp.
**Example:**
```rust
// Source: baude-core/src/bridge.rs:59-67 (existing) [VERIFIED] — extend, don't restructure
let bridge = json!({
    "schema": 2,                       // NEW (STL-02)
    "session_id": sid,
    "updated_unix_ms": now_unix_ms(),
    "cost_usd": v["cost"]["total_cost_usd"].as_f64(),
    "context_used_pct": v["context_window"]["used_percentage"].as_f64(),
    "five_hour": window(&v, "five_hour", "fiveHour"),
    "seven_day": window(&v, "seven_day", "sevenDay"),
    // NEW (STL-01) — nested-object sources, snake_case per docs:
    "model": v["model"]["display_name"].as_str().or_else(|| v["model"]["id"].as_str()),
    "effort": v["effort"]["level"].as_str(),
    "thinking": v["thinking"]["enabled"].as_bool(),
    "pr": /* object: number, url, review_state — see Code Examples */,
    "worktree": /* object: name, path, branch */,
    "vim_mode": v["vim"]["mode"].as_str(),
});
```

### Pattern 3: `Value`-accessor reader, never typed Deserialize for the bridge file
**What:** `read_bridge_file` pulls each field with `.as_*()` + `if let Some`, so unknown keys are ignored and absent keys leave the `Option` at `None`.
**When to use:** STL-02 reader-side additions.
**Example:**
```rust
// Source: baude-core/src/meta.rs:250-268 (existing pattern) [VERIFIED]
if let Some(cost) = v["cost_usd"].as_f64() {
    self.session_cost_usd = Some(cost);
}
// NEW additive reads follow the identical shape — see Code Examples.
```

### Anti-Patterns to Avoid
- **`#[derive(Deserialize)]` on the bridge file shape:** A typed struct forces field enumeration and risks rejecting/`None`-ing on shape drift; it discards the "ignore unknown keys, tolerate absent" behavior that makes STL-02 back-compat *free*. The success criterion 2 (old `meta.rs` build still parses) is satisfied today **only because** both sides use `Value`. Keep it.
- **Branching reader logic on `schema`:** `schema: 2` is informational. Do **not** write `if schema == 2 { read new fields }` — read every field optionally regardless. A `schema: 1` file (older writer) must still yield the four legacy fields, and a future `schema: 3` must not make the v2 reader bail.
- **Flattening nested payload objects:** `model`, `effort`, `thinking`, `pr`, `worktree`, `vim` are **nested objects** in the source payload (e.g. `effort.level`, not `effort`). Reading `v["effort"].as_str()` returns `None`. Index into the sub-key.
- **Breaking the wrapped statusline on capture failure:** Capture is best-effort and must never abort the delegation to the wrapped command [bridge.rs:56 comment, :73-87]. Keep all new reads inside the existing best-effort block.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Tolerating absent/null/unknown JSON keys | A custom presence-checking layer | `serde_json::Value` `.as_*()` returning `Option` | Already the codebase idiom; returns `None` for both absent and wrong-type, which is exactly the back-compat semantics needed |
| snake/camel key drift | A normalization pre-pass over the whole payload | Per-field `.or_else()` fallback like `window()` | Localized, matches existing code, no allocation; a global normalizer is over-engineering for ~6 fields |
| PR/worktree sub-object capture | A flattened set of `pr_number`, `pr_state` top-level keys | Persist `pr` and `worktree` as nested JSON objects in the bridge file | Mirrors the source shape, keeps the reader symmetric, and lets Tier 2 consume the whole object without a re-capture |

**Key insight:** The entire back-compat guarantee (STL-02) is an emergent property of using `serde_json::Value` on both writer and reader. The "don't hand-roll" rule here is really "don't *replace* the existing untyped approach with something stricter."

## Runtime State Inventory

This is an additive feature phase, not a rename/refactor/migration. The one piece of runtime state worth noting:

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | `/tmp/baude-usage-<sessionId>.json` — written by the bridge, read by `meta.rs`. Stale `schema:1` files (no new fields) may exist from sessions started before this change. | None — reader tolerates absent fields by design; a stale file simply yields `None` for new fields until the session's next statusLine refresh overwrites it (every assistant message). No migration needed. |
| Live service config | The seeded `statusLine` command in `settings.json` (`baude statusline` or `... --wrap ...`) is unchanged — the *command* is the same binary; only its output grows. | None — no settings.json change in Phase 1 (HOOK-01 in Phase 2 touches settings). |
| OS-registered state | None — verified: no Task Scheduler / launchd / systemd registration of the bridge; it's invoked by Claude Code as the statusLine command. | None |
| Secrets/env vars | None reference the captured fields. `CLAUDE_CONFIG_DIR` (read in meta.rs:23) is unaffected. | None |
| Build artifacts | None — no package rename; `cargo build` regenerates binaries normally. | None |

**The canonical question — after every file is updated, what runtime systems still have old state?** Only `/tmp/baude-usage-*.json` files written by the pre-change binary, and those are self-healing on the next statusLine tick because the writer fully overwrites them and the reader tolerates the missing fields.

## Common Pitfalls

### Pitfall 1: Reading a nested object field as a scalar
**What goes wrong:** `v["effort"].as_str()` / `v["pr"].as_u64()` returns `None`, silently dropping the field.
**Why it happens:** The payload nests these: `effort.level`, `pr.number`, `thinking.enabled`, `worktree.branch`, `vim.mode` [CITED: code.claude.com/docs/en/statusline — Full JSON schema].
**How to avoid:** Always index to the leaf: `v["effort"]["level"].as_str()`, `v["pr"]["number"].as_u64()`, `v["thinking"]["enabled"].as_bool()`.
**Warning signs:** Info overlay shows `—` for effort/thinking even when Claude reported them; a unit test feeding the documented example JSON produces `None`.

### Pitfall 2: Assuming fields are always present
**What goes wrong:** Tests or UI assume `model`/`effort`/`pr` exist; they don't for many sessions.
**Why it happens:** Per docs, `effort` is absent when the model lacks the reasoning-effort parameter; `pr` is absent until an open PR exists for the branch and disappears on merge/close; `vim` only when vim mode is on; `worktree` only during `--worktree` sessions; `rate_limits` only for Pro/Max after the first API response [CITED: code.claude.com/docs/en/statusline — "Fields that may be absent"].
**How to avoid:** Every new `ClaudeMeta` field is `Option<>`; the UI uses the existing `opt()` / `unwrap_or("—")` idiom [ui.rs:789, :844].
**Warning signs:** `unwrap()` on a new field; a test that only covers the full payload and never the minimal one.

### Pitfall 3: `context_used_pct` / `used_percentage` may be `null` early or after `/compact`
**What goes wrong:** `as_f64()` yields `None` and the cast `(pct.round() as u64)` is skipped — fine — but a naive change that unwraps would panic.
**Why it happens:** `context_window.used_percentage` is null before the first API call and after `/compact` until the next call [CITED: code.claude.com/docs/en/statusline — "Fields that may be null"].
**How to avoid:** Keep the existing `if let Some(pct) = ...` guard [meta.rs:253]. Do not change context handling in this phase.
**Warning signs:** New `.unwrap()` near context fields.

### Pitfall 4: `pr.review_state` independently absent
**What goes wrong:** Code assumes if `pr` exists, `review_state` exists.
**Why it happens:** Docs: `pr.review_state` "may be independently absent even when `pr` is present" [CITED: code.claude.com/docs/en/statusline].
**How to avoid:** Read each PR sub-field optionally; persist the `pr` object with whatever sub-keys are present (`json!` with `.as_*()` yields JSON `null` for missing, which the reader then treats as `None`).
**Warning signs:** Info overlay renders "review: " with an empty value instead of omitting it.

### Pitfall 5: clippy `-D warnings` on the new code
**What goes wrong:** CI fails on `clippy -D warnings` [VERIFIED: PROJECT.md:59].
**Why it happens:** Common triggers in this kind of change: `needless_return`, `redundant_clone` on `String`, `manual_map`/`option_map_unit_fn`, building a closure that clippy thinks should be a function, or `field_reassign_with_default` if you construct then mutate a struct.
**How to avoid:** Run `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` locally before commit (the two non-test CI gates). Prefer the existing `opt`/`row` closure shapes in `ui.rs` so style matches.
**Warning signs:** Any clippy output at all — the gate is `-D warnings`, so zero tolerance.

## Code Examples

Verified patterns grounded in the actual files under change.

### STL-01: Extend the bridge writer (bridge.rs::run)
```rust
// Source: extends baude-core/src/bridge.rs:59-67 [VERIFIED current shape]
// Payload field names per code.claude.com/docs/en/statusline [CITED]
let pr = {
    let p = &v["pr"];
    if p.is_object() {
        json!({
            "number": p["number"].as_u64(),
            "url": p["url"].as_str(),
            "review_state": p["review_state"].as_str()
                .or_else(|| p["reviewState"].as_str()),  // defensive camel fallback
        })
    } else { Value::Null }
};
let worktree = {
    let w = &v["worktree"];
    if w.is_object() {
        json!({
            "name": w["name"].as_str(),
            "path": w["path"].as_str(),
            "branch": w["branch"].as_str(),
        })
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

### STL-02: ClaudeMeta fields + reader (meta.rs)
```rust
// Source: extends baude-core/src/meta.rs:64-88 (struct) and :243-269 (reader) [VERIFIED]

// In ClaudeMeta struct — all Option, additive:
pub effort: Option<String>,
pub thinking: Option<bool>,
pub vim_mode: Option<String>,
pub pr: Option<PrInfo>,            // small struct OR keep as serde_json::Value
pub worktree: Option<WorktreeInfo>,

// A typed sub-struct is fine for the READER side (in-memory model); the
// back-compat constraint is about the on-disk file + its Value parsing, not
// about ClaudeMeta's internal representation.
#[derive(Default, Clone)]
pub struct PrInfo {
    pub number: Option<u64>,
    pub url: Option<String>,
    pub review_state: Option<String>,
}

// In read_bridge_file(), after the existing rate-window reads:
if let Some(s) = v["effort"].as_str() { self.effort = Some(s.to_string()); }
if let Some(b) = v["thinking"].as_bool() { self.thinking = Some(b); }
if let Some(s) = v["vim_mode"].as_str() { self.vim_mode = Some(s.to_string()); }
if v["pr"].is_object() {
    let p = &v["pr"];
    self.pr = Some(PrInfo {
        number: p["number"].as_u64(),
        url: p["url"].as_str().map(str::to_string),
        review_state: p["review_state"].as_str().map(str::to_string),
    });
}
// model: bridge is now authoritative, but transcript also sets it (meta.rs:218).
// Decide precedence explicitly — recommend: bridge wins when present, else
// keep transcript-derived value. Do NOT unconditionally overwrite with None.
if let Some(m) = v["model"].as_str() { self.model = Some(m.to_string()); }
```

### STL-03: Info overlay rows (ui.rs, local Modal::Info branch)
```rust
// Source: extends baude/src/ui.rs:845-870 (local info lines vec) [VERIFIED]
// `row(label, value)` and `opt(&Option<String>)` helpers already defined at :837-844.
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
// Insert these before the blank-line/usage section so the overlay stays compact.
// The overlay height is computed from lines.len() [ui.rs:896], so it auto-sizes.
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Bridge captures 4 fields, discards the rest of the payload | Capture the full useful payload | This phase | Bridge becomes authoritative for model/effort/thinking/pr/worktree/vim |
| `model` inferred from transcript JSONL (`message.model`) [meta.rs:218] | Bridge `model.display_name` is authoritative; transcript is fallback | This phase | Fewer transcript reads needed for model; resolve precedence (bridge-wins-when-present) |
| `context_window` token fields were cumulative session totals | As of Claude Code **v2.1.132**, `total_input_tokens`/`total_output_tokens` reflect *current context*, not cumulative | v2.1.132 [CITED: docs] | Not used by the bridge today (it reads `used_percentage`), but relevant if a future task touches token math |

**Deprecated/outdated:** None affecting this phase. The `rate_limits` camel variants (`fiveHour`, `resetsAt`, `utilization`) the current `window()` tolerates are not in the current docs' schema (which shows snake_case) — they were real in some past version; keep the tolerance, it costs nothing [VERIFIED: bridge.rs:32-48].

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | A camelCase variant of the *new* fields (e.g. `reviewState`, `displayName`) may appear in some Claude Code version, justifying defensive `.or_else()` fallbacks. The docs only show snake_case. | Code Examples / Pattern 1 | Low — the fallback is harmless dead code if no camel variant ever ships; snake_case is confirmed to work against v2.1.177. |
| A2 | Keeping `pr`/`worktree` as nested JSON objects in the bridge file (vs flattened keys) is the preferred shape. | Don't Hand-Roll | Low — purely a serialization-shape choice internal to baude; either works, nested mirrors the source and helps Tier 2. |
| A3 | STL-03 targets the **local** `Modal::Info` branch only (success criterion 4 says "selecting a session and pressing `i`"); the remote `RemoteInfo`/`SessionInfo` path is Tier-2/out-of-scope here. | Architectural Responsibility Map | Medium — if the planner intends remote parity in Phase 1, `SessionInfo` (manager.rs:45) and `RemoteInfo` (remote.rs:20) plus the daemon serialization must also grow. Confirm scope with the user. |
| A4 | Model precedence should be "bridge wins when present, else transcript-derived." | Code Examples STL-02 | Low — either order yields the same model string in practice; the only risk is transient `None` flicker if mishandled, avoided by the `if let Some` guard. |

## Open Questions

1. **Remote/PWA parity for the new fields**
   - What we know: STL-03's success criterion is phrased for the local TUI `i` overlay; the remote info overlay (ui.rs:780-832) and `RemoteInfo`/`SessionInfo` structs do not carry effort/thinking/pr.
   - What's unclear: whether Phase 1 must also surface these to the daemon/PWA, or whether that is deferred to Tier 2 (the plan doc lists "PR row in sidebar" explicitly as Tier 2).
   - Recommendation: scope Phase 1 to local TUI per the literal success criteria; flag remote parity as a fast follow. Confirm in `/gsd-discuss-phase`.

2. **`vim.mode` surfacing**
   - What we know: STL-01 requires capturing `vim.mode`; the plan marks it "low priority, info only" and STL-03 lists only effort/thinking/pr for the overlay.
   - What's unclear: whether to also show vim mode in the overlay.
   - Recommendation: capture it (STL-01) but only render effort/thinking/pr in the overlay (STL-03 verbatim). Capturing without rendering is fine — it lands in `ClaudeMeta` for future use.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain (cargo) | build/test all three crates | ✓ | (workspace; CI uses stable) | — |
| `claude` CLI | runtime source of the statusLine payload; manual end-to-end verification | ✓ | **2.1.177** | Unit tests use mock JSON — no live CLI needed for CI |
| `serde_json` crate | JSON parse/build | ✓ | workspace-pinned | — |
| `ratatui` crate | overlay render | ✓ | workspace-pinned | — |

**Missing dependencies with no fallback:** None.
**Missing dependencies with fallback:** None — the verified Claude Code version (2.1.177) is newer than the docs' example (`version: 2.1.90`) and includes all six target fields per the current schema.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Built-in Rust `#[cfg(test)] mod tests` + `#[test]` (no external test crate) [VERIFIED: bauded/src/transcript.rs:255-320 pattern] |
| Config file | none — `cargo test` drives it |
| Quick run command | `cargo test -p baude-core bridge` (after adding a `bridge` test module) |
| Full suite command | `cargo test --workspace` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| STL-01 | Full payload (model, effort, thinking, pr, worktree, vim) captured to bridge JSON | unit | `cargo test -p baude-core bridge::tests::full_payload_captured` | ❌ Wave 0 — `bridge.rs` has no `#[cfg(test)]` module today [VERIFIED: grep found none] |
| STL-01 | snake/camel tolerance preserved for rate windows; nested-object reads | unit | `cargo test -p baude-core bridge::tests::snake_camel_tolerated` | ❌ Wave 0 |
| STL-01 | Absent fields → null/omitted, never panic (minimal payload) | unit | `cargo test -p baude-core bridge::tests::minimal_payload_ok` | ❌ Wave 0 |
| STL-02 | Bridge JSON carries `schema: 2` | unit | `cargo test -p baude-core bridge::tests::schema_is_2` | ❌ Wave 0 |
| STL-02 | Old reader parses new file (round-trip: write v2, read with current `read_bridge_file` accessors) | unit | `cargo test -p baude-core meta::tests::reads_v2_bridge` | ❌ Wave 0 — `meta.rs` has no test module today |
| STL-02 | New reader parses old (schema:1 / missing fields) file → legacy fields present, new fields `None` | unit | `cargo test -p baude-core meta::tests::reads_legacy_bridge` | ❌ Wave 0 |
| STL-03 | Info overlay includes effort/thinking/pr rows when present | manual + (optional) render-line unit | `cargo build -p baude` then manual `i` keypress; OR extract row-building into a testable fn | ⚠ Manual — ratatui overlay is rendered inline in `ui.rs`; pure unit testing would require refactoring the lines `vec` into a function. Recommend a small extractable `fn info_lines(&Session) -> Vec<Line>` to make STL-03 unit-testable. |

### Sampling Rate
- **Per task commit:** `cargo test -p baude-core` + `cargo clippy --all-targets -- -D warnings` + `cargo fmt --check`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** `cargo test --workspace` green AND `cargo fmt --check` AND `cargo clippy -D warnings` (the three CI gates [VERIFIED: PROJECT.md:59]) before `/gsd-verify-work`.

### Wave 0 Gaps
- [ ] `baude-core/src/bridge.rs` — add `#[cfg(test)] mod tests` covering full/minimal/snake-camel payloads + `schema:2`. Follow the inline-JSON-string fixture style from `transcript.rs` tests.
- [ ] `baude-core/src/meta.rs` — add `#[cfg(test)] mod tests` for `read_bridge_file` legacy/v2 round-trips. These tests will write a temp bridge file (use a unique `/tmp` name keyed by `std::process::id()` like `transcript.rs:309`) and assert the parsed `ClaudeMeta` fields. Note: `read_bridge_file` is private and keys off `self.session_id` + `bridge_path(sid)` — either test via a small helper or set `session_id` and point `bridge_path` at a temp file (consider making the path injectable for tests).
- [ ] (STL-03) Optional refactor: extract the local info-overlay line builder into a free function so it can be asserted without a live terminal.

## Security Domain

`security_enforcement: true`, `security_asvs_level: 1` [VERIFIED: .planning/config.json]. This phase reads a local file the same process tree already writes and renders strings in a local TUI — minimal new surface.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | No auth in baude by design (VPN-bound) |
| V3 Session Management | no | No web session here |
| V4 Access Control | no | Single-user, local files in `/tmp` |
| V5 Input Validation | yes | The statusLine payload is untrusted-ish input parsed via `serde_json` (memory-safe, no `unwrap`); all fields read as `Option`. Render strings as-is in ratatui (no shell/SQL interpolation). |
| V6 Cryptography | no | No crypto introduced |

### Known Threat Patterns for Rust JSON capture + TUI render

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Malformed/huge payload on stdin | Denial of Service | `serde_json::from_str` is bounded by the read string; current code reads stdin fully — acceptable since the writer is Claude Code, a trusted local process. No change needed. |
| Untrusted field content rendered in TUI (e.g. ANSI in `pr.url`/`model`) | Tampering / spoofing the display | ratatui renders `Span`/`Line` as text, not as raw terminal escapes — content is not interpreted as control sequences. Avoid `print!`-ing payload strings directly to the terminal outside ratatui. The bridge's `--wrap` delegation pipes the *original* payload to the wrapped command unchanged, which is existing, accepted behavior [bridge.rs:73-87]. |
| Path injection via `session_id` into `bridge_path` | Tampering | `bridge_path` interpolates `session_id` into `/tmp/baude-usage-<sid>.json` [bridge.rs:26-28]. A `session_id` containing `/` or `..` could redirect the write. This is **pre-existing** and unchanged by Phase 1; `session_id` comes from a trusted local Claude process. Note for the planner: if hardening is desired, sanitize `sid` to `[A-Za-z0-9-]`, but it is out of scope for STL-01/02/03. |

## Sources

### Primary (HIGH confidence)
- `code.claude.com/docs/en/statusline` (official Claude Code docs, fetched 2026-06-15) — full statusLine JSON schema incl. `model`, `effort.level`, `thinking.enabled`, `pr.{number,url,review_state}`, `worktree.{name,path,branch}`, `vim.mode`, absence/null rules, version-gated field semantics.
- Installed `claude --version` → **2.1.177** — confirms the running CLI is newer than the docs example (2.1.90) and exposes all six fields.
- `baude-core/src/bridge.rs` (read in full) — current 4-field capture, `window()` snake/camel helper, best-effort + `--wrap` delegation.
- `baude-core/src/meta.rs` (read in full) — `ClaudeMeta` struct, `read_bridge_file`, `RateWindow`, `Value`-accessor reader pattern.
- `baude/src/ui.rs:779-908` — `Modal::Info` local + remote overlay rendering, `row`/`opt` helpers.
- `baude/src/app.rs:807-811, 825-829` — `i` key opens `Modal::Info`; any key closes.
- `bauded/src/manager.rs:43-64` — `SessionInfo` (the daemon/remote serialization, out of scope but mapped).
- `baude/src/remote.rs:18-33` — `RemoteInfo` (remote info source).
- `docker-entrypoint.sh` — statusLine seeding (`baude statusline`), "never touch existing settings.json" rule.
- `bauded/src/transcript.rs:255-320` — the workspace test idiom (inline JSON fixtures, temp files keyed by pid).

### Secondary (MEDIUM confidence)
- Live bridge run: piped the documented example payload through `cargo run -p baude -- statusline` and inspected `/tmp/baude-usage-test-abc.json` — confirmed current output is exactly the 4-field subset, validating the "discards the rest" claim empirically.

### Tertiary (LOW confidence)
- None.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new deps; verified against existing `use` statements and Cargo files.
- Architecture: HIGH — all change sites read line-by-line in the actual files; data flow traced end to end and confirmed with a live bridge run.
- Payload shape: HIGH — official docs + installed CLI version cross-checked.
- Pitfalls: HIGH — derived from the documented absence/null rules and the codebase's own patterns.
- Remote/PWA scope: MEDIUM — see Open Question 1; literal success criteria point to local-only.

**Research date:** 2026-06-15
**Valid until:** ~2026-07-15 (30 days; Claude Code statusLine schema is additive but evolving — re-verify field names if a task slips past a Claude Code minor bump).
