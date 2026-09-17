---
phase: 10-clickable-terminal-links
plan: "04"
subsystem: ui
tags: [link-hints, overlay, clipboard, opener, remote-attach, tdd, ratatui]

requires:
  - phase: 10-clickable-terminal-links
    provides: "10-01: Modal::LinkHints + ctrl+o chord + open_link_hints bracket + activate_link/spawn_opener seam; 10-02: link-faithful fork grid + snapshot parity; 10-03: collect_bare_links + hardened validate_http_url (collect_links signature frozen)"
provides:
  - "Completed LinkHints overlay: bottom-anchored [a] destination list, selected-row bold/reversed, viewport scrolling past 26 entries, width-safe middle truncation (scheme+host always visible)"
  - "c/y destination copy through an injected sink (production: existing clipboard path — no new spawn), j/k/arrow navigation, a-z hint-letter jump"
  - "Gesture call site finalized: render-parity remote-attach filter (remote_id + liveness), (row, start_col) top-to-bottom ordering across both detection passes"
  - "LINK-04 swallow proof at handle_key granularity via a deterministic RemoteAttach::test_stub channel spy; LINK-08 failure surface regression-tested"
affects: [12-release-gate]

actuals:
  tokens: 10605
  tasks: 3
  commits: 5
plan_head_before: b829a1248974b27e3282869695422c975cdc6144

tech-stack:
  added: []
  patterns:
    - "dual injected sinks (open + copy) on the modal key handler — every side effect test-reachable, no test touches a real spawn"
    - "RemoteAttach::test_stub: socketless attach whose input channel the test holds — absence-of-forwarding is a deterministic assertion, not a sleep"

key-files:
  created:
    - .planning/phases/10-clickable-terminal-links/10-04-t1-red-evidence.json
    - .planning/phases/10-clickable-terminal-links/10-04-t2-red-evidence.json
  modified:
    - baude/src/app.rs
    - baude/src/ui.rs
    - baude/src/links.rs
    - baude/src/remote.rs

key-decisions:
  - "Action keys c/y/j/k shadow their hint letters; shadowed rows stay reachable via j/k navigation (a-z labels kept per CONTEXT discretion, priority given to the locked per-link actions)"
  - "display_truncated_width anchors on url[..Position::BeforePath] so scheme://host survives ANY width budget — the one case display may exceed the budget rather than hide the origin (T-10-15)"
  - "open_link_hints filters the attach on remote_id + liveness — identical predicate to ui.rs draw_remote_content, so the chord always reads the parser the render path draws"
  - "Links sort by (row, start_col) at the call site so hint letters label links in visual order regardless of detection pass"

patterns-established:
  - "Test-stub constructors for socket-backed structs live cfg(test) beside the struct; the test holds the channel receiver"

requirements-completed: [LINK-04, LINK-05, LINK-06, LINK-08]

coverage:
  - id: D1
    description: "c/y copy the selected link's full normalized destination without opening it; modal closes; 'copied' message set (LINK-06)"
    requirement: LINK-06
    verification:
      - kind: unit
        ref: "baude/src/app.rs#app::link_hints::c_copies_full_destination_without_opening"
        status: pass
      - kind: unit
        ref: "baude/src/app.rs#app::link_hints::y_copies_selected_entry"
        status: pass
    human_judgment: false
  - id: D2
    description: "Overlay is fully navigable (j/k/arrows with clamping, a-z letter jump, Enter opens selected only) and display truncation is render-only middle-ellipsis that never drops scheme or host (LINK-05)"
    requirement: LINK-05
    verification:
      - kind: unit
        ref: "baude/src/app.rs#app::link_hints (navigation_moves_selected_with_bounds, hint_letter_jumps_selection, enter_opens_selected_entry_only, 3 truncation tests)"
        status: pass
    human_judgment: false
  - id: D3
    description: "Chord collects at the pane's scroll offset inside a restored set_scrollback bracket (including the empty path), merges both detection passes ordered top-to-bottom, resolves only the render path's attach parser, and surfaces zero links as a message"
    requirement: LINK-04
    verification:
      - kind: integration
        ref: "baude/src/app.rs#app::link_hints (chord_at_scrolled_offset_collects_viewed_rows_and_restores_bracket, chord_collects_both_passes_ordered_top_to_bottom, chord_with_zero_links_sets_message_and_restores_bracket, chord_ignores_attach_for_a_different_remote)"
        status: pass
    human_judgment: false
  - id: D4
    description: "While the overlay is open, every key driven through handle_key is handled or swallowed by the modal path — no byte reaches the child (proven against a live channel spy, with a modal-closed forwarding control); pre-existing selection/drag-copy/clipboard tests pass unmodified"
    requirement: LINK-04
    verification:
      - kind: integration
        ref: "baude/src/app.rs#app::link_hints::modal_open_swallows_every_key_from_the_child"
        status: pass
      - kind: other
        ref: "cargo test --workspace --locked (140 baude + 357 bauded + 93 baude-core + fork suites, 0 failed)"
        status: pass
    human_judgment: false
  - id: D5
    description: "Opener spawn error surfaces exactly one set_message naming the failure with the session demonstrably still processing events; Ok path announces with the truncated display form while the opener received the full URL (LINK-08)"
    requirement: LINK-08
    verification:
      - kind: unit
        ref: "baude/src/app.rs#app::link_open::opener_error_surfaces_one_warning_and_session_survives"
        status: pass
      - kind: unit
        ref: "baude/src/app.rs#app::link_open::opener_ok_sets_opening_message_with_display_form"
        status: pass
    human_judgment: false
  - id: D6
    description: "Real-terminal dogfood: ctrl+o over live OSC8 + bare output lists destinations, c copies to the actual clipboard, Enter opens the real browser, selection/drag still work"
    verification: []
    human_judgment: true
    rationale: "Real browser launch, pbcopy clipboard contents, and terminal-emulator interaction cannot be asserted in unit tests; the plan marks this leg optional dogfood feeding SHIP-03 (RESEARCH Validation Architecture), not this plan's gate"

