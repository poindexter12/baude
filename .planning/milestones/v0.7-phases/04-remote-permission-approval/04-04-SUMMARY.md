---
phase: 04-remote-permission-approval
plan: 04
subsystem: pwa
tags: [pwa, vanilla-js, permission, approve-deny, xss, service-worker, perm-03]

# Dependency graph
requires:
  - phase: 04-remote-permission-approval
    provides: "04-02: daemon GET/POST /sessions/{id}/permission routes + PermissionView {request_id, tool, input, decision} — the card fetches the pending request and POSTs the decision here"
  - phase: 04-remote-permission-approval
    provides: "04-03: SessionInfo.waiting_reason === 'permission' + the distinct permission push — the card's trigger (fetched off /sessions, surfaced by the push)"
provides:
  - "bauded/web/app.js: state.pendingPermission + fetchPermission(sid) (GET /permission, driven off the open session's waiting_reason in refresh()) + permSummary(input) + approve()/deny() (POST {decision}) + the perm-card render in renderChat() above the composer"
  - "bauded/web/style.css: .perm-card / .perm-actions / .perm-btn.allow / .perm-btn.deny styles in the PWA's visual language"
  - "bauded/web/sw.js: CACHE baude-v3 -> baude-v4 so the new app.js/style.css ship to the installed PWA"
affects:
  - "(phase complete) the phone-mediated approval surface end-to-end — final plan of phase 04"

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Permission card is a mechanical re-application of the renderChat()/esc()/api()-POST conventions: card injected between ${activityStrip} and the composer form, EVERY dynamic string esc()'d (the established PWA XSS rule), buttons wired in the same handler block as escbtn/killbtn"
    - "Card trigger is data-driven off SessionInfo.waiting_reason (already on /sessions from 04-03) — refresh() fetches GET /permission when the open session is waiting on a permission and clears state.pendingPermission otherwise; covers both chat-open (route → refresh) and the permission push/SSE (the new waiting_reason lands in the next /sessions poll)"
    - "Optimistic removal + refetch on resolve (SC3): POST the decision, null the card immediately, then re-GET to confirm it cleared"
    - "Deny denies the single tool call only — POSTs {decision:'deny'} to /permission, NOT the interrupt/kill route (the session survives; Claude continues with the tool denied)"

key-files:
  created: []
  modified:
    - bauded/web/app.js
    - bauded/web/style.css
    - bauded/web/sw.js

key-decisions:
  - "Drove the card off SessionInfo.waiting_reason inside refresh() (the 3s poll + route-driven refresh) rather than threading a new SSE/push handler: waiting_reason === 'permission' already arrives on /sessions (04-03), so chat-open and a permission push both surface the card via the existing poll — no new stream, minimal surface."
  - "state.pendingPermission stores the GET /permission view only when it is a live PENDING request (view.tool present AND no view.decision); a resolved-decision view or null clears it — so a stale prior-turn decision never re-renders the card."
  - "permSummary(input) only shapes/clips the (possibly object) tool input into a short single line; the CALLER esc()'s it before innerHTML — escaping stays at the single innerHTML boundary, matching the activity strip."
  - "CSS landed in style.css (a SHELL asset) with the v4 cache bump carrying it — the plan's files_modified lists app.js/sw.js, but the card needs styles to be usable; style.css is the established home for PWA styles and ships via the same sw.js shell cache (documented as a Rule-2 addition below)."

requirements-completed: []  # PERM-03 code complete but NOT marked done — gated by the Task 2 human-verify UAT (live browser + prompt-mode session + a real permission request); a vanilla-JS PWA has no test runner (PERM-03 is manual-only by 04-VALIDATION.md).

# Metrics
duration: 12min
completed: 2026-06-15
---

# Phase 4 Plan 4: PWA approve/deny permission card Summary

**A vanilla-JS approve/deny card rendered above the composer in the chat view while a permission is pending — gated on `waiting_reason === "permission"` + a live `GET /permission` fetch, every attacker-influenced string `esc()`'d — that POSTs `{decision}` to `/sessions/{id}/permission` (Approve runs the tool, Deny denies only the single tool call so the session survives) and optimistically disappears + refetches on resolve; sw.js cache bumped `baude-v3 → baude-v4`. Browser/device UAT (PERM-03) pending — vanilla PWA has no test runner.**

## Status: CHECKPOINT REACHED (Task 2 — human-verify UAT)

Task 1 (the autonomous code task) is complete and committed (`f60bb3f`). **Task 2 is a `checkpoint:human-verify` gate (`gate="blocking"`) that CANNOT run headlessly** — it requires a real browser (the PWA is vanilla JS with no test runner / no build step; PERM-03 is manual by construction per 04-VALIDATION.md) and a live prompt-mode session emitting a real permission request, plus a device for the distinct Web Push. No browser results were fabricated. The exact manual steps are reproduced below.

