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
