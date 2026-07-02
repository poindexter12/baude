//! Permission-mode spawn-flag selection — the pure, testable half of PERM-01.
//!
//! A per-deploy `BAUDE_PERMISSION_MODE = skip | prompt` env var (default
//! `skip`) selects exactly one permission flag for the spawned `claude`
//! command:
//!
//! - `skip` (the unattended default) appends `--dangerously-skip-permissions`,
//!   preserving today's behavior.
//! - `prompt` (strictly opt-in) appends `--permission-prompt-tool
//!   mcp__baude__approve` and — at the spawn sites, NOT here — seeds a
//!   `.mcp.json` registering the `permission-mcp` stdio server.
//!
//! SECURITY-CRITICAL (PERM-01 / T-04-01): `prompt` is reachable ONLY by the
//! exact literal `"prompt"`; an unset var and ANY unrecognized value fall back
//! to `skip` (fail-safe — never accidentally gate tool execution behind a
//! phone). A regression making `prompt` the default would block overnight runs
//! and is a high-severity finding.
//!
//! Like [`crate::hook`], this module is the PURE half: no HTTP, no process
//! spawning, no filesystem writes. The env read happens here (it is a pure
//! read of process state); the `current_exe()` resolution and the actual
//! `.mcp.json` write live in the binaries (the same core/binary split as
//! `hook::seed_settings` vs `hook::baude_hook_command`). Every value is built
//! via `serde_json::json!` so a build never panics.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// The flag appended to a base `claude` command in `skip` mode (and the
/// default). Leading space lets the caller append directly to `base_cmd`.
const SKIP_FLAG: &str = " --dangerously-skip-permissions";

/// The flag appended in `prompt` mode. The MCP tool name follows the standard
/// `mcp__<server>__<tool>` form (research §A); the `baude` server exposing the
/// `approve` tool is registered via the seeded `.mcp.json`.
const PROMPT_FLAG: &str = " --permission-prompt-tool mcp__baude__approve";

/// Build the `claude` command to spawn for the current `BAUDE_PERMISSION_MODE`,
/// applied to `base_cmd`. Reads the env; the env-free core is
/// [`resolve_claude_cmd`] (split so tests never mutate the process-global var,
/// which would race concurrent spawns — same pattern as `hook`).
pub fn resolve_claude_cmd_env(base_cmd: &str) -> ResolvedCmd {
    resolve_claude_cmd(
        std::env::var("BAUDE_PERMISSION_MODE").ok().as_deref(),
        base_cmd,
    )
}

/// The spawn command plus whether a conflicting skip flag was stripped (so the
/// caller can warn). `stripped_skip` is only ever `true` in `prompt` mode.
#[derive(Debug, PartialEq, Eq)]
pub struct ResolvedCmd {
    pub cmd: String,
    pub stripped_skip: bool,
}

/// Pure: resolve the spawn command for an explicit `mode` (`None` = unset).
///
/// `prompt` is an EXPLICIT opt-in and WINS over a `--dangerously-skip-permissions`
/// baked into the operator's `claude_cmd` (BL-04 — that combination previously
/// suppressed prompt mode silently via the old no-double-add short-circuit,
/// leaving sessions in skip while baude still seeded `.mcp.json`). In `prompt`
/// mode a conflicting skip flag is STRIPPED from `base_cmd` before [`PROMPT_FLAG`]
/// is appended, and `stripped_skip` is set so the caller can warn. An
/// operator-set `--permission-prompt-tool`/`--permission-mode` is still respected
/// (no double-add) — they explicitly chose a prompt-style flag.
///
/// `skip`/default (unset, `"skip"`, any unrecognized value — fail-safe, T-04-01)
/// keeps the original no-double-add: append [`SKIP_FLAG`] only when `base_cmd`
/// carries no permission flag at all.
pub fn resolve_claude_cmd(mode: Option<&str>, base_cmd: &str) -> ResolvedCmd {
    let has_prompt = base_cmd.contains("--permission-prompt-tool");
    let has_mode = base_cmd.contains("--permission-mode");
    let has_skip = base_cmd.contains("--dangerously-skip-permissions");
    match mode {
        Some("prompt") => {
            if has_prompt || has_mode {
                // Operator explicitly set a prompt-style flag — respect it.
                ResolvedCmd {
                    cmd: base_cmd.to_string(),
                    stripped_skip: false,
                }
            } else if has_skip {
                // Explicit opt-in wins: drop the conflicting skip, add prompt.
                let stripped = strip_token(base_cmd, "--dangerously-skip-permissions");
                ResolvedCmd {
                    cmd: format!("{stripped}{PROMPT_FLAG}"),
                    stripped_skip: true,
                }
            } else {
                ResolvedCmd {
                    cmd: format!("{base_cmd}{PROMPT_FLAG}"),
                    stripped_skip: false,
                }
            }
        }
        _ => {
            let cmd = if has_prompt || has_mode || has_skip {
                base_cmd.to_string()
            } else {
                format!("{base_cmd}{SKIP_FLAG}")
            };
            ResolvedCmd {
                cmd,
                stripped_skip: false,
            }
        }
    }
}

/// Remove every whitespace-delimited occurrence of `token` from `cmd`,
/// collapsing surrounding whitespace to single spaces. The realistic
/// `claude_cmd` is simple flag tokens (no quoted spaces), so a split/rejoin is
/// safe and total.
fn strip_token(cmd: &str, token: &str) -> String {
    cmd.split_whitespace()
        .filter(|t| *t != token)
        .collect::<Vec<_>>()
        .join(" ")
}

/// `true` iff `prompt` mode is active (the exact literal `"prompt"`).
///
/// The spawn sites use this to decide whether to additionally seed `.mcp.json`.
/// Mirrors the fail-safe of [`resolve_claude_cmd`]: only the exact `"prompt"`
/// literal is `prompt` mode; everything else is `skip`.
pub fn is_prompt_mode() -> bool {
    std::env::var("BAUDE_PERMISSION_MODE").as_deref() == Ok("prompt")
}

/// BL-02: the claude `permissionMode` baude's spawn flag IMPLIES, for display
/// when the transcript hasn't reported one yet (a freshly spawned session shows
/// no model/mode until its transcript resolves and carries `permissionMode`).
///
/// `skip`/default → `Some("bypassPermissions")` (the `--dangerously-skip-permissions`
/// baude spawns with). `prompt` → `None`: claude runs in its normal default mode
/// and the `approve` tool intercepts, so there is no distinct mode label to imply.
/// Callers use this only as a FALLBACK — a transcript-reported mode always wins
/// (the user may switch modes mid-session, and claude is authoritative).
pub fn spawn_permission_mode() -> Option<&'static str> {
    if is_prompt_mode() {
        None
    } else {
        Some("bypassPermissions")
    }
}

