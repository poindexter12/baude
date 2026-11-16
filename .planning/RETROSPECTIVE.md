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

## Milestone: v2.3 — Launch Defaults and Startup Speed

**Shipped:** 2026-09-22 (v2.3.0) | **Records closed:** 2026-10-13
**Phases:** 5 (13-17) | **Plans:** 12 | **Tasks:** 21

### What Was Built
- Workspace derivation from the launch folder: ancestor walk for recorded bindings, else the sanitized repo-root name, with explicit env and config still winning; one shared `launch::start_workspace` resolver drives both TUI and daemon.
- New-session and open default to the launch repository's git root; `new_session_dir` applies only outside a repository; TUI title and the daemon's `/info` `workspace_source` show the workspace with its source, or exactly `(blank)`.
- Stable managed-worktree identity: a SHA256 digest of the canonical common directory, claimed through an atomic marker file, plus `collision_line()` reporting shared by TUI, daemon, and scan.
- Startup made measurable (`BAUDE_TIMING=1`, nine stages), the kitty keyboard probe bounded at 250 ms, and session restore moved off the first-frame path.
- Idle cost driven to zero: dirty-flag redraw, live-row-only metadata polling with mtime gating, opt-in SIGSTOP/SIGCONT child suspension, configurable usage poller, and static status codes replacing the timer-driven spinner.

### What Worked
- One shared startup seam instead of two hand-rolled paths. The TUI and daemon had already diverged — the daemon never claimed a workspace lock — and a single `launch::start_workspace` made parity testable rather than aspirational.
- Deriving identity from the repository itself rather than from a counter. Per-state-file counters restart at 1, so the collision was structural; a digest of the common directory survives a state reset, which no amount of careful counter bookkeeping would have.
- Treating the spinner as a performance decision, not a cosmetic one. Recognizing that timer-driven animation forces a repaint every tick is what made zero idle CPU reachable at all.
- Building the measurement before certifying the claim. The milestone's headline is that idle baude stops burning battery, and until SQ-11 that rested on a unit test proving zero *draws* — a proxy. The real-PTY `idle-cpu.sh` sample is what turned it into evidence.

