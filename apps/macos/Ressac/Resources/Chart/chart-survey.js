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
  const REPLY_MS = 15000; // an edit the app never answers stops being pending after this long
  const NO_REPLY = "The app did not answer; the chart was reloaded.";
  const lateReply = what => `The app answered late: ${what} was done.`; // the status after a late ok reply
  const SURVEY_MESSAGE = Object.freeze({type: "survey"}); // asks the app for a fresh snapshot (Chart.update)

  // ---------- the edit session ----------
  // One rule for pending (« Names decided here »): an edit is pending from send to its result; a result that
  // is not ok ends it at once; an ok result keeps it pending until the next Chart.update (updated()) or
  // PENDING_MS after the result, whichever comes first. Nothing is sent while an edit is pending. An edit with
  // no result REPLY_MS after send ends too: the session posts {type: "survey"} and calls
  // onTimeout({kind: "timeout", message: NO_REPLY}). The app may still apply it (it waited behind other work): a late
  // ok result is recorded as a send or a step would have been, and edited() answers {kind: "late", message, edit}, so the
  // history stays exact; a late refusal or failure only forgets it. A late result never touches the edit pending now.
  // post(message) carries {type: "edit", seq, edit} to the app; onChange() follows every change of state.
  const hasLand = edit => edit && (edit.op !== "join" && edit.op !== "leave" ||
    (edit.land != null && typeof edit.land === "object" && Number.isInteger(edit.land.id)));
  const emptyPatch = edit => !edit || typeof edit !== "object" || (edit.op === "patch" && !(edit.changes || []).length);

  function createSession({post, onChange, onTimeout, setTimeout: later} = {}) {
    const changed = () => { if (onChange) onChange(); };
    const wait = later || ((fn, ms) => globalThis.setTimeout(fn, ms));
    let h = core.history.empty(), seq = 0, pending = null; // pending = {seq, which, record, landed, edit}
    const late = new Map(); // seq → the dispatch that timed out, until its result comes

    function dispatch(edit, which, record) {
      if (pending || !hasLand(edit)) return false;
      const text = JSON.stringify(edit);
      if (typeof text !== "string" || core.editTooBig(text)) return false;
      seq += 1;
      const mine = pending = {seq, which, record, landed: false, edit};
      post({type: "edit", seq, edit: text});
      wait(() => {
        if (pending !== mine || mine.landed) return;
        pending = null; // an unanswered undo or redo keeps its record, as a failed one does
        late.set(mine.seq, mine);
        post({...SURVEY_MESSAGE});
        if (onTimeout) onTimeout({kind: "timeout", message: NO_REPLY});
        changed();
      }, REPLY_MS);
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
      h = {...h, [which]: h[which].filter(r => r !== record)};
    }

    // an ok result's step: a send is recorded, an undo or redo moves its record to the other stack
    function keep(which, record, done) {
      if (!which) h = core.history.record(h, done);
      else if (done && !emptyPatch(done.undo)) h = core.history.land(h, which, record, done);
      else drop(which, record); // nothing to step back to: no empty step on the other stack
    }
    function lateResult(result) {
      const {which, record, edit} = late.get(result.seq);
      late.delete(result.seq);
      if (result.ok !== true) return null;
      const done = result.done && typeof result.done === "object" ? result.done : null;
      keep(which, record, done);
      changed();
      const what = which ? `${which === "undo" ? "Undo" : "Redo"} ${record.label}` : String(done?.label ?? "the edit");
      return {kind: "late", message: lateReply(what), edit};
    }

    function edited(text) {
      let result;
      try { result = typeof text === "string" ? JSON.parse(text) : text; } catch (_) { return null; }
      if (!result || typeof result !== "object") return null;
      if (late.has(result.seq)) return lateResult(result);
      if (!pending || pending.landed || result.seq !== pending.seq) return null;
      const {which, record, edit} = pending;
      if (result.ok === true) {
        keep(which, record, result.done && typeof result.done === "object" ? result.done : null);
        const mine = pending;
        mine.landed = true;
        wait(() => { if (pending === mine) { pending = null; changed(); } }, core.PENDING_MS);
        changed();
        return {kind: "done", message: null, edit}; // the edit that was done (an undo or redo: its patch)
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
  const keyed = (el, key) => { if (key) el.setAttribute("data-key", key); return el; };
  const keyOf = el => (el && typeof el.getAttribute === "function" ? el.getAttribute("data-key") : null);
  function findKey(nodes, key) {
    for (const n of nodes) {
      if (!n || n.nodeType !== 1) continue;
      if (keyOf(n) === key) return n;
      const inner = findKey(n.childNodes ? [...n.childNodes] : [], key);
      if (inner) return inner;
    }
    return null;
  }
  const titleOf = f => (f.title || "").trim() || "(untitled)";
  const feeds = n => `${n} ${n === 1 ? "feed" : "feeds"}`;
  const call = (fn, ...args) => { if (typeof fn === "function") fn(...args); };
  const stop = e => { if (e && e.preventDefault) e.preventDefault(); };

  // ---------- the name form ----------
  // A <form class="name-form"> headed by `title`: the name (40 characters at most), an optional `With:` choice of
  // [{id, label}] (no default; submit waits for one), the error line, Cancel and the submit button named by
  // `action` (a verb: `Rename`, `Create`). `choice` and `error` restore a form rebuilt from memory; `key` gives its
  // controls data-key attributes; `disabled` disables them all. Enter or submit → on.submit(value, choice id | null);
  // Esc or Cancel → on.cancel(); typing, choosing or moving the caret → on.change({value, start, end, choice}).
  // Returns {nodes, setError, input}.
  function nameForm(doc, {title, value, withChoices, action, choice, error: firstError, key, disabled} = {}, on = {}) {
    const sub = name => (key ? `${key}:${name}` : null);
    const form = make(doc, "form", "name-form");
    form.setAttribute("novalidate", "");
    const head = make(doc, "p", "name-form-title", title ?? "");
    const input = keyed(make(doc, "input", "name-form-input"), sub("input"));
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
      select = keyed(make(doc, "select", "name-form-choice"), sub("choice"));
      const known = choices.some(c => c.id === choice);
      const blank = option(doc, "", "", !known);
      blank.disabled = true;
      select.append(blank, ...choices.map(c => option(doc, String(c.id), String(c.label ?? ""), known && c.id === choice)));
      select.value = known ? String(choice) : "";
      label.append("With:", select);
      form.append(label);
    }
    const error = make(doc, "p", "name-form-error", firstError ?? "");
    error.setAttribute("aria-live", "polite");
    const submit = keyed(make(doc, "button", "name-form-ok", action || "Create"), sub("ok"));
    submit.type = "submit";
    const cancel = keyed(makeButton(doc, "Cancel", "name-form-cancel", () => call(on.cancel)), sub("cancel"));
    const buttons = make(doc, "div", "name-form-buttons");
    buttons.append(cancel, submit);
    form.append(error, buttons);

    const chosen = () => {
      if (!choices) return null;
      const c = choices.find(x => String(x.id) === select.value);
      return c ? c.id : undefined;
    };
    const ready = () => !disabled && chosen() !== undefined;
    submit.disabled = !ready();
    if (disabled) [input, cancel, select].forEach(c => { if (c) c.disabled = true; });
    const report = () => call(on.change, {value: input.value, start: input.selectionStart ?? null,
      end: input.selectionEnd ?? null, choice: chosen() ?? null});
    if (select) select.addEventListener("change", () => { submit.disabled = !ready(); report(); });
    const trySubmit = () => { if (ready()) call(on.submit, input.value, chosen()); };
    form.addEventListener("submit", e => { stop(e); trySubmit(); });
    form.addEventListener("keydown", e => {
      if (!e || e.isComposing) return;
      if (e.key === "Escape") {
        stop(e);
        if (e.stopPropagation) e.stopPropagation();
        call(on.cancel);
      } else if (e.key === "Enter" && e.target === input) {
        stop(e); // no implicit submission as well
        trySubmit();
      }
    });
    input.addEventListener("input", () => { setError(null); report(); });
    for (const type of ["keyup", "select", "click"]) input.addEventListener(type, report);
    function setError(text) { error.textContent = text == null ? "" : String(text); }
    return {nodes: [form], setError, input};
  }

  // ---------- reordering seas ----------
  // The row under the pointer, given the rows' middles (top to bottom): the boundary between two rows is
  // halfway between their middles; above every row is the first, below every row the last.
  function seaDropIndex(rowMids, y) {
    if (!rowMids.length) return -1;
    return rowMids.slice(1).filter((mid, j) => (rowMids[j] + mid) / 2 <= y).length;
  }

  // ---------- what the log keeps between rebuilds ----------
  // The page rebuilds the log on every Chart.update; one memory, made once, carries across rebuilds the open
  // inline form (its key, typed value, caret, choice, error, and whether it was sent), the focused control and the
  // folded sections (collapsed.lands, collapsed.seas: open at first, never saved beyond this memory).
  // After putting a rebuilt log in the page, call restore(): it gives the focus back (and the caret).
  // Feed it each edit's outcome with result({kind, message}): a sent form closes when its edit is done or the
  // chart changed, and stays open with the message on any other outcome (a name refusal from the core above all).
  function logMemory() {
    const m = {
      open: null, // {key, value, start, end, choice, error, sent}
      focus: null, // data-key of the control to focus
      lost: false, // the focus could not be restored (a disabled control): try again on the next rebuild
      collapsed: {lands: false, seas: false},
      target: null,
      capture(doc) {
        const a = doc && doc.activeElement, key = keyOf(a);
        if (key && key.startsWith("log:")) {
          m.focus = key; m.lost = false;
          if (m.open && key === `${m.open.key}:input`) {
            m.open.value = a.value;
            m.open.start = a.selectionStart ?? m.open.start;
            m.open.end = a.selectionEnd ?? m.open.end;
          }
        } else if ((a && a !== doc.body) || !m.lost) {
          m.focus = null; m.lost = false; // the focus moved elsewhere: leave it there
        }
      },
      result(outcome) {
        if (!m.open || !m.open.sent || !outcome) return;
        if (outcome.kind === "done" || outcome.kind === "changed") { m.open = null; return; }
        m.open.sent = false;
        m.open.error = outcome.message ?? null;
      },
      restore() {
        const el = m.target;
        m.target = null;
        if (!el) return null;
        if (el.disabled) { m.lost = true; return null; }
        if (typeof el.focus === "function") el.focus({preventScroll: true}); // a scrolled log stays where it was
        m.lost = false;
        if (m.open && keyOf(el) === `${m.open.key}:input` && m.open.start != null && typeof el.setSelectionRange === "function") {
          el.setSelectionRange(m.open.start, m.open.end ?? m.open.start);
        }
        return el;
      },
    };
    return m;
  }

  // ---------- the Cartographer's Log ----------
  // logContent(doc, data, state, on, memory): the drawer's contents for one snapshot. state = {selected, busy,
  // message, titles} (titles = session.titles(); a null title disables its button). memory = logMemory(), kept by
  // the caller (without it, nothing survives a rebuild). Handlers: select(feed), setSea(feed, seaId | null),
  // setLand(feed, landId), leave(feed), newLand(name, [feed ids in chart order]), renameLand(land, name),
  // deleteLand(land) (a continent's too: back to open water), makeContinent(sea, name), renameSea(sea, name),
  // moveSea(sea, delta rows), newSea(name), deleteSea(sea), undo(), redo().
  // Names are checked (core.checkName) before a handler is called; a bad one keeps its form open with the text.
  // Rows carry data-land / data-sea ids; every control carries a data-key ("log:…") the focus is restored by.
  function logContent(doc, data, state = {}, on = {}, memory = logMemory()) {
    memory.capture(doc);
    const busy = !!state.busy, ledger = (data && data.ledger) || {seas: [], lands: []};
    const index = core.ledgerIndex(data), chartFeeds = (data && data.feeds) || [];
    const order = new Map(chartFeeds.map((f, i) => [f.id, i]));
    const controls = [];
    const control = el => { controls.push(el); return el; };
    const btn = (text, className, key, onClick) =>
      control(keyed(makeButton(doc, text, className, e => { if (!busy) onClick(e); }), key));

    // inline forms: one slot per form key; at most one open; the open one lives in memory across rebuilds
    const slots = new Map();
    let current = null; // {key, close}
    function open(key, local) {
      const s = slots.get(key);
      if (!s) return;
      if (current && current.key !== key) current.close(false);
      if (!memory.open || memory.open.key !== key) memory.open = {key, value: null, start: null, end: null, choice: null, error: null, sent: false};
      const kept = [...s.slot.childNodes];
      const close = refocus => {
        s.slot.replaceChildren(...kept);
        if (current && current.key === key) current = null;
        if (memory.open && memory.open.key === key) memory.open = null;
        if (refocus && s.opener && typeof s.opener.focus === "function") s.opener.focus();
      };
      current = {key, close};
      const built = s.build(close, memory.open);
      s.slot.replaceChildren(...built.nodes);
      if (local && built.first && typeof built.first.focus === "function") built.first.focus();
    }
    // a name form: check the name against the ledger, then hand it on; an unchanged name sends nothing
    function nameSlot(key, slot, opener, {title, value, action, kind, selfId, current: unchanged, withChoices}, send) {
      slots.set(key, {slot, opener, build: (close, o) => {
        const form = nameForm(doc, {title, value: o.value ?? value, withChoices, action, choice: o.choice, error: o.error, key, disabled: busy}, {
          change: s => { o.value = s.value; o.start = s.start; o.end = s.end; o.choice = s.choice; },
          cancel: () => close(true),
          submit: (raw, choice) => {
            const checked = core.checkName(raw, kind, ledger, selfId);
            if (checked.error) { o.error = checked.error; o.value = raw; form.setError(checked.error); return; }
            if (unchanged != null && checked.name === unchanged) { close(true); return; }
            Object.assign(o, {value: raw, choice, error: null, sent: true});
            form.setError(null);
            send(checked.name, choice);
          },
        });
        controls.push(form.input);
        return {nodes: form.nodes, first: form.input};
      }});
    }

    const aside = make(doc, "aside", "log");
    aside.setAttribute("aria-labelledby", "log-title");
    const title = make(doc, "h2", "log-title", "Cartographer's Log");
    title.id = "log-title";
    aside.append(title);

    // --- the selected island ---
    const block = make(doc, "section", "log-island-block");
    const feed = chartFeeds.find(f => f.id === state.selected) || null;
    const islandLabel = make(doc, "label", "log-field");
    const island = control(keyed(make(doc, "select", "log-island"), "log:island"));
    island.append(option(doc, "", "", !feed), ...chartFeeds.map(f => option(doc, String(f.id), titleOf(f), feed && f.id === feed.id)));
    island.value = feed ? String(feed.id) : "";
    island.addEventListener("change", () => { if (!busy && island.value !== "") call(on.select, Number(island.value)); });
    islandLabel.append("Island:", island);
    block.append(islandLabel);

    if (feed) {
      const sea = index.seaRef(feed.theme), continent = feed.theme ? index.continentOf(feed.theme) : null;
      const land = index.landOf(feed.id), hint = continent ? T.continentHint(continent.name) : null;
      block.append(make(doc, "h3", "log-island-title", titleOf(feed)));

      const seaLabel = make(doc, "label", "log-field");
      const seaSelect = control(keyed(make(doc, "select", "log-sea-select"), "log:island:sea"));
      seaSelect.append(...(ledger.seas || []).map(t => option(doc, String(t.id), t.sea, sea && sea.id === t.id)),
        option(doc, "", core.UNCHARTED.name, !sea));
      seaSelect.value = sea ? String(sea.id) : "";
      seaSelect.addEventListener("change", () => {
        if (busy) return;
        const to = seaSelect.value === "" ? null : Number(seaSelect.value);
        if (to !== (sea ? sea.id : null)) call(on.setSea, feed.id, to);
      });
      seaLabel.append("Sea:", seaSelect);

      const landLabel = make(doc, "label", "log-field");
      const landSelect = control(keyed(make(doc, "select", "log-land-select"), "log:island:land"));
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
      const leave = btn("Leave Land", "log-leave", "log:island:leave", () => call(on.leave, feed.id));
      if (continent) { leave.disabled = true; leave.title = hint; } else if (!land) leave.disabled = true;
      const others = chartFeeds.filter(f => f.id !== feed.id && !(f.theme && index.continentOf(f.theme)));
      const formKey = `log:new-land:${feed.id}`;
      const newLand = btn("New Land…", "log-new-land", "log:island:new-land", () => open(formKey, true));
      if (continent) { newLand.disabled = true; newLand.title = hint; } else if (!others.length) newLand.disabled = true;
      actions.append(leave, newLand);
      if (!newLand.disabled) {
        nameSlot(formKey, actions, newLand, {title: "Name this land", value: "", action: "Create", kind: "land",
          withChoices: others.map(f => ({id: f.id, label: titleOf(f)}))}, (name, other) => {
          const ids = [feed.id, other].sort((a, b) => (order.get(a) ?? 0) - (order.get(b) ?? 0));
          call(on.newLand, name, ids);
        });
      }
      block.append(seaLabel, landLabel, actions);
    }
    aside.append(block);

    // --- the section headers: "Lands · 12" on a disclosure button folding the section's body (not an edit: it
    // works while one is on its way); memory.collapsed keeps the fold across rebuilds ---
    function section(className, name, which, count) {
      const box = make(doc, "section", className), head = make(doc, "h3", "log-section-head");
      const body = make(doc, "div", "log-section-body");
      body.id = `${className}-body`;
      const toggle = keyed(make(doc, "button", "log-disclosure"), `log:${which}:toggle`);
      toggle.type = "button";
      toggle.setAttribute("aria-controls", body.id);
      const mark = make(doc, "span", "log-disclosure-mark");
      mark.setAttribute("aria-hidden", "true");
      toggle.append(mark, make(doc, "span", "log-section-name", `${name} \u00B7 ${count}`));
      const show = () => {
        const open = !memory.collapsed[which];
        toggle.setAttribute("aria-expanded", String(open));
        mark.textContent = open ? "\u25BE" : "\u25B8"; // ▾ open, ▸ folded
        body.hidden = !open;
      };
      toggle.addEventListener("click", () => { memory.collapsed[which] = !memory.collapsed[which]; show(); });
      show();
      head.append(toggle);
      box.append(head, body);
      return {box, body};
    }

    // --- Lands: every territory and continent ---
    const {box: lands, body: landsBody} = section("log-lands", "Lands", "lands", (ledger.lands || []).length);
    const landList = make(doc, "ul", "log-list");
    landsBody.append(landList);
    (ledger.lands || []).forEach(l => {
      const row = make(doc, "li", "log-land"), actions = make(doc, "span", "log-actions"), k = `log:land:${l.id}`;
      row.setAttribute("data-land", String(l.id));
      const count = l.continent ? ((ledger.seas || []).find(t => t.key === l.continent) || {feeds: 0}).feeds : (l.feeds || []).length;
      const rename = btn("Rename", "log-rename", `${k}:rename`, () => open(`${k}:rename-form`, true));
      nameSlot(`${k}:rename-form`, actions, rename, {title: "Rename", value: l.name, action: "Rename", kind: "land",
        selfId: l.id, current: l.name}, name => call(on.renameLand, l.id, name));
      const remove = l.continent
        ? btn("Back to Open Water", "log-unmake", `${k}:unmake`, () => call(on.deleteLand, l.id))
        : btn("Delete", "log-delete", `${k}:delete`, () => call(on.deleteLand, l.id));
      actions.append(rename, remove);
      row.append(make(doc, "span", "log-name", l.name), make(doc, "span", "log-count", feeds(count)), actions);
      landList.append(row);
    });
    aside.append(lands);

    // --- Seas: every theme in display order; reordered by Alt+↑/↓ on a row or by dragging its ≡ ---
    const ledgerSeas = ledger.seas || [];
    const {box: seas, body: seasBody} = section("log-seas", "Seas", "seas", ledgerSeas.length);
    const seaList = make(doc, "ol", "log-list");
    // while a ≡ is dragged, an ink line between rows marks where the sea will land (absolute in the body: no row moves)
    const dropLine = make(doc, "div", "log-drop-line");
    dropLine.setAttribute("aria-hidden", "true");
    dropLine.hidden = true;
    seasBody.append(seaList, dropLine);
    const seaRows = [];
    // the row a sea dragged from row `from` lands on for the pointer of `e` (the same answer for the line and the
    // release), or null when the pointer is off the list, left or right
    const dropAt = e => {
      const box = seaList.getBoundingClientRect();
      if (box.width > 0 && e.clientX != null && (e.clientX < box.left || e.clientX > box.right)) return null;
      const rects = seaRows.map(r => r.getBoundingClientRect());
      return {to: seaDropIndex(rects.map(b => (b.top + b.bottom) / 2), e.clientY), rects};
    };
    // above the row it passes moving up, below it moving down; none over its own row
    const showDropLine = (from, e) => {
      const d = dropAt(e);
      if (!d || d.to < 0 || d.to === from) { dropLine.hidden = true; return; }
      const edge = d.to < from ? d.rects[d.to].top : d.rects[d.to].bottom;
      dropLine.style.top = `${edge - seasBody.getBoundingClientRect().top}px`;
      dropLine.hidden = false;
    };
    ledgerSeas.forEach((t, i) => {
      const row = keyed(make(doc, "li", "log-sea"), `log:sea:${t.id}`), actions = make(doc, "span", "log-actions");
      const k = `log:sea:${t.id}`;
      row.setAttribute("data-sea", String(t.id));
      row.tabIndex = 0;
      row.setAttribute("aria-keyshortcuts", "Alt+ArrowUp Alt+ArrowDown");
      const handle = make(doc, "span", "log-handle", "≡");
      handle.setAttribute("aria-hidden", "true");
      const name = make(doc, "span", "log-name", t.sea), count = make(doc, "span", "log-count", feeds(t.feeds || 0));
      name.id = `log-sea-${t.id}-name`;
      count.id = `log-sea-${t.id}-count`;
      row.setAttribute("aria-labelledby", `${name.id} ${count.id}`); // the row reads as its name and count
      const continent = index.continentOf(t.key);
      const rename = btn("Rename", "log-rename", `${k}:rename`, () => open(`${k}:rename-form`, true));
      nameSlot(`${k}:rename-form`, actions, rename, {title: "Rename", value: t.sea, action: "Rename", kind: "sea",
        selfId: t.id, current: t.sea}, name => call(on.renameSea, t.id, name));
      actions.append(rename);
      if (!continent) {
        const continentButton = btn("Make a continent", "log-make-continent", `${k}:continent`, () => open(`${k}:continent-form`, true));
        nameSlot(`${k}:continent-form`, actions, continentButton, {title: "Make a continent", value: "", action: "Create", kind: "land"},
          name => call(on.makeContinent, t.id, name));
        actions.append(continentButton);
      }
      const remove = btn("Delete", "log-delete", `${k}:delete`, () => open(`${k}:confirm`, true));
      slots.set(`${k}:confirm`, {slot: actions, opener: remove, build: close => {
        const question = make(doc, "span", "log-question", core.deleteSeaQuestion(t.sea, t.feeds || 0, !!continent));
        const yes = btn("Delete", "log-delete", `${k}:confirm:yes`, () => { memory.open = null; call(on.deleteSea, t.id); });
        const no = btn("Cancel", "log-cancel", `${k}:confirm:no`, () => close(true));
        return {nodes: [question, yes, no], first: no};
      }});
      actions.append(remove);
      row.append(handle, name, count, actions);

      row.addEventListener("keydown", e => {
        if (busy || !e || !e.altKey || (e.key !== "ArrowUp" && e.key !== "ArrowDown")) return;
        if (e.target && /^(INPUT|SELECT|TEXTAREA)$/.test(e.target.tagName || "")) return; // a rename field keeps its keys
        const delta = e.key === "ArrowUp" ? -1 : 1;
        if (i + delta < 0 || i + delta >= ledgerSeas.length) return;
        stop(e);
        call(on.moveSea, t.id, delta);
      });
      // the drag: the row dims, the line follows the pointer; it ends on release (the sea lands where the line
      // was), on pointercancel, or on Esc anywhere on the page (that Esc goes no further)
      let dragging = false, pointer = null;
      const end = () => {
        if (!dragging) return;
        dragging = false;
        row.classList.remove("dragging");
        dropLine.hidden = true;
        doc.removeEventListener("keydown", escape, true);
      };
      const escape = e => {
        if (!e || e.key !== "Escape") return;
        stop(e);
        if (e.stopImmediatePropagation) e.stopImmediatePropagation();
        else if (e.stopPropagation) e.stopPropagation();
        if (pointer != null && handle.hasPointerCapture && handle.hasPointerCapture(pointer)) handle.releasePointerCapture(pointer);
        end();
      };
      handle.addEventListener("pointerdown", e => {
        if (busy || (e && e.button != null && e.button !== 0)) return;
        end();
        dragging = true;
        pointer = e && e.pointerId != null ? e.pointerId : null;
        row.classList.add("dragging");
        stop(e);
        if (pointer != null && handle.setPointerCapture) handle.setPointerCapture(pointer);
        doc.addEventListener("keydown", escape, true);
      });
      handle.addEventListener("pointermove", e => { if (dragging) showDropLine(i, e); });
      handle.addEventListener("pointercancel", end);
      handle.addEventListener("pointerup", e => {
        if (!dragging) return;
        const d = dropAt(e);
        end();
        if (!d || d.to < 0) return; // let go off the list
        if (!busy && d.to !== i) call(on.moveSea, t.id, d.to - i);
      });
      seaRows.push(row);
      seaList.append(row);
    });
    const newSea = make(doc, "div", "log-new-sea");
    const newSeaButton = btn("New Sea…", "log-new-sea-button", "log:new-sea", () => open("log:new-sea-form", true));
    newSea.append(newSeaButton);
    nameSlot("log:new-sea-form", newSea, newSeaButton, {title: "New Sea", value: "", action: "Create", kind: "sea"},
      name => call(on.newSea, name));
    seasBody.append(newSea);
    aside.append(seas);

    // --- undo, redo and the status line ---
    const titles = state.titles || {undo: null, redo: null};
    const history = make(doc, "div", "log-history");
    const undo = btn(titles.undo ?? "Undo", "log-undo", "log:undo", () => call(on.undo));
    const redo = btn(titles.redo ?? "Redo", "log-redo", "log:redo", () => call(on.redo));
    if (titles.undo == null) undo.disabled = true;
    if (titles.redo == null) redo.disabled = true;
    history.append(undo, redo);
    const status = make(doc, "p", "log-status", state.message ?? "");
    status.setAttribute("role", "status");
    aside.append(history, status);

    // the form open before the rebuild opens again; one whose target is gone is forgotten
    if (memory.open) {
      if (slots.has(memory.open.key)) open(memory.open.key, false);
      else memory.open = null;
    }
    if (busy) controls.forEach(c => { c.disabled = true; });
    memory.target = memory.focus ? findKey([aside], memory.focus) : null;
    if (memory.focus && !memory.target) { memory.focus = null; memory.lost = false; }
    return [aside];
  }

  // ---------- Survey mode ----------
  // The DOM-free state the page draws from. Off at first, and whenever the page loads. toggle() turns it on only when
  // `available` (a value, or a function read at each toggle: a bridge and a document with a ledger); turning on
  // opens the log, turning off closes the log and any rename (off is always allowed). toggleLog() works only in
  // Survey mode. click(target), where target is {kind: "island", id}, a label placement or null, answers "popover"
  // outside Survey mode (the chart as it always was); in it, a territory, continent or sea name opens its rename
  // ("rename") and anything else is "select", never the popover. undo() and redo() send nothing outside Survey mode
  // or while a name is being typed in place (the anti-goal: nothing changes unless Survey mode is on).
  // onChange() follows every change of state.
  const RENAMABLE = new Set(["territory", "continent", "sea"]);
  function createMode({session, available, onChange} = {}) {
    let active = false, logOpen = false, renaming = false;
    const can = () => !!(typeof available === "function" ? available() : available);
    const changed = () => { if (onChange) onChange(); };
    const mode = {
      get active() { return active; },
      get logOpen() { return logOpen; },
      get renaming() { return renaming; },
      toggle() {
        if (active) { active = false; logOpen = false; renaming = false; }
        else if (can()) { active = true; logOpen = true; }
        else return false;
        changed();
        return true;
      },
      toggleLog() {
        if (!active) return false;
        logOpen = !logOpen;
        changed();
        return true;
      },
      rename(placement) {
        if (!active || !placement || !RENAMABLE.has(placement.kind)) return false;
        renaming = true;
        changed();
        return true;
      },
      endRename() {
        if (!renaming) return;
        renaming = false;
        changed();
      },
      click(target) {
        if (!active) return "popover";
        return mode.rename(target) ? "rename" : "select";
      },
      undo: () => (active && !renaming ? session.undo() : false),
      redo: () => (active && !renaming ? session.redo() : false),
      updated() { session.updated(); },
    };
    return Object.freeze(mode);
  }

  return Object.freeze({createSession, createMode, logContent, logMemory, nameForm, seaDropIndex, REPLY_MS, NO_REPLY, lateReply});
});