/// Build the `.mcp.json` body registering the `baude` permission MCP server.
///
/// Returns `{"mcpServers":{"baude":{"command":<exe>,"args":["permission-mcp"]}}}`.
/// `exe` is the absolute `current_exe()` path the CALLER resolves (mirroring
/// [`crate::hook::baude_hook_command`]) — core stays pure and never calls
/// `current_exe()`. Building the Value never panics.
pub fn mcp_server_config(exe: &str) -> serde_json::Value {
    serde_json::json!({
        "mcpServers": {
            "baude": {
                "command": exe,
                "args": ["permission-mcp"],
            }
        }
    })
}

/// Idempotently merge baude's `permission-mcp` server registration into an
/// existing `.mcp.json` value (PERM-01 / T-04-03).
///
/// Pure `Value -> Value` transform mirroring [`crate::hook::merge_hook_settings`]:
/// the binaries own the read/write (the env-read/`current_exe()`/filesystem
/// half), this owns the non-clobbering merge. Only the `mcpServers.baude` key is
/// set (to `exe` + `["permission-mcp"]`); a user's sibling servers and any other
/// top-level keys survive byte-intact. Re-running on its own output is a no-op
/// (idempotent). Never panics on a minimal / non-object / odd file.
pub fn merge_mcp_config(existing: &serde_json::Value, exe: &str) -> serde_json::Value {
    let mut root = existing.clone();
    if !root.is_object() {
        root = serde_json::json!({});
    }
    let obj = root.as_object_mut().expect("root coerced to object");
    let servers = obj
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}));
    if !servers.is_object() {
        *servers = serde_json::json!({});
    }
    let servers_obj = servers
        .as_object_mut()
        .expect("mcpServers coerced to object");
    // Overwrite only our own `baude` entry; sibling servers are untouched.
    servers_obj.insert(
        "baude".to_string(),
        mcp_server_config(exe)["mcpServers"]["baude"].clone(),
    );
    root
}

/// The seeded `.mcp.json` location for a session cwd. Both spawn sites agree on
/// this so the daemon (which re-spawns on `restore`) and the TUI write the same
/// file.
pub fn mcp_config_path(cwd: &Path) -> PathBuf {
    cwd.join(".mcp.json")
}

// ===== 04-02: hand-rolled JSON-RPC framing + MCP `approve`-tool transforms ==
//
// The pure, testable half of the `permission-mcp` stdio bridge (PERM-01
// transport + PERM-02). Like [`crate::hook`], NO HTTP and NO stdin reads live
// here — the binary owns the network/stdio (the `dispatch_hook` split). Every
// value is read untyped via `serde_json::Value` and a malformed/partial/odd
// payload yields `None`/empty and NEVER panics (Pitfall 5, T-04-06).
//
// SECURITY-ISOLATION: [`parse_frame`], [`parse_tool_call`], and
// [`build_approve_result`] are deliberately the ONLY functions encoding the
// ASSUMED Claude Code `--permission-prompt-tool` wire contract (RESEARCH §C/§D,
// MEDIUM confidence — no complete official example, claude-code #1175). The §F
// CONTRACT human-verify UAT confirms the live 2.1.178 shape; if it diverges,
// only these three functions change (framing, request field names, response
// envelope) — the binary loop and daemon round-trip are untouched.

/// Parse a single JSON-RPC frame from the front of `buf`.
///
/// Supports BOTH framings Claude's MCP stdio transport may use (Assumption A4,
/// confirmed by the §F UAT):
///
/// - **`Content-Length:` header + body** (LSP/MCP-stdio style): parse the byte
///   count, skip the blank-line separator (`\r\n\r\n` or `\n\n`), and slice
///   exactly that many body bytes.
/// - **bare line-delimited JSON**: the buffer up to the next newline is one
///   JSON body.
///
/// Returns `Some((body, consumed))` where `consumed` is the total bytes of the
/// frame (header + separator + body, or the line incl. its newline) so the
/// caller can advance its accumulating buffer and parse the next frame. Returns
/// `None` on an INCOMPLETE frame (more bytes needed) OR an unparseable body —
/// the binary loop reads more stdin and retries. Never panics (T-04-06).
pub fn parse_frame(buf: &[u8]) -> Option<(Value, usize)> {
    // Try Content-Length framing first if the buffer opens with the header.
    // Case-insensitive, tolerant of either CRLF or LF separators.
    if starts_with_ci(buf, b"content-length:") {
        return parse_content_length_frame(buf);
    }
    // Otherwise: line-delimited. A frame is everything up to (and including)
    // the next '\n'; without a newline the frame is incomplete -> None.
    let nl = buf.iter().position(|&b| b == b'\n')?;
    let line = &buf[..nl];
    let body = serde_json::from_slice::<Value>(line).ok()?;
    Some((body, nl + 1))
}

/// `true` iff `buf` begins with `prefix`, ASCII-case-insensitively.
fn starts_with_ci(buf: &[u8], prefix: &[u8]) -> bool {
    buf.len() >= prefix.len()
        && buf[..prefix.len()]
            .iter()
            .zip(prefix)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
}

/// Parse an LSP-style `Content-Length: N\r\n\r\n<N bytes>` frame. Tolerates a
/// bare-LF (`\n\n`) separator. Returns `None` until the full body has arrived,
/// on a non-numeric/overflowing length, or on an unparseable body. No panics.
fn parse_content_length_frame(buf: &[u8]) -> Option<(Value, usize)> {
    // Find the header/body separator: prefer the CRLF pair, fall back to LF.
    let (sep_start, sep_len) = find_separator(buf)?;
    let header = std::str::from_utf8(&buf[..sep_start]).ok()?;
    let len: usize = header
        .split(':')
        .nth(1)?
        .trim()
        .parse() // a negative/overflowing value fails to parse -> None
        .ok()?;
    let body_start = sep_start + sep_len;
    let body_end = body_start.checked_add(len)?;
    if buf.len() < body_end {
        return None; // body not fully arrived yet
    }
    let body = serde_json::from_slice::<Value>(&buf[body_start..body_end]).ok()?;
    Some((body, body_end))
}

/// Locate the blank-line separator that ends the header block. Returns the
/// byte offset where the separator starts and its length (`4` for `\r\n\r\n`,
/// `2` for `\n\n`). `None` if no complete separator is present yet.
fn find_separator(buf: &[u8]) -> Option<(usize, usize)> {
    let crlf = buf.windows(4).position(|w| w == b"\r\n\r\n");
    let lf = buf.windows(2).position(|w| w == b"\n\n");
    match (crlf, lf) {
        (Some(c), Some(l)) if c <= l => Some((c, 4)),
        (_, Some(l)) => Some((l, 2)),
        (Some(c), None) => Some((c, 4)),
        (None, None) => None,
    }
}

