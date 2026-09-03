# Phase 7: Local TUI Dogfood Release - Pattern Map

**Mapped:** 2026-08-30
**Files analyzed:** 16 proposed new/modified files
**Analogs found:** 16 / 16 (2 are composite/partial matches)

## Scope Guardrails

- Project local instructions: no `AGENTS.md`, `.claude/skills/`, or `.agents/skills/` exists.
- Preserve the durable repository graph and Phase 6 lifecycle authority. Presentation code must not perform Git mutations or invent retry permission.
- Keep remote daemon rows and `/sessions` flat. `bauded/src/api.rs` receives a compatibility test only.
- Exclude dormant branches, branch deletion, daemon hierarchy, PWA work, force removal, fetch, main-checkout switching, publishing, tagging, pushing, and PR creation.
- `.release-please-manifest.json` stays at `0.14.0`; it is last-release history, not proposed source version.

## File Classification

| New/Modified File | Role | Data Flow | Closest Existing Analog | Match Quality |
|---|---|---|---|---|
| `baude-core/src/lifecycle.rs` | model / utility | transform, event-driven | same file's reducer and recovery projection | exact |
| `baude/src/main.rs` | config / controller | event-driven | same file's module declarations and draw loop | exact |
| `baude/src/hierarchy.rs` (new) | component / utility | transform | `baude-core/src/repository.rs` + `baude/src/app.rs::ordered_ids` | composite |
| `baude/src/app.rs` | controller / store | event-driven, request-response | same file's checkout-key lifecycle adapters | exact |
| `baude/src/ui.rs` | component | transform | same file's sidebar rows, responsive geometry, modals, tests | exact |
| `bauded/src/api.rs` | route test | request-response | same file's flat router and `tower::ServiceExt` tests | exact |
| `.github/workflows/ci.yml` | config | batch | `.github/workflows/release.yml` build matrix/package layout | role-match |
| `Cargo.toml` | config | batch | current workspace dependency declarations | exact |
| `baude-core/Cargo.toml` | config | batch | the three current package manifests | exact |
| `baude/Cargo.toml` | config | batch | the three current package manifests | exact |
| `bauded/Cargo.toml` | config | batch | the three current package manifests | exact |
| `Cargo.lock` | config / generated | batch | current workspace package lock entries | exact |
| `release-please-config.json` | config | batch | current root package and `extra-files` mapping | exact |
| `README.md` | documentation | transform | current Install, Keys, Worktrees, remote compatibility sections | exact |
| `CHANGELOG.md` | documentation | batch | current release-please-generated release sections | exact |
| `docs/local-tui-dogfood.md` (new) | documentation / test runbook | file-I/O, batch | `docs/deploy.md` numbered command runbook + App real-Git fixtures | composite |

## Pattern Assignments

### `baude-core/src/lifecycle.rs` (model / utility, transform)

**Analog:** `baude-core/src/lifecycle.rs`

Add the pure `LifecycleCapability` projection beside existing pure lifecycle projections, not in `ui.rs` or `app.rs`. Copy the exhaustive, side-effect-free style of `startup_recovery_program` (lines 371-417):

```rust
pub enum RecoveryStep {
    StopOwned(CheckoutKey),
    FinishRemoval(CheckoutKey),
    RecoverActivation(CheckoutKey),
    ReconcileTopology(CheckoutKey),
    Launch(CheckoutKey),
}

pub fn startup_recovery_program(state: &RepositoryState) -> Vec<RecoveryStep> {
    // exhaustive match on checkout.lifecycle(); return typed values only
}
```

Use the legal-transition refusal pattern (lines 48-99): unsupported state/event pairs retain state and emit no effects. Capability must similarly expose only App-dispatchable `RetryReopen` or `RetryRecovery`, never infer from display text.

Testing pattern (lines 2168-2211): construct durable state, assert an exact ordered typed vector, normalize state, then assert repeated projection is empty/idempotent.

### `baude/src/main.rs` (config / controller, event-driven)

