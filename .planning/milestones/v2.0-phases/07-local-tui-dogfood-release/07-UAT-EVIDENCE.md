---
audit_acknowledged:
  milestone: v2.0
  at: 2026-09-03
  gap_snapshot: "unknown::scenarios=0"
---

# Phase 07 Dogfood Evidence

## 2026-08-31 - Initial real-sidebar observation

**Evidence:** User-provided screenshot of the local `baude v2.0.0-beta` sidebar
running against real repositories. The exact installed source commit was not
independently captured in the screenshot, so this is visual feedback rather than
release certification.

### Observed

- Repository parents and their main checkout rows were both prominent and
  consecutively selectable, producing a duplicate-feeling navigation stop.
- The selected repository parent dominated the visual hierarchy even though the
  checkout/worktree is the actionable coding target.
- Parent and checkout styling was too similar to distinguish context from work.
- One unavailable repository (`iarx-com`) remained useful as a repository-level
  fallback because its checkout topology was missing.
- User also reported that opening an existing non-Git folder currently needs a
  supported workflow; such folders may intentionally never become repositories.

### Requested changes

- Skip repository parents in normal navigation whenever an available checkout
  exists; retain the parent as fallback when no checkout is available.
- Make checkout/worktree rows the primary unindented item and render repository
  context as muted/indented.
- Differentiate status and topology roles more clearly through color and style.
- Support non-Git folders as first-class standalone sessions, without synthetic
  repository identity.

### Implementation status

- Checkout-first navigation and inverted visual emphasis are implemented in the
  working tree and covered by focused hierarchy, viewport, and real-Git restart
  tests. Full validation is pending.
- Standalone non-Git folder support is now implemented through schema-v3 durable
  records, canonical-path deduplication, exact runtime ownership, root-level rows,
  close/reopen/missing recovery, and explicit Git-action refusals. Automated
  coverage passes; live folder UAT is recorded below and image screenshots are
  still required for formal visual sign-off.

## 2026-08-31 - Isolated local TUI dogfood

**Source:** dirty working tree based on
`d88fffb5f4f2322cc707e50996589b7596b458e7`; this is implementation evidence,
not release certification for an exact commit.

**Host/toolchain:** Darwin 25.6.0 arm64; `rustc 1.98.0`; `cargo 1.98.0`;
source and isolated installed binaries reported `baude 2.0.0-beta` and
`bauded 2.0.0-beta`.

**Isolation root:**
`/var/folders/q1/335pbzl13sz825k87q4ywl300000gq/T/opencode/baude-beta-dogfood.oq1rak`.
HOME, XDG config/data/state, repository, bare origin, install root, standalone
folder, and evidence directory were all below this root. The root is retained
temporarily so the recorded files can be inspected; it is not source-controlled.

### Observed passes

- Wide `160x40` rendering showed muted repository context, primary checkout,
  managed checkout, durable selection bands, role/status lines, and distinct
  lowercase close versus uppercase remove copy.
- Restart initialized selection on checkout key 1, the first available local
  checkout. Explicit `j` reselection reached the retained managed child; `enter`
  reopened it without a duplicate row or runtime.
- Narrow `40x12` sidebar focus preserved target, status, hierarchy context, and
  the literal `enter reopen · X remove · ? more` footer without panic. Claude
  focus intentionally showed the content pane; `ctrl+q` restored the sidebar.
- Repository key 1 had first-seen order 1. Main checkout key 1 had order 2.
  The managed child retained its key/order through close, restart, and reopen,
  then was absent after confirmed removal.
- Standalone key 1 had canonical path ending in `plain-folder` and first-seen
  order 5. Opening its symlink alias retained exactly one durable row and one
  runtime.
- Standalone agent, shell, editor (`BAUDE_EDITOR_CMD=/usr/bin/true`), info,
  activity, GSD, archive, close, reopen, and restart paths were exercised.
  Archive, shell-open intent, key, path, and order survived restart.
- Standalone `w` refused with “standalone folder, not a Git checkout”; uppercase
  `X` refused removal and stated that the folder was unchanged.
- Renaming the standalone folder produced one durable `missing` row and no
  standalone spawn. Restoring the exact path and pressing `enter` reconciled and
  reopened the same key, including its retained shell.
- Managed removal confirmation named
  `refs/heads/feature/dogfood-beta`, the exact managed path, and branch
  retention. After confirmation only the main worktree remained; the feature
  branch still resolved to seed commit
  `e1a67962c85d7b1d6e856c21c312ebc72cf9b867`.
- Final schema was 3. Final durable state retained repository key 1, main
  checkout key 1, and standalone key 1; the removed managed checkout was absent.

### Findings and corrections

