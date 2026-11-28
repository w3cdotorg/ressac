// Tests of chart-survey.js: the edit session (one edit at a time, history), the Cartographer's Log and the
// name form, built on the shared fake document. Run with `node --test 'apps/macos/ChartTests/*.test.cjs'`.
"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");

const CHART = path.join(__dirname, "..", "Ressac", "Resources", "Chart");
const survey = require(path.join(CHART, "chart-survey.js"));
const core = require(path.join(CHART, "chart-core.js"));
const {fakeDocument, pressKey, walk, elements, byClass} = require("./fake-dom.cjs");

const T = core.EDIT_TEXT;
const fixture = () => JSON.parse(JSON.stringify(require("./fixture.json")));
const TRICKY_TITLE = '<b>Bold</b> & "Quoted" Notes <img src=x onerror=alert(1)> </option>';
const tricky = data => data.feeds.find(f => f.title === TRICKY_TITLE);

// ---------- helpers over the fake DOM ----------
const allByClass = (nodes, cls) => elements(nodes).filter(n => n.className.split(" ").includes(cls));
const buttons = nodes => elements(nodes).filter(n => n.tagName === "BUTTON");
const button = (nodes, text) => buttons(nodes).find(b => b.textContent === text);
// lookups that must find something: a missing element fails as an assertion, not as a TypeError
const btn = (nodes, text) => { const b = button(nodes, text); assert.ok(b, `a button ${text}`); return b; };
const one = (nodes, cls) => { const el = byClass(nodes, cls); assert.ok(el, `an element .${cls}`); return el; };
const tagged = (nodes, tag) => { const el = elements(nodes).find(e => e.tagName === tag); assert.ok(el, `an element ${tag}`); return el; };
const ev = extra => ({preventDefault() { this.defaultPrevented = true; }, stopPropagation() {}, ...extra});
const inputIn = node => tagged([node], "INPUT");
const nameOf = row => one([row], "log-name").textContent;
const countOf = row => one([row], "log-count").textContent;

// handlers that record every call: on.calls = [[name, ...args]]
function recorder() {
  const calls = [];
  const names = ["select", "setSea", "setLand", "leave", "newLand", "renameLand", "deleteLand", "makeContinent",
    "renameSea", "moveSea", "newSea", "deleteSea", "undo", "redo", "submit", "cancel"];
  const on = Object.fromEntries(names.map(n => [n, (...args) => calls.push([n, ...args])]));
  on.calls = calls;
  return on;
}
const log = (data, state, on, doc = fakeDocument()) =>
  survey.logContent(doc, data, {selected: null, busy: false, message: null, ...state}, on || recorder());

// a session over stubs: posted messages, onChange count, timers to fire by hand
function stubSession() {
  const posted = [], timers = [], timeouts = [];
  let changes = 0;
  const session = survey.createSession({
    post: m => posted.push(m),
    onChange: () => { changes += 1; },
    onTimeout: outcome => timeouts.push(outcome),
    setTimeout: (fn, ms) => { timers.push({fn, ms}); return timers.length; },
  });
  // the timers of one duration, oldest first (PENDING_MS after a result, REPLY_MS after a send)
  const after = ms => timers.filter(t => t.ms === ms);
  return {session, posted, timers, after, timeouts, changes: () => changes};
}
const ok = (seq, label, undo) => JSON.stringify({seq, ok: true, done: {event: "chart_edit", label, undo}});
const refused = (seq, error) => JSON.stringify({seq, ok: false, refused: true, error});
const failed = (seq, error) => JSON.stringify({seq, ok: false, refused: false, error});
const patch = (label, n = 1) => core.edits.patch(label, Array.from({length: n}, (_, i) => ({kind: "member", feed: i + 1, land: 1, before: null, after: i})));
const JOIN = core.edits.join(12, {id: 11, name: "APPLE PRESS"});

// ---------- the log ----------
test("log_feed_titles_are_text", () => {
  const doc = fakeDocument(), data = fixture(), feed = tricky(data);
  assert.ok(feed, "the fixture has the tricky feed");
  assert.throws(() => { doc.createElement("div").innerHTML = "<b>x</b>"; }, /innerHTML/, "the trap is armed");

  const nodes = log(data, {selected: feed.id}, recorder(), doc);
  const island = one(nodes, "log-island");
  assert.equal(island.tagName, "SELECT");
  const option = elements([island]).find(o => o.tagName === "OPTION" && o.value === String(feed.id));
  assert.deepEqual(option.childNodes, [{nodeType: 3, data: TRICKY_TITLE}], "the option's text is the exact title");
  const heading = one(nodes, "log-island-title");
  assert.match(heading.tagName, /^H\d$/);
  assert.deepEqual(heading.childNodes, [{nodeType: 3, data: TRICKY_TITLE}], "the heading is the exact title, as text");
  assert.ok(!elements(nodes).some(e => e.tagName === "IMG" || e.tagName === "SCRIPT"), "no element made from a title");
  assert.equal(elements([island]).filter(o => o.tagName === "OPTION").length, data.feeds.length + 1, "every chart feed (and the blank)");

  // the tricky title as a With: choice, through the log's New Land… and in a bare nameForm
  const other = data.feeds.find(f => f.theme === "maps" && f.id !== feed.id);
  const nodes2 = log(data, {selected: other.id}, recorder(), doc);
  btn(nodes2, "New Land…").dispatch("click", ev());
  const choice = elements(nodes2).find(o => o.tagName === "OPTION" && o.value === String(feed.id));
  assert.ok(choice, "the tricky feed is offered as a With: choice");
  assert.deepEqual(choice.childNodes, [{nodeType: 3, data: TRICKY_TITLE}]);
  assert.ok(!elements(nodes2).some(e => e.tagName === "IMG" || e.tagName === "SCRIPT"));

  const form = survey.nameForm(doc, {title: "Name this land", value: "", withChoices: [{id: feed.id, label: feed.title}]}, recorder());
  const opt = elements(form.nodes).find(o => o.tagName === "OPTION" && o.value === String(feed.id));
  assert.deepEqual(opt.childNodes, [{nodeType: 3, data: TRICKY_TITLE}]);
  assert.ok(!elements(form.nodes).some(e => e.tagName === "IMG" || e.tagName === "SCRIPT"));
});

test("log_lists_every_sea_and_land_from_the_ledger", () => {
  const data = fixture(), nodes = log(data, {});
  const aside = nodes[0];
  assert.ok(aside, "the log is built");
  assert.equal(aside.tagName, "ASIDE");
  const title = elements(nodes).find(e => e.id === aside.getAttribute("aria-labelledby"));
  assert.equal(title.textContent, "Cartographer's Log");

  const seaRows = allByClass(nodes, "log-sea");
  assert.deepEqual(seaRows.map(nameOf), data.ledger.seas.map(s => s.sea), "every sea, in ledger order");
  assert.ok(seaRows.map(nameOf).includes("UNSAILED SEA"), "the empty sea too");
  assert.deepEqual(seaRows.map(countOf), data.ledger.seas.map(s => `${s.feeds} ${s.feeds === 1 ? "feed" : "feeds"}`));
  seaRows.forEach(r => assert.equal(one([r], "log-handle").textContent, "≡"));
  const seaRow = sea => seaRows.find(r => nameOf(r) === sea);
  assert.ok(button([seaRow("SEA OF KITES")], "Make a continent"), "a sea without a continent offers one");
  assert.equal(button([seaRow("GULF OF BELLS")], "Make a continent"), undefined, "a sea with a continent does not");
  for (const r of seaRows) assert.ok(button([r], "Rename") && btn([r], "Delete"));

  const landRows = allByClass(nodes, "log-land");
  assert.deepEqual(landRows.map(nameOf), data.ledger.lands.map(l => l.name), "every land and continent, in ledger order");
  assert.ok(landRows.map(nameOf).includes("OLD MILL PRESS"), "the hidden land too");
  const landRow = name => landRows.find(r => nameOf(r) === name);
  assert.equal(countOf(landRow("GULL PRESS GROUP")), "3 feeds");
  assert.equal(countOf(landRow("OLD MILL PRESS")), "2 feeds");
  assert.equal(countOf(landRow("BELLMARK")), "9 feeds", "a continent counts its sea's feeds");
  for (const name of ["BELLMARK", "CRUSTLAND", "THE KNOTTED LAND", "KILN COUNTRY"]) {
    assert.ok(button([landRow(name)], "Back to Open Water"), name);
    assert.equal(button([landRow(name)], "Delete"), undefined, name);
  }
  assert.ok(button([landRow("APPLE PRESS")], "Delete"));
  assert.equal(button([landRow("APPLE PRESS")], "Back to Open Water"), undefined);
  for (const r of landRows) assert.ok(button([r], "Rename"));

  assert.ok(button(nodes, "New Sea…"));
  const sections = allByClass(nodes, "log-section-name").map(e => e.textContent);
  assert.deepEqual(sections, [`Lands \u00B7 ${data.ledger.lands.length}`, `Seas \u00B7 ${data.ledger.seas.length}`]);
  assert.equal(one(nodes, "log-status").getAttribute("role"), "status");
});

