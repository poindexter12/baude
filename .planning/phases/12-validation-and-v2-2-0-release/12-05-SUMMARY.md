---
plan: 12-05
phase: 12-validation-and-v2-2-0-release
title: Merge train and v2.2.0 publish
status: complete
completed: 2026-09-19
plan_head_before: fd22555
commits: 3
requirements: [SHIP-04]
key-files:
  created: []
  modified:
    - .planning/phases/12-validation-and-v2-2-0-release/12-RELEASE-CHECKLIST.md
---

# Plan 12-05 — Merge Train and v2.2.0 Publish

## What happened

Both blocking-human checkpoints were presented and answered by the maintainer,
two days apart, and both answers are recorded.

| Gate | Decision | Date |
|---|---|---|
| Merge PR #87 into main (one-way door) | authorized | 2026-09-17 |
| Publish v2.2.0 | **deferred**, then authorized | deferred 2026-09-17, authorized 2026-09-19 |

The deferral is kept in the record rather than overwritten. It was a real
decision — the maintainer chose to hold on thin human-observed evidence — and
the release is more legible with it than without.

## Outcome

v2.2.0 is public: tag `v2.2.0` at `1c37551`, release published (not draft, not
prerelease), release workflow run 35456777276 green across all 8 jobs, 4 target
tarballs plus `SHA256SUMS.txt`, and a multi-arch container manifest.

Verification was done against the published artifact, not a local build: the
aarch64-apple-darwin tarball was downloaded from the release, checksum-verified
(`OK`), extracted to exactly `baude` + `bauded`, and both executed and reported
`2.2.0` — agreeing with the tag, all four manifests, and the release notes.

## The finding worth keeping

**`release:hold` did not exist.** `release-automerge.yml:8` documents it as the
veto for the `*/30` auto-merge cron, and the workflow really does filter on
`index("release:hold") | not` — but no such label had ever been created in the
repository. The first labelling attempt failed with `'release:hold' not found`,
which left release PR #88 sitting with exactly the `release:minor` +
`autorelease: pending` labels the cron selects for. The label was created and
applied, and the filter re-read from the workflow source rather than assumed.

The consequence reaches backwards: every prior minor release in this repo was
auto-mergeable with no working way to veto it. The documented escape hatch was
decorative. It is now real and permanent.

## Merge strategy

PR #87 was merged as a **merge commit**, deliberately, so release-please saw all
52 conventional commits; the generated CHANGELOG carries every phase 8-11
`feat`. PR #86's precedent decided this — a `docs:`-titled squash cut no release
at all. PR #88 (release-please's own single-commit PR) was squashed, which is
the normal shape for it.

## What shipped on what evidence

Strong automated evidence: 646 tests / 0 failed, `fmt` and
`clippy -D warnings` clean (clippy genuinely green for the first time — the
vendored fork's failure under `-D warnings` had meant baude's own crates were
never linted), 11/11 required CI contexts, artifact-readiness on all four
targets, phases 8-11 all re-verified at HEAD with no regressions.

Thin human evidence, recorded as thin: SHIP-03's macOS session is a labeled bulk
attestation with three signed gaps (LINK-01's label-vs-destination never
narrated; the reported click-to-open was iTerm2's own URL handling; TKEY-01/02
rest on tests plus attestation, not a confirmed keystroke). The Linux session
was never run. The publish authorization states all of this explicitly so a
future bug report can be read against what was actually checked.

## Deviations

Task ordering was driven by the maintainer's decisions rather than executed
straight through, which is what a `blocking-human` gate is for. The publish
deferral on 2026-09-17 ended the first pass at the gate; the second pass resumed
at the same gate with no rework needed.