- The first restart reproduced
  `ContradictoryLifecycle(CheckoutKey(1))`: startup restored a Running checkout,
  then positional-path admission redundantly reapplied the Active transition.
  `admit_repository` now lets `ensure_primary` focus the restored runtime. The
  regression
  `active_launch_repository_restart_focuses_restored_runtime_without_duplicate_spawn`
  passes with one checkout, one runtime, and one spawn.
- Clean removal initially refused because the Claude backend-generated
  `.claude/settings.local.json` is intentionally counted by the strict preflight,
  even when ignored. The refusal was preserved as evidence. The isolated fixture
  removed only that known generated file, then clean removal passed. The runbook
  now documents this exact guarded fixture cleanup.
- The `n` modal prefills the launch directory. The first scripted append produced
  a valid refusal for a concatenated path; the runbook now explicitly says to
  press `ctrl+u` before entering the standalone path.
- Isolating HOME exposed a host `.zshrc` include warning in spawned PTYs. It did
  not affect lifecycle behavior and is not attributed to baude.

### Preserved evidence

- 33 files under the temporary `evidence/` directory: wide/narrow ANSI terminal
  captures, standalone overlays/refusals/recovery captures, schema-v3 state
  snapshots, before/after worktree inventories, and branch-ref outputs.
- These are terminal captures, not image screenshots. Supported Linux/runtime
  certification, independent phase verification/Nyquist/UI sign-off, requirement
  completion, and publication authorization remain pending.

### Automated follow-up

- `cargo fmt --all -- --check`, workspace Clippy with `-D warnings`, all 345
  workspace tests, handoff JSON validation, and `git diff --check` passed after
  the live restart correction.
- Focused follow-up review found no high/medium issues. The regression was
  tightened to assert checkout selection, Claude focus, and absence of an
  admission error in addition to one checkout/runtime/session/spawn.

## 2026-09-01 - Clean-commit runbook re-certification (scripted PTY)

**Source:** clean working tree at exact commit
`6014b63a0c6103ca65f3671bede41980134545dc` (the commit the published
`v2.0.0-beta` prerelease tag points at; only pre-existing untracked
`opencode.json` present). This run re-certifies the runbook against an exact
commit, upgrading the 2026-08-31 dirty-tree evidence, and covers the two
behavior changes landed since: existing-worktree auto-population (`59e67b6`)
and seed-exempt safe removal (`6014b63`).

**Host/toolchain:** Darwin 25.6.0 arm64; `rustc 1.98.0`; `cargo 1.98.0`.
`cargo build --workspace --release --locked` and
`cargo install --path baude --root "$INSTALL_ROOT" --locked` passed; source,
workspace, and isolated installed binaries all reported `baude 2.0.0-beta` /
`bauded 2.0.0-beta`.

**Isolation root (retained):**
`/var/folders/q1/335pbzl13sz825k87q4ywl300000gq/T/baude-beta-dogfood.tOV3o8`
holding isolated HOME, XDG config/data/state, bare origin, clone, install
root, standalone folder, and 18 evidence files (ANSI captures, schema-v3
state snapshots, worktree inventories, branch refs).

**Method:** the TUI was driven by scripted keystrokes over a `script(1)` PTY
with fixed delays, and frames were re-rendered from the raw captures with a
terminal emulator. These are terminal captures, not image screenshots. The
`40x12` observation is a fresh launch at that size, not a mid-session resize.

### Observed passes

- Step 5 (wide `160x40` open): one primary main-checkout row (`● repository`,
  `↳ default · bypass`) under muted `repo · repository` context.
  RepositoryKey 1 / order 1; CheckoutKey 1 / order 2 on `refs/heads/main`.
- Step 6: `w feature/dogfood-beta` created exactly one managed child
  (key 2, order 3) after the older main child; repeating `w` with the same
  branch did not add a row (worktree inventory stayed at two). The lowercase
  `x` modal read "Close session “repository:feature/dogfood-beta” and keep
  its checkout"; after confirm the child persisted with `active_intent`
  false and unchanged key/order, and `refs/heads/feature/dogfood-beta`
  resolved to the seed commit.
- Step 7: restart initialized selection at the first available local
  checkout (band on the primary; child rendered `○ … · closed`). Explicit
  `j` + `enter` reopened the child: one child row, one runtime, band on the
  child, pane titled `repository:feature/dogfood-beta`. Fresh `40x12`
  launch rendered hierarchy context, both rows with status glyphs, selection
  band, and the `enter attach · x close · ? more` footer without panic; both
  retained sessions respawned with no duplicates.
