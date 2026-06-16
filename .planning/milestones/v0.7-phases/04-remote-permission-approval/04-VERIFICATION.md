---
phase: 04-remote-permission-approval
verified: 2026-06-15T00:00:00Z
status: human_needed
uat_note: "Prompt-mode data path + deny-default posture FULLY validated live by Claude against a real daemon (bridge<->daemon round-trip: register pending -> GET -> allow/deny -> bridge verdict; fail-closed-no-daemon; no daemonize; 400/404 validation). 1 real bug fixed (43a940b: bridge POSTed text/plain -> daemon Json extractor 415 -> prompt mode silently denied every tool). Residual: real-claude 2.1.178 MCP wire shape (04-02 CONTRACT gate) + PWA card visual (04-04) — both genuinely live-only."
score: 18/18 must-haves verified (code-level); 2 live-only items routed to human
overrides_applied: 0
re_verification:
  previous_status: none
  previous_score: n/a
human_verification:
  - test: "Live --permission-prompt-tool wire-contract confirmation against claude 2.1.178 (04-02 Task 4 CONTRACT gate)"
    expected: "A real `claude -p --permission-prompt-tool mcp__baude__approve --mcp-config .mcp.json \"...\"` fires the seeded approve tool; the logged frames confirm the framing (Content-Length vs line), the tools/call request field names (tool_name / input / tool_use_id), and that the content[0].text JSON.stringify({behavior}) result is accepted to unblock the tool. If the shape diverges, only parse_frame / parse_tool_call / build_approve_result need correcting."
    why_human: "The exact 2.1.178 --permission-prompt-tool wire shape is MEDIUM-confidence (no complete official example, claude-code #1175). Only a live CLI invoking the seeded MCP tool confirms the framing + field names + accepted response envelope. No automated path exists; the bridge currently encodes the ASSUMED RESEARCH §C/§D contract, deliberately isolated to three functions so a divergence is cheap to correct."
  - test: "PWA approve/deny card + distinct-push end-to-end on a live prompt-mode session (04-04 Task 2 UAT)"
    expected: "Spawn BAUDE_PERMISSION_MODE=prompt, trigger a tool Claude must ask about. (a) A distinct push fires (\"<name> needs permission\"), separate from the generic waiting push. (b) The approve/deny card appears above the composer showing the tool + an esc()'d input summary. (c) Approve runs the tool and the card clears. (d) Deny denies only that single tool call (session survives, turn continues) and the card clears. (e) No response past BAUDE_PERMISSION_TIMEOUT_S denies on timeout (never auto-allows). Web Push phone-verification status noted (separate deferred milestone, not a blocker)."
    why_human: "The PWA is vanilla JS with no test runner / no build step (PERM-03 is manual by construction per 04-VALIDATION.md), and real Web Push needs a device/browser. node --check + grep confirm the card, fetch, both POST decisions, deny-uses-permission-route, and the sw.js cache bump statically; the user-visible behavior (render/clear, run-on-approve, deny-survives, timeout-denies) is the irreducible human UAT."
---

# Phase 4: Remote Permission Approval Verification Report

**Phase Goal:** From the phone, a pending tool-permission request can be approved or denied, gated behind an opt-in per-deploy mode, with its own distinct push.
**Verified:** 2026-06-15
**Status:** human_needed
**Re-verification:** No — initial verification

## Goal Achievement

This is the SECURITY-CRITICAL phase. Goal-backward verification confirms every code-level
must-have is implemented, substantive, wired, and data-flowing in the actual source. The
security invariants (default-skip, deny-on-timeout, deny-default decision validation,
fail-closed-on-no-daemon, no Phase-2 daemonize trap, XSS-escaped card) all HOLD against the
real lines, not the SUMMARY narrative. Two items short of full pass are the two live-only
human-verify gates the plans deliberately deferred (live `claude` wire contract + browser/device
PWA card), which is why status is `human_needed` rather than `passed`.

