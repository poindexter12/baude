# Design: Three-state session status mirroring Claude's agent view

Status: **Proposed** (design only — not implemented)
Author: design pass, 2026-07-02
Assumed decisions (override if wrong):
- Notification policy: **distinct "finished" push** on the Working→Completed edge.
- Completed label/icon: **`✓` green "completed"** (mirrors Claude Code agent-view wording).

## Problem

baude collapses every idle session into one `Waiting` state that flashes yellow
and demands attention (`baude/src/ui.rs:288`). Two very different situations map
to it:

1. **Claude finished its turn cleanly** (`Stop` event) — your move, calm, not urgent.
2. **Claude is blocked on you** (`Notification` / permission prompt) — urgent.

Claude Code's own agent view keeps these distinct as **Working / Waiting-for-input
/ Completed**. baude's core value ("which session needs you *next*") is blunted
when a done-and-calm session shouts as loudly as a genuinely-blocked one.

The distinguishing signal is already captured — `Stop` vs `Notification` in the
event stream (`baude-core/src/meta.rs:441-455`). This feature stops discarding
information we already have; it adds no new data source.

## State model

| State | Meaning | Signal | Icon | Color | Flash | Timer |
|-------|---------|--------|------|-------|-------|-------|
| **Working** | thinking/streaming | `hook_status` busy, or output within `BUSY_WINDOW_MS` | `◐` spinner | Blue | no | — |
| **Needs input** | asked a question / needs permission | last terminal event = `Notification` | `●` | Yellow | yes | wait timer (urgent) |
| **Completed** | turn ended cleanly, your move | last terminal event = `Stop` | `✓` | Green (dim) | no | subtle "done Xm ago" |
| **Exited** | process died | exited | `✗` | Gray | no | — |

"Needs input" is today's `Waiting`, correctly scoped to genuine prompts.
**Completed is the only new state.** It recolors *in place* — the stable sidebar
order (sort-by-name at `baude/src/app.rs:84`, not by status) is preserved, exactly
as today's flash-in-place behavior.

## Derivation — do not touch the precedence engine

`decide_status` (`baude-core/src/session.rs:145`) already answers busy-vs-idle
correctly from the `Hook > SessionFile > Silence` precedence tiers, and is
heavily unit-tested. Leave it untouched. Refine only the *idle* bucket into
`Completed` vs `NeedsInput` with a new pure classifier:

```
idle_kind(last_stop_ts, last_notification) ->
    NeedsInput  if last_notification newer than last_stop  (or a "permission" type)
    Completed   if last_stop is the most recent terminal event
    NeedsInput  if neither is known        // fail-safe (see below)
```

- `last_notification` already exists (`baude-core/src/meta.rs:130`). Add a
  symmetric `last_stop: Option<u64>` captured in the same `match`
  (`baude-core/src/meta.rs:443`).
- Compose with `waiting_reason` (`baude-core/src/permission.rs:568`):
  `permission`/`input` become sub-reasons of Needs-input; add a `completed`
  reason when idle-kind is Completed.

### Fail-safe direction

When idle with **no hook history** (pure silence/session-file fallback — e.g. a
session that never fired hooks), classify as **Needs input**, not Completed.
Rationale matches the codebase's fail-closed posture (permissions deny on
unknown): a false "needs input" costs one extra flash; a false "completed" could
make you *miss a session blocked on you*. Never trade away the attention
guarantee.

## Surfaces to change

| File | Change |
|------|--------|
| `baude-core/src/session.rs` | Add `Completed` to `Status`; add `idle_kind`; order `Completed` between `Waiting` and `Exited` in the `Ord` derive (counts only, not sidebar sort). |
| `baude-core/src/meta.rs` | Capture `last_stop` alongside `last_notification` in the event `match`. |
| `baude-core/src/permission.rs` | Extend `waiting_reason` to return `"completed"`. |
| `baude/src/ui.rs` | `session_row` icon/color/flash for `Completed` (~286); status pill (~520); legend (~1258); count bar add "done" (~755); `remote_status` parse (~207). |
| `baude/src/app.rs` | `status_counts` → `(needs_input, working, completed)` (~313). |
| `bauded/src/manager.rs` | `status_str` → add `"completed"` (~153). |
| `bauded/src/notify.rs` | Notification policy — see below. |
| `baude/src/remote.rs` + PWA | Wire `"completed"` status string; PWA renders calm/green state. |

## Notification policy (assumed: distinct "finished" push)

Today a `Stop` → `Waiting` → after 10s debounce fires "X is waiting for you"
(`bauded/src/notify.rs:84`), so every completed turn nags. Under the new model:

- **Needs-input** and **permission** keep their urgent pushes unchanged.
- **Completed** fires a distinct, gentler "X finished" push **once** on the
  Working→Completed edge (tracked like `notified_permission`/`notified_waiting`).

Strictly clearer than today (done vs blocked distinguishable on the phone) and
mirrors the agent view. This is a behavior change to a shipped push path —
confirm before implementing. Alternatives: no push on Completed (quietest); keep
today's "waiting for you" (least change, keeps ambiguity).

## Interactions / edge cases

- **Auto-archive** (`session.rs:87`): both Completed and Needs-input still park
  after `AUTO_ARCHIVE_IDLE_MS`. Completed shows a dim "done Xm ago" instead of
  the yellow wait timer.
- **Wire back-compat**: adding a `"completed"` status string is additive; older
  PWA/remote clients fall through `remote_status`'s `_ => Waiting` arm and simply
  render it as needs-input (safe degradation).
- **`Ord` consumers**: audit any `Status` comparisons before reordering the enum;
  currently only counts use it, not sidebar order.

## Test plan (sketch)

- `idle_kind`: Stop-newer→Completed; Notification-newer→NeedsInput;
  permission-type→NeedsInput regardless; neither-known→NeedsInput (fail-safe).
- `waiting_reason`: new `completed` arm; permission still wins.
- `notify`: Working→Completed edge fires exactly one "finished"; no "waiting for
  you" for completed; permission/needs-input unchanged.
- `meta`: `last_stop` captured and updated by later Stop events; Notification
  after Stop flips idle-kind back to NeedsInput.
- Precedence (`decide_status`) tests unchanged — proves no regression.
