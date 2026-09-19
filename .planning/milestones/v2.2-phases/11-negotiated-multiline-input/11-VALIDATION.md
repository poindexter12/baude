---
phase: "11"
slug: "negotiated-multiline-input"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-09-16"
---

# Phase 11 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust workspace) |
| **Config file** | Cargo.toml (workspace root) |
| **Quick run command** | `cargo test -p baude keys` |
| **Full suite command** | `cargo test --workspace --locked` |
| **Estimated runtime** | ~140 seconds (full), ~15 seconds (quick) |

---

## Sampling Rate

- **After every task commit:** Run the task's targeted module tests
- **After every plan wave:** Run `cargo test --workspace --locked`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 140 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| (filled by planner) | — | — | TKEY-01..05 | — | enhanced bytes only on verified paths; mode always restored | unit + integration | (filled by planner) | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements — Phase 8 isolation and
Phase 9/10 injected-seam precedents stand.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Real kitty-terminal Shift+Enter newline + mode-restore residue check | TKEY-01/04 | Needs a live enhanced terminal | Run baude in kitty/ghostty, Shift+Enter in a Claude prompt, exit and confirm outer keyboard mode intact (folds into Phase 12 validation) |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 140s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
