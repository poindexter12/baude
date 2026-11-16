# Phase 22: Validation and v2.8.0 Release - Research

**Researched:** 2026-11-13
**Domain:** Rust in-crate integration tests over real git and PTY fixtures, README prose, release-please mechanics
**Confidence:** MEDIUM. Release mechanics and the docs facts were read in source this session. The coverage matrix is built from test names, fixture greps and six test bodies read in part; cells marked "by name" were not read in full.

How to read the tags: `[VERIFIED: file:line]` means the file was opened or the command was run this session. `[BY NAME]` means the test exists at that line but its body was not read, so what it asserts is inferred from its name. `[ASSUMED]` means training knowledge or inference, not checked.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### Shipping v2.8.0
- The autonomous run pushes the branch and opens ONE pull request to `main` with a `feat` title, then stops. The owner merges it. Nothing in this phase merges to `main`. — **Reversibility:** one-way — a merge to protected `main` triggers release-please and a published v2.8.0; a pushed branch and an open PR can still be closed.
- After the owner merges, release-please opens the 2.8.0 release PR and the minor bump auto-merges after its soak. Success criterion 3 is therefore only fully met after the owner's merge; the phase records that as the one open item rather than claiming a release.
- Phases 18, 19 and 20 are re-verified with the automated verifier before the push. A human-needed result goes back to the owner as a question. — **Reversibility:** costly — releasing unverified phases cannot be recalled once v2.8.0 is published.
- The intermittent macOS teardown 500 is NOT fixed in this phase. It is recorded as BL-11 in `.planning/BACKLOG.md` and v2.8.0 ships with it known.

#### Decided autonomously
- [Tests] Q: 'Where do the REL-01 tests live?' → Selected: 'In-crate test modules, as every existing test does; the repo has no tests directories' — moving a test module is a local edit
- [Tests] Q: 'What matrix?' → Selected: 'Four row kinds, with a live runtime and with none, on cloned and git init fixtures, asserting on process, worktree, binding and marker state read from the fixture' — test-only
- [Tests] Q: 'Browser automation for the PWA?' → Selected: 'None. Add a Rust test that subscribes to /events over real HTTP and sees rows_removed for a real removal' — no new dependency, test-only
- [Docs] Q: 'Where do the docs go?' → Selected: 'README: a new Removing projects section, the Keys table, and the bauded section for the removal endpoints, the SSE event and the PWA modal' — prose edits
- [Release] Q: 'Changelog and version?' → Selected: 'release-please owns both; no hand edits and no Release-As footer unless the computed bump is wrong' — nothing is written by hand
- [Release] Q: 'Gates?' → Selected: 'scripts/gate.sh and scripts/gate.sh --linux on the final commit before any push, exit codes read directly' — process only

### Claude's Discretion
- Exact test names, fixture helpers and how the matrix is split across `baude-core`, `baude` and `bauded`.
- README wording and section order.
- Whether Phase 21 code-review warnings still open at planning time are fixed here or left to the review-fix pass.

### Deferred Ideas (OUT OF SCOPE)
- BL-11: intermittent macOS teardown 500.
- Standalone removal leaves the daemon-written `.claude` directory; the PWA lists unclaimed sessions without an 'other sessions' heading; older state files serve `managed_branch` for a main checkout; the PWA shows full paths.
- Browser-level PWA tests.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| REL-01 | Integration tests cover each row kind with a live runtime and with no runtime, using fixtures shaped like real cloned and `git init` repositories, and assert on fixture output that process, worktree, binding and marker state are gone | Coverage Matrix, Gap List, Fixture Helpers, Pattern 1 and Pattern 2 |
| REL-02 | v2.8.0 ships through release-please with README and PWA docs describing the remove action, its confirmation, and what is and is not deleted | Docs Facts, Release Mechanics |
</phase_requirements>

## Summary

The removal behaviour is well tested per end state, and not at all per cell. `baude-core/src/removal.rs` proves process extinction, directory keep or delete, and branch preservation on both fixture shapes, and in a separate block it proves binding, marker and suppression clearing. No test in that second block starts a runtime `[VERIFIED: awk over baude-core/src/removal.rs:3987-4861 found zero `spawn_trapping_runtime` calls]`. So REL-01's sentence, "assert that process, worktree, binding and marker state are gone", is true of the suite as a whole and false of every single test. The planner's work for criterion 1 is one table-driven matrix that reads all four end states from the fixture in the same test, not a new set of behaviours.

The daemon tests drive the router in process with `tower::ServiceExt::oneshot`. The only test that binds a socket is `pty_websocket_round_trip` `[VERIFIED: bauded/src/api.rs:6746-6748]`. The locked decision for a real-HTTP `/events` test therefore has no existing test behind it; the six `events_` tests are in-process. The README has no word about `D`, `/removals`, `/rows` or `/events` `[VERIFIED: grep over README.md]`.

Release mechanics are clean. The branch is 223 commits ahead of `origin/main` (CONTEXT says 214), every commit passes the parenthesis guard, a simulated squash body of all 223 messages passes it too, and no commit carries a breaking marker. A `feat:` PR title takes 2.7.3 to 2.8.0. The minor release PR is labelled `release:minor`, held with auto-merge off, and merged by a 30-minute cron once it is 2 hours old and the three required checks are green.

**Primary recommendation:** write one all-four-end-states matrix in `baude-core/src/removal.rs`, one real-HTTP `/events` test in `bauded/src/api.rs` on the `pty_websocket_round_trip` pattern, close the two or three named cell gaps, add the README section from the facts below, and open the PR with a plain `feat:` title. Add as few new live-runtime daemon tests as the matrix allows, because each one is another draw on BL-11 in the required macOS check.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Kill, disk, binding and marker end states | `baude-core` (`removal.rs`) | — | The lifecycle operation lives here and both surfaces call it, so the full matrix belongs here |
| Runtime custody and handle hand-off | `bauded` (`manager.rs`), `baude` (`app.rs`) | `baude-core` | Each surface owns its session registry and moves handles into `removal::remove` |
| REST status codes and SSE publish | `bauded` (`api.rs`) | — | Wire contract only; end states are core's |
| Confirmation rendering | `baude` (`ui.rs`), PWA (`bauded/web/removal.js`) | — | Both render from the same `RemovalReport` |
| Version, tag, changelog | release-please (GitHub Actions) | — | Locked: nothing written by hand |