test("log_buttons_call_their_handlers", () => {
  const data = fixture(), on = recorder(), nodes = log(data, {}, on);
  const row = (cls, name) => { const r = allByClass(nodes, cls).find(x => nameOf(x) === name); assert.ok(r, `a row ${name}`); return r; };
  const landRow = name => row("log-land", name), seaRow = sea => row("log-sea", sea);
  btn([landRow("APPLE PRESS")], "Delete").dispatch("click", ev());
  btn([landRow("BELLMARK")], "Back to Open Water").dispatch("click", ev());
  assert.deepEqual(on.calls, [["deleteLand", 11], ["deleteLand", 12]]);

  // deleting a sea asks first, with the core's question; Cancel puts the row back
  on.calls.length = 0;
  btn([seaRow("GULF OF BELLS")], "Delete").dispatch("click", ev());
  assert.equal(on.calls.length, 0, "nothing deleted before the confirmation");
  assert.ok(walk([seaRow("GULF OF BELLS")]).some(n => n.nodeType === 3 && n.data === core.deleteSeaQuestion("GULF OF BELLS", 9, true)));
  btn([seaRow("GULF OF BELLS")], "Cancel").dispatch("click", ev());
  assert.equal(on.calls.length, 0);
  btn([seaRow("GULF OF BELLS")], "Delete").dispatch("click", ev());
  btn([seaRow("GULF OF BELLS")], "Delete").dispatch("click", ev());
  assert.deepEqual(on.calls, [["deleteSea", 3]]);

  // rename: a clash keeps the form open with the text; an unchanged name closes it; a new name is sent trimmed
  on.calls.length = 0;
  btn([landRow("APPLE PRESS")], "Rename").dispatch("click", ev());
  let input = tagged([landRow("APPLE PRESS")], "INPUT");
  assert.equal(input.value, "APPLE PRESS");
  assert.equal(buttons([landRow("APPLE PRESS")]).find(b => b.type === "submit").textContent, "Rename");
  input.value = " gull press group ";
  one([landRow("APPLE PRESS")], "name-form").dispatch("submit", ev());
  assert.equal(on.calls.length, 0);
  assert.equal(one([landRow("APPLE PRESS")], "name-form-error").textContent, T.landTaken);
  input.value = "APPLE PRESS";
  one([landRow("APPLE PRESS")], "name-form").dispatch("submit", ev());
  assert.equal(on.calls.length, 0, "an unchanged name sends nothing");
  assert.equal(byClass([landRow("APPLE PRESS")], "name-form"), undefined, "and closes the form");
  btn([landRow("APPLE PRESS")], "Rename").dispatch("click", ev());
  input = tagged([landRow("APPLE PRESS")], "INPUT");
  input.value = "  Pear Press ";
  one([landRow("APPLE PRESS")], "name-form").dispatch("submit", ev());
  assert.deepEqual(on.calls, [["renameLand", 11, "Pear Press"]]);

  on.calls.length = 0;
  btn([seaRow("SEA OF KITES")], "Rename").dispatch("click", ev());
  input = tagged([seaRow("SEA OF KITES")], "INPUT");
  assert.equal(buttons([seaRow("SEA OF KITES")]).find(b => b.type === "submit").textContent, "Rename");
  input.value = "tidal sea";
  one([seaRow("SEA OF KITES")], "name-form").dispatch("submit", ev());
  assert.equal(one([seaRow("SEA OF KITES")], "name-form-error").textContent, T.seaTaken);
  input.value = "Sea of Paper";
  one([seaRow("SEA OF KITES")], "name-form").dispatch("submit", ev());
  btn([seaRow("TIDAL SEA")], "Make a continent").dispatch("click", ev());
  input = tagged([seaRow("TIDAL SEA")], "INPUT");
  input.value = "Tideland";
  one([seaRow("TIDAL SEA")], "name-form").dispatch("submit", ev());
  btn(nodes, "New Sea…").dispatch("click", ev());
  input = tagged([one(nodes, "log-new-sea")], "INPUT");
  assert.equal(buttons([one(nodes, "log-new-sea")]).find(b => b.type === "submit").textContent, "Create");
  input.value = "Comics";
  one([one(nodes, "log-new-sea")], "name-form").dispatch("submit", ev());
  assert.deepEqual(on.calls, [["renameSea", 1, "Sea of Paper"], ["makeContinent", 2, "Tideland"], ["newSea", "Comics"]]);
});

test("log_island_block_selects_and_edits_the_island", () => {
  const data = fixture(), on = recorder();
  let nodes = log(data, {}, on);
  assert.equal(byClass(nodes, "log-island-title"), undefined, "no island heading without a selection");
  const island = one(nodes, "log-island");
  island.value = "12";
  island.dispatch("change", ev());
  assert.deepEqual(on.calls, [["select", 12]]);

  on.calls.length = 0;
  nodes = log(data, {selected: 12}, on); // The Tidal Tribune: kites, no land
  const sea = one(nodes, "log-sea-select"), land = one(nodes, "log-land-select");
  assert.equal(sea.value, "1");
  const seaOptions = elements([sea]).filter(o => o.tagName === "OPTION").map(o => o.textContent);
  assert.deepEqual(seaOptions, [...data.ledger.seas.map(s => s.sea), "Uncharted waters"]);
  const landOptions = elements([land]).filter(o => o.tagName === "OPTION" && o.value !== "").map(o => o.textContent);
  assert.deepEqual(landOptions, data.ledger.lands.filter(l => !l.continent).map(l => l.name), "territories only");
  assert.equal(btn(nodes, "Leave Land").disabled, true, "nothing to leave");
  sea.value = "2"; sea.dispatch("change", ev());
  sea.value = ""; sea.dispatch("change", ev());
  land.value = "11"; land.dispatch("change", ev());
  assert.deepEqual(on.calls, [["setSea", 12, 2], ["setSea", 12, null], ["setLand", 12, 11]]);

  // New Land…: With: lists the other non-continental islands; the name is checked; feeds go in chart order
  on.calls.length = 0;
  btn(nodes, "New Land…").dispatch("click", ev());
  const form = one(nodes, "name-form"), input = tagged([form], "INPUT");
  const with_ = tagged([form], "SELECT");
  assert.equal(buttons([form]).find(b => b.type === "submit").textContent, "Create");
  const offered = elements([with_]).filter(o => o.tagName === "OPTION" && o.value !== "").map(o => Number(o.value));
  const expected = data.feeds.filter(f => f.id !== 12 && !["bells", "bread", "knots", "kilns"].includes(f.theme)).map(f => f.id);
  assert.deepEqual(offered, expected);
  with_.value = "3"; with_.dispatch("change", ev());
  input.value = "apple press";
  form.dispatch("submit", ev());
  assert.equal(on.calls.length, 0, "a taken name is caught before sending");
  assert.equal(one([form], "name-form-error").textContent, T.landTaken);
  input.value = " Trio ";
  form.dispatch("submit", ev());
  assert.deepEqual(on.calls, [["newLand", "Trio", [3, 12]]], "chart order");

  on.calls.length = 0;
  nodes = log(data, {selected: 1}, on); // GULL PRESS GROUP
  assert.equal(one(nodes, "log-land-select").value, "1");
  btn(nodes, "Leave Land").dispatch("click", ev());
  assert.deepEqual(on.calls, [["leave", 1]]);
});

test("leave_land_is_disabled_on_a_continental_island", () => {
  const data = fixture(), on = recorder();
  const nodes = log(data, {selected: 37}, on); // bells, a continent sea (BELLMARK)
  const hint = T.continentHint("BELLMARK");
  const leave = btn(nodes, "Leave Land");
  assert.equal(leave.disabled, true);
  assert.equal(leave.title, hint);
  const land = one(nodes, "log-land-select");
  assert.equal(land.disabled, true);
  assert.equal(land.title, hint);
  assert.equal(btn(nodes, "New Land…").disabled, true);
  assert.ok(!one(nodes, "log-sea-select").disabled, "a continental island may still change sea");
  leave.dispatch("click", ev());
  assert.equal(on.calls.length, 0);

  const plain = log(data, {selected: 1}, recorder());
  assert.ok(!btn(plain, "Leave Land").disabled);
  assert.ok(!btn(plain, "Leave Land").title);
});

test("log_controls_are_disabled_while_busy", () => {
  const data = fixture(), on = recorder();
  const nodes = log(data, {selected: 1, busy: true, titles: {undo: "Undo Join APPLE PRESS", redo: null}}, on);
  // every edit control; the section headers only fold, and stay usable
  const isHeader = e => e.className.split(" ").includes("log-disclosure");
  const controls = elements(nodes).filter(e => ["BUTTON", "SELECT", "INPUT"].includes(e.tagName) && !isHeader(e));
  assert.ok(controls.length > 40);
  for (const c of controls) assert.equal(c.disabled, true, `${c.tagName} ${c.textContent}`);
  assert.equal(elements(nodes).filter(isHeader).length, 2);
  const row = allByClass(nodes, "log-sea")[1];
  assert.ok(row, "a second sea row");
  row.dispatch("keydown", ev({key: "ArrowUp", altKey: true}));
  assert.equal(on.calls.length, 0, "no reorder while busy");
});

test("log_undo_and_redo_show_the_titles", () => {
  const on = recorder();
  let nodes = log(fixture(), {titles: {undo: "Undo Join APPLE PRESS", redo: null}, message: T.changed}, on);
  const undo = one(nodes, "log-undo"), redo = one(nodes, "log-redo");
  assert.equal(undo.textContent, "Undo Join APPLE PRESS");
  assert.ok(!undo.disabled);
  assert.equal(redo.textContent, "Redo");
  assert.equal(redo.disabled, true);
  undo.dispatch("click", ev());
  assert.deepEqual(on.calls, [["undo"]]);
  assert.equal(one(nodes, "log-status").textContent, T.changed);
  nodes = log(fixture(), {}, on);
  assert.equal(one(nodes, "log-undo").disabled, true);
  assert.equal(one(nodes, "log-undo").textContent, "Undo");
});