### Observable Truths (Roadmap Success Criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| SC1 | `BAUDE_PERMISSION_MODE` defaults to `skip`; `prompt` routes via `--permission-prompt-tool` | ✓ VERIFIED | `permission_flag_for` (permission.rs:67-80): `Some("prompt")` → prompt flag, every other case (None/skip/bogus/case-variant) → `--dangerously-skip-permissions`. No-double-add returns "". Both spawn sites apply it: manager.rs:375, app.rs:452. Tests at permission.rs:523-595 pin fail-safe. |
| SC2 | GET returns the pending request; POST {allow\|deny} resolves and unblocks/denies | ✓ VERIFIED | `get_permission` api.rs:283-350 (long-poll outside lock, returns PermissionView/null/404); `post_permission` api.rs:385-407 validates decision ∈ {allow,deny} → else 400, resolves via `resolve_pending` (manager.rs:559). Round-trip tests api.rs:856-994. |
| SC3 | PWA approve/deny card appears while pending and disappears once resolved | ⚠ CODE-VERIFIED / human-UAT | `perm-card` rendered in renderChat (app.js:685-696) gated on `s.waiting_reason==="permission" && state.pendingPermission`; buttons wired app.js:730-733; optimistic clear + refetch app.js:379-381. Visual render/clear is the live UAT (04-04 Task 2). |
| SC4 | A pending permission fires a distinct push describing the action, separate from generic waiting | ✓ VERIFIED | notify.rs:67-83: `waiting_reason=="permission"` fires a distinct "{name} needs permission" push via `notified_permission`, mutually exclusive with the generic waiting push, re-armed when the reason flips away. Tests notify.rs:208-244. |

### Must-Have Truths (per-plan frontmatter, all four plans)

| # | Truth (plan) | Status | Evidence |
|---|------|--------|----------|
| 1 | unset → `--dangerously-skip-permissions` (04-01) | ✓ VERIFIED | permission.rs:74-79, tested :526-529 |
| 2 | `=prompt` → prompt flag + seeded `.mcp.json` (04-01) | ✓ VERIFIED | permission.rs:75 + seeding manager.rs:384/app.rs:471 via `merge_mcp_config` |
| 3 | two flags never both appended (04-01) | ✓ VERIFIED | exactly one of {skip,prompt,""}; test permission.rs:579-595 |
| 4 | existing permission flag → neither appended (04-01) | ✓ VERIFIED | `already` guard permission.rs:68-73; test :553-576 |
| 5 | prompt-mode tool call invokes seeded tool, blocks until decision (04-02) | ⚠ CODE-VERIFIED / human-UAT | `run_permission_mcp` blocking loop main.rs:84-120; live invocation is 04-02 Task 4 |
| 6 | GET returns pending while pending, null/204 otherwise (04-02) | ✓ VERIFIED | api.rs:329-349; test :864-870 returns null |
| 7 | POST {allow\|deny} resolves and unblocks bridge (04-02) | ✓ VERIFIED | api.rs:393-406 + Notify wake manager.rs:578-580; test :1000 long-poll wakes |
| 8 | no decision before deadline → deny, never auto-allow (04-02) | ✓ VERIFIED | `decide_with_timeout` permission.rs:335-342 + bridge loop main.rs:104-119; fail-closed no-daemon main.rs:86-88 |
| 9 | both binaries handle `permission-mcp`, no second daemon (04-02) | ✓ VERIFIED | baude/main.rs:166, bauded/main.rs:158 — both dispatch before daemonize/TUI fall-through |
| 10 | unknown decision rejected (400), never allow (04-02) | ✓ VERIFIED | api.rs:395-400 (400) + defense-in-depth coercion manager.rs:567 |
| 11 | recent permission notification → waiting_reason=="permission" (04-03) | ✓ VERIFIED | `waiting_reason` permission.rs:502-508; populated manager.rs:839; test manager.rs:1290 |
| 12 | non-permission waiting → "input"; active → "none" (04-03) | ✓ VERIFIED | permission.rs:505-506; tests permission.rs:1088-1101 |
| 13 | pending fires ONE distinct push, separate from generic (04-03) | ✓ VERIFIED | notify.rs:67-83; test :208 |
| 14 | distinct push re-arms once permission resolves (04-03) | ✓ VERIFIED | notify.rs:67-70 removes id when reason flips; test notify.rs:228-244 |
| 15 | RemoteInfo deserializes waiting_reason backward-compat (04-03) | ✓ VERIFIED | remote.rs:44 `#[serde(default)] pub waiting_reason: Option<String>` |
| 16 | card appears while pending, shows tool + esc()'d input (04-04) | ⚠ CODE-VERIFIED / human-UAT | app.js:685-696, every dynamic string esc()'d (:689-690); render is live UAT |
| 17 | Approve POSTs allow / Deny POSTs deny to /permission (04-04) | ✓ VERIFIED | resolvePermission app.js:371-385 POSTs {decision}; deny uses permission route NOT kill |
| 18 | deny denies the single tool only — does NOT kill session (04-04) | ✓ VERIFIED | app.js:367-388 — deny() → resolvePermission("deny") → POST /permission, never interrupt/kill |

