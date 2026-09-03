---
status: testing
phase: 01-full-status-line-capture
source: [01-VERIFICATION.md]
started: 2026-06-15
updated: 2026-06-15
audit_acknowledged:
  milestone: v2.0
  at: 2026-09-03
  gap_snapshot: "testing::scenarios=1"
---

## Current Test

number: 1
name: `i` info overlay renders effort / thinking / PR rows (STL-03)
expected: |
  Run baude. Select a session whose bridge file
  (/tmp/baude-usage-<sessionId>.json) has `effort`, `thinking`, and `pr`
  set, and press `i`. The info overlay shows three new rows — effort,
  thinking (on/off), and PR (#N (review_state)) — in addition to the
  existing identity/usage rows. Each row is OMITTED entirely when its
  field is absent (not shown as `—`). `vim_mode` is NOT shown. The remote
  (⇄) session overlay is unchanged.
awaiting: user response

## Tests

### 1. `i` info overlay renders effort / thinking / PR rows (STL-03)

expected: With a session whose bridge JSON has effort/thinking/pr set, pressing `i` shows effort, thinking (on/off), and PR (#number (review_state)) rows in the local info overlay; absent fields omit their row; vim_mode is not rendered; remote overlay unchanged.
result: [pending]

## Summary

total: 1
passed: 0
issues: 0
pending: 1
skipped: 0
blocked: 0

## Gaps
