---
phase: 03-tool-activity-timeline
verified: 2026-06-15T22:15:00Z
status: human_needed
score: 4/4 must-haves implemented (2 automated-verified, 2 pending live-render UAT)
overrides_applied: 0
uat_note: "Data feed (ACT-01 ring + ACT-02 daemon GET /activity + SSE /activity-stream + SessionInfo.activity) FULLY validated live by Claude against a real daemon (stood up a scratch CLAUDE_CONFIG_DIR + fake session file to resolve meta.session_id): POST->ingest->poll->ring->GET returns events in order, ?limit clamps, unknown id 404s, SSE delivers live events end-to-end, /sessions bundles SessionInfo.activity. Only the two VISUAL render surfaces remain — ACT-03 PWA strip (browser) and ACT-04 TUI v overlay (terminal) — which render the already-validated feed. Code review found 0 critical / 2 warnings / 2 info, all 4 fixed (WR-01 snapshot dedup, WR-02 SSE-404 teardown, IN-01/02 PWA/TUI mirror consistency)."
human_verification:

  - test: "PWA activity strip live-render (ACT-03 / SC3)"
    expected: "Strip present below chat / above composer, COLLAPSED by default; expands to show recent tool sequence (icon + tool/type + relative time), newest at bottom, scrollable; new rows append live with no page reload and no gap/duplicate at the snapshot↔live seam; HTML-ish tool/notification strings render as literal text; no stale EventSource after navigating away and back."
    why_human: "Vanilla-JS PWA embedded via include_bytes! — no JS test runner, no build step. Live render, SSE append behavior, SW cache eviction, and XSS-escaped display are visually/behaviorally observable only in a real browser against a live bauded session."
  - test: "TUI `v` activity overlay live-render (ACT-04 / SC4)"
    expected: "Pressing `v` on a selected session (local OR remote) opens the Modal::Activity overlay; it renders the recent tool sequence newest-at-bottom mirroring the `i` Info overlay; any key dismisses back to Modal::None; the overlay refreshes live on the existing draw tick (local ~1s, remote ~3s); local reads s.meta.activity(), remote reads RemoteInfo.activity with no extra round-trip."
    why_human: "No app.rs key-dispatch/render test seam exists (per 03-VALIDATION.md). Open/dismiss key flow and live in-terminal rendering can only be confirmed by driving the TUI against a session producing tool events."
audit_acknowledged:
  milestone: v2.0
  at: 2026-09-03
  status: human_needed
---

# Phase 3: Tool-Activity Timeline Verification Report

**Phase Goal:** The hook event stream is exposed as a live, capped (~200) per-session tool-activity feed that renders in both the PWA and the TUI.
**Verified:** 2026-06-15T22:15:00Z
**Status:** human_needed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths (ROADMAP Success Criteria)

| # | Truth | Status | Evidence |
| --- | ----- | ------ | -------- |
| SC1 | `manager.rs` / core retains a per-session ring buffer of recent tool events capped at ~200; older events dropped, not unbounded | ✓ VERIFIED | `baude-core/src/meta.rs:85` `ACTIVITY_CAP=200`; `:147` `activity: VecDeque<HookEvent>`; `:462-470` `push_back` + `pop_front` cap; `:414` `activity.clear()` on path rotation. Tests `activity_ring_caps_drop_oldest`, `..records_each_event_in_order`, `..clears_on_path_rotation`, `..skips_malformed_lines` all PASS. `manager.rs:477` `activity(id, limit)` reads the ring. |
| SC2 | `GET /sessions/{id}/activity` returns recent events AND new events arrive live over SSE without a page reload | ✓ VERIFIED | `api.rs:40-41` routes for `/activity` + `/activity-stream`; `:178` `get_activity` (limit clamped to `ACTIVITY_CAP`, 404 on unknown); `:430` `activity_stream` SSE via dedicated `EventTail` offset-tail of `/tmp/baude-events-<sid>.jsonl`. Tests `activity_returns_events_clamps_limit_and_404s_unknown`, `activity_stream_guards_known_and_unknown` PASS. |
| SC3 | PWA chat view shows a collapsible activity strip with the recent tool sequence, updating live | ⚠️ IMPLEMENTED — pending live UAT | `app.js:203` `openActivity` GET-then-SSE backfill with ts-dedup (WR-01); `:592` `.activity-strip` collapsed-by-default render; `esc()` on every event field (`:272-274`); `sw.js:4` `CACHE="baude-v3"`. Code complete; live render/SSE/XSS confirmation is human-only by construction. |
| SC4 | TUI `v` opens an activity overlay mirroring the same feed for the selected session | ⚠️ IMPLEMENTED — pending live UAT | `app.rs:822` `v` → `Modal::Activity` (local OR remote); `:842` any-key dismiss; `ui.rs:1019` render arm, remote-first then local, Clear+Paragraph+Block mirroring Modal::Info; `remote.rs:46` `#[serde(default)] activity` back-compat. Open/dismiss/live-render is human-only (no test seam). |

