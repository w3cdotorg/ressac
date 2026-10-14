# Phase 4: The Appearance setting and the live root switch

**Runs after:** phase 3.

Read the plan index first. Global Constraints apply. Spec §1 (Success), §2 (Placement), §5 (Switch, Settings), §10 (Bedstead missing) and §11 (Settings: live Native ↔ Antiope switch keeping the selection). Review Focus #5, part 1, is pinned here (part 2, the search prompt, belongs to phase 9).

**Prerequisites:** phase 3 (`TeletextFont`, `TeletextColor`, Bedstead bundled) and phase 6. The switch test launches a copy of the contract fixture through `Launcher`, which checks the schema: with the v3 fixture of phase 1b it only reaches `.ready` once phase 6 has set `knownSchemaVersion = 3` (see phase 3's Execution order).

**Files (5):**
- Create: `apps/macos/Ressac/Settings/Appearance.swift`
- Create: `apps/macos/Ressac/Antiope/AntiopeRootView.swift`
- Modify: `apps/macos/Ressac/App/RessacApp.swift` (`LaunchView`, lines 25–55)
- Modify: `apps/macos/Ressac/Settings/SettingsView.swift` (`ReadingSettings`, lines 81–133)
- Create: `apps/macos/RessacTests/AppearanceTests.swift`

**No `project.yml` change.** `Antiope/` and `Settings/Appearance.swift` sit under the `Ressac` source path; Bedstead is already bundled and registered by phase 3 through the existing `ATSApplicationFontsPath = Fonts`.

**Step 0:** nothing to clean. `RessacApp.swift` has 55 lines and `SettingsView.swift` 232; `AppState.swift` (647 lines) is not touched: both looks share it as it is.

**Test command** (narrow with the suite of the task):

```sh
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)/apps/macos" && xcodegen generate --quiet && \
  xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test \
    -only-testing:RessacTests/AppearanceTests \
  2>&1 | grep -E '^/.+: error:|✘|Test run|\*\* TEST'
```

**Never write the real setting from a test.** The tests run inside `Ressac.app`, whose standard defaults are Clément's. Every test that renders a view gives it a throwaway suite with `.defaultAppStorage(_:)`, which every `@AppStorage` without an explicit store reads, and removes the suite afterwards.

### Task 4.1: The `Appearance` setting

**Interfaces:**
- Consumes: nothing.
- Produces (`Settings/Appearance.swift`), exactly as the plan index:
  - `enum Appearance: String, CaseIterable, Identifiable, Sendable { case native, antiope }`, `var id: Self`, `var displayName: String` (`"Native"`, `"Antiope"`);
  - `enum Keys { static let appearance = "appearance"; static let crt = "antiope.crt" }` (nested; `crt` is used from phase 13);
  - `static func effective(stored: Appearance, fontAvailable: Bool) -> Appearance`: Antiope only when stored so **and** Bedstead is available.

- [ ] **Step 1: Write the failing test.** Create `apps/macos/RessacTests/AppearanceTests.swift`:

```swift
import AppKit
import Foundation
import SwiftUI
import Testing

@testable import Ressac

@Suite struct AppearanceTests {
    @Test func storedValuesNamesAndKeys() {
        #expect(Appearance.allCases == [.native, .antiope])
        #expect(Appearance(rawValue: "antiope") == .antiope)
        #expect(Appearance(rawValue: "teletext") == nil)
        #expect(Appearance.allCases.map(\.displayName) == ["Native", "Antiope"])
        #expect(Appearance.Keys.appearance == "appearance")
        #expect(Appearance.Keys.crt == "antiope.crt")
    }

    /// Review Focus #5 (part 1) and spec §10: never Antiope in a fallback font.
    @Test func antiopeFallsBackToNativeWithoutBedstead() {
        #expect(Appearance.effective(stored: .antiope, fontAvailable: true) == .antiope)
        #expect(Appearance.effective(stored: .antiope, fontAvailable: false) == .native)
        #expect(Appearance.effective(stored: .native, fontAvailable: true) == .native)
        #expect(Appearance.effective(stored: .native, fontAvailable: false) == .native)
    }
}
```

- [ ] **Step 2: Run red** (test command above). Expected: `error: cannot find 'Appearance' in scope`, then `** TEST FAILED **`.

- [ ] **Step 3: Implement.** Create `apps/macos/Ressac/Settings/Appearance.swift`:

```swift
/// The look of the reading window (spec §5, Switch): today's native window, or the Antiope
/// teletext set. Stored with `@AppStorage(Keys.appearance)` and applied live.
enum Appearance: String, CaseIterable, Identifiable, Sendable {
    case native, antiope

    var id: Self { self }

    var displayName: String {
        switch self {
        case .native: "Native"
        case .antiope: "Antiope"
        }
    }

    enum Keys {
        static let appearance = "appearance"
        /// The CRT screen checkbox (phase 13).
        static let crt = "antiope.crt"
    }

    /// Antiope only when Bedstead is available: the app never draws Antiope in a fallback font.
    static func effective(stored: Appearance, fontAvailable: Bool) -> Appearance {
        stored == .antiope && fontAvailable ? .antiope : .native
    }
}
```

- [ ] **Step 4: Run green**, same command. Expected: `✔ Test run with 2 tests in 1 suite passed`, `** TEST SUCCEEDED **`.

- [ ] **Step 5: Commit.**

```bash
git add apps/macos/Ressac/Settings/Appearance.swift apps/macos/RessacTests/AppearanceTests.swift
git commit -F - <<'EOF'
macOS app: the Appearance setting, Antiope only with Bedstead

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 4.2: `AntiopeRootView` stub and the live switch in `LaunchView`

**Interfaces:**
- Consumes: `Appearance` (Task 4.1); `TeletextFont.isAvailable`, `TeletextFont.font(size:)`, `TeletextFont.snapped(_:)`, `TeletextColor`, `EnvironmentValues.teletextSize`, `TeletextFont.defaultSize`, `ReaderStyle.Keys.antiopeSize` (phase 3); `Launcher`, `AppState`, `RunCoordinator.isLongRunActive`, `AppState.refreshCurrent()`; test helpers `TestDatabase`, `Stub`, `eventually`.
- Produces:
  - `struct AntiopeRootView: View` with `init(state: AppState)` (`Antiope/AntiopeRootView.swift`): black, one centred line `RESSAC ANTIOPE` in Bedstead at `TeletextFont.snapped` of Antiope's own stored size (`ReaderStyle.Keys.antiopeSize`, default 20; decision of 2026-10-14: the Native `reader.size` is never read for Antiope), and `\.teletextSize` set to that size for its content. Phase 8 replaces the body and keeps the initialiser.
  - `LaunchView` reads `@AppStorage(Appearance.Keys.appearance)` and, in `.ready`, shows `MainView(state:)` or `AntiopeRootView(state:)` for `Appearance.effective(stored:fontAvailable: TeletextFont.isAvailable)`. `.libraryChrome(state)` and `.environment(state)` wrap the switch, so the OPML flow's `@State` (an import sheet in progress) survives a switch.

Why the state survives: `AppState` is owned by `Launcher`, not by either root view, and the selection, the selected entry, the search text and `RunCoordinator` all live in it. The test proves that neither root resets them while appearing or disappearing (`MainView` binds `searchText` to `.searchable` and mirrors the search field's focus).

- [ ] **Step 1: Write the failing test.** Append to `AppearanceTests.swift`. `switchingKeepsTheSelectionTheSearchAndTheRun` pins Review Focus #5, part 1:

```swift
/// Review Focus #5 (part 1): switching looks while a refresh runs and a search is typed.
@MainActor @Suite struct AppearanceSwitchTests {
    /// `fetch` stays active for 3 s, so that both switches happen during the run.
    static let script = """
        case "$4" in
          fetch)
            echo '{"event":"started","total":1}'
            sleep 3
            echo '{"event":"feed_done","feed_id":8,"title":"Daring Fireball","error":null}'
            echo '{"event":"finished"}' ;;
        esac
        """

    static func descendants(of view: NSView) -> [NSView] {
        view.subviews + view.subviews.flatMap(descendants)
    }

    /// `MainView`'s sidebar is a `List`, which AppKit draws with an `NSOutlineView` subclass
    /// (notes.md, keyboard findings); the Antiope root has none.
    static func showsNativeWindow(_ host: NSView) -> Bool {
        host.layoutSubtreeIfNeeded()
        return descendants(of: host).contains { $0 is NSOutlineView }
    }

    @Test func switchingKeepsTheSelectionTheSearchAndTheRun() async throws {
        try #require(TeletextFont.isAvailable, "Bedstead must be registered in the test host (phase 3)")
        // A private defaults suite: the test host is Ressac.app, whose standard defaults are the user's.
        let suite = "ressac.tests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }

        let test = try TestDatabase()
        let id = try test.entry(feed: 8, title: "Albatross season", body: "albatross")
        let launcher = Launcher()
        await launcher.launch(database: test.url, executable: try Stub(Self.script).executable)
        let state = try #require(launcher.state)
        state.autoReadDelay = .seconds(60)
        state.selection = .feed(8)
        #expect(await eventually { state.entries.map(\.entryId) == [id] })
        state.searchText = "albatross"
        #expect(await eventually { state.entries.map(\.entryId) == [id] })
        state.selectedEntryId = id
        let run = state.refreshCurrent()
        #expect(state.runs.isLongRunActive)

        let host = NSHostingView(rootView: LaunchView(launcher: launcher).defaultAppStorage(defaults))
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1200, height: 700),
                              styleMask: [.titled], backing: .buffered, defer: false)
        window.contentView = host
        defer { window.contentView = nil }
        #expect(await eventually { Self.showsNativeWindow(host) })

        for (look, native) in [(Appearance.antiope, false), (.native, true)] {
            defaults.set(look.rawValue, forKey: Appearance.Keys.appearance)
            #expect(await eventually { Self.showsNativeWindow(host) == native }, "\(look)")
            #expect(state.selection == .feed(8), "\(look)")
            #expect(state.selectedEntryId == id, "\(look)")
            #expect(state.searchText == "albatross", "\(look)")
            #expect(state.entries.map(\.entryId) == [id], "\(look)")
            #expect(state.runs.isLongRunActive, "\(look)")
        }

        #expect(await run.value == .succeeded)
        #expect(state.selectedEntryId == id && state.searchText == "albatross")
    }
}
```

- [ ] **Step 2: Run red** with `-only-testing:RessacTests/AppearanceSwitchTests`. Expected: `✘ … Expectation failed: await eventually { Self.showsNativeWindow(host) == native } antiope` (`LaunchView` ignores the setting, so the native window never goes away), then `** TEST FAILED **`. If instead it fails at `try #require(launcher.state)`, phase 6 has not landed.

