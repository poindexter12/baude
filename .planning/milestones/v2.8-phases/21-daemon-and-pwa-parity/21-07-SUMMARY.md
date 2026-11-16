---
phase: 21-daemon-and-pwa-parity
plan: 07
subsystem: ui
tags: [pwa, javascript, node-test, removal, escaping, gate, ci]

requires:
  - phase: 21-01
    provides: the RemovalReport wire shape, POST /removals and its 409 drift body
  - phase: 21-02
    provides: the GET /rows body, the key to name index every label here is resolved through
provides:
  - bauded/web/removal.js, the pure PWA removal view model, fifteen exported functions on BaudeRemoval
  - bauded/web/tests/removal.test.js, 29 node:test cases over fixture output
  - scripts/check-web-suites.sh, the one command the gate, the Linux gate and CI use to run the PWA suites
affects: [21-05, 21-08, 21-11]

tech-stack:
  added: []
  patterns:
    - "PWA logic lives in a dependency-free module that loads as a browser script tag and as a Node CommonJS module, so it is tested with node --test and no package"
    - "Daemon data stays raw through the view model and is escaped at exactly one site, renderConfirmation"
    - "A gate step that cannot run is a failure: no node, no directory and no suite file each exit 1"

key-files:
  created:
    - bauded/web/removal.js
    - bauded/web/tests/removal.test.js
    - scripts/check-web-suites.sh
  modified:
    - scripts/gate.sh
    - .github/workflows/ci.yml

key-decisions:
  - "tildePath copies the TUI's plain string prefix strip, quirk included, so /Users/joe2/x under home /Users/joe reads ~2/x in both clients"
  - "dropRows and applySnapshot share one private walk, filterRows, and one match rule, rowKeyId, so the local drop and the later arbitration cannot disagree about scope"
  - "A tombstone keeps its original stamp when the same row is reported again, so a duplicate 200 or a self-originated rows_removed frame is a no-op"
  - "The node step runs first in the gate and outside the isolated bracket, because node may be a version-manager shim that needs HOME and the suites write nothing"

requirements-completed: []
requirements-advanced: [API-03]

coverage:
  - deliverable: "Every daemon-supplied value in the confirmation is escaped, asserted on the returned HTML string"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/web/tests/removal.test.js#render_confirmation_escapes_every_daemon_supplied_value"
        status: pass
  - deliverable: "Every reported row key is listed, resolved through GET /rows or honestly labelled from the key"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/web/tests/removal.test.js#report_row_names_never_invents_a_name"
        status: pass
  - deliverable: "A pre-removal snapshot applied after the removal cannot resurrect the row"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/web/tests/removal.test.js#apply_snapshot_suppresses_a_tombstoned_row_the_snapshot_still_lists"
        status: pass
  - deliverable: "A tombstone expires, so a client cannot permanently hide a row the daemon keeps reporting"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/web/tests/removal.test.js#prune_tombstones_expires_after_the_ttl_and_not_before"
        status: pass
  - deliverable: "tildePath agrees with the TUI helper, quirk included"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/web/tests/removal.test.js#tilde_path_keeps_the_plain_prefix_quirk"
        status: pass
  - deliverable: "The gate goes red on the PWA step when node is missing, a test fails, or no suite exists"
    human_judgment: false
    verification:
      - kind: command
        ref: "Task 4 verify V2 and V3, recorded below"
        status: pass
  - deliverable: "The check job of ci.yml runs the PWA suites before cargo fmt"
    human_judgment: false
    verification:
      - kind: command
        ref: "Task 4 verify V4, parsed ci.yml; the job itself has not yet run on a push"
        status: pass

duration: 13min
completed: 2026-11-11
status: complete
actuals:
  tokens: 14058
  tasks: 4
  commits: 5
plan_head_before: 4f7d39d2d05e68b061933b53f36537f5d1e6cfbd
plan_head_after: 79860eace628e2da17958c1c1bd7953310678853
---

# Phase 21 Plan 07: PWA pure removal module Summary

