"use strict";

// node:test suite for bauded/web/removal-controller.js. No package, no browser,
// no build step: `node --test bauded/web/tests/` runs it beside removal.test.js,
// which is what scripts/check-web-suites.sh does for the gate and for CI.
//
// The controller is driven only through what createRemovalController returns.
// Its environment is the six injected deps, and the doubles below stand in for
// them the way app.js binds them: `request` behaves like api() in app.js, which
// resolves parsed JSON on 2xx and rejects with an Error whose `message` is the
// raw response text and whose `status` is the HTTP status.
//
// Every fixture is a daemon shape: the GET /rows body as Manager::rows serves
// it in bauded/src/manager.rs, and RemovalReport as baude-core/src/removal.rs
// serialises it. RowKey and RemovalTarget are externally tagged
// ({"checkout":7}); PersistenceOutcome is internally tagged
// ({"outcome":"saved"}).

const test = require("node:test");
const assert = require("node:assert/strict");

const BaudeRemoval = require("../removal.js");

// Loaded per test, so a missing module or a missing export fails the test that
// needs it by name rather than failing the file before any test is reported.
function controllerModule() {
  return require("../removal-controller.js");
}

const HOME = "/Users/joe";
const REPO_PATH = `${HOME}/code/repo`;
const WORKTREES = `${HOME}/.local/share/baude/worktrees/claude/repository-1`;

// ---- fixture factories: the shapes GET /rows serves ----

// The repository's own main checkout: named for the folder, not managed.
function mainCheckout(overrides = {}) {
  return {
    checkout_key: 5,
    repository_key: 1,
    display_name: "repo",
    branch: "main",
    path: REPO_PATH,
    role: "main",
    managed_by_baude: false,
    archived: false,
    session_id: 2,
    ...overrides,
  };
}

// A worktree baude created: display name is `repo:topic`.
function worktreeCheckout(overrides = {}) {
  return {
    checkout_key: 7,
    repository_key: 1,
    display_name: "repo:topic",
    branch: "topic",
    path: `${WORKTREES}/topic-7`,
    role: "managed_branch",
    managed_by_baude: true,
    archived: false,
    session_id: 3,
    ...overrides,
  };
}

function repositoryRow(overrides = {}) {
  return {
    repository_key: 1,
    display_name: "repo",
    path: REPO_PATH,
    health: { status: "available" },
    checkouts: [mainCheckout(), worktreeCheckout()],
    ...overrides,
  };
}

function standaloneRow(overrides = {}) {
  return {
    standalone_key: 2,
    display_name: "notes",
    path: `${HOME}/notes`,
    archived: false,
    session_id: 4,
    ...overrides,
  };
}

// A GET /rows body: one object, two arrays.
function rowsBody(overrides = {}) {
  return {
    repositories: [repositoryRow()],
    standalone: [standaloneRow()],
    ...overrides,
  };
}

// The POST /removals/preview answer for checkout 7: planned, nothing done yet.
function previewReport(overrides = {}) {
  return {
    target: { checkout: 7 },
    planned: true,
    rows_removed: [{ checkout: 7 }],
    runtimes_killed: [{ row: { checkout: 7 }, agent_pid: 4101, shell_pid: 4102 }],
    paths_deleted: [`${WORKTREES}/topic-7`],
    paths_kept: [REPO_PATH],
    already_removed: [],
    readers_parked: [],
    persistence: { outcome: "planned" },
    cleared: [],
    clearing_failures: [],
    ...overrides,
  };
}

// The POST /removals 200 for the same target: executed and saved.
function executedReport(overrides = {}) {
  return previewReport({
    planned: false,
    persistence: { outcome: "saved" },
    ...overrides,
  });
}

// ---- doubles ----

// The request double records every call and leaves it pending until the test
// answers it, so a test decides the order in which responses land.
function harness() {
  const calls = [];
  const notices = [];
  const gone = [];
  const sources = [];
  // One entry per onChange: how many requests had been made by then and what
  // the confirmation looked like, so a test can assert on order.
  const changes = [];
  const counters = { changes: 0, clock: 1700000000000 };
  let controller = null;

  const deps = {
    removal: BaudeRemoval,
    request(path, opts = {}) {
      const call = { path, method: opts.method || "GET", opts, settled: false };
      call.promise = new Promise((resolve, reject) => {
        call.resolve = (value) => { call.settled = true; resolve(value); };
        call.reject = (error) => { call.settled = true; reject(error); };
      });
      calls.push(call);
      return call.promise;
    },
    openEvents(url) {
      // The parts of an EventSource the page may touch. `emit` is the test's
      // side: it delivers one event to the listeners registered for its name,
      // which is how a browser dispatches a named SSE event. A named event
      // never reaches onmessage, so the slot exists only to be found unset.
      const source = {
        url, listeners: {}, onmessage: null, onopen: null, onerror: null, closed: false, readyState: 1,
      };
      source.addEventListener = (name, handler) => {
        (source.listeners[name] = source.listeners[name] || []).push(handler);
      };
      source.close = () => { source.closed = true; source.readyState = 2; };
      source.emit = (name, data) => {
        for (const handler of source.listeners[name] || []) handler({ type: name, data });
      };
      sources.push(source);
      return source;
    },
    now: () => counters.clock,
    onChange: () => {
      counters.changes += 1;
      const modal = controller ? controller.getModal() : null;
      changes.push({
        calls: calls.length,
        phase: modal ? modal.phase : null,
        html: modal ? modal.html : null,
        errorText: modal ? modal.errorText : null,
      });
    },
    notify: (text) => { notices.push(text); },
    onSessionGone: (sessionId) => { gone.push(sessionId); },
  };

  controller = controllerModule().createRemovalController(deps);

  // The oldest unanswered call for `method path`.
  function pending(method, path) {
    const call = calls.find((c) => !c.settled && c.method === method && c.path === path);
    assert.ok(call, `no pending ${method} ${path}; saw ${describeCalls(calls)}`);
    return call;
  }

  // Answer it 2xx with parsed JSON, then let the controller's handlers run.
  async function reply(method, path, value) {
    pending(method, path).resolve(clone(value));
    await settle();
  }

  // Answer it non-2xx the way api() in app.js rejects.
  async function fail(method, path, status, text) {
    pending(method, path).reject(Object.assign(new Error(text), { status }));
    await settle();
  }

  // Deliver a fresh GET /rows body through the controller's own loadRows.
  async function serveRows(body) {
    const loading = controller.loadRows();
    await reply("GET", "/rows", body);
    await loading;
  }

  function count(method, path) {
    return calls.filter((c) => c.method === method && c.path === path).length;
  }

  // Subscribe to the daemon's event stream and hand back the source double.
  function subscribe() {
    controller.subscribe();
    return sources[sources.length - 1];
  }

  // Every phase the confirmation has been seen in.
  function phases() {
    return changes.map((change) => change.phase).filter((phase) => phase !== null);
  }

  return {
    controller, calls, notices, gone, sources, changes, counters,
    pending, reply, fail, serveRows, count, subscribe, phases,
  };
}