- [ ] **Step 3: Create the stub.** Create `apps/macos/Ressac/Antiope/AntiopeRootView.swift`:

```swift
import SwiftUI

/// The Antiope window (spec §5). A stub until phase 8 brings the page header, the three
/// columns and the Minitel bar: one centred line on black, in Bedstead at Antiope's own size.
/// It shares `state` with `MainView`, so switching looks loses nothing.
struct AntiopeRootView: View {
    let state: AppState
    @AppStorage(ReaderStyle.Keys.antiopeSize) private var size = TeletextFont.defaultSize

    var body: some View {
        let teletextSize = TeletextFont.snapped(size)
        ZStack {
            TeletextColor.black.color
            Text("RESSAC ANTIOPE")
                .font(TeletextFont.font(size: teletextSize))
                .foregroundStyle(TeletextColor.white.color)
        }
        .ignoresSafeArea()
        .environment(\.teletextSize, teletextSize)
    }
}
```

- [ ] **Step 4: Switch in `LaunchView`.** Re-read `apps/macos/Ressac/App/RessacApp.swift` first (Edit Safety). Replace:

```swift
struct LaunchView: View {
    let launcher: Launcher

```

with:

```swift
struct LaunchView: View {
    let launcher: Launcher
    @AppStorage(Appearance.Keys.appearance) private var appearance = Appearance.native

```

