// The sea chart's page: canvas drawing of what chart-core.js computes, the animated overlay, hover, popover,
// floating toolbar, key, and the bridge to the app. The builders and the drawing take their document and
// canvas context as arguments, so Node tests them with stand-ins; only createPage touches the real page.
// Feed-supplied strings reach the page through canvas text and textContent only, never as HTML.
(function (root, factory) {
  "use strict";
  if (typeof module === "object" && module.exports) {
    const d3 = require("./d3.min.js"), core = require("./chart-core.js");
    core.useD3(d3);
    module.exports = factory(core, d3);
  } else {
    const api = factory(root.ChartCore, root.d3);
    root.ChartDraw = api;
    root.Chart = Object.freeze({load: api.load, update: api.update});
  }
})(typeof globalThis !== "undefined" ? globalThis : this, function (core, d3) {
  "use strict";

  const {W, H, CELL, GW, GH, PORT, RX, RY, RINGS, ISOBATHS, CARTOUCHE, COMPASS, FONTS, INKS, font, mulberry, fbm, ringFrac} = core;
  const EMPTY_MESSAGE = "No feed has published in six months";
  const NOTE = "Click the name to open this feed in Ressac.";

  // ---------- data and preferences ----------
  const VIEWS = ["harbour", "themes"], INK_NAMES = Object.keys(INKS);
  function normalisePrefs(raw) {
    const p = raw && typeof raw === "object" ? raw : {};
    return {
      view: VIEWS.includes(p.view) ? p.view : "harbour",
      ink: INK_NAMES.includes(p.ink) ? p.ink : "bleu",
      animate: typeof p.animate === "boolean" ? p.animate : true,
    };
  }
  const prefsMessage = p => ({type: "prefs", view: p.view, ink: p.ink, animate: p.animate});

  // the chart document (§6), as a JSON string or an object; the lists default to empty
  function parseData(json) {
    const data = typeof json === "string" ? JSON.parse(json) : json;
    if (!data || typeof data !== "object" || data.version !== 1) {
      throw new TypeError(`unsupported chart version: ${data && data.version}`);
    }
    const list = v => (Array.isArray(v) ? v : []);
    return {...data, themes: list(data.themes), feeds: list(data.feeds), territories: list(data.territories), continents: list(data.continents)};
  }

  // the app's message handler, or null outside Ressac (the preview page)
  function bridge(win) {
    const handler = win && win.webkit && win.webkit.messageHandlers && win.webkit.messageHandlers.ressac;
    return handler && typeof handler.postMessage === "function" ? message => handler.postMessage(message) : null;
  }

  // ---------- words ----------
  const DAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
  const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
  const pad = n => String(n).padStart(2, "0");
  // "Monday 19 October 2026, 17:40", in local time
  function formatSurvey(iso) {
    const d = new Date(typeof iso === "string" ? iso : NaN);
    if (Number.isNaN(d.getTime())) return "";
    return `${DAYS[d.getDay()]} ${d.getDate()} ${MONTHS[d.getMonth()]} ${d.getFullYear()}, ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  }
  // an entry's published day "2026-10-13" as "13 Oct 2026" (the day is already local to the survey)
  function dayLabel(day) {
    const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(day || "");
    return m ? `${Number(m[3])} ${MONTHS[Number(m[2]) - 1].slice(0, 3)} ${m[1]}` : "";
  }
  const GROUP = {daily: "Daily", weekly: "Weekly", monthly: "Monthly", rare: "Rare", silent: "Silent"};
  const since = a => (a == null ? "no entries" : a < 1 ? "last entry today" : a < 30 ? `last entry ${Math.round(a)} d ago`
    : a < 365 ? `last entry ${Math.round(a / 30)} mo ago` : `last entry ${Math.round(a / 365)} yr ago`);
  const plural = (n, one, many) => `${n} ${n === 1 ? one : many}`;
  const titleOf = n => (n.title || "").trim() || "(untitled)";
  const isWeb = url => typeof url === "string" && /^https?:\/\//i.test(url);

  function facts(node, data) {
    const feed = node.feed || {}, theme = data.themes.find(t => t.key === node.theme);
    return [theme ? theme.name : core.UNCHARTED.name, feed.folder ? `folder ${feed.folder}` : null,
      GROUP[node.group] || node.group, since(node.age), `${node.unread} unread`, feed.page != null ? `page ${feed.page}` : null]
      .filter(Boolean).join(" · ");
  }

  // ---------- hover and popover contents (DOM nodes, text through textContent only) ----------
  function make(doc, tag, className, text) {
    const el = doc.createElement(tag);
    if (className) el.className = className;
    if (text != null) el.textContent = text;
    return el;
  }
  function tooltipContent(doc, node) {
    const counts = `${node.unread ? `${node.unread} unread` : "nothing unread"} · ${plural(node.week, "entry", "entries")} this week`;
    return [make(doc, "b", null, titleOf(node)), make(doc, "span", null, counts)];
  }
  // [head, entries, facts, note]; `on` = {openFeed(id), openURL(url, event), close()}, each optional
  function popoverContent(doc, node, data, on) {
    const head = make(doc, "div", "pop-head"), h = make(doc, "h3");
    h.id = "pop-title";
    const name = make(doc, "button", "pop-title", titleOf(node));
    name.type = "button";
    name.addEventListener("click", () => on.openFeed && on.openFeed(node.id));
    h.append(name);
    const close = make(doc, "button", "pop-close", "×");
    close.type = "button";
    close.setAttribute("aria-label", "Close");
    close.addEventListener("click", () => on.close && on.close());
    head.append(h, close);

    const list = make(doc, "ol", "pop-entries"), entries = (node.feed && node.feed.entries) || [];
    entries.forEach(entry => {
      const li = make(doc, "li", entry.read ? "" : "unread"), when = make(doc, "time", null, dayLabel(entry.day));
      when.dateTime = entry.day;
      const title = (entry.title || "").trim() || "(untitled)", web = isWeb(entry.url);
      const t = make(doc, web ? "a" : "span", null, title);
      if (web) {
        t.href = entry.url;
        t.addEventListener("click", e => on.openURL && on.openURL(entry.url, e));
      }
      li.append(when, t);
      list.append(li);
    });
    if (!entries.length) list.append(make(doc, "li", null, "No entries yet."));
    return [head, list, make(doc, "p", "pop-meta", facts(node, data)), make(doc, "p", "pop-foot", NOTE)];
  }

  // ---------- the scene: everything the drawing needs, computed once per document and view ----------
  function buildScene(data, view, measure) {
    const v = data.feeds.length ? view : "harbour"; // an empty sea still has its harbour
    const nodes = core.place(data, v), terr = core.territories(nodes, data), ground = core.field(nodes, v, data, terr);
    const placements = core.labels(nodes, data, v, measure, {field: ground, territories: terr});
    const {strokes, currents} = seaStrokes(ground.at);
    const treasure = nodes.reduce((a, b) => (b.unread > (a ? a.unread : 0) ? b : a), null);
    return {
      data, view: v, empty: !nodes.length, nodes, ground, placements, strokes, currents, treasure,
      contours: d3.contours().size([GW, GH]).thresholds(ISOBATHS)(ground.grid),
      seas: core.seas(data).filter(s => s.count > 0),
      maxActivity: nodes.reduce((m, n) => Math.max(m, n.activity), 0),
      fresh: nodes.filter(n => n.unread > 0 && n.group !== "silent"),
    };
  }

  // Open water as Marine Le Breton draws it: short pen strokes following the wind out at sea, bending to run
  // parallel to the coast in the shallows; an occupancy grid keeps them evenly spaced. `currents` are the
  // longer strokes clear of the compass and the cartouche, which the gusts run along.
  function seaStrokes(at) {
    const OC = 6, OW = Math.ceil(W / OC), occ = new Int32Array(OW * Math.ceil(H / OC));
    const slope = (x, y) => [at(x + CELL, y) - at(x - CELL, y), at(x, y + CELL) - at(x, y - CELL)];
    const heading = (x, y, prev) => {
      const v = at(x, y), wind = fbm(x * .0022 + 3, y * .0022 - 5) * Math.PI * 2.4 + .7 * Math.sin(y * .004 + x * .001);
      let dx = Math.cos(wind), dy = Math.sin(wind);
      const [gx, gy] = slope(x, y), gl = Math.hypot(gx, gy), near = Math.min(1, Math.max(0, (v - .05) / .18));
      if (gl > 1e-4 && near > 0) {
        let tx = -gy / gl, ty = gx / gl;
        if (tx * dx + ty * dy < 0) { tx = -tx; ty = -ty; }
        dx = dx * (1 - near) + tx * near; dy = dy * (1 - near) + ty * near;
      }
      if (prev && dx * prev[0] + dy * prev[1] < 0) { dx = -dx; dy = -dy; }
      const l = Math.hypot(dx, dy) || 1;
      return [dx / l, dy / l];
    };
    const clearOfFrames = ([x, y]) => !((x > CARTOUCHE.x - 10 && y > CARTOUCHE.y - 10) || (x < 250 && y < 255));
    const strokes = [], currents = [], rnd = mulberry(909);
    let id = 0;
    for (let sy = 44; sy < H - 44; sy += 10) for (let sx = 44; sx < W - 44; sx += 10) {
      let x = sx + (rnd() - .5) * 8, y = sy + (rnd() - .5) * 8;
      if (at(x, y) > .3 || occ[Math.floor(y / OC) * OW + Math.floor(x / OC)]) continue;
      id++;
      const pts = [[x, y]], len = 5 + Math.floor(rnd() * 7);
      let prev = null, steps = 0;
      for (; steps < len; steps++) {
        const d = heading(x, y, prev); prev = d; x += d[0] * 3; y += d[1] * 3;
        const c = Math.floor(y / OC) * OW + Math.floor(x / OC);
        if (x < 40 || x > W - 40 || y < 40 || y > H - 40 || at(x, y) > .34 || (occ[c] && occ[c] !== id)) break;
        occ[c] = id; pts.push([x, y]);
      }
      if (steps > 2) {
        strokes.push(pts);
        if (pts.length > 4 && pts.every(clearOfFrames)) currents.push(pts);
      }
    }
    return {strokes, currents};
  }

  // ---------- drawing on the base canvas, in chart units (the caller sets the transform) ----------
  function halo(ctx, ink, text, x, y) {
    ctx.save(); ctx.strokeStyle = ink.paper; ctx.lineWidth = 4; ctx.lineJoin = "round"; ctx.globalAlpha = .85;
    ctx.strokeText(text, x, y); ctx.restore();
    ctx.fillText(text, x, y);
  }
  function spaced(ctx, ink, text, x, y, track) {
    const chars = [...text], widths = chars.map(c => ctx.measureText(c).width);
    let cx = x - (widths.reduce((s, w) => s + w + track, 0) - track) / 2;
    chars.forEach((c, i) => { halo(ctx, ink, c, cx + widths[i] / 2, y); cx += widths[i] + track; });
  }

  function drawChart(ctx, scene, inkName) {
    const ink = INKS[inkName] || INKS.bleu, harbour = scene.view === "harbour";
    if (ink.parchment) parchment(ctx);
    if (harbour) rangeLines(ctx, scene, ink);
    stipple(ctx, scene, ink);
    ctx.strokeStyle = ink.dot; ctx.lineWidth = .6; ctx.lineCap = "round"; ctx.globalAlpha = .5;
    scene.strokes.forEach(pts => {
      ctx.beginPath(); ctx.moveTo(pts[0][0], pts[0][1]);
      for (let i = 1; i < pts.length; i++) ctx.lineTo(pts[i][0], pts[i][1]);
      ctx.stroke();
    });
    ctx.globalAlpha = 1;
    coasts(ctx, scene, ink);
    if (harbour) portSymbol(ctx, ink, PORT.x, PORT.y, 1);
    drawLabels(ctx, scene, ink);
    if (scene.empty) {
      ctx.fillStyle = ink.label; ctx.textAlign = "center"; ctx.textBaseline = "middle"; ctx.font = font(24, FONTS.body, "italic 500");
      halo(ctx, ink, EMPTY_MESSAGE, PORT.x, PORT.y + 130);
    }
    if (ink.treasure && scene.treasure) treasure(ctx, ink, scene.treasure);
    compass(ctx, ink);
    neatline(ctx, ink);
    cartouche(ctx, scene, ink);
  }

  // range rings and the sectors' leading lines, under everything
  function rangeLines(ctx, scene, ink) {
    ctx.strokeStyle = ink.ink; ctx.lineWidth = .7;
    RINGS.forEach(({days}) => {
      const f = ringFrac(days);
      ctx.globalAlpha = .45; ctx.setLineDash([2, 6]);
      ctx.beginPath(); ctx.ellipse(PORT.x, PORT.y, RX * f, RY * f, 0, 0, Math.PI * 2); ctx.stroke();
    });
    ctx.setLineDash([10, 8]); ctx.globalAlpha = .25;
    if (scene.seas.length > 1) scene.seas.forEach(s => {
      ctx.beginPath(); ctx.moveTo(PORT.x + Math.cos(s.from) * 40, PORT.y + Math.sin(s.from) * 26);
      ctx.lineTo(PORT.x + Math.cos(s.from) * RX * 1.15, PORT.y + Math.sin(s.from) * RY * 1.15); ctx.stroke();
    });
    ctx.setLineDash([]); ctx.globalAlpha = 1;
  }

  // stippled shallows, denser near the coasts, as on an engraved chart
  function stipple(ctx, scene, ink) {
    const rnd = mulberry(4242), at = scene.ground.at, onLand = ink === INKS.pop ? 0 : .035;
    ctx.fillStyle = ink.dot;
    for (let i = 0; i < 520000; i++) {
      const x = rnd() * W, y = rnd() * H, v = at(x, y);
      const p = v >= .5 ? onLand : Math.pow(Math.max(0, (v - .045) / .455), 1.35) * .9;
      if (rnd() < p) ctx.fillRect(x, y, .85, .85);
    }
  }

  // land, then isobaths and the coast
  function coasts(ctx, scene, ink) {
    const path = d3.geoPath(d3.geoIdentity().scale(CELL), ctx);
    scene.contours.forEach(c => { if (c.value === .5) { ctx.beginPath(); path(c); ctx.fillStyle = ink.land; ctx.fill(); } });
    scene.contours.forEach(c => {
      const v = c.value;
      ctx.beginPath(); path(c); ctx.strokeStyle = ink.ink;
      if (v === .5) { ctx.lineWidth = 1.5; ctx.setLineDash([]); ctx.globalAlpha = 1; }
      else if (v > .5) { ctx.lineWidth = .55; ctx.setLineDash([]); ctx.globalAlpha = .55; }
      else if (v === .37) { ctx.lineWidth = .6; ctx.setLineDash([]); ctx.globalAlpha = .7; }
      else if (v === .25) { ctx.lineWidth = .6; ctx.setLineDash([5, 3]); ctx.globalAlpha = .6; }
      else if (v === .15) { ctx.lineWidth = .5; ctx.setLineDash([1.2, 3]); ctx.globalAlpha = .55; }
      else { ctx.lineWidth = .45; ctx.setLineDash([8, 6]); ctx.globalAlpha = .28; }
      ctx.stroke();
    });
    ctx.setLineDash([]); ctx.globalAlpha = 1;
  }

  // the placements chart-core chose, in its priority order
  const LABEL_ALPHA = {continent: .78, sea: .82, ed: .7};
  function drawLabels(ctx, scene, ink) {
    ctx.textAlign = "center"; ctx.textBaseline = "middle";
    scene.placements.forEach(p => {
      ctx.font = p.font; ctx.fillStyle = ink.label;
      if (p.leader) {
        ctx.save(); ctx.strokeStyle = ink.ink; ctx.globalAlpha = .55; ctx.lineWidth = .6;
        ctx.beginPath(); ctx.moveTo(p.leader.x1, p.leader.y1); ctx.lineTo(p.leader.x2, p.leader.y2); ctx.stroke(); ctx.restore();
      }
      ctx.globalAlpha = LABEL_ALPHA[p.kind] ?? 1;
      if (p.kind === "ed") ctx.fillText(p.text, p.x, p.y);
      else if (p.angle) { ctx.save(); ctx.translate(p.x, p.y); ctx.rotate(p.angle); spaced(ctx, ink, p.text, 0, 0, p.track); ctx.restore(); }
      else if (p.track) spaced(ctx, ink, p.text, p.x, p.y, p.track);
      else halo(ctx, ink, p.text, p.x, p.y);
      ctx.globalAlpha = 1;
    });
  }

  // pirate ink: a sun-browned sheet, darker at the edges, with a few stains
  function parchment(ctx) {
    const g = ctx.createRadialGradient(W / 2, H / 2, H * .35, W / 2, H / 2, W * .62);
    g.addColorStop(0, "rgba(120,72,24,0)"); g.addColorStop(1, "rgba(120,72,24,.38)");
    ctx.fillStyle = g; ctx.fillRect(0, 0, W, H);
    const r = mulberry(1717);
    for (let i = 0; i < 9; i++) {
      const x = r() * W, y = r() * H, rad = 40 + r() * 140, s = ctx.createRadialGradient(x, y, 0, x, y, rad);
      s.addColorStop(0, "rgba(120,72,24,.10)"); s.addColorStop(.7, "rgba(120,72,24,.05)"); s.addColorStop(1, "rgba(120,72,24,0)");
      ctx.fillStyle = s; ctx.beginPath(); ctx.arc(x, y, rad, 0, Math.PI * 2); ctx.fill();
    }
  }
  // X marks the island with the most unread entries
  function treasure(ctx, ink, n) {
    const s = 13;
    ctx.save(); ctx.strokeStyle = ink.treasure; ctx.lineWidth = 5; ctx.lineCap = "round";
    ctx.beginPath(); ctx.moveTo(n.x - s, n.y - s); ctx.lineTo(n.x + s, n.y + s); ctx.moveTo(n.x + s, n.y - s); ctx.lineTo(n.x - s, n.y + s); ctx.stroke();
    ctx.fillStyle = ink.treasure; ctx.font = font(16, FONTS.body, "italic 600"); ctx.textAlign = "left"; ctx.textBaseline = "middle";
    halo(ctx, ink, `X marks ${n.unread} unread`, n.x + s + 8, n.y - s - 6);
    ctx.restore();
  }
  // the harbour's star, with the light flare of nautical charts drawn as an outline so it does not read as a shadow
  function portSymbol(ctx, ink, x, y, s) {
    ctx.save(); ctx.translate(x, y); ctx.scale(s, s);
    ctx.fillStyle = ink.ink; ctx.beginPath();
    for (let i = 0; i < 10; i++) { const a = -Math.PI / 2 + i * Math.PI / 5, rr = i % 2 ? 3 : 8; ctx.lineTo(Math.cos(a) * rr, Math.sin(a) * rr); }
    ctx.closePath(); ctx.fill();
    ctx.strokeStyle = ink.ink; ctx.lineWidth = .9; ctx.beginPath(); ctx.moveTo(4, -3);
    ctx.bezierCurveTo(10, -12, 19, -14, 21, -7); ctx.bezierCurveTo(17, -3, 10, -2, 4, -3); ctx.stroke();
    ctx.restore();
  }
  function compass(ctx, ink) {
    const r = COMPASS.r;
    ctx.save(); ctx.translate(COMPASS.x, COMPASS.y); ctx.strokeStyle = ink.ink; ctx.fillStyle = ink.label;
    ctx.lineWidth = .7; ctx.beginPath(); ctx.arc(0, 0, r, 0, Math.PI * 2); ctx.stroke();
    ctx.beginPath(); ctx.arc(0, 0, r - 9, 0, Math.PI * 2); ctx.stroke();
    for (let deg = 0; deg < 360; deg += 5) {
      const a = deg * Math.PI / 180 - Math.PI / 2, l = deg % 30 === 0 ? 9 : deg % 10 === 0 ? 6 : 3;
      ctx.beginPath(); ctx.moveTo(Math.cos(a) * r, Math.sin(a) * r); ctx.lineTo(Math.cos(a) * (r - l), Math.sin(a) * (r - l)); ctx.stroke();
    }
    ctx.lineWidth = .5;
    for (let i = 0; i < 32; i++) {
      const a = i * Math.PI / 16;
      ctx.beginPath(); ctx.moveTo(0, 0); ctx.lineTo(Math.cos(a) * (r - 12), Math.sin(a) * (r - 12));
      ctx.globalAlpha = i % 4 ? .35 : .9; ctx.stroke();
    }
    ctx.globalAlpha = 1;
    for (let i = 0; i < 4; i++) {
      ctx.save(); ctx.rotate(i * Math.PI / 2);
      ctx.beginPath(); ctx.moveTo(0, -(r - 14)); ctx.lineTo(6, 0); ctx.lineTo(0, 0); ctx.closePath(); ctx.fill();
      ctx.beginPath(); ctx.moveTo(0, -(r - 14)); ctx.lineTo(-6, 0); ctx.lineTo(0, 0); ctx.closePath(); ctx.stroke();
      ctx.restore();
    }
    ctx.font = font(13, FONTS.display, "600"); ctx.textAlign = "center"; ctx.textBaseline = "middle"; ctx.fillText("N", 0, -r - 12);
    ctx.restore();
  }
  function neatline(ctx, ink) {
    ctx.strokeStyle = ink.ink; ctx.lineWidth = 1.2; ctx.strokeRect(22, 22, W - 44, H - 44);
    ctx.lineWidth = .6; ctx.strokeRect(34, 34, W - 68, H - 68);
    ctx.fillStyle = ink.ink;
    for (let x = 34, i = 0; x < W - 34; x += 40, i++) if (i % 2 === 0) { ctx.fillRect(x, 26, 40, 5); ctx.fillRect(x, H - 31, 40, 5); }
    for (let y = 34, i = 0; y < H - 34; y += 40, i++) if (i % 2 === 0) { ctx.fillRect(26, y, 5, 40); ctx.fillRect(W - 31, y, 5, 40); }
    ctx.font = font(10, FONTS.mono); ctx.fillStyle = ink.label; ctx.textAlign = "center"; ctx.textBaseline = "middle";
    for (let x = 34 + 160, j = 1; x < W - 60; x += 320, j++) ctx.fillText(`${j * 8}′`, x, 13);
  }
  // a line of the cartouche, at its size or smaller if it would run past the frame
  function fitted(ctx, text, x, y, px, family, style, maxWidth) {
    let size = px;
    ctx.font = font(size, family, style);
    while (size > 9 && ctx.measureText(text).width > maxWidth) { size -= .5; ctx.font = font(size, family, style); }
    ctx.fillText(text, x, y);
  }
  function cartouche(ctx, scene, ink) {
    const {x, y, w, h} = CARTOUCHE, cx = x + w / 2, inner = w - 28, totals = scene.data.totals || {};
    ctx.fillStyle = ink.paper; ctx.fillRect(x, y, w, h);
    ctx.strokeStyle = ink.ink; ctx.lineWidth = 1; ctx.strokeRect(x, y, w, h); ctx.lineWidth = .5; ctx.strokeRect(x + 5, y + 5, w - 10, h - 10);
    ctx.fillStyle = ink.label; ctx.textAlign = "center"; ctx.textBaseline = "alphabetic";
    ctx.font = font(28, FONTS.display, "600"); spaced(ctx, ink, "RESSAC", cx, y + 38, 8);
    ctx.font = font(15, FONTS.display, "500"); spaced(ctx, ink, "Chart of the feeds", cx, y + 57, 1.5);
    const surveyed = formatSurvey(scene.data.surveyed_at);
    if (surveyed) fitted(ctx, `Surveyed ${surveyed}`, cx, y + 78, 16, FONTS.body, "italic 500", inner);
    fitted(ctx, `${plural(totals.feeds ?? scene.nodes.length, "feed", "feeds")} · ${totals.unread_week ?? 0} unread this week`,
      cx, y + 97, 11, FONTS.mono, null, inner);
    fitted(ctx, scene.view === "harbour" ? "Distance from the harbour: time since the last entry" : "One archipelago per theme",
      cx, y + 115, 13, FONTS.body, "italic 500", inner);
  }

  // ---------- the animated overlay: activity lights, gusts, the harbour light, ripples ----------
  // `motion` is null when still; otherwise {t, gusts} and the returned array is the gusts to keep
  function drawOver(ctx, scene, inkName, motion) {
    const ink = INKS[inkName] || INKS.bleu, t = motion ? motion.t : 0;
    let gusts = [];
    if (motion) gusts = drawGusts(ctx, scene, ink, t, motion.gusts);
    drawLights(ctx, scene, inkName, motion ? t : null);
    if (!motion) return gusts;
    if (scene.view === "harbour") {
      const a = (t / 6000) * Math.PI * 2, beam = 260, g = ctx.createRadialGradient(PORT.x, PORT.y, 4, PORT.x, PORT.y, beam);
      g.addColorStop(0, `rgba(${ink.beam},.2)`); g.addColorStop(1, `rgba(${ink.beam},0)`);
      ctx.fillStyle = g; ctx.beginPath(); ctx.moveTo(PORT.x, PORT.y); ctx.arc(PORT.x, PORT.y, beam, a - .15, a + .15); ctx.closePath(); ctx.fill();
    }
    ctx.strokeStyle = ink.ripple; ctx.lineWidth = .8; ctx.setLineDash([2, 3]);
    scene.fresh.forEach(n => {
      for (let j = 0; j < 2; j++) {
        const ph = ((t / 4200) + (n.seed % 100) / 100 + j * .5) % 1;
        ctx.globalAlpha = (1 - ph) * .5;
        ctx.beginPath(); ctx.ellipse(n.x, n.y, n.r * (1.05 + ph * .9), n.r * (.85 + ph * .75), 0, 0, Math.PI * 2); ctx.stroke();
      }
    });
    ctx.globalAlpha = 1; ctx.setLineDash([]);
    return gusts;
  }
  // short gusts of ink running along the sea's pen strokes, endlessly renewed
  function drawGusts(ctx, scene, ink, t, previous) {
    const lines = scene.currents;
    if (!lines.length) return [];
    const gusts = previous.filter(g => t - g.born < g.life);
    while (gusts.length < 240) {
      gusts.push({line: lines[Math.floor(Math.random() * lines.length)], born: t - Math.random() * 2500, life: 1600 + Math.random() * 1600});
    }
    ctx.strokeStyle = ink.dot; ctx.lineCap = "round"; ctx.lineWidth = 1.3;
    gusts.forEach(g => {
      const p = (t - g.born) / g.life, pts = g.line, head = Math.min(pts.length - 1, Math.floor(p * (pts.length + 3))), tail = Math.max(0, head - 3);
      if (head <= tail) return;
      ctx.globalAlpha = Math.sin(Math.PI * p) * .75;
      ctx.beginPath(); ctx.moveTo(pts[tail][0], pts[tail][1]);
      for (let i = tail + 1; i <= head; i++) ctx.lineTo(pts[i][0], pts[i][1]);
      ctx.stroke();
    });
    ctx.globalAlpha = 1;
    return gusts;
  }
  // one light per feed at the heart of its island, a halo with no ringed dot; the busiest ones flicker
  function drawLights(ctx, scene, inkName, t) {
    scene.nodes.forEach(n => {
      const h = core.heatLevel(n.activity, scene.maxActivity), c = d3.color(core.heat(n.activity, scene.maxActivity, inkName));
      const glow = 7 + 13 * h, flicker = t != null && h > .55 ? .82 + .18 * Math.sin(t / 260 + n.seed % 17) : 1;
      const g = ctx.createRadialGradient(n.x, n.y, 0, n.x, n.y, glow);
      g.addColorStop(0, `rgba(${c.r},${c.g},${c.b},${.9 * flicker})`);
      g.addColorStop(.35, `rgba(${c.r},${c.g},${c.b},${.5 * flicker})`);
      g.addColorStop(1, `rgba(${c.r},${c.g},${c.b},0)`);
      ctx.fillStyle = g; ctx.beginPath(); ctx.arc(n.x, n.y, glow, 0, Math.PI * 2); ctx.fill();
    });
  }

  // ---------- the key's symbols, on 44 × 30 canvases ----------
  const KEY = [
    ["daily", "Large island", "a daily feed"],
    ["weekly", "Middling island", "weekly; monthly and rare ones smaller"],
    ["silent", "Islet (ED)", "a silent feed, marked “existence doubtful” as on real charts"],
    ["sounding", "Sounding", "unread entries from the past week"],
    ["light", "Activity light", "pale: little, over a week ago; deep and warm: much, very recent"],
    ["ring", "Range ring", "1 day, 1 week, 1 month, 3 months, 6 months since the last entry"],
    ["territory", "Territory", "one publisher’s feeds joined by land; a whole theme makes a continent"],
    ["port", "Ressac Harbour", "today, at the centre of the chart"],
  ];
  function drawKeySymbol(ctx, sym, inkName) {
    const ink = INKS[inkName] || INKS.bleu, ramp = d3.interpolateRgbBasis(ink.heat);
    ctx.fillStyle = ink.paper; ctx.fillRect(0, 0, 44, 30);
    ctx.strokeStyle = ink.ink; ctx.fillStyle = ink.label;
    if (sym === "daily" || sym === "weekly" || sym === "silent") {
      const r = sym === "daily" ? 11 : sym === "weekly" ? 8 : 4, rnd = mulberry(sym.length * 31);
      ctx.fillStyle = ink.dot;
      for (let i = 0; i < 260; i++) {
        const a = rnd() * Math.PI * 2, d = r + rnd() * 9;
        if (rnd() < 1 - (d - r) / 9) ctx.fillRect(22 + Math.cos(a) * d, 15 + Math.sin(a) * d * .8, .8, .8);
      }
      ctx.fillStyle = ink.land; ctx.beginPath(); ctx.ellipse(22, 15, r, r * .8, 0, 0, Math.PI * 2); ctx.fill();
      ctx.lineWidth = 1.1; ctx.stroke();
      if (sym === "silent") { ctx.fillStyle = ink.label; ctx.font = font(8, FONTS.mono); ctx.textAlign = "center"; ctx.fillText("ED", 22, 28); }
    } else if (sym === "sounding") {
      ctx.font = font(17, FONTS.body, "italic 500"); ctx.textAlign = "center"; ctx.textBaseline = "middle"; ctx.fillText("12", 22, 15);
    } else if (sym === "ring") {
      ctx.lineWidth = .9; ctx.setLineDash([2, 4]); ctx.beginPath(); ctx.ellipse(22, 22, 18, 12, 0, Math.PI * 1.05, Math.PI * 1.95); ctx.stroke(); ctx.setLineDash([]);
    } else if (sym === "port") portSymbol(ctx, ink, 16, 18, 1);
    else if (sym === "light") {
      [0, .25, .5, .75, 1].forEach((h, i) => { ctx.fillStyle = ramp(h); ctx.beginPath(); ctx.arc(6 + i * 8, 15, 2.5 + h * 1.5, 0, Math.PI * 2); ctx.fill(); });
    } else if (sym === "territory") {
      ctx.fillStyle = ink.land; ctx.lineWidth = 1.1;
      ctx.beginPath(); ctx.ellipse(10, 16, 7, 6, 0, 0, Math.PI * 2); ctx.moveTo(14, 13); ctx.quadraticCurveTo(22, 9, 30, 13); ctx.lineTo(30, 19); ctx.quadraticCurveTo(22, 21, 14, 19);
      ctx.fill(); ctx.stroke(); ctx.beginPath(); ctx.ellipse(34, 15, 7, 6, 0, 0, Math.PI * 2); ctx.fill(); ctx.stroke();
    }
  }

  // ---------- the page (browser only) ----------
  const FONT_FACES = ['500 16px "Cormorant Garamond"', '600 16px "Cormorant Garamond"', 'italic 400 16px "Cormorant Garamond"',
    'italic 500 16px "Cormorant Garamond"', '500 16px "Cormorant SC"', '600 16px "Cormorant SC"', '400 16px "IBM Plex Mono"', '500 16px "IBM Plex Mono"'];
  const INK_LABELS = {bleu: "Blue ink", nuit: "White on black", pop: "Pop", pirate: "Pirates"};
  const VIEW_LABELS = {harbour: "From the harbour", themes: "By theme"};
  const IDLE_MS = 2500, FRAME_MS = 33; // the toolbar fades after 2.5 s still; the overlay runs at about 30 frames a second

  function createPage(win) {
    const doc = win.document, post = bridge(win);
    const reduced = win.matchMedia ? win.matchMedia("(prefers-reduced-motion: reduce)") : {matches: false};
    let data = null, prefs = normalisePrefs(null), scene = null, gusts = [], raf = 0, lastFrame = -Infinity;
    let motionChosen = false; // the animation was switched on by hand while Reduce Motion is on
    let popFor = null, view = {k: 1, ox: 0, oy: 0, dpr: 1};

    // --- the page's elements, made once ---
    const plate = make(doc, "main", "plate"), base = make(doc, "canvas"), over = make(doc, "canvas");
    base.setAttribute("role", "img");
    over.setAttribute("aria-hidden", "true");
    const tip = make(doc, "div", "tip"), pop = make(doc, "div", "pop");
    tip.hidden = true; pop.hidden = true;
    pop.setAttribute("role", "dialog"); pop.setAttribute("aria-labelledby", "pop-title");
    plate.append(base, over, tip, pop);

    const bar = make(doc, "div", "bar");
    bar.setAttribute("role", "toolbar"); bar.setAttribute("aria-label", "Chart display");
    const button = (label, onClick) => { const b = make(doc, "button", null, label); b.type = "button"; b.addEventListener("click", onClick); return b; };
    const group = (label, buttons) => { const g = make(doc, "div", "group"); g.setAttribute("role", "group"); g.setAttribute("aria-label", label); g.append(...buttons); return g; };
    const viewButtons = VIEWS.map(v => Object.assign(button(VIEW_LABELS[v], () => setView(v)), {value: v}));
    const inkButtons = INK_NAMES.map(n => Object.assign(button(INK_LABELS[n] || n, () => setInk(n)), {value: n}));
    const animButton = button("Animation", toggleAnimation), keyButton = button("Key", () => showKey(keyPanel.hidden));
    keyButton.setAttribute("aria-controls", "chart-key");
    bar.append(group("View", viewButtons), group("Ink", inkButtons), animButton, keyButton);

    const keyPanel = make(doc, "section", "key"), keyList = make(doc, "ul");
    keyPanel.id = "chart-key"; keyPanel.hidden = true; keyPanel.setAttribute("aria-labelledby", "chart-key-title");
    const keyTitle = make(doc, "h2", null, "Key");
    keyTitle.id = "chart-key-title";
    const keyCanvases = KEY.map(([sym, name, what]) => {
      const li = make(doc, "li"), c = make(doc, "canvas"), text = make(doc, "div");
      c.dataset.sym = sym;
      text.append(make(doc, "strong", null, name), make(doc, "small", null, what));
      li.append(c, text); keyList.append(li);
      return c;
    });
    keyPanel.append(keyTitle, keyList);
    doc.body.append(plate, bar, keyPanel);

    // --- geometry: the 1920 × 1080 chart scaled to fit the window, letterboxed ---
    function fit() {
      const w = win.innerWidth, h = win.innerHeight, dpr = Math.min(2, win.devicePixelRatio || 1);
      [base, over].forEach(c => { c.width = Math.round(w * dpr); c.height = Math.round(h * dpr); });
      const k = Math.min(w / W, h / H);
      view = {k, ox: (w - W * k) / 2, oy: (h - H * k) / 2, dpr};
    }
    const chartTransform = ctx => { const s = view.k * view.dpr; ctx.setTransform(s, 0, 0, s, view.ox * view.dpr, view.oy * view.dpr); };
    const toChart = e => [(e.clientX - view.ox) / view.k, (e.clientY - view.oy) / view.k];
    function islandAt(e) {
      if (!scene) return null;
      const [x, y] = toChart(e);
      let best = null, bd = Infinity;
      scene.nodes.forEach(n => { const d = Math.hypot(n.x - x, n.y - y) - n.r; if (d < bd) { bd = d; best = n; } });
      return bd < 10 ? best : null; // within 10 chart units of its coast
    }

    // --- drawing ---
    const measureCtx = doc.createElement("canvas").getContext("2d"), widths = new Map();
    function measure(text, f) {
      const key = f + "\u0000" + text;
      if (!widths.has(key)) { measureCtx.font = f; widths.set(key, measureCtx.measureText(text).width); }
      return widths.get(key);
    }
    const ink = () => INKS[prefs.ink];
    function paintBase() {
      if (!scene || !base.width) return;
      const ctx = base.getContext("2d");
      ctx.setTransform(1, 0, 0, 1, 0, 0); ctx.fillStyle = ink().paper; ctx.fillRect(0, 0, base.width, base.height);
      chartTransform(ctx);
      drawChart(ctx, scene, prefs.ink);
      base.setAttribute("aria-label", scene.empty ? `Sea chart: ${EMPTY_MESSAGE.toLowerCase()}`
        : `Sea chart: ${plural(scene.nodes.length, "island", "islands")} ${scene.view === "harbour"
          ? "around Ressac Harbour, at a distance that grows with the time since each feed's last entry" : "in one archipelago per theme"}`);
    }
    function paintOver(t) {
      if (!scene || !over.width) return;
      const ctx = over.getContext("2d");
      ctx.setTransform(1, 0, 0, 1, 0, 0); ctx.clearRect(0, 0, over.width, over.height);
      chartTransform(ctx);
      gusts = drawOver(ctx, scene, prefs.ink, moving() ? {t, gusts} : null);
    }
    function paintKey() {
      const dpr = Math.min(2, win.devicePixelRatio || 1);
      keyCanvases.forEach(c => {
        c.width = 44 * dpr; c.height = 30 * dpr;
        const ctx = c.getContext("2d");
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        drawKeySymbol(ctx, c.dataset.sym, prefs.ink);
      });
    }
    function applyInk() {
      const i = ink(), style = doc.documentElement.style;
      style.setProperty("--paper", i.paper); style.setProperty("--ink", i.ink); style.setProperty("--label", i.label);
      doc.documentElement.dataset.ink = prefs.ink;
    }

    // --- animation: the user's choice, off by default under Reduce Motion unless switched on by hand ---
    const moving = () => prefs.animate && (!reduced.matches || motionChosen);
    function frame(t) {
      if (t - lastFrame >= FRAME_MS) { paintOver(t); lastFrame = t; }
      raf = moving() ? win.requestAnimationFrame(frame) : 0;
    }
    function syncMotion() {
      animButton.setAttribute("aria-pressed", String(moving()));
      if (moving() && !raf) raf = win.requestAnimationFrame(frame);
      if (!moving()) { if (raf) win.cancelAnimationFrame(raf); raf = 0; gusts = []; paintOver(0); }
    }
    if (reduced.addEventListener) reduced.addEventListener("change", () => { motionChosen = false; syncMotion(); });

    // --- controls ---
    function savePrefs() { if (post) post(prefsMessage(prefs)); }
    function pressed() {
      viewButtons.forEach(b => b.setAttribute("aria-pressed", String(b.value === prefs.view)));
      inkButtons.forEach(b => b.setAttribute("aria-pressed", String(b.value === prefs.ink)));
      keyButton.setAttribute("aria-expanded", String(!keyPanel.hidden));
    }
    function setView(v) {
      if (v === prefs.view) return;
      prefs = {...prefs, view: v}; closePop(); pressed(); savePrefs();
      rebuild();
    }
    function setInk(n) {
      if (n === prefs.ink) return;
      prefs = {...prefs, ink: n}; pressed(); savePrefs();
      applyInk(); paintBase(); paintKey(); paintOver(win.performance.now());
    }
    function toggleAnimation() {
      const on = !moving();
      prefs = {...prefs, animate: on}; motionChosen = on && reduced.matches; savePrefs();
      syncMotion();
    }
    function showKey(open) { keyPanel.hidden = !open; pressed(); if (open) paintKey(); }

    // the toolbar fades when the pointer is still
    let idleTimer = 0;
    function wake() {
      doc.body.classList.remove("idle");
      win.clearTimeout(idleTimer);
      idleTimer = win.setTimeout(() => doc.body.classList.add("idle"), IDLE_MS);
    }
    doc.addEventListener("pointermove", wake);
    doc.addEventListener("keydown", wake);

    // --- hover and popover ---
    plate.addEventListener("pointermove", e => {
      const n = islandAt(e);
      plate.style.cursor = n ? "pointer" : "";
      if (!n || !pop.hidden) { tip.hidden = true; return; }
      tip.replaceChildren(...tooltipContent(doc, n));
      tip.hidden = false;
      tip.style.left = e.clientX + "px"; tip.style.top = e.clientY + "px";
    });
    plate.addEventListener("pointerleave", () => { tip.hidden = true; });
    const handlers = {
      close: closePop,
      openFeed: id => { if (post) post({type: "openFeed", id}); },
      openURL: (url, e) => { if (post) { e.preventDefault(); post({type: "openURL", url}); } },
    };
    function closePop() { pop.hidden = true; popFor = null; }
    function openPop(n, x, y, focus) {
      tip.hidden = true;
      pop.replaceChildren(...popoverContent(doc, n, data, handlers));
      pop.hidden = false; popFor = {id: n.id, x, y};
      const w = pop.offsetWidth, h = pop.offsetHeight, vw = win.innerWidth, vh = win.innerHeight;
      let left = x + 14;
      if (left + w > vw - 12) left = x - w - 14;
      pop.style.left = Math.max(12, left) + "px";
      pop.style.top = Math.max(12, Math.min(vh - h - 12, y - 20)) + "px";
      if (focus) pop.querySelector(".pop-close").focus({preventScroll: true});
    }
    plate.addEventListener("click", e => {
      if (pop.contains(e.target)) return;
      const n = islandAt(e);
      if (n) openPop(n, e.clientX, e.clientY, true);
      else { closePop(); showKey(false); }
    });
    doc.addEventListener("keydown", e => {
      if (e.key !== "Escape") return;
      if (!pop.hidden) closePop();
      else if (!keyPanel.hidden) { showKey(false); keyButton.focus(); }
    });

    // --- resizing, and moving to a screen of another density ---
    let resizeTimer = 0;
    function relayout() { fit(); paintBase(); paintOver(win.performance.now()); }
    win.addEventListener("resize", () => { win.clearTimeout(resizeTimer); resizeTimer = win.setTimeout(relayout, 120); });
    (function watchDensity() {
      if (!win.matchMedia) return;
      const mq = win.matchMedia(`(resolution: ${win.devicePixelRatio || 1}dppx)`);
      const onChange = () => { relayout(); paintKey(); watchDensity(); };
      if (mq.addEventListener) mq.addEventListener("change", onChange, {once: true});
    })();

    // --- documents in, one after the other once the fonts are there (labels are measured with them) ---
    const fontsReady = doc.fonts && doc.fonts.load
      ? Promise.race([Promise.all(FONT_FACES.map(f => doc.fonts.load(f))), new Promise(r => win.setTimeout(r, 3000))]).catch(() => {})
      : Promise.resolve();
    let queue = fontsReady;
    function rebuild() {
      if (!data) return;
      scene = buildScene(data, prefs.view, measure);
      paintBase(); paintOver(win.performance.now());
      const n = popFor && scene.nodes.find(m => m.id === popFor.id);
      if (n) openPop(n, popFor.x, popFor.y, false); else closePop();
    }
    function show(next, nextPrefs) {
      queue = queue.then(() => {
        data = next;
        if (nextPrefs) { prefs = nextPrefs; motionChosen = false; applyInk(); pressed(); }
        rebuild(); syncMotion();
      }).catch(err => { if (win.console) win.console.error(err); });
    }

    fit(); applyInk(); pressed(); wake();
    return {
      // parsing is synchronous so the app sees a bad document as an error; nothing is returned (WebKit
      // cannot hand a promise back to evaluateJavaScript)
      load(json, rawPrefs) { show(parseData(json), normalisePrefs(rawPrefs)); },
      update(json) { show(parseData(json), null); },
    };
  }

  let page = null;
  const thePage = () => (page ||= createPage(globalThis));

  return Object.freeze({
    normalisePrefs, prefsMessage, parseData, bridge, formatSurvey, tooltipContent, popoverContent, buildScene, drawChart,
    load(json, prefs) { thePage().load(json, prefs); },
    update(json) { thePage().update(json); },
  });
});