function describeCalls(calls) {
  return calls.map((c) => `${c.method} ${c.path}${c.settled ? "" : " (pending)"}`).join(", ") || "none";
}

// A response body is parsed from the wire, so no test and no controller state
// shares an object with a fixture.
function clone(value) {
  return JSON.parse(JSON.stringify(value));
}

// What was put on the wire, whether the caller serialised it or not.
function sentBody(call) {
  return typeof call.opts.body === "string" ? JSON.parse(call.opts.body) : call.opts.body;
}

function settle() {
  return new Promise((resolve) => setImmediate(resolve));
}

function checkoutKeys(rows) {
  return rows.repositories.flatMap((r) => r.checkouts.map((c) => c.checkout_key));
}

// ---- Task 1: one checkout row, previewed then confirmed or cancelled ----

test("happy_path_preview_then_confirm_drops_the_row_and_closes", async () => {
  const h = harness();
  await h.serveRows(rowsBody());
  assert.deepEqual(checkoutKeys(h.controller.getRows()), [5, 7]);

  const opening = h.controller.openPreview({ checkout: 7 });
  assert.equal(h.controller.getModal().phase, "loading");
  const previewCall = h.pending("POST", "/removals/preview");
  // D-01: the bare RemovalTarget is the whole body, with no wrapper object.
  assert.deepEqual(sentBody(previewCall), { checkout: 7 });

  await h.reply("POST", "/removals/preview", previewReport());
  await opening;
  const modal = h.controller.getModal();
  assert.equal(modal.phase, "preview");
  assert.equal(typeof modal.html, "string");
  assert.notEqual(modal.html, "");
  assert.ok(modal.html.includes("repo:topic"), "the preview names the row from /rows");
  assert.deepEqual(modal.target, { checkout: 7 });

  const confirming = h.controller.confirm();
  assert.equal(h.count("POST", "/removals"), 1);
  await h.reply("POST", "/removals", executedReport());
  await confirming;

  assert.equal(h.controller.getModal(), null);
  assert.deepEqual(checkoutKeys(h.controller.getRows()), [5]);
  assert.equal(h.controller.getRows().standalone.length, 1);
  assert.equal(h.notices.length, 1);
  assert.equal(h.notices[0], "removed 1 row, killed 1 runtime, deleted 1 path");
});

test("confirm_posts_the_previewed_report_verbatim", async () => {
  const h = harness();
  await h.serveRows(rowsBody());
  // A report with content in every optional array, so "verbatim" is a claim
  // about the whole object and not about three fields.
  const served = previewReport({
    already_removed: [`${WORKTREES}/gone-9`],
    cleared: [{ kind: "suppression_added", path: REPO_PATH }],
  });

  const opening = h.controller.openPreview({ checkout: 7 });
  await h.reply("POST", "/removals/preview", served);
  await opening;
  h.controller.confirm();

  assert.deepEqual(sentBody(h.pending("POST", "/removals")), served);
});

test("load_rows_passes_every_body_through_apply_snapshot", async () => {
  const h = harness();
  await h.serveRows(rowsBody());
  const opening = h.controller.openPreview({ checkout: 7 });
  await h.reply("POST", "/removals/preview", previewReport());
  await opening;
  const confirming = h.controller.confirm();
  await h.reply("POST", "/removals", executedReport());
  await confirming;

  // A later poll whose body was built before the removal still lists the row.
  h.counters.clock += 3000;
  await h.serveRows(rowsBody());

  assert.deepEqual(checkoutKeys(h.controller.getRows()), [5]);
  // The same body handed straight to applyRows takes the same single path.
  h.controller.applyRows(rowsBody());
  assert.deepEqual(checkoutKeys(h.controller.getRows()), [5]);
});

test("no_state_change_before_the_daemon_answers", async () => {
  const h = harness();
  await h.serveRows(rowsBody());
  const before = clone(h.controller.getRows());
  const opening = h.controller.openPreview({ checkout: 7 });
  await h.reply("POST", "/removals/preview", previewReport());
  await opening;

  const confirming = h.controller.confirm();
  await settle();

  // The request is with the daemon and nothing has come back: no optimistic
  // drop, no notice, and the confirmation is still up.
  assert.equal(h.pending("POST", "/removals").settled, false);
  assert.deepEqual(h.controller.getRows(), before);
  assert.equal(h.controller.getModal().phase, "executing");
  assert.deepEqual(h.notices, []);

  await h.reply("POST", "/removals", executedReport());
  await confirming;
  assert.deepEqual(checkoutKeys(h.controller.getRows()), [5]);
});

