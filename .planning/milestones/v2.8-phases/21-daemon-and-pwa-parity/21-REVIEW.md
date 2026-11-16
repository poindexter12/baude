---
phase: 21-daemon-and-pwa-parity
reviewed: 2026-11-14T00:16:45Z
depth: standard
files_reviewed: 15
files_reviewed_list:
  - .github/workflows/ci.yml
  - baude/src/app.rs
  - baude/src/remote.rs
  - baude/src/ui.rs
  - bauded/Cargo.toml
  - bauded/src/api.rs
  - bauded/src/manager.rs
  - bauded/src/web.rs
  - bauded/web/app.js
  - bauded/web/index.html
  - bauded/web/removal-controller.js
  - bauded/web/removal.js
  - bauded/web/sw.js
  - scripts/check-web-suites.sh
  - scripts/gate.sh
findings:
  critical: 0
  warning: 6
  info: 3
  total: 9
  necessity: 0
status: issues_found
---

# Phase 21: Code Review Report

**Reviewed:** 2026-11-14T00:16:45Z
**Depth:** standard
**Files Reviewed:** 15
**Status:** issues_found

## Summary

Reviewed the change in `09fcd33..HEAD` for the daemon removal endpoints and `GET /events`, the PWA removal modal, the TUI remote removal path, the two scripts and the CI step. No Critical finding was proven. There are six Warnings and three Info items.

What held up under the points called out for attention:

- **Input validation and scope on the new endpoints.** `POST /removals` reads only the echoed report's target and its three compared sets; every path, runtime and row acted on comes from the daemon's own state (`bauded/src/manager.rs:1123-1150`). The `Json` extractor requires a JSON content type, so a cross-site simple-form POST is refused before the handler runs. The daemon has no caller identity on any route; that predates this range and is not reported.
- **SSE.** The event is published at one exit only, after core's save returned (`bauded/src/manager.rs:1235`), and a lagged subscriber keeps its stream (`bauded/src/api.rs` `events`, `RecvError::Lagged`).
- **Stale preview.** A 409 carries the fresh report and both clients require a new confirm.
- **TUI ticket.** The 75 s wait (60 s + 15 s grace) covers the worker's worst case of one 60 s call plus one 3 s rows read, and a missing answer on an execute is an unknown outcome, never a failure, except for the case in WR-04.

Limits of this review, stated so nothing reads as covered that was not:

- `bauded/web/style.css`, `bauded/web/tests/removal-controller.test.js` and `bauded/web/tests/removal.test.js` were NOT reviewed and are left out of `files_reviewed_list`.
- The Rust test modules (most of the inserted lines) were spot-checked only. The one fixture-realism check made: the TUI's scripted daemon sends `/events` chunked, as axum does (`baude/src/remote.rs:1498`, `1695`).
- Nothing was built or run. Every finding is from reading the code; WR-06 in particular is reasoned, not measured.
- Necessity was assessed against `necessity-review.md`; no necessity finding was raised.
- The known intermittent macOS 500 on `DELETE /sessions/{id}` was not investigated and no cause was found.

## Narrative Findings (AI reviewer)

## Warnings

### WR-01: Retry button is disabled after a failed preview, so the controller's retry path is unreachable

**File:** `bauded/web/app.js:678`
**Issue:** `confirmBtn.disabled = m.phase === "loading" || m.phase === "executing" || !m.report;` disables the button whenever no report is held. A preview that fails (`openPreview` rejection, `bauded/web/removal-controller.js:311`) lands in phase `error` with `report: null`, and line 679-680 labels the button `retry`. The controller's `confirm()` handles exactly this case (`removal-controller.js:324`, "With no report held, the preview that failed is what gets asked for again"), but the button that would call it is disabled. The user sees an error and a dead `retry` button and has to cancel and tap the remove control again.
**Fix:** Disable only when there is nothing to do:
```js
confirmBtn.disabled = m.phase === "loading" || m.phase === "executing"
  || (m.phase !== "error" && !m.report);
```

### WR-02: Removal notice and clearing failures overwrite each other in a single toast

