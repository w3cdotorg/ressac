// Tests of the chart's pure core (layout, coasts, labels, heat), run with `node --test 'apps/macos/ChartTests/*.test.cjs'`.
"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");
const fs = require("node:fs");
const vm = require("node:vm");

const CHART = path.join(__dirname, "..", "Ressac", "Resources", "Chart");
const d3 = require(path.join(CHART, "d3.min.js"));
const core = require(path.join(CHART, "chart-core.js"));
core.useD3(d3);

const fixture = () => JSON.parse(JSON.stringify(require("./fixture.json")));
const VIEWS = ["harbour", "themes"];
const W = 1920, H = 1080;
// the mockup's reserved boxes: the cartouche (bottom right) and the compass rose (top left)
const CARTOUCHE = [W - 400, H - 180, W - 50, H - 52];
const COMPASS = [78, 83, 242, 247];

// a fixed-width estimate stands in for the canvas: 0.55 em per character
function measure(text, font) {
  const px = parseFloat(/(\d+(?:\.\d+)?)px/.exec(font)[1]);
  return [...text].length * px * 0.55;
}
const meet = (a, b) => a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1];
const byId = nodes => new Map(nodes.map(n => [n.id, n]));
const newFeed = (id, title, theme, group) => ({
  id, title, theme, folder: "Blogs", group, age_days: 2.5, unread_week: 1, entries_week: 2, activity: 0.8, page: null, entries: [],
});

test("deterministic_for_same_input", () => {
  for (const view of VIEWS) {
    const a = core.place(fixture(), view), b = core.place(fixture(), view);
    assert.deepEqual(a.map(n => [n.id, n.x, n.y, n.r]), b.map(n => [n.id, n.x, n.y, n.r]), view);
    const fa = core.field(a, view, fixture()), fb = core.field(b, view, fixture());
    assert.deepEqual(fa.grid, fb.grid, view);
    assert.deepEqual(core.labels(a, fixture(), view, measure), core.labels(b, fixture(), view, measure), view);
  }
});

test("island_radius_by_rhythm", () => {
  const radius = {daily: 36, weekly: 27, monthly: 21, rare: 16, silent: 11};
  const data = fixture();
  for (const view of VIEWS) {
    const scale = view === "harbour" ? 0.7 : 1;
    core.place(data, view).forEach(n => assert.equal(n.r, radius[n.group] * scale, `${view} ${n.group}`));
  }
});

test("islands_never_overlap", () => {
  for (const view of VIEWS) {
    const nodes = core.place(fixture(), view);
    assert.equal(nodes.length, 100);
    for (let i = 0; i < nodes.length; i++) for (let j = i + 1; j < nodes.length; j++) {
      const a = nodes[i], b = nodes[j], d = Math.hypot(a.x - b.x, a.y - b.y);
      assert.ok(d >= 0.8 * (a.r + b.r), `${view}: ${a.title} and ${b.title} are ${d.toFixed(1)} apart`);
    }
  }
});

test("crowded_seas_still_never_overlap", () => {
  // two and a half times today's feeds, mostly daily (the largest islands): water runs out at full clearance
  const data = fixture(), keys = data.themes.map(t => t.key).concat([null]);
  data.feeds = Array.from({length: 250}, (_, i) =>
    newFeed(i + 1, `Crowd Feed ${i + 1}`, keys[i % 7 === 0 ? 0 : i % keys.length], i % 5 === 4 ? "weekly" : "daily"));
  data.territories = []; data.continents = [];
  const nodes = core.place(data, "themes");
  let worst = Infinity;
  for (let i = 0; i < nodes.length; i++) for (let j = i + 1; j < nodes.length; j++) {
    const a = nodes[i], b = nodes[j];
    worst = Math.min(worst, Math.hypot(a.x - b.x, a.y - b.y) / (a.r + b.r));
  }
  assert.ok(worst >= 0.8, `two islands are only ${worst.toFixed(2)} × (r1 + r2) apart`);
});

