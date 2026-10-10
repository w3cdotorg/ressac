# Phase 11: Settings, bundled fonts, first run, OPML import and export

Read the plan index first. Global Constraints apply. Spec §7 (Settings; First run and empty states; Drag and drop, in), §8 (helper missing), Keyboard table (`⌘O`, `⇧⌘E`, `⌘+`/`⌘−`/`⌘0`, `⌘,`).

**Files (5):**
- Create: `apps/macos/Ressac/Settings/SettingsView.swift` (also `DatabaseLocation`)
- Create: `apps/macos/Ressac/App/OPMLImport.swift` (dry-run preview, import/export flow, first-run view, confirmation sheet, drop target)
- Modify: `apps/macos/Ressac/App/RessacApp.swift`
- Modify: `apps/macos/project.yml`
- Create: `apps/macos/RessacTests/FirstRunTests.swift`
- Binary assets (not counted): `apps/macos/Ressac/Resources/Fonts/Newsreader-Variable.ttf`, `Newsreader-Italic-Variable.ttf`, `Newsreader-OFL.txt`, `AtkinsonHyperlegibleNext-Variable.ttf`, `AtkinsonHyperlegibleNext-Italic-Variable.ttf`, `AtkinsonHyperlegibleNext-OFL.txt`

The per-context empty states of the entry list already exist (Phase 9's `AppState.emptyListMessage`); the Garden's "Nothing to triage" comes with Phase 12. Phase 10's reader is plugged into the window by Phase 12, so the font and size checks on a rendered article run there.

**Consumes from Phases 6–10** (names as their phase files define them):
- `ReaderStyle`, `ReaderFont`, `LineWidth`, `ArticleTemplate.fontFaces(for:)` (Phase 10).
- `RunRequest.importOPML(_:dryRun:)`, `.fetch(feeds:)`, `.export(to:cemetery:)`, `RessacCLI.bundledExecutable()` (Phase 8); `RunCoordinator.submit(_:) async -> Bool`, which returns `true` only when the run was accepted **and** exited `0`, so import then fetch can be chained (Phase 8).
- `AppDatabase.defaultURL(environment:)` (Phase 7: `RESSAC_DB`, else `~/Library/Application Support/Ressac/ressac.sqlite`); `AppState.sidebar.feeds` (Phase 9, observed with immediate scheduling).
- Phase 9: `Launcher.launch(database:executable:)`, called from `RessacApp` with `AppDatabase.defaultURL()`; a missing database is created by `ressac migrate`, so "no database" and "no feeds" are the same first-run case; `LaunchView` shows `MainView(state: state).environment(state)` once ready; the File menu's Import OPML… (`⌘O`) and Export OPML… (`⇧⌘E`) set `state.pendingPanel` to `.importOPML` / `.exportOPML`; the View menu already binds `⌘+`, `⌘−` and `⌘0` to `@AppStorage("reader.size")`. This phase answers `pendingPanel` and binds no shortcut of its own.

### Task 11.1: Bundle Newsreader and Atkinson Hyperlegible Next

**Interfaces:**
- Consumes: `ReaderFont.bundledFaces` (Phase 10: files `Newsreader-Variable`, `Newsreader-Italic-Variable`, `AtkinsonHyperlegibleNext-Variable`, `AtkinsonHyperlegibleNext-Italic-Variable`, in `Fonts/`).
- Produces: `Ressac.app/Contents/Resources/Fonts/` with the four variable fonts and both licences, registered for the app process by `ATSApplicationFontsPath = Fonts` (used by the Settings previews); `ArticleTemplate.fontFaces(for:)` now returns two `@font-face` rules for each bundled family.

Both families are under the SIL Open Font License 1.1, which allows bundling with the licence text. The files come from the `google/fonts` repository, pinned to commit `bd8f81ddb5c74d5c8897b36ad88b440266245103` (2026-10-09), and are renamed without brackets. The licences are renamed so that they cannot collide in the bundle.

- [ ] **Step 1: Write the failing test.** Create `apps/macos/RessacTests/FirstRunTests.swift`:

```swift
import AppKit
import Foundation
import Testing
@testable import Ressac

@Suite struct BundledFontTests {
    @Test func bundledFontsShipWithTheirLicences() {
        #expect(NSFont(name: "Newsreader", size: 12) != nil, "ATSApplicationFontsPath did not register Newsreader")
        #expect(NSFont(name: "Atkinson Hyperlegible Next", size: 12) != nil, "ATSApplicationFontsPath did not register Atkinson")
        for licence in ["Newsreader-OFL", "AtkinsonHyperlegibleNext-OFL"] {
            #expect(Bundle.main.url(forResource: licence, withExtension: "txt", subdirectory: "Fonts") != nil, "missing \(licence).txt")
        }
        let newsreader = ArticleTemplate.fontFaces(for: .newsreader)
        #expect(newsreader.components(separatedBy: "@font-face").count == 3)
        #expect(newsreader.contains("font-style: italic"))
        #expect(newsreader.contains("data:font/ttf;base64,"))
        #expect(ArticleTemplate.fontFaces(for: .atkinson).contains(#"font-family: "Atkinson Hyperlegible Next""#))
        #expect(ArticleTemplate.fontFaces(for: .charter).isEmpty)
    }
}
```

- [ ] **Step 2: Run red.** `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/BundledFontTests`. Expected: the test fails on `NSFont(name: "Newsreader", size: 12) != nil`, the two licence lookups and the `@font-face` count (the files are not bundled yet).

- [ ] **Step 3: Download the fonts and check them.** Run from the repository root:

```bash
FONTS=apps/macos/Ressac/Resources/Fonts
BASE=https://raw.githubusercontent.com/google/fonts/bd8f81ddb5c74d5c8897b36ad88b440266245103/ofl
mkdir -p "$FONTS"
curl -fsSL -o "$FONTS/Newsreader-Variable.ttf" "$BASE/newsreader/Newsreader%5Bopsz%2Cwght%5D.ttf"
curl -fsSL -o "$FONTS/Newsreader-Italic-Variable.ttf" "$BASE/newsreader/Newsreader-Italic%5Bopsz%2Cwght%5D.ttf"
curl -fsSL -o "$FONTS/Newsreader-OFL.txt" "$BASE/newsreader/OFL.txt"
curl -fsSL -o "$FONTS/AtkinsonHyperlegibleNext-Variable.ttf" "$BASE/atkinsonhyperlegiblenext/AtkinsonHyperlegibleNext%5Bwght%5D.ttf"
curl -fsSL -o "$FONTS/AtkinsonHyperlegibleNext-Italic-Variable.ttf" "$BASE/atkinsonhyperlegiblenext/AtkinsonHyperlegibleNext-Italic%5Bwght%5D.ttf"
curl -fsSL -o "$FONTS/AtkinsonHyperlegibleNext-OFL.txt" "$BASE/atkinsonhyperlegiblenext/OFL.txt"
(cd "$FONTS" && shasum -a 256 -c <<'EOF'
8a08d13f8a6c0d51be379a60af84f945f65369a67e509ee3c3bdcc421254d7c1  Newsreader-Variable.ttf
796668611f80b64d5adf182fde3b6f29ed83b4e7cbec7b96937e84ac01364792  Newsreader-Italic-Variable.ttf
fdfad38143ec470553cae82a1e45320bdd1b9ec70415d37bd0171051d8a4ded8  Newsreader-OFL.txt
5a455d1cfa099b601ab70751bb9673e8fe1854dc4500c80e1a220d0d75e31745  AtkinsonHyperlegibleNext-Variable.ttf
ce9cffed32742ad2d9238c561a93220385e5934cdc02b8eb4097a50efa957dc6  AtkinsonHyperlegibleNext-Italic-Variable.ttf
aca6a428580965d2297d1b718042dd427c2a9443ece3b0d02d758e161e0c4030  AtkinsonHyperlegibleNext-OFL.txt
EOF
)
```

Expected: six `OK` lines. Sizes: 451 664, 495 684, 4 394, 114 552, 123 916 and 4 431 bytes. All six URLs answered `HTTP/2 200` to `curl -I` on 2026-10-10.

- [ ] **Step 4: Bundle them.** In `apps/macos/project.yml`, in the `Ressac` target, Phase 6 has:

```yaml
    sources:
      - path: Ressac
```

Replace those two lines with the following, which keeps `Fonts` out of the `Ressac` group, copies it as a folder reference, and adds a base Info.plist holding the key (at the same indentation, between `sources:` and `dependencies:`):

```yaml
    sources:
      - path: Ressac
        excludes:
          - "Resources/Fonts"
      - path: Ressac/Resources/Fonts
        type: folder
        buildPhase: resources
    info:
      path: build/Info.plist
      properties:
        ATSApplicationFontsPath: Fonts
```

Keep `GENERATE_INFOPLIST_FILE: YES` and the `INFOPLIST_KEY_*` settings: Xcode merges them into this base plist. `build/` is already ignored by Phase 6's `.gitignore`, so the generated plist never enters the repository. `ATSApplicationFontsPath` is relative to `Contents/Resources`; the folder reference keeps the files in `Contents/Resources/Fonts/`, where `ReaderFont.bundledFaces` and the licence lookups expect them. All of this was checked with XcodeGen 2.46.0 and Xcode 27 on a scratch project of the same shape (`excludes` plus `type: folder` give `Contents/Resources/Fonts/…`; the built Info.plist holds both `ATSApplicationFontsPath` and the `INFOPLIST_KEY_` values).

- [ ] **Step 5: Run green.** Same command as Step 2. Expected: `BundledFontTests` passes. Also check the product:

```bash
APP=$(find ~/Library/Developer/Xcode/DerivedData -path '*Build/Products/Debug/Ressac.app' -maxdepth 5 | head -n 1)
/usr/libexec/PlistBuddy -c "Print :ATSApplicationFontsPath" "$APP/Contents/Info.plist"   # Fonts
ls "$APP/Contents/Resources/Fonts"                                                       # the six files
```

- [ ] **Step 6: Commit.**

```bash
git add apps/macos/project.yml apps/macos/Ressac/Resources/Fonts apps/macos/RessacTests/FirstRunTests.swift
git commit -m "Bundle Newsreader and Atkinson Hyperlegible Next (OFL) with their licences"
```

### Task 11.2: Settings and the database location

**Interfaces:**
- Consumes: `ReaderStyle.Keys`, `ReaderFont.swiftUIFont(size:)`, `LineWidth`.
- Produces (`Settings/SettingsView.swift`):
  - `enum DatabaseLocation` with `static let defaultsKey = "databasePath"`, `static var defaultURL: URL` (Phase 7's default without `RESSAC_DB`), `static var logsURL: URL`, `static func resolve(_ path: String?) -> URL`, `static func current(environment:defaults:) -> URL`. Order: `RESSAC_DB` (Phase 9's smoke runs and Phase 13's acceptance run use it), then the Settings choice, then the default shared with the terminal;
  - `extension UTType { static let sqliteDatabase }`;
  - `struct SettingsView: View`, with two tabs. **Reading:** font picker (each name in its own face), text size slider 12–28 pt, line width segmented control, live preview. **Data:** database path with Choose… / New… / Use Default, a note when `RESSAC_DB` overrides it, a relaunch prompt after a change, and Reveal in Finder for `~/Library/Logs/Ressac/`.

The database is read once at launch (`AppState.database` is a `let`), so a new location takes effect after a relaunch; Settings offers "Relaunch Now".

- [ ] **Step 1: Write the failing test.** Append to `FirstRunTests.swift`:

```swift
@Suite struct DatabaseLocationTests {
    @Test func environmentThenSettingThenTheSharedDefault() throws {
        let suite = "ressac.tests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let shared = URL.homeDirectory.appending(path: "Library/Application Support/Ressac/ressac.sqlite")
        #expect(DatabaseLocation.current(environment: [:], defaults: defaults) == shared)
        defaults.set("  ", forKey: DatabaseLocation.defaultsKey)
        #expect(DatabaseLocation.current(environment: [:], defaults: defaults) == shared)
        defaults.set("~/Feeds/other.sqlite", forKey: DatabaseLocation.defaultsKey)
        #expect(DatabaseLocation.current(environment: [:], defaults: defaults) == URL.homeDirectory.appending(path: "Feeds/other.sqlite"))
        #expect(DatabaseLocation.current(environment: ["RESSAC_DB": "/tmp/copy.sqlite"], defaults: defaults) == URL(filePath: "/tmp/copy.sqlite"))
        #expect(DatabaseLocation.current(environment: ["RESSAC_DB": ""], defaults: defaults) == URL.homeDirectory.appending(path: "Feeds/other.sqlite"))
    }
}
```

- [ ] **Step 2: Run red.** `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/DatabaseLocationTests`. Expected: build failure, `cannot find 'DatabaseLocation' in scope`.

- [ ] **Step 3: Implement.** Create `apps/macos/Ressac/Settings/SettingsView.swift`:

```swift
import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// Where the database lives: `RESSAC_DB`, else the Settings choice, else the core's default
/// (shared with the `ressac` command). The app reads it once at launch.
enum DatabaseLocation {
    /// `UserDefaults` key of the Settings choice; empty or missing means the default.
    static let defaultsKey = "databasePath"

    /// `$HOME/Library/Application Support/Ressac/ressac.sqlite` (Phase 7, without `RESSAC_DB`).
    static var defaultURL: URL { AppDatabase.defaultURL(environment: [:]) }

    /// Where the bundled `ressac` writes its daily logs.
    static var logsURL: URL {
        URL.homeDirectory.appending(path: "Library/Logs/Ressac", directoryHint: .isDirectory)
    }

    static func resolve(_ path: String?) -> URL {
        guard let path, !path.trimmingCharacters(in: .whitespaces).isEmpty else { return defaultURL }
        return URL(filePath: (path as NSString).expandingTildeInPath)
    }

    static func current(environment: [String: String] = ProcessInfo.processInfo.environment,
                        defaults: UserDefaults = .standard) -> URL {
        if let path = environment["RESSAC_DB"], !path.isEmpty {
            return URL(filePath: path)
        }
        return resolve(defaults.string(forKey: defaultsKey))
    }
}

extension UTType {
    static let sqliteDatabase = UTType(filenameExtension: "sqlite", conformingTo: .database) ?? .database
}

/// Settings (⌘,): Reading and Data (spec §7).
struct SettingsView: View {
    var body: some View {
        TabView {
            Tab("Reading", systemImage: "textformat") {
                ReadingSettings()
            }
            Tab("Data", systemImage: "externaldrive") {
                DataSettings()
            }
        }
        .frame(width: 540)
        .scenePadding()
    }
}

private struct ReadingSettings: View {
    @AppStorage(ReaderStyle.Keys.font) private var font = ReaderFont.sfPro
    @AppStorage(ReaderStyle.Keys.size) private var size = ReaderStyle.defaultSize
    @AppStorage(ReaderStyle.Keys.width) private var width = LineWidth.medium

    var body: some View {
        Form {
            Picker("Font:", selection: $font) {
                ForEach(ReaderFont.allCases) { option in
                    Text(option.displayName)
                        .font(option.swiftUIFont(size: NSFont.systemFontSize))
                        .tag(option)
                }
            }

            LabeledContent("Text size:") {
                HStack(spacing: 8) {
                    Slider(value: $size, in: ReaderStyle.sizeRange, step: ReaderStyle.sizeStep) {
                        Text("Text size")
                    } minimumValueLabel: {
                        Image(systemName: "textformat.size.smaller")
                    } maximumValueLabel: {
                        Image(systemName: "textformat.size.larger")
                    }
                    .labelsHidden()
                    Text("\(Int(ReaderStyle.clampedSize(size))) pt")
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                        .frame(width: 40, alignment: .trailing)
                }
            }

            Picker("Line width:", selection: $width) {
                ForEach(LineWidth.allCases) { option in
                    Text(option.displayName).tag(option)
                }
            }
            .pickerStyle(.segmented)

            LabeledContent("Preview:") {
                Text("The quick brown fox jumps over 1 234 lazy dogs. Il1 O0, « l'écart » se creuse.")
                    .font(font.swiftUIFont(size: ReaderStyle.clampedSize(size)))
                    .lineLimit(3)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }

            Text("⌘+ and ⌘− change the text size while you read; ⌘0 restores it.")
                .font(.callout)
                .foregroundStyle(.secondary)
        }
    }
}

private struct DataSettings: View {
    @AppStorage(DatabaseLocation.defaultsKey) private var databasePath = ""
    @State private var needsRelaunch = false

    private var databaseURL: URL { DatabaseLocation.resolve(databasePath) }

    var body: some View {
        Form {
            LabeledContent("Database:") {
                VStack(alignment: .leading, spacing: 6) {
                    Text(databaseURL.path(percentEncoded: false))
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .textSelection(.enabled)
                        .help(databaseURL.path(percentEncoded: false))
                    HStack {
                        Button("Choose…", action: chooseExisting)
                        Button("New…", action: chooseNew)
                        Button("Use Default") { update("") }
                            .disabled(databasePath.isEmpty)
                    }
                    Text(databasePath.isEmpty
                         ? "Shared with the ressac command in Terminal."
                         : "In Terminal, pass --db with this path to use the same database.")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                    if ProcessInfo.processInfo.environment["RESSAC_DB"] != nil {
                        Text("RESSAC_DB is set, so this launch uses it instead.")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                    }
                    if needsRelaunch {
                        HStack {
                            Text("Ressac opens the new database after a relaunch.")
                                .font(.callout)
                            Button("Relaunch Now", action: relaunch)
                        }
                    }
                }
            }

            LabeledContent("Logs:") {
                Button("Reveal in Finder", action: revealLogs)
            }
        }
    }

    private func update(_ path: String) {
        guard path != databasePath else { return }
        databasePath = path
        needsRelaunch = true
    }

    private func chooseExisting() {
        let panel = NSOpenPanel()
        panel.message = "Choose a Ressac database."
        panel.allowedContentTypes = [.sqliteDatabase]
        panel.directoryURL = databaseURL.deletingLastPathComponent()
        guard panel.runModal() == .OK, let url = panel.url else { return }
        update(url.path(percentEncoded: false))
    }

    private func chooseNew() {
        let panel = NSSavePanel()
        panel.message = "Ressac creates the database here on its next launch."
        panel.nameFieldStringValue = "ressac.sqlite"
        panel.allowedContentTypes = [.sqliteDatabase]
        panel.canCreateDirectories = true
        guard panel.runModal() == .OK, let url = panel.url else { return }
        update(url.path(percentEncoded: false))
    }

    private func relaunch() {
        let configuration = NSWorkspace.OpenConfiguration()
        configuration.createsNewApplicationInstance = true
        Task {
            _ = try? await NSWorkspace.shared.openApplication(at: Bundle.main.bundleURL, configuration: configuration)
            NSApp.terminate(nil)
        }
    }

    private func revealLogs() {
        let logs = DatabaseLocation.logsURL
        try? FileManager.default.createDirectory(at: logs, withIntermediateDirectories: true)
        NSWorkspace.shared.open(logs)
    }
}
```

- [ ] **Step 4: Run green.** Same command. Expected: `DatabaseLocationTests` passes, 1 test.

- [ ] **Step 5: Commit.**

```bash
git add apps/macos/Ressac/Settings/SettingsView.swift apps/macos/RessacTests/FirstRunTests.swift
git commit -m "Settings: reading font, size and width; database location; reveal logs"
```

### Task 11.3: OPML import (dry-run sheet, first run, drop) and export

**Interfaces:**
- Consumes: `RunRequest.importOPML(_:dryRun:)`, `.fetch(feeds:)`, `.export(to:cemetery:)`, `RunCoordinator.submit(_:)`, `RessacCLI.bundledExecutable()`, `AppState.sidebar`, `AppState.pendingPanel`, `DatabaseLocation.current()`.
- Produces (`App/OPMLImport.swift`):
  - `struct ImportReport: Decodable, Sendable, Equatable { dryRun; foldersAdded; feedsAdded; merged: [[String]]; skippedExisting }`, the JSON line of `ressac --json import` (core `ImportReport`, snake case);
  - `struct ImportPreviewFailure: Error, Equatable { exitCode: Int32; message: String }` (a signal death has `exitCode = -signal`, as `RunFailure`);
  - `enum ImportPreview` with `arguments(database:file:)`, `decode(_:)` and `static func run(executable: URL, database: URL, file: URL) async throws -> ImportReport`;
  - `@MainActor @Observable final class OPMLFlow` with `step: Step` (`idle`, `previewing(URL)`, `confirming(URL, ImportReport)`, `importing(URL)`, `failed(String)`), `isChoosingFile`, `init(preview:submit:)`, `static func live(database:runs:)`, `choose()`, `open(_ urls: [URL]) async -> Bool`, `confirm() async`, `cancel()`, `export(to:) async -> Bool`, `chooseExportDestination() async`, `static func summary(of:) -> [String]`, `static func firstOPML(in:) -> URL?`;
  - `extension View { func libraryChrome(_ state: AppState) -> some View }`: shows `FirstRunView` while the database has no feed, accepts dropped `.opml` files with a dashed accent outline, presents the confirmation sheet, the failure alert and the file chooser, and answers `state.pendingPanel` (it resets it to `nil`, then `.importOPML` opens the chooser and `.exportOPML` the save sheet);
  - `struct FirstRunView`, `struct ImportConfirmationSheet`.

Why a separate runner for the preview: `ressac --json import` prints one JSON object **without** an `"event"` key, so `RessacCLI`'s NDJSON stream would log and skip it (Review Focus #5). The preview is read-only and short; it drains stdout and stderr concurrently so a chatty stderr can never block the child, and reports the last stderr line without anyhow's `error: ` prefix. The real import goes through `RunCoordinator` like every other write.

- [ ] **Step 1: Write the failing tests.** Append to `FirstRunTests.swift`:

```swift
private let sampleLine = #"{"dry_run":true,"folders_added":16,"feeds_added":186,"merged":[["Daring Fireball (1)","Daring Fireball"]],"skipped_existing":3}"#
private let sampleReport = ImportReport(dryRun: true, foldersAdded: 16, feedsAdded: 186,
                                        merged: [["Daring Fireball (1)", "Daring Fireball"]], skippedExisting: 3)

/// A throwaway directory with executable stand-ins for the bundled `ressac`.
private func stubDirectory() throws -> URL {
    let dir = FileManager.default.temporaryDirectory.appending(path: "ressac-stub-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    let scripts = [
        "ok": "#!/bin/sh\necho 'starting' >&2\necho '\(sampleLine)'\n",
        "bad": "#!/bin/sh\necho 'error: cannot read /nowhere.opml' >&2\nexit 1\n",
        "killed": "#!/bin/sh\nkill -9 $$\n",
    ]
    for (name, body) in scripts {
        let url = dir.appending(path: name)
        try body.write(to: url, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: url.path(percentEncoded: false))
    }
    return dir
}

@MainActor private final class Recorder {
    var requests: [RunRequest] = []
    var results: [Bool] = []
    func submit(_ request: RunRequest) async -> Bool {
        requests.append(request)
        return results.isEmpty ? true : results.removeFirst()
    }
}

@Suite struct ImportPreviewTests {
    @Test func decodesTheCLIJSONLine() throws {
        #expect(try ImportPreview.decode(Data("\(sampleLine)\n".utf8)) == sampleReport)
        #expect(throws: ImportPreviewFailure.self) { try ImportPreview.decode(Data("not json\n".utf8)) }
    }

    @Test func argumentsAskForAJSONDryRun() {
        let args = ImportPreview.arguments(database: URL(filePath: "/tmp/a b/ressac.sqlite"), file: URL(filePath: "/tmp/f.opml"))
        #expect(args == ["--db", "/tmp/a b/ressac.sqlite", "--json", "import", "--dry-run", "/tmp/f.opml"])
    }

    @Test func runsTheHelperAndReportsFailures() async throws {
        let dir = try stubDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let db = dir.appending(path: "ressac.sqlite")
        let file = dir.appending(path: "f.opml")
        #expect(try await ImportPreview.run(executable: dir.appending(path: "ok"), database: db, file: file) == sampleReport)
        await #expect(throws: ImportPreviewFailure(exitCode: 1, message: "cannot read /nowhere.opml")) {
            try await ImportPreview.run(executable: dir.appending(path: "bad"), database: db, file: file)
        }
        await #expect(throws: ImportPreviewFailure(exitCode: -9, message: "The import preview failed.")) {
            try await ImportPreview.run(executable: dir.appending(path: "killed"), database: db, file: file)
        }
    }
}

@Suite @MainActor struct OPMLFlowTests {
    private let file = URL(filePath: "/tmp/vienna.opml")

    @Test func summaryDescribesTheDryRun() {
        #expect(OPMLFlow.summary(of: sampleReport) == [
            "186 feeds in 16 new folders.",
            "1 duplicate inside the file will be merged:",
            "“Daring Fireball (1)” into “Daring Fireball”",
            "3 feeds already in Ressac will be skipped.",
        ])
        let nothing = ImportReport(dryRun: true, foldersAdded: 0, feedsAdded: 0, merged: [], skippedExisting: 1)
        #expect(OPMLFlow.summary(of: nothing) == ["No new feeds in this file.", "1 feed already in Ressac will be skipped."])
    }

    @Test func confirmingImportsThenFetchesEverything() async {
        let recorder = Recorder()
        let flow = OPMLFlow(preview: { _ in sampleReport }, submit: recorder.submit)
        #expect(await flow.open([URL(filePath: "/tmp/notes.txt"), file]))
        #expect(flow.step == .confirming(file, sampleReport))
        await flow.confirm()
        #expect(recorder.requests == [.importOPML(file, dryRun: false), .fetch(feeds: [])])
        #expect(flow.step == .idle)
    }

    @Test func aFailedImportDoesNotFetch() async {
        let recorder = Recorder()
        recorder.results = [false]
        let flow = OPMLFlow(preview: { _ in sampleReport }, submit: recorder.submit)
        await flow.open([file])
        await flow.confirm()
        #expect(recorder.requests == [.importOPML(file, dryRun: false)])
        #expect(flow.step == .idle)
    }

    @Test func previewFailuresAreShownAndNothingIsWritten() async {
        let recorder = Recorder()
        let flow = OPMLFlow(preview: { _ in throw ImportPreviewFailure(exitCode: 1, message: "invalid OPML") }, submit: recorder.submit)
        await flow.open([file])
        #expect(flow.failureMessage == "invalid OPML")
        flow.cancel()
        #expect(flow.step == .idle)
        #expect(recorder.requests.isEmpty)
    }

    @Test func onlyOPMLFilesAreAccepted() async {
        let flow = OPMLFlow(preview: { _ in sampleReport }, submit: { _ in true })
        #expect(await flow.open([URL(filePath: "/tmp/feeds.xml"), URL(string: "https://example.org/a.opml")!]) == false)
        #expect(flow.step == .idle)
        #expect(OPMLFlow.firstOPML(in: [URL(filePath: "/tmp/A.OPML")]) == URL(filePath: "/tmp/A.OPML"))
    }

    @Test func exportRunsRessacExport() async {
        let recorder = Recorder()
        let flow = OPMLFlow(preview: { _ in sampleReport }, submit: recorder.submit)
        let target = URL(filePath: "/tmp/Ressac.opml")
        #expect(await flow.export(to: target))
        #expect(recorder.requests == [.export(to: target, cemetery: false)])
    }
}
```

- [ ] **Step 2: Run red.** `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/ImportPreviewTests -only-testing:RessacTests/OPMLFlowTests`. Expected: build failure, `cannot find 'ImportReport' in scope`, `cannot find 'ImportPreview' in scope`, `cannot find 'OPMLFlow' in scope`.

- [ ] **Step 3: Implement.** Create `apps/macos/Ressac/App/OPMLImport.swift`:

```swift
import AppKit
import SwiftUI
import UniformTypeIdentifiers

extension UTType {
    static let opml = UTType(filenameExtension: "opml", conformingTo: .xml) ?? .xml
}

/// The JSON line `ressac --json import [--dry-run]` prints (core `ImportReport`).
struct ImportReport: Decodable, Sendable, Equatable {
    let dryRun: Bool
    let foldersAdded: Int
    let feedsAdded: Int
    /// `[dropped title, kept title]` for duplicates inside the file.
    let merged: [[String]]
    let skippedExisting: Int
}

struct ImportPreviewFailure: Error, Equatable {
    let exitCode: Int32
    let message: String
}

/// Runs `ressac import --dry-run` for the confirmation sheet. Its JSON line carries no `event`
/// tag, so it does not go through `RessacCLI`'s event stream; it is read-only and short.
enum ImportPreview {
    static func arguments(database: URL, file: URL) -> [String] {
        ["--db", database.path(percentEncoded: false), "--json", "import", "--dry-run", file.path(percentEncoded: false)]
    }

    static func decode(_ output: Data) throws -> ImportReport {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard let line = output.split(separator: UInt8(ascii: "\n")).last(where: { $0.first == UInt8(ascii: "{") }) else {
            throw ImportPreviewFailure(exitCode: 0, message: "The import preview printed nothing.")
        }
        return try decoder.decode(ImportReport.self, from: Data(line))
    }

    static func run(executable: URL, database: URL, file: URL) async throws -> ImportReport {
        let process = Process()
        process.executableURL = executable
        process.arguments = arguments(database: database, file: file)
        let stdout = Pipe()
        let stderr = Pipe()
        process.standardOutput = stdout
        process.standardError = stderr
        let (exits, exitContinuation) = AsyncStream.makeStream(of: Int32.self)
        process.terminationHandler = { finished in
            let code = finished.terminationReason == .uncaughtSignal ? -finished.terminationStatus : finished.terminationStatus
            exitContinuation.yield(code)
            exitContinuation.finish()
        }
        try process.run()
        let outHandle = stdout.fileHandleForReading
        let errHandle = stderr.fileHandleForReading
        // Drain both pipes concurrently so a chatty stderr can never block the child.
        async let output = Task.detached { outHandle.readDataToEndOfFile() }.value
        async let errors = Task.detached { errHandle.readDataToEndOfFile() }.value
        let (outData, errData) = await (output, errors)
        var status: Int32 = 0
        for await code in exits { status = code }
        guard status == 0 else {
            let last = String(decoding: errData, as: UTF8.self).split(whereSeparator: \.isNewline).last.map(String.init)
            let message = last.map { $0.hasPrefix("error: ") ? String($0.dropFirst(7)) : $0 } ?? "The import preview failed."
            throw ImportPreviewFailure(exitCode: status, message: message)
        }
        return try decode(outData)
    }
}

/// Import (drop, ⌘O, first run) and export (⇧⌘E) of OPML files. Phase 9's File menu
/// sets `AppState.pendingPanel`; `libraryChrome` turns it into `choose()` or the save panel.
@MainActor @Observable final class OPMLFlow {
    enum Step: Equatable {
        case idle
        case previewing(URL)
        case confirming(URL, ImportReport)
        case importing(URL)
        case failed(String)
    }

    private(set) var step: Step = .idle
    var isChoosingFile = false

    @ObservationIgnored private let preview: @Sendable (URL) async throws -> ImportReport
    @ObservationIgnored private let submit: @MainActor (RunRequest) async -> Bool

    init(preview: @escaping @Sendable (URL) async throws -> ImportReport,
         submit: @escaping @MainActor (RunRequest) async -> Bool) {
        self.preview = preview
        self.submit = submit
    }

    /// The bundled helper on the database in use, and the app's run coordinator.
    static func live(database: URL, runs: RunCoordinator) -> OPMLFlow {
        OPMLFlow(
            preview: { file in
                guard let executable = RessacCLI.bundledExecutable() else {
                    throw ImportPreviewFailure(exitCode: 1, message: "The ressac helper is missing from the app.")
                }
                return try await ImportPreview.run(executable: executable, database: database, file: file)
            },
            submit: { await runs.submit($0) }
        )
    }

    var isBusy: Bool {
        switch step {
        case .previewing, .importing: true
        case .idle, .confirming, .failed: false
        }
    }

    var failureMessage: String? {
        if case .failed(let message) = step { message } else { nil }
    }

    var pendingReport: (file: URL, report: ImportReport)? {
        if case .confirming(let file, let report) = step { (file, report) } else { nil }
    }

    func choose() {
        guard !isBusy else { return }
        isChoosingFile = true
    }

    static func firstOPML(in urls: [URL]) -> URL? {
        urls.first { $0.isFileURL && $0.pathExtension.lowercased() == "opml" }
    }

    /// Previews the first `.opml` among `urls` with `--dry-run`. Returns `false` when there is none.
    @discardableResult
    func open(_ urls: [URL]) async -> Bool {
        guard !isBusy, let file = Self.firstOPML(in: urls) else { return false }
        step = .previewing(file)
        do {
            step = .confirming(file, try await preview(file))
        } catch let failure as ImportPreviewFailure {
            step = .failed(failure.message)
        } catch {
            step = .failed(error.localizedDescription)
        }
        return true
    }

    /// Imports the confirmed file, then fetches every feed (spec §7, first run).
    func confirm() async {
        guard case .confirming(let file, _) = step else { return }
        step = .importing(file)
        if await submit(.importOPML(file, dryRun: false)) {
            _ = await submit(.fetch(feeds: []))
        }
        step = .idle
    }

    func cancel() {
        guard !isBusy else { return }
        step = .idle
    }

    @discardableResult
    func export(to url: URL) async -> Bool {
        await submit(.export(to: url, cemetery: false))
    }

    /// Asks where to save, as a sheet on the key window, then runs `ressac export -o`.
    func chooseExportDestination() async {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = "Ressac.opml"
        panel.allowedContentTypes = [.opml]
        panel.canCreateDirectories = true
        let response = if let window = NSApp.keyWindow {
            await panel.beginSheetModal(for: window)
        } else {
            await panel.begin()
        }
        guard response == .OK, let url = panel.url else { return }
        await export(to: url)
    }

    /// The confirmation sheet's lines, from the dry-run counts.
    static func summary(of report: ImportReport) -> [String] {
        var lines: [String] = []
        if report.feedsAdded == 0 {
            lines.append("No new feeds in this file.")
        } else {
            lines.append("\(count(report.feedsAdded, "feed")) in \(count(report.foldersAdded, "new folder")).")
        }
        if !report.merged.isEmpty {
            lines.append("\(count(report.merged.count, "duplicate")) inside the file will be merged:")
            lines += report.merged.compactMap { pair in
                pair.count == 2 ? "“\(pair[0])” into “\(pair[1])”" : nil
            }
        }
        if report.skippedExisting > 0 {
            lines.append("\(count(report.skippedExisting, "feed")) already in Ressac will be skipped.")
        }
        return lines
    }

    private static func count(_ n: Int, _ noun: String) -> String {
        "\(n) \(noun)\(n == 1 ? "" : "s")"
    }
}

extension View {
    /// First-run screen, OPML drop target, import sheet, file chooser and export panel for
    /// the main window.
    func libraryChrome(_ state: AppState) -> some View {
        modifier(LibraryChrome(state: state))
    }
}

private struct LibraryChrome: ViewModifier {
    let state: AppState
    @State private var flow: OPMLFlow
    @State private var isDropTargeted = false

    init(state: AppState) {
        self.state = state
        _flow = State(initialValue: OPMLFlow.live(database: DatabaseLocation.current(), runs: state.runs))
    }

    func body(content: Content) -> some View {
        Group {
            // `sidebar` is observed with immediate scheduling (Phase 9), so an existing
            // library never flashes the first-run screen.
            if state.sidebar.feeds.isEmpty {
                FirstRunView(flow: flow)
            } else {
                content
            }
        }
        .dropDestination(for: URL.self) { urls, _ in
            guard OPMLFlow.firstOPML(in: urls) != nil, !flow.isBusy else { return false }
            Task { await flow.open(urls) }
            return true
        } isTargeted: { isDropTargeted = $0 }
        .overlay {
            if isDropTargeted {
                RoundedRectangle(cornerRadius: 10)
                    .strokeBorder(Color.accentColor, style: StrokeStyle(lineWidth: 3, dash: [8, 6]))
                    .background(Color.accentColor.opacity(0.05), in: RoundedRectangle(cornerRadius: 10))
                    .padding(6)
                    .allowsHitTesting(false)
                    .transition(.opacity)
            }
        }
        .animation(.easeOut(duration: 0.15), value: isDropTargeted)
        .sheet(isPresented: Binding(get: { flow.pendingReport != nil }, set: { if !$0 { flow.cancel() } })) {
            if let pending = flow.pendingReport {
                ImportConfirmationSheet(file: pending.file, report: pending.report, flow: flow)
            }
        }
        .alert("This OPML file can't be imported",
               isPresented: Binding(get: { flow.failureMessage != nil }, set: { if !$0 { flow.cancel() } })) {
            Button("OK", role: .cancel) { flow.cancel() }
        } message: {
            Text(flow.failureMessage ?? "")
        }
        .fileImporter(isPresented: $flow.isChoosingFile, allowedContentTypes: [.opml]) { result in
            guard case .success(let url) = result else { return }
            Task { await flow.open([url]) }
        }
        .onChange(of: state.pendingPanel, initial: true) { _, panel in
            guard let panel else { return }
            state.pendingPanel = nil
            switch panel {
            case .importOPML: flow.choose()
            case .exportOPML: Task { await flow.chooseExportDestination() }
            }
        }
    }
}

/// "No database, or no feeds" (spec §7): a centred empty state with a button.
struct FirstRunView: View {
    let flow: OPMLFlow

    var body: some View {
        ContentUnavailableView {
            Label("No Feeds Yet", systemImage: "dot.radiowaves.up.forward")
        } description: {
            Text("Drop an OPML file here, or choose one…")
        } actions: {
            Button("Choose OPML File…") { flow.choose() }
                .keyboardShortcut(.defaultAction)
                .disabled(flow.isBusy)
            if flow.isBusy {
                ProgressView().controlSize(.small)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// Shows the `--dry-run` counts before anything is written.
struct ImportConfirmationSheet: View {
    let file: URL
    let report: ImportReport
    let flow: OPMLFlow

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Label("Import “\(file.lastPathComponent)”?", systemImage: "tray.and.arrow.down")
                .font(.headline)
            VStack(alignment: .leading, spacing: 4) {
                ForEach(Array(OPMLFlow.summary(of: report).enumerated()), id: \.offset) { _, line in
                    Text(line)
                }
            }
            .textSelection(.enabled)
            if report.feedsAdded > 0 {
                Text("Ressac fetches every feed right after the import.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            HStack {
                Spacer()
                Button("Cancel", role: .cancel) { flow.cancel() }
                    .keyboardShortcut(.cancelAction)
                Button("Import") { Task { await flow.confirm() } }
                    .keyboardShortcut(.defaultAction)
                    .disabled(report.feedsAdded == 0)
            }
        }
        .padding(20)
        .frame(width: 420)
    }
}
```

- [ ] **Step 4: Run green.** Same command as Step 2. Expected: 9 tests pass (3 + 6). `runsTheHelperAndReportsFailures` spawns the three stub scripts in a temporary directory; it never runs the real `ressac`.

- [ ] **Step 5: Commit.**

```bash
git add apps/macos/Ressac/App/OPMLImport.swift apps/macos/RessacTests/FirstRunTests.swift
git commit -m "OPML import with a dry-run sheet, first-run screen, drop target; export"
```

### Task 11.4: Wire Settings, the database location and the first run into the app

**Interfaces:**
- Consumes: `SettingsView`, `DatabaseLocation.current()`, `View.libraryChrome(_:)`, Phase 9's `RessacApp` and `LaunchView`.
- Produces: the `Settings` scene (`⌘,` comes with it), the launch on `DatabaseLocation.current()`, and the main window with first run, OPML drop and the import/export panels.

- [ ] **Step 1: Edit `apps/macos/Ressac/App/RessacApp.swift`.** Three changes to Phase 9's file, nothing else:

1. In `RessacApp.body`, the launch call reads the Settings choice:

```swift
await launcher.launch(database: DatabaseLocation.current(), executable: RessacCLI.bundledExecutable())
```

2. After the `WindowGroup` (and its `.commands { RessacCommands(state: launcher.state) }`), add the Settings scene:

```swift
Settings {
    SettingsView()
}
```

3. In `LaunchView`, the `.ready` case applies the library chrome inside the environment injection:

```swift
case let .ready(state):
    MainView(state: state)
        .libraryChrome(state)
        .environment(state)
```

- [ ] **Step 2: Build and run the tests.** `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test`. Expected: all tests pass, zero warnings in our sources.

- [ ] **Step 3: Manual check on a throwaway database** (Phase 9's pattern: `RESSAC_DB` points at the copy, `HOME` keeps the helper's logs out of `~/Library`; never launch without `RESSAC_DB` before Phase 13):

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)"
APP="$(xcodebuild -project apps/macos/Ressac.xcodeproj -scheme Ressac -showBuildSettings 2>/dev/null | awk '/ BUILT_PRODUCTS_DIR /{print $3}')/Ressac.app"
SMOKE="$(mktemp -d)"; mkdir -p "$SMOKE/home"
RESSAC_DB="$SMOKE/ressac.sqlite" HOME="$SMOKE/home" "$APP/Contents/MacOS/Ressac" >"$SMOKE/app.log" 2>&1 &
```

Check:
1. The database file does not exist yet: the launch creates it with `ressac migrate`, then the window shows "No Feeds Yet" and "Drop an OPML file here, or choose one…", with no sidebar or toolbar clutter.
2. Dragging `tests/fixtures/vienna-like.opml` over the window draws the dashed accent outline; dropping it opens the sheet "Import “vienna-like.opml”?" with "11 feeds in 7 new folders.", "1 duplicate inside the file will be merged:" and "“Daring Fireball (1)” into “Daring Fireball”". Cancel writes nothing (`sqlite3 "$SMOKE/ressac.sqlite" "SELECT count(*) FROM feed"` prints `0`).
3. File ▸ Import OPML… (`⌘O`) opens the chooser; choosing the same file and pressing Import shows the three-column window as soon as the feeds exist, with the progress bar of the fetch that follows (11 real feeds are fetched).
4. File ▸ Export OPML… (`⇧⌘E`) opens a save sheet; `grep -c xmlUrl` on the saved file prints `11`.
5. `⌘,` opens Settings: the font menu shows each name in its own face (Newsreader and Atkinson included); the size slider follows View ▸ Bigger Text (`⌘+`), Smaller Text (`⌘−`) and Actual Size (`⌘0`); the Data tab shows "RESSAC_DB is set, so this launch uses it instead."; Reveal in Finder opens `$SMOKE/home/Library/Logs/Ressac/`. Do not press "Relaunch Now" here.

Quit the app, then `rm -rf "$SMOKE"`.

- [ ] **Step 4: Verification gate.**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test
```

- [ ] **Step 5: Commit and push.**

```bash
git add apps/macos/Ressac/App/RessacApp.swift
git commit -m "App: Settings scene, database location setting, first-run chrome"
git -c credential.helper= -c credential.helper='!gh auth git-credential' push
```

## Notes for the executor

- **Verified ahead of time.** `SettingsView.swift` and `OPMLImport.swift` were type-checked with Swift 6.4 in Swift 6 mode with warnings as errors, against stand-ins for the Phase 7–9 types; `ImportPreviewTests`, `OPMLFlowTests` and `DatabaseLocationTests` ran green there (`BundledFontTests` needs the app bundle). `submit: recorder.submit` (a `@MainActor` method passed as `@MainActor (RunRequest) async -> Bool`) compiles without a `Sendable` diagnostic.
- **`ATSApplicationFontsPath` was checked in a hand-made bundle** on this Mac: with `Fonts` and the two variable fonts inside, `NSFont(name: "Newsreader", size: 12)` resolves to `Newsreader16pt-Regular`, and `"Atkinson Hyperlegible Next"` to `AtkinsonHyperlegibleNext-Regular`. The `.txt` licences in the same folder are ignored by the font loader. These registrations do not reach `WKWebView`, hence Phase 10's `data:` URLs.
- **`ressac --json import --dry-run`** on `tests/fixtures/vienna-like.opml` with a missing database prints `{"dry_run":true,"feeds_added":11,"folders_added":7,"merged":[["Daring Fireball (1)","Daring Fireball"]],"skipped_existing":0}` and creates nothing (it uses an in-memory store), checked with the current binary, `HOME` and `--db` in a temporary directory.
- **`OPMLFlow.confirm()` chains on `RunCoordinator.submit`**, which Phase 8 keeps for exactly this: it returns after the run, `true` only on exit `0`. `RunCoordinator` already calls `AppDatabase.notifyExternalChanges()` after each run, so the sidebar observation that hides the first-run screen needs nothing more.
- **`ATSApplicationFontsPath` needs a real Info.plist key.** Phase 6 generates Info.plist from `INFOPLIST_KEY_*` build settings, and Xcode silently drops keys it does not know: `INFOPLIST_KEY_ATSApplicationFontsPath` never reaches the product (checked on a scratch project with Xcode 27). XcodeGen's `info:` key writes a base plist that Xcode merges with the `INFOPLIST_KEY_*` values; pointing it at `build/Info.plist` keeps it in Phase 6's ignored `apps/macos/build/`, out of the source tree (checked: the built plist has both `ATSApplicationFontsPath` and `CFBundleDisplayName`).
- **"Relaunch Now"** starts a new instance with `createsNewApplicationInstance` and terminates this one; both briefly hold the database, which WAL and the busy timeout tolerate.
- **`pendingPanel` is answered with `onChange(of:initial:)`** and reset to `nil` first, so pressing `⌘O` twice in a row opens the chooser twice.