**File:** `bauded/web/removal-controller.js:213-214` (wired at `bauded/web/app.js:955`)
**Issue:** `succeed()` calls `say(notice)` and then `say(failure)` once per `clearing_failures` entry. `notify` is wired to `toast`, which replaces `$toast.textContent` and restarts one 2.6 s timer (`app.js:94-99`). With any clearing failure the `removed N rows...` notice, including an appended `saved_degraded` detail, is replaced before it can be read, and with several failures only the last is ever visible.
**Fix:** Join the lines into one notification in `succeed()` (`say([notice, ...failures].join("; "))`), or give the controller a `notify` that queues.

### WR-03: A standalone row is left recording a dead runtime when the release save fails

**File:** `bauded/src/manager.rs:2593-2600`
**Issue:** When `release_runtime(owner)` fails, the agent and shell are killed and the error is returned. For `RuntimeOwner::Standalone`, `release_runtime` restores `repository_state = before` on an uncommitted save (`manager.rs:2387-2391`), which is the state `register_runtime` left: lifecycle `Launching` with an `owned_runtime` whose processes were just killed. The launch-failure branch directly above (`manager.rs:2578-2589`) calls `recover_standalone_ownership(key, StandaloneLifecycle::Active)` for this; this branch does not. I did not find a later repair for the row and did not run the path.
**Fix:** Mirror the launch-failure branch before returning:
```rust
if let RuntimeOwner::Standalone(key) = owner {
    if let Err(recovery) = self.recover_standalone_ownership(key, StandaloneLifecycle::Active) {
        return Err(anyhow!("{error}; standalone launch cleanup failed: {recovery}"));
    }
}
return Err(error);
```

### WR-04: A committed remote removal whose 200 body cannot be read is shown as a refusal and resent on Enter

**File:** `baude/src/remote.rs:494-502`
**Issue:** In `read_reply`, a 2xx answer whose body does not parse as a `RemovalReport`, or runs past `MAX_REPORT_BYTES`, becomes `RemovalReply::Refused { status: 200, .. }` for both calls. For an execute, a 200 means the daemon committed the removal. `settle_remote_execute` (`baude/src/app.rs:4316`) treats `Refused` as "nothing happened": it reinstalls the confirmation with `unknown_outcome: false`, the legend `enter retry`, and Enter sends the same execute again. That contradicts the module's own rule a few lines below ("for an execute the outcome is not known to be a failure, so it is not called one", applied to a body read error) and the "never resent" contract on `execute_call`. The realistic trigger is version skew: the daemon runs on another host, and a newer daemon that adds a variant to `ClearingKind`, `PersistenceOutcome` or `DiskRemovalFailureKind` fails this parse on an older TUI. The resend is harmless (the daemon answers 404 or 409), but the user is told a removal that happened was refused. No test covers a 200 with an unreadable body.
**Fix:** Route both arms through the call kind, as the read-error arm already does:
```rust
Err(_) => match call {
    RemovalCall::Preview => RemovalReply::Refused { status, message: "...".into() },
    RemovalCall::Execute => call.unanswered("the daemon's answer was not a removal report".into()),
},
```
and the same for `Ok(None)`. `run_execute` then reconciles against a fresh `GET /rows` and reports `rows_gone`.

### WR-05: The TUI drops the daemon's `saved_degraded` outcome on a successful remote removal

**File:** `baude/src/app.rs:4325-4327`
**Issue:** `settle_remote_execute` closes a 200 with `Self::removal_notice(&report)`, which reports counts and `clearing_failures` only (`app.rs:4536-4567`). `report.persistence` is never read on the remote path. When the daemon answers `SavedDegraded { detail }` it has set its own `persistence_dirty` and `persistence_error` (`bauded/src/manager.rs:1223-1226`), and the PWA appends that detail to its notice (`bauded/web/removal-controller.js:209-211`). The TUI user gets a plain success line. The local path does not need the text because it sets `persistence_dirty` on the App (`app.rs:4526-4527`); the remote path has no equivalent, so the detail is simply lost. This is a parity gap between the two clients of the same endpoint.
**Fix:** Append the detail for a remote report:
```rust
RemovalReply::Report(report) => {
    let mut notice = Self::removal_notice(&report);
    if let PersistenceOutcome::SavedDegraded { detail } = &report.persistence {
        notice.push_str(&format!("; daemon state save degraded: {detail}"));
    }
    self.close_remote_removal(notice);
}
```

