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
const measure = (text, font) => [...text].length * parseFloat(/(\d+(?:\.\d+)?)px/.exec(font)[1]) * 0.55;
const EMPTY = "No feed has published in six months";

// ---------- a small DOM: enough for the builders, and it refuses innerHTML outright ----------
function fakeDocument() {
  const text = data => ({nodeType: 3, data: String(data)});
  const element = tag => {
    const el = {
      nodeType: 1, tagName: tag.toUpperCase(), childNodes: [], attributes: {}, listeners: {}, className: "", style: {},
      append(...nodes) { nodes.forEach(n => el.childNodes.push(typeof n === "string" ? text(n) : n)); },
      setAttribute(k, v) { el.attributes[k] = String(v); },
      getAttribute(k) { return el.attributes[k] ?? null; },
      addEventListener(type, fn) { (el.listeners[type] ||= []).push(fn); },
      dispatch(type, event) { (el.listeners[type] || []).forEach(fn => fn(event)); },
      set textContent(v) { el.childNodes = [text(v)]; },
      get textContent() { return el.childNodes.map(n => (n.nodeType === 3 ? n.data : n.textContent)).join(""); },
      set innerHTML(_) { throw new Error("innerHTML must never be used"); },
      set outerHTML(_) { throw new Error("outerHTML must never be used"); },
      insertAdjacentHTML() { throw new Error("insertAdjacentHTML must never be used"); },
    };
    return el;
  };
  return {createElement: element, createTextNode: text};
}
const walk = (nodes, out = []) => { nodes.forEach(n => { out.push(n); if (n.childNodes) walk(n.childNodes, out); }); return out; };
const elements = nodes => walk(nodes).filter(n => n.nodeType === 1);
const byClass = (nodes, cls) => elements(nodes).find(n => n.className.split(" ").includes(cls));

// ---------- a canvas context that records what is drawn ----------
function recordingContext() {
  const log = [], state = {};
  const ctx = new Proxy(state, {
    get(target, prop) {
      if (prop in target) return target[prop];
      return (...args) => {
        log.push([prop, ...args]);
        if (prop === "measureText") return {width: measure(String(args[0]), target.font || "10px serif")};
        if (prop === "createRadialGradient" || prop === "createLinearGradient") return {addColorStop() {}};
        return undefined;
      };
    },
    set(target, prop, value) { target[prop] = value; return true; },
  });
  return {ctx, log, texts: () => log.filter(([op]) => op === "fillText").map(([, t]) => t)};
}

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
  assert.deepEqual(name.childNodes, [{nodeType: 3, data: '<b>Bold</b> & "Quoted" Notes'}]);
  assert.ok(!elements(tip).some(e => e !== name && e.tagName === "B"), "the title's <b> must stay text");

  const pop = draw.popoverContent(doc, node, data, {});
  const all = walk(pop);
  assert.ok(all.some(n => n.nodeType === 3 && n.data === '<b>Bold</b> & "Quoted" Notes'), "title as one text node");
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
  assert.deepEqual(draw.normalisePrefs(undefined), {view: "harbour", ink: "bleu", animate: true});
  assert.deepEqual(draw.normalisePrefs({view: "sideways", ink: "rouge", animate: "yes"}), {view: "harbour", ink: "bleu", animate: true});
  assert.deepEqual(draw.normalisePrefs({view: "toString", ink: "__proto__"}), {view: "harbour", ink: "bleu", animate: true});
  for (const view of ["harbour", "themes"]) for (const ink of ["bleu", "nuit", "pop", "pirate"]) for (const animate of [true, false]) {
    const prefs = {view, ink, animate}, message = draw.prefsMessage(prefs);
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
  for (const view of ["harbour", "themes"]) for (const ink of ["bleu", "pirate"]) {
    const scene = draw.buildScene(fixture(), view, measure), {ctx, texts} = recordingContext();
    draw.drawChart(ctx, scene, ink);
    const drawn = texts().join("\u0000");
    scene.placements.forEach(p => {
      if (p.track) [...p.text].forEach(c => assert.ok(texts().includes(c), `${view}: ${p.kind} ${p.text}`));
      else assert.ok(drawn.includes(p.text), `${view}: ${p.kind} ${p.text}`);
    });
    assert.ok(texts().includes("Surveyed Monday 19 October 2026, 17:40"), "the cartouche carries the survey time");
    if (ink === "pirate") assert.ok(texts().some(t => /^X marks \d+ unread$/.test(t)), "the pirates' X");
  }
});