- Step 8 (standalone): `n` + `ctrl+u` + absolute path opened the plain
  folder as a root-level row sorted before the repository group. Schema 3;
  StandaloneKey 1, order 5, canonical `/private/...` path. Opening a symlink
  alias kept exactly one durable row. `w` refused with "this is a standalone
  folder, not a Git checkout"; uppercase `X` refused with "standalone
  sessions have no branch or worktree removal authority. The folder is
  unchanged." — and the folder's content was verified unchanged. Renaming
  the folder produced a durable `missing` row with no spawn; restoring the
  exact path and pressing `enter` reconciled the SAME key/order back to
  running.
- Step 9 (seed-exempt removal — NEW semantics): the managed worktree
  contained the Claude-seeded `.claude/settings.local.json` (verified with
  `test -f` immediately before removal) and NO manual deletion was
  performed. Uppercase `X` on the primary was refused ("not a baude-managed
  linked worktree") — the unmanaged guard held. On the managed child the red
  confirmation read "Remove this clean baude-managed linked worktree?",
  named `branch: refs/heads/feature/dogfood-beta`, and stated the local
  branch is retained and parent/siblings unchanged. After `enter`:
  "worktree removed — local branch refs/heads/feature/dogfood-beta
  retained"; inventory dropped to exactly one worktree; the branch ref
  survived at the seed commit; durable state retained only checkout key 1
  and standalone key 1.
- Step 10: skipped — no isolated daemon existed; no remote observation is
  claimed.

### Honest gaps and notes

- Standalone editor/info/activity/GSD/archive/close/reopen sub-actions were
  not re-exercised in this run; they remain covered by the 2026-08-31 live
  evidence and automated tests.
- Two scripted navigation misfires occurred before step 9 succeeded (a `j`
  that landed on the primary produced the genuine unmanaged-removal refusal
  recorded above; an earlier `j j` reopened the child instead of the
  standalone row). Both were harmless and re-run with corrected navigation.
- The isolated-HOME `.zshrc` include warning inside spawned PTYs recurred;
  fixture-only, not attributed to baude.
- Selection-contract observation for verification: before the standalone row
  existed (step 7), restart selection initialized on the repository's first
  available checkout as documented. Once the standalone row existed and was
  available (step 9), restart selection initialized on the standalone row,
  which sorts above the repository group. Phase verification should decide
  whether "first available local checkout" is meant to cover standalone rows
  or whether checkouts should win initialization over standalone sessions.
- Image screenshots, supported Linux/runtime certification, independent
  phase verification/Nyquist/UI sign-off, and requirement completion remain
  pending.

## 2026-09-02 - Linux and CI matrix certification (PR #56)

**Source:** commit `da1b7f2` on `gsd/phase-07-local-tui-dogfood-release` (the
commit the re-cut `v2.0.0-beta` prerelease targets), after merging main
(v0.14.1 + ctrl-x passthrough) into the branch — required because the draft
PR was CONFLICTING and GitHub does not run `pull_request` workflows without a
merge ref, which is why no CI had ever executed for this branch.

**Evidence (GitHub Actions, PR #56, run 33641979151):**

- `check (ubuntu-22.04)`: PASS — `cargo fmt --check`, `cargo clippy
  --all-targets -- -D warnings`, and the full `cargo test` executed on Linux:
  53 (baude, including the isolated real-Git dogfood subprocess) + 220
  (baude-core) + 79 (bauded) tests, zero failures. This is the first Linux
  execution of the phase-6 `cfg(linux)` process-identity path (compile fixed
  in `0c24995`), the pre-exec registration gate, and descendant
  process-group teardown tests — the Linux/runtime certification gate.
- `check (macos-14)`: PASS (same steps).
- `artifact-readiness` on all four supported targets (both Linux and both
  macOS): PASS — locked release builds, versioned `baude`/`bauded` archive
  verification.
- `docker`: PASS. CodeQL analyses: PASS.

**Findings fixed en route (each verified on a subsequent green run):**

- Seven baude tests failed only on ubuntu because git 2.34 refuses non-force
  `git worktree remove` over untracked files (the Claude-seeded
  `.claude/settings.local.json`) while git 2.50 on the dev machine permits
  it — verified empirically. All raw test-fixture worktree removals now pass
  `--force` (`f00c316`); the fail-closed removal contract remains pinned by
  the preflight and verified-removal product tests. Design note recorded: on
  modern git the status-based preflight is the ONLY untracked-file gate for
  removal; git's own refusal is a backstop only on older git.
- Restart selection initialization now prefers checkouts over standalone
  rows (`da1b7f2`), closing the selection-contract question per the owner's
  decision.

The `v2.0.0-beta` prerelease was re-cut at `da1b7f2` with all four tarballs
and checksums; the local mise prerelease lane was refreshed and reports
`baude 2.0.0-beta`.