## Coverage Matrix (research question 1)

### Row kinds

Core has three targets and three row keys `[VERIFIED: baude-core/src/removal.rs:49-62]`:

```
pub enum RemovalTarget { Repository(RepositoryKey), Checkout(CheckoutKey), Standalone(StandaloneKey) }
pub enum RowKey       { Repository(RepositoryKey), Checkout(CheckoutKey), Standalone(StandaloneKey) }
```

A checkout splits by role and management `[VERIFIED: baude-core/src/repository.rs:284-288, baude-core/src/removal.rs:1088-1093]`:

```
pub enum CheckoutRole { Main, PrimaryDefault, ManagedBranch }
(CheckoutRole::Main, _) | (_, false) => DiskAction::KeepDirectory,
(_, true) => DiskAction::RemoveManagedWorktree,
```

That gives five user-visible kinds: repository parent, main checkout, managed worktree, unmanaged checkout, standalone. CONTEXT locks the matrix at four. The fifth, unmanaged, is already covered in core at `removal.rs:3560` and `:3708` `[BY NAME]`; keep it as it is and do not widen the locked matrix.

Callers: `Manager::preview_removal` at `bauded/src/manager.rs:1105` and `Manager::remove_rows` at `:1179` (from 21-VERIFICATION.md, not reopened this session); TUI `D` dispatch at `baude/src/app.rs:338`, modal at `:504`, `:4068`, `:4246` `[VERIFIED: grep]`.

### What "marker gone" means per kind

The four end states are not literal for every kind. The clearing vocabulary is `[VERIFIED: baude-core/src/removal.rs:152-159]`:

```
pub enum ClearingKind { Binding, Marker, RepositoryDir, SuppressionAdded, SuppressionDropped }
```

| Kind | Process | Worktree | Binding | Marker |
|------|---------|----------|---------|--------|
| Repository parent | every child group extinct | managed dirs gone, main and unmanaged kept | repository binding cleared | owned marker and repository dir cleared; child suppressions dropped |
| Main checkout | group extinct | directory KEPT | own key cleared | parent marker KEPT; suppression ADDED |
| Managed worktree | group extinct | directory gone, git registration gone, branch kept | own key cleared | parent marker KEPT; suppression ADDED |
| Standalone | group extinct | folder KEPT | own key cleared if one exists | none; no suppression written |

Source for the marker column: test names `repository_removal_clears_owned_marker_and_directory` (`removal.rs:4385`), `child_removal_keeps_parent_marker_and_writes_suppression` (`:4444`), `repository_removal_drops_child_suppression_and_standalone_writes_none` (`:4492`), `standalone_without_binding_leaves_bindings_file_byte_identical` (`:4142`) `[BY NAME]`. The planner must write the matrix expectations from this table, not from a uniform "all four gone".

### Core (`baude-core/src/removal.rs`)

P = process, W = worktree or directory, B = binding, M = marker or suppression. "n/a" = no process to assert.

| Kind | Runtime | Fixture | Test (line) | P | W | B | M |
|------|---------|---------|-------------|---|---|---|---|
| Main | none | cloned | `main_checkout_keeps_directory_and_branches` (3439) `[VERIFIED: body read]` | n/a | yes, plus `branch_snapshot` | no | no |
| Main | live | cloned | `live_main_checkout_runtime_is_killed_and_directory_kept` (3615) `[VERIFIED: body read]` | yes: `group_is_extinct`, `assert_reaped_promptly` | yes | no | no |
| Main | none or live | git init | none as the removal target. `git_init_unclean_cwd_held_managed_is_deleted_and_main_kept` (3762) removes the managed row and checks main survives | — | — | — | — |
| Managed | live | git init | `git_init_unclean_cwd_held_managed_is_deleted_and_main_kept` (3762) `[VERIFIED: head read]` | yes | yes | no | no |
| Managed | live | cloned | `managed_worktree_unclean_and_cwd_held_is_deleted` (3835) `[VERIFIED: head read]` | yes | yes | no | no |
| Managed | none | either | only the already-gone case, `already_removed_managed_directory_drops_row` (3498) `[BY NAME]`. No dedicated test deletes a present managed directory with no runtime | — | weak | no | no |
| Standalone | none | plain folder | `standalone_keeps_folder` (3471) `[BY NAME]` | n/a | yes | no | no |
| Standalone | live | plain folder | `live_standalone_runtime_is_killed_and_folder_kept` (3658) `[BY NAME]` | yes | yes | no | no |
| Repository | not read | cloned | `repository_removes_children_then_parent` (3885) `[VERIFIED: head read]` | not read | yes | no | no |
| Repository | none | git init | `save_closure_is_called_exactly_once_for_parent_with_children` (2980) `[BY NAME]` | n/a | not read | no | no |
| Binding, any kind | none | mixed | `clear_stage_removes_repository_binding_and_reports_it` (3992), `checkout_and_standalone_removals_clear_their_own_keys` (4058, cloned at 4064) | never | — | yes | — |
| Marker, any kind | none | `init_repo` (4306) | 4385, 4444, 4492, 4758 | never | — | — | yes |

`[VERIFIED: baude-core/src/removal.rs:3987-4861]` the binding and marker block spawns no runtime and calls `cloned_topology` once, at 4064.

### TUI (`baude/src/app.rs`)

Fixture: `relaunch_fixture` (10703) builds a cloned repository through `admission_repo_cloned` (10704), a second repository with `git init -b main` (10735), and a plain `notes` folder for the standalone row (10731) `[VERIFIED: sed over 10703-10756]`. So one fixture carries both shapes.