/// Extract `(tool_name, input)` from a `tools/call` params object (§C).
///
/// Reads `tool_name` as the tool string. For the input, reads `input`, falling
/// back to `parameters` then `tool_input` when `input` is absent (the field
/// name varies across CLI versions — untyped reads make baude robust to which
/// 2.1.178 emits). A missing `tool_use_id` is tolerated (not required here). A
/// minimal/odd/`null` params yields `("", Value::Null)` and never panics.
pub fn parse_tool_call(params: &Value) -> (String, Value) {
    let tool = params["tool_name"].as_str().unwrap_or_default().to_string();
    let input = if !params["input"].is_null() {
        params["input"].clone()
    } else if !params["parameters"].is_null() {
        params["parameters"].clone()
    } else {
        // tool_input fallback (null when also absent — never panics).
        params["tool_input"].clone()
    };
    (tool, input)
}

/// Build the MCP `tools/call` result the `approve` tool returns (§D).
///
/// On `behavior == "allow"`, the inner `PermissionResult` is
/// `{"behavior":"allow","updatedInput":<updated_input or {}>}` — the input is
/// echoed back verbatim (the safest rule: a CLI that requires `updatedInput`
/// on allow is satisfied). On ANY other value (`"deny"`, an empty string, an
/// unknown token) it coerces to `{"behavior":"deny","message":<message or
/// "denied">}` — NEVER emit allow for a non-"allow" string (deny-default,
/// SECURITY-CRITICAL T-04-04/V4). The inner object is `serde_json::to_string`'d
/// and wrapped as `{"content":[{"type":"text","text":<that string>}]}`.
///
/// This is the ONE function encoding the response envelope so the §F UAT can
/// correct it cheaply (e.g. to return the object directly) if 2.1.178 diverges.
pub fn build_approve_result(
    behavior: &str,
    updated_input: Option<&Value>,
    message: Option<&str>,
) -> Value {
    let inner = if behavior == "allow" {
        json!({
            "behavior": "allow",
            "updatedInput": updated_input.cloned().unwrap_or_else(|| json!({})),
        })
    } else {
        // Deny-default: coerce every non-"allow" value to deny, dropping any
        // echoed input so an approval payload can never leak on a coercion.
        json!({
            "behavior": "deny",
            "message": message.unwrap_or("denied"),
        })
    };
    // Isolated text/JSON.stringify wrapping (§D). Serialization of a plain
    // object never fails; fall back to "{}" defensively rather than panic.
    let text = serde_json::to_string(&inner).unwrap_or_else(|_| "{}".to_string());
    json!({
        "content": [ { "type": "text", "text": text } ]
    })
}

/// Build a JSON-RPC success envelope: `{"jsonrpc":"2.0","id":<id>,"result":..}`.
/// `id` is echoed verbatim (numbers and strings both valid per JSON-RPC). A
/// notification (no `id`) gets no response — the caller skips this entirely.
pub fn rpc_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// Build a JSON-RPC error envelope:
/// `{"jsonrpc":"2.0","id":<id>,"error":{"code":..,"message":..}}`.
pub fn rpc_error(id: Value, code: i64, msg: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": msg } })
}

/// Read the deny-on-timeout window (`BAUDE_PERMISSION_TIMEOUT_S`, default 120s).
/// A missing/garbage/zero value falls back to the default — never 0 (which would
/// deny instantly) and never panics. The deny-default makes any value safe (A5).
/// Shared by both binaries' `permission-mcp` bridge so the window is one rule.
pub fn permission_timeout_s() -> u64 {
    const DEFAULT: u64 = 120;
    std::env::var("BAUDE_PERMISSION_TIMEOUT_S")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT)
}

/// The pure deny-on-timeout resolution rule (SECURITY-CRITICAL, T-04-04 / V4).
///
/// Given the decision read so far (`None` = none yet) and whether the deadline
/// has passed, return the final verdict the bridge emits:
/// - a recorded `"allow"` wins (the human approved);
/// - a recorded non-`"allow"` value coerces to `"deny"` (deny-default — an
///   unknown value is NEVER allow);
/// - no decision AND deadline passed -> `"deny"` (never auto-allow);
/// - no decision AND still within the window -> `""` (keep polling).
///
/// Shared by both binaries' bridge loop so the security-critical rule is tested
/// once and never diverges between `baude` and `bauded`.
pub fn decide_with_timeout(decision: Option<&str>, deadline_passed: bool) -> &'static str {
    match decision {
        Some("allow") => "allow",
        Some(_) => "deny",
        None if deadline_passed => "deny",
        None => "",
    }
}

/// The MCP protocol version baude advertises in `initialize`. Claude's stdio
/// transport echoes/negotiates this; a recent stable value is safe (the §F UAT
/// confirms the handshake is accepted for 2.1.178).
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// The single tool baude exposes — `mcp__baude__approve`, the value passed to
/// `--permission-prompt-tool`.
const APPROVE_TOOL: &str = "approve";

/// The `tools/list` descriptor for the one `approve` tool (§G). The input
/// schema names the request fields baude reads (`tool_name`/`input`/
/// `tool_use_id`) so the CLI knows the tool's shape.
fn approve_tool_descriptor() -> Value {
    json!({
        "name": APPROVE_TOOL,
        "description": "Route a tool-permission decision through baude (phone approval).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "tool_name": { "type": "string" },
                "input": { "type": "object" },
                "tool_use_id": { "type": "string" }
            }
        }
    })
}