test("islands_keep_clear_of_the_compass_and_cartouche", () => {
  const gap = (n, b) => Math.hypot(Math.max(b[0] - n.x, 0, n.x - b[2]), Math.max(b[1] - n.y, 0, n.y - b[3]));
  for (const view of VIEWS) {
    core.place(fixture(), view).forEach(n => {
      assert.ok(Math.hypot(n.x - 160, n.y - 165) > 82 + n.r, `${view}: ${n.title} lies on the compass rose`); // ring 70, N at 82
      assert.ok(gap(n, CARTOUCHE) > n.r, `${view}: ${n.title} lies on the cartouche`);
    });
  }
});

test("adding_a_feed_barely_moves_others_by_theme", () => {
  const withoutUnthemed = () => { const d = fixture(); d.feeds = d.feeds.filter(f => f.theme !== null); return d; };
  const cases = [
    ["a daily feed in the largest sea", fixture, newFeed(1001, "The Newcomer Kite", "kites", "daily")],
    ["a weekly feed in a one-feed sea", fixture, newFeed(1002, "A Second Comet", "comets", "weekly")],
    ["an unthemed feed", fixture, newFeed(1003, "Fresh From An OPML", null, "silent")],
    ["the first unthemed feed", withoutUnthemed, newFeed(1004, "First Stranger", null, "monthly")],
  ];
  for (const [what, make, feed] of cases) {
    const before = byId(core.place(make(), "themes"));
    const data = make(); data.feeds.push(feed);
    const after = core.place(data, "themes");
    let worst = 0, who = null;
    after.forEach(n => {
      if (n.id === feed.id) return;
      const m = before.get(n.id), d = Math.hypot(n.x - m.x, n.y - m.y);
      if (d > worst) { worst = d; who = n.title; }
    });
    assert.ok(worst <= 40, `${what}: ${who} moved ${worst.toFixed(1)} units`);
  }
});

test("labels_never_overlap", () => {
  for (const view of VIEWS) {
    const data = fixture(), nodes = core.place(data, view), placements = core.labels(nodes, data, view, measure);
    const kinds = new Set(placements.map(p => p.kind));
    for (const kind of ["sea", "continent", "territory", "feed", "sounding"]) assert.ok(kinds.has(kind), `${view}: no ${kind} label`);
    if (view === "harbour") for (const kind of ["harbour", "ring"]) assert.ok(kinds.has(kind), `harbour: no ${kind} label`);
    for (let i = 0; i < placements.length; i++) {
      const p = placements[i], b = p.box;
      assert.ok(b[0] >= 40 && b[1] >= 40 && b[2] <= W - 40 && b[3] <= H - 40, `${view}: "${p.text}" leaves the chart`);
      assert.ok(!meet(b, CARTOUCHE), `${view}: "${p.text}" over the cartouche`);
      assert.ok(!meet(b, COMPASS), `${view}: "${p.text}" over the compass`);
      for (let j = i + 1; j < placements.length; j++) {
        assert.ok(!meet(b, placements[j].box), `${view}: "${p.text}" (${p.kind}) overlaps "${placements[j].text}" (${placements[j].kind})`);
      }
    }
  }
});

test("feed_names_are_placed_busiest_first", () => {
  // busiest by activity, then unread this week, then rhythm (spec §2): placements come out in that order
  const rank = {daily: 0, weekly: 1, monthly: 2, rare: 3, silent: 4};
  const before = (a, b) => (b.activity - a.activity) || (b.unread - a.unread) || (rank[a.group] - rank[b.group]);
  for (const view of VIEWS) {
    const data = fixture(), nodes = core.place(data, view), node = byId(nodes);
    const named = core.labels(nodes, data, view, measure).filter(p => p.kind === "feed" || p.kind === "ed").map(p => node.get(p.id));
    assert.ok(named.length > 10, view);
    for (let i = 1; i < named.length; i++) {
      assert.ok(before(named[i - 1], named[i]) <= 0, `${view}: ${named[i - 1].title} placed before busier ${named[i].title}`);
    }
  }
});

