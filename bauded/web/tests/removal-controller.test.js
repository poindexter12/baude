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
  const counters = { changes: 0, clock: 1700000000000 };

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
      const source = { url, listeners: {}, onmessage: null, onopen: null, onerror: null, closed: false };
      source.addEventListener = (name, handler) => {
        (source.listeners[name] = source.listeners[name] || []).push(handler);
      };
      source.close = () => { source.closed = true; };
      sources.push(source);
      return source;
    },
    now: () => counters.clock,
    onChange: () => { counters.changes += 1; },
    notify: (text) => { notices.push(text); },
    onSessionGone: (sessionId) => { gone.push(sessionId); },
  };

  const controller = controllerModule().createRemovalController(deps);

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

  return {
    controller, calls, notices, gone, sources, counters, pending, reply, fail, serveRows, count,
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
