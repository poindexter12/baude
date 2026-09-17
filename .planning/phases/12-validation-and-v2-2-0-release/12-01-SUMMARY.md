---
phase: 12-validation-and-v2-2-0-release
plan: 01
subsystem: infra
tags: [clippy, ci, rust, vendored-fork, release-build, lint]

# Dependency graph
requires:
  - phase: 10-terminal-link-handling
    provides: the link detection and link-hints overlay code whose two masked lints this plan fixed, plus the link_fidelity/kitty_keyboard suites that proved the rewrites behavior-preserving
  - phase: 08-real-root-isolation
    provides: scripts/assert-real-roots-untouched.sh — the --self-test/before/after bracket used unchanged as gates 3, 4 and 6
  - phase: 11-negotiated-multiline-input
    provides: the deferred-items ledger whose clippy entry recorded a wrong cause
provides:
  - "cargo clippy --all-targets -- -D warnings at exit 0 from a clean tree, with baude's own crates demonstrably linted"
  - "A commented FORK (baude) lint-relaxation block in vendor/vt100/src/lib.rs — the fork's entire clippy diff-vs-upstream surface"
  - "Local per-gate exit-code evidence for all seven CI-parity gates (SHIP-01's local half)"
  - "Proof that cargo build --workspace --release --locked succeeds with the vendored path dependency in the workspace"
  - "A phase-11 deferred ledger that records the true cause of the red clippy gate"
affects: [12-02, 12-03, 12-04, phase-8-re-stamp, release-v2.2.0]

actuals:
  tokens: 1310
  tasks: 3
  commits: 2
  plan_head_before: 2d8c0ff9f09383c92865e273ffbbeafa9465d0f0

tech-stack:
  added: []
  patterns:
    - "Vendored-fork lint isolation: a fork's in-source lint header is relaxed inside one commented FORK (baude) block rather than fixed site-by-site, keeping the diff-vs-upstream surface auditable"
    - "Gate-ladder evidence: each CI-parity gate's exit code captured directly into a result file, never through a pipe"

key-files:
  created: []
  modified:
    - vendor/vt100/src/lib.rs
    - baude/src/links.rs
    - baude/src/ui.rs
    - .planning/phases/11-negotiated-multiline-input/deferred-items.md

key-decisions:
  - "Relaxed the vendored fork's clippy group opt-ins rather than rewriting 30 upstream sites or adding publishing metadata to three never-published manifests (D-04, D-14)"
  - "Used clippy::all/pedantic/nursery/cargo group allows inside the fork instead of the ten named lints, because a named list drifts every time the runner's clippy advances and this fork is meant to be frozen against upstream"
  - "Kept the fork's two narrow warn lines (as_conversions, get_unwrap) active — the relaxation is scoped to the group opt-ins only"
  - "Ran the locked release build as the packaging verification rather than any packaging rework (D-10)"

patterns-established:
  - "FORK (baude) comment marker: any deliberate divergence from vendored upstream carries a grep-able marker naming the reason and bounding the diff surface"

requirements-completed: [SHIP-01]

