import Foundation
import ImageIO
import Synchronization
import WebKit

/// Why an image has no mosaic. Every case shows the magenta IMAGE block (spec §7).
enum MosaicFailure: Error, Equatable, Sendable {
    /// Not a mosaic address, or an original that is not http(s).
    case refused
    case status(Int)
    /// Over `MosaicSchemeHandler.maxBytes`, or too many pixels to decode safely.
    case tooLarge
    /// Not an image ImageIO decodes: HTML, an empty body, SVG…
    case unreadable
    /// Timeout, DNS, connection, TLS.
    case network
}

/// A `ressac-mosaic:` address: `ressac-mosaic:image?cols=40&src=<original, percent-encoded>`.
struct MosaicRequest: Equatable, Sendable {
    var original: URL
    var columns: Int

    /// Everything but unreserved ASCII is percent-encoded, so the original survives intact.
    private static let unreserved = CharacterSet(charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~")

    static func url(for original: URL, columns: Int) -> String {
        let source = original.absoluteString.addingPercentEncoding(withAllowedCharacters: unreserved) ?? ""
        return "\(MosaicSchemeHandler.scheme):image?cols=\(columns)&src=\(source)"
    }

    /// Nil unless `url` is a well-formed mosaic address for an http(s) original.
    init?(_ url: URL?) {
        guard let url, url.scheme?.lowercased() == MosaicSchemeHandler.scheme,
              let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems,
              let columns = items.first(where: { $0.name == "cols" })?.value.flatMap(Int.init),
              (1...MosaicRenderer.maxColumns).contains(columns),
              let original = LinkPolicy.webURL(items.first(where: { $0.name == "src" })?.value)
        else { return nil }
        self.original = original
        self.columns = columns
    }

    var cacheKey: NSString { "\(columns) \(original.absoluteString)" as NSString }
}

/// Downloads originals with the mosaic limits: a byte cap enforced while the body streams in
/// (the transfer is cancelled as soon as it is passed) and one timeout for the whole transfer.
final class MosaicFetcher: Sendable {
    static let shared = MosaicFetcher()

    private let session: URLSession
    private let delegate: Delegate

    init(configuration: URLSessionConfiguration = .ephemeral, maxBytes: Int = MosaicSchemeHandler.maxBytes,
         timeout: TimeInterval = MosaicSchemeHandler.timeout) {
        configuration.timeoutIntervalForRequest = timeout
        configuration.timeoutIntervalForResource = timeout
        configuration.httpCookieAcceptPolicy = .never
        configuration.httpShouldSetCookies = false
        configuration.urlCache = nil
        delegate = Delegate(maxBytes: maxBytes)
        session = URLSession(configuration: configuration, delegate: delegate, delegateQueue: nil)
    }

    deinit {
        session.invalidateAndCancel()
    }

    func fetch(_ url: URL) async throws(MosaicFailure) -> Data {
        let task = session.dataTask(with: URLRequest(url: url))
        let delegate = delegate
        // The slot exists before the task can be cancelled, so an early completion is kept for
        // the continuation instead of being lost.
        delegate.prepare(task)
        let result: Result<Data, MosaicFailure> = await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                delegate.attach(continuation, to: task)
                task.resume()
            }
        } onCancel: {
            task.cancel()
        }
        return try result.get()
    }

    private final class Delegate: NSObject, URLSessionDataDelegate, Sendable {
        private typealias Continuation = CheckedContinuation<Result<Data, MosaicFailure>, Never>

        /// One transfer: its body so far, then its outcome or the caller waiting for it.
        private struct Load {
            var data = Data()
            var continuation: Continuation?
            var result: Result<Data, MosaicFailure>?
        }

        private let maxBytes: Int
        private let loads = Mutex<[Int: Load]>([:])

        init(maxBytes: Int) {
            self.maxBytes = maxBytes
        }

        func prepare(_ task: URLSessionTask) {
            loads.withLock { $0[task.taskIdentifier] = Load() }
        }

        func attach(_ continuation: CheckedContinuation<Result<Data, MosaicFailure>, Never>, to task: URLSessionTask) {
            let early = loads.withLock { loads -> Result<Data, MosaicFailure>? in
                guard var load = loads[task.taskIdentifier] else { return .failure(.network) }
                if let result = load.result {
                    loads[task.taskIdentifier] = nil
                    return result
                }
                load.continuation = continuation
                loads[task.taskIdentifier] = load
                return nil
            }
            if let early { continuation.resume(returning: early) }
        }

        /// The first outcome wins (the cap, the status check or the completion); later ones are ignored.
        private func finish(_ task: URLSessionTask, _ result: Result<Data, MosaicFailure>) {
            let waiting = loads.withLock { loads -> Continuation? in
                guard var load = loads[task.taskIdentifier], load.result == nil else { return nil }
                guard let continuation = load.continuation else {
                    load.result = result
                    loads[task.taskIdentifier] = load
                    return nil
                }
                loads[task.taskIdentifier] = nil
                return continuation
            }
            waiting?.resume(returning: result)
        }

        func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive response: URLResponse,
                        completionHandler: @escaping @Sendable (URLSession.ResponseDisposition) -> Void) {
            let status = (response as? HTTPURLResponse)?.statusCode ?? 200
            if !(200...299).contains(status) {
                finish(dataTask, .failure(.status(status)))
                completionHandler(.cancel)
            } else if response.expectedContentLength > Int64(maxBytes) {
                finish(dataTask, .failure(.tooLarge))
                completionHandler(.cancel)
            } else {
                completionHandler(.allow)
            }
        }

        /// "http/https only" holds across redirects: any other target is not followed, and the 3xx
        /// answer itself ends the transfer as `status(3xx)`.
        func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                        newRequest request: URLRequest, completionHandler: @escaping @Sendable (URLRequest?) -> Void) {
            completionHandler(LinkPolicy.webURL(request.url?.absoluteString) == nil ? nil : request)
        }

        func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) {
            let overflow = loads.withLock { loads -> Bool in
                guard let load = loads[dataTask.taskIdentifier], load.result == nil else { return false }
                guard load.data.count + data.count <= maxBytes else { return true }
                // In place: copying the struct out would copy the whole buffer on every chunk.
                loads[dataTask.taskIdentifier]?.data.append(data)
                return false
            }
            if overflow {
                finish(dataTask, .failure(.tooLarge))
                dataTask.cancel()
            }
        }

        func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: (any Error)?) {
            let data = loads.withLock { $0[task.taskIdentifier]?.data } ?? Data()
            finish(task, error == nil ? .success(data) : .failure(.network))
        }
    }
}