| Test (line) | Kinds | Runtime | P | W | B | M |
|-------------|-------|---------|---|---|---|---|
| `d_opens_confirm_remove_modal_with_preview_report_on_each_row_kind` (14363) | all | live (14465) | preview only | preview only | no | no |
| `confirm_remove_managed_worktree_drops_row_in_one_save` (14492) | managed | live (14505) | not confirmed read | yes (14539) | no | no |
| `confirm_remove_repository_parent_drops_every_child_in_one_save` (14598) | parent | live (14629) | `group_is_extinct` helper at 14741 | yes (14723) | no | no |
| `remove_project_failure_kill_keeps_rows_and_handles_then_retry_succeeds` (14789) | managed | live | yes (14905) | yes (14903) | no | no |
| `removed_rows_stay_removed_across_relaunch` (10950) via `remove_four_rows` (10822) | four | not read | not read | not read | not read | re-admission across a second `App` |
| `remove_project_clearing_failure_is_a_footer_notice_not_an_error` (15031) | — | — | — | — | — | suppression file (15045) |

`[VERIFIED: awk over baude/src/app.rs:14258-15200]` no line in the local removal tests reads a bindings file or a marker. No TUI test confirms a main checkout or a standalone removal on its own outside the relaunch helper `[BY NAME]`.

### Daemon (`bauded/src/api.rs`)

Fixtures: `FixtureRepo` at 933 is `git init -b main` (916, 980); `push_to_bare_origin` (2341) adds a bare origin to an init repository (used at 2308, 4816); `cloned_repo` (4700) is a true clone, used at 4721, 4789 and 5904 only `[VERIFIED: grep]`. Process state is read with `pid_is_alive` and `wait_for_pid_gone` (2453, 2475, 2789-2799). All requests go through `router.oneshot` `[VERIFIED: grep; TcpListener appears only at 6746]`.

| Group | Tests (line) | What it pins |
|-------|--------------|--------------|
| `removals_` | 2426, 2493, 2565, 2645, 4030, 4098, 4116 | checkout row dropped in one save with pid gone, metadata, restore, concurrency, preview purity, 404 |
| `standalone_` | 2875, 3139, 3303, 3368, 3414, 3893 | standalone end to end with a live pid; closed row with no runtime (3139); vanished folder |
| `suppression_` | 3670, 3764, 3803, 3864 | suppression entries on disk |
| `matrix_` (14) | 5267, 5415, 5500, 5561, 5599, 5657, 5729, 5769, 5812, 5902, 5992, 6039, 6076, 6115 | status codes: 503 missing handle, 409 drift, 500 runtime survived, 503 persist, 200 saved_degraded, 400 refused admission |
| `events_` (10) | 6288, 6321, 6346, 6394, 6440, 6467, 6495, 6539, 6568, 6640 | `rows_removed` publish, in process |

The 14 `matrix_` tests are a failure and status matrix, not a kind by runtime by fixture matrix. `[VERIFIED: grep for bindings|marker over bauded/src/api.rs]` no daemon test reads a bindings file or a marker file; hits are a parameter named `marker` and one JSON literal at 5619.

### Gap list: the only tests to write

| # | Gap | Surface | Confidence the gap is real |
|---|-----|---------|----------------------------|
| G1 | No test asserts P, W, B and M together for one removal. Binding and marker clearing is never asserted after a live kill | core | HIGH `[VERIFIED]` |
| G2 | Binding and marker assertions run on one fixture shape each; not crossed with cloned versus `git init` | core | HIGH `[VERIFIED]` |
| G3 | Main checkout as the removal target on a `git init` fixture, live and not | core | MEDIUM `[BY NAME]` |
| G4 | Managed worktree, directory present, no runtime, either fixture | core | MEDIUM `[BY NAME]` |
| G5 | `/events` over real HTTP seeing `rows_removed` for a real removal (locked decision) | daemon | HIGH `[VERIFIED]` |
| G6 | Daemon removal of main, managed and parent on a true clone (`cloned_repo`), with binding and marker read from disk | daemon | MEDIUM: three `cloned_repo` users exist and were not read |
| G7 | TUI confirm of a main checkout and of a standalone row as single targets, reading binding and suppression state after | TUI | MEDIUM `[BY NAME]` |

G1 and G2 are closed by one table-driven core test. Standalone has no cloned or `git init` axis because the folder is not a repository, so the core matrix is 3 kinds x 2 runtimes x 2 fixtures plus standalone x 2 runtimes: 14 cells. Before writing G3, G4, G6 and G7, the executor should read the named neighbouring test bodies; a `[BY NAME]` gap may already be closed inside a larger test.

## Fixture Helpers (research question 2)

### `baude-core/src/testing.rs` (feature `test-support`) `[VERIFIED: grep of the file's pub items]`

| Helper | Line | Use |
|--------|------|-----|
| `thread_local!` redirects | 85 | config dir, Claude config dir, home, worktrees base, hook command, workspace |
| `TestRedirect::new(root)` | 110, 131 | hold on the test thread for the whole test |
| `TestRedirect::with_workspace` | 154 | pinned workspace |
| `assert_contained` | 294 | the guard that panics when a path resolves to a real root |
| `git_in(dir, args)` | 314 | run git in a fixture |
| `ClonedTopology` / `cloned_topology(root)` | 337, 356 | a real clone with a default worktree and a managed worktree under the managed root |
| `HolderGuard`, `holder_pgid` | 426, 445 | process doubles |
| `spawn_parked_reader_runtime` | 513 | real PTY with a reader parked on an out-of-group holder |
| `spawn_unkillable_runtime`, `arm_unkillable_runtime` | 546, 532 | real PTY that survives escalation |

### In `removal.rs` tests (module starts at 1170) `[VERIFIED: name listing]`

`RemovalFixture::new(label)` (the `git init` shape, with `managed_worktree`, `repo`, `state_root`), `seed_repository` (1829), `seed_live` (1845), `spawn_trapping_runtime` (1959, a real PTY child), `handle_for` (2018), `worktree_list` (2031), `counting_save` (2035), `group_is_extinct` (1929), `assert_reaped_promptly` (1918), `branch_snapshot` (3326), `seed_cloned` (3353), `notes_folder` (3401), `seed_notes` (3408), `bindings_on_disk` (3987), `marker_bytes` (4286), `init_repo` (4306), `suppressions_on_disk` (4319), `listing` (4324), `cleared_of` (4339), `failures_of` (4348).

Live runtimes in these tests are real PTY processes, not mocks: `spawn_trapping_runtime` returns a `Pty` and tests read `pty.process_identity().pid` and `.process_group` `[VERIFIED: removal.rs:3621-3623]`.

