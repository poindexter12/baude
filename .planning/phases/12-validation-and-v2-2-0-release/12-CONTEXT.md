# Phase 12: Validation and v2.2.0 Release - Context

**Gathered:** 2026-09-16
**Status:** Ready for planning

<domain>
## Phase Boundary

Maintainers have observed regression, CI, documentation, and terminal evidence
sufficient to publish v2.2.0 through the existing release process.

In scope: the full automated gate ladder (SHIP-01), the SHIP-02 documentation
audit and gap-fill (including new lock-recovery guidance), the structured
real-terminal smoke-evidence checklist (SHIP-03), and release preparation
through the existing release-please flow with a blocking-human publish gate
(SHIP-04).

Out of scope: new features, packaging rework, and re-verifying phases 9-11
beyond running their suites in the gates.

</domain>

<decisions>
## Implementation Decisions

### Validation Scope & Evidence
- One plan runs the full ladder locally: focused regressions → workspace
  suite → fmt → clippy → CI-parity checks; CI evidence comes from the pushed
  branch's GitHub Actions run.
- Smoke evidence is a structured `12-SMOKE-EVIDENCE.md` checklist covering
  links, selection, scrollback, mouse, Shift+Enter, ordinary Enter, and
  terminal restoration on macOS and Linux; the maintainer fills it during a
  guided dogfood — baude drives what it can, the human observes.
- Linux: automatable legs run in Docker/CI; interactive terminal legs are
  marked as requiring a real Linux terminal session and may be deferred with
  explicit sign-off if unavailable.
- The pre-existing vendored-vt100/metadata clippy lints are resolved now
  (allow-list or fix) so the clippy gate is a clean exit 0.

### Documentation (SHIP-02)
- README feature sections are the home: link activation/preview/copy
  gestures, tested terminal support, Shift+Enter setup/fallback (phases 10-11
  landed pieces — audit and fill gaps), plus a NEW workspace-lock recovery
  section (pid diagnostic meaning, safe recovery steps) — the one SHIP-02
  item no prior phase wrote.
- Every documented item gets a grep-based acceptance criterion.
- Release notes are assembled from phase summaries per the release-please
  convention already in use.

### Release Mechanics (SHIP-04)
- Merge train: phase branches → main via the existing PR flow; release-please
  cuts v2.2.0 (minor = auto after soak per the recorded release policy).
- SHIP-04 executes only after phase verification passes AND the maintainer
  approves — the publish step is a blocking-human checkpoint, never
  auto-published.
- Existing release workflow outputs (4-target tarballs + Docker) are reused
  as-is; the phase verifies they still build with the vendored fork included
  (`cargo build --workspace --release --locked` + the release workflow's own
  checks), no packaging rework.
- Phase 8's stale verification must be re-stamped (`/gsd-verify-work 8`)
  before milestone completion — surfaced as an explicit pre-release checklist
  item, not silently absorbed (the release PR already carries phase-8 work in
  its lineage).

### Claude's Discretion
- Exact smoke-checklist item wording and ordering.
- README section placement and phrasing.
- Whether clippy lints are fixed or precisely allow-listed (narrowest scope
  wins).

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- ci.yml + release-please config (v2.0.0-beta lineage) — the existing release
  process SHIP-04 rides.
- Phase 8's scripts/assert-real-roots-untouched.sh CI bracket.
- Phase 9-11 test suites (641+ tests) and their targeted filters for the
  focused-regression leg.
- README sections already touched by 11-03 (shift+enter row) and earlier.

### Established Patterns
- Verify-gate exit codes captured directly, never piped (recorded policy).
- release-please policy: major=manual, minor=auto after ~2h soak,
  patch=auto on green.
- Deferred-items ledgers per phase for known non-blockers.

### Integration Points
- GitHub Actions CI on the pushed branch; the PR flow to main.
- README.md; release notes via release-please.

</code_context>

<specifics>
## Specific Ideas

- SHIP-03's evidence must be observed, not synthesized: the checklist records
  what the maintainer actually saw, with date and terminal identity.
- SHIP-04's "matching versions" means source Cargo.toml versions, release
  notes, and tag all agree when release-please cuts the release.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>
