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

// MARK: Addresses

@Suite struct MosaicRequestTests {
    @Test func theOriginalSurvivesTheRoundTrip() throws {
        let original = try #require(URL(string: "https://img.example.org/a b/photo.jpg?w=800&h=600#x"))
        let address = MosaicRequest.url(for: original, columns: 40)
        #expect(address.hasPrefix("ressac-mosaic:image?cols=40&src=https%3A%2F%2Fimg.example.org"))
        let request = try #require(MosaicRequest(URL(string: address)))
        #expect(request.original == original)
        #expect(request.columns == 40)
    }

    @Test func otherSchemesAndBadColumnsAreRefused() {
        for source in ["file:///etc/passwd", "javascript:alert(1)", "data:image/png;base64,AAAA", "ftp://example.org/a.png", "https:///nohost"] {
            let address = MosaicRequest.url(for: URL(string: source)!, columns: 40)
            #expect(MosaicRequest(URL(string: address)) == nil, "\(source)")
        }
        let good = "https%3A%2F%2Fexample.org%2Fa.png"
        #expect(MosaicRequest(URL(string: "ressac-mosaic:image?cols=0&src=\(good)")) == nil)
        #expect(MosaicRequest(URL(string: "ressac-mosaic:image?cols=41&src=\(good)")) == nil)
        #expect(MosaicRequest(URL(string: "ressac-mosaic:image?src=\(good)")) == nil)
        #expect(MosaicRequest(URL(string: "https://example.org/a.png")) == nil)
        #expect(MosaicRequest(nil) == nil)
    }
}

// MARK: Fetching (Review Focus #4)

/// Serves canned answers to `MosaicFetcher` in process: a status, headers and body chunks.
final class StubProtocol: URLProtocol, @unchecked Sendable {
    struct Answer: Sendable {
        var status = 200
        var headers: [String: String] = [:]
        var chunks: [Data] = []
        /// Seconds before the response, to keep a load in flight.
        var delay: TimeInterval = 0
    }

    static let answers = Mutex<[String: Answer]>([:])
    private let stopped = Mutex(false)

    static func serve(_ path: String, _ answer: Answer) {
        answers.withLock { $0[path] = answer }
    }

    static func configuration() -> URLSessionConfiguration {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [StubProtocol.self]
        return configuration
    }

    override class func canInit(with request: URLRequest) -> Bool { request.url?.host() == "stub.ressac.test" }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        let url = request.url!
        guard let answer = Self.answers.withLock({ $0[url.path()] }) else {
            client?.urlProtocol(self, didFailWithError: URLError(.cannotFindHost))
            return
        }
        Thread.detachNewThread { [self] in
            Thread.sleep(forTimeInterval: answer.delay)
            let response = HTTPURLResponse(url: url, statusCode: answer.status, httpVersion: "HTTP/1.1", headerFields: answer.headers)!
            client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            for chunk in answer.chunks {
                if stopped.withLock({ $0 }) { return }
                client?.urlProtocol(self, didLoad: chunk)
            }
            if !stopped.withLock({ $0 }) { client?.urlProtocolDidFinishLoading(self) }
        }
    }

    override func stopLoading() {
        stopped.withLock { $0 = true }
    }
}

private func stub(_ path: String) -> MosaicRequest {
    MosaicRequest(URL(string: MosaicRequest.url(for: URL(string: "https://stub.ressac.test\(path)")!, columns: 40)))!
}

@Suite(.serialized) struct MosaicFetchTests {
    let fetcher = MosaicFetcher(configuration: StubProtocol.configuration())

    @Test func anImageBecomesAMosaicPNG() async throws {
        StubProtocol.serve("/red.png", .init(headers: ["Content-Type": "image/png"],
                                             chunks: [encoded([flat(255, 0, 0, width: 120, height: 200)], as: .png)]))
        let png = try await MosaicSchemeHandler.mosaic(for: stub("/red.png"), fetcher: fetcher).get()
        let decoded = try #require(decodePNG(png))
        #expect(decoded.width == 10 * 6)  // 120 pixels: 10 columns of 6 font pixels
        #expect(decoded.height == 10 * 10)  // 0.6 × 10 × 200 / 120 = 10 rows of 10 font pixels
        #expect(decoded.rgb(5, 5) == (255, 0, 0))
    }

