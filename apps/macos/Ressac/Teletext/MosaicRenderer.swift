import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

/// One teletext mosaic cell: 2 × 3 sub-pixels, each in the foreground or the background colour.
struct MosaicCell: Equatable, Sendable {
    var foreground: TeletextColor
    var background: TeletextColor
    /// 6 bits, bit 0 = top-left, row-major (1 top-right, 2 middle-left, 3 middle-right,
    /// 4 bottom-left, 5 bottom-right). A set bit is a foreground sub-pixel.
    var mask: UInt8
}

/// A picture as teletext mosaics: `columns × rows` cells, row-major.
struct MosaicImage: Equatable, Sendable {
    var columns: Int
    var rows: Int
    var cells: [MosaicCell]
}

/// Turns an image into two-colour block mosaics (spec §7). Pure: no I/O, no global state.
///
/// Geometry follows Bedstead and the SAA5050 it copies: a cell is 6 × 10 font pixels
/// (0.6 em × 1 em), split into sub-pixel columns of 3 + 3 and rows of 3 + 4 + 3 font pixels.
/// Each cell takes the pair of teletext colours that best fits its six sub-pixels, by least
/// squared RGB error, and each sub-pixel takes the nearer of the two.
///
/// Canonical form, so equal pictures give equal cells: a cell whose six sub-pixels share one
/// colour has `foreground == background` and `mask == 0`; otherwise the background is the
/// colour of more sub-pixels, and on a 3–3 tie the one listed first in `TeletextColor`.
enum MosaicRenderer {
    static let cellWidth = 6
    static let cellHeight = 10
    /// Sub-pixel boundaries inside a cell, in font pixels.
    static let columnEdges = [0, 3, 6]
    static let rowEdges = [0, 3, 7, 10]
    /// Widest mosaic, in cells (spec §7).
    static let maxColumns = 40
    /// Tallest mosaic, in cells: a strip-shaped image must not make the page endless.
    static let maxRows = 100
    /// Image pixels per cell when sizing a picture: one cell at the default 20 pt is 12 pt wide,
    /// so a 120-pixel picture gets 10 columns and a 1-pixel tracker one.
    static let pixelsPerColumn = 12.0

    /// How many columns an image `width` pixels wide gets, at most `maxColumns`.
    static func columns(forPixelWidth width: Int, maxColumns: Int = maxColumns) -> Int {
        let wanted = Int((Double(max(width, 1)) / pixelsPerColumn).rounded(.up))
        return min(max(wanted, 1), max(maxColumns, 1))
    }

    /// The cell grid for a `width × height` image `columns` wide. Cells are 0.6 em by 1 em, so
    /// `rows = 0.6 × columns × height / width`; past `maxRows` the columns shrink to keep the shape.
    static func grid(width: Int, height: Int, columns: Int) -> (columns: Int, rows: Int) {
        let aspect = Double(max(height, 1)) / Double(max(width, 1))
        func rows(for columns: Int) -> Int { max(1, Int((0.6 * Double(columns) * aspect).rounded())) }
        var columns = max(columns, 1)
        if rows(for: columns) > maxRows {
            columns = max(1, Int(Double(maxRows) / (0.6 * aspect)))
        }
        return (columns, min(rows(for: columns), maxRows))
    }

    static func mosaic(from image: CGImage, maxColumns: Int) -> MosaicImage {
        let (columns, rows) = grid(width: image.width, height: image.height, columns: maxColumns)
        let width = columns * cellWidth
        let pixels = fontPixels(of: image, width: width, height: rows * cellHeight)
        var cells: [MosaicCell] = []
        cells.reserveCapacity(columns * rows)
        for row in 0..<rows {
            for column in 0..<columns {
                let samples = (0..<6).map { bit in
                    let x = column * cellWidth, y = row * cellHeight
                    return average(pixels, stride: width,
                                   x: (x + columnEdges[bit % 2])..<(x + columnEdges[bit % 2 + 1]),
                                   y: (y + rowEdges[bit / 2])..<(y + rowEdges[bit / 2 + 1]))
                }
                cells.append(cell(for: samples))
            }
        }
        return MosaicImage(columns: columns, rows: rows, cells: cells)
    }