test("alt_arrows_move_a_sea", () => {
  const data = fixture(), on = recorder(), nodes = log(data, {}, on);
  const rows = allByClass(nodes, "log-sea");
  assert.equal(rows.length, data.ledger.seas.length, "a row per sea");
  const second = data.ledger.seas[1].id;
  rows[1].dispatch("keydown", ev({key: "ArrowUp", altKey: true}));
  assert.deepEqual(on.calls, [["moveSea", second, -1]]);
  rows[1].dispatch("keydown", ev({key: "ArrowDown", altKey: true}));
  rows[1].dispatch("keydown", ev({key: "ArrowUp"}));
  rows[0].dispatch("keydown", ev({key: "ArrowUp", altKey: true}));
  rows[rows.length - 1].dispatch("keydown", ev({key: "ArrowDown", altKey: true}));
  assert.deepEqual(on.calls, [["moveSea", second, -1], ["moveSea", second, 1]], "no move without Alt or past either end");
  btn([rows[2]], "Rename").dispatch("click", ev());
  rows[2].dispatch("keydown", ev({key: "ArrowUp", altKey: true, target: tagged([rows[2]], "INPUT")}));
  assert.equal(on.calls.length, 2, "Alt+arrows inside a rename field stay with the field");
  assert.equal(rows[1].tabIndex, 0, "a sea row takes the focus");
});

test("dragging_the_handle_moves_a_sea", () => {
  const data = fixture(), on = recorder(), nodes = log(data, {}, on);
  const rows = allByClass(nodes, "log-sea");
  assert.equal(rows.length, data.ledger.seas.length, "a row per sea");
  rows.forEach((r, i) => { r.rect = {left: 0, right: 300, top: i * 30, bottom: i * 30 + 30, width: 300, height: 30, x: 0, y: i * 30}; });
  const first = data.ledger.seas[0].id, handle = one([rows[0]], "log-handle");
  handle.dispatch("pointerdown", ev({clientY: 15, pointerId: 1, button: 0}));
  handle.dispatch("pointerup", ev({clientY: 75, pointerId: 1}));
  assert.deepEqual(on.calls, [["moveSea", first, 2]]);
  handle.dispatch("pointerdown", ev({clientY: 15, pointerId: 1, button: 0}));
  handle.dispatch("pointerup", ev({clientY: 15, pointerId: 1}));
  handle.dispatch("pointerdown", ev({clientY: 22, pointerId: 1, button: 0}));
  handle.dispatch("pointerup", ev({clientY: 29, pointerId: 1})); // the lower half of its own row, below its middle
  assert.equal(on.calls.length, 1, "released on its own row: nothing");
  handle.dispatch("pointerup", ev({clientY: 200, pointerId: 1}));
  assert.equal(on.calls.length, 1, "a release without a press: nothing");
  const fourth = one([rows[3]], "log-handle");
  fourth.dispatch("pointerdown", ev({clientY: 105, pointerId: 2, button: 0}));
  fourth.dispatch("pointerup", ev({clientY: 0, pointerId: 2}));
  assert.deepEqual(on.calls[1], ["moveSea", data.ledger.seas[3].id, -3]);

  // rows A B C D at 0, 30, 60, 90: a release anywhere on B's lower half lands on B, in both directions
  on.calls.length = 0;
  handle.dispatch("pointerdown", ev({clientY: 15, pointerId: 3, button: 0}));
  handle.dispatch("pointerup", ev({clientY: 50, pointerId: 3})); // A dropped on B: B A C D
  fourth.dispatch("pointerdown", ev({clientY: 105, pointerId: 4, button: 0}));
  fourth.dispatch("pointerup", ev({clientY: 50, pointerId: 4})); // D dropped on B: A D B C
  handle.dispatch("pointerdown", ev({clientY: 15, pointerId: 5, button: 0}));
  handle.dispatch("pointerup", ev({clientY: 85, pointerId: 5})); // A dropped on C's lower half: B C A D
  fourth.dispatch("pointerdown", ev({clientY: 105, pointerId: 6, button: 0}));
  fourth.dispatch("pointerup", ev({clientY: 35, pointerId: 6})); // D dropped on B's upper half: A D B C
  const [a, d] = [data.ledger.seas[0].id, data.ledger.seas[3].id];
  assert.deepEqual(on.calls, [["moveSea", a, 1], ["moveSea", d, -2], ["moveSea", a, 2], ["moveSea", d, -2]]);

  // let go off the list, left or right: nothing
  on.calls.length = 0;
  tagged([one(nodes, "log-seas")], "OL").rect = {left: 0, right: 300, top: 0, bottom: 480, width: 300, height: 480, x: 0, y: 0};
  handle.dispatch("pointerdown", ev({clientY: 15, clientX: 40, pointerId: 7, button: 0}));
  handle.dispatch("pointerup", ev({clientY: 75, clientX: 640, pointerId: 7}));
  handle.dispatch("pointerdown", ev({clientY: 15, clientX: 40, pointerId: 8, button: 0}));
  handle.dispatch("pointerup", ev({clientY: 75, clientX: -20, pointerId: 8}));
  assert.equal(on.calls.length, 0, "released off the list");
  handle.dispatch("pointerdown", ev({clientY: 15, clientX: 40, pointerId: 9, button: 0}));
  handle.dispatch("pointerup", ev({clientY: 75, clientX: 120, pointerId: 9}));
  assert.deepEqual(on.calls, [["moveSea", a, 2]]);
});

// ---------- the drop line while a sea is dragged ----------
// rows of 30 px from y = 100, the seas' section body from y = 60: the line's top is the row edge it marks, in the body
function dragLog() {
  const doc = fakeDocument(), data = fixture(), on = recorder(), nodes = log(data, {}, on, doc);
  doc.body.append(...nodes); // in the page, as the drawer holds it
  const rows = allByClass(nodes, "log-sea"), seas = one(nodes, "log-seas");
  rows.forEach((r, i) => { r.rect = {left: 0, right: 300, top: 100 + i * 30, bottom: 130 + i * 30, width: 300, height: 30, x: 0, y: 100 + i * 30}; });
  one([seas], "log-section-body").rect = {left: 0, right: 300, top: 60, bottom: 600, width: 300, height: 540, x: 0, y: 60};
  tagged([seas], "OL").rect = {left: 0, right: 300, top: 100, bottom: 100 + rows.length * 30, width: 300, height: rows.length * 30, x: 0, y: 100};
  const line = one([seas], "log-drop-line");
  const mids = rows.map(r => (r.rect.top + r.rect.bottom) / 2);
  return {doc, data, on, nodes, rows, line, mids};
}

test("dragging_a_sea_shows_where_it_will_land", () => {
  const {data, on, rows, line, mids} = dragLog();
  assert.equal(line.hidden, true, "no line before a drag");
  assert.equal(line.getAttribute("aria-hidden"), "true");
  // from the first row (moving down) and from the fourth (moving up and down), at every height
  for (const from of [0, 3]) {
    const handle = one([rows[from]], "log-handle"), id = data.ledger.seas[from].id;
    for (let y = 80; y <= 100 + rows.length * 30 + 20; y += 7) {
      on.calls.length = 0;
      handle.dispatch("pointerdown", ev({clientX: 40, clientY: mids[from], pointerId: 1, button: 0}));
      assert.ok(rows[from].classList.contains("dragging"), "the dragged row dims");
      handle.dispatch("pointermove", ev({clientX: 40, clientY: y, pointerId: 1}));
      const to = survey.seaDropIndex(mids, y);
      if (to === from) assert.equal(line.hidden, true, `over its own row (y ${y}): no line`);
      else {
        assert.equal(line.hidden, false, `a line at y ${y}`);
        const edge = to < from ? rows[to].rect.top : rows[to].rect.bottom; // above the row it passes moving up, below it moving down
        assert.equal(line.style.top, `${edge - 60}px`, `from ${from} at y ${y}: the line marks row ${to}`);
      }
      handle.dispatch("pointerup", ev({clientX: 40, clientY: y, pointerId: 1}));
      assert.deepEqual(on.calls, to === from ? [] : [["moveSea", id, to - from]], `released at y ${y}: lands where the line was`);
      assert.equal(line.hidden, true, "the line goes on release");
      assert.ok(!rows[from].classList.contains("dragging"), "the row is itself again on release");
    }
  }
  // off the list, left or right: no line, and nothing on release
  const handle = one([rows[0]], "log-handle");
  on.calls.length = 0;
  handle.dispatch("pointerdown", ev({clientX: 40, clientY: mids[0], pointerId: 1, button: 0}));
  handle.dispatch("pointermove", ev({clientX: 40, clientY: mids[2], pointerId: 1}));
  assert.equal(line.hidden, false);
  handle.dispatch("pointermove", ev({clientX: 640, clientY: mids[2], pointerId: 1}));
  assert.equal(line.hidden, true, "off the list: no line");
  handle.dispatch("pointerup", ev({clientX: 640, clientY: mids[2], pointerId: 1}));
  assert.deepEqual(on.calls, []);
  // a move without a press shows nothing
  handle.dispatch("pointermove", ev({clientX: 40, clientY: mids[2], pointerId: 1}));
  assert.equal(line.hidden, true, "no press, no line");
});

