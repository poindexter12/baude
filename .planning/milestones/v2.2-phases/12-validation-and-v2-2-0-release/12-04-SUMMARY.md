---
phase: 12-validation-and-v2-2-0-release
plan: 04
subsystem: infra
tags: [git-rebase, github-actions, ci, pull-request, release-please, branch-protection]

# Dependency graph
requires:
  - phase: 12-01
    provides: the green local CI-parity gate ladder and the real-roots bracket result this plan re-ran post-rebase and cited in the phase-8 re-stamp item
  - phase: 12-02
    provides: the SHIP-02 documentation audit whose completion the evidence gate cites
  - phase: 12-03
    provides: 12-SMOKE-EVIDENCE.md, whose bulk-attestation strength and three signed gaps the checklist represents verbatim rather than rounding up
provides:
  - "The phase branch rebased onto origin/main and pushed — the first time phases 8-11 have ever existed on the remote"
  - "PR #87 open against main with a conventional feat: title and a body recording the merge-strategy recommendation and its reason"
  - "The first CI run ever to exercise phases 8-11, green on all three required contexts plus all four artifact-readiness targets"
  - "12-RELEASE-CHECKLIST.md — CI evidence plus a 26-item pre-release checklist covering version agreement, distribution outputs, the evidence gate, and release:hold timing"
  - "A proven attribution for the local real-roots gate failure, and the clean-room measurement that closes it"
affects: [12-05, release-v2.2.0, gsd-complete-milestone]

actuals:
  tokens: 4203
  tasks: 3
  commits: 4
  plan_head_before: e07b28e897609b3a30fcdca45d13e5e237dfc7c9

tech-stack:
  added: []
  patterns:
    - "Squash-aware rebase: when origin/<default> is a squash of the branch's own ancestor commits, rebase with `git rebase --onto <default> <squash-point>` and prove tree equivalence with an empty `git diff <old-head> HEAD`, rather than conflicting through already-applied content"
    - "Null-bracket attribution: when a host-global filesystem assertion fails, re-run the before/after pair with no workload between the snapshots — a failure there proves the delta belongs to a concurrent actor, not to the thing under test"
    - "Evidence scoped to a sha: a CI-evidence artifact names the sha its measurement judged and proves later commits left the source tree identical, so docs-only pushes cannot silently invalidate it"

key-files:
  created:
    - .planning/phases/12-validation-and-v2-2-0-release/12-RELEASE-CHECKLIST.md
  modified: []

key-decisions:
  - "Rebased with `git rebase --onto origin/main 985f543` instead of a plain `git rebase origin/main`, because origin/main's 30ce007 is a squash of this chain's own first seven planning commits and a plain rebase conflicted on already-applied content at commit 1 of 181"
  - "Pushed with the local real-roots gate red, after proving by null bracket that the delta came from two identified concurrent host processes rather than from the test suite — and recorded it as an unmeasurable gate, never as a green one"
  - "Pre-ticked exactly one checklist box (the phase-8 re-stamp), because it demonstrably already happened on 2026-09-16 and leaving it open would misrepresent the project state"
  - "Added a release:hold timing item the plan did not ask for, since the human publish gate is opt-in and a */30 cron can otherwise publish v2.2.0 on a timer with nobody authorizing it"

patterns-established:
  - "A release checklist represents upstream evidence at its stated strength: the smoke-evidence item spells out the bulk attestation, the three signed gaps, and the full Linux deferral, so signing the box cannot be mistaken for twelve behaviors observed on two platforms"

requirements-completed: [SHIP-01, SHIP-04]

