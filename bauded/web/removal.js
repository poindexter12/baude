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

  // An externally tagged RowKey ({"checkout":7}) as the string "checkout:7".
  // Anything else is null: an unknown tag, a second key, a non-integer key.
  function rowKeyId(rowKey) {
    if (!isObject(rowKey)) return null;
    const names = Object.keys(rowKey);
    if (names.length !== 1) return null;
    const kind = names[0];
    if (!ROW_KINDS.includes(kind) || !hasOwn(rowKey, kind)) return null;
    const key = rowKey[kind];
    if (typeof key !== "number" || !Number.isInteger(key) || key < 0) return null;
    return `${kind}:${key}`;
  }

  function indexEntry(kind, key, row, repositoryKey) {
    return {
      kind,
      key,
      display_name: row.display_name,
      branch: kind === "checkout" && typeof row.branch === "string" ? row.branch : null,
      path: row.path,
      session_id: row.session_id === undefined ? null : row.session_id,
      repository_key: repositoryKey,
    };
  }

  // GET /rows as a lookup from rowKeyId to what the page needs to name a row.
  // Read-only and tolerant: a missing or malformed part of the body is empty.
  function rowsIndex(rowsBody) {
    const index = Object.create(null);
    const body = isObject(rowsBody) ? rowsBody : {};
    const add = (kind, key, row, repositoryKey) => {
      const id = rowKeyId({ [kind]: key });
      if (id !== null) index[id] = indexEntry(kind, key, row, repositoryKey);
    };
    for (const repository of asArray(body.repositories)) {
      if (!isObject(repository)) continue;
      add("repository", repository.repository_key, repository, repository.repository_key);
      for (const checkout of asArray(repository.checkouts)) {
        if (!isObject(checkout)) continue;
        add("checkout", checkout.checkout_key, checkout, checkout.repository_key);
      }
    }
    for (const standalone of asArray(body.standalone)) {
      if (!isObject(standalone)) continue;
      add("standalone", standalone.standalone_key, standalone, null);
    }
    return index;
  }

  function rowLabel(rowKey, index) {
    const id = rowKeyId(rowKey);
    if (id === null) return "unknown row";
    const entry = isObject(index) ? index[id] : undefined;
    if (entry && typeof entry.display_name === "string") return entry.display_name;
    return id.replace(":", " ");
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

  // A RemovalReport as { title, sections: [{ label, count, lines }] }, in the
  // order of remove_project_confirmation_lines (baude/src/ui.rs:2509). Every
  // value is RAW daemon data. Escaping happens once, in renderConfirmation.
  function confirmationView(report, index) {
    const source = isObject(report) ? report : {};
    const orNone = (lines) => (lines.length === 0 ? ["none"] : lines);

    const rows = asArray(source.rows_removed);
    const killed = asArray(source.runtimes_killed);
    const deleted = asArray(source.paths_deleted);
    const kept = asArray(source.paths_kept);
    const alreadyRemoved = asArray(source.already_removed);
    const cleared = asArray(source.cleared);

    const sections = [
      {
        label: "rows dropped",
        count: rows.length,
        lines: rows.map((rowKey) => rowLabel(rowKey, index)),
      },
      {
        label: "runtimes killed",
        count: killed.length,
        lines: orNone(killed.map((runtime) => {
          const entry = isObject(runtime) ? runtime : {};
          const shell = entry.shell_pid === null || entry.shell_pid === undefined
            ? ""
            : ` shell ${entry.shell_pid}`;
          return `${rowLabel(entry.row, index)} (pid ${entry.agent_pid}${shell})`;
        })),
      },
      { label: "paths deleted", count: deleted.length, lines: orNone(deleted.map(String)) },
      { label: "paths kept", count: kept.length, lines: orNone(kept.map(String)) },
    ];
    if (alreadyRemoved.length > 0) {
      sections.push({
        label: "already removed",
        count: alreadyRemoved.length,
        lines: alreadyRemoved.map(String),
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

  const BaudeRemoval = {
    escapeHtml,
    rowKeyId,
    rowsIndex,
    confirmationView,
    renderConfirmation,
  };

  globalThis.BaudeRemoval = BaudeRemoval;
  if (typeof module !== "undefined" && module.exports) {
    module.exports = BaudeRemoval;
  }
})();