test("cancel_while_idle_closes_without_mutating", async () => {
  const h = harness();
  await h.serveRows(rowsBody());

  // No modal open: cancel is harmless.
  const callsBefore = h.calls.length;
  assert.doesNotThrow(() => h.controller.cancel());
  assert.equal(h.calls.length, callsBefore);
  assert.deepEqual(h.notices, []);
  assert.equal(h.controller.getModal(), null);

  // A preview is on screen for checkout 7.
  const before = clone(h.controller.getRows());
  const opening = h.controller.openPreview({ checkout: 7 });
  await h.reply("POST", "/removals/preview", previewReport());
  await opening;
  assert.equal(h.controller.getModal().phase, "preview");
  const callsAtPreview = h.calls.length;
  const changesAtPreview = h.counters.changes;

  h.controller.cancel();
  await settle();

  assert.equal(h.controller.getModal(), null);
  assert.equal(h.counters.changes, changesAtPreview + 1, "cancel announces the close");
  assert.deepEqual(h.controller.getRows(), before);
  assert.equal(h.calls.length, callsAtPreview, "cancel sends no request");
  assert.equal(h.count("POST", "/removals"), 0);
  assert.deepEqual(h.notices, []);

  // No tombstone was recorded: a later poll that lists checkout 7 shows it.
  h.counters.clock += 3000;
  await h.serveRows(rowsBody());
  assert.deepEqual(checkoutKeys(h.controller.getRows()), [5, 7]);
});

test("cancel_while_executing_is_a_noop", async () => {
  const h = harness();
  await h.serveRows(rowsBody());
  const opening = h.controller.openPreview({ checkout: 7 });
  await h.reply("POST", "/removals/preview", previewReport());
  await opening;
  const confirming = h.controller.confirm();
  await settle();
  const executing = h.controller.getModal();
  assert.equal(executing.phase, "executing");
  const callsWhileExecuting = h.calls.length;

  h.controller.cancel();
  await settle();

  // The request cannot be recalled, so the confirmation stays up.
  assert.equal(h.controller.getModal().phase, "executing");
  assert.equal(h.controller.getModal().seq, executing.seq);
  assert.equal(h.calls.length, callsWhileExecuting);
  assert.deepEqual(checkoutKeys(h.controller.getRows()), [5, 7]);

  await h.reply("POST", "/removals", executedReport());
  await confirming;

  assert.equal(h.controller.getModal(), null);
  assert.deepEqual(checkoutKeys(h.controller.getRows()), [5]);
  assert.equal(h.notices.length, 1);
});

// ---- Task 2: arbitration, the event stream and the failure matrix ----

// Rows loaded and the preview for `target` on screen.
async function preview(h, options = {}) {
  await h.serveRows(options.rows || rowsBody());
  const target = options.target || { checkout: 7 };
  const opening = h.controller.openPreview(target);
  await h.reply("POST", "/removals/preview", options.report || previewReport());
  await opening;
  assert.equal(h.controller.getModal().phase, "preview");
}

// ...and Confirm tapped: POST /removals is with the daemon, unanswered.
async function execute(h, options = {}) {
  await preview(h, options);
  h.confirming = h.controller.confirm();
  await settle();
  assert.equal(h.controller.getModal().phase, "executing");
}

// One SSE frame's data line, as the daemon writes it.
function frame(report) {
  return JSON.stringify(report);
}

async function emit(source, name, data) {
  source.emit(name, data);
  await settle();
}

// A repository whose worktree is named `zeta` on branch `feat/zeta`.
function zetaRows(checkoutOverrides = {}) {
  return rowsBody({
    repositories: [
      repositoryRow({
        checkouts: [
          mainCheckout(),
          worktreeCheckout({ display_name: "zeta", branch: "feat/zeta", ...checkoutOverrides }),
        ],
      }),
    ],
  });
}

// The preview of removing repository 1 whole: its children, then the parent.
function repositoryReport(overrides = {}) {
  return previewReport({
    target: { repository: 1 },
    rows_removed: [{ checkout: 5 }, { checkout: 7 }, { repository: 1 }],
    runtimes_killed: [
      { row: { checkout: 5 }, agent_pid: 4001, shell_pid: null },
      { row: { checkout: 7 }, agent_pid: 4101, shell_pid: 4102 },
    ],
    ...overrides,
  });
}

// GET /rows after another client added a worktree to repository 1.
function rowsWithNewChild() {
  return rowsBody({
    repositories: [
      repositoryRow({
        checkouts: [
          mainCheckout(),
          worktreeCheckout(),
          worktreeCheckout({
            checkout_key: 9,
            display_name: "repo:hotfix",
            branch: "hotfix",
            path: `${WORKTREES}/hotfix-9`,
            session_id: 6,
          }),
        ],
      }),
    ],
  });
}

// The 409 body for that drift: a fresh preview report that now lists child 9.
function driftReport() {
  return repositoryReport({
    rows_removed: [{ checkout: 5 }, { checkout: 7 }, { checkout: 9 }, { repository: 1 }],
    paths_deleted: [`${WORKTREES}/topic-7`, `${WORKTREES}/hotfix-9`],
  });
}

const SURVIVED = "row Checkout(CheckoutKey(7)): runtime survived the kill stage: still running";
const BLOCKED = "daemon persistence is blocked after a state load failure";

test("pending_poll_cannot_resurrect_a_removed_row", async () => {
  // Evidence one: the 200 from POST /removals.
  {
    const h = harness();
    await execute(h);
    const poll = h.controller.loadRows(); // leaves before the removal lands
    const pollCall = h.pending("GET", "/rows");

    await h.reply("POST", "/removals", executedReport());
    await h.confirming;
    assert.deepEqual(checkoutKeys(h.controller.getRows()), [5]);

    pollCall.resolve(clone(rowsBody())); // built before the removal: lists 7
    await poll;
    assert.deepEqual(checkoutKeys(h.controller.getRows()), [5]);
  }
  // Evidence two: a rows_removed frame, with no modal open in this tab.
  {
    const h = harness();
    const source = h.subscribe();
    await h.serveRows(rowsBody());
    const poll = h.controller.loadRows();
    const pollCall = h.pending("GET", "/rows");

    await emit(source, "rows_removed", frame(executedReport()));
    assert.deepEqual(checkoutKeys(h.controller.getRows()), [5]);

    pollCall.resolve(clone(rowsBody()));
    await poll;
    assert.deepEqual(checkoutKeys(h.controller.getRows()), [5]);
  }
});

