// Tests of the gesture geometry in chart-core.js (presses, loops, drop targets, the cartouche's spot): pure.
"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");
const {measure, fakeDocument} = require("./fake-dom.cjs");

const CHART = path.join(__dirname, "..", "Ressac", "Resources", "Chart");
const d3 = require(path.join(CHART, "d3.min.js"));
const core = require(path.join(CHART, "chart-core.js"));
core.useD3(d3);

const fixture = () => JSON.parse(JSON.stringify(require("./fixture.json")));
const T = core.EDIT_TEXT;

// the scene Task 11 builds from the real chart: what buildScene returns, made here from place, territories and field
function sceneOf(data, view) {
  const nodes = core.place(data, view), terr = core.territories(nodes, data);
  return {view, nodes, territories: terr, ground: core.field(nodes, view, data, terr)};
}
const node = (scene, id) => scene.nodes.find(n => n.id === id);
const inLand = (scene, id) => scene.territories.some(t => t.members.some(m => m.id === id));
const segDist = (x, y, a, b) => {
  const dx = b.x - a.x, dy = b.y - a.y, l2 = dx * dx + dy * dy || 1;
  const u = Math.max(0, Math.min(1, ((x - a.x) * dx + (y - a.y) * dy) / l2));
  return Math.hypot(x - a.x - u * dx, y - a.y - u * dy);
};
// a point in clear water: no island within r + 40, no link within 40, open sea; `want(x, y)` narrows it further
function freePoint(scene, want = () => true) {
  for (let y = 100; y < 960; y += 20) for (let x = 100; x < 1820; x += 20) {
    if (scene.nodes.some(n => Math.hypot(n.x - x, n.y - y) < n.r + 40)) continue;
    if (scene.territories.some(t => t.links.some(([a, b]) => segDist(x, y, a, b) < 40))) continue;
    if (scene.ground.at(x, y) >= .4 || !want(x, y)) continue;
    return {x, y};
  }
  throw new Error("no free point");
}
const nearestCell = (data, x, y) => core.seas(data).reduce((b, s) => (Math.hypot(s.x - x, s.y - y) < Math.hypot(b.x - x, b.y - y) ? s : b));

test("a_press_that_moves_on_a_name_draws", () => {
  const at = (x, y) => ({x, y});
  assert.equal(core.classifyPress([at(100, 100)]), "click");
  assert.equal(core.classifyPress([at(100, 100), at(103, 100), at(100, 103), at(102, 102)]), "click"); // 3 px of jitter
  assert.equal(core.classifyPress([at(100, 100), at(103, 100), at(105, 100)]), "stroke"); // a 5 px move
  assert.equal(core.classifyPress([[100, 100], [100, 104]]), "stroke"); // 4 px exactly
  assert.equal(core.classifyPress([at(100, 100), at(120, 100), at(100, 101)]), "stroke"); // went out, came back: still a stroke
});

test("closed_loop_needs_length_and_closure", () => {
  const ring = (r, n, closeBy = 0) => Array.from({length: n + 1}, (_, i) => {
    const a = 2 * Math.PI * Math.min(i, n) / n;
    return {x: 500 + r * Math.cos(a) + (i === n ? closeBy : 0), y: 500 + r * Math.sin(a)};
  });
  assert.equal(core.closedLoop(ring(30, 40)), true); // length ~188
  assert.equal(core.closedLoop(ring(15, 40)), false); // length ~94: too short
  assert.equal(core.closedLoop(ring(30, 40, 39)), true); // ends 39 from the start
  assert.equal(core.closedLoop(ring(30, 40, 45)), false); // ends 45 away
  const line = Array.from({length: 50}, (_, i) => ({x: 500 + i * 10, y: 500}));
  assert.equal(core.closedLoop(line), false); // long but open
  assert.equal(core.closedLoop([]), false);
  assert.equal(core.closedLoop([{x: 1, y: 1}]), false);
});

