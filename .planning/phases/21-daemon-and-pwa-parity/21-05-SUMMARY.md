---
phase: 21-daemon-and-pwa-parity
plan: 05
subsystem: ui
tags: [pwa, javascript, node-test, removal, sse, service-worker, axum, fixtures]

requires:
  - phase: 21-07
    provides: BaudeRemoval in removal.js, the 15 pure exports this plan calls and does not re-implement
  - phase: 21-02
    provides: GET /rows, the only route that serves row keys; clients join on session_id
  - phase: 21-03
    provides: the 404, 409, 500, 503 and saved_degraded 200 status matrix of POST /removals
  - phase: 21-04
    provides: GET /events with the named rows_removed message carrying the RemovalReport
provides:
  - BaudeRemovalController in bauded/web/removal-controller.js with createRemovalController and renderRowsList
  - A hierarchical PWA row list built from GET /rows with one remove control per keyed row
  - A report-backed removal confirmation with Cancel, Escape and backdrop all routed through controller.cancel
  - A GET /events subscription that drops rows in every open tab and reconciles through GET /rows
  - /removal.js and /removal-controller.js served by web::router, and the baude-v7 service-worker shell
  - bauded/web/tests/removal-controller.test.js, 40 node tests
affects: [21-11, 21-08, 21-06]

actuals:
  tokens: 25657
  tasks: 3
  commits: 6
plan_head_before: 86ace68197b467f2a4d61af524a39828e37b9d2f
plan_head_after: 66ee732fa7372f1063e35480969e73240b277ed9

tech-stack:
  added: []
  patterns:
    - "Controller state sits behind injected deps: request, openEvents, now, onChange, notify, onSessionGone. Node tests drive it through deferred request doubles and an emitting event-source double, with /rows bodies and reports in the shapes the daemon serves"
    - "One way in for a fetched /rows body, applyRows, and one writer of tombstones, commit. app.js never assigns a fetched rows body"
    - "List markup is proven on the string renderRowsList returns, and served assets on responses from web::router, never by reading source files"

key-files:
  created:
    - bauded/web/removal-controller.js
    - bauded/web/tests/removal-controller.test.js
  modified:
    - bauded/web/app.js
    - bauded/web/index.html
    - bauded/web/style.css
    - bauded/web/sw.js
    - bauded/src/web.rs

key-decisions:
  - "A fetched /sessions list is filtered through the controller as well as /rows, so a stale poll cannot bring back the card of a removed row"
  - "A 409 refetches /rows on a request of its own and never reuses a poll already in flight, then draws the fresh report; it never confirms for the user"
  - "A 404 on preview or execute is treated as removed: the row is dropped and tombstoned, the modal closes with no Retry and /rows is refetched"
  - "Error text and notices keep the daemon's wording with one substitution: a Rust key token becomes the row label, or the words unknown row"
  - "A session no /rows entry claims is listed as a card with no remove control; app.js lists these itself and does not pass options.keyless"
  - "Without both removal scripts the page falls back to the flat list with no remove controls"

requirements-completed: []
requirements-advanced: [API-02, API-03]

