// The sea chart's page: canvas drawing of what chart-core.js computes, the animated overlay, hover, popover,
// floating toolbar, key, Survey mode (chart-survey.js's session, log and mode, wired here), and the bridge to the
// app. The builders and the drawing take their document and canvas context as arguments, so Node tests them with
// stand-ins; only createPage touches the page, and Node runs it on a fake window.
// Feed-supplied strings reach the page through canvas text and textContent only, never as HTML.
(function (root, factory) {
  "use strict";
  if (typeof module === "object" && module.exports) {
    const d3 = require("./d3.min.js"), core = require("./chart-core.js");
    core.useD3(d3);
    module.exports = factory(core, d3, require("./chart-survey.js"));
  } else {
    const api = factory(root.ChartCore, root.d3, root.ChartSurvey);
    root.ChartDraw = api;
    root.Chart = api.chart; // what the apps call: load, update, edited, undo, redo
  }
})(typeof globalThis !== "undefined" ? globalThis : this, function (core, d3, survey) {
  "use strict";

  const {W, H, CELL, GW, GH, PORT, RX, RY, RINGS, ISOBATHS, CARTOUCHE, COMPASS, FONTS, INKS, font, mulberry, fbm, ringFrac} = core;
  const EMPTY_MESSAGE = "No feed has published in six months";
  const NOTE = "Click the name to open this feed in Ressac.";

  // ---------- data and preferences ----------
  // reduceMotion is the system's setting as the app reads it (Reduce Motion on the Mac, Plasma's instant
  // animations): the page follows it and never reports it back, so prefsMessage leaves it out.
  const VIEWS = ["harbour", "themes"], INK_NAMES = Object.keys(INKS);
  function normalisePrefs(raw) {
    const p = raw && typeof raw === "object" ? raw : {};
    return {
      view: VIEWS.includes(p.view) ? p.view : "harbour",
      ink: INK_NAMES.includes(p.ink) ? p.ink : "bleu",
      animate: typeof p.animate === "boolean" ? p.animate : true,
      reduceMotion: p.reduceMotion === true,
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
      GROUP[node.group] || node.group, since(node.age), `${node.unread} unread this week`, feed.page != null ? `page ${feed.page}` : null]
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
  // where a w × h tooltip goes for a pointer at (x, y) in a vw × vh window: right of and below the pointer, on
  // the side away from any edge it would cross, and never outside the window
  function tooltipPosition(x, y, w, h, vw, vh) {
    const GAP = 12, MARGIN = 8;
    const side = (p, size, room) => {
      const after = p + GAP;
      const at = after + size > room - MARGIN ? p - GAP - size : after;
      return Math.max(MARGIN, Math.min(at, room - size - MARGIN));
    };
    return {left: side(x, w, vw), top: side(y, h, vh)};
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
  // It is also the Scene that chart-core.js's dropTarget reads (view, nodes at home, territories, ground).
  function buildScene(data, view, measure) {
    const v = data.feeds.length ? view : "harbour"; // an empty sea still has its harbour
    const nodes = core.place(data, v), terr = core.territories(nodes, data), ground = core.field(nodes, v, data, terr);
    const placements = core.labels(nodes, data, v, measure, {field: ground, territories: terr});
    const {strokes, currents} = seaStrokes(ground.at);
    return {
      data, view: v, empty: !nodes.length, nodes, territories: terr, ground, placements, strokes, currents,
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
    const rnd = mulberry(4242), at = scene.ground.at, onLand = .035;
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

  // Survey mode on the overlay: a faint survey grid on the neatline's 80-unit marks, and a dashed ring round the
  // island selected in the log. `px` is one screen pixel in chart units: the grid is one pixel wide at any size and
  // density (in chart units it would vanish on a small window), faint by its alpha.
  const SURVEY_GRID = 80, SURVEY_GRID_ALPHA = .2;
  function drawSurveyLayer(ctx, scene, inkName, selectedId, px = 1) {
    const ink = INKS[inkName] || INKS.bleu;
    ctx.save();
    ctx.strokeStyle = ink.ink; ctx.lineWidth = px; ctx.globalAlpha = SURVEY_GRID_ALPHA; ctx.setLineDash([]);
    ctx.beginPath();
    for (let x = 34 + SURVEY_GRID; x < W - 34; x += SURVEY_GRID) { ctx.moveTo(x, 34); ctx.lineTo(x, H - 34); }
    for (let y = 34 + SURVEY_GRID; y < H - 34; y += SURVEY_GRID) { ctx.moveTo(34, y); ctx.lineTo(W - 34, y); }
    ctx.stroke();
    const n = selectedId == null ? null : scene.nodes.find(m => m.id === selectedId);
    if (n) {
      ctx.globalAlpha = .9; ctx.lineWidth = 1.2; ctx.setLineDash([4, 3]);
      ctx.beginPath(); ctx.ellipse(n.x, n.y, n.r + 8, (n.r + 8) * .85, 0, 0, Math.PI * 2); ctx.stroke();
    }
    ctx.restore();
  }

  // ---------- the quill: Survey mode's cursor over the chart (chart.css reads it from --quill) ----------
  // Its nib, bottom left, is the hot spot. WebKit (the Mac) shows a cursor at its CSS size on any screen. Qt WebEngine
  // (the KDE app, whose adapter marks the root `qt-cursors`) shows the bitmap Chromium rasterised at the screen's
  // density one pixel per logical pixel, dropping its scale, so a 32 px quill came out at 64 on a 2× screen: there it is
  // given at 32 / density CSS px, which comes out at the Mac's 32. `scale` is 1 for WebKit, the density for Qt.
  const QUILL_SVG = "<g stroke-linecap='round' stroke-linejoin='round'>"
    + "<path d='M2 30 L9 19 C14 12 21 6 30 2 C27 11 21 18 13 23 Z' fill='#f7f8f6' stroke='#f7f8f6' stroke-width='4'/>"
    + "<path d='M2 30 L9 19 C14 12 21 6 30 2 C27 11 21 18 13 23 Z' fill='#f7f8f6' stroke='#1f3c88' stroke-width='1.4'/>"
    + "<path d='M2 30 L17 14 M9 19 L13 23' fill='none' stroke='#1f3c88' stroke-width='1.2'/></g>";
  function quillCursor(scale = 1) {
    const size = Math.round(32 / (Number.isFinite(scale) && scale >= 1 ? scale : 1)), at = v => Math.round(v * size / 32);
    const svg = `<svg xmlns='http://www.w3.org/2000/svg' width='${size}' height='${size}' viewBox='0 0 32 32'>${QUILL_SVG}</svg>`;
    const data = svg.replace(/</g, "%3C").replace(/>/g, "%3E").replace(/#/g, "%23");
    return `url("data:image/svg+xml,${data}") ${at(2)} ${at(30)}, crosshair`;
  }

  // The ring round the island an edit moved (recette fix 4), in the current ink, on the overlay. `phase` in [0, 1) is a
  // pulse's progress (the ring widens and fades); null draws it still (Reduce Motion). Wider than the selection's
  // dashed ring (n.r + 8), so both show. `px` is one screen pixel in chart units.
  const EDIT_RING = Object.freeze({gap: 10, grow: 16, still: 12, pulseMs: 900, pulses: 2, stillMs: 2000});
  function drawEditRing(ctx, node, inkName, phase, px = 1) {
    if (!node) return;
    const ink = INKS[inkName] || INKS.bleu, r = node.r + (phase == null ? EDIT_RING.still : EDIT_RING.gap + EDIT_RING.grow * phase);
    ctx.save();
    ctx.strokeStyle = ink.ink; ctx.setLineDash([]);
    ctx.globalAlpha = phase == null ? .9 : .9 * (1 - phase);
    ctx.lineWidth = 2.5 * px; // 2.5 screen pixels at any size (`px` is one screen pixel in chart units)
    ctx.beginPath(); ctx.ellipse(node.x, node.y, r, r * .85, 0, 0, Math.PI * 2); ctx.stroke();
    ctx.restore();
  }

  // ---------- gestures: the island in ink, the quill's line, the spring-back and the fade ----------
  // `g` is the page's gesture: {node, ghost: {x, y}} (an island dragged, drawn in ink where it is) or
  // {points, alpha} (the quill's line, fading when alpha drops), in chart units
  function drawGesture(ctx, g, inkName) {
    const ink = INKS[inkName] || INKS.bleu;
    ctx.save();
    ctx.setLineDash([]); ctx.lineCap = "round"; ctx.lineJoin = "round";
    if (g.ghost) {
      ctx.globalAlpha = .92; ctx.fillStyle = ink.land; ctx.strokeStyle = ink.ink; ctx.lineWidth = 1.6;
      ctx.beginPath(); ctx.ellipse(g.ghost.x, g.ghost.y, g.node.r, g.node.r * .85, 0, 0, Math.PI * 2); ctx.fill(); ctx.stroke();
    }
    if (g.points && g.points.length > 1 && g.alpha > 0) {
      ctx.globalAlpha = g.alpha; ctx.strokeStyle = ink.ink; ctx.lineWidth = 2.2;
      ctx.beginPath(); ctx.moveTo(g.points[0].x, g.points[0].y);
      for (let i = 1; i < g.points.length; i++) ctx.lineTo(g.points[i].x, g.points[i].y);
      ctx.stroke();
    }
    ctx.restore();
  }
  // step(u) for u in (0, 1) on successive animation frames of `win`, then done() once; with no motion, done() at
  // once and no frame at all (Reduce Motion, Plasma's instant animations)
  const SPRING_FRAMES = 18, FADE_FRAMES = 12;
  function tween(win, motion, frames, step, done) {
    if (!motion || !win || typeof win.requestAnimationFrame !== "function") { done(); return; }
    let i = 0;
    const next = () => {
      i += 1;
      if (i >= frames) { done(); return; }
      step(i / frames);
      win.requestAnimationFrame(next);
    };
    win.requestAnimationFrame(next);
  }
  // brings `ghost` ({x, y}) back to `node`'s place, overshooting a little like a spring, through win's animation
  // frames when `motion`, at once when not; ends exactly at node.x, node.y and calls done() once
  function springBack(ghost, node, win, motion, done) {
    const from = {x: ghost.x, y: ghost.y}, s = 1.70158;
    const back = u => { const v = u - 1; return 1 + (s + 1) * v * v * v + s * v * v; }; // ease-out-back
    tween(win, motion, SPRING_FRAMES, u => {
      const e = back(u);
      ghost.x = from.x + (node.x - from.x) * e; ghost.y = from.y + (node.y - from.y) * e;
    }, () => { ghost.x = node.x; ghost.y = node.y; done(); });
  }
  // the cartouche's size on screen (chart.css .cartouche): what cartouchePlace keeps clear of the labels before the
  // page has measured it
  const CARTOUCHE_PX = Object.freeze({w: 250, h: 124});
  // Where the cartouche goes, in chart units (its centre): beside its land, never over an island (`scene.nodes`, and
  // `keep`: the dragged island where it lies), the compass rose, the title cartouche or the land's own box. It may
  // cover labels: it is a passing form. cartouchePlace's spot is taken when it is clear of all that and of labels;
  // otherwise every spot inside the margin, on a 12-unit lattice, is weighed: the fewest of those obstacles, then the
  // nearest to the land by 12-unit bands, then the fewest labels within a band, then the nearest. So it never lands
  // more than one band farther than the nearest spot clear of the obstacles.
  const CARTOUCHE_MARGIN = 40, CARTOUCHE_LATTICE = 12, CARTOUCHE_BAND = 12, ISLAND_CLEAR = 6;
  function cartoucheAt(box, scene, size, keep = []) {
    const discs = [...(scene.nodes || []), ...keep].map(n => ({x: n.x, y: n.y, r: n.r + ISLAND_CLEAR}))
      .concat([{x: COMPASS.x, y: COMPASS.y, r: COMPASS.r + 12}]);
    const rects = [[CARTOUCHE.x, CARTOUCHE.y, CARTOUCHE.x + CARTOUCHE.w, CARTOUCHE.y + CARTOUCHE.h], box];
    const labels = (scene.placements || []).map(p => p.box);
    const meets = (a, b) => a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3];
    const touches = (r, d) => Math.hypot(Math.max(r[0] - d.x, 0, d.x - r[2]), Math.max(r[1] - d.y, 0, d.y - r[3])) < d.r;
    const hard = r => discs.filter(d => touches(r, d)).length + rects.filter(b => meets(r, b)).length;
    const gap = r => Math.hypot(Math.max(box[0] - r[2], 0, r[0] - box[2]), Math.max(box[1] - r[3], 0, r[1] - box[3]));
    const rectAt = c => [c.x - size.w / 2, c.y - size.h / 2, c.x + size.w / 2, c.y + size.h / 2];
    const placed = core.cartouchePlace(box, scene.placements || [], size), first = rectAt(placed);
    const inside = r => r[0] >= CARTOUCHE_MARGIN - 1e-9 && r[1] >= CARTOUCHE_MARGIN - 1e-9 && r[2] <= W - CARTOUCHE_MARGIN + 1e-9 && r[3] <= H - CARTOUCHE_MARGIN + 1e-9;
    if (inside(first) && !hard(first) && !labels.some(b => meets(first, b))) return placed;
    // [obstacles, band, labels, distance], compared in that order
    const better = (a, b) => { for (let i = 0; i < 4; i++) if (a[i] !== b[i]) return a[i] < b[i]; return false; };
    let best = null;
    for (let y = CARTOUCHE_MARGIN; y + size.h <= H - CARTOUCHE_MARGIN; y += CARTOUCHE_LATTICE) {
      for (let x = CARTOUCHE_MARGIN; x + size.w <= W - CARTOUCHE_MARGIN; x += CARTOUCHE_LATTICE) {
        const r = [x, y, x + size.w, y + size.h], h = hard(r);
        if (best && h > best.w[0]) continue;
        const d = gap(r), band = Math.floor(d / CARTOUCHE_BAND);
        if (best && h === best.w[0] && band > best.w[1]) continue;
        const w = [h, band, labels.filter(b => meets(r, b)).length, d];
        if (!best || better(w, best.w)) best = {w, r};
      }
    }
    return best ? {x: (best.r[0] + best.r[2]) / 2, y: (best.r[1] + best.r[3]) / 2} : placed;
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
  const INK_LABELS = {bleu: "Blue ink", nuit: "White on black"};
  const VIEW_LABELS = {harbour: "From the harbour", themes: "By theme"};
  const IDLE_MS = 2500, FRAME_MS = 33; // the toolbar fades after 2.5 s still; the overlay runs at about 30 frames a second
  const DRAWER = 320; // the Cartographer's Log's width: the chart keeps clear of it (chart.css .drawer)
  const DRAWER_GAP = 12; // and keeps this much paper before it, as the toolbar does (chart.css body.log-open .plate)

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
    const surveyButton = button("✒ Survey", () => mode.toggle()), logButton = button("Log", () => mode.toggleLog());
    // Undo and Redo: the Log's own buttons' path (logHandlers), in the toolbar for Survey mode
    const undoButton = button("Undo", () => logHandlers.undo()), redoButton = button("Redo", () => logHandlers.redo());
    surveyButton.hidden = true; logButton.hidden = true; undoButton.hidden = true; redoButton.hidden = true;
    logButton.setAttribute("aria-controls", "chart-log");
    bar.append(group("View", viewButtons), group("Ink", inkButtons), animButton, keyButton, surveyButton, logButton, undoButton, redoButton);

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
    const drawer = make(doc, "div", "drawer"), renameBox = make(doc, "div", "rename"), cartBox = make(doc, "div", "cartouche");
    drawer.id = "chart-log"; drawer.hidden = true; renameBox.hidden = true; cartBox.hidden = true;
    cartBox.setAttribute("role", "dialog");
    // the status line when the log is closed: a refused drop's hint, a refusal (the log shows them when open)
    const statusBox = make(doc, "p", "chart-status");
    statusBox.setAttribute("role", "status"); statusBox.hidden = true;
    plate.append(renameBox, cartBox, statusBox);
    doc.body.append(plate, bar, keyPanel, drawer);

    // --- geometry: the 1920 × 1080 chart scaled to fit the window, letterboxed ---
    function fit() {
      const w = Math.max(1, win.innerWidth - (drawerOpen() ? Math.min(DRAWER + DRAWER_GAP, win.innerWidth - 1) : 0));
      const h = win.innerHeight, dpr = Math.min(2, win.devicePixelRatio || 1);
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
      if (mode.active) drawSurveyLayer(ctx, scene, prefs.ink, selected, 1 / (view.k || 1));
      if (editRing && mode.active) drawEditRing(ctx, scene.nodes.find(n => n.id === editRing.id), prefs.ink, ringPhase(t ?? 0), 1 / (view.k || 1));
      if (gesture) drawGesture(ctx, gesture, prefs.ink);
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

    // --- animation: the user's choice, off by default under Reduce Motion (the system's, or the app's pref) unless
    // switched on by hand; body.still also stops the page's CSS transitions ---
    const reducedNow = () => prefs.reduceMotion || reduced.matches;
    const moving = () => prefs.animate && (!reducedNow() || motionChosen);
    const applyMotion = () => doc.body.classList.toggle("still", !core.editMotion(prefs, reduced.matches));
    function frame(t) {
      if (t - lastFrame >= FRAME_MS) { paintOver(t); lastFrame = t; }
      raf = moving() ? win.requestAnimationFrame(frame) : 0;
    }
    function syncMotion() {
      animButton.setAttribute("aria-pressed", String(moving()));
      if (moving() && !raf) raf = win.requestAnimationFrame(frame);
      if (!moving()) { if (raf) win.cancelAnimationFrame(raf); raf = 0; gusts = []; paintOver(0); }
    }
    if (reduced.addEventListener) reduced.addEventListener("change", () => { motionChosen = false; applyMotion(); syncMotion(); });

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
      stopRing(); applyInk(); paintBase(); paintKey(); paintOver(win.performance.now());
    }
    function toggleAnimation() {
      const on = !moving();
      prefs = {...prefs, animate: on}; motionChosen = on && reducedNow(); savePrefs();
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
      if (press && press.kind) { tip.hidden = true; plate.style.cursor = press.kind === "drag" ? "grabbing" : ""; return; }
      const n = islandAt(e);
      plate.style.cursor = n ? (mode.active ? "grab" : "pointer") : mode.active && nameTarget(e) ? "text" : ""; // else the quill (chart.css)
      if (!n || !pop.hidden) { tip.hidden = true; return; }
      tip.replaceChildren(...tooltipContent(doc, n));
      tip.hidden = false;
      const {left, top} = tooltipPosition(e.clientX, e.clientY, tip.offsetWidth, tip.offsetHeight, win.innerWidth, win.innerHeight);
      tip.style.left = left + "px"; tip.style.top = top + "px";
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
    const onChart = t => t === base || t === over; // not the popover, the rename or the cartouche (even once emptied)
    plate.addEventListener("click", e => {
      // the click a browser sends after a drag or a stroke: on the plate (the pointer was captured) or a canvas
      if (swallowClick) { swallowClick = false; return; }
      if (!onChart(e.target)) return;
      const target = mode.active ? surveyTarget(e) : null, answer = mode.click(target);
      if (answer === "popover") { // the chart as it always was
        const n = islandAt(e);
        if (n) openPop(n, e.clientX, e.clientY, true);
        else { closePop(); showKey(false); }
        return;
      }
      closePop();
      if (answer === "rename") openRename(target);
      else if (target && target.kind === "island") select(target.id);
    });
    doc.addEventListener("keydown", e => {
      if (e.key !== "Escape") return;
      if (naming) cancelNaming(); // the forms' own Esc stops there; this is Esc from elsewhere on the page
      else if (rename) closeRename();
      else if (!pop.hidden) closePop();
      else if (!keyPanel.hidden) { showKey(false); keyButton.focus(); }
    });

    // --- Survey mode: one session, one log memory and one mode for this window ---
    // The mode is the state (chart-survey.js, tested without a DOM); drawSurvey() draws it: the toolbar buttons,
    // body.survey (grid and quill), the drawer with the log (rebuilt on every change, its open form, typed text,
    // caret and focus carried by `memory`), and the rename form over a label.
    let selected = null, message = null, rename = null, drawnOpen = false; // rename = {kind, id, p, form, sent}
    // press: the pointer while it is down ({pointer, node, offset, screen, points, kind: null | "drag" | "ink",
    // idle: a press that may not edit}); gesture: what the overlay shows (see drawGesture), with `sent` (an edit
    // carries it) and `landed` (its edit was done: it stays until the new chart); naming: the open cartouche
    // ({form, box, edit(name), gesture, sent}); swallowClick: the click after a drag or a stroke is not a click
    let press = null, gesture = null, naming = null, swallowClick = false;
    const memory = survey.logMemory();
    const session = survey.createSession({
      post: m => { if (post) post(m); },
      onChange: () => drawSurvey(),
      onTimeout: outcome => settle(outcome),
      setTimeout: (fn, ms) => win.setTimeout(fn, ms),
    });
    const available = () => !!post && !!(data && data.ledger);
    const mode = survey.createMode({session, available, onChange: () => drawSurvey()});
    const drawerOpen = () => mode.active && mode.logOpen;
    const ledger = () => (data && data.ledger) || {seas: [], lands: []};
    const index = () => core.ledgerIndex(data);
    const seaRef = id => { const t = index().seaById(id); return t ? {id: t.id, key: t.key, sea: t.sea} : null; };

    function drawSurvey() {
      if (mode.active && !available()) { mode.toggle(); return; } // a document without a ledger: Survey ends (and redraws)
      surveyButton.hidden = !available();
      surveyButton.setAttribute("aria-pressed", String(mode.active));
      logButton.hidden = !mode.active;
      for (const [b, word, title] of [[undoButton, "Undo", session.titles().undo], [redoButton, "Redo", session.titles().redo]]) {
        b.hidden = !mode.active;
        b.disabled = title == null || session.pending();
        b.title = title ?? word;
      }
      logButton.setAttribute("aria-expanded", String(drawerOpen()));
      doc.body.classList.toggle("survey", mode.active);
      doc.body.classList.toggle("log-open", drawerOpen());
      if (!mode.active) memory.open = null; // a half-typed log form does not outlive Survey mode
      if (rename && !mode.renaming) closeRename();
      if (rename) rename.form.input.readOnly = session.pending();
      if (!mode.active) { press = null; swallowClick = false; closeNaming(false); gesture = null; ringFor = null; stopRing(); } // gestures and the ring end with Survey mode
      if (naming) {
        naming.form.input.readOnly = session.pending();
        const cancel = naming.form.nodes[0].querySelector(".name-form-cancel");
        if (cancel) cancel.disabled = naming.sent; // its edit is on its way: the reply decides
      }
      if (gesture && gesture.landed && !session.pending()) gesture = null; // the new chart draws it where it went
      if (drawerOpen() && data) {
        const state = {selected, busy: session.pending(), message, titles: session.titles()};
        drawer.replaceChildren(...survey.logContent(doc, data, state, logHandlers, memory));
        memory.restore();
      } else drawer.replaceChildren();
      drawer.hidden = !drawerOpen();
      statusBox.textContent = message ?? "";
      statusBox.hidden = !mode.active || drawerOpen() || !message;
      if (drawerOpen() !== drawnOpen) { drawnOpen = drawerOpen(); relayout(); }
      else paintOver(win.performance.now());
    }

    // an edit from the log or a rename; nothing is sent outside Survey mode
    function send(edit) {
      if (!mode.active || !edit) return false;
      message = null;
      return session.send(edit);
    }
    // an edit's outcome (Chart.edited, or no reply): the log's form, the rename form and the status line follow it
    function settle(outcome) {
      // a late reply: its forms and gesture ended with its timeout; whatever is in flight now waits for its own reply
      if ((outcome.kind === "done" || outcome.kind === "late") && outcome.edit) ringFor = core.movedFeed(outcome.edit);
      if (outcome.kind === "late") { message = outcome.message; drawSurvey(); return; }
      memory.result(outcome);
      message = outcome.message ?? null;
      if (rename && rename.sent) {
        rename.sent = false;
        if (outcome.kind === "name" || outcome.kind === "invalid") rename.form.setError(outcome.message); // stays open
        else closeRename();
      }
      const g = gesture && gesture.sent ? gesture : null, nameRefused = outcome.kind === "name" || outcome.kind === "invalid";
      if (naming && naming.sent) {
        naming.sent = false;
        if (nameRefused) { naming.form.setError(outcome.message); if (g) g.sent = false; } // stays open, ink and all
        else closeNaming(false);
      }
      if (g && g.sent) {
        if (outcome.kind === "done") g.landed = true; // stays where it was dropped until the new chart
        else release(g); // refused, failed or unanswered: back home, or the line fades
      }
      drawSurvey();
    }
    function select(id) {
      selected = id;
      if (!mode.logOpen) mode.toggleLog(); else drawSurvey();
    }
    const logHandlers = {
      select,
      setSea: (feed, to) => {
        const f = data.feeds.find(x => x.id === feed), toRef = to == null ? null : seaRef(to);
        if (f && (to == null || toRef)) send(core.edits.setSea(feed, index().seaRef(f.theme), toRef));
      },
      setLand: (feed, land) => { const l = index().landRef(land); if (l) send(core.edits.join(feed, l)); },
      leave: feed => { const l = index().landOf(feed); if (l) send(core.edits.leave(feed, {id: l.id, name: l.name})); },
      newLand: (name, ids) => send(core.edits.found(name, ids)),
      renameLand: (land, name) => { const l = index().landRef(land); if (l) send(core.edits.renameLand(l, name)); },
      deleteLand: land => { const l = index().landRef(land); if (l) send(core.edits.deleteLand(l)); },
      makeContinent: (sea, name) => { const t = seaRef(sea); if (t) send(core.edits.makeContinent(t, name)); },
      renameSea: (sea, name) => { const t = seaRef(sea); if (t) send(core.edits.renameSea(t, name)); },
      moveSea: (sea, delta) => {
        const ids = (ledger().seas || []).map(t => t.id), i = ids.indexOf(sea), j = Math.max(0, Math.min(ids.length - 1, i + delta));
        if (i < 0 || j === i) return;
        ids.splice(i, 1); ids.splice(j, 0, sea);
        send(core.edits.orderSeas(ids));
      },
      newSea: name => send(core.edits.newSea(name)),
      deleteSea: sea => { const t = seaRef(sea); if (t) send(core.edits.deleteSea(t)); },
      undo: () => mode.undo(),
      redo: () => mode.redo(),
    };

    // what a click in Survey mode is aimed at: an island first (a continent's name sits among its islands, and a
    // territory's along them), then a land's, continent's or sea's name
    function nameTarget(e) {
      if (!scene) return null;
      const [x, y] = toChart(e), p = core.nameAt(scene.placements, x, y);
      return p && entryOf(p) ? p : null;
    }
    function surveyTarget(e) {
      const n = islandAt(e);
      return n ? {kind: "island", id: n.id} : nameTarget(e);
    }
    // the ledger entry a name placement shows: a sea by key, a land or continent by its (unique) name
    function entryOf(p) {
      if (p.kind === "sea") { const t = (ledger().seas || []).find(s => s.key === p.key); return t ? {kind: "sea", id: t.id} : null; }
      const l = (ledger().lands || []).find(x => x.name === p.text && !!x.continent === (p.kind === "continent"));
      return l ? {kind: "land", id: l.id} : null;
    }
    const current = r => (r.kind === "sea" ? index().seaById(r.id) : index().landById(r.id));
    const currentName = (r, entry) => (r.kind === "sea" ? entry.sea : entry.name);

    // renaming in place: a name form over the label; Enter renames, Esc or an unchanged name cancels
    function openRename(p) {
      const target = entryOf(p), entry = target && current(target);
      if (!entry || naming) { mode.endRename(); return; } // the cartouche asks one name at a time
      if (rename) closeRename(false);
      const form = survey.nameForm(doc, {title: "Rename", value: currentName(target, entry), action: "Rename"}, {
        cancel: () => closeRename(),
        submit: raw => submitRename(raw),
      });
      rename = {...target, p, form, sent: false};
      renameBox.replaceChildren(...form.nodes);
      renameBox.hidden = false;
      placeRename();
      form.input.focus({preventScroll: true});
      if (typeof form.input.select === "function") form.input.select();
    }
    function submitRename(raw) {
      if (!rename || rename.sent || session.pending()) return;
      const entry = current(rename);
      if (!entry) { closeRename(); return; }
      const checked = core.checkName(raw, rename.kind, ledger(), entry.id);
      if (checked.error) { rename.form.setError(checked.error); return; }
      if (checked.name === currentName(rename, entry)) { closeRename(); return; }
      const r = rename, edit = r.kind === "sea"
        ? core.edits.renameSea({id: entry.id, key: entry.key, sea: entry.sea}, checked.name)
        : core.edits.renameLand({id: entry.id, name: entry.name}, checked.name);
      r.form.setError(null);
      r.sent = send(edit);
    }
    function closeRename(endMode = true) {
      if (!rename) return;
      rename = null;
      renameBox.hidden = true;
      renameBox.replaceChildren();
      if (endMode && mode.renaming) mode.endRename();
    }
    // over the label (screen px), inside the chart's room; after a new survey, over the label's new place, and gone
    // with its land or sea
    function placeRename() {
      if (!rename) return;
      if (!current(rename)) { closeRename(); return; }
      const entry = current(rename);
      const p = (scene ? scene.placements : []).find(q => (rename.kind === "sea" ? q.kind === "sea" && q.key === entry.key
        : (q.kind === "territory" || q.kind === "continent") && q.text === entry.name)) || rename.p;
      rename.p = p;
      const room = win.innerWidth - (drawerOpen() ? DRAWER + DRAWER_GAP : 0), w = renameBox.offsetWidth || 0, h = renameBox.offsetHeight || 0;
      const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));
      renameBox.style.left = `${clamp(view.ox + p.x * view.k, w / 2 + 8, room - w / 2 - 8)}px`;
      renameBox.style.top = `${clamp(view.oy + p.y * view.k, h / 2 + 8, win.innerHeight - h / 2 - 8)}px`;
    }
    // --- gestures (Survey mode): a press on an island drags it; a press elsewhere that moves draws ink ---
    const motionNow = () => core.editMotion(prefs, reduced.matches);
    // the window's animation frames, repainting the overlay after each step of a spring or a fade
    const frames = {requestAnimationFrame: fn => win.requestAnimationFrame(t => { fn(t); paintOver(t ?? win.performance.now()); })};
    const repaint = () => paintOver(win.performance.now());
    // The pointer is captured only once a press becomes a drag or a stroke: a captured press sends its click to the
    // plate, so a plain click must stay uncaptured to reach the chart. The click after a gesture is swallowed; the flag
    // never outlives its gesture (the next press, the end of the input task, or Survey off clears it).
    const capture = id => { if (id != null && typeof plate.setPointerCapture === "function") { try { plate.setPointerCapture(id); } catch (_) { /* gone */ } } };
    const uncapture = id => {
      if (id == null || typeof plate.releasePointerCapture !== "function") return;
      try { if (typeof plate.hasPointerCapture !== "function" || plate.hasPointerCapture(id)) plate.releasePointerCapture(id); } catch (_) { /* gone */ }
    };
    plate.addEventListener("pointerdown", e => {
      swallowClick = false;
      if (!mode.active || (e.button ?? 0) !== 0 || !onChart(e.target) || !scene || !data) return;
      const n = islandAt(e), [x, y] = toChart(e);
      // one edit at a time, and one name at a time (the cartouche, or a rename): such a press may still swallow its click
      const idle = session.pending() || !!naming || !!rename;
      press = {pointer: e.pointerId, node: n, offset: n ? {x: n.x - x, y: n.y - y} : null, screen: [{x: e.clientX, y: e.clientY}],
        points: [{x, y}], kind: null, idle};
    });
    plate.addEventListener("pointermove", e => {
      if (!press || (press.pointer != null && e.pointerId != null && e.pointerId !== press.pointer)) return;
      const [x, y] = toChart(e);
      press.screen.push({x: e.clientX, y: e.clientY});
      press.points.push({x, y});
      if (!press.kind && core.classifyPress(press.screen) === "stroke") {
        swallowClick = true;
        if (press.idle) { press = null; return; }
        press.kind = press.node ? "drag" : "ink";
        capture(press.pointer);
        tip.hidden = true;
        if (message) { message = null; drawSurvey(); } // a new gesture: the last one's hint is done
        gesture = press.node ? {node: press.node, ghost: {x: press.node.x, y: press.node.y}} : {points: press.points, alpha: 1};
      }
      if (press.kind === "drag") { gesture.ghost.x = x + press.offset.x; gesture.ghost.y = y + press.offset.y; }
      if (press.kind) repaint();
    });
    const letGo = e => {
      const p = press;
      press = null;
      if (p && p.kind) uncapture(p.pointer);
      if (swallowClick) win.setTimeout(() => { swallowClick = false; }, 0); // its click, if any, comes before this
      if (!p || !p.kind || (e && e.type === "pointercancel")) { if (p && p.kind) release(gesture); return; }
      if (p.kind === "drag") drop(p.node, gesture); else loopDone(p.points, gesture);
    };
    plate.addEventListener("pointerup", letGo);
    plate.addEventListener("pointercancel", letGo);

    // sends a gesture's edit; the gesture stays on the chart while it is pending
    function sendFor(g, edit) {
      if (!send(edit)) return false;
      g.sent = true;
      repaint();
      return true;
    }
    // back home (spring) or faded (ink); the overlay forgets it at the end, unless another gesture took its place
    function release(g) {
      if (!g) return;
      g.sent = false;
      const gone = () => { if (gesture === g) { gesture = null; repaint(); } };
      if (g.ghost) springBack(g.ghost, (scene && scene.nodes.find(n => n.id === g.node.id)) || g.node, frames, motionNow(), gone);
      else tween(frames, motionNow(), FADE_FRAMES, u => { g.alpha = 1 - u; }, gone);
    }
    const landRef = l => (l && Number.isInteger(l.id) ? {id: l.id, name: l.name} : null); // never a join or leave without one
    function drop(node, g) {
      if (!g || !g.ghost || !scene || !data) return;
      const found = core.dropTarget(scene, data, node, g.ghost.x, g.ghost.y), led = index();
      if (found.kind === "join" || found.kind === "leave") {
        const land = landRef(found.land);
        if (land && sendFor(g, core.edits[found.kind](node.id, land))) return;
      } else if (found.kind === "sea") {
        const to = found.key == null ? null : led.seaRef(found.key);
        if ((found.key == null || to) && sendFor(g, core.edits.setSea(node.id, led.seaRef(node.theme), to))) return;
      } else if (found.kind === "pair") {
        const other = scene.nodes.find(n => n.id === found.with);
        if (other) {
          const ids = data.feeds.map(f => f.id).filter(id => id === node.id || id === other.id); // the chart's order
          const box = [Math.min(g.ghost.x - node.r, other.x - other.r), Math.min(g.ghost.y - node.r, other.y - other.r),
            Math.max(g.ghost.x + node.r, other.x + other.r), Math.max(g.ghost.y + node.r, other.y + other.r)];
          askName({title: "Name this land", value: "", box, gesture: g, edit: name => core.edits.found(name, ids)});
          return;
        }
      } else if (found.kind === "refuse") {
        message = found.hint; // the log's status line
        drawSurvey();
      }
      release(g); // none, refused, or nothing could be sent
    }
    function loopDone(points, g) {
      if (!g || !scene || !data) return;
      const sel = core.closedLoop(points) ? core.loopSelection(scene.nodes, points, data) : {islands: [], continentSea: null};
      const xs = points.map(p => p.x), ys = points.map(p => p.y);
      const box = [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)];
      const sea = sel.continentSea ? index().seaRef(sel.continentSea) : null;
      if (sea) askName({title: "Make a continent", value: sea.sea, box, gesture: g, edit: name => core.edits.makeContinent(sea, name)});
      else if (sel.islands.length >= 2) askName({title: "Name this land", value: "", box, gesture: g, edit: name => core.edits.found(name, sel.islands)});
      else release(g); // fewer than two islands: the line fades
    }

    // the cartouche: a name form beside the new land, clear of the labels (cartouchePlace)
    function askName({title, value, box, gesture: g, edit}) {
      closeNaming(false);
      const form = survey.nameForm(doc, {title, value, action: "Create"}, {cancel: () => cancelNaming(), submit: raw => submitNaming(raw)});
      naming = {form, box, edit, gesture: g, sent: false};
      cartBox.setAttribute("aria-label", title);
      cartBox.replaceChildren(...form.nodes);
      cartBox.hidden = false;
      placeNaming();
      repaint();
      form.input.focus({preventScroll: true});
      if (typeof form.input.select === "function") form.input.select();
    }
    function submitNaming(raw) {
      const n = naming;
      if (!n || n.sent || session.pending()) return;
      const checked = core.checkName(raw, "land", ledger());
      if (checked.error) { n.form.setError(checked.error); return; }
      n.form.setError(null);
      n.sent = true; // before the send: the session redraws as it sends, and the cartouche must already be waiting
      if (sendFor(n.gesture, n.edit(checked.name))) n.form.input.readOnly = true;
      else { n.sent = false; drawSurvey(); }
    }
    // Cancel or Esc: not while its edit is on its way (the reply closes it, and moves the island or springs it back)
    function cancelNaming() { if (naming && !naming.sent) closeNaming(); }
    // closes the cartouche; cancelled (`back`), its island springs back or its line fades
    function closeNaming(back = true) {
      if (!naming) return;
      const n = naming;
      naming = null;
      cartBox.hidden = true;
      cartBox.replaceChildren();
      if (back && !n.gesture.landed) release(n.gesture);
    }
    function placeNaming() {
      if (!naming) return;
      const k = view.k || 1, w = cartBox.offsetWidth || CARTOUCHE_PX.w, h = cartBox.offsetHeight || CARTOUCHE_PX.h;
      const g = naming.gesture, ghost = g && g.ghost ? [{x: g.ghost.x, y: g.ghost.y, r: g.node.r}] : [];
      const c = cartoucheAt(naming.box, scene || {placements: [], nodes: []}, {w: w / k, h: h / k}, ghost);
      cartBox.style.left = `${view.ox + c.x * k}px`;
      cartBox.style.top = `${view.oy + c.y * k}px`;
    }

    // --- the ring round the island an edit moved: its edit is answered, the next chart shows where the island went; the
    // log selects it, and the ring pulses twice on the overlay, or stays still for 2 s under Reduce Motion ---
    let ringFor = null, editRing = null; // ringFor: the feed to ring on the next chart; editRing: {id, t0, still, timer, frame}
    function startRing(id) {
      stopRing();
      const mine = editRing = {id, t0: win.performance.now(), still: !motionNow(), timer: 0, frame: 0};
      if (mine.still) { mine.timer = win.setTimeout(() => { if (editRing === mine) { editRing = null; repaint(); } }, EDIT_RING.stillMs); return; }
      const step = t => {
        if (editRing !== mine) return;
        if (t - mine.t0 >= EDIT_RING.pulseMs * EDIT_RING.pulses) { editRing = null; repaint(); return; }
        if (!raf) paintOver(t); // the overlay's own loop, when it runs, paints the ring with everything else
        mine.frame = win.requestAnimationFrame(step);
      };
      mine.frame = win.requestAnimationFrame(step);
    }
    // ends the ring at once (Survey off, a new chart, a view or ink switch, a new ring): its timer and frame are cancelled;
    // the caller repaints the overlay
    function stopRing() {
      const r = editRing;
      if (!r) return;
      editRing = null;
      if (r.timer) win.clearTimeout(r.timer);
      if (r.frame) win.cancelAnimationFrame(r.frame);
    }
    // the ring's phase at time t (null when still)
    const ringPhase = t => (editRing.still ? null : (Math.max(0, t - editRing.t0) % EDIT_RING.pulseMs) / EDIT_RING.pulseMs);

    // ⌘Z in a name field belongs to the field
    const typing = () => {
      const a = doc.activeElement;
      return !!a && (a.tagName === "INPUT" || a.tagName === "TEXTAREA") && (drawer.contains(a) || renameBox.contains(a) || cartBox.contains(a));
    };

    // --- resizing, and moving to a screen of another density ---
    let resizeTimer = 0;
    function relayout() { fit(); paintBase(); paintOver(win.performance.now()); placeRename(); placeNaming(); }
    win.addEventListener("resize", () => { win.clearTimeout(resizeTimer); resizeTimer = win.setTimeout(relayout, 120); });
    (function watchDensity() {
      if (!win.matchMedia) return;
      const mq = win.matchMedia(`(resolution: ${win.devicePixelRatio || 1}dppx)`);
      const onChange = () => { relayout(); paintKey(); applyQuill(); watchDensity(); };
      if (mq.addEventListener) mq.addEventListener("change", onChange, {once: true});
    })();

    // --- documents in, one after the other once the fonts are there (labels are measured with them) ---
    const fontsReady = doc.fonts && doc.fonts.load
      ? Promise.race([Promise.all(FONT_FACES.map(f => doc.fonts.load(f))), new Promise(r => win.setTimeout(r, 3000))]).catch(() => {})
      : Promise.resolve();
    let queue = fontsReady;
    function rebuild() {
      if (!data) return;
      stopRing(); // a full redraw (a new chart, a view switch) ends the ring; show() starts the next one after it
      scene = buildScene(data, prefs.view, measure);
      paintBase(); paintOver(win.performance.now());
      const n = popFor && scene.nodes.find(m => m.id === popFor.id);
      if (n) openPop(n, popFor.x, popFor.y, false); else closePop();
      placeRename();
      placeNaming();
    }
    function show(next, nextPrefs) {
      queue = queue.then(() => {
        data = next;
        if (nextPrefs) { prefs = nextPrefs; motionChosen = false; ringFor = null; applyInk(); applyMotion(); pressed(); }
        if (selected != null && !data.feeds.some(f => f.id === selected)) selected = null;
        rebuild(); syncMotion();
        if (!nextPrefs) mode.updated(); // a new survey ends an edit that was answered (the one pending rule)
        const ringed = ringFor;
        ringFor = null;
        if (ringed != null && !nextPrefs && mode.active && scene.nodes.some(n => n.id === ringed)) { selected = ringed; startRing(ringed); }
        drawSurvey();
      }).catch(err => { if (win.console) win.console.error(err); });
    }

    // the quill at this screen's density (see quillCursor); again when the window moves to a screen of another density
    const applyQuill = () => doc.documentElement.style.setProperty("--quill",
      quillCursor(doc.documentElement.classList.contains("qt-cursors") ? win.devicePixelRatio || 1 : 1));

    fit(); applyInk(); applyMotion(); applyQuill(); pressed(); wake();
    return {
      // parsing is synchronous so the app sees a bad document as an error; nothing is returned (WebKit
      // cannot hand a promise back to evaluateJavaScript)
      load(json, rawPrefs) { show(parseData(json), normalisePrefs(rawPrefs)); },
      update(json) { show(parseData(json), null); },
      // the app's answer to {type: "edit"}: {"seq", "ok", "done" | "refused", "error"} as text
      edited(text) { const outcome = session.edited(text); if (outcome) settle(outcome); },
      // the apps' ⌘Z / ⇧⌘Z (Ctrl+Z / Ctrl+Shift+Z): only in Survey mode, never while a name is being typed
      undo: () => (typing() ? false : mode.undo()),
      redo: () => (typing() ? false : mode.redo()),
    };
  }

  let page = null;
  const thePage = () => (page ||= createPage(globalThis));
  const chart = Object.freeze({
    load(json, prefs) { thePage().load(json, prefs); },
    update(json) { thePage().update(json); },
    edited(text) { thePage().edited(text); },
    undo() { return thePage().undo(); },
    redo() { return thePage().redo(); },
  });

  return Object.freeze({
    normalisePrefs, prefsMessage, parseData, bridge, formatSurvey, tooltipContent, tooltipPosition, popoverContent, buildScene, drawChart,
    drawSurveyLayer, drawEditRing, EDIT_RING, springBack, CARTOUCHE_PX, cartoucheAt, quillCursor, createPage, chart, load: chart.load, update: chart.update,
  });
});
