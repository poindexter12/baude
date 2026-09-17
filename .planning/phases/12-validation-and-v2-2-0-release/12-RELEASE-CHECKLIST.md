# v2.2.0 Release Checklist

Evidence and pre-release items for the v2.2 milestone release. Created by plan
12-04; the unchecked items below are worked through by the maintainer before
plan 12-05's blocking-human gates authorize anything.

## CI evidence (SHIP-01)

**This is the first CI run that has ever touched phases 8 through 11.** None of
`gsd/phase-08` … `gsd/phase-11` was ever pushed; the previous `ci.yml` runs
reached only `main` and the milestone planning branch. SHIP-01's
"existing supported-platform CI checks pass" had no evidence at all until this
run existed.

| Field | Value |
|-------|-------|
| Pull request | [#87](https://github.com/poindexter12/baude/pull/87) — `feat: v2.2 reliability and terminal usability` |
| Base → head | `main` ← `gsd/phase-12-validation-and-v2-2-0-release` |
| Head sha CI judged | `e07b28e897609b3a30fcdca45d13e5e237dfc7c9` |
| Workflow run | `ci.yml` run [35253356923](https://github.com/poindexter12/baude/actions/runs/35253356923) |
| Run conclusion | **success** |
| Commits carried | 174 (33 `feat`, 20 `fix`, 72 `docs`, 43 `test`, 5 `style`, 1 `refactor`; 0 breaking) |
| PR state at time of writing | **OPEN, unmerged** |

### Which sha this evidence is about

`e07b28e` is the sha at which the **code** was judged. Commits pushed after it
in this plan are `.planning/`-only (this checklist, the plan SUMMARY, and the
STATE/ROADMAP updates), so they do not change the source tree — provable with
`git diff e07b28e HEAD -- ':!.planning'`, which is empty. Each such push
re-triggers `ci.yml` on the new head, and those runs are expected to be green
for the same reason.

The maintainer's merge-time check is therefore "are the three required contexts
green on PR #87's *current* head", which is its own item under
**Evidence gate** below. This section is the record of the first run that ever
exercised phases 8 through 11, not a substitute for that check.

### Required status contexts — all three green

`main`'s branch protection requires exactly these three, with
`enforce_admins: true`, so there is no merge path around a red one.

| Job | Conclusion | Run URL |
|-----|-----------|---------|
| `check (macos-14)` | **SUCCESS** | [job 105310839908](https://github.com/poindexter12/baude/actions/runs/35253356923/job/105310839908) |
| `check (ubuntu-22.04)` | **SUCCESS** | [job 105310839448](https://github.com/poindexter12/baude/actions/runs/35253356923/job/105310839448) |
| `docker` | **SUCCESS** | [job 105310839864](https://github.com/poindexter12/baude/actions/runs/35253356923/job/105310839864) |

### Non-required contexts (recorded, not blocking)

`artifact-readiness` runs on every PR but is **not** a required context. It is
recorded here because it is the cross-platform build proof for the four release
targets.

| Job | Conclusion | Run URL |
|-----|-----------|---------|
| `artifact-readiness (aarch64-apple-darwin, macos-14)` | SUCCESS | [job 105310839829](https://github.com/poindexter12/baude/actions/runs/35253356923/job/105310839829) |
| `artifact-readiness (x86_64-apple-darwin, macos-14)` | SUCCESS | [job 105310839975](https://github.com/poindexter12/baude/actions/runs/35253356923/job/105310839975) |
| `artifact-readiness (x86_64-unknown-linux-gnu, ubuntu-22.04)` | SUCCESS | [job 105310840031](https://github.com/poindexter12/baude/actions/runs/35253356923/job/105310840031) |
| `artifact-readiness (aarch64-unknown-linux-gnu, ubuntu-22.04-arm)` | SUCCESS | [job 105310839792](https://github.com/poindexter12/baude/actions/runs/35253356923/job/105310839792) |
| `CodeQL` / `Analyze (rust, actions, javascript-typescript)` | SUCCESS | [run 35253356223](https://github.com/poindexter12/baude/actions/runs/35253356223) |

### What the two `check` jobs prove that the local ladder could not

`check` runs the same seven-gate ladder as plan 12-01, including the phase-8
real-root bracket (`assert-real-roots-untouched.sh --self-test` / `before` /
`after`). On an ephemeral runner there is no other process on the machine, so
the `after` assertion measures *only* the suite.

That matters because the post-rebase local re-run of the ladder **could not
measure that gate on this host**. Six of seven gates exited 0 (including the
full 645-test serial suite and the locked release build), but
`assert-real-roots-untouched.sh after` exited 1 for reasons provably external to
baude's test suite:

- A **null bracket** — `before`, 45 seconds of sleep, `after`, with no suite run
  at all — also exited 1, on three roots. The roots mutate with nothing under
  test.
- Two concurrent actors on the host were identified: pid 8413, the maintainer's
  live `baude` instance, appending `breadcrumbs-claude.json` inside
  `~/.config/baude`; and pid 21951, a FHIR IG publisher (`sushi` +
  `publisher.jar` in Docker) bind-mounted onto
  `~/.local/share/baude/worktrees/claude/repository-16/primary-52/fhir/pharma`,
  which accounts for the 44 removed `fsh-generated/*` entries.
- The script takes exactly three modes (`before | after | --self-test`) and has
  no narrowing option, so no clean local measurement was available while those
  processes ran.

`check (macos-14)` and `check (ubuntu-22.04)` both concluding SUCCESS is
therefore the authoritative clean-room measurement of that gate, and it closes
the gap rather than papering over it. The local failure is recorded as a
measurement-environment fact, not as a green gate.

### Rebase provenance

`origin/main`'s `30ce007` (PR #86) is a **squash** of this chain's own first
seven planning commits: `git diff 985f543 30ce007` is empty, so its tree is
byte-identical to `gsd/v2.2-reliability-terminal-usability`. A plain
`git rebase origin/main` therefore conflicted immediately on already-applied
content. The branch was rebased with `git rebase --onto origin/main 985f543`,
replaying only the 174 commits that follow the squash point: zero conflicts, and
`git diff fd22555 HEAD` confirms the resulting tree is byte-identical to the
pre-rebase tree.

Lineage was asserted before the rebase — `gsd/phase-08`, `gsd/phase-09`,
`gsd/phase-10`, `gsd/phase-11` and `gsd/v2.2-reliability-terminal-usability`
were each confirmed an ancestor of HEAD via
`git merge-base --is-ancestor`, so one PR carries all four phases (D-08). The
rebase rewrote those commits, so the old branch refs are no longer ancestors of
the new HEAD; their post-rebase equivalents are `9a46f33`, `454a91e`,
`5b13ba9`, and `fcaf40b`.

## Pre-release checklist

Worked through by the maintainer before plan 12-05's blocking-human checkpoints
authorize the merge, the label change, or the publish. Plan 12-04 wrote this
list; with one documented exception it does not tick it.

### Phase 8 re-stamp (D-11)

- [x] Re-run `/gsd-verify-work 8`. **Done 2026-09-16** — commits `8ca3d42`
      (`test(08): complete UAT - 2 passed, 0 issues`) and `164fc26`
      (`docs(phase-8): re-verify and complete phase`). `08-UAT.md` now exists
      and `08-VERIFICATION.md` reads `status: passed`.

  Why it was outstanding, recorded so the tick is auditable rather than
  assumed: `08-VERIFICATION.md`'s frontmatter said `passed` while its own Gaps
  Summary said `human_needed`, for one reason only — two of its verifications
  are by their nature measurements of the developer's real filesystem, and the
  phase-8 verifier operated under a hard constraint forbidding it to read,
  write, or fingerprint those roots.

  **Plan 12-01's bracket is exactly that forbidden measurement, and it passed.**
  `assert-real-roots-untouched.sh --self-test` (40 checks, 0 failures) then
  `before`, then the full 645-test serial suite, then `after` → exit 0,
  `PASS: the run left all four real roots untouched.` The four roots
  fingerprinted on this host were `/Users/joese/.config/baude`,
  `/Users/joese/.poindexter/claude`,
  `/Users/joese/.local/share/baude/worktrees`, and `/Users/joese/Code`; all four
  reported `unchanged (present, ...)` in the after-check. `check (macos-14)` and
  `check (ubuntu-22.04)` on PR #87 re-prove the same bracket in a clean room.

  This item is the one pre-ticked box in this list. It is ticked because the
  re-stamp demonstrably already happened and is no longer maintainer work at the
  gate; leaving it open would misrepresent the project state.

### Planning-state consistency

- [ ] Reconcile `.planning/STATE.md` before `/gsd-complete-milestone`. Its
      frontmatter still reads `current_phase: 09`, `status: planning`,
      `stopped_at: Phase 8 complete, ready to plan Phase 09` while phase 12 is
      executing. `/gsd-complete-milestone` will otherwise be working from a
      position four phases stale. The ROADMAP half of this inconsistency is
      already resolved — phase 8 now reads `8/8 | Complete | 2026-09-16`, not
      the "In Progress with all plans done" state research recorded.

- [ ] Note, no action expected: phases 9, 10 and 11 report `stale` to the
      staleness scanner even though each `NN-VERIFICATION.md` reads
      `status: passed` and the ROADMAP shows all three Complete. The cause is
      structural, not a real gap — `.planning/REQUIREMENTS.md` appears in every
      phase's `covered_files` list, and each subsequent phase completion
      rewrites that file, which invalidates every earlier phase's file
      fingerprints. Re-verifying them would not change any finding. Recorded so
      a later reader does not mistake the label for outstanding work.

### Version agreement (SHIP-04)

"Matching versions" for this release means these seven all read **2.2.0**:

- [ ] Root `Cargo.toml` inline marker (line 15,
      `baude-core = { path = "baude-core", version = "=2.1.5" } # x-release-please-version`)
- [ ] `baude-core/Cargo.toml` `package.version`
- [ ] `baude/Cargo.toml` `package.version`
- [ ] `bauded/Cargo.toml` `package.version`
- [ ] `.release-please-manifest.json` (currently `{ ".": "2.1.5" }`)
- [ ] The git tag `v2.2.0`
- [ ] The `CHANGELOG.md` heading for 2.2.0

release-please owns all four source files via its `extra-files` list plus the
manifest, and `release-please.yml` separately clones the release branch and runs
`cargo update --workspace` to refresh `Cargo.lock`, committing
`chore: finalize release metadata`.

- [ ] Confirm `vendor/vt100/Cargo.toml` still reads `version = "0.15.2"`.
      **The vendored fork is deliberately excluded from version matching** and
      must stay at its upstream value: that number names the upstream vt100
      release the fork derives from, not baude's. It is intentionally absent
      from `extra-files`, and bumping it would be a defect, not a correction.

- [ ] Confirm no version was hand-edited anywhere in this phase. A manual bump
      fights the release PR and desyncs `.release-please-manifest.json`.
      release-please's arithmetic from `2.1.5` over 33 `feat`, 20 `fix` and zero
      breaking changes lands on 2.2.0 on its own, which `release-please.yml`
      classifies as `tier=minor`.

### Distribution outputs (D-10)

Confirm at publish time. No packaging rework is in scope for this phase; the
existing `release.yml` outputs are reused as-is and verified, and all four of
the last four release runs (v2.1.2 … v2.1.5) concluded success.

- [ ] `baude-v2.2.0-aarch64-apple-darwin.tar.gz` attached
- [ ] `baude-v2.2.0-x86_64-apple-darwin.tar.gz` attached
- [ ] `baude-v2.2.0-x86_64-unknown-linux-gnu.tar.gz` attached
- [ ] `baude-v2.2.0-aarch64-unknown-linux-gnu.tar.gz` attached
- [ ] Each of the four tarballs carries **both** binaries (`baude` and `bauded`)
- [ ] `SHA256SUMS.txt` present (generated by `release.yml`'s
      `shasum -a 256 *.tar.gz > SHA256SUMS.txt`)
- [ ] The multi-arch container image was pushed (the `image` + `image-manifest`
      jobs, two architectures behind one manifest)

- [ ] Note, no action expected: `cargo publish` is not used anywhere in this
      repo — distribution is entirely GitHub Release tarballs plus the ghcr.io
      image. This is why `baude-core`'s vendored path dependency
      (`vt100 = { path = "../vendor/vt100" }`) is harmless to both shipping
      paths. A path dependency would be fatal to `cargo publish` and is
      irrelevant to a tarball or an image build. No mitigation is needed.

### Evidence gate

The artifacts plan 12-05's checkpoints act on. Each must be present and read as
claimed before the gate is answered.

- [ ] **CI evidence:** the `## CI evidence (SHIP-01)` section above — PR #87,
      head `e07b28e`, `ci.yml` run 35253356923, all three required contexts
      SUCCESS.

- [ ] **Confirm the three required contexts are green on PR #87's current
      head**, not only on `e07b28e`. `enforce_admins: true` gates on the current
      head, and this plan pushed `.planning/`-only commits after the recorded
      run. `gh pr checks 87` answers it.

- [ ] **Smoke evidence (SHIP-03):** `12-SMOKE-EVIDENCE.md` is complete per plan
      12-03, meaning no leg is unaccounted for. Read what it actually is before
      signing, because it is weaker than "twelve behaviors verified on two
      platforms" and says so in its own header:

  - The **macOS session is a labeled bulk attestation**, not a leg-by-leg
    walkthrough. Exactly one leg was reported individually (leg 1, `ctrl+o`
    opens the link overlay: "yeah, ctrl o works"). The other eleven rest on a
    single set-level statement, "all works flawlessly", recorded as covering the
    set rather than paraphrased into each leg.
  - **Three gaps are signed, not smoothed over.** (1) Leg 2's
    label-vs-destination check was never narrated, which is precisely the
    property LINK-01 exists to guarantee; automated `links::` tests cover it,
    a human did not visually confirm it. (2) The reported mouse-click link
    opening is the outer terminal's behavior, not baude's — iTerm2 detects URLs
    independently, and phase 10 shipped keyboard-only activation deliberately.
    (3) Legs 8-9 may not have been exercised at all, since they need a claude
    pane and the smoke instance started with no sessions.
  - The **Linux session is deferred in full**, with sign-off. No Linux terminal
    was available; all twelve Linux legs read DEFERRED. Legs 1-4 and 8-9 name
    `check (ubuntu-22.04)` as their automated proxy — that job's run URL is
    recorded in the CI evidence section above. Legs 5-7 and 10-12 are
    outer-terminal behaviors with no automated substitute and are deferred
    outright.

  Signing this box means accepting that evidence at its stated strength. It does
  not mean twelve behaviors were observed on two platforms.

- [ ] **Documentation (SHIP-02):** the grep criteria are satisfied per plan
      12-02, which recorded `requirements-completed: [SHIP-02]` and
      `status: complete`.

### Merge strategy and release gating

- [ ] **Choose the merge method for PR #87.** This is the maintainer's call at
      the gate, not plan 12-04's.

  **Recommended: merge commit.** It is the only option that produces the
  per-feature changelog the milestone asked for — release-please parses all 174
  commits individually and emits 33 Features plus 20 Bug Fixes, filtering
  `docs`/`test`/`style`/`refactor` out by default. `required_linear_history` is
  false on this repo so a merge commit is permitted, and it matches how the
  release PRs themselves are merged.

  A **squash** collapses those 174 into one commit whose body concatenates the
  messages; whether release-please extracts individual conventional commits from
  a concatenated body is unverified, so the safe reading is a one-line v2.2.0
  changelog.

  **Hazard if squashing: a non-`feat` PR title cuts no release at all.** PR
  #86's `docs:` title produced none. PR #87 is titled
  `feat: v2.2 reliability and terminal usability`, which is correct under both
  strategies — so this hazard is already neutralized as long as the title is
  not edited at merge time.

- [ ] **Apply `release:hold` as the very first action after PR #87 merges**,
      before anything else. `release-automerge.yml` runs on a `*/30` cron and
      merges a `release:minor` PR once `SOAK_HOURS=2` has elapsed and required
      checks are green. The human gate is **opt-in, not opt-out**: without the
      label, v2.2.0 can publish on a timer without anyone authorizing it.

      ```bash
      num=$(gh pr list --state open --label 'release:minor' --json number --jq '.[0].number')
      gh pr edit "$num" --add-label release:hold
      ```

- [ ] Release the gate only after the boxes above are answered:
      `gh pr edit "$num" --remove-label release:hold` (the cron lands it within
      30 minutes), or `gh pr merge "$num" --merge` to publish immediately.

- [ ] Do not hand-write the v2.2.0 changelog. release-please generates and owns
      `CHANGELOG.md`; a hand-written file is overwritten on the release branch.


---

## Merge Authorization

**Merge authorized by:** Joe Seymour on 2026-09-17

- PR [#87](https://github.com/poindexter12/baude/pull/87) merged into `main` as a
  **merge commit** (`aa12cb2ee0045ce5e6426618a85ab83c43f0cf57`) at
  2026-09-17T18:24:17Z. Merge-commit rather than squash so release-please sees
  every conventional commit; PR #86's `docs:`-titled squash cut no release and is
  the precedent that decided this.
- All 11 checks were green at merge time, including the three required contexts
  `check (macos-14)`, `check (ubuntu-22.04)` and `docker`, plus CodeQL and
  artifact-readiness on all four release targets.
- Authorization was given in response to an explicit one-way-door checkpoint that
  named the consequence (release-please cutting a v2.2.0 release PR) and the
  `release:hold` timing hazard.

### Release PR and the hold

- release-please opened [#88](https://github.com/poindexter12/baude/pull/88)
  (`chore(main): release 2.2.0`) 80 seconds after the merge.
- **The `release:hold` label did not exist in this repository.** The first
  labelling attempt failed with `'release:hold' not found`, leaving #88 carrying
  only `release:minor` + `autorelease: pending` — precisely the state
  `release-automerge.yml` selects for its `*/30` cron. The label was created
  (`B60205`, "Blocks release-automerge: requires explicit human authorization to
  publish") and applied; `autoMergeRequest` is `none`.
- Hold verified against the workflow itself, not assumed:
  `release-automerge.yml:40-42` filters
  `select([.labels[].name] | index("release:hold") | not)`, so #88 is now
  excluded from auto-merge.
- **Worth fixing at leisure:** the escape hatch documented at
  `release-automerge.yml:8` depended on a label nobody had created, so every
  prior minor release was auto-mergeable with no way to veto it. The label now
  exists permanently.

## Publish Authorization

_Not yet authorized. v2.2.0 remains unpublished; PR #88 is held._

To authorize, this section must carry a line of the form
`**Publish authorized by:** <name> on <YYYY-MM-DD>` — a distinct sentinel from
the merge record above, so a merge approval can never be mistaken for a publish
approval (plan 12-05 Task 3's gate gates on exactly this heading and line).
