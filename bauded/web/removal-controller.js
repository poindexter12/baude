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

  // How long a removed row's live session stays out of a fetched /sessions
  // list. The same window removal.js gives a tombstone.
  const GONE_SESSION_TTL_MS = 60000;

  // A row key as Rust's Debug prints it, which is how the daemon's error text
  // names a row: Checkout(CheckoutKey(7)). The inner word has to match the
  // outer one. The optional lead is the whole word `row` and one space.
  const KEY_TOKEN = /(\brow )?(Repository|Checkout|Standalone)\(\2Key\(([0-9]+)\)\)/g;

  // The text a user reads for `text`: every key token swapped for the row's
  // label from `index`, or for the words `unknown row` when the index cannot
  // name it. Nothing else changes. Paths stay as the daemon sent them and a
  // text with no token comes back as it went in.
  function userText(removal, text, index) {
    if (typeof text !== "string") return "";
    // A function, never a replacement string: a branch may hold `$&`.
    return text.replace(KEY_TOKEN, (_token, _lead, word, digits) => {
      const key = Number(digits);
      if (!Number.isSafeInteger(key)) return "unknown row";
      const id = removal.rowKeyId({ [word.toLowerCase()]: key });
      const known = id !== null && index !== null && typeof index === "object"
        && Object.prototype.hasOwnProperty.call(index, id);
      const entry = known ? index[id] : null;
      if (!isRecord(entry) || typeof entry.display_name !== "string" || entry.display_name === "") {
        return "unknown row";
      }
      return typeof entry.branch === "string" && entry.branch !== ""
        ? `${entry.display_name} (${entry.branch})`
        : entry.display_name;
    });
  }

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

    // Live session id of a removed row -> when its suppression expires. The
    // /sessions poll is a second way a removed row could reappear.
    const goneSessions = new Map();

    // One object, replaced and never mutated, or null when no confirmation is
    // open: { seq, target, phase, report, html, errorText, drift, settled }.
    // Phases are loading, preview, executing and error. `settled` is set when
    // a rows_removed frame names the target of a removal still executing.
    let modal = null;
    let seq = 0;

    // The /events source in use, and whether its next open follows a gap in
    // which frames may have been missed.
    let events = null;
    let everSubscribed = false;
    let missedFrames = false;

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

    function statusOf(error) {
      return error !== null && typeof error === "object" ? error.status : undefined;
    }

    // The only two writers of text the user reads. Both pass it through
    // userText, so a Rust key token reaches neither the error line nor a
    // notice. setError names rows from the list as it stands; say takes the
    // index to use when the rows it should name have just been dropped.
    function setError(text) {
      setModal({ ...modal, phase: "error", errorText: userText(removal, text, removal.rowsIndex(rows)) });
    }

    function say(text, index) {
      deps.notify(userText(removal, text, index === undefined ? removal.rowsIndex(rows) : index));
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

    // GET /rows by a request that leaves now. A poll already in flight left
    // before whatever prompted this, so its body may predate it: that one is
    // let finish and a new request follows.
    function refetchRows() {
      const inFlight = rowsLoading;
      if (inFlight === null) return loadRows();
      return inFlight.then(() => loadRows(), () => loadRows());
    }

    // Record a removal the daemon reported: suppress its rows in every later
    // snapshot and drop them from the current one. Sessions are resolved
    // against the rows as they stood, since the report carries keys only.
    // The only writer of tombstones and of the gone-session map.
    function commit(report) {
      const index = removal.rowsIndex(rows);
      const now = deps.now();
      const sessions = [];
      const removed = isRecord(report) ? asArray(report.rows_removed) : [];
      for (const rowKey of removed) {
        const id = removal.rowKeyId(rowKey);
        const entry = id !== null ? index[id] : undefined;
        if (entry && entry.session_id !== null && entry.session_id !== undefined) {
          sessions.push(entry.session_id);
        }
      }
      tombstones = removal.tombstone(tombstones, report, now);
      rows = removal.dropRows(rows, report);
      for (const sessionId of sessions) goneSessions.set(sessionId, now + GONE_SESSION_TTL_MS);
      for (const sessionId of sessions) deps.onSessionGone(sessionId);
    }

    // A fetched GET /sessions list without the live sessions of rows removed
    // in the last minute. The argument is not changed.
    function filterSessions(list) {
      const now = deps.now();
      for (const [sessionId, expires] of goneSessions) {
        if (expires <= now) goneSessions.delete(sessionId);
      }
      if (!Array.isArray(list) || goneSessions.size === 0) return list;
      return list.filter((session) => !(isRecord(session) && goneSessions.has(session.id)));
    }

    function renderReport(report) {
      const view = removal.confirmationView(report, removal.rowsIndex(rows), { home: null });
      return removal.renderConfirmation(view);
    }

    // The removal the daemon reported is done: close the confirmation that
    // asked for it, drop the rows and say what happened. `index` is the list
    // as it stood before the rows went, so a notice can still name them.
    function succeed(mine, report, index) {
      // Closed before the commit, so a route change made for a gone session
      // never finds this confirmation still open.
      if (modal !== null && modal.seq === mine) modal = null;
      commit(report);
      let notice = removal.removalNotice(report);
      // Saved, with a caveat the daemon put into words. Still a success.
      const persistence = isRecord(report) && isRecord(report.persistence) ? report.persistence : {};
      if (persistence.outcome === "saved_degraded" && typeof persistence.detail === "string") {
        notice = `${notice}; ${persistence.detail}`;
      }
      say(notice, index);
      for (const failure of removal.clearingFailures(report, { home: null })) say(failure, index);
      deps.onChange();
    }

    // The daemon answered 404: the target is not in its saved state, so
    // there is nothing to remove and nothing to retry. No removal.js helper
    // takes a bare key, so the target is wrapped in the shape commit reads.
    // That object is never rendered and never sent anywhere.
    function alreadyGone() {
      const target = modal.target;
      const index = removal.rowsIndex(rows);
      modal = null;
      commit({
        target,
        planned: false,
        rows_removed: [target],
        runtimes_killed: [],
        paths_deleted: [],
        paths_kept: [],
        already_removed: [],
        readers_parked: [],
        cleared: [],
        clearing_failures: [],
      });
      say("already removed", index);
      deps.onChange();
      // The children of a repository that went converge from the daemon.
      refetchRows().catch(() => {});
    }

    // Another client removed the row this confirmation is about.
    function removedElsewhere(index) {
      modal = null;
      say("removed elsewhere", index);
      deps.onChange();
    }

    // The daemon answered 409. Only this status carries a report: the plan
    // as it stands now, which no longer matches the one on screen. It may
    // name rows this page has never listed, so the list is fetched again
    // before the fresh report is drawn, and the report and its rendering are
    // swapped together. Nothing is confirmed on the user's behalf.
    function conflict(mine, body) {
      const drift = removal.parseDriftBody(body);
      if (drift.report === null) {
        setError(drift.text);
        return Promise.resolve();
      }
      if (modal.settled) {
        removedElsewhere(modal.settled.index);
        return Promise.resolve();
      }
      // Back to loading with the old plan still drawn: Confirm is off, Cancel
      // works, and a frame naming the target closes it as removed elsewhere.
      setModal({ ...modal, phase: "loading", errorText: null });
      return refetchRows().catch(() => {}).then(() => {
        if (modal === null || modal.seq !== mine) return;
        setModal({
          ...modal,
          phase: "preview",
          report: drift.report,
          html: renderReport(drift.report),
          errorText: null,
          drift: true,
        });
      });
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
        seq: mine,
        target: rowKey,
        phase: "loading",
        report: null,
        html: "",
        errorText: null,
        drift: false,
        settled: null,
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
          if (statusOf(error) === 404) alreadyGone();
          else setError(errorMessage(error));
        },
      );
    }

    // Execute the report on screen, or try again from an error. Nothing but
    // the held report is ever posted, and nothing is dropped until the daemon
    // says the rows are gone. With no report held, the preview that failed is
    // what gets asked for again.
    function confirm() {
      if (modal === null || (modal.phase !== "preview" && modal.phase !== "error")) {
        return Promise.resolve();
      }
      if (modal.report === null) return openPreview(modal.target);
      const mine = modal.seq;
      const held = modal.report;
      setModal({ ...modal, phase: "executing", errorText: null, settled: null });
      return Promise.resolve(
        deps.request("/removals", { method: "POST", body: JSON.stringify(held) }),
      ).then(
        (report) => {
          // A frame that got here first dropped the rows already; the index
          // it took is the one that can still name them.
          const settled = modal !== null && modal.seq === mine ? modal.settled : null;
          succeed(mine, report, settled ? settled.index : removal.rowsIndex(rows));
        },
        (error) => {
          if (modal === null || modal.seq !== mine) return undefined;
          const status = statusOf(error);
          if (status === 409) return conflict(mine, errorMessage(error));
          // Only a 404 means the row is gone. A 503 never does.
          if (status === 404) return alreadyGone();
          // The frame for this removal arrived, so it happened: a lost or
          // failed response does not make it an error.
          if (modal.settled) return succeed(mine, modal.settled.report, modal.settled.index);
          return setError(errorMessage(error));
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

    // One rows_removed frame from GET /events: the RemovalReport of a removal
    // some client completed, possibly this one. A frame only ever subtracts
    // rows, and one that cannot be read subtracts none.
    function onRowsRemoved(data) {
      let report = null;
      try {
        report = JSON.parse(data);
      } catch (_) {
        report = null;
      }
      if (!isRecord(report) || !Array.isArray(report.rows_removed) || report.planned === true) {
        loadRows().catch(() => {});
        return;
      }
      const index = removal.rowsIndex(rows);
      const open = modal;
      const names = open !== null
        && report.rows_removed.some((rowKey) => removal.keyEquals(rowKey, open.target));
      if (names && open.phase === "executing") {
        // No order is promised between this frame and the POST response, so
        // the response still closes the confirmation and words the notice.
        modal = { ...open, settled: { report, index } };
        commit(report);
        deps.onChange();
        return;
      }
      if (names) {
        modal = null;
        commit(report);
        removedElsewhere(index);
        return;
      }
      commit(report);
      deps.onChange();
    }

    function unsubscribe() {
      if (events === null) return;
      const source = events;
      events = null;
      source.close();
    }

    // Open GET /events, replacing any source already open, and return it.
    // Listeners are registered by name: the daemon sends the named event
    // rows_removed, which never reaches onmessage. The stream has no replay,
    // so an open that follows an error, or a source replacing an earlier one,
    // reconciles through GET /rows.
    function subscribe() {
      unsubscribe();
      const source = deps.openEvents("/events");
      events = source;
      missedFrames = everSubscribed;
      everSubscribed = true;
      source.addEventListener("rows_removed", (event) => {
        if (events === source) onRowsRemoved(event.data);
      });
      source.addEventListener("open", () => {
        if (events !== source || !missedFrames) return;
        missedFrames = false;
        loadRows().catch(() => {});
      });
      source.addEventListener("error", () => {
        if (events === source) missedFrames = true;
      });
      return source;
    }

    return {
      getRows,
      getModal,
      applyRows,
      loadRows,
      openPreview,
      confirm,
      cancel,
      filterSessions,
      subscribe,
      unsubscribe,
    };
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
    // A repository and its own checkout share a name, and removing one is
    // not removing the other, so the larger one says what it is.
    const label = removal.escapeHtml(kind === "repository" ? `remove repository ${name}` : `remove ${name}`);
    return `<button type="button" class="rm" data-kind="${kind}" data-key="${key}" `
      + `aria-label="${label}" title="${label}">✕</button>`;
  }

  // A row with no live session: nothing to open, so nothing to tap.
  function plainLine(removal, row) {
    const esc = removal.escapeHtml;
    const detail = [
      typeof row.branch === "string" && row.branch !== "" ? `⎇ ${esc(row.branch)}` : null,
      esc(row.path),
    ].filter(Boolean).join(" · ");
    return `<div class="rowline">`
      + `<div class="line1"><span class="name">${esc(row.display_name)}</span></div>`
      + `<div class="line2">${detail}</div></div>`;
  }

  function cardFor(options, sessionId) {
    if (sessionId === null || sessionId === undefined) return null;
    if (!options || typeof options.card !== "function") return null;
    const html = options.card(sessionId);
    return typeof html === "string" && html !== "" ? html : null;
  }

  // What a checkout is, in the two words a user has for it. The repository's
  // own checkout is `main` whichever of its two roles the daemon gave it.
  function roleLabel(role) {
    if (role === "main" || role === "primary_default") return "main";
    if (role === "managed_branch") return "worktree";
    return "";
  }

  // The rows of one group in the order served, archived ones after the rest.
  // A new array: the body is never reordered.
  function archivedLast(list) {
    const rows = asArray(list).filter(isRecord);
    return rows.filter((row) => row.archived !== true).concat(rows.filter((row) => row.archived === true));
  }

  // One row: its card when it has a live session the page knows, else its
  // name and path, then what it is, then its remove control.
  function rowHtml(removal, kind, key, row, role, options) {
    const card = cardFor(options, row.session_id);
    const body = card !== null ? card : plainLine(removal, row);
    const tag = role !== "" ? `<span class="role">${removal.escapeHtml(role)}</span>` : "";
    return `<div class="rowwrap${row.archived === true ? " archived" : ""}">${body}${tag}`
      + `${removeControl(removal, kind, key, row.display_name)}</div>`;
  }

  // The GET /rows body as the list's HTML: each repository as a heading with
  // its checkouts beneath, then folders, then the sessions no row claims.
  // Options:
  //   removal   the BaudeRemoval object
  //   card      card(sessionId) returns the page's card HTML for a live
  //             session, or null when the page holds no such session
  //   keyless   sessions matching no /rows session_id; listed last as cards
  //             with no remove control, since they have no key to remove by
  // Every daemon string goes through removal.escapeHtml.
  function renderRowsList(rowsBody, options) {
    const removal = options && options.removal ? options.removal : globalThis.BaudeRemoval;
    const esc = removal.escapeHtml;
    const body = isRecord(rowsBody) ? rowsBody : {};
    let html = "";
    for (const repository of asArray(body.repositories)) {
      if (!isRecord(repository)) continue;
      html += `<div class="repo"><div class="repohead"><div class="repotitle">`
        + `<span class="name">${esc(repository.display_name)}</span>`
        + `<span class="path">${esc(repository.path)}</span></div>`
        + `${removeControl(removal, "repository", repository.repository_key, repository.display_name)}`
        + `</div>`;
      for (const checkout of archivedLast(repository.checkouts)) {
        html += rowHtml(removal, "checkout", checkout.checkout_key, checkout, roleLabel(checkout.role), options);
      }
      html += "</div>";
    }
    const folders = archivedLast(body.standalone);
    if (folders.length > 0) {
      html += `<div class="secthead">folders</div>`;
      for (const folder of folders) {
        html += rowHtml(removal, "standalone", folder.standalone_key, folder, "", options);
      }
    }
    const others = asArray(options ? options.keyless : null)
      .map((session) => (isRecord(session) ? cardFor(options, session.id) : null))
      .filter((card) => card !== null);
    if (others.length > 0) {
      html += `<div class="secthead">other sessions</div>${others.join("")}`;
    }
    return html;
  }

  const BaudeRemovalController = { createRemovalController, renderRowsList };

  globalThis.BaudeRemovalController = BaudeRemovalController;
  if (typeof module !== "undefined" && module.exports) {
    module.exports = BaudeRemovalController;
  }
})();
