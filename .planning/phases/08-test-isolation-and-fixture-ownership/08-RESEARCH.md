# Phase 8: Test Isolation and Fixture Ownership - Research

**Researched:** 2026-09-13
**Domain:** Rust test isolation (cross-crate cfg gating, thread-local redirects, RAII guards) + filesystem leak forensics/preview tooling
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Redirect Mechanism**
- The new config redirect is an **RAII guard type** that resets on drop. The two
  existing redirects are bare `thread_local!` + setter with no reset
  (`git.rs:1727`, `hook.rs:91`), which is why arming leaks across tests within a
  binary.
- **Refactor `bauded/src/push.rs` to call `persist::config_dir()`** before adding
  the redirect. It currently duplicates `config_base()` at `push.rs:27-33`, so a
  baude-core-only hook would not cover VAPID keys (`push.rs:24`) or subscriptions
  (`push.rs:189`).
- `meta::claude_config_dir()` (`meta.rs:24`) is isolated by **thread-local
  redirect**, consistent with the guard above. This avoids reshaping
  `ClaudeMeta::poll`'s two call sites (`meta.rs:217`, `meta.rs:261`), which have
  no `_at` variant today.
- **Unify all four redirects under one guard struct** — worktrees base, hook
  command, config dir, claude config dir. One place to arm, one place to reset.
  This is what fixes the per-binary arming gap in Success Criterion 3.

**Workspace Identity**
- Replace the process-wide `ACTIVE: OnceLock<Workspace>` (`workspace.rs:199`)
  with a **thread-local override checked first, `OnceLock` retained as the
  production fallback**. Same pattern as the redirect guard.
- **Inject config into `initialize()`** as a parameter. Production passes
  `load_config()`; fixtures pass a literal. This removes the real-config read
  from the identity path entirely, rather than merely redirecting it.
- **`active()` keeps its current signature** (`workspace.rs:221`). The resolution
  change is internal, so there is no call-site churn and no risk to the path
  composition at `git.rs:1778` (`managed_default_worktree_path`).
- A test that reaches `active()` with no override set **panics via the escape
  guard**. Silent fallback to config-derived identity is exactly what pins the
  process today.

