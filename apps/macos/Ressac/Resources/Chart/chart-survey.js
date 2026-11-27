// The territory editor's page half: the edit session (one edit at a time, the undo history), the
// Cartographer's Log and the name form. The builders take their document as an argument, as chart-draw.js's
// do, so Node tests them with the shared fake document; nothing here touches the real page by itself.
// Feed titles and names reach the log through textContent and <option> text only, never as HTML.
(function (root, factory) {
  "use strict";
  if (typeof module === "object" && module.exports) module.exports = factory(require("./chart-core.js"));
  else root.ChartSurvey = factory(root.ChartCore);
})(typeof globalThis !== "undefined" ? globalThis : this, function (core) {
  "use strict";

  const T = core.EDIT_TEXT;

  // ---------- the edit session ----------
  // One rule for pending (« Names decided here »): an edit is pending from send to its result; a result that
  // is not ok ends it at once; an ok result keeps it pending until the next Chart.update (updated()) or
  // PENDING_MS after the result, whichever comes first. Nothing is sent while an edit is pending.
  // post(message) carries {type: "edit", seq, edit} to the app; onChange() follows every change of state.
  const hasLand = edit => edit && (edit.op !== "join" && edit.op !== "leave" ||
    (edit.land != null && typeof edit.land === "object" && Number.isInteger(edit.land.id)));

  function createSession({post, onChange, setTimeout: later} = {}) {
    const changed = () => { if (onChange) onChange(); };
    const wait = later || ((fn, ms) => globalThis.setTimeout(fn, ms));
    let h = core.history.empty(), seq = 0, pending = null; // pending = {seq, which, record, landed}

    function dispatch(edit, which, record) {
      if (pending || !hasLand(edit)) return false;
      const text = JSON.stringify(edit);
      if (typeof text !== "string" || core.editTooBig(text)) return false;
      seq += 1;
      pending = {seq, which, record, landed: false};
      post({type: "edit", seq, edit: text});
      changed();
      return true;
    }
    function step(which) {
      if (pending) return false;
      const s = core.history.step(h, which);
      return s ? dispatch(s.record.edit, which, s.record) : false;
    }
    // the record leaves its stack (a refused undo or redo: it can never apply again)
    function drop(which, record) {
      const list = h[which];
      h = {...h, [which]: list.filter(r => r !== record)};
    }

    function edited(text) {
      let result;
      try { result = typeof text === "string" ? JSON.parse(text) : text; } catch (_) { return null; }
      if (!result || typeof result !== "object" || !pending || pending.landed || result.seq !== pending.seq) return null;
      const {which, record} = pending;
      if (result.ok === true) {
        const done = result.done && typeof result.done === "object" ? result.done : null;
        const inverse = done && done.undo && typeof done.undo === "object";
        if (!which) h = core.history.record(h, done);
        else if (inverse) h = core.history.land(h, which, record, done);
        else drop(which, record);
        const mine = pending;
        mine.landed = true;
        wait(() => { if (pending === mine) { pending = null; changed(); } }, core.PENDING_MS);
        changed();
        return {kind: "done", message: null};
      }
      pending = null;
      const kind = core.editFailure(result);
      if (which && kind !== "failed") drop(which, record); // refused: drop it; failed (it did not run): keep it
      changed();
      return {kind, message: kind === "changed" ? T.changed : String(result.error ?? "")};
    }

    function updated() {
      if (pending && pending.landed) { pending = null; changed(); }
    }

    return Object.freeze({
      send: edit => dispatch(edit, null, null),
      edited,
      undo: () => step("undo"),
      redo: () => step("redo"),
      titles: () => core.history.titles(h),
      pending: () => pending !== null,
      updated,
    });
  }

  // ---------- DOM helpers ----------
  function make(doc, tag, className, text) {
    const el = doc.createElement(tag);
    if (className) el.className = className;
    if (text != null) el.textContent = text;
    return el;
  }
  function makeButton(doc, text, className, onClick) {
    const b = make(doc, "button", className, text);
    b.type = "button";
    b.addEventListener("click", e => { if (!b.disabled) onClick(e); });
    return b;
  }
  function option(doc, value, text, selected) {
    const o = make(doc, "option", null, text);
    o.value = value;
    if (selected) o.selected = true;
    return o;
  }
  const titleOf = f => (f.title || "").trim() || "(untitled)";
  const feeds = n => `${n} ${n === 1 ? "feed" : "feeds"}`;
  const call = (fn, ...args) => { if (typeof fn === "function") fn(...args); };

  // ---------- the name form ----------
  // A <form class="name-form"> headed by `title`: the name (40 characters at most), an optional `With:` choice of
  // [{id, label}] (no default; submit waits for one), the error line, the submit button and Cancel.
  // Enter or submit → on.submit(value, choice id | null); Esc or Cancel → on.cancel(). Returns {nodes, setError, input}.
  function nameForm(doc, {title, value, withChoices} = {}, on = {}) {
    const form = make(doc, "form", "name-form");
    form.setAttribute("novalidate", "");
    const head = make(doc, "p", "name-form-title", title ?? "");
    const input = make(doc, "input", "name-form-input");
    input.type = "text";
    input.setAttribute("maxlength", "40");
    input.setAttribute("autocomplete", "off");
    input.setAttribute("spellcheck", "false");
    if (title) input.setAttribute("aria-label", title);
    input.value = value ?? "";
    form.append(head, input);

    const choices = Array.isArray(withChoices) ? withChoices : null;
    let select = null;
    if (choices) {
      const label = make(doc, "label", "name-form-with");
      select = make(doc, "select", "name-form-choice");
      const blank = option(doc, "", "", true);
      blank.disabled = true;
      select.append(blank, ...choices.map(c => option(doc, String(c.id), String(c.label ?? ""))));
      select.value = "";
      label.append("With:", select);
      form.append(label);
    }
    const error = make(doc, "p", "name-form-error");
    error.setAttribute("aria-live", "polite");
    const submit = make(doc, "button", "name-form-ok", "OK");
    submit.type = "submit";
    const cancel = makeButton(doc, "Cancel", "name-form-cancel", () => call(on.cancel));
    const buttons = make(doc, "div", "name-form-buttons");
    buttons.append(cancel, submit);
    form.append(error, buttons);

    const chosen = () => {
      if (!choices) return null;
      const c = choices.find(x => String(x.id) === select.value);
      return c ? c.id : undefined;
    };
    const ready = () => chosen() !== undefined;
    submit.disabled = !ready();
    if (select) select.addEventListener("change", () => { submit.disabled = !ready(); });
    const trySubmit = () => { if (ready()) call(on.submit, input.value, chosen()); };
    form.addEventListener("submit", e => { if (e && e.preventDefault) e.preventDefault(); trySubmit(); });
    form.addEventListener("keydown", e => {
      if (!e || e.isComposing) return;
      if (e.key === "Escape") {
        if (e.preventDefault) e.preventDefault();
        if (e.stopPropagation) e.stopPropagation();
        call(on.cancel);
      } else if (e.key === "Enter" && e.target === input) {
        if (e.preventDefault) e.preventDefault(); // no implicit submission as well
        trySubmit();
      }
    });
    input.addEventListener("input", () => setError(null));
    function setError(text) { error.textContent = text == null ? "" : String(text); }
    return {nodes: [form], setError, input};
  }

  // ---------- reordering seas ----------
  // The row a dragged ≡ lands on: the first row whose middle is at or below the pointer, else the last.
  function seaDropIndex(rowMids, y) {
    const i = rowMids.findIndex(mid => mid >= y);
    return i === -1 ? rowMids.length - 1 : i;
  }

  // ---------- the Cartographer's Log ----------
  // logContent(doc, data, state, on): the drawer's contents for one snapshot. state = {selected, busy, message,
  // titles} (titles = session.titles(); a null title disables its button). Handlers: select(feed),
  // setSea(feed, seaId | null), setLand(feed, landId), leave(feed), newLand(name, [feed ids in chart order]),
  // renameLand(land, name), deleteLand(land) (a continent's too: back to open water), makeContinent(sea, name),
  // renameSea(sea, name), moveSea(sea, delta rows), newSea(name), deleteSea(sea), undo(), redo().
  // Names are checked (core.checkName) before a handler is called; a bad one keeps its form open with the text.
  function logContent(doc, data, state = {}, on = {}) {
    const busy = !!state.busy, ledger = (data && data.ledger) || {seas: [], lands: []};
    const index = core.ledgerIndex(data), chartFeeds = (data && data.feeds) || [];
    const order = new Map(chartFeeds.map((f, i) => [f.id, i]));
    const controls = [];
    const control = el => { controls.push(el); return el; };
    const btn = (text, className, onClick) => control(makeButton(doc, text, className, e => { if (!busy) onClick(e); }));

    // an inline name form in `slot`, replacing what it held until submitted or cancelled
    function openForm(slot, spec, accept) {
      const kept = [...slot.childNodes];
      const restore = () => slot.replaceChildren(...kept);
      const form = nameForm(doc, spec, {
        submit: (value, choice) => {
          const verdict = accept(value, choice);
          if (verdict && verdict.error) form.setError(verdict.error);
          else restore();
        },
        cancel: restore,
      });
      slot.replaceChildren(...form.nodes);
      if (form.input.focus) form.input.focus();
      return form;
    }
    // check a name against the ledger, then hand it on; an unchanged name sends nothing
    const named = (kind, selfId, current, send) => value => {
      const checked = core.checkName(value, kind, ledger, selfId);
      if (checked.error) return checked;
      if (current != null && checked.name === current) return null;
      send(checked.name);
      return null;
    };

    const aside = make(doc, "aside", "log");
    aside.setAttribute("aria-labelledby", "log-title");
    const title = make(doc, "h2", "log-title", "Cartographer's Log");
    title.id = "log-title";
    aside.append(title);

    // --- the selected island ---
    const block = make(doc, "section", "log-island-block");
    const feed = chartFeeds.find(f => f.id === state.selected) || null;
    const islandLabel = make(doc, "label", "log-field");
    const island = control(make(doc, "select", "log-island"));
    island.append(option(doc, "", "", !feed), ...chartFeeds.map(f => option(doc, String(f.id), titleOf(f), feed && f.id === feed.id)));
    island.value = feed ? String(feed.id) : "";
    island.addEventListener("change", () => { if (!busy && island.value !== "") call(on.select, Number(island.value)); });
    islandLabel.append("Island:", island);
    block.append(islandLabel);

    if (feed) {
      const sea = index.seaRef(feed.theme), continent = feed.theme ? index.continentOf(feed.theme) : null;
      const land = index.landOf(feed.id), hint = continent ? T.continentHint(continent.name) : null;
      block.append(make(doc, "h3", "log-island-title", titleOf(feed)));

      const seaLabel = make(doc, "label", "log-field"), seaSelect = control(make(doc, "select", "log-sea-select"));
      seaSelect.append(...(ledger.seas || []).map(t => option(doc, String(t.id), t.sea, sea && sea.id === t.id)),
        option(doc, "", core.UNCHARTED.name, !sea));
      seaSelect.value = sea ? String(sea.id) : "";
      seaSelect.addEventListener("change", () => {
        if (busy) return;
        const to = seaSelect.value === "" ? null : Number(seaSelect.value);
        if (to !== (sea ? sea.id : null)) call(on.setSea, feed.id, to);
      });
      seaLabel.append("Sea:", seaSelect);

      const landLabel = make(doc, "label", "log-field"), landSelect = control(make(doc, "select", "log-land-select"));
      if (continent) {
        landSelect.append(option(doc, String(continent.id), continent.name, true));
        landSelect.value = String(continent.id);
        landSelect.disabled = true;
        landSelect.title = hint;
      } else {
        if (!land) { const blank = option(doc, "", "", true); blank.disabled = true; landSelect.append(blank); }
        landSelect.append(...(ledger.lands || []).filter(l => !l.continent)
          .map(l => option(doc, String(l.id), l.name, land && land.id === l.id)));
        landSelect.value = land ? String(land.id) : "";
        landSelect.addEventListener("change", () => {
          if (busy || landSelect.value === "") return; // never a join without a land
          const to = Number(landSelect.value);
          if (!land || to !== land.id) call(on.setLand, feed.id, to);
        });
      }
      landLabel.append("Land:", landSelect);

      const actions = make(doc, "div", "log-island-actions");
      const leave = btn("Leave Land", "log-leave", () => call(on.leave, feed.id));
      if (continent) { leave.disabled = true; leave.title = hint; } else if (!land) leave.disabled = true;
      const others = chartFeeds.filter(f => f.id !== feed.id && !(f.theme && index.continentOf(f.theme)));
      const newLand = btn("New Land…", "log-new-land", () => openForm(actions,
        {title: "Name this land", value: "", withChoices: others.map(f => ({id: f.id, label: titleOf(f)}))},
        (value, other) => named("land", null, null, name => {
          const ids = [feed.id, other].sort((a, b) => (order.get(a) ?? 0) - (order.get(b) ?? 0));
          call(on.newLand, name, ids);
        })(value)));
      if (continent) { newLand.disabled = true; newLand.title = hint; } else if (!others.length) newLand.disabled = true;
      actions.append(leave, newLand);
      block.append(seaLabel, landLabel, actions);
    }
    aside.append(block);

    // --- Lands: every territory and continent ---
    const lands = make(doc, "section", "log-lands"), landList = make(doc, "ul", "log-list");
    lands.append(make(doc, "h3", null, "Lands"), landList);
    (ledger.lands || []).forEach(l => {
      const row = make(doc, "li", "log-land"), actions = make(doc, "span", "log-actions");
      const count = l.continent ? ((ledger.seas || []).find(t => t.key === l.continent) || {feeds: 0}).feeds : (l.feeds || []).length;
      const rename = btn("Rename", "log-rename", () => openForm(actions, {title: "Rename", value: l.name},
        named("land", l.id, l.name, name => call(on.renameLand, l.id, name))));
      const remove = l.continent
        ? btn("Back to Open Water", "log-unmake", () => call(on.deleteLand, l.id))
        : btn("Delete", "log-delete", () => call(on.deleteLand, l.id));
      actions.append(rename, remove);
      row.append(make(doc, "span", "log-name", l.name), make(doc, "span", "log-count", feeds(count)), actions);
      landList.append(row);
    });
    aside.append(lands);

    // --- Seas: every theme in display order; reordered by Alt+↑/↓ on a row or by dragging its ≡ ---
    const seas = make(doc, "section", "log-seas"), seaList = make(doc, "ol", "log-list");
    seas.append(make(doc, "h3", null, "Seas"), seaList);
    const seaRows = [], ledgerSeas = ledger.seas || [];
    ledgerSeas.forEach((t, i) => {
      const row = make(doc, "li", "log-sea"), actions = make(doc, "span", "log-actions");
      row.tabIndex = 0;
      row.setAttribute("aria-label", t.sea);
      row.setAttribute("aria-keyshortcuts", "Alt+ArrowUp Alt+ArrowDown");
      const handle = make(doc, "span", "log-handle", "≡");
      handle.setAttribute("aria-hidden", "true");
      const continent = index.continentOf(t.key);
      const rename = btn("Rename", "log-rename", () => openForm(actions, {title: "Rename", value: t.sea},
        named("sea", t.id, t.sea, name => call(on.renameSea, t.id, name))));
      actions.append(rename);
      if (!continent) {
        actions.append(btn("Make a continent", "log-make-continent", () => openForm(actions, {title: "Make a continent", value: ""},
          named("land", null, null, name => call(on.makeContinent, t.id, name)))));
      }
      actions.append(btn("Delete", "log-delete", () => {
        const kept = [...actions.childNodes];
        const question = make(doc, "span", "log-question", core.deleteSeaQuestion(t.sea, t.feeds || 0, !!continent));
        const yes = btn("Delete", "log-delete", () => call(on.deleteSea, t.id));
        const no = btn("Cancel", "log-cancel", () => actions.replaceChildren(...kept));
        actions.replaceChildren(question, yes, no);
        if (no.focus) no.focus();
      }));
      row.append(handle, make(doc, "span", "log-name", t.sea), make(doc, "span", "log-count", feeds(t.feeds || 0)), actions);

      row.addEventListener("keydown", e => {
        if (busy || !e || !e.altKey || (e.key !== "ArrowUp" && e.key !== "ArrowDown")) return;
        if (e.target && /^(INPUT|SELECT|TEXTAREA)$/.test(e.target.tagName || "")) return; // a rename field keeps its keys
        const delta = e.key === "ArrowUp" ? -1 : 1;
        if (i + delta < 0 || i + delta >= ledgerSeas.length) return;
        if (e.preventDefault) e.preventDefault();
        call(on.moveSea, t.id, delta);
      });
      let dragging = false;
      handle.addEventListener("pointerdown", e => {
        if (busy || (e && e.button != null && e.button !== 0)) return;
        dragging = true;
        row.className = "log-sea dragging";
        if (e && e.preventDefault) e.preventDefault();
        if (e && e.pointerId != null && handle.setPointerCapture) handle.setPointerCapture(e.pointerId);
      });
      const end = () => { dragging = false; row.className = "log-sea"; };
      handle.addEventListener("pointercancel", end);
      handle.addEventListener("pointerup", e => {
        if (!dragging) return;
        end();
        const own = row.getBoundingClientRect();
        if (e.clientY >= own.top && e.clientY < own.bottom) return; // released on its own row: nothing
        const mids = seaRows.map(r => { const b = r.getBoundingClientRect(); return (b.top + b.bottom) / 2; });
        const delta = seaDropIndex(mids, e.clientY) - i;
        if (!busy && delta !== 0) call(on.moveSea, t.id, delta);
      });
      seaRows.push(row);
      seaList.append(row);
    });
    const newSea = make(doc, "div", "log-new-sea");
    newSea.append(btn("New Sea…", "log-new-sea-button", () => openForm(newSea, {title: "New Sea", value: ""},
      named("sea", null, null, name => call(on.newSea, name)))));
    seas.append(newSea);
    aside.append(seas);

    // --- undo, redo and the status line ---
    const titles = state.titles || {undo: null, redo: null};
    const history = make(doc, "div", "log-history");
    const undo = btn(titles.undo ?? "Undo", "log-undo", () => call(on.undo));
    const redo = btn(titles.redo ?? "Redo", "log-redo", () => call(on.redo));
    if (titles.undo == null) undo.disabled = true;
    if (titles.redo == null) redo.disabled = true;
    history.append(undo, redo);
    const status = make(doc, "p", "log-status", state.message ?? "");
    status.setAttribute("role", "status");
    aside.append(history, status);

    if (busy) controls.forEach(c => { c.disabled = true; });
    return [aside];
  }

  return Object.freeze({createSession, logContent, nameForm, seaDropIndex});
});
