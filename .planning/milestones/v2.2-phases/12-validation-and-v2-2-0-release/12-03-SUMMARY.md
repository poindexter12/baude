---
plan: 12-03
phase: 12-validation-and-v2-2-0-release
title: SHIP-03 smoke-evidence capture
status: complete
completed: 2026-09-17
plan_head_before: 8ca3d42
commits: 4
requirements: [SHIP-03]
key-files:
  created:
    - .planning/phases/12-validation-and-v2-2-0-release/12-SMOKE-EVIDENCE.md
  modified: []
---

# Plan 12-03 — SHIP-03 Smoke Evidence

## What was built

The SHIP-03 evidence artifact: a two-session (macOS + Linux) checklist of the
twelve behaviors SHIP-03 names — links detect/preview/copy/open, selection,
scrollback, mouse, Shift+Enter, ordinary Enter, and three restoration paths —
created empty, stamped against a real binary, then filled from the maintainer's
own report.

## Honest characterization of the evidence

**The macOS session is a labeled bulk attestation, not a leg-by-leg walkthrough.**
This matters and is stated plainly rather than buried:

- Exactly one leg was reported individually. Leg 1 (`ctrl+o` opens the link
  overlay) is recorded verbatim: "yeah, ctrl o works", after the maintainer
  confirmed two seeded links rendered in a shell pane.
- The other eleven macOS legs rest on a single set-level statement, "all works
  flawlessly". That sentence is recorded as what it is — an attestation covering
  the set — and is NOT paraphrased into each Observed cell as though each leg had
  been separately narrated. The artifact says so in its own header
  (`**Evidence basis:** BULK ATTESTATION`) and in an "On the strength of this
  session" note.

Three gaps are recorded and signed rather than smoothed over:

1. **Leg 2's label-vs-destination check was never narrated.** The maintainer was
   asked three times whether the overlay row shows `example.com/real-target` or
   the display text `CLICK-ME`, and answered about other things each time. This
   is precisely the property LINK-01 exists to guarantee. Automated `links::`
   tests cover it at this commit; a human did not visually confirm it.
2. **The reported mouse-click link opening is the terminal's behavior, not
   baude's.** The maintainer reported "clicking on them opened links". iTerm2
   detects and opens URLs independently of the inner application, and phase 10
   shipped keyboard-only activation with mouse activation deliberately not
   implemented. Recorded so no later reader mistakes it for leg-4 evidence.
3. **Legs 8-9 may not have been exercised.** They require a claude pane; the
   smoke instance ran under `BAUDE_WORKSPACE=smoke`, which starts with no
   sessions. Whether one was added first was not confirmed. TKEY-01/TKEY-02 have
   automated coverage, but phase 11's verification routed the real-terminal leg
   here, so that routing may still be open.

**The Linux session is deferred in full, with sign-off.** No Linux terminal was
available. All twelve Linux legs read DEFERRED. Legs 1-4 and 8-9 name
`check (ubuntu-22.04)` as their automated proxy, whose CI run URL plan 12-04
will cite once the branch is pushed; legs 5-7 and 10-12 are outer-terminal
behaviors with no automated substitute and are deferred outright. The artifact
records the absence rather than implying Linux was exercised.

## Unplanned observation worth keeping

The first launch attempt was refused:

```
baude: workspace claude is already open in another baude (pid 8413).
       Quit that instance, or run this one in another workspace: BAUDE_WORKSPACE=<name> baude
       lock: /Users/joese/.config/baude/.state-claude.json.lock
```

The maintainer's live session held the lock. That is WLOCK-01/WLOCK-03 behavior
— refusal before session operations, diagnostic pid, actionable recovery
guidance — observed incidentally against real state rather than a fixture. Plan
09-02 pinned this with tests; this is the same contract seen in the wild.

## Session provenance

| Field | Value |
|-------|-------|
| Terminal | iTerm2 3.6.9 (`TERM_PROGRAM=iTerm.app`, `LC_TERMINAL=iTerm2 3.6.9`) |
| OS | macOS 27.0, arm64 |
| Binary | `target/release/baude`, reported `baude 2.1.5` |
| Commit | `e94aeed` (dirty — GSD orchestrator state only; no source differs) |
| Observer | Joe Seymour |

The binary reporting `2.1.5` rather than `2.2.0` is expected: release-please
cuts `2.2.0` from the merged branch, so no pre-release commit can report it.

## Verification

| Gate | Result |
|------|--------|
| Structure: 24 leg rows, 2 session blocks, 2 deferral closers | exit 0 |
| Pre-fill gate at template creation (every Observed cell empty) | exit 0 (Tasks 1, 2) |
| Completeness: no unfilled leg; DEFERRED legs carry `Signed off by <name> on <date>` | exit 0 |

Exit codes captured directly, never piped.

## Deviations

**The checkpoint was resolved by the orchestrator rather than an executor.** Task
3 is a `checkpoint:human-action`; the maintainer's words exist only in the
orchestrator conversation, so transcription happened there. A dispatched
continuation executor then stalled (no progress for 600s) before writing
anything — no commits, no partial files — and the close-out was completed
inline. Nothing was salvaged or duplicated.

**The sign-off line format was corrected.** The deferral entries were first
written as `— Joe Seymour, 2026-09-16`, which the Task 3 gate's
`Signed off by .* on YYYY-MM-DD` pattern does not match. Rewritten to the
canonical form; the gate then passed. The gate catching this is the mechanism
working.

## Requirements

SHIP-03 is delivered in the sense the plan defines: the evidence artifact exists,
is complete (no unaccounted leg), and is honest about its own strength. A reader
deciding whether to trust it has everything needed to judge — including the three
signed gaps and the Linux deferral.