**`bauded/web/removal.js`: fifteen pure functions that turn a `RemovalReport` plus a `GET /rows` body into an escaped confirmation, a named row list and a tombstone-filtered snapshot, with 29 `node:test` cases now run first by `scripts/gate.sh`, `scripts/gate.sh --linux` and the `check` job of `ci.yml`**

## Performance

- **Duration:** 13 min
- **Started:** 2026-11-11T21:11:30Z
- **Completed:** 2026-11-11T21:24:00Z
- **Tasks:** 4
- **Files modified:** 5, three created and two modified

## Accomplishments

- The PWA has a removal view model with no network, no DOM, no clock and no dependency. 21-05 wires it; it invents nothing.
- Escaping has one site. `confirmationView` returns raw daemon data and `renderConfirmation` encodes it, and the test asserts on the returned string.
- A removal's scope can be held as a tombstone set that every later `/rows` snapshot is filtered through. Idempotency and expiry are each proven by a named test.
- The PWA suites are a gate step that can fail, proven on a red test, an empty directory, a missing directory and a missing `node`.

## Task Commits

1. **Task 1: a daemon report becomes escaped HTML** - `cbc21b6` (feat)
2. **Task 2: tildePath, the key to name join, dropRows and the drift-body parse** - `1d6a1d6` (feat)
3. **Task 3: snapshot arbitration** - `f305cc1` (feat)
4. **Task 4: the PWA suites in the gate, the Linux gate and CI** - `9bc769e` (ci)
5. **Addition: three plan-level edge tests** - `79860ea` (test)

## Exported surface of `bauded/web/removal.js`

The module is one IIFE. It sets `globalThis.BaudeRemoval` and, under Node, `module.exports` to the same object. It reads nothing from the host.

| Function | Signature | Returns |
|---|---|---|
| `escapeHtml` | `(value)` | string; the five replacements of `esc` at `bauded/web/app.js:33` |
| `tildePath` | `(path, home)` | string; `home` prefix becomes `~`; identity when `home` is falsy or `/` |
| `rowKeyId` | `(rowKey)` | `"checkout:7"` style id, or `null` for anything that is not one known tag with a non-negative integer |
| `keyEquals` | `(a, b)` | bool; same kind and same key; an unreadable shape equals nothing |
| `rowsIndex` | `(rowsBody)` | prototype-free object, `rowKeyId` to `{ kind, key, display_name, branch, path, session_id, repository_key }` |
| `reportRowNames` | `(report, index)` | array, one `{ id, kind, key, label, branch, resolved }` per `rows_removed` entry, none dropped |
| `confirmationView` | `(report, index, options)` | `{ title, sections: [{ label, count, lines }] }`, all values raw |
| `renderConfirmation` | `(view)` | HTML string; throws `TypeError` when handed a report or anything without a `sections` array |
| `dropRows` | `(rowsBody, report)` | new `/rows` body without the rows in `report.rows_removed` |
| `tombstone` | `(tombstones, report, nowMs)` | new prototype-free set, `rowKeyId` to stamp; an existing id keeps its first stamp |
| `pruneTombstones` | `(tombstones, nowMs, ttlMs)` | new set keeping entries with `nowMs - stamp <= ttlMs`; `ttlMs` defaults to `60000` |
| `applySnapshot` | `(snapshot, tombstones)` | new `/rows` body without tombstoned rows; does not prune |
| `parseDriftBody` | `(bodyText)` | `{ report, text }`; `report` only for a JSON object carrying `rows_removed`, otherwise the raw text |
| `clearingFailures` | `(report, options)` | array of `"<kind> <path>: <error>"` strings |
| `removalNotice` | `(report)` | `"removed N rows, killed N runtimes, deleted N paths"`, singular at one |

Notes on behaviour a caller can rely on:

