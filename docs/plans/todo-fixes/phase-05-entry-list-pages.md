# Phase 05: Entry lists load one page at a time

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Runs after:** Phase 04 (4a and 4b), after Phases 01 to 03 (3c included). Phase 04 edits `Queries.garden` in `Queries.swift` and `GardenRow` in `Records.swift`; this phase edits other parts of both files. No earlier phase touches `AppState.swift`, `DataTests.swift` or `AppStateTests.swift`. Line numbers below are those of `855cd2c` (`todo-fixes` before Phase 01), so match on the quoted text, not on the number; checked on 15/10/2026 with Phases 01 to 04 applied first, every quoted text matches. Phase 06 builds on this phase (`entryTotal`, `entryRowAppeared`). Phase 07 later edits `Launcher` in `AppState.swift` and `LauncherTests` in `AppStateTests.swift`: this phase makes `AppState.swift` 770 lines long (`Launcher` moves from line 600 to 702) and appends `ListPageTests` after `LauncherTests`; Phase 07's numbers and Step 0 count are those after this phase.

**Goal:** TODO « Reporté ▸ Lecture », first item: "the entry list has no limit and is re-read on every read mark: a folder of several thousand entries in All mode will feel it". A list now loads 500 rows, then 500 more on demand, and the "read" refetch reads that window plus two counts instead of the whole folder. `EntryRow` is decoded by hand, which divides the decode cost by nine. The selection is never pushed out of the list, the list keeps the counts of the whole list (for the sub-page line, Mark All as Read and Space), and the sidebar counts are untouched. Antiope's entry column shares `AppState.entries`, so it gets the same pages; its load-more hook and the "412 3/20" count come in Phase 06.

**Tech stack:** Swift 6.4, SwiftUI, GRDB 7.11.1 (`ValueObservation`), Swift Testing.

## What the cost actually is

Measured on 15/10/2026 on a copy of the real database (10,221 entries, never pruned) and with a release build of the current `Records.swift` and `Queries.swift` against GRDB 7.11.1:

| What | Rows | Cost |
|---|---|---|
| Infos (page 400, folder 5 and its subfolders), All: list SQL alone | 5,907 | 29 ms |
| The same list, decoded into `EntryRow` through `Codable` | 5,907 | **213 ms** (36 µs a row) |
| The same, decoded by hand (`init(row:)`) | 5,907 | 23 ms |
| Unread list, `Codable` | 1,024 | 36 ms |
| Sidebar snapshot (feeds, unread, today, starred counts) | | 19 ms |
| One 500-row page of Infos plus its two counts (this phase) | 500 | 1.4 ms + 3.6 ms SQL |
| SwiftUI update of one changed row, native `List` | 300 / 6,000 | 7 ms / 8 ms |
| The same, Antiope `LazyVStack` | 6,000 | 2.5 ms |

What happens on one "read" mark (`j`, Space, the 1 s auto-read, `m`):

1. `AppState.update` flips the row in `entries` at once (optimistic), and SwiftUI diffs the list.
2. `Writes.setRead` writes `entry.read_at`. GRDB re-runs every observation that reads `entry`: the list, the sidebar and the selected entry. The list observation fetches **the whole list** again: 213 ms of CPU for Infos in All, off the main thread.
3. The fresh list is assigned to `entries` again, although it equals what step 1 made, so SwiftUI diffs the list a second time.

Two more costs follow from the same unbounded query. Every list start (source, filter, search, midnight) runs that fetch **synchronously on the main thread** (`scheduling: .immediate`), so opening page 400 stalls the window for over 200 ms. And Infos grows by about 2,000 entries a month, so every one of these costs keeps growing.

The SwiftUI diff is nearly flat in the row count (7 ms at 300 rows, 8 ms at 6,000): it is not the problem. The fetch and the `Codable` decode are, and both scale with the rows read.

## Design

- **A list loads one page of 500 rows** (`AppState.listPageSize`). It starts with the newest page and stops at a *floor*, the `(published_at, entry_id)` key of its oldest loaded row (`ListFloor`). The observation reads the rows at or after the floor, plus `COUNT(*)` and the unread count of the whole list (`EntryPage`).
- **The floor only moves down while a list is shown.** `loadMoreEntries()` lowers it by one page; new entries from a refresh sort above it and are simply added. So a refresh never pushes a loaded row (the selection) out. When a list restarts (filter, search, midnight), its first page is lowered to keep the selected entry loaded.
- **A short list stays whole**, with no floor. If a refresh then makes it longer than a page, which happens on the first run, it is paged again on the next turn, keeping the selection.
- **The keyboard reaches the rows that are not loaded.** `j` on the last loaded row loads the next page first. Space looks for the next unread entry page by page, and only while the list's unread count says that unread entries remain beyond the loaded rows. The list views call `entryRowAppeared(_:)` as rows appear; it loads one page per turn when a row among the last 20 appears. The views are wired in Phase 06.
- **The counts are the whole list's.** `entryTotal` is the list size plus the entries read during this visit that an unread-only list keeps. `listUnread` is the unread count, and `hasMoreEntries` says whether rows remain to load. Mark All as Read stays available while unread rows remain beyond the page. Its SQL already covers the whole source (`Writes.markAllRead` uses `Queries.predicate`). The sidebar's counts come from their own query and do not change.
- **The second, identical assignment is skipped.** `entries` is set only when the merged rows differ.
- **`EntryRow` decodes by hand**: 213 ms becomes 23 ms for 5,907 rows. That still matters for a list grown by several pages, and for the Unread list.

Out of scope: moving the sidebar counts off `v_entry`, which costs 19 ms per write and is unchanged; Antiope's large-folder scroll hitches, which are postponed; and the NSTableView reentrancy warning, which needs a trace first. This phase does not touch the views, so the `List` and the Antiope column only reach rows past the first page by keyboard until Phase 06 wires `entryRowAppeared(_:)`, and between the two phases the Antiope sub-page line counts the loaded rows.

## Global constraints

- At most 5 files: `Records.swift`, `Queries.swift`, `AppState.swift`, `DataTests.swift`, `AppStateTests.swift`.
- Tests use `TestDatabase` (a temp copy of `tests/fixtures/contract.sqlite`), never the real database. No simulated keystrokes.
- Zero warnings: the app target builds with `SWIFT_TREAT_WARNINGS_AS_ERRORS`.
- English for code, comments and messages. Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review focus