coverage:
  - id: D1
    description: "The phase branch is rebased onto origin/main and pushed, carrying phases 8-11 as one PR rather than four"
    requirement: "SHIP-01"
    verification:
      - kind: other
        ref: "git merge-base --is-ancestor origin/main HEAD (exit 0)"
        status: pass
      - kind: other
        ref: "git rev-list --count '@{upstream}..HEAD' == 0 (branch pushed with upstream tracking)"
        status: pass
      - kind: other
        ref: "git merge-base --is-ancestor <tip> HEAD for all four phase tips plus gsd/v2.2-reliability-terminal-usability (all exit 0, pre-rebase)"
        status: pass
      - kind: other
        ref: "git diff --stat fd22555 HEAD (empty — rebase preserved the tree byte-for-byte)"
        status: pass
    human_judgment: false
  - id: D2
    description: "An open PR to main exists with a conventional feat: title and a body recording the intended merge strategy and its reason"
    requirement: "SHIP-01"
    verification:
      - kind: other
        ref: "gh pr view --json number,url,state --jq 'if .state == \"OPEN\" then .url else error(...) end' (exit 0, PR #87)"
        status: pass
      - kind: other
        ref: "PR #87 title is 'feat: v2.2 reliability and terminal usability'; body carries a '## Merge strategy — recommendation, not a decision' section"
        status: pass
    human_judgment: false
  - id: D3
    description: "All three required status contexts concluded SUCCESS on the pushed branch — SHIP-01's CI half, and the first CI run ever to touch phases 8-11"
    requirement: "SHIP-01"
    verification:
      - kind: other
        ref: "gh pr view --json statusCheckRollup --jq '[...check (macos-14)/check (ubuntu-22.04)/docker] | if length == 3 and all(.conclusion==\"SUCCESS\") then \"ok\" else error(...) end' (exit 0)"
        status: pass
      - kind: other
        ref: "ci.yml run 35253356923 on head e07b28e — conclusion success; all four artifact-readiness targets and CodeQL also SUCCESS"
        status: pass
    human_judgment: false
  - id: D4
    description: "The post-rebase CI-parity ladder and locked release build were re-run before the push, with every exit code observed directly"
    requirement: "SHIP-01"
    verification:
      - kind: other
        ref: "cargo fmt --check; cargo clippy --all-targets -- -D warnings; assert-real-roots-untouched.sh --self-test | before; cargo test -- --test-threads=1 (645 passed, 0 failed); cargo build --workspace --release --locked (Cargo.lock unchanged) — six of seven gates exit 0"
        status: pass
      - kind: other
        ref: "bash scripts/assert-real-roots-untouched.sh after (exit 1 — NOT attributable to the suite; see Deviations, and closed by check (macos-14) / check (ubuntu-22.04) on a clean runner)"
        status: fail
    human_judgment: true
    rationale: "Gate 6 exited 1 on this host and cannot be made to pass while the maintainer's live baude and a concurrent FHIR IG publisher are running. Attribution was proven mechanically by a null bracket, and the authoritative clean-room measurement is green in CI — but a human should confirm they accept that substitution rather than have an executor auto-pass a red gate."
  - id: D5
    description: "12-RELEASE-CHECKLIST.md records the CI evidence and a fully unchecked pre-release checklist covering the phase-8 re-stamp, version agreement, distribution outputs, the evidence gate, and merge strategy"
    requirement: "SHIP-04"
    verification:
      - kind: other
        ref: "grep -nE '^## CI evidence \\(SHIP-01\\)' && grep -cE 'https://github\\.com/.*/actions/runs/[0-9]+' (exit 0, 9 run URLs)"
        status: pass
      - kind: other
        ref: "grep -nE '^## Pre-release checklist' && 'gsd-verify-work 8' && 'SHA256SUMS' && '2\\.2\\.0' && 'vendor/vt100' (all exit 0)"
        status: pass
      - kind: other
        ref: "grep -cE '^- \\[ \\]' == 26 unchecked boxes (1 ticked, documented as a deliberate exception)"
        status: pass
    human_judgment: false
  - id: D6
    description: "Nothing was merged, labeled, or published — plan 12-05's gates are untouched"
    requirement: "SHIP-04"
    verification:
      - kind: other
        ref: "gh pr view 87 --json state,mergedAt --jq '{state,mergedAt}' -> {\"state\":\"OPEN\",\"mergedAt\":null}"
        status: pass
      - kind: other
        ref: "gh pr view 87 --json labels --jq '[.labels[].name]' -> [] (no release:hold, no release:minor)"
        status: pass
    human_judgment: false

# Metrics
duration: ~45min
completed: 2026-09-17
status: complete
---

# Phase 12 Plan 04: Rebase, Push, PR and Pre-Release Checklist Summary

