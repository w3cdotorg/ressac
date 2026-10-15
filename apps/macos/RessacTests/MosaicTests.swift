import CoreGraphics
import Foundation
import ImageIO
import Synchronization
import Testing
import UniformTypeIdentifiers
import WebKit

@testable import Ressac

// MARK: Fixtures

/// An sRGB image filled pixel by pixel.
private func image(width: Int, height: Int, alpha: UInt8 = 255, _ color: (Int, Int) -> (UInt8, UInt8, UInt8)) -> CGImage {
    var bytes = [UInt8](repeating: 0, count: width * height * 4)
    for y in 0..<height {
        for x in 0..<width {
            let c = color(x, y)
            let i = (y * width + x) * 4
            bytes[i] = UInt8(Int(c.0) * Int(alpha) / 255)
            bytes[i + 1] = UInt8(Int(c.1) * Int(alpha) / 255)
            bytes[i + 2] = UInt8(Int(c.2) * Int(alpha) / 255)
            bytes[i + 3] = alpha
        }
    }
    let provider = CGDataProvider(data: Data(bytes) as CFData)!
    return CGImage(width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: width * 4,
                   space: CGColorSpace(name: CGColorSpace.sRGB)!,
                   bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                   provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)!
}

private func flat(_ r: UInt8, _ g: UInt8, _ b: UInt8, width: Int = 60, height: Int = 100) -> CGImage {
    image(width: width, height: height) { _, _ in (r, g, b) }
}

/// `images` encoded as one file of `type` (several frames for a GIF).
private func encoded(_ images: [CGImage], as type: UTType) -> Data {
    let data = NSMutableData()
    let destination = CGImageDestinationCreateWithData(data, type.identifier as CFString, images.count, nil)!
    for image in images { CGImageDestinationAddImage(destination, image, nil) }
    #expect(CGImageDestinationFinalize(destination))
    return data as Data
}

