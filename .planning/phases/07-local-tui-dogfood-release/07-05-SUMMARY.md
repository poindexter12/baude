---
phase: 07-local-tui-dogfood-release
plan: 05
subsystem: release-engineering
tags: [rust, cargo, release-please, github-actions, prerelease, artifacts]

requires:
  - phase: 07-local-tui-dogfood-release
    plan: 04
    provides: Isolated real-Git restart dogfood and flat remote compatibility evidence
provides:
  - Exact 2.0.0-beta Cargo package, path dependency, lockfile, and binary metadata
  - Release-please beta proposal configuration with truthful 0.14.0 published history
  - Four-target two-binary non-publishing artifact readiness CI
affects: [07-06, release-certification, artifact-readiness, release-please]

tech-stack:
  added: []
  patterns:
    - Exact prerelease version alignment across workspace manifests and lock metadata
    - Artifact readiness builds with read-only contents permission and no publication credentials

key-files:
  created:
    - .planning/phases/07-local-tui-dogfood-release/07-05-SUMMARY.md
    - .planning/phases/07-local-tui-dogfood-release/deferred-items.md
  modified:
    - Cargo.toml
    - Cargo.lock
    - baude-core/Cargo.toml
    - baude/Cargo.toml
    - bauded/Cargo.toml
    - release-please-config.json
    - .github/workflows/ci.yml

key-decisions:
  - "Published history remains exactly 0.14.0 while source and proposal metadata target 2.0.0-beta."
  - "Artifact readiness copies only the supported target and two-binary archive shape, with read-only contents permission and no publication authority."

patterns-established:
  - "Readiness boundary: build, package, archive, extract, and execute version assertions locally or in CI without invoking release automation."
  - "Version boundary: the local baude-core path dependency carries an exact registry-compatible prerelease version."

requirements-completed: [REL-02]

duration: 9min
completed: 2026-08-31
---

# Phase 7 Plan 5: Beta Package and Artifact Readiness Summary

**All crates, packaged sources, host binaries, release proposal fields, and four supported CI archive targets now agree on `2.0.0-beta` without changing or invoking publication automation.**

## Performance

- **Duration:** 9 min
- **Started:** 2026-08-31T09:50:28Z
- **Completed:** 2026-08-31T09:59:02Z
- **Tasks:** 3
- **Files modified:** 7 release/config files plus this summary and deferred-items log

## Accomplishments

- Synchronized all three workspace package versions, exact `baude-core` path dependency metadata, and lockfile entries to `2.0.0-beta` without changing the dependency graph.
- Preserved the simple root-package release-please pattern and three TOML updaters while setting exact beta proposal fields.
- Added a read-only, non-publishing CI matrix for the four supported Apple/Linux targets that builds, archives, extracts, and version-checks both binaries.
- Proved local source package assembly, verified `baude-core` packaging, host release archive extraction, and exact runtime binary versions.
- Kept `.release-please-manifest.json` at `0.14.0`; both publication workflows retained their original byte hashes and were neither run nor triggered.

## Task Commits

1. **Task 1: Synchronize exact beta Cargo metadata** — `5b712a4` (`chore`)
2. **Task 2: Prescribe exact release-please beta proposal behavior** — `30ca7f6` (`chore`)
3. **Task 3: Add supported-target non-publishing artifact readiness CI** — `9c9b694` (`ci`)

## Files Created/Modified

- `Cargo.toml` — Exact `=2.0.0-beta` version paired with the local `baude-core` path.
- `baude-core/Cargo.toml` — Core package version set to `2.0.0-beta`.
- `baude/Cargo.toml` — TUI package version set to `2.0.0-beta`.
- `bauded/Cargo.toml` — Daemon package version set to `2.0.0-beta`.
- `Cargo.lock` — Only the three workspace package versions changed.
- `release-please-config.json` — Exact release-as, prerelease versioning, prerelease flag, and beta type.
- `.github/workflows/ci.yml` — Four-target locked release build and two-binary archive verification matrix.
- `.planning/phases/07-local-tui-dogfood-release/deferred-items.md` — Records pre-existing actionlint shellcheck findings in the untouched Docker smoke script.

