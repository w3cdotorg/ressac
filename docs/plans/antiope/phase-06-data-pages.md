# Phase 6: The data layer reads page numbers (schema v3)

**Runs after:** phase 1b (the v3 contract fixture with its page numbers). It needs nothing from phases 2 to 5; phase 2 runs after it.

Read the plan index first. Global Constraints apply. Spec §4 (Data, Assignment), §10 ("Older `ressac` on a v3 database") and §11 (Swift tests). Review Focus #3 is pinned here.

**Prerequisite: phase 1b, and only phase 1b.** This phase reads `tests/fixtures/contract.sqlite` as phase 1b regenerates it: schema v3, `folder.page` and `feed.page` filled by the core's assignment, view `v_page`. It needs nothing from phases 3 to 5.

**Run it right after phase 1b, before phase 2.** From the end of phase 1a the fixture is at v3 while the app still accepts only v2, so six existing Swift tests fail with `.tooNew(3)` (listed in phase 3's Execution order) and the app cannot open a database that its own bundled `ressac` has migrated. This phase restores both; until it lands, no Swift phase can pass its gate.

**Files (5):**
- Modify: `apps/macos/Ressac/Data/Database.swift` (`knownSchemaVersion`, `check(at:)`, new `check(at:known:)`)
- Modify: `apps/macos/Ressac/Data/Records.swift` (`FeedItem`, `FolderItem`)
- Modify: `apps/macos/Ressac/Data/Queries.swift` (`Queries.sidebar`)
- Modify: `apps/macos/RessacTests/DataTests.swift`
- Modify: `apps/macos/RessacTests/AppStateTests.swift` (`LauncherTests`: a "newer" database is now v4)
- Fixture (not counted, not modified here): `tests/fixtures/contract.sqlite`, regenerated at v3, with its page numbers, by phase 1b.

**Step 0:** nothing to clean. The three source files have 90, 279 and 191 lines; the two test files are over 300 lines, but this phase only adds tests and changes two literals, it restructures nothing.

**Test command:**

```sh
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)/apps/macos" && xcodegen generate --quiet && \
  xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test \
    -only-testing:RessacTests/DatabaseTests -only-testing:RessacTests/LauncherTests \
  2>&1 | grep -E '^/.+: error:|✘|Test run|\*\* TEST'
```

**The fixture's pages.** The plan index's assignment algorithm, applied to the folders and feeds of `vienna-like.opml` (ids and positions as mac-app phase 7 lists them), gives:

| Page | Kind | Id | Title | Why |
|---|---|---|---|---|
| 200 | folder | 1 | Blogs | first root folder |
| 201 | feed | 1 | xkcd.com | Blogs, position 0 |
| 202 | folder | 2 | potes | Blogs, position 1 |
| 203 | feed | 2 | Orange Drenka | inside potes (pre-order) |
| 204 | feed | 3 | Orange-Drenka | |
| 205 | feed | 4 | Photos from bouquiniste | buried in the fixture, keeps its number |
| 206 | feed | 5 | Girl on the Couch | Blogs, position 2 |
| 300 | folder | 3 | Biblis | second root folder |
| 301 | feed | 6 | Bibliobsession 2.0 | |
| 302 | feed | 7 | Unphotographable | |
| 400 | folder | 4 | Infos | third root folder |
| 401 | feed | 8 | Daring Fireball | Infos, position 0 |
| 402 | feed | 9 | Six Colors | Infos, position 2 (position 1 was the merged duplicate) |
| 403 | folder | 5 | Science | Infos, position 3 |
| 404 | feed | 10 | Center for Humane Technology | inside Science |
| 500 | folder | 6 | Jon | fourth root folder |
| 501 | folder | 7 | YouTube | Jon, position 0 |
| 502 | feed | 11 | Center for Humane Technology (1) | inside YouTube |

### Task 6.1: Accept schema v3, refuse a newer one, and let tests play an older app

**Interfaces:**
- Consumes: the v3 contract fixture (phase 1b); `TestDatabase` and `Stub` (existing test helpers).
- Produces (`Data/Database.swift`):
  - `static let knownSchemaVersion = 3`;
  - `static func check(at url: URL, known: Int) throws -> SchemaCheck`, the comparison against any known version;
  - `static func check(at url: URL) throws -> SchemaCheck`, unchanged signature, now `check(at: url, known: knownSchemaVersion)`. Its callers (`Launcher.launch`, `DatabaseLocation.problem(with:)`) are untouched.

- [ ] **Step 1: Check the fixture phase 1b produced.** From the repository root:

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)"
sqlite3 -readonly tests/fixtures/contract.sqlite "SELECT version FROM schema_meta"
sqlite3 -readonly tests/fixtures/contract.sqlite "SELECT page || ' ' || kind || ' ' || ref_id FROM v_page ORDER BY page"
```

Expected: `3`, then the 18 lines of the table above, in order: `200 folder 1`, `201 feed 1`, `202 folder 2`, `203 feed 2`, `204 feed 3`, `205 feed 4`, `206 feed 5`, `300 folder 3`, `301 feed 6`, `302 feed 7`, `400 folder 4`, `401 feed 8`, `402 feed 9`, `403 folder 5`, `404 feed 10`, `500 folder 6`, `501 folder 7`, `502 feed 11`. **If the version is 2 or a page differs, stop:** phase 1's assignment or phase 1b's regeneration disagrees with the plan index, and the fix belongs there, not in the app.

- [ ] **Step 2: Write the failing tests.** In `apps/macos/RessacTests/DataTests.swift` (re-read it first):

1. Replace the doc comment of `TestDatabase` (lines 9–11):

```swift
/// A private copy of `tests/fixtures/contract.sqlite` reduced to its schema (v2) and the
/// 7 folders and 11 feeds of `vienna-like.opml`, all active: entries, full texts,
/// diagnoses, fetch logs and decisions are cleared, so each test seeds what it asserts on.
```

with:

```swift
/// A private copy of `tests/fixtures/contract.sqlite` reduced to its schema (v3) and the
/// 7 folders and 11 feeds of `vienna-like.opml`, all active, with the page numbers the core
/// gave them: entries, full texts, diagnoses, fetch logs and decisions are cleared, so each
/// test seeds what it asserts on.
```

2. Replace the test `newer_database_is_refused` in `DatabaseTests`:

```swift
    @Test func newer_database_is_refused() throws {
        let test = try TestDatabase()
        try test.execute("UPDATE schema_meta SET version = 3")
        #expect(try AppDatabase.check(at: test.url) == .tooNew(3))
    }