/// Dispatch ONE parsed JSON-RPC request to its MCP reply (pure — no IO).
///
/// Handles the three methods baude's stdio server answers (§G):
/// - `initialize` → `protocolVersion` + `capabilities.tools` + `serverInfo`.
/// - `tools/list` → the single [`approve_tool_descriptor`].
/// - `tools/call` (name == `approve`) → `parse_tool_call` the params, invoke
///   `resolve(tool, &input)` for the decision string, then
///   `build_approve_result(decision, Some(&input), …)` (echo input on allow).
///   Any unknown decision string is coerced to deny inside the builder.
///
/// A JSON-RPC notification (no `id`) returns `None` (no reply — the loop skips
/// it). Two distinct error codes (IN-01): an unknown *method* returns `-32601`
/// (Method not found), while a `tools/call` for an unknown *tool name* returns
/// `-32602` (Invalid params) — the tool name is a param of the one registered
/// `tools/call` method, so `-32602` is the intended code (pinned by the unknown-
/// tool test). `resolve` is injected so the
/// network round-trip (POST request + long-poll, deny-on-timeout) stays in the
/// binary; this dispatch is fully unit-testable. Never panics on odd input.
pub fn dispatch_rpc<F>(req: &Value, resolve: F) -> Option<Value>
where
    F: FnOnce(&str, &Value) -> String,
{
    // A notification (no id) is fire-and-forget — no response.
    let id = req.get("id")?.clone();
    let method = req["method"].as_str().unwrap_or_default();
    match method {
        "initialize" => Some(rpc_response(
            id,
            json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "baude", "version": env!("CARGO_PKG_VERSION") },
            }),
        )),
        "tools/list" => Some(rpc_response(
            id,
            json!({ "tools": [ approve_tool_descriptor() ] }),
        )),
        "tools/call" => {
            let params = &req["params"];
            let name = params["name"].as_str().unwrap_or_default();
            if name != APPROVE_TOOL {
                return Some(rpc_error(id, -32602, "unknown tool"));
            }
            let (tool, input) = parse_tool_call(&params["arguments"]);
            // The injected resolver does the daemon round-trip; deny-on-timeout
            // and fail-closed-on-no-daemon live there. Any non-"allow" string
            // is coerced to deny by build_approve_result (deny-default).
            let decision = resolve(&tool, &input);
            Some(rpc_response(
                id,
                build_approve_result(&decision, Some(&input), None),
            ))
        }
        _ => Some(rpc_error(id, -32601, "method not found")),
    }
}

/// Run the blocking stdio JSON-RPC `permission-mcp` server until stdin closes.
///
/// The BLOCKING inverse of [`crate::hook::dispatch_hook`]: the hook is
/// fire-and-forget exit-0; this server sits on Claude's critical path and each
/// `tools/call` blocks (via the injected `resolve`) until a human decision
/// arrives or the deadline denies. Accumulates stdin bytes, extracts each frame
/// with [`parse_frame`] (Content-Length AND line framing), dispatches via
/// [`dispatch_rpc`], and writes each reply as a newline-delimited JSON line to
/// stdout (the framing live claude's MCP stdio client requires — §F UAT).
///
/// `resolve(tool, &input) -> decision` is the ONLY injected seam: the binary
/// owns the env read (`$BAUDE_EVENT_URL`-derived permission URL +
/// `$BAUDE_PERMISSION_TIMEOUT_S`) and the `ureq` POST-then-long-poll, returning
/// `"allow"`/`"deny"`. Keeping it a closure means the framing/protocol is
/// unit-tested here while no HTTP dependency enters baude-core. Best-effort
/// throughout: a malformed frame is skipped, never panicked on (Pitfall 5).
pub fn run_permission_mcp<R, W, F>(mut input: R, mut output: W, mut resolve: F)
where
    R: std::io::Read,
    W: std::io::Write,
    F: FnMut(&str, &Value) -> String,
{
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        // Drain any complete frames already buffered.
        while let Some((req, consumed)) = parse_frame(&buf) {
            buf.drain(..consumed);
            if let Some(reply) = dispatch_rpc(&req, |tool, inp| resolve(tool, inp)) {
                if write_frame(&mut output, &reply).is_err() {
                    return; // stdout closed — Claude went away.
                }
            }
        }
        // Need more bytes.
        match input.read(&mut chunk) {
            Ok(0) => return, // stdin EOF — Claude closed the server.
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(_) => return,
        }
    }
}

/// Write one JSON-RPC reply as a single newline-delimited JSON line — the MCP
/// stdio transport framing.
///
/// CONTRACT (§F UAT, live claude 2.1.187): Claude Code's MCP stdio *client*
/// accepts ONLY newline-delimited JSON on the server's stdout; an LSP-style
/// `Content-Length:`-framed reply is silently dropped, the `initialize`
/// handshake never completes, and the `approve` tool is reported "not found"
/// (prompt mode fully broken). `serde_json::to_vec` is compact (no embedded
/// newlines), so a trailing `\n` is a valid single frame. Input framing is
/// unaffected — [`parse_frame`] still accepts both line- and Content-Length-
/// framing for robustness; live claude only ever sends line-delimited.
fn write_frame<W: std::io::Write>(out: &mut W, v: &Value) -> std::io::Result<()> {
    let body = serde_json::to_vec(v).unwrap_or_else(|_| b"{}".to_vec());
    out.write_all(&body)?;
    out.write_all(b"\n")?;
    out.flush()
}

/// Derive the daemon `/permission` URL for THIS session from the spawn-injected
/// `$BAUDE_EVENT_URL` (`…/sessions/{id}/event`). The MCP server inherits the
/// session shell's exported env (it is a grandchild of the spawn). Returns
/// `None` when the var is absent (no daemon) — the bridge then fails CLOSED to
/// deny (never allow), the security-critical no-daemon path.
pub fn permission_url_from_event_url(event_url: &str) -> Option<String> {
    let trimmed = event_url.strip_suffix("/event")?;
    Some(format!("{trimmed}/permission"))
}

