# Design: release-please with tiered auto-release gating

Status: **Proposed** (design only — not implemented)
Author: design pass, 2026-07-02
Prereq that needs YOU: a bot token secret (see §5) — a design blocker only the repo owner can create.

## Goal

Automate the manual release chore (the `chore: release-vX.Y.Z` version-bump PR +
hand-pushed `vX.Y.Z` tag) using **release-please**, gated by bump size:

| Bump | Commit type | Release gate |
|------|-------------|--------------|
| **patch** | `fix:` | auto-merge the release PR as soon as required checks pass |
| **minor** | `feat:` | auto-merge after a **~2h soak** (tunable) once checks are green |
| **major** | `feat!` / `BREAKING CHANGE:` | **manual** — never auto-merged; human approves |

The existing tag/release build (`.github/workflows/release.yml`: 4 binary tarballs
+ multi-arch ghcr `bauded` image + GitHub release) is reused, with one edit (§4).

## How release-please fits

release-please watches `main`, reads Conventional Commits (already the repo
convention), and maintains ONE open "release PR" that bumps the version, updates
`Cargo.lock`, and writes `CHANGELOG.md`. Merging that PR creates the `vX.Y.Z` tag
and a GitHub Release. **release-please itself has no auto/soak/manual concept** —
that gating is a thin layer we add on top (§3).

## 1. Config files (new)

- `release-please-config.json` — one component at repo root:
  ```json
  {
    "packages": {
      ".": {
        "release-type": "rust",
        "changelog-path": "CHANGELOG.md",
        "include-component-in-tag": false,
        "bump-minor-pre-major": false
      }
    },
    "$schema": "https://raw.githubusercontent.com/googleapis/release-please/main/schemas/config.json"
  }
  ```
- `.release-please-manifest.json` — seed with the shipped version:
  ```json
  { ".": "0.7.4" }
  ```
- `CHANGELOG.md` — created/owned by release-please going forward.

### VALIDATION RISK — workspace version updater

baude's version lives in `[workspace.package] version` with members using
`version.workspace = true` (a virtual manifest, no root `[package]`). Confirm the
`rust` release-type bumps `[workspace.package].version` **and** `Cargo.lock`.
If it doesn't, fall back to the generic-annotation approach: add
`version = "0.7.4" # x-release-please-version` to `Cargo.toml`, list it under
`extra-files`, and keep `rust` type for the `Cargo.lock` refresh. This must be
proven on a throwaway test PR before we rely on it.

## 2. release-please workflow (new)

`.github/workflows/release-please.yml`, `on: push: branches: [main]`:
- Runs `googleapis/release-please-action@v4` with the config above.
- Uses a **bot token** (§5), NOT the default `GITHUB_TOKEN` — critical (§5).
- On the run where the PR is first opened/updated, a follow-up step classifies the
  bump and applies a label (§3).

## 3. Gating layer — "scheduled workflow + labels"

**Classify the bump** (in the release-please workflow, after the PR is
opened/updated): compare the PR's proposed version against the current
`.release-please-manifest.json` and label the PR exactly one of
`release:patch` / `release:minor` / `release:major`.

**Auto-merge decisions:**
- **patch** → enable GitHub **native auto-merge** on the PR
  (`gh pr merge --auto --merge`). GitHub merges it the instant the 3 required
  checks pass. No soak.
- **minor** → do NOT enable native auto-merge (it can't express a delay). A
  **cron workflow** (`on: schedule`, every ~30 min) finds the open
  `release:minor` PR and merges it iff: PR age ≥ 2h **and** all required checks
  green **and** not `release:major`. The 2h is a single `SOAK_HOURS` env knob.
- **major** → nothing auto. The workflow adds a `release:manual` label + a comment
  ("major bump — review and merge manually to release"). Human merges.

Branch protection still gates every path: the merge (native or cron) only lands
after `check (macos-14)`, `check (ubuntu-22.04)`, `docker` pass. `enforce_admins`
is satisfied because the merge waits on checks either way.

**Escape hatches:** a maintainer can always merge manually (fast-track a minor
past its soak), or add a `release:hold` label the cron step treats as a veto.

## 4. Reuse the existing release build (one edit)

Today `release.yml` triggers on tag push and does `gh release create … --generate-notes`.
release-please will ALSO create the GitHub Release for the tag → collision.

Fix: release-please owns tag + Release (with changelog notes); `release.yml`
switches to attach assets to that Release:
- `on:` tag push → **`on: release: [published]`**.
- `gh release create …` → **`gh release upload "${{ github.event.release.tag_name }}" dist/*`**.
- Drop `--generate-notes` (release-please's CHANGELOG is the notes).

Everything else in `release.yml` (build matrix, ghcr image, manifest) is unchanged.
Ordering is safe: release-please publishes the Release, which fires
`release: published`, which runs the build+upload.

## 5. Bot token (YOUR action — design blocker)

PRs opened by the default `GITHUB_TOKEN` do **not** trigger other workflows, so
the release PR's required checks (`ci.yml`) would never run and branch protection
would make it unmergeable. release-please must authenticate as a **GitHub App**
(recommended) or a **PAT**:
- Preferred: a minimal GitHub App (Contents: RW, Pull requests: RW) installed on
  the repo; mint a token via `actions/create-github-app-token` and pass it to the
  action. Its pushes trigger CI.
- Simpler: a fine-grained PAT (Contents RW, PRs RW) stored as secret
  `RELEASE_PLEASE_TOKEN`.

The gating workflows (auto-merge) also use this token so their merges trigger the
tag→release chain.

## 6. Rollout

1. Add config files + `CHANGELOG.md` seed + the two workflows + the `release.yml`
   edit, in one PR.
2. Land the bot-token secret first (§5).
3. Prove the workspace-version updater on a test PR (§1 validation risk).
4. Merge. From then on: land `feat:`/`fix:` commits → release-please keeps a live
   release PR → it auto-releases per the tier policy; majors wait for a human.

## Surfaces

| File | Change |
|------|--------|
| `release-please-config.json` | new — root component, `release-type: rust` |
| `.release-please-manifest.json` | new — seed `{".":"0.7.4"}` |
| `CHANGELOG.md` | new — release-please-owned |
| `.github/workflows/release-please.yml` | new — action + bump classification/label |
| `.github/workflows/release-automerge.yml` | new — cron soak/auto-merge gating |
| `.github/workflows/release.yml` | edit — `on: release: [published]` + `gh release upload` |
| `Cargo.toml` | only if the generic-annotation fallback is needed (§1) |

## Open questions for review

- SOAK_HOURS = 2 confirmed? (tunable, single env var)
- GitHub App vs PAT for the bot token (§5)?
- Any commit scopes/types to exclude from release notes (e.g. `chore:`, `docs:` —
  release-please hides them by default; confirm that's desired)?
