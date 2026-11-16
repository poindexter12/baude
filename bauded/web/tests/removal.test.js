"use strict";

// node:test suite for bauded/web/removal.js. No package, no browser, no build
// step: run it with `node --test bauded/web/tests/`, which is what
// scripts/check-web-suites.sh does for the gate and for CI.
//
// Every fixture below is a daemon shape read off baude-core/src/removal.rs and
// the GET /rows projection in bauded/src/manager.rs: RowKey and RemovalTarget
// are externally tagged ({"checkout":7}), PersistenceOutcome is internally
// tagged ({"outcome":"planned"}), and the clearing arrays are `cleared` and
// `clearing_failures`.

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const BaudeRemoval = require("../removal.js");

const HOME = "/Users/joe";
const REPO_PATH = `${HOME}/code/repo`;
const WORKTREES = `${HOME}/.local/share/baude/worktrees/claude/repository-1`;

// ---- fixture factories ----

function checkoutRow(overrides = {}) {
  return {
    checkout_key: 7,
    repository_key: 1,
    display_name: "repo:topic",
    branch: "refs/heads/topic",
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
    checkouts: [
      checkoutRow(),
      checkoutRow({
        checkout_key: 8,
        display_name: "repo:other",
        branch: "refs/heads/other",
        path: `${WORKTREES}/other-8`,
        session_id: null,
      }),
    ],
    ...overrides,
  };
}

function standaloneRow(overrides = {}) {
  return {
    standalone_key: 2,
    display_name: "notes",
    path: `${HOME}/notes`,
    archived: false,
    session_id: null,
    ...overrides,
  };
}

// A GET /rows body: one object, two arrays, never a bare array.
function rowsBody(overrides = {}) {
  return {
    repositories: [repositoryRow()],
    standalone: [standaloneRow()],
    ...overrides,
  };
}

