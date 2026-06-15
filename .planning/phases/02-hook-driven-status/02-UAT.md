---
status: testing
phase: 02-hook-driven-status
source: [02-VERIFICATION.md]
started: 2026-06-15T20:05:00Z
updated: 2026-06-15T20:05:00Z
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
result: [pending]

### 2. User statusLine + user-defined hook survive the merge in a real spawned session
expected: Pre-create a scratch .claude/settings.local.json with a user statusLine + one user hook; spawn a managed session there; inspect the merged file — user statusLine + user hook intact AND baude's four hooks (UserPromptSubmit/Stop/Notification/PostToolUse) appended, each command = absolute current_exe() path + ' hook'
result: [pending]

### 3. Re-spawn / restart does not duplicate baude's hook entries (idempotent on the live path)
expected: Re-spawn or restart the session and re-inspect settings.local.json — baude's entries are NOT duplicated
result: [pending]

### 4. Daemon path: events arrive via POST /sessions/{id}/event and drive the same state
expected: Start bauded, spawn a session via the daemon, confirm events arrive via the daemon-seeded $BAUDE_EVENT_URL and drive the same working/waiting state
result: [pending]

## Summary

total: 4
passed: 0
issues: 0
pending: 4
skipped: 0
blocked: 0

## Gaps
