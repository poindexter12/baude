# Retrospective: baude

## Milestone: v2.0 — Local TUI Dogfood Release

**Shipped:** 2026-09-03
**Phases:** 3 (5-7) | **Plans:** 16

### What Was Built
Checkout-first durable repository hierarchy with managed worktree lifecycle
(create/activate, retained close, reopen, verified safe removal), one shared
baude-core lifecycle engine behind mirrored App/Manager adapters, schema-v3
state with standalone non-git sessions, and the v2.0.0-beta release channel
(manual bootstrap → release-please beta.1).

### What Worked
- Goal-backward verification + owner adjudication converted a paused,
  wip-flagged branch into defensible certification in two days.
- Scripted-PTY runbook execution produced repeatable evidence (ANSI captures,
  state snapshots) without manual dogfood sessions.
- Test-as-contract discipline caught real product gaps (seed-blocked removal,
  occupied-protected guards) rather than papering over them.

### What Was Inefficient
- Environment-sensitive tests masked failures for days: backend resolution
  from the host config, global gitignore hiding untracked seeds, git
  2.34-vs-2.50 worktree-remove behavior, and the appended permission flag
  silently breaking bare `sleep 30` fixtures (#58) — four distinct
  environment couplings, each found only when a new environment ran the suite.
- CI never ran pre-merge because the draft PR sat CONFLICTING (no merge ref)
  — Linux went uncompiled until publication week.
- The v2.0.0-beta tag was re-cut five times during dogfood; tolerable for a
  single-consumer bootstrap, not a pattern to repeat post-beta.1.

### Patterns Established
- Fake agent fixtures MUST be `sh -c '...'` so appended backend flags park in $0.
- Seed-exempt removal: baude-owned files verified pure before preflight
  exemption and deleted inside verified removal; git refusal is backstop only.
- Milestone certification chain: verification → adjudication → Nyquist →
  UI audit → CI matrix evidence, each writing durable phase artifacts.

### Key Lessons
- A green suite proves the environment as much as the code; run gates in a
  clean env (isolated HOME/config) before calling anything certified.
- Flaky-looking CI failures deserve instrumented reproduction (10x diagnostic
  runs found the real bug after three wrong theories).
- Publishing decisions beat readiness docs: record overrides explicitly
  instead of letting stale "no publish" language rot.

### Cost Observations
- Sessions: primarily one long interactive session over 2026-09-01..03.
- Notable: background CI watchers + subagent verifiers kept the main context
  lean; diagnostics-by-PR was cheaper than local guesswork.

## Milestone: v2.2 — Reliability and Terminal Usability

**Shipped:** 2026-09-19 (v2.2.0)
**Phases:** 5 (8-12) | **Plans:** 25 | **Tasks:** 49

### What Was Built
- Fixture-owned test isolation (`TestRedirect` plus escape guard) across all three crates, and a fail-closed managed-worktree leak scan that is preview-only by default (#72).
- Guarded hook and MCP seeding that never overwrites unparseable settings and seeds a shell-safe quoted command (#70); WLOCK diagnostics pinned by tests (#71).
- Clickable OSC8 and bare-URL links via a vendored vt100 fork, `ctrl+o` hint overlay, preview and copy, a validated argv-only opener, and remote-attach parity.
- Negotiated Shift+Enter newlines over the kitty keyboard protocol with a legacy byte-freeze fallback.
- Release validation: clippy gate repaired, README updated, 174 commits pushed and CI-green, smoke evidence recorded, v2.2.0 published.

### What Worked
- Requirements reconciliation before execution (2026-09-13): re-verifying each requirement against v2.1.5 showed 6 of 12 had already shipped in point releases, so Phases 8 and 9 were narrowed to the real gaps instead of re-implementing them.
- Fail-closed design for destructive tooling: the leak scan's three-way verdict plus saved-report gating made "removes nothing by default" a structural property rather than a flag.
- Per-plan code review with fix commits (Phase 10: 4 findings, 6 commits; Phase 11: 3 warnings, 4 commits) caught real bugs (OSC8 truncation guard, restore_terminal leak) before verification.
- Research and pattern mapping before planning kept the terminal-protocol phases grounded in the vendored parser rather than speculation.

### What Was Inefficient
- `cargo clippy -D warnings` had never actually linted baude's own crates: the vendored vt100 fork's lint header failed compilation first and masked two genuine lints for all of Phase 10. Found only in Phase 12.
- Phases 8 through 11 accumulated 174 unpushed commits on local branches until plan 12-04; the single late rebase was clean, but CI never saw the work until the last phase.
- Phase 8 planning stalled on a monthly API spend limit and on fixture/env isolation conflicts that needed a GSD upgrade (1.13 to 1.14.0) plus a plan revision.
- Phase 12 was executed but never run through `/gsd-verify-work`, and STATE.md frontmatter drifted (still pointed at Phase 09 while Phase 12 executed), so the milestone closed by override.
- One flaky bauded lifecycle test surfaced once (356/357) and could not be named because tmp-dir git fixtures obscured the failing test.

### Patterns Established
- Fixture realism: every test path resolves through one redirect owned by the fixture; an escape guard aborts any test that reaches the real home directory.
- Gesture-time collection for terminal metadata keeps the render loop untouched; parser changes land in the vendored fork inside commented `FORK (baude)` blocks.
- Doubly-verified protocol negotiation (outer terminal probe plus the child's observed push) before emitting enhanced key encodings; legacy bytes frozen in a corpus test.
- One guarded seeding helper (`read_settings_guarded`) shared by every settings and MCP write path, with warnings surfaced on all spawn paths.

### Key Lessons
- Run the exact CI clippy invocation locally at phase start; a workspace member that fails to compile under `-D warnings` silently exempts everything downstream of it.
- Push phase branches as they complete so CI runs continuously instead of once at release.
- Run `/gsd-verify-work` per phase before moving on; a shipped release does not substitute for the verification artifact, and closing by override leaves a permanent gap note.
- When a milestone's requirements predate several point releases, reconcile against the current tag before planning.

### Cost Observations
- Model mix: balanced profile (opus-class planning and review, sonnet-class execution); not instrumented precisely this milestone.
- Sessions: roughly 12 GSD sessions between 2026-09-13 and 2026-09-19.
- Notable: Phase 8 put 8 of the milestone's 25 plans into test-isolation infrastructure, after which Phases 9 through 11 executed cleanly to 645 green tests.

## Cross-Milestone Trends

| Milestone | Phases | Plans | Shipped |
|-----------|--------|-------|---------|
| v0.7 Session Visibility | 4 | 14 | 2026-07-02 |
| v2.0 Local TUI Dogfood Release | 3 | 16 | 2026-09-03 |
| v2.2 Reliability and Terminal Usability | 5 | 25 | 2026-09-19 |
