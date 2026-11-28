// Tests of the chart page's pure helpers (popover, tooltip, prefs, survey time, drawing commands), run with
// `node --test 'apps/macos/ChartTests/*.test.cjs'`. The page itself is checked by eye in preview.html.
"use strict";
process.env.TZ = "Europe/Paris"; // the cartouche shows local time: pin it before any Date is made
const test = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");

const CHART = path.join(__dirname, "..", "Ressac", "Resources", "Chart");
const draw = require(path.join(CHART, "chart-draw.js"));
const core = require(path.join(CHART, "chart-core.js"));

const fixture = () => JSON.parse(JSON.stringify(require("./fixture.json")));
const empty = () => ({...fixture(), feeds: [], territories: [], totals: {feeds: 0, unread_week: 0}});
const EMPTY = "No feed has published in six months";

const {measure, fakeDocument, fakeWindow, press, walk, elements, byClass, recordingContext} = require("./fake-dom.cjs");

const TRICKY_TITLE = '<b>Bold</b> & "Quoted" Notes <img src=x onerror=alert(1)> </option>';
const tricky = () => fixture().feeds.find(f => f.title.includes("<b>"));

test("feed_text_is_never_html", () => {
  const doc = fakeDocument(), feed = tricky();
  feed.entries[0].title = '<img src=x onerror="alert(1)"> & "more"';
  const data = fixture();
  data.feeds = data.feeds.map(f => (f.id === feed.id ? feed : f));
  const node = core.place(data, "themes").find(n => n.id === feed.id);

  const tip = draw.tooltipContent(doc, node);
  const name = tip[0];
  assert.equal(name.tagName, "B");
  assert.deepEqual(name.childNodes, [{nodeType: 3, data: TRICKY_TITLE}]);
  assert.ok(!elements(tip).some(e => e !== name && e.tagName === "B"), "the title's <b> must stay text");

  const pop = draw.popoverContent(doc, node, data, {});
  const all = walk(pop);
  assert.ok(all.some(n => n.nodeType === 3 && n.data === TRICKY_TITLE), "title as one text node");
  assert.ok(all.some(n => n.nodeType === 3 && n.data === '<img src=x onerror="alert(1)"> & "more"'), "entry title as one text node");
  assert.ok(!elements(pop).some(e => e.tagName === "IMG"), "no element made from feed text");
});

test("empty_chart_message", () => {
  for (const view of ["harbour", "themes"]) {
    const scene = draw.buildScene(empty(), view, measure);
    assert.equal(scene.nodes.length, 0);
    const {ctx, texts} = recordingContext();
    draw.drawChart(ctx, scene, "bleu");
    assert.ok(texts().includes(EMPTY), `${view}: the empty sea says so`);
    assert.ok(texts().includes("Ressac Harbour"), `${view}: the harbour is still there`);
  }
  const {ctx, texts} = recordingContext();
  draw.drawChart(ctx, draw.buildScene(fixture(), "harbour", measure), "bleu");
  assert.ok(!texts().includes(EMPTY), "a chart with feeds has no such message");
});

test("prefs_round_trip", () => {
  const DEFAULT = {view: "harbour", ink: "bleu", animate: true, reduceMotion: false};
  assert.deepEqual(draw.normalisePrefs(undefined), DEFAULT);
  assert.deepEqual(draw.normalisePrefs({view: "sideways", ink: "rouge", animate: "yes"}), DEFAULT);
  for (const gone of ["pop", "pirate"]) assert.equal(draw.normalisePrefs({view: "themes", ink: gone, animate: false}).ink, "bleu", gone);
  assert.deepEqual(draw.normalisePrefs({view: "toString", ink: "__proto__"}), DEFAULT);
  for (const view of ["harbour", "themes"]) for (const ink of ["bleu", "nuit"]) for (const animate of [true, false]) {
    const prefs = {view, ink, animate, reduceMotion: false}, message = draw.prefsMessage(prefs);
    assert.deepEqual(message, {type: "prefs", view, ink, animate});
    assert.deepEqual(draw.normalisePrefs(message), prefs); // what the app stores comes back unchanged
    assert.deepEqual(draw.normalisePrefs(JSON.parse(JSON.stringify(message))), prefs);
  }
});

test("survey_time_reads_like_a_cartouche", () => {
  assert.equal(draw.formatSurvey("2026-10-19T15:40:00Z"), "Monday 19 October 2026, 17:40");
  assert.equal(draw.formatSurvey("2026-01-04T08:05:00Z"), "Sunday 4 January 2026, 09:05");
  assert.equal(draw.formatSurvey("not a date"), "");
  assert.equal(draw.formatSurvey(undefined), "");
});

test("popover_lists_entries_first_then_facts", () => {
  const doc = fakeDocument(), data = fixture();
  const node = core.place(data, "harbour").find(n => n.feed.page != null && n.feed.entries.some(e => !e.read));
  const opened = [], urls = [];
  const pop = draw.popoverContent(doc, node, data, {openFeed: id => opened.push(id), openURL: (url, e) => urls.push([url, e])});
  assert.deepEqual(pop.map(e => e.className), ["pop-head", "pop-entries", "pop-meta", "pop-foot"]);

  const items = elements([pop[1]]).filter(e => e.tagName === "LI");
  assert.equal(items.length, node.feed.entries.length);
  node.feed.entries.forEach((entry, i) => {
    assert.equal(items[i].className, entry.read ? "" : "unread");
    const link = elements([items[i]]).find(e => e.tagName === "A");
    if (entry.url) {
      assert.equal(link.href, entry.url);
      assert.equal(link.target, undefined, "no new window inside the web view");
      const event = {};
      link.dispatch("click", event);
      assert.deepEqual(urls.at(-1), [entry.url, event]);
    } else assert.equal(link, undefined);
  });

  const facts = pop[2].textContent, theme = data.themes.find(t => t.key === node.feed.theme);
  assert.ok(facts.startsWith(theme.name), facts);
  assert.ok(facts.includes(`folder ${node.feed.folder}`), facts);
  assert.ok(facts.includes(`page ${node.feed.page}`), facts);
  assert.ok(facts.includes(`${node.unread} unread this week`), facts);

  byClass(pop, "pop-title").dispatch("click", {});
  assert.deepEqual(opened, [node.id]);
  assert.equal(pop[3].textContent, "Click the name to open this feed in Ressac.");

  const bare = {...node, feed: {...node.feed, page: null, folder: null, theme: null, entries: []}, theme: null};
  const facts2 = draw.popoverContent(doc, bare, data, {})[2].textContent;
  assert.ok(facts2.startsWith("Uncharted waters"), facts2);
  assert.ok(!facts2.includes("page") && !facts2.includes("folder"), facts2);
  assert.ok(draw.popoverContent(doc, bare, data, {})[1].textContent.includes("No entries yet."));
});

test("only_web_links_get_an_href", () => {
  const doc = fakeDocument(), data = fixture(), node = core.place(data, "themes")[0];
  const feed = {...node.feed, entries: [{id: 1, title: "a", url: "javascript:alert(1)", day: "2026-10-19", read: false},
    {id: 2, title: "b", url: "file:///etc/passwd", day: "2026-10-18", read: true},
    {id: 3, title: "c", url: "HTTPS://example.org/x", day: "2026-10-17", read: true}]};
  const links = elements(draw.popoverContent(doc, {...node, feed}, data, {})).filter(e => e.tagName === "A");
  assert.deepEqual(links.map(a => a.href), ["HTTPS://example.org/x"]);
});

test("tooltip_counts_the_week", () => {
  const doc = fakeDocument();
  const line = n => draw.tooltipContent(doc, n)[1].textContent;
  assert.equal(line({title: "A", unread: 3, week: 5}), "3 unread · 5 entries this week");
  assert.equal(line({title: "A", unread: 0, week: 1}), "nothing unread · 1 entry this week");
  assert.equal(draw.tooltipContent(doc, {title: "  ", unread: 0, week: 0})[0].textContent, "(untitled)");
});

test("tooltip_stays_inside_the_window", () => {
  // a 200 × 60 tooltip in a 1000 × 700 window: right and below the pointer, 12 px away, unless that edge is too near
  const at = (x, y) => draw.tooltipPosition(x, y, 200, 60, 1000, 700);
  assert.deepEqual(at(400, 300), {left: 412, top: 312});
  assert.deepEqual(at(900, 300), {left: 688, top: 312}, "near the right edge: on the left of the pointer");
  assert.deepEqual(at(400, 680), {left: 412, top: 608}, "near the bottom: above the pointer");
  assert.deepEqual(at(990, 695), {left: 778, top: 623}, "bottom right corner: left and above");
  assert.deepEqual(at(0, 0), {left: 12, top: 12}, "top left corner: right and below");
  assert.deepEqual(draw.tooltipPosition(100, 50, 200, 60, 150, 80), {left: 8, top: 8}, "a window smaller than the tip");
});

test("load_accepts_a_string_or_an_object", () => {
  const text = JSON.stringify(fixture());
  assert.deepEqual(draw.parseData(text), draw.parseData(fixture()));
  assert.throws(() => draw.parseData({...fixture(), version: 2}), /version/);
  assert.throws(() => draw.parseData("{"), SyntaxError);
  const bare = draw.parseData({version: 1, surveyed_at: "2026-10-19T15:40:00Z"});
  assert.deepEqual([bare.themes, bare.feeds, bare.territories, bare.continents], [[], [], [], []]);
});

test("bridge_posts_only_when_present", () => {
  const sent = [];
  draw.bridge({webkit: {messageHandlers: {ressac: {postMessage: m => sent.push(m)}}}})({type: "openFeed", id: 4});
  assert.deepEqual(sent, [{type: "openFeed", id: 4}]);
  assert.equal(draw.bridge({}), null);
  assert.equal(draw.bridge({webkit: {messageHandlers: {}}}), null);
});

