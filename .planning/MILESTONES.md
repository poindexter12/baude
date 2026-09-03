# Milestones

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

- **v0.7 — Native Claude integration** (in planning): replace inferred session
  state with first-party Claude Code data. Full plan: `docs/plans/tier-1-native-claude-integration.md`.

## Notes

- Pre-v0.7 work landed without GSD phases — the above is a record, not a set of
  GSD-verified milestones.

- Web Push (v0.5) has not yet been verified on a real phone (needs
  `tailscale serve` HTTPS + an installed PWA).
