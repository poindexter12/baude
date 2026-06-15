---
phase: 4
slug: remote-permission-approval
status: approved
nyquist_compliant: true
wave_0_complete: true
created: 2026-06-15
---

# Phase 4 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `cargo test`; PWA has NO JS test runner (no build step). The `--permission-prompt-tool` MCP wire contract is verified by a live contract-confirmation UAT (Claude Code 2.1.178). |
| **Config file** | none — Cargo workspace |
| **Quick run command** | `cargo test -p bauded` |
| **Full suite command** | `cargo test --workspace && cargo fmt --check && cargo clippy --workspace -- -D warnings` |
| **Estimated runtime** | ~30–60 seconds |

---

## Sampling Rate

- **After every task commit:** the touched crate's `cargo test`
- **After every plan wave:** `cargo test --workspace`
- **Before `/gsd-verify-work`:** full suite + fmt + clippy green
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-----------|--------|
| 4-01-01 | 01 | 1 | PERM-01 | V1 access-control | `BAUDE_PERMISSION_MODE` defaults to skip; skip→`--dangerously-skip-permissions`, prompt→`--permission-prompt-tool`; never double-add | unit | `cargo test -p bauded permission_mode` | ❌ W0 | ⬜ pending |
| 4-02-01 | 02 | 2 | PERM-02 | V1 | Pending state on Session; GET returns pending/204; POST resolves; deny-on-timeout (never auto-allow) | unit/integration | `cargo test -p bauded permission_api` | ❌ W0 | ⬜ pending |
| 4-02-02 | 02 | 2 | PERM-01/02 | V5 input-validation | The `permission-mcp` bridge: stdin JSON-RPC → daemon POST → blocked decision → MCP allow/deny response; both binaries handle the subcommand (no daemonize fall-through) | unit | `cargo test -p baude-core permission` | ❌ W0 | ⬜ pending |
| 4-03-01 | 03 | 3 | PERM-04 | — | `waiting_reason` enum on SessionInfo from last_notification; distinct `notified_permission` push; notify.rs constructor fix | unit | `cargo test -p bauded waiting_reason` | ❌ W0 | ⬜ pending |
| 4-CONTRACT | 02 | 2 | PERM-01 | — | LIVE `--permission-prompt-tool` MCP request/response shape confirmed (raw-frame log, hardcoded allow) before prompt-mode finalized | human-verify | (live claude — UAT) | n/a | ⬜ pending |
| 4-04-01 | 04 | 3 | PERM-03 | — | PWA approve/deny card while pending, disappears on resolve | manual-UAT | (no JS runner — UAT) | n/a | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `bauded` permission-mode + permission-api tests via the existing tower-oneshot + manager harness
- [ ] `baude-core` JSON-RPC / permission-bridge tests (hand-rolled stdio framing, deny-on-timeout)
- [ ] CONTRACT GATE: the live `--permission-prompt-tool` MCP request/response shape is confirmed via a human-verify UAT (research §F) BEFORE prompt-mode spawn wiring is treated as final — the wire contract has a documented ambiguity (no complete official example)

*Existing notify.rs / api.rs / manager.rs / hook.rs test modules provide the patterns.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| `--permission-prompt-tool` MCP request/response contract | PERM-01 | The exact wire shape has no complete official example (claude-code #1175); only a live `claude` 2.1.178 invoking the seeded MCP tool confirms it | Spawn a prompt-mode session with a `permission-mcp` that logs the raw request frame and returns a hardcoded allow; confirm the tool fires, the frame matches the assumed `{tool_name,input,tool_use_id}`, and allow unblocks the tool |
| PWA approve/deny card + distinct push | PERM-03/04 | Vanilla-JS PWA (no test runner) + real Web Push needs a device | With a prompt-mode session, trigger a tool permission; confirm the distinct push fires ("wants to run X — approve?"), the card appears in chat, Approve unblocks / Deny denies, card disappears on resolve |

---

## Validation Sign-Off

- [x] PERM-02/PERM-04 (+ bridge) have automated verify; PERM-01 wire contract is the gated human-verify UAT; PERM-03 is manual UAT
- [x] Sampling continuity: no 3 consecutive automatable tasks without automated verify
- [x] Wave 0 covers the permission-mode/api/bridge scaffolds + the CONTRACT gate
- [x] No watch-mode flags
- [x] Feedback latency < 60s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved 2026-06-15