**Score:** 4/4 success criteria implemented in code; SC1+SC2 automated-verified (14 passing tests), SC3+SC4 pending human live-render UAT.

### Required Artifacts

| Artifact | Expected | Status | Details |
| -------- | -------- | ------ | ------- |
| `baude-core/src/meta.rs` | HookEvent, ACTIVITY_CAP, activity field + accessor, ring append, clear-on-rotation | ✓ VERIFIED | All present; `HookEvent` serializes `{event, tool?, notification_type?, ts}` with `skip_serializing_if`; 8 ring/tail tests pass. |
| `bauded/src/manager.rs` | `event_path(id)`, `activity(id, limit)`, `SessionInfo.activity` + population | ✓ VERIFIED | `:466` `event_path`, `:477` `activity` (ring slice), `:72`/`:657` SessionInfo bounded ~30. Tests pass. |
| `bauded/src/api.rs` | `get_activity` + route, `activity_stream` SSE + route, `ActivityQuery` | ✓ VERIFIED | Routes `:40-41`; handlers `:178`/`:430`; limit clamp + 404 guards; WR-02 SSE teardown on session-deleted. Tests pass. |
| `bauded/src/transcript.rs` | event-line tail yielding HookEvent (NOT ChatMessage Tail) | ✓ VERIFIED | `:262` `EventTail`, `:245` `parse_event_line`, `:269` `end_of`, `:275` `read_new`. 2 tests pass. |
| `bauded/src/notify.rs` | test SessionInfo constructor updated (`activity: vec![]`) | ✓ VERIFIED | `:122` `activity: vec![]`. |
| `bauded/web/app.js` | `openActivity` GET-then-SSE, state, strip render, toggle, cleanup | ✓ VERIFIED | `:203` openActivity, `:592` strip render, `:279` toggle, `:150` SSE close on exit. |
| `bauded/web/style.css` | `.activity-strip` collapsed/expanded scrollable styles | ✓ VERIFIED | `:179` `.activity-strip`, `:210` `.open .act-feed`. |
| `bauded/web/sw.js` | CACHE version bump | ✓ VERIFIED | `:4` `CACHE="baude-v3"`. |
| `baude/src/remote.rs` | `RemoteInfo.activity` `#[serde(default)]` back-compat | ✓ VERIFIED | `:46` field; back-compat covered by 2 serde tests (with/without field). |
| `baude/src/app.rs` | `Modal::Activity`, `v` dispatch, dismiss arm | ✓ VERIFIED | `:822` `v` dispatch (local/remote), `:842` dismiss. |
| `baude/src/ui.rs` | `Modal::Activity` render arm (remote first, then local) | ✓ VERIFIED | `:1019` render arm; activity_icon/label/age helpers; Help line. |

### Key Link Verification

| From | To | Via | Status | Details |
| ---- | -- | --- | ------ | ------- |
| `meta.rs::read_event_tail` | `self.activity` ring | `push_back` + cap `pop_front` | ✓ WIRED | `:462-470` same per-line match loop. |
| event-path rotation block | `self.activity.clear()` | reset alongside last_tool/hook_status | ✓ WIRED | `:414`. |
| `api.rs::activity_stream` | `event_path(id)` + EventTail | offset-tail serializing HookEvent | ✓ WIRED | `:430-471`. |
| `api.rs::get_activity` | `manager.activity(id, limit)` | limit clamped to ACTIVITY_CAP | ✓ WIRED | `:184`. |
| `manager::session_info` | `SessionInfo.activity` | bounded ~30 from `meta.activity()` | ✓ WIRED | `:657-665`. |
| `app.js::openActivity` | `/activity-stream` + `/activity` | EventSource → buffer → GET → drain (ts-dedup) | ✓ WIRED | `:205-234`. |
| `app.js` strip render | `esc()` on every field | XSS-safe innerHTML | ✓ WIRED | `:272-274`. |
| `app.rs::v` | `Modal::Activity` | set when selected() or selected_remote() | ✓ WIRED | `:822-825`. |
| `ui.rs::draw_modal Activity` | `RemoteInfo.activity` / `s.meta.activity()` | Clear+Paragraph+Block, remote-then-local | ✓ WIRED | `:1019-1062`. |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
| -------- | ------------- | ------ | ------------------ | ------ |
| activity ring | `ClaudeMeta.activity` | parsed hook-event JSONL lines in `read_event_tail` | Yes — appended per real event line | ✓ FLOWING |
| `/activity` endpoint | `manager.activity()` | in-memory ring slice | Yes — reads live ring | ✓ FLOWING |
| `/activity-stream` | `EventTail.read_new` | offset-tail of `/tmp/baude-events-<sid>.jsonl` | Yes — emits appended lines | ✓ FLOWING |
| PWA strip | `state.activity` | GET `/activity?limit=30` + SSE | Yes — populated from real endpoints (live confirmation = UAT) | ✓ FLOWING (render = UAT) |
| TUI overlay | `r.activity` / `s.meta.activity()` | `/sessions` poll bundle / local ring | Yes — no hardcoded empty (render = UAT) | ✓ FLOWING (render = UAT) |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| -------- | ------- | ------ | ------ |
| Workspace activity tests exist | `cargo test -p baude-core -p bauded --no-run` + enumeration | 14 activity/tail/endpoint tests enumerated | ✓ PASS |
| Activity tests pass | `cargo test -p baude-core -p bauded` (single run) | baude-core 58 passed / bauded 42 passed / 0 failed | ✓ PASS |
| TUI crate compiles | `cargo build -p baude` | Finished clean | ✓ PASS |
| Debt markers (TBD/FIXME/XXX) in modified files | grep across 11 files | none found | ✓ PASS |

