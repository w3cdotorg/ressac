// Tests of the territory editor's pure part in chart-core.js: the edit texts, the name rules, the wire
// builders, the ledger index and the page history. Run with `node --test 'apps/macos/ChartTests/*.test.cjs'`.
"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");
const fs = require("node:fs");

const ROOT = path.join(__dirname, "..", "..", "..");
const core = require(path.join(ROOT, "apps", "macos", "Ressac", "Resources", "Chart", "chart-core.js"));
const fixture = () => JSON.parse(JSON.stringify(require("./fixture.json")));
const T = core.EDIT_TEXT;

test("edit_texts_match_the_core", () => {
  const rs = fs.readFileSync(path.join(ROOT, "crates", "ressac-core", "src", "error.rs"), "utf8");
  const quoted = text => `"${text}"`;
  for (const key of ["changed", "landTaken", "seaTaken", "nameLength", "newLandTwo"]) {
    assert.ok(typeof T[key] === "string" && T[key], key);
    assert.ok(rs.includes(quoted(T[key])), `${key}: ${T[key]} is not in error.rs`);
  }
  const hint = T.continentHint("{name}");
  assert.ok(rs.includes(quoted(hint)), `continentHint: ${hint} is not in error.rs`);
  assert.equal(T.continentHint("BELLMARK"), "Part of the continent BELLMARK: move it to another sea, or turn the continent back into open water.");
  assert.equal(core.MAX_EDIT, 65536);
  assert.equal(core.PENDING_MS, 5000);
});

test("max_edit_matches_the_core", () => {
  const rs = fs.readFileSync(path.join(ROOT, "crates", "ressac-core", "src", "store", "chart_edit.rs"), "utf8");
  assert.ok(/pub const MAX_EDIT: usize = 64 \* 1024;/.test(rs));
});

test("edit_size_counts_utf8_bytes", () => {
  assert.equal(core.editBytes("abc"), 3);
  assert.equal(core.editBytes("é"), 2);
  assert.equal(core.editBytes("🌊"), 4);
  assert.equal("🌊".length, 2);
  const big = JSON.stringify({a: "é".repeat(core.MAX_EDIT / 2)}); // 32768 chars, over the limit in bytes
  assert.ok(big.length < core.MAX_EDIT && core.editBytes(big) > core.MAX_EDIT);
  assert.equal(core.editTooBig(big), true);
  assert.equal(core.editTooBig("{}"), false);
});

test("name_cases_from_the_shared_table", () => {
  const table = JSON.parse(fs.readFileSync(path.join(ROOT, "tests", "fixtures", "chart-name-cases.json"), "utf8"));
  assert.ok(table.cases.length >= 17);
  for (const c of table.cases) {
    const ledger = c.kind === "land"
      ? {seas: [], lands: c.existing.map(l => ({id: l.id, name: l.name, continent: null, feeds: []}))}
      : {seas: c.existing.map((s, i) => ({id: s.id, key: `k${i}`, name: s.name, sea: s.sea, position: i + 1, feeds: 0})), lands: []};
    assert.deepEqual(core.checkName(c.raw, c.kind, ledger, c.self), c.expect, c.case);
  }
});

test("trim_follows_rust_white_space", () => {
  assert.equal(core.trimName("\u0085 x 　"), "x"); // NEL and ideographic space go
  assert.equal(core.trimName("﻿x﻿"), "﻿x﻿"); // the BOM stays
  assert.equal(core.trimName("  x "), "x");
  assert.equal(core.trimName("​x"), "​x"); // zero-width space is not White_Space
});

test("check_name_clashes_without_case", () => {
  const data = fixture(), ledger = data.ledger;
  const apple = ledger.lands.find(l => l.name === "APPLE PRESS");
  assert.ok(apple, "the fixture has the land APPLE PRESS");
  assert.deepEqual(core.checkName(" apple press ", "land", ledger, null), {error: T.landTaken});
  assert.deepEqual(core.checkName(" apple press ", "land", ledger, apple.id), {name: "apple press"});
  assert.deepEqual(core.checkName("apple press", "land", ledger, 9999), {error: T.landTaken});
  assert.deepEqual(core.checkName("sea of kites", "sea", ledger, null), {error: T.seaTaken});
  const kites = ledger.seas.find(s => s.sea === "SEA OF KITES");
  assert.deepEqual(core.checkName("sea of kites", "sea", ledger, kites.id), {name: "sea of kites"});
  // a sea also clashes with another theme's descriptive name
  assert.deepEqual(core.checkName("kites and GLIDERS", "sea", ledger, null), {error: T.seaTaken});
  assert.deepEqual(core.checkName("kites and gliders", "sea", ledger, null), {error: T.seaTaken});
  // a land does not clash with a sea, nor the reverse
  assert.deepEqual(core.checkName("Sea of Kites", "land", ledger, null), {name: "Sea of Kites"});
  assert.deepEqual(core.checkName("Apple Press", "sea", ledger, null), {name: "Apple Press"});
  // a continent is a land too
  assert.deepEqual(core.checkName("bellmark", "land", ledger, null), {error: T.landTaken});
  assert.deepEqual(core.checkName("   ", "land", ledger, null), {error: T.nameLength});
  assert.deepEqual(core.checkName("x".repeat(41), "sea", ledger, null), {error: T.nameLength});
});