test("events_subscribe_uses_named_listener_only", () => {
  const h = harness();
  const source = h.subscribe();

  assert.equal(h.sources.length, 1);
  assert.equal(source.url, "/events");
  assert.equal(source.listeners.rows_removed.length, 1);
  // A named event never reaches onmessage, so a handler there would be dead.
  assert.equal(source.onmessage, null);
  assert.equal(source.listeners.message, undefined);
});

test("event_drops_rows_without_a_fetch", async () => {
  const h = harness();
  const source = h.subscribe();
  await h.serveRows(rowsBody());
  const callsBefore = h.calls.length;

  await emit(source, "rows_removed", frame(executedReport()));

  assert.deepEqual(checkoutKeys(h.controller.getRows()), [5]);
  assert.equal(h.calls.length, callsBefore, "one frame is enough; nothing is fetched");
  assert.deepEqual(h.gone, [3], "the removed row's live session is reported gone");
  assert.deepEqual(h.notices, [], "no confirmation was open, so there is nothing to say");

  // A repository removal arrives as one frame: children, then the parent.
  await emit(source, "rows_removed", frame(repositoryReport({
    planned: false, persistence: { outcome: "saved" },
  })));
  assert.deepEqual(h.controller.getRows().repositories, []);
  assert.equal(h.controller.getRows().standalone.length, 1);
  assert.equal(h.calls.length, callsBefore);
});

test("own_event_before_post_response_is_not_removed_elsewhere", async () => {
  const h = harness();
  const source = h.subscribe();
  await execute(h);

  // No ordering is promised between the frame and the POST response.
  await emit(source, "rows_removed", frame(executedReport()));

  assert.equal(h.controller.getModal().phase, "executing");
  assert.deepEqual(h.notices, []);

  await h.reply("POST", "/removals", executedReport());
  await h.confirming;

  assert.equal(h.controller.getModal(), null);
  assert.deepEqual(h.notices, ["removed 1 row, killed 1 runtime, deleted 1 path"]);
  assert.ok(!h.notices.some((text) => text.includes("removed elsewhere")));
});

test("event_for_open_preview_target_closes_as_removed_elsewhere", async () => {
  // The confirmation is on screen and another client removes the row.
  {
    const h = harness();
    const source = h.subscribe();
    await preview(h);

    await emit(source, "rows_removed", frame(executedReport()));

    assert.equal(h.controller.getModal(), null);
    assert.deepEqual(h.notices, ["removed elsewhere"]);
    assert.equal(h.count("POST", "/removals"), 0);
    assert.deepEqual(checkoutKeys(h.controller.getRows()), [5]);
  }
  // The same in the error phase, after a failed attempt of our own.
  {
    const h = harness();
    const source = h.subscribe();
    await execute(h);
    await h.fail("POST", "/removals", 500, "failed to terminate exact PTY process group");
    await h.confirming;
    assert.equal(h.controller.getModal().phase, "error");

    await emit(source, "rows_removed", frame(executedReport()));

    assert.equal(h.controller.getModal(), null);
    assert.deepEqual(h.notices, ["removed elsewhere"]);
    assert.equal(h.count("POST", "/removals"), 1, "no second POST was sent");
  }
  // A frame about some other row leaves the confirmation alone.
  {
    const h = harness();
    const source = h.subscribe();
    await preview(h);

    await emit(source, "rows_removed", frame(executedReport({
      target: { standalone: 2 }, rows_removed: [{ standalone: 2 }], runtimes_killed: [],
    })));

    assert.equal(h.controller.getModal().phase, "preview");
    assert.deepEqual(h.notices, []);
  }
});

test("event_then_network_error_settles_as_success", async () => {
  const h = harness();
  const source = h.subscribe();
  await execute(h);

  await emit(source, "rows_removed", frame(executedReport()));
  // fetch itself failed: api() in app.js rejects with a TypeError, no status.
  h.pending("POST", "/removals").reject(new TypeError("Failed to fetch"));
  await settle();
  await h.confirming;

  assert.equal(h.controller.getModal(), null);
  assert.ok(!h.phases().includes("error"));
  assert.deepEqual(h.notices, ["removed 1 row, killed 1 runtime, deleted 1 path"]);
  assert.deepEqual(checkoutKeys(h.controller.getRows()), [5]);
});