### Thread-local redirect rule

The redirects are thread-local on purpose and `remove_rows` resolves the config dir and the worktrees base while it saves and clears. A `spawn_blocking` probe broke `removals_execute_drops_a_checkout_row_in_one_save`, `removals_execute_survives_a_fresh_restore` and `suppression_parent_removal_drops_the_child_entry` with "config dir resolved to the real user path ... hold a baude_core::testing::TestRedirect on this thread" `[VERIFIED: 21-REVIEW-FIX.md WR-06]`.

Rules for every new test:
- Use `#[tokio::test]` with its default current-thread flavour. Never `flavor = "multi_thread"`, never `spawn_blocking`, never `std::thread::spawn` for code that resolves a baude path.
- Declare the redirect first so it drops last. `pty_websocket_round_trip` states why: "the spawned `axum::serve` task and the PTY it drives run on this same current-thread runtime, so the thread-local redirect must outlive the socket" `[VERIFIED: bauded/src/api.rs:6732-6734]`.
- The real-HTTP `/events` client must also run on the test's runtime (a `tokio::net::TcpStream`), not on a helper thread.

### Where new tests live

In-crate, per the locked decision: core matrix in `baude-core/src/removal.rs` `mod tests`; `/events` over HTTP next to `pty_websocket_round_trip` in `bauded/src/api.rs`; TUI cells beside `remove_project_app` (14258) in `baude/src/app.rs`.

## Standard Stack

No new dependency. Everything needed is already a dev-dependency.

| Library | Where | Purpose |
|---------|-------|---------|
| `tokio` `TcpListener`, `axum::serve` | `bauded/src/api.rs:6746-6748` `[VERIFIED]` | real socket for the `/events` test |
| `futures-util`, `http-body-util`, `tower` (util), `tokio-tungstenite` | `bauded/Cargo.toml` `[dev-dependencies]` `[VERIFIED]` | body and stream handling |
| `baude-core` feature `test-support` | `bauded/Cargo.toml` `[VERIFIED]` | `testing.rs` helpers |

No HTTP client crate is a dev-dependency of `bauded`. Write the SSE request by hand over `tokio::net::TcpStream` (`GET /events HTTP/1.1`, `Host`, `Accept: text/event-stream`) and read until a `rows_removed` frame. `[ASSUMED]` that hand-written HTTP/1.1 is enough for axum's SSE response; the executor should expect chunked transfer encoding and match on the `event: rows_removed` line, not on exact framing.

## Package Legitimacy Audit

Not applicable. This phase installs no package.

## Architecture Patterns

### Data flow under test

```
D key / PWA confirm / POST /removals
        |
        v
surface takes runtime handles out of its registry (app.rs, manager.rs)
        |
        v
removal::remove  --select_rows--> preflight --> validate candidate
        |                                              |
        |                                   refusal: nothing touched
        v
   kill stage (Session::kill_and_wait, abort on first survivor)
        v
   disk stage (forced removal of managed worktrees only)
        v
   one save of the candidate state
        v
   clearing stage (binding, marker, repository dir, suppression)
        v
RemovalReport --> TUI footer / 200 body / SSE rows_removed on GET /events
```

Stage order is from the `remove` doc comment `[VERIFIED: baude-core/src/removal.rs:400-415]`; the clearing stage as fourth is from ROADMAP plan 19-01.

### Pattern 1: one end-state reader, one table

Write a single reader that takes the fixture and returns the four observations, then loop the cells. Compose it from helpers that exist: `group_is_extinct` and `assert_reaped_promptly` for process, `worktree_list` and `Path::exists` for the worktree, `bindings_on_disk` for the binding, `marker_bytes`, `listing` and `suppressions_on_disk` for the marker column. Every observation comes from the fixture's disk or the process table, never from the `RemovalReport` alone, since the report is the code's own claim.

### Pattern 2: real-HTTP `/events`

Copy the harness of `pty_websocket_round_trip`: redirect first, `TcpListener::bind("127.0.0.1:0")`, `tokio::spawn(axum::serve(listener, app))`. Connect a `TcpStream`, send the GET, then perform the removal through `POST /removals` on the same address and assert a `rows_removed` frame arrives and that `GET /rows` no longer lists the row. Reuse `removal_fixture` (2265) for the seeded live checkout.

### Anti-patterns to avoid
- **Asserting on the report only.** REL-01 says fixture output.
- **A new multi-threaded runtime or a blocking hop.** It loses the redirect; see above.
- **Retries around a teardown 500.** That hides BL-11, which is deliberately left visible.
- **Uniform "all four gone" expectations.** Main and standalone directories must still exist; a child removal must leave the parent marker.
- **Docs that claim a browser run.** CONTEXT forbids it; none exists.

## Don't Hand-Roll

| Problem | Don't build | Use instead |
|---------|-------------|-------------|
| Cloned repository fixture | another `git clone` helper | `baude_core::testing::cloned_topology` (356), or `cloned_repo` in `api.rs` (4700) |
| Live runtime | a mock session | `spawn_trapping_runtime` (removal.rs 1959), `spawn_parked_reader_runtime` (testing.rs 513) |
| Process-gone check | `ps` parsing | `group_is_extinct`, `assert_reaped_promptly`, `wait_for_pid_gone` |
| Version bump and changelog | hand edits | release-please |
| Commit message check | eyeballing | `scripts/check-commit-message.sh --range origin/main..HEAD` |

## Docs Facts (research question 3)

### What the README says now `[VERIFIED: grep over README.md]`

- Keys table: `x` at line 149, `X` at line 150. No `D` row.
- `## Worktrees` (355): lines 384-389 describe `x` and `X`.
- `## Remote sessions in the TUI` (763): lines 769-770 say `X` is never available remotely. `D` now works on remote rows, so this paragraph needs a sentence.
- `## bauded (experimental)` (773): endpoint table has `DELETE /sessions/{id}` "kill and remove" at 802. No `/removals`, `/removals/preview`, `/rows` or `/events`.
- Line 23 says managed-worktree removal stays unavailable for standalone sessions. Check the sentence still reads true beside the new section.
- No separate PWA docs file was looked for; CONTEXT places PWA docs in the README's bauded section.