test("delete_sea_question_names_feeds_and_continent", () => {
  const q = core.deleteSeaQuestion;
  assert.equal(q("SILICON SHORES", 12, true), "Delete the sea SILICON SHORES? Its 12 feeds will be uncharted, and its continent removed.");
  assert.equal(q("SILICON SHORES", 12, false), "Delete the sea SILICON SHORES? Its 12 feeds will be uncharted.");
  assert.equal(q("SILICON SHORES", 1, false), "Delete the sea SILICON SHORES? Its 1 feed will be uncharted.");
  assert.equal(q("SILICON SHORES", 1, true), "Delete the sea SILICON SHORES? Its 1 feed will be uncharted, and its continent removed.");
  assert.equal(q("SILICON SHORES", 0, true), "Delete the sea SILICON SHORES? Its continent will be removed.");
  assert.equal(q("SILICON SHORES", 0, false), "Delete the sea SILICON SHORES?");
  // the continent sentence is the one of the plan's Global Constraints
  assert.ok(T.continentRemoved === "Its continent will be removed.");
});

const done = (label, undo) => ({label, undo: {op: "patch", label, changes: undo}});

test("history_records_undoes_and_redoes", () => {
  let h = core.history.empty();
  assert.deepEqual(core.history.titles(h), {undo: null, redo: null});
  assert.equal(core.history.step(h, "undo"), null);
  assert.equal(core.history.step(h, "redo"), null);

  h = core.history.record(h, done("Join APPLE PRESS", [{kind: "member"}]));
  h = core.history.record(h, done("Join TRIO", [{kind: "member", feed: 3}]));
  assert.deepEqual(core.history.titles(h), {undo: "Undo Join TRIO", redo: null});

  const frozen = JSON.stringify(h);
  const s = core.history.step(h, "undo");
  assert.deepEqual(s.record, {label: "Join TRIO", edit: {op: "patch", label: "Join TRIO", changes: [{kind: "member", feed: 3}]}});
  assert.equal(JSON.stringify(h), frozen, "step does not change the history");
  assert.equal(s.rest.undo.length, 1);

  // the undo succeeded: its inverse goes on redo
  const back = done("Join TRIO", [{kind: "member", feed: 3, again: true}]);
  const h2 = core.history.land(h, "undo", s.record, back);
  assert.equal(JSON.stringify(h), frozen, "land returns a new object");
  assert.equal(h2.undo.length, 1);
  assert.deepEqual(h2.redo, [{label: "Join TRIO", edit: back.undo}]);
  assert.deepEqual(core.history.titles(h2), {undo: "Undo Join APPLE PRESS", redo: "Redo Join TRIO"});
  // landing on the history `step` returned (rest) gives the same result
  assert.deepEqual(core.history.land(s.rest, "undo", s.record, back), h2);

  // redo walks back
  const r = core.history.step(h2, "redo");
  assert.equal(r.record.label, "Join TRIO");
  const h3 = core.history.land(h2, "redo", r.record, done("Join TRIO", [{kind: "member", feed: 3}]));
  assert.equal(h3.redo.length, 0);
  assert.equal(h3.undo.length, 2);
  assert.equal(core.history.titles(h3).undo, "Undo Join TRIO");

  // a new edit clears redo
  const h4 = core.history.record(h2, done("Leave APPLE PRESS", [{kind: "member", feed: 1}]));
  assert.deepEqual(h4.redo, []);
  assert.deepEqual(core.history.titles(h4), {undo: "Undo Leave APPLE PRESS", redo: null});
});

test("an_empty_patch_is_not_a_history_step", () => {
  const h = core.history.empty();
  const same = core.history.record(h, done("Reorder Seas", []));
  assert.deepEqual(same, h);
  assert.deepEqual(core.history.record(h, done("Reorder Seas", [{kind: "sea"}])).undo.length, 1);
});

test("edit_failure_classifies_results", () => {
  const refused = error => ({ok: false, refused: true, error});
  assert.equal(core.editFailure(refused(T.landTaken)), "name");
  assert.equal(core.editFailure(refused(T.seaTaken)), "name");
  assert.equal(core.editFailure(refused(T.nameLength)), "name");
  assert.equal(core.editFailure(refused(T.newLandTwo)), "invalid");
  assert.equal(core.editFailure(refused(T.continentHint("BELLMARK"))), "continent");
  assert.equal(core.editFailure(refused(T.changed)), "changed");
  assert.equal(core.editFailure(refused("Something else the core refused.")), "changed");
  assert.equal(core.editFailure({ok: false, refused: false, error: "no such file"}), "failed");
  assert.equal(core.editFailure({ok: false, error: "no such file"}), "failed");
});

