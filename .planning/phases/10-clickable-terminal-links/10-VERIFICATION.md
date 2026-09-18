---
phase: 10-clickable-terminal-links
verified: 2026-09-18T00:00:00Z
status: human_needed
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
covered_digest: "v1:sha256:33feb11a85c3fa42b26417a90a6222c6a926bc26480e0e9602b86ca564d39843"
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: passed
  previous_score: 23/24
  previous_verified: 2026-09-16T10:30:00Z
  trigger: "covered_digest went stale — 9 covered paths changed after it was written (2 planning docs, 7 source files), the largest source drift of the three re-verified phases"
  gaps_closed: []
  gaps_remaining: []
  regressions: []
  drift_reviewed:
    - "vendor/vt100/src/screen.rs — phase 11 kitty keyboard stacks: diff is PURELY ADDITIVE with respect to link machinery (no removed line matches link|osc|intern|clear); OSC 8 arm, intern table, fail-closed guards, Cell::clear erase-strip and wide-char spacer all byte-identical in behavior"
    - "baude-core/src/pty.rs — phase 11 prepended kitty flag re-emission to subscribe(); contents_formatted() still emitted (pty.rs:417), link parity test still green"
    - "baude/src/links.rs:201 — 12-01 clippy manual_flatten: behavior-preserving, covered by green span tests"
    - "baude/src/ui.rs:2120 — 12-01 clippy manual_clamp: behavior-preserving by algebraic identity (NOT test-covered; see advisory)"
    - "baude/src/ui.rs — phase 11 grew the help overlay 35→39 rows and moved the ctrl+o line 2209→2212; line still present, visibility threshold unchanged"
    - "baude/src/app.rs — phase 11 changes are confined to the key-encoding path (encode_key → forward_key), which sits DOWNSTREAM of the modal-first swallow at app.rs:3871; no link/paste/opener/Modal line touched"
    - "baude/src/main.rs, vendor/vt100/src/lib.rs — phase 11 only; no phase-10 surface"
prohibitions:
  - statement: "output alone never opens a link"
    tier: judgment
    llm_judge_verdict: "pass (NON-AUTHORITATIVE)"
    flagged: true
    human_disposition: "accepted by maintainer at the autonomous validation gate, 2026-09-16 ('All good — continue')"
    evidence: "re-confirmed at HEAD: spawn_opener invoked only at app.rs:4420 inside handle_link_hints_key; Modal::LinkHints assigned in production only at app.rs:5612 inside open_link_hints; open_link_hints called only from the ctrl+o chord (app.rs:3909-3915); all other Modal::LinkHints sites are inside #[cfg(test)] mod link_hints (5896+) / mod link_open (6402+); modal_open_swallows_every_key_from_the_child (incl. paste leg) green"
  - statement: "opener never receives shell code (argv data only)"
    tier: judgment
    llm_judge_verdict: "pass (NON-AUTHORITATIVE)"
    flagged: true
    human_disposition: "accepted by maintainer at the autonomous validation gate, 2026-09-16"
    evidence: "re-confirmed at HEAD: spawn_opener (app.rs:5734) is Command::new(OPENER).arg(url) with null stdio, no shell anywhere; links::tests::tracer_end_to_end spy asserts the exact argv string; validated URLs always start http(s):// so never parse as a flag"
  - statement: "non-HTTP(S)/malformed/control-bearing targets never activatable"
    tier: judgment
    llm_judge_verdict: "pass (NON-AUTHORITATIVE)"
    flagged: true
    human_disposition: "accepted by maintainer at the autonomous validation gate, 2026-09-16"
    evidence: "re-confirmed at HEAD: validate_http_url (links.rs:296-318) canonical-prefix + pre-decode + post-percent-decode control reject + scheme allowlist; 7 links::validate tests green; failed candidates never collected"
  - statement: "opener failure never kills the session"
    tier: judgment
    llm_judge_verdict: "pass (NON-AUTHORITATIVE)"
    flagged: true
    human_disposition: "accepted by maintainer at the autonomous validation gate, 2026-09-16"
    evidence: "activate_link (app.rs:5696) resolves both arms to set_message; link_open::opener_error_surfaces_one_warning_and_session_survives green"
  - statement: "link handling never breaks selection/drag-copy/child mouse"
    tier: judgment
    llm_judge_verdict: "pass (NON-AUTHORITATIVE)"
    flagged: true
    human_disposition: "accepted by maintainer at the autonomous validation gate, 2026-09-16; the 12-SMOKE-EVIDENCE macOS session legs 5 and 7 carry BULK ATTESTATION only, not leg-by-leg observation"
    evidence: "no mouse arm added or altered in the phase diff or in the post-verification drift; pre-existing selection/clipboard tests unmodified and green in the shared full-suite run (646 results, 0 failed, exit 0)"
coincidental_reliance_items:
  - truth: "contents_formatted round-trip → identical ids/targets in a fresh parser; pty.rs subscribe-snapshot parity (truth 7, LINK-03 remote leg)"
    reason: fixture-only
    harden: "Migrate pty::tests::subscribe_snapshot_replays_pre_attach_links (pty.rs:1146) onto the shared subscribe_snapshot_bytes helper (pty.rs:1205) that phase 11 introduced, or better, have it call the production subscribe() path. The test currently hand-mirrors subscribe()'s byte assembly inline, and phase 11 changed production without updating that mirror — so the test can no longer detect a regression in how subscribe() assembles the snapshot."
