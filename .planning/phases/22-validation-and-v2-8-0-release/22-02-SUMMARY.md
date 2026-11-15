---
phase: 22-validation-and-v2-8-0-release
plan: 02
subsystem: daemon removal and events
tags: [removal, events, sse, real-socket, cloned-fixture, tests-only, REL-01]
status: complete
requires:
  - the PTY process group fix of plan 22-06 and the fixes of plans 22-07 and 22-08
provides:
  - one daemon test that sees `rows_removed` on `GET /events` over a real loopback socket for a real removal
  - one daemon test that removes a repository parent on a true `git clone` and reads the end state from disk, the process table and git
  - hand-written HTTP/1.1 socket helpers in `mod tests` of `bauded/src/api.rs`, with chunked decoding and every read under a timeout
affects:
  - plan 22-05, which runs the full gates on the final tree
  - whoever picks up the open item below on daemon repository directories without an ownership marker
tech-stack:
  added: []
  patterns:
    - the router is moved into `axum::serve`, so no request in the socket test can reach it through `oneshot`
    - socket I/O through `readable` and `try_read`, `writable` and `try_write`, because `bauded` does not enable tokio `io-util`
    - an absence is pinned by walking the whole fixture root, and the walker is proven by planting the file once
key-files:
  created: []
  modified:
    - bauded/src/api.rs
    - .planning/phases/22-validation-and-v2-8-0-release/22-02-PLAN.md
decisions:
  - No product code changed; both tests pass on the tree as found
  - The daemon writes no ownership marker, so the cloned-fixture test pins its absence instead of its removal; orchestrator decision, option 1, on 2026-11-14
  - The cloned-fixture test was renamed to end in `repository_dir_on_disk`, since a name saying it clears a marker would be false
  - The report's `cleared` list is held to exactly `binding` and `repository_dir`, the one observation taken from the removal report
  - REL-01 is not marked complete here; this plan delivers its daemon half only
metrics:
  duration: 17m
  completed: 2026-11-14
  tasks: 2
  files: 2
actuals:
  tokens: 9434
  tasks: 2
  commits: 3
plan_head_before: 13923359c602acbf7a4b2aef371c444b7f3691dc
plan_head_after: 5887fea0e98adacf59483afacb5bbf070acc7c83
---

# Phase 22 Plan 02: Daemon events over a real socket and a cloned-repository parent removal Summary

Two live-runtime tests in `mod tests` of `bauded/src/api.rs`, the cap the plan set. One serves the router on `127.0.0.1:0` and watches a hand-written `GET /events` subscriber receive `rows_removed` for a real removal. The other removes a repository parent on a true `git clone` with both children live and reads what is left from the fixture. Tests only: 695 lines added to `api.rs`, none removed, every hunk at or after line 802, `Cargo.lock` and `bauded/Cargo.toml` unchanged.

## Commits

| Task | Commit | Subject |
| ---- | ------ | ------- |
| 1 | `3e5ba36` | test(22-02): see rows_removed on GET /events over a real socket |
| 2 | `4e272e7` | test(22-02): remove a repository parent on a true clone and read the end state from disk |
| plan | `5887fea` | docs(22-02): amend the plan because the daemon writes no ownership marker |

Base was `1392335`, as expected.

## What the socket test witnessed

`rel01_events_rows_removed_reach_a_real_http_subscriber` closes gap G5.

- The subscriber's `GET /events` came back 200, `text/event-stream`, `Transfer-Encoding: chunked`.
- Before, all over the socket: `GET /rows` listed the row, `POST /removals/preview` returned 200, the agent pid was alive.
- `POST /removals` with the preview echoed returned 200, `persistence` saved.
- The subscriber's socket then delivered one message, `event: rows_removed` with a single `data:` line whose JSON equalled the removal response body. No second message arrived in the next 300 ms.
- After: `GET /rows` no longer listed the row, the pid was gone, the state file bytes had changed, and a fresh `Manager` restoring that file relaunched 0 sessions and answered `UnknownTarget` for the key.
- Bite check, with the removal call taken out: failed in 6.01 s without hanging on `rel01 events over a socket: no rows_removed message within 5 s of a committed removal; the socket delivered ""`.

This is not a browser run and does not stand in for one.

## What the cloned-fixture test witnessed

`rel01_cloned_parent_removal_clears_binding_and_repository_dir_on_disk` closes gap G6. Triage found no existing daemon test on a `cloned_repo` fixture that reads a bindings file or a marker.