test("every_placed_label_is_drawn", () => {
  for (const view of ["harbour", "themes"]) for (const ink of ["bleu", "nuit", "pop"]) { // "pop" is a retired ink: it draws as blue
    const scene = draw.buildScene(fixture(), view, measure), {ctx, texts} = recordingContext();
    draw.drawChart(ctx, scene, ink);
    const drawn = texts().join("\u0000");
    scene.placements.forEach(p => {
      if (p.track) [...p.text].forEach(c => assert.ok(texts().includes(c), `${view}: ${p.kind} ${p.text}`));
      else assert.ok(drawn.includes(p.text), `${view}: ${p.kind} ${p.text}`);
    });
    assert.ok(texts().includes("Surveyed Monday 19 October 2026, 17:40"), "the cartouche carries the survey time");
    assert.ok(!texts().some(t => /^X marks/.test(t)), "no treasure X on any ink");
  }
});

// ---------- Task 10: the Chart object, Reduce Motion, and Survey mode in the page ----------
const fs = require("node:fs");
const vm = require("node:vm");
const d3 = require(path.join(CHART, "d3.min.js"));
const survey = require(path.join(CHART, "chart-survey.js"));
const T = core.EDIT_TEXT;

test("chart_exposes_edited_undo_and_redo", () => {
  // chart-draw.js as the page runs it (no `module`): what it leaves on the window
  const win = {ChartCore: core, d3, ChartSurvey: survey};
  vm.runInNewContext(fs.readFileSync(path.join(CHART, "chart-draw.js"), "utf8"), win);
  assert.deepEqual(Object.keys(win.Chart), ["load", "update", "edited", "undo", "redo"]);
  assert.ok(Object.isFrozen(win.Chart));
  for (const k of Object.keys(win.Chart)) assert.equal(typeof win.Chart[k], "function", k);
  assert.deepEqual(Object.keys(draw.chart), ["load", "update", "edited", "undo", "redo"], "the same object for Node");
});

test("prefs_keep_reduce_motion", () => {
  assert.equal(draw.normalisePrefs({reduceMotion: true}).reduceMotion, true);
  assert.equal(draw.normalisePrefs({reduceMotion: false}).reduceMotion, false);
  for (const odd of ["true", 1, null, undefined, {}]) assert.equal(draw.normalisePrefs({reduceMotion: odd}).reduceMotion, false, String(odd));
  assert.equal(draw.normalisePrefs({view: "themes", ink: "nuit", animate: false, reduceMotion: true}).view, "themes", "the rest unchanged");
});

test("prefs_message_never_carries_reduce_motion", () => {
  for (const reduceMotion of [true, false]) {
    const message = draw.prefsMessage(draw.normalisePrefs({view: "themes", ink: "nuit", animate: false, reduceMotion}));
    assert.deepEqual(message, {type: "prefs", view: "themes", ink: "nuit", animate: false});
    assert.ok(!("reduceMotion" in message), "the system's setting is never stored as the page's");
  }
});

// ---------- the page on a fake window ----------
const settle = () => new Promise(r => setImmediate(r)); // documents go in through a promise queue
const toolbarButton = (doc, text) => {
  const b = elements([doc.body]).find(e => e.tagName === "BUTTON" && e.textContent === text);
  assert.ok(b, `a toolbar button ${text}`);
  return b;
};
const drawerOf = doc => byClass([doc.body], "drawer");
const canvases = doc => elements([doc.body]).filter(e => e.tagName === "CANVAS");
const landRow = (doc, id) => elements([drawerOf(doc)]).find(e => e.getAttribute("data-land") === String(id));
const inputIn = node => { const i = elements([node]).find(e => e.tagName === "INPUT"); assert.ok(i, "a name field"); return i; };
const buttonIn = (node, text) => { const b = elements([node]).find(e => e.tagName === "BUTTON" && e.textContent === text); assert.ok(b, `a button ${text}`); return b; };
const ev = extra => ({preventDefault() {}, stopPropagation() {}, ...extra});
const edits = win => win.posted.filter(m => m.type === "edit");
async function openPage({bridge = true, data = fixture(), prefs = {}, reduced = false} = {}) {
  const win = fakeWindow({bridge, reduced}), page = draw.createPage(win), doc = win.document;
  page.load(JSON.stringify(data), {view: "themes", ink: "bleu", animate: false, ...prefs});
  await settle();
  assert.deepEqual(win.errors, [], "the document drew");
  return {win, page, doc};
}
// the same chart surveyed again a minute later: every Chart.update during a fetch looks like this
const resurveyed = (minute = 1) => ({...fixture(), surveyed_at: `2026-10-19T15:4${minute}:00Z`});

test("survey_mode_in_the_page", async () => {
  // no bridge (the preview), or a document without a ledger: no Survey button
  let {doc, page} = await openPage({bridge: false});
  assert.equal(toolbarButton(doc, "✒ Survey").hidden, true, "no bridge: hidden");
  assert.equal(page.undo(), false);
  const bare = fixture();
  delete bare.ledger;
  ({doc} = await openPage({data: bare}));
  assert.equal(toolbarButton(doc, "✒ Survey").hidden, true, "no ledger: hidden");

  let win;
  ({win, doc, page} = await openPage());
  const surveyButton = toolbarButton(doc, "✒ Survey"), logButton = toolbarButton(doc, "Log");
  const bar = byClass([doc.body], "bar"), order = elements([bar]).filter(e => e.tagName === "BUTTON").map(b => b.textContent);
  assert.equal(order[order.indexOf("Key") + 1], "✒ Survey", "right after Key");
  assert.equal(surveyButton.hidden, false);
  assert.equal(surveyButton.getAttribute("aria-pressed"), "false", "Survey mode starts off");
  assert.equal(logButton.hidden, true, "no Log button outside Survey mode");
  assert.equal(drawerOf(doc).hidden, true);
  assert.equal(canvases(doc)[0].width, 1920);

  surveyButton.dispatch("click", ev());
  assert.equal(surveyButton.getAttribute("aria-pressed"), "true");
  assert.ok(doc.body.classList.contains("survey"), "body.survey: the grid and the quill");
  assert.equal(logButton.hidden, false);
  assert.equal(logButton.getAttribute("aria-expanded"), "true");
  assert.equal(drawerOf(doc).hidden, false, "the log opens with Survey mode");
  assert.ok(byClass([drawerOf(doc)], "log"), "the Cartographer's Log is in the drawer");
  assert.equal(canvases(doc)[0].width, 1920 - 320 - 12, "the chart keeps clear of the drawer, with a margin before it");

  logButton.dispatch("click", ev());
  assert.equal(drawerOf(doc).hidden, true, "the Log button closes it");
  assert.equal(logButton.getAttribute("aria-expanded"), "false");
  assert.equal(canvases(doc)[0].width, 1920);
  logButton.dispatch("click", ev());
  assert.equal(drawerOf(doc).hidden, false);

  // one edit from the log, answered and surveyed: one step to undo
  buttonIn(landRow(doc, 11), "Delete").dispatch("click", ev());
  assert.deepEqual(edits(win).map(m => JSON.parse(m.edit)), [core.edits.deleteLand({id: 11, name: "APPLE PRESS"})]);
  const undoPatch = core.edits.patch("Delete APPLE PRESS", [{kind: "land", before: null, after: {id: 11, name: "APPLE PRESS", continent: null}}]);
  page.edited(JSON.stringify({seq: 1, ok: true, done: {event: "chart_edit", label: "Delete APPLE PRESS", undo: undoPatch}}));
  page.update(JSON.stringify(resurveyed()));
  await settle();
  assert.ok(!buttonIn(drawerOf(doc), "Undo Delete APPLE PRESS").disabled, "the log shows the step");

  surveyButton.dispatch("click", ev());
  assert.equal(surveyButton.getAttribute("aria-pressed"), "false");
  assert.ok(!doc.body.classList.contains("survey"));
  assert.equal(drawerOf(doc).hidden, true, "off: the log closes");
  assert.equal(logButton.hidden, true);
  assert.equal(page.undo(), false, "Chart.undo does nothing outside Survey mode");
  assert.equal(page.redo(), false);
  assert.equal(edits(win).length, 1, "nothing posted");

  surveyButton.dispatch("click", ev());
  assert.equal(page.undo(), true, "in Survey mode it undoes");
  assert.deepEqual(JSON.parse(edits(win).at(-1).edit), undoPatch);
  assert.deepEqual(win.errors, []);
});