/// Decodes a downloaded original into a small image for `MosaicRenderer` (spec §7).
enum MosaicDecoder {
    /// Images with more pixels than this are refused unread: ImageIO decodes a PNG in full
    /// before it downsamples, and 190 KB of PNG can announce 8000 × 8000 pixels (checked).
    static let maxSourcePixels = 40_000_000

    /// The first frame (a GIF's too), downsampled by ImageIO to about two image pixels per font
    /// pixel of the mosaic, with the columns that the original's width gives (`maxColumns` at most).
    static func thumbnail(of data: Data, maxColumns: Int,
                          maxPixels: Int = maxSourcePixels) -> Result<(image: CGImage, columns: Int), MosaicFailure> {
        guard !data.isEmpty,
              let source = CGImageSourceCreateWithData(data as CFData, [kCGImageSourceShouldCache: false] as CFDictionary),
              CGImageSourceGetType(source) != nil, CGImageSourceGetCount(source) > 0,
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let width = properties[kCGImagePropertyPixelWidth] as? Int, width > 0,
              let height = properties[kCGImagePropertyPixelHeight] as? Int, height > 0
        else { return .failure(.unreadable) }
        let pixels = width.multipliedReportingOverflow(by: height)
        guard !pixels.overflow, pixels.partialValue <= maxPixels else { return .failure(.tooLarge) }
        let columns = MosaicRenderer.columns(forPixelWidth: width, maxColumns: maxColumns)
        let grid = MosaicRenderer.grid(width: width, height: height, columns: columns)
        let longSide = 2 * max(grid.columns * MosaicRenderer.cellWidth, grid.rows * MosaicRenderer.cellHeight)
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceThumbnailMaxPixelSize: longSide,
        ]
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else { return .failure(.unreadable) }
        return .success((image, columns))
    }
}

/// Serves `ressac-mosaic:` images to the reader (spec §7): fetches the original, decodes a
/// thumbnail, renders a mosaic PNG. WebKit calls it on the main thread; the download, decoding
/// and rendering run off it. A failure answers a small black PNG and marks the image's box
/// `data-failed`, so the stylesheet shows the magenta IMAGE block with the alt text: an image
/// request that fails makes WebKit draw its own blue "?" icon and no alt text (checked on
/// macOS 27), so the fallback cannot come from WebKit itself.
@MainActor final class MosaicSchemeHandler: NSObject, WKURLSchemeHandler {
    nonisolated static let scheme = "ressac-mosaic"
    nonisolated static let maxBytes = 10 * 1024 * 1024
    nonisolated static let timeout: TimeInterval = 15

    /// Mosaics survive a page reload and a switch of look; keyed by original and columns.
    private static let sharedCache: NSCache<NSString, CachedMosaic> = {
        let cache = NSCache<NSString, CachedMosaic>()
        cache.totalCostLimit = 16 << 20
        return cache
    }()

    /// What a failure answers: one black cell, hidden by the stylesheet once the box is marked.
    nonisolated static let failurePNG = MosaicRenderer.png(
        MosaicImage(columns: 1, rows: 1, cells: [MosaicCell(foreground: .black, background: .black, mask: 0)]))

