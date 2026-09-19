# baude

## What This Is

baude is a Rust workspace for running and orchestrating many coding-agent
sessions at once. Its ratatui **TUI** (`baude`) manages Claude Code and OpenCode
sessions across repositories and git worktrees, while a headless **daemon**
(`bauded`) owns sessions over REST/SSE/WebSocket and a phone-first **PWA** makes
them available remotely. It is single-user and self-hosted, secured by binding
a Tailscale/VPN interface rather than an auth layer.

## Core Value

You can see at a glance which of your many coding-agent sessions needs you next —
and act on it — whether you're at the terminal or on your phone.

## Current State (v2.2.0 shipped 2026-09-19)

v2.2.0 is the current release and the source baseline for the next milestone. The latest GSD-completed milestone is v2.2 through Phase 12 (phases 8-12, archived under `milestones/v2.2-*`). The codebase is about 51.6k lines of Rust across `baude-core`, `baude`, and `bauded`.

## Current Milestone: v2.3 Launch Defaults and Startup Speed

**Goal:** Opening baude from any folder with no arguments lands in the right workspace, offers the right repo, never collides with another repo's worktrees, and reaches the first frame fast.

**Target features:**
- Derive the workspace from the launch folder when nothing is passed: nearest recorded folder binding walking up from the launch dir, else the repo root's folder name; explicit env/config still win and the derived binding is recorded.
- Default new-session and open paths to the launch repository's git root, ahead of `new_session_dir`, which applies only outside any repository.
- Give managed worktree paths a stable repository identity so `repository-<key>` collisions across TUI and daemon state files (or after a state reset) cannot happen; migrate existing worktrees in place without deleting anything.
- Make startup measurable and fast: a timing facility, a non-blocking kitty keyboard probe, and lazy or parallel session restore with far fewer full-state fsyncs so the first frame renders before the first metadata poll.

## Requirements

### Validated

<!-- Shipped and confirmed valuable. -->

