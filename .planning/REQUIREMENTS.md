# Requirements: baude — v0.7 Native Claude integration

**Defined:** 2026-06-15
**Core Value:** You can see at a glance which of your many Claude Code sessions needs you next — and act on it — whether you're at the terminal or on your phone.

This milestone replaces inferred session state with first-party Claude Code data
(status-line payload + hooks), then builds a live tool-activity feed and remote
permission approval on top of it. Full plan: `docs/plans/tier-1-native-claude-integration.md`.

## v1 Requirements

Requirements for the v0.7 milestone. Each maps to roadmap phases.

### Status-line capture (STL)

- [ ] **STL-01**: The `baude statusline` bridge persists the full useful payload (model, effort, thinking, pr, worktree, vim.mode), each optional, tolerating snake/camel key drift like `bridge.rs::window()` does today
- [ ] **STL-02**: Bridge JSON is versioned (`schema: 2`); `meta.rs` reader and `ClaudeMeta` gain the new optional fields without breaking existing readers
- [ ] **STL-03**: The `i` info overlay surfaces effort, thinking mode, and PR state for the selected session

### Hook-driven status (HOOK)

- [ ] **HOOK-01**: On spawn, a hook set is seeded into the managed session's `settings.json` by merging into existing arrays — never clobbering a user's existing hooks or statusline (reuse the container's statusLine-seeding path)
- [ ] **HOOK-02**: A session's working/waiting/done state derives from Claude Code hooks (`UserPromptSubmit` → turn started, `Stop` → turn done/waiting, `Notification` → waiting for permission/input, `PostToolUse` → ran tool X); the PTY-silence heuristic remains only as a labeled fallback
- [ ] **HOOK-03**: Hook events transport via a per-session file-tail (`/tmp/baude-events-<sid>.jsonl`) for TUI-local sessions and `POST /sessions/{id}/event` in the daemon; one event model serves both

### Tool-activity timeline (ACT)

- [ ] **ACT-01**: `manager.rs` keeps a per-session capped ring buffer (~200) of recent tool events
- [ ] **ACT-02**: `GET /sessions/{id}/activity` returns recent events and they stream live (SSE channel, standalone or folded into `/stream`)
- [ ] **ACT-03**: The PWA chat view shows a collapsible activity strip ("editing src/foo.rs → running cargo test → …")
- [ ] **ACT-04**: The TUI has an activity overlay (key `v`) mirroring the feed

### Remote permission approval (PERM)

- [ ] **PERM-01**: A per-deploy `BAUDE_PERMISSION_MODE = skip | prompt` (default `skip`) controls whether managed sessions run `--dangerously-skip-permissions` or route tool calls to baude via `--permission-prompt-tool`; `skip` preserves today's unattended behaviour
- [ ] **PERM-02**: `GET /sessions/{id}/permission` returns the pending request (if any); `POST /sessions/{id}/permission {decision: allow|deny, scope?}` resolves it
- [ ] **PERM-03**: The PWA chat view shows an approve/deny card while a permission request is pending
- [ ] **PERM-04**: A pending permission fires a distinct push (e.g. "wants to run `rm -rf build/` — approve?") separate from the generic "waiting" push, driven by the `Notification` hook and a `waiting_reason` (`permission` | `input` | none) on `SessionInfo`

## v2 Requirements

Deferred to future milestones. Tracked but not in the v0.7 roadmap.

### Diff / review loop (DIFF) — Tier 2

- **DIFF-01**: Read surface for git diff/status/PR state per session
- **DIFF-02**: Diff viewer in TUI and PWA; inline-comment → follow-up prompt
- **DIFF-03**: PR lifecycle (create/view status) from baude

### Orchestration (ORCH) — Tier 3

- **ORCH-01**: Race N worktree sessions on one prompt, compare diffs, pick a winner
- **ORCH-02**: Cross-session prompt/task queue (Dispatcher), persisted across restart
- **ORCH-03**: Scheduled / webhook-triggered session launch with a safety gate

### Ergonomics (ERGO) — Tier 4

- **ERGO-01**: Fuzzy switcher + command palette (`/` filter, `:` command)
- **ERGO-02**: Tags / grouping / sort / filter (opt-in, stable order preserved)
- **ERGO-03**: Frecency ranking + last-two quick-toggle
- **ERGO-04**: Opt-in per-repo worktree bootstrap script

## Out of Scope

Explicitly excluded for v0.7 (and structurally, where noted).

| Feature | Reason |
|---------|--------|
| Multi-user / auth layer | Security model is "bind the VPN interface"; single-user by design — structural, not just deferred |
| Native Claude remote (claude.ai/code, `--remote-control`) as backend | baude owns its own stack |
| Supporting agents other than Claude Code | baude is Claude-native on purpose |
| Remote vt100 rendering as the primary remote UX | The message/chat model is core; raw PTY is an escape hatch |
| Permission-prompt mode as the unattended default | Prompting blocks overnight runs on phone approval; `skip` stays default, `prompt` is opt-in (PERM-01) |

## Traceability

Mapped to roadmap phases (see `.planning/ROADMAP.md`).

| Requirement | Phase | Status |
|-------------|-------|--------|
| STL-01 | Phase 1 | Pending |
| STL-02 | Phase 1 | Pending |
| STL-03 | Phase 1 | Pending |
| HOOK-01 | Phase 2 | Pending |
| HOOK-02 | Phase 2 | Pending |
| HOOK-03 | Phase 2 | Pending |
| ACT-01 | Phase 3 | Pending |
| ACT-02 | Phase 3 | Pending |
| ACT-03 | Phase 3 | Pending |
| ACT-04 | Phase 3 | Pending |
| PERM-01 | Phase 4 | Pending |
| PERM-02 | Phase 4 | Pending |
| PERM-03 | Phase 4 | Pending |
| PERM-04 | Phase 4 | Pending |

**Coverage:**
- v1 requirements: 14 total
- Mapped to phases: 14 ✓
- Unmapped: 0 ✓

---
*Requirements defined: 2026-06-15*
*Last updated: 2026-06-15 after roadmap creation for milestone v0.7*