    /// An `NSObject`, so that a test can hand the handler an `NSCache` subclass that never evicts.
    final class CachedMosaic: NSObject {
        let result: Result<Data, MosaicFailure>
        init(_ result: Result<Data, MosaicFailure>) { self.result = result }
    }

    private struct Running {
        let token: Int
        let work: Task<Void, Never>
    }

    private let fetcher: MosaicFetcher
    private let cache: NSCache<NSString, CachedMosaic>
    private var running: [ObjectIdentifier: Running] = [:]
    private var nextToken = 0

    init(fetcher: MosaicFetcher = .shared, cache: NSCache<NSString, CachedMosaic>? = nil) {
        self.fetcher = fetcher
        self.cache = cache ?? Self.sharedCache
    }

    /// How many loads are in flight (tests).
    var runningCount: Int { running.count }

    func webView(_ webView: WKWebView, start urlSchemeTask: any WKURLSchemeTask) {
        guard let url = urlSchemeTask.request.url else {
            urlSchemeTask.didFailWithError(URLError(.badURL))
            return
        }
        guard let request = MosaicRequest(url) else {
            return respond(.failure(.refused), url: url, to: urlSchemeTask, in: webView)
        }
        if let cached = cache.object(forKey: request.cacheKey) {
            return respond(cached.result, url: url, to: urlSchemeTask, in: webView)
        }
        let key = ObjectIdentifier(urlSchemeTask)
        nextToken += 1
        let token = nextToken
        let fetcher = fetcher
        let work = Task {
            let result = await Self.mosaic(for: request, fetcher: fetcher)
            // Stopped (or replaced): WebKit throws if a stopped task is called back.
            guard running[key]?.token == token else { return }
            running[key] = nil
            if result != .failure(.network) {
                cache.setObject(CachedMosaic(result), forKey: request.cacheKey, cost: (try? result.get().count) ?? 64)
            }
            respond(result, url: url, to: urlSchemeTask, in: webView)
        }
        running[key] = Running(token: token, work: work)
    }

    func webView(_ webView: WKWebView, stop urlSchemeTask: any WKURLSchemeTask) {
        running.removeValue(forKey: ObjectIdentifier(urlSchemeTask))?.work.cancel()
    }

    /// The mosaic PNG for `request`, computed off the main actor.
    @concurrent nonisolated static func mosaic(for request: MosaicRequest, fetcher: MosaicFetcher) async -> Result<Data, MosaicFailure> {
        let data: Data
        do {
            data = try await fetcher.fetch(request.original)
        } catch {
            return .failure(error)
        }
        return MosaicDecoder.thumbnail(of: data, maxColumns: request.columns).map { thumbnail in
            MosaicRenderer.png(MosaicRenderer.mosaic(from: thumbnail.image, maxColumns: thumbnail.columns))
        }
    }

    private func respond(_ result: Result<Data, MosaicFailure>, url: URL, to task: any WKURLSchemeTask, in webView: WKWebView) {
        let body: Data
        switch result {
        case .success(let png):
            body = png
        case .failure:
            body = Self.failurePNG
            markFailed(url, in: webView)
        }
        let headers = ["Content-Type": "image/png", "Content-Length": String(body.count), "Cache-Control": "no-store"]
        task.didReceive(HTTPURLResponse(url: url, statusCode: 200, httpVersion: "HTTP/1.1", headerFields: headers)
            ?? URLResponse(url: url, mimeType: "image/png", expectedContentLength: body.count, textEncodingName: nil))
        task.didReceive(body)
        task.didFinish()
    }

    /// Flags every box showing `url`, through app JavaScript (the page's own stays off).
    private func markFailed(_ url: URL, in webView: WKWebView) {
        let script = """
        for (const box of document.querySelectorAll(".mosaic")) {
          const image = box.querySelector("img");
          if (box.dataset.mosaic === source || (image && image.src === source)) { box.dataset.failed = ""; }
        }
        """
        Task { _ = try? await webView.callAsyncJavaScript(script, arguments: ["source": url.absoluteString], contentWorld: .page) }
    }
}

/// An `NSCache` that never evicts, for the one test that needs an entry to stay.
final class KeepingCache: NSCache<NSString, MosaicSchemeHandler.CachedMosaic>, @unchecked Sendable {
    private var kept: [NSString: MosaicSchemeHandler.CachedMosaic] = [:]

    override func object(forKey key: NSString) -> MosaicSchemeHandler.CachedMosaic? { kept[key] }
    override func setObject(_ obj: MosaicSchemeHandler.CachedMosaic, forKey key: NSString) { kept[key] = obj }
    override func setObject(_ obj: MosaicSchemeHandler.CachedMosaic, forKey key: NSString, cost g: Int) { kept[key] = obj }
}