and replace the `.ready` case:

```swift
        case let .ready(state):
            MainView(state: state)
                .libraryChrome(state)
                .environment(state)
```

with:

```swift
        case let .ready(state):
            // Both looks read the same `state`, so a switch keeps the selection, the selected
            // entry, the search text and a running refresh (spec §5, Switch). The library chrome
            // wraps the switch: an OPML import in progress survives it too.
            Group {
                switch Appearance.effective(stored: appearance, fontAvailable: TeletextFont.isAvailable) {
                case .native: MainView(state: state)
                case .antiope: AntiopeRootView(state: state)
                }
            }
            .libraryChrome(state)
            .environment(state)
```

Read the file again and check that the `switch launcher.phase` still has its five cases and that nothing else changed.

- [ ] **Step 5: Run green** with `-only-testing:RessacTests/AppearanceTests -only-testing:RessacTests/AppearanceSwitchTests`. Expected: `✔ Test run with 3 tests in 2 suites passed` (the switch test takes about 4 s: it waits for the 3 s run), `** TEST SUCCEEDED **`.

- [ ] **Step 6: Commit.**

```bash
git add apps/macos/Ressac/Antiope/AntiopeRootView.swift apps/macos/Ressac/App/RessacApp.swift apps/macos/RessacTests/AppearanceTests.swift
git commit -F - <<'EOF'
macOS app: switch the window between Native and an Antiope stub, live

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 4.3: The Appearance picker at the top of Settings

**Interfaces:**
- Consumes: `Appearance` (Task 4.1); `TeletextFont.isAvailable`, `TeletextFont.font(size:)`, `TeletextFont.snapped(_:)`, `TeletextFont.defaultSize`, `ReaderStyle.Keys.antiopeSize` (phase 3).
- Produces (`Settings/SettingsView.swift`, `ReadingSettings`):
  - a segmented **Appearance** picker (Native / Antiope) as the first row of the Reading tab, bound to `@AppStorage(Appearance.Keys.appearance)`. It shows the effective look, so it reads Native when Bedstead failed to load; it is then disabled, with the note "Antiope needs the Bedstead font, which did not load." (spec §10);
  - in Antiope, the font picker is disabled with the note "Bedstead in Antiope", and the preview is drawn in Bedstead at Antiope's own size, `ReaderStyle.Keys.antiopeSize` snapped (spec §5, Settings; decision of 2026-10-14);
  - the row phase 13 puts the CRT checkbox under. The text size slider, the size steps and the 40-column width are phase 10's.

- [ ] **Step 1: Write the failing test.** Append to `AppearanceTests.swift`:

```swift
/// Settings → Reading: the Appearance picker comes first and writes the shared setting.
@MainActor @Suite struct AppearanceSettingsTests {
    /// SwiftUI draws a segmented `Picker` with an `NSSegmentedControl` subclass.
    static func segmentedControl(_ labels: [String], in host: NSView) -> NSSegmentedControl? {
        host.layoutSubtreeIfNeeded()
        return AppearanceSwitchTests.descendants(of: host).compactMap { $0 as? NSSegmentedControl }.first { control in
            (0..<control.segmentCount).map { control.label(forSegment: $0) } == labels
        }
    }