test("an_open_log_form_survives_chart_update", async () => {
  const {win, doc, page} = await openPage();
  toolbarButton(doc, "✒ Survey").dispatch("click", ev());
  buttonIn(landRow(doc, 11), "Rename").dispatch("click", ev());
  let field = inputIn(landRow(doc, 11));
  assert.equal(doc.activeElement, field);
  field.value = "Gull Press Group";
  field.selectionStart = 3; field.selectionEnd = 7;
  field.dispatch("input", ev());

  // live refresh while typing: the form, its value and caret, and the focus survive
  page.update(JSON.stringify(resurveyed(1)));
  await settle();
  field = inputIn(landRow(doc, 11));
  assert.equal(field.value, "Gull Press Group", "the typed value");
  assert.deepEqual([field.selectionStart, field.selectionEnd], [3, 7], "the caret");
  assert.equal(doc.activeElement, field, "the focus");

  // the page's own check refuses the clash: no edit, the form stays with its message
  byClass([landRow(doc, 11)], "name-form").dispatch("submit", ev());
  assert.equal(edits(win).length, 0);
  assert.equal(byClass([landRow(doc, 11)], "name-form-error").textContent, T.landTaken);

  // a name the core refuses (another writer took it meanwhile): the form stays open with the core's text
  field = inputIn(landRow(doc, 11));
  field.value = "Gulls";
  field.selectionStart = field.selectionEnd = 5;
  field.dispatch("input", ev());
  byClass([landRow(doc, 11)], "name-form").dispatch("submit", ev());
  assert.deepEqual(JSON.parse(edits(win).at(-1).edit), core.edits.renameLand({id: 11, name: "APPLE PRESS"}, "Gulls"));
  page.edited(JSON.stringify({seq: 1, ok: false, refused: true, error: T.landTaken}));
  field = inputIn(landRow(doc, 11));
  assert.equal(byClass([landRow(doc, 11)], "name-form-error").textContent, T.landTaken, "the core's refusal shows at once");

  for (const minute of [2, 3, 4]) { // more surveys arrive while the message is on screen
    page.update(JSON.stringify(resurveyed(minute)));
    await settle();
  }
  field = inputIn(landRow(doc, 11));
  assert.equal(field.value, "Gulls");
  assert.deepEqual([field.selectionStart, field.selectionEnd], [5, 5]);
  assert.equal(byClass([landRow(doc, 11)], "name-form-error").textContent, T.landTaken, "the error");
  assert.equal(doc.activeElement, field);
  assert.equal(page.undo(), false, "⌘Z in a name field is the field's, not the chart's");

  // done: the form closes
  byClass([landRow(doc, 11)], "name-form").dispatch("submit", ev());
  page.edited(JSON.stringify({seq: 2, ok: true, done: {event: "chart_edit", label: "Rename Gulls", undo: core.edits.patch("Rename Gulls", [{kind: "land"}])}}));
  assert.equal(byClass([landRow(doc, 11)], "name-form"), undefined);

  // with a step to undo: ⌘Z while typing in a log field still belongs to the field
  page.update(JSON.stringify(resurveyed(5)));
  await settle();
  buttonIn(landRow(doc, 1), "Rename").dispatch("click", ev());
  assert.equal(doc.activeElement, inputIn(landRow(doc, 1)));
  const sent = edits(win).length;
  assert.equal(page.undo(), false, "the field has the focus: no chart undo");
  assert.equal(page.redo(), false);
  assert.equal(edits(win).length, sent);
  buttonIn(landRow(doc, 1), "Cancel").dispatch("click", ev()); // the focus goes back to the Rename button
  assert.equal(page.undo(), true, "out of the field, ⌘Z undoes the rename");
  assert.deepEqual(JSON.parse(edits(win).at(-1).edit), core.edits.patch("Rename Gulls", [{kind: "land"}]));
  assert.deepEqual(win.errors, []);
});

test("renaming_in_place_over_the_label", async () => {
  const {win, doc, page} = await openPage();
  const plate = byClass([doc.body], "plate"), base = canvases(doc)[0];
  const sea = draw.buildScene(fixture(), "themes", measure).placements.find(p => p.kind === "sea" && p.key === "tides");
  assert.ok(sea, "TIDAL SEA has its name on the chart");
  // the drawer takes 320 px and the chart keeps 12 px clear of it: 1588 wide
  const click = (p, k, ox, oy) => plate.dispatch("click", ev({target: base, clientX: ox + p.x * k, clientY: oy + p.y * k}));
  const renameBox = () => byClass([plate], "rename");

  click(sea, 1, 0, 0);
  assert.ok(!renameBox() || renameBox().hidden, "outside Survey mode a name is not renamed");

  toolbarButton(doc, "✒ Survey").dispatch("click", ev());
  const k = 1588 / 1920, oy = (1080 - 1080 * k) / 2;
  click(sea, k, 0, oy);
  const box = renameBox();
  assert.ok(box && !box.hidden, "a click on a sea's name opens its rename");
  let field = inputIn(box);
  assert.equal(field.value, "TIDAL SEA");
  assert.equal(doc.activeElement, field);
  assert.equal(box.style.left, `${sea.x * k}px`, "over the label");
  assert.equal(box.style.top, `${oy + sea.y * k}px`);
  assert.equal(page.undo(), false, "no undo while renaming");

  byClass([box], "name-form").dispatch("submit", ev());
  assert.ok(box.hidden || !box.childNodes.length, "an unchanged name cancels");
  assert.equal(edits(win).length, 0);

  click(sea, k, 0, oy);
  field = inputIn(renameBox());
  field.value = "Sea of Kites";
  byClass([renameBox()], "name-form").dispatch("submit", ev());
  assert.equal(byClass([renameBox()], "name-form-error").textContent, T.seaTaken, "the page's own check");
  field.value = "Tidal Reach";
  byClass([renameBox()], "name-form").dispatch("submit", ev());
  assert.deepEqual(JSON.parse(edits(win).at(-1).edit), core.edits.renameSea({id: 2, key: "tides", sea: "TIDAL SEA"}, "Tidal Reach"));
  page.edited(JSON.stringify({seq: 1, ok: false, refused: true, error: T.seaTaken}));
  assert.ok(!renameBox().hidden, "a name refusal keeps it open");
  assert.equal(byClass([renameBox()], "name-form-error").textContent, T.seaTaken);
  page.update(JSON.stringify(resurveyed()));
  await settle();
  assert.equal(inputIn(renameBox()).value, "Tidal Reach", "an update leaves the field as typed");
  byClass([renameBox()], "name-form").dispatch("submit", ev());
  page.edited(JSON.stringify({seq: 2, ok: true, done: {event: "chart_edit", label: "Rename Tidal Reach", undo: core.edits.patch("Rename Tidal Reach", [{kind: "sea"}])}}));
  assert.ok(renameBox().hidden, "done: the form goes");
  assert.equal(page.undo(), false, "and the edit is still pending until the next survey");
  page.update(JSON.stringify(resurveyed(2)));
  await settle();
  assert.equal(page.undo(), true, "then ⌘Z undoes it");

  // an island click in Survey mode selects it in the log
  const node = core.place(fixture(), "themes").find(n => n.id === 12);
  page.edited(JSON.stringify({seq: 3, ok: false, refused: false, error: "x"}));
  click(node, k, 0, oy);
  assert.equal(byClass([drawerOf(doc)], "log-island").value, "12");
  assert.ok(!byClass([doc.body], "pop") || byClass([doc.body], "pop").hidden, "no popover in Survey mode");
  assert.deepEqual(win.errors, []);
});

// final review I-1: a reply that comes after the page gave up becomes an undo step, and leaves the edit in flight alone
test("a_late_reply_in_the_page_adds_its_step_and_keeps_the_next_edit", async () => {
  const {win, doc, page} = await openPage();
  const plate = byClass([doc.body], "plate"), base = canvases(doc)[0];
  const sea = draw.buildScene(fixture(), "themes", measure).placements.find(p => p.kind === "sea" && p.key === "tides");
  const k = 1588 / 1920, oy = (1080 - 1080 * k) / 2;
  const click = p => plate.dispatch("click", ev({target: base, clientX: p.x * k, clientY: oy + p.y * k}));
  const renameBox = () => byClass([plate], "rename");
  const rename = name => { click(sea); inputIn(renameBox()).value = name; byClass([renameBox()], "name-form").dispatch("submit", ev()); };
  toolbarButton(doc, "✒ Survey").dispatch("click", ev());

  rename("Tidal Reach");
  assert.equal(edits(win).length, 1);
  const reply = win.timers.filter(t => t.ms === survey.REPLY_MS);
  assert.equal(reply.length, 1);
  reply[0].fn(); // the app did not answer in time
  assert.equal(statusLine(doc), survey.NO_REPLY);
  assert.ok(renameBox().hidden, "the form went with the timeout");

  rename("Tidal Waters"); // the next edit, in flight
  assert.equal(edits(win).length, 2);
  page.edited(JSON.stringify({seq: 1, ok: true, done: {event: "chart_edit", label: "Rename Tidal Reach",
    undo: core.edits.patch("Rename Tidal Reach", [{kind: "sea"}])}}));
  assert.equal(statusLine(doc), survey.lateReply("Rename Tidal Reach"), "the status says what happened");
  assert.ok(!renameBox().hidden, "the rename in flight still waits for its own reply");
  page.edited(JSON.stringify({seq: 2, ok: true, done: {event: "chart_edit", label: "Rename Tidal Waters",
    undo: core.edits.patch("Rename Tidal Waters", [{kind: "sea"}])}}));
  assert.ok(renameBox().hidden, "its own reply closes it");
  page.update(JSON.stringify(resurveyed()));
  await settle();
  assert.equal(page.undo(), true);
  assert.equal(JSON.parse(edits(win).at(-1).edit).label, "Rename Tidal Waters");
  page.edited(JSON.stringify({seq: 3, ok: true, done: {event: "chart_edit", label: "Rename Tidal Waters",
    undo: core.edits.patch("Rename Tidal Waters", [{kind: "sea"}])}}));
  page.update(JSON.stringify(resurveyed(2)));
  await settle();
  assert.equal(page.undo(), true, "the late step is undone next");
  assert.equal(JSON.parse(edits(win).at(-1).edit).label, "Rename Tidal Reach");
  assert.deepEqual(win.errors, []);
});

