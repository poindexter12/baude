---
phase: 02-hook-driven-status
verified: 2026-06-15T20:05:00Z
status: passed
uat_status: claude_driven_substantially_complete
uat_note: "UAT driven by Claude against the real binaries (see 02-UAT.md): items 2/3/4 passed live; item 1 mechanism-validated (event production + transports proven live; visual TUI overlay flip with a real interactive claude session is the only residual). 2 real bugs found and fixed during UAT — bauded missing the `hook` subcommand (d933edb) and daemon ingest requiring poll-resolved session_id (a7e49ab)."
score: 4/4 must-haves verified
overrides_applied: 0
human_verification:
  - test: "Live Claude CLI session fires hooks and flips state without the silence timer (Plan 02-03 Task 4 UAT)"
    expected: "On prompt submit the session shows working essentially instantly (faster than the ~2s silence window) with overlay state row = hook (not silence); when a tool runs the tool row updates; on Stop it flips to waiting with source still hook; schema-1 lines appear in /tmp/baude-events-<sid>.jsonl"
    why_human: "Requires a live `claude` CLI 2.1.177 invoking the seeded command hook — real lifecycle-event firing cannot be exercised by `cargo test`"
  - test: "User statusLine + user-defined hook survive the merge in a real spawned session"
    expected: "Pre-create a scratch .claude/settings.local.json with a user statusLine and one user hook, spawn a managed session there, inspect the merged file: user statusLine + user hook intact AND baude's four hooks (UserPromptSubmit/Stop/Notification/PostToolUse) appended, each command = absolute current_exe() path + ' hook'"
    why_human: "Requires a real TUI spawn writing to a scratch cwd; merge logic is unit-tested but live seed-then-inspect is the contract"
  - test: "Re-spawn / restart does not duplicate baude's hook entries (idempotent on the live path)"
    expected: "Re-spawn or restart the session and re-inspect settings.local.json — baude's entries are NOT duplicated"
    why_human: "Live re-spawn through the daemon restore() path; idempotency is unit-tested but live confirmation is part of the UAT"
  - test: "Daemon path: events arrive via POST /sessions/{id}/event and drive the same state"
    expected: "Start bauded, spawn a session via the daemon, confirm events arrive via the daemon-seeded $BAUDE_EVENT_URL and drive the same working/waiting state"
    why_human: "Requires a running daemon + live claude session POSTing real hook events"
---

# Phase 2: Hook-Driven Status Verification Report

**Phase Goal:** A managed session's working/waiting/done state is derived from Claude Code hook events (UserPromptSubmit, Stop, Notification, PostToolUse) rather than PTY-output silence, with the silence heuristic preserved only as a labeled fallback.
**Verified:** 2026-06-15T20:05:00Z
**Status:** human_needed
**Re-verification:** No — initial verification

## Goal Achievement

Every automatable must-have is VERIFIED against the real source (not SUMMARY claims). The only thing preventing `passed` is the live-`claude`-CLI UAT (Plan 02-03 Task 4), a `checkpoint:human-verify` that cannot run in an automated context and was correctly left pending rather than fabricated.