1. **A refresh while the selection is the last loaded row**: the row stays listed and selected, and the new entries appear above it (`new_entries_never_push_the_loaded_rows_out`).
2. **Toggling Unread Only, or midnight, while the selection sits on page two or later**: the list restarts on one page but keeps the selection loaded (`the_selection_stays_loaded_when_the_list_restarts`).
3. **First run**: an empty Unread list filled by thousands of entries in one refresh comes back to one page (`a_short_list_made_long_by_a_refresh_is_paged_again`).
4. **Space with only read rows loaded and an unread entry further down**: the unread entry is found; with none left, no page is loaded for nothing (`space_looks_for_unread_entries_in_the_rows_not_loaded`).
5. **Mark All as Read when every loaded row is read but older rows are not**: the command stays available and marks the whole source (`mark_all_read_counts_the_rows_not_loaded`).

---

### Task 5.0: Step 0, dead-code audit of `AppState.swift`

`AppState.swift` has 668 lines and this phase restructures its list handling, so Step 0 applies. `Queries.swift`, `Records.swift` and the two test files are either under 300 lines or only receive additions.

- [ ] **Step 1: Look for debug output, markers and unused members**

```bash
cd /Users/willow/Sites/ressac
wc -l apps/macos/Ressac/App/AppState.swift
grep -nE 'print\(|debugPrint|NSLog|dump\(|TODO|FIXME' apps/macos/Ressac/App/AppState.swift
for name in $(grep -oE '(func|var|let) [a-zA-Z_][a-zA-Z0-9_]*' apps/macos/Ressac/App/AppState.swift | awk '{print $2}' | sort -u); do
  uses=$(grep -rw "$name" apps/macos/Ressac apps/macos/RessacTests | grep -vc "\(func\|var\|let\) $name\b")
  [ "$uses" -eq 0 ] && echo "unused: $name"
done
```

Expected: `668 apps/macos/Ressac/App/AppState.swift` (no earlier phase of this plan changes it), and neither the `grep` nor the loop prints anything. All three imports are used: `AppKit` (`NSWorkspace`, `NSWindow`), `GRDB` (`AnyDatabaseCancellable`) and `Observation` (`@Observable`). No Step 0 commit.

If something is printed, remove it in its own commit (`git commit -m "AppState: Step 0, remove dead code"` plus the trailer) before Task 5.1.

---

### Task 5.1: Decode `EntryRow` by hand

**Files:**
- Modify: `apps/macos/Ressac/Data/Records.swift:59-62`
- Test: `apps/macos/RessacTests/DataTests.swift` (new test before `:206`)

**Interfaces:**
- Produces: `EntryRow.init(row: Row) throws`, which GRDB uses for every `EntryRow` fetch in place of the `Decodable` default. A `published_at` that is not RFC 3339 throws `DatabaseError(resultCode: .SQLITE_MISMATCH, message: "published_at is not an RFC 3339 date: <value>")`.

The decoded values do not change; `entry_rows_decode_from_v_entry` and `list_rows_are_shortened_but_entry_is_complete` already pin the list columns. The new test pins every column of the full row, and the one observable difference: the error for a malformed date.

- [ ] **Step 1: Write the failing test**

In `apps/macos/RessacTests/DataTests.swift`, insert before this line (`:206`):

```swift
    @Test func list_rows_are_shortened_but_entry_is_complete() async throws {
```

the new test:

```swift
    /// `EntryRow` decodes by hand (Codable cost 36 µs a row): every column, and a clear error.
    @Test func entry_rows_decode_every_column_by_hand() async throws {
        let test = try TestDatabase()
        let id = try test.entry(feed: 2, title: "T", url: "https://example.org/t", read: true, starred: true)
        try test.execute("UPDATE entry SET author = 'Ann', content_html = '<p>C</p>' WHERE id = ?", [id])
        try test.execute(
            "INSERT INTO entry_fulltext (entry_id, html, fetched_at) VALUES (?, '<p>F</p>', '2026-10-10T08:00:00.000Z')", [id])
        let db = try test.open()
        let row = try #require(try await value(of: Queries.entry(id: id), in: db))
        #expect(row == EntryRow(entryId: id, feedId: 2, feedTitle: "Orange Drenka", folderId: 2, title: "T", author: "Ann",
                                url: "https://example.org/t", publishedAt: Timestamp.date(from: "2026-10-01T08:00:00.000Z")!,
                                isRead: true, isStarred: true, hasFulltext: true, summaryHtml: "<p>Summary</p>", contentHtml: "<p>C</p>"))
        try test.execute("UPDATE entry SET published_at = 'yesterday' WHERE id = ?", [id])
        let error = await #expect(throws: DatabaseError.self) {
            try await db.reader.read { try EntryRow.fetchOne($0, sql: "SELECT * FROM v_entry WHERE entry_id = ?", arguments: [id]) }
        }
        #expect(error?.message == "published_at is not an RFC 3339 date: yesterday")
    }

```

(Feed 2 of the fixture is "Orange Drenka", in folder 2.)

- [ ] **Step 2: Run it and watch it fail**

```bash
cd /Users/willow/Sites/ressac/apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:'RessacTests/QueryTests/entry_rows_decode_every_column_by_hand()'
```

Expected: FAIL, `expected error of type DatabaseError, but "could not decode Date from database value "yesterday" - column: "published_at" …" of type RowDecodingError was thrown instead`, then `error?.message == …` fails.

- [ ] **Step 3: Implement**

In `apps/macos/Ressac/Data/Records.swift`, replace lines 59-62:

```swift
    var link: URL? {
        url.flatMap(URL.init(string:)).flatMap { $0.scheme == "http" || $0.scheme == "https" ? $0 : nil }
    }
}
```

with:

```swift
    var link: URL? {
        url.flatMap(URL.init(string:)).flatMap { $0.scheme == "http" || $0.scheme == "https" ? $0 : nil }
    }
}

extension EntryRow {
    /// Decoded by hand, not through `Codable` (snake-case keys, custom dates): 5,907 list rows
    /// took 213 ms that way and take 23 ms this way, the fetch included, at every refetch.
    init(row: Row) throws {
        guard let published = (row["published_at"] as String?).flatMap(Timestamp.date(from:)) else {
            throw DatabaseError(resultCode: .SQLITE_MISMATCH,
                                message: "published_at is not an RFC 3339 date: \(row["published_at"] as String? ?? "NULL")")
        }
        self.init(entryId: row["entry_id"], feedId: row["feed_id"], feedTitle: row["feed_title"], folderId: row["folder_id"],
                  title: row["title"], author: row["author"], url: row["url"], publishedAt: published,
                  isRead: row["is_read"], isStarred: row["is_starred"], hasFulltext: row["has_fulltext"],
                  summaryHtml: row["summary_html"], contentHtml: row["content_html"])
    }
}
```

It goes in an extension so that the struct keeps its memberwise initializer, which the tests use. A type's own `init(row:)` takes precedence over the default that `FetchableRecord` gives `Decodable` types.

- [ ] **Step 4: Run the query tests**

```bash
cd /Users/willow/Sites/ressac/apps/macos && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/QueryTests
```

Expected: PASS, all tests of the suite.

- [ ] **Step 5: Commit**

```bash
cd /Users/willow/Sites/ressac
git add apps/macos/Ressac/Data/Records.swift apps/macos/RessacTests/DataTests.swift
git commit -m "$(cat <<'EOF'
Entry list: decode EntryRow by hand (213 ms to 23 ms for 5,907 rows)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 5.2: Paged list queries with the whole list's counts

**Files:**
- Modify: `apps/macos/Ressac/Data/Queries.swift:29` (new types before it), `:72-98` (`entries`, `search`), `:184` (private helpers before it)
- Modify: `apps/macos/Ressac/App/AppState.swift:515` (one line, so that the app still builds)
- Test: `apps/macos/RessacTests/DataTests.swift:193-194`, `:212`, `:270`, `:286`, `:347-352`, and a new test before `:280`

**Interfaces:**
- Produces:
  - `struct ListFloor: Comparable, Sendable { var publishedAt: String; var entryId: Int64; static let oldest; func admits(_ row: EntryRow) -> Bool }`
  - `struct EntryPage: Equatable, Sendable { var rows: [EntryRow]; var total: Int; var unread: Int }`
  - `Queries.entries(source:unreadOnly:floor: ListFloor? = nil, now:) -> LiveQuery<EntryPage>`
  - `Queries.search(_:in:floor: ListFloor? = nil, now:) -> LiveQuery<EntryPage>`
  - `Queries.floor(source:unreadOnly:search: String?, below: ListFloor?, rows: Int, keeping: Int64? = nil, now:, in db: Database) throws -> ListFloor?`
- Consumes: `Queries.predicate(for:now:)`, `Queries.ftsQuery(_:)`, `Queries.listColumns` (unchanged).

`(published_at, entry_id) >= (?, ?)` is SQLite's row-value comparison, the same order as the lists' `ORDER BY published_at DESC, entry_id DESC`. The floor keeps the raw `published_at` text, so the comparison is SQLite's own. `ListFloor.oldest` (`""`, `Int64.min`) sorts below every row: it marks a list whose last page has been loaded, as distinct from a list that was short enough to load whole (`nil`).

- [ ] **Step 1: Write the failing tests**

In `apps/macos/RessacTests/DataTests.swift`:

Replace lines 193-194:

```swift
        let rows = try await value(of: Queries.entries(source: .feed(1), unreadOnly: false), in: test.open())
        let row = try #require(rows.first)
```

with:

```swift
        let page = try await value(of: Queries.entries(source: .feed(1), unreadOnly: false), in: test.open())
        #expect((page.total, page.unread) == (1, 1))
        let row = try #require(page.rows.first)
```

Replace line 212:

```swift
        #expect(listed.first?.summaryHtml?.count == Queries.listSummaryLength)
```

with:

```swift
        #expect(listed.rows.first?.summaryHtml?.count == Queries.listSummaryLength)
```

Replace line 270:

```swift
            try await value(of: Queries.entries(source: source, unreadOnly: unreadOnly), in: db).map(\.entryId)
```

with:

```swift
            try await value(of: Queries.entries(source: source, unreadOnly: unreadOnly), in: db).rows.map(\.entryId)
```

Replace line 286:

```swift
            Set(try await value(of: Queries.search(text, in: source), in: db).map(\.entryId))
```

with:

```swift
            Set(try await value(of: Queries.search(text, in: source), in: db).rows.map(\.entryId))
```

Replace lines 347-348:

```swift
        #expect(try await value(of: observation, in: db).isEmpty)
        let seen = Task { try await value(of: observation, in: db, where: { $0.count == 1 }) }
```

with:

```swift
        #expect(try await value(of: observation, in: db).rows.isEmpty)
        let seen = Task { try await value(of: observation, in: db, where: { $0.rows.count == 1 }) }
```

Replace line 352:

```swift
        #expect(try await seen.value.count == 1)
```

with:

```swift
        #expect(try await seen.value.rows.count == 1)