**174 commits spanning phases 8 through 11 reached the remote for the first time via a squash-aware `--onto` rebase that produced zero conflicts and a byte-identical tree, and PR #87's first-ever CI run over that work is green on all three required contexts — with the one red local gate proven, by a null bracket, to be measuring the maintainer's live machine rather than baude's test suite.**

## Performance

- **Duration:** ~45 min
- **Completed:** 2026-09-17T17:44Z
- **Tasks:** 3
- **Files created:** 1 (`12-RELEASE-CHECKLIST.md`)

## Accomplishments

- **The chain is on `origin/main`'s shoulders, and the rebase was not the one the plan expected.** `origin/main`'s `30ce007` (PR #86) turned out to be a *squash* of this very chain's first seven planning commits — `git diff 985f543 30ce007` is empty. A plain `git rebase origin/main` therefore conflicted on already-applied content at commit 1 of 181, in `.planning/PROJECT.md` and `.planning/STATE.md`. `git rebase --onto origin/main 985f543` replayed only the 174 post-squash commits: **zero conflicts across all 174**, and `git diff fd22555 HEAD` is empty, proving the resulting tree is byte-identical to the pre-rebase tree.
- **Lineage was asserted before anything moved.** All four phase tips plus `gsd/v2.2-reliability-terminal-usability` were confirmed ancestors of HEAD via `git merge-base --is-ancestor`, so D-08's one-PR assumption held. The rebase rewrote them; their post-rebase equivalents (`9a46f33`, `454a91e`, `5b13ba9`, `fcaf40b`) are recorded rather than left as a surprise for a later reader who finds the old refs no longer ancestors.
- **SHIP-01's CI half now exists.** PR [#87](https://github.com/poindexter12/baude/pull/87) on head `e07b28e`, `ci.yml` run [35253356923](https://github.com/poindexter12/baude/actions/runs/35253356923): `check (macos-14)`, `check (ubuntu-22.04)` and `docker` all **SUCCESS**, plus all four `artifact-readiness` targets (including `ubuntu-22.04-arm`) and CodeQL. Before this run, no CI had ever touched phases 8-11 — the requirement had literally no evidence.
- **The one red local gate was diagnosed, not rationalized.** See Deviations. The short version: a 45-second bracket with *no suite running at all* also fails, so the delta is not the suite's.
- **The checklist represents its inputs at their real strength.** The smoke-evidence item spells out that macOS was a labeled bulk attestation with one individually-reported leg, names all three signed gaps, and states the full Linux deferral — so ticking that box cannot be mistaken for twelve behaviors observed on two platforms.
- **Nothing was merged, labeled, or published.** PR #87 is `OPEN`, `mergedAt: null`, labels `[]`.

## Task Commits

1. **Task 1: Rebase, re-run the bracket, push** — no commit by design (`<files>` is "no files modified — this task moves git refs"). The ref movement is the artifact: `e07b28e` pushed with upstream tracking.
2. **Task 2: Open the PR and harvest CI evidence** — `562f072` (docs)
3. **Task 3: Assemble the pre-release checklist** — `4736ec3` (docs)

**Close-out:** `20b8098` (docs) scopes the evidence to the sha it judged, plus the plan-metadata commit following this SUMMARY.

## Files Created/Modified

- `.planning/phases/12-validation-and-v2-2-0-release/12-RELEASE-CHECKLIST.md` — CI evidence for PR #87 (judged head sha, run URLs and conclusions for all three required contexts, the four artifact-readiness targets, and CodeQL), the rebase provenance, the local-gate attribution, and a 27-box pre-release checklist across six sections.

## Decisions Made

- **`--onto` over a plain rebase.** The squash relationship made the plain form structurally wrong, not merely inconvenient: it would have re-applied seven commits whose content is already on `main`, and then dragged `.planning/STATE.md` conflicts through 174 replays. The `--onto` form starts from a tree byte-identical to `30ce007`, which is why every subsequent commit applied cleanly. A merge would also have satisfied the acceptance criterion, but it would have cost the linear history that lets release-please parse all 174 commits individually.
- **Pushed with gate 6 red, after proving attribution.** Recorded as an unmeasurable gate with its evidence, never as green. The authoritative measurement is the same bracket running on two ephemeral CI runners, both SUCCESS.
- **One pre-ticked box.** The plan's acceptance criterion forbids pre-ticking items "that belong to the maintainer at the gate". The phase-8 re-stamp no longer does — it happened on 2026-09-16. Ticking it with both commit hashes and the 12-01 bracket citation records a fact; leaving it open would have manufactured a phantom blocker.
- **Added `release:hold` timing as an item.** Not in the plan's list, but `release-automerge.yml` merges a soaked `release:minor` PR on a `*/30` cron and the human gate is opt-in. A checklist for a release that can publish itself on a timer is incomplete without it (deviation Rule 2).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] The planned rebase was structurally infeasible; used `--onto` instead**
- **Found during:** Task 1
- **Issue:** `git rebase origin/main` conflicted immediately (commit 1 of 181) in `.planning/PROJECT.md` and `.planning/STATE.md`. Research predicted "the only divergent commit to be a `.planning/` docs commit" and expected a trivial resolution. The real cause is that `30ce007` is a **squash merge of this chain's own first seven commits** (its body lists them verbatim), so the rebase was trying to re-apply content already present on the base.
- **Fix:** Verified the squash relationship (`git diff 985f543 30ce007` is empty — trees identical), aborted, and rebased with `git rebase --onto origin/main 985f543` to replay only the 174 post-squash commits. Zero conflicts.
- **Verification:** `git diff --stat fd22555 HEAD` empty (tree byte-identical to pre-rebase); `git merge-base --is-ancestor origin/main HEAD` exit 0; `0 174` divergence.
- **Committed in:** no commit (ref movement only)

**2. [Rule 3 - Blocking] Preserved the orchestrator's uncommitted config state across the rebase without stashing**
- **Found during:** Task 1
- **Issue:** The plan's precondition requires an empty `git status --porcelain`, but `.planning/config.json` carried an uncommitted orchestrator flag (`_auto_chain_active: true`) and git refuses to rebase with unstaged changes. `git stash` is prohibited — its stack is shared across the main checkout and every linked worktree.
- **Fix:** Captured `git diff -- .planning/config.json` to a scratchpad patch, restored the file, rebased, then re-applied the patch. `.gsd/` and `.planning/milestone.lock` are untracked and do not block a rebase, so they were left alone.
- **Verification:** `grep -n '_auto_chain_active' .planning/config.json` → `true` after the rebase; `git status --porcelain` shows the same three pre-existing entries as at plan start. Nothing in `.planning/config.json`, `.gsd/`, or `.planning/milestone.lock` was committed.
- **Committed in:** no commit (working-tree state only)

### Gate not satisfied, with attribution

**3. [Gate 6] `assert-real-roots-untouched.sh after` exited 1 on this host and could not be made to pass**

This is recorded as an unmet gate, not an auto-fix, because I did not fix it — I established what it was measuring.

- **Found during:** Task 1's post-rebase ladder. Gates 1-5 and 7 all exit 0, including the full 645-test serial suite and `cargo build --workspace --release --locked` with `Cargo.lock` unchanged.
- **Observed delta:** `/Users/joese/.config/baude` (`breadcrumbs-claude.json` grew 20200 → 20288 bytes) and `/Users/joese/.local/share/baude/worktrees` (44 entries removed, all under `claude/repository-16/primary-52/fhir/pharma/fsh-generated/`).
- **Attribution, proven mechanically rather than argued:** a **null bracket** — `before`, 45 seconds of sleep, `after`, with no suite run whatsoever — also exited 1, on three roots. The roots mutate with nothing under test. The process list then named both actors: **pid 8413**, the maintainer's live `baude` instance (the same one that held the workspace lock during plan 12-03), appending breadcrumbs; and **pid 21951**, a FHIR IG publisher running `sushi` and `publisher.jar` in Docker with `~/.local/share/baude/worktrees/claude/repository-16/primary-52/fhir/pharma` bind-mounted, which accounts for the removed `fsh-generated/*` entries. FHIR implementation-guide artifacts are not a shape the baude suite can produce.
- **Why it was not fixed:** the script takes exactly three modes (`before | after | --self-test`) with no narrowing option, so no clean local measurement exists while those processes run. Quitting the maintainer's editor or killing their IG build to make a gate pass would be destructive and out of scope.
- **How it is closed:** `check (macos-14)` and `check (ubuntu-22.04)` run this identical bracket on ephemeral runners where no other process exists, so the `after` assertion measures only the suite. **Both concluded SUCCESS on PR #87.** That is the authoritative measurement, and it is the same evidence class SHIP-01 asks for.
- **Represented as:** an explicit subsection of `12-RELEASE-CHECKLIST.md` (`### What the two check jobs prove that the local ladder could not`) and `human_judgment: true` with a `status: fail` verification entry on coverage item D4 — so `/gsd-verify-work` routes it to a human rather than auto-passing it.

---

**Total deviations:** 2 auto-fixed (both Rule 3 - blocking), 1 gate unmet with proven attribution.
**Impact on plan:** No scope creep. Both auto-fixes were mechanical prerequisites to executing Task 1 at all. The unmet gate did not change what shipped; it changed which measurement of that gate is authoritative, and the substitution is recorded rather than hidden.

## Issues Encountered

- **The plan's precondition was not literally met at start.** `git status --porcelain` was non-empty (`.planning/config.json` modified, `.gsd/` and `.planning/milestone.lock` untracked). The orchestrator pre-declared these as its own transient state and instructed that they be left untouched and never committed, which is what happened. The substantive half of the precondition — an authenticated `gh` — held (`gh auth status` → logged in as `poindexter12`, git protocol ssh).
- **Research's numbers had drifted.** It recorded 165 commits ahead, 19 `fix`, and a phase-11 tip of `97b7a7f`; the real figures at execution were 181 ahead (174 post-squash), 20 `fix`, and tip `2d8c0ff`. Phases 8-11 continued to advance after research was written. No conclusion changed: 33 `feat`, 0 breaking, `bump-minor-pre-major: false` from `2.1.5` still lands on 2.2.0.
- **Docs-only commits after the evidence run shift PR #87's head.** Rather than chase the sha, the checklist now states that post-evidence commits are `.planning/`-only and proves it (`git diff e07b28e HEAD -- ':!.planning'` is empty), and carries an explicit evidence-gate item to re-check the required contexts against the PR's current head at merge time, since `enforce_admins: true` gates on the current head.

## Known Stubs

None. No placeholder values, no skipped tests, and no `<verify>` command left unrun — every gate in this plan was executed and its exit code observed directly rather than through a pipe.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- **Plan 12-05 has everything its gates act on.** PR #87 is open, unmerged, unlabeled, with three green required contexts; `12-RELEASE-CHECKLIST.md` carries the CI evidence and 27 pre-release boxes; `12-SMOKE-EVIDENCE.md` and the SHIP-02 audit are complete from plans 12-03 and 12-02.
- **Deliberately not done, and reserved for 12-05's blocking-human checkpoints:** merging PR #87, applying or removing any release label, and publishing. The `release:hold` timing hazard is recorded as a checklist item because the human publish gate is opt-in, not opt-out.
- **One carried-forward item for `/gsd-complete-milestone`:** `.planning/STATE.md` still reads `current_phase: 09` while phase 12 executes. Recorded in the checklist. The ROADMAP half of research's phase-8 inconsistency is already resolved (phase 8 now reads `8/8 | Complete | 2026-09-16`).
- **A note for any future local ladder run on this host:** the real-roots gate is a machine-global assertion and will fail whenever a live `baude` or any process writing under the managed-worktrees root is running. The null bracket is the cheap discriminator; CI is the clean room.

## Self-Check: PASSED

- `12-RELEASE-CHECKLIST.md` verified present on disk.
- All three task/close-out commits verified present in `git log`: `562f072`, `4736ec3`, `20b8098`.
- Branch verified pushed with upstream tracking and zero unpushed commits at the time of writing.
- PR #87 verified `OPEN`, `mergedAt: null`, labels `[]`.
- All eight plan `<verify>` commands across the three tasks were executed; seven exit 0, and the one failure (gate 6) is documented above with proven attribution rather than skipped.

---
*Phase: 12-validation-and-v2-2-0-release*
*Completed: 2026-09-17*
