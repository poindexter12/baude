# Phase 7: Local TUI Dogfood Release - Research

**Researched:** 2026-08-30  
**Domain:** Rust/Ratatui repository hierarchy, safe local worktree actions, and beta release readiness  
**Confidence:** HIGH

## User Constraints

### Locked scope

- Phase 7 is the smallest complete local-TUI dogfood slice on top of Phases 5 and 6. Preserve the existing Ratatui design and `07-UI-SPEC.md`; do not redesign the product. [VERIFIED: user-provided scope]
- Implement local repository parents and durable checkout/worktree children, stable structural ordering, durable-key selection, contextual local actions, responsive terminal behavior, and `v2.0.0-beta` readiness. [VERIFIED: user-provided scope]
- Reuse Phase 5 repository admission/persistence/reconciliation and Phase 6 shared lifecycle authority. Do not reimplement Git or lifecycle transactions in the UI. [VERIFIED: user-provided scope]
- Keep the flat daemon/session projection visually separate and semantically compatible. Do not add daemon hierarchy or destructive remote behavior. [VERIFIED: user-provided scope]
- Exclude dormant branch rows/deletion, PWA work, force removal, branch deletion, fetch, main-checkout switching, release publication, push, and PR creation. [VERIFIED: user-provided scope]
- Separate implementation and local automated gates from deferred human UAT, Linux/runtime certification, independent deep review, phase verification, and publication. [VERIFIED: user-provided scope]
- Validation must name exact tests and include zero-match guards so a filtered Cargo invocation cannot pass by running no tests. [VERIFIED: user-provided scope]

### the agent's Discretion

- Choose the internal hierarchy projection, durable selection model, test fixture factoring, and release-readiness command structure, provided they preserve the locked UI and lifecycle contracts. [VERIFIED: user-provided scope]

### Deferred Ideas (OUT OF SCOPE)

