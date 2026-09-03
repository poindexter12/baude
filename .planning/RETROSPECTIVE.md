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

## Cross-Milestone Trends

| Milestone | Phases | Plans | Shipped |
|-----------|--------|-------|---------|
| v0.7 Session Visibility | 4 | 14 | 2026-07-02 |
| v2.0 Local TUI Dogfood Release | 3 | 16 | 2026-09-03 |