/// PERM-04: classify *why* a session is waiting, for `SessionInfo.waiting_reason`
/// (and the distinct permission push). Pure, total, panic-free over the
/// event-derived `last_notification` (`meta.rs:447`, `Option<(type, ts)>`):
///
/// - a notification type CONTAINING `"permission"` → `"permission"` (the phone
///   shows the approve/deny card and gets a distinct push);
/// - otherwise, if the session is `waiting` → `"input"` (generic prompt);
/// - otherwise, if the session is `completed` (three-state status:
///   `Status::Completed`, decided by `session::idle_kind`) → `"completed"`;
/// - otherwise → `"none"` (active / not waiting).
///
/// The `"permission"` arm fires even when `waiting` is false: a pending
/// permission is itself a waiting state, and the notification is the
/// authority. `waiting` and `completed` are mutually exclusive by
/// construction (both are derived from the same `Status`), so their relative
/// order below is immaterial — kept as `waiting` first to match the
/// pre-existing precedence.
pub fn waiting_reason(
    last_notification: Option<&(String, u64)>,
    waiting: bool,
    completed: bool,
) -> &'static str {
    match last_notification {
        Some((nt, _)) if nt.contains("permission") => "permission",
        _ if waiting => "input",
        _ if completed => "completed",
        _ => "none",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// `BAUDE_PERMISSION_MODE` is process-global; serialize the env-mutating
    /// tests so parallel cases never race the same var.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    // ---- resolve_claude_cmd: pure mode resolution (no env mutation) -----
    // Branch coverage uses the env-free seam so it never races concurrent
    // session spawns (in any crate) that read the process-global env var.

    #[test]
    fn resolve_claude_cmd_mode_selection() {
        // None (unset) -> default skip (security-critical default, T-04-01).
        assert_eq!(
            resolve_claude_cmd(None, "claude").cmd,
            "claude --dangerously-skip-permissions"
        );
        // Explicit "skip" -> skip.
        assert_eq!(
            resolve_claude_cmd(Some("skip"), "claude").cmd,
            "claude --dangerously-skip-permissions"
        );
        // "prompt" -> prompt flag (only on the exact literal).
        assert_eq!(
            resolve_claude_cmd(Some("prompt"), "claude").cmd,
            "claude --permission-prompt-tool mcp__baude__approve"
        );
        // Unrecognized value -> fail-safe to skip (never reach prompt).
        assert_eq!(
            resolve_claude_cmd(Some("bogus"), "claude").cmd,
            "claude --dangerously-skip-permissions"
        );
        // Case-mismatch is unrecognized -> skip (exact literal only).
        assert_eq!(
            resolve_claude_cmd(Some("Prompt"), "claude").cmd,
            "claude --dangerously-skip-permissions"
        );
        // None of these strip anything (no conflicting skip to remove).
        assert!(!resolve_claude_cmd(Some("prompt"), "claude").stripped_skip);
    }

    #[test]
    fn resolve_claude_cmd_no_double_add() {
        // Operator-set prompt-style flags are respected, never doubled (T-04-02).
        let r = resolve_claude_cmd(
            Some("prompt"),
            "claude --permission-prompt-tool mcp__other__x",
        );
        assert_eq!(r.cmd, "claude --permission-prompt-tool mcp__other__x");
        assert!(!r.stripped_skip);
        let r = resolve_claude_cmd(Some("prompt"), "claude --permission-mode acceptEdits");
        assert_eq!(r.cmd, "claude --permission-mode acceptEdits");
        // Skip mode never double-adds its own flag.
        assert_eq!(
            resolve_claude_cmd(Some("skip"), "claude --dangerously-skip-permissions").cmd,
            "claude --dangerously-skip-permissions"
        );
    }

    #[test]
    fn resolve_claude_cmd_bl04_prompt_strips_conflicting_skip() {
        // BL-04: prompt mode is an explicit opt-in and WINS over a skip flag
        // baked into claude_cmd — the skip is stripped, prompt is added, and
        // stripped_skip flags it so the caller can warn.
        let r = resolve_claude_cmd(Some("prompt"), "claude --dangerously-skip-permissions");
        assert_eq!(r.cmd, "claude --permission-prompt-tool mcp__baude__approve");
        assert!(r.stripped_skip, "must report it stripped the skip flag");
        // Strip works mid-command too, and only in prompt mode.
        let r = resolve_claude_cmd(
            Some("prompt"),
            "claude --dangerously-skip-permissions --foo x",
        );
        assert_eq!(
            r.cmd,
            "claude --foo x --permission-prompt-tool mcp__baude__approve"
        );
        // Skip/default mode never strips (preserves the operator's skip).
        assert!(!resolve_claude_cmd(None, "claude --dangerously-skip-permissions").stripped_skip);
    }

    #[test]
    fn resolve_claude_cmd_never_both_flags() {
        for mode in [None, Some("skip"), Some("prompt"), Some("bogus"), Some("")] {
            for base in [
                "claude",
                "claude --dangerously-skip-permissions",
                "claude --permission-mode acceptEdits",
            ] {
                let cmd = resolve_claude_cmd(mode, base).cmd;
                assert!(
                    !(cmd.contains("--dangerously-skip-permissions")
                        && cmd.contains("--permission-prompt-tool")),
                    "the two flags must never appear together: mode {mode:?} base {base:?} -> {cmd}"
                );
            }
        }
    }

    // ---- resolve_claude_cmd_env: the env-reading wrapper delegates ------
    // One guarded smoke test that the env wrapper reads BAUDE_PERMISSION_MODE
    // and routes to resolve_claude_cmd. Kept minimal and mutex-guarded;
    // restores the var immediately to shrink the race window with spawns.

    #[test]
    fn resolve_claude_cmd_env_reads_env_and_delegates() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("BAUDE_PERMISSION_MODE");
        assert_eq!(
            resolve_claude_cmd_env("claude").cmd,
            "claude --dangerously-skip-permissions"
        );
        std::env::remove_var("BAUDE_PERMISSION_MODE");
    }

    // ---- is_prompt_mode -------------------------------------------------

    #[test]
    fn is_prompt_mode_only_on_exact_literal() {
        let _guard = ENV_LOCK.lock().unwrap();

        std::env::remove_var("BAUDE_PERMISSION_MODE");
        assert!(!is_prompt_mode());

        std::env::set_var("BAUDE_PERMISSION_MODE", "prompt");
        assert!(is_prompt_mode());

        std::env::set_var("BAUDE_PERMISSION_MODE", "Prompt");
        assert!(!is_prompt_mode());

        std::env::remove_var("BAUDE_PERMISSION_MODE");
    }

    #[test]
    fn spawn_permission_mode_maps_skip_to_bypass_prompt_to_none() {
        let _guard = ENV_LOCK.lock().unwrap();
        // BL-02: skip/default implies bypassPermissions (the flag baude spawns);
        // prompt implies no distinct mode label (claude runs default + tool).
        std::env::remove_var("BAUDE_PERMISSION_MODE");
        assert_eq!(spawn_permission_mode(), Some("bypassPermissions"));
        std::env::set_var("BAUDE_PERMISSION_MODE", "skip");
        assert_eq!(spawn_permission_mode(), Some("bypassPermissions"));
        std::env::set_var("BAUDE_PERMISSION_MODE", "prompt");
        assert_eq!(spawn_permission_mode(), None);
        std::env::remove_var("BAUDE_PERMISSION_MODE");
    }

    // ---- mcp_server_config ---------------------------------------------

    #[test]
    fn mcp_server_config_shape() {
        let v = mcp_server_config("/abs/baude");
        assert_eq!(
            v["mcpServers"]["baude"]["command"].as_str(),
            Some("/abs/baude")
        );
        assert_eq!(
            v["mcpServers"]["baude"]["args"][0].as_str(),
            Some("permission-mcp")
        );
        assert_eq!(
            v["mcpServers"]["baude"]["args"].as_array().map(|a| a.len()),
            Some(1)
        );
    }

    #[test]
    fn mcp_server_config_never_panics_on_odd_exe() {
        // Empty / odd exe strings still build a valid Value (never panics).
        let v = mcp_server_config("");
        assert_eq!(v["mcpServers"]["baude"]["command"].as_str(), Some(""));
        let v = mcp_server_config("with spaces/baude binary");
        assert_eq!(
            v["mcpServers"]["baude"]["command"].as_str(),
            Some("with spaces/baude binary")
        );
    }

    // ---- merge_mcp_config ----------------------------------------------

    #[test]
    fn merge_mcp_config_preserves_siblings_and_is_idempotent() {
        let existing: serde_json::Value = serde_json::from_str(
            r#"{"mcpServers":{"other":{"command":"other-srv"}},"extra":true}"#,
        )
        .unwrap();

        let once = merge_mcp_config(&existing, "/abs/baude");
        // Sibling server + unrelated top-level key survive.
        assert_eq!(
            once["mcpServers"]["other"]["command"].as_str(),
            Some("other-srv")
        );
        assert_eq!(once["extra"].as_bool(), Some(true));
        // Our server registered with the permission-mcp arg.
        assert_eq!(
            once["mcpServers"]["baude"]["args"][0].as_str(),
            Some("permission-mcp")
        );
        assert_eq!(
            once["mcpServers"]["baude"]["command"].as_str(),
            Some("/abs/baude")
        );

        // Idempotent: re-merging its own output is a no-op.
        let twice = merge_mcp_config(&once, "/abs/baude");
        assert_eq!(once, twice);
    }

    #[test]
    fn merge_mcp_config_never_panics_on_odd_inputs() {
        // Empty object.
        let v = merge_mcp_config(&serde_json::json!({}), "/x");
        assert_eq!(
            v["mcpServers"]["baude"]["args"][0].as_str(),
            Some("permission-mcp")
        );
        // Non-object root.
        let v = merge_mcp_config(&serde_json::json!(42), "/x");
        assert!(v.is_object());
        assert_eq!(v["mcpServers"]["baude"]["command"].as_str(), Some("/x"));
        // mcpServers is a non-object scalar — coerced, no panic.
        let v = merge_mcp_config(&serde_json::json!({"mcpServers": 5}), "/x");
        assert_eq!(v["mcpServers"]["baude"]["command"].as_str(), Some("/x"));
    }

    // ---- mcp_config_path ------------------------------------------------

    #[test]
    fn mcp_config_path_joins_dot_mcp_json() {
        let p = mcp_config_path(Path::new("/tmp/session"));
        assert_eq!(p, PathBuf::from("/tmp/session/.mcp.json"));
    }

    // ==== 04-02 Task 1: JSON-RPC framing + MCP transforms ================
    // The §F-CONTRACT-isolated wire functions. Every bullet of the task
    // <behavior> is pinned here, including the *_never_panics posture mirrored
    // from hook.rs:222-296. The wire shape is the ASSUMED RESEARCH §C/§D
    // contract; the §F UAT confirms/corrects it cheaply via these functions.

    // ---- parse_frame ----------------------------------------------------

    #[test]
    fn parse_frame_line_delimited() {
        let buf = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n";
        let (body, consumed) = parse_frame(buf).expect("line frame parses");
        assert_eq!(body["method"].as_str(), Some("initialize"));
        assert_eq!(body["id"].as_u64(), Some(1));
        assert_eq!(consumed, buf.len());
    }

    #[test]
    fn parse_frame_content_length() {
        let body = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
        let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        let (parsed, consumed) = parse_frame(frame.as_bytes()).expect("LSP frame parses");
        assert_eq!(parsed["method"].as_str(), Some("tools/list"));
        assert_eq!(consumed, frame.len());
    }

    #[test]
    fn parse_frame_content_length_lf_only_separator() {
        // Some peers use bare \n line endings; tolerate both.
        let body = r#"{"id":3,"method":"x"}"#;
        let frame = format!("Content-Length: {}\n\n{}", body.len(), body);
        let (parsed, consumed) = parse_frame(frame.as_bytes()).expect("LF-only LSP frame parses");
        assert_eq!(parsed["method"].as_str(), Some("x"));
        assert_eq!(consumed, frame.len());
    }

    #[test]
    fn parse_frame_consumes_only_one_frame() {
        // Two line-delimited frames back to back: parse_frame returns the first
        // and reports exactly how many bytes it consumed so the caller can
        // advance and parse the rest.
        let first = "{\"id\":1}\n";
        let buf = format!("{first}{{\"id\":2}}\n");
        let (body, consumed) = parse_frame(buf.as_bytes()).expect("first frame parses");
        assert_eq!(body["id"].as_u64(), Some(1));
        assert_eq!(consumed, first.len());
        let (body2, _) = parse_frame(&buf.as_bytes()[consumed..]).expect("second frame parses");
        assert_eq!(body2["id"].as_u64(), Some(2));
    }

    #[test]
    fn parse_frame_partial_yields_none() {
        // Incomplete line (no newline yet) -> None (accumulate more bytes).
        assert!(parse_frame(b"{\"id\":1").is_none());
        // Content-Length header but body not fully arrived -> None.
        let frame = b"Content-Length: 50\r\n\r\n{\"id\":1}";
        assert!(parse_frame(frame).is_none());
        // Header line started but not terminated -> None.
        assert!(parse_frame(b"Content-Length: 10").is_none());
    }

    #[test]
    fn parse_frame_never_panics_on_garbage() {
        assert!(parse_frame(b"").is_none());
        assert!(parse_frame(b"not json\n").is_none());
        assert!(parse_frame(b"Content-Length: abc\r\n\r\n{}").is_none());
        // Negative/overflowing length never panics.
        assert!(parse_frame(b"Content-Length: -1\r\n\r\n{}").is_none());
        assert!(parse_frame(b"\n").is_none());
    }

    // ---- parse_tool_call ------------------------------------------------

    #[test]
    fn parse_tool_call_reads_tool_name_and_input() {
        let params = json!({
            "tool_name": "Bash",
            "input": {"command": "rm -rf build/"},
            "tool_use_id": "toolu_01"
        });
        let (tool, input) = parse_tool_call(&params);
        assert_eq!(tool, "Bash");
        assert_eq!(input["command"].as_str(), Some("rm -rf build/"));
    }

    #[test]
    fn parse_tool_call_parameters_fallback() {
        // §C: when `input` is absent, fall back to `parameters` then `tool_input`.
        let p1 = json!({"tool_name": "Edit", "parameters": {"path": "/x"}});
        let (t, i) = parse_tool_call(&p1);
        assert_eq!(t, "Edit");
        assert_eq!(i["path"].as_str(), Some("/x"));

        let p2 = json!({"tool_name": "Write", "tool_input": {"path": "/y"}});
        let (t, i) = parse_tool_call(&p2);
        assert_eq!(t, "Write");
        assert_eq!(i["path"].as_str(), Some("/y"));
    }

    #[test]
    fn parse_tool_call_tolerates_missing_tool_use_id() {
        // tool_use_id is optional (§C) — absence must not break parsing.
        let params = json!({"tool_name": "Read", "input": {}});
        let (tool, _input) = parse_tool_call(&params);
        assert_eq!(tool, "Read");
    }

    #[test]
    fn parse_tool_call_empty_never_panics() {
        let (tool, input) = parse_tool_call(&json!({}));
        assert_eq!(tool, "");
        assert!(input.is_null());
        let (tool, input) = parse_tool_call(&Value::Null);
        assert_eq!(tool, "");
        assert!(input.is_null());
        // Odd-typed fields never panic.
        let (tool, input) = parse_tool_call(&json!({"tool_name": 5, "input": "str"}));
        assert_eq!(tool, "");
        assert_eq!(input.as_str(), Some("str"));
    }

    // ---- build_approve_result -------------------------------------------

    fn inner_body(env: &Value) -> Value {
        let text = env["content"][0]["text"]
            .as_str()
            .expect("content[0].text is a string");
        assert_eq!(env["content"][0]["type"].as_str(), Some("text"));
        serde_json::from_str(text).expect("inner text is JSON")
    }

    #[test]
    fn build_approve_result_allow_echoes_input() {
        let input = json!({"command": "ls"});
        let env = build_approve_result("allow", Some(&input), None);
        let body = inner_body(&env);
        assert_eq!(body["behavior"].as_str(), Some("allow"));
        assert_eq!(body["updatedInput"]["command"].as_str(), Some("ls"));
    }

    #[test]
    fn build_approve_result_allow_without_input_uses_empty_object() {
        let env = build_approve_result("allow", None, None);
        let body = inner_body(&env);
        assert_eq!(body["behavior"].as_str(), Some("allow"));
        assert!(body["updatedInput"].is_object());
    }

    #[test]
    fn build_approve_result_deny() {
        let env = build_approve_result("deny", None, None);
        let body = inner_body(&env);
        assert_eq!(body["behavior"].as_str(), Some("deny"));
        assert_eq!(body["message"].as_str(), Some("denied"));
    }

    #[test]
    fn build_approve_result_deny_custom_message() {
        let env = build_approve_result("deny", None, Some("denied from phone"));
        let body = inner_body(&env);
        assert_eq!(body["behavior"].as_str(), Some("deny"));
        assert_eq!(body["message"].as_str(), Some("denied from phone"));
    }

    #[test]
    fn build_approve_result_unknown_behavior_coerces_to_deny() {
        // SECURITY: any non-"allow" behavior is coerced to deny — never emit
        // allow for an unrecognized value (deny-default, T-04-04/V4).
        for bogus in ["", "ALLOW", "yes", "approve", "true", "Allow "] {
            let env = build_approve_result(bogus, Some(&json!({"x":1})), None);
            let body = inner_body(&env);
            assert_eq!(
                body["behavior"].as_str(),
                Some("deny"),
                "behavior {bogus:?} must coerce to deny, never allow"
            );
            // The echoed input must NOT leak as updatedInput on a deny coercion.
            assert!(body["updatedInput"].is_null());
        }
    }

    // ---- rpc_response / rpc_error ---------------------------------------

    #[test]
    fn rpc_response_well_formed() {
        let r = rpc_response(json!(7), json!({"ok": true}));
        assert_eq!(r["jsonrpc"].as_str(), Some("2.0"));
        assert_eq!(r["id"].as_u64(), Some(7));
        assert_eq!(r["result"]["ok"].as_bool(), Some(true));
        assert!(r.get("error").is_none());
    }

    #[test]
    fn rpc_error_well_formed() {
        let r = rpc_error(json!(8), -32601, "method not found");
        assert_eq!(r["jsonrpc"].as_str(), Some("2.0"));
        assert_eq!(r["id"].as_u64(), Some(8));
        assert_eq!(r["error"]["code"].as_i64(), Some(-32601));
        assert_eq!(r["error"]["message"].as_str(), Some("method not found"));
        assert!(r.get("result").is_none());
    }

    #[test]
    fn rpc_response_string_id_preserved() {
        // JSON-RPC ids may be strings; echo whatever the request used.
        let r = rpc_response(json!("abc"), json!(null));
        assert_eq!(r["id"].as_str(), Some("abc"));
    }

    // ---- dispatch_rpc ---------------------------------------------------

    fn never_resolve(_t: &str, _i: &Value) -> String {
        panic!("resolve must not be called for non tools/call methods");
    }

    #[test]
    fn dispatch_initialize() {
        let req = json!({"jsonrpc":"2.0","id":1,"method":"initialize"});
        let r = dispatch_rpc(&req, never_resolve).unwrap();
        assert_eq!(r["id"].as_u64(), Some(1));
        assert_eq!(r["result"]["protocolVersion"].as_str(), Some("2024-11-05"));
        assert!(r["result"]["capabilities"]["tools"].is_object());
        assert_eq!(r["result"]["serverInfo"]["name"].as_str(), Some("baude"));
    }

    #[test]
    fn dispatch_tools_list_has_one_approve_tool() {
        let req = json!({"jsonrpc":"2.0","id":2,"method":"tools/list"});
        let r = dispatch_rpc(&req, never_resolve).unwrap();
        let tools = r["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"].as_str(), Some("approve"));
        assert!(tools[0]["inputSchema"]["properties"]["tool_name"].is_object());
    }

    #[test]
    fn dispatch_tools_call_allow_echoes_input() {
        let req = json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params": {
                "name": "approve",
                "arguments": {"tool_name":"Bash","input":{"command":"ls"}}
            }
        });
        let r = dispatch_rpc(&req, |tool, input| {
            assert_eq!(tool, "Bash");
            assert_eq!(input["command"].as_str(), Some("ls"));
            "allow".to_string()
        })
        .unwrap();
        let text = r["result"]["content"][0]["text"].as_str().unwrap();
        let inner: Value = serde_json::from_str(text).unwrap();
        assert_eq!(inner["behavior"].as_str(), Some("allow"));
        assert_eq!(inner["updatedInput"]["command"].as_str(), Some("ls"));
    }

    #[test]
    fn dispatch_tools_call_deny() {
        let req = json!({
            "jsonrpc":"2.0","id":4,"method":"tools/call",
            "params": {"name":"approve","arguments":{"tool_name":"Write","input":{}}}
        });
        let r = dispatch_rpc(&req, |_, _| "deny".to_string()).unwrap();
        let text = r["result"]["content"][0]["text"].as_str().unwrap();
        let inner: Value = serde_json::from_str(text).unwrap();
        assert_eq!(inner["behavior"].as_str(), Some("deny"));
    }

    #[test]
    fn dispatch_tools_call_unknown_decision_coerces_to_deny() {
        // SECURITY: a resolver returning anything but "allow" denies.
        let req = json!({
            "jsonrpc":"2.0","id":5,"method":"tools/call",
            "params": {"name":"approve","arguments":{"tool_name":"Bash","input":{"x":1}}}
        });
        let r = dispatch_rpc(&req, |_, _| "weird".to_string()).unwrap();
        let text = r["result"]["content"][0]["text"].as_str().unwrap();
        let inner: Value = serde_json::from_str(text).unwrap();
        assert_eq!(inner["behavior"].as_str(), Some("deny"));
    }

    #[test]
    fn dispatch_unknown_tool_is_error() {
        let req = json!({
            "jsonrpc":"2.0","id":6,"method":"tools/call",
            "params": {"name":"other","arguments":{}}
        });
        let r = dispatch_rpc(&req, |_, _| "allow".to_string()).unwrap();
        assert_eq!(r["error"]["code"].as_i64(), Some(-32602));
    }

    #[test]
    fn dispatch_unknown_method_is_error() {
        let req = json!({"jsonrpc":"2.0","id":7,"method":"frobnicate"});
        let r = dispatch_rpc(&req, never_resolve).unwrap();
        assert_eq!(r["error"]["code"].as_i64(), Some(-32601));
    }

    #[test]
    fn dispatch_notification_gets_no_reply() {
        // No id -> notification -> no response (and resolve never runs).
        let req = json!({"jsonrpc":"2.0","method":"notifications/initialized"});
        assert!(dispatch_rpc(&req, never_resolve).is_none());
    }

    // ---- run_permission_mcp (the stdio loop, with mock IO) --------------

    #[test]
    fn run_permission_mcp_full_session_over_mock_io() {
        // Two line-delimited requests on stdin: initialize then a tools/call.
        // The resolver approves; assert both replies are newline-delimited
        // (the MCP stdio framing live claude requires — §F UAT) and that the
        // approve result echoes the input.
        let stdin = concat!(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"approve\",\"arguments\":{\"tool_name\":\"Bash\",\"input\":{\"command\":\"ls\"}}}}\n",
        );
        let mut out: Vec<u8> = Vec::new();
        run_permission_mcp(stdin.as_bytes(), &mut out, |_t, _i| "allow".to_string());
        let s = String::from_utf8(out).unwrap();
        // No LSP-style Content-Length framing on output (claude drops it).
        assert!(!s.contains("Content-Length:"), "got: {s}");
        // Exactly two newline-delimited reply frames, no embedded newlines in
        // either body.
        assert_eq!(s.trim_end().matches('\n').count(), 1, "two lines: {s}");
        assert!(s.ends_with('\n'), "trailing newline: {s}");
        assert!(s.contains("\"protocolVersion\""));
        assert!(s.contains("\\\"behavior\\\":\\\"allow\\\""), "got: {s}");
    }

    #[test]
    fn run_permission_mcp_skips_notifications_and_eofs_clean() {
        // A notification (no id) yields no frame; EOF ends the loop cleanly.
        let stdin = "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n";
        let mut out: Vec<u8> = Vec::new();
        run_permission_mcp(stdin.as_bytes(), &mut out, |_, _| {
            panic!("no tools/call -> resolve unused")
        });
        assert!(out.is_empty(), "a notification must produce no reply");
    }

    #[test]
    fn run_permission_mcp_content_length_framed_input() {
        let body = r#"{"jsonrpc":"2.0","id":9,"method":"tools/list"}"#;
        let stdin = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        let mut out: Vec<u8> = Vec::new();
        run_permission_mcp(stdin.as_bytes(), &mut out, never_resolve);
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("\"approve\""), "got: {s}");
    }

    // ---- permission_url_from_event_url ----------------------------------

    #[test]
    fn permission_url_derives_from_event_url() {
        assert_eq!(
            permission_url_from_event_url("http://127.0.0.1:8642/sessions/7/event").as_deref(),
            Some("http://127.0.0.1:8642/sessions/7/permission")
        );
        // Absent/odd value -> None (the bridge then fails closed to deny).
        assert!(permission_url_from_event_url("").is_none());
        assert!(permission_url_from_event_url("http://x/sessions/7").is_none());
    }

    // ---- waiting_reason mapper (PERM-04) --------------------------------

    #[test]
    fn waiting_reason_maps_permission_input_none() {
        // A recent permission_prompt notification -> permission (regardless of
        // the waiting flag; a pending permission IS a kind of waiting).
        assert_eq!(
            waiting_reason(Some(&("permission_prompt".to_string(), 1)), true, false),
            "permission"
        );
        assert_eq!(
            waiting_reason(Some(&("permission_prompt".to_string(), 1)), false, false),
            "permission"
        );
        // Waiting with a non-permission notification -> input.
        assert_eq!(
            waiting_reason(Some(&("idle".to_string(), 1)), true, false),
            "input"
        );
        // Waiting with no notification at all -> input.
        assert_eq!(waiting_reason(None, true, false), "input");
        // Not waiting, no notification -> none.
        assert_eq!(waiting_reason(None, false, false), "none");
        // Not waiting, a stale non-permission notification -> none.
        assert_eq!(
            waiting_reason(Some(&("idle".to_string(), 1)), false, false),
            "none"
        );
    }

    #[test]
    fn waiting_reason_tolerant_permission_substring() {
        // Any notification type CONTAINING "permission" maps to permission,
        // so an odd/variant hook label still routes the distinct push.
        assert_eq!(
            waiting_reason(Some(&("needs_permission".to_string(), 1)), false, false),
            "permission"
        );
        assert_eq!(
            waiting_reason(Some(&("PERMISSION".to_string(), 1)), true, false),
            "input",
            "matching is case-sensitive substring; upper-case is not 'permission'"
        );
    }

    #[test]
    fn waiting_reason_completed_arm() {
        // three-state status: a Completed session (not waiting, not a pending
        // permission) reports "completed".
        assert_eq!(waiting_reason(None, false, true), "completed");
        // A stale non-permission notification doesn't block the completed
        // reason (mirrors idle_kind: only a permission-typed notification
        // can coexist with the idle bucket being anything other than input).
        assert_eq!(
            waiting_reason(Some(&("idle".to_string(), 1)), false, true),
            "completed"
        );
        // permission still wins over completed, defense-in-depth (idle_kind
        // guarantees this combination can't actually occur upstream).
        assert_eq!(
            waiting_reason(Some(&("permission_prompt".to_string(), 1)), false, true),
            "permission"
        );
        // waiting wins over completed too (shouldn't co-occur upstream either).
        assert_eq!(waiting_reason(None, true, true), "input");
    }
}