```

with these three tests:

```swift
    @Test func newer_database_is_refused() throws {
        let test = try TestDatabase()
        try test.execute("UPDATE schema_meta SET version = 4")
        #expect(try AppDatabase.check(at: test.url) == .tooNew(4))
    }

    /// Review Focus #3: a database the v3 core migrated, opened by an app that knows v2,
    /// gets "This database was created by a newer Ressac" (`SchemaCheck.tooNew`).
    @Test func an_older_app_refuses_a_v3_database() throws {
        let test = try TestDatabase()
        #expect(AppDatabase.knownSchemaVersion == 3)
        #expect(try AppDatabase.schemaVersion(at: test.url) == 3)
        #expect(try AppDatabase.check(at: test.url, known: 2) == .tooNew(3))
        #expect(try AppDatabase.check(at: test.url, known: 3) == .ok)
        #expect(try AppDatabase.check(at: test.url, known: 4) == .needsMigration(3))
        #expect(try AppDatabase.check(at: test.dir.appending(path: "nope.sqlite"), known: 2) == .missing)
    }

    /// Every database made before this sub-project is at v2: the app runs `ressac migrate` first.
    @Test func a_v2_database_requests_migration() throws {
        let test = try TestDatabase()
        try test.execute("UPDATE schema_meta SET version = 2")
        #expect(try AppDatabase.check(at: test.url) == .needsMigration(2))
    }
```

3. In `apps/macos/RessacTests/AppStateTests.swift` (re-read it first), `LauncherTests`: in `v1_database_is_migrated_before_opening`, replace the comment line

```swift
        // The stub "migrates" by copying the v2 fixture over the --db file.
```

with

```swift
        // The stub "migrates" by copying the v3 fixture over the --db file.
