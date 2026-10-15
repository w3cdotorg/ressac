import AppKit
import Foundation
import SwiftUI
import Testing

@testable import Ressac

/// Bedstead's design: one advance of 600/1000 em, ascent 800, descent 200, no line gap.
@Suite struct BedsteadBundleTests {
    @Test func bedsteadShipsWithItsDedication() throws {
        let font = try #require(NSFont(name: "Bedstead", size: 20), "ATSApplicationFontsPath did not register Bedstead")
        #expect(font.familyName == "Bedstead")
        #expect(font.isFixedPitch)
        #expect(Bundle.main.url(forResource: "bedstead", withExtension: "otf", subdirectory: "Fonts") != nil)
        #expect(Bundle.main.url(forResource: "Bedstead-CC0", withExtension: "txt", subdirectory: "Fonts") != nil)
    }

    @Test(arguments: [15.0, 20.0, 25.0, 30.0])
    func everyGlyphTakesSixTenthsOfAnEm(size: Double) throws {
        let font = try #require(NSFont(name: "Bedstead", size: size))
        let row = "MMMMMiiiii«é€▐🬀"  // 15 characters, mosaics included
        // As Double: inside #expect, comparing a CGFloat with this Double failed (see Notes).
        let width = Double((row as NSString).size(withAttributes: [.font: font]).width)
        #expect(width == size * 6 / 10 * 15)
        #expect(Double(NSLayoutManager().defaultLineHeight(for: font)) == size)
        #expect(abs(font.ascender - size * 0.8) < 0.01)
        #expect(abs(font.descender + size * 0.2) < 0.01)
        #expect(font.leading == 0)
    }
}
