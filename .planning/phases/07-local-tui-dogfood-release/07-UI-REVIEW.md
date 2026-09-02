# Phase 7 — UI Review

**Audited:** 2026-09-02
**Baseline:** 07-UI-SPEC.md (approved design contract), plus post-plan approved scope: standalone non-Git sessions (82faef9), existing-worktree auto-population (59e67b6), seed-exempt removal (6014b63), checkout-first restart selection (da1b7f2), merged-main help text `ctrl+q, x   close session`
**Screenshots:** not captured — Ratatui terminal UI with no browser surface (a service on :3000 is unrelated to this project). Visual evidence: recorded ANSI captures per 07-UAT-EVIDENCE.md (2026-08-31 live run, 2026-09-01 scripted PTY re-certification at commit 6014b63, 2026-09-02 Linux CI matrix at da1b7f2), plus `TestBackend` buffer assertions in `baude/src/ui.rs`.

Audit method adapted for terminal rendering: pillars scored against the UI-SPEC's Ratatui contract (copy literals, glyph/style matrix, terminal color roles, weight/emphasis rules, cell-grid spacing at 160×40 → 40×12, interaction/refusal contract). Registry audit skipped by rule: no `components.json`; UI-SPEC declares no registries (Rust/Ratatui project).

---

## Pillar Scores

| Pillar | Score | Key Finding |
|--------|-------|-------------|
| 1. Copywriting | 3/4 | Refusal/hint/modal literals match the spec's exact tables, but the RetryReopen reopen-refusal literal is missing (`reopen blocked: {error}` generic fallback), empty-state copy diverged, and the close-modal title differs |
| 2. Visuals | 3/4 | Full glyph/anatomy matrix implemented and buffer-tested; `↳` connector indent is inconsistent between live and runtime-less meta lines (col 2 vs col 4) |
| 3. Color | 3/4 | Local surface honors the 60/30/10 contract; the legacy remote close modal keeps a red border, diluting red = physical-destruction semantics |
| 4. Typography | 4/4 | Regular/bold only, one em size, one-row leading; italic on repo context is spec-prescribed and paired with dimming, not italic-only |
| 5. Spacing | 3/4 | Exact breakpoint math and cell-aware truncation; repository-row prefix reserve is off by 2 cells, `input_tail` uses scalar counting, footer/compact ordering inverts at height 19 |
| 6. Experience Design | 4/4 | Capability-gated dispatch, exhaustive zero-mutation refusals, distinct x/X with double preflight, deterministic selection — verified by tests and three live dogfood passes |

**Overall: 20/24**

No BLOCKERs. All findings below classified WARNING (degrade quality; no user-task completion breaks).

---

## Top 3 Priority Fixes