**Analog:** `baude/src/main.rs`

Register the new sibling module exactly like the existing declarations (lines 1-6):

```rust
mod app;
mod keys;
mod notify_desktop;
mod remote;
mod ui;
mod usage;
```

Preserve the controller ordering in the draw loop (lines 317-324): tick, obtain viewport, synchronize visible PTY sizes, then draw. Hierarchy logic belongs in the new pure module; `main.rs` only registers it.

### `baude/src/hierarchy.rs` (new component / utility, transform)

**Composite analogs:** `baude-core/src/repository.rs`, `baude/src/app.rs`, `baude/src/ui.rs`

Use durable key types directly (`repository.rs` lines 11-27):

```rust
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RepositoryKey(u64);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct CheckoutKey(u64);
```

Project from `RepositoryState.repositories` and `.checkouts` (`repository.rs` lines 349-357), with child identity/order from `SavedCheckout` (`repository.rs` lines 244-263):

```rust
pub struct SavedCheckout {
    pub key: CheckoutKey,
    pub repository_key: RepositoryKey,
    pub role: CheckoutRole,
    pub managed_by_baude: bool,
    pub observed_path: PersistedPath,
    pub observed_branch: Option<String>,
    pub first_seen_order: u64,
    // lifecycle/session are decoration inputs, not identity/sort keys
}
```

Replace, do not extend, the current status-grouped `ordered_ids` pattern (`app.rs` lines 1414-1447). The useful part to retain is deterministic sorting and key-valued output; forbidden inputs are archive/runtime/status. Parent comparator: case-folded display name, canonical main path, `RepositoryKey`. Child comparator: `(first_seen_order, CheckoutKey)`.

Model selection as `Repository(RepositoryKey)`, `Checkout(CheckoutKey)`, `Remote(u64)`. Keep row labels, optional runtime IDs, status, and attention separate. Provide pure helpers for selection reconciliation and removal fallback: same key if present; after removal next sibling, previous sibling, then parent.

Tests should follow inline fixture style from `repository.rs` lines 650-693: small `path`, `repository`, and `checkout` constructors, followed by direct key-vector assertions. Mutate runtime/status/archive/health and assert the row-key vector remains identical.

### `baude/src/app.rs` (controller / store, event-driven)

**Analog:** existing durable checkout lifecycle adapters in `baude/src/app.rs`

Imports and ownership seam (lines 11-27, 327-329):

```rust
use baude_core::lifecycle::{self, LifecycleOutcome, RepositoryReservations};
use baude_core::repository::{CheckoutKey, RepositoryState, ...};

repository_state: RepositoryState,
runtime_checkouts: HashMap<CheckoutKey, u64>,
repository_reservations: RepositoryReservations,
```

Durable-target dispatch must copy these existing methods rather than calling Git from handlers:

- Branch: `activate_branch_worktree` lines 1053-1218.
- Reopen: `reopen_checkout` lines 1230-1279 and core `plan_reopen` lines 979-1039.
- First remove preflight: `prepare_remove_worktree` lines 1842-1862.
- Confirm/second preflight: `confirm_remove_worktree` lines 1864 onward.
- Retained close: `close_retained_session` lines 2055-2108.

Persistence/error pattern (lines 388-400): clone prior state, apply the opaque candidate, save, mark dirty, and restore memory only when replacement did not commit.

```rust
let before = self.app.repository_state.clone();
candidate.apply(&mut self.app.repository_state)?;
if let Err(error) = self.app.save_durable_status() {
    self.app.persistence_dirty = true;
    if !error.replacement_committed() {
        self.app.repository_state = before;
    }
    return Err(anyhow::Error::new(error));
}
```

Action routing should remain centralized like `handle_sidebar_key` (lines 2521-2601), but switch on durable selection kind plus explicit capability. Match Shift+X separately from lowercase `x`; `x` opens retained-close confirmation only, while Shift+X runs first preflight before opening remove confirmation. Every defensive refusal leaves selection/state/runtime/Git unchanged and uses the exact UI-spec copy.

