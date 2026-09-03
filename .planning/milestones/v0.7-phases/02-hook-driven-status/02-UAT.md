---
status: testing
phase: 02-hook-driven-status
source: [02-VERIFICATION.md]
started: 2026-06-15T20:05:00Z
updated: 2026-06-15T20:05:00Z
audit_acknowledged:
  milestone: v2.0
  at: 2026-09-03
  gap_snapshot: "testing::scenarios=0"
---

## Current Test

number: 1
name: Live Claude CLI session fires hooks and flips state without the silence timer
expected: |
  On prompt submit the session shows "working" essentially instantly (faster than the
  ~2s silence window) with the `i` overlay state row = hook (not silence); when a tool
  runs the tool row updates; on Stop it flips to waiting with source still hook; schema-1
  lines appear in /tmp/baude-events-<sid>.jsonl
awaiting: user response

## Tests

### 1. Live Claude CLI session fires hooks and flips state without the silence timer

expected: On prompt submit the session shows "working" essentially instantly (faster than the ~2s silence window) with overlay state row = hook (not silence); when a tool runs the tool row updates; on Stop it flips to waiting with source still hook; schema-1 lines appear in /tmp/baude-events-<sid>.jsonl
steps: |

  1. cargo build --workspace, run the TUI (cargo run -p baude)
  2. Spawn a managed session, submit a prompt
  3. Confirm "working" shows faster than ~2s; press `i` → state row reads `hook`
  4. When Claude runs a tool, the overlay `tool` row updates; on turn end it flips to `waiting` (source still hook)
  5. tail /tmp/baude-events-<sid>.jsonl — schema-1 lines present

result: [partial — 2026-06-15, Claude-driven] Event PRODUCTION validated live against the real binaries: `baude hook` and `bauded hook` both emit correct schema-1 lines for all four events (UserPromptSubmit→Busy, PostToolUse:tool, Notification:permission_prompt, Stop), and malformed/empty payloads exit 0 without panic. Event→state derivation is unit-tested (read_event_tail + decide_status; silence fallback byte-identical). RESIDUAL: the visual TUI overlay flip (state row = `hook` faster than the 2s window) requires a real interactive `claude` session + the ratatui TUI, which cannot be driven headlessly — left for an interactive glance.

### 2. User statusLine + user-defined hook survive the merge in a real spawned session

expected: Pre-create a scratch .claude/settings.local.json with a user statusLine + one user hook; spawn a managed session there; inspect the merged file — user statusLine + user hook intact AND baude's four hooks (UserPromptSubmit/Stop/Notification/PostToolUse) appended, each command = absolute current_exe() path + ' hook'
result: [passed — 2026-06-15, Claude-driven] Validated live through the real daemon spawn path (BAUDE_CLAUDE_CMD=sleep). After spawning into a scratch dir pre-seeded with a user statusLine + user hook: user statusLine (`my-custom-statusline`) preserved, user hook (`my-user-hook`) preserved, and all four baude hooks appended with the absolute current_exe() path + ' hook'.

### 3. Re-spawn / restart does not duplicate baude's hook entries (idempotent on the live path)

expected: Re-spawn or restart the session and re-inspect settings.local.json — baude's entries are NOT duplicated
result: [passed — 2026-06-15, Claude-driven] Creating a second session in the same cwd re-ran the seed; the sentinel-guarded merge left exactly one baude hook group per event (no duplication); user hook + baude hooks all still present.

### 4. Daemon path: events arrive via POST /sessions/{id}/event and drive the same state

expected: Start bauded, spawn a session via the daemon, confirm events arrive via the daemon-seeded $BAUDE_EVENT_URL and drive the same working/waiting state
result: [passed — 2026-06-15, Claude-driven] Live daemon: POST /sessions/{id}/event → 204 for a known session, event appended to the /tmp file keyed by the body session_id (converges with the file-tail reader); unknown id → 404 (never 500/panic). Transport selection also verified: `baude hook` POSTs to a live listener and falls back to the /tmp file on a dead/wrong port (bounded timeout, no hang).

## Bugs found and fixed during UAT

- **bauded did not handle the `hook` subcommand** (`d933edb`): daemon seeding writes `current_exe()` (= `bauded`) as the hook command, but `bauded`'s arg dispatch only matched `--version`/`--help` — `bauded hook` fell through and booted a *second daemon* instead of emitting an event, silently breaking hook-driven status for every daemon-managed session. Fixed by extracting the shared dispatch into `baude_core::hook::dispatch_hook` and adding a `run_hook()` arm to both binaries. Caught only by driving the real daemon binary — unit tests exercise `baude_core::hook` directly, never the binary's CLI dispatch.
- **daemon ingest required the poll-resolved session_id** (`a7e49ab`): `POST /sessions/{id}/event` 404'd until the ~1s poll cycle resolved `meta.session_id`, dropping a real session's earliest events. Fixed to prefer the authoritative `session_id` embedded in the POSTed event line.

## Summary

total: 4
passed: 3
issues: 0
pending: 0
partial: 1
skipped: 0
blocked: 0
bugs_found_and_fixed: 2

## Gaps

- UAT-1 residual: visual TUI overlay state-flip with a real interactive `claude` session (mechanism fully validated; only the on-screen glance is outstanding).
