---
phase: 2
slug: hook-driven-status
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-06-15
---

# Phase 2 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `cargo test` (unit tests `#[cfg(test)]` next to source) |
| **Config file** | none — Cargo workspace |
| **Quick run command** | `cargo test -p baude-core` |
| **Full suite command** | `cargo test --workspace && cargo fmt --check && cargo clippy --workspace -- -D warnings` |
| **Estimated runtime** | ~30–60 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p baude-core` (or the touched crate)
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd-verify-work`:** Full suite + fmt + clippy must be green
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 2-01-01 | 01 | 0 | — | — | `session.rs` test module exists (Wave-0 gap) | unit | `cargo test -p baude-core session` | ❌ W0 | ⬜ pending |
| 2-01-02 | 01 | 1 | HOOK-01 | T-2-01 / V5 input-validation | Merge preserves existing user hooks + statusLine; idempotent on re-spawn | unit | `cargo test -p baude-core settings_seed` | ❌ W0 | ⬜ pending |
| 2-02-01 | 02 | 1 | HOOK-03 | T-2-01 / V5 | `baude hook` parses minimal stdin Value without panic; emits schema-versioned line | unit | `cargo test -p baude-core hook_event` | ❌ W0 | ⬜ pending |
| 2-02-02 | 02 | 2 | HOOK-02 | — | Event→state mapping (UserPromptSubmit→Busy, Stop→Waiting, Notification→Waiting, PostToolUse→tool) | unit | `cargo test -p baude-core event_state` | ❌ W0 | ⬜ pending |
| 2-02-03 | 02 | 2 | HOOK-02 | — | Precedence Hook>SessionFile>Silence; silence fallback byte-identical when no hooks | unit | `cargo test -p baude-core status_source` | ❌ W0 | ⬜ pending |
| 2-03-01 | 03 | 2 | HOOK-03 | T-2-01 / V5 | `POST /sessions/{id}/event` feeds same consume path; 204 on success | integration | `cargo test -p bauded event_endpoint` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `baude-core/src/session.rs` — add `#[cfg(test)] mod tests` (none exists today — researcher Wave-0 gap)
- [ ] Reuse existing temp-file test helpers from `meta.rs` for event-file tail tests
- [ ] No framework install needed — `cargo test` is built in

*Existing bridge.rs / meta.rs / api.rs test modules provide the pure-function + temp-file + HTTP test patterns.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| A real Claude Code session fires `baude hook` and flips state without the silence timer | HOOK-02 | Requires a live `claude` CLI session and real hook invocation — cannot be exercised in `cargo test` | Spawn a managed session in the TUI, submit a prompt, confirm it shows "working" instantly (StateSource=Hook), then "waiting" on Stop — all faster than the 2s silence window |
| Seeded `.claude/settings.local.json` keeps a pre-existing user statusLine/hook intact | HOOK-01 | Needs inspection of the real on-disk settings file after a real spawn | Pre-create `.claude/settings.local.json` with a user hook, spawn, inspect the merged file |

---

## Validation Sign-Off

- [ ] All tasks have automated verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references (session.rs test module)
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