### What removal deletes and keeps

| Row kind | Directory | Source |
|----------|-----------|--------|
| Managed worktree (`managed_by_baude` true, role not `Main`) | deleted, forced, even when unclean or held as a cwd | `removal.rs:1088-1093` `[VERIFIED]`; tests 3762, 3835 |
| Main checkout | kept | `removal.rs:1090` `[VERIFIED]`; asserted at 3456-3457 |
| Unmanaged checkout (hand-made linked worktree) | kept | `removal.rs:1090`, the `(_, false)` arm `[VERIFIED]` |
| Standalone folder | kept | `removal.rs:345-348`, `DiskAction::KeepDirectory` `[VERIFIED]` |
| Repository parent | no directory of its own; children follow their own rules | `removal.rs:319-322`, `path: None` `[VERIFIED]` |
| Local branches | never deleted | `assert_eq!(branch_snapshot(&t.clone), before)` at `removal.rs:3467` and `:3654` `[VERIFIED]` |
| Managed directory already gone | row dropped, git registration pruned | `DiskAction::AlreadyRemoved` at `removal.rs:76`, `:1102-1103` `[VERIFIED]` |

Also cleared: the folder-workspace binding, baude's ownership marker and repository directory, and suppression entries (`ClearingKind`, `removal.rs:152-159`). A clearing failure is reported, not raised: test names at 4206 and 4567 `[BY NAME]`.

The runtime is killed first; a survivor leaves the row in place: `RemovalError::RuntimeSurvived` at `removal.rs:242` `[VERIFIED]`.

`[ASSUMED]` a standalone folder's daemon-written `.claude` directory is left behind. This comes from CONTEXT's deferred list and 21-VERIFICATION.md, not from code read this session. The docs can say the folder and everything in it are left untouched, which is true either way.

The exact git command used for forced removal was not read. Do not quote one in the README.

### The confirmation

TUI `[VERIFIED: baude/src/ui.rs]`:
- Key `D` (`app.rs:338`). Help line: "D removes row, kills runtime, deletes managed worktrees" (`ui.rs:2425`).
- Sidebar hints carry `D remove*` on every kind, and `D rm` in the narrow form (`ui.rs:1471-1521`). The meaning of the asterisk was not read; check before documenting it.
- Modal title `Remove this {kind_label}?` (`ui.rs:2753`), a `runtimes killed` heading with a count (`ui.rs:2769`), footer `y/enter remove · n/esc cancel` (`ui.rs:2849`).
- The older `X` modal is separate: "Remove this clean baude-managed linked worktree?" (`ui.rs:2502`).

PWA `[VERIFIED: bauded/web/removal.js]`:
- Title `Remove this ${targetKind(source.target)}?` (223).
- Sections `rows dropped` (192), `runtimes killed` (197), `paths deleted` (207), `paths kept` (208), `already removed` (212), `cleared` (218).
- Result line `removed N rows, killed N runtimes, deleted N paths` (311-312).
- After a failed preview or execute the button reads retry and stays pressable (21-REVIEW-FIX WR-01).

Daemon `[VERIFIED: bauded/src/api.rs router]`: `POST /removals/preview`, `POST /removals`, `GET /rows`, `GET /events`. Statuses named by the `matrix_` tests: 200, 200 with `saved_degraded`, 400, 404, 409 with a fresh preview report, 500 runtime survived, 503. The event name is `rows_removed` (`api.rs:6250`).

Docs must not say the PWA or the live SSE drop was exercised in a browser.

## Release Mechanics (research question 4)

| Fact | Value | Source |
|------|-------|--------|
| Manifest version | `2.7.3` | `.release-please-manifest.json` `[VERIFIED]` |
| Release type | `simple`, one package `.`, `bump-minor-pre-major: false`, no `exclude-paths` | `release-please-config.json` `[VERIFIED]` |
| Version files | `Cargo.toml` (generic, line 15 annotation), `baude-core/Cargo.toml`, `baude/Cargo.toml`, `bauded/Cargo.toml` (`$.package.version`) | `release-please-config.json` `[VERIFIED]` |
| Release-relevant paths | all of them; one root package with no path filter | same |
| Action | `googleapis/release-please-action@v5`, on push to `main`, authenticated as a GitHub App | `.github/workflows/release-please.yml` `[VERIFIED]` |
| Commits ahead of `origin/main` | 223 (CONTEXT says 214) | `git rev-list --count origin/main..HEAD` `[VERIFIED]` |
| Commit types on the branch | 120 docs, 47 test, 34 feat, 15 fix, 4 refactor, 2 chore, 1 ci | `git log` tally `[VERIFIED]` |
| Breaking markers | none: no `!:` header, no `BREAKING CHANGE`, no `Release-As` | grep exit 1 `[VERIFIED]` |
| Nested parentheses, per commit | none: `scripts/check-commit-message.sh --range origin/main..HEAD` exit 0 | run this session `[VERIFIED]` |
| Nested parentheses, simulated squash body | none: all 223 messages concatenated oldest first under a `feat:` header, 1120 lines, exit 0 | run this session `[VERIFIED]` |
| Merge settings | squash, merge and rebase all allowed; `squash_merge_commit_title: COMMIT_OR_PR_TITLE`; `squash_merge_commit_message: COMMIT_MESSAGES`; `allow_auto_merge: true` | `gh api repos/poindexter12/baude` `[VERIFIED]` |
| Rulesets | none (`[]`) | `gh api .../rulesets` `[VERIFIED]` |
| Branch protection on `main` | required checks `check (macos-14)`, `check (ubuntu-22.04)`, `docker`; `strict: false`; `enforce_admins: true`; force pushes and deletions off. The response carried no required-reviews key | `gh api .../branches/main/protection` `[VERIFIED]` |
| Branch naming | no rule enforces a prefix; remote branches use `fix/`, `docs/`; the current `gsd/phase-18-...` name is allowed | rulesets empty, `git branch -r` `[VERIFIED]` |

### What drives the bump

