---
phase: 12-validation-and-v2-2-0-release
plan: 02
subsystem: docs
tags: [readme, ship-02, link-hints, mouse-capture, alternate-screen, workspace-lock]

# Dependency graph
requires:
  - phase: 10-link-activation
    provides: the ctrl+o link-hint gesture, LINK-04/05/07/08 semantics, and the overlay footer string this README now quotes
  - phase: 11-terminal-keys
    provides: the Shift+Enter kitty negotiation and its fallback prose, which this plan preserved and broadened around
  - phase: 08-workspace-locking
    provides: WLOCK-01..04, the single-writer state lock and its pid-bearing refusal diagnostic, shipped in v2.1.2-v2.1.5 with no user docs
provides:
  - README `ctrl+o` Keys row plus a `### Link hints` subsection quoting baude/src/ui.rs:2159 and :2212
  - README `### Mouse, selection, and scrollback` documenting unconditional mouse capture, baude's own pane selection, wheel routing, and the alternate screen's empty host scrollback
  - README `### Tested terminals` restating the verified-terminal list across links and mouse, not Shift+Enter alone
  - README `### When a workspace is already open` carrying the verbatim lock refusal, the lock path, pid semantics, the never-delete rule, and safe recovery
  - Eleven grep acceptance criteria plus three paragraph-scoped awk gates pinning the prose to source tokens
affects: [12-03-smoke-evidence, 12-04-release, release-notes]

# Actuals (#2632)
actuals:
  tokens: 1921
  tasks: 3
  commits: 4
  plan_head_before: 57e66cd6cb4469d2700fbee3223c90318adc1c62

tech-stack:
  added: []
  patterns:
    - "Documentation gates grep user-facing tokens pulled from source (a chord, an overlay footer, a verbatim diagnostic, a path shape) rather than asserting a heading exists — so the criterion breaks when the code drifts (D-06)"
    - "Paragraph-scoped awk gates (RS=\"\") assert that a problem and its workaround co-occur in one paragraph, which a whole-file grep cannot express"

key-files:
  created: []
  modified:
    - README.md

key-decisions:
  - "Moved the tested-terminal list out of the Shift+Enter negotiation paragraph into its own `### Tested terminals` subsection, so one statement covers all three negotiated gestures instead of scoping verification to the newline modifier"
  - "Documented baude's own pane selection (click-drag copies to the clipboard on release) alongside the suppression of native drag-select — the research note framed capture only as suppression, which would have read as a feature loss"
  - "Phrased the native-selection workaround as 'your terminal's override modifier' with Shift and Option named as common examples, never as a promised key, because the binding is the terminal's choice"
  - "Stated the daemon's lock behavior precisely: bauded does not claim the lock at startup, so it surfaces the same diagnostic at its first state save"

patterns-established:
  - "README quotes shipped strings verbatim inside fenced blocks so a user can paste the message they saw into a search and land on the section"

requirements-completed: [SHIP-02]