human_verification:
  - test: "Backstop (insufficient_spec): confirm no byte sequence arriving as terminal output can trigger link activation without the explicit gesture"
    expected: "cat a file of hostile OSC8/URL-bearing bytes into a pane; nothing opens, no process spawns; only ctrl+o + Enter opens anything"
    why_human: "universal negative tagged verification: backstop in 10-01 frontmatter — structural single-entry proof plus the swallow test are presence+wiring evidence, which never qualifies for a backstop-tier truth; no held-out/property test enumerates arbitrary output bytes"
    disposition: "ACCEPTED by maintainer at the autonomous validation gate, 2026-09-16 ('All good — continue'). Acceptance is a decision, not evidence: the universal negative remains unproven by test. Recorded, not upgraded."
  - test: "Judgment-tier prohibitions (5, listed in frontmatter): resolve each flagged must-NOT"
    expected: "human accepts the recorded NON-AUTHORITATIVE pass verdicts (evidence cited per item) or reopens"
    why_human: "judgment-tier prohibitions require explicit human resolution — never a silent pass"
    disposition: "ACCEPTED by maintainer at the autonomous validation gate, 2026-09-16. All five re-confirmed against code at this HEAD."
  - test: "Dogfood the real open/copy path: run baude in a real terminal, seed a labelled OSC 8 link plus a bare URL, press ctrl+o, then exercise j/k, c, Enter, Esc, and text selection"
    expected: "overlay lists destinations (not labels); c copies to the real clipboard; Enter opens the real browser; selection/drag-copy still work"
    why_human: "real browser launch, real clipboard, and visual overlay quality cannot be exercised by tests (every test injects sinks by design — WINDOWS entry 6)"
    disposition: "PARTIALLY OBSERVED, STILL OPEN. .planning/phases/12-validation-and-v2-2-0-release/12-SMOKE-EVIDENCE.md records a macOS 27.0 / iTerm2 3.6.9 session (commit e94aeed, dirty) explicitly labelled BULK ATTESTATION, not a leg-by-leg walkthrough. Leg 1 (ctrl+o opens the overlay with both seeded links rendered) is the ONE individually narrated leg, verbatim: 'yeah, ctrl o works'. Legs 2 (preview), 3 (copy), 4 (open) are covered only by the set-level 'all works flawlessly' judgement and carry two signed gaps that remain OPEN: (gap 1) the observer was asked three times whether the overlay row shows example.com/real-target rather than CLICK-ME and never answered — that is the load-bearing LINK-01/LINK-05 distinction, never visually confirmed; (gap 2) the observer's report that 'clicking opened links' is iTerm2's own URL detection, NOT baude's ctrl+o→Enter activation path, which ships keyboard-only. Neither gap is resolved here."
    residual: "Narrate leg 2 (label-vs-destination in the overlay row) and leg 4 (Enter on the overlay launching the real opener) individually in a real terminal."
---

# Phase 10: Clickable Terminal Links Verification Report

**Phase Goal:** Users can inspect, copy, and explicitly open validated HTTP(S) destinations while terminal rendering, selection, and remote attachment remain authoritative and safe.
**Verified:** 2026-09-18T00:00:00Z
**Status:** human_needed
**Re-verification:** **Yes — this is a RE-VERIFICATION of phase 10 at HEAD `ee6fa3c`, not the initial pass.**

## Why this re-verification ran

The prior report (2026-09-16, HEAD `8cd42fb`) went stale for a mechanical reason: its
`covered_digest` is computed over `covered_files`, and nine covered paths changed after it
was written — `.planning/REQUIREMENTS.md`, `.planning/WINDOWS.md`, and seven source files
(`baude-core/src/pty.rs`, `baude/src/app.rs`, `baude/src/links.rs`, `baude/src/main.rs`,
`baude/src/ui.rs`, `vendor/vt100/src/lib.rs`, `vendor/vt100/src/screen.rs`). Phase 10 carried
the largest source drift of the three phases re-verified in this pass, so it is where a
regression was most likely.

`8cd42fb` is still reachable from HEAD, so the drift was reviewed as a real diff rather than
inferred. Source at this HEAD is byte-identical to `origin/main` (`git diff --name-only
origin/main..HEAD` yields only a phase-12 planning doc) — this verifies shipped code.

Commit hashes cited in the phase SUMMARY and REVIEW-FIX files (`dac2658`, `8f354a2`,
`0849269`, `84e0c87`) are unreachable from HEAD: phase history was rebased during the
release. Every claim below is bound to **code at this HEAD plus a test run at this HEAD**,
never to a hash.

**Two corrections to the prior report's bookkeeping** (documentation accuracy, not gaps):
it claimed "18 fork `link_fidelity` tests" and "2 + 18"; the actual counts at HEAD are
**2** `hyperlink.rs` + **16** `link_fidelity.rs` = 18 fork link tests total. It claimed "36
baude link tests"; the actual count is **40**. It also recorded frontmatter `status: passed`
while its own body read `**Status:** human_needed` — the same inconsistency that propagated
out of the phase-9 report. Both fields in this report read `human_needed`.

## Drift review: did anything disturb a phase-10 guarantee?

**No regression found.** Each hazard was chased to code.

### Hazard 1 — phase 11's vt100 fork rewrite (`screen.rs`, +137 lines; `pty.rs`, +156)

Filtering the `screen.rs` diff for link-relevant tokens (`link|osc|intern|clear`) returns
exactly **two** lines, both additions: `self.kitty_alternate_stack.clear()` and a comment
noting the kitty code "copies the OSC 8 fail-closed posture". **Zero removed lines touch
link machinery.** The change is additive: a `KITTY_STACK_MAX` const, two `Vec<u16>` fields on
`Screen`, and their accessors. Every phase-10 mechanism re-read and confirmed intact at HEAD:

| Mechanism | Location at HEAD | State |
|---|---|---|
| OSC 8 arm, `params[2..].join(&b';')` URI rejoin | screen.rs:1805-1839 | Intact |
| `VTE_MAX_OSC_RAW = 1024` fail-closed guard | screen.rs:1814, 1830 | Intact |
| `VTE_MAX_OSC_PARAMS = 16` fail-closed guard (**CR-01**) | screen.rs:1826, 1831 | Intact |
| Empty-URI closes the run | screen.rs:1829 (`uri.is_empty()` → `attrs.link = None`) | Intact |
| Link intern table + `MAX_LINK_URI_LEN`/`MAX_LINKS` caps | screen.rs:12, 17, 614-627 | Intact |
| `Screen::link_target(id)` | screen.rs:603 | Intact |
| `Cell::clear` unconditional erase-strip | cell.rs:60-68 | Intact |
| Wide-char continuation spacer carries the base link id (**WR-03**) | screen.rs:1132 `next_cell.set_link(attrs.link)` | Intact |
| SGR reset must not close a hyperlink | screen.rs:1494 | Intact |
| OSC 8 re-emission threaded through `contents_formatted`/diff writers | attrs.rs:97-105, row.rs, grid.rs | Intact |

Production `subscribe()` still emits `screen.contents_formatted()` (pty.rs:417); phase 11
only **prepended** kitty flag sequences ahead of it.

### Hazard 2 — plan 12-01's clippy fixes inside phase-10 code

Both edits are behavior-preserving. Verified, not assumed:

- **`links.rs:201` (`manual_flatten`).** `for cell in &cells[i..end] { if let Some((r,c)) = cell { … } }`
  became `for (r, c) in cells[i..end].iter().flatten() { … }`. `&Option<T>` is
  `IntoIterator<Item = &T>`, so `flatten()` yields exactly the `Some` payloads in the same
  order — identical to the `if let` it replaced. The enclosing `end_col` walk is the LINK-02
  span anchor, and it is test-covered: `detects_url_embedded_in_prose_with_span`,
  `wide_char_label_records_full_span` (span `(0,0,3)`), `soft_wrapped_url_joined_via_row_wrapped`,
  and `offscreen_tail_joined_via_bounded_continuation` are all green at this HEAD.
- **`ui.rs:2120` (`manual_clamp`).** `links.len().min(10).max(1)` became `links.len().clamp(1, 10)`.
  For every `usize x`: `max(min(x,10),1)` and `clamp(1,10)` agree at `x = 0` (both `1`),
  `1 ≤ x ≤ 10` (both `x`), and `x > 10` (both `10`); the bounds are constants with `1 ≤ 10`,
  so the `clamp` panic path is unreachable. Identity holds on the whole domain. **Honest
  caveat:** no test renders the `Modal::LinkHints` draw arm (ui.rs:2114), so this one is
  verified by proof, not by test. Truth 22's substance — that the *model, copy, and open* use
  the full URL while only display truncates — is separately test-covered via
  `display_truncated_width` (3 green tests).

### Hazard 3 — phase 11's `app.rs` and help-overlay edits

The `app.rs` diff since `8cd42fb` contains **no** line matching `link|paste|opener|clipboard|Modal::`;
every removed line is in the key-encoding path (`encode_key` → kitty-aware `forward_key`).
That path sits **downstream** of the modal-first swallow: `handle_key` routes to
`handle_modal_key` at app.rs:3871-3872, before the ctrl+o chord (3909) and before any
`forward_key` call (3918-3920). So phase 11 cannot leak a byte past an open overlay, and the
behavioral test confirms it.

The help overlay grew from 35 to 39 rows and gained two `shift+enter` lines, moving
`ctrl+o      link hints (inspect/copy/open urls)` from ui.rs:2209 to **ui.rs:2212**. The line
is still present and is paragraph line **26 of 37**, unchanged by the additions (they land
after it), so the terminal height at which it clips (`area.height ≥ 28`) is **not** a
regression. Noting honestly: no test asserts this line — truth 5 rests on presence, as it did
before.

### One NEW finding (advisory, not a regression)

`pty::tests::subscribe_snapshot_replays_pre_attach_links` (pty.rs:1146) — the LINK-03 remote
leg — **hand-mirrors** `subscribe()`'s byte assembly inline in its own body. Phase 11 changed
production `subscribe()` to prepend kitty flag re-emission and introduced a shared helper
`subscribe_snapshot_bytes` (pty.rs:1205) for its *own* new tests, but left the phase-10 test
on its now-stale private copy.

Link parity is **not** affected: the added bytes are prepended kitty sequences, and this
fixture pushes no kitty flags, so `kitty_main_stack()` is empty and production and the mirror
emit identical bytes. The test also exercises the *real* `contents_formatted()` and a *real*
fresh `vt100::Parser`, which is the part that matters for LINK-03. The test is green.

What it can no longer do is catch a regression in how production `subscribe()` assembles the
snapshot — if `subscribe()` stopped calling `contents_formatted()`, this test would still
pass. That is `coincidental-reliance: fixture-only`, recorded as advisory with a hardening
recommendation, exactly as the tier rules require. Truth 7 stays VERIFIED (the link behavior
is directly exercised) with the reliance flag attached.

## Goal Achievement

Shared full-suite evidence at this exact HEAD (`ee6fa3c`), run once by the orchestrator and
cited rather than re-run: `cargo test --workspace --locked -- --test-threads=1` → **exit 0,
646 test results, 0 failed**; `cargo fmt --check` → exit 0; `cargo clippy --workspace
--all-targets -- -D warnings` → exit 0. Targeted runs performed by this verification are
named per row below.