coverage:
  - deliverable: "A pre-removal /rows body that resolves after the removal cannot put the row back"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/web/tests/removal-controller.test.js#pending_poll_cannot_resurrect_a_removed_row"
        status: pass
      - kind: test
        ref: "bauded/web/tests/removal-controller.test.js#stale_poll_after_unknown_target_cannot_resurrect_the_row"
        status: pass
  - deliverable: "A 409 refetches /rows before the fresh report is drawn and names a child added after the preview"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/web/tests/removal-controller.test.js#conflict_refetches_rows_before_rerender_and_names_new_child"
        status: pass
      - kind: test
        ref: "bauded/web/tests/removal-controller.test.js#conflict_refetch_does_not_reuse_a_poll_already_in_flight"
        status: pass
  - deliverable: "The /events subscription uses a named listener only, drops rows without a fetch and refetches after a reconnect"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/web/tests/removal-controller.test.js#events_subscribe_uses_named_listener_only"
        status: pass
      - kind: test
        ref: "bauded/web/tests/removal-controller.test.js#event_drops_rows_without_a_fetch"
        status: pass
      - kind: test
        ref: "bauded/web/tests/removal-controller.test.js#reconnect_refetches_rows"
        status: pass
  - deliverable: "The initiating tab's own frame is not reported as removed elsewhere, and a duplicate frame and response change state once"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/web/tests/removal-controller.test.js#own_event_before_post_response_is_not_removed_elsewhere"
        status: pass
      - kind: test
        ref: "bauded/web/tests/removal-controller.test.js#duplicate_event_and_response_change_state_once"
        status: pass
      - kind: test
        ref: "bauded/web/tests/removal-controller.test.js#double_confirm_sends_one_post"
        status: pass
  - deliverable: "500 and 503 keep the modal open with the daemon's text and a Retry that re-posts the held report"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/web/tests/removal-controller.test.js#server_error_keeps_modal_open_and_retry_reposts_same_report"
        status: pass
      - kind: test
        ref: "bauded/web/tests/removal-controller.test.js#busy_503_keeps_modal_open_with_retry"
        status: pass
  - deliverable: "No error line and no notice carries a Rust key token"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/web/tests/removal-controller.test.js#error_text_names_a_known_row_by_label"
        status: pass
      - kind: test
        ref: "bauded/web/tests/removal-controller.test.js#error_text_reads_unknown_row_for_a_key_the_index_cannot_name"
        status: pass
      - kind: test
        ref: "bauded/web/tests/removal-controller.test.js#error_text_without_a_key_token_is_unchanged"
        status: pass
      - kind: test
        ref: "bauded/web/tests/removal-controller.test.js#notice_text_names_a_row_removed_by_the_same_response"
        status: pass
  - deliverable: "Cancel closes without mutating and does nothing while a removal is executing, the controller half of D-13"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/web/tests/removal-controller.test.js#cancel_while_idle_closes_without_mutating"
        status: pass
      - kind: test
        ref: "bauded/web/tests/removal-controller.test.js#cancel_while_executing_is_a_noop"
        status: pass
  - deliverable: "All four row kinds carry one remove control beside the card, keyless sessions carry none, daemon strings are escaped"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/web/tests/removal-controller.test.js#rows_list_offers_remove_on_all_four_kinds"
        status: pass
      - kind: test
        ref: "bauded/web/tests/removal-controller.test.js#rows_list_never_nests_remove_inside_a_card_button"
        status: pass
      - kind: test
        ref: "bauded/web/tests/removal-controller.test.js#keyless_session_has_no_remove_control"
        status: pass
      - kind: test
        ref: "bauded/web/tests/removal-controller.test.js#rows_list_escapes_daemon_strings"
        status: pass
  - deliverable: "The daemon serves both scripts in load order and a baude-v7 shell that lists them"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/web.rs#removal_assets_are_served_in_load_order"
        status: pass
      - kind: test
        ref: "bauded/src/web.rs#service_worker_shell_matches_served_assets"
        status: pass
  - deliverable: "In a real browser: tap targets, Escape and backdrop, the wide modal at phone width, two-tab removal"
    human_judgment: true
    verification:
      - kind: backstop
        ref: "21-11 checkpoint:human-verify"
        status: not-run

metrics:
  duration: 40min
  completed: 2026-11-12
status: complete
---

# Phase 21 Plan 05: PWA removal controller and report-backed modal Summary

The PWA now builds its list from `GET /rows`, offers remove on repository, main checkout, worktree
and folder rows through a preview-then-confirm modal over the removal routes, and drops rows in
every open tab from the `rows_removed` stream, with 40 node tests driving the controller through
its exported interface.

**The UI was not exercised in a browser.** Nothing in this plan rendered the page, tapped a
control or opened the stylesheet in a viewport. Every claim below is about strings, state and
served bytes. What 21-11 must look at by hand is listed at the end.

## What was built

