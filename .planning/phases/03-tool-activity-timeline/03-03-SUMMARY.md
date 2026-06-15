---
phase: 03-tool-activity-timeline
plan: 03
subsystem: pwa
tags: [pwa, vanilla-js, sse, eventsource, xss-escape, service-worker, activity-feed]

# Dependency graph
requires:
  - phase: 03-tool-activity-timeline
    provides: GET /sessions/{id}/activity (ring snapshot, ?limit clamp) + GET /sessions/{id}/activity-stream (standalone SSE) + HookEvent {event, tool?, notification_type?, ts} (03-02)
provides:
  - openActivity(sid) — GET-then-SSE-with-buffer backfill against /activity + /activity-stream (mirrors openChat, no snapshot↔tail gap)
  - Collapsible .activity-strip in the PWA chat view (collapsed by default, recent ~30 newest-at-bottom, scrollable, live append)
  - XSS-escaped event rendering (every event field through esc() before innerHTML)
  - Activity SSE lifecycle (opened on route enter / visibilitychange recover, closed in closeStream on view exit — no leak)
  - sw.js CACHE bump baude-v2 → baude-v3 (deployed clients refetch embedded app.js/style.css)
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Second EventSource (activity-stream) cloned from openChat's GET-then-buffer-then-drain discipline — keyless append-only channel, deduped by snapshot order not Event::id"
    - "Activity SSE torn down inside closeStream alongside the chat ES so both die on every navigation (Pitfall 5)"
    - "render-time .slice(-30) clamp so live appends never grow the rendered strip past the snapshot window"

key-files:
  created: []
  modified:
    - bauded/web/app.js
    - bauded/web/style.css
    - bauded/web/sw.js

key-decisions:
  - "Relative time = humanMs(Date.now() - e.ts) since HookEvent.ts is documented unix-milliseconds (baude-core/src/meta.rs:104)"
  - "Icon set by event kind: PostToolUse ⚙ / Notification 🔔 / UserPromptSubmit ✎ / Stop ■ / fallback • (Claude's discretion per CONTEXT)"
  - "Strip label precedence tool || notification_type || event — single one-line row, all fields esc()-escaped (T-03-09)"
  - "Strip mounted between #screen drawer and #composer (below chat history / above composer per CONTEXT)"
  - "openActivity called alongside openChat on route enter and on visibilitychange recovery so the strip recovers when the tab refocuses"

patterns-established:
  - "Two standalone SSE channels per chat view (chat /stream + activity /activity-stream), both buffer-then-drain, both closed in closeStream"

requirements-completed: []  # ACT-03 satisfied in code but PENDING human-verify UAT (Task 2)

# Metrics
duration: ~8min
completed: 2026-06-15
---

# Phase 3 Plan 03: PWA Activity Strip Summary

**The phone-facing tool-activity feed: a collapsible `.activity-strip` in the PWA chat view that backfills the recent ~30 events via GET-then-SSE-with-buffer (mirroring `openChat`), appends live over the standalone `/activity-stream` channel, escapes every event field against XSS, and ships behind a `baude-v3` service-worker cache bump so deployed phones refetch the embedded assets.**

## Status

**Task 1 (code): COMPLETE + committed.** Build green, CI triad green, symbols verified.
**Task 2 (UAT): PENDING human-verify.** The PWA is vanilla JS/CSS embedded via `include_bytes!` — no JS test runner, no build step — so ACT-03 is manual-only by construction. The UAT requires a live `bauded` session in a real browser (ideally a phone, to also exercise the SW cache bump). It has NOT been run and is NOT fabricated; see the manual steps below.

## Performance

- **Duration:** ~8 min (code task)
- **Completed:** 2026-06-15
- **Tasks:** 2 (1 code complete, 1 UAT pending)
- **Files modified:** 3

## Accomplishments
- Added `openActivity(sid)` mirroring `openChat` exactly: open `EventSource('/sessions/${sid}/activity-stream')` into `state.aesBuffer` first, then `await api('/sessions/${sid}/activity?limit=30')`, set `state.activity = recent`, drain the buffer, null it, `render()`. The `state.sid !== sid` guard on both the onmessage and the post-await path prevents cross-session bleed (the openChat seam — no event falls between snapshot and tail).
- Added `activity: []`, `activityOpen: false` (collapsed by default), `aes: null`, `aesBuffer: null` to `state`; reset `activity`/`activityOpen` on route change.
- Extended `closeStream()` to also `state.aes.close()` + null `aes`/`aesBuffer`, so the second EventSource dies on every navigation (Pitfall 5 — no leak across views). `closeStream` is already called from `route()` on every hash change.
- Rendered the collapsible strip in `renderChat` between the `#screen` drawer and `#composer`: a toggle button (`#actbtn`, caret + "activity" + event count) and, when open, a scrollable `#activity-feed` showing `state.activity.slice(-30)` newest-at-bottom, one `.act-row` per event (icon + label + relative time). Auto-scrolls when near bottom on live append.
- XSS: every event-origin string (`activityIcon`, `activityLabel` → `e.tool`/`e.notification_type`/`e.event`, and the relative-time string) passes through `esc()` before innerHTML interpolation — never raw (T-03-09 mitigation).
- Wired `#actbtn.onclick = toggleActivity` in the same handler-binding block as `screenbtn`/`archbtn`; `toggleActivity` flips `state.activityOpen` and re-renders.
- Recovered the strip on tab refocus: the `visibilitychange` handler now also `openActivity(state.sid)` when `!state.aes`, alongside the existing `openChat` recovery.
- Added `.activity-strip` styles to `style.css` mirroring the `#screen` drawer (collapsed/open, `max-height: 30vh` scrollable feed, one-line `.act-row` with ellipsis on the label, right-aligned relative time).
- Bumped `sw.js` `CACHE = "baude-v2"` → `"baude-v3"`; the activate handler already evicts non-matching caches and `/app.js`+`/style.css` are already in `SHELL`, so no new entry needed (Pitfall 4).