test("conflict_refetches_rows_before_rerender_and_names_new_child", async () => {
  const h = harness();
  await execute(h, { target: { repository: 1 }, report: repositoryReport() });
  const htmlBefore = h.controller.getModal().html;
  assert.ok(!htmlBefore.includes("repo:hotfix"));
  assert.ok(!JSON.stringify(rowsBody()).includes("repo:hotfix"));
  const rowsCallsBefore = h.count("GET", "/rows");

  await h.fail("POST", "/removals", 409, JSON.stringify(driftReport()));

  // The refetch is with the daemon and the confirmation has not been redrawn.
  assert.equal(h.count("GET", "/rows"), rowsCallsBefore + 1);
  const refetch = h.pending("GET", "/rows");
  const refetchIndex = h.calls.indexOf(refetch);
  assert.ok(refetchIndex > h.calls.findIndex((c) => c.path === "/removals"));
  assert.equal(h.controller.getModal().html, htmlBefore);
  assert.notEqual(h.controller.getModal().phase, "preview");
  // Confirm is not available while the fresh report is being prepared.
  h.controller.confirm();
  assert.equal(h.count("POST", "/removals"), 1);

  await h.reply("GET", "/rows", rowsWithNewChild());
  await h.confirming;

  const modal = h.controller.getModal();
  assert.equal(modal.phase, "preview");
  assert.equal(modal.drift, true);
  assert.ok(modal.html.includes("repo:hotfix"), "the new child is named from the fresh /rows");
  assert.ok(modal.html.includes("hotfix"));
  assert.deepEqual(modal.report, driftReport());
  // On recorded order: the html first changed only after the refetch call.
  const firstRedraw = h.changes.find((change) => change.html !== null
    && change.html !== htmlBefore && change.html !== "");
  assert.ok(firstRedraw, "the confirmation was redrawn");
  assert.ok(firstRedraw.calls > refetchIndex, "GET /rows was recorded before the html changed");
  // A conflict removes nothing.
  assert.deepEqual(checkoutKeys(h.controller.getRows()), [5, 7, 9]);
  assert.deepEqual(h.notices, []);
  assert.deepEqual(h.gone, []);

  // The re-render never confirms on the user's behalf: one POST so far, and
  // an explicit Confirm sends the fresh report.
  assert.equal(h.count("POST", "/removals"), 1);
  h.controller.confirm();
  assert.equal(h.count("POST", "/removals"), 2);
  assert.deepEqual(sentBody(h.pending("POST", "/removals")), driftReport());
});

test("conflict_refetch_does_not_reuse_a_poll_already_in_flight", async () => {
  const h = harness();
  await execute(h, { target: { repository: 1 }, report: repositoryReport() });
  const htmlBefore = h.controller.getModal().html;
  // The 3 s poll left before the conflict; its body predates the new child.
  const poll = h.controller.loadRows();
  const stale = h.pending("GET", "/rows");

  await h.fail("POST", "/removals", 409, JSON.stringify(driftReport()));
  stale.resolve(clone(rowsBody()));
  await poll;
  await settle();

  // The stale body is not what the fresh report is rendered against.
  assert.equal(h.controller.getModal().html, htmlBefore);
  assert.notEqual(h.controller.getModal().phase, "preview");

  await h.reply("GET", "/rows", rowsWithNewChild());
  await h.confirming;

  assert.equal(h.controller.getModal().phase, "preview");
  assert.ok(h.controller.getModal().html.includes("repo:hotfix"));
});

test("conflict_with_failed_refetch_lists_unresolved_keys_by_fallback_label", async () => {
  const h = harness();
  await execute(h, { target: { repository: 1 }, report: repositoryReport() });

  await h.fail("POST", "/removals", 409, JSON.stringify(driftReport()));
  await h.fail("GET", "/rows", 503, BLOCKED);
  await h.confirming;

  const modal = h.controller.getModal();
  assert.equal(modal.phase, "preview");
  assert.equal(modal.drift, true);
  // Still listed, under the label 21-07 gives a key the index cannot name.
  assert.ok(modal.html.includes("checkout 9"));
  assert.ok(modal.html.includes("repo:topic"));
  assert.deepEqual(modal.report, driftReport());
  assert.deepEqual(checkoutKeys(h.controller.getRows()), [5, 7]);
});

test("conflict_body_that_is_not_json_becomes_error_text", async () => {
  const h = harness();
  await execute(h);
  const body = "removal preview no longer matches the saved state";

  await h.fail("POST", "/removals", 409, body);
  await h.confirming;

  const modal = h.controller.getModal();
  assert.equal(modal.phase, "error");
  assert.equal(modal.errorText, body);
  assert.deepEqual(modal.report, previewReport());
  assert.deepEqual(checkoutKeys(h.controller.getRows()), [5, 7]);
});

// A 500 or a 503 on execute: the confirmation stays, nothing is dropped,
// nothing is suppressed, and Retry sends the same report again.
async function assertServerErrorKeepsModalAndRetries(status, body) {
  const h = harness();
  await execute(h);
  const before = clone(h.controller.getRows());
  const first = sentBody(h.pending("POST", "/removals"));

  await h.fail("POST", "/removals", status, body);
  await h.confirming;

  const modal = h.controller.getModal();
  assert.equal(modal.phase, "error", `status ${status}`);
  assert.equal(modal.errorText, body);
  assert.deepEqual(h.controller.getRows(), before);
  assert.deepEqual(h.notices, []);
  assert.deepEqual(h.gone, []);
  // No tombstone: a poll that lists the row shows it.
  await h.serveRows(rowsBody());
  assert.deepEqual(checkoutKeys(h.controller.getRows()), [5, 7]);

  h.controller.confirm(); // Retry
  await settle();
  assert.equal(h.count("POST", "/removals"), 2);
  assert.deepEqual(sentBody(h.pending("POST", "/removals")), first);
  assert.equal(h.controller.getModal().phase, "executing");
}

test("server_error_keeps_modal_open_and_retry_reposts_same_report", async () => {
  await assertServerErrorKeepsModalAndRetries(500, "failed to terminate exact PTY process group");
  await assertServerErrorKeepsModalAndRetries(503, "could not persist the removed row set: disk full");
});

