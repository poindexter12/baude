---
phase: 07-local-tui-dogfood-release
plan: 06
subsystem: release-readiness
tags: [rust, cargo, documentation, dogfood, prerelease, local-install]

requires:
  - phase: 07-local-tui-dogfood-release
    plan: 05
    provides: Exact beta package metadata and non-publishing artifact readiness
provides:
  - Readiness-only v2.0.0-beta README and changelog guidance
  - Fully isolated local TUI morning dogfood runbook and evidence boundary
  - Observed local format, lint, test, package, archive, version, and install evidence
affects: [morning-certification, phase-verification, nyquist-review, release-decision]

tech-stack:
  added: []
  patterns:
    - Temporary HOME/config/data/state/install roots for manual local certification
    - Evidence-only validation with human and platform gates explicitly pending

key-files:
  created:
    - docs/local-tui-dogfood.md
    - .planning/phases/07-local-tui-dogfood-release/07-06-SUMMARY.md
  modified:
    - README.md
    - CHANGELOG.md
    - .planning/phases/07-local-tui-dogfood-release/07-VALIDATION.md

key-decisions:
  - "The beta is described only as a local source-readiness target; stable remote install guidance remains at v0.14.0."
  - "Morning UAT evidence is created only from observed commands, screenshots, and certification outcomes; implementation creates no placeholder evidence file."

patterns-established:
  - "Dogfood isolation: invoke the absolute binary under a temporary install root with isolated HOME, XDG roots, repository, and harmless backend."
  - "Certification boundary: a green local implementation gate does not approve UI, Linux/runtime behavior, requirements, phases, Nyquist, or publication."

requirements-completed: []

duration: 12min
completed: 2026-08-31
---

# Phase 7 Plan 6: Local Beta Dogfood and Final Readiness Gate Summary

**Isolated `v2.0.0-beta` source-install and TUI dogfood guidance backed by a green 337-test local package/archive/install gate, with every morning certification and publication decision still pending.**

## Performance

- **Duration:** 12 min
- **Started:** 2026-08-31T10:05:48Z
- **Completed:** 2026-08-31T10:18:11Z
- **Tasks:** 2
- **Files modified:** 4 implementation/validation files plus this summary

## Accomplishments

- Reworked README hierarchy, lifecycle keys, archive behavior, worktrees, remote compatibility, and source installation to match the durable local TUI without implying a remote beta asset exists.
- Added a readiness-only `2.0.0-beta` changelog section with no date, compare link, or remote-availability claim.
- Added a command-first dogfood runbook that isolates HOME, XDG roots, Git origin/main repository, fake backend, and install output while covering open, create-or-activate, close, restart, explicit reselection, reopen, wide/narrow screenshots, safe removal, key/order evidence, and branch survival.
- Passed the complete local format, Clippy, workspace test, package, locked release build, archive/extract/version, and isolated source-install gate.
- Updated validation with exact observed local evidence while leaving the UAT evidence file absent and all morning certification/approval/completion decisions pending.

## Task Commits

Each task was committed atomically:

1. **Task 1: Document hierarchy and isolated morning dogfood without release claims** — `81fbf88` (`docs`)
2. **Task 2: Run final-gate sampling and record observed evidence only** — `872f524` (`docs`)

## Files Created/Modified

- `README.md` — Durable parent/child behavior, lifecycle keys, retained close versus safe removal, flat remote compatibility, and isolated beta source install.
- `CHANGELOG.md` — Readiness-only `2.0.0-beta` feature and safety section above the unchanged 0.14.0 history.
- `docs/local-tui-dogfood.md` — Isolated morning runbook, wide/narrow visual checklist, durable identity/order evidence, inventory/branch checks, and cleanup.
- `.planning/phases/07-local-tui-dogfood-release/07-VALIDATION.md` — Exact local gate results and explicit morning pending matrix.
- `.planning/phases/07-local-tui-dogfood-release/07-06-SUMMARY.md` — Plan execution and evidence boundary record.

## Decisions Made