// a hand-made chart: nodes and the data's feeds in an order that differs from the nodes'
function mini(feeds, continents = []) {
  const themes = ["a", "b", "c"].map((k, i) => ({id: i + 1, key: k, name: k, sea: k.toUpperCase()}));
  const nodes = feeds.map(([id, theme, x, y]) => ({id, theme, x, y, r: 20}));
  const data = {themes, continents: continents.map((c, i) => ({id: 10 + i, name: c.toUpperCase(), theme: c})),
    feeds: feeds.map(([id, theme]) => ({id, theme})).sort((p, q) => p.id - q.id)};
  return {nodes, data};
}
const rect = (x0, y0, x1, y1) => [{x: x0, y: y0}, {x: x1, y: y0}, {x: x1, y: y1}, {x: x0, y: y1}];

test("loop_selects_by_centre_only", () => {
  const {nodes, data} = mini([[3, null, 400, 300], [1, null, 500, 300], [2, null, 800, 700]]);
  data.feeds = [{id: 3, theme: null}, {id: 1, theme: null}, {id: 2, theme: null}]; // the order of the chart
  // round the first two: both, in the order of data.feeds
  assert.deepEqual(core.loopSelection(nodes, rect(350, 250, 560, 350), data).islands, [3, 1]);
  // over island 1's edge and label but not its centre (500, 300): not selected
  const sel = core.loopSelection(nodes, rect(505, 250, 640, 350), data);
  assert.deepEqual(sel.islands, []);
  // a loop with its centre inside and the rest outside still selects it
  assert.deepEqual(core.loopSelection(nodes, rect(495, 295, 505, 305), data).islands, [1]);
  // any sea: a loop over islands of different seas selects them all
  const m = mini([[1, "a", 100, 100], [2, "b", 160, 100], [3, null, 220, 100]]);
  assert.deepEqual(core.loopSelection(m.nodes, rect(50, 50, 300, 150), m.data).islands, [1, 2, 3]);
  // too few points to enclose anything
  assert.deepEqual(core.loopSelection(nodes, [{x: 0, y: 0}, {x: 900, y: 900}], data), {islands: [], continentSea: null});
});

test("loop_offers_a_continent_for_one_whole_sea", () => {
  const {nodes, data} = mini([[1, "a", 100, 100], [2, "a", 160, 100], [3, "a", 130, 160], [4, "b", 700, 100], [5, "b", 760, 100], [6, "c", 100, 600], [7, "c", 160, 600]]);
  const around = (ids, pad = 40) => {
    const ns = nodes.filter(n => ids.includes(n.id));
    return rect(Math.min(...ns.map(n => n.x)) - pad, Math.min(...ns.map(n => n.y)) - pad, Math.max(...ns.map(n => n.x)) + pad, Math.max(...ns.map(n => n.y)) + pad);
  };
  assert.equal(core.loopSelection(nodes, around([1, 2, 3]), data).continentSea, "a"); // all of a sea without a continent
  assert.equal(core.loopSelection(nodes, around([1, 2]), data).continentSea, null); // one island missing
  assert.equal(core.loopSelection(nodes, rect(60, 60, 800, 160), data).continentSea, null); // all of a, plus another sea's islands
  assert.equal(core.loopSelection(nodes, around([4]), data).continentSea, null); // fewer than two islands
  assert.equal(core.loopSelection(nodes, around([1, 2, 3, 6]), data).continentSea, null); // one island from another sea
  // a sea that has a continent: no offer, and its continental islands are left out of `islands`
  const withC = mini([[1, "a", 100, 100], [2, "a", 160, 100], [3, "b", 700, 100], [4, "b", 760, 100]], ["a"]);
  const sel = core.loopSelection(withC.nodes, rect(50, 50, 220, 150), withC.data);
  assert.equal(sel.continentSea, null);
  assert.deepEqual(sel.islands, []);
  const mixed = core.loopSelection(withC.nodes, rect(50, 50, 800, 150), withC.data);
  assert.deepEqual(mixed.islands, [3, 4]);
  assert.equal(mixed.continentSea, null);
  // the uncharted waters are not a sea that can have a continent
  const lost = mini([[1, null, 100, 100], [2, null, 160, 100]]);
  assert.equal(core.loopSelection(lost.nodes, rect(50, 50, 220, 150), lost.data).continentSea, null);
});

