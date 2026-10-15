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

@Suite struct TeletextColorTests {
    @Test func theEightColoursArePure() {
        #expect(TeletextColor.allCases == [.black, .red, .green, .yellow, .blue, .magenta, .cyan, .white])
        #expect(TeletextColor.allCases.map(\.css) == [
            "#000000", "#FF0000", "#00FF00", "#FFFF00", "#0000FF", "#FF00FF", "#00FFFF", "#FFFFFF",
        ])
        for colour in TeletextColor.allCases {
            let (r, g, b) = colour.rgb
            #expect([r, g, b].allSatisfy { $0 == 0 || $0 == 0xFF }, "\(colour)")
        }
        #expect(TeletextColor(rawValue: "magenta") == .magenta)
    }

    @Test func swiftUIAndAppKitColoursMatchTheTriplet() throws {
        for colour in TeletextColor.allCases {
            let (r, g, b) = colour.rgb
            let expected = [Double(r), Double(g), Double(b)].map { $0 / 255 }
            let ns = try #require(colour.nsColor.usingColorSpace(.sRGB))
            #expect([ns.redComponent, ns.greenComponent, ns.blueComponent].map(Double.init) == expected, "\(colour)")
            #expect(ns.alphaComponent == 1)
            let resolved = colour.color.resolve(in: EnvironmentValues())
            #expect([resolved.red, resolved.green, resolved.blue].map(Double.init) == expected, "\(colour)")
            #expect(resolved.opacity == 1)
        }
    }
}