1. **Missing RetryReopen reopen-refusal literal; generic `reopen blocked: {error}` fallback** (`baude/src/app.rs:3721`) — when reopen fails after capability was granted, the user gets an unstructured error instead of the contracted `Cannot reopen "<target>": Git topology is still unavailable (<cause>). Repair the checkout at "<path>", then press r to recheck and reopen.` (no occurrence of that literal exists in the codebase) — implement the typed literal and route `reopen_checkout` failures through the refusal formatter so cause + next step + retry authorization are always named.
2. **Metadata connector indent shifts 2 cells with runtime presence** — a checkout's second line renders `▌ ↳ …` (connector at col 2 via `chips_line`, `ui.rs:639`) while the same row when closed renders `▌   ↳ …` (col 4, `ui.rs:522`); the row anatomy in the spec shows the 4-cell indent, and the jump on close/reopen reads as a layout glitch — indent `chips_line`'s connector to match the runtime-less variant (and align `standalone_row`'s connector, `ui.rs:591`).
3. **Copy-contract drift pinned by tests instead of reconciled** — `EMPTY_HEADING`/`EMPTY_BODY` (`ui.rs:26-27`, "no sessions yet" / "press n to open a repository or folder, or c to clone") diverge from the contract literals (`no repositories yet` / `press n to open a repository or c to clone one`), and the close modal title is ` close local session ` (`ui.rs:1553`) vs contracted `close checkout session`; the copy test (`ui.rs:2423`) now asserts the divergent strings — either amend 07-UI-SPEC's Copywriting Contract for the approved standalone scope or restore the contracted literals, so the spec and tests agree on one source of truth.

---

## Detailed Findings

### Pillar 1: Copywriting (3/4)

**Compliant (verified against spec literals):**
- All exact typed refusals implemented: repository close/remove/shell/archive (`app.rs:949-959`), already-closed (`app.rs:962`), main/unmanaged/standalone remove (`app.rs:966-972`), unavailable-topology × create/close/reopen/remove (`app.rs:984-1003`), activation/teardown/stopped-active recovery matrix (`app.rs:1006-1023`), removal blockers dirty/conflicted/locked/submodule/indeterminate (`app.rs:3789-3820`), branch failures invalid-ref/remote-only/collision/occupied-protected (`app.rs:4154-4177`), busy-lifecycle (`app.rs:4118`).
- Success copy exact: `created worktree for <branch>` / `activated <branch>` / `focused existing <branch>` (`app.rs:4121-4127`), `session closed — checkout kept` (`app.rs:3996`), `worktree removed — local branch <ref> retained` (`app.rs:4015`), `reopening "<target>"…` (`app.rs:3719`), persistence error (`app.rs:35`).
- Branch prompt title exact: `create or activate branch in <repository> — local branch name` (`app.rs:3733`).
- Both tiny-height shell messages exact (`app.rs:3386,3392`).
- All 8 full-width hints and both narrow retained variants exact, asserted literal-for-literal in `ui.rs:2519-2575`.
- Post-plan approved copy present: seed-exempt removal semantics (UAT 2026-09-01 step 9), standalone `w`/`X` refusals (`app.rs:972,975`), help `ctrl+q, x   close session` (`ui.rs:2098`).

**Findings (WARNING):**
- **Missing literal:** the spec's capability-present reopen refusal (`…still unavailable… then press r to recheck and reopen`) exists only as a hint string, never as a refusal; failures after the capability gate fall through to `reopen blocked: {error}` (`app.rs:3721`), violating the `Cannot <action> "<target>": <cause>. <safe next step>.` pattern.
- **Empty state:** `no sessions yet` / `press n to open a repository or folder, or c to clone` (`ui.rs:26-27`) vs contract `no repositories yet` / `press n to open a repository or c to clone one`. Rational adaptation for standalone scope, but the contract was never amended and plan 07-03 explicitly required the original literals.
- **Close modal title:** ` close local session ` (`ui.rs:1553`) vs contracted `close checkout session`.
- **Help modal accuracy** (`ui.rs:2064-2109`): section header claims "sidebar sorts alphabetically" — true for parents/standalone but children sort by persisted first-seen order; `w` described as "new worktree session for selected repo" omitting the activate-existing-branch behavior (primary CTA is `Create or activate branch`); `r` described as "restart exited claude" understating capability gating.
- **Remote close modal** hints `y close · n cancel` (`ui.rs:1512`) — legacy-preserved per the flat-compatibility carve-out; noted, not penalized.

### Pillar 2: Visuals (3/4)

**Compliant:**
- Full state glyph matrix: pulsing `●` (360 ms), `◐/◓/◑/◒` spinner (130 ms), dim `✓`, `✗`, `○` closed, `·` archived, `!` unavailable with cause text `missing`/`changed`/`recovery` (`ui.rs:426-448`, `hierarchy.rs:463-475`).
- Muted italic/dim `repo ·` parent context, bold-cyan only on fallback selection (`ui.rs:367-375`); checkout as primary unindented item (checkout-first inversion from UAT feedback implemented).
- Role chips main/default/worktree in Blue/Cyan/Magenta (`ui.rs:500-508`); waiting/unavailable-only parent aggregates, right-aligned (`ui.rs:376-384`); no working/completed aggregation.
- Selection band spans both lines including connector cells without erasing status colors; buffer tests assert Indexed(237) never touches the border column (`ui.rs:2347-2353`).
- Waiting animation limited to icon/timer; name never flashes; archived rows never animate (`ui.rs:723-767`).
- Remote section visually separate under `⇄ remote` / `⇄ remote (offline)` (`ui.rs:616-625`).

**Findings (WARNING):**
- Connector indent inconsistency (Top Fix 2): live meta line `↳` at col 2, runtime-less at col 4, standalone at col 2 — spec anatomy shows col 4.
- Hierarchy `checkout_row` waiting glyph omits `Modifier::BOLD` that legacy `session_row` applies (`ui.rs:429` vs `ui.rs:735`) — the pulse reads weaker on local hierarchy rows than remote rows.
- Help modal is 32 rows in a `centered(area, 60, 32)` rect; at 40×12 roughly two-thirds of the key matrix (including the close hint) clips with no overflow indicator (`ui.rs:2065`). The viewport test only asserts two strings are visible.

### Pillar 3: Color (3/4)

**Compliant (grep audit of `Color::` usage in ui.rs/app.rs):**
- Dominant `Color::Reset` background; secondary Indexed(237)/DarkGray/Gray carry band, borders, metadata, connectors, retained/archived — matching the 60/30 split.
- Cyan confined to focused border, focused gutter, input cursor block, non-destructive/info modal borders, default role chip, working checkout name — no accent spray across actions or parents.
- Red confined to: remove-confirmation border/title (`ui.rs:1582`), context ≥80% and `bypassPermissions` alarm chips, rate ≥80% — matching "destructive + severe alarm only" for the local surface.
- Yellow: waiting pulse/timer, `!` unavailable, transient message bar (black-on-yellow, `ui.rs:1194`), parent `unavailable` aggregate. Green: calm completed check, GSD modal border (existing GSD-positive surface). Magenta/Cyan/Blue role chips exactly as contracted.
- No hardcoded RGB outside the vt100 PTY passthrough (`vt_color`), which renders guest content, not chrome.
- Status is never color-alone: every state pairs glyph + text (`state_text`, chip text, aggregates).

**Finding (WARNING):**
- The remote `ConfirmKill` close modal keeps a **red** border (`ui.rs:1520`) for an ordinary retained close. The spec's flat-compatibility carve-out shields the behavior, but now that red is the established physical-removal signal locally, a red "close session" modal on remote rows dilutes exactly the destructive/non-destructive color distinction this phase built. Recommend recoloring to cyan with the retained-close hint wording (behavior unchanged, no new API).

### Pillar 4: Typography (4/4)

- Exactly two weights in chrome: regular and bold (selection, headings, waiting icon); one terminal em everywhere; one-row leading; no simulated large ASCII headings (grep for `Modifier::` confirms BOLD/ITALIC/DIM/UNDERLINED/REVERSED only, with UNDERLINED/REVERSED confined to PTY passthrough and cursor/selection inversion).
- Italic appears solely on the repository context label (`ui.rs:374`) — prescribed by the spec's own row anatomy ("italic/dim basename") and paired with DarkGray plus the `repo ·` prefix, so it is not an italic-only distinction. The spec's typography table and row anatomy are internally inconsistent here; implementation follows the anatomy and remains monochrome-legible.
- DIM modifier on the completed check matches "calm completed check". No finding rises to a deduction; checks performed: modifier grep, glyph matrix cross-reference, monochrome-distinguishability review of parent/child/selected/closed states (distinct via italic+prefix, bold, band, glyph).

### Pillar 5: Spacing (3/4)

**Compliant:**
- Breakpoints exact per contract: 42 cols at ≥120, `clamp(width/3, 28, 38)` at 80–119, 26 at 60–79, single-pane <60 (`ui.rs:54-58`), verified border-position assertions at all five contracted viewports (`ui.rs:2277-2307`).
- Two-cell gutter exact (`ui.rs:143-154`); saturating centering and `min(preferred, viewport)` modal geometry (`ui.rs:1357-1366`); status bar always one row.
- Cell-cluster truncation keeps zero-width combining marks attached and preserves the branch leaf/path tail (`ui.rs:163-216`), asserted with wide+combining fixtures (`ui.rs:2329-2331`).
- Scroll keeps selected row and parent context visible together (`ui.rs:344-356`); footer carved only with ≥4 list rows remaining (`ui.rs:229`).

**Findings (WARNING):**
- `repository_row` truncates the name to `width − 9 − reserve` but the actual prefix is 11 cells (gutter 2 + `"  repo · "` 9) (`ui.rs:390-393`) — a maximal-length repository name can push the right-aligned `N waiting`/`unavailable` aggregate 2 cells past the row width, clipping it at narrow sidebars. `checkout_row` reserves 5 for a 4-cell prefix (safe); fix the constant to 11.
- `input_tail` uses `chars().count()`/`chars().skip()` (`ui.rs:1349-1353`) — scalar counting the spec explicitly forbids; a CJK/wide path or branch typed into the input modal can overflow the modal's inner width.
- At exactly height 19, rows go compact (`area.height < 20`, `ui.rs:107`) while the 6-row usage footer still renders (`area.height >= 19`, `ui.rs:229`) — inverting the contracted "hide usage footer first, then compact" degradation order for that one row of height.
- Remove confirmation is a fixed 8-row rect that truncates branch/path values (`ui.rs:1568-1575`) rather than wrapping onto continuation rows and growing toward the viewport as contracted. Mitigation: tail-preserving truncation keeps the basename and full-ref leaf visible and the tiny-viewport test proves target/action lines survive at width 38 — identity is never obscured, so this is a deviation, not a safety issue.

### Pillar 6: Experience Design (4/4)

**State coverage and interaction contract — all verified:**
- Exhaustive selection×key dispatch table (`sidebar_action`, `app.rs:273-362`) mirrors the spec's action matrix: `X` strictly separated from `x` including shift-modifier handling, remove accepted only for `Managed && can_remove`, `r` only via core `RetryReopen`/`RetryRecovery`, hidden actions defensively refused with typed copy. Guarded test `hierarchy_action_matrix_dispatches_only_authorized_local_actions` asserts zero state/process/Git/order/selection mutation on every refusal.
- Capability is core-derived (`hierarchy.rs:83-125`), never inferred from glyphs, cause strings, or runtime absence.
- Deterministic selection: durable-key reconciliation, next/prev-sibling/parent removal fallback within the repository (`hierarchy.rs:247-304`), checkout-first restart initialization (`hierarchy.rs:185-199`, da1b7f2 closing the UAT selection-contract question).
- Loading/pending: `reopening "<target>"…` without optimistic rows; empty state and welcome pane present; destructive confirmation defaults to preservation (`n`/`Esc` keep; unrelated keys inert per action-matrix evidence); double preflight with compensation preserved from Phase 6.
- Resize safety: hidden panes never resized, positive-dims-only (`sync_sizes`, `app.rs:3378-3420`), immediate tiny-height shell focus transfer with exact copy, dead-pane focus fallback to sidebar.
- Live verification: three dogfood passes (wide/narrow ANSI captures, restart/dedup/removal flows, standalone recovery) plus the full Linux/macOS CI matrix green at da1b7f2.

**Minor notes (no deduction):** `Modal::ConfirmKill` retains dead rendering branches for local selections (`ui.rs:1482-1506`) — unreachable via `confirm_close_selected` (`app.rs:3621-3641`, remote-only) but worth pruning; the `reopen blocked` generic fallback (counted under Copywriting) slightly weakens next-step guidance in one failure path.

Registry audit: skipped — no `components.json`; UI-SPEC Registry Safety table declares no registries.

---

## Files Audited

- `/Users/joese/Code/github.com/poindexter12/baude/baude/src/ui.rs` (full — layout, rows, modals, hints, tests)
- `/Users/joese/Code/github.com/poindexter12/baude/baude/src/hierarchy.rs` (full — projection, ActionView, selection)
- `/Users/joese/Code/github.com/poindexter12/baude/baude/src/app.rs` (copy constants, refusal formatter, sidebar dispatch, modal open/confirm paths, sync_sizes/focus transfer, branch modal)
- `.planning/phases/07-local-tui-dogfood-release/07-UI-SPEC.md` (baseline)
- `07-01…07-06` PLAN.md and SUMMARY.md, `07-UAT-EVIDENCE.md` (evidence base)
- Post-plan commits inspected: `59e67b6`, `6014b63`, `da1b7f2`
