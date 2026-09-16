---
phase: 10-clickable-terminal-links
reviewed: 2026-09-16T08:47:13Z
depth: standard
files_reviewed: 11
files_reviewed_list:
  - baude-core/src/pty.rs
  - baude/src/app.rs
  - baude/src/links.rs
  - baude/src/main.rs
  - baude/src/remote.rs
  - baude/src/ui.rs
  - vendor/vt100/src/attrs.rs
  - vendor/vt100/src/cell.rs
  - vendor/vt100/src/grid.rs
  - vendor/vt100/src/parser.rs
  - vendor/vt100/src/screen.rs
findings:
  critical: 1
  warning: 3
  info: 6
  total: 10
status: issues_found
---

# Phase 10: Code Review Report

**Reviewed:** 2026-09-16T08:47:13Z
**Depth:** standard
**Files Reviewed:** 11
**Status:** issues_found

## Summary

Reviewed the phase-10 clickable-terminal-links diff (`55e3193..HEAD`) plus the
vendored vt100 fork diffed against upstream 0.15.2 from the cargo registry
(`row.rs` and `term.rs` were also diffed: formatting + link-table threading
only, no logic change beyond the threaded `links` parameter). Findings were
verified empirically where possible: a scratchpad probe crate against the
vendored vt100 confirmed both the CR-01 truncation bypass and the WR-03
wide-char span defect; `cargo test -p baude link`, the pty snapshot tests, and
the vt100 crate tests all pass.

Security invariants checked adversarially:

- **Modal interception**: ctrl+o is the only entry; `handle_key` routes to the
  modal before global chords; mouse events are fully swallowed while any modal
  is open (app.rs:5372). Holds for keys and mouse — but NOT for paste (WR-01).
- **Argv-only opener**: `spawn_opener` uses `Command::new(OPENER).arg(url)`
  with null stdio — no shell anywhere. URLs cannot start with `-` (validated
  `http(s)://` prefix), so no argv-option injection. Holds.
- **Validation gate**: `validate_http_url` requires the canonical raw
  `http(s)://` prefix (single-slash spellings rejected), rejects control
  chars/whitespace pre-decode and control chars post-percent-decode, and
  allowlists the parsed scheme. Exercised via the LINK-07 matrix tests. Holds.
- **Erase/overwrite strips link ids**: `Cell::clear` unconditionally strips
  `attrs.link`, covering every erase/fill call site (verified ech/el/ed and
  wide-char neighbor clears at screen.rs:954/1027 route through it). Holds.
- **OSC truncation fail-closed**: the 1024-byte raw guard works for the plain
  long-URI case (probe confirmed rejection of a 1500-byte URI) — but a second
  vte truncation vector (param-count saturation) fails OPEN. See CR-01.
- **Intern caps**: `MAX_LINKS` (10 000) + the 1024-byte raw guard bound the
  table at roughly 10–20 MB worst case per pane; entries are append-only so
  scrollback ids stay valid. Note `MAX_LINK_URI_LEN` (2083) is currently
  unreachable dead defense (the 1024 raw guard rejects first) — intentional
  per the fork comment. Holds.
- **Remote attach filter**: `open_link_hints` filters
  `a.remote_id == id && !a.is_closed()`, byte-identical to the render path
  (ui.rs:1108); a mismatched attach yields "no links" rather than falling
  through to another parser. No cross-session leak. Holds.

## Critical Issues

### CR-01: OSC 8 URI truncation via vte param saturation fails open — wrong destination interned

**File:** `vendor/vt100/src/screen.rs:1659-1676`
**Issue:** The fork guards against vte's `MAX_OSC_RAW` (1024-byte) truncation
by refusing the link when `raw_len >= 1024` — but vte 0.11.1 has a **second,
independent truncation vector**: `MAX_OSC_PARAMS = 16`. Once 16 params are
recorded, further `;` separators hit the `MAX_OSC_PARAMS => return` arm in
`Action::OscPut` and the last param's end index is frozen; everything after
the 16th separator is dropped from the dispatched params. Because
`raw_len` is computed as the sum of the *dispatched* param lengths, the dropped
tail is invisible to the guard, and `params[2..].join(&b';')` reconstructs a
truncated — but well-formed, still-openable — URI. This is exactly the failure
class the fork's own comment declares must fail closed ("a longer URI arrives
TRUNCATED, and interning it would produce a wrong ... destination. refuse the
link instead").