### Observable Truths (merged: 4 ROADMAP success criteria + plan must_haves)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | On spawn, baude seeds its hook set into the session's settings by merging into existing arrays; user-defined hooks/statusLine kept intact (HOOK-01) | ✓ VERIFIED | `hook::merge_hook_settings` (hook.rs:92-121) only `.entry().or_insert()` into `hooks.<event>` arrays, sentinel-guarded by command string; sibling keys untouched. `hook::seed_settings` (hook.rs:147-158) shared by TUI (`app.rs:432`, before `Pty::spawn` at :435) and daemon (`manager.rs:279`, before spawn). Test `merge_preserves_user_statusline_and_user_hook` asserts statusLine byte-intact + user PostToolUse hook survives + baude's 4 entries added; `merge_idempotent_applied_twice` + `seed_settings_writes_idempotent_merge` prove idempotency. |
| 2 | Session shows working on UserPromptSubmit, flips to waiting/done on Stop, reports last tool (PostToolUse), all without the silence timer firing (HOOK-02) | ✓ VERIFIED | `meta::read_event_tail` (meta.rs:356-417) maps UserPromptSubmit/PostToolUse→Busy, Stop/Notification→Waiting, records `last_tool`/`last_notification`; called in `poll()` after `read_bridge_file()` (meta.rs:146-147). `session::decide_live` (session.rs:164-192) returns `(Busy/Waiting, StateSource::Hook)` for a fresh hook ahead of SessionFile/Silence. `last_tool` surfaced on `SessionInfo` (manager.rs:606) and overlay tool row (ui.rs:884). |
| 3 | The same event model is consumed from a per-session file-tail and from POST /sessions/{id}/event in the daemon — one model, both transports (HOOK-03) | ✓ VERIFIED | `hook::event_path`/`append_event` (hook.rs:44-134) write `/tmp/baude-events-<sid>.jsonl`; `read_event_tail` tails the same path. Daemon `post_event` (api.rs:232-239) → `Manager::ingest_event` (manager.rs:374-382) resolves baude id → Claude sid → `append_event` onto the same `/tmp` file. `route_event` (hook.rs:171-188) + main.rs:65-72 pick POST vs append at runtime; on POST failure it falls back to file-append (WR-02) so both transports converge. |
| 4 | With hooks disabled/unavailable, the session still reaches correct waiting state via the silence fallback, and the fallback is labeled as such — no v0.6.1 regression (HOOK-02) | ✓ VERIFIED | `decide_live` (session.rs:179-191) keeps the v0.6.1 claude_status + silence branches byte-identical (prepend-only); returns `StateSource::Silence`. Test `no hook → Silence + same Busy/Waiting` + stale-hook fall-through tests present. Overlay renders the label `silence`/`session-file`/`hook` (ui.rs:878-883) so a regression is observable. |

**Score:** 4/4 truths verified (all automatable evidence present). UAT pending for live confirmation.

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `baude-core/src/hook.rs` | build_event, merge_hook_settings, event_path, append_event, baude_hook_command, seed_settings, route_event + tests | ✓ VERIFIED | 469 lines, 16 unit tests covering every behavior incl. WR-02 route_event fallback + WR-06 seed_settings |
| `baude/src/main.rs` | `baude hook` dispatch arm (stdin→build_event→route→exit 0) | ✓ VERIFIED | main.rs:44-74, bounded ureq agent (WR-04: 500ms connect / 2s read), `route_event`, unconditional `exit(0)` |
| `baude/src/app.rs` | seed before Pty::spawn in add_session | ✓ VERIFIED | `seed_settings(&cwd)` at app.rs:432, immediately before `Pty::spawn` (:435) |
| `baude-core/src/meta.rs` | read_event_tail + hook_status/last_tool/last_notification/offset_events + path-change reset | ✓ VERIFIED | read_event_tail meta.rs:356-417; WR-03 path-change reset at :366-371 resets offset + stale hook fields |
| `baude-core/src/session.rs` | StateSource enum, status_with_source, HOOK_FRESH_MS, decide_status | ✓ VERIFIED | enum + HOOK_FRESH_MS(5000) + decide_status/decide_live; WR-05 honest source on Exited (:142-157) |
| `bauded/src/manager.rs` | ingest_event + BAUDE_EVENT_URL injection + seed + SessionInfo state_source/last_tool | ✓ VERIFIED | ingest_event :374; `spawn_command` exports URL surviving `|| exec` (WR-01, :122-128); SessionInfo fields :52/:54 populated :599-606 |
| `bauded/src/api.rs` | POST /sessions/{id}/event route + post_event | ✓ VERIFIED | route registered api.rs:37; handler :232-239 → ingest_event → 204 / not_found→404; `Path<u64>` rejects non-numeric |
| `baude/src/ui.rs` | StateSource + last_tool overlay rows (local + remote) | ✓ VERIFIED | local rows ui.rs:878-885; remote rows :794/:823-824 |
| `baude/src/remote.rs` | RemoteInfo state_source/last_tool (serde default) | ✓ VERIFIED | deserializes daemon's new fields (scope extension within intent) |

### Key Link Verification

