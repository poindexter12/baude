---
phase: 02-hook-driven-status
plan: 03
subsystem: api
tags: [claude-code-hooks, axum, daemon, settings-merge, env-injection, state-source]

# Dependency graph
requires:
  - phase: 02-hook-driven-status (Plan 01)
    provides: "hook::merge_hook_settings, baude_hook_command, append_event, event_path"
  - phase: 02-hook-driven-status (Plan 02)
    provides: "session::status_with_source, StateSource{Hook,SessionFile,Silence}, meta.last_tool"
provides:
  - "POST /sessions/{id}/event route + post_event handler (204 / 404, never 500)"
  - "Manager::ingest_event — baude id -> Claude session_id -> append onto the shared /tmp consume path"
  - "Daemon spawn seeds .claude/settings.local.json idempotently + injects $BAUDE_EVENT_URL"
  - "SessionInfo.state_source + SessionInfo.last_tool surfaced over the REST API"
  - "StateSource + last-tool rows in the i info overlay (local Session + remote RemoteInfo paths)"
affects: [phase-03-act-timeline, phase-04-perm]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Env injection via spawn command-string prefix (BAUDE_EVENT_URL={url} {cmd}) — Pty::spawn has no env-map param"
    - "Dual-transport convergence: POST ingest and TUI file-tail both land on /tmp/baude-events-<sid>.jsonl"
    - "Idempotent daemon-side seed reusing the TUI merge, safe across the restore() re-spawn loop"
    - "Capture-but-render-lightly: state_source/last_tool exposed on SessionInfo, rendered as minimal overlay rows"

key-files:
  created: []
  modified:
    - bauded/src/manager.rs
    - bauded/src/api.rs
    - bauded/src/notify.rs
    - baude/src/ui.rs
    - baude/src/remote.rs

key-decisions:
  - "event_url uses the loopback DEFAULT_BIND (127.0.0.1:8642) directly; Manager stores no bind addr so a custom --bind port is NOT honored (documented known limitation, deferred)"
  - "id assigned before the spawn command is built so the injected $BAUDE_EVENT_URL carries the session id; next_id increments after a successful Pty::spawn (unchanged failure semantics)"
  - "Daemon seed mirrors baude/src/app.rs::seed_session_hooks byte-for-byte (best-effort, never aborts spawn); idempotency exercised by the restore() re-spawn path"
  - "post_event takes the raw body String (the line is already built by `baude hook`) and appends best-effort; unknown/unresolvable -> 404 via not_found, no 500 path"
  - "Added a #[cfg(test)] Manager::session_id_for_test helper so the cross-module api:: integration test can pin a Claude session_id without a live Claude writing sessions/<pid>.json"

patterns-established:
  - "Pattern: command-string env-prefix injection at daemon spawn (BAUDE_EVENT_URL) inheriting to claude + its hook child"
  - "Pattern: REST ingest endpoint converging onto the same /tmp file the poll loop tails (one event model, two transports)"

requirements-completed: [HOOK-01, HOOK-03]

# Metrics
duration: 22min
completed: 2026-06-15
---

# Phase 2 Plan 3: Daemon Event Endpoint + Overlay Surfacing Summary

**The daemon now closes the hook loop: it seeds `.claude/settings.local.json` and injects `$BAUDE_EVENT_URL` at session spawn, accepts `POST /sessions/{id}/event` and feeds those lines onto the same `/tmp` consume path the poll loop tails, exposes `state_source`/`last_tool` on `SessionInfo`, and renders them minimally in the `i` info overlay — with the live end-to-end UAT left pending human verification.**

## Performance

- **Duration:** ~22 min (autonomous tasks; UAT pending human verification)
- **Started:** 2026-06-15
- **Completed:** 2026-06-15 (autonomous code tasks)
- **Tasks:** 3 of 4 complete; Task 4 is a human-verify UAT awaiting a live Claude session
- **Files modified:** 5 (0 created, 5 modified)

## Accomplishments
- Wired the daemon transport half of HOOK-03: `Manager::ingest_event` resolves the baude `u64` id to the Claude `session_id` and appends posted event lines to `/tmp/baude-events-<sid>.jsonl`, converging the POST and file-tail transports onto one event model.
- Completed the daemon side of HOOK-01: `spawn` seeds the session cwd's `.claude/settings.local.json` idempotently (the same non-clobbering merge as the TUI) and prefixes the spawn command with `BAUDE_EVENT_URL=http://127.0.0.1:8642/sessions/{id}/event` so daemon-managed hooks POST back.
- Added the `POST /sessions/{id}/event` route + `post_event` handler (204 / 404, no 500 path; `Path<u64>` rejects non-numeric ids at the framework layer).
- Surfaced `state_source` + `last_tool` on `SessionInfo` and rendered them as minimal rows in both the local and remote `i` info overlays, with the silence fallback visibly labeled so a regression is observable.