test("loop_selection_is_right_on_a_large_input", () => {
  // 2400 islands on a lattice over 4 seas, one loop of 360 points (a wavy ring): compare with an independent test
  const feeds = [];
  for (let i = 0; i < 2400; i++) feeds.push([i + 1, ["a", "b", "c", null][i % 4], 40 + (i % 60) * 30, 40 + Math.floor(i / 60) * 30]);
  const {nodes, data} = mini(feeds);
  const loop = Array.from({length: 360}, (_, i) => {
    const a = 2 * Math.PI * i / 360, r = 420 + 140 * Math.sin(5 * a);
    return {x: 900 + r * Math.cos(a), y: 600 + r * Math.sin(a)};
  });
  const inside = (px, py) => { // even-odd ray casting
    let c = false;
    for (let i = 0, j = loop.length - 1; i < loop.length; j = i++) {
      const a = loop[i], b = loop[j];
      if ((a.y > py) !== (b.y > py) && px < (b.x - a.x) * (py - a.y) / (b.y - a.y) + a.x) c = !c;
    }
    return c;
  };
  const expected = data.feeds.map(f => f.id).filter(id => { const n = nodes.find(m => m.id === id); return inside(n.x, n.y); });
  assert.ok(expected.length > 300 && expected.length < 2000, `the loop must split the lattice (${expected.length})`);
  const sel = core.loopSelection(nodes, loop, data);
  assert.deepEqual(sel.islands, expected);
  assert.equal(sel.continentSea, null);
});

test("name_at_finds_territory_continent_and_sea_names_only", () => {
  const data = fixture(), nodes = core.place(data, "themes"), terr = core.territories(nodes, data);
  const placements = core.labels(nodes, data, "themes", measure, {territories: terr});
  const centre = p => [(p.box[0] + p.box[2]) / 2, (p.box[1] + p.box[3]) / 2];
  for (const kind of ["territory", "continent", "sea"]) {
    const p = placements.find(q => q.kind === kind);
    assert.ok(p, `the fixture has a ${kind} name`);
    assert.equal(core.nameAt(placements, ...centre(p)), p, kind);
  }
  const feed = placements.find(q => q.kind === "feed");
  assert.ok(feed);
  assert.equal(core.nameAt(placements.filter(q => q.kind === "feed"), ...centre(feed)), null); // an island's own label is not a name
  assert.equal(core.nameAt(placements, -50, -50), null);
});