coverage:
  - id: D1
    description: "cargo clippy --all-targets -- -D warnings reaches a clean exit 0 from a clean tree, with baude's own crates actually linted rather than skipped behind a failed dependency"
    requirement: "SHIP-01"
    verification:
      - kind: other
        ref: "cargo clippy --all-targets -- -D warnings (exit 0; output contains 'Checking baude v2.1.5')"
        status: pass
      - kind: other
        ref: "cargo clippy --workspace --all-targets -- -D warnings (exit 0)"
        status: pass
    human_judgment: false
  - id: D2
    description: "The clippy group relaxation is confined to the vendored fork and did not leak into baude's own crate sources"
    requirement: "SHIP-01"
    verification:
      - kind: other
        ref: "! grep -rn --include='*.rs' -E 'allow\\(clippy::(all|pedantic|nursery|cargo)\\)' baude/src baude-core/src bauded/src (exit 0, zero hits)"
        status: pass
      - kind: other
        ref: "grep -n 'FORK (baude)' vendor/vt100/src/lib.rs (exit 0, hit at line 36)"
        status: pass
    human_judgment: false
  - id: D3
    description: "The two mechanical rewrites in baude's own source (manual_flatten in links.rs, manual_clamp in ui.rs) preserved behavior"
    requirement: "SHIP-01"
    verification:
      - kind: unit
        ref: "cargo test -p baude links -- --test-threads=1 (21 passed, 0 failed)"
        status: pass
      - kind: integration
        ref: "vendor/vt100/tests/link_fidelity.rs (16 passed, 0 failed)"
        status: pass
      - kind: integration
        ref: "vendor/vt100/tests/kitty_keyboard.rs (14 passed, 0 failed)"
        status: pass
    human_judgment: false
  - id: D4
    description: "The full six-command CI-parity bracket passes locally in CI's order with every exit code observed directly"
    requirement: "SHIP-01"
    verification:
      - kind: other
        ref: "cargo fmt --check; cargo clippy --all-targets -- -D warnings; assert-real-roots-untouched.sh --self-test|before; cargo test -- --test-threads=1; assert-real-roots-untouched.sh after (all exit 0; 645 tests, 0 failures)"
        status: pass
    human_judgment: false
  - id: D5
    description: "The workspace test suite left all four real user roots untouched"
    requirement: "SHIP-01"
    verification:
      - kind: other
        ref: "bash scripts/assert-real-roots-untouched.sh after (exit 0; 'PASS: the run left all four real roots untouched.')"
        status: pass
      - kind: other
        ref: "bash scripts/assert-real-roots-untouched.sh --self-test (exit 0; 40 checks, 0 failures)"
        status: pass
    human_judgment: false
  - id: D6
    description: "The vendored path dependency survives a locked release build (the packaging verification, D-10)"
    requirement: "SHIP-01"
    verification:
      - kind: other
        ref: "cargo build --workspace --release --locked (exit 0; Cargo.lock unchanged)"
        status: pass
    human_judgment: false
  - id: D7
    description: "The phase-11 deferred ledger records the true cause of the red clippy gate and points at its closure in phase 12"
    verification:
      - kind: other
        ref: "! grep -niE 'presumed green' .planning/phases/11-negotiated-multiline-input/deferred-items.md (exit 0, zero hits)"
        status: pass
      - kind: other
        ref: "grep -nE 'links\\.rs' && grep -nE 'ui\\.rs' && grep -nE '12-01|phase 12|Phase 12' on the ledger (exit 0)"
        status: pass
    human_judgment: false

# Metrics
duration: 21min
completed: 2026-09-16
status: complete
---

# Phase 12 Plan 01: Local CI-Parity Gate Ladder Summary

**Clippy goes from exit 101 to exit 0 by relaxing the vendored vt100 fork's own lint header inside one commented `FORK (baude)` block, which uncovered and fixed two genuine phase-10 lints that the fork's compile failure had been hiding — then all seven CI-parity gates plus the locked release build proved green locally.**

## Performance

- **Duration:** ~21 min
- **Started:** 2026-09-16T17:26:00Z (approx.)
- **Completed:** 2026-09-16T17:47:00Z
- **Tasks:** 3
- **Files modified:** 4

## Accomplishments

- **The clippy gate is green for the right reason.** `vendor/vt100` is a first-class workspace member, and its upstream lint header (`#![warn(clippy::{cargo,pedantic,nursery})]`) made it fail to compile under `-D warnings`. Because `baude-core` depends on it and `baude` on `baude-core`, cargo stopped before ever checking baude's own crates — the gate was red for 40 fork diagnostics while two real lints in baude's source sat invisible behind them. Relaxing the fork uncovered both; the run now reaches `Finished` with `Checking baude v2.1.5` in its output, which is the proof the dependent crates were actually reached rather than skipped.
- **The relaxation is confined to one commented block.** A grep gate over `baude/src`, `baude-core/src`, and `bauded/src` confirms zero clippy group relaxations leaked into baude's own sources. Neither prohibition was touched: no publishing metadata was added to the three manifests, and the `test-support` feature was not renamed.
- **The full ladder is green locally, per-gate.** All seven gates exit 0 with each exit code captured directly into a result file — never piped to `grep`/`tail` and then read from `$?`, which is recorded project policy precisely because it masks the real failure.
- **645 tests, 0 failures, four real roots untouched.** The phase-8 bracket was run in both halves around the serial suite, which is the aggregate isolation measurement phase 8's own verifier was forbidden to perform.
- **The phase-11 ledger no longer misleads.** Its pinned-toolchain hypothesis is replaced by the real cause.

