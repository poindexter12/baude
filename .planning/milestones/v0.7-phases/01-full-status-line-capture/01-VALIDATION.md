---
phase: 1
slug: full-status-line-capture
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-06-15
---

# Phase 1 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness (`cargo test`) |
| **Config file** | none — `#[cfg(test)]` modules added in Wave 0 (bridge.rs and meta.rs have no test module today) |
| **Quick run command** | `cargo test -p baude-core` |
| **Full suite command** | `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` |
| **Estimated runtime** | ~30–90 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p baude-core`
- **After every plan wave:** Run `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 90 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 1-01-01 | 01 | 1 | STL-01 | — | N/A | unit | `cargo test -p baude-core bridge` | ❌ W0 | ⬜ pending |
| 1-01-02 | 01 | 1 | STL-02 | — | older reader parses schema:2 without error | unit | `cargo test -p baude-core meta` | ❌ W0 | ⬜ pending |
| 1-01-03 | 01 | 1 | STL-03 | — | N/A | manual | TUI `i` overlay shows effort/thinking/PR | — | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky · Map refined by the planner during PLAN.md authoring.*

---

## Wave 0 Requirements

- [ ] `baude-core/src/bridge.rs` — add `#[cfg(test)] mod tests` covering full-payload capture + snake/camel tolerance (STL-01)
- [ ] `baude-core/src/meta.rs` — add `#[cfg(test)] mod tests` covering additive deserialization + schema:2 back-compat against an old-shape fixture (STL-02)

*Rust uses inline `#[cfg(test)]` modules — no separate framework install needed.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| `i` info overlay renders effort/thinking/PR rows | STL-03 | TUI rendering against a live session; no headless ratatui assertion harness in-repo | Run baude, select a session with effort/thinking/PR present in its bridge file, press `i`, confirm the three rows render (and degrade to `—` when absent) |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 90s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
