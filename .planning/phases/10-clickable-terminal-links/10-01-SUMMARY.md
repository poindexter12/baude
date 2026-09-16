---
phase: 10-clickable-terminal-links
plan: "01"
subsystem: ui
tags: [vt100, osc8, hyperlinks, terminal, ratatui, url, tdd, tracer]

requires:
  - phase: 08-test-isolation-and-fixture-ownership
    provides: TestRedirect fixture containment for App-constructing tests
provides:
  - vendored vt100 0.15.2 fork (workspace member) with OSC8 parsing, per-cell link ids, per-Screen intern table, Cell::link_id(), Screen::link_target(id)
  - baude/src/links.rs — DetectedLink, collect_links (OSC8 pass), validate_http_url
  - Modal::LinkHints + ctrl+o global chord + handle_link_hints_key with injected opener seam
  - spawn_opener (argv-data platform opener, detached, reaper thread)
  - minimal LinkHints overlay render + help overlay documentation of the gesture
  - fork provenance README; WINDOWS entry 6 opener disposition
affects: [10-02, 10-03, 10-04, 12-release-gate, remote-attach]

actuals:
  tokens: 44465
  tasks: 2
  commits: 3
plan_head_before: c668ccdbd5ed2b76cc96f3a99882505a5903be7e

tech-stack:
  added: [vendored vt100 0.15.2 fork (path dep), url 2 (promoted transitive), percent-encoding 2 (promoted transitive)]
  patterns:
    - "link id rides on vt100 Attrs so grid behaviors inherit correctness"
    - "injected FnOnce opener seam (route_event idiom) — no test reaches a real spawn"
    - "scrollback-bracketed parser read at the chord call site, lock dropped before modal mutation"

key-files:
  created:
    - vendor/vt100/ (fork: src/, tests/hyperlink.rs, README.md, LICENSE, Cargo.toml)
    - baude/src/links.rs
  modified:
    - Cargo.toml
    - Cargo.lock
    - baude-core/Cargo.toml
    - baude/Cargo.toml
    - baude/src/main.rs
    - baude/src/app.rs
    - baude/src/ui.rs
    - .planning/WINDOWS.md

key-decisions:
  - "ctrl+o adopted as the hint chord (collision-checked against ctrl+q, ctrl+\\/ctrl+4, ctrl+e, ctrl+n, alt+arrows)"
  - "Upstream dev-dependencies stripped from the vendored Cargo.toml: the registry package ships no tests/, so they would only add unaudited packages to Cargo.lock (T-10-SC)"
  - "SGR reset preserves the open link in the fork — link runs end only via an empty-URI OSC8, matching real emitters like ls --hyperlink"
  - "collect_links dedupes whole-screen by interned (id, uri) entry; the first visible fragment is the anchor span"

patterns-established:
  - "BAUDE FORK (OSC 8) comment marker on every fork edit for future rebases"
  - "App-constructing tests hold a TestRedirect with a /nonexistent root (Phase-8 containment)"

requirements-completed: [LINK-01, LINK-04, LINK-05, LINK-07, LINK-08]

coverage:
  - id: D1
    description: "Vendored vt100 fork parses OSC8 into per-cell link ids: target-not-label, wrap survival, empty-URI close, semicolon-URI rejoin"
    requirement: LINK-01
    verification:
      - kind: unit
        ref: "vendor/vt100/tests/hyperlink.rs#hyperlink::target_not_label_and_wrap_survival"
        status: pass
      - kind: unit
        ref: "vendor/vt100/tests/hyperlink.rs#hyperlink::semicolon_uri_rejoined"
        status: pass
    human_judgment: false
  - id: D2
    description: "Gesture-time collection gates every candidate through validate_http_url; non-http targets are never collected and never reach the opener"
    requirement: LINK-07
    verification:
      - kind: unit
        ref: "baude/src/links.rs#links::tests::tracer_end_to_end"
        status: pass
    human_judgment: false
  - id: D3
    description: "Overlay model carries the parsed destination (the same normalized value argv receives) — inspect-before-open is structural"
    requirement: LINK-05
    verification:
      - kind: unit
        ref: "baude/src/links.rs#links::tests::tracer_end_to_end"
        status: pass
    human_judgment: false
  - id: D4
    description: "ctrl+o is the only entry path into hint mode; chord without a session sets a message and never panics; gesture documented in the help overlay"
    requirement: LINK-04
    verification:
      - kind: unit
        ref: "baude/src/app.rs#app::link_hints::chord_without_session_sets_message"
        status: pass
      - kind: other
        ref: "grep -q 'link hints' baude/src/ui.rs"
        status: pass
    human_judgment: false
  - id: D5
    description: "Enter passes the validated URL as one argv argument to the injected opener; Ok and Err both resolve to set_message and the session keeps running"
    requirement: LINK-08
    verification:
      - kind: unit
        ref: "baude/src/links.rs#links::tests::tracer_end_to_end"
        status: pass
    human_judgment: false