duration: 23min
completed: 2026-09-16
status: complete
---

# Phase 10 Plan 04: Overlay Completion, Copy Sink, Gesture Call Sites Summary

**Hint mode finished: bottom-anchored a-z overlay with width-safe origin-preserving truncation, c/y copy through the existing clipboard path via an injected sink, render-parity remote-attach resolution with (row,col)-ordered dual-pass collection, and LINK-04/LINK-08 guarantees regression-tested at handle_key granularity against a deterministic socketless attach stub**

## Performance

- **Duration:** 23 min
- **Started:** 2026-09-16T08:08:48Z
- **Completed:** 2026-09-16T08:32:06Z
- **Tasks:** 3 (2 TDD, 1 auto)
- **Files modified:** 4 source + 2 RED evidence records

## Accomplishments

- LINK-06 shipped: `c`/`y` dispatch the selected link's full normalized destination through a second injected sink (production passes `App::copy_to_clipboard` — the existing pbcopy path, no new spawn per WINDOWS entry 6), close the modal, and set a "copied …" message; tests prove the sink payload is byte-identical to `Url::as_str()` and the opener stays untouched
- Overlay UX complete: bottom-anchored `[a] destination` list, selected row bold/reversed, viewport scrolling keeps the selection visible past 26 entries (rows beyond `z` carry no letter, reachable via j/k), and `display_truncated_width` middle-ellipsizes for width only — `url[..Position::BeforePath]` keeps scheme+host visible at ANY budget (T-10-15)
- Gesture call site finalized: the chord resolves the remote-attach parser only under the exact predicate the render path draws with (`remote_id` match + liveness), and merged OSC8+bare results sort by `(row, start_col)` so hint letters read top-to-bottom
- LINK-04 proven at `handle_key` granularity: with the overlay open, printable keys, ctrl chords, Tab, and Enter are all handled or swallowed by the modal path — asserted against a `RemoteAttach::test_stub` whose input channel the test holds (synchronous, no IO thread, no sleeps), with a modal-closed control proving the spy channel is live
- LINK-08 regression-tested: injected-opener `Err` yields exactly one warning naming the failure and "session unaffected", the modal closes, and the very next key is processed normally; `spawn_opener` retains the 10-01 shape (single argv arg, three null stdio handles, reaper thread)
- Phase gate green: `cargo test --workspace --locked` — 0 failures across all suites (140 baude incl. 17 new phase tests, 357 bauded, 93 baude-core, 13 link_fidelity + fork/doc suites); zero compiler warnings

## Task Commits

1. **Task 1 (TDD RED):** `6fabdc2` (test) — copy/navigation/letter/truncation tests + inert seam (second sink param, identity truncation stub); RED evidence verdict `RED_EVIDENCE_OK`
2. **Task 1 (TDD GREEN):** `ed68e57` (feat) — copy dispatch, navigation, hint letters, real truncation, completed ui.rs overlay
3. **Task 2 (TDD RED):** `efa875e` (test) — gesture-integration tests + `RemoteAttach::test_stub` infra; RED evidence verdict `RED_EVIDENCE_OK`
4. **Task 2 (TDD GREEN):** `83fdca9` (feat) — render-parity attach filter + (row, start_col) ordering
5. **Task 3:** `cc89446` (test) — LINK-08 failure-surface and liveness proofs; phase-gate suite run

_No REFACTOR commits: both GREEN diffs were written factored; nothing left to clean with tests green._

## TDD Gate Compliance

- Task 1: RED (`test(10-04)`, 6fabdc2) precedes GREEN (`feat(10-04)`, ed68e57); evidence `10-04-t1-red-evidence.json`, verdict `RED_EVIDENCE_OK` (target `c_copies_full_destination_without_opening` failed on the copy-sink assertion, exit 101)
- Task 2: RED (`test(10-04)`, efa875e) precedes GREEN (`feat(10-04)`, 83fdca9); evidence `10-04-t2-red-evidence.json`, verdict `RED_EVIDENCE_OK` (target `chord_ignores_attach_for_a_different_remote` failed on the parity assertion; second RED target failed on ordering; 13 confirmations passing at RED recorded, 10-02 precedent)

