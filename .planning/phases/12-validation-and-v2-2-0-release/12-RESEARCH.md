# Phase 12: Validation and v2.2.0 Release - Research

**Researched:** 2026-09-16
**Domain:** Release engineering — Rust workspace gates, GitHub Actions CI, release-please, human-observed terminal smoke evidence
**Confidence:** HIGH (nearly every finding was produced by running the real command in this repo this session)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Validation Scope & Evidence**
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

**Documentation (SHIP-02)**
- README feature sections are the home: link activation/preview/copy
  gestures, tested terminal support, Shift+Enter setup/fallback (phases 10-11
  landed pieces — audit and fill gaps), plus a NEW workspace-lock recovery
  section (pid diagnostic meaning, safe recovery steps) — the one SHIP-02
  item no prior phase wrote.
- Every documented item gets a grep-based acceptance criterion.
- Release notes are assembled from phase summaries per the release-please
  convention already in use.

**Release Mechanics (SHIP-04)**
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

### Deferred Ideas (OUT OF SCOPE)
None — discussion stayed within phase scope.

### Out of scope (from Phase Boundary)
New features, packaging rework, and re-verifying phases 9-11 beyond running
their suites in the gates.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| SHIP-01 | A maintainer can run focused regression tests plus workspace tests, fmt, clippy, and the existing supported-platform CI checks successfully after test isolation is in place. | §CI Parity (exact command set, measured green), §Clippy Remediation (the only three edits standing between here and exit 0, empirically proven), §Branch & Merge State (no CI has ever run on phases 8-11) |
| SHIP-02 | A user can find documented link activation/preview/copy gestures, tested terminal support, Shift+Enter setup or fallback, and workspace-lock recovery instructions. | §README Audit (line-numbered gap list; 3 of 4 items are gaps, not 1), §Workspace-Lock Recovery Facts (source-verified diagnostic + the "don't delete the lock file" hazard) |
| SHIP-03 | A maintainer has recorded real macOS/Linux terminal smoke evidence covering links, selection/scrollback, mouse interaction, Shift+Enter, ordinary Enter, and terminal restoration before release approval. | §Smoke Evidence Template (07-UAT-EVIDENCE.md precedent + provenance discipline), §Terminal-Mode Facts (mouse capture is unconditional — this shapes the selection/scrollback legs) |
| SHIP-04 | A maintainer can publish v2.2.0 through the existing release workflow after verification passes, with matching release notes/version metadata and the existing supported binary/container distribution outputs. | §Release Machinery Map (every file/line), §The Human Publish Gate (`release:hold` is the existing lever), §Merge Strategy Decision (squash vs merge-commit determines whether the changelog is one line or fifty) |
</phase_requirements>

## Summary

This phase is unusually well-scoped because the release machinery already
exists, is exercised (four releases since 2026-09-11), and needs no changes.
The work is: make the gates green, fill three documentation holes, capture
human-observed terminal evidence, and drive an existing merge-and-release
pipeline with one deliberate human stop.

Three findings change the shape of the plan versus what CONTEXT assumed.
**First, the clippy problem is larger and smaller than recorded.** Phase 11's
deferred ledger guessed "CI is presumed green on its pinned toolchain" — there
is no pinned toolchain, and the vendored crate's compile failure was *masking*
two real lints in `baude`'s own source that nobody has ever seen, because
`baude` was never reached. I ran the fix end to end this session: exactly three
edits (one vendored lint header, `baude/src/links.rs:201`,
`baude/src/ui.rs:2120`) take `cargo clippy --all-targets -- -D warnings` from
exit 101 to `Finished`. **Second, the README is missing three of SHIP-02's four
items, not one.** Phase 10 documented `ctrl+o` in the *in-app help overlay* and
the vendored fork's README, never in `README.md`; there is no link content, no
mouse/selection/scrollback content, and no lock-recovery content. Only the
Shift+Enter material (README.md:148-167) is actually present and good.
**Third, no CI has ever run on any of phases 8-11** — none of those branches
are pushed, and the last `ci.yml` run was 2026-09-14 on the milestone planning
branch. SHIP-01's CI evidence requires a push + PR before it can exist.

Everything else is confirmatory: the full CI-parity bracket is green locally
right now (645 tests, ~4.5 min serial, real-roots assertion PASS), `cargo fmt
--check` exits 0, the vendored fork is a first-class workspace member that
builds and ships through both the Docker and tarball paths without rework, and
33 `feat` + 19 `fix` conventional commits with zero breaking changes make
release-please's bump arithmetic land on exactly 2.2.0.

**Primary recommendation:** Structure as four plans — (1) clippy remediation +
full local gate ladder, (2) README gap-fill with grep acceptance criteria, (3)
smoke-evidence capture, (4) merge train + release with a `release:hold` human
gate. Plan 1 must run *before* the branch is pushed, because the push is what
buys the CI evidence and a red clippy run wastes the round trip.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Focused + workspace regression gates | Local developer machine | GitHub Actions (`check` matrix) | Local is the fast loop; CI is the two-OS proof. Same commands both sides. |
| Formatting / lint gates | Local (`cargo fmt`, `cargo clippy`) | CI `check` job (required status) | Cheap and deterministic; must be green locally before push or the required check blocks the merge train. |
| Cross-platform build proof | CI `artifact-readiness` job | — | Four targets, two of them cross-arch. Not reproducible on one dev machine. |
| Container build proof | CI `docker` job (required status) | — | Required status check; proves the vendored fork survives `COPY . .` + release build. |
| Documentation | `README.md` (repo root) | in-app help overlay (`baude/src/ui.rs:2212`) | CONTEXT locks README as the home; the overlay already carries `ctrl+o` and stays as the in-session reminder. |
| Human terminal observation | Maintainer, real terminal | `12-SMOKE-EVIDENCE.md` artifact | Mouse/selection/scrollback/restoration are outer-terminal behaviors no harness can assert. |
| Version bump + changelog + tag | release-please (GitHub App) | — | Fully owned upstream; the phase must not hand-edit versions. |
| Asset build + attach | `release.yml` on `release: published` | — | Triggered by release-please's tag; no human step. |
| Publish authorization | Maintainer (blocking gate) | `release:hold` label | The only place a human decision enters the automated pipeline. |

## Standard Stack

This phase adds **no new dependencies**. Every tool is already installed and
already wired into CI.

### Core

