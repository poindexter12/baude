---
phase: "10"
slug: "clickable-terminal-links"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-09-15"
---

# Phase 10 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust workspace, incl. vendored vt100 fork tests) |
| **Config file** | Cargo.toml (workspace root) |
| **Quick run command** | `cargo test -p baude-core --lib pty:: && cargo test -p vt100` |
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
| (filled by planner) | — | — | LINK-01..08 | — | invalid targets non-activatable; argv-data opener; output never opens | unit + integration | (filled by planner) | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements — Phase 8 fixture
isolation stands; the vendored vt100 fork imports with its upstream test
suite intact.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Real terminal-emulator hint overlay + open gesture dogfood | LINK-04/05 | Visual/interactive in a live terminal | Run baude, emit OSC8 + bare URLs in a pane, invoke hint mode, preview/copy/open |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 140s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
