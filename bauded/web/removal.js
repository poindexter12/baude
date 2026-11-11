// baude PWA removal view model.
//
// Pure functions only: daemon JSON in, plain data or an HTML string out. This
// file touches no network, no DOM, no clock and no host object, so the same
// source loads as a browser script tag (it publishes `BaudeRemoval` on the
// global object) and as a Node CommonJS module for the suite under
// bauded/web/tests/. The controller that fetches, listens and writes to the
// page lives in app.js and calls in here.
//
// Shapes consumed: `RemovalReport` from baude-core/src/removal.rs and the
// `GET /rows` body from bauded/src/manager.rs.

(function () {
  "use strict";

  const ROW_KINDS = ["repository", "checkout", "standalone"];

  function hasOwn(object, key) {
    return Object.prototype.hasOwnProperty.call(object, key);
  }

  function isObject(value) {
    return value !== null && typeof value === "object";
  }

  // A JSON object, as opposed to a JSON array.
  function isRecord(value) {
    return isObject(value) && !Array.isArray(value);
  }

  function asArray(value) {
    return Array.isArray(value) ? value : [];
  }

  // The replacement table is esc() at bauded/web/app.js:33. Keep the two
  // identical: a second escaping convention in one page is the defect this
  // module exists to prevent.
  const HTML_ENTITIES = {
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  };

  function escapeHtml(value) {
    return String(value).replace(/[&<>"']/g, (c) => HTML_ENTITIES[c]);
  }

  // Shorten a path for display: home becomes `~`. The authority is tilde_path
  // at baude/src/ui.rs:1360, which uses Rust's strip_prefix on the displayed
  // string: a plain string prefix strip, NOT a path segment comparison. So
  // `/Users/joe2/x` under home `/Users/joe` reads `~2/x` in the TUI, and it
  // reads the same here on purpose. Do not make this segment-aware unless the
  // TUI changes first. The daemon serves no home path, so the caller passes
  // one; no home (or `/`, the resolver's "no home at all") is the identity.
  function tildePath(path, home) {
    const text = String(path);
    if (!home || home === "/") return text;
    const prefix = String(home);
    return text.startsWith(prefix) ? `~${text.slice(prefix.length)}` : text;
  }

  // An externally tagged RowKey ({"checkout":7}) as the string "checkout:7".
  // Anything else is null: an unknown tag, a second key, a non-integer key.
  function rowKeyId(rowKey) {
    if (!isRecord(rowKey)) return null;
    const names = Object.keys(rowKey);
    if (names.length !== 1) return null;
    const kind = names[0];
    if (!ROW_KINDS.includes(kind) || !hasOwn(rowKey, kind)) return null;
    const key = rowKey[kind];
    if (typeof key !== "number" || !Number.isInteger(key) || key < 0) return null;
    return `${kind}:${key}`;
  }

  // RowKey equality: same kind AND same key. The three key types share one
  // number space, so {"checkout":7} is not {"repository":7}. An unrecognised
  // shape equals nothing, itself included. This is the one matching rule in
  // the module; dropRows and applySnapshot both decide through rowKeyId.
  function keyEquals(a, b) {
    if (!isObject(a) || !isObject(b)) return false;
    const left = rowKeyId(a);
    return left !== null && left === rowKeyId(b);
  }

  function indexEntry(kind, key, row, repositoryKey) {
    return {
      kind,
      key,
      display_name: row.display_name,
      branch: kind === "checkout" && typeof row.branch === "string" ? row.branch : null,
      path: row.path,
      session_id: row.session_id === undefined ? null : row.session_id,
      repository_key: repositoryKey === undefined ? null : repositoryKey,
    };
  }

  // GET /rows as a lookup from rowKeyId to what the page needs to name a row.
  // Read-only and tolerant: a missing or malformed part of the body is empty.
  function rowsIndex(rowsBody) {
    const index = Object.create(null);
    const body = isRecord(rowsBody) ? rowsBody : {};
    const add = (kind, key, row, repositoryKey) => {
      const id = rowKeyId({ [kind]: key });
      if (id !== null) index[id] = indexEntry(kind, key, row, repositoryKey);
    };
    for (const repository of asArray(body.repositories)) {
      if (!isRecord(repository)) continue;
      add("repository", repository.repository_key, repository, repository.repository_key);
      for (const checkout of asArray(repository.checkouts)) {
        if (!isRecord(checkout)) continue;
        add("checkout", checkout.checkout_key, checkout, checkout.repository_key);
      }
    }
    for (const standalone of asArray(body.standalone)) {
      if (!isRecord(standalone)) continue;
      add("standalone", standalone.standalone_key, standalone, null);
    }
    return index;
  }

  // One RowKey as { id, kind, key, label, branch, resolved }. A key the index
  // cannot name is labelled from the key itself ("checkout 8") and marked
  // unresolved. A name is never guessed.
  function rowName(rowKey, index) {
    const id = rowKeyId(rowKey);
    if (id === null) {
      return { id: null, kind: null, key: null, label: "unknown row", branch: null, resolved: false };
    }
    const kind = Object.keys(rowKey)[0];
    const key = rowKey[kind];
    const entry = isObject(index) && hasOwn(index, id) ? index[id] : null;
    if (isObject(entry) && typeof entry.display_name === "string" && entry.display_name !== "") {
      const branch = typeof entry.branch === "string" ? entry.branch : null;
      return { id, kind, key, label: entry.display_name, branch, resolved: true };
    }
    return { id, kind, key, label: `${kind} ${key}`, branch: null, resolved: false };
  }

  // Every entry of report.rows_removed, named through the index. No input row
  // is ever dropped from the result, resolved or not.
  function reportRowNames(report, index) {
    const rows = isObject(report) ? asArray(report.rows_removed) : [];
    return rows.map((rowKey) => rowName(rowKey, index));
  }

  function targetKind(target) {
    const id = rowKeyId(target);
    if (id === null) return "row";
    const kind = id.slice(0, id.indexOf(":"));
    return kind === "standalone" ? "standalone session" : kind;
  }

  function plural(count, one, many) {
    return `${count} ${count === 1 ? one : many}`;
  }

  // The TUI's grouped `cleared:` summary (baude/src/ui.rs:2577).
  function clearedSummary(cleared) {
    const count = (kinds) => cleared.filter((item) => isObject(item) && kinds.includes(item.kind)).length;
    const parts = [];
    const bindings = count(["binding"]);
    if (bindings > 0) parts.push(plural(bindings, "binding", "bindings"));
    const markers = count(["marker", "repository_dir"]);
    if (markers > 0) parts.push(plural(markers, "marker", "markers"));
    const suppression = count(["suppression_added", "suppression_dropped"]);
    if (suppression > 0) parts.push(plural(suppression, "suppression entry", "suppression entries"));
    return parts.join(", ");
  }

  function homeOf(options) {
    return isObject(options) && typeof options.home === "string" ? options.home : null;
  }

  // A RemovalReport as { title, sections: [{ label, count, lines }] }, in the
  // order of remove_project_confirmation_lines (baude/src/ui.rs:2509). Every
  // value is RAW daemon data. Escaping happens once, in renderConfirmation.
  // `options.home` shortens paths through tildePath; without it they stay the
  // absolute paths the daemon sent.
  function confirmationView(report, index, options) {
    const source = isObject(report) ? report : {};
    const home = homeOf(options);
    const orNone = (lines) => (lines.length === 0 ? ["none"] : lines);
    const paths = (list) => list.map((path) => tildePath(path, home));

    const rows = reportRowNames(source, index);
    const killed = asArray(source.runtimes_killed);
    const deleted = asArray(source.paths_deleted);
    const kept = asArray(source.paths_kept);
    const alreadyRemoved = asArray(source.already_removed);
    const cleared = asArray(source.cleared);

    const sections = [
      {
        label: "rows dropped",
        count: rows.length,
        lines: rows.map((row) => (row.branch ? `${row.label} ⎇ ${row.branch}` : row.label)),
      },
      {
        label: "runtimes killed",
        count: killed.length,
        lines: orNone(killed.map((runtime) => {
          const entry = isObject(runtime) ? runtime : {};
          const shell = entry.shell_pid === null || entry.shell_pid === undefined
            ? ""
            : ` shell ${entry.shell_pid}`;
          return `${rowName(entry.row, index).label} (pid ${entry.agent_pid}${shell})`;
        })),
      },
      { label: "paths deleted", count: deleted.length, lines: orNone(paths(deleted)) },
      { label: "paths kept", count: kept.length, lines: orNone(paths(kept)) },
    ];
    if (alreadyRemoved.length > 0) {
      sections.push({
        label: "already removed",
        count: alreadyRemoved.length,
        lines: paths(alreadyRemoved),
      });
    }
    if (cleared.length > 0) {
      sections.push({ label: "cleared", count: cleared.length, lines: [clearedSummary(cleared)] });
    }
    // readers_parked is not rendered: it is always empty in a preview and the
    // TUI does not show it either.

    return { title: `Remove this ${targetKind(source.target)}?`, sections };
  }

  // The single escaping site. Takes a view, never a report: a report handed
  // straight to the renderer would route daemon data around the view model.
  // Emits no buttons and no event handlers; the controller owns those.
  function renderConfirmation(view) {
    if (!isObject(view) || hasOwn(view, "rows_removed") || !Array.isArray(view.sections)) {
      throw new TypeError("renderConfirmation takes a confirmationView result, not a report");
    }
    const sections = view.sections.map((section) => {
      const entry = isObject(section) ? section : {};
      const lines = asArray(entry.lines).map((line) => `<li>${escapeHtml(line)}</li>`).join("");
      return `<div class="section"><h4>${escapeHtml(entry.label)} (${escapeHtml(entry.count)})</h4>`
        + `<ul>${lines}</ul></div>`;
    });
    return `<h3>${escapeHtml(view.title)}</h3>${sections.join("")}`;
  }

  // A /rows body with every row `isGone` names left out. A repository takes
  // its checkouts with it; a checkout leaves its parent. Returns a new body
  // with both root keys present as arrays and never mutates its argument:
  // untouched entries are reused, a parent that lost a child is rebuilt.
  function filterRows(rowsBody, isGone) {
    const body = isRecord(rowsBody) ? rowsBody : {};
    const repositories = [];
    for (const repository of asArray(body.repositories)) {
      if (!isRecord(repository)) {
        repositories.push(repository);
        continue;
      }
      if (isGone({ repository: repository.repository_key })) continue;
      const checkouts = asArray(repository.checkouts);
      const kept = checkouts.filter(
        (checkout) => !(isRecord(checkout) && isGone({ checkout: checkout.checkout_key })),
      );
      repositories.push(
        kept.length === checkouts.length ? repository : { ...repository, checkouts: kept },
      );
    }
    const standalone = asArray(body.standalone).filter(
      (entry) => !(isRecord(entry) && isGone({ standalone: entry.standalone_key })),
    );
    return { ...body, repositories, standalone };
  }

  // The /rows body with every row named by report.rows_removed absent. A key
  // the body does not hold changes nothing.
  function dropRows(rowsBody, report) {
    const removed = isObject(report) ? asArray(report.rows_removed) : [];
    return filterRows(rowsBody, (rowKey) => removed.some((gone) => keyEquals(gone, rowKey)));
  }

  // The raw text of an error response as { report, text }. A 409 is the only
  // JSON error body the daemon sends and it is a fresh RemovalReport; every
  // other error is plain text. JSON that is not a report ("null", "42", some
  // other object) is text to the caller, so it cannot pass for a drift report.
  function parseDriftBody(bodyText) {
    const text = String(bodyText);
    let parsed;
    try {
      parsed = JSON.parse(text);
    } catch (_) {
      return { report: null, text };
    }
    if (isRecord(parsed) && hasOwn(parsed, "rows_removed")) return { report: parsed, text: null };
    return { report: null, text };
  }

  // One display string per report.clearing_failures entry. Those happen after
  // the save and do not fail the removal, so they are reported, not thrown.
  function clearingFailures(report, options) {
    const home = homeOf(options);
    const failures = isObject(report) ? asArray(report.clearing_failures) : [];
    return failures.map((failure) => {
      const entry = isObject(failure) ? failure : {};
      return `${entry.kind} ${tildePath(entry.path, home)}: ${entry.error}`;
    });
  }

  // The footer line for a completed removal, counts worded as the TUI's
  // removal_notice (baude/src/app.rs) words them. Clearing failures are not
  // folded in; clearingFailures returns those for the caller to show.
  function removalNotice(report) {
    const source = isObject(report) ? report : {};
    const rows = asArray(source.rows_removed).length;
    const killed = asArray(source.runtimes_killed).length;
    const deleted = asArray(source.paths_deleted).length;
    return `removed ${plural(rows, "row", "rows")}, killed ${plural(killed, "runtime", "runtimes")}, `
      + `deleted ${plural(deleted, "path", "paths")}`;
  }

  // ---- snapshot arbitration ----
  //
  // A list poll that left before a removal can land after it and still list
  // the removed row. Assigning that response straight into render state would
  // resurrect the row until the next poll. The three functions below let the
  // controller hold a removal's scope as a tombstone set and filter every
  // later snapshot through it. The wiring is the controller's, in app.js:
  //
  //   - every GET /rows response goes through applySnapshot before it reaches
  //     render state;
  //   - a 200 report, or a rows_removed frame, goes through tombstone;
  //   - pruneTombstones runs on the poll tick.
  //
  // All three are pure. None reads a clock: the caller passes nowMs.

  const TOMBSTONE_TTL_MS = 60000;

  function copyTombstones(tombstones, keep) {
    const next = Object.create(null);
    if (!isRecord(tombstones)) return next;
    for (const id of Object.keys(tombstones)) {
      if (keep(tombstones[id])) next[id] = tombstones[id];
    }
    return next;
  }

  // The set plus one `rowKeyId -> nowMs` entry per report.rows_removed. An id
  // already present keeps its ORIGINAL stamp: marking a row again must not
  // extend how long it is suppressed, and that is what makes a duplicate
  // report a no-op. A row key that cannot be read is skipped.
  function tombstone(tombstones, report, nowMs) {
    const next = copyTombstones(tombstones, () => true);
    const removed = isObject(report) ? asArray(report.rows_removed) : [];
    for (const rowKey of removed) {
      const id = rowKeyId(rowKey);
      if (id !== null && !hasOwn(next, id)) next[id] = nowMs;
    }
    return next;
  }

  // The set without entries older than ttlMs. The PWA list poll runs every
  // 3 s (bauded/web/app.js:800), so the 60 s default covers any response in
  // flight twenty times over. It still expires, so a client can never
  // permanently hide a row the daemon keeps reporting.
  function pruneTombstones(tombstones, nowMs, ttlMs) {
    const ttl = ttlMs === undefined ? TOMBSTONE_TTL_MS : ttlMs;
    return copyTombstones(tombstones, (stamp) => nowMs - stamp <= ttl);
  }

  // A /rows snapshot with every tombstoned row left out, both root keys
  // present as arrays. Built on filterRows, the same walk dropRows uses, with
  // the same rowKeyId match, so the immediate local drop and the arbitration
  // of a later snapshot cannot disagree about scope. Does not prune: expiry
  // is the caller's explicit pruneTombstones call.
  function applySnapshot(snapshot, tombstones) {
    const set = isRecord(tombstones) ? tombstones : null;
    return filterRows(snapshot, (rowKey) => {
      const id = rowKeyId(rowKey);
      return set !== null && id !== null && hasOwn(set, id);
    });
  }

  const BaudeRemoval = {
    escapeHtml,
    tildePath,
    rowKeyId,
    keyEquals,
    rowsIndex,
    reportRowNames,
    confirmationView,
    renderConfirmation,
    dropRows,
    tombstone,
    pruneTombstones,
    applySnapshot,
    parseDriftBody,
    clearingFailures,
    removalNotice,
  };

  globalThis.BaudeRemoval = BaudeRemoval;
  if (typeof module !== "undefined" && module.exports) {
    module.exports = BaudeRemoval;
  }
})();
