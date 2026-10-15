# Phase 4: The Cemetery shows the unread entries of buried feeds

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Runs after:** Phase 3 (3a, 3b and 3c), after Phases 1 and 2. None of them touches the files below, so the line numbers are those of `855cd2c`; match on the quoted texts anyway. Phase 3c adds two app tests, so the Swift counts start at 320, not 318. Later phases edit some of the same files elsewhere: Phase 5 `Queries.swift` (`entries`, `search`, new helpers; this phase changes `Queries.garden` only) and `Records.swift` (`EntryRow`; this phase changes `GardenRow`), Phase 7 cites `AntiopeGardenTests.swift` (this phase's test moves its line numbers down by 18), and Phase 8 `GardenPage.swift` (`logDate`, while this phase changes `feedLine`, about 160 lines above it, and makes the file 8 lines longer).

**Goal:** close « Reporté ▸ Cœur et CLI » "`v_entry` et `v_unread_count` cachent les flux enterrés : le Cimetière ne peut pas afficher leurs non-lus", **without a schema change** (still v3). The reading views stay as they are; the Cemetery counts a buried feed's unread entries on the `entry` table itself, and both looks show the count on buried rows: "12 unread" in the native Garden, a yellow `12` before the chip on P802.

**Tech stack:** Swift 6.4 (Swift 6 mode, strict concurrency), GRDB 7.11.1, Swift Testing; Rust for the contract test.

## Why no schema change

- **The views are right for reading.** `v_entry` and `v_unread_count` filter `f.state = 'active'` so that a buried feed's entries leave Unread, Today, Starred, the folders, search and every sidebar count. Making them include buried feeds would need migration `0004` (schema v4, a backup, a new fixture), and then a `state = 'active'` filter in each of those queries: `Queries.sidebar` (four counts), `Queries.predicate` (shared with `Writes.markAllRead`), `Queries.search`, `Queries.entry`, and the future TUI of sub-project 2b. All that to serve one number per buried feed.
- **The base table already has what the Cemetery needs.** `entry.feed_id` and `entry.read_at` are v1 columns, and the v2 partial index `entry_unread ON entry(feed_id) WHERE read_at IS NULL` makes the count an index lookup. Burying a feed keeps its entries: `record_decision` only changes `feed.state`.
- **The app already reads base tables** next to the views: `folder`, `feed`, `feed_health` (sidebar), `fetch_log`, `entry_fulltext`, `entry_fts`. Reading `entry` for one count follows that practice.
- **Consequences, checked:**
  - `Queries.swift`: only `Queries.garden` changes, and only its Cemetery scope reads `entry`. GRDB tracks the tables a `ValueObservation` reads, so reading `entry` in every scope would refetch the All Feeds and Needs Triage pages at every entry write of a refresh and every "mark read". The other scopes select `0 AS unread`. `GardenStore` takes its buried rows (shown in the Cemetery, and dimmed in All Feeds during a visit) from the Cemetery query, so every buried row it shows has its count; `GardenInspector` reads the same scopes.
  - **Contract test:** the views' columns do not change, so `reading_columns_and_views_are_stable` and the fixture stay as they are. A new test pins what the app now relies on: a buried feed leaves `v_entry` and `v_unread_count` and keeps its unread rows in `entry`. The committed fixture is not regenerated (its buried feed has no entries); the Swift test adds entries to a private copy.
  - **No Rust code change:** the core never reads these counts.

## Two sub-phases

**Phase 4a: data and contract (4 files)**
- Modify: `crates/ressac-core/tests/contract.rs` (565 lines): one test.
- Modify: `apps/macos/Ressac/Data/Queries.swift` (191 lines): `Queries.garden`.
- Modify: `apps/macos/Ressac/Data/Records.swift` (283 lines): `GardenRow.unread`.
- Modify: `apps/macos/RessacTests/GardenTests.swift` (313 lines): one database test.

**Phase 4b: display in both looks (5 files)**
- Modify: `apps/macos/Ressac/Garden/GardenModel.swift` (256 lines): `GardenModel.unreadLabel(for:)`.
- Modify: `apps/macos/Ressac/Garden/GardenTable.swift` (142 lines): `GardenRowView`.
- Modify: `apps/macos/Ressac/Antiope/GardenPage.swift` (338 lines): `GardenPage.feedLine`.
- Modify: `apps/macos/RessacTests/GardenTests.swift`: the `row` helper, one model test.
- Modify: `apps/macos/RessacTests/AntiopeGardenTests.swift` (400 lines): the `gardenRow` helper, one page test.

**Step 0 rule:** `contract.rs`, `GardenPage.swift`, `GardenTests.swift` and `AntiopeGardenTests.swift` are over 300 lines but only get additions and a few changed lines (`feedLine` keeps its structure and gains one span); nothing is restructured, so no cleanup commit. Check the counts with `wc -l` when each sub-phase starts.

**`TODO.md`** is updated in the plan's final docs phase (see Notes).

## Review focus

1. **Only buried rows show a count,** and only when it is above zero: an active feed has its count in the sidebar, and "0 unread" would be noise. Pinned by `onlyBuriedFeedsShowTheirUnreadCount`.
2. **The P802 row stays exactly 28 cells** with a count: the count takes cells from the title, never from the chip. `buriedFeedRowsShowTheirUnreadCount` checks the text and the width; the existing `everyLineIsExactlyTheColumnWidth` still covers long and wide titles (its rows have no count).
3. **A huge count cannot break the row:** the teletext count is capped at 99 999, as `PageIndexColumn` caps its counts, and the title shrinks to fit.
4. **Reading or fetching does not refetch the other garden pages:** only the Cemetery's query reads `entry`, and `cemeteryRowsCountTheUnreadEntriesOfBuriedFeeds` asserts the All Feeds rows carry `0`.
5. **VoiceOver reads the count** in the Antiope row ("Gone, Dead, 12 unread, buried"); the native row reads its `Text` as is.

---

# Phase 4a: data and contract

### Task 4.1: The Cemetery query counts unread entries on `entry`

**Files:**
- Modify: `crates/ressac-core/tests/contract.rs` (a test before `:165`)
- Modify: `apps/macos/Ressac/Data/Queries.swift:116-123`
- Modify: `apps/macos/Ressac/Data/Records.swift:253-257`
- Modify: `apps/macos/RessacTests/GardenTests.swift` (a test before `:278`)

**Interfaces:**
- Consumes: the contract fixture (`TestDatabase.fixture`, buried feed "Photos from bouquiniste", no entries); `AppDatabase.open(at:)`; the suite's `fixtureCopy()` and `firstValue(_:in:)`.
- Produces: `GardenRow.unread: Int` (default `0`, so the memberwise initializer gains a trailing `unread:` parameter with a default and existing callers compile unchanged).

- [ ] **Step 1: Pin the contract in Rust**

In `crates/ressac-core/tests/contract.rs`, insert just before

```rust
#[test]
fn page_columns_and_view_are_stable() {
```

(lines 165-166):

```rust
/// `v_entry` and `v_unread_count` cover active feeds only, so a buried feed's entries leave
/// the reading lists. The Cemetery counts them on `entry` itself (`Queries.garden` in the
/// app), which needs its `feed_id` and `read_at` columns.
#[test]
fn buried_feeds_leave_the_reading_views_but_keep_their_unread_entries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("buried.sqlite");
    let store = Store::open(&path).unwrap();
    store
        .import_outlines(
            &ressac_core::opml::parse(
                r#"<opml><body><outline text="A" xmlUrl="https://a.example.org/feed"/></body></opml>"#,
            )
            .unwrap(),
            false,
            now(),
        )
        .unwrap();
    let entries = vec![
        parsed("a1", 1, "<p>One.</p>"),
        parsed("a2", 2, "<p>Two.</p>"),
    ];
    store
        .record_attempt("contract", &fetched(&store, 1, entries))
        .unwrap();
    garden::decide(&store, 1, Decision::Bury, now()).unwrap();
    let conn = Connection::open(&path).unwrap();
    let int = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
    assert_eq!(int("SELECT count(*) FROM v_entry"), 0);
    assert_eq!(int("SELECT count(*) FROM v_unread_count"), 0);
    assert_eq!(
        int("SELECT count(*) FROM entry WHERE feed_id = 1 AND read_at IS NULL"),
        2
    );
}

```

- [ ] **Step 2: Run it**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo test -p ressac-core --test contract buried_feeds_leave_the_reading_views_but_keep_their_unread_entries
```

Expected: PASS at once. It pins existing behaviour that the app starts to rely on in Step 5, so that a later schema change cannot drop it silently.

- [ ] **Step 3: Write the failing Swift test**

In `apps/macos/RessacTests/GardenTests.swift`, insert inside `GardenDecodingTests`, after `gardenRowsDecodeTheCoreJSONShapes`: just before the `}` that closes the suite (line 278, two lines above `@Suite struct GardenStatusStyleTests {`):

```swift

    /// The reading views hide buried feeds; the Cemetery still counts their unread entries.
    @Test func cemeteryRowsCountTheUnreadEntriesOfBuriedFeeds() async throws {
        let url = try fixtureCopy()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        do {
            let queue = try DatabaseQueue(path: url.path(percentEncoded: false))
            try await queue.write { db in
                try db.execute(sql: """
                    INSERT INTO entry (feed_id, guid, title, published_at, fetched_at, read_at)
                    SELECT f.id, 'photo-' || n.i, 'Photo ' || n.i, '2026-10-09T08:00:00.000Z', '2026-10-10T08:00:00.000Z',
                           CASE n.i WHEN 3 THEN '2026-10-10T09:00:00.000Z' END
                    FROM feed f, (SELECT 1 AS i UNION ALL SELECT 2 UNION ALL SELECT 3) n
                    WHERE f.title = 'Photos from bouquiniste';
                    """)
            }
        }
        let database = try AppDatabase.open(at: url)
        let cemetery = try await firstValue(Queries.garden(scope: .cemetery), in: database.reader)
        #expect(cemetery.map(\.title) == ["Photos from bouquiniste"])
        #expect(cemetery.first?.unread == 2, "two unread, one read")
        let all = try await firstValue(Queries.garden(scope: .all), in: database.reader)
        #expect(all.allSatisfy { $0.unread == 0 }, "only the Cemetery counts")
        let listed = try await database.reader.read { db in
            try Int.fetchOne(db, sql: "SELECT COUNT(*) FROM v_entry WHERE title LIKE 'Photo %'")
        }
        #expect(listed == 0, "the reading views still hide them")
    }
```

- [ ] **Step 4: Run it to see it fail**

```bash
cd apps/macos
xcodegen generate
xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/GardenDecodingTests 2>&1 | grep -E 'error:|Test run with|\*\* TEST'
cd ../..
```

Expected: `** TEST FAILED **` on a compile error, `value of type 'GardenRow' has no member 'unread'`.

- [ ] **Step 5: Add `unread` to the record and the query**

In `apps/macos/Ressac/Data/Records.swift`, replace lines 253-257 (the end of `GardenRow`):

```swift
    var htmlUrl: String?
    var xmlUrl: String

    var id: Int64 { feedId }
}
```

with:

```swift
    var htmlUrl: String?
    var xmlUrl: String
    /// Unread entries of a buried feed, counted by `Queries.garden(scope: .cemetery)` only (0 in
    /// the other scopes): the reading views hide buried feeds, so the sidebar has no count for them.
    var unread = 0

    var id: Int64 { feedId }
}
```

In `apps/macos/Ressac/Data/Queries.swift`, replace lines 116-123:

```swift
        case .cemetery: "state = 'buried'"
        }
        return ValueObservation.tracking { db in
            try GardenRow.fetchAll(db, sql: """
                SELECT feed_id, title, folder_path, state, status, reason, clue, evidence,
                       COALESCE(remarks, '[]') AS remarks, proposed_action, needs_triage, last_decision,
                       wayback_url, html_url, xml_url
                FROM v_garden_row WHERE \(filter)
```

with:

```swift
        case .cemetery: "state = 'buried'"
        }
        // `v_unread_count` covers active feeds only, so the Cemetery counts on `entry` itself. The
        // other scopes leave `entry` out: reading or fetching entries does not refetch them.
        let unread = scope == .cemetery
            ? "(SELECT COUNT(*) FROM entry e WHERE e.feed_id = v_garden_row.feed_id AND e.read_at IS NULL)"
            : "0"
        return ValueObservation.tracking { db in
            try GardenRow.fetchAll(db, sql: """
                SELECT feed_id, title, folder_path, state, status, reason, clue, evidence,
                       COALESCE(remarks, '[]') AS remarks, proposed_action, needs_triage, last_decision,
                       wayback_url, html_url, xml_url, \(unread) AS unread
                FROM v_garden_row WHERE \(filter)
```

The column is selected in every scope because `GardenRow` decodes it (`Codable` requires the key even with a default value).

- [ ] **Step 6: Run it to see it pass**

```bash
cd apps/macos
xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/GardenDecodingTests 2>&1 | grep -E 'error:|Test run with|\*\* TEST'
cd ../..
```

Expected: `✔ Test run with 2 tests in 1 suite passed`, `** TEST SUCCEEDED **`.

- [ ] **Step 7: Commit**

```bash
git add crates/ressac-core/tests/contract.rs apps/macos/Ressac/Data/Queries.swift apps/macos/Ressac/Data/Records.swift apps/macos/RessacTests/GardenTests.swift
git commit -F - <<'EOF'
Garden: the Cemetery query counts the unread entries of buried feeds

v_entry and v_unread_count cover active feeds only, as the reading lists
need. The Cemetery counts on entry itself (feed_id, read_at, under the
entry_unread index), so the schema stays v3. The contract test pins it.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Phase 4a verification gate

- [ ] **Step 1: Run the Rust gate, then the Swift gate**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace 2>&1 | grep -E '^test result|FAILED|panicked'
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test 2>&1 | tee "$TMPDIR/ressac-4a.log" | grep -E 'Test run with|\*\* TEST'
grep -cE '\.swift:[0-9]+:[0-9]+: (warning|error)' "$TMPDIR/ressac-4a.log"
cd ../..
```

Expected: `fmt` prints nothing; clippy ends with `Finished` and no warning; every `test result:` line says `0 failed`, with `contract` now 9 (the others as after Phase 3b: `ressac` 35, `cli` 19, core lib 260 + 1 ignored, `article_proxy` 1, `end_to_end` 4); `✔ Test run with 321 tests in 79 suites passed`, `** TEST SUCCEEDED **`; the last `grep -c` prints `0` (the target treats warnings as errors anyway).

- [ ] **Step 2: Stop.** Report the gate output and wait for Clément's approval before Phase 4b.

---

# Phase 4b: the count in both looks

### Task 4.2: "12 unread" on buried rows, native and Antiope

**Files:**
- Modify: `apps/macos/Ressac/Garden/GardenModel.swift` (after `chip(for:)`, lines 221-223)
- Modify: `apps/macos/Ressac/Garden/GardenTable.swift:122-128`
- Modify: `apps/macos/Ressac/Antiope/GardenPage.swift:122-135`
- Modify: `apps/macos/RessacTests/GardenTests.swift:6-13` and the top of `GardenModelTests` (line 15)
- Modify: `apps/macos/RessacTests/AntiopeGardenTests.swift:10-16` and a test before `:157`

**Interfaces:**
- Consumes: `GardenRow.unread` (Task 4.1), `GardenModel.isBuried(_:)`, `GardenModel.chip(for:)`, `CellSpan`, `CellLine`.
- Produces: `static func GardenModel.unreadLabel(for row: GardenRow) -> String?`: `"\(row.unread) unread"` for a buried row with `unread > 0`, else `nil`.

- [ ] **Step 1: Write the failing tests**

In `apps/macos/RessacTests/GardenTests.swift`, replace the helper, lines 6-13:

```swift

private func row(_ id: Int64, _ title: String, status: String?, folder: String? = "Blogs", state: String = "active",
                 action: ProposedAction? = nil, needsTriage: Bool = false, decision: String? = nil,
                 site: String? = nil, wayback: String? = nil) -> GardenRow {
    GardenRow(feedId: id, title: title, folderPath: folder, state: state, status: status, reason: nil, clue: nil,
              evidence: nil, remarks: [], proposedAction: action, needsTriage: needsTriage, lastDecision: decision,
              waybackUrl: wayback, htmlUrl: site, xmlUrl: "https://feeds.example.org/\(id)")
}
```

with:

```swift

private func row(_ id: Int64, _ title: String, status: String?, folder: String? = "Blogs", state: String = "active",
                 action: ProposedAction? = nil, needsTriage: Bool = false, decision: String? = nil,
                 site: String? = nil, wayback: String? = nil, unread: Int = 0) -> GardenRow {
    GardenRow(feedId: id, title: title, folderPath: folder, state: state, status: status, reason: nil, clue: nil,
              evidence: nil, remarks: [], proposedAction: action, needsTriage: needsTriage, lastDecision: decision,
              waybackUrl: wayback, htmlUrl: site, xmlUrl: "https://feeds.example.org/\(id)", unread: unread)
}
```

and insert at the top of `GardenModelTests`, just after `@Suite struct GardenModelTests {` (line 15):

```swift
    /// Only a buried feed says how many entries it left unread: active feeds have the sidebar.
    @Test func onlyBuriedFeedsShowTheirUnreadCount() {
        #expect(GardenModel.unreadLabel(for: row(4, "Gone", status: "Dead", state: "buried", unread: 12)) == "12 unread")
        #expect(GardenModel.unreadLabel(for: row(4, "Gone", status: "Dead", state: "buried")) == nil)
        #expect(GardenModel.unreadLabel(for: row(1, "Live", status: "Alive", unread: 3)) == nil)
    }

```

In `apps/macos/RessacTests/AntiopeGardenTests.swift`, replace lines 10-16 (the end of the `gardenRow` helper):

```swift
                       action: ProposedAction? = nil, needsTriage: Bool = false, decision: String? = nil,
                       site: String? = nil, wayback: String? = nil) -> GardenRow {
    GardenRow(feedId: id, title: title, folderPath: folder, state: state, status: status, reason: reason, clue: nil,
              evidence: evidence.map { GardenEvidence(bullets: $0, clue: "", language: nil) }, remarks: remarks,
              proposedAction: action, needsTriage: needsTriage, lastDecision: decision,
              waybackUrl: wayback, htmlUrl: site, xmlUrl: "https://feeds.example.org/\(id)")
}
```

with:

```swift
                       action: ProposedAction? = nil, needsTriage: Bool = false, decision: String? = nil,
                       site: String? = nil, wayback: String? = nil, unread: Int = 0) -> GardenRow {
    GardenRow(feedId: id, title: title, folderPath: folder, state: state, status: status, reason: reason, clue: nil,
              evidence: evidence.map { GardenEvidence(bullets: $0, clue: "", language: nil) }, remarks: remarks,
              proposedAction: action, needsTriage: needsTriage, lastDecision: decision,
              waybackUrl: wayback, htmlUrl: site, xmlUrl: "https://feeds.example.org/\(id)", unread: unread)
}
```

and insert just before the doc comment `    /// VoiceOver reads the original title; the cells show a `?` for what Bedstead lacks, which` (line 157):

```swift
    /// A buried feed's unread count sits before its chip, in yellow like the index's counts, and
    /// VoiceOver reads it; without unread entries the row is unchanged.
    @Test func buriedFeedRowsShowTheirUnreadCount() {
        let counted = GardenPage.feedLine(gardenRow(4, "Gone", status: "Dead", state: "buried", decision: "bury", unread: 12),
                                          selected: false, cells: cells)
        #expect(counted.text == "  Gone" + spaces(12) + " 12 buried")
        #expect(counted.cells.width == 28)
        #expect(counted.cells.spans[2] == CellSpan(" 12", cells: 3, foreground: .yellow))
        #expect(counted.cells.spans.last == CellSpan(" buried", cells: 7, foreground: .green))
        #expect(counted.cells.accessibilityText == "Gone, Dead, 12 unread, buried")

        let none = GardenPage.feedLine(gardenRow(4, "Gone", status: "Dead", state: "buried", decision: "bury"),
                                       selected: false, cells: cells)
        #expect(none.text == "  Gone" + spaces(15) + " buried")
        #expect(none.cells.spans.count == 3)
        #expect(none.cells.accessibilityText == "Gone, Dead, buried")
    }

```

- [ ] **Step 2: Run them to see them fail**

```bash
cd apps/macos
xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/GardenModelTests -only-testing:RessacTests/GardenPageTests 2>&1 | grep -E 'error:|Test run with|\*\* TEST'
cd ../..
```

Expected: `** TEST FAILED **` on a compile error, `type 'GardenModel' has no member 'unreadLabel'`.

- [ ] **Step 3: The label, in the model**

In `apps/macos/Ressac/Garden/GardenModel.swift`, insert after the end of `chip(for:)`, whose last lines are (221-223):

```swift
        case nil: return triageStatuses.contains(status(of: row)) ? "wait" : nil
        }
    }
```

the function (with one blank line before it):

```swift

    /// "12 unread" on a buried feed: the reading lists hide its entries, so the garden is the only
    /// place that tells what burying left unread. Nil for active feeds and when nothing is left.
    static func unreadLabel(for row: GardenRow) -> String? {
        isBuried(row) && row.unread > 0 ? "\(row.unread) unread" : nil
    }
```

- [ ] **Step 4: The native row**

In `apps/macos/Ressac/Garden/GardenTable.swift` (`GardenRowView`), replace lines 122-128:

```swift
            if let clue = row.clue, !clue.isEmpty {
                Text(clue)
                    .font(.callout)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
```

with:

```swift
            if let clue = row.clue, !clue.isEmpty {
                Text(clue)
                    .font(.callout)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            if let unread = GardenModel.unreadLabel(for: row) {
                Text(unread)
                    .font(.callout)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
```

The count sits between the clue and the chip, in the clue's style.

- [ ] **Step 5: The teletext row**

In `apps/macos/Ressac/Antiope/GardenPage.swift`, replace lines 122-135 (the doc comment and the first lines of `feedLine`, up to its `let line = …`):

```swift
    /// "● Title … bury": a yellow dot while the feed needs triage, the title, then the chip (the
    /// proposed action in cyan, or the decision once made in green), as `EntryPageColumn` lays out
    /// an entry. Black on white when selected; a `?` replacement stays magenta (`CellSpan`).
    static func feedLine(_ row: GardenRow, selected: Bool, cells: Int) -> Line {
        let decided = row.lastDecision != nil && !row.needsTriage
        let chip = GardenModel.chip(for: row).map { " " + $0 } ?? ""
        let markerCells = min(2, max(0, cells))
        let chipCells = min(chip.count, max(0, cells - markerCells))
        let spoken = [row.title, GardenModel.status(of: row), GardenModel.chip(for: row)].compactMap { $0 }
        let line = CellLine([
            CellSpan(row.needsTriage ? "●" : "", cells: markerCells, foreground: .yellow),
            CellSpan(row.title, cells: cells - markerCells - chipCells, foreground: .white),
            CellSpan(chip, cells: chipCells, foreground: decided ? .green : .cyan),
        ], accessibilityText: spoken.joined(separator: ", "))
```

with:

```swift
    /// "● Title … bury": a yellow dot while the feed needs triage, the title, a buried feed's unread
    /// count in yellow (as the index shows counts), then the chip (the proposed action in cyan, or
    /// the decision once made in green), as `EntryPageColumn` lays out an entry. Black on white when
    /// selected; a `?` replacement stays magenta (`CellSpan`).
    static func feedLine(_ row: GardenRow, selected: Bool, cells: Int) -> Line {
        let decided = row.lastDecision != nil && !row.needsTriage
        let chip = GardenModel.chip(for: row).map { " " + $0 } ?? ""
        let unreadLabel = GardenModel.unreadLabel(for: row)
        let count = unreadLabel == nil ? "" : " \(min(row.unread, 99_999))"
        let markerCells = min(2, max(0, cells))
        let chipCells = min(chip.count, max(0, cells - markerCells))
        let countCells = min(count.count, max(0, cells - markerCells - chipCells))
        let spoken = [row.title, GardenModel.status(of: row), unreadLabel, GardenModel.chip(for: row)].compactMap { $0 }
        var spans = [
            CellSpan(row.needsTriage ? "●" : "", cells: markerCells, foreground: .yellow),
            CellSpan(row.title, cells: cells - markerCells - countCells - chipCells, foreground: .white),
        ]
        if !count.isEmpty {
            spans.append(CellSpan(count, cells: countCells, foreground: .yellow))
        }
        spans.append(CellSpan(chip, cells: chipCells, foreground: decided ? .green : .cyan))
        let line = CellLine(spans, accessibilityText: spoken.joined(separator: ", "))
```

The rest of `feedLine` (`return Line(id: "feed-\(row.feedId)", …)`) is unchanged. Without a count, the spans are the same three as before.

- [ ] **Step 6: Run them to see them pass**

```bash
cd apps/macos
xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/GardenModelTests -only-testing:RessacTests/GardenPageTests 2>&1 | grep -E 'error:|Test run with|\*\* TEST'
cd ../..
```

Expected: `** TEST SUCCEEDED **`, `0` failures.

- [ ] **Step 7: Commit**

```bash
git add apps/macos/Ressac/Garden/GardenModel.swift apps/macos/Ressac/Garden/GardenTable.swift apps/macos/Ressac/Antiope/GardenPage.swift apps/macos/RessacTests/GardenTests.swift apps/macos/RessacTests/AntiopeGardenTests.swift
git commit -F - <<'EOF'
Garden: buried rows show their unread count in both looks

"12 unread" between the clue and the chip in the native Garden; a yellow
count before the chip on P802, read by VoiceOver. Active rows show none:
the sidebar has their counts.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Phase 4b verification gate

- [ ] **Step 1: Run the Rust gate, then the Swift gate**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace 2>&1 | grep -E '^test result|FAILED|panicked'
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test 2>&1 | tee "$TMPDIR/ressac-4b.log" | grep -E 'Test run with|\*\* TEST'
grep -cE '\.swift:[0-9]+:[0-9]+: (warning|error)' "$TMPDIR/ressac-4b.log"
cd ../..
```

Expected: the Rust counts of Phase 4a (no Rust change here); `✔ Test run with 323 tests in 79 suites passed`, `** TEST SUCCEEDED **`; the `grep -c` prints `0`.

- [ ] **Step 2: Look at it (manual, for Clément; skip it when no one can look at the screen).** Bury a feed that has unread entries in a copy of the database, never the real one (`RESSAC_DB` or the app's database setting pointed at a temp file), and open the Cemetery in both looks: the native row reads "N unread" before "buried"; P802's row has a yellow count before the green "buried", and stays on one line.

- [ ] **Step 3: Stop.** Report the gate output and wait for Clément's approval before the next phase.

---

## Notes for the executor

- **Validated in a scratch copy** (`855cd2c` plus Phases 2 and 3, without Phase 1): every edit above was applied; the Swift test of Task 4.1 failed to compile as written before Step 5. Re-checked on 15/10/2026 after Phases 1, 2, 3a, 3b and 3c applied in order: the full Xcode test run passed with 320 tests before this phase (318 at `855cd2c`, plus Phase 3c's two), 321 after 4a and 323 after 4b, with no Swift warning.
- **Decided by Clément on 15/10/2026:** keep Phases 4a and 4b as written: the Cemetery shows the counts.
- **`TODO.md`** (for the plan's final docs phase), « Reporté ▸ Cœur et CLI »: tick "`v_entry` et `v_unread_count` cachent les flux enterrés…", noting that the views stay active-only and that the Cemetery counts on `entry`.
- **Out of scope:** reading a buried feed's entries (they stay out of every reading list; restoring the feed with `k` brings them back); a count on P801's dimmed buried rows comes for free, since they are the Cemetery query's rows.