// final review I-3 (spec rule 5): a sea's name is drawn in capitals, in the italic face water names keep
test("sea_names_are_drawn_in_capitals", () => {
  const data = fixture();
  const tides = data.themes.find(t => t.key === "tides");
  tides.sea = "Tidal Sea of Tests";
  const placements = draw.buildScene(data, "themes", measure).placements;
  const sea = placements.find(p => p.kind === "sea" && p.key === "tides");
  assert.ok(sea, "the sea has its name on the chart");
  assert.equal(sea.text, "TIDAL SEA OF TESTS");
  assert.equal(sea.font, `italic 500 ${sea.size}px "Cormorant Garamond", Garamond, serif`, "water names stay italic");
  // seas already in capitals draw as before
  const kites = placements.find(p => p.kind === "sea" && p.text === "SEA OF KITES");
  assert.ok(kites, "an imported sea keeps its text");
});

test("reduce_motion_from_the_app_stills_the_page", async () => {
  let {win, doc} = await openPage({prefs: {animate: true}});
  assert.ok(win.frames.length > 0, "positive control: the overlay animates");
  assert.ok(!doc.body.classList.contains("still"));
  ({win, doc} = await openPage({prefs: {animate: true, reduceMotion: true}}));
  assert.equal(win.frames.length, 0, "Reduce Motion from the app: no animation frame");
  assert.ok(doc.body.classList.contains("still"), "body.still: no CSS transition either");
  assert.equal(toolbarButton(doc, "Animation").getAttribute("aria-pressed"), "false");
  ({win, doc} = await openPage({prefs: {animate: true}, reduced: true}));
  assert.equal(win.frames.length, 0, "the system's own setting too");
  assert.ok(doc.body.classList.contains("still"));
});

// ---------- gestures on the chart: drag and drop, the quill's loop, the cartouche (Task 11) ----------
test("the_real_scene_feeds_drop_target", () => {
  for (const view of ["harbour", "themes"]) {
    const data = fixture(), scene = draw.buildScene(data, view, measure);
    assert.equal(scene.view, view);
    assert.ok(Array.isArray(scene.nodes) && scene.nodes.length === data.feeds.length, `${view}: every feed has an island`);
    assert.deepEqual(scene.territories, core.territories(scene.nodes, data), `${view}: the scene keeps territories()`);
    assert.equal(typeof scene.ground.at, "function");
    assert.deepEqual(core.sceneProblems(scene, data), [], `${view}: the real scene is what dropTarget reads (S4)`);
  }
  const data = fixture(), scene = draw.buildScene(data, "themes", measure), node = id => scene.nodes.find(n => n.id === id);
  // 12 and 13 are lone islands of SEA OF KITES; 10 and 11 make APPLE PRESS
  assert.deepEqual(core.dropTarget(scene, data, node(13), node(12).x, node(12).y), {kind: "pair", with: 12});
  const apple = data.ledger.lands.find(l => l.name === "APPLE PRESS");
  assert.deepEqual(core.dropTarget(scene, data, node(13), node(10).x, node(10).y), {kind: "join", land: apple});
});

test("a_reduced_spring_back_lands_home_without_frames", () => {
  const counting = () => { const frames = []; return {frames, requestAnimationFrame: fn => { frames.push(fn); return frames.length; }}; };
  const node = {id: 7, x: 300, y: 200, r: 20};
  // Reduce Motion: home at once, no frame, done once
  let win = counting(), ghost = {x: 640, y: 480}, done = 0;
  assert.equal(typeof draw.springBack, "function", "ChartDraw.springBack");
  draw.springBack(ghost, node, win, false, () => { done += 1; });
  assert.deepEqual([ghost.x, ghost.y], [300, 200], "back at its place");
  assert.equal(done, 1);
  assert.equal(win.frames.length, 0, "no animation frame");
  // positive control: with motion it takes frames, and ends at the same place
  win = counting(); ghost = {x: 640, y: 480}; done = 0;
  draw.springBack(ghost, node, win, true, () => { done += 1; });
  assert.ok(win.frames.length >= 1, "motion: frames are scheduled");
  assert.equal(done, 0, "not done before its frames");
  let flushed = 0, t = 0;
  while (win.frames.length && flushed < 1000) { win.frames.shift()(t += 16); flushed += 1; }
  assert.ok(flushed > 1, "it moves over several frames");
  assert.deepEqual([ghost.x, ghost.y], [300, 200], "and lands home");
  assert.equal(done, 1, "done once");
});

// what a cartouche of `size` centred at `c` would cover: islands (r + 6), the compass rose, the title cartouche, labels
function coveredBy(c, size, scene, box) {
  const r = [c.x - size.w / 2, c.y - size.h / 2, c.x + size.w / 2, c.y + size.h / 2];
  const meets = (a, b) => a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3];
  const touches = d => Math.hypot(Math.max(r[0] - d.x, 0, d.x - r[2]), Math.max(r[1] - d.y, 0, d.y - r[3])) < d.r;
  const T = core.CARTOUCHE;
  return {
    islands: (scene.nodes || []).filter(n => touches({x: n.x, y: n.y, r: n.r + 6})).map(n => n.id),
    compass: touches({x: core.COMPASS.x, y: core.COMPASS.y, r: core.COMPASS.r + 12}),
    title: meets(r, [T.x, T.y, T.x + T.w, T.y + T.h]),
    land: box ? meets(r, box) : false,
    labels: (scene.placements || []).filter(p => meets(r, p.box)).length,
    inside: r[0] >= 40 - 1e-6 && r[1] >= 40 - 1e-6 && r[2] <= 1880 + 1e-6 && r[3] <= 1040 + 1e-6,
  };
}
const clean = (cov, labels = 0) => ({islands: [], compass: false, title: false, land: false, labels, inside: true});
// how far a cartouche of `size` centred at `c` is from the land's box
const gapTo = (c, size, box) => Math.hypot(Math.max(box[0] - (c.x + size.w / 2), 0, (c.x - size.w / 2) - box[2]),
  Math.max(box[1] - (c.y + size.h / 2), 0, (c.y - size.h / 2) - box[3]));
// the nearest a cartouche can be to the land while clear of islands, the rose, the title and the land (brute force, 4-unit lattice)
function nearestClear(box, scene, size, keep = []) {
  let best = Infinity;
  for (let y = 40; y + size.h <= 1040; y += 4) for (let x = 40; x + size.w <= 1880; x += 4) {
    const c = {x: x + size.w / 2, y: y + size.h / 2}, cov = coveredBy(c, size, {nodes: [...scene.nodes, ...keep]}, box);
    if (!cov.islands.length && !cov.compass && !cov.title && !cov.land) best = Math.min(best, gapTo(c, size, box));
  }
  return best;
}

test("the_cartouche_keeps_clear_of_islands_the_rose_and_the_title", () => {
  const size = {w: 200, h: 100}, box = [800, 500, 900, 560];
  // nothing in the way: exactly cartouchePlace's spot
  assert.deepEqual(draw.cartoucheAt(box, {nodes: [], placements: []}, size), core.cartouchePlace(box, [], size));
  // an island where cartouchePlace would put it: elsewhere, covering nothing
  const right = core.cartouchePlace(box, [], size), island = {id: 1, x: right.x, y: right.y, r: 20};
  let c = draw.cartoucheAt(box, {nodes: [island], placements: []}, size);
  assert.deepEqual(coveredBy(c, size, {nodes: [island]}, box), clean());
  // the dragged island lying where it was dropped (`keep`) counts as an island
  c = draw.cartoucheAt(box, {nodes: [], placements: []}, size, [island]);
  assert.ok(!coveredBy(c, size, {nodes: [island]}, box).islands.length);
  // where cartouchePlace's spot would cover the compass rose or the title cartouche: neither is covered
  for (const near of [[45, 140, 60, 190], [1480, 920, 1505, 980]]) {
    const plain = coveredBy(core.cartouchePlace(near, [], size), size, {}, near);
    assert.ok(plain.compass || plain.title, `cartouchePlace alone would cover one (${near})`);
    c = draw.cartoucheAt(near, {nodes: [], placements: []}, size);
    const cov = coveredBy(c, size, {}, near);
    assert.ok(!cov.compass && !cov.title && !cov.land && cov.inside, `near ${near}: ${JSON.stringify(cov)}`);
  }
  // labels everywhere but one free patch far away: it stays beside the land and covers labels (a passing form)
  const lattice = [];
  for (let y = 40; y < 1040; y += 20) for (let x = 40; x < 1880; x += 20) if (!(x > 1500 && y > 100 && y < 400)) lattice.push({kind: "feed", box: [x, y, x + 8, y + 8]});
  c = draw.cartoucheAt(box, {nodes: [], placements: lattice}, size);
  assert.ok(gapTo(c, size, box) < 12, "beside the land, not in the far free patch");
  // among spots at the same distance, the one covering fewer labels: labels thinner below the land
  const thin = lattice.filter(p => !(p.box[1] > 560 && p.box[0] % 40 === 0));
  c = draw.cartoucheAt(box, {nodes: [], placements: thin}, size);
  assert.ok(gapTo(c, size, box) < 12 && c.y > 560, "beside the land, on its lighter side");
  // labels everywhere: the fewest, still no island, rose or title
  const full = [];
  for (let y = 40; y < 1040; y += 20) for (let x = 40; x < 1880; x += 20) full.push({kind: "feed", box: [x, y, x + 8, y + 8]});
  c = draw.cartoucheAt(box, {nodes: [island], placements: full}, size);
  const cov = coveredBy(c, size, {nodes: [island], placements: full}, box);
  assert.ok(cov.labels > 0 && !cov.islands.length && !cov.compass && !cov.title && !cov.land && cov.inside);
});

