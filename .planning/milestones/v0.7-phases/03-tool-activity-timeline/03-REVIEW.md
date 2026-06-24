---
phase: 03-tool-activity-timeline
reviewed: 2026-06-15T00:00:00Z
depth: standard
files_reviewed: 11
files_reviewed_list:
  - baude-core/src/meta.rs
  - baude/src/app.rs
  - baude/src/remote.rs
  - baude/src/ui.rs
  - bauded/src/api.rs
  - bauded/src/manager.rs
  - bauded/src/notify.rs
  - bauded/src/transcript.rs
  - bauded/web/app.js
  - bauded/web/style.css
  - bauded/web/sw.js
findings:
  critical: 0
  warning: 2
  info: 2
  total: 4
status: issues_found
---

# Phase 3: Code Review Report

**Reviewed:** 2026-06-15
**Depth:** standard
**Files Reviewed:** 11
**Status:** issues_found

## Summary

Reviewed the Phase 3 (Tool-Activity Timeline) change set across the Rust workspace
(`baude-core`, `baude` TUI, `bauded` daemon) and the embedded vanilla-JS PWA. The
implementation is high quality: `cargo clippy --workspace --all-targets -D warnings`
is clean, and the full workspace test suite passes (102 tests). The design's named
pitfalls are all handled correctly:

- **Ring buffer (meta.rs):** `VecDeque<HookEvent>` is bounded drop-oldest at
  `ACTIVITY_CAP`, and `read_event_tail` **does** `self.activity.clear()` on event-path
  rotation (line 414) — the reset genuinely clears it (covered by
  `activity_ring_clears_on_path_rotation`).
- **EventTail (transcript.rs):** A distinct type that yields `HookEvent` via
  `parse_event_line`, NOT the `ChatMessage` `Tail`; offset/partial-line/truncation
  handling mirrors `Tail` and is unit-tested.
- **Security:** `?limit` is defaulted and clamped to `ACTIVITY_CAP`
  (`q.limit.unwrap_or(ACTIVITY_CAP).min(ACTIVITY_CAP)`) — no oversized alloc, no
  panic; unknown id → 404 via `not_found` on both `/activity` and `/activity-stream`.
  Session-id path construction is sanitized by the reused `hook::event_path`
  (`..`→`_`, `/`→`_`). The PWA escapes every event-origin field through `esc()`
  before `innerHTML` (icon, label, relative time) — no raw interpolation of
  `tool`/`notification_type`/`event`.
- **PWA lifecycle:** the second `EventSource` (`aes`) is torn down in `closeStream`
  and re-opened only when `!state.aes`, so no leaked/duplicate SSE connections.
  `sw.js` `CACHE` bumped `v2`→`v3` and `activate` deletes stale caches.
- **No-regression:** `SessionInfo.activity` / `RemoteInfo.activity` use
  `#[serde(default)]`; `notify.rs` test constructor updated; back-compat
  deserialize tests present.

Findings below are limited to a snapshot↔live seam race that can duplicate display
rows, a benign reconnect-churn against deleted sessions, and two cosmetic items.

## Warnings

### WR-01: Snapshot↔live SSE seam can duplicate activity rows (no dedup)

**File:** `bauded/web/app.js:189-216`, `bauded/src/api.rs:430-462`
**Issue:** The GET-then-buffer ordering claims "nothing can fall between the snapshot
and the live tail," but the two data sources advance independently and there is no
de-duplication:
- `activity_stream` server-side seeds `EventTail::end_of(&path)` from the **on-disk
  file's** current EOF at the first poll iteration (T0).
- The `?limit=30` snapshot reads the **in-memory `ClaudeMeta` ring**, populated by the
  daemon's `poll()` loop, at a later time T1 > T0.

Any hook event whose file-append occurs at/after T0 **and** is also present in the
ring snapshot at T1 is delivered twice: once in `state.activity = recent` and once via
a buffered SSE `message`. Hook events carry no uuid, so (by design) no `Event::id` is
set and the client does no dedup (`state.activity.push(e)` unconditionally). The result
is a duplicated row at the seam. This is display-only (the strip triggers no actions),
hence WARNING not BLOCKER, but the design's "avoids gaps/dupes" guarantee is not met.
**Fix:** De-dup at the client seam using a stable event key. Since events are
append-only with monotonic `ts`, drop buffered SSE events that are not strictly newer
than the last snapshot event:
```js
const recent = await api(`/sessions/${sid}/activity?limit=30`);
if (state.sid !== sid) return;
state.activity = recent;
const lastTs = recent.length ? recent[recent.length - 1].ts : 0;
for (const e of state.aesBuffer || []) {
  if (e.ts > lastTs) state.activity.push(e); // drop overlap with snapshot
}
state.aesBuffer = null;
```
(Equal-`ts` collisions within the same millisecond are still possible; if that matters,
key on `ts`+ordinal. For this single-user low-volume tool, `e.ts > lastTs` is adequate.)

### WR-02: activity-stream EventSource reconnect-churns against a deleted session

**File:** `bauded/web/app.js:195-198`, `bauded/src/api.rs:430-461`
**Issue:** When a session is deleted, the server stream loop hits
`Err(_) => break` and ends the SSE normally (a 200 stream that simply closes). The
browser `EventSource` treats a closed stream as a transient drop and **auto-reconnects**;
the reconnect re-runs `activity_stream`, whose up-front `event_path(id)` guard now
returns 404. `es.onerror` is a no-op, so the connection retries in a loop until the user
navigates away (route change tears `aes` down via `closeStream`). This is not a leak
(it is bounded to the current view), but it is needless reconnect churn and log noise
against a session that will never return.
**Fix:** Detect the unrecoverable case and stop. Either close on the snapshot 404
(the `catch` block already runs for the redirect path — also null out `aes`), or close
on `onerror` when the EventSource is in the CLOSED ready state:
```js
es.onerror = () => {
  if (es.readyState === EventSource.CLOSED) { es.close(); if (state.aes === es) state.aes = null; }
};
```

## Info

### IN-01: `humanMs(0)` renders "0s" on the strip vs the TUI's "now"

**File:** `bauded/web/app.js:38-43, 224`
**Issue:** The PWA relative time uses `humanMs(Math.max(0, Date.now() - e.ts))`, which
returns `"0s"` for a just-now (or clock-skewed-future) event, while the TUI's
`activity_age` (ui.rs:739-742) returns `"now"` for the same case. Minor cross-surface
inconsistency in the "at-a-glance" tone the code explicitly aims to match.
**Fix:** Special-case 0 in the row builder, e.g. `const rel = e.ts ? (Date.now()-e.ts < 1000 ? "now" : humanMs(...)) : "";`.

### IN-02: `activityLabel`/`activity_label` fallback strings diverge silently

**File:** `bauded/web/app.js:233-235`, `baude/src/ui.rs:711-721`
**Issue:** For a `PostToolUse` event missing `tool`, the TUI falls back to `"tool"`
and a `Notification` missing `notification_type` to `"notification"`; the PWA's
`activityLabel` instead falls through to `e.event` (e.g. `"PostToolUse"`). The two
overlays will show different text for the same malformed event. Harmless (both escape
output and neither crashes) but the surfaces claim to "mirror" each other.
**Fix:** Align the PWA fallback with the TUI, or document the divergence as intentional.

---

_Reviewed: 2026-06-15_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