- `bauded/web/removal-controller.js`, new. `createRemovalController(deps)` holds the `/rows`
  body, its tombstones, the gone-session map and the confirmation modal. `applyRows` is the one
  way a fetched body becomes state and `commit` is the one writer of tombstones, fed by a 200, a
  `rows_removed` frame and a 404 alike. `renderRowsList(rowsBody, options)` returns the list
  HTML. `removal.js` has no changed line and none of its 15 exports is re-implemented.
- `bauded/web/app.js`. One controller built at boot from `api`, `new EventSource(url)`,
  `Date.now`, a redraw, `toast` and a hash reset for an open session that went away. `refresh`
  calls `loadRows` beside the `/sessions` fetch and filters the fetched list through the
  controller. One delegated click listener on the list reads `data-kind` and `data-key`. The
  confirmation element is created once and redrawn from `getModal()`; the error line is written
  with `textContent`.
- `bauded/web/index.html`: the two scripts load ahead of `app.js`.
- `bauded/web/style.css`: `.repohead`, `.rowwrap`, `.rowline`, `.rm` at a 44px minimum, and
  `#modal .box.wide` capped at the smaller of 92vw and 720px with an inner scroll. The 420px rule
  of the new-session form is untouched.
- `bauded/web/sw.js`: cache `baude-v7`, shell gains `/removal.js` and `/removal-controller.js`.
  The fetch handler is unchanged.
- `bauded/src/web.rs`: two asset routes and a tests module of two tests over `web::router`.

## Task commits

| Task | RED | GREEN |
| ---- | --- | ----- |
| 1 Tracer over one checkout row | `39d700f` | `11dbdf7` |
| 2 Snapshot arbitration, events and the failure matrix | `aca5535` | `a2899f0` |
| 3 Four row kinds, wide modal, service-worker shell | `d83a949` | `66ee732` |

RED runs, recorded before each production edit and kept on disk under
`/var/folders/q1/335pbzl13sz825k87q4ywl300000gq/T/`:

| Task | File | Status | Result |
| ---- | ---- | ------ | ------ |
| 1 | `baude-21-05-t1-red.node.status` | 1 | 0 pass, 6 fail |
| 1 | `baude-21-05-t1-red.cargo.status` | 101 | asset routes missing |
| 2 | `baude-21-05-t2-red.status` | 1 | 14 pass, 20 fail |
| 3 | `baude-21-05-t3-red.node.status` | 1 | 63 pass, 6 fail |
| 3 | `baude-21-05-t3-red.cargo.status` | 101 | panics with `cache name is not baude-v7` |

## Pass floors, recomputed

The plan wrote its floors assuming 21-07 shipped 20 node tests. `removal.test.js` measured 29
before Task 1, a difference of 9.

| Task | Verify runs | Plan floor | Recomputed floor | Measured |
| ---- | ----------- | ---------- | ---------------- | -------- |
| 1 | `removal-controller.test.js` only | at least 6 | at least 6, no 21-07 share | 6 |
| 2 | `removal-controller.test.js` only | at least 33 | at least 33, no 21-07 share | 34 |
| 3 | all of `bauded/web/tests/` | at least 59 | at least 68 | 69 |

The plan's own checks are lower bounds written with `-ge`; they were kept as lower bounds and
none was loosened. Final `# pass` of `removal-controller.test.js`: **40**. With 21-07's 29 the
directory total is **69**, 0 failing.

## Closing gates

| Command | Exit |
| ------- | ---- |
| `scripts/check-web-suites.sh` | 0 |
| `cargo fmt --all --check` | 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 |
| `cargo test -p bauded` | 0, 176 passed, up from 174 |

The open `failed to terminate exact PTY process group` flake did not appear in this run, so no
second run was needed. `scripts/gate.sh` and `scripts/gate.sh --linux` were not run; 21-11 owns
them.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] One assertion in this plan's own RED test could never pass**
- **Found during:** Task 3 GREEN
- **Issue:** `rows_list_escapes_daemon_strings` asserted the raw sequence `"><` was absent from
  the list. That sequence is ordinary markup wherever one tag closes an attribute and the next
  tag opens, so no correct list could satisfy it.