test("the_survey_grid_is_one_screen_pixel_wide", () => {
  const scene = draw.buildScene(fixture(), "themes", measure);
  for (const px of [1 / (1588 / 1920), 1 / (1110 / 1920)]) {
    const {ctx, log} = recordingContext();
    draw.drawSurveyLayer(ctx, scene, "bleu", null, px);
    assert.equal(ctx.lineWidth, px, "one screen pixel, in chart units");
    assert.ok(ctx.globalAlpha > 0 && ctx.globalAlpha <= .25, "faint");
    assert.ok(log.filter(([op]) => op === "lineTo").length > 20, "the grid's lines");
  }
});

// the page with Survey on and the log open: the chart is 1588 wide (320 for the drawer, 12 clear of it), centred
const K = 1588 / 1920, OY = (1080 - 1080 * K) / 2;
const screen = p => ({clientX: p.x * K, clientY: OY + p.y * K});
const themesScene = () => draw.buildScene(fixture(), "themes", measure);
const nodeOf = id => themesScene().nodes.find(n => n.id === id);
async function surveyPage(options) {
  const opened = await openPage(options);
  toolbarButton(opened.doc, "✒ Survey").dispatch("click", ev());
  const plate = byClass([opened.doc.body], "plate"), base = canvases(opened.doc)[0];
  return {...opened, plate, base, P: {doc: opened.doc, plate, base}};
}
// a press at `from` that moves to `to` and lets go, all in chart units, then the click the browser sends after it
// a press that moves from `from` to `to` and lets go, in chart units, with the click a browser sends after it
// (retargeted to whatever captured the pointer, as browsers do)
function drag(P, from, to, steps = 6) {
  const points = Array.from({length: steps + 1}, (_, i) => ({x: from.x + (to.x - from.x) * i / steps, y: from.y + (to.y - from.y) * i / steps}));
  press(P.doc, P.plate, P.base, points, screen);
}
const stroke = (P, points) => press(P.doc, P.plate, P.base, points, screen);
const tap = (P, at) => press(P.doc, P.plate, P.base, [at, at], screen); // a click: down and up on one spot
// a closed ring of n segments, starting where no island would take the press (a press on an island drags it)
const clearOfIslands = p => !themesScene().nodes.some(n => Math.hypot(n.x - p.x, n.y - p.y) - n.r < 12);
function ring(cx, cy, r, n = 40) {
  const at = a => ({x: cx + r * Math.cos(a), y: cy + r * Math.sin(a)});
  const start = Array.from({length: n}, (_, i) => 2 * Math.PI * i / n).find(a => clearOfIslands(at(a)));
  assert.ok(start !== undefined, "the ring starts in clear water");
  return Array.from({length: n + 1}, (_, i) => at(start + 2 * Math.PI * i / n));
}
// what the overlay drew since its last clear
const lastOverlay = doc => { const log = canvases(doc)[1].recording.log, i = log.map(c => c[0]).lastIndexOf("clearRect"); return log.slice(i + 1); };
// an island drawn in ink on the overlay at (x, y), of radius r
const inkIslandAt = (doc, x, y, r) => lastOverlay(doc).some(([op, ex, ey, rx]) => op === "ellipse" && Math.abs(ex - x) < .01 && Math.abs(ey - y) < .01 && Math.abs(rx - r) < .01);
// segments of ink drawn through the points of `path` (the survey grid's lines are not among them)
const inkLines = (doc, path) => lastOverlay(doc).filter(([op, x, y]) => op === "lineTo" && path.some(p => Math.abs(p.x - x) < .01 && Math.abs(p.y - y) < .01)).length;
const statusLine = doc => byClass([drawerOf(doc)], "log-status").textContent;
const cartoucheOf = doc => byClass([byClass([doc.body], "plate")], "cartouche");
const flushFrames = win => { let t = 0, n = 0; while (win.frames.length && n < 1000) { win.frames.shift()(t += 16); n += 1; } return n; };
// clear water at least 40 units from every island and link, whose nearest sea cell is `key`
function waterIn(key) {
  const scene = themesScene(), data = fixture();
  const seg = (x, y, a, b) => { const dx = b.x - a.x, dy = b.y - a.y, l2 = dx * dx + dy * dy || 1, u = Math.max(0, Math.min(1, ((x - a.x) * dx + (y - a.y) * dy) / l2)); return Math.hypot(x - a.x - u * dx, y - a.y - u * dy); };
  const cell = (x, y) => core.seas(data).reduce((b, s) => (Math.hypot(s.x - x, s.y - y) < Math.hypot(b.x - x, b.y - y) ? s : b));
  for (let y = 100; y < 960; y += 10) for (let x = 100; x < 1820; x += 10) {
    if (scene.nodes.some(n => Math.hypot(n.x - x, n.y - y) < n.r + 40)) continue;
    if (scene.territories.some(t => t.links.some(([a, b]) => seg(x, y, a, b) < 40))) continue;
    if (scene.ground.at(x, y) >= .4 || cell(x, y).key !== key) continue;
    return {x, y};
  }
  throw new Error(`no clear water in ${key}`);
}

test("dragging_an_island_in_the_page", async () => {
  const {win, doc, page, plate, base, P} = await surveyPage();
  const n10 = nodeOf(10), n12 = nodeOf(12), n13 = nodeOf(13);

  // onto a territory's island: join, the land as a LandRef; the island stays at its drop point while pending
  drag(P, n12, n10);
  assert.deepEqual(edits(win).map(m => JSON.parse(m.edit)), [core.edits.join(12, {id: 11, name: "APPLE PRESS"})]);
  assert.equal(byClass([drawerOf(doc)], "log-island").value, "", "a drag is not a click: nothing selected");
  assert.ok(inkIslandAt(doc, n10.x, n10.y, n12.r), "the island stays where it was dropped");
  drag(P, n13, n12);
  assert.equal(edits(win).length, 1, "gestures wait while an edit is pending");
  page.edited(JSON.stringify({seq: 1, ok: true, done: {event: "chart_edit", label: "Join APPLE PRESS", undo: core.edits.patch("Join APPLE PRESS", [{kind: "member"}])}}));
  assert.ok(inkIslandAt(doc, n10.x, n10.y, n12.r), "still there until the new chart");
  page.update(JSON.stringify(resurveyed(1)));
  await settle();
  assert.ok(!inkIslandAt(doc, n10.x, n10.y, n12.r), "the new chart replaces it");

  // onto its own land: nothing is sent (S3)
  drag(P, n10, nodeOf(11));
  assert.equal(edits(win).length, 1, "a drop on its own land is a no-op");

  // onto a lone island: the cartouche asks the name, then found with both ids in the chart's order
  drag(P, n13, n12);
  const box = cartoucheOf(doc);
  assert.ok(box && !box.hidden, "the cartouche opens");
  assert.equal(byClass([box], "name-form-title").textContent, "Name this land");
  assert.equal(buttonIn(box, "Create").textContent, "Create");
  assert.equal(doc.activeElement, inputIn(box), "its field has the focus");
  assert.equal(edits(win).length, 1, "nothing is sent before the name");
  inputIn(box).value = "apple press";
  byClass([box], "name-form").dispatch("submit", ev());
  assert.equal(byClass([box], "name-form-error").textContent, T.landTaken, "the name rules hold");
  inputIn(box).value = "Kite Twins";
  byClass([box], "name-form").dispatch("submit", ev());
  assert.deepEqual(JSON.parse(edits(win).at(-1).edit), core.edits.found("Kite Twins", [12, 13]));
  page.edited(JSON.stringify({seq: 2, ok: true, done: {event: "chart_edit", label: "New Land Kite Twins", undo: core.edits.patch("New Land Kite Twins", [{kind: "land"}])}}));
  assert.ok(cartoucheOf(doc).hidden, "done: the cartouche goes");
  page.update(JSON.stringify(resurveyed(2)));
  await settle();
  assert.ok(!buttonIn(drawerOf(doc), "Undo New Land Kite Twins").disabled, "the log offers its undo");

  // cancelled: nothing sent, the island springs back
  drag(P, n13, n12);
  buttonIn(cartoucheOf(doc), "Cancel").dispatch("click", ev());
  assert.ok(cartoucheOf(doc).hidden);
  flushFrames(win);
  assert.ok(!inkIslandAt(doc, n12.x, n12.y, n13.r) && !inkIslandAt(doc, n13.x, n13.y, n13.r), "the ghost is gone");
  drag(P, n13, n12);
  doc.dispatch("keydown", ev({key: "Escape"}));
  assert.ok(cartoucheOf(doc).hidden, "Esc closes the cartouche too");
  flushFrames(win);
  assert.equal(edits(win).length, 2);

  // a continental island onto a lone one: springs back with the hint in the log's status line
  drag(P, nodeOf(89), n12);
  assert.equal(edits(win).length, 2, "refused by the page: nothing sent");
  assert.equal(statusLine(doc), T.continentHint("KILN COUNTRY"));
  assert.ok(win.frames.length > 0, "with motion, it springs back");
  flushFrames(win);
  assert.ok(!inkIslandAt(doc, n12.x, n12.y, nodeOf(89).r), "home again");

  // open water in its own sea: leave (the land as a LandRef)
  drag(P, n10, waterIn("kites"));
  assert.deepEqual(JSON.parse(edits(win).at(-1).edit), core.edits.leave(10, {id: 11, name: "APPLE PRESS"}));
  page.edited(JSON.stringify({seq: 3, ok: false, refused: true, error: T.changed}));
  assert.equal(statusLine(doc), T.changed, "a refusal shows");
  flushFrames(win);
  assert.ok(!inkIslandAt(doc, waterIn("kites").x, waterIn("kites").y, n10.r), "and the island springs back");

  // "By theme": into another sea's water, set_sea
  drag(P, n12, waterIn("comets"));
  assert.deepEqual(JSON.parse(edits(win).at(-1).edit),
    core.edits.setSea(12, {id: 1, key: "kites", sea: "SEA OF KITES"}, {id: 15, key: "comets", sea: "COMET DEEP"}));
  assert.deepEqual(win.errors, []);
});

