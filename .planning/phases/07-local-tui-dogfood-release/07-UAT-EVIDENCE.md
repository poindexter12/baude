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
