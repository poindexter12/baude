---
phase: 10-clickable-terminal-links
fixed_at: 2026-09-16T00:00:00Z
review_path: .planning/phases/10-clickable-terminal-links/10-REVIEW.md
iteration: 1
findings_in_scope: 4
fixed: 4
skipped: 0
status: all_fixed
---

# Phase 10: Code Review Fix Report

**Fixed at:** 2026-09-16
**Source review:** .planning/phases/10-clickable-terminal-links/10-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 4 (CR-01, WR-01, WR-02, WR-03; Info findings out of scope per orchestrator)
- Fixed: 4
- Skipped: 0

## Fixed Issues

### CR-01: OSC 8 URI truncation via vte param saturation fails open

**Files modified:** `vendor/vt100/src/screen.rs`, `vendor/vt100/tests/link_fidelity.rs`
**Commit:** dac2658
**Applied fix:** Added `VTE_MAX_OSC_PARAMS = 16` and extended the fail-closed
guard to `params.len() >= VTE_MAX_OSC_PARAMS`, mirroring the 1024-byte raw
guard's boundary semantics (a saturated table is indistinguishable from a
truncated one). Regression tests: `param_saturated_uri_is_not_a_link`
(15-semicolon URI — no intern, parsing continues, later links still intern)
and `uri_below_param_cap_still_interns_intact` (12-semicolon boundary control
proving the guard is not over-broad).

### WR-01: Paste bytes reach the child while the LinkHints overlay is open

**Files modified:** `baude/src/app.rs`
**Commit:** 8f354a2
**Applied fix:** `handle_paste` now returns early when the modal is any
non-`None`, non-`Input` variant (the review's defensive variant), mirroring
`handle_key`'s modal-first routing; `Modal::Input`'s dedicated paste path is
preserved. `modal_open_swallows_every_key_from_the_child` gained a swallowed-
paste leg while the overlay is open (covered by the existing
`rx.try_recv().is_err()` assertion) plus a forwarded-paste control leg after
the modal closes, proving the swallow assertion is not vacuous.

### WR-02: c/y copy reports "copied …" unconditionally; sink is macOS-only

**Files modified:** `baude/src/app.rs`
**Commit:** 0849269
**Applied fix:** Copy sink generic is now
`C: FnOnce(&str) -> std::io::Result<()>` (mirrors the opener seam; injected-
sink test seam intact). `copy_to_clipboard` is cfg-gated like `OPENER`
(`pbcopy` on macOS; `wl-copy` with `xclip -selection clipboard` fallback
elsewhere), propagates spawn/write errors, and waits on + status-checks the
child (also removes the prior zombie window). c/y messages
`copy failed: {e}` on `Err` — the same non-fatal surface as `activate_link`
— and the mouse selection-copy path surfaces failure too. New test:
`copy_error_surfaces_failure_not_success`.

### WR-03: Wide characters inside a link break the recorded span

**Files modified:** `vendor/vt100/src/cell.rs`, `vendor/vt100/src/screen.rs`, `vendor/vt100/tests/link_fidelity.rs`, `baude/src/links.rs`
**Commit:** 84e0c87
**Applied fix:** Took the review's grid-layer variant: the wide-char
continuation spacer now carries the base cell's link id via a new crate-
private `Cell::set_link`, applied where `text()` writes the spacer
(screen.rs). Per-cell ids are contiguous across wide glyphs, so
`collect_links` records the true `end_col` unchanged; erase/overwrite still
strip the spacer's id via `Cell::clear`. Tests: fork
`wide_char_label_keeps_contiguous_link_ids` (contiguity + overwrite-strips
leg) and detection `wide_char_label_records_full_span` (span `(0, 0, 3)` for
label `a中b`).

## Additional Commits

- **a5a0f9f** `style(10)`: `cargo fmt` across the workspace — the pre-fix
  baseline (6373c6e) already failed `cargo fmt --check` with 16 hunks
  (verified in a throwaway worktree); this commit clears that drift plus the
  new fix code so the gate is green.
- **1a3d27d** `docs(10)`: Resolution notes added under each in-scope finding
  in 10-REVIEW.md. Info findings (IN-01..IN-06) untouched.

## Verification

All runs performed in the **main checkout** on branch
`gsd/phase-10-clickable-terminal-links` (worktree isolation deliberately
bypassed per orchestrator directive: sequential mode, and the full-workspace
cargo gates need the main checkout's build cache).

- `cargo test -p vt100` — green (17 tests incl. 3 new fork tests)
- `cargo test -p baude link` / `cargo test -p baude --bins modal` — green
- `cargo test --workspace --locked` — exit 0, 612 passed / 0 failed
  (exit captured directly, not through a pipe)
- `cargo fmt --check` — exit 0 (after the style commit)
- `cargo clippy --workspace --all-targets` — exit 0; remaining warnings are
  pre-existing (vendored `grid.rs:610`, phase `ui.rs:2120`) in regions not
  touched by these fixes

## Skipped Issues

None.

---

_Fixed: 2026-09-16_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