/// Decodes a PNG to (width, height, RGB at a pixel).
private func decodePNG(_ data: Data) -> (width: Int, height: Int, rgb: (Int, Int) -> (UInt8, UInt8, UInt8))? {
    guard let source = CGImageSourceCreateWithData(data as CFData, nil),
          let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else { return nil }
    let width = image.width, height = image.height
    var bytes = [UInt8](repeating: 0, count: width * height * 4)
    bytes.withUnsafeMutableBytes { buffer in
        let context = CGContext(data: buffer.baseAddress, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
                                space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
    }
    let pixels = bytes
    return (width, height, { x, y in
        let i = (y * width + x) * 4
        return (pixels[i], pixels[i + 1], pixels[i + 2])
    })
}

private func uniform(_ color: TeletextColor) -> MosaicCell {
    MosaicCell(foreground: color, background: color, mask: 0)
}

// MARK: Renderer

@Suite struct MosaicRendererTests {
    @Test func aFlatColourGivesUniformCellsInCanonicalForm() {
        let mosaic = MosaicRenderer.mosaic(from: flat(255, 0, 0), maxColumns: 10)
        #expect(mosaic.columns == 10)
        #expect(mosaic.rows == 10)  // 0.6 × 10 × 100 / 60
        #expect(mosaic.cells == Array(repeating: uniform(.red), count: 100))
    }

    @Test func aHalfBlackHalfWhiteCellTakesBlackAsBackground() {
        // One cell, 6 × 10 pixels: the left sub-pixel column white, the right one black. A 3–3 tie
        // puts the colour listed first (black) in the background.
        let leftWhite = image(width: 6, height: 10) { x, _ in x < 3 ? (255, 255, 255) : (0, 0, 0) }
        let mosaic = MosaicRenderer.mosaic(from: leftWhite, maxColumns: 1)
        #expect(mosaic.columns == 1 && mosaic.rows == 1)
        #expect(mosaic.cells == [MosaicCell(foreground: .white, background: .black, mask: 0b01_0101)])
        // The top row white (2 of 6): bits 0 and 1.
        let topWhite = image(width: 6, height: 10) { _, y in y < 3 ? (255, 255, 255) : (0, 0, 0) }
        #expect(MosaicRenderer.mosaic(from: topWhite, maxColumns: 1).cells
            == [MosaicCell(foreground: .white, background: .black, mask: 0b00_0011)])
        // Four white of six: white is the background, the two black sub-pixels the foreground.
        let mostlyWhite = image(width: 6, height: 10) { _, y in y < 7 ? (255, 255, 255) : (0, 0, 0) }
        #expect(MosaicRenderer.mosaic(from: mostlyWhite, maxColumns: 1).cells
            == [MosaicCell(foreground: .black, background: .white, mask: 0b11_0000)])
    }

    @Test func coloursQuantiseToTheNearestTeletextColour() {
        #expect(MosaicRenderer.mosaic(from: flat(30, 220, 235), maxColumns: 4).cells.allSatisfy { $0 == uniform(.cyan) })
        #expect(MosaicRenderer.mosaic(from: flat(200, 30, 40), maxColumns: 4).cells.allSatisfy { $0 == uniform(.red) })
        #expect(MosaicRenderer.mosaic(from: flat(20, 20, 20), maxColumns: 4).cells.allSatisfy { $0 == uniform(.black) })
        let samples = Array(repeating: SIMD3<Double>(250, 240, 20), count: 6)
        #expect(MosaicRenderer.cell(for: samples) == uniform(.yellow))
    }

    @Test func transparencyReadsAsTheBlackPaper() {
        let clear = image(width: 12, height: 20, alpha: 0) { _, _ in (255, 255, 255) }
        #expect(MosaicRenderer.mosaic(from: clear, maxColumns: 2).cells.allSatisfy { $0 == uniform(.black) })
    }

    @Test func widthAndHeightStayWithinTheLimits() {
        let wide = MosaicRenderer.mosaic(from: flat(0, 0, 255, width: 1000, height: 500), maxColumns: 40)
        #expect(wide.columns == 40)
        #expect(wide.rows == 12)  // 0.6 × 40 × 500 / 1000
        #expect(MosaicRenderer.columns(forPixelWidth: 100) == 9)
        #expect(MosaicRenderer.columns(forPixelWidth: 5000) == 40)
        #expect(MosaicRenderer.columns(forPixelWidth: 1) == 1)
        #expect(MosaicRenderer.columns(forPixelWidth: 480, maxColumns: 20) == 20)
        // A tall strip keeps its shape under the row limit.
        let strip = MosaicRenderer.grid(width: 100, height: 10_000, columns: 40)
        #expect(strip.rows <= MosaicRenderer.maxRows)
        #expect(strip.columns == 1)
    }

    @Test func thePNGHasOnePixelPerFontPixelWithSextantRowsOfThreeFourThree() throws {
        // Two cells: the left one has only its middle-left sub-pixel (bit 2) in green on blue,
        // the right one is uniform magenta.
        let mosaic = MosaicImage(columns: 2, rows: 1, cells: [
            MosaicCell(foreground: .green, background: .blue, mask: 0b00_0100), uniform(.magenta),
        ])
        let png = try #require(decodePNG(MosaicRenderer.png(mosaic)))
        #expect(png.width == 12 && png.height == 10)
        #expect(png.rgb(1, 2) == (0, 0, 255))  // top row: background
        #expect(png.rgb(1, 3) == (0, 255, 0))  // middle row starts at font pixel 3
        #expect(png.rgb(2, 6) == (0, 255, 0))  // and is 4 pixels tall
        #expect(png.rgb(1, 7) == (0, 0, 255))  // bottom row
        #expect(png.rgb(3, 4) == (0, 0, 255))  // right column of the cell
        #expect(png.rgb(6, 0) == (255, 0, 255))
        let doubled = try #require(decodePNG(MosaicRenderer.png(mosaic, scale: 2)))
        #expect(doubled.width == 24 && doubled.height == 20)
        #expect(MosaicRenderer.png(MosaicImage(columns: 2, rows: 1, cells: [])).isEmpty)
    }
}
