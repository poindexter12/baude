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