Empirically confirmed against the vendored crate:

- `\e]8;;https://evil.example/a;b;c;…;p/TAIL\e\\` (15 semicolons) interns
  `https://evil.example/a;b;c;d;e;f;g;h;i;j;k;l;m;n` — `/TAIL` silently gone.
- A URI with 13 short segments then a 1200-byte tail after the 14th `;`
  interns the truncated prefix and **bypasses the 1024-byte guard entirely**
  (`raw_len` sums to ~55).

Any URI containing ≥14 semicolons (legal in paths and queries) records a
destination that differs from what the emitting program sent. The hint overlay
does display the (truncated) value that will open, which limits user
deception, but the phase's stated invariant is that truncated sequences must
degrade to "not a link".

**Fix:** treat a saturated param table as potentially truncated and fail
closed, matching the raw-length guard's boundary semantics:
```rust
const VTE_MAX_OSC_RAW: usize = 1024;
const VTE_MAX_OSC_PARAMS: usize = 16; // vte 0.11.1 MAX_OSC_PARAMS
let raw_len: usize = params.iter().map(|p| p.len()).sum();
let uri: Vec<u8> = params[2..].join(&b';');
if uri.is_empty() || raw_len >= VTE_MAX_OSC_RAW || params.len() >= VTE_MAX_OSC_PARAMS {
    self.attrs.link = None;
} else if let Ok(uri) = String::from_utf8(uri) {
    ...
}
```
(Cost: a legitimate URI with exactly 13 semicolons is refused — the same
fail-closed trade already accepted at the 1024-byte boundary.) Add a
regression test with a ≥14-semicolon URI asserting no link is interned.

## Warnings

### WR-01: Paste bytes reach the child while the LinkHints overlay is open

**File:** `baude/src/app.rs:3966-4010` (interaction with the new modal)
**Issue:** The phase's LINK-04 claim — "no byte ever reaches the child while
the hints overlay is open" — is enforced and tested at `handle_key`
granularity only. `handle_paste` never checks `self.modal`: with focus on a
content pane and the hints overlay open, an `Event::Paste` forwards the pasted
bytes straight to the child (`a.write_input(&bytes)` at app.rs:3997, and the
local-session branch below), invisibly behind the overlay. This does not let
output open a link (paste is user-initiated input), but it violates the
documented invariant and can inject text into the child the user believed was
suspended.
**Fix:** In `handle_paste`, when `matches!(self.modal, Modal::LinkHints { .. })`
(or, more defensively, any non-`Input` modal), swallow the paste. Extend
`modal_open_swallows_every_key_from_the_child` with a paste leg asserting
`rx.try_recv().is_err()` after `handle_paste` while the overlay is open.

### WR-02: c/y copy reports "copied …" unconditionally; clipboard sink is macOS-only

**File:** `baude/src/app.rs` (handle_link_hints_key copy arm; sink at app.rs:5497-5509)
**Issue:** The new c/y path calls `Self::copy_to_clipboard` — a `pbcopy`-only
sink whose spawn/write failures are silently discarded (`if let Ok(..)`, no
wait, errors dropped) — and then always sets a `copied {url}` message. The
opener constant is cfg-gated (`open`/`xdg-open`) but the clipboard is not: on
any non-macOS build, `pbcopy` does not exist, so LINK-06 copy silently does
nothing while the UI claims success. Even on macOS, a failed spawn still
reports "copied". The sink itself is pre-existing code, but the phase newly
routes a headline feature through it and asserts success it cannot observe.
**Fix:** Make the copy sink fallible (`C: FnOnce(&str) -> std::io::Result<()>`,
mirroring the opener seam), gate the binary per-OS like `OPENER`
(`pbcopy`/`wl-copy` or `xclip`), and message `copy failed: {e}` on `Err` —
the same non-fatal surface pattern `activate_link` already uses.

### WR-03: Wide characters inside a link break the recorded span (`end_col` wrong)