Typed refusal translation must match variants, not `Display` strings. Branch variants are in `baude-core/src/git.rs` lines 974-1014 (`InvalidLiteral`, `RemoteOnly`, `PathCollision`, etc.). Removal blockers are in `git.rs` lines 1661-1684. Generic `Display` implementations (`git.rs` lines 1016-1054; `lifecycle.rs` lines 564-579) are diagnostic fallbacks only.

Resize pattern to replace (lines 2282-2300): retain `ui::layout(area)` as the single geometry source, but resize only a visible content rectangle whose inner width and height are positive. In hidden sidebar-only mode retain the last valid PTY size. Preserve remote minimums from `remote.rs` lines 312-323 (`rows.max(2)`, `cols.max(10)`).

Real-Git dogfood test should factor, not duplicate, these fixtures (`app.rs` lines 3449-3516): argv-only `git`, a temp bare origin + main checkout, per-test state root, sleeping backend, and process-scoped key seed. Compose existing assertions from:

- activation/reuse: lines 3695-3745;
- close retains keys/order: lines 4079-4194;
- reopen exactly once: lines 4307-4361;
- double-preflight remove and branch survival: lines 4371-4439.

Drop and recreate `App` using the same isolated persistence root as restore tests (`app.rs` lines 642-695), then assert one parent, stable child keys/order, one checkout-to-runtime mapping, and no writes outside the fixture.

### `baude/src/ui.rs` (component, transform)

**Analog:** existing `baude/src/ui.rs`

Preserve imports and visual vocabulary (lines 1-15), rounded/focus borders (lines 59-65), gutter (lines 79-94), selection band (lines 96-101), and status glyph styles (lines 327-429).

Use hierarchy rows in place of the current flat loop (`ui.rs` lines 142-218). Keep the remote header flat and separate (`ui.rs` lines 221-242). Remove only the local `▼ archived` regrouping; archive becomes child decoration.

Unicode sizing must replace `chars().count()` uses in `truncate`, `chips_line`, row padding, and status hints. Follow the already imported Ratatui `Line`/`Span` composition and measure the composed `Line::width()` before padding/truncation.

Responsive geometry should evolve the pure `layout(Rect) -> LayoutRects` seam (lines 20-49), returning visibility/mode as well as rectangles. Preserve saturating arithmetic and the existing zero-area early return in `draw_term` (lines 715-725). Implement the UI-spec width/height breakpoints exactly.

Modal pattern: use `centered` (lines 895-904), `Clear`, rounded `Block`, and bounded dimensions. Split the current combined close modal (lines 1046-1064): close is cyan/non-destructive and says checkout kept; remove follows the red exact-target pattern (lines 1066-1079 and helper lines 1411-1418) but adds target, full branch ref, exact path, retention statement, and safe negative action.

Testing pattern: keep inline `#[cfg(test)] mod tests` (lines 1421-1473), add `ratatui::backend::TestBackend` whole-screen tests and direct buffer assertions. Render 160x40, 100x30, 79x24, 59x20, and 40x12; assert connectors, glyphs, xterm-237 two-line selection band, modal target lines, and no panic/out-of-bounds.

### `bauded/src/api.rs` (route test, request-response)

**Analog:** flat router and route tests in `bauded/src/api.rs`

Production compatibility boundary is the existing router (lines 21-48): `/sessions` remains the only list projection and no hierarchy/remove-worktree endpoint is added.

Copy the test harness style (lines 667-708): `Arc<Mutex<Manager>>`, `router`, `tower::ServiceExt::oneshot`, request helpers, and JSON body collection. The new compatibility test should inspect `/sessions`, prove it remains a JSON array of flat session objects, exercise compatibility DELETE as retained close, and assert nonexistent hierarchy/remove-worktree paths are not routes. Do not modify PWA or production routes.

### `.github/workflows/ci.yml` (config, batch)

**Analog:** `.github/workflows/release.yml` lines 81-112