test("drop_targets_follow_the_precedence", () => {
  const data = fixture(), themes = sceneOf(data, "themes"), harbour = sceneOf(data, "harbour");
  const lone = themes.nodes.filter(n => !inLand(themes, n.id));
  const L = lone[0], M = lone[1], P = node(themes, 1);
  // on a territory's island: join its land (a plain island dragged)
  const onTerritory = core.dropTarget(themes, data, L, P.x, P.y);
  assert.equal(onTerritory.kind, "join");
  assert.equal(onTerritory.land.name, "GULL PRESS GROUP");
  // on a lone island: pair
  assert.deepEqual(core.dropTarget(themes, data, L, M.x, M.y), {kind: "pair", with: M.id});
  // on a continent's island: nothing (a continent is not joined by hand)
  const onContinent = themes.nodes.find(n => data.continents.some(c => c.theme === n.theme)
    && !themes.territories.some(t => !t.continent && t.members.some(m => m.id === n.id)));
  assert.equal(core.dropTarget(themes, data, L, onContinent.x, onContinent.y).kind, "none");
  // on itself: nothing
  assert.equal(core.dropTarget(themes, data, L, L.x, L.y).kind, "none");
  // in another sea's cell (themes view): sea, with that sea's key; the same spot in harbour view is not a sea
  const own = L.theme;
  const spot = freePoint(themes, (x, y) => nearestCell(data, x, y).key !== own);
  const cell = nearestCell(data, spot.x, spot.y);
  assert.deepEqual(core.dropTarget(themes, data, L, spot.x, spot.y), {kind: "sea", key: cell.key});
  assert.notEqual(core.dropTarget(harbour, data, harbour.nodes.find(n => n.id === L.id), spot.x, spot.y).kind, "sea");
  // the uncharted waters are a cell too: key null
  if (own !== null) {
    const lost = freePoint(themes, (x, y) => nearestCell(data, x, y).key === null);
    assert.deepEqual(core.dropTarget(themes, data, L, lost.x, lost.y), {kind: "sea", key: null});
  }
  // open water for an island in a territory: leave (harbour view has no seas to drop on)
  const open = freePoint(harbour);
  const land = harbour.territories.find(t => !t.continent && t.members.some(m => m.id === 1));
  assert.deepEqual(core.dropTarget(harbour, data, node(harbour, 1), open.x, open.y), {kind: "leave", land});
  // open water for a lone island: nothing
  assert.deepEqual(core.dropTarget(harbour, data, harbour.nodes.find(n => n.id === L.id), open.x, open.y), {kind: "none"});
  // a territory's link: join its land
  const [a, b] = themes.territories.find(t => t.name === "SALTWORKS").links[0];
  const mid = {x: (a.x + b.x) / 2, y: (a.y + b.y) / 2};
  const onLink = core.dropTarget(themes, data, L, mid.x, mid.y);
  assert.equal(onLink.kind, "join");
  assert.equal(onLink.land.name, "SALTWORKS");
});

test("dropping_on_its_own_land_is_a_no_op", () => {
  const data = fixture();
  for (const view of ["themes", "harbour"]) {
    const scene = sceneOf(data, view), land = scene.territories.find(t => t.name === "GULL PRESS GROUP");
    const dragged = node(scene, 1);
    for (const other of land.members.filter(m => m.id !== 1)) {
      assert.deepEqual(core.dropTarget(scene, data, dragged, other.x, other.y), {kind: "none"}, `${view}: its land's island ${other.id}`);
    }
    for (const [a, b] of land.links) {
      const mid = {x: (a.x + b.x) / 2, y: (a.y + b.y) / 2};
      assert.deepEqual(core.dropTarget(scene, data, dragged, mid.x, mid.y), {kind: "none"}, `${view}: its land's link`);
    }
  }
});

test("continental_islands_only_change_sea", () => {
  const data = fixture(), themes = sceneOf(data, "themes");
  const cont = themes.nodes.find(n => n.theme === "bells"), hint = T.continentHint("BELLMARK");
  const P = node(themes, 1), lone = themes.nodes.find(n => !inLand(themes, n.id) && !data.continents.some(c => c.theme === n.theme));
  assert.deepEqual(core.dropTarget(themes, data, cont, P.x, P.y), {kind: "refuse", hint}); // would join
  assert.deepEqual(core.dropTarget(themes, data, cont, lone.x, lone.y), {kind: "refuse", hint}); // would pair
  const harbour = sceneOf(data, "harbour"), c2 = node(harbour, cont.id);
  const open = freePoint(harbour);
  assert.deepEqual(core.dropTarget(harbour, data, c2, open.x, open.y), {kind: "refuse", hint}); // would leave
  const spot = freePoint(themes, (x, y) => nearestCell(data, x, y).key !== "bells");
  assert.deepEqual(core.dropTarget(themes, data, cont, spot.x, spot.y), {kind: "sea", key: nearestCell(data, spot.x, spot.y).key});
  // on its own continent's island: nothing, not a refusal
  const mate = themes.territories.find(t => t.name === "BELLMARK").members.find(m => m.id !== cont.id);
  assert.deepEqual(core.dropTarget(themes, data, cont, mate.x, mate.y), {kind: "none"});
});

