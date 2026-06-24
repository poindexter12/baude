# Backlog

Captured ideas and observations not yet scheduled into a milestone phase. Triage
into ROADMAP phases when picked up.

---

## Captured 2026-06-15

### BL-01 — Sidebar "idle"/status is inaccurate (silence-only, not real working/waiting)

**Observation (user):** The sidebar's "idle" indicator only reflects "we haven't
typed anything" (the PTY-output-silence heuristic) — it does not show whether
Claude is actually *working* vs *waiting for user input*. What it should show is
"more input needed to continue" — i.e. Claude is **not working and is waiting on
the user**.

**Status:** **Directly addressed by v0.7 Phase 2 (Hook-Driven Status)** — already
planned and in progress. Hook events give exactly this signal:
`UserPromptSubmit`→working, `Stop`→waiting/done, `Notification`→needs input/permission,
with the silence heuristic demoted to a labeled fallback (`StateSource`). This
backlog entry is the real-world symptom that validates Phase 2's design.

**Action:** No new work item — verify during Phase 2 UAT that the sidebar's
working/waiting/needs-input distinction is now accurate for live sessions. If the
sidebar label wording still reads "idle" ambiguously after Phase 2, file a small
follow-up to relabel.

---

### BL-02 — Model / permission-mode (bypass) / planning mode not shown for every session

**Observation (user):** The model isn't shown for everything, and neither is the
permission mode (e.g. bypass), planning mode, etc. — they're missing for some
sessions.

**Likely cause:** These come from the v0.7 Phase 1 status-line capture / `bridge.rs`
+ `meta.rs` sources. Some sessions don't surface them — possibly sessions without a
seeded statusLine bridge, or where the session-file / bridge file hasn't been read
yet. Needs investigation: is it a capture gap (field never populated) or a render
gap (captured but not displayed in the sidebar vs the info overlay)?

**Action:** Triage as a Phase 1 follow-up / bug. Determine which sessions lack the
fields and whether it's capture vs render. Candidate for a small phase or a v0.7
audit item.

---

### BL-03 — Wire GSD phase/state into the sidebar

**Observation (user):** Since GSD is used heavily, surfacing GSD phase/state in the
sidebar would be valuable "if it makes sense."

**Context:** baude already reads GSD state from disk (noted in PROJECT.md validated
features, v0.3 — "live Claude metadata from disk … GSD state"). This item is about
giving it more prominent/consistent placement in the sidebar so active GSD work is
visible at a glance.

**Action:** New feature idea — scope a future phase (likely a later v0.7 ergonomics
item or a Tier-2+ milestone). Confirm desired placement (sidebar line vs overlay)
with user before planning.

---

## Captured 2026-06-23

### BL-04 — `BAUDE_PERMISSION_MODE=prompt` silently suppressed when `claude_cmd` already contains `--dangerously-skip-permissions`

**Observation (found during v0.7 §F UAT setup):** With `~/.config/baude/config.json`
`"claude_cmd": "claude --dangerously-skip-permissions"`, setting
`BAUDE_PERMISSION_MODE=prompt` does NOT enable prompt mode. The locked
no-double-add rule (`permission_flag_for`, T-04-02) sees the existing
`--dangerously-skip-permissions` in the base cmd and returns `""`, so skip wins.
Meanwhile `is_prompt_mode()` (which does NOT inspect the base cmd) still returns
true and seeds `.mcp.json` — a half-configured state (mcp server seeded, but
claude runs in skip mode) with NO warning. The `approve` tool is never invoked.

**Impact:** Any user who bakes `--dangerously-skip-permissions` into `claude_cmd`
(a very common, natural setting for unattended use) cannot turn on prompt mode via
the env var alone, and gets no signal explaining why. This is the inverse of WR-01
(which warns on TUI-prompt-with-no-daemon).

**Candidate fixes:** (a) when `is_prompt_mode()` but the resolved base cmd carries
`--dangerously-skip-permissions`, log/surface a warning at spawn (and skip the
.mcp.json seed for consistency); (b) in prompt mode, strip a conflicting
`--dangerously-skip-permissions` from the base cmd before appending the prompt
flag (prompt is the explicit opt-in, so it should win over a config default);
(c) document the interaction. (b) is probably the least-surprising behavior.

**Workaround (UAT):** override with `BAUDE_CLAUDE_CMD=claude` so the base cmd has
no permission flag and prompt mode engages.