```

Then insert before this line (`:280`):

```swift
    @Test func search_matches_every_word_within_the_source() async throws {
```

the new test:

```swift
    /// A long list loads down to its floor; the counts are the whole list's.
    @Test func entry_pages_stop_at_the_floor_and_count_the_whole_list() async throws {
        let test = try TestDatabase()
        let oldest = try test.entry(feed: 1, published: "2026-10-01T08:00:00.000Z", read: true)
        let middle = try test.entry(feed: 1, published: "2026-10-02T08:00:00.000Z", body: "cafe")
        let newest = try test.entry(feed: 1, published: "2026-10-03T08:00:00.000Z", body: "cafe")
        let db = try test.open()
        func floor(below: ListFloor? = nil, rows: Int, keeping: Int64? = nil, search: String? = nil) async throws -> ListFloor? {
            try await db.reader.read { db in
                try Queries.floor(source: .feed(1), unreadOnly: false, search: search, below: below, rows: rows,
                                  keeping: keeping, in: db)
            }
        }
        let first = try #require(try await floor(rows: 2))
        #expect(first == ListFloor(publishedAt: "2026-10-02T08:00:00.000Z", entryId: middle))
        let page = try await value(of: Queries.entries(source: .feed(1), unreadOnly: false, floor: first), in: db)
        #expect(page.rows.map(\.entryId) == [newest, middle])
        #expect((page.total, page.unread) == (3, 2))
        // One row left below the floor: it fits in the next page, so there is no floor.
        #expect(try await floor(below: first, rows: 2) == nil)
        #expect(try await floor(below: first, rows: 1) == ListFloor(publishedAt: "2026-10-01T08:00:00.000Z", entryId: oldest))
        let whole = try await value(of: Queries.entries(source: .feed(1), unreadOnly: false, floor: .oldest), in: db)
        #expect(whole.rows.map(\.entryId) == [newest, middle, oldest])
        #expect(ListFloor.oldest.admits(try #require(whole.rows.last)))
        // Keeping an older entry loaded lowers the floor to it; a newer one changes nothing.
        #expect(try await floor(rows: 1, keeping: oldest)?.entryId == oldest)
        #expect(try await floor(rows: 2, keeping: newest) == first)
        // Search results are paged the same way.
        #expect(try await floor(rows: 1, search: "cafe")?.entryId == newest)
        let found = try await value(of: Queries.search("cafe", in: .feed(1), floor: try await floor(rows: 1, search: "cafe")), in: db)
        #expect(found.rows.map(\.entryId) == [newest])
        #expect((found.total, found.unread) == (2, 2))
        #expect(ListFloor(publishedAt: "2026-10-02T08:00:00.000Z", entryId: middle).admits(page.rows[0]))
        #expect(!first.admits(try #require(try await value(of: Queries.entries(source: .feed(1), unreadOnly: false), in: db).rows.last)))
    }

```

- [ ] **Step 2: Run them and watch them fail**

```bash
cd /Users/willow/Sites/ressac/apps/macos && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/QueryTests
```

Expected: the test target does not build: `value of type '[EntryRow]' has no member 'rows'`, `cannot find type 'ListFloor' in scope`, `extra argument 'floor' in call`, `type 'Queries' has no member 'floor'`.

- [ ] **Step 3: Implement the queries**

In `apps/macos/Ressac/Data/Queries.swift`, replace line 29:

```swift
/// A live query: refetched when GRDB sees a change, duplicates dropped.
```

with:

```swift
/// Where a long entry list stops: the `published_at` text and the id of its oldest loaded row.
/// Rows at or after it (the lists run newest first) are loaded. The raw text, so that SQLite
/// compares it the way it sorts the column.
struct ListFloor: Comparable, Sendable {
    var publishedAt: String
    var entryId: Int64

    /// Below every row: the whole list, loaded to its end on purpose.
    static let oldest = ListFloor(publishedAt: "", entryId: .min)

    static func < (a: ListFloor, b: ListFloor) -> Bool {
        (a.publishedAt, a.entryId) < (b.publishedAt, b.entryId)
    }

    /// Whether `row` sorts at or after the floor.
    func admits(_ row: EntryRow) -> Bool {
        guard let date = Timestamp.date(from: publishedAt) else { return true }
        return (row.publishedAt, row.entryId) >= (date, entryId)
    }
}

/// The loaded rows of an entry list, newest first, and the size of the whole list.
struct EntryPage: Equatable, Sendable {
    var rows: [EntryRow] = []
    /// Rows of the whole list, loaded or not.
    var total = 0
    /// Unread rows of the whole list, loaded or not.
    var unread = 0
}

/// A live query: refetched when GRDB sees a change, duplicates dropped.
```

Replace lines 72-98:

```swift
    /// The entry list of a source, newest first. Garden sources have no entries.
    static func entries(source: Source, unreadOnly: Bool, now: Date = Date()) -> LiveQuery<[EntryRow]> {
        ValueObservation.tracking { db in
            guard let filter = predicate(for: source, now: now) else { return [] }
            let unread = unreadOnly ? " AND NOT is_read" : ""
            return try EntryRow.fetchAll(db, sql: """
                SELECT \(listColumns) FROM v_entry
                WHERE \(filter.sql)\(unread)
                ORDER BY published_at DESC, entry_id DESC
                """, arguments: filter.arguments)
        }
        .removeDuplicates()
    }

    /// Full-text search within a source (spec §7, Search). Every word must appear (see `ftsQuery`).
    static func search(_ text: String, in source: Source, now: Date = Date()) -> LiveQuery<[EntryRow]> {
        ValueObservation.tracking { db in
            guard let match = ftsQuery(text), let filter = predicate(for: source, now: now) else { return [] }
            return try EntryRow.fetchAll(db, sql: """
                SELECT \(listColumns) FROM v_entry
                WHERE entry_id IN (SELECT rowid FROM entry_fts WHERE entry_fts MATCH ?)
                  AND \(filter.sql)
                ORDER BY published_at DESC, entry_id DESC
                """, arguments: [match] + filter.arguments)
        }
        .removeDuplicates()
    }
```

with:

```swift
    /// The entry list of a source, newest first, down to `floor` (every row when nil). Garden
    /// sources have no entries.
    static func entries(source: Source, unreadOnly: Bool, floor: ListFloor? = nil, now: Date = Date()) -> LiveQuery<EntryPage> {
        page(of: condition(source: source, unreadOnly: unreadOnly, search: nil, now: now), floor: floor)
    }

    /// Full-text search within a source (spec §7, Search), down to `floor` like `entries`. Every
    /// word must appear (see `ftsQuery`).
    static func search(_ text: String, in source: Source, floor: ListFloor? = nil, now: Date = Date()) -> LiveQuery<EntryPage> {
        page(of: condition(source: source, unreadOnly: false, search: text, now: now), floor: floor)
    }

    /// The floor that loads `rows` more rows of a list after `below` (from its newest row when
    /// nil), lowered to entry `keeping` when that entry sorts older. Nil when fewer rows are
    /// left: they all load. `search` nil is the plain list.
    static func floor(source: Source, unreadOnly: Bool, search: String?, below: ListFloor?, rows: Int,
                      keeping entryId: Int64? = nil, now: Date = Date(), in db: Database) throws -> ListFloor? {
        guard let list = condition(source: source, unreadOnly: unreadOnly, search: search, now: now) else { return nil }
        let rest = bounded(list, by: below, "<")
        guard let row = try Row.fetchOne(db, sql: """
            SELECT published_at, entry_id FROM v_entry WHERE \(rest.sql)
            ORDER BY published_at DESC, entry_id DESC LIMIT 1 OFFSET ?
            """, arguments: rest.arguments + [max(rows, 1) - 1])
        else { return nil }
        let floor = ListFloor(publishedAt: row["published_at"], entryId: row["entry_id"])
        guard let entryId, let kept = try Row.fetchOne(db, sql: "SELECT published_at, id FROM entry WHERE id = ?", arguments: [entryId])
        else { return floor }
        return min(floor, ListFloor(publishedAt: kept["published_at"], entryId: kept["id"]))
    }
```

Insert before line 184:

```swift
    /// `"word1" "word2"`: every token quoted, so FTS5 operators typed by the user stay literal.
```

the private helpers:

```swift
    /// The `v_entry` rows of a list: a source's, unread only or not, or those matching `search`
    /// (which ignores the unread filter). Nil when the list is empty by definition: a garden
    /// source, a search without words.
    private static func condition(source: Source, unreadOnly: Bool, search: String?, now: Date) -> Predicate? {
        guard let filter = predicate(for: source, now: now) else { return nil }
        guard let search else {
            return unreadOnly ? Predicate(sql: filter.sql + " AND NOT is_read", arguments: filter.arguments) : filter
        }
        guard let match = ftsQuery(search) else { return nil }
        return Predicate(sql: "entry_id IN (SELECT rowid FROM entry_fts WHERE entry_fts MATCH ?) AND \(filter.sql)",
                         arguments: [match] + filter.arguments)
    }

    /// `list` cut at `floor` with `comparison` (`>=`: the rows at or after it; `<`: the older
    /// ones); `list` itself when there is no floor.
    private static func bounded(_ list: Predicate, by floor: ListFloor?, _ comparison: String) -> Predicate {
        guard let floor else { return list }
        return Predicate(sql: list.sql + " AND (published_at, entry_id) \(comparison) (?, ?)",
                         arguments: list.arguments + [floor.publishedAt, floor.entryId])
    }

    /// The rows of `list` down to `floor`, and the counts of the whole list.
    private static func page(of list: Predicate?, floor: ListFloor?) -> LiveQuery<EntryPage> {
        ValueObservation.tracking { db in
            guard let list else { return EntryPage() }
            let loaded = bounded(list, by: floor, ">=")
            let rows = try EntryRow.fetchAll(db, sql: """
                SELECT \(listColumns) FROM v_entry WHERE \(loaded.sql)
                ORDER BY published_at DESC, entry_id DESC
                """, arguments: loaded.arguments)
            guard let counts = try Row.fetchOne(db, sql: """
                SELECT COUNT(*) AS total, COALESCE(SUM(NOT is_read), 0) AS unread FROM v_entry WHERE \(list.sql)
                """, arguments: list.arguments)
            else { return EntryPage(rows: rows, total: rows.count) }
            return EntryPage(rows: rows, total: counts["total"], unread: counts["unread"])
        }
        .removeDuplicates()
    }

```

The conditions are the old ones: the unread filter appends ` AND NOT is_read` as before, and the search keeps its `entry_id IN (… MATCH ?) AND <source>` shape with the match argument first.

- [ ] **Step 4: Keep the app building**

In `apps/macos/Ressac/App/AppState.swift`, replace line 515:

```swift
                entries = Self.merge(fresh: fresh, previous: entries, retained: retained)
```

with:

```swift
                entries = Self.merge(fresh: fresh.rows, previous: entries, retained: retained)
```

(Task 5.3 replaces this whole function.)

- [ ] **Step 5: Run the query and app-state tests**

```bash
cd /Users/willow/Sites/ressac/apps/macos && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/QueryTests -only-testing:RessacTests/AppStateTests
```

Expected: PASS (both suites; the app still loads whole lists, since `AppState` passes no floor yet).

- [ ] **Step 6: Commit**

```bash
cd /Users/willow/Sites/ressac
git add apps/macos/Ressac/Data/Queries.swift apps/macos/Ressac/App/AppState.swift apps/macos/RessacTests/DataTests.swift
git commit -m "$(cat <<'EOF'
Entry list: pages down to a floor, with the whole list's counts

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 5.3: `AppState` loads lists one page at a time

**Files:**
- Modify: `apps/macos/Ressac/App/AppState.swift:95-96`, `:112`, `:134-136`, `:235`, `:257-281`, `:506-517`, `:584-588`
- Test: `apps/macos/RessacTests/AppStateTests.swift` (new suite at the end, after `:324`)

**Interfaces:**
- Consumes: Task 5.2's `ListFloor`, `EntryPage`, `Queries.entries(…floor:…)`, `Queries.search(…floor:…)`, `Queries.floor(…)`.
- Produces, used by Phase 06:
  - `private(set) var entryTotal: Int`: the whole list's size, plus the read rows an unread-only list keeps.
  - `private(set) var listUnread: Int`, `private(set) var hasMoreEntries: Bool`.
  - `var listPageSize: Int` (500; tests lower it), `static let loadMoreMargin = 20`.
  - `func loadMoreEntries()`: synchronous; `entries` has the next page on return.
  - `func entryRowAppeared(_ entryId: Int64)`: for the list views; loads one page on the next turn when the row is among the last `loadMoreMargin` loaded rows.
  - `static func merge(fresh:previous:retained:floor:)`, which gains `floor:`.

- [ ] **Step 1: Write the failing tests**

Append to `apps/macos/RessacTests/AppStateTests.swift`, after the closing `}` of `AppStateTests` (the last line, `:324`):

```swift

/// Long lists load one page at a time: a folder of thousands of entries in "All" is never read
/// whole, and the counts stay those of the whole list.
@MainActor @Suite struct ListPageTests {
    /// `count` entries of `feed`, one a day from 1 October; their ids, newest first.
    private func days(_ test: TestDatabase, _ count: Int, feed: Int64 = 1, read: (Int) -> Bool = { _ in false }) throws -> [Int64] {
        try (1...count).map { day in
            try test.entry(feed: feed, published: String(format: "2026-10-%02dT08:00:00.000Z", day), read: read(day))
        }.reversed()
    }

    @Test func a_long_list_loads_one_page_and_counts_the_whole_list() throws {
        let (state, test, _) = try makeState()
        let ids = try days(test, 5)
        state.listPageSize = 2
        state.selection = .feed(1)
        #expect(state.entries.map(\.entryId) == Array(ids.prefix(2)))
        #expect((state.entryTotal, state.listUnread, state.hasMoreEntries) == (5, 5, true))
        state.loadMoreEntries()
        #expect(state.entries.map(\.entryId) == Array(ids.prefix(4)))
        state.loadMoreEntries()
        #expect(state.entries.map(\.entryId) == ids)
        #expect(!state.hasMoreEntries)
    }

    /// What the list views report as rows appear: one page per turn, only near the end.
    @Test func rows_appearing_near_the_end_load_one_more_page() async throws {
        let (state, test, _) = try makeState()
        let ids = try days(test, 26)
        let short = try days(test, 6, feed: 2)
        state.listPageSize = 24
        state.selection = .feed(1)
        state.entryRowAppeared(ids[3])  // 21 rows from the end of the 24 loaded: not near it
        try await Task.sleep(for: .milliseconds(100))
        #expect(state.entries.count == 24)
        state.entryRowAppeared(ids[4])  // 20 rows from the end
        #expect(await eventually { state.entries.count == 26 })
        #expect(!state.hasMoreEntries)
        // Rows appearing together load one page, not one each.
        state.listPageSize = 2
        state.selection = .feed(2)
        state.entryRowAppeared(short[0])
        state.entryRowAppeared(short[1])
        #expect(await eventually { state.entries.count == 4 })
        try await Task.sleep(for: .milliseconds(100))
        #expect(state.entries.count == 4)
    }

    @Test func next_entry_past_the_loaded_rows_loads_the_next_page() throws {
        let (state, test, _) = try makeState()
        let ids = try days(test, 3)
        state.listPageSize = 2
        state.selection = .feed(1)
        state.autoReadDelay = .seconds(60)
        state.selectNext()
        state.selectNext()
        #expect(state.selectedEntryId == ids[1] && state.entries.count == 2)
        state.selectNext()
        #expect(state.selectedEntryId == ids[2])
        #expect(state.entries.count == 3 && !state.hasMoreEntries)
    }

    @Test func space_looks_for_unread_entries_in_the_rows_not_loaded() throws {
        let (state, test, _) = try makeState()
        let ids = try days(test, 5) { $0 != 1 }  // only the oldest is unread
        let allRead = try days(test, 3, feed: 2) { _ in true }
        state.listPageSize = 2
        state.autoReadDelay = .seconds(60)
        state.selection = .feed(1)
        state.selectedEntryId = ids[0]
        #expect((state.listUnread, state.entries.count) == (1, 2))
        state.selectNextUnread()
        #expect(state.selectedEntryId == ids[4])
        #expect(state.entries.count == 5)
        // Nothing unread beyond the loaded rows: no page is loaded for nothing.
        state.selection = .feed(2)
        state.selectedEntryId = allRead[0]
        state.selectNextUnread()
        #expect(state.selectedEntryId == allRead[0])
        #expect(state.entries.count == 2)
    }

    @Test func new_entries_never_push_the_loaded_rows_out() async throws {
        let (state, test, _) = try makeState()
        let ids = try days(test, 3)
        state.listPageSize = 2
        state.selection = .feed(1)
        state.autoReadDelay = .seconds(60)
        state.selectedEntryId = ids[1]  // the last loaded row
        let newer = try (20...23).map { try test.entry(feed: 1, published: "2026-10-\($0)T08:00:00.000Z") }
        try await state.database.notifyExternalChanges()
        #expect(await eventually { state.entryTotal == 7 })
        #expect(state.entries.map(\.entryId) == Array(newer.reversed()) + Array(ids.prefix(2)))
        #expect(state.selectedEntryId == ids[1] && state.hasMoreEntries)
    }

    /// The first run: the list was empty, so whole, when the first refresh filled it.
    @Test func a_short_list_made_long_by_a_refresh_is_paged_again() async throws {
        let (state, test, _) = try makeState()
        state.listPageSize = 2
        state.selection = .feed(1)
        #expect(state.entries.isEmpty && !state.hasMoreEntries)
        let ids = try days(test, 5)
        try await state.database.notifyExternalChanges()
        #expect(await eventually { state.entries.count == 2 })
        #expect(state.entries.map(\.entryId) == Array(ids.prefix(2)))
        #expect((state.entryTotal, state.hasMoreEntries) == (5, true))
    }

    @Test func the_selection_stays_loaded_when_the_list_restarts() throws {
        let (state, test, _) = try makeState()
        let ids = try days(test, 4)
        state.listPageSize = 2
        state.selection = .feed(1)
        state.autoReadDelay = .seconds(60)
        state.loadMoreEntries()
        state.selectedEntryId = ids[3]  // the oldest, on the second page
        state.unreadOnly = true
        #expect(state.entries.map(\.entryId) == ids)
        #expect(!state.hasMoreEntries)
    }

    @Test func mark_all_read_counts_the_rows_not_loaded() async throws {
        let (state, test, _) = try makeState()
        _ = try days(test, 3) { $0 != 1 }  // the oldest, not loaded, is the only unread one
        state.listPageSize = 2
        state.selection = .feed(1)
        #expect(state.entries.allSatisfy(\.isRead) && state.canMarkAllRead)
        await state.markAllRead()?.value
        #expect(try test.one(Int.self, "SELECT COUNT(*) FROM entry WHERE read_at IS NULL") == 0)
        #expect(await eventually { !state.canMarkAllRead })
    }
}
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cd /Users/willow/Sites/ressac/apps/macos && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/ListPageTests
```

Expected: the test target does not build: `value of type 'AppState' has no member 'listPageSize'` (and `entryTotal`, `listUnread`, `hasMoreEntries`, `loadMoreEntries`, `entryRowAppeared`).

- [ ] **Step 3: State (re-read `AppState.swift` first)**

Replace lines 95-96:

```swift
    /// The entry list, or the search results while `searchText` is not empty.
    private(set) var entries: [EntryRow] = []
```

with:

```swift
    /// The entry list, or the search results while `searchText` is not empty: its loaded rows,
    /// newest first. A long list loads `listPageSize` rows at a time (`loadMoreEntries`).
    private(set) var entries: [EntryRow] = []
    /// The whole list's size, loaded or not, with the entries read during this visit that it
    /// keeps: the "/20" of the sub-page line.
    private(set) var entryTotal = 0
    /// Unread entries of the whole list, loaded or not.
    private(set) var listUnread = 0
    /// Rows of the list are not loaded yet.
    private(set) var hasMoreEntries = false
```

Replace line 112:

```swift
    @ObservationIgnored var autoReadDelay: Duration = .seconds(1)
```

with:

```swift
    @ObservationIgnored var autoReadDelay: Duration = .seconds(1)
    /// Rows a list loads at a time. A list starts with one page and grows by one per `loadMoreEntries`.
    @ObservationIgnored var listPageSize = 500
    /// Rows from the end of the loaded ones whose appearance loads the next page (`entryRowAppeared`).
    static let loadMoreMargin = 20
```

Replace lines 134-136:

```swift
    /// Entries read during this visit of the source: they stay listed in unread-only
    /// lists until the source changes, so the selection never vanishes.
    @ObservationIgnored private var retained: Set<Int64> = []
```

with:

```swift
    /// Entries read during this visit of the source: they stay listed in unread-only
    /// lists until the source changes, so the selection never vanishes.
    @ObservationIgnored private var retained: Set<Int64> = []
    /// The list's oldest loaded row. It only moves down while the list is shown, so new entries
    /// never push a loaded row (the selection) out. Nil: the list was short enough to load whole
    /// when it started; `.oldest`: its last page was loaded.
    @ObservationIgnored private var listFloor: ListFloor?
    @ObservationIgnored private var isLoadMoreScheduled = false
```

Read the three places back to check them.

- [ ] **Step 4: Commands (re-read `AppState.swift` first)**

Replace line 235:

```swift
    var canMarkAllRead: Bool { !selection.isGarden && searchText.isEmpty && entries.contains { !$0.isRead } }
```

with:

```swift
    var canMarkAllRead: Bool {
        !selection.isGarden && searchText.isEmpty && (entries.contains { !$0.isRead } || (hasMoreEntries && listUnread > 0))
    }
```

Replace lines 257-281:

```swift
    func selectNext() {
        guard canNavigateEntries else { return }
        let index = selectedEntryId.flatMap { id in entries.firstIndex { $0.entryId == id } }
        let next = index.map { $0 + 1 } ?? 0
        guard next < entries.count else { return }
        markSelectedReadOnLeaving()
        selectedEntryId = entries[next].entryId
    }

    func selectPrevious() {
        guard canNavigateEntries else { return }
        let index = selectedEntryId.flatMap { id in entries.firstIndex { $0.entryId == id } }
        let previous = index.map { $0 - 1 } ?? 0
        guard previous >= 0, previous < entries.count else { return }
        selectedEntryId = entries[previous].entryId
    }

    /// The next unread entry after the selection (spec §7, Space at the bottom).
    func selectNextUnread() {
        guard canNavigateEntries else { return }
        let start = selectedEntryId.flatMap { id in entries.firstIndex { $0.entryId == id } }.map { $0 + 1 } ?? 0
        guard let next = entries[start...].first(where: { !$0.isRead && $0.entryId != selectedEntryId }) else { return }
        markSelectedReadOnLeaving()
        selectedEntryId = next.entryId
    }
```

with:

```swift
    /// The next entry; past the last loaded row, the next page first.
    func selectNext() {
        guard canNavigateEntries else { return }
        if let index = selectedIndex, index + 1 == entries.count { loadMoreEntries() }
        let next = selectedIndex.map { $0 + 1 } ?? 0
        guard next < entries.count else { return }
        markSelectedReadOnLeaving()
        selectedEntryId = entries[next].entryId
    }

    func selectPrevious() {
        guard canNavigateEntries else { return }
        let previous = selectedIndex.map { $0 - 1 } ?? 0
        guard previous >= 0, previous < entries.count else { return }
        selectedEntryId = entries[previous].entryId
    }

    /// The next unread entry after the selection (spec §7, Space at the bottom), loading pages
    /// while the list has unread entries that are not loaded.
    func selectNextUnread() {
        guard canNavigateEntries else { return }
        while true {
            let start = selectedIndex.map { $0 + 1 } ?? 0
            if let next = entries[start...].first(where: { !$0.isRead && $0.entryId != selectedEntryId }) {
                markSelectedReadOnLeaving()
                selectedEntryId = next.entryId
                return
            }
            let loaded = entries.count
            guard hasMoreEntries, listUnread > entries.count(where: { !$0.isRead }) else { return }
            loadMoreEntries()
            guard entries.count > loaded else { return }
        }
    }

    /// A list view shows the row of `entryId`. Among the last `loadMoreMargin` loaded rows, the
    /// next page loads, on the next turn: a row appears while its table lays out, where changing
    /// the rows is a reentrant operation. One page per turn, however many rows appear.
    func entryRowAppeared(_ entryId: Int64) {
        guard !isLoadMoreScheduled, hasMoreEntries,
              entries.suffix(Self.loadMoreMargin).contains(where: { $0.entryId == entryId })
        else { return }
        isLoadMoreScheduled = true
        Task { [weak self] in
            guard let self else { return }
            isLoadMoreScheduled = false
            loadMoreEntries()
        }
    }

    /// Loads the next `listPageSize` rows of the list. `entries` has them on return.
    func loadMoreEntries() {
        guard hasMoreEntries, let current = listFloor else { return }
        do {
            listFloor = try floor(below: current, keeping: nil) ?? .oldest
        } catch {
            report(error)
            return
        }
        observeList()
    }
```

The three copies of the selected-index lookup become one private property, `selectedIndex`, added in Step 5.

Read the two places back to check them.

- [ ] **Step 5: The list's observation (re-read `AppState.swift` first)**

Replace lines 506-517, as Task 5.2 left them:

```swift
    private func listDidChange() {
        let observation = searchText.trimmingCharacters(in: .whitespaces).isEmpty
            ? Queries.entries(source: selection, unreadOnly: unreadOnly, now: now())
            : Queries.search(searchText, in: selection, now: now())
        listObservation = observation.start(
            in: database.reader, scheduling: .immediate,
            onError: { [weak self] in self?.report($0) },
            onChange: { [weak self] fresh in
                guard let self else { return }
                entries = Self.merge(fresh: fresh.rows, previous: entries, retained: retained)
            })
    }
```

with:

```swift
    /// The list restarts with one page, keeping the selection loaded.
    private func listDidChange() {
        do {
            listFloor = try floor(below: nil, keeping: selectedEntryId)
        } catch {
            listFloor = nil
            report(error)
        }
        observeList()
    }

    /// The search words, or nil for the plain list.
    private var activeSearch: String? {
        searchText.trimmingCharacters(in: .whitespaces).isEmpty ? nil : searchText
    }

    private var selectedIndex: Int? {
        selectedEntryId.flatMap { id in entries.firstIndex { $0.entryId == id } }
    }

    /// The floor one page below `current` (below the newest row when nil), lowered to keep entry
    /// `keeping` loaded; nil when the rest of the list fits in the page.
    private func floor(below current: ListFloor?, keeping entryId: Int64?) throws -> ListFloor? {
        let (source, unreadOnly, search, rows, now) = (selection, unreadOnly, activeSearch, listPageSize, now())
        return try database.reader.read { db in
            try Queries.floor(source: source, unreadOnly: unreadOnly, search: search, below: current, rows: rows,
                              keeping: entryId, now: now, in: db)
        }
    }

    /// (Re)starts the list's observation at `listFloor`. `.immediate`: `entries` is up to date on return.
    private func observeList() {
        let observation = activeSearch.map { Queries.search($0, in: selection, floor: listFloor, now: now()) }
            ?? Queries.entries(source: selection, unreadOnly: unreadOnly, floor: listFloor, now: now())
        listObservation = observation.start(
            in: database.reader, scheduling: .immediate,
            onError: { [weak self] in self?.report($0) },
            onChange: { [weak self] page in self?.listDidLoad(page) })
    }

    private func listDidLoad(_ page: EntryPage) {
        let merged = Self.merge(fresh: page.rows, previous: entries, retained: retained, floor: listFloor)
        // Equal after an optimistic change already made: the views are not updated twice.
        if merged != entries { entries = merged }
        entryTotal = page.total + merged.count - page.rows.count
        listUnread = page.unread
        hasMoreEntries = page.rows.count < page.total
        // A list that was whole when it started (a short one) and that a refresh made long, as on
        // the first run: one page again. On the next turn, not inside the observation's callback.
        if listFloor == nil, page.rows.count > listPageSize {
            Task { [weak self] in
                guard let self, listFloor == nil, entries.count > listPageSize else { return }
                listDidChange()
            }
        }
    }
```

Replace lines 584-588:

```swift
    /// Fresh rows, plus the entries read during this visit that an unread-only query
    /// dropped, newest first.
    static func merge(fresh: [EntryRow], previous: [EntryRow], retained: Set<Int64>) -> [EntryRow] {
        let freshIds = Set(fresh.map(\.entryId))
        let kept = previous.filter { retained.contains($0.entryId) && !freshIds.contains($0.entryId) }
```

with:

```swift
    /// Fresh rows, plus the entries read during this visit that an unread-only query
    /// dropped, newest first. Only rows at or after `floor` are kept: below it, a row of the
    /// list is merely not loaded.
    static func merge(fresh: [EntryRow], previous: [EntryRow], retained: Set<Int64>, floor: ListFloor?) -> [EntryRow] {
        let freshIds = Set(fresh.map(\.entryId))
        let kept = previous.filter {
            retained.contains($0.entryId) && !freshIds.contains($0.entryId) && floor?.admits($0) != false
        }
```

Read the two places back to check them. Why the counts hold: a kept row sorts at or after the floor and is missing from the fresh rows, so it does not match the list's query (it would be among the fresh rows otherwise). It is therefore not in `page.total`, and `entryTotal = page.total + kept` counts every listed row once.

- [ ] **Step 6: Run the new and the existing app-state tests**

```bash
cd /Users/willow/Sites/ressac/apps/macos && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/ListPageTests -only-testing:RessacTests/AppStateTests -only-testing:RessacTests/QueryTests
```

Expected: PASS (36 tests in 3 suites when this was drafted).

- [ ] **Step 7: Commit**

```bash
cd /Users/willow/Sites/ressac
git add apps/macos/Ressac/App/AppState.swift apps/macos/RessacTests/AppStateTests.swift
git commit -m "$(cat <<'EOF'
Entry list: load 500 rows at a time, keep the selection and the whole list's counts

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Verification gate

- [ ] **Rust**

```bash
cd /Users/willow/Sites/ressac
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: no output from `fmt`, no warnings, all tests pass, with the counts of Phase 4a (`ressac` 35, `cli` 19, core lib 260 + 1 ignored, `article_proxy` 1, `contract` 9, `end_to_end` 4). This phase touches no Rust.

- [ ] **Swift**

```bash
cd /Users/willow/Sites/ressac/apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test
```

Expected: `✔ Test run with 333 tests in 80 suites passed`, `** TEST SUCCEEDED **` and zero warnings (warnings are errors in the app target). This phase adds 10 tests to the 323 of Phase 4b: 2 in `QueryTests`, and 8 in the new `ListPageTests` suite. Re-checked on 15/10/2026 on a scratch copy with Phases 01 to 04 (3c included) applied first, in order: every quoted text matched, and Step 0 still finds `668` lines and nothing to remove (no earlier phase touches `AppState.swift`).

---

## Notes for the executor

- **No view changes here.** Until Phase 06, the native `List` and the Antiope column show the first 500 rows and reach later rows only by keyboard (`j` and Space load pages). The Antiope sub-page line also counts the loaded rows (`3/500` instead of `3/5907`) until Phase 06, Task 6.1.
- **`entryRowAppeared(_:)` has no caller in the app yet.** Phase 06 wires it into both list views. It is tested here.
- **Behaviour change, deliberate:** a list restarted by Unread Only, a search or midnight starts on one page again (keeping the selection), not on everything the user had loaded.
- **The sidebar observation is unchanged.** It still re-counts `v_entry` (about 19 ms, off the main thread) on every write; moving those counts to indexed columns would be a schema change and is out of scope.
- **Measurement method**, if someone wants to repeat it: copy the database with `sqlite3 "file:$HOME/Library/Application Support/Ressac/ressac.sqlite?mode=ro" ".backup /tmp/copy.sqlite"`, then fetch through GRDB 7.11.1 in a release build, never on the real file.
