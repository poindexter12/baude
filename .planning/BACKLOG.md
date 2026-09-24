# Backlog

Captured ideas and observations not yet scheduled into a milestone phase. Triage
into ROADMAP phases when picked up.

---

## Captured 2026-06-15

### BL-01 — Sidebar "idle"/status is inaccurate (silence-only, not real working/waiting)

**Observation (user):** The sidebar's "idle" indicator only reflects "we haven't
typed anything" (the PTY-output-silence heuristic) — it does not show whether
Claude is actually *working* vs *waiting for user input*. What it should show is
"more input needed to continue" — i.e. Claude is **not working and is waiting on
the user**.

**Status:** **Directly addressed by v0.7 Phase 2 (Hook-Driven Status)** — already
planned and in progress. Hook events give exactly this signal:
`UserPromptSubmit`→working, `Stop`→waiting/done, `Notification`→needs input/permission,
with the silence heuristic demoted to a labeled fallback (`StateSource`). This
backlog entry is the real-world symptom that validates Phase 2's design.

**Action:** No new work item — verify during Phase 2 UAT that the sidebar's
working/waiting/needs-input distinction is now accurate for live sessions. If the
sidebar label wording still reads "idle" ambiguously after Phase 2, file a small
follow-up to relabel.

---

### BL-02 — Model / permission-mode (bypass) / planning mode not shown for every session — ⚠️ PARTIALLY RESOLVED 2026-06-24

**Triage (2026-06-24):** It's a CAPTURE gap, not a render gap — the sidebar/overlay
already show each field when present (`Some`→show, `None`→omit). Root cause:
- `model`: dual-sourced (transcript `assistant` messages + statusline bridge
  `schema:2`), but both are gated on the session's `sessionId` resolving (the
  `sessions/<pid>.json` pid-match or the cwd-fallback transcript) — the session
  file itself carries NO model/mode, only `sessionId` + status. No assistant turn
  yet AND no bridge → no model.
- `permission_mode`: transcript-ONLY (the `permissionMode` field on transcript
  records). The statusline payload does NOT carry it (confirmed against a live
  bridge file), so there's no second source.

**Fix shipped (commit on `gsd/phase-04`):** `permission_mode` now falls back to
baude's spawn intent via `spawn_permission_mode()` (skip/default→`bypassPermissions`,
prompt→none) in both the daemon `SessionInfo` and the TUI local line, so the mode
shows for every session immediately; a transcript-reported mode still wins.

