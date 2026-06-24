# Phase 4: Remote Permission Approval - Research

**Researched:** 2026-06-15
**Domain:** Claude Code `--permission-prompt-tool` MCP contract; Rust daemon blocking-bridge + pending state; Web Push; vanilla-JS PWA
**Confidence:** MEDIUM (HIGH on codebase integration & no-new-deps; MEDIUM on the Claude Code wire contract — the precise request/response JSON is NOT in a complete official example, see flagged ambiguity)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Permission Mode & Spawn Wiring**
- A per-deploy **`BAUDE_PERMISSION_MODE = skip | prompt`** env var (matches the `BAUDE_CLAUDE_CMD`/`BAUDED_BIND` convention), **default `skip`**.
- `skip` → append **`--dangerously-skip-permissions`** to the base claude command (unless the base cmd already carries a permission flag — don't double-add); `prompt` → append **`--permission-prompt-tool <tool>`**.
- `prompt` routes permission checks to a **baude bridge** that Claude invokes — mirroring the `baude hook` bridge: it forwards the request to the daemon and returns the decision. **The exact `--permission-prompt-tool` contract (MCP tool name vs command, the request/response JSON shape) is RESEARCH-GATED** — research must pin Claude Code's actual contract before the planner commits the transport.
- Decision shape: **`allow | deny`** plus an optional **`scope`** passthrough (the tool input may carry scope); keep minimal — no rich once/session/always UI.

**Daemon Permission State & API**
- Pending state lives on the **daemon `Session`** as `pending_permission: Option<PendingPermission { request_id, tool, input, ts }>` (`Manager` owns set/resolve), since this is daemon-mediated.
- **`GET /sessions/{id}/permission`** returns the pending request (or 204/null); **`POST /sessions/{id}/permission { decision: allow|deny, scope? }`** resolves it.
- Unblock: the bridge POSTs the request, then **waits (bounded poll/long-poll) on the daemon until a decision is POSTed**, then returns it to Claude.
- **Deny on timeout** after a long phone-approval window (never auto-allow — the safe default). The exact window is configurable and research-tuned.

**waiting_reason & Distinct Push (PERM-04)**
- Add a **`waiting_reason` enum `{ permission, input, none }`** on `SessionInfo`, derived from `last_notification` (a recent `permission_prompt` → `permission`, else `input` when waiting, else `none`).
- A pending permission fires a **distinct push** via a separate **`notified_permission`** set in `Notifier`, with a title/body describing the action, **re-armed when the permission resolves**. Separate from the generic "waiting" push.
- Push payload stays **lean** (`sid` + a permission marker); the PWA fetches `GET /permission` for the tool/input details.
- Builds on the existing v0.5 Web Push path (additive). Phone-verification of Web Push is a separate manual step — flagged, NOT a blocker.

**PWA Approve/Deny Card**
- A card **above the composer** in the chat view appears while `waiting_reason === "permission"` AND a pending permission exists; shows the tool + an input summary with **Approve / Deny** buttons.
- PWA learns of a pending permission via **push/SSE (`waiting_reason=permission`) and on chat open**, then fetches `GET /permission` for details.
- On resolve: **`POST /permission` → optimistic card removal + refetch** (card disappears once resolved — SC3).
- **Deny denies the single tool call** (Claude continues with the tool denied) — it does NOT kill/interrupt the session.

### Claude's Discretion
- The exact `PendingPermission`/`request_id` shape, the bridge's poll cadence and timeout window, the `--permission-prompt-tool` wiring details, and the card's input-summary formatting — constrained by the research-pinned Claude Code contract and existing hook/notify/PWA conventions.

### Deferred Ideas (OUT OF SCOPE)
- Rich permission scopes (once/session/always allow-lists), permission history/audit log, and a TUI approve/deny surface — out of scope unless research/UAT surfaces a need.
- First real-phone Web Push verification — a separate manual milestone task, not part of this phase's code.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| PERM-01 | `BAUDE_PERMISSION_MODE = skip\|prompt` (default `skip`) controls `--dangerously-skip-permissions` vs `--permission-prompt-tool`; `skip` preserves today's unattended behaviour | Spawn-command wiring in `manager.rs::spawn_command` + `app.rs::claude_cmd`; flags confirmed mutually-exclusive (skip bypasses prompting). See "Claude Code Contract" §A and §E. |
| PERM-02 | `GET /sessions/{id}/permission` returns pending request; `POST` `{decision, scope?}` resolves it | Mirrors `interrupt`/`post_event` `Path<u64>` handlers; `Manager` owns pending state. See "Architecture Patterns" P2/P3. |
| PERM-03 | PWA chat view shows approve/deny card while pending | `renderChat()` + POST-action pattern (`sendMessage`/`interrupt`); `esc()` all strings. See "Architecture Patterns" P5. |
| PERM-04 | Distinct push driven by `Notification` hook + `waiting_reason` on `SessionInfo` | `last_notification` already captured in `meta.rs:447`; `Notifier` `notified_*` debounce pattern; existing `push::send`. See P4. |
</phase_requirements>

## Summary

This phase adds an **opt-in, phone-mediated permission gate** on Claude's critical path. The default stays `skip` (`--dangerously-skip-permissions`, today's unattended behavior); `prompt` mode routes each tool-permission decision through baude. The entire daemon/PWA/push half (PERM-02/03/04) reuses existing, well-established patterns in the codebase — the `Path<u64>` REST handlers, the `Manager`-owned session state, the `Notifier` debounce sets, the existing Web Push send path, and the vanilla-JS `renderChat()` card. **No new crates are required**; `ureq`, `serde_json`, `axum`, and `tokio` already cover the blocking POST, JSON, routing, and async-wait needs.

