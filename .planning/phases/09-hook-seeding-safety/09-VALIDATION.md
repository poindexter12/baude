---
phase: "9"
slug: "hook-seeding-safety"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-09-15"
---

# Phase 9 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust workspace) |
| **Config file** | Cargo.toml (workspace root) |
| **Quick run command** | `cargo test -p baude-core --lib hook::` |
| **Full suite command** | `cargo test --workspace --locked` |
| **Estimated runtime** | ~120 seconds (full), ~10 seconds (quick) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p baude-core --lib hook::`
- **After every plan wave:** Run `cargo test --workspace --locked`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 120 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| (filled by planner) | — | — | HREG-03 / HREG-04 | — | unsafe files left untouched; quoted command invokes exact executable | unit + integration | (filled by planner) | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements — Phase 8 landed
`TestRedirect`, owner-struct fixtures, and `BAUDE_TEST_FIXTURE_ROOT`
containment; hook.rs already carries a unit-test module.

---

## Manual-Only Verifications

All phase behaviors have automated verification (the two v2.1.3
inspection-only lock behaviors are being converted to automated tests this
phase per success criterion 4).

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