**Score:** 18/18 code-level must-haves verified. 3 of them (5, 16, and SC3's visual half) have a residual live-only confirmation routed to human UAT.

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `baude-core/src/permission.rs` | flag selector + JSON-RPC framing + approve-result + waiting_reason (pure) | ✓ VERIFIED | 1118 lines, ~50 tests; all behavior bullets pinned; security isolation documented :155-161 |
| `bauded/src/manager.rs` | spawn-flag + .mcp.json seed + pending/resolve + waiting_reason | ✓ VERIFIED | spawn :375/384, set/pending/resolve/decision :521-582, Notify :588, waiting_reason :839 |
| `bauded/src/api.rs` | GET + POST /permission + decision validation | ✓ VERIFIED | routes :41-42, handlers :283-423, 400/404/202 + long-poll-outside-lock |
| `bauded/src/main.rs` | byte-identical permission-mcp arm (no daemonize) | ✓ VERIFIED | :59-122 bridge + :158 arm before daemon boot; PERM-BUG application/json fix :97-100 |
| `bauded/src/notify.rs` | notified_permission distinct push + re-arm + test-ctor fix | ✓ VERIFIED | :22 set, :67-83 distinct/exclusive/re-arm, :136 `waiting_reason: None` ctor fix |
| `baude/src/main.rs` | permission-mcp arm (TUI binary) | ✓ VERIFIED | :73-122 bridge + :166 arm |
| `baude/src/app.rs` | TUI spawn flag + WR-01 warn | ✓ VERIFIED | flag :452, seed gated :471, WR-01 warn_prompt_mode_without_daemon :93 |
| `baude/src/remote.rs` | RemoteInfo.waiting_reason mirror | ✓ VERIFIED | :44 with `#[serde(default)]` |
| `bauded/web/app.js` | perm card + approve/deny POST + GET fetch | ✓ VERIFIED | card :685-696, fetch :333-346, resolve :371-388, wiring :730-733; node --check passes |
| `bauded/web/sw.js` | cache-version bump | ✓ VERIFIED | CACHE = "baude-v4" :4 |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|----|--------|---------|
| manager.rs spawn | permission::permission_flag | base_cmd build | ✓ WIRED | manager.rs:375 |
| app.rs spawn | permission::permission_flag | claude_cmd build | ✓ WIRED | app.rs:452 |
| baude/main.rs | baude_core::permission (parse/build/decide) | run_permission_mcp dispatch | ✓ WIRED | main.rs:73-120 |
| permission-mcp bridge | /sessions/{id}/permission | ureq POST(application/json) + long-poll GET | ✓ WIRED | main.rs:97-113 — content-type fix prevents 415 silent-deny |
| api.rs post_permission | Manager::resolve_pending | decision path | ✓ WIRED | api.rs:404 |
| api.rs get_permission | Manager::permission_notify | await outside lock | ✓ WIRED | api.rs:302-322 (lock dropped before await; WR-04 missed-wakeup closed) |
| manager.rs session_info | permission::waiting_reason | field population | ✓ WIRED | manager.rs:839 |
| notify.rs tick | SessionInfo.waiting_reason | permission branch | ✓ WIRED | notify.rs:67-83 |
| renderChat | state.pendingPermission | card between activity and composer | ✓ WIRED | app.js:685-696 |
| app.js approve/deny | /sessions/{id}/permission | POST {decision} | ✓ WIRED | app.js:375-378 |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|--------|--------------------|--------|
| perm-card (app.js) | state.pendingPermission | GET /sessions/{id}/permission → PermissionView from manager pending state | Yes (real daemon round-trip; not hardcoded) | ✓ FLOWING |
| distinct push (notify.rs) | s.waiting_reason | permission::waiting_reason(meta.last_notification, waiting) — hook-derived | Yes (last_notification captured by Notification hook, meta.rs:447) | ✓ FLOWING |
| bridge decision (main.rs) | decision | long-poll GET; resolved via POST /permission → resolve_pending | Yes (real pending/decision store, Notify-woken) | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Workspace tests GREEN | `cargo test --workspace` | 160 passed; 0 failed (per-crate: baude-core lib, bauded 58, baude 2, api, etc.) | ✓ PASS |
| PWA JS parses | `node --check bauded/web/app.js` | OK | ✓ PASS |
| Live MCP wire contract | (needs live claude 2.1.178) | — | ? SKIP → human |
| PWA card render/clear on device | (needs browser/device) | — | ? SKIP → human |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| PERM-01 | 04-01, 04-02 | per-deploy BAUDE_PERMISSION_MODE skip\|prompt (default skip) controlling --dangerously-skip-permissions vs --permission-prompt-tool | ✓ SATISFIED | permission_flag_for + both spawn sites + permission-mcp transport; default-skip fail-safe tested |
| PERM-02 | 04-02 | GET returns pending; POST {decision,scope?} resolves | ✓ SATISFIED | api.rs:283-423 GET/POST + manager set/resolve; 400/404/202; deny-on-timeout |
| PERM-03 | 04-04 | PWA approve/deny card while pending | ⚠ SATISFIED (code) / human-UAT | app.js card + wiring; visual render is manual UAT by design (no JS test runner) |
| PERM-04 | 04-03 | distinct push driven by Notification hook + waiting_reason on SessionInfo | ✓ SATISFIED | waiting_reason mapper + SessionInfo/RemoteInfo + notified_permission distinct push + re-arm |

All four declared requirement IDs (PERM-01..04) are accounted for across the plan frontmatter
and present in REQUIREMENTS.md (lines 35-38). No orphaned requirements for Phase 4.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| — | — | None | — | No TBD/FIXME/XXX/HACK/PLACEHOLDER debt markers in any modified file; no stub returns; deny-default holds everywhere |

Code review (04-REVIEW.md) found 0 critical, 4 warnings, 3 info. All 4 warnings are addressed
in code: WR-01 (warn_prompt_mode_without_daemon, app.rs:93), WR-02 (client timeout 8s > server
wait 5s, main.rs:70-79), WR-03 (scope documented as accepted-but-unenforced, api.rs:363-377),
WR-04 (register Notified before state re-read, api.rs:294-322). One real bug found+fixed during
drive-validation (43a940b): the bridge now POSTs application/json so the daemon's Json extractor
does not 415 and silently deny every tool (main.rs:93-100).

### Human Verification Required

#### 1. Live --permission-prompt-tool wire-contract confirmation (04-02 Task 4 CONTRACT gate)

**Test:** Build the workspace, temporarily make `run_permission_mcp` log raw stdin frames and
return a hardcoded allow, seed `.mcp.json` in a scratch dir (or run a prompt-mode spawn), then run
`claude -p --permission-prompt-tool mcp__baude__approve --mcp-config .mcp.json "create a file named hello.txt"`.
**Expected:** The seeded approve tool fires; the logged frames confirm framing (Content-Length vs
line-delimited), the tools/call request field names (tool_name / input / tool_use_id), and that the
`content[0].text = JSON.stringify({behavior})` result is accepted to unblock the tool. If anything
diverges, only `parse_frame` / `parse_tool_call` / `build_approve_result` in baude-core need
correcting, then revert the raw-log/hardcoded-allow hack to restore the real daemon round-trip.
**Why human:** The 2.1.178 wire shape is MEDIUM-confidence (no complete official example,
claude-code #1175); only a live CLI invocation confirms it. No automated path exists.

#### 2. PWA approve/deny card + distinct push end-to-end (04-04 Task 2 UAT)

**Test:** Run bauded, open the PWA, hard-refresh (sw.js bumped to baude-v4 should pull new app.js).
Spawn a session with `BAUDE_PERMISSION_MODE=prompt`, submit a prompt that triggers a tool Claude
must ask about. Open the chat.
**Expected:** (a) a distinct push fires ("<name> needs permission"), separate from the generic
waiting push; (b) the approve/deny card appears above the composer, shows the tool + an esc()'d
input summary (no broken layout on `< > & "`); (c) Approve runs the tool, card clears; (d) Deny
denies only that single tool call (session stays alive, turn continues), card clears; (e) no
response past BAUDE_PERMISSION_TIMEOUT_S → tool denied (never auto-allowed). Note Web Push
phone-verification status (separate deferred milestone, NOT a blocker for sign-off).
**Why human:** The PWA is vanilla JS with no test runner/build step (PERM-03 manual by design per
04-VALIDATION.md); real Web Push needs a device. Static checks (node --check, grep for card/fetch/
POST/deny-route/cache-bump) all pass; the user-visible behavior is the irreducible UAT.

### Gaps Summary

No code-level gaps. Every must-have is implemented, substantive, wired, and data-flowing in the
actual source; the security-critical invariants (default-skip, deny-on-timeout, fail-closed,
deny-default validation, no Phase-2 daemonize trap, XSS-escaped card, deny-≠-kill) all HOLD against
the real lines. 160 workspace tests pass; node --check passes; no debt markers; all four code-review
warnings addressed; one real bug found+fixed (application/json content-type).

The only items short of `passed` are the two live-only confirmations the plans deliberately deferred
to end-of-phase human UAT: the live `claude` 2.1.178 MCP wire contract (the one MEDIUM-confidence
unknown, isolated to three correctable functions) and the PWA card visual + distinct-push behavior
on a browser/device. Status is therefore `human_needed`, not `gaps_found`.

---

_Verified: 2026-06-15_
_Verifier: Claude (gsd-verifier)_