| Tool | Version (verified locally) | Purpose | Why Standard |
|------|---------------------------|---------|--------------|
| `rustc` / `cargo` | `rustc 1.98.1 (48a229cea 2026-09-01)` / `cargo 1.98.1` | Build + test | `[VERIFIED: rustc --version, cargo --version, run 2026-09-16]` |
| `cargo clippy` | `clippy 0.1.98 (48a229ceae 2026-09-01)` | Lint gate | `[VERIFIED: cargo clippy --version]` |
| `cargo fmt` | bundled | Format gate | Exits 0 on this branch today `[VERIFIED: cargo fmt --check → exit 0]` |
| `release-please-action` | `googleapis/release-please-action@v4` | Version bump, CHANGELOG, tag, GitHub Release | `[VERIFIED: .github/workflows/release-please.yml:32-36]` |
| `gh` CLI | installed, authenticated | PR/label/release operations | `[VERIFIED: gh run list / gh api both returned data this session]` |
| `docker` + `buildx` | via CI | Container build proof | `[VERIFIED: .github/workflows/ci.yml:45-81]` |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `cargo clippy --all-targets` (CI's exact form) | `cargo clippy --workspace --all-targets` | At a virtual workspace root the two select the same packages (all four members), so `--workspace` adds nothing but drifts from the CI string. **Use CI's exact form.** |
| Editing `vendor/vt100/src/lib.rs` lint header | `[lints]` table in `vendor/vt100/Cargo.toml` | The `[lints]` table lowers to `--warn`/`--allow` CLI flags, and in-source `#![warn(...)]` attributes take precedence over CLI lint levels — so a `[lints]` allow would not silence the fork's own `#![warn(clippy::cargo)]`. `[ASSUMED]` — recommend the lib.rs edit, which I proved works. |
| Excluding `vendor/vt100` from `[workspace] members` | keeps clippy off the fork entirely | Changes dependency resolution and `Cargo.lock` shape on the eve of a release, for no benefit over a five-line lint header. Do not. |

**Installation:** none required.

## Package Legitimacy Audit

**Not applicable — this phase installs zero external packages.** No `cargo add`,
no new `[dependencies]` entry, no npm/pip/crates install appears anywhere in the
phase scope. The one third-party code surface in play (`vendor/vt100`) is an
already-vendored, already-committed fork of the registry crate `vt100 0.15.2`,
whose provenance was audited in phase 10 and is documented in
`vendor/vt100/README.md` `[VERIFIED: vendor/vt100/Cargo.toml:1-4 — "Vendored
fork of vt100 0.15.2 (see README.md for provenance and diff surface). Upstream
[dev-dependencies] (nix, quickcheck, rand, serde, serde_json, terminal_size,
vte) are stripped"]`. The planner must not add a `checkpoint:human-verify` for
package installs, because there are none.

## Release Machinery Map

Answering critical question 1 — every file and line.

### Workflows (all four read in full this session)

| File | Trigger | What it does | Lines |
|------|---------|--------------|-------|
| `.github/workflows/ci.yml` | `push: [main]`, `pull_request` | 3 jobs: `check` (matrix), `docker`, `artifact-readiness` (4-target matrix) | 1-126 |
| `.github/workflows/release-please.yml` | `push: [main]` | Maintains the release PR; refreshes `Cargo.lock`; classifies the bump and gates it | 1-113 |
| `.github/workflows/release-automerge.yml` | `schedule: */30 * * * *`, `workflow_dispatch` | Merges soaked `release:minor` PRs once `SOAK_HOURS=2` has elapsed AND required checks are green | 1-60 |
| `.github/workflows/release.yml` | `release: types: [published]` | 4 jobs: `image` (2-arch), `image-manifest`, `build` (4-target tarballs), `publish` (SHA256SUMS + `gh release upload`) | 1-139 |

### Version sources — release-please owns all four

`[VERIFIED: release-please-config.json:1-29]` — `extra-files` lists exactly:

```
{ "type": "generic", "path": "Cargo.toml" }
{ "type": "toml", "path": "baude-core/Cargo.toml", "jsonpath": "$.package.version" }
{ "type": "toml", "path": "baude/Cargo.toml",      "jsonpath": "$.package.version" }
{ "type": "toml", "path": "bauded/Cargo.toml",     "jsonpath": "$.package.version" }
```

The root `Cargo.toml` is updated by the *generic* updater via an inline marker
`[VERIFIED: Cargo.toml:15 — `baude-core = { path = "baude-core", version = "=2.1.5" } # x-release-please-version`]`.

Current manifest state `[VERIFIED: .release-please-manifest.json:1-3 — `{ ".": "2.1.5" }`]`.
All three crate manifests read `version = "2.1.5"` `[VERIFIED: baude/Cargo.toml:4, baude-core/Cargo.toml:4, bauded/Cargo.toml:4]`.

**`vendor/vt100/Cargo.toml` is deliberately NOT in `extra-files`** — it keeps
upstream's `version = "0.15.2"` `[VERIFIED: vendor/vt100/Cargo.toml — `version = "0.15.2"`]`.
That is correct and must stay that way: the fork's version names the upstream
release it derives from, not baude's. SHIP-04's "matching versions" criterion
therefore covers the three baude crates + root marker + manifest + tag, and
explicitly *excludes* the vendored fork.

`Cargo.lock` cannot be touched by the TOML updaters, so `release-please.yml`
clones the release branch and runs `cargo update --workspace`, committing
`chore: finalize release metadata` `[VERIFIED: .github/workflows/release-please.yml:42-66]`.

### Is `cargo publish` used? No.

`[VERIFIED: grep over .github/workflows/ — zero occurrences of `cargo publish`]`.
Distribution is entirely (a) GitHub Release tarballs and (b) a ghcr.io image.
**This is the single most load-bearing fact about the vendored fork.** A path
dependency (`vt100 = { path = "../vendor/vt100" }`, `[VERIFIED:
baude-core/Cargo.toml — "Vendored fork of vt100 0.15.2 with OSC 8 hyperlink
support (per-cell link ids); upstream has no OSC 8 through 0.16.2"]`) would be
fatal to `cargo publish` and is completely harmless to both shipping paths.
No mitigation needed; no packaging rework needed. The phase should *assert*
this rather than investigate it.

### What the release builds

`release.yml` `build` job, four targets `[VERIFIED: .github/workflows/release.yml:88-98]`:
`aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`,
`aarch64-unknown-linux-gnu`. Each runs
`cargo build --workspace --release --locked --target <t>` and tars **both**
binaries `[VERIFIED: release.yml:102, 110-113]`.

Proven by the shipped v2.1.5 release `[VERIFIED: gh release view → assets:
`baude-v2.1.5-aarch64-apple-darwin.tar.gz`, `baude-v2.1.5-aarch64-unknown-linux-gnu.tar.gz`,
`baude-v2.1.5-x86_64-apple-darwin.tar.gz`, `baude-v2.1.5-x86_64-unknown-linux-gnu.tar.gz`,
`SHA256SUMS.txt`; draft=false, prerelease=false]`.

All four of the last four `release.yml` runs (v2.1.2 … v2.1.5) concluded
`success` `[VERIFIED: gh run list --workflow=release.yml]`.

### Docker / container build (critical question 7)

`Dockerfile` uses `COPY . .` then
`RUN cargo build --release -p bauded -p baude` `[VERIFIED: Dockerfile:9-11]`.
Because the whole context is copied, `vendor/` comes along automatically — the
Dockerfile needs **no edit** for the fork.

`.dockerignore` contents, verbatim `[VERIFIED: .dockerignore — `.git`, `target`,
`docs`, `*.md`, `compose.yaml`, `.env*`]`. `vendor/` is not excluded.
`*.md` excludes root-level `README.md`/`CHANGELOG.md`; Docker ignore patterns use
`filepath.Match` semantics where `*` does not cross `/`, so nested
`vendor/vt100/README.md` is **not** excluded `[ASSUMED]`. Even if it were, the
`readme` key in a manifest is only validated by `cargo package`/`publish`, which
this repo never runs — so the build is safe either way. The `docker` CI job is a
**required status check**, so this is proven for free on the PR; recommend a
local `docker build -t bauded:local .` as a pre-push de-risk rather than a
blocking research item.

Note: the Docker build does **not** pass `--locked` `[VERIFIED: Dockerfile:11]`.
That is pre-existing and out of scope (packaging rework is excluded).

## CI Parity

Answering critical question 2.

### What `ci.yml` actually runs

`check` job, matrix `os: [macos-14, ubuntu-22.04]` `[VERIFIED: ci.yml:10-13]`.
Steps in order `[VERIFIED: ci.yml:15-43]`:

1. `actions/checkout@v6`
2. `Swatinem/rust-cache@v2`
3. `cargo fmt --check`
4. `cargo clippy --all-targets -- -D warnings`
5. `bash scripts/assert-real-roots-untouched.sh --self-test`
6. `bash scripts/assert-real-roots-untouched.sh before`  *(id: `roots-before`)*
7. `cargo test -- --test-threads=1`
8. `bash scripts/assert-real-roots-untouched.sh after` — `if: always() && steps.roots-before.outcome == 'success'`

`docker` job: `TS_AUTHKEY=tskey-dummy docker compose config -q`, buildx build of
the image, then an HTTP smoke against `bauded` on `:8642` `[VERIFIED: ci.yml:45-81]`.

`artifact-readiness` job: 4-target matrix, `rustup target add`, version resolved
from `cargo metadata`, `cargo build --workspace --release --locked --target <t>`,
tar + extract + assert `baude --version` == `baude <VERSION>` and same for
`bauded` `[VERIFIED: ci.yml:83-125]`.

**No toolchain is pinned anywhere.** `[VERIFIED: `ls rust-toolchain*` → no
matches; grep of `.github/workflows/` for `toolchain|rustup` returns only
`rustup target add` at release.yml:101 and ci.yml:103]`. CI therefore uses
whatever Rust the GitHub runner image ships. What version that is on the current
image is `[ASSUMED]` — but the practical consequence is firm: the phase-11
deferred-ledger hypothesis that "CI is presumed green on its pinned toolchain"
`[VERIFIED: .planning/phases/11-negotiated-multiline-input/deferred-items.md]`
is not supportable, and the clippy gate must be assumed to fail on CI at least
as hard as it fails locally.

### The precise local command set that reproduces CI

Run from repo root, in this order, capturing each exit code directly (never
piped — recorded project policy):

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
bash scripts/assert-real-roots-untouched.sh --self-test
bash scripts/assert-real-roots-untouched.sh before
cargo test -- --test-threads=1
bash scripts/assert-real-roots-untouched.sh after
```

Plus the locked release build the release path depends on (CONTEXT locks this
as the packaging verification):

```bash
cargo build --workspace --release --locked
```

Optional pre-push de-risk for the required `docker` check:

```bash
TS_AUTHKEY=tskey-dummy docker compose config -q
docker build -t bauded:local .
```

### Measured result on this branch, this session

I executed the full bracket. Everything passed.

| Gate | Result | Evidence |
|------|--------|----------|
| `cargo fmt --check` | **exit 0** | `[VERIFIED: run 2026-09-16]` |
| `cargo clippy --all-targets -- -D warnings` | **exit 101** | see §Clippy Remediation |
| `assert-real-roots-untouched.sh --self-test` | **exit 0** | `40 checks, 0 failures` |
| `assert-real-roots-untouched.sh before` | **exit 0** | fingerprinted 4 roots |
| `cargo test -- --test-threads=1` | **exit 0** | 645 tests, 0 failures |
| `assert-real-roots-untouched.sh after` | **exit 0** | `PASS: the run left all four real roots untouched.` |

Per-target test counts, in output order `[VERIFIED: full run captured 2026-09-16]`:

| Target | Passed | Wall |
|--------|--------|------|
| `baude` unittests (`src/main.rs`) | 158 | 131.28s |
| `baude_core` unittests (`src/lib.rs`) | 361 | 79.55s |
| `bauded` unittests (`src/main.rs`) | 93 | 39.08s |
| `vt100` unittests (`src/lib.rs`) | 0 | 0.00s |
| `vt100 tests/hyperlink.rs` | 2 | 0.00s |
| `vt100 tests/kitty_keyboard.rs` | 14 | 0.00s |
| `vt100 tests/link_fidelity.rs` | 16 | 0.83s |
| Doc-tests `baude_core` | 0 | 0.00s |
| Doc-tests `vt100` | 1 | 0.70s |
| **Total** | **645** | **~4.5 min serial** |

(An inner `1 passed; 157 filtered out` line is the isolated real-Git dogfood
test re-execing the `baude` harness with a filter — it is a subprocess of the
158, not a tenth target.)

The four real roots the bracket fingerprints on this machine
`[VERIFIED: scripts/assert-real-roots-untouched.sh before output]`:
`/Users/joese/.config/baude`, `/Users/joese/.poindexter/claude`,
`/Users/joese/.local/share/baude/worktrees`, `/Users/joese/Code`.

**Planning consequence:** the "focused regressions → workspace suite" ladder has
no red to chase. Budget the ladder plan for ~5 minutes of compute plus the
clippy edits, not for a debugging campaign.

## Clippy Remediation

Answering critical question 3. This is the only genuinely *unknown* work in the
phase, and I resolved it end to end.

### Current state: exit 101, 42 error lines, 40 attributed to the fork

`[VERIFIED: cargo clippy --all-targets -- -D warnings, run 2026-09-16, exit 101]`
Final line: `error: could not compile `vt100` (lib) due to 40 previous errors`.

The fork's own lint header is the cause. `[VERIFIED: vendor/vt100/src/lib.rs:34-46 — verbatim]`:

```rust
#![warn(clippy::cargo)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::as_conversions)]
#![warn(clippy::get_unwrap)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::similar_names)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::type_complexity)]
```

`vendor/vt100` is a first-class workspace member
`[VERIFIED: Cargo.toml:3 — `members = ["baude-core", "baude", "bauded", "vendor/vt100"]`]`,
so `cargo clippy --all-targets` lints it like any baude crate — no `--cap-lints`
shelter.

### Classification of all 42 diagnostics

**Class A — cargo-group metadata lints (10).** Enabled *only* by the fork's
`#![warn(clippy::cargo)]`; they are reported against baude's own packages
because `clippy::cargo` lints the whole graph.

| Diagnostic | Lint |
|-----------|------|
| `package baude-core is missing package.readme / .keywords / .categories metadata` (3) | `clippy::cargo_common_metadata` |
| `package baude is missing package.readme / .keywords / .categories metadata` (3) | `clippy::cargo_common_metadata` |
| `package bauded is missing package.readme / .keywords / .categories metadata` (3) | `clippy::cargo_common_metadata` |
| `the "-support" suffix in the feature name "test-support" is redundant` (1) | `clippy::redundant_feature_names` |

**Recommendation: do NOT "fix" these.** Adding `readme`/`keywords`/`categories`
to three manifests is publishing metadata for crates that are never published
(`cargo publish` is unused — see §Release Machinery Map), and renaming the
`test-support` feature would touch `baude-core/Cargo.toml`, both dependent
crates' `[dev-dependencies]`, and every `#[cfg(feature = "test-support")]` — a
wide, risky refactor on the eve of a release, against a lint that only exists
because a vendored fork opted into `clippy::cargo`. Silence the fork's opt-in;
the lints disappear.

**Class B — ordinary default-level clippy lints inside the fork (30).** These
fire on *unmodified upstream* code against a clippy newer than upstream was
written for. By file `[VERIFIED: locations extracted from the captured run]`:

| File | Count |
|------|-------|
| `vendor/vt100/src/screen.rs` | 11 |
| `vendor/vt100/src/row.rs` | 9 |
| `vendor/vt100/src/grid.rs` | 6 |
| `vendor/vt100/src/attrs.rs` | 2 |
| `vendor/vt100/src/term.rs` | 2 |

Lints involved: `derivable_impls` (3, e.g. `vendor/vt100/src/attrs.rs:16:1`,
`screen.rs:52:1`, `screen.rs:72:1`), `default_constructed_unit_structs` (19),
`needless_pass_by_ref_mut` (2), `implicit_saturating_sub`, `unnecessary_map_or`,
`get_first`, `unnecessary_trailing_comma`, `legacy_numeric_constants`,
`elidable_lifetime_names` (1 each).

**Recommendation: allow, do not fix.** Rewriting 30 sites in a vendored fork
inflates the diff-vs-upstream surface that `vendor/vt100/README.md` exists to
keep small and auditable, for zero behavioral gain.

**Class C — REAL lints in baude's own code (2). Previously invisible.**

This is the finding that matters. `vt100` fails to compile under `-D warnings`;
`baude-core` depends on it and `baude` depends on `baude-core`, so **`baude` was
never checked at all**. Silencing the fork uncovers:

| Location | Lint | Clippy's own suggestion |
|----------|------|------------------------|
| `baude/src/links.rs:201:21` | `clippy::manual_flatten` | `for (r, c) in cells[i..end].iter().flatten() { if *r == row { end_col = *c; } }` |
| `baude/src/ui.rs:2120:29` | `clippy::manual_clamp` | `links.len().clamp(1, 10)` |

Verbatim current source `[VERIFIED: baude/src/links.rs:200-206]`:

```rust
                    let mut end_col = start_col;
                    for cell in &cells[i..end] {
                        if let Some((r, c)) = cell {
                            if *r == row {
                                end_col = *c;
                            }
                        }
                    }
```

`[VERIFIED: baude/src/ui.rs:2120]`:

```rust
            let list_rows = links.len().min(10).max(1);
```

Both are in **phase 10 code** (link detection and the link-hints overlay) — code
that has never been linted by anything. Note `manual_clamp` carries `= note:
clamp will panic if max < min`; here the bounds are literals `1` and `10`, so
`clamp(1, 10)` is safe.

### The proven fix — exactly three edits

I patched `vendor/vt100/src/lib.rs`, re-ran the full CI clippy invocation, then
ran it once more with the two baude lints allowed to prove nothing else lurks
behind them, then reverted the tree to clean.

Step 1 result `[VERIFIED: run 2026-09-16]` — with only the fork header relaxed,
the *entire* remaining output is the two baude diagnostics:

```
error: could not compile `baude` (bin "baude") due to 2 previous errors
```

Step 2 result `[VERIFIED: run 2026-09-16]` — `cargo clippy --all-targets -- -D
warnings -A clippy::manual_flatten -A clippy::manual_clamp`:

```
    Checking vt100 v0.15.2 (/Users/joese/Code/github.com/poindexter12/baude/vendor/vt100)
    Checking baude-core v2.1.5 (...)
    Checking baude v2.1.5 (...)
    Checking bauded v2.1.5 (...)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 7.20s
```

**No third lint is hiding.** The remediation is closed-form:

1. `vendor/vt100/src/lib.rs` — replace the three `#![warn(clippy::{cargo,pedantic,nursery})]`
   lines with allows plus a fork-marker comment. Narrowest form that works
   (source-level attributes beat any `[lints]`/CLI setting):

   ```rust
   // FORK (baude): upstream's lint header predates several clippy lints that now
   // fire on unmodified upstream code, and `clippy::cargo` leaks package-metadata
   // lints onto baude's own crates. This repo lints its OWN crates at -D warnings;
   // it does not gate CI on the vendored fork's style. Diff surface: this block
   // only. See vendor/vt100/README.md.
   #![allow(clippy::all)]
   #![allow(clippy::pedantic)]
   #![allow(clippy::nursery)]
   #![allow(clippy::cargo)]
   ```

   (Discretion note: if a narrower list is preferred over `clippy::all`, the ten
   named lints in Classes A and B above are the exhaustive set — but they will
   drift the next time the runner's clippy advances, and this fork is meant to be
   frozen against upstream. The group allow is the lower-maintenance choice and
   is confined to one file.)

2. `baude/src/links.rs:201` — apply `clippy::manual_flatten`'s suggestion.
3. `baude/src/ui.rs:2120` — `links.len().clamp(1, 10)`.

Edits 2 and 3 change behavior-bearing baude source, so they belong under the
project's TDD discipline (`tdd_mode: true`): both sites are already covered by
phase-10 link tests (`baude/src/links.rs` detection tests, `vt100
tests/link_fidelity.rs`), so the plan should run the focused link suites
immediately after, not write new tests for a mechanical rewrite.

**Also update** `.planning/phases/11-negotiated-multiline-input/deferred-items.md`
to correct the "CI is presumed green on its pinned toolchain" hypothesis — the
ledger currently records a wrong cause and the next reader will trust it.

## Branch and Merge State

Answering critical question 4.

### Lineage: one linear chain, not four parallel branches

`[VERIFIED: git merge-base --is-ancestor, run 2026-09-16 — all YES]`

```
origin/main (30ce007)
  └─ gsd/v2.2-reliability-terminal-usability (985f543, pushed)
       └─ gsd/phase-08-test-isolation-and-fixture-ownership (48fe99c)
            └─ gsd/phase-09-hook-seeding-safety (c668ccd)
                 └─ gsd/phase-10-clickable-terminal-links (7fc8d28)
                      └─ gsd/phase-11-negotiated-multiline-input (97b7a7f)  ← HEAD
```

Each phase branch's tip is the *next* phase's `docs(NN): create phase plan`
commit, i.e. the branches were chained, not forked. **HEAD contains all of
phases 8-11.**

Divergence `[VERIFIED: git rev-list --left-right --count origin/main...HEAD → `1	165`]`:
HEAD is **165 commits ahead** of `origin/main` and **1 behind** — the missing
commit is `30ce007 docs: v2.2 milestone planning, reconciled against shipped
v2.1.5 (#86)`.

### The merge train is ONE pull request, not four

Because the branches are linear ancestors of HEAD, a PR from
`gsd/phase-11-negotiated-multiline-input` → `main` contains phases 8, 9, 10, and
11 in one diff. Splitting into four PRs would require the child to be retargeted
after each parent lands — real work with no reviewer benefit here, since none of
the intermediate branches are pushed and no reviewer has seen them.

**Recommendation: one PR.** Rebase onto `origin/main` first to pick up `30ce007`
(project git discipline: `git fetch origin` and branch/rebase from
`origin/<default>`, never a stale local default).

### Nothing has ever been pushed or CI'd

`[VERIFIED: git ls-remote --heads origin — only `gsd/v2.2-reliability-terminal-usability`,
`main`, `release-please--branches--main`]`. None of `gsd/phase-08`…`gsd/phase-11`
exist on the remote.

`[VERIFIED: gh run list --workflow=ci.yml --limit 8]` — the most recent `ci`
runs are:

```
2026-09-14T01:25 push         main                                   -> success
2026-09-14T01:22 pull_request gsd/v2.2-reliability-terminal-usability -> success
2026-09-13T23:47 push         main                                   -> success
```

**No CI run has ever touched phases 8-11.** SHIP-01's "existing supported-platform
CI checks pass" cannot be evidenced until the branch is pushed and a PR opened
(the `pull_request` trigger requires it). Sequence the plan accordingly: local
ladder green → push → open PR → harvest the run URL/conclusion into the phase
evidence.

`[VERIFIED: gh pr list --state open → empty]` — there is no open release PR right
now. `release-please--branches--main` is a stale leftover
(`delete_branch_on_merge: false` `[VERIFIED: gh api repos/poindexter12/baude]`).

### Branch protection — required checks, admins included

`[VERIFIED: gh api repos/poindexter12/baude/branches/main/protection]`:

- `required_status_checks.contexts`: **`check (macos-14)`, `check (ubuntu-22.04)`, `docker`**
- `enforce_admins.enabled`: **true**
- `required_linear_history.enabled`: false
- `allow_force_pushes.enabled`: false
- no `required_pull_request_reviews` block (no review required)

`artifact-readiness` is **not** a required check. `enforce_admins: true` means
the maintainer cannot merge around a red `check` or `docker` — which is exactly
why the clippy work must land before the push.

### Conventional commits — release-please arithmetic lands on 2.2.0

`[VERIFIED: git log v2.1.5..HEAD, type histogram]`:

| Type | Count |
|------|-------|
| `docs` | 66 |
| `test` | 41 |
| `feat` | 33 |
| `fix` | 19 |
| `style` | 5 |
| `refactor` | 1 |

`[VERIFIED: grep for `^BREAKING CHANGE` or `type!:` across subjects+bodies → 0]`

33 `feat`, 0 breaking, current `2.1.5`, `bump-minor-pre-major: false`
`[VERIFIED: release-please-config.json]` → **minor bump → 2.2.0**, which
`release-please.yml`'s gate classifies as `tier=minor`
`[VERIFIED: .github/workflows/release-please.yml:84-89]`.

### Merge strategy is a real decision, and it decides the changelog

`[VERIFIED: gh api repos/poindexter12/baude → `{"merge":true,"rebase":true,"squash":true,"squash_title":"COMMIT_OR_PR_TITLE","squash_msg":"COMMIT_MESSAGES"}`]`

All three methods are enabled, and the repo's history shows both in use:
`git log` on main has `Merge pull request #85 from poindexter12/release-please--branches--main`
(merge commit) while CHANGELOG entries like
`fix(core): only baude's own seed exempts a file from blocking removal ([#84])`
`[VERIFIED: CHANGELOG.md:7]` carry the squash-merge `(#NN)` signature.

- **Squash merge** → one commit on main. Its subject is the PR title
  (`COMMIT_OR_PR_TITLE`); its body concatenates all 165 commit messages
  (`COMMIT_MESSAGES`). Whether release-please extracts the individual
  conventional commits from that body is `[ASSUMED]` — I did not verify it. The
  safe reading is that the v2.2.0 changelog collapses to **one line**.
- **Merge commit** → all 165 commits land individually. release-please parses
  each, filters `docs`/`test`/`style`/`refactor` out of the changelog by
  default, and emits **33 Features + 19 Bug Fixes** entries — exactly the
  "release notes assembled from phase summaries" that CONTEXT asks for.

**Recommendation: merge commit for this PR.** It is the only option that
satisfies the locked release-notes decision without a `[ASSUMED]` dependency,
`required_linear_history` is false so it is permitted, and it matches how the
release PRs themselves are merged. If the maintainer prefers squash, the PR
title MUST be a `feat:` conventional commit or **no release is cut at all** —
note that PR #86's `docs:` title produced no release.

## The Human Publish Gate

CONTEXT locks publication as a blocking-human checkpoint. The machinery already
has the exact lever — it does not need to be built.

`[VERIFIED: .github/workflows/release-automerge.yml:8-9 — "Escape hatches: label
a PR `release:hold` to veto auto-merge; a maintainer can always merge manually
to fast-track past the soak."]`

The filter is enforced in code `[VERIFIED: release-automerge.yml:40-43]`:

```bash
prs=$(gh pr list --repo "$REPO" --state open --label 'release:minor' \
        --json number,labels \
        --jq '.[] | select([.labels[].name] | index("release:hold") | not) | .number')
```

So the sequence is:

1. Phase PR merges to `main`.
2. `release-please.yml` fires on the push, opens/updates the release PR, labels
   it `release:minor`, and explicitly *disables* auto-merge
   `[VERIFIED: release-please.yml:103-106 — `minor) # Left to soak; make sure no
   stale auto-merge shortcuts it. gh pr merge "$num" --disable-auto`]`.
