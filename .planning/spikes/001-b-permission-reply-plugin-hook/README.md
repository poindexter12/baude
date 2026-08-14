---
spike: 001b
idea: opencode-backend
name: permission-reply-plugin-hook
type: comparison
validates: "Given a .opencode/plugins/ plugin registering permission.ask, when the agent requests a bash tool call, then a deferred async decision (allow/deny resolved seconds later) is honored"
verdict: INVALIDATED
related: [001a-permission-reply-server-api]
tags: [opencode, permissions, plugin]
---

# Spike 001b: Remote Permission Reply via Plugin `permission.ask` Hook

## What This Validates

Whether a plugin in `.opencode/plugins/` registering the documented
`permission.ask(input, output)` hook can hold the decision open (awaiting a simulated
remote approval that arrives seconds later) and have a late `allow`/`deny` honored —
an alternative to the server-API reply flow of spike 001a.

## Research

- `@opencode-ai/plugin` types declare `"permission.ask"?: (input: Permission, output:
  {status: "ask" | "deny" | "allow"}) => Promise<void>`.
- But the current permission service (`packages/opencode/src/permission/index.ts`,
  Effect-based) contains **no plugin trigger at all**: it evaluates config rules, then
  publishes `Event.Asked` and awaits an HTTP reply. The hook string still appears in the
  1.18.16 binary only in docs/legacy text.

## How to Run

```
node run-spike.mjs
```

Sandbox gets `permission: { bash: "ask" }` plus `plugin/deferred-permission.js` copied into
`.opencode/plugins/`. The plugin logs which surfaces fire (`permission.ask` hook, generic
`event` hook) to `plugin-log.jsonl`; the driver holds 3s, then resolves via the plugin's
deferred decision if the hook fired, else via the HTTP endpoint (recorded as fallback).

## Observability

`events.jsonl` (SSE + API), `plugin-log.jsonl` (in-process plugin sightings),
`result.json` (structured verdict).

## Investigation Trail

1. First run: plugin factory loaded (confirmed in `plugin-log.jsonl`), `permission.asked`
   appeared on SSE — but `permission.ask` never fired. 60s deferred wait never started.
2. Checked the 1.18.16 binary strings and current source: the new permission service has no
   plugin invocation in its ask path. The hook is dead code on this version.
3. Variant run adding a generic `event` hook: plugin DOES receive `permission.asked` (and
   `permission.replied`) through the event bus, both rounds. So a plugin can *observe*
   pending permissions in-process, but to *answer* one it would have to call the same
   HTTP reply endpoint as any external client.

## Results

**INVALIDATED** on opencode 1.18.16:

- `askHookFired=false` in both rounds — the documented `permission.ask` hook never runs.
- `eventHookFired=true` in both rounds — the generic `event` hook receives
  `permission.asked`.
- Behavior itself (held pending, approve executes, reject doesn't) was re-confirmed via the
  HTTP fallback, matching 001a.

**Head-to-head verdict:** the server API (001a) wins outright. A plugin adds a JS artifact
baude would have to seed into every project (the same settings-seeding burden the Claude
backend carries with `.claude/settings.local.json`) and still ends up calling the identical
HTTP endpoint — while depending on a hook surface that has already rotted once. The baude
opencode driver should use SSE `/event` + `POST /session/{id}/permissions/{permissionID}`
directly and skip plugins entirely.