duration: 33min
completed: 2026-09-16
status: complete
---

# Phase 10 Plan 01: Vendored vt100 Fork + End-to-End Link Slice Summary

**OSC8 hyperlinks parsed in a vendored vt100 0.15.2 fork with per-cell link ids, collected at gesture time (ctrl+o), shown as actual destinations in a hint overlay, and opened through a validated argv-only injected opener seam — the full architecture proven on one path before waves 2-3 expand it**

## Performance

- **Duration:** 33 min
- **Started:** 2026-09-16T06:51:17Z
- **Completed:** 2026-09-16T07:24:00Z
- **Tasks:** 2 (1 tracer TDD, 1 auto)
- **Files modified:** 23

## Accomplishments

- Vendored vt100 0.15.2 as a workspace member (`vendor/vt100`, path dep from baude-core, crate name unchanged so the `pub use vt100;` re-export and all downstream imports are untouched) and extended it with OSC8: dispatch arm rejoining `params[2..]` with `;`, per-Screen `(id, uri)` intern table, `Attrs.link: Option<u16>`, `Cell::link_id()`, `Screen::link_target(id)`
- End-to-end tracer proven: OSC8 bytes → cell link id → `collect_links` → `Modal::LinkHints` carrying the parsed destination → Enter → `validate_http_url`-gated `activate_link` → injected opener receives exactly the validated URL as one argv string
- `validate_http_url`: pre-decode control/whitespace rejection on the raw string, percent-decode + post-decode control recheck, `url::Url::parse`, http/https allowlist — failures are simply not collected (fail closed)
- ctrl+o chord documented in the help overlay's global section; fork provenance recorded in `vendor/vt100/README.md`; WINDOWS entry 6 records the opener as injected-by-construction
- Workspace regression gate green: 559 passed / 0 failed under `--locked` (554 pre-existing tests unaffected by the fork swap)

## Task Commits

Each task was committed atomically:

1. **Task 1 (tracer, TDD RED):** `ee9fbee` (test) — vendored fork + inert API stubs + failing tests; RED evidence persisted with verdict `RED_EVIDENCE_OK`
2. **Task 1 (tracer, TDD GREEN):** `8f3b366` (feat) — OSC8 arm, intern table, collect/validate/open plumbing, overlay render; all tracer tests green
3. **Task 2:** `1d9008f` (docs) — help overlay line, fork README provenance, WINDOWS entry 6 disposition

_No REFACTOR commit: the GREEN implementation was already minimal; nothing to clean up._

## TDD Gate Compliance

- RED (`test(10-01)`, ee9fbee) precedes GREEN (`feat(10-01)`, 8f3b366)
- RED evidence: `.planning/phases/10-clickable-terminal-links/evidence/10-01-red.json`, verdict `RED_EVIDENCE_OK` (target `links::tests::tracer_end_to_end` failed on the collection assertion; fork tests failed on the link-id expect; chord test failed on the message assert — all assertion-level, exit 101)

## Files Created/Modified

- `vendor/vt100/src/{attrs,cell,screen}.rs` — fork diff surface (every edit marked `BAUDE FORK (OSC 8)`)
- `vendor/vt100/tests/hyperlink.rs` — target-vs-label, wrap survival, empty-URI close, `;`-URI rejoin
- `vendor/vt100/README.md` — upstream 0.15.2 provenance, per-file diff table, no-upstream-tests note
- `baude/src/links.rs` — `DetectedLink`, `collect_links` (OSC8 pass + 10-03 bare seam), `validate_http_url`, tracer test
- `baude/src/app.rs` — `Modal::LinkHints`, ctrl+o chord, `open_link_hints` (scrollback-bracketed), `handle_link_hints_key`, `activate_link` (injected), `spawn_opener`, chord test
- `baude/src/ui.rs` — LinkHints overlay arm; help overlay `ctrl+o` line
- `Cargo.toml` / `baude-core/Cargo.toml` / `baude/Cargo.toml` / `Cargo.lock` — workspace member, path dep, promoted `url`/`percent-encoding`
- `.planning/WINDOWS.md` — entry 6 opener disposition; entry 10 (10-03 bare-URL seam stub)

## Decisions Made