```

and replace the test `newer_database_is_refused`:

```swift
    @Test func newer_database_is_refused() async throws {
        let test = try TestDatabase()
        try test.execute("UPDATE schema_meta SET version = 3")
        let launcher = Launcher()
        await launcher.launch(database: test.url, executable: try Stub("exit 0").executable)
        guard case .tooNew(3) = launcher.phase else {
            Issue.record("expected tooNew(3), got \(launcher.phase)")
            return
        }
    }
```

with:

```swift
    @Test func newer_database_is_refused() async throws {
        let test = try TestDatabase()
        try test.execute("UPDATE schema_meta SET version = 4")
        let launcher = Launcher()
        await launcher.launch(database: test.url, executable: try Stub("exit 0").executable)
        guard case .tooNew(4) = launcher.phase else {
            Issue.record("expected tooNew(4), got \(launcher.phase)")
            return
        }
    }
```

- [ ] **Step 3: Run red** (test command above). Expected: `error: extra argument 'known' in call` (in `an_older_app_refuses_a_v3_database`) and `** TEST FAILED **`.

- [ ] **Step 4: Implement.** In `apps/macos/Ressac/Data/Database.swift` (re-read it first), replace:

```swift
    /// Schema version this build understands (`schema_meta.version`).
    static let knownSchemaVersion = 2
```

with:

```swift
    /// Schema version this build understands (`schema_meta.version`): v3 adds page numbers.
    static let knownSchemaVersion = 3
```

and replace:

```swift
    static func check(at url: URL) throws -> SchemaCheck {
        guard let version = try schemaVersion(at: url) else { return .missing }
        if version < knownSchemaVersion { return .needsMigration(version) }
        if version > knownSchemaVersion { return .tooNew(version) }
        return .ok
    }
```

with:

```swift
    static func check(at url: URL) throws -> SchemaCheck {
        try check(at: url, known: knownSchemaVersion)
    }

    /// `check(at:)` against any known version, so that a test can play an older app
    /// opening a newer database (Review Focus #3).
    static func check(at url: URL, known: Int) throws -> SchemaCheck {
        guard let version = try schemaVersion(at: url) else { return .missing }
        if version < known { return .needsMigration(version) }
        if version > known { return .tooNew(version) }
        return .ok
    }
```

- [ ] **Step 5: Run green**, same command. Expected: `✔ Test run with 12 tests in 2 suites passed` (7 in `DatabaseTests`, 5 in `LauncherTests`), `** TEST SUCCEEDED **`. Then run the two other suites that open a copy of the fixture, `-only-testing:RessacTests/FirstRunWALTests -only-testing:RessacTests/QueryTests`: all pass (they failed with `.tooNew(3)` since phase 1a).

- [ ] **Step 6: Commit.**

```bash
git add apps/macos/Ressac/Data/Database.swift apps/macos/RessacTests/DataTests.swift apps/macos/RessacTests/AppStateTests.swift
git commit -F - <<'EOF'
macOS app: open schema v3 databases, refuse newer ones