## Files Created/Modified

- `baude/src/app.rs` — `handle_link_hints_key` (copy/nav/letter arms, dual injected sinks), `display_truncated_width`, render-parity filter + sort in `open_link_hints`, `mod link_hints` (15 tests) + `mod link_open` (2 tests)
- `baude/src/ui.rs` — completed LinkHints draw arm: bottom-anchored list, hint labels, viewport scrolling, truncated destinations, action footer
- `baude/src/links.rs` — `DetectedLink` span docs: `row`/`start_col` production-consumed by ordering; `end_col`/`source` explicitly reserved (doc'd `#[allow(dead_code)]`) for pointer hit-testing; stale allow on `LinkSource::Bare` removed
- `baude/src/remote.rs` — `#[cfg(test)] RemoteAttach::test_stub` + `pub(crate) AttachInput` (test infrastructure only; zero production change)

## Decisions Made

- **Action-key shadowing:** hint labels stay a-z (CONTEXT discretion) but `c`/`y`/`j`/`k` resolve as actions first — the locked per-link action set wins; shadowed rows are reachable via navigation
- **Truncation anchor:** `url[..Position::BeforePath]` (scheme + authority) is the never-truncated prefix; below-prefix budgets return `prefix…` rather than hide the origin
- **Modifier guard:** action and letter arms require plain (or shift-only) modifiers, so ctrl chords inside the overlay are pure swallows rather than accidental actions

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `RemoteAttach::test_stub` + `pub(crate) AttachInput` added to remote.rs**
- **Found during:** Task 2 (RED — remote-leg and swallow tests unimplementable otherwise)
- **Issue:** The plan's must-haves require proving the chord resolves the remote-attach parser and that no byte reaches the child, but `RemoteAttach`'s fields are private and its only constructor performs a real websocket handshake — untestable without either a socket server in a unit test (flake surface, sandbox-hostile) or a test-only constructor
- **Fix:** `#[cfg(test)] RemoteAttach::test_stub(remote_id, parser) -> (RemoteAttach, Receiver<AttachInput>)` — no socket, no IO thread; `write_input` sends synchronously into the receiver the test holds, making absence-of-forwarding deterministic. `AttachInput` widened to `pub(crate)` so app tests can match `Bytes`
- **Plan-constraint note:** the plan's verification says "No changes outside baude/src/{app,ui}.rs (parallel-safety and blast-radius check)" — this sequential run is the phase's last plan with no concurrent sibling, so the constraint's rationale is moot (same reasoning as 10-03's fork deviation); zero production code changed
- **Files modified:** baude/src/remote.rs
- **Verification:** all 5 Task-2 integration tests green; full workspace suite green
- **Committed in:** efa875e

**2. [Rule 1 - Bug] Dead-code warning on `DetectedLink` resolved (10-03 handoff)**
- **Found during:** Task 2 (GREEN — the sort consumed `row`/`start_col`, narrowing the pre-existing bin-target warning to `end_col`/`source`)
- **Issue:** 10-03's summary flagged the `DetectedLink` span-field dead-code warning as "10-04 resolves it structurally"; ordering consumes `row`/`start_col` in production, but `end_col`/`source` have no production consumer until pointer hit-testing (modifier-click), which CONTEXT deliberately keeps out of v2.2
- **Fix:** `row`/`start_col` now production-read by the call-site sort; `end_col`/`source` carry a doc-commented `#[allow(dead_code)]` naming the reservation; the stale allow on `LinkSource::Bare` (constructed since 10-03) removed. Workspace builds with zero warnings
- **Files modified:** baude/src/links.rs
- **Verification:** `cargo check --workspace --all-targets --locked` — no warnings
- **Committed in:** 83fdca9

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug)
**Impact on plan:** Both touch files outside the plan's {app,ui}.rs list; both are minimal, test-infrastructure or lint-hygiene only, and required by the plan's own must-haves. No scope creep.

## Known Stubs

None. No placeholder values, no skipped tests, no unrun verifies introduced by this plan.

## Threat Flags

None — no new network endpoints, auth paths, file access, or trust-boundary changes beyond the plan's threat model (T-10-14..17 all carry their planned mitigations: argv-only opener, origin-preserving truncation, reaper-threaded spawn, modal-first key routing — each regression-tested).

## Issues Encountered

- One unidentified bauded lifecycle test failed once (356/357) in the first full-workspace run and passed on immediate re-run (357/357, exit 0). Phase 10 does not touch bauded; recorded in `deferred-items.md` as a pre-existing flake candidate — not fixed here (scope boundary).

## Authentication Gates

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phase 10 complete: all four plans summarized; LINK-01..LINK-08 delivered with automated backing; full workspace suite green under `--locked`
- Residual manual leg: the optional real-terminal/browser dogfood (D6) feeds SHIP-03 at the release gate, per RESEARCH Validation Architecture
- Ready for `/gsd-verify-work 10` and Phase 11 planning

---
*Phase: 10-clickable-terminal-links*
*Completed: 2026-09-16*

## Self-Check: PASSED