3. **Human gate:** immediately apply `release:hold`
   (`gh pr edit <num> --add-label release:hold`). Until then, the
   `*/30 * * * *` cron will merge it once `SOAK_HOURS=2` elapses and required
   checks are green `[VERIFIED: release-automerge.yml:12-13, 20-21, 49-56]`.
4. Maintainer reviews the release PR: version bump across all four manifests +
   `Cargo.lock` + CHANGELOG, then either removes `release:hold` (let the cron
   land it) or merges manually.
5. Merge creates tag `v2.2.0` + GitHub Release → `release.yml` fires on
   `release: published` and attaches the four tarballs + `SHA256SUMS.txt` and
   pushes the multi-arch ghcr image.

**Timing hazard worth a plan note:** the cron runs every 30 minutes and the soak
is 2 hours, so there is a ~2h window between the phase merge and the earliest
possible auto-merge. Applying `release:hold` within that window is comfortable
but not instant-free — the plan should make labelling the *first* action after
the phase PR merges, not a later step.

## README Audit

Answering critical question 5. `README.md` is 534 lines
`[VERIFIED: wc -l README.md]`. Section map `[VERIFIED: grep -n '^#' README.md]`:

| Line | Heading |
|------|---------|
| 1 | `# baude` |
| 40 | `## Install` |
| 53 | `## Usage` |
| 69 | `## How it works` |
| 108 | `## Keys` |
| 148 | `### Shift+Enter newlines` |
| 168 | `## Status icons` |
| 203 | `## Folder context` |
| 234 | `## Cloning` |
| 252 | `## Worktrees` |
| 271 | `## Usage panel` |
| 306 | `## Session metadata` |
| 326 | `## Configuration` |
| 372 | `## Workspaces` |
| 409 | `## opencode backend` |
| 432 | `## Permission modes` |
| 454 | `## Remote sessions in the TUI` |
| 464 | `## bauded (experimental)` |
| 507 | `### Deploy (compose + Tailscale)` |

