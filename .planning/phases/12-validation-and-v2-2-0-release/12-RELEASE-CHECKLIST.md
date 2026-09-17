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
