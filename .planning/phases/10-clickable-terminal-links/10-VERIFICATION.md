---
phase: 10-clickable-terminal-links
verified: 2026-09-16T10:30:00Z
status: passed
score: 23/24 must-haves verified
covered_files:
  - ".planning/REQUIREMENTS.md"
  - ".planning/WINDOWS.md"
  - ".planning/phases/10-clickable-terminal-links/10-01-PLAN.md"
  - ".planning/phases/10-clickable-terminal-links/10-01-SUMMARY.md"
  - ".planning/phases/10-clickable-terminal-links/10-02-PLAN.md"
  - ".planning/phases/10-clickable-terminal-links/10-02-SUMMARY.md"
  - ".planning/phases/10-clickable-terminal-links/10-03-PLAN.md"
  - ".planning/phases/10-clickable-terminal-links/10-03-SUMMARY.md"
  - ".planning/phases/10-clickable-terminal-links/10-04-PLAN.md"
  - ".planning/phases/10-clickable-terminal-links/10-04-SUMMARY.md"
  - "Cargo.lock"
  - "Cargo.toml"
  - "baude-core/Cargo.toml"
  - "baude-core/src/pty.rs"
  - "baude/Cargo.toml"
  - "baude/src/app.rs"
  - "baude/src/links.rs"
  - "baude/src/main.rs"
  - "baude/src/remote.rs"
  - "baude/src/ui.rs"
  - "vendor/vt100/Cargo.toml"
  - "vendor/vt100/README.md"
  - "vendor/vt100/src/attrs.rs"
  - "vendor/vt100/src/cell.rs"
  - "vendor/vt100/src/grid.rs"
  - "vendor/vt100/src/lib.rs"
  - "vendor/vt100/src/parser.rs"
  - "vendor/vt100/src/row.rs"
  - "vendor/vt100/src/screen.rs"
  - "vendor/vt100/src/term.rs"
  - "vendor/vt100/tests/hyperlink.rs"
  - "vendor/vt100/tests/link_fidelity.rs"
covered_digest: "v1:sha256:c72f61c047e03c4917181740d2089ed8ae4b767f29dc6b0c17eb17f02f158e2a"
behavior_unverified: 0
overrides_applied: 0
prohibitions:
  - statement: "output alone never opens a link"
    tier: judgment
    llm_judge_verdict: "pass (NON-AUTHORITATIVE)"
    flagged: true
    evidence: "spawn_opener invoked only at app.rs:4407 inside handle_link_hints_key; Modal::LinkHints set only in open_link_hints (app.rs:5599); open_link_hints called only from the ctrl+o chord (app.rs:3913); modal_open_swallows_every_key_from_the_child + paste leg green"
  - statement: "opener never receives shell code (argv data only)"
    tier: judgment
    llm_judge_verdict: "pass (NON-AUTHORITATIVE)"
    flagged: true
    evidence: "spawn_opener (app.rs:5705) is Command::new(OPENER).arg(url), null stdio, no shell anywhere; tracer_end_to_end spy asserts exact argv string; validated URLs always start http(s):// so never parsed as a flag"
  - statement: "non-HTTP(S)/malformed/control-bearing targets never activatable"
    tier: judgment
    llm_judge_verdict: "pass (NON-AUTHORITATIVE)"
    flagged: true
    evidence: "validate_http_url (links.rs:298) canonical-prefix + pre/post-percent-decode control reject + scheme allowlist; links::validate matrix (7 tests) green; failed candidates never collected"
  - statement: "opener failure never kills the session"
    tier: judgment
    llm_judge_verdict: "pass (NON-AUTHORITATIVE)"
    flagged: true
    evidence: "activate_link resolves both arms to set_message; link_open::opener_error_surfaces_one_warning_and_session_survives green"
  - statement: "link handling never breaks selection/drag-copy/child mouse"
    tier: judgment
    llm_judge_verdict: "pass (NON-AUTHORITATIVE)"
    flagged: true
    evidence: "no new mouse arms added in the phase diff; all pre-existing selection/clipboard tests pass unmodified in the 612-test suite run (exit 0)"