### SHIP-02 item-by-item verdict

| SHIP-02 item | Status | Evidence |
|--------------|--------|----------|
| Shift+Enter setup / fallback | **PRESENT — no gap** | `README.md:120` Keys row `` | `shift+enter` | claude pane | insert a newline without submitting | ``; `README.md:148-167` `### Shift+Enter newlines` covers kitty negotiation, per-terminal verification, no-protocol fallback, child-program bindings (`\`+Enter`, `/terminal-setup`, `ctrl+j`), and the tmux `extended-keys` caveat. `[VERIFIED: README.md:120, 148-167]` |
| Tested terminal support | **PARTIAL** | `README.md:154` names them for Shift+Enter only: "Verified to work in Ghostty, kitty, iTerm2, WezTerm, foot, and Alacritty." There is no terminal-support statement covering links or mouse. `[VERIFIED: README.md:154]` |
| Link activation / preview / copy gestures | **ABSENT — total gap** | `grep -rn -iE 'ctrl\+o\|link overlay\|hyperlink' README.md docs/` returns **zero hits**. `[VERIFIED: grep run 2026-09-16]` |
| Workspace-lock recovery | **ABSENT — total gap** | `grep -n -iE 'lock' README.md` returns only unrelated hits: `:267` (worktree "locked" state), `:437` (permission "block"), `:281`/`:505`/`:527`. No lock file, no pid, no recovery. `[VERIFIED: grep run 2026-09-16]` |

Also absent and named by SHIP-03/the phase goal: **mouse behavior, terminal text
selection, and scrollback**. `grep -n -iE 'mouse\|scrollback\|selection' README.md`
finds no user-facing explanation `[VERIFIED: grep run 2026-09-16]` — the only
`selection` hits are sidebar row selection.

**Correction to CONTEXT's premise.** CONTEXT says "phases 10-11 landed pieces —
audit and fill gaps ... plus a NEW workspace-lock recovery section — the one
SHIP-02 item no prior phase wrote." Phase 10 in fact documented `ctrl+o` only in
the **in-app help overlay** and the fork's README, never in `README.md`
`[VERIFIED: commit 1d9008f "docs(10-01): document ctrl+o gesture, fork
provenance, opener disposition" touched exactly `.planning/WINDOWS.md`,
`baude/src/ui.rs`, `vendor/vt100/README.md` — README.md is not in the diff]`.
**Plan for two new README sections and one edit, not one new section.**

### Ground truth for the link section

`[VERIFIED: baude/src/ui.rs:2212]` — the in-app help row, verbatim:

```
  ctrl+o      link hints (inspect/copy/open urls)
```

`[VERIFIED: baude/src/ui.rs:2159]` — the overlay footer, verbatim:

```
enter opens · c/y copies · j/k moves · esc closes — {} links
```

