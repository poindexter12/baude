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

## Current State (v2.6.0; last GSD milestone v2.3 shipped 2026-09-22)

v2.6.0 is the current release. The latest GSD-completed milestone is v2.3 through Phase 17 (phases 13-17, archived under `milestones/v2.3-*`, closed out 2026-10-13). v2.3.1 through v2.6.0 shipped from the backlog and from sidequest tickets rather than from GSD phases, so no roadmap phase covers them. The codebase is about 62.3k lines of Rust across `baude-core`, `baude`, and `bauded`.

## Current Milestone: none

v2.3 is closed. The next milestone has not been scoped — run `/gsd-new-milestone` to open one.

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
- ✓ Workspace derives from the launch folder when nothing is passed (nearest ancestor binding, else sanitized repo-root name, recorded at the repo root); explicit env/config still win; TUI and daemon share one `launch::start_workspace` resolver — Phase 13
- ✓ New-session and open default to the launch repository's git root; `new_session_dir` applies only outside a repository; the TUI title (and remote header via `/info` `workspace_source`) shows the workspace with its source or `(blank)` — Phase 13
- ✓ Managed worktree paths carry a stable repository identity (SHA256 digest of the canonical common directory, claimed through an atomic marker file); `repository-<key>` can no longer collide across TUI/daemon state or after a reset, and existing worktrees migrate in place — v2.3 Phase 14
- ✓ Startup is measurable stage by stage (`BAUDE_TIMING=1`), the kitty keyboard probe is bounded at 250 ms, and session restore no longer blocks the first frame — v2.3 Phase 15
- ✓ Idle baude costs no measurable CPU: dirty-flag redraw, live-row-only metadata polling with mtime gating, opt-in idle-child suspension via SIGSTOP/SIGCONT, and a configurable (disableable) usage poller — v2.3 Phase 15, measured at 0.000000 CPU-seconds per idle minute
- ✓ Static single-character status codes replace the wall-clock spinner and needs-input flash, so nothing repaints on a timer — v2.3 Phase 15 (UX-02)
- ✓ Pane focus is remembered across session switches — v2.3 Phase 16 (UX-01)

> v0.7 code-complete; data paths Claude-validated live (4 integration bugs found + fixed). Pending human UATs before public ship: hook-state flip visual (BL-01), PWA activity-strip + TUI `v` overlay visuals, live-`claude` `--permission-prompt-tool` MCP wire contract, first-phone Web Push. Tracked in `.planning/STATE.md` Deferred Items + per-phase UAT.md.

### Active

<!-- Current milestone scope. -->

No milestone is open. Candidates carried out of v2.3:

- [ ] Reduce first-frame latency. v2.3 made startup *measurable* and idle *quiet*, which it set out to do, but the measured `first_frame` is 5,537 ms of a 6,859 ms total on a release binary with an empty state. Cutting that was never in v2.3's scope and is still open.
- [x] Replace `timing_disabled_when_env_unset` with a test that exercises the production `BAUDE_TIMING` gate rather than re-reading the environment variable. Done (SQ-12): the gate is now `timing_requested`, called by `main` and tested over env values crossed with the flag.

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

- Mature codebase at **v2.6.0**, about 62.3k lines of Rust; public repo `github.com/poindexter12/baude`, MIT.
- Cargo workspace: `baude-core/` (pty, session, meta, persist, git, bridge — no UI deps), `baude/` (ratatui TUI), `bauded/` (axum daemon + embedded PWA).
- Distributed as prebuilt binaries via `mise`/`ubi` (release.yml builds 4 targets) and a multi-arch `ghcr.io/poindexter12/bauded` image.
- CI gates on `cargo fmt --check` + `clippy -D warnings` + tests — all three must pass before push.
- The active workspace binds a backend and keeps Claude Code and OpenCode session pools, commands, state files, and daemon ports isolated.
- Worktree creation/removal and dirty-state checks already exist in `baude-core/src/git.rs`; v2.0 changes the product model from a flat session list to a persistent repository hierarchy.
- Phases 5 through 7 are retained as completed v2.0 history. The Phase 6 corrective shared-core work is shipped, not an active v2.2 blocker; explicitly deferred human verification remains recorded in STATE.md.
- The v2.3 GSD-completed milestone runs through Phase 17; the next phase number is 18.
- Full-suite cost is host-dependent by roughly an order of magnitude. The same 832 tests across 10 binaries, both measured 2026-10-13: 59 s run serially on a 3-core CI runner with a warm cache, versus 659 s wall clock on a loaded dev host with a cold compile. Each test spawns a real `git` inside a contained fixture root, which makes the suite sensitive to host load rather than intrinsically slow. Take any timeout from the host that will actually run it — a 600 s limit chosen against the loaded figure has already cost one executor a false hang report.
- Workspace (state namespace) resolution chain: `BAUDE_WORKSPACE` -> folder-memory hint (`~/.config/baude/folder-workspaces.json`, keyed by exact canonical launch dir) -> config `workspace` -> `BAUDE_BACKEND` -> config `backend` -> `claude`. The launch folder itself never names the workspace.
- Managed worktrees live at `~/.local/share/baude/worktrees/<workspace>/repository-<key>/<primary|branch-label>-<key>`; keys are per-state-file counters (TUI `state-<ws>.json` and daemon `daemon-state-<ws>.json` each start at 1). Since v2.3 Phase 14 each path also carries a stable `physical_key` — a SHA256 digest of the canonical common directory, claimed through an atomic marker file — so two repos can no longer collide on the same `repository-1`; collisions that do occur are reported through `collision_line()` in TUI, daemon, and scan output.
- User config is `~/.config/baude/config.json` (`claude_cmd`, `new_session_dir`, `clone_base_dir`, `workspace`/`workspaces`, `auto_archive_minutes`); the `n` prompt prefills `new_session_dir` when set, else the launch dir (argv[1] or cwd, canonicalized, not the git toplevel).