Copy only the four-target native build matrix and two-binary tar layout:

```yaml
strategy:
  matrix:
    include:
      - target: aarch64-apple-darwin
        os: macos-14
      - target: x86_64-apple-darwin
        os: macos-14
      - target: x86_64-unknown-linux-gnu
        os: ubuntu-22.04
      - target: aarch64-unknown-linux-gnu
        os: ubuntu-22.04-arm
```

Build with `--locked`, archive `baude bauded`, extract, and assert both `--version` outputs. Artifact upload is allowed. Do not copy release workflow permissions, Docker login/push, manifest creation, checksums-for-release, `gh release upload`, tags, or publication trigger.

Retain the existing check convention from `ci.yml` lines 9-19, updating commands to explicit workspace/all-target forms where planned.

### `Cargo.toml`, package manifests, and `Cargo.lock` (config, batch)

**Analogs:** current workspace and package manifests

Keep workspace inheritance and add an exact version alongside the local path (`Cargo.toml` lines 10-15):

```toml
[workspace.dependencies]
baude-core = { path = "baude-core", version = "=2.0.0-beta" }
```

Set each literal package version in `baude-core/Cargo.toml`, `baude/Cargo.toml`, and `bauded/Cargo.toml` at line 4 to `2.0.0-beta`; preserve `edition.workspace`, `license.workspace`, and `repository.workspace`. Keep `baude-core.workspace = true` in dependent crates.

Do not hand-edit lock dependency graphs. Regenerate `Cargo.lock` with Cargo and verify the three workspace entries currently at lines 199-228 all become `2.0.0-beta`.

### `release-please-config.json` (config, batch)

**Analog:** current `release-please-config.json` lines 3-15

Preserve the single root package, simple release type, changelog path, tag behavior, and the three TOML `extra-files`. Add exact beta readiness (`release-as: 2.0.0-beta`, prerelease enabled, appropriate beta prerelease type) without running release-please.

Do not edit `.release-please-manifest.json`; its tracked value remains:

```json
{ ".": "0.14.0" }
```

### `README.md` (documentation, transform)

**Analog:** current `README.md`

Update, do not duplicate, the current sections: Install (lines 30-44), Keys (97-123), Worktrees (182-187), and remote compatibility (366-374). Replace flat alphabetical/archive wording with stable repository-parent and persisted oldest-first child behavior. Document `x` as retained close and `X` as clean managed-worktree removal retaining the branch. Keep remote rows flat and non-destructive.

Source install should pin the local beta source state or use an isolated `cargo install --root`; do not imply a GitHub release exists and do not overwrite the user's global install during dogfood.

### `CHANGELOG.md` (documentation, batch)

**Analog:** existing release sections at `CHANGELOG.md` lines 3-18

Add an unreleased/readiness section above `0.14.0`, grouped under feature/fix headings like existing entries. Say “ready for `v2.0.0-beta`” only after gates pass; never say released or published, and do not add a compare URL requiring a nonexistent tag.

### `docs/local-tui-dogfood.md` (new documentation / test runbook, file-I/O)

**Composite analogs:** `docs/deploy.md` and App real-Git fixtures

Copy the numbered, command-first runbook structure from `docs/deploy.md` (for example lines 56-67 and 119-129), but make every path isolated: temporary HOME, temporary bare origin/main repository, local `cargo build --release --locked`, harmless sleeping backend, and `cargo install --root <temp-root>` if installation is exercised.

The manual sequence must be open/admit -> create or activate -> close -> restart -> reopen -> safe remove. Record temp paths, selected `RepositoryKey`/`CheckoutKey`, branch ref, wide/narrow screenshots, and before/after `git worktree list --porcelain`. End with cleanup. Include no `git push`, tag, release-please, `gh`, publish, Docker push, or PR command.

## Shared Patterns

### Durable Identity and Ordering

**Sources:** `baude-core/src/repository.rs` lines 11-27, 234-263, 349-357