### Probe Execution

No probe scripts declared for this phase (not a migration/tooling phase). N/A.

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| ----------- | ----------- | ----------- | ------ | -------- |
| ACT-01 | 03-01 | Per-session capped ring buffer (ACTIVITY_CAP=200) of tool events | ✓ SATISFIED | meta.rs ring + 8 tests pass |
| ACT-02 | 03-02 | `GET /activity` + live SSE stream | ✓ SATISFIED | api.rs routes/handlers + 6 tests pass |
| ACT-03 | 03-03 | PWA collapsible activity strip | ⚠️ NEEDS HUMAN | app.js/style.css/sw.js complete; live render = UAT (vanilla JS, no test runner) |
| ACT-04 | 03-04 | TUI `v` activity overlay | ⚠️ NEEDS HUMAN | remote.rs/app.rs/ui.rs complete + 2 serde tests; open/dismiss/render = UAT (no app.rs seam) |

All four Phase 3 requirements (ACT-01..04) declared in plan frontmatter and mapped 1:1 in REQUIREMENTS.md (lines 87-90). No orphaned requirements.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| bauded/web/app.js | 476,478,626 | HTML `placeholder=` attributes | ℹ️ Info | Legitimate input placeholders, not stub markers — no impact |

No blocker or warning anti-patterns. No unreferenced debt markers. Code review (03-REVIEW.md) found 0 critical / 2 warnings / 2 info; all 4 (WR-01 snapshot dedup, WR-02 SSE-404 teardown, IN-01/IN-02 PWA/TUI mirror consistency) verified present in code.

### Human Verification Required

#### 1. PWA activity strip live-render (ACT-03 / SC3)

**Test:** `cargo build -p bauded`, run `bauded`, open the PWA on a device (ideally a phone to also exercise the SW cache bump). Hard-refresh once or confirm SW evicted `baude-v2` → `baude-v3`. Open a session chat view.
**Expected:** Activity strip present below chat / above composer, COLLAPSED by default. Expand → recent tool sequence, one line per event (icon + tool/type + relative time), newest at bottom, scrollable. Drive several tool calls → rows append live (no page reload), no gap/duplicate at the snapshot↔live seam. An HTML-ish tool/notification string renders as literal text. Navigate away and back → no duplicate/stale EventSource.
**Why human:** Vanilla-JS PWA embedded via `include_bytes!`; no JS test runner or build step. Live render, SSE append, SW cache eviction, and XSS-escaped display are observable only in a real browser against a live session.

#### 2. TUI `v` activity overlay live-render (ACT-04 / SC4)

**Test:** Run `baude` (local and against a remote daemon). Select a session producing tool events, press `v`.
**Expected:** `Modal::Activity` overlay opens for local OR remote selection, renders the recent tool sequence newest-at-bottom mirroring the `i` Info overlay; any key dismisses back to no modal; overlay refreshes live on the draw tick (local ~1s, remote ~3s); remote reads bundled `RemoteInfo.activity` with no extra round-trip.
**Why human:** No app.rs key-dispatch/render test seam (per 03-VALIDATION.md). Open/dismiss key flow and live in-terminal rendering are confirmable only by driving the TUI.

### Gaps Summary

No gaps. All four success criteria are structurally implemented and wired end-to-end in the codebase, with real data flowing through the ring → endpoints → both client surfaces. SC1 (ring) and SC2 (daemon endpoints/SSE) are automated-test-verified (14 passing tests across baude-core/bauded). SC3 (PWA strip) and SC4 (TUI overlay) are code-complete but their live-render/interaction behavior is verifiable only by a human, by construction — the PWA has no JS test runner and the TUI has no key-dispatch/render test seam. Both UATs are explicitly pending in their SUMMARYs (not fabricated). Status is `human_needed`, not `passed`, solely because of these two live-render checks; nothing is missing or stubbed in the code.

---

_Verified: 2026-06-15T22:15:00Z_
_Verifier: Claude (gsd-verifier)_
