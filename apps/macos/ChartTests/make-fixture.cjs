// Writes fixture.json: a synthetic chart document (spec §6) for the chart tests and preview.
// Every name is invented; nothing comes from a real database. Run: node apps/macos/ChartTests/make-fixture.cjs
"use strict";
const fs = require("node:fs");
const path = require("node:path");

function mulberry(a) {
  return function () {
    a |= 0; a = a + 0x6D2B79F5 | 0;
    let t = Math.imul(a ^ a >>> 15, 1 | a);
    t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t;
    return ((t ^ t >>> 14) >>> 0) / 4294967296;
  };
}
const rnd = mulberry(20261019);
const pick = list => list[Math.floor(rnd() * list.length)];
const round = (x, d) => Math.round(x * 10 ** d) / 10 ** d;

// 15 themes, in position order, with how many of the 96 themed feeds each holds
const THEMES = [
  ["kites", "Kites and gliders", "SEA OF KITES", 22],
  ["tides", "Tides and harbours", "TIDAL SEA", 12],
  ["bells", "Bells and clocks", "GULF OF BELLS", 9],
  ["maps", "Maps and atlases", "MAPMAKERS' SEA", 8],
  ["bread", "Bread and ovens", "SEA OF CRUSTS", 7],
  ["moths", "Moths and lanterns", "LANTERN SEA", 6],
  ["glass", "Glass and mirrors", "MIRROR BAY", 6],
  ["knots", "Knots and ropes", "SEA OF KNOTS", 5],
  ["owls", "Owls and the night", "NIGHT SOUND", 5],
  ["salt", "Salt and pans", "SALT SEA", 4],
  ["ferns", "Ferns and mosses", "MOSS REACH", 4],
  ["kilns", "Kilns and pottery", "POTTERS' GULF", 3],
  ["quills", "Inks and quills", "QUILL SEA", 3],
  ["bees", "Bees and hives", "HIVE BAY", 1],
  ["comets", "Comets", "COMET DEEP", 1],
];
const UNTHEMED = 4; // 96 themed + 4 unthemed = 100 feeds

const ADJ = ["Amber", "Brass", "Copper", "Drowsy", "Eastern", "Faded", "Gilded", "Hollow", "Idle", "Jade", "Lunar", "Misty",
  "Northern", "Olive", "Pale", "Quiet", "Rusty", "Silver", "Tidal", "Umber", "Velvet", "Windy", "Yellow", "Zinc"];
const NOUN = ["Almanac", "Beacon", "Courier", "Dispatch", "Echo", "Gazette", "Herald", "Journal", "Lantern", "Ledger",
  "Logbook", "Messenger", "Notebook", "Observer", "Pamphlet", "Quarterly", "Record", "Review", "Signal", "Tribune"];
const FOLDERS = ["Blogs", "Press", "Press / Weekly", "Science", "Science / Space", "Friends", "Letters", "Craft"];
// a few titles that stress the labels: long ones, accents, markup and quotes (shown as text, never as HTML)
const SPECIAL = ["The Extraordinarily Long-Winded Harbour Gazette", "Café des Ondes", "<b>Bold</b> & \"Quoted\" Notes",
  "Notes from the Lighthouse Keeper's Daughter", "Zéphyr"];

const GROUPS = [ // group, share of the feeds, age range in days (all within six months)
  ["daily", 15, 0.02, 1.5],
  ["weekly", 25, 0.3, 12],
  ["monthly", 25, 2, 45],
  ["rare", 20, 8, 130],
  ["silent", 15, 40, 182],
];

const surveyed = Date.parse("2026-10-19T15:40:00Z");
const DAY = 86400000;
const used = new Set();
function title(i) {
  if (i % 19 === 7 && SPECIAL.length) return SPECIAL.shift();
  for (;;) {
    const t = `The ${pick(ADJ)} ${pick(NOUN)}`;
    if (!used.has(t)) { used.add(t); return t; }
  }
}
const groupOf = i => { let k = i % 100; for (const g of GROUPS) { if (k < g[1]) return g; k -= g[1]; } return GROUPS[0]; };