Apply to `hierarchy.rs`, `app.rs`, and `ui.rs`: keys are identity; paths/labels/runtime IDs are facts or decoration. Local row order is structural and cannot use status, attention, archive, runtime presence, or health.

### Lifecycle Authorization and Persistence

**Sources:** `baude-core/src/lifecycle.rs` lines 48-99, 109-190; `baude/src/app.rs` lines 377-443

Apply to every local mutation: core selects a legal candidate/effect, App persists before effects, and the UI consumes typed capability/outcome. Illegal transitions/refusals have zero effects.

### Typed Git Safety

**Sources:** `baude-core/src/git.rs` lines 1211-1333, 1661-1723, 2201-2266

Apply to branch and removal actions: literal argv, exact refs, fresh topology, typed blockers, and opaque verified removal target. Never shell-expand input, parse diagnostic text for authority, recurse-delete, or use `--force`.

### Distinct Close and Remove

**Sources:** `baude-core/src/lifecycle.rs` lines 501-521, 614-679; `baude/src/app.rs` lines 2055-2108

Close snapshots and retains the child. Remove performs first preflight, confirmation, stop, fresh second preflight, verified non-force Git removal, postconditions, then exact child removal. Their keys, copy, colors, and confirmations remain separate.

### Flat Remote Compatibility

**Sources:** `baude/src/remote.rs` lines 19-73; `bauded/src/api.rs` lines 21-48

Remote identity remains runtime `u64`; remote list ordering/attach/restart/close behavior stays separate from local durable hierarchy. No remote repository parent or local Shift+X dispatch.

### Exact-Test Zero-Match Guard

For every prescribed new test, first require its fully qualified name in `cargo test -- --list`, then run the same name with `--exact`:

```bash
name='hierarchy::tests::local_hierarchy_orders_parents_and_children_by_durable_identity'
cargo test -p baude -- --list | rg -Fx -- "$name: test"
cargo test -p baude "$name" -- --exact --nocapture
```

## Partial Analogs / Research-Led Files

| File | Limitation | Planner Direction |
|---|---|---|
| `baude/src/hierarchy.rs` | No existing durable hierarchy projection exists; current sidebar is runtime-flat. | Use the composite durable model/sort/row patterns above and `07-UI-SPEC.md` as behavioral authority. |
| `docs/local-tui-dogfood.md` | No existing isolated local TUI dogfood runbook exists. | Use `docs/deploy.md` structure plus existing App real-Git fixture mechanics; follow Research REL-01 evidence checklist. |

## Tracked-Path Verification

All 14 proposed existing files were verified with `git ls-files --error-unmatch` and are tracked:

`baude-core/src/lifecycle.rs`, `baude/src/main.rs`, `baude/src/app.rs`, `baude/src/ui.rs`, `bauded/src/api.rs`, `.github/workflows/ci.yml`, `Cargo.toml`, `baude-core/Cargo.toml`, `baude/Cargo.toml`, `bauded/Cargo.toml`, `Cargo.lock`, `release-please-config.json`, `README.md`, `CHANGELOG.md`.

The two proposed new files are absent as expected: `baude/src/hierarchy.rs` and `docs/local-tui-dogfood.md`. Analog sources `baude-core/src/repository.rs`, `baude-core/src/git.rs`, `baude/src/remote.rs`, `.github/workflows/release.yml`, and `docs/deploy.md` were also verified tracked. `.release-please-manifest.json` is tracked and intentionally unchanged.

Pre-existing unrelated untracked paths observed during mapping: `graphify-out/` and `opencode.json`. Do not include or modify them.

## Metadata

**Analog search scope:** `baude-core/src`, `baude/src`, `bauded/src`, `.github/workflows`, workspace/package manifests, root release metadata, `README.md`, `CHANGELOG.md`, and `docs/`

**Primary analog files read:** 18 source/config/documentation files plus Phase 5/6 summaries and Phase 7 research/UI contract

**Pattern extraction date:** 2026-08-30