**Still open:** `model` remains data-gated — absent until the transcript/bridge
resolves and yields a model. The deeper sub-cause (sessions whose `sessionId`
never resolves: cwd-encoding mismatch, the "ignore session files >20s older than
spawn" filter, or daemon sessions without the bridge) needs a REAL-session repro
to confirm and fix; could not be reproduced headlessly (scratch sessions don't
resolve `claude_session_id`). Carry as a focused follow-up if model is still
missing on real sessions after this.

**Observation (user):** The model isn't shown for everything, and neither is the
permission mode (e.g. bypass), planning mode, etc. — they're missing for some
sessions.

**Likely cause:** These come from the v0.7 Phase 1 status-line capture / `bridge.rs`
+ `meta.rs` sources. Some sessions don't surface them — possibly sessions without a
seeded statusLine bridge, or where the session-file / bridge file hasn't been read
yet. Needs investigation: is it a capture gap (field never populated) or a render
gap (captured but not displayed in the sidebar vs the info overlay)?

**Action:** Triage as a Phase 1 follow-up / bug. Determine which sessions lack the
fields and whether it's capture vs render. Candidate for a small phase or a v0.7
audit item.

---

### BL-03 — Wire GSD phase/state into the sidebar — ✅ RESOLVED 2026-06-24

**Resolution:** GSD already showed on local TUI sessions (`ph<active_phase>`) but
nowhere else. Made placement CONSISTENT across surfaces (kept the existing
sidebar-line placement rather than redesign): added `gsd_active_phase` to
`SessionInfo` + `RemoteInfo`, rendered `ph<phase>` on the remote TUI line, and
added a `⬡ <milestone> ph<phase>` chip to the PWA session rows (sw.js v5→v6).
GSD state is now visible at a glance on local TUI, remote TUI, and the PWA.
Commit on `gsd/phase-04`.



**Observation (user):** Since GSD is used heavily, surfacing GSD phase/state in the
sidebar would be valuable "if it makes sense."

**Context:** baude already reads GSD state from disk (noted in PROJECT.md validated
features, v0.3 — "live Claude metadata from disk … GSD state"). This item is about
giving it more prominent/consistent placement in the sidebar so active GSD work is
visible at a glance.

**Action:** New feature idea — scope a future phase (likely a later v0.7 ergonomics
item or a Tier-2+ milestone). Confirm desired placement (sidebar line vs overlay)
with user before planning.

---

## Captured 2026-08-31

### BL-05 — First-class standalone sessions for non-Git folders — RESOLVED 2026-08-31

**Observation (user):** Sometimes a folder is opened for coding-agent work but it
is not a Git repository and never will be. The v2.0 beta currently ignores a
non-Git launch directory and the `n` flow refuses it as repository admission.

**Decision:** A real standalone local session should be a top-level row, not a
synthetic repository parent or invalid checkout. It retains normal agent, shell,
editor, resume, archive, info, activity, GSD, close, and reopen behavior. Git-only
branch/worktree actions (`w`, `X`) are unavailable.

**Implementation boundary:** Add a durable `StandaloneKey`/record and runtime map,
strict schema migration, standalone selection/projection/action semantics, canonical
path deduplication, restart/close/reopen persistence, and missing-folder recovery.
Do not weaken repository/checkouts invariants or persist an unowned runtime.

**Resolution:** Implemented schema-v3 standalone keys/records and strict v2
migration, canonical-path admission/deduplication, exact registered runtime and
shell ownership, restart/close/reopen/missing-folder recovery, root-level TUI
projection, normal local session actions, and explicit Git-action refusals. Full
formatting, Clippy, and 345 workspace tests pass. Isolated live UAT covered alias
deduplication, all local actions, close/reopen/restart, archive retention,
missing-folder recovery, and Git-action refusals; image screenshots and formal
phase certification remain pending.

---

## Captured 2026-06-23

### BL-04 — `BAUDE_PERMISSION_MODE=prompt` silently suppressed when `claude_cmd` already contains `--dangerously-skip-permissions` — ✅ RESOLVED 2026-06-24

**Resolution:** Fixed via `resolve_claude_cmd` (replaces the append-only
`permission_flag`): in prompt mode the conflicting `--dangerously-skip-permissions`
is stripped from `claude_cmd` and the prompt flag wins (explicit opt-in), with a
daemon `eprintln` warning (`stripped_skip`). Chose candidate (b) from below. Both
spawn sites migrated; BL-04 unit tests added. Commit on `gsd/phase-04`.



**Observation (found during v0.7 §F UAT setup):** With `~/.config/baude/config.json`
`"claude_cmd": "claude --dangerously-skip-permissions"`, setting
`BAUDE_PERMISSION_MODE=prompt` does NOT enable prompt mode. The locked
no-double-add rule (`permission_flag_for`, T-04-02) sees the existing
`--dangerously-skip-permissions` in the base cmd and returns `""`, so skip wins.
Meanwhile `is_prompt_mode()` (which does NOT inspect the base cmd) still returns
true and seeds `.mcp.json` — a half-configured state (mcp server seeded, but
claude runs in skip mode) with NO warning. The `approve` tool is never invoked.

**Impact:** Any user who bakes `--dangerously-skip-permissions` into `claude_cmd`
(a very common, natural setting for unattended use) cannot turn on prompt mode via
the env var alone, and gets no signal explaining why. This is the inverse of WR-01
(which warns on TUI-prompt-with-no-daemon).

**Candidate fixes:** (a) when `is_prompt_mode()` but the resolved base cmd carries
`--dangerously-skip-permissions`, log/surface a warning at spawn (and skip the
.mcp.json seed for consistency); (b) in prompt mode, strip a conflicting
`--dangerously-skip-permissions` from the base cmd before appending the prompt
flag (prompt is the explicit opt-in, so it should win over a config default);
(c) document the interaction. (b) is probably the least-surprising behavior.

**Workaround (UAT):** override with `BAUDE_CLAUDE_CMD=claude` so the base cmd has
no permission flag and prompt mode engages.

---

## Captured 2026-09-22

### BL-06 — release-please silently skips a commit whose body nests parentheses; no release PR opens

**Observation (2026-09-22, #93 / #94):** The squash commit for #93 (`ace9894`)
quoted the runtime error `ContradictoryLifecycle(CheckoutKey(1))` in its body.
release-please v17 (`googleapis/release-please-action@v4`) logged
`commit could not be parsed ... unexpected token '(' at 13:35`, counted zero
releasable commits, reported the run as **success**, and opened no 2.3.1 PR.
An empty carrier commit (#94) was needed to cut the release. The parser treats
an opening parenthesis inside the body like a scope opener and demands a
closing one; any nested pair fails the whole message, and nothing in CI says
so.

**Impact:** Every fix that quotes Rust error text, a `Foo(Bar(1))` value, or a
function call with a call inside it will ship no release and leave the
changelog missing the fix. The failure is silent at every step: PR checks are
green, the merge is green, the release-please run is green.

**Fix:** A commit-message check owned by this repo, in the same shape as
`scripts/assert-real-roots-untouched.sh`:

- `scripts/check-commit-message.sh <file|-> ` rejects nested parentheses in
  the header or body, reports `line:col`, and exits non-zero.
  `--self-test` exercises the cases below against synthetic messages.
- `--range <base>..<head>` checks every non-merge commit in a PR; CI runs it
  in the `check` job over `origin/main..HEAD` **and** over the PR title, since
  a squash merge takes its header from the title.
- A `commit-msg` hook at `.githooks/commit-msg` calling the script, enabled
  with `git config core.hooksPath .githooks`, documented in README.
- Oracle: the check must agree with the parser release-please actually uses
  (`@conventional-commits/parser`). Where `node` is available, `--self-test`
  runs each case through both and asserts the verdicts match, so the check
  never drifts from the thing it guards.

**Tests (write-up; `--self-test` implements these):**

| # | Message | Expected |
|---|---------|----------|
| 1 | `fix(core): plain header` | pass |
| 2 | header with squash suffix `... (#93)` | pass |
| 3 | body with one level: `passes validate() and foo(bar)` | pass |
| 4 | body quoting `ContradictoryLifecycle(CheckoutKey(1))` | **fail**, names `13:35`-style position |
| 5 | nested parens inside a fenced code block in the body | **fail** (the parser has no notion of fences) |
| 6 | footer `Refs #92, #93.` | pass |
| 7 | header scope nesting `fix(core(x)): ...` | **fail** |
| 8 | `--range` with one bad commit among three good | **fail**, names the sha |
| 9 | `--range` containing a merge commit | merge commit skipped |
| 10 | unbalanced `(` in the body | verdict taken from the oracle; recorded in the self-test, not assumed |
| 11 | `--self-test` with `node` present | every case agrees with `@conventional-commits/parser` |

**Status:** ✅ RESOLVED 2026-09-23 via PR #97, released in v2.4.0. Background thread started 2026-09-22 from the #92 session.

---

### BL-07 — One unreconcilable retained checkout kills every restored session (Phase A all-or-nothing) — #92 design item

**Observation (2026-09-22, #92):** 2.3.0 defers every save made during
restore into Phase A's single durable write (`finish_restore_phase_a`). If that
write fails validation, every gated session is killed: "N restored session(s)
were stopped and none was released". #93 removed the one cause seen in the
wild (a protected row keeping a stale `owned_runtime`), but the structure is
unchanged: any future per-row contradiction, from any of the restore
reconcilers, is again a total outage instead of one refused row. 2.2.0's
per-row save and rollback contained this by accident, not design.

**Where a row can still go wrong before Phase A:** `reconcile_teardown_recoveries`,
`reconcile_activation_recoveries`, `restore_standalones`, the launch-directory
admission, and `plan_reopen`'s success path, whose `debug_assert!(next.validate())`
has the same blind spot #93 fixed on the blocked path (`validate()` never checks
lifecycle against runtime; `validate_lifecycle_views()` does).

**Candidates (decision needed):**
- (a) Every restore-time transition validates on a clone with both validators
  before committing to `App` state, the way `plan_reopen` now does. A row that
  cannot be made valid is refused with its own message and never dispatched.
- (b) On Phase A save failure, isolate instead of kill: map the
  `ValidationError` to its checkout or standalone key, stop only that row, retry
  the save once, release the rest.
- (c) Both: (a) as the rule, (b) as the backstop.

**Tests (write-up):**

1. Parametrize `restore_survives_one_unreconcilable_checkout_with_a_stale_runtime`
   over every `ReconciliationUnavailable` variant (`Missing`, `PathChanged`,
   `BranchChanged`, `Detached`, `LockedOrPrunable`, `IdentityChanged`,
   `Discovery`): the bad row ends `Protected` with no runtime, the others
   restore and release, `save_attempts_for_test == 1`.
2. Same with the stale row's lifecycle at `Launching(gen)` and `Stopping(gen)`,
   not only `Running(gen)`.
3. A standalone session whose record contradicts its runtime: same isolation.
4. Launch-directory admission that fails validation does not take saved
   sessions down.
5. Phase A save failure attributable to one row (inject a validation failure
   for a single key): that row is stopped and the message names it; the other
   N-1 release. This is the new behavior under (b).
6. Phase A save failure attributable to no row (`AtomicFailure::Write`, the
   existing `restore_save_failure_kills_paused_children...` test): unchanged.
7. Fixture: the seeded shape "Running + dead-pid runtime + moved branch" from
   #93 becomes a shared helper so the above do not each rebuild it. This is
   the fixture-realism gap that let #92 through (`.planning` fixture note).

**Status:** ✅ RESOLVED 2026-09-23 as option (c) via PR #99, released in v2.4.1. Was: needs the (a)/(b)/(c) decision; candidate for the next
milestone's reliability phase.

## Captured 2026-09-24

### BL-08 — Bare `#NN` issue/PR references in pane text are not activatable links

**Observation (2026-09-24):** Claude's transcript inside a baude pane is full
of bare references like `#21 comment`, `release-please (#21)`, `Refs #92,
#93`. Today only OSC 8 hyperlinks and bare `http(s)://` URLs are collected by
the link gesture (`baude/src/links.rs`, Phase 10), so a `#21` is plain text
and the user has to work out which repo it belongs to and type the URL.

**Ask:** make `#NN` activatable, resolved relative to what the pane is
presenting: the checkout the session runs in, hence that repository's
`origin`. `#21` in a baude pane opens `https://github.com/poindexter12/baude/issues/21`
(GitHub redirects `/issues/NN` to `/pull/NN` when NN is a PR, so one URL form
covers both). Do it without hurting rendering or CPU by scanning aggressively.

**Why the cost concern is already answered by the Phase 10 design:** link
collection is gesture-time only. `collect_links` runs when the user opens the
hint overlay (`app.rs` around 5600), over the visible `vt100::Screen` under
the parser lock, never per frame and never over scrollback beyond the
`BARE_CONTINUATION_BOUND` join. Adding a third pass costs one more walk of
rows × cols per gesture, same order as the bare-URL pass. Nothing is
underlined or re-rendered; hints are labels in the overlay. So the guard rail
is: keep the new pass inside `collect_links` and never move detection into
the draw path.

**Where the repo comes from:** repository rows in `RepositoryState` do not
record a remote today (`repository.rs`; no remote/identity URL field), and
`git::parse_clone_target` (`git.rs` ~1832) already turns any origin form into
`(host, owner, repo)`. Resolve `origin` once per repository at reconcile time
and cache it on the row or a side map; never shell out at gesture time or per
frame. Panes without a resolvable GitHub-style origin (standalone sessions in
a non-repo dir, a remote with an unknown host) simply do not collect `#NN`,
matching LINK-07 fail-closed.

**Detection rules (proposed):**
- Token grammar: `#` followed by 1 to 7 digits, preceded by start-of-line,
  whitespace or `(`/`[`, followed by end, whitespace or `)`/`]`/`,`/`.`/`:`/`;`.
  Rejects `#1f2937` (hex color), `#!/bin/sh`, `C#`, `foo#3` (fragment-ish),
  and `#` inside an OSC 8 run (explicit link wins, as the bare-URL pass does).
- `owner/repo#NN` form resolves to that repo on the pane's origin host.
- `GH-NN` is out of scope; so is Jira/ADO. If a second tracker ever
  matters, this becomes a per-repository config knob, not a detector change.
- Hint destination is the full URL, displayed in the overlay like any
  other link (LINK-05/LINK-08 single source), so the user sees which repo
  it resolved to before activating.

**Tests (write-up):**
1. `links.rs` unit tests over a synthetic screen: each grammar case above,
   positive and negative, plus a wrapped row where `#12` is split across a
   `row_wrapped` boundary (must not join into `#1` + `2`, must not miss it).
2. A pane whose repository has no cached origin collects zero `#NN` links
   while still collecting bare URLs.
3. `owner/repo#NN` overrides the pane's own repo.
4. Ordering: `#NN` hints sort into the same top-to-bottom, left-to-right
   sequence as the other passes.
5. Cost: a test that asserts `collect_links` is the only entry point that
   calls the new pass (no call from `ui.rs`/draw), or a benchmark-ish test
   bounding a full-screen walk of dense `#NN` text to the same order as the
   bare-URL pass.
6. Origin cache: reconciliation refresh updates the cached origin when the
   remote changes; a checkout that moves between repositories re-resolves.

**Status:** open. Candidate for the next UX phase alongside link work.

### BL-09 — A checkout whose recorded `resume_id` has no transcript can never be reopened from the TUI

**Observation (2026-09-24, iarx-com / ai-scorecard, baude 2.4.0):** the row
is listed closed; archive/unarchive work; Enter shows `No conversation found
with session ID: 76eaf047-…` and `r` answers "this state has no
lifecycle-authorized manual retry". Reproducible on 2.4.1 code.

**Chain:**
1. `meta.rs` `apply_session_file` learns `session_id` from Claude's
   per-process `sessions/<pid>.json`, which Claude writes at startup, before
   any transcript exists. `save_durable_status` persists it as
   `session.resume_id`.
2. Open a checkout, never send a prompt, quit: no
   `projects/<cwd-dashed>/<id>.jsonl` is ever written. (Claude's transcript
   cleanup deleting one later has the same effect.)
3. Reopen (`app.rs` ~3138, and the standalone path ~1818) maps a present
   `resume_id` to `SpawnMode::ResumeId`; `backend/claude.rs` `spawn_plan`
   builds `exec claude --resume "$BAUDE_RESUME_ID"` with NO fallback, unlike
   `ContinueLatest`'s `--continue 2>/dev/null || exec claude`. Claude exits
   immediately. The pane keeps the dead screen.
4. Nothing clears `resume_id` on that failure, so every Enter repeats it. The
   row's lifecycle still reads `Running` with the dead child, which carries
   no `Retry*` capability, so `r` is refused.

**Fix (proposed):**
- Before choosing `ResumeId`, check
  `<CLAUDE_CONFIG_DIR>/projects/<cwd-dashed>/<id>.jsonl` exists; if not,
  downgrade to `ContinueLatest`. Deterministic and testable without Claude.
  Belt and braces: give the `ResumeId` shell the same `|| exec claude
  --continue … || exec claude` fallback shape.
- A child that exits within a few seconds of a targeted resume clears
  `resume_id` and leaves the row in a state where `r` is authorized, so the
  TUI cannot wedge on a bad id.

**Tests:** fixture with a session-file id and no transcript reopens with
`--continue`; a row whose child dies right after a targeted resume ends with
`resume_id: None` and `r` allowed; a row with a real transcript still gets
`--resume`.

**Workaround:** quit the baude holding the workspace lock, set that row's
`session.resume_id` to `null` in `~/.config/baude/state-<ws>.json`, relaunch.

**Status:** open.

### BL-10 — PTY children that exit on their own, or through `Pty::kill()`, are never reaped (permanent zombies)

**Observation (2026-09-24):** 15 `<defunct>` processes on the machine, every
one parented by a running baude 2.4.0 (10 under iarx-com, 3 under
poindexter12, 2 under joese-iarx), the oldest 26 hours. One zombie per
closed or failed session.

**Where:** `pty.rs` reader thread sets `exited = true` on EOF and never
waits; `is_exited()` then short-circuits on the flag and never reaches its
`try_wait`. `Pty::kill()` calls `child.kill()` and sets the flag, no wait.
`Session::kill()` uses it for agent and shell; app.rs calls that at 3586,
3854 and the BL-07 isolate path. Only `kill_and_wait()` reaps.

**Fix:** wait after EOF and after `kill()`; keep `is_exited` reaping when it
is first to observe exit; idempotent after reaping. Tests assert via
`ps -o stat= -p <pid>` that a self-exited child, a killed child, and a
`Session::kill()` pair are not `Z`.

**Also seen:** four `bash --noprofile --norc -i -c sh -c 'sleep 30'`
test-fixture shells reparented to launchd since 2026-09-22 05:57, each with a
defunct child. The suite leaked them when its harness died; separate from the
runtime leak but the same reaping discipline applies to fixtures.

**Status:** in progress (board SQ-3, dispatched 2026-09-24).