    @Test func aGIFUsesItsFirstFrame() async throws {
        let gif = encoded([flat(255, 0, 0, width: 24, height: 40), flat(0, 0, 255, width: 24, height: 40)], as: .gif)
        StubProtocol.serve("/anim.gif", .init(chunks: [gif]))
        let png = try await MosaicSchemeHandler.mosaic(for: stub("/anim.gif"), fetcher: fetcher).get()
        #expect(try #require(decodePNG(png)).rgb(0, 0) == (255, 0, 0))
    }

    @Test func htmlEmptyAndSVGBodiesAreUnreadable() async {
        StubProtocol.serve("/page.html", .init(headers: ["Content-Type": "text/html"], chunks: [Data("<html><body>Not an image</body></html>".utf8)]))
        StubProtocol.serve("/empty.png", .init(chunks: []))
        StubProtocol.serve("/logo.svg", .init(chunks: [Data(#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"/>"#.utf8)]))
        for path in ["/page.html", "/empty.png", "/logo.svg"] {
            #expect(await MosaicSchemeHandler.mosaic(for: stub(path), fetcher: fetcher) == .failure(.unreadable), "\(path)")
        }
    }

    @Test func anHTTPErrorIsAFailure() async {
        StubProtocol.serve("/gone.png", .init(status: 404, chunks: [Data("Not found".utf8)]))
        #expect(await MosaicSchemeHandler.mosaic(for: stub("/gone.png"), fetcher: fetcher) == .failure(.status(404)))
    }

    @Test func aBodyOverTenMegabytesIsCutOffWithoutHanging() async {
        let megabyte = Data(repeating: 0x41, count: 1 << 20)
        // No Content-Length: the cap is enforced while the body streams in.
        StubProtocol.serve("/huge.jpg", .init(chunks: Array(repeating: megabyte, count: 50)))
        let start = ContinuousClock.now
        #expect(await MosaicSchemeHandler.mosaic(for: stub("/huge.jpg"), fetcher: fetcher) == .failure(.tooLarge))
        #expect(ContinuousClock.now - start < .seconds(5))
        // An announced length over the cap is refused before the body.
        StubProtocol.serve("/announced.jpg", .init(headers: ["Content-Length": "\(50 << 20)"], chunks: [megabyte]))
        #expect(await MosaicSchemeHandler.mosaic(for: stub("/announced.jpg"), fetcher: fetcher) == .failure(.tooLarge))
    }

    /// 190 KB of PNG can announce 8000 × 8000 pixels, which ImageIO would decode in full: the
    /// size is checked from the header first. A small real PNG under a small limit stands for it.
    @Test func tooManyPixelsAreRefusedBeforeDecoding() {
        let png = encoded([flat(255, 255, 255, width: 100, height: 100)], as: .png)
        guard case .failure(let failure) = MosaicDecoder.thumbnail(of: png, maxColumns: 40, maxPixels: 9_999) else {
            Issue.record("decoded an image over the pixel limit")
            return
        }
        #expect(failure == .tooLarge)
        guard case .success(let thumbnail) = MosaicDecoder.thumbnail(of: png, maxColumns: 40, maxPixels: 10_000) else {
            Issue.record("refused an image at the pixel limit")
            return
        }
        #expect(thumbnail.columns == 9)
    }

    @Test func aServerThatNeverAnswersTimesOut() async throws {
        let server = try HangingServer()
        let quick = MosaicFetcher(timeout: 0.5)
        let request = try #require(MosaicRequest(URL(string: MosaicRequest.url(
            for: URL(string: "http://127.0.0.1:\(server.port)/slow.png")!, columns: 40))))
        let start = ContinuousClock.now
        #expect(await MosaicSchemeHandler.mosaic(for: request, fetcher: quick) == .failure(.network))
        #expect(ContinuousClock.now - start < .seconds(3))
    }
}

// MARK: Handler

/// Records what the handler sends to one WebKit task.
final class FakeSchemeTask: NSObject, WKURLSchemeTask {
    let request: URLRequest
    private(set) var events: [String] = []
    private(set) var body = Data()
    private(set) var response: HTTPURLResponse?

