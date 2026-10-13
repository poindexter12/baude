# Milestones

## v2.3 Launch Defaults and Startup Speed (Shipped: 2026-09-22)

**Phases completed:** 5 phases (13-17), 12 plans, 21 tasks
**Release:** v2.3.0 via release-please (PR #91); source range v2.2.0..v2.3.0 = 2 commits, 196 files, +27,188/-1,581. The whole milestone landed as one squash commit `ab72a36` ("feat: v2.3 launch defaults and startup speed", #90) plus the release commit.
**Timeline:** 2026-09-19 (v2.2.0 baseline) -> 2026-09-22 (v2.3.0 published); GSD phases executed 2026-09-19 -> 2026-09-22; records closed out 2026-10-13
**Closeout type:** override_closeout
**Known verification overrides:** 0 newly acknowledged, 14 carried forward from a prior close (see STATE.md Deferred Items)

**Delivered:** baude now starts in the right place and stops working when you are not. Launching from any folder derives the correct workspace and repository without configuration, managed worktrees carry a stable identity that survives being moved, startup is measurable stage by stage, an idle instance draws nothing and burns no measurable CPU, and pane focus survives session switches — published as v2.3.0.

**Key accomplishments:**
- Workspace derivation from repository root, ancestor walk for stable folder bindings, source-labeled TUI title, and shared startup helper for TUI/daemon parity
- Comprehensive TDD test suite for ancestor walk, workspace derivation, precedence chain, new-session prefill, and repository deduplication
- Daemon calls shared startup helper with lock_base="daemon-state", exposes workspace_source via /info endpoint, displays remote workspace with source label, and comprehensive README documents full precedence chain and derivation rule with examples
- Atomic marker file ownership claim and stable repository identity via SHA256 digest of canonical common directory
- Metadata polling optimization with archived/exited gating and mtime checks; idle child suspension via SIGSTOP/SIGCONT; configurable usage poller interval with disable option

### Known Gaps

- All five phases read `verification_status: stale` at closeout, so this closed as `override_closeout`. Each had passed on its own evidence at execution time — Phase 13 9/9 (2026-09-20), Phase 14 25/25 (2026-09-21), Phase 15 9/9 (2026-09-22), with 16 and 17 likewise `passed`. The staleness is digest drift, not a failure: records were closed on 2026-10-13, by which point main had advanced through v2.4, v2.5, and v2.6. Re-running the verifier would have checked v2.3 phase goals against three milestones of later rework.
- Phase 13 and Phase 14 `VALIDATION.md` were never filled in by `validate-phase` — both remain the seeded template, with literal `{pytest 7.x / jest 29.x}` placeholders, and are left at `status: draft` rather than back-filled, since a contract invented at closeout never governed the execution it would claim to describe. Phase 15 is the only completed validation contract in this milestone (`validated`, `nyquist_compliant: true`, approved 2026-10-13). Phases 16 and 17 have no `VALIDATION.md` at all.
- Phase 15's `timing_disabled_when_env_unset` (`baude/src/main.rs`) is a tautology — it re-reads `BAUDE_TIMING` and asserts it is unset rather than exercising the production gate. Recorded in `15-VALIDATION.md`, left unrepaired at closeout.
- PERF-01 startup wall-clock timing and real-PTY idle CPU are Manual-Only by necessity; host-dependent figures cannot be asserted in CI. Measured evidence is recorded in `15-VALIDATION.md` (first frame 5,537 ms, total 6,859 ms, idle 0.000000 CPU-seconds per minute over a 61 s window against a 0.25 threshold).
- `first_frame` at 5,537 ms is the weakest measured result in a milestone named "Launch Defaults and Startup Speed". The phase goal was that startup be *measurable* and idle be quiet, which it is and does; actually reducing first-frame latency was never in v2.3's scope and remains open.
- No GSD `v2.3` tag was created; the release-please tag `v2.3.0` is the ship marker. `git.create_tag` is now `false` in `.planning/config.json` so GSD cannot mint a bare `vX.Y` tag into release-please's namespace.

---

## v2.2 Reliability and Terminal Usability (Shipped: 2026-09-19)

**Phases completed:** 5 phases (8-12), 25 plans, 49 tasks
**Release:** v2.2.0 via release-please (PR #88); source range v2.1.0..v2.2.0 = 205 commits, 154 files, +43,947/-2,013
**Timeline:** 2026-09-08 (v2.1.0 baseline) -> 2026-09-19 (v2.2.0 published); GSD phases executed 2026-09-13 -> 2026-09-19
**Closeout type:** override_closeout
**Known verification overrides:** 2 newly acknowledged, 12 carried forward from a prior close (see STATE.md Deferred Items)

**Delivered:** Everyday session management is safer (no hook accumulation, actionable lock diagnostics, leak-proof test fixtures, a fail-closed worktree leak scan) and the terminal is more usable (clickable OSC8 and bare-URL links with preview and copy, negotiated Shift+Enter newlines with a legacy fallback), validated and shipped as v2.2.0.

**Key accomplishments:**

- Test isolation: one `TestRedirect` owns every fixture path across all three crates (cargo feature, escape guard aborts any test that reaches the real home) and `workspace::active()` resolves per fixture, so tests can no longer leak or race through global state (#72).
- Worktree leak scan: `baude worktrees scan` classifies every managed `repository-<key>` candidate through a fail-closed verdict; removal is reachable only behind a saved report plus two opt-ins and never deletes real user worktrees.
- Hook seeding safety: guarded `settings.local.json` and `.mcp.json` seeding never overwrites unreadable or unparseable settings, seeds an always-quoted shell-safe hook command, and surfaces warnings on all spawn paths (#70, HREG-03/04); WLOCK diagnostics pinned by tests (#71).
- Clickable terminal links: a vendored vt100 fork carries per-cell OSC8 link ids; the `ctrl+o` hint overlay collects OSC8 and RFC 3986 bare URLs at gesture time with destination preview, copy, and an argv-only validated opener, with remote-attach parity.
- Negotiated multiline input: a bounded kitty keyboard probe gates Shift+Enter (ESC CR to Claude, CR to shell, CSI-u only for kitty children) with conditional pop on restore and a byte-frozen legacy fallback corpus.
- Release validation: clippy restored from exit 101 to 0 (the vendored fork's lint header had masked two real lints), README updated for links, mouse, scrollback and lock recovery, 174 phase commits rebased and pushed conflict-free, smoke evidence recorded, v2.2.0 published.

### Known Gaps

- Phase 12 was executed (5/5 plans) but never run through `/gsd-verify-work`; closed by override. SHIP-03 and SHIP-04 were marked Complete from 12-03, 12-04 and 12-05 summary evidence rather than a VERIFICATION.md.
- No GSD `v2.2` tag was created; the release-please tag `v2.2.0` is the ship marker.
- One flaky bauded lifecycle test (Phase 10 note) remains a flake-hunt candidate.

---

## v2.0 Local TUI Dogfood Release (Shipped: 2026-09-03)

**Phases completed:** 3 phases, 16 plans, 38 tasks
**Known verification overrides:** 12 newly acknowledged, 0 carried forward from a prior close (see STATE.md Deferred Items)

**Key accomplishments:**

- Byte-safe canonical repository discovery with offline remote-HEAD resolution and non-destructive, inventory-verified default-worktree reuse or creation
- Validated repository intent with byte-exact Unix paths, one-time selected-source legacy migration, typed corruption failures, and synced atomic replacement
- Canonical local admission now atomically reserves one Git-reconciled default primary before active-backend spawn, then focuses, resumes, or retains it without duplication
- Literal branch requests now create from the verified default, activate unchanged local refs, or reuse occupied worktrees through one durable App/Manager lifecycle contract
- Managed worktree activation now rejects every unsafe ref/path fact and compensates uncommitted Git additions without deleting branches or creating duplicate runtimes
- Managed worktree removal now requires conclusive porcelain-v2, recursive-submodule, ownership, topology, branch, and OID proof before an opaque removal target can exist
- Managed session close now preserves exact checkout and conversation context while committing inactive intent before stopping one App or daemon runtime
- Retained main and managed checkouts now reconcile exact Git topology, durably activate, and resume the intended Claude Code or OpenCode conversation through one checkout-key runtime
- Clean baude-managed linked worktrees now require a target-specific confirmation, a fresh post-stop authorization, plain Git removal, preservation proofs, and commit-aware runtime recovery
- One durable lifecycle engine now governs App and Manager persistence, Git/process effects, exact PTY ownership, restart, close, removal, rollback, and recovery behavior.
- Persistent repository parents and checkout children now render from durable state and retain key-based selection across refresh, removal, and focus changes.
- Durable hierarchy targets now create or activate local branches, retain-close and reopen sessions, and separately remove safe managed worktrees through Phase 6 lifecycle authority.
- Ratatui hierarchy rendering now honors the exact viewport, Unicode-cell, focus, hint, confirmation, and durable interaction contract without changing lifecycle authority.
- A fixture-isolated production App flow now proves durable real-Git restart/reopen/removal behavior while remote TUI rows and daemon sessions remain flat and non-destructive.
- All crates, packaged sources, host binaries, release proposal fields, and four supported CI archive targets now agree on `2.0.0-beta` without changing or invoking publication automation.
- Isolated `v2.0.0-beta` source-install and TUI dogfood guidance backed by a green 337-test local package/archive/install gate, with every morning certification and publication decision still pending.

---

## v0.7 Native Claude Integration (Shipped: 2026-06-16)

**Phases completed:** 4 phases, 14 plans, 28 tasks

**Key accomplishments:**

- `baude statusline` now captures the full useful Claude Code payload — model, effort, thinking, pr, worktree, vim_mode — alongside the existing four fields, stamps the bridge JSON with `schema: 2`, and proves it with a net-new `build_bridge` unit-test module.
- `ClaudeMeta` now reads the full schema:2 bridge payload — effort, thinking, vim_mode, pr, and worktree — as additive optional fields via `Value` accessors, with bridge-wins-when-present model precedence and back-compat proven in both directions by a net-new meta test module.
- The local `i` info overlay now surfaces a selected session's effort, thinking mode, and PR state as three conditional rows that are omitted entirely when the underlying ClaudeMeta field is absent — completing the user-facing goal of Phase 1 (STL-03).
- A new `baude-core::hook` module (pure `build_event` + idempotent `merge_hook_settings` + `/tmp` file-tail helpers), a `baude hook` subcommand that normalizes Claude Code lifecycle-event stdin into one schema-1 event line and routes it POST-or-append (always exiting 0), and TUI session-spawn seeding of `.claude/settings.local.json` that never clobbers a user's statusLine or hooks.
- Session working/waiting/done now derives from the Claude Code hook event stream via an offset-tracked `read_event_tail`, layered into `session.rs` behind a `StateSource{Hook,SessionFile,Silence}` precedence (Hook>SessionFile>Silence) with a `HOOK_FRESH_MS` staleness guard, while the v0.6.1 silence fallback stays byte-identical (no regression).
- The daemon now closes the hook loop: it seeds `.claude/settings.local.json` and injects `$BAUDE_EVENT_URL` at session spawn, accepts `POST /sessions/{id}/event` and feeds those lines onto the same `/tmp` consume path the poll loop tails, exposes `state_source`/`last_tool` on `SessionInfo`, and renders them minimally in the `i` info overlay — with the live end-to-end UAT left pending human verification.
- Capped (200) drop-oldest `VecDeque<HookEvent>` ring on `ClaudeMeta`, appended by `read_event_tail` and cleared on session rotation — the single source of truth for the activity timeline.
- The server half of the activity feed: a ring-backed `GET /sessions/{id}/activity` JSON endpoint (clamped `?limit`) and a standalone `GET /sessions/{id}/activity-stream` SSE channel that offset-tails the on-disk hook-event file via a dedicated `HookEvent` tail — never the ChatMessage `Tail`.
- The phone-facing tool-activity feed: a collapsible `.activity-strip` in the PWA chat view that backfills the recent ~30 events via GET-then-SSE-with-buffer (mirroring `openChat`), appends live over the standalone `/activity-stream` channel, escapes every event field against XSS, and ships behind a `baude-v3` service-worker cache bump so deployed phones refetch the embedded assets.
- A `v`-triggered `Modal::Activity` overlay rendering the recent tool sequence newest-at-bottom (mirroring the `i` Info overlay), reading `s.meta.activity()` for local sessions and a `#[serde(default)]` `RemoteInfo.activity` bundled into the `/sessions` poll for remote sessions — live-refreshing on the existing draw tick, no extra round-trip.
- Per-deploy `BAUDE_PERMISSION_MODE = skip | prompt` (default `skip`) selects exactly one permission flag for the spawned `claude` command at both spawn sites, with prompt mode additionally seeding a non-clobbering `.mcp.json` registering the `permission-mcp` stdio server — the PERM-01 security-critical default-stays-skip gate.
- A hand-rolled stdio JSON-RPC `permission-mcp` MCP server (in both binaries) that blocks on Claude's critical path POSTing each unresolved tool-permission decision to the daemon and long-polling for a human `allow`/`deny` — deny-on-timeout, never auto-allow — backed by daemon `pending_permission` state and `GET`/`POST /sessions/{id}/permission`, with the live wire contract gated by a mandatory §F human-verify UAT.
- A pure `waiting_reason` mapper in `baude_core::permission` (permission/input/none) populated on `SessionInfo` (and mirrored on `RemoteInfo`) from the already-captured `last_notification`, plus a `notified_permission` debounce set in the `Notifier` that fires ONE lean distinct permission push — re-armed when the permission resolves and mutually exclusive with the generic waiting push (PERM-04).
- A vanilla-JS approve/deny card rendered above the composer in the chat view while a permission is pending — gated on `waiting_reason === "permission"` + a live `GET /permission` fetch, every attacker-influenced string `esc()`'d — that POSTs `{decision}` to `/sessions/{id}/permission` (Approve runs the tool, Deny denies only the single tool call so the session survives) and optimistically disappears + refetches on resolve; sw.js cache bumped `baude-v3 → baude-v4`. Browser/device UAT (PERM-03) pending — vanilla PWA has no test runner.

---

Shipped history of baude, reconstructed from git tags and release notes for the
lean GSD scaffold (baude predates GSD tracking; pre-v0.7 milestones were not run
through GSD phases).

| Version | Theme | Shipped | Headline |
|---------|-------|---------|----------|
| v0.1.0 | Multi-session TUI | 2026-06-11 | Embedded PTYs, worktree sessions, persist/resume, `BAUDE_CLAUDE_CMD` |
| v0.2.0 | Sidebar UX | 2026-06-12 | Stable session order, in-place waiting flash, unified global chords, shell pane + editor key |
| v0.3.0 | Observability | 2026-06-12 | Live Claude metadata (model/context/mode/tokens/GSD), cost + rate-limit panel, `baude statusline` bridge |
| v0.4.0 | Remote daemon + PWA | 2026-06-12 | `bauded` (REST/SSE), containerized deploy behind Tailscale, phone PWA (triage list, chat, terminal peek) |
| v0.5.0 | Remote attach + push | 2026-06-12 | TUI raw-PTY attach to daemon sessions over WebSocket; Web Push on waiting/exited |
| v0.6.0–v0.6.1 | Archiving | 2026-06-13 | Idle-session auto-archive (30m) + manual archive everywhere; slim image; archive-bug fixes |

## Current

- **Next milestone** (in planning via `/gsd-new-milestone`): default the workspace
  and new/open paths to the repository root baude was launched from, and stop
  cross-repo `repository-<key>` worktree path collisions.

## Notes

- Pre-v0.7 work landed without GSD phases — the above is a record, not a set of
  GSD-verified milestones.

- Web Push (v0.5) has not yet been verified on a real phone (needs
  `tailscale serve` HTTPS + an installed PWA).