- ✓ Multi-session TUI with stable sidebar order, in-place waiting flash, and live status (waiting/working/exited) — v0.1–v0.2
- ✓ Git-worktree sessions for parallel work in one repo; keep/remove on close — v0.1
- ✓ Per-session shell pane and "open folder in editor" — v0.2
- ✓ Live Claude metadata from disk (model, context %, permission mode, tokens, GSD state) — v0.3
- ✓ Usage/cost panel: per-session cost, today/week via ccusage, 5h + weekly rate-limit windows; `baude statusline` bridge — v0.3
- ✓ Headless `bauded` daemon: REST + SSE, sessions survive client detach, restore via `claude --continue` — v0.4
- ✓ Shared core lifecycle state machine governing Git, durable commit stages, exact process ownership, recovery, and rollback across App and Manager — v2.0
- ✓ Persistent repository parents with checkout/worktree children, stable durable ordering, and context-aware actions — v2.0
- ✓ Branch worktree create/activate, retained close, reopen, and seed-aware verified safe removal — v2.0
- ✓ Standalone non-git folder sessions as first-class durable rows — v2.0 (added mid-milestone from dogfood feedback)
- ✓ v2.0.0-beta released (manual bootstrap + release-please beta channel), 4-target tarballs bundling both binaries — v2.0 (publish decision overrode the original no-publish framing)
- ✓ Containerized deploy: Dockerfile + compose behind a Tailscale sidecar (VPN-only) — v0.4
- ✓ Phone PWA: triage list, chat with live SSE, queued-message bubbles, terminal-peek drawer, interrupt, create/kill/restart — v0.4
- ✓ TUI attaches to remote daemon sessions over WebSocket (raw PTY) — v0.5
- ✓ Web Push notifications when a session waits or exits — v0.5
- ✓ Idle-session archiving: auto after 30m, manual everywhere — v0.6
- ✓ Full Claude Code status-line payload capture (model, effort, thinking, PR, worktree, vim) via the schema:2 bridge — v0.7
- ✓ Hook-driven working/waiting/done state (Claude Code hooks; silence heuristic demoted to a labeled `StateSource` fallback) — v0.7
- ✓ Live per-session tool-activity timeline (capped ring → `GET /activity` + SSE) in the PWA and TUI — v0.7
- ✓ Remote tool-permission approve/deny from the phone (opt-in `prompt` mode via `--permission-prompt-tool` MCP bridge; distinct push) — v0.7
- ✓ Backend-isolated Claude Code and OpenCode workspaces, commands, metadata, and session pools — v0.8-v0.14
- ✓ Hook configuration no longer accumulates (#70): idempotent, guarded seeding preserves user hooks and settings — v2.2
- ✓ State-lock contention is diagnosed (#71) without removing locks or overwriting a live owner — v2.2
- ✓ Test worktrees and environments are isolated per fixture (#72); fail-closed managed-worktree leak scan, preview-only by default — v2.2
- ✓ OSC8 labeled links and bare URLs open on gesture (`ctrl+o` hints) with destination preview and copy, no shell evaluation, selection preserved — v2.2
- ✓ Shift+Enter inserts a newline on kitty-protocol terminals with mode restoration and a legacy fallback — v2.2
- ✓ v2.2.0 published through the existing release process after test, CI, and smoke validation — v2.2

> v0.7 code-complete; data paths Claude-validated live (4 integration bugs found + fixed). Pending human UATs before public ship: hook-state flip visual (BL-01), PWA activity-strip + TUI `v` overlay visuals, live-`claude` `--permission-prompt-tool` MCP wire contract, first-phone Web Push. Tracked in `.planning/STATE.md` Deferred Items + per-phase UAT.md.

### Active

<!-- Current milestone scope. -->

- [ ] Workspace derives from the launch folder when nothing is passed (nearest ancestor binding, else repo-root name), with explicit env/config still winning.
- [ ] New-session and open default to the launch repository's git root; `new_session_dir` applies only outside a repository.
- [ ] Managed worktree paths carry a stable repository identity; no `repository-<key>` collisions across TUI/daemon state or after a reset; existing worktrees migrate in place.
- [ ] Startup is measurable (timing facility) and fast (non-blocking keyboard probe, lazy/parallel restore, batched state writes, first frame before first poll).

### Out of Scope

<!-- Explicit boundaries. -->

- Multi-user / auth layer — security model is "bind the VPN interface"; single-user by design
- Native Claude remote (claude.ai/code, `--remote-control`) as the backend — baude owns its own stack
- Supporting coding agents other than Claude Code and OpenCode — backend support is intentionally explicit
- Remote vt100 rendering as the primary remote UX — the message/chat model is the core; raw PTY is an escape hatch
- Dormant local branch rows, dormant-branch activation UI, and safe branch deletion — deferred to a future milestone
- Daemon-backed remote TUI and PWA repository hierarchy/action parity — deferred to a future milestone; existing flat APIs remain non-destructive compatibility projections
- Proxy-monitor integration: deferred until the external telemetry contract is resolved; this milestone will not claim a confirmed protocol solution

## Context

- Mature codebase at **v2.2.0** (shipped 2026-09-19), about 51.6k lines of Rust; public repo `github.com/poindexter12/baude`, MIT.
- Cargo workspace: `baude-core/` (pty, session, meta, persist, git, bridge — no UI deps), `baude/` (ratatui TUI), `bauded/` (axum daemon + embedded PWA).
- Distributed as prebuilt binaries via `mise`/`ubi` (release.yml builds 4 targets) and a multi-arch `ghcr.io/poindexter12/bauded` image.
- CI gates on `cargo fmt --check` + `clippy -D warnings` + tests — all three must pass before push.
- The active workspace binds a backend and keeps Claude Code and OpenCode session pools, commands, state files, and daemon ports isolated.
- Worktree creation/removal and dirty-state checks already exist in `baude-core/src/git.rs`; v2.0 changes the product model from a flat session list to a persistent repository hierarchy.
- Phases 5 through 7 are retained as completed v2.0 history. The Phase 6 corrective shared-core work is shipped, not an active v2.2 blocker; explicitly deferred human verification remains recorded in STATE.md.
- The v2.2 GSD-completed milestone runs through Phase 12; the next phase number is 13.
- Workspace (state namespace) resolution chain: `BAUDE_WORKSPACE` -> folder-memory hint (`~/.config/baude/folder-workspaces.json`, keyed by exact canonical launch dir) -> config `workspace` -> `BAUDE_BACKEND` -> config `backend` -> `claude`. The launch folder itself never names the workspace.
- Managed worktrees live at `~/.local/share/baude/worktrees/<workspace>/repository-<key>/<primary|branch-label>-<key>`; keys are per-state-file counters (TUI `state-<ws>.json` and daemon `daemon-state-<ws>.json` each start at 1) and the path carries no repository identity, so two repos can claim the same `repository-1` and collide as a hard `PathCollision` error.
- User config is `~/.config/baude/config.json` (`claude_cmd`, `new_session_dir`, `clone_base_dir`, `workspace`/`workspaces`, `auto_archive_minutes`); the `n` prompt prefills `new_session_dir` when set, else the launch dir (argv[1] or cwd, canonicalized, not the git toplevel).

## Constraints

- **Tech stack**: Rust (ratatui TUI, axum/tokio daemon, portable-pty + vt100); vanilla JS/CSS PWA embedded in the binary with no build step — keep it that way.
- **Security**: VPN/Tailscale-only; no auth layer is added. New endpoints inherit this model.
- **Compatibility**: backend-specific integrations must tolerate upstream schema drift; pin verified Claude Code and OpenCode versions in comments where wire assumptions are made.
- **Safety**: managed sessions run `--dangerously-skip-permissions` for unattended work; any permission-prompting mode is opt-in and must not become the unattended default. Preserve user hooks and settings, never remove state locks or overwrite another live owner, isolate tests per fixture without global-environment races, and never automatically delete real user worktrees; cleanup requires preview and manual approval.
- **Terminal compatibility**: links must open only on user gesture without shell evaluation and must preserve text selection; terminal mode must be restored, with a legacy fallback when supported input protocols are unavailable.
- **Release validation**: publish v2.2.0 only after test, CI, and smoke checks pass through the existing release process.
- **No regressions**: stable sidebar order and the dual-source (session-file + silence-fallback) waiting logic are hard-won; changes must preserve current behavior as a labeled fallback.

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| GSD-track baude starting at v0.7 (lean scaffold, no full re-interview) | Codebase is mature and well-understood; a full new-project interview would re-derive known facts | ✓ Good — v0.7 shipped 4 phases through the full GSD chain |
| Prefer first-party Claude data (status-line JSON, hooks) over inference | Accuracy + unlocks PR/effort/activity data for free | ✓ Good — hooks now drive state; silence is a labeled fallback (v0.7) |
| Local hook transport via per-session event files; HTTP only in the daemon | Matches existing `meta.rs`/bridge file-tail patterns; avoids a new bind for the TUI | ✓ Good — one event model serves file-tail + daemon POST (v0.7) |
| Permission-prompt mode is opt-in; `skip` stays default | Unattended overnight runs must not block on phone approval | ✓ Good — fail-safe default-stays-skip + deny-on-timeout, security-reviewed (v0.7) |
| `--permission-prompt-tool` requires a stdio MCP server (not a plain command) | Pinned by v0.7 research; baude hand-rolls a 3-method JSON-RPC server in both binaries, no new deps | ⚠️ Revisit — wire contract is MEDIUM-confidence (claude-code #1175); confirm against live claude 2.1.178 before public ship |
| Narrow v2.0 to shared lifecycle ownership plus a local-TUI dogfood release | Deep Phase 6 review exposed duplicated App/Manager ownership and unsafe recovery transitions; remote/PWA and dormant-branch breadth would compound that risk | Completed in v2.0; later publication superseded the original no-publish boundary |
| Fail-closed managed-worktree leak scan; removal only behind a saved report plus two opt-ins | 1433 live candidates matched the naive "path shape + missing gitdir" signal; deletion must never be authorized by evidence every live worktree satisfies | ✓ Good — shipped v2.2, preview-only default |
| One `TestRedirect` owns all fixture paths via a cargo feature (not `cfg(test)`), with an escape guard | Tests leaked worktrees into the real home (#72); `cfg(test)` cannot reach the binaries' integration tests | ✓ Good — 16 isolation blockers closed, 645 tests green |
| Vendor and fork vt100 0.15.2 to carry per-cell OSC8 link ids; collect links at gesture time only | Upstream has no link-id plumbing; gesture-time collection leaves the render path unchanged | ✓ Good — links shipped with remote-attach parity; the fork's lint header had to be relaxed (12-01) |
| Shift+Enter via negotiated kitty keyboard protocol, doubly verified (outer terminal and child), legacy bytes frozen in a corpus | Blind CSI-u would break terminals and children that do not speak kitty | ✓ Good — honest fallback, byte-freeze corpus guards regressions |
| Guarded settings seeding: never overwrite unreadable, unparseable, or non-object settings; warn instead | Silent replacement destroyed user hooks (#70) | ✓ Good — HREG-03/04 delivered |
| Ship v2.2.0 through release-please; close GSD v2.2 by override (no Phase 12 VERIFICATION.md) and no separate GSD tag | Release already published with CI green; GSD verification would duplicate 12-VALIDATION and the smoke evidence | ⚠️ Revisit — run `/gsd-verify-work` before close next time |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-09-19 after v2.3 milestone start*