test("unknown_target_closes_drops_the_row_and_offers_no_retry", async () => {
  const gone = "removal target Checkout(CheckoutKey(7)) is not in the saved state";
  // On the execute request.
  {
    const h = harness();
    await execute(h);
    const rowsCallsBefore = h.count("GET", "/rows");

    await h.fail("POST", "/removals", 404, gone);
    await h.confirming;

    assert.equal(h.controller.getModal(), null);
    assert.deepEqual(checkoutKeys(h.controller.getRows()), [5]);
    assert.deepEqual(h.notices, ["already removed"]);
    assert.ok(!h.phases().includes("error"));
    assert.equal(h.count("GET", "/rows"), rowsCallsBefore + 1, "the list converges from the daemon");
    // Nothing to retry: Confirm now sends nothing.
    h.controller.confirm();
    assert.equal(h.count("POST", "/removals"), 1);
    // Tombstoned: a body that still lists the row cannot bring it back.
    await h.reply("GET", "/rows", rowsBody());
    assert.deepEqual(checkoutKeys(h.controller.getRows()), [5]);
  }
  // On the preview request.
  {
    const h = harness();
    await h.serveRows(rowsBody());
    const rowsCallsBefore = h.count("GET", "/rows");
    const opening = h.controller.openPreview({ checkout: 7 });

    await h.fail("POST", "/removals/preview", 404, gone);
    await opening;

    assert.equal(h.controller.getModal(), null);
    assert.deepEqual(checkoutKeys(h.controller.getRows()), [5]);
    assert.deepEqual(h.notices, ["already removed"]);
    assert.ok(!h.phases().includes("error"));
    assert.equal(h.count("GET", "/rows"), rowsCallsBefore + 1);
    assert.equal(h.count("POST", "/removals"), 0);
    await h.reply("GET", "/rows", rowsBody());
    assert.deepEqual(checkoutKeys(h.controller.getRows()), [5]);
  }
});

test("stale_poll_after_unknown_target_cannot_resurrect_the_row", async () => {
  const h = harness();
  await execute(h);
  const poll = h.controller.loadRows(); // requested before the 404
  const stale = h.pending("GET", "/rows");

  await h.fail("POST", "/removals", 404, "removal target Checkout(CheckoutKey(7)) is not in the saved state");
  await h.confirming;
  assert.deepEqual(checkoutKeys(h.controller.getRows()), [5]);

  stale.resolve(clone(rowsBody())); // still lists checkout 7
  await poll;
  await settle();

  assert.deepEqual(checkoutKeys(h.controller.getRows()), [5]);
});

test("busy_503_keeps_modal_open_with_retry", async () => {
  const h = harness();
  await execute(h);
  const before = clone(h.controller.getRows());
  const body = `repository busy: ${REPO_PATH}`;

  await h.fail("POST", "/removals", 503, body);
  await h.confirming;

  const modal = h.controller.getModal();
  assert.equal(modal.phase, "error");
  assert.ok(modal.errorText.startsWith("repository busy:"));
  assert.ok(modal.errorText.includes(REPO_PATH));
  assert.equal(modal.errorText, body, "no key token, so nothing was substituted");
  assert.deepEqual(h.controller.getRows(), before);

  h.controller.confirm(); // Retry
  await settle();
  assert.equal(h.count("POST", "/removals"), 2);
  assert.deepEqual(sentBody(h.pending("POST", "/removals")), previewReport());
});

// errorText after POST /removals answers `status` with `body`.
async function executeErrorText(rows, status, body) {
  const h = harness();
  await execute(h, { rows });
  await h.fail("POST", "/removals", status, body);
  await h.confirming;
  assert.equal(h.controller.getModal().phase, "error");
  return h.controller.getModal().errorText;
}

test("error_text_names_a_known_row_by_label", async () => {
  assert.equal(
    await executeErrorText(zetaRows(), 500, SURVIVED),
    "zeta (feat/zeta): runtime survived the kill stage: still running",
  );

  // The same body on a rejected preview request.
  {
    const h = harness();
    await h.serveRows(zetaRows());
    const opening = h.controller.openPreview({ checkout: 7 });
    await h.fail("POST", "/removals/preview", 500, SURVIVED);
    await opening;
    assert.equal(h.controller.getModal().phase, "error");
    assert.equal(
      h.controller.getModal().errorText,
      "zeta (feat/zeta): runtime survived the kill stage: still running",
    );
    // No report is held, so Retry asks for the preview again.
    h.controller.confirm();
    await settle();
    assert.equal(h.count("POST", "/removals/preview"), 2);
    assert.equal(h.count("POST", "/removals"), 0);
    assert.deepEqual(sentBody(h.pending("POST", "/removals/preview")), { checkout: 7 });
  }

  // A path in the text is the daemon's, byte for byte, with no tilde.
  const path = "/home/dev/.baude/worktrees/zeta";
  const withPath = await executeErrorText(
    zetaRows(), 500, `row Checkout(CheckoutKey(7)): could not remove ${path}: Permission denied`,
  );
  assert.equal(withPath, `zeta (feat/zeta): could not remove ${path}: Permission denied`);
  assert.ok(!withPath.includes("~"));

  // A checkout with no branch is its display name alone.
  assert.equal(
    await executeErrorText(zetaRows({ branch: null }), 500, SURVIVED),
    "zeta: runtime survived the kill stage: still running",
  );

  // A label is literal text: `$&` in a name is not a replacement pattern.
  assert.equal(
    await executeErrorText(zetaRows({ display_name: "ze$&ta", branch: "feat/$1" }), 500, SURVIVED),
    "ze$&ta (feat/$1): runtime survived the kill stage: still running",
  );
});

test("error_text_reads_unknown_row_for_a_key_the_index_cannot_name", async () => {
  assert.equal(
    await executeErrorText(
      zetaRows(), 500, "row Checkout(CheckoutKey(99)): runtime survived the kill stage: still running",
    ),
    "unknown row: runtime survived the kill stage: still running",
  );

  const both = await executeErrorText(
    zetaRows(),
    500,
    "row Repository(RepositoryKey(41)) has a persisted runtime but no live handle was supplied; "
      + "removal target Standalone(StandaloneKey(42)) is not in the saved state",
  );
  assert.equal(
    both,
    "unknown row has a persisted runtime but no live handle was supplied; "
      + "removal target unknown row is not in the saved state",
  );
  assert.ok(!both.includes("Key("));

  // A number too large to be a key is not looked up at all.
  const huge = await executeErrorText(
    zetaRows(), 500, "row Checkout(CheckoutKey(99999999999999999999)): runtime survived the kill stage: x",
  );
  assert.equal(huge, "unknown row: runtime survived the kill stage: x");
});