- `confirmationView` sections, in order: `rows dropped`, `runtimes killed`, `paths deleted`, `paths kept`, then `already removed` and `cleared` only when non-empty. Empty `runtimes killed`, `paths deleted` and `paths kept` carry the single line `none`. `readers_parked` is not rendered.
- A row line is the `display_name`, with ` ⎇ <branch>` appended when the index has a branch. A worktree therefore reads `repo:topic ⎇ refs/heads/topic` today; the branch value is 21-09's to correct, and nothing here normalizes it.
- An unresolved key is labelled from the key, for example `checkout 8`, with `resolved: false`. An unreadable key is `unknown row`.
- `renderConfirmation` emits `<h3>`, then one `<div class="section"><h4>label (count)</h4><ul><li>..</li></ul></div>` per section. No buttons and no handlers; the controller owns those.
- `dropRows` and `applySnapshot` never mutate an argument, always return both `repositories` and `standalone` as arrays, keep any other root key, and remove a repository together with its checkouts. Removing a repository's last checkout leaves the parent with `checkouts: []`, the same shape `GET /rows` serves; 21-05 decides whether to render or hide it.
- The rendered confirmation carries no session id. Joins stay on `session_id` inside the index only.

### The `home` argument

`options.home` on `confirmationView` and `clearingFailures`, and the second argument of `tildePath`, has no default. When it is absent, not a string, empty or `/`, paths are shown as the absolute paths the daemon sent. `GET /info` serves no home path today: its fields are `workspace`, `backend`, `workspace_source`, `version`, `persistence`, `collisions`, `collision_count` and the startup timing. Until some endpoint serves one, 21-05 either passes nothing and shows absolute paths, or adds the field deliberately.

### The tombstone TTL and the three call sites 21-05 must wire

`TOMBSTONE_TTL_MS` is `60000`. The list poll is the `setInterval` at `bauded/web/app.js:800`, every 3 s, so the window covers an in-flight response twenty times over and still expires. Nothing is wired here and `app.js` is untouched. 21-05 wires exactly these:

1. Every `GET /rows` response goes through `applySnapshot(body, tombstones)` before it reaches render state.
2. A 200 report from `POST /removals`, and a `rows_removed` SSE frame, each go through `tombstone(tombstones, report, Date.now())`. `dropRows` gives the immediate local drop from the same report.
3. `pruneTombstones(tombstones, Date.now())` runs on the poll tick.

The caller owns the clock. No function in the module reads one.

## Node test count

**29 tests, 29 pass, 0 fail**, in one file, `bauded/web/tests/removal.test.js`. 21-11 should compare against `# pass 29`: a lower number means tests were replaced, not added.

```
node --test --test-reporter=tap bauded/web/tests/removal.test.js > /tmp/baude-21-07-final.log 2>&1
NODE=0  NOFAIL=0  PASS=29
```

Count by commit: 6 at Task 1, 18 at Task 2, 26 at Task 3, 29 after the addition. The export probe printed `exports ok: 15`.

## RED observations

RED is the first failing run before the production edit, not a separate commit, per the Phase 20 convention. The status and log files are kept under `/tmp`.

| Task | Status file | Observed |
|---|---|---|
| 1 | `/tmp/baude-21-07-t1-red.status` = 1 | `# Error: Cannot find module '../removal.js'`, `# pass 0`, `# fail 1`, with `bauded/web/removal.js` absent |
| 2 | `/tmp/baude-21-07-t2-red.status` = 1 | `# pass 6`, `# fail 12`; the twelve new tests, 7 to 18, each `not ok` |
| 3 | `/tmp/baude-21-07-t3-red.status` = 1 | `# pass 18`, `# fail 8`; the eight arbitration tests, 19 to 26, each `not ok` |

## How the gate and CI invoke the web suites

One script, no argument, in all three places, so they cannot drift.

`scripts/check-web-suites.sh [directory]` runs `node --test "<directory>/"`, default `bauded/web/tests`. It prints `check-web-suites: node <version>, N suite file(s) in <dir>` and, on success, `check-web-suites: passed`. It exits 1 when `node` is not on `PATH`, when the directory is missing, when the directory holds no `*.test.js`, and when a test fails; it exits 2 on more than one argument. It reads no environment variable.