## Performance
- **Duration:** ~12 min
- **Tasks:** 1 of 2 (Task 1 code complete + committed; Task 2 is the human-verify UAT — pending)
- **Files modified:** 3

## Accomplishments
- **`state.pendingPermission` + `fetchPermission(sid)` (app.js):** a GET `/sessions/{id}/permission` fetch (mirroring the activity GET shape) that stores the view only when it is a live *pending* request (`view.tool` present, no `view.decision`); 404/offline leaves the card hidden. Driven off the open session's `waiting_reason` inside `refresh()` (the 3s poll + the route-driven refresh on chat open), so a permission push/SSE update (which lands `waiting_reason === "permission"` on `/sessions`, 04-03) and chat-open both surface the card; any non-permission state clears it. Also cleared on route change (leaving the chat).
- **`permSummary(input)` (app.js):** collapses the (string or object) tool input into a short single readable line (whitespace-squashed, clipped at ~140 chars). Shape/clip only — the caller `esc()`'s it.
- **`approve()` / `deny()` (app.js):** POST `{decision:"allow"|"deny"}` to `/sessions/{id}/permission`, optimistically null the card + render, then re-`fetchPermission` to confirm it cleared (SC3). On error, `toast(...)` like `interrupt()`. **Deny uses the permission route — NOT interrupt/kill** (the session survives; Claude continues with the tool denied — T-04-14).
- **`perm-card` render in `renderChat()` (app.js):** gated on `s && s.waiting_reason === "permission" && state.pendingPermission`, inserted in the `$app.innerHTML` template BETWEEN `${activityStrip}` and the composer `<form>` (above the composer). Shows the tool name + the input summary + Approve/Deny buttons, **every dynamic string `esc()`'d** (tool name + input summary are attacker-influenced via Claude's tool args — T-04-13 XSS). Buttons wired (`#permallow`/`#permdeny`) in the same handler block as `escbtn`/`killbtn`.
- **`.perm-card` CSS (style.css):** minimal styles in the PWA's visual language (yellow permission accent, green approve / red-outline deny pill buttons, safe-area padding).
- **`sw.js` cache bump:** `baude-v3 → baude-v4` so the updated `app.js` + `style.css` ship to the installed PWA (the 03-03 cache-bump precedent).

## Task Commits
1. **Task 1: PWA approve/deny permission card above the composer** — `f60bb3f` (feat)

## Files Created/Modified
- `bauded/web/app.js` — `state.pendingPermission`; `fetchPermission(sid)`, `permSummary(input)`, `approve()`/`deny()`/`resolvePermission(decision)`; the `permCard` render between `${activityStrip}` and the composer; `#permallow`/`#permdeny` wiring; the `refresh()` waiting_reason drive + the route-change clear.
- `bauded/web/style.css` — `.perm-card` / `.perm-head` / `.perm-tool` / `.perm-input` / `.perm-actions` / `.perm-btn.allow` / `.perm-btn.deny` styles.
- `bauded/web/sw.js` — `CACHE` constant `baude-v3 → baude-v4`.

## Decisions Made
- See `key-decisions` frontmatter. Load-bearing: the card is driven off `SessionInfo.waiting_reason` in the existing `refresh()` poll (no new stream) and `state.pendingPermission` only ever holds a *live pending* view (never a resolved decision), so a stale prior-turn decision never re-renders the card.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] `.perm-card` CSS added to `bauded/web/style.css`**
- **Found during:** Task 1 (the card needs styles to be usable; the plan action explicitly says "Add minimal CSS for `.perm-card`/`.perm-actions`/`.allow`/`.deny`").
- **Issue:** The plan's `files_modified` frontmatter lists only `app.js` + `sw.js`, but an unstyled card is not a usable approve/deny surface and the action text mandates the CSS.
- **Fix:** Added the `.perm-card` style block to `style.css` (a SHELL asset already in the sw.js cache list), in the PWA's existing visual language. It ships via the same `baude-v4` shell-cache bump.
- **Files modified:** `bauded/web/style.css`.
- **Verification:** `cargo build -p bauded` (style.css is embedded via `include_bytes!`) green; CI triad green.
- **Committed in:** `f60bb3f` (Task 1 commit).

---