const themeOf = [];
THEMES.forEach(([key, , , n]) => { for (let j = 0; j < n; j++) themeOf.push(key); });
for (let j = 0; j < UNTHEMED; j++) themeOf.push(null);
// shuffle the groups over the feeds so each theme gets a mix
const order = [...Array(100).keys()];
for (let i = 99; i > 0; i--) { const j = Math.floor(rnd() * (i + 1)); [order[i], order[j]] = [order[j], order[i]]; }

let entryId = 1;
const feeds = themeOf.map((theme, i) => {
  const [group, , lo, hi] = groupOf(order[i]);
  const age = round(lo * Math.pow(hi / lo, rnd()), 3);
  const spacing = { daily: 0.6, weekly: 5, monthly: 20, rare: 40, silent: 60 }[group];
  const entries = [];
  let activity = 0, unreadWeek = 0, entriesWeek = 0;
  for (let k = 0; k < 5; k++) {
    const a = age + k * spacing * (0.6 + rnd() * 0.8);
    if (a > 400) break;
    const read = a > 3 || rnd() < 0.4;
    if (a <= 30) activity += Math.pow(2, -a / 3);
    if (a <= 7) { entriesWeek++; if (!read) unreadWeek++; }
    entries.push({
      id: entryId++,
      title: `${pick(["On", "About", "Against", "Towards", "After"])} ${pick(NOUN).toLowerCase()}s, part ${k + 1}`,
      url: entryId % 23 === 0 ? null : `https://example.org/feed-${i + 1}/entry-${entryId - 1}`,
      day: new Date(surveyed - a * DAY).toISOString().slice(0, 10),
      read,
    });
  }
  return {
    id: i + 1, title: title(i), theme, folder: pick(FOLDERS), group, age_days: age,
    unread_week: unreadWeek, entries_week: entriesWeek, activity: round(activity, 4),
    page: rnd() < 0.15 ? null : 200 + i, entries,
  };
});

const ids = key => feeds.filter(f => f.theme === key).map(f => f.id);
const territories = [
  ["GULL PRESS GROUP", ids("kites").slice(0, 3)],
  ["WINDWARD HOUSE", ids("kites").slice(3, 5)],
  ["THE KITE GUILD", ids("kites").slice(5, 9)],
  ["HARBOURMASTER'S PRESS", ids("tides").slice(0, 3)],
  ["CARTOGRAPHERS' ROW", ids("maps").slice(0, 2)],
  ["LAMPLIGHTERS", ids("moths").slice(0, 2)],
  ["SILVERING WORKS", ids("glass").slice(0, 2)],
  ["CHIME BROTHERS", ids("bells").slice(0, 2)], // inside a continent
  ["SALTWORKS", ids("salt").slice(0, 2)],
  ["COAST AND CHART", [ids("tides")[5], ids("maps")[4]]], // one publisher across two themes
].map(([name, members]) => ({ name, feeds: members }));
const continents = [["BELLMARK", "bells"], ["CRUSTLAND", "bread"], ["THE KNOTTED LAND", "knots"], ["KILN COUNTRY", "kilns"]]
  .map(([name, theme]) => ({ name, theme }));

const doc = {
  version: 1,
  surveyed_at: "2026-10-19T15:40:00Z",
  totals: { feeds: feeds.length, unread_week: feeds.reduce((s, f) => s + f.unread_week, 0) },
  themes: THEMES.map(([key, name, sea]) => ({ key, name, sea, count: ids(key).length })),
  feeds, territories, continents,
};

// one feed, theme, territory or continent per line: readable diffs, modest size
const lines = list => list.map(x => "  " + JSON.stringify(x)).join(",\n");
const out = `{"version": 1,
"surveyed_at": ${JSON.stringify(doc.surveyed_at)},
"totals": ${JSON.stringify(doc.totals)},
"themes": [
${lines(doc.themes)}
],
"feeds": [
${lines(doc.feeds)}
],
"territories": [
${lines(doc.territories)}
],
"continents": [
${lines(doc.continents)}
]}
`;
JSON.parse(out); // the hand-made layout must stay valid JSON
fs.writeFileSync(path.join(__dirname, "fixture.json"), out);
console.log(`fixture.json: ${feeds.length} feeds, ${doc.themes.length} themes, ${territories.length} territories, ${continents.length} continents`);