## Task Commits

Each autonomous task was committed atomically:

1. **Task 1: seed hooks + inject BAUDE_EVENT_URL + ingest_event in manager.rs** - `fbc32b0` (feat)
2. **Task 2: POST /sessions/{id}/event route + post_event handler** - `3e09c61` (feat)
3. **Task 3: surface StateSource + last tool in the i info overlay** - `ad4cc8b` (feat)

**Task 4 (human-verify UAT):** not committed — no code changes; pending live verification (see "Pending Human Verification" below).

_Note: Tasks 1 and 2 carry `tdd="true"`. Following the established Plan 01/02 precedent for code that ships impl + `mod tests` in one file, each landed as a single `feat(02-03)` commit carrying full `<behavior>`-mapped coverage rather than split `test(...)` RED then `feat(...)` GREEN commits. See TDD Gate Compliance below._

## Files Created/Modified
- `bauded/src/manager.rs` (modified) - `source_str(StateSource)->&'static str` mapper; `event_url(id)` (loopback default bind); `seed_session_hooks(cwd)` (mirrors the TUI seed); spawn now assigns `id` first, seeds settings, and prefixes the command with `BAUDE_EVENT_URL=`; `ingest_event(id, body)` (resolve sid -> `hook::append_event`); `state_source`/`last_tool` added to `SessionInfo` and populated in `session_info` via `status_with_source` + `meta.last_tool`; `#[cfg(test)] session_id_for_test` helper; 4 new tests.
- `bauded/src/api.rs` (modified) - `.route("/sessions/{id}/event", post(post_event))`; `post_event` handler (raw body -> `ingest_event` -> 204; 404 on unknown/unresolvable); integration test asserting 204 + /tmp append for a known session and 404 for a bogus id.
- `bauded/src/notify.rs` (modified) - updated the `#[cfg(test)]` `SessionInfo` constructor for the two new fields (Rule 3 — blocking compile fix).
- `baude/src/ui.rs` (modified) - `StateSource` import; local overlay `state` row (hook/session-file/silence via `status_with_source`) + conditional `tool` row from `meta.last_tool`; remote overlay `state` row + conditional `tool` row.
- `baude/src/remote.rs` (modified) - `RemoteInfo` gained `state_source: Option<String>` + `last_tool: Option<String>` (`#[serde(default)]`) to deserialize the daemon's new fields.

## Decisions Made
- **Loopback default bind for `$BAUDE_EVENT_URL`** — `Manager` stores no bind addr, and the hook only needs same-host reachability, so `event_url` hardcodes `http://127.0.0.1:8642/...`. A custom `--bind` port is NOT honored (documented known limitation; honoring it would require threading the bind addr into `Manager`, deferred).
- **`id` assigned before command construction** so the injected URL carries the session id; `next_id` still increments only after a successful `Pty::spawn`, preserving the prior failure semantics.
- **Daemon seed is byte-for-byte the TUI seed** (best-effort, never aborts spawn). Idempotency is required by — and exercised by — the daemon `restore()` re-spawn path, not `restart()` (which inlines its own `Pty::spawn` and never re-seeds).
- **`post_event` takes the raw body String** (the event line is already built by `baude hook`) and appends best-effort; there is no 500 path.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Updated notify.rs test-helper SessionInfo constructor**
- **Found during:** Task 1
- **Issue:** Adding `state_source`/`last_tool` to `SessionInfo` broke the `#[cfg(test)] fn info(...)` constructor in `bauded/src/notify.rs` (missing-fields compile error), blocking `cargo test -p bauded`.
- **Fix:** Added the two new fields (`state_source: "silence"`, `last_tool: None`) to the test constructor.
- **Files modified:** `bauded/src/notify.rs`
- **Commit:** `fbc32b0` (committed with Task 1)

Otherwise the plan executed as written. No new dependencies (matches the threat register's `accept` disposition for installs: none this phase).

### Scope Extension (within plan intent)

The plan's Task 3 says to "match whichever data source the overlay already uses." The overlay has two paths: the local `Session` path (uses `status_with_source()` directly) and the remote `RemoteInfo` path (deserialized from the daemon's `GET /sessions`). To surface the state in both, `RemoteInfo` gained `state_source`/`last_tool` (`#[serde(default)]`, so it stays backward-compatible against an older daemon). This is the natural consumer of the `SessionInfo` fields added in Task 1 and stays within the capture-but-render-lightly remit.

## TDD Gate Compliance