test("builders_write_the_wire_format", () => {
  const data = fixture(), idx = core.ledgerIndex(data);
  const apple = idx.landRef(data.ledger.lands.find(l => l.name === "APPLE PRESS").id);
  assert.deepEqual(apple, {id: apple.id, name: "APPLE PRESS"});
  const kites = idx.seaRef("kites");
  assert.deepEqual(kites, {id: 1, key: "kites", sea: "SEA OF KITES"});
  assert.equal(idx.seaRef(null), null);
  const E = core.edits;
  assert.deepEqual(E.join(7, apple), {op: "join", feed: 7, land: apple});
  assert.deepEqual(E.leave(7, apple), {op: "leave", feed: 7, land: apple});
  assert.deepEqual(E.found("TRIO", [1, 2, 3]), {op: "found", name: "TRIO", feeds: [1, 2, 3]});
  assert.deepEqual(E.renameLand(apple, "ORCHARD"), {op: "rename_land", land: apple, name: "ORCHARD"});
  assert.deepEqual(E.deleteLand(apple), {op: "delete_land", land: apple});
  assert.deepEqual(E.makeContinent(kites, "KITELAND"), {op: "make_continent", sea: kites, name: "KITELAND"});
  assert.deepEqual(E.setSea(7, kites, null), {op: "set_sea", feed: 7, from: kites, to: null});
  assert.deepEqual(E.setSea(7, null, kites), {op: "set_sea", feed: 7, from: null, to: kites});
  assert.ok("from" in E.setSea(7, undefined, undefined) && E.setSea(7, undefined, undefined).from === null, "null is written out");
  assert.deepEqual(E.newSea("COMICS"), {op: "new_sea", name: "COMICS"});
  assert.deepEqual(E.renameSea(kites, "KITE SEA"), {op: "rename_sea", sea: kites, name: "KITE SEA"});
  assert.deepEqual(E.orderSeas([2, 1, 3]), {op: "order_seas", seas: [2, 1, 3]});
  assert.deepEqual(E.deleteSea(kites), {op: "delete_sea", sea: kites});
  const changes = [{kind: "feed_seas", feeds: [1], before: 1, after: null}];
  assert.deepEqual(E.patch("Move to X", changes), {op: "patch", label: "Move to X", changes});
  // every wire object survives JSON as it is
  for (const e of [E.join(7, apple), E.setSea(7, kites, null), E.orderSeas([1])]) assert.deepEqual(JSON.parse(JSON.stringify(e)), e);
});

test("ledger_index_finds_lands_and_seas", () => {
  const data = fixture(), idx = core.ledgerIndex(data);
  const hidden = data.ledger.lands.find(l => l.name === "OLD MILL PRESS");
  assert.ok(hidden && hidden.feeds.length === 2, "the fixture has a land hidden off the chart");
  assert.ok(!data.territories.some(t => t.name === hidden.name), "it is not on the chart");
  assert.equal(idx.landOf(hidden.feeds[0]).id, hidden.id);
  assert.equal(idx.landOf(hidden.feeds[1]).name, "OLD MILL PRESS");
  assert.equal(idx.landOf(424242), null);
  const apple = data.ledger.lands.find(l => l.name === "APPLE PRESS");
  assert.equal(idx.landOf(apple.feeds[0]).id, apple.id);
  assert.equal(idx.landById(apple.id).name, "APPLE PRESS");
  assert.equal(idx.landById(-1), null);
  assert.equal(idx.continentOf("bells").name, "BELLMARK");
  assert.equal(idx.continentOf("kites"), null);
  assert.equal(idx.seaById(1).key, "kites");
  assert.equal(idx.seaById(-1), null);
  assert.deepEqual(idx.landRef(hidden.id), {id: hidden.id, name: "OLD MILL PRESS"});
  assert.equal(idx.landRef(-1), null);
  // an empty sea is in the ledger though no feed is on it
  const unsailed = data.ledger.seas.find(s => s.sea === "UNSAILED SEA");
  assert.equal(unsailed.feeds, 0);
  assert.ok(!data.themes.some(t => t.key === unsailed.key));
  // a continent's own row never counts as a member's land
  assert.equal(idx.landOf(undefined), null);
});

test("the_fixture_carries_ids_and_a_ledger", () => {
  const data = fixture();
  const ids = [...data.themes.map(t => t.id)];
  assert.equal(new Set(ids).size, ids.length);
  const landIds = [...data.territories, ...data.continents].map(l => l.id);
  assert.equal(new Set(landIds).size, landIds.length);
  for (const t of data.territories) assert.ok(data.ledger.lands.some(l => l.id === t.id && l.name === t.name && l.continent === null));
  for (const c of data.continents) assert.ok(data.ledger.lands.some(l => l.id === c.id && l.continent === c.theme));
  for (const t of data.themes) assert.equal(data.ledger.seas.find(s => s.id === t.id).feeds, t.count);
  const bad = data.feeds.find(f => f.title.includes("<img"));
  assert.ok(bad && bad.title.includes("</option>"), "a hostile title is in the fixture");
});