**Escape Guard**
- The guard is **armed unconditionally under `#[cfg(test)]`**, not as a side
  effect of `set_worktrees_base_for_test` (`git.rs:1747`). Every test binary is
  armed from the first instruction, independent of fixture order — this is the
  named defect in Success Criterion 3 (`baude-core`'s own tests never arm it).
- **Panic on escape**, as today (`git.rs:1753`). A test that reaches the real
  data dir has already failed; a `Result` would let call sites swallow it.
- The guard covers **all five paths**: worktrees base, config dir, claude config
  dir, state dir, and the push/VAPID store.
- **`#[cfg(test)]`-only compilation.** Zero production cost, and the real data
  dir is the correct target in production.

**Leak Preview and Removal Tooling (TISO-04)**
- **CLI subcommand** (`baude worktrees scan` / `--prune`). Scriptable, works
  headless, no TUI state to thread through.
- **Ownership is proven by strict path-shape match** under the real worktrees
  base: `<base>/<workspace>/repository-<u64>/…` exactly as composed at
  `git.rs:1776`. Workspace state has zero record of the leaked directories, so
  ownership cannot be sourced from state.
- **Two-step approval**: `scan` prints; `--prune` requires an explicit `--yes`
  and re-verifies ownership at removal time.
- **Preview only by default, never delete.** Matches the PROJECT.md constraint:
  "never automatically delete real user worktrees; cleanup requires preview and
  manual approval."

### Claude's Discretion
- The re-exec'd real-git dogfood child (`app.rs:7643`) runs as a spawned process,
  so `#[cfg(test)]`-only arming does not reach it. It already receives
  `BAUDE_TEST_FIXTURE_ROOT`. Planning should confirm that is sufficient; if it is
  not, adding a runtime flag is an acceptable deviation to raise rather than a
  settled decision.
- Whether the ~12-line fixture preamble copy-pasted 10 times in
  `bauded/src/manager.rs` (`:2615, 2698, 2729, 2811, 2843, 2933, 3049, 3136,
  3215, 3521`) is extracted into a shared helper as part of this phase, or left
  for a follow-up. Extraction is favoured — it is the natural consumer of the new
  unified guard.
- Exact CLI noun/verb naming for the TISO-04 subcommand.

### Deferred Ideas (OUT OF SCOPE)
- Actually deleting the 1433 leaked directories. The tooling built in this phase
  produces the preview; the deletion is a separate, explicitly approved
  operation and is not part of phase execution.
- Cleaning the orphaned `.state-*.json.tmp-*` files in the real config dir, and
  any hardening of the atomic-write cleanup path that produced them.
- Rotating the VAPID keypair that tests wrote into the real config dir.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| TISO-01 (remainder) | Config and state files confined to a test-owned temp root | §Pattern 1 (unified `TestRedirect` guard), §Pattern 2 (cross-crate cfg gate), §Don't Hand-Roll; the `config_base()`/`claude_config_dir()`/`push.rs::config_base()` call-site inventory in §Integration Surface |
| TISO-02 (remainder) | Concurrent fixtures do not share cached workspace identity | §Pattern 3 (thread-local `&'static` override via `Box::leak`, verified to preserve `active()`'s signature); §Finding 2 (libtest gives each test its own thread) |
| TISO-03 (remainder) | Failing test on escape, covering config/state/`~/.claude`, armed independently of fixture order | §Finding 1 (`cfg(test)` does **not** reach dependency crates — the locked decision as written cannot work); §Pattern 2 gives a mechanism that does; §Pattern 4 (containment predicate replaces the arming `AtomicBool`) |
| TISO-04 | Preview leaked worktrees; removal requires verified ownership + separate approval, never a missing gitdir alone | §Finding 5 (path shape alone is provably insufficient — live counter-examples); §Pattern 5 (layered ownership evidence); §Pitfall 5 (symlink TOCTOU); reusable `discover_repository` / `inspect_removal` |
</phase_requirements>

## Summary

This phase is almost entirely *in-repo mechanism design*, not library selection. There is no
new dependency to add and none is recommended: every capability the phase needs (thread-local
redirects, RAII guards, cargo feature gating, directory enumeration, git porcelain parsing,
removal safety analysis) already exists in the standard library, in Cargo itself, or in
`baude-core`. The research effort therefore went into verifying the **locked decisions compile
and behave as assumed** — and two of them do not.

Two decisions in CONTEXT.md rest on Rust behaviour that a local compile probe refutes. First,
`#[cfg(test)]` is set only when *that crate* is compiled as a test harness; it is **false inside
`baude-core` when `baude`'s or `bauded`'s test binary is built**. A `#[cfg(test)]`-only escape
guard in `baude-core` would therefore be absent from exactly the two test binaries that leaked
1431 directories. Second, the stated rationale for the RAII guard — "arming leaks across tests
within a binary" — is true of the process-wide `AtomicBool`, but **not** of the thread-locals:
libtest spawns a fresh thread per test even under `--test-threads=1`, so a thread-local override
never reaches the next test. The RAII guard is still the right shape, for different reasons
(nested scoping, and eliminating the arming flag entirely); the planner should not encode the
refuted rationale into a verification step that will pass vacuously.

The third correction is to TISO-04's ownership rule. The live dataset contains direct
counter-examples to "strict path-shape match proves ownership": `worktrees/claude/repository-2/primary-2`
holds a real 39-entry checkout for a repository the developer actively uses, and
`worktrees/claude/repository-1` and `repository-5` were created *after* the v2.1.4 containment
fix landed, by production runs, not tests. Path shape is a necessary filter, never a sufficient
proof. Ownership must be layered from independent evidence, and the safest signal available —
cross-referencing `SavedCheckout.observed_path` in the persisted workspace state — is the one
CONTEXT.md explicitly dismissed.

**Primary recommendation:** Gate all test-support code with `cfg(any(test, feature = "test-support"))`
on `baude-core`, enable that feature from `baude`/`bauded` `[dev-dependencies]` (resolver v2 keeps
it out of release builds), replace the arming `AtomicBool` with a *containment predicate* that
accepts either a thread-local override or a path inside `BAUDE_TEST_FIXTURE_ROOT`, and build
TISO-04's ownership proof from layered evidence with `scan` defaulting to a report that authorizes nothing.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Config/state path resolution + redirect | `baude-core` (`persist`) | — | `persist::config_base()` is already the single real resolver; `bauded/src/push.rs:27` is a duplicate to delete |
| `~/.claude` path resolution + redirect | `baude-core` (`meta`) | — | `meta::claude_config_dir()` (`meta.rs:24`) is the only resolver; its two callers are internal to `ClaudeMeta` |
| Managed worktree root resolution | `baude-core` (`git`) | — | `worktrees_base()` (`git.rs:1750`) already owns this and already carries the v2.1.4 guard |
| Workspace identity resolution | `baude-core` (`workspace`) | — | `ACTIVE: OnceLock` (`workspace.rs:199`) is the single cache; `active()` has ~30 readers across all three crates |
| Escape guard enforcement | `baude-core` (each resolver) | — | The guard must sit where the real path is produced, not where it is consumed; consumers cannot know they escaped |
| Test-support feature declaration | `baude-core` `[features]` | `baude`/`bauded` `[dev-dependencies]` | Cargo's only mechanism for "on during `cargo test`, off during `cargo build --release`" |
| Leak enumeration + classification | `baude-core` (new module) | — | Needs private `worktrees_base()`, `discover_repository`, and state loading — all core-internal |
| Leak CLI surface (`scan`/`prune`) | `baude` binary (`main.rs`) | — | `main.rs:225-256` already dispatches subcommands before TUI init; matches `statusline`/`hook`/`permission-mcp` |
| Removal safety analysis | `baude-core` (`git::inspect_removal`) | — | `inspect_removal` (`git.rs:2499`) + `remove_verified_worktree` (`git.rs:2660`) already encode fail-closed semantics |

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| Rust std `thread_local!` + `Cell`/`RefCell` | rustc 1.98.1 | Per-fixture redirect storage | Already the established in-repo pattern (`git.rs:1727`, `hook.rs:91`), chosen deliberately over env mutation in PR #82 |
| Rust std `Drop` | rustc 1.98.1 | RAII reset of redirects | Zero-dependency scope guard; `GitFixture` (`git.rs:2904`) already uses this shape for fixture cleanup |
| Cargo features + resolver v2 | cargo 1.98.1 | Cross-crate test-support gating | The only supported way to enable code in dependency crates during `cargo test` but not `cargo build --release` |
| Rust std `std::fs::read_dir` | rustc 1.98.1 | Leak enumeration | 1433-entry, 3-level tree; no walker crate earns its place |
| `serde_json` | 1 (workspace dep) | `scan --json` output if needed | Already a workspace dependency in all three crates |

**No new external dependency is recommended for this phase.** [VERIFIED: local cargo probe, see §Finding 1]

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `baude_core::git::discover_repository` | in-repo (`git.rs:294`) | Resolve a candidate's owning repository and live worktree inventory | Only for candidates that still contain a `.git` file/dir; 0 of 1433 do |
| `baude_core::git::inspect_removal` | in-repo (`git.rs:2499`) | Fail-closed removal authorization | Required on the `--prune` path for any candidate git still registers |
| `baude_core::persist::load_for_workspace_strict_at` | in-repo (`persist.rs:338`) | Read persisted workspace state to cross-reference `observed_path` | The ownership-negative evidence source (see §Pattern 5) |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Cargo `test-support` feature | `#[cfg(test)]` only | **Does not work across crates** — refuted by probe (§Finding 1). Not a tradeoff, a defect |
| Cargo `test-support` feature | `#[cfg(debug_assertions)]` | Arms the guard in every dev build of the shipped binary, so running `cargo run` locally would panic on the real config dir |
| Cargo `test-support` feature | `ctor`/`inventory` crate to run arming code at binary start | New dependency, link-time-init fragility, and does not solve *which* path is legitimate — only *when* arming happens |
| Thread-local override for workspace | Pass `&Workspace` through every call site | ~30 call sites across 3 crates; explicitly rejected in CONTEXT.md and would risk the path composition at `git.rs:1772`/`git.rs:1807` |
| `clap` for the `scan`/`prune` CLI | hand-rolled `args.get(N)` match | The repo has **no** arg-parsing crate; `main.rs:220-272` dispatches by hand. Adding `clap` for one subcommand is a large new dependency tree for a dev-facing tool |
| `walkdir` for enumeration | `std::fs::read_dir` | The tree is exactly 3 levels deep and the shape is the ownership filter; a generic recursive walker would *weaken* the shape check |

**Installation:**
```bash
# None. Zero new external packages.
# The only manifest change is a feature declaration + dev-dependency wiring:
#   baude-core/Cargo.toml:  [features] test-support = []
#   baude/Cargo.toml:       [dev-dependencies] baude-core = { workspace = true, features = ["test-support"] }
#   bauded/Cargo.toml:      [dev-dependencies] baude-core = { workspace = true, features = ["test-support"] }
```

**Version verification:** Toolchain confirmed in this environment:
```
rustc 1.98.1 (48a229cea 2026-09-01)
cargo 1.98.1 (797e8a9bc 2026-08-05)
```
[VERIFIED: `rustc --version` / `cargo --version`, run 2026-09-13]

There is no `rust-toolchain.toml` and no `rust-version` key in any manifest [VERIFIED: `ls rust-toolchain*` returned no matches; `grep rust-version */Cargo.toml Cargo.toml` returned nothing]. `persist.rs:588` uses `std::fs::TryLockError`, so the effective MSRV is already ≥ the release that stabilized `File::try_lock` [ASSUMED — the exact stabilizing release was not verified this session].

## Package Legitimacy Audit

**Not applicable — this phase installs zero external packages.**

The only manifest changes recommended are a `[features]` entry on `baude-core` (an in-repo path
dependency already declared at `Cargo.toml` workspace level as
`baude-core = { path = "baude-core", version = "=2.1.5" }`) and `[dev-dependencies]` wiring of that
same path dependency in `baude` and `bauded`. No registry package is added, so there is no
slopsquatting surface and no `checkpoint:human-verify` install gate is required.

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

## Findings That Correct the Locked Decisions

These are the highest-value outputs of this research. Each was produced by running code in this
environment, not by recalling documentation.

### Finding 1 — `#[cfg(test)]` does not reach dependency crates (refutes an Escape Guard decision)

CONTEXT.md locks: *"The guard is armed unconditionally under `#[cfg(test)]` … Every test binary is
armed from the first instruction"* and *"`#[cfg(test)]`-only compilation. Zero production cost."*

The Rust Reference defines the `test` cfg as: *"Enabled when compiling the test harness. Done with
`rustc` by using the `--test` flag."* [CITED: https://doc.rust-lang.org/reference/conditional-compilation.html]
That is per-crate. When `cargo test -p baude` builds `baude`'s test harness, `baude-core` is compiled
as an ordinary library — without `--test`.

Falsification probe run in this environment (two-crate workspace, `resolver = "2"`, core exposing
`pub fn cfg_test_is_on() -> bool { cfg!(test) }`):

```
$ cargo test -p probeapp -- --nocapture
TEST: cfg_test_in_dep=false feature_in_dep=true

$ cargo run -p probeapp
bin: cfg_test_in_dep=false feature_in_dep=false

$ cargo run -p probeapp --release
bin: cfg_test_in_dep=false feature_in_dep=false
```

`cfg_test_in_dep=false` inside the downstream test run is the refutation. A `#[cfg(test)]`-only guard
in `baude-core` would be **absent from `baude`'s and `bauded`'s test binaries** — which are exactly
the binaries whose fixtures produced the 1431 test-shaped leaked directories (§Finding 5). Success
Criterion 3 would be satisfied only for `baude-core`'s own tests.

The same probe shows the working mechanism: a cargo feature enabled via `[dev-dependencies]` is on
during `cargo test` (`feature_in_dep=true`) and off in both debug and release plain builds. The
second probe confirms `cfg(any(test, feature = "test-support"))` also covers `baude-core`'s **own**
test binary, where nothing enables the feature:

```
$ cargo test -p probecore -- --nocapture
CORE-OWN-TEST combined=true
```

[VERIFIED: local cargo probe, rustc/cargo 1.98.1, 2026-09-13 — output pasted above]

Cargo's documented rule confirms the build-profile behaviour: *"Features enabled on dev-dependencies
will not be unified when those same dependencies are used as a normal dependency, unless those
dev-dependencies are currently being built"*, and *"`cargo test` or `cargo build --all-targets` will
unify these features"* [CITED: https://doc.rust-lang.org/cargo/reference/resolver.html]. The workspace
already sets `resolver = "2"` at `Cargo.toml:2`, and edition 2021 defaults to it anyway.

**Build-command audit** — which invocations turn the feature on:

| Invocation | Where | Feature on? |
|------------|-------|-------------|
| `cargo test -- --test-threads=1` | `.github/workflows/ci.yml:24` | Yes (intended) |
| `cargo clippy --all-targets -- -D warnings` | `.github/workflows/ci.yml:18` | Yes — `--all-targets` unifies. Gated code must be clippy-clean |
| `cargo build --workspace --release --locked --target …` | `ci.yml:92`, `release.yml:102` | **No** — zero production cost preserved |
| `RUN cargo build --release -p bauded -p baude` | `Dockerfile:11` | **No** |

[VERIFIED: `grep -n "cargo build\|cargo test\|--all-targets" Dockerfile .github/workflows/release.yml .github/workflows/ci.yml`]

### Finding 2 — libtest gives each test its own thread, even serially (refutes the RAII rationale)

CONTEXT.md locks the RAII guard with the rationale: *"The two existing redirects are bare
`thread_local!` + setter with no reset (`git.rs:1727`, `hook.rs:91`), **which is why arming leaks
across tests within a binary**."*

Probe — three tests, the first sets a thread-local, the other two read it:

```
$ cargo test -- --test-threads=1 --nocapture
test tests::a_sets ... A tid=ThreadId(2) get=Some("from-a")
test tests::b_observes ... B tid=ThreadId(3) get=None
test tests::c_observes ... C tid=ThreadId(4) get=None

$ cargo test -- --nocapture
A tid=ThreadId(2) get=Some("from-a")
B tid=ThreadId(3) get=None
C tid=ThreadId(4) get=None
```

Distinct `ThreadId`s and `get=None` under `--test-threads=1` — the value did **not** leak. libtest
allocates a fresh thread per test regardless of the thread count [VERIFIED: local cargo probe,
rustc 1.98.1, output pasted above]. This matters because CI runs precisely `cargo test --
--test-threads=1` (`ci.yml:24`).

What *does* leak is `REQUIRE_WORKTREES_OVERRIDE` (`git.rs:1739`), a process-wide
`static AtomicBool` [VERIFIED: baude-core/src/git.rs:1735-1739, read this session:
`static REQUIRE_WORKTREES_OVERRIDE: AtomicBool = AtomicBool::new(false);`]. Once the first fixture
calls `set_worktrees_base_for_test` it stays armed for the process, and until then every test runs
unguarded. That — not the thread-local — is the Success Criterion 3 defect.

**Planner impact:** build the RAII guard (it is still correct — it enables nested/scoped overrides
and lets the arming flag be deleted), but do **not** write a verification step of the form "prove
the override does not leak into the next test." That assertion passes today, before any change, and
would certify nothing. Verify instead that *an unguarded test binary panics*, which is the real gap.

### Finding 3 — `active() -> &'static Workspace` requires `Box::leak` for a thread-local override

`workspace.rs:221` is `pub fn active() -> &'static Workspace`, and CONTEXT.md locks *"`active()`
keeps its current signature."* A `thread_local!` cannot hand out `&'static` references to its own
contents. The resolution is to leak the per-fixture `Workspace` and store the resulting `&'static`
reference (which is `Copy`, so a `Cell` suffices).

Probe — full shape, compiled and passing:

```rust
thread_local! { static OVERRIDE: Cell<Option<&'static Workspace>> = const { Cell::new(None) }; }

pub fn active() -> &'static Workspace {                 // signature UNCHANGED
    if let Some(ws) = OVERRIDE.with(|c| c.get()) { return ws; }
    initialize("from-real-config")
}

pub fn override_for_test(name: &str) -> WorkspaceGuard {
    let leaked: &'static Workspace = Box::leak(Box::new(Workspace { /* … */ }));
    let prev = OVERRIDE.with(|c| c.replace(Some(leaked)));
    WorkspaceGuard { prev }
}
impl Drop for WorkspaceGuard {
    fn drop(&mut self) { OVERRIDE.with(|c| c.set(self.prev)); }
}
```

```
PROBE OK: &'static preserved, guard resets, nesting works
test result: ok. 1 passed; 0 failed
```

[VERIFIED: local cargo probe, rustc 1.98.1 — output pasted above]

`Workspace` is already `'static`-compatible: its fields are `String`, `&'static dyn Backend`,
`Option<String>`, `Option<u16>` [VERIFIED: baude-core/src/workspace.rs:46-56, read this session —
`pub struct Workspace { pub name: String, pub backend: &'static dyn Backend, pub daemon_url: Option<String>, pub daemon_port: Option<u16> }`].
The leak is bounded by fixture count and is confined to `cfg(any(test, feature = "test-support"))`
code, so it never ships.

### Finding 4 — no test currently writes a VAPID key; the redirect needs a new test to prove it

CONTEXT.md's Specific Ideas state: *"`~/.config/baude/daemon-vapid.json` — real VAPID keypair
written by tests."*

`Vapid::load_or_generate` is called from exactly one place, and `PushState::load` from exactly one
place:

```
$ grep -rn "load_or_generate" --include='*.rs' .
bauded/src/push.rs:51:    pub fn load_or_generate() -> Result<Vapid> {
bauded/src/push.rs:197:            vapid: Vapid::load_or_generate()?,

$ grep -rn "PushState::load" --include='*.rs' .
bauded/src/main.rs:182:    let push_state: push::SharedPush = Arc::new(Mutex::new(push::PushState::load(true)?));
```

`bauded/src/main.rs:182` is production. No `#[test]` or `#[tokio::test]` constructs a `PushState`,
and `bauded/src/api.rs` only references `push_router` at its definition (`api.rs:51`) — the
`/push/*` routes have no test coverage at all [VERIFIED: `grep -n "push" bauded/src/api.rs` returned
only lines 51, 52, 71, 72, 77, 79, 92 — all in the router definition — plus an unrelated
`push_str` at 1607].

The on-disk file supports this: `daemon-vapid.json` is dated `Jun 15 13:06`, long before the current
test suite [VERIFIED: `ls -la ~/.config/baude/`, run 2026-09-13].

**Planner impact:** the `push.rs` refactor to `persist::config_dir()` is still correct and still
required by Success Criterion 1 — but there is **no existing failing test it will fix**. A plan that
treats "existing tests stop writing VAPID keys" as the acceptance signal will verify nothing. The
plan must add a test that constructs `PushState` under a redirect and asserts both `daemon-vapid.json`
and `daemon-push.json` land inside the fixture root. That test is new coverage for a currently
untested subsystem, which is a benefit, not a cost.

Note also `PushState::load(false)` still calls `Vapid::load_or_generate()` unconditionally
(`push.rs:197`) — the `persist` flag suppresses subscription reads/writes but **not** key generation
[VERIFIED: bauded/src/push.rs:187-201, read this session]. A "just pass `persist: false` in tests"
shortcut would not contain the key write.

### Finding 5 — path shape alone provably cannot prove ownership (corrects the TISO-04 rule)

CONTEXT.md locks: *"Ownership is proven by strict path-shape match under the real worktrees base…
Workspace state has zero record of the leaked directories, so ownership cannot be sourced from
state."*

The live dataset was re-measured this session and confirms the scale, but also produces direct
counter-examples to shape-as-proof.

Scale, confirmed [VERIFIED: `find ~/.local/share/baude/worktrees` etc., run 2026-09-13]:

| Measure | Value |
|---------|-------|
| `repository-*` directories at depth 2 | 1433 |
| Completely empty | 1286 |
| Workspaces present | `claude` (4), `opencode` (1263), `prerelease` (166) |
| Oldest / newest depth-2 entry | 2026-08-30 13:25 / 2026-09-13 18:30 |

The leaked branch labels map directly to named tests, which is a far stronger ownership signal than
shape:

```
$ grep -rn "feature/manager-spawn\|feature/reload-resolved\|feature/local-contract\|…" --include='*.rs' .
baude/src/app.rs:6962   .activate_branch_worktree(&repo, "feature/local-contract")
baude/src/app.rs:7310   .activate_branch_worktree(&repo, "feature/reload-resolved")
bauded/src/manager.rs:3271 .activate_branch_worktree(&repo, "feature/manager-spawn", None)
```

and the leaked `repository-<key>` keys are pid-derived (`98206`, `177247`, `218432`), matching the
fixture convention `next_repository_key = u64::from(std::process::id()) + key_offset`
[VERIFIED: baude/src/app.rs:5700, read this session: `app.repository_state.next_repository_key = u64::from(std::process::id()) + key_offset;`].

**The counter-examples.** Four entries under `worktrees/claude/` match the identical path shape and
are *not* test leaks:

| Path | mtime | Content | What it is |
|------|-------|---------|-----------|
| `claude/repository-2/primary-2` | 2026-09-08 21:49 | 39 entries | A real managed default worktree with a real checkout |
| `claude/repository-14/primary-14` | 2026-09-01 07:24 | 19 entries | Same |
| `claude/repository-1` | 2026-09-13 16:36 | empty | Created **after** the containment fix, by production |
| `claude/repository-5` | 2026-09-13 18:30 | empty | Same |

[VERIFIED: `stat -f "%Sm %N"` over the tree and `ls -la` per directory, run 2026-09-13]

The timing is decisive. PR #82 (`725c558 fix(test): contain managed worktrees in fixture roots (#82)`)
is dated **2026-09-13 15:10:03 -0700** and `v2.1.4` is tagged 2026-09-13 22:13:55 UTC
[VERIFIED: `git log -1 --format=%ai 725c558` and `git log --tags --simplify-by-decoration`].
`repository-1` (16:36 PDT) and `repository-5` (18:30 PDT) postdate the fix, carry small sequential
keys — not pid-derived — and sit in the `claude` workspace. Keys 1 and 5 are live repositories in
the developer's real state:

```
repositories key=1  observed_main_worktree=/Users/joese/Code/github.com/poindexter12/jacaranda-network
repositories key=2  observed_main_worktree=/Users/joese/Code/github.com/poindexter12/jacaranda-home-automation
repositories key=5  observed_main_worktree=/Users/joese/Code/github.com/joese-iarx/inbox-zero-local
```

[VERIFIED: `~/.config/baude/state-claude.json`, decoded `state.repositories[]`, read this session —
9 repositories, keys 1..9, all checkouts `managed_by_baude: false`]

So the strict shape `<base>/<workspace>/repository-<u64>/…` matches, simultaneously, (a) 1431
test leaks, (b) two real managed worktrees holding real content, and (c) two directories the
developer's own baude created hours ago. **Shape is a necessary filter and a sufficient condition
for nothing.**

CONTEXT.md is right that state has no record of the *leaked* directories — but that is precisely
what makes state useful: it is the source of **ownership-negative** evidence. `SavedCheckout` records
`observed_path` [VERIFIED: baude-core/src/repository.rs:318-335, read this session:
`pub struct SavedCheckout { pub key: CheckoutKey, pub repository_key: RepositoryKey, pub role: CheckoutRole, pub managed_by_baude: bool, pub observed_path: PersistedPath, … }`], so a
directory referenced by any workspace state is *definitively live* and must never be a candidate.

### Finding 6 — the empty-parent leak is also a production defect, still active

Both `ensure_default_worktree` and `activate_branch_with_post_add_hook` create the
`repository-<key>` parent before invoking git, and nothing removes it if the subsequent step fails:

```rust
// baude-core/src/git.rs:1069
if let Some(parent) = managed_path.parent() {
    std::fs::create_dir_all(parent).map_err(|source| {
        EnsureDefaultWorktreeError::CreateParent { path: parent.to_path_buf(), source }
    })?;
}
// baude-core/src/git.rs:1568
if let Some(parent) = managed_path.parent() {
    std::fs::create_dir_all(parent).map_err(|source| BranchActivationError::CreateParent {
        path: parent.to_path_buf(), source })?;
}
```

[VERIFIED: baude-core/src/git.rs:1069-1074 and 1568-1573, read this session]

The only `remove_dir`/`remove_dir_all` calls in `git.rs` are at `:2323` (a worktree's own `.claude`
dir), `:2906` (the `GitFixture` `Drop`), and `:4402` (inside a test) [VERIFIED:
`grep -n "remove_dir" baude-core/src/git.rs`]. No code removes an empty `repository-<key>` parent.

This explains the 1286 empty directories *and* the two post-fix production ones. It is not listed in
TISO-01..04 and is arguably out of phase scope, but the planner must know it, because it means the
scan tool will keep finding new candidates after the phase ships. See §Open Questions.

### Finding 7 — `release_state_lock_for_test` is not reusable from `baude`/`bauded`

CONTEXT.md lists it as a Reusable Asset. It is declared:

```rust
// baude-core/src/persist.rs:552-553
#[cfg(test)]
fn release_state_lock_for_test(destination: &std::path::Path) {
```

[VERIFIED: baude-core/src/persist.rs:551-560, read this session]

No `pub`, and `#[cfg(test)]` — so it is invisible to `baude` and `bauded` on both counts. If the
unified guard needs to release a state lock when a fixture root is reused, this function must be
made `pub` and moved under the same `cfg(any(test, feature = "test-support"))` gate as everything
else. Two other listed assets are likewise private: `parse_worktree_porcelain` (`git.rs:211`),
`load_named_at` (`persist.rs:1007`), and `worktrees_base` (`git.rs:1750`) [VERIFIED: `grep -n` over
both files, this session].

### Finding 8 — the dogfood child *does* compile the guard, but relies on env, not thread-locals

CONTEXT.md's discretion item assumes *"the re-exec'd real-git dogfood child runs as a spawned
process, so `#[cfg(test)]`-only arming does not reach it."*

The child is `std::env::current_exe()` re-invoked with `--exact <test name>`
[VERIFIED: baude/src/app.rs:7648-7663, read this session: `Command::new(std::env::current_exe().unwrap()).args(["--exact", "app::tests::local_tui_dogfood_real_git_flow_survives_restart_without_duplicates", "--nocapture"])`].
That *is* the test binary — every gate that holds in the parent holds in the child. The arming
reaches it fine.

The real hazard is the opposite one: the child isolates by **environment**, not by thread-local.

```rust
.env("BAUDE_TEST_FIXTURE_ROOT", &root)
.env("XDG_DATA_HOME", root.join("data"))
.env("HOME", root.join("home"))
```

[VERIFIED: baude/src/app.rs:7656-7659, read this session]

`XDG_CONFIG_HOME` is **not** set. `persist::config_base()` falls back to `dirs::home_dir()`, and
`dirs-sys` 0.4.1 resolves `home_dir()` on unix as `env::var_os("HOME")` first [VERIFIED:
`~/.cargo/registry/src/…/dirs-sys-0.4.1/src/lib.rs:33-37`: `return env::var_os("HOME").and_then(|h| if h.is_empty() { None } else { Some(h) }).or_else(|| unsafe { fallback() }).map(PathBuf::from);`].
So in the child, config resolves to `<root>/home/.config/baude` and `~/.claude` to
`<root>/home/.claude` — both already contained, but via `HOME`, with **no thread-local override set**.

A guard that demands a thread-local override would therefore panic the dogfood test on
`App::new`'s `persist::load_config()` (`app.rs:719`). §Pattern 4 resolves this: make the guard a
*containment* predicate rather than an *override-presence* predicate. That answers the discretion
item — `BAUDE_TEST_FIXTURE_ROOT` is sufficient, provided the guard checks containment against it
rather than merely noting it exists.

## Architecture Patterns

### System Architecture Diagram

```
                        ┌──────────────────── cfg(any(test, feature="test-support")) ───────────────────┐
                        │                                                                              │
  fixture body          │   TestRedirect::new(root)  ──arms──►  thread_local REDIRECTS { … }           │
  (one per #[test],     │        │                                    │                                │
   own libtest thread)  │        └──on drop──► restore previous ◄─────┘                                │
                        └──────────────────────────────────────────────────────────────────────────────┘
                                                     │ read first
                                                     ▼
   caller ──► persist::config_base() ──────► redirect? ──yes──► <fixture>/config
   caller ──► meta::claude_config_dir() ───► redirect? ──yes──► <fixture>/.claude
   caller ──► git::worktrees_base() ───────► redirect? ──yes──► <fixture>/data/baude/worktrees
   caller ──► workspace::active() ─────────► redirect? ──yes──► &'static leaked Workspace
                                                     │
                                                    no
                                                     ▼
                                         resolve real path (XDG_* → HOME → fallback)
                                                     │
                        ┌────────────── escape guard (same cfg gate) ───────────────┐
                        │  contained in BAUDE_TEST_FIXTURE_ROOT ?                   │
                        │        yes ──► return real path (dogfood child)           │
                        │        no  ──► panic!("escaped fixture root: …")          │
                        └───────────────────────────────────────────────────────────┘
                                                     │ (production: gate absent)
                                                     ▼
                                            real ~/.config, ~/.claude,
                                            ~/.local/share/baude/worktrees


   ── TISO-04, separate path, production code ──

   baude worktrees scan ──► enumerate <real base>/<ws>/repository-<u64>/[child]
                                │
                                ├─ shape filter        (necessary, not sufficient)
                                ├─ state cross-ref  ──► referenced by any SavedCheckout.observed_path?  ──► LIVE, excluded
                                ├─ git inventory    ──► discover_repository + worktree list, if a gitdir exists
                                ├─ content class    ──► empty / non-empty / symlink / foreign
                                └─► Candidate { path, classification, evidence[], removable: bool }
                                        │
                        default ────────┴──────── --prune --yes
                           │                          │
                        report only               re-verify each candidate from scratch,
                        (authorizes                refuse on any missing/ambiguous evidence,
                         nothing)                  refuse on symlink, then remove
```

### Recommended Project Structure

```
baude-core/src/
├── testing.rs          # NEW: unified TestRedirect guard + containment predicate.
│                       #      cfg(any(test, feature = "test-support")) at module level.
├── worktree_scan.rs    # NEW: TISO-04 enumeration + classification. PRODUCTION code, no cfg gate.
├── persist.rs          # config_base() consults redirect; release_state_lock_for_test → pub + gated
├── meta.rs             # claude_config_dir() consults redirect
├── git.rs              # worktrees_base() consults redirect; delete REQUIRE_WORKTREES_OVERRIDE;
│                       #   add pub real_worktrees_base() for the scanner
├── hook.rs             # HOOK_COMMAND_OVERRIDE folded into testing.rs
└── workspace.rs        # thread-local &'static override; initialize(config, hint)

baude/src/
└── main.rs             # NEW subcommand arm alongside statusline/hook/permission-mcp (main.rs:225-256)

bauded/src/
└── push.rs             # DELETE local config_base() (:27-33); call persist::config_dir()
```

Placing the guard in one new `testing.rs` module is what makes "one place to arm, one place to
reset" real. Each resolver keeps its own `if redirect { … }` line, but the storage, the guard type,
and the containment predicate live together.

### Pattern 1: Unified RAII redirect guard

**What:** One struct holding every redirect a fixture needs, restoring the previous values on drop.
**When to use:** Every fixture in all three crates; replaces the four separate setters.

```rust
// baude-core/src/testing.rs
#![cfg(any(test, feature = "test-support"))]

use std::cell::RefCell;
use std::path::{Path, PathBuf};

#[derive(Default, Clone)]
pub(crate) struct Redirects {
    pub worktrees_base: Option<PathBuf>,
    pub config_dir: Option<PathBuf>,
    pub claude_config_dir: Option<PathBuf>,
    pub hook_command: Option<String>,
    pub workspace: Option<&'static crate::workspace::Workspace>,
}

thread_local! {
    pub(crate) static REDIRECTS: RefCell<Redirects> = RefCell::new(Redirects::default());
}

/// Point every isolated resolver at `root` for the CURRENT THREAD. Restores the
/// previous values on drop, so nesting composes and a fixture cannot outlive its root.
#[must_use = "the redirect is reverted when this guard is dropped"]
pub struct TestRedirect {
    previous: Redirects,
}

impl TestRedirect {
    pub fn new(root: impl AsRef<Path>) -> TestRedirect {
        let root = root.as_ref();
        let next = Redirects {
            worktrees_base: Some(root.join("data")),
            config_dir: Some(root.join("config")),
            claude_config_dir: Some(root.join("claude-home")),
            hook_command: Some(format!("{} hook", root.join("bin").join("baude").display())),
            workspace: None, // set via .workspace(name) — see Pattern 3
        };
        let previous = REDIRECTS.with(|cell| cell.replace(next));
        TestRedirect { previous }
    }
}

impl Drop for TestRedirect {
    fn drop(&mut self) {
        REDIRECTS.with(|cell| *cell.borrow_mut() = std::mem::take(&mut self.previous));
    }
}
```

Note `#[must_use]`: without it, `TestRedirect::new(root);` (no binding) drops immediately and the
fixture silently runs unredirected. That is the single most likely way to reintroduce the leak, and
the attribute turns it into a compiler warning — which CI escalates to an error via
`cargo clippy --all-targets -- -D warnings` (`ci.yml:18`).

### Pattern 2: Cross-crate test-support gating

**What:** A no-default cargo feature on `baude-core`, enabled from the two downstream crates'
dev-dependencies.
**When to use:** Every piece of test-support code in `baude-core` that `baude` or `bauded` tests
must reach — the guard, the redirect setters, `release_state_lock_for_test`, and any new `pub`
accessor added for fixtures.

```toml
# baude-core/Cargo.toml
[features]
test-support = []

# baude/Cargo.toml  and  bauded/Cargo.toml
[dev-dependencies]
baude-core = { workspace = true, features = ["test-support"] }
```

```rust
// Use this cfg, NOT bare #[cfg(test)] — see Finding 1.
#[cfg(any(test, feature = "test-support"))]
pub fn set_worktrees_base_for_test(base: impl Into<PathBuf>) -> TestRedirect { /* … */ }
```

`test` covers `baude-core`'s own harness (where the feature is not enabled); the feature covers the
downstream harnesses (where `test` is false). Together they cover all three test binaries and
nothing else. [VERIFIED: local cargo probe, §Finding 1]

Because the workspace manifest declares `baude-core = { path = "baude-core", version = "=2.1.5" }`
[VERIFIED: Cargo.toml:15, read this session], the dev-dependency must re-declare the features while
inheriting the rest: `baude-core = { workspace = true, features = ["test-support"] }`.

### Pattern 3: Thread-local `&'static` workspace override

**What:** Check a thread-local first, fall back to the existing `OnceLock`; preserve `active()`'s
signature by leaking the fixture workspace.
**When to use:** `workspace::active()` only.

```rust
pub fn initialize(config: &Config, hint: Option<&str>) -> &'static Workspace {
    ACTIVE.get_or_init(|| resolve_with_hint(
        std::env::var("BAUDE_WORKSPACE").ok().as_deref(),
        std::env::var("BAUDE_BACKEND").ok().as_deref(),
        hint, config, |msg| eprintln!("baude: {msg}"),
    ))
}

pub fn active() -> &'static Workspace {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(ws) = crate::testing::workspace_override() { return ws; }

    #[cfg(any(test, feature = "test-support"))]
    crate::testing::assert_no_escape_workspace();   // panic: fixture forgot to set identity

    initialize(&crate::persist::load_config(), None)
}
```

Config injection into `initialize()` is the locked decision and is what removes the real-config read
from the identity path. `baude/src/main.rs:296` already computes `let config = persist::load_config();`
at `main.rs:281` before calling `initialize` [VERIFIED: baude/src/main.rs:281 and 299, read this
session — `let config = baude_core::persist::load_config();` … `let workspace = baude_core::workspace::initialize(plan.hint.as_deref());`], so the production call site passes it with no extra read.

Full verified skeleton for the override storage and guard is in §Finding 3.

### Pattern 4: Containment predicate instead of an arming flag

**What:** Replace `REQUIRE_WORKTREES_OVERRIDE: AtomicBool` with a predicate evaluated at every real
resolution, under the test-support cfg. Nothing to arm, so nothing to arm *late*.
**When to use:** All five guarded resolvers.

```rust
// baude-core/src/testing.rs
/// A test binary may resolve a real path only when that path is demonstrably inside
/// the declared fixture root. Presence of the variable is not enough — containment is.
pub(crate) fn assert_contained(resolved: &Path, what: &str) {
    let Some(root) = std::env::var_os("BAUDE_TEST_FIXTURE_ROOT").map(PathBuf::from) else {
        panic!("{what} resolved to the real user directory {} during a test; \
                hold a baude_core::testing::TestRedirect on this thread first",
               resolved.display());
    };
    assert!(
        resolved.starts_with(&root),
        "{what} resolved to {} which escapes the declared fixture root {}",
        resolved.display(), root.display(),
    );
}
```

```rust
// baude-core/src/git.rs
fn worktrees_base() -> PathBuf {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(base) = crate::testing::worktrees_base_override() {
        return base.join("baude").join("worktrees");
    }
    let real = real_worktrees_base();
    #[cfg(any(test, feature = "test-support"))]
    crate::testing::assert_contained(&real, "managed worktree root");
    real
}

/// The production root, unguarded. TISO-04's scanner needs this by definition.
pub fn real_worktrees_base() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".local").join("share")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("baude")
        .join("worktrees")
}
```

Two properties this buys:

1. **Armed from the first instruction of every test binary** — Success Criterion 3 — because there
   is no arming state. The guard is a property of the compiled binary, not of execution history.
2. **The dogfood child passes**, because its `HOME`/`XDG_DATA_HOME` redirection puts every real
   resolution inside `BAUDE_TEST_FIXTURE_ROOT` (§Finding 8). Containment is checked, not trusted.

`real_worktrees_base()` must be `pub` and **ungated** — the scanner is production code that
deliberately targets the real root.

### Pattern 5: Layered ownership evidence for TISO-04

**What:** A candidate accumulates independent evidence items; `--prune` requires positive
non-ownership, not absence of contradiction.
**When to use:** The scan/prune path, per candidate.

```rust
pub enum Evidence {
    ShapeMatch,                              // <base>/<ws>/repository-<u64>[/<child>-<u64>]
    NotReferencedByState { workspaces: Vec<String> },  // checked every state file, found nothing
    NoGitdir,                                // NEVER sufficient alone (TISO-04 wording)
    Empty,                                   // no entries at any level
    GitDisownsIt { repository: PathBuf },    // discover_repository ran, path absent from inventory
    ContainsCheckout { entries: usize },     // real content — a removal blocker
    ReferencedByState { workspace: String }, // a removal blocker
    IsSymlink,                               // a removal blocker (see Pitfall 5)
    StateUnreadable { workspace: String },   // INDETERMINATE — blocks, never authorizes
}

pub enum Verdict {
    Live,                                    // referenced by state, or git registers it
    Indeterminate { why: Vec<Evidence> },    // default for anything not positively cleared
    Removable { proof: Vec<Evidence> },      // requires ShapeMatch + NotReferencedByState + (Empty | GitDisownsIt)
}
```

The rules that fall out of §Finding 5:

- `ShapeMatch` alone → `Indeterminate`. It matches real production worktrees.
- `NoGitdir` alone → `Indeterminate`. All 1433 candidates have no gitdir; treating it as proof
  would authorize deleting every one, which is exactly what TISO-04 forbids.
- Any state file that fails to parse → `Indeterminate` for **every** candidate in that workspace.
  The alternative is deriving "not referenced" from a file you could not read — an absence that
  proves nothing. This matches the fail-closed precedent set for removal in
  `inspect_removal`/`RemovalSafety`.
- `ReferencedByState`, `ContainsCheckout`, `IsSymlink` → hard blockers, independent of everything else.
- `scan` prints all three verdicts. `--prune --yes` operates only on `Removable`, and re-derives
  every evidence item from scratch at removal time (the two-step approval decision).

State cross-referencing must cover **every** workspace's state files, not just the active one.
Filenames are `state-<ws>.json` and `daemon-state-<ws>.json` plus the legacy un-suffixed
`state.json`/`daemon-state.json` for the `claude` workspace [VERIFIED: baude-core/src/workspace.rs:63-71,
read this session: `pub fn state_file(&self, base: &str) -> String { format!("{base}-{}.json", self.name) }`
and `pub fn legacy_state_file(&self, base: &str) -> Option<String> { (self.name == DEFAULT).then(|| format!("{base}.json")) }`,
with `pub const DEFAULT: &str = "claude";` at workspace.rs:58]. The real config dir holds all of
these today [VERIFIED: `ls -la ~/.config/baude/` — `state-claude.json`, `state-opencode.json`,
`state-prerelease.json`, `daemon-state-claude.json`, `daemon-state-opencode.json`,
`daemon-state-prerelease.json`, `daemon-state.json`, `state.json`].

### Pattern 6: Hand-rolled subcommand dispatch (matching the existing CLI)

**What:** A new `args.get(1)` arm in `baude/src/main.rs`, before the TUI touches the terminal.
**When to use:** The TISO-04 CLI surface.

```rust
// baude/src/main.rs — alongside the statusline (:225), hook (:240), permission-mcp (:249) arms
if args.get(1).map(String::as_str) == Some("worktrees") {
    let prune = args.iter().any(|a| a == "--prune");
    let yes = args.iter().any(|a| a == "--yes");
    std::process::exit(run_worktrees(prune, yes));
}
```

The `--help` text at `main.rs:265` currently reads
`subcommands: statusline, hook, permission-mcp` [VERIFIED: baude/src/main.rs:265, read this session]
and must be extended — a new subcommand that is not discoverable defeats the point of a
preview tool.

### Anti-Patterns to Avoid

- **`#[cfg(test)]` on shared test-support code in `baude-core`.** Silently absent from the
  downstream test binaries (§Finding 1). Use `cfg(any(test, feature = "test-support"))`.
- **Returning `Result` from the escape guard.** Locked as panic, and correctly: `worktrees_base()`
  and `config_base()` return `PathBuf`, so a `Result` would force ~15 call sites to change and every
  one could swallow it.
- **`TestRedirect::new(root);` as a bare statement.** Drops immediately; `#[must_use]` catches it.
- **Deriving "unreferenced by state" from a state file that failed to parse.** An unreadable file is
  silence about every path, not evidence about one.
- **Treating a missing gitdir as ownership.** Explicitly forbidden by TISO-04, and §Finding 5 shows
  why: it is true of all 1433 candidates including the two production ones.
- **Keeping `REQUIRE_WORKTREES_OVERRIDE` alongside the new guard.** Two mechanisms means the weaker
  one decides; delete it in the same change.
- **Redirecting config by mutating `HOME`/`XDG_CONFIG_HOME` in-process.** PR #82's recorded rationale
  rejects env mutation for exactly the parallel-test reason [VERIFIED: baude-core/src/git.rs:1728-1731,
  read this session: *"Thread-local, not an env var: the test binary runs cases in parallel, and a
  process-wide `XDG_DATA_HOME` written by one case would decide where another one's worktrees land."*].

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| "On during `cargo test`, off in release, across crates" | A runtime env-var flag, a `ctor` shim, or `debug_assertions` | Cargo feature + resolver v2 dev-dependency wiring | The documented, compiler-enforced mechanism; verified to behave correctly in this workspace (§Finding 1) |
| Scope-restoring redirects | Manual `set_x(old)` at the end of each test | `Drop` impl | A test that panics mid-body never reaches a manual restore; `Drop` runs during unwind |
| Worktree inventory for a candidate | Parsing `.git` files or walking `worktrees/` in `$GIT_DIR` | `git::discover_repository` (`git.rs:294`) + `git worktree list --porcelain` | Already handles common-dir canonicalization, symlinks, and linked-worktree topology — the v2.0 identity work |
| Removal safety analysis | Fresh `is_dirty`/submodule/lock checks | `git::inspect_removal` (`git.rs:2499`), `RemovalBlocker`, `RemovalSafety`, `remove_verified_worktree` (`git.rs:2660`) | Encodes the fail-closed rules from Phase 6, including "any recursive submodule record blocks non-force removal" |
| CLI argument parsing | Adding `clap` | `args.get(1)` match, as `main.rs:220-272` already does | Three existing subcommands use this; a dependency tree for one dev-facing verb is not justified |
| Atomic state writes for the scanner | Anything | Nothing — the scanner does not write state | Adding a writer to a leak-preview tool creates a new class of the bug it exists to find |
| Recursive directory walking | `walkdir` | `std::fs::read_dir` at fixed depths | The fixed shape *is* the filter; a generic walker would accept paths the shape rejects |

**Key insight:** the only genuinely novel code in this phase is the redirect plumbing and the
ownership *decision* logic. Every filesystem and git primitive it needs already exists in
`baude-core`, written under the Phase 5/6 fail-closed discipline. Rewriting any of them for the
scanner would produce a second, weaker set of safety rules governing a *destructive* operation.

## Runtime State Inventory

This is a refactor phase that changes where files are written, so runtime state matters.

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | `~/.config/baude/` holds 8 state/daemon-state JSON files across 3 workspaces (`claude`, `opencode`, `prerelease`) plus legacy `state.json`/`daemon-state.json`, `config.json`, `breadcrumbs-*.json`, `folder-workspaces.json`, `daemon-vapid.json`, `daemon-push.json` [VERIFIED: `ls -la ~/.config/baude/`]. `~/.local/share/baude/worktrees/` holds 1433 `repository-*` directories, 1.2 GB [VERIFIED: `find`, this session] | **No migration.** This phase must not move, rewrite, or delete any real file. The redirect changes only where *tests* write. TISO-04 tooling reads the worktrees tree and the state files; deletion is explicitly deferred |
| Live service config | None. `bauded` config is the same `~/.config/baude` tree; there is no external service holding a baude identity [VERIFIED: `grep -rn "config_dir()\|config_base()"` found no non-filesystem config source] | None |
| OS-registered state | None found. No launchd plist, systemd unit, or pm2 entry is written by this codebase [VERIFIED: `grep -rn "launchctl\|systemd\|plist\|pm2"` over `*/src` returned nothing relevant] | None |
| Secrets / env vars | `~/.config/baude/daemon-vapid.json` — a real VAPID keypair, dated Jun 15 [VERIFIED: `ls -la`]. `BAUDE_TEST_FIXTURE_ROOT`, `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, `HOME`, `CLAUDE_CONFIG_DIR`, `BAUDE_WORKSPACE`, `BAUDE_BACKEND` are the env names this phase reads | The `push.rs` refactor changes the *resolver*, not the filename (`daemon-vapid.json`, `daemon-push.json` — `push.rs:24-25`), so the production path is byte-identical and the existing key keeps working. Rotation is explicitly deferred |
| Build artifacts | `target/` carries compiled test binaries. Adding a cargo feature changes the feature set, forcing a rebuild of `baude-core` and both dependents | None — cargo handles it. Note CI's `Swatinem/rust-cache@v2` (`ci.yml:15`) will see a cache miss on the first run after the feature lands; expect one slow CI run |

## Common Pitfalls

### Pitfall 1: `#[cfg(test)]` that silently vanishes downstream
**What goes wrong:** The guard compiles, `cargo test -p baude-core` passes, and `baude`/`bauded`
tests keep leaking into the real data dir with no signal.
**Why it happens:** `cfg(test)` is set per-crate by `rustc --test`; a dependency is never built with it.
**How to avoid:** `cfg(any(test, feature = "test-support"))` everywhere, with the dev-dependency wiring.
**Warning signs:** A test that *should* panic passes. Add a deliberate escape test in `baude`'s and
`bauded`'s test modules — not just `baude-core`'s — and assert it panics via `#[should_panic]`.
The cheapest proof the gate actually works is a `#[test] fn gate_is_active() { assert!(cfg!(feature = "test-support")); }`
in each downstream crate.

### Pitfall 2: A redirect guard that is constructed but not bound
**What goes wrong:** `TestRedirect::new(root);` arms and immediately disarms; the fixture writes to
the real directories.
**Why it happens:** Rust drops temporaries at the end of the statement. The existing setters return
`()`, so every one of the ~20 call sites is currently written as a bare statement and will look
correct after a mechanical edit.
**How to avoid:** `#[must_use]` on `TestRedirect`, plus `let _guard = …` (never `let _ = …`, which
also drops immediately).
**Warning signs:** `let _ = TestRedirect::new(…)` anywhere. Worth a grep in the verification step.

### Pitfall 3: Thread-locals do not reach spawned threads
**What goes wrong:** Code resolving a redirected path on a thread the fixture did not create sees
no override, then either panics on the guard or writes to the real directory.
**Why it happens:** `thread_local!` is per-thread by definition; `std::thread::spawn` starts fresh.
**Current exposure — low but real.** Spawned threads in the codebase: `pty.rs:169` (PTY reader,
no path resolution), `app.rs:4496` (`git::clone_repo`, no redirected resolver),
`usage.rs:33`, `remote.rs:87`, `remote.rs:253`, `notify_desktop.rs:142`, `bauded/main.rs:190`
(manager poll loop — **does** reach `ClaudeMeta::poll` → `claude_config_dir()`),
`bauded/api.rs:541`, `bauded/permission_bridge.rs:62` [VERIFIED: `grep -rn "thread::spawn"`].
Every `#[tokio::test]` in `bauded/src/api.rs` uses the default current-thread flavor (16 occurrences,
none with `flavor = "multi_thread"`) [VERIFIED: `grep -rn "tokio::test"` and
`grep -rn "multi_thread"` — the only runtime builder is `new_current_thread()` at manager.rs:4130],
so axum handlers run on the test's own thread and do see the override.
**How to avoid:** Do not introduce a `flavor = "multi_thread"` test, and do not resolve a redirected
path inside `std::thread::spawn` in code under test. If one becomes necessary, capture the resolved
`PathBuf` before spawning and move it in.
**Warning signs:** A new `#[tokio::test(flavor = "multi_thread")]`, or a guard panic whose backtrace
does not start at a `#[test]` frame.

### Pitfall 4: Verifying the refuted property
**What goes wrong:** The plan asserts "an override set by test A is not visible to test B," it
passes, and everyone concludes the RAII guard works — but that assertion passes on today's code too
(§Finding 2), so it certifies nothing.
**Why it happens:** The CONTEXT rationale names the wrong mechanism.
**How to avoid:** Verify the properties that are actually new: (a) a test binary with **no** fixture
at all panics when it resolves a real path; (b) `baude-core`'s own test binary panics too (it never
armed before); (c) a guard dropped mid-test restores the prior value, exercised within one test.
**Warning signs:** A verification step whose assertion you cannot make fail by reverting the change.

### Pitfall 5: Symlink and TOCTOU exposure on the `--prune` path
**What goes wrong:** A candidate is a symlink (or contains one) pointing outside the worktrees base;
`remove_dir_all` follows it and deletes real user data. Or: evidence is gathered, the developer
approves, and the directory changes before removal.
**Why it happens:** `std::fs::metadata` follows symlinks; `remove_dir_all` on a symlink-to-directory
behaves differently across platforms and versions.
**How to avoid:** Use `std::fs::symlink_metadata` for every classification decision and refuse any
candidate whose own entry is a symlink. There is in-repo precedent: `verify_removal_postconditions`
already uses `std::fs::symlink_metadata(&target.path)` [VERIFIED: baude-core/src/git.rs:2594, read
this session]. Re-derive all evidence immediately before removal — the locked two-step approval.
**Warning signs:** `std::fs::metadata` or `Path::exists()` (which also follows symlinks) in
classification code.

### Pitfall 6: The scan finding new candidates after the phase ships
**What goes wrong:** The team treats a clean `scan` as the phase's exit criterion, then the count
climbs again within days.
**Why it happens:** §Finding 6 — `create_dir_all(parent)` at `git.rs:1069` and `git.rs:1568` leaves
an empty `repository-<key>` behind on any downstream failure, in production. Two such directories
appeared on 2026-09-13 *after* v2.1.4 landed.
**How to avoid:** Do not make "zero candidates" an acceptance criterion. The phase's criterion is
that *tests* stop producing them and that existing ones can be previewed.
**Warning signs:** New candidates with small sequential keys in the `claude` workspace.

### Pitfall 7: `persist::config_dir()` is a shared root, not just config
**What goes wrong:** Redirecting `config_base()` moves more than `config.json` — it moves state
files, the state lock (`persist.rs:540`), breadcrumbs (`app.rs:1277`), and folder-workspace memory
(`main.rs:291`). A fixture that redirects config but still passes an explicit
`persistence_root_for_test` now has state in two places.
**Why it happens:** `config_dir()` returns `config_base()` verbatim (`persist.rs:850-852`) and is
the parent of all sibling stores.
**How to avoid:** Treat the config redirect as covering the state dir too — Success Criterion 3
lists them separately but they share one resolver. Decide per fixture whether
`persistence_root_for_test` is still needed once the redirect exists; keeping both is fine, but the
plan should say which is authoritative.
**Warning signs:** Assertions about a state file path that pass under `persistence_root_for_test`
and would pass equally with the config redirect broken.

## Code Examples

### Redirect-aware resolver (the shape every one of the five takes)

```rust
// baude-core/src/persist.rs — replaces the current config_base() at :840
fn config_base() -> PathBuf {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(dir) = crate::testing::config_dir_override() {
        return dir;
    }
    let real = real_config_base();
    #[cfg(any(test, feature = "test-support"))]
    crate::testing::assert_contained(&real, "baude config dir");
    real
}

fn real_config_base() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("baude")
}
```
The `real_*` body is the current code verbatim [VERIFIED: baude-core/src/persist.rs:840-847, read
this session]. Splitting it out is what lets TISO-04's scanner reach the real root while tests
cannot.

### The `bauded/src/push.rs` deduplication

```rust
// DELETE bauded/src/push.rs:27-33 entirely:
//   fn config_base() -> PathBuf {
//       std::env::var_os("XDG_CONFIG_HOME")
//           .map(PathBuf::from)
//           .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
//           .unwrap_or_else(|| PathBuf::from("."))
//           .join("baude")
//   }
// and rewrite the three call sites (:52, :189, :207):
let path = baude_core::persist::config_dir().join(VAPID_FILE);
```
The duplicate is byte-identical to `persist::config_base()` [VERIFIED: bauded/src/push.rs:27-33 and
baude-core/src/persist.rs:840-847, both read this session], so the production path does not change.

### Fixture preamble after extraction (the 10 copies in `manager.rs`)

```rust
// Before — repeated at manager.rs:2615, 2698, 2729, 2811, 2843, 2933, 3049, 3136, 3215, 3521
let root = std::env::temp_dir().join(format!("bauded-manager-reconcile-{}", std::process::id()));
let _ = std::fs::remove_dir_all(&root);
baude_core::git::set_worktrees_base_for_test(root.join("data"));
baude_core::hook::set_hook_command_for_test(format!(
    "{} hook", root.join("bin").join("baude").display()));

// After
let fixture = ManagerFixture::new("reconcile");   // holds TestRedirect + root; cleans on drop
let root = fixture.root();
```
The current preamble is verbatim from `bauded/src/manager.rs:2612-2618` [VERIFIED: read this
session]. Note it uses a pid-only suffix, so two tests with the same label in one process share a
root — `GitFixture` avoids that with `NEXT_FIXTURE: AtomicU64` and
`format!("baude-git-test-{}-{sequence}", std::process::id())` [VERIFIED: baude-core/src/git.rs:2706-2718,
read this session]. The extracted helper should adopt the sequence counter.

### Scanner enumeration skeleton

```rust
// baude-core/src/worktree_scan.rs — PRODUCTION code, no cfg gate
pub fn enumerate(base: &Path) -> std::io::Result<Vec<Candidate>> {
    let mut out = Vec::new();
    for workspace in read_dir_sorted(base)? {                       // <base>/<workspace>
        if !workspace.file_type()?.is_dir() { continue; }           // symlink_metadata-backed
        for repository in read_dir_sorted(&workspace.path())? {     // repository-<u64>
            let name = repository.file_name();
            let Some(key) = name.to_str()
                .and_then(|n| n.strip_prefix("repository-"))
                .and_then(|n| n.parse::<u64>().ok()) else { continue };  // shape filter
            out.push(Candidate::inspect(&repository.path(), &workspace.file_name(), key)?);
        }
    }
    Ok(out)
}
```
`repository-<u64>` and the `primary-<u64>` / `<label>-<u64>` children are the exact compositions at
`git.rs:1768-1774` and `git.rs:1796-1810` [VERIFIED: baude-core/src/git.rs:1768-1774, read this
session:
```rust
pub fn managed_default_worktree_path(repository_key: u64, checkout_key: u64) -> PathBuf {
    worktrees_base()
        .join(&crate::workspace::active().name)
        .join(format!("repository-{repository_key}"))
        .join(format!("primary-{checkout_key}"))
}
```
and git.rs:1805-1810: `.join(&crate::workspace::active().name).join(format!("repository-{repository_key}")).join(format!("{label}-{checkout_key}"))`].
The branch label is `sanitize`d and truncated to 48 bytes [VERIFIED: baude-core/src/git.rs:1785-1795
and 1796-1804, read this session], so the child name is **not** losslessly reversible to a branch —
the scanner must not try to recover branch identity from it.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Tests mutate process `HOME`/`XDG_*` | Thread-local redirects, because cases run in parallel | PR #82 / `725c558`, 2026-09-13 | The established in-repo rule; this phase extends it rather than revisiting it |
| Managed worktrees land wherever `XDG_DATA_HOME` points | `WORKTREES_BASE_OVERRIDE` + escape assert | v2.1.4, 2026-09-13 22:13 UTC | One of five paths covered; this phase covers the rest |
| `set_*_for_test` returns `()`, no reset | RAII guard | This phase | Nesting composes; the `#[must_use]` catches unbound construction |
| `REQUIRE_WORKTREES_OVERRIDE: AtomicBool` armed by the first fixture | Containment predicate under a cfg gate | This phase | Removes the fixture-order dependency named in Success Criterion 3 |
| No leak enumeration at all | `baude worktrees scan` | This phase | Greenfield — nothing in the repo walks the worktrees tree today |

**Deprecated/outdated:**
- `baude_core::git::set_worktrees_base_for_test` and `baude_core::hook::set_hook_command_for_test`
  as free setters returning `()` — folded into `TestRedirect`.
- `REQUIRE_WORKTREES_OVERRIDE` — delete rather than extend; two guard mechanisms means the weaker
  one decides.
- The private `config_base()` in `bauded/src/push.rs:27` — a duplicate of `persist::config_base()`.

## Project Constraints (from PROJECT.md)

There is no `CLAUDE.md` or `.claude/CLAUDE.md` in this repository, and no `.claude/skills/` or
`.agents/skills/` directory [VERIFIED: `ls` of the repo root and `ls .claude/skills .agents/skills`,
this session]. The governing directives are in `.planning/PROJECT.md:101`:

- **Preserve user hooks and settings** — the phase touches `hook.rs`'s override only, not seeding
  (that is Phase 9's HREG-03).
- **Never remove state locks or overwrite another live owner** — the scanner must not touch
  `.state-*.json.lock` files; `release_state_lock_for_test` stays test-only even after being made `pub`.
- **Isolate tests per fixture without global-environment races** — the thread-local rule; do not
  reintroduce env mutation.
- **Never automatically delete real user worktrees; cleanup requires preview and manual approval** —
  `scan` is the default and authorizes nothing; `--prune` requires `--yes` plus re-verified evidence.

Additional repository conventions the plan must honour:
- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are CI gates (`ci.yml:17-18`).
  `--all-targets` means feature-gated code is linted; it must be warning-free.
- Tests run `cargo test -- --test-threads=1` in CI (`ci.yml:24`) on both `macos-14` and `ubuntu-22.04`.
- Commit scopes follow release-please conventions; the phase branch is
  `gsd/phase-08-test-isolation-and-fixture-ownership` per `.planning/config.json`.

## Integration Surface

Exhaustive call-site inventory the plan must cover [VERIFIED: `grep -rn "config_dir()\|config_base()"`
and `grep -rn "workspace::active()\|workspace::initialize"` over all three crates, this session].

**`config_base()` / `config_dir()` — 12 sites**

| Site | Role |
|------|------|
| `baude-core/persist.rs:840` | the resolver (redirect here) |
| `baude-core/persist.rs:850-852` | `pub fn config_dir()` — passthrough |
| `baude-core/persist.rs:540` | state lock path |
| `baude-core/persist.rs:947` | `load_config()` |
| `baude-core/persist.rs:978, 994, 1000, 1004` | load/save wrappers |
| `baude/main.rs:291` | folder-workspace memory root |
| `baude/app.rs:1277` | breadcrumbs root |
| `bauded/push.rs:27` | **duplicate resolver — delete** |
| `bauded/push.rs:52, 189, 207` | VAPID + subscriptions |

**`claude_config_dir()` — 3 sites:** the resolver at `meta.rs:24`, callers at `meta.rs:217` (session
files) and `meta.rs:261` (transcripts).

**`worktrees_base()` — 3 sites:** the resolver at `git.rs:1750`, callers at `git.rs:1771` and
`git.rs:1806`.

**`workspace::active()` — 27 call sites** across `persist.rs` (2), `git.rs` (2), `backend/mod.rs` (1),
`baude` (17, of which 9 are in `#[cfg(test)]` blocks), `bauded` (3). `initialize` is called once, at
`baude/main.rs:299`. Keeping `active()`'s signature avoids all 27.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `rustc` | all work | ✓ | 1.98.1 (48a229cea 2026-09-01) | — |
| `cargo` | all work | ✓ | 1.98.1 (797e8a9bc 2026-08-05) | — |
| `git` | test fixtures, scanner | ✓ | used throughout the existing suite | — |
| `dirs` crate | path resolution | ✓ | 5.0.1 (`dirs-sys` 0.4.1) | — |
| Real leak dataset for validation | TISO-04 acceptance | ✓ | 1433 dirs, 1.2 GB, 3 workspaces | — |

**Missing dependencies with no fallback:** none
**Missing dependencies with fallback:** none

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in libtest (`#[test]`) + `tokio` 1 `#[tokio::test]` for `bauded` async tests |
| Config file | none — dev-dependencies in each crate's `Cargo.toml` |
| Quick run command | `cargo test -p baude-core --lib` (≈250 tests in the core crate) |
| Full suite command | `cargo test -- --test-threads=1` (CI parity, `ci.yml:24`) |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| TISO-01 | `persist::config_base()` honours the redirect | unit | `cargo test -p baude-core --lib persist::tests::config_dir_honours_redirect` | ❌ Wave 0 |
| TISO-01 | `meta::claude_config_dir()` honours the redirect | unit | `cargo test -p baude-core --lib meta::tests::claude_config_dir_honours_redirect` | ❌ Wave 0 |
| TISO-01 | `PushState::load` writes VAPID + subscriptions inside the fixture | unit | `cargo test -p bauded --lib push::tests::vapid_and_subs_stay_in_fixture` | ❌ Wave 0 — see §Finding 4, no existing coverage |
| TISO-01 | No test run touches the real config dir | integration | `cargo test -- --test-threads=1` then assert `~/.config/baude` mtime unchanged | ❌ Wave 0 (manual/scripted check) |
| TISO-02 | Two fixtures on different threads resolve different workspace identities | unit | `cargo test -p baude-core --lib workspace::tests::per_fixture_identity_is_independent` | ❌ Wave 0 |
| TISO-02 | `initialize()` takes config by parameter; no real-config read on the identity path | unit | `cargo test -p baude-core --lib workspace::tests::initialize_uses_injected_config` | ❌ Wave 0 |
| TISO-03 | `baude-core`'s own test binary panics on an unguarded real resolution | unit | `cargo test -p baude-core --lib testing::tests::unguarded_resolution_panics` (`#[should_panic]`) | ❌ Wave 0 |
| TISO-03 | `baude`'s test binary has the gate compiled in | unit | `cargo test -p baude --lib app::tests::test_support_gate_is_active` | ❌ Wave 0 — proves Finding 1's fix |
| TISO-03 | `bauded`'s test binary has the gate compiled in | unit | `cargo test -p bauded --lib manager::tests::test_support_gate_is_active` | ❌ Wave 0 |
| TISO-03 | The guard covers all five paths | unit | `cargo test -p baude-core --lib testing::tests::guard_covers_every_path` | ❌ Wave 0 |
| TISO-03 | The dogfood child still passes under the guard | integration | `cargo test -p baude --lib local_tui_dogfood_real_git_flow_survives_restart_without_duplicates` | ✅ exists (`app.rs:7641`) — must keep passing |
| TISO-04 | Shape-matching candidate that state references is classified `Live` | unit | `cargo test -p baude-core --lib worktree_scan::tests::state_reference_blocks_removal` | ❌ Wave 0 |
| TISO-04 | Missing gitdir alone yields `Indeterminate`, never `Removable` | unit | `cargo test -p baude-core --lib worktree_scan::tests::missing_gitdir_never_authorizes` | ❌ Wave 0 — the literal TISO-04 wording |
| TISO-04 | Unreadable state file blocks every candidate in that workspace | unit | `cargo test -p baude-core --lib worktree_scan::tests::unreadable_state_blocks` | ❌ Wave 0 |
| TISO-04 | A symlink candidate is refused | unit | `cargo test -p baude-core --lib worktree_scan::tests::symlink_candidate_refused` | ❌ Wave 0 |
| TISO-04 | `scan` without `--prune --yes` removes nothing | integration | `cargo test -p baude --lib main::tests::scan_is_read_only` | ❌ Wave 0 |
| TISO-04 | `--prune` re-verifies evidence at removal time | unit | `cargo test -p baude-core --lib worktree_scan::tests::prune_reverifies` | ❌ Wave 0 |

### Sampling Rate

- **Per task commit:** `cargo test -p <crate> --lib` for the crate touched, plus `cargo fmt --check`.
- **Per wave merge:** `cargo test -- --test-threads=1` (matches CI) plus
  `cargo clippy --all-targets -- -D warnings`.
- **Phase gate:** full suite green on both `macos-14` and `ubuntu-22.04`, plus a manual run of
  `baude worktrees scan` against the real 1433-directory dataset with output reviewed and nothing deleted.

### Wave 0 Gaps

- [ ] `baude-core/src/testing.rs` — new module; the guard has no home today
- [ ] `baude-core/Cargo.toml` `[features] test-support = []`
- [ ] `baude/Cargo.toml` + `bauded/Cargo.toml` `[dev-dependencies]` feature wiring
- [ ] `baude-core/src/worktree_scan.rs` — new module, entirely greenfield
- [ ] A shared `ManagerFixture` helper in `bauded/src/manager.rs` to replace the 10 copied preambles
- [ ] A scripted real-config-untouched assertion for the full-suite run (no existing mechanism)

No framework install is needed — libtest and `tokio` are already present.

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | No authentication surface in this phase |
| V3 Session Management | no | No session tokens; "session" here means a PTY |
| V4 Access Control | **yes** | The `--prune` path is a destructive operation on user data. Control: two-step approval (`scan` → `--prune --yes`) with evidence re-derived at removal time; `Indeterminate` is the default verdict, so absence of evidence never authorizes |
| V5 Input Validation | **yes** | CLI arguments and every filesystem path. Control: parse `repository-<u64>` with `str::parse::<u64>()` (rejects overflow and non-numeric); classify with `symlink_metadata`, never `metadata`/`exists()`; refuse any candidate outside the canonicalized real base |
| V6 Cryptography | **yes (do not touch)** | VAPID keys move resolvers only. The p256/HKDF/AES-GCM construction in `push.rs` is unchanged; do not regenerate, re-encode, or "improve" the key file format. Rotation is explicitly deferred |
| V12 File and Resource | **yes** | Path traversal and symlink following on the prune path; see V5 controls |

### Known Threat Patterns for Rust + filesystem tooling

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Symlinked candidate causes deletion outside the base | Tampering / Denial | `symlink_metadata` for all classification; refuse symlink candidates outright. Precedent: `git.rs:2594` |
| TOCTOU between preview and prune | Tampering | Re-derive every evidence item immediately before removal (the locked two-step design) |
| Absence of evidence read as proof of non-ownership | Tampering | `Indeterminate` default; an unreadable state file blocks its whole workspace |
| Missing gitdir treated as ownership | Tampering | Explicit TISO-04 prohibition; all 1433 candidates would qualify |
| Guard silently absent in a test binary | Information Disclosure (real user data written by tests) | The `test_support_gate_is_active` assertions in each downstream crate |
| VAPID private key written to a shared/real location | Information Disclosure | The `push.rs` redirect; the key is a signing key for push endpoints |
| `u64` key parse overflow | Tampering | `parse::<u64>()` returns `Err` on overflow; treat as shape mismatch, skip |

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The effective MSRV is already at or above the release that stabilized `File::try_lock` (used at `persist.rs:588`); adding a cargo feature does not raise it | Standard Stack | None practically — no MSRV is declared anywhere and CI pins nothing; worst case is a note in the PR |
| A2 | libtest's one-thread-per-test behaviour (Finding 2) is an implementation detail, not a stability guarantee, so the RAII reset remains worth having even though nothing leaks today | Finding 2 | If a future toolchain reuses threads, the reset becomes load-bearing — which is the argument for building it now |
| A3 | `bauded`'s `bin` target is not exercised by any test, so no test path reaches `main.rs:182`'s `PushState::load(true)` | Finding 4 | If a future integration test spawns the `bauded` binary, the redirect must reach it by env, not thread-local |
| A4 | The `prerelease` workspace's 166 directories are test leaks like the `opencode` ones, judged by pid-derived keys and test-named branch labels | Finding 5 | Low — the layered ownership design does not depend on this classification; it is context, not a rule |
| A5 | Extracting the 10 `manager.rs` preambles into one helper is low-risk because each currently differs only in its label string | Code Examples | Medium — some of the 10 may set up more than the preamble; the plan should diff all 10 before extracting |
| A6 | `cargo clippy --all-targets` enabling `test-support` causes no clippy failures in the gated code | Finding 1 build audit | Low — a CI failure caught on the first push, fixed by satisfying clippy |

## Open Questions

1. **Does this phase also fix the production empty-parent leak (§Finding 6)?**
   - What we know: `create_dir_all(parent)` at `git.rs:1069` and `git.rs:1568` leaves an empty
     `repository-<key>` on any downstream failure; nothing removes it; two such directories appeared
     on 2026-09-13 *after* v2.1.4.
   - What's unclear: it is not covered by TISO-01..04, and the phase boundary explicitly defers
     *deleting* leaked directories.
   - Recommendation: **out of scope for implementation, in scope for documentation.** Record it as a
     v2.2 follow-up issue during planning. If the team wants it now, the fix is a compensating
     `let _ = std::fs::remove_dir(parent);` on the error paths (`remove_dir` refuses non-empty
     directories, so it is inherently safe — the same reasoning already used at `git.rs:2322`).
     Raise it as a deviation rather than silently expanding scope.

2. **Is `persistence_root_for_test` retired once the config redirect lands?**
   - What we know: `App` carries `persistence_root_for_test: Option<PathBuf>` used at 20+ test sites
     (`app.rs:526, 769, 1202, 1572`, and 16 assignments in tests), and `Manager` has the analogous
     `persistence_target_for_test` (`manager.rs:99`).
   - What's unclear: with `config_dir()` redirected, state files already land in the fixture, so the
     explicit root becomes redundant — but it also serves failure injection
     (`save_current_at_test(root, file, state, atomic_failure_for_test, None)`).
   - Recommendation: keep both; redirect is the containment mechanism, `persistence_root_for_test`
     stays for failure injection. Say so explicitly in the plan so the two do not disagree
     (§Pitfall 7).

3. **What does `scan` output look like, and does it need `--json`?**
   - What we know: 1433 candidates is too many for unstructured text; the repo already depends on
     `serde_json` in all three crates.
   - What's unclear: whether a human-readable summary (counts by workspace and verdict, with a
     `--verbose` per-candidate listing) is enough for the deferred deletion step.
   - Recommendation: default to a grouped summary plus a `--json` flag for the eventual scripted
     deletion. This is the "exact CLI noun/verb naming" discretion item extended one notch.

4. **Should the scanner live in `baude-core` or the `baude` binary?**
   - What we know: it needs `worktrees_base()` (private), `discover_repository`, and state loading —
     all core-internal. The CLI arm belongs in `baude/src/main.rs`.
   - What's unclear: whether `bauded` should also expose it.
   - Recommendation: logic in `baude-core::worktree_scan`, CLI arm in `baude` only. `bauded` is
     headless and has no operator at the keyboard to approve a deletion.

## Sources

### Primary (HIGH confidence)
- Local cargo probes, rustc/cargo 1.98.1, run 2026-09-13 — cross-crate `cfg(test)`, dev-dependency
  feature unification, libtest thread-per-test, `Box::leak` + thread-local `&'static` override.
  Output pasted inline in Findings 1, 2, 3.
- Repository source read this session: `baude-core/src/{git,persist,meta,workspace,hook,repository}.rs`,
  `baude/src/{main,app}.rs`, `bauded/src/{push,manager,api,main}.rs`, `Cargo.toml` ×4,
  `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `Dockerfile`, `.planning/PROJECT.md`.
- Live filesystem measurement: `~/.local/share/baude/worktrees` (1433 dirs) and `~/.config/baude`
  (8 state files + orphaned temps), run 2026-09-13.
- `~/.cargo/registry/src/…/dirs-sys-0.4.1/src/lib.rs:33-37` — unix `home_dir()` honours `$HOME`.
- `git log` / `git log --tags` — PR #82 at `725c558` (2026-09-13 15:10:03 -0700), `v2.1.4` tag
  (2026-09-13 22:13:55 UTC).

### Secondary (MEDIUM confidence)
- https://doc.rust-lang.org/cargo/reference/resolver.html — resolver v2 dev-dependency feature
  unification rules.
- https://doc.rust-lang.org/reference/conditional-compilation.html — the `test` cfg predicate.

### Tertiary (LOW confidence)
- None. No claim in this document rests on a web search alone.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — zero new external packages; every mechanism verified by running it here.
- Architecture: HIGH — the three load-bearing patterns (cross-crate gate, `&'static` override,
  containment predicate) were each compiled and executed in this environment.
- Pitfalls: HIGH for 1-5 (each traced to a verified line or probe), MEDIUM for 6-7 (inference from
  verified facts about code paths, not directly exercised).
- TISO-04 ownership rules: HIGH on the evidence (live counter-examples measured), MEDIUM on the
  proposed verdict taxonomy (a design proposal, not a verified artifact).

**Note on the confidence seam:** `gsd-tools query classify-confidence --provider webfetch --verified`
returns `LOW` for the two documentation lookups. Those two claims are tagged `[CITED: …]`
accordingly. Every `[VERIFIED: …]` tag in this document rests on a command run in this session with
its output pasted, or on a source file opened with `Read`/`sed` and quoted verbatim — not on the
webfetch provider tier.

**Research date:** 2026-09-13
**Valid until:** 2026-10-13 (30 days — the Rust and Cargo behaviours are stable; the live leak
dataset will drift and should be re-measured before TISO-04 acceptance)
