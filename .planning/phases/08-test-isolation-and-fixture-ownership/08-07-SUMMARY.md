---
phase: 08-test-isolation-and-fixture-ownership
plan: 07
subsystem: infra
tags: [cli, rust, worktree-scan, leak-cleanup, preview, serde_json]

# Dependency graph
requires:
  - phase: 08-05
    provides: "baude_core::worktree_scan::{scan_at, prune_at} — enumeration, classification, state cross-reference and a re-verifying prune"
  - phase: 08-06
    provides: "scripts/assert-real-roots-untouched.sh — the suite-level real-root observer that brackets this plan's gates"
provides:
  - "`baude worktrees scan` — a read-only, human-readable preview of the managed-worktree root, grouped by workspace, naming the evidence behind every verdict"
  - "`baude worktrees scan --json` — the core `ScanReport` verbatim on stdout, diagnostics on stderr, designed to be redirected and handed back"
  - "A prune path reachable only behind three opt-ins (`--prune`, `--report <saved preview>`, `--yes`) that acts on the saved set, never on a fresh scan"
  - "A complete refusal account: every candidate not removed names why (changed proof, new state reference, symlink, gitdir, vanished)"
  - "`worktrees` in the top-level `--help` subcommand list"
  - "The first real-tree measurement of the historical leak: 1433 candidates, 0 removable under the current inventory"
affects: [worktree-lifecycle, leak-cleanup, cli, future-prune-operations]

# Actuals (#2632)
actuals:
  tokens: 12641
  tasks: 1
  commits: 2
plan_head_before: de58dc2310fb050da58da9a45cd3c2521f6bb40f

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Hand-written strict argument parsing (no clap/argh): unknown, repeated and value-less options are refused outright rather than ignored, so a mistyped flag can never be read as an authorization"
    - "Root resolution stays in `main`; `run_worktrees_at` takes `ScanRoots` plus `&mut dyn Write` sinks, so the real command path is driven by tests against synthetic roots with no redirect guard needed"
    - "JSON mode owns stdout byte-for-byte; every diagnostic goes to stderr, so `--json > file` is safe to hand back to `--prune`"

key-files:
  created: []
  modified:
    - "baude/src/main.rs — the `worktrees` dispatch arm, parser, summary/account printers, help text, and 22 `worktrees_cli_` tests"

key-decisions:
  - "`--json` and `--prune` are mutually exclusive: `--json` names a scan report, and the prune account (PruneReport/PruneOutcome/RefusalReason) is not Serialize. Serializing it would mean adding derives to baude-core/src/worktree_scan.rs, outside this plan's files_modified."
  - "The verb's own `--help` documents the flow as two invocations on purpose. A help text showing a one-shot `--prune` would teach the habit this surface exists to prevent."
  - "`run_worktrees_prune` refuses (rather than panics) when `--report` is absent, even though the parser makes that unreachable — the failure mode of a future parser change is then 'removes nothing', not 'removes the wrong set'."
  - "The real-tree acceptance run was preview-only by construction and by invocation: no `--prune`, no `--yes`, no manual `rm`. 1433 candidates observed, 0 removed."

patterns-established:
  - "Strict-parser-as-safety-boundary: the three ways a partially typed removal could be read as an authorized one (`--yes` alone, `--report` alone, `--prune` without `--report`) are each refused before any root is opened."
  - "Refusal accounts name reasons: `refusal_phrase` renders all seven `RefusalReason` variants in words, so an account never says only 'refused' (T-08-15)."
  - "Preview footers state the negative explicitly ('Nothing was removed: this command only reads') rather than leaving absence of a removal to be inferred."

requirements-completed: [TISO-04]