Tasks 1 and 2 carry `tdd="true"`. Following the Plan 01/02 precedent the plan points to (impl + `#[cfg(test)] mod tests` shipped together in one file), each landed as a single `feat(02-03)` commit with full `<behavior>`-mapped coverage rather than split `test(...)` -> `feat(...)` commits. Every `<behavior>` bullet has a corresponding passing assertion:
- Task 1: `ingest_event_appends_to_resolved_tmp_file`, `ingest_event_errors_on_unknown_id_and_missing_session_id`, `event_url_is_loopback_default_bind`, `session_info_carries_state_source_and_last_tool`.
- Task 2: `post_event_appends_and_404s_unknown` (204 + /tmp append for a known session; 404 for a bogus id).

This divergence from strict RED->GREEN commit ordering is recorded here for traceability per the plan-level gate note; functional coverage is complete.

## Threat Model Notes
- **T-02-08** (unauthenticated `POST /sessions/{id}/event`): accepted per the project baseline — inherits the existing tailnet/loopback binding (default `127.0.0.1:8642`); no new exposure beyond the existing REST surface, no auth layer by design.
- **T-02-09** (body + `{id}` parsing DoS/tampering): mitigated — `Path<u64>` rejects non-numeric ids at the framework layer; the body is handled best-effort; unknown/unresolvable -> 404 via `not_found`; there is no 500/panic path. Verified by `post_event_appends_and_404s_unknown` (bogus id -> 404).
- **T-02-10** (`$BAUDE_EVENT_URL` injection): accepted — the URL is a hardcoded loopback endpoint, not a secret and not user input.
- **T-02-11** (daemon seed clobbering user config): mitigated — same sentinel-guarded `merge_hook_settings` as the TUI; sibling keys untouched; idempotent across the `restore()` re-spawn. (Live idempotency on a real re-spawn is part of the pending UAT step 7.)

No new security surface beyond the threat model. No threat flags.

## Pending Human Verification

**Task 4 is a `checkpoint:human-verify` UAT that requires a live `claude` CLI session firing real hooks — this cannot be performed in the automated executor and was NOT fabricated.** All automatable build/test verification passed (see Verification below). The live UAT remains pending and must be performed by a human:

1. `cargo build --workspace`, then run the TUI (`baude` / `cargo run -p baude`).
2. Pre-create a scratch `.claude/settings.local.json` with a user `statusLine` and one user-defined hook. Spawn a managed session there. Inspect the merged file: the user statusLine + user hook survive intact AND baude's four hooks (UserPromptSubmit/Stop/Notification/PostToolUse) were appended, each command being the absolute `current_exe()` path + ` hook`.
3. Submit a prompt. Confirm it shows "working" essentially instantly (faster than the ~2s silence window). Press `i` — the overlay `state` row should read "hook" (NOT "silence").
4. When Claude runs a tool, the overlay `tool` row updates; when the turn ends (Stop), the session flips to "waiting" with `state` still "hook".
5. Tail `/tmp/baude-events-<sid>.jsonl` and confirm schema-1 lines were appended.
6. (Daemon path) start `bauded`, spawn a session via the daemon, and confirm events arrive via `POST /sessions/{id}/event` (the daemon-seeded `$BAUDE_EVENT_URL`) and drive the same state.
7. Re-spawn / restart and re-inspect `settings.local.json`: baude's entries must NOT be duplicated (idempotent).

Note: if `claude --version` has advanced past 2.1.177, re-verify the hook stdin field names and update the pinned doc-comment in `hook.rs`.

## Verification (automated — all green)
- `cargo test -p bauded manager::` — 13/13 green (4 new: ingest append, ingest errors, event_url, SessionInfo fields).
- `cargo test -p bauded api::` — 7/7 green (new: `post_event_appends_and_404s_unknown`).
- `cargo build -p baude` — clean (overlay rows compile).
- `cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace` — full CI triad green (baude-core 42, bauded 34; clippy clean with -D warnings; fmt clean).

## Self-Check: PASSED

- FOUND: bauded/src/manager.rs (ingest_event, event_url, seed_session_hooks)
- FOUND: bauded/src/api.rs (post_event route)
- FOUND: baude/src/ui.rs (state/tool overlay rows)
- FOUND: baude/src/remote.rs (RemoteInfo state_source/last_tool)
- FOUND: .planning/phases/02-hook-driven-status/02-03-SUMMARY.md
- FOUND commit fbc32b0 (Task 1)
- FOUND commit 3e09c61 (Task 2)
- FOUND commit ad4cc8b (Task 3)

---
*Phase: 02-hook-driven-status*
*Completed (autonomous tasks): 2026-06-15 — UAT pending human verification*