coverage:
  - id: D1
    description: "A user can find the ctrl+o link gesture in the Keys table and read how to preview, copy, and open a link, what is activatable, and that the chord no longer reaches the child"
    requirement: "SHIP-02"
    verification:
      - kind: other
        ref: "grep -n 'ctrl+o' README.md (3 hits) && grep -niE 'link hints' && grep -niE 'c/y|copies' && grep -niE 'inspect|preview|actual destination'"
        status: pass
      - kind: other
        ref: "grep -n '^#' README.md — `### Link hints` at :170, between `### Shift+Enter newlines` (:149) and `## Status icons` (:232)"
        status: pass
    human_judgment: false
  - id: D2
    description: "A user can read why plain drag-to-select does not work while baude runs, how to get native selection back, and why the host terminal's scrollback stays empty"
    requirement: "SHIP-02"
    verification:
      - kind: other
        ref: "grep -niE 'mouse' README.md && grep -niE 'scrollback|alternate screen' README.md"
        status: pass
      - kind: other
        ref: "awk 'BEGIN{RS=\"\"} /[Mm]odifier/ && /[Dd]rag|[Ss]elect/ {f=1} END{exit !f}' README.md — exit 0"
        status: pass
    human_judgment: false
  - id: D3
    description: "The tested-terminal statement covers the negotiated gestures as a set (links and mouse), not Shift+Enter alone, and the Shift+Enter setup/fallback prose survived the edit"
    requirement: "SHIP-02"
    verification:
      - kind: other
        ref: "awk 'BEGIN{RS=\"\"} /Ghostty|WezTerm|Alacritty/ && /[Ll]ink|[Mm]ouse/ {f=1} END{exit !f}' README.md — exit 0"
        status: pass
      - kind: other
        ref: "grep -niE 'shift\\+enter' README.md && grep -niE 'terminal-setup|ctrl\\+j|extended-keys' README.md"
        status: pass
    human_judgment: false
  - id: D4
    description: "A user who hits the workspace-lock refusal can find the verbatim message, the lock path, what the pid means, and recovery that cannot corrupt a state file"
    requirement: "SHIP-02"
    verification:
      - kind: other
        ref: "grep -niE 'already open in another baude' && grep -nE '\\.state-.*\\.lock' && grep -niE 'BAUDE_WORKSPACE=' && grep -niE 'never delete the lock file' && grep -niE 'ps -p|lsof' README.md"
        status: pass
      - kind: other
        ref: "grep -nE '\\brm\\b.*\\.lock' README.md — 0 hits (T-12-04 negative gate)"
        status: pass
    human_judgment: false
  - id: D5
    description: "The documented behavior matches what the binaries actually print and do — the overlay footer, the help row, the lock diagnostic, and the lock path shape are quoted from source rather than paraphrased"
    verification: []
    human_judgment: true
    rationale: "Grep pins the tokens, but whether the prose is an accurate account of the runtime experience (does the overlay really look like that, does drag-select really behave as described) is only settled by the 12-03 smoke run in a real terminal."

# Metrics
duration: 7 min
completed: 2026-09-16
status: complete
---

# Phase 12 Plan 02: README SHIP-02 Gap-Fill Summary

**README.md now carries all four SHIP-02 items — the `ctrl+o` link-hint overlay quoted from `ui.rs`, unconditional mouse capture with the terminal override-modifier workaround, the alternate screen's empty host scrollback, a tested-terminal statement covering links and mouse, and a workspace-lock recovery section that never tells you to delete the lock.**

## Performance

- **Duration:** 7 min
- **Started:** 2026-09-16T17:49:58Z
- **Completed:** 2026-09-16T17:56:39Z
- **Tasks:** 3
- **Files modified:** 1 (README.md, +118 / -2)

## Accomplishments

- **Link gesture documented for the first time in README.md.** Phase 10 put `ctrl+o` in the in-app help overlay and the vendored fork's README only; `README.md` had zero hits. Added a Keys-table row phrased to match `baude/src/ui.rs:2212` verbatim, plus a `### Link hints` subsection that quotes the overlay footer from `baude/src/ui.rs:2159` (`enter opens · c/y copies · j/k moves · esc closes — N links`) and covers destination-not-label preview, middle truncation with scheme and host preserved, the ten-row cap, LINK-07 fail-closed validation, LINK-04 explicit activation, and LINK-08 non-fatal opener failure.
- **The two behaviors that would have read as bugs during the smoke run are now documented ahead of it.** `### Mouse, selection, and scrollback` states that mouse capture is unconditional (`baude/src/main.rs:427-433`), that the terminal's own drag-to-select is therefore suppressed, that baude substitutes its own pane selection which copies on release, and that the terminal's override modifier is the route back to native selection. It also states that baude runs on the alternate screen, so session output never enters the host terminal's scrollback, and names what the single restore path gives back on exit (`baude/src/main.rs:118-128`).
- **The tested-terminal statement was broadened out of Shift+Enter scope.** The list (Ghostty, kitty, iTerm2, WezTerm, foot, Alacritty) moved from the kitty-negotiation paragraph into its own `### Tested terminals` subsection naming all three negotiated gestures, with an honest definition of "verified" that points at the release's smoke evidence for exact terminal identities.
- **Workspace-lock recovery written from scratch** — the one SHIP-02 item no prior phase touched, for behavior live in released binaries since v2.1.2. Quotes both forms of the refusal verbatim, names the lock path and its derivation, explains that an absent pid means a failed best-effort stamp rather than a stale lock, and carries the never-delete rule with the two-writers explanation.
- **Eleven grep criteria plus three paragraph-scoped awk gates**, every one of which measured red against the pre-task tree except the two deliberately guarding SHIP-02's already-shipped fourth item from regression.

