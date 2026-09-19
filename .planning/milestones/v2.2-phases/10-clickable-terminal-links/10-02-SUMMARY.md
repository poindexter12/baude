---
phase: 10-clickable-terminal-links
plan: "02"
subsystem: ui
tags: [vt100, osc8, hyperlinks, terminal, grid, scrollback, remote-attach, tdd]

requires:
  - phase: 10-clickable-terminal-links (plan 10-01)
    provides: vendored vt100 fork with OSC8 dispatch arm, Attrs.link, per-Screen intern table, Cell::link_id(), Screen::link_target(id)
provides:
  - erase-strip in Cell::clear — cleared/erased cells never carry a link id (Pitfall 4 closed)
  - intern-table caps (2083-byte URI, 10k entries) plus a vte OSC-buffer truncation guard — bounded memory, truncated URIs refused (Pitfall 6 closed)
  - OSC8 open/close re-emission in Attrs::write_escape_code_diff with the intern table threaded through all grid/row formatted+diff writers — contents_formatted round-trips identical link state (Pitfall 3 closed)
  - link_fidelity fork test suite (13 tests) + pty.rs subscribe-snapshot parity test
affects: [10-03, 10-04, 12-release-gate, remote-attach]

actuals:
  tokens: 10253
  tasks: 2
  commits: 2
plan_head_before: 01b3ed8bcf0ebe09ff956a3a61b37903ae06701d

tech-stack:
  added: []
  patterns:
    - "hyperlink transitions emit from a single point (Attrs::write_escape_code_diff) with the intern table threaded down from Screen"
    - "fail closed on untrusted-input caps: over-cap/truncated OSC8 degrades to plain text, parsing continues"

key-files:
  created:
    - vendor/vt100/tests/link_fidelity.rs
    - .planning/phases/10-clickable-terminal-links/10-02-red-evidence.json
  modified:
    - vendor/vt100/src/attrs.rs
    - vendor/vt100/src/cell.rs
    - vendor/vt100/src/grid.rs
    - vendor/vt100/src/row.rs
    - vendor/vt100/src/screen.rs
    - vendor/vt100/README.md
    - baude-core/src/pty.rs

key-decisions:
  - "vte 0.11's default feature set includes no_std, capping the OSC buffer at 1024 bytes — truncated OSC8 sequences are refused (link = None) instead of interning a silently-truncated URI; the planned 2083-byte cap stays as defense in depth"
  - "Erase-strip implemented inside Cell::clear (single point) rather than at each call site — every clear/fill path is covered by construction"
  - "OSC8 re-emission lives in Attrs::write_escape_code_diff so formatted, diff, cursor-position, and attributes writers all re-emit correctly from one code path"

patterns-established:
  - "Link-state invariant: output alone never opens a link; blank cells never link; formatted output is link-faithful"

requirements-completed: [LINK-01, LINK-03]

coverage:
  - id: D1
    description: "Link ids survive scrolling into scrollback, soft wrap, resize, and are removed by overwrite — LINK-03 grid inheritance proven"
    requirement: LINK-03
    verification:
      - kind: unit
        ref: "vendor/vt100/tests/link_fidelity.rs#link_fidelity::scroll_into_scrollback_retains_link"
        status: pass
      - kind: unit
        ref: "vendor/vt100/tests/link_fidelity.rs#link_fidelity::resize_keeps_surviving_ids"
        status: pass
      - kind: unit
        ref: "vendor/vt100/tests/link_fidelity.rs#link_fidelity::overwrite_with_plain_text_clears_link"
        status: pass
    human_judgment: false
  - id: D2
    description: "Erased/cleared cells carry no link id — \\e[2J and \\e[K while a run is open leave zero phantom links"
    requirement: LINK-03
    verification:
      - kind: unit
        ref: "vendor/vt100/tests/link_fidelity.rs#link_fidelity::erase_screen_leaves_no_link_ids"
        status: pass
      - kind: unit
        ref: "vendor/vt100/tests/link_fidelity.rs#link_fidelity::erase_line_leaves_no_link_ids"
        status: pass
    human_judgment: false
  - id: D3
    description: "Intern table is bounded: URIs over the cap and entries past 10k degrade to plain text, parsing continues, existing ids resolve"
    requirement: LINK-01
    verification:
      - kind: unit
        ref: "vendor/vt100/tests/link_fidelity.rs#link_fidelity::uri_over_2083_bytes_is_not_a_link"
        status: pass
      - kind: unit
        ref: "vendor/vt100/tests/link_fidelity.rs#link_fidelity::intern_table_cap_stops_new_entries"
        status: pass
    human_judgment: false
  - id: D4
    description: "contents_formatted re-emits OSC8 (including id= grouping and still-open runs) so a fresh parser reconstructs identical link targets — remote-attach snapshot parity at pty.rs subscribe()"
    requirement: LINK-03
    verification:
      - kind: unit
        ref: "vendor/vt100/tests/link_fidelity.rs#link_fidelity::formatted_round_trip_preserves_link_targets"
        status: pass
      - kind: unit
        ref: "vendor/vt100/tests/link_fidelity.rs#link_fidelity::formatted_round_trip_preserves_id_param_grouping"
        status: pass
      - kind: unit
        ref: "vendor/vt100/tests/link_fidelity.rs#link_fidelity::formatted_round_trip_keeps_open_run_open"
        status: pass
      - kind: unit
        ref: "baude-core/src/pty.rs#pty::tests::subscribe_snapshot_replays_pre_attach_links"
        status: pass
    human_judgment: false
  - id: D5
    description: "Multi-semicolon URIs rejoin intact; empty-URI OSC8 closes the run — LINK-01 edges held under the hardened arm"
    requirement: LINK-01
    verification:
      - kind: unit
        ref: "vendor/vt100/tests/link_fidelity.rs#link_fidelity::multi_semicolon_uri_rejoined_intact"
        status: pass
      - kind: unit
        ref: "vendor/vt100/tests/link_fidelity.rs#link_fidelity::empty_uri_close_ends_run"
        status: pass
    human_judgment: false

