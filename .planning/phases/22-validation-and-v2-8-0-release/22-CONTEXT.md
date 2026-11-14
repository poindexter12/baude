# Phase 22: Validation and v2.8.0 Release - Context

**Gathered:** 2026-11-13
**Status:** Ready for planning

<domain>
## Phase Boundary

Prove removal end to end on realistic fixtures for every row kind with and without a live runtime, document the remove action in the README, and take v2.8.0 as far as an open pull request to `main`. Requirements REL-01 and REL-02. No new removal behaviour is added here; fixes are limited to defects the new tests or the gates expose.

</domain>

<decisions>
## Implementation Decisions

### Shipping v2.8.0
- The autonomous run pushes the branch and opens ONE pull request to `main` with a `feat` title, then stops. The owner merges it. Nothing in this phase merges to `main`. — **Reversibility:** one-way — a merge to protected `main` triggers release-please and a published v2.8.0; a pushed branch and an open PR can still be closed.
- After the owner merges, release-please opens the 2.8.0 release PR and the minor bump auto-merges after its soak. Success criterion 3 is therefore only fully met after the owner's merge; the phase records that as the one open item rather than claiming a release.
- Phases 18, 19 and 20 are re-verified with the automated verifier before the push. A human-needed result goes back to the owner as a question. — **Reversibility:** costly — releasing unverified phases cannot be recalled once v2.8.0 is published.
- The intermittent macOS teardown 500 is NOT fixed in this phase. It is recorded as BL-11 in `.planning/BACKLOG.md` and v2.8.0 ships with it known.

### Decided autonomously
- [Tests] Q: 'Where do the REL-01 tests live?' → Selected: 'In-crate test modules, as every existing test does; the repo has no tests directories' — moving a test module is a local edit
- [Tests] Q: 'What matrix?' → Selected: 'Four row kinds, with a live runtime and with none, on cloned and git init fixtures, asserting on process, worktree, binding and marker state read from the fixture' — test-only
- [Tests] Q: 'Browser automation for the PWA?' → Selected: 'None. Add a Rust test that subscribes to /events over real HTTP and sees rows_removed for a real removal' — no new dependency, test-only
- [Docs] Q: 'Where do the docs go?' → Selected: 'README: a new Removing projects section, the Keys table, and the bauded section for the removal endpoints, the SSE event and the PWA modal' — prose edits
- [Release] Q: 'Changelog and version?' → Selected: 'release-please owns both; no hand edits and no Release-As footer unless the computed bump is wrong' — nothing is written by hand
- [Release] Q: 'Gates?' → Selected: 'scripts/gate.sh and scripts/gate.sh --linux on the final commit before any push, exit codes read directly' — process only

### Claude's Discretion
- Exact test names, fixture helpers and how the matrix is split across `baude-core`, `baude` and `bauded`.
- README wording and section order.
- Whether Phase 21 code-review warnings still open at planning time are fixed here or left to the review-fix pass.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `baude-core/src/removal.rs`: `select_rows`, `preview_removal`, `remove`, `RemovalReport`; Phase 18 and 19 tests already build cloned and `git init` fixtures and live process doubles.
- `bauded/src/manager.rs` and `bauded/src/api.rs`: `Manager::remove_rows`, the preview and remove endpoints, 14 `matrix_` tests, SSE `rows_removed` on `GET /events`.
- `bauded/web/removal.js`, `removal-controller.js` with node suites run by `scripts/check-web-suites.sh`, which `scripts/gate.sh` and the CI `check` job call.
- `baude/src/app.rs`: TUI remove action and remote removal tickets.

### Established Patterns
- Fixtures must be shaped like what real users produce: cloned versus `git init` repositories, stale runtime rows, command doubles that accept every appended argument. Assert on fixture output, never by grepping source.
- `scripts/gate.sh` is the CI `check` job; `--linux` runs it in `rust:1-bookworm` as uid 1000 and is required for platform code.
- Conventional commits; never nest parentheses in a commit body, squash message or PR title, or release-please skips the commit.

### Integration Points
- README sections `## Keys` (line 121), `## Worktrees` (355), `## Remote sessions in the TUI` (763), `## bauded (experimental)` (773, endpoint table near 802).
- `release-please-config.json` (release-type simple, extra-files for the four Cargo.toml versions); manifest at 2.7.3.
- Branch `gsd/phase-18-shared-kill-then-remove-lifecycle` carries phases 18 to 22, 214 commits ahead of `origin/main`, unpushed.

</code_context>

<specifics>
## Specific Ideas

- Phase 21 verification was marked passed on owner sign-off without the live checks. The live SSE client drop and PWA removal in a browser were never observed. The `/events` test in this phase is the automated stand-in for the daemon half; it does not replace a browser run, and the docs must not claim one.
- The first push is also the first proof that the CI node step works on a runner.

</specifics>

<deferred>
## Deferred Ideas

- BL-11: intermittent macOS teardown 500.
- Standalone removal leaves the daemon-written `.claude` directory; the PWA lists unclaimed sessions without an 'other sessions' heading; older state files serve `managed_branch` for a main checkout; the PWA shows full paths.
- Browser-level PWA tests.

</deferred>