    @Test func thePickerLeadsTheReadingTabAndWritesTheSetting() async throws {
        try #require(TeletextFont.isAvailable, "Bedstead must be registered in the test host (phase 3)")
        let suite = "ressac.tests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let host = NSHostingView(rootView: SettingsView().defaultAppStorage(defaults))
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 640, height: 600),
                              styleMask: [.titled], backing: .buffered, defer: false)
        window.contentView = host
        defer { window.contentView = nil }

        let names = Appearance.allCases.map(\.displayName)
        #expect(await eventually { Self.segmentedControl(names, in: host) != nil })
        let picker = try #require(Self.segmentedControl(names, in: host))
        let width = try #require(Self.segmentedControl(LineWidth.allCases.map(\.displayName), in: host))
        #expect(picker.isEnabled)
        #expect(picker.selectedSegment == 0)
        // Above the line-width control: window coordinates grow upwards.
        #expect(picker.convert(picker.bounds, to: nil).minY > width.convert(width.bounds, to: nil).maxY)

        // The setting drives the picker...
        defaults.set(Appearance.antiope.rawValue, forKey: Appearance.Keys.appearance)
        #expect(await eventually { Self.segmentedControl(names, in: host)?.selectedSegment == 1 })

        // ...and the picker writes the setting (an in-process action, no keystroke).
        let current = try #require(Self.segmentedControl(names, in: host))
        current.selectedSegment = 0
        current.sendAction(current.action, to: current.target)
        #expect(await eventually { defaults.string(forKey: Appearance.Keys.appearance) == Appearance.native.rawValue })
    }
}
```

- [ ] **Step 2: Run red** with `-only-testing:RessacTests/AppearanceSettingsTests`. Expected: `✘ … Expectation failed: await eventually { Self.segmentedControl(names, in: host) != nil }` and the `#require` after it, then `** TEST FAILED **`.

