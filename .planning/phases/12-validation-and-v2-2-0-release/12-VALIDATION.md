---
phase: "12"
slug: "validation-and-v2-2-0-release"
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-09-16"
---

# Phase 12 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test + cargo fmt/clippy + GitHub Actions CI parity |
| **Config file** | Cargo.toml, .github/workflows/ci.yml |
| **Quick run command** | `cargo clippy --workspace --all-targets -- -D warnings` |
| **Full suite command** | `cargo test --workspace --locked` (plus fmt, clippy, assert-real-roots-untouched bracket) |
| **Estimated runtime** | ~4.5 minutes serial (full bracket) |

---

## Sampling Rate

- **After every task commit:** Run the task's gate command
- **After every plan wave:** Run the full CI-parity bracket
- **Before `/gsd-verify-work`:** Bracket green, CI green on the pushed branch
- **Max feedback latency:** 300 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| (filled by planner) | — | — | SHIP-01..04 | — | publish only after verification + human approval | gate + docs grep | (filled by planner) | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements — CI, release-please,
and the phase 8 real-roots bracket are in place.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Real macOS/Linux terminal smoke: links, selection, scrollback, mouse, Shift+Enter, Enter, restoration | SHIP-03 | Must be observed in a live terminal by the maintainer | Follow 12-SMOKE-EVIDENCE.md leg by leg; record terminal identity + date |
| Publish approval for v2.2.0 | SHIP-04 | Release decision is a human gate (release:hold label) | Approve only after verification passes and smoke evidence is recorded |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or are explicit manual-only rows
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 300s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