- Dormant branch rows and branch deletion; daemon/remote hierarchy; PWA hierarchy; publication, push, and PR creation; force removal; automatic stash/commit/reset/clean; fetch; main-checkout switching. [VERIFIED: user-provided scope]

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| REPO-05 | Show the main checkout and distinct managed default worktree under one repository. | Project durable state already records repository membership and `CheckoutRole::{Main, PrimaryDefault, ManagedBranch}`; project these records rather than runtime sessions. [VERIFIED: `baude-core/src/repository.rs`]
| HIER-01 | Render repository parents with checkout/worktree children. | Add a pure hierarchy projection and render parent/child rows from durable keys. [VERIFIED: `07-UI-SPEC.md`]
| HIER-02 | Keep parents visible without running children. | Build rows from `RepositoryState.repositories` and `.checkouts`, not `App.sessions`. [VERIFIED: codebase inspection]
| HIER-03 | Parent name ordering and persisted oldest-first child ordering. | Sort parents by display name/path/key and children by `first_seen_order`/`CheckoutKey`. [VERIFIED: `07-UI-SPEC.md`]
| HIER-04 | Volatile status never reorders or hides attention. | Keep sort keys structural; join runtime status only after row order is fixed. [VERIFIED: `07-UI-SPEC.md`]
| WORK-01 | Create or activate a local branch from repository context. | Route `w` to existing `App::activate_branch_worktree` with the selected durable parent. [VERIFIED: `baude/src/app.rs`]
| WORK-02 | Refuse invalid refs, collisions, and unsafe occupancy. | Preserve typed Git/lifecycle failures and map them to the UI contract's target-specific copy. [VERIFIED: `baude-core/src/git.rs`; `07-UI-SPEC.md`]
| WORK-03 | Close runtime while retaining checkout child. | Replace the combined close/remove modal with the existing retained-close lifecycle path and distinct copy. [VERIFIED: `baude/src/app.rs`; `07-UI-SPEC.md`]
| WORK-04 | Reopen a retained main/worktree child. | Resolve `CheckoutKey` to `App::reopen_checkout`; never identify a retained child by runtime ID. [VERIFIED: `baude/src/app.rs`]
| WORK-05 | Separately confirm safe managed-worktree removal and retain branch. | Route Shift+X through existing first preflight and confirmation/second-preflight transaction. [VERIFIED: `baude/src/app.rs`; `baude-core/src/lifecycle.rs`]
| WORK-06 | Block dirty/conflicted/locked/submodule/indeterminate removal before mutation. | Translate existing `RemovalFailure`/`RemovalBlocker` variants without weakening fail-closed preflight. [VERIFIED: `baude-core/src/git.rs`; `baude-core/src/lifecycle.rs`]
| SURF-01 | Contextual actions, hints, and actual-target confirmations. | Derive an action view from selection kind plus explicit lifecycle capability. [VERIFIED: `07-UI-SPEC.md`]
| SURF-02 | Preserve applicable existing local actions and navigation. | Adapt existing shell/editor/info/GSD/archive/session-cycle handlers through checkout-to-runtime resolution. [VERIFIED: `baude/src/app.rs`]
| SURF-05 | Keep flat daemon/session compatibility non-destructive during transition. | Leave `RemoteInfo`, `/sessions`, and remote action semantics flat; add compatibility assertions only. [VERIFIED: `baude/src/remote.rs`; `bauded/src/api.rs`]
| REL-01 | Isolated real-repository local dogfood flow survives restart without duplicates or lost work. | Add one real-Git automated flow plus a HOME-isolated manual TUI runbook. [VERIFIED: existing real-Git fixtures in `baude/src/app.rs`; `07-UI-SPEC.md`]
| REL-02 | Format, lint, tests, package checks, and supported artifact builds pass. | Add exact-test gates, package archives, host extraction checks, and a non-publishing CI artifact matrix. [CITED: https://doc.rust-lang.org/cargo/commands/cargo-package.html]
| REL-03 | Source/package/docs consistently target `v2.0.0-beta` without release publication or push. | Synchronize package versions and release configuration, preserve the last-released manifest, document readiness, and run only local/CI build checks. [CITED: https://github.com/googleapis/release-please/blob/main/docs/manifest-releaser.md]
</phase_requirements>

## Summary

Phases 5 and 6 already supply the hard safety work: canonical repository admission, durable repository/checkout identity, persisted first-seen ordering, Git reconciliation, branch activation, retained close/reopen, clean managed-worktree removal, exact PTY ownership, and shared App/Manager lifecycle transactions. Phase 7 should therefore be a presentation-and-routing cutover, not a new lifecycle implementation. [VERIFIED: Phase 5/6 summaries and codebase inspection]

The current TUI remains runtime-session-centric: `SelId::Local(u64)` cannot select a parent or a retained child with no runtime; `ordered_ids()` groups active local, active remote, and archived rows by status; the sidebar renders flat two-line sessions; and worktree close still combines “keep” and “remove” in one modal. Current layout can also produce a zero-width sidebar/content relationship at narrow sizes, while `sync_sizes()` forwards the resulting inner size directly to PTYs. [VERIFIED: `baude/src/app.rs:195-200,1414-1447,2282-2300,2499-2518`; `baude/src/ui.rs:26-49,113-218,1046-1079`]

**Primary recommendation:** introduce one pure durable hierarchy projection, change local selection identity to `RepositoryKey`/`CheckoutKey`, route every action through existing App lifecycle methods and explicit core capabilities, test projection/render/action/resize independently, then run an isolated real-Git dogfood and non-publishing beta-readiness gate. [VERIFIED: synthesis of codebase and locked UI contract]

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Durable repository/checkout identity and lifecycle capability | Core domain (`baude-core`) | Local App adapter | The core owns lifecycle truth; App may resolve effects but UI must not infer permission. [VERIFIED: Phase 6 summary; `07-UI-SPEC.md`]
| Hierarchy projection and durable selection | Local App/presentation model | Ratatui renderer | Projection joins durable records to optional runtimes; renderer consumes rows without owning identity or sort rules. [VERIFIED: codebase inspection]
| Branch/close/reopen/remove effects | Local App adapter | Core lifecycle/Git | Existing App methods adapt UI requests to shared lifecycle authority and real Git. [VERIFIED: `baude/src/app.rs`; `baude-core/src/lifecycle.rs`]
| Responsive layout and visual semantics | Ratatui UI | App resize synchronization | UI computes visible panes; App resizes only to a valid visible content pane. [VERIFIED: `baude/src/ui.rs`; `07-UI-SPEC.md`]
| Flat remote compatibility | Existing TUI remote client and daemon API | Local hierarchy renderer | Remote rows remain a separate compatibility section and never become local durable parents. [VERIFIED: `07-UI-SPEC.md`; codebase inspection]
| Package and artifact readiness | Cargo/CI | Documentation | Cargo assembles/verifies packages; CI builds supported targets; docs record local evidence without publication claims. [CITED: https://doc.rust-lang.org/cargo/commands/cargo-package.html]

## Current Implementation Boundary

### Already built — preserve and reuse

| Area | Existing authority/seam | Planning consequence |
|------|-------------------------|----------------------|
| Repository graph | `RepositoryState`, `SavedRepository`, `SavedCheckout`, durable keys, roles, paths, branch facts, and `first_seen_order`. [VERIFIED: `baude-core/src/repository.rs`] | Do not add another hierarchy store or migration. |
| Admission/default worktree | Existing open/admission and branch activation paths create/reuse canonical children. [VERIFIED: Phase 5 summary; `baude/src/app.rs`] | UI only selects parent context and dispatches. |
| Close/reopen/remove | `close_retained_session`, `reopen_checkout`, `prepare_remove_worktree`, and `confirm_remove_worktree`. [VERIFIED: `baude/src/app.rs`] | Reuse these transactions; alter entry points and presentation only. |
| Safety blockers | `RemovalBlocker` covers management/topology, tracked/untracked/conflict, lock/prunable, and submodule cases; inspection failures remain indeterminate. [VERIFIED: `baude-core/src/git.rs:1661-1684`; `baude-core/src/lifecycle.rs:553-611`] | Map typed causes to exact copy; never parse debug text to authorize effects. |
| Shared lifecycle | Phase 6 records lifecycle and exact process ownership before effects and mirrors App/Manager contract tests. [VERIFIED: `06-07-SUMMARY.md`] | No owner-local state machine in hierarchy/UI code. |
| Remote compatibility | `RemoteInfo`, `RemoteSnapshot`, remote attach, and `/sessions` remain flat. [VERIFIED: `baude/src/remote.rs`; `bauded/src/api.rs`] | Production daemon/PWA changes are unnecessary. |

### Actual Phase 7 gaps

| Gap | Evidence | Required change |
|-----|----------|-----------------|
| Local selection is runtime-only | `SelId::Local(u64)` and `selected()` resolve only `Session`. [VERIFIED: `baude/src/app.rs:195-200,1476-1487`] | Use `SelId::{Repository(RepositoryKey), Checkout(CheckoutKey), Remote(u64)}` and explicit checkout-to-runtime lookup. |
| Local ordering is status-grouped | Active/remote/archive groups are sorted by names. [VERIFIED: `baude/src/app.rs:1414-1447`] | Project structural local order first; preserve remote's existing compatibility order separately. |
| Sidebar is flat | Local rows come exclusively from `App.sessions`; archived rows move under `▼ archived`. [VERIFIED: `baude/src/ui.rs:142-218`] | Render parents and all durable children in place; remove only the local archive section. |
| `w` depends on a live session | Repository context is obtained from `selected().repo_root`. [VERIFIED: `baude/src/app.rs:2554-2573`] | Resolve repository context from parent/child durable selection. |
| Close and remove are conflated | A worktree close modal offers `k keep` and `r remove`. [VERIFIED: `baude/src/app.rs:2663-2695`; `baude/src/ui.rs:1046-1064`] | `x` is retained close only; Shift+X is a separate preflight and red confirmation. |
| Retry authorization is implicit | App has activation/teardown retry methods, but core exposes no presentation capability. [VERIFIED: codebase grep] | Add a pure `LifecycleCapability` projection; expose only actions with an implemented App dispatch path. |
| Narrow layout can resize hidden panes | Sidebar is `min(42,width/3)` and App always resizes to inner content dimensions. [VERIFIED: `baude/src/ui.rs:26-49`; `baude/src/app.rs:2282-2300`] | Implement the UI-spec breakpoints and retain last valid PTY size when content is hidden/too small. |
| Unicode sizing is naive | Truncation and padding use `chars().count()`. [VERIFIED: `baude/src/ui.rs:103-110,257-268,395-400`] | Use Ratatui `Line::width()`/grapheme-aware rendering and bounded `Buffer`/widget APIs. |
| Package check is blocked | Root workspace dependency is path-only; the observed package probe rejected it for lacking a version. [VERIFIED: local `cargo package` probe; `Cargo.toml:15`] | Set a versioned path dependency synchronized to the beta package version. |

## Standard Stack

### Core

| Library/tool | Version | Purpose | Why standard here |
|--------------|---------|---------|-------------------|
| Rust/Cargo | 1.98.0 on research host | Build, test, package, release artifacts | Existing workspace and available toolchain. [VERIFIED: local environment probe] |
| Ratatui | 0.30.0 | TUI layout, lines, styles, buffers, test backend | Existing pinned dependency and locked UI contract; no replacement is authorized. [VERIFIED: `baude/Cargo.toml`; `07-UI-SPEC.md`] |
| Git CLI | 2.50.1 on research host | Real repository/worktree fixture and dogfood operations | Existing production Git adapter executes explicit argument vectors. [VERIFIED: local environment probe; `baude-core/src/git.rs`] |
| Cargo package/build | Cargo 1.98.0 | Source package and binary artifact readiness | `cargo package` creates a distributable crate archive and normally verifies a pristine build. [CITED: https://doc.rust-lang.org/cargo/commands/cargo-package.html] |

### Supporting, already present

| API | Purpose | When to use |
|-----|---------|-------------|
| `ratatui::text::Line::width()` | Unicode display width of a composed line | Measure rendered row/padding/truncation budgets instead of `chars().count()`. [CITED: https://docs.rs/ratatui/0.30.0/ratatui/text/struct.Line.html#method.width] |
| `ratatui::backend::TestBackend` | Full-terminal in-memory render integration tests | Exercise the required viewport matrix and whole-screen mode switching. [CITED: https://docs.rs/ratatui/0.30.0/ratatui/backend/struct.TestBackend.html] |
| Direct `Buffer` assertions | Widget-level cell, symbol, color, background, and bound checks | Test rows/modals without snapshot dependencies. Ratatui recommends direct buffers for widget unit tests. [CITED: https://docs.rs/ratatui/0.30.0/ratatui/backend/struct.TestBackend.html] |
| `Buffer::cell`/`cell_mut` and bounded widget render | Non-panicking cell access | Verify small viewports without indexing outside the buffer. [CITED: https://docs.rs/ratatui/0.30.0/ratatui/buffer/struct.Buffer.html#method.cell] |

### Alternatives Considered

| Instead of | Could use | Tradeoff |
|------------|-----------|----------|
| Pure hierarchy projection | Render directly from `App.sessions` plus ad hoc repository lookups | Cannot represent runtime-less parents/children reliably and encourages duplicated sort/action logic; reject. [VERIFIED: codebase/UI contract mismatch] |
| Durable-key selection | Preserve runtime IDs and synthesize fake rows | Retained children have no runtime and runtime IDs change; reject. [VERIFIED: `RepositoryState` and App runtime map] |
| Direct buffer/TestBackend assertions | Add a snapshot-test crate | Snapshot dependency is explicitly unauthorized and unnecessary for the required semantic/style assertions; reject. [VERIFIED: `07-UI-SPEC.md`] |
| Explicit core capability | Infer retry from color, cause text, or missing runtime | Violates lifecycle authority and can advertise an action with no legal dispatch; reject. [VERIFIED: `07-UI-SPEC.md`] |

**Installation:** No new package or crate installation is required or authorized. [VERIFIED: `07-UI-SPEC.md`]

## Package Legitimacy Audit

Not applicable: Phase 7 should install no external package. Existing locked dependencies remain unchanged, so the Package Legitimacy Gate is not triggered. [VERIFIED: recommended stack and UI contract]

## Architecture Patterns

### System Architecture Diagram

```text
RepositoryState + optional App runtimes + flat RemoteSnapshot
                    |
                    v
        pure hierarchy/action projection
        |          |                 |
        |          |                 +--> flat remote rows (unchanged)
        |          +--> explicit lifecycle capabilities
        +--> ordered RepositoryKey / CheckoutKey rows
                    |
                    v
          Ratatui responsive renderer
                    |
          keyboard selection/action intent
                    |
                    v
       App durable-target dispatcher
       |       |        |          |
       w       x      enter/r      Shift+X
       |       |        |          |
       +-------+--------+----------+
                    |
                    v
     existing shared lifecycle + Git authority
                    |
                    v
     persist/effect outcome -> refresh same durable selection
```

The diagram separates projection, rendering, and mutation: status decorates rows but cannot determine identity, order, or lifecycle authorization. [VERIFIED: locked UI contract and Phase 6 authority]

### Recommended Project Structure and Change Surface

```text
baude-core/src/
└── lifecycle.rs                 # add pure, derived presentation capabilities + unit test
baude/src/
├── main.rs                      # register hierarchy module
├── hierarchy.rs                 # NEW: pure rows, sorting, labels, action view, fallback selection
├── app.rs                       # durable selection, target resolution, dispatch, resize, real-Git test
└── ui.rs                        # hierarchy/modal/hint rendering and viewport tests
bauded/src/
└── api.rs                       # compatibility test only; no production hierarchy API
.github/workflows/
└── ci.yml                       # non-publishing supported artifact-readiness matrix
Cargo.toml                       # versioned baude-core path dependency
baude-core/Cargo.toml            # 2.0.0-beta
baude/Cargo.toml                 # 2.0.0-beta
bauded/Cargo.toml                # 2.0.0-beta
Cargo.lock                       # synchronized local package versions
release-please-config.json       # exact beta target/prerelease readiness
.release-please-manifest.json    # intentionally UNCHANGED: last release remains 0.14.0
README.md                        # hierarchy/keys/source install wording
CHANGELOG.md                     # unreleased beta-readiness section, never “published”
docs/local-tui-dogfood.md        # NEW: isolated manual flow and evidence checklist
```

This is the complete recommended tracked surface. Do not modify daemon production routes, PWA files, `release.yml`, or repository persistence schema unless implementation proves an unanticipated compile-only need. [VERIFIED: scope and codebase boundaries]

### Pattern 1: Pure hierarchy projection before decoration

**What:** Build ordered parent/child view records from durable state, then join optional runtime status by `CheckoutKey`. Keep display labels and status outside identity. [VERIFIED: `07-UI-SPEC.md`]

**When to use:** Every draw, selection move, action hint, and post-transition fallback. [VERIFIED: UI contract]

```rust
// Source: project UI contract + RepositoryState types
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SelId {
    Repository(RepositoryKey),
    Checkout(CheckoutKey),
    Remote(u64),
}

fn local_rows(state: &RepositoryState) -> Vec<LocalRow> {
    // 1. Sort parents by case-folded display name, canonical main path, key.
    // 2. For each parent, sort children by first_seen_order, key.
    // 3. Only then join optional runtime/session presentation.
    todo!()
}
```

### Pattern 2: Capability-gated action projection

**What:** Core returns `RetryReopen` only for launchable retained state and `RetryRecovery` only for protected states that App can actually retry. The UI uses that value for hints and `r`; it never infers permission from glyphs or error strings. [VERIFIED: `07-UI-SPEC.md`; available App retry methods]

**Recommended first-cut mapping:** `Inactive -> RetryReopen`; activation recovery and teardown/stopped-active recovery -> `RetryRecovery`; removal tombstones and generic unavailable topology -> no retry until an App manual dispatch exists. [VERIFIED: `baude/src/app.rs:992-1055`; `baude-core/src/lifecycle.rs:380-409`]

### Pattern 3: Stable in-process selection reconciliation and deterministic restart initialization

**What:** Preserve the selected durable key after every in-process refresh/status transition. On successful child removal choose next sibling, else previous sibling, else parent. Selection is not part of persisted `RepositoryState`, so do not expand the schema or claim restart restoration: a newly restored process selects the first local repository parent in rendered name/path/key order; if there is no local repository, it selects the first flat remote row in existing compatibility order; if no row exists, selection is empty. [VERIFIED: `07-UI-SPEC.md` rendered order; current `App` selection is in-memory]

**When to use:** Tick refresh, close/reopen/archive, admission, activation, remote refresh, and removal success. [VERIFIED: UI behavioral matrix]

### Pattern 4: Visibility-aware resize

**What:** Layout returns pane visibility plus a valid non-zero content rectangle. `App::sync_sizes` resizes only a visible pane with positive inner dimensions; otherwise it retains the previous valid PTY size. At height `<13`, transfer focus away from a hidden shell exactly as the UI contract specifies. [VERIFIED: `07-UI-SPEC.md`]

### Pattern 5: Release readiness without release action

**What:** Build, package, extract, version-check, and document locally/CI. Never invoke the release-please action, `gh release`, push, or tag creation. [VERIFIED: scope; `.github/workflows/release*.yml`]

**Metadata rule:** set the three package versions and exact versioned path dependency to `2.0.0-beta`. Preserve the project's single root package, `release-type: simple`, changelog/tag/bump/draft fields, and three TOML `extra-files`; set exactly `release-as: 2.0.0-beta`, `versioning: prerelease`, `prerelease: true`, and `prerelease-type: beta`. The official current schema defines all four fields, and the current versioning documentation requires `versioning: prerelease` together with `prerelease: true` to create prerelease versions. Leave `.release-please-manifest.json` exactly `{ ".": "0.14.0" }` because it records the last released version and official docs permit manual manifest edits only for bootstrap. Do not run release-please. [VERIFIED: current `release-please-config.json`; CITED: official config schema, customizing, and manifest-releaser docs]

### Anti-Patterns to Avoid

- **Status-based local ordering:** archived, waiting, working, exited, and recovery are decorations, never sort/group inputs. [VERIFIED: `07-UI-SPEC.md`]
- **Runtime identity as checkout identity:** a runtime can be absent or replaced; use `CheckoutKey`. [VERIFIED: repository model and Phase 6 summary]
- **UI parsing `Display` text:** translate typed errors/capabilities centrally; strings are presentation, not authorization. [VERIFIED: lifecycle authority contract]
- **Combined close/remove modal:** `x` retains; Shift+X physically removes only after separate preflights. [VERIFIED: `07-UI-SPEC.md`]
- **Optimistic row deletion:** remove a child only after lifecycle success; degraded or refused outcomes keep selection and truthful state. [VERIFIED: `07-UI-SPEC.md`]
- **Editing the release manifest to fake readiness:** it is release history, not current source version. [CITED: https://github.com/googleapis/release-please/blob/main/docs/manifest-releaser.md]
- **Running `release.yml` as a check:** it pushes images and uploads to an existing GitHub Release; build equivalent artifacts in CI without those publication steps. [VERIFIED: `.github/workflows/release.yml`]

## Don't Hand-Roll

| Problem | Don't build | Use instead | Why |
|---------|-------------|-------------|-----|
| Repository/worktree truth | A UI-local repository graph | Existing `RepositoryState` and durable keys | Persistence, canonical identity, roles, and first-seen ordering already exist. [VERIFIED: `baude-core/src/repository.rs`] |
| Lifecycle permission | UI booleans inferred from status/copy | Pure capability derived from `CheckoutLifecycle` | Keeps authority in core and prevents unsafe stale action hints. [VERIFIED: Phase 6/UI contract] |
| Branch validation/worktree mutation | String regexes or shell command strings | Existing `baude_core::git` and lifecycle activation | Git literals, occupancy, collision, compensation, and topology verification are already handled. [VERIFIED: `baude-core/src/git.rs`] |
| Safe removal | Recursive delete or one-time cleanliness check | Existing two-preflight verified removal transaction | It handles TOCTOU, exact topology, stop/compensation, branch retention, and degraded persistence. [VERIFIED: `baude-core/src/lifecycle.rs`; App tests] |
| Unicode cell width | Byte length or `chars().count()` | Ratatui `Line::width`, grapheme iteration, and bounded rendering | Terminal display width differs from scalar/byte count. [CITED: https://docs.rs/ratatui/0.30.0/ratatui/text/struct.Line.html] |
| Render snapshots | A new snapshot framework | Existing Ratatui `TestBackend` and direct `Buffer` assertions | Official APIs already cover integration and widget-level tests. [CITED: https://docs.rs/ratatui/0.30.0/ratatui/backend/struct.TestBackend.html] |
| Package tarball semantics | Custom source archive scripts | `cargo package` | Cargo normalizes manifests, includes lock/VCS data, extracts, and normally verifies pristine builds. [CITED: https://doc.rust-lang.org/cargo/commands/cargo-package.html] |

**Key insight:** nearly every difficult safety primitive already exists; Phase 7 succeeds by making identity, projection, and action routing explicit rather than duplicating domain behavior in renderer code. [VERIFIED: codebase synthesis]

## Runtime State Inventory

Phase 7 refactors local selection/presentation and changes source package versions, so runtime state was checked even though no durable schema rename is planned. [VERIFIED: phase scope]

| Category | Items Found | Action Required |
|----------|-------------|-----------------|
| Stored data | Workspace repository state already stores repositories, checkouts, lifecycle, and runtime ownership; `selected_id` is an App field and is not part of `RepositoryState`. [VERIFIED: `baude-core/src/repository.rs`; `baude/src/app.rs`] | No data migration. Retain durable-key selection through in-process refreshes; after restart choose first local repository parent in rendered order, else first flat remote row, else none. |
| Live service config | Flat daemon state and API remain external to the local hierarchy; no hierarchy configuration lives in a service UI/database. [VERIFIED: daemon source and locked scope] | No live-service migration. Run compatibility tests only. |
| OS-registered state | Existing globally/mise-installed `baude` binaries may remain 0.14.0; the phase does not register launchd/systemd tasks. [VERIFIED: README install model and repository scan] | Dogfood the explicitly built local binary or isolated `cargo install --root`; do not overwrite or claim the user's installed release. |
| Secrets/env vars | No secret key or environment-variable name is renamed. Dogfood isolation changes `HOME` only for the launched fixture process and requires no credential. [VERIFIED: phase scope/runbook recommendation] | No secret migration; do not add or print credentials. |
| Build artifacts | Existing `target/` binaries and package archives carry 0.14.0 until Cargo rebuilds them; release images/assets are publication outputs and must remain untouched. [VERIFIED: current manifests and workflows] | Rebuild with `--locked`, recreate local archives, and assert extracted `--version`; do not push images or upload assets. |

**Canonical result:** after repository files change, no database, service UI, OS registration, or secret name needs migration; only local build/install artifacts can retain the old source version. [VERIFIED: inventory above]

## Common Pitfalls

### Pitfall 1: Retained children disappear
**What goes wrong:** Closing a runtime removes the only flat `Session` row.  
**Why it happens:** Rendering iterates `App.sessions`, not durable checkouts. [VERIFIED: `baude/src/ui.rs`]  
**How to avoid:** Project from `RepositoryState`; runtime is optional decoration.  
**Warning sign:** A parent or child count changes after close without a removal outcome.

### Pitfall 2: Selection jumps after status/archive changes
**What goes wrong:** Selected row index now points at another target.  
**Why it happens:** The current ordering regroups active/archive records and stores runtime IDs. [VERIFIED: `baude/src/app.rs`]  
**How to avoid:** Store a durable selection key and recompute its row index after projection.  
**Warning sign:** Tests assert row indices rather than keys.

### Pitfall 3: `r` advertises nonexistent recovery
**What goes wrong:** UI tells a user to retry a state that App cannot legally dispatch.  
**Why it happens:** `startup_recovery_program` names more categories than App's current explicit retry methods. [VERIFIED: codebase grep]  
**How to avoid:** Capability means “an implemented manual dispatch exists,” not merely “state is protected.”  
**Warning sign:** Capability mapping has no exhaustive action-dispatch test.

### Pitfall 4: Shift+X collapses into lowercase x
**What goes wrong:** A destructive remove path becomes reachable through ordinary close.  
**Why it happens:** Matching only `KeyCode::Char('x')` without checking `SHIFT`, or preserving the existing combined modal. [VERIFIED: current handler/UI]  
**How to avoid:** Match uppercase/Shift+X distinctly and test every selection kind with zero mutation on refusal.  
**Warning sign:** Close confirmation contains “remove,” or red styling is used for retained close.

### Pitfall 5: Narrow panes receive zero dimensions
**What goes wrong:** PTY resize or rendering becomes invalid when sidebar-only mode hides content.  
**Why it happens:** Saturating geometry avoids arithmetic panic but still produces zero-sized inner rectangles. [VERIFIED: current layout/resize code]  
**How to avoid:** Model visibility separately and resize only positive visible inner rectangles.  
**Warning sign:** Resize tests assert only “no panic,” not recorded PTY dimensions/focus transfer.

### Pitfall 6: Filtered tests silently run zero tests
**What goes wrong:** Cargo exits successfully because the filter matches nothing.  
**Why it happens:** Cargo filters are substring-based unless the harness gets `--exact`, and a missing test is not inherently a failure. [VERIFIED: observed Cargo behavior in project workflow]  
**How to avoid:** List first, require an exact `name: test` line with `rg -Fx`, then run the same fully qualified name with `--exact`.  
**Warning sign:** A plan uses only `cargo test <short-name>`.

### Pitfall 7: Package “verification” depends on an unpublished workspace crate
**What goes wrong:** Packaged `baude`/`bauded` tries to resolve `baude-core` from the registry during pristine verification.  
**Why it happens:** Published packages ignore `path` and use the required registry `version`. [CITED: https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#multiple-locations]  
**How to avoid:** Add the exact versioned path dependency; fully verify `baude-core`, assemble all workspace packages with `--no-verify`, and separately build/test the workspace and release binaries. Do not publish merely to satisfy this gate.  
**Warning sign:** Treating `--no-verify` alone as complete release evidence.

### Pitfall 8: Readiness metadata falsely claims release
**What goes wrong:** Docs/changelog say “released,” or the manifest is advanced before a release exists.  
**Why it happens:** Source version, proposed version, and last-released version are conflated. [CITED: https://github.com/googleapis/release-please/blob/main/docs/manifest-releaser.md]  
**How to avoid:** Say “ready for v2.0.0-beta,” keep manifest at 0.14.0, and do not run push/tag/release workflows.  
**Warning sign:** Any local gate invokes `gh`, `git push`, release-please, or the release workflow.

## Code Examples

### Unicode-aware line measurement

```rust
// Source: https://docs.rs/ratatui/0.30.0/ratatui/text/struct.Line.html#method.width
use ratatui::text::Line;

let row = Line::from("  └─ ● repo:feature/界");
let occupied_cells = row.width();
```

### Whole-TUI viewport test

```rust
// Source: https://docs.rs/ratatui/0.30.0/ratatui/backend/struct.TestBackend.html
use ratatui::{backend::TestBackend, Terminal};

let backend = TestBackend::new(59, 20);
let mut terminal = Terminal::new(backend).unwrap();
terminal.draw(|frame| crate::ui::draw(frame, &app)).unwrap();
let buffer = terminal.backend().buffer();
assert_eq!(buffer.area.width, 59);
```

### Exact-test zero-match guard

```bash
name='app::tests::local_tui_dogfood_real_git_flow_survives_restart_without_duplicates'
cargo test -p baude -- --list | rg -Fx -- "$name: test"
cargo test -p baude "$name" -- --exact --nocapture
```

The first command is the guard; the second is not accepted as evidence unless the guard succeeds. [VERIFIED: user validation constraint]

## Dogfood Harness and Release Gate

### Isolated automated dogfood

Add one App-level real-Git test that creates a temporary bare origin and main checkout, sets test-only persistence root and a sleeping fake backend, then executes open/admit → create/activate → close → drop/reload App → reopen exactly once → clean remove. Assert repository/child keys and order at every stage, branch survival with `git show-ref --verify`, no duplicate runtime mapping, and no writes outside the fixture. Reuse/factor the existing `admission_repo` and `removal_app` helpers rather than creating a second Git-fixture dialect. [VERIFIED: existing App test helpers and REL-01]

### Isolated manual TUI dogfood

`docs/local-tui-dogfood.md` should create a temporary HOME and repository, build the local binaries, launch with a harmless fake backend, and walk the exact wide/narrow interaction sequence. The runbook must record paths, branch ref, selected durable target, wide/narrow screenshots, and before/after `git worktree list --porcelain`; it must end with local cleanup instructions and contain no push/publish command. [VERIFIED: `07-UI-SPEC.md` manual pass]

### Non-publishing release-readiness commands

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Source package readiness. baude-core can receive pristine package verification;
# dependents remain unpublishable in the registry until baude-core exists there.
cargo package -p baude-core --locked
cargo package --workspace --locked --no-verify

# Host artifact and extracted version/readability check.
cargo build --workspace --release --locked
test "$(target/release/baude --version)" = 'baude 2.0.0-beta'
test "$(target/release/bauded --version)" = 'bauded 2.0.0-beta'
```

The CI readiness job should reuse the four existing release targets (`aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`) and the existing `baude`+`bauded` tar layout, but stop after build/archive/extract/version checks and never log in, push, upload, tag, or create a release. [VERIFIED: `.github/workflows/release.yml:81-112`; locked scope]

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Built-in Rust test harness via Cargo 1.98.0; Ratatui 0.30 `TestBackend`/`Buffer`. [VERIFIED: repository and environment] |
| Config file | None; workspace manifests define packages. [VERIFIED: file scan] |
| Quick run command | Exact fully qualified test with list guard, as shown below. |
| Full suite command | `cargo test --workspace` |

### Exact New Tests

| Fully qualified test | File | Covers |
|----------------------|------|--------|
| `lifecycle::tests::lifecycle_capabilities_expose_only_dispatchable_reopen_and_recovery_actions` | `baude-core/src/lifecycle.rs` | Explicit RetryReopen/RetryRecovery mapping and negative states. |
| `hierarchy::tests::local_hierarchy_orders_parents_and_children_by_durable_identity` | `baude/src/hierarchy.rs` | REPO-05, HIER-01/02/03. |
| `hierarchy::tests::local_hierarchy_order_ignores_runtime_and_session_status` | `baude/src/hierarchy.rs` | HIER-04, archive/attention stability. |
| `hierarchy::tests::local_hierarchy_selection_survives_refresh_and_removal_falls_back_locally` | `baude/src/hierarchy.rs` | Durable selection and deterministic sibling/parent fallback. |
| `app::tests::hierarchy_action_matrix_dispatches_only_authorized_local_actions` | `baude/src/app.rs` | WORK-01–06, SURF-01/02, refusal no-mutation matrix. |
| `app::tests::hierarchy_flat_remote_compatibility_has_no_local_parent_or_remove_action` | `baude/src/app.rs` | SURF-05. |
| `app::tests::hierarchy_resize_never_sends_zero_dimensions_and_transfers_hidden_shell_focus` | `baude/src/app.rs` | Responsive resize/focus contract. |
| `app::tests::local_tui_dogfood_real_git_flow_survives_restart_without_duplicates` | `baude/src/app.rs` | REL-01 end-to-end real Git. |
| `ui::tests::hierarchy_viewport_matrix_renders_without_panic_and_preserves_semantics` | `baude/src/ui.rs` | Required 160×40, 100×30, 79×24, 59×20, 40×12 matrix. |
| `ui::tests::hierarchy_tracer_renders_real_app_parent_and_child` | `baude/src/ui.rs` | First production tracer reaches real App state and renders parent/main/default durable rows. |
| `ui::tests::hierarchy_modals_name_exact_targets_and_distinguish_close_from_remove` | `baude/src/ui.rs` | Exact copy, target/path/ref, red remove vs non-red close. |
| `ui::tests::hierarchy_copy_contract_matches_ui_spec_for_empty_pending_success_and_hints` | `baude/src/ui.rs` | All remaining exact empty, persistence, pending, success, no-runtime, full-hint, and narrow-hint strings. |
| `ui::tests::hierarchy_unicode_width_scroll_and_selection_band_are_cell_correct` | `baude/src/ui.rs` | Wide/combining labels, viewport scroll, two-line background. |
| `api::tests::flat_session_api_remains_a_non_hierarchical_compatibility_projection` | `bauded/src/api.rs` | SURF-05 route shape and no hierarchy/remove-worktree endpoint. |

These names are prescriptions for Wave 0; they do not exist at research time. [VERIFIED: current `cargo test -- --list` and source scan]

### Existing Regression Tests to Reuse

| Test | Purpose |
|------|---------|
| `app::tests::lifecycle_create_activate_local_persists_once_and_reuses_runtime` | Branch activation/reuse authority. [VERIFIED: test list] |
| `app::tests::lifecycle_close_local_snapshots_resume_context_and_retains_hierarchy` | Retained close. [VERIFIED: test list] |
| `app::tests::lifecycle_reopen_local_targets_retained_checkout_once_and_obeys_save_boundary` | Reopen exactly once. [VERIFIED: test list] |
| `app::tests::lifecycle_remove_clean_local_rechecks_after_stop_and_compensates_a_race` | Second-preflight race/compensation. [VERIFIED: test list] |
| `app::tests::remove_confirmation_is_distinct_targeted_and_cancel_is_non_mutating` | Existing distinct target/cancel seam. [VERIFIED: test list] |
| `git::tests::removal::only_exact_managed_linked_topology_produces_an_opaque_target` and sibling blocker tests | Fail-closed Git categories. [VERIFIED: source test list] |
| `manager::tests::lifecycle_protocol_contract_manager_vectors` and App counterpart | Shared adapter authority remains intact. [VERIFIED: test list] |
| `api::tests::real_atomic_persistence_failures_are_503_for_every_mutation` | Flat API persistence compatibility. [VERIFIED: test list] |

### Phase Requirements → Test Map

| Requirements | Automated command | File exists? |
|--------------|-------------------|--------------|
| REPO-05, HIER-01–04 | guarded exact hierarchy projection tests | ❌ Wave 0 (`baude/src/hierarchy.rs`) |
| WORK-01–06, SURF-01/02 | guarded exact action matrix plus existing lifecycle/Git tests | ❌ new App matrix; ✅ lifecycle regressions |
| SURF-05 | guarded App remote and daemon API compatibility tests | ❌ new tests; ✅ existing flat API tests |
| REL-01 | guarded exact real-Git dogfood test + end-of-phase manual runbook | ❌ Wave 0/new docs |
| REL-02 | full local gate, packages, host artifacts, CI target matrix | ❌ CI readiness job/package fix |
| REL-03 | version assertions, package metadata inspection, docs copy review | ❌ beta metadata/docs edits |

### Sampling Rate

- **Per task commit:** list-guarded exact new test plus directly affected existing regression test. [VERIFIED: requested validation policy]
- **Per wave merge:** `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`. [VERIFIED: existing CI convention]
- **Phase implementation gate:** full suite, package/archive checks, host artifact version checks, and clean working-tree diff review. [VERIFIED: REL-02]
- **Deferred certification gate:** supported CI artifact matrix, manual wide/narrow dogfood, Linux/runtime certification, independent review, `/gsd-verify-work`, and publication decision. These are not implementation-task blockers. [VERIFIED: user-provided scope]

### Zero-Match Guard Template

For every exact test in a plan, use the fully qualified name twice:

```bash
name='hierarchy::tests::local_hierarchy_orders_parents_and_children_by_durable_identity'
cargo test -p baude -- --list | rg -Fx -- "$name: test"
cargo test -p baude "$name" -- --exact --nocapture
```

For `baude-core`, substitute `-p baude-core`; for daemon tests, use `-p bauded`. A missing guard match fails the task even if the second Cargo command exits zero. [VERIFIED: user constraint]

### Wave 0 Gaps

- [ ] `baude/src/hierarchy.rs` projection fixtures/tests.
- [ ] Core lifecycle capability test.
- [ ] App action/resize/real-Git dogfood tests.
- [ ] Ratatui tracer/viewport/modal/copy-contract/Unicode tests.
- [ ] Flat daemon compatibility test.
- [ ] CI artifact-readiness job and package dependency fix.
- [ ] Isolated manual dogfood runbook.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|-------------|-----------|---------|----------|
| Git | real-repository fixtures/dogfood | ✓ | 2.50.1 Apple Git | None; blocking if absent. [VERIFIED: environment probe] |
| Rust/Cargo | implementation/tests/package/artifacts | ✓ | 1.98.0 | Existing CI macOS/Linux runners. [VERIFIED: environment probe; CI workflow] |
| Docker CLI | existing daemon CI only, not local hierarchy | ✓ | 29.7.2 | Skip for Phase 7 quick tests; existing CI owns daemon smoke. [VERIFIED: environment probe; CI workflow] |
| BSD tar | host artifact archive/extraction | ✓ | 3.5.3/libarchive 3.7.4 | CI runner tar. [VERIFIED: environment probe] |
| `shasum` | optional local checksum evidence | ✓ | 6.02 | CI uses the same command in release workflow. [VERIFIED: environment probe; release workflow] |

**Missing dependencies with no fallback:** None on the research host. [VERIFIED: environment probe]

**Missing dependencies with fallback:** None. Supported non-host target execution remains a deferred CI/certification gate rather than a local dependency assumption. [VERIFIED: user scope and CI target matrix]

## Security Domain

OWASP describes ASVS as a web-application verification standard; this phase is a local TUI, so web authentication/session categories do not newly apply. Input validation and OS command-injection controls do apply to branch/path-driven Git operations. [CITED: https://owasp.org/www-project-application-security-verification-standard/]

### Applicable ASVS Categories

| ASVS Category | Applies | Standard control |
|---------------|---------|------------------|
| V2 Authentication | No new control | Local process inherits existing OS/user authority; no auth surface is added. [VERIFIED: phase scope] |
| V3 Session Management | No new web session control | Preserve existing PTY/session ownership and workspace separation. [VERIFIED: Phase 6 summary] |
| V4 Access Control | Yes, domain authorization | Durable target identity plus explicit lifecycle capability gates every mutation; remote rows cannot reach local remove. [VERIFIED: UI contract] |
| V5 Input Validation | Yes | Existing literal Git branch validation, canonical repository discovery, managed-path collision checks, and typed fail-closed removal inspection. [VERIFIED: `baude-core/src/git.rs`] |
| V6 Cryptography | No new control | No cryptographic behavior changes; do not modify daemon crypto/PWA code. [VERIFIED: scope] |

### Known Threat Patterns for This Stack

| Pattern | STRIDE | Standard mitigation |
|---------|--------|---------------------|
| Branch/path OS command injection | Tampering/Elevation | Continue `Command::args` literal vectors and Git's ref validation; never introduce `sh -c` with user branch/path text. OWASP ASVS v5 specifically recommends parameterized OS queries/contextual encoding. [CITED: https://owasp.org/www-project-application-security-verification-standard/] |
| Stale selection targets wrong checkout | Tampering | Resolve `RepositoryKey`/`CheckoutKey` at dispatch and name the resolved target in confirmation. [VERIFIED: UI contract] |
| Removal TOCTOU loses work | Tampering/Denial | Existing first preflight, explicit confirmation, runtime stop, fresh second preflight, verified Git remove, and compensation. [VERIFIED: lifecycle code/tests] |
| Remote/local confused deputy | Elevation/Tampering | Distinct `SelId::Remote`, separate action matrix, no local Shift+X dispatch for remote rows. [VERIFIED: UI contract] |
| Hidden zero-size pane captures input | Spoofing/Denial | Transfer focus from hidden shell and never route ordinary keys to a non-visible pane. [VERIFIED: UI contract] |
| False release provenance | Spoofing | Keep last-release manifest truthful and avoid tag/release/upload commands during readiness. [CITED: release-please manifest docs]

## State of the Art

| Old/current approach | Phase 7 approach | Impact |
|----------------------|------------------|--------|
| Flat runtime rows with archived regrouping. [VERIFIED: current source] | Durable repository/checkout projection with status decoration. [VERIFIED: UI contract] | Runtime absence and archive no longer erase/reorder topology. |
| Runtime ID selection. [VERIFIED: current source] | Repository/checkout durable-key selection. [VERIFIED: repository model] | Retained and parent rows become actionable without fake runtimes. |
| Combined worktree keep/remove prompt. [VERIFIED: current source] | Separate retained close and safe physical removal. [VERIFIED: UI contract] | Destructive intent is explicit and branch retention is clear. |
| Naive scalar-count truncation. [VERIFIED: current source] | Ratatui Unicode-width/grapheme APIs. [CITED: Ratatui Line docs] | Wide/combining labels fit terminal cells predictably. |
| Release workflow used only after publication event. [VERIFIED: current workflow] | Non-publishing CI artifact-readiness mirror. [VERIFIED: scope] | Supported builds can be certified before any release exists. |

**Deprecated/outdated for this phase:** local archived section, “kill” wording for retained close, combined close/remove modal, runtime-only selection, and path-only packaged `baude-core` dependency. [VERIFIED: current source vs locked contract/Cargo docs]

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | After adding an exact version to the local `baude-core` path dependency, `cargo package --workspace --locked --no-verify` will assemble all three packages. [ASSUMED] | Dogfood Harness and Release Gate | If Cargo exposes another metadata/package blocker, the plan needs a small manifest correction before REL-02 can pass. |

No recommendation depends on an unverified third-party package. [VERIFIED: no-install stack]

## Open Questions (RESOLVED)

1. **When will deferred platform/runtime certification run?**
   - Resolution: certification runs in the morning, after implementation/local automated evidence exists. Until then Linux/runtime certification, independent deep review, phase verification, Nyquist/UI approval, requirement completion, phase completion, and publication remain pending. [VERIFIED: user decision]

2. **Where should manual screenshots/evidence be recorded?**
   - Resolution: record observed manual wide/narrow screenshots, commands, and certification outcomes in `.planning/phases/07-local-tui-dogfood-release/07-UAT-EVIDENCE.md`. Create that file only when evidence actually exists; planning and implementation must not create an empty or anticipatory evidence artifact. [VERIFIED: user decision]

## Sources

### Primary (HIGH confidence)

- Project source: `baude-core/src/{repository,lifecycle,git}.rs`, `baude/src/{app,ui,remote,main}.rs`, `bauded/src/{manager,api}.rs` — current identity, lifecycle, rendering, routing, compatibility, and test seams. [VERIFIED: codebase inspection]
- Project planning: `REQUIREMENTS.md`, `07-UI-SPEC.md`, and Phase 5/6 summaries — locked acceptance and already-built boundaries. [VERIFIED: project files]
- https://docs.rs/ratatui/0.30.0/ratatui/text/struct.Line.html — Unicode width/grapheme APIs. [CITED: official API docs]
- https://docs.rs/ratatui/0.30.0/ratatui/backend/struct.TestBackend.html — integration test backend and direct-buffer recommendation. [CITED: official API docs]
- https://docs.rs/ratatui/0.30.0/ratatui/buffer/struct.Buffer.html — safe cell access and bounded rendering. [CITED: official API docs]
- https://doc.rust-lang.org/cargo/commands/cargo-package.html — package steps, path dependency rule, verification, and `--no-verify`. [CITED: official Cargo docs]
- https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html — versioned path dependencies and prerelease exact-version guidance. [CITED: official Cargo docs]
- https://github.com/googleapis/release-please/blob/main/docs/manifest-releaser.md — manifest meaning, `release-as`, prerelease configuration, and cargo workspace notes. [CITED: official project docs]
- https://github.com/googleapis/release-please/blob/main/docs/customizing.md — Rust release type, prerelease strategy, and TOML updater. [CITED: official project docs]
- https://owasp.org/www-project-application-security-verification-standard/ — ASVS 5.0 applicability and OS command injection control example. [CITED: official OWASP project]

### Secondary (MEDIUM confidence)

- None. No community-only source was needed. [VERIFIED: research log]

### Tertiary (LOW confidence)

- None. [VERIFIED: research log]

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — existing locked stack and official Ratatui/Cargo docs. [VERIFIED: manifests/docs]
- Architecture: HIGH — directly derived from durable model, current handlers, Phase 6 authority, and locked UI contract. [VERIFIED: codebase/planning]
- Pitfalls: HIGH — each is observable in current code or explicitly guarded by official/project contracts. [VERIFIED: codebase/docs]
- Release readiness: MEDIUM-HIGH — metadata semantics are official; the corrected workspace package assembly remains unexecuted until implementation changes the manifest. [VERIFIED: official docs; A1 disclosed]

**Research date:** 2026-08-30  
**Valid until:** 2026-09-29 (stable pinned stack; re-check release tooling if workflows change)
