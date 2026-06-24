---
status: testing
phase: 04-remote-permission-approval
source: [04-VERIFICATION.md]
started: 2026-06-15T23:30:00Z
updated: 2026-06-15T23:30:00Z
---

## Current Test

number: 1
name: Live --permission-prompt-tool wire-contract confirmation (claude 2.1.178)
expected: |
  A real `claude -p --permission-prompt-tool mcp__baude__approve --mcp-config .mcp.json "..."`
  fires the seeded approve tool; logged frames confirm framing (Content-Length vs line), the
  tools/call request field names (tool_name / input / tool_use_id), and that the
  content[0].text JSON.stringify({behavior}) result is accepted to unblock the tool.
awaiting: user response

## Tests

### 1. Live --permission-prompt-tool wire-contract confirmation (04-02 CONTRACT gate)
expected: A live claude 2.1.178 invoking mcp__baude__approve confirms the framing + tools/call field names + accepted response envelope. If divergent, only parse_frame / parse_tool_call / build_approve_result need correcting (deliberately isolated).
steps: |
  1. cargo build --workspace.
  2. Temporarily log raw stdin frames + return hardcoded allow in run_permission_mcp (research §F).
  3. Run: claude -p --permission-prompt-tool mcp__baude__approve --mcp-config .mcp.json "create hello.txt"
  4. Confirm framing, tools/call field names (tool_name/input/tool_use_id), and that the result envelope unblocks the tool.
  5. If divergent, correct the 3 isolated baude-core functions; re-run cargo test -p baude-core permission::; revert the hack.
why_human: Exact 2.1.178 wire shape is MEDIUM-confidence (no complete official example, claude-code #1175). Only a live CLI confirms it.
result: [passed — 2026-06-23, Claude-driven against live claude 2.1.187; 1 ship-blocking bug found + fixed]

  **Method (no code hack needed):** drove an *independent* Python MCP stdio
  server (a probe) under a real `claude -p --permission-prompt-tool
  mcp__probe__approve` to capture the exact wire shape, then re-ran with the
  probe made byte-identical to baude (framing + advertised protocolVersion) to
  isolate divergences, then confirmed end-to-end against the REAL
  `target/debug/baude permission-mcp` binary.

  **Ground-truth wire shape (claude 2.1.187):**
  - Framing IN: newline-delimited JSON (NOT Content-Length); two requests can
    arrive in one read (`notifications/initialized\n` + `tools/list\n`).
    baude's `parse_frame` line path handles both ✓.
  - `initialize`: client offers `protocolVersion: "2025-11-25"`; a server
    advertising baude's pinned `"2024-11-05"` is ACCEPTED ✓ (isolation Test A).
  - `tools/call` params: `{name:"approve", arguments:{tool_name:"Write",
    input:{file_path,content}, tool_use_id:"toolu_…"}, _meta:{…}}`. baude reads
    `arguments.tool_name` + `arguments.input` exactly ✓ — primary field names
    correct; `parameters`/`tool_input` fallbacks unused but harmless.
  - Response envelope: `{content:[{type:"text", text: JSON.stringify({behavior,
    updatedInput|message})}]}` — accepted live; `allow` executed the Write,
    `deny` blocked it. baude's `build_approve_result` matches ✓.

  **BUG FOUND (ship-blocking) + FIXED:** baude's `write_frame` emitted
  LSP-style `Content-Length:`-framed *output*. Claude Code's MCP stdio client
  accepts ONLY newline-delimited JSON on stdout — it silently drops
  Content-Length frames, the `initialize` handshake never completes, and the
  `approve` tool is reported "not found" → **prompt mode was 100% broken
  against live claude.** Isolation proved framing (not protocolVersion) was the
  cause: Test A (line + "2024-11-05") worked; Test B (Content-Length +
  "2025-11-25") failed identically to the real binary. Fixed `write_frame` to
  write one newline-delimited line (`baude-core/src/permission.rs`); input
  framing untouched (`parse_frame` still accepts both). Updated the one output-
  framing unit test. `cargo fmt`(permission.rs)/`clippy -D warnings`/`cargo test
  --workspace` (160 tests) all green.

  **End-to-end re-verify (real binary):** `claude -p` against
  `baude permission-mcp` with NO `$BAUDE_EVENT_URL` → server connected, `approve`
  invoked, baude failed CLOSED to deny, claude honored it ("The Write call was
  denied", file not created) — confirms framing + field names + envelope + the
  security-critical fail-closed-no-daemon posture in one shot.

  NOTE: fix is applied to the working tree + verified, NOT yet committed (await
  user). The live PWA-card + distinct-push + real-phone leg (Test 2) is still
  human-only.

### 2. PWA approve/deny card + distinct push, live prompt-mode session (04-04 UAT)
expected: prompt-mode session → (a) distinct push fires ("<name> needs permission"), separate from the generic waiting push; (b) approve/deny card above the composer with the tool + esc()'d input; (c) Approve runs the tool, card clears; (d) Deny denies only that tool call (session survives), card clears; (e) timeout past BAUDE_PERMISSION_TIMEOUT_S denies (never auto-allows). Web Push phone-verification noted (separate deferred milestone).
why_human: Vanilla-JS PWA (no test runner) + real Web Push needs a device.
result: [pending]

## Data-path validation (Claude-driven, 2026-06-15)

The full prompt-mode bridge↔daemon data path was validated live end-to-end against a
real `bauded` (without real claude — the irreducible residual above):
- `bauded permission-mcp` does NOT daemonize (the Phase-2 bauded-hook trap fix holds);
  it speaks JSON-RPC, `initialize` → MCP handshake (protocol 2024-11-05).
- Fail-closed: a `tools/call` with NO daemon URL returns `{behavior:"deny"}` (never allow).
- FULL round-trip: `tools/call` registers pending → `GET /permission` shows it →
  human `POST {decision:"allow"}` → bridge returns `{behavior:"allow",updatedInput}`;
  `POST {decision:"deny"}` → bridge returns `{behavior:"deny",message:"denied"}`.
- Decision validation: bad decision → 400; unknown id → 404; never 500/panic.

**1 real bug found+fixed during validation (`43a940b`):** the bridge registered the
pending request via ureq `send_string` (Content-Type `text/plain`), which the daemon's
`Json<PermissionBody>` extractor 415'd → pending never registered → prompt mode silently
denied every tool. Fixed to POST `application/json` in both binaries. Caught only by
driving the real bridge↔daemon round-trip; unit tests set the content-type via
tower-oneshot and missed it.

So the prompt-mode mechanism + deny-default posture are live-validated; only the
real-claude wire shape and the PWA visual render remain for human verification.

## Summary

total: 2
passed: 1
issues: 0
pending: 1
skipped: 0
blocked: 0
data_path_validated: true
bugs_found_and_fixed: 2

## Gaps

- Test 1 (§F wire contract) PASSED against live claude 2.1.187 — found + fixed a
  ship-blocking output-framing bug (`write_frame` Content-Length → newline).
- Test 2 (PWA approve/deny card + distinct push + real-phone Web Push) remains
  human-only — needs a device + a live prompt-mode daemon session.