**Total deviations:** 1 auto-fixed (missing-critical: the card's CSS, mandated by the action text but absent from the file list). No scope creep.

## Threat Model Compliance
- **T-04-13 (Tampering/XSS — perm card render):** EVERY dynamic string in the card is `esc()`'d before `innerHTML` — `esc(pp.tool || "tool")` and `esc(permSummary(pp.input))`. The tool name + input summary originate from Claude's tool args (attacker-influenced); `permSummary` only shapes/clips, the `esc()` at the innerHTML boundary is the escape (matching the activity strip's `activityRowHtml`). **To be confirmed in the UAT** with a tool input containing `< > & "` (no broken layout).
- **T-04-14 (Elevation of Privilege — deny action):** `deny()` POSTs `{decision:"deny"}` to `/permission` — it denies the single tool call only and does NOT call the interrupt/kill route. The session stays alive and Claude continues with the tool denied. **To be confirmed in the UAT** (Deny → session survives).
- **T-04-15 (Spoofing — POST /permission), T-04-SC (installs):** accepted/no-op — inherits the project bind (no auth layer by design); no package installs (vanilla JS, no build step).

## Known Stubs
- None. The card is wired end to end against the live daemon routes (`GET`/`POST /sessions/{id}/permission`) and the populated `SessionInfo.waiting_reason`. It simply does not render until a real permission flows — which is the Task 2 UAT.

## Verification (automated, all green)
- `node --check bauded/web/app.js` — parses clean.
- `grep -nE "perm-card|pendingPermission|/permission" bauded/web/app.js` — confirms the card, the state field, the GET fetch, and both POST decisions.
- `grep -nE "CACHE|baude-v" bauded/web/sw.js` — confirms `CACHE = "baude-v4"`.
- `cargo build -p bauded` — green (the PWA assets are embedded via `include_bytes!`, so the binary rebuilds with the new app.js/style.css/sw.js).
- **CI triad green:** `cargo fmt --check` (clean); `cargo clippy --workspace --all-targets -- -D warnings` (exit 0); `cargo test --workspace` (100 baude-core + 57 bauded + 2 baude + 0 doc — all pass).

## UAT (Task 2) — PENDING human-verify (browser + device)

The PWA is vanilla JS with NO test runner / NO build step (PERM-03 is manual by construction per 04-VALIDATION.md), and the distinct Web Push needs a real device. **This cannot be run headlessly.** No browser results were fabricated.

### Exact manual steps
1. Build + run: `cargo build --workspace`, start `bauded`, open the PWA, hard-refresh (the `baude-v4` sw.js cache bump should pull the new app.js/style.css — confirm via DevTools → Application → Service Workers).
2. Spawn a session with `BAUDE_PERMISSION_MODE=prompt` (so 04-01 seeds `.mcp.json` and selects `--permission-prompt-tool`). Submit a prompt that triggers a tool Claude must ask about (e.g. "delete build/" or a file write), so the call routes to `mcp__baude__approve`.
   - **Note:** prompt mode itself is gated by the 04-02 §F CONTRACT human-verify UAT (live `claude` 2.1.178 wire shape — see 04-02-SUMMARY.md), which is also still pending. If the bridge contract has not yet been confirmed/corrected, the card may receive a malformed/empty pending — verify the §F gate first or in tandem.
3. CONFIRM the distinct push fires (phone or browser) — title/body describes the action ("<name> needs permission" / "wants to run a tool — approve?"), DISTINCT from the generic "is waiting for you" push. (If Web Push is not yet phone-verified — a separate deferred milestone — confirm the Notification at least reaches the browser; note phone-verification status.)
4. Open the session's chat. CONFIRM the approve/deny card appears ABOVE the composer, shows the tool + an input summary, and the strings are escaped (feed a tool input containing `< > & "` — no broken layout).
5. Tap **Approve** → CONFIRM the tool runs (Claude proceeds) and the card disappears (optimistic removal + the refetch shows no pending).
6. Trigger another permission. Tap **Deny** → CONFIRM the single tool call is denied (Claude continues, the tool did not run) and the SESSION IS NOT KILLED (it stays alive, the turn continues). The card disappears.
7. (Timeout safety) Trigger a permission and DO NOT respond past `BAUDE_PERMISSION_TIMEOUT_S` → CONFIRM the tool is DENIED on timeout (never auto-allowed) and the card clears.

### Resume signal
Type `approved` once the card appears/clears correctly, Approve runs the tool, Deny denies just the tool (session survives), the distinct push fired, and timeout denies. Otherwise describe what diverged (and note Web Push phone-verification status — a separate deferred milestone, not a blocker for this phase's sign-off).

## Next Phase Readiness
- This is the FINAL plan of phase 04. Once the Task 2 UAT (and the 04-02 §F CONTRACT UAT it depends on) pass, PERM-03 — and the v0.7 milestone's phone-mediated approval surface — is complete end to end.
- Web Push phone-verification remains a separate deferred milestone (additive trigger only — the send/encryption path is untouched).

## Self-Check: PASSED
- FOUND: `bauded/web/app.js` (perm-card, pendingPermission, /permission)
- FOUND: `bauded/web/style.css` (.perm-card)
- FOUND: `bauded/web/sw.js` (baude-v4)
- FOUND: `.planning/phases/04-remote-permission-approval/04-04-SUMMARY.md`
- FOUND commit: `f60bb3f`

---
*Phase: 04-remote-permission-approval*
*Completed (code task): 2026-06-15 — Task 2 human-verify UAT pending*