With more than one commit and `COMMIT_OR_PR_TITLE`, the squash commit's header is the PR title with ` (#N)` appended, and with `COMMIT_MESSAGES` its body is the list of the branch's commit messages `[ASSUMED: GitHub's documented behaviour for these two settings, not observed on this repo this session]`. release-please reads that one commit on `main`, so **the PR title drives the bump**: a `feat:` title yields 2.7.3 to 2.8.0. History agrees: `87a0fb3 fix: ... (#122)` produced 2.7.2 `[VERIFIED: git log origin/main]`.

Three consequences:
1. The PR title is very likely the only changelog line for v2.8.0 `[ASSUMED]`. Write it as the user-facing summary of the milestone.
2. The squash body is 63 KB of 223 messages. It passes the guard today. CI checks each commit and the PR title, not the assembled body (`ci.yml`, step "check pull request commits and title") `[VERIFIED]`. Rerun the simulation after the last Phase 22 commit: `git log --reverse --no-merges --format='* %B' origin/main..HEAD`, prefix the PR title, pipe to `scripts/check-commit-message.sh -`. The owner can also trim the body in the merge dialog.
3. If the owner merges with a merge commit or a rebase instead, release-please reads all 223 commits. 34 are `feat`, none is breaking, so the result is still 2.8.0, with a long changelog `[ASSUMED: release-please's standard feat-to-minor rule]`.

The PR title must itself pass the guard: one pair of parentheses at most, and GitHub adds ` (#N)`, so use no parentheses and no scope. Example: `feat: remove projects from the TUI, the daemon and the PWA`.

### The soak

On the push to `main`, release-please opens the release PR. The "gate release pr" step compares the PR title's version with the manifest on `main`, labels it `release:minor`, and runs `gh pr merge --disable-auto` `[VERIFIED: release-please.yml]`. `release-automerge.yml` runs every 30 minutes; it merges a `release:minor` PR with `--merge` once it is at least `SOAK_HOURS: "2"` old and `gh pr checks --required` is green; a `release:hold` label vetoes it; a maintainer can merge by hand `[VERIFIED: release-automerge.yml]`. Merging the release PR creates the tag and the GitHub Release, which fires `release.yml` to build the images and assets `[VERIFIED: release.yml:1-12]`.

The release-please run also pushes a `chore: finalize release metadata` commit that refreshes `Cargo.lock` on the release branch `[VERIFIED: release-please.yml]`. Nothing in Phase 22 should touch `Cargo.lock` versions by hand.

## Common Pitfalls

### Pitfall 1: BL-11 turns a required check red
**What goes wrong:** the macOS `check` job runs the whole suite single-threaded and BL-11 was seen in 1 of 3 and 1 of 5 full runs `[VERIFIED: .planning/BACKLOG.md BL-11]`. A red required check blocks the owner's merge and makes the soak cron skip the release PR.
**How to avoid:** do not fix it and do not wrap it in retries. Keep new live-runtime daemon tests to the minimum the matrix needs, put the full matrix in core where the kill goes through `Session::kill_and_wait` directly, and record in the phase summary that a rerun of the job is the remedy.
**Warning signs:** a 500 with "failed to terminate exact PTY process group".

### Pitfall 2: the Linux gate's target directory
**What goes wrong:** STATE.md says `scripts/gate.sh --linux` is unrunnable verbatim on this host because `CARGO_TARGET_DIR` sits on the Docker Desktop bind mount. `scripts/gate.sh:46` still sets `CARGO_TARGET_DIR=/work/target/linux-gate/target` `[VERIFIED]`. Yet 21-REVIEW-FIX.md records `scripts/gate.sh --linux` exit 0 at `e2170d0`.
**Reading:** the race is intermittent or the STATE entry is stale; this session did not run the gate, so which one is unknown.
**How to avoid:** run it verbatim. If it fails with a file-visibility error rather than a test failure, that is a gate defect, which CONTEXT allows fixing here. Plan it as a conditional task, not a certain one.

### Pitfall 3: gates run on a commit that is not the last
Phase 22 adds tests and README edits after `e2170d0`. Both gates must run on the final commit, after every docs and planning commit, and the squash-body simulation with them.

### Pitfall 4: uncommitted planning files
`git status` shows ` M .planning/STATE.md` and an untracked `.planning/phases/21-daemon-and-pwa-parity/.review-diagnostics/` `[VERIFIED: session git status]`. Decide before the push whether the diagnostics directory is committed or ignored.

### Pitfall 5: first CI run of the node step
`ci.yml` sets up node 26 and runs `scripts/check-web-suites.sh`; this has never run on a runner `[VERIFIED: 21-VERIFICATION.md]`. `gate.sh --linux` copies node from `node:26-bookworm`, so the majors match `[VERIFIED: gate.sh:39, ci.yml]`. Watch the first PR run and treat a failure there as a gate-exposed defect.

### Pitfall 6: executor claims
STATE.md records repeated cases of executors reporting gates green that were not and tests that asserted nothing. For this phase the plan's verification steps should name the exit code to capture and require reading each new test body.

## Code Examples

The harness to copy for G5, quoted from the file `[VERIFIED: bauded/src/api.rs:6746-6748]`:

```rust
let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
// ...
tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
```

The process and keep assertions to reuse for G1, quoted from `[VERIFIED: baude-core/src/removal.rs:3644-3654]`:

```rust
assert!(group_is_extinct(pgid), "group {pgid} must be extinct");
assert_reaped_promptly(pid);
assert!(t.clone.exists(), "the main checkout must stay on disk");
assert_eq!(report.paths_kept, vec![t.clone.clone()]);
assert!(report.paths_deleted.is_empty());
assert_eq!(branch_snapshot(&t.clone), before);
```

## State of the Art