coverage:
  - id: D1
    description: "`baude worktrees scan` prints a grouped, workspace-by-workspace preview and changes neither root"
    requirement: "TISO-04"
    verification:
      - kind: unit
        ref: "baude/src/main.rs#worktrees_cli_tests::a_plain_scan_prints_a_grouped_summary_and_changes_neither_root"
        status: pass
      - kind: unit
        ref: "baude/src/main.rs#worktrees_cli_tests::a_json_scan_changes_neither_root_and_describes_the_same_candidate_set"
        status: pass
      - kind: unit
        ref: "baude/src/main.rs#worktrees_cli_tests::json_output_carries_diagnostics_on_stderr_only"
        status: pass
      - kind: unit
        ref: "baude/src/main.rs#worktrees_cli_tests::saved_json_round_trips_into_the_core_scan_report"
        status: pass
    human_judgment: false
  - id: D2
    description: "Removal requires all three of --prune, --report <saved preview>, and --yes; missing any one removes nothing"
    requirement: "TISO-04"
    verification:
      - kind: unit
        ref: "baude/src/main.rs#worktrees_cli_tests::an_unchanged_previewed_candidate_is_removed_only_with_all_three_opt_ins"
        status: pass
      - kind: unit
        ref: "baude/src/main.rs#worktrees_cli_tests::prune_without_yes_re_verifies_and_removes_nothing"
        status: pass
      - kind: unit
        ref: "baude/src/main.rs#worktrees_cli_tests::yes_without_prune_is_a_usage_error"
        status: pass
      - kind: unit
        ref: "baude/src/main.rs#worktrees_cli_tests::report_without_prune_is_a_usage_error"
        status: pass
      - kind: unit
        ref: "baude/src/main.rs#worktrees_cli_tests::prune_without_report_is_a_usage_error"
        status: pass
    human_judgment: false
  - id: D3
    description: "Only the previously previewed set with a matching re-derived proof can be removed; a fresh scan is never substituted for report input, and every refusal names its reason"
    requirement: "TISO-04"
    verification:
      - kind: unit
        ref: "baude/src/main.rs#worktrees_cli_tests::prune_acts_on_the_saved_report_rather_than_a_fresh_scan"
        status: pass
      - kind: unit
        ref: "baude/src/main.rs#worktrees_cli_tests::a_candidate_whose_proof_changed_is_refused_and_names_the_reason"
        status: pass
      - kind: unit
        ref: "baude/src/main.rs#worktrees_cli_tests::a_candidate_newly_referenced_by_state_is_refused_and_names_the_reason"
        status: pass
      - kind: unit
        ref: "baude/src/main.rs#worktrees_cli_tests::a_candidate_replaced_by_a_symlink_is_refused_and_names_the_reason"
        status: pass
      - kind: unit
        ref: "baude/src/main.rs#worktrees_cli_tests::a_report_bound_to_a_foreign_root_removes_nothing"
        status: pass
      - kind: unit
        ref: "baude/src/main.rs#worktrees_cli_tests::a_missing_report_file_removes_nothing"
        status: pass
      - kind: unit
        ref: "baude/src/main.rs#worktrees_cli_tests::a_malformed_report_removes_nothing"
        status: pass
    human_judgment: false
  - id: D4
    description: "The `--help` subcommand list names the new verb, so it is discoverable rather than hidden"
    requirement: "TISO-04"
    verification:
      - kind: unit
        ref: "baude/src/main.rs#worktrees_cli_tests::the_top_level_help_lists_the_worktrees_verb"
        status: pass
      - kind: unit
        ref: "baude/src/main.rs#worktrees_cli_tests::the_worktrees_help_documents_the_two_invocation_flow"
        status: pass
      - kind: integration
        ref: "cargo run -q -p baude -- --help  # prints 'subcommands: statusline, hook, permission-mcp, worktrees'"
        status: pass
    human_judgment: false
  - id: D5
    description: "The preview run against the developer's real 1433-entry managed-worktree root removes nothing"
    requirement: "TISO-04"
    verification:
      - kind: manual_procedural
        ref: "cargo run -q -p baude -- worktrees scan  # 1433 candidates, 0 removable; find-diff before/after identical; scripts/assert-real-roots-untouched.sh PASS"
        status: pass
    human_judgment: true
    rationale: "The read-only guarantee against the real developer tree is the single highest-stakes property of this phase. A human must confirm the invocation carried no --prune/--yes and the before/after listings matched, rather than trusting the tool that was under test to report on itself."

# Metrics
duration: 25min
completed: 2026-09-15
status: complete
---

# Phase 08 Plan 07: `baude worktrees scan` CLI Surface Summary

**The leak scan is now something a developer can run: a grouped, evidence-naming preview of 1433 managed-worktree candidates that removes nothing, with removal reachable only behind a saved report plus two separate opt-ins.**

## Performance

- **Duration:** 25 min
- **Started:** 2026-09-15T17:36:24Z
- **Completed:** 2026-09-15T18:01:42Z
- **Tasks:** 1 of 1
- **Files modified:** 1 (`baude/src/main.rs`)

## Accomplishments