## Task Commits

1. **Task 1: Document the link gesture — Keys row plus a Link hints section** - `44625c5` (docs)
2. **Task 2: Document mouse capture, text selection, scrollback, and tested terminal support** - `78aa6fe` (docs)
3. **Task 3: Add the workspace-lock recovery section** - `456904a` (docs)

**Plan metadata:** the `docs(12-02): complete README SHIP-02 gap-fill plan` commit carrying this SUMMARY, STATE.md, ROADMAP.md, and REQUIREMENTS.md. It is not cited by hash here because a commit cannot contain its own hash.

`actuals.commits: 4` is `git rev-list --count 57e66cd..HEAD` measured with the metadata commit included, so `/gsd-verify-work` reproduces the same number with the same instrument: the three task commits above plus that metadata commit.

## Files Created/Modified

- `README.md` — four new subsections and one restructured statement:
  - `## Keys` table: `ctrl+o | anywhere | link hints (inspect/copy/open urls)` (:120)
  - `### Link hints` (:170)
  - `### Mouse, selection, and scrollback` (:198)
  - `### Tested terminals` (:222) — list relocated out of the Shift+Enter negotiation paragraph
  - `### When a workspace is already open` (:473), at the end of `## Workspaces`, before `## opencode backend`

## SHIP-02 Acceptance Record (D-06)

The plan-level sweep, run as one `&&` chain at plan close with its exit observed directly:

```
grep -n 'ctrl+o' README.md && grep -niE 'link hints' README.md && grep -niE 'c/y|copies' README.md && grep -niE 'inspect|preview|actual destination' README.md && grep -niE 'mouse' README.md && grep -niE 'scrollback|alternate screen' README.md && grep -niE 'shift\+enter' README.md && grep -niE 'terminal-setup|ctrl\+j|extended-keys' README.md && grep -niE 'already open in another baude' README.md && grep -nE '\.state-.*\.lock' README.md && grep -niE 'never delete the lock file' README.md
```

**exit 0.**

| Gate | Before | After |
|------|--------|-------|
| `grep -c 'ctrl+o'` | 0 | 3 |
| `grep -ciE 'link hints'` | 0 | 2 |
| `grep -ciE 'c/y\|copies'` | 0 | ≥1 |
| `grep -ciE 'inspect\|preview\|actual destination'` | 0 | ≥1 |
| `grep -ciE 'mouse'` | 0 | 5 |
| `grep -ciE 'scrollback\|alternate screen'` | 0 | 4 |
| `grep -ciE 'shift\+enter'` | ≥1 (regression guard) | ≥1 |
| `grep -ciE 'terminal-setup\|ctrl\+j\|extended-keys'` | ≥1 (regression guard) | 2 |
| `grep -ciE 'already open in another baude'` | 0 | 2 |
| `grep -cE '\.state-.*\.lock'` | 0 | 4 |
| `grep -ciE 'never delete the lock file'` | 0 | 1 |
| `awk RS="" /[Mm]odifier/ && /[Dd]rag\|[Ss]elect/` | exit 1 | exit 0 |
| `awk RS="" /Ghostty\|WezTerm\|Alacritty/ && /[Ll]ink\|[Mm]ouse/` | exit 1 | exit 0 |
| `grep -cE '\brm\b.*\.lock'` (must be 0) | 0 | 0 |