test("drawing_a_loop_in_the_page", async () => {
  const {win, doc, page, plate, base, P} = await surveyPage();
  const scene = themesScene();

  // round one island: nothing happens and the line goes
  const small = ring(nodeOf(12).x, nodeOf(12).y, 30);
  stroke(P, small);
  assert.equal(edits(win).length, 0);
  assert.ok(cartoucheOf(doc) === undefined || cartoucheOf(doc).hidden, "no cartouche");
  flushFrames(win);
  assert.equal(inkLines(doc, small), 0, "the ink line has faded");

  // an open stroke is not a loop
  const line = Array.from({length: 30}, (_, i) => ({x: 310 + i * 10, y: 120}));
  assert.ok(clearOfIslands(line[0]));
  stroke(P, line);
  assert.ok(cartoucheOf(doc) === undefined || cartoucheOf(doc).hidden);

  // round two lone islands: Name this land, clear of the labels, then found in the chart's order
  const loop = ring(355, 165, 50);
  assert.deepEqual(core.loopSelection(scene.nodes, loop, fixture()).islands, [12, 13], "the loop holds 12 and 13");
  stroke(P, loop);
  const box = cartoucheOf(doc);
  assert.ok(box && !box.hidden, "the cartouche opens");
  assert.equal(byClass([box], "name-form-title").textContent, "Name this land");
  assert.ok(inkLines(doc, loop) > 10, "the ink line stays while it is named");
  const size = {w: draw.CARTOUCHE_PX.w / K, h: draw.CARTOUCHE_PX.h / K};
  const cx = parseFloat(box.style.left) / K, cy = (parseFloat(box.style.top) - OY) / K;
  const spot = [cx - size.w / 2, cy - size.h / 2, cx + size.w / 2, cy + size.h / 2];
  const loopBox = [Math.min(...loop.map(p => p.x)), Math.min(...loop.map(p => p.y)), Math.max(...loop.map(p => p.x)), Math.max(...loop.map(p => p.y))];
  // this coast is crowded (cartouchePlace's spot meets labels): beside the land all the same, over no island
  assert.ok(coveredBy(core.cartouchePlace(loopBox, scene.placements, size), size, scene, loopBox).labels > 0, "a crowded coast");
  const cov = coveredBy({x: cx, y: cy}, size, scene, loopBox);
  assert.deepEqual([cov.islands, cov.compass, cov.title, cov.land, cov.inside], [[], false, false, false, true], "no island, rose, title or land covered");
  const near = nearestClear(loopBox, scene, size), gapNow = gapTo({x: cx, y: cy}, size, loopBox);
  assert.ok(gapNow <= near + 12 + 1e-6, `never farther than the nearest clear spot (${gapNow} vs ${near})`);
  // the top-left is islands wall to wall: no spot this size clear of them lies nearer than about 425 units
  assert.ok(near > 400 && gapNow < 450, `the nearest clear spot, not the far corner of the chart (${gapNow} units away)`);
  const xs = loop.map(p => p.x), ys = loop.map(p => p.y);
  const want = draw.cartoucheAt([Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)], scene, size);
  assert.ok(Math.abs(cx - want.x) < 1e-6 && Math.abs(cy - want.y) < 1e-6, `at cartoucheAt's spot (${cx}, ${cy}) vs (${want.x}, ${want.y})`);
  inputIn(box).value = "  Twin Kites ";
  const input = inputIn(box);
  byClass([box], "name-form").dispatch("keydown", ev({key: "Enter", target: input}));
  assert.deepEqual(JSON.parse(edits(win).at(-1).edit), core.edits.found("Twin Kites", [12, 13]));
  page.edited(JSON.stringify({seq: 1, ok: false, refused: true, error: T.landTaken}));
  assert.ok(!cartoucheOf(doc).hidden, "a name refusal keeps it open");
  assert.equal(byClass([cartoucheOf(doc)], "name-form-error").textContent, T.landTaken);
  byClass([cartoucheOf(doc)], "name-form").dispatch("submit", ev());
  page.edited(JSON.stringify({seq: 2, ok: true, done: {event: "chart_edit", label: "New Land Twin Kites", undo: core.edits.patch("New Land Twin Kites", [{kind: "land"}])}}));
  assert.ok(cartoucheOf(doc).hidden, "named: the cartouche goes");
  assert.ok(inkLines(doc, loop) > 10, "the ink stays until the new chart");
  page.update(JSON.stringify(resurveyed(1)));
  await settle();
  assert.equal(inkLines(doc, loop), 0, "the invented coastline replaces it");

  // round every island of a sea that has no continent: Make a continent, and Enter makes it
  const quills = ring(240, 835, 110);
  assert.equal(core.loopSelection(scene.nodes, quills, fixture()).continentSea, "quills");
  stroke(P, quills);
  const c = cartoucheOf(doc);
  assert.ok(!c.hidden);
  assert.equal(byClass([c], "name-form-title").textContent, "Make a continent");
  byClass([c], "name-form").dispatch("keydown", ev({key: "Enter", target: inputIn(c)}));
  assert.deepEqual(JSON.parse(edits(win).at(-1).edit), core.edits.makeContinent({id: 13, key: "quills", sea: "QUILL SEA"}, "QUILL SEA"));
  page.edited(JSON.stringify({seq: 3, ok: false, refused: true, error: T.changed}));
  assert.ok(cartoucheOf(doc).hidden, "any other refusal closes it");
  assert.equal(statusLine(doc), T.changed);
  assert.ok(inkLines(doc, quills) > 10, "the line was there");
  flushFrames(win);
  assert.equal(inkLines(doc, quills), 0, "and the line fades");

  // cancelled: the line goes; while it is open, a name's click opens no rename beside it
  stroke(P, loop);
  const tidal = scene.placements.find(p => p.kind === "sea" && p.key === "tides");
  plate.dispatch("click", ev({target: base, ...screen(tidal)}));
  assert.ok(!byClass([plate], "rename") || byClass([plate], "rename").hidden, "no rename while the cartouche is open");
  assert.equal(page.undo(), false, "and no undo while its name is typed");
  buttonIn(cartoucheOf(doc), "Cancel").dispatch("click", ev());
  flushFrames(win);
  assert.equal(inkLines(doc, loop), 0);
  assert.equal(edits(win).length, 3);
  assert.deepEqual(win.errors, []);
});

test("a_refused_drop_snaps_back_under_reduce_motion", async () => {
  for (const options of [{prefs: {reduceMotion: true}}, {reduced: true}]) {
    const {win, doc, plate, base, P} = await surveyPage(options);
    const n89 = nodeOf(89), n12 = nodeOf(12);
    const before = win.frames.length;
    drag(P, n89, n12);
    assert.equal(statusLine(doc), T.continentHint("KILN COUNTRY"));
    assert.equal(win.frames.length, before, "no animation frame");
    assert.ok(!inkIslandAt(doc, n12.x, n12.y, n89.r), "already home");
    const small = ring(nodeOf(12).x, nodeOf(12).y, 30);
    stroke(P, small);
    assert.equal(win.frames.length, before, "no ink fade either");
    assert.equal(inkLines(doc, small), 0, "the line simply disappears");
    assert.deepEqual(win.errors, []);
  }
});

test("islands_win_over_label_boxes_in_survey", async () => {
  const {win, doc, plate, base, P} = await surveyPage();
  const scene = themesScene(), kiln = scene.placements.find(p => p.kind === "continent" && p.text === "KILN COUNTRY");
  const n89 = nodeOf(89);
  assert.ok(n89.x >= kiln.box[0] && n89.x <= kiln.box[2] && n89.y >= kiln.box[1] && n89.y <= kiln.box[3], "island 89 sits in KILN COUNTRY's label");
  plate.dispatch("click", ev({target: base, ...screen(n89)}));
  assert.equal(byClass([drawerOf(doc)], "log-island").value, "89", "a click on it selects the island");
  assert.ok(!byClass([plate], "rename") || byClass([plate], "rename").hidden, "and opens no rename");
  drag(P, n89, nodeOf(12));
  assert.equal(statusLine(doc), T.continentHint("KILN COUNTRY"), "a press on it drags the island");
  assert.deepEqual(win.errors, []);
});

test("renaming_a_territory_or_a_continent_over_its_label", async () => {
  const {win, doc, page, plate, base, P} = await surveyPage();
  const scene = themesScene();
  // a point of the label's box clear of every island's reach
  const clear = p => {
    for (let y = p.box[1] + 1; y < p.box[3]; y += 2) for (let x = p.box[0] + 1; x < p.box[2]; x += 2) {
      if (!scene.nodes.some(n => Math.hypot(n.x - x, n.y - y) - n.r < 12)) return {x, y};
    }
    throw new Error(`no clear point on ${p.text}`);
  };
  for (const [kind, text, ref] of [["territory", "GULL PRESS GROUP", {id: 1, name: "GULL PRESS GROUP"}], ["continent", "KILN COUNTRY", {id: 15, name: "KILN COUNTRY"}]]) {
    const p = scene.placements.find(q => q.kind === kind && q.text === text);
    plate.dispatch("click", ev({target: base, ...screen(clear(p))}));
    const box = byClass([plate], "rename");
    assert.ok(box && !box.hidden, `${kind}: the rename opens`);
    assert.equal(inputIn(box).value, text);
    inputIn(box).value = `${text} North`;
    byClass([box], "name-form").dispatch("submit", ev());
    assert.deepEqual(JSON.parse(edits(win).at(-1).edit), core.edits.renameLand(ref, `${text} North`), `${kind}: renamed by id`);
    // answered, not run: the rename closes and the next edit can go
    page.edited(JSON.stringify({seq: edits(win).length, ok: false, refused: false, error: "x"}));
    assert.ok(byClass([plate], "rename").hidden, `${kind}: closed`);
  }
  assert.deepEqual(win.errors, []);
});