test("the_scene_shape_is_checked", () => {
  const data = fixture();
  for (const view of ["themes", "harbour"]) assert.deepEqual(core.sceneProblems(sceneOf(data, view)), [], view);
  const scene = sceneOf(data, "themes");
  assert.ok(core.sceneProblems({...scene, ground: {}}).some(p => /ground/.test(p)));
  assert.ok(core.sceneProblems({...scene, view: "sideways"}).some(p => /view/.test(p)));
  assert.ok(core.sceneProblems({...scene, nodes: scene.nodes.map(n => ({...n, r: undefined}))}).some(p => /node/.test(p)));
  assert.ok(core.sceneProblems({...scene, territories: [{name: "X", continent: false, links: [[1, 2]], members: []}]}).length > 0);
  assert.ok(core.sceneProblems(null).length > 0);
});

test("cartouche_avoids_labels_and_falls_back_when_every_spot_is_taken", () => {
  const size = {w: 350, h: 128}, loop = [800, 450, 1100, 650];
  const spots = core.cartoucheSpots(loop, size); // the four candidate rectangles: right, left, below, above
  assert.equal(spots.length, 4);
  const centreOf = s => ({x: (s[0] + s[2]) / 2, y: (s[1] + s[3]) / 2});
  const meets = (a, b) => a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3];
  let seed = 12345;
  const rnd = () => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed / 4294967296; };
  const boxIn = s => { // a small label box that overlaps the spot
    const x = s[0] + rnd() * (s[2] - s[0]), y = s[1] + rnd() * (s[3] - s[1]);
    return [x - 20, y - 8, x + 20, y + 8];
  };
  // free: the "above" spot; 500 boxes, spread over the other three spots, with the fourth guaranteed clear
  for (let free = 0; free < 4; free++) {
    const taken = [0, 1, 2, 3].filter(i => i !== free);
    const placements = [];
    while (placements.length < 500) {
      const b = boxIn(spots[taken[placements.length % 3]]);
      if (!meets(b, spots[free])) placements.push({box: b});
    }
    const got = core.cartouchePlace(loop, placements, size);
    assert.deepEqual(got, centreOf(spots[free]), `spot ${free} was the only free one`);
    const box = [got.x - size.w / 2, got.y - size.h / 2, got.x + size.w / 2, got.y + size.h / 2];
    assert.ok(placements.every(p => !meets(box, p.box)), "meets no label");
    assert.ok(box[0] >= 40 && box[1] >= 40 && box[2] <= core.W - 40 && box[3] <= core.H - 40, "inside the margin");
  }
  // none free: every spot has labels over it, the fallback is exact
  const all = Array.from({length: 500}, (_, i) => ({box: boxIn(spots[i % 4])}));
  assert.deepEqual(core.cartouchePlace(loop, all, size), {x: 40 + size.w / 2, y: 40 + size.h / 2});
  // no labels at all: the first spot, to the right
  assert.deepEqual(core.cartouchePlace(loop, [], size), centreOf(spots[0]));
  // a loop at the right edge has no room to its right: it goes left
  const edge = [1500, 450, 1880, 650];
  assert.ok(core.cartouchePlace(edge, [], size).x < 1500);
});

test("edit_motion_is_off_when_motion_is_reduced", () => {
  assert.equal(core.editMotion({reduceMotion: true}, false), false);
  assert.equal(core.editMotion({reduceMotion: false}, true), false);
  assert.equal(core.editMotion({reduceMotion: false}, false), true);
  assert.equal(core.editMotion({}, false), true);
  assert.equal(core.editMotion(undefined, false), true);
  assert.equal(core.editMotion({reduceMotion: true}, true), false);
});

test("the_fake_dom_takes_any_event_and_a_settable_rect", () => {
  const doc = fakeDocument(), el = doc.createElement("div"), seen = [];
  el.addEventListener("pointerdown", e => seen.push(e));
  el.dispatch("pointerdown", {x: 1});
  assert.deepEqual(seen, [{x: 1}]);
  el.rect = {left: 5, top: 6, width: 7, height: 8};
  assert.deepEqual(el.getBoundingClientRect(), {left: 5, top: 6, width: 7, height: 8});
});
