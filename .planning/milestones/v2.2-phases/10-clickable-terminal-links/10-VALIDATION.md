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
| 10-01.T1 (tracer) | 10-01 | 1 | LINK-01, 04, 05, 07, 08 | T-10-01..05, T-10-SC | destination from link_target never label; validate_http_url gate; argv-data injected open | unit e2e (RED-first) | `cargo test -p vt100 hyperlink --locked && cargo test -p baude links:: --locked && cargo test -p baude link_hints --locked && cargo check --workspace --all-targets --locked` | ⬜ created in-task | ⬜ pending |
| 10-01.T2 | 10-01 | 1 | LINK-04 (documented gesture) | — | — | grep-positive + compile | `grep -q "link hints" baude/src/ui.rs && grep -q "0.15.2" vendor/vt100/README.md && grep -qi "opener" .planning/WINDOWS.md && cargo check -p baude --locked` | ⬜ created in-task | ⬜ pending |
| 10-02.T1 (RED) | 10-02 | 2 | LINK-01, 03 | T-10-06..09 | erased cells carry no link; caps fail closed | tdd-red-evidence gate | `gsd_run check tdd-red-evidence .planning/phases/10-clickable-terminal-links/10-02-red-evidence.json` | ⬜ created in-task | ⬜ pending |
| 10-02.T2 (GREEN) | 10-02 | 2 | LINK-01, 03 | T-10-06..09 | snapshot round-trip parity; bounded intern table | unit fork + round-trip | `cargo test -p vt100 --locked && cargo test -p baude-core --lib pty:: --locked && cargo check --workspace --all-targets --locked` | ⬜ created in-task | ⬜ pending |
| 10-03.T1 (RED) | 10-03 | 2 | LINK-02, 07 | T-10-10..13 | rejection matrix incl. pre/post-decode controls | tdd-red-evidence gate | `gsd_run check tdd-red-evidence .planning/phases/10-clickable-terminal-links/10-03-red-evidence.json` | ⬜ created in-task | ⬜ pending |
| 10-03.T2 (GREEN) | 10-03 | 2 | LINK-02, 07 | T-10-10..13 | only validated http(s) collected; fail closed | unit pure fn | `cargo test -p baude links:: --locked && cargo check --workspace --all-targets --locked` | ⬜ created in-task | ⬜ pending |
| 10-04.T1 | 10-04 | 3 | LINK-05, 06 | T-10-15 | copy emits full destination; truncation render-only | unit (injected copy sink) | `cargo test -p baude link_hints --locked && cargo check --workspace --all-targets --locked` | ⬜ created in-task | ⬜ pending |
| 10-04.T2 | 10-04 | 3 | LINK-03 (gesture leg), 04 | T-10-17 | modal swallows all keys pre-forwarding; selection untouched | unit integration | `cargo test -p baude link_hints --locked && cargo test -p baude --locked && cargo check --workspace --all-targets --locked` | ⬜ created in-task | ⬜ pending |
| 10-04.T3 | 10-04 | 3 | LINK-08 | T-10-14, 16 | opener Err → one warning, session alive | unit (injected opener) + full suite | `cargo test -p baude link_open --locked && cargo test --workspace --locked` | ⬜ created in-task | ⬜ pending |

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