- [ ] **Step 3: Implement.** Re-read `apps/macos/Ressac/Settings/SettingsView.swift` first. In `ReadingSettings`, replace:

```swift
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
```

with:

```swift
private struct ReadingSettings: View {
    @AppStorage(Appearance.Keys.appearance) private var appearance = Appearance.native
    @AppStorage(ReaderStyle.Keys.font) private var font = ReaderFont.sfPro
    @AppStorage(ReaderStyle.Keys.size) private var size = ReaderStyle.defaultSize
    /// Antiope's own text size (decision of 2026-10-14), for the preview in Antiope.
    @AppStorage(ReaderStyle.Keys.antiopeSize) private var antiopeSize = TeletextFont.defaultSize
    @AppStorage(ReaderStyle.Keys.width) private var width = LineWidth.medium

    /// What the window shows: Native whenever Bedstead failed to load.
    private var shown: Appearance {
        Appearance.effective(stored: appearance, fontAvailable: TeletextFont.isAvailable)
    }

    var body: some View {
        Form {
            Picker("Appearance:", selection: Binding(get: { shown }, set: { appearance = $0 })) {
                ForEach(Appearance.allCases) { option in
                    Text(option.displayName).tag(option)
                }
            }
            .pickerStyle(.segmented)
            .disabled(!TeletextFont.isAvailable)
            if !TeletextFont.isAvailable {
                Text("Antiope needs the Bedstead font, which did not load.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }

            Picker("Font:", selection: $font) {
                ForEach(ReaderFont.allCases) { option in
                    Text(option.displayName)
                        .font(option.swiftUIFont(size: NSFont.systemFontSize))
                        .tag(option)
                }
            }
            .disabled(shown == .antiope)
            if shown == .antiope {
                Text("Bedstead in Antiope")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
```

Then, in the Preview row, replace:

```swift
                    .font(font.swiftUIFont(size: ReaderStyle.clampedSize(size)))
```

with:

```swift
                    .font(shown == .antiope
                          ? TeletextFont.font(size: TeletextFont.snapped(antiopeSize))
                          : font.swiftUIFont(size: ReaderStyle.clampedSize(size)))
```

Read the file again: `DataSettings` and `DatabaseLocation` are unchanged.

- [ ] **Step 4: Run green** with `-only-testing:RessacTests/AppearanceSettingsTests`. Expected: `✔ Test run with 1 test in 1 suite passed`, `** TEST SUCCEEDED **`. Then the whole file: `-only-testing:RessacTests/AppearanceTests -only-testing:RessacTests/AppearanceSwitchTests -only-testing:RessacTests/AppearanceSettingsTests`, 4 tests in 3 suites.

- [ ] **Step 5: Commit.**