check(at:known:) lets a test play an app that only knows v2 (Review Focus #3).

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 6.2: Page numbers in the sidebar snapshot

**Interfaces:**
- Consumes: `folder.page` and `feed.page` (schema v3), written by the core only.
- Produces (`Data/Records.swift`, `Data/Queries.swift`):
  - `FolderItem.page: Int?` and `FeedItem.page: Int?`, decoded from the `page` columns (`RessacRecord`'s snake-case strategy needs no key mapping). Nil means the core has not numbered the row yet; in practice every row has a number after `ressac migrate`, `import` or a garden decision that adds a feed.
  - Each `page` is declared **last** in its struct, so the memberwise initialisers are `FolderItem(folderId:parentId:name:position:page:)` and `FeedItem(feedId:title:folderId:position:unread:status:preferFullText:htmlUrl:page:)`, with `page` defaulting to nil. Phase 7's `PageDirectory` tests build snapshots with them.
  - `Queries.sidebar` selects both columns. It still lists active feeds only; buried feeds keep their numbers in the database and in `v_page`, and the Cemetery (P802) reads them through `Queries.garden(scope: .cemetery)`.

- [ ] **Step 1: Write the failing tests.** Append to `QueryTests` in `DataTests.swift`, after `sidebar_snapshot_lists_folders_feeds_and_counts`:

```swift
    /// Schema v3: the numbers the core gave the fixture (see the table in phase 6).
    @Test func sidebar_snapshot_carries_page_numbers() async throws {
        let test = try TestDatabase()
        let db = try test.open()
        let snapshot = try await value(of: Queries.sidebar(), in: db)
        let folders = Dictionary(uniqueKeysWithValues: snapshot.folders.map { ($0.folderId, $0.page) })
        #expect(folders == [1: 200, 2: 202, 3: 300, 4: 400, 5: 403, 6: 500, 7: 501])
        let feeds = Dictionary(uniqueKeysWithValues: snapshot.feeds.map { ($0.feedId, $0.page) })
        #expect(feeds == [1: 201, 2: 203, 3: 204, 4: 205, 5: 206, 6: 301, 7: 302, 8: 401, 9: 402, 10: 404, 11: 502])
        // The view of the contract lists the same 18 numbers (through the app's pool, in WAL mode).
        let viewRows = try await db.reader.read { try Int.fetchOne($0, sql: "SELECT COUNT(*) FROM v_page") }
        #expect(viewRows == 18)
    }

    @Test func a_row_not_numbered_yet_reads_as_nil() async throws {
        let test = try TestDatabase()
        try test.execute("UPDATE feed SET page = NULL WHERE id = 8; UPDATE folder SET page = NULL WHERE id = 5")
        let snapshot = try await value(of: Queries.sidebar(), in: test.open())
        #expect(snapshot.feeds.first { $0.feedId == 8 }?.page == nil)
        #expect(snapshot.feeds.first { $0.feedId == 9 }?.page == 402)
        #expect(snapshot.folders.first { $0.folderId == 5 }?.page == nil)
        #expect(FeedItem(feedId: 1, title: "x", folderId: nil, position: 0, unread: 0, status: nil,
                         preferFullText: false, htmlUrl: nil).page == nil)
        #expect(FolderItem(folderId: 1, parentId: nil, name: "Blogs", position: 0, page: 200).page == 200)
    }
```

- [ ] **Step 2: Run red** with `-only-testing:RessacTests/QueryTests`. Expected: `error: value of type 'FolderItem' has no member 'page'` (and the same for `FeedItem`), then `** TEST FAILED **`.

- [ ] **Step 3: Implement the records.** In `apps/macos/Ressac/Data/Records.swift` (re-read it first), replace:

```swift
    var preferFullText: Bool
    var htmlUrl: String?

    var id: Int64 { feedId }
```

with:

```swift
    var preferFullText: Bool
    var htmlUrl: String?
    /// Teletext page number (schema v3), given once by the core; nil only before it ran.
    var page: Int?

    var id: Int64 { feedId }
```

and replace:

```swift
struct FolderItem: RessacRecord, Identifiable {
    var folderId: Int64
    var parentId: Int64?
    var name: String
    var position: Int

    var id: Int64 { folderId }
}
```

with:

```swift
struct FolderItem: RessacRecord, Identifiable {
    var folderId: Int64
    var parentId: Int64?
    var name: String
    var position: Int
    /// Teletext page number (schema v3): a root folder holds a hundred (200, 300, …).
    var page: Int?

    var id: Int64 { folderId }
}
```

- [ ] **Step 4: Implement the query.** In `apps/macos/Ressac/Data/Queries.swift` (re-read it first), in `sidebar(now:)`, replace:

```swift
                    SELECT id AS folder_id, parent_id, name, position FROM folder ORDER BY position, id
```

with:

```swift
                    SELECT id AS folder_id, parent_id, name, position, page FROM folder ORDER BY position, id
```

and replace:

```swift
                           h.status, f.prefer_full_text, f.html_url
```

with:

```swift
                           h.status, f.prefer_full_text, f.html_url, f.page
```

- [ ] **Step 5: Run green** with `-only-testing:RessacTests/QueryTests`. Expected: `✔ Test run with 11 tests in 1 suite passed`, `** TEST SUCCEEDED **`.

- [ ] **Step 6: Commit.**

```bash
git add apps/macos/Ressac/Data/Records.swift apps/macos/Ressac/Data/Queries.swift apps/macos/RessacTests/DataTests.swift
git commit -F - <<'EOF'
macOS app: read folder and feed page numbers (schema v3)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Verification gate

```sh
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/macos && xcodegen generate && \
  xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test 2>&1 | tee "${TMPDIR:-/tmp}/ressac-xcodebuild.log" | tail -3
grep -E '^/.+/apps/macos/.+: (warning|error):' "${TMPDIR:-/tmp}/ressac-xcodebuild.log" || echo "no warnings in our sources"
```

Expected: Rust checks unchanged; `** TEST SUCCEEDED **` with the whole suite green again (the six tests listed in phase 3's Execution order pass, plus 4 new tests); `no warnings in our sources`. `SmokeTests.bundled_helper_is_embedded_and_runs` also proves the bundled `ressac` is the v3 core of phase 1. Push:

```bash
git -c credential.helper= -c credential.helper='!gh auth git-credential' push
```

**Manual check on throwaway databases** (mac-app phase 11's pattern: `RESSAC_DB` and `HOME` in a temp dir; no keystroke is sent to any app):

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)"
APP="$(xcodebuild -project apps/macos/Ressac.xcodeproj -scheme Ressac -showBuildSettings 2>/dev/null | awk '/ BUILT_PRODUCTS_DIR /{print $3}')/Ressac.app"
SMOKE="$(mktemp -d)"; mkdir -p "$SMOKE/home"
cp tests/fixtures/contract.sqlite "$SMOKE/v3.sqlite"
cp tests/fixtures/contract.sqlite "$SMOKE/v4.sqlite" && sqlite3 "$SMOKE/v4.sqlite" "UPDATE schema_meta SET version = 4"
RESSAC_DB="$SMOKE/v3.sqlite" HOME="$SMOKE/home" "$APP/Contents/MacOS/Ressac" >"$SMOKE/v3.log" 2>&1 &
```

1. The v3 copy opens straight into the three-column window, with no "Updating the database…" screen. Quit.
2. `RESSAC_DB="$SMOKE/v4.sqlite" HOME="$SMOKE/home" "$APP/Contents/MacOS/Ressac" >"$SMOKE/v4.log" 2>&1 &` shows "This database was created by a newer Ressac". Quit.

The real v2 → v3 migration of an existing library is the core's (phases 1 and 2 test it) and is run on Clément's database in phase 14. `rm -rf "$SMOKE"`.

### Notes for the executor

- **Verified ahead of time.** Both tasks were built and run in a scratch copy of the repository (Xcode 27, Swift 6.4, warnings as errors) against a v3 fixture made by hand from today's v2 fixture: the plan index's `0003_pages.sql`, the pages of the table above, and `schema_meta.version = 3`. `DatabaseTests`, `LauncherTests` and `QueryTests` pass (23 tests), and so does the whole suite. With that fixture and `knownSchemaVersion` still 2, exactly the six tests listed in phase 3's Execution order failed, plus phase 4's switch test (it launches the fixture). The real fixture comes from phase 1b; Step 1 of Task 6.1 checks it holds the same numbers.
- **Read `v_page` through the app's pool.** A first version counted the view with `test.one(…)` on `TestDatabase.raw` right after `value(of:in: test.open())`, whose pool had just been released: it failed once with `SQLite error 5: database is locked`. Keeping the pool in `db` and reading through `db.reader` passed three runs in a row.
- **Why `check(at:known:)` and not a default argument.** The plan index names both functions; two plain functions also keep `check(at:)` usable as a function reference with one parameter.
- **`ReadingFixRound1Tests.swift`** keeps a doc comment that says "A schema v2 database in WAL mode" (line 260, `FirstRunWALTests.makeClosedWAL`). It is now a v3 database; the comment is left alone because this phase is at its five-file limit. Phase 14 (docs) or the next phase that touches that file fixes the word.
- **`Queries.sidebar` reads the tables, not `v_page`.** The view is the contract for the TUI and for `ressac page list`; the app already reads `folder` and `feed` for the sidebar, and the two columns are the same data. Phase 7's `PageDirectory` builds on `SidebarSnapshot`.
- **Memberwise initialisers.** No app code builds `FolderItem` or `FeedItem` by hand today (`grep -rn "FeedItem(\|FolderItem(" apps/macos` finds only the new test); adding `page` last keeps any future literal source-compatible apart from the new label.
