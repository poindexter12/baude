---
phase: 3
slug: tool-activity-timeline
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-06-15
---

# Phase 3 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `cargo test` (unit/integration `#[cfg(test)]`); PWA has NO JS test runner (no build step — do not add one) |
| **Config file** | none — Cargo workspace |
| **Quick run command** | `cargo test -p baude-core` |
| **Full suite command** | `cargo test --workspace && cargo fmt --check && cargo clippy --workspace -- -D warnings` |
| **Estimated runtime** | ~30–60 seconds |

---

## Sampling Rate

- **After every task commit:** Run the touched crate's `cargo test`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd-verify-work`:** Full suite + fmt + clippy must be green
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 3-01-01 | 01 | 1 | ACT-01 | — | Ring buffer caps at ACTIVITY_CAP, drops oldest; resets on event-path rotation | unit | `cargo test -p baude-core activity` | ❌ W0 | ⬜ pending |
| 3-02-01 | 02 | 2 | ACT-02 | V5 input-validation | `GET /sessions/{id}/activity` returns recent events JSON (+?limit); 404 unknown id, never 500 | integration | `cargo test -p bauded activity` | ❌ W0 | ⬜ pending |
| 3-02-02 | 02 | 2 | ACT-02 | — | `GET /sessions/{id}/activity-stream` SSE tails event file (dedicated HookEvent tail, NOT the ChatMessage Tail) | integration | `cargo test -p bauded activity_stream` | ❌ W0 | ⬜ pending |
| 3-03-01 | 03 | 3 | ACT-03 | — | PWA collapsible activity strip: GET-then-SSE backfill, live append | manual-UAT | (no JS runner — UAT) | n/a | ⬜ pending |
| 3-04-01 | 04 | 3 | ACT-04 | — | TUI `v` opens Modal::Activity (local meta buffer + remote RemoteInfo.activity); dismiss | unit-if-seam-else-UAT | `cargo test -p baude activity_modal` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] Confirm/extend a `baude-core` test for the activity ring (append + cap + path-rotation reset) alongside the existing event-tail tests in `meta.rs`
- [ ] `bauded` activity endpoint tests via the existing tower-oneshot harness (mirror the `post_event`/`stream` test patterns)
- [ ] Wave-0 CHECK: does `baude/src/app.rs` have a key-dispatch test seam for the `v`→Modal::Activity open/dismiss? If yes → unit test; if no → record ACT-04 open/dismiss as manual UAT (do NOT build a TUI harness for this phase)

*Existing meta.rs / api.rs / manager.rs test modules provide the temp-file + tower-oneshot patterns.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| PWA activity strip renders the live tool sequence and updates without reload | ACT-03 | Vanilla-JS PWA has no test runner / build step (deliberate constraint) | Open the PWA chat view for an active session, expand the activity strip, drive tool calls, confirm new rows append live and the strip is collapsed by default |
| TUI `v` overlay mirrors the feed for local AND remote sessions | ACT-04 | Full ratatui render + live remote poll needs an interactive terminal | Press `v` on a local session and a remote session; confirm both show the recent tool sequence newest-at-bottom and refresh live |

---

## Validation Sign-Off

- [ ] ACT-01/ACT-02 have automated verify (ring buffer + endpoints); ACT-03 is UAT; ACT-04 unit-if-seam-else-UAT
- [ ] Sampling continuity: no 3 consecutive automatable tasks without automated verify
- [ ] Wave 0 covers the activity ring + endpoint test scaffolds and the ACT-04 seam check
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