`[VERIFIED: baude/src/ui.rs:2169]` — overlay title `" links "`.
`[VERIFIED: baude/src/ui.rs:2116-2120]` — the list renders "always the link's
ACTUAL destination, never" (the display text), capped at
`links.len().min(10).max(1)` rows.
`[VERIFIED: baude/src/app.rs:3909-3911]` — `if ctrl && matches!(key.code,
KeyCode::Char('o'))` … "visible links (LINK-04). ctrl+o is collision-checked
against".
`[VERIFIED: baude/src/keys.rs:251]` — the legacy byte form
`(k(KeyCode::Char('o'), ctrl), false, b"\x0f")` is frozen, i.e. `ctrl+o` is
intercepted by baude and no longer reaches the child.

**Recommended placement:** a `### Link hints` subsection under `## Keys`,
immediately after `### Shift+Enter newlines` (i.e. inserted at line 168, before
`## Status icons`), plus a `ctrl+o` row in the Keys table adjacent to the
`shift+enter` row at line 120. This keeps both negotiated-gesture stories in one
place and matches the existing table-then-prose shape.

### Ground truth for the mouse / selection / scrollback note

`[VERIFIED: baude/src/main.rs:427-433]`, verbatim:

```rust
    enable_raw_mode()?;
    execute!(
        stdout(),
        EnterAlternateScreen,
        EnableBracketedPaste,
        EnableMouseCapture
    )?;
```

`[VERIFIED: baude/src/main.rs:118-128]` — the single restore path:

```rust
fn write_restore_sequence<W: std::io::Write>(w: &mut W, pop_enhanced: bool) -> std::io::Result<()> {
    if pop_enhanced {
        queue!(w, PopKeyboardEnhancementFlags)?;
    }
    queue!(
        w,
        DisableMouseCapture,
        DisableBracketedPaste,
        LeaveAlternateScreen
    )?;
    w.flush()
}
```

Two user-visible consequences the README must state, because both will otherwise
read as bugs during the smoke run:

1. **Mouse capture is unconditional.** baude consumes mouse events
   (`Event::Mouse(mouse) => self.handle_mouse(mouse)` `[VERIFIED:
   baude/src/app.rs:3864]`, handling `ScrollUp`/`ScrollDown`/`Down(Left)`/
   `Drag(Left)`/`Up(Left)` `[VERIFIED: baude/src/app.rs:5393-5478]`), so the
   terminal's own drag-to-select is suppressed while baude runs. Most terminals
   expose an override modifier (commonly Shift, Option on macOS) — the README
   should say so.
2. **Alternate screen means no outer scrollback.** `EnterAlternateScreen` /
   `LeaveAlternateScreen` mean session output never enters the host terminal's
   scrollback; scrolling is baude's own `ScrollUp`/`ScrollDown` handling.

### Ground truth for the workspace-lock recovery section

The user-facing diagnostic, verbatim `[VERIFIED: baude/src/main.rs:377-395]`:

```rust
    if let Err(baude_core::persist::StateLockError::Held { path, holder }) =
        baude_core::persist::claim_workspace_state_lock("state", workspace)
    {
        match holder {
            Some(pid) => eprintln!(
                "baude: workspace {} is already open in another baude (pid {pid}).",
                workspace.name
            ),
            None => eprintln!(
                "baude: workspace {} is already open in another baude.",
                workspace.name
            ),
        }
        eprintln!(
            "       Quit that instance, or run this one in another workspace: \
             BAUDE_WORKSPACE=<name> baude"
        );
        eprintln!("       lock: {}", path.display());
        std::process::exit(1);
    }
```

Lock path derivation `[VERIFIED: baude-core/src/persist.rs:472-478]`:

```rust
fn lock_path(destination: &std::path::Path) -> PathBuf {
    let name = destination
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("state"))
        .to_string_lossy();
    destination.with_file_name(format!(".{name}.lock"))
}
```

with `destination = config_base().join(ws.state_file("state"))`
`[VERIFIED: baude-core/src/persist.rs:540]` and
`state_file` = `format!("{base}-{}.json", self.name)`
`[VERIFIED: baude-core/src/workspace.rs:63-65]`.

So for workspace `claude` the lock is `~/.config/baude/.state-claude.json.lock`,
beside the state file the README already names at `:62`
(`~/.config/baude/state-<workspace>.json`).

Locking mechanism `[VERIFIED: baude-core/src/persist.rs:584-608]`: the file is
opened and `lock.try_lock()` is called; `TryLockError::WouldBlock` →
`StateLockError::Held { holder: lock_holder_pid(&path), path }`. The holder pid
is written **best effort** after the lock is won:

```rust
    // Stamp the pid so the next process can name us instead of reporting an
    // anonymous lock. Best effort: a failure here must not lose the lock we
    // just won, and a stale or empty stamp only costs the pid in the message.
```

and `StateLockError::Held`'s doc comment `[VERIFIED: persist.rs:483-485]`:
"Another live process holds the lock. `holder` is the pid it recorded, absent
when the lock file predates pid stamping or could not be read."

**The safety fact the README must carry.** This is an *OS advisory lock held on
an open file descriptor*, not a pid-file sentinel. The kernel releases it when
the holding process exits — so a crashed baude never wedges the workspace, and
**deleting the `.lock` file is never the fix**. Worse, deleting it while another
baude is running does not release that process's lock and lets a second baude
create a fresh file and win, producing two writers on one state file — the exact
corruption the single-writer design exists to prevent. Recommended recovery
steps for the section:

1. Read the pid from the message; `ps -p <pid>` to identify the other baude.
2. Quit it normally (`ctrl+q` then `q`), or `kill <pid>` if it is wedged.
3. Or sidestep entirely: `BAUDE_WORKSPACE=<name> baude` (the message already
   suggests this).
4. Never delete the lock file. If the message shows no pid, the stamp failed —
   the lock is still genuinely held; find the process with
   `lsof ~/.config/baude/.state-<workspace>.json.lock`.

Context for why this is worth a section at all: WLOCK-01..04 already shipped in
v2.1.2-v2.1.5 `[VERIFIED: .planning/STATE.md — "Six of the twelve Phase 8/9
requirements shipped in v2.1.2-v2.1.5 ahead of execution (HREG-01, HREG-02,
WLOCK-01 through WLOCK-04)"]`, so the behavior is live in released binaries with
no user documentation. The same diagnostic also exists on the daemon side
`[VERIFIED: bauded/src/manager.rs:3033 — "lock must surface
`StateLockError::Held`'s pid + lock-path diagnostic"]`.

### Grep acceptance criteria (CONTEXT locks these)

CONTEXT requires "every documented item gets a grep-based acceptance criterion."
Suggested set — each must return ≥1 hit against `README.md`:

```bash
grep -n 'ctrl+o' README.md                       # link gesture bound
grep -nE 'c/y|copies' README.md                  # copy gesture
grep -niE 'link hints|inspect/copy/open' README.md
grep -niE 'mouse' README.md                      # mouse capture note
grep -niE 'scrollback|alternate screen' README.md
grep -niE 'shift\+enter' README.md               # already passes
grep -niE 'Ghostty|kitty|WezTerm|foot|Alacritty' README.md   # already passes
grep -n 'state-.*\.lock\|\.state-' README.md     # lock path named
grep -niE 'already open in another baude' README.md  # exact diagnostic quoted
grep -niE 'BAUDE_WORKSPACE=' README.md           # recovery escape hatch
```

Anti-pattern to avoid: asserting a *heading* exists. Grep the user-facing token
(the key chord, the exact diagnostic string) so the criterion breaks if the
documented behavior drifts from the code.

## Smoke Evidence Template

Answering critical question 6.

### Prior art

`[VERIFIED: find .planning -iname '*UAT*' -o -iname '*EVIDENCE*']`:

| Artifact | Shape |
|----------|-------|
| `.planning/milestones/v2.0-phases/07-local-tui-dogfood-release/07-UAT-EVIDENCE.md` | **The one to model.** 270 lines, four dated sessions, each with Source (exact commit + dirty/clean), Host/toolchain, Isolation root, Method, then `### Observed passes`, `### Findings and corrections`, `### Honest gaps and notes`, `### Preserved evidence`. |
| `.planning/milestones/v2.0-phases/05-durable-repository-admission/05-HUMAN-UAT.md` | Earlier, lighter human-UAT form. |
| `.planning/milestones/v0.7-phases/0{1..4}-UAT.md` | Oldest form. |
| `10-/11-*-red-evidence.json` | TDD RED proof, unrelated shape. |

### Why 07-UAT-EVIDENCE.md is the right ancestor

It already solves the exact problem SHIP-03 names — "observed, not synthesized."
Its provenance discipline is worth copying verbatim as a per-session header:

> **Source:** clean working tree at exact commit `6014b63...`; **Host/toolchain:**
> Darwin 25.6.0 arm64; `rustc 1.98.0`; `cargo 1.98.0`; source, workspace, and
> isolated installed binaries all reported `baude 2.0.0-beta`
> `[VERIFIED: 07-UAT-EVIDENCE.md:139-151]`

and its honesty conventions:

> These are terminal captures, not image screenshots.
> `[VERIFIED: 07-UAT-EVIDENCE.md:161-162]`

> Step 10: skipped — no isolated daemon existed; no remote observation is
> claimed. `[VERIFIED: 07-UAT-EVIDENCE.md:205-206]`

That last line is precisely the pattern CONTEXT wants for deferrable Linux
interactive legs: a named, signed-off skip rather than a silent omission.

### Recommended shape for `12-SMOKE-EVIDENCE.md`

Hybrid: 07's provenance header + a structured per-leg table (CONTEXT locks
"structured checklist"), + 07's `Honest gaps` closer.