### What Was Inefficient
- The milestone shipped on 2026-09-22 and the records closed on 2026-10-13, three weeks and three releases later. By then every phase read `stale` and the close had to be an override — the second milestone in a row to close that way, for a different reason than v2.2's.
- Phase 13 and Phase 14 never ran `validate-phase`, so their `VALIDATION.md` files are still the seeded template with `{pytest 7.x / jest 29.x}` placeholders. Nobody noticed for three weeks because nothing enforces it.
- Phase 13's plans went through four Codex convergence cycles before executing, and the 13-02 TDD gate was accepted as debt with no `feat(13-02)` GREEN commit.
- The whole milestone landed as one squash commit (#90) of 196 files and +27,188 lines. Nothing in the git history distinguishes Phase 13 from Phase 17.
- An executor timed out the full suite at 600 s and reported it as a hang. It was never hung — but the closeout's first correction was wrong too. Recording "the suite takes about 659 s" generalized a single wall clock taken on a heavily loaded dev host with a cold compile; CI ran the same suite serially, on three cores, in 59 s the same day. The timeout was tuned to a guess, and the number brought in to replace the guess was read off the worst host available.

### Patterns Established
- Resolve launch-time state through one shared helper with a fixed write order (canonicalize, look up binding, derive, seed cache, claim lock, record binding), so TUI/daemon parity is a test rather than a convention.
- Bind derived workspaces to the repository root, never the launch subfolder; unbound non-git folders record nothing, because they have no stable identity to bind.
- Refuse startup outright when the workspace lock is held (`StartError::LockHeld`) and record no binding — preserving the single-writer invariant beats degrading gracefully into two writers.
- Host-dependent performance claims get a documented Manual-Only entry plus a reproducible script that states its own limits, rather than a CI assertion that encodes one machine's scheduler.

### Key Lessons
- Close the milestone records when the milestone ships. Every day of delay converts passing verification into stale verification, and the only honest remedies are an override or a re-verification that checks old goals against new code.
- A green unit test that proves a proxy is not evidence for the claim on the box. "Zero draws while idle" and "costs no CPU while idle" are different statements; only one of them was what the milestone promised.
- A wall clock from one host is not the cost of the thing measured. This closeout replaced a guessed 600 s timeout with an observed 659 s, then watched CI run the same suite serially in 59 s — an order of magnitude apart, same commit, same day. Timeouts and "it hung" reports both need a figure from the host that will do the work, and a measurement taken under unknown load is a guess wearing a number.
- A test that re-reads its own input and asserts on it passes forever and guards nothing. `timing_disabled_when_env_unset` survived a 9/9 verification score by being a tautology.
- When a phase template ships with `{placeholder}` braces intact, no gate catches it. Three weeks of planning artifacts said `draft` and nothing escalated.

### Cost Observations
- Model mix: balanced profile; opus-class planning and review, sonnet- and codex-class execution. Not precisely instrumented.
- Sessions: roughly 8 GSD sessions 2026-09-19 to 2026-09-22, plus the 2026-10-13 closeout session.
- Notable: closeout cost a full 11-minute suite run plus three sidequest reconciliation refusals, each with a distinct real cause (a prunable worktree registration left by the phase's own measurement script, a verify running in a checkout parked on a stale branch, and a squash-merge making the pinned candidate unreachable).

## Milestone: v2.8 — Project Removal

**Shipped:** 2026-11-15 (v2.8.0), cleanup 2026-11-16 (v2.8.1) | **Records closed:** 2026-11-16
**Phases:** 6 (18-22, 22.1 inserted) | **Plans:** 49 | **Tasks:** 57

### What Was Built
- One `baude-core::removal` pipeline for every row kind: preview, kill the owned process group, delete managed worktrees, one atomic save, crash-consistent at every stage. Later split into `prepare`, `stop_and_delete` and `commit` so the daemon can run the slow stage unlocked.
- Re-admission suppression: removal clears folder bindings and ownership markers and writes a suppression entry; only `n`, `c` or launching inside the folder brings a project back, through one `lift_key` shared by TUI and daemon.
- TUI `D` with a confirmation naming the rows, the runtime to kill and the folders deleted versus kept.
- Daemon `GET /rows`, removal preview and execute, `rows_removed` events, 503 Busy while a removal holds a repository, folder or session; PWA removal whose confirmation says when a folder will be deleted.
- Test hygiene: `FixtureRoot`, `RuntimeReaper`, and a leftover check wrapping the suite in the gate and CI.

### What Worked
- Closing the records the day the cleanup shipped. v2.3 waited three weeks and every phase went stale from main moving on; here the only staleness came from the milestone's own cleanup phase, and the two phases that mattered were re-verified against the final tree before the close.
- Inserting a cleanup phase after the audit instead of backlogging the debt. Suppression sync, test leftovers, PWA wording and the whole-removal mutex were each small then and would each have been harder later.
- A sandbox daemon with a fake agent for the owner's browser checks. Two PWA sign-offs took minutes each and touched nothing real.
- Code review after execution caught three real defects in 22.1 (panic in locked stages, reaper signalling a recycled pid, a busy reconcile dropped too early) before merge.

### What Was Inefficient
- Phase 21 needed three replan cycles before execution, and a convergence stall had to be resumed from orchestrator state across sessions.
- The suite runs serially (#58), so every gate and CI run pays it in full: 464 s on `macos-15` against 176 s on Ubuntu, and about 13 minutes on the dev host. A docs-only PR paid the same 9.5 minutes.
- Load-sensitive tests: four `bauded` tests failed at a load average near 38 and passed in isolation, costing a full gate re-run on a docs-only branch.
- GSD bookkeeping drift: roadmap plan counts read `0/?` for three completed phases, `phase.complete` set state back to "ready to plan" a finished phase, and two verifications had to be regenerated purely for digest freshness.

### Patterns Established
- Hold a shared lock only to reserve and to commit; run slow, irreversible work unlocked against a reservation, and answer 503 Busy to anything that would touch the reserved set.
- A shared function, not a convention, for any key two clients must agree on (`suppression::lift_key`).
- Tests that start runtimes reap them by identity, process start time included, and the gate fails on anything left behind.
- Owner browser checks run against a sandbox daemon with its own HOME and a fake agent, torn down afterwards.

### Key Lessons
- A cleanup phase inserted after release is cheaper than the same items aged in the backlog, as long as it is scoped by the audit and not reopened.
- When a test compares whole snapshots of live state, fields that move on their own, like `waiting_reason`, make it a load test. Compare what the test is about.
- Re-verify the phases the final code actually changed; for the rest, an honest override with the reason written down beats re-checking unchanged goals.

### Cost Observations
- Model mix: opus-class orchestration, planning and review; executors and verifiers as GSD subagents. Not precisely instrumented.
- Sessions: many, 2026-11-01 to 2026-11-16, including a multi-session Phase 21 replan.
- Notable: the serial test suite is now the dominant wall-clock cost of every change, including docs-only ones.

## Cross-Milestone Trends

| Milestone | Phases | Plans | Shipped |
|-----------|--------|-------|---------|
| v0.7 Session Visibility | 4 | 14 | 2026-07-02 |
| v2.0 Local TUI Dogfood Release | 3 | 16 | 2026-09-03 |
| v2.2 Reliability and Terminal Usability | 5 | 25 | 2026-09-19 |
| v2.3 Launch Defaults and Startup Speed | 5 | 12 | 2026-09-22 |
| v2.8 Project Removal | 6 | 49 | 2026-11-15 |

### Recurring

- **Closing by override, three times running.** v2.2 closed without a Phase 12 `VERIFICATION.md`; v2.3 closed with all five verifications stale; v2.8 closed with four stale. v2.8 is the mildest: it closed the day its cleanup shipped, the staleness came only from its own cleanup phase, and the final-tree phases were re-verified first.
- **Verification artifacts treated as paperwork after shipping.** v2.2's lesson was "run `/gsd-verify-work` per phase before moving on." v2.3 ran it on every phase and then let the records go stale instead. The gap moved rather than closed.
- **Lint and test gates that pass without testing anything.** v2.2: a vendored crate failing to compile under `-D warnings` silently exempted everything downstream. v2.3: a tautological test counted toward a 9/9 score. Both passed cleanly for weeks.
- **Serial test suite as the pacing cost.** Serialized for #58 interference, the suite now dominates every gate and CI run; v2.8 also saw it flake under host load. Sharding across runners and replacing fixed sleeps are the open levers.