```bash
git add apps/macos/Ressac/Settings/SettingsView.swift apps/macos/RessacTests/AppearanceTests.swift
git commit -F - <<'EOF'
macOS app: Appearance picker in Settings; Antiope locks the font to Bedstead

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

Expected: Rust checks unchanged; `** TEST SUCCEEDED **` (4 more tests than after phase 3); `no warnings in our sources`.

**Manual check on a throwaway database** (mac-app phase 11's pattern; no keystroke is sent to any app, Clément clicks):

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)"
APP="$(xcodebuild -project apps/macos/Ressac.xcodeproj -scheme Ressac -showBuildSettings 2>/dev/null | awk '/ BUILT_PRODUCTS_DIR /{print $3}')/Ressac.app"
SMOKE="$(mktemp -d)"; mkdir -p "$SMOKE/home"
cp tests/fixtures/contract.sqlite "$SMOKE/ressac.sqlite"
RESSAC_DB="$SMOKE/ressac.sqlite" HOME="$SMOKE/home" "$APP/Contents/MacOS/Ressac" >"$SMOKE/app.log" 2>&1 &
```

Preferences go through `cfprefsd`, which does not follow `HOME`: the Appearance chosen here is written to Clément's real `org.w3cdotorg.ressac` domain. Note the current value first with `defaults read org.w3cdotorg.ressac appearance` ("does not exist" is fine) and restore it at the end. Check:
1. Select a feed and an entry, type a word in the search field, then `⌘,` ▸ Reading ▸ Appearance ▸ Antiope: the window turns black with `RESSAC ANTIOPE` in Bedstead, without a relaunch; the font picker is disabled with "Bedstead in Antiope" under it and the preview is in Bedstead.
2. Back to Native: the same feed, entry and search text are there.
3. The menus still work in Antiope: View ▸ Bigger Text (`⌘+`) still adds 1 pt to the Native size (`reader.size`) and the stub line stays at 20 pt, Antiope's own size (`reader.size.antiope`, decision of 2026-10-14); phase 10a makes `⌘+`, `⌘−` and `⌘0` act on the size of the look shown. The single-letter reading keys do not, by design: the key router is attached to `MainView` and phase 7 gives Antiope its own.

Quit, restore the setting (`defaults delete org.w3cdotorg.ressac appearance` if it was absent), then `rm -rf "$SMOKE"`. Push:

```bash
git -c credential.helper= -c credential.helper='!gh auth git-credential' push
```

### Notes for the executor

- **Verified ahead of time.** The three tasks were built and run in a scratch copy of the repository (Xcode 27, Swift 6.4, warnings as errors) with phases 3 and 6 applied: the 4 tests pass, the switch test in about 4.3 s, and the whole suite stays green with no warning in our sources. Without phase 6, `switchingKeepsTheSelectionTheSearchAndTheRun` stops at `try #require(launcher.state)` (`.tooNew(3)`).
- **How the test tells the roots apart.** SwiftUI's `List` in `MainView`'s sidebar is an `NSOutlineView` subclass (`SwiftUIOutlineListView`, notes.md), present as soon as the hosting view is laid out, even in a window that is never ordered front. The Antiope stub has no AppKit table. `@AppStorage` follows `defaults.set` on the suite passed to `.defaultAppStorage`, so the switch is driven exactly as Settings drives it.
- **What the Settings test can see.** A segmented `Picker` is an `NSSegmentedControl` subclass (`SwiftUISegmentedControl`), so the test reads its labels, its enabled state and its selection, and fires its action in-process. The menu-style font picker is drawn by SwiftUI itself (no `NSPopUpButton` in the hierarchy on macOS 27), so its disabled state and the "Bedstead in Antiope" note are checked by hand (manual check 1).
- **`Appearance.effective` is evaluated in `body`**, so a missing Bedstead logs once (phase 3's `teletext` category, on the first access to `TeletextFont`) and never draws Antiope. Nothing else logs in a view body.
- **The window chrome** (hidden title bar, full-size content, no toolbar) is phase 8's: the stub keeps the native title bar, and `MainView`'s toolbar and search field simply disappear with it.
