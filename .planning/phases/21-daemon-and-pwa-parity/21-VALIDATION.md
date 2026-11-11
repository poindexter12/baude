---
phase: "21"
slug: "daemon-and-pwa-parity"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: true
created: "2026-11-05"
updated: "2026-11-11"
---

# Phase 21 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Rebuilt 2026-11-11 against the 11-plan set (`21-01-PLAN.md` … `21-11-PLAN.md`), wave order
> 01, 02, 07+09, 10, 03, 04, 05+08, 06, 11. 33 tasks: 32 with an `<automated>` verify, 1 manual checkpoint.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (Rust workspace: `baude-core`, `baude`, `bauded`) and `node --test` (built-in runner, no package) for the two PWA suites |
| **Config file** | none — `Cargo.toml` workspace; the node suites need no config. `bauded/web/tests/` does not exist yet: 21-07 creates `removal.test.js`, 21-05 creates `removal-controller.test.js` |
| **Quick run command** | the task's own `<automated>` filter from the map below, e.g. `cargo test -p bauded api::tests::removals_ -- --test-threads=1` |
| **Full suite command** | `scripts/gate.sh` (fmt, clippy, commit-message checks, real-roots bracket, `cargo test -- --test-threads=1`). From 21-07 Task 4 on, its first step is `bash scripts/check-web-suites.sh`, which runs every suite under `bauded/web/tests/`; `scripts/gate.sh --linux` runs the same step in the container, and the CI `check` job runs it on every push. Before that commit lands the gate does not run the node suites |
| **PWA suites** | `node --test --test-reporter=tap bauded/web/tests/removal.test.js bauded/web/tests/removal-controller.test.js` — the tap-reporter form, run inside 21-07 and 21-05 task verifies and by 21-11 Task 1 to read the pass count against the floor. The pass or fail verdict on every push comes from the gate: user decision 2026-11-11, implemented by 21-07 Task 4, makes `bash scripts/check-web-suites.sh` (`node --test bauded/web/tests/`, failing loudly on a missing node, a missing directory or no suite found) the first step of `scripts/gate.sh`, of `scripts/gate.sh --linux` and of the CI `check` job |
| **Estimated runtime** | not measured. Per-task filters are a single crate filter or one node file; `scripts/gate.sh` runs the whole workspace single-threaded and is the slow path |

---

## Sampling Rate

- **After every task commit:** run that task's `<automated>` command(s) from the map below. Each plan writes them as: redirect to a log, capture the exit status in a variable as its own statement, judge with `test`. Never pipe a gate step to `tail`/`grep` and read `$?`.
- **After every plan:** the plan's last task carries the crate gate (`cargo fmt --check` plus crate-wide `cargo test -p <crate> -- --test-threads=1`, and `cargo clippy -p <crate> --all-targets -- -D warnings` where the plan lists it).
- **After every plan wave / before every push:** `scripts/gate.sh`. From wave 3 on, once 21-07 Task 4 has landed, that run starts with the node suites; CI runs the same step on every push.
- **Phase end (21-11 only):** `scripts/gate.sh` (which runs both node suites; its log must carry `check-web-suites: passed`), one tap-reporter `node --test` run over both suites for the pass floor, `node --check bauded/web/app.js`, then `scripts/gate.sh --linux`, then the 17-step manual checkpoint.
- **Before `/gsd-verify-work`:** `scripts/gate.sh` green on the final commit and 21-11 Task 3 approved.
- **Max feedback latency:** not measured; no numeric bound is claimed here.

---

## Per-Task Verification Map

Conventions for this table:

- **Requirement** is the plan's frontmatter `requirements` (the plans do not tag requirements per task).
- **Threat Ref** is the plan's threat-model ID set, not a per-task mapping; every plan also carries `T-21-SC`.
- **Automated Command** is the command core from the task's `<verify><automated>`; log redirection and status-capture boilerplate are elided. Where a command is too long to quote, the plan line is cited and is authoritative.
- **File Exists**: every test named below is written by its own task, RED first (tdd plans). None exists in the tree today, so nothing is ✅ yet.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 21-01-01 | 01 | 1 | API-01 | T-21-01, 02, 13, 14 | One checkout row removed over REST; save closure preserves merges | integration (REST) | `cargo test -p bauded api::tests::removals_ -- --test-threads=1` (≥1 test run) | ❌ in-task | ⬜ pending |
| 21-01-02 | 01 | 1 | API-01 | T-21-01, 02, 13, 14 | Survivor metadata not regressed; drop survives a fresh restore | integration | `cargo test -p bauded api::tests::removals_ -- --test-threads=1`; `cargo test -p bauded api::tests::removals_preserve_surviving_session_metadata -- --test-threads=1 --exact` | ❌ in-task | ⬜ pending |
| 21-01-03 | 01 | 1 | API-01 | T-21-01, 02, 13, 14 | Two concurrent removals of one row resolve safely | integration + crate gate | `cargo test -p bauded api::tests::removals_ -- --test-threads=1`; `cargo fmt --check` | ❌ in-task | ⬜ pending |
| 21-02-01 | 02 | 2 | API-03 | T-21-15 to 18 | `/sessions` stays a flat compatibility projection (D-09 Option B) | integration | `cargo test -p bauded api::tests::flat_session_api_remains_a_non_hierarchical_compatibility_projection -- --test-threads=1 --exact` | ❌ in-task | ⬜ pending |
| 21-02-02 | 02 | 2 | API-03 | T-21-15 to 18 | `GET /rows` is the authoritative key-to-name index | integration | `cargo test -p bauded api::tests::rows_ -- --test-threads=1` | ❌ in-task | ⬜ pending |
| 21-02-03 | 02 | 2 | API-03 | T-21-15 to 18 | D-09 wire consequence proven; API-03 concurrency edge closed | integration + crate gate | `cargo test -p bauded api::tests:: -- --test-threads=1`; `cargo fmt --check` | ❌ in-task | ⬜ pending |
| 21-07-01 | 07 | 3 | API-03 | T-21-19 to 23 | Daemon report becomes escaped HTML (`<` leaves as `&lt;`) | unit (node) | `node --check bauded/web/removal.js`; `node --test --test-reporter=tap bauded/web/tests/removal.test.js` (`# fail 0`, pass ≥ 5) | ❌ in-task | ⬜ pending |
| 21-07-02 | 07 | 3 | API-03 | T-21-19 to 23 | tildePath, key-to-name join, dropRows, drift-body parse | unit (node) | same suite (`# fail 0`, pass ≥ 15); `node --check bauded/web/removal.js` | ❌ in-task | ⬜ pending |
| 21-07-03 | 07 | 3 | API-03 | T-21-19 to 23 | Tombstones a pending poll cannot undo | unit (node) | same suite (`# fail 0`, pass ≥ 20); `node --check bauded/web/removal.js`; `node -e` export-surface probe (21-07 line 443) | ❌ in-task | ⬜ pending |
| 21-07-04 | 07 | 3 | API-03 | T-21-96, 97 | PWA suites run first in the gate, the Linux gate and CI; the step cannot pass without running a test | gate step (shell + node) | `bash scripts/check-web-suites.sh` (exit 0, log ends `check-web-suites: passed`); the same script against a failing fixture, an empty directory, a missing directory and a node-less `PATH` (each exit 1); `PATH=/usr/bin:/bin bash scripts/gate.sh` (exit 1, log has `FAIL (1): bash scripts/check-web-suites.sh`, no cargo step); `ruby -ryaml` check of the parsed `check` job; container run of the script in the image `scripts/gate.sh --linux` builds (21-07 lines 551 to 560) | ❌ in-task | ⬜ pending |
| 21-09-01 | 09 | 3 | API-01 | T-21-24 to 29 | Repository parent removed over REST; main checkout admitted as `main` | integration | `cargo test -p bauded api::tests::row_kinds_ -- --test-threads=1`; `cargo test -p bauded api::tests::removals_ -- --test-threads=1` | ❌ in-task | ⬜ pending |
| 21-09-02 | 09 | 3 | API-01 | T-21-24 to 29 | `/rows` serves git's branch name; main checkout removable without touching its directory | integration | `cargo test -p bauded api::tests::row_kinds_branch_ -- --test-threads=1`; `… api::tests::row_kinds_ …`; `… api::tests::rows_ …` | ❌ in-task | ⬜ pending |
| 21-09-03 | 09 | 3 | API-01 | T-21-24 to 29 | Fan-out target edges; pre-plan row shape | integration + crate gate | `cargo test -p bauded api::tests::row_kinds_ -- --test-threads=1`; `cargo test -p bauded -- --test-threads=1`; `cargo fmt --check` | ❌ in-task | ⬜ pending |
| 21-10-01 | 10 | 4 | API-01, API-03 | T-21-30 to 38 | Plain folder is a standalone row: create, serve, remove | integration | `cargo test -p bauded api::tests::standalone_ -- --test-threads=1`; `cargo test -p bauded plain_admission_ -- --test-threads=1`; `cargo test -p bauded -- --test-threads=1` | ❌ in-task | ⬜ pending |
| 21-10-02 | 10 | 4 | API-01, API-03 | T-21-30 to 38 | Folder session custody on close, exit, restart route, daemon restart | integration | `cargo test -p bauded api::tests::standalone_ -- --test-threads=1`; `cargo test -p bauded poll_clears_an_immediately_dead_targeted_resume -- --test-threads=1`; `cargo test -p bauded -- --test-threads=1` | ❌ in-task | ⬜ pending |
| 21-10-03 | 10 | 4 | API-01, API-03 | T-21-30 to 38 | Explicit admission wired to the suppression store; two real folder shapes | integration + crate gate | `cargo test -p bauded api::tests::suppression_ -- --test-threads=1`; `cargo test -p bauded suppress -- --test-threads=1`; `… api::tests::standalone_ …`; `cargo test -p bauded -- --test-threads=1`; `cargo fmt --check` | ❌ in-task | ⬜ pending |
| 21-03-01 | 03 | 5 | API-01 | T-21-39 to 47 | Row whose runtime handle the daemon lacks is 503 and touches nothing | integration | `cargo test -p bauded api::tests::matrix_ -- --test-threads=1` | ❌ in-task | ⬜ pending |
| 21-03-02 | 03 | 5 | API-01 | T-21-39 to 47 | Each compared set refuses 409; uncompared fields cannot steer the daemon | integration | `cargo test -p bauded api::tests::matrix_ -- --test-threads=1`; `cargo test -p bauded api::tests::matrix_drift_uncompared_fields_cannot_steer_the_daemon -- --test-threads=1 --exact` | ❌ in-task | ⬜ pending |
| 21-03-03 | 03 | 5 | API-01 | T-21-39 to 47 | 500, 503 and degraded 200 each driven by its cause | integration + crate gate | `cargo test -p bauded api::tests::matrix_ -- --test-threads=1`; `cargo test -p bauded -- --test-threads=1`; `cargo fmt --check` | ❌ in-task | ⬜ pending |
| 21-04-01 | 04 | 6 | API-02 | T-21-50 to 56 | `rows_removed` over `GET /events` for one checkout row | integration (SSE) | `cargo test -p bauded api::tests::events_removal_over_rest -- --test-threads=1`; `… api::tests::events_ …`; `… api::tests::stream_guards_unknown_session …`; `cargo build -p bauded` | ❌ in-task | ⬜ pending |
| 21-04-02 | 04 | 6 | API-02 | T-21-50 to 56 | Every row kind publishes once; every failure publishes nothing | integration + mutant check | early-send mutant log check on `api::tests::events_silent_on_persistence_503` (21-04 line 286); `cargo test -p bauded api::tests::events_ -- --test-threads=1`; `cargo test -p bauded -- --test-threads=1` | ❌ in-task | ⬜ pending |
| 21-04-03 | 04 | 6 | API-02 | T-21-50 to 56 | Lag driven through the route's own loop | integration + mutant check + crate gate | lag mutant log check on `api::tests::events_route_survives_lag` (21-04 line 325); `cargo test -p bauded api::tests::events_ -- --test-threads=1`; `cargo fmt --check`; `cargo clippy -p bauded --all-targets -- -D warnings`; `cargo test -p bauded -- --test-threads=1` | ❌ in-task | ⬜ pending |
| 21-05-01 | 05 | 7 | API-03, API-02 | T-21-60 to 69 | One checkout row previewed then confirmed or cancelled from the PWA | unit (node, controller) | `node --test --test-reporter=tap bauded/web/tests/removal-controller.test.js` plus named-test presence checks incl. `happy_path_preview_then_confirm_drops_the_row_and_closes` (21-05 line 231) | ❌ in-task | ⬜ pending |
| 21-05-02 | 05 | 7 | API-03, API-02 | T-21-60 to 69 | Snapshot arbitration, `/events` subscription, failure matrix | unit (node, controller) | same controller suite plus named-test checks incl. `pending_poll_cannot_resurrect_a_removed_row` (21-05 line 302) | ❌ in-task | ⬜ pending |
| 21-05-03 | 05 | 7 | API-03, API-02 | T-21-60 to 69 | Four row kinds, wide modal, service-worker shell; remove control never nested in a card button | unit (node) + integration | `node --test --test-reporter=tap bauded/web/tests/` incl. `rows_list_never_nests_remove_inside_a_card_button`; `cargo test -p bauded web::tests -- --test-threads=1` (21-05 line 345) | ❌ in-task | ⬜ pending |
| 21-08-01 | 08 | 7 | API-01, API-02 | T-21-70 to 76 | Remote removal through the client; bodies read; key found by rows join | unit/integration (client) | recorded RED-status check (21-08 line 239); `cargo test -p baude remote::tests -- --test-threads=1`; `cargo test -p baude remote::tests::removal_closed_without_an_answer -- --test-threads=1`; `cargo fmt --check`; `cargo clippy -p baude --all-targets -- -D warnings` | ❌ in-task | ⬜ pending |
| 21-08-02 | 08 | 7 | API-01, API-02 | T-21-70 to 76 | Removal round trips off the draw thread, own timeout, unanswered execute reconciled | unit/integration | `cargo test -p baude remote::tests::ticket_ -- --test-threads=1`; `… remote::tests::ticket_closed_without_an_answer …`; `Timeouts`/`idle_with` source-shape grep (21-08 line 300); `cargo test -p baude remote::tests -- --test-threads=1` | ❌ in-task | ⬜ pending |
| 21-08-03 | 08 | 7 | API-01, API-02 | T-21-70 to 76 | Events subscriber with reachable stop, rows poll, tombstone-filtered snapshot | unit/integration + crate gate | `cargo test -p baude remote::tests::snapshot_ -- --test-threads=1`; `cargo test -p baude remote::tests::events_ -- --test-threads=1`; `cargo test -p baude --no-run`; `cargo fmt --check`; `cargo clippy -p baude --all-targets -- -D warnings`; commit file-scope check (21-08 line 369) | ❌ in-task | ⬜ pending |
| 21-06-01 | 06 | 8 | API-01, API-02 | T-21-80 to 89 | `D` on a remote row removes it through the daemon | unit (TUI) | `cargo test -p baude -- app::tests::sidebar_action_d_ app::tests::d_on_remote_row_ ui::tests::remote_removal_modal_ ui::tests::hierarchy_copy_contract app::tests::remove_project_ app::tests::confirm_remove_ …` (full filter list: 21-06 line 312); `cargo clippy -p baude --all-targets -- -D warnings` | ❌ in-task | ⬜ pending |
| 21-06-02 | 06 | 8 | API-01, API-02 | T-21-80 to 89 | Every other daemon answer, and no answer at all | unit (TUI) | `cargo test -p baude -- app::tests::remote_remove_ app::tests::d_on_remote_row_ app::tests::sidebar_action_d_ ui::tests::remote_removal_modal_ …` (full list: 21-06 line 376) | ❌ in-task | ⬜ pending |
| 21-06-03 | 06 | 8 | API-01, API-02 | T-21-80 to 89 | Error-text register in the modal; daemon-index naming; stale Out of Scope row corrected | unit (TUI) + crate gate + doc check | `cargo test -p baude -- ui::tests::removal_text_ ui::tests::remote_removal_modal_ app::tests::remote_remove_ …` (full list: 21-06 line 428); `cargo fmt --check`; `cargo clippy -p baude --all-targets -- -D warnings`; `awk` count on `.planning/REQUIREMENTS.md` (stale=0, corrected present; line 432) | ❌ in-task | ⬜ pending |
| 21-11-01 | 11 | 9 | API-01, API-02, API-03 | T-21-90 to 95 | Host gate and both PWA suites green over the committed final tree | phase gate | `scripts/gate.sh` (log ends `gate: all steps passed` and carries `check-web-suites: passed`); `node --test --test-reporter=tap bauded/web/tests/removal.test.js bauded/web/tests/removal-controller.test.js` (`# fail 0`, pass ≥ 20 and ≥ the floor in `/tmp/baude-21-11-pass-floor`); `node --check bauded/web/app.js` | n/a (runs existing suites) | ⬜ pending |
| 21-11-02 | 11 | 9 | API-01, API-02, API-03 | T-21-90 to 95 | Same commit green on Linux as uid 1000 | phase gate | `scripts/gate.sh --linux` (log contains `gate: all steps passed` and `check-web-suites: passed`) | n/a | ⬜ pending |
| 21-11-03 | 11 | 9 | API-01, API-02, API-03 | T-21-90 to 95 | Live two-tab PWA and TUI `D` on a remote row behave as specified | manual checkpoint (`checkpoint:human-verify`, blocking) | **No automated verify.** 17 manual steps; see Manual-Only Verifications | n/a | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements: `cargo test` and Node's built-in runner need no install, and no plan depends on a pre-seeded stub. Every test file and test name in the map is created RED-first by the task that owns it:

- `bauded/web/tests/removal.test.js` — created by 21-07 Task 1
- `scripts/check-web-suites.sh` — created by 21-07 Task 4, with its step in `scripts/gate.sh` and `.github/workflows/ci.yml`. The host needs `node` on `PATH`; CI gets Node 26 from `actions/setup-node@v7`; the `--linux` image gets it by a copy from `node:26-bookworm`
- `bauded/web/tests/removal-controller.test.js` — created by 21-05 Task 1
- Rust tests under `bauded` `api::tests::*`, `web::tests`, and `baude` `remote::tests::*`, `app::tests::*`, `ui::tests::*` — added by their own tasks

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| PWA in two tabs against a live `bauded` (steps 1–10): list shape, remove control vs. card tap, cancel by button / `Escape` / backdrop, wide modal at 390 px, remove-and-confirm drops the row in both tabs, no reappearance over 15 s, concurrent removal under an open modal, reload with a clean console | API-03, API-02 (through API-01) | Real browser DOM, viewport fit, service worker and cross-tab SSE timing are not reachable from `node --test`, which drives the controller only | 21-11 Task 3 steps 1–10 (`21-11-PLAN.md` from line 192) |
| TUI `D` on remote rows (steps 11–17): confirmation layout, `Esc` then `Enter` with a pending state, session ended elsewhere under an open modal, same row removed from the PWA first, near-simultaneous confirm from both clients, 60 s idle with no reconnect notice then an SSE-driven drop, local row still shows the Phase 20 modal, clean quit | API-01, API-02 | Live terminal rendering, real daemon timing and two-client races | 21-11 Task 3 steps 11–17. Resume signal: "approved" if all 17 steps passed, otherwise the failed or skipped step numbers with what was seen |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies — 32 of 33 do; 21-11 Task 3 is a blocking human checkpoint with no automated verify
- [x] Sampling continuity: no 3 consecutive tasks without automated verify (the only task without one is the last)
- [x] Wave 0 covers all MISSING references (none; tests are created in-task)
- [x] No watch-mode flags
- [ ] Feedback latency bound — not measured, no figure claimed
- [ ] `nyquist_compliant: true` set in frontmatter — left `false`: planned coverage only, no task has run and latency is unmeasured. The PWA suites no longer sit outside `scripts/gate.sh`: 21-07 Task 4 puts them in the gate and in CI

**Approval:** pending