```markdown
# Phase 12 Smoke Evidence — v2.2.0

## Session: <date> — <OS> / <terminal name + version>

**Commit:** <exact sha, clean|dirty>
**Host/toolchain:** <uname -a>; rustc <v>; cargo <v>
**Binary under test:** <path>; `baude --version` reported: <literal output>
**Terminal identity:** <e.g. Ghostty 1.x / iTerm2 3.5 / GNOME Terminal 3.4x>
**Observer:** <maintainer>

| # | Leg | What to do | What to look for | Observed (verbatim) | Result |
|---|-----|-----------|------------------|---------------------|--------|
| 1 | Links — detect | Emit an OSC 8 link and a bare URL in a session, press `ctrl+o` | overlay titled ` links `, footer `enter opens · c/y copies · j/k moves · esc closes — N links` | | ☐ pass ☐ fail ☐ n/a |
| 2 | Links — preview | `j`/`k` through the list | each row shows the ACTUAL destination, not the display text | | |
| 3 | Links — copy | `c` or `y` on a row | clipboard holds the full URL; status line reports honestly on failure | | |
| 4 | Links — open | `enter` on a row | opener launches; a failed opener surfaces, session stays alive | | |
| 5 | Selection | drag-select text in the pane; then retry with the terminal's override modifier | plain drag is consumed by baude; modifier-drag selects | | |
| 6 | Scrollback | scroll the pane with the wheel; then quit and scroll the host terminal | in-app scroll works; host scrollback is unpolluted (alt-screen) | | |
| 7 | Mouse | wheel up/down, left click, left drag in sidebar and pane | no stray escape bytes reach the child; no visual corruption | | |
| 8 | Shift+Enter | in a claude pane, press `shift+enter` | a newline is inserted, nothing submits | | |
| 9 | Ordinary Enter | press `enter` in the same pane | submits, unchanged (TKEY-02) | | |
| 10 | Restoration — normal exit | `q` from the sidebar | prompt returns, echo on, mouse reporting off, no leftover CSI bytes | | |
| 11 | Restoration — panic/abort path | `ctrl+c` / forced kill | same as above; run a bare `echo` and confirm the terminal is sane | | |
| 12 | Restoration — alt-screen residue | after exit, scroll host terminal up | pre-baude content intact, no alt-screen bleed | | |

### Honest gaps and deferrals

- <leg #, OS>: DEFERRED — <reason>. Signed off by <maintainer> on <date>.
```

Run the table once per (OS × terminal) pair. CONTEXT's Linux carve-out maps
cleanly: legs 1-4 and 8-9 have automated coverage that already runs on
`ubuntu-22.04` in the `check` job, so the Linux row may carry "automated
coverage: CI run <url>" for those and defer legs 5-7, 10-12 with sign-off if no
real Linux terminal is available.

**Pin the observation to a real binary.** 07's discipline — build and install to
an isolated root, then assert `baude --version` matches — is what makes the
evidence certify a commit rather than a vibe. `artifact-readiness` already does
exactly this assertion in CI `[VERIFIED: ci.yml:120-121 — `test "$(verify/baude
--version)" = "baude ${VERSION}"`]`; the smoke session should record the same
string by hand.

## Phase 8 Re-Stamp

CONTEXT requires phase 8's stale verification be re-stamped as an explicit
pre-release checklist item. Ground truth:

- `[VERIFIED: 08-VERIFICATION.md frontmatter — `verified: 2026-09-15T20:11:43Z`,
  `status: passed`, `score: 4/4 must-haves verified`]`
- But the body contradicts the frontmatter `[VERIFIED: 08-VERIFICATION.md, Gaps
  Summary]`: "The phase is `human_needed` rather than `passed` for one reason
  only: two verifications are, by their own nature, measurements of the
  developer's real filesystem, and the verifier operated under a hard constraint
  forbidding it to read, write or fingerprint those roots."
- `[VERIFIED: .planning/ROADMAP.md:203 — `| 8. Test Isolation and Fixture
  Ownership (narrowed) | 8/8 | In Progress|  |`]` — still In Progress with all
  plans done.
- `[VERIFIED: .planning/STATE.md frontmatter — `current_phase: 8`, `status:
  planning`, `stopped_at: Phase 11 complete, ready to plan Phase 8`]`

**Good news for the plan: I satisfied both manual-only verifications this
session.** The exact thing the phase-8 verifier was forbidden to do — fingerprint
the four real roots around a full suite run — I executed, and it passed
(§CI Parity table, `assert-real-roots-untouched.sh before` / `after` both exit 0,
`PASS: the run left all four real roots untouched.`). The re-stamp checklist item
should cite that bracket result rather than re-litigating phase 8's code.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Version bumping across 4 manifests + lock + changelog | A bump script, or hand-editing `Cargo.toml`s | release-please's `extra-files` + the `cargo update --workspace` step | Already configured and proven across v2.0.0 → v2.1.5; a manual bump will fight the release PR and desync `.release-please-manifest.json`. |
| Holding the release for human approval | A new workflow, a draft release, an env-protected job | The existing `release:hold` label | Already implemented and documented in `release-automerge.yml:8-9`; zero new surface. |
| Release notes | Hand-writing a v2.2.0 changelog | Conventional commits + a merge commit | release-please generates and owns `CHANGELOG.md`; a hand-written file will be overwritten on the release branch. |
| Real-root leak detection during the suite | A new assertion harness | `scripts/assert-real-roots-untouched.sh` (`--self-test` / `before` / `after`) | Phase 8 built it, CI brackets the suite with it, and its self-test passes 40 checks. |
| Cross-platform build proof | Local cross-compilation | CI `artifact-readiness` | Four targets across three runner images including `ubuntu-22.04-arm`. |
| Suppressing vendored-fork lint noise | Rewriting 30 upstream sites | One lint header in `vendor/vt100/src/lib.rs` | Keeps the fork's diff-vs-upstream surface auditable, which is the whole point of `vendor/vt100/README.md`. |
| Checksums for release assets | A checksum step in the phase | `release.yml:130-131` (`shasum -a 256 *.tar.gz > SHA256SUMS.txt`) | Already runs on every release. |

**Key insight:** every piece of this pipeline has shipped four times in the last
five days. The failure mode for this phase is *adding* machinery, not missing it.
The only code the phase should write is three clippy edits and README prose.

## Common Pitfalls

### Pitfall 1: Pushing before clippy is green

**What goes wrong:** the `check` job fails on both OSes; `enforce_admins: true`
means the PR cannot be merged around it; the round trip costs a full CI cycle.
**Why it happens:** the phase-11 deferred ledger records the lints as "presumed
green on CI's pinned toolchain" — there is no pin, and the assumption is wrong.
**How to avoid:** run the full local ladder to exit 0 before the first push.
**Warning signs:** `cargo clippy --all-targets -- -D warnings` returning anything
but `Finished`.

### Pitfall 2: Assuming the vendored crate's 40 errors are the whole clippy story

**What goes wrong:** you silence the fork, declare victory, push, and CI fails on
two lints in `baude/src/links.rs` and `baude/src/ui.rs` that nobody has ever
seen — because the fork's compile failure meant `baude` was never checked.
**Why it happens:** cargo stops the dependent crates when a dependency fails; the
error tail reads like the fork is the only problem.
**How to avoid:** the fix is all three edits or none. Both real lints and their
exact suggestions are in §Clippy Remediation.
**Warning signs:** clippy output that never contains a line beginning
`Checking baude v2.1.5`.

### Pitfall 3: Squash-merging the phase PR

