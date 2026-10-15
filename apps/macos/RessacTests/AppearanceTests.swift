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