test("error_text_without_a_key_token_is_unchanged", async () => {
  const bodies = [
    "could not persist the removed row set: disk full",
    "removed state failed validation: missing checkout (stale)",
    `${WORKTREES}/topic-7`,
  ];
  for (const body of bodies) {
    assert.equal(await executeErrorText(zetaRows(), 500, body), body);
  }
});

test("notice_text_names_a_row_removed_by_the_same_response", async () => {
  const h = harness();
  await execute(h, { rows: zetaRows() });
  // Harsher than core emits today: a clearing failure whose error text names
  // the very row this response removes, in Rust debug form.
  const report = executedReport({
    clearing_failures: [{
      kind: "marker",
      path: `${WORKTREES}/topic-7`,
      error: "row Checkout(CheckoutKey(7)): Permission denied (os error 13)",
    }],
  });

  await h.reply("POST", "/removals", report);
  await h.confirming;

  assert.equal(h.notices.length, 2);
  assert.equal(h.notices[0], "removed 1 row, killed 1 runtime, deleted 1 path");
  // Only holds if the index was taken before the row was dropped.
  assert.ok(h.notices[1].includes("zeta (feat/zeta)"), h.notices[1]);
  assert.ok(h.notices[1].includes(`${WORKTREES}/topic-7`));
  for (const text of h.notices) assert.ok(!text.includes("Key("), text);
});

test("saved_degraded_200_drops_rows_and_shows_a_notice", async () => {
  const h = harness();
  await execute(h);
  const detail = "state file replaced, but syncing its directory failed: Input/output error (os error 5)";

  await h.reply("POST", "/removals", executedReport({
    persistence: { outcome: "saved_degraded", detail },
  }));
  await h.confirming;

  // It is a success with a caveat, not an error.
  assert.equal(h.controller.getModal(), null);
  assert.deepEqual(checkoutKeys(h.controller.getRows()), [5]);
  assert.equal(h.notices.length, 1);
  assert.ok(h.notices[0].startsWith("removed 1 row, killed 1 runtime, deleted 1 path"));
  assert.ok(h.notices[0].includes(detail));
  assert.ok(!h.phases().includes("error"));
  assert.ok(h.changes.every((change) => !change.errorText));
  // No retry: Confirm sends nothing more.
  h.controller.confirm();
  assert.equal(h.count("POST", "/removals"), 1);
  // Tombstoned like any other removal.
  await h.serveRows(rowsBody());
  assert.deepEqual(checkoutKeys(h.controller.getRows()), [5]);
});

test("blocked_503_on_execute_keeps_row_and_modal_open", async () => {
  const h = harness();
  // Previewed, then the daemon restarted into blocked persistence. A blocked
  // daemon answers 503 before it looks the target up.
  await execute(h);
  const first = sentBody(h.pending("POST", "/removals"));

  await h.fail("POST", "/removals", 503, BLOCKED);
  await h.confirming;

  const modal = h.controller.getModal();
  assert.equal(modal.phase, "error");
  assert.equal(modal.errorText, BLOCKED);
  assert.deepEqual(checkoutKeys(h.controller.getRows()), [5, 7]);
  assert.ok(!h.notices.includes("already removed"));
  assert.deepEqual(h.notices, []);
  // No tombstone: the row is still listed after the next poll.
  await h.serveRows(rowsBody());
  assert.deepEqual(checkoutKeys(h.controller.getRows()), [5, 7]);

  h.controller.confirm(); // Retry
  await settle();
  assert.equal(h.count("POST", "/removals"), 2);
  assert.deepEqual(sentBody(h.pending("POST", "/removals")), first);
});

test("retry_after_503_that_answers_409_rerenders_the_fresh_report", async () => {
  const h = harness();
  await execute(h);
  // The save failed after the worktree was deleted from disk.
  await h.fail("POST", "/removals", 503, "could not persist the removed row set: disk full");
  await h.confirming;
  assert.equal(h.controller.getModal().phase, "error");

  const retrying = h.controller.confirm();
  await settle();
  const fresh = previewReport({
    paths_deleted: [],
    already_removed: [`${WORKTREES}/topic-7`],
  });
  await h.fail("POST", "/removals", 409, JSON.stringify(fresh));
  await h.reply("GET", "/rows", rowsBody());
  await retrying;

  const modal = h.controller.getModal();
  assert.equal(modal.phase, "preview");
  assert.equal(modal.drift, true);
  assert.deepEqual(modal.report, fresh);
  assert.ok(modal.html.includes(`${WORKTREES}/topic-7`));
  assert.ok(modal.html.includes("already removed"));
  assert.ok(!modal.errorText);
  assert.deepEqual(checkoutKeys(h.controller.getRows()), [5, 7]);

  // A new explicit Confirm posts the fresh report, not the first one.
  assert.equal(h.count("POST", "/removals"), 2);
  h.controller.confirm();
  await settle();
  assert.equal(h.count("POST", "/removals"), 3);
  assert.deepEqual(sentBody(h.pending("POST", "/removals")), fresh);
});

test("error_bodies_other_than_conflict_are_never_parsed", async () => {
  const h = harness();
  await execute(h);
  const rowsCallsBefore = h.count("GET", "/rows");
  // A 500 whose text happens to be a whole RemovalReport in JSON.
  const body = JSON.stringify(previewReport({ rows_removed: [{ checkout: 5 }, { checkout: 7 }] }));

  await h.fail("POST", "/removals", 500, body);
  await h.confirming;

  const modal = h.controller.getModal();
  assert.equal(modal.phase, "error");
  assert.equal(modal.errorText, body);
  assert.equal(modal.drift, false);
  assert.deepEqual(modal.report, previewReport(), "the held report is still the one on screen");
  assert.equal(h.count("GET", "/rows"), rowsCallsBefore, "no drift refetch was started");
  assert.deepEqual(checkoutKeys(h.controller.getRows()), [5, 7]);
});

