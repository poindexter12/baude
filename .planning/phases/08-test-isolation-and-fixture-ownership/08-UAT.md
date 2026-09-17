---
status: complete
phase: 08-test-isolation-and-fixture-ownership
source: [08-VERIFICATION.md human_verification items]
started: 2026-09-16T18:24:00Z
updated: 2026-09-16T18:42:00Z
---

## Current Test

[testing complete]

## Tests

### 1. Real-roots before/suite/after triple against the DEVELOPER'S REAL roots
expected: before exit 0, suite exit 0, after exit 0 — all real roots (`~/.config/baude`, `~/.claude`, `~/.local/share/baude/worktrees`) byte-identical across the suite run.
result: pass
source: orchestrator-observed (2026-09-16, real roots, no env redirection)
reported: |
  before exit 0; `cargo test --workspace --locked -- --test-threads=1` exit 0 (646 test
  results reported, 0 FAILED); after exit 1.

  The `after` exit 1 is a CONFIRMED FALSE POSITIVE from ambient activity, proven by a
  control experiment, not by argument:

  - Suite run flagged two roots: `~/.config/baude` (breadcrumbs-claude.json 15429->17043,
    state-claude.json 208325->209671) and `~/Code` (new dir
    github.com/iarx-com/nexia-medication-label, created 11:32 during the run).
  - CONTROL: the same `before` / 90s idle / `after` triple was run with NO suite at all.
    It flagged the SAME two roots: breadcrumbs-claude.json touched (mtime moved, size
    identical 16854->16854 — a live app rewriting the same content) and the
    nexia-medication-label clone grew 224->256 as it continued fetching.
  - Therefore the flagged writes are the developer's LIVE baude instance plus an in-flight
    clone-on-demand of a work repo (git remote: iarx-com/nexia-medication-label), not the
    test suite.
  - The two roots the phase goal turns on were reported `unchanged` in BOTH the suite run
    and the control: Claude config root (`~/.poindexter/claude`) and the managed worktrees
    root (`~/.local/share/baude/worktrees`).
  - Corroboration: tests use Box::leak'd per-fixture workspace identities (plan 08-03) and
    never name the live workspace `claude`, which is the workspace whose state/breadcrumb
    files moved; and the prior verification run under XDG/CLAUDE_CONFIG_DIR redirection
    proved the suite writes nothing through the real-root resolver chain.

  KNOWN LIMITATION (recorded, not hidden): on a machine where baude is running live, this
  observer cannot by construction distinguish a suite write from an ambient write to the
  same root. The control experiment is what closes that gap here; a quiescent machine (or
  a run with baude stopped) would give a clean exit 0 and is the stronger future check.
  This reproduces the same ambient-write finding recorded for this phase on 2026-09-15.

### 2. Read-only `worktrees scan` against the real managed-worktree root
expected: A grouped summary prints; exit 0; nothing is removed (`find ~/.local/share/baude/worktrees -mindepth 2 -maxdepth 2` identical before and after); no directory holding a real checkout is reported removable.
result: pass
source: orchestrator-observed (2026-09-16, real data root, no --prune, no --yes)
reported: |
  `cargo run -q -p baude -- worktrees scan` exit 0. Grouped summary printed across three
  workspaces:

  | workspace  | candidates | removable | live | indeterminate |
  |------------|-----------:|----------:|-----:|--------------:|
  | claude     |          5 |         0 |    5 |             0 |
  | opencode   |       1263 |         0 |  130 |          1133 |
  | prerelease |        166 |         0 |   16 |           150 |
  | **total**  |   **1434** |     **0** | **151** |      **1283** |

  Inventory before and after the scan: 1434 entries, byte-identical listing (diff clean) —
  nothing removed. Zero directories reported removable, so no live checkout was at risk.

  This independently reproduces 08-07-SUMMARY.md's attested result (1433 candidates,
  0 removable, 150 live, 1283 indeterminate): indeterminate matches exactly at 1283; the
  +1 candidate / +1 live is a worktree created between that run and this one.

  The scan correctly reports the state inventory as INCOMPLETE, naming the six orphaned
  `.state-*.json.tmp-*` files plus legacy `state.json` / `daemon-state.json` — which is
  why every unreferenced candidate withholds clearance rather than qualifying as
  removable. That is the designed fail-closed behavior (plan 08-05, T-08-16).

## Summary

total: 2
passed: 2
issues: 0
pending: 0
skipped: 0
blocked: 0

## Gaps

[none]