// fix round 1: a real click in Survey mode (pointerdown, pointerup, click) still reaches the chart, and the click
// swallowed after a gesture never outlives that gesture
test("plain_clicks_in_survey_select_and_rename", async () => {
  const {win, doc, page, plate, base, P} = await surveyPage();
  tap(P, nodeOf(12));
  assert.ok(doc.capturedThisPress === null, "a click captures nothing");
  assert.equal(byClass([drawerOf(doc)], "log-island").value, "12", "a click on an island selects it");
  const tidal = themesScene().placements.find(p => p.kind === "sea" && p.key === "tides");
  tap(P, tidal);
  const box = byClass([plate], "rename");
  assert.ok(box && !box.hidden, "a click on a sea's name opens its rename");
  assert.equal(inputIn(box).value, "TIDAL SEA");
  doc.dispatch("keydown", ev({key: "Escape"}));
  assert.ok(box.hidden, "Esc cancels it");

  // a drag captures the pointer and releases it at the end
  drag(P, nodeOf(10), nodeOf(11));
  assert.ok(doc.capturedThisPress === plate, "a drag captures the pointer");
  assert.ok(doc.capture === null, "and lets it go");
  assert.equal(byClass([drawerOf(doc)], "log-island").value, "12", "the click after the drag selects nothing");
  tap(P, nodeOf(13));
  assert.equal(byClass([drawerOf(doc)], "log-island").value, "13", "the next click is a click again");

  // a gesture whose click never comes (released off the chart) leaves nothing behind: not past the end of its
  // input task, and not past Survey mode (the clicks below come with no press, as a click without one would)
  const lostClick = () => {
    plate.dispatch("pointerdown", ev({target: base, button: 0, pointerId: 1, ...screen(nodeOf(10))}));
    plate.dispatch("pointermove", ev({target: base, pointerId: 1, ...screen({x: nodeOf(10).x + 40, y: nodeOf(10).y})}));
    plate.dispatch("pointerup", ev({target: plate, button: 0, pointerId: 1, ...screen({x: nodeOf(10).x + 40, y: nodeOf(10).y})}));
    win.frames.splice(0); // the spring back is not the point here
  };
  const endOfTask = () => win.timers.splice(0).forEach(t => { if (t.ms === 0) t.fn(); else win.timers.push(t); });
  lostClick();
  endOfTask();
  plate.dispatch("click", ev({target: base, ...screen(nodeOf(14))}));
  assert.equal(byClass([drawerOf(doc)], "log-island").value, "14", "after its input task, a click is a click");
  lostClick();
  toolbarButton(doc, "✒ Survey").dispatch("click", ev());
  plate.dispatch("click", ev({target: base, clientX: nodeOf(12).x * 1, clientY: nodeOf(12).y * 1}));
  const pop = byClass([plate], "pop");
  assert.ok(pop && !pop.hidden, "Survey off: the first click opens the popover");
  assert.deepEqual(win.errors, []);
  assert.equal(page.undo(), false);
});

test("a_log_rebuilt_mid_drag_leaves_esc_to_the_page", async () => {
  const {win, doc, page, plate, P} = await surveyPage();
  const keydowns = () => (doc.listeners.keydown || []).length, before = keydowns();
  const handle = byClass([drawerOf(doc)], "log-handle");
  assert.ok(handle, "a sea's handle in the log");
  handle.dispatch("pointerdown", ev({button: 0, pointerId: 1, clientX: 0, clientY: 0}));
  assert.equal(keydowns(), before + 1, "the drag listens for Esc");
  page.update(JSON.stringify(resurveyed())); // a survey lands mid-drag: the log is rebuilt
  await settle();
  assert.notEqual(byClass([drawerOf(doc)], "log-handle"), handle, "the log was rebuilt");
  const tidal = themesScene().placements.find(p => p.kind === "sea" && p.key === "tides");
  tap(P, tidal);
  assert.ok(!byClass([plate], "rename").hidden, "a rename open over the chart");
  doc.dispatch("keydown", ev({key: "Escape"}));
  assert.ok(byClass([plate], "rename").hidden, "the Esc reached the page: the rename closed");
  assert.equal(keydowns(), before, "no listener left behind");
  assert.deepEqual(win.errors, []);
});

test("one_name_at_a_time_and_a_pending_name_waits_for_its_reply", async () => {
  const {win, doc, page, plate, base, P} = await surveyPage();
  const n10 = nodeOf(10), n12 = nodeOf(12), n13 = nodeOf(13);
  // a rename open: no drag and no loop
  const tidal = themesScene().placements.find(p => p.kind === "sea" && p.key === "tides");
  tap(P, tidal);
  assert.ok(!byClass([plate], "rename").hidden);
  drag(P, n12, n10);
  stroke(P, ring(355, 165, 50));
  assert.equal(edits(win).length, 0, "nothing while a rename is open");
  assert.ok(!cartoucheOf(doc) || cartoucheOf(doc).hidden, "no cartouche beside it");
  doc.dispatch("keydown", ev({key: "Escape"}));

  // the cartouche open: no other drag or loop
  drag(P, n13, n12);
  assert.ok(!cartoucheOf(doc).hidden);
  drag(P, n10, nodeOf(14));
  stroke(P, ring(240, 835, 110));
  assert.equal(edits(win).length, 0, "nothing while the cartouche asks its name");
  assert.equal(byClass([cartoucheOf(doc)], "name-form-title").textContent, "Name this land", "the same cartouche");

  // named and sent: Cancel and Esc wait for the reply, the island stays where it was dropped
  inputIn(cartoucheOf(doc)).value = "Kite Twins";
  byClass([cartoucheOf(doc)], "name-form").dispatch("submit", ev());
  assert.equal(edits(win).length, 1);
  const cancel = buttonIn(cartoucheOf(doc), "Cancel");
  assert.equal(cancel.disabled, true, "Cancel is off while its edit is on its way");
  cancel.dispatch("click", ev());
  doc.dispatch("keydown", ev({key: "Escape"}));
  assert.ok(!cartoucheOf(doc).hidden, "still open");
  flushFrames(win);
  assert.ok(inkIslandAt(doc, n12.x, n12.y, n13.r), "the island has not sprung back");
  page.edited(JSON.stringify({seq: 1, ok: true, done: {event: "chart_edit", label: "New Land Kite Twins", undo: core.edits.patch("New Land Kite Twins", [{kind: "land"}])}}));
  assert.ok(cartoucheOf(doc).hidden, "the reply closes it");
  assert.ok(inkIslandAt(doc, n12.x, n12.y, n13.r), "and the island waits for the new chart");
  assert.deepEqual(win.errors, []);
});

test("a_drop_hint_shows_with_the_log_closed", async () => {
  const {win, doc, plate, P} = await surveyPage();
  toolbarButton(doc, "Log").dispatch("click", ev()); // the chart is now 1920 wide: positions at that scale
  const wide = p => ({x: p.x, y: p.y});
  const at1 = p => ({clientX: p.x, clientY: p.y});
  press(doc, plate, P.base, [nodeOf(89), {x: (nodeOf(89).x + nodeOf(12).x) / 2, y: (nodeOf(89).y + nodeOf(12).y) / 2}, nodeOf(12)].map(wide), at1);
  const status = byClass([plate], "chart-status");
  assert.ok(status && !status.hidden, "the status line shows over the chart");
  assert.equal(status.textContent, T.continentHint("KILN COUNTRY"));
  assert.equal(status.getAttribute("role"), "status");
  toolbarButton(doc, "Log").dispatch("click", ev());
  assert.ok(status.hidden, "the log shows it when open");
  assert.equal(statusLine(doc), T.continentHint("KILN COUNTRY"));
  toolbarButton(doc, "Log").dispatch("click", ev());
  // the next gesture clears it
  const small = ring(nodeOf(12).x, nodeOf(12).y, 30).map(wide);
  press(doc, plate, P.base, small, at1);
  assert.ok(status.hidden, "a new gesture: the old hint goes");
  assert.deepEqual(win.errors, []);
});

// Recette fix 2: Qt WebEngine shows a cursor's bitmap one pixel per logical pixel, so the KDE app's quill came out twice
// the Mac's on a 2× screen. The page gives the quill at 32 / density CSS px there (the KDE adapter marks the root).
test("quill_cursor_at_the_screen_density_for_qt_webengine", async () => {
  const QUILL = "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='32' height='32' viewBox='0 0 32 32'%3E%3Cg stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M2 30 L9 19 C14 12 21 6 30 2 C27 11 21 18 13 23 Z' fill='%23f7f8f6' stroke='%23f7f8f6' stroke-width='4'/%3E%3Cpath d='M2 30 L9 19 C14 12 21 6 30 2 C27 11 21 18 13 23 Z' fill='%23f7f8f6' stroke='%231f3c88' stroke-width='1.4'/%3E%3Cpath d='M2 30 L17 14 M9 19 L13 23' fill='none' stroke='%231f3c88' stroke-width='1.2'/%3E%3C/g%3E%3C/svg%3E\") 2 30, crosshair";
  assert.equal(draw.quillCursor(1), QUILL, "the Mac's quill, as chart.css drew it before: 32 px, the nib at 2 30");
  assert.equal(draw.quillCursor(), QUILL);
  assert.equal(draw.quillCursor(2), QUILL.replace("width='32' height='32'", "width='16' height='16'").replace(" 2 30,", " 1 15,"),
    "on a 2× screen, 16 CSS px: Chromium rasterises 32 device px, which Qt WebEngine shows at 32 logical px");
  assert.match(draw.quillCursor(1.5), /width='21' height='21'.*\) 1 20, crosshair$/, "a fractional scale too");
  assert.equal(draw.quillCursor(0), QUILL, "no density: the plain quill");

  const quillOf = async (qt, dpr) => {
    const win = fakeWindow();
    win.devicePixelRatio = dpr;
    if (qt) win.document.documentElement.classList.add("qt-cursors");
    draw.createPage(win).load(JSON.stringify(fixture()), {view: "themes", ink: "bleu", animate: false});
    await settle();
    return win.document.documentElement.style["--quill"];
  };
  assert.equal(await quillOf(false, 2), QUILL, "WebKit scales a cursor itself: the 32 px quill at any density");
  assert.equal(await quillOf(true, 1), QUILL, "Qt WebEngine at 1×");
  assert.equal(await quillOf(true, 2), draw.quillCursor(2), "Qt WebEngine at 2×");
});