- **`scripts/gate.sh`:** the first step is `step bash scripts/check-web-suites.sh || exit 1`, ahead of `cargo fmt --check`. It runs outside the `isolated` bracket.
- **`scripts/gate.sh --linux`:** the same script runs inside the `baude-gate` image. The image build now ends with `COPY --from=node:26-bookworm /usr/local/bin/node /usr/local/bin/node`, because `rust:1-bookworm` has no node. That major must stay equal to `node-version` in `ci.yml`.
- **`.github/workflows/ci.yml`, job `check`:** after `Swatinem/rust-cache@v2` and before `cargo fmt --check`, a step `uses: actions/setup-node@v7` with `node-version: 26`, then a step named `run the PWA suites` with `run: bash scripts/check-web-suites.sh`. No cache is configured, since there is no package and no lockfile.

`actions/setup-node@v7` was confirmed to exist before it was written: `git ls-remote --tags https://github.com/actions/setup-node` lists `refs/tags/v7` at `949feb2`, the same commit as `v7.1.0`. The major-tag pin matches the style of the other actions in the file.

### Task 4 verify statuses

| Verify | Result |
|---|---|
| V1, the step over the real suites | `run=0 passed=0` |
| V2, the failure modes against fixture directories | `red=1 empty=1 absent=1 nonode=1 usage=2` |
| V3, the gate with no node on `PATH` | `gate=1 named=0 reached=1`; the log ends at `FAIL (1): bash scripts/check-web-suites.sh` and no cargo step started |
| V4, `ci.yml` parsed with ruby | `ci=0`, `ci ok` |
| V5, the image build and the step in the container as uid 1000 | `build=0 linux=0 passed=0 major=0` |

Node versions reported: the host printed `node v26.8.1`, the container printed `node v26.10.0`. Same major, different minor, as expected from `node:26-bookworm`.

Plan-level verification: `STEP=0 GATE=1 NAMED=0`, step log first line `check-web-suites: node v26.8.1, 1 suite file(s) in bauded/web/tests`.

### The full Linux gate

The plan defers the full gate to 21-11, because 21-09 was planned to share this wave. This run was sequential with no other plan's diff in the tree, and the orchestrator asked for it, so it was run once over the final tree:

```
bash scripts/gate.sh --linux > /tmp/baude-21-07-linux-gate.log 2>&1
exit status 0, last line: gate: all steps passed
```

Every step ran: the PWA suites with 29 passing, `cargo fmt --check`, clippy, both commit-message checks including the `origin/main..HEAD` range over this plan's commits, the real-roots self-test and bracket, and `cargo test -- --test-threads=1` with 276, 560, 120, 2, 14, 16 and 1 passing and 1 ignored.

Ledger rows 13 and 14 in `.planning/WINDOWS.md` record `scripts/gate.sh --linux` exiting 1 on this macOS host with E0463. That did not reproduce in this run. The rows were left as they are; one green run does not prove a race is gone.

The `check` job has not yet run on a push. Its first run is the remaining proof of the `ci.yml` half.

## Closing checks

| Command | Exit |
|---|---|
| `bash scripts/check-web-suites.sh` | 0 |
| `cargo fmt --all --check` | 0 |
| `cargo test -p bauded` | 0, 120 passed and 0 failed |
| `bash scripts/gate.sh --linux` | 0 |

Scope fence, `git diff --name-status 4f7d39d HEAD -- bauded baude baude-core scripts .github`: exactly `A bauded/web/removal.js`, `A bauded/web/tests/removal.test.js`, `A scripts/check-web-suites.sh`, `M scripts/gate.sh`, `M .github/workflows/ci.yml`. No Rust file, no `app.js`, no `web.rs`, no `index.html`.

## Deviations from Plan