### WR-06: `POST /removals` runs the whole kill-and-delete on an async worker while holding the global manager mutex

**File:** `bauded/src/api.rs:172-177`
**Issue:** `execute_removal` is an `async fn` that takes the `std::sync::Mutex` guard and calls `manager.remove_rows` synchronously. That call stops every runtime of the subtree in turn and deletes worktree directories from disk. The client side budgets 60 s for it and says why (`baude/src/remote.rs:20-25`: "The daemon stops a subtree one runtime after another and each stop can wait 2 s on its reader"). For that whole time every other handler that calls `lock(&state)` blocks its tokio worker on the same mutex, including `GET /sessions`, `GET /rows` and `GET /info`. Two consequences follow from the code: a TUI poll (3 s `REQUEST_TIMEOUT`) fails during any removal longer than 3 s, so `apply_poll` marks the daemon offline (`ok = false`) while its own removal is in progress; and with enough pollers every worker can be parked on the mutex, which stalls the `/events` keep-alives that the TUI uses to decide the stream is dead (`EVENTS_SILENCE`, 35 s). `delete_session` follows the same pattern, so this is the existing convention, but its hold time is one runtime, not a subtree plus disk deletion. Not measured.
**Fix:** At minimum move the body off the async workers so they are not consumed by the wait:
```rust
let result = tokio::task::spawn_blocking(move || {
    let mut manager = lock(&state);
    // existing match on manager.remove_rows(&previewed)
}).await;
```
Readers still queue on the mutex for the duration. Removing that needs the kill and disk stages to run without the guard, with the repository reservation (already taken at `manager.rs:1124-1131`) as the exclusion.

## Info

### IN-01: Stale `#[allow(dead_code)]` from the 21-04 staging hides production code that 21-06 never wired

**File:** `baude/src/remote.rs:24`, `236`, `382`, `941`, `951`, `956`, `1096`, `1117`, `1125`, `1132`, `1140`, `1157`
**Issue:** The attributes carry comments such as "wired by 21-06" and "quoted by the pending modal in 21-06". 21-06 has landed, so most are now no-ops (`REMOVAL_TIMEOUT`, `RemovalReply`, `RemovalTicket`, `rows`, the two `spawn_*` methods). The rest still suppress real warnings: `RemoteRows::repository_target_for_session` (line 257), `RemotePoller::refresh_rows` (1133), `preview_removal` (1118) and `execute_removal` (1126) are called only from tests. The blanket allow on `impl RemoteRows` (236) would hide any further unused method added there.
**Fix:** Remove the attributes; gate the test-only methods with `#[cfg(test)]` or delete them.

### IN-02: `snapshot()` hides the last good state on a poisoned lock, which `lock_shared` exists to prevent

**File:** `baude/src/remote.rs:1087-1092`
**Issue:** `snapshot()` uses `self.shared.lock().map(..).unwrap_or_default()`, so a poisoned mutex yields an empty snapshot with `ok: false`. `rows()` and every writer use `lock_shared`, whose comment says "A panic elsewhere must not hide the last good state from the UI". After a panic in any thread holding the lock, the TUI would show no sessions next to a populated rows index.
**Fix:** `lock_shared(&self.shared).snapshot.clone()`.

### IN-03: Report text from the daemon reaches the TUI without the sanitizing the module says is universal

**File:** `baude/src/remote.rs:421-424` (claim), `baude/src/app.rs:4326` and `4548-4558` (use)
**Issue:** The doc comment on `sanitize_daemon_text` says every daemon-supplied string shown to the user passes through it. Only plain-text error bodies do. A `RemovalReport` from the daemon is shown as received: `clearing_failures[].error` and `.path` in the footer notice, and `paths_deleted`, `paths_kept` and `already_removed` in the confirmation (`baude/src/ui.rs:2785-2801`), with no length cap. `RemoteConfirm::row_label` strips control characters from names and caps the name but not the branch. This is not a terminal-injection hole: ratatui 0.30 drops control characters when it writes cells. It is an unbounded string and a comment that overstates the guarantee.
**Fix:** Pass report strings through `sanitize_daemon_text` where a remote report is turned into text, or narrow the comment to what is true.

---

_Reviewed: 2026-11-14T00:16:45Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