human_verification:
  - test: "Backstop (insufficient_spec): confirm no byte sequence arriving as terminal output can trigger link activation without the explicit gesture"
    expected: "cat a file of hostile OSC8/URL-bearing bytes into a pane; nothing opens, no process spawns; only ctrl+o + Enter opens anything"
    why_human: "universal negative tagged verification: backstop in 10-01 frontmatter — structural single-entry proof (chord is the only path to Modal::LinkHints; Enter the only path to the opener) plus the swallow test are presence+wiring evidence, which never qualifies for a backstop-tier truth; no held-out/property test enumerates arbitrary output bytes"
  - test: "Judgment-tier prohibitions (5, listed in frontmatter): resolve each flagged must-NOT at the end-of-phase checkpoint"
    expected: "human accepts the recorded NON-AUTHORITATIVE pass verdicts (evidence cited per item) or reopens"
    why_human: "judgment-tier prohibitions require explicit human resolution — never a silent pass"
  - test: "Dogfood (harvested from 10-04 Task 3 human-check, feeds SHIP-03): run baude in a real terminal, `printf '\\e]8;;https://example.com\\e\\\\click\\e]8;;\\e\\\\ and https://exa\\nmple bare\\n'` in a shell pane, press ctrl+o"
    expected: "overlay lists destinations (not labels); c copies; Enter opens the real browser; selection/drag-copy still work"
    why_human: "real browser launch, real clipboard, and visual overlay quality cannot be exercised by tests (every test injects sinks by design — WINDOWS entry 6)"
---

# Phase 10: Clickable Terminal Links Verification Report

**Phase Goal:** Users can inspect, copy, and explicitly open validated HTTP(S) destinations while terminal rendering, selection, and remote attachment remain authoritative and safe.
**Verified:** 2026-09-16T10:30:00Z
**Status:** human_needed
**Re-verification:** No — initial verification

## Goal Achievement