duration: 16min
completed: 2026-09-16
status: complete
---

# Phase 10 Plan 02: Fork Grid Fidelity + Remote Snapshot Parity Summary

**LINK-03 proven across all six grid behaviors plus remote-attach parity: erase fills strip link ids, the intern table is capped (with a vte truncation guard RESEARCH missed), and contents_formatted re-emits OSC8 so a fresh parser — the exact subscribe() snapshot path — reconstructs identical link state**

## Performance

- **Duration:** 16 min
- **Started:** 2026-09-16T07:29:39Z
- **Completed:** 2026-09-16T07:46:19Z
- **Tasks:** 2 (TDD RED + GREEN)
- **Files modified:** 9

## Accomplishments

- Erase-strip landed at the single choke point (`Cell::clear` sets `attrs.link = None`), so every clear/erase/fill path — `\e[2J`, `\e[K`, ECH, wide-char partner clears, truncation — leaves blank cells with zero link ids (Pitfall 4)
- Intern table bounded: URIs > 2083 bytes and entries past 10 000 degrade to "not a link" while parsing continues and existing ids keep resolving; additionally, OSC8 sequences that filled vte's 1024-byte accumulation buffer are refused rather than interned truncated (Pitfall 6 + a RESEARCH correction, see Deviations)
- OSC8 re-emission implemented at a single point — `Attrs::write_escape_code_diff` emits `\x1b]8;[id=X];URI\x1b\\` open and `\x1b]8;;\x1b\\` close transitions, with the Screen's intern table threaded through every grid/row formatted and diff writer — so `contents_formatted()`, `contents_diff()`, `rows_formatted()`, `attributes_formatted()`, and the cursor-position redraw all re-emit hyperlinks, preserving the `id=` param and still-open runs
- Remote-attach parity proven: a pure pty.rs test builds the snapshot byte-for-byte the way `subscribe()` does (pty.rs:390-410) and asserts a fresh parser reports identical per-cell link targets (Pitfall 3 / Open Question 1 shipped this phase)
- Full workspace regression gate green under `--locked`: 573 passed / 0 failed (was 559; +13 link_fidelity, +1 pty parity)

## LINK-03 Assumption Confirmation (flagged planner assumption)

The RED run made the grid-model inheritance assumption observable. **Six inheritance cases passed before any implementation** — confirming that `link` riding on `Attrs` makes these behaviors correct for free:

- `scroll_into_scrollback_retains_link` (scrolling + scrollback view)
- `wrapped_fragments_share_one_id` (soft wrap)
- `resize_keeps_surviving_ids` (shrink + grow)
- `overwrite_with_plain_text_clears_link` (overwrite)
- `multi_semicolon_uri_rejoined_intact` (rejoin regression)
- `empty_uri_close_ends_run` (edge lift)

The seven guaranteed-RED targets (erase x2, URI cap, intern cap, round-trip x3, pty snapshot parity) all failed on planned-behavior assertions, exactly as planned.

## Task Commits