**What goes wrong:** 33 features and 19 fixes collapse into a single changelog
line — or, if the PR title is not a conventional `feat:`, release-please cuts no
release at all (PR #86's `docs:` title produced none).
**Why it happens:** squash is enabled and is the repo's habit for small fix PRs.
**How to avoid:** merge commit for this PR (see §Merge Strategy Decision).
**Warning signs:** the release PR title proposes `2.1.6` instead of `2.2.0`, or
no release PR appears at all within a few minutes of the merge.

### Pitfall 4: The 2-hour soak auto-merging the release before the human looks

**What goes wrong:** `release-automerge.yml` merges the `release:minor` PR on its
`*/30` cron once 2h have elapsed and required checks are green — publishing
v2.2.0 without the blocking-human gate CONTEXT requires.
**Why it happens:** the gate is opt-in (`release:hold`), not opt-out.
**How to avoid:** apply `release:hold` as the very first action after the phase
PR merges, before anything else.
**Warning signs:** a `v2.2.0` tag you did not authorize.

### Pitfall 5: Deleting the workspace lock file (and documenting that as recovery)

**What goes wrong:** the README ships advice that creates two writers on one
state file.
**Why it happens:** `.state-<ws>.json.lock` looks like a pid-file sentinel; it is
an fd-held advisory lock.
**How to avoid:** §Workspace-Lock Recovery Facts — the kernel releases it on
process exit; recovery is quitting the other baude or switching workspace.
**Warning signs:** draft README prose containing `rm` next to the lock path.

### Pitfall 6: Synthesizing smoke evidence

**What goes wrong:** the checklist is filled in from what the code *should* do,
which is exactly what SHIP-03 forbids ("observed, not synthesized").
**Why it happens:** legs 5-7 and 10-12 are tedious and have no automated proxy.
**How to avoid:** the template's `Observed (verbatim)` column plus the per-session
terminal identity and `baude --version` string. A deferral with sign-off is a
valid outcome; a fabricated pass is not.
**Warning signs:** an `Observed` cell that paraphrases the `What to look for`
cell.

### Pitfall 7: Forgetting the 1 commit behind `origin/main`

**What goes wrong:** the PR diff is computed against a base the branch does not
contain, or the merge introduces a spurious conflict in `.planning/`.
**Why it happens:** `30ce007` (PR #86) landed on main after the phase chain
started.
**How to avoid:** `git fetch origin && git rebase origin/main` before pushing —
also the project's recorded git discipline.

## Code Examples

### The vendored-fork lint header (the proven fix)

```rust
// vendor/vt100/src/lib.rs — replaces upstream lines 34-36.
// FORK (baude): upstream's lint header predates several clippy lints that now
// fire on unmodified upstream code, and `clippy::cargo` leaks package-metadata
// lints onto baude's own crates. This repo lints its OWN crates at -D warnings;
// it does not gate CI on the vendored fork's style. Diff surface: this block
// only. See vendor/vt100/README.md.
#![allow(clippy::all)]
#![allow(clippy::pedantic)]
#![allow(clippy::nursery)]
#![allow(clippy::cargo)]
#![warn(clippy::as_conversions)]
#![warn(clippy::get_unwrap)]
```

### Applying the human publish gate

```bash
# Immediately after the phase PR merges to main and release-please opens the
# release PR. Escape hatch documented at release-automerge.yml:8-9.
num=$(gh pr list --state open --label 'release:minor' --json number --jq '.[0].number')
gh pr edit "$num" --add-label release:hold
gh pr view "$num" --json title,labels --jq '{title,labels:[.labels[].name]}'
```

### Releasing the gate after approval

```bash
gh pr edit "$num" --remove-label release:hold     # cron lands it within 30 min
# or, to publish immediately:
gh pr merge "$num" --merge
```

### Harvesting CI evidence for SHIP-01

```bash
git fetch origin && git rebase origin/main
git push -u origin gsd/phase-11-negotiated-multiline-input
gh pr create --base main --title 'feat: v2.2 reliability and terminal usability' --body '...'
gh pr checks --watch
gh run list --workflow=ci.yml --limit 3 \
  --json headBranch,conclusion,url,createdAt \
  --jq '.[] | "\(.createdAt) \(.headBranch) \(.conclusion) \(.url)"'
```

## Runtime State Inventory

This is a release/validation phase, not a rename or refactor — but it does mutate
state outside the repo, so the categories are answered explicitly.

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | **None.** The phase writes no databases or datastores. The developer's `~/.config/baude/state-*.json` is *read* during the smoke dogfood but the suite is proven not to touch it (`assert-real-roots-untouched.sh after` → PASS this session). | none |
| Live service config | **GitHub repo state:** labels `release:minor` / `release:hold` (created idempotently by `release-please.yml:93` and by hand), branch protection on `main` (3 required contexts, `enforce_admins: true`), and the `RELEASE_PLEASE_APP_ID` / `RELEASE_PLEASE_APP_PRIVATE_KEY` secrets the release App authenticates with `[VERIFIED: release-please.yml:6-11, 27-29]`. None live in git. | verify the secrets are still valid before relying on the release PR; label creation is automatic |
| OS-registered state | **None.** No task scheduler, launchd, pm2, or systemd registration is created or renamed. | none |
| Secrets / env vars | `RELEASE_PLEASE_APP_ID`, `RELEASE_PLEASE_APP_PRIVATE_KEY` (repo secrets, unchanged by this phase); `TS_AUTHKEY` used only as a dummy in the `docker` CI job `[VERIFIED: ci.yml:50]`. `BAUDE_WORKSPACE` / `BAUDE_CLAUDE_CMD` / `BAUDE_EDITOR_CMD` appear in the smoke runbook, not as persisted config. | none — read-only |
| Build artifacts | **`target/` is stale relative to the clippy fix** (I built against a patched then reverted `vendor/vt100/src/lib.rs`, so the fork's fingerprint in `target/` no longer matches the tree). Also `~/.local/share/baude/worktrees` and `~/.config/baude` are the real roots the suite must not touch. | the ladder's own `cargo clippy` / `cargo test` will rebuild; no manual `cargo clean` needed, but do not trust a cached green from before the edits |

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in `libtest` (`cargo test`), no external harness |
| Config file | none — targets discovered from `Cargo.toml` members |
| Quick run command | `cargo test -p baude links` / `cargo test -p baude_core <module>` (focused filters) |
| Full suite command | `cargo test -- --test-threads=1` (CI's exact invocation, `ci.yml:35`) |

Serial execution is deliberate `[VERIFIED: ci.yml:29-34 — "Serial test execution
on CI (#58): the manager close-rollback test fails only on loaded 3-core CI
runners ... Every assertion stays intact; serial execution removes the
interference channel."]`. Use `--test-threads=1` locally too, or the local run is
not CI parity.

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| SHIP-01 | fmt gate green | gate | `cargo fmt --check` | ✅ (passes today) |
| SHIP-01 | clippy gate green | gate | `cargo clippy --all-targets -- -D warnings` | ❌ **red today** — 3 edits, §Clippy Remediation |
| SHIP-01 | focused link regressions | unit/integration | `cargo test -p baude links -- --test-threads=1`; `cargo test -p vt100 --test link_fidelity` | ✅ (16 + detection tests) |
| SHIP-01 | focused keyboard regressions | integration | `cargo test -p vt100 --test kitty_keyboard` | ✅ (14 tests) |
| SHIP-01 | workspace suite | full | `cargo test -- --test-threads=1` | ✅ (645 green) |
| SHIP-01 | real-root isolation | harness | `bash scripts/assert-real-roots-untouched.sh {--self-test,before,after}` | ✅ (40 self-checks, bracket PASS) |
| SHIP-01 | two-OS CI | CI | `gh pr checks` on the pushed PR | ❌ **branch not pushed** — no CI has ever run on phases 8-11 |
| SHIP-02 | each documented item present | grep | the 10 greps in §README Audit | ❌ 3 of 4 items absent |
| SHIP-03 | terminal smoke legs | manual-only | none — `12-SMOKE-EVIDENCE.md` | ❌ Wave 0 |
| SHIP-04 | locked release build | gate | `cargo build --workspace --release --locked` | ✅ (CI `artifact-readiness` proves 4 targets) |
| SHIP-04 | container build | CI | `docker` job (required status) | ✅ (green on main) |
| SHIP-04 | version/tag/notes agree | manual review | inspect the release PR diff | ❌ Wave 0 |

### Sampling Rate

- **Per task commit:** the focused filter for the touched area (`cargo test -p
  baude links`, `cargo test -p vt100 --test link_fidelity`) — seconds.
- **Per wave merge:** `cargo fmt --check` + `cargo clippy --all-targets -- -D
  warnings` + `cargo test -- --test-threads=1` — ~5 min.
- **Phase gate:** full bracket (all six commands, exit codes captured directly,
  never piped — recorded project policy) **plus** a green `pull_request` CI run
  on the pushed branch, before `/gsd-verify-work`.

### Wave 0 Gaps

- [ ] `12-SMOKE-EVIDENCE.md` — no such artifact exists; SHIP-03 has zero
      automated proxy for legs 5-7 and 10-12.
- [ ] Branch push + PR — the prerequisite for any CI evidence at all.
- [ ] Clippy green — blocks the required `check` contexts, so it blocks the merge
      train, so it blocks SHIP-04.

No new test *files* are needed: the 2 real clippy fixes are mechanical rewrites
inside code already covered by phase-10 tests.

## Security Domain

`security_enforcement: true`, `security_asvs_level: 1`
`[VERIFIED: .planning/config.json]`. This is a release phase; its security surface
is supply-chain and release integrity, not application input handling.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | yes (CI identity) | GitHub App token via `actions/create-github-app-token@v2`; workflow's own token stays read-only `[VERIFIED: release-please.yml:16-19 — `permissions: contents: read / pull-requests: read`]` |
| V3 Session Management | no | no sessions in scope |
| V4 Access Control | yes | branch protection with `enforce_admins: true`; `packages: write` scoped to `release.yml` only `[VERIFIED: release.yml:13-15]` |
| V5 Input Validation | no | no new input surfaces; phase 10's URL allowlist (`url` + `percent-encoding`, LINK-07) already shipped and is unchanged |
| V6 Cryptography | yes | `shasum -a 256` release checksums `[VERIFIED: release.yml:130-131]`; ghcr push-by-digest `[VERIFIED: release.yml:46]` |
| V14 Configuration | yes | `--locked` builds in both CI jobs pin the dependency graph to `Cargo.lock` |

### Known Threat Patterns

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Unauthorized release published | Elevation of Privilege | `release:hold` blocking-human gate; release-please App is the only writer |
| Dependency substitution in the release build | Tampering | `cargo build --workspace --release --locked` in `release.yml:102` and `ci.yml:111` |
| Vendored-fork drift going unnoticed | Tampering | `vendor/vt100/README.md` records provenance + per-file diff surface; the recommended lint header confines the fork edit to one commented block |
| Asset substitution after publish | Tampering | `SHA256SUMS.txt` attached to every release; digest-pinned multi-arch image manifest |
| Secret exposure in workflow logs | Information Disclosure | App token is minted per-run and never echoed; `TS_AUTHKEY` in CI is a literal dummy |

**Not a finding, but worth the planner's awareness:** relaxing lints on
`vendor/vt100` means future *upstream* code in that crate is unlinted. The fork
is frozen against `vt100 0.15.2` and every change to it goes through the
documented diff surface, so the exposure is bounded — but any future re-vendor
should re-review rather than trust the allow.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Tag-push triggers the release build | `release: types: [published]` triggers it | pre-v2.1 | Avoids a double-release collision since release-please creates the tag `[VERIFIED: release.yml:3-8]` |
| `bauded` shipped only via the ghcr image | Both binaries ride in every tarball | v2.x | `auto_daemon`'s next-to-the-binary lookup works for tarball/mise installs `[VERIFIED: release.yml:104-109]` |
| Uniform release automation | Tiered: patch auto-merge, minor 2h soak, major manual | pre-v2.1 | `[VERIFIED: release-please.yml:68-113]`; matches the recorded release-please policy |
| Parallel `cargo test` on CI | `--test-threads=1` | issue #58 | Removes cross-test PTY/pgid interference on 3-core runners |
| Per-resolver leak guards only | Whole-suite real-root bracket | issue #72, phase 8 | `[VERIFIED: ci.yml:23-26]` — proves the aggregate claim, not just per-resolver containment |

**Deprecated / stale in this repo:**
- The phase-11 deferred-items claim that CI has a pinned toolchain — false, and
  it should be corrected in that ledger as part of this phase.
- `origin/release-please--branches--main` — a merged leftover branch
  (`delete_branch_on_merge: false`); harmless, not this phase's problem.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The GitHub runner images ship a Rust/clippy at least as new as local `1.98.1`, so CI's clippy will report at least the lints observed locally. | CI Parity, Clippy Remediation | LOW. If CI's clippy were *older*, the fix is still correct and still green — the fix silences lints rather than relying on their absence. Only risk is a *newer* clippy adding a lint the group-allow does not cover, which the recommended `clippy::all` form largely forecloses. |
| A2 | A squash merge with `squash_msg: COMMIT_MESSAGES` does not reliably yield per-commit release-please changelog entries. | Merge Strategy Decision | MEDIUM. Drives the merge-commit recommendation. If wrong, squash would also work — but merge-commit is correct either way, so acting on this assumption is safe. |
| A3 | `.dockerignore`'s `*.md` does not exclude nested `vendor/vt100/README.md` (Docker patterns don't cross `/`). | Docker / container build | LOW. Even if excluded, `readme` is only validated by `cargo package`/`publish`, which this repo never runs. Proven for free by the required `docker` CI check. |
| A4 | A `[lints]` table in `vendor/vt100/Cargo.toml` cannot override the in-source `#![warn(clippy::cargo)]`, because source-level attributes beat CLI-provided lint levels. | Standard Stack → Alternatives | LOW. Only affects which of two fix routes is chosen; the recommended route (lib.rs edit) was empirically proven to work this session. |
| A5 | release-please's default changelog sections exclude `docs`/`test`/`style`/`refactor`, so a merge-commit release would show 33 Features + 19 Bug Fixes. | Branch and Merge State | LOW. `CHANGELOG.md`'s existing entries show only `### Features` / `### Bug Fixes` / `### Miscellaneous Chores` sections, consistent with the default. Worst case the notes are longer than expected. |

## Open Questions (RESOLVED)

1. **Squash vs merge commit for the phase PR.**
   - What we know: both are enabled; merge commit yields per-commit changelog
     entries; squash requires a `feat:` PR title or no release is cut.
   - What's unclear: maintainer preference, and A2's exact release-please
     behavior on concatenated squash bodies.
   - Recommendation: merge commit. If the maintainer insists on squash, the PR
     title must be `feat: ...` and the plan must accept a one-line changelog.

2. **Whether Linux interactive smoke legs will be available.**
   - What we know: CONTEXT explicitly permits deferral with sign-off; the
     automatable Linux legs are already covered by the `check (ubuntu-22.04)` CI
     job.
   - What's unclear: whether the maintainer has a real Linux terminal session
     available during the phase.
   - Recommendation: design `12-SMOKE-EVIDENCE.md` so the Linux table can be
     submitted with legs 5-7 and 10-12 marked DEFERRED + signed, without
     invalidating the artifact.

3. **Scope of the lint allow in `vendor/vt100/src/lib.rs`.**
   - What we know: `clippy::all` + `pedantic` + `nursery` + `cargo` is proven to
     produce exit 0; the exhaustive narrow list is the 10 lints named in §Clippy
     Remediation.
   - What's unclear: CONTEXT gives Claude discretion ("narrowest scope wins") but
     a narrow list will drift as clippy advances against frozen upstream code.
   - Recommendation: group allow confined to the fork's one commented block. Flag
     for the maintainer if they read "narrowest" strictly as per-lint.

4. **Whether phase 8's ROADMAP/STATE status should be corrected in this phase.**
   - What we know: ROADMAP:203 says "In Progress" with 8/8 plans; STATE says
     `current_phase: 8`; 08-VERIFICATION's frontmatter says `passed` while its
     body says `human_needed`.
   - What's unclear: whether the re-stamp is `/gsd-verify-work 8` alone or also a
     ROADMAP/STATE correction.
   - Recommendation: include both as checklist items; the state inconsistency
     will otherwise confuse `/gsd-complete-milestone`.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `cargo` / `rustc` | every gate | ✓ | 1.98.1 | — |
| `cargo clippy` | clippy gate | ✓ | 0.1.98 | — |
| `cargo fmt` | fmt gate | ✓ | bundled | — |
| `bash` + `scripts/assert-real-roots-untouched.sh` | isolation bracket | ✓ | self-test 40/40 | — |
| `git` | rebase, push, merge train | ✓ | — | — |
| `gh` CLI (authenticated) | PR, labels, CI evidence, release inspection | ✓ | `gh run list` / `gh api` both returned data | GitHub web UI |
| `docker` + `docker compose` | local container de-risk | not probed | — | CI `docker` job is a required check and proves it |
| Real macOS terminal | SHIP-03 legs 1-12 | ✓ (Darwin 27.0.0, this host) | — | — |
| Real Linux terminal | SHIP-03 Linux interactive legs | **unknown** | — | Docker/CI for automatable legs; explicit deferral with sign-off per CONTEXT |
| GitHub Actions runners | SHIP-01 CI evidence, SHIP-04 assets | ✓ | 4 green release runs since 2026-09-11 | — |
| `RELEASE_PLEASE_APP_ID` / `_PRIVATE_KEY` secrets | release PR creation | presumed ✓ | — | none — without them no release PR is opened |

**Missing dependencies with no fallback:** none blocking.
**Missing with fallback:** real Linux terminal (deferral path is locked in CONTEXT).

## Sources

### Primary (HIGH confidence — read or executed in this repo, this session)

- `.github/workflows/ci.yml` (1-126), `release.yml` (1-139), `release-please.yml`
  (1-113), `release-automerge.yml` (1-60) — read in full
- `release-please-config.json`, `.release-please-manifest.json`, root `Cargo.toml`,
  `baude/Cargo.toml`, `baude-core/Cargo.toml`, `bauded/Cargo.toml`,
  `vendor/vt100/Cargo.toml`, `Dockerfile`, `.dockerignore` — read in full
- `baude/src/main.rs:108-141, 365-448`; `baude/src/links.rs:195-210`;
  `baude/src/ui.rs:2114-2170, 2212`; `baude-core/src/persist.rs:170-238, 468-609`;
  `baude-core/src/workspace.rs:63-71` — read
- `README.md` (section map + targeted greps); `CHANGELOG.md:1-60`
- `.planning/phases/12-.../12-CONTEXT.md`; `.planning/REQUIREMENTS.md:69-76`;
  `.planning/ROADMAP.md:182-204`; `.planning/STATE.md`;
  `08-VERIFICATION.md`; `11-VERIFICATION.md`; three `deferred-items.md`;
  `.planning/milestones/v2.0-phases/07-local-tui-dogfood-release/07-UAT-EVIDENCE.md`
- Commands executed: `cargo clippy --all-targets -- -D warnings` (3 variants incl.
  the proven fix), `cargo fmt --check`, `cargo test -- --test-threads=1`,
  `scripts/assert-real-roots-untouched.sh {--self-test,before,after}`,
  `rustc/cargo/clippy --version`, `git merge-base/rev-list/log/ls-remote`,
  `gh run list`, `gh pr list`, `gh release view`,
  `gh api repos/poindexter12/baude{,/branches/main/protection,/rulesets}`

### Secondary (MEDIUM confidence)

- None. No external documentation was needed — every question resolved against
  the repository or a live command.

### Tertiary (LOW confidence)

- Items A1-A5 in the Assumptions Log.

## Metadata

**Confidence breakdown:**
- Release machinery map: HIGH — all four workflows and both release-please config
  files read in full; corroborated by four successful release runs and the shipped
  v2.1.5 asset list.
- CI parity + gate status: HIGH — the full bracket was executed this session and
  the exit codes observed directly.
- Clippy remediation: HIGH — the fix was applied, verified to exit 0, and reverted;
  the two previously-masked baude lints are a novel finding with exact locations.
- Branch/merge state: HIGH — ancestry, divergence, remote refs, protection, and
  merge settings all queried live.
- README audit: HIGH — line-numbered, grep-verified, and cross-checked against the
  phase-10 commit that was believed to have documented the links.
- Smoke template: MEDIUM-HIGH — grounded in a real prior artifact and in
  source-verified terminal-mode facts, but the leg list is Claude's discretion per
  CONTEXT and unvalidated by a maintainer.
- Merge strategy: MEDIUM — the recommendation is safe under both branches of A2,
  but A2 itself is unverified.

**Research date:** 2026-09-16
**Valid until:** 2026-09-23 (7 days — the branch is 165 commits ahead of a moving
`main`, the clippy surface moves with the runner's toolchain, and the release PR
state is live).

> RESOLVED dispositions: Q1 (merge vs squash) — merge commit, adopted in 12-05 Task 1. Q2 (Linux terminal availability) — DEFERRED-with-signed-sign-off design, 12-03. Q3 (fork lint scope) — group allow at the fork crate root with a fork-marker comment, 12-01 Task 1. Q4 (phase-8 ROADMAP/STATE correction) — two explicit checklist items, 12-04 Task 3.