### ROADMAP Success Criteria

| # | Success Criterion | Status | Evidence at HEAD `ee6fa3c` |
|---|-------------------|--------|----------|
| 1 | Labeled OSC8 links open their actual HTTP(S) destination; preview/copy before opening | ✓ VERIFIED | `cargo test -p vt100`: `hyperlink::target_not_label_and_wrap_survival` green (destination resolved via `Screen::link_target`, never cell text); `cargo test -p baude link`: `links::tests::tracer_end_to_end` green (spy receives exactly the parsed URI as one argv string), `c_copies_full_destination_without_opening` green; overlay renders `Url::as_str()` (ui.rs:2114) |
| 2 | Bare HTTP(S) URLs, incl. soft-wrapped, retain valid characters without prose punctuation | ✓ VERIFIED | 11 `links::bare_url` tests green incl. `soft_wrapped_url_joined_via_row_wrapped`, `explicit_newline_is_never_joined`, `unbalanced_trailing_paren_stripped_balanced_retained`, `trailing_prose_punctuation_stripped`, `offscreen_tail_joined_via_bounded_continuation`, `osc8_run_cells_excluded_from_bare_pass`; join keys on `row_wrapped` only, no column heuristic |
| 3 | Link metadata attached to correct cells through scroll/scrollback/wrap/resize/overwrite/erasure — local and attached remote | ✓ VERIFIED | `cargo test -p vt100`: all 16 `link_fidelity` tests green (`scroll_into_scrollback_retains_link`, `wrapped_fragments_share_one_id`, `resize_keeps_surviving_ids`, `overwrite_with_plain_text_clears_link`, `erase_screen_leaves_no_link_ids`, `erase_line_leaves_no_link_ids`, `wide_char_label_keeps_contiguous_link_ids`, 3 `formatted_round_trip_*`, caps and param-saturation); `cargo test -p baude-core subscribe`: `subscribe_snapshot_replays_pre_attach_links` green (remote leg — see advisory) |
| 4 | Documented explicit gesture activates; selection/drag-copy/scrolling/child mouse stay usable; output alone never opens | ✓ VERIFIED* | Help line present at ui.rs:2212; chord at app.rs:3909-3915 is the sole entry into hint mode; `modal_open_swallows_every_key_from_the_child` green (with the WR-01 paste leg and a non-vacuous post-close forwarding control); modal-first routing precedes phase 11's new `forward_key`; selection/clipboard tests unmodified and green in the shared run. *The universal-negative leg is the declared backstop — routed to human, human-accepted 2026-09-16 |
| 5 | Unsupported/malformed/control targets non-activatable; allowed targets passed as data; failure leaves session running with useful error | ✓ VERIFIED | 7 `links::validate` tests green (schemes, raw and percent-encoded controls, malformed spellings — all fail closed); `spawn_opener` (app.rs:5734) is `Command::new(OPENER).arg(url)` with null stdio, no shell; `opener_error_surfaces_one_warning_and_session_survives` green |

### Observable Truths (merged from 4 PLAN frontmatters, deduped — full list carried forward, nothing dropped)