test("label_placements_carry_what_the_drawing_needs", () => {
  const data = fixture(), nodes = core.place(data, "harbour"), placements = core.labels(nodes, data, "harbour", measure);
  placements.forEach(p => {
    assert.equal(typeof p.text, "string");
    assert.ok(Number.isFinite(p.x) && Number.isFinite(p.y) && Number.isFinite(p.angle) && p.size > 0, p.text);
    assert.match(p.font, /\d(\.\d+)?px /);
  });
  const territory = placements.find(p => p.kind === "territory");
  assert.ok(Math.abs(territory.angle) <= Math.PI / 2, "territory names never read upside down");
  const feeds = placements.filter(p => p.kind === "feed");
  feeds.forEach(p => assert.ok([...p.text].length <= 26, `"${p.text}" is too long for the chart`));
  const long = feeds.find(p => p.text.startsWith("The Extraordinarily"));
  assert.equal(long.text, "The Extraordinarily Long-\u2026", "long names are cut at 25 characters and an ellipsis");
  placements.filter(p => p.leader).forEach(p => {
    const n = nodes.find(m => m.id === p.id);
    assert.ok(Math.hypot(p.leader.x1 - n.x, p.leader.y1 - n.y) <= n.r, "a leader line starts on its island");
  });
});

test("territory_groups_have_two_members", () => {
  for (const view of VIEWS) {
    const data = fixture(), nodes = core.place(data, view), ids = new Set(nodes.map(n => n.id));
    const groups = core.territories(nodes, data);
    assert.ok(groups.length > 0, view);
    groups.forEach(t => {
      assert.ok(t.links.length >= 1, `${view}: ${t.name} has no link`);
      t.groups.forEach(g => {
        assert.ok(g.length >= 2, `${view}: ${t.name} has a lone member`);
        g.forEach(n => assert.ok(ids.has(n.id)));
      });
      t.links.forEach(([a, b]) => assert.ok(Math.hypot(a.x - b.x, a.y - b.y) < 150 * 1.2, `${view}: ${t.name} link too long`));
    });
    const continents = groups.filter(t => t.continent).map(t => t.name).sort();
    assert.ok(continents.length >= 1 && continents.every(n => data.continents.some(c => c.name === n)), view);
  }
  // a territory left with a single member on the chart draws no land
  const data = fixture();
  data.territories = [{name: "LONE ROCK", feeds: [data.feeds[0].id, 99999]}];
  data.continents = [];
  assert.deepEqual(core.territories(core.place(data, "harbour"), data), []);
});

test("unthemed_go_to_uncharted_waters", () => {
  const data = fixture();
  data.feeds[0].theme = "no-such-theme"; // a theme the document does not list counts as unthemed too
  const seas = core.seas(data), last = seas[seas.length - 1];
  assert.equal(last.key, null);
  assert.equal(last.sea, "UNCHARTED WATERS");
  assert.equal(last.count, 5);
  assert.deepEqual(seas.slice(0, -1).map(s => s.key), data.themes.map(t => t.key));

  const unthemed = n => n.theme === null;
  const harbour = core.place(data, "harbour").filter(unthemed);
  assert.equal(harbour.length, 5);
  harbour.forEach(n => {
    let a = Math.atan2((n.ty - core.PORT.y) / core.RY, (n.tx - core.PORT.x) / core.RX) - last.from;
    a = ((a % (2 * Math.PI)) + 2 * Math.PI) % (2 * Math.PI);
    assert.ok(a <= last.span + 1e-9, `${n.title} bears outside the uncharted sector`);
  });
  const themes = core.place(data, "themes").filter(unthemed);
  themes.forEach(n => assert.ok(Math.hypot(n.tx - last.x, (n.ty - last.y) / 0.75) <= last.spread + 1e-9, n.title));

  for (const view of VIEWS) {
    const nodes = core.place(data, view);
    assert.ok(core.labels(nodes, data, view, measure).some(p => p.kind === "sea" && p.text === "UNCHARTED WATERS"), view);
  }
});

test("only_blue_and_night_inks_remain_and_others_fall_back_to_blue", () => {
  assert.deepEqual(Object.keys(core.INKS), ["bleu", "nuit"]);
  for (const gone of ["pop", "pirate", "rouge"]) {
    assert.equal(core.heat(0, 1, gone), core.heat(0, 1, "bleu"), gone);
    assert.equal(core.heat(1, 1, gone), core.heat(1, 1, "bleu"), gone);
  }
});