test("the_drop_line_goes_on_escape_and_on_cancel", () => {
  const {doc, data, on, rows, line, mids} = dragLog();
  const handle = one([rows[1]], "log-handle");
  // pointercancel
  handle.dispatch("pointerdown", ev({clientX: 40, clientY: mids[1], pointerId: 1, button: 0}));
  handle.dispatch("pointermove", ev({clientX: 40, clientY: mids[3], pointerId: 1}));
  assert.equal(line.hidden, false);
  handle.dispatch("pointercancel", ev({pointerId: 1}));
  assert.equal(line.hidden, true, "cancelled: the line goes");
  assert.ok(!rows[1].classList.contains("dragging"));
  handle.dispatch("pointerup", ev({clientX: 40, clientY: mids[3], pointerId: 1}));
  assert.deepEqual(on.calls, [], "a cancelled drag moves nothing");
  // Esc, anywhere on the page: the drag ends there, and the page's own Esc does not see it
  handle.dispatch("pointerdown", ev({clientX: 40, clientY: mids[1], pointerId: 2, button: 0}));
  handle.dispatch("pointermove", ev({clientX: 40, clientY: mids[3], pointerId: 2}));
  assert.equal(line.hidden, false);
  let stopped = false;
  doc.dispatch("keydown", ev({key: "Escape", stopImmediatePropagation() { stopped = true; }}));
  assert.equal(line.hidden, true, "Esc: the line goes");
  assert.ok(stopped, "the Esc ends with the drag");
  assert.ok(!rows[1].classList.contains("dragging"));
  handle.dispatch("pointerup", ev({clientX: 40, clientY: mids[3], pointerId: 2}));
  assert.deepEqual(on.calls, [], "an Esc'd drag moves nothing");
  assert.equal((doc.listeners.keydown || []).length, 0, "no key listener left behind");
  // another key does not end the drag; the listener goes with the release
  handle.dispatch("pointerdown", ev({clientX: 40, clientY: mids[1], pointerId: 3, button: 0}));
  doc.dispatch("keydown", ev({key: "a"}));
  handle.dispatch("pointermove", ev({clientX: 40, clientY: mids[3], pointerId: 3}));
  assert.equal(line.hidden, false);
  handle.dispatch("pointerup", ev({clientX: 40, clientY: mids[3], pointerId: 3}));
  assert.deepEqual(on.calls, [["moveSea", data.ledger.seas[1].id, 2]]);
  assert.equal((doc.listeners.keydown || []).length, 0, "released: no key listener left behind");
});

test("a_drag_whose_log_was_rebuilt_leaves_esc_to_the_page", () => {
  const {doc, on, rows, line, mids} = dragLog();
  const pageEsc = [];
  doc.addEventListener("keydown", e => { if (e.key === "Escape") pageEsc.push(e); }); // the page's own Esc
  const keydowns = () => (doc.listeners.keydown || []).length;
  const down = (h, id) => h.dispatch("pointerdown", ev({clientX: 40, clientY: mids[1], pointerId: id, button: 0}));
  const rebuild = () => { const nodes = log(fixture(), {}, recorder(), doc); doc.body.replaceChildren(...nodes); return nodes; };
  // a live drag keeps its Esc
  const handle = one([rows[1]], "log-handle");
  down(handle, 1);
  doc.dispatch("keydown", ev({key: "Escape"}));
  assert.equal(pageEsc.length, 0, "a live drag's Esc stops there");
  assert.equal(keydowns(), 1);
  // rebuilt mid-drag (a Chart.update): the next Esc goes to the page, and the old drag lets go of it
  down(handle, 2);
  assert.equal(keydowns(), 2);
  rebuild();
  assert.equal(rows[1].isConnected, false, "the old row left the page");
  doc.dispatch("keydown", ev({key: "Escape"}));
  assert.equal(pageEsc.length, 1, "the Esc reached the page");
  assert.equal(keydowns(), 1, "only the page's listener is left");
  assert.equal(line.hidden, true);
  assert.ok(!rows[1].classList.contains("dragging"));
  // rebuilt mid-drag, then a move: the old drag ends there
  let nodes = rebuild();
  let h = one([allByClass(nodes, "log-sea")[1]], "log-handle");
  down(h, 3);
  rebuild();
  h.dispatch("pointermove", ev({clientX: 40, clientY: mids[3], pointerId: 3}));
  assert.equal(keydowns(), 1, "a move after a rebuild ends the old drag");
  assert.equal(one(nodes, "log-drop-line").hidden, true, "and shows no line");
  // the capture lost: the drag ends, and a later release moves nothing
  nodes = rebuild();
  h = one([allByClass(nodes, "log-sea")[1]], "log-handle");
  down(h, 4);
  h.dispatch("pointermove", ev({clientX: 40, clientY: mids[3], pointerId: 4}));
  assert.equal(one(nodes, "log-drop-line").hidden, false);
  h.dispatch("lostpointercapture", ev({pointerId: 4}));
  assert.equal(one(nodes, "log-drop-line").hidden, true, "capture lost: the line goes");
  assert.equal(keydowns(), 1, "capture lost: the listener goes");
  h.dispatch("pointerup", ev({clientX: 40, clientY: mids[3], pointerId: 4}));
  assert.deepEqual(on.calls, []);
});