| # | Truth (source) | Status | Evidence at this HEAD |
|---|----------------|--------|----------|
| 1 | ctrl+o over an OSC8-bearing pane opens LinkHints with parsed destination, never the label (10-01) | ✓ VERIFIED | Chord app.rs:3909 → `open_link_hints` (5577) → `collect_links` → `Modal::LinkHints` (5612); `tracer_end_to_end` + `target_not_label_and_wrap_survival` green |
| 2 | Enter passes validated URL as one argv arg to injected opener; Ok and Err both set_message, session runs (10-01) | ✓ VERIFIED | `activate_link` (app.rs:5696) both arms set_message; `opener_ok_sets_opening_message_with_display_form` + `opener_error_surfaces_one_warning_and_session_survives` green |
| 3 | Empty-URI OSC8 closes the run; empty targets never activatable (10-01/10-02) | ✓ VERIFIED | screen.rs:1829 `uri.is_empty()` → `attrs.link = None`; `link_fidelity::empty_uri_close_ends_run` green |
| 4 | Only validate_http_url-accepted targets activatable (10-01) | ✓ VERIFIED | Every candidate gated in `collect_links`; `validate_http_url` (links.rs:296-318) re-read; 7-test matrix green |
| 5 | Chord documented in help overlay global section (10-01) | ✓ VERIFIED | ui.rs:2212 exact line present (moved from 2209 by phase 11's overlay growth; clipping threshold unchanged). Presence-only evidence — no test asserts it |
| 6 | Link ids correct through scroll/scrollback/wrap/resize/overwrite; erased cells carry none (10-02) | ✓ VERIFIED | 6 grid-fidelity + 2 erase tests green; `Cell::clear` (cell.rs:67) strips unconditionally at the single choke point |
| 7 | contents_formatted round-trip → identical ids/targets in fresh parser; pty.rs snapshot parity (10-02) | ✓ VERIFIED (coincidental-reliance) | 3 `formatted_round_trip_*` + `subscribe_snapshot_replays_pre_attach_links` green. Reliance flag: the pty test hand-mirrors `subscribe()` rather than calling it, and phase 11 changed production without updating the mirror — see advisory and `coincidental_reliance_items` |
| 8 | Semicolon-bearing URI reconstructed intact (10-01/10-02) | ✓ VERIFIED | `params[2..].join(&b';')` at screen.rs:1828; `hyperlink::semicolon_uri_rejoined` + `link_fidelity::multi_semicolon_uri_rejoined_intact` green |
| 9 | Over-cap URI / full table / truncated OSC → link None, parse continues, bounded memory (10-02) | ✓ VERIFIED | `intern_link` caps (screen.rs:614-627) + both fail-closed guards (screen.rs:1830-1831, CR-01 present); `uri_over_2083_bytes_is_not_a_link`, `intern_table_cap_stops_new_entries`, `param_saturated_uri_is_not_a_link`, `uri_below_param_cap_still_interns_intact` green |
| 10 | Soft-wrapped bare URL detected as ONE URL via row_wrapped only (10-03) | ✓ VERIFIED | Logical-line builder flushes at `!row_wrapped`; wrap-join + `explicit_newline_is_never_joined` green |
| 11 | Unbalanced trailing punctuation stripped; balanced closers retained (10-03) | ✓ VERIFIED | `trim_trailing_punctuation` with open/close balance count; both tests green |
| 12 | Control chars rejected pre-parse AND post-percent-decode (10-03) | ✓ VERIFIED | links.rs:307-315 (both checks re-read at HEAD); `rejects_raw_control_chars_and_whitespace`, `rejects_percent_encoded_controls_post_decode` green |
| 13 | Valid percent-encoded/UTF-8 URLs survive; collected destination = normalized parse shown and opened (10-03) | ✓ VERIFIED | `percent_encoded_utf8_survives_detection_intact`, `accepted_normalized_form_is_returned` green |
| 14 | Non-http(s) schemes never collected; empty rejected (10-03) | ✓ VERIFIED | `rejects_non_http_schemes`, `rejects_empty_and_malformed` green; scheme allowlist at links.rs:317 |
| 15 | OSC8-run cells excluded from bare pass — one link, not two (10-03) | ✓ VERIFIED | `osc8_run_cells_excluded_from_bare_pass` green |
| 16 | c/y copies actual destination without opening; Enter opens; Esc dismisses (10-04) | ✓ VERIFIED | `c_copies_full_destination_without_opening`, `y_copies_selected_entry`, `enter_opens_selected_entry_only`, `esc_dismisses`, `copy_error_surfaces_failure_not_success` (WR-02 fallible sink) all green |
| 17 | Every key (and paste — WR-01) while overlay open handled/swallowed; no byte reaches child (10-04) | ✓ VERIFIED | `handle_key` modal-first at app.rs:3871; `handle_paste` early-return at app.rs:3986 (`!matches!(self.modal, Modal::None \| Modal::Input {..})`) — WR-01 present; `modal_open_swallows_every_key_from_the_child` + `unhandled_keys_are_swallowed` green (live channel spy) |
| 18 | Zero links → message, no modal (10-04) | ✓ VERIFIED | `chord_with_zero_links_sets_message_and_restores_bracket`, `chord_without_session_sets_message` green |
| 19 | Gesture collects OSC8 + bare at current scroll offset, set_scrollback bracket, lock dropped before modal mutation (10-04) | ✓ VERIFIED | `chord_at_scrolled_offset_collects_viewed_rows_and_restores_bracket`, `chord_collects_both_passes_ordered_top_to_bottom` green |
| 20 | Chord resolves focused pane's parser for local Claude, Shell, AND active remote attach — render-parity filter (10-04) | ✓ VERIFIED | `chord_ignores_attach_for_a_different_remote` green |
| 21 | Opener spawn error → one set_message naming failure; session keeps running (10-04) | ✓ VERIFIED | `opener_error_surfaces_one_warning_and_session_survives` green (asserts a subsequent event is still processed) |
| 22 | Display middle-truncated only; scheme+host always visible; copy/open use full URL (10-04) | ✓ VERIFIED | `truncation_is_noop_when_url_fits`, `truncation_preserves_scheme_and_host_at_narrow_width`, `truncation_below_prefix_width_still_shows_scheme_and_host` green; model stores the full `Url` (app.rs:5747-5756) |
| 23 | Wide-char labels keep contiguous ids / full span (WR-03, post-review edge lift) | ✓ VERIFIED | `next_cell.set_link(attrs.link)` at screen.rs:1132 (WR-03 present); `wide_char_label_keeps_contiguous_link_ids` + `links::tests::wide_char_label_records_full_span` green |
| 24 | **Backstop:** no byte sequence arriving as output can trigger activation without the gesture (10-01, `verification: backstop`) | ⚠️ insufficient_spec | Structural single-entry proof re-confirmed at HEAD (production `Modal::LinkHints` assignment exists only at app.rs:5612; every other site is inside a `#[cfg(test)]` module), but this is presence+wiring, which never verifies a universal negative. Human-ACCEPTED 2026-09-16 — a decision, not evidence, so it does not count toward the score |

**Score:** 23/24 truths verified (0 present-but-behavior-unverified; 1 backstop item held open by its declared tier, not by any observed defect). One verified truth carries an advisory `coincidental-reliance` flag, which changes neither score nor status.

### Review-fix persistence check (all four still shipped)

| Fix | Requirement | Location at HEAD | Regression test | State |
|---|---|---|---|---|
| CR-01 | Fail closed on vte param saturation | screen.rs:1826 (`VTE_MAX_OSC_PARAMS = 16`), 1831 (guard) | `param_saturated_uri_is_not_a_link` + `uri_below_param_cap_still_interns_intact` green | ✓ Present |
| WR-01 | Paste swallowed while a modal is open | app.rs:3979-3988 | `modal_open_swallows_every_key_from_the_child` (paste leg) green | ✓ Present |
| WR-02 | Fallible cfg-gated clipboard sink | `copy_to_clipboard` wired at app.rs:4420 and 5510 | `copy_error_surfaces_failure_not_success` green | ✓ Present |
| WR-03 | Wide-char continuation carries the link id | screen.rs:1132, `Cell::set_link` at cell.rs:125 | `wide_char_label_keeps_contiguous_link_ids` + `wide_char_label_records_full_span` green | ✓ Present |

### Required Artifacts

| Artifact | Expected | Status | Details at HEAD |
|----------|----------|--------|---------|
| `vendor/vt100/` | Workspace-member fork: OSC8 arm, `Attrs.link`, intern table, `link_id()`, `link_target()` | ✓ VERIFIED | All present; BAUDE FORK markers intact; phase 11's kitty additions did not remove or alter any link-machinery line |
| `baude/src/links.rs` | `DetectedLink`, `collect_links` (both passes), `validate_http_url` | ✓ VERIFIED | Substantive; both passes complete; clippy edit at :201 behavior-preserving and span-test-covered |
| `baude/src/app.rs` link surface | `Modal::LinkHints`, ctrl+o chord, `handle_link_hints_key` (dual injected sinks), `spawn_opener`, `open_link_hints` | ✓ VERIFIED | Production-wired at 3909 / 4420 / 5577 / 5612 / 5625 / 5696 / 5734; untouched by phase 11 |
| `baude/src/ui.rs` | LinkHints draw arm + help line | ✓ VERIFIED | Draw arm at 2114 (clamp edit behavior-preserving); help line at 2212 |
| `vendor/vt100/README.md` | Provenance: upstream 0.15.2, diff surface | ✓ VERIFIED | Unchanged since prior verification |
| `.planning/WINDOWS.md` entry 6 | Opener injected-by-construction disposition | ✓ VERIFIED | Entry present |
| Fork test suites | `hyperlink.rs`, `link_fidelity.rs` | ✓ VERIFIED | 2 + 16 = 18 tests, all green (prior report's "2 + 18" was a miscount) |
| `baude-core/src/pty.rs` test | subscribe-snapshot link parity | ⚠️ VERIFIED with reliance flag | `subscribe_snapshot_replays_pre_attach_links` at pty.rs:1146, green — but mirrors `subscribe()` inline instead of calling it, and the mirror is now stale relative to production |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| ctrl+o chord | `Modal::LinkHints` | `open_link_hints` → `collect_links` | ✓ WIRED | app.rs:3913 → 5577 → 5612; **sole** production entry re-proven: `mod link_hints` starts at 5896 and `mod link_open` at 6402, so every other `Modal::LinkHints` site is test-only |
| `Attrs.link` | destination string | Screen intern table → `link_target(id)` | ✓ WIRED | screen.rs:603/614; cell text never consulted |
| Enter | OS opener | validate-gated → `activate_link(spawn_opener)` → `Command::new(OPENER).arg(url)` | ✓ WIRED | app.rs:4420 passes `spawn_opener` + `Self::copy_to_clipboard` in production; tests inject spies |
| c/y | clipboard | injected copy sink → fallible cfg-gated `copy_to_clipboard` | ✓ WIRED | Status-checked; failure surfaces (WR-02) |
| `contents_formatted` | remote parser | OSC 8 re-emission in formatted/diff writers | ✓ WIRED | attrs.rs:97-105 threaded through row.rs/grid.rs; production `subscribe()` still calls it at pty.rs:417 |
| baude-core re-export | downstream imports | `pub use vt100;` unchanged | ✓ WIRED | Fork keeps the crate name; zero import changes |
| modal-first swallow | phase 11 `forward_key` | `handle_key` routes modal before any forward | ✓ WIRED | app.rs:3871-3872 precedes 3918-3920 — phase 11's new encoder is downstream of the swallow |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|--------|-------------------|--------|
| LinkHints overlay row | `links[i].destination` | parser screen → intern table → `collect_links` at gesture time | Yes (`tracer_end_to_end`) | ✓ FLOWING |
| Opener argv | `url.as_str()` | same `DetectedLink.destination` (full normalized `Url`, not the display truncation) | Yes (spy asserts byte-identical) | ✓ FLOWING |
| Copy sink input | full destination | same model value | Yes (copy spy asserts the full URL) | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Fork link machinery after phase 11's rewrite | `cargo test -p vt100 --locked` | 2 hyperlink + 16 link_fidelity + 14 kitty + 1 doc-test, all `ok`, 0 failed | ✓ PASS |
| Detection, validation, overlay, open/copy after 12-01's clippy edits | `cargo test -p baude --locked link` | 40 passed, 0 failed | ✓ PASS |
| Remote snapshot link parity after phase 11's `subscribe()` change | `cargo test -p baude-core --locked subscribe` | 6 passed, 0 failed, incl. `subscribe_snapshot_replays_pre_attach_links` | ✓ PASS |
| Overlay swallow (WR-01) after phase 11's key-encoding change | `cargo test -p baude --locked --bins modal` | 2 passed, 0 failed | ✓ PASS |
| Workspace suite / fmt / clippy at this HEAD | shared orchestrator run (cited, not re-run) | exit 0; 646 results, 0 failed; fmt 0; clippy `-D warnings` 0 | ✓ PASS |
| `clamp` identity at `ui.rs:2120` | algebraic proof over `usize` | `max(min(x,10),1) == clamp(1,10)` on the whole domain | ✓ PASS (proof, no test) |
| Real browser open / real clipboard / overlay visual quality | — | not runnable without side effects | ? SKIP → human (dogfood item, partially observed) |

### Probe Execution

No `scripts/*/tests/probe-*.sh` probes exist in this project and none are declared by the phase plans — N/A.

### Requirements Coverage

All 8 phase requirement IDs are claimed by plans; REQUIREMENTS.md maps exactly LINK-01..08 to Phase 10 and every ID appears in at least one PLAN `requirements` field. No orphans.

| Requirement | Source Plan(s) | Status | Evidence at this HEAD |
|-------------|----------------|--------|----------|
| LINK-01 | 10-01, 10-02 | ✓ SATISFIED | `target_not_label_and_wrap_survival`, `semicolon_uri_rejoined`, intern-table resolution. Honest note: the human-observable leg of this guarantee (overlay row shows the target, not `CLICK-ME`) is smoke-evidence gap 1 and remains un-narrated |
| LINK-02 | 10-03 | ✓ SATISFIED | 11 `bare_url` tests; clippy edit at links.rs:201 behavior-preserving |
| LINK-03 | 10-02 | ✓ SATISFIED | 16 `link_fidelity` tests + pty snapshot parity; phase 11's fork rewrite left every mechanism intact. Remote-leg test carries a fixture-only reliance flag |
| LINK-04 | 10-01, 10-04 | ✓ SATISFIED* | Help line ui.rs:2212 + swallow/paste tests + unmodified selection tests; modal-first routing precedes phase 11's encoder. *Universal-negative leg = backstop (human-accepted) |
| LINK-05 | 10-01, 10-04 | ✓ SATISFIED | Model carries the full `Url`; truncation preserves scheme+host (3 tests) |
| LINK-06 | 10-04 | ✓ SATISFIED | Copy tests with the opener spy uncalled; WR-02 honest failure surface |
| LINK-07 | 10-01, 10-03 | ✓ SATISFIED | 7-test validate matrix; fail-closed collection; both vte truncation vectors guarded |
| LINK-08 | 10-01, 10-04 | ✓ SATISFIED | `spawn_opener` argv-data shape; `link_open` tests |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| vendor/vt100/src/screen.rs | 1490 (was 1371) | `XXX` comment | ℹ️ Info | Upstream-inherited, re-confirmed this pass: `git log -L1490,1490` attributes the line to `a60fb6c test(10-01): add failing tracer tests…`, the commit that **created** the file as a verbatim vendored import. It sits in upstream's `sgr()`, unrelated to link code. Vendored-verbatim-by-design (T-10-SC); not phase-introduced debt, so not a blocker under the debt-marker gate. Line number moved only because phase 11 inserted code above it |

No `TODO`/`FIXME`/`TBD`/`HACK`/`PLACEHOLDER`/"not yet implemented" markers in any phase-10 file (`links.rs`, `ui.rs`, `app.rs`, `pty.rs`, and all six fork source files scanned). The 10-01 `collect_bare_links` stub (WINDOWS entry 10) was closed by 10-03 and stays closed.

### Human Verification Required

Three items, all carried forward. Two were dispositioned by the maintainer; the third is partially observed and stays open.

#### 1. Backstop: output can never activate (insufficient_spec) — ACCEPTED, not proven

**Test:** In a running baude session, `cat` a file of hostile bytes (OSC 8 sequences with control-laden / 13+-semicolon / oversized URIs, bare URLs, escape soup) into a shell pane. Do not press ctrl+o.
**Expected:** Nothing opens, no process spawns; links appear only after ctrl+o and open only on Enter.
**Why human:** Tagged `verification: backstop` in 10-01 frontmatter. The structural single-entry proof (re-confirmed at this HEAD) and the swallow test are presence+wiring evidence, which by tier rule never verifies a universal negative over all byte sequences.
**Disposition:** ACCEPTED by the maintainer at the autonomous validation gate on 2026-09-16 ("All good — continue"). Recorded as a decision, not as evidence — the item is closed by authority, not by test, and so is not counted toward the score.

#### 2. Judgment-tier prohibitions (5 items) — ACCEPTED

**Test:** Review the five flagged prohibitions in the frontmatter with their cited evidence.
**Expected:** Accept the NON-AUTHORITATIVE pass verdicts or reopen.
**Why human:** Judgment-tier prohibitions require explicit human resolution — never a silent pass.
**Disposition:** ACCEPTED by the maintainer on 2026-09-16. All five re-confirmed against code at this HEAD (evidence strings in the frontmatter were rewritten to cite HEAD line numbers, not the stale ones). Note on prohibition 5: its human-observable legs (smoke legs 5 and 7, selection and mouse) carry bulk attestation only.

#### 3. Dogfood the real open/copy path — PARTIALLY OBSERVED, STILL OPEN

**Test:** Run baude in a real terminal; seed a labelled OSC 8 link plus a bare URL in a pane; press ctrl+o; exercise j/k, `c`, Enter, Esc; then confirm text selection and drag-copy still work.
**Expected:** Overlay lists destinations (not the label); `c` puts the URL on the real clipboard; Enter opens the real browser; selection/drag-copy unaffected.
**Why human:** Every automated test injects the opener and copy sinks by design (WINDOWS entry 6) — the real `open`/`pbcopy` handoff and the overlay's visual quality are only observable by a person.

**What the smoke evidence actually establishes.** `.planning/phases/12-validation-and-v2-2-0-release/12-SMOKE-EVIDENCE.md` records a macOS 27.0 (arm64) / iTerm2 3.6.9 session at commit `e94aeed` (dirty: GSD orchestrator state only). Its own header labels the evidence basis **BULK ATTESTATION — "This is not a leg-by-leg walkthrough."** Read honestly:

- **Leg 1 (detect) is the one individually narrated leg.** Observer, verbatim: *"yeah, ctrl o works"*, plus confirmation that both seeded lines rendered (*"thos eboth worked"*). The overlay's footer text and link count were not separately reported. This genuinely advances the dogfood item's first third.
- **Legs 2, 3, 4 (preview, copy, open) rest on the set-level judgement "all works flawlessly"** and are not transcribed into per-leg observations. Two signed gaps remain **open** and are not resolved by this verification:
  - **Gap 1 — the LINK-01 distinction was never narrated.** The observer was asked three separate times whether the overlay row shows `example.com/real-target` or `CLICK-ME`, and answered about other things each time. This is precisely the property LINK-01 exists to guarantee. It is covered by green automated tests and by the set-level attestation, but it was **not visually confirmed**. Signed off by Joe Seymour, 2026-09-16.
  - **Gap 2 — "clicking opened links" is iTerm2, not baude.** The observer reported that clicking opened links. iTerm2 opens detected URLs on click regardless of the inner application, and baude ships **no** mouse link activation — phase 10 shipped keyboard-only (`ctrl+o` → overlay → Enter). This is explicitly **not** leg-4 evidence. Signed off by Joe Seymour, 2026-09-16.
- The Linux session is PENDING/DEFERRED with sign-off; legs 1-4 there cite `check (ubuntu-22.04)` CI as an automated proxy, not a live observation.

**Residual ask:** narrate leg 2 (label-vs-destination in the overlay row) and leg 4 (Enter on the overlay launching the real opener) individually in a real terminal.

### Advisory (New Scope, Unevidenced)

| # | Finding | Category | Why Advisory |
|---|---------|----------|--------------|
| 1 | `pty::tests::subscribe_snapshot_replays_pre_attach_links` (pty.rs:1146) hand-mirrors `subscribe()`'s byte assembly inline. Phase 11 changed production `subscribe()` (prepended kitty flag re-emission) and added a shared `subscribe_snapshot_bytes` helper (pty.rs:1205) for its own tests, but left this phase-10 test on its stale private copy. The test therefore cannot detect a regression in how production assembles the snapshot | architectural | Not a regression and not blocking: the added bytes are prepended kitty sequences, this fixture pushes no kitty flags, and the test does exercise the real `contents_formatted()` and a real fresh parser — the LINK-03-relevant behavior is directly proven and green. The weakness is evidentiary coupling, recorded as `coincidental-reliance: fixture-only` with a named hardening step, per the tier rules |
| 2 | No test renders the `Modal::LinkHints` draw arm (ui.rs:2114), so 12-01's `manual_clamp` edit at ui.rs:2120 is verified by algebraic proof rather than by test. Likewise, no test asserts the `ctrl+o` help line at ui.rs:2212 (truth 5 is presence-only) | other | Both are provable/observable without a test and neither shows a defect. Raised so a future overlay edit is known to be uncovered at the render level. Cheap fix: a `render(&app, …)` test for the LinkHints arm, mirroring the `help_overlay_lists_shift_enter` guard phase 11 added |

### Deferred Items

None. No phase-10 concern is scheduled into a later phase; the milestone is at its release phase.

### Gaps Summary

**No gaps, and no regression.** This was the pass most likely to find one — phase 10 carried
the largest post-verification source drift, and two independent hazards were aimed straight at
its guarantees — so the negative result is stated with its reasoning shown rather than
asserted.

Phase 11's vt100 fork rewrite is additive with respect to the link machinery: filtering the
`screen.rs` diff for link-relevant tokens returns two additions and zero removals, and every
one of the ten phase-10 fork mechanisms (OSC 8 arm and `;`-rejoin, both fail-closed truncation
guards, empty-URI close, intern table and its two caps, `link_target`, `Cell::clear`
erase-strip, the WR-03 wide-char spacer, the SGR-reset carve-out, and the
`contents_formatted` re-emission thread) was re-read at HEAD and confirmed intact, with all 18
fork link tests green. Phase 11's `app.rs` change is confined to the key-encoding path, which
sits downstream of the modal-first swallow at app.rs:3871 — it cannot leak a byte past an open
overlay, and the swallow test confirms that behaviorally. Plan 12-01's two clippy edits inside
phase-10 code are behavior-preserving in fact, not merely in intent: the `manual_flatten`
rewrite is semantically identical and covered by four green span tests, and the `manual_clamp`
rewrite is an identity on the entire `usize` domain. All four post-SUMMARY review fixes
(CR-01, WR-01, WR-02, WR-03) are still present in code at named lines with their regression
tests green.

The only new finding is evidentiary, not behavioral: the LINK-03 remote-parity test duplicates
production's snapshot assembly instead of calling it, and phase 11 widened that duplication
without noticing. The link behavior it guards is still directly and correctly exercised, so
truth 7 stays verified — with a `coincidental-reliance` flag and a one-line hardening step,
which is the honest disposition rather than either a silent pass or a manufactured blocker.

Status is `human_needed`, not `passed`, because human items remain. Two of the three were
dispositioned by the maintainer on 2026-09-16 and are recorded as accepted — a decision, which
is not the same thing as evidence, so the backstop still does not count toward the score
(23/24, unchanged). The third, the real-terminal dogfood, is now **partially** observed: the
macOS smoke session genuinely narrated leg 1 (`ctrl+o` opens the overlay), but it is a labeled
bulk attestation for legs 2-4 and carries two signed, still-open gaps that bear directly on
phase 10 — the label-vs-destination distinction at the heart of LINK-01 was asked three times
and never narrated, and the maintainer's "clicking opened links" is iTerm2's own URL handling
rather than baude's keyboard activation path. Neither gap is upgraded or quietly closed here.

---

_Verified: 2026-09-18T00:00:00Z_
_Verifier: Claude (gsd-verifier) — re-verification at HEAD ee6fa3c_
