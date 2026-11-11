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

  assert.equal(view.title, "Remove this checkout?");
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

  // The title names the target kind for all three RemovalTarget shapes.
  const titled = (target) =>
    confirmationView(checkoutReport({ target }), rowsIndex(rowsBody())).title;
  assert.equal(titled({ repository: 1 }), "Remove this repository?");
  assert.equal(titled({ standalone: 2 }), "Remove this standalone session?");
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