The fixture is a real clone: `origin` resolves to the upstream and `refs/remotes/origin/HEAD` points at `origin/main`. Two sessions are admitted, the main checkout and a managed worktree on `topic`, and a neighbour folder is bound to the same workspace.

| Observation | Before | After |
| ----------- | ------ | ----- |
| both agent pids | alive | gone |
| managed worktree directory | present | gone |
| clone directory, git dir, tracked file | present | kept |
| branch `topic` in the clone | listed | kept |
| clone entry in `folder-workspaces.json` | present | gone |
| neighbour entry in the same file | present | unchanged |
| repository directory `repository-1` | present | gone |
| `.baude-marker.json` anywhere under the fixture root | none | none |
| repository parents in `GET /rows` | one | none |

The report's `cleared` list holds exactly a `binding` and a `repository_dir` entry.

Bite checks: expecting the clone directory gone failed on `the clone directory .../clone must be kept`. Planting a marker in the repository directory failed on `the daemon wrote an ownership marker: [".../repository-1/.baude-marker.json"]`, which proves the absence walk can see one.

## Verification

All exit codes read directly; output is under `target/gsd-22/`.

| Check | Exit | Result |
| ----- | ---- | ------ |
| both `rel01_` tests, 10 runs in a row, one thread | 0 each | 10 of 10, two tests ok per run |
| `cargo fmt --all -- --check` | 0 | |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | |
| `cargo test -p bauded` | 0 | 179 passed, 0 failed, 0 ignored |
| `rel01_` and `events_` together, one thread | 0 | 12 passed |
| `rel01_` tests listed | 0 | 2 |
| diff hunks in `api.rs` all at or after line 802 | 0 | |
| `Cargo.lock` and `bauded/Cargo.toml` unchanged since base | 0 | |
| `scripts/check-commit-message.sh --range 1392335..HEAD` | 0 | |

The text "failed to terminate exact PTY process group" appeared in no run. No `sleep 30` agent processes and no `bauded-removal-rel01-*` fixture directories are left behind. `scripts/gate.sh` was not run, by instruction.

## Deviations from Plan

### Plan amended after a checkpoint

**1. The daemon writes no ownership marker**
- **Found during:** Task 2, first run of the test as planned
- **Expected by the plan:** an ancestor of the managed worktree holds `.baude-marker.json` before the removal, and the marker is gone after
- **Observed:** no `.baude-marker.json` anywhere under the fixture root. The failing line was `no ancestor of .../repository-1/topic-2 under <root> holds .baude-marker.json`
- **Cause:** `bauded/src/manager.rs` builds its repository rows itself. The only non-test marker writer is `lifecycle::ensure_repository` in core, and nothing in `bauded/src` calls it. `marker::clear_owned` answers `MarkerOutcome::Missing` for a missing marker, so the removal is handled by design
- **Handling:** stopped and reported under the defect rule, with nothing committed for the task. The orchestrator chose to accept the behaviour. The test now pins the marker absent before and after, and the plan's truth 3 and Task 2 text were amended in place with a dated note
- **Commits:** `4e272e7`, `5887fea`

### Smaller differences

- **Test name:** `rel01_cloned_parent_removal_clears_binding_and_repository_dir_on_disk`, not the plan's `..._and_marker_on_disk`. The plan line was updated to match
- **Socket I/O:** readiness plus `try_read` and `try_write`, because `bauded` does not enable tokio `io-util` and the manifests are frozen
- **Extra assertions in Task 1:** silence for 300 ms after the message, the state file byte change, and the row's path and session id
- **Extra assertions in Task 2:** the neighbour binding unchanged, the clone's remote and remote HEAD, and the report's `cleared` kinds held to an exact list
- **Observation line:** Task 1 prints the delivered frame under `--nocapture`, as the `21-03 observation` lines do

## Open Items

- **Owned by no plan:** daemon repository directories carry no ownership marker, so an admission after state loss cannot rediscover them the way the TUI path does. Nothing in Phase 22 addresses this.
- REL-01 stays open until the remaining plans of this phase land.

## Known Stubs

None.

## Self-Check: PASSED

- `bauded/src/api.rs` and `22-02-PLAN.md` exist with the changes above
- `3e5ba36`, `4e272e7` and `5887fea` are ancestors of HEAD
- `git rev-list --count 1392335..HEAD` read 3 when this file was written