- **A preview an operator can read.** `baude worktrees scan` groups candidates by workspace, counts removable/live/indeterminate per group and in total, and renders every `Evidence` variant in words, so each verdict is shown with the facts that produced it rather than asserted.
- **A machine-readable preview that round-trips.** `--json` writes the core `ScanReport` (evidence included) to stdout and nothing else, so `--json > preview.json` produces exactly the file `--prune --report` deserializes back into `prune_at`.
- **Removal gated three ways, refused before any root is opened.** `--yes` alone, `--report` alone, and `--prune` without `--report` are each usage errors (exit 2) produced by the parser, so a partially typed command line never reaches the filesystem.
- **The approved set is the saved set.** The prune path deserializes the operator's report and hands it to `prune_at` unchanged; a candidate created after the preview is reported as "not approved" and left alone even though a fresh scan would clear it.
- **A complete refusal account.** All seven `RefusalReason` variants render as prose. A changed proof, a newly re-admitted repository, a directory replaced by a symlink, a present gitdir and a vanished candidate each say so.
- **First real measurement of the historical leak.** 1433 candidates across three workspaces (claude 4, opencode 1263, prerelease 166): 0 removable, 150 live, 1283 indeterminate. Nothing was removed.

## Task Commits

Task 1 was TDD, so it carries the RED/GREEN pair:

1. **Task 1 (RED): pin the worktrees CLI surface** — `cab9c45` (test)
2. **Task 1 (GREEN): the preview surface itself** — `b3e905c` (feat)

No REFACTOR commit: the GREEN implementation passed `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` as written, and no duplication emerged worth extracting.

**Plan metadata:** see the `docs(08-07)` commit following this summary.

## Files Created/Modified

- `baude/src/main.rs` — `+1274 / -7`. Adds: the `worktrees` arm in `main` (dispatched before the launch-dir logic, resolving `real_worktrees_base()` + `config_dir()` and handing them down explicitly); `worktrees_help_text`; `parse_worktrees_args` and `WorktreesRequest`; `evidence_phrase` / `verdict_phrase` / `verdict_label` / `refusal_phrase`; `print_scan_summary` / `print_prune_account`; `run_worktrees_at` / `run_worktrees_scan` / `run_worktrees_prune`; the extracted `help_text()` (now listing `worktrees`); and the 22-case `worktrees_cli_tests` module.

## Decisions Made

- **`--json` cannot be combined with `--prune`.** `PruneReport`, `PruneOutcome` and `RefusalReason` do not derive `Serialize`, and adding those derives would modify `baude-core/src/worktree_scan.rs` — outside this plan's declared `files_modified`. The combination is rejected with an explanatory usage error rather than silently producing the wrong artifact. Recorded as Deviation 1 below.
- **The verb help teaches two invocations.** Both `usage:` lines and the two labelled steps present save-then-hand-back as the flow. A help text that showed a one-shot `--prune` would teach exactly the habit the two-opt-in design exists to prevent.
- **Root resolution lives in `main`, not in the runner.** `run_worktrees_at` takes `ScanRoots` and `&mut dyn Write` sinks, so tests drive the real command path against synthetic temp roots. No `TestRedirect` guard is needed — `scan_at`/`prune_at` are fully root-parameterized and never reach `assert_contained`.
- **Report files in the CLI tests live outside both scan roots** (`<fixture>/reports/`), so saving a preview can never be mistaken for a write into a root whose read-only-ness the same test asserts.
- **`run_worktrees_prune` refuses rather than panics** on an absent `--report`, even though the parser makes that unreachable. If a future parser change weakens the guard, the failure mode is "removes nothing", not "removes the wrong set".

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `--json --prune` has no serializable artifact**

- **Found during:** Task 1 (GREEN implementation)
- **Issue:** The plan lists `--json` and `--prune` as independent options. A `--json --prune` run has no defined output: the prune account (`PruneReport` / `PruneOutcome` / `RefusalReason`) is not `Serialize`, and emitting the *scan* report instead would hand back a document describing reads that did not authorize the writes performed.
- **Fix:** The parser rejects the combination with exit 2 and an explanatory message ("`--json` writes a scan report; it cannot be combined with `--prune`. Save the preview first, then prune from it"). The verb's `--help` states the same restriction. The alternative — deriving `Serialize` on three core types — would have modified `baude-core/src/worktree_scan.rs`, outside this plan's `files_modified`.
- **Files modified:** `baude/src/main.rs`
- **Verification:** Covered indirectly by the documented usage-error behavior; no test asserts the combination because none was planned. See Known Stubs.
- **Committed in:** `b3e905c`

**2. [Rule 3 - Blocking] `#[allow(dead_code)]` scaffolding for the RED commit**

