---
spike: 001a
idea: opencode-backend
name: permission-reply-server-api
type: comparison
validates: "Given `permission: bash=ask` and a running `opencode serve`, when the agent requests a bash tool call, then `permission.asked` arrives on /event and POST /session/{id}/permissions/{permissionID} with once/reject resolves it"
verdict: VALIDATED
related: [001b-permission-reply-plugin-hook]
tags: [opencode, permissions, sse, server-api]
---

# Spike 001a: Remote Permission Reply via opencode Server API

## What This Validates

Given a sandbox project with `permission: { bash: "ask" }` and `opencode serve` running,
when the agent requests a bash tool call, then the pending permission is announced on the
`/event` SSE stream and an external HTTP client can approve (`once`) or reject (`reject`)
it — with the tool provably executing only on approval. This is the opencode equivalent of
baude's `--permission-prompt-tool mcp__baude__approve` prompt mode.

## Research

- Verified against opencode **1.18.16** (mise install), authenticated via github-copilot
  (`gpt-5.4` model from Joe's global config).
- Contract sources: bundled `@opencode-ai/sdk` type definitions + live behavior + the
  current permission service source (`packages/opencode/src/permission/index.ts` on
  sst/opencode): `ask()` parks the request in a pending map behind an Effect `Deferred`,
  publishes `Event.Asked`, and blocks the tool until `reply()` resolves or fails it.
- Reply endpoint: `POST /session/{id}/permissions/{permissionID}` body
  `{"response": "once" | "always" | "reject"}` → `200 true`.

## How to Run

```
node run-spike.mjs
```

Spawns `opencode serve --port 14711` in `sandbox/`, runs an approve round and a reject
round, writes `result.json` (verdict) and `events.jsonl` (forensic log of every SSE event
and API call with ISO timestamps). Exit 0 = VALIDATED.

## What to Expect

- Approve round: `permission.asked` → 3s hold with no execution → reply `once` → 200 →
  `permission.replied` → `sandbox/proof-approve.txt` exists → `session.idle`.
- Reject round: same shape with reply `reject`; `sandbox/proof-reject.txt` must NOT exist
  and the session still reaches `session.idle` (agent reports it couldn't run the command).

## Observability

`events.jsonl` — every SSE event and API call, timestamped. `result.json` — structured
verdict with the captured permission object and event counts. `serve.log` — raw server
output.

## Investigation Trail

1. First run timed out waiting for `permission.updated` — the event name the bundled SDK
   types declare. The live server actually emits **`permission.asked`**. The permission WAS
   held pending for the full 3-minute timeout (heartbeats only, no execution), already
   proving the deferred-approval property.
2. Second run: approve path fully worked, but my `permission.replied` matcher missed —
   live properties are `{sessionID, requestID, reply}`, not the SDK's
   `{permissionID, response}`. Two schema drifts between bundled SDK types and the live
   server in one spike.
3. Third run: both rounds green.

## Results

**VALIDATED.** Both paths behaved exactly as baude's prompt mode needs:

- `replyAccepted=true`, `heldWithoutExecuting=true` on both rounds — the agent is genuinely
  blocked (arbitrarily long; observed 3+ minutes) until the external reply arrives.
- Approve: proof file written with expected content after the reply, session went idle.
- Reject: proof file never created; session finished gracefully.
- The `permission.asked` payload carries everything a phone-approval UI needs:
  `id`, `sessionID`, `permission` ("bash"), `patterns` (the exact command), `metadata.command`,
  `always` patterns (what "always allow" would whitelist, e.g. `echo *`), and the
  originating `tool.messageID`/`callID`.
- Bonus findings from source: `reject` also auto-rejects all other pending permissions in
  the same session; `always` persists an allow rule and auto-approves matching pending
  requests; a reject reply can carry a feedback `message` (CorrectedError) — useful later
  for "deny with instructions".

**Surprise / risk to carry into the build:** the bundled SDK types lag the live server
(`permission.updated`→`permission.asked`, `permissionID/response`→`requestID/reply`).
A baude opencode driver must be written against observed wire behavior per pinned opencode
version, with schema assumptions documented the way `hook.rs`/`bridge.rs` already do for
Claude Code.