Verified against the actual codebase at HEAD (8cd42fb), which includes all four review-fix commits (dac2658 CR-01, 8f354a2 WR-01, 0849269 WR-02, 84e0c87 WR-03). SUMMARY claims were checked against code, not trusted. Full workspace suite run once in this verification: `cargo test --workspace --locked` → exit 0, **612 passed / 0 failed** (matches the fix report's claim; every named phase test confirmed present in the captured output).

### ROADMAP Success Criteria

| # | Success Criterion | Status | Evidence |
|---|-------------------|--------|----------|
| 1 | Labeled OSC8 links open their actual HTTP(S) destination; preview/copy before opening | ✓ VERIFIED | Fork `hyperlink::target_not_label_and_wrap_survival` (destination via `Screen::link_target`, never cell text); `links::tests::tracer_end_to_end` (spy receives exactly the parsed URI as one argv string); overlay renders `Url::as_str()` (ui.rs:2114); `c_copies_full_destination_without_opening` — all green |
| 2 | Bare HTTP(S) URLs, incl. soft-wrapped, retain valid characters without prose punctuation | ✓ VERIFIED | 11 `links::bare_url` tests green: `soft_wrapped_url_joined_via_row_wrapped`, `explicit_newline_is_never_joined`, `unbalanced_trailing_paren_stripped_balanced_retained`, `trailing_prose_punctuation_stripped`, offscreen continuation, OSC8 exclusion; join uses `row_wrapped` only (links.rs:100-178, no column heuristic) |
| 3 | Link metadata attached to correct cells through scroll/scrollback/wrap/resize/overwrite/erasure — local and attached remote | ✓ VERIFIED | 18 fork `link_fidelity` tests green (scroll, wrap, wide-char contiguity, resize, overwrite, `\e[2J`/`\e[K` erase-strip, caps, param-saturation, round-trip); `pty::tests::subscribe_snapshot_replays_pre_attach_links` green (remote snapshot parity); `Cell::clear` strips link at the single choke point |
| 4 | Documented explicit gesture activates; selection/drag-copy/scrolling/child mouse stay usable; output alone never opens | ✓ VERIFIED* | Help overlay line ui.rs:2209 ("ctrl+o link hints"); chord at app.rs:3909-3915 is the sole entry; `modal_open_swallows_every_key_from_the_child` incl. WR-01 paste leg + non-vacuous forwarding controls green; pre-existing selection/clipboard tests unmodified and green. *The universal-negative "output alone never opens" leg is the phase backstop — routed to human verification (see below), per its `verification: backstop` tag |
| 5 | Unsupported/malformed/control targets non-activatable; allowed targets passed as data; failure leaves session running with useful error | ✓ VERIFIED | `links::validate` matrix (7 tests) green — schemes, raw/percent-encoded controls, malformed spellings all rejected fail-closed; `spawn_opener` is argv-data only (app.rs:5705); `opener_error_surfaces_one_warning_and_session_survives` green |

### Observable Truths (merged from 4 PLAN frontmatters, deduped)

| # | Truth (source) | Status | Evidence |
|---|----------------|--------|----------|
| 1 | ctrl+o over an OSC8-bearing pane opens LinkHints with parsed destination, never the label (10-01) | ✓ VERIFIED | chord → `open_link_hints` → `collect_links` → `Modal::LinkHints` wiring at app.rs:3913/5564-5603; tracer + fork target-not-label tests green |
| 2 | Enter passes validated URL as one argv arg to injected opener; Ok and Err both set_message, session runs (10-01) | ✓ VERIFIED | `activate_link` (app.rs:5683-5692) both arms set_message; tracer spy + `link_open` tests green |
| 3 | Empty-URI OSC8 closes the run; empty targets never activatable (10-01/10-02) | ✓ VERIFIED | screen.rs:1693-1699 (`uri.is_empty()` → `attrs.link = None`); `link_fidelity::empty_uri_close_ends_run` green |
| 4 | Only validate_http_url-accepted targets activatable (10-01) | ✓ VERIFIED | every candidate gated in `collect_links`; validate matrix green |
| 5 | Chord documented in help overlay global section (10-01) | ✓ VERIFIED | ui.rs:2209 exact line present |
| 6 | Link ids correct through scroll/scrollback/wrap/resize/overwrite; erased cells carry none (10-02) | ✓ VERIFIED | 6 grid-fidelity tests + erase tests green; `Cell::clear` single-point strip |
| 7 | contents_formatted round-trip → identical ids/targets in fresh parser; pty.rs snapshot parity (10-02) | ✓ VERIFIED | 3 `formatted_round_trip_*` tests + `subscribe_snapshot_replays_pre_attach_links` green |
| 8 | Semicolon-bearing URI reconstructed intact (10-01/10-02) | ✓ VERIFIED | `params[2..].join(&b';')` at screen.rs:1692; `semicolon_uri_rejoined`, `multi_semicolon_uri_rejoined_intact` green |
| 9 | Over-cap URI / full table / truncated OSC → link None, parse continues, bounded memory (10-02) | ✓ VERIFIED | `intern_link` caps (screen.rs:594-602) + `VTE_MAX_OSC_RAW`/`VTE_MAX_OSC_PARAMS` fail-closed guards (CR-01 fix, dac2658); `uri_over_2083_bytes_is_not_a_link`, `intern_table_cap_stops_new_entries`, `param_saturated_uri_is_not_a_link`, `uri_below_param_cap_still_interns_intact` green |
| 10 | Soft-wrapped bare URL detected as ONE URL via row_wrapped only (10-03) | ✓ VERIFIED | logical-line builder flushes at `!row_wrapped` (links.rs); wrap-join + newline-never-joins tests green |
| 11 | Unbalanced trailing punctuation stripped; balanced closers retained (10-03) | ✓ VERIFIED | `trim_trailing_punctuation` (links.rs:269-293) with open/close balance count; both tests green |
| 12 | Control chars rejected pre-parse AND post-percent-decode (10-03) | ✓ VERIFIED | links.rs:309-317; `rejects_raw_control_chars_and_whitespace`, `rejects_percent_encoded_controls_post_decode` green |
| 13 | Valid percent-encoded/UTF-8 URLs survive; collected destination = normalized parse shown and opened (10-03) | ✓ VERIFIED | `percent_encoded_utf8_survives_detection_intact`, `accepted_normalized_form_is_returned` green |
| 14 | Non-http(s) schemes never collected; empty rejected (10-03) | ✓ VERIFIED | `rejects_non_http_schemes` (file/javascript/data/ftp/mailto), `rejects_empty_and_malformed` green |
| 15 | OSC8-run cells excluded from bare pass — one link, not two (10-03) | ✓ VERIFIED | OSC8 cells contribute non-URL placeholder; `osc8_run_cells_excluded_from_bare_pass` green |
| 16 | c/y copies actual destination without opening; Enter opens; Esc dismisses (10-04) | ✓ VERIFIED | `c_copies_full_destination_without_opening`, `y_copies_selected_entry`, `enter_opens_selected_entry_only`, `esc_dismisses` behavior green; copy sink fallible + honest messaging (WR-02 fix, `copy_error_surfaces_failure_not_success` green) |
| 17 | Every key (and paste — WR-01) while overlay open handled/swallowed; no byte reaches child (10-04) | ✓ VERIFIED | `handle_modal_key` routes LinkHints first (app.rs:4406); `handle_paste` early-returns for non-None/non-Input modal (app.rs:3973-3975); swallow test with live channel spy + forwarding controls green |
| 18 | Zero links → message, no modal (10-04) | ✓ VERIFIED | app.rs:5601 ("no links visible"); `chord_with_zero_links_sets_message_and_restores_bracket` green |
| 19 | Gesture collects OSC8 + bare at current scroll offset, set_scrollback bracket, lock dropped before modal mutation (10-04) | ✓ VERIFIED | app.rs:5588-5596 (bracket inside lock closure, guard drops before `self.modal =`); bracket-restore + both-passes tests green |
| 20 | Chord resolves focused pane's parser for local Claude, Shell, AND active remote attach — render-parity filter (10-04) | ✓ VERIFIED | app.rs:5565-5586 filters `remote_id == id && !is_closed()` identical to draw path; `chord_ignores_attach_for_a_different_remote` green |
| 21 | Opener spawn error → one set_message naming failure; session keeps running (10-04) | ✓ VERIFIED | `opener_error_surfaces_one_warning_and_session_survives` (asserts subsequent event processed) green |
| 22 | Display middle-truncated only; scheme+host always visible; copy/open use full URL (10-04) | ✓ VERIFIED | `display_truncated_width` anchors on `Position::BeforePath`; 3 truncation tests green; model stores full `Url` |
| 23 | Wide-char labels keep contiguous ids / full span (WR-03, post-review edge lift) | ✓ VERIFIED | spacer carries base link id via `Cell::set_link` (screen.rs:1057); `wide_char_label_keeps_contiguous_link_ids`, `wide_char_label_records_full_span` green |
| 24 | **Backstop:** no byte sequence arriving as output can trigger activation without the gesture (10-01, `verification: backstop`) | ⚠️ insufficient_spec | Structural evidence is strong (single entry: only `open_link_hints` sets `Modal::LinkHints`, only the ctrl+o chord calls it, only modal-Enter reaches the opener; grep found no other call sites) but this is presence+wiring, which never qualifies for a backstop-tier universal negative — routed to human verification |

**Score:** 23/24 truths verified (0 present-but-behavior-unverified; 1 backstop item routed to human by tier rule, not by any observed gap)

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `vendor/vt100/` | Workspace-member fork: OSC8 arm, Attrs.link, intern table, link_id(), link_target() | ✓ VERIFIED | All present; workspace member (Cargo.toml:3), path dep (baude-core/Cargo.toml:28), `pub use vt100;` unchanged (lib.rs:6); BAUDE FORK markers on edits |
| `baude/src/links.rs` | DetectedLink, collect_links (both passes), validate_http_url | ✓ VERIFIED | 673 lines, substantive; bare pass complete (10-03 stub deviation closed — WINDOWS entry 10 marked fixed) |
| `baude/src/app.rs` link surface | Modal::LinkHints, ctrl+o chord, handle_link_hints_key (dual injected sinks), spawn_opener, open_link_hints | ✓ VERIFIED | All present and production-wired at app.rs:408/3913/4407/5564/5612/5705 |
| `baude/src/ui.rs` | LinkHints draw arm + help line | ✓ VERIFIED | Draw arm ui.rs:2114 (hint letters, selection highlight, truncation); help line 2209 |
| `vendor/vt100/README.md` | Provenance: upstream 0.15.2, diff surface | ✓ VERIFIED | Upstream version, source, license, per-file diff surface documented |
| `.planning/WINDOWS.md` entry 6 | Opener injected-by-construction disposition | ✓ VERIFIED | Entry 6 updated; no duplicate clipboard/editor entries |
| Fork test suites | `hyperlink.rs`, `link_fidelity.rs` | ✓ VERIFIED | 2 + 18 tests (incl. CR-01/WR-03 regressions), all green |
| `baude-core/src/pty.rs` test | subscribe-snapshot link parity | ✓ VERIFIED | `subscribe_snapshot_replays_pre_attach_links` at pty.rs:1120, green |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| ctrl+o chord | Modal::LinkHints | `open_link_hints` → `collect_links` | ✓ WIRED | app.rs:3913 → 5564 → 5599; only entry path (grep: no other `Modal::LinkHints` assignment in production code) |
| Attrs.link | destination string | Screen intern table → `link_target(id)` | ✓ WIRED | cell text never consulted; screen.rs:583/594 |
| Enter | OS opener | `validate`-gated → `activate_link(spawn_opener)` → `Command::new(OPENER).arg(url)` | ✓ WIRED | app.rs:4407 passes `spawn_opener` + `Self::copy_to_clipboard` in production; tests inject spies |
| c/y | clipboard | injected copy sink → fallible cfg-gated `copy_to_clipboard` | ✓ WIRED | pbcopy / wl-copy→xclip, exit-status-checked (WR-02) |
| contents_formatted | remote parser | OSC8 re-emission in formatted/diff writers | ✓ WIRED | round-trip + pty parity tests green |
| baude-core re-export | downstream imports | `pub use vt100;` unchanged | ✓ WIRED | fork keeps crate name; zero import changes outside manifests |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|--------|-------------------|--------|
| LinkHints overlay row | `links[i].destination` | parser screen → intern table → `collect_links` at gesture time | Yes (tracer test end-to-end) | ✓ FLOWING |
| Opener argv | `url.as_str()` | same `DetectedLink.destination` (full normalized Url, not display truncation) | Yes (spy asserts byte-identical) | ✓ FLOWING |
| Copy sink input | full destination | same model value | Yes (copy spy asserts full URL) | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full workspace suite (single run, output saved) | `cargo test --workspace --locked` | exit 0; 612 passed / 0 failed | ✓ PASS |
| All named phase tests present in that run | grep of saved output | 20 fork tests, 36 baude link tests, pty parity, review-fix regressions — all `ok` | ✓ PASS |
| Real browser open / real clipboard | — | not runnable without side effects | ? SKIP → human (dogfood item) |

### Probe Execution

No `scripts/*/tests/probe-*.sh` probes exist in this project and none are declared by the phase plans — N/A.

### Requirements Coverage

All 8 phase requirement IDs are claimed by plans (no orphans: REQUIREMENTS.md maps exactly LINK-01..08 to Phase 10; every ID appears in at least one PLAN `requirements` field).

| Requirement | Source Plan(s) | Description | Status | Evidence |
|-------------|----------------|-------------|--------|----------|
| LINK-01 | 10-01, 10-02 | Labeled OSC8 opens target, not label | ✓ SATISFIED | target-not-label + rejoin tests; intern-table resolution |
| LINK-02 | 10-03 | Bare URLs incl. soft-wrapped, chars retained, punctuation trimmed | ✓ SATISFIED | 11 bare_url tests |
| LINK-03 | 10-02 | Metadata correct through 6 grid behaviors, local + remote | ✓ SATISFIED | 18 link_fidelity tests + pty snapshot parity |
| LINK-04 | 10-01, 10-04 | Documented gesture; non-interference; output alone never opens | ✓ SATISFIED* | help line + swallow/paste tests + selection tests; *universal-negative leg = backstop (human item) |
| LINK-05 | 10-01, 10-04 | Inspect actual destination pre-open | ✓ SATISFIED | overlay model carries full Url; truncation preserves scheme+host |
| LINK-06 | 10-04 | Copy destination without opening | ✓ SATISFIED | copy tests (opener spy uncalled); WR-02 honest failure surface |
| LINK-07 | 10-01, 10-03 | Non-activatable bad targets; validated http(s) only | ✓ SATISFIED | validate matrix; fail-closed collection |
| LINK-08 | 10-01, 10-04 | Argv-data opener; failure leaves session running | ✓ SATISFIED | spawn_opener shape; link_open tests |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| vendor/vt100/src/screen.rs | 1371 | `XXX` comment | ℹ️ Info | Upstream-inherited: identical comment exists verbatim in registry vt100 0.15.2 (screen.rs:1359, `sgr()` — unrelated to link code). Vendored-verbatim-by-design (T-10-SC); not phase-introduced debt, so not a blocker under the debt-marker gate |

No TODO/FIXME/TBD/placeholder/stub patterns in phase-authored code. The 10-01 `collect_bare_links` stub (WINDOWS entry 10) was closed by 10-03 and is marked fixed. Post-SUMMARY review fixes (CR-01, WR-01, WR-02, WR-03) all verified present in code with their regression tests green — the SUMMARYs' pre-fix state is superseded by commits dac2658/8f354a2/0849269/84e0c87 as delivered state.

### Human Verification Required

#### 1. Backstop: output can never activate (insufficient_spec)

**Test:** In a running baude session, `cat` a file containing hostile bytes (OSC8 sequences with control-laden/13+-semicolon/oversized URIs, bare URLs, escape soup) into a shell pane. Do not press ctrl+o.
**Expected:** Nothing opens, no process spawns, terminal renders normally; links appear only after ctrl+o and open only on Enter.
**Why human:** The truth is tagged `verification: backstop` (10-01 frontmatter). The structural single-entry proof and the swallow test are presence+wiring evidence, which by tier rule never verifies a universal negative over all byte sequences.

#### 2. Judgment-tier prohibitions (5 items)

**Test:** Review the five flagged prohibitions in the frontmatter with their cited evidence.
**Expected:** Accept the NON-AUTHORITATIVE pass verdicts (all five have direct code + green-test evidence) or reopen.
**Why human:** Judgment-tier prohibitions require explicit human resolution at the end-of-phase checkpoint — never a silent pass.

#### 3. Dogfood the real open/copy path (harvested from 10-04 Task 3 human-check; feeds SHIP-03)

**Test:** Run baude in a real terminal; in a shell pane run `printf '\e]8;;https://example.com\e\\click\e]8;;\e\\ and https://exa\nmple bare\n'`; press ctrl+o; try navigation, `c`, Enter, Esc; then verify text selection and drag-copy still work.
**Expected:** Overlay lists destinations (not "click"); `c` puts the URL on the real clipboard; Enter opens the real browser; selection/drag-copy unaffected.
**Why human:** Every automated test injects the opener/copy sinks by design (WINDOWS entry 6) — the real `open`/`pbcopy` handoff and visual overlay quality are only observable by a person.

### Gaps Summary

No gaps. Every artifact exists, is substantive, and is production-wired; all 24 merged must-have truths have direct codebase evidence, with 23 verified by passing named tests in a single full-suite run (612/612 green under `--locked`) and 1 (the backstop universal negative) routed to human verification by its declared tier rather than by any observed defect. All four post-SUMMARY review fixes are confirmed in code with regression tests. The phase goal — inspect, copy, and explicitly open validated HTTP(S) destinations with rendering, selection, and remote attachment staying authoritative and safe — is achieved pending the three human checks above.

---

_Verified: 2026-09-16T10:30:00Z_
_Verifier: Claude (gsd-verifier)_
