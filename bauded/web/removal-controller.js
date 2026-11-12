// baude PWA removal controller.
//
// The state machine between the page and the daemon's removal contract: the
// row list from GET /rows, the preview-then-confirm modal over
// POST /removals/preview and POST /removals, and what counts as evidence that
// a row is gone. It decides; removal.js computes. Every report, snapshot and
// HTML string here comes from a BaudeRemoval function, and none of those is
// re-implemented in this file.
//
// Nothing environmental is touched directly. The network, the event stream,
// the clock, the re-render, the toast and the route change all arrive through
// `deps`, so the same source loads as a browser script tag (it publishes
// `BaudeRemovalController` on the global object) and as a Node CommonJS module
// for the suite under bauded/web/tests/, which drives the real state machine
// through scripted doubles.

(function () {
  "use strict";

  const ROW_KINDS = ["repository", "checkout", "standalone"];

  // deps:
  //   removal            the BaudeRemoval object from removal.js
  //   request(path, o)   resolves parsed JSON on 2xx; rejects with an object
  //                      carrying `status` and `message`, the raw response text
  //   openEvents(url)    returns an EventSource-like object
  //   now()              milliseconds
  //   onChange()         controller state changed; re-render
  //   notify(text)       show one line to the user
  //   onSessionGone(id)  the row owning this live session id was removed
  function createRemovalController(deps) {
    const removal = deps.removal;

    // The /rows body the page renders from. Written in exactly two places:
    // applyRows, for a fetched body, and commit, for a removal the daemon
    // reported. A third writer is how a stale poll resurrects a removed row.
    let rows = removal.applySnapshot(null, null);
    let tombstones = Object.create(null);
    let rowsLoading = null;

    // One object, replaced and never mutated, or null when no confirmation is
    // open: { seq, target, phase, report, html, errorText, drift }. Phases are
    // loading, preview, executing and error.
    let modal = null;
    let seq = 0;

    function getRows() {
      return rows;
    }

    function getModal() {
      return modal;
    }

    function setModal(next) {
      modal = next;
      deps.onChange();
    }

    function errorMessage(error) {
      if (error !== null && typeof error === "object" && typeof error.message === "string") {
        return error.message;
      }
      return String(error);
    }

    // The single entry point for a fetched GET /rows body. Tombstones are
    // pruned first, so a row the daemon keeps reporting comes back once its
    // suppression expires, then the body is filtered through what is left.
    function applyRows(body) {
      tombstones = removal.pruneTombstones(tombstones, deps.now());
      rows = removal.applySnapshot(body, tombstones);
      deps.onChange();
    }

    // GET /rows into applyRows. Calls made while one is in flight share it.
    function loadRows() {
      if (rowsLoading !== null) return rowsLoading;
      const loading = Promise.resolve(deps.request("/rows")).then(
        (body) => {
          if (rowsLoading === loading) rowsLoading = null;
          applyRows(body);
        },
        (error) => {
          if (rowsLoading === loading) rowsLoading = null;
          throw error;
        },
      );
      rowsLoading = loading;
      return loading;
    }

    // Record a removal the daemon reported: suppress its rows in every later
    // snapshot and drop them from the current one. Sessions are resolved
    // against the rows as they stood, since the report carries keys only.
    function commit(report) {
      const index = removal.rowsIndex(rows);
      const sessions = [];
      const removed = report !== null && typeof report === "object" && Array.isArray(report.rows_removed)
        ? report.rows_removed
        : [];
      for (const rowKey of removed) {
        const id = removal.rowKeyId(rowKey);
        const entry = id !== null ? index[id] : undefined;
        if (entry && entry.session_id !== null && entry.session_id !== undefined) {
          sessions.push(entry.session_id);
        }
      }
      tombstones = removal.tombstone(tombstones, report, deps.now());
      rows = removal.dropRows(rows, report);
      for (const sessionId of sessions) deps.onSessionGone(sessionId);
    }

    function renderReport(report) {
      const view = removal.confirmationView(report, removal.rowsIndex(rows), { home: null });
      return removal.renderConfirmation(view);
    }

    // Open the confirmation for one row. The request body is the bare
    // RemovalTarget the daemon's preview handler extracts, for example
    // {"checkout":7}: no wrapper object and no other field.
    function openPreview(rowKey) {
      // A removal already with the daemon keeps its confirmation on screen.
      if (modal !== null && modal.phase === "executing") return Promise.resolve();
      seq += 1;
      const mine = seq;
      setModal({
        seq: mine, target: rowKey, phase: "loading", report: null, html: "", errorText: null, drift: false,
      });
      return Promise.resolve(
        deps.request("/removals/preview", { method: "POST", body: JSON.stringify(rowKey) }),
      ).then(
        (report) => {
          // A closed or replaced confirmation takes no late answer.
          if (modal === null || modal.seq !== mine) return;
          setModal({ ...modal, phase: "preview", report, html: renderReport(report), errorText: null });
        },
        (error) => {
          if (modal === null || modal.seq !== mine) return;
          setModal({ ...modal, phase: "error", errorText: errorMessage(error) });
        },
      );
    }

    // Execute the report on screen. Nothing but the held report is ever
    // posted, and nothing is dropped until the daemon answers 200.
    function confirm() {
      if (modal === null || (modal.phase !== "preview" && modal.phase !== "error")) {
        return Promise.resolve();
      }
      if (modal.report === null) return Promise.resolve();
      const mine = modal.seq;
      const held = modal.report;
      setModal({ ...modal, phase: "executing", errorText: null });
      return Promise.resolve(
        deps.request("/removals", { method: "POST", body: JSON.stringify(held) }),
      ).then(
        (report) => {
          // Closed before the commit, so a route change made for a gone
          // session never finds this confirmation still open.
          if (modal !== null && modal.seq === mine) modal = null;
          commit(report);
          deps.notify(removal.removalNotice(report));
          for (const failure of removal.clearingFailures(report, { home: null })) {
            deps.notify(failure);
          }
          deps.onChange();
        },
        (error) => {
          if (modal === null || modal.seq !== mine) return;
          setModal({ ...modal, phase: "error", errorText: errorMessage(error) });
        },
      );
    }

    // The one cancel path: the Cancel button, Escape and a backdrop click. It
    // sends nothing, drops nothing and records nothing. While a removal is
    // executing the request cannot be recalled, so the confirmation stays up
    // until the daemon answers.
    function cancel() {
      if (modal === null || modal.phase === "executing") return;
      setModal(null);
    }

    return { getRows, getModal, applyRows, loadRows, openPreview, confirm, cancel };
  }

  // ---- the row list ----

  function isRecord(value) {
    return value !== null && typeof value === "object" && !Array.isArray(value);
  }

  function asArray(value) {
    return Array.isArray(value) ? value : [];
  }

  // A remove control for one durable row key, or nothing when the key is not
  // a safe integer. It is a sibling of the card, never a descendant of it: a
  // button inside a button is invalid HTML and a tap would mean two things.
  function removeControl(removal, kind, key, name) {
    if (!ROW_KINDS.includes(kind) || !Number.isSafeInteger(key) || key < 0) return "";
    const label = removal.escapeHtml(`remove ${name}`);
    return `<button type="button" class="rm" data-kind="${kind}" data-key="${key}" `
      + `aria-label="${label}" title="${label}">✕</button>`;
  }

  // A row with no live session: nothing to open, so nothing to tap.
  function plainLine(removal, row, archived) {
    const esc = removal.escapeHtml;
    const detail = [
      typeof row.branch === "string" && row.branch !== "" ? `⎇ ${esc(row.branch)}` : null,
      esc(row.path),
    ].filter(Boolean).join(" · ");
    return `<div class="rowline${archived ? " archived" : ""}">`
      + `<div class="line1"><span class="name">${esc(row.display_name)}</span></div>`
      + `<div class="line2">${detail}</div></div>`;
  }

  function cardFor(options, sessionId) {
    if (sessionId === null || sessionId === undefined) return null;
    if (!options || typeof options.card !== "function") return null;
    const html = options.card(sessionId);
    return typeof html === "string" && html !== "" ? html : null;
  }

  function checkoutHtml(removal, checkout, options) {
    const card = cardFor(options, checkout.session_id);
    const body = card !== null ? card : plainLine(removal, checkout, checkout.archived === true);
    return `<div class="rowwrap">${body}`
      + `${removeControl(removal, "checkout", checkout.checkout_key, checkout.display_name)}</div>`;
  }

  // The GET /rows body as the list's HTML: one heading per repository with
  // its checkouts beneath. `options.removal` is the BaudeRemoval object and
  // `options.card(sessionId)` returns the page's card HTML for a live session,
  // or null. Every daemon string goes through removal.escapeHtml.
  function renderRowsList(rowsBody, options) {
    const removal = options && options.removal ? options.removal : globalThis.BaudeRemoval;
    const esc = removal.escapeHtml;
    const body = isRecord(rowsBody) ? rowsBody : {};
    let html = "";
    for (const repository of asArray(body.repositories)) {
      if (!isRecord(repository)) continue;
      html += `<div class="repo"><div class="repohead">`
        + `<span class="name">${esc(repository.display_name)}</span>`
        + `<span class="path">${esc(repository.path)}</span></div>`;
      for (const checkout of asArray(repository.checkouts)) {
        if (isRecord(checkout)) html += checkoutHtml(removal, checkout, options);
      }
      html += "</div>";
    }
    return html;
  }

  const BaudeRemovalController = { createRemovalController, renderRowsList };

  globalThis.BaudeRemovalController = BaudeRemovalController;
  if (typeof module !== "undefined" && module.exports) {
    module.exports = BaudeRemovalController;
  }
})();
