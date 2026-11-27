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

const {measure, fakeDocument, fakeWindow, walk, elements, byClass, recordingContext} = require("./fake-dom.cjs");

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
  assert.equal(canvases(doc)[0].width, 1920 - 320, "the chart keeps clear of the drawer");

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
  // the drawer takes 320 px: the chart is 1600 wide, 900 high, 90 px down
  const click = (p, k, ox, oy) => plate.dispatch("click", ev({target: base, clientX: ox + p.x * k, clientY: oy + p.y * k}));
  const renameBox = () => byClass([plate], "rename");

  click(sea, 1, 0, 0);
  assert.ok(!renameBox() || renameBox().hidden, "outside Survey mode a name is not renamed");

  toolbarButton(doc, "✒ Survey").dispatch("click", ev());
  const k = 1600 / 1920, oy = (1080 - 1080 * k) / 2;
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