- Kept existing stable installation guidance explicit at v0.14.0 and described `v2.0.0-beta` only as an explicitly checked-out local source state.
- Used an isolated Cargo install root and absolute binary path so dogfood cannot overwrite or accidentally invoke the user's installed baude.
- Kept `07-UAT-EVIDENCE.md` absent. The morning operator creates it only after commands, screenshots, or certification outcomes have actually been observed.
- Did not mark REL-03, other pending requirements, Phase 6, or Phase 7 complete; local implementation evidence is necessary but not sufficient for certification.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Restored certification-aware milestone progress after state-handler miscalculation**
- **Found during:** Plan metadata finalization
- **Issue:** `state.advance-plan` could not parse the intentionally certification-focused Current Position, while `state.update-progress` counted all 16 plan summaries but reset completed phases and milestone percentage to zero.
- **Fix:** Preserved Phase 6 as the certification focus and Phase 7 as locally implemented but incomplete while restoring one completed phase, 33% milestone progress, 16 completed plans, 301 execution minutes, and Phase 7 at 6/6 locally implemented.
- **Files modified:** `.planning/STATE.md`
- **Verification:** State and roadmap retain both pending phases while reporting exact on-disk plan/summary counts.
- **Committed in:** Plan metadata commit

---

**Total deviations:** 1 auto-fixed bug
**Impact on plan:** Metadata accuracy only; local readiness evidence and every morning certification/publication boundary remain unchanged.

## Issues Encountered

- Workspace negative-path tests printed expected fixture-local Git diagnostics for deliberately invalid removal targets. All containing assertions passed; no product or test failure occurred.
- The isolated Cargo install printed informational notices about the environment-selected Rust 1.98.0 toolchain and temporary bin directory not being on PATH. Installation and exact version verification passed.

## Verification

- Documentation assertions passed for exact beta text, uppercase `X`, branch retention, `40x12`, first-parent restart initialization, non-persisted selection, and the exact morning evidence destination.
- The forbidden distribution-command scan passed, and the new changelog section contains no release/publication status claim.
- `cargo fmt --all -- --check` passed.
- `cargo clippy --workspace --all-targets -- -D warnings` passed without a Clippy warning.
- `cargo test --workspace` passed: 45 `baude`, 213 `baude-core`, and 79 `bauded` tests, 337 total, plus 0 doc tests.
- `cargo package -p baude-core --locked` packaged 19 files and passed pristine verification.
- `cargo package --workspace --locked --no-verify` assembled all three `2.0.0-beta` crates.
- `cargo build --workspace --release --locked` passed; both host binaries reported exact `2.0.0-beta` versions.
- A temporary host archive contained exactly `baude` and `bauded`; extraction and both version assertions passed.
- `cargo install --path baude --root <temporary>/install --locked` passed; the isolated binary reported `baude 2.0.0-beta` and the temporary root was removed.
- `git diff --check` and tracked-membership checks passed. Only pre-existing unrelated untracked `graphify-out/` and `opencode.json` remained.

## Certification Boundary

- No manual TUI run, screenshot, Linux/runtime certification, supported remote CI result, independent review, phase verification, Nyquist approval, UI-SPEC checker approval, requirement/phase completion, or publication decision was represented as observed.
- `.planning/phases/07-local-tui-dogfood-release/07-UAT-EVIDENCE.md` does not exist.
- No push, PR, tag, release, registry login, publication, or dependency change occurred.

## Known Stubs

None found in files created or modified by this plan.

## User Setup Required

None during implementation. Morning dogfood follows `docs/local-tui-dogfood.md` and records only observed evidence.

## Next Phase Readiness

- Local implementation and packaging evidence is green and ready for morning manual dogfood and certification.
- Still pending: wide/narrow screenshots, supported CI/Linux/runtime checks including Phase 6 process certification, independent deep review, phase verification, Nyquist approval, UI-SPEC checker sign-off, requirement/phase completion, audit/cleanup, and an explicit publication decision.

## Self-Check: PASSED

- README, changelog, runbook, validation, and this summary exist.
- Task commits `81fbf88` and `872f524` are present in git history.
- `07-UAT-EVIDENCE.md` remains absent as required without observed morning evidence.

---
*Phase: 07-local-tui-dogfood-release*
*Completed: 2026-08-31*