## CI-Parity Gate Record (SHIP-01 local evidence)

Run from repo root on macOS (darwin 27.0.0), branch `gsd/phase-12-validation-and-v2-2-0-release`, in `ci.yml:15-43` order. Each exit code observed directly.

| # | Gate | Exit | Evidence |
|---|------|------|----------|
| 1 | `cargo fmt --check` | **0** | no diff |
| 2 | `cargo clippy --all-targets -- -D warnings` | **0** | `Finished dev profile`; output contains `Checking baude v2.1.5` |
| 3 | `bash scripts/assert-real-roots-untouched.sh --self-test` | **0** | `40 checks, 0 failures` |
| 4 | `bash scripts/assert-real-roots-untouched.sh before` | **0** | fingerprinted 4 roots |
| 5 | `cargo test -- --test-threads=1` | **0** | 645 tests, 0 failures, ~4.4 min serial |
| 6 | `bash scripts/assert-real-roots-untouched.sh after` | **0** | `PASS: the run left all four real roots untouched.` |
| 7 | `cargo build --workspace --release --locked` | **0** | `Finished release profile` in 49.77s; `Cargo.lock` unchanged |

Supplementary (not a CI gate, run to satisfy the orchestrator's `--workspace` phrasing of the same check): `cargo clippy --workspace --all-targets -- -D warnings` → **exit 0**. It is equivalent here — gate 2's output shows all four workspace members (`vt100`, `baude-core`, `baude`, `bauded`) being checked — and gate 2 retains `ci.yml:18`'s exact form.

**The four real roots the bracket fingerprinted on this host:**

1. `/Users/joese/.config/baude` (config and state root)
2. `/Users/joese/.poindexter/claude` (Claude config root)
3. `/Users/joese/.local/share/baude/worktrees` (managed worktrees root)
4. `/Users/joese/Code` (clone destination root)

All four reported `unchanged (present, ...)` in the `after` check.

**Per-target test counts** (gate 5, in output order):

| Target | Passed | Wall |
|--------|--------|------|
| `baude` unittests (`src/main.rs`) | 158 | 101.09s |
| `baude_core` unittests (`src/lib.rs`) | 361 | 66.44s |
| `bauded` unittests (`src/main.rs`) | 93 | 87.93s |
| `vt100` unittests (`src/lib.rs`) | 0 | 0.00s |
| `vt100 tests/hyperlink.rs` | 2 | 0.00s |
| `vt100 tests/kitty_keyboard.rs` | 14 | 0.00s |
| `vt100 tests/link_fidelity.rs` | 16 | 0.88s |
| Doc-tests `baude_core` | 0 | 0.00s |
| Doc-tests `vt100` | 1 | 1.31s |
| **Total** | **645** | **~4.4 min** |

Summing the raw `test result:` lines yields 646. The extra is the inner `1 passed; 157 filtered out` line — the isolated real-Git dogfood test re-execing the `baude` harness with a filter. It is a subprocess of the 158, not a tenth target, exactly as `12-RESEARCH.md` §CI Parity documented. The true total of 645 matches research to the test.

## Task Commits

1. **Task 1: Make the clippy gate green end to end** — `4e3f462` (fix)
2. **Task 2: Run the full CI-parity bracket plus the locked release build** — no commit (gate task, no files modified by design)
3. **Task 3: Correct the phase-11 deferred ledger's pinned-toolchain claim** — `8ce0e26` (docs)

**Plan metadata:** see the `docs(12-01)` commit following this SUMMARY.

## Files Created/Modified

- `vendor/vt100/src/lib.rs` — replaced the three group-level `#![warn(clippy::{cargo,pedantic,nursery})]` lines with a five-line `FORK (baude)` comment plus four group `allow`s. The two narrow `warn` lines (`as_conversions`, `get_unwrap`) and all seven pre-existing `allow` lines are untouched. No change to `vendor/vt100/Cargo.toml` and no change to the workspace members list.
- `baude/src/links.rs` — anchor-span loop flattened per `clippy::manual_flatten`: `for (r, c) in cells[i..end].iter().flatten()`. The off-screen-scheme comment above it is retained.
- `baude/src/ui.rs` — link-hints row count is now `links.len().clamp(1, 10)` per `clippy::manual_clamp`. Both bounds are literals, so the clamp cannot panic.
- `.planning/phases/11-negotiated-multiline-input/deferred-items.md` — the clippy entry's cause corrected; entry format and structure preserved (still one entry under one heading).

## Decisions Made

- **Group allows over a named-lint list in the fork.** Research enumerated the exhaustive ten lints across Classes A and B, but a named list drifts every time the runner's clippy advances, and this fork is meant to be frozen against upstream. The group form is lower-maintenance and is confined to a single file whose divergence is already tracked in `vendor/vt100/README.md`.
- **Fixed, not allow-listed, in baude's own code.** The two real lints are in baude's source, where the project does gate at `-D warnings`. Allowing them there would have been the prohibited route to a green gate.
- **Ran gates individually rather than as a single `&&` chain.** The plan's `<verify>` gives the chain form; running each gate separately with its exit code captured into a result file is semantically identical (execution still halts on the first failure) but yields the per-gate record the plan's own acceptance criteria and SUMMARY requirements ask for. The `after` roots check was deliberately structured to run regardless of the suite's outcome, mirroring CI's `if: always()`.

## Deviations from Plan

None — plan executed exactly as written. No deviation rules were triggered; the remediation was closed-form exactly as research proved, and no third lint was hiding behind the two.

## Issues Encountered

None. The precondition held (all three source files clean, confirming research's patch-and-revert left no residue), and `target/` staleness was handled by the edits themselves forcing a cold recompile — gate 2's first run took 31.71s and printed the full `Checking` sequence, so no cached green was trusted.

## Estimate vs Actuals

The plan estimated 55,000 tokens; the realized diff measures 1,310 (chars/4). The gap is structural, not an estimation error worth correcting toward: this plan's cost was dominated by *reading* (research sections, three source sites, the CI workflow, the roots script contract) and by *waiting on gates* (~8 min of cold compile, serial suite, and release build), while its write surface was 13 inserted and 9 deleted lines across four files. A diff-volume instrument cannot see that shape of work. Future gate-ladder plans should be estimated on gates run and files read, not lines changed.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- **The branch is safe to push.** Every gate the three required status contexts (`check (macos-14)`, `check (ubuntu-22.04)`, and the compile half of `docker`) run is green on this machine from a clean tree. With `enforce_admins: true` on `main`, this was the round trip worth avoiding.
- **Ready for 12-02** (documentation audit and gap-fill) and the remaining phase plans. Push is explicitly 12-04's job and was not performed here.
- **Carried forward for 12-04:** this gate record is the local half of SHIP-01 and is cited by the phase-8 re-stamp checklist item — a green `before`/`after` bracket around a full suite run is exactly the measurement phase 8's verifier was forbidden to perform, and it is now on record.
- **One note for future re-vendoring:** the `FORK (baude)` block means a future `vendor/vt100` update must re-review the fork's lints rather than trust the allow. The marker is grep-able for precisely that reason.

## Self-Check: PASSED

All four modified files verified present on disk. Both task commits (`4e3f462`, `8ce0e26`) verified present in `git log`. No stubs, skipped tests, or unrun `<verify>` commands — every gate in this plan was executed and its exit code observed directly.

---
*Phase: 12-validation-and-v2-2-0-release*
*Completed: 2026-09-16*