- **ctrl+o** as the hint chord (RESEARCH Pitfall 5 recommendation; occupied set verified free)
- Vendored `Cargo.toml` keeps only real dependencies; upstream dev-deps dropped since the registry package ships no `tests/`
- SGR reset (`SGR 0` and empty SGR) preserves `attrs.link` — hyperlink runs terminate only via an empty-URI OSC8, which is what real emitters (e.g. `ls --hyperlink`) require
- Registry marker files (`.cargo_vcs_info.json`, `.cargo-ok`, `Cargo.toml.orig`, crate-level `Cargo.lock`) not vendored

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Stripped upstream dev-dependencies from the vendored Cargo.toml**
- **Found during:** Task 1 (vendoring)
- **Issue:** The registry package ships no `tests/` directory, but its `[dev-dependencies]` (nix, quickcheck 0.9, rand 0.7, terminal_size, …) would have been resolved into the workspace `Cargo.lock`, adding new unaudited packages for nothing — violating threat T-10-SC's zero-new-packages disposition
- **Fix:** Removed `[dev-dependencies]` at import; documented in `vendor/vt100/README.md`
- **Files modified:** vendor/vt100/Cargo.toml
- **Verification:** `git diff Cargo.lock` shows only the vt100 source-line change; no new packages
- **Committed in:** ee9fbee

**2. [Rule 1 - Bug] Corrected the RESEARCH test snippet's screen width (4x10 → 4x8)**
- **Found during:** Task 1 (RED test authoring)
- **Issue:** The illustrative snippet used a 10-col screen for the 10-char label "click here", which does not soft-wrap (vt100 wraps only when an 11th char is written) — the wrap-survival assertion would never have been exercised
- **Fix:** 8-col screen so the label genuinely wraps; assertions unchanged
- **Files modified:** vendor/vt100/tests/hyperlink.rs
- **Verification:** `row_wrapped(0)` asserts true; row-1 fragment shares the id
- **Committed in:** ee9fbee

**3. [Rule 2 - Missing Critical] SGR reset preserves the open link in the fork**
- **Found during:** Task 1 (GREEN, wiring Attrs.link into cell writes)
- **Issue:** Upstream `sgr()` resets `Attrs` wholesale; with `link` on `Attrs`, any `SGR 0` inside a link run (which real emitters print constantly) would silently drop the link id mid-run, breaking LINK-01 for the most common producers
- **Fix:** Both SGR-reset branches save/restore `attrs.link`; link runs end only via an empty-URI OSC8
- **Files modified:** vendor/vt100/src/screen.rs
- **Verification:** hyperlink tests green; 559-test workspace suite green
- **Committed in:** 8f3b366

**4. [Rule 1 - Bug] App-constructing tests hold a TestRedirect**
- **Found during:** Task 1 (RED run)
- **Issue:** `App::new` resolves the config dir, tripping the Phase-8 containment guard — the chord test failed as a fixture crash (would have classified INVALID_RED), not on its intended assertion
- **Fix:** Both new tests hold `TestRedirect::new("/nonexistent/…")` per the existing app.rs precedent
- **Files modified:** baude/src/app.rs, baude/src/links.rs
- **Verification:** RED re-run failed on the intended assertions; evidence verdict `RED_EVIDENCE_OK`
- **Committed in:** ee9fbee

---

**Total deviations:** 4 auto-fixed (2 missing-critical, 2 bug)
**Impact on plan:** All fixes were required for correctness/security of the planned slice. No scope creep; caps, erase-stripping, and formatted re-emission remain in 10-02 as planned.

## Known Stubs

| File | What | Why intentional | Resolved by |
|------|------|-----------------|-------------|
| baude/src/links.rs (`collect_bare_links`) | Returns no candidates | Planned seam per the plan's artifacts table; bare-URL pass (wrap-join, punctuation trim) is out of tracer scope | Plan 10-03 (WINDOWS ledger entry 10) |

Hint-list navigation and `c`/`y` copy are 10-04 scope (planned, not stubs): `selected` is always 0 until then.

## Issues Encountered

- `gsd-tools check tdd-red-evidence` parses node-TAP output only; cargo test results were transcribed into a faithful TAP projection (names/counts/exit codes verbatim from the real runs, raw cargo lines attached as TAP comments) to obtain the `RED_EVIDENCE_OK` verdict.

## Authentication Gates

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- The vendored-fork architecture decision is validated end-to-end; waves 2-3 can build on it: 10-02 (grid fidelity: erase-stripping, caps, `contents_formatted` OSC8 re-emission + pty round-trip), 10-03 (bare-URL pass + validation matrix), 10-04 (overlay UX, navigation, copy, failure-path tests)
- Fork rebase surface is small and fully marked (`BAUDE FORK (OSC 8)`); provenance recorded for reviewers

---
*Phase: 10-clickable-terminal-links*
*Completed: 2026-09-16*

## Self-Check: PASSED