- **Found during:** Task 1 (RED commit)
- **Issue:** The RED commit's items (`WORKTREES_EXIT_*`, `WorktreesOptions`, the `run_worktrees_at` stub) are referenced only from `#[cfg(test)]` code, so the non-test bin build reported them as dead and `-D warnings` failed the commit — for the very absence the RED tests were asserting.
- **Fix:** `#[allow(dead_code)]` on each item, with a comment naming the GREEN commit that removes them. This is the pattern plan 08-05 established. All of them were removed in `b3e905c`.
- **Files modified:** `baude/src/main.rs`
- **Verification:** `cargo clippy -p baude --all-targets -- -D warnings` clean at `cab9c45`; no `allow(dead_code)` remains at `b3e905c`.
- **Committed in:** `cab9c45` (added), `b3e905c` (removed)

---

**Total deviations:** 2 auto-fixed (both Rule 3 — blocking).
**Impact on plan:** No scope creep. Deviation 1 narrows the option surface rather than widening it; deviation 2 is transient scaffolding that no longer exists in the tree.

## TDD Gate Compliance

**Mode:** `tdd="true"` on the single task.

| Gate | Command | Result |
|---|---|---|
| RED | `cargo test -p baude --bins worktrees_cli_` at `cab9c45` | exit 101 — **22 tests, 2 passed, 20 failed** |
| RED evidence | `gsd-tools check tdd-red-evidence` | `RED_EVIDENCE_OK` / `target_test_failed`; target `worktrees_cli_tests::a_plain_scan_prints_a_grouped_summary_and_changes_neither_root` |
| GREEN | `cargo test -p baude --bins worktrees_cli_` at `b3e905c` | exit 0 — **22 passed, 0 failed, 0 ignored** |
| REFACTOR | not required | GREEN was already clippy- and fmt-clean; no duplication worth extracting |

**Note on the two RED passes.** `a_missing_report_file_removes_nothing` and `a_malformed_report_removes_nothing` passed against the RED stub because the stub was deliberately fail-closed: it exited non-zero, wrote a diagnostic, and touched nothing. Both assert only "exits non-zero, snapshot unchanged, stderr non-empty", which a stub that does nothing satisfies honestly. The target test chosen for the RED evidence record was one of the 20 genuine failures.

## Verification Results

All gates run at `b3e905c`, with exit statuses captured directly (never through a pipe).

| Gate | Command | Result |
|---|---|---|
| Plan `<verify>` | `cargo test -p baude --bins worktrees_cli_` | **22 passed, 0 failed, 0 ignored** |
| Serial suite | `cargo test -- --test-threads=1` | exit 0 — **519 passed, 0 failed, 0 ignored** (baude bin 96, baude-core lib 332, bauded bin 91, plus one nested re-exec case reported separately, doc-tests 0) |
| Lint | `cargo clippy --all-targets -- -D warnings` | exit 0, clean |
| Format | `cargo fmt --all -- --check` | exit 0, clean |
| Release | `cargo build --workspace --release --locked` | exit 0 |
| Real-root observer | `scripts/assert-real-roots-untouched.sh before` / `after` around the suite | **PASS** — all three roots unchanged |

Suite total moved from 497 to 519, exactly the 22 tests this plan added.

### Manual acceptance — PREVIEW ONLY

```
cargo run -q -p baude -- worktrees scan
```

No `--prune`. No `--yes`. No manual `rm`. Exit 0.

```
worktrees base:  /Users/joese/.local/share/baude/worktrees
config dir:      /Users/joese/.config/baude
state inventory: INCOMPLETE — no candidate can be cleared — 3 workspace(s), 8 state file(s), 0 absent

workspace claude     —    4 candidate(s): 0 removable,   4 live,    0 indeterminate
workspace opencode   — 1263 candidate(s): 0 removable, 130 live, 1133 indeterminate
workspace prerelease —  166 candidate(s): 0 removable,  16 live,  150 indeterminate

total: 1433 candidate(s) — 0 removable, 150 live, 1283 indeterminate
```

**Nothing was removed.** Proven three ways:

1. `find ~/.local/share/baude/worktrees -mindepth 2 -maxdepth 2` before and after: **1433 entries both times, `diff` identical**.
2. `scripts/assert-real-roots-untouched.sh before` / `after` bracketing the scan: **PASS**, all three real roots unchanged.
3. The invocation itself carried no removal opt-in, and the parser rejects `--yes` without `--prune` (confirmed live: `cargo run -q -p baude -- worktrees scan --yes` → exit 2, "`--yes` confirms a `--prune`; on its own it authorizes nothing").