| From | To | Via | Status |
|------|-----|-----|--------|
| main.rs hook arm | `hook::build_event` / `route_event` | dispatch arm | ✓ WIRED (main.rs:50,70) |
| app.rs add_session | `hook::seed_settings` | before Pty::spawn | ✓ WIRED (app.rs:432→435) |
| meta.rs poll | read_event_tail | after read_bridge_file | ✓ WIRED (meta.rs:146-147) |
| session status_with_source | meta.hook_status | highest-precedence branch | ✓ WIRED (session.rs:205,173-177) |
| api.rs post_event | Manager::ingest_event | Path<u64>→204/404 | ✓ WIRED (api.rs:237) |
| manager.rs spawn | BAUDE_EVENT_URL | exported command prefix | ✓ WIRED (manager.rs:122-128,281) |
| ui.rs overlay | status_with_source / state_source | overlay row | ✓ WIRED (ui.rs:878,794) |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|--------|-------------------|--------|
| ui.rs overlay state row | StateSource | `status_with_source()` → `decide_status` off real `meta.hook_status`/`claude_status`/PTY output | Yes (computed from live meta, not hardcoded) | ✓ FLOWING |
| ui.rs overlay tool row | meta.last_tool | `read_event_tail` parsing real `/tmp` event lines | Yes (conditional row, omitted when absent) | ✓ FLOWING |
| SessionInfo.state_source/last_tool | source_str/meta.last_tool | session_info builder (manager.rs:599-606) | Yes (from status_with_source, not static) | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full workspace test suite green | `cargo test --workspace` | 48 + 35 = 83 passed; 0 failed | ✓ PASS |
| Hook event tail + merge tests exist & pass | (within above) | baude-core 48 incl. hook::/meta::/session:: | ✓ PASS |
| Daemon ingest + endpoint tests pass | (within above) | bauded 35 incl. manager::/api:: | ✓ PASS |

### Probe Execution

N/A — Rust workspace, no `scripts/*/tests/probe-*.sh` and no probe-based criteria. Step 7c skipped.

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| HOOK-01 | 02-01, 02-03 | Seed hook set merging into existing arrays, never clobbering user hooks/statusline | ✓ SATISFIED | merge_hook_settings + seed_settings (TUI + daemon), idempotent, statusLine-preserving (tests) |
| HOOK-02 | 02-02 | State derives from hooks; PTY-silence heuristic only as labeled fallback | ✓ SATISFIED | read_event_tail + StateSource precedence Hook>SessionFile>Silence, silence byte-identical + labeled |
| HOOK-03 | 02-01, 02-03 | Transport via per-session file-tail (TUI) and POST /sessions/{id}/event (daemon); one model | ✓ SATISFIED | append_event/event_path + post_event/ingest_event converge on the same /tmp file |

No orphaned requirements — all three HOOK IDs declared in plans and present in REQUIREMENTS.md (Phase 2).

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| — | — | None | — | No TBD/FIXME/XXX/TODO/HACK/placeholder in any of the 10 modified source files |

### Human Verification Required

The four items in frontmatter `human_verification` correspond to Plan 02-03 Task 4 (`checkpoint:human-verify`, `gate="blocking"`). They require a live `claude` CLI 2.1.177 invoking the seeded command hook — behavior that `cargo test` cannot exercise. The 02-03 SUMMARY correctly marks this pending and did NOT fabricate a result. The manual steps are reproduced in the SUMMARY's "Pending Human Verification" section and the plan's `<how-to-verify>`.

### Gaps Summary

No gaps. Every automatable must-have is implemented, substantive, wired, and data-flowing in the real source. The 6 code-review warnings (WR-01..WR-06) are all fixed in the codebase and verified here:
- WR-01: `spawn_command` uses `export BAUDE_EVENT_URL=...;` so the var survives the `|| exec claude` resume fallback (manager.rs:122-128, test `spawn_command_exports_event_url_on_both_paths`).
- WR-02: `route_event` falls back to file-append on POST failure (hook.rs:171-188, tests).
- WR-03: `read_event_tail` resets `offset_events` + stale hook fields on event-file path change (meta.rs:366-371).
- WR-04: bounded ureq agent caps the hook POST (main.rs:66-69).
- WR-05: Exited carries the honest underlying source, not a fabricated Hook (session.rs:142-157, test).
- WR-06: `seed_settings` hoisted into `baude_core::hook`, shared by both crates (hook.rs:147-158).

CI triad green: 83 tests pass; SUMMARY claim independently confirmed by running the suite once.

---

_Verified: 2026-06-15T20:05:00Z_
_Verifier: Claude (gsd-verifier)_