// Recette fix 4 (Clément's wish): the island an edit moved gets a ring in the current ink on the next chart, which pulses
// twice (still for 2 s under Reduce Motion), on the overlay; the log selects it.
// the ink ring's ellipses on the overlay round island `n` (the selection's dashed ring is n.r + 8: the ink ring is wider)
const inkRings = (doc, n) => lastOverlay(doc).filter(([op, x, y, rx]) => op === "ellipse" && Math.abs(x - n.x) < .01 && Math.abs(y - n.y) < .01 && rx >= n.r + 10 - 1e-9);
const islandSelect = doc => byClass([drawerOf(doc)], "log-island");
function chooseIsland(doc, id) {
  const s = islandSelect(doc);
  s.value = String(id);
  s.dispatch("change", ev());
}
test("the_moved_island_gets_an_ink_ring_after_a_leave", async () => {
  const {win, doc, page, P} = await surveyPage();
  const n10 = nodeOf(10);
  chooseIsland(doc, 10);
  buttonIn(drawerOf(doc), "Leave Land").dispatch("click", ev());
  assert.deepEqual(JSON.parse(edits(win).at(-1).edit), core.edits.leave(10, {id: 11, name: "APPLE PRESS"}));
  tap(P, nodeOf(13)); // the user looks elsewhere while the edit goes
  assert.equal(islandSelect(doc).value, "13");
  page.edited(JSON.stringify({seq: 1, ok: true, done: {event: "chart_edit", label: "Leave APPLE PRESS",
    undo: core.edits.patch("Leave APPLE PRESS", [{kind: "member", feed: 10, land: 11, before: null, after: 1}])}}));
  assert.equal(inkRings(doc, n10).length, 0, "no ring before the new chart shows where the island went");
  const framesBefore = win.frames.length;
  page.update(JSON.stringify(resurveyed(1)));
  await settle();
  assert.equal(islandSelect(doc).value, "10", "the log selects the island the edit moved");
  assert.ok(win.frames.length > framesBefore, "the ring animates");
  // two pulses: the ring grows and fades, starts again once, then is gone
  const radii = [];
  let n = 0;
  while (win.frames.length && n < 1000) {
    win.frames.shift()(16 * ++n);
    const r = inkRings(doc, n10);
    if (r.length) radii.push(r[0][3]);
  }
  assert.ok(radii.length > 10, "drawn on many frames");
  const restarts = radii.filter((r, i) => i > 0 && r < radii[i - 1]).length;
  assert.equal(restarts, 1, "it pulses twice");
  assert.equal(inkRings(doc, n10).length, 0, "then it is gone");
  assert.equal(win.frames.length, 0, "and asks for no more frames");
  const strokes = canvases(doc)[1].recording.log.filter(([op]) => op === "stroke").length;
  assert.ok(strokes > 0);
  assert.deepEqual(win.errors, []);
});

test("the_moved_island_rings_still_for_two_seconds_under_reduce_motion_after_a_set_sea", async () => {
  const {win, doc, page, P} = await surveyPage({reduced: true});
  const n12 = nodeOf(12);
  chooseIsland(doc, 12);
  const sea = byClass([drawerOf(doc)], "log-sea-select");
  sea.value = "2"; // TIDAL SEA
  sea.dispatch("change", ev());
  assert.equal(JSON.parse(edits(win).at(-1).edit).op, "set_sea");
  tap(P, nodeOf(13));
  assert.equal(islandSelect(doc).value, "13");
  page.edited(JSON.stringify({seq: 1, ok: true, done: {event: "chart_edit", label: "Move to TIDAL SEA",
    undo: core.edits.patch("Move to TIDAL SEA", [{kind: "feed_seas", feeds: [12], before: 2, after: 1}])}}));
  page.update(JSON.stringify(resurveyed(1)));
  await settle();
  assert.equal(win.frames.length, 0, "Reduce Motion: no animation frame");
  assert.equal(islandSelect(doc).value, "12", "the log selects it");
  const still = inkRings(doc, n12);
  assert.equal(still.length, 1, "one still ring");
  const timer = win.timers.filter(t => t.ms === 2000);
  assert.equal(timer.length, 1, "it stays 2 s");
  timer[0].fn();
  assert.equal(inkRings(doc, n12).length, 0, "then it is gone");
  assert.deepEqual(win.errors, []);
});

test("only_an_edit_that_moved_one_island_rings_it", () => {
  assert.equal(core.movedFeed(core.edits.leave(10, {id: 11, name: "APPLE PRESS"})), 10);
  assert.equal(core.movedFeed(core.edits.join(12, {id: 11, name: "APPLE PRESS"})), 12);
  assert.equal(core.movedFeed(core.edits.setSea(12, null, {id: 2, key: "tides", sea: "TIDAL SEA"})), 12);
  // an undo or redo that puts one island back
  assert.equal(core.movedFeed(core.edits.patch("Leave APPLE PRESS", [{kind: "member", feed: 10, land: 11, before: null, after: 1}])), 10);
  assert.equal(core.movedFeed(core.edits.patch("Move to TIDAL SEA", [{kind: "feed_seas", feeds: [12], before: 2, after: 1}])), 12);
  // no island, or several
  assert.equal(core.movedFeed(core.edits.newSea("SEA OF TESTS")), null);
  assert.equal(core.movedFeed(core.edits.found("KITE TWINS", [12, 13])), null);
  assert.equal(core.movedFeed(core.edits.patch("Delete APPLE PRESS", [{kind: "land"}, {kind: "member", feed: 10}, {kind: "member", feed: 11}])), null);
  assert.equal(core.movedFeed(null), null);
  assert.equal(core.movedFeed({op: "leave", feed: "10"}), null);
});

// Fix round 1 of the recette fixes (review I-1): the ring belongs to Survey mode and to the chart it was drawn on. Turning
// Survey off, a new chart, a view or ink switch, or a new ring stops it at once: no ring left on the overlay, no frame asked.
async function ringedPage(options) {
  const opened = await surveyPage(options);
  const {win, doc, page} = opened;
  chooseIsland(doc, 10);
  buttonIn(drawerOf(doc), "Leave Land").dispatch("click", ev());
  page.edited(JSON.stringify({seq: 1, ok: true, done: {event: "chart_edit", label: "Leave APPLE PRESS",
    undo: core.edits.patch("Leave APPLE PRESS", [{kind: "member", feed: 10, land: 11, before: null, after: 1}])}}));
  page.update(JSON.stringify(resurveyed(1)));
  await settle();
  if (win.frames.length) win.frames.shift()(16); // one frame of the pulse
  assert.equal(inkRings(doc, nodeOf(10)).length, 1, "positive control: the ring shows");
  return opened;
}
const runFrames = win => { let n = 0; while (win.frames.length && n < 1000) { win.frames.shift()(16 * ++n + 16); } return n; };
test("turning_survey_off_stops_the_ink_ring", async () => {
  for (const reduced of [false, true]) {
    const {win, doc} = await ringedPage({reduced});
    toolbarButton(doc, "✒ Survey").dispatch("click", ev());
    assert.equal(inkRings(doc, nodeOf(10)).length, 0, `Survey off: the ring is gone at once (reduced ${reduced})`);
    assert.ok(runFrames(win) <= 1, "no frame asked for the ring");
    assert.equal(inkRings(doc, nodeOf(10)).length, 0, "and it does not come back");
    if (reduced) assert.ok(win.cleared.includes(win.timers.findIndex(t => t.ms === 2000) + 1), "its 2 s timer is cancelled");
    assert.deepEqual(win.errors, []);
  }
});
test("a_view_or_ink_switch_or_a_new_chart_stops_the_ink_ring", async () => {
  for (const act of ["ink", "view", "load"]) {
    const {win, doc, page} = await ringedPage();
    if (act === "ink") toolbarButton(doc, "White on black").dispatch("click", ev());
    if (act === "view") toolbarButton(doc, "From the harbour").dispatch("click", ev());
    if (act === "load") { page.load(JSON.stringify(fixture()), {view: "themes", ink: "bleu", animate: false}); await settle(); }
    const island = act === "view" ? draw.buildScene(fixture(), "harbour", measure).nodes.find(n => n.id === 10) : nodeOf(10);
    assert.equal(inkRings(doc, island).length, 0, `${act}: the ring is gone at once`);
    assert.ok(runFrames(win) <= 1, `${act}: no frame asked for the ring`);
    assert.equal(inkRings(doc, island).length, 0, `${act}: and it does not come back`);
    assert.deepEqual(win.errors, []);
  }
});
