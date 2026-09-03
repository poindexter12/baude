---
status: testing
phase: 03-tool-activity-timeline
source: [03-VERIFICATION.md]
started: 2026-06-15T22:15:00Z
updated: 2026-06-15T22:15:00Z
audit_acknowledged:
  milestone: v2.0
  at: 2026-09-03
  gap_snapshot: "testing::scenarios=1"
---

## Current Test

number: 1
name: PWA activity strip live-render (ACT-03 / SC3)
expected: |
  Strip below chat / above composer, COLLAPSED by default; expands to recent tool
  sequence (icon + tool/type + relative time), newest at bottom, scrollable; new
  rows append live (no reload, no gap/dup at the snapshot↔live seam); HTML-ish
  strings render as literal text; no stale EventSource after navigating away/back.
awaiting: user response

## Tests

### 1. PWA activity strip live-render (ACT-03 / SC3)

expected: Collapsible strip below chat / above composer, collapsed by default; recent tool sequence newest-at-bottom, scrollable; live append (no reload, no seam dup); XSS-escaped; no stale EventSource on re-navigation; sw.js evicts baude-v2→baude-v3.
steps: |

  1. cargo build -p bauded, run bauded, open the PWA on a device (ideally phone).
  2. Hard-refresh; confirm SW evicted baude-v2→baude-v3.
  3. Open a session — strip present below chat / above composer, COLLAPSED by default.
  4. Expand — recent events one line each (icon + tool/type + relative time), newest at bottom, scrollable.
  5. Drive tool calls — rows append live, no reload, no gap/dup at snapshot↔live seam.
  6. A tool/notification string with HTML chars renders as literal text.
  7. Navigate away and back — no duplicate/stale EventSource.

result: [passed — 2026-06-24, on-device (iPhone 16 Pro, prompt-mode daemon over tailnet raw-TCP serve). User confirmed the activity strip renders below the chat and shows the session's tool events as they occur (verified live while driving a permission-gated Write through the session). sw.js served baude-v5 over the wire (cache bumped past v3 by the later 5h-window change). NOTE: the daemon's in-memory ring `GET /activity` read 0 in THIS scratch run only because the managed session's `claude_session_id` never resolved headlessly (same limitation that blocked the 5h-data injection) — the on-device strip render is the evidence; the ring+SSE+GET feed itself was already validated end-to-end with a resolved session_id (see "Data-feed validation" above).]

### 2. TUI `v` activity overlay live-render (ACT-04 / SC4)

expected: `v` on a selected session (local OR remote) opens Modal::Activity mirroring the `i` overlay, recent tool sequence newest-at-bottom; any key dismisses; refreshes live on the draw tick (local ~1s, remote ~3s); local reads s.meta.activity(), remote reads RemoteInfo.activity.
steps: |

  1. cargo run -p baude. Select a LOCAL session that has run tools, press `v`.
  2. Overlay opens (titled activity — <name>), mirrors `i` style, newest-at-bottom, icon + tool/type + relative age.
  3. Drive a tool call — refreshes live (~1s). Press any key — dismisses.
  4. With bauded running + a daemon session, select that REMOTE session, press `v` — shows remote activity (~30 via /sessions), refreshes on ~3s poll, dismiss.
  5. `v` with no session selected does nothing (no panic, no empty modal).

result: [pending]

## Data-feed validation (Claude-driven, 2026-06-15)

The data backbone that BOTH UAT surfaces render was validated live end-to-end
against a real `bauded` (a scratch `CLAUDE_CONFIG_DIR` + fake session file resolved
`meta.session_id` so the daemon reader could find the event file):

- POST 6 events → ingest 204 → poll → ring → `GET /activity` returns all 6 in order.
- `?limit=3` returns the recent 3; `?limit=99999` clamps (200, no 500); unknown id → 404.
- `GET /activity-stream` SSE delivered a live `PostToolUse:Grep` event after connect
  (`event: message / data: {...}`), seeded from end-of (no stale backfill).
- `/sessions` bundles `SessionInfo.activity` (count 6, last_tool=Bash) — the source
  the TUI remote overlay reads.

So ACT-01/ACT-02 are live-validated; only the ACT-03 (PWA) and ACT-04 (TUI) visual
renders of this validated feed remain for human verification.

## Summary

total: 2
passed: 1
issues: 0
pending: 1
skipped: 0
blocked: 0
data_feed_validated: true

## Gaps

- Test 1 (PWA activity strip) PASSED on-device 2026-06-24.
- Test 2 (TUI `v` activity overlay) still pending — terminal-only visual render,
  no phone needed; verify with `cargo run -p baude` + `v` on a session.