test("heat_endpoints_per_ink", () => {
  const ramps = {
    bleu: ["#d9ece2", "#8a4417"], nuit: ["#24504c", "#ff9433"],
  };
  const rgb = hex => `rgb(${parseInt(hex.slice(1, 3), 16)}, ${parseInt(hex.slice(3, 5), 16)}, ${parseInt(hex.slice(5, 7), 16)})`;
  assert.deepEqual(Object.keys(core.INKS).sort(), Object.keys(ramps).sort());
  for (const [ink, [cold, hot]] of Object.entries(ramps)) {
    assert.equal(core.heat(0, 4, ink), rgb(cold), `${ink} cold`);
    assert.equal(core.heat(4, 4, ink), rgb(hot), `${ink} hot`);
    assert.equal(core.INKS[ink].heat.length, 6);
  }
  assert.equal(core.heat(2, 4, "no-such-ink"), core.heat(2, 4, "bleu"), "an unknown ink falls back to blue");
  assert.equal(core.heat(0.3, 0, "bleu"), core.heat(0.3, 1, "bleu"), "a quiet chart scales to at least 1");
});

test("rings_and_ringfrac", () => {
  assert.deepEqual(core.RINGS.map(r => [r.days, r.label]),
    [[1, "1 day"], [7, "1 week"], [30, "1 month"], [91, "3 months"], [182, "6 months"]]);
  const fracs = core.RINGS.map(r => core.ringFrac(r.days));
  for (let i = 1; i < fracs.length; i++) assert.ok(fracs[i - 1] < fracs[i]);
  assert.ok(fracs[fracs.length - 1] <= 1.0);
  assert.ok(Math.abs(core.ringFrac(182) - 1) < 1e-12);
  assert.ok(Math.abs(core.ringFrac(0) - 0.13) < 1e-12);
  assert.equal(core.ringFrac(400), core.ringFrac(182), "older than six months stays on the edge");
  assert.deepEqual([core.W, core.H, core.PORT.x, core.PORT.y, core.RX, core.RY], [1920, 1080, 960, 560, 830, 430]);
});

test("an_empty_chart_still_has_its_harbour", () => {
  const data = fixture();
  data.feeds = []; data.themes = []; data.territories = []; data.continents = [];
  for (const view of VIEWS) {
    const nodes = core.place(data, view);
    assert.deepEqual(nodes, []);
    assert.equal(core.field(nodes, view, data).grid.length, core.GW * core.GH);
    const kinds = core.labels(nodes, data, view, measure).map(p => p.kind);
    assert.ok(!kinds.includes("sea") && !kinds.includes("feed"), view);
    if (view === "harbour") assert.ok(kinds.includes("harbour"));
  }
});

test("field_is_land_under_islands_and_sea_far_out", () => {
  const data = fixture(), nodes = core.place(data, "themes"), f = core.field(nodes, "themes", data);
  nodes.forEach(n => assert.ok(f.at(n.x, n.y) >= 0.5, `${n.title} is not on land`));
  assert.ok(f.at(45, H - 45) < 0.5, "the far corner is open sea");
});

test("works_as_a_page_script_with_the_global_d3", () => {
  // as in the page: two classic scripts, no module system, d3 found on the global object
  const page = vm.createContext({setTimeout, clearTimeout, setInterval, clearInterval}); // d3 schedules with timers
  vm.runInContext(fs.readFileSync(path.join(CHART, "d3.min.js"), "utf8"), page);
  vm.runInContext(fs.readFileSync(path.join(CHART, "chart-core.js"), "utf8"), page);
  const pageCore = vm.runInContext("ChartCore", page);
  const nodes = pageCore.place(fixture(), "harbour");
  assert.deepEqual(nodes.map(n => [n.x, n.y]), core.place(fixture(), "harbour").map(n => [n.x, n.y]));
  assert.equal(pageCore.heat(1, 1, "nuit"), core.heat(1, 1, "nuit"));
});