- **Fix:** the assertion now requires the encoded form of the hostile value,
  `&quot;&gt;&lt;script&gt;alert(2)&lt;/script&gt;`, and keeps every other raw-sequence check.
- **Files modified:** `bauded/web/tests/removal-controller.test.js`
- **Commit:** `66ee732`

### Plan gap, left as found

**2. `options.keyless` is tested but the page does not pass it**
- Task 3 specifies `options.keyless` and a heading reading `other sessions`, and also says
  `app.js` needs no change and to stop rather than widen the task's files. Task 1 never asked
  `app.js` to pass the option. `app.js` therefore lists unclaimed sessions itself, as Task 1 left
  it: active ones directly after the hierarchy with no heading, archived ones under `archived`.
- The behaviour the truth requires holds in the page: those cards carry no remove control. What
  differs is the heading text, and the `keyless` branch of `renderRowsList` is reached only by
  its test. Wiring it is about eight lines in `renderList`. Not done here because the plan
  forbids widening Task 3 to `app.js`; raised to the orchestrator.

### Notes

- Six commits, not three: the plan names a `test` and a `feat` commit for each task.
- The repository control's label reads `remove repository <name>`, because a repository and its
  main checkout share a display name and the two removals differ.
- Requirements API-02 and API-03 are advanced, not completed. No task in the plan marks one, and
  the last truth is a browser backstop owned by 21-11. API-01, API-02 and API-03 stay pending.

## Open items carried from the plan

1. **No home directory reaches the PWA.** `confirmationView` is called with `{ home: null }`, so
   paths render in full and the wide modal wraps them. Serving a home field is a `/rows` wire
   change owned by 21-02 and is not invented here.
2. **API-02 edge, unresolved.** A missed frame is recovered only by the 3 s `/rows` poll and by
   the refetch on reconnect. Flagged assumption, not resolved by this plan.
3. **Archived rows.** They stay under their repository, dimmed and sorted after non-archived
   siblings, instead of under one trailing heading. Assumption.

## Unverified until 21-11

**The Escape and backdrop bindings in `app.js` are unverified until 21-11's human-verify
(D-13).** The node tests prove `controller.cancel`; nothing proves the `keydown` listener or the
backdrop click listener reach it. 21-11's step 3 covers the Cancel button only, so this hand-off
needs a step added there. This plan does not edit 21-11.

What 21-11's manual check must look at, against a live daemon:

1. Tapping a remove control opens the confirmation and does not navigate to the chat view.
   Tapping a card navigates and does not open the confirmation.
2. Escape closes the confirmation with the row still listed. A tap on the backdrop does the
   same. Escape in the chat textarea and in the new-session form behaves as before.
3. While a removal is executing, Cancel, Escape and the backdrop do nothing and the button reads
   `removing…`. After a 500 or 503 the daemon's text shows and the button reads `retry`.
4. The wide confirmation at phone width: full paths wrap with no horizontal clipping, the list
   scrolls inside the box and the two actions stay on screen.
5. The list layout, none of which has been seen: repository heading with its own remove control,
   `main` and `worktree` tags, the 44px remove target beside each card, rows with no live
   session shown as plain lines, archived rows dimmed and last, the `folders` heading, and
   where sessions that no row claims land.
6. Two tabs: removing a row in one drops it in the other with no refresh, and the first tab
   shows one notice, not `removed elsewhere`.
7. A daemon restart with a tab open: the stream reconnects and the list reconciles.
8. An installed PWA picks up `baude-v7` and loads both new scripts; with the old cache the page
   still lists sessions and shows no remove controls.

## Known Stubs

None. The unwired `keyless` option is recorded above as a plan gap, not a stub: the page lists
those sessions through its own path.

## Self-Check: PASSED

- All 7 source files and this SUMMARY exist on disk.
- Commits `39d700f`, `11dbdf7`, `aca5535`, `a2899f0`, `d83a949` and `66ee732` are ancestors of HEAD.
- All five RED status files exist with the statuses in the table above.
- `git diff 86ace68..HEAD -- bauded/web/removal.js` is empty.