Heading order confirmed by `grep -n '^#' README.md`: `## Keys` (108) → `### Shift+Enter newlines` (149) → `### Link hints` (170) → `### Mouse, selection, and scrollback` (198) → `### Tested terminals` (222) → `## Status icons` (232); and `## Workspaces` (436) → `### When a workspace is already open` (473) → `## opencode backend` (525).

## Decisions Made

- **Relocated the terminal list rather than duplicating it.** The plan asked for the tested-terminal statement to stop being Shift+Enter-scoped. Restating it in place would have left the list inside a paragraph about the kitty keyboard protocol, which is what the awk gate was written to catch. Moving it to `### Tested terminals` gives one statement for all three gestures and leaves the Shift+Enter negotiation paragraph pointing at it by anchor link.
- **Documented baude's own selection, not just the suppression.** `baude/src/app.rs:5446-5519` shows click-drag builds a pane-scoped selection and the release copies it to the clipboard (`pbcopy` / `wl-copy` / `xclip`), surfacing `copy failed:` rather than a silent no-op. Describing only the suppression of native drag-select — the framing in RESEARCH — would have told users a capability was lost when it was actually replaced.
- **Described the wheel's two behaviors.** The scroll goes to baude's own pane scrollback unless the child has its own mouse mode on, in which case it is forwarded (`baude/src/app.rs:5402-5444`). Without this, "baude consumes mouse events" implies full-screen child programs stop scrolling, which is not what happens.
- **Named the daemon's contention point precisely.** `bauded/src/manager.rs:3031-3035` documents that bauded never claims the workspace lock at startup — its contention surface is the first save. The one-line note says so rather than implying a launch-time refusal the daemon does not perform.

## Deviations from Plan

None — plan executed exactly as written. The three additions above are elaborations within the prose the plan directed, not departures from it; no deviation rule was invoked and no gate was relaxed.

## Issues Encountered

None. Every gate measured red before its task and green after, in the direction the plan predicted.

## Threat Flags

None. The plan's own threat register is fully addressed:

- **T-12-04 (Tampering, lock-recovery prose):** mitigated. The never-delete rule is stated explicitly with its two-writers consequence, and the negative gate `grep -nE '\brm\b.*\.lock' README.md` returns zero hits.
- **T-12-05 (Repudiation, doc drift):** mitigated. Every criterion greps a token lifted from source — the chord, the overlay footer, the verbatim diagnostic, the lock path shape — so a refactor that changes the behavior breaks the criterion.
- **T-12-06 (Information Disclosure):** as accepted. The only paths introduced are `~/.config/baude/.state-claude.json.lock` and a `/Users/you/...` placeholder inside the quoted diagnostic; no real user path, hostname, or token appears.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- SHIP-02 is closed for README content. 12-03's smoke evidence can now cite README sections by name for the link, mouse, selection, scrollback, and Shift+Enter legs rather than describing behavior from scratch.
- `### Tested terminals` promises that each release records exact terminal identities and OS versions in its smoke evidence. 12-03 must actually populate `12-SMOKE-EVIDENCE.md` with observed identities, or that sentence becomes a claim the repo does not keep.
- Nothing pushed. The branch carries three new doc commits on top of `57e66cd`; push and CI remain 12-04's work.

## Self-Check: PASSED

- `README.md` exists on disk with all five new/modified sections at the line numbers recorded above.
- All three task commits verified present: `git log --oneline --all | grep` found `44625c5`, `78aa6fe`, `456904a`.
- `git rev-list --count 57e66cd..HEAD` = 4 (three task commits plus the metadata commit), matching `actuals.commits`.
- Plan-level 11-grep sweep: exit 0. Both paragraph-scoped awk gates: exit 0. Lock-deletion negative gate: 0 hits.

---
*Phase: 12-validation-and-v2-2-0-release*
*Completed: 2026-09-16*