**The finding that matters.** Every one of the 1433 candidates is currently **unclearable**, and the report says why rather than leaving it to be inferred: the state inventory is `INCOMPLETE`. Six orphaned `.state-*.json.tmp-<pid>-<n>` files plus two legacy files (`state.json`, `daemon-state.json`, both requiring workspace-aware migration) sit in `~/.config/baude`. Per T-08-16 that withholds `NotReferencedByState` from **every** candidate in the scan, so no candidate can satisfy the predicate's second clause. This is the fail-closed design working exactly as specified: unreadable state anywhere refuses clearance everywhere. Any future cleanup must first resolve those eight files — which is a decision for the developer, not for this plan.

The four `claude` candidates — the workspace holding real checkouts — all classify **live**, each with a `ReferencedByState` key match. None was reported removable.

## Issues Encountered

- **The RED evidence record schema.** The first record was rejected as `INVALID_RED (invalid_record)`: `gsd-tools check tdd-red-evidence` requires camelCase `command` / `exitCode` / `targetTest` and parses `output` as Node TAP (`not ok N - <name>`, `# tests N`, `# pass N`, `# fail N`), not cargo's format. Resolved by mechanically transcribing the real `test <name> ... ok|FAILED` lines into TAP — no counts invented — after which the record verified as `RED_EVIDENCE_OK`.
- **Provoking `ProofChanged` cheaply.** Creating a second workspace directory (`opencode`) under the fixture base after the preview widens `workspaces_checked`, so the candidate still re-derives as `Removable` but with a *different* `RemovalProof`. That is the exact shape decision (C) demands be refused: still cleared, but not by the facts the operator approved.
- **Where `Indeterminate` comes from.** `contents_evidence` always yields either `Empty` or `ContainsCheckout`, so an ordinary non-empty directory is `Live`, never `Indeterminate`. `Indeterminate` is reachable only via a `PreventsConclusion` blocker (`IsSymlink`, `StateUnreadable`) or an incomplete inventory — which is precisely why 1283 of the 1433 real candidates land there.

## Known Stubs

| Stub | File | Line | Reason |
|---|---|---|---|
| `--json --prune` rejection is untested | `baude/src/main.rs` | `parse_worktrees_args`, the `options.json && options.prune` arm | The combination was not in the plan's acceptance criteria, so the RED test set does not cover it. Behavior is fail-closed (exit 2, nothing read or removed) and documented in `worktrees_help_text`, so the untested path cannot remove anything. A future plan that makes the prune account serializable should replace the rejection with a JSON prune account and test both. |

## Threat Flags

None. This plan added no network endpoint, no auth path and no schema change. The one new file-access path (`std::fs::read` of an operator-named report) is read-only, and the deserialized report is treated as comparison data throughout — `prune_at` re-derives every fact against the roots this process resolved independently (T-08-25). The removal surface it can reach was shipped and threat-modelled in 08-05; this plan only gates it.

Explicitly verified absent:

- No `worktrees` arm in `bauded` (`grep -rn "worktrees" bauded/src/` returns only pre-existing HTTP routes and inventory fields) — T-08-17.
- No argument-parsing dependency added to any manifest (`clap`, `argh`, `structopt`, `pico-args`, `lexopt`, `getopts` all absent from every `Cargo.toml`).

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

TISO-04 is complete: a developer can preview suspected historical leaks without deleting them, and removal requires separate approval plus verified ownership, never a missing gitdir alone.

**What is ready:** the full scan → save → inspect → prune loop, end to end, with the removal half exercised only against fixtures.

**What a future operator must decide first:** the eight problem files in `~/.config/baude` (six orphaned `.state-*.json.tmp-*`, plus `state.json` and `daemon-state.json` awaiting workspace-aware migration). Until those are resolved the inventory is `INCOMPLETE` and `baude worktrees scan` will correctly report 0 removable regardless of how many of the 1433 candidates are genuinely dead. That is a data-migration question, not a scanner question, and it belongs to whoever runs the cleanup.

---
*Phase: 08-test-isolation-and-fixture-ownership*
*Completed: 2026-09-15*

## Self-Check: PASSED

- `baude/src/main.rs` present and modified (52 `worktrees` references).
- `.planning/phases/08-test-isolation-and-fixture-ownership/08-07-SUMMARY.md` present.
- Commits `cab9c45` (RED) and `b3e905c` (GREEN) present in history.