1. **Task 1 (TDD RED):** `1f8135f` (test) — 13 fork fidelity tests + pty snapshot-parity test; RED evidence verdict `RED_EVIDENCE_OK`
2. **Task 2 (TDD GREEN):** `00f6705` (feat) — erase-strip, caps, truncation guard, OSC8 re-emission, README diff surface

_No REFACTOR commit: the GREEN diff was already minimal (single-point emission, mechanical threading); nothing to clean up._

## TDD Gate Compliance

- RED (`test(10-02)`, 1f8135f) precedes GREEN (`feat(10-02)`, 00f6705)
- RED evidence: `.planning/phases/10-clickable-terminal-links/10-02-red-evidence.json`, verdict `RED_EVIDENCE_OK` (target `link_fidelity::erase_screen_leaves_no_link_ids` failed on the phantom-link assertion, exit 101; all other guaranteed-RED targets failed on assertions, no compile/discovery failures)

## Files Created/Modified

- `vendor/vt100/tests/link_fidelity.rs` — 13-test fidelity suite (grid ops, caps, round-trips)
- `vendor/vt100/src/cell.rs` — `Cell::clear` strips link from fill attrs
- `vendor/vt100/src/screen.rs` — caps + truncation guard in the OSC8 path; intern table passed into every formatted writer
- `vendor/vt100/src/attrs.rs` — OSC8 transition emission in `write_escape_code_diff`
- `vendor/vt100/src/grid.rs`, `vendor/vt100/src/row.rs` — intern table threaded through formatted/diff/cursor writers
- `vendor/vt100/README.md` — diff-surface table updated (grid.rs, row.rs, link_fidelity.rs now listed)
- `baude-core/src/pty.rs` — pure `subscribe_snapshot_replays_pre_attach_links` test (no production change, as PATTERNS predicted)

## Decisions Made

- Erase-strip inside `Cell::clear` (one point) instead of per-call-site attrs mutation — covers all clear/fill paths by construction, smaller fork diff
- Re-emission at `Attrs::write_escape_code_diff` — one emission point serves formatted, diff, rows, attributes, and cursor-position writers; SGR bytes never carry link state
- Truncated OSC8 sequences (vte buffer full) refuse the link rather than interning a truncated-but-valid-looking URL (fail closed, matching the project's safety-critical-path pattern)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] vte truncates OSC at 1024 bytes — the planned 2083-byte cap alone could intern silently-truncated URIs**
- **Found during:** Task 2 (GREEN — `uri_over_2083_bytes_is_not_a_link` stayed red after the caps landed)
- **Issue:** RESEARCH Pitfall 2 verified `osc_raw: Vec<u8>` as unbounded in std builds, but vte 0.11's *default* feature set includes `no_std`, making `osc_raw` an `ArrayVec` capped at `MAX_OSC_RAW = 1024`. A 3000-byte URI arrived truncated to 1023 bytes — under the 2083 cap — and interned as a wrong-but-well-formed, still-openable destination
- **Fix:** The OSC8 arm computes the total accumulated length and refuses the link (`attrs.link = None`) when the buffer hit its cap; the 2083-byte check in `intern_link` retained as defense in depth for future rebases
- **Files modified:** vendor/vt100/src/screen.rs
- **Verification:** `uri_over_2083_bytes_is_not_a_link` green; full fork + workspace suites green
- **Committed in:** 00f6705

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Required for correctness — without it the URI-cap must-have was unsatisfiable and truncated links would have been openable. No scope creep; effective URI limit is now ~1016 bytes (vte's buffer bound), stricter than the planned 2083.

## Known Stubs

None introduced by this plan. (The pre-existing `collect_bare_links` seam from 10-01 remains 10-03 scope, WINDOWS entry 10; `DetectedLink` position fields emit a dead-code warning until 10-03/10-04 consume them — pre-existing, out of this plan's scope.)

## Issues Encountered

- `gsd-tools check tdd-red-evidence` parses node-TAP output only; cargo test results were transcribed into a faithful TAP projection (names/counts/exit codes verbatim from the real runs, raw cargo lines as TAP comments), same as 10-01.

## Authentication Gates

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Grid fidelity and remote snapshot parity are implementation-backed; 10-03 (bare-URL pass) and 10-04 (overlay UX) build on a fork whose link state is bounded, erase-correct, and snapshot-faithful
- Wave-2 merge gate (`cargo test --workspace --locked`) green from this plan's side; combine with 10-03 at merge

---
*Phase: 10-clickable-terminal-links*
*Completed: 2026-09-16*

## Self-Check: PASSED