## Decisions Made

- Kept `.release-please-manifest.json` as last-published history rather than advancing it to source readiness.
- Retained `release-type: simple` and the current generic TOML updater pattern instead of introducing the Rust strategy or a workspace plugin.
- Gave the readiness job only `contents: read`; it receives no release, registry, PR, tag, or push authority.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Restored certification-aware milestone progress after state-handler miscalculation**
- **Found during:** Plan metadata finalization
- **Issue:** `state.advance-plan` could not parse the intentionally certification-focused Current Position, and `state.update-progress` reset completed phases and displayed progress to zero while counting 15 completed plans.
- **Fix:** Preserved Phase 6 as the certification focus while restoring one completed phase, 33% milestone progress, 15 completed plans, 289 execution minutes, and Phase 7 at 5/6.
- **Files modified:** `.planning/STATE.md`
- **Verification:** State retains pending certification status while reporting the exact on-disk plan and prior completed-phase counts.
- **Committed in:** Plan metadata commit

---

**Total deviations:** 1 auto-fixed bug
**Impact on plan:** Metadata accuracy only; package, artifact, publication-boundary, and certification status are unchanged.

## Issues Encountered

- Full `actionlint` surfaced pre-existing SC2015 and SC2034 shellcheck findings in the unchanged Docker smoke script. The issue was logged to `deferred-items.md`; `actionlint -shellcheck=` confirmed workflow structure and expressions without changing unrelated behavior.

## Verification

- Exact locked Cargo metadata assertion passed for `baude`, `baude-core`, and `bauded` at `2.0.0-beta`.
- Exact versioned local dependency assertion passed.
- Exact release-please root package, preserved fields, beta fields, TOML updater list, and `0.14.0` manifest assertion passed.
- Exact four-target CI, locked workspace build, archive/extract, dual runtime version, and forbidden-publication-token assertion passed.
- `actionlint -shellcheck= .github/workflows/ci.yml` passed.
- `cargo fmt --all -- --check` passed.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo test --workspace` passed: 45 `baude`, 213 `baude-core`, and 79 `bauded` tests (337 total).
- `cargo package -p baude-core --locked` passed with pristine verification.
- `cargo package --workspace --locked --no-verify` assembled all three `2.0.0-beta` crates.
- `cargo build --workspace --release --locked` passed.
- A host tar archive containing `baude` and `bauded` was created and extracted; both extracted binaries reported exact `2.0.0-beta` versions.
- Publication guard hashes remained exact: manifest `f97276d606ef961e699fb7f20db6c27c281de15b`, release-please workflow `b6a1889adb02ec264ffe34b98852e02a3eb7b8ac`, release workflow `18ef59b57acf2513fd47f6571aa83733fe097d04`.
- `git diff --check` passed.

## Publication Boundary

- No publish, push, tag, PR, release, registry login, release-please invocation, or publication workflow execution occurred.
- No dependency was added and the locked external dependency graph was unchanged.
- Existing unrelated untracked `graphify-out/` and `opencode.json` were not modified or staged.

## User Setup Required

None - no external service configuration required.

## Known Stubs

None found in files created or modified by this plan.

## Next Phase Readiness

- Plan 07-06 can document readiness and sample the final implementation gate using exact beta package/artifact evidence.
- Supported remote CI execution, manual wide/narrow dogfood, Linux/runtime certification, independent review, phase verification, Nyquist approval, and publication decision remain pending. This plan establishes local readiness only.

## Self-Check: PASSED

- All seven modified release/config files, this summary, and the deferred-items log exist.
- Task commits `5b712a4`, `30ca7f6`, and `9c9b694` are present in git history.

---
*Phase: 07-local-tui-dogfood-release*
*Completed: 2026-08-31*