Nothing external changed. The repo moved to `release-please-action@v5` and `actions/checkout@v7` in `cf64ade` (#126), the last commit on `origin/main` `[VERIFIED: git log]`.

## STATE.md Blockers (research question 5)

| Entry | Verdict | Basis |
|-------|---------|-------|
| Phase 18: `gate.sh --linux` unrunnable verbatim; "blocks `/gsd-ship` until landed" | Live risk, possibly stale. See Pitfall 2 | `gate.sh:46` unchanged; 21-REVIEW-FIX reports a verbatim pass |
| Phase 18: `18-VALIDATION.md` draft, no `18-SECURITY.md` | Open process debt. `security_enforcement` is on in `.planning/config.json`, so a ship workflow may stop on it | `[VERIFIED: config.json]`; the files themselves were not opened |
| Phase 18: reviewer lanes stubbed | Not a ship blocker | tooling note |
| Phase 13 security and validation; Phase 06, 07, 08 entries | Stale for this release: those milestones shipped through v2.7.3 | `b86365f chore(main): release 2.7.3` on `origin/main` `[VERIFIED]`; staleness itself `[ASSUMED]` |
| Not in STATE: ROADMAP shows Phases 19 and 20 unchecked (lines 73-74); REQUIREMENTS shows TUI-01 to TUI-04 Pending (lines 105-108) | Bookkeeping to settle in this phase, tied to the locked re-verification of 18, 19 and 20 | `[VERIFIED]` |
| Not in STATE: Phase 19 has no `19-VERIFICATION.md`; Phase 20 has one | The re-verification of 19 is a first verification | `ls` of both phase directories `[VERIFIED]` |
| Not in STATE: Phase 21 passed on sign-off with 2 of 3 truths behaviour-unverified | Known and accepted in CONTEXT; G5 is the automated stand-in for the daemon half only | 21-VERIFICATION.md `[VERIFIED]` |
| 21-REVIEW-FIX WR-06 skipped; WR-03 and WR-04 "requires human verification" | Discretion item. WR-06 is a locking redesign and does not fit "no new behaviour" | 21-REVIEW-FIX.md `[VERIFIED]` |

Nothing found blocks opening the PR. Two things can block the release after it: a red required check (BL-11 or the unproven node step) and a human-needed result from re-verifying Phases 18 to 20.

## Assumptions Log

| # | Claim | Section | Risk if wrong |
|---|-------|---------|---------------|
| A1 | The squash commit takes its header from the PR title and its body from the branch's commit messages | Release Mechanics | A different header could change the bump; the body shape changes what the guard must pass |
| A2 | release-please records only the squash header as a changelog entry, not the commits listed in the body | Release Mechanics | The changelog would carry many more lines; the version is unaffected |
| A3 | A `feat` commit on a 2.x package with `bump-minor-pre-major: false` yields a minor bump | Release Mechanics | Wrong version; CONTEXT allows a `Release-As` footer only in that case |
| A4 | A hand-written HTTP/1.1 GET over `TcpStream` can read axum's SSE stream | Standard Stack | G5 needs a different client; `tokio-tungstenite` does not help for SSE |
| A5 | Cells marked `[BY NAME]` assert what their names say and nothing more | Coverage Matrix | A gap in G3, G4, G6 or G7 may already be closed, or a covered cell may be weaker than listed |
| A6 | Standalone removal leaves a daemon-written `.claude` directory | Docs Facts | Docs wording; the suggested phrasing is safe either way |
| A7 | The older-milestone STATE blockers are stale | STATE.md Blockers | A forgotten gate could stop a ship workflow |
| A8 | The asterisk in `D remove*` marks a confirmation step | Docs Facts | README would misdescribe the hint; check `ui.rs` near 1471 first |

## Open Questions

1. **Does the "four row kinds" matrix count unmanaged checkouts?**
   - Known: CONTEXT says four; TUI-01 names five kinds; core covers unmanaged at 3560 and 3708.
   - Recommendation: keep four as locked and cite the two existing unmanaged tests in the validation record.

2. **Is the Linux gate race still live?**
   - Known: the script is unchanged and one recent verbatim run passed.
   - Recommendation: a conditional plan task, with the `/tmp` target directory as the known remedy.

3. **Does a ship workflow stop on missing `18-SECURITY.md`?**
   - Known: `security_enforcement: true`, `security_block_on: "high"`.
   - Recommendation: the plan should push and open the PR with plain `git` and `gh`, as CONTEXT describes, and report the missing files rather than route through a workflow gate that may refuse.

4. **How much of the daemon matrix is worth the BL-11 exposure?**
   - Recommendation: G5 plus one cloned-fixture parent removal; leave the full kind by runtime product to core.

## Environment Availability

| Dependency | Required by | Available | Notes |
|------------|-------------|-----------|-------|
| `git`, `gh` (authenticated for `poindexter12/baude`) | push, PR | yes | `gh api` calls succeeded this session |
| `cargo`, `rustfmt`, `clippy` | `scripts/gate.sh` | not probed | the gate ran green on this host per 21-REVIEW-FIX |
| `node` | `scripts/check-web-suites.sh` | not probed | same |
| Docker | `scripts/gate.sh --linux` | not probed | same; see Pitfall 2 |
| GitHub App secrets for release-please | the release itself | not observable from here | v2.7.3 was released through it |

## Validation Architecture

### Test framework

| Property | Value |
|----------|-------|
| Framework | `cargo test` with in-crate `mod tests`; `node --test` for `bauded/web/tests` |
| Config file | none |
| Quick run | `cargo test -p baude-core --lib removal::tests -- --test-threads=1` |
| Full suite | `scripts/gate.sh`, then `scripts/gate.sh --linux` |

Read each exit code directly. Never pipe a gate command into `tail` or `grep` and read `$?`.

### Phase requirements to test map

| Req | Behaviour | Type | Command | Exists? |
|-----|-----------|------|---------|---------|
| REL-01 | 14-cell core matrix reading P, W, B, M from the fixture | integration, in-crate | `cargo test -p baude-core --lib removal::tests::<matrix name> -- --test-threads=1` | No: Wave 0, G1 to G4 |
| REL-01 | `rows_removed` seen on `/events` over a real socket for a real removal | integration | `cargo test -p bauded --bin bauded -- <events http name> --test-threads=1` | No: Wave 0, G5 |
| REL-01 | daemon removal on a true clone reading binding and marker state | integration | `cargo test -p bauded --bin bauded -- <name> --test-threads=1` | No: G6 |
| REL-01 | TUI single-target main and standalone confirm | integration | `cargo test -p baude -- <name> --test-threads=1` | No: G7 |
| REL-01 | existing suites still green | regression | `cargo test -p bauded --bin bauded -- matrix_ events_ flat_session_api` (27 passed at verification) | Yes |
| REL-02 | README names `D`, the confirmation, deletes and keeps, the endpoints and the event | manual review against Docs Facts | read the diff; every claim traced to a file:line above | manual |
| REL-02 | commit messages and PR title parse | automated | `bash scripts/check-commit-message.sh --range origin/main..HEAD`; title through `... -` | Yes |
| REL-02 | squash body parses | automated, ad hoc | the simulation in Release Mechanics | command only |
| REL-02 | both gates pass on the final commit | automated | `scripts/gate.sh`; `scripts/gate.sh --linux` | Yes |
| REL-02 | v2.8.0 published | manual, after the owner's merge | `gh release view v2.8.0` | open item by decision |

### Automated versus manual

Automated: every REL-01 cell, both gates, the commit and title guards, the PWA node suites. Manual: the README accuracy review, the owner's merge, the release itself, and everything 21-VERIFICATION.md lists under human verification (browser PWA run, live SSE drop, 15 s keep-alive). The `/events` HTTP test does not replace a browser run.

### Sampling rate
- **Per task commit:** the quick run for the crate touched, plus `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`.
- **Per wave merge:** `scripts/gate.sh`.
- **Phase gate:** both gates on the final commit, then the squash-body simulation, then the push.

### Wave 0 gaps
- [ ] Core end-state reader and the 14-cell matrix in `baude-core/src/removal.rs` (G1 to G4)
- [ ] Real-HTTP `/events` test in `bauded/src/api.rs` (G5)
- [ ] Read the `[BY NAME]` neighbours before writing G3, G4, G6, G7

TDD mode is on in config. These are tests of existing behaviour, so a new test is expected to pass at once; a red result is a defect the tests exposed, which CONTEXT puts in scope. The plan should say this so a missing RED commit is not read as a process failure.

## Security Domain

No new attack surface: the phase adds tests and prose. Phase 21 owns the threat model for the removal endpoints.

| ASVS category | Applies | Note |
|---------------|---------|------|
| V2 Authentication | no | no change |
| V3 Session Management | no | no change |
| V4 Access Control | no | no change; the daemon's exposure model is unchanged |
| V5 Input Validation | no new input | existing: `matrix_drift_tampered_paths_deleted_is_409_and_touches_nothing` (5561), `matrix_drift_uncompared_fields_cannot_steer_the_daemon` (5599) |
| V6 Cryptography | no | none |

| Pattern | STRIDE | Mitigation in this phase |
|---------|--------|--------------------------|
| A test deleting or writing under a real user root | Tampering | hold a `TestRedirect`; `assert_contained` (testing.rs 294) and the gate's real-roots bracket catch a miss |
| Docs overstating safety, for example "never deletes anything" | Repudiation | write deletes and keeps from the table in Docs Facts only |
| A release cut from unverified phases | — | the locked re-verification of 18, 19, 20 before the push |

## Project Constraints (from CLAUDE.md)

- Run `scripts/gate.sh` before every push; it is the CI `check` job in order with unpiped exit codes.
- Run `scripts/gate.sh --linux` for anything touching platform code (`cfg`, procfs, file timing).
- Never judge a gate step by piping it to `tail` or `grep` and reading `$?`.
- release-please owns versions, tags and the GitHub Release. `main` is protected; every change lands by PR.
- Patch bumps auto-merge on green, minor bumps after a soak, majors by hand.
- Never nest parentheses in a commit body, squash message or PR title.
- Fixtures must be shaped like what real users produce: cloned versus `git init` repositories, stale runtime rows, command doubles that accept every appended argument.
- Assert on fixture output, never by grepping source.
- From the user's global instructions: git stays on SSH; check ruleset and branch-prefix conventions before pushing (done: none enforced); `git fetch origin` before relying on `origin/main`; pass multi-line `gh pr create --body` as a direct quoted string, not a heredoc.

## Sources

### Primary (read or run this session)
- `baude-core/src/removal.rs` (public types 49-255, `select_rows` 306-350, `remove` doc 400-421, classification 1073-1106, test bodies at 3353, 3439, 3615 and heads at 3762, 3835, 3885)
- `baude-core/src/testing.rs` (pub item listing), `baude-core/src/repository.rs:284-288`
- `bauded/src/api.rs` (router, test name listing, fixtures at 2265-2352, 6700-6800), `bauded/Cargo.toml`
- `baude/src/app.rs` (test name listing, fixtures at 9315, 10703, 14258), `baude/src/ui.rs` (strings), `bauded/web/removal.js` (strings)
- `README.md` (headings and removal greps)
- `scripts/gate.sh`, `scripts/check-commit-message.sh:1-60`, `scripts/check-web-suites.sh`, `.githooks/commit-msg`
- `release-please-config.json`, `.release-please-manifest.json`, `.github/workflows/ci.yml`, `release-please.yml`, `release-automerge.yml`, `release.yml:1-40`
- `gh api` for the repository, its rulesets and `main` protection (read-only)
- `.planning`: 22-CONTEXT.md, REQUIREMENTS.md, ROADMAP.md 66-207, STATE.md 324-415, BACKLOG.md, 21-VERIFICATION.md, 21-REVIEW-FIX.md, config.json

### Not consulted
- No web or documentation lookup was made; release-please and GitHub squash behaviour are `[ASSUMED]` (A1 to A3).
- The knowledge graph and the research-plan cache seam were not queried.
- `bauded/src/manager.rs`, `baude/src/remote.rs`, `removal-controller.js`, 21-REVIEW.md and the Phase 18 to 20 validation files were not opened.

## Metadata

**Confidence breakdown:**
- Release mechanics: HIGH for repo config and commit checks (read and run); MEDIUM for the squash and changelog behaviour (A1 to A3).
- Coverage matrix: HIGH for G1, G2, G5; MEDIUM for G3, G4, G6, G7.
- Docs facts: HIGH for the delete and keep rules and the strings quoted; LOW for the asterisk and the `.claude` remark.
- Pitfalls: MEDIUM; the Linux gate was not run.

**Research date:** 2026-11-13
**Valid until:** the next commit that touches `removal.rs`, `api.rs` or `app.rs` tests moves the line numbers; the release facts hold until `main` advances.