test("log_css_ends_sections_cleanly_and_keeps_still_under_reduce_motion", () => {
  const css = require("node:fs").readFileSync(path.join(CHART, "chart.css"), "utf8");
  assert.ok(/\.log-list > li:last-child\s*\{[^}]*border-bottom:\s*0/.test(css), "no dotted rule after a section's last row");
  assert.ok(/\.log h3\s*\{[^}]*border-top:\s*1px solid/.test(css), "the solid rule between sections stays");
  assert.ok(/\.log-sea\.dragging\s*\{[^}]*opacity:/.test(css), "the dragged row dims");
  assert.ok(/\.log-drop-line\s*\{[^}]*position:\s*absolute/.test(css), "the drop line takes no room");
  assert.ok(/body\.still \.log-drop-line[^{]*\{[^}]*transition:\s*none/.test(css), "no animation under the app's Reduce Motion");
  assert.ok(/prefers-reduced-motion: reduce\)\s*\{[^}]*\.log-drop-line[^{]*\{[^}]*transition:\s*none/.test(css), "nor the system's");
});

// ---------- the section headers ----------
const header = (nodes, cls) => { const h = one([one(nodes, cls)], "log-disclosure"); assert.equal(h.tagName, "BUTTON"); return h; };
const bodyOf = (nodes, cls) => one([one(nodes, cls)], "log-section-body");
const markOf = h => one([h], "log-disclosure-mark").textContent;

test("lands_and_seas_are_headers_that_fold_their_lists", () => {
  const data = fixture(), memory = survey.logMemory(), doc = fakeDocument();
  const nodes = survey.logContent(doc, data, {}, recorder(), memory);
  for (const [cls, name, n] of [["log-lands", "Lands", data.ledger.lands.length], ["log-seas", "Seas", data.ledger.seas.length]]) {
    const h = header(nodes, cls), body = bodyOf(nodes, cls);
    assert.equal(h.type, "button");
    assert.equal(h.getAttribute("aria-expanded"), "true", `${name} open by default`);
    assert.equal(h.getAttribute("aria-controls"), body.id);
    assert.ok(body.id, "the body has an id");
    assert.equal(markOf(h), "▾");
    assert.equal(one([h], "log-disclosure-mark").getAttribute("aria-hidden"), "true");
    const label = one([h], "log-section-name");
    assert.deepEqual(label.childNodes, [{nodeType: 3, data: `${name} · ${n}`}], "text only");
    assert.ok(!body.hidden);
    h.dispatch("click", ev());
    assert.equal(h.getAttribute("aria-expanded"), "false", `${name} folds`);
    assert.equal(markOf(h), "▸");
    assert.equal(body.hidden, true);
    h.dispatch("click", ev());
    assert.equal(h.getAttribute("aria-expanded"), "true", `${name} opens again`);
    assert.equal(markOf(h), "▾");
    assert.ok(!body.hidden);
  }
  assert.ok(one([bodyOf(nodes, "log-lands")], "log-land"), "the lands' rows are in their body");
  assert.ok(one([bodyOf(nodes, "log-seas")], "log-sea"), "the seas' rows are in theirs");
  assert.ok(button([bodyOf(nodes, "log-seas")], "New Sea…"), "New Sea… folds with the seas");
});

test("a_folded_section_stays_folded_across_rebuilds", () => {
  const data = fixture(), memory = survey.logMemory(), doc = fakeDocument();
  let nodes = survey.logContent(doc, data, {}, recorder(), memory);
  const h = header(nodes, "log-lands");
  h.focus();
  h.dispatch("click", ev());
  // a new snapshot: one land fewer, one sea more
  const next = fixture();
  next.ledger.lands = next.ledger.lands.slice(1);
  next.ledger.seas = [...next.ledger.seas, {...next.ledger.seas[0], id: 999, key: "new-sea", sea: "NEW SEA", feeds: 0}];
  nodes = survey.logContent(doc, next, {}, recorder(), memory);
  memory.restore();
  const lands = header(nodes, "log-lands"), seas = header(nodes, "log-seas");
  assert.equal(lands.getAttribute("aria-expanded"), "false", "Lands stays folded");
  assert.equal(bodyOf(nodes, "log-lands").hidden, true);
  assert.equal(markOf(lands), "▸");
  assert.equal(one([lands], "log-section-name").textContent, `Lands · ${next.ledger.lands.length}`, "with the new count");
  assert.equal(seas.getAttribute("aria-expanded"), "true", "Seas stays open");
  assert.equal(one([seas], "log-section-name").textContent, `Seas · ${next.ledger.seas.length}`);
  assert.equal(doc.activeElement, lands, "the focus stays on the header");
  // opened again, and rebuilt: open
  lands.dispatch("click", ev());
  nodes = survey.logContent(doc, next, {}, recorder(), memory);
  assert.equal(header(nodes, "log-lands").getAttribute("aria-expanded"), "true");
  assert.ok(!bodyOf(nodes, "log-lands").hidden);
  // a busy log still folds (folding is no edit)
  nodes = survey.logContent(doc, next, {busy: true}, recorder(), memory);
  assert.ok(!header(nodes, "log-seas").disabled, "the header works while an edit is on its way");
  // a fresh memory (a new window): both open
  nodes = survey.logContent(fakeDocument(), next, {}, recorder(), survey.logMemory());
  assert.equal(header(nodes, "log-lands").getAttribute("aria-expanded"), "true");
});

test("a_folded_section_opens_for_its_form_and_its_refusal", () => {
  const data = fixture(), memory = survey.logMemory(), doc = fakeDocument(), on = recorder();
  const land = data.ledger.lands[0], sea = data.ledger.seas[0];
  let nodes = survey.logContent(doc, data, {}, on, memory);
  // folding Lands with an unsent rename open in it cancels that rename, as Cancel would
  btn([allByClass(nodes, "log-land")[0]], "Rename").dispatch("click", ev());
  assert.equal(memory.open && memory.open.key, `log:land:${land.id}:rename-form`);
  header(nodes, "log-lands").dispatch("click", ev());
  assert.equal(memory.open, null, "the unsent rename is cancelled");
  assert.equal(header(nodes, "log-lands").getAttribute("aria-expanded"), "false");
  // a form in a folded section, restored from memory: the section opens for it
  doc.activeElement = null; // the cancelled field's focus went with it
  memory.open = {key: `log:land:${land.id}:rename-form`, value: "NEW NAME", start: null, end: null, choice: null, error: null, sent: false};
  nodes = survey.logContent(doc, data, {}, on, memory);
  assert.equal(header(nodes, "log-lands").getAttribute("aria-expanded"), "true", "Lands opens for its form");
  assert.ok(!bodyOf(nodes, "log-lands").hidden);
  assert.equal(inputIn(bodyOf(nodes, "log-lands")).value, "NEW NAME");
  memory.open = null;
  // a sent rename: folding keeps it on its way; its refusal opens the section with the message
  nodes = survey.logContent(doc, data, {}, on, memory);
  const seaRow = allByClass(nodes, "log-sea")[0];
  btn([seaRow], "Rename").dispatch("click", ev());
  inputIn(seaRow).value = "FRESH WATERS";
  one([seaRow], "name-form").dispatch("submit", ev());
  assert.deepEqual(on.calls.at(-1), ["renameSea", sea.id, "FRESH WATERS"]);
  header(nodes, "log-seas").dispatch("click", ev());
  assert.ok(memory.open && memory.open.sent, "the sent rename waits for its reply");
  nodes = survey.logContent(doc, data, {busy: true}, on, memory);
  assert.equal(header(nodes, "log-seas").getAttribute("aria-expanded"), "false", "still folded while it is on its way");
  memory.result({kind: "refused", message: "That name is taken."});
  nodes = survey.logContent(doc, data, {}, on, memory);
  assert.equal(header(nodes, "log-seas").getAttribute("aria-expanded"), "true", "the refusal opens Seas");
  assert.ok(!bodyOf(nodes, "log-seas").hidden);
  assert.equal(one([bodyOf(nodes, "log-seas")], "name-form-error").textContent, "That name is taken.");
  // a done edit closes the form and leaves a folded section folded
  header(nodes, "log-seas").dispatch("click", ev()); // the refused form is unsent now: folding cancels it
  assert.equal(memory.open, null);
  nodes = survey.logContent(doc, data, {}, on, memory);
  assert.equal(header(nodes, "log-seas").getAttribute("aria-expanded"), "false");
});

test("enter_and_space_fold_a_section", () => {
  const nodes = survey.logContent(fakeDocument(), fixture(), {}, recorder(), survey.logMemory());
  const h = header(nodes, "log-seas"), body = bodyOf(nodes, "log-seas");
  pressKey(h, "Enter");
  assert.equal(h.getAttribute("aria-expanded"), "false", "Enter folds");
  assert.equal(body.hidden, true);
  pressKey(h, " ");
  assert.equal(h.getAttribute("aria-expanded"), "true", "Space opens");
  assert.ok(!body.hidden);
  pressKey(h, "a");
  assert.equal(h.getAttribute("aria-expanded"), "true", "another key does nothing");
});

test("sea_drop_index_follows_the_pointer", () => {
  const mids = [15, 45, 75];
  assert.equal(survey.seaDropIndex(mids, -50), 0, "above the rows");
  assert.equal(survey.seaDropIndex(mids, 15), 0, "on the first row's middle");
  assert.equal(survey.seaDropIndex(mids, 30), 1, "between the first and second");
  assert.equal(survey.seaDropIndex(mids, 60), 2, "between the second and third");
  assert.equal(survey.seaDropIndex(mids, 75), 2);
  assert.equal(survey.seaDropIndex(mids, 500), 2, "below the rows: the last");
  // the row under the pointer, its lower half included (rows 30 px tall: 0-30, 30-60, 60-90)
  assert.equal(survey.seaDropIndex(mids, 29), 0, "the first row's lower half");
  assert.equal(survey.seaDropIndex(mids, 50), 1, "the second row's lower half");
  assert.equal(survey.seaDropIndex(mids, 85), 2, "the third row's lower half");
  assert.equal(survey.seaDropIndex(mids, 31), 1, "the second row's upper half");
  assert.equal(survey.seaDropIndex([], 10), -1, "no rows");
});

// ---------- the name form ----------
test("name_form_submits_and_shows_errors", () => {
  const doc = fakeDocument();
  let on = recorder();
  let form = survey.nameForm(doc, {title: "Name this land", value: "Apple"}, on);
  const root = form.nodes[0];
  assert.ok(root, "the form is built");
  assert.equal(root.tagName, "FORM");
  assert.equal(root.className, "name-form");
  assert.ok(walk(form.nodes).some(n => n.nodeType === 3 && n.data === "Name this land"), "headed by its title");
  const input = tagged(form.nodes, "INPUT");
  assert.equal(input.getAttribute("maxlength"), "40");
  assert.equal(input.value, "Apple");
  const enter = ev({key: "Enter", target: input});
  root.dispatch("keydown", enter);
  assert.deepEqual(on.calls, [["submit", "Apple", null]]);
  assert.equal(enter.defaultPrevented, true);
  root.dispatch("submit", ev());
  assert.deepEqual(on.calls[1], ["submit", "Apple", null], "the submit button (a form submit) too");
  root.dispatch("keydown", ev({key: "Escape", target: input}));
  btn(form.nodes, "Cancel").dispatch("click", ev());
  assert.deepEqual(on.calls.slice(2), [["cancel"], ["cancel"]]);

  const error = one(form.nodes, "name-form-error");
  form.setError(T.landTaken);
  assert.equal(error.textContent, T.landTaken);
  form.setError(null);
  assert.equal(error.textContent, "");

  on = recorder();
  form = survey.nameForm(doc, {title: "Name this land", value: "Apple", withChoices: [{id: 7, label: "The Northern Courier"}, {id: 9, label: "The Pale Signal"}]}, on);
  const select = tagged(form.nodes, "SELECT");
  assert.ok(walk(form.nodes).some(n => n.nodeType === 3 && n.data === "With:"));
  assert.equal(select.value, "", "no default choice");
  const submit = buttons(form.nodes).find(b => b.type === "submit");
  assert.equal(submit.textContent, "Create", "the submit button is a verb, Create unless told");
  assert.equal(submit.disabled, true, "submit waits for a choice");
  form.nodes[0].dispatch("submit", ev());
  form.nodes[0].dispatch("keydown", ev({key: "Enter", target: tagged(form.nodes, "INPUT")}));
  assert.equal(on.calls.length, 0, "nothing submitted without a choice");
  select.value = "7";
  select.dispatch("change", ev());
  assert.equal(submit.disabled, false);
  form.nodes[0].dispatch("submit", ev());
  assert.deepEqual(on.calls, [["submit", "Apple", 7]], "the chosen id, as given");
});

// ---------- the session ----------
test("session_sends_one_edit_at_a_time", () => {
  const {session, posted, after, changes} = stubSession();
  assert.equal(session.pending(), false);
  assert.equal(session.send(JOIN), true);
  assert.deepEqual(posted, [{type: "edit", seq: 1, edit: JSON.stringify(JOIN)}]);
  assert.equal(session.pending(), true);
  assert.equal(changes(), 1, "onChange after send");
  assert.equal(session.send(JOIN), false, "one edit at a time");

  assert.deepEqual(session.edited(ok(1, "Join APPLE PRESS", patch("Join APPLE PRESS"))), {kind: "done", message: null, edit: JOIN},
    "done, with the edit that was done");
  assert.equal(session.pending(), true, "an ok result keeps the edit pending");
  assert.equal(session.send(JOIN), false);
  assert.equal(after(core.PENDING_MS).length, 1);
  session.updated();
  assert.equal(session.pending(), false, "the next Chart.update ends it");
  after(core.PENDING_MS)[0].fn();
  after(survey.REPLY_MS)[0].fn();
  assert.equal(session.pending(), false, "a stale timer does nothing");

  assert.equal(session.send(JOIN), true);
  assert.equal(posted[1].seq, 2, "seq counts up");
  session.updated();
  assert.equal(session.pending(), true, "an update before the result does not end it");
  session.edited(ok(2, "Join APPLE PRESS", patch("Join APPLE PRESS")));
  after(survey.REPLY_MS)[1].fn();
  assert.equal(session.pending(), true, "the reply timer does nothing once the result came");
  after(core.PENDING_MS)[1].fn();
  assert.equal(session.pending(), false, "PENDING_MS after an ok result ends it");

  assert.equal(session.send(JOIN), true);
  const before = changes();
  assert.deepEqual(session.edited(refused(3, T.landTaken)), {kind: "name", message: T.landTaken});
  assert.equal(session.pending(), false, "a refusal ends it at once");
  assert.equal(changes(), before + 1);
  assert.equal(session.send(JOIN), true, "and the next edit goes at once");
  assert.deepEqual(session.edited(failed(4, "ressac: no database")), {kind: "failed", message: "ressac: no database"});
  assert.equal(session.pending(), false);

  const big = core.edits.found("é".repeat(core.MAX_EDIT / 2), [1, 2]); // fits in UTF-16 units, not in UTF-8 bytes
  assert.ok(JSON.stringify(big).length < core.MAX_EDIT);
  const n = posted.length;
  assert.equal(session.send(big), false, "an oversized edit is not posted");
  assert.equal(posted.length, n);
  assert.equal(session.pending(), false);
});

test("session_never_sends_a_join_or_leave_without_a_land", () => {
  const {session, posted} = stubSession();
  for (const edit of [core.edits.join(12, null), core.edits.leave(1, null), core.edits.join(12, undefined), {op: "leave", feed: 1}]) {
    assert.equal(session.send(edit), false, JSON.stringify(edit));
  }
  assert.equal(posted.length, 0);
});

test("a_changed_refusal_shows_the_message_and_keeps_history", () => {
  const {session} = stubSession();
  session.send(JOIN);
  session.edited(ok(1, "Join APPLE PRESS", patch("Join APPLE PRESS")));
  session.updated();
  assert.deepEqual(session.titles(), {undo: "Undo Join APPLE PRESS", redo: null});
  session.send(core.edits.renameLand({id: 11, name: "APPLE PRESS"}, "Pear"));
  assert.deepEqual(session.edited(refused(2, T.changed)), {kind: "changed", message: T.changed});
  assert.deepEqual(session.titles(), {undo: "Undo Join APPLE PRESS", redo: null}, "history keeps its record");
  // any other refusal text reads as "changed", with the one message
  session.send(core.edits.deleteLand({id: 11, name: "APPLE PRESS"}));
  assert.deepEqual(session.edited(refused(3, "No feed #9.")), {kind: "changed", message: T.changed});
  session.send(core.edits.leave(37, {id: 12, name: "BELLMARK"}));
  assert.deepEqual(session.edited(refused(4, T.continentHint("BELLMARK"))), {kind: "continent", message: T.continentHint("BELLMARK")});
  assert.deepEqual(session.titles(), {undo: "Undo Join APPLE PRESS", redo: null});
});

test("session_undo_redo_round_trip", () => {
  const {session, posted} = stubSession();
  assert.equal(session.undo(), false, "nothing to undo");
  assert.equal(session.redo(), false);
  assert.equal(posted.length, 0);

  const undoJoin = patch("Join APPLE PRESS", 1), redoJoin = patch("Join APPLE PRESS", 2);
  session.send(JOIN);
  session.edited(ok(1, "Join APPLE PRESS", undoJoin));
  assert.equal(session.undo(), false, "not while the edit is pending");
  session.updated();
  assert.equal(session.undo(), true);
  assert.deepEqual(posted[1], {type: "edit", seq: 2, edit: JSON.stringify(undoJoin)}, "undo posts the record's patch");
  assert.equal(session.edited(ok(1, "Join APPLE PRESS", redoJoin)), null, "a result with a stale seq is ignored");
  assert.equal(session.edited(ok(99, "Join APPLE PRESS", redoJoin)), null);
  assert.equal(session.edited("not json"), null);
  assert.equal(session.pending(), true);
  session.edited(ok(2, "Join APPLE PRESS", redoJoin));
  session.updated();
  assert.deepEqual(session.titles(), {undo: null, redo: "Redo Join APPLE PRESS"}, "the undone step moves to redo");
  assert.equal(session.redo(), true);
  assert.deepEqual(posted[2], {type: "edit", seq: 3, edit: JSON.stringify(redoJoin)}, "with the new patch");
  session.edited(ok(3, "Join APPLE PRESS", undoJoin));
  session.updated();
  assert.deepEqual(session.titles(), {undo: "Undo Join APPLE PRESS", redo: null});

  // a failed undo keeps its record; a refused one drops it
  session.undo();
  assert.deepEqual(session.edited(failed(4, "ressac: timed out")), {kind: "failed", message: "ressac: timed out"});
  assert.deepEqual(session.titles(), {undo: "Undo Join APPLE PRESS", redo: null}, "failed: kept");
  session.undo();
  assert.deepEqual(session.edited(refused(5, T.changed)), {kind: "changed", message: T.changed});
  assert.deepEqual(session.titles(), {undo: null, redo: null}, "refused: dropped");

  // a refused redo drops its record too
  session.send(JOIN);
  session.edited(ok(6, "Join APPLE PRESS", undoJoin));
  session.updated();
  session.undo();
  session.edited(ok(7, "Join APPLE PRESS", redoJoin));
  session.updated();
  session.redo();
  session.edited(refused(8, T.changed));
  assert.deepEqual(session.titles(), {undo: null, redo: null});
});

test("an_edit_with_nothing_to_undo_is_no_step", () => {
  const {session} = stubSession();
  session.send(core.edits.orderSeas([1, 2, 3]));
  session.edited(ok(1, "Reorder Seas", core.edits.patch("Reorder Seas", [])));
  session.updated();
  assert.deepEqual(session.titles(), {undo: null, redo: null}, "an empty order_seas is never recorded");
});

// ---------- fix round 1: no reply, rebuilds that keep forms and focus, verbs, ids ----------
test("session_ends_an_unanswered_edit", () => {
  const {session, posted, after, timeouts, changes} = stubSession();
  assert.equal(survey.REPLY_MS, 15000);
  assert.equal(survey.NO_REPLY, "The app did not answer; the chart was reloaded.");
  session.send(JOIN);
  assert.equal(after(survey.REPLY_MS).length, 1, "a reply timer per send");
  const before = changes();
  after(survey.REPLY_MS)[0].fn();
  assert.equal(session.pending(), false, "an edit with no reply stops being pending");
  assert.deepEqual(posted.at(-1), {type: "survey"}, "and the page asks for a fresh snapshot, through post");
  assert.deepEqual(timeouts, [{kind: "timeout", message: survey.NO_REPLY}]);
  assert.equal(changes(), before + 1);
  assert.deepEqual(session.titles(), {undo: null, redo: null});
  assert.equal(session.send(JOIN), true, "the next edit goes at once");

  // an unanswered undo keeps its record, as a failed one does
  session.edited(ok(2, "Join APPLE PRESS", patch("Join APPLE PRESS")));
  session.updated();
  session.undo();
  after(survey.REPLY_MS).at(-1).fn();
  assert.deepEqual(session.titles(), {undo: "Undo Join APPLE PRESS", redo: null});
  assert.equal(posted.filter(m => m.type === "survey").length, 2);
});

// final review I-1: an edit can apply after the page gave up on it; its late ok reply still becomes a history step
test("a_late_ok_reply_records_its_undo_step", () => {
  const {session, after, changes} = stubSession();
  assert.equal(survey.lateReply("Join APPLE PRESS"), "The app answered late: Join APPLE PRESS was done.");
  session.send(JOIN);
  after(survey.REPLY_MS)[0].fn();
  assert.equal(session.pending(), false);
  let before = changes();
  assert.deepEqual(session.edited(ok(1, "Join APPLE PRESS", patch("Join APPLE PRESS"))),
    {kind: "late", message: survey.lateReply("Join APPLE PRESS"), edit: JOIN});
  assert.deepEqual(session.titles(), {undo: "Undo Join APPLE PRESS", redo: null}, "the step is in the history after all");
  assert.equal(session.pending(), false, "a late reply makes nothing pending");
  assert.equal(changes(), before + 1, "onChange: the titles changed");
  assert.equal(session.edited(ok(1, "Join APPLE PRESS", patch("Join APPLE PRESS"))), null, "a second copy is ignored");
  assert.deepEqual(session.titles(), {undo: "Undo Join APPLE PRESS", redo: null});

  // an undo whose reply comes late moves to the redo stack then
  assert.equal(session.undo(), true);
  after(survey.REPLY_MS)[1].fn();
  assert.deepEqual(session.titles(), {undo: "Undo Join APPLE PRESS", redo: null}, "kept while unanswered");
  assert.deepEqual(session.edited(ok(2, "Leave APPLE PRESS", patch("Leave APPLE PRESS"))),
    {kind: "late", message: survey.lateReply("Undo Join APPLE PRESS"), edit: patch("Join APPLE PRESS")}, "an undo's edit is its patch");
  assert.deepEqual(session.titles(), {undo: null, redo: "Redo Join APPLE PRESS"});

  // late while another edit is pending: recorded, and the other edit stays pending
  session.send(JOIN);
  after(survey.REPLY_MS)[2].fn();
  assert.equal(session.send(core.edits.newSea("Sea of Tests")), true);
  assert.equal(session.edited(ok(3, "Join APPLE PRESS", patch("Join APPLE PRESS"))).kind, "late");
  assert.equal(session.pending(), true, "the edit in flight is still pending");
  assert.deepEqual(session.edited(ok(4, "New Sea of Tests", patch("New Sea of Tests"))), {kind: "done", message: null, edit: core.edits.newSea("Sea of Tests")});
  session.updated();
  assert.deepEqual(session.titles(), {undo: "Undo New Sea of Tests", redo: null});
  session.undo();
  session.edited(ok(5, "New Sea of Tests", patch("Delete Sea of Tests")));
  session.updated();
  assert.deepEqual(session.titles(), {undo: "Undo Join APPLE PRESS", redo: "Redo New Sea of Tests"}, "both steps, in order");

  // a late refusal or failure only clears it
  session.send(JOIN);
  after(survey.REPLY_MS).at(-1).fn();
  before = changes();
  assert.equal(session.edited(refused(6, T.changed)), null);
  assert.deepEqual(session.titles(), {undo: "Undo Join APPLE PRESS", redo: "Redo New Sea of Tests"}, "nothing recorded");
  assert.equal(changes(), before);
});

test("an_undo_with_nothing_to_redo_leaves_no_step", () => {
  const {session} = stubSession();
  session.send(JOIN);
  session.edited(ok(1, "Join APPLE PRESS", patch("Join APPLE PRESS")));
  session.updated();
  session.undo();
  session.edited(ok(2, "Join APPLE PRESS", core.edits.patch("Join APPLE PRESS", [])));
  session.updated();
  assert.deepEqual(session.titles(), {undo: null, redo: null}, "no empty redo step");
});

const landRowIn = (nodes, id) => allByClass(nodes, "log-land").find(r => r.getAttribute("data-land") === String(id));
const seaRowIn = (nodes, id) => allByClass(nodes, "log-sea").find(r => r.getAttribute("data-sea") === String(id));
// the same chart surveyed again, after another writer renamed a land: what a Chart.update brings
function resurveyed() {
  const data = fixture();
  data.surveyed_at = "2026-11-27T18:00:00Z";
  data.ledger.lands.find(l => l.name === "GULL PRESS GROUP").name = "GULLS";
  return data;
}

test("rows_carry_their_ids", () => {
  const data = fixture(), nodes = log(data, {});
  assert.deepEqual(allByClass(nodes, "log-land").map(r => Number(r.getAttribute("data-land"))), data.ledger.lands.map(l => l.id));
  assert.deepEqual(allByClass(nodes, "log-sea").map(r => Number(r.getAttribute("data-sea"))), data.ledger.seas.map(t => t.id));
  const row = seaRowIn(nodes, 1);
  assert.equal(row.getAttribute("aria-label"), null, "no label that hides the count");
  const parts = row.getAttribute("aria-labelledby").split(" ").map(id => elements([row]).find(e => e.id === id).textContent);
  assert.deepEqual(parts, ["SEA OF KITES", "22 feeds"]);
});

test("an_open_form_survives_a_rebuild", () => {
  const doc = fakeDocument(), memory = survey.logMemory(), on = recorder();
  const nodes = survey.logContent(doc, fixture(), {}, on, memory);
  btn([landRowIn(nodes, 11)], "Rename").dispatch("click", ev());
  const input = tagged([landRowIn(nodes, 11)], "INPUT");
  assert.equal(doc.activeElement, input, "opening a form focuses its field");
  input.value = "gulls";
  input.selectionStart = 2; input.selectionEnd = 4;
  input.dispatch("input", ev());
  one([landRowIn(nodes, 11)], "name-form").dispatch("submit", ev()); // clashes with nothing yet: renamed elsewhere next
  assert.deepEqual(on.calls, [["renameLand", 11, "gulls"]]);
  on.calls.length = 0;

  // the core refuses the name (GULL PRESS GROUP became GULLS meanwhile): the busy rebuild keeps the form, disabled
  memory.result(undefined);
  let again = survey.logContent(doc, resurveyed(), {busy: true}, on, memory);
  let field = tagged([landRowIn(again, 11)], "INPUT");
  assert.equal(field.value, "gulls");
  assert.equal(field.disabled, true, "while busy");
  assert.equal(memory.restore(), null, "a disabled field takes no focus");
  doc.activeElement = doc.body; // the old field left the page
  memory.result({kind: "name", message: T.landTaken});
  again = survey.logContent(doc, resurveyed(), {message: T.landTaken}, on, memory);
  field = tagged([landRowIn(again, 11)], "INPUT");
  assert.equal(field.value, "gulls", "the typed value is kept");
  assert.ok(!field.disabled);
  assert.equal(one([landRowIn(again, 11)], "name-form-error").textContent, T.landTaken, "the core's message stays with the field");
  const focusing = [], focus = field.focus;
  field.focus = options => { focusing.push(options); focus.call(field, options); };
  assert.equal(memory.restore(), field, "the field has the focus again");
  assert.equal(doc.activeElement, field);
  assert.deepEqual(focusing, [{preventScroll: true}], "without scrolling the log back to it (final review Minor 2)");
  assert.deepEqual([field.selectionStart, field.selectionEnd], [2, 4], "and its caret");

  // typing more, then any number of plain updates: value, caret and error are kept
  field.value = "gulls two";
  field.selectionStart = field.selectionEnd = 9;
  field.dispatch("input", ev());
  one([landRowIn(again, 11)], "name-form").dispatch("submit", ev());
  on.calls.length = 0;
  memory.result({kind: "name", message: T.landTaken});
  for (let i = 0; i < 3; i++) {
    again = survey.logContent(doc, resurveyed(), {}, on, memory);
    memory.restore();
  }
  field = tagged([landRowIn(again, 11)], "INPUT");
  assert.equal(field.value, "gulls two");
  assert.deepEqual([field.selectionStart, field.selectionEnd], [9, 9]);
  assert.equal(one([landRowIn(again, 11)], "name-form-error").textContent, T.landTaken);
  assert.equal(doc.activeElement, field);

  // done: the form closes on the next rebuild
  one([landRowIn(again, 11)], "name-form").dispatch("submit", ev());
  memory.result({kind: "done", message: null});
  again = survey.logContent(doc, resurveyed(), {}, on, memory);
  assert.equal(byClass([landRowIn(again, 11)], "name-form"), undefined);
  assert.ok(button([landRowIn(again, 11)], "Rename"));
});

test("forms_survive_rebuilds_until_their_target_goes", () => {
  const doc = fakeDocument(), memory = survey.logMemory(), on = recorder();
  // New Land… keeps its name and choice
  let nodes = survey.logContent(doc, fixture(), {selected: 12}, on, memory);
  btn(nodes, "New Land…").dispatch("click", ev());
  let form = one(nodes, "name-form");
  tagged([form], "INPUT").value = "Trio";
  tagged([form], "INPUT").dispatch("input", ev());
  tagged([form], "SELECT").value = "3";
  tagged([form], "SELECT").dispatch("change", ev());
  nodes = survey.logContent(doc, resurveyed(), {selected: 12}, on, memory);
  form = one(nodes, "name-form");
  assert.equal(tagged([form], "INPUT").value, "Trio");
  assert.equal(tagged([form], "SELECT").value, "3");
  assert.equal(buttons([form]).find(b => b.type === "submit").disabled, false, "the kept choice enables submit");
  form.dispatch("submit", ev());
  assert.deepEqual(on.calls, [["newLand", "Trio", [3, 12]]]);
  // another island selected: that form is gone, and forgotten
  nodes = survey.logContent(doc, resurveyed(), {selected: 1}, on, memory);
  assert.equal(byClass(nodes, "name-form"), undefined);
  assert.equal(memory.open, null);

  // a pending Delete confirmation survives; its sea deleted elsewhere, it goes
  btn([seaRowIn(nodes, 3)], "Delete").dispatch("click", ev());
  nodes = survey.logContent(doc, resurveyed(), {}, on, memory);
  assert.ok(walk([seaRowIn(nodes, 3)]).some(n => n.nodeType === 3 && n.data === core.deleteSeaQuestion("GULF OF BELLS", 9, true)));
  const gone = resurveyed();
  gone.ledger.seas = gone.ledger.seas.filter(t => t.id !== 3);
  nodes = survey.logContent(doc, gone, {}, on, memory);
  assert.equal(memory.open, null);
  nodes = survey.logContent(doc, resurveyed(), {}, on, memory);
  assert.ok(button([seaRowIn(nodes, 3)], "Delete") && !walk([seaRowIn(nodes, 3)]).some(n => n.nodeType === 3 && /^Delete the sea/.test(n.data)));

  // Cancel forgets the form and gives the focus back to its button
  btn([seaRowIn(nodes, 1)], "Rename").dispatch("click", ev());
  btn([seaRowIn(nodes, 1)], "Cancel").dispatch("click", ev());
  assert.equal(memory.open, null);
  assert.equal(doc.activeElement, btn([seaRowIn(nodes, 1)], "Rename"));
  nodes = survey.logContent(doc, resurveyed(), {}, on, memory);
  assert.equal(byClass([seaRowIn(nodes, 1)], "name-form"), undefined);

  // a direct nameForm takes its verb
  const renameForm = survey.nameForm(doc, {title: "Rename", value: "X", action: "Rename"}, recorder());
  assert.equal(buttons(renameForm.nodes).find(b => b.type === "submit").textContent, "Rename");
});

test("focus_comes_back_to_the_same_control_after_a_rebuild", () => {
  const doc = fakeDocument(), memory = survey.logMemory(), on = recorder();
  // Alt+↑ twice on one sea: the row keeps the focus across the busy rebuild and the update after it
  let data = fixture();
  let nodes = survey.logContent(doc, data, {}, on, memory);
  seaRowIn(nodes, 2).focus();
  seaRowIn(nodes, 2).dispatch("keydown", ev({key: "ArrowUp", altKey: true}));
  assert.deepEqual(on.calls, [["moveSea", 2, -1]]);
  data = fixture();
  data.ledger.seas = [data.ledger.seas[1], data.ledger.seas[0], ...data.ledger.seas.slice(2)];
  nodes = survey.logContent(doc, data, {busy: true}, on, memory);
  assert.equal(memory.restore(), seaRowIn(nodes, 2), "a row is never disabled: it keeps the focus while busy");
  nodes = survey.logContent(doc, data, {}, on, memory);
  memory.restore();
  assert.equal(doc.activeElement, seaRowIn(nodes, 2));
  assert.equal(allByClass(nodes, "log-sea")[0], doc.activeElement, "at its new place");

  // a button disabled while busy gets the focus back once the edit is over
  btn([landRowIn(nodes, 11)], "Delete").focus();
  nodes = survey.logContent(doc, data, {busy: true}, on, memory);
  assert.equal(memory.restore(), null);
  doc.activeElement = doc.body; // the old button left the page
  nodes = survey.logContent(doc, data, {}, on, memory);
  assert.equal(memory.restore(), btn([landRowIn(nodes, 11)], "Delete"));
  assert.equal(doc.activeElement.getAttribute("data-key"), "log:land:11:delete");

  // focus that moved out of the log stays where it went
  doc.activeElement = doc.createElement("canvas");
  nodes = survey.logContent(doc, data, {}, on, memory);
  assert.equal(memory.restore(), null);
  doc.activeElement = doc.body;
  nodes = survey.logContent(doc, data, {}, on, memory);
  assert.equal(memory.restore(), null, "nor does it come back later");

  // the island select keeps the focus across a new selection
  one(nodes, "log-island").focus();
  nodes = survey.logContent(doc, data, {selected: 5}, on, memory);
  assert.equal(memory.restore(), one(nodes, "log-island"));
});

// ---------- Task 10: Survey mode, DOM-free (createPage draws from it) ----------
// a mode over a stub session; `available` may be a value or a function read at each toggle
function stubMode(available = true) {
  const s = stubSession();
  let changes = 0;
  const mode = survey.createMode({session: s.session, available, onChange: () => { changes += 1; }});
  return {...s, mode, modeChanges: () => changes};
}
const SEA_LABEL = {kind: "sea", key: "kites", text: "SEA OF KITES", box: [0, 0, 10, 10]};
const LAND_LABEL = {kind: "territory", text: "APPLE PRESS", box: [0, 0, 10, 10]};
const ISLAND = {kind: "island", id: 12};
// one edit done and surveyed: the history has one record to undo
function withOneEdit(s) {
  s.session.send(JOIN);
  s.session.edited(ok(1, "Join APPLE PRESS", patch("Join APPLE PRESS")));
  s.session.updated();
  return s;
}

test("survey_toggle_opens_and_closes_the_log", () => {
  const {mode, modeChanges} = stubMode();
  assert.deepEqual([mode.active, mode.logOpen, mode.renaming], [false, false, false], "Survey mode always starts off");
  assert.equal(mode.toggleLog(), false, "no log outside Survey mode");
  assert.equal(mode.logOpen, false);
  mode.toggle();
  assert.deepEqual([mode.active, mode.logOpen], [true, true], "turning on opens the log");
  assert.equal(modeChanges(), 1, "onChange after the toggle");
  assert.equal(mode.toggleLog(), true);
  assert.deepEqual([mode.active, mode.logOpen], [true, false], "the Log button closes it");
  mode.toggleLog();
  assert.deepEqual([mode.active, mode.logOpen], [true, true], "and opens it again");
  assert.equal(mode.rename(SEA_LABEL), true);
  mode.toggle();
  assert.deepEqual([mode.active, mode.logOpen, mode.renaming], [false, false, false], "turning off closes the log and the rename");
  assert.equal(modeChanges(), 5);

  for (const unavailable of [false, () => false]) {
    const off = stubMode(unavailable).mode;
    off.toggle();
    assert.deepEqual([off.active, off.logOpen], [false, false], "no bridge or no ledger: Survey stays off");
  }
  let ready = false;
  const later = stubMode(() => ready).mode;
  later.toggle();
  assert.equal(later.active, false);
  ready = true; // the first document with a ledger arrived
  later.toggle();
  assert.equal(later.active, true, "available is read at each toggle");
});

test("clicks_select_or_rename_only_in_survey", () => {
  const {mode} = stubMode();
  for (const target of [ISLAND, SEA_LABEL, LAND_LABEL, {...LAND_LABEL, kind: "continent"}, null]) {
    assert.equal(mode.click(target), "popover", `inactive: ${target && target.kind}`);
  }
  assert.equal(mode.rename(SEA_LABEL), false, "no rename outside Survey mode");
  assert.equal(mode.renaming, false);

  mode.toggle();
  assert.equal(mode.click(ISLAND), "select");
  assert.equal(mode.renaming, false);
  assert.equal(mode.click(SEA_LABEL), "rename");
  assert.equal(mode.renaming, true, "a name click opens its rename");
  mode.endRename();
  assert.equal(mode.renaming, false);
  for (const kind of ["territory", "continent"]) {
    assert.equal(mode.click({...LAND_LABEL, kind}), "rename", kind);
    mode.endRename();
  }
  for (const target of [ISLAND, null, {kind: "feed", text: "The Pale Signal"}, {kind: "harbour", text: "today"}]) {
    assert.notEqual(mode.click(target), "popover", "never the popover in Survey mode");
    assert.equal(mode.renaming, false, "only lands, continents and seas are renamed");
  }
});

test("undo_and_redo_do_nothing_outside_survey", () => {
  const s = withOneEdit(stubMode()), {mode, posted} = s;
  const sent = posted.length;
  assert.equal(mode.undo(), false, "inactive: no undo");
  assert.equal(mode.redo(), false);
  assert.equal(posted.length, sent, "nothing posted");
  assert.equal(s.session.pending(), false);

  mode.toggle();
  mode.rename(LAND_LABEL);
  assert.equal(mode.undo(), false, "not while a name is being typed");
  assert.equal(mode.redo(), false);
  assert.equal(posted.length, sent);

  mode.endRename();
  assert.equal(mode.undo(), true);
  assert.deepEqual(posted.at(-1), {type: "edit", seq: 2, edit: JSON.stringify(patch("Join APPLE PRESS"))}, "the patch is posted");
  s.session.edited(ok(2, "Join APPLE PRESS", patch("Join APPLE PRESS", 2)));
  mode.updated();
  mode.toggle();
  assert.equal(mode.redo(), false, "inactive: no redo either, with a step to redo");
  assert.equal(posted.length, sent + 1);
  mode.toggle();
  mode.rename(SEA_LABEL);
  assert.equal(mode.redo(), false, "nor while renaming");
  mode.endRename();
  assert.equal(mode.redo(), true, "redo through the mode");
  assert.deepEqual(posted.at(-1), {type: "edit", seq: 3, edit: JSON.stringify(patch("Join APPLE PRESS", 2))});
  s.session.edited(ok(3, "Join APPLE PRESS", patch("Join APPLE PRESS")));
  mode.updated();
  mode.toggle();
  assert.equal(mode.undo(), false, "off again: no undo");
  assert.equal(posted.length, sent + 2);
});

test("update_ends_a_pending_edit_through_the_mode", () => {
  const {mode, session} = stubMode();
  mode.toggle();
  session.send(JOIN);
  session.edited(ok(1, "Join APPLE PRESS", patch("Join APPLE PRESS")));
  assert.equal(session.pending(), true, "an ok result keeps the edit pending");
  mode.updated();
  assert.equal(session.pending(), false, "the next Chart.update ends it, through the mode");
  // and it forwards even with Survey mode off (an edit sent just before turning it off)
  session.send(JOIN);
  session.edited(ok(2, "Join APPLE PRESS", patch("Join APPLE PRESS")));
  mode.toggle();
  mode.updated();
  assert.equal(session.pending(), false);
});
