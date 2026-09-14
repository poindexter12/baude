---
phase: "8"
slug: "test-isolation-and-fixture-ownership"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: true
wave_0_complete: true
created: "2026-09-13"
---

# Phase 8 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `libtest` (`#[test]` / `#[should_panic]`), cargo workspace, edition 2021 |
| **Config file** | none — `Cargo.toml` workspace at repo root; no external test runner |
| **Quick run command** | `cargo test -p <package> <target-filter> <name-filter>` — see the target-filter table below |
| **Full suite command** | `cargo test -- --test-threads=1` (CI parity, `.github/workflows/ci.yml`) |
| **Estimated runtime** | quick filter ~5-20s warm; full serial suite ~4-6 min (spawns real PTYs and a re-exec'd dogfood child) |

### Target filters — binary-only crates do not accept `--lib`

`baude` and `bauded` have no `src/lib.rs` and no `[lib]` section. `cargo test -p baude --lib`
fails with *"no library targets found in package `baude`"* — a dead gate. Verified in this
environment: `--lib` errors; `--bins` resolves `Executable unittests src/main.rs`.

| Package | Correct form | Why |
|---------|--------------|-----|
| `baude` | `cargo test -p baude --bins [filter]` | binary-only |
| `bauded` | `cargo test -p bauded --bins [filter]` | binary-only |
| `baude-core` | `cargo test -p baude-core --lib [filter]` | real library crate |

### Gate hygiene — never read an exit status through a pipe

`cmd 2>&1 | tail -5` exits with `tail`'s status, so a failed build passes the gate. No
`<automated>` command in this phase pipes a gate into another command. Where two independent
results must both be reported (plan 06 task 2), each status is captured into its own variable
on its own line and tested afterwards.

---

## Sampling Rate

- **Fast feedback during tasks:** run the named contained filters in each PLAN. Target
  approximately 5-20 seconds warm for resolver/pure tests; git-backed fixture groups may
  take 20-60 seconds or longer. These are estimates, not measured maxima.
- **Wave 1 and concurrent wave 2:** no broad crate/binary/workspace tests. Only new contained
  tests and compile-only checks run. 08-03's fixture_identity_isolation filters are owner/
  identity/path-only and construct no App or PTY; UI tests are authored/compiled, not run.
- **Wave 3 worker boundary:** 08-08 first makes App usage/ambient remote workers inert,
  runs worker_isolation_app_ plus ui_fixture_isolation_, then contains the PTY child
  environment and runs worker_isolation_pty_. 08-04 remains parallel and synthetic-only.
- **First broad-test boundary:** 08-06 task 2 in wave 4, after 01/02/03/08 and 06 task 1.
  This includes app/API/UI/core/manager guards and identities, returned lifetimes, inert
  App workers, contained PTY environments, and the dogfood child's config environment.
  Repeat worker/UI filters before the full serial suite and full downstream binaries with
  default concurrency, bracketed by the external observer. No-write snapshots do not
  certify absence of reads; synthetic worker/child tests supply separate no-read evidence.
- **End-of-phase integration:** after wave 5, repeat the suite/observer pair, clippy,
  formatting and release build so scanner/CLI changes receive full regression coverage.
- **Latency accounting:** 08-01 task 2 also contains a compile-only check; 08-01 task 3
  contains a release build; 08-03 task 2 compiles all targets; 08-06 task 2 is a full
  integration gate (~4-6 minutes warm for the serial suite alone). Clippy/release builds
  and cold builds can take several additional minutes. None has a sub-20-second claim.
- **Failure direction:** every PLAN automated block has an adjacent fails_when. Nonzero
  exit or a missing intended test fails; an empty Cargo filter is not accepted as evidence.
  Expected-panic tests fail if the guarded call returns normally. Manual/integration
  results remain pending until observed; this revision executed no tests or real-root checks.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 8-01-01 | 01 | 1 | TISO-01, TISO-03 | T-08-01 / T-08-02 | Downstream observes dependency-gated config resolver | tracer | `cargo test -p baude --bins test_support_gate_is_active && cargo test -p baude-core --lib persist::tests::config_dir_honours_redirect` | New tests in task | Pending |
| 8-01-02 | 01 | 1 | TISO-01, TISO-03 | T-08-01 / T-08-09 | Root/command RAII and complete setter migration compile | unit + compile-only | `cargo test -p baude-core --lib testing:: && cargo check --workspace --all-targets --locked` | New tests in task | Pending |
| 8-01-03 | 01 | 1 | TISO-03 | T-08-02 / T-08-23 / T-08-03 | Escape panics in all harnesses; release compiles | unit + release build | `cargo test -p baude --bins unguarded_resolution_panics && cargo test -p bauded --bins unguarded_resolution_panics && cargo test -p baude-core --lib unguarded_resolution_panics && cargo build --workspace --release --locked` | New tests in task | Pending |
| 8-02-01 | 02 | 2 | TISO-01, TISO-03 | T-08-04 | Synthetic Claude resolver/poll tests; escape panics | unit | `cargo test -p baude-core --lib meta::tests::claude_config_dir_` | New tests in task | Pending |
| 8-02-02 | 02 | 2 | TISO-01 | T-08-01 / T-08-08 / T-08-10 | VAPID/subscriptions in synthetic roots; no developer-root observation | store tests | `cargo test -p bauded --bins push::tests::store_isolation_` | New store cases in existing crypto module | Pending |
| 8-03-01 | 03 | 2 | TISO-02, TISO-03 | T-08-05 / T-08-11 / T-08-24 | Reader-only identity; override required before cache lookup; thread-local read counter | unit/concurrency | `cargo test -p baude-core --lib workspace::` | Extended tests in task | Pending |
| 8-03-02 | 03 | 2 | TISO-01, TISO-02, TISO-03 | T-08-06 / T-08-24 | Explicit startup and retained app/API/UI guards; executes owner-only filters, compiles UI cases | unit + compile-only | `cargo test -p baude-core --lib workspace:: && cargo test -p baude --bins fixture_identity_isolation && cargo test -p bauded --bins fixture_identity_isolation && cargo check --workspace --all-targets --locked` | New tests in task; UI cases execute at 8-08-01 | Pending |
| 8-04-01 | 04 | 3 | TISO-04 | T-08-12 | Removal predicate needs human response before implementation | checkpoint:decision | N/A; blocking human gate | N/A | Not approved |
| 8-04-02 | 04 | 3 | TISO-04 | T-08-12 / T-08-15 | Weak evidence never clears; blockers win | pure unit | `cargo test -p baude-core --lib worktree_scan::tests::verdict` | New module/tests in task | Pending |
| 8-04-03 | 04 | 3 | TISO-04 | T-08-04 / T-08-13 / T-08-14 | Read-only synthetic scan, symlink refusal and non-locking reader | filesystem fixture | `cargo test -p baude-core --lib worktree_scan::` | Extended tests in task | Pending |
| 8-05-01 | 05 | 4 | TISO-04 | T-08-16 | All state files, repo keys and path overlaps checked; unreadability blocks clearing globally | unit | `cargo test -p baude-core --lib worktree_scan::` | Extended tests in task | Pending |
| 8-05-gate | 05 | 4 | TISO-04 | T-08-05 | Prune semantics need separate human response | checkpoint:decision | N/A; blocking human gate | N/A | Not approved |
| 8-05-02 | 05 | 4 | TISO-04 | T-08-05 / T-08-04 / T-08-18 / T-08-25 | Prior report/proof and fresh re-verification; no confirmation means no removal | unit | `cargo test -p baude-core --lib worktree_scan::` | Extended tests in task | Pending |
| 8-06-01 | 06 | 4 | TISO-01, TISO-02, TISO-03 | T-08-09 / T-08-21 | Helper holds root and literal identity through use and PTY teardown, including persist=false | fixture unit | `cargo test -p bauded --bins manager::tests::fixture_isolation_` | New helper tests in task | Pending |
| 8-06-02 | 06 | 4 | TISO-01, TISO-02, TISO-03 | T-08-20 / T-08-22 / T-08-26 / T-08-28 | Worker/UI no-read controls before suite; observer failures independent | longer integration gate | `bash scripts/assert-real-roots-untouched.sh --self-test && cargo test -p baude --bins worker_isolation_app_ && cargo test -p baude --bins ui_fixture_isolation_ && cargo test -p baude-core --lib worker_isolation_pty_ && bash scripts/assert-real-roots-untouched.sh before && { cargo test -- --test-threads=1; status=$?; bash scripts/assert-real-roots-untouched.sh after; after=$?; test "$status" -eq 0 -a "$after" -eq 0; }` | Script/self-tests created in task | Pending |
| 8-07-01 | 07 | 5 | TISO-04 | T-08-17 / T-08-18 / T-08-19 / T-08-25 | CLI consumes inspected report; no new candidate enters prune | CLI fixture | `cargo test -p baude --bins worktrees_cli_` | New tests in task | Pending |
| 8-08-01 | 08 | 3 | TISO-01, TISO-02, TISO-03 | T-08-26 / T-08-27 | Inert App workers; real App/UI render retains fixture roots and identity | tracer, synthetic child | `cargo test -p baude --bins worker_isolation_app_ && cargo test -p baude --bins ui_fixture_isolation_` | Worker test created in task; UI cases authored by 03 | Pending |
| 8-08-02 | 08 | 3 | TISO-01, TISO-02, TISO-03 | T-08-28 / T-08-29 | Test PTY child roots/startup inputs contained before registration | synthetic child + compile-only | `cargo test -p baude-core --lib worker_isolation_pty_ && cargo check --workspace --all-targets --locked` | New tests and five existing fixture migrations in task | Pending |

This map records scheduled checks, not observed passes. All automatic commands have an
adjacent scenario-specific fails_when in their PLAN. Empty filters and nonzero exits fail;
expected-panic tests fail when a guarded call returns normally.

---

## Wave 0 Requirements

No separate Wave 0 pass is needed: every new test/scaffold named in the map is created by
its owning task before implementation or verification. Existing source-file presence does
not mean the new test exists. The framework is cargo's built-in libtest, already in use
across all three crates.

- `baude-core/src/worktree_scan.rs` — created by task 8-04-02 before its own tests run; 8-04-03 and both plan 05 tasks extend the same module.
- `bauded/src/push.rs` store-isolation tests are added by 8-02-02 to the existing crypto test module; the filesystem store path has no coverage today.
- `scripts/assert-real-roots-untouched.sh` — created by task 8-06-02 together with its CI wiring.
- `baude-core/src/testing.rs` — created by task 8-01-01; every later plan consumes it, which is why plan 01 is the wave-1 tracer.

Sampling continuity: no three consecutive tasks lack automated verification. The two
human decision checkpoints (08-04 task 1 and the 08-05 prune gate) remain unapproved and
are bracketed by automated implementation tasks.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| `baude worktrees scan` against the developer's real 1433-entry tree reports a grouped summary, removes nothing, and marks no directory holding a real checkout as removable | TISO-04 | The dataset is this machine's real `~/.local/share/baude/worktrees`. It cannot be committed as a fixture, and the two live `claude` directories that must not be cleared exist only here. The automated equivalent runs against a synthetic fixture tree. | Run `cargo run -p baude -- worktrees scan`. Confirm the summary prints; confirm `claude/repository-2` and `claude/repository-14` are not reported removable; confirm `claude/repository-1` and `claude/repository-5` are blocked as referenced by state. Record the observed counts per verdict in `08-07-SUMMARY.md`. |
| A full local `cargo test -- --test-threads=1` leaves the three real roots unchanged | TISO-01, TISO-02, TISO-03 | CI runners have no populated real roots, so the CI green signal is weaker than the local one. The measurement that decides whether the phase goal is met must be taken on a machine that has them. | Run the `before` / suite / `after` triple from task 8-06-02 locally and record in `08-06-SUMMARY.md` whether any root changed. A changed root is a gap-closure finding, not something to work around by loosening the script. |

---

## Multi-Source Coverage Audit

Coverage means planned, not implemented or approved. The two human decision gates remain
pending. IDs follow the stable prose-decision map in 08-01; CONTEXT itself is unchanged.

| Source | ID / feature | Plan | Status / disposition |
|--------|--------------|------|----------------------|
| GOAL | Suite cannot access developer config/state/Claude files | 01, 02, 03, 06, 08 | COVERED: contained resolvers, fixture-owned identity, inert usage workers, contained PTY child env and external no-write observer |
| GOAL | Historical leaks inspectable before deletion | 04, 05, 07 | COVERED: read-only preview and separately gated report-bound prune |
| REQ | TISO-01 | 01, 02, 03, 06, 08 | COVERED: retained root guards across core/app/API/UI/manager/push and explicit subprocess roots |
| REQ | TISO-02 | 03, 06, 08 | COVERED: literal per-thread identity, no parent environment mutation |
| REQ | TISO-03 | 01, 02, 03, 06, 08 | COVERED: unconditional guard, expected-panic proofs, cache-bypass and pre-spawn regressions |
| REQ | TISO-04 | 04, 05, 07 | COVERED: complete state protection, inspected-report transport and re-verification; human gates pending |
| CONTEXT | D-01 RAII config redirect | 01 | COVERED |
| CONTEXT | D-02 shared push config resolver | 02 | COVERED |
| CONTEXT | D-03 thread-local Claude redirect | 02 | COVERED |
| CONTEXT | D-04 one unified redirect guard | 01, 03, 06 | COVERED: scoped hook/workspace updates use the same storage and guard |
| CONTEXT | D-05 override-first identity with production OnceLock | 03 | COVERED |
| CONTEXT | D-06 injected config with no identity-path reads | 03 | COVERED: explicit startup, same-thread read instrumentation |
| CONTEXT | D-07 unchanged active signature | 03 | COVERED |
| CONTEXT | D-08 no-override panic | 03, 06 | COVERED: checked before any cached identity, including contained child paths |
| CONTEXT | D-09 first-instruction arming | 01 | COVERED: existing documented cross-crate mechanism correction retained |
| CONTEXT | D-10 panic on escape | 01, 02, 03 | COVERED |
| CONTEXT | D-11 all five path categories | 01, 02, 06 | COVERED |
| CONTEXT | D-12 no shipped test support | 01, 06 | COVERED: dev-only feature wiring plus release graph check |
| CONTEXT | D-13 CLI, not TUI | 07 | COVERED |
| CONTEXT | D-14 ownership predicate | 04, 05 | COVERED by genuine human checkpoint and fail-closed implementation tasks; proposed correction not approved |
| CONTEXT | D-15 separate approval and re-verification | 05, 07 | COVERED; exact prune semantics remain pending human response |
| CONTEXT | D-16 preview default | 04, 05, 07 | COVERED |
| CONTEXT | D-17 dogfood child discretion | 01, 03, 06 | COVERED: child-only env plus explicit literal identity |
| CONTEXT | D-18 manager helper discretion | 06 | COVERED |
| CONTEXT | D-19 CLI naming discretion | 07 | COVERED |
| RESEARCH | Findings 1-3: cross-crate cfg, libtest scope, static lifetime | 01, 03 | COVERED: dependency behavior proof, nested/concurrent tests, bounded fixture allocation |
| RESEARCH | Findings 4/7: new push store tests and lock helper visibility | 01, 02, 04 | COVERED: existing crypto tests retained, new store coverage, read-only scanner avoids locking loader |
| RESEARCH | Finding 5: live counterexamples and weak ownership signals | 04, 05 | COVERED: realistic empty repo-1/repo-5 fixtures, all state files, key/path parent-child exclusions, global fail-closed uncertainty |
| RESEARCH | Finding 8: environment-contained re-exec | 01, 03, 06 | COVERED: exact per-root env precedence and fixture identity scope |
| RESEARCH | Patterns 1-6 and pitfalls 1-5/7 | 01-08 | COVERED: scoped guard lifetimes/custom commands, no env races, unchanged poll interfaces, strict scan inputs, symlink/TOCTOU refusal, existing git safety reuse |
| RESEARCH | UI constructor census and worker-boundary correction | 03, 06, 08 | COVERED: 44 test App sites including five UI sites; retained hierarchy helper owner, inert ccusage/ambient remote, explicit no-startup-file PTY env and synthetic regressions |
| RESEARCH | Output/placement/explicit-root questions | 01, 04, 05, 07 | COVERED: four RESOLVED entries record existing choices, full JSON report transports proof |
| RESEARCH | Security/no-new-packages constraints | 01-07 | COVERED: threat registers, existing std/serde tooling, no package installs |

Excluded by existing source scope: production empty-parent cleanup (Finding 6), actual
historical deletion, orphaned real atomic-write temps, VAPID rotation, and work assigned
to phases 9-12. Nothing from those exclusions is implemented by these plans. No uncovered
in-scope source item was found in this revision.

## Revision Scope and Dependency Check

Eight plans, five waves: 1=[01], 2=[02,03], 3=[04,08], 4=[05,06], 5=[07].
08 depends on 01/02/03; 06 adds 08; 07 depends explicitly on 05/06. The new app.rs
writer runs after 03, and no same-wave modified-file overlap is introduced. Both human
decision checkpoints remain pending. STATE/config and runtime locks are not updated by
this planning-only revision.

The 08-01 scope warning remains for human escalation, not a waived pass: 11 files,
62,000 raw/calibrated tokens, factor 1 with zero samples and low confidence. Its scope_budget
records why a one-file reassignment leaves incomplete setter/lifetime migration; a larger
additive-API/migration split requires deliberate re-decomposition. The separate 08 worker
plan addresses newly found scope without pretending to shrink 01. 03 is 54,000 tokens;
08 is 38,000 tokens. These are projections, not measured execution costs.

## Validation Sign-Off

- [x] All implementation tasks have automated verification; both decision checkpoints require actual human responses
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references — each is created by the task that first needs it
- [x] No watch-mode flags
- [x] Fast filter targets distinguished from compile/release and 4-6 minute full-suite integration gates; no measured maximum claimed
- [x] No `--lib` filter on a binary-only crate; no gate read through a pipe
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