// A preview RemovalReport for checkout 7, all eleven fields present.
function checkoutReport(overrides = {}) {
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

// ---- Task 1: a daemon report becomes escaped HTML ----

test("escape_html_matches_the_app_js_table", () => {
  const { escapeHtml } = BaudeRemoval;
  assert.equal(escapeHtml("&"), "&amp;");
  assert.equal(escapeHtml("<"), "&lt;");
  assert.equal(escapeHtml(">"), "&gt;");
  assert.equal(escapeHtml('"'), "&quot;");
  assert.equal(escapeHtml("'"), "&#39;");
  assert.equal(escapeHtml("plain text 123"), "plain text 123");
  // esc() in app.js coerces through String(); so does this.
  assert.equal(escapeHtml(42), "42");
  assert.equal(escapeHtml(null), "null");
  assert.equal(escapeHtml(undefined), "undefined");
});

test("render_confirmation_escapes_every_daemon_supplied_value", () => {
  const { renderConfirmation, confirmationView, rowsIndex } = BaudeRemoval;
  const hostile = `${WORKTREES}/a<b>c"d'e&f`;
  const report = checkoutReport({ paths_deleted: [hostile] });
  const rows = rowsBody({
    repositories: [
      repositoryRow({
        checkouts: [checkoutRow({ display_name: "wt<b>", path: hostile })],
      }),
    ],
  });

  const html = renderConfirmation(confirmationView(report, rowsIndex(rows)));

  assert.equal(typeof html, "string");
  assert.ok(html.includes("&lt;b&gt;"), "an angle bracket leaves as an entity");
  assert.ok(html.includes("wt&lt;b&gt;"), "the display name is escaped");
  assert.ok(html.includes("&quot;"), "a double quote leaves as an entity");
  assert.ok(html.includes("&#39;"), "a single quote leaves as an entity");
  assert.ok(html.includes("&amp;f"), "an ampersand leaves as an entity");
  // The property at risk: no raw tag opener from daemon data reaches the DOM.
  assert.equal(html.includes("<b"), false, "raw <b must not appear");
});

test("render_confirmation_does_not_double_escape", () => {
  const { renderConfirmation, confirmationView, rowsIndex } = BaudeRemoval;

  // A raw angle bracket through the whole pipeline is escaped exactly once:
  // confirmationView hands it over raw and renderConfirmation encodes it.
  const report = checkoutReport({ paths_deleted: [`${WORKTREES}/a<b`] });
  const piped = renderConfirmation(confirmationView(report, rowsIndex(rowsBody())));
  assert.ok(piped.includes("a&lt;b"));
  assert.equal(piped.includes("&amp;lt;"), false, "the pipeline escaped twice");

  // A hand-built view whose values are already entity text is literal text to
  // the renderer: encoded once, so the ampersand is never encoded twice.
  const view = {
    title: "Remove this checkout?",
    sections: [{ label: "paths deleted", count: 1, lines: ["&lt;"] }],
  };
  const html = renderConfirmation(view);
  assert.ok(html.includes("<li>&amp;lt;</li>"), "entity text is encoded once");
  assert.equal(html.includes("&amp;amp;"), false, "an ampersand was encoded twice");
});

test("render_confirmation_rejects_a_report", () => {
  const { renderConfirmation } = BaudeRemoval;
  assert.throws(() => renderConfirmation(checkoutReport()), TypeError);
  // A report that also carries a sections array still is not a view.
  assert.throws(
    () => renderConfirmation({ ...checkoutReport(), title: "x", sections: [] }),
    TypeError,
  );
  assert.throws(() => renderConfirmation({ title: "no sections" }), TypeError);
  assert.throws(() => renderConfirmation(null), TypeError);
  assert.throws(() => renderConfirmation(undefined), TypeError);
});

test("confirmation_view_mirrors_the_tui_section_order", () => {
  const { confirmationView, rowsIndex } = BaudeRemoval;
  const view = confirmationView(checkoutReport(), rowsIndex(rowsBody()));

  assert.equal(view.title, "Remove this worktree and delete its folder?");
  assert.deepStrictEqual(
    view.sections.map((s) => s.label),
    ["rows dropped", "runtimes killed", "paths deleted", "paths kept"],
  );
  assert.deepStrictEqual(
    view.sections.map((s) => s.count),
    [1, 1, 1, 1],
  );
  assert.deepStrictEqual(view.sections[1].lines, ["repo:topic (pid 4101 shell 4102)"]);
  assert.deepStrictEqual(view.sections[2].lines, [`${WORKTREES}/topic-7`]);
  assert.deepStrictEqual(view.sections[3].lines, [REPO_PATH]);

  // The three always-present sections keep a single `none` line when empty,
  // and a null shell pid drops the shell half, as the TUI does.
  const quiet = confirmationView(
    checkoutReport({ runtimes_killed: [], paths_deleted: [], paths_kept: [] }),
    rowsIndex(rowsBody()),
  );
  assert.deepStrictEqual(
    quiet.sections.map((s) => [s.label, s.count, s.lines]),
    [
      ["rows dropped", 1, quiet.sections[0].lines],
      ["runtimes killed", 0, ["none"]],
      ["paths deleted", 0, ["none"]],
      ["paths kept", 0, ["none"]],
    ],
  );
  const noShell = confirmationView(
    checkoutReport({
      runtimes_killed: [{ row: { checkout: 7 }, agent_pid: 4101, shell_pid: null }],
    }),
    rowsIndex(rowsBody()),
  );
  assert.deepStrictEqual(noShell.sections[1].lines, ["repo:topic (pid 4101)"]);

  // `already removed` and `cleared` appear only when they have content.
  const full = confirmationView(
    checkoutReport({
      already_removed: [`${WORKTREES}/gone-9`],
      cleared: [
        { kind: "binding", path: REPO_PATH },
        { kind: "marker", path: `${WORKTREES}/topic-7` },
        { kind: "repository_dir", path: WORKTREES },
        { kind: "suppression_added", path: REPO_PATH },
      ],
    }),
    rowsIndex(rowsBody()),
  );
  assert.deepStrictEqual(
    full.sections.map((s) => s.label),
    ["rows dropped", "runtimes killed", "paths deleted", "paths kept", "already removed", "cleared"],
  );
  assert.deepStrictEqual(full.sections[5].lines, ["1 binding, 2 markers, 1 suppression entry"]);
  assert.equal(full.sections[5].count, 4);

  // The title names the target kind for all three RemovalTarget shapes. This
  // report deletes one path, so the repository title counts it.
  const titled = (target) =>
    confirmationView(checkoutReport({ target }), rowsIndex(rowsBody())).title;
  assert.equal(titled({ repository: 1 }), "Remove this repository and delete 1 folder?");
  assert.equal(titled({ standalone: 2 }), "Remove this standalone session?");
});

// repositoryReport and standaloneReport are the Task 2 factories below; a
// function declaration is hoisted, so they are in scope here.
test("confirmation_title_says_when_a_folder_is_deleted", () => {
  const { confirmationView, renderConfirmation, rowsIndex, escapeHtml } = BaudeRemoval;
  const index = rowsIndex(rowsBody());
  const title = (report) => confirmationView(report, index).title;
  // Repository 1's preview, deleting exactly `deleted`.
  const repository = (deleted, overrides = {}) =>
    repositoryReport({ paths_deleted: deleted, ...overrides });

  // A checkout whose removal deletes its folder says so.
  assert.equal(title(checkoutReport()), "Remove this worktree and delete its folder?");

  // A repository counts the folders its removal deletes.
  assert.equal(
    title(repository([`${WORKTREES}/topic-7`])),
    "Remove this repository and delete 1 folder?",
  );
  assert.equal(
    title(repository([`${WORKTREES}/topic-7`, `${WORKTREES}/other-8`, `${WORKTREES}/third-9`])),
    "Remove this repository and delete 3 folders?",
  );

  // A folder that is already gone is not deleted by this removal.
  const gone = [`${WORKTREES}/topic-7`];
  assert.equal(
    title(checkoutReport({ paths_deleted: [], already_removed: gone })),
    "Remove this checkout?",
  );
  assert.equal(
    title(repository([], { already_removed: gone })),
    "Remove this repository?",
  );

  // A standalone session keeps its title whatever paths_deleted holds.
  assert.equal(title(standaloneReport()), "Remove this standalone session?");
  assert.equal(
    title(standaloneReport({ paths_deleted: [`${HOME}/notes`] })),
    "Remove this standalone session?",
  );

  // The view carries the title raw and the renderer escapes it exactly once.
  for (const report of [checkoutReport(), repository([`${WORKTREES}/topic-7`]), standaloneReport()]) {
    const view = confirmationView(report, index);
    const html = renderConfirmation(view);
    assert.ok(
      html.startsWith(`<h3>${escapeHtml(view.title)}</h3>`),
      `heading is the once-escaped title: ${html.slice(0, 80)}`,
    );
  }
});

test("module_loads_as_a_browser_script_with_no_globals", () => {
  // The same file is a browser script tag. Run it in a context that has the
  // language built-ins and nothing else: no module loader, no host object, no
  // network, no DOM, no timers. Loading or rendering would throw if the module
  // reached for any of them.
  const source = fs.readFileSync(path.join(__dirname, "..", "removal.js"), "utf8");
  const sandbox = vm.createContext({});
  vm.runInContext(source, sandbox, { filename: "removal.js" });

  const names = vm.runInContext("Object.keys(globalThis.BaudeRemoval).join(',')", sandbox);
  assert.equal(names, Object.keys(BaudeRemoval).join(","), "one surface, both loaders");

  sandbox.report = JSON.stringify(checkoutReport({ paths_deleted: ["/x/a<b"] }));
  sandbox.rows = JSON.stringify(rowsBody());
  const html = vm.runInContext(
    "const R = globalThis.BaudeRemoval;" +
      "R.renderConfirmation(R.confirmationView(JSON.parse(report), R.rowsIndex(JSON.parse(rows))))",
    sandbox,
  );
  assert.ok(html.includes("a&lt;b"));
  assert.equal(html.includes("<b"), false);
});

// ---- Task 2: paths, the key-to-name join, dropRows, the drift body ----

// A repository-parent preview: both children, then the parent itself.
function repositoryReport(overrides = {}) {
  return checkoutReport({
    target: { repository: 1 },
    rows_removed: [{ checkout: 7 }, { checkout: 8 }, { repository: 1 }],
    paths_deleted: [`${WORKTREES}/topic-7`, `${WORKTREES}/other-8`],
    ...overrides,
  });
}

function standaloneReport(overrides = {}) {
  return checkoutReport({
    target: { standalone: 2 },
    rows_removed: [{ standalone: 2 }],
    runtimes_killed: [],
    paths_deleted: [],
    paths_kept: [`${HOME}/notes`],
    ...overrides,
  });
}

// A /rows body the client fetched before checkout 8 existed: the HIGH #6
// shape, where a fresh report names a child the page has never seen.
function rowsBodyWithoutCheckout8() {
  return rowsBody({ repositories: [repositoryRow({ checkouts: [checkoutRow()] })] });
}

test("tilde_path_mirrors_the_tui_helper", () => {
  const { tildePath } = BaudeRemoval;
  assert.equal(tildePath("/Users/joe/code/x", "/Users/joe"), "~/code/x");
  assert.equal(tildePath("/var/tmp/x", "/Users/joe"), "/var/tmp/x");
  assert.equal(tildePath("/Users/joe", "/Users/joe"), "~");
});

test("tilde_path_keeps_the_plain_prefix_quirk", () => {
  // strip_prefix in baude/src/ui.rs:1370 is a string prefix strip, not a path
  // segment comparison. The PWA agrees with it, quirk included.
  assert.equal(BaudeRemoval.tildePath("/Users/joe2/x", "/Users/joe"), "~2/x");
});

test("tilde_path_without_a_home_is_the_identity", () => {
  const { tildePath } = BaudeRemoval;
  const p = "/Users/joe/code/x";
  assert.equal(tildePath(p, "/"), p);
  assert.equal(tildePath(p, ""), p);
  assert.equal(tildePath(p, null), p);
  assert.equal(tildePath(p, undefined), p);
  assert.equal(tildePath(p), p);
});

test("confirmation_view_shortens_paths_only_when_given_a_home", () => {
  const { confirmationView, rowsIndex } = BaudeRemoval;
  const report = checkoutReport({ already_removed: [`${WORKTREES}/gone-9`] });
  const index = rowsIndex(rowsBody());
  const lines = (view, label) => view.sections.find((s) => s.label === label).lines;

  // No home: the absolute paths /rows and the report already carry.
  const absolute = confirmationView(report, index);
  assert.deepStrictEqual(lines(absolute, "paths deleted"), [`${WORKTREES}/topic-7`]);
  assert.deepStrictEqual(lines(absolute, "paths kept"), [REPO_PATH]);
  assert.deepStrictEqual(lines(absolute, "already removed"), [`${WORKTREES}/gone-9`]);

  const short = confirmationView(report, index, { home: HOME });
  assert.deepStrictEqual(lines(short, "paths deleted"), [
    "~/.local/share/baude/worktrees/claude/repository-1/topic-7",
  ]);
  assert.deepStrictEqual(lines(short, "paths kept"), ["~/code/repo"]);
  assert.deepStrictEqual(lines(short, "already removed"), [
    "~/.local/share/baude/worktrees/claude/repository-1/gone-9",
  ]);
});

test("report_row_names_resolves_every_key_through_the_index", () => {
  const { reportRowNames, confirmationView, rowsIndex } = BaudeRemoval;
  const report = repositoryReport();
  const index = rowsIndex(rowsBody());

  assert.deepStrictEqual(reportRowNames(report, index), [
    { id: "checkout:7", kind: "checkout", key: 7, label: "repo:topic", branch: "refs/heads/topic", resolved: true },
    { id: "checkout:8", kind: "checkout", key: 8, label: "repo:other", branch: "refs/heads/other", resolved: true },
    { id: "repository:1", kind: "repository", key: 1, label: "repo", branch: null, resolved: true },
  ]);

  const dropped = confirmationView(report, index).sections[0];
  assert.equal(dropped.label, "rows dropped");
  assert.strictEqual(dropped.lines.length, report.rows_removed.length);
  assert.deepStrictEqual(dropped.lines, [
    "repo:topic ⎇ refs/heads/topic",
    "repo:other ⎇ refs/heads/other",
    "repo",
  ]);
});

test("report_row_names_never_invents_a_name", () => {
  const { reportRowNames, confirmationView, rowsIndex } = BaudeRemoval;
  const report = repositoryReport();
  const index = rowsIndex(rowsBodyWithoutCheckout8());

  const names = reportRowNames(report, index);
  assert.strictEqual(names.length, 3, "an unresolved row is still listed");
  assert.deepStrictEqual(names[1], {
    id: "checkout:8", kind: "checkout", key: 8, label: "checkout 8", branch: null, resolved: false,
  });
  assert.strictEqual(names[1].resolved, false);
  assert.strictEqual(names[1].label, "checkout 8");
  assert.equal(names[0].resolved, true);
  assert.equal(names[2].resolved, true);

  const dropped = confirmationView(report, index).sections[0];
  assert.strictEqual(dropped.lines.length, report.rows_removed.length);
  assert.deepStrictEqual(dropped.lines, ["repo:topic ⎇ refs/heads/topic", "checkout 8", "repo"]);

  // The other two kinds get the same honest label, and a key the module
  // cannot read at all is still listed rather than dropped.
  const strange = reportRowNames(
    { rows_removed: [{ repository: 4 }, { standalone: 5 }, { session: 6 }, null] },
    rowsIndex(rowsBody()),
  );
  assert.deepStrictEqual(strange.map((n) => [n.label, n.resolved]), [
    ["repository 4", false],
    ["standalone 5", false],
    ["unknown row", false],
    ["unknown row", false],
  ]);
});

test("report_row_names_resolves_a_child_with_no_live_session", () => {
  const { reportRowNames, rowsIndex } = BaudeRemoval;
  const index = rowsIndex(rowsBody());
  // D-16 parity: checkout 8 has no runtime and is listed all the same.
  assert.strictEqual(index["checkout:8"].session_id, null);
  assert.strictEqual(index["checkout:7"].session_id, 3);

  const child = reportRowNames(repositoryReport(), index)[1];
  assert.equal(child.resolved, true);
  assert.equal(child.label, "repo:other");
  assert.equal(child.branch, "refs/heads/other");
});

test("drop_rows_removes_exactly_the_reported_keys", () => {
  const { dropRows } = BaudeRemoval;
  const body = rowsBody();
  const before = structuredClone(body);

  // A checkout: gone, with its parent and its sibling left alone.
  const afterCheckout = dropRows(body, checkoutReport());
  assert.deepStrictEqual(body, before);
  assert.deepStrictEqual(afterCheckout, {
    repositories: [repositoryRow({ checkouts: [before.repositories[0].checkouts[1]] })],
    standalone: [standaloneRow()],
  });

  // A repository: gone with every checkout under it.
  const afterRepository = dropRows(body, repositoryReport());
  assert.deepStrictEqual(body, before);
  assert.deepStrictEqual(afterRepository, { repositories: [], standalone: [standaloneRow()] });

  // A repository key alone still takes its children: they live inside it.
  const parentOnly = dropRows(body, repositoryReport({ rows_removed: [{ repository: 1 }] }));
  assert.deepStrictEqual(body, before);
  assert.deepStrictEqual(parentOnly, { repositories: [], standalone: [standaloneRow()] });

  // A standalone session: only that entry.
  const afterStandalone = dropRows(body, standaloneReport());
  assert.deepStrictEqual(body, before);
  assert.deepStrictEqual(afterStandalone, { repositories: [repositoryRow()], standalone: [] });

  // The three key types share one number space and are never conflated:
  // checkout 1 is not repository 1, and standalone 7 is not checkout 7.
  const crossKind = dropRows(body, checkoutReport({ rows_removed: [{ checkout: 1 }, { standalone: 7 }] }));
  assert.deepStrictEqual(crossKind, before);
});

test("drop_rows_is_a_noop_for_an_absent_key", () => {
  const { dropRows } = BaudeRemoval;
  const body = rowsBody();
  const before = structuredClone(body);
  // API-03: an SSE frame for a row another client already removed.
  const after = dropRows(body, checkoutReport({ target: { checkout: 99 }, rows_removed: [{ checkout: 99 }] }));
  assert.deepStrictEqual(after, before);
  assert.deepStrictEqual(body, before);
});

test("parse_drift_body_reads_the_409_json_and_preserves_plain_text", () => {
  const { parseDriftBody } = BaudeRemoval;
  const report = repositoryReport({ planned: true });

  assert.deepStrictEqual(parseDriftBody(JSON.stringify(report)), { report, text: null });
  assert.deepStrictEqual(parseDriftBody("removal failed: runtime survived"), {
    report: null,
    text: "removal failed: runtime survived",
  });
  assert.deepStrictEqual(parseDriftBody(""), { report: null, text: "" });

  // JSON that is not a report is plain text to the caller.
  assert.deepStrictEqual(parseDriftBody("null"), { report: null, text: "null" });
  assert.deepStrictEqual(parseDriftBody("42"), { report: null, text: "42" });
  assert.deepStrictEqual(parseDriftBody('{"error":"nope"}'), { report: null, text: '{"error":"nope"}' });
  assert.deepStrictEqual(parseDriftBody("[1,2]"), { report: null, text: "[1,2]" });
});

test("key_equals_compares_row_keys_by_kind_and_key", () => {
  assert.strictEqual(typeof BaudeRemoval.keyEquals, "function");
  const { keyEquals } = BaudeRemoval;
  const both = (a, b) => [keyEquals(a, b), keyEquals(b, a)];

  // Two distinct instances of the same kind and key, in all three shapes.
  assert.deepStrictEqual(both({ repository: 1 }, { repository: 1 }), [true, true]);
  assert.deepStrictEqual(both({ checkout: 7 }, { checkout: 7 }), [true, true]);
  assert.deepStrictEqual(both({ standalone: 2 }, { standalone: 2 }), [true, true]);

  // Same kind, different key.
  assert.deepStrictEqual(both({ checkout: 7 }, { checkout: 8 }), [false, false]);

  // Same number, different kind: the key types share a u64 space.
  assert.deepStrictEqual(both({ checkout: 7 }, { repository: 7 }), [false, false]);
  assert.deepStrictEqual(both({ checkout: 7 }, { standalone: 7 }), [false, false]);

  // An unrecognised shape equals nothing, itself included.
  assert.deepStrictEqual(both({ Checkout: 7 }, { Checkout: 7 }), [false, false]);
  assert.deepStrictEqual(both({ session: 7 }, { session: 7 }), [false, false]);
  assert.deepStrictEqual(both({}, {}), [false, false]);
  const sameInstance = { session: 7 };
  assert.strictEqual(keyEquals(sameInstance, sameInstance), false);

  // A missing side is false, never a throw.
  assert.deepStrictEqual(both(null, { checkout: 7 }), [false, false]);
  assert.deepStrictEqual(both({ checkout: 7 }, undefined), [false, false]);
  assert.deepStrictEqual(both(null, undefined), [false, false]);
  assert.deepStrictEqual(both("checkout:7", { checkout: 7 }), [false, false]);
});

test("removal_notice_and_clearing_failures_read_the_report", () => {
  const { removalNotice, clearingFailures } = BaudeRemoval;

  assert.equal(
    removalNotice(repositoryReport({ planned: false, persistence: { outcome: "saved" } })),
    "removed 3 rows, killed 1 runtime, deleted 2 paths",
  );
  assert.equal(
    removalNotice(standaloneReport({ planned: false, persistence: { outcome: "saved" } })),
    "removed 1 row, killed 0 runtimes, deleted 0 paths",
  );

  assert.deepStrictEqual(clearingFailures(checkoutReport()), []);
  const failed = checkoutReport({
    planned: false,
    persistence: { outcome: "saved_degraded", detail: "marker left behind" },
    clearing_failures: [
      { kind: "marker", path: `${WORKTREES}/topic-7`, error: "Permission denied (os error 13)" },
      { kind: "binding", path: REPO_PATH, error: "read-only file system" },
    ],
  });
  assert.deepStrictEqual(clearingFailures(failed), [
    `marker ${WORKTREES}/topic-7: Permission denied (os error 13)`,
    `binding ${REPO_PATH}: read-only file system`,
  ]);
  assert.deepStrictEqual(clearingFailures(failed, { home: HOME }), [
    "marker ~/.local/share/baude/worktrees/claude/repository-1/topic-7: Permission denied (os error 13)",
    "binding ~/code/repo: read-only file system",
  ]);
});

// ---- Task 3: snapshot arbitration ----

// A committed 200 report: what a tombstone is built from.
function committed(report) {
  return { ...report, planned: false, persistence: { outcome: "saved" } };
}

// The tombstone set is a null-prototype object; compare it as plain entries.
function entries(tombstones) {
  return Object.keys(tombstones).sort().map((id) => [id, tombstones[id]]);
}

test("apply_snapshot_suppresses_a_tombstoned_row_the_snapshot_still_lists", () => {
  const { applySnapshot, tombstone } = BaudeRemoval;
  const tombstones = tombstone(Object.create(null), committed(checkoutReport()), 1000);
  const tombstonesBefore = entries(tombstones);
  // HIGH #5: a poll that left before the removal lands after it, and still
  // lists checkout 7.
  const snapshot = rowsBody();
  const snapshotBefore = structuredClone(snapshot);

  const body = applySnapshot(snapshot, tombstones);

  assert.deepStrictEqual(
    body.repositories[0].checkouts.map((c) => c.checkout_key),
    [8],
    "checkout 7 stays gone and its sibling survives",
  );
  assert.equal(body.repositories[0].repository_key, 1, "the parent survives");
  assert.deepStrictEqual(body.standalone, [standaloneRow()]);
  assert.deepStrictEqual(snapshot, snapshotBefore);
  assert.deepStrictEqual(entries(tombstones), tombstonesBefore);
  assert.deepStrictEqual(tombstonesBefore, [["checkout:7", 1000]]);
});

test("apply_snapshot_suppresses_a_tombstoned_repository_and_its_children", () => {
  const { applySnapshot, tombstone } = BaudeRemoval;
  // The parent key alone is enough: the children live inside it.
  const report = committed(repositoryReport({ rows_removed: [{ repository: 1 }] }));
  const tombstones = tombstone(Object.create(null), report, 1000);
  const tombstonesBefore = entries(tombstones);
  const snapshot = rowsBody();
  const snapshotBefore = structuredClone(snapshot);
  assert.equal(snapshot.repositories[0].checkouts.length, 2);

  const body = applySnapshot(snapshot, tombstones);

  assert.deepStrictEqual(body.repositories, []);
  assert.deepStrictEqual(body.standalone, [standaloneRow()]);
  assert.deepStrictEqual(snapshot, snapshotBefore);
  assert.deepStrictEqual(entries(tombstones), tombstonesBefore);
});

test("apply_snapshot_passes_untombstoned_rows_through_unchanged", () => {
  const { applySnapshot } = BaudeRemoval;
  const snapshot = rowsBody();
  const snapshotBefore = structuredClone(snapshot);
  const empty = Object.create(null);

  assert.deepStrictEqual(applySnapshot(snapshot, empty), snapshotBefore);
  assert.deepStrictEqual(snapshot, snapshotBefore);
  assert.deepStrictEqual(entries(empty), []);

  // A plain {} and a missing set are empty sets too, and a tombstone for a
  // row of another kind with the same number suppresses nothing.
  assert.deepStrictEqual(applySnapshot(snapshot, {}), snapshotBefore);
  assert.deepStrictEqual(applySnapshot(snapshot, undefined), snapshotBefore);
  assert.deepStrictEqual(applySnapshot(snapshot, { "standalone:7": 1000, "checkout:1": 1000 }), snapshotBefore);
});

test("apply_snapshot_is_idempotent", () => {
  const { applySnapshot, tombstone } = BaudeRemoval;
  const tombstones = tombstone(Object.create(null), committed(checkoutReport()), 1000);
  const tombstonesBefore = entries(tombstones);
  const snapshot = rowsBody();
  const snapshotBefore = structuredClone(snapshot);

  const first = applySnapshot(snapshot, tombstones);
  const second = applySnapshot(snapshot, tombstones);
  assert.deepStrictEqual(second, first);
  // Feeding the result back in changes nothing either.
  assert.deepStrictEqual(applySnapshot(first, tombstones), first);
  assert.deepStrictEqual(snapshot, snapshotBefore);
  assert.deepStrictEqual(entries(tombstones), tombstonesBefore);
});

test("tombstoning_the_same_report_twice_adds_nothing", () => {
  const { tombstone } = BaudeRemoval;
  const report = committed(repositoryReport());
  const empty = Object.create(null);

  const once = tombstone(empty, report, 1000);
  const onceBefore = entries(once);
  // A duplicate 200, or the removal's own rows_removed frame reaching the tab
  // that started it.
  const twice = tombstone(once, report, 2000);

  assert.strictEqual(Object.keys(twice).length, Object.keys(once).length);
  assert.deepStrictEqual(entries(twice), [
    ["checkout:7", 1000],
    ["checkout:8", 1000],
    ["repository:1", 1000],
  ]);
  assert.deepStrictEqual(entries(once), onceBefore, "the earlier set is not mutated");
  assert.deepStrictEqual(entries(empty), [], "the empty set is not mutated");
  assert.notStrictEqual(twice, once);

  // A later, different report adds its own rows at its own time and leaves
  // the earlier stamps alone. A row key the module cannot read is skipped.
  const later = tombstone(once, committed(standaloneReport({ rows_removed: [{ standalone: 2 }, { session: 9 }] })), 3000);
  assert.deepStrictEqual(entries(later), [
    ["checkout:7", 1000],
    ["checkout:8", 1000],
    ["repository:1", 1000],
    ["standalone:2", 3000],
  ]);
});

test("drop_rows_then_apply_snapshot_agree", () => {
  const { dropRows, applySnapshot, tombstone } = BaudeRemoval;
  const reports = [
    checkoutReport(),
    repositoryReport(),
    repositoryReport({ rows_removed: [{ repository: 1 }] }),
    standaloneReport(),
    checkoutReport({ rows_removed: [{ checkout: 99 }] }),
    checkoutReport({ rows_removed: [] }),
  ];
  for (const report of reports) {
    const body = rowsBody();
    const before = structuredClone(body);
    const tombstones = tombstone({}, committed(report), 1000);
    assert.deepStrictEqual(
      applySnapshot(body, tombstones),
      dropRows(body, committed(report)),
      `scope disagrees for ${JSON.stringify(report.rows_removed)}`,
    );
    assert.deepStrictEqual(body, before);
  }
});

test("prune_tombstones_expires_after_the_ttl_and_not_before", () => {
  const { pruneTombstones, tombstone, applySnapshot } = BaudeRemoval;
  const t = 1_700_000_000_000;
  const set = tombstone(Object.create(null), committed(checkoutReport()), t);
  const setBefore = entries(set);

  assert.deepStrictEqual(entries(pruneTombstones(set, t + 59_999, 60_000)), [["checkout:7", t]]);
  assert.deepStrictEqual(entries(pruneTombstones(set, t + 60_001, 60_000)), []);
  assert.deepStrictEqual(entries(set), setBefore, "pruning returns a new set");
  assert.notStrictEqual(pruneTombstones(set, t, 60_000), set);

  // The default window is 60 s.
  assert.deepStrictEqual(entries(pruneTombstones(set, t + 59_999)), [["checkout:7", t]]);
  assert.deepStrictEqual(entries(pruneTombstones(set, t + 60_001)), []);

  // Only the expired entry goes, and once it has, the daemon's row shows
  // again: a client cannot hide a row the daemon keeps reporting forever.
  const mixed = tombstone(set, committed(standaloneReport()), t + 30_000);
  const pruned = pruneTombstones(mixed, t + 60_001, 60_000);
  assert.deepStrictEqual(entries(pruned), [["standalone:2", t + 30_000]]);
  const body = applySnapshot(rowsBody(), pruned);
  assert.deepStrictEqual(body.repositories[0].checkouts.map((c) => c.checkout_key), [7, 8]);
  assert.deepStrictEqual(body.standalone, []);
});

test("apply_snapshot_tolerates_a_partial_body", () => {
  const { applySnapshot, tombstone } = BaudeRemoval;
  const tombstones = tombstone(Object.create(null), committed(checkoutReport()), 1000);
  const tombstonesBefore = entries(tombstones);

  const noStandalone = { repositories: [repositoryRow()] };
  const noStandaloneBefore = structuredClone(noStandalone);
  const first = applySnapshot(noStandalone, tombstones);
  assert.deepStrictEqual(first.standalone, []);
  assert.deepStrictEqual(first.repositories[0].checkouts.map((c) => c.checkout_key), [8]);
  assert.deepStrictEqual(noStandalone, noStandaloneBefore);

  const noRepositories = { standalone: [standaloneRow()] };
  const noRepositoriesBefore = structuredClone(noRepositories);
  const second = applySnapshot(noRepositories, tombstones);
  assert.deepStrictEqual(second, { repositories: [], standalone: [standaloneRow()] });
  assert.deepStrictEqual(noRepositories, noRepositoriesBefore);

  // A truncated or wrong-typed response still yields a renderable body, and
  // a parent the daemon serves with no checkouts at all passes through.
  for (const broken of [null, undefined, "", [], { repositories: "x", standalone: 3 }]) {
    assert.deepStrictEqual(applySnapshot(broken, tombstones), { repositories: [], standalone: [] });
  }
  const childless = { repositories: [repositoryRow({ checkouts: [] })], standalone: [] };
  assert.deepStrictEqual(applySnapshot(childless, tombstones), childless);
  assert.deepStrictEqual(entries(tombstones), tombstonesBefore);
});

// ---- plan-level edges ----

test("drop_rows_applied_twice_changes_nothing_more", () => {
  // API-03 idempotency: the same report arriving twice, as a duplicate 200 or
  // as a 200 followed by its own rows_removed frame.
  const { dropRows } = BaudeRemoval;
  for (const report of [checkoutReport(), repositoryReport(), standaloneReport()]) {
    const body = rowsBody();
    const before = structuredClone(body);
    const once = dropRows(body, report);
    const onceBefore = structuredClone(once);
    assert.deepStrictEqual(dropRows(once, report), once);
    assert.deepStrictEqual(once, onceBefore);
    assert.deepStrictEqual(body, before);
  }
});

test("rendered_confirmation_carries_no_session_identifier", () => {
  // The view names rows by display name, branch, path, pid and count. The
  // session id is a join column in the index and never reaches the page.
  const { renderConfirmation, confirmationView, rowsIndex } = BaudeRemoval;
  const sessionId = 987654321;
  const rows = rowsBody({
    repositories: [repositoryRow({ checkouts: [checkoutRow({ session_id: sessionId })] })],
  });
  const index = rowsIndex(rows);
  assert.strictEqual(index["checkout:7"].session_id, sessionId, "the index does carry it");

  const view = confirmationView(checkoutReport(), index);
  const html = renderConfirmation(view);
  assert.equal(JSON.stringify(view).includes(String(sessionId)), false);
  assert.equal(html.includes(String(sessionId)), false);
  assert.ok(html.includes("repo:topic"), "the row is still named");
});

test("indices_and_tombstones_ignore_prototype_shaped_keys", () => {
  // Daemon-derived strings become property names. Neither store has a
  // prototype, and a lookup never answers from one.
  const { rowsIndex, reportRowNames, tombstone, applySnapshot } = BaudeRemoval;
  const index = rowsIndex(rowsBody());
  assert.strictEqual(Object.getPrototypeOf(index), null);
  const set = tombstone(Object.create(null), committed(checkoutReport()), 1000);
  assert.strictEqual(Object.getPrototypeOf(set), null);

  // A key shaped like an inherited property name is an unknown row, and a
  // plain-object index does not resolve it through Object.prototype.
  const hostile = JSON.parse('{"rows_removed":[{"__proto__":1},{"constructor":1},{"toString":1}]}');
  assert.deepStrictEqual(reportRowNames(hostile, {}).map((n) => n.label), [
    "unknown row",
    "unknown row",
    "unknown row",
  ]);
  assert.deepStrictEqual(entries(tombstone(Object.create(null), hostile, 1000)), []);
  assert.equal(Object.prototype.hasOwnProperty.call(Object.prototype, "checkout:7"), false);

  // A plain-object tombstone set only suppresses what it owns.
  const body = rowsBody();
  assert.deepStrictEqual(applySnapshot(body, {}), body);
});