    /// The best cell for six sub-pixel colours (0...255 per channel), in canonical form.
    static func cell(for samples: [SIMD3<Double>]) -> MosaicCell {
        let distances = samples.map { sample in
            palette.map { colour in ((sample - colour.rgb) * (sample - colour.rgb)).sum() }
        }
        // a == b is the uniform cell; listed first, it wins any tie with a pair that adds nothing.
        var best = (a: 0, b: 0, error: Double.infinity)
        for a in palette.indices {
            for b in a..<palette.count {
                let error = distances.reduce(0) { $0 + min($1[a], $1[b]) }
                if error < best.error { best = (a, b, error) }
            }
        }
        var maskOfB: UInt8 = 0
        for (bit, distance) in distances.enumerated() where distance[best.b] < distance[best.a] {
            maskOfB |= 1 << UInt8(bit)
        }
        let a = palette[best.a].color, b = palette[best.b].color
        switch maskOfB.nonzeroBitCount {
        case 0: return MosaicCell(foreground: a, background: a, mask: 0)
        case 6: return MosaicCell(foreground: b, background: b, mask: 0)
        case 4, 5: return MosaicCell(foreground: a, background: b, mask: ~maskOfB & 0b11_1111)
        default: return MosaicCell(foreground: b, background: a, mask: maskOfB)  // b is the minority, or a 3–3 tie: a, listed first, is the background
        }
    }

    /// A PNG with one pixel per font pixel × `scale`: a cell is `6·scale × 10·scale` pixels.
    /// The page shows it with `image-rendering: pixelated` and zooms it by the font pixel.
    static func png(_ mosaic: MosaicImage, scale: Int = 1) -> Data {
        let scale = max(scale, 1)
        guard mosaic.columns > 0, mosaic.rows > 0, mosaic.cells.count == mosaic.columns * mosaic.rows else { return Data() }
        let width = mosaic.columns * cellWidth * scale, height = mosaic.rows * cellHeight * scale
        var bytes = [UInt8](repeating: 255, count: width * height * 4)
        for y in 0..<height {
            let fontY = y / scale
            let row = fontY / cellHeight
            let subRow = rowEdges.lastIndex { $0 <= fontY % cellHeight } ?? 0
            for x in 0..<width {
                let fontX = x / scale
                let cell = mosaic.cells[row * mosaic.columns + fontX / cellWidth]
                let bit = UInt8(subRow * 2 + (fontX % cellWidth < columnEdges[1] ? 0 : 1))
                let rgb = (cell.mask & (1 << bit) != 0 ? cell.foreground : cell.background).rgb
                let i = (y * width + x) * 4
                bytes[i] = rgb.r
                bytes[i + 1] = rgb.g
                bytes[i + 2] = rgb.b
            }
        }
        guard let provider = CGDataProvider(data: Data(bytes) as CFData),
              let image = CGImage(width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: width * 4,
                                  space: sRGB, bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue),
                                  provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)
        else { return Data() }
        let data = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(data, UTType.png.identifier as CFString, 1, nil) else { return Data() }
        CGImageDestinationAddImage(destination, image, nil)
        return CGImageDestinationFinalize(destination) ? data as Data : Data()
    }

    // MARK: Private

    private static let sRGB = CGColorSpace(name: CGColorSpace.sRGB) ?? CGColorSpaceCreateDeviceRGB()

    private static let palette: [(color: TeletextColor, rgb: SIMD3<Double>)] = TeletextColor.allCases.map {
        ($0, SIMD3(Double($0.rgb.r), Double($0.rgb.g), Double($0.rgb.b)))
    }

    /// The image drawn over black (the page's paper, so transparency reads as black) at
    /// `width × height`, as RGBA bytes, top row first.
    private static func fontPixels(of image: CGImage, width: Int, height: Int) -> [UInt8] {
        var bytes = [UInt8](repeating: 0, count: width * height * 4)
        bytes.withUnsafeMutableBytes { buffer in
            guard let context = CGContext(data: buffer.baseAddress, width: width, height: height, bitsPerComponent: 8,
                                          bytesPerRow: width * 4, space: sRGB,
                                          bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
            else { return }
            let frame = CGRect(x: 0, y: 0, width: width, height: height)
            context.setFillColor(CGColor(srgbRed: 0, green: 0, blue: 0, alpha: 1))
            context.fill(frame)
            context.interpolationQuality = .high
            context.draw(image, in: frame)
        }
        return bytes
    }

    private static func average(_ pixels: [UInt8], stride: Int, x: Range<Int>, y: Range<Int>) -> SIMD3<Double> {
        var sum = SIMD3<Double>(0, 0, 0)
        for row in y {
            for column in x {
                let i = (row * stride + column) * 4
                sum += SIMD3(Double(pixels[i]), Double(pixels[i + 1]), Double(pixels[i + 2]))
            }
        }
        return sum / Double(x.count * y.count)
    }
}