**1. [Addition] A fifth commit with three plan-level edge tests**
- **Found during:** the pass over the plan's must-haves and prohibitions after Task 4
- **What:** `drop_rows_applied_twice_changes_nothing_more`, `rendered_confirmation_carries_no_session_identifier` and `indices_and_tombstones_ignore_prototype_shaped_keys`. Each pins a claim the plan makes that no Task 1 to 3 test asserted directly. No production change was needed; all three passed on first run.
- **Effect:** five commits, not four, and 29 tests, not 26. The scope check was run against `4f7d39d`, the first commit's parent, as the plan allows.
- **Commit:** 79860ea

**2. [Test correction] One assertion in `render_confirmation_does_not_double_escape`, before the Task 1 commit**
- **Found during:** Task 1, first GREEN run, 5 of 6 passing
- **Issue:** the failing assertion was wrong in the test, not in the module. A hand-built view line that is already entity text must be encoded once more by the renderer, so an ampersand entity in the output is correct there.
- **Fix:** the test now asserts the two things that matter: a raw `<` through the whole pipeline comes out as `&lt;` and never `&amp;lt;`, and entity text handed to the renderer is encoded exactly once, never to `&amp;amp;`. This is stricter than the first draft, not weaker.
- **Commit:** cbc21b6

**3. [Scope] The full `scripts/gate.sh --linux` was run**
- The plan says not to run the full gate here. The reason it gives did not apply to a sequential run, and the orchestrator's constraints asked for the result. It passed; see above.

**4. [Bookkeeping] API-03 left pending**
- The plan frontmatter lists `requirements: [API-03]`. API-01, API-02 and API-03 are shared with later plans in this phase and are not marked complete in REQUIREMENTS.md.

**Total deviations:** 1 addition, 1 test correction, 1 extra verification run, 1 bookkeeping. **Impact:** the exported surface and the gate wiring are exactly as planned.

## TDD Gate Compliance

- RED: three recorded runs, one per TDD task, each with status 1 and the failing test names in the log. Not separate commits.
- GREEN: `cbc21b6`, `1d6a1d6`, `f305cc1`.
- REFACTOR: none. The private `filterRows` walk was written in Task 2 under `dropRows`; Task 3 added `applySnapshot` on top of it with no change to it.
- Tracer gate, Task 1: auto mode is active. The Task 1 verify passed on the tree that was committed, `check=0`, `# pass 6`, `# fail 0`, and Task 2's RED run then showed those six still passing before any expansion code was written.

## Issues Encountered

None beyond the deviations above.

## Known Stubs

None. The module is not yet loaded by the page; that is 21-05's wiring, not a stub in this plan.

## Next Phase Readiness

For 21-05:

- Load `removal.js` before `app.js` and call through `BaudeRemoval`. This plan changed no served file list and no page markup; the plan's `<source_corrections>` has the verified serving mechanism.
- Never insert daemon data into the page except through `renderConfirmation`, or through `escapeHtml` for a single value such as `removalNotice` and `clearingFailures` lines. Those two return raw strings.
- On a non-200 from `POST /removals`, pass the body text to `parseDriftBody`. A non-null `report` is the fresh 409 preview to re-render; otherwise show `text`.
- Wire the three tombstone call sites listed above, and pass a `home` only if an endpoint serves one.
- Decide deliberately what a parent with `checkouts: []` looks like.

For 21-11:

- Expect `# pass 29` from the TAP run and `exports ok: 15` from the probe.
- The gate's first step is `bash scripts/check-web-suites.sh` and prints `check-web-suites: passed`. A new suite file under `bauded/web/tests/` is picked up with no gate edit.
- The first push is the first run of the `check` job with the node steps. Confirm it there.
- If the node major changes, change it in both `ci.yml` and the `COPY --from=node:26-bookworm` line of `scripts/gate.sh`.

## Self-Check: PASSED

- FOUND: bauded/web/removal.js, bauded/web/tests/removal.test.js, scripts/check-web-suites.sh, scripts/gate.sh, .github/workflows/ci.yml
- FOUND: cbc21b6, 1d6a1d6, f305cc1, 9bc769e, 79860ea as ancestors of HEAD
- `git rev-list --count 4f7d39d..HEAD` measured 5 before the metadata commit