## Task Commits

1. **Task 1: activity strip — openActivity, render, toggle, SSE cleanup, cache bump** — `5d50ddd` (feat)
2. **Task 2: PWA activity strip UAT (live session)** — PENDING human-verify (no automated path by construction; not committed).

## Files Created/Modified
- `bauded/web/app.js` — Added `activity`/`activityOpen`/`aes`/`aesBuffer` state; `openActivity`/`scrollActivity`/`activityIcon`/`activityLabel`/`activityRowHtml`/`toggleActivity`; route-change reset + `openActivity` on enter; `closeStream` activity teardown; the strip render + `#actbtn` binding in `renderChat`; `visibilitychange` recovery.
- `bauded/web/style.css` — Added the `.activity-strip` / `.act-toggle` / `.act-feed` / `.act-row` block mirroring the `#screen` drawer.
- `bauded/web/sw.js` — `CACHE` bumped to `"baude-v3"`.

## Decisions Made
- `HookEvent.ts` is unix-**milliseconds** (`baude-core/src/meta.rs:104`), so relative time is `humanMs(Date.now() - e.ts)` directly — no ×1000.
- Icons are event-kind based (`⚙`/`🔔`/`✎`/`■`/`•`), label precedence `tool || notification_type || event` — both at Claude's discretion per CONTEXT (CONTEXT line 65 leaves icons/field-naming/format open).
- A render-time `.slice(-30)` clamp guards the displayed window even if live SSE appends grow `state.activity` past the 30-event snapshot (the snapshot is `?limit=30`; appends are unbounded until the next route change resets).
- The activity channel is keyless (no `Event::id`, per 03-02 Pitfall 2) — dedup is by the GET-then-buffer ordering, exactly like the chat seam, so no `ts`-based dedup is needed.

## Deviations from Plan

None — plan executed exactly as written. (The `visibilitychange` `openActivity` recovery is the natural analog of the existing `openChat` recovery already in that handler; it preserves the "no leak / always-live" invariant the plan calls for and is not a structural change.)

## Threat Model Compliance
- **T-03-09 (XSS via tool/notification strings in the strip):** mitigated — `activityIcon`, `activityLabel`, and the relative-time string all flow through `esc()` (app.js:28) before innerHTML; `e.tool`/`e.notification_type`/`e.event` are never interpolated raw. Final verification is UAT step 5 (literal-text rendering of HTML-ish input) — PENDING.
- **T-03-10 (two concurrent EventSources per view):** accepted as planned — the strip ES is scoped to the chat view and closed in `closeStream` on every navigation; two SSE on one client is well within the HTTP/1.1 ~6-connection cap.
- **T-03-SC (package installs):** accepted — no installs; vanilla JS, no build step, no JS dependencies added.

## Known Stubs
None. The strip is wired to the live `/activity` + `/activity-stream` endpoints (03-02), not mock data. The empty-state (`no tool activity yet`) renders only when the live feed is genuinely empty.

## PENDING UAT — Manual Browser Verification (ACT-03, by construction)

ACT-03 has no automated path (vanilla-JS PWA, no test runner, no build step). Run these against a live session — ideally on the phone, to also exercise the SW cache bump. Do NOT mark ACT-03 complete until this passes.

1. `cargo build -p bauded`, run `bauded`, open the PWA on a device.
2. Hard-refresh once (or confirm the SW evicted `baude-v2` → `baude-v3`); open a session's chat view. Confirm the **activity strip is present below the chat / above the composer and is COLLAPSED by default**.
3. Expand the strip. Confirm it shows the recent tool sequence — one line per event (icon + tool/type + relative time), **newest at the bottom, scrollable**.
4. With the session active, drive several tool calls in Claude. Confirm new rows **append live (no page reload)** with no gap or duplicate at the snapshot↔live seam.
5. Confirm a tool/notification string with HTML-ish characters renders as **literal text** (XSS escaping via `esc()`).
6. Navigate away from the chat view and back; confirm **no duplicate/stale EventSource** (the strip ES closed on exit).

**Resume signal:** Type "approved", or describe issues (strip empty while file grows → /activity-stream tail bug from plan 02; blank on phone but fine on desktop → stale SW cache, re-check the CACHE bump).

## Automated Verification (run, green)
- `cargo build -p bauded` — clean (embedded assets rebuilt).
- `grep baude-v3 bauded/web/sw.js` ✓ ; `grep openActivity bauded/web/app.js` ✓ ; `grep activity-stream bauded/web/app.js` ✓.
- CI triad green: `cargo fmt --check` (clean), `cargo clippy --workspace --all-targets -- -D warnings` (clean), `cargo test --workspace` (42 + 0 doc tests, 0 failed).

## Next Phase Readiness
- Plan 04 (TUI activity overlay): consumes `SessionInfo.activity` (bounded ~30, already riding `/sessions` from 03-02) and the local `s.meta.activity()` ring; the PWA surface for ACT-03 is in place pending UAT sign-off.

## Self-Check: PASSED
