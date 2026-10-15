import SwiftUI
import Testing

@testable import Ressac

@Suite struct CRTGeometryTests {
    @Test func theScreenIsOnlyForAntiopeWithTheBoxChecked() {
        #expect(CRTGeometry.isActive(appearance: .antiope, crt: true))
        #expect(!CRTGeometry.isActive(appearance: .antiope, crt: false))
        #expect(!CRTGeometry.isActive(appearance: .native, crt: true))
        #expect(!CRTGeometry.isActive(appearance: .native, crt: false))
    }

    /// Spec §9: Increase contrast turns the bleed off and keeps the scanlines and vignette.
    @Test func increaseContrastTurnsTheBleedOff() {
        #expect(CRTGeometry.bleedIsActive(true, contrast: .standard))
        #expect(!CRTGeometry.bleedIsActive(true, contrast: .increased))
        #expect(!CRTGeometry.bleedIsActive(false, contrast: .standard))
        #expect(!CRTGeometry.bleedIsActive(false, contrast: .increased))
    }

    /// Spec §11: the scanline bands land on font-pixel rows for each text size. Each band ends on
    /// its row's bottom edge, covers the lower part of the row rounded to whole Retina pixels,
    /// and every edge sits on a device pixel.
    @Test func bandsLandOnFontPixelRowsAtEveryTextSize() {
        let expectedBand: [Double: Double] = [15: 0.5, 20: 1.0, 25: 1.0, 30: 1.0]
        #expect(TeletextFont.sizeSteps == [15, 20, 25, 30])
        for size in TeletextFont.sizeSteps {
            let pixel = TeletextFont.pixel(forSize: size)
            let bands = CRTGeometry.scanlineBands(height: 600, pixel: pixel)
            #expect(bands.count == Int(600 / pixel), "\(size) pt")
            for (row, band) in bands.enumerated() {
                #expect(band.upperBound == Double(row + 1) * pixel, "\(size) pt, row \(row)")
                #expect(band.upperBound - band.lowerBound == expectedBand[size], "\(size) pt, row \(row)")
                #expect((band.lowerBound * 2).rounded() == band.lowerBound * 2, "\(size) pt, row \(row): crisp at 2x")
                #expect(band.upperBound - band.lowerBound <= pixel / 2, "\(size) pt: a band never covers more than half a row")
            }
        }
    }

    @Test func theLastBandStopsAtTheWindowEdge() {
        let bands = CRTGeometry.scanlineBands(height: 101, pixel: 2)
        #expect(bands.count == 50)
        #expect(bands.last == 99...100)
        let taller = CRTGeometry.scanlineBands(height: 101.5, pixel: 2)
        #expect(taller.count == 51)
        #expect(taller.last == 101...101.5)
        #expect(CRTGeometry.scanlineBands(height: 0, pixel: 2).isEmpty)
        #expect(CRTGeometry.scanlineBands(height: 100, pixel: 0).isEmpty)
        #expect(CRTGeometry.scanlineBands(height: 10, pixel: 2, scale: 1).first == 1...2, "one whole pixel at 1x")
    }

    @Test func tubeCornersAreThreePercentOfTheShorterSide() {
        #expect(CRTGeometry.cornerRadius(for: CGSize(width: 900, height: 500)) == 15)
        #expect(CRTGeometry.cornerRadius(for: CGSize(width: 1440, height: 900)) == 27)
        #expect(CRTGeometry.cornerRadius(for: .zero) == 0)
    }

    @Test func theValuesAreLeLionsAndTheSpecs() {
        #expect(CRTGeometry.scanlineOpacity == 0.28)
        #expect(CRTGeometry.scanlineFraction == 0.4)
        #expect(CRTGeometry.vignetteOpacity == 0.35)
        #expect(CRTGeometry.cornerPercent == 3)
        #expect(CRTGeometry.bleedOffset == 1)
        #expect(CRTGeometry.bleedOpacity == 0.45)
        #expect(CRTGeometry.glowRadius == 2)
    }
}