**File:** `baude/src/links.rs:57-67` (with `vendor/vt100/src/screen.rs:1047`)
**Issue:** Wide-char continuation cells carry default attrs (`link_id() ==
None`), so an OSC 8 run whose label contains a wide char splits into multiple
runs at each continuation cell. Probe-confirmed: for label `a中b`, cells read
`Some(0), Some(0), None, Some(0)`. `collect_links` records the first run only
— `end_col` stops at the wide char instead of the run's true end — and only
the interned-entry dedupe (`seen`) prevents a duplicate hint from the second
fragment. `end_col` is `#[allow(dead_code)]` today, but the field's own doc
comment designates it "the hit-testing surface for pointer activation if
modifier-click ever ships", and the detection tests assert it — so the surface
is wrong precisely where it will matter, with no test covering wide chars.
**Fix:** In the run scan, extend the run through cells where
`c.is_wide_continuation()` while the surrounding cells share the id (or set
the continuation cell's `attrs.link` from the base cell at
screen.rs:1047-1048). Add a detection test with a CJK label asserting the full
span.

## Info

### IN-01: `#[allow(dead_code)]` on a live enum variant

**File:** `baude/src/remote.rs:200-202`
**Issue:** `AttachInput::Resize` is annotated dead but is constructed at
remote.rs:249 and 347 and matched at 266 in production code. The annotation
and its comment ("constructed by the IO loop; matched in app.rs tests") are
misleading.
**Fix:** Remove the attribute if it compiles clean without it; otherwise scope
it with an accurate comment (e.g. only the tuple fields are unread in
cfg(test) builds).

### IN-02: pty.rs test duplicates subscribe()'s snapshot construction

**File:** `baude-core/src/pty.rs` (subscribe_snapshot_replays_pre_attach_links)
**Issue:** The test hand-mirrors the alternate-screen preamble + clear + home
+ `contents_formatted` + mode replay from `subscribe()`. If `subscribe()`
changes its snapshot recipe, this test keeps passing against the stale copy —
the parity it proves silently stops covering production.
**Fix:** Extract the snapshot-building into a shared fn used by both
`subscribe()` and the test.

### IN-03: `visible_rows` yields more than `rows_len` rows when scrolled back beyond screen height

**File:** `vendor/vt100/src/grid.rs` (saturating_sub fix region)
**Issue:** The `saturating_sub` fix correctly removes the debug-build
underflow panic, but with `scrollback_offset > rows_len` the iterator now
yields `offset` scrollback rows chained with zero grid rows — more than
`rows_len` total. `cell()`-based consumers read only the first `rows_len`
(correct), but `contents()` / `contents_formatted()` / `rows_formatted()`
iterate everything and would emit extra rows if ever called while scrolled
back (currently they are not — the fork comment's "consumers read at most the
first rows_len elements" is true today by call-site discipline only).
**Fix:** Consider `.take(rows_len)` on the chained iterator to restore the
exact-height contract structurally.

### IN-04: Whole-screen dedupe collapses genuinely separate identical links

**File:** `baude/src/links.rs:64-67`
**Issue:** `seen` dedupes by interned entry, and the intern table dedupes by
`(id, uri)` globally — so two independent no-id OSC 8 links to the same URI on
different rows collapse to one hint anchored at the top occurrence. The
comment frames the dedupe as "wrapped fragment / repeat", but it also merges
distinct visual links. Destination-safe (identical URL), mildly surprising for
hint labeling.
**Fix:** If per-occurrence hints are wanted later, dedupe per contiguous
region rather than per interned id; otherwise document the collapse as
intended.

### IN-05: Userinfo URLs render the `user@` part inside the always-visible origin prefix

**File:** `baude/src/app.rs` (display_truncated_width, `Position::BeforePath`)
**Issue:** `validate_http_url` accepts userinfo (`http://user@host/p` is in
the ACCEPT table), and the truncation prefix `url[..BeforePath]` includes it —
so `https://google.com@evil.com/x` displays with `google.com` in the position
a casual reader takes for the host. Nothing is hidden (the true host is
shown), but T-10-15's origin-clarity goal is weakened by visual spoofing.
**Fix:** Consider highlighting `url.host_str()` distinctly in the overlay, or
flagging/rejecting userinfo URLs (browsers warn on these for the same reason).

### IN-06: Global ctrl+o chord permanently shadows 0x0F for child applications

**File:** `baude/src/app.rs:3909-3915`
**Issue:** The chord is collision-checked against baude's own chord set, but
it also means the child pane can never receive ctrl+o (nano's WriteOut, emacs
open-line, fzf toggle in some setups). A deliberate product trade — worth a
line in the help text/docs so users know why the child never sees it.
**Fix:** Document; optionally provide a pass-through (e.g. double ctrl+o) if
users report breakage.

---

_Reviewed: 2026-09-16T08:47:13Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