test("double_confirm_sends_one_post", async () => {
  const h = harness();
  await preview(h);

  // Two taps in the same tick.
  const first = h.controller.confirm();
  const second = h.controller.confirm();
  await settle();
  assert.equal(h.count("POST", "/removals"), 1);

  await h.reply("POST", "/removals", executedReport());
  await Promise.all([first, second]);
  assert.equal(h.count("POST", "/removals"), 1);
  assert.equal(h.notices.length, 1);
  assert.deepEqual(checkoutKeys(h.controller.getRows()), [5]);
});

test("late_preview_response_after_cancel_is_ignored", async () => {
  const h = harness();
  await h.serveRows(rowsBody());
  const opening = h.controller.openPreview({ checkout: 7 });
  assert.equal(h.controller.getModal().phase, "loading");

  h.controller.cancel();
  assert.equal(h.controller.getModal(), null);
  await h.reply("POST", "/removals/preview", previewReport());
  await opening;

  assert.equal(h.controller.getModal(), null, "a cancelled confirmation does not reopen");
  assert.deepEqual(h.notices, []);

  // Nor does a late answer land in the confirmation opened after it.
  const stale = h.controller.openPreview({ checkout: 7 });
  h.controller.cancel();
  const current = h.controller.openPreview({ checkout: 5 });
  const [staleCall, currentCall] = h.calls.filter((c) => c.path === "/removals/preview" && !c.settled);
  staleCall.resolve(clone(previewReport()));
  await stale;
  await settle();
  assert.equal(h.controller.getModal().phase, "loading");
  assert.deepEqual(h.controller.getModal().target, { checkout: 5 });
  const mainReport = previewReport({
    target: { checkout: 5 }, rows_removed: [{ checkout: 5 }], paths_deleted: [],
  });
  currentCall.resolve(clone(mainReport));
  await current;
  assert.deepEqual(h.controller.getModal().report, mainReport);
});

test("duplicate_event_and_response_change_state_once", async () => {
  // The 200, then the matching frame.
  {
    const h = harness();
    const source = h.subscribe();
    await execute(h);
    await h.reply("POST", "/removals", executedReport());
    await h.confirming;
    const afterFirst = clone(h.controller.getRows());

    await emit(source, "rows_removed", frame(executedReport()));

    assert.deepEqual(h.controller.getRows(), afterFirst);
    assert.equal(h.notices.length, 1);
    assert.deepEqual(h.gone, [3]);
  }
  // The frame, then the 200.
  {
    const h = harness();
    const source = h.subscribe();
    await execute(h);
    await emit(source, "rows_removed", frame(executedReport()));
    const afterFirst = clone(h.controller.getRows());
    assert.deepEqual(checkoutKeys(afterFirst), [5]);

    await h.reply("POST", "/removals", executedReport());
    await h.confirming;

    assert.deepEqual(h.controller.getRows(), afterFirst);
    assert.equal(h.notices.length, 1);
    assert.deepEqual(h.gone, [3]);
  }
});

test("reconnect_refetches_rows", async () => {
  const h = harness();
  const source = h.subscribe();

  await emit(source, "open");
  assert.equal(h.count("GET", "/rows"), 0, "the first open has missed nothing");

  // The stream has no replay: whatever committed during the gap was never
  // announced, so the reconnect reconciles through GET /rows.
  await emit(source, "error");
  assert.equal(h.count("GET", "/rows"), 0);
  await emit(source, "open");
  assert.equal(h.count("GET", "/rows"), 1);
  await h.reply("GET", "/rows", rowsBody());
  assert.deepEqual(checkoutKeys(h.controller.getRows()), [5, 7]);

  await emit(source, "open");
  assert.equal(h.count("GET", "/rows"), 1, "an open with no error before it fetches nothing");
});

test("malformed_event_frame_is_ignored_and_refetches", async () => {
  const h = harness();
  const source = h.subscribe();
  await h.serveRows(rowsBody());
  const rowsCallsBefore = h.count("GET", "/rows");

  await emit(source, "rows_removed", "{\"target\":{\"checkout\":7},\"rows_remo");

  assert.deepEqual(checkoutKeys(h.controller.getRows()), [5, 7]);
  assert.deepEqual(h.notices, []);
  assert.equal(h.count("GET", "/rows"), rowsCallsBefore + 1);
});

// A GET /sessions entry, with the fields SessionInfo serialises.
function sessionInfo(id, name, overrides = {}) {
  return {
    id,
    name,
    title: null,
    status: "idle",
    state_source: "hooks",
    last_tool: null,
    waiting_for_ms: null,
    waiting_reason: null,
    model: "claude-opus-4",
    permission_mode: "default",
    context_used_pct: 12,
    rate_5h_used_pct: null,
    rate_5h_resets_at_unix_s: null,
    branch: null,
    cwd: REPO_PATH,
    repo_root: REPO_PATH,
    is_worktree: false,
    archived: false,
    ...overrides,
  };
}

test("removed_row_session_is_filtered_from_sessions", async () => {
  const h = harness();
  const sessions = [
    sessionInfo(2, "repo", { branch: "main" }),
    sessionInfo(3, "repo:topic", { branch: "topic", cwd: `${WORKTREES}/topic-7`, is_worktree: true }),
    sessionInfo(4, "notes", { cwd: `${HOME}/notes`, repo_root: `${HOME}/notes` }),
  ];
  await execute(h);
  assert.deepEqual(h.controller.filterSessions(sessions), sessions);

  await h.reply("POST", "/removals", executedReport());
  await h.confirming;

  // A /sessions body fetched before the removal still carries session 3.
  assert.deepEqual(h.controller.filterSessions(sessions).map((s) => s.id), [2, 4]);
  assert.deepEqual(h.gone, [3]);
  assert.equal(sessions.length, 3, "the argument is not mutated");

  // The suppression expires, so a session the daemon keeps reporting returns.
  h.counters.clock += 60001;
  assert.deepEqual(h.controller.filterSessions(sessions).map((s) => s.id), [2, 3, 4]);
});