    init(_ url: String) {
        request = URLRequest(url: URL(string: url)!)
    }

    func didReceive(_ response: URLResponse) {
        self.response = response as? HTTPURLResponse
        events.append("response")
    }

    func didReceive(_ data: Data) {
        body.append(data)
        events.append("data")
    }

    func didFinish() { events.append("finish") }
    func didFailWithError(_ error: any Error) { events.append("fail") }
}

@MainActor @Suite(.serialized) struct MosaicSchemeHandlerTests {
    let webView = WKWebView()

    func handler() -> MosaicSchemeHandler {
        MosaicSchemeHandler(fetcher: MosaicFetcher(configuration: StubProtocol.configuration()), cache: NSCache())
    }

    @Test func aMosaicIsAnsweredAsAPNG() async throws {
        StubProtocol.serve("/blue.png", .init(chunks: [encoded([flat(0, 0, 255, width: 60, height: 60)], as: .png)]))
        let handler = handler()
        let task = FakeSchemeTask(MosaicRequest.url(for: URL(string: "https://stub.ressac.test/blue.png")!, columns: 40))
        handler.webView(webView, start: task)
        #expect(await eventually { task.events == ["response", "data", "finish"] })
        #expect(task.response?.statusCode == 200)
        #expect(task.response?.value(forHTTPHeaderField: "Content-Type") == "image/png")
        #expect(try #require(decodePNG(task.body)).width == 5 * 6)
        #expect(handler.runningCount == 0)
    }

    /// A cached mosaic is answered at once, without a download. (A real `NSCache` may evict at
    /// any time, even right after `setObject`, under the memory pressure of the full suite: the
    /// test hands the handler a cache that keeps what it is given.)
    @Test func aCachedMosaicIsAnsweredAtOnce() throws {
        let cache = KeepingCache()
        let handler = MosaicSchemeHandler(fetcher: MosaicFetcher(configuration: StubProtocol.configuration()), cache: cache)
        let address = MosaicRequest.url(for: URL(string: "https://stub.ressac.test/never-fetched.png")!, columns: 40)
        let request = try #require(MosaicRequest(URL(string: address)))
        cache.setObject(MosaicSchemeHandler.CachedMosaic(.success(Data("cached".utf8))), forKey: request.cacheKey)
        let task = FakeSchemeTask(address)
        handler.webView(webView, start: task)
        #expect(task.events == ["response", "data", "finish"])
        #expect(task.body == Data("cached".utf8))
        #expect(handler.runningCount == 0)
    }

    @Test func aFailureAnswersTheBlackPlaceholder() async {
        StubProtocol.serve("/not-an-image.html", .init(chunks: [Data("<html></html>".utf8)]))
        let handler = handler()
        let task = FakeSchemeTask(MosaicRequest.url(for: URL(string: "https://stub.ressac.test/not-an-image.html")!, columns: 40))
        handler.webView(webView, start: task)
        #expect(await eventually { task.events == ["response", "data", "finish"] })
        #expect(task.body == MosaicSchemeHandler.failurePNG)
        let refused = FakeSchemeTask("ressac-mosaic:image?cols=40&src=file%3A%2F%2F%2Fetc%2Fpasswd")
        handler.webView(webView, start: refused)
        #expect(refused.events == ["response", "data", "finish"])
        #expect(refused.body == MosaicSchemeHandler.failurePNG)
    }

    /// WebKit throws if a stopped task is called back: nothing may reach it after `stop`.
    @Test func aStoppedTaskIsNeverCalledBack() async throws {
        StubProtocol.serve("/slow.png", .init(chunks: [encoded([flat(0, 255, 0)], as: .png)], delay: 0.3))
        let handler = handler()
        let task = FakeSchemeTask(MosaicRequest.url(for: URL(string: "https://stub.ressac.test/slow.png")!, columns: 40))
        handler.webView(webView, start: task)
        #expect(handler.runningCount == 1)
        handler.webView(webView, stop: task)
        #expect(handler.runningCount == 0)
        try await Task.sleep(for: .milliseconds(800))
        #expect(task.events.isEmpty)
    }
}