## Constraints

- **Tech stack**: Rust (ratatui TUI, axum/tokio daemon, portable-pty + vt100); vanilla JS/CSS PWA embedded in the binary with no build step — keep it that way.
- **Security**: VPN/Tailscale-only; no auth layer is added. New endpoints inherit this model.
- **Compatibility**: backend-specific integrations must tolerate upstream schema drift; pin verified Claude Code and OpenCode versions in comments where wire assumptions are made.
- **Safety**: managed sessions run `--dangerously-skip-permissions` for unattended work; any permission-prompting mode is opt-in and must not become the unattended default. Preserve user hooks and settings, never remove state locks or overwrite another live owner, isolate tests per fixture without global-environment races, and never automatically delete real user worktrees; cleanup requires preview and manual approval.
- **Terminal compatibility**: links must open only on user gesture without shell evaluation and must preserve text selection; terminal mode must be restored, with a legacy fallback when supported input protocols are unavailable.
- **Release validation**: publish only after test, CI, and smoke checks pass through the existing release process. release-please owns semver tags (`vX.Y.Z`); `git.create_tag` is `false` in `.planning/config.json` so GSD never mints a bare `vX.Y` tag into that namespace.
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
| One shared `launch::start_workspace(launch_dir, config, env, lock_base)` in baude-core drives both TUI (`"state"`) and daemon (`"daemon-state"`) startup: canonicalize, ancestor-walk binding lookup, repo-root derivation, cache seeding, lock claim, then binding record | Two hand-rolled startup paths had already diverged (daemon never claimed a lock); one seam makes parity testable and keeps the write order fixed | ✓ Good — Phase 13, parity and end-to-end tests pass |
| A held workspace lock refuses startup (`StartError::LockHeld`) and never records a binding | Preserves the v2.1.3 single-writer invariant (#71); continuing without the lock would let two instances write one state file | ✓ Good — Phase 13 |
| Derived workspace bindings are keyed by the repository root, never the launch subfolder, and unbound non-git folders record nothing | Subfolder launches must resolve to the same workspace; non-git folders have no stable identity to bind | ✓ Good — Phase 13 |
| The TUI title renders `name (source)` for explicit, bound, and derived workspaces and exactly `(blank)` for the implicit default | An unconfigured launch should look unconfigured instead of showing the backend name as if chosen | ✓ Good — Phase 13 (Joe's request) |
| Phase 13 plans went through four Codex convergence cycles then executed with remaining findings accepted; the 13-02 TDD gate (no `feat(13-02)` GREEN commit) was accepted as debt | Each cycle surfaced new source-level facts rather than regressions; the tests exist and pass | ⚠️ Revisit — split behavior fixes out of test commits in later TDD plans |
| Replace the wall-clock spinner and needs-input flash with static status glyphs (2026-09-20) | Timer-driven animation forces a repaint every tick even when nothing changed; a static busy/thinking icon lets the dirty-flag redraw fire only on real state changes, which is what makes idle CPU reach zero | ✓ Good — Phase 15; idle measured at 0.000000 CPU-seconds per minute over a 61 s real-PTY window |
| Status indicators become single-character codes (`?` `B` `✓` `✗` `-` `A` `!`) keeping the existing per-state colors (2026-09-20) | Filled/empty circles and blue-vs-yellow carried meaning only with a mental key; a static indicator has no motion to hint at state, so the character itself must say what is happening | ✓ Good — Phase 15 (UX-02), pinned by `status_code_table_matches_ux_02` |
| Repository identity is a SHA256 digest of the canonical common directory, claimed through an atomic marker file, rather than a per-state-file counter | Counters restart at 1 per state file, so TUI and daemon independently mint `repository-1` for different repos; identity has to come from the repo itself to survive a state reset | ✓ Good — Phase 14, migrates in place without deleting anything |
| Suspend idle Claude children with SIGSTOP/SIGCONT, opt-in | A backgrounded agent process keeps costing CPU and battery with nothing to do; stopping it is reversible and cheap, but pausing someone's running work by default is not acceptable | ✓ Good — Phase 15, shipped opt-in |
| Close v2.3 as `override_closeout` on stale verification rather than re-verifying 5 phases (2026-10-13) | Every phase passed in September; the staleness is digest drift from main advancing through v2.6. Re-running the verifier would check v2.3 goals against three milestones of later rework and report drift as failure | — Pending — honest for a late closeout, but it is the second milestone in a row to close by override |
| Leave Phase 13 and 14 `VALIDATION.md` as unfilled templates rather than back-filling them at closeout (2026-10-13) | A validation contract invented after the fact never governed the execution it claims to describe; a visible gap is worth more than a fabricated record | ✓ Good — recorded in MILESTONES.md Known Gaps |
| GSD must not create tags: `git.create_tag: false` (2026-10-13) | release-please owns the `vX.Y.Z` namespace; GSD's closeout would mint a bare `v2.3`, colliding with it | ✓ Good — no bare GSD tag exists for v2.3 |

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
*Last updated: 2026-10-13 after v2.3 milestone closeout*