The single genuinely research-gated unknown is the **Claude Code `--permission-prompt-tool` wire contract**. The official CLI reference confirms the flag *value is an MCP tool name* and that MCP servers register via `--mcp-config`/`.mcp.json`. The request/response *JSON shape* (the tool receives `{tool_name, input, tool_use_id}` and returns an MCP `text` content whose text is `JSON.stringify({behavior: "allow"|"deny", updatedInput?, message?})`) is consistently reported across multiple secondary sources and matches the SDK `PermissionResult` type — but Anthropic has **not** published a complete, official end-to-end example (tracked gap: claude-code issue #1175). This means baude must expose a **minimal stdio JSON-RPC MCP server with one tool**, not a plain command. The contract details and the safest fallback are pinned in the dedicated section below.

**Primary recommendation:** In `prompt` mode, seed a one-tool stdio MCP server (`baude permission-mcp`, registered via a seeded `.mcp.json` + `--mcp-config`, exactly like Phase 2 seeded hooks into `settings.local.json`) and pass `--permission-prompt-tool mcp__baude__approve`. The MCP tool reads `{tool_name, input, tool_use_id}`, POSTs to the daemon, long-polls `GET /permission` until the human resolves it (or the timeout fires → **deny**), and returns the JSON-stringified `{behavior}` MCP result. Hand-roll the JSON-RPC over stdio (consistent with the no-new-deps pattern; see §C/§D for the minimal framing).

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Permission-mode flag selection | baude/bauded spawn (`manager.rs`/`app.rs`) | — | The spawn site is the only place the claude command string is built; the flag is a launch-time decision per deploy. |
| Permission-prompt transport (MCP server) | `baude permission-mcp` subcommand (CLI bridge) | baude-core (pure JSON-RPC framing) | Mirrors `baude hook`: the binary owns the network/stdio, baude-core owns the pure transforms (no HTTP dep in core). |
| Pending-permission state | daemon `Manager`/`Session` | — | Daemon-mediated; the bridge and PWA both reach it over REST. Locked decision. |
| Decision API | daemon `bauded/src/api.rs` | — | REST is the daemon's surface; inherits the tailnet/loopback bind. |
| `waiting_reason` derivation | `baude-core` meta (`last_notification`) → `SessionInfo` | daemon | Already captured upstream; this is a pure read of existing event-derived state. |
| Distinct push decision | daemon `Notifier` | daemon `push::send` | Notifier already owns the per-status debounce; push is the existing send path. |
| Approve/deny card | PWA (`web/app.js`) | — | Phone-first UX; vanilla JS, no build step. |

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `ureq` | 2.x (already pinned) | Blocking HTTP from the `permission-mcp` bridge to the daemon (POST request, long-poll GET) | Already the hook bridge's client; the permission bridge needs the SAME bounded-agent blocking-POST shape, just with a longer read timeout and a poll loop. `[VERIFIED: bauded/Cargo.toml]` |
| `serde_json` | (workspace) | Parse stdin JSON-RPC frames + tool input; build the MCP result + the daemon request/response | Used identically by `hook.rs`/`bridge.rs` via untyped `Value` accessors. `[VERIFIED: Cargo.toml workspace]` |
| `axum` | 0.8 (already pinned) | The two new REST routes (`GET`/`POST /sessions/{id}/permission`) | The existing router; `Path<u64>` + `Json<>` handlers are the proven pattern. `[VERIFIED: bauded/Cargo.toml]` |
| `tokio` | 1.x (already pinned) | Async wait on the daemon side for the POSTed decision (a `Notify`/watch channel or a poll under the `META_POLL_MS` cadence) | The daemon is already `#[tokio::main]`; the bridge BLOCKS but the daemon need not — see Pitfall 4. `[VERIFIED: bauded/Cargo.toml]` |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| — | — | — | **No new dependency is needed.** The MCP stdio server is hand-rolled JSON-RPC (line-delimited or `Content-Length`-framed) using `serde_json`, exactly as the hook bridge hand-handles stdin. |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Hand-rolled stdio JSON-RPC MCP server | `rmcp` (official Rust MCP SDK) or `mcp-sdk` crate | Adds a heavy async dependency + a learning surface for ONE tool with one method. The project NON-NEGOTIABLE is "prefer no new heavy deps; hand-roll JSON-RPC over stdio like the bridge already hand-handles stdin." Hand-rolling 1 tool's `initialize` + `tools/list` + `tools/call` is ~80 lines. **Recommend hand-roll.** |
| MCP stdio server | A plain command for `--permission-prompt-tool` | **Not supported.** The flag value is an *MCP tool name* (`mcp__server__tool`), not a shell command — see §A. A plain command is rejected. |
| Long-poll GET | SSE permission channel | The bridge is a short-lived headless process that needs ONE blocking answer; a bounded long-poll/poll loop is simpler than an SSE consumer and matches the bridge's request/response shape. SSE is for the PWA's live card, not the bridge. |

**Installation:**
```bash
# None. No new crates. All transport reuses ureq/serde_json/axum/tokio already in the workspace.
```

**Version verification:** Installed CLI is **`claude --version` → 2.1.178** `[VERIFIED: claude --version]` (the repo pins ~2.1.177 in `hook.rs`/`bridge.rs`; bump the pinned doc-comment to 2.1.178 and re-verify hook/contract field names against this version). No registry packages added this phase.

## Package Legitimacy Audit

> No external packages are installed this phase. All transport reuses crates already vendored in the workspace (`ureq 2`, `serde_json`, `axum 0.8`, `tokio 1`, all `[VERIFIED: Cargo.toml]`).

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| (none) | — | — | — | — | — | No installs this phase |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

## Claude Code `--permission-prompt-tool` Contract

> **This is the #1 research target and the one research-gated decision.** Installed CLI: **2.1.178**.

### §A — What the flag accepts (HIGH confidence)

`--permission-prompt-tool` takes an **MCP tool name**, not a shell command. The official CLI reference describes it as: *"Specify an MCP tool to handle permission prompts in non-interactive mode"* with example `claude -p --permission-prompt-tool mcp_auth_tool "query"`. `[CITED: code.claude.com/docs/en/cli-reference]`

The tool name follows the standard MCP form **`mcp__<server>__<tool>`** (the `mcp__<server>__<tool>` naming is confirmed in the permissions/MCP docs). `[CITED: code.claude.com/docs/en/agent-sdk/permissions]` So baude registers an MCP server named e.g. `baude` exposing a tool `approve`, and passes `--permission-prompt-tool mcp__baude__approve`.

**MCP server registration** is via `--mcp-config <file-or-json>` (and optionally `--strict-mcp-config` to ignore all other MCP config). `[CITED: code.claude.com/docs/en/cli-reference]` This is the direct analog of Phase 2's settings-seeding: seed a `.mcp.json` into the session cwd (or pass `--mcp-config`) describing the stdio server command (`current_exe() + " permission-mcp"`), exactly as `seed_settings` seeds the hook command as `current_exe() + " hook"`.

### §B — When the tool is invoked (HIGH confidence)

Claude evaluates static rules first (`settings.json` allow/deny, `--allowedTools`/`--disallowedTools`); **only unresolved calls fall through to the permission-prompt tool**. `[CITED: code.claude.com/docs/en/agent-sdk/permissions]` `--dangerously-skip-permissions` / `bypassPermissions` short-circuits *before* the prompt, so the two modes are effectively mutually exclusive (see §E).

### §C — Request payload the tool RECEIVES (MEDIUM — corroborated, not in a complete official example)

The permission tool is called like any MCP tool: its arguments contain the tool-call under review. Reported fields:

```jsonc
// arguments passed to the mcp__baude__approve tool call
{
  "tool_name": "Bash",                 // string — the tool Claude wants to run
  "input": { "command": "rm -rf build/" }, // object — that tool's input/parameters
  "tool_use_id": "toolu_01..."         // string — correlation id (optional/availability varies)
}
```
`[ASSUMED — field names tool_name/input/tool_use_id reported by multiple secondary sources (vibesparking playbook, LobeHub) and consistent with the SDK; NOT confirmed in an official end-to-end example]`

Because baude reads everything via untyped `serde_json::Value` accessors (the established `hook.rs`/`bridge.rs` posture), **read both `input` and a fallback `parameters`/`tool_input` key** and tolerate a missing `tool_use_id`. This makes baude robust to the exact field name regardless of which variant 2.1.178 emits.

### §D — Response the tool must RETURN (MEDIUM — corroborated, not in a complete official example)

The tool returns a normal MCP `tools/call` result whose **`content[0]` is `{type: "text", text: <JSON string>}`**, where the text is a `JSON.stringify` of a `PermissionResult`:

```jsonc
// the MCP tools/call result the baude approve tool returns
{
  "content": [
    {
      "type": "text",
      "text": "{\"behavior\":\"allow\",\"updatedInput\":{\"command\":\"rm -rf build/\"}}"
      //  or:  "{\"behavior\":\"deny\",\"message\":\"denied from phone\"}"
    }
  ]
}
```

The inner `PermissionResult` object is `{ behavior: "allow" | "deny", updatedInput?: object, message?: string }`. `behavior` is required; `updatedInput` echoes (or modifies) the approved input on allow; `message` is an optional human reason (used on deny). The `PermissionResult` type (`behavior`/`message`/`updatedInput`) is confirmed from the SDK permissions docs; the **MCP `text`-content wrapping with a JSON-stringified body** is reported by the secondary sources. `[CITED: code.claude.com/docs/en/agent-sdk/permissions for PermissionResult]` `[ASSUMED — the text/JSON.stringify wrapping is from secondary sources, not an official complete example]`

**Safest implementation rule:** On allow, return `{"behavior":"allow","updatedInput":<the input unchanged>}` (echo `input` back verbatim so a CLI that requires `updatedInput` on allow is satisfied). On deny, return `{"behavior":"deny","message":"denied"}`.

### §E — Interaction with `--dangerously-skip-permissions` (HIGH confidence)

`bypassPermissions` / `--dangerously-skip-permissions` **auto-approves everything before the prompt tool is consulted**, so the prompt is never reached. `[CITED: code.claude.com/docs/en/agent-sdk/permissions]` Therefore the mode switch is strictly one-or-the-other: `skip` appends `--dangerously-skip-permissions`; `prompt` appends `--permission-prompt-tool mcp__baude__approve` (plus the seeded `--mcp-config`). **Never append both.** Honor the "don't double-add if the base cmd already carries a permission flag" locked decision by scanning `BAUDE_CLAUDE_CMD` for an existing `--dangerously-skip-permissions`/`--permission-prompt-tool`/`--permission-mode` before appending.

### §F — Ambiguity statement + fallback (REQUIRED honesty)

**Ambiguity:** Anthropic has not published a complete, official, end-to-end `--permission-prompt-tool` MCP example; this is an explicit open gap (claude-code issue #1175, still asking for a minimal working example). The flag's *value type* (MCP tool name) and *registration* (`--mcp-config`) are official; the exact *request field names* and the *MCP text/JSON.stringify response wrapping* are corroborated across independent secondary sources and the SDK `PermissionResult` type, but are not officially specified end-to-end.

**Safest interpretation (what to build):** a stdio JSON-RPC MCP server exposing one tool `approve`; read `tool_name` + `input` (with `parameters`/`tool_input` fallbacks) via untyped `Value`; return MCP `content:[{type:"text", text: JSON.stringify({behavior, updatedInput, message})}]`. Tolerate unknown/extra fields; never panic on an odd payload (the established core posture).

**Fallback if 2.1.178 diverges (planner MUST gate behind a `checkpoint:human-verify`):** Before wiring the mode into the default spawn path, add a human-verify UAT that runs `claude -p --permission-prompt-tool mcp__baude__approve --mcp-config <seeded> "create a file"` against a baude `permission-mcp` that **logs the raw stdin JSON-RPC frames** and returns a hardcoded `allow`. Inspect the logged `tools/call` params to confirm the exact field names and the accepted result shape for THIS CLI version, then finalize. If the CLI rejects the `text`/JSON.stringify result, try returning the `PermissionResult` object directly as the tool result (some versions accept structured content). This UAT de-risks the only MEDIUM-confidence claim before `prompt` mode ships.

### §G — Minimal stdio MCP server shape (hand-rolled, no new deps)

baude must answer three JSON-RPC methods over stdio (line-delimited or `Content-Length` framed — Claude's MCP stdio transport uses `Content-Length` headers like LSP; handle both by reading a frame, parsing the body as `Value`):

1. `initialize` → reply with `protocolVersion`, `capabilities: { tools: {} }`, `serverInfo`.
2. `tools/list` → reply with one tool: `{ name: "approve", description, inputSchema: { type:"object", properties:{ tool_name:{}, input:{}, tool_use_id:{} } } }`.
3. `tools/call` (name == "approve") → extract `tool_name`/`input`, POST to the daemon, long-poll for the decision, return the `content` result above.

Keep the pure framing/transform (`parse a frame`, `build the approve result`) in `baude-core` (testable, no HTTP), and the stdin loop + `ureq` POST/poll in the binary — exactly the `dispatch_hook` split. **This is the one place a baude subcommand BLOCKS on Claude's critical path with a long timeout**, unlike the always-exit-0 hook; document that contrast in the subcommand doc-comment.

## Architecture Patterns

### System Architecture Diagram

```
                           BAUDE_PERMISSION_MODE
                          ┌──── skip ──────────────► claude … --dangerously-skip-permissions
 daemon spawn site ───────┤                          (prompt tool NEVER reached — §E)
 (manager.rs/app.rs)      └──── prompt ────────────► claude … --permission-prompt-tool mcp__baude__approve
                                                              --mcp-config <seeded .mcp.json>
                                                                       │
   Claude wants to run a tool (e.g. Bash rm -rf) ─── unresolved ──────▼
                                                     ┌─────────────────────────────┐
                                                     │ baude permission-mcp (stdio)│  ← BLOCKS (bounded)
                                                     │  JSON-RPC tools/call approve │
                                                     └──────────────┬──────────────┘
                                                          POST /sessions/{id}/permission
                                                          {request_id, tool, input, ts}
                                                                     ▼
                                            ┌──────────────────────────────────────────┐
                                            │ daemon Manager: Session.pending_permission │
                                            │  set on POST-from-bridge; long-poll wait   │
                                            └───────┬───────────────────────────┬────────┘
                          GET /…/permission (PWA)   │                           │  Notifier.tick():
                          ◄─── pending request ─────┘                           │  waiting_reason==permission
                                                                                ▼  → notified_permission → push::send
   PWA chat: approve/deny card above composer ◄──── push (lean: sid+marker) ────┘  (distinct from waiting push)
            │ POST /sessions/{id}/permission {decision:allow|deny, scope?}
            ▼
   Manager resolves pending_permission ──► wakes the bridge's long-poll ──► bridge returns
        {behavior:allow|deny} to Claude  ──► Claude runs / skips the tool, continues turn
        (timeout with no decision ──► resolve as DENY, never auto-allow)
```

### Recommended Project Structure (additive — no new files required)

```
baude-core/src/
└── permission.rs   # NEW (optional): pure JSON-RPC frame parse + MCP approve-result build
                    #   + PendingPermission/Decision structs (no HTTP) — testable like hook.rs
baude/src/main.rs    # add `permission-mcp` subcommand arm (mirrors `hook` arm)
bauded/src/main.rs   # add byte-identical `permission-mcp` arm (current_exe seeds bauded)
bauded/src/manager.rs# Session.pending_permission + set/resolve + waiting_reason on SessionInfo
                    #   + permission flag selection in spawn_command/spawn
bauded/src/api.rs    # GET/POST /sessions/{id}/permission handlers + routes
bauded/src/notify.rs # notified_permission set + distinct Notification in tick()
bauded/web/app.js    # approve/deny card in renderChat() + api(.../permission) POST
```

### Pattern 1: Mode-gated spawn flag selection (PERM-01)
**What:** Read `BAUDE_PERMISSION_MODE` (env, default `skip`) once at spawn; append exactly one permission flag to the base claude command, skipping if one is already present.
**When to use:** In `manager.rs::spawn_command` (daemon) AND `baude/src/app.rs::claude_cmd` (TUI) — both build the command string. Note `spawn_command` already wraps with `export …; exec` for resume; append the flag to the inner `base_cmd`.
**Example:**
```rust
// Source: pattern from bauded/src/manager.rs:126-133 spawn_command + default_claude_cmd
fn permission_flag(base_cmd: &str) -> &'static str {
    // Don't double-add if the operator already set a permission flag (locked decision).
    let already = base_cmd.contains("--dangerously-skip-permissions")
        || base_cmd.contains("--permission-prompt-tool")
        || base_cmd.contains("--permission-mode");
    if already { return ""; }
    match std::env::var("BAUDE_PERMISSION_MODE").as_deref() {
        Ok("prompt") => " --permission-prompt-tool mcp__baude__approve",
        _ => " --dangerously-skip-permissions", // default skip preserves today's unattended behavior
    }
}
```
**Note:** in `prompt` mode also seed `.mcp.json` (or add `--mcp-config <path>`) describing the stdio server, alongside the existing `seed_settings` call.

### Pattern 2: Manager-owned pending state + resolve (PERM-02)
**What:** `Session.pending_permission: Option<PendingPermission>`; `Manager::set_pending`/`resolve_pending` under the lock; the bridge waits for resolve.
**When to use:** Mirror how `interrupt(id)`/`ingest_event(id)` route `Err → 404` via `self.session(id)?`.
**Example:**
```rust
// Source: pattern from bauded/src/manager.rs ingest_event + session(id)? (404 routing)
pub struct PendingPermission {
    pub request_id: String, // bridge-generated (uuid-ish; or daemon-assigned counter)
    pub tool: String,
    pub input: serde_json::Value,
    pub ts: u64,            // for the timeout deadline
}
// set_pending(id, p) -> stores Some(p); resolve_pending(id, decision) -> clears + signals waiters.
```
**Wake mechanism:** prefer a `tokio::sync::Notify` or a `watch`/oneshot per request so `GET`-long-poll and the bridge wait wake immediately on POST; a simple poll under `META_POLL_MS` is an acceptable fallback (see Pitfall 4).

### Pattern 3: Path<u64> GET/POST permission routes (PERM-02)
**What:** Two handlers exactly like `interrupt`(POST) and `get_session`(GET); `Path<u64>` rejects non-numeric ids at the framework layer (no 500 path).
**Example:**
```rust
// Source: bauded/src/api.rs interrupt (213) + post_event (256) + get_messages (GET) patterns
.route("/sessions/{id}/permission", get(get_permission).post(post_permission))

async fn get_permission(State(s): State<Shared>, Path(id): Path<u64>)
    -> Result<Json<Option<PendingView>>, ApiError> {
    Ok(Json(lock(&s).pending(id).map_err(not_found)?)) // None -> null/204; tolerate no-pending
}
#[derive(Deserialize)]
struct Decision { decision: String /* allow|deny */, scope: Option<String> }
async fn post_permission(State(s): State<Shared>, Path(id): Path<u64>, Json(d): Json<Decision>)
    -> Result<StatusCode, ApiError> {
    lock(&s).resolve_pending(id, &d.decision, d.scope).map_err(not_found)?;
    Ok(StatusCode::ACCEPTED)
}
```
**Security clamp:** validate `decision` is exactly `allow`|`deny` (reject anything else → 400, never treat unknown as allow — V5 + deny-default).

### Pattern 4: Distinct push via a separate notified set (PERM-04)
**What:** Add `notified_permission: HashSet<u64>` to `Notifier`; when a session is `waiting` AND `waiting_reason == "permission"` AND not yet notified, push a distinct Notification; re-arm (remove from the set) when the permission resolves / a new turn starts.
**When to use:** In `Notifier::tick`, alongside the existing `notified_waiting`/`notified_exited` branches; the send path (`push::send` + `Notification::to_json`) is unchanged. Keep the payload lean (`sid` + a `kind:"permission"` marker); the PWA fetches `GET /permission` for detail.
**Example:**
```rust
// Source: bauded/src/notify.rs:42-94 tick + Notification::to_json (lean payload at 29-39)
// In the "waiting" arm, branch on s.waiting_reason:
if s.waiting_reason.as_deref() == Some("permission") {
    if self.notified_permission.insert(s.id) {
        out.push(Notification { title: format!("{} needs permission", s.name),
                                body: permission_summary(s), sid: s.id });
    }
} else { /* existing generic waiting debounce */ }
// re-arm on resolve: remove s.id from notified_permission when waiting_reason flips away.
```

### Pattern 5: Approve/deny card above the composer (PERM-03)
**What:** In `renderChat()`, render a card between `activityStrip` and the composer `<form>` when `state.session(sid).waiting_reason === "permission"` and a fetched pending exists; Approve/Deny buttons POST `/permission`, optimistically remove the card, then refetch.
**When to use:** Mirror `sendMessage`/`interrupt` POST-action shape (`api(url,{method:"POST",body})`, `toast` on error); `esc()` ALL dynamic strings (tool name, input summary) — XSS.
**Example:**
```js
// Source: bauded/web/app.js renderChat() (561) + sendMessage/interrupt POST pattern (293/308)
const perm = state.pendingPermission; // fetched from GET /permission on open/push
const permCard = (s && s.waiting_reason === "permission" && perm) ? `
  <div class="perm-card">
    <div class="perm-tool">${esc(perm.tool)}</div>
    <div class="perm-input">${esc(permSummary(perm.input))}</div>
    <div class="perm-actions">
      <button class="deny" id="permdeny">Deny</button>
      <button class="allow" id="permallow">Approve</button>
    </div>
  </div>` : "";
// insert permCard immediately before the <form id="composer">; wire onclick to
// api(`/sessions/${sid}/permission`,{method:"POST",body:JSON.stringify({decision})})
// then state.pendingPermission = null; render(); refetch.
```

### Anti-Patterns to Avoid
- **Treating `--permission-prompt-tool` as a plain command.** It is an MCP tool name; a bare command silently never fires the prompt (§A).
- **Appending both permission flags.** They're mutually exclusive; `skip` wins before the prompt (§E) — wiring both is a confusing no-op for `prompt`.
- **Auto-allowing on timeout or on a malformed decision.** Always deny-default; an unknown `decision` value is a 400, a timeout is a deny.
- **Blocking the daemon's tokio runtime while waiting for the human.** The *bridge process* blocks; the daemon must NOT hold the manager lock or a runtime thread across the (long) approval window (Pitfall 4).
- **Making `prompt` the default.** Explicit PROJECT non-negotiable — `skip` stays the unattended default.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| MCP protocol *framing* edge cases (batch, notifications, ids) | A full JSON-RPC engine | Minimal hand-rolled handler for `initialize`/`tools/list`/`tools/call` only, untyped `Value` | You only need 3 methods + 1 tool; a full engine is over-build, but the 3-method handler IS the right "hand-roll" per the project rule. |
| HTTP client / TLS / pooling for the bridge | A new http crate | The existing `ureq` agent (bounded, as in `run_hook`) | Already vendored, already the bridge's client; just lengthen the read timeout + add a poll loop. |
| Cross-process wakeup of the long-poll | A polling busy-loop with no bound | `tokio::sync::Notify`/`watch` (already have tokio) OR a bounded poll at `META_POLL_MS` | Avoids both a tight spin and an unbounded wait; deny-on-deadline caps it. |
| Web Push encryption/VAPID | Anything new | The existing `push::send`/`push.rs` path (additive) | v0.5 already ships it; this phase only adds a new Notification *trigger*, not a new protocol. |
| XSS-safe rendering | Manual escaping ad hoc | The existing `esc()` helper on every dynamic string | Established PWA convention; the card injects a tool name + arbitrary tool input. |

**Key insight:** Every piece except the Claude-Code wire contract is a *re-application* of an existing baude pattern. The phase's risk is concentrated entirely in §C/§D of the contract — gate it with the §F UAT, then the rest is mechanical.

## Runtime State Inventory

> Not a rename/refactor/migration phase — this is additive feature work. State inventory is N/A; no stored data, OS-registered state, or secrets are renamed. The only persisted artifact touched is the per-session seeded config: in `prompt` mode a `.mcp.json` is seeded into the session cwd (additive, idempotent, non-clobbering — mirror `seed_settings`), and it must be re-seeded on `restore()` re-spawn like the hook settings. **Verified by:** review of `manager.rs::spawn`/`restore` and `hook::seed_settings`.

## Common Pitfalls

### Pitfall 1: Plain-command vs MCP-tool confusion
**What goes wrong:** Wiring `--permission-prompt-tool "baude permission"` (a command) — Claude never invokes it; tool calls hang or auto-resolve per other rules.
**Why it happens:** The flag *looks* like the `statusLine`/hook command pattern, but it's an MCP tool name.
**How to avoid:** Pass `mcp__baude__approve` and register the stdio server via `--mcp-config`/`.mcp.json`.
**Warning signs:** No JSON-RPC frames ever arrive on the `permission-mcp` stdin during the §F UAT.

### Pitfall 2: Daemon-binary subcommand fall-through (the Phase 2 `bauded hook` trap)
**What goes wrong:** `current_exe()` seeds the MCP command as `bauded permission-mcp` for daemon sessions; if `bauded` lacks the `permission-mcp` arm, it boots a *second daemon* instead of speaking MCP — exactly the documented `bauded hook` failure mode.
**Why it happens:** Both binaries seed `current_exe()`; both MUST handle the subcommand identically.
**How to avoid:** Add a byte-identical `permission-mcp` arm to BOTH `baude/src/main.rs` and `bauded/src/main.rs`; keep shared logic in `baude-core` (the `dispatch_hook` precedent).
**Warning signs:** A spurious extra `bauded listening on …` line, or the port already bound.

### Pitfall 3: Timeout that auto-allows / never fires
**What goes wrong:** A bounded wait that returns `allow` on expiry, or an unbounded wait that hangs Claude forever.
**Why it happens:** Reusing the hook's "best-effort exit 0" mindset where the *answer matters*.
**How to avoid:** On deadline (configurable, default a generous phone window — research-tuned, e.g. a few minutes), resolve as **deny** and return `{behavior:"deny"}`. Make the deadline an env knob (e.g. `BAUDE_PERMISSION_TIMEOUT_S`).
**Warning signs:** A tool runs after the phone never answered (auto-allow regression) — security-critical; cover with a test that asserts timeout → deny.

### Pitfall 4: Holding the manager lock / a runtime thread across the human wait
**What goes wrong:** The daemon stalls all other sessions while one waits minutes for phone approval.
**Why it happens:** Naively waiting under `lock(&state)` or in a blocking handler thread.
**How to avoid:** The *bridge process* blocks (it's a short-lived child, fine). On the daemon, set pending state under the lock, then **release the lock** and await a `Notify`/`watch` (or poll at `META_POLL_MS`) OUTSIDE the lock — the same "decide under the lock, send outside it" rule the notifier loop already follows (`bauded/src/main.rs:93-126`).
**Warning signs:** Other sessions' `/sessions` polls or chat stall while one permission is pending.

### Pitfall 5: MCP stdio framing mismatch
**What goes wrong:** Reading line-delimited JSON when Claude sends `Content-Length`-framed messages (LSP-style), or vice-versa — frames never parse.
**Why it happens:** MCP stdio transport convention isn't pinned in the secondary sources.
**How to avoid:** In the §F UAT, log raw stdin bytes; support reading a `Content-Length:` header + body, falling back to line-delimited. Parse the body untyped and tolerate notifications (no `id`).
**Warning signs:** `initialize` never gets a reply; Claude reports the MCP server failed to start.

## Code Examples

### Bounded blocking POST-then-poll from the bridge (deny on timeout)
```rust
// Source: pattern derived from baude/src/main.rs:40-53 run_hook (bounded ureq agent)
// The permission bridge mirrors run_hook's agent but BLOCKS for an answer with a long
// read timeout and polls GET until resolved; on deadline it returns DENY (never allow).
let agent = ureq::AgentBuilder::new()
    .timeout_connect(std::time::Duration::from_millis(500))
    .timeout(std::time::Duration::from_secs(5)) // per-request; the WAIT is the poll loop below
    .build();
// 1) POST the pending request to the daemon (request_id, tool, input).
let _ = agent.post(&post_url).send_string(&req_json);
// 2) Poll GET /permission/<request_id> (or /sessions/{id}/permission) until resolved or deadline.
let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_s);
let decision = loop {
    if std::time::Instant::now() >= deadline { break "deny"; } // deny-on-timeout (security)
    if let Ok(resp) = agent.get(&poll_url).call() {
        if let Ok(v) = resp.into_json::<serde_json::Value>() {
            if let Some(d) = v["decision"].as_str() { break if d == "allow" { "allow" } else { "deny" }; }
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(500));
};
// 3) Emit the MCP tools/call result with behavior = decision (echo input on allow).
```

### waiting_reason derivation on SessionInfo (PERM-04)
```rust
// Source: baude-core/src/meta.rs:447 last_notification already captured on Notification events
// Map the most recent notification_type to the enum; "permission_prompt" -> permission.
pub fn waiting_reason(last_notification: Option<&(String, u64)>, waiting: bool) -> &'static str {
    match last_notification {
        Some((nt, _)) if nt.contains("permission") => "permission",
        _ if waiting => "input",
        _ => "none",
    }
}
// Surface as SessionInfo.waiting_reason: Option<String> (and RemoteInfo with #[serde(default)]).
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Permission prompts only interactive in the TUI | `--permission-prompt-tool` delegates to an MCP tool for non-interactive/headless approval | Present in CLI ≥ mid-2025 | Enables baude's phone-mediated approval at all. |
| Hand-roll MCP from scratch | Official `rmcp` Rust SDK exists | 2025 | Available, but rejected here for the no-new-deps rule (1 tool). |
| `canUseTool` SDK callback (in-process) | `--permission-prompt-tool` (out-of-process MCP) is the CLI/headless equivalent | — | baude uses the CLI/MCP path; it is the daemon-spawns-`claude`-as-a-child architecture, not the SDK. |

**Deprecated/outdated:**
- Pinned CLI in `hook.rs`/`bridge.rs` comments says 2.1.177; installed is 2.1.178 — bump the doc-comment and re-verify field names.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Request fields are `tool_name`, `input`, `tool_use_id` | §C | Wrong field name → baude reads null tool/input; mitigated by untyped reads + `parameters`/`tool_input` fallbacks + §F UAT. |
| A2 | Response is MCP `content:[{type:"text", text: JSON.stringify({behavior,...})}]` | §D | Wrong wrapping → Claude rejects the result, tool call errors; mitigated by §F UAT (try object-result fallback). |
| A3 | `updatedInput` should echo `input` on allow | §D | If omitted-on-allow is required, harmless; echoing is the safer superset. |
| A4 | MCP stdio uses `Content-Length` framing (LSP-style) | §G/Pitfall 5 | Wrong framing → server never initializes; mitigated by supporting both + UAT raw-byte logging. |
| A5 | A generous timeout (minutes) is acceptable UX for phone approval | Pitfall 3 | Too short denies legitimate approvals; make it an env knob, tune in UAT. Deny-default keeps it safe either way. |

**If this table is empty:** it is not — A1/A2/A4 in particular MUST be confirmed by the §F human-verify UAT before `prompt` mode ships. The planner must insert a `checkpoint:human-verify` task BEFORE the spawn-wiring task is marked done.

## Open Questions (RESOLVED)

1. **Exact MCP request/response field names + framing for CLI 2.1.178** — **RESOLVED: gated by the §F CONTRACT human-verify UAT (plan 04-02 Task 4) — a raw-frame-logging `permission-mcp` returning hardcoded allow captures the live frames before prompt-mode spawn-wiring is finalized; parse/response-wrap functions isolated for cheap correction.**
   - What we know: flag value is an MCP tool name; registration via `--mcp-config`; `PermissionResult{behavior,updatedInput,message}`; secondary-sourced `{tool_name,input,tool_use_id}` request + text/JSON.stringify response.
   - What's unclear: whether 2.1.178 emits `input` vs `parameters`, includes `tool_use_id`, uses `Content-Length` vs line framing, and accepts text-wrapped vs object result.

2. **Timeout window default** — **RESOLVED: env knob `BAUDE_PERMISSION_TIMEOUT_S`, default ~120s; deny-on-expiry makes any value safe. Claude's discretion per CONTEXT.**
   - What we know: must deny on expiry; phone approval can be slow.

3. **Wakeup mechanism (Notify/watch vs poll)** — **RESOLVED: `tokio::sync::Notify`/`watch` for instant wake; bounded `META_POLL_MS` poll is the acceptable simpler fallback. Claude's discretion.**

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `claude` CLI | `--permission-prompt-tool`, hooks | ✓ | 2.1.178 | — |
| Rust toolchain (cargo) | build/CI | ✓ (assumed; workspace builds) | — | — |
| Existing crates (`ureq`,`serde_json`,`axum`,`tokio`) | all transport | ✓ | vendored | — |
| Web Push (VAPID + browser sub) | PERM-04 distinct push | ✓ (path exists v0.5) | — | Phone verification is a separate manual milestone, NOT a blocker (CONTEXT). |

**Missing dependencies with no fallback:** none.
**Missing dependencies with fallback:** Web Push *phone verification* is deferred/manual (not blocking).

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[cfg(test)] mod tests` (`cargo test`); no external test crate |
| Config file | none — Cargo built-in |
| Quick run command | `cargo test -p baude-core permission::` / `cargo test -p bauded manager:: api:: notify::` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| PERM-01 | `prompt` mode appends `--permission-prompt-tool`, `skip` appends `--dangerously-skip-permissions`, no double-add | unit | `cargo test -p bauded manager::permission_flag` | ❌ Wave 0 (new test) |
| PERM-01 | mutually exclusive — never both flags | unit | same | ❌ Wave 0 |
| PERM-02 | `GET /permission` returns pending / null; 404 unknown id | integration | `cargo test -p bauded api::permission` | ❌ Wave 0 |
| PERM-02 | `POST /permission` resolves; unknown `decision` → 400; 404 unknown id | integration | same | ❌ Wave 0 |
| PERM-02 | resolve wakes a waiter; **timeout → deny** (never allow) | unit | `cargo test -p bauded manager::pending` | ❌ Wave 0 |
| PERM-04 | `waiting_reason` maps `permission_prompt`→permission, else input/none | unit | `cargo test -p baude-core meta::waiting_reason` | ❌ Wave 0 |
| PERM-04 | distinct push fires once via `notified_permission`, re-arms on resolve | unit | `cargo test -p bauded notify::permission` | ❌ Wave 0 |
| §C/§D | MCP approve-result builder produces `content[0].text` = JSON of `{behavior,...}`; untyped request parse tolerates `parameters`/`tool_input`/missing fields | unit | `cargo test -p baude-core permission::` | ❌ Wave 0 |
| §F | live `claude -p --permission-prompt-tool` round-trip confirms field names/framing | manual-only | `checkpoint:human-verify` UAT | ❌ Wave 0 (gating) |
| PERM-03 | card renders while pending, disappears on resolve | manual-only | PWA UAT (browser) | n/a |

### Sampling Rate
- **Per task commit:** the targeted `cargo test -p <crate> <module>::` for the task's module.
- **Per wave merge:** `cargo test --workspace`.
- **Phase gate:** full CI triad green before `/gsd-verify-work`: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.

### Wave 0 Gaps
- [ ] `bauded/src/manager.rs` tests — `permission_flag` selection (skip/prompt/no-double-add), `pending`/`resolve_pending` incl. timeout→deny — covers PERM-01/02
- [ ] `bauded/src/api.rs` tests — GET/POST `/permission` (pending, null, 404, 400 on bad decision) — covers PERM-02
- [ ] `bauded/src/notify.rs` test — distinct permission push debounce + re-arm; update the `#[cfg(test)] info(...)` constructor for the new `waiting_reason` field (the recurring 02-03/03-02 Rule-3 compile fix) — covers PERM-04
- [ ] `baude-core` tests — `waiting_reason` mapping + MCP approve-result builder + untyped request parse — covers PERM-04/§C/§D
- [ ] No framework install needed (Cargo built-in).

## Security Domain

> **This is the ONE phase that gates tool execution.** `security_enforcement` is enabled; ASVS Level 1. Two controls are SECURITY-CRITICAL and must be enforced + tested: **deny-on-timeout (never auto-allow)** and **`skip` stays the unattended default (`prompt` opt-in only)**.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | Inherits the project model: bind the VPN/tailnet interface; no auth layer (single-user by design). New `/permission` routes inherit this. |
| V3 Session Management | no | No sessions/cookies; daemon is single-user loopback/tailnet. |
| V4 Access Control | yes (deny-default) | The permission decision itself IS an access-control gate: default-deny on timeout, on malformed decision, and on unknown id. `prompt` is opt-in; `skip` default is explicit and documented. |
| V5 Input Validation | yes | `Path<u64>` rejects non-numeric ids (framework layer); validate `decision ∈ {allow,deny}` (else 400); untyped `Value` reads of the MCP request never panic on odd payloads; `event_path`-style sanitization if any id is used in a filesystem path. |
| V6 Cryptography | no (reuse) | Web Push VAPID/encryption is the existing v0.5 path — never hand-rolled, not modified. |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Auto-allow on timeout / silent fail-open | Elevation of Privilege | **Deny on timeout**; test asserts timeout → deny; deny-default everywhere. SECURITY-CRITICAL. |
| `prompt` becoming the unattended default (overnight runs blocked OR a default-prompt regression) | Denial of Service / EoP | `skip` is the hard default; `permission_flag` defaults to `--dangerously-skip-permissions`; test pins the default. PROJECT non-negotiable. |
| Malformed/unknown `decision` value treated as allow | Tampering / EoP | Validate `decision` strictly; unknown → 400, never allow. |
| XSS via tool name / tool input in the PWA card | Tampering (client) | `esc()` every dynamic string in the card (established PWA rule). |
| Unauthenticated `/permission` routes | Spoofing | Accepted per project baseline — inherits the tailnet/loopback bind; no new exposure beyond the existing REST surface (same disposition as T-02-08). |
| Path/id injection on the new routes | Tampering | `Path<u64>` rejects non-numeric ids at the framework layer (no 500 path), matching `post_event`/`get_activity`. |
| Daemon stalls while one permission pends (lock held across human wait) | Denial of Service | Set pending under the lock, await OUTSIDE it (Pitfall 4) — the existing "decide under lock, act outside" rule. |

## Sources

### Primary (HIGH confidence)
- `claude --version` → **2.1.178** — installed CLI version pin `[VERIFIED]`
- `code.claude.com/docs/en/cli-reference` — `--permission-prompt-tool` is "an MCP tool to handle permission prompts in non-interactive mode"; `--mcp-config`/`--strict-mcp-config` registration `[CITED]`
- `code.claude.com/docs/en/agent-sdk/permissions` — permission evaluation order (bypass short-circuits before prompt); `PermissionResult{behavior,message,updatedInput}`; `mcp__<server>__<tool>` naming `[CITED]`
- Codebase: `baude-core/src/hook.rs`, `baude/src/main.rs`, `bauded/src/main.rs`, `bauded/src/manager.rs`, `bauded/src/api.rs`, `bauded/src/notify.rs`, `baude-core/src/meta.rs:398-474`, `bauded/web/app.js` — all integration patterns `[VERIFIED: grep/read]`

### Secondary (MEDIUM confidence)
- vibesparking.com "Outsource Permissions … --permission-prompt-tool" — request `{tool_use_id, tool_name, input}`; response `content` `type:"text"` with `JSON.stringify({behavior:"allow"|"deny"})`
- LobeHub `--permission-prompt-tool` MCP server overview — same request/response shape, three-layer evaluation
- WebSearch digest corroborating `PermissionResult{behavior, message?, updatedInput?}`

### Tertiary (LOW confidence)
- claude-code GitHub issue #1175 — confirms NO complete official example exists (the ambiguity itself); used only to establish the gap, not the shape.

## Metadata

**Confidence breakdown:**
- Standard stack / no-new-deps: HIGH — all transport crates already vendored; verified in `Cargo.toml`.
- Daemon/PWA/push integration (PERM-02/03/04): HIGH — direct re-application of `interrupt`/`post_event`/`Notifier`/`renderChat` patterns read in-repo.
- Claude Code `--permission-prompt-tool` flag value + registration (§A/§B/§E): MEDIUM-HIGH — official CLI/SDK docs.
- Claude Code request/response JSON shape + framing (§C/§D/§G): MEDIUM — corroborated secondary + SDK type, but no complete official example; gated by the §F human-verify UAT.

**Research date:** 2026-06-15
**Valid until:** 2026-07-15 for the integration patterns (stable repo); **7 days** for the Claude Code contract (fast-moving CLI — re-verify on any `claude` upgrade past 2.1.178).

## RESEARCH COMPLETE

**Phase:** 4 - Remote Permission Approval
**Confidence:** MEDIUM (HIGH integration; MEDIUM on the Claude Code wire contract — flagged + UAT-gated)

### Key Findings
- `--permission-prompt-tool` takes an **MCP tool name** (`mcp__baude__approve`), not a command; register the stdio server via `--mcp-config`/`.mcp.json` (the Phase-2 seeding analog). Plain-command wiring silently never fires. `[CITED: cli-reference]`
- The request/response JSON (`{tool_name,input,tool_use_id}` → MCP `text` content of `JSON.stringify({behavior,updatedInput?,message?})`) is corroborated but NOT in a complete official example (issue #1175). **Build the safest interpretation and gate it behind a §F human-verify UAT** before `prompt` ships.
- **No new crates** — `ureq`/`serde_json`/`axum`/`tokio` cover the blocking bridge, JSON-RPC, routes, and wait. Hand-roll the 3-method stdio MCP server per the project no-new-deps rule.
- PERM-02/03/04 are mechanical re-applications of existing patterns (`Path<u64>` handlers, `Manager` state, `Notifier` debounce sets, existing `push::send`, `renderChat()` card). Watch the `bauded permission-mcp` fall-through trap and the `notify.rs` test-constructor Rule-3 fix.
- **Security-critical:** deny-on-timeout (never auto-allow) and `skip`-stays-default — both must be enforced AND unit-tested; the bridge blocks but the daemon must wait outside the lock.

### File Created
`.planning/phases/04-remote-permission-approval/04-RESEARCH.md`

### Confidence Assessment
| Area | Level | Reason |
|------|-------|--------|
| Standard Stack | HIGH | No new deps; all vendored, verified in Cargo.toml |
| Architecture | HIGH | Direct re-use of read-in-repo patterns |
| Claude Code contract | MEDIUM | Flag value/registration official; JSON shape secondary + SDK, no official end-to-end example — UAT-gated |
| Pitfalls / Security | HIGH | Grounded in prior-phase threat notes + project non-negotiables |

### Open Questions
- Exact request field names + MCP stdio framing for 2.1.178 (resolve via §F UAT, gate spawn-wiring behind it).
- Timeout default (env knob, deny-default makes any value safe).

### Ready for Planning
Research complete. Planner can create PLAN.md files. **MUST** insert a `checkpoint:human-verify` task implementing the §F contract-confirmation UAT BEFORE the `prompt`-mode spawn-wiring task is finalized.
