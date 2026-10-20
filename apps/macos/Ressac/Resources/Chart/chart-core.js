// The sea chart's pure core: seeded noise, seas and bearings, island placement, the height field
// with its invented coasts, territory groups, label placement and the activity heat ramps.
// No DOM and no canvas: the page draws what these functions return, and Node tests them.
// d3 comes from the page (globalThis.d3) or is injected with useD3(d3) under Node.
(function (root, factory) {
  "use strict";
  const core = factory();
  if (typeof module === "object" && module.exports) module.exports = core;
  else root.ChartCore = core;
})(typeof globalThis !== "undefined" ? globalThis : this, function () {
  "use strict";

  let injectedD3 = null;
  function useD3(lib) { injectedD3 = lib; }
  function d3() {
    const lib = injectedD3 || globalThis.d3;
    if (!lib) throw new Error("ChartCore needs d3: load d3.min.js first, or call ChartCore.useD3(d3)");
    return lib;
  }

  // ---------- the chart's frame ----------
  const W = 1920, H = 1080, CELL = 4, GW = W / CELL, GH = H / CELL;
  const PORT = Object.freeze({x: 960, y: 560}), RX = 830, RY = 430;
  const AGEMAX = 182; // six months: older feeds are not on the chart
  const RINGS = Object.freeze([[1, "1 day"], [7, "1 week"], [30, "1 month"], [91, "3 months"], [182, "6 months"]]
    .map(([days, label]) => Object.freeze({days, label})));
  const RADIUS = Object.freeze({daily: 36, weekly: 27, monthly: 21, rare: 16, silent: 11});
  const HARBOUR_SCALE = 0.7; // islands are smaller round the harbour, where most of them crowd
  const MAXLINK = 150; // longest stretch of land between two headlands, in chart units
  const ISOBATHS = Object.freeze([.08, .15, .25, .37, .5, .68, .84]); // .5 is the coast
  const CARTOUCHE = Object.freeze({x: W - 400, y: H - 180, w: 350, h: 128});
  const COMPASS = Object.freeze({x: 160, y: 165, r: 70});
  const UNCHARTED = Object.freeze({name: "Uncharted waters", sea: "UNCHARTED WATERS"});
  const FONTS = Object.freeze({
    display: '"Cormorant SC", "Cormorant Garamond", Garamond, serif',
    body: '"Cormorant Garamond", Garamond, serif',
    mono: '"IBM Plex Mono", Menlo, monospace',
  });
  const INKS = Object.freeze(Object.fromEntries(Object.entries({
    bleu: {paper: "#f7f8f6", ink: "#1f3c88", dot: "#1f3c88", land: "#f7f8f6", label: "#1f3c88", beam: "31,60,136", ripple: "#1f3c88",
      heat: ["#d9ece2", "#a7d3c0", "#e9cf72", "#dd9a3c", "#b8642a", "#8a4417"]},
    nuit: {paper: "#0b0d12", ink: "#e8ecf4", dot: "#e8ecf4", land: "#0b0d12", label: "#e8ecf4", beam: "232,236,244", ripple: "#e8ecf4",
      heat: ["#24504c", "#3f9c92", "#7fd3c4", "#ecd77f", "#ffb347", "#ff9433"]},
  }).map(([name, ink]) => [name, Object.freeze({...ink, heat: Object.freeze(ink.heat)})])));

  const font = (px, family, style) => (style ? `${style} ${px}px ${family}` : `${px}px ${family}`);

  // ---------- seeded helpers ----------
  function hash(s) { let h = 2166136261; for (const c of s) { h ^= c.codePointAt(0); h = Math.imul(h, 16777619); } return h >>> 0; }
  function mulberry(a) {
    return function () {
      a |= 0; a = a + 0x6D2B79F5 | 0;
      let t = Math.imul(a ^ a >>> 15, 1 | a);
      t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t;
      return ((t ^ t >>> 14) >>> 0) / 4294967296;
    };
  }
  const perm = (() => {
    const r = mulberry(1910), p = [...Array(256).keys()];
    for (let i = 255; i > 0; i--) { const j = Math.floor(r() * (i + 1)); [p[i], p[j]] = [p[j], p[i]]; }
    return p.concat(p);
  })();
  const grad = (h, x, y) => ((h & 1) ? x : -x) + ((h & 2) ? y : -y);
  const fade = t => t * t * t * (t * (t * 6 - 15) + 10);
  function noise(x, y) {
    const X = Math.floor(x) & 255, Y = Math.floor(y) & 255; x -= Math.floor(x); y -= Math.floor(y);
    const u = fade(x), v = fade(y), a = perm[X] + Y, b = perm[X + 1] + Y;
    const l1 = grad(perm[a], x, y) + u * (grad(perm[b], x - 1, y) - grad(perm[a], x, y));
    const l2 = grad(perm[a + 1], x, y - 1) + u * (grad(perm[b + 1], x - 1, y - 1) - grad(perm[a + 1], x, y - 1));
    return l1 + v * (l2 - l1);
  }
  const fbm = (x, y) => noise(x, y) * .6 + noise(x * 2.1, y * 2.1) * .28 + noise(x * 4.3, y * 4.3) * .12;

  // distance from the harbour for a feed silent for `age` days: stretched near the quay, where most feeds are
  const ringFrac = age => .13 + .87 * Math.pow(Math.min(1, Math.log1p(Math.max(0, age)) / Math.log1p(AGEMAX)), .5);

  function checkView(view) {
    if (view !== "harbour" && view !== "themes") throw new TypeError(`unknown chart view: ${view}`);
    return view === "harbour";
  }
  // a feed whose theme the document does not list sails in the uncharted waters
  const seaKeyOf = (feed, known) => (feed.theme != null && known.has(feed.theme) ? feed.theme : null);

  // ---------- seas: one per theme, in position order, then the uncharted waters ----------
  // Each sea has a bearing round the harbour (a sector sized by its feeds, the first one centred on the north)
  // and a cell in the "By theme" grid, walked like a serpentine so neighbours stay neighbours. The uncharted
  // waters always keep the last cell, so the first unthemed feed does not reshuffle the other seas.
  function seas(data) {
    const known = new Set(data.themes.map(t => t.key)), count = new Map();
    data.feeds.forEach(f => { const k = seaKeyOf(f, known); count.set(k, (count.get(k) || 0) + 1); });
    const list = data.themes.map(t => ({key: t.key, name: t.name, sea: t.sea, count: count.get(t.key) || 0}));
    list.push({key: null, name: UNCHARTED.name, sea: UNCHARTED.sea, count: count.get(null) || 0});

    const total = data.feeds.length;
    let a = -Math.PI / 2 - (total ? Math.PI * list[0].count / total : 0);
    list.forEach(s => { s.from = a; s.span = total ? 2 * Math.PI * s.count / total : 0; a += s.span; });

    const n = list.length, cols = Math.max(Math.min(n, 5), Math.ceil(n / 3)), rows = Math.ceil(n / cols);
    list.forEach((s, i) => {
      const row = Math.floor(i / cols), col = row % 2 ? cols - 1 - i % cols : i % cols;
      s.x = cols > 1 ? 250 + col * 1420 / (cols - 1) : W / 2;
      s.y = rows > 1 ? 230 + row * 640 / (rows - 1) : H / 2;
      s.spread = 34 + Math.sqrt(s.count) * 24;
    });
    return list;
  }

  // ---------- islands ----------
  const room = n => n.r * 1.35 + 8; // the water an island keeps round itself
  const inBounds = (x, y) => [Math.max(80, Math.min(W - 80, x)), Math.max(80, Math.min(H - 110, y))];
  // the compass rose and the cartouche (three discs along it) keep their water: islands settle round them
  const KEEP_CLEAR = Object.freeze([{x: COMPASS.x, y: COMPASS.y, r: COMPASS.r + 12},
    ...[58, 175, 292].map(dx => ({x: CARTOUCHE.x + dx, y: CARTOUCHE.y + CARTOUCHE.h / 2, r: 88}))]);

  // Round the harbour, distance is time since the last entry and the bearing is the sea's sector; by theme,
  // each feed's target depends only on its own name, its sea and its territory, so a newcomer only nudges.
  function place(data, view) {
    const harbour = checkView(view);
    const list = seas(data), sea = new Map(list.map(s => [s.key, s]));
    const known = new Set(data.themes.map(t => t.key));
    const continental = new Set(data.continents.map(c => c.theme));
    const family = new Map(); // feed id → [territory, position in its chain]
    data.territories.forEach(t => t.feeds.forEach((id, i) => family.set(id, [t, i])));

    const nodes = data.feeds.map(f => ({
      id: f.id, title: f.title ?? "", theme: seaKeyOf(f, known), group: f.group,
      age: f.age_days, unread: f.unread_week || 0, week: f.entries_week || 0, activity: f.activity || 0, feed: f,
      r: (RADIUS[f.group] || RADIUS.silent) * (harbour ? HARBOUR_SCALE : 1), seed: hash(f.title ?? ""),
    }));
    nodes.forEach(n => {
      const s = sea.get(n.theme), fam = family.get(n.id);
      if (harbour) {
        const r = mulberry(n.seed), rf = ringFrac(n.age ?? AGEMAX);
        const th = fam ? s.from + s.span * (.15 + .7 * mulberry(hash(fam[0].name))()) + (fam[1] % 2 ? .035 : -.035)
          : continental.has(n.theme) ? s.from + s.span * (.5 + (r() - .5) * .5) : s.from + s.span * (.12 + .76 * r());
        n.tx = PORT.x + Math.cos(th) * RX * rf; n.ty = PORT.y + Math.sin(th) * RY * rf;
        n.x = n.tx + (r() - .5) * 20; n.y = n.ty + (r() - .5) * 20;
      } else {
        const r = mulberry(fam ? hash(fam[0].name) : n.seed), a = r() * Math.PI * 2;
        const d = Math.sqrt(r()) * s.spread * (fam ? .5 : continental.has(n.theme) ? .6 : 1);
        n.tx = s.x + Math.cos(a) * d; n.ty = s.y + Math.sin(a) * d * .75;
        if (fam) { const dir = r() * Math.PI; n.tx += Math.cos(dir) * fam[1] * 58; n.ty += Math.sin(dir) * fam[1] * 40; }
        n.x = n.tx; n.y = n.ty;
      }
    });

    if (harbour) {
      const D = d3(), clear = KEEP_CLEAR.map(k => ({x: k.x, y: k.y, tx: k.x, ty: k.y, fx: k.x, fy: k.y, keep: k.r}));
      D.forceSimulation(nodes.concat(clear))
        .force("x", D.forceX(d => d.tx).strength(.5))
        .force("y", D.forceY(d => d.ty).strength(.5))
        .force("c", D.forceCollide(d => d.keep ?? room(d)).iterations(3))
        .stop().tick(400);
      nodes.forEach(n => {
        [n.x, n.y] = inBounds(n.x, n.y);
        if (Math.hypot(n.x - PORT.x, (n.y - PORT.y) * 1.6) < 70) { // keep the quay clear
          const a = Math.atan2(n.y - PORT.y, n.x - PORT.x);
          n.x = PORT.x + Math.cos(a) * 75; n.y = PORT.y + Math.sin(a) * 50;
        }
        delete n.vx; delete n.vy; delete n.index;
      });
    } else settle(nodes);
    nodes.forEach(n => { n.sa = mulberry(n.seed ^ 0x5bd1)() * Math.PI * 2; }); // where soundings and leader lines start looking
    return nodes;
  }

  // By theme, islands settle one at a time in the order their feeds joined Ressac (by id), each on the free
  // spot nearest its target among the islands already settled. A newcomer therefore finds room without
  // pushing anyone, where a global force layout would reshuffle a crowded sea whenever one island lands in it.
  // When the full water round an island no longer fits anywhere, islands settle closer, down to the least
  // the chart allows (centres 0.82 × (r1 + r2) apart); past that, on the spot where they overlap least.
  const CLEARANCES = [room, n => n.r + 4, n => n.r * .82];
  function settle(nodes) {
    // Water only shrinks as islands settle, so once no island of radius r fits at a clearance, no later island
    // of radius r or more can: those skip straight to the next clearance.
    const settled = [], full = CLEARANCES.map(() => Infinity);
    nodes.slice().sort((a, b) => a.id - b.id).forEach(n => {
      let spot = null;
      for (let level = 0; level < CLEARANCES.length && !spot; level++) {
        if (n.r >= full[level]) continue;
        spot = nearestFree(n, settled, CLEARANCES[level]);
        if (!spot) full[level] = n.r;
      }
      [n.x, n.y] = spot || leastCrowded(n, settled, CLEARANCES[CLEARANCES.length - 1]);
      settled.push(n);
    });
  }
  const X0 = 80, X1 = W - 80, Y0 = 80, Y1 = H - 110; // where island centres may lie (as inBounds)
  // The water a settling island must keep from each settled one and from the compass and cartouche, as discs
  // round them, filed in a grid of cells as wide as the largest disc so a point is checked against its
  // neighbours only.
  function obstacles(n, settled, gap) {
    const keep = Math.max(gap(n), n.r + 4);
    const list = KEEP_CLEAR.map(k => ({x: k.x, y: k.y, r: keep + k.r}))
      .concat(settled.map(m => ({x: m.x, y: m.y, r: gap(n) + gap(m)})));
    const size = Math.max(1, ...list.map(c => c.r)), cols = Math.ceil(W / size) + 1, rows = Math.ceil(H / size) + 1;
    const cells = Array.from({length: cols * rows}, () => []);
    const col = x => Math.min(cols - 1, Math.max(0, Math.floor(x / size))), row = y => Math.min(rows - 1, Math.max(0, Math.floor(y / size)));
    list.forEach(c => cells[row(c.y) * cols + col(c.x)].push(c));
    // true as soon as `test` holds for a disc filed within `span` cells of (x, y)
    const some = (x, y, span, test) => {
      const cx = col(x), cy = row(y);
      for (let j = Math.max(0, cy - span); j <= Math.min(rows - 1, cy + span); j++) {
        for (let i = Math.max(0, cx - span); i <= Math.min(cols - 1, cx + span); i++) {
          for (const c of cells[j * cols + i]) if (test(c)) return true;
        }
      }
      return false;
    };
    return {list, some};
  }
  // The points where the nearest free spot can lie: the target's projection on a disc's edge, two discs'
  // crossings, a disc meeting the margin, or the margin itself, all within `reach` of the target.
  function spots(n, discs, tx, ty, reach) {
    const points = [[tx, ty], [tx, Y0], [tx, Y1], [X0, ty], [X1, ty], [X0, Y0], [X0, Y1], [X1, Y0], [X1, Y1]];
    const near = new Set(discs.list.filter(c => Math.hypot(tx - c.x, ty - c.y) - c.r <= reach));
    near.forEach(c => {
      const d = Math.hypot(tx - c.x, ty - c.y), a = d > 1e-9 ? Math.atan2(ty - c.y, tx - c.x) : (n.seed % 360) * Math.PI / 180;
      points.push([c.x + Math.cos(a) * c.r, c.y + Math.sin(a) * c.r]);
      for (const x of [X0, X1]) { const h = c.r * c.r - (x - c.x) ** 2; if (h >= 0) points.push([x, c.y - Math.sqrt(h)], [x, c.y + Math.sqrt(h)]); }
      for (const y of [Y0, Y1]) { const h = c.r * c.r - (y - c.y) ** 2; if (h >= 0) points.push([c.x - Math.sqrt(h), y], [c.x + Math.sqrt(h), y]); }
      discs.some(c.x, c.y, 2, e => { // two discs that cross lie within two cells of each other
        if (e === c || !near.has(e) || e.x < c.x || (e.x === c.x && e.y <= c.y)) return false; // each pair once
        const dx = e.x - c.x, dy = e.y - c.y, dd = Math.hypot(dx, dy);
        if (dd < 1e-9 || dd > c.r + e.r || dd < Math.abs(c.r - e.r)) return false;
        const along = (c.r * c.r - e.r * e.r + dd * dd) / (2 * dd), h = Math.sqrt(Math.max(0, c.r * c.r - along * along));
        const mx = c.x + dx * along / dd, my = c.y + dy * along / dd;
        points.push([mx - dy * h / dd, my + dx * h / dd], [mx + dy * h / dd, my - dx * h / dd]);
        return false;
      });
    });
    return points.map(p => [Math.hypot(p[0] - tx, p[1] - ty), p]).filter(([d]) => d <= reach).sort((a, b) => a[0] - b[0]);
  }
  const inChart = ([x, y]) => x >= X0 - 1e-9 && x <= X1 + 1e-9 && y >= Y0 - 1e-9 && y <= Y1 + 1e-9;
  // The point nearest the target that keeps `gap` of water from every settled island, inside the chart;
  // candidates are tried nearest first, within a reach that doubles until one is free. Null when none is.
  function nearestFree(n, settled, gap) {
    const [tx, ty] = inBounds(n.tx, n.ty), discs = obstacles(n, settled, gap);
    const free = p => inChart(p) && !discs.some(p[0], p[1], 1, c => Math.hypot(p[0] - c.x, p[1] - c.y) < c.r - 1e-6);
    if (free([tx, ty])) return [tx, ty];
    for (let reach = 150; ; reach *= 2) {
      const found = spots(n, discs, tx, ty, reach).find(([, p]) => free(p));
      if (found) return found[1];
      if (reach >= 2 * Math.hypot(W, H)) return null;
    }
  }
  // No water left at all: the candidate spot where the island overlaps its neighbours least.
  function leastCrowded(n, settled, gap) {
    const [tx, ty] = inBounds(n.tx, n.ty), discs = obstacles(n, settled, gap);
    const overlap = p => {
      let worst = 0;
      discs.some(p[0], p[1], 1, c => { worst = Math.max(worst, (c.r - Math.hypot(p[0] - c.x, p[1] - c.y)) / c.r); return false; });
      return worst;
    };
    let best = [tx, ty], least = overlap(best);
    spots(n, discs, tx, ty, Infinity).forEach(([, p]) => {
      if (!inChart(p)) return;
      const o = overlap(p);
      if (o < least - 1e-9) { best = p; least = o; }
    });
    return best;
  }

  // ---------- territories ----------
  // members joined by a territory's links, as connected groups of two or more
  function groups(links) {
    const parent = new Map(), find = n => { while (parent.get(n) !== n) n = parent.get(n); return n; };
    links.forEach(([a, b]) => { [a, b].forEach(n => { if (!parent.has(n)) parent.set(n, n); }); parent.set(find(a), find(b)); });
    const out = new Map();
    parent.forEach((_, n) => { const r = find(n); if (!out.has(r)) out.set(r, []); out.get(r).push(n); });
    return [...out.values()].filter(g => g.length > 1);
  }

  // A publisher's feeds are chained nearest-first from the member closest to the harbour; a continent joins
  // its whole theme by a minimum spanning tree. Links longer than the land can stretch stay open sea.
  function territories(nodes, data) {
    const byId = new Map(nodes.map(n => [n.id, n]));
    const dist = (a, b) => Math.hypot(a.x - b.x, a.y - b.y);
    const result = (name, links, continent) => {
      if (!links.length) return null;
      return {name, continent, links, groups: groups(links), members: [...new Set(links.flat())]};
    };
    const families = data.territories.map(t => {
      const members = t.feeds.map(id => byId.get(id)).filter(Boolean);
      if (members.length < 2) return null;
      const left = members.slice(), fromPort = n => Math.hypot(n.x - PORT.x, n.y - PORT.y);
      const chain = [left.splice(left.reduce((bi, n, i) => (fromPort(n) < fromPort(left[bi]) ? i : bi), 0), 1)[0]];
      while (left.length) {
        const last = chain[chain.length - 1];
        const bi = left.reduce((best, n, i) => (dist(n, last) < dist(left[best], last) ? i : best), 0);
        chain.push(left.splice(bi, 1)[0]);
      }
      const links = chain.slice(1).map((n, i) => [chain[i], n]).filter(([a, b]) => dist(a, b) < MAXLINK * 1.2);
      return result(t.name, links, false);
    });
    const continents = data.continents.map(c => {
      const members = nodes.filter(n => n.theme === c.theme);
      if (members.length < 2) return null;
      const inTree = [members[0]], out = members.slice(1), links = [];
      while (out.length) {
        let best = null;
        inTree.forEach(a => out.forEach((b, j) => { const d = dist(a, b); if (!best || d < best[2]) best = [a, j, d]; }));
        const b = out.splice(best[1], 1)[0];
        if (best[2] < MAXLINK) links.push([best[0], b]);
        inTree.push(b);
      }
      return result(c.name, links, true);
    });
    return families.concat(continents).filter(Boolean);
  }

  // ---------- the height field: land ≥ .5, shallows below, open sea near 0 ----------
  function field(nodes, view, data, terr) {
    const harbour = checkView(view), D = d3();
    const grid = new Float32Array(GW * GH); // the highest bump wins, so each feed keeps its own coast
    function bump(x0, y0, r, seed) {
      const reach = r * 3.4, ox = (seed % 997) / 13, oy = (seed % 613) / 17;
      const gx0 = Math.max(0, Math.floor((x0 - reach) / CELL)), gx1 = Math.min(GW - 1, Math.ceil((x0 + reach) / CELL));
      const gy0 = Math.max(0, Math.floor((y0 - reach) / CELL)), gy1 = Math.min(GH - 1, Math.ceil((y0 + reach) / CELL));
      for (let gy = gy0; gy <= gy1; gy++) for (let gx = gx0; gx <= gx1; gx++) {
        const px = gx * CELL, py = gy * CELL;
        let d = Math.hypot(px - x0, py - y0) / r;
        d *= 1 + .95 * fbm(px * .011 + ox, py * .011 + oy) + .25 * fbm(px * .05 + oy, py * .05 + ox);
        const v = Math.exp(-d * d * .78), i = gy * GW + gx;
        if (v > grid[i]) grid[i] = v;
      }
    }
    // invented coasts: each joined group of a territory becomes one landmass, its outline the group's hull,
    // pushed outwards and frayed by noise into bays and capes
    function landmass(members, seed, wide) {
      const pts = members.map(n => [n.x, n.y]);
      const hull = pts.length > 2 ? D.polygonHull(pts) : null, poly = hull || pts;
      const avgR = members.reduce((s, n) => s + n.r, 0) / members.length;
      const pad = (wide ? 12 : 8) + .55 * avgR, amp = wide ? 20 : 13, ox = (seed % 997) / 9, oy = (seed % 389) / 7;
      const xs = poly.map(p => p[0]), ys = poly.map(p => p[1]), reach = pad + amp * 1.6;
      const gx0 = Math.max(0, Math.floor((Math.min(...xs) - reach) / CELL)), gx1 = Math.min(GW - 1, Math.ceil((Math.max(...xs) + reach) / CELL));
      const gy0 = Math.max(0, Math.floor((Math.min(...ys) - reach) / CELL)), gy1 = Math.min(GH - 1, Math.ceil((Math.max(...ys) + reach) / CELL));
      const edges = poly.map((p, i) => [p, poly[(i + 1) % poly.length]]);
      for (let gy = gy0; gy <= gy1; gy++) for (let gx = gx0; gx <= gx1; gx++) {
        const px = gx * CELL, py = gy * CELL;
        let d = Infinity;
        edges.forEach(([p, q]) => {
          const dx = q[0] - p[0], dy = q[1] - p[1], l2 = dx * dx + dy * dy || 1;
          const u = Math.max(0, Math.min(1, ((px - p[0]) * dx + (py - p[1]) * dy) / l2));
          d = Math.min(d, Math.hypot(px - p[0] - u * dx, py - p[1] - u * dy));
        });
        const inside = hull ? D.polygonContains(hull, [px, py]) : false;
        const sd = (inside ? d : -d) + pad + amp * (fbm(px * .007 + ox, py * .007 + oy) + .45 * fbm(px * .03 + oy, py * .03 - ox));
        const v = sd > 0 ? .5 + .27 * Math.tanh(sd / 26) : .5 * Math.exp(-(sd / 38) * (sd / 38)); // shallows fade offshore
        const i = gy * GW + gx;
        if (v > grid[i]) grid[i] = v;
      }
    }

    nodes.forEach(n => bump(n.x, n.y, n.r, n.seed));
    if (harbour) bump(PORT.x, PORT.y, 13, 77);
    (terr || territories(nodes, data)).forEach(t => t.groups.forEach(g => landmass(g, hash(t.name), t.continent)));
    for (let gy = 0; gy < GH; gy++) for (let gx = 0; gx < GW; gx++) {
      const i = gy * GW + gx;
      grid[i] = Math.max(grid[i], .02 + .02 * fbm(gx * CELL * .004, gy * CELL * .004));
    }
    const at = (x, y) => {
      const gx = Math.min(GW - 1, Math.max(0, Math.round(x / CELL))), gy = Math.min(GH - 1, Math.max(0, Math.round(y / CELL)));
      return grid[gy * GW + gx];
    };
    return {grid, at};
  }

  // ---------- labels, placed by priority so nothing writes over anything else ----------
  // Returns placements {kind, text, x, y, angle, size, font, track, box, id?, key?, leader?}: text centred on
  // (x, y), rotated by angle, letter-spaced by track; box is the axis-aligned box it reserves. `measure(text,
  // font)` gives a text's width in chart units (a canvas in the page). `geo` may carry the field and the
  // territories already computed for the drawing. A feed whose name finds no room shows it on hover.
  function labels(nodes, data, view, measure, geo) {
    const harbour = checkView(view);
    const terr = (geo && geo.territories) || territories(nodes, data);
    const ground = (geo && geo.field) || field(nodes, view, data, terr);
    const out = [];
    const boxes = [
      [CARTOUCHE.x, CARTOUCHE.y, CARTOUCHE.x + CARTOUCHE.w, CARTOUCHE.y + CARTOUCHE.h],
      [COMPASS.x - COMPASS.r - 12, COMPASS.y - COMPASS.r - 12, COMPASS.x + COMPASS.r + 12, COMPASS.y + COMPASS.r + 12],
    ];
    const meet = (a, b) => a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1];
    const outside = b => b[0] < 40 || b[2] > W - 40 || b[1] < 40 || b[3] > H - 40;
    const hit = b => outside(b) || boxes.some(o => meet(b, o));
    const islandBox = n => [n.x - n.r * .85, n.y - n.r * .75, n.x + n.r * .85, n.y + n.r * .75];
    const overIsland = (b, self) => nodes.some(n => n !== self && meet(b, islandBox(n)));
    const onLand = b => {
      for (let i = 0; i <= 6; i++) for (let j = 0; j <= 2; j++) {
        if (ground.at(b[0] + (b[2] - b[0]) * i / 6, b[1] + (b[3] - b[1]) * j / 2) >= .45) return true;
      }
      return false;
    };
    const spacedWidth = (text, f, track) => [...text].reduce((w, c) => w + measure(c, f) + track, -track);
    const box = (x, y, w, h) => [x - w / 2 - 3, y - h / 2 - 2, x + w / 2 + 3, y + h / 2 + 2];
    const put = p => { out.push({angle: 0, track: 0, ...p}); boxes.push(p.box); };

    // the harbour first: it never moves (the two lines sit 18 apart, so their boxes do not touch)
    if (harbour) {
      const name = font(15, FONTS.display, "600"), today = font(12, FONTS.body, "italic 500");
      put({kind: "harbour", text: "Ressac Harbour", x: PORT.x, y: PORT.y + 27, size: 15, font: name,
        box: box(PORT.x, PORT.y + 27, measure("Ressac Harbour", name), 15)});
      put({kind: "harbour", text: "today", x: PORT.x, y: PORT.y + 45, size: 12, font: today,
        box: box(PORT.x, PORT.y + 45, measure("today", today), 12)});
      boxes.push([PORT.x - 12, PORT.y - 18, PORT.x + 26, PORT.y + 12]); // the harbour's star and light flare
    }

    // continents: large spaced capitals over their own land, nudged up or down if another name is there
    terr.filter(t => t.continent).forEach(t => {
      const cx = t.members.reduce((s, n) => s + n.x, 0) / t.members.length, cy = t.members.reduce((s, n) => s + n.y, 0) / t.members.length;
      const f = font(21, FONTS.display, "600"), w = spacedWidth(t.name, f, 7);
      for (const dy of [0, -26, 26, -52, 52]) {
        const b = box(cx, cy + dy, w, 22);
        if (!hit(b)) { put({kind: "continent", text: t.name, x: cx, y: cy + dy, size: 21, font: f, track: 7, box: b}); break; }
      }
    });

    // seas: on their bearing, or above and below their archipelago, in the most open water found there
    seas(data).filter(s => s.count > 0).forEach(s => {
      const n = s.count, track = n > 8 ? 7 : 4, size = n > 20 ? 26 : n > 8 ? 21 : 17, h = n > 20 ? 26 : 21;
      const f = font(size, FONTS.body, "italic 500"), w = spacedWidth(s.sea, f, track), spots = [];
      if (harbour) {
        const mid = s.from + s.span / 2;
        [1.06, 1, .94, 1.12, .88, .82].forEach(k => [0, .05, -.05].forEach(da =>
          spots.push([PORT.x + Math.cos(mid + da) * RX * k, PORT.y + Math.sin(mid + da) * RY * k * 1.04])));
      } else [-1, 1].forEach(side => [40, 60, 80].forEach(g => spots.push([s.x, s.y + side * (s.spread + g)])));
      const ok = spots.map(([x, y]) => [Math.max(w / 2 + 50, Math.min(W - w / 2 - 50, x)), y])
        .map(([x, y]) => [x, y, box(x, y, w, h)]).filter(([, , b]) => !hit(b));
      const best = ok.find(([, , b]) => !overIsland(b) && !onLand(b)) || ok.find(([, , b]) => !onLand(b)) || ok[0]; // water names stay on the water
      if (best) put({kind: "sea", key: s.key, text: s.sea, x: best[0], y: best[1], size, font: f, track, box: best[2]});
    });

    // territories: small capitals laid along their land, on the longest link that has room
    terr.filter(t => !t.continent).forEach(t => {
      const f = font(12.5, FONTS.display, "600"), w = spacedWidth(t.name, f, 3.2), h = 13;
      const length = ([a, b]) => Math.hypot(b.x - a.x, b.y - a.y);
      for (const [a, b] of t.links.slice().sort((p, q) => length(q) - length(p))) {
        let ang = Math.atan2(b.y - a.y, b.x - a.x);
        if (ang > Math.PI / 2) ang -= Math.PI;
        if (ang < -Math.PI / 2) ang += Math.PI;
        const mx = (a.x + b.x) / 2, my = (a.y + b.y) / 2, c = Math.abs(Math.cos(ang)), sn = Math.abs(Math.sin(ang));
        const hx = c * w / 2 + sn * h / 2, hy = sn * w / 2 + c * h / 2, bb = [mx - hx, my - hy, mx + hx, my + hy];
        if (hit(bb)) continue;
        put({kind: "territory", text: t.name, x: mx, y: my, angle: ang, size: 12.5, font: f, track: 3.2, box: bb});
        break;
      }
    });

    // feed names, busiest first (activity, then unread this week, then rhythm): eight spots round the island,
    // then a smaller size, then further out on a leader line; silent feeds get "ED" (existence doubtful)
    const rank = {daily: 0, weekly: 1, monthly: 2, rare: 3, silent: 4};
    const order = n => rank[n.group] ?? 4;
    const busier = (a, b) => (b.activity - a.activity) || (b.unread - a.unread) || (order(a) - order(b));
    nodes.slice().sort(busier).forEach(n => {
      if (n.group === "silent") {
        const f = font(9, FONTS.mono), y = n.y + n.r * .9 + 9, b = box(n.x, y, measure("ED", f), 9);
        if (!hit(b) && !overIsland(b, n)) put({kind: "ed", id: n.id, text: "ED", x: n.x, y, size: 9, font: f, box: b});
        return;
      }
      const name = [...n.title.trim()], text = name.length > 26 ? name.slice(0, 25).join("") + "…" : name.join("");
      if (!text) return;
      for (const [px, weight] of n.group === "daily" ? [[14, "600"], [11.5, "600"]] : [[12.5, "500"], [10.5, "500"]]) {
        const f = font(px, FONTS.body, weight), w = measure(text, f), r = n.r;
        const spots = [[0, r * .85 + px * .9], [0, -r * .85 - px * .7], [r + 5 + w / 2, 0], [-r - 5 - w / 2, 0],
          [r * .6 + w / 2, r * .7 + px * .8], [-r * .6 - w / 2, r * .7 + px * .8], [r * .6 + w / 2, -r * .7 - px * .6], [-r * .6 - w / 2, -r * .7 - px * .6]];
        for (const [dx, dy] of spots) {
          const x = n.x + dx, y = n.y + dy, b = box(x, y, w, px);
          if (!hit(b) && !overIsland(b, n)) { put({kind: "feed", id: n.id, text, x, y, size: px, font: f, box: b}); return; }
        }
      }
      const f = font(11, FONTS.body, "500"), w = measure(text, f);
      for (const dist of [n.r + 34, n.r + 56, n.r + 80, n.r + 108]) for (let i = 0; i < 16; i++) {
        const a = n.sa + i * Math.PI / 8, cos = Math.cos(a), sin = Math.sin(a);
        const lx = n.x + cos * (dist + w / 2 * Math.abs(cos)), ly = n.y + sin * dist * .8, b = box(lx, ly, w, 11);
        if (hit(b) || overIsland(b, n)) continue;
        const leader = {x1: n.x + cos * n.r * .9, y1: n.y + sin * n.r * .75, x2: lx - cos * (w / 2 * Math.abs(cos) + 3), y2: ly - sin * 8};
        put({kind: "feed", id: n.id, text, x: lx, y: ly, size: 11, font: f, box: b, leader});
        return;
      }
    });

    // soundings: unread entries of the week, in italic figures wherever there is water round the island
    const sounding = font(14, FONTS.body, "italic 500");
    nodes.filter(n => n.unread > 0).forEach(n => {
      const text = String(n.unread), w = measure(text, sounding);
      for (let i = 0; i < 8; i++) {
        const a = n.sa + i * Math.PI / 4, d = n.r * 1.3 + 12, x = n.x + Math.cos(a) * d, y = n.y + Math.sin(a) * d * .85, b = box(x, y, w, 13);
        if (!hit(b) && !overIsland(b, n)) { put({kind: "sounding", id: n.id, text, x, y, size: 14, font: sounding, box: b}); break; }
      }
    });

    // range rings last: their figures only where nothing else stands
    if (harbour) {
      const f = font(13, FONTS.body, "italic 500");
      RINGS.forEach(({days, label}) => {
        const y = PORT.y - RY * ringFrac(days) - 2, b = box(PORT.x + 6, y, measure(label, f), 13);
        if (!hit(b) && !overIsland(b)) put({kind: "ring", text: label, x: PORT.x + 6, y, size: 13, font: f, box: b});
      });
    }
    return out;
  }

  // ---------- activity light ----------
  // 0 for a quiet feed, 1 for the busiest one, on a log scale (the chart's busiest feed counts as at least 1)
  const heatLevel = (activity, maxActivity) =>
    Math.min(1, Math.log1p(Math.max(0, activity || 0)) / Math.log1p(Math.max(1, maxActivity || 0)));
  const ramps = new Map();
  function heat(activity, maxActivity, ink) {
    const name = Object.prototype.hasOwnProperty.call(INKS, ink) ? ink : "bleu";
    if (!ramps.has(name)) ramps.set(name, d3().interpolateRgbBasis(INKS[name].heat));
    return ramps.get(name)(heatLevel(activity, maxActivity));
  }

  return Object.freeze({
    useD3,
    W, H, CELL, GW, GH, PORT, RX, RY, RINGS, RADIUS, ISOBATHS, CARTOUCHE, COMPASS, UNCHARTED, FONTS, INKS,
    font, hash, mulberry, fbm, ringFrac,
    seas, place, territories, field, labels, heatLevel, heat,
  });
});
